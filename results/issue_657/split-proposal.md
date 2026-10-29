<!-- pdca:split-proposal v1 -->
# Split proposal — issue 657

**Intake-cap override (wyrd-pdca-P1), 2026-10-20.** `scripts/plan-cap --track alpha --need 2`
answered `planned 7/6 (cap, track alpha) — room for 0, need 2: Plan intake closed` (PLANNED 508
625 633 658 659, BUILT 654, ITERATE_DO 653). The human said in this Plan session, in answer to
the cap refusal, "split it please": the override covers **this split of #657 only**. Accepting it
takes alpha to 9/6 while the two children are open. Why it was granted: #657 blocks five of the
seven counted alpha bundles (658, 625 and 508 directly; 659 and 633 through 658), so alpha's count
cannot fall while #657 is stuck.

## Why this slice is oversized

The 2026-10-19 sign-off sent #657 back to Plan (`iterated-to-Plan`). Its one build was a 196 KB
patch (threshold 100 KB) of about 3,700 added lines: 1,601 in the new staging module and 2,088 in
its test file. That patch also carried four reproduced defects and 31 surviving mutants. Two
outcomes were sharing one slice, and they split cleanly at the commit batch:

1. **Getting bytes in safely: staging.** Reserve an in-flight slot. Stream the body into chunks,
   registering each one as an owned `sidx:` entry, fenced on the session and on the drain ledger,
   before any of its fragments is written. Keep the slot and owned leases alive for as long as
   the request is live. Nothing in this half writes a `part:` record. Its correctness questions
   are admission, the drain fence, the write deadline and lease liveness.
2. **Turning staged bytes into a part, or into a retirement: commit.** Write the `part:` +
   `psum:` pair, release the slot and delete the `sidx:` entries in one batch. Classify a
   `Conflict` or unknown outcome from a re-read. Compensate a loser, handle a re-upload and the
   ceilings, and compose the whole `UploadPart`. Its correctness question is attribution:
   answering `Committed` only for a commit that is this attempt's own. Three of the four
   reproduced defects sit here.

The cut needs one explicit hand-off: staging returns everything the commit needs to build its
batch, or its compensation, without going back to the body. Each child ships its **own new test
file**. This instance's C4-verify counts only an **added** `*/tests/*.rs` as the red→green test
(`engine/scripts/run-verify.sh:141-144`); a `--classify` dry run on patches shaped like each
child returned `ADDED_TEST` for both files below.

## Wave sketch

Two waves, strictly stacked. The commit consumes the staged attempt, so `child-2` `Depends on`
`child-1` and builds on its folded result (`$PDCA_VERIFY_BASE`, which `run-verify.sh:264-265`
honours). Neither child conflicts with #653, the other wave-1 bundle of the 2026-10-19 batch:
#653 edits `crates/core/src/lib.rs`, `crates/core/src/staged_publication.rs`, its two test files
and `docs/design/architecture/08-crosscutting-concepts.md`, and both children are barred from
those files. #656, which the parent depended on, is COMPLETE (merged as #894 + #895). It is not a
sibling, so it cannot appear in a child's `Depends on`; both children build on `main`, which
already carries it.

**Dependents of #657 must be re-pointed after acceptance.** The scheduler does not move a split
parent's dependents onto its children (`src/pdca_harness/flow.py:1616-1620`). A split parent ends
COMPLETE, so `Depends on: 657` would be met at once, and #658, whose tests stage parts through
#657, could build before any staging code exists. On acceptance, replace `657` with both child
ids in the `Depends on` of #658, #625 and #508. Strictly only #658 needs it (#625 and #508 also
depend on #658), but a field naming a finished split parent misleads its next reader.

