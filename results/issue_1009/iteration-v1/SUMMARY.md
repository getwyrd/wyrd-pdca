# Result — issue 1009 / multipart-complete-publish-segmented

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-2, a Complete whose parts total more than `MAX_MAP_CHUNKS` chunks is
  refused, so no object past 158 chunks can be stored by multipart (`0016:2289-2321`). This
  slice makes Complete the first real producer of `ChunkMap::Segmented`. It publishes through
  #653's committer with the group `(session segment_nonce, E+1)`. A fence that wrote segments
  stays charged against `attempts`, because it minted a segment epoch and a rollback obligation
  the cap exists to bound. Its release installs `retire:records:{seg}` for the attempt's group.
- Success criterion: New legs appended to `crates/core/tests/multipart_complete.rs`. child-2's
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 58 mutants tested in 4m: 5 missed, 41 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1009/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.78s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #1009’s segmented multipart publication and segment-charged attempt budget have no builder-fixable findings; fitness-to-purpose remains a human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The requested publication, ownership, and attempt-cap outcomes are falsifiable and match the protocol’s segment-write, flip, and release obligations (`brief.md:16`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`). |
| C2 Reproduction (red pre-fix) | PASS | Independently reproduced the limiting behavior: the unchanged B assertion fails on the stashed implementation with `SegmentedUnsupported { chunks: 165, limit: 158 }` (`reviewer-red.log:13`; `crates/core/tests/multipart_complete.rs:4528`). |
| C3 Change | PASS | The four-file change stays within the requested core publication slice and documents the resulting persisted behavior in the living architecture (`crates/core/src/multipart/complete.rs:1325`; `docs/design/architecture/05-building-block-view.md:254`). |
| C4 Verification (red→green) | PASS | Restoring the patch passes all 615 core tests, including 62 Complete tests; frozen full CI passes, while the independent full-CI attempt has the target-copy gitlink caveat detailed below (`reviewer-green-restored.log:233`; `gate-logs/C4-ci.log:4788`; `reviewer-ci.log:3602`). |
| C5 Causal adequacy | PASS | The tests establish actual segmented publication, bounded charged attempts, and atomic retirement ownership; the five surviving mutations do not demonstrate a reachable protocol defect, and no capability-probe symptom guard was added (`crates/core/tests/multipart_complete.rs:4528`; `crates/core/tests/multipart_complete.rs:4708`; `crates/core/src/multipart/complete.rs:1074`). |
| T1 Structure | PASS | Publication remains behind the existing metadata trait and shared committer, preserving one authority for segment sizing and transaction envelopes (`crates/core/src/multipart/complete.rs:1341`; `crates/core/src/multipart/complete.rs:1494`). |
| T2 Shape | PASS | Exact-byte session preconditions and typed retirement obligations preserve the serialization and ownership boundaries needed for safe CAS transitions (`crates/core/src/multipart/complete.rs:422`; `crates/core/src/multipart/complete.rs:1074`; `crates/core/src/multipart/complete.rs:1570`). |
| T3 Runtime | PASS | Unknown segment outcomes stop further commits, retries remain bounded, and the failed-read release case is exercised over redb; no new clock source is introduced (`crates/core/src/multipart/complete.rs:1361`; `crates/core/tests/multipart_complete.rs:4851`; `crates/core/tests/multipart_complete.rs:4913`; `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; the substantive audit must run at publish, so this deferred gate requires no human clearance (`gate-logs/T4-contribution.log:10`). Path-based prior-art checks and the other T4 evidence are recorded below. |
| T5 Judgment | PASS | Scope remains the agreed core milestone; recovery, the value ceiling, and seeded multipart DST have explicit tracked deferrals, which are settled rather than reopened (`brief.md:70`; `crates/core/src/multipart/complete.rs:723`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this core-only milestone and its verification evidence — segmented reads and failure handling work in-process, but production usability is outside this slice, and the local full-CI dependency audit relies on the frozen successful run because the target copy lacks its gitlink (`brief.md:89`; `docs/design/architecture/05-building-block-view.md:254`; `reviewer-target-state.log:4`). |

No patch defect was found. The conclusion rests on an independent assertion-level red→green, exercised ownership and charging invariants, and review of the failure paths and standing rubric. Source citations above refer to the supplied `$PDCA_TARGET`; evidence-file citations refer to this review directory. The tracked patch was restored byte-for-byte after verification.

The red→green is a behavioral failure, not a compilation failure. I stashed the patch, retained the new test fixture, and omitted only the unrelated new `DoubtfulCommit::Publication` match arm and G0-seg test so the fixture could compile against the old API. B’s body was unchanged. `cargo test -p wyrd-core --test multipart_complete b_a_list_past_one_flat_map_publishes_a_segmented_root_that_reads_back --locked` then failed with the 165-versus-158 refusal. After restoring the original patch, `cargo test -p wyrd-core --locked` passed 615 tests, including B, D3, H2-seg, J-seg, G0-seg, and the earlier Complete legs (`reviewer-red.log:6`; `reviewer-green-restored.log:84`; `reviewer-green-restored.log:233`).

The mutation survivors are advisory evidence, not five established defects. The independent `cargo mutants --in-place --in-diff ../patch.diff -p wyrd-core -o ..` run found the same five survivors: 58 tested, 28 caught, 25 unviable (`reviewer-mutants.log:9`). Its unviable count differs from the frozen run’s 12 because local denied warnings also reject mutations; it must not be described as 41 independently caught mutants. Four survivors alter the addressability check at `crates/core/src/multipart.rs:2726`: planned cursors are at most 512 (`crates/core/src/metadata.rs:572`; `crates/core/src/staged_publication.rs:605`), while read-back cursors must first pass the existing decoder (`crates/core/src/multipart/complete.rs:423`). The fifth removes the backward-cursor check at `crates/core/src/multipart/complete.rs:1062`; the owning Complete alone advances this fence’s cursor, latest-value reads cannot rewind it, and other lifecycle transitions change its exact identity (`crates/traits/src/lib.rs:1348`; `crates/core/src/multipart/complete.rs:431`). No implementation finding is raised from these unreachable distinctions.

The verification limitations are specific and evidenced:

- **Full CI:** the independent `cargo xtask ci` ran spelling, docs lint/render, formatting, workspace clippy/build/tests, conformance, dependency discovery, static-state and deployment guards, and DST clippy/tests. It ended with two failures sharing one cause: this synthetic target’s index has no `.github/advisory-db` gitlink, failing the index test and preventing the pinned dependency audit (`reviewer-ci.log:3602`; `reviewer-ci.log:3620`; `reviewer-target-state.log:4`). This is a target-state caveat, not a C4 patch defect. The frozen run explicitly shows successful pinned advisory/license checks and full CI completion (`gate-logs/C4-ci.log:4142`; `gate-logs/C4-ci.log:4147`; `gate-logs/C4-ci.log:4788`). The brief’s `typos` and docs-renderer dependencies were exercised independently, including a 101-page render and link audit (`reviewer-ci.log:2`; `reviewer-docs.log:2`).
- **Diff coverage:** no percentage was measured. The frozen oracle could not apply this dependent patch to `origin/main`; that base mismatch does not contradict the successful compilation and tests on the supplied target (`gate-logs/C4-diff-cov.log:10`).
- **Other frozen gates:** C4-verify is explicitly green-only; the independent red above supplies the missing behavioral leg (`gate-logs/C4-verify.log:161`). T4 batch review reports zero findings (`gate-logs/T4-batch-review.log:10`). The TiKV log shows both requested feature clippy builds completing, without claiming service integration (`gate-logs/host-tikv.log:112`; `gate-logs/host-tikv.log:211`). T4-contribution remains **N/A** until its mandatory publish-time re-run.

Prior art was checked by every affected path. Default-branch commit-history queries returned 19 entries for `crates/core/src/multipart.rs`, one each for `crates/core/src/multipart/complete.rs` and `crates/core/tests/multipart_complete.rs`, and 49 for `docs/design/architecture/05-building-block-view.md` (`reviewer-prior-art.log:1`). Closed-PR searches using those same paths returned 18/1/1/18 matches; corresponding closed-unmerged searches returned none (`reviewer-prior-search.json:1`). Relevant predecessors include the nonce work [#847](https://github.com/getwyrd/wyrd/pull/847), committer [#930](https://github.com/getwyrd/wyrd/pull/930), and Complete fence [#1032](https://github.com/getwyrd/wyrd/pull/1032). No conflicting prior implementation was found. The documented lack of a production caller was also checked: excluding the Complete test file, searching Rust sources for `complete_upload(` found only its definition (`crates/core/src/multipart/complete.rs:1262`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1009 (segmented publication, segment-charged budget)

Bottom line: I could not break the fix itself. The red→green proof holds up when I re-run it. What I did find are gaps in the tests: one real production path that no test exercises, a few surviving mutants, and a stale bundle.

## The evidence (re-run at `$PDCA_TARGET`, in a scratch copy)

- **Red→green is real, not a tautology.** C4-verify only reported `PASS (green-only)`, so I ran the red leg myself. I took the pre-fix `crates/core/src/multipart.rs` and `crates/core/src/multipart/complete.rs` and added only a stub `DoubtfulCommit::Publication` variant so the new test file compiles. Without that stub the new file fails with `E0599`. With it, `cargo test -p wyrd-core --test multipart_complete` gives **6 failed / 56 passed**: B (both legs), D3, H2-seg, J-seg and G0-seg fail, and every child-1/child-2 leg passes. With the patch it gives 62/62. The legs drive `complete_upload` over a real `RedbMetadataStore` through the wrapper. That is the production path, not a copy of it.
- **All four of the brief's demonstrated reds reproduce** as mutations of `complete.rs`:
  - attempts restored on a release after segments landed → D3, H2-seg and J-seg fail;
  - the `{seg}` obligation dropped from `release_batch` (`complete.rs:1074`) → D3, H2-seg and J-seg fail;
  - the segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` at `complete.rs:1494` → B, D3, G0-seg, H2-seg and J-seg fail;
  - an unknown outcome treated as `Conflict` → G0-seg and G0 fail.
- **My own mutations that the tests caught:**
  - `resume_from: 0` always → D3 fails;
  - dropping the session re-read on the fault path → J-seg fails;
  - spending a release round when the session is found at a later cursor → J-seg (read fails) fails;
  - always classifying the in-doubt commit as `Flip` → G0-seg fails.

## Refutation attempts that landed

- NEEDS-HUMAN [impl] — **No test covers resuming from the middle of the segment phase.** That is the case `0 < own.cursor < plan.segments` at `crates/core/src/multipart/complete.rs:1350` (`resume_from: own.cursor`). Every leg stages exactly one segment batch (`SEGMENT_BATCHES = 1`, `crates/core/tests/multipart_complete.rs:4349`), so the only resume ever tested is at the full plan (D3, rounds 2–3).
  - Concrete case: I mutated `:1350` to `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }`, and **all 62 legs pass**.
  - In production that mutant breaks any round that follows a later batch's `Conflict` (for example a TiKV lock on batch 2 after batch 1 landed). The round would re-send batch 0 requiring the session at cursor 0, lose on the session pin every time, and burn all `R_PUBLISH` rounds. It would end charged with `OperationAborted`.
  - This path is reachable well inside the session ceiling. `MAX_BATCH_OPS` is 500 and each row costs 2 ops, so one batch holds at most about 249 segments, or roughly 39k narrow chunks. `MAX_STAGED_CHUNKS` is 80,896.
  - `OwnFence::follow` (`complete.rs:1057`) following the cursor to a mid-plan position during a release is untested for the same reason.
  - Fix options: add a leg that forces at least two segment batches, or record an explicit `// deferred: #660` for this path. The existing #660 marker covers races, not this sequential resume.
- NEEDS-HUMAN [impl] — **Five C5 mutants survived and the gates don't record why.**
  - The four mutants on the cursor bound at `crates/core/src/multipart.rs:2726` (`written > MAX_SEGMENT_INDEX + 1`) survive because no plan gets near 1,000,000. That leaves the doc comment's claim ("no record it returns is one the decoder refuses") untested: under `>` → `==`, `with_segments_written(1_000_002)` returns a record the decoder at `multipart.rs:2846` refuses. A unit test at `MAX_SEGMENT_INDEX + 1` and `+ 2` kills all four.
  - The `follow` guard mutant at `complete.rs:1062` (`cursor >= self.cursor` → `true`) is equivalent in practice, because no writer moves the cursor backwards. Accepting it needs a written reason.
  - One more of mine survives: `writes_segments = plan.segments > 0` at `complete.rs:1340`. No leg sends an unknown outcome in a resumed round that only sends the flip, so whether that round reports `DoubtfulCommit::Flip` or `DoubtfulCommit::Publication` is never checked.
- NEEDS-HUMAN [impl] — **The bundle is stale, so diff coverage was never measured.** `gate-logs/C4-diff-cov.log` says "patch.diff does not apply on origin/main — the bundle is stale; rebase Do". The patch rewrites the pre-fence arm of `fence_complete` (`complete.rs:807-834`, where `Admit::refuses` is removed). The brief's ordering note assigns that region to child-5 ("the pre-fence and frozen-set checks") and claims no conflict. I can't check this here, because the target has no `origin/main` ref. The builder should rebase, then re-run C4-ci and C4-diff-cov.

## Attempted, could not refute

- **A release from a stale cursor 0 cannot land over written `seg:` records.** Each segment batch moves `segments_written` in the same batch that writes its records. The cursor-0 release requires the fence's cursor-0 bytes (`complete.rs:1074`), so once any batch lands, that release can only `Conflict`. The `follow` call then re-sends it charged, with `{seg}` (J-seg, read-fails leg). So "a fence that wrote none never spends the budget" and "every written `seg:` record is named" both hold.
- **The obligation key can't be taken by anyone else.** `retire:records:s:<id>:<E+1>` is otherwise written only by the flip and by `completing_teardown`, and both move the session too. So a `require_absent` conflict on it with the session unchanged has no legitimate cause.
- **The segment count can't drift.** `planned_segments` and the committer's `plan` both call `split_segments` (`crates/core/src/staged_publication.rs:417-423`, `:545`). Also, `fence_track` refuses any hook that is not a function of the cursor, before any write.
- **The flat path is unchanged.** `Placed::at(0)` re-encodes the fence record byte for byte, and all child-2 flat legs pass.
- **The release loop is bounded.** A round is skipped only when the cursor goes up, and only this Complete's batches move it.
- **`TooManySegments` behind the fence** (a non-answer on every retry) is reachable only with chunk refs near their widest encoding. It belongs to the value-ceiling slice (child-5 / #1011), so I'm not raising it.

## Where the verdict over-claims

- C4-verify's `path_line` says the red can't be isolated because "the test is co-located with the fix". That is wrong: the test is a separate file, `crates/core/tests/multipart_complete.rs`. The real reason is that the modified test file doesn't compile against pre-fix source. The behavioural red I measured above makes this harmless, but "green-only" should not be read as "red unproven".
- `overall: pass` sits on top of a failed diff-coverage row and five unexplained surviving mutants. Both rows are non-gating. Together they are the reason the mid-plan resume gap above went unseen.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found. Segment batches pin and advance the session cursor, retries resume from it, charged releases install the segment obligation atomically, and unknown outcomes stop further commits (`crates/core/src/multipart/complete.rs:1325`, `crates/core/src/multipart/complete.rs:1350`, `crates/core/src/multipart/complete.rs:1074`, `crates/core/src/multipart/complete.rs:1361`).
- No actionable reuse, simplification, or efficiency findings. The patch uses the existing committer's segment-count API and the existing attempt-segment obligation constructor (`crates/core/src/multipart/complete.rs:1494`, `crates/core/src/multipart/complete.rs:1080`).

Validation was assessed from frozen gate evidence; no builds were rerun. CI and core tests passed. Diff coverage was not measured. The five surviving mutants concern defensive cursor bounds or backward cursor movement, outside the current committer's reachable lifecycle; they do not establish an introduced defect.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept this core-only milestone and its verification evidence — segmented reads and failure handling work in-process, but production usability is outside this slice, and the local full-CI dependency audit relies on the frozen successful run because the target copy lacks its gitlink (`brief.md:89`; `docs/design/architecture/05-building-block-view.md:254`; `reviewer-target-state.log:4`).
- [ ] **No test covers resuming from the middle of the segment phase.** That is the case `0 < own.cursor < plan.segments` at `crates/core/src/multipart/complete.rs:1350` (`resume_from: own.cursor`). Every leg stages exactly one segment batch (`SEGMENT_BATCHES = 1`, `crates/core/tests/multipart_complete.rs:4349`), so the only resume ever tested is at the full plan (D3, rounds 2–3).
- [ ] **Five C5 mutants survived and the gates don't record why.**
- [ ] **The bundle is stale, so diff coverage was never measured.** `gate-logs/C4-diff-cov.log` says "patch.diff does not apply on origin/main — the bundle is stale; rebase Do". The patch rewrites the pre-fence arm of `fence_complete` (`complete.rs:807-834`, where `Admit::refuses` is removed). The brief's ordering note assigns that region to child-5 ("the pre-fence and frozen-set checks") and claims no conflict. I can't check this here, because the target has no `origin/main` ref. The builder should rebase, then re-run C4-ci and C4-diff-cov.
- [ ] size backstop — this slice is behaving oversized: patch is 124 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Close the test gaps from the adversary review (§6 items 2 and 3); no code defect was found, so keep the production logic. - Mid-plan resume: add a leg that forces at least two segment batches (today SEGMENT_BATCHES = 1, crates/core/tests/multipart_complete.rs:4349) and has a later batch answer Conflict after an earlier one landed, so the next round resumes at 0 < own.cursor < plan.segments (crates/core/src/multipart/complete.rs:1350). It must kill the mutant `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }`. Also cover OwnFence::follow (complete.rs:1057) following the cursor to a mid-plan position during a release. - Surviving mutants: add a unit test at MAX_SEGMENT_INDEX + 1 and + 2 for the cursor bound at crates/core/src/multipart.rs:2726 (kills the four survivors there); cover `writes_segments = plan.segments > 0` (complete.rs:1340) with an unknown outcome in a resumed flip-only round, asserting DoubtfulCommit::Flip vs Publication; record in build-notes.md a written reason for the equivalent mutant at complete.rs:1062 (`cursor >= self.cursor` -> true; no writer moves the cursor backwards). - The human chose iterate-do over the size backstop's iterate-plan recommendation (124 KB > 100 KB): do not split.
- By / date: Eduard Ralph / 2026-11-09

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
