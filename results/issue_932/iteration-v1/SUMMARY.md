# Result — issue 932 / multipart-upload-part-stage-core

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Sessions can be created (#894) and aborted (#895), but nothing stages a part's
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
- Success criterion: A NEW test file `crates/core/tests/multipart_upload_part_stage.rs`. It
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the safe half of `UploadPart`'s staging as store-I/O functions in `wyrd-core`: the
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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.2% — 765 of 795 instrumentable changed lines executed (floor 80%); 795 of 1685 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 135 mutants tested in 15m: 103 caught, 32 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_932/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.27s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '--property', 'MemoryMax=16G', '--property', 'MemorySwapMax=0', '--property', 'ManagedOOMMemoryPressure=kill', '--', 'codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check', '-m', 'gpt-6-astra', '-c', 'model_reasoning_effort=xhigh', '-c', 'sandbox_workspace_write.network_access=true', '--json']' returned non-zero exit status 1.).

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

### Advisory — adversary

# Adversarial review — #932 (657.1a, `UploadPart` staging core)

**Bottom line:** I could not break the safety properties. Each chunk's owned entry is written before any of its fragments. The session fence, the drain fence and the one-slot-per-attempt rule all hold, and a lapsed lease is never revived. The four findings below are test gaps and contract nits that the builder can fix. None is a safety break.

**How I attacked it.** I copied the patched tree into scratch and ran `crates/core/tests/multipart_upload_part_stage.rs`: 54/54 pass. C4-verify reports UNVERIFIABLE because the test uses API the patch adds (`gate-logs/C4-verify.log`; this was declared in advance). So I made the red legs myself with 23 hand mutations of `crates/core/src/multipart/stage.rs`. 20 of them go red. That includes every brief-listed red I tried:
- the per-server `require_absent` removed (B1 fails);
- planning against the full topology (B1, B2 and all of B3 fail);
- the intent's `require(mpu == Open@E)` removed (E′1, E′2 and two others fail);
- a reserve that probes onward after reading another attempt's id, or after its ambiguous rounds run out (the C legs fail);
- the pinned-reserve re-read on a session move removed (`stage.rs:961`), and the intent's re-read on a session move removed (`:1520`);
- slot-read adoption removed (`:1495`);
- the lapse check run only in round 1, or taken before the drain read, and `<=` changed to `<` (the D legs fail);
- an `Unknown` write counted as written, and an abandoned write reported as "nothing landed" (A′ fails);
- the deadline taken at dispatch (A′ fails);
- an empty piece re-arming the idle deadline (A″ fails through the watchdog);
- the chunk bound off by one (G fails);
- `ChunkIdTaken` ignored;
- `.max(..)` dropped from `next_expiry` (10 tests fail).

The three mutants that survive are all in finding 1.

- NEEDS-HUMAN [impl] — **No test covers a store read that fails while a commit is still in doubt, so C5's "103 caught, 0 missed" overstates the coverage.** Two hand mutants survive all 54 tests:
  - (a) At `crates/core/src/multipart/stage.rs:1479-1482` (the entry read after an intent commit), map a read fault to `StageError::stopped(..)` instead of `doubt.end(..)`. Staging then reports "no batch of this part attempt is in doubt" (`stage.rs:526-527`) while a held intent may still land. That is the rubric's *Transactions* class: a write that may still land, reported as if nothing is pending.
  - (b) At `stage.rs:679`, swap `held.or(unsettled)` to `unsettled.or(held)`. This silently turns `InDoubt.may_still_commit` from `true` into `false`.

  A concrete schedule that catches both:
  1. Intent round 1 answers `Act::Hold`.
  2. Round 2 answers `Act::Unknown { applied: false, may_still_commit: false }`.
  3. The following `get(sidx:…)` faults.

  The correct answer is `InDoubt { may_still_commit: true }`; both mutants answer otherwise. diff-cov already lists these fault arms as never run (`stage.rs:895, 931, 954, 959, 980, 1475`, in `gate-logs/C4-diff-cov.log`). cargo-mutants does not generate swaps like these inside closures, which is why C5 stayed silent.

  Fix: add a read-fault hook to the wrapper (`Then` at `crates/core/tests/multipart_upload_part_stage.rs:552-561` has none), then one reserve leg and one intent leg.

  A smaller gap with the same cause: `bounded`'s post-timer `now_or_never` (`stage.rs:762-764`, diff-cov MISS at `:763`) never runs, and deleting it keeps all 54 tests green. So the module-doc row "answered `Ok` … whatever the clock reads when the answer is seen" (`stage.rs:42`) has no test. Deleting that code would only make staging more cautious.