<!-- pdca:child child-1 -->
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
  fragment put of that chunk (`0016:1788`).
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
    (same `attempt_id`). The session's held-slot count equals the number of live attempts
    (`0016:1779`).
  - **The same-index rule is shown by a schedule that tells it apart from "lowest free index".**
    Index 0 is held by another attempt X. This reserve takes index 1, and its batch is held
    (`may_still_commit: true`, absent on re-read). X releases index 0 before the retry. The retry
    must go to index **1** again with the same `attempt_id` (`0016:1707-1716`). Releasing the
    held batch then leaves exactly **one** slot of this attempt. A reserve that probes the lowest
    free index after an ambiguous outcome takes index 0 here and ends with two slots, so this leg
    fails on it.
  - After a bounded number of ambiguous rounds the reserve refuses with `SlowDown`
    (`0016:1716-1717`). The number of reserve commits equals the stated bound.
  **(D) Renewal.**
  - Every renewal batch rewrites the attempt's own `slot:` record in the same batch as its owned
    `sidx:` leases (`0016:349`, `:659`).
  - **Renewal runs while the body is still arriving, before the first chunk too.** A body
    delivered in small pieces, with the clock advanced past half a lease TTL between them and
    before the first chunk boundary, gets its slot lease moved forward before the first intent.
    Neither the slot lease nor any owned lease is ever observed lapsed while the client is
    still supplying data (`0016:1824-1837`).
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
  (A′)'s and (D)'s clocks are test-controlled. (F)'s conflicts are scripted. (H) is the gating
  `C4-ci` check, which runs `cargo xtask ci`. In the discontinued #636 patch both fence mutants
  (deleting the `require_absent` loop; replacing `excluding(draining)` with `clone()`) survived
  the whole suite. In #657's first attempt, removing the same-index pin survived, because its
  test began with every slot free. B1, B2 and the (C) schedule exist to kill exactly those.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the staging side. Every staged fragment is named by a durable
  owned `sidx:` record, with its placement, from **before** it is written. No owned record can
  be created once the session has left `Open`, or naming a server whose drain is recorded. A
  live request's slot and owned leases do not lapse while it is supplying data, and a lapsed one
  is never revived. An `Open` session is never refused as `NoSuchUpload`, and the session's held
  slots never outnumber its live attempts. **Sources:** the maintainer's rule 2026-07-25 (via
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
  authorization deadline; in-flight renewal of the slot and owned leases for the whole request;
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
session left `Open` stops the upload and recreates nothing. The code is runtime-free (ADR-0009):
renewal happens inline, between body polls, not in a spawned task.

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
owned entry's key, current bytes and placement, in chunk order; the part number, length and
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core: UploadPart commit — part record, outcome classification, compensation, re-upload, ceilings (657.2)

