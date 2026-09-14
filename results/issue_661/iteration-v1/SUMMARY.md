# Result — issue 661 / gc-orphan-ledger-paged-walk

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the `orphan:` ledger is read with **one `scan`**, in two places: GC
  (`orphan_leases`, `crates/custodian/src/gc.rs:522-538`, called at `:177`) and the post-restore
  pass (`crates/custodian/src/restore.rs:308`). `scan` fails whole past
  `SCAN_CAP = 1 << 20` (`crates/traits/src/lib.rs:286`, the "no partial `Vec`" contract at
  `:275`). One maximum segmented-object retirement installs ~1.78 M marks (`0016:1392-1398`), so
  a single large delete takes GC down on every pass from then on. The deployed loop runs GC in
  its own `reconcile_step` call (`crates/server/src/custodian.rs:590-611`), so GC stops and the
  post-restore command can never finish. The failure seals itself: the pass that should shrink
  the ledger is the pass that cannot start. Separately, **a mark whose position holds no fragment
  is never visited**: GC consumes a mark only while iterating a `list_fragments()` result
  (`gc.rs:183-219`), so fragment-less marks accumulate toward that cap (`0016:1359-1368`).
- Success criterion: the NEW file `crates/custodian/tests/gc_ledger_walk.rs` passes. It runs
  over in-memory doubles, and the metadata double's `scan` enforces a **lowered** cap: it returns
  `wyrd_traits::ScanCapExceeded` when the result would exceed the cap, shaped after
  `crates/metadata-redb/tests/scan.rs:9-11`. Its `scan_page` is implemented directly over its
  map, **not** through `wyrd_testkit::test_double_scan_page`, which pages over `scan` and
  inherits the cap (`crates/traits/src/lib.rs:1430-1437`). The double counts the `orphan:`
  entries each pass receives and records any `scan` of the `orphan:` prefix. Every pass builds a
  fresh `GcContext`, exactly as the deployed loop does (`crates/server/src/custodian.rs:601-607`).
  Legs:
  **(A) GC survives a ledger past the cap.** Seed an `orphan:` population larger than the lowered
  cap, every mark actionable: fragment present, unreferenced, past grace. `reconcile_step` with
  a `GcContext` returns `Ok`. On the base it returns `Err(ReconcileError::Store)` (the `?` at
  `gc.rs:177`) — the red.
  **(B) One pass reads a bounded, pinned amount.** No pass receives more than `B` `orphan:`
  entries. `B` is the per-pass budget this slice chooses, a named constant with its derivation in
  the doc comment, at most 65,536 (1/16 of `SCAN_CAP`). The test **hard-codes the same
  literal**. When at least `B` entries remain, a pass receives **exactly** `B`, so a budget that
  silently under-reads fails too. The population drains in exactly `⌈P / B⌉` passes, not fewer.
  The population must exceed `B`, or this leg cannot bite (v1's leg was derived from the pass's
  own measured read and pinned nothing). No `scan` of the `orphan:` prefix happens anywhere in the
  step (`0016:1398`: `orphan:` is "never read by a single scan").
  **(C) The tail does not starve.** Seed a **retention-safe head** — more than `B` marks still
  inside their grace window, placed first in key order — followed by an **actionable tail**. Run
  passes with a fresh `GcContext` each time. Every tail fragment is reclaimed within
  `⌈(head + tail) / B⌉ + 1` passes, and no head fragment is touched. An implementation that
  restarts at the first page every pass never reaches the tail and fails. Set the head plus the
  tail above the lowered cap so the base goes red (its single scan errors).
  **(D) A mark with no fragment is swept, and only when that is safe.** Let `D` be the
  late-write deadline `W_repoint + W_write + δ_clock` (`0016:1383-1391`), a named constant with
  its derivation. The test hard-codes `D` too. Five cases: (i) a mark at a position no listed
  server reports, aged exactly `D`, with the listing taken at or after `orphaned_at + D`, is
  deleted; (ii) the same mark aged `D − 1` ms survives; (iii) a mark whose D server is not in the
  pass's fleet is never swept; (iv) a mark whose position **is** listed is never swept; (v) **a
  listing from an earlier pass never licenses a sweep** (X96, `0016:2625`): run a pass while the
  mark is aged `D − 1` ms (it survives, as (ii)), then write a fragment into that position, then
  run the next pass with the mark aged at least `D` and still inside the test's grace window —
  the mark survives, because only this pass's own listing may show the position empty. On the
  base (i) survives forever — the red. Cases (ii), (iv) and (v) pin the deadline and the listing
  rule (v1's leg checked 10 s and 50 s and pinned neither).
  **(E) `D` stays strictly inside the deployed grace.** `D <` the grace window the deployed pass
  actually uses (`GC_GRACE_WINDOW_MILLIS`, `crates/server/src/custodian.rs:114`, which is
  `LEASE_TTL_MILLIS = 60_000`, `crates/server/src/cli.rs:78`). Prove it against **that constant
  itself**, not a copy of its value, because `0016:1383-1391` requires `G_orphan > D` strictly.
  v1 compared against a hard-coded 60 000. Where the proof lives is Do's call: a
  `crates/server` test can read both, if the custodian constant is visible to it. Say which in
  `build-notes.md`.
  **(F) Restore survives the same ledger, and never re-stamps a mark it did not read.**
  `reconcile_after_restore` over a store whose `orphan:` population exceeds the lowered cap
  returns `Ok`. Restore writes a fresh stamp for any stranded fragment it does not find in its
  read of the ledger (`restore.rs:413-416`, the unconditional `put` at `:426-429`), so a restore
  that judged "already marked" from one page or one budget's worth would silently overwrite
  every mark beyond it. Seed the pre-marked fragments so that their marks sort **after** both
  the first `B` `orphan:` keys and the first lowered-cap's worth, at least one on the last page,
  each with a stamp older than the pass's clock. Every one of them keeps its value bytes
  unchanged (its grace clock is not restarted) and is counted in
  `RestoreReport::already_marked`. On the base `restore.rs:308` returns `Err` — the red.
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: GC's and restore's reads of the `orphan:` ledger, and the reclaim decision that
  consumes GC's read. That includes the sweep of marks with no fragment, and the rules for what a
  pass may conclude from a partial read. The continuation across passes must survive the
  per-pass rebuild of `GcContext` the deployed loop does (`crates/server/src/custodian.rs:601`).
  The walk's own writes (key deletes after a reclaim, sweep deletes, any persisted continuation)
  commit in bounded batches, never in one batch sized by the pass. A pass that reclaims `B` marks
  must not hand the backend a `B`-key transaction; restore bounds its marks with `MARK_BATCH` for
  the same reason (`restore.rs:103`, rationale at `:432-440`; `0016:1398-1402`).
  Whether it also survives a leader change is Do's call, stated in `build-notes.md`. Must NOT
  change `reconcile_step`'s or `reconcile_after_restore`'s signature, and must NOT add a field to
  `GcContext` (`gc.rs:72-83`) or to any other context. The test builds them with struct
  literals, so either change breaks its base compile and turns every red into UNVERIFIABLE.
  Decoding is unchanged here: marks remain the bare decimal `mark_orphaned` writes
  (`gc.rs:117-129`). A value that does not parse is left untouched, never rewritten, and never
  reclaimed on. The richer value shapes are child-2's. Docs currency (`AGENTS.md:154-157`): if
  the continuation is persisted, it is a new persisted record, so describe it and the paged walk
  in `docs/design/architecture/06-runtime-view.md`, where the custodian loops are described. /
  out of scope: the staged reference set, reclamation intent, the three `orphan:` value shapes
  and keyed retire protection (child-2, #662); the orphan-identity migration gate (X92, #659);
  drain-health telemetry beyond what an operator needs to see that a walk fell behind;
  `scrub.rs`, `reconstruction.rs`, `rebalance.rs`, `desired_state.rs`; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — gate exceeded its 7200s timeout
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 38 mutants tested in 42s: 4 missed, 11 caught, 23 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.04s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: replace whole-ledger `orphan:` scans with a bounded, resumable GC walk and safe fragment-less-mark cleanup without resetting restore evidence — advisory verdict: the patch is not safe to accept yet.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether this slice must enforce `W_repoint`/`W_write` before enabling fragment-less sweeping or defer that sweep — both live maintenance writers still authorize destination writes with no deadline, so a finite `D` is not yet a sound boundary (`crates/custodian/src/reconstruction.rs:934`; `crates/custodian/src/rebalance.rs:534`). |
| C2 Reproduction (red pre-fix) | PASS | The retained test compiled against the stashed base and independently produced five assertion failures; restoring the patch made all 9 tests green (`crates/custodian/tests/gc_ledger_walk.rs:288`, `crates/custodian/tests/gc_ledger_walk.rs:385`, `crates/custodian/tests/gc_ledger_walk.rs:439`, `crates/custodian/tests/gc_ledger_walk.rs:639`). |
| C3 Change | FAIL | The change does not restore the stated bounded-and-safe lifecycle: restore still accumulates the entire ledger, an unread orphan can fall through to pending reclamation, and a sweep deletes without pinning the value it observed (`crates/custodian/src/gc.rs:284`, `crates/custodian/src/gc.rs:336`, `crates/custodian/src/gc.rs:695`). |
| C4 Verification (red→green) | PASS | Independent stash/restore reproduced five red assertions and 9/9 green; affected-package tests, clippy, fmt, typos, and docs passed, while the full gate's remaining `cargo deny` failure was a host-only read-only advisory-lock fault (`crates/custodian/tests/gc_ledger_walk.rs:281`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must cover the causal counter-cases — an orphan outside the current page must outrank an expired pending record, and deletion must lose to a concurrent mark refresh — because the current sequential examples exercise neither branch (`crates/custodian/src/gc.rs:273`, `crates/custodian/src/gc.rs:334`; `crates/custodian/tests/gc_ledger_walk.rs:569`). |
| T1 Structure | PASS | The patch keeps public reconciliation/context signatures intact and places the persisted cursor beside GC with the required living-architecture update (`crates/custodian/src/gc.rs:101`; `docs/design/architecture/06-runtime-view.md:74`). |
| T2 Shape | FAIL | Restore's page loop extends every page into one `HashMap`, so ledger cardinality still controls peak memory and can merely exchange `SCAN_CAP` failure for OOM (`crates/custodian/src/gc.rs:695`). |
| T3 Runtime | FAIL | A within-grace orphan omitted from this page is reclaimable through the expired-pending arm, and the blind fragment-less delete can erase a newer concurrent stamp, risking premature byte deletion or permanently unevidenced bytes (`crates/custodian/src/gc.rs:273`, `crates/custodian/src/gc.rs:284`, `crates/custodian/src/gc.rs:334`, `crates/custodian/src/gc.rs:336`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent during Check; the deferred gate states that their substantive audit is mandatory at publish (`check-gates.json:115`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add seeded Tier-0 coverage for the new destructive/concurrent path and assert intermediate commit count/size — the sequential suite leaves four batch-control mutants alive (`crates/custodian/tests/gc_ledger_walk.rs:439`; `crates/custodian/src/gc.rs:297`; `crates/custodian/src/gc.rs:337`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected walk's production memory, transaction, progress, and operator-observability envelope is acceptable at maximum ledger cardinality — in-memory functional tests do not establish those deployment properties (`crates/custodian/src/gc.rs:53`; `crates/custodian/src/gc.rs:93`). |

Prior-art check: PASS — the brief names both affected paths and records the merged/open search plus rejected #508 and #637 approaches (`brief.md:157`).


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether this slice must enforce `W_repoint`/`W_write` before enabling fragment-less sweeping or defer that sweep — both live maintenance writers still authorize destination writes with no deadline, so a finite `D` is not yet a sound boundary (`crates/custodian/src/reconstruction.rs:934`; `crates/custodian/src/rebalance.rs:534`).
- [ ] C5 Causal adequacy — Rebuild must cover the causal counter-cases — an orphan outside the current page must outrank an expired pending record, and deletion must lose to a concurrent mark refresh — because the current sequential examples exercise neither branch (`crates/custodian/src/gc.rs:273`, `crates/custodian/src/gc.rs:334`; `crates/custodian/tests/gc_ledger_walk.rs:569`).
- [ ] T5 Judgment — Rebuild must add seeded Tier-0 coverage for the new destructive/concurrent path and assert intermediate commit count/size — the sequential suite leaves four batch-control mutants alive (`crates/custodian/tests/gc_ledger_walk.rs:439`; `crates/custodian/src/gc.rs:297`; `crates/custodian/src/gc.rs:337`).
- [ ] Validation — fitness-to-purpose — Decide whether the corrected walk's production memory, transaction, progress, and operator-observability envelope is acceptable at maximum ledger cardinality — in-memory functional tests do not establish those deployment properties (`crates/custodian/src/gc.rs:53`; `crates/custodian/src/gc.rs:93`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — gate exceeded its 7200s timeout
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must cover the causal counter-cases — an orphan outside the current page must outrank an expired pending record, and deletion must lose to a concurrent mark refresh — because the current sequential examples exercise neither branch (`crates/custodian/src/gc.rs:273`, `crates/custodian/src/gc.rs:334`; `crates/custodian/tests/gc_ledger_walk.rs:569`).; T5 Judgment — Rebuild must add seeded Tier-0 coverage for the new destructive/concurrent path and assert intermediate commit count/size — the sequential suite leaves four batch-control mutants alive (`crates/custodian/tests/gc_ledger_walk.rs:439`; `crates/custodian/src/gc.rs:297`; `crates/custodian/src/gc.rs:337`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
