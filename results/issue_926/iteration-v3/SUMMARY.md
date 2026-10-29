# Result — issue 926 / multipart-upload-part-stage

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Sessions can be created (#894) and aborted (#895), but nothing stages a part's
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
- Success criterion: A NEW test file `crates/core/tests/multipart_upload_part_stage.rs`. It
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the first half of `UploadPart` as store-I/O functions in `wyrd-core`. That means
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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.4% — 884 of 917 instrumentable changed lines executed (floor 80%); 917 of 1863 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 143 mutants tested in 16m: 107 caught, 36 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.75s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #926: add UploadPart slot reservation, owned chunk staging, drain fencing and renewal; the deadline test double needs correction before its race evidence can be trusted.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The staging-only contract is falsifiable, and the later sign-off explicitly authorizes the body-idle bound and proposal amendment; no new scope decision is owed (`brief.md:25`, `brief.md:404`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality without a pre-existing staging API; reverting production yields missing-symbol errors rather than an executed failing test (`brief.md:206`, `review-red.log:2`). |
| C3 Change | PASS | The change stays within the authorized staging slice and preserves the shared drain-key bytes; the living architecture records the new API and absence of a production caller (`crates/core/src/metadata.rs:98`, `docs/design/architecture/05-building-block-view.md:244`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared net-new verification exception—stash/reapply reproduced a compile-only pre-fix failure and 61 passing post-fix tests, so a behavioral red→green is not established (`brief.md:213`, `review-red.log:2`, `review-green.log:68`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Require a deadline-enforcing Fleet and a truthful timing schedule—the current success test publishes 101 ms late when replayed against the real seam contract (`crates/core/tests/multipart_upload_part_stage.rs:894`, `crates/core/tests/multipart_upload_part_stage.rs:1589`, `review-deadline.log:8`). |
| T1 Structure | PASS | Core depends on the metadata/chunk-store traits and caller runtime; the shared key remains below the custodian in dependency direction, with no spawned renewal task (`crates/core/src/multipart/stage.rs:116`, `crates/core/src/multipart/stage.rs:199`, `crates/custodian/src/desired_state.rs:39`). |
| T2 Shape | PASS | The commit hand-off has complete ordered chunk references and exact record bytes, while slot construction uses the existing validating wire conversion (`crates/core/src/multipart/stage.rs:317`, `crates/core/src/multipart/stage.rs:347`, `crates/core/src/multipart.rs:2762`). |
| T3 Runtime | PASS | Inspected production paths use one caller clock, bounded commit rounds, and pending-I/O deadlines; the reproduced defect is in the test model, not an observed production deadline violation (`crates/core/src/multipart/stage.rs:94`, `crates/core/src/multipart/stage.rs:1195`, `crates/core/src/multipart/stage.rs:1470`). |
| T4 Contribution | FAIL | The frozen batch review has one unresolved test-fidelity defect reported three times, independently confirmed here; contribution-artifact auditing is separately deferred to publish (`gate-logs/T4-batch-review.log:10`, `AGENTS.md:188`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The capability-probe smell test does not fire; approved scope exceptions and tracked DST deferrals remain settled, and affected-path prior-art searches covered merged and closed work (`brief.md:270`, `brief.md:404`, `crates/core/src/multipart/stage.rs:103`, `review-prior-art.json:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this staging-only public hand-off is suitable for 657.2 after the test correction—no production UploadPart caller or end-to-end commit is part of this slice (`brief.md:242`, `crates/core/src/multipart/stage.rs:287`, `docs/design/architecture/05-building-block-view.md:244`). |

Source citations above are relative to `$PDCA_TARGET`; brief, gate-log and reviewer-log citations are relative to this report. The disposable target is usable, and its submitted files match `patch.diff` after the stash/reapply check (`review-target.log:19`). No builder notes were consulted.

- **One confirmed defect: the deadline double accepts an impossible success.** `Fleet::put_fragment_at` logs the deadline, waits on its semaphore, then inserts the fragment without enforcing that deadline (`crates/core/tests/multipart_upload_part_stage.rs:878`, `:891`, `:894`). The trait requires refusal at publication once the deadline is reached (`crates/traits/src/lib.rs:1085`). In `a_prime_a_write_answered_while_a_renewal_commits_is_taken`, the synchronous hook releases the permit and immediately advances the clock by 200 ms before the put can resume (`crates/core/tests/multipart_upload_part_stage.rs:1589`). Releasing a permit is not publication or an answer. In an isolated diagnostic copy, adding `WriteDeadlineExpired::if_elapsed` immediately before insertion makes this test fail: publication time `1005201`, deadline `1005100`, `NotApplied`. The complete diagnostic suite is **60 passed, 1 failed** (`review-deadline-suite.log:77`). The separate slow-renewal schedule waits for `fleet.stores(held)` before advancing time and still passes (`crates/core/tests/multipart_upload_part_stage.rs:1677`, `review-deadline.log:24`). Correct the double, and make the disputed success schedule actually publish before expiry; also assert that a held put released after expiry is refused without publication. The three frozen review messages collapse to this single finding; their stale line numbers 905/939 are not adopted.

- **The execution evidence supports the feature, with a specific limit on pre-fix verification.** Independently ran `cargo test -p wyrd-core --test multipart_upload_part_stage`, stashed production changes while keeping the new test, reran, restored the stash, and reran again. The original and restored suites pass 61/61; the pre-fix leg fails compilation with 13 missing-API errors and executes no tests (`review-red.log:82`, `review-green.log:68`). That matches `gate-logs/C4-verify.log:97`; it is not a behavioral red. The isolated deadline diagnostic changes no submitted source. The frozen diff-coverage log reports **884/917 instrumentable changed lines, 96.4%**, with 946 other changed lines unscored (`gate-logs/C4-diff-cov.log:637`). The frozen mutation log reports **143 tested: 107 caught, 36 unviable**, but supplies no individual named hand-mutation outcomes (`gate-logs/C5-mutants.log:13`); it does not establish model fidelity. TiKV/server feature compilation is evidenced by the real clippy output (`gate-logs/host-tikv.log:209`). Instance-scoped coverage, mutation and batch-review wrappers were adjudicated from their frozen logs, not treated as missing target tools. **T4 contribution artifacts: N/A** at Check because they are drafted later and the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`).

- **The build and required external tools were exercised; the local CI host caveat was resolved.** The frozen full gate passes (`gate-logs/C4-ci.log:4396`). My `cargo xtask ci` rerun completed all build, test, DST, fmt, clippy, conformance and structural checks successfully, but exited 1 because its two advisory scans could not lock the read-only home cache (`review-ci.log:3654`, `review-ci.log:4291`). Both failed scans then passed with freshly fetched advisory data in a writable directory under this review root (`review-deny.log:1`). The scratch configs differ only by `advisories.db-path`; no policy, suppression, source file or tool was substituted. Thus this is a resolved host caveat, not a C4 patch defect. `typos` and the real docs renderer both ran; rendering produced 99 pages with a successful link audit (`review-ci.log:2`, `review-ci.log:9`). Neither named external dependency remains unmet.

- **Scope and prior art were checked against the actual affected paths.** Queried each of the eight changed paths separately for merged and closed/unmerged PRs (`review-prior-art.json:1`), then checked changed-file lists for merged #793, #908, #909 and #822 and closed/unmerged #647 (`review-prior-art-details.json:1`). These corroborate the owned-entry, Create, Abort and drain predecessors in the brief. Also fetched #636: it is closed as `not_planned`, and its recorded scope includes staging in `crates/core/src/write.rs`, the drain fence and coupled renewal. The brief records its rejected schedules and #657's earlier iteration (`brief.md:276`); current tests explicitly distinguish the drain and same-index cases (`crates/core/tests/multipart_upload_part_stage.rs:1793`, `:2134`). The local repository has one synthetic base commit, so no full local history is claimed. Proposal 0016's amendment and patch-size waiver were already approved (`brief.md:404`); seeded staging races remain tracked in #660/#665 (`crates/core/src/multipart/stage.rs:103`). Neither is re-raised as a defect. No visual/manual application check applies to this API-only slice.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 926 (657.1, UploadPart staging)

The toolchain was available, so I re-ran the evidence instead of reading it: the new test file passes 61/61 on the patched tree. I also ran 33 mutants of my own (a mutant is a small deliberate code change, to see whether a test catches it), covering every red leg the brief demands plus the iteration-1 and iteration-2 fixes. All 33 were caught. The core logic held up against everything I threw at it. What I did break is one runtime bound, one test that passes for the wrong reason, the refusal answers the suite never checks, and one batch-size limit.

## Findings

- NEEDS-HUMAN [impl] — **Empty body pieces reset the body-idle bound, so a stalled client keeps its slot for ever.** `crates/core/src/multipart/stage.rs:1392-1396` re-arms the idle deadline on every read, and an empty piece is skipped at `:1401` but still counts as progress. Concrete case, run in scratch: `body_idle_millis = 10_000`, then ten `Bytes::new()` pieces, each sent 9,999 ms after the last. Staging stayed live for 99,990 ms with 10 renewals and zero bytes received, then returned `Staged`; with more pieces it never ends. This contradicts the module doc (`stage.rs:48-50`), the field doc (`:185-189`) and the iteration-2 sign-off ("a stalled client cannot hold its slot and leases forever"). HTTP/2 allows empty DATA frames (the CVE-2019-9518 "empty frames" class), so a production body can produce these. Fix: measure idle from the last non-empty piece (keep the old deadline when a piece is empty) and add this leg. A slow 1-byte trickle is a documented design choice and is not part of this finding.

- NEEDS-HUMAN [impl] — **The T4 blocking finding is real: `a_prime_a_write_answered_while_a_renewal_commits_is_taken` passes only because the double accepts a late write.** The renewal hook at `crates/core/tests/multipart_upload_part_stage.rs:1589-1593` releases the held put and then moves the clock 200 ms forward before the put task runs. `Fleet::put_fragment_at` (`:871-901`) then stores the fragment without checking `deadline_millis` (`:894-899`). When I made the double refuse `now > deadline`, as the D server does under #638, this leg failed 5 times out of 5: the put landed at 1,005,201 against a deadline of 1,005,100. So the leg shows a late write being kept, not an on-time one. Its sibling `a_prime_a_write_answered_while_a_slow_renewal_waits_is_taken` (`:1644`) does it correctly (it waits for `fleet.stores(held)` before moving the clock) and still passes with the stricter double, so the property is still covered and the production rule at `stage.rs:1464-1466` is not shown wrong. Fix: make the `Fleet` double refuse late puts, and in the `:1589` hook wait for the store to complete before advancing the clock.

- NEEDS-HUMAN [impl] — **The refusal answers are unguarded; three one-line swaps survive the whole suite.** (1) `stage.rs:1115`: changing `stage_part`'s reserve refusal to `Refused(NoSuchUpload)` keeps all 61 tests green. That is the brief's own invariant ("an `Open` session is never refused as `NoSuchUpload`", the #636 defect), and no leg calls `stage_part` with the slot range full; the (C) cap legs only call `reserve_slot`. (2) `stage.rs:998`: a session that leaves `Open` (say an Abort) between the first read and the reserve commit can be answered `SlowDown` (a retryable 503) instead of `NoSuchUpload`, and nothing fails. (3) `stage.rs:985`: the same swap on the pinned path also survives. `C4-diff-cov` already lists all three lines as MISS. C5's "0 missed" does not cover them, because cargo-mutants does not generate this kind of match-arm swap. Fix: add a `stage_part` leg with all `MAX_INFLIGHT_PARTS` slots held that expects `SlowDown { InflightParts }`, and a reserve leg where the session leaves `Open` between the session read and the reserve commit that expects `NoSuchUpload` (with a pinned variant).

- NEEDS-HUMAN [impl] — **The renewal batch has no size clamp.** It carries `3 + 2N` operations for N owned entries (`stage.rs:1221-1229`), but `knob_clamps_hold` only checks the part commit, `N + 8 ≤ B_ops` (`crates/core/src/multipart.rs:5515-5516`). Concrete case, run in scratch: `KnobSet { scheme: EcScheme::None, max_part_chunks: 249 | 300 | 400, ..DEPLOYED }` (re-derived) passes `knob_clamps_hold`, while the renewal grows to 501, 603 and 803 operations, past `MAX_BATCH_OPS = 500`. Per `multipart.rs:5157-5168`, a batch past that "cannot finish ... however often it is retried", so a long part could never renew its leases. The shipped profile (158 chunks, 319 operations) fits. `StageRequest::max_part_chunks` (`stage.rs:184`) and `max_inflight_parts` (`:167`) are not checked against anything either; the test default is 1,000. Fix: add a `2·max_part_chunks + 3 ≤ batch_ops` clamp, or reject oversize values in `stage_part`. Otherwise decline it with a tracking issue against #508/#625, which load the knobs.

## Refutation attempts that failed

- **The required red legs are real.** Each named leg fails under its edit: drain `require_absent` loop removed → B1; plan against the full topology → B1, B2, B3; slot rewrite dropped from renewal → 11 (D) legs; same-index pin removed → 4 (C) legs; last `ChunkRef.len = chunk_size` → (A); a piece spanning a chunk boundary merged into one chunk → 19 legs; deadline taken at dispatch, `None`, or `u64::MAX` → (A′); a fresh index probed after an ambiguous refusal (both exits) → (C) landed-after-refusal and held-index legs; lapse check removed from renewal or intent → (D); no renewal timer during a body read or a fragment write → (D-i), (D-ii); renewal not raced against the deadline, or writes not polled during a renewal → (A′); post-move re-reads removed from reserve, intent or renewal → each one's own leg. The precondition removals (owned `require`, session `require`, slot `require`, `sidx:` `require_absent`), the placement pre-check removal, and a drain read taken only once are all caught too.
- **The wrapper's held batches are faithful.** A held batch is committed through the inner redb store when released (`multipart_upload_part_stage.rs:634-643`), so its preconditions are checked again and the "at most one lands" legs prove something.
- **Could not break** the drain-key bytes (`crates/core/src/metadata.rs:102` uses the custodian's old format string), lease monotonicity and slot reuse (`stage.rs:1150-1154`: every lease is strictly later than the one it replaces), or lapsed-lease handling. A renewal is polled before the writes in `renewing`, so no fragment write is ever dispatched under a lapsed entry.

## On the verdict

- `C4-verify` UNVERIFIABLE is the pre-declared net-new case, not a red against the fix.
- `T4-batch-review` is a real failure, not reviewer noise (second finding above).
- A reviewer reading "C5: 0 survivors" plus "96.4% diff coverage" as proof that the refusal answers are checked would be wrong (third finding).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_stage.rs:894`: The Fleet double publishes fragments without enforcing `deadline_millis`, masking an invalid deadline-race schedule. At `crates/core/tests/multipart_upload_part_stage.rs:1589`, the renewal hook releases the held put and synchronously advances the clock by 200 ms. The put cannot resume until that hook returns, so it publishes at `T0 + HALF + 201`, after its `T0 + HALF + 100` deadline, yet the test expects staging to succeed at line 1618. Production must refuse this write (`crates/traits/src/lib.rs:1085`). Check the deadline immediately before the double's insertion using `WriteDeadlineExpired::if_elapsed`, and make the success schedule observe actual publication before advancing past the deadline; also assert that a put released after expiry is refused. This consolidates the three duplicate findings in the frozen T4 review log.

No additional correctness or reuse/simplification findings. Reviewed the target source and frozen gate evidence; no checks were rerun.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared net-new verification exception—stash/reapply reproduced a compile-only pre-fix failure and 61 passing post-fix tests, so a behavioral red→green is not established (`brief.md:213`, `review-red.log:2`, `review-green.log:68`).
- [ ] C5 Causal adequacy — Require a deadline-enforcing Fleet and a truthful timing schedule—the current success test publishes 101 ms late when replayed against the real seam contract (`crates/core/tests/multipart_upload_part_stage.rs:894`, `crates/core/tests/multipart_upload_part_stage.rs:1589`, `review-deadline.log:8`).
- [ ] Validation — fitness-to-purpose — Decide whether this staging-only public hand-off is suitable for 657.2 after the test correction—no production UploadPart caller or end-to-end commit is part of this slice (`brief.md:242`, `crates/core/src/multipart/stage.rs:287`, `docs/design/architecture/05-building-block-view.md:244`).
- [ ] **Empty body pieces reset the body-idle bound, so a stalled client keeps its slot for ever.** `crates/core/src/multipart/stage.rs:1392-1396` re-arms the idle deadline on every read, and an empty piece is skipped at `:1401` but still counts as progress. Concrete case, run in scratch: `body_idle_millis = 10_000`, then ten `Bytes::new()` pieces, each sent 9,999 ms after the last. Staging stayed live for 99,990 ms with 10 renewals and zero bytes received, then returned `Staged`; with more pieces it never ends. This contradicts the module doc (`stage.rs:48-50`), the field doc (`:185-189`) and the iteration-2 sign-off ("a stalled client cannot hold its slot and leases forever"). HTTP/2 allows empty DATA frames (the CVE-2019-9518 "empty frames" class), so a production body can produce these. Fix: measure idle from the last non-empty piece (keep the old deadline when a piece is empty) and add this leg. A slow 1-byte trickle is a documented design choice and is not part of this finding.
- [ ] **The T4 blocking finding is real: `a_prime_a_write_answered_while_a_renewal_commits_is_taken` passes only because the double accepts a late write.** The renewal hook at `crates/core/tests/multipart_upload_part_stage.rs:1589-1593` releases the held put and then moves the clock 200 ms forward before the put task runs. `Fleet::put_fragment_at` (`:871-901`) then stores the fragment without checking `deadline_millis` (`:894-899`). When I made the double refuse `now > deadline`, as the D server does under #638, this leg failed 5 times out of 5: the put landed at 1,005,201 against a deadline of 1,005,100. So the leg shows a late write being kept, not an on-time one. Its sibling `a_prime_a_write_answered_while_a_slow_renewal_waits_is_taken` (`:1644`) does it correctly (it waits for `fleet.stores(held)` before moving the clock) and still passes with the stricter double, so the property is still covered and the production rule at `stage.rs:1464-1466` is not shown wrong. Fix: make the `Fleet` double refuse late puts, and in the `:1589` hook wait for the store to complete before advancing the clock.
- [ ] **The refusal answers are unguarded; three one-line swaps survive the whole suite.** (1) `stage.rs:1115`: changing `stage_part`'s reserve refusal to `Refused(NoSuchUpload)` keeps all 61 tests green. That is the brief's own invariant ("an `Open` session is never refused as `NoSuchUpload`", the #636 defect), and no leg calls `stage_part` with the slot range full; the (C) cap legs only call `reserve_slot`. (2) `stage.rs:998`: a session that leaves `Open` (say an Abort) between the first read and the reserve commit can be answered `SlowDown` (a retryable 503) instead of `NoSuchUpload`, and nothing fails. (3) `stage.rs:985`: the same swap on the pinned path also survives. `C4-diff-cov` already lists all three lines as MISS. C5's "0 missed" does not cover them, because cargo-mutants does not generate this kind of match-arm swap. Fix: add a `stage_part` leg with all `MAX_INFLIGHT_PARTS` slots held that expects `SlowDown { InflightParts }`, and a reserve leg where the session leaves `Open` between the session read and the reserve commit that expects `NoSuchUpload` (with a pinned variant).
- [ ] **The renewal batch has no size clamp.** It carries `3 + 2N` operations for N owned entries (`stage.rs:1221-1229`), but `knob_clamps_hold` only checks the part commit, `N + 8 ≤ B_ops` (`crates/core/src/multipart.rs:5515-5516`). Concrete case, run in scratch: `KnobSet { scheme: EcScheme::None, max_part_chunks: 249 | 300 | 400, ..DEPLOYED }` (re-derived) passes `knob_clamps_hold`, while the renewal grows to 501, 603 and 803 operations, past `MAX_BATCH_OPS = 500`. Per `multipart.rs:5157-5168`, a batch past that "cannot finish ... however often it is retried", so a long part could never renew its leases. The shipped profile (158 chunks, 319 operations) fits. `StageRequest::max_part_chunks` (`stage.rs:184`) and `max_inflight_parts` (`:167`) are not checked against anything either; the test default is 1,000. Fix: add a `2·max_part_chunks + 3 ≤ batch_ops` clamp, or reject oversize values in `stage_part`. Otherwise decline it with a tracking issue against #508/#625, which load the knobs.
- [ ] `crates/core/tests/multipart_upload_part_stage.rs:894`: The Fleet double publishes fragments without enforcing `deadline_millis`, masking an invalid deadline-race schedule. At `crates/core/tests/multipart_upload_part_stage.rs:1589`, the renewal hook releases the held put and synchronously advances the clock by 200 ms. The put cannot resume until that hook returns, so it publishes at `T0 + HALF + 201`, after its `T0 + HALF + 100` deadline, yet the test expects staging to succeed at line 1618. Production must refuse this write (`crates/traits/src/lib.rs:1085`). Check the deadline immediately before the double's insertion using `WriteDeadlineExpired::if_elapsed`, and make the success schedule observe actual publication before advancing past the deadline; also assert that a put released after expiry is refused. This consolidates the three duplicate findings in the frozen T4 review log.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- [ ] **The stated slot invariant contradicts permitted residue.** `brief.md:140-141` says held slots “never outnumber its live attempts,” but target `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1716-1717` explicitly permits an ambiguous reserve to install a slot after the request has refused; `:1839-1842` requires crashed attempts' slots to remain and count against admission. `crates/core/src/multipart.rs:4467-4469` encodes the same rule. The brief's own bounded-refusal case (`brief.md:82-83`) can therefore leave one slot and zero live attempts. Revise the invariant to prohibit duplicate slots per attempt while counting residue toward the cap; restrict live-count equality to the no-residue schedule. Add the held-reserve landing after refusal to that criterion, without introducing cleanup into this slice.
- [ ] **Between-piece renewal does not establish the promised whole-request liveness.** `brief.md:87-91` promises leases never lapse while data is arriving, but `brief.md:279-286` prescribes renewal inline between body polls without specifying a timer/wakeup. The cited peer awaits `source.next()` at `crates/core/src/write.rs:583` and awaits chunk work at `:592-604`; the existing `Clock` seam only reads time (`crates/testkit/src/lib.rs:23-26`). A body read or fragment write can remain pending across the renewal deadline, preventing another inline check. Advancing the clock between delivered pieces, as D specifies, does not test that window. Specify how renewal gets polled independently of pending I/O, using an injected timer/tick if necessary, and require a deterministic test that observes renewal while the next body piece is still withheld, plus delayed fragment I/O. Otherwise narrow the liveness promise and state the required progress bounds.
- [ ] **The enumerated handoff cannot build the promised part record from itself alone.** `brief.md:297-302` lists owned keys/bytes/placements and total part length, but omits each chunk's logical length. `OwnedEntry` and `StagedPlacement` do not retain that information (`crates/core/src/multipart.rs:4136-4139`, `:4045-4047`), whereas `PartRecord` contains ordered `ChunkRef`s (`:2930-2935`) and every `ChunkRef` requires `len` (`crates/core/src/metadata.rs:351-361`). Total length alone does not supply the chunk boundaries; the handoff does not even list the configured chunk size. Require ordered complete chunk references, including the short final chunk's length, and assert those fields in A. Otherwise 657.2 must revisit staging or reconstruct information the brief explicitly promises it will receive.
- [ ] size backstop — this slice is behaving oversized: patch is 271 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): The slice is too big: 271 KB against a 100 KB limit, three rounds, and implementation-level findings every round. Split it in the re-plan (`pdca split 926`, then `--accept`) instead of rebuilding it whole again. The staging core held up (CI green, 96.4% diff coverage, every mutant caught, including the adversary's 33 hand-made mutants), so keep its design. Carry these confirmed findings into whichever child brief owns the code: - Test double: the Fleet fake must refuse a fragment put published after its deadline (`WriteDeadlineExpired::if_elapsed` just before insert). The `:1589` renewal hook must wait for the store to finish before moving the clock. Add a leg showing a held put released after expiry is refused and not published. - Body-idle bound: an empty body piece must not re-arm the idle deadline (stage.rs:1392-1401); measure idle from the last non-empty piece. Add a leg with only empty pieces. - Refusal answers: add a stage_part leg with every MAX_INFLIGHT_PARTS slot held, expecting SlowDown{InflightParts}, and a reserve leg where the session leaves Open between the read and the commit, expecting NoSuchUpload, plus a pinned variant (stage.rs:985, :998, :1115). - Renewal batch size: the 3 + 2N ops renewal batch has no limit against MAX_BATCH_OPS; add a `2*max_part_chunks + 3 <= batch_ops` check (or reject oversize knobs). The re-plan decides whether this goes in a child or becomes a follow-up against #508/#625.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
