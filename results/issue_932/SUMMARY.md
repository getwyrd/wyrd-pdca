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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.0% — 833 of 859 instrumentable changed lines executed (floor 80%); 859 of 1813 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 146 mutants tested in 15m: 112 caught, 34 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_932/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.17s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #932: implement core `UploadPart` staging with slot reservation, owned chunk intents, drain/session fences, and bounded fragment writes and body reads.

No builder-fixable implementation defect was found. Independent tests and fault injections support the staging safety claims; human sign-off remains necessary for the new-feature verification basis, prior-art currency, and fitness-to-purpose including the documented publication/GC gap.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The safety invariant and acceptance schedules are falsifiable, and renewal, commit, reclamation and production wiring are explicitly outside this slice (`brief.md:232`, `brief.md:263`). |
| C2 Reproduction (red pre-fix) | N/A | This adds an API absent from the base, rather than repairing an existing callable behavior; the independently reproduced compilation failure is assessed under C4, not counted as a behavioral red (`brief.md:287`, `review-red.log:176`). |
| C3 Change | PASS | The eight changed paths stay within the authorized slice; existing drain-key bytes and the custodian API remain compatible, and the new writer is documented without adding a production caller (`crates/core/src/metadata.rs:102`, `crates/custodian/src/desired_state.rs:39`, `docs/design/architecture/05-building-block-view.md:246`, `review-target-state.log:17`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept post-fix execution plus mutation reds as this new feature's verification basis — the retained test cannot compile with production changes stashed, so no behavioral pre-fix red exists; restored tests and full CI pass independently (`review-red.log:176`, `review-final-green.log:66`, `review-ci.log:4429`, `brief.md:298`). |
| C5 Causal adequacy | PASS | The tests distinguish the required safety behavior from fence, slot-pin, lease, ambiguity, deadline and hand-off regressions: all 13 independent fault injections failed executing tests, then all 59 tests passed after restoration; no capability-probe/load-time symptom guard was introduced (`review-mutations.json:1`, `crates/core/tests/multipart_upload_part_stage.rs:3935`, `review-final-green.log:66`). |
| T1 Structure | PASS | The core implementation retains narrow metadata/placement seams and caller-owned time; sharing the drain-key definition preserves dependency direction rather than making core depend on the custodian (`crates/core/src/multipart/stage.rs:247`, `crates/core/src/multipart/stage.rs:1188`, `crates/custodian/src/desired_state.rs:28`). |
| T2 Shape | PASS | The commit hand-off retains exact record bytes and complete ordered chunk references, including the short final length; slot construction preserves decoder invariants and round-trip identity (`crates/core/src/multipart.rs:2757`, `crates/core/tests/multipart_upload_part_stage.rs:511`, `crates/core/tests/multipart_upload_part_stage.rs:1484`). |
| T3 Runtime | PASS | One supplied clock governs leases and waits; retry counts terminate, late body answers fail closed, and abandoned/unverified publications retain indeterminate status; metadata awaits rely on the backend's explicit termination contract (`crates/core/src/multipart/stage.rs:860`, `crates/core/src/multipart/stage.rs:1010`, `crates/core/src/multipart/stage.rs:1490`, `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | N/A | Contribution text is intentionally not drafted at Check; its substantive audit is owed to the publish-time rerun, so this deferred gate is neither a pass nor a missing-evidence finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the affected-path prior-art review remains current at base `582da99` — the brief records merged and rejected work checked at `4bac538`, but the supplied checkout has one synthetic commit and no remote, preventing independent verification of intervening work (`brief.md:386`, `review-target-state.log:3`, `review-target-state.log:7`, `review-target-state.log:11`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated staging-only hand-off meets this slice's purpose, and record ownership/disposition of the post-grace publication gap before multipart verbs ship — a publication begun in time can finish after the deadline and after its GC evidence is reclaimed (`brief.md:443`, `crates/traits/src/lib.rs:1128`, `crates/core/tests/multipart_upload_part_stage.rs:1859`). |

The independent evidence supports the implementation, within the declared boundary:

- **Full CI passed:** `cargo xtask ci` completed successfully, including formatting, clippy, build, workspace tests, dependency audits, conformance, static-state checks and the deterministic simulation suite (`review-ci.log:4429`). Both named external dependencies were exercised: `typos` ran, and the renderer generated 99 pages with a successful link audit (`review-ci.log:2`, `review-ci.log:7`). No unmet tool dependency is inferred from these checks.
- **The pre-fix limitation was reproduced:** production changes were stashed while retaining the new test. Cargo exited 101 on missing `multipart::stage` and the new drain-key helpers, without executing a test (`review-red.log:176`, `review-red.log:262`). Restoring the patch produced 59 passing tests (`review-restored-green.log:68`). This agrees with the frozen verifier's explicit UNVERIFIABLE verdict (`gate-logs/C4-verify.log:103`), not a confirmed verification defect.
- **Targeted behavioral reds were reproduced:** independent temporary mutations disabled the drain fence, drain-aware planning, intent session fence, same-index pin, pinned-index reread after an epoch change, lease checks, retention of an earlier held outcome, entry reread after the slot changed, authorization-time deadline, abandoned-write classification, aggregate-error precedence, late-body rejection and final-chunk length. Each compiled and failed executing tests; exact edits and results are in `review-mutations.py`, `review-mutations.json:1` and the corresponding `review-mutant-*.log` files. After restoration the complete focused suite passed (`review-final-green.log:66`). These are 13 independently reproduced mutations, not a claim to have rerun every mutation named in the brief.

The remaining frozen gate results were adjudicated from their supplied logs. Instance-scoped wrappers were not supplied in this target; their absence is not a defect:

- **C4-diff-cov — PASS, frozen evidence:** 833 of 859 instrumentable changed lines executed, 97.0%; 954 changed lines were unscored. The log explicitly notes the contribution of the custodian's existing suite (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:628`).
- **C5-mutants — PASS, frozen evidence:** the baseline passed; 146 mutants comprised 112 caught and 34 unviable, with none reported surviving. This full campaign was not rerun independently (`gate-logs/C5-mutants.log:10`).
- **T4-batch-review — PASS, frozen summary:** the supplied output reports zero blocking findings. It contains no detailed prior-art search evidence (`gate-logs/T4-batch-review.log:10`).
- **host-tikv — PASS, frozen compilation evidence:** both feature-enabled clippy commands finished successfully. This establishes compilation, not a live TiKV topology test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:111`, `gate-logs/host-tikv.log:209`).
- **T4-contribution — N/A:** the log explicitly defers the absent PR description to the substantive publish-time audit (`gate-logs/T4-contribution.log:10`).

All source citations above resolve under `$PDCA_TARGET`; evidence citations resolve in this review directory. The target was readable, the patch reverse-application check passed, and all temporary source mutations were restored. The path-based prior-art investigation returned only the synthetic base commit and no configured remote; the brief does document both merged and rejected/iterated predecessors. No `INTEGRATION.md` was present (`review-target-state.log:22`). The standing rubric was applied; the explicitly tracked seeded-race deferrals remain settled under #660/#665 (`crates/core/src/multipart/stage.rs:129`). No production `stage_part`/`reserve_slot` caller was found (`review-target-state.log:17`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #932 (657.1a, UploadPart staging core)

The toolchain was present. I copied the patched tree into scratch, built
`multipart_upload_part_stage` (59/59 green, as in `gate-logs/C4-ci.log:1251-1314`), and
applied hand-made mutations, one at a time, restoring the file after each. The core claims
hold. I found two untested branches, one case where the answer depends on read timing, one
input the validator lets through, and one gate verdict I don't trust.

## Findings

- NEEDS-HUMAN [impl] — **A refusal-answer swap survives the whole suite** (the class the brief says sank v3).
  At `crates/core/src/multipart/stage.rs:1655-1662`, a held intent that landed before the
  session moved to a **non-`Open`** state is answered `Refused(NoSuchUpload)` and writes no
  fragment (the comment at `:1659-1660` says so). I changed `:1661` to
  `let _ = self.footprint.moved_on(moved, &mut doubt);`. All 59 tests still passed.
  Concrete schedule: `Act::Hold{advance:0}` on the first intent, then
  `after_read(slot:<id>:0, skip 0, [ReleaseHeld, Abort])`, one-chunk body. Unpatched code:
  `Refused(NoSuchUpload)`, 0 puts. Mutant: `Staged`, 6 fragment puts, session `Aborting`.
  The only test of this branch,
  `e_a_held_intent_landing_before_the_session_moves_is_adopted`
  (`crates/core/tests/multipart_upload_part_stage.rs:3745-3782`), moves the session to a
  later `Open` epoch, so the `Session::Refused` arm is never taken. Fix: add the Abort run of
  that leg, and assert `NoSuchUpload`, no puts, and the owned entry left as residue.

- NEEDS-HUMAN [impl] — **For one store history, the answer depends on which read sees the Abort first.**
  The slot-read adopt path at `stage.rs:1623-1639` adopts the landed intent and writes its
  fragments without looking at the session. Schedule: same hold, but hook the **entry** read
  (`after_read(sidx:<id>:<n>:1, skip 0, [ReleaseHeld, Abort])`). Result: `Staged`, 6 puts,
  session `Aborting`. The schedule above (Abort seen by the session read) gives `NoSuchUpload`,
  0 puts. This does not break C-1: the entry is durable before any put, and the reaper's
  full-placement orphan-mark covers late writes. But the rule at `:1659-1660` ("under a session
  no longer `Open` this is the refusal, and no fragment of the chunk is written") is only true on
  one of the two paths. Pick one rule: either re-judge the session after the `:1638` adopt, or
  reword `:1659-1660`, which overclaims. Then test both paths.

- NEEDS-HUMAN [impl] — **The reserve's `doubt.settle()` is untested.** I deleted `stage.rs:1075`
  and all 59 tests passed. Concrete case: the reserve answers
  `Unknown{applied:false, may_still_commit:false}` at index 0. The slot read finds it absent,
  and the reserve probes index 1. Then a store fault on the next `scan(slot:<id>:)` should be
  `Stopped(Store)`. The mutant reports `InDoubt{may_still_commit:false}` for a batch a read has
  already settled. The intent has this leg (`tests/...:3560`). The reserve does not.
  Low severity: over-reporting doubt is the safe direction.

- NEEDS-HUMAN [impl] — **`max_part_chunks: 0` passes `StageRequest::check`** (`stage.rs:216-244`).
  That breaks the check's own rule ("a field no staging can honour … before anything is read or
  written"). Tried: a 64-byte body with `max_part_chunks = 0` reserves a slot (1 reserve commit,
  1 `slot:` record) and returns `Ok(Staged)` with 0 chunks, `len 0`, `chunk_bound_exceeded`
  true (`:1384`). It should be `Stop::InvalidRequest` with nothing written, and the (H) leg
  should cover it.

- NEEDS-HUMAN [human] — **An over-limit body comes back under the success variant.** (G) is met
  as the brief words it. But `StageOutcome::Staged` is documented as "the body is staged: every
  chunk is registered" (`stage.rs:267`). With `chunk_bound_exceeded` set, it holds a **prefix**
  whose `len()`/`digest()` describe only that prefix (`:398-410`). So a #927 commit that forgets
  to check the flag publishes a truncated part with a valid-looking digest. The rubric's
  protocol-input class says oversize input is "an error — never silently accepted". Should the
  overrun be its own outcome variant carrying the attempt? That would still satisfy (G) and
  would stop a caller from skipping the check. This is an API-shape call for the #927 owner. At
  minimum, the doc at `:267` is wrong for this case.

- NEEDS-HUMAN [human] — **The gating T4 review verdict looks vacuous.** `check-gates.json`'s
  `T4-batch-review` row is `gating: true, result: pass`, with "0 blocking, 0 recorded-rejected,
  0 noise-dropped" in **21.18 s** (`gate-logs/T4-batch-review.log`). That is supposed to be
  three codex passes over a 6,251-line, 284 KB diff. The brief's carry-forward records that last
  round's codex reviewer "exited 1 early". Zero findings *and* zero noise in 21 s looks like a
  review that never read the diff, not a clean one. Check `review-batch.md` before counting this
  row toward the definition of done.

## Claims I tried to refute and could not

- **C4-verify `unverifiable`**: this is the brief's pre-declared exit 77
  (`gate-logs/C4-verify.log`, a compile failure on the reverted base). I re-ran the brief's
  required reds that history says survived before. Each went red on the leg its label names:
  - per-server `require_absent(desired:dserver:<S>)` removed → B1
  - planning on the full topology → B2 (and B1/B3)
  - `stage_part` full-range refusal swapped to `NoSuchUpload` → `c_stage_part_with_every_slot_held_answers_slow_down`
  - session-left-`Open` answered `SlowDown`, plain path `:1113` and pinned path `:1100` → the two (C) session legs
  - same-index pin removed → five (C) legs
  - intent's `require(mpu == Open@E)` removed → E′1, E′2 and three more
  - an empty piece re-arming the idle deadline → `a_double_prime_empty_pieces_do_not_count_as_progress`
  - the fake's publication-point check removed → the late-put and refused-as-late legs
  - a late body answer accepted → `a_double_prime_a_body_answer_seen_past_its_idle_deadline_is_late`
  - the pre-dispatch check `>=` made `>` → `a_prime_a_deadline_passed_before_dispatch_sends_nothing`

  The red→green evidence is real and runs the production functions. The fake enforces the
  deadline at its publication point (`tests/...:948-955`), and the fake's puts run in their own
  task, so an abandoned put really does finish later.
- **Silently dropped fragments via `.zip(placement)`** (`stage.rs:1471-1481`): not possible.
  `select_distinct_domains` returns exactly `n` or errors (`crates/core/src/placement.rs:217-244`).
- **Fragment deadline later than `t_auth + W`**: an adopted round uses its own `t_auth`. Two
  rounds can only put identical entry bytes at the same clock reading, so the first-match
  `position` (`:1612`, `:1634`, `:1655`) never picks a later deadline.
- **Reserve giving one attempt two slots**: after a session move the pin is dropped
  (`:1107-1110`), but only after a second read of the pinned index (`:1090-1105`). Every
  in-flight batch required the old session bytes, so a probe elsewhere is safe.
- **`parse_desired_dserver_key` accepts `+5` / `05`** (`crates/core/src/metadata.rs:107-113`).
  It was moved verbatim from the custodian, and the staging planner only errs toward *excluding*
  a server. Old code, not raised.
- **`write_window_millis` ≥ `lease_ttl_millis` is not rejected**: a fragment can then publish
  after its owned lease lapses. Teardown orphan-marks the full staged placement (0016 X49), so
  the fragment is still evidenced. Not a C-1 break.
- **No seeded DST coverage**: deferred to #660/#665 in the module doc (`stage.rs:129-131`).
  That deferral is settled under the rubric. Not raised.

All scratch work was done in `$PDCA_SCRATCH/pdca-adversary-932-mut`. The target was not changed:
`stage.rs` diffs clean against `$PDCA_TARGET`.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: no introduced correctness bug or actionable reuse, simplification, or efficiency issue found.

Reviewed the affected source at `$PDCA_TARGET`, including reserve/intent reconciliation, session and drain fences, lease checks, fragment and body deadlines, error classification, and the test harness. Validation evidence is the supplied frozen gate logs; no builds or tests were rerun. Those logs report passing CI, 59 passing staging tests, 97.0% diff coverage, and 112 caught / 34 unviable mutants with no survivors. The pre-fix test run is explicitly unverifiable because the new staging API is absent on the base, as anticipated in the brief.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept post-fix execution plus mutation reds as this new feature's verification basis — the retained test cannot compile with production changes stashed, so no behavioral pre-fix red exists; restored tests and full CI pass independently (`review-red.log:176`, `review-final-green.log:66`, `review-ci.log:4429`, `brief.md:298`).
- [x] T5 Judgment — Confirm the affected-path prior-art review remains current at base `582da99` — the brief records merged and rejected work checked at `4bac538`, but the supplied checkout has one synthetic commit and no remote, preventing independent verification of intervening work (`brief.md:386`, `review-target-state.log:3`, `review-target-state.log:7`, `review-target-state.log:11`).
- [x] Validation — fitness-to-purpose — Decide whether the demonstrated staging-only hand-off meets this slice's purpose, and record ownership/disposition of the post-grace publication gap before multipart verbs ship — a publication begun in time can finish after the deadline and after its GC evidence is reclaimed (`brief.md:443`, `crates/traits/src/lib.rs:1128`, `crates/core/tests/multipart_upload_part_stage.rs:1859`).
- [x] **A refusal-answer swap survives the whole suite** (the class the brief says sank v3). At `crates/core/src/multipart/stage.rs:1655-1662`, a held intent that landed before the session moved to a **non-`Open`** state is answered `Refused(NoSuchUpload)` and writes no fragment (the comment at `:1659-1660` says so). I changed `:1661` to `let _ = self.footprint.moved_on(moved, &mut doubt);`. All 59 tests still passed. Concrete schedule: `Act::Hold{advance:0}` on the first intent, then `after_read(slot:<id>:0, skip 0, [ReleaseHeld, Abort])`, one-chunk body. Unpatched code: `Refused(NoSuchUpload)`, 0 puts. Mutant: `Staged`, 6 fragment puts, session `Aborting`. The only test of this branch, `e_a_held_intent_landing_before_the_session_moves_is_adopted` (`crates/core/tests/multipart_upload_part_stage.rs:3745-3782`), moves the session to a later `Open` epoch, so the `Session::Refused` arm is never taken. Fix: add the Abort run of that leg, and assert `NoSuchUpload`, no puts, and the owned entry left as residue.
- [x] **For one store history, the answer depends on which read sees the Abort first.** The slot-read adopt path at `stage.rs:1623-1639` adopts the landed intent and writes its fragments without looking at the session. Schedule: same hold, but hook the **entry** read (`after_read(sidx:<id>:<n>:1, skip 0, [ReleaseHeld, Abort])`). Result: `Staged`, 6 puts, session `Aborting`. The schedule above (Abort seen by the session read) gives `NoSuchUpload`, 0 puts. This does not break C-1: the entry is durable before any put, and the reaper's full-placement orphan-mark covers late writes. But the rule at `:1659-1660` ("under a session no longer `Open` this is the refusal, and no fragment of the chunk is written") is only true on one of the two paths. Pick one rule: either re-judge the session after the `:1638` adopt, or reword `:1659-1660`, which overclaims. Then test both paths.
- [x] **The reserve's `doubt.settle()` is untested.** I deleted `stage.rs:1075` and all 59 tests passed. Concrete case: the reserve answers `Unknown{applied:false, may_still_commit:false}` at index 0. The slot read finds it absent, and the reserve probes index 1. Then a store fault on the next `scan(slot:<id>:)` should be `Stopped(Store)`. The mutant reports `InDoubt{may_still_commit:false}` for a batch a read has already settled. The intent has this leg (`tests/...:3560`). The reserve does not. Low severity: over-reporting doubt is the safe direction.
- [x] **`max_part_chunks: 0` passes `StageRequest::check`** (`stage.rs:216-244`). That breaks the check's own rule ("a field no staging can honour … before anything is read or written"). Tried: a 64-byte body with `max_part_chunks = 0` reserves a slot (1 reserve commit, 1 `slot:` record) and returns `Ok(Staged)` with 0 chunks, `len 0`, `chunk_bound_exceeded` true (`:1384`). It should be `Stop::InvalidRequest` with nothing written, and the (H) leg should cover it.
- [x] **An over-limit body comes back under the success variant.** (G) is met as the brief words it. But `StageOutcome::Staged` is documented as "the body is staged: every chunk is registered" (`stage.rs:267`). With `chunk_bound_exceeded` set, it holds a **prefix** whose `len()`/`digest()` describe only that prefix (`:398-410`). So a #927 commit that forgets to check the flag publishes a truncated part with a valid-looking digest. The rubric's protocol-input class says oversize input is "an error — never silently accepted". Should the overrun be its own outcome variant carrying the attempt? That would still satisfy (G) and would stop a caller from skipping the check. This is an API-shape call for the #927 owner. At minimum, the doc at `:267` is wrong for this case.
- [x] **The gating T4 review verdict looks vacuous.** `check-gates.json`'s `T4-batch-review` row is `gating: true, result: pass`, with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **21.18 s** (`gate-logs/T4-batch-review.log`). That is supposed to be three codex passes over a 6,251-line, 284 KB diff. The brief's carry-forward records that last round's codex reviewer "exited 1 early". Zero findings *and* zero noise in 21 s looks like a review that never read the diff, not a clean one. Check `review-batch.md` before counting this row toward the definition of done.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The deadline invariant assumes a guarantee the landed backend explicitly does not provide.** `brief.md:199-200` promises “No fragment is published after its authorization deadline”; `brief.md:392` says “no store may apply them any more.” However, `crates/traits/src/lib.rs:1128-1141` explicitly permits publication to straddle expiry and return `WriteEffect::Unknown`, leaving any published bytes in place. The concrete filesystem backend performs the rename at `crates/chunkstore-fs/src/lib.rs:389` and only afterwards checks its timing at `:416-422`; dropping the caller's wait cannot establish non-publication. The fake specified at `brief.md:37-39` and the held-before-publication test at `:70-73` exercise only the definite-refusal case. Revise the invariant and error contract to preserve indeterminate write effects, and add a deterministic fake schedule that starts publication before expiry and finishes afterwards. Explicitly reconcile this with the orphan-grace safety claim at `brief.md:395-396`: retaining the absolute landing bound requires a prerequisite beyond this slice, whose scope excludes backend changes (`brief.md:242-243`). A green instantaneous-publication fake cannot certify that bound.
- [x] **The required reds omit the intent's session fence.** The invariant at `brief.md:198-199` forbids owned entries after the session leaves `Open`, but the Abort-between-read-and-commit legs at `:108-110` exercise the reserve. The held-intent leg at `:166` requires stopping and clearing doubt, without requiring the held batch to be released afterwards and rejected. The mutation list at `:265-290` removes the drain precondition, but never the intent's `require(mpu == Open@E)`. This leaves the decisive safety edge without a required discriminator: the target proposal explicitly identifies post-fence intent creation as undiscoverable residue (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1774`). Add a schedule that reserves successfully, runs the landed Abort immediately before an intent commit, and asserts no new owned entry or fragment; also release an ambiguous held intent after Abort and assert its batch conflicts. Require removing only the intent's session precondition to turn that test red. These fit the existing wrapper and need no reaper implementation or deferred seeded-DST work.
- [x] size backstop — this slice is behaving oversized: patch is 278 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #932 follow-up: held intent + Abort → NoSuchUpload path untested (mutant at multipart/stage.rs:1661 survives); add the Abort leg
- #932 follow-up: adopt-via-entry-read path ignores session (stage.rs:1623-1639) vs comment :1659-1660; pick one rule, test both
- #932 follow-up: reserve doubt.settle() (stage.rs:1075) untested
- #932 follow-up: max_part_chunks=0 passes StageRequest::check; should be InvalidRequest
- #927 owner: over-limit body returns StageOutcome::Staged with chunk_bound_exceeded; consider its own variant; doc at stage.rs:267 is wrong
- #932 follow-up: post-grace publication gap (traits/src/lib.rs:1128) — assign owner before multipart verbs ship
- #932 follow-up: required reds omit the intent's session fence
- harness: T4-batch-review passed with 0/0/0 in 21s on a 284 KB diff; looks like codex never reviewed — gate should fail on an empty run
