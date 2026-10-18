# Build notes — issue 895 / multipart-abort-fence (attempt 2)

Target: `getwyrd/wyrd` `main`, built on the bundle's base `07631d3` (`pdca-integrate: issue_894`,
stack base `pdca-integration/main-rc423c4501a6d`), which is `main` plus the sibling Create patch
(`f9514c4`). `patch.diff` is `git diff 07631d3` of the worktree (new files added with
`git add -N` for the diff only, then reset); `git apply --check -R patch.diff` passes on the tree
every result below was taken on. Lines cite the **patched** tree.

## What this round is

The iteration-1 sign-off accepted the production code as is and asked for tests plus one doc
sentence. So I re-applied `iteration-v1/patch.diff` unchanged and added only what the
carry-forward lists. A per-file comparison of the two patches (each file's hunk text with the
`index` lines stripped, hashed) shows these four files byte-identical to attempt 1:

- `crates/core/src/multipart/abort.rs` (sha256 of the file `e6d0493f…47a7`, before and after
  every red edit below)
- `crates/core/src/multipart.rs`
- `crates/custodian/src/restore.rs`
- `crates/dst/tests/multipart_abort.rs`

Only `crates/core/tests/multipart_abort.rs` and
`docs/design/architecture/05-building-block-view.md` differ. No change to `abort.rs` behaviour,
to `ABORT_ROUNDS` (`abort.rs:58`), or to the immediate retry with no backoff
(`abort.rs:314-351`).

## The three carry-forward items

### 1. The held doubt being cleared is now observable on plain `cargo test`

`Doubt::settle` (`crates/core/src/multipart/abort.rs:422-431`) clears the held doubt when a
successful read finds the session no longer the bytes the held batch requires. Attempt 1's only
test of that path, `i_a_session_moved_under_a_held_fence_ends_without_the_doubt`
(`crates/core/tests/multipart_abort.rs:1133`), moves the session to `Completing`. The Abort then
returns `Ok(Refused(OperationAborted))` from the table (`abort.rs:306-310`) before it ever calls
`Doubt::end`, so whether the doubt was cleared can't be seen. That is why
`replace Doubt::settle with ()` survived C5
(`iteration-v1/gate-logs/C5-mutants.log`: `MISSED abort.rs:423:9`).

New test `i_a_session_moved_to_a_later_open_epoch_clears_the_held_doubt`
(`crates/core/tests/multipart_abort.rs:1166`), using the plan the sign-off gave:
`[HoldThenMove(session_bytes(&open, 2, 1, Open)), Fail { transient: true }]`.

- Round 1: the wrapper holds the batch (which requires `Open@0`), plants `Open@2`, answers
  `CommitUnknownResult { may_still_commit: true }`. `observe` (`abort.rs:405-412`) sets `held`.
- The re-read (`abort.rs:346-349`) finds `Open@2`. Not this Abort's put, so `settle` runs: the
  bytes `held` requires are gone, so `held` is cleared.
- `Open@2` still fences (`abort.rs:306-313`), so round 2 commits a fence at epoch 2. The wrapper
  fails it with a plain `TransientFault`. `end` (`abort.rs:435-446`) finds nothing held or
  unsettled → `Stopped { cause: Store(transient) }`.
- Asserts: `Err(AbortError::Stopped { .. })`, `classify == ErrorClass::Transient`, cause is
  `Stop::Store` wrapping a `TransientFault`, upload id is the session's (helper
  `stopped_by_a_transient_fault`, `:358`). Also: two commits, the second with
  `expected_fence(&id, &Open@2)`'s preconditions; `release_held()` returns `Conflict`; the
  session is still `Open@2`; no obligation; `mpuctl.count()` unchanged (E).

With `settle` a no-op, `held` survives and `end` returns `InDoubt { may_still_commit: true }`,
which classifies `Indeterminate`. The test fails on the `Stopped` match (red run below).

### 2. The out-of-flight half

New test `i2_an_out_of_flight_outcome_a_read_settled_is_not_kept`
(`crates/core/tests/multipart_abort.rs:1233`), plan
`[Unknown { applied: false, may_still_commit: false }, Fail { transient: true }]`.

