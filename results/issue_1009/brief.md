# core: Complete — segmented publication and the segment-charged budget (636.5c)

> Third of the five slices of #658 (636.5). It builds on child-2 (636.5b, flat publication).
> Do reads ONLY this file. Read 0016 the **Segment write**, **Root flip** and **Fence release**
> rows (`0016:662-664`), **decision 7(b)–(d)** (`0016:2342-2401`), and X58 (`0016:2577`,
> `:1479`) before writing code. Citations were verified at `4bac538`; re-resolve them on your
> base.

- **Slug:** multipart-complete-publish-segmented
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** After child-2, a Complete whose parts total more than `MAX_MAP_CHUNKS` chunks is
  refused, so no object past 158 chunks can be stored by multipart (`0016:2289-2321`). This
  slice makes Complete the first real producer of `ChunkMap::Segmented`. It publishes through
  #653's committer with the group `(session segment_nonce, E+1)`. A fence that wrote segments
  stays charged against `attempts`, because it minted a segment epoch and a rollback obligation
  the cap exists to bound. Its release installs `retire:records:{seg}` for the attempt's group.
- **Success criterion:** New legs appended to `crates/core/tests/multipart_complete.rs`. child-2's
  leg K is replaced by B.
  **(B) Segmented.** With a chunk size small enough that the parts total more than
  `MAX_MAP_CHUNKS` chunks (`multipart.rs:4959`), the published root is `ChunkMap::Segmented` and
  names the group `(session segment_nonce, E)` where `E` is the Completing epoch. The read is
  byte-identical. The session's `segments_written` cursor matches the plan's segment count, taken
  from the committer, never re-derived.
  **(D3) A fence that wrote segments stays charged, and the cap stops the next one**
  (`multipart.rs:5194-5200`). On a fresh session as in B, the wrapper lets every segment batch
  land and answers every flip `Conflict` without applying it. Run `MAX_COMPLETE_ATTEMPTS` (3)
  valid Completes in a row. Each one:
  - fences at a new epoch `E_i`;
  - writes its `seg:` records durably;
  - loses `R_PUBLISH` flips;
  - releases and answers `Refused(OperationAborted)`.
  
  After each one, the session is `Open` and `attempts` is one higher than before (charged, not
  restored). A `retire:records:` obligation naming the group `(segment_nonce, E_i)` exists and
  decodes. The next valid Complete answers `Refused(CompleteAttemptsExhausted)` with **zero
  commits**: no fence, and no `seg:` record of a new epoch.
  **(H2-seg)** child-2's H2, segmented: the marker is deleted after the last segment batch lands.
  Complete answers `Refused(NoSuchBucket)`. The release installed a `retire:records:` obligation
  for `(segment_nonce, E+1)`, which decodes, and `attempts` stays charged.
  **(J-seg)** child-2's J, segmented: released with the segment obligation, charged.
  **(G0-seg)** A segment batch answers `CommitUnknownResult` (once applied, once not). Complete
  returns the in-doubt error carrying it and sends no further commit.
  Every leg of child-1 and child-2 still passes.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. D3's lost flips,
  H2's deletion and G0's outcome are wrapper steps.
- **Invariant to restore:** child-2's (C-1), extended to segments:
  - every `seg:` record a fence wrote is named by the published root or by a `retire:records:`
    obligation installed in the batch that releases that fence;
  - the attempts cap bounds every fence that wrote a segment;
  - a fence that wrote none never spends the budget.
  
  **Sources:** `0016:662-664`, X58 (`0016:2577`, `:1479`), decision 7. SELF-TEST: it binds the
  segment phase, the release and the budget.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1008
- **Conflicts with:**
- **Ordering note:** `Depends on child-2`: it extends child-2's publish phase and removes
  child-2's flat-only refusal. It does not conflict with child-5, which runs in the same wave:
  child-5 edits the pre-fence and frozen-set checks and its own new test file, while this slice
  edits the publish phase and the release's obligation. child-4 builds on this slice.
