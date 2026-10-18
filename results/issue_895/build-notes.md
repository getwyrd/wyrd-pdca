# Build notes — issue 895 / multipart-abort-fence (attempt 3)

Target: `getwyrd/wyrd` `main`, built on the bundle's base `07631d3` (`pdca-integrate: issue_894`,
stack base `pdca-integration/main-rc423c4501a6d`), which is `main` plus the sibling Create patch
(`f9514c4`). `patch.diff` is `git diff 07631d3` of the worktree (new files added with
`git add -N` for the diff only, then reset); `git apply --check -R patch.diff` passes on the tree
every result below was taken on. Lines cite the **patched** tree.

## What this round is

The iteration-2 sign-off accepted the production code as is and asked for **one test only**. So
I re-applied `iteration-v2/patch.diff` unchanged (it applied cleanly on `07631d3`) and added one
test. A per-file comparison of the two patches (each file's hunk text with the `index` lines
stripped, hashed) shows every file but one byte-identical to attempt 2:

| File | attempt 2 → attempt 3 |
|---|---|
| `crates/core/src/multipart/abort.rs` | same (file sha256 `e6d0493f…47a7`, before and after every red edit below) |
| `crates/core/src/multipart.rs` | same |
| `crates/custodian/src/restore.rs` | same |
| `crates/dst/tests/multipart_abort.rs` | same |
| `docs/design/architecture/05-building-block-view.md` | same |
| `crates/core/tests/multipart_abort.rs` | **one test added** (+40 lines), nothing else changed |

No change to `abort.rs` behaviour, to `ABORT_ROUNDS` (`abort.rs:58`), or to the immediate retry
with no backoff (`abort.rs:314-351`).

## The carry-forward item: a held doubt must survive a failed read in the Conflict branch

`abort.rs:337` is the `Err` arm of the `Conflict` branch: a store read inside `lost()`
(`abort.rs:370-388`) failed. It ends the Abort with `doubt.end(upload_id, Stop::Store(fault))`,
so a held "may still land" doubt (`Doubt::held`, `abort.rs:399`) is reported as
`AbortError::InDoubt`. No test reached that arm with a doubt held:

- H2 (`crates/core/tests/multipart_abort.rs:967`) has conflicts, but every read succeeds.
- `i_a_held_fence_survives_a_failed_re_read` (`:1090`) has a failed read, but it is the re-read
  after an unknown commit (`abort.rs:346-349`, the arm at `:348`), not one inside `lost()`.

So the adversary's mutant (that arm returns a plain `AbortError::Stopped`, dropping the doubt)
passed all 23 tests.

**New test** `i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict`
(`crates/core/tests/multipart_abort.rs:1105-1143`), in the shape the sign-off suggested:
`store.script(vec![OnCommit::Hold, OnCommit::ConflictUnmoved], None)` and
`store.state().fail_session_reads_after = Some(2)`.

Trace through production:

1. First read (`abort.rs:289`): no commits yet, so it succeeds and finds `Open@0`.
2. Round 1 commit: the wrapper holds the real batch and answers
   `CommitUnknownResult { may_still_commit: true }` (`:549-552` of the test). `observe`
   (`abort.rs:405-412`) sets `held`.
3. Re-read (`abort.rs:346`): one commit made, below the threshold of 2, so it succeeds and finds
   `Open@0` unchanged. `settle` (`abort.rs:422-431`) keeps `held`, since the bytes it requires
   are still there.
4. Round 2 commit: the wrapper answers `Ok(Conflict)` without applying anything (TiKV's lock
   conflict).
5. `lost()` reads the session (`abort.rs:376`): two commits made, so the wrapper fails it with a
   plain (non-transient) fault. That error comes back through `abort.rs:337`.

What the test asserts:

- `in_doubt(outcome, &id, true)` (helper at `:326`): the result is `AbortError::InDoubt`, it
  classifies `ErrorClass::Indeterminate`, its source is the store's `CommitUnknownResult` with
  `may_still_commit: true`, and its text names the upload id.
