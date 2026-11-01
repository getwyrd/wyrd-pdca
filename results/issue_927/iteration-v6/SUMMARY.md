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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.2% — 660 of 665 instrumentable changed lines executed (floor 80%); 665 of 1546 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 83 mutants tested in 6m: 66 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.33s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #927’s core UploadPart commit, outcome classification, compensation, re-upload and ceilings: no new builder-fixable defect found; verification, the expressly unresolved missing-session policy, and fitness-to-purpose need human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The latest carry-forward defines an actionable correction: a non-Open record at the same/lower epoch is damage, while the earlier dispatch-only and scope decisions remain settled (brief.md:707; brief.md:714; target/crates/core/src/multipart/commit.rs:660). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no pre-existing commit API; independently reverting production while retaining the discriminator produces compile errors before any test executes, not a behavioral red (brief.md:321; reviewer-red-green.log:6). |
| C3 Change | PASS | The five-file patch stays within the authorized core slice, preserves the staged-fragment handoff in atomic batches, and documents the new writer without adding a production caller (target/crates/core/src/multipart/commit.rs:698; target/docs/design/architecture/05-building-block-view.md:248). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared net-new verification exception — 57 tests independently pass after restoration, but the pre-fix discriminator cannot compile, so a behavioral base-to-patch red→green is not established (brief.md:328; reviewer-red-green.log:32; reviewer-red-green.log:115; gate-logs/C4-verify.log:43). |
| C5 Causal adequacy | PASS | The latest fix rejects invalid epoch evidence before it can settle a doubtful batch; prior bytes recurring under repair also retain doubt. The exercised regressions and two independently caught classification mutants support the correction; no optional-capability symptom guard was added (target/crates/core/src/multipart/commit.rs:660; target/crates/core/src/multipart/commit.rs:943; reviewer-mutants.log:5). |
| T1 Structure | PASS | Core depends on the metadata/chunk-store traits and caller clock; writer constructors preserve decode validation, and the compensation path consumes a validated non-empty obligation payload (target/crates/core/src/multipart/commit.rs:783; target/crates/core/src/multipart.rs:3057; target/crates/core/src/multipart.rs:4180). |
| T2 Shape | PASS | Public behavior is documented in living architecture, persisted values retain canonical decode/encode identity, and formatting, core Clippy and documentation checks pass independently (target/crates/core/tests/multipart_upload_part_commit.rs:485; target/docs/design/architecture/05-building-block-view.md:248; reviewer-checks.log:3; reviewer-checks.log:147). |
| T3 Runtime | PASS | Retry counts, dispatch-margin boundaries, delayed landing and compensation are exercised; correctness clock reads use the staging clock, and store waits inherit the trait’s termination contract (target/crates/core/src/multipart/commit.rs:887; target/crates/core/src/multipart/commit.rs:1165; target/crates/traits/src/lib.rs:1333; reviewer-red-green.log:115). |
| T4 Contribution | PASS | Independent affected-path history and all 19 closed/unmerged PR file lists reveal no competing prior implementation; the architecture update is present. Contribution-artifact auditing is N/A until its mandatory publish rerun (reviewer-prior-art.json:10; target/docs/design/architecture/05-building-block-view.md:248; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN | Decide whether a missing mpu record with the attempt’s slot still held is terminal deletion or damage — the exercised branch settles doubt and returns NoSuchUpload, although terminal deletion removes surviving slots atomically; the brief expressly leaves this policy undecided (brief.md:712; target/crates/core/src/multipart/commit.rs:653; target/crates/core/tests/multipart_upload_part_commit.rs:4577; target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:582). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this core-only slice as fit for its intended handoff — tests exercise real redb plus fault-injecting doubles, while production UploadPart wiring and end-to-end completion/reclamation remain the expressly scoped follow-ups (brief.md:389; target/crates/core/src/multipart/commit.rs:172; target/docs/design/architecture/05-building-block-view.md:248). |

The implementation meets the reviewed legal-history invariants. Commit and compensation require the attempt’s own last-written slot bytes, so competing rounds cannot both release it. A held commit is settled only after a later slot observation follows evidence that an irreversible precondition failed; a changed prior part alone supplies no such evidence. Damage at an unchanged or lower session epoch preserves the unknown outcome. These properties are grounded at target/crates/core/src/multipart/commit.rs:714, :743, :955, :964 and :660, and exercised by the read-gap, repaired-session and repair-away-and-back tests at target/crates/core/tests/multipart_upload_part_commit.rs:2709, :4147 and :4797.

Independent verification used the disposable target, whose recorded base is 376b106248d2ac2e816c5d258609e04de09a425f. I stashed only the production/documentation patch, retained the new integration test, ran it, restored the stash, and reran it: the base failed compilation with four missing-API errors; the restored patch passed all 57 tests. The original patch still passes `git apply --reverse --check ../patch.diff`. No implementation edits were made. Evidence is in reviewer-red-green.log:1 and :115.

The independent scanner run passed `git diff --check`, `cargo fmt --all -- --check`, `cargo clippy --offline --locked -p wyrd-core --all-targets`, `typos`, docs lint, the 101-page render/link audit, the real repository guards invoked by `cargo xtask ci-dry-run`, and `cargo xtask statics` (reviewer-checks.log:1, :147, :149, :151, :154, :159, :177). The dry run was used for its actual guards, not counted as a workspace build/test run. Both declared external dependencies, spelling and rendering, were exercised without substitutions.

The focused independent mutation run selected `judge_session` from the bundle diff and ran the new integration suite in a scratch copy: two viable mutants were caught and one was unviable (reviewer-mutants.log:1). Inverting the epoch comparison caused 17 behavioral test failures, including held-commit damage, held-compensation damage and repaired-mid-round histories (pdca-reviewer-927-scratch/mutants.out/log/crates__core__src__multipart__commit.rs_line_660_col_23.log:476; :494; :496; :636). This is mutation evidence, not a substitute for the unavailable pre-feature behavioral red.

All supplied frozen gate logs were inspected. The instance-scoped wrapper scripts are not part of this target; the full workspace CI, full diff-coverage calculation, complete 83-mutant campaign, batch-review wrapper and TiKV feature checks below are adjudicated from those logs rather than claimed as independent reruns:

- C4-ci — PASS: the log includes fmt, Clippy, build/test, dependency checks, conformance and DST, ending with all checks passed (gate-logs/C4-ci.log:30; :3900; :3926; :4173; :4552).
- C4-verify — NEEDS-HUMAN for the net-new exception described above: 57 green tests, followed by a reverted-base compile failure and exit 77, with no behavioral red (gate-logs/C4-verify.log:14; :16; :43).
- C4-diff-cov — PASS on its stated measure: 660/665 instrumentable changed lines, 99.2%; 881 other changed lines were unscored, not claimed covered (gate-logs/C4-diff-cov.log:83; :101).
- C5-mutants — PASS on the captured campaign: 66 caught and 17 unviable out of 83; no surviving viable mutant is reported (gate-logs/C5-mutants.log:13).
- T4-batch-review — PASS on the recorded aggregate: zero blocking/rejected/noise findings. Its short summary does not expose individual review reasoning; it did not replace this source review (gate-logs/T4-batch-review.log:10; brief.md:698).
- T4-contribution — N/A: the PR description is intentionally not yet drafted; substantive contribution checks are owed at publish (gate-logs/T4-contribution.log:10).
- host-tikv — PASS on compilation/lint evidence: both feature-enabled Clippy commands finished successfully; the log does not claim a live TiKV runtime test (gate-logs/host-tikv.log:2; :112; :211).

The outstanding missing-session decision is evidenced, not a request to investigate. The passing test deletes only mpu while leaving the attempt’s slot, then observes NoSuchUpload and a held batch conflicting while the record stays absent (target/crates/core/tests/multipart_upload_part_commit.rs:4577). Proposal 0016’s terminal-delete contract removes the session and surviving slots together (target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:582). Thus this fixture does not demonstrate a legal terminal deletion with a remaining slot. Whether such absence must instead retain doubt as repairable damage is the human policy decision expressly reserved by brief.md:712; this review does not route an unauthorized behavior change back to the builder.

Prior-art checking was by all five affected file paths against history reachable from the brief’s base, followed by inspection of every changed-file list for the 19 closed/unmerged PRs returned by GitHub. Neither new file has merged history; existing-file history contains the record, Create, Abort and staging work described in the brief; none of the closed/unmerged PRs touches an affected path (reviewer-prior-art.json:1). Rejected local attempts are accounted for by the brief’s carry-forwards, including the latest epoch defect. Settled decisions about size, dispatch-only leases, retryable live-session loss and rollback residue are not reopened (brief.md:714). Seeded DST remains the explicitly tracked #660 deferral (target/crates/core/src/multipart/commit.rs:178), rather than a new finding.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #927 UploadPart commit (iteration 6)

**Result:** I could not refute the classification on any legal history. No false `Committed`, and no compensation while a commit can still land. What I did find: two damage histories that get around the epoch rule this rebuild added, three error paths no test protects, and one gate pass that is weak evidence. All probes ran on a scratch copy of the patched tree (`cargo test -p wyrd-core --test multipart_upload_part_commit`, 57 passed before any edit).

## Findings

- NEEDS-HUMAN [human] — **The quiesce renewal (the attempt's last lease renewal before the commit) does not apply this rebuild's epoch rule, so the module doc's claim is wrong.** `crates/core/src/multipart/commit.rs:88-89` says each session read "is judged the same way (`judge_session`)". The renewal uses a different function with the same name, `crates/core/src/multipart/stage.rs:1103-1112`, reached through `stage.rs:1797-1800`. It has no epoch check, and `commit.rs:806-815` trusts its answer. Three probes, each planting the record after staging and before `commit_part`:
  - An `Aborting` record at the same or a lower epoch gives `Ok(Refused(NoSuchUpload))`, no compensation and `slots=1`. Once the record is repaired, the session is `Open` and the slot stays held, so this attempt never gives it back (the reaper does not walk an `Open` session).
  - An `Aborting` record at the same epoch, planted right after a renewal round is held, also gives `Refused(NoSuchUpload)`. The held renewal then lands after the repair (`Committed`), which is the "damage settled a held batch" shape iteration 5 rejected, here for a renewal batch rather than a commit.
  - An `Open` record at a lower epoch, with the session at epoch 2, is adopted: the answer is `Committed` with `part.session_epoch=0`. An `Open` record under other bytes at the same epoch is also adopted and answers `Committed`. `commit.rs:92` calls both of these damage.

  This takes damage to reach, not a legal history, and no false `Committed` results: the part did land. It needs a human because the brief limits this slice's `stage.rs` edit to one doc fix, and the refusal case can't be told from a real fence without another read or a `stage.rs` change. A check in `commit.rs` comparing the epoch before and after the renewal would catch only the adopted-`Open` case. The decision is whether to extend the rule here, record the gap, or at least correct the doc at `commit.rs:88-89`. The quiesce-rollback tests, such as `crates/core/tests/multipart_upload_part_commit.rs:1759`, cover only the legal rollback.

- NEEDS-HUMAN [human] — **The undecided missing-`mpu:` case: new evidence that it can only be damage.** Iteration 5 left this open and I did not treat it as a defect to rebuild. `commit.rs:653-655` and `commit.rs:90-91` read an absent record as a terminal delete, and the test doc at `multipart_upload_part_commit.rs:4559` says "never damage". But the terminal delete row at `0016:674` also deletes surviving `slot:` records in the same batch. So an absent `mpu:` with this attempt's slot still held is not a legal terminal delete. Probe: commit held, `mpu:` erased, answer `Ok(Refused(NoSuchUpload))` with `slots=1`. After the record is restored, releasing the held batch answers `Committed` and the part exists. So the client got a 404-class answer and the part became durable. The comment "it can never land" at `multipart_upload_part_commit.rs:4577` holds only because that leg never restores the record. This is the same shape as iteration 5's bug. The decision needed is whether absent-with-slot-held should keep the doubt, like `SessionEpochNotRisen`.

- NEEDS-HUMAN [impl] — **Three paths that keep the doubt have no test that fails if they drop it.** I replaced each with a plain stop, which reports "no batch of this part attempt is in doubt" and clears `may_still_commit`. All 57 tests still passed:
  - `commit.rs:962`: an undecodable slot under a held commit.
  - `commit.rs:1208`: `ObligationKeyTaken` under a held compensation.
  - `commit.rs:1217`: an undecodable slot under a held compensation.

  The code is correct today. My probes got `InDoubt` with `may_still_commit: true` and the expected cause on all three, for example `Act::hold().after(rig.plant(attempt.slot_key().to_vec(), ...))`. This is the same class as the `compensate` store-fault tests the human required in iteration 4. Three legs close it: hold the batch, damage the key, and assert the `InDoubt` batch, flag and cause.

- NEEDS-HUMAN [human] — **The T4 pass in `check-gates.json:104-113` is weak evidence.** The row says three passes over the bundle found 0 blocking and took 17.49 s, on a 313 KB patch. That time looks too short for three real reviews, though I can't tell why from the log. `gate-logs/T4-batch-review.log` has one summary line, and `review-batch.md` is outside my inputs. The iteration 4 sign-off already recorded that default-effort T4 passes missed known bugs 3 of 3. The sign-off should not count this row as the "deep, multi-pass review" the rubric asks for without opening that file and checking that the passes really ran at high effort on this patch.

- Doc nit, no rebuild needed on its own. The `Refusal::OperationAborted` doc (`crates/core/src/multipart.rs`, the list just above `:4726`) names two causes: a same-number rival, or a Complete rollback. `commit.rs:1039-1043` also answers it when a repair rewrote the prior part, as ratified in iteration 4. Worth adding if the file is touched again.

## What I attacked and could not break

- **The red→green proof.** `C4-verify` is UNVERIFIABLE (`gate-logs/C4-verify.log`): the test can't compile on the reverted base. The brief declared this in advance, so it is not a refutation, but it also proves nothing. I replaced it with 26 deliberate edits to `commit.rs`. One did not compile (it left a method unused), so I reworded it, and the reworded edit is one of the 26. Of the 25 that compiled, 21 made tests fail. The 4 that passed are the three in the third finding above, plus one harmless case below.
- **This rebuild's fix.** Reverting the epoch check for non-`Open` records (`commit.rs:660`) fails 4 tests: `b_a_damaged_session_under_a_held_commit_keeps_the_doubt`, `b_a_session_damaged_and_repaired_mid_round_settles_nothing`, `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt` and `g_a_session_whose_epoch_has_not_risen_is_damage_not_a_rollback_or_a_fence`. The tests go through production `commit_part` over a real redb store, not a copy of the logic.
- **The fix's main assumption.** "Every fence raises the epoch" holds in the code (`multipart.rs:2615`, `:2639`) and in `0016:704-708`, and I found no legal writer that changes a session's bytes without raising it.
- **False `Committed`, including read order and the epoch chase.** I broke the compensation's read order, the settle step, the read-only lap and the `doubt.required` check in turn, and each edit failed at least one test. Removing the commit's first `session_seen` (`commit.rs:955`) changes no answer, because the extra lap settles it anyway. That read only saves a lap, so it is harmless.
- **Stale chunk references at commit.** The commit does not require the `sidx:` bytes, so a repair of an in-flight chunk could in principle leave it stale. It can't here: re-place (`0016:671`) only rewrites `part:` records.
- **CI completeness.** `gate-logs/C4-ci.log` shows `custodian_gc`, machete, deny, conformance and docs all ran, ending in `xtask ci: all checks passed`. The iteration 2 concern about a truncated run does not apply to this round.
- **C5's limit.** "83 mutants, 0 missed" (`check-gates.json:66-75`) is accurate, but cargo-mutants did not catch the three gaps in the third finding, so 0 missed does not mean every branch is protected.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:590`: The slot-ownership guard takes the attempt ID from the batch's own `expected` bytes and uses that ID to select `last_slot`. If attempts A and B have both used the same index through the wrapper, an erroneous batch from A requiring B's latest bytes passes this assertion; the delete check only requires a precondition to exist. Thus the guard does not establish the advertised invariant that a batch requires its caller's own slot bytes. Supply the expected attempt identity independently of the batch, compare it before the lookup, and check that the guard rejects a batch carrying another recorded attempt's bytes. This is a test-validation gap, not a demonstrated production defect.

No production correctness defects or actionable reuse, simplification, or efficiency findings identified in this diff. Validation evidence was reviewed from the frozen gate logs; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared net-new verification exception — 57 tests independently pass after restoration, but the pre-fix discriminator cannot compile, so a behavioral base-to-patch red→green is not established (brief.md:328; reviewer-red-green.log:32; reviewer-red-green.log:115; gate-logs/C4-verify.log:43).
- [ ] T5 Judgment — Decide whether a missing mpu record with the attempt’s slot still held is terminal deletion or damage — the exercised branch settles doubt and returns NoSuchUpload, although terminal deletion removes surviving slots atomically; the brief expressly leaves this policy undecided (brief.md:712; target/crates/core/src/multipart/commit.rs:653; target/crates/core/tests/multipart_upload_part_commit.rs:4577; target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:582).
- [ ] Validation — fitness-to-purpose — Accept this core-only slice as fit for its intended handoff — tests exercise real redb plus fault-injecting doubles, while production UploadPart wiring and end-to-end completion/reclamation remain the expressly scoped follow-ups (brief.md:389; target/crates/core/src/multipart/commit.rs:172; target/docs/design/architecture/05-building-block-view.md:248).
- [ ] **The quiesce renewal (the attempt's last lease renewal before the commit) does not apply this rebuild's epoch rule, so the module doc's claim is wrong.** `crates/core/src/multipart/commit.rs:88-89` says each session read "is judged the same way (`judge_session`)". The renewal uses a different function with the same name, `crates/core/src/multipart/stage.rs:1103-1112`, reached through `stage.rs:1797-1800`. It has no epoch check, and `commit.rs:806-815` trusts its answer. Three probes, each planting the record after staging and before `commit_part`:
- [ ] **The undecided missing-`mpu:` case: new evidence that it can only be damage.** Iteration 5 left this open and I did not treat it as a defect to rebuild. `commit.rs:653-655` and `commit.rs:90-91` read an absent record as a terminal delete, and the test doc at `multipart_upload_part_commit.rs:4559` says "never damage". But the terminal delete row at `0016:674` also deletes surviving `slot:` records in the same batch. So an absent `mpu:` with this attempt's slot still held is not a legal terminal delete. Probe: commit held, `mpu:` erased, answer `Ok(Refused(NoSuchUpload))` with `slots=1`. After the record is restored, releasing the held batch answers `Committed` and the part exists. So the client got a 404-class answer and the part became durable. The comment "it can never land" at `multipart_upload_part_commit.rs:4577` holds only because that leg never restores the record. This is the same shape as iteration 5's bug. The decision needed is whether absent-with-slot-held should keep the doubt, like `SessionEpochNotRisen`.
- [ ] **Three paths that keep the doubt have no test that fails if they drop it.** I replaced each with a plain stop, which reports "no batch of this part attempt is in doubt" and clears `may_still_commit`. All 57 tests still passed:
- [ ] **The T4 pass in `check-gates.json:104-113` is weak evidence.** The row says three passes over the bundle found 0 blocking and took 17.49 s, on a 313 KB patch. That time looks too short for three real reviews, though I can't tell why from the log. `gate-logs/T4-batch-review.log` has one summary line, and `review-batch.md` is outside my inputs. The iteration 4 sign-off already recorded that default-effort T4 passes missed known bugs 3 of 3. The sign-off should not count this row as the "deep, multi-pass review" the rubric asks for without opening that file and checking that the passes really ran at high effort on this patch.
- [ ] `crates/core/tests/multipart_upload_part_commit.rs:590`: The slot-ownership guard takes the attempt ID from the batch's own `expected` bytes and uses that ID to select `last_slot`. If attempts A and B have both used the same index through the wrapper, an erroneous batch from A requiring B's latest bytes passes this assertion; the delete check only requires a precondition to exist. Thus the guard does not establish the advertised invariant that a batch requires its caller's own slot bytes. Supply the expected attempt identity independently of the batch, compare it before the lookup, and check that the guard rejects a batch carrying another recorded attempt's bytes. This is a test-validation gap, not a demonstrated production defect.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The lease fix assumes a deadline the metadata seam does not guarantee.** `brief.md:174-188` calls `E_TX_MILLIS` the commit's transaction deadline and tests only the lease remaining when a batch is sent; `brief.md:403-412` uses that to promise coverage throughout the commit. On the target, `crates/core/src/multipart.rs:5113-5118` defines a five-second FDB envelope, but `crates/traits/src/lib.rs:1332-1345` guarantees termination, not completion within five seconds, and explicitly permits application after a timeout. FDB's five seconds start after obtaining a read version; its configured default timeout is ten seconds (`crates/metadata-fdb/src/lib.rs:369-388`). TiKV's default is fifty seconds (`crates/metadata-tikv/src/lib.rs:171-181`, applied at `:1359-1365`). A batch sent with exactly five seconds of lease left can therefore pass every proposed dispatch assertion and still apply after expiry. Revise the brief to identify the actual enforceable timing contract, and require a delayed-after-dispatch witness. If this slice guarantees only sufficient lease at dispatch, narrow the promised invariant explicitly; a caller timeout alone cannot guarantee when an outstanding batch applies.
- [ ] **The definition of “settled not landed” lacks the read ordering needed to make it true.** `brief.md:458-466` accepts an irreversibly changed session/part precondition “with the own slot still at its last-written bytes”; `brief.md:494-495` permits reading the session after the slot and part. Counterexample: an unknown commit is held; the slot read sees this attempt still holding it; the commit then lands and a same-number rival replaces its part; the part read sees the rival and the session read still sees `Open@E`. Those observations satisfy the stated settlement conditions even though this attempt committed. C3 (`brief.md:117-126`) puts the rival before the re-read, so it does not force this gap. H's rollback history (`:208-210`) exposes the analogous session race but conflicts with the Design's unqualified settlement rule. The target staging code explicitly guards this class by reading the slot again after observing a session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise the proof to require an own-slot observation **after** the irreversible precondition change, or retain doubt, and add the held-commit → rival-between-reads history with an assertion that it cannot be classified as a compensated loser. Otherwise Do can follow the Design and compensate a commit that landed.
- [ ] size backstop — this slice is behaving oversized: patch is 306 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design and start from this attempt's patch. Why rejected: no legal history breaks it, but two damaged-record histories still give a wrong answer, and three doubt-keeping paths have no test that fails if they drop the doubt. Fixes: 1. Bug, now decided: a missing `mpu:` record with this attempt's slot still held is damage, not a terminal delete. A real terminal delete removes surviving `slot:` records in the same batch (`0016:674`). `commit.rs:653-655` and `commit.rs:90-91` read the absence as a terminal delete: with a commit held and `mpu:` erased, the answer is `Refused(NoSuchUpload)` with `slots=1`, and after the record is restored the held batch lands and the part is durable. Keep the doubt and `may_still_commit`, as `SessionEpochNotRisen` does. Correct the test doc at `multipart_upload_part_commit.rs:4559` ("never damage") and the comment at `:4577` ("it can never land"), update the legs that expect the old answer, and add a leg that restores the record and lands the held batch. Cover a held commit and a held compensation. These tests must fail if the fix is reverted. 2. Quiesce renewal: `stage.rs:1103-1112`, reached through `stage.rs:1797-1800`, has no epoch check, and `commit.rs:806-815` trusts its answer. Do not change `stage.rs` behaviour; the slice's one doc-fix limit on that file stands. Correct the claim at `commit.rs:88-89` that every session read is judged by `judge_session`. In `commit.rs`, compare the epoch before and after the renewal, so an `Open` record at a lower epoch, or under other bytes at the same epoch, is not adopted. Add a test that fails without this check. The non-`Open` refusal case stays a known gap: record it in `build-notes.md` and the doc. 3. Add tests, each failing under the named edit: `commit.rs:962` (undecodable slot under a held commit), `:1208` (`ObligationKeyTaken` under a held compensation), `:1217` (undecodable slot under a held compensation). For each, replace the doubt-keeping return with a plain stop. Hold the batch, damage the key, and assert the `InDoubt` batch, flag and cause. 4. Test guard: `multipart_upload_part_commit.rs:590` takes the attempt id from the batch's own `expected` bytes. Supply the expected attempt identity independently of the batch, compare it before the lookup, and show the guard rejects a batch carrying another recorded attempt's bytes. 5. Doc: add a repair rewriting the prior part as a cause in the `Refusal::OperationAborted` doc (`crates/core/src/multipart.rs`, the list just above `:4726`; `commit.rs:1039-1043` already answers it). Decided at sign-off: - The size backstop stays overridden: do not split. Do not trim tests to reduce size. - Keep the earlier decisions: the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, slots left held after a rollback, and a plain-repair changed part compensating with nothing in doubt. Review caveat: the T4 row on this attempt does not count as a deep review. Its three passes took 10-16 s each, ran 0 commands and used 217-311 reasoning tokens, against 120-320 s, 9-15 commands and 2,738-7,863 tokens for the previous patch at high effort. `scripts/review-branch` sets no effort level, and `review-passes.json` still carries the previous patch's hash. The next sign-off needs a confirmed high-effort review of the final patch.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
