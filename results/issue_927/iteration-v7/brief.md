# core: UploadPart commit — part record, outcome classification, compensation, re-upload, ceilings (657.2)

> Second of the two slices #657 was split into on 2026-10-20. Staging (#932, #933) is merged, and
> hands this slice a staged attempt: a held slot and one owned `sidx:` entry per chunk. Do reads
> this file, and the one previous attempt it names under **Citations expected**. The design is
> normative in proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read
> these before writing code: the `part:` and `psum:` rows of the record table (`0016:351-352`); the
> **Part commit** and **Live-session compensation** rows of the batch inventory (`0016:660`, `:673`);
> decision 5's commit classification (`0016:1584-1653`) and quiesce rule (`0016:1720-1733`); and the
> post-staging refusal rule (`0016:1420-1431`). Citations were verified against `getwyrd/wyrd`
> `origin/main` @ `376b106` on 2026-10-30.
>
> **Re-plan of attempt 1.** Attempt 1 was rejected at sign-off on 2026-10-30. Its protocol work mostly
> held under attack, but it let a commit run past its lease, left leftovers under a live session on a
> settled stop, and had five test gaps. Legs (F), (G) and (H) below are what changed; the rest of the
> criterion is unchanged in meaning. The plan review of this re-plan added (F)'s delayed-after-dispatch
> leg, (B)'s read-gap leg, and the read-order rule in Design § *Settle before compensating*.