> Second of the two slices #657 was split into on 2026-10-20. It builds on 657.1 (staging),
> which hands it a staged attempt: a held slot and one owned `sidx:` entry per chunk. Do reads
> ONLY this file. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read these before writing
> code: the `part:` and `psum:` rows of the record table (`0016:351-352`); the **Part commit** and
> **Live-session compensation** rows of the batch inventory (`0016:660`, `:673`); and decision 5's
> commit classification (`0016:1584-1653`) and quiesce rule (`0016:1720-1733`). Citations were
> verified against `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-20; 657.1's own code is
> cited by what it does, since it lands in the wave before this one.

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
    chunks, its slot released, and its `sidx:` entries deleted, all in one batch. It answers a
    refusal, never `Committed` and never `NoSuchUpload`, because the session is `Open`.
  - A same-part rival committed first: the loser commits one `retire:bytes:` obligation for its
    **own** chunks (it must decode under `decode_retire_obligation`, `multipart.rs:3891`),
    releases its slot and deletes its `sidx:` entries in one batch, and answers a refusal.
  - A commit whose batch lands but answers `CommitUnknownResult`: the attempt re-reads, finds its
    own commit, answers `Committed`, and its slot is released exactly once.
  - A re-upload whose batch did not land (`may_still_commit: false`): the re-read finds the
    **prior** part. The attempt must not answer `Committed` for it; it carries on as for a commit
    that did not land, and ends with its own part committed.
  - **A held batch** (`may_still_commit: true`; the wrapper holds it and lands it later): the
    re-read finds the prior part, or none. The attempt neither answers `Committed` nor
    compensates on that read alone. Every batch it sends next requires its own slot at the bytes
    the held batch also requires, so at most one of them lands. Run it three times: (i) the held
    batch released before the attempt's next batch; (ii) released after the attempt has
    returned; (iii) every later batch answered `Conflict` unapplied until the rounds run out, then
    the held batch released. In every run: exactly one of {its own part, its compensation} is
    durable at the end; the slot is released exactly once; and the answer is `Committed` only if
    its own part is the one that landed. In (iii) the attempt returns an in-doubt error that keeps
    the `may_still_commit` doubt, as Abort's does (`crates/core/src/multipart/abort.rs:264-274`).
  - A same-number rival plus an unknown outcome: the re-read finds the rival's record. Never
    `Committed`; the attempt compensates under its own slot.
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
    its commit released both.
  - An unknown outcome on the **compensation** batch (landed; not landed with
    `may_still_commit: false`; held) is settled by re-reading its own obligation. Each run ends
    with exactly one obligation and the slot released once, or an in-doubt error carrying the
    doubt.
  - Every loop of this slice (commit retry, compensation, the epoch chase after a rollback) has a
    stated maximum. With a wrapper that answers every batch `Conflict` unapplied, each ends in a
    typed error after **exactly** that many commits (the test counts them).
  **(D) Re-upload and ceilings.**
  - Re-uploading a committed part number replaces it under `require(part == prior)` and puts the
    prior part's chunks in a `retire:bytes:` obligation in the same batch.
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
  by hook; C3's rival lands by hook between the commit and the re-read; the overshoot witness
  holds one commit by hand. (E) is the gating `C4-ci` check. In #657's first attempt, C1, C2 and
  C3 were each reproduced against the patched tree by independent probes, and the rollback path
  and the bounded rounds had no test that could fail.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the commit side. Every staged fragment stays named by a durable
  owned `sidx:` record until a `part:` record or a `retire:` obligation names it, in the same
  batch that removes the `sidx:` record. `Committed` is answered only on evidence that **this
  attempt's own** commit landed, evidence no other writer's record or teardown can produce. A
  slot is released exactly once. An `Open` session is never answered `NoSuchUpload`, and a
  departed attempt's residue is left only to a reaper that will walk it. **Sources:** the
  maintainer's rule 2026-07-25 (via principles.md §5); 0016's refutation standard
  (`0016:2809-2813`); the commit classification (`0016:1584-1653`) and the two failure-mode rows
  on unknown outcomes (`0016:1780`, `:1792`). SELF-TEST: it binds the commit, the classification
  re-read, the compensation and the re-upload batches, so guarding one function cannot satisfy
  it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.1 (staging): this slice consumes the staged attempt it
  returns and edits the module and records it adds. It builds on 657.1's folded result. #658
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
  `psum:` summary derived from it, the per-part `{chunks}` row of `RetireObligation`, the docs
  sentence, and the new test file. / out of scope:
  - the slot reserve, the owned intent, the drain fence, placement, the fragment writes and
    renewal itself (657.1; change them only where this slice's hand-off needs it, and say why);
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
  - a loop counter that never reaches its bound → (C)'s bounded-rounds leg.
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
a lease covering the commit's transaction deadline, and send no renewal after it. Before building
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

**After a `CommitUnknownResult`, settle identity, not presence** (`0016:1630-1651`: "finds its own
commit present"). `PartRecord` carries no attempt id, and an empty part carries no chunk ids
either, so the part's bytes alone do not identify the attempt (C1). The commit batch deletes the
attempt's own slot. While the session is `Open@E`, the only writers of that slot are this
attempt's own batches (`0016:1625-1629`). So "the slot is gone" identifies this attempt's commit,
provided (a) this attempt sent no compensation that could have deleted it, and (b) the session
was **still the same `Open@E` after** the slot was observed gone. A session state read before the
slot read cannot vouch for it (C2); one way that holds is to read the session last, or again
after the slot and part reads. Anything that does not settle it proves "did not land" **only**
when `may_still_commit` is `false` (`crates/traits/src/lib.rs:240-247`). With `true`, keep the
doubt the way #895's Abort does: every later batch of this attempt requires its own slot at the
bytes the held batch also requires, and the held batch deletes that slot, so whichever lands
first excludes the other. A commit that may have landed stays in the doubt until a read settles
it (C3). Bound the rounds; if they run out unsettled, return an in-doubt error carrying the
doubt, never a guess.

**Writer constructor.** `PartRecord` has no writer-side constructor today ("No writer-side
constructor — see `SessionRecord`'s doc", `multipart.rs:2927`). Add a crate-private one in the
shape of `SessionRecord::open`, so every value written is one the decoder accepts back unchanged.
`PartSummary` has public fields (`:3029-3038`); build the `psum:` value from the `PartRecord` it
summarizes, so the pair cannot disagree (`:3022-3026`).

**Docs currency** (`AGENTS.md:154-157`). Extend 657.1's sentence in the multipart records
paragraph of `docs/design/architecture/05-building-block-view.md` (`:244` on `main`): `part:` and
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
<!-- pdca:end child-2 -->
