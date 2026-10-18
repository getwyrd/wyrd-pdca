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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 22 mutants tested in 2m: 1 missed, 17 caught, 4 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_895/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.76s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #895: add multipart Abort fencing and share its atomic teardown batch with restore; one builder-fixable regression-test gap remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The specified state answers, atomic retirement obligation, and unchanged admission count match the normative protocol; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:665`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:977`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept a compile-only baseline failure for this new API — the independent stash run fails at the missing import before executing a test, so it establishes no behavioral pre-fix red; `crates/core/tests/multipart_abort.rs:41`, `reviewer-red.log:2`. |
| C3 Change | PASS | The patch stays within the agreed core/restore scope and updates the living architecture; no client Abort caller or admission change is introduced; `crates/custodian/src/restore.rs:1055`, `docs/design/architecture/05-building-block-view.md:206`. |
| C4 Verification (red→green) | PASS | Executed fault mutations fail and restored tests pass, supporting atomicity, conflict handling, uncertainty preservation, and restore compatibility; `reviewer-restored-green.log:28`, `reviewer-dst.log:276`, `reviewer-dst.log:288`; the unmeasured diff coverage is a stacked-base caveat, not a patch failure. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add an observable held-doubt settlement regression — the existing moved-session test still passes if clearing held doubt is removed, allowing an obsolete fence to misclassify a later transient fault as indeterminate; `crates/core/tests/multipart_abort.rs:1112`, `crates/core/src/multipart/abort.rs:422`; independently demonstrated below. |
| T1 Structure | PASS | Both fence writers now share the precondition/put construction through the narrow MetadataStore seam, without new concrete backend dependencies; `crates/core/src/multipart/abort.rs:77`, `crates/core/src/multipart/abort.rs:282`, `crates/custodian/src/restore.rs:1055`. |
| T2 Shape | PASS | Exact-byte preconditions, typed obligation construction, and decode errors preserve the repository’s serialization and validation boundaries; formatting and spelling reruns pass; `crates/core/src/multipart/abort.rs:84`, `crates/core/src/multipart/abort.rs:302`, `crates/core/tests/multipart_abort.rs:1171`. |
| T3 Runtime | PASS | Fence work is bounded by the round budget and terminating store contract, with no new clock reads or background tasks; seeded ambiguity/race tests execute successfully; `crates/core/src/multipart/abort.rs:273`, `crates/traits/src/lib.rs:1334`, `crates/dst/tests/multipart_abort.rs:550`. |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit must rerun at publish; this deferred row is not a missing-evidence finding; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Scope and prior art support this separate Abort slice: all six affected paths were checked against recent merged history and the file lists of 32 closed multipart PRs, including unmerged #647; `reviewer-prior-art.md:5`, `reviewer-prior-art.md:16`; rejected #656 attempts are recorded in `brief.md:209`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this core-only milestone’s utility and delivery sequencing — the function is exercised, but user-facing Abort and retirement completion depend on later gateway/drain work; `crates/core/src/multipart/abort.rs:15`, `crates/core/src/multipart/abort.rs:26`. |

Source citations above are relative to `$PDCA_TARGET`; evidence logs and `brief.md` are relative to the review directory. The target is readable, includes the prerequisite Create implementation, and passes reverse patch applicability. All six affected files retain their original hashes after verification.

**R1 — The moved-session test does not observe whether held doubt was cleared.** At `crates/core/tests/multipart_abort.rs:1112`, the wrapper moves the session to `Completing`. Abort then returns `OperationAborted` directly at `crates/core/src/multipart/abort.rs:309`, regardless of whether `Doubt::settle` cleared anything. Removing only the held-doubt clearing, while retaining out-of-flight settlement, passes all 21 submitted core tests and all six Abort DST tests, including the 50-seed entry point (`reviewer-held-clear-mutant-original-tests.log:29`, `reviewer-held-clear-mutant-dst.log:16`). This is a coverage defect; the submitted production code handles the case correctly.

