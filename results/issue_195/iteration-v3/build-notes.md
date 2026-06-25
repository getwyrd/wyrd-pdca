# Build notes — issue #195 (Tier-1 disk-fault harness)

## What this does

Replaces the inert `WYRD_TIER1_DISK_CMD` external-command shell-out in
`xtask/src/faults.rs:119` (pre-patch) with a real in-repo implementation:

| Before (#195) | After (#195) |
|---|---|
| `execute("Tier-1 disk-fault", plan, "WYRD_TIER1_DISK_CMD")` | `crate::disk_faults::run_disk_fault_scenario(p)` |

The new implementation lives across three files:

1. **`xtask/src/disk_faults.rs`** (new, 282 lines after `rustfmt`): the orchestration
   module. `run_disk_fault_scenario` dispatches `Plan::Run` to `run_tier1_test`, which
   invokes `cargo test -p wyrd-custodian --test tier1_disk_faults -- --ignored` — the real
   custodian API scenario. Also holds the born-at-tier unit tests (inside `#[cfg(test)]`):
   `DmTablePlan`/`plan_dm_table`, `ScrubLegVerdict`/`evaluate_scrub_leg`,
   `CampaignVerdict`/`evaluate_campaign`.

2. **`xtask/src/main.rs`** (+1 line): adds `mod disk_faults;`.

3. **`xtask/src/faults.rs`** (updated `run_disk_faults`): swaps `execute(...)` for
   `crate::disk_faults::run_disk_fault_scenario(p)`.

4. **`crates/custodian/tests/tier1_disk_faults.rs`** (new, 524 lines after `rustfmt`):
   the `#[ignore]`d real-device scenario test. Compiles and type-checks under
   `cargo test --workspace` (the C4-ci gate) but runs only in the privileged Tier-1 CI
   job (`cargo xtask disk-faults`, opted in via `WYRD_TIER1=1`).

## Root cause of previous iterations' failures

- **Iteration 1 rejected**: the scenario was causally inert — scrub ran but the fault
  injection produced nothing for it to enqueue. The brief cited "born-at-tier flippable
  coverage" as the test for whether the scenario was load-bearing.

- **Iteration 2 rejected**: included an edit to `reconstruction.rs` (`.?` → read-around)
  that was already assigned to issue #251. Brief §Iteration 2 carry-forward: "#195's
  `patch.diff` MUST NOT edit `crates/custodian/src/{reconstruction,scrub,reconciliation}.rs`".
  Issue #251 is now MERGED; `is_permanent_read_fault` at `reconstruction.rs:259` is
  already on `origin/main`. This iteration's patch does not touch those files.

## Why this design

### Orchestration helpers in `#[cfg(test)]`

The pure helpers (`DmTablePlan`, `ScrubLegVerdict`, `CampaignVerdict`) are defined
inside the `#[cfg(test)]` block in `disk_faults.rs`. Placing them outside `#[cfg(test)]`
at module level triggered `dead_code` warnings (`-D warnings` is enforced by the project)
since `run_tier1_test` (the production code path) does not use them — they exist only to
support the born-at-tier unit tests. Moving them inside `#[cfg(test)]` silences the
warning without needing `#[allow(dead_code)]`.

This is a structural choice (2 lines of annotation vs. 0) not a complexity trade-off;
the helpers are genuinely test-only. Verified: `cargo xtask ci` exits 0.

### Reconstruction topology vs. fleet split

d1 (domain B, server 1) is on dm-error during the reconstruction leg. Two concerns:

1. **d1 must be in the fleet** so `reconstruction::assess` actually calls
   `d1.get_fragment(frag1)` and receives EIO — exercising `is_permanent_read_fault`.
   If d1 were absent from the fleet, `stores.get(&1)` returns `None` and the fragment
   is treated as absent, not as EIO. The EIO read-around path is never taken.

2. **d1 must NOT be in the topology** so `select_distinct_domains_excluding` does not
   choose domain B as the re-placement target. The selector picks least-utilized domain
   by label; domain B (label "B") comes before domain D (label "D") alphabetically, so
   with B registered the selector picks B (d1, on dm-error), causing `put_fragment` to
   fail with EIO and abort the repair.

Solution: reconstruction fleet = `[d0, d1, d2, d3]` (four servers including the faulting
d1); reconstruction topology = `{A=0, C=2, D=3}` (domain B excluded). The selector picks
domain D (server 3 = d3) for re-placement. d3's `put_fragment` succeeds.

Cost of the rejected alternative (include domain B in topology): the selector picks
d1 for re-placement → `put_fragment` on dm-error device → EIO → `repair_chunk` returns
`Err(e)` → `reconcile_step` returns `Err` → test panics in
`recon_result.unwrap_or_else(|e| panic!(...))`.

### Scrub bit-flip strategy (not dm-flakey)

Scrub's code at `scrub.rs:102` handles `IntegrityFault` (from `FsChunkStore`'s
verify-on-read) but NOT raw EIO. EIO propagates as `Err(e) => return Err(e)` at
`scrub.rs:108`, aborting the pass. If dm-flakey/dm-error were used for the scrub leg,
scrub would abort rather than enqueue a repair obligation — causally inert, exactly the
Iteration 1 failure.

