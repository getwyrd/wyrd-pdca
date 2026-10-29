# core: UploadPart staging — slot reserve, owned intent, drain fence, renewal (657.1)

> First of the two slices #657 was split into on 2026-10-20 (the second is the part commit,
> 657.2). Do reads ONLY this file. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read these before writing
> code: the `slot:` and `sidx:` rows of the record table (`0016:349`, `:353`); the **Part slot
> reserve** and **Part intent** rows of the batch inventory (`0016:658-659`); decision 5's
> admission and liveness text (`0016:1676-1733`, above all the reserve's unknown-outcome rule
> `:1699-1719`); and the slot lease rules (`0016:1824-1847`). Citations were verified against
> `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-20.

- **Slug:** multipart-upload-part-stage
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** Sessions can be created (#894) and aborted (#895), but nothing stages a part's
  bytes, and no code writes a `slot:` or `sidx:` record. This slice adds the first half of
  `UploadPart` as `wyrd-core` functions over `MetadataStore` and a placement-aware chunk store.
  It (1) claims one in-flight slot `slot:<id>:<k>`, answering in the landed vocabulary
  (`ReserveOutcome` `crates/core/src/multipart.rs:4590`, `Refusal` `:4487`). It (2) streams the
  body into chunks and registers each one as an owned `sidx:` entry **before** any of its
  fragments is written, in a batch fenced on the session being `Open@E` and on no selected
  D-server having a drain recorded. Placement is against the topology **minus draining
  servers**, and a drain that lands mid-stage makes the chunk re-plan, never refuse. It (3)
  writes each fragment with a bounded authorization deadline. It (4) renews the slot and the
  owned leases together for as long as the request is live. It (5) hands the staged attempt to
  the commit (657.2). No `part:` or `psum:` record is written here.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_upload_part_stage.rs`. It
  runs over `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store (the
  shape of `Fleet`, `crates/core/tests/domain_placement.rs:60`, with its `PlacementChunkStore`
  impl at `:113`), and a test-local store wrapper. The wrapper logs every batch and can, before
  a chosen commit: answer `Conflict` without applying; answer `CommitUnknownResult` with either
  `may_still_commit` value, applied or not; hold the batch and land it later (as
  `crates/core/tests/multipart_abort.rs:961-980` does); or write a key first. Every session in
  it is made by #894's Create. The test shows all of the following.
  **(A) Staging, and the order that makes it safe.** A body of at least three chunks is fed as a
  stream of **uneven pieces**: one piece smaller than a chunk, one that spans a chunk boundary,
  and none aligned to the chunk size. It stages exactly `ceil(len / chunk_size)` chunks, and the
  chunk bytes read back through the fragments, concatenated, equal the body. The staged
  attempt's length and digest equal the body's length and its SHA-256 `Digest`
  (`multipart.rs:1036-1052`, ADR-0047). While it stages (after the first intent, before the
  function returns), `scan("sidx:")` is non-empty and `scan("pending:")` is empty. When staging
  returns, the store holds this attempt's one `slot:` record and exactly one owned `sidx:` entry
  per chunk, each decoding with the placement its fragments were written to, and nothing under
  `part:` or `psum:`. The wrapper's log shows every chunk's `sidx:` commit **before** the first
  fragment put of that chunk (`0016:1788`). **The hand-off carries complete chunk references:**
  the staged attempt lists, in chunk order, one full `ChunkRef` per chunk — `id`, `scheme`, `len`
  and `placement`, every field `ChunkRef` has (`crates/core/src/metadata.rs:351-361`) — because
  `PartRecord` stores exactly that list (`multipart.rs:2930-2935`) and neither `OwnedEntry` nor
  `StagedPlacement` records a chunk's length (`:4136-4139`, `:4045-4047`). Assert: the `len`s
  sum to the body length; every chunk but the last has `len == chunk_size` and the last has the
  short remainder; each `id` is the chunk id of its owned `sidx:` entry; and each
  `scheme`/`placement` equals that entry's `StagedPlacement`.
  **(A′) The write deadline is exact.** The test passes a controlled clock and a write window
  `W`; the functions take both as inputs. The chunk-store double records `deadline_millis` on
  every staged fragment put (`crates/traits/src/lib.rs:1224-1229`; #638). Each one equals
  `t_auth + W`, where `t_auth` is the clock reading taken before that chunk's intent batch was
  committed. The wrapper advances the clock right after the intent commit returns (a delayed
  dispatch), and the deadline is still `t_auth + W`: never `t_dispatch + W`, never `None`, and
  never an unbounded value such as `u64::MAX`.
  **(B) The drain fence.**
  - **B1:** the wrapper writes `desired:dserver:<S>`, for a server `S` the plan selected, **just
    before** that chunk's intent commits. The intent must fail and re-plan. Staging still
    completes, and no `sidx:` entry or fragment of the part names `S`. Removing the per-server
    `require_absent(desired:dserver:<S>)` from the intent batch makes B1 fail.
  - **B2:** `desired:dserver:<S>` is present **before** staging, and `S` is the server the
    selector prefers (lowest utilization). Staging completes, nothing names `S`, and the wrapper
    sees **no** intent conflict. Planning against the full topology instead of the topology minus
    draining servers makes B2 fail.
  - **B3:** with 4 of 12 single-server domains draining, and a scheme that fits in the remaining
    8 (e.g. RS(4,2)), a part stages. With too few non-draining domains for the scheme, the answer
    is a typed placement error, never `NoSuchUpload` for an `Open` session.
  - The drain key keeps the custodian's exact bytes, `desired:dserver:<id>`
    (`crates/custodian/src/desired_state.rs:35-40`).
  **(C) Slots.**
  - `MAX_INFLIGHT_PARTS` reserves (`multipart.rs:5032`) succeed on one session, and the next
    answers `Refused(SlowDown { pressure: Backpressure::InflightParts { .. } })`
    (`multipart.rs:4470`, `:4528`).
  - A reserve against a non-`Open` session answers `Refused(NoSuchUpload)` and writes nothing.
  - A reserve whose batch lands but answers `CommitUnknownResult` adopts its own slot on re-read
    (same `attempt_id`). In this schedule, which leaves no residue, the session's held-slot count
    equals the number of live attempts (`0016:1779`).
  - **The same-index rule is shown by a schedule that tells it apart from "lowest free index".**
    Index 0 is held by another attempt X. This reserve takes index 1, and its batch is held
    (`may_still_commit: true`, absent on re-read). X releases index 0 before the retry. The retry
    must go to index **1** again with the same `attempt_id` (`0016:1707-1716`). Releasing the
    held batch then leaves exactly **one** slot of this attempt. A reserve that probes the lowest
    free index after an ambiguous outcome takes index 0 here and ends with two slots, so this leg
    fails on it.
  - After a bounded number of ambiguous rounds the reserve refuses with `SlowDown`
    (`0016:1716-1717`). The number of reserve commits equals the stated bound. **Then the held
    reserve lands after the refusal** (0016 permits this, `:1716-1717`): with
    `MAX_INFLIGHT_PARTS − 1` other slots already held, the wrapper releases the held batch.
    Exactly **one** slot of the refused attempt exists, never two; no live attempt owns it; and
    it counts toward the cap, so the next reserve answers
    `Refused(SlowDown { pressure: Backpressure::InflightParts { .. } })` (residue is counted on
    purpose, `0016:1839-1842`, `multipart.rs:4467-4469`). This slice adds no cleanup for it: the
    reaper reclaims it with the session (F11a).
  **(D) Renewal.**
  - Every renewal batch rewrites the attempt's own `slot:` record in the same batch as its owned
    `sidx:` leases (`0016:349`, `:659`).
  - **Renewal runs while the body is still arriving, before the first chunk too.** A body
    delivered in small pieces, with the clock advanced past half a lease TTL between them and
    before the first chunk boundary, gets its slot lease moved forward before the first intent.
  - **Renewal does not wait for I/O to return** (2026-10-20). Renewal checked only between
    delivered pieces cannot run while a body read or a fragment write is still pending, and either
    can stay pending past the lease. So renewal is driven by a timer the caller supplies (see
    Design § Renewal), and the test drives that timer by hand. (D-i) **Body withheld:** the body
    is a channel the test feeds; after the slot is reserved the test withholds the next piece,
    advances the clock past half a TTL and fires the timer; a renewal batch rewriting the slot
    commits **while the read is still pending**. Then the piece is delivered and staging finishes
    normally. (D-ii) **Fragment write delayed:** the chunk-store double holds one fragment put of
    chunk 2 (after chunk 1's owned entry is durable); the test advances the clock and fires the
    timer; a renewal batch rewriting the slot and chunk 1's and chunk 2's owned leases commits
    **while the put is still pending**. Then the put is released and staging finishes normally,
    and that put is not re-sent. In both, neither the slot lease nor any owned lease is ever
    observed lapsed while the client is still supplying data (`0016:1824-1837`).
  - **A lapsed lease is refused, never resurrected.** With the clock advanced past the slot's or
    an owned entry's lease before the next renewal, the renewal refuses, writes nothing, and
    staging stops with a typed error (`0016:1843-1847`; the peer rule at
    `crates/core/src/write.rs:496-517`).
  - A renewal after the session leaves `Open` refuses and resurrects nothing.
  **(E) Unknown outcomes on the intent and the renewal are settled by a re-read, never guessed.**
  - An intent whose batch lands but answers `CommitUnknownResult` is recognised by re-reading
    the owned entry's exact bytes. Staging goes on without a second entry for that chunk, and its
    fragments go to the placement that landed.
  - An intent answered `CommitUnknownResult { may_still_commit: false }` and not applied is
    retried, and staging ends with exactly one owned entry per chunk.
  - An intent held with `may_still_commit: true`: no fragment of that chunk is written on the
    strength of an absent read. Every later batch of the attempt requires its own slot at the
    bytes the held batch also requires, so at most one of them lands. In two runs, (i) the held
    batch released before the attempt's next batch and (ii) every later batch answered `Conflict`
    unapplied until the rounds run out and then the held batch released, two things hold: every
    fragment written is named by a durable owned entry with that fragment's placement, and in
    (ii) staging returns an in-doubt error that keeps the `may_still_commit` doubt, as Abort's
    does (`crates/core/src/multipart/abort.rs:264-274`).
  - A renewal answered `CommitUnknownResult` is settled by re-reading its own slot's bytes, with
    the same two cases (landed; not landed with `may_still_commit: false`).
  **(F) Every loop is bounded.** The reserve probe, the ambiguous reserve, the intent re-plan and
  the renewal each have a stated maximum. With a wrapper that answers every batch `Conflict`
  without applying it, each loop ends in a typed error after **exactly** that many commits (the
  test counts them).
  **(G) The per-part chunk bound.** A body longer than `MAX_PART_CHUNKS` chunks
  (`multipart.rs:4972`) never registers more than `MAX_PART_CHUNKS` owned entries for its part.
  Staging stops and reports the overrun, and its caller receives the staged attempt intact, so
  the commit slice can refuse and compensate it. Use a test-sized bound only if the function
  takes it as a parameter.
  **(H) The whole gate.** `cargo xtask ci` is green on the patched tree, the tracker's acceptance
  line and `AGENTS.md:41-45`'s rule for any change that can affect the Rust build. This slice
  also edits `wyrd-custodian` and the architecture docs, which `cargo test -p wyrd-core` does not
  cover.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and the wrapper forces every red leg rather than leaving it to timing. B1's drain record is
  written right before the intent commits. (C)'s and (E)'s unknown and held outcomes are
  injected. (C)'s same-index leg frees index 0 by hand between the held batch and the retry.
  (A′)'s and (D)'s clocks are test-controlled, and (D)'s renewal timer is fired by hand while a
  body piece or a fragment put is held. (F)'s conflicts are scripted. (H) is the gating
  `C4-ci` check, which runs `cargo xtask ci`. In the discontinued #636 patch both fence mutants
  (deleting the `require_absent` loop; replacing `excluding(draining)` with `clone()`) survived
  the whole suite. In #657's first attempt, removing the same-index pin survived, because its
  test began with every slot free. B1, B2 and the (C) schedule exist to kill exactly those.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the staging side. Every staged fragment is named by a durable
  owned `sidx:` record, with its placement, from **before** it is written. No owned record can
  be created once the session has left `Open`, or naming a server whose drain is recorded. A
  live request's slot and owned leases do not lapse while it is supplying data, and a lapsed one
  is never revived. An `Open` session is never refused as `NoSuchUpload`. No attempt ever holds
  more than one slot; a slot an ambiguous reserve installs after its attempt refused, or a
  crashed attempt leaves, is residue that counts toward the cap until the reaper reclaims it with
  the session (`0016:1716-1717`, `:1839-1842`; `multipart.rs:4467-4469`), so held slots equal
  live attempts only in a schedule with no such residue. **Sources:** the maintainer's rule 2026-07-25 (via
  principles.md §5); 0016's refutation standard (`0016:2809-2813`); decision 5's properties
  (`0016:1529-1545`), the drain fence (`0016:353`, `:659`) and the reserve rule
  (`0016:1699-1719`). SELF-TEST: it binds the reserve, intent and renewal batches and the
  placement step, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** First of the split; 657.2 (the commit) depends on it. #656 (Create + Abort,
  merged as #894 + #895) is on `main` already. **No `Conflicts with` #653, on purpose:** #653
  edits neither `crates/core/src/metadata.rs` nor
  `docs/design/architecture/05-building-block-view.md`. Do not touch `crates/core/src/lib.rs` or
  `docs/design/architecture/08-crosscutting-concepts.md` (#653's). Intake: created by the split
  of #657 under the human's per-id cap override of 2026-10-20 (alpha 7/6 before it).
- **Difficulty:** high
- **Do model:** opus
- **Scope:** the first half of `UploadPart` as store-I/O functions in `wyrd-core`. That means
  the slot reserve; the per-chunk owned intent; staged placement against the topology minus
  draining servers, with re-plan on a drain-fence conflict; the fragment writes with their
  authorization deadline; in-flight renewal of the slot and owned leases for the whole request,
  driven by a caller-supplied wait so it runs while a body read or fragment write is pending;
  the bounded per-part chunk count; and the staged attempt handed to the commit. Also: one home
  in core for the `desired:dserver:<S>` key, so core and the custodian spell it once; the
  writer-side constructor the slot record lacks; the docs sentence; and the new test file. / out
  of scope:
  - the part commit, its outcome classification, live-session compensation, re-upload, the
    refusal of an over-limit part and the cumulative `MAX_STAGED_CHUNKS` ceiling, and the
    composed `UploadPart` (all 657.2);
  - Complete, the segment committer and publication (#658, #653); draining any `retire:`
    obligation, the owned-`sidx:` walk and the terminal delete (#659); the reaper (#625);
    seeded DST races (#660, #665);
  - the D-server side of the `W_write` deadline (built by #638; this slice only passes the
    deadline on every staged write);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's);
    any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). Today `write::intent` writes only ordinary
  `pending:` entries with `owner: None, staged: None` (`crates/core/src/write.rs:201-219`), and no
  code writes a `slot:` or `sidx:` record.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_stage.rs`, a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator; confirmed with
  `run-verify.sh --classify`).