- `doubt.outcome.may_still_commit` is true (the held outcome is the one kept).
- `doubt.cause` is `Stop::Store` wrapping the wrapper's plain fault (not a `TransientFault`, text
  equal to `FAULT`). This pins that the Abort stopped on the read, not on something else.
- Exactly two commits, both carrying the `Open@0` fence's preconditions. Because round 2 answered
  `Conflict`, the only reads after it are in `lost()`, so this pins that the failing read was
  the one at `abort.rs:376` and the error left through `abort.rs:337`.
- Before release, the session is still `Open@0` with no obligation (`assert_still_open`).
- `release_held()` returns `Committed`, and afterwards `assert_fenced` holds: `Aborting@1` with
  exactly its one `{session, all}` obligation.
- `mpuctl.count()` is unchanged (E).

### Red for the new test

Edit at `abort.rs:337`, exactly the adversary's mutant:

```diff
-                    Err(fault) => return Err(doubt.end(upload_id, Stop::Store(fault))),
+                    Err(fault) => {
+                        return Err(AbortError::Stopped {
+                            upload_id: upload_id.clone(),
+                            cause: Stop::Store(fault),
+                        })
+                    }
```

Result: **23 passed, 1 failed**, and the one failure is the new test:

```
thread 'i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict' panicked at
crates/core/tests/multipart_abort.rs:333:18:
expected InDoubt naming 00000000000000000000000000000001, got Err(Stopped { upload_id:
UploadId("00000000000000000000000000000001"), cause: Store("the test wrapper's store failed") })
```

Every other test stays green under that mutant, which confirms the gap the adversary reported
and that this test is what closes it. `abort.rs` was restored byte-identical (sha256 checked).

## Demonstrated reds (all re-run on this round's test file)

Each is a temporary edit to `abort.rs`, then
`timeout 900 cargo test -p wyrd-core --test multipart_abort`, then the original restored. A
script did the edit → run → restore cycle and asserted the file was byte-identical afterwards
(sha256 `e6d0493f…47a7` before and after). Baseline on the final tree: **24 passed, 0 failed**.