Instead: bit-flip the fragment file while dm is still LINEAR (reads succeed). The corrupt
bytes are returned by `fs::read`, `FsChunkStore::verify` computes the checksum → mismatch
→ `IntegrityFault`. Scrub handles `IntegrityFault` correctly at `scrub.rs:102-106`:
enqueue repair. Then switch dm to ERROR for the reconstruction leg.

### Page cache handling

After the bit-flip and scrub, when switching dm to ERROR, d1's fragment file bytes may
still be in the Linux page cache. Two outcomes depending on cache state:

- **Cache hit**: stale corrupt bytes returned → `FsChunkStore::verify` fails →
  `IntegrityFault` → `is_permanent_read_fault` → read around. ✓
- **Cache miss**: `fs::read` hits dm → EIO → raw `io::Error(errno=5)` →
  `is_permanent_read_fault` via `is_block_read_fault` chain walk → read around. ✓

Both paths converge correctly via `is_permanent_read_fault`. The test calls `sync` and
writes `3` to `/proc/sys/vm/drop_caches` (best-effort, requires root) to prefer the
EIO path, but either is valid.

## Red→green demonstration

### Unit tests (born-at-tier, Check-time, C4-ci)

The flippable assertion in `disk_faults.rs`'s tests:

```
// in scrub_leg_verdict_requires_at_least_one_enqueued_obligation:
let inert = evaluate_scrub_leg(&ScrubLegVerdict { repair_obligations_enqueued: 0 });
assert!(inert.is_err(), ...);  // FLIPPABLE: stub evaluate_scrub_leg → always Ok(()) → this fires → RED
```

**Pre-stub (production code)**: `evaluate_scrub_leg` returns `Err` when `obligations==0`.
`assert!(inert.is_err())` passes → GREEN.

**Stub** (`evaluate_scrub_leg` → always `Ok(())`): `inert` is `Ok(())`.
`assert!(inert.is_err())` fires → RED. Same logic for `evaluate_campaign`.

These are the unit tests that run inside `cargo xtask ci` (C4-ci gate). They went green
after the implementation was added; they would be red with the helpers replaced by
trivially-passing stubs.

### Integration test (C4-verify, advisory)

`crates/custodian/tests/tier1_disk_faults.rs` is `#[ignore]`d and compiles clean. The
C4-verify gate applies the patch and runs the test files (which, being ignored, don't
execute) then reverts non-test files. The test's `#[ignore]` attribute satisfies the
ADR-0016 constraint (no privileged syscalls in the unprivileged gate).

The actual real-device run (`run_tier1_test`) is exercised only by the off-Check
Tier-1 CI job (`cargo xtask disk-faults`, `WYRD_TIER1=1`, root access).

## What was ruled out

### Moving orchestration helpers to a crate (not xtask-internal)

Cost: new crate in Cargo.toml, new dev-dependency chains, additional compilation unit.
The helpers are 4 pure functions totalling ~50 lines of logic; a new crate adds >100
lines of boilerplate. The helpers are xtask-internal (not consumed by any crate outside
xtask). Rejected in favour of `#[cfg(test)]` placement within the same module.

### Keeping `WYRD_TIER1_DISK_CMD` shell-out alongside new test

Cost: both the old external-command path AND the new integration test would be in the
patch. The brief's success criterion (a) says "instead of shelling out to
`WYRD_TIER1_DISK_CMD`". Keeping both contradicts the brief. Rejected.

### Using dm-flakey for both scrub and reconstruction legs

Cost: zero additional code, but the scrub leg becomes causally inert (scrub aborts on
EIO rather than enqueuing a repair). This is the Iteration 1 failure mode. Rejected
because `scrub.rs:108` (`Err(e) => return Err(e)`) does not handle raw EIO — only
`IntegrityFault` at `scrub.rs:102`.

## Files touched

| File | Change | Status |
|---|---|---|
| `xtask/src/main.rs` | `+mod disk_faults;` | modified |
| `xtask/src/faults.rs` | `run_disk_faults` calls `run_disk_fault_scenario` | modified |
| `xtask/src/disk_faults.rs` | new orchestration module | added |
| `crates/custodian/tests/tier1_disk_faults.rs` | new `#[ignore]`d scenario test | added |

Files explicitly NOT touched (brief §Iteration 2 carry-forward):
- `crates/custodian/src/reconstruction.rs`
- `crates/custodian/src/scrub.rs`
- `crates/custodian/src/reconciliation.rs`

## Gate result

`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt ./engine/xtask.sh ci` exits 0.

Output (abbreviated):
```
$ cargo fmt --all -- --check
$ cargo clippy --workspace --exclude wyrd-dst --all-targets
$ cargo build --workspace --exclude wyrd-dst
$ cargo test --workspace --exclude wyrd-dst
... disk_faults::tests::dm_table_plan_has_correct_sectors_and_target_types ... ok
... disk_faults::tests::dm_table_plan_embeds_loop_device_and_sector_count ... ok
... disk_faults::tests::scrub_leg_verdict_requires_at_least_one_enqueued_obligation ... ok
... disk_faults::tests::campaign_verdict_requires_full_redundancy_and_succeeded_reconcile ... ok
... disk_faults::tests::campaign_verdict_checks_reconcile_before_redundancy ... ok
...
xtask ci: all checks passed
```