- NEEDS-HUMAN [impl] — **The error class of an unknown-effect write depends on timing, not on what landed.** In `a_prime_a_publication_straddling_the_deadline_is_never_counted`, both runs end with the same store state: the fragment is stored at a position the owned entry names. I printed `classify(&err)` for each: run 1 = `Indeterminate`, run 2 = `Terminal`.
  - Why: `Stop::WriteEffectUnknown` uses its first report as `source()` (`stage.rs:619-621`). A stop where every unknown write was abandoned has no report, so `classify` falls back to `Terminal` (`crates/traits/src/lib.rs:803`). The seam defines `Terminal` as "retrying cannot help" (`lib.rs:685-689`), and the trait already maps the same fact (`WriteEffect::Unknown`) to `Indeterminate` (`lib.rs:798-799`).
  - The same flaw hits mixed reports. Reports are kept in the order writes answered, so a write that fails with a `TransientFault` and answers first sets the class, even if a later write answered `WriteEffect::Unknown`.
  - The test checks the class only in run 1 (`multipart_upload_part_stage.rs:1765-1771`). `stage.rs:372-375` documents the choice, but 657.2 and #669 will route errors by class.

  Fix: use the most indeterminate report as the source, give an abandoned-only stop an `Indeterminate` source, and assert the class in run 2.
- NEEDS-HUMAN [impl] — **The lease docs claim slot values never repeat, and they do.** `stage.rs:67-69` says "every batch puts a slot value no earlier batch of the attempt put", and `stage.rs:1128-1130` says the same about `next_expiry`.
  - Why it's false: `next_expiry` depends only on the round's clock reading and the last *adopted* slot. So intent retries at one reading put byte-identical slot values. I checked with a probe in `f_the_intent_loop_is_bounded`: its 4 intent rounds put 1 distinct slot value. The reserve's ambiguous rounds also repeat bytes, on purpose (`stage.rs:875-876`).
  - The code already knows this (`stage.rs:1500-1501`) and settles by the entry's bytes, so there is no live bug today.
  - The risk: 657.1b adds renewal on top of these docs. A renewal settle step that trusts the claim and identifies its batch by slot bytes alone would adopt the wrong batch.

  Fix: reword to "later than the slot lease the batch replaces; retry rounds of one batch may repeat a value."
- NEEDS-HUMAN [impl] — **`reserve_slot` skips the request checks that `stage_part` applies, though (H) covers "the functions".** `reserve_slot` is at `stage.rs:839-856`; the checks in `stage_part` are at `stage.rs:1073-1084`. Two concrete cases:
  - `reserve_slot` with `lease_ttl_millis = 1` commits a slot whose lease is `stamp + 1`. That is exactly the request `stage_part` refuses as "would lapse at the next reading" (`stage.rs:1077`).
  - `max_inflight_parts = 0` answers `SlowDown` forever instead of a typed invalid-request error.

  `h_an_invalid_request_writes_nothing` (`multipart_upload_part_stage.rs:3600-3630`) only calls `stage_part`. The stakes are low (no production caller), and the fix is cheap: share one validator.

