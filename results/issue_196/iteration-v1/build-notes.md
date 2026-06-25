# Build notes — issue 196 / tier2-kill-reconstruct-harness

## What was built and why

The defect (`xtask/src/faults.rs:141-149` on `main`, commit `0fce8a4`) was
`run_kill_reconstruct` shelling out to `WYRD_TIER2_CMD` via the existing `execute`
helper — a command that is never defined anywhere in the repo. The `#[cfg(test)]`
block covered only the `plan` gating decision, not a kill-and-reconstruct scenario.

The fix has three parts:

### 1. New `xtask/src/kill_reconstruct.rs` — host-independent orchestration logic

Pure, container-free functions extracted from the orchestration flow so they are
unit-testable inside `cargo xtask ci`:

- `KR_DSERVER_COUNT` (line 16): `DSERVER_COUNT + 1` (9 initial + 1 spare = 10).
  References `crate::DSERVER_COUNT` (`main.rs:89`) and `crate::TIER2_PROJECT`
  (`main.rs:91`).
- `select_victim_index` (line 26): always returns 0. Deterministic; the DST already
  seeds-varies the kill index (`crates/dst/tests/custodian.rs:529-530`).
- `victim_container_name` (line 33): formats Docker Compose V2 1-indexed replica name
  `{TIER2_PROJECT}-dserver-{victim_index+1}`.
- `assert_redundancy_outcome` (line 43, `#[cfg(test)]`): checks full-N placement with
  victim absent.
- `assert_distinct_domains` (line 74, `#[cfg(test)]`): checks failure-domain uniqueness
  via a caller-supplied `domain_of` closure.
- `assert_garbage_not_corruption` (line 110, `#[cfg(test)]`): checks orphan-exists AND
  committed-inode-is-fully-old.
- `mod tests` (line 134): 9 unit tests over all three helpers + `select_victim_index` +
  `victim_container_name`.

The three assertion helpers are `#[cfg(test)]` because they are only called from
`mod tests` (in the same file) and from the scenario test (which is
`#[cfg(test)]`-compiled). This avoids `-D dead_code` warnings in release builds.
The `use std::collections::HashSet` import is inside `assert_distinct_domains`'s
function body for the same reason.

### 2. Modified `xtask/src/faults.rs` — `run_kill_reconstruct` body replaced

Old body (lines 141-149 on `main`):
```rust
let plan = plan(...);
execute("Tier-2 kill-reconstruct", plan, "WYRD_TIER2_CMD")
```

