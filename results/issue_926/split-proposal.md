<!-- pdca:split-proposal v1 -->
# Split proposal — issue 926

**Intake-cap override (wyrd-pdca-P1), 2026-10-22.** `scripts/plan-cap --track alpha --need 2`
answered `planned 7/6 (cap, track alpha) — room for 0, need 2: Plan intake closed` (PLANNED 508
625 633 658 659 927, BUILT 654). The human said in this Plan session, in answer to the cap refusal,
"Override the split", then "Cut as suggested and update "depends on"". The override covers **this
split of #926 only**. Accepting it takes alpha to 9/6 while the two children are open. Why it was
granted: #926 blocks five of the seven counted alpha bundles (927, 658, 508 and 625 directly, 633
through 658), so alpha's count cannot fall while #926 is stuck, and its v3 sign-off
(`iteration-v3/SUMMARY.md` §9) asked for exactly this split.

## Why this slice is oversized

#926 (657.1, UploadPart staging) went to Check three times and was sent back to Plan on the third
round. Its last build was a 278 KB patch (threshold 100 KB): 162 KB of test file, 77 KB of the new
staging module, 22 KB of architecture doc (one 8.5 KB line rewritten) and 17 KB elsewhere. The
staging design held up: CI green, 96.4% diff coverage, every mutant caught, including the
adversary's 33 hand-made ones. But each round produced new implementation findings, and they
cluster. Of the 13 confirmed across the three rounds, 8 were in **renewal** or the waits it races
against: renewal retry rounds re-reading the clock, the `renew` signature, a renewal test that hung
instead of failing, the race between renewal and the write deadline, a renewal test that passed
only because the fake accepted a late write, the renewal batch's size, and the body-idle bound
(twice: missing, then re-armed by empty pieces). The other 5 (intent lease checks, intent recovery,
the reserve's pinned index, the 0016 sentence, the refusal answers) were small fixes to code that
otherwise held.

So the slice has two outcomes, and they divide at renewal:

1. **Staging that is safe: 657.1a.** Reserve one slot. Register each chunk as an owned `sidx:`
   entry, fenced on the session and the drain ledger, before any of its fragments is written. Put
   every fragment write and every body read under a fail-closed deadline. Hand the staged attempt
   to the commit. Leases are written at the reserve and at each intent, and never renewed in
   between, so a request slower than one lease TTL stops with a typed lapse error. That is safe:
   nothing is resurrected and nothing is lost. It is not yet live.
2. **Staging that stays alive: 657.1b.** Renew the slot and every owned lease together for as
   long as the request runs, including while a body read or a fragment write is still pending,
   without ever holding staging past a write deadline or the body-idle bound. Give the commit its
   quiesce renewal (`StagedAttempt::renew`). Keep every renewal batch inside `B_ops`.

The body-idle bound goes with the **first** child, not the renewal one. The target's rubric makes
every await on outside work bounded and fail-closed (`AGENTS.md:181-183`), and 657.1a awaits the
client's body. Without the bound, 657.1a would ship an unbounded await. The bound is the same
mechanism as the write deadline, and 657.1a already needs that. (This moves the empty-piece fix to
657.1a too; in Plan it was first proposed for 657.1b.)

**Expected sizes, from the v3 patch.** 657.1b is about 80 KB: about 44 KB of v3's test legs, about
30 KB of renewal code, and a short doc edit. 657.1a is still about 190 KB: about 60 KB of code, the
44 KB test harness (scripted store wrapper, Fleet fake, manual clock and timer), about 70 KB of
legs, and the new legs this re-plan adds. A three-way split would not bring it under 100 KB either:
the harness and the legs are what make it big. The human chose two children knowing this. Expect
the size backstop to fire on 657.1a at Check. Most of the patch is tests.

**Both children carve from the v3 code.** Each brief cites
`/home/eddie/wyrd/wyrd-pdca/results/issue_926/iteration-v3/patch.diff` as its reference
implementation (the one prior-cycle file Do may open), names the part of it the child owns, and
lists the confirmed fixes to apply. Neither child designs staging from scratch.

**One test file, shared.** 657.1a ships `crates/core/tests/multipart_upload_part_stage.rs` as a new
file. 657.1b **appends** its legs to that file and reuses its harness. It does not copy the 44 KB
harness into a second file, and it does not put it in a shared module under `tests/`, because
`engine/scripts/run-verify.sh:141-144` classifies every added `*/tests/*.rs` as a test target. An
added `crates/core/tests/support/mod.rs` would make C4-verify run `cargo test --test mod`, which does
not exist (`run-verify.sh --is-test crates/core/tests/support/mod.rs` answers `yes`). The cost: C4-verify reports `PASS (green-only)` for 657.1b
(`run-verify.sh:493-504`), so 657.1b's per-fix red is carried by its demonstrated reds, not by that
gate. Its brief says so. Dry runs of `run-verify.sh --classify` on patches shaped like each child:
657.1a → `ADDED_TEST crates/core/tests/multipart_upload_part_stage.rs`; 657.1b (which also adds
rows to `crates/core/tests/multipart_knobs.rs`) → no `ADDED_TEST`.

## Wave sketch

Two waves, strictly stacked. `child-2` `Depends on` `child-1`: renewal rewrites the slot and the
owned entries that 657.1a's reserve and intent create, and it races the deadlines 657.1a's waits
enforce, in the module 657.1a adds. It builds on 657.1a's folded result (`$PDCA_VERIFY_BASE`,
honoured by `run-verify.sh`). Neither child conflicts with #653 (folded on
`pdca-integration/main-r42c5348f203d`, not on `main`): #653 edits `crates/core/src/lib.rs`,
`crates/core/src/staged_publication.rs`, its tests and
`docs/design/architecture/08-crosscutting-concepts.md`, and both children are barred from those
files. #744 (open PR #931) edits `05-building-block-view.md:297`, the `validate` paragraph; 657.1a
adds a paragraph after `:244` and 657.1b edits only that new paragraph, so git merges them cleanly.

**Dependents of #926 must be re-pointed after acceptance.** A split parent ends COMPLETE (as #657
does today), and the scheduler does not move its dependents onto its children, so
`Depends on: 926` would count as met at once. On acceptance, replace `926` with both child ids in
the `Depends on` of #927, #658, #508 and #625. #927 needs both: it consumes 657.1a's staged attempt
and calls 657.1b's `StagedAttempt::renew` for its quiesce step (`0016:1720-1733`).

<!-- pdca:child child-1 -->
# core: UploadPart staging — slot reserve, owned intent, drain fence, write and body deadlines (657.1a)

