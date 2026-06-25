# Build notes — issue 196 / tier2-kill-reconstruct-harness (iteration 2)

## What the carry-forward required

Iteration 1 was rejected on T4 for three defects. This iteration addresses all three:

### 1. `assert_garbage_not_corruption` was logically inverted (carry-forward item 1)

**Root cause (two sentences):** After a crash before the version-conditional commit, the
custodian DOES NOT advance the inode — the commit never landed — so the victim IS still in
the committed placement (the inode is *fully old*). The v1 helper treated
`committed_placement_has_victim == true` as a violation, which is exactly backwards: it
should PASS when the victim is still in the committed placement (fully old inode), and FAIL
when the victim is ABSENT (which would indicate the commit partially landed — a torn/hybrid
chunk).

**Evidence the inversion was wrong:** `crates/dst/tests/custodian.rs:616-619` asserts
`crashed.chunk_map[0].placement == vec![0, 1, 2]` after a crash — victim 0 IS still in
the placement. The v1 scenario test at `tier2_kill_reconstruct.rs:414-416` asserted the
same (`placement[VICTIM_INDEX] == VICTIM_INDEX as DServerId`). The v1 helper's unit test
called `assert_garbage_not_corruption(true, false)` and expected `Ok(())` — but `(true,
false)` means "orphan exists, victim NOT in committed placement", which is the torn/hybrid
case that should FAIL, not the fully-old case that should pass. Both the helper and its
test encoded the same inversion, making the green vacuous.

**Fix:** Invert the committed-placement check in `assert_garbage_not_corruption`. The
function now returns `Err` when `!committed_placement_has_victim` (victim absent from
committed placement after crash = hybrid = bad). It passes on `(true, true)` = orphan
exists + victim still in committed placement = fully old = good.

**Updated unit test:** `garbage_not_corruption_passes_when_orphan_exists_and_victim_still_in_committed_placement`
now calls `assert_garbage_not_corruption(true, true)` → `Ok(())`. The negative test
`garbage_not_corruption_fails_when_victim_not_in_committed_placement` calls `(true, false)` →
`Err(…"hybrid"…)`. This is the load-bearing test: if the function is stubbed to return `Ok`
always, `garbage_not_corruption_fails_when_victim_not_in_committed_placement` fails — the
seam is genuinely load-bearing.

### 2. Orphaned-helper architecture (carry-forward item 2)

**Root cause:** The v1 helpers (`assert_garbage_not_corruption`, `assert_redundancy_outcome`,
`assert_distinct_domains`) were under `#[cfg(test)]` in `xtask/src/kill_reconstruct.rs` — a
different crate from the scenario test in `crates/chunkstore-grpc/tests/`. They couldn't
be called from the scenario test. Their unit tests were the ONLY callers; removing/stubbing
a helper would break its unit test, but the scenario test was unaffected. "Load-bearing" was
false for this architecture.

**Options considered:**

Option A — Move helpers to the scenario test file, have the scenario test call them, add
non-ignored unit tests in the same file. Cost: the helpers are now in `tier2_kill_reconstruct.rs`
(not `kill_reconstruct.rs`), which means the xtask unit test coverage for them shifts to
the chunkstore-grpc crate. But the scenario test DOES call them, making them genuinely
load-bearing. Non-ignored unit tests run at Check. This is the correct architecture.

Option B — Keep in xtask, make them non-`#[cfg(test)]`, call from `run_kill_reconstruct`.
Cost: `run_kill_reconstruct` delegates to `cargo test` as a subprocess; it doesn't have
access to the inode placement data the helpers check. Wiring them there would require
parsing subprocess output — complex and wrong.

Option C — Drop the three assert_* helpers entirely, rely on scenario test's inline asserts.
Cost: the brief explicitly says these helpers should be unit-tested at Check. A pure
inline-only scenario test (which is `#[ignore]`d) does not provide Check-time coverage of
the assertion logic itself.

**Chosen: Option A.** The three assertion helpers are now in `tier2_kill_reconstruct.rs`
as regular functions (not `#[cfg(test)]`-only). The scenario test calls them. Non-ignored
`#[test]` unit tests in the same file cover the logic. This is load-bearing: stubbing a
helper → unit test fails; removing a helper → scenario test fails to compile.

The `xtask/src/kill_reconstruct.rs` module now contains only the two genuinely-wired
orchestration helpers: `select_victim_index` (called from `faults.rs:run_kill_reconstruct`
at xtask/src/faults.rs:155) and `victim_container_name` (called at faults.rs:156). Their
unit tests remain in that module.

