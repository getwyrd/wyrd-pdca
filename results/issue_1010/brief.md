# core: Complete — recovery from unknown commit outcomes (636.5d)

> Fourth of the five slices of #658 (636.5). It builds on child-3 (636.5c). Until this slice,
> Complete returns the in-doubt error on every unknown outcome. Do reads ONLY this file. Read
> 0016 `:942-964` and `:1028`, and #895's Abort, `crates/core/src/multipart/abort.rs`
> (`:236-282`, the two kinds of doubt), before writing code. Citations were verified at
> `4bac538`; re-resolve them on your base.

- **Slug:** multipart-complete-unknown-outcomes
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** After child-1 to child-3, any `CommitUnknownResult` on a commit of Complete's
  returns the in-doubt error. That is safe, but coarse in two ways:
  - a flip that **landed** and was answered unknown makes Complete return an error for an object
    it published;
  - an unknown segment write or initial fence that did not land stops a Complete that could
    resume or retry.
  
  0016 requires the publisher to re-read and recover (`0016:942-964`). Two traps must stay closed:
  - an unchanged re-read after `may_still_commit: true` does not prove the held batch will not
    land (`crates/traits/src/lib.rs:240-247`);
  - after an unknown **initial** fence, a re-read showing `Completing@E+1` with the proposed
    bytes is not proof of ownership. A `Completing` record names no request
    (`multipart.rs:2186-2194`).