> First of the two slices #926 (657.1) was split into on 2026-10-22. The second, 657.1b, adds
> in-flight renewal on top of this one. Do reads ONLY this file, plus the one reference
> implementation it cites. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read these before writing
> code: the `slot:` and `sidx:` rows of the record table (`0016:349`, `:353`); the **Part slot
> reserve** and **Part intent** rows of the batch inventory (`0016:658-659`); decision 5's
> admission text (`0016:1676-1719`, above all the reserve's unknown-outcome rule `:1699-1719`);
> and the write deadline (`0016:1553-1578`). Citations were verified against `getwyrd/wyrd`
> `origin/main` @ `4bac538` on 2026-10-22 (`12dd082` plus a `deny`-only change, #929). Lines
> marked **v3** are in the reference implementation, not on `main`.

- **Slug:** multipart-upload-part-stage-core
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** Sessions can be created (#894) and aborted (#895), but nothing stages a part's
  bytes, and no code writes a `slot:` or `sidx:` record. This slice adds the safe half of
  `UploadPart`'s staging as `wyrd-core` functions over `MetadataStore` and a placement-aware chunk
  store. It (1) claims one in-flight slot `slot:<id>:<k>`, answering in the landed vocabulary
  (`ReserveOutcome` `crates/core/src/multipart.rs:4590`, `Refusal` `:4487`). It (2) streams the
  body into chunks and registers each one as an owned `sidx:` entry **before** any of its
  fragments is written, in a batch fenced on the session being `Open@E` and on no selected
  D-server having a drain recorded. Placement is against the topology **minus draining servers**,
  and a drain that lands mid-stage makes the chunk re-plan, never refuse. It (3) writes each
  fragment with an authorization deadline and waits for it no longer than that deadline. It (4)
  waits for each body read no longer than a caller-supplied idle bound. It (5) hands the staged
  attempt to the commit (657.2). Leases are written at the reserve and at each intent and are not
  renewed in between: in-flight renewal is 657.1b. No `part:` or `psum:` record is written here.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_upload_part_stage.rs`. It
  runs over `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store (the shape
  of `Fleet`, `crates/core/tests/domain_placement.rs:60`, with its `PlacementChunkStore` impl at
  `:113`), a test-local store wrapper, and a test runtime whose clock and timer the test moves by
  hand. The wrapper logs every batch and can, before a chosen commit: answer `Conflict` without
  applying; answer `CommitUnknownResult` with either `may_still_commit` value, applied or not;
  hold the batch and land it later (as `crates/core/tests/multipart_abort.rs:961-980` does); or
  write a key first. **The chunk-store fake enforces the write deadline as the trait requires:**
  it refuses a put whose deadline has passed at its publication point, the instant before it
  stores the fragment, with `WriteDeadlineExpired::if_elapsed` (`crates/traits/src/lib.rs:925`;
  the rule at `:1080-1096`). Every session in it is made by #894's Create. Every leg is bounded by
  a watchdog and ends any body channel it feeds, so a regression fails fast instead of hanging.
  The test shows all of the following.
  **(A) Staging, and the order that makes it safe.** A body of at least three chunks is fed as a
  stream of **uneven pieces**: one piece smaller than a chunk, one that spans a chunk boundary,
  and none aligned to the chunk size. It stages exactly `ceil(len / chunk_size)` chunks, and the
  chunk bytes read back through the fragments, concatenated, equal the body. The staged attempt's
  length and digest equal the body's length and its SHA-256 `Digest` (`multipart.rs:1036-1052`,
  ADR-0047). While it stages (after the first intent, before the function returns),
  `scan("sidx:")` is non-empty and `scan("pending:")` is empty. When staging returns, the store
  holds this attempt's one `slot:` record and exactly one owned `sidx:` entry per chunk, each
  decoding with the placement its fragments were written to, and nothing under `part:` or
  `psum:`. The wrapper's log shows every chunk's `sidx:` commit **before** the first fragment put
  of that chunk (`0016:1788`). An empty body stages no chunk. **The hand-off carries complete
  chunk references:** the staged attempt lists, in chunk order, one full `ChunkRef` per chunk
  (`id`, `scheme`, `len`, `placement`: every field, `crates/core/src/metadata.rs:351-361`),
  because `PartRecord` stores exactly that list (`multipart.rs:2930-2935`) and neither
  `OwnedEntry` nor `StagedPlacement` records a chunk's length (`:4136-4139`, `:4045-4047`).
  Assert: the `len`s sum to the body length; every chunk but the last has `len == chunk_size` and
  the last has the short remainder; each `id` is the chunk id of its owned `sidx:` entry; and
  each `scheme`/`placement` equals that entry's `StagedPlacement`.
  **(A′) The write deadline is exact, and nothing is published after it.**
  - The fake records `deadline_millis` on every staged fragment put
    (`crates/traits/src/lib.rs:1224-1229`; #638). Each one equals `t_auth + W`, where `t_auth` is
    the clock reading taken before that chunk's intent batch was committed. The wrapper advances
    the clock right after the intent commit returns (a delayed dispatch), and the deadline is
    still `t_auth + W`: never `t_dispatch + W`, never `None`, and never an unbounded value such as
    `u64::MAX`.
  - A fragment put the fake holds past its deadline is abandoned: staging stops with a typed
    error naming the elapsed write deadline, at the deadline, without waiting for the put.
  - **A held put released after expiry is refused and not published** (v3 sign-off). The
    fake holds one put, the test moves the clock past its deadline, staging stops, and then the
    test releases the put. The fake refuses it, and no fragment of that put is readable afterwards.
    A fake without the publication-point check fails this leg.
  **(A″) The body-idle bound.** The staging functions take a body-idle bound (a caller-supplied
  parameter, like the clock).
  - A body read left unanswered for the bound stops staging with a typed error, on the test's
    clock and timer, and nothing is written after it.
  - The bound is per read, not per body: a body that sends a piece inside every window is never
    cut off.
  - **Empty pieces do not count as progress** (v3 sign-off; the v3 defect at v3
    `crates/core/src/multipart/stage.rs:1392-1401`). A body that sends only empty pieces, each
    just inside the bound after the last, is stopped at the bound measured from the last
    **non-empty** piece (or from the start of the body, if there was none). HTTP/2 permits empty
    DATA frames, so a production body can produce this.
  **(B) The drain fence.**
  - **B1:** the wrapper writes `desired:dserver:<S>`, for a server `S` the plan selected, **just
    before** that chunk's intent commits. The intent must fail and re-plan. Staging still
    completes, and no `sidx:` entry or fragment of the part names `S`. Removing the per-server
    `require_absent(desired:dserver:<S>)` from the intent batch makes B1 fail.
  - **B2:** `desired:dserver:<S>` is present **before** staging, and `S` is the server the
    selector prefers (lowest utilization). Staging completes, nothing names `S`, and the wrapper
    sees **no** intent conflict. Planning against the full topology instead of the topology minus
    draining servers makes B2 fail.
  - **B3:** with 4 of 12 single-server domains draining, and a scheme that fits in the remaining 8
    (e.g. RS(4,2)), a part stages. With too few non-draining domains for the scheme, before or
    after the reserve, the answer is a typed placement error that writes no owned entry, never
    `NoSuchUpload` for an `Open` session.
  - The drain key keeps the custodian's exact bytes, `desired:dserver:<id>`
    (`crates/custodian/src/desired_state.rs:35-40`).
  **(C) Slots.**
  - `MAX_INFLIGHT_PARTS` reserves (`multipart.rs:5032`) succeed on one session, and the next
    answers `Refused(SlowDown { pressure: Backpressure::InflightParts { .. } })`
    (`multipart.rs:4467-4470`, `:4528`).
  - **The same answer from `stage_part`** (v3 sign-off): with every slot index held,
    `stage_part` answers `Refused(SlowDown { InflightParts })`, never `Refused(NoSuchUpload)`.
    (In v3, swapping that answer at v3 `stage.rs:1115` kept the whole suite green.)
  - A reserve against a non-`Open` session answers `Refused(NoSuchUpload)` and writes nothing.
  - **A session that leaves `Open` between the reserve's session read and its commit** (an Abort
    in between) answers `Refused(NoSuchUpload)`, not `SlowDown`; and so does the same move on the
    pinned path, after an ambiguous outcome (v3 `stage.rs:998`, `:985`).
  - A reserve whose batch lands but answers `CommitUnknownResult` adopts its own slot on re-read
    (same `attempt_id`). In this schedule, which leaves no residue, the session's held-slot count
    equals the number of live attempts (`0016:1779`).
  - **The same-index rule is shown by a schedule that tells it apart from "lowest free index".**
    Index 0 is held by another attempt X. This reserve takes index 1, and its batch is held
    (`may_still_commit: true`, absent on re-read). X releases index 0 before the retry. The retry
    must go to index **1** again with the same `attempt_id` (`0016:1707-1716`). Releasing the held
    batch then leaves exactly **one** slot of this attempt. A reserve that probes the lowest free
    index after an ambiguous outcome takes index 0 here and ends with two slots, so this leg fails
    on it.
  - After a bounded number of ambiguous rounds the reserve refuses with `SlowDown`. The number of
    reserve commits equals the stated bound. **Then the held reserve lands after the refusal:**
    with `MAX_INFLIGHT_PARTS − 1` other slots already held, the wrapper releases the held batch.
    Exactly **one** slot of the refused attempt exists, never two; no live attempt owns it; and it
    counts toward the cap, so the next reserve answers `Refused(SlowDown { InflightParts })`
    (residue is counted on purpose, `0016:1839-1842`, `multipart.rs:4467-4469`). This slice adds
    no cleanup for it: the reaper reclaims it with the session (F11a).
  - The index a doubtful batch may still install, found carrying another attempt's id, refuses
    `SlowDown` and probes nowhere else.
  - **A pinned index is not dropped on a newer session epoch alone** (iteration-1 finding): after
    an absent read of the pinned index and then a read showing the session at a newer `Open`
    epoch, the reserve re-reads and settles the pinned index first. A held reserve that lands
    between those two reads is adopted, and the attempt holds one slot. The reserve under a
    session moved to a later `Open` epoch reserves under that epoch.
  - An undecodable slot record at the reserved index is a typed error, never adopted.
  **(D) Leases in this slice: written, checked, never revived.**
  - The reserve writes the slot lease at `now + TTL`. Each intent writes its owned entry's lease at
    `now + TTL` and rewrites the slot with its lease moved forward, in the same batch
    (`0016:659`). No lease this slice writes is earlier than the slot lease it replaces.
  - **Every intent round checks every lease of the attempt at its own fresh clock reading before
    it sends anything** (iteration-1 finding): the slot's lease and every earlier owned lease, in
    the first round and in every retry, including after the drain read. A lapsed lease refuses the
    intent, writes nothing, stops staging with a typed error, and keeps the doubt of any batch
    already sent (`0016:1843-1847`; the peer rule at `crates/core/src/write.rs:496-517`). Legs: the
    slot lease lapses during the intent's drain read; an owned lease lapses during it; a lease
    lapses between two intent rounds.
  **(E) Unknown intent outcomes are settled by a re-read, never guessed.**
  - An intent whose batch lands but answers `CommitUnknownResult` is recognised by re-reading the
    owned entry's exact bytes. Staging goes on without a second entry for that chunk, and its
    fragments go to the placement that landed.
  - An intent answered `CommitUnknownResult { may_still_commit: false }` and not applied is
    retried, and staging ends with exactly one owned entry per chunk. A read that settled it is not
    reported as in doubt.
  - An intent held with `may_still_commit: true`: no fragment of that chunk is written on the
    strength of an absent read. Every later batch of the attempt requires its own slot at the bytes
    the held batch also requires, so at most one of them lands. In two runs, (i) the held batch
    released before the attempt's next batch and (ii) every later batch answered `Conflict`
    unapplied until the rounds run out and then the held batch released, two things hold: every
    fragment written is named by a durable owned entry with that fragment's placement, and in (ii)
    staging returns an in-doubt error that keeps the `may_still_commit` doubt, as Abort's does
    (`crates/core/src/multipart/abort.rs:264-270`).
  - **A held intent that lands between the entry read and the slot read is adopted**
    (iteration-1 finding): the slot read finds the slot bytes one of this attempt's own intents
    put, so the intent reads the entry again and adopts it, instead of answering that the slot was
    lost. Also adopted: a held intent that lands before the session is seen to move.
  - A held intent whose session has moved to a non-`Open` state is no longer in doubt and stops;
    an intent under a session moved to a later `Open` epoch lands under that epoch.
  - A chunk id already taken, and an owned entry removed after its intent landed, are typed errors.
  **(F) Every loop is bounded.** The reserve probe (also while the session keeps moving), the
  ambiguous reserve, and the intent loop (re-plans included) each have a stated maximum. With a
  wrapper that answers every batch `Conflict` without applying it, each loop ends in a typed error
  after **exactly** that many commits (the test counts them).
  **(G) The per-part chunk bound.** A body longer than `MAX_PART_CHUNKS` chunks
  (`multipart.rs:4972`) never registers more than `MAX_PART_CHUNKS` owned entries for its part.
  Staging stops and reports the overrun, and its caller receives the staged attempt intact, so the
  commit slice can refuse and compensate it. Use a test-sized bound only if the function takes it
  as a parameter.
  **(H) Input and the whole gate.** A request the functions cannot honour (a zero chunk size, a
  lease TTL below 2, a zero idle bound) writes nothing and answers a typed error. `cargo xtask ci`
  is green on the patched tree: the tracker's acceptance line and `AGENTS.md:41-45`'s rule for any
  change that can affect the Rust build. This slice also edits `wyrd-custodian` and the
  architecture docs, which `cargo test -p wyrd-core` does not cover.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and the wrapper, the fake and the hand-moved clock and timer force every red leg rather than
  leaving it to timing. B1's drain record is written right before the intent commits. (C)'s and
  (E)'s unknown and held outcomes are injected; (C)'s same-index leg frees index 0 by hand between
  the held batch and the retry; its session-leaves-`Open` legs run the Abort between the session
  read and the commit. (A′)'s late put is released by hand after the clock passes its deadline.
  (A″)'s empty pieces are sent by hand on the test clock. (F)'s conflicts are scripted. (H) is the
  gating `C4-ci` check, which runs `cargo xtask ci`. History says what these legs must kill: in the
  discontinued #636 patch both fence mutants survived the whole suite; in #657's first attempt,
  removing the same-index pin survived; in #926's v3, three refusal-answer swaps survived (v3
  `stage.rs:985`, `:998`, `:1115`), empty pieces re-armed the idle bound, and a fake that ignored
  `deadline_millis` let a late write pass as on time. B1, B2, (C)'s schedules, (A′)'s late put and
  (A″)'s empty pieces exist for exactly those.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the staging side, safety half. Every staged fragment is named by a
  durable owned `sidx:` record, with its placement, from **before** it is written. No owned record
  can be created once the session has left `Open`, or naming a server whose drain is recorded. No
  fragment is published after its authorization deadline, and staging never waits on outside work
  (a body read, a fragment write) without a fail-closed bound. A lapsed lease is never revived. An
  `Open` session is never refused as `NoSuchUpload`. No attempt ever holds more than one slot; a
  slot an ambiguous reserve installs after its attempt refused, or a crashed attempt leaves, is
  residue that counts toward the cap until the reaper reclaims it with the session
  (`0016:1716-1717`, `:1839-1842`; `multipart.rs:4467-4469`). **Sources:** the maintainer's rule
  2026-07-25 (via principles.md §5); 0016's refutation standard (`0016:2809-2813`); decision 5's
  properties (`0016:1529-1545`), the drain fence (`0016:353`, `:659`), the reserve rule
  (`0016:1699-1719`) and the write deadline (`0016:1553-1578`); the rubric's await discipline
  (`AGENTS.md:181-183`). SELF-TEST: it binds the reserve and intent batches, the placement step and
  both waits, so guarding one function cannot satisfy it. Liveness (leases kept alive while the
  client supplies data) is 657.1b's invariant, not this slice's.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** First of #926's split; 657.1b depends on it, and #927 (657.2), #658, #508 and
  #625 depend on both children. **No `Conflicts with` #653, on purpose:** #653 edits neither
  `crates/core/src/metadata.rs` nor `docs/design/architecture/05-building-block-view.md`. Do not
  touch `crates/core/src/lib.rs` or `docs/design/architecture/08-crosscutting-concepts.md` (#653's).
  #744's PR #931 edits `05-building-block-view.md:297`, far from this slice's paragraph. Intake:
  created by the split of #926 under the human's per-id cap override of 2026-10-22 (alpha 7/6
  before it, 9/6 with both children open).
- **Difficulty:** high
- **Do model:** opus
- **Scope:** the safe half of `UploadPart`'s staging as store-I/O functions in `wyrd-core`: the
  slot reserve; the per-chunk owned intent; staged placement against the topology minus draining
  servers, with re-plan on a drain-fence conflict; the fragment writes with their authorization
  deadline, and a wait for them bounded by it; the body reads, each bounded by a caller-supplied
  idle bound that only a non-empty piece resets; the leases the reserve and the intents write, and
  their lapse checks; the bounded per-part chunk count; and the staged attempt handed to the commit.
  Also: one home in core for the `desired:dserver:<S>` key, so core and the custodian spell it
  once; the writer-side constructor the slot record lacks; the 0016 sentence the v2
  sign-off authorized changing (`0016:1714-1719`); the docs paragraph; and the new test file. / out
  of scope:
  - in-flight renewal of the slot and owned leases, the renewal timer and its race against the
    deadlines, `StagedAttempt::renew`, and the renewal batch-size clamp (all 657.1b);
  - the part commit, its outcome classification, live-session compensation, re-upload, the refusal
    of an over-limit part and the cumulative `MAX_STAGED_CHUNKS` ceiling, and the composed
    `UploadPart` (all 657.2, #927);
  - Complete, the segment committer and publication (#658, #653); draining any `retire:`
    obligation, the owned-`sidx:` walk and the terminal delete (#659); the reaper (#625); seeded
    DST races (#660, #665);
  - the D-server side of the `W_write` deadline (built by #638; this slice passes the deadline on
    every staged write and bounds its own wait);
  - the gateway / S3 `UploadPart` verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's); any
    edit to proposal 0016 other than the one sentence above, or to any ADR; `Cargo.lock` and
    `deny*.toml`.
- **Repro instruction:** n/a (new functionality). Today `write::intent` writes only ordinary
  `pending:` entries with `owner: None, staged: None` (`crates/core/src/write.rs:201-219`), and no
  code writes a `slot:` or `sidx:` record.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_stage.rs`, a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator,
  `engine/scripts/run-verify.sh:141-144`; a `--classify` dry run on a patch shaped like this
  slice returned `ADDED_TEST crates/core/tests/multipart_upload_part_stage.rs`). Keep the harness
  in this file: 657.1b appends its legs here and reuses it. Do not move it into a module under
  `tests/` (an added `tests/<dir>/mod.rs` is classified as a test target too, and C4-verify would
  run `cargo test --test mod`).
- **Verification posture:** (a) net-new. The staging functions do not exist on `main`, so the test
  cannot compile on C4-verify's reverted base, and C4-verify reports **UNVERIFIABLE** (exit 77), a
  pre-declared sign-off item.
  **Built and exercised at Check:** the reserve, intent, placement, fragment-write and body-read
  paths, driven by the new test over a real store, a deadline-enforcing chunk-store fake and a
  hand-moved clock and timer.
  **Demonstrated reds (required, recorded in `build-notes.md` with the exact edit and the name of
  the test that failed).** Do makes each temporary edit, shows the named leg fail, then restores
  the file. Each red must come from the edit its label names:
  - the per-server `require_absent(desired:dserver:<S>)` removed from the intent batch → B1;
  - planning against the full topology instead of the topology minus draining servers → B2;
  - the same-index pin removed after an ambiguous reserve (lowest free index taken) → (C);
  - after an ambiguous reserve refuses, a probe of a fresh index for the same attempt → (C)'s
    landed-after-refusal leg (two slots of one attempt);
  - `stage_part`'s full-range refusal answered `NoSuchUpload` → (C)'s `stage_part` leg;
  - a session that left `Open` between read and commit answered `SlowDown`, on the plain and the
    pinned path (two edits) → (C)'s session-leaves-`Open` legs;
  - the pinned index cleared on a newer session epoch without settling it → (C)'s epoch leg;
  - the lapse check skipped on an intent retry, and skipped after the drain read (two edits) → (D);
  - after an unknown intent outcome, an absent read taken as "did not land" with
    `may_still_commit: true` → (E);
  - the slot-read path answering "slot lost" without re-reading the entry → (E)'s
    between-the-reads leg;
  - a loop counter that never reaches its bound (e.g. `rounds += 1` made `rounds *= 1`) → (F);
  - a staged fragment put with `None`, with `Some(u64::MAX)`, or with its deadline taken at
    dispatch (three edits) → (A′);
  - the wait for fragment writes not bounded by their deadline → (A′)'s abandoned-write leg;
  - the fake's publication-point check removed → (A′)'s late-put leg;
  - an empty piece re-arming the idle deadline → (A″)'s empty-pieces leg;
  - the body read awaited without the idle bound → (A″)'s stalled-body leg;
  - a body piece that spans a chunk boundary merged into one oversize chunk → (A);
  - the hand-off's last `ChunkRef` given `len == chunk_size` instead of the remainder → (A).
  **C5 (advisory) survivors:** `build-notes.md` lists every mutant `cargo mutants --in-diff` leaves
  alive in this slice's code, each with the test that now kills it or why it is equivalent.
- **Production reach:**
  - (a) The test exercises staging for real against a real store. Production stages no part in
    this slice.
  - (b) 657.1b adds renewal; 657.2 (#927) consumes the staged attempt; the S3 verb lands with #669
    under #508, after #658/#659 and the reaper (#625), per 0016's same-release rule
    (`0016:236-268`).
  - (c) This slice **must not** add a production caller. Until 657.1b lands, a request slower than
    one lease TTL stops with a typed lapse error; with no caller, no client can see it.
  - (d) The one existing production path it may touch is the custodian's use of the
    `desired:dserver:` key, which must keep the same bytes.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Reference implementation (the one prior-cycle file you may open):**
    `/home/eddie/wyrd/wyrd-pdca/results/issue_926/iteration-v3/patch.diff`, the third build of the
    parent slice, whose design the v3 sign-off kept. Carve this slice from its
    `crates/core/src/multipart/stage.rs`, `crates/core/tests/multipart_upload_part_stage.rs` and
    its edits to `multipart.rs`, `metadata.rs`, `write.rs`, `desired_state.rs` and 0016. Leave out
    the renewal machinery (v3's `Footprint::renew`/`apply`/`reconcile`, `RenewalLedger`,
    `renew_if_due`, the renewal arm of `Stager::renewing`, `StagedAttempt::renew`, `RenewOutcome`,
    `RENEWAL_ROUNDS`, `ABANDONED_COMMIT`) and the legs that test it: those are 657.1b's. Keep the
    deadline race (v3 `Stager::renewing` without its renewal arm, and `Deadline`). Apply this
    brief's fixes: the fake's publication-point check, the empty-piece rule, and the refusal-answer
    legs. Do not re-submit v3 unchanged.
  - **Peer callsite for the per-chunk loop:** `lease_write_chunk` and `stream_write_data`
    (`crates/core/src/write.rs:485-535`, `:559`): lease (here: owned intent) before writing a
    chunk's fragments, and **refuse rather than resurrect** a lapsed lease (`:496-517`). Encode a
    chunk as that path does (`encode_chunk`, `write.rs:138`, private today). Plan placement as
    `WritePlan::place` (`write.rs:112`) does, over `Topology::excluding`
    (`crates/core/src/placement.rs:147`).
  - **Peer for settling an unknown outcome:** #895's Abort,
    `crates/core/src/multipart/abort.rs:282`. It re-reads after `Conflict`, never reads
    `CommitUnknownResult` as `Conflict`, and keeps a `may_still_commit: true` doubt (`:264-270`).
  - **Peer for the caller-supplied wait:** `CreateRuntime::wait`
    (`crates/core/src/multipart/create.rs:81-82`), core's seam for time passing, since `Clock`
    only reads time (`crates/testkit/src/lib.rs:23-26`).
  - **The drain-fence key:** today only `crates/custodian/src/desired_state.rs:35-40`
    (`DESIRED_PREFIX`, `desired_key`). Core cannot depend on the custodian (ADR-0010), so the key
    gets one home in core, the custodian uses it, and its bytes do not change. The custodian's
    `deferred: #657` notes (`desired_state.rs:226-230`, `:246-249`) are then settled: update them
    to say the intent writer now fences (the seeded interleaving stays `deferred: #665`).
  - **The fake's deadline check:** `WriteDeadlineExpired::if_elapsed`
    (`crates/traits/src/lib.rs:925`), applied at the publication point the trait defines
    (`:1088-1096`).
  - **Records and keys:** `slot_key` / `slot_range` (`multipart.rs:1251`, `:1261`), `sidx_key` /
    `sidx_range` (`:1328`, `:1339`), `SlotRecord` (`:2719`), `StagedPlacement` (`:4045`),
    `OwnedEntry` (`:4136`), `AttemptId` (`:891`), `SlotIndex` (`:1000`), `upload_part_answer`
    (`:4756`), `MAX_INFLIGHT_PARTS` (`:5032`), `MAX_PART_CHUNKS` (`:4972`), `Digest`
    (`:1036-1052`). `SessionRecord::open` is the model for a crate-private writer-side
    constructor (see its doc, `:2323-2328`). Declare the new module beside `pub mod abort;` /
    `pub mod create;` (`multipart.rs:150-151`).
  - **`W_write`:** `W_WRITE_MILLIS` (`crates/custodian/src/gc.rs:202`), which core cannot import
    (ADR-0010), so the caller supplies it, as it supplies the clock.
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/write.rs`,
  `crates/core/src/multipart.rs`, `crates/core/src/multipart/`, `crates/core/src/metadata.rs`,
  `crates/custodian/src/desired_state.rs`, `crates/core/tests/`,
  `docs/design/architecture/05-building-block-view.md`, proposal 0016) on `origin/main` @
  `4bac538`. Merged: the record family and decoders (#654, via PRs #703, #725, #734, #793), knobs
  (#801), Create (#908), Abort (#909), the custodian's staged protection and drain counting (#807,
  #822). Rejected or iterated: #636's single-slice attempt (discontinued 2026-10-10: both fence
  mutants survived, and an `Open` session was answered `NoSuchUpload` with 4 of 12 D-servers
  draining); #657's first attempt (iterated to Plan 2026-10-19, oversized, then split); #926's
  three builds (`iteration-v1` to `-v3`, iterated to Plan 2026-10-22 and split here). Their
  confirmed staging-side findings are this slice's (C), (D), (E), (A′) and (A″) legs. The new test
  file does not exist on `main`. Open PRs on getwyrd/wyrd touching these paths: #931 (#744,
  `05-building-block-view.md:297`, the `validate` paragraph only), 2026-10-22.
- **Disposition hint:** new-feature

## Design

The reference implementation (v3) is this design, with renewal still in it. Keep its shape.

**Reserve** (`0016:349`, `:658`, `:1676-1719`). Read the bounded `slot:<id>:` range and the
session. Refuse `NoSuchUpload` unless the session is `Open@E` (`upload_part_answer`). Pick a free
index `k`, and commit `require(mpu == Open@E)` + `require_absent(slot:<id>:<k>)` + the slot record
(`part_number`, `attempt_id`, `reserved_at_millis`, `lease_expiry_millis`). A lost index probes the
next free one, at most `max_inflight_parts` probes; a full range is `SlowDown { InflightParts }`. A
session found no longer `Open` is `NoSuchUpload`, on every path. On `CommitUnknownResult`, re-read
`slot:<id>:<k>`. Its own `attempt_id` means adopt it. With `may_still_commit: false`, anything else
means it did not land. With `may_still_commit: true`, absence proves nothing, so retry the **same**
index with the same `attempt_id`; another attempt's id there does not prove a loss either (it may
release the index and the batch then land), so refuse `SlowDown` on reading one, and after a bounded
number of ambiguous rounds. A newer session epoch is not a reason to drop the pinned index: re-read
it and settle it first.

**Intent per chunk** (`0016:353`, `:659`). Take `t_auth` from the clock. Check the slot lease and
every earlier owned lease at `t_auth`; refuse a lapsed one. Plan the chunk's placement against the
topology minus the servers with a `desired:dserver:` record (read from the store). Then commit one
batch: `require(mpu == Open@E)`; `require_absent(desired:dserver:<S>)` for **every** server in the
placement; `require(slot:<id>:<k> == prior)`; `require_absent` of the entry; the owned
`sidx:<id>:<part>:<chunk>` entry (`OwnedEntry` with the planned `StagedPlacement`); and a rewrite of
the slot (its lease moved forward). Every round, retries included, takes a fresh `t_auth` and checks
the leases again before it sends. A `Conflict` re-reads. Session not `Open` ⇒ stop with
`NoSuchUpload` and leave the residue to the reaper. A drain record now present ⇒ re-plan without
that server and try again.

**Fragment writes and their deadline.** Only once the intent is observed durable, write the
fragments, each carrying `t_auth + W_write` as `put_fragment_at`'s `deadline_millis` (the D server
refuses a late write, #638). Every fragment of the chunk carries that same value however late it is
dispatched, so the deadline can only be early, never late. The function must not substitute an
unbounded or default deadline. Race the pending writes against the caller's wait for that same
instant: writes still pending at the deadline are abandoned (no store may apply them any more) and
staging stops with a typed error, fail-closed (`0016:1555-1557`). A write the store answered is
taken whatever the clock reads when it is seen, since a store refuses a write after its deadline.
The orphan grace is sized on this bound (`G_orphan > W_write + δ_clock`, `0016:1567-1578`); an
unbounded staged write would let a straggler land unevidenced.

**Body reads and the idle bound.** Each pending body read is raced against the caller's wait for
the idle deadline: the last non-empty piece (or the start of the body) plus the bound. A read still
pending then stops staging with a typed error. An empty piece is skipped and does **not** move the
deadline. A piece is never dropped or re-requested: the read is polled across the wait, not
restarted. The code is runtime-free (ADR-0009): it spawns nothing, and both waits come from the
caller's runtime, on the clock that owns the leases.

**Settling an unknown outcome.** Re-read and compare exact bytes: the slot for a reserve, the owned
entry for an intent. Equal ⇒ it landed. Anything else proves "did not land" **only** when
`may_still_commit` is `false` (`crates/traits/src/lib.rs:240-247`). With `true`, keep the doubt the
way #895's Abort does: every later batch requires the slot (or, for a reserve, the slot's absence)
at the bytes the held batch also requires, so whichever lands first excludes the other. A slot read
that finds bytes one of this attempt's own intents put means that intent landed: read the entry
again and adopt it. Bound the rounds. If they run out unsettled, return an in-doubt error carrying
the doubt, never a guess. No fragment is written for a chunk whose owned entry has not been
observed durable.

**The hand-off to the commit (657.2).** Staging returns a public value the commit can build its
batch, or its compensation, from **alone**, without re-reading the body or re-deriving anything: the
session bytes it was fenced on (`Open@E`); the slot index and the slot's current bytes; each owned
entry's key, current bytes and placement, in chunk order; one complete `ChunkRef` per chunk, in
chunk order (the last chunk's `len` is the short remainder), which is exactly the list `PartRecord`
stores; the part number, length and digest; and whether the per-part bound was hit. Make it public:
657.2's tests and #658's tests call it from `crates/core/tests/`. 657.1b adds the quiesce renewal to
it; leave room for a method, nothing more.

**Writer constructor.** The slot record has no writer-side constructor today. Add a crate-private
one in the shape of `SessionRecord::open`, so every slot value written is one `decode` accepts back
unchanged.

**Proposal 0016, one sentence** (authorized by the human at the v2 sign-off, overriding the
"no edit to 0016" line of the earlier brief). `0016:1714-1719` says the reserver "probes onward only
once it reads that index carrying *another* attempt's id". That is unsafe: the held batch's
`require_absent` can still pass if the other attempt releases the index. Change the text to the
code's reading (refuse `503` on reading another attempt's id, or after the bounded ambiguous
retries; never probe onward), as v3 did.

**Docs currency** (`AGENTS.md:154-157`). Describe the new writer in the living architecture doc:
`slot:` and `sidx:` now have a writer; staging registers each chunk before its bytes are written, in
a batch that fails once the session has left `Open` or a selected server's drain is recorded; every
fragment write and body read is under a fail-closed deadline; an ambiguous reserve retries only the
index it may already hold; and nothing calls it in production until #669. **Add it as a new
paragraph after `docs/design/architecture/05-building-block-view.md:244`**, rather than rewriting
`:244` itself: that paragraph is a single 8.5 KB line, so any edit to it puts the whole line into
the diff twice, and its two claims stay true ("landed ahead of most of their writers"; nothing
writes them in production yet). 657.1b extends the new paragraph with renewal. Say nothing there
about renewal.

## Alternatives considered

- **Refuse the part when a selected server starts draining.** That makes routine operator action a
  client-visible failure across the fleet (the discontinued patch's `NoSuchUpload` with 4 of 12
  draining). Rejected; re-plan.
- **Leave the body read unbounded until renewal lands.** Simpler, but it ships an await on outside
  work with no bound, against the rubric's MUST (`AGENTS.md:181-183`), in a slice that already has
  the deadline mechanism. Rejected.
- **Renew owned leases inside each intent batch.** It would make every intent O(chunks so far) and
  duplicate 657.1b's batch. Rejected; this slice writes leases and checks them, nothing more.
- **Owned entries under `pending:`.** Every global `pending:` scan would enumerate them
  (`0016:1773`). Rejected; `sidx:` is disjoint.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core: UploadPart staging — in-flight renewal of the slot and owned leases (657.1b)

> Second of the two slices #926 (657.1) was split into on 2026-10-22. It builds on 657.1a
> (reserve, owned intent, drain fence, write and body deadlines), which lands in the wave before
> this one. Do reads ONLY this file, plus the one reference implementation it cites. The design is
> normative in proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read
> these before writing code: the `slot:` row of the record table (`0016:349`); the **Part intent**
> row of the batch inventory (`0016:659`), which names renewal's coupling to the slot; the slot
> lease and renewal rules (`0016:1824-1847`); and the quiesce rule (`0016:1720-1733`). Citations
> were verified against `getwyrd/wyrd` `origin/main` @ `4bac538` on 2026-10-22. 657.1a's own code
> is cited by what it does; lines marked **v3** are in the reference implementation.

- **Slug:** multipart-upload-part-stage-renewal
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.1a a part can be staged safely, but its leases are written only at the
  reserve and at each intent. A request slower than one lease TTL, before its first chunk or
  between chunks, therefore stops with a lapse error. And until it stops, the reaper and the
  idle fence would read it as abandoned (`0016:1824-1837`). This slice adds in-flight renewal. It
  (1) moves the slot's lease and every owned lease forward together, in one batch, half a TTL
  ahead, for the whole request, including while a body read or a fragment write is still pending.
  It (2) races that renewal against 657.1a's write deadline and body-idle bound, so renewal never
  holds staging past either. It (3) refuses a lapsed lease rather than reviving it. It (4) settles
  an unknown renewal outcome by a re-read, or reports it in doubt. It (5) gives the staged attempt
  its quiesce renewal for the commit (`StagedAttempt::renew`, `0016:1720-1733`). It (6) keeps every
  renewal batch inside `B_ops` (`MAX_BATCH_OPS`, `crates/core/src/multipart.rs:5106`).
- **Success criterion:** New legs **appended** to `crates/core/tests/multipart_upload_part_stage.rs`
  (657.1a's file), over its harness: the logging store wrapper, the deadline-enforcing chunk-store
  fake, and the hand-moved clock and timer. Every leg is bounded by the harness's watchdog and ends
  any body channel it feeds, so a regression fails fast instead of hanging. R8's clamp rows go in
  the existing knob test, `crates/core/tests/multipart_knobs.rs`. Every one of 657.1a's legs still
  passes unchanged, except where a leg asserted the absence of renewal; such a leg is updated, and
  `build-notes.md` names it. The legs show all of the following.
  **(R1) The renewal batch.** Every renewal batch rewrites the attempt's own `slot:` record in the
  same batch as **every** owned `sidx:` lease of the attempt, under `require(mpu == Open@E)`,
  `require(slot == prior)` and `require(sidx: == prior)` for each entry (`0016:349`, `:659`). Each
  lease it writes is strictly later than the slot lease it replaces.
  **(R2) Renewal runs for the whole request, whatever is pending.**
  - **Before the first chunk:** a body delivered in small pieces, with the clock moved past half a
    lease TTL between them and before the first chunk boundary, gets its slot lease moved forward
    before the first intent.
  - **Body withheld (D-i):** after the slot is reserved, the test withholds the next piece, moves
    the clock past half a TTL and fires the timer. A renewal batch rewriting the slot commits
    **while the read is still pending**. Then the piece is delivered and staging finishes
    normally, with the piece neither dropped nor re-requested.
  - **Fragment write held (D-ii):** the fake holds one fragment put of chunk 2, after chunk 1's
    owned entry is durable. The test moves the clock and fires the timer. A renewal batch rewriting
    the slot and chunk 1's and chunk 2's owned leases commits **while the put is still pending**.
    Then the put is released (before its deadline), staging finishes normally, and that put is not
    re-sent.
  - A renewal falls due at exactly half a TTL after the last one; a renewal due as an intent starts
    is sent before that intent, and never while the attempt's own intent batch is in flight.
  - In all of these, neither the slot lease nor any owned lease is ever observed lapsed while the
    client is still supplying data (`0016:1824-1837`).
  **(R3) Renewal never holds staging past a deadline.**
  - **The reviewers' schedule** (v2 sign-off): the fake holds a fragment put and the
    wrapper holds a renewal commit; the test moves the clock past the fragment deadline. Staging
    ends at the deadline, before the renewal is released, with an in-doubt error that keeps the
    renewal's `may_still_commit` doubt.
  - **The adversary's schedule:** a put answered in time while a slow renewal is still committing
    is taken, and staging succeeds.
  - **A write answered while a renewal commits is taken** — and the leg proves the write was
    published **before** its deadline: the hook that releases the held put waits until the fake
    has stored the fragment before it moves the clock (v3 sign-off; v3's hook at v3
    `crates/core/tests/multipart_upload_part_stage.rs:1589` moved the clock first, so the put
    published 101 ms late and passed only because v3's fake did not enforce deadlines; 657.1a's
    fake now refuses that).
  - A renewal cut off by the deadline between two of its own commits keeps exactly the doubt of
    the commit still in flight, no more and no less.
  - **The body-idle bound still holds under renewal:** a stalled body is stopped at 657.1a's idle
    bound even though renewals keep its leases alive meanwhile, and empty pieces still do not reset
    it.
  **(R4) A lapsed lease is refused, never revived** (`0016:1843-1847`; the peer rule at
  `crates/core/src/write.rs:496-517`).
  - With the clock moved past the slot's lease, or past an owned entry's, before the next renewal,
    the renewal refuses, writes nothing, and staging stops with a typed error.
  - **Every renewal round re-reads the clock and re-checks every lease before it sends**
    (iteration-1 finding), while still settling a batch it already sent: a conflict or held round
    whose retry crosses a lease's expiry refuses the retry and keeps the earlier round's doubt.
  - A renewal after the session left `Open` refuses and resurrects nothing.
  - A slot lost to another writer under an unchanged session is a typed error.
  **(R5) Unknown renewal outcomes are settled by a re-read, never guessed.** A renewal answered
  `CommitUnknownResult` is settled by re-reading its own slot's exact bytes: landed is adopted, and
  not landed with `may_still_commit: false` is retried. A held renewal that lands before the
  session is seen to move is adopted. A held renewal that never settles ends in an in-doubt error
  carrying its `may_still_commit` doubt once the rounds run out.
  **(R6) The renewal loop is bounded.** With a wrapper that answers every batch `Conflict` without
  applying it, one renewal ends in a typed error after **exactly** its stated maximum of commits.
  **(R7) The quiesce renewal (`StagedAttempt::renew`).** The method takes the caller's clock (not
  a fixed `now_millis`), reads it every round, never moves a lease backwards, refuses a lapsed
  lease, and answers a typed refusal (`NoSuchUpload`) once the session has left `Open`. 657.2 (#927)
  calls it as the last renewal before its part commit.
  **(R8) Every renewal batch fits `B_ops`** (v3 sign-off).
  - `knob_clamps_hold` (`crates/core/src/multipart.rs:5430`) refuses, **by name**, a `KnobSet`
    whose `max_part_chunks` would make a full renewal batch exceed its `batch_ops`, the way it
    refuses an oversize part commit (`:5385`, `:5453-5454`). The adversary's case passes every
    clamp on `main` today (re-checked in Plan, 2026-10-22, by a scratch run of
    `multipart_knobs.rs`): `derived(with(D, |k| { k.scheme = EcScheme::None; k.max_part_chunks =
    249; }))`, re-derived by that file's own `derived` (`crates/core/tests/multipart_knobs.rs:105-116`;
    without the re-derivation the set fails `SessionsOverFootprint` instead). Its renewal would
    carry `3 + 2 × 249 = 501` operations against `MAX_BATCH_OPS = 500` (`:5106`). With the clamp it
    is refused; the set one chunk smaller (248, 499 operations) passes. `KnobSet::DEPLOYED` still
    passes (158 chunks, 319 operations).
  - **The new clamp binds before the part-commit clamp.** `2N + 3 > N + 8` for every `N ≥ 6`, so at
    the shipped `N = 158` a `batch_ops` that fails the part commit (`N + 8 = 166`) already fails
    the renewal (319). So existing Leg 2 rows stop testing what they say: the on-bound row
    `with(D, |k| k.batch_ops = part_ops) => Ok(())` (`multipart_knobs.rs:200`) fails outright, and
    the two `PartCommitOverOps` rows (`:242-243`) answer the renewal clamp instead if it is checked
    first. Keep Leg 2's rule, "for every clamp, a set exactly on its bound passes and one step past
    is refused by name" (`multipart_knobs.rs:8-10`): move the part-commit rows to a set where the
    part commit binds (`N ≤ 4`, e.g. `TINY`, `:68-74`), and add the renewal clamp's own on-bound and
    one-past rows. `build-notes.md` names every row it changed.
  - Staging refuses, before writing anything, a request whose chunk bound would build a renewal
    batch past `MAX_BATCH_OPS`.
  **(R9) The whole gate.** `cargo xtask ci` is green on the patched tree (`AGENTS.md:41-45`).
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target. The
  timer is fired by hand while a body piece is withheld or a fragment put is held, the clock is
  moved by hand, and the wrapper holds, conflicts and answers unknown on the chosen renewal commit,
  so no leg depends on timing. Each red is a renewal that does not happen (R2), happens at the wrong
  moment (R2, R3), outlives a deadline (R3), revives a lease (R4), or guesses an outcome (R5). On
  657.1a's base, without this slice, every R2 leg fails (no renewal batch is ever sent) and R8's
  `knob_clamps_hold` leg fails (the adversary's set passes). (R9) is the gating `C4-ci` check. The
  per-fix red is **not** measured by C4-verify for this slice (see Verification posture); it is
  carried by the demonstrated reds.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), on the staging side, liveness half. A live request's slot lease and
  owned leases do not lapse while it is supplying data, so the reaper never reads a live request as
  abandoned. A lapsed lease is never revived. Renewal never holds staging past a fragment's write
  deadline or a body read's idle bound. An unknown renewal outcome is settled by a re-read or
  reported in doubt, never guessed. Every renewal batch fits `B_ops`, so no part can reach a size
  whose leases cannot be renewed. **Sources:** the maintainer's rule 2026-07-25 (via principles.md
  §5); 0016's slot-lease and renewal rules (`0016:1824-1847`), the slot row (`0016:349`), the
  intent row (`:659`), the quiesce rule (`:1720-1733`) and the write deadline (`:1553-1578`); the
  lapsed-lease rule of #490 (`crates/core/src/write.rs:496-517`); `B_ops` and why a batch past it
  is stuck (`crates/core/src/multipart.rs:5090-5106`). SELF-TEST: it binds the renewal batch, its
  scheduling against every pending wait, the quiesce method and the knob clamp, so guarding one
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.1a: renewal rewrites the slot and the owned entries 657.1a
  creates, races the deadlines 657.1a's waits enforce, and extends the module, the test file and
  the docs paragraph 657.1a adds. It builds on 657.1a's folded result. #927 (657.2) depends on
  this slice for `StagedAttempt::renew`. **No `Conflicts with` #653:** this slice edits neither
  `crates/core/src/lib.rs` nor `docs/design/architecture/08-crosscutting-concepts.md`. Intake:
  created by the split of #926 under the human's per-id cap override of 2026-10-22 (alpha 7/6
  before it, 9/6 with both children open).
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** in-flight renewal of an attempt's slot and owned leases in `wyrd-core`'s staging
  module: the renewal batch; when it runs (half a TTL ahead, for the whole request, including while
  a body read or fragment write is pending, on the caller's timer, never while the attempt's own
  intent is in flight); its race against 657.1a's write deadline and body-idle bound; its lapse
  refusal and per-round clock re-read; the settling of its unknown outcomes; its bounded loop; the
  quiesce renewal on the staged attempt; the renewal batch-size clamp in `knob_clamps_hold` and in
  staging's request check; the module docs and the docs paragraph 657.1a added; and the new legs.
  / out of scope:
  - anything 657.1a owns that renewal does not change: the reserve, the intent, placement, the drain
    fence, the deadlines themselves, the hand-off's fields, the drain key, 0016;
  - the part commit and everything else in 657.2 (#927), including **when** the commit calls the
    quiesce renewal;
  - the reaper (#625), the idle fence's own batch, and any knob loading (#508);
  - seeded DST races (#660, #665); any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md` (#653's); any
    edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`. 0016's knob table
    (`0016:1462-1479`) does not list the renewal clamp; do not edit it here, and say so in
    `build-notes.md` so the human can decide whether 0016 needs the row.
- **Repro instruction:** n/a (new functionality). On 657.1a's base, stage a body whose pieces
  arrive more than half a lease TTL apart: no renewal batch is ever sent, and once a lease has
  lapsed, the next intent refuses with a lapse error.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_stage.rs`, **MODIFIED**: this slice
  appends its legs to 657.1a's file and reuses its harness, instead of copying the 44 KB harness
  into a second file. R8's clamp rows go in `crates/core/tests/multipart_knobs.rs`, also modified.
  A `--classify` dry run on a patch shaped like this slice returned no `ADDED_TEST`.
- **Verification posture:** (a) net-new, and **C4-verify cannot measure this slice's red.** With no
  added test file, C4-verify takes its green-only branch and reports `PASS (green-only)`
  (`engine/scripts/run-verify.sh:493-504`). That line is **not** a red→green: it says only that
  the patched tree's `wyrd-core` tests pass. Read it as such at sign-off.
  **Built and exercised at Check:** the renewal batch, its timer race, the quiesce method and the
  clamp, driven by the appended legs over a real store, the deadline-enforcing fake and the
  hand-moved clock and timer.
  **Demonstrated reds (required, recorded in `build-notes.md` with the exact edit and the name of
  the test that failed).** These carry the per-fix red. Do makes each temporary edit, shows the
  named leg fail, then restores the file. Each red must come from the edit its label names:
  - renewal removed entirely (657.1a's behaviour) → every R2 leg;
  - the slot rewrite dropped from the renewal batch → (R1);
  - renewal checked only between delivered pieces, with no timer → (R2) D-i;
  - the timer not raced against a pending fragment put → (R2) D-ii;
  - a renewal sent while the attempt's own intent is in flight → (R2)'s intent-start leg;
  - the renewal awaited before the pending work instead of raced against it → (R3)'s reviewers'
    and adversary's schedules;
  - a renewal still committing at the deadline reported as clean instead of in doubt → (R3);
  - renewal deferring the body-idle bound → (R3)'s idle leg;
  - renewal that moves a lapsed lease forward instead of refusing → (R4);
  - a renewal retry that reuses the first round's clock reading → (R4)'s between-rounds leg;
  - after an unknown renewal outcome, an absent read taken as "did not land" with
    `may_still_commit: true` → (R5);
  - a renewal loop counter that never reaches its bound → (R6);
  - `StagedAttempt::renew` given a fixed `now_millis` → (R7);
  - the renewal clamp removed from `knob_clamps_hold` → R8's one-past row in
    `multipart_knobs.rs`; and removed from staging's request check → R8's staging leg (two
    edits).
  **C5 (advisory) survivors:** `build-notes.md` lists every mutant `cargo mutants --in-diff` leaves
  alive in this slice's code, each with the test that now kills it or why it is equivalent.
- **Production reach:**
  - (a) The legs exercise renewal for real against a real store. Production stages no part in this
    slice.
  - (b) 657.2 (#927) calls `StagedAttempt::renew` as its quiesce step; the S3 verb lands with #669
    under #508, after #658/#659 and the reaper (#625), per 0016's same-release rule
    (`0016:236-268`). #508 loads the knobs the new clamp judges.
  - (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Reference implementation (the one prior-cycle file you may open):**
    `/home/eddie/wyrd/wyrd-pdca/results/issue_926/iteration-v3/patch.diff`, the third build of the
    parent slice, whose design the v3 sign-off kept. This slice is its renewal machinery:
    v3's `Footprint::renew`, `apply`, `reconcile` and `moved_on`, `RenewalLedger`, `renew_if_due`,
    the renewal arm of `Stager::renewing`, `StagedAttempt::renew`, `RenewOutcome`,
    `RENEWAL_ROUNDS`, `ABANDONED_COMMIT`, and the legs that test them (in its
    `crates/core/src/multipart/stage.rs` and `crates/core/tests/multipart_upload_part_stage.rs`).
    Port them onto 657.1a's module, apply this brief's fixes (the hook that waits for publication,
    the clamp), and do not re-submit v3's renewal unchanged.
  - **Peer for renewal:** `lease_write_chunk` (`crates/core/src/write.rs:485-535`): renew half a
    TTL ahead, and refuse rather than resurrect a lapsed lease (`:496-517`).
  - **Peer for settling an unknown outcome:** #895's Abort,
    `crates/core/src/multipart/abort.rs:282` (keeps a `may_still_commit: true` doubt, `:264-270`).
  - **Peer for the clamp:** `PartCommitOverOps` in `knob_clamps_hold`
    (`crates/core/src/multipart.rs:5385`, `:5453-5454`); `MAX_BATCH_OPS` and why a batch past it
    can never finish (`:5090-5106`); `KnobSet::batch_ops` (`:5308`) and `KnobSet::DEPLOYED`
    (`:5325-5342`).
  - **The trait's unknown-outcome flag:** `CommitUnknownResult::may_still_commit`
    (`crates/traits/src/lib.rs:240-247`).
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/src/write.rs`, `crates/core/tests/`,
  `docs/design/architecture/05-building-block-view.md`) on `origin/main` @ `4bac538`. Merged: the
  knob clamps (#801), the streaming write path's renewal and its lapsed-lease refusal (#490), Abort
  (#909). Rejected or iterated: #657's first attempt renewed only at chunk boundaries and revived a
  lapsed lease; #926's three builds (`iteration-v1` to `-v3`) carried renewal's retry-round,
  signature, deadline-race and test-fake findings, all named in this brief's legs. Open PRs on
  getwyrd/wyrd touching these paths: #931 (#744, `05-building-block-view.md:297` only),
  2026-10-22.
- **Disposition hint:** new-feature

## Design

The reference implementation (v3) is this design. Keep its shape.

**The renewal batch** (`0016:349`, `:659`, `:1824-1847`). One batch, under `require(mpu ==
Open@E)` and `require(slot == prior)`, rewrites the slot with its lease moved forward and every
owned `sidx:` entry with its lease moved forward, each under `require(sidx: == prior)`. Every
lease it writes is strictly later than the slot lease it replaces. A renewal that finds a lease of
its own already lapsed refuses and writes nothing (`0016:1843-1847`). A renewal that fails because
the session left `Open` stops staging and recreates nothing.

**When it runs.** Half a TTL after the last renewal, or after the reservation, measured on the
caller's clock. Before each intent, and never while the attempt's own intent batch is in flight
(both rewrite the slot under `require(slot == prior)`, so they would only fence each other out).
And while a body read or a fragment write is pending: 657.1a already races each of those against
the caller's wait for its deadline; this slice adds the next renewal instant to that race. When the
renewal instant wins, the renewal is sent and the pending read or write keeps being polled: it is
resumed, never dropped or re-sent. The code stays runtime-free (ADR-0009): it spawns nothing.

**Renewal against the deadlines.** A due renewal is raced against the pending work and the
deadline, never awaited first. Work that finishes while the renewal commits is taken (a store
refuses a late write, so a write it answered was applied in time); the renewal is then run to its
end, since no other batch of the attempt may be sent while it is in flight. A renewal still
committing when the deadline passes is abandoned with the work, and its outcome is reported in
doubt, carrying `may_still_commit` (v3's `ABANDONED_COMMIT` backend names it). Renewal never moves
657.1a's body-idle deadline.

**Rounds.** Each renewal round, the first and every retry, takes a fresh clock reading and checks
every lease at it before it sends, while still settling a round already sent. A `Conflict` re-reads.
An unknown outcome is settled by re-reading the slot's exact bytes, as 657.1a settles an intent:
equal ⇒ landed; anything else proves "did not land" only with `may_still_commit: false`. Bound the
rounds; if they run out unsettled, return the in-doubt error.

**The quiesce renewal.** Add a public `renew` to the staged attempt, taking the caller's clock, so
657.2 can run the last renewal before its part commit (`0016:1720-1733`). Same batch, same rules,
same bounds as the in-flight renewal.

**The batch-size clamp.** A full renewal batch for `N` owned entries carries `3 + 2N` operations
in v3's shape (the session and slot requires, the slot put, and a require and a put per entry).
Make `knob_clamps_hold` refuse a set whose `max_part_chunks` would put that batch past `batch_ops`,
beside `PartCommitOverOps`, and make staging refuse a request whose chunk bound would, before it
writes anything. Do states the exact count its batch carries and the clamp matches it.

**Docs currency** (`AGENTS.md:154-157`). Extend the paragraph 657.1a added after
`docs/design/architecture/05-building-block-view.md:244`: for as long as the request runs,
including while a body read or a fragment write is pending (on a timer the caller supplies), one
batch moves the slot's lease and every owned lease forward together; that renewal never holds
staging past a pending write's deadline or the body-idle bound, and one still committing at the
deadline is reported as an unknown outcome; every round checks every lease at its own clock reading,
so a lapsed lease is refused, never revived; and no renewal batch can exceed `B_ops`. Do not touch
`:244` itself.

## Alternatives considered

- **Renew only at chunk boundaries, or between delivered pieces.** A slow client that has not filled
  its first chunk then holds a slot whose lease lapses, and a pending read or write can outlast the
  lease with nothing to wake the renewal (`0016:1824-1837`). Rejected; renewal is on the caller's
  timer.
- **Await a due renewal before resuming the pending work.** A slow renewal commit then holds staging
  past a write deadline and can discard a write answered in time (the v2 finding). Rejected;
  race them.
- **Split a large renewal across several batches.** The slot and the owned leases must move together
  (`0016:349`, `:659`); two batches would let one land without the other. Rejected; clamp the size.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-2 -->
