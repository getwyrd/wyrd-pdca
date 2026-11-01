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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.2% — 659 of 664 instrumentable changed lines executed (floor 80%); 664 of 1538 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 85 mutants tested in 6m: 68 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.76s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #927’s core `UploadPart` commit, classification, compensation, re-upload and ceilings: one implementation defect is reproduced; the supplied 57-test suite passes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance histories define ownership, uncertainty, lease and cleanup obligations; the later sign-off explicitly corrects the prior-record recurrence assumption (`brief.md:269`, `brief.md:683`). |
| C2 Reproduction (red pre-fix) | N/A | This is declared new functionality: the base has no commit API, and the retained test fails compilation before executing any assertion (`brief.md:321`, `reviewer-red-green.log:175`). |
| C3 Change | PASS | The five affected files stay within the authorized core slice and update its living architecture description; production wiring remains the separately assigned milestone (`brief.md:300`, `docs/design/architecture/05-building-block-view.md:248`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared absence of an executable pre-fix red — stash/pop independently reproduces compilation failure on the base and 57 passing tests with the patch; this is not a behavioral red→green (`brief.md:328`, `reviewer-red-green.log:208`, `reviewer-red-green.log:288`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve uncertainty when a non-Open session has an equal or lower epoch — four probes return definite `NoSuchUpload` although repairing the session lets the held commit or compensation land (`crates/core/src/multipart/commit.rs:666`, `reviewer-probe.log:3`). |
| T1 Structure | PASS | Core retains the existing metadata, placement and clock seams without importing concrete backends or introducing shared global state (`crates/core/src/multipart/commit.rs:182`, `crates/core/src/multipart/commit.rs:1287`). |
| T2 Shape | PASS | Persisted shapes remain compatible with existing decoders and CAS byte identity; the executed wrapper checks part and summary round trips and retirement payload validity (`crates/core/tests/multipart_upload_part_commit.rs:482`, `crates/core/src/multipart.rs:4117`). |
| T3 Runtime | PASS | Dispatch uses the staging clock, retries have explicit finite budgets, and external awaits inherit the store’s termination contract (`crates/core/src/multipart/commit.rs:211`, `crates/core/src/multipart/commit.rs:885`, `crates/traits/src/lib.rs:1333`). |
| T4 Contribution | FAIL | The required review gate still has one unresolved finding, independently reproduced here; publish-time contribution artifacts are separately N/A because drafting is intentionally deferred (`gate-logs/T4-batch-review.log:13`, `gate-logs/T4-contribution.log:10`, `AGENTS.md:221`). |
| T5 Judgment | PASS | The ratified size and dispatch decisions and tracked DST deferral remain settled; affected-path history and all 19 closed/unmerged PR file lists were checked, with no overlapping rejected PR (`brief.md:693`, `crates/core/src/multipart/commit.rs:175`, `reviewer-prior-art.log:53`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this core-only increment is fit for the milestone after resolving C5 — the tests exercise real redb storage with injected histories, while end-to-end S3 UploadPart remains assigned to #669 (`brief.md:389`, `docs/design/architecture/05-building-block-view.md:248`). |

**The reproduced defect loses uncertainty about a batch that can still commit.** In `judge_session`, only the `Open` branch checks that the observed epoch exceeds the required epoch; the non-`Open` branch immediately returns `Left` (`crates/core/src/multipart/commit.rs:652`, `:666`). `Doubt::session_seen` then marks the batch unable to land, and a subsequent held-slot read discards that doubt (`crates/core/src/multipart/commit.rs:516`, `:536`). A real fence must advance the epoch (`crates/core/src/multipart.rs:2600`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:538`). An `Aborting` record at the same or an earlier epoch is therefore damage, not proof of an irreversible transition.

I compiled a separate probe against the patched core library, reusing the supplied real-store harness without editing target source. Each case stages a part, holds its commit or its loser's compensation, and substitutes a canonical, decodable `Aborting` session record with an epoch that has not increased. After recording the function's answer, it restores the exact required session bytes and releases the held batch. The observed results were:

| Held batch | Required epoch | Observed Aborting epoch | Returned answer | Held batch after repair |
|---|---|---|---|---|
| Commit | 0 | 0 | `Refused(NoSuchUpload)` | `Committed` |
| Commit | 2 | 0 | `Refused(NoSuchUpload)` | `Committed` |
| Compensation | 0 | 0 | `Refused(NoSuchUpload)` | `Committed` |
| Compensation | 2 | 0 | `Refused(NoSuchUpload)` | `Committed` |

All four cases released the slot exactly once after repair. The commit cases also verified that this attempt's own part became durable. Thus the problem is the definite answer and discarded uncertainty, not a speculative inability to apply the patch. The probe fails its requirement for `InDoubt` with `may_still_commit: true` and a record-error cause (`reviewer-probe.log:3`, `reviewer-probe.log:18`). Run it in this review directory with `./pdca-reviewer-927-probe reviewer_non_open_nonincreasing_epoch_preserves_doubt --exact --nocapture`; its source is `pdca-reviewer-927-probe.rs:4969`, and compilation is recorded in `reviewer-probe-build.log:1`.

The builder should validate epoch progression for every changed, present session before using its state to settle doubt, and extend the damage tests to non-`Open` records at equal/lower epochs for both phases, including damage repaired between reads. The existing fixture tests only undecodable bytes and changed `Open` bytes (`crates/core/tests/multipart_upload_part_commit.rs:4006`). This is the same concern identified by the frozen review gate, independently reproduced rather than counted as a second defect. The capability-probe/load-time symptom-guard smell-test does not apply: no such probe was added.

**Verification distinguishes observed results from frozen evidence.** The disposable target's synthetic base names `376b106248d2ac2e816c5d258609e04de09a425f`, matching the brief. Reverse-application checking succeeds. Stashing the tracked production changes leaves the new test against the old public API: exit 101, unresolved `multipart::commit` and four missing-variant errors, with no test run. Popping the stash produces 57/57 passing tests. All five affected files were checked byte-for-byte after restoration. Source citations above are relative to `$PDCA_TARGET`; evidence citations are relative to this review directory.

| Gate | Verdict | Evidence and limits |
|------|---------|---------------------|
| C4-ci | PASS | The frozen run completed CI (`gate-logs/C4-ci.log:4552`). The independent full rerun exits 1 solely on two checks affected by the copy’s missing advisory-db gitlink (`reviewer-ci.log:4013`); it is not a fully green independent run. |
| C4-verify | NEEDS-HUMAN | Both the frozen verifier and independent stash/pop run show green tests but no executable pre-fix red (`gate-logs/C4-verify.log:49`, `reviewer-red-green.log:208`). This is the C4 decision above. |
| C4-diff-cov | PASS | Frozen output reports 659/664 instrumentable changed lines executed, 99.2%; 874 unscored lines are not claimed covered (`gate-logs/C4-diff-cov.log:83`, `:101`). |
| C5-mutants | PASS | Frozen output reports 85 mutants: 68 caught, 17 unviable, no survivors; that set does not cover the independently reproduced epoch case (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | FAIL | One unresolved finding at `commit.rs:666`; the independent four-case probe confirms its impact (`gate-logs/T4-batch-review.log:13`, `reviewer-probe.log:3`). |
| T4-contribution | N/A | Artifacts are absent by design; the substantive audit must run at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Frozen log shows both requested feature-enabled clippy builds completed; it does not claim a live TiKV service test (`gate-logs/host-tikv.log:2`, `:211`). |

**The CI rerun's advisory-db failure belongs to the harness copy.** Its index has no `.github/advisory-db` gitlink, although `.gitmodules` remains. The existing index test fails at `xtask/tests/advisory_db.rs:66`, and CI cannot pin the database for its dependency audit (`reviewer-ci.log:3367`, `reviewer-ci.log:3386`). The frozen run contains the successful index test, actual pinned dependency-audit invocations and final CI success (`gate-logs/C4-ci.log:3468`, `:3905`, `:4552`). The rerun completed with exactly those two failed checks; formatting, clippy, build, dependency-usage scanning, format conformance, statics/dependency-direction guards and DST completed, including the long 33-test custodian suite (`reviewer-ci.log:3933`, `reviewer-ci.log:4013`). This is an environment caveat, not a C4 patch defect. The brief's external tools were exercised: the independent run invokes `typos` and successfully lints, renders 101 documentation pages and audits links (`reviewer-ci.log:2`, `reviewer-ci.log:4`, `reviewer-ci.log:7`). Coverage, mutation and TiKV results above are adjudicated from frozen logs, not presented as independent reruns.

**Prior art was checked by file path.** GitHub history at the declared base was queried separately for all five affected files. It shows the merged staging/renewal, Create, Abort and record work; the new commit module and test have no prior history there. Every changed-file list for the 19 closed/unmerged PRs was inspected, and none intersects the affected paths (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:53`). Rejected local attempts and ratified corrections are recorded in `brief.md:640` onward. The #660 seeded-DST deferral is explicit and remains settled under `AGENTS.md:215`; it is not raised again. No project `INTEGRATION.md` was present in the supplied inputs or target, so no additional §4 items can be inferred.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #927, part commit (iteration 5)

**Result: refuted on one point.** The blocking T4 finding is real, and I reproduced it by running it. It needs a damaged session record, so no legal store history reaches it. The rest of the classification held up under attack. The probes and trial edits ran in a scratch copy of `$PDCA_TARGET`, and none of them are in the patch.

## Refutations

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:666`: `judge_session` treats any non-`Open` record that decodes as a real fence, whatever its epoch. Every legal fence raises the epoch (`Open@E → Aborting@E+1`, `crates/core/src/multipart.rs:2612-2616`; `Completing@E+1`, `0016:705`), so `Aborting@E` is damage, like the `Open` case the code already rejects at `:660`. `Doubt::session_seen` (`commit.rs:516-525`) then clears the doubt, which breaks the rule that damage settles nothing. I planted `{"kind":"Aborting"}` at the same epoch and got these results:
  - **Held commit, damage stays:** the answer is `Refused(NoSuchUpload)`. After the session is repaired, the held batch answers `Committed` and the part is durable.
  - **Held commit over an empty prior with a rival, damage after read 1 and repaired after read 2:** a compensation is sent while the commit can still land, and the answer is `Refused(OperationAborted)`.
  - **Held first upload, same damage and repair, with the compensation answered unknown as the held commit lands:** the answer is `Stopped { CommitRoundsSpent, slot_released: true }`. The attempt's own part is durable and no obligation exists, so the error's "no batch is in doubt" and "chunks are retired" are both false.
  - **Held compensation:** the answer is `Refused(NoSuchUpload)`, and the batch lands after the repair (the `Left` path at `commit.rs:1265-1274`).

  **Trial fix:** in the `Refused` arm, return `Left` only when `record.epoch() > required.epoch`, and otherwise return `SessionEpochNotRisen`. With that one change, all 57 existing tests and the three probes above that assert pass (60 of 60). The fourth case above only printed its result, so I did not re-check it under the fix. The builder should also widen the doc at `crates/core/src/multipart.rs:386-394` and `commit.rs:88-92`, which say only `Open` records can be this damage.

  **Sign-off wording:** the iteration 3 carry-forward says a record that "decodes and is not `Open`" counts as moved, which is what the builder wrote. This fix narrows that rule, but it changes no legal history.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:4008-4021`: this is why the suite stayed green. `session_damage` plants only torn bytes and an `Open` record at the same epoch, and the repaired-mid-round leg plants only `TORN` (`:4117`, `:4154`). Add a non-`Open` record at the same epoch and at a lower epoch to `session_damage`, and use one in the repaired-mid-round leg. Without that, a fix to `:666` has no test that fails if it is reverted.

- NEEDS-HUMAN [human] — `crates/core/src/multipart/commit.rs:645-647`: a missing `mpu:` record has the same flaw, but a test pins the current answer, so this is a decision. The module doc calls it a terminal delete (`commit.rs:90-91`), yet that delete removes surviving slots in the same batch (`0016:350`), so "session absent, own slot still held" is not a legal history either. I erased the key under a held commit: the answer was `Refused(NoSuchUpload)` with the slot held, and after restoring the key the held batch answered `Committed` with the part durable. A prior part record that goes missing is already treated as damage (`commit.rs:1014-1019`). Iteration 3 asked for a test of this "session gone" row, and `multipart_upload_part_commit.rs:4511-4521` expects `NoSuchUpload`. Should a held slot with no session become a record error, or does the current answer stand?

## The gate verdicts

- `check-gates.json` row `T4-batch-review`: the `fail` is correct, and the finding is not noise. Passes 1 and 3 came back clean at high effort and pass 2 caught it (`gate-logs/T4-batch-review.log`), so a clean pass here is weak evidence, even at high effort.
- `C4-verify` is `unverifiable`, as the brief declared in advance, because the new test file cannot compile without the new API. That is not a refutation. I re-ran the green leg and got 57 passed, and the tests call the real `commit_part` over a real redb store.
- `C5-mutants` reports all 68 viable mutants caught, which only covers the edits `cargo mutants` generates. I hand-ran 19 further logic edits, and 18 failed tests. These included:
  - the iteration 4 rule (`commit.rs:938`), which fails four legs
  - the obligation epoch set to 0
  - `COMPENSATION_ROUNDS` lowered by one
  - all four compensation store-fault paths
  - both missing and damaged part checks
  - both read-only laps
  - the obligation byte comparison made always true
  - the slot `Rewritten` case treated as `Gone`
- The one survivor is removing `doubt.session_seen(first)` at `commit.rs:950`, which passes all 57 tests. From reading the code, it looks harmless in legal histories: the sticky mark from the last read plus the read-only lap reach the same answer, one lap later. I did not test that, and I raise no action on it.
- `C4-ci` did run to completion this time: `custodian_gc`, machete, deny and conformance all ran, and the log ends `xtask ci: all checks passed` (`gate-logs/C4-ci.log:4552`). This closes the iteration 2 item.

## Attacked and could not refute

- **False `Committed`:** I traced rival, rollback, teardown and slot-reuse histories through the own-commit proof at `commit.rs:973-986`, and found no legal history that reaches it wrongly.
- **Compensation while a commit can still land:** in legal histories, a doubt leaves only at a slot read that follows the read showing the commit cannot land (`commit.rs:959-963`). The only break I found is the damage case above.
- **The epoch chase:** a round held under `E` and a new round under `E'` both require the same slot bytes, so at most one lands. An empty part whose slot is gone after the session moved answers in doubt rather than guessing (`commit.rs:1229-1237`).
- **Obligation key sharing:** the commit and compensation use the same retire key at one epoch. No collision is reachable, because a compensation is sent only after the commit is settled as not landed.
- **The quiesce renewal:** `stage.rs:1656-1668` returns the in-doubt error for a round still in doubt, so compensating after a settled renewal stop cannot race one of its own renewals.
- **The test wrapper's "every leg" checks:** `check_slot_bytes` and `check_margin` run inside `commit` on the test's own task (`multipart_upload_part_commit.rs:702-708`), so a breach fails the leg. They are not disconnected checks.

I did not re-raise the items ratified at sign-off, or the DST coverage deferred to #660.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:666`: `judge_session` treats every decodable non-`Open` record as an irreversible fence without checking its epoch. Legal transitions must increase the epoch (`crates/core/src/multipart.rs:2582`). With a commit held at `Open@E`, a damaged `Aborting@E` (or older) makes `Doubt::session_seen` mark it unable to land; the following held-slot read clears the doubt and the call returns `NoSuchUpload`. Restoring the required `Open@E` bytes then lets that held commit land. The same helper affects compensation. Check epoch monotonicity for every changed session state, preserve doubt on invalid transitions, and extend the damage regressions to non-`Open` records at equal/lower epochs, including repair between reads. This confirms the finding recorded in the frozen T4 gate log.

No additional correctness findings or actionable reuse, simplification, or efficiency changes found. Reviewed target source and frozen gate evidence; no builds were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared absence of an executable pre-fix red — stash/pop independently reproduces compilation failure on the base and 57 passing tests with the patch; this is not a behavioral red→green (`brief.md:328`, `reviewer-red-green.log:208`, `reviewer-red-green.log:288`).
- [ ] C5 Causal adequacy — Preserve uncertainty when a non-Open session has an equal or lower epoch — four probes return definite `NoSuchUpload` although repairing the session lets the held commit or compensation land (`crates/core/src/multipart/commit.rs:666`, `reviewer-probe.log:3`).
- [ ] Validation — fitness-to-purpose — Decide whether this core-only increment is fit for the milestone after resolving C5 — the tests exercise real redb storage with injected histories, while end-to-end S3 UploadPart remains assigned to #669 (`brief.md:389`, `docs/design/architecture/05-building-block-view.md:248`).
- [ ] C4-verify — Both the frozen verifier and independent stash/pop run show green tests but no executable pre-fix red (`gate-logs/C4-verify.log:49`, `reviewer-red-green.log:208`). This is the C4 decision above.
- [ ] `crates/core/src/multipart/commit.rs:666`: `judge_session` treats any non-`Open` record that decodes as a real fence, whatever its epoch. Every legal fence raises the epoch (`Open@E → Aborting@E+1`, `crates/core/src/multipart.rs:2612-2616`; `Completing@E+1`, `0016:705`), so `Aborting@E` is damage, like the `Open` case the code already rejects at `:660`. `Doubt::session_seen` (`commit.rs:516-525`) then clears the doubt, which breaks the rule that damage settles nothing. I planted `{"kind":"Aborting"}` at the same epoch and got these results:
- [ ] `crates/core/tests/multipart_upload_part_commit.rs:4008-4021`: this is why the suite stayed green. `session_damage` plants only torn bytes and an `Open` record at the same epoch, and the repaired-mid-round leg plants only `TORN` (`:4117`, `:4154`). Add a non-`Open` record at the same epoch and at a lower epoch to `session_damage`, and use one in the repaired-mid-round leg. Without that, a fix to `:666` has no test that fails if it is reverted.
- [ ] `crates/core/src/multipart/commit.rs:645-647`: a missing `mpu:` record has the same flaw, but a test pins the current answer, so this is a decision. The module doc calls it a terminal delete (`commit.rs:90-91`), yet that delete removes surviving slots in the same batch (`0016:350`), so "session absent, own slot still held" is not a legal history either. I erased the key under a held commit: the answer was `Refused(NoSuchUpload)` with the slot held, and after restoring the key the held batch answered `Committed` with the part durable. A prior part record that goes missing is already treated as damage (`commit.rs:1014-1019`). Iteration 3 asked for a test of this "session gone" row, and `multipart_upload_part_commit.rs:4511-4521` expects `NoSuchUpload`. Should a held slot with no session become a record error, or does the current answer stand?
- [ ] `crates/core/src/multipart/commit.rs:666`: `judge_session` treats every decodable non-`Open` record as an irreversible fence without checking its epoch. Legal transitions must increase the epoch (`crates/core/src/multipart.rs:2582`). With a commit held at `Open@E`, a damaged `Aborting@E` (or older) makes `Doubt::session_seen` mark it unable to land; the following held-slot read clears the doubt and the call returns `NoSuchUpload`. Restoring the required `Open@E` bytes then lets that held commit land. The same helper affects compensation. Check epoch monotonicity for every changed session state, preserve doubt on invalid transitions, and extend the damage regressions to non-`Open` records at equal/lower epochs, including repair between reads. This confirms the finding recorded in the frozen T4 gate log.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- [ ] **The lease fix assumes a deadline the metadata seam does not guarantee.** `brief.md:174-188` calls `E_TX_MILLIS` the commit's transaction deadline and tests only the lease remaining when a batch is sent; `brief.md:403-412` uses that to promise coverage throughout the commit. On the target, `crates/core/src/multipart.rs:5113-5118` defines a five-second FDB envelope, but `crates/traits/src/lib.rs:1332-1345` guarantees termination, not completion within five seconds, and explicitly permits application after a timeout. FDB's five seconds start after obtaining a read version; its configured default timeout is ten seconds (`crates/metadata-fdb/src/lib.rs:369-388`). TiKV's default is fifty seconds (`crates/metadata-tikv/src/lib.rs:171-181`, applied at `:1359-1365`). A batch sent with exactly five seconds of lease left can therefore pass every proposed dispatch assertion and still apply after expiry. Revise the brief to identify the actual enforceable timing contract, and require a delayed-after-dispatch witness. If this slice guarantees only sufficient lease at dispatch, narrow the promised invariant explicitly; a caller timeout alone cannot guarantee when an outstanding batch applies.
- [ ] **The definition of “settled not landed” lacks the read ordering needed to make it true.** `brief.md:458-466` accepts an irreversibly changed session/part precondition “with the own slot still at its last-written bytes”; `brief.md:494-495` permits reading the session after the slot and part. Counterexample: an unknown commit is held; the slot read sees this attempt still holding it; the commit then lands and a same-number rival replaces its part; the part read sees the rival and the session read still sees `Open@E`. Those observations satisfy the stated settlement conditions even though this attempt committed. C3 (`brief.md:117-126`) puts the rival before the re-read, so it does not force this gap. H's rollback history (`:208-210`) exposes the analogous session race but conflicts with the Design's unqualified settlement rule. The target staging code explicitly guards this class by reading the slot again after observing a session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise the proof to require an own-slot observation **after** the irreversible precondition change, or retain doubt, and add the held-commit → rival-between-reads history with an assertion that it cannot be classified as a compensated loser. Otherwise Do can follow the Design and compensate a commit that landed.
- [ ] size backstop — this slice is behaving oversized: patch is 301 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design and start from this attempt's patch. Why rejected: the T4 gate fails on one finding, confirmed by running it. Fixes: 1. Bug: `crates/core/src/multipart/commit.rs:666`, `judge_session` treats any decodable non-`Open` record as a real fence, whatever its epoch. Every legal fence raises the epoch, so a non-`Open` record at the same or a lower epoch is damage and settles nothing. Return `Left` only when `record.epoch() > required.epoch`; otherwise return `SessionEpochNotRisen`, keeping the doubt and `may_still_commit`. The adversary's trial of this change passed the 57 existing tests and its three asserting probes. This narrows the iteration 3 wording that a record that "decodes and is not `Open`" counts as moved; it changes no legal history. 2. Tests: add a non-`Open` record at the same epoch and at a lower epoch to `session_damage` (`crates/core/tests/multipart_upload_part_commit.rs:4008-4021`), and use one in the repaired-mid-round leg (`:4117`, `:4154`). Cover both a held commit and a held compensation. These tests must fail if the fix is reverted. 3. Docs: widen `crates/core/src/multipart.rs:386-394` and `commit.rs:88-92`, which say only `Open` records can be this damage. Not decided at this sign-off: the missing `mpu:` record with the attempt's slot still held (`commit.rs:645-647`, pinned at `multipart_upload_part_commit.rs:4511-4521`). Do not change this behaviour in this rebuild. Keep the earlier decisions: the size backstop stays overridden, do not trim tests to reduce size, the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, and slots left held after a rollback.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
