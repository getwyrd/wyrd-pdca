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
  **(A′) The write deadline is exact, no publication starts after it, and an unverified one is
  never counted.**
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
  - **A publication that straddles the deadline** (added 2026-10-22). The trait lets a store
    begin publishing inside the window and finish after it, and then answer
    `WriteEffect::Unknown` with whatever landed left in place (`crates/traits/src/lib.rs:1128-1141`,
    `:871-892`); the filesystem store renames first and checks its clock after
    (`crates/chunkstore-fs/src/lib.rs:389`, `:416-422`). So the fake gets a second hold point,
    **inside** its publishing step: it passes the publication-point check before the deadline,
    holds, and the test moves the clock past the deadline. Run 1: the test lets the fake finish —
    the fragment becomes readable — and the fake answers `WriteEffect::Unknown`
    (`WriteDeadlineExpired::if_publication_unverified`, `:958`). Run 2: the stager reaches the
    deadline while the fake is still inside the step, abandons the put, and returns; only then does
    the fake finish and publish. In both runs: staging stops with a typed error that says the
    write's effect is **unknown** — never success, never the clean "nothing landed" refusal of the
    leg above; no staged attempt is handed off as complete; the fragment that did land sits on a
    position the chunk's durable owned `sidx:` entry names (its `StagedPlacement`), so it is
    evidenced; and the owned entry and the slot are still present, residue the reaper reclaims
    with the session, as for every other stopped staging. A stager that counts an `Unknown` put as written, or reports either run as
    nothing written, fails this leg.
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
  **(E′) The intent's own session fence** (added 2026-10-22). The reserve legs in (C) run an Abort
  between the reserve's read and its commit; these run it against the **intent**, which needs its
  own `require(mpu == Open@E)` (`0016:659`) — the Abort fence rewrites only the session and its
  obligation (`crates/core/src/multipart/abort.rs:77-93`), so the intent's slot precondition alone
  would still pass.
  - **E′1:** the reserve succeeds; the wrapper runs a real #895 Abort immediately before the
    first chunk's intent commits (and, in a second run, before the third chunk's). The intent
    conflicts, staging stops with `NoSuchUpload`, and afterwards no owned `sidx:` entry exists for
    that chunk and the fake holds no fragment of it. Entries of earlier chunks stay, as residue for
    the reaper.
  - **E′2:** an intent held with `may_still_commit: true`, not applied; the wrapper then runs the
    Abort. Staging re-reads, finds the session no longer `Open`, and stops (that batch can no longer
    land, so nothing is in doubt). The test then releases the held batch to the inner store: it
    answers `Conflict`, and no owned entry for that chunk exists.
  Removing only the intent's `require(mpu == Open@E)` makes both fail: the post-Abort intent lands,
  an owned entry appears under an `Aborting` session, and in E′1 its fragments are written.
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
  (A″)'s empty pieces are sent by hand on the test clock. (A′)'s straddling put is held inside the
  fake's publishing step while the clock moves. (E′)'s Abort runs by hook right before the intent
  commits, and its held intent is released by hand afterwards. (F)'s conflicts are scripted. (H) is the
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
  fragment write **begins** publishing after its authorization deadline (the store judges at its
  publication point); a write whose effect the store could not verify, or that staging stopped
  waiting for, is never counted as written and never reported as nothing written; and every
  position such a write could land on is already named by a durable owned record. Staging never
  waits on outside work (a body read, a fragment write) without a fail-closed bound. A lapsed lease is never revived. An
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
  - a `WriteEffect::Unknown` put counted as written, and an abandoned put reported as nothing
    written (two edits) → (A′)'s straddle leg;
  - the intent's `require(mpu == Open@E)` removed (and nothing else) → (E′);
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
instant: writes still pending at the deadline are abandoned and staging stops with a typed error,
fail-closed (`0016:1555-1557`). No store may **begin** publishing an abandoned write any more, but
one already inside its publishing step may still finish (`crates/traits/src/lib.rs:1128-1141`), so
the error says the effect of the abandoned writes is unknown, never that nothing landed. A write
the store answered `Ok` is taken whatever the clock reads when it is seen (`Ok` means published
strictly before the deadline, checked at both ends). A refusal (`WriteEffect::NotApplied`) means
nothing landed. A `WriteEffect::Unknown` answer is neither: the chunk is not counted staged,
staging stops with the same unknown-effect error, and nothing is retracted — the bytes, if any, sit
on a position the chunk's owned entry already names (position coverage, `0016:1549-1552`).
**What this slice does not establish: an absolute landing bound.** 0016 sizes the orphan grace on
"no accepted write can be applied more than `W_write` after it was authorized"
(`G_orphan > W_write + δ_clock`, `0016:1553-1578`). The landed seam gives less: a publishing step
whose duration no store bounds can straddle the deadline by any amount, and that is exactly the
case `Unknown` reports. This slice's two guarantees — no publication begins after the deadline,
and every landing position is named before any fragment is written — keep a straggler evidenced
**while its position's mark survives**, not after GC has reclaimed it. Closing that gap needs a
bound on each backend's publishing step, or a reclaimer that tolerates a post-grace straggler, and
both are outside this slice (backends are out of scope). Do not claim the bound in code comments or
docs. ⚑ **Human:** decide whether that gap gets an issue (the backends under #638, or the
`G_orphan` owner #625) before the multipart verbs ship; this slice only states it.

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

Plan-review response (2026-10-22): both findings hold at `4bac538` and are revised in place.
(1) Deadline: the invariant now promises what the seam gives. No publication begins after the
deadline; an `Unknown` or abandoned write is never counted as written or as nothing written; and
every landing position is named first. A new (A′) leg holds the fake inside its publishing step
across the deadline, in two runs. The Design states that the absolute landing bound behind
`G_orphan > W_write + δ_clock` is not established here (a publishing step can straddle by any
amount, `traits/src/lib.rs:1128-1141`), and ⚑ flags the gap for the human instead of widening
scope into backends. (2) Session fence: new leg (E′) runs a real Abort right before an intent
commits, and releases a held intent after an Abort and asserts it conflicts. Removing only the
intent's `require(mpu == Open@E)` is a required red.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: No advisory review exists (the codex reviewer exited 1 early with a codex-side error, not a memory kill); the next Check must produce one. Keep the design and slice boundary. Fix the five open review findings: 1. Add a read-fault hook to the test wrapper and cover a store read that fails while a commit is in doubt: one reserve leg, one intent leg (schedule: intent Hold, then Unknown{applied:false, may_still_commit:false}, then get(sidx:) faults -> InDoubt{may_still_commit:true}). Also cover bounded's post-timer now_or_never (stage.rs:762-764). 2. Make the error class of an unknown-effect stop independent of timing: use the most indeterminate report as source(), give an abandoned-only stop an Indeterminate source, and assert the class in run 2 of the straddle leg. 3. Fix the lease docs (stage.rs:67-69, :1128-1130): slot values are later than the lease they replace, but retry rounds of one batch may repeat a value. 4. Share one request validator between reserve_slot and stage_part (lease TTL < 2, max_inflight_parts = 0) and test reserve_slot in the (H) leg. 5. bounded must enforce the idle deadline when it accepts a ready body read (a late piece or EOF must not slip past it); add the deterministic schedule for that. Keep the rule that an acknowledged fragment Ok proves timely publication.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  No advisory review exists (the codex reviewer exited 1 early with a codex-side error, not a memory kill); the next Check must produce one. Keep the design and slice boundary. Fix the five open review findings:
  1. Add a read-fault hook to the test wrapper and cover a store read that fails while a commit is in doubt: one reserve leg, one intent leg (schedule: intent Hold, then Unknown{applied:false, may_still_commit:false}, then get(sidx:) faults -> InDoubt{may_still_commit:true}). Also cover bounded's post-timer now_or_never (stage.rs:762-764).
  2. Make the error class of an unknown-effect stop independent of timing: use the most indeterminate report as source(), give an abandoned-only stop an Indeterminate source, and assert the class in run 2 of the straddle leg.
  3. Fix the lease docs (stage.rs:67-69, :1128-1130): slot values are later than the lease they replace, but retry rounds of one batch may repeat a value.
  4. Share one request validator between reserve_slot and stage_part (lease TTL < 2, max_inflight_parts = 0) and test reserve_slot in the (H) leg.
  5. bounded must enforce the idle deadline when it accepts a ready body read (a late piece or EOF must not slip past it); add the deterministic schedule for that. Keep the rule that an acknowledged fragment Ok proves timely publication.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
