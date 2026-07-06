# Prove the redb→TiKV metadata swap upholds the single-zone consistency contract

## Summary
**User impact:** M4 replaces Wyrd's metadata store — the component that
records every object's inode and version — with a real distributed TiKV
cluster in place of the deterministic in-memory/redb fake. The deterministic
simulation only proves the *fake* backend commits atomically; nothing proved
the *real* backend keeps the same guarantees. Because the real TiKV commit
awaits network I/O between re-reading a key's precondition and finalizing the
write — a window the single-transaction redb backend never has — a
concurrency regression in the swap (a lost update, a torn multi-key commit)
could reach operators running the distributed backend with no automated
signal that anything broke.

This PR adds the missing test evidence: a live scenario that drives the
production TiKV commit path on a real ≥3-replica Raft group under a fault and
concurrent contention, plus the offline arithmetic and integration legs
around it. No production source or trait bytes change.

## What to look at
- `crates/metadata-tikv/tests/tier1_metadata_consistency.rs` — the live
  consistency scenario: symmetric, bidirectional isolation of the region
  **leader** mid-run, ≥2 concurrent writers racing the same version-cell
  compare-and-swap, and read-after-commit / exactly-once-convergence /
  no-lost-update asserted as independent signals across the heal.
- `crates/testkit/src/lib.rs` — the pure quorum, PD-heartbeat, heal, and
  no-lost-update oracles plus their unit tests (hand-computed expectations,
  not the value the function returns).
- `xtask/src/metadata_faults.rs` — the tier dispatch/runner that builds the
  fault-agent image and stands up the cluster.
- `deploy/tikv-multi-replica/` — the bridge-networked ≥3-replica TiKV/PD
  stack and the netns `iptables-agent` used for the partition.
- `crates/dst/tests/tikv_await_commit_interleaving.rs` — a redb-only
  coverage seed, explicitly labelled as carrying no TiKV correctness weight.

To exercise the offline parts, run `cargo xtask ci`: the pure oracles and
the coverage seed run there and the live legs skip cleanly (no cluster
needed). To exercise the live legs, run the tier runner on a privileged
Docker host (`WYRD_TIER1=1` / `WYRD_TIER2=1`), which brings up the cluster
and runs the ignored, endpoint-gated tests.

## Root cause
The deterministic backend's `commit()` is one synchronous write transaction,
so the DST concurrency harness's "no `await` inside commit" rationale holds
for it — but it is false for `TikvMetadataStore::commit`, which awaits
network I/O between the `get_for_update` precondition re-check and the
terminal `commit()` (`crates/metadata-tikv/src/lib.rs:540-601`). No test
drove the real commit path across that window under contention, so the
re-check that prevents a lost update was never guarded by an executed test.

## Fix
Add a test-and-tooling layer only. The live Tier-1 scenario drives the real
production commit behind the unchanged trait against a real Raft group while
the leader is isolated and concurrent writers contend the same key; the
integration legs cover multi-key atomic create/rename/delete and single-node
real I/O; the pure oracles run in the whole-tree gate. Production code
(`crates/metadata-tikv/src`) and the `MetadataStore` trait
(`crates/traits/src/lib.rs`) are untouched byte-for-byte.

## Verification
- **Claim:** the swap upholds the single-zone contract (read-after-commit,
  exactly-once convergence, no lost update) on a real ≥3-replica cluster,
  and a real partition — not a no-op — gates the verdict.
  **Checked:** `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`
  (leader isolation + concurrent-writer teeth at `:449-476`; independent
  signals + heal at `:500-556`) against a live pingcap/tikv v8.5.1 cluster.
  A real leader cut passes green with the fault materialized; skipping the
  cut (no-op control) fails on `fault_materialized = false`.
- **Claim:** the concurrency guard has teeth — weakening the commit-point
  re-check is caught, not silently tolerated.
  **Test:** with a scratch, never-committed mutation deleting/weakening the
  `get_for_update` re-check at `crates/metadata-tikv/src/lib.rs:555-573`,
  the contention leg goes red on an observed lost update
  (`no_lost_update = false`); reverting restores green.
- **Claim:** the offline arithmetic is non-tautological and the invariants
  hold.
  **Checked:** `crates/testkit/src/lib.rs` oracle unit tests use
  hand-computed expectations and negating any single consistency signal
  fails `consistency_passes`; `git diff crates/metadata-tikv/src
  crates/traits` is empty. `cargo xtask ci` is green with no TiKV present.

Fixes #257