**Not raised, on purpose:** seeded DST coverage (tracked as deferred in #660 and #665), and the missing absolute landing bound for a write whose publishing step straddles the deadline. The brief already marks that bound for a human decision, and I checked that neither the code nor the docs claim it (`stage.rs:51-53`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:759` (body caller at `crates/core/src/multipart/stage.rs:1257`): **A late body response can bypass the idle deadline.** `bounded` checks the clock before waiting, but accepts a ready result on either wake branch without checking the clock again. If a pending body read becomes ready after its deadline and both it and the timer are ready when staging resumes, the late piece or EOF is accepted. For example, buffer a partial chunk, leave the next read pending, advance beyond the idle bound while keeping the lease live, then close the body and fire the timer before polling staging again: EOF wins and the buffered chunk is written instead of returning `BodyIdle`. The existing stalled-body test keeps the channel open until staging stops (`crates/core/tests/multipart_upload_part_stage.rs:1851`), so it misses this race. Enforce expiry when accepting body-read results and add this deterministic schedule; preserve the separate fragment-write rule that an acknowledged `Ok` already proves timely publication.

No separate reuse, simplification, or efficiency findings. Reviewed source and frozen gate evidence; no target changes or test reruns.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] leaf produced no usable verdict (needs a human) — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [ ] **No test covers a store read that fails while a commit is still in doubt, so C5's "103 caught, 0 missed" overstates the coverage.** Two hand mutants survive all 54 tests:
- [ ] **The error class of an unknown-effect write depends on timing, not on what landed.** In `a_prime_a_publication_straddling_the_deadline_is_never_counted`, both runs end with the same store state: the fragment is stored at a position the owned entry names. I printed `classify(&err)` for each: run 1 = `Indeterminate`, run 2 = `Terminal`.
- [ ] **The lease docs claim slot values never repeat, and they do.** `stage.rs:67-69` says "every batch puts a slot value no earlier batch of the attempt put", and `stage.rs:1128-1130` says the same about `next_expiry`.
- [ ] **`reserve_slot` skips the request checks that `stage_part` applies, though (H) covers "the functions".** `reserve_slot` is at `stage.rs:839-856`; the checks in `stage_part` are at `stage.rs:1073-1084`. Two concrete cases:
- [ ] `crates/core/src/multipart/stage.rs:759` (body caller at `crates/core/src/multipart/stage.rs:1257`): **A late body response can bypass the idle deadline.** `bounded` checks the clock before waiting, but accepts a ready result on either wake branch without checking the clock again. If a pending body read becomes ready after its deadline and both it and the timer are ready when staging resumes, the late piece or EOF is accepted. For example, buffer a partial chunk, leave the next read pending, advance beyond the idle bound while keeping the lease live, then close the body and fire the timer before polling staging again: EOF wins and the buffered chunk is written instead of returning `BodyIdle`. The existing stalled-body test keeps the channel open until staging stops (`crates/core/tests/multipart_upload_part_stage.rs:1851`), so it misses this race. Enforce expiry when accepting body-read results and add this deterministic schedule; preserve the separate fragment-write rule that an acknowledged `Ok` already proves timely publication.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The deadline invariant assumes a guarantee the landed backend explicitly does not provide.** `brief.md:199-200` promises “No fragment is published after its authorization deadline”; `brief.md:392` says “no store may apply them any more.” However, `crates/traits/src/lib.rs:1128-1141` explicitly permits publication to straddle expiry and return `WriteEffect::Unknown`, leaving any published bytes in place. The concrete filesystem backend performs the rename at `crates/chunkstore-fs/src/lib.rs:389` and only afterwards checks its timing at `:416-422`; dropping the caller's wait cannot establish non-publication. The fake specified at `brief.md:37-39` and the held-before-publication test at `:70-73` exercise only the definite-refusal case. Revise the invariant and error contract to preserve indeterminate write effects, and add a deterministic fake schedule that starts publication before expiry and finishes afterwards. Explicitly reconcile this with the orphan-grace safety claim at `brief.md:395-396`: retaining the absolute landing bound requires a prerequisite beyond this slice, whose scope excludes backend changes (`brief.md:242-243`). A green instantaneous-publication fake cannot certify that bound.
- [ ] **The required reds omit the intent's session fence.** The invariant at `brief.md:198-199` forbids owned entries after the session leaves `Open`, but the Abort-between-read-and-commit legs at `:108-110` exercise the reserve. The held-intent leg at `:166` requires stopping and clearing doubt, without requiring the held batch to be released afterwards and rejected. The mutation list at `:265-290` removes the drain precondition, but never the intent's `require(mpu == Open@E)`. This leaves the decisive safety edge without a required discriminator: the target proposal explicitly identifies post-fence intent creation as undiscoverable residue (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1774`). Add a schedule that reserves successfully, runs the landed Abort immediately before an intent commit, and asserts no new owned entry or fragment; also release an ambiguous held intent after Abort and assert its batch conflicts. Require removing only the intent's session precondition to turn that test red. These fit the existing wrapper and need no reaper implementation or deferred seeded-DST work.
- [ ] size backstop — this slice is behaving oversized: patch is 245 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): No advisory review exists (the codex reviewer exited 1 early with a codex-side error, not a memory kill); the next Check must produce one. Keep the design and slice boundary. Fix the five open review findings: 1. Add a read-fault hook to the test wrapper and cover a store read that fails while a commit is in doubt: one reserve leg, one intent leg (schedule: intent Hold, then Unknown{applied:false, may_still_commit:false}, then get(sidx:) faults -> InDoubt{may_still_commit:true}). Also cover bounded's post-timer now_or_never (stage.rs:762-764). 2. Make the error class of an unknown-effect stop independent of timing: use the most indeterminate report as source(), give an abandoned-only stop an Indeterminate source, and assert the class in run 2 of the straddle leg. 3. Fix the lease docs (stage.rs:67-69, :1128-1130): slot values are later than the lease they replace, but retry rounds of one batch may repeat a value. 4. Share one request validator between reserve_slot and stage_part (lease TTL < 2, max_inflight_parts = 0) and test reserve_slot in the (H) leg. 5. bounded must enforce the idle deadline when it accepts a ready body read (a late piece or EOF must not slip past it); add the deterministic schedule for that. Keep the rule that an acknowledged fragment Ok proves timely publication.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
