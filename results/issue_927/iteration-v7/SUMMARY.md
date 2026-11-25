# Result — issue 927 / multipart-upload-part-commit

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Staging (#932, #933) can stage a part's chunks, but nothing turns them into a part. This
  slice adds the second half of `UploadPart` as `wyrd-core` functions. It (1) commits the
  `part:` + `psum:` pair, the slot release and the `sidx:` deletes in one batch, after a last
  renewal, sending each commit round only while the lease has the dispatch margin left (F). It (2) classifies a `Conflict` or a `CommitUnknownResult` from a re-read, never
  blind. It (3) runs the live-session compensation, including against a session rolled back to
  a newer `Open@E'`, and on any settled stop with nothing in doubt. It (4) handles a re-upload of a committed part number, retiring the prior
  part's chunks in the same batch. It (5) refuses a part past the per-part or cumulative chunk
  ceiling and compensates what it staged. It (6) composes the whole `UploadPart` (reserve →
  stage → commit), answering in the landed vocabulary (`UploadPartOutcome`
  `crates/core/src/multipart.rs:4669`, `Refusal` `:4550`).
- Success criterion: A NEW test file `crates/core/tests/multipart_upload_part_commit.rs`,
  over the same harness as `crates/core/tests/multipart_upload_part_stage.rs`: `RedbMetadataStore::in_memory()`, a test-local
  placement-aware chunk store, and a store wrapper that logs batches and can inject `Conflict`,
  `CommitUnknownResult` (either flag, applied or not), a held batch landed later, a write before
  a chosen commit, or **an action between two reads**. Each core test file carries its own copy,
  as `crates/core/tests/multipart_abort.rs` does. Every session is made by #894's Create, and
  every part is staged by `stage_part` (#932). The test shows all of the following.
  **(A) Commit.** Parts staged **out of order** (3, then 1, then 2), at least one of them more
  than one chunk, each commit to `Committed { part_number, digest }`. After each commit, the
  part's `part:` and `psum:` records decode and agree (chunk count, length, digest), and no
  `sidx:` entry or `slot:` record of that attempt remains. The wrapper's log shows the attempt's
  last renewal **before** the commit's reads, and no renewal batch between those reads and the
  commit (`0016:1720-1733`).
  **(B) A lost commit is classified, never compensated blind** (`0016:1584-1653`).
  - The session fenced to `Aborting` between the last intent and the commit: the commit answers
    `Refused(NoSuchUpload)` and writes nothing (the residue is the reaper's).
  - **The session rolled back to `Open@E'` (`E' > E`)** between the last intent and the commit
    (a Complete fenced it and rolled back, decision 3). The attempt compensates under
    `require(mpu == Open@E')` (`0016:1607-1620`): one `retire:bytes:` obligation for its own
    chunks, its slot released, and its `sidx:` entries deleted, all in one batch. It answers
    exactly `Refused(OperationAborted)` (see Design § *The answer for a lost commit on a live
    session*), never `Committed` and never `NoSuchUpload`, because the session is `Open`.
  - A same-part rival committed first: the loser commits one `retire:bytes:` obligation for its
    **own** chunks (it must decode under `decode_retire_obligation`, `multipart.rs:3954`),
    releases its slot and deletes its `sidx:` entries in one batch, and answers exactly
    `Refused(OperationAborted)`.
  - A commit whose batch lands but answers `CommitUnknownResult`: the attempt re-reads, finds its
    own commit, answers `Committed`, and its slot is released exactly once.
  - A re-upload whose batch did not land (`may_still_commit: false`): the re-read finds the
    **prior** part. The attempt must not answer `Committed` for it; it carries on as for a commit
    that did not land, and ends with its own part committed.
  - **A held batch** (`may_still_commit: true`; the wrapper holds it and lands it later): the
    re-read finds the prior part, or none. The attempt neither answers `Committed` nor
    compensates on that read alone, and it sends **no compensation batch while the held batch
    may still land** (Design § *Settle before compensating*). Every batch it sends next requires
    its own slot at the bytes the held batch also requires, so at most one of them lands. Run it three times: (i) the held
    batch released before the attempt's next batch; (ii) released after the attempt has
    returned; (iii) every later batch answered `Conflict` unapplied until the rounds run out, then
    the held batch released. In every run: exactly one of {its own part, its compensation} is
    durable at the end; the slot is released exactly once; and the answer is `Committed` only if
    its own part is the one that landed. In (iii) the attempt returns an in-doubt error that keeps
    the `may_still_commit` doubt, as Abort's does (`crates/core/src/multipart/abort.rs:264-274`).
  - A same-number rival plus an unknown outcome, **with this attempt's commit not applied**:
    the wrapper answers the commit `CommitUnknownResult` without applying
    it (run once per `may_still_commit` value; the part number had no record before, so the
    commit required the key absent), a non-empty rival part then lands, and the
    re-read finds the rival's record **and this attempt's own slot still held at the bytes it
    last wrote**, on a slot read made **after** the read that found the rival. That held slot is
    what proves the commit did not land (the commit deletes it), and because the commit required a
    value that cannot come back (the key absent, or a non-empty prior), it never can.
    Never `Committed`; the attempt compensates under its own slot and answers exactly
    `Refused(OperationAborted)`. This is the history that tells B apart from C3, whose commit
    **did** land and whose slot is gone: the own-commit proof is checked **first**, and the
    rival-record branch applies only when it fails (Design § *After a
    `CommitUnknownResult`*).
  - **A held commit, then an unknown empty compensation.** An empty re-upload
    of part 1 over an empty part 1, clock fixed (C1's shape). The wrapper holds the commit
    (`may_still_commit: true`, not applied), then lands an empty same-number rival in the same
    millisecond, whose bytes equal the prior's, so the held commit's `require(part == prior)`
    can still hold. From then on, if the attempt sends a compensation batch (one that deletes its
    slot and puts no `part:`), the wrapper first lands the held commit and then answers the
    compensation `CommitUnknownResult { may_still_commit: true }` unapplied; any other batch it
    answers `Conflict` unapplied. A held commit is released, if still held, after the attempt
    returns. Required: the wrapper's log shows **no compensation batch** sent while the held
    commit was unsettled; the answer is never `Refused(OperationAborted)` — it is `Committed` if
    the attempt saw its slot gone with no compensation sent, else the in-doubt error keeping
    `may_still_commit`; at the end the slot is released exactly once and no `retire:` key
    exists. An implementation that compensates while the commit is held, then reads "slot gone at
    `Open@E`" as its compensation landing, answers `OperationAborted` for a commit that landed,
    and fails here.
  - **The read-gap leg: a held commit, and a precondition that dies between the reads** (Design §
    *Settle before compensating*). The commit (of a non-empty part) is held (`may_still_commit: true`). At
    **each** gap between the reads the attempt makes to classify it, in turn, the wrapper runs one
    of four histories. *Landed, then a rival:* it lands the held commit, then a same-number rival
    that replaces it. Required: the log shows **no compensation batch**, the answer is `Committed`,
    and the slot is released exactly once. *Landed, then a rollback:* it lands the held commit, then
    a Complete rolls the session back to `Open@E'`. Required: no compensation batch, the in-doubt
    error, and never `Refused(OperationAborted)`. *Not landed, a rival:* a same-number rival lands
    and the held commit does not. *Not landed, a rollback:* the session rolls back and the held
    commit does not land. In both of those the attempt must reach a slot read made after it saw the
    change, and only then compensate, answering `Refused(OperationAborted)`; the held batch,
    released after the attempt returns, answers `Conflict` and writes nothing. The not-landed runs
    are there so an attempt cannot pass by never compensating.
  **(C) The counterexamples found in #657's first attempt, each as a regression.**
  - **C1, an empty part looks the same as somebody else's.** With a fixed clock, an empty part 1
    is committed. Then an empty re-upload of part 1 in the same millisecond and session epoch
    commits with an answer of `CommitUnknownResult` and is **not applied** (run once per
    `may_still_commit` value). Its record's bytes equal the prior's, because an empty part has no
    chunk ids of its own. The answer must not be `Committed` on the strength of those bytes. When
    the call returns, the attempt's slot is no longer held, or, for an in-doubt return, is still
    covered by the doubt it reports; the session's held-slot count equals its live attempts
    (`0016:1779`). Same for a same-number rival whose empty part landed in the same millisecond.
  - **C2, a torn read.** The commit is held (`may_still_commit: true`) and never applied. At
    **each** gap between the reads the attempt makes to classify it, in turn, the wrapper runs a
    real #895 Abort and then deletes the session's `sidx:` entries and `slot:` records (standing
    in for #659's teardown). The answer is never `Committed`, since no part exists. It is
    `NoSuchUpload` or an in-doubt error. A reaper cannot touch an `Open` session (`0016:1628`), so
    a slot that vanished proves the attempt's own release only if the session was still the same
    `Open@E` when the slot was found gone.
  - **C3, a landed commit, then a rival before the re-read.** The attempt's commit **lands** but
    answers `CommitUnknownResult { may_still_commit: false }`. A rival re-upload of the same part
    number then replaces the part before the re-read. The answer is `Committed`: its slot vanished
    while the session stayed `Open@E`, and only its own commit could do that. It is never an
    error claiming that no batch is in doubt and that its slot and staged entries are left, when
    its commit released both. **C3′, and the freed index is reused** (2026-10-20): as C3, and
    then a third attempt reserves the index the commit freed and holds it, all before the
    re-read, with the session still `Open@E`. The re-read finds the rival's part and a slot at
    that index carrying **another** `attempt_id` (`SlotRecord::attempt_id`,
    `multipart.rs:2796`). The answer is still `Committed`, and the third attempt's slot is
    byte-for-byte unchanged: the attempt sends no batch that requires or rewrites it. 0016 names
    that third attempt as a writer of the key (`0016:1625-1628`), so "the key is present" does
    not mean "my slot is still held".
  - **C4, empty parts and retirement** (2026-10-20). An obligation must name at least one chunk:
    `RetireObligation` refuses an empty `chunks` list and an empty payload
    (`multipart.rs:3752`; pinned by
    `crates/core/tests/multipart_retire_obligation.rs:467-473`), and this slice keeps that
    contract. So an obligation is installed only when the chunk list it would name is non-empty,
    and the slot release and `sidx:` deletes stay in the same batch either way. Shown by: an
    empty re-upload over an empty prior (C1's shape, every batch applied) commits with **no**
    `retire:` key written; an empty
    re-upload over a non-empty prior writes one obligation for the prior's chunks; and an empty
    part that loses to a same-number rival compensates with a batch that releases its slot and
    writes **no** obligation, answering `Refused(OperationAborted)`. That empty compensation's
    unknown outcome (landed; not landed with `may_still_commit: false`; held) has no obligation
    to re-read, so it is settled by the attempt's own slot and the session (see Design): each
    run ends with the slot released once and no `retire:` key, or an in-doubt error carrying
    the doubt.
  - An unknown outcome on the **compensation** batch (landed; not landed with
    `may_still_commit: false`; held) is settled by re-reading its own obligation, its own slot
    and the session (for an empty part, slot and session alone; C4). Each run ends with exactly
    one obligation and the slot released once, or an in-doubt error carrying the doubt.
  - Every loop of this slice (commit retry, compensation, the epoch chase after a rollback) has a
    stated maximum. With a wrapper that answers every batch `Conflict` unapplied, each ends in a
    typed error after **exactly** that many commits. The test counts them and compares against a
    **number written in the test**, never the imported constant, so changing a bound fails it. A
    run that spends the commit rounds then spends the compensation rounds (G), each counted.
  **(D) Re-upload and ceilings.**
  - Re-uploading a committed part number replaces it under `require(part == prior)` and puts the
    prior part's chunks in a `retire:bytes:` obligation in the same batch (when the prior part
    has at least one chunk; C4).
  - A part whose staging hit the per-part bound (`StagedAttempt::chunk_bound_exceeded`,
    `stage.rs:534`) answers `Refused(EntityTooLarge { .. })` and compensates what it staged in one
    batch, whether it reaches `commit_part` directly or through the composed `UploadPart`. No
    `part:` record exists afterwards: the attempt's `len()` and `digest()` describe only a prefix.
  - **The cumulative ceiling is a read-time check, and 0016 allows the race it leaves**
    (`0016:1441-1451`; `multipart.rs:5108`). A commit whose read of the session's `psum:`
    range shows it would take the cumulative staged chunks past `MAX_STAGED_CHUNKS` answers
    `Refused(EntityTooLarge { .. })` and compensates. Concurrent commits may overshoot by at most
    `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`. **Witness, forced by the wrapper:** with `psum:`
    records planted one chunk below the ceiling, two attempts on two different part numbers, each
    holding a one-chunk part, both read that one chunk of headroom (the wrapper holds the first
    commit until the second has committed, then releases it). Both commit, and the total is
    `MAX_STAGED_CHUNKS + 1`, inside the bound. A third commit after them refuses. Do **not** add a
    shared counter or a session-wide fence to stop the overshoot; 0016 rejects that
    (`0016:1686-1742`). Use a test-sized ceiling only if the function takes it as a parameter;
    otherwise plant `psum:` records to approach it.
  **(F) Every commit round is dispatched with the margin left, and a late landing is safe**
  (`stage.rs:376-378`, `0016:1726-1728`; Design § *What the lease check does and does not promise*).
  The clock advances during reads and retries; the wrapper moves it by a chosen amount on each read
  or batch.
  - **Boundary:** a commit round is sent when the slot lease still has at least the dispatch margin
    (`E_TX_MILLIS`, `multipart.rs:5118`) left at that moment. With exactly that much left it is sent;
    with one millisecond less it is not.
  - **Used up by the reads:** the summary scan and part read take the lease below that mark, so no
    commit is sent at all.
  - **Used up by retries:** `Conflict` rounds separated by slow reads take it below that mark before
    a later round. With nothing in doubt, no further commit is sent and the attempt compensates (G).
    The answer is a typed stop, never `Committed`.
  - **With a commit in doubt:** a commit answered `CommitUnknownResult { may_still_commit: true }`
    is outstanding when the lease runs short. No further commit and no compensation are sent, and the
    attempt returns the in-doubt error keeping the flag.
  - **On every leg of this file:** the wrapper fails the test if any part-commit batch is sent with
    less than that margin left on the lease that attempt last wrote.
  - **Delayed after dispatch.** A commit is sent with the margin left, and the wrapper holds it
    (`may_still_commit: true`) and moves the clock past the lease's expiry. So the batch passes the
    dispatch check and has not applied when the lease runs out. Run it four times. (i) The held
    batch lands before the attempt's re-read: `Committed`, with the records of (A). (ii) It lands
    after the attempt has returned: the in-doubt error keeping the flag, and at the end the part is
    durable and the slot released once. (iii) A test-local batch in the idle fence's shape
    (`require(mpu == Open@E)`, `require(slot == the bytes it read)`, the session moved to `Aborting`;
    `0016:1859-1863`; #625 owns the real one) lands first, then the held batch is released: that
    batch answers `Conflict`, no `part:` record exists, and the attempt's answer is `NoSuchUpload` or
    the in-doubt error. (iv) The held batch lands, then that fence batch, built from slot bytes read
    before the landing, is sent: the fence answers `Conflict`, the session stays `Open@E`, and the
    part is durable. In all four the log shows no compensation batch: an expired lease is never
    evidence that a commit did not land.
  **(G) A settled stop under a live session compensates** (`0016:1420-1431`, `:673`; decided at
  sign-off, 2026-10-30, see Design).
  - Each of these, with nothing in doubt, sends the live-session compensation: the quiesce renewal
    stopping with `Stop::CoverShort`, `RenewalRoundsSpent`, `LeaseLapsed` or `InvalidRequest`; the
    commit rounds spent; the lease short before a commit round (F). The store then holds one
    obligation for the attempt's own chunks (none for an empty part), no slot and no `sidx:` entry
    of the attempt. The answer is the typed error for the original stop, it keeps that stop's error
    class, and it reports that the slot was released.
  - **The cap witness:** `MAX_INFLIGHT_PARTS` + 1 attempts on one session each end in a settled
    stop. The session's held-slot count is 0 afterwards, and the next reserve succeeds instead of
    answering `SlowDown` (`stage.rs:1137`).
  - **Not compensated:** the wrapper's log shows no compensation batch, and the answer does not
    claim a release, for a renewal `StageError::InDoubt`, `Stop::SlotLost`, `Stop::EntryLost`,
    `Stop::Record`, `Stop::Store`, and a read that faults during classification.
  - **During that compensation:** a rollback to `Open@E'` is chased as in (B). A session found
    fenced answers `Refused(NoSuchUpload)` with no further write. A compensation that spends its
    rounds or stays in doubt returns its own typed error, which does not claim a release.
  **(H) The five gaps attempt 1's tests left**, each a named leg that fails under the edit listed
  in `Verification posture`.
  - **A commit held while a rollback lands.** The commit is held. Right after the attempt's slot
    read, the held commit lands and a Complete rolls the session back to `Open@E'`. The answer is the
    in-doubt error; no compensation is sent. (This is the slot-read gap of (B)'s read-gap leg, kept
    as its own named leg.)
  - **A held compensation under a teardown.** An empty part loses to a rival, its compensation is
    held, then an Abort and a teardown run. The answer is the in-doubt error, never
    `Refused(OperationAborted)`, because nothing of this attempt landed and the session is gone.
  - **The obligation re-read.** A compensation lands with an unknown outcome, then a client Abort
    runs before the re-read. Only the obligation re-read can settle this: the answer is not an
    in-doubt error. A second run plants other bytes at the obligation's key and ends in
    `Stop::ObligationKeyTaken`.
  - **The epoch chase's real bound.** One rollback beats the commit, then two more beat
    compensations: three rollbacks, `MAX_COMPLETE_ATTEMPTS`, the most a session can see. The next
    compensation lands, and the slot is released under the live session.
  - **The class of a composed error.** An in-doubt error from the composed `upload_part`, read
    through `wyrd_traits::classify` (`crates/traits/src/lib.rs:786-803`), is `Indeterminate`.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree, the tracker's acceptance
  line and `AGENTS.md:41-45`'s rule.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the second half of `UploadPart` as store-I/O functions in `wyrd-core`. That means
  the part commit and its last renewal; the classification of a lost or unknown commit; the
  live-session compensation, including against a rolled-back `Open@E'` and on a settled stop; the
  lease check before every commit round; the re-upload of a part
  number with the prior part's chunks retired; the per-part and cumulative ceiling refusals; and
  the composed `UploadPart`. Also: the writer-side constructor the part record lacks, with its
  `psum:` summary derived from it, the per-part `{chunks}` row of `RetireObligation` (which
  keeps refusing an empty list), the widened doc of `Refusal::OperationAborted` (no new variant),
  the corrected doc of `StageOutcome::Staged` (`stage.rs:363`), the docs paragraph, and the new test
  file. / out of scope:
  - the slot reserve, the owned intent, the drain fence, placement, the fragment writes and
    renewal itself (#932, #933), and any new `StageOutcome` variant (the doc fix above is the whole
    of this slice's edit to `stage.rs`, decided 2026-10-30). A stop during staging keeps leaving
    its residue for the reaper: no attempt is handed back to compensate with;
  - compensating after a definite store fault or a damaged record (see Design);
  - Complete, the segment committer and publication (#658, #653); draining any `retire:`
    obligation, the owned-`sidx:` walk and the terminal delete (#659); the reaper (#625);
    seeded DST races (#660);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's);
    any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.0% — 694 of 701 instrumentable changed lines executed (floor 80%); 701 of 1636 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 86 mutants tested in 6m: 69 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.43s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #927: add UploadPart commit, outcome classification, compensation, re-upload and ceilings; two reproduced builder-fixable concerns remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The core-only slice has explicit outcomes, adversarial histories and scope boundaries; the latest instructions require rejecting damaged renewal adoptions and independently checking attempt identity (`brief.md:295`, `brief.md:726`, `brief.md:728`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality: reverting production changes removes the API imported by the discriminator, so its compilation failure is not a reproduced behavioral red (`crates/core/tests/multipart_upload_part_commit.rs:69`; `reviewer-red.log:2`). |
| C3 Change | PASS | The five affected files stay within the authorized slice, including the staging documentation correction and living architecture update; no production caller or dependency change was added (`crates/core/src/multipart/stage.rs:363`, `crates/core/src/multipart/commit.rs:1364`, `docs/design/architecture/05-building-block-view.md:248`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the declared new-API exception supplies sufficient pre-fix evidence—independent stash/restore reproduces compilation failure before tests on the base and 60 passing tests with the patch, but establishes no behavioral red (`brief.md:327`; `reviewer-red.log:2`; `reviewer-green-restored.log:67`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Reject invalid epoch adoption before a settled renewal error can authorize compensation—three probes delete the slot and staging entry under damaged same-epoch session bytes instead of retaining them with a record error (`crates/core/src/multipart/commit.rs:834`, `crates/core/src/multipart/commit.rs:846`; `reviewer-probe.log:6`). |
| T1 Structure | PASS | The new functionality stays behind the existing metadata, placement and clock seams; validated record construction and batch construction remain separate, without a concrete production-backend dependency (`crates/core/src/multipart/commit.rs:192`, `crates/core/src/multipart/commit.rs:714`, `crates/core/src/multipart.rs:4197`). |
| T2 Shape | PASS | Record/summary consistency, nonempty retirement obligations and architecture currency have executable coverage; formatting, spelling, documentation lint and the 101-page render/link audit pass (`crates/core/src/multipart.rs:3074`, `crates/core/tests/multipart_upload_part_commit.rs:493`, `docs/design/architecture/05-building-block-view.md:248`; `reviewer-docs.log:3`). |
| T3 Runtime | PASS | Commit/compensation retries are finite, the dispatch checks use staging's clock, and store waits rely on the explicit backend termination contract; held-result histories execute in the passing suite (`crates/core/src/multipart/commit.rs:221`, `crates/core/src/multipart/commit.rs:925`, `crates/core/src/multipart/commit.rs:1221`, `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | FAIL | The required batch review remains unresolved: its three reports describe the same epoch-validation bypass, independently reproduced here; its findings therefore lack the fixed-or-rejected disposition required by the rubric (`gate-logs/T4-batch-review.log:10`; `AGENTS.md:221`; `crates/core/src/multipart/commit.rs:834`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Bind the slot invariant to the initiating attempt—the guard currently accepts A's cleanup using B's recorded slot bytes, so its claimed independent ownership check does not detect the forbidden substitution (`crates/core/tests/multipart_upload_part_commit.rs:605`, `crates/core/tests/multipart_upload_part_commit.rs:5258`; `reviewer-guard-probe.log:6`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected core-only slice meets the intended multipart milestone—its exercised store protocol does not establish end-to-end S3 behavior, whose production caller belongs to the later wiring work (`crates/core/src/multipart/commit.rs:179`, `crates/core/src/multipart/commit.rs:1364`; `brief.md:389`). |

Source citations are relative to `$PDCA_TARGET`; brief and evidence citations are relative to this report's directory. The disposable target matches the patch: reverse-apply checking succeeds, and all five affected file hashes match the patch's postimages. No target-state or host-toolchain fault explains either finding.

**1. Renewal failure bypasses the new epoch validation (C5).** The error return at `crates/core/src/multipart/commit.rs:834` precedes the comparison at `crates/core/src/multipart/commit.rs:846`. Staging can adopt changed `Open` bytes without comparing epochs (`crates/core/src/multipart/stage.rs:1820`), and `renewal_stopped` passes those bytes directly to compensation (`crates/core/src/multipart/commit.rs:1144`). Consequently, a failed renewal can authorize cleanup under a session the success path rejects as damaged.

The independent probes stage one nonempty part, replace the session with different bytes at the same epoch during a conflicting renewal, then separately force `RenewalRoundsSpent`, `CoverShort`, and `LeaseLapsed`. All three return `CommitError::Renewal` with `slot_released: true`, send one compensation, and leave zero own slots and zero owned entries (`reviewer-probe.log:6`, `reviewer-probe.log:14`, `reviewer-probe.log:21`). This violates the explicit damaged-record stop rule at `crates/core/src/multipart/commit.rs:125` and the latest requirement at `brief.md:726`. Validate the adopted session before any error branch can compensate, while preserving any renewal uncertainty. Add failing regressions for these error outcomes; the current adoption regression allows the next renewal to succeed (`crates/core/tests/multipart_upload_part_commit.rs:5150`). The three frozen batch-review reports are duplicates of this one defect, not three independent defects.

**2. The slot guard authenticates the current owner, not the caller (T5).** At `crates/core/tests/multipart_upload_part_commit.rs:605`, `claimed` still comes from the batch. Comparing it with the current slot owner only establishes that the supplied bytes belong to whoever now occupies that index. It does not establish that they belong to the attempt submitting the batch. The new negative test supplies A's stale bytes after B reserves the index (`crates/core/tests/multipart_upload_part_commit.rs:5270`); the dangerous substitution supplies B's current bytes in A's cleanup.

The independent probe reserves A, frees its index, reserves B at that index, and builds A's cleanup with A's owned-entry deletes but B's slot bytes. `check_slot_bytes` returns `Ok(())` (`reviewer-guard-probe.log:6`). Supply the initiating attempt identity independently of the batch and current slot occupant, then require the guard to reject this substitution while accepting B's own batch. This is a test-fidelity defect, not a demonstrated production ownership violation: the production builders currently take slot bytes directly from the attempt (`crates/core/src/multipart/commit.rs:730`, `crates/core/src/multipart/commit.rs:759`). It matters because the brief explicitly relies on this invariant where the normal classification prevents a behavioral mutant from being reached (`brief.md:373`, `brief.md:728`).

**Verification supports the exercised paths, with the limits stated above.** Independent reruns completed:

- `cargo test --offline --locked -p wyrd-core --test multipart_upload_part_commit`: 60/60 pass. Stashing tracked production changes while retaining the new discriminator produces the expected missing-API compiler errors; restoring the stash returns to 60/60 pass. The tracked diff was checked unchanged (`reviewer-red-green.log:1`, `reviewer-green-restored.log:67`).
- `cargo clippy -p wyrd-core --test multipart_upload_part_commit -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`, `typos`, documentation lint/render, and `cargo xtask statics`: pass. Both named external dependencies, spelling and docs rendering, were exercised without a substitute.
- Four diagnostic assertions fail against the unchanged production patch: three renewal-error variants and the slot-guard substitution. Their source artifacts are `reviewer-probe.rs` and `reviewer-guard-probe.rs`; they copy the supplied test harness and append the probes. They were run as temporary integration-test targets and moved back into the review directory afterward. The target retains only the supplied patch changes.

The instance-scoped gate commands were adjudicated from their complete frozen logs; this review does not claim to have independently rerun the entire workspace CI, mutation campaign, coverage measurement or TiKV feature builds:

- C4-ci: **PASS**, completed workspace checks and DST tests through the final success line (`gate-logs/C4-ci.log:4555`). C4-verify: **UNVERIFIABLE**, agrees with the independent missing-API result (`gate-logs/C4-verify.log:55`).
- C4-diff-cov: **PASS**, 694/701 instrumentable changed lines executed, 99.0%; 935 changed lines were unscored (`gate-logs/C4-diff-cov.log:106`). C5-mutants: **PASS**, 69 caught and 17 unviable out of 86; the log does not demonstrate the brief's separately named temporary-edit reds (`gate-logs/C5-mutants.log:13`).
- T4-batch-review: **FAIL**, one distinct confirmed defect reported three times (`gate-logs/T4-batch-review.log:10`). host-tikv: **PASS**, both requested feature builds complete (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`).
- T4-contribution: **N/A**, `pr-description.md` is absent by design; its substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). This is not a missing-evidence escalation.

**Prior art was checked by affected paths.** Path-filtered merged history at base `376b106` confirms the record, Create, Abort, staging and renewal predecessors; the new commit test has no history (`reviewer-prior-art-merged.json:2`). The GitHub search for closed, unmerged PRs returned 19; fetching every PR's changed-file list found no match in the affected multipart paths or architecture file (`reviewer-prior-art-closed.json:2`). Rejected local attempts and their overriding decisions are recorded in `brief.md:640` through the final carry-forward. The accepted size decision, dispatch-only lease promise and tracked DST deferral (#660 at `crates/core/src/multipart/commit.rs:185`) remain settled. The already acknowledged non-Open renewal-refusal gap is not re-raised (`brief.md:726`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #927 part commit (iteration 7)

I refuted this patch on three counts: T4's blocking finding is real, one legal history gets the wrong answer, and damage to the slot gets a false `Committed`. I found no legal history that gives a false `Committed` or sends a compensation while a commit may still land. All probes ran on a scratch copy of the patched tree, over the test file's own rig and real redb store. The session-damage and slot-erase histories are not legal ones.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:833-834`: **T4's blocking finding reproduces. The renewal's error path skips the epoch check at `:846`.** Failing case: the renewal's first round answers `Conflict` while an `Open` record at the same epoch under other bytes is planted, so the renewal adopts it. Every later round then conflicts. Actual result: `Err(Renewal { RenewalRoundsSpent, slot_released: true })`, with 1 compensation landed under the damaged bytes, slots=0 and obligations=1. A second run with landed rounds that never cover gives the same result with `CoverShort`. Expected: `Stop::Record(SessionEpochNotRisen)` with no compensation, as the Design's table says for a damaged record, and as the sign-off asked ("is not adopted"). The doc claim at `commit.rs:91-92` and `:841-842`, that the commit judges the adoption before anything else happens, is false on this path. Trap for the fix: moving the check above the `match` unchanged would replace a renewal `StageError::InDoubt` with a plain stop and drop its doubt. Only the arms that would go on to send a batch should be gated: `Covered`, and an error where `compensates_after` is true.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1061-1066` and `:1337-1347`: **a legal terminal delete is reported as damage, because the slot read came before the session read.** Failing case at the commit: Abort fences the session and the commit answers `Conflict`. The reads find the part, the session (`Aborting`) and the slot (held). The terminal delete then lands, and it removes the session, slots and owned entries in one batch, as `0016:674` says. The last session read finds nothing. Actual result: `Stopped { Record(SessionRecordMissing), slot_released: false }`. Expected: `Refused(NoSuchUpload)`. The message also says "what it staged stays in the store", while the probe found slots=0. The compensation gives the same wrong answer when the delete lands right after its slot read, because the extra read at `:1337` runs only with a doubt. This breaks the brief's own rule that a slot read made before the read that found the change proves nothing. The fix is to read the slot once more before calling held-plus-gone damage, in both places; a slot still held after that is damage, as the sign-off decided. The test comment and count at `crates/core/tests/multipart_upload_part_commit.rs:4687-4688` ("no second lap of reads") pin the defect and would need updating.

- NEEDS-HUMAN [human] — `crates/core/src/multipart/commit.rs:1012-1018`: **the own-commit proof answers `Committed` for a commit that never landed when the slot alone is damaged.** Failing case: a commit is held, or answered unknown with `may_still_commit: false`, and never applied; the slot key is then erased with the session untouched. Actual result in both runs: `Ok(Committed)`, with no `part:` record and the owned entry still there. The held batch, released afterwards, answers `Conflict`. The compensation has the same gap at `commit.rs:1290`: an unknown, unapplied compensation plus an erased slot answers `Refused(OperationAborted)` with 0 obligations and the owned entry still there, though the `Refusal::OperationAborted` doc (`crates/core/src/multipart.rs`) says the chunks were put in an obligation. No legal writer does this, so it is the same class as the two damage histories that rejected attempt 6, but here the answer is a false success. For a commit that required the key absent, one more read of the part key after the slot read would catch it: a landed commit must leave a record there. I tagged this for a human because the sign-offs said to keep the own-commit proof and the classification design as they are.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:5142-5173`: **the lower-epoch half of sign-off item 2 has no test.** Changing `session.epoch <= staged.epoch` to `==` at `commit.rs:846` leaves all 60 tests passing, because the one leg plants only a same-epoch record. The C5 result of 69 caught and 17 unviable does not show this edit, since it is a hand edit rather than one of its mutants. Production is right today: staging at epoch 2 and planting the old `Open` record at epoch 0 gives `SessionEpochNotRisen { found: 0, required: 2 }` and no commit. That probe can be added as the missing leg.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:843-845`: **the known gap is recorded by citing `build-notes.md`, a bundle file that will not exist in the repo after merge, and it has no tracking issue.** The gap is a renewal answering a non-`Open` record at an unrisen epoch as `NoSuchUpload`. The sign-off accepted the behaviour, so I am not re-raising that. The rubric treats a deferral as settled only when it names an issue (`// deferred: #N`), so the comment should cite one.

- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log:6`: **the deep review the last sign-off required is not shown by this evidence.** The iteration 6 caveat said the next sign-off "needs a confirmed high-effort review of the final patch". This row ran three passes in 36.92 s in total, against 120-320 s per pass for the earlier high-effort run. Nothing in my inputs records an effort level. Its three findings are real, but they are all the same bug. Treat a future clean T4 row as weak evidence until the effort is confirmed.

## Attempted, could not refute

- **Test run:** I re-ran `cargo test -p wyrd-core --test multipart_upload_part_commit`: 60 passed. The tests call the real `wyrd_core::multipart::commit` functions over `RedbMetadataStore`, not a copy of them.
- **Red→green:** `C4-verify` is UNVERIFIABLE because the test cannot compile on the base, as the brief declared in advance. As a partial substitute I made eight hand edits. The six at `commit.rs:1070`, `:531-534`, `:1337`, `:1082`, `:856` and `:1281` each failed at least one test. The two that left all 60 passing were `:846`, the gap above, and removing the first session read's `session_seen` at `:987`. From reading the code, that second edit mostly costs one extra lap of reads, but I did not prove that it never changes an answer.
- **Full gate:** `gate-logs/C4-ci.log` shows `typos`, docs, fmt, clippy, workspace tests, `cargo-machete`, `cargo deny`, the statics gate and the DST tests all ran, so the earlier incomplete-CI concern does not apply to this run. I read this log and did not re-run it.
- **Commit loop, by reading:** I found no break in the held-commit settling order, slot reuse by a third attempt, retries of the same batch, the read-only lap, or the loop bounds.
- **Compensation, by reading:** I found no break in the epoch chase, the obligation key shared by a commit and a compensation, or the psum ceiling.
- **Legal session rewrites:** a same-epoch `Open` rewrite that would make `SessionEpochNotRisen` fire wrongly did not turn up. The `Completing` cursor rewrites keep a risen epoch relative to the `Open@E` a batch requires.
- **Staged repair and owned entries:** the commit batch does not require the `sidx:` bytes. That is safe as far as I can see, since the staged re-place row of `0016` rewrites only `part:` records.

## Limits

- Nothing ran on FoundationDB or TiKV.
- I did not probe a damaged session adopted during staging, which the `staged` baseline at `commit.rs:822` would not catch.
- Seeded DST coverage is deferred to #660, so I did not raise it.
- I ran one `ls` on the results directory named in the T4 log, which is outside my inputs. It showed file names only. I did not open `build-notes.md` or any other file there.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:834`: Settled renewal errors bypass the adopted-session epoch check at line 846. If the first renewal conflicts and adopts changed `Open` bytes at the same/lower epoch, then subsequent renewals exhaust their rounds (or end `CoverShort`), `renewal_stopped` sends compensation under those damaged bytes. That batch can delete the slot and owned entries and install retirement obligations instead of returning `SessionEpochNotRisen` without compensation. Validate the adoption before any compensation, while preserving an in-doubt renewal's uncertainty. Extend `crates/core/tests/multipart_upload_part_commit.rs:5142` to cover failed renewals; its current script lets renewal succeed. This confirms the single defect repeated by the three frozen T4 findings.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:606`: The slot guard compares the claimed attempt against the slot's current owner, not the attempt submitting the batch. After A releases an index and B reserves it, a faulty batch from A requiring B's recorded bytes passes both checks; those are exactly the bytes accepted as `good` at line 5278. Conversely, the regression rejects A's own old bytes, which would safely conflict in the store. Supply the submitting attempt identity independently of the batch and slot's current occupant, and test that A using B's bytes is rejected. Otherwise this guard does not establish the claimed invariant that every batch requires bytes its own attempt wrote.

No additional actionable reuse, simplification, or efficiency findings. Review used the target source and frozen gate evidence; no target files were changed and no gates were re-run.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether the declared new-API exception supplies sufficient pre-fix evidence—independent stash/restore reproduces compilation failure before tests on the base and 60 passing tests with the patch, but establishes no behavioral red (`brief.md:327`; `reviewer-red.log:2`; `reviewer-green-restored.log:67`).
- [ ] C5 Causal adequacy — Reject invalid epoch adoption before a settled renewal error can authorize compensation—three probes delete the slot and staging entry under damaged same-epoch session bytes instead of retaining them with a record error (`crates/core/src/multipart/commit.rs:834`, `crates/core/src/multipart/commit.rs:846`; `reviewer-probe.log:6`).
- [ ] T5 Judgment — Bind the slot invariant to the initiating attempt—the guard currently accepts A's cleanup using B's recorded slot bytes, so its claimed independent ownership check does not detect the forbidden substitution (`crates/core/tests/multipart_upload_part_commit.rs:605`, `crates/core/tests/multipart_upload_part_commit.rs:5258`; `reviewer-guard-probe.log:6`).
- [ ] Validation — fitness-to-purpose — Decide whether the corrected core-only slice meets the intended multipart milestone—its exercised store protocol does not establish end-to-end S3 behavior, whose production caller belongs to the later wiring work (`crates/core/src/multipart/commit.rs:179`, `crates/core/src/multipart/commit.rs:1364`; `brief.md:389`).
- [ ] `crates/core/src/multipart/commit.rs:833-834`: **T4's blocking finding reproduces. The renewal's error path skips the epoch check at `:846`.** Failing case: the renewal's first round answers `Conflict` while an `Open` record at the same epoch under other bytes is planted, so the renewal adopts it. Every later round then conflicts. Actual result: `Err(Renewal { RenewalRoundsSpent, slot_released: true })`, with 1 compensation landed under the damaged bytes, slots=0 and obligations=1. A second run with landed rounds that never cover gives the same result with `CoverShort`. Expected: `Stop::Record(SessionEpochNotRisen)` with no compensation, as the Design's table says for a damaged record, and as the sign-off asked ("is not adopted"). The doc claim at `commit.rs:91-92` and `:841-842`, that the commit judges the adoption before anything else happens, is false on this path. Trap for the fix: moving the check above the `match` unchanged would replace a renewal `StageError::InDoubt` with a plain stop and drop its doubt. Only the arms that would go on to send a batch should be gated: `Covered`, and an error where `compensates_after` is true.
- [ ] `crates/core/src/multipart/commit.rs:1061-1066` and `:1337-1347`: **a legal terminal delete is reported as damage, because the slot read came before the session read.** Failing case at the commit: Abort fences the session and the commit answers `Conflict`. The reads find the part, the session (`Aborting`) and the slot (held). The terminal delete then lands, and it removes the session, slots and owned entries in one batch, as `0016:674` says. The last session read finds nothing. Actual result: `Stopped { Record(SessionRecordMissing), slot_released: false }`. Expected: `Refused(NoSuchUpload)`. The message also says "what it staged stays in the store", while the probe found slots=0. The compensation gives the same wrong answer when the delete lands right after its slot read, because the extra read at `:1337` runs only with a doubt. This breaks the brief's own rule that a slot read made before the read that found the change proves nothing. The fix is to read the slot once more before calling held-plus-gone damage, in both places; a slot still held after that is damage, as the sign-off decided. The test comment and count at `crates/core/tests/multipart_upload_part_commit.rs:4687-4688` ("no second lap of reads") pin the defect and would need updating.
- [ ] `crates/core/src/multipart/commit.rs:1012-1018`: **the own-commit proof answers `Committed` for a commit that never landed when the slot alone is damaged.** Failing case: a commit is held, or answered unknown with `may_still_commit: false`, and never applied; the slot key is then erased with the session untouched. Actual result in both runs: `Ok(Committed)`, with no `part:` record and the owned entry still there. The held batch, released afterwards, answers `Conflict`. The compensation has the same gap at `commit.rs:1290`: an unknown, unapplied compensation plus an erased slot answers `Refused(OperationAborted)` with 0 obligations and the owned entry still there, though the `Refusal::OperationAborted` doc (`crates/core/src/multipart.rs`) says the chunks were put in an obligation. No legal writer does this, so it is the same class as the two damage histories that rejected attempt 6, but here the answer is a false success. For a commit that required the key absent, one more read of the part key after the slot read would catch it: a landed commit must leave a record there. I tagged this for a human because the sign-offs said to keep the own-commit proof and the classification design as they are.
- [ ] `crates/core/tests/multipart_upload_part_commit.rs:5142-5173`: **the lower-epoch half of sign-off item 2 has no test.** Changing `session.epoch <= staged.epoch` to `==` at `commit.rs:846` leaves all 60 tests passing, because the one leg plants only a same-epoch record. The C5 result of 69 caught and 17 unviable does not show this edit, since it is a hand edit rather than one of its mutants. Production is right today: staging at epoch 2 and planting the old `Open` record at epoch 0 gives `SessionEpochNotRisen { found: 0, required: 2 }` and no commit. That probe can be added as the missing leg.
- [ ] `crates/core/src/multipart/commit.rs:843-845`: **the known gap is recorded by citing `build-notes.md`, a bundle file that will not exist in the repo after merge, and it has no tracking issue.** The gap is a renewal answering a non-`Open` record at an unrisen epoch as `NoSuchUpload`. The sign-off accepted the behaviour, so I am not re-raising that. The rubric treats a deferral as settled only when it names an issue (`// deferred: #N`), so the comment should cite one.
- [ ] `gate-logs/T4-batch-review.log:6`: **the deep review the last sign-off required is not shown by this evidence.** The iteration 6 caveat said the next sign-off "needs a confirmed high-effort review of the final patch". This row ran three passes in 36.92 s in total, against 120-320 s per pass for the earlier high-effort run. Nothing in my inputs records an effort level. Its three findings are real, but they are all the same bug. Treat a future clean T4 row as weak evidence until the effort is confirmed.
- [ ] `crates/core/src/multipart/commit.rs:834`: Settled renewal errors bypass the adopted-session epoch check at line 846. If the first renewal conflicts and adopts changed `Open` bytes at the same/lower epoch, then subsequent renewals exhaust their rounds (or end `CoverShort`), `renewal_stopped` sends compensation under those damaged bytes. That batch can delete the slot and owned entries and install retirement obligations instead of returning `SessionEpochNotRisen` without compensation. Validate the adoption before any compensation, while preserving an in-doubt renewal's uncertainty. Extend `crates/core/tests/multipart_upload_part_commit.rs:5142` to cover failed renewals; its current script lets renewal succeed. This confirms the single defect repeated by the three frozen T4 findings.
- [ ] `crates/core/tests/multipart_upload_part_commit.rs:606`: The slot guard compares the claimed attempt against the slot's current owner, not the attempt submitting the batch. After A releases an index and B reserves it, a faulty batch from A requiring B's recorded bytes passes both checks; those are exactly the bytes accepted as `good` at line 5278. Conversely, the regression rejects A's own old bytes, which would safely conflict in the store. Supply the submitting attempt identity independently of the batch and slot's current occupant, and test that A using B's bytes is rejected. Otherwise this guard does not establish the claimed invariant that every batch requires bytes its own attempt wrote.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- [ ] **The lease fix assumes a deadline the metadata seam does not guarantee.** `brief.md:174-188` calls `E_TX_MILLIS` the commit's transaction deadline and tests only the lease remaining when a batch is sent; `brief.md:403-412` uses that to promise coverage throughout the commit. On the target, `crates/core/src/multipart.rs:5113-5118` defines a five-second FDB envelope, but `crates/traits/src/lib.rs:1332-1345` guarantees termination, not completion within five seconds, and explicitly permits application after a timeout. FDB's five seconds start after obtaining a read version; its configured default timeout is ten seconds (`crates/metadata-fdb/src/lib.rs:369-388`). TiKV's default is fifty seconds (`crates/metadata-tikv/src/lib.rs:171-181`, applied at `:1359-1365`). A batch sent with exactly five seconds of lease left can therefore pass every proposed dispatch assertion and still apply after expiry. Revise the brief to identify the actual enforceable timing contract, and require a delayed-after-dispatch witness. If this slice guarantees only sufficient lease at dispatch, narrow the promised invariant explicitly; a caller timeout alone cannot guarantee when an outstanding batch applies.
- [ ] **The definition of “settled not landed” lacks the read ordering needed to make it true.** `brief.md:458-466` accepts an irreversibly changed session/part precondition “with the own slot still at its last-written bytes”; `brief.md:494-495` permits reading the session after the slot and part. Counterexample: an unknown commit is held; the slot read sees this attempt still holding it; the commit then lands and a same-number rival replaces its part; the part read sees the rival and the session read still sees `Open@E`. Those observations satisfy the stated settlement conditions even though this attempt committed. C3 (`brief.md:117-126`) puts the rival before the re-read, so it does not force this gap. H's rollback history (`:208-210`) exposes the analogous session race but conflicts with the Design's unqualified settlement rule. The target staging code explicitly guards this class by reading the slot again after observing a session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise the proof to require an own-slot observation **after** the irreversible precondition change, or retain doubt, and add the held-commit → rival-between-reads history with an assertion that it cannot be classified as a compensated loser. Otherwise Do can follow the Design and compensate a commit that landed.
- [ ] size backstop — this slice is behaving oversized: patch is 325 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Human decision at sign-off: return to Plan and split this slice. Why: attempt 7's patch is 333 KB against the 100 KB signal, up from 225 KB when the split was first overridden, and each rebuild still finds something new. This decision ends the earlier "do not split" override. Author the split in Plan (`pdca split`). Findings on attempt 7 for the split to place (none was decided at this sign-off): 1. Legal history, wrong answer: a terminal delete landing after the slot read reports `Stop::Record(SessionRecordMissing)` instead of `Refused(NoSuchUpload)` (`crates/core/src/multipart/commit.rs:1061-1066`, `:1337-1347`). The adversary's suggested fix is to read the slot once more before calling it damage. `crates/core/tests/multipart_upload_part_commit.rs:4687-4688` pins the defect. 2. T4 blocking: a settled renewal error bypasses the adopted-session epoch check, so compensation runs under damaged session bytes (`commit.rs:834`, `:846`). Do not drop a renewal `InDoubt`'s doubt when fixing this. 3. Damage only, false success: with the slot erased and the session untouched, the own-commit proof answers `Committed` with no part record (`commit.rs:1012-1018`); the compensation has the same gap (`:1290`). Whether to fix this is still open. 4. Test gaps: the slot guard accepts A's cleanup carrying B's slot bytes (`multipart_upload_part_commit.rs:605`), and no test covers a lower-epoch adoption (`commit.rs:846`). 5. `commit.rs:843-845` records a known gap by citing `build-notes.md`; it needs a tracking issue. 6. Review depth: the T4 row ran three passes in 36.92 s in total, with no effort level recorded, so it does not show the high-effort review the last sign-off required.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
