# core: multipart Abort fence + shared fence batch (636.3b)

> The Plan artifact for slice 3b of #636. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. Split from #656 at the 2026-10-15 re-plan.
> It builds on the sibling Create slice #894 (636.3a), whose Create function makes the sessions this
> slice fences. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read the **Abort / reap
> fence** row (`0016:665`) and decision 3's verb × state table (`0016:973-979`, the Abort row
> `:977`) before writing code. Citations verified against `getwyrd/wyrd` `origin/main` @
> `7bf7eb9` on 2026-10-15. The sibling's store module is not on `main` yet; build on the base
> this bundle is given.

- **Slug:** multipart-abort-fence
- **Track:** alpha
- **Goal:** Nothing but the post-restore pass fences a multipart session today. Restore's
  `fence_session` builds its fence batch inline (`crates/custodian/src/restore.rs:1045-1057`).
  This slice adds `AbortMultipartUpload`'s teardown fence as a `wyrd-core` function over
  `MetadataStore`, answering in `AbortOutcome` (`crates/core/src/multipart.rs:4440`) and
  `Refusal` (`:4302`). It also moves the fence **batch** construction into one core builder that
  both the client Abort and restore use. That is the "one batch builder, not two" #656's first
  tracker comment (2026-09-12) asks for.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_abort.rs`, over
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
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and every red leg is forced by a wrapper rather than left to the scheduler. (H)'s wrapper
  returns the adapter's conflict on demand. redb alone never produces it, because redb reports
  Conflict only for a failed precondition (`crates/metadata-redb/src/lib.rs:212-222`); so the
  wrapper is what makes the attempt-4 defect fail on every run, and it also places a rival's
  commit between two reads for H4. (I)'s wrapper holds a batch in flight past the Abort's
  return, which is the case the defect lies about; (I2)'s fails the reads after an applied
  out-of-flight outcome. (D)'s race is forced
  the same way. (G) is invariant coverage, not the red for (H) or (I). In attempt 4 the sim
  landed in-flight batches on a fair coin at nearly every store call, so the window rarely
  opened. The deterministic core tests are the regression for (H) and (I). (G) is madsim-only,
  and C4-ci cannot reach it today (see `Verification posture`).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*). Every Abort that fences a session installs, **in the same batch**,
  an obligation naming everything the session protects. No answer or error lets its caller
  take the fence for one that installed nothing while a fence of it may still land. The
  admission count never changes on a fence. So no fenced session is ever outside the reach of
  the actor that reclaims it (#659's drain), and every writer of the `Open → Aborting` edge
  builds the same batch. **Sources:** the maintainer's rule 2026-07-25 (via principles.md §5);
  0016's refutation standard (`0016:2813`); the batch row `0016:665`. SELF-TEST: it binds two
  writers (client Abort and restore), a batch and a caller-visible answer, so guarding a single
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 894
- **Conflicts with:** 768
- **Ordering note:** `Depends on 894`: Abort's tests and DST need `Open` sessions, and only
  the sibling's Create can make one through production code (`SessionRecord` has no free
  constructor, `crates/core/src/multipart.rs:2259-2263`). This slice also adds to the store
  module the sibling creates. `Conflicts with 768` (added by the planner after `--accept`): both edit
  `docs/design/architecture/05-building-block-view.md` (`:202` here, `:197-200` in #768).
  **Intake-cap override (wyrd-pdca-P1):** the human allowed both children of #656's split in
  over the alpha cap in the 2026-10-15 Plan session; count then 7/6, room 0, need 2.
- **Scope:** `AbortMultipartUpload`'s fence as a store-I/O function in `wyrd-core` answering in
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
- **Repro instruction:** n/a (new functionality). Today no client Abort exists, and the only
  fence batch is the inline one at `crates/custodian/src/restore.rs:1045-1057`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_abort.rs`, a **NEW** file (C4-verify treats an
  added `crates/*/tests/*.rs` as the per-fix discriminator). The DST scenario
  `crates/dst/tests/multipart_abort.rs` is also NEW.
