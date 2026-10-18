# Result — issue 895 / multipart-abort-fence

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Nothing but the post-restore pass fences a multipart session today. Restore's
  `fence_session` builds its fence batch inline (`crates/custodian/src/restore.rs:1045-1057`).
  This slice adds `AbortMultipartUpload`'s teardown fence as a `wyrd-core` function over
  `MetadataStore`, answering in `AbortOutcome` (`crates/core/src/multipart.rs:4440`) and
  `Refusal` (`:4302`). It also moves the fence **batch** construction into one core builder that
  both the client Abort and restore use. That is the "one batch builder, not two" #656's first
  tracker comment (2026-09-12) asks for.
- Success criterion: A NEW test file `crates/core/tests/multipart_abort.rs`, over
  `RedbMetadataStore::in_memory()`, shows all of the following. Every `Open` session in it is
  made by the sibling slice's Create. A `Completing` or `Completed` session is planted as
  canonical bytes the decoder accepts.
  **(D) Abort per decision 3 (`0016:977`).**
  - On `Open@E` it answers `Fenced`. Afterwards the session decodes as `Aborting@E+1`, and
    `retire:bytes:s:<id>:<E>` holds exactly the `{session, all}` obligation
    `SessionRecord::open_teardown` mints (`multipart.rs:2360-2372`), committed in the **same**
    batch as the fence.
  - A second Abort answers `AlreadyAborting`.
  - **Two Aborts forced to race one `Open` session** through a test-local wrapper answer one
    `Fenced` and one `AlreadyAborting`, never `OperationAborted`.
  - Abort of a `Completing` session answers `Refused(OperationAborted)` and leaves the store
    byte-identical.
  - Abort of a `Completed` session, or of an upload id with no record, answers
    `Refused(NoSuchUpload)`.
  - An unknown outcome on the **last** round whose batch landed answers `Fenced`, and a rival
    Abort that wins on the last round answers `AlreadyAborting`: the final round's re-read is
    answered like any other.
  - A plain (not unknown) store fault on a fence commit, with nothing in doubt, is returned as
    that fault.
  **(E) Abort never moves the counter.** `mpuctl.count()` is identical before and after every
  leg of this file, and a `Completed` tombstone stays counted. The decrement belongs only to the
  terminal delete (#659).
  **(H) A lost commit is classified from what the store holds, not inferred.** Through a store
  wrapper that can answer `Ok(CommitOutcome::Conflict)` **without applying anything** (what
  TiKV's adapter returns when a locking `get_for_update` fails on another writer's lock,
  `crates/metadata-tikv/src/lib.rs:1388-1393`, `:928-939`):
  - **H1:** one such Conflict, with the session unchanged and the obligation key absent. Abort
    does **not** report the obligation key taken. It spends a round and answers `Fenced`.
  - **H2:** round 1 returns `CommitUnknownResult { may_still_commit: true }` with its batch held,
    and every later round gets such a Conflict until the rounds are spent. Abort ends with an
    error that still says `may_still_commit: true`. Released **after** Abort has returned, the
    held batch lands, and the session is `Aborting` with its one obligation.
  - **H3:** with the obligation key really present (planted) and the session unchanged, Abort
    ends with a typed fault naming that key and writes nothing. The session is still unchanged
    when Abort re-reads it after finding the key, so this stays damage.
  - **H4, a rival fence between the two reads is not damage:** Abort A gets such a Conflict and
    re-reads the session (`Open@E`, unchanged). The wrapper then lets Abort B commit its fence
    (session `Aborting@E+1` and the obligation, one batch) **before** A reads the obligation key.
    A finds the key present. A must answer `AlreadyAborting`, not a damage fault: B's batch is
    atomic, so a present key with a session that has **moved** is a valid transition. Each `get`
    is its own transaction (`crates/metadata-redb/src/lib.rs:113-116`;
    `crates/metadata-fdb/src/lib.rs:1794-1800`), so nothing stops B landing between A's reads.
  **(I) The doubt of a fence that may still land is kept.** Once a fence returns
  `CommitUnknownResult { may_still_commit: true }`, Abort keeps that doubt across a later
  unknown with `may_still_commit: false`, a failed re-read, and a later plain store fault, for
  as long as the session is unchanged. Each case releases the held first batch **after** Abort
  has returned. It asserts that the returned error still says `may_still_commit: true` and that
  the batch lands. Only a read that finds the session **moved** clears the doubt. A session moved
  under a held fence ends without that doubt, and the released batch loses.
  **(I2) An unsettled out-of-flight outcome stays unknown.** With **no** earlier
  `may_still_commit: true`: the wrapper applies the fence batch, returns
  `CommitUnknownResult { may_still_commit: false }`, and then fails every read of `mpu:<id>`
  until Abort returns. The fence has landed, but Abort never learned it. Abort must end with an
  error of the unknown-outcome class (`ErrorClass::Indeterminate`,
  `crates/traits/src/lib.rs:676-699`) naming the upload id: not the read's plain fault (which
  classifies `Transient` or `Terminal`), and not the "no fence of this Abort can land" error.
  `may_still_commit: false` means only that the batch is out of flight; it still needs a
  successful read to say whether it landed (`crates/traits/src/lib.rs:240-246`). Assert the
  returned error's class, that it names the id, and that the store holds the session
  `Aborting@E+1` with its one obligation.
  **(J) Restore is unchanged.** `fence_session` uses the shared builder and puts the same
  preconditions and records in the same order. Restore's existing tests pass unmodified: the
  `wyrd-custodian` tests under `cargo test`, and the custodian DST file under `cargo xtask dst`.
  **(G) Seeded coverage** (`AGENTS.md:188-190`). A NEW madsim scenario
  `crates/dst/tests/multipart_abort.rs` runs concurrent Creates (the sibling's) and concurrent
  Aborts, including two Aborts of one session, on `SimFdbMetadataStore` with the commit-ambiguity
  nemesis armed (1021, 1031, and a mixed leg drawing each commit's code from a seed-derived
  coin). The nemesis strikes often enough that some Aborts spend all their rounds. Fault
  histories are tracked **per Abort invocation**, never keyed by session and operation, which
  would merge two concurrent Aborts of one session. After every seed, on the store after
  `quiesce`:
  - the number of `mpu:` records equals `mpuctl.count()`;
  - every `Aborting` session has its `retire:bytes:` `{session, all}` obligation;
  - a session none of whose Aborts answered `Fenced` or `AlreadyAborting`, and none of whose
    Aborts ended with an error saying a fence may still land, is still `Open`; one an Abort
    answered `Fenced` for is `Aborting`.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `AbortMultipartUpload`'s fence as a store-I/O function in `wyrd-core` answering in
  the landed vocabulary, including how it classifies a lost or unknown commit. Also in scope:
  - the shared fence batch builder in `wyrd-core`;
  - the one behaviour-preserving change to restore's `fence_session`
    (`crates/custodian/src/restore.rs:1045-1057`) to use it;
  - the seeded DST scenario `crates/dst/tests/multipart_abort.rs`;
  - the module header and living-architecture sentence for Abort.
  / out of scope:
  - Create and admission (the sibling slice #894, 636.3a);
  - restore's own conflict classification (`restore.rs:1067-1083`): only its batch construction
    is shared;
  - the `Completing → Aborting` edge, which a client Abort never takes (the table answers `409`);
  - the reaper and operator abort, including their `Completing → Aborting` rows (#625, #633);
  - the retirement drain and terminal delete (#659);
  - every gateway, S3, CLI or custodian **caller** of Abort (#668);
  - any edit to proposal 0016 or to any ADR;
  - `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 22 mutants tested in 2m: 18 caught, 4 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_895/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.20s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #895’s multipart Abort fence and shared restore batch; the requested doubt-clearing regressions and race-documentation correction are present.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The agreed core-only scope and state-dependent answers match the normative Abort contract; the iteration’s two tests and documentation qualification are explicit (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:977`; `brief.md:329`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API, so there is no executable pre-fix behavioral discriminator: independently reverting production leaves an unresolved Abort import before any test runs (`brief.md:157`; `reviewer-red-green.log:177`). |
| C3 Change | PASS | Atomic fencing retains a reclamation obligation without releasing admission capacity; the forced-race and store-state assertions exercise that contract (`crates/core/src/multipart/abort.rs:83`; `crates/core/tests/multipart_abort.rs:590`; `crates/core/tests/multipart_abort.rs:686`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API verification posture and unmeasured diff coverage — restored-code tests and mutation reds pass, but the pre-fix test cannot execute and the coverage gate used an incompatible origin/main baseline (`reviewer-red-green.log:177`; `reviewer-red-green.log:235`; `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | Both obsolete-doubt cases now have discriminating tests: disabling settlement fails exactly those two tests, while the real implementation passes; no capability probe masks a load-time cause (`crates/core/tests/multipart_abort.rs:1166`; `crates/core/tests/multipart_abort.rs:1233`; `reviewer-mutants-capped.log:4`). |
| T1 Structure | PASS | The MetadataStore seam preserves dependency direction, and both fence writers use one builder whose precondition/put ordering is pinned (`crates/core/src/multipart/abort.rs:77`; `crates/custodian/src/restore.rs:1055`; `crates/core/tests/multipart_abort.rs:1271`). |
| T2 Shape | PASS | Persisted records retain canonical decoding and typed obligation construction; the living architecture now qualifies the racing-Abort answer when commit outcomes are unknown (`crates/core/src/multipart/abort.rs:302`; `crates/core/src/multipart.rs:3913`; `docs/design/architecture/05-building-block-view.md:206`). |
| T3 Runtime | PASS | Bounded rounds use the backend termination contract without adding clocks or shared mutable globals; independent seeded runs exercise late landings and concurrent Aborts (`crates/core/src/multipart/abort.rs:273`; `crates/traits/src/lib.rs:1334`; `crates/dst/tests/multipart_abort.rs:551`; `reviewer-dst.log:248`). |
| T4 Contribution | N/A | Contribution text is absent by design at Check; its substantive audit remains mandatory at publish, rather than a missing human action (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Prior art was checked by all six affected paths against merged history and closed/rejected work; the accepted production policy and tracked follow-ons remain settled (`reviewer-prior-art.log:1`; `brief.md:203`; `brief.md:329`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the exercised atomic-fence and uncertainty contracts meet the intended core-only use — gateway behavior and final reclamation are agreed later slices, so this evidence establishes the fence boundary rather than an end-to-end multipart lifecycle (`brief.md:180`; `crates/core/src/multipart/abort.rs:26`). |

The remaining decisions concern verification posture and fitness, not an identified implementation defect. Source citations above resolve against the supplied `$PDCA_TARGET`; brief and log citations refer to this review bundle. The target is usable and matches the patch: reverse-apply checking succeeds, and all six affected files retain their reviewed bytes. The coverage gate’s failure against origin/main is a base-state caveat for this dependent slice, not evidence that the supplied patch fails to apply or compile.

Independent verification supports the requested iteration:

- **Executable behavior:** `cargo test -p wyrd-core --test multipart_abort` passed all 23 tests. Stashing the four production/doc paths while retaining the tests reproduced the missing-API compile failure; popping the stash restored identical production bytes and all 23 tests passed again (`reviewer-red-green.log:204`). `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test multipart_abort` passed all six tests, including the deliberate bad-adapter negative control (`reviewer-dst.log:248`).
- **Regression strength:** an independent cargo-mutants run, with lint capping so mutants reach tests, caught both mutations of `Doubt::settle` (`reviewer-mutants-capped.log:6`). The no-op mutation fails exactly the two requested tests, reporting retained `InDoubt` instead of `Stopped`/`Transient` (`pdca-reviewer-895-scratch/mutation-capped/mutants.out/log/crates__core__src__multipart__abort.rs_line_423_col_9.log:502`). The frozen full-diff run separately reports 18 caught and four unviable mutants (`gate-logs/C5-mutants.log:13`).
- **Repository checks:** the independent `cargo xtask ci` run passed spelling, docs lint/render/link audit, formatting, clippy, builds, tests, conformance, statics, deployment guards, and DST, including all 33 custodian simulations (`reviewer-ci.log:4040`). Its exit was nonzero solely because two advisory checks could not lock the sandbox’s read-only Cargo database (`reviewer-ci.log:3483`; `reviewer-ci.log:4111`). Both were rerun successfully with only the database location moved into writable scratch; parsed policy was otherwise identical (`reviewer-deny.log:2`; `reviewer-deny.log:6`). Thus the brief’s `typos` and docs-renderer dependencies were exercised, and the local database fault is resolved. The frozen complete CI run also passed (`gate-logs/C4-ci.log:4215`).
- **Other gate evidence:** both TiKV and server feature compilations passed independently (`reviewer-tikv.log:202`), matching `gate-logs/host-tikv.log`. The instance-scoped batch-review wrapper was adjudicated from its frozen log, which reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). The contribution audit remains deferred to publish; no missing-oracle finding is raised.

Prior-art investigation found the expected merged restore fences and no merged history for the new Abort module or either Abort test file. All 388 closed PRs were enumerated and all 19 unmerged PRs checked by affected path. The sole rejected-work path overlap was PR #647’s segmented-map restore changes, a different subject; the rejected combined Create/Abort attempts and their conflict-classification lesson are recorded in the brief (`reviewer-prior-art.log:1`; `reviewer-prior-art.log:9`; `brief.md:209`). No settled deferral or accepted retry-policy decision is reopened.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #895 (multipart Abort fence + shared fence batch), iteration 2

I copied the target into scratch, ran `cargo test -p wyrd-core --test multipart_abort` (23/23 green, same as the gate),
then ran 11 hand-made mutants of `crates/core/src/multipart/abort.rs` against that suite. 8 were killed. 3 survived:
one is a real gap (first bullet); the other two are harmless (see the last bullet).

- NEEDS-HUMAN [impl] — **No test checks that a held "may still land" doubt survives a failed read inside the Conflict branch** (`crates/core/src/multipart/abort.rs:337`). I swapped `doubt.end(upload_id, Stop::Store(fault))` on that arm for a plain `AbortError::Stopped { .. }`. All 23 core tests still pass. The DST cannot catch it either: the simulated FDB `get` never fails (`crates/dst/tests/support/mod.rs:871-877`). cargo-mutants does not generate this mutation, so C5's "18 caught, 0 missed" does not cover it. The mutant breaks C-1: on TiKV (lock conflict, then a read timeout) Abort would say "no fence of this abort is in doubt" while its first fence is still in flight. The brief's Design keeps the doubt "across … a failed re-read … and unexplained conflicts". H2 (`crates/core/tests/multipart_abort.rs:967`) covers conflicts with reads that succeed, and `i_a_held_fence_survives_a_failed_re_read` (`:1090`) covers a failed read after an *unknown* commit. Nothing covers the two together. A test that fixes it, using the existing wrapper: `store.script(vec![OnCommit::Hold, OnCommit::ConflictUnmoved], None); store.state().fail_session_reads_after = Some(2);` then `in_doubt(outcome, &id, true)`, `doubt.cause` is `Stop::Store(_)`, `release_held() == Committed`, `assert_fenced`, count unchanged. I ran it: it passes on the patch and fails on the mutant ("expected InDoubt …, got Err(Stopped { … Store(\"the test wrapper's store failed\") })"). This is a test-only change, so it fits this round's "tests + doc only" rule.

- NEEDS-HUMAN [impl] — **The new race sentence at `docs/design/architecture/05-building-block-view.md:206` still promises more than the code does on TiKV.** It says two racing Aborts "answer one fenced and one already aborting when the store reports both commits' outcomes". On TiKV, loser B's `Conflict` from winner A's lock *is* a reported outcome. But A has not committed yet, so B's reads find the session unchanged and the key absent. B spends the round and retries at once (`abort.rs:332`). With `ABORT_ROUNDS` = 4 (`MAX_COMPLETE_ATTEMPTS` = 3, `crates/core/src/multipart.rs:5200`), B can use up all four rounds while A holds its locks, and end `Stopped { RoundsSpent }` while A answers `Fenced`. The behaviour is fine: nothing is in doubt, and the human cleared the no-backoff retry. Only the sentence is wrong. Suggested fix: add "…and neither runs out of rounds on a lock it cannot see", or limit the claim to stores whose `Conflict` means a precondition moved. Low severity.

- NEEDS-HUMAN [human] — **The T4-batch-review row may not reflect a real review of this round's diff** (`check-gates.json`, `T4-batch-review`). It reports a "3x codex, union, triaged" review with 0 blocking, 0 recorded-rejected and 0 noise-dropped in 14.74 s, over a 2,566-line `patch.diff`. That is too fast for three real passes, and a result with no items at all suggests a cached run from the previous round's diff (this round changed the tests and the doc). Before counting it as the rubric's "one deep, multi-pass review", check that `results/issue_895/review-batch.md` was generated from this round's diff. I did not open it: it is outside my allowed inputs.

- Note, not a refutation: **C4-diff-cov was not measured** ("patch.diff does not apply on origin/main"). That is expected for a slice stacked on #894. But the brief also lists a conflict with #768 on the same doc paragraph, so main may have moved under this bundle. Rebase before publishing. My mutant run partly stands in for the missing coverage evidence.

- **Attempted to refute these; could not:**
  - *Carry-forward items 1–2 are real discriminators.* With `Doubt::settle` as a no-op, both new tests fail. Removing only the held-clearing branch (`abort.rs:422-431`) fails `i_a_session_moved_to_a_later_open_epoch_clears_the_held_doubt`. Removing only `self.unsettled = None` fails `i2_an_out_of_flight_outcome_a_read_settled_is_not_kept`. They fail on the asserted class, not on something incidental.
  - *Production path.* Every leg calls `abort_session` / `fence_batch` over `RedbMetadataStore`. No test re-implements the logic.
  - *The brief's demonstrated reds, re-run by me.* Skipping the obligation-key read (attempt 4's rule) fails H1, H2 and three others. Skipping the second session read fails H4. Returning the unsettled outcome before the held one in `end` fails (I). Clearing the doubt on an unexplained conflict fails H2. Dropping the doubt on a plain commit fault (`abort.rs:343`) fails (I).
  - *Over-reporting doubt.* `unsettled` is only set between `observe` and the read right after it (`abort.rs:345-348`). Every successful read clears it at the top of the loop, so a later Conflict-branch failure never reports a stale out-of-flight doubt.
  - *Two surviving mutants, judged harmless.* (a) Letting `observe` replace the held outcome (`abort.rs:409`): the held entry's required bytes are always the current read, so the result is the same. (b) Dropping the doubt on `ObligationKeyTaken` (`abort.rs:335`): the held batch `require_absent`s that same key, so it cannot land while the key is there. The current code reports the doubt anyway, which is the safer choice. No test pins it, but I don't think it is worth a round.
  - *(J) restore refactor* (`crates/custodian/src/restore.rs:1055`): same preconditions and puts in the same order. Restore's tests are unchanged and pass in C4-ci, as does the custodian DST under `cargo test -p wyrd-dst --cfg madsim`. C4-ci now gets through the deny step and runs the new DST file (6/6).
  - *C4-verify UNVERIFIABLE* was declared in advance by the brief (net-new API; the test cannot compile on the reverted base). It is not evidence against the fix.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- No correctness findings. The conflict/read races and unknown outcomes are handled consistently with the store contract (`crates/core/src/multipart/abort.rs:282`). Both added doubt-clearing regressions force the relevant transition and assert `Stopped` / `Transient`, making retained doubt observable (`crates/core/tests/multipart_abort.rs:1166`, `crates/core/tests/multipart_abort.rs:1233`).
- No actionable reuse, simplification, or efficiency findings. The shared fence builder preserves restore's preconditions and mutation order (`crates/core/src/multipart/abort.rs:77`, `crates/custodian/src/restore.rs:1055`).

Validation: inspected target source and frozen gate evidence; no tests rerun. CI passed, including 23 core Abort tests and 6 DST tests. Mutation testing caught 18 mutants; 4 were unviable. The predeclared pre-fix run could not compile without the new API, and diff coverage was not measured because the patch did not apply to origin/main.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared new-API verification posture and unmeasured diff coverage — restored-code tests and mutation reds pass, but the pre-fix test cannot execute and the coverage gate used an incompatible origin/main baseline (`reviewer-red-green.log:177`; `reviewer-red-green.log:235`; `gate-logs/C4-diff-cov.log:10`).
- [ ] Validation — fitness-to-purpose — Confirm that the exercised atomic-fence and uncertainty contracts meet the intended core-only use — gateway behavior and final reclamation are agreed later slices, so this evidence establishes the fence boundary rather than an end-to-end multipart lifecycle (`brief.md:180`; `crates/core/src/multipart/abort.rs:26`).
- [ ] **No test checks that a held "may still land" doubt survives a failed read inside the Conflict branch** (`crates/core/src/multipart/abort.rs:337`). I swapped `doubt.end(upload_id, Stop::Store(fault))` on that arm for a plain `AbortError::Stopped { .. }`. All 23 core tests still pass. The DST cannot catch it either: the simulated FDB `get` never fails (`crates/dst/tests/support/mod.rs:871-877`). cargo-mutants does not generate this mutation, so C5's "18 caught, 0 missed" does not cover it. The mutant breaks C-1: on TiKV (lock conflict, then a read timeout) Abort would say "no fence of this abort is in doubt" while its first fence is still in flight. The brief's Design keeps the doubt "across … a failed re-read … and unexplained conflicts". H2 (`crates/core/tests/multipart_abort.rs:967`) covers conflicts with reads that succeed, and `i_a_held_fence_survives_a_failed_re_read` (`:1090`) covers a failed read after an *unknown* commit. Nothing covers the two together. A test that fixes it, using the existing wrapper: `store.script(vec![OnCommit::Hold, OnCommit::ConflictUnmoved], None); store.state().fail_session_reads_after = Some(2);` then `in_doubt(outcome, &id, true)`, `doubt.cause` is `Stop::Store(_)`, `release_held() == Committed`, `assert_fenced`, count unchanged. I ran it: it passes on the patch and fails on the mutant ("expected InDoubt …, got Err(Stopped { … Store(\"the test wrapper's store failed\") })"). This is a test-only change, so it fits this round's "tests + doc only" rule.
- [ ] **The new race sentence at `docs/design/architecture/05-building-block-view.md:206` still promises more than the code does on TiKV.** It says two racing Aborts "answer one fenced and one already aborting when the store reports both commits' outcomes". On TiKV, loser B's `Conflict` from winner A's lock *is* a reported outcome. But A has not committed yet, so B's reads find the session unchanged and the key absent. B spends the round and retries at once (`abort.rs:332`). With `ABORT_ROUNDS` = 4 (`MAX_COMPLETE_ATTEMPTS` = 3, `crates/core/src/multipart.rs:5200`), B can use up all four rounds while A holds its locks, and end `Stopped { RoundsSpent }` while A answers `Fenced`. The behaviour is fine: nothing is in doubt, and the human cleared the no-backoff retry. Only the sentence is wrong. Suggested fix: add "…and neither runs out of rounds on a lock it cannot see", or limit the claim to stores whose `Conflict` means a precondition moved. Low severity.
- [ ] **The T4-batch-review row may not reflect a real review of this round's diff** (`check-gates.json`, `T4-batch-review`). It reports a "3x codex, union, triaged" review with 0 blocking, 0 recorded-rejected and 0 noise-dropped in 14.74 s, over a 2,566-line `patch.diff`. That is too fast for three real passes, and a result with no items at all suggests a cached run from the previous round's diff (this round changed the tests and the doc). Before counting it as the rubric's "one deep, multi-pass review", check that `results/issue_895/review-batch.md` was generated from this round's diff. I did not open it: it is outside my allowed inputs.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The tracker-derived constraints cannot be checked from the supplied record.** `brief.md:19-21` attributes the shared batch requirement to #656's first comment; `brief.md:109-110` and `:149-154` treat intake and verification exceptions as human rulings. Neither `notes.json` nor `sources/` exists in this sandbox. The source supports the missing Abort writer and inline restore batch, but cannot substantiate those tracker claims or reveal omitted constraints. Include the relevant thread excerpts and ruling references in the revised brief, or explicitly mark them unverified; this review cannot confirm them.
- [ ] **Reading the obligation after the session does not establish a damaged key.** `brief.md:206-209` prescribes a typed damage fault when the session re-read is unchanged and the subsequent obligation read finds a key. Counterexample: A receives an unexplained lock Conflict, reads `Open@E`, then B atomically commits the fence and obligation before A reads the obligation. A reports damage even though B performed the valid transition; this contradicts the two-Abort result required at `brief.md:32-33`. Each `get` opens a separate transaction (`crates/metadata-redb/src/lib.rs:113-116`; `crates/metadata-fdb/src/lib.rs:1794-1800`), and the cited TiKV lock-conflict path permits the initial unexplained Conflict (`crates/metadata-tikv/src/lib.rs:1386-1393`). Revise the classification to revalidate the session after observing a present obligation before declaring damage, and add a deterministic leg placing B's commit between those reads. The planted-key H3 case must still report damage when that revalidation confirms the unchanged session. Restore's existing classification can remain outside this slice.
- [ ] **The error contract omits an unresolved outcome that is already out of flight.** `brief.md:212-218` retains `CommitUnknownResult` only when a fence may still land, otherwise prescribing an error saying no fence can land; the failed-read coverage at `:58-64` likewise starts with `may_still_commit: true`. But `false` means only that the transaction is out of flight, not that it failed: a successful re-read is still needed to determine its outcome (`crates/traits/src/lib.rs:240-246`). Force a batch to land, return `CommitUnknownResult { may_still_commit: false }`, then fail the re-read. No future fence can land, yet this fence is already durable and its outcome remains unknown to Abort. Specify that this path preserves the indeterminate outcome rather than degrading to an ordinary read/terminal fault, and add a deterministic regression without a preceding `true` outcome. The seam explicitly distinguishes indeterminate from terminal failures (`crates/traits/src/lib.rs:676-699`).
- [ ] size backstop — this slice is behaving oversized: patch is 129 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Production code is accepted as is; this round is ONE test only. Do not change abort.rs behaviour, the round budget, or the no-backoff retry. 1. Add a core test in crates/core/tests/multipart_abort.rs pinning that a held "may still land" doubt survives a failed re-read inside the Conflict branch (abort.rs:337). Nothing covers that combination today: H2 (:967) has conflicts with successful reads, i_a_held_fence_survives_a_failed_re_read (:1090) has a failed read after an unknown commit. The adversary's mutant (replacing `doubt.end(upload_id, Stop::Store(fault))` on that arm with a plain AbortError::Stopped) passes all 23 tests. Suggested shape with the existing wrapper: store.script(vec![OnCommit::Hold, OnCommit::ConflictUnmoved], None); store.state().fail_session_reads_after = Some(2); then assert in_doubt(outcome, &id, true), doubt.cause is Stop::Store(_), release_held() == Committed, assert_fenced, mpuctl count unchanged. It must pass on the patch and fail on that mutant.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
