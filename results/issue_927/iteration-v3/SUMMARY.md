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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.8% — 619 of 633 instrumentable changed lines executed (floor 80%); 633 of 1406 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 78 mutants tested in 6m: 63 caught, 15 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 20.16s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The UploadPart commit slice (#927)—atomic part publication, outcome classification, compensation, re-upload and ceilings—needs one implementation correction: malformed session records can erase doubt about a batch that still lands.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable outcomes, read ordering, lease boundaries and settled scope decisions; the requested surface is specific and testable (brief.md:31, brief.md:269, brief.md:640). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no baseline commit API; the independent stash run fails to import the module before executing any test, so it establishes no behavioral red (brief.md:321; reviewer-red-green.log:6). The acceptance decision is recorded under C4. |
| C3 Change | PASS | The five-file patch stays within the approved core-only slice and updates its living architecture; it adds no production caller or protocol-schema change (patch.diff:1; crates/core/src/multipart/commit.rs:151; docs/design/architecture/05-building-block-view.md:248). The correctness defect is recorded under C5. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the predeclared lack of an executable baseline red—44/44 tests pass after restoration, but the reverted API cannot compile; frozen full CI passed and independent CI has the target-copy caveat below (brief.md:328; reviewer-red-green.log:6; reviewer-red-green.log:83; gate-logs/C4-ci.log:4540). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Validate session observations before changing outstanding doubt—three independent probes show a definite stop or false may_still_commit despite a later successful landing (crates/core/src/multipart/commit.rs:878, :896, :1136; reviewer-probe.log:69). |
| T1 Structure | PASS | The core module uses the existing metadata/chunk-store traits and staging hand-off without a concrete backend or runtime dependency in production (crates/core/src/multipart/commit.rs:164, :168, :1202). |
| T2 Shape | PASS | Writer constructors and derived summaries preserve record identity; the test wrapper checks decoding and slot preconditions on every submitted batch (crates/core/src/multipart.rs:3029, :3058, :4102; crates/core/tests/multipart_upload_part_commit.rs:464, :570). |
| T3 Runtime | PASS | Retry budgets, dispatch-margin checks and one caller-owned clock bound the exercised paths; external operations inherit the backend termination contract (crates/core/src/multipart/commit.rs:193, :200, :817; crates/traits/src/lib.rs:1334). Outcome-classification correctness is assessed under C5. |
| T4 Contribution | FAIL | The frozen multi-pass review's session-validation finding remains reproducible; the independent affected-path history/closed-PR check found no conflicting prior art, and contribution artifacts are N/A until their mandatory publish audit (gate-logs/T4-batch-review.log:10; reviewer-prior-art.json:10; gate-logs/T4-contribution.log:10). |
| T5 Judgment | PASS | Ratified size, dispatch-only lease semantics and live-session refusal choices are respected; seeded DST work remains the settled #660 deferral, and no load-time capability probe was added (brief.md:641; crates/core/src/multipart/commit.rs:30, :157). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the core-only UploadPart API and evidence meet slice #927's intended use after the confirmed defect is corrected; production S3 wiring remains the already-agreed later slice (brief.md:300, brief.md:389; docs/design/architecture/05-building-block-view.md:248). |

Source citations are relative to `$PDCA_TARGET`; `brief.md`, `patch.diff`, logs and reviewer probes are evidence in this review directory. The target matches the patch (`git apply --reverse --check ../patch.diff` succeeds), and the stashed production changes were restored. No production source was edited by this review.

**F1 — Preserve uncertainty when a session record cannot be decoded.** The first session observation reaches `Doubt::session_seen` without validation; that function treats differing bytes as irreversible movement and clears `may_land` (`crates/core/src/multipart/commit.rs:496`). A subsequent held-slot read then removes the doubt entirely (`:890`). The final commit-session read (`:896`) and compensation-session read (`:1136`) likewise update doubt before decoding, incorrectly changing its live flag even when decoding fails. This violates the metadata-validation boundary and the requirement to retain an unknown outcome; it is the same underlying defect as the frozen batch-review finding, with two additional affected sites.

Three probes against unchanged production code establish the impact:

- First session read corrupted: returned `Stopped { cause: Record, slot_released: false }`; repairing the session and releasing the held commit returned `Committed` (reviewer-probe.log:82).
- Last commit-session read corrupted: returned `InDoubt { may_still_commit: false }`; the held commit still committed after repair (reviewer-probe.log:76).
- Compensation-session read corrupted: returned `InDoubt { may_still_commit: false }`; the held compensation still committed after repair (reviewer-probe.log:69).

The existing damaged-session cases inject definite conflicts, so they cannot detect losing outstanding doubt (`crates/core/tests/multipart_upload_part_commit.rs:3212, :3230, :3239`). Decode each session observation before using it as evidence of irreversible movement, preserve unresolved doubt on decode failure, and add these held-batch cases. The isolated probe crate copies the supplied harness and appends three tests; rerun from this directory with:

```sh
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline   --manifest-path pdca-reviewer-927-probe/Cargo.toml reviewer_ -- --nocapture
```

**Gate evidence distinguishes passing checks from unverified claims.** All seven frozen logs were readable; instance-scoped wrappers were adjudicated from their captured output, with the regression and full CI commands independently rerun.

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-ci | PASS in frozen evidence; independent target-copy caveat below | Full frozen CI reaches dependency audits, conformance and DST, then reports all checks passed (gate-logs/C4-ci.log:3888, :3900, :3914, :4540). |
| C4-verify | NEEDS-HUMAN | Both frozen and independent runs have green tests but a compile-only reverted failure; no executed pre-fix red (gate-logs/C4-verify.log:15; reviewer-red-green.log:6, :83). |
| C4-diff-cov | PASS in frozen evidence | 619/633 instrumentable changed lines executed, 97.8%; 773 unscored changed lines are explicitly excluded (gate-logs/C4-diff-cov.log:70, :98). |
| C5-mutants | PASS in frozen evidence | 78 tested: 63 caught, 15 unviable; no reported survivors. This does not cover the three newly demonstrated cases (gate-logs/C5-mutants.log:13). |
| T4-batch-review | FAIL, independently confirmed | F1 reproduces and also affects the two other session observations (gate-logs/T4-batch-review.log:10; reviewer-probe.log:96). |
| T4-contribution | N/A | PR/commit artifacts are absent by design; their substantive audit must rerun at publish (gate-logs/T4-contribution.log:10). |
| host-tikv | PASS in frozen evidence | Both real feature-enabled clippy builds finish; this establishes compilation, not live-cluster behavior (gate-logs/host-tikv.log:112, :211). |

**Prior art is checked by affected path.** Independent GitHub queries examined merged history at the brief's base and all 19 closed, unmerged PRs' changed files. None of those PRs touched the multipart source/test surfaces or architecture page; merged history shows the existing Create, Abort and staging work, and no commit-test file (reviewer-prior-art.json:10, :55, :89). The rejected internal attempts and their disposition are recorded in the brief (brief.md:423, :640); their ratified decisions were not reopened.

**The independent CI failure is a target-copy caveat, not a patch defect.** `cargo xtask ci` completed with exit 1 and two failures caused by the same absent `.github/advisory-db` gitlink: its index test and the advisory-pin prerequisite (reviewer-ci.log:3355, :4001). The harness copy's `git ls-files --stage .github/advisory-db` is empty, while the actual base records submodule SHA `40f5be6fb12ed93fc591bf4ab23b7d793b3f2239` (reviewer-base-pin.json:1); neither this pin nor its test is changed by the patch. Thus the local run cannot establish a full CI green or completed dependency audits; the frozen full-run log supplies that evidence. Locally, `custodian_gc` passed all 10 tests in 0.17 seconds and the 33-test DST custodian suite passed in 167.03 seconds (reviewer-ci.log:2596, :3921). The brief's external tools, `typos` and the docs renderer, actually ran successfully (reviewer-ci.log:2, :7); no shim or code-read substitutes for them.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 927 (part commit)

**Result: refuted on one defect family, damaged session bytes under a commit still in flight.** It is the same family as the gating T4-batch-review failure, but wider than that finding says. I found no false `Committed` and no compensation sent during a live commit unless the session bytes were damaged or the code was deliberately mutated.

All probes and edits ran in a scratch copy of `$PDCA_TARGET`, using the real `commit_part` over the test file's real harness. Baseline: 44 of 44 tests green.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:878`: confirms the T4 blocker. `session_seen` runs on the first session read before anything decodes it. Failing case: hold a commit (`may_still_commit: true`) and plant `b"torn"` at `mpu:` straight after it. The first read clears the commit's may-land flag, the slot read (`Held`) then drops it from the doubt, and the last read returns a record error. The answer is `Stopped(Record)`, classed `Terminal`, with the message "no batch of this part attempt is in doubt ... what it staged stays in the store". After repairing the session, the held batch answered `Committed` and the attempt's own part was durable. So the error's claim was false.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:496`: the root cause is wider than T4's line. `session_seen` treats any different bytes as "an epoch only rises", without decoding them, and it is also called at `:896` and `:1136`. Fixing only `:878` leaves these two failing cases:
  - Damage right after the commit's slot read (`:896`): `InDoubt { may_still_commit: false }`, then the held batch lands after repair.
  - A held compensation with damage straight after it (`:1136`): `InDoubt { batch: Compensation, may_still_commit: false }`, then the held batch lands after repair.

  The fix should count the session as moved only for a record that decodes and is either not `Open` or at a higher epoch. Undecodable bytes should settle nothing, which is the rule the sign-off set for `part:` damage. `crates/core/tests/multipart_upload_part_commit.rs:3642-3683` tests that rule for the part key only. The session-damage rows at `:3212` use `Act::conflict()` with no held batch, so they cannot catch this.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:878`: the same defect can give a wrong answer for a landed commit, not just a wrong error. Failing case:
  1. Empty prior. Hold the commit, land a non-empty rival, and damage the session.
  2. Repair the session right after the first session read.
  3. When the compensation is sent, an empty rival restores the prior's bytes, the held commit lands, and the compensation answers unknown and unapplied.

  The answer is `Ok(Refused(OperationAborted))`, with one compensation sent and the attempt's own part durable. This breaks the rule that no compensation is sent while the attempt's own commit may still land. Its trigger is narrow, since it needs damage that gets repaired mid-round. Replacing the `:878` call with `let _ = &first;` gives the in-doubt error with no compensation here, so that call alone causes this case. The full suite stays green with that edit, so no test pins the call. It does not fix the flag: the first failing case then still reports `may_still_commit: false`.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:490`: no test pins `may_still_commit: false` as "out of flight". With `may_land: true` hard-coded, all 44 tests stay green. Failing case the suite lacks: every commit round answers `Act::unknown(false, false)`. The real code spends 4 rounds, then compensates and returns `Stopped(CommitRoundsSpent)` with `slot_released: true`. The mutant returns `InDoubt { may_still_commit: true }` and leaves the slot held, which is the slot leak leg (G) exists to prevent. The probe passes on the real code and fails on the mutant, so it can be added as written.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:579` (with `:926`): nothing tests the arm that blocks a false `Committed` when a slot is rewritten under the attempt's own id while a commit is in doubt. The existing "rewritten" legs have nothing in doubt, and there `Gone` and `Rewritten` give the same answer. With that check forced to `Gone`, all 44 stay green. Failing case: hold a commit, then rewrite the slot under its own id. The real code answers `InDoubt(SlotLost)`. The mutant answers `Committed` with no part record in the store. A conforming attempt should not reach this history, but the arm is the only guard on the invariant if one does.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:870` (answered at `:954`): an absent part key is treated as a rival. Carry-forward fix 1 made undecodable bytes a `Stop::Record`, but `Ok(None)` still counts as a moved key. Failing case: a re-upload over a non-empty prior, with the prior's `part:` key deleted before a `Conflict` and the session still `Open@E`. The answer is `Refused(OperationAborted)`, plus a compensation, with no rival anywhere. The Design says a `part:` record is removed only after the session leaves `Open`, so this history is damage. The rubric's *Absent or unsupported entries* class wants an explicit error here, and the attempt should not report a same-number loss. The prior part's chunks now have no record naming them, and nothing reports that.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:598` and `:1187`: two arms the suite never runs. Replacing either with `panic!` leaves all 44 green, and diff coverage lists both as MISS.
  - `:598` is the "session gone" row in the module doc's table, and it has no test.
  - `:1187` looks unreachable. The previous lap's session read already cleared every may-land flag, so the slot read on this lap empties the doubt first. Either delete it or show a history that reaches it.

## The verdict's claims

- `check-gates.json` C5-mutants ("63 caught, 15 unviable"): accurate, but weaker than it reads. It shows no missed mutants among the 78 that `cargo mutants` generated. Of my 26 hand edits, 21 were killed and 5 survived: the `:878` call, `:490`, `:579`, `:598` and `:1187`. It should not be read as proof that the classification logic is fully pinned.

- `check-gates.json` C4-verify (`unverifiable`): this is not a refutation. The test cannot compile without the new module, and the brief declared that in advance. The brief's demonstrated reds are recorded in `build-notes.md`, which I was not given, so I did not check them from there.

- `check-gates.json` C4-diff-cov (97.8%): the 14 missed lines are mostly store-fault and decode arms. `:598` and `:1187` are the two that matter, covered above.

## Attacked, and could not refute

- **Carry-forward fix 2:** each of the five named edits is killed by the test written for it: `can_recur` forced false, the ceiling's exclusion of the part's own prior, `require_absent` on the obligation key, `.last()` in the doubt search, and the epoch chase keeping only the latest obligation.
- **Tests:** they drive the production path, and the wrapper's slot-byte and margin checks fail the test from inside `commit`, so they are real checks. Other hand edits were killed too: dropping the `mpu:` requirement from either batch, using the first session read for the own-commit proof, removing the read-only laps, and stamping the wrong epoch or time.
- **Carry-forward fix 4:** the C4-ci log shows a complete run in 357 s. `custodian_gc` finished, and machete, deny, the conformance tests and the DST (deterministic simulation) tests all ran.
- **Classification by reading the code:** I found no input that breaks these outside session damage:
  - the slot-then-session read order;
  - the rule that a commit landed before a rollback stays in doubt;
  - bytes returning only for an empty prior;
  - the loop bounds;
  - the renewal hand-off (`stage.rs:1642-1772`).
- **Not raised, because already settled:** seeded DST coverage (`// deferred: #660`), slots left held after a Complete rollback, and the dispatch-only lease promise.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:878`: Validate session reads before changing commit uncertainty. After a held commit returns `CommitUnknownResult { may_still_commit: true }`, a malformed `mpu:` value makes `session_seen` mark it unable to land; the following held-slot read at `:890` then discards that doubt. The final session decode returns `Stopped(Record)` claiming no batch is in doubt, although restoring the original session bytes lets the held commit land. The same validation ordering at `:896` and `:1136` can incorrectly clear `may_still_commit` before returning a record error. Decode changed session values before calling `session_seen`, preserving uncertainty on malformed records. Add held-commit and held-compensation regressions that corrupt the session, assert the uncertainty flag survives, restore its bytes, and release the held batch. The damaged-session tests at `crates/core/tests/multipart_upload_part_commit.rs:3227` inject only definite conflicts and miss this case. This independently confirms the finding in the frozen batch-review log.

No additional reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept or reject the predeclared lack of an executable baseline red—44/44 tests pass after restoration, but the reverted API cannot compile; frozen full CI passed and independent CI has the target-copy caveat below (brief.md:328; reviewer-red-green.log:6; reviewer-red-green.log:83; gate-logs/C4-ci.log:4540).
- [ ] C5 Causal adequacy — Validate session observations before changing outstanding doubt—three independent probes show a definite stop or false may_still_commit despite a later successful landing (crates/core/src/multipart/commit.rs:878, :896, :1136; reviewer-probe.log:69).
- [ ] Validation — fitness-to-purpose — Decide whether the core-only UploadPart API and evidence meet slice #927's intended use after the confirmed defect is corrected; production S3 wiring remains the already-agreed later slice (brief.md:300, brief.md:389; docs/design/architecture/05-building-block-view.md:248).
- [x] C4-verify — Both frozen and independent runs have green tests but a compile-only reverted failure; no executed pre-fix red (gate-logs/C4-verify.log:15; reviewer-red-green.log:6, :83).
- [ ] `crates/core/src/multipart/commit.rs:878`: confirms the T4 blocker. `session_seen` runs on the first session read before anything decodes it. Failing case: hold a commit (`may_still_commit: true`) and plant `b"torn"` at `mpu:` straight after it. The first read clears the commit's may-land flag, the slot read (`Held`) then drops it from the doubt, and the last read returns a record error. The answer is `Stopped(Record)`, classed `Terminal`, with the message "no batch of this part attempt is in doubt ... what it staged stays in the store". After repairing the session, the held batch answered `Committed` and the attempt's own part was durable. So the error's claim was false.
- [ ] `crates/core/src/multipart/commit.rs:496`: the root cause is wider than T4's line. `session_seen` treats any different bytes as "an epoch only rises", without decoding them, and it is also called at `:896` and `:1136`. Fixing only `:878` leaves these two failing cases:
- [ ] `crates/core/src/multipart/commit.rs:878`: the same defect can give a wrong answer for a landed commit, not just a wrong error. Failing case:
- [ ] `crates/core/src/multipart/commit.rs:490`: no test pins `may_still_commit: false` as "out of flight". With `may_land: true` hard-coded, all 44 tests stay green. Failing case the suite lacks: every commit round answers `Act::unknown(false, false)`. The real code spends 4 rounds, then compensates and returns `Stopped(CommitRoundsSpent)` with `slot_released: true`. The mutant returns `InDoubt { may_still_commit: true }` and leaves the slot held, which is the slot leak leg (G) exists to prevent. The probe passes on the real code and fails on the mutant, so it can be added as written.
- [ ] `crates/core/src/multipart/commit.rs:579` (with `:926`): nothing tests the arm that blocks a false `Committed` when a slot is rewritten under the attempt's own id while a commit is in doubt. The existing "rewritten" legs have nothing in doubt, and there `Gone` and `Rewritten` give the same answer. With that check forced to `Gone`, all 44 stay green. Failing case: hold a commit, then rewrite the slot under its own id. The real code answers `InDoubt(SlotLost)`. The mutant answers `Committed` with no part record in the store. A conforming attempt should not reach this history, but the arm is the only guard on the invariant if one does.
- [ ] `crates/core/src/multipart/commit.rs:870` (answered at `:954`): an absent part key is treated as a rival. Carry-forward fix 1 made undecodable bytes a `Stop::Record`, but `Ok(None)` still counts as a moved key. Failing case: a re-upload over a non-empty prior, with the prior's `part:` key deleted before a `Conflict` and the session still `Open@E`. The answer is `Refused(OperationAborted)`, plus a compensation, with no rival anywhere. The Design says a `part:` record is removed only after the session leaves `Open`, so this history is damage. The rubric's *Absent or unsupported entries* class wants an explicit error here, and the attempt should not report a same-number loss. The prior part's chunks now have no record naming them, and nothing reports that.
- [ ] `crates/core/src/multipart/commit.rs:598` and `:1187`: two arms the suite never runs. Replacing either with `panic!` leaves all 44 green, and diff coverage lists both as MISS.
- [ ] `crates/core/src/multipart/commit.rs:878`: Validate session reads before changing commit uncertainty. After a held commit returns `CommitUnknownResult { may_still_commit: true }`, a malformed `mpu:` value makes `session_seen` mark it unable to land; the following held-slot read at `:890` then discards that doubt. The final session decode returns `Stopped(Record)` claiming no batch is in doubt, although restoring the original session bytes lets the held commit land. The same validation ordering at `:896` and `:1136` can incorrectly clear `may_still_commit` before returning a record error. Decode changed session values before calling `session_seen`, preserving uncertainty on malformed records. Add held-commit and held-compensation regressions that corrupt the session, assert the uncertainty flag survives, restore its bytes, and release the held batch. The damaged-session tests at `crates/core/tests/multipart_upload_part_commit.rs:3227` inject only definite conflicts and miss this case. This independently confirms the finding in the frozen batch-review log.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- [x] **The lease fix assumes a deadline the metadata seam does not guarantee.** `brief.md:174-188` calls `E_TX_MILLIS` the commit's transaction deadline and tests only the lease remaining when a batch is sent; `brief.md:403-412` uses that to promise coverage throughout the commit. On the target, `crates/core/src/multipart.rs:5113-5118` defines a five-second FDB envelope, but `crates/traits/src/lib.rs:1332-1345` guarantees termination, not completion within five seconds, and explicitly permits application after a timeout. FDB's five seconds start after obtaining a read version; its configured default timeout is ten seconds (`crates/metadata-fdb/src/lib.rs:369-388`). TiKV's default is fifty seconds (`crates/metadata-tikv/src/lib.rs:171-181`, applied at `:1359-1365`). A batch sent with exactly five seconds of lease left can therefore pass every proposed dispatch assertion and still apply after expiry. Revise the brief to identify the actual enforceable timing contract, and require a delayed-after-dispatch witness. If this slice guarantees only sufficient lease at dispatch, narrow the promised invariant explicitly; a caller timeout alone cannot guarantee when an outstanding batch applies.
- [x] **The definition of “settled not landed” lacks the read ordering needed to make it true.** `brief.md:458-466` accepts an irreversibly changed session/part precondition “with the own slot still at its last-written bytes”; `brief.md:494-495` permits reading the session after the slot and part. Counterexample: an unknown commit is held; the slot read sees this attempt still holding it; the commit then lands and a same-number rival replaces its part; the part read sees the rival and the session read still sees `Open@E`. Those observations satisfy the stated settlement conditions even though this attempt committed. C3 (`brief.md:117-126`) puts the rival before the re-read, so it does not force this gap. H's rollback history (`:208-210`) exposes the analogous session race but conflicts with the Design's unqualified settlement rule. The target staging code explicitly guards this class by reading the slot again after observing a session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise the proof to require an own-slot observation **after** the irreversible precondition change, or retain doubt, and add the held-commit → rival-between-reads history with an assertion that it cannot be classified as a compensated loser. Otherwise Do can follow the Design and compensate a commit that landed.
- [x] size backstop — this slice is behaving oversized: patch is 244 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design. Only the items below change. 1. Bug: decode each session read before treating it as evidence the session moved (`commit.rs:496`, called at `:878`, `:896`, `:1136`). Only a record that decodes and is not `Open`, or is at a higher epoch, counts as moved. Undecodable bytes settle nothing and keep the doubt and `may_still_commit` as they were. Add held-commit and held-compensation tests that damage the session, check the doubt survives, repair the session and land the batch. Include the damage-repaired-mid-round history, which must not answer `Refused(OperationAborted)` or send a compensation. 2. A missing `part:` record for a non-empty prior with the session still `Open@E` (`commit.rs:870`, `:954`) is damage: return an explicit record error, not `OperationAborted`, and test it. 3. Add tests: every commit round answering unknown with `may_still_commit: false` must compensate and return `Stopped(CommitRoundsSpent)` with `slot_released: true` (`:490`); a slot rewritten under the attempt's own id during a held commit must answer `InDoubt(SlotLost)` (`:579`); the "session gone" row (`:598`). 4. `:1187`: delete it, or add a test that reaches it.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