- **Verification posture:** (a) net-new. The Abort function does not exist on this bundle's base,
  which is `main` plus the sibling's accepted patch. So the discriminating test cannot compile
  on C4-verify's reverted base, and C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared
  sign-off item.
  **Built and exercised at Check:** the Abort function and the shared builder, driven by the new
  test over a real store. Restore's fence goes through the builder under restore's existing
  tests.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - an `mpuctl` decrement added to the fence batch → (E) fails;
  - an unchanged session after `Conflict` taken as proof the obligation key is taken, without
    reading it (attempt 4's rule) → H1 and H2 fail;
  - a present obligation key named as damage without re-reading the session → H4 fails;
  - the held doubt replaced by the latest outcome → (I) fails;
  - a failed re-read after `may_still_commit: false` returned as the read's own fault → (I2)
    fails.
  **The inherited deny failure (human's ruling, 2026-10-15).** `cargo xtask ci` stops at the
  all-features cargo-deny step (RUSTSEC-2026-0346/0349) before its conformance, statics,
  orchestrator-guard and DST steps (`xtask/src/main.rs:1683-1687`). Do does not touch
  `Cargo.lock` or `deny*.toml`. Do runs `cargo xtask conformance`, `cargo xtask statics` and
  `cargo xtask dst` on its own and puts each command's result in `build-notes.md`. `cargo xtask
  dst` also covers the custodian DST file that exercises restore's fence (J). Test-local
  wrappers keep state in struct fields, never in a `static` (`xtask/src/main.rs:1221-1242`).
- **Production reach:**
  - (a) The test exercises Abort for real against a real store. Production calls no client
    Abort in this slice.
  - (b) The S3 verb lands with #668, under 0016's same-release rule for the reaper and operator
    abort (`0016:236-268`).
  - (c) This slice **must not** add a production caller of Abort.
  - (d) The one production path it changes is restore's existing fence, which switches to the
    shared builder with identical preconditions and puts.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer callsite to mirror:** restore's `fence_session`, `crates/custodian/src/restore.rs:1027-1089`.
    Its batch (`:1045-1057`) `require`s the exact session bytes it read, `require_absent`s each
    obligation key, and puts session and obligations in one batch. Its `Conflict` branch
    (`:1067-1083`) re-reads the session, and when the session is unchanged it **reads each
    obligation key** before naming one taken. A conflict that a re-read cannot explain is
    `SessionUnsettled::LostConflict` (`:324`), not damage. Its `Err` arm (`:1085-1089`) never
    reads an unknown outcome as `Conflict`. The client Abort must classify the same way. It
    differs only where the client contract needs more: it retries an unexplained conflict
    within its rounds, and it keeps a doubt (see Design).
  - **Records:** `SessionRecord::open_teardown` (`crates/core/src/multipart.rs:2360-2372`) and
    `completing_teardown` (`:2386-2399`) mint the session and obligations.
    `decode_retire_obligation` (`:3710`) is the reader every obligation must satisfy at its key.
  - **Answers:** `AbortOutcome` (`:4440`), `Refusal` (`:4302`), and `MAX_COMPLETE_ATTEMPTS`
    (`:4975`) if Do sizes the round budget from it.
- **Prior-art check (triage cycles):** searched by path `crates/custodian/src/restore.rs`,
  `crates/core/src/multipart.rs`, `crates/core/tests/multipart_abort.rs`,
  `crates/dst/tests/multipart_abort.rs` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main`. Merged: `8d15209` (the
  open-session restore fence, #841), `c0dacf3` / PR #856 (the completing-session restore fence,
  #842), `39bf35a` (the fence-completion record). #664 (637.4, restore fence) is closed. Neither
  new file exists. Rejected: #656's four attempts at Create and Abort together (this split).
  Attempt 4's only blocking finding was this slice's conflict branch, now leg (H). #636 was closed
  not-planned 2026-10-10. Open PRs on getwyrd/wyrd touching these paths: none.
- **Disposition hint:** new-feature

**Sources for the claims taken from the tracker and the Plan session** (for the sign-off reader;
Do need not open them, all paths in the wyrd-pdca repo):
- "one batch builder, not two": #656's tracker comment of 2026-09-12,
  https://github.com/getwyrd/wyrd/issues/656#issuecomment-5648455230 ("The client Abort in this
  issue crosses the same `Open → Aborting` edge, so the two should use one batch builder, not
  two. Whichever lands first builds it, and the other reuses it."), which also states the
  `decode_retire_obligation` rule for any first writer. Copy in `results/issue_656/notes.json`;
- the `{session, all}` payload and the 128,916-byte figure: #656's tracker comment of
  2026-10-05, https://github.com/getwyrd/wyrd/issues/656#issuecomment-5998992104 ("10,000 sparse
  parts = 128,916 bytes > 100,000, and FoundationDB refuses it permanently … use
  `{session, all}` as the Open fence does");
- the intake-cap override and the inherited-deny ruling: the 2026-10-15 Plan session, recorded
  at the top of `results/issue_656/split-proposal.md:4-14`;
- attempt 4's blocking finding (the unchanged-session inference):
  `results/issue_656/iteration-v4/check-review.md:9`, `:12`, and R1 at `:17-24`.

Plan-review response (2026-10-15): all three findings hold and are revised in place.
(1) Sources: listed above, with the two tracker comments quoted. (2) The obligation-read race:
a present key now triggers a session re-read before any damage verdict (Design), H4 forces a
rival fence between the two reads, and H3 still reports damage when the re-read confirms the
session unchanged; restore's classification stays out of scope. (3) The out-of-flight case:
Design now keeps an unsettled `may_still_commit: false` outcome as an `Indeterminate` error, and
(I2) pins it with no earlier `true` outcome.

## Design

**Abort (`0016:665`, `:977`).** Read `mpu:<id>` and answer from its state:
- `Open@E`: commit `open_teardown`'s session and obligation in one batch, built by the shared
  builder. The batch `require`s the exact bytes read and `require_absent`s the obligation key.
  A client Abort carries no slot preconditions (`0016:665`).
- `Aborting`: `AlreadyAborting`.
- `Completing`: `OperationAborted`, with nothing written.
- `Completed`, or no record: `NoSuchUpload`.
The fence never touches `mpuctl`.

**Classifying a commit that did not say `Committed`.** Abort runs a bounded number of rounds,
and each round re-reads and answers from what it finds:
- **`Conflict` and the session moved:** answer from the new state. A rival Abort that won leaves
  `Aborting`, so the answer is `AlreadyAborting`: that is the "no 409 race". A session at a later
  `Open@E'` is fenced again at `E'`.
- **`Conflict` and the session unchanged:** read the obligation key.
  - **Present:** re-read the session before naming anything. If it has **moved**, a rival fence
    landed between the two reads (its session and obligation are one batch): answer from the new
    state, as above (H4). Only if it is **still** the exact bytes this round read is the key
    damage: a typed fault naming it (a token is minted once, `0016:358-373`, so a taken key is
    damage a retry cannot settle; H3).
  - **Absent:** the conflict is unexplained (TiKV's lock conflict,
    `crates/metadata-tikv/src/lib.rs:1388-1393`). Spend a round and try again, keeping any doubt.
  Restore's own classification (`restore.rs:1067-1083`) reads in the same order and has the
  same gap; it stays out of scope here (see Scope).
- **`CommitUnknownResult`:** never read as `Conflict`. Re-read. A fence that landed shows as
  `Aborting` and is answered as `Fenced`.
- **Two kinds of doubt, both kept.**
  - *May still land* (`may_still_commit: true`): keep it while the session is unchanged, across
    later out-of-flight unknowns, a failed re-read, a later plain store fault, and unexplained
    conflicts. Only a read that finds the session moved clears it, because every fence requires
    the exact bytes it read and the epoch only rises.
  - *Out of flight, outcome not yet read* (`may_still_commit: false`): the batch can no longer
    land later, but it may already have landed. One successful read settles it. Until that read
    succeeds, the outcome is still unknown.
  When the rounds run out, or a fault ends the Abort: return the held `CommitUnknownResult`
  with `may_still_commit: true` if any fence of this Abort may still land; else, if any of its
  unknown outcomes was never settled by a successful read, return a `CommitUnknownResult` (class
  `Indeterminate`) naming the upload id (I2); only otherwise return the plain fault or a typed
  error saying no fence of this Abort can land.

**The shared fence batch builder.** What #841 shipped is a *record* builder: `open_teardown`
returns the session and obligation. The `WriteBatch` with its preconditions is still built inline
in restore's `fence_session`. Move that batch construction into `wyrd-core` as one builder: given
the `mpu:` key, the exact bytes read, the fenced session and its obligations, it `require`s the
bytes, `require_absent`s every obligation key, puts the session, and puts every obligation. It
takes a list of obligations, because restore also fences `Completing` sessions (two obligations,
`restore.rs:1124-1131`). It writes only obligations that `decode_retire_obligation` accepts at
the key they go under (the 2026-09-12 tracker comment's rule for any first writer).
`fence_session` calls it in place of `:1045-1057`, keeping the same preconditions, the same puts
and the same order. Restore's tests pass unmodified. This is a refactor of an existing writer,
not a new production caller.

**The `{session, all}` payload.** Abort uses `open_teardown`'s `{session, all}`, never a
`{session, parts: <set>}` list. The list form can exceed `MAX_VALUE_BYTES` for a sparse session:
10,000 sparse parts is 128,916 bytes against a 100,000 ceiling, and FoundationDB refuses that
permanently (the 2026-10-05 tracker comment). `all` is sound because no part commits outside
`Open` (`0016:712-717`).

**Docs currency** (`AGENTS.md:154-157`): add Abort to the paragraph at
`docs/design/architecture/05-building-block-view.md:202`. Say that Abort fences an `Open` session
to `Aborting` with its retirement obligation in the same batch, that the client Abort and the
restore fence build that batch in one place, that the fence never moves the admission count, and
that an Abort which ends while a fence may still land ends with that unknown outcome. Note there
is no production caller until #668.

## Alternatives considered

- **A per-session decrement on Abort.** It releases a slot while the session still holds staged
  bytes, so admission would stop bounding the namespaces it exists to bound. The withdrawn
  "`Completed` releases its admission slot" criterion is unsatisfiable by a conforming
  implementation and must not be re-earned. Rejected.
- **A second fence batch builder beside restore's.** Two writers of one edge drift apart.
  Rejected.
- **Reusing only `open_teardown` and calling that "the shared builder".** It shares the records
  but leaves two copies of the batch preconditions. Rejected.
- **Inferring a taken obligation key from an unchanged session.** This held for redb and the
  sim-FDB model, but not for TiKV, and it discarded a fence still able to land (attempt 4's
  blocking finding). Rejected; read the key.
- **A settling look before returning, as Create does.** A fence released after Abort returns
  cannot be found by a look taken before it returns. Abort's contract is to return the doubt
  instead. Rejected.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Production code is accepted as is; this round is tests + one doc sentence only. Do not change abort.rs behaviour, the round budget, or the no-backoff retry (the human cleared that as in-scope). 1. Add a core test in crates/core/tests/multipart_abort.rs that makes the held-doubt clearing in Doubt::settle (abort.rs:422-431) observable: plan [HoldThenMove(session_bytes(&open, 2, 1, r#"{"kind":"Open"}"#)), Fail { transient: true }]; assert Err(AbortError::Stopped{..}), classify == ErrorClass::Transient, and that release_held() returns Conflict. The existing test at :1109 moves to Completing and so cannot see the doubt. 2. Add a core test for the out-of-flight half: plan [Unknown { applied: false, may_still_commit: false }, Fail { transient: true }] (successful unchanged re-read between), expecting Stopped / Transient. Today only the madsim DST catches a settle() no-op; plain cargo test (the C5 mutants gate) must catch it too. 3. Qualify docs/design/architecture/05-building-block-view.md:206 "two Aborts racing for one session answer one fenced and one already aborting": true only when neither commit's outcome is unknown; an Abort that cannot tell its own fence from a rival's byte-identical one answers Fenced (both answer Fenced, harmless).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Production code is accepted as is; this round is tests + one doc sentence only. Do not change abort.rs behaviour, the round budget, or the no-backoff retry (the human cleared that as in-scope).
  1. Add a core test in crates/core/tests/multipart_abort.rs that makes the held-doubt clearing in Doubt::settle (abort.rs:422-431) observable: plan [HoldThenMove(session_bytes(&open, 2, 1, r#"{"kind":"Open"}"#)), Fail { transient: true }]; assert Err(AbortError::Stopped{..}), classify == ErrorClass::Transient, and that release_held() returns Conflict. The existing test at :1109 moves to Completing and so cannot see the doubt.
  2. Add a core test for the out-of-flight half: plan [Unknown { applied: false, may_still_commit: false }, Fail { transient: true }] (successful unchanged re-read between), expecting Stopped / Transient. Today only the madsim DST catches a settle() no-op; plain cargo test (the C5 mutants gate) must catch it too.
  3. Qualify docs/design/architecture/05-building-block-view.md:206 "two Aborts racing for one session answer one fenced and one already aborting": true only when neither commit's outcome is unknown; an Abort that cannot tell its own fence from a rival's byte-identical one answers Fenced (both answer Fenced, harmless).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 22 mutants tested in 2m: 1 missed, 17 caught, 4 unviable
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Production code is accepted as is; this round is ONE test only. Do not change abort.rs behaviour, the round budget, or the no-backoff retry. 1. Add a core test in crates/core/tests/multipart_abort.rs pinning that a held "may still land" doubt survives a failed re-read inside the Conflict branch (abort.rs:337). Nothing covers that combination today: H2 (:967) has conflicts with successful reads, i_a_held_fence_survives_a_failed_re_read (:1090) has a failed read after an unknown commit. The adversary's mutant (replacing `doubt.end(upload_id, Stop::Store(fault))` on that arm with a plain AbortError::Stopped) passes all 23 tests. Suggested shape with the existing wrapper: store.script(vec![OnCommit::Hold, OnCommit::ConflictUnmoved], None); store.state().fail_session_reads_after = Some(2); then assert in_doubt(outcome, &id, true), doubt.cause is Stop::Store(_), release_held() == Committed, assert_fenced, mpuctl count unchanged. It must pass on the patch and fail on that mutant.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Production code is accepted as is; this round is ONE test only. Do not change abort.rs behaviour, the round budget, or the no-backoff retry.
  1. Add a core test in crates/core/tests/multipart_abort.rs pinning that a held "may still land" doubt survives a failed re-read inside the Conflict branch (abort.rs:337). Nothing covers that combination today: H2 (:967) has conflicts with successful reads, i_a_held_fence_survives_a_failed_re_read (:1090) has a failed read after an unknown commit. The adversary's mutant (replacing `doubt.end(upload_id, Stop::Store(fault))` on that arm with a plain AbortError::Stopped) passes all 23 tests. Suggested shape with the existing wrapper: store.script(vec![OnCommit::Hold, OnCommit::ConflictUnmoved], None); store.state().fail_session_reads_after = Some(2); then assert in_doubt(outcome, &id, true), doubt.cause is Stop::Store(_), release_held() == Committed, assert_fenced, mpuctl count unchanged. It must pass on the patch and fail on that mutant.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
