# Adversarial review: #1029 (657.2c), part commit settles a `CommitUnknownResult`

**Verdict: I could not refute the fix.** I re-ran the test file on the patched tree (69/69 pass, and 69/69 on each of 8 more runs, so no flakiness). Then I made 34 hand mutations of the new logic in `crates/core/src/multipart/commit.rs` in a scratch copy. Every one of them made at least one test fail. What is left is about the evidence and the gates, not the code.

## Attempted refutations that failed

- `crates/core/src/multipart/commit.rs:741`, `:1179`, `:1197`, `:1202`, `:1211`, `:1325`: I changed `doubt.end(upload_id, Stop::Store(fault))` to `stopped(...)` at each site, one at a time. All six are caught, by `c_a_store_fault_at_each_re_read_under_a_held_commit_keeps_the_doubt` (`crates/core/tests/multipart_upload_part_commit.rs:4966`) or `c_a_later_round_failing_definitely_keeps_the_held_rounds_doubt` (`:4906`). The iteration-1 sign-off gap is closed, and the part read at `:1179`, which the sign-off did not list, is pinned too.
- The brief's demonstrated reds, each re-created and each caught:
  - `.find(may_land).or_else(last)` replaced by `.last()` (`commit.rs:819`): caught by the kept-doubt leg.
  - The torn-part check moved ahead of the own-commit proof (`commit.rs:1218-1220`): caught by `r_damage_under_a_held_commit_keeps_the_doubt`.
  - Settling on the slot read made before the last session read (inserted after `commit.rs:1213`): caught by the read-gap leg and H.
  - A non-empty prior treated as settling (`commit.rs:1187` guard dropped): caught by all three changed-prior legs.
  - The rival-over-absent rule never settling (`:1187` made a no-op): caught by the B rival leg, the read-gap leg and C1.
  - A short lease compensating (`commit.rs:1351` sent to `unexplained_stop`): caught by both F legs.
  - Spent rounds compensating while a round is in doubt (`:1345`): caught by 6 tests.
  - Removing the first-read or last-read `session_seen` (`:1198`, `:1213`): caught.
  - `retain` replaced by `clear` or a no-op (`:809`): caught.
  - The reported flag taken from the store instead of the settled value (`:824`): caught.
  - `observe` forced to `true` or to `false`: caught.
  - The `session_gone` doubt arm removed, or its `cannot_land` removed (`:730`): caught.
  - The moved-and-gone arm removed (`:1232`): caught.
  - `doubt.any() ||` dropped from the `Again` arm (`:1254`): caught.
  - The proof's `doubt.any()` guard dropped, or the proof run on the first session read instead of the last: caught.
  - `ReadAgain` treated as an immediate end, or as a re-send (`:1332`): caught.
  - Every damage arm switched from `doubt.end` to `stopped` (part, slot, session, `PartRecordMissing`, rewritten slot): caught.
- The proof "slot gone while a later session read still finds `Open@E` means my own commit landed" depends on nothing else deleting `slot:` under an `Open` session. I grepped: the custodian (`gc.rs`, `rebalance.rs`, `reconstruction/staged.rs`) never writes `slot:` keys, and Abort fences before it deletes. The epoch only rises (`commit.rs:636-640` treats an equal or lower epoch as damage), so "`Same` at the later read" does mean `Open@E` at the slot read. I found no history that breaks it.
- C5's one surviving mutant, `commit.rs:1332:34` (guard `send` replaced with `true`), is **equivalent**, not a test gap. `ReadAgain` needs a held slot with a round still in doubt (`:1238`). The lap that sends nothing is entered only after the last read marked every round `may_land = false` (`:1213`). That lap observes no new round, so its slot read (`:1206` into `:809`) empties the doubt before `:1238` can match. The arm at `:1338` is therefore dead code, and both versions of the guard behave the same.
- The test wrapper's slot guard was loosened to accept "bytes this attempt released" (`multipart_upload_part_commit.rs:597`). That escape applies to every batch kind, compensations included, not only the "round sent again" its doc names. I tested whether this hides a compensation sent after the attempt's own commit landed (mutation: `Judged::Landed` also calls `refuse`). Eight tests catch it through their own compensation counts, so no hole is open today. Optional tightening: limit the escape to `Kind::Commit`.
- Rubric checks: the missing seeded DST coverage is an in-code deferral (`commit.rs:258`, `// deferred: #660`), so it is settled. The living doc is updated in the same change (`docs/design/architecture/05-building-block-view.md:250`). No production caller is added: the patch touches only `commit.rs`, its test file and that doc.

## Claims in `check-gates.json` that I think are not earned

- NEEDS-HUMAN [human] — `T4-batch-review` (a gating row) reports `pass` with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **14.85 s** (`gate-logs/T4-batch-review.log`). It is described as three review passes over a 156 KB patch. That time is too short for a fresh review of this size, and zero findings of every kind (not even dropped noise) fits a cache hit or empty reviewer output better than a real pass. The brief requires "a confirmed high-effort review of the final patch", and this row does not show one. Open `results/issue_1029/review-batch.md` and confirm it reviewed this revision, the one with the new fault-site tests, and not the iteration-v1 bundle.
- NEEDS-HUMAN [human] — `C4-diff-cov` says "patch.diff does not apply on origin/main", so coverage of the changed lines was never measured. One possibility is that #1028 (the slice this one builds on) is not merged yet, which is expected for a stacked slice. The other is that `main` has moved and the bundle is stale. I can't tell from the snapshot (the target has no `origin` refs). Confirm the base before merge. My 34 mutations cover the same new lines, but only for the paths I chose.
- `C4-verify` reports `PASS (green-only)` with the reason "test is co-located with the fix (no separate */tests/*.rs)". That reason is wrong for this patch: the tests live in the separate file `crates/core/tests/multipart_upload_part_commit.rs`. They are appended to an existing file and use new API (`InDoubt::cause`, `Stop::SessionMoved`), so they would fail to compile on the pre-fix code. That is a trivial red, not a meaningful one. The brief declared green-only in advance, so this is not a refutation. The mutation runs above stand in for the red-to-green evidence the gate did not produce.

<!-- pdca:leaf-complete -->
