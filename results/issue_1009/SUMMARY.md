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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 58 mutants tested in 4m: 1 missed, 45 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1009/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.39s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1009: enable multipart Complete beyond 158 chunks while bounding failed segment-writing attempts and preserving retirement obligations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Publication, segment ownership, charged retries, and unknown outcomes have falsifiable acceptance criteria consistent with proposal 0016's segment/flip/release protocol (`brief.md:17`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`). |
| C2 Reproduction (red pre-fix) | PASS | An independently run 159-chunk publication assertion fails on the stashed baseline with `SegmentedUnsupported`, establishing the actual missing behavior (`reviewer-evidence/red.log:13`; `crates/core/tests/multipart_complete.rs:4779`). |
| C3 Change | PASS | The four-file patch stays within the agreed publication/cursor/release scope and updates the living architecture; the size exception was already authorized (`crates/core/src/multipart/complete.rs:1325`; `docs/design/architecture/05-building-block-view.md:254`; `brief.md:114`). |
| C4 Verification (red→green) | PASS | The identical publication/readback assertion passes after unstashing, all 65 Complete tests and the full core suite pass, and coverage is measured locally; whole-workspace CI has the disposable-target caveat below (`reviewer-evidence/green.log:6`; `reviewer-evidence/ci.log:978`; `reviewer-evidence/diff-coverage.json:5`). |
| C5 Causal adequacy | PASS | Fault injection refutes refunded attempts, missing retirement, wrong segment counts, unknown-result retries, and incorrect resume/classification; the flat-only restriction is removed without a capability-probe workaround (`reviewer-evidence/mutant-probes.log:1`; `crates/core/src/multipart/complete.rs:1074`; `crates/core/src/multipart/complete.rs:1494`). |
| T1 Structure | PASS | The metadata trait remains the storage boundary and the shared committer owns segmentation, so this caller does not duplicate transaction sizing or introduce backend coupling (`crates/core/src/multipart/complete.rs:1274`; `crates/core/src/multipart/complete.rs:1341`; `crates/core/src/staged_publication.rs:627`). |
| T2 Shape | PASS | Cursor records preserve canonical identity and decode bounds, while released segment groups remain epoch-scoped; the boundary and round-trip assertions exercise this contract (`crates/core/src/multipart.rs:2717`; `crates/core/src/multipart.rs:6184`; `crates/core/tests/multipart_complete.rs:4586`). |
| T3 Runtime | PASS | Publication retries and release retries remain bounded; unknown commits stop further writes, and two-batch tests exercise partial progress rather than only a completed segment phase (`crates/core/src/multipart/complete.rs:1138`; `crates/core/src/multipart/complete.rs:1361`; `crates/core/tests/multipart_complete.rs:5185`). |
| T4 Contribution | N/A | Commit-message/PR-description audit is deferred by design to the mandatory publish-time rerun; affected-path prior-art checks and the other T4 evidence are recorded below (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The carry-forward test gaps are addressed, including a killed mid-plan restart mutant; the remaining cursor-guard survivor is equivalent under the current writer and store contracts, and tracked deferrals remain settled (`crates/core/tests/multipart_complete.rs:5290`; `crates/core/src/multipart/complete.rs:1062`; `crates/traits/src/lib.rs:1275`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the intended core-only milestone against user needs — publication and byte identity are demonstrated, but no production caller exists and recovery, reclamation, S3 wiring, and DST integration remain the already tracked later slices (`brief.md:70`; `crates/core/src/multipart/complete.rs:1274`; `crates/core/tests/multipart_complete.rs:3112`). |

No implementation defect was found. Independent execution establishes the missing behavior and the required failure semantics; the outstanding decision is fitness of the agreed core-only delivery. Source citations resolve under `$PDCA_TARGET` (`target/`); brief and evidence citations resolve beside this report. The target contains the prerequisite flat-publication implementation, applies this patch, and compiles. Its tracked diff was restored byte-for-byte after the red leg. Builder notes were not read.

The red→green evidence is behavioral, not a compilation failure. A temporary integration probe reused the baseline's staging/read helpers, required a 159-chunk Complete to publish a segmented root, and compared the read bytes with the concatenated input. With `git stash`, it compiled and failed at the publication assertion; after `git stash pop`, the same probe passed. The probe is retained in `reviewer-evidence/reviewer_boundary.rs` and was moved out of the target before the normal checks. Commands used `cargo test --offline --locked -p wyrd-core --test reviewer_boundary reviewer_segmented_boundary_publishes_and_reads_back`; results are in `reviewer-evidence/red.log:6` and `reviewer-evidence/green.log:6`.

The carry-forward gaps are closed by executable counterexamples. In a separate disposable copy, seven deliberate faults each compiled and caused an assertion failure; no fault was left in the review target. `reviewer-evidence/run-mutant-probes.py` records the exact replacements and commands, and `reviewer-evidence/mutant-probes.log:1` records their outcomes:

- Refund a segment-writing attempt: D3 fails.
- Omit its retirement obligation: D3 and H2-seg each fail.
- Re-derive the segment count from chunk count: the wide-reference B test fails.
- Continue after an unknown segment outcome: G0-seg fails.
- Restart a partially written plan at zero: M returns `OperationAborted` where publication is required (`reviewer-evidence/restart-mid-plan.log:11`).
- Label every segmented round as potentially writing segments: the resumed-flip test rejects `Publication` where only `Flip` is in doubt (`reviewer-evidence/misclassify-resumed-flip.log:11`).

The two-batch fixture exercises the actual committer and metadata store, with its first committed cursor strictly between zero and the plan total. It also exercises release after the intervening read fails (`crates/core/tests/multipart_complete.rs:5216`; `crates/core/tests/multipart_complete.rs:5330`). Its part records are planted and it stores no fragment bodies, so it proves resume/retirement and resolved-map identity, not end-to-end reads; the separate staged B fixture supplies the byte-for-byte readback (`crates/core/tests/multipart_complete.rs:4438`; `crates/core/tests/multipart_complete.rs:4730`). This fixture can exhibit, and did exhibit under mutation, the forbidden restart failure.

The surviving `cursor >= self.cursor` → `true` mutant is not a demonstrated defect. Only this Complete's sequential segment batches advance that same fence, their CAS pins the preceding cursor, and a later read observes the latest committed value. Other lifecycle writers change epoch/state and fail `Placed::cursor_of`'s exact-record comparison. Therefore a lower cursor on the identical fence is unreachable under the current contracts (`crates/core/src/multipart/complete.rs:422`; `crates/core/src/multipart/complete.rs:1325`; `crates/core/src/staged_publication.rs:455`; `crates/traits/src/lib.rs:1275`). The cursor-bound comparison/arithmetic mutants at `multipart.rs:2726` were caught locally; the new unit test checks the maximum accepted cursor and the next rejected values (`crates/core/src/multipart.rs:6184`).

The gate evidence supports the following limits and results:

| Gate | Evidence and adjudication |
|------|---------------------------|
| C4-ci | Frozen CI passes, including the pinned dependency audit (`gate-logs/C4-ci.log:4144`, `:4160`, `:4791`). The independent run passes typos, docs lint/render, fmt, clippy, build, core tests, conformance, structural scanners, and the existing DST suite. Its two reported failures share one harness cause: the disposable repository index lacks `.github/advisory-db`'s gitlink, breaking the gitlink assertion and advisory-pin lookup (`reviewer-evidence/ci.log:3607`, `:3625`). This is a target-state caveat; the local full CI run is not claimed green. |
| C4-verify | Frozen evidence is explicitly green-only (`gate-logs/C4-verify.log:161`). The reviewer separately establishes behavioral red→green as described above and runs the full core suite under coverage. |
| C4-diff-cov | The frozen wrapper measured nothing because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`). Against the supplied prerequisite-aware target, `cargo llvm-cov -p wyrd-core --lcov` succeeds. Intersecting added lines with LCOV records measures 196/200 changed executable lines (98%) in the two source files, including inline unit tests; uncovered lines are `complete.rs:1303`, `:1330`, `:1381`, and `:1382` (`reviewer-evidence/diff-coverage.json:2`). This independent measure does not relabel the frozen wrapper result. |
| C5-mutants | Frozen evidence reports 58 tested: 45 caught, 12 unviable, one survivor (`gate-logs/C5-mutants.log:13`). The first independent run yields 32 caught, 25 unviable, one survivor; rerunning the 17 warning-affected cases with `--cap-lints true` catches 13 and leaves four genuinely unviable, reconciling to the same 45/12/1 totals (`reviewer-evidence/mutants.log:5`; `reviewer-evidence/mutants-lints.log:3`). The survivor is adjudicated above, not silently counted as caught. |
| T4-batch-review | The frozen wrapper reports zero blocking findings, zero recorded rejections, and zero dropped noise; the log contains that summary, not its underlying three reviews (`gate-logs/T4-batch-review.log:10`). This report independently reviews protocol, failure paths, tests, and the standing rubric. |
| T4-contribution | N/A: artifacts are not drafted at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | Both requested clippy commands pass independently with the TiKV crate and server feature selections; `reviewer-evidence/tikv.log` records the builds, corroborating `gate-logs/host-tikv.log:211`. |

Both named external dependencies were exercised: typos ran and the docs renderer produced 101 pages with a successful link audit (`reviewer-evidence/ci.log:2`, `:9`). No missing-tool shim or code-read substitute supports the publication verdict. The missing advisory gitlink limits the local repository-wide rerun; the frozen log supplies the actual original dependency-audit result.

Prior art was checked by all four affected paths, using GitHub's path-filtered commit history and closed-PR searches for each exact path. The history identifies the existing fence work ([#1032](https://github.com/getwyrd/wyrd/pull/1032)); the closed-work search also identifies the shared committer ([#930](https://github.com/getwyrd/wyrd/pull/930)). The discontinued broader protocol issue [#636](https://github.com/getwyrd/wyrd/issues/636) is closed as `not_planned`; its rejected cross-plane scope is not revived here. Query results are retained in `reviewer-evidence/prior-art.json` and `reviewer-evidence/prior-art-extra.json`. No contradictory prior disposition was found. The existing tracked deferrals for #625, #659, #660, #670, and subsequent Complete slices are not raised again; no `INTEGRATION.md` was present in the supplied target.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1009 (segmented publication, segment-charged budget)

Bottom line: I could not break the production logic. The red→green evidence holds up when re-run. One of
the brief's three invariants has no test on the segmented path, and a plausible wrong rule there passes
all 65 legs.

## Findings

- NEEDS-HUMAN [impl] — **The invariant "a fence that wrote none never spends the budget" is untested on the segmented path.** `crates/core/src/multipart/complete.rs:1076` (the `cursor == 0` branch of `release_batch`) and `:1359` (the lost-round path). Every new leg either lands a segment batch first (D3 `multipart_complete.rs:4838`, H2-seg `:4932`, J-seg `:4981`, M `:5185`/`:5290`) or ends in doubt (G0-seg). No leg has a segmented round lose with nothing landed and then release. **Concrete surviving mutant:** charge any attempt whose lost round *sent* segment batches (add `own.charge |= writes_segments` in the `Ok(Conflict)` arm at `:1359`, and take the charged branch at `:1076` when it is set). All 65 legs pass under it. **Concrete failing case it hides:** wide fixture, the segment batch answers `Conflict` without applying in all `R_PUBLISH` rounds. The release then keeps `attempts` spent and installs a `{seg}` obligation for an empty group, so three such Completes reach `CompleteAttemptsExhausted` with zero `seg:` records ever written. That is the #636 wedge, now on the segmented path. The production code gets this right. I checked with a throwaway leg in scratch, which passes on the patch and fails on the mutant: `staged_wide()`, script `[Pass] + [ConflictUnmoved; STATED_PUBLISH_ROUNDS]`, then assert `OperationAborted`, `1 + R_PUBLISH + 1` commits, `segments_of(group_of(fence)) == 0`, `attempts` unchanged, and `obligations(RETIRE_RECORDS_PREFIX)` empty. Fix: add that leg.
- Minor, info only: `complete.rs:1380-1383` is the one new branch no test runs (checked with llvm-cov). It covers a round refused for good while it had segment batches to send, where the follow-up session read finds another writer moved the session. I read `moved()` (`complete.rs:1201`) and it is correct here. `Aborting` answers from its state, and `Open` or a torn record ends `Gone` with the fault. The teardown or rollback that moved the session carries the `{seg}` obligation (0016 *Abort / reap fence* and *Completing → Aborting* rows). Fold a leg in only if the builder iterates for the item above; this is not worth a rebuild on its own.
- NEEDS-HUMAN [human] — **T4-batch-review's gating `pass` is thin evidence.** `check-gates.json` row `T4-batch-review` records "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 17.53 s for a ~148 KB diff. `gate-logs/T4-batch-review.log` holds only that one summary line, so a cache hit or three empty review outputs would look the same. I can't see `results/issue_1009/review-batch.md`. Glance at it before counting T4 as the "one deep, multi-pass review" the rubric's Definition of done asks for. This is uncertain, not a known fault.

## Evidence re-run (the brief's demonstrated reds, plus the carry-forward mutants)

I built and ran `cargo test -p wyrd-core --test multipart_complete` on a scratch copy of `$PDCA_TARGET`: 65/65 pass. The legs drive `complete_upload` (the production path) over a real Redb store through the `Logged` wrapper. Each mutant below was planted alone and the suite re-run:

- `charged` built from `opened.attempts()` (`complete.rs:396`, attempts restored on every release): D3, H2-seg, J-seg and M-release fail.
- Obligation dropped from `release_batch` (`:1082`, `:1084`): the same four fail.
- Segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` (`:1494`): B, D3, H2-seg, J-seg and both G0-seg legs fail.
- Unknown segment-round outcome mapped to `Conflict` (`:1355`): both G0-seg legs fail.
- Carry-forward `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }` (`:1350`): M fails.
- Carry-forward `writes_segments = plan.segments > 0` (`:1340`): G0-seg (resumed) fails.
- `release_batch` always restores (`:1076`): four legs fail.
- A followed cursor spends a release round (`:1171`): J-seg and M-release fail.
- The `follow` guard at `:1062` set to `true` survives. I accept the recorded reason: `with_segments_written` is called only from `complete.rs`, teardown moves the session to `Aborting`, rollback moves it to `Open`, and nothing moves the cursor backwards.

C4-diff-cov's `fail` ("patch.diff does not apply on origin/main") is not a refutation. The brief says this slice depends on #1008, so it probably stacks on an unmerged branch; the target has no `origin` ref, so I can't confirm. I measured coverage myself with `cargo llvm-cov`. In `multipart.rs`, 48/48 instrumented added lines ran. In `complete.rs`, 148/152 ran. The four that did not:
- `:1303`: the existing torn-identity `give_up`; only its signature changed.
- `:1330`: the `step` fallback, unreachable by design because the committer refuses an unfenced batch.
- `:1381-1382`: the branch in the minor item above.

## Attacks tried that did not land

- **Cursor above 0 means a segment landed.** Holds: each batch carries the cursor move in the same atomic batch (`complete.rs:1324-1331`), and the committer's `fence_track`/`prepare` (`staged_publication.rs:507-519`) refuses any contribution off the track.
- **A stale cursor after a failed read.** The release requires the old bytes, conflicts, re-reads, `follow`s forward and goes again without spending a round (`complete.rs:1171`). That is bounded by the number of batches that landed.
- **The count the flip requires vs the committer's plan.** Both come from `split_segments` (`staged_publication.rs:417-423`, `:545`), and a mismatch is refused before any write.
- **Cursor bound.** `with_segments_written` refuses past `MAX_SEGMENT_INDEX + 1`, the same bound the decoder enforces (`multipart.rs:2845`). `MAX_ROOT_SEGMENTS` (512) is far below it.
- **Release batch vs 0016:664.** It matches the row, and adds a stricter `require_absent` on the obligation key.
- **Key collision.** The `{seg}` obligation and the flip's `{parts}` obligation share `retire:records:s:<id>:<E+1>`, but at most one of the two batches can land per fence, so they never collide.
- **`R_PUBLISH` now counts rounds, segment-batch losses included.** Consistent with 0016:1478, where it is the publish-retry bound that makes Complete terminate.
- **Clock.** No new clock reads.
- **DST coverage of the new concurrent paths.** Covered by the `// deferred: #660` marker, which the rubric treats as settled; not raised.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: introduced correctness bugs (including test fidelity), and actionable reuse, simplification, or efficiency issues.

- Checked cursor advancement and resume, charged release obligations, and immediate termination on unknown outcomes (`crates/core/src/multipart/complete.rs:1074`, `crates/core/src/multipart/complete.rs:1325`, `crates/core/src/multipart/complete.rs:1350`, `crates/core/src/multipart/complete.rs:1361`). The implementation reuses the committer's segment planner and the existing retirement-obligation constructor.
- The added tests exercise intermediate-cursor resume and release, resumed flip-only unknown outcomes, and the cursor's decode boundary (`crates/core/tests/multipart_complete.rs:5185`, `crates/core/tests/multipart_complete.rs:5290`, `crates/core/tests/multipart_complete.rs:5108`, `crates/core/src/multipart.rs:6184`).

Validation used the frozen gate evidence; no builds were rerun. CI and core tests passed. Diff coverage was not measured because the patch did not apply to origin/main. The sole surviving mutant at `crates/core/src/multipart/complete.rs:1062` removes the backward-cursor guard; no supported writer moves this fence's cursor backward, consistent with the brief's recorded disposition.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the intended core-only milestone against user needs — publication and byte identity are demonstrated, but no production caller exists and recovery, reclamation, S3 wiring, and DST integration remain the already tracked later slices (`brief.md:70`; `crates/core/src/multipart/complete.rs:1274`; `crates/core/tests/multipart_complete.rs:3112`).
- [x] **The invariant "a fence that wrote none never spends the budget" is untested on the segmented path.** `crates/core/src/multipart/complete.rs:1076` (the `cursor == 0` branch of `release_batch`) and `:1359` (the lost-round path). Every new leg either lands a segment batch first (D3 `multipart_complete.rs:4838`, H2-seg `:4932`, J-seg `:4981`, M `:5185`/`:5290`) or ends in doubt (G0-seg). No leg has a segmented round lose with nothing landed and then release. **Concrete surviving mutant:** charge any attempt whose lost round *sent* segment batches (add `own.charge |= writes_segments` in the `Ok(Conflict)` arm at `:1359`, and take the charged branch at `:1076` when it is set). All 65 legs pass under it. **Concrete failing case it hides:** wide fixture, the segment batch answers `Conflict` without applying in all `R_PUBLISH` rounds. The release then keeps `attempts` spent and installs a `{seg}` obligation for an empty group, so three such Completes reach `CompleteAttemptsExhausted` with zero `seg:` records ever written. That is the #636 wedge, now on the segmented path. The production code gets this right. I checked with a throwaway leg in scratch, which passes on the patch and fails on the mutant: `staged_wide()`, script `[Pass] + [ConflictUnmoved; STATED_PUBLISH_ROUNDS]`, then assert `OperationAborted`, `1 + R_PUBLISH + 1` commits, `segments_of(group_of(fence)) == 0`, `attempts` unchanged, and `obligations(RETIRE_RECORDS_PREFIX)` empty. Fix: add that leg.
- [x] **T4-batch-review's gating `pass` is thin evidence.** `check-gates.json` row `T4-batch-review` records "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 17.53 s for a ~148 KB diff. `gate-logs/T4-batch-review.log` holds only that one summary line, so a cache hit or three empty review outputs would look the same. I can't see `results/issue_1009/review-batch.md`. Glance at it before counting T4 as the "one deep, multi-pass review" the rubric's Definition of done asks for. This is uncertain, not a known fault.
- [x] size backstop — this slice is behaving oversized: patch is 144 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-11-09

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- #1009 follow-up: add the segmented "wrote nothing → not charged" test leg (staged_wide, segment batch Conflict-unmoved for all R_PUBLISH rounds; assert attempts unchanged, no {seg} obligation); fold into child-4.
