# core: UploadPart commit — part record, outcome classification, compensation, re-upload, ceilings (657.2)

> Second of the two slices #657 was split into on 2026-10-20. It builds on 657.1 (staging),
> which hands it a staged attempt: a held slot and one owned `sidx:` entry per chunk. Do reads
> ONLY this file. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read these before writing
> code: the `part:` and `psum:` rows of the record table (`0016:351-352`); the **Part commit** and
> **Live-session compensation** rows of the batch inventory (`0016:660`, `:673`); and decision 5's
> commit classification (`0016:1584-1653`) and quiesce rule (`0016:1720-1733`). Citations were
> verified against `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-20, and the passages revised
> on 2026-10-22 (B's last two legs, *Settle before compensating*, the check order, the slot-bytes
> evidence) at `4bac538`; 657.1's own code is cited by what it does, since it lands in the wave
> before this one.

- **Slug:** multipart-upload-part-commit
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.1 a part's chunks can be staged, but nothing turns them into a part. This
  slice adds the second half of `UploadPart` as `wyrd-core` functions. It (1) commits the
  `part:` + `psum:` pair, the slot release and the `sidx:` deletes in one batch, after a last
  renewal. It (2) classifies a `Conflict` or a `CommitUnknownResult` from a re-read, never
  blind. It (3) runs the live-session compensation, including against a session rolled back to
  a newer `Open@E'`. It (4) handles a re-upload of a committed part number, retiring the prior
  part's chunks in the same batch. It (5) refuses a part past the per-part or cumulative chunk
  ceiling and compensates what it staged. It (6) composes the whole `UploadPart` (reserve →
  stage → commit), answering in the landed vocabulary (`UploadPartOutcome`
  `crates/core/src/multipart.rs:4606`, `Refusal` `:4487`).
- **Success criterion:** A NEW test file `crates/core/tests/multipart_upload_part_commit.rs`,
  over the same harness as 657.1's test: `RedbMetadataStore::in_memory()`, a test-local
  placement-aware chunk store, and a store wrapper that logs batches and can inject `Conflict`,
  `CommitUnknownResult` (either flag, applied or not), a held batch landed later, a write before
  a chosen commit, or **an action between two reads**. Each core test file carries its own copy,
  as `crates/core/tests/multipart_abort.rs` does. Every session is made by #894's Create, and
  every part is staged by 657.1. The test shows all of the following.
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
    **own** chunks (it must decode under `decode_retire_obligation`, `multipart.rs:3891`),
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
  - A same-number rival plus an unknown outcome, **with this attempt's commit not applied**
    (revised 2026-10-22): the wrapper answers the commit `CommitUnknownResult` without applying
    it (run once per `may_still_commit` value; the part number had no record before, so the
    commit required the key absent), a non-empty rival part then lands, and the
    re-read finds the rival's record **and this attempt's own slot still held at the bytes it
    last wrote**. The held slot is what proves the commit did not land (the commit deletes it),
    and because the commit required a value that cannot come back (the key absent, or a non-empty
    prior), it never can.
    Never `Committed`; the attempt compensates under its own slot and answers exactly
    `Refused(OperationAborted)`. This is the history that tells B apart from C3, whose commit
    **did** land and whose slot is gone: the own-commit proof is checked **first**, and the
    rival-record branch applies only when it fails (Design § *After a
    `CommitUnknownResult`*).
  - **A held commit, then an unknown empty compensation** (added 2026-10-22). An empty re-upload
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
  **(C) The three counterexamples found in #657's first attempt, each as a regression.**
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
    `multipart.rs:2732-2734`). The answer is still `Committed`, and the third attempt's slot is
    byte-for-byte unchanged: the attempt sends no batch that requires or rewrites it. 0016 names
    that third attempt as a writer of the key (`0016:1625-1628`), so "the key is present" does
    not mean "my slot is still held".
  - **C4, empty parts and retirement** (2026-10-20). An obligation must name at least one chunk:
    `RetireObligation` refuses an empty `chunks` list and an empty payload
    (`multipart.rs:3272-3277`, `:3705-3711`; pinned by
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
    typed error after **exactly** that many commits (the test counts them).
  **(D) Re-upload and ceilings.**
  - Re-uploading a committed part number replaces it under `require(part == prior)` and puts the
    prior part's chunks in a `retire:bytes:` obligation in the same batch (when the prior part
    has at least one chunk; C4).
  - A part whose staging hit the per-part bound (657.1) answers `Refused(EntityTooLarge { .. })`
    and compensates what it staged in one batch.
  - **The cumulative ceiling is a read-time check, and 0016 allows the race it leaves**
    (`0016:1441-1451`; `multipart.rs:5034-5043`). A commit whose read of the session's `psum:`
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
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree, the tracker's acceptance
  line and `AGENTS.md:41-45`'s rule.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and the wrapper forces every red leg rather than leaving it to timing. The rollback and the
  Abort are written by the wrapper before the commit; unknown and held outcomes are injected; C1
  pins the clock, so identical bytes are certain; C2's Abort and teardown run at each read gap
  by hook; C3's rival (and C3′'s third reserve) lands by hook between the commit and the re-read; the