- Round 1 answers `CommitUnknownResult { may_still_commit: false }` without applying.
  `observe` sets `unsettled`.
- The re-read succeeds and finds `Open@0` unchanged. `settle` clears `unsettled`
  (`abort.rs:423`).
- Round 2 fences the same `Open@0` and fails with a plain transient fault → `Stopped` /
  `Transient`.
- Asserts the same `Stopped` / `Transient` shape, that both commits carry the `Open@0` fence's
  preconditions, that the session is still `Open@0` with no obligation, and that the count is
  unchanged.

With `settle` a no-op, `unsettled` survives and `end` returns `InDoubt { may_still_commit: false }`.

### 3. The doc sentence

`docs/design/architecture/05-building-block-view.md:206`: "two Aborts racing for one session
answer one fenced and one already aborting" now holds only "when the store reports both commits'
outcomes". The added clause says an Abort whose own commit's outcome was unknown can't tell its
fence from a rival's byte-identical one, so it answers fenced too, which does no harm because the
session was fenced once, with one obligation. Why it is byte-identical: both Aborts read the same
`Open@E` bytes, and `open_teardown` is a pure function of those bytes and the id, so both batches
put the same `Aborting@E+1`. `Doubt::landed` (`abort.rs:415-417`) matches on those bytes.

I left the `abort_session` doc (`abort.rs:242-244`) alone. It describes the `Conflict` branch
only, where the claim holds as written, and the sign-off scoped this round to "tests + one doc
sentence".

The file header's (I)/(I2) bullet (`crates/core/tests/multipart_abort.rs:24-26`) now names both
directions (a moved session clears the doubt; a read that follows an out-of-flight outcome
settles it).

## Demonstrated reds

Each is a temporary edit to `abort.rs`, then
`timeout 900 cargo test -p wyrd-core --test multipart_abort`, then the original restored. A
script did the edit → run → restore cycle and asserted the file was byte-identical afterwards
(sha256 `e6d0493f…47a7` before and after). Baseline on the final tree: **23 passed, 0 failed**.