| Red edit | Result |
|---|---|
| **New this round.** `abort.rs:337` returns a plain `Stopped`, dropping the held doubt | **1 failed**: `i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict` alone |
| Brief 1. An `mpuctl` decrement added to the fence batch (`require` the ledger bytes, put `count - 1`) | **(E) fails**: 15 failed, the new test among them |
| Brief 2. Attempt 4's rule: an unchanged session after `Conflict` taken as a taken key without reading it | **H1 and H2 fail**, plus H4 and both last-round D legs (5 failed) |
| Brief 3. A present obligation key named damage without re-reading the session | **H4 fails** alone |
| Brief 4. The held doubt replaced by the latest outcome (`observe` drops `held` on a `may_still_commit: false` outcome) | **(I) fails**: `i_a_held_fence_survives_a_later_out_of_flight_unknown` alone |
| Brief 5. `end` ignores `unsettled` (a failed re-read after `may_still_commit: false` returned as the read's own fault) | **(I2) fails**: `i2_an_out_of_flight_outcome_no_read_settled_ends_indeterminate` alone |
| Attempt 2's. `Doubt::settle` body emptied | 2 failed: `i_a_session_moved_to_a_later_open_epoch_clears_the_held_doubt`, `i2_an_out_of_flight_outcome_a_read_settled_is_not_kept` |
| Attempt 2's. Only the held-doubt clearing in `settle` removed | 1 failed: `i_a_session_moved_to_a_later_open_epoch_clears_the_held_doubt` |
| Attempt 2's. Only `self.unsettled = None` removed | 1 failed: `i2_an_out_of_flight_outcome_a_read_settled_is_not_kept` |

**Whole-production revert:** the test imports `wyrd_core::multipart::abort`, which does not exist
on the base, so the test file does not compile there. C4-verify will report UNVERIFIABLE (exit
77), as the brief's Verification posture says in advance.

## Refuting my own test

- **(a) Genuine red?** Yes. With the adversary's mutant at `abort.rs:337`, the new test fails and
  nothing else does (output above). The brief's five reds each still fail their named leg on
  this round's file, and so do attempt 2's three `settle` reds. A full revert does not compile.
- **(b) Production path?** Yes. The test calls the real `abort_session` in `wyrd-core` over a
  real `RedbMetadataStore::in_memory()`, with the session made by the real `create_session`. The
  `Scripted` wrapper only changes what the store *answers*: it holds the real batch production
  built and applies it to the real store on `release_held()`, which returns the store's real
  `Committed`; the failed read is a store `get` returning an error, the same shape a real
  adapter's read fault has.
- **(c) Fixture includes the fault?** Yes. The fixture has a real held fence that really can
  still land (the release really commits it, asserted), then a real `Conflict` with nothing
  moved, then a real failed read inside the Conflict branch. A doubt wrongly dropped turns
  `InDoubt`/`Indeterminate` into `Stopped`, which is exactly what the test rejects.

## Gates run (on the final tree)

The brief's inherited-deny ruling: `cargo xtask ci` reaches the all-features cargo-deny step
(RUSTSEC-2026-0346/0349), so I ran the steps it names separately. `Cargo.lock` and `deny*.toml`
are untouched.

| Command | Result |
|---|---|
| `./engine/xtask.sh conformance` | pass, exit 0: "5 valid + 6 invalid vectors pass" |
| `./engine/xtask.sh statics` | pass, exit 0: "no DST-reachable shared mutable global state (ADR-0035)" |
| `./engine/xtask.sh dst` | pass, exit 0: `custodian.rs` 33/33 (restore's fence scenarios, J); `multipart_abort.rs` 6/6; no file reported a failure |
| `cargo test -p wyrd-core -p wyrd-custodian` | pass, exit 0: `multipart_abort` 24/24, `multipart_create` 43/43, every other core file green; every custodian file green, including restore's unmodified `restore_open_fence` 11/11, `restore_completing_fence` 7/7, `restore_reconcile` 17/17, `restore_fence_generation` 12/12 (J) |
| `timeout 900 cargo test -p wyrd-core --test multipart_abort` | 24 passed, 0 failed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p wyrd-core --all-targets -- -D warnings` | clean |
| `typos` on every changed file | clean |
| `scripts/mutants-in-diff` (C5) | 22 mutants: 18 caught, 4 unviable, 0 missed |

The target repo has no commit hooks of its own (no `.pre-commit-config.yaml`, no
`core.hooksPath`), so `cargo fmt` and `clippy` are the formatter and linters that matter.

C5 count is the same as attempt 2: the adversary's mutant at `:337` is a hand-written one, not a
mutant `cargo mutants` generates, so the new test does not change C5's numbers. Its red is shown
above instead.

## The failing advisory gate from attempt 2

- **C4 diff coverage ("patch.diff does not apply on origin/main"):** not fixable in Do. The patch
  is stacked on the sibling #894 (`f9514c4`), which is still not on `origin/main` (checked:
  `git merge-base --is-ancestor f9514c4 origin/main` is false at `71e9b9f`). The brief says to
  build on the base this bundle is given. The gate will keep reporting this until #894 merges or
  the gate measures against the stack base.

## Housekeeping

- External dependencies `typos` and `docs-renderer` were both present; no NEEDS-HUMAN dependency.
  The doc is unchanged from attempt 2, so the render check was not re-run.
- Bundle copies of the test: `multipart_abort.rs` (bundle root, as in earlier rounds) and
  `crates/core/tests/multipart_abort.rs`; both equal the worktree file.
  `crates/dst/tests/multipart_abort.rs` in the bundle equals the worktree file.
- Left in the worktree, all git-ignored: `mutants.out/` and `mutants.out.old/` (cargo-mutants
  output). Scratch under `$PDCA_SCRATCH/pdca-builder-895-reds-v3/` holds the `abort.rs` backup,
  the red-edit script and its logs, and the conformance, statics, C5 and DST logs. All for the
  harness to reclaim.
- Nothing pushed; no PR opened.