- **Slug:** multipart-upload-part-commit
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** Staging (#932, #933) can stage a part's chunks, but nothing turns them into a part. This
  slice adds the second half of `UploadPart` as `wyrd-core` functions. It (1) commits the
  `part:` + `psum:` pair, the slot release and the `sidx:` deletes in one batch, after a last
  renewal, sending each commit round only while the lease has the dispatch margin left (F). It (2) classifies a `Conflict` or a `CommitUnknownResult` from a re-read, never
  blind. It (3) runs the live-session compensation, including against a session rolled back to
  a newer `Open@E'`, and on any settled stop with nothing in doubt. It (4) handles a re-upload of a committed part number, retiring the prior
  part's chunks in the same batch. It (5) refuses a part past the per-part or cumulative chunk
  ceiling and compensates what it staged. It (6) composes the whole `UploadPart` (reserve →
  stage → commit), answering in the landed vocabulary (`UploadPartOutcome`
  `crates/core/src/multipart.rs:4669`, `Refusal` `:4550`).
- **Success criterion:** A NEW test file `crates/core/tests/multipart_upload_part_commit.rs`,
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
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and the wrapper forces every red leg rather than leaving it to timing. The rollback and the
  Abort are written by the wrapper before the commit; unknown and held outcomes are injected; C1
  pins the clock, so identical bytes are certain; C2's Abort and teardown run at each read gap
  by hook; C3's rival (and C3′'s third reserve) lands by hook between the commit and the re-read; the
  held-commit / unknown-empty-compensation leg lands the held commit by hook the moment a
  compensation is sent; C4's empty parts are planted, so no obligation can exist; the overshoot
  witness holds one commit by hand; (F) moves a `ManualClock` by hook, which attempt 1's suite never
  did, and its delayed leg holds the batch past expiry by hand; (B)'s read-gap leg runs its
  histories at each read gap by hook; (G) and (H) force their stops the same way. (E) is the gating `C4-ci` check. In #657's first attempt, C1, C2 and
  C3 were each reproduced against the patched tree by independent probes, and the rollback path
  and the bounded rounds had no test that could fail. In #927's attempt 1, the lease overrun and each
  (H) gap were reproduced against the patched tree by independent probes (the bundle's `iteration-v1/check-review.md` and
  `check-advisory-adversary.md`).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the commit side. Every staged fragment stays named by a durable
  owned `sidx:` record until a `part:` record or a `retire:` obligation names it, in the same
  batch that removes the `sidx:` record. `Committed` is answered only on evidence that **this
  attempt's own** commit landed, evidence no other writer's record or teardown can produce. A
  slot is released exactly once, and never by an attempt that does not own it. No compensation is
  sent while the attempt's own commit may have landed or may still land. No commit round is sent
  with less than the dispatch margin left on the lease. That rule covers dispatch only, and this
  slice does not promise when a sent batch applies: a batch applying after the lease ran out is made
  safe by its `require`s, not by the clock (`0016:1851-1852`, `:1859-1863`). An `Open` session is never answered `NoSuchUpload`, and an attempt whose
  commit is settled as not landed releases its slot under a live session on every stop the Design's
  table marks *Compensate*, because the reaper does not walk an `Open` session (`0016:1600-1620`). **Sources:** the
  maintainer's rule 2026-07-25 (via principles.md §5); 0016's refutation standard
  (`0016:2809-2813`, "only availability, latency, capacity and operational costs are acceptable
  trade-offs"); the post-staging refusal rule (`0016:1420-1431`); the commit classification (`0016:1584-1653`) and the two failure-mode rows
  on unknown outcomes (`0016:1780`, `:1792`). SELF-TEST: it binds the commit, the classification
  re-read, the compensation and the re-upload batches, so guarding one function cannot satisfy
  it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** No prerequisite is left: #932 and #933 are merged (PRs #942, #965), so `main` is
  the base. Eight briefs name this slice in `Depends on` (#658, #625, #508, #1008, #1015, #668, #669,
  #670); they stay valid because this slice is **not split**. **Size, decided 2026-10-30:** the human
  chose to keep this as one bundle, knowing the patch stays over the 100 KB signal (expected about
  165 KB). Most of that is test harness and test legs, so do not trim tests to get under it. **Intake:**
  this is a repair of an UNPLANNED id, which the cap allows; alpha was 23/6 before this brief and is
  24/6 with it, with no override and no new tracker issue. **No `Conflicts with` #653:** this slice edits neither
  `crates/core/src/lib.rs` nor `docs/design/architecture/08-crosscutting-concepts.md`.
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** the second half of `UploadPart` as store-I/O functions in `wyrd-core`. That means
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
- **Repro instruction:** n/a (new functionality). On `main` no code writes a `part:` or `psum:`
  record; `PartRecord` (`multipart.rs:2993`) and `PartSummary` (`:3092`) have decoders and no
  writer.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator; confirmed with
  `run-verify.sh --classify`).
- **Verification posture:** (a) net-new. The commit functions do not exist on `main`, so the test cannot compile on C4-verify's reverted base, and C4-verify
  reports **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Built and exercised at Check:** the commit, classification, compensation, re-upload and
  ceiling paths and the composed `UploadPart`, driven by the new test over a real store and a
  chunk-store double.
  **Demonstrated reds (required, recorded in `build-notes.md` with the exact edit and the name
  of the test that failed).** Do makes each temporary edit, shows the named leg fail, then
  restores the file. Each red must come from the edit its label names:
  - compensating on `CommitUnknownResult` without re-reading → (B)'s landed-unknown leg;
  - after an unknown outcome, "a part is there" taken as `Committed` without checking it is the
    attempt's own → (B)'s re-upload leg;
  - part bytes alone taken as proof of the attempt's own commit → C1;
  - the session's state taken from a read made before the slot read, never re-checked → C2;
  - the attempt's possibly-landed commit dropped from its doubt before the classification looks
    for it → C3;
  - the rolled-back `Open@E'` sent to the reaper instead of compensated → (B)'s rollback leg;
  - an unknown compensation outcome treated as "did not land" with `may_still_commit: true` →
    (C)'s compensation leg;
  - a loop counter that never reaches its bound → (C)'s bounded-rounds leg;
  - "my slot is gone" read as "the slot key is absent" (ignoring `attempt_id`) → C3′;
  - a compensation sent while the attempt's commit is held and unsettled → (H)'s
    held-commit-and-rollback leg (not (B)'s held-commit / unknown-empty-compensation leg: its
    rival's bytes equal the prior's, so that edit falls through to a benign retry there);
  - the rival-record branch checked before the own-commit proof → C3 (it compensates a landed
    commit);
  - an empty compensation or re-upload that still writes a `{chunks}` obligation → C4 (the
    record is refused);
  - the rollback or same-part loser answering any refusal other than `OperationAborted` → (B);
  - the lease coverage check removed, and separately off by one → (F)'s retry and boundary legs;
  - a commit round allowed while a commit is in doubt and the lease is short, or a compensation
    sent there → (F)'s in-doubt leg;
  - a lease past expiry taken as proof that a held commit did not land → (F)'s delayed leg (a
    compensation appears in the log);
  - a rival or a moved session seen **after** the last slot read counted as settling the commit
    → (B)'s read-gap leg, in its landed-then-rival and landed-then-rollback runs;
  - settling never allowed after a precondition dies, so the attempt only ever returns the in-doubt
    error → (B)'s read-gap leg, in its not-landed runs;
  - a settled stop that returns without compensating → (G), including the cap witness;
  - a compensation after `SlotLost`, or after a renewal `InDoubt` → (G)'s not-compensated leg;
  - the over-limit check in the commit removed → (D)'s per-part leg (no `part:` record may exist);
  - the session check after an unknown compensation made always true → (H)'s teardown leg;
  - the obligation re-read skipped, and its byte comparison made always true → (H)'s re-read leg
    and its `ObligationKeyTaken` run;
  - `COMPENSATION_ROUNDS` lowered by one → (H)'s epoch-chase leg;
  - `UploadPartError::source` returning `None` → (H)'s class leg.
  **Slot bytes, by invariant rather than by red.** "Every later batch requires
  the slot bytes this attempt last wrote" is no longer assigned a red in C3′: a conforming
  attempt recognises its own commit there and returns `Committed` before any later batch is
  built, so an edit confined to the retry builder never runs in that fixture. And no conforming
  path sends a batch after another attempt holds the index, so no fixture can reach it. Instead:
  (1) the test's wrapper checks, on **every** batch in **every** leg, that any batch of an
  attempt naming its `slot:` key requires exactly the bytes that attempt last wrote there (the
  wrapper records them from the attempt's own reserve and renewal batches); a breach fails the
  leg it happens in; (2) `build-notes.md` cites where the batch builders take the slot bytes from
  the attempt's own state, and lists the re-read-bytes mutant among the survivors below as
  unreachable under the classification, with this reason.
  **C5 (advisory) survivors:** `build-notes.md` lists every mutant `cargo mutants --in-diff`
  leaves alive in this slice's code, each with the test that now kills it or why it is
  equivalent. Attempt 1 left nine; the six that change an answer must now die. Its two
  `lossy` and one `Display` survivors change only error text: kill them with an assertion on the
  message or justify them.
- **Production reach:**
  - (a) The test exercises the commit for real against a real store. Production stages and
    commits no part in this slice.
  - (b) #658's Complete consumes the `part:` records; the S3 verb lands with #669 under #508,
    after #658/#659 and the reaper (#625), per 0016's same-release rule (`0016:236-268`).
  - (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the classification:** #895's Abort, `crates/core/src/multipart/abort.rs:282`. It
    re-reads after `Conflict`, never reads `CommitUnknownResult` as `Conflict`, keeps a
    `may_still_commit: true` doubt, and returns `InDoubt` when its rounds run out (`:264-274`).
    Its test's held-batch pattern is `crates/core/tests/multipart_abort.rs:961-980`.
  - **Records and keys:** `part_key` (`multipart.rs:1321`), `psum_key` (`:1336`), `PartRecord`
    (`:2993`), `PartSummary` (`:3092`), `retire_key` (`:1517`), `decode_retire_obligation`
    (`:3954`), and `RetireObligation` (`:3976`), the one writer-side route for an obligation: add
    the per-part `{chunks}` row there, as the doc at `:3679-3681` asks. Also `UploadPartOutcome`
    (`:4669`), `MAX_PART_CHUNKS` (`:5037`), `MAX_INFLIGHT_PARTS` (`:5097`), `MAX_STAGED_CHUNKS`
    (`:5108`), `E_TX_MILLIS` (`:5118`) and `MAX_COMPLETE_ATTEMPTS` (`:5279`). `SessionRecord::open`
    is the model for a crate-private writer-side constructor (its doc, `:2350-2353`).
  - **Staging's hand-off:** `StageOutcome` (`crates/core/src/multipart/stage.rs:362`),
    `RenewOutcome::Covered` (`:375-384`, "the commit checks both against its own deadline"),
    `StagedAttempt::renew` (`:563`), `StageError` (`:610`) and `Stop` (`:644`). `stage.rs:1713-1732`
    is the peer for the read-order rule: it reads the slot again after seeing a session move,
    before letting a doubt go.
  - **The store's timing contract:** `crates/traits/src/lib.rs:1332-1345` promises termination, and
    that a commit abandoned at a deadline is undetermined. The drivers' deadlines are
    `crates/metadata-fdb/src/lib.rs:388` and `crates/metadata-tikv/src/lib.rs:181` (applied at
    `:1363`).
  - **Start from attempt 1 (decided 2026-10-30).** Do MAY open, as a narrow exception to reading
    this file only, the bundle's `iteration-v1/patch.diff`. It applies cleanly to `376b106`, and an
    independent attack found no fault in its classification apart from what this brief changes.
    Its round already reads the part, the session, the slot, then the session again, and lets a
    doubt go only at a slot read (`Doubt::slot_held`), which meets the read-order rule; no test
    of it holds that order, so (B)'s read-gap leg is new. Start from it and correct it. Do not resubmit it unchanged, and take nothing else from
    `iteration-v1/`. Line numbers it is cited by below are of its patched tree.
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/tests/multipart*`,
  `docs/design/architecture/05-building-block-view.md`) on `origin/main` @ `376b106` and on the
  tracker, 2026-10-30. Merged: the record family and decoders (#654: PRs #725, #792, #793, #799), knobs
  (#801), Create (#908), Abort (#909), staging (#942, #965). Iterated: #657's first attempt (to Plan
  2026-10-19, oversized), whose commit-side defects are this slice's C1–C3 legs; #927's attempt 1
  (to Plan 2026-10-30), whose findings are (F), (G) and (H). Closed unmerged PRs (19 searched) touching
  these paths: none. Open PRs touching them: none. The new test file and `commit.rs` do not exist on
  `main`.
- **Disposition hint:** new-feature

## Design

**Quiesce, then commit** (`0016:660`, `:1720-1733`). Run the staged attempt's last renewal, with
a lease covering at least the dispatch margin (`E_TX_MILLIS`), and send no renewal after it. #933's
`StagedAttempt::renew` takes that coverage as `cover_millis`: pass at least that margin. Before building
the batch, read the bounded `psum:<id>:` range and refuse `EntityTooLarge` past
`MAX_STAGED_CHUNKS`, compensating what was staged. That read is unfenced on purpose: the overshoot
concurrent commits can add is bounded by the slot cap and charged in `U_REF` (`0016:1441-1451`),
so no precondition on the total belongs in the batch. Then commit one batch:
`require(mpu == Open@E)`; `require_absent(part:…)`, or `require(part:… == prior)` for a re-upload;
`require(slot:<id>:<k> == prior)`; put `part:` and its `psum:` summary; delete the slot; delete the
attempt's `sidx:` entries; and, for a re-upload, put one `retire:bytes:` obligation for the
replaced part's chunks (token per `0016:358-373`, installed under `require_absent`). The part
digest is the SHA-256 basis `Digest` already uses (`multipart.rs:1035-1052`, ADR-0047).

**Keep the lease, and check it before every commit round** (attempt 1's confirmed defect:
`commit.rs:627-631` discarded `Covered { checked_at_millis, lease_expiry_millis }`, so a probe's
fourth commit landed about 2 s after its slot lease expired and still answered `Committed`).
`stage.rs:376-378` says the commit checks both values against its own deadline; `0016:1726-1728`
says quiescing must not let the lease lapse under the commit. A renewal guarantees only its cover at
the moment it returns, and the scan, the reads and each retry spend from that. So read the attempt's
clock before each round, the first included, and send only with `E_TX_MILLIS` left. Do **not** renew
again to buy time: a renewal rewrites the slot the commit requires, which is what the quiesce exists
to prevent (`0016:1720-1726`), and it would need its own bound. When the lease is short, stop:
with a commit in doubt keep the doubt and return the in-doubt error; with nothing in doubt, compensate.

**What the lease check does and does not promise** (plan review, 2026-10-30). ⚑ This slice
guarantees the margin at dispatch only, and the Plan flags that narrowing for sign-off. `0016:1727-1728`
says the commit's deadline is "bounded by the `E_tx` envelope, so quiescing cannot let the lease
lapse under the commit". The store does not deliver that:

- `E_TX_MILLIS` is FoundationDB's five-second envelope (`multipart.rs:5113-5118`), not a deadline
  on `commit`. The trait promises termination, not five seconds, and a commit abandoned at a deadline
  may still apply (`crates/traits/src/lib.rs:1332-1345`). The drivers' own deadlines are 10 s for FDB
  (`crates/metadata-fdb/src/lib.rs:388`) and 50 s for TiKV (`crates/metadata-tikv/src/lib.rs:181`),
  both changeable by environment variable.
- `wyrd-core` cannot see those deadlines, and sizing the margin to them would not bound the landing
  either: a caller's timeout limits how long the writer waits, not when the store applies the batch
  (`0016:1560`, said there of fragment writes). So no check in this slice can stop a sent batch
  applying after its lease ran out.

So `E_TX_MILLIS` is a **dispatch margin**, 0016's chosen number for how much lease a commit should
start with. It lowers the odds of a stall outlasting the lease, and it rules out what attempt 1
did: sending a commit round on a lease already spent. Safety does not rest on it. The store enforces
bytes, never expiry (`0016:1851-1852`). A commit requires `mpu == Open@E` and its own slot's bytes, and
the idle fence requires the same session bytes and pins that slot (`0016:1859-1863`). Whichever lands
first makes the other's `require` fail, with no clock involved, so a late commit either lands
before any fence, which is sound, or does not land. A commit stalled past its lease can lose to the
fence and fail the request, which is an availability cost and allowed (`0016:2809-2813`). An expired
lease is therefore never evidence about whether a commit landed; only the reads below settle that.
This slice does not edit 0016; the wording at `:1727-1728` is for the human to take up there.

**A settled stop compensates (decided at sign-off, 2026-10-30).** `0016:1420-1431` says every
refusal after staging must compensate, or repeats fill `MAX_INFLIGHT_PARTS` and the usable session
answers `503`. These stops are errors rather than refusals, but the reason is the same, and attempt
1's "stays for the reaper" (`commit.rs:176-179`) is wrong for them: the reaper does not walk an
`Open` session (`0016:1600-1620`), so a session still in use holds those slots until `W_session`.

| The stop | Action |
|---|---|
| Renewal `Stopped`: `CoverShort`, `RenewalRoundsSpent`, `LeaseLapsed`, `InvalidRequest` | Compensate |
| Commit rounds spent, nothing in doubt | Compensate |
| Lease short before a commit round, nothing in doubt | Compensate |
| Renewal `InDoubt`, or any commit round still in doubt | Keep the doubt; no compensation |
| `SlotLost`, `EntryLost` | No compensation: the attempt cannot show the records are still its own |
| Session has left `Open` | Answer the refusal; the residue is the reaper's |
| `Stop::Store`, `Stop::Record` | No compensation: the store is failing or a record is damaged; the error says the residue stays. This is the same class of residue a crash leaves (`0016:1778`) |

A lapsed lease does not block compensating. Renewal refuses to revive a lapsed lease
(`0016:1845`), but compensation revives nothing, is clock-independent (`0016:1651-1653`), and lands
only if the session and slot are unchanged. After a renewal stop, the attempt already holds the
bytes the store holds (`stage.rs:561`, `:1771-1772`). The compensation's landing does not change the
answer into a refusal: the caller gets the original stop's typed error, which reports whether the
slot was released; the wire mapping is #669's.

**After a `Conflict`, classify from a re-read** (`0016:1597-1647`). Session fenced (non-`Open`) ⇒
`NoSuchUpload`, no self-compensation. Session back at a newer `Open@E'` ⇒ compensate under
`Open@E'`, chasing a further rollback a bounded number of times. Part key moved (same-part rival)
⇒ compensate. Own slot moved with the part unchanged ⇒ benign retry. **Compensation** is one
batch under `require(mpu == Open@E)` and `require(slot == prior)`: one `retire:bytes:` obligation
naming this attempt's own chunks, the slot delete, and the `sidx:` deletes (`0016:673`).

**Empty chunk lists.** An obligation must name something: `RetireObligation` refuses an empty
`chunks` list and an empty payload (`multipart.rs:3752`), and this slice
keeps that contract. The compensation and the re-upload install their `retire:bytes:{chunks}`
obligation only when the list is non-empty; the slot release and `sidx:` deletes are in the same
batch regardless. An unknown outcome of a compensation is settled by re-reading, in order, its
obligation (when it has one), its own slot, then the session: the obligation present, or this
attempt's slot gone while the session is still the epoch it compensated under, means it landed.
That second test is sound **only because no commit of this attempt is outstanding** when it
compensates (next paragraph): the slot CAS proves that one of this attempt's slot-releasing
batches landed, not which one. For an empty part there is no obligation, so the slot and session
rule alone decides; with `may_still_commit: true` the doubt is kept as for the commit.

**Settle before compensating** (`0016:1649-1651`: after a `CommitUnknownResult` "only a settled
*not-committed* outcome enters the classification"). The attempt sends a compensation batch only
once **every** commit round it sent is settled as not landed and unable to land later. That takes
two facts, and the order of the reads is what makes them true.

- *Unable to land later:* the round's outcome was `may_still_commit: false` (out of flight), or a
  read found a precondition of the commit that can never hold again — the session no longer
  `Open@E` (the epoch only rises), or the part key no longer holding what the commit requires, when
  that value cannot come back — the commit required the key absent and it is now present (a
  `part:` record is removed only after the session leaves `Open`), or it required a **non-empty**
  prior record, whose chunk ids no later write can carry. An empty prior's bytes **can** come back
  (C1: an empty rival in the same millisecond writes them again), so a rival's empty record settles
  nothing.
- *Did not land:* a slot read made **after** that error returned, or after that precondition read,
  found the attempt's own slot still at the bytes it last wrote. The commit deletes the slot, so a
  commit that had landed by then would have left it gone.

A slot read made **before** the read that found the precondition gone proves nothing. A held commit
can land between the two, and a rival or a rollback can follow it: the slot read saw the attempt
holding, the part read saw the rival, the session read saw `Open@E`, and the attempt committed
(`stage.rs:1713-1732` guards the same history). So a change first seen after the last slot read
sends the attempt back to read the slot again. If that read finds the slot gone, it is the
own-commit proof below, or the in-doubt error if the session moved. It never leads to a compensation.
Until it is settled, the attempt may re-read and may retry the
commit (both requiring the same slot bytes, so at most one lands), but not compensate; when its
rounds run out it returns the in-doubt error.

**The answer for a lost commit on a live session.** When the session is still `Open` and the
attempt compensates — a same-number rival won, or a Complete fenced and rolled the session back
to `Open@E'` — it answers exactly `Refused(OperationAborted)`: a conflicting operation on the
resource intervened and the client may retry the part, which is what S3's
`409 OperationAborted` says. That variant's doc says only "fenced into `Completing`"
(`multipart.rs:4557-4559`); widen it in this slice to cover these two cases too, and add no new
`Refusal` variant. ⚑ This choice is the Plan's (2026-10-20) and is flagged for sign-off; #669
maps it to the wire.

**After a `CommitUnknownResult`, settle identity, not presence** (`0016:1630-1651`: "finds its own
commit present"). `PartRecord` carries no attempt id, and an empty part carries no chunk ids
either, so the part's bytes alone do not identify the attempt (C1). The commit batch deletes the
attempt's own slot. While the session is `Open@E`, the writers of slot index `k` are this
attempt's own batches and, **once this attempt has released it**, whichever later attempt
reserves the freed index (`0016:1625-1628`); no reaper touches an `Open` session. A later
reserve needs `require_absent(slot:<id>:<k>)`, so it can only follow this attempt's own delete.
So the evidence is **ownership, not key presence**: "this attempt's slot is gone" means the
key is absent **or** holds a record whose `attempt_id` is not this attempt's
(`multipart.rs:2796`), and that identifies this attempt's commit, provided (a) this attempt
sent no compensation that could have deleted it, and (b) the session was **still the same
`Open@E` after** the slot was observed gone (C3′). Every later batch of this attempt requires
the slot bytes **this attempt last wrote**, never bytes re-read from the key, so a slot another
attempt now holds is never required, rewritten or deleted by this one. A session state read before the
slot read cannot vouch for it (C2). The two proofs need opposite orders: the own-commit proof
needs a session read after the slot read, and settling needs a slot read after the session and
part reads. One round that gives both reads the part and the session, then the slot, then the session
again; a session the last read found moved settles nothing until the slot is read again. Anything that does not settle it proves "did not land" **only**
when `may_still_commit` is `false` (`crates/traits/src/lib.rs:240-247`). With `true`, keep the
doubt the way #895's Abort does: every later batch of this attempt requires its own slot at the
bytes the held batch also requires, and the held batch deletes that slot, so whichever lands
first excludes the other. A commit that may have landed stays in the doubt until a read settles
it (C3). **Order of the checks on each re-read:** first the own-commit proof above (slot gone or
another attempt's, session still `Open@E`, no compensation sent) ⇒ `Committed`, whatever the part
key now holds (C3, C3′); only if it fails, the classification of a lost commit (session moved,
rival record, benign retry), and the rival-record branch only once the commit is settled (*Settle
before compensating*). Bound the rounds; if they run out unsettled, return an in-doubt error
carrying the doubt, never a guess.

**Writer constructor.** `PartRecord` has no writer-side constructor today ("No writer-side
constructor — see `SessionRecord`'s doc", `multipart.rs:2990`). Add a crate-private one in the
shape of `SessionRecord::open`, so every value written is one the decoder accepts back unchanged.
`PartSummary` has public fields (`:3092-3101`); build the `psum:` value from the `PartRecord` it
summarizes, so the pair cannot disagree (`:3085-3089`).

**An over-limit body (from #932's Check, decided 2026-10-30).** `stage_part` answers
`StageOutcome::Staged` for a body past `max_part_chunks`, holding a prefix whose `len()` and
`digest()` describe only that prefix. The variant's doc says "every chunk is registered and its
fragments are written" (`stage.rs:363`), which is wrong for this case: correct it. Do not add a
variant. `commit_part` is the only route to a `part:` record, so its first step refuses such an
attempt and compensates, and a test fails if that step is removed.

**Docs currency** (`AGENTS.md:169-172`). Add one **new, short paragraph** to
`docs/design/architecture/05-building-block-view.md`, between the staging paragraph (`:246`) and the
`mpufence` paragraph (`:248`), and edit neither. Those are very long single lines, and attempt 1's
rewrite of `:246` cost 20 KB of diff for one sentence. Say: `part:` and `psum:` now have their writer,
the commit lands them with the slot release and `sidx:` deletes in one batch, and nothing calls it in
production until #669.

## Alternatives considered

- **A per-session in-flight counter.** A shared writable key on the part path; commits starve on
  unrelated parts' releases (`0016:1686-1742`). Rejected; the slot key space is the cap.
- **Compensate on any non-`Committed` outcome.** It orphan-marks a part that exists after an
  unknown outcome (`0016:1792`). Rejected; re-read first.
- **Leave a settled stop's residue for the reaper** (attempt 1). It lets repeated stops fill a live
  session's slots (`0016:1420-1431`). Rejected at sign-off, 2026-10-30.
- **Renew again when the lease runs short.** It adds a writer of the slot the commit requires, and a
  new bound. Rejected; stop instead.
- **Size the margin to the backend's real deadline, or wrap the commit in a caller timeout, to promise
  coverage until the batch applies.** `wyrd-core` cannot see the driver's deadline (50 s by default
  on TiKV), and neither limits when an outstanding batch applies (`crates/traits/src/lib.rs:1343-1345`).
  Rejected; the promise is narrowed to dispatch, and the `require`s carry safety.
- **A new `StageOutcome` variant for an over-limit body** (the #932 Check's suggestion). It only adds
  real protection with a separate attempt type, a larger edit to merged staging code than the risk
  warrants, since `commit_part` is the one route to a part. Rejected at sign-off, 2026-10-30.
- **An attempt id in `PartRecord`, to make the bytes attempt-specific.** It changes a landed
  record shape and its decoder, which #658 and #659 read, and the slot already identifies the
  attempt. Rejected for this slice.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Human override of the size backstop (225 KB vs 100 KB): do NOT split; rebuild this slice and make the fixes below. Classifying and compensating a lost commit depend on each other, and this list is small. Keep the settle order and the own-commit proof as they are: no false Committed and no compensation during a live commit was found. Fixes: 1. Bug: crates/core/src/multipart/commit.rs:857 (with the Same arm at :939) treats a damaged part: record on the re-read as a same-number rival. A commit answered Conflict with b"not a part record" at the part key gives Refused(OperationAborted) and a compensation. Decode part_now when it differs from required, return a decode failure through torn as Stop::Record with no compensation, and add this leg, including the held-commit path. 2. Add a test that fails under each edit: - commit.rs:788, can_recur forced to false: empty prior; commit held; a non-empty rival lands; when a compensation is sent, an empty rival in the same millisecond restores the prior's bytes, the held commit lands, and the compensation answers unknown. Expect the in-doubt error with no compensation. - commit.rs:620, the ceiling's exclusion of the part's own prior disabled: psum: planted at MAX_STAGED_CHUNKS - 1, a committed one-chunk part 1, then a one-chunk re-upload of part 1. Expect Committed. - commit.rs:646-648, require_absent on the obligation key removed: other bytes planted at the key, then a re-upload over a non-empty prior. Expect those bytes and the prior kept, ending in ObligationKeyTaken. - commit.rs:523-527, the doubt search replaced with .last(): round 1 held, round 2 Unknown { may_still_commit: false }, then the next read faults. Expect may_still_commit: true. - commit.rs:1081, the epoch chase keeping only the latest obligation: the loser's first compensation held; right after its slot read that batch lands and the session rolls back; the second round conflicts. Expect OperationAborted. 3. Cover the never-run "slot gone and session fenced" arms at commit.rs:906-911, :1026 and :1137. 4. Full CI: the last C4-ci hit its 7200 s limit in custodian_gc, so later test binaries and the machete, deny and conformance checks never ran. Criterion (E) needs a complete cargo xtask ci run. If custodian_gc stalls again, find out why before calling it unrelated. Decided at sign-off: - Slots left held when a Complete fence rolls back (commit.rs:736, :950; multipart.rs:4936): accept for this slice, until W_session. Do not change this behaviour here. It is to be raised against proposal 0016; do not edit 0016 in this slice. - A rollback before the quiesce renewal is adopted by that renewal and may answer Committed under the live epoch (stage.rs:1817-1823). The code is right. Read (B)'s rollback rule as covering a rollback between the last renewal and the commit. - The lease promise covers dispatch only, narrower than 0016's deadline wording: ratified. - Refused(OperationAborted) for a compensated live-session loss: ratified.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human override of the size backstop (225 KB vs 100 KB): do NOT split; rebuild this slice and make the fixes below. Classifying and compensating a lost commit depend on each other, and this list is small. Keep the settle order and the own-commit proof as they are: no false Committed and no compensation during a live commit was found.

  Fixes:
  1. Bug: crates/core/src/multipart/commit.rs:857 (with the Same arm at :939) treats a damaged part: record on the re-read as a same-number rival. A commit answered Conflict with b"not a part record" at the part key gives Refused(OperationAborted) and a compensation. Decode part_now when it differs from required, return a decode failure through torn as Stop::Record with no compensation, and add this leg, including the held-commit path.
  2. Add a test that fails under each edit:
     - commit.rs:788, can_recur forced to false: empty prior; commit held; a non-empty rival lands; when a compensation is sent, an empty rival in the same millisecond restores the prior's bytes, the held commit lands, and the compensation answers unknown. Expect the in-doubt error with no compensation.
     - commit.rs:620, the ceiling's exclusion of the part's own prior disabled: psum: planted at MAX_STAGED_CHUNKS - 1, a committed one-chunk part 1, then a one-chunk re-upload of part 1. Expect Committed.
     - commit.rs:646-648, require_absent on the obligation key removed: other bytes planted at the key, then a re-upload over a non-empty prior. Expect those bytes and the prior kept, ending in ObligationKeyTaken.
     - commit.rs:523-527, the doubt search replaced with .last(): round 1 held, round 2 Unknown { may_still_commit: false }, then the next read faults. Expect may_still_commit: true.
     - commit.rs:1081, the epoch chase keeping only the latest obligation: the loser's first compensation held; right after its slot read that batch lands and the session rolls back; the second round conflicts. Expect OperationAborted.
  3. Cover the never-run "slot gone and session fenced" arms at commit.rs:906-911, :1026 and :1137.
  4. Full CI: the last C4-ci hit its 7200 s limit in custodian_gc, so later test binaries and the machete, deny and conformance checks never ran. Criterion (E) needs a complete cargo xtask ci run. If custodian_gc stalls again, find out why before calling it unrelated.

  Decided at sign-off:
  - Slots left held when a Complete fence rolls back (commit.rs:736, :950; multipart.rs:4936): accept for this slice, until W_session. Do not change this behaviour here. It is to be raised against proposal 0016; do not edit 0016 in this slice.
  - A rollback before the quiesce renewal is adopted by that renewal and may answer Committed under the live epoch (stage.rs:1817-1823). The code is right. Read (B)'s rollback rule as covering a rollback between the last renewal and the commit.
  - The lease promise covers dispatch only, narrower than 0016's deadline wording: ratified.
  - Refused(OperationAborted) for a compensated live-session loss: ratified.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design. Only the items below change. 1. Bug: decode each session read before treating it as evidence the session moved (`commit.rs:496`, called at `:878`, `:896`, `:1136`). Only a record that decodes and is not `Open`, or is at a higher epoch, counts as moved. Undecodable bytes settle nothing and keep the doubt and `may_still_commit` as they were. Add held-commit and held-compensation tests that damage the session, check the doubt survives, repair the session and land the batch. Include the damage-repaired-mid-round history, which must not answer `Refused(OperationAborted)` or send a compensation. 2. A missing `part:` record for a non-empty prior with the session still `Open@E` (`commit.rs:870`, `:954`) is damage: return an explicit record error, not `OperationAborted`, and test it. 3. Add tests: every commit round answering unknown with `may_still_commit: false` must compensate and return `Stopped(CommitRoundsSpent)` with `slot_released: true` (`:490`); a slot rewritten under the attempt's own id during a held commit must answer `InDoubt(SlotLost)` (`:579`); the "session gone" row (`:598`). 4. `:1187`: delete it, or add a test that reaches it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design. Only the items below change.
  1. Bug: decode each session read before treating it as evidence the session moved (`commit.rs:496`, called at `:878`, `:896`, `:1136`). Only a record that decodes and is not `Open`, or is at a higher epoch, counts as moved. Undecodable bytes settle nothing and keep the doubt and `may_still_commit` as they were. Add held-commit and held-compensation tests that damage the session, check the doubt survives, repair the session and land the batch. Include the damage-repaired-mid-round history, which must not answer `Refused(OperationAborted)` or send a compensation.
  2. A missing `part:` record for a non-empty prior with the session still `Open@E` (`commit.rs:870`, `:954`) is damage: return an explicit record error, not `OperationAborted`, and test it.
  3. Add tests: every commit round answering unknown with `may_still_commit: false` must compensate and return `Stopped(CommitRoundsSpent)` with `slot_released: true` (`:490`); a slot rewritten under the attempt's own id during a held commit must answer `InDoubt(SlotLost)` (`:579`); the "session gone" row (`:598`).
  4. `:1187`: delete it, or add a test that reaches it.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design. One design rule is corrected, and this rationale overrides the brief where they differ. Start from this attempt's patch. Why rejected: three high-effort review passes on this patch each independently flagged `crates/core/src/multipart/commit.rs:904` (`Ok(Some(_)) => doubt.cannot_land()`). The brief's rule that a changed non-empty prior part record "never comes back" is false, because custodian staged reconstruction rewrites `part:` records while the session is `Open@E` (`crates/custodian/src/reconstruction/staged.rs`, `0016:671`) and may be able to move a part back to its earlier placement. History: a re-upload's commit is held; repair moves the prior; the attempt decides the commit cannot land and compensates; a second repair restores the prior's bytes; the held commit lands. A compensation with an unknown outcome then sees the slot gone with the session unchanged and answers `Refused(OperationAborted)` for a part that committed (`compensate`), and one answered `Conflict` ends in a definite `SlotLost`. This was confirmed by reading the code, not by running it. Check `repointed_part` for whether the exact bytes can return, but make the fix regardless: the rule rests on an assumption reconstruction does not honour. Changes: 1. Fix: a changed prior part record, empty or non-empty, no longer proves a held commit cannot land. Treat a non-empty prior as `can_recur` treats an empty one, and keep the doubt. Only `may_still_commit: false` or a session that has moved can settle it; a key that was required absent and is now present still settles it. More in-doubt answers are an allowed availability cost. Correct the code comment and module docs that state the old rule. 2. New test: a held commit while repair moves the prior away and back, using real re-place batches or their exact shape. Required: no compensation sent while the commit may land, never a false `Refused(OperationAborted)`, and the slot released exactly once. 3. New test, with the corrected answer: a held re-upload over a non-empty prior that loses to a rival answers the in-doubt error with no compensation, not `Refused(OperationAborted)`. It must fail if `can_recur` goes back to treating a non-empty prior as settled. Update any existing leg that expects the old answer. 4. Add the test gaps the adversary found, each failing under the named edit: - Store faults inside `compensate` (`commit.rs:1151`, `:1162`, `:1170`, `:1177`): replacing `end(doubt, ...)` with a plain stop on each. Hold the compensation, fault the next obligation, slot or session read, and expect `InDoubt { batch: Compensation, may_still_commit: true }`. `:1151` needs a hold followed by a failed round. - A re-upload above epoch 0 (`commit.rs:812-814`): the obligation's epoch hard-coded to 0. Commit part 1, stage a re-upload, roll the session back before the quiesce renewal, commit, and expect the obligation under the live epoch. - A damaged `part:` record after the session has moved (`commit.rs:970`): a commit answered `Conflict`, `b"not a part record"` at the part key, the session at `Open@E'`. Expect `Stop::Record` with no compensation. - `commit.rs:1125`, `:1135`: delete these two `Stop::Record` paths if no legal history reaches them, or add a test that reaches them. Decided at sign-off: - With nothing in doubt, a part record changed by a plain repair keeps the existing classification: compensate and answer the retryable `Refused(OperationAborted)`. Do not add a retry against the new record. - The size backstop stays overridden: do not split. Do not trim tests to reduce size. - Keep the earlier ratified decisions: the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, and slots left held after a rollback. Review caveat: the T4 gate's default-effort passes missed this bug 3 of 3 and missed the known v3 bug 3 of 3 in a re-test, while high-effort passes caught this bug 3 of 3 and the v3 bug 2 of 3. A clean T4 result at default settings is weak evidence, so do not rely on it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design. One design rule is corrected, and this rationale overrides the brief where they differ. Start from this attempt's patch.

  Why rejected: three high-effort review passes on this patch each independently flagged `crates/core/src/multipart/commit.rs:904` (`Ok(Some(_)) => doubt.cannot_land()`). The brief's rule that a changed non-empty prior part record "never comes back" is false, because custodian staged reconstruction rewrites `part:` records while the session is `Open@E` (`crates/custodian/src/reconstruction/staged.rs`, `0016:671`) and may be able to move a part back to its earlier placement. History: a re-upload's commit is held; repair moves the prior; the attempt decides the commit cannot land and compensates; a second repair restores the prior's bytes; the held commit lands. A compensation with an unknown outcome then sees the slot gone with the session unchanged and answers `Refused(OperationAborted)` for a part that committed (`compensate`), and one answered `Conflict` ends in a definite `SlotLost`. This was confirmed by reading the code, not by running it. Check `repointed_part` for whether the exact bytes can return, but make the fix regardless: the rule rests on an assumption reconstruction does not honour.

  Changes:
  1. Fix: a changed prior part record, empty or non-empty, no longer proves a held commit cannot land. Treat a non-empty prior as `can_recur` treats an empty one, and keep the doubt. Only `may_still_commit: false` or a session that has moved can settle it; a key that was required absent and is now present still settles it. More in-doubt answers are an allowed availability cost. Correct the code comment and module docs that state the old rule.
  2. New test: a held commit while repair moves the prior away and back, using real re-place batches or their exact shape. Required: no compensation sent while the commit may land, never a false `Refused(OperationAborted)`, and the slot released exactly once.
  3. New test, with the corrected answer: a held re-upload over a non-empty prior that loses to a rival answers the in-doubt error with no compensation, not `Refused(OperationAborted)`. It must fail if `can_recur` goes back to treating a non-empty prior as settled. Update any existing leg that expects the old answer.
  4. Add the test gaps the adversary found, each failing under the named edit:
     - Store faults inside `compensate` (`commit.rs:1151`, `:1162`, `:1170`, `:1177`): replacing `end(doubt, ...)` with a plain stop on each. Hold the compensation, fault the next obligation, slot or session read, and expect `InDoubt { batch: Compensation, may_still_commit: true }`. `:1151` needs a hold followed by a failed round.
     - A re-upload above epoch 0 (`commit.rs:812-814`): the obligation's epoch hard-coded to 0. Commit part 1, stage a re-upload, roll the session back before the quiesce renewal, commit, and expect the obligation under the live epoch.
     - A damaged `part:` record after the session has moved (`commit.rs:970`): a commit answered `Conflict`, `b"not a part record"` at the part key, the session at `Open@E'`. Expect `Stop::Record` with no compensation.
     - `commit.rs:1125`, `:1135`: delete these two `Stop::Record` paths if no legal history reaches them, or add a test that reaches them.

  Decided at sign-off:
  - With nothing in doubt, a part record changed by a plain repair keeps the existing classification: compensate and answer the retryable `Refused(OperationAborted)`. Do not add a retry against the new record.
  - The size backstop stays overridden: do not split. Do not trim tests to reduce size.
  - Keep the earlier ratified decisions: the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, and slots left held after a rollback.

  Review caveat: the T4 gate's default-effort passes missed this bug 3 of 3 and missed the known v3 bug 3 of 3 in a re-test, while high-effort passes caught this bug 3 of 3 and the v3 bug 2 of 3. A clean T4 result at default settings is weak evidence, so do not rely on it.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design and start from this attempt's patch. Why rejected: the T4 gate fails on one finding, confirmed by running it. Fixes: 1. Bug: `crates/core/src/multipart/commit.rs:666`, `judge_session` treats any decodable non-`Open` record as a real fence, whatever its epoch. Every legal fence raises the epoch, so a non-`Open` record at the same or a lower epoch is damage and settles nothing. Return `Left` only when `record.epoch() > required.epoch`; otherwise return `SessionEpochNotRisen`, keeping the doubt and `may_still_commit`. The adversary's trial of this change passed the 57 existing tests and its three asserting probes. This narrows the iteration 3 wording that a record that "decodes and is not `Open`" counts as moved; it changes no legal history. 2. Tests: add a non-`Open` record at the same epoch and at a lower epoch to `session_damage` (`crates/core/tests/multipart_upload_part_commit.rs:4008-4021`), and use one in the repaired-mid-round leg (`:4117`, `:4154`). Cover both a held commit and a held compensation. These tests must fail if the fix is reverted. 3. Docs: widen `crates/core/src/multipart.rs:386-394` and `commit.rs:88-92`, which say only `Open` records can be this damage. Not decided at this sign-off: the missing `mpu:` record with the attempt's slot still held (`commit.rs:645-647`, pinned at `multipart_upload_part_commit.rs:4511-4521`). Do not change this behaviour in this rebuild. Keep the earlier decisions: the size backstop stays overridden, do not trim tests to reduce size, the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, and slots left held after a rollback.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design and start from this attempt's patch. Why rejected: the T4 gate fails on one finding, confirmed by running it.

  Fixes:
  1. Bug: `crates/core/src/multipart/commit.rs:666`, `judge_session` treats any decodable non-`Open` record as a real fence, whatever its epoch. Every legal fence raises the epoch, so a non-`Open` record at the same or a lower epoch is damage and settles nothing. Return `Left` only when `record.epoch() > required.epoch`; otherwise return `SessionEpochNotRisen`, keeping the doubt and `may_still_commit`. The adversary's trial of this change passed the 57 existing tests and its three asserting probes. This narrows the iteration 3 wording that a record that "decodes and is not `Open`" counts as moved; it changes no legal history.
  2. Tests: add a non-`Open` record at the same epoch and at a lower epoch to `session_damage` (`crates/core/tests/multipart_upload_part_commit.rs:4008-4021`), and use one in the repaired-mid-round leg (`:4117`, `:4154`). Cover both a held commit and a held compensation. These tests must fail if the fix is reverted.
  3. Docs: widen `crates/core/src/multipart.rs:386-394` and `commit.rs:88-92`, which say only `Open` records can be this damage.

  Not decided at this sign-off: the missing `mpu:` record with the attempt's slot still held (`commit.rs:645-647`, pinned at `multipart_upload_part_commit.rs:4511-4521`). Do not change this behaviour in this rebuild.

  Keep the earlier decisions: the size backstop stays overridden, do not trim tests to reduce size, the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, and slots left held after a rollback.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design and start from this attempt's patch. Why rejected: no legal history breaks it, but two damaged-record histories still give a wrong answer, and three doubt-keeping paths have no test that fails if they drop the doubt. Fixes: 1. Bug, now decided: a missing `mpu:` record with this attempt's slot still held is damage, not a terminal delete. A real terminal delete removes surviving `slot:` records in the same batch (`0016:674`). `commit.rs:653-655` and `commit.rs:90-91` read the absence as a terminal delete: with a commit held and `mpu:` erased, the answer is `Refused(NoSuchUpload)` with `slots=1`, and after the record is restored the held batch lands and the part is durable. Keep the doubt and `may_still_commit`, as `SessionEpochNotRisen` does. Correct the test doc at `multipart_upload_part_commit.rs:4559` ("never damage") and the comment at `:4577` ("it can never land"), update the legs that expect the old answer, and add a leg that restores the record and lands the held batch. Cover a held commit and a held compensation. These tests must fail if the fix is reverted. 2. Quiesce renewal: `stage.rs:1103-1112`, reached through `stage.rs:1797-1800`, has no epoch check, and `commit.rs:806-815` trusts its answer. Do not change `stage.rs` behaviour; the slice's one doc-fix limit on that file stands. Correct the claim at `commit.rs:88-89` that every session read is judged by `judge_session`. In `commit.rs`, compare the epoch before and after the renewal, so an `Open` record at a lower epoch, or under other bytes at the same epoch, is not adopted. Add a test that fails without this check. The non-`Open` refusal case stays a known gap: record it in `build-notes.md` and the doc. 3. Add tests, each failing under the named edit: `commit.rs:962` (undecodable slot under a held commit), `:1208` (`ObligationKeyTaken` under a held compensation), `:1217` (undecodable slot under a held compensation). For each, replace the doubt-keeping return with a plain stop. Hold the batch, damage the key, and assert the `InDoubt` batch, flag and cause. 4. Test guard: `multipart_upload_part_commit.rs:590` takes the attempt id from the batch's own `expected` bytes. Supply the expected attempt identity independently of the batch, compare it before the lookup, and show the guard rejects a batch carrying another recorded attempt's bytes. 5. Doc: add a repair rewriting the prior part as a cause in the `Refusal::OperationAborted` doc (`crates/core/src/multipart.rs`, the list just above `:4726`; `commit.rs:1039-1043` already answers it). Decided at sign-off: - The size backstop stays overridden: do not split. Do not trim tests to reduce size. - Keep the earlier decisions: the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, slots left held after a rollback, and a plain-repair changed part compensating with nothing in doubt. Review caveat: the T4 row on this attempt does not count as a deep review. Its three passes took 10-16 s each, ran 0 commands and used 217-311 reasoning tokens, against 120-320 s, 9-15 commands and 2,738-7,863 tokens for the previous patch at high effort. `scripts/review-branch` sets no effort level, and `review-passes.json` still carries the previous patch's hash. The next sign-off needs a confirmed high-effort review of the final patch.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design and start from this attempt's patch. Why rejected: no legal history breaks it, but two damaged-record histories still give a wrong answer, and three doubt-keeping paths have no test that fails if they drop the doubt.

  Fixes:
  1. Bug, now decided: a missing `mpu:` record with this attempt's slot still held is damage, not a terminal delete. A real terminal delete removes surviving `slot:` records in the same batch (`0016:674`). `commit.rs:653-655` and `commit.rs:90-91` read the absence as a terminal delete: with a commit held and `mpu:` erased, the answer is `Refused(NoSuchUpload)` with `slots=1`, and after the record is restored the held batch lands and the part is durable. Keep the doubt and `may_still_commit`, as `SessionEpochNotRisen` does. Correct the test doc at `multipart_upload_part_commit.rs:4559` ("never damage") and the comment at `:4577` ("it can never land"), update the legs that expect the old answer, and add a leg that restores the record and lands the held batch. Cover a held commit and a held compensation. These tests must fail if the fix is reverted.
  2. Quiesce renewal: `stage.rs:1103-1112`, reached through `stage.rs:1797-1800`, has no epoch check, and `commit.rs:806-815` trusts its answer. Do not change `stage.rs` behaviour; the slice's one doc-fix limit on that file stands. Correct the claim at `commit.rs:88-89` that every session read is judged by `judge_session`. In `commit.rs`, compare the epoch before and after the renewal, so an `Open` record at a lower epoch, or under other bytes at the same epoch, is not adopted. Add a test that fails without this check. The non-`Open` refusal case stays a known gap: record it in `build-notes.md` and the doc.
  3. Add tests, each failing under the named edit: `commit.rs:962` (undecodable slot under a held commit), `:1208` (`ObligationKeyTaken` under a held compensation), `:1217` (undecodable slot under a held compensation). For each, replace the doubt-keeping return with a plain stop. Hold the batch, damage the key, and assert the `InDoubt` batch, flag and cause.
  4. Test guard: `multipart_upload_part_commit.rs:590` takes the attempt id from the batch's own `expected` bytes. Supply the expected attempt identity independently of the batch, compare it before the lookup, and show the guard rejects a batch carrying another recorded attempt's bytes.
  5. Doc: add a repair rewriting the prior part as a cause in the `Refusal::OperationAborted` doc (`crates/core/src/multipart.rs`, the list just above `:4726`; `commit.rs:1039-1043` already answers it).

  Decided at sign-off:
  - The size backstop stays overridden: do not split. Do not trim tests to reduce size.
  - Keep the earlier decisions: the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, slots left held after a rollback, and a plain-repair changed part compensating with nothing in doubt.

  Review caveat: the T4 row on this attempt does not count as a deep review. Its three passes took 10-16 s each, ran 0 commands and used 217-311 reasoning tokens, against 120-320 s, 9-15 commands and 2,738-7,863 tokens for the previous patch at high effort. `scripts/review-branch` sets no effort level, and `review-passes.json` still carries the previous patch's hash. The next sign-off needs a confirmed high-effort review of the final patch.
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision at sign-off: return to Plan and split this slice. Why: attempt 7's patch is 333 KB against the 100 KB signal, up from 225 KB when the split was first overridden, and each rebuild still finds something new. This decision ends the earlier "do not split" override. Author the split in Plan (`pdca split`). Findings on attempt 7 for the split to place (none was decided at this sign-off): 1. Legal history, wrong answer: a terminal delete landing after the slot read reports `Stop::Record(SessionRecordMissing)` instead of `Refused(NoSuchUpload)` (`crates/core/src/multipart/commit.rs:1061-1066`, `:1337-1347`). The adversary's suggested fix is to read the slot once more before calling it damage. `crates/core/tests/multipart_upload_part_commit.rs:4687-4688` pins the defect. 2. T4 blocking: a settled renewal error bypasses the adopted-session epoch check, so compensation runs under damaged session bytes (`commit.rs:834`, `:846`). Do not drop a renewal `InDoubt`'s doubt when fixing this. 3. Damage only, false success: with the slot erased and the session untouched, the own-commit proof answers `Committed` with no part record (`commit.rs:1012-1018`); the compensation has the same gap (`:1290`). Whether to fix this is still open. 4. Test gaps: the slot guard accepts A's cleanup carrying B's slot bytes (`multipart_upload_part_commit.rs:605`), and no test covers a lower-epoch adoption (`commit.rs:846`). 5. `commit.rs:843-845` records a known gap by citing `build-notes.md`; it needs a tracking issue. 6. Review depth: the T4 row ran three passes in 36.92 s in total, with no effort level recorded, so it does not show the high-effort review the last sign-off required.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision at sign-off: return to Plan and split this slice. Why: attempt 7's patch is 333 KB against the 100 KB signal, up from 225 KB when the split was first overridden, and each rebuild still finds something new. This decision ends the earlier "do not split" override. Author the split in Plan (`pdca split`).

  Findings on attempt 7 for the split to place (none was decided at this sign-off):
  1. Legal history, wrong answer: a terminal delete landing after the slot read reports `Stop::Record(SessionRecordMissing)` instead of `Refused(NoSuchUpload)` (`crates/core/src/multipart/commit.rs:1061-1066`, `:1337-1347`). The adversary's suggested fix is to read the slot once more before calling it damage. `crates/core/tests/multipart_upload_part_commit.rs:4687-4688` pins the defect.
  2. T4 blocking: a settled renewal error bypasses the adopted-session epoch check, so compensation runs under damaged session bytes (`commit.rs:834`, `:846`). Do not drop a renewal `InDoubt`'s doubt when fixing this.
  3. Damage only, false success: with the slot erased and the session untouched, the own-commit proof answers `Committed` with no part record (`commit.rs:1012-1018`); the compensation has the same gap (`:1290`). Whether to fix this is still open.
  4. Test gaps: the slot guard accepts A's cleanup carrying B's slot bytes (`multipart_upload_part_commit.rs:605`), and no test covers a lower-epoch adoption (`commit.rs:846`).
  5. `commit.rs:843-845` records a known gap by citing `build-notes.md`; it needs a tracking issue.
  6. Review depth: the T4 row ran three passes in 36.92 s in total, with no effort level recorded, so it does not show the high-effort review the last sign-off required.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
