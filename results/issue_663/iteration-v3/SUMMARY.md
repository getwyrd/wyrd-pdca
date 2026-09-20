# Result — issue 663 / staged-scrub-and-repair

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged redundancy decays untended. **Scrub** walks only committed placements
  (`crates/custodian/src/scrub.rs:88`, `:131-199`), so a committed part's fragment can rot for
  the hours a session stays open and nothing notices. **Reconstruction** resolves a repair
  obligation only against committed inodes (`read_committed`, `reconstruction.rs:468`). A staged
  chunk finds no committed map, the obligation is assessed `Drain` (`reconstruction.rs:613`),
  and it is silently dropped (`:218`, committed at `:322`). So even an obligation someone
  queued for a staged chunk is discarded, and the part stays one fragment short until it is
  published — or forever, if the client never completes.
- Success criterion: the NEW file `crates/custodian/tests/staged_repair.rs` passes, plus
  one seeded case appended to the existing `crates/dst/tests/custodian.rs`. It runs over
  in-memory doubles, with records seeded as raw JSON the base decoders accept (the fixture
  shapes in `crates/core/tests/multipart_session_records.rs:81-145`). The D-server doubles
  **enforce** the write deadline `put_fragment` carries: a write arriving at or after its
  `deadline_millis` is refused with `wyrd_traits::WriteDeadlineExpired`, as the real D server
  does since #638. v1's doubles ignored it (`_deadline_millis`,
  `crates/custodian/tests/gc.rs:127`), so the deadline was never exercised. Legs:
  **(A) Scrub checks committed staged fragments and queues repair.** A committed `part:`
  record's fragment carrying a single bit flip (the `corrupt_fragment` idiom,
  `crates/custodian/tests/scrub.rs:154-160`) results, after a scrub pass, in a queued repair for
  that chunk (`wyrd_core::repair::queued_repairs`, `crates/core/src/repair.rs:151`). A
  **missing** committed-part fragment does too. An **in-flight** (`sidx:`-only) chunk with a
  missing fragment queues **nothing**: a still-streaming chunk is expected to be incomplete, and
  verification needs the committed scheme the part record carries (`0016:824`). On the base
  scrub never sees the part's fragments, so the first two go red.
  **(B) Reconstruction repairs a staged chunk — the whole protocol, not just the metadata.**
  With a committed part's fragment lost and its repair queued, run `reconcile_step` with a
  `ReconstructionContext`. All of the following must hold:
  - the new D server holds a fragment for that chunk that is **intact and scheme-correct**
    (its header matches the chunk's identity, `wyrd_core::repair::header_matches_identity`);
  - the `part:` record's `ChunkRef.placement` names **that** server;
  - the destination's pre-mark `orphan:<P_new>` is **gone**;
  - the vacated source `P_old` carries an `orphan:` mark GC will act on after grace;
  - the obligation has **drained**.

  A changed placement alone is not enough: it proves metadata moved, not that a byte was
  rebuilt. On the base the obligation is dropped and nothing is written — the red.
  **(C) The losing branch leaves nothing stranded (X29, `0016:888`).** The D-server double's
  `put_fragment` hook fences the session **after** the destination fragment is written and
  **before** the adoption CAS, by moving the `mpu:` record from `Open@E` to `Aborting@E+1`.
  Then: the re-place makes no adoption; the `part:` record is byte-identical; the pre-mark
  `orphan:<P_new>` **stands**, so GC will reclaim the written fragment; and the obligation stays
  queued. Repeat the same assertions with the `part:` record rewritten instead of the session
  fenced (`require(part == prior)` loses). On the base the obligation is dropped, so the
  "still queued" assertion goes red.
  **(D) The pre-mark and write-deadline rules (`0016:1300-1354`), each with its own case — v1's
  review found every one of these untested:**
  (i) the pre-mark is durable **before** the destination write: the D-server double asserts
  `orphan:<P_new>` is present when `put_fragment` arrives;
  (ii) a destination position already carrying a mark from another event, or a legacy mark, is
  **re-stamped** fresh, never reused as the pre-mark with its old stamp. A reused old stamp
  lets GC's sweep of fragment-less marks take it before the write lands;
  (iii) a destination carrying a `reclaiming` mark is never used;
  (iv) the destination write carries an authorization deadline, and when the double refuses it
  as expired, the re-place aborts: no adoption, pre-mark standing, obligation queued;
  (v) the worker does **not** authorize the write if its own pre-mark is older than `W_repoint`
  (`0016:1338-1346`). A hook advances the clock past it between pre-mark and write; the worker
  then restarts from a fresh pre-mark or aborts, and no write is authorized on the stale one;
  (vi) a source `P_old` whose existing `orphan:` value decodes as none of the three shapes makes
  the move **abort before its CAS**. It never overwrites metadata it cannot parse (ADR-0045),
  the obligation stays queued, and the fault is surfaced. v1 logged it and committed anyway
  (`results/issue_637/iteration-v1/review-batch.md`).
  **(E) The destination is fenced against a drain (`0016:885`, "the same fence applies to the
  destination of a staged re-place").** A draining server (`set_lifecycle`) is never chosen as
  the destination. A drain recorded between destination selection and the adoption CAS makes
  that CAS lose: the batch carries `require_absent(desired:dserver:<S_new>)`, `S_new` is not
  adopted, and the pre-mark stands.
  **(F) Seeded DST for X29**, appended to the **existing** `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53` — keep that attribute). It sweeps the fence across every point of
  the re-place: before the pre-mark, between pre-mark and write, between write and CAS, and after
  the CAS. In every interleaving, no fragment ends unreferenced **and** unevidenced, and the
  session never ends `Aborting` with a `part:` record naming a fragment that was not written.
  It runs under `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1616`, `--cfg madsim`).
  Record the seed count in `build-notes.md`.
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: scrub over committed staged fragments (verify, and queue repair for corrupt or
  missing ones), and reconstruction's resolution and repair of an obligation for a staged chunk.
  The repair re-places the fragment under 0016's rules: pre-mark before write, write deadline
  and `W_repoint`, adoption CAS pinned to the session state and the prior part record, the
  drain fence on the destination. It leaves the obligation queued on any loss. The committed
  repair path (`repair_chunk`, `reconstruction.rs:829-955`) keeps its current behaviour.
  Must NOT change `reconcile_step`'s signature, and must NOT add a field to any context struct.
  / out of scope: `seg:`-resident repair (#777); rebalance, drain status and restore
  (child-4 — do not touch `desired_state.rs`, `rebalance.rs`, `restore.rs`,
  `crates/core/src/multipart.rs` beyond what is already on the base, `crates/server/src/cli.rs`
  or `docs/`); the ledger walk and mark codec (child-1 and child-2 — consume them, do not
  reshape them); the upload-side drain fence (#657, X59); any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.0% — 592 of 604 instrumentable changed lines executed (floor 80%); 604 of 1456 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 93 mutants tested in 7m: 40 caught, 51 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.43s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make staged multipart fragments scrubbed and safely reconstructable under session, orphan-mark, deadline, and destination-drain fences.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the staged scrub/repair invariant, loss branches, dependency assumptions, and iteration-3 waivers explicit enough to judge without inventing scope. |
| C2 Reproduction (red pre-fix) | FAIL | The pre-fix discriminator never executes: independent stashing reproduces a compile error because the new test names patch-added context fields at `crates/custodian/tests/staged_repair.rs:741-742`, contrary to the required base-visible test seam. |
| C3 Change | PASS | The core change follows the requested protocol: committed parts enter scrub at `crates/custodian/src/scrub.rs:172-180`, and staged repair durably pre-marks before the deadline-bound write and fenced adoption at `crates/custodian/src/reconstruction/staged.rs:612-710`. |
| C4 Verification (red→green) | FAIL | Green is independently confirmed at 39/39 and frozen CI/DST are green, but no behavioral red exists because the stashed build stops at `crates/custodian/tests/staged_repair.rs:741-742`; therefore the required red→green claim is unverified. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Mutation produced two hang-inducing timeouts rather than survivors, but the rebuild must cover placements whose server is absent from the live fleet — `by_dserver` includes them at `crates/custodian/src/scrub.rs:166-180`, while only `ctx.fleet` is visited at `crates/custodian/src/scrub.rs:183-250`. |
| T1 Structure | PASS | The rebuilt design uses one injected clock for mark stamps and deadlines (`crates/custodian/src/reconstruction.rs:111-127`) and wires the run-loop source through the required Clock seam (`crates/server/src/custodian.rs:503-541`). |
| T2 Shape | PASS | The large protocol is isolated behind the staged reconstruction submodule (`crates/custodian/src/reconstruction.rs:76-77`), while the carry-forward explicitly settles the otherwise out-of-scope context-field expansion. |
| T3 Runtime | FAIL | A deployed pass drops unreachable peers from the live view at `crates/server/src/custodian.rs:265-287`, then scrub silently skips their staged placements and may report `Satisfied` at `crates/custodian/src/scrub.rs:183-264`, leaving redundancy degraded indefinitely. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the brief's explicit `docs/` exclusion may override documentation currency — `docs/design/architecture/06-runtime-view.md:80` now materially contradicts staged scrub behavior; TiKV compile passed and the publish-only artifact audit is correctly N/A/deferred. |
| T5 Judgment | PASS | The affected-path prior-art review and rejected-v1 comparison were already recorded and human-settled across the carried iterations, and no rejected finding class is being re-raised. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide fitness after the live-fleet repair gap and missing executable red are resolved — accepting now would claim staged redundancy maintenance without evidence for a deployed peer-loss path. |

### Advisory — adversary

# Adversarial review — issue 663 (staged scrub and repair), iteration 3

**Verdict: I could not break the fix's core claims.** I rebuilt the red leg by hand and it is real. I attacked the re-place's compare-and-swap (CAS) protocol, the choice of destination, the clock wiring and the D-server doubles, and found no input that strands a fragment or drops an obligation. The remaining findings are one scope conflict (docs), one T4 blocker that describes existing behaviour, and one throughput limit the patch copies from the committed path.

## The evidence

- **C4-verify's UNVERIFIABLE is a false negative, not a missing red** (`crates/custodian/tests/staged_repair.rs:741-742`). The RED leg fails to compile only because the test sets the two `ReconstructionContext` fields the iteration-2 sign-off explicitly allowed (`clock`, `staged_write_window_millis`). I rebuilt that leg by hand: base production code (`git checkout` of `crates/custodian/src`, `crates/server`, the other touched tests), the new test kept, and only those two lines deleted. Result: **37 of 39 fail, all by assertion**. Leg A: `Satisfied` != `Changed` at `staged_repair.rs:870`. Leg B at `:995`, leg C at `:1135`, leg D(iv) at `:1385`. The two that pass on the base (`a_scrub_verifies_a_chunk_named_by_both_classes_once`, `a_staged_chunk_already_whole_drains_its_obligation`) guard against over-reach and are not the brief's red legs. The green leg re-ran at 39/39. The test drives the production `reconcile_step`, not a copy of it, and the D-server double enforces the deadline through the seam's own `WriteDeadlineExpired::if_elapsed` / `if_publication_unverified` (`staged_repair.rs:310-331`). Not a finding. It supports the verdict, and the human can use this reproduction to sign off C4-verify.
- **The DST case ran under the simulator** (`gate-logs/C4-ci.log:3578`, `:3586`: `staged_replace_never_strands`, `staged_replace_reaches_every_window` under `--cfg madsim`). The deterministic window sweep (`crates/dst/tests/custodian.rs:4051`) checks the no-strand invariant at every fence point.

## Findings

- NEEDS-HUMAN [human] — **Docs currency conflicts with the brief's scope, and the gating T4 row is red on it.** `docs/design/architecture/06-runtime-view.md:80` still says "Scrub and the drain-status query read committed references only". After this patch that is false: scrub now verifies committed-part fragments (`crates/custodian/src/scrub.rs:100`, `:172-181`), and reconstruction resolves and re-places staged chunks (`crates/custodian/src/reconstruction.rs:220-226`). The rubric makes docs currency a merge requirement. The brief's out-of-scope list bars `docs/`. The builder cannot resolve that alone. A human must either lift the `docs/` exclusion for this one sentence or record a waiver with a tracking issue. Two of T4's three blockers are this one finding.
- NEEDS-HUMAN [human] — **T4's `scrub.rs:180` blocker is behaviour that already existed, not something this diff adds. Record it as rejected rather than rebuild for it.** Scrub has always visited only servers in `ctx.fleet` (base `crates/custodian/src/scrub.rs:138`, now `:184-187`). The deployed loop documents that an unreachable peer is dropped and "read *around* by reconstruction, unchanged" (`crates/server/src/custodian.rs:524-527`). A committed chunk on a dropped server gets exactly the same treatment. The staged half inherits the rule; it does not introduce it. Under the rubric's out-of-scope rule this gets a decline with an issue reference. T4 is gating, so a human has to make that call.
- NEEDS-HUMAN [human] — **Only one degraded chunk per part is repaired per pass** (`crates/custodian/src/reconstruction/staged.rs:620-622`, `:677-679`). Every plan inside one part pins the same `part.prior` bytes. The first adoption rewrites the record, so each later plan in that part loses its pre-mark (`pre-mark-lost`, `staged.rs:632-635`) after it has already gathered all k+m fragments and rebuilt the chunk. Probe (scratch only): one part naming two chunks, each missing fragment 1. Pass 1 gives `Changed` with one chunk still queued and `pre-mark-lost` logged; pass 2 drains it. Losing a D server degrades every chunk of every open part on it. A part with N degraded chunks then takes N passes (30 s default interval, `crates/server/src/cli.rs:883`, so about 8 h for N=1000) and O(N²) fragment reads, and the part stays one fragment short all that time. This is the exact exposure the brief's Defect line describes. The committed path has the same shared-snapshot CAS (`reconstruction.rs:1084-1087`), so this is inherited, not a regression, and nothing is stranded (the losing plan writes nothing). Scope question: accept for this slice and file a follow-up (re-read the part after an adoption, or fold all owed chunks of one part into one re-place), or require it now.

## Refutation attempts that failed

- **CAS ordering / ABA** (`staged.rs:617-635`, `:676-706`). Both batches pin the session bytes, the part bytes and `require_absent(desired:dserver:<S_new>)`. The adoption also pins the exact pre-mark bytes, and GC swaps a mark to `reclaiming` under an exact-value precondition (`crates/custodian/src/gc.rs:440-446`), so a reclaim decided at any point makes the adoption lose. The vacated source is pinned as read. I found no ordering where a written fragment is neither named nor marked.
- **Pre-mark event length with real 128-bit chunk ids** (`staged.rs:618`). The tests use tiny ids (`0x663`). `replace:<32 hex>:<20 digits>` is at most about 61 bytes, under `MAX_ORPHAN_EVENT_LEN = 256` (`crates/core/src/metadata.rs:98`). No `?` failure there.
- **Re-writing a fragment that already exists** (retry after an `Unknown` landing, or an in-place rebuild over a corrupt fragment). `FsChunkStore` publishes by rename, last writer wins (`crates/chunkstore-fs/src/lib.rs:303-307`), so no error aborts the pass.
- **A stale pre-mark left on a still-referenced position after a lost in-place adoption.** Harmless: reference protection outranks every mark (`gc.rs:476-483`), and every writer that dereferences a fragment blind-puts a fresh stamp (`crates/core/src/metadata.rs:2116-2119`, `:2224-2227`).
- **Destination matching** (`staged.rs:401-522`). Every round reads a new position or considers a new server, so the loop ends. A failed augment leaves `held` untouched, and a position ruled out for one fragment index leaves its server free for another. The two C5 timeouts (`staged.rs:455`, `:510`) are mutants that make the loop spin. The tests catch them by hanging; they are not survivors.
- **One clock** (`crates/server/src/custodian.rs:511`, `:540`). One `LoopClock` supplies every pass's `now_millis` and the re-place's stamps. The `Mutex` is held only for the duration of the call, never across an await.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Mutation produced two hang-inducing timeouts rather than survivors, but the rebuild must cover placements whose server is absent from the live fleet — `by_dserver` includes them at `crates/custodian/src/scrub.rs:166-180`, while only `ctx.fleet` is visited at `crates/custodian/src/scrub.rs:183-250`.
- [ ] T4 Contribution — Decide whether the brief's explicit `docs/` exclusion may override documentation currency — `docs/design/architecture/06-runtime-view.md:80` now materially contradicts staged scrub behavior; TiKV compile passed and the publish-only artifact audit is correctly N/A/deferred.
- [ ] Validation — fitness-to-purpose — Human must decide fitness after the live-fleet repair gap and missing executable red are resolved — accepting now would claim staged redundancy maintenance without evidence for a deployed peer-loss path.
- [ ] **Docs currency conflicts with the brief's scope, and the gating T4 row is red on it.** `docs/design/architecture/06-runtime-view.md:80` still says "Scrub and the drain-status query read committed references only". After this patch that is false: scrub now verifies committed-part fragments (`crates/custodian/src/scrub.rs:100`, `:172-181`), and reconstruction resolves and re-places staged chunks (`crates/custodian/src/reconstruction.rs:220-226`). The rubric makes docs currency a merge requirement. The brief's out-of-scope list bars `docs/`. The builder cannot resolve that alone. A human must either lift the `docs/` exclusion for this one sentence or record a waiver with a tracking issue. Two of T4's three blockers are this one finding.
- [ ] **T4's `scrub.rs:180` blocker is behaviour that already existed, not something this diff adds. Record it as rejected rather than rebuild for it.** Scrub has always visited only servers in `ctx.fleet` (base `crates/custodian/src/scrub.rs:138`, now `:184-187`). The deployed loop documents that an unreachable peer is dropped and "read *around* by reconstruction, unchanged" (`crates/server/src/custodian.rs:524-527`). A committed chunk on a dropped server gets exactly the same treatment. The staged half inherits the rule; it does not introduce it. Under the rubric's out-of-scope rule this gets a decline with an issue reference. T4 is gating, so a human has to make that call.
- [ ] **Only one degraded chunk per part is repaired per pass** (`crates/custodian/src/reconstruction/staged.rs:620-622`, `:677-679`). Every plan inside one part pins the same `part.prior` bytes. The first adoption rewrites the record, so each later plan in that part loses its pre-mark (`pre-mark-lost`, `staged.rs:632-635`) after it has already gathered all k+m fragments and rebuilt the chunk. Probe (scratch only): one part naming two chunks, each missing fragment 1. Pass 1 gives `Changed` with one chunk still queued and `pre-mark-lost` logged; pass 2 drains it. Losing a D server degrades every chunk of every open part on it. A part with N degraded chunks then takes N passes (30 s default interval, `crates/server/src/cli.rs:883`, so about 8 h for N=1000) and O(N²) fragment reads, and the part stays one fragment short all that time. This is the exact exposure the brief's Defect line describes. The committed path has the same shared-snapshot CAS (`reconstruction.rs:1084-1087`), so this is inherited, not a regression, and nothing is stranded (the losing plan writes nothing). Scope question: accept for this slice and file a follow-up (re-read the part after an adoption, or fold all owed chunks of one part into one re-place), or require it now.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 248 KB (threshold 100 KB); patch touches 20 files (threshold 20); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Slice is oversized: patch is 248 KB (threshold 100 KB), touches 20 files (threshold 20), and 2 rounds are already spent (threshold 2) — the bundle's own size backstop flags this. The adversarial reviewer could not break the fix's core protocol (CAS ordering, pre-marks, deadlines, clock wiring all held), so the gating T4 failures and §6 items (docs currency, "already existed" fleet-visit behavior, one-chunk-per-part throughput limit, live-fleet mutation gap) read as slicing fallout from a too-big diff, not a broken implementation. Send back to Plan to split into smaller briefs (`pdca split`) rather than spending another iterate-do round on the same-shaped findings.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
