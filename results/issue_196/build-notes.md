# Build notes — issue 196 / tier2-kill-reconstruct-harness (Iteration 3)

## What iteration 2 was rejected for

Sign-off rejected on the codex advisory: broken intra-doc links in the new
`xtask/src/kill_reconstruct.rs` module doc. The doc referenced
`[`assert_garbage_not_corruption`]`, `[`assert_redundancy_outcome`]`,
`[`assert_distinct_domains`]` as rustdoc bracket-links, but those functions were
re-homed into the chunkstore-grpc test crate
(`crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`) after iteration 1, so they are
unresolvable from xtask. Root `Cargo.toml:170` sets `rustdoc::broken_intra_doc_links =
"deny"`, so `cargo doc -p xtask` would error. The gate (`cargo xtask ci`) runs no
`cargo doc` step, so the lint was never exercised by C4-ci — it was caught by the advisory
codex reviewer at T5.

## Change made in iteration 3

**Single targeted fix**: in `xtask/src/kill_reconstruct.rs` module doc, lines that
referenced the three helpers as `[`assert_garbage_not_corruption`]`,
`[`assert_redundancy_outcome`]`, `[`assert_distinct_domains`]` were converted to plain
code spans — drop the square brackets:

```
// BEFORE (broken):
//! ([`assert_garbage_not_corruption`], [`assert_redundancy_outcome`], [`assert_distinct_domains`])

// AFTER (correct):
//! (`assert_garbage_not_corruption`, `assert_redundancy_outcome`, `assert_distinct_domains`)
```

No other change from iteration 2. The rest of the patch is identical to v2.

## Cross-crate intra-doc link scan

Before declaring done, I scanned all new doc comments for `[`...`]` bracket-links that
cross crate boundaries:

- `xtask/src/kill_reconstruct.rs`:
  - `[`crate::faults::run_kill_reconstruct`]` — within xtask, valid ✓
  - `[`crate::DSERVER_COUNT`]` — within xtask, valid ✓
  - `[`crate::TIER2_PROJECT`]` — within xtask, valid ✓
  - `assert_garbage_not_corruption`, `assert_redundancy_outcome`, `assert_distinct_domains` — **now plain code spans**, not bracket-links ✓

- `xtask/src/faults.rs` (new doc on `run_kill_reconstruct`):
  - `[`crate::kill_reconstruct::KR_DSERVER_COUNT`]` — within xtask, valid ✓

- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`:
  - All `[`...`]` links refer to items defined in the same file (`KR_DSERVER_COUNT`,
    `assert_garbage_not_corruption`, etc.) — valid ✓
  - Note: `cargo doc` does not process integration test files (tests/ directory), so
    broken_intra_doc_links is never exercised here — moot in any case.

## What was already correct from iteration 2

All the substantive harness logic was correct in v2:

1. `assert_garbage_not_corruption` logic: iteration 1 had it logically inverted — it was
   fixed in iteration 2. The correct logic: PASS when `orphaned_fragment_exists == true`
   AND `committed_placement_has_victim == true` (fully old inode after crash, orphan on spare
   server). This matches the DST property (`crates/dst/tests/custodian.rs:617`) and the
   live scenario test assertion at `tier2_kill_reconstruct.rs:785-786`.

2. Helper architecture: the three assertion helpers (`assert_garbage_not_corruption`,
   `assert_redundancy_outcome`, `assert_distinct_domains`) live in
   `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — they are NOT in xtask.
   This is correct because they are called by the scenario test in that crate.
   The xtask `kill_reconstruct.rs` only holds the host-independent orchestration helpers
   (`select_victim_index`, `victim_container_name`) that are unit-tested inside
   `cargo xtask ci`.

3. The born-at-tier coverage: the unit tests for the three assertion helpers in
   `tier2_kill_reconstruct.rs` are NON-`#[ignore]`d and run inside `cargo xtask ci`'s
   `cargo test --workspace`. If a helper is stubbed, those unit tests fail (demonstrated
   below).

## Five changed files (all in the worktree)

