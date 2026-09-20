# Brief — issue 663 / staged-scrub-and-repair

> Child 3 of 4 of #637's split (637.3). Do reads ONLY this file. Keep the `- **Label:** value`
> lines. `path:line` citations are on `origin/main` @ `3969a3a` (re-verified 2026-09-12). This
> bundle's base is `origin/main` **plus #661 and #662** (wave 3), which add the paged ledger walk,
> the staged set in the shared reference set, the three `orphan:` value shapes and the
> reclaim-intent ordering. Locate those by symbol on the base. Background: the scrub and
> reconstruction rows of 0016's decision-2 table
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`), its failure table
> `:874-890`, and the pre-mark and write-deadline rules `:1300-1354`.

- **Slug:** staged-scrub-and-repair
- **Kind:** enhancement
- **Defect:** staged redundancy decays untended. **Scrub** walks only committed placements
  (`crates/custodian/src/scrub.rs:88`, `:131-199`), so a committed part's fragment can rot for
  the hours a session stays open and nothing notices. **Reconstruction** resolves a repair
  obligation only against committed inodes (`read_committed`, `reconstruction.rs:468`). A staged
  chunk finds no committed map, the obligation is assessed `Drain` (`reconstruction.rs:613`),
  and it is silently dropped (`:218`, committed at `:322`). So even an obligation someone
  queued for a staged chunk is discarded, and the part stays one fragment short until it is
  published — or forever, if the client never completes.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_repair.rs` passes, plus
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
- **Falsifiability:** RED is produced in-process on this bundle's base — `origin/main` + #661 +
  #662. When the waves run in one flow that fold is exported as `$PDCA_VERIFY_BASE`, which
  `engine/scripts/run-verify.sh` honours (`:247-265`); once both have merged, `main` itself is
  that base. Legs A (committed part), B, C and D(iv) fail by **assertion** there, because the
  base drops the obligation (`reconstruction.rs:613`) and never scrubs part fragments. D(i)–(iii),
  (v) and (vi) and E hold only once the re-place exists, so on the base they fail on the missing
  repair, not on the rule. That is acceptable, but each rule's own branch must be reached in the
  green leg (the diff-coverage gate reports it). The DST case is a **modified** file, so it never
  joins C4-verify's invocation. C4-ci is its gate. The new test may name only base-visible
  symbols — the store, key and record helpers of `wyrd_core::{metadata, multipart, repair}`,
  `wyrd_custodian::{reconcile_step, ReconstructionContext, ScrubContext, GcContext,
  set_lifecycle, DServerLifecycle, Custodian, FencedZone}` and `wyrd_traits` — and nothing this
  slice adds. A compile failure on the RED leg reports UNVERIFIABLE (`run-verify.sh:521-547`).
- **Invariant to restore:** a staged chunk's redundancy is maintained the way a committed
  chunk's is — verified, and repaired when degraded — and no repair outcome strands a
  fragment. Every fragment a re-place writes is, at every instant, either adopted by a
  reference or covered by an `orphan:` mark GC can act on, and a repair obligation is removed
  only once the repair is durable. Source: the scrub and reconstruction rows of 0016's table
  (`0016:824-825`) and failure rows `:887-889`; "no fragment is written after its evidence may
  have been reclaimed" (`0016:1355-1358`); the custodian repair contract (proposal 0005, `docs/design/proposals/accepted/0005-milestone-3-custodians.md:269-286`);
  ADR-0045. SELF-TEST: fixing reconstruction alone passes B and C while scrub never queues the
  obligation (leg A). Fixing scrub alone queues obligations that reconstruction then drops (leg
  B).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 803, 804