held-commit / unknown-empty-compensation leg lands the held commit by hook the moment a
compensation is sent; C4's
  empty parts are planted, so no obligation can exist; the overshoot witness
  holds one commit by hand. (E) is the gating `C4-ci` check. In #657's first attempt, C1, C2 and
  C3 were each reproduced against the patched tree by independent probes, and the rollback path
  and the bounded rounds had no test that could fail.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the commit side. Every staged fragment stays named by a durable
  owned `sidx:` record until a `part:` record or a `retire:` obligation names it, in the same
  batch that removes the `sidx:` record. `Committed` is answered only on evidence that **this
  attempt's own** commit landed, evidence no other writer's record or teardown can produce. A
  slot is released exactly once, and never by an attempt that does not own it. No compensation is
  sent while the attempt's own commit may still land. An `Open` session is never answered `NoSuchUpload`, and a
  departed attempt's residue is left only to a reaper that will walk it. **Sources:** the
  maintainer's rule 2026-07-25 (via principles.md §5); 0016's refutation standard
  (`0016:2809-2813`); the commit classification (`0016:1584-1653`) and the two failure-mode rows
  on unknown outcomes (`0016:1780`, `:1792`). SELF-TEST: it binds the commit, the classification
  re-read, the compensation and the re-upload batches, so guarding one function cannot satisfy
  it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 932, 933