- **Verification posture:** (a) net-new. The staging functions do not exist on `main`, so the
  test cannot compile on C4-verify's reverted base, and C4-verify reports **UNVERIFIABLE** (exit
  77), a pre-declared sign-off item.
  **Built and exercised at Check:** the reserve, intent, placement, fragment-write and renewal
  paths, driven by the new test over a real store and a chunk-store double.
  **Demonstrated reds (required, recorded in `build-notes.md` with the exact edit and the name
  of the test that failed).** Do makes each temporary edit, shows the named leg fail, then
  restores the file. Each red must come from the edit its label names:
  - the per-server `require_absent(desired:dserver:<S>)` removed from the intent batch → B1;
  - planning against the full topology instead of the topology minus draining servers → B2;
  - the slot rewrite dropped from the renewal batch → (D);
  - renewal only at chunk boundaries (none while the body is still arriving) → (D);
  - renewal checked only between delivered pieces, with no timer → (D-i);
  - the timer not raced against a pending fragment put → (D-ii);
  - the hand-off's last `ChunkRef` given `len == chunk_size` instead of the remainder → (A);
  - after an ambiguous reserve refuses, a probe of a fresh index for the same attempt → (C)'s
    landed-after-refusal leg (two slots of one attempt);
  - renewal that moves a lapsed lease forward instead of refusing → (D);
  - the same-index pin removed after an ambiguous reserve (lowest free index taken) → (C);
  - after an unknown intent outcome, an absent read taken as "did not land" with
    `may_still_commit: true` → (E);
  - a loop counter that never reaches its bound (e.g. `rounds += 1` made `rounds *= 1`) → (F);
  - a staged fragment put with `None`, with `Some(u64::MAX)`, or with its deadline taken at
    dispatch (three edits) → (A′);
  - a body piece that spans a chunk boundary merged into one oversize chunk → (A).
  **C5 (advisory) survivors:** `build-notes.md` lists every mutant `cargo mutants --in-diff`
  leaves alive in this slice's code, each with the test that now kills it or why it is
  equivalent.