| File | Status | What changed |
|------|--------|-------------|
| `xtask/src/kill_reconstruct.rs` | NEW | Host-independent orchestration helpers; FIXED: plain code spans instead of cross-crate bracket-links in module doc |
| `xtask/src/main.rs:28` | MODIFIED | Added `mod kill_reconstruct;` |
| `xtask/src/faults.rs:137-240` | MODIFIED | Replaced `execute(…, "WYRD_TIER2_CMD")` shell-out with real in-repo harness |
| `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` | NEW | Scenario test + born-at-tier helper unit tests |
| `crates/chunkstore-grpc/Cargo.toml:46-52` | MODIFIED | Added `wyrd-custodian` + `wyrd-coordination-mem` dev-dependencies |
| `.github/workflows/tier2-kill-reconstruct.yml` | NEW | Privileged off-Check CI job |
| `Cargo.lock` | MODIFIED | Auto-updated by cargo |

## Red → green demonstration

### Pre-fix red (criterion absence + demonstrated red)

**Criterion absence**: Before this patch, `xtask/src/faults.rs::run_kill_reconstruct`
shells out to `WYRD_TIER2_CMD` (an env var not defined anywhere in the repo):
```rust
execute("Tier-2 kill-reconstruct", plan, "WYRD_TIER2_CMD")  // faults.rs:148 on main
```
No in-repo harness exists; no unit tests over kill-victim selection or the assertion
helpers exist. `grep -rn "WYRD_TIER2_CMD" ../wyrd` shows the command is never defined.

**Demonstrated red** (born-at-tier seam): I temporarily stubbed
`assert_garbage_not_corruption` to `Ok(())` (always succeed) and ran the test suite.
Result: 2 born-at-tier unit tests fail:
```
FAILED: garbage_not_corruption_fails_when_orphan_is_absent
FAILED: garbage_not_corruption_fails_when_victim_not_in_committed_placement
```
With the real implementation restored: 9 passed, 0 failed, 1 ignored (`cargo xtask ci`).

### Post-fix green

`./engine/xtask.sh ci` (the gating gate) exits 0:
- fmt, clippy, build, test (`tier2_kill_reconstruct.rs` compiles + 9 unit tests pass, 1
  `#[ignore]`d scenario compiles), cargo-machete (no unused deps), cargo-deny, conformance,
  DST — all pass.

The `#[ignore]`d scenario test (`kill_reconstruct_restores_full_redundancy_in_distinct_domains`)
compiles and type-checks, proving it is real API-bound Rust calling the production
`reconcile_step` + `ReconstructionContext` APIs.

## Alternatives considered and rejected

### Alternative: make links resolvable by `pub re-exporting` from xtask

Could re-export the three helpers from `xtask/src/kill_reconstruct.rs` via `pub use`
after making them available as a separate crate. This would require making a new shared
crate, adding it as a dev-dependency for both xtask and chunkstore-grpc, moving the
helpers there, and re-wiring the imports — approximately +1 new crate + 4 files touched.
Cost: ~50 lines of new crate boilerplate. The plain-code-span fix is ~3 characters per
link × 3 links = 9 characters changed. The helpers semantically belong in the scenario
test crate (they are called by and unit-tested near the scenario), not in xtask.
Rejected: disproportionate overhead for what is a documentation cross-reference, not an
API design issue.

### Alternative: move helpers back into xtask `#[cfg(test)]`

This was the original iteration 1 architecture, rejected because orphaned `#[cfg(test)]`
helpers in xtask were unreachable from the scenario test (separate crate). The sign-off
for iteration 2 already accepted the re-home to chunkstore-grpc tests as the correct
architecture. Rejected: re-introduces the orphaned-helper problem.

## Open items (T5 judgment — human sign-off)

These were open in iteration 2 and remain T5/NEEDS-HUMAN. They are not a rejection cause:

1. **T5 fidelity**: `MemMeta`/`CrashMeta` is an in-memory MetadataStore seam vs. the
   proposal 0005 §13.2 "real NVMe/fsync" Tier-2 mandate. The interpretation: the
   _metadata_ path (inode/chunk-map) uses an in-memory backend to avoid a redb/NVMe
   dependency in the test crate, while the _fragment_ path (gRPC put/get) uses real
   containers over a real network — which is what Tier-2 uniquely adds over the Tier-0
   DST. The maintainer must ratify whether this interpretation satisfies §13.2.

2. **Validation**: confirm the privileged `WYRD_TIER2=1` job runs the scenario green on a
   real node (docker available); that `CrashMeta`'s single-positive-precondition model
   matches the production reconstruction commit sequence; and that the gate base contains
   merged #195.