- **Conflicts with:**
- **Ordering note:** **RE-POINTED 2026-10-22 — `926` → `932, 933`.** #926 (657.1) was split at its third re-plan
  into #932 (657.1a: slot reserve, owned intent, drain fence, write and body deadlines) and #933
  (657.1b: in-flight renewal and `StagedAttempt::renew`, depends on #932) —
  `results/issue_926/split-proposal.md`. A split parent ends COMPLETE, so `926` would be met at
  once. `Depends on` both: this slice consumes the staged attempt #932
  returns, calls #933's `StagedAttempt::renew` as its quiesce step (`0016:1720-1733`), and edits
  the module and records they add. It builds on their folded result. #658
  (Complete) must depend on this slice, which supplies the composed `UploadPart` its tests stage
  parts through. **No `Conflicts with` #653:** this slice edits neither `crates/core/src/lib.rs`
  nor `docs/design/architecture/08-crosscutting-concepts.md`. Intake: created by the split of
  #657 under the human's per-id cap override of 2026-10-20 (alpha 7/6 before it).
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** the second half of `UploadPart` as store-I/O functions in `wyrd-core`. That means
  the part commit and its last renewal; the classification of a lost or unknown commit; the
  live-session compensation, including against a rolled-back `Open@E'`; the re-upload of a part
  number with the prior part's chunks retired; the per-part and cumulative ceiling refusals; and
  the composed `UploadPart`. Also: the writer-side constructor the part record lacks, with its
  `psum:` summary derived from it, the per-part `{chunks}` row of `RetireObligation` (which
  keeps refusing an empty list), the widened doc of `Refusal::OperationAborted` (no new variant),
  the docs sentence, and the new test file. / out of scope:
  - the slot reserve, the owned intent, the drain fence, placement, the fragment writes and
    renewal itself (657.1: #932 and #933; change them only where this slice's hand-off needs it,
    and say why);
  - Complete, the segment committer and publication (#658, #653); draining any `retire:`
    obligation, the owned-`sidx:` walk and the terminal delete (#659); the reaper (#625);
    seeded DST races (#660);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's);
    any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). On `main` no code writes a `part:` or `psum:`
  record; `PartRecord` (`multipart.rs:2930`) and `PartSummary` (`:3029`) have decoders and no
  writer.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator; confirmed with
  `run-verify.sh --classify`).
- **Verification posture:** (a) net-new. The commit functions exist neither on `main` nor on
  657.1's folded base, so the test cannot compile on C4-verify's reverted base, and C4-verify
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
  - a compensation sent while the attempt's commit is held and unsettled → (B)'s held-commit /
    unknown-empty-compensation leg;
  - the rival-record branch checked before the own-commit proof → C3 (it compensates a landed
    commit);
  - an empty compensation or re-upload that still writes a `{chunks}` obligation → C4 (the
    record is refused);
  - the rollback or same-part loser answering any refusal other than `OperationAborted` → (B).
  **Slot bytes, by invariant rather than by red** (revised 2026-10-22). "Every later batch requires
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
  equivalent.
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
  - **Records and keys:** `part_key` (`multipart.rs:1296`), `psum_key` (`:1311`), `PartRecord`
    (`:2930`), `PartSummary` (`:3029`), `retire_key` (`:1492`), `decode_retire_obligation`
    (`:3891`), and `RetireObligation` (`:3913`), the one writer-side route for an obligation: add
    the per-part `{chunks}` row there, as its doc at `:3615-3618` asks. Also `UploadPartOutcome`
    (`:4606`), `MAX_PART_CHUNKS` (`:4972`), `MAX_INFLIGHT_PARTS` (`:5032`) and
    `MAX_STAGED_CHUNKS` with its doc (`:5034-5043`). `SessionRecord::open` is the model for a
    crate-private writer-side constructor (its doc, `:2323-2328`).
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/tests/`,
  `docs/design/architecture/05-building-block-view.md`) on `origin/main` @ `12dd082`. Merged: the
  record family and decoders (#654), knobs (#801), Create (#908), Abort (#909). Iterated: #657's
  first attempt (to Plan 2026-10-19, oversized). Its commit-side defects are this slice's C1–C3
  legs. In that attempt the rollback (`Open@E'`) path never ran, the unknown-outcome checks for
  compensation had no test, and the loop bounds could be broken without any test failing. The
  new test file does not exist. Open PRs on getwyrd/wyrd touching these paths: none
  (2026-10-20).
- **Disposition hint:** new-feature

## Design

**Quiesce, then commit** (`0016:660`, `:1720-1733`). Run the staged attempt's last renewal, with
a lease covering the commit's transaction deadline, and send no renewal after it. #933's
`StagedAttempt::renew` takes that coverage as `cover_millis`: pass at least the commit's
transaction deadline, and treat its typed coverage error as a refusal to commit. Before building
the batch, read the bounded `psum:<id>:` range and refuse `EntityTooLarge` past
`MAX_STAGED_CHUNKS`, compensating what was staged. That read is unfenced on purpose: the overshoot
concurrent commits can add is bounded by the slot cap and charged in `U_REF` (`0016:1441-1451`),
so no precondition on the total belongs in the batch. Then commit one batch:
`require(mpu == Open@E)`; `require_absent(part:…)`, or `require(part:… == prior)` for a re-upload;
`require(slot:<id>:<k> == prior)`; put `part:` and its `psum:` summary; delete the slot; delete the
attempt's `sidx:` entries; and, for a re-upload, put one `retire:bytes:` obligation for the
replaced part's chunks (token per `0016:358-373`, installed under `require_absent`). The part
digest is the SHA-256 basis `Digest` already uses (`multipart.rs:1036-1052`, ADR-0047).

**After a `Conflict`, classify from a re-read** (`0016:1597-1647`). Session fenced (non-`Open`) ⇒
`NoSuchUpload`, no self-compensation. Session back at a newer `Open@E'` ⇒ compensate under
`Open@E'`, chasing a further rollback a bounded number of times. Part key moved (same-part rival)
⇒ compensate. Own slot moved with the part unchanged ⇒ benign retry. **Compensation** is one
batch under `require(mpu == Open@E)` and `require(slot == prior)`: one `retire:bytes:` obligation
naming this attempt's own chunks, the slot delete, and the `sidx:` deletes (`0016:673`).

**Empty chunk lists.** An obligation must name something: `RetireObligation` refuses an empty
`chunks` list and an empty payload (`multipart.rs:3272-3277`, `:3705-3711`), and this slice
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
once its commit is settled as not landed and unable to land later. Settled means one of: a read
found its own slot still at the bytes it last wrote and the commit's outcome was
`may_still_commit: false` (out of flight, and the commit deletes the slot, so it did not land);
or a read found a precondition of the commit that can never hold again — the session no longer
`Open@E` (the epoch only rises), or the part key no longer holding what the commit requires, when
that value cannot come back — the commit required the key absent and it is now present (a
`part:` record is removed only after the session leaves `Open`), or it required a **non-empty**
prior record, whose chunk ids no later write can carry — with the own slot still at its
last-written bytes. An empty prior's bytes **can** come back (C1: an empty rival in the same
millisecond writes them again), so a rival's empty record settles nothing. Until it is settled, the attempt may re-read and may retry the
commit (both requiring the same slot bytes, so at most one lands), but not compensate; when its
rounds run out it returns the in-doubt error.

**The answer for a lost commit on a live session.** When the session is still `Open` and the
attempt compensates — a same-number rival won, or a Complete fenced and rolled the session back
to `Open@E'` — it answers exactly `Refused(OperationAborted)`: a conflicting operation on the
resource intervened and the client may retry the part, which is what S3's
`409 OperationAborted` says. That variant's doc says only "fenced into `Completing`"
(`multipart.rs:4494-4496`); widen it in this slice to cover these two cases too, and add no new
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
(`multipart.rs:2732-2734`), and that identifies this attempt's commit, provided (a) this attempt
sent no compensation that could have deleted it, and (b) the session was **still the same
`Open@E` after** the slot was observed gone (C3′). Every later batch of this attempt requires
the slot bytes **this attempt last wrote**, never bytes re-read from the key, so a slot another
attempt now holds is never required, rewritten or deleted by this one. A session state read before the
slot read cannot vouch for it (C2); one way that holds is to read the session last, or again
after the slot and part reads. Anything that does not settle it proves "did not land" **only**
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
constructor — see `SessionRecord`'s doc", `multipart.rs:2927`). Add a crate-private one in the
shape of `SessionRecord::open`, so every value written is one the decoder accepts back unchanged.
`PartSummary` has public fields (`:3029-3038`); build the `psum:` value from the `PartRecord` it
summarizes, so the pair cannot disagree (`:3022-3026`).

**Docs currency** (`AGENTS.md:154-157`). Extend the staging paragraph #932 adds (and #933
extends) right after the multipart records paragraph of
`docs/design/architecture/05-building-block-view.md` (`:244` on `main`); do not rewrite `:244`
itself, a single 8.5 KB line: `part:` and
`psum:` now have their writer, the commit lands them with the slot release and `sidx:` deletes in
one batch, and nothing calls it in production until #669.