- **Conflicts with:** 777
- **Ordering note:** **Re-pointed 2026-09-15:** #662 was split at its re-plan into #803 (the
  staged reference set) and #804 (reclaim intent before deletion, and the `orphan:` value
  shapes). This slice needs both — the staged set to find its chunks, and the
  record-before-destroy order for its adoption CAS — so `Depends on` names both children in
  place of `662`; the driver does not re-point a split parent's dependents itself. Wave 4 of
  the run: #803 · #664 · #804 · this slice · #800 (since 2026-09-15 #804 conflicts with #664,
  so child-4 no longer shares this slice's wave). child-2 (now #803 + #804) supplies the staged set and the reclaim-intent ordering. The adoption CAS's
  precondition on the pre-mark's original bytes is only sound because GC records `reclaiming`
  before deleting (`0016:1312-1336`). **Outside the proposal:** this slice conflicts with #777
  (segmented repair through `repoint_chunk`), which also edits `crates/custodian/src/reconstruction.rs`
  and `crates/dst/tests/custodian.rs`. That id was added to `Conflicts with` at acceptance (2026-09-12).
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **Scope:** scrub over committed staged fragments (verify, and queue repair for corrupt or
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
- **Repro instruction:** on `origin/main`, seed an `Open` session with one committed `part:`
  record, delete one of its fragments from its D server, `enqueue_repair` its chunk, and run a
  reconstruction pass: the obligation is gone, and no fragment was rebuilt.
- **External dependencies:** none — in-process doubles, and the DST case runs under
  `cargo xtask ci`'s own `--cfg madsim` sweep.
- **Test file:** `crates/custodian/tests/staged_repair.rs` — a **NEW** file. The C4-verify gate
  earns its red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`). The
  DST case goes in the **existing** `crates/dst/tests/custodian.rs`, never a new DST file. An
  added `#![cfg(madsim)]` file would join C4-verify's single cargo invocation, which would then
  run under `--cfg madsim` (`run-verify.sh:159-200`) and put the new custodian test's red at
  risk. The existing dev-dependencies suffice (`wyrd-chunk-format` for the bit flip); make no
  `Cargo.toml` change.
- **Production reach:** the passes under test are the production `reconcile_step` (scrub and
  reconstruction loops). The session fence is applied by the test, because Abort and Complete
  (#656, #658) do not exist yet. That is the intended state: the race is the re-place against
  *any* fence, and the fence is a CAS on the `mpu:` record whoever writes it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/reconstruction.rs:600-720` (`assess`) and `:829-955` (`repair_chunk`)
    — the committed re-place to mirror for rebuild, destination choice and CAS shape. It passes
    no deadline today (`:934`), and the staged path must.
  * `crates/custodian/src/scrub.rs:131-199` — the verify-and-enqueue loop to extend to committed
    part fragments.
  * `crates/traits/src/lib.rs:837-1030` — `WriteDeadlineExpired` and `is_write_deadline_expired`.
  * `crates/custodian/tests/scrub.rs:154-160` and `crates/custodian/tests/reconstruction.rs` —
    the bit-flip and repair-harness idioms.
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/scrub.rs`,
  `reconstruction.rs`) across merged history and open PRs: neither has ever read `part:` or
  `sidx:`, and no open PR touches them. Rejected prior art: #637 v1
  (`results/issue_637/iteration-v1/`) built this re-place inside the oversized patch. Its review
  and adversary pass found the undecodable-source commit, the reused destination stamp and the
  never-exercised write deadline. Legs D(ii), D(iv) and D(vi) exist for those findings.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild required. Human confirmed the following advisory-review findings as real blockers and overrode the size-backstop's iterate-plan recommendation (173 KB vs. 100 KB threshold) in favor of a direct rebuild. Required fixes: 1. Drain filter/drain fence mismatch (`crates/custodian/src/reconstruction.rs:212` vs. `crates/custodian/src/reconstruction/staged.rs:461`): `choose_destinations` (`staged.rs:492-547`) must exclude any server whose `desired:dserver:<S>` key is present at all, not just those with `draining`/`decommissioning` values, so destination selection and the adoption CAS test the same fact. Concrete repro in the adversary review (seed `desired:dserver:3 = "maintenance"`, 4 passes never repair, all report Satisfied). Add a regression test. 2. Pass-start deadline stall (`crates/custodian/src/reconstruction/staged.rs:402-423`, `crates/server/src/custodian.rs:533`): read the clock at pre-mark commit time, not once at pass start, so a slow pass (scans + assessment over ~20s) does not expire every staged write before it is attempted. 3. D-server test double must enforce the write deadline it claims to test (`crates/dst/tests/custodian.rs:3611`, `:287`): `ReplaceDServer` delegates to `MemDServer`, which ignores `deadline_millis`. Make the double actually refuse expired writes so the DST campaign exercises production refusal semantics, not a no-op. 4. New requirement (first time this pattern is raised — human wants it fixed now, not deferred to #800): promote `W_WRITE_MILLIS` (`staged.rs:99`) from a comment-only constant to a CLI-configurable constant in `crates/server/src/cli.rs`, following the existing convention there (e.g. `LEASE_TTL_MILLIS`). Document in its doc comment the inequality it must satisfy against #800's `D = W_repoint + W_write + δ_clock` grace window, so GC's fragment-less-mark sweep (#800) cannot be sized shorter than this window without a visible contradiction. Not carried forward as blockers (resolved by discussion): - D(v) worker-side staleness gate built as D-server enforcement rather than a worker-side clock check: accepted as-is. Human's rationale: no release has shipped yet, so there is no mixed-version-fleet backward-compatibility concern that would require the stricter worker-side gate. - T4's 6 blocking rubric findings are duplicate phrasings of items 2 and 3 above; no separate fix needed. Validation/fitness-to-purpose (advisory review's overall judgment item) was conditioned on items 1-3; treated as resolved once those are fixed in the rebuild.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild required. Human confirmed the following advisory-review findings as real blockers and overrode the size-backstop's iterate-plan recommendation (173 KB vs. 100 KB threshold) in favor of a direct rebuild.

  Required fixes:
  1. Drain filter/drain fence mismatch (`crates/custodian/src/reconstruction.rs:212` vs. `crates/custodian/src/reconstruction/staged.rs:461`): `choose_destinations` (`staged.rs:492-547`) must exclude any server whose `desired:dserver:<S>` key is present at all, not just those with `draining`/`decommissioning` values, so destination selection and the adoption CAS test the same fact. Concrete repro in the adversary review (seed `desired:dserver:3 = "maintenance"`, 4 passes never repair, all report Satisfied). Add a regression test.
  2. Pass-start deadline stall (`crates/custodian/src/reconstruction/staged.rs:402-423`, `crates/server/src/custodian.rs:533`): read the clock at pre-mark commit time, not once at pass start, so a slow pass (scans + assessment over ~20s) does not expire every staged write before it is attempted.
  3. D-server test double must enforce the write deadline it claims to test (`crates/dst/tests/custodian.rs:3611`, `:287`): `ReplaceDServer` delegates to `MemDServer`, which ignores `deadline_millis`. Make the double actually refuse expired writes so the DST campaign exercises production refusal semantics, not a no-op.
  4. New requirement (first time this pattern is raised — human wants it fixed now, not deferred to #800): promote `W_WRITE_MILLIS` (`staged.rs:99`) from a comment-only constant to a CLI-configurable constant in `crates/server/src/cli.rs`, following the existing convention there (e.g. `LEASE_TTL_MILLIS`). Document in its doc comment the inequality it must satisfy against #800's `D = W_repoint + W_write + δ_clock` grace window, so GC's fragment-less-mark sweep (#800) cannot be sized shorter than this window without a visible contradiction.

  Not carried forward as blockers (resolved by discussion):
  - D(v) worker-side staleness gate built as D-server enforcement rather than a worker-side clock check: accepted as-is. Human's rationale: no release has shipped yet, so there is no mixed-version-fleet backward-compatibility concern that would require the stricter worker-side gate.
  - T4's 6 blocking rubric findings are duplicate phrasings of items 2 and 3 above; no separate fix needed.

  Validation/fitness-to-purpose (advisory review's overall judgment item) was conditioned on items 1-3; treated as resolved once those are fixed in the rebuild.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 79 mutants tested in 2m: 4 missed, 34 caught, 41 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Human confirmed the size backstop's iterate-plan recommendation (210 KB vs. 100 KB threshold, second consecutive iteration to trip it) and overrode it in favor of a direct rebuild. Required fix: 1. `choose_destinations` (`crates/custodian/src/reconstruction/staged.rs:388-394`): confirmed liveness bug. Rejecting one unusable (server, fragment-index) position excludes the whole server, so a repair with multiple missing fragments can stall forever even when a valid swap exists (adversary-confirmed with an RS(2,2) probe: two missing fragments, two free domains, one stale `reclaiming` mark — 3 passes, 0 writes, obligation never drains, every pass reports Satisfied, even though the swap was valid the whole time). Fix: exclude the position, not the server — e.g. a per-index exclusion set, or try a rejected server against the chunk's other missing indices before dropping it entirely. Add a regression test for this case. Brief waivers for this rebuild (both require lifting the "no new context-struct field / no `reconcile_step` signature change" constraint, which blocked a clean fix last iteration): 2. `crates/server/src/cli.rs:126-133` vs `crates/custodian/src/reconstruction.rs:95`: the "CLI-configurable" constant is currently only a compile-time-asserted copy (changing it breaks the build, doesn't change runtime behaviour) — ownership is reversed vs. the `LEASE_TTL_MILLIS` convention it's meant to follow (there, `cli.rs` owns the value and the server passes it down). Human: override the brief so this can be threaded down for real, matching the `LEASE_TTL_MILLIS` pattern. 3. `crates/custodian/src/reconciliation.rs:126-147` (`StepClock`): currently mixes a caller-supplied timestamp with real `Instant::elapsed`, so tests cannot fully control time (forced a 60s slack tolerance, no test can pin a write landing exactly at a deadline) and it doesn't go through the project's single testkit `Clock` seam (ADR-0024), which T1 Structure flagged as a one-time-source-lifecycle violation (AGENTS.md:132). No production failure was found under probing — this is a test-control and structure concern, not a runtime bug. Human: must follow ADR-0024 (route through the testkit Clock seam) even though it requires the same brief waiver as item 2. Not carried forward as blockers (confirmed non-bugs / out of scope, do not re-attempt): - `staged.rs:536` (late `WriteEffect::Unknown` write can land after GC reclaims the destination's pre-mark, stranding an unmarked fragment): confirmed real by the adversary, but it is a system-wide gap in the write-deadline model shared by every deadline-carrying writer (`crates/traits/src/lib.rs:886-891`), not introduced by or in scope for this slice. Filed as an Act candidate (§10) to become its own issue — do not fix inside this bundle. - `staged.rs:314` (`EcScheme::None` → `Unrepairable` without a fetch attempt): confirmed correct by design. `EcScheme::None` (`--durability none` / `replication(1)`, k=1 m=0) is zero-redundancy storage — the one fragment written *is* the data, so there is no second source to reconstruct from if it's lost. Mirrors the existing committed-path behaviour at `reconstruction.rs:641` verbatim; the brief requires the committed path's behaviour be kept unchanged. No fix needed. T5 Judgment and Validation/fitness-to-purpose: human confirmed both fine as-is (prior art checked across iterations already; no Tier-1/Tier-2 fault campaigns required for this round) — not required to be re-cleared for this iterate-do, since disposition is not accept.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human confirmed the size backstop's iterate-plan recommendation (210 KB vs. 100 KB threshold, second consecutive iteration to trip it) and overrode it in favor of a direct rebuild.

  Required fix:
  1. `choose_destinations` (`crates/custodian/src/reconstruction/staged.rs:388-394`): confirmed liveness bug. Rejecting one unusable (server, fragment-index) position excludes the whole server, so a repair with multiple missing fragments can stall forever even when a valid swap exists (adversary-confirmed with an RS(2,2) probe: two missing fragments, two free domains, one stale `reclaiming` mark — 3 passes, 0 writes, obligation never drains, every pass reports Satisfied, even though the swap was valid the whole time). Fix: exclude the position, not the server — e.g. a per-index exclusion set, or try a rejected server against the chunk's other missing indices before dropping it entirely. Add a regression test for this case.

  Brief waivers for this rebuild (both require lifting the "no new context-struct field / no `reconcile_step` signature change" constraint, which blocked a clean fix last iteration):
  2. `crates/server/src/cli.rs:126-133` vs `crates/custodian/src/reconstruction.rs:95`: the "CLI-configurable" constant is currently only a compile-time-asserted copy (changing it breaks the build, doesn't change runtime behaviour) — ownership is reversed vs. the `LEASE_TTL_MILLIS` convention it's meant to follow (there, `cli.rs` owns the value and the server passes it down). Human: override the brief so this can be threaded down for real, matching the `LEASE_TTL_MILLIS` pattern.
  3. `crates/custodian/src/reconciliation.rs:126-147` (`StepClock`): currently mixes a caller-supplied timestamp with real `Instant::elapsed`, so tests cannot fully control time (forced a 60s slack tolerance, no test can pin a write landing exactly at a deadline) and it doesn't go through the project's single testkit `Clock` seam (ADR-0024), which T1 Structure flagged as a one-time-source-lifecycle violation (AGENTS.md:132). No production failure was found under probing — this is a test-control and structure concern, not a runtime bug. Human: must follow ADR-0024 (route through the testkit Clock seam) even though it requires the same brief waiver as item 2.

  Not carried forward as blockers (confirmed non-bugs / out of scope, do not re-attempt):
  - `staged.rs:536` (late `WriteEffect::Unknown` write can land after GC reclaims the destination's pre-mark, stranding an unmarked fragment): confirmed real by the adversary, but it is a system-wide gap in the write-deadline model shared by every deadline-carrying writer (`crates/traits/src/lib.rs:886-891`), not introduced by or in scope for this slice. Filed as an Act candidate (§10) to become its own issue — do not fix inside this bundle.
  - `staged.rs:314` (`EcScheme::None` → `Unrepairable` without a fetch attempt): confirmed correct by design. `EcScheme::None` (`--durability none` / `replication(1)`, k=1 m=0) is zero-redundancy storage — the one fragment written *is* the data, so there is no second source to reconstruct from if it's lost. Mirrors the existing committed-path behaviour at `reconstruction.rs:641` verbatim; the brief requires the committed path's behaviour be kept unchanged. No fix needed.

  T5 Judgment and Validation/fitness-to-purpose: human confirmed both fine as-is (prior art checked across iterations already; no Tier-1/Tier-2 fault campaigns required for this round) — not required to be re-cleared for this iterate-do, since disposition is not accept.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized: patch is 248 KB (threshold 100 KB), touches 20 files (threshold 20), and 2 rounds are already spent (threshold 2) — the bundle's own size backstop flags this. The adversarial reviewer could not break the fix's core protocol (CAS ordering, pre-marks, deadlines, clock wiring all held), so the gating T4 failures and §6 items (docs currency, "already existed" fleet-visit behavior, one-chunk-per-part throughput limit, live-fleet mutation gap) read as slicing fallout from a too-big diff, not a broken implementation. Send back to Plan to split into smaller briefs (`pdca split`) rather than spending another iterate-do round on the same-shaped findings.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 93 mutants tested in 7m: 40 caught, 51 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
