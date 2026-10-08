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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 31 mutants tested in 4m: 1 missed, 12 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_810/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.12s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #810: persist restore-fence progress, preserve unresolved session residue across reruns and crashes, and report completion accurately to operators.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance criteria distinguish pass completion from restore identity and explicitly limit blockers to session findings, making the safety claim testable within the declared scope (`brief.md:26`, `brief.md:49`, `brief.md:125`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new regression file produced 12 assertion failures on the supplied base, with successful compilation (`reviewer-red-green.log:149`, `reviewer-red-green.log:255`). |
| C3 Change | PASS | The persisted field, operator output, and all three living architecture updates stay within the requested slice; the existing session-fence implementation is retained (`crates/core/src/multipart.rs:1955`, `crates/server/src/cli.rs:1460`, `docs/design/architecture/05-building-block-view.md:204`). |
| C4 Verification (red→green) | PASS | Restoring the fix independently produced 12/12 passing regressions; workspace, DST, and feature checks also passed; the full CI verdict relies on frozen evidence for the locally inaccessible advisory cache (`reviewer-red-green.log:301`, `reviewer-ci-remaining.log:545`, `reviewer-tikv.log:200`, `gate-logs/C4-ci.log:3337`). |
| C5 Causal adequacy | PASS | Durable session records retain the unresolved finding across both interruption windows, and generation preconditions reject stale completion; this addresses lost certification state without a capability-probe workaround (`crates/custodian/src/restore.rs:957`, `crates/custodian/src/restore.rs:1024`, `crates/custodian/tests/restore_fence_generation.rs:723`, `crates/custodian/tests/restore_fence_generation.rs:887`). |
| T1 Structure | PASS | The codec remains in core, store operations use the existing MetadataStore seam, and the CLI consumes the returned report; no concrete-backend dependency crosses that boundary (`crates/core/src/multipart.rs:1997`, `crates/custodian/src/restore.rs:901`, `crates/server/src/cli.rs:1313`). |
| T2 Shape | PASS | The singleton has a disjoint key, positive generation numbers, strict decoding, and byte-identical round trips, preserving the identity required by its compare-and-set writes (`crates/core/src/multipart.rs:1139`, `crates/core/tests/multipart_budget_admission.rs:465`, `crates/core/tests/multipart_budget_admission.rs:497`). |
| T3 Runtime | PASS | An unacknowledged opening stops all subsequent writes, earlier commit errors prevent completion, and closing conflicts preserve the newer generation; no new clock or shared-global lifecycle is introduced (`crates/custodian/src/restore.rs:599`, `crates/custodian/src/restore.rs:918`, `crates/custodian/src/restore.rs:959`, `crates/custodian/src/restore.rs:987`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the recorded rejection/disposition of #664 iteration 1 — all affected paths and closed PRs were checked, but its original patch/review are unavailable and issue #664 has no comments, leaving that rejection history independently unsettled (`brief.md:182`, `reviewer-prior-art-full.log:136`, `reviewer-prior-art-rejected.log:175`, `reviewer-prior-art.log:148`). |
| T5 Judgment | PASS | The prior report-field, opening-conflict, and idempotence gaps now have behavioral assertions; the independently reproduced surviving mutant changes batching frequency without defeating the completion invariant (`crates/custodian/tests/restore_open_fence.rs:765`, `crates/custodian/tests/restore_open_fence.rs:881`, `crates/custodian/tests/restore_fence_generation.rs:923`, `reviewer-mutant.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept ordering-only protection until #508 and session-only completion blockers — a restored old COMPLETE cannot identify this restore, while unrelated lost or unreadable objects may coexist with a completed fence (`docs/design/architecture/05-building-block-view.md:204`, `docs/design/architecture/m4-first-deployment-blueprint.md:647`, `brief.md:49`). |

No implementation defect was found. Independent execution supports the requested completion protocol and the repaired regression coverage. The two human decisions above remain advisory sign-off matters.

All source citations are relative to `$PDCA_TARGET` (the supplied `target/` copy); brief and evidence citations are relative to this review directory. All 11 patch postimages match that target's bytes. The tracked fix was restored byte-for-byte after the stash/pop experiment, and no implementation edits remain from review (`reviewer-red-green.log:304`).

The independent runs exercised the production pass with durable-state copies, unknown outcomes applied and unapplied, late completion, competing opening writes, and repairs. Workspace tests also passed the real-redb path through backend dispatch and CLI rendering, including complete and incomplete generations (`reviewer-ci.log:2174`; `crates/server/src/cli.rs:3461`). Spelling, docs lint/render, formatting, clippy, build, workspace tests, and dependency-usage checks passed locally. Separate conformance, statics, DST clippy/tests, and both TiKV feature checks passed (`reviewer-ci-remaining.log:1`, `reviewer-ci-remaining.log:4`, `reviewer-ci-remaining.log:545`, `reviewer-tikv.log:101`, `reviewer-tikv.log:200`). The brief's external dependencies, `typos` and the docs renderer, were actually exercised (`reviewer-ci.log:2`, `reviewer-ci.log:7`).

The local full-CI command stopped at cargo-deny's inability to lock the read-only advisory database; an offline attempt met the same restriction (`reviewer-ci.log:3241`, `reviewer-deny.log:1`). This is a reviewer-host limitation. The frozen log explicitly shows successful default and all-feature dependency audits, the deployment guard, and the completed CI run (`gate-logs/C4-ci.log:3337`, `gate-logs/C4-ci.log:3348`, `gate-logs/C4-ci.log:3351`, `gate-logs/C4-ci.log:3358`, `gate-logs/C4-ci.log:3948`). The advisory diff-coverage row measured no coverage because its origin/main base could not apply this stacked patch; that supplies no patch-defect evidence and no coverage percentage (`gate-logs/C4-diff-cov.log:10`). The supplied review target itself built and tested successfully.

The remaining mutation survivor is not a missed completion failure: replacing `>= MARK_BATCH` with `< MARK_BATCH` flushes each first pending mark immediately, then clears the batch, while retaining acknowledgement before completion (`crates/custodian/src/restore.rs:798`, `crates/custodian/src/restore.rs:805`). It changes throughput and commit count. The focused rerun reproduced that survivor and caught the other selected mutation (`reviewer-mutant.log:4`); the frozen full run recorded 12 caught, one missed, and 18 unviable mutants, so it does not establish exhaustive coverage (`gate-logs/C5-mutants.log:14`). The tracked #843 deferral for seeded session-fence DST coverage is respected (`crates/custodian/tests/restore_fence_generation.rs:18`). Tier-1 disk-fault and Tier-2 kill/reconstruct observation remain worthwhile deployment follow-ups for this custodian durability change; neither live campaign was run here.

The prior-art investigation queried merged/default-branch history by every affected file path and enumerated 356 closed PRs, inspecting all 19 closed-unmerged file lists. Only closed-unmerged #647 overlapped, for segmented-map handling rather than a fence-generation record (`reviewer-prior-art-full.log:2`, `reviewer-prior-art-full.log:136`, `reviewer-prior-art-full.log:146`, `reviewer-prior-art-rejected.log:1`). The unavailable #664 artifacts are the specific unresolved evidence, not a new objection to the corrected residue handling.

`T4-contribution`: **N/A** — the publication artifacts are absent by design, and the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). The frozen batched-review row records zero blocking findings; its log supports that count (`gate-logs/T4-batch-review.log:10`).

### Advisory — adversary

# Adversarial review — #810 restore-fence generation

Advisory only. Bottom line: I could not break the fix. One question about scope goes to a human (below); everything else I tried held.

## Findings

- NEEDS-HUMAN [human] — **"Complete" checks less than the brief's invariant says.** The brief's invariant (`brief.md:109-111`) says a restored image counts as fenced only when "every record a fenced session wrote has a named deleter". The completion test, `sessions_settled` (`crates/custodian/src/restore.rs:409`), only looks at #842's segment check. For an `Aborting` session, `recheck_fenced` (`crates/custodian/src/restore.rs:1163-1168`) checks the `retire:records` obligation and the `seg:` range, and never looks at the `retire:bytes` obligation that owes the session's `part:`/`sidx:` records.
  - **Failing case (I ran it):** seed `mpu:abab…` as `Aborting@4` (the state an earlier Open fence leaves), plus `part:abab…:000001`, and no `retire:` record at all. The pass returns generation 1 **complete**, `needs_human() == false`, and names nothing. The CLI then prints "none is left for a human" (`crates/server/src/cli.rs:1465`) over part records that nothing will ever delete.
  - **Why it is not `[impl]`:** the brief's own list of what blocks completion (legs M/M-scope, `brief.md:44-58`) names only #842's H cases. It also puts the fence and its re-check out of scope (`brief.md:137-138`). So the patch matches the brief's legs but not its invariant text. The existing `deferred: #659` marker (`restore.rs:1146`) covers the `seg:` range and the records obligation, not this case.
  - **Impact:** this is a leak, not a safety hole. An `Aborting` session cannot publish. The case needs a damaged store or a half-run retire drain (#659 has not landed).
  - **Decision needed:** accept the narrower claim (the 05 doc text at `docs/design/architecture/05-building-block-view.md:204` already states it accurately), or file a follow-up to extend #842's re-check to the bytes obligation.

## What I attacked and could not break

- **The red→green proof.** I re-ran it on a fresh clone of the target's base (`08122d6`). Base plus only the new test file: 12 of 12 fail, all by assertion, none by compile error. With `patch.diff` applied, the tree matches `$PDCA_TARGET` byte for byte, and `restore_fence_generation` (12), `restore_open_fence` (11) and `restore_completing_fence` (7) all pass. The tests call the production `reconcile_after_restore` through the `MetadataStore` trait, not a copy of it.
- **The iteration-1 regression (leg N).** I put back #664 iteration 1's defect: `Plan::Fenced` returns `Ok(())` without calling `recheck_fenced` (`restore.rs:1025`). Both `residue_survives_a_re_fence_and_only_both_repairs_complete_the_generation` (`crates/custodian/tests/restore_fence_generation.rs:647`) and `residue_survives_an_interrupted_pass` (`:723`) fail. So leg N does fail against the old logic, as the brief requires.
- **Last round's untested compare-and-set on the opening write** (`restore.rs:913-927`). I re-ran both hand mutants from last round: (A) drop the `require`/`require_absent`, and (C) treat the opening `Conflict` as acknowledged. Both are now caught by `a_record_changed_under_the_pass_is_never_written_over` (`restore_fence_generation.rs:922`).
- **Last round's untested report field.** `RestoreReport::fence_generation` is now checked against the stored record on a real pass, in two places: `the_report_carries_the_generation_the_pass_wrote` (`crates/custodian/tests/restore_open_fence.rs:867`) and the redb-backed `restore_verdict_prints_the_generation_the_pass_left_in_the_store` (`crates/server/src/cli.rs:3461`). The K check is back to full strength (`restore_open_fence.rs:766`).
- **Late landings and concurrent passes.** Generation numbers only go up within one store, and both writes require the exact bytes last read. So a late open or close from an earlier pass always finds a different record and writes nothing. The one repeat of the same bytes I found needs an in-flight commit to survive a whole metadata restore, or an operator deleting an unreadable record. I don't count that as realistic.
- **Unused-looking new code.** The `Conflict` arm in `commit_marks` (`restore.rs:990`) has no test. That is fine: the trait contract says a batch with no preconditions never answers `Conflict` (`crates/traits/src/lib.rs:1466`), so the arm is defensive only.
- **Runbook claim.** The runbook says only the NOT FENCED and SEGMENTS bills hold the generation back (`docs/design/architecture/m4-first-deployment-blueprint.md:649-650`). An `mpu:` key that will not parse is listed under UNREADABLE (`:611-612`), so I checked it. It shows up in both `unresolvable` and `sessions_unsettled` and does block completion, so the claim holds.

## Gate evidence, read directly

- **C5 surviving mutant** (`gate-logs/C5-mutants.log`: `restore.rs:798:26 replace >= with <`). This is the existing `if batched.len() >= MARK_BATCH` check. The patch only changed the line inside it, and the mutant gives the same end state (one commit per mark). It is not a hole in this diff's tests.
- **C4-diff-cov "fail"** (`gate-logs/C4-diff-cov.log`). It says the patch does not apply on `origin/main`. That is expected for a stacked bundle whose base is `origin/main` plus #839–#842: the patch applied cleanly to that base. So it is not evidence the bundle is stale, but diff coverage was never measured.

### Advisory — code-review

No findings. The diff is clean under both advisory lenses: introduced correctness bugs (including test fidelity), and actionable reuse, simplification, or efficiency issues.

Reviewed the generation's opening and completion preconditions and error propagation (`crates/custodian/src/restore.rs:901`, `crates/custodian/src/restore.rs:946`), durable residue rechecks, codec boundaries, and operator reporting. The added tests cover opening conflicts (`crates/custodian/tests/restore_fence_generation.rs:922`) and the generation returned by a real pass (`crates/custodian/tests/restore_open_fence.rs:869`).

Validation used the frozen evidence; no builds or tests were rerun. CI passed, and all 12 regression tests passed post-fix and failed by assertion on the base. Diff coverage was not measured because the patch did not apply to origin/main. The sole surviving mutant changes the pre-existing batching condition at `crates/custodian/src/restore.rs:798`; it is not an introduced defect.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm the recorded rejection/disposition of #664 iteration 1 — all affected paths and closed PRs were checked, but its original patch/review are unavailable and issue #664 has no comments, leaving that rejection history independently unsettled (`brief.md:182`, `reviewer-prior-art-full.log:136`, `reviewer-prior-art-rejected.log:175`, `reviewer-prior-art.log:148`).
- [x] Validation — fitness-to-purpose — Accept ordering-only protection until #508 and session-only completion blockers — a restored old COMPLETE cannot identify this restore, while unrelated lost or unreadable objects may coexist with a completed fence (`docs/design/architecture/05-building-block-view.md:204`, `docs/design/architecture/m4-first-deployment-blueprint.md:647`, `brief.md:49`).
- [x] **"Complete" checks less than the brief's invariant says.** The brief's invariant (`brief.md:109-111`) says a restored image counts as fenced only when "every record a fenced session wrote has a named deleter". The completion test, `sessions_settled` (`crates/custodian/src/restore.rs:409`), only looks at #842's segment check. For an `Aborting` session, `recheck_fenced` (`crates/custodian/src/restore.rs:1163-1168`) checks the `retire:records` obligation and the `seg:` range, and never looks at the `retire:bytes` obligation that owes the session's `part:`/`sidx:` records.
- [x] **A pass counter does not identify the current restore.** `brief.md:24–29` assumes an absent initial marker and says advancing it on a second pass means “a later restore invalidates an earlier completion.” The backup covers the **whole database** (`docs/design/architecture/m4-first-deployment-blueprint.md:563`), so it can restore an old complete marker alongside sessions created after that marker. Before the new pass starts, that marker still reads complete; two consecutive passes do not test this case. Revise the claim and the documented #508 contract to specify how completion is associated with this restore, and add a restored-old-marker scenario. Retaining mandatory deployment ordering is a valid scope choice; the marker alone does not establish that ordering.
- [x] **“A failed pass never reads complete” exceeds the specified failure model.** Leg Q (`brief.md:48–49`) injects only a mid-pass commit failure. `MetadataStore` explicitly permits an `Err` after a write lands (`crates/traits/src/lib.rs:1301–1312`), even a transaction that lands later (`:240–246`). Thus the final completion write can become durable despite an error; conversely, failure of the initial invalidation can leave the previous complete marker untouched. Define the expected generation and distinguish unfinished work from an unknown commit outcome. Require boundary cases for initial invalidation and final publication, including applied-but-unacknowledged commits, rather than treating every error as proof of non-completion.
- [x] **The residue regression omits a crash during the pass.** N/P restart only after M finishes; Q checks the failed pass's marker without restarting it (`brief.md:31–49`). Because P explicitly permits residue in the generation record, saving that residue only at the end could pass these tests yet lose the finding if execution stops after `Completing → Aborting` and before the residue save. The target protocol requires the fence and its reclamation obligations in one batch precisely to prevent a session with no reclaimer (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:665`). Add interruption immediately after a durable fence and during generation rollover, followed by a fresh-context retry that must retain the blocker and eventually install the repaired obligation. This constrains crash behavior without choosing where residue lives.
- [x] **The completion predicate adds a broader availability policy.** The stated invariant concerns resurrected sessions and their deleters (`brief.md:57–59`), but scope requires “nothing needing a human” (`:67–69`). Existing `needs_human()` includes unrelated committed-file `dangling` and `misplaced` findings (`crates/custodian/src/restore.rs:212–216`); dangling files can be irrecoverably lost (`:150–153`). A fully fenced image could therefore never complete while an unrelated lost-file map remains, making the future #508 gate withhold all multipart service. M tests only session residue. Either narrow completion blockers to the intended fencing invariant, or explicitly include this broader policy, its operator remedy, and a test with fully fenced sessions plus an unrelated dangling file.
- [x] size backstop — this slice is behaving oversized: patch is 111 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] **The runbook and the CLI now tell the operator different things for the same state.** `docs/design/architecture/m4-first-deployment-blueprint.md:653` (new) says "re-enable no gateway (step 8) until a run of this step has said COMPLETE". Step 8 (`:664`) is "Resume writers", so this blocks **all** writes. Eleven lines up, `:644` still says "Serve no multipart uploads until a run has fenced every upload". The CLI's NOT-complete line (`crates/server/src/cli.rs:1474`) says only "Keep multipart uploads off this store". Concrete case: an #842 H(i) record (`Completing`, no nonce) waiting on a manual repair. The runbook keeps the whole cluster write-down until someone fixes it. The CLI implies only multipart is affected. The brief's own M-scope reasoning called withholding service over a finding "an availability policy 0016 does not state". A human should pick the intended rule (all writers held vs. multipart only, given no per-verb switch exists before #508), and the other two texts should then be aligned to it.

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (#810): extend the fenced-session re-check (`crates/custodian/src/restore.rs:1163-1168`) to the `retire:bytes` obligation for `part:`/`sidx:` records, so an `Aborting` session missing it blocks "complete" (today it leaks silently).
- Follow-up (#810): rule is 1a — no writers (step 8) until a run says COMPLETE. Align the CLI NOT-complete text (`crates/server/src/cli.rs`, "Keep multipart uploads off this store") and the older runbook line (`m4-first-deployment-blueprint.md:644`) to it. Revisit in #508, which can switch off only multipart.