| Red edit | Result |
|---|---|
| **New A.** `Doubt::settle` body emptied (the exact mutant C5 missed) | **2 failed / 21 passed**: exactly the two new tests. Both panic "expected Stopped with nothing in doubt, got Err(InDoubt(… may_still_commit: true …))" / "(… false …)". Every attempt-1 test stays green, which confirms this was a real gap |
| **New B.** Only the held-doubt clearing removed (`unsettled = None` kept) | 1 failed: `i_a_session_moved_to_a_later_open_epoch_clears_the_held_doubt` |
| **New C.** Only `self.unsettled = None` removed | 1 failed: `i2_an_out_of_flight_outcome_a_read_settled_is_not_kept` |
| Brief 1. An `mpuctl` decrement added to the fence batch (`require` the ledger bytes, put `count - 1`) | **(E) fails**: 14 failed ("(E) mpuctl unchanged" in 10; the other 4 fail first on a batch-precondition check: the D one-batch leg, the moved-epoch leg, and both new tests) |
| Brief 2. Attempt 4's rule: an unchanged session after `Conflict` taken as a taken key without reading it | **H1 and H2 fail**, plus H4 and both last-round D legs (5 failed) |
| Brief 3. A present obligation key named damage without re-reading the session | **H4 fails** alone |
| Brief 4. The held doubt replaced by the latest outcome (`observe` drops `held` on a `may_still_commit: false` outcome) | **(I) fails**: `i_a_held_fence_survives_a_later_out_of_flight_unknown` alone |
| Brief 5. `end` ignores `unsettled` (a failed re-read after `may_still_commit: false` returned as the read's own fault) | **(I2) fails**: `i2_an_out_of_flight_outcome_no_read_settled_ends_indeterminate` alone |

**C5 re-run on this patch** (`scripts/mutants-in-diff`, `PDCA_BUNDLE` = this bundle,
`PDCA_WORKTREE` = the worktree, `TMPDIR` in scratch): **22 mutants tested in 2m: 18 caught,
4 unviable, 0 missed**. Attempt 1 had 17 caught and 1 missed.

**Whole-production revert:** the test imports `wyrd_core::multipart::abort`, which does not exist
on the base, so the test file does not compile there. C4-verify will report UNVERIFIABLE (exit
77), as the brief's Verification posture says in advance.

## Refuting my own test

- **(a) Genuine red?** Yes. With `Doubt::settle` emptied (the surviving mutant), the two new
  tests fail and nothing else does. Each half of `settle` removed alone fails its own new test.
  The five reds the brief requires each fail their named leg (table above). A full revert does
  not compile.
- **(b) Production path?** Yes. Both new tests call the real `abort_session` in `wyrd-core` over
  a real `RedbMetadataStore::in_memory()`, with the session made by the real `create_session`.
  The `Scripted` wrapper only changes what the store *answers*: it holds the real batch the
  production code built and applies it to the real store on `release_held()` (which returns the
  store's real `Conflict`), and it plants `Open@2` with a real commit.
- **(c) Fixture includes the fault?** Yes. Test 1 has a real held fence whose required bytes are
  then really overwritten (the release really loses, asserted). Test 2 has a real unknown
  outcome followed by a real successful re-read. Both end on a real transient fault, so a doubt
  wrongly kept would change the class from `Transient` to `Indeterminate`, and that is what each
  test asserts.

## Gates run (on the final tree)

The brief's inherited-deny ruling: `cargo xtask ci` reaches the all-features cargo-deny step
(RUSTSEC-2026-0346/0349), so I ran the steps it names separately. `Cargo.lock` and `deny*.toml`
are untouched.

| Command | Result |
|---|---|
| `./engine/xtask.sh conformance` | pass: "5 valid + 6 invalid vectors pass" |
| `./engine/xtask.sh statics` | pass: "no DST-reachable shared mutable global state (ADR-0035)" |
| `./engine/xtask.sh dst` | pass, exit 0: madsim clippy clean; `custodian.rs` 33/33 (restore's fence scenarios, J); `multipart_abort.rs` 6/6; every other DST file green |
| `cargo test -p wyrd-core -p wyrd-custodian` | pass, exit 0: `multipart_abort` 23/23, `multipart_create` 43/43, every other core file green; every custodian file green, including restore's unmodified `restore_open_fence` 11/11, `restore_completing_fence` 7/7, `restore_reconcile` 17/17, `restore_fence_generation` 12/12 (J) |
| `./engine/xtask.sh ci-dry-run` (repo-hygiene guards) | pass: "every repo-hygiene guard passed" |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p wyrd-core --all-targets -- -D warnings` | clean |
| `typos` on every changed file | clean |
| `python3 docs/publishing/tools/render_site.py --check` | "link audit OK" |
| `scripts/mutants-in-diff` (C5) | 18 caught, 4 unviable, 0 missed |

## The two failing gates from attempt 1

- **C5 (1 missed mutant):** fixed by items 1 and 2. Re-run above: 0 missed.
- **C4 diff coverage ("patch.diff does not apply on origin/main"):** not fixable in Do. The patch
  is stacked on the sibling #894 (`f9514c4`), which is still not on `origin/main` (checked:
  `git merge-base --is-ancestor f9514c4 origin/main` is false at `71e9b9f`). The brief says to
  build on the base this bundle is given. The gate will keep reporting this until #894 merges or
  the gate measures against the stack base.

## Housekeeping

- External dependencies `typos` and `docs-renderer` were both present; no NEEDS-HUMAN dependency.
- Left in the worktree, all git-ignored: `build/` (render check output), `mutants.out/` and
  `mutants.out.old/` (cargo-mutants output; `.old` is an earlier run's output that cargo-mutants
  renamed when mine started). Scratch under `$PDCA_SCRATCH/pdca-builder-895-reds-v2/` holds the `abort.rs` backup, the
  red-edit script and its logs, the C5 log and the DST log. All for the harness to reclaim.
- Nothing pushed; no PR opened.