The independently executed discriminator holds the first fence, moves the session to canonical `Open@2`, then returns a plain transient error on the next fence commit. It asserts `AbortError::Stopped`, `ErrorClass::Transient`, and that releasing the obsolete held batch returns `Conflict`. The patch passes; the held-clear mutant returns `Indeterminate` and fails (`reviewer-probe-green.log`, `reviewer-held-clear-mutant-probes.log:14`). Add this case using the existing `HoldThenMove`/`Fail` wrapper actions. A second probe, an unapplied out-of-flight unknown followed by a successful unchanged read and then a transient commit fault, also passes the patch and rejects the complete `settle` deletion. Probe code is preserved in `pdca-reviewer-895-probes/crates/core/tests/multipart_abort.rs:1235`.

**Independent verification supports the remaining behavior.** Commands executed against the supplied target were `cargo test -p wyrd-core --test multipart_abort` (21 pass), `cargo test -p wyrd-custodian` (304 pass, one pre-existing ignored test), and `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test multipart_abort --test custodian` (six Abort and 33 custodian tests pass). `cargo fmt --all -- --check`, `typos`, docs lint, docs rendering/link audit, `cargo xtask conformance`, and `cargo xtask statics` all pass. The named external tools, typos and the docs renderer, were exercised without shims. In the isolated probe copy, executable mutations that decrement admission, infer obligation damage from an unchanged session, omit the rival-fence re-read, replace held doubt with the latest outcome, and return a failed read’s plain fault all trigger test failures. Restoring production code passes all 23 core tests including the two reviewer probes (`reviewer-probe-restored-green.log`).

**Frozen gate results are consistent with those limits.** The full CI log shows docs checks, fmt/clippy/build/tests, all three deny steps, conformance, statics, and DST finishing successfully (`gate-logs/C4-ci.log:3588`, `gate-logs/C4-ci.log:4213`), superseding the brief’s anticipated inherited deny failure. The TiKV log shows both requested feature builds finishing, and the batch-review log reports zero blocking findings (`gate-logs/host-tikv.log:211`, `gate-logs/T4-batch-review.log:10`). Those full runs are frozen evidence, not claimed as independent reruns. The verification log’s compile-only red agrees with the stash experiment (`gate-logs/C4-verify.log:23`). Diff coverage was never measured because its origin/main base rejected the stacked patch (`gate-logs/C4-diff-cov.log:10`); the supplied target itself is applicable and compiles. The mutation log’s `Doubt::settle` survivor was investigated rather than treated as a production failure (`gate-logs/C5-mutants.log:13`). No capability-probe symptom guard or unresolved external-tool dependency was found.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #895 (multipart Abort fence + shared fence batch)

Short version: I could not break the fix. Every Abort ending I could force is correct, and the
restore refactor is equivalent to the code it replaces. What I did find is one real test gap
(the C5 survivor), one doc sentence that overclaims, and one design question about retry spacing
on TiKV.

## Findings

- NEEDS-HUMAN [impl] — **Half of `Doubt::settle` has no test anywhere; brief leg (I)'s last
  sentence is unproven.** `crates/core/src/multipart/abort.rs:422-431` clears the held
  `may_still_commit: true` doubt when a read finds the session moved. The test that claims to
  cover this, `crates/core/tests/multipart_abort.rs:1109`
  (`i_a_session_moved_under_a_held_fence_ends_without_the_doubt`), moves the session to
  `Completing`, so the Abort returns `Ok(Refused(OperationAborted))`. An `Ok` answer cannot carry
  a doubt, so the test passes whether or not the doubt is cleared. I confirmed this in a scratch
  copy. With the C5 mutant (`settle` → no-op), all 21 shipped core tests still pass. With only
  the held-clearing removed (`unsettled = None` kept), all 21 core tests **and** all 6 madsim DST
  tests (`MADSIM_TEST_NUM=50`) still pass. **Concrete failing case the suite cannot see:** round 1
  is held (`may_still_commit: true`) and the session moves to `Open@2`. The re-read finds `Open@2`,
  so the held batch (which requires `Open@0` bytes) can never land. The Abort re-fences at epoch 2,
  and that commit fails with a plain `TransientFault`. Correct ending: `AbortError::Stopped`,
  class `Transient`. With the held-clearing removed, it ends `InDoubt { may_still_commit: true }`,
  class `Indeterminate`. That would make the caller re-read for a fence that cannot land. It errs
  on the safe side, so it is not a C-1 breach, but it breaks the brief's "only otherwise return the
  plain fault". A test that pins it, using the existing `Scripted` wrapper: plan
  `[HoldThenMove(session_bytes(&open, 2, 1, r#"{"kind":"Open"}"#)), Fail { transient: true }]`,
  then assert `Err(Stopped{..})`, `classify == Transient`, and that `release_held()` returns
  `Conflict`. I ran it: green on the patch, red on both mutants. The other half (an out-of-flight
  outcome settled by a read) **is** caught, but only by the madsim DST's 1021 leg, invariant (4)
  at `crates/dst/tests/multipart_abort.rs:453`. That is why C5 (plain `cargo test`) reports the
  mutant missed. A core test with plan
  `[Unknown { applied: false, may_still_commit: false }, Fail { transient: true }]` expecting
  `Stopped`/`Transient` would let the in-process target catch it too. I ran that one as well:
  green on the patch, red on the full mutant.