- **Success criterion:** New legs appended to `crates/core/tests/multipart_complete.rs`. The
  wrapper gains a **hold** step: it keeps a batch with `may_still_commit: true` and lands it later
  (the shape of `crates/core/tests/multipart_abort.rs:961-980`). The G0 cases for the flip, the
  segment batch and the unapplied initial fence are replaced by G1, G4 and G6. The G0 cases for
  the applied fence, the release and the failed read stay.
  **(G1)** The wrapper applies the flip and answers `CommitUnknownResult`. Complete answers
  `Published` once. The inode version advanced once, and exactly one set of obligations exists.
  **(G2) A held flip.** The wrapper holds the flip (`may_still_commit: true`) and answers every
  later commit `Conflict` without applying it, until the rounds run out. Complete returns the
  in-doubt error. Released afterwards, the held flip lands: the object is published once, with
  one set of obligations.
  **(G3) A failed re-read.** After an unknown flip outcome, the re-read of `mpu:<id>` fails.
  Complete returns the in-doubt error, never an answer.
  **(G4) An unknown segment write.** A segment batch answers `CommitUnknownResult {
  may_still_commit: false }`, applied in one run and not in another. Complete re-reads, resumes
  from `segments_written` at the same epoch, writes no `seg:` record twice, and publishes once.
  **(G5) An unknown INITIAL fence never proves ownership.** With the clock fixed, Complete A
  (parts 1, 2) and Complete B (parts 1, 3) both read `Open@E` and build byte-identical fences.
  - **Run 1:** the wrapper holds A's fence, lets B's commit (a definite success), then answers A's
    `CommitUnknownResult { may_still_commit: false }` unapplied.
  - **Run 2:** the wrapper applies A's fence and answers it unknown, then B's fence conflicts.
  
  In both runs, A's re-read finds `Completing@E+1` with exactly its proposed bytes, and A returns
  the in-doubt error with no `seg:` record and no flip. In run 1, B publishes parts 1 + 3 once.
  In run 2, B answers `Refused(OperationAborted)`, nothing is published, and the session stays
  `Completing@E+1` for the reaper (#625).
  **(G6) An unknown initial fence that did not land is retried.** The first fence answers
  `CommitUnknownResult { may_still_commit: false }` unapplied, and the re-read finds `Open@E`
  unchanged. Complete retries the fence (bounded), gets a definite success, and publishes once.
  Every earlier leg still passes.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. Every leg is a
  wrapper step (applied, held, failed-read and same-millisecond rival outcomes), not timing.
- **Invariant to restore:** child-2's and child-3's (C-1), with the recovery clause made exact:
  - a Complete answers only from a state it has read and can attribute to itself;
  - it writes segments or a flip only under a fence a commit of its own answered with a definite
    success;
  - an unknown outcome it cannot settle by a read is returned as the in-doubt error, never
    guessed;
  - a held batch is treated as able to land until a read finds the session moved.
  
  **Sources:** `0016:942-964`, `:1028`; `crates/traits/src/lib.rs:240-247`; #895's Abort.
  SELF-TEST: it binds the fence, segment, flip and release paths together.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1009
- **Conflicts with:** 1011
- **Ordering note:** `Depends on child-3`: G4 resumes child-3's segment phase, and every other leg
  recovers on paths child-1 to child-3 wrote. `Conflicts with child-5`: both edit the code around
  the fence (this slice the fence's unknown-outcome path, child-5 the check before it). The
  dependency already puts them in different waves, and the conflict keeps it that way if child-5
  slips.
- **Scope:**
  - the re-read, resume, retry and settling logic after a `CommitUnknownResult` on the initial
    fence, segment batches, flip and release;
  - the in-doubt error's doubt bookkeeping (the first `may_still_commit: true` outcome is kept,
    as `abort.rs` does);
  - the wrapper's hold step.
  
  Out of scope:
  - any change to the error type's arms (child-1 owns them);
  - any record shape change (none is needed);
  - the value ceiling (child-5);
  - the reaper (#625), the gateway's same-request recovery (#670), DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** n/a (new functionality). After child-3, G1 shows the gap: an applied flip
  answered unknown returns the in-doubt error.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/multipart_complete.rs` (appended). C4-verify reports
  `PASS (green-only)`. Its red is carried by the demonstrated reds.
- **Verification posture:** (b) green-only at C4-verify; red demonstrated.
  **Demonstrated reds:**
  - the flip's unknown outcome returned without a re-read → (G1) fails;
  - an unchanged re-read after `may_still_commit: true` taken as "did not land" → (G2) fails;
  - a failed re-read answered as a refusal → (G3) fails;
  - a resume restarting from segment 0 → (G4) fails;
  - matching fence bytes taken as ownership → (G5) fails;
  - an unapplied unknown fence returned without a retry → (G6) fails.
- **Production reach:**
  - (a) in-process over a real store;
  - (b) the S3 verb (#670 under #508) relies on this contract for its same-gateway recovery;
  - (c) no production caller;
  - (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` for every change. Peers: `abort.rs:236-282`;
  `crates/traits/src/lib.rs:240-247`; the committer's `resume_from` (#653);
  `multipart.rs:2186-2194`.
- **Prior-art check (triage cycles):** #895's Abort solved the same classification for its fence
  (merged as #909).
- **Disposition hint:** new-feature

## Design

**Who owns the fence.** After a `CommitUnknownResult` on the **initial** fence:
- a re-read showing `Open@E` unchanged with `may_still_commit: false` means the fence did not
  land, and it may be retried (bounded, G6);
- a re-read showing `Completing@E+1` with the proposed bytes is **not** ownership: return the
  in-doubt error and write nothing more (G5);
- with `may_still_commit: true` and `Open@E` unchanged, keep the doubt as Abort does. A retried
  fence that answers a definite success settles it.

The cost: a fence that really was this request's is left for the reaper after `W_completing`
(#625), and that attempt stays charged.

**After ownership is proven**, on an unknown outcome under that fence, re-read the session
(`0016:942-964`):
- `Completed` with this request's fingerprint ⇒ `Published` with the recorded publication (G1);
- `Completing@E+1` ⇒ resume from the cursor and flip again (G4);
- `Open` ⇒ answer as a fresh Complete.

Keep a `may_still_commit: true` doubt: every later batch of this attempt requires the session at
the bytes the held batch also requires, so the first of them to land settles it. When the bounded
rounds run out, or a read fails, return the in-doubt error (G2, G3). Never release a fence that
cannot be proven still held.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The production logic holds; close the test gaps. The design (brief) stands. - Kill every surviving mutant the reviewers found, using their probe cases as regressions: fence phase — Doubt::holds -> false (complete.rs ~:912/:1834: held first fence + bucket deleted + conflicting retries -> InDoubt, held fence still lands after); fence `stop` closure ignoring the doubt (~:867/:943: failed mpu: read after an unknown fence with may_still_commit true -> InDoubt{Fence, true}, not Stopped{NotPlaced}); no doubt.settle at ~:877 (unapplied unknown, may_still_commit false, then conflicts -> Stopped{NotPlaced, FenceRoundsSpent}). publication phase — doubt check before a definite refusal (~:1481-1483: [Pass, Hold, Fail{transient}] -> InDoubt{Flip, true}, no release); Doubt::in_doubt ordering (~:1862-1867: first may_still_commit:true outcome is kept); Doubt::settle no-op / no settle at ~:1525 (unapplied unknown flip, unchanged reread, conflicts, release -> Refused(OperationAborted), session Open). - Add the wrapper support needed for the two pin rules and test them: clearing a pinned held doubt once a read finds the cursor moved (~:1847), and re-pinning on a second held outcome (~:1826). - Human decisions at sign-off: refusing with OperationAborted when the reread finds Open (client retries, no re-fence with a stale stamp) is approved, including the G1-open leg; gate-row weakness (diff-cov / green-only verify) is a stacked-base artifact, cleared; the size backstop (109 KB) is overridden — do not split.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The production logic holds; close the test gaps. The design (brief) stands.
  - Kill every surviving mutant the reviewers found, using their probe cases as regressions:
    fence phase — Doubt::holds -> false (complete.rs ~:912/:1834: held first fence + bucket deleted + conflicting retries -> InDoubt, held fence still lands after); fence `stop` closure ignoring the doubt (~:867/:943: failed mpu: read after an unknown fence with may_still_commit true -> InDoubt{Fence, true}, not Stopped{NotPlaced}); no doubt.settle at ~:877 (unapplied unknown, may_still_commit false, then conflicts -> Stopped{NotPlaced, FenceRoundsSpent}).
    publication phase — doubt check before a definite refusal (~:1481-1483: [Pass, Hold, Fail{transient}] -> InDoubt{Flip, true}, no release); Doubt::in_doubt ordering (~:1862-1867: first may_still_commit:true outcome is kept); Doubt::settle no-op / no settle at ~:1525 (unapplied unknown flip, unchanged reread, conflicts, release -> Refused(OperationAborted), session Open).
  - Add the wrapper support needed for the two pin rules and test them: clearing a pinned held doubt once a read finds the cursor moved (~:1847), and re-pinning on a second held outcome (~:1826).
  - Human decisions at sign-off: refusing with OperationAborted when the reread finds Open (client retries, no re-fence with a stale stamp) is approved, including the G1-open leg; gate-row weakness (diff-cov / green-only verify) is a stacked-base artifact, cleared; the size backstop (109 KB) is overridden — do not split.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 20 mutants tested in 3m: 2 missed, 10 caught, 6 unviable, 2 timeouts
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Production logic holds except one gap; the design stands with one extension decided here. - Human decision: extend the approved publication-phase rule to the fence phase. After an unknown initial fence, if the re-read finds the session rolled back by the reaper (the charged Open@E+2 shape), answer OperationAborted so the client retries with a fresh timestamp. Never mint a new fence with the original request.fenced_at_millis. A rival's give-back (restored attempt, e.g. Open@2 attempts 0) may still fence again, so g6_held_a_read_that_finds_the_session_moved_clears_a_held_fence stays green. This overrides the brief's fence-table row "any other state | answered as after a Conflict" for that case. Update the module doc (complete.rs:118-122) and the building-block-view text to match. - Add the reviewer's probe as a regression: applied-unknown initial fence, rolled back to Open@2 before the re-read, bucket and parts still valid -> OperationAborted, no second fence (reviewer-fence-probe.rs). - Add a staged_wide() run to G1 (plan [Pass; 1 + SEGMENT_BATCHES] + Unknown { applied: true }) asserting Published, assert_published_once and assert_every_segment_named, so the unknown_flips push at complete.rs:1472 is guarded on the segmented path. - Size backstop (138 KB) is overridden again: do not split. Production code is unchanged from v1; the growth is the tests requested last round. - Diff coverage "not measured" remains the stacked-base artifact, already cleared.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Production logic holds except one gap; the design stands with one extension decided here.
  - Human decision: extend the approved publication-phase rule to the fence phase. After an unknown initial fence, if the re-read finds the session rolled back by the reaper (the charged Open@E+2 shape), answer OperationAborted so the client retries with a fresh timestamp. Never mint a new fence with the original request.fenced_at_millis. A rival's give-back (restored attempt, e.g. Open@2 attempts 0) may still fence again, so g6_held_a_read_that_finds_the_session_moved_clears_a_held_fence stays green. This overrides the brief's fence-table row "any other state | answered as after a Conflict" for that case. Update the module doc (complete.rs:118-122) and the building-block-view text to match.
  - Add the reviewer's probe as a regression: applied-unknown initial fence, rolled back to Open@2 before the re-read, bucket and parts still valid -> OperationAborted, no second fence (reviewer-fence-probe.rs).
  - Add a staged_wide() run to G1 (plan [Pass; 1 + SEGMENT_BATCHES] + Unknown { applied: true }) asserting Published, assert_published_once and assert_every_segment_named, so the unknown_flips push at complete.rs:1472 is guarded on the segmented path.
  - Size backstop (138 KB) is overridden again: do not split. Production code is unchanged from v1; the growth is the tests requested last round.
  - Diff coverage "not measured" remains the stacked-base artifact, already cleared.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1010/review-
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Production logic and design stand; close the two adversary findings from v3, nothing else. - Fix: a session read that does not decode after an unknown outcome must not settle the doubt. Return doubt.in_doubt(..) before doubt.settle when the re-read fails to decode — fence phase (complete.rs ~:888, settle runs before the decode at ~:895-896) and publication phase (~:1549-1555, the torn read sent to moved()). Regressions: [write_after(Unknown{applied: true, may_still_commit: false}, put(mpu, b"not a record"))] -> InDoubt, not Stopped{NotPlaced, Record}; [Pass, write_after(Unknown{applied: true, may_still_commit: false}, torn)] -> InDoubt, not Stopped{Gone}. - Add regression tests for the untested "stay in doubt" branches, using the adversary's probes: ~:945 [Hold, Fail{transient: true}] -> InDoubt{Fence{1}, true}, 2 commits, release_held() commits; ~:1562 [Pass, arm(FailGets(bucket_key(BUCKET)), Hold)] -> InDoubt{Flip{1}, true}, no release, then clear_read_faults + release_held lands and assert_published_once; ~:1577 same with FailGets(dirent_key(ROOT, "photos/object-1")); ~:1468 (flip build fails after a held batch, e.g. prior at u64::MAX) if it can be set up. - Size: iterate-do chosen over a split; growth should be the tests above and the small decode fix only.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Production logic and design stand; close the two adversary findings from v3, nothing else.
  - Fix: a session read that does not decode after an unknown outcome must not settle the doubt. Return doubt.in_doubt(..) before doubt.settle when the re-read fails to decode — fence phase (complete.rs ~:888, settle runs before the decode at ~:895-896) and publication phase (~:1549-1555, the torn read sent to moved()). Regressions: [write_after(Unknown{applied: true, may_still_commit: false}, put(mpu, b"not a record"))] -> InDoubt, not Stopped{NotPlaced, Record}; [Pass, write_after(Unknown{applied: true, may_still_commit: false}, torn)] -> InDoubt, not Stopped{Gone}.
  - Add regression tests for the untested "stay in doubt" branches, using the adversary's probes: ~:945 [Hold, Fail{transient: true}] -> InDoubt{Fence{1}, true}, 2 commits, release_held() commits; ~:1562 [Pass, arm(FailGets(bucket_key(BUCKET)), Hold)] -> InDoubt{Flip{1}, true}, no release, then clear_read_faults + release_held lands and assert_published_once; ~:1577 same with FailGets(dirent_key(ROOT, "photos/object-1")); ~:1468 (flip build fails after a held batch, e.g. prior at u64::MAX) if it can be set up.
  - Size: iterate-do chosen over a split; growth should be the tests above and the small decode fix only.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