## Alternatives considered

- **A per-session in-flight counter.** A shared writable key on the part path; commits starve on
  unrelated parts' releases (`0016:1686-1742`). Rejected; the slot key space is the cap.
- **Compensate on any non-`Committed` outcome.** It orphan-marks a part that exists after an
  unknown outcome (`0016:1792`). Rejected; re-read first.
- **An attempt id in `PartRecord`, to make the bytes attempt-specific.** It changes a landed
  record shape and its decoder, which #658 and #659 read, and the slot already identifies the
  attempt. Rejected for this slice.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-20): all three findings hold at `12dd082` and are revised in place.
(1) Slot reuse: the classification now uses the slot's `attempt_id`, not whether the key is
present (0016 names a later reserver as a writer, `:1625-1628`), and later batches require the
attempt's own last-written bytes. C3′ adds a third attempt reserving the freed index before the
re-read: the answer is `Committed` and that slot is untouched. (2) Empty parts: an obligation is
installed only for a non-empty chunk list (the existing record contract stays), the slot release
stays atomic, and an empty compensation's unknown outcome is settled by slot and session; C4
tests it. (3) Answer: both live-session losses answer exactly `Refused(OperationAborted)`, its doc
is widened in scope, no new variant; flagged for sign-off.

Plan-review response (2026-10-22): all three findings hold at `4bac538` and are revised in place.
(1) Compensation now waits for a settled not-committed commit, as `0016:1649-1651` requires
(Design § *Settle before compensating*): the slot CAS proves one of the attempt's releases
landed, not which one, so slot-gone settles a compensation only when no commit is outstanding.
An empty rival settles nothing, because its bytes can recur. New leg in (B): a held empty commit,
an identical empty rival, and an unknown empty compensation, which must never answer
`OperationAborted`. (2) B's rival-plus-unknown leg now states its history: the commit was not
applied and the own slot is still held. The own-commit proof is checked before the rival-record
branch, so B and C3 no longer prescribe opposite answers to the same observation. (3) The
slot-bytes rule is no longer a C3′ red, since no conforming path reaches the retry builder there.
It is a wrapper check on every batch in every leg, plus a cited builder and a listed survivor.