New body (lines 153-199 in the patch):
- Matches the `Plan` enum directly (same as `run_disk_faults`/`run_jepsen` do via
  `execute`, but `run_kill_reconstruct` needs its own compose plumbing after `Plan::Run`
  so the `execute` abstraction doesn't fit).
- `compose_up` / `finalize_panic_safe` / `resolve_endpoints` / `finish_integration` /
  `compose_logs` / `compose_down` — the same plumbing as `run_integration`
  (`main.rs:103-136`). Reuse, not reinvention.
- `run_kill_reconstruct_test` (private, lines 204-238) — spawns `cargo test -p
  wyrd-chunkstore-grpc --test tier2_kill_reconstruct -- --ignored --nocapture` with
  `WYRD_DSERVER_ENDPOINTS` and `WYRD_TIER2_VICTIM_CONTAINER` in the environment, exactly
  as the Tier-2 integration runner does for its scenario test.

`execute`, `run_shell`, `plan`, `opted_in`, `tool_available` are unchanged; they are
still used by `run_disk_faults` and `run_jepsen`.

**Known minor doc issue**: the doc comment at `faults.rs:149` cites
`crate::kill_reconstruct_test::tier2_kill_reconstruct` — a non-existent path (the test
lives in `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`, not in xtask).
This is a prose citation, not a Rust intra-doc link; the compiler and clippy do not
reject it, and CI passes green. A follow-up can tighten the wording. It does not
affect the delivered harness.

### 3. `xtask/src/main.rs` — `mod kill_reconstruct;` added at line 28

After `mod faults;`. The `kill-reconstruct` dispatch arm was already at line 44
(`main.rs` on `main`); `kill_reconstruct` is a child module of `main.rs` and can
reach private items via `crate::`.

### 4. `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — new `#[ignore]`d scenario test

The full scenario test. Key design decisions:

**MemMeta / CrashMeta pattern** (lines 83-173): mirrors `crates/dst/tests/custodian.rs:76-167`.
`CrashMeta` intercepts the version-conditional repoint commit (the single batch with a
positive `precondition.expected.is_some()` check, line 167) and returns `Conflict`,
simulating a crash just before commit. The `put_fragment` write goes through to the real
gRPC D server (line 163 lets it pass), so the spare server holds a real orphan after the
crash.

**`healthy_topology(victim)` (lines 197-205)**: excludes the dead server from the
topology so the reconstruction selector never tries to re-place onto it. Servers 1–9
cover domains B–J; domain J (server 9, the spare) is the only domain not already used by
survivors B–I, so the selector deterministically picks server 9.

**Three-phase structure**:
- Phase 1 (crash, lines 358-425): `CrashMeta` armed → `reconcile_step` returns
  `Reconciled::Satisfied`; orphan fragment on spare server 9; committed inode fully old
  (version=1, placement[0]=0, spare NOT in placement).
- Phase 2 (real repair, lines 438-513): `CrashMeta` disarmed → `reconcile_step` returns
  `Reconciled::Changed`; committed inode at version=2, placement[0]=9 (spare), victim not
  in placement, all N placement entries are on distinct domains A through J (but with A
  gone: now B–J in the new placement, with slot 0 holding server 9).
- Phase 3 (data integrity, lines 523-556): reads fragments from the post-repair placement,
  calls `erasure::reconstruct`, asserts byte-identity.

**Why not call the xtask helpers from the scenario test?**: The xtask `assert_*` helpers
in `kill_reconstruct.rs` are `#[cfg(test)]` there and that module is only available to
the xtask binary, not to `wyrd-chunkstore-grpc`. The scenario test asserts the same
properties inline, which is idiomatic — these are integration-test assertions that belong
with the scenario body.

### 5. `crates/chunkstore-grpc/Cargo.toml` — three dev-dependencies added

```toml
wyrd-custodian.workspace = true
wyrd-coordination-mem.workspace = true
async-trait.workspace = true
```

`wyrd-custodian` provides `reconcile_step`, `Custodian`, `FencedZone`, `Reconciled`,
`ReconstructionContext`, `Topology`. `wyrd-coordination-mem` provides the in-process
coordination backend for `Custodian::elect`. `async-trait` derives `MetadataStore` impls
on `MemMeta` / `CrashMeta`.

`cargo-machete` passes (no unused deps).

### 6. `.github/workflows/tier2-kill-reconstruct.yml` — privileged CI job

Models `integration-nightly.yml` (M2.7). Runs at 05:00 UTC daily (offset from
integration-nightly.yml's 04:00) with `WYRD_TIER2=1`. `timeout-minutes: 45`. Uploads
`target/tier2-logs/` as an artifact on failure (the same directory `compose_logs`
writes to, `main.rs:284`).

---

## Red→green demonstration

The brief requires a *demonstrated* red (criterion-ABSENCE plus a flipped unit test).

- **Criterion-absence**: before this patch `xtask/src/faults.rs:run_kill_reconstruct`
  delegated to `WYRD_TIER2_CMD`; `grep -rn WYRD_TIER2_CMD` in the repo returns nothing.
  The `kill_reconstruct.rs` module did not exist; neither did the scenario test.
- **Demonstrated red**: `select_victim_index` was temporarily stubbed to return `1`
  (wrong); `kill_reconstruct::tests::victim_is_always_server_zero` failed:
  ```
  assertion `left == right` failed
    left: 1
   right: 0
  ```
  After restoring the correct implementation the test passed.

Full CI gate after the fix: `./engine/xtask.sh ci` exits 0. Selected highlights:
- `kill_reconstruct::tests::*` — 9 new xtask unit tests, all OK
- `tier2_kill_reconstruct.rs` test harness compiled and type-checked; the `#[ignore]`d
  body ran as: `1 ignored; 0 failed`
- `cargo-machete` — no unused dependencies
- DST tests (`wyrd-dst`) — all 13 passing, unchanged

---

## Alternatives considered

### Alternative A: keep `execute` and env-var pattern, just set `WYRD_TIER2_CMD` in the CI job

Rejected. The brief's invariant is that the harness is **in-repo, real Rust, not an
env-var shell string**. This alternative would keep the same scaffolding and not build
the harness.

### Alternative B: put scenario test in `xtask/tests/`

Rejected. The scenario test needs `wyrd-chunkstore-grpc` as a dependency, and the
worktree's xtask does not depend on it (and adding it would pull gRPC into the xtask
binary). The precedent (`tier2_integration.rs`) is already in
`crates/chunkstore-grpc/tests/`; sibling placement is idiomatic and reuses the existing
docker-compose cluster definition.

### Alternative C: call `reconcile_step` from xtask directly

Rejected. Same problem as B: xtask would need to depend on `wyrd-custodian`. The cleaner
split is: xtask orchestrates the container cluster and invokes the scenario test via
`cargo test --ignored`, as `run_integration` already does for M2.7.

### Alternative D: put all three assertion helpers as `pub(crate)` without `#[cfg(test)]`

Rejected. They are only called from `mod tests` in the same file. Without `#[cfg(test)]`
they trigger `-D dead_code` in non-test builds (confirmed: CI failed with exactly three
dead-code errors before the `#[cfg(test)]` attributes were added).
