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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.41s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #895: add atomic multipart Abort fencing and a shared restore fence builder, including the requested regression for uncertainty surviving a failed conflict re-read.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The state answers, atomic reclamation obligation, unchanged admission count, and current regression scope are explicit and consistent with the protocol (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:665`, `:977`; `brief.md:341`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared absence of a behavioral pre-fix red for this new API — stashing production while retaining its test produces an unresolved `multipart::abort` import, not an executed failing test (`reviewer-evidence/base-red.log:4`; `brief.md:157`). |
| C3 Change | PASS | The requested regression forces a held commit, an unapplied conflict, and a failed session read; it checks the reported uncertainty, subsequent durable fence, obligation, and unchanged count (`crates/core/tests/multipart_abort.rs:1112`). |
| C4 Verification (red→green) | PASS | Independently reproduced the requested mutant's assertion failure and restored 24/24 core tests plus 6/6 seeded DST tests; CI and coverage qualifications are recorded below (`reviewer-evidence/conflict-read-mutant-red.log:14`, `reviewer-evidence/core-restored-green.log:32`, `reviewer-evidence/dst-abort.log:248`). |
| C5 Causal adequacy | PASS | The regression detects the precise loss of an earlier in-flight commit's uncertainty at the conflict-read error arm; releasing that commit after return proves the consequence, and no optional-capability/load-time guard is introduced (`crates/core/src/multipart/abort.rs:337`; `crates/core/tests/multipart_abort.rs:1118`, `:1138`). |
| T1 Structure | PASS | The existing `MetadataStore` seam and shared batch builder preserve dependency direction and one atomic fence construction for both writers (`crates/core/src/multipart/abort.rs:77`, `:282`; `crates/custodian/src/restore.rs:1055`). |
| T2 Shape | PASS | Exact session-byte preconditions, absent obligation keys, mutation order, and no admission decrement are asserted directly, including restore's two-obligation case (`crates/core/tests/multipart_abort.rs:606`, `:1311`). |
| T3 Runtime | PASS | Work remains bounded by the accepted round budget and backend termination contract, with no new clock source or shared mutable static; seeded tests exercise late landings and per-invocation ambiguity histories (`crates/core/src/multipart/abort.rs:273`; `crates/traits/src/lib.rs:1334`; `crates/dst/tests/multipart_abort.rs:263`, `:388`). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design and their substantive audit must rerun at publish; the affected-path prior-art check was independently repeated (`gate-logs/T4-contribution.log:10`; `reviewer-evidence/prior-art.log:1`, `:72`). |
| T5 Judgment | PASS | The requested caller-visible uncertainty regression is effective; the accepted retry choices and tracked scope deferrals need no new implementation decision (`brief.md:341`; `crates/core/tests/multipart_abort.rs:1112`; `docs/design/architecture/05-building-block-view.md:206`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the observed fencing and late-commit evidence fulfills this slice's reclamation-safety purpose — executable checks support C-1, while acceptance of the intended operational outcome remains human judgment (`brief.md:112`; `crates/core/tests/multipart_abort.rs:1137`; `crates/dst/tests/multipart_abort.rs:409`). |

No new implementation defect was found. The requested regression rejects the exact adversarial change, the restored patch passes its direct tests, and the remaining qualifications concern evidence scope and human validation.

Source paths above are relative to `$PDCA_TARGET`; `brief.md`, gate logs, and reviewer evidence are relative to this report's directory. The supplied target compiled successfully and passed a reverse-application check against `patch.diff`. Stashing and the temporary mutation were confined to that disposable target; all six patched files were restored byte-identically (`reviewer-evidence/source-integrity.log:1`). No builder notes were read.

- **Regression evidence:** `cargo test --offline --locked -p wyrd-core --test multipart_abort` passes all 24 tests. Temporarily replacing only the error return at `crates/core/src/multipart/abort.rs:337` with `AbortError::Stopped` makes `i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict` execute and fail: expected `InDoubt`, received `Stopped`. Restoring the original source makes the complete file green. This establishes the current round's behavioral discrimination, separately from the new API's noncompiling base comparison.
- **Simulation evidence:** `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50 cargo test --offline --locked -p wyrd-dst --test multipart_abort` passes six tests, including the expected-panic negative control. The plain tests additionally sweep 64 seeds for each ambiguity leg (`crates/dst/tests/multipart_abort.rs:92`, `:526`; `reviewer-evidence/dst-abort.log:240`).
- **CI evidence:** An independent `cargo xtask ci` run completed formatting, clippy, build, workspace tests, dependency-use checks, conformance, statics, deployment guards, and DST; the existing custodian DST suite passed 33 tests (`reviewer-evidence/ci.log:4140`). Its aggregate exit was **1 solely because two cargo-deny steps could not lock the read-only home advisory database** (`reviewer-evidence/ci.log:3583`, `:4211`). Both real cargo-deny commands then passed using a writable copy of that database; parsed policies were identical except for `advisories.db-path`, and target policy files were untouched (`reviewer-evidence/deny-local-db.log:1`). The frozen CI run is green too (`gate-logs/C4-ci.log:4216`). Both declared external tools were exercised: spelling, documentation lint, rendering of 99 pages, and link audit passed (`reviewer-evidence/ci.log:2`). No unsatisfied tool dependency or patch defect is inferred from the resolved lock restriction.
- **Frozen gates and coverage:** The instance-scoped wrapper results were adjudicated from their supplied logs. C5 reports 18 caught and four unviable mutants, with no survivors (`gate-logs/C5-mutants.log:13`). The batched review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`); the TiKV/server feature compile completes successfully (`gate-logs/host-tikv.log:211`). C4-verify records the same missing-API base comparison reproduced here (`gate-logs/C4-verify.log:23`). Diff coverage was **not measured** because its `origin/main` base lacks this stacked patch's prerequisites (`gate-logs/C4-diff-cov.log:10`; `brief.md:125`). This is a base-ordering caveat, not evidence that the supplied patch fails to apply or compile. The deferred contribution audit is N/A until its mandatory publish rerun.
- **Prior art:** Queried merged history separately for all six affected paths, inspected affected-file intersections for all 32 closed multipart PR search results, and searched closed Abort/admission work. The new Abort module and both test files have no `main` history; the existing restore work matches #851/#856. The rejected combined approach is recorded in the brief; #664 is completed and #636 is closed not-planned (`reviewer-evidence/prior-art.log:11`, `:72`, `:93`; `brief.md:203`). The accepted round-budget/no-backoff decision and tracked deferrals remain settled.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #895 (636.3b), iteration 3

Scope of this round: the sign-off allowed one new test, pinning that a held "may still land"
doubt survives a failed read inside the Conflict branch. I tried to refute that test, the rest of
the bundle's production path, and the gate verdicts. I re-ran the evidence myself: I copied the
target into scratch, ran a baseline (24/24 pass), then ran 23 hand-written mutants of
`crates/core/src/multipart/abort.rs` against `crates/core/tests/multipart_abort.rs`.

## Findings

- **Could not refute this round's test.** `crates/core/tests/multipart_abort.rs:1112`
  (`i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict`) goes red on the
  sign-off's mutant and green on the patch. With `crates/core/src/multipart/abort.rs:337`
  changed from `doubt.end(upload_id, Stop::Store(fault))` to a plain `AbortError::Stopped`,
  **only** that test fails (23 pass, 1 fail). It fails at the `in_doubt` helper (`:333`) with
  `expected InDoubt naming …0001, got Err(Stopped { …, cause: Store("the test wrapper's store failed") })`.
  It drives the real `abort_session` over real redb. The only thing scripted is the commit
  answer (`Hold`, then `ConflictUnmoved`) and the `mpu:` read failing after two commits. So the
  failing read can only be `lost()`'s first session read (`abort.rs:376`): no reads come after a
  Conflict except inside `lost()`. The test is not a copy of production, and it does not pass by
  accident. It also checks that the held batch released afterwards lands (`:1138-1139`).

- **One doubt-keeping arm is still unpinned (informational; production is correct).** Changing
  `abort.rs:335` from `doubt.end(upload_id, Stop::ObligationKeyTaken { key })` to a plain
  `AbortError::Stopped { cause: ObligationKeyTaken }` passes all 24 tests. H3 (`:991`) covers
  "taken key" only when nothing is in doubt. The case that tells them apart:
  1. Round 1 is `Hold` on `Open@0`.
  2. The re-read finds the session unchanged.
  3. A squatter is planted at `retire:bytes:s:<id>:0`.
  4. Round 2 gets a real Conflict, and `lost()` returns `Taken`.

  Production answers `InDoubt { may_still_commit: true, cause: ObligationKeyTaken }`. The mutant
  answers "none landed, and none can land later". That is false only if the squatter is later
  removed and the held batch then lands. The held batch `require_absent`s that key, so the
  real-world risk is small. This is the same kind of gap as last round's item (one `doubt.end`
  arm with no test). I am **not** filing it as NEEDS-HUMAN: the human limited this round to one
  test, and the rubric says not to chase silence. If wanted later, the test needs one new wrapper
  action, "plant this key, then pass the commit".

- **The other survivors are equivalent mutants, not gaps.** Making `abort.rs:291` (first read),
  `:304` (torn record) or `:312` (epoch exhausted) return a plain `Stopped` passes all tests, and
  it should:
  - `:291`: the doubt is still empty at the first read.
  - `:304` and `:312`: `Doubt::settle` (`abort.rs:422-431`) clears `held` on any read that is not
    the required bytes. Torn bytes always differ from them. A held fence can never require
    `Open@u64::MAX` bytes, because `open_teardown` refuses that epoch.

  Always replacing `held` with the latest outcome (`abort.rs:409`) changes only which of two
  equal `may_still_commit: true` reports is shown. I counted 23 mutants in total:
  - 16 were caught, and each failed the leg named for it (H1/H2/H4/I/I2/D/E/J). This includes the
    brief's five demonstrated reds: the `mpuctl` put, the attempt-4 "unchanged means taken" rule,
    no third read, the held doubt clobbered by the latest outcome, and a settle that is a no-op.
  - 6 survived as equivalent: the four above, plus two more (`settle` skipped when the read is
    `None`, and an extra condition in `observe` that never changes anything).
  - 1 survived and is not equivalent: `:335`, above.

- **Restore's refactor holds.** `crates/custodian/src/restore.rs:1055` calls
  `fence_batch(key, read, &fence.session, &fence.obligations)`. It builds the same `require` on
  the exact bytes, `require_absent` per key in order, and puts in the same order as the removed
  inline code. The J leg (`multipart_abort.rs:1311`) pins that shape for the two-obligation
  `Completing` case. In `gate-logs/C4-ci.log`, the workspace tests and the madsim DST run
  (`:3846` onward, including the custodian DST) passed with no restore test edited. Could not
  refute.

- **Other attacks that failed.**
  - Top-level `downcast_ref::<CommitUnknownResult>()` (`abort.rs:341`) matches the seam contract
    (`crates/traits/src/lib.rs:216`). The adapters return that type unwrapped
    (`crates/metadata-fdb/src/lib.rs:1711`, `crates/metadata-tikv/src/lib.rs:898`, `:977`).
  - Unbounded awaits are covered: the store contract says every operation terminates
    (`crates/traits/src/lib.rs:1334`), and every loop pass either returns or spends a round.
  - The "Fenced instead of AlreadyAborting" false positive from byte-identical rival fences is
    now stated correctly in `docs/design/architecture/05-building-block-view.md:206`.

- **The brief's verification posture is out of date (no defect).** It says `cargo xtask ci` stops
  at cargo-deny before the DST step. This round's `gate-logs/C4-ci.log:3591-3597` shows deny
  passing. At `:4147-4157` the new DST file `crates/dst/tests/multipart_abort.rs` ran (6 passed,
  including the `should_panic` red at `:625`). So (G) is now exercised by the gating CI row, not
  only by Do's own runs.

- **C4-diff-cov measured nothing.** It reports `fail` because `patch.diff does not apply on
  origin/main` (`gate-logs/C4-diff-cov.log`). That is the stack on #894, not a coverage result,
  and it has been the same in all three rounds. My mutation run above stands in for it on
  `abort.rs`. It covers neither `restore.rs` nor the DST file. Not a refutation.

- NEEDS-HUMAN [human] — `check-gates.json` row `T4-batch-review` passes as **gating** in 13.03 s
  ("3x codex, union, triaged", `0 blocking, 0 recorded-rejected, 0 noise-dropped`), on a
  2,606-line bundle diff. `gate-logs/T4-batch-review.log` holds one summary line and no proof that
  three fresh reviews of this round's diff ran. A cached result from an earlier diff, or a review
  that read nothing, would give the same line. Please confirm that
  `results/issue_895/review-batch.md` was produced from this round's `patch.diff` before counting
  it as the rubric's "one deep, multi-pass review".

Bottom line: I tried to refute the new test (red→green, real path, right reason), the production
doubt handling (23 mutants), restore's refactor, and the gate verdicts. The only real gap is
untested and low-risk (`abort.rs:335`). The only open question is whether the T4 review row
really reviewed this diff.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no introduced correctness defect or actionable reuse, simplification, or efficiency issue identified.

- Correctness: `crates/core/tests/multipart_abort.rs:1112` forces a held unknown commit, an unapplied conflict, and a failed conflict re-read, reaching `crates/core/src/multipart/abort.rs:337`. It asserts the retained indeterminate outcome, then releases the held batch after Abort returns and verifies the fence, obligation, and unchanged admission count. Returning plain `Stopped` on that error arm would fail this test.
- Reuse: `crates/core/src/multipart/abort.rs:77` centralizes fence construction, and `crates/custodian/src/restore.rs:1055` uses it with the existing preconditions and write order preserved. No further actionable simplification found.

Validation used the frozen gate evidence, without rerunning builds: 24 core tests and 6 DST tests passed; mutation testing reported 18 caught and 4 unviable. Baseline red remains unverifiable because the new API is absent on the base; diff coverage was not measured because the patch did not apply to origin/main.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Accept the declared absence of a behavioral pre-fix red for this new API — stashing production while retaining its test produces an unresolved `multipart::abort` import, not an executed failing test (`reviewer-evidence/base-red.log:4`; `brief.md:157`).
- [x] Validation — fitness-to-purpose — Decide whether the observed fencing and late-commit evidence fulfills this slice's reclamation-safety purpose — executable checks support C-1, while acceptance of the intended operational outcome remains human judgment (`brief.md:112`; `crates/core/tests/multipart_abort.rs:1137`; `crates/dst/tests/multipart_abort.rs:409`).
- [x] `check-gates.json` row `T4-batch-review` passes as **gating** in 13.03 s ("3x codex, union, triaged", `0 blocking, 0 recorded-rejected, 0 noise-dropped`), on a 2,606-line bundle diff. `gate-logs/T4-batch-review.log` holds one summary line and no proof that three fresh reviews of this round's diff ran. A cached result from an earlier diff, or a review that read nothing, would give the same line. Please confirm that `results/issue_895/review-batch.md` was produced from this round's `patch.diff` before counting it as the rubric's "one deep, multi-pass review".
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The tracker-derived constraints cannot be checked from the supplied record.** `brief.md:19-21` attributes the shared batch requirement to #656's first comment; `brief.md:109-110` and `:149-154` treat intake and verification exceptions as human rulings. Neither `notes.json` nor `sources/` exists in this sandbox. The source supports the missing Abort writer and inline restore batch, but cannot substantiate those tracker claims or reveal omitted constraints. Include the relevant thread excerpts and ruling references in the revised brief, or explicitly mark them unverified; this review cannot confirm them.
- [x] **Reading the obligation after the session does not establish a damaged key.** `brief.md:206-209` prescribes a typed damage fault when the session re-read is unchanged and the subsequent obligation read finds a key. Counterexample: A receives an unexplained lock Conflict, reads `Open@E`, then B atomically commits the fence and obligation before A reads the obligation. A reports damage even though B performed the valid transition; this contradicts the two-Abort result required at `brief.md:32-33`. Each `get` opens a separate transaction (`crates/metadata-redb/src/lib.rs:113-116`; `crates/metadata-fdb/src/lib.rs:1794-1800`), and the cited TiKV lock-conflict path permits the initial unexplained Conflict (`crates/metadata-tikv/src/lib.rs:1386-1393`). Revise the classification to revalidate the session after observing a present obligation before declaring damage, and add a deterministic leg placing B's commit between those reads. The planted-key H3 case must still report damage when that revalidation confirms the unchanged session. Restore's existing classification can remain outside this slice.
- [x] **The error contract omits an unresolved outcome that is already out of flight.** `brief.md:212-218` retains `CommitUnknownResult` only when a fence may still land, otherwise prescribing an error saying no fence can land; the failed-read coverage at `:58-64` likewise starts with `may_still_commit: true`. But `false` means only that the transaction is out of flight, not that it failed: a successful re-read is still needed to determine its outcome (`crates/traits/src/lib.rs:240-246`). Force a batch to land, return `CommitUnknownResult { may_still_commit: false }`, then fail the re-read. No future fence can land, yet this fence is already durable and its outcome remains unknown to Abort. Specify that this path preserves the indeterminate outcome rather than degrading to an ordinary read/terminal fault, and add a deterministic regression without a preceding `true` outcome. The seam explicitly distinguishes indeterminate from terminal failures (`crates/traits/src/lib.rs:676-699`).
- [x] size backstop — this slice is behaving oversized: patch is 131 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up: add a core test pinning the doubt-keeping arm at `crates/core/src/multipart/abort.rs:335` (held `may_still_commit: true` fence, then a taken obligation key → must answer `InDoubt`, not plain `Stopped`); needs a wrapper action "plant this key, then pass the commit".