- **Production reach:**
  - (a) The test exercises staging for real against a real store. Production stages no part in
    this slice.
  - (b) 657.2 consumes the staged attempt; the S3 verb lands with #669 under #508, after
    #658/#659 and the reaper (#625), per 0016's same-release rule (`0016:236-268`).
  - (c) This slice **must not** add a production caller.
  - (d) The one existing production path it may touch is the custodian's use of the
    `desired:dserver:` key, which must keep the same bytes.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer callsite for the per-chunk loop:** `lease_write_chunk` and `stream_write_data`
    (`crates/core/src/write.rs:485-535`, `:559`). Lease (here: owned intent) before writing a
    chunk's fragments, renew half a TTL ahead, and **refuse rather than resurrect** a lapsed
    lease (`:496-517`). Encode a chunk as that path does (`encode_chunk`, `write.rs:138`,
    private today). Plan placement as `WritePlan::place` (`write.rs:112`) does, over
    `Topology::excluding` (`crates/core/src/placement.rs:147`).
  - **Peer for settling an unknown outcome:** #895's Abort, `crates/core/src/multipart/abort.rs:282`.
    It re-reads after `Conflict`, never reads `CommitUnknownResult` as `Conflict`, and keeps a
    `may_still_commit: true` doubt (`:264-274`).
  - **The drain-fence key:** today only `crates/custodian/src/desired_state.rs:35-40`
    (`DESIRED_PREFIX`, `desired_key`). Core cannot depend on the custodian (ADR-0010), so the key
    gets one home in core, the custodian uses it, and its bytes do not change. The custodian's
    `deferred: #657` notes (`desired_state.rs:226-230`, `:246-248`) are then settled: update them
    to say the intent writer now fences (the seeded interleaving stays `deferred: #665`).
  - **Records and keys:** `slot_key` / `slot_range` (`multipart.rs:1251`, `:1261`), `sidx_key` /
    `sidx_range` (`:1328`, `:1339`), `SlotRecord` (`:2719`), `StagedPlacement` (`:4045`),
    `OwnedEntry` (`:4136`), `AttemptId` (`:891`), `SlotIndex` (`:1000`), `upload_part_answer`
    (`:4756`), `MAX_INFLIGHT_PARTS` (`:5032`), `MAX_PART_CHUNKS` (`:4972`), `Digest`
    (`:1036-1052`). `SessionRecord::open` is the model for a crate-private writer-side
    constructor (see its doc, `:2323-2328`).
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/write.rs`,
  `crates/core/src/multipart.rs`, `crates/core/src/multipart/`, `crates/core/src/metadata.rs`,
  `crates/custodian/src/desired_state.rs`, `crates/core/tests/`,
  `docs/design/architecture/05-building-block-view.md`) on `origin/main` @ `12dd082`. Merged: the
  record family and decoders (#654, via PRs #703, #725, #734, #793), knobs (#801), Create
  (#908), Abort (#909), the custodian's staged protection and drain counting (#807, #822).
  Rejected or iterated: #636's single-slice attempt (discontinued 2026-10-10: both fence mutants
  survived, and an `Open` session was answered `NoSuchUpload` with 4 of 12 D-servers draining);
  #657's first attempt (iterated to Plan 2026-10-19, oversized). Its staging-side defects are
  this slice's (D) and (C) legs: no renewal while the body was still arriving, renewal that
  revived a lapsed lease, and a same-index test that could not tell pinning from "lowest free
  index". The new test file does not exist. Open PRs on getwyrd/wyrd touching these paths: none
  (2026-10-20).
- **Disposition hint:** new-feature

## Design

**Reserve** (`0016:349`, `:658`, `:1676-1719`). Read the bounded `slot:<id>:` range and the
session. Refuse `NoSuchUpload` unless the session is `Open@E` (`upload_part_answer`). Pick a free
index `k`, and commit `require(mpu == Open@E)` + `require_absent(slot:<id>:<k>)` + the slot record
(`part_number`, `attempt_id`, `reserved_at_millis`, `lease_expiry_millis`). A lost index probes
the next free one, at most `MAX_INFLIGHT_PARTS` probes; a full range is `SlowDown { InflightParts }`.
On `CommitUnknownResult`, re-read `slot:<id>:<k>`. Its own `attempt_id` means adopt it. With
`may_still_commit: false`, anything else means it did not land. With `may_still_commit: true`,
absence proves nothing, so retry the **same** index with the same `attempt_id`, and refuse after
a bounded number of ambiguous rounds.

**Intent per chunk** (`0016:353`, `:659`). Plan the chunk's placement against the topology minus
the servers with a `desired:dserver:` record (read from the store). Then commit one batch:
`require(mpu == Open@E)`; `require_absent(desired:dserver:<S>)` for **every** server in the
placement; `require(slot:<id>:<k> == prior)`; the owned `sidx:<id>:<part>:<chunk>` entry
(`OwnedEntry` with the planned `StagedPlacement`); and a rewrite of the slot (its lease moved
forward). A `Conflict` re-reads. Session not `Open` ⇒ stop with `NoSuchUpload` and leave the
residue to the reaper. A drain record now present ⇒ re-plan without that server and try again.
Only then write the fragments, each carrying its authorization deadline (`put_fragment_at`'s
`deadline_millis`; the D server refuses a late write, #638). The deadline is `t_auth + W_write`,
where `t_auth` is the clock reading taken **before** the intent batch is committed (so the
deadline can only be early, never late), and every fragment of the chunk carries that same value
however late it is dispatched. `W_write` is `W_WRITE_MILLIS` (`crates/custodian/src/gc.rs:202`),
which core cannot import (ADR-0010), so the caller supplies it, as it supplies the clock. The
function must not substitute an unbounded or default deadline. The orphan grace is sized on that
bound (`G_orphan > W_write + δ_clock`, `0016:1567-1578`); an unbounded staged write would let a
straggler land unevidenced.

**Renewal** (`0016:1824-1847`). Half a TTL ahead, as `lease_write_chunk` does, one batch renews
every owned `sidx:` lease of this attempt **and** rewrites its slot, under `require(mpu ==
Open@E)`. It runs for the whole request, including while the body is still arriving and before
the first chunk is complete: the slot lease is the only liveness evidence a request has before its
first intent (`0016:1824-1837`). A renewal that finds a lease of its own already lapsed refuses
(`0016:1843-1847`); it never moves a lapsed lease forward. A renewal that fails because the
session left `Open` stops the upload and recreates nothing. The code is runtime-free (ADR-0009),
so it spawns nothing; and checking for renewal only between body polls is not enough, because a
body read (`source.next()`, as `write.rs:583` awaits it) or a fragment write (`:592-604`) can stay
pending past the lease. **So the staging functions take a caller-supplied wait**, the shape of
`CreateRuntime::wait` (`crates/core/src/multipart/create.rs:81-82`; core's existing seam for
time passing, since `Clock` only reads time), and each pending body read or fragment write is
raced against a wait for the next renewal instant. When the wait wins, the renewal batch is sent
and the pending read or write keeps going: it is resumed, never dropped and re-sent. Renewal is
never sent while the attempt's own intent batch is in flight (both rewrite the slot under
`require(slot == prior)`, so they would only conflict with each other).

**Settling an unknown outcome** (intent and renewal). Re-read and compare exact bytes: the owned
entry this intent built, or the slot bytes this renewal built. Equal ⇒ it landed. Anything else
proves "did not land" **only** when `may_still_commit` is `false`
(`crates/traits/src/lib.rs:240-247`). With `true`, keep the doubt the way #895's Abort does:
every later batch requires its own slot at the bytes the held batch also requires, and the held
batch rewrites that slot, so whichever lands first excludes the other. Bound the rounds. If they
run out unsettled, return an in-doubt error carrying the doubt, never a guess. No fragment is
written for a chunk whose owned entry has not been observed durable.

**The hand-off to the commit (657.2).** Staging returns a public value the commit can build its
batch, or its compensation, from **alone**, without re-reading the body or re-deriving anything:
the session bytes it was fenced on (`Open@E`); the slot index and the slot's current bytes; each
owned entry's key, current bytes and placement, in chunk order; **one complete `ChunkRef` per
chunk, in chunk order** (`id`, `scheme`, `len`, `placement`; the last chunk's `len` is the short
remainder), which is exactly the list `PartRecord` stores; the part number, length and
digest; whether the per-part bound was hit; and the means to run one more renewal (the commit's
quiesce step, `0016:1720-1733`). Make it public: 657.2's tests and #658's tests call it from
`crates/core/tests/`.

**Writer constructor.** The slot record has no writer-side constructor today. Add a
crate-private one in the shape of `SessionRecord::open`, so every slot value written is one
`decode` accepts back unchanged.

**Docs currency** (`AGENTS.md:154-157`). Update the multipart records paragraph at
`docs/design/architecture/05-building-block-view.md:244`, which says the types are "landed ahead
of most of their writers". The new text: `slot:` and `sidx:` now have their writer; staging
registers each chunk before its bytes are written, in a batch that fails once the session has
left `Open` or a selected server's drain is recorded; and nothing calls it in production until
#669.

## Alternatives considered

- **Refuse the part when a selected server starts draining.** That makes routine operator
  action a client-visible failure across the fleet (the discontinued patch's `NoSuchUpload` with
  4 of 12 draining). Rejected; re-plan.
- **Renew only at chunk boundaries.** A slow client that has not filled its first chunk then
  holds a slot whose lease lapses, and an idle reaper can abandon a live session
  (`0016:1824-1837`). Rejected.
- **Owned entries under `pending:`.** Every global `pending:` scan would enumerate them
  (`0016:1773`). Rejected; `sidx:` is disjoint.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-20): all three findings hold at `12dd082` and are revised in place.
(1) Slots: the invariant now says no attempt holds two slots and that residue counts toward the
cap (`0016:1716-1717`, `:1839-1842`); equality with live attempts is claimed only for a schedule
with no residue. (C) adds the held reserve landing after the refusal: one slot, counted toward
the cap, and no cleanup in this slice. (2) Renewal: the staging functions take a caller-supplied
wait shaped like `CreateRuntime::wait` and race it against pending body reads and fragment
writes without dropping them. (D-i) and (D-ii) check that a renewal commits while a body piece
is withheld and while a fragment put is held. (3) Hand-off: it now carries one complete
`ChunkRef` per chunk in order, including the short last `len`, and (A) checks every field.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix the confirmed lease/authority bugs; keep the slice as is (no split). - Intent must check the slot lease and every earlier owned lease at its fresh t_auth before EACH submission (first round and every retry, including after the drain scan); refuse a lapsed lease, keeping any outstanding may_still_commit doubt. Add tests that lapse the lease during the drain scan and between intent rounds. - Renewal retries must re-read the clock and re-run check_live before each new submission, while still settling batches already sent. Add a conflict/hold schedule that crosses expiry between renewal rounds. - StagedAttempt::renew must take the clock/runtime, not a fixed now_millis, and re-check liveness per round (657.2 builds on this signature). - Intent recovery: if the fresh slot read matches the bytes of an intent this attempt already sent, settle that intent (re-read sidx:) instead of returning SlotLost. Test a held intent landing between the sidx: read and the slot read. - Reserve: don't clear the pinned index after an absent read followed by a newer session epoch. Re-read and settle the original index first (stage.rs:832), and add that interleaving to the tests. - Minor: d_a_lapsed_slot_lease test should drop tx inside the driver so a regression fails fast instead of hanging until LEG_TIMEOUT.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Fix the confirmed lease/authority bugs; keep the slice as is (no split).
  - Intent must check the slot lease and every earlier owned lease at its fresh t_auth before EACH submission (first round and every retry, including after the drain scan); refuse a lapsed lease, keeping any outstanding may_still_commit doubt. Add tests that lapse the lease during the drain scan and between intent rounds.
  - Renewal retries must re-read the clock and re-run check_live before each new submission, while still settling batches already sent. Add a conflict/hold schedule that crosses expiry between renewal rounds.
  - StagedAttempt::renew must take the clock/runtime, not a fixed now_millis, and re-check liveness per round (657.2 builds on this signature).
  - Intent recovery: if the fresh slot read matches the bytes of an intent this attempt already sent, settle that intent (re-read sidx:) instead of returning SlotLost. Test a held intent landing between the sidx: read and the slot read.
  - Reserve: don't clear the pinned index after an absent read followed by a newer session epoch. Re-read and settle the original index first (stage.rs:832), and add that interleaving to the tests.
  - Minor: d_a_lapsed_slot_lease test should drop tx inside the driver so a regression fails fast instead of hanging until LEG_TIMEOUT.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 121 mutants tested in 19m: 82 caught, 32 unviable, 7 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix the confirmed deadline bug; keep the slice as is (size warning waived by the human again: most of the patch is the test file). - Renewal must not block the fragment-write deadline (stage.rs:1334). Race the renewal against the pending work and the deadline instead of awaiting renew_if_due first; a write answered in time during a renewal must not be discarded as WriteDeadlineElapsed. If the deadline cuts off a submitted renewal, report its outcome as unknown (keep the may_still_commit doubt). Add the reviewers' schedule as a test: hold a fragment put and a renewal commit, advance past the fragment deadline, assert staging ends before the renewal is released; plus the adversary's case (put answered in time while a slow renewal commits must succeed). - Body-read timeout: core takes a body-idle bound now (a caller-supplied parameter, no runtime spawn), so a stalled client cannot hold its slot and leases forever. Test it with the hand-driven timer. - Proposal 0016 :1714 (after an ambiguous reserve, "probes onward only once it reads that index carrying another attempt's id"): the human authorizes editing 0016 in this slice, overriding the brief's "no edit to proposal 0016" scope line. Change the text to match the code's safe reading (refuse SlowDown rather than probe onward, since the held batch's require_absent can still pass if the other attempt releases the index).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Fix the confirmed deadline bug; keep the slice as is (size warning waived by the human again: most of the patch is the test file).
  - Renewal must not block the fragment-write deadline (stage.rs:1334). Race the renewal against the pending work and the deadline instead of awaiting renew_if_due first; a write answered in time during a renewal must not be discarded as WriteDeadlineElapsed. If the deadline cuts off a submitted renewal, report its outcome as unknown (keep the may_still_commit doubt). Add the reviewers' schedule as a test: hold a fragment put and a renewal commit, advance past the fragment deadline, assert staging ends before the renewal is released; plus the adversary's case (put answered in time while a slow renewal commits must succeed).
  - Body-read timeout: core takes a body-idle bound now (a caller-supplied parameter, no runtime spawn), so a stalled client cannot hold its slot and leases forever. Test it with the hand-driven timer.
  - Proposal 0016 :1714 (after an ambiguous reserve, "probes onward only once it reads that index carrying another attempt's id"): the human authorizes editing 0016 in this slice, overriding the brief's "no edit to proposal 0016" scope line. Change the text to match the code's safe reading (refuse SlowDown rather than probe onward, since the held batch's require_absent can still pass if the other attempt releases the index).
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: The slice is too big: 271 KB against a 100 KB limit, three rounds, and implementation-level findings every round. Split it in the re-plan (`pdca split 926`, then `--accept`) instead of rebuilding it whole again. The staging core held up (CI green, 96.4% diff coverage, every mutant caught, including the adversary's 33 hand-made mutants), so keep its design. Carry these confirmed findings into whichever child brief owns the code: - Test double: the Fleet fake must refuse a fragment put published after its deadline (`WriteDeadlineExpired::if_elapsed` just before insert). The `:1589` renewal hook must wait for the store to finish before moving the clock. Add a leg showing a held put released after expiry is refused and not published. - Body-idle bound: an empty body piece must not re-arm the idle deadline (stage.rs:1392-1401); measure idle from the last non-empty piece. Add a leg with only empty pieces. - Refusal answers: add a stage_part leg with every MAX_INFLIGHT_PARTS slot held, expecting SlowDown{InflightParts}, and a reserve leg where the session leaves Open between the read and the commit, expecting NoSuchUpload, plus a pinned variant (stage.rs:985, :998, :1115). - Renewal batch size: the 3 + 2N ops renewal batch has no limit against MAX_BATCH_OPS; add a `2*max_part_chunks + 3 <= batch_ops` check (or reject oversize knobs). The re-plan decides whether this goes in a child or becomes a follow-up against #508/#625.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The slice is too big: 271 KB against a 100 KB limit, three rounds, and implementation-level findings every round. Split it in the re-plan (`pdca split 926`, then `--accept`) instead of rebuilding it whole again. The staging core held up (CI green, 96.4% diff coverage, every mutant caught, including the adversary's 33 hand-made mutants), so keep its design. Carry these confirmed findings into whichever child brief owns the code:
  - Test double: the Fleet fake must refuse a fragment put published after its deadline (`WriteDeadlineExpired::if_elapsed` just before insert). The `:1589` renewal hook must wait for the store to finish before moving the clock. Add a leg showing a held put released after expiry is refused and not published.
  - Body-idle bound: an empty body piece must not re-arm the idle deadline (stage.rs:1392-1401); measure idle from the last non-empty piece. Add a leg with only empty pieces.
  - Refusal answers: add a stage_part leg with every MAX_INFLIGHT_PARTS slot held, expecting SlowDown{InflightParts}, and a reserve leg where the session leaves Open between the read and the commit, expecting NoSuchUpload, plus a pinned variant (stage.rs:985, :998, :1115).
  - Renewal batch size: the 3 + 2N ops renewal batch has no limit against MAX_BATCH_OPS; add a `2*max_part_chunks + 3 <= batch_ops` check (or reject oversize knobs). The re-plan decides whether this goes in a child or becomes a follow-up against #508/#625.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
