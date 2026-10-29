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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.1% — 712 of 749 instrumentable changed lines executed (floor 80%); 749 of 1505 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 121 mutants tested in 19m: 82 caught, 32 unviable, 7 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.30s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Revision recommended for #926’s UploadPart staging—slot reservation, owned intents, drain fencing and renewal—because a fresh intent can revive expired slot authority.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The staging-only boundary and observable safety obligations are explicit; publication and production wiring remain separate slices (`target/crates/core/src/multipart/stage.rs:15`; `target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:1843`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality; reverting the implementation removes the API required by the discriminator, so no pre-existing behavioral red executes (`target/crates/core/tests/multipart_upload_part_stage.rs:40`; `reviewer-red.log:2`). |
| C3 Change | PASS | The seven affected files stay within the authorized staging, shared-key, constructor, test and living-document surfaces; reverse-apply validation confirms the supplied patch matches the target (`target/crates/core/src/multipart.rs:155`; `target/crates/custodian/src/desired_state.rs:39`; `target/docs/design/architecture/05-building-block-view.md:244`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared net-new exception to an executable pre-fix red—stash/revert produces 11 compilation errors, while restoring the patch passes all 43 supplied tests; this is not a demonstrated behavioral red→green (`reviewer-red.log:70`; `reviewer-restored-green.log:50`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Refuse expired authority at each fresh intent authorization—both a delayed drain scan and an ambiguous retry currently revive the slot and successfully write six fragments; add regressions preserving any outstanding commit doubt (`target/crates/core/src/multipart/stage.rs:1310`; `reviewer-probes.log:27`). |
| T1 Structure | PASS | The trait-based, caller-clock design preserves dependency direction and one clock for lease correctness; sharing the drain key leaves its persisted spelling intact (`target/crates/core/src/multipart/stage.rs:76`; `target/crates/core/src/metadata.rs:102`; `target/crates/custodian/src/desired_state.rs:39`). |
| T2 Shape | PASS | The public hand-off supplies the complete references and exact record bytes needed by the next slice, with decoder-backed slot construction and explicit refusal/error types (`target/crates/core/src/multipart/stage.rs:303`; `target/crates/core/src/multipart.rs:2762`; `target/crates/core/tests/multipart_upload_part_stage.rs:1138`). |
| T3 Runtime | FAIL | The C5 defect violates the required lapse refusal in executed schedules: staging succeeds at 1,010,001 ms after a slot expired at 1,010,000 ms (`target/crates/core/src/multipart/stage.rs:1303`; `reviewer-probes.log:27`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit must rerun at publish, as the frozen deferred row requires; path-based prior art was independently checked below (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No capability-probe workaround or scope expansion was found; explicit DST deferrals remain settled, and the separate late-renewal allegation is not established by the normative sampled-time contract (`target/crates/core/src/multipart/stage.rs:83`; `target/AGENTS.md:200`; `target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:1848`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the isolated staging/commit hand-off fulfills slice 657.1 after C5 is corrected—these tests exercise real metadata with simulated fragment storage, while composed UploadPart and production wiring remain future work (`target/crates/core/tests/multipart_upload_part_stage.rs:1`; `target/crates/core/src/multipart/stage.rs:15`). |

**One confirmed implementation defect: intent authorization can revive an expired slot.** `stage_chunk` checks whether renewal is due before calling `intent` (`target/crates/core/src/multipart/stage.rs:1251`). The intent then awaits a drain scan, takes a fresh `t_auth`, and extends the slot without checking its existing lease at that reading (`target/crates/core/src/multipart/stage.rs:1303`, `:1310`, `:1317`). Its retries take the same unchecked route. The subsequent renewal sees the newly extended slot, so a first-chunk attempt can complete successfully instead of refusing the lapse.

Two independent probes against the unchanged patched library reproduced this:

- Reserve at 1,000,000 ms with a 10,000 ms TTL; advance the clock by 10,001 ms during the first intent’s drain scan. Staging commits an intent, writes six fragments, and returns `Staged`.
- Hold the first intent unapplied with `CommitUnknownResult { may_still_commit: true }`, advancing by 10,001 ms before its retry. The retry also commits, writes six fragments, and returns `Staged`.

Both refusal assertions fail (`reviewer-probes.log:36`). Check every existing slot/owned lease at the fresh authorization reading before sending another intent; if an earlier batch remains uncertain, preserve that uncertainty when stopping. The scratch probes are in `pdca-reviewer-926-probes/reviewer_probes.rs:2912` and `:2958`. From this review directory, reproduce with:

```sh
CARGO_TARGET_DIR="$PWD/reviewer-build" TMPDIR="$PWD/reviewer-scratch"   cargo test --offline --manifest-path pdca-reviewer-926-probes/Cargo.toml   --test reviewer_probes reviewer_ -- --nocapture --test-threads=1
```

**The frozen batch-review log represents one confirmed defect, not four independent defects.** Its three intent findings collapse into the case above (`gate-logs/T4-batch-review.log:10`). The renewal-only allegation is not confirmed: `Footprint::renew` checks liveness at its supplied timestamp and retries the same renewal values (`target/crates/core/src/multipart/stage.rs:986`, `:997`). Proposal 0016 explicitly allows a renewal sampled while live to commit after expiry (`target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:1848`). The observation probe returns `Renewed` after the old expiry (`reviewer-probes.log:26`), but that alone does not distinguish a defect from this permitted delayed completion. It is therefore not raised as a second implementation finding.

**Independent verification supports the existing tests, with a documented baseline limitation.** I stashed the production changes while retaining the discriminator, observed compilation failure before any test ran, popped the stash, and observed 43/43 passing tests. `git diff --check` and `git apply --reverse --check patch.diff` pass after restoration. Source citations ground on the supplied disposable target, whose snapshot records base `61e53418bb3e73674eef9352733f0e985796d582`; no stale-target failure was observed.

- The full `cargo xtask ci` rerun completed. Formatting, clippy, build, tests, conformance, static-state/dependency guards, typos and the 99-page docs render passed. Its only two failures were cargo-deny attempts to lock the read-only host advisory cache (`reviewer-ci.log:3736`, `:4373`). Both checks subsequently passed with a writable copy inside this review directory and configurations verified identical except for `advisories.db-path` (`reviewer-deny.log:1`; `reviewer-deny-all-features.log:1`). The original aggregate invocation remains exit 1; the host fault is resolved by these component reruns, not attributed to the patch. The frozen aggregate independently reports success (`gate-logs/C4-ci.log:4378`). Both declared external dependencies, typos and docs-renderer, actually ran.
- Both TiKV feature clippy commands passed independently (`reviewer-host-tikv.log:200`), matching `gate-logs/host-tikv.log:209`.
- Instance-root coverage/mutation wrappers are not part of this artifact-only target; these results are frozen evidence, not independent reruns. Coverage reports 712/749 instrumentable changed lines, 95.1%, with its stated uninstrumented-line boundary (`gate-logs/C4-diff-cov.log:639`). Mutation testing reports 82 caught, 32 unviable and seven timeouts, exit 3; it reports no surviving mutant (`gate-logs/C5-mutants.log:13`, `:20`). Those timeouts do not demonstrate a baseline patch failure, and this generic run does not independently establish every bespoke mutation listed in the brief.

**Prior art was checked by all seven affected paths, including closed work.** GitHub history queries anchored to the target base returned 25 metadata, 15 multipart, 15 write, six desired-state and 40 architecture commits; the new staging module and test have no prior commits (`reviewer-merged-path-check.json:1`). I enumerated 399 closed PRs and checked the file lists of all 19 unmerged ones; #647 overlaps metadata, write and desired-state (`reviewer-closed-path-check.json:124`). The issue lookup also confirms #636 is closed as `not_planned`, while #657 remains the open parent (`reviewer-issue-636.json:1`; `reviewer-issue-657.json:1`). No additional prior-art decision is owed. Tracked deferrals #660/#665 were not reopened, and no commit-signoff finding was inferred.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #926 (657.1, UploadPart staging)

Every claim below was run against a scratch copy of `$PDCA_TARGET`. I added probe tests to the scratch copy of `crates/core/tests/multipart_upload_part_stage.rs` and reused its own `Scripted` / `Fleet` / `Runtime` rig. The scratch copy is deleted. Line numbers point at the target tree.

## Fix refuted: four failing cases

- NEEDS-HUMAN [impl] — **An intent retry revives a lapsed slot lease, and staging returns `Staged`.** `crates/core/src/multipart/stage.rs:1310-1320` reads `t_auth` fresh on each intent round and puts the slot at `next_expiry(t_auth, ttl)`. It never runs `check_live(t_auth)`. The only liveness check is in `renew_if_due` before the loop (`:1251`), and that check only fires once renewal is due. **Failing case:** script chunk 1's first intent as `Act::Hold { advance: TTL + 1 }` and pass the retry. The log shows round 2 at clock `1010001`, requiring slot lease `1010000` (already lapsed), committing, and putting lease `1020001`. Six fragments are then written and the result is `Ok(Staged)`. The rig's own `assert_no_lapse_observed` (test `:980`) panics: "a lease required at 1010001 had lapsed at 1010000". This breaks brief (D) ("A lapsed lease is refused, never resurrected") and the Invariant line ("a lapsed one is never revived"). It is the same issue as T4-batch-review's blocking items at `:1296`, `:1310` and `:1317`, now shown with a concrete schedule. The intent also never checks the leases of earlier chunks' owned entries, so chunk N's intent can land after chunk 1's entry has lapsed.
- NEEDS-HUMAN [impl] — **A renewal retry reuses its stale clock reading and rewrites a lapsed lease.** `stage.rs:986` checks liveness once, at the first `now_millis`. `:997` stamps every later round from that same reading, with no new clock read and no new check. **Failing case:** use `renewal_body` and script the first renewal as `Act::Hold { advance: HALF }`. Round 2 commits at clock `1010001` over slot lease `1010000` (lapsed), and staging returns `Staged`. `assert_no_lapse_observed` panics. This is T4's `:997` item, confirmed.
- NEEDS-HUMAN [impl] — **The commit's last renewal (`StagedAttempt::renew`) can report `Renewed` with a lease already in the past. The fix above cannot reach it as the API stands.** `stage.rs:344-349` takes a fixed `now_millis: u64` instead of a clock, so a retried round cannot re-stamp. **Failing case:** stage one chunk, then call `attempt.renew(&store, now = T0+100, TTL)` with the first renewal scripted `Hold { advance: 2*TTL }`. It returns `Ok(Renewed)`, and the slot lease is `1010100` while the clock reads `1020000`. The brief says this step "stamps a lease covering the commit's own deadline" (Design § hand-off, `0016:1720-1733`). 657.2 will build on this signature, so fix it here: take `&impl StageRuntime` (or a clock), and re-read plus `check_live` on every round, in the same way as the two items above.
- NEEDS-HUMAN [impl] — **A held intent that lands between its two re-reads is reported as `SlotLost` with "no batch in doubt", even though the slot and the entry are this attempt's.** After an unknown outcome the intent reads the `sidx:` key (`stage.rs:1352`), then makes a *separate*, fresh read of the slot (`:1364`). `reconcile` treats any slot bytes other than `self.slot` as a lost slot and calls `doubt.release()` (`:1084-1091`). The comment there says the doubtful batches "required the slot bytes that are gone". But here the doubtful intent itself is what replaced those bytes. **Failing case:** script the intent as `Hold { advance: 0 }` and release the held batch on the first `get(slot:…:000000)`. Result: `Err(Stopped { cause: SlotLost })`. The store holds 1 owned entry, and the slot's `attempt_id` is ours. Brief (E) says a landed unknown intent "is recognised … Staging goes on". A Percolator (TiKV's commit protocol) primary commit landing between two client reads is a realistic way to hit this. The renewal and reserve loops don't have this race, because they judge the same slot read they matched against `sent`. Fix: in the intent, check the fresh slot read against each `sent` intent's slot bytes (and re-read `sidx:`) before treating it as lost.

## Evidence re-run (could not refute)

- I re-made the brief's labelled red edits myself and ran the full test file each time. None survived:
  - Fence loop removed (`stage.rs:1332-1334`; the `unused_mut` lint has to be allowed or it won't compile): only **B1** fails.
  - `excluding(draining)` replaced by the full topology (`:657`): **B1, B2** and all three B3 legs fail.
  - Same-index pin removed (`:792-794`): all three (C) pin legs fail.
  - Fragment puts not raced against the timer: **D-ii** fails.
  - Body read not raced against the timer: **D-i** and the half-TTL leg fail.
  - Slot put dropped from the renewal batch: six (D)/(E) legs fail.

  C5's "fail" is 7 TIMEOUTs and **0 missed** mutants.
- Minor test nit (not gating): some reds show up only as a hang until the 120 s `LEG_TIMEOUT` (test `:68`), not as an assertion. This is why cargo-mutants (88 s limit) scored them TIMEOUT. Example: `d_a_lapsed_slot_lease_is_refused_never_revived` (test `:2108-2114`) drops `tx` only after the join. With `check_live` mutated away, staging waits on the body forever. Dropping `tx` inside the driver after firing the timer would turn that into a fast `expect_err` failure.
- What the reviewer may have over-trusted: the (D) lapse legs (test `:2096`, `:2150`) only advance the clock *before a renewal starts*. No leg lapses a lease during an intent's retries or between renewal rounds. So the suite's coverage is narrower than the brief's "never revived" claim, and the first two findings above slip through that gap.
- Practical impact of the revival findings is limited today: GC protects every fragment an owned entry names whatever its lease (`crates/custodian/src/gc.rs:1376-1389`), and the reaper's idle fence pins slot bytes (`0016:1859-1863`). So I found no data loss. These are breaks of a stated acceptance rule, not lost bytes.
- Low-severity, fail-closed: `renewing` checks the write deadline before polling `work` (`stage.rs:1222-1228`). Puts that finished before the deadline, while a slow renewal commit was in flight, are still reported `WriteDeadlineElapsed`. That costs a retry, not safety. Separately, `write_window_millis` is not checked (`:149-153`), so `u64::MAX` yields `Some(u64::MAX)` deadlines, the value (A′) forbids. Only a caller can pass it, and none exists before #669.
- Attempted and could not refute: the drain-key bytes and custodian re-export (`crates/core/src/metadata.rs:98-114`, unchanged parse); the reserve's adopt / same-index / landed-after-refusal logic (`stage.rs:779-852`); the chunk bound (`:1157`); the `ChunkRef` hand-off and its short last `len` (`:1255-1260`); the deadline stamped from the adopted batch's own `t_auth` (`:1263`); the docs-currency edit (`05-building-block-view.md:244`). Missing DST coverage is `deferred: #660, #665` in the module doc and the brief's scope, so I treat it as settled.
<!-- pdca:leaf-complete -->

### Advisory — code-review

Reviewed the diff against the read-only target and frozen gate evidence. These are advisory implementation findings; no source was changed or gates rerun.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:832`: **Clearing the pinned index can give one attempt two slots.** After an unknown reserve, the slot read at line 805 can return absent; the held batch can then land under `Open@E`, followed by a Complete/rollback to `Open@E+2` before the session read returns. This branch clears `pinned`, and the next scan skips the attempt's now-occupied index and reserves another free index. The original slot leaks until session teardown and consumes admission capacity. Re-read and settle the original index after observing the newer epoch before permitting another index. Add this interleaving to the reservation tests.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:1310`: **An intent can revive an expired slot.** The pre-intent renewal check precedes the awaited drain scan. If that scan, or a conflict/recovery round, crosses the slot's expiry, this fresh `t_auth` is used to extend the slot without checking the existing leases. With unchanged session/slot bytes, the commit succeeds and subsequent renewal sees the extended slot, allowing staging to continue. Check the slot and all existing owned leases at `t_auth` before each intent submission, preserving any outstanding unknown-outcome uncertainty. Test expiry during the drain scan and between intent rounds.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:997`: **Renewal retries reuse a stale liveness decision.** `check_live(now_millis)` runs only once, and every retry uses that same timestamp despite intervening commits and reads. For example, a lease expiring at 1000 can pass the check at 500 with TTL 1000, encounter a conflict whose recovery finishes at 1100, then be renewed to 1500 after it has lapsed. Supply the owning clock to renewal and recheck liveness before each new submission, while still settling previously submitted batches. Add a conflict/recovery schedule that advances the clock across expiry; the existing lapse tests expire leases before entering renewal.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/stage.rs:1364`: **A late successful intent is misclassified as a lost slot.** With `may_still_commit: true`, the owned-entry read can return absent, then the held intent can commit before this slot read. The slot now contains that intent's own replacement bytes, but `Footprint::reconcile` (`crates/core/src/multipart/stage.rs:1084`) compares only against the old slot and returns terminal `SlotLost`. A valid upload stops and leaves its reservation and owned entry behind. Recognize a submitted intent's slot bytes and settle its owned entry before declaring slot loss. Exercise a held commit landing between these two reads; the existing test releases it before the next commit instead.

No additional actionable reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared net-new exception to an executable pre-fix red—stash/revert produces 11 compilation errors, while restoring the patch passes all 43 supplied tests; this is not a demonstrated behavioral red→green (`reviewer-red.log:70`; `reviewer-restored-green.log:50`).
- [ ] C5 Causal adequacy — Refuse expired authority at each fresh intent authorization—both a delayed drain scan and an ambiguous retry currently revive the slot and successfully write six fragments; add regressions preserving any outstanding commit doubt (`target/crates/core/src/multipart/stage.rs:1310`; `reviewer-probes.log:27`).
- [ ] Validation — fitness-to-purpose — Decide whether the isolated staging/commit hand-off fulfills slice 657.1 after C5 is corrected—these tests exercise real metadata with simulated fragment storage, while composed UploadPart and production wiring remain future work (`target/crates/core/tests/multipart_upload_part_stage.rs:1`; `target/crates/core/src/multipart/stage.rs:15`).
- [ ] **An intent retry revives a lapsed slot lease, and staging returns `Staged`.** `crates/core/src/multipart/stage.rs:1310-1320` reads `t_auth` fresh on each intent round and puts the slot at `next_expiry(t_auth, ttl)`. It never runs `check_live(t_auth)`. The only liveness check is in `renew_if_due` before the loop (`:1251`), and that check only fires once renewal is due. **Failing case:** script chunk 1's first intent as `Act::Hold { advance: TTL + 1 }` and pass the retry. The log shows round 2 at clock `1010001`, requiring slot lease `1010000` (already lapsed), committing, and putting lease `1020001`. Six fragments are then written and the result is `Ok(Staged)`. The rig's own `assert_no_lapse_observed` (test `:980`) panics: "a lease required at 1010001 had lapsed at 1010000". This breaks brief (D) ("A lapsed lease is refused, never resurrected") and the Invariant line ("a lapsed one is never revived"). It is the same issue as T4-batch-review's blocking items at `:1296`, `:1310` and `:1317`, now shown with a concrete schedule. The intent also never checks the leases of earlier chunks' owned entries, so chunk N's intent can land after chunk 1's entry has lapsed.
- [ ] **A renewal retry reuses its stale clock reading and rewrites a lapsed lease.** `stage.rs:986` checks liveness once, at the first `now_millis`. `:997` stamps every later round from that same reading, with no new clock read and no new check. **Failing case:** use `renewal_body` and script the first renewal as `Act::Hold { advance: HALF }`. Round 2 commits at clock `1010001` over slot lease `1010000` (lapsed), and staging returns `Staged`. `assert_no_lapse_observed` panics. This is T4's `:997` item, confirmed.
- [ ] **The commit's last renewal (`StagedAttempt::renew`) can report `Renewed` with a lease already in the past. The fix above cannot reach it as the API stands.** `stage.rs:344-349` takes a fixed `now_millis: u64` instead of a clock, so a retried round cannot re-stamp. **Failing case:** stage one chunk, then call `attempt.renew(&store, now = T0+100, TTL)` with the first renewal scripted `Hold { advance: 2*TTL }`. It returns `Ok(Renewed)`, and the slot lease is `1010100` while the clock reads `1020000`. The brief says this step "stamps a lease covering the commit's own deadline" (Design § hand-off, `0016:1720-1733`). 657.2 will build on this signature, so fix it here: take `&impl StageRuntime` (or a clock), and re-read plus `check_live` on every round, in the same way as the two items above.
- [ ] **A held intent that lands between its two re-reads is reported as `SlotLost` with "no batch in doubt", even though the slot and the entry are this attempt's.** After an unknown outcome the intent reads the `sidx:` key (`stage.rs:1352`), then makes a *separate*, fresh read of the slot (`:1364`). `reconcile` treats any slot bytes other than `self.slot` as a lost slot and calls `doubt.release()` (`:1084-1091`). The comment there says the doubtful batches "required the slot bytes that are gone". But here the doubtful intent itself is what replaced those bytes. **Failing case:** script the intent as `Hold { advance: 0 }` and release the held batch on the first `get(slot:…:000000)`. Result: `Err(Stopped { cause: SlotLost })`. The store holds 1 owned entry, and the slot's `attempt_id` is ours. Brief (E) says a landed unknown intent "is recognised … Staging goes on". A Percolator (TiKV's commit protocol) primary commit landing between two client reads is a realistic way to hit this. The renewal and reserve loops don't have this race, because they judge the same slot read they matched against `sent`. Fix: in the intent, check the fresh slot read against each `sent` intent's slot bytes (and re-read `sidx:`) before treating it as lost.
- [ ] `crates/core/src/multipart/stage.rs:832`: **Clearing the pinned index can give one attempt two slots.** After an unknown reserve, the slot read at line 805 can return absent; the held batch can then land under `Open@E`, followed by a Complete/rollback to `Open@E+2` before the session read returns. This branch clears `pinned`, and the next scan skips the attempt's now-occupied index and reserves another free index. The original slot leaks until session teardown and consumes admission capacity. Re-read and settle the original index after observing the newer epoch before permitting another index. Add this interleaving to the reservation tests.
- [ ] `crates/core/src/multipart/stage.rs:1310`: **An intent can revive an expired slot.** The pre-intent renewal check precedes the awaited drain scan. If that scan, or a conflict/recovery round, crosses the slot's expiry, this fresh `t_auth` is used to extend the slot without checking the existing leases. With unchanged session/slot bytes, the commit succeeds and subsequent renewal sees the extended slot, allowing staging to continue. Check the slot and all existing owned leases at `t_auth` before each intent submission, preserving any outstanding unknown-outcome uncertainty. Test expiry during the drain scan and between intent rounds.
- [ ] `crates/core/src/multipart/stage.rs:997`: **Renewal retries reuse a stale liveness decision.** `check_live(now_millis)` runs only once, and every retry uses that same timestamp despite intervening commits and reads. For example, a lease expiring at 1000 can pass the check at 500 with TTL 1000, encounter a conflict whose recovery finishes at 1100, then be renewed to 1500 after it has lapsed. Supply the owning clock to renewal and recheck liveness before each new submission, while still settling previously submitted batches. Add a conflict/recovery schedule that advances the clock across expiry; the existing lapse tests expire leases before entering renewal.
- [ ] `crates/core/src/multipart/stage.rs:1364`: **A late successful intent is misclassified as a lost slot.** With `may_still_commit: true`, the owned-entry read can return absent, then the held intent can commit before this slot read. The slot now contains that intent's own replacement bytes, but `Footprint::reconcile` (`crates/core/src/multipart/stage.rs:1084`) compares only against the old slot and returns terminal `SlotLost`. A valid upload stops and leaves its reservation and owned entry behind. Recognize a submitted intent's slot bytes and settle its owned entry before declaring slot loss. Exercise a held commit landing between these two reads; the existing test releases it before the next commit instead.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_926/review-b
- [ ] **The stated slot invariant contradicts permitted residue.** `brief.md:140-141` says held slots “never outnumber its live attempts,” but target `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1716-1717` explicitly permits an ambiguous reserve to install a slot after the request has refused; `:1839-1842` requires crashed attempts' slots to remain and count against admission. `crates/core/src/multipart.rs:4467-4469` encodes the same rule. The brief's own bounded-refusal case (`brief.md:82-83`) can therefore leave one slot and zero live attempts. Revise the invariant to prohibit duplicate slots per attempt while counting residue toward the cap; restrict live-count equality to the no-residue schedule. Add the held-reserve landing after refusal to that criterion, without introducing cleanup into this slice.
- [ ] **Between-piece renewal does not establish the promised whole-request liveness.** `brief.md:87-91` promises leases never lapse while data is arriving, but `brief.md:279-286` prescribes renewal inline between body polls without specifying a timer/wakeup. The cited peer awaits `source.next()` at `crates/core/src/write.rs:583` and awaits chunk work at `:592-604`; the existing `Clock` seam only reads time (`crates/testkit/src/lib.rs:23-26`). A body read or fragment write can remain pending across the renewal deadline, preventing another inline check. Advancing the clock between delivered pieces, as D specifies, does not test that window. Specify how renewal gets polled independently of pending I/O, using an injected timer/tick if necessary, and require a deterministic test that observes renewal while the next body piece is still withheld, plus delayed fragment I/O. Otherwise narrow the liveness promise and state the required progress bounds.
- [ ] **The enumerated handoff cannot build the promised part record from itself alone.** `brief.md:297-302` lists owned keys/bytes/placements and total part length, but omits each chunk's logical length. `OwnedEntry` and `StagedPlacement` do not retain that information (`crates/core/src/multipart.rs:4136-4139`, `:4045-4047`), whereas `PartRecord` contains ordered `ChunkRef`s (`:2930-2935`) and every `ChunkRef` requires `len` (`crates/core/src/metadata.rs:351-361`). Total length alone does not supply the chunk boundaries; the handoff does not even list the configured chunk size. Require ordered complete chunk references, including the short final chunk's length, and assert those fields in A. Otherwise 657.2 must revisit staging or reconstruct information the brief explicitly promises it will receive.
- [ ] size backstop — this slice is behaving oversized: patch is 203 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Fix the confirmed lease/authority bugs; keep the slice as is (no split). - Intent must check the slot lease and every earlier owned lease at its fresh t_auth before EACH submission (first round and every retry, including after the drain scan); refuse a lapsed lease, keeping any outstanding may_still_commit doubt. Add tests that lapse the lease during the drain scan and between intent rounds. - Renewal retries must re-read the clock and re-run check_live before each new submission, while still settling batches already sent. Add a conflict/hold schedule that crosses expiry between renewal rounds. - StagedAttempt::renew must take the clock/runtime, not a fixed now_millis, and re-check liveness per round (657.2 builds on this signature). - Intent recovery: if the fresh slot read matches the bytes of an intent this attempt already sent, settle that intent (re-read sidx:) instead of returning SlotLost. Test a held intent landing between the sidx: read and the slot read. - Reserve: don't clear the pinned index after an absent read followed by a newer session epoch. Re-read and settle the original index first (stage.rs:832), and add that interleaving to the tests. - Minor: d_a_lapsed_slot_lease test should drop tx inside the driver so a regression fails fast instead of hanging until LEG_TIMEOUT.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