- **Scope:**
  - the segment group from `attempt_segment_group` (`multipart.rs:2503`);
  - the committer's segment phase with `resume_from` from the session's cursor and a per-batch
    contribution that pins `Completing@E` and moves `segments_written` (`0016:613-620`, `:662`);
  - the release's `retire:records:{seg}` obligation and the charging rule;
  - removing child-2's leg K and its cause;
  - the docs sentence on maps past 158 chunks.
  
  Out of scope:
  - minting the nonce (Create does, #847);
  - recovery after unknown outcomes (child-4);
  - the value ceiling (child-5);
  - the drain (#659), the reaper (#625), the S3 verb (#670), DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** after child-2, leg K: a list past `MAX_MAP_CHUNKS` chunks is refused.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_complete.rs` (appended). C4-verify reports
  `PASS (green-only)`. Its red is carried by the demonstrated reds.
- **Verification posture:** (b) green-only at C4-verify; red demonstrated.
  **Demonstrated reds:**
  - `attempts` restored on every release, including one that wrote segments → (D3) fails;
  - the release's `retire:records:{seg}` obligation omitted → (D3) and (H2-seg) fail;
  - a segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` instead of taken from the
    committer, on a wide-ref fixture → (B) fails (#653's review, `results/issue_653/SUMMARY.md`);
  - an unknown segment outcome treated as `Conflict` → (G0-seg) fails.
- **Production reach:**
  - (a) in-process over a real store;
  - (b) via #670;
  - (c) no production caller;
  - (d) until #659 lands, a published segmented object cannot be deleted or overwritten
    (`crates/core/src/metadata.rs:2148-2151`, `:2188-2193`). That is harmless while nothing in
    production publishes one.
- **Citations expected:** Do cites `path:line` for every change: `attempt_segment_group`
  (`multipart.rs:2503`), the committer's `resume_from` and segment-count API (#653),
  `MAX_MAP_CHUNKS` (`:4959`), `MAX_COMPLETE_ATTEMPTS` (`:5194-5200`).
- **Prior-art check (triage cycles):** the parent's; #653 is the committer this slice is the
  first real caller of for segments.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Close the test gaps from the adversary review (§6 items 2 and 3); no code defect was found, so keep the production logic. - Mid-plan resume: add a leg that forces at least two segment batches (today SEGMENT_BATCHES = 1, crates/core/tests/multipart_complete.rs:4349) and has a later batch answer Conflict after an earlier one landed, so the next round resumes at 0 < own.cursor < plan.segments (crates/core/src/multipart/complete.rs:1350). It must kill the mutant `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }`. Also cover OwnFence::follow (complete.rs:1057) following the cursor to a mid-plan position during a release. - Surviving mutants: add a unit test at MAX_SEGMENT_INDEX + 1 and + 2 for the cursor bound at crates/core/src/multipart.rs:2726 (kills the four survivors there); cover `writes_segments = plan.segments > 0` (complete.rs:1340) with an unknown outcome in a resumed flip-only round, asserting DoubtfulCommit::Flip vs Publication; record in build-notes.md a written reason for the equivalent mutant at complete.rs:1062 (`cursor >= self.cursor` -> true; no writer moves the cursor backwards). - The human chose iterate-do over the size backstop's iterate-plan recommendation (124 KB > 100 KB): do not split.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Close the test gaps from the adversary review (§6 items 2 and 3); no code defect was found, so keep the production logic.
  - Mid-plan resume: add a leg that forces at least two segment batches (today SEGMENT_BATCHES = 1, crates/core/tests/multipart_complete.rs:4349) and has a later batch answer Conflict after an earlier one landed, so the next round resumes at 0 < own.cursor < plan.segments (crates/core/src/multipart/complete.rs:1350). It must kill the mutant `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }`. Also cover OwnFence::follow (complete.rs:1057) following the cursor to a mid-plan position during a release.
  - Surviving mutants: add a unit test at MAX_SEGMENT_INDEX + 1 and + 2 for the cursor bound at crates/core/src/multipart.rs:2726 (kills the four survivors there); cover `writes_segments = plan.segments > 0` (complete.rs:1340) with an unknown outcome in a resumed flip-only round, asserting DoubtfulCommit::Flip vs Publication; record in build-notes.md a written reason for the equivalent mutant at complete.rs:1062 (`cursor >= self.cursor` -> true; no writer moves the cursor backwards).
  - The human chose iterate-do over the size backstop's iterate-plan recommendation (124 KB > 100 KB): do not split.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 58 mutants tested in 4m: 5 missed, 41 caught, 12 unviable
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
