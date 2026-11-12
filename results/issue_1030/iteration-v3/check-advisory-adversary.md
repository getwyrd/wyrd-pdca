# Adversarial review — issue 1030 (compensation in doubt, own-commit part read)

The fix holds up. I could not make a compensation answered unknown give a false landed answer,
and the red→green evidence is real. Two gaps are left: a test gap in one lap of the settle, and
one damage-only false `Committed` that the patch does not close and the docs do not name.

## Findings

- NEEDS-HUMAN [impl] — **An empty part's second settle lap is never tested.** `crates/core/src/multipart/commit.rs:1284`:
  narrowing the empty part's proof to the first lap (`if obligation.is_none() && laps == 1`) leaves all 92
  tests green. The second lap is there to catch a compensation that lands after lap 1's slot read, and no
  test lands one there. (S) row 3 lands before lap 1 (`multipart_upload_part_commit.rs:6158`); row 4 lands
  only after the return. Concrete case: an empty part on the rival route, the compensation `Act::hold()`, and
  `land_held(Committed)` at gap 2 of `at_compensation_gaps(&attempt, 0, 2, …)` (after lap 1's session read).
  I ran it in a scratch copy. The patch gives `Refused(OperationAborted)` with 2 laps of reads, which is
  correct. The mutant gives `InDoubt { Compensation, may_still_commit: true, OutcomeUnknown }`. Fix: add
  this run to (S). It costs availability, not C-1 safety, but nothing in the suite would catch the
  regression.

- NEEDS-HUMAN [human] — **One damage-only false `Committed` is left: a rival's record plus an erased slot.**
  `commit.rs:1032` (`Ok(Some(_)) => doubt.cannot_land()`) takes any record that decodes as proof
  (`:151`, "Any record there is a record, this attempt's or a rival's"). Concrete case, run in a scratch
  copy: a first-upload commit is held and never lands. At gap 0 a same-number rival commits and the
  attempt's slot key is erased; the session is untouched. Result: `Ok(Committed { digest: <this attempt's> })`.
  But the record is the rival's (`is_own` is false), the held batch later answers `Conflict`, and
  `releases == 0`. The brief's (K) asks for "any record there is a record", so this is by design. A record
  left by this history reads the same as a landed commit later replaced by a rival re-upload. Still, it is
  the "silent success" the rubric's *Absent or unsupported entries* class names. Neither the module doc's
  limits (`:155-157`, re-upload only; `:240`, empty compensation) nor the brief's *Scope* lists it. Decide
  whether to accept it as a third documented limit (a one-paragraph doc change) or narrow the proof.

- NEEDS-HUMAN [human] — **(K) says only the h4 test's outcomes change. A second one changed too.**
  `multipart_upload_part_commit.rs:5642-5758`: the last run of `r_damage_under_a_held_commit_keeps_the_doubt`
  went from `committed_as` to `InDoubt { Commit, may_still_commit: true }` with a torn-part cause. In that run
  the commit **did** land and its part key was damaged afterwards. This follows from the iteration-2
  sign-off ("undecodable bytes at the part key must not fall through to `Committed`"). The history is
  forced: it gives the same reads as the unlanded D1 torn run. But it breaks the brief's literal (K)
  promise. Sign-off should record it as an accepted deviation, so a reviewer does not wave it through as
  "same behaviour".

- NEEDS-HUMAN [human] — **The T4 batch review looks empty again.** `check-gates.json` row `T4-batch-review` is a
  gating `pass` ("0 blocking, 0 recorded-rejected, 0 noise-dropped") after 26.44 s for a 131 KB patch
  (`gate-logs/T4-batch-review.log`). The `review-batch.md` it names is 563 bytes on disk. I only saw the
  file size, not the contents, since the file is not one of my inputs. The iteration-2 sign-off flagged the
  same thing (17 s, empty file) and asked for "a real high-effort review of the final patch" this round.
  This gate row does not show one happened. Confirm before sign-off.

## What I tried and could not break

- **Red→green, re-run at `$PDCA_TARGET`.** The patched tree is 92/92 green. With the base `commit.rs` and the
  patched test file, 21 tests go red, among them every new leg (S, X3, O×3, T×3, F, D1×2, D2, K×2) and the
  reworked h4. The tests drive the real `commit_part` through the wrapper over redb
  (`multipart_upload_part_commit.rs:1098-1101`), not a copy of it. That fills in C4-verify's pre-declared
  `PASS (green-only)`.
- **Hand-made mutants: 24 of 25 caught.** These include every demonstrated red the brief lists: the
  compensation sent again after an unknown round; slot-gone accepted for a non-empty part (D3); the
  obligation read under the first epoch, not the round's (O rollback); no obligation read after a moved
  session (`:1300-1304`, T second history); a moved session accepted for an empty part (T first history);
  other bytes at the obligation key taken as proof, as absent, or lowering the flag (O); a missing session
  lowering the flag with or without the later slot read (T); a plain stop in place of each kept doubt (F); the
  part-key check removed, or made on the lap's first read (`:1024-1038`, D1, D2, read-gap legs); a torn
  part ignored. The only survivor is the first finding above.
- **Forging the non-empty proof.** The re-upload commit's obligation shares the key (`commit.rs:1157-1173`),
  but every commit sent before a compensation was answered `Conflict`, so none landed. A chased round keys
  under its own epoch (`:1328`, `:1340`). Payload equality uses the same `encode` the batch puts
  (`:945`, `:1229`). No forgery found.
- **Forging the empty proof.** Another attempt's slot at the freed index can only appear after this slot
  was released. A teardown or Abort changes the session bytes first, and an epoch never comes back. The
  only forgery is damage that erases the slot, which is the documented limit (`:240`).
- **Gates.** C5's "fail" is 3 timeouts, all infinite-loop mutants on the lap guard (`:1265`, `:1294`), with 0
  missed. C4-diff-cov's "does not apply" is the expected stale base. C4-ci reports all checks passed.
<!-- pdca:leaf-complete -->
