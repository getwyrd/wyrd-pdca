# Result — issue 810 / restore-fence-generation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: no durable record tells a gateway the restore fence has run, and residue does not
  survive a re-fence. 0016 requires the restore-fence generation to complete before any gateway
  serves multipart verbs on the restored image (`0016:723-728`, `:3017-3021`, X17b). Iteration
  1's version was unsound: a session fenced to `Aborting` with unrepaired residue was skipped
  on every later pass, so the documented repair-and-rerun remedy cleared the residue finding
  and certified the generation complete with the obligations still unmet. A gateway trusting
  that marker would resume multipart verbs over an unfenced image.
- Success criterion: the NEW file `crates/custodian/tests/restore_fence_generation.rs`
  passes over in-memory doubles. The generation record is read by **raw key**, so the test
  compiles on the base. Seeded session records carry `segment_nonce` in #840's spelling
  (immediately after `clock_source`, every state), except the #842 H(i) record, which by
  definition lacks it.
  *What the record can and cannot claim.* It records the post-restore pass's own progress, in the
  store that pass fences. A restore rewinds the **whole** database
  (`docs/design/architecture/m4-first-deployment-blueprint.md:563-566`), this record included, so
  an image captured after an earlier pass completed comes back reading `complete` until the new
  pass starts. The record alone therefore cannot tell this restore's completion from one the
  image carried. In this slice X17b is closed by the deployment ordering 0016 allows
  (`0016:3017-3021`), which the m4 runbook already follows: writers stay stopped from step 7, the
  pass, until step 8 (`m4-first-deployment-blueprint.md:581-626`). The docs written for #508
  must say so (Scope). Legs:
  **(I) The generation record, on durable state.** (i) On a store no post-restore pass has
  touched, the record is absent. (ii) **During** a pass, read through a double hook at the first
  fence commit, it names the pass's generation and reads not-complete. (iii) After the pass it
  reads complete for that generation. (iv) *A restored old marker.* Seed the record as
  `complete` for generation N, as an image captured after pass N would carry it, beside a
  resurrected `Open` session. At the new pass's first fence commit the record already reads N+1
  not-complete, and after the pass N+1 complete: once a pass has started, no fence it owes runs
  under a `complete` reading. "Complete" becomes observable only after every write the pass
  makes, the mark batches included.
  **(M) A session finding that needs a human blocks completion.** For each of #842's cases that
  needs a human — a `Completing` record with no nonce (#842 H(i): left unfenced, named) and a
  `Completing` session whose `seg:` records name a chunk no `part:` record holds (#842 H(ii):
  fenced, still named) — the pass ends with `needs_human()` true and the generation **not**
  complete.
  **(M-scope) Only session findings block completion.** Every session in the store fenced
  cleanly, plus one unrelated dangling committed chunk (fewer than k fragments anywhere,
  `restore.rs:569-572`): the pass ends with `needs_human()` true **and** the generation
  complete. Blocks completion: a session the pass could not read or could not fence (an `mpu:`
  record whose key or value will not read; child-3's unsettled sessions), and a fenced session
  still named as needing a human (#842's H cases). Does not block it: findings about committed
  objects (dangling, misplaced, an unreadable `inode:` record), the pending ledger, or `part:` /
  `sidx:` records (#839's untrusted staged records included). None of those lets a session
  publish, and withholding multipart over a lost file is an availability policy 0016 does not
  state.
  **(N) Residue survives a re-fence.** After M, run the pass again in a fresh context with
  **nothing repaired**. The #842 H(ii) session is already `Aborting`; the H(i) one still does
  not decode. The second pass names both, and the new generation is **not** complete. This is
  the regression test for #664 iteration 1's `restore.rs:916` (it skipped every already-`Aborting`
  session); it must fail against that logic.
  **(N-crash) Residue survives an interrupted pass.** Two interruptions, each followed by a
  fresh-context pass with nothing repaired: (a) stop the pass immediately after the durable
  `Completing → Aborting` fence of the #842 H(ii) session (the double fails every commit after
  that one); (b) stop a later pass immediately after it writes its new generation's not-complete
  record, before any fence. After each, the next pass names the H(ii) session and does not read
  complete; after O's repair, a pass does. This constrains crash behaviour without choosing where
  residue lives (leg P).
  **(O) Repair, then a pass, and only then complete.** After M, apply the two repairs: (a) put
  the missing `part:` record back for the H(ii) session: the next pass names nothing for it and
  writes no second obligation (its fence obligations stay byte-identical); (b) replace the H(i)
  record with a decodable one carrying its nonce, standing in for the operator's repair: the next
  pass fences it, and its obligations decode through `decode_retire_obligation`. **Only** when
  both are repaired does the generation read complete. The counter-arm: repairing one of the two
  leaves it not complete.
  **(P) The judgement comes from durable state.** Every later pass above runs in a fresh context
  that shares nothing in memory with the one before. State in `build-notes.md` which durable fact
  carries the residue — re-deriving it from each `Aborting` session's records each pass (#842's
  leg K already does), or a field on the generation record — and why neither N-crash
  interruption can lose it.
  **(Q) "Complete" is never written over unfinished or unknown work.** The rule: the pass writes
  complete only after every earlier write it made was acknowledged committed, and runs no fence
  before it knows its not-complete write landed. The store may report an error for a write that
  landed, or land it after reporting (`crates/traits/src/lib.rs:204-247`, `:1301-1312`), so each
  boundary gets its own leg:
  (a) *mid-pass:* a fence or mark commit fails, once as a definite error and once as an unknown
  outcome. The pass returns `Err` and the generation reads not-complete.
  (b) *the initial invalidation:* its commit answers an unknown outcome a re-read cannot settle
  (`may_still_commit`, `lib.rs:240-247`), once applied and once not. The pass returns `Err` with
  every session untouched. Where it did not apply, the record reads what it read before (a
  restored `complete`, as in I(iv): the pre-pass state this slice documents rather than closes).
  (c) *the final completion write:* its commit answers an unknown outcome, once applied and once
  not. Where it applied the record reads complete, which is true because every earlier write was
  acknowledged. Where it did not, the record reads not-complete and the next pass completes.
  (d) *a late landing:* generation N's completion write answers unknown with `may_still_commit`,
  a new pass writes N+1 not-complete, and then the N write lands (the double applies it late,
  judging its preconditions when it lands). The record still reads N+1 not-complete: a stale
  completion never masks a newer pass.
  **(R) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: one small durable record naming the post-restore pass's generation and whether it
  finished, written not-complete before the first fence (known to have landed before any fence
  runs), and complete only after the last write, only when every earlier write was acknowledged,
  and only when no session finding needs a human (M-scope); re-evaluation of already-fenced
  sessions on every pass; the record's key and shape documented for #508 in one place
  (`05-building-block-view.md:202`, plus a paragraph in `06-runtime-view.md` and the m4
  blueprint's restore steps), **including what it cannot tell**: a `complete` restored with the
  image reads the same as this restore's until the new pass starts, so #508's gate needs a
  restore-scoped signal beside it, and until #508 the guarantee is the runbook's ordering (step 7
  before step 8); the operator text in `crates/server/src/cli.rs` saying whether the generation
  completed. Key name: use the one 0016 gives; if it gives none, `mpufence`, 1-based so that
  absent is the only spelling of "no pass has run". / out of scope: the fence itself and the
  nonce (#840–#842 — change them only if leg N, N-crash or O cannot pass otherwise, and say so in
  `build-notes.md`); a restore-scoped signal for the gateway (#508's design); the gateway's reading of the record (#508); drain status (child-1);
  `scrub.rs`, `reconstruction.rs` (#663); the mark codec (#804); the retire drain (#659);
  `crates/dst/tests/custodian.rs` unless an existing case stops passing; any edit to 0016 or an
  ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 31 mutants tested in 4m: 4 missed, 9 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_810/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.78s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #810: make post-restore multipart fence completion durable across reruns, interruptions, repairs, and uncertain commits.

The production change satisfies the exercised safety cases; one fixable reporting-test gap remains. Human decisions concern the unavailable rejected prior-art record and whether deployment ordering is sufficient until #508.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief distinguishes this pass's completion from restore identity and defines falsifiable crash, repair, and unknown-outcome cases; the operational limitation is explicit (`brief.md:109`, `brief.md:125`, `docs/design/architecture/m4-first-deployment-blueprint.md:647`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new test produced 11 assertion failures after successful compilation, including the missing durable marker (`reviewer-red.log:243`, `crates/custodian/tests/restore_fence_generation.rs:474`). |
| C3 Change | PASS | The changed surfaces fit the agreed generation-record, operator-output, and living-doc scope; existing fence semantics remain intact (`crates/custodian/src/restore.rs:599`, `crates/server/src/cli.rs:1313`, `docs/design/architecture/05-building-block-view.md:204`). |
| C4 Verification (red→green) | PASS | Restoring the patch made all 11 new tests and 16 existing fence tests pass; the frozen full CI passed, while independent CI stopped only at the sandbox's advisory-database lock (`reviewer-green.log:15`, `reviewer-green.log:32`, `reviewer-green.log:47`, `gate-logs/C4-ci.log:3938`, `reviewer-ci.log:3308`). |
| C5 Causal adequacy | PASS | Durable re-evaluation preserves residue across both interruption points, and conditional completion cannot overwrite a later generation; the fix introduces no capability probe or symptom guard (`crates/custodian/tests/restore_fence_generation.rs:629`, `crates/custodian/tests/restore_fence_generation.rs:705`, `crates/custodian/tests/restore_fence_generation.rs:868`, `crates/custodian/src/restore.rs:957`). |
| T1 Structure | PASS | Record validation stays in core and orchestration uses the existing MetadataStore seam, preserving dependency direction (`crates/core/src/multipart.rs:1997`, `crates/custodian/src/restore.rs:901`). |
| T2 Shape | PASS | A nonzero generation, closed canonical encoding, and checked exhaustion preserve exact-byte compare-and-set identity; the round-trip tests exercise that contract (`crates/core/src/multipart.rs:1955`, `crates/core/tests/multipart_budget_admission.rs:469`). |
| T3 Runtime | PASS | Progress adds constant-size conditional writes; unknown commits stop the pass, and new awaits inherit the store's termination contract without adding a clock lifecycle (`crates/custodian/src/restore.rs:918`, `crates/custodian/src/restore.rs:959`, `crates/traits/src/lib.rs:1333`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the recorded rejection/disposition of #664 iteration 1—merged history and closed PRs were checked by every affected path, but its named local patch/review are absent and GitHub #664 supplies no review comments, so that part of prior art cannot be independently settled (`brief.md:182`, `reviewer-prior-art.log:13`, `reviewer-prior-art-detail.log:168`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Cover the incomplete generation returned by the real pass—the surviving field-deletion mutant can suppress the required CLI status while all tests remain green; current CLI tests supply the field themselves (`gate-logs/C5-mutants.log:13`, `crates/custodian/src/restore.rs:662`, `crates/server/src/cli.rs:1313`, `crates/server/src/cli.rs:3417`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the operational scope—gateways must stay stopped until this restore's pass finishes, because an old complete marker is restored with the image; also accept session-only blockers while unrelated object damage remains a separate operator finding (`docs/design/architecture/m4-first-deployment-blueprint.md:647`, `crates/custodian/src/restore.rs:409`, `brief.md:162`). |

Source citations above are relative to `$PDCA_TARGET`; brief and evidence citations are relative to this review directory. All 11 post-patch file hashes match `patch.diff`. The disposable target was restored after the red run; no implementation changes were made.

The reporting gap is narrow and demonstrated by the mutation evidence. Deleting `fence_generation: Some(generation)` leaves a session-blocked report at `None`: `close_generation` returns without replacing it (`crates/custodian/src/restore.rs:951`), and the CLI emits no generation line. The new M test checks the stored marker and session findings, but not that returned field (`crates/custodian/tests/restore_fence_generation.rs:545`). Add a regression through the real pass for the incomplete report, alongside the existing formatter assertions. This is a test-fidelity defect, not evidence that the submitted implementation currently omits the field.

The other three surviving mutants do not establish production defects. Reversing the mark-batch threshold flushes smaller batches and preserves the tested safety outcome; the two `Cut::text` mutations remove explanatory prose while the structured failure verdict remains. The frozen mutation run reports 31 tested, 9 caught, 4 missed, and 18 unviable (`gate-logs/C5-mutants.log:14`, `gate-logs/C5-mutants.log:17`). Its instance-scoped wrapper was not reproduced here.

Verification evidence is sufficient for the code verdict, with these explicit limits:

- Independent commands were `cargo test --offline -p wyrd-custodian --test restore_fence_generation` with the tracked fix stashed, then the same target plus `restore_open_fence` and `restore_completing_fence` after `git stash pop`. Build output stayed inside this review directory. Results are retained in `reviewer-red.log` and `reviewer-green.log` and agree with `gate-logs/C4-verify.log:119`.
- Independent `cargo xtask ci` passed typos, docs lint/render/link audit, repository guards, formatting, workspace clippy/build/tests, and dependency-use checking. It then stopped because the sandbox cannot lock `/home/eddie/.cargo/advisory-dbs/db.lock` (`reviewer-ci.log:3308`). This is a host caveat, not a patch failure. Separately rerun conformance, statics, and the existing 50-seed DST command passed (`reviewer-conformance.log:1`, `reviewer-statics.log:3`, `reviewer-dst.log:469`, `reviewer-dst.log:577`). The frozen CI log supplies the successful advisory/license checks and remaining gate evidence (`gate-logs/C4-ci.log:3329`, `gate-logs/C4-ci.log:3938`). Both external dependencies named in the brief were actually exercised (`reviewer-ci.log:2`, `reviewer-ci.log:10`).
- Diff coverage remains unmeasured: its gate could not apply this stacked patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target itself compiled and passed; the coverage gate's base mismatch is not a C4 implementation defect.
- The frozen TiKV log shows both requested clippy builds completing; it does not demonstrate a live TiKV restore (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). The batch-review log records zero blocking findings, without supplying individual reviews (`gate-logs/T4-batch-review.log:10`). Neither substitutes for this review.
- T4 contribution-artifact audit: **N/A**. `pr-description.md` is absent by design and the substantive check must rerun at publish (`gate-logs/T4-contribution.log:10`); this deferral does not require human clearance.

The path-based prior-art investigation covered all 11 changed paths, 356 closed PRs, and the file lists of all 19 closed without merge (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:13`). Only closed PR #647 overlapped; its affected hunks concern segmented-map handling, not a fence-generation record (`reviewer-prior-art.log:23`, `reviewer-prior-art-detail.log:2`). The #664 iteration-1 artifacts named by the brief are outside the supplied evidence, and its GitHub issue has zero comments. The claimed historical disposition therefore remains for the human; the described Aborting-session failure itself is independently exercised by the passing N/N-crash cases.

Standing-rubric deferrals remain settled: seeded restore-fence DST is tracked in #843, and retire-drain interactions in #659 (`crates/custodian/tests/restore_fence_generation.rs:14`, `crates/custodian/src/restore.rs:1146`). No new live Tier-1/Tier-2 durability campaign was run; those remain useful operational follow-up. No `INTEGRATION.md` was present in the supplied target.

### Advisory — adversary

# Adversarial review — #810 restore-fence-generation

**Bottom line:** I could not break the safety property. The red→green proof holds, and the pass never writes `complete` over a write that was not acknowledged. What I did find: two parts of the fix that no test checks (shown with hand-made mutants that survive the whole `wyrd-custodian` suite), one operator-guidance conflict for a human to settle, and some smaller notes.

## Evidence re-run

- Reproduced locally in a scratch copy of `$PDCA_TARGET`. With the fix, `cargo test -p wyrd-custodian --test restore_fence_generation` gives 11/11 green. With `crates/custodian/src/restore.rs` and `crates/core/src/multipart.rs` reverted to the base, it gives 11/11 red. Each red is a failed assertion on the `mpufence` record or on the pass's `Err` (for example `crates/custodian/tests/restore_fence_generation.rs:474`, `:509`, `:640`), not a panic in a helper. The legs call the production `reconcile_after_restore` (`crates/custodian/src/restore.rs:591`), not a copy of it. This matches gate-logs/C4-verify.log.

## Findings

- NEEDS-HUMAN [impl] — **No test checks `RestoreReport::fence_generation` from a real pass, and that field is the only thing the CLI reads.** Hand mutant: delete `report.fence_generation = Some(closed);` at `crates/custodian/src/restore.rs:962`. The full `wyrd-custodian` test suite still passes. Concrete failing case: on a clean store the pass writes `{"generation":1,"complete":true}` to the store, but the operator sees `restore-fence generation 1 NOT complete — an upload session needs a human (named below). Keep multipart uploads off this store`, and nothing is named below it (`crates/server/src/cli.rs:1313`, `:1460-1478`). The other arm is C5's surviving mutant at `restore.rs:662` (delete `fence_generation: Some(generation)`). With that mutant, a run left NOT complete prints no generation line at all. The only CLI test (`crates/server/src/cli.rs:3408`) builds the report by hand. Fix: in a test file that may name base symbols (`restore_open_fence.rs` / `restore_completing_fence.rs` already import `MPUFENCE_KEY`), assert that `report.fence_generation` equals the stored record decoded, on one complete pass and one not-complete pass.

- NEEDS-HUMAN [impl] — **No test checks the opening write's compare-and-set (a write conditioned on the exact bytes read).** Two hand mutants both survive the full `wyrd-custodian` suite. (A) Drop the `require`/`require_absent` at `crates/custodian/src/restore.rs:913-916`. (C) Treat the opening `Ok(CommitOutcome::Conflict)` at `:923-927` as acknowledged. No double in `restore_fence_generation.rs` can change `mpufence` between the pass's `get` and its `commit`, so the opening path's `FenceGenerationFault::ChangedUnderPass` is never reached by any test. Yet `crates/core/src/multipart.rs:1934` and `docs/design/architecture/05-building-block-view.md:204` both promise "both writes are conditioned on the bytes last read there". Concrete failing case under (C): passes P1 and P2 both read `{3,true}`. P1 opens `{4,false}`. P2's open conflicts but P2 carries on, finishes, and its close (`{4,false}`→`{4,true}`) lands while P1 is still fencing. The record now reads complete over P1's unfinished pass. Q(d) only checks the close's precondition. Fix: add a `Fault` that rewrites `mpufence` between the read and the commit, and assert `Err` with no mark and no fence written. Severity is moderate: the runbook runs one pass with writers stopped.

- NEEDS-HUMAN [human] — **The runbook and the CLI now tell the operator different things for the same state.** `docs/design/architecture/m4-first-deployment-blueprint.md:653` (new) says "re-enable no gateway (step 8) until a run of this step has said COMPLETE". Step 8 (`:664`) is "Resume writers", so this blocks **all** writes. Eleven lines up, `:644` still says "Serve no multipart uploads until a run has fenced every upload". The CLI's NOT-complete line (`crates/server/src/cli.rs:1474`) says only "Keep multipart uploads off this store". Concrete case: an #842 H(i) record (`Completing`, no nonce) waiting on a manual repair. The runbook keeps the whole cluster write-down until someone fixes it. The CLI implies only multipart is affected. The brief's own M-scope reasoning called withholding service over a finding "an availability policy 0016 does not state". A human should pick the intended rule (all writers held vs. multipart only, given no per-verb switch exists before #508), and the other two texts should then be aligned to it.

- NEEDS-HUMAN [impl] — (minor) The patch weakens the K check (a second pass changes nothing) in `crates/custodian/tests/restore_open_fence.rs:763`. The old `unsettled_debug` compare covered every report field from `sessions_unsettled` onward, which included `segments_unaccounted`. The replacement compares `sessions_unsettled` only. Also compare `first.segments_unaccounted == second.segments_unaccounted` to keep the old strength.

## Notes (no action required to gate)

- `Cut::text` at `crates/custodian/src/restore.rs:1803` (two C5 survivors): no test asserts the audit summary's INCOMPLETE reason for either cut. `Cut::Generation` is new text an operator would grep for. Low priority.
- C4-diff-cov's "fail" is a staging artifact, not a defect: the patch is stacked on #839–#842, so it does not apply to bare `origin/main`. Diff coverage was therefore **not measured**. The hand mutants above partly fill that gap.
- check-gates.json's T4-batch-review row ("0 blocking, 0 recorded-rejected, 0 noise-dropped") finished in 32.5 s, for three reviewers over a diff of about 1,900 lines. I cannot see `review-batch.md`. Someone should confirm it actually reviewed this diff before treating it as the rubric's "one deep, multi-pass review".
- The C5 survivor at `restore.rs:798` (`>=` → `<`) is an equivalent mutant: the batching logic is unchanged from before and the mutant only produces more, smaller mark batches. Not a finding.

## Attempted to refute, could not

- **Complete over an unacknowledged write:** every write before `close_generation` returns `Err` on failure. Mark batches go through `commit_marks` with `?` (`restore.rs:799`, `:811`; `Ok(Conflict)` now errors too). A fence commit's `Err` returns (`:1074-1077`). `check_attempt` only reads. So `close_generation` is reached only after every earlier write was acknowledged.
- **Residue lost across passes or crashes:** each `Aborting@E'` session is re-judged from its own records on every pass (`recheck_fenced`, `:1149`), and an undecodable `mpu:` value is named again by `plan_fence` (`:1102`). Nothing is carried in memory or on the record, so neither N-crash interruption can drop it. A pass that skips `Aborting` sessions fails leg N, and a predicate of plain `needs_human()` fails M-scope's `dangling` arm, as the brief's self-test requires.
- **Older tests passing for the wrong reason:** the new first write could absorb a commit fault meant for a later commit. But the fault doubles in `restore_open_fence.rs:101`, `restore_completing_fence.rs:104` and `gc_reclaim_intent.rs:295` all match on keys, not commit counts, and `gc_ledger_walk.rs:1562` checks the error's text.
- **ABA on the record bytes** (the same bytes coming back, so a stale write's precondition matches again): a stale completion can match a newer pass's bytes only if the generation counter goes backwards (a restore, or the operator removing the record as `FenceGenerationFault::Unreadable` advises) while that completion is still in flight. I could not make this concrete within a backend transaction's lifetime.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/tests/restore_fence_generation.rs:545`, `crates/server/src/cli.rs:3417`: The incomplete-generation test checks the stored marker but never the returned report's generation, while the CLI test supplies that field itself. The frozen C5 evidence confirms that deleting `fence_generation: Some(generation)` at `crates/custodian/src/restore.rs:662` survives. That regression would leave incomplete reports with `None`, silently suppressing the operator's generation/“NOT complete” line at `crates/server/src/cli.rs:1313`. Add a post-fix test asserting that a real report with unsettled sessions contains the same non-complete generation as the durable marker; keep the base-compatible regression file free of new API dependencies.

No additional findings on introduced production correctness bugs or reuse, simplification, and efficiency. Validation used the frozen gate evidence; gates were not rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Confirm the recorded rejection/disposition of #664 iteration 1—merged history and closed PRs were checked by every affected path, but its named local patch/review are absent and GitHub #664 supplies no review comments, so that part of prior art cannot be independently settled (`brief.md:182`, `reviewer-prior-art.log:13`, `reviewer-prior-art-detail.log:168`).
- [ ] T5 Judgment — Cover the incomplete generation returned by the real pass—the surviving field-deletion mutant can suppress the required CLI status while all tests remain green; current CLI tests supply the field themselves (`gate-logs/C5-mutants.log:13`, `crates/custodian/src/restore.rs:662`, `crates/server/src/cli.rs:1313`, `crates/server/src/cli.rs:3417`).
- [ ] Validation — fitness-to-purpose — Accept the operational scope—gateways must stay stopped until this restore's pass finishes, because an old complete marker is restored with the image; also accept session-only blockers while unrelated object damage remains a separate operator finding (`docs/design/architecture/m4-first-deployment-blueprint.md:647`, `crates/custodian/src/restore.rs:409`, `brief.md:162`).
- [ ] **No test checks `RestoreReport::fence_generation` from a real pass, and that field is the only thing the CLI reads.** Hand mutant: delete `report.fence_generation = Some(closed);` at `crates/custodian/src/restore.rs:962`. The full `wyrd-custodian` test suite still passes. Concrete failing case: on a clean store the pass writes `{"generation":1,"complete":true}` to the store, but the operator sees `restore-fence generation 1 NOT complete — an upload session needs a human (named below). Keep multipart uploads off this store`, and nothing is named below it (`crates/server/src/cli.rs:1313`, `:1460-1478`). The other arm is C5's surviving mutant at `restore.rs:662` (delete `fence_generation: Some(generation)`). With that mutant, a run left NOT complete prints no generation line at all. The only CLI test (`crates/server/src/cli.rs:3408`) builds the report by hand. Fix: in a test file that may name base symbols (`restore_open_fence.rs` / `restore_completing_fence.rs` already import `MPUFENCE_KEY`), assert that `report.fence_generation` equals the stored record decoded, on one complete pass and one not-complete pass.
- [ ] **No test checks the opening write's compare-and-set (a write conditioned on the exact bytes read).** Two hand mutants both survive the full `wyrd-custodian` suite. (A) Drop the `require`/`require_absent` at `crates/custodian/src/restore.rs:913-916`. (C) Treat the opening `Ok(CommitOutcome::Conflict)` at `:923-927` as acknowledged. No double in `restore_fence_generation.rs` can change `mpufence` between the pass's `get` and its `commit`, so the opening path's `FenceGenerationFault::ChangedUnderPass` is never reached by any test. Yet `crates/core/src/multipart.rs:1934` and `docs/design/architecture/05-building-block-view.md:204` both promise "both writes are conditioned on the bytes last read there". Concrete failing case under (C): passes P1 and P2 both read `{3,true}`. P1 opens `{4,false}`. P2's open conflicts but P2 carries on, finishes, and its close (`{4,false}`→`{4,true}`) lands while P1 is still fencing. The record now reads complete over P1's unfinished pass. Q(d) only checks the close's precondition. Fix: add a `Fault` that rewrites `mpufence` between the read and the commit, and assert `Err` with no mark and no fence written. Severity is moderate: the runbook runs one pass with writers stopped.
- [ ] **The runbook and the CLI now tell the operator different things for the same state.** `docs/design/architecture/m4-first-deployment-blueprint.md:653` (new) says "re-enable no gateway (step 8) until a run of this step has said COMPLETE". Step 8 (`:664`) is "Resume writers", so this blocks **all** writes. Eleven lines up, `:644` still says "Serve no multipart uploads until a run has fenced every upload". The CLI's NOT-complete line (`crates/server/src/cli.rs:1474`) says only "Keep multipart uploads off this store". Concrete case: an #842 H(i) record (`Completing`, no nonce) waiting on a manual repair. The runbook keeps the whole cluster write-down until someone fixes it. The CLI implies only multipart is affected. The brief's own M-scope reasoning called withholding service over a finding "an availability policy 0016 does not state". A human should pick the intended rule (all writers held vs. multipart only, given no per-verb switch exists before #508), and the other two texts should then be aligned to it.
- [ ] (minor) The patch weakens the K check (a second pass changes nothing) in `crates/custodian/tests/restore_open_fence.rs:763`. The old `unsettled_debug` compare covered every report field from `sessions_unsettled` onward, which included `segments_unaccounted`. The replacement compares `sessions_unsettled` only. Also compare `first.segments_unaccounted == second.segments_unaccounted` to keep the old strength.
- [ ] `crates/custodian/tests/restore_fence_generation.rs:545`, `crates/server/src/cli.rs:3417`: The incomplete-generation test checks the stored marker but never the returned report's generation, while the CLI test supplies that field itself. The frozen C5 evidence confirms that deleting `fence_generation: Some(generation)` at `crates/custodian/src/restore.rs:662` survives. That regression would leave incomplete reports with `None`, silently suppressing the operator's generation/“NOT complete” line at `crates/server/src/cli.rs:1313`. Add a post-fix test asserting that a real report with unsettled sessions contains the same non-complete generation as the durable marker; keep the base-compatible regression file free of new API dependencies.
- [ ] **A pass counter does not identify the current restore.** `brief.md:24–29` assumes an absent initial marker and says advancing it on a second pass means “a later restore invalidates an earlier completion.” The backup covers the **whole database** (`docs/design/architecture/m4-first-deployment-blueprint.md:563`), so it can restore an old complete marker alongside sessions created after that marker. Before the new pass starts, that marker still reads complete; two consecutive passes do not test this case. Revise the claim and the documented #508 contract to specify how completion is associated with this restore, and add a restored-old-marker scenario. Retaining mandatory deployment ordering is a valid scope choice; the marker alone does not establish that ordering.
- [ ] **“A failed pass never reads complete” exceeds the specified failure model.** Leg Q (`brief.md:48–49`) injects only a mid-pass commit failure. `MetadataStore` explicitly permits an `Err` after a write lands (`crates/traits/src/lib.rs:1301–1312`), even a transaction that lands later (`:240–246`). Thus the final completion write can become durable despite an error; conversely, failure of the initial invalidation can leave the previous complete marker untouched. Define the expected generation and distinguish unfinished work from an unknown commit outcome. Require boundary cases for initial invalidation and final publication, including applied-but-unacknowledged commits, rather than treating every error as proof of non-completion.
- [ ] **The residue regression omits a crash during the pass.** N/P restart only after M finishes; Q checks the failed pass's marker without restarting it (`brief.md:31–49`). Because P explicitly permits residue in the generation record, saving that residue only at the end could pass these tests yet lose the finding if execution stops after `Completing → Aborting` and before the residue save. The target protocol requires the fence and its reclamation obligations in one batch precisely to prevent a session with no reclaimer (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:665`). Add interruption immediately after a durable fence and during generation rollover, followed by a fresh-context retry that must retain the blocker and eventually install the repaired obligation. This constrains crash behavior without choosing where residue lives.
- [ ] **The completion predicate adds a broader availability policy.** The stated invariant concerns resurrected sessions and their deleters (`brief.md:57–59`), but scope requires “nothing needing a human” (`:67–69`). Existing `needs_human()` includes unrelated committed-file `dangling` and `misplaced` findings (`crates/custodian/src/restore.rs:212–216`); dangling files can be irrecoverably lost (`:150–153`). A fully fenced image could therefore never complete while an unrelated lost-file map remains, making the future #508 gate withhold all multipart service. M tests only session residue. Either narrow completion blockers to the intended fencing invariant, or explicitly include this broader policy, its operator remedy, and a test with fully fenced sessions plus an unrelated dangling file.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 Contribution — Confirm the recorded rejection/disposition of #664 iteration 1—merged history and closed PRs were checked by every affected path, but its named local patch/review are absent and GitHub #664 supplies no review comments, so that part of prior art cannot be independently settled (`brief.md:182`, `reviewer-prior-art.log:13`, `reviewer-prior-art-detail.log:168`).; T5 Judgment — Cover the incomplete generation returned by the real pass—the surviving field-deletion mutant can suppress the required CLI status while all tests remain green; current CLI tests supply the field themselves (`gate-logs/C5-mutants.log:13`, `crates/custodian/src/restore.rs:662`, `crates/server/src/cli.rs:1313`, `crates/server/src/cli.rs:3417`).; **No test checks `RestoreReport::fence_generation` from a real pass, and that field is the only thing the CLI reads.** Hand mutant: delete `report.fence_generation = Some(closed);` at `crates/custodian/src/restore.rs:962`. The full `wyrd-custodian` test suite still passes. Concrete failing case: on a clean store the pass writes `{"generation":1,"complete":true}` to the store, but the operator sees `restore-fence generation 1 NOT complete — an upload session needs a human (named below). Keep multipart uploads off this store`, and nothing is named below it (`crates/server/src/cli.rs:1313`, `:1460-1478`). The other arm is C5's surviving mutant at `restore.rs:662` (delete `fence_generation: Some(generation)`). With that mutant, a run left NOT complete prints no generation line at all. The only CLI test (`crates/server/src/cli.rs:3408`) builds the report by hand. Fix: in a test file that may name base symbols (`restore_open_fence.rs` / `restore_completing_fence.rs` already import `MPUFENCE_KEY`), assert that `report.fence_generation` equals the stored record decoded, on one complete pass and one not-complete pass.; **No test checks the opening write's compare-and-set (a write conditioned on the exact bytes read).** Two hand mutants both survive the full `wyrd-custodian` suite. (A) Drop the `require`/`require_absent` at `crates/custodian/src/restore.rs:913-916`. (C) Treat the opening `Ok(CommitOutcome::Conflict)` at `:923-927` as acknowledged. No double in `restore_fence_generation.rs` can change `mpufence` between the pass's `get` and its `commit`, so the opening path's `FenceGenerationFault::ChangedUnderPass` is never reached by any test. Yet `crates/core/src/multipart.rs:1934` and `docs/design/architecture/05-building-block-view.md:204` both promise "both writes are conditioned on the bytes last read there". Concrete failing case under (C): passes P1 and P2 both read `{3,true}`. P1 opens `{4,false}`. P2's open conflicts but P2 carries on, finishes, and its close (`{4,false}`→`{4,true}`) lands while P1 is still fencing. The record now reads complete over P1's unfinished pass. Q(d) only checks the close's precondition. Fix: add a `Fault` that rewrites `mpufence` between the read and the commit, and assert `Err` with no mark and no fence written. Severity is moderate: the runbook runs one pass with writers stopped.; (minor) The patch weakens the K check (a second pass changes nothing) in `crates/custodian/tests/restore_open_fence.rs:763`. The old `unsettled_debug` compare covered every report field from `sessions_unsettled` onward, which included `segments_unaccounted`. The replacement compares `sessions_unsettled` only. Also compare `first.segments_unaccounted == second.segments_unaccounted` to keep the old strength.; `crates/custodian/tests/restore_fence_generation.rs:545`, `crates/server/src/cli.rs:3417`: The incomplete-generation test checks the stored marker but never the returned report's generation, while the CLI test supplies that field itself. The frozen C5 evidence confirms that deleting `fence_generation: Some(generation)` at `crates/custodian/src/restore.rs:662` survives. That regression would leave incomplete reports with `None`, silently suppressing the operator's generation/“NOT complete” line at `crates/server/src/cli.rs:1313`. Add a post-fix test asserting that a real report with unsettled sessions contains the same non-complete generation as the durable marker; keep the base-compatible regression file free of new API dependencies.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
