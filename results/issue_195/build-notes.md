# Build notes — issue #195 (tier1-disk-fault-harness), iteration 4

## What this change does

Replaces the inert `WYRD_TIER1_DISK_CMD` external-command shell-out in
`xtask/src/faults.rs:114` (`run_disk_faults`) with a real in-repo Tier-1 disk-fault
harness, mirroring the Tier-2 container precedent (`run_integration` +
`tier2_integration.rs`).

### Files changed

| File | Kind | Role |
|---|---|---|
| `xtask/src/faults.rs` | Modified | `run_disk_faults` now calls `run_tier1_scenario()` instead of `execute(plan, "WYRD_TIER1_DISK_CMD")` |
| `xtask/src/lib.rs` | New | Makes xtask a lib+bin crate so integration tests can import `use xtask::disk_faults::*` |
| `xtask/src/disk_faults.rs` | New | Host-independent orchestration logic: `dm_table_linear`, `dm_table_error`, `verdict_scrub_leg`, `verdict_campaign`, `verdict_passes` |
| `xtask/tests/disk_faults_orchestration.rs` | New | 12 non-ignored orchestration tests (the Check-time flippable seam) |
| `crates/custodian/tests/tier1_disk_faults.rs` | New | `#[ignore]`d real-device scenario test — compiles/type-checks at Check, runs in off-Check Tier-1 job |
| `.github/workflows/tier1-disk-faults.yml` | New | Privileged CI workflow (nightly + on-demand, `WYRD_TIER1=1`) |

---

## Root cause of the defect (two sentences)

`run_disk_faults` delegated to `execute(plan, "WYRD_TIER1_DISK_CMD")`, which shell-outs to an
environment-supplied command defined nowhere in the repo. No in-repo Rust code set up a faulted
block device, drove the production custodian path over it, or asserted the redundancy outcome.

---

## Why the C4-verify RED check works

The C4-verify script (`engine/scripts/run-verify.sh`) keeps added `*/tests/*.rs` files and
removes all other newly added files in the RED check. The key seam:

- **Kept (test files):** `xtask/tests/disk_faults_orchestration.rs`,
  `crates/custodian/tests/tier1_disk_faults.rs`
- **Removed (non-test new files):** `xtask/src/lib.rs`, `xtask/src/disk_faults.rs`
- **Reverted (modified files):** `xtask/src/faults.rs`

In the RED state, `xtask/tests/disk_faults_orchestration.rs` has `use xtask::disk_faults::*`
which requires `xtask` to have a lib target. With `lib.rs` removed, the lib target vanishes
and the orchestration test fails to compile → `cargo test` exits non-zero → RED ✓.

Verified manually: removing `lib.rs` + `disk_faults.rs`, running `cargo test -p xtask --test
disk_faults_orchestration` gives `error[E0433]: cannot find module or crate 'xtask'`.

The scenario test (`crates/custodian/tests/tier1_disk_faults.rs`) does NOT import xtask (to
avoid a cross-crate dev-dep), so its compilation is unaffected by the RED check. It is
`#[ignore]`d in any case, so it doesn't contribute to RED/GREEN by running. The RED signal
comes entirely from the orchestration test compile failure.

---

## Design decisions

### (1) Why `xtask/src/lib.rs` (lib target) instead of `#[cfg(test)]` in `faults.rs`

**Iteration 3 failure**: The orchestration verdict/plan logic was inside `#[cfg(test)]` in
`disk_faults.rs`. In the RED check, removing `disk_faults.rs` left the `#[ignore]`d scenario
as a standalone function — no longer importing anything from that file. The scenario compiled,
ran (as ignored), "passed" → RED check failed.

**Fix**: Making xtask a lib+bin crate via `lib.rs` means the orchestration tests in
`xtask/tests/` import `use xtask::disk_faults::*`, which only resolves when `lib.rs` exists.
Remove `lib.rs` → compilation fails in the test → RED ✓. The production helpers live in the
lib (not `#[cfg(test)]`), so the seam is real.

**Alternative rejected**: Putting the orchestration helpers inline in the scenario test as a
helper mod (no xtask import). Cost: ~0 diff lines, but breaks the RED mechanism because the
scenario test doesn't import from xtask and removal of disk_faults.rs has no effect on it.
That was exactly the iteration 3 failure.