### 3. Broken intra-doc link (carry-forward item 3)

The v1 doc comment on `run_kill_reconstruct` referenced
`crate::kill_reconstruct_test::tier2_kill_reconstruct` — a path that does not exist in the
xtask crate (the scenario test is in a different crate). Fixed by replacing the intra-doc
link with plain prose text referencing the file path. The root `Cargo.toml` has
`broken_intra_doc_links = "deny"` so this would fail `cargo doc` if left in.

## What was built

### Files changed (path:line on main base)

- **`crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`** — NEW. Contains:
  - `AssertResult` type alias (avoids `wyrd_traits::Result` shadowing)
  - `assert_garbage_not_corruption` (correct logic: pass on `(true, true)`, fail on `(true, false)`)
  - `assert_redundancy_outcome` and `assert_distinct_domains` (unchanged in logic from v1,
    relocated from xtask)
  - 9 non-ignored unit tests for the three helpers (run at Check)
  - The `#[ignore]`d scenario test `kill_reconstruct_restores_full_redundancy_in_distinct_domains`
    that calls all three helpers (making them load-bearing)

- **`xtask/src/kill_reconstruct.rs`** — NEW. Contains only:
  - `KR_DSERVER_COUNT` (used in `faults.rs` at xtask/src/faults.rs:153,155,156,162)
  - `select_victim_index` (called from `faults.rs:155`)
  - `victim_container_name` (called from `faults.rs:156`)
  - 2 unit tests for these (genuinely wired into production path, load-bearing)

- **`xtask/src/faults.rs`** — MODIFIED. `run_kill_reconstruct` (xtask/src/faults.rs:141)
  replaces the `WYRD_TIER2_CMD` shell-out with real orchestration (compose_up → resolve →
  kill → run_kill_reconstruct_test). Intra-doc link fixed at the doc comment.

- **`xtask/src/main.rs:27`** — `mod kill_reconstruct;` added between `faults` and `vectors`.

- **`crates/chunkstore-grpc/Cargo.toml`** — `wyrd-custodian.workspace = true` and
  `wyrd-coordination-mem.workspace = true` added to `[dev-dependencies]`.

- **`Cargo.lock`** — updated with `wyrd-coordination-mem` and `wyrd-custodian` in
  `wyrd-chunkstore-grpc`'s dependency list.

- **`.github/workflows/tier2-kill-reconstruct.yml`** — NEW. Privileged off-Check CI job
  modelled on `integration-nightly.yml`, opts in via `WYRD_TIER2=1`, runs daily at 05:00 UTC.

## Demonstrated red→green

**Born-at-tier coverage — demonstrated red:** If `assert_garbage_not_corruption` were
stubbed to return `Ok(())` always, the test
`garbage_not_corruption_fails_when_victim_not_in_committed_placement` would fail:
```
assert!(result.is_err(), ...) → FAILED: returned Ok(())
```
The seam is load-bearing. The C4-verify gate confirms this mechanically: reverting the
production changes (including `Cargo.toml` dev-deps) leaves the scenario test file
compiled against missing crates → compilation error → RED. With all changes applied → GREEN
(9 helper unit tests pass, scenario test correctly ignored).

**C4-verify gate output:**
- GREEN: 9 passed; 0 failed; 1 ignored
- RED: compilation error (unresolved imports `wyrd_coordination_mem`, `wyrd_custodian`)
- Gate result: PASS — red without the fix, green with it.

## What was ruled out

- **Moving assertion helpers to non-cfg(test) in xtask and calling from run_kill_reconstruct:**
  `run_kill_reconstruct` calls `cargo test` as a subprocess and doesn't have the placement
  data needed to run the assertions. Wiring would require subprocess output parsing — ~30
  extra lines of fragile parsing vs. 0 lines by relocating the helpers to the test file.

- **Dropping the assert_* helpers entirely:** The brief explicitly mandates
  "host-independent orchestration logic (kill-victim selection, the redundancy /
  distinct-domain / garbage-not-corruption assertion helpers) unit-tested inside cargo
  xtask ci". Option C violates this mandate.

- **Keeping helpers in xtask as #[cfg(test)]:** This was the v1 architecture that was
  rejected. It's dead duplication — the scenario test can't reach them, so the unit tests
  prove nothing about the scenario.
