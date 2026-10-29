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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.5% — 786 of 823 instrumentable changed lines executed (floor 80%); 823 of 1669 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 132 mutants tested in 12m: 98 caught, 34 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.37s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

UploadPart staging (slot reservation, owned chunk intents, drain fencing and lease renewal) has one confirmed deadline defect requiring builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The staging-only boundary and falsifiable ordering, ambiguity, renewal and chunk-limit requirements are explicit; the deadline obligation is normative (`brief.md:35`, `brief.md:186`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1553`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no existing staging operation to reproduce; retaining the new tests while stashing production changes gives missing-API compilation errors, not a behavioral red (`brief.md:206`, `reviewer-red.log:2`). |
| C3 Change | PASS | The seven affected files stay within the authorized slice, preserve the custodian key bytes and update the living architecture; no production staging caller was found (`crates/custodian/src/desired_state.rs:39`, `docs/design/architecture/05-building-block-view.md:244`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared net-new verification posture — 54 tests pass after restoration, but the pre-fix discriminator cannot execute, so a behavioral red→green is unproved (`reviewer-restored-green.log:61`, `reviewer-red.log:2`, `brief.md:213`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Repair R1 before relying on the deadline guarantee — an in-flight metadata renewal prevents staging from responding at the fragment deadline (`crates/core/src/multipart/stage.rs:1334`, `reviewer-deadline-probe.log:3`). |
| T1 Structure | PASS | Shared encoding and drain-key ownership preserve dependency direction; store operations remain in the staging child module with caller-supplied time (`crates/core/src/write.rs:140`, `crates/core/src/metadata.rs:98`, `crates/core/src/multipart/stage.rs:175`). |
| T2 Shape | PASS | The hand-off exposes ordered complete chunk references and exact CAS bytes; typed errors retain ambiguous outcomes, and slot construction shares decode validation (`crates/core/src/multipart/stage.rs:295`, `crates/core/src/multipart/stage.rs:325`, `crates/core/src/multipart/stage.rs:632`, `crates/core/src/multipart.rs:2768`). |
| T3 Runtime | FAIL | A finite metadata delay can extend the pending fragment wait beyond its authorization deadline; bounded metadata termination alone does not satisfy the earlier fragment deadline (`crates/core/src/multipart/stage.rs:1109`, `crates/core/src/multipart/stage.rs:1334`, `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | FAIL | The recorded review gate still has its one unresolved finding, independently confirmed as R1; affected-path prior art was checked, while contribution-artifact auditing is N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.log:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add the missing R1 regression schedule — the current deadline test explicitly prevents renewal being due, so it cannot detect the reproduced defect (`crates/core/tests/multipart_upload_part_stage.rs:1363`, `reviewer-deadline-probe.log:16`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this staging-only hand-off is suitable for the next commit slice after R1 is resolved — this slice exercises neither part publication nor a production UploadPart caller (`brief.md:241`, `crates/core/src/multipart/stage.rs:15`). |

Repository citations above and below resolve beneath `$PDCA_TARGET`; input and evidence citations resolve in the review directory. The supplied patch passes `git apply --reverse --check` against that target. The stash/pop reproduction restored the six stashed production/docs files byte-for-byte, while the new test stayed in place; no implementation or original test was edited.

**R1: Keep the fragment deadline active during renewal.** At `crates/core/src/multipart/stage.rs:1334`, `renewing` awaits `renew_if_due` before checking the deadline or installing the next timer race. That renewal awaits metadata at `crates/core/src/multipart/stage.rs:1109`. When renewal becomes due before a pending write's deadline, staging cannot observe the deadline until metadata returns. This violates both the module's promised bounded wait (`crates/core/src/multipart/stage.rs:29`) and the standing await rule (`AGENTS.md:181`). The impact demonstrated here is delayed termination and retained request work; this probe does not claim that a conforming D-server accepts an expired write.

The independent probe uses the existing real redb fixture, a held fragment, a 10,000 ms lease and a 6,000 ms write window. At 5,000 ms it fires renewal and holds that metadata commit. At 6,000 ms it fires the deadline timer. Staging remains pending until the finite metadata delay is released, then returns `WriteDeadlineElapsed`. The assertion fails against the unchanged patch (`reviewer-deadline-probe.log:3`). The metadata wrapper also has a five-second timeout, so the counterexample does not require a nonterminating backend. Re-run the preserved probe from this directory:

```sh
./pdca-reviewer-926-scratch/reviewer_deadline_probe --exact reviewer_fragment_deadline_must_remain_active_during_renewal --nocapture
```

Keep the deadline effective across the renewal await and add this interleaving to the patch's tests. Preserve conservative unknown-outcome reporting if a deadline interrupts a metadata commit. The existing regression at `crates/core/tests/multipart_upload_part_stage.rs:1364` asserts `W + 1 < HALF`; the separate pending-write renewal test observes a completed renewal before releasing the fragment (`crates/core/tests/multipart_upload_part_stage.rs:2209`, `crates/core/tests/multipart_upload_part_stage.rs:2228`). Neither covers the failing combination. C5, T3 and T5 describe this one defect and its missing regression, not three independent defects. The frozen review's old line citation resolves to the current `stage.rs:1334`.

**Verification supports the other exercised behavior, with an explicit pre-fix limitation.** Independent `cargo test --offline -p wyrd-core --test multipart_upload_part_stage` runs passed all 54 tests both before the stash and after restoration. The stashed-production run exited 101 on the absent `multipart::stage` import and drain-key APIs, matching the frozen UNVERIFIABLE result (`gate-logs/C4-verify.log:97`). It never executed a failing test.

- **C4-ci:** the frozen run completed successfully (`gate-logs/C4-ci.log:4391`). The independent full `cargo xtask ci` run completed compilation, tests, conformance, formatting, linting, dependency-use checks, statics/deployment scans, spelling and docs rendering. Its only two failures were advisory-cache lock attempts on a read-only path (`reviewer-ci.log:3647`, `reviewer-ci.log:3652`). Both exact-policy scans then passed using config copies differing only in the advisory database cache path, inside this sandbox (`reviewer-advisories.log:1`). This is a resolved reviewer-host limitation, not a patch defect or a claim that the original command exited zero.
- **C4-diff-cov:** the frozen log records 786/823 instrumentable changed lines covered (95.5%), with 846 changed lines unscored (`gate-logs/C4-diff-cov.log:634`). This is log-derived evidence; the instance-scoped wrapper was not rerun here.
- **C5-mutants:** the frozen log records 132 tested: 98 caught and 34 unviable (`gate-logs/C5-mutants.log:13`). The instance-scoped mutation wrapper was not rerun here. This aggregate does not identify the brief's individual hand-edited mutant legs or establish coverage of R1.
- **T4-batch-review:** its one reported defect is corroborated by the independent executable probe; it remains unresolved (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution:** N/A. The artifacts are intentionally drafted after Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- **host-tikv:** both requested feature-enabled clippy commands passed independently (`reviewer-tikv.log:200`), consistent with `gate-logs/host-tikv.log:209`.

The brief's external tools were exercised: real `typos`, docs lint and the renderer completed, including rendering 99 pages and auditing links (`reviewer-ci.log:2`, `reviewer-ci.log:9`). No external dependency remains undisclosed behind a code-read or tool shim. This does not expand the declared staging tests into a production integration claim.

**Scope and prior art are accounted for.** Read-only GitHub queries checked merged history at the target's original base SHA by every affected file path, then enumerated all 399 closed PRs and inspected the changed files of all 19 unmerged PRs. The latter found the overlapping discontinued segmented-map PR #647; the new stage module and test have no merged history (`reviewer-prior-art.log:2`). Issue #636 is closed as not planned and records the earlier split; #657 remains open. The scripted tests exercise the prior same-index and drain-fence failure schedules (`crates/core/tests/multipart_upload_part_stage.rs:1747`, `crates/core/tests/multipart_upload_part_stage.rs:1406`). Tracked DST deferrals #660/#665 and production wiring #669 remain settled scope boundaries (`crates/core/src/multipart/stage.rs:96`, `crates/custodian/src/desired_state.rs:221`). No capability probe or runtime guard masking a load-time cause was introduced. No `INTEGRATION.md` is present in the supplied target.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #926 (657.1, UploadPart staging)

Re-ran the proof on a scratch copy of the patched tree: `cargo test -p wyrd-core --test multipart_upload_part_stage` is green, 54/54. C4-verify is UNVERIFIABLE because the test calls API the patch adds; the brief declared that in advance, so it is not a refutation. I found one concrete failing case (the first bullet). It is the same root cause as the gating T4 blocker, which it confirms and makes sharper.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:1334-1343`: `renewing` awaits `renew_if_due` first, then checks the write deadline, and only after that polls `work` again. So a fragment put that the D server answers **before** its deadline, while a renewal the timer started is still committing, is thrown away, and the chunk fails `WriteDeadlineElapsed`. Concrete case (a probe test I added to a scratch copy, using the file's own `Rig`):
  - `write_window_millis = HALF + 100`, so the deadline is T0+5100, and chunk 1's fragment 0 put is held.
  - The driver advances the clock to T0+5001 and fires the timer. The renewal commit (`Act::First` hook) releases the held put at T0+5001, which is inside the deadline, then advances the clock 200 ms (a slow metadata commit).
  - Result: `Err(Stopped { cause: WriteDeadlineElapsed { chunk: 1, deadline_millis: 1005100 } })`, even though every put was answered in time.

  This also contradicts the module doc at `stage.rs:29-33` ("writes still pending once their deadline has passed are abandoned"): these writes were finished, just not polled. The existing deadline leg cannot see it, because it pins `W + 1 < HALF` so that no renewal is ever due (`crates/core/tests/multipart_upload_part_stage.rs:1364`). Adding `if let Some(out) = futures_util::FutureExt::now_or_never(work.as_mut()) { return Ok(out); }` right after `renew_if_due` makes the probe pass and keeps all 54 tests green. That one change does not close T4's broader point, though: while a renewal is committing, the deadline is not enforced at all. To close both, race the renewal against `work` and the deadline, or check the deadline before starting a renewal. Either way, add the probe schedule as a test.
- NEEDS-HUMAN [human] — `stage.rs:947-951`, `:1266`: a body read has no bound in core. Renewal keeps the slot and owned leases alive for as long as `body.next()` stays pending, so a client that stalls forever holds its slot forever. With `MAX_INFLIGHT_PARTS = 16` (`multipart.rs:5094`), 16 stalled requests leave a session refusing every new part with `SlowDown`. The rubric's *Await discipline* rule says every await on external work is bounded. Today the bound is only handed to "the gateway's idle timeout, #669" in a doc comment, not through a `deferred: #669` marker or a parameter. Decide whether core takes a body-idle bound now, or whether this becomes a tracked deferral.
- NEEDS-HUMAN [human] — `stage.rs:892` vs `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1714`. Proposal 0016 says that after an ambiguous reserve, the reserver "probes onward only once it reads that index carrying *another* attempt's id". The code refuses `SlowDown` instead, and `c_a_held_index_taken_by_another_refuses` pins that. Under the trait contract (`crates/traits/src/lib.rs:240-247`), the code's reading is the safe one: if the other attempt later releases the index, the held batch's `require_absent` can still pass. So this is a gap in the spec, not a bug in the code. It costs extra 503s under contention. This slice may not edit 0016, so someone should file the spec fix.
- Note (no action needed here): `stage.rs:348-354`, `:1272-1275`: an overrun body comes back as `StageOutcome::Staged` with `chunk_bound_exceeded() == true`, and `len()`/`digest()` describe the truncated prefix. If 657.2 forgets to check the flag, it commits a truncated part whose digest still matches. The brief asked for a flag, so this is in spec. A separate outcome variant would make the mistake impossible; worth weighing before 657.2 builds on this.

## Attempted to refute, could not

- **Drain fence (B1/B2), the two mutants that survived in #636:**
  - Replacing `topology.excluding(draining)` with `topology.clone()` (`stage.rs:684`) fails B1, B2 and all three B3 legs.
  - Removing the per-server `require_absent` loop (`stage.rs:1455`) fails B1.
  - A fence on one server only would fail B1's `fenced == named` set check.
- **Same-index pin (C), which survived in #657's first attempt:**
  - Deleting `pinned = Some(index)` (`stage.rs:823`) fails 4 (C) legs, including `c_an_ambiguous_reserve_retries_the_same_index`.
  - Making a held index taken by another attempt probe onward (`stage.rs:892`) fails `c_a_held_index_taken_by_another_refuses`.
- **Intent recovery, renewal settle and the iteration-1 lease fixes.** I traced `intent` (`stage.rs:1397-1534`) and `Footprint::renew` (`:1064-1154`) by hand:
  - Every round reads the clock afresh and runs `check_live` after the drain scan.
  - Every later batch requires the slot bytes a held batch also requires, so at most one of them lands.
  - A held intent that lands between the entry read and the slot read is adopted.
  - I found no schedule that writes a fragment before its entry is observed durable, or that revives a lapsed lease.
- **C5:** 132 mutants, 0 missed. **Docs currency:** `05-building-block-view.md` is updated. **Drain key bytes:** the custodian re-exports `desired_dserver_key` as `desired_key`, and the parser is unchanged.

Scratch copy, probe and mutants were built under `$PDCA_SCRATCH/pdca-adversary-926-probe` and removed afterwards; no file under `$PDCA_TARGET` was changed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:1334`: **A pending renewal defeats the fragment-write deadline.** `renewing` awaits `renew_if_due` before checking the deadline or polling its timer. If a fragment remains pending, renewal becomes due at time 50, and its metadata commit takes until time 70, a fragment deadline of 60 cannot end staging at 60. The backend's operation timeout does not enforce this earlier deadline. Keep the absolute fragment deadline active during renewal and its recovery reads, preserving an unknown outcome if a submitted renewal is abandoned. Add a test holding both a fragment put and a renewal commit, then advance to the fragment deadline and assert staging terminates before releasing the renewal. The existing timeout test explicitly excludes this overlap (`crates/core/tests/multipart_upload_part_stage.rs:1364`). This confirms the frozen T4 finding at its current target-source location.

No additional correctness or reuse/simplification/efficiency findings. Reviewed the target read-only and used the frozen gate evidence; gates were not re-run.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared net-new verification posture — 54 tests pass after restoration, but the pre-fix discriminator cannot execute, so a behavioral red→green is unproved (`reviewer-restored-green.log:61`, `reviewer-red.log:2`, `brief.md:213`).
- [ ] C5 Causal adequacy — Repair R1 before relying on the deadline guarantee — an in-flight metadata renewal prevents staging from responding at the fragment deadline (`crates/core/src/multipart/stage.rs:1334`, `reviewer-deadline-probe.log:3`).
- [ ] T5 Judgment — Add the missing R1 regression schedule — the current deadline test explicitly prevents renewal being due, so it cannot detect the reproduced defect (`crates/core/tests/multipart_upload_part_stage.rs:1363`, `reviewer-deadline-probe.log:16`).
- [ ] Validation — fitness-to-purpose — Decide whether this staging-only hand-off is suitable for the next commit slice after R1 is resolved — this slice exercises neither part publication nor a production UploadPart caller (`brief.md:241`, `crates/core/src/multipart/stage.rs:15`).
- [ ] `crates/core/src/multipart/stage.rs:1334-1343`: `renewing` awaits `renew_if_due` first, then checks the write deadline, and only after that polls `work` again. So a fragment put that the D server answers **before** its deadline, while a renewal the timer started is still committing, is thrown away, and the chunk fails `WriteDeadlineElapsed`. Concrete case (a probe test I added to a scratch copy, using the file's own `Rig`):
- [ ] `stage.rs:947-951`, `:1266`: a body read has no bound in core. Renewal keeps the slot and owned leases alive for as long as `body.next()` stays pending, so a client that stalls forever holds its slot forever. With `MAX_INFLIGHT_PARTS = 16` (`multipart.rs:5094`), 16 stalled requests leave a session refusing every new part with `SlowDown`. The rubric's *Await discipline* rule says every await on external work is bounded. Today the bound is only handed to "the gateway's idle timeout, #669" in a doc comment, not through a `deferred: #669` marker or a parameter. Decide whether core takes a body-idle bound now, or whether this becomes a tracked deferral.
- [ ] `stage.rs:892` vs `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1714`. Proposal 0016 says that after an ambiguous reserve, the reserver "probes onward only once it reads that index carrying *another* attempt's id". The code refuses `SlowDown` instead, and `c_a_held_index_taken_by_another_refuses` pins that. Under the trait contract (`crates/traits/src/lib.rs:240-247`), the code's reading is the safe one: if the other attempt later releases the index, the held batch's `require_absent` can still pass. So this is a gap in the spec, not a bug in the code. It costs extra 503s under contention. This slice may not edit 0016, so someone should file the spec fix.
- [ ] `crates/core/src/multipart/stage.rs:1334`: **A pending renewal defeats the fragment-write deadline.** `renewing` awaits `renew_if_due` before checking the deadline or polling its timer. If a fragment remains pending, renewal becomes due at time 50, and its metadata commit takes until time 70, a fragment deadline of 60 cannot end staging at 60. The backend's operation timeout does not enforce this earlier deadline. Keep the absolute fragment deadline active during renewal and its recovery reads, preserving an unknown outcome if a submitted renewal is abandoned. Add a test holding both a fragment put and a renewal commit, then advance to the fragment deadline and assert staging terminates before releasing the renewal. The existing timeout test explicitly excludes this overlap (`crates/core/tests/multipart_upload_part_stage.rs:1364`). This confirms the frozen T4 finding at its current target-source location.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- [x] **The stated slot invariant contradicts permitted residue.** `brief.md:140-141` says held slots “never outnumber its live attempts,” but target `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1716-1717` explicitly permits an ambiguous reserve to install a slot after the request has refused; `:1839-1842` requires crashed attempts' slots to remain and count against admission. `crates/core/src/multipart.rs:4467-4469` encodes the same rule. The brief's own bounded-refusal case (`brief.md:82-83`) can therefore leave one slot and zero live attempts. Revise the invariant to prohibit duplicate slots per attempt while counting residue toward the cap; restrict live-count equality to the no-residue schedule. Add the held-reserve landing after refusal to that criterion, without introducing cleanup into this slice.
- [x] **Between-piece renewal does not establish the promised whole-request liveness.** `brief.md:87-91` promises leases never lapse while data is arriving, but `brief.md:279-286` prescribes renewal inline between body polls without specifying a timer/wakeup. The cited peer awaits `source.next()` at `crates/core/src/write.rs:583` and awaits chunk work at `:592-604`; the existing `Clock` seam only reads time (`crates/testkit/src/lib.rs:23-26`). A body read or fragment write can remain pending across the renewal deadline, preventing another inline check. Advancing the clock between delivered pieces, as D specifies, does not test that window. Specify how renewal gets polled independently of pending I/O, using an injected timer/tick if necessary, and require a deterministic test that observes renewal while the next body piece is still withheld, plus delayed fragment I/O. Otherwise narrow the liveness promise and state the required progress bounds.
- [x] **The enumerated handoff cannot build the promised part record from itself alone.** `brief.md:297-302` lists owned keys/bytes/placements and total part length, but omits each chunk's logical length. `OwnedEntry` and `StagedPlacement` do not retain that information (`crates/core/src/multipart.rs:4136-4139`, `:4045-4047`), whereas `PartRecord` contains ordered `ChunkRef`s (`:2930-2935`) and every `ChunkRef` requires `len` (`crates/core/src/metadata.rs:351-361`). Total length alone does not supply the chunk boundaries; the handoff does not even list the configured chunk size. Require ordered complete chunk references, including the short final chunk's length, and assert those fields in A. Otherwise 657.2 must revisit staging or reconstruct information the brief explicitly promises it will receive.
- [x] size backstop — this slice is behaving oversized: patch is 237 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Fix the confirmed deadline bug; keep the slice as is (size warning waived by the human again: most of the patch is the test file). - Renewal must not block the fragment-write deadline (stage.rs:1334). Race the renewal against the pending work and the deadline instead of awaiting renew_if_due first; a write answered in time during a renewal must not be discarded as WriteDeadlineElapsed. If the deadline cuts off a submitted renewal, report its outcome as unknown (keep the may_still_commit doubt). Add the reviewers' schedule as a test: hold a fragment put and a renewal commit, advance past the fragment deadline, assert staging ends before the renewal is released; plus the adversary's case (put answered in time while a slow renewal commits must succeed). - Body-read timeout: core takes a body-idle bound now (a caller-supplied parameter, no runtime spawn), so a stalled client cannot hold its slot and leases forever. Test it with the hand-driven timer. - Proposal 0016 :1714 (after an ambiguous reserve, "probes onward only once it reads that index carrying another attempt's id"): the human authorizes editing 0016 in this slice, overriding the brief's "no edit to proposal 0016" scope line. Change the text to match the code's safe reading (refuse SlowDown rather than probe onward, since the held batch's require_absent can still pass if the other attempt releases the index).
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