### (2) Why the scenario test does NOT import `xtask::disk_faults::*`

Adding xtask as a dev-dep of wyrd-custodian would create a bidirectional-ish relationship
(xtask is a build-tool crate; custodian is a library crate). While no circular dependency
exists at the type level, it's architecturally unusual. More importantly: the C4-verify RED
check is already satisfied entirely by the orchestration test (`xtask/tests/`), which does
import xtask and fails to compile without lib.rs.

The scenario test's role is to type-check the production custodian APIs at Check — not to
close the RED seam. The dm table strings (`"0 N linear dev 0"` / `"0 N error"`) are inlined
as private helpers in the scenario test, which is a ~4-line duplication vs the alternative of
adding xtask as a dep.

**Rejected alternative**: add xtask as `[dev-dependencies]` in wyrd-custodian. Cost: 1-line
Cargo.toml change + compile-time coupling of custodian tests to xtask. The inlined helpers
in the scenario test are the simpler, cleaner boundary (the test helpers are just format
strings; no logic to drift).

### (3) Why dm-error for reconstruction and direct byte-flip for scrub

**Scrub leg**: needs `FsChunkStore::get_fragment` to return `IntegrityFault` (corrupt on-disk
bytes). Using dm-error here would make `get_fragment` return EIO instead, which `scrub.rs:108`
propagates (aborts the pass) rather than enqueuing. So byte-flip on disk (through dm-linear =
healthy) → FsChunkStore detects checksum mismatch → IntegrityFault → enqueue. ✓

**Reconstruction leg**: needs `get_fragment` on the faulted device to return block-layer EIO.
After switching to dm-error and dropping caches, reads from the ext4 filesystem go to the
block layer → dm-error returns EIO → propagates as `io::Error` with `raw_os_error() == 5`.
`reconstruction::is_permanent_read_fault` (issue #251) detects EIO in the source chain →
reads around it → rebuilds from k=2 survivors (d0, d2) → re-places on d3.

**Mandatory cache eviction**: Between scrub (which reads fragment 1's corrupt bytes → caches
them) and reconstruction (which must see EIO from dm-error, not the cached corrupt bytes), we
call `echo 3 > /proc/sys/vm/drop_caches`. Before each phase we assert the error type (pre-scrub:
IntegrityFault; pre-reconstruction: EIO with errno=5), so the scenario fails fast if
cache-eviction doesn't take effect.

### (4) Reconstruction topology: server 1 in fleet but not in topology

The reconstruction fleet includes d1 (dm-error, server 1) so `assess` actually calls
`get_fragment` on it and exercises `is_permanent_read_fault`. Without server 1 in the fleet,
assess never calls `get_fragment` on it and the EIO read-around is never exercised.

The reconstruction TOPOLOGY registers only servers 0 (A), 2 (C), 3 (D) — not server 1 (B).
This means `select_distinct_domains_excluding` considers only these domains, and after
excluding survivor domains A and C (servers 0 and 2), picks D (server 3 = d3, the healthy
re-placement target). Without excluding server 1 from the topology, the selector might try to
re-place on B (server 1) — which is still dm-error and would reject `put_fragment` — aborting
the repair rather than committing it.

### (5) Fragment writing: `plan_write` instead of raw `erasure::encode`

The reconstruction tests use `write_new_object_placed` (which needs `PlacementChunkStore`). The
scenario test writes fragments directly using `plan_write` (pure, no store access) then
`put_fragment` on each `FsChunkStore` individually. This avoids implementing `PlacementChunkStore`
for a custom fleet while producing correctly-encoded EC fragments (proper v1 headers with
`ec_k`, `ec_m`, `ec_fragment_index` — all verified by `FsChunkStore::put_fragment`).

---

## Correctness verification performed

1. **Compile check** (`cargo check --tests -p xtask`): passes ✓
2. **Compile check** (`cargo check --tests -p wyrd-custodian`): passes ✓
3. **GREEN** (`./engine/xtask.sh ci`): exits 0, all 12 orchestration tests pass, scenario test
   compiles and shows 1 ignored ✓
4. **RED** (removed `lib.rs` + `disk_faults.rs`): `cargo test -p xtask --test
   disk_faults_orchestration` exits non-zero with `E0433: cannot find module or crate 'xtask'` ✓
5. **Format** (`cargo fmt --all`): applied, gate still green ✓
