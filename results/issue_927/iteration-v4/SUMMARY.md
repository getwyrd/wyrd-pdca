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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.3% — 637 of 648 instrumentable changed lines executed (floor 80%); 648 of 1473 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 83 mutants tested in 6m: 68 caught, 15 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_927/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.43s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No new patch defect found in #927’s core UploadPart commit, outcome classification, compensation and re-upload slice; the verification exception and fitness judgment remain for sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract covers ownership, uncertain outcomes, lease dispatch and live-session capacity, including the latest damage regressions; the sign-off decisions are explicit (`brief.md:32`, `brief.md:656`, `brief.md:668`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no executable pre-fix commit API; independently reverting production changes produces missing-symbol errors, not a behavioral red (`brief.md:321`, `reviewer-red.log:2`). |
| C3 Change | PASS | Durable chunk ownership and single slot release are preserved across commit, replacement and compensation by their atomic batches and original slot preconditions (`crates/core/src/multipart/commit.rs:665`, `crates/core/src/multipart/commit.rs:701`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API verification exception and reliance on frozen full-CI evidence for checkout limitations — 52 tests pass independently, but no pre-fix test executes and local full CI is not independently green (`brief.md:328`, `reviewer-green.log:59`, `reviewer-ci-serial.log:2923`, `gate-logs/C4-ci.log:4548`). |
| C5 Causal adequacy | PASS | Forced read-gap and damage/repair histories distinguish this attempt’s commit from rival or teardown effects; independently tested mutations yield 68 caught, 15 unviable and no survivors (`crates/core/tests/multipart_upload_part_commit.rs:2702`, `crates/core/tests/multipart_upload_part_commit.rs:4033`, `crates/core/tests/multipart_upload_part_commit.rs:4089`, `reviewer-mutants-cap-lints.log:4`). |
| T1 Structure | PASS | The new public operations stay behind the existing store and clock seams, and their production reach is accurately documented; no dependency-direction or persisted-format expansion was found (`crates/core/src/multipart/commit.rs:750`, `crates/core/src/multipart/commit.rs:1243`, `docs/design/architecture/05-building-block-view.md:248`). |
| T2 Shape | PASS | The five-file change stays within the expressly retained single-bundle scope; staging changes only its hand-off documentation, and the added volume serves the requested protocol and regression coverage (`patch.diff:1566`, `brief.md:304`, `brief.md:665`). |
| T3 Runtime | PASS | Retry budgets, lease boundaries, late landing and settled-stop capacity are exercised, with one caller-owned clock and no compensation while a commit remains unsettled (`crates/core/src/multipart/commit.rs:847`, `crates/core/tests/multipart_upload_part_commit.rs:2410`, `crates/core/tests/multipart_upload_part_commit.rs:2787`, `crates/core/tests/multipart_upload_part_commit.rs:3013`). |
| T4 Contribution | PASS | Affected-path history and all 19 closed unmerged PRs were independently checked: prerequisites exist and no rejected PR touches these paths; the contribution-artifact gate is N/A until its required publish rerun (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:7`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The patch respects the ratified dispatch-only lease promise, live-session refusal and residue decisions; the tracked DST deferral is settled, and no capability-probe symptom guard was introduced (`brief.md:656`, `crates/core/src/multipart/commit.rs:169`, `crates/core/src/multipart/commit.rs:618`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Judge whether the exercised core protocol is fit to land as this agreed slice — forced store histories pass, while a production S3 UploadPart request is intentionally outside this patch’s reach (`brief.md:389`, `docs/design/architecture/05-building-block-view.md:248`). |

The independent behavioral run used `cargo test -p wyrd-core --test multipart_upload_part_commit`. Stashing the tracked production changes while keeping the new test reproduced the missing-API compile failure; restoring the stash returned all 52 tests to green. The target still matches `patch.diff` under `git apply --reverse --check`; source citations above are relative to `$PDCA_TARGET`, and evidence-log citations are relative to this bundle.

The full-CI rerun has checkout and execution caveats, not a demonstrated patch defect. The default parallel run stalled in unchanged `custodian_day_one` tests (`reviewer-ci.log:2485`); the same binary passed all 15 tests serially (`reviewer-custodian-serial.log:19`). The serial workspace run reached the advisory-db test, where the disposable checkout’s Git index lacks `.github/advisory-db`; both `git ls-files -s` and `git ls-tree HEAD` return no entry. That metadata is untouched by the patch, and the failure is explicitly identified at `xtask/tests/advisory_db.rs:66` and `reviewer-ci-serial.log:2923`. The serial rerun completed with exit 1: its only two failed checks were the advisory-db index test (within workspace tests) and advisory-pin preparation; the remaining simulation suite completed. This is not an independent full-CI pass. The frozen full-CI log contains the dependency audits, conformance and completed simulation suite, ending in success (`gate-logs/C4-ci.log:3895`, `gate-logs/C4-ci.log:4548`).

Independent formatting, Clippy with warnings denied, spelling and documentation checks passed. Both declared external dependencies were actually exercised: `typos` ran and the renderer produced 101 pages with a successful link audit (`reviewer-ci-serial.log:2`, `reviewer-ci-serial.log:7`). The mutation rerun used `cargo mutants --in-diff ../patch.diff -p wyrd-core --jobs 4 --copy-target false --cap-lints true`; suppressing lint failures reproduces the frozen result of 68 caught and 15 genuinely unviable mutations (`reviewer-mutants-cap-lints.log:4`).

The remaining frozen evidence was read at its recorded scope: coverage shows 637 of 648 instrumentable changed lines executed, with 825 additional changed lines unscored (`gate-logs/C4-diff-cov.log:78`, `gate-logs/C4-diff-cov.log:102`); the batched review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`); and both TiKV feature compilation commands completed (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). T4-contribution is **N/A**, because the PR description is drafted after Check and the substantive audit must run at publish (`gate-logs/T4-contribution.log:10`). No new implementation iteration is requested.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #927 part commit

**Result: I could not break the production code.** No history I built or traced gave a false `Committed`, a compensation (the batch that retires a lost attempt's chunks and frees its slot) while a commit could still land, or a double slot release. The findings are four test gaps, one spec rule nobody appears to own, and two gate claims that are weaker than they look.

**What I ran.** I re-ran the new test file on a scratch copy of the patched tree: 52 passed. I then made 35 hand-written mutants (small deliberate breaks in `commit.rs`, of kinds `cargo mutants` does not generate): 27 were killed and 8 survived. I also added three probe tests of my own. All three pass on the real code, so the gaps below are in the tests, not in production behaviour.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:824` (with `:904`): no test covers a held re-upload over a **non-empty** prior that then loses to a rival. Changing `can_recur` to `prior.is_some()` survives all 52 tests, although the changed code never compensates here and returns an in-doubt error. Failing case: commit part 1 with chunks, hold a re-upload's commit, land a rival. The expected answer is `Refused(OperationAborted)` with one compensation, and the held batch then answers `Conflict`. The real code does this (my probe passes and kills the mutant). The brief's rule at `brief.md:537-538` has no test on this side; the iteration 2 test only pins the empty-prior side. The read-gap leg (`crates/core/tests/multipart_upload_part_commit.rs:2702`) uses a part with no prior, so it misses this too.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1151`, `:1162`, `:1170`, `:1177`: the store-fault paths inside `compensate` never run (the same lines are the `MISS` entries in `gate-logs/C4-diff-cov.log`). Replacing `end(doubt, …)` with a plain stop on each of them, one at a time, survives all 52 tests. That change would report "no batch of this part attempt is in doubt" while a held compensation can still land, which is the rubric's *Transactions* class. Failing case: hold the compensation, then fault the next obligation, slot or session read (reads 5, 6 and 7 after the first commit). The expected answer is `InDoubt { batch: Compensation, may_still_commit: true }`. My probe passes on the real code for all three reads; I confirmed it kills the slot-read mutant only, and it does not cover the definite commit fault at `:1151`, which needs a hold followed by a failed round.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:812-814`: no test runs a re-upload above epoch 0. Hard-coding the re-upload obligation's epoch to 0 survives all 52 tests, so the obligation key could be minted under the wrong epoch unnoticed. Failing case: commit part 1, stage a re-upload, roll the session back before the quiesce renewal (the attempt's last lease renewal before its commit), and commit. The obligation must be named under epoch 2. The real code does this (my probe passes and kills the mutant). The extra leg belongs beside `crates/core/tests/multipart_upload_part_commit.rs:2508`.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:970`: no test pins what a damaged `part:` record does once the session has moved. A mutant that stops on damage only under the same session survives, and `crates/core/tests/multipart_upload_part_commit.rs:3632` does not separate the two. Case: a commit answers `Conflict`, the part key holds `b"not a part record"`, and the session is at `Open@E'`. From reading the code, today's answer is `Stop::Record` with no compensation, which matches the iteration 2 fix. I did not run this history. Add a leg so that answer cannot change unnoticed.
- NEEDS-HUMAN [human] — `crates/core/src/multipart/commit.rs:807`: the commit stamps `committed_at_millis` without comparing its clock to the session's `clock_source`. `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1974` says every operation writing a session-scoped timestamp MUST make that check and refuse with `503`, and it names the `psum:` `committed_at` stamp (see also X106 at `:2645`). Case: a session created under `clock_source: "wall"`, committed through a gateway on another clock, is stamped and answered `Committed`; nothing refuses it. The fix cannot stay in this slice: `StageClock` (`crates/core/src/multipart/stage.rs:340`) has no source name to compare, the merged slot stamp has the same gap, and the brief limits this slice's `stage.rs` edit to one doc fix. A keyword search of the bodies of #927, #669, #625, #660, #1015, #633, #1019 and #508 (first five matches per issue) showed only the reaper's guard and alarm, not this writer check, so a tracking issue may exist that I did not find. A human should decide which slice owns it, and it needs a tracking issue before #669 wires the verb.
- NEEDS-HUMAN [human] — `check-gates.json:108` (`gate-logs/T4-batch-review.log`): T4 reports three review passes over a 286 KB patch in 25.54 s, with zero findings in every bucket, including zero noise. The log is one summary line, so it cannot show that the passes reviewed this patch rather than returning a cached or empty result. The previous iteration's T4 found a blocker, so the clean result matters. I could not check this: `review-batch.md` is outside my inputs. Before relying on this gate, confirm that file shows three real passes over this tree.
- `check-gates.json:70`: "83 mutants: 68 caught, 15 unviable" is true but narrower than it reads. Each survivor in the findings above is a change `cargo mutants` does not make, such as swapping an argument or a returned error. The C5 pass is not evidence that these paths are tested.
- `check-gates.json:45`: C4-verify is `unverifiable` because the test file cannot compile against the reverted base (`gate-logs/C4-verify.log`). The brief declared this in advance, so it is not a refutation. It does mean no gate ran a red. The required demonstrated reds are in `build-notes.md`, which is withheld from me, so I could not audit them. My mutants are only a partial substitute. They include the read-order red (slot check moved after the last session read), which the read-gap and held-rollback legs do catch.
- `crates/core/src/multipart/commit.rs:1125`, `:1135`: these two `Stop::Record` paths in `compensate` never run. As far as I can tell no legal history reaches them, because the session bytes were already decoded and the chunk schemes were checked earlier. This is low priority, but iteration 3 asked for this kind of path to be deleted or tested, so it should be decided rather than left.

## Attempts that failed to refute

- **Own-commit proof:** third-attempt slot reuse, a rollback or teardown between each read, a repaired session, and an empty prior whose bytes come back. All held by reading, and the existing legs killed my mutants of the read order, the session judgement and the damage handling.
- **Settling:** every path that clears a doubt depends on a slot read made after the read that found the precondition gone. I found no path to a compensation while a commit can still land.
- **Compensation:** a held round beside the epoch chase, an empty part with no obligation to re-read, a reused slot index, and an obligation key shared with the re-upload's. All are safe because the slot compare-and-set (write only if the bytes still match) lets only one batch land.
- **Loops:** each loop is bounded, and the read-only lap cannot repeat.
- **Batch builders:** removing the session requirement or the `sidx:` deletes, or using the wrong refusal or epoch in the compensation, was killed every time.
- **Size limits:** the part commit's batch shape matches the `0016:660` row, and `MAX_PART_CHUNKS` is tied to the value capacity. The `scan` of `psum:` stays far under `SCAN_CAP` (10,000 records against 1,048,576).
- **Full CI:** iteration 2 asked for a complete run, and this one is complete. `custodian_gc` ran 10 tests in 0.17 s (`gate-logs/C4-ci.log:2696-2710`), machete and deny both ran, and the log ends with `xtask ci: all checks passed`. No stall recurred, so its cause was not investigated in this log.

One of the 8 surviving mutants is not reported above: removing the first `session_seen` call. I judged it equivalent by reading, because it only costs one more lap of reads, and I did not test that separately.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No actionable findings on either lens: introduced correctness bugs, or reuse/simplification/efficiency. Reviewed the commit classification and lease checks (`crates/core/src/multipart/commit.rs:750`), compensation (`crates/core/src/multipart/commit.rs:1103`), record construction (`crates/core/src/multipart.rs:3055`, `crates/core/src/multipart.rs:4128`), and regression harness, including read-gap and damage-repair histories (`crates/core/tests/multipart_upload_part_commit.rs:2702`, `crates/core/tests/multipart_upload_part_commit.rs:4089`). Citations were checked against `$PDCA_TARGET`.

Validation relied on frozen gate evidence, without rebuilding or modifying the target: CI passed, all 52 integration tests passed, diff coverage was 98.3%, and mutation testing reported 68 caught and 15 unviable mutants. The pre-fix run was unverifiable because the new API does not exist on the base, as anticipated in the brief. Recorded scope decisions and deferrals were respected.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared new-API verification exception and reliance on frozen full-CI evidence for checkout limitations — 52 tests pass independently, but no pre-fix test executes and local full CI is not independently green (`brief.md:328`, `reviewer-green.log:59`, `reviewer-ci-serial.log:2923`, `gate-logs/C4-ci.log:4548`).
- [ ] Validation — fitness-to-purpose — Judge whether the exercised core protocol is fit to land as this agreed slice — forced store histories pass, while a production S3 UploadPart request is intentionally outside this patch’s reach (`brief.md:389`, `docs/design/architecture/05-building-block-view.md:248`).
- [ ] `crates/core/src/multipart/commit.rs:824` (with `:904`): no test covers a held re-upload over a **non-empty** prior that then loses to a rival. Changing `can_recur` to `prior.is_some()` survives all 52 tests, although the changed code never compensates here and returns an in-doubt error. Failing case: commit part 1 with chunks, hold a re-upload's commit, land a rival. The expected answer is `Refused(OperationAborted)` with one compensation, and the held batch then answers `Conflict`. The real code does this (my probe passes and kills the mutant). The brief's rule at `brief.md:537-538` has no test on this side; the iteration 2 test only pins the empty-prior side. The read-gap leg (`crates/core/tests/multipart_upload_part_commit.rs:2702`) uses a part with no prior, so it misses this too.
- [ ] `crates/core/src/multipart/commit.rs:1151`, `:1162`, `:1170`, `:1177`: the store-fault paths inside `compensate` never run (the same lines are the `MISS` entries in `gate-logs/C4-diff-cov.log`). Replacing `end(doubt, …)` with a plain stop on each of them, one at a time, survives all 52 tests. That change would report "no batch of this part attempt is in doubt" while a held compensation can still land, which is the rubric's *Transactions* class. Failing case: hold the compensation, then fault the next obligation, slot or session read (reads 5, 6 and 7 after the first commit). The expected answer is `InDoubt { batch: Compensation, may_still_commit: true }`. My probe passes on the real code for all three reads; I confirmed it kills the slot-read mutant only, and it does not cover the definite commit fault at `:1151`, which needs a hold followed by a failed round.
- [ ] `crates/core/src/multipart/commit.rs:812-814`: no test runs a re-upload above epoch 0. Hard-coding the re-upload obligation's epoch to 0 survives all 52 tests, so the obligation key could be minted under the wrong epoch unnoticed. Failing case: commit part 1, stage a re-upload, roll the session back before the quiesce renewal (the attempt's last lease renewal before its commit), and commit. The obligation must be named under epoch 2. The real code does this (my probe passes and kills the mutant). The extra leg belongs beside `crates/core/tests/multipart_upload_part_commit.rs:2508`.
- [ ] `crates/core/src/multipart/commit.rs:970`: no test pins what a damaged `part:` record does once the session has moved. A mutant that stops on damage only under the same session survives, and `crates/core/tests/multipart_upload_part_commit.rs:3632` does not separate the two. Case: a commit answers `Conflict`, the part key holds `b"not a part record"`, and the session is at `Open@E'`. From reading the code, today's answer is `Stop::Record` with no compensation, which matches the iteration 2 fix. I did not run this history. Add a leg so that answer cannot change unnoticed.
- [ ] `crates/core/src/multipart/commit.rs:807`: the commit stamps `committed_at_millis` without comparing its clock to the session's `clock_source`. `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1974` says every operation writing a session-scoped timestamp MUST make that check and refuse with `503`, and it names the `psum:` `committed_at` stamp (see also X106 at `:2645`). Case: a session created under `clock_source: "wall"`, committed through a gateway on another clock, is stamped and answered `Committed`; nothing refuses it. The fix cannot stay in this slice: `StageClock` (`crates/core/src/multipart/stage.rs:340`) has no source name to compare, the merged slot stamp has the same gap, and the brief limits this slice's `stage.rs` edit to one doc fix. A keyword search of the bodies of #927, #669, #625, #660, #1015, #633, #1019 and #508 (first five matches per issue) showed only the reaper's guard and alarm, not this writer check, so a tracking issue may exist that I did not find. A human should decide which slice owns it, and it needs a tracking issue before #669 wires the verb.
- [ ] `check-gates.json:108` (`gate-logs/T4-batch-review.log`): T4 reports three review passes over a 286 KB patch in 25.54 s, with zero findings in every bucket, including zero noise. The log is one summary line, so it cannot show that the passes reviewed this patch rather than returning a cached or empty result. The previous iteration's T4 found a blocker, so the clean result matters. I could not check this: `review-batch.md` is outside my inputs. Before relying on this gate, confirm that file shows three real passes over this tree.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The lease fix assumes a deadline the metadata seam does not guarantee.** `brief.md:174-188` calls `E_TX_MILLIS` the commit's transaction deadline and tests only the lease remaining when a batch is sent; `brief.md:403-412` uses that to promise coverage throughout the commit. On the target, `crates/core/src/multipart.rs:5113-5118` defines a five-second FDB envelope, but `crates/traits/src/lib.rs:1332-1345` guarantees termination, not completion within five seconds, and explicitly permits application after a timeout. FDB's five seconds start after obtaining a read version; its configured default timeout is ten seconds (`crates/metadata-fdb/src/lib.rs:369-388`). TiKV's default is fifty seconds (`crates/metadata-tikv/src/lib.rs:171-181`, applied at `:1359-1365`). A batch sent with exactly five seconds of lease left can therefore pass every proposed dispatch assertion and still apply after expiry. Revise the brief to identify the actual enforceable timing contract, and require a delayed-after-dispatch witness. If this slice guarantees only sufficient lease at dispatch, narrow the promised invariant explicitly; a caller timeout alone cannot guarantee when an outstanding batch applies.
- [ ] **The definition of “settled not landed” lacks the read ordering needed to make it true.** `brief.md:458-466` accepts an irreversibly changed session/part precondition “with the own slot still at its last-written bytes”; `brief.md:494-495` permits reading the session after the slot and part. Counterexample: an unknown commit is held; the slot read sees this attempt still holding it; the commit then lands and a same-number rival replaces its part; the part read sees the rival and the session read still sees `Open@E`. Those observations satisfy the stated settlement conditions even though this attempt committed. C3 (`brief.md:117-126`) puts the rival before the re-read, so it does not force this gap. H's rollback history (`:208-210`) exposes the analogous session race but conflicts with the Design's unqualified settlement rule. The target staging code explicitly guards this class by reading the slot again after observing a session move (`crates/core/src/multipart/stage.rs:1713-1732`). Revise the proof to require an own-slot observation **after** the irreversible precondition change, or retain doubt, and add the held-commit → rival-between-reads history with an assertion that it cannot be classified as a compensated loser. Otherwise Do can follow the Design and compensate a commit that landed.
- [ ] size backstop — this slice is behaving oversized: patch is 279 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human decision at sign-off: rebuild this slice, do not split it. Keep the classification design. One design rule is corrected, and this rationale overrides the brief where they differ. Start from this attempt's patch. Why rejected: three high-effort review passes on this patch each independently flagged `crates/core/src/multipart/commit.rs:904` (`Ok(Some(_)) => doubt.cannot_land()`). The brief's rule that a changed non-empty prior part record "never comes back" is false, because custodian staged reconstruction rewrites `part:` records while the session is `Open@E` (`crates/custodian/src/reconstruction/staged.rs`, `0016:671`) and may be able to move a part back to its earlier placement. History: a re-upload's commit is held; repair moves the prior; the attempt decides the commit cannot land and compensates; a second repair restores the prior's bytes; the held commit lands. A compensation with an unknown outcome then sees the slot gone with the session unchanged and answers `Refused(OperationAborted)` for a part that committed (`compensate`), and one answered `Conflict` ends in a definite `SlotLost`. This was confirmed by reading the code, not by running it. Check `repointed_part` for whether the exact bytes can return, but make the fix regardless: the rule rests on an assumption reconstruction does not honour. Changes: 1. Fix: a changed prior part record, empty or non-empty, no longer proves a held commit cannot land. Treat a non-empty prior as `can_recur` treats an empty one, and keep the doubt. Only `may_still_commit: false` or a session that has moved can settle it; a key that was required absent and is now present still settles it. More in-doubt answers are an allowed availability cost. Correct the code comment and module docs that state the old rule. 2. New test: a held commit while repair moves the prior away and back, using real re-place batches or their exact shape. Required: no compensation sent while the commit may land, never a false `Refused(OperationAborted)`, and the slot released exactly once. 3. New test, with the corrected answer: a held re-upload over a non-empty prior that loses to a rival answers the in-doubt error with no compensation, not `Refused(OperationAborted)`. It must fail if `can_recur` goes back to treating a non-empty prior as settled. Update any existing leg that expects the old answer. 4. Add the test gaps the adversary found, each failing under the named edit: - Store faults inside `compensate` (`commit.rs:1151`, `:1162`, `:1170`, `:1177`): replacing `end(doubt, ...)` with a plain stop on each. Hold the compensation, fault the next obligation, slot or session read, and expect `InDoubt { batch: Compensation, may_still_commit: true }`. `:1151` needs a hold followed by a failed round. - A re-upload above epoch 0 (`commit.rs:812-814`): the obligation's epoch hard-coded to 0. Commit part 1, stage a re-upload, roll the session back before the quiesce renewal, commit, and expect the obligation under the live epoch. - A damaged `part:` record after the session has moved (`commit.rs:970`): a commit answered `Conflict`, `b"not a part record"` at the part key, the session at `Open@E'`. Expect `Stop::Record` with no compensation. - `commit.rs:1125`, `:1135`: delete these two `Stop::Record` paths if no legal history reaches them, or add a test that reaches them. Decided at sign-off: - With nothing in doubt, a part record changed by a plain repair keeps the existing classification: compensate and answer the retryable `Refused(OperationAborted)`. Do not add a retry against the new record. - The size backstop stays overridden: do not split. Do not trim tests to reduce size. - Keep the earlier ratified decisions: the dispatch-only lease promise, `Refused(OperationAborted)` for a compensated live-session loss, and slots left held after a rollback. Review caveat: the T4 gate's default-effort passes missed this bug 3 of 3 and missed the known v3 bug 3 of 3 in a re-test, while high-effort passes caught this bug 3 of 3 and the v3 bug 2 of 3. A clean T4 result at default settings is weak evidence, so do not rely on it.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- issue_927: clock-source check is missing on part commit and slot stamps (`0016:1974`); needs a tracking issue and an owner before #669 wires the verb.
- issue_927: T4 review runs Codex with unpinned model and effort; default passes missed known bugs that high-effort passes caught. Pin both (harness upstream) and re-check recently passed bundles.
- issue_927: `custodian_gc` deadlock on `main` has no issue filed; also check whether the `custodian_day_one` stall is the same bug.
- issue_927: open question: who enforces `MAX_PARTS_PER_SESSION`? `commit.rs` and `stage.rs` do not, and empty parts use no chunk budget.
- issue_927: raise against 0016: the lease deadline wording (`:1727-1728`), slots held after a rollback, and whether it assumes a changed non-empty part record cannot return, given repair rewrites them.