- NEEDS-HUMAN [impl] — **A living-doc sentence overclaims the race answer.**
  `docs/design/architecture/05-building-block-view.md:206` says "two Aborts racing for one session
  answer one fenced and one already aborting". That holds only when neither commit's outcome is
  unknown. Counter-case (I ran it as a probe): Abort B fences, and Abort A's own commit comes back
  `CommitUnknownResult { may_still_commit: false }` without being applied. A re-reads and finds
  `Aborting@1`. `open_teardown` is deterministic, so B's put is byte-identical to A's.
  `Doubt::landed` (`abort.rs:295`, `:415`) matches it, and A answers `Fenced`. **Both** Aborts
  answer `Fenced`. The behaviour is harmless: both answers are success, and the one obligation is
  installed. The unconditional sentence is wrong, though. Qualify it, e.g. "…when neither commit's
  outcome is unknown; an Abort that cannot tell its own fence from a rival's identical one answers
  `Fenced`". The brief's (D) race leg (`tests/multipart_abort.rs:662`) is a pure-Conflict race, so
  it does not contradict this.

- NEEDS-HUMAN [human] — **No wait between rounds, unlike Create.** `abort.rs:34` ("retried at
  once, with no backoff") and `abort.rs:58` (`ABORT_ROUNDS` = `MAX_COMPLETE_ATTEMPTS + 1` = 4) give
  four back-to-back rounds. A TiKV lock conflict with nothing moved spends one of them
  (`abort.rs:332`). Create, in the same spot, backs off with jitter
  (`crates/core/src/multipart/create.rs:454`). Concrete case: on TiKV, any other transaction
  holding a lock on `mpu:<id>` for longer than about 4 × (one commit + up to 3 reads) ends the
  Abort with `Stopped { RoundsSpent }`, though nothing is wrong. Examples are a stalled transaction
  until its lock TTL expires or, once #657 lands, part commits that `require` the session.
  Correctness holds: the error is honest, and the tests prove it never claims a fence it lacks.
  This is a liveness question, and today only rival Aborts and the restore fence lock that key.
  Decision: does Abort need a `CreateRuntime`-style wait seam before #668 gives it a production
  caller, or is a 500 under lock contention acceptable? The brief's "spend a round and try again"
  asked for exactly what shipped, so the call is on scope, not on the build.

## Refutation attempts that failed (the fix held)

- **Evidence re-run.** The frozen C4-ci log shows the 21 new core tests and the 6 new DST tests
  green. It also shows the madsim DST step ran: `cargo test -p wyrd-dst (--cfg madsim)` at log
  line 3843, past the deny step the brief expected to block it. That covers (G), and (J) via the
  custodian DST (33 passed). I rebuilt in scratch and got the same results. C4-verify is
  UNVERIFIABLE because the test imports `multipart::abort`, which does not exist on the reverted
  base (E0432). The brief pre-declared this, and it is not a refutation. C4-diff-cov's "does not
  apply on origin/main" happens because the bundle's base is main plus #894. That is a gate
  environment issue, not evidence against the fix.
- **Production path, not a copy.** The tests call the real `abort_session` (`abort.rs:282`) and
  `fence_batch` (`abort.rs:77`) over a real redb store. Wrappers only script store answers. The
  rival in H4 and in the last-round test is the real `abort_session` running on the inner store.
- **A rival lands between `lost`'s reads** (`abort.rs:370`): the third session read sees the move,
  and the answer is `AlreadyAborting` (H4, `tests/multipart_abort.rs:1009`). If this Abort's own
  held batch lands between the reads, the same path finds its put and answers `Fenced`.
- **A taken key while a held doubt exists** (`abort.rs:333`) ends `InDoubt`, not as damage. That
  is the right call: a held batch could still land if the key were later removed.
- **The last round's re-read is answered**, because the budget check (`abort.rs:314`) comes after
  the state answer. A session reopened at `Open@E'` is re-fenced there, with its key at `E'`.
- **Restore equivalence:** `crates/custodian/src/restore.rs:1055` builds the same
  `require` → `require_absent`* → put session → put obligations* sequence as the removed inline
  code. The custodian unit tests and the custodian DST pass unmodified.
- **(E):** no batch touches `mpuctl`, and every leg asserts the count. The DST checks
  `mpu:` records == `mpuctl.count()` after every seed.

Scratch work ran under `$PDCA_SCRATCH/pdca-adversary-895-settle` and is removed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_abort.rs:1113`: The test claiming that a moved session clears commit doubt does not detect failure to clear it. It moves directly to `Completing`, whose refusal returns without consulting the retained doubt (`crates/core/src/multipart/abort.rs:309`). The frozen C5 log confirms that replacing `Doubt::settle` with a no-op survives. Add a deterministic case that holds the first fence, moves the session to a later `Open` epoch, then fails the next commit with a transient fault; assert `AbortError::Stopped` with `ErrorClass::Transient` and that releasing the old fence conflicts. Also cover an unapplied `may_still_commit: false` outcome followed by a successful unchanged read and a plain fault. These cases make both clearing branches observable and catch stale uncertainty incorrectly turning a definite failure into `Indeterminate`.

No introduced production correctness bug or actionable reuse, simplification, or efficiency issue found. Reviewed the target source and frozen gate evidence; no builds or mutations were run against the read-only target.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Accept a compile-only baseline failure for this new API — the independent stash run fails at the missing import before executing a test, so it establishes no behavioral pre-fix red; `crates/core/tests/multipart_abort.rs:41`, `reviewer-red.log:2`.
- [ ] C5 Causal adequacy — Add an observable held-doubt settlement regression — the existing moved-session test still passes if clearing held doubt is removed, allowing an obsolete fence to misclassify a later transient fault as indeterminate; `crates/core/tests/multipart_abort.rs:1112`, `crates/core/src/multipart/abort.rs:422`; independently demonstrated below.
- [x] Validation — fitness-to-purpose — Accept this core-only milestone’s utility and delivery sequencing — the function is exercised, but user-facing Abort and retirement completion depend on later gateway/drain work; `crates/core/src/multipart/abort.rs:15`, `crates/core/src/multipart/abort.rs:26`.
- [ ] **Half of `Doubt::settle` has no test anywhere; brief leg (I)'s last sentence is unproven.** `crates/core/src/multipart/abort.rs:422-431` clears the held `may_still_commit: true` doubt when a read finds the session moved. The test that claims to cover this, `crates/core/tests/multipart_abort.rs:1109` (`i_a_session_moved_under_a_held_fence_ends_without_the_doubt`), moves the session to `Completing`, so the Abort returns `Ok(Refused(OperationAborted))`. An `Ok` answer cannot carry a doubt, so the test passes whether or not the doubt is cleared. I confirmed this in a scratch copy. With the C5 mutant (`settle` → no-op), all 21 shipped core tests still pass. With only the held-clearing removed (`unsettled = None` kept), all 21 core tests **and** all 6 madsim DST tests (`MADSIM_TEST_NUM=50`) still pass. **Concrete failing case the suite cannot see:** round 1 is held (`may_still_commit: true`) and the session moves to `Open@2`. The re-read finds `Open@2`, so the held batch (which requires `Open@0` bytes) can never land. The Abort re-fences at epoch 2, and that commit fails with a plain `TransientFault`. Correct ending: `AbortError::Stopped`, class `Transient`. With the held-clearing removed, it ends `InDoubt { may_still_commit: true }`, class `Indeterminate`. That would make the caller re-read for a fence that cannot land. It errs on the safe side, so it is not a C-1 breach, but it breaks the brief's "only otherwise return the plain fault". A test that pins it, using the existing `Scripted` wrapper: plan `[HoldThenMove(session_bytes(&open, 2, 1, r#"{"kind":"Open"}"#)), Fail { transient: true }]`, then assert `Err(Stopped{..})`, `classify == Transient`, and that `release_held()` returns `Conflict`. I ran it: green on the patch, red on both mutants. The other half (an out-of-flight outcome settled by a read) **is** caught, but only by the madsim DST's 1021 leg, invariant (4) at `crates/dst/tests/multipart_abort.rs:453`. That is why C5 (plain `cargo test`) reports the mutant missed. A core test with plan `[Unknown { applied: false, may_still_commit: false }, Fail { transient: true }]` expecting `Stopped`/`Transient` would let the in-process target catch it too. I ran that one as well: green on the patch, red on the full mutant.
- [ ] **A living-doc sentence overclaims the race answer.** `docs/design/architecture/05-building-block-view.md:206` says "two Aborts racing for one session answer one fenced and one already aborting". That holds only when neither commit's outcome is unknown. Counter-case (I ran it as a probe): Abort B fences, and Abort A's own commit comes back `CommitUnknownResult { may_still_commit: false }` without being applied. A re-reads and finds `Aborting@1`. `open_teardown` is deterministic, so B's put is byte-identical to A's. `Doubt::landed` (`abort.rs:295`, `:415`) matches it, and A answers `Fenced`. **Both** Aborts answer `Fenced`. The behaviour is harmless: both answers are success, and the one obligation is installed. The unconditional sentence is wrong, though. Qualify it, e.g. "…when neither commit's outcome is unknown; an Abort that cannot tell its own fence from a rival's identical one answers `Fenced`". The brief's (D) race leg (`tests/multipart_abort.rs:662`) is a pure-Conflict race, so it does not contradict this.
- [x] **No wait between rounds, unlike Create.** `abort.rs:34` ("retried at once, with no backoff") and `abort.rs:58` (`ABORT_ROUNDS` = `MAX_COMPLETE_ATTEMPTS + 1` = 4) give four back-to-back rounds. A TiKV lock conflict with nothing moved spends one of them (`abort.rs:332`). Create, in the same spot, backs off with jitter (`crates/core/src/multipart/create.rs:454`). Concrete case: on TiKV, any other transaction holding a lock on `mpu:<id>` for longer than about 4 × (one commit + up to 3 reads) ends the Abort with `Stopped { RoundsSpent }`, though nothing is wrong. Examples are a stalled transaction until its lock TTL expires or, once #657 lands, part commits that `require` the session. Correctness holds: the error is honest, and the tests prove it never claims a fence it lacks. This is a liveness question, and today only rival Aborts and the restore fence lock that key. Decision: does Abort need a `CreateRuntime`-style wait seam before #668 gives it a production caller, or is a 500 under lock contention acceptable? The brief's "spend a round and try again" asked for exactly what shipped, so the call is on scope, not on the build.
- [ ] `crates/core/tests/multipart_abort.rs:1113`: The test claiming that a moved session clears commit doubt does not detect failure to clear it. It moves directly to `Completing`, whose refusal returns without consulting the retained doubt (`crates/core/src/multipart/abort.rs:309`). The frozen C5 log confirms that replacing `Doubt::settle` with a no-op survives. Add a deterministic case that holds the first fence, moves the session to a later `Open` epoch, then fails the next commit with a transient fault; assert `AbortError::Stopped` with `ErrorClass::Transient` and that releasing the old fence conflicts. Also cover an unapplied `may_still_commit: false` outcome followed by a successful unchanged read and a plain fault. These cases make both clearing branches observable and catch stale uncertainty incorrectly turning a definite failure into `Indeterminate`.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The tracker-derived constraints cannot be checked from the supplied record.** `brief.md:19-21` attributes the shared batch requirement to #656's first comment; `brief.md:109-110` and `:149-154` treat intake and verification exceptions as human rulings. Neither `notes.json` nor `sources/` exists in this sandbox. The source supports the missing Abort writer and inline restore batch, but cannot substantiate those tracker claims or reveal omitted constraints. Include the relevant thread excerpts and ruling references in the revised brief, or explicitly mark them unverified; this review cannot confirm them.
- [x] **Reading the obligation after the session does not establish a damaged key.** `brief.md:206-209` prescribes a typed damage fault when the session re-read is unchanged and the subsequent obligation read finds a key. Counterexample: A receives an unexplained lock Conflict, reads `Open@E`, then B atomically commits the fence and obligation before A reads the obligation. A reports damage even though B performed the valid transition; this contradicts the two-Abort result required at `brief.md:32-33`. Each `get` opens a separate transaction (`crates/metadata-redb/src/lib.rs:113-116`; `crates/metadata-fdb/src/lib.rs:1794-1800`), and the cited TiKV lock-conflict path permits the initial unexplained Conflict (`crates/metadata-tikv/src/lib.rs:1386-1393`). Revise the classification to revalidate the session after observing a present obligation before declaring damage, and add a deterministic leg placing B's commit between those reads. The planted-key H3 case must still report damage when that revalidation confirms the unchanged session. Restore's existing classification can remain outside this slice.
- [x] **The error contract omits an unresolved outcome that is already out of flight.** `brief.md:212-218` retains `CommitUnknownResult` only when a fence may still land, otherwise prescribing an error saying no fence can land; the failed-read coverage at `:58-64` likewise starts with `may_still_commit: true`. But `false` means only that the transaction is out of flight, not that it failed: a successful re-read is still needed to determine its outcome (`crates/traits/src/lib.rs:240-246`). Force a batch to land, return `CommitUnknownResult { may_still_commit: false }`, then fail the re-read. No future fence can land, yet this fence is already durable and its outcome remains unknown to Abort. Specify that this path preserves the indeterminate outcome rather than degrading to an ordinary read/terminal fault, and add a deterministic regression without a preceding `true` outcome. The seam explicitly distinguishes indeterminate from terminal failures (`crates/traits/src/lib.rs:676-699`).
- [x] size backstop — this slice is behaving oversized: patch is 124 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Production code is accepted as is; this round is tests + one doc sentence only. Do not change abort.rs behaviour, the round budget, or the no-backoff retry (the human cleared that as in-scope). 1. Add a core test in crates/core/tests/multipart_abort.rs that makes the held-doubt clearing in Doubt::settle (abort.rs:422-431) observable: plan [HoldThenMove(session_bytes(&open, 2, 1, r#"{"kind":"Open"}"#)), Fail { transient: true }]; assert Err(AbortError::Stopped{..}), classify == ErrorClass::Transient, and that release_held() returns Conflict. The existing test at :1109 moves to Completing and so cannot see the doubt. 2. Add a core test for the out-of-flight half: plan [Unknown { applied: false, may_still_commit: false }, Fail { transient: true }] (successful unchanged re-read between), expecting Stopped / Transient. Today only the madsim DST catches a settle() no-op; plain cargo test (the C5 mutants gate) must catch it too. 3. Qualify docs/design/architecture/05-building-block-view.md:206 "two Aborts racing for one session answer one fenced and one already aborting": true only when neither commit's outcome is unknown; an Abort that cannot tell its own fence from a rival's byte-identical one answers Fenced (both answer Fenced, harmless).
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
