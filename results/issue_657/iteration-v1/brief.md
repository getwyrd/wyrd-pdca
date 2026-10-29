# core: UploadPart staging — slots, sidx/psum, drain fence (636.4) — issue 657

> The Plan artifact for slice 4 of #636. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read the `slot:` / `part:` /
> `psum:` / `sidx:` rows of the record table (`0016:349-353`), the **Part slot reserve**, **Part
> intent**, **Part commit** and **Live-session compensation** rows of the batch inventory
> (`0016:658-660`, `:673`), and **decision 5** (`0016:1507-1793`, above all the commit
> classification `:1584-1647`, the reserve's unknown-outcome rule `:1699-1719`, and the
> quiesce-before-commit rule `:1720-1733`) before writing code. Citations verified against
> `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-19. #636 itself was closed not-planned on
> 2026-10-10 (its discontinued bundle's cleanup); its slices carry on, and #654, #655, #656
> (as #894 + #895) are merged.

- **Slug:** multipart-upload-part-staging
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** Sessions can be created (#894) and aborted (#895), but nothing stages a part. This
  slice adds `UploadPart`'s staging as `wyrd-core` functions over `MetadataStore` and a
  placement-aware chunk store, answering in the landed vocabulary (`ReserveOutcome`
  `crates/core/src/multipart.rs:4590`, `UploadPartOutcome` `:4606`, `Refusal` `:4487`): claim
  one in-flight slot `slot:<id>:<k>`; stream the part's chunks, registering each as an owned
  `sidx:` staging entry **before** its fragments are written, in a batch fenced on the session
  being `Open@E` and on no selected D-server having a drain recorded; renew the owned leases and
  the slot together; and commit the `part:` + `psum:` pair, the slot release and the `sidx:`
  deletes in one batch. Staged placement selects against the topology **minus draining
  servers**, and a drain that lands mid-stage makes the part re-plan, never refuse.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_upload_part.rs`, over
  `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store (the shape of
  `Fleet`, `crates/core/tests/domain_placement.rs:113`), and a test-local store wrapper that can
  log batches and inject an outcome or a write before a chosen commit. Every session in it is
  made by #894's Create. It shows all of the following.
  **(A) Staging and commit.** Parts staged **out of order** (3, then 1, then 2), at least one of
  them more than one chunk, each commit to `Committed { part_number, digest }`. While a part is
  mid-stage (after its first chunk's intent, before its commit), `scan("sidx:")` is non-empty and
  `scan("pending:")` is empty. After each commit, that part's `part:` and `psum:` records decode
  and agree (chunk count, length, digest), and no `sidx:` entry or `slot:` record of that part
  attempt remains. The wrapper's log shows every chunk's `sidx:` commit **before** the first
  fragment put of that chunk (`0016:1788`). **The write deadline is exact.** The test passes a
  controlled clock and a write window `W` (the functions take both as inputs, see Design), and
  the chunk-store double records `deadline_millis` (`crates/traits/src/lib.rs:1229`; #638) on
  every staged fragment put. Each one equals `t_auth + W`, where `t_auth` is the clock's reading
  taken before that chunk's intent batch was committed. With the clock advanced between the
  intent commit and the fragment dispatch (a delayed dispatch, the wrapper advancing it right
  after the intent commit returns), the deadline is still `t_auth + W`: never `t_dispatch + W`,
  never `None`, and never an unbounded value such as `u64::MAX`.
  **(B) The drain fence (kills the fence mutants of the discontinued patch).**
  - **B1:** the wrapper writes `desired:dserver:<S>` for a server `S` the plan selected, **just
    before** that chunk's intent commits. The intent must fail and re-plan; the part still
    commits, and no `sidx:`, `part:` or fragment of it names `S`. Removing the per-server
    `require_absent(desired:dserver:<S>)` from the intent batch makes B1 fail.
  - **B2:** with `desired:dserver:<S>` present **before** staging, and `S` the server the
    selector prefers (lowest utilization), the part commits, nothing names `S`, and the wrapper
    sees **no** intent conflict. Planning against the full topology instead of the topology minus
    draining servers makes B2 fail.
  - **B3:** with 4 of 12 single-server domains draining and a scheme that fits in the remaining 8
    (e.g. RS(4,2)), a part stages and commits. With too few non-draining domains for the scheme,
    the answer is a typed error, never `NoSuchUpload` for an `Open` session.
  **(C) Slots.** `MAX_INFLIGHT_PARTS` reserves (`multipart.rs:5032`) succeed on one session and
  the next answers `Refused(SlowDown { pressure: Backpressure::InflightParts { .. } })`
  (`multipart.rs:4470`, `:4528`); a reserve against a non-`Open` session
  answers `Refused(NoSuchUpload)` and writes nothing. A reserve whose batch lands but whose
  outcome is `CommitUnknownResult` adopts its own slot on re-read (same `attempt_id`): the
  session's held-slot count equals the number of live attempts (`0016:1779`). With
  `may_still_commit: true` and the slot read absent, the reserve retries the **same** index with
  the same `attempt_id`, never a different one (`0016:1707-1716`).
  **(D) Renewal.** Each renewal batch rewrites the attempt's own `slot:` record in the same batch
  as its owned `sidx:` leases (`0016:349`, `:659`); a renewal after the session leaves `Open`
  refuses and resurrects nothing.
  **(E) A lost commit is classified, never compensated blind** (`0016:1584-1647`).
  - The session fenced to `Aborting` between the last intent and the commit: the commit answers
    `Refused(NoSuchUpload)` and writes nothing (the residue is the drain's).
  - A same-part rival committed first: the loser commits one `retire:bytes:` obligation for its
    **own** chunks (it must decode under `decode_retire_obligation`, `multipart.rs:3891`),
    releases its slot and deletes its `sidx:` entries in one batch, and answers a refusal.
  - A commit whose batch lands but answers `CommitUnknownResult`: the writer re-reads and finds
    **its own** record (the exact `part:` bytes it built; its chunk ids are its own, so no other
    attempt writes those bytes), answers `Committed`, and releases the slot exactly once.
  - **A re-upload whose batch did not land** answers `CommitUnknownResult { may_still_commit:
    false }`: the re-read finds the **prior** part. The writer must not answer `Committed` for it;
    it carries on as for a commit that did not land, and ends with its own part committed.
  - **A held batch** (`may_still_commit: true`; the wrapper holds the batch and lands it later,
    as `crates/core/tests/multipart_abort.rs:961-980` does): the re-read finds the prior part (or
    none). The writer neither answers `Committed` nor compensates on that read alone. Every batch
    it sends next requires its own slot at the bytes the held batch also requires, so at most one
    of them lands. Run it three times: (i) the held batch released before the writer's next
    batch; (ii) released after the writer has returned; (iii) every later batch of the writer
    answered `Conflict` without being applied until its rounds run out, then the held batch
    released. In every run exactly one of {its own part, its compensation} is durable at the end,
    the slot is released exactly once, and the answer is `Committed` only if its own part is the
    one that landed. In (iii) the writer returns an in-doubt error that keeps the
    `may_still_commit` doubt, as Abort's does (`crates/core/src/multipart/abort.rs:264-274`).
  - **A same-number rival plus an unknown outcome:** the re-read finds the rival's record (not
    its own bytes). Never `Committed`; the writer compensates under its own slot.
  **(F) Re-upload and ceilings.** Re-uploading a committed part number replaces it under
  `require(part == prior)` and puts the prior part's chunks in a `retire:bytes:` obligation in
  the same batch. **The staged-chunk ceiling is a read-time check, and 0016 allows the race it
  leaves** (`0016:1443-1450`; `multipart.rs:5034-5042`). A commit whose read of the session's
  `psum:` range shows it would take the cumulative staged chunks past `MAX_STAGED_CHUNKS`
  (`multipart.rs:5043`) answers `Refused(EntityTooLarge { .. })` and compensates what it staged.
  Concurrent commits may overshoot by at most `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`. **Witness,
  forced by the wrapper:** with `psum:` records planted one chunk below the ceiling, two attempts
  on two different part numbers, each holding a one-chunk part, both read that one chunk of
  headroom (the wrapper holds the first
  commit until the second has committed, then releases it). Both commit, and the total is
  `MAX_STAGED_CHUNKS + 1`, inside the bound. A third commit after them refuses. Do **not** add a
  shared counter or a session-wide fence to stop the overshoot: 0016 rejects that
  (`0016:1686-1742`). Use a test-sized ceiling only if the function takes it as a parameter;
  otherwise plant `psum:` records to approach it.
  **(G) The whole gate.** `cargo xtask ci` is green on the patched tree. This is the tracker's
  own acceptance line, and `AGENTS.md:41-45` requires it for any change that can affect the Rust
  build. It matters here because this slice also edits `wyrd-custodian` and the architecture docs,
  which `cargo test -p wyrd-core` does not cover.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and every red leg is forced by the wrapper rather than left to timing: B1's drain record is
  written by the wrapper right before the intent commits; (C) and (E)'s unknown outcomes, held
  batches and rival commits are injected; (A)'s deadline runs on a test-controlled clock; (F)'s
  overshoot witness holds one commit by hand. (G) is the gating `C4-ci` check, which runs
  `cargo xtask ci`. In the discontinued #636 patch both fence mutants (deleting the
  `require_absent` loop; replacing `excluding(draining)` with `clone()`) survived the whole suite;
  B1 and B2 exist to kill exactly those two, and Do must show it (see `Verification posture`).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*). Every staged fragment is named by a durable owned `sidx:` record
  (with its placement) from **before** it is written until a `part:` record or a `retire:`
  obligation names it in the same batch that removes the `sidx:` record; no owned record can be
  created once the session leaves `Open` or naming a server whose drain is recorded; and no
  session can be left unable to accept parts by anything but its own slots being held (no
  refusal of an `Open` session as `NoSuchUpload`). **Sources:** the maintainer's rule 2026-07-25
  (via principles.md §5); 0016's refutation standard (`0016:2802-2813`); decision 5's
  properties (`0016:1529-1545`) and the drain fence (`0016:353`, `:659`). SELF-TEST: it binds the
  intent, renewal, commit and compensation batches and the placement step, so guarding one
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 656
- **Conflicts with:**
- **Ordering note:** `Depends on 656`: Create and Abort (merged as #894 + #895; #656 is the split
  parent, COMPLETE). This bundle is in wave 1 of the 2026-10-19 batch beside #653; #658
  (Complete) depends on both. **No `Conflicts with` 653, on purpose:** #653 puts its committer in
  a new core module and edits neither `crates/core/src/metadata.rs` nor
  `docs/design/architecture/05-building-block-view.md`, both of which this slice may edit. Do not
  touch `crates/core/src/lib.rs` or `docs/design/architecture/08-crosscutting-concepts.md`
  (#653's). Intake cap (wyrd-pdca-P1): this brief is inside the room (alpha 5/6 when written,
  room 1; the second of the two in-room briefs).
- **Scope:** `UploadPart`'s staging as store-I/O functions in `wyrd-core`: slot reserve, owned
  intent, renewal, part commit with its classification, the live-session compensation, re-upload
  of a part number, and the per-part and cumulative chunk ceilings; staged placement against the
  topology minus draining servers, with re-plan on a drain-fence conflict; one home in core for
  the `desired:dserver:<S>` key so core and the custodian spell it once; the writer-side
  constructors the slot and part records lack; the docs sentence; the new test file. / out of
  scope:
  - Complete, the segment committer and publication (#658, #653);
  - draining any `retire:` obligation, the owned-`sidx:` walk, and the terminal delete (#659);
  - the reaper (#625); seeded DST races for the protocol (#660);
  - the D-server side of the `W_write` deadline (built by #638; this slice only passes the
    deadline on every staged write);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). Today `write::intent` writes only ordinary
  `pending:` entries with `owner: None, staged: None` (`crates/core/src/write.rs:201-219`), and no
  code writes a `slot:`, `part:`, `psum:` or `sidx:` record.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part.rs`, a **NEW** file (C4-verify treats
  an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new. The staging functions do not exist on `main`, so the
  discriminating test cannot compile on C4-verify's reverted base and C4-verify reports
  **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Built and exercised at Check:** the reserve, intent, renewal, commit and compensation paths,
  driven by the new test over a real store and chunk-store double.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - the per-server `require_absent(desired:dserver:<S>)` removed from the intent batch → B1 fails;
  - planning against the full topology instead of the topology minus draining servers → B2 fails;
  - the slot rewrite dropped from the renewal batch → (D) fails;
  - compensating on `CommitUnknownResult` without re-reading → (E)'s unknown-outcome leg fails;
  - after an unknown outcome, "a part is there" taken as `Committed` without comparing it to the
    attempt's own bytes → (E)'s re-upload leg fails;
  - a different index probed after an ambiguous reserve outcome → (C)'s same-index leg fails;
  - a staged fragment put with `None`, with `Some(u64::MAX)`, or with its deadline taken at
    dispatch instead of at authorization (three edits) → (A)'s deadline leg fails.
- **Production reach:**
  - (a) The test exercises staging for real against a real store. Production stages no part in
    this slice.
  - (b) The S3 verb lands with #669 under #508, after #658/#659 and the reaper (#625), per 0016's
    same-release rule (`0016:236-268`).
  - (c) This slice **must not** add a production caller.
  - (d) The one existing production path it may touch is the custodian's use of the
    `desired:dserver:` key, which must keep the same bytes.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer callsite to mirror for the per-chunk loop:** `lease_write_chunk` and
    `stream_write_data` (`crates/core/src/write.rs:485-535`, `:559`): lease (here: owned intent)
    before writing a chunk's fragments, renew half a TTL ahead, and **refuse rather than
    resurrect** a lapsed lease (`:496-517`). Placement as `WritePlan::place`
    (`write.rs:112`) does it, over `Topology::excluding` (`crates/core/src/placement.rs:147`).
  - **Peer for the fence classification:** #895's Abort, `crates/core/src/multipart/abort.rs:282`
    (re-read after `Conflict`; never read `CommitUnknownResult` as `Conflict`; keep a
    `may_still_commit: true` doubt).
  - **The drain-fence key:** today only `crates/custodian/src/desired_state.rs:35-40`
    (`DESIRED_PREFIX`, `desired_key`). Core cannot depend on the custodian (ADR-0010), so the key
    gets one home in core, the custodian uses it, and its bytes do not change. The custodian's
    `deferred: #657` notes (`desired_state.rs:228`, `:248`) are then settled: update them to say
    the intent writer now fences (the seeded interleaving stays `deferred: #665`).
  - **Records and keys:** `slot_key` / `slot_range` (`multipart.rs:1251`, `:1261`), `part_key`
    (`:1296`), `psum_key` (`:1311`), `sidx_key` / `sidx_range` (`:1328`, `:1339`), `SlotRecord`
    (`:2719`), `PartRecord` (`:2930`), `PartSummary` (`:3029`), `StagedPlacement::new` (`:4045`),
    `OwnedEntry::new` (`:4136`), `retire_key` (`:1492`), `decode_retire_obligation` (`:3891`),
    `RetireObligation` (`:3913`, the one writer-side route for an obligation: add the per-part
    `{chunks}` row there, as its doc at `:3615-3618` asks),
    `upload_part_answer` (`:4756`), `MAX_PART_CHUNKS` (`:4972`), `MAX_INFLIGHT_PARTS` (`:5032`),
    `MAX_STAGED_CHUNKS` (`:5043`). `SessionRecord::open` is the model for a crate-private
    writer-side constructor (see its doc, `:2323-2328`).
- **Prior-art check (triage cycles):** searched by path `crates/core/src/write.rs`,
  `crates/core/src/multipart.rs`, `crates/core/src/multipart/`, `crates/core/src/metadata.rs`,
  `crates/custodian/src/desired_state.rs`, `crates/core/tests/` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main`. Merged: the record family
  and decoders (#654 via PRs #703, #725, #734, #793), knobs (#801), Create (#908), Abort (#909),
  the custodian's staged protection and drain counting (#807, #822). Rejected: #636's single-slice
  attempt (discontinued 2026-10-10, `results/issue_636/patch.diff` in wyrd-pdca), whose staging
  let both fence mutants survive and answered `NoSuchUpload` for an `Open` session with 4 of 12
  D-servers draining. The new test file does not exist. Open PRs on getwyrd/wyrd: none.
- **Disposition hint:** new-feature

## Motivation

`UploadPart` is the only way bytes enter a multipart session. Every later slice reasons about
staged bytes through the records this slice writes: the custodian's staged protection class
already reads `part:` and `sidx:` (#807), drain status counts them (#822), and the drain and
terminal delete (#659) will walk them. Until something writes them through the fences 0016
requires, those readers protect a class nothing produces.

## Design

**Reserve** (`0016:349`, `:658`, `:1676-1719`). Read the bounded `slot:<id>:` range and the
session; refuse `NoSuchUpload` unless `Open@E` (`upload_part_answer`); pick a free index `k`;
commit `require(mpu == Open@E)` + `require_absent(slot:<id>:<k>)` + put the slot record
(`part_number`, `attempt_id`, `reserved_at_millis`, `lease_expiry_millis`). A lost index probes
the next free one, at most `MAX_INFLIGHT_PARTS` probes; a full range is `SlowDown { InflightParts }`.
On `CommitUnknownResult`, re-read `slot:<id>:<k>`: own `attempt_id` means adopt it; with
`may_still_commit: false` anything else means it did not land; with `may_still_commit: true`
absence proves nothing, so retry the same index with the same `attempt_id`, and give up with a
refusal after a bounded number of ambiguous rounds.

**Intent per chunk** (`0016:353`, `:659`). Plan the chunk's placement against the topology minus
the servers with a `desired:dserver:` record (read from the store), then commit one batch:
`require(mpu == Open@E)`, `require_absent(desired:dserver:<S>)` for **every** server in the
placement, `require(slot:<id>:<k> == prior)`, put the owned `sidx:<id>:<part>:<chunk>` entry
(`OwnedEntry` with the planned `StagedPlacement`), and rewrite the slot (its lease moved forward).
A `Conflict` re-reads: session not `Open` ⇒ stop with `NoSuchUpload` and leave the residue to the
drain; a drain record now present ⇒ re-plan without that server and try again. Only then write the
fragments, each carrying its authorization deadline (`put_fragment_at`'s `deadline_millis`,
`crates/traits/src/lib.rs:1224-1229`; the D server refuses a late write, #638): `t_auth + W_write`,
where `t_auth` is the clock reading taken **before** the intent batch is committed (so the deadline
can only be early, never late), and the same value for every fragment of that chunk however late
it is dispatched. `W_write` is `W_WRITE_MILLIS` (`crates/custodian/src/gc.rs:202`), which core
cannot import (ADR-0010), so the caller supplies the value, as it supplies the clock; the function
must not substitute an unbounded or default deadline when none is given. The orphan grace is sized on that bound
(`G_orphan > W_write + δ_clock`, `0016:1567-1578`); an unbounded staged write would let a
straggler land unevidenced.

**Renewal.** Half a TTL ahead, as `lease_write_chunk` does, one batch renews every owned `sidx:`
lease of this attempt **and** rewrites its slot, under `require(mpu == Open@E)`. A renewal that
fails because the session left `Open` stops the upload; it never recreates anything. Quiesce the
renewal before building the commit batch, with a last renewal whose lease covers the commit
(`0016:1720-1733`).

**Commit** (`0016:660`). One batch: `require(mpu == Open@E)`, `require_absent(part:…)` (or
`require(part:… == prior)` for a re-upload), `require(slot:<id>:<k> == prior)`; put `part:` and
its `psum:` summary; delete the slot; delete the attempt's `sidx:` entries; for a re-upload, put
one `retire:bytes:` obligation for the replaced part's chunks (token per `0016:358-373`, installed
under `require_absent`). Before building it, read the bounded `psum:<id>:` range and refuse
`EntityTooLarge` past `MAX_STAGED_CHUNKS`, compensating what was staged. That read is not fenced on
purpose: the overshoot concurrent commits can add is bounded by the slot cap and charged in
`U_REF` (`0016:1443-1450`), so no precondition on the total belongs in the batch. A part over
`MAX_PART_CHUNKS` is refused the same way before it outgrows one value. The part digest is the
SHA-256 basis `Digest` already uses (`multipart.rs:1038-1050`, ADR-0047).

**After a `Conflict`, classify from a re-read** (`0016:1597-1647`): session fenced (non-`Open`)
⇒ `NoSuchUpload`, no self-compensation; session back at a newer `Open@E'` ⇒ compensate under
`Open@E'`; part key moved (same-part rival) ⇒ compensate; own slot moved with the part unchanged
⇒ benign retry. **Compensation** is one batch under `require(mpu == Open@E)` and
`require(slot == prior)`: one `retire:bytes:` obligation naming this attempt's own chunks, the
slot delete, and the `sidx:` deletes.

**After a `CommitUnknownResult`, settle identity, not presence** (`0016:1630-1651`: "finds its own
commit present"). `PartRecord` has no attempt id (`multipart.rs:2930-2935`), so the test of "its
own" is the record's bytes: the re-read `part:` value equals the exact value this attempt built
(its chunk ids are its own). Equal ⇒ `Committed`, and the slot is not released a second time.
Anything else (the prior part on a re-upload, a rival's record, or nothing) proves the batch did
not land **only** when `may_still_commit` is `false`; with `true` it proves nothing
(`crates/traits/src/lib.rs:240-247`). Keep the doubt the way #895's Abort does: every later batch
of this attempt (the commit retried, a renewal, or the compensation) requires its own slot at the
bytes the held batch also requires, and the held batch deletes that slot, so whichever lands first
excludes the other and settles the doubt. Bound the rounds; if they run out unsettled, return an
in-doubt error carrying the doubt, never a guess.

**Writer constructors.** The slot and part records have no writer-side constructor today
("No writer-side constructor — see `SessionRecord`'s doc"). Add crate-private ones in the shape of
`SessionRecord::open`, so every value written is one the decoder accepts back unchanged.

**Docs currency** (`AGENTS.md:154-157`): update the multipart records paragraph at
`docs/design/architecture/05-building-block-view.md:244` (it says the types are "landed ahead of
most of their writers"): `slot:`, `sidx:`, `part:` and `psum:` now have their writer, staging
registers each chunk before its bytes are written, in a batch that fails once the session has
left `Open` or a selected server's drain is recorded, and nothing calls it in production until
#669.

## Alternatives considered

- **A per-session in-flight counter.** Shared writable key on the part path; commits starve on
  unrelated parts' releases (`0016:1686-1742`). Rejected; the slot key space is the cap.
- **Refuse the part when a selected server starts draining.** Routine operator action becomes a
  client-visible failure across the fleet (the discontinued patch's `NoSuchUpload` with 4 of 12
  draining). Rejected; re-plan.
- **Compensate on any non-`Committed` outcome.** Orphan-marks a part that exists after an unknown
  outcome (`0016:1792`). Rejected; re-read first.
- **Owned entries under `pending:`.** Every global `pending:` scan would enumerate them
  (`0016:1773`). Rejected; `sidx:` is disjoint.

## Impact & compatibility

No record shape changes: every record written here already has a decoder on `main`. The custodian
keeps reading the same `desired:dserver:` bytes. No production path changes behaviour.

## Open questions

- None blocking.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-19, revision pass): all four findings accepted and the brief
revised. (1) Unknown commit: "its own commit present" is now tested by the record's exact bytes,
`may_still_commit: true` keeps the doubt, and (E) adds a re-upload that did not land, a held batch
released before and after the writer returns, and a rival plus unknown outcome. (2) Ceiling: (F)
now separates the read-time refusal from the overshoot 0016 allows (`0016:1443-1450`) and adds a
two-commit witness; it forbids adding serialization. (3) Deadline: (A) asserts `t_auth + W`
exactly on a controlled clock, including a delayed dispatch, and the mutants now include
`Some(u64::MAX)` and a dispatch-time deadline. (4) (G) adds `cargo xtask ci` green.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Oversized slice (patch 196 KB, threshold 100 KB; ~3.7k added lines). Split in Plan (`pdca split 657`), e.g. (a) slot reserve + owned sidx: intent with drain fence + renewal, (b) part commit + ambiguous-outcome classification + compensation + re-upload + ceilings. The rebuild of whichever slice owns them must fix the reproduced defects: - Empty-part identity (upload_part.rs:810, :1423): identical empty-part bytes are taken as this attempt's commit -> false Committed and a leaked slot. Count a commit as landed only with attempt-specific evidence (part bytes AND own slot gone; read part before slot). - Torn read (upload_part.rs:1071-1073, :1134-1151): session read before slot/part lets an Abort+teardown in between turn an absent slot into a false Committed. Re-read/read session last before inferring a landed commit; add the interleaving as a regression. - settle() (:1118 vs :1139-1145) prunes the may_still_commit:false pending commit before the slot-release branch -> landed commit reported as SlotLost. - Renewal: none while buffering below a chunk boundary (:935); renewal resurrects already-expired leases (:1318) — must refuse. Tests: the (C) same-index test must discriminate pinning (occupy index 0, hold reserve at 1, release 0 before retry; assert pinned and one slot). Cover unknown-outcome paths for intent/renewal/compensation, reopen (rolled-back Complete), bounded rounds, and a multi-piece body stream (C5 left 31 survivors there).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Oversized slice (patch 196 KB, threshold 100 KB; ~3.7k added lines). Split in Plan (`pdca split 657`), e.g. (a) slot reserve + owned sidx: intent with drain fence + renewal, (b) part commit + ambiguous-outcome classification + compensation + re-upload + ceilings.
  The rebuild of whichever slice owns them must fix the reproduced defects:
  - Empty-part identity (upload_part.rs:810, :1423): identical empty-part bytes are taken as this attempt's commit -> false Committed and a leaked slot. Count a commit as landed only with attempt-specific evidence (part bytes AND own slot gone; read part before slot).
  - Torn read (upload_part.rs:1071-1073, :1134-1151): session read before slot/part lets an Abort+teardown in between turn an absent slot into a false Committed. Re-read/read session last before inferring a landed commit; add the interleaving as a regression.
  - settle() (:1118 vs :1139-1145) prunes the may_still_commit:false pending commit before the slot-release branch -> landed commit reported as SlotLost.
  - Renewal: none while buffering below a chunk boundary (:935); renewal resurrects already-expired leases (:1318) — must refuse.
  Tests: the (C) same-index test must discriminate pinning (occupy index 0, hold reserve at 1, release 0 before retry; assert pinned and one slot). Cover unknown-outcome paths for intent/renewal/compensation, reopen (rolled-back Complete), bounded rounds, and a multi-piece body stream (C5 left 31 survivors there).
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 120 mutants tested in 29m: 31 missed, 48 caught, 28 unviable, 13 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_657/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
