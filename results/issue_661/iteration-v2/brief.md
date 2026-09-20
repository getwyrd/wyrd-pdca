# Brief — issue 661 / gc-orphan-ledger-paged-walk

> Child 1 of 4 of #637's split (637.1). Do reads ONLY this file. Keep the `- **Label:** value`
> lines. Every `path:line` below is on `origin/main` @ `3969a3a`, re-verified 2026-09-12.
> Background: proposal 0016 `docs/design/proposals/draft/0016-multipart-commit-protocol.md`
> `:1359-1404` (read it before writing code — it is the design this slice implements).
> Amended 2026-09-12 by the plan-review pass on #637 (`results/issue_637/brief.md`, *Plan-review
> response* F2): leg D gained case (v), and leg F now places the existing marks off the first page.

- **Slug:** gc-orphan-ledger-paged-walk
- **Kind:** enhancement
- **Defect:** the `orphan:` ledger is read with **one `scan`**, in two places: GC
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
- **Success criterion:** the NEW file `crates/custodian/tests/gc_ledger_walk.rs` passes. It runs
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
- **Falsifiability:** RED is produced in-process on this bundle's base (`origin/main`), no
  container or cluster needed. Legs A, C, D(i) and F fail by **assertion** there. Leg B's
  per-pass bound goes red as part of A: the base errors before reading a page. Leg D(ii)–(v) and
  E guard against over-deletion and may already be green on the base; that is expected. The test
  file may name only symbols present on `origin/main`: `wyrd_custodian::{reconcile_step,
  reconcile_after_restore, GcContext, ExpiredPendingPolicy, Custodian, FencedZone, Reconciled,
  ReconcileError, RestoreReport}` (`crates/custodian/src/lib.rs:33-44`),
  `wyrd_core::metadata::{orphan_key, ORPHAN_PREFIX}` (`crates/core/src/metadata.rs:62-78`),
  `wyrd_traits::{MetadataStore, ChunkStore, ScanPage, ScanCapExceeded, WriteBatch, CommitOutcome}`,
  and nothing this slice adds. Each budget constant appears in the test as a literal. The RED
  leg reports **UNVERIFIABLE**, not red, if the file fails to compile with the production change
  reverted (`engine/scripts/run-verify.sh:521-547`), so a build error is a defect in the test.
  Record in `build-notes.md` how many tests ran red and that each failure was an assertion.
- **Invariant to restore:** the `orphan:` ledger is readable at every size, and every mark has a
  deleter. No read of it can fail on its size. A pass's footprint is bounded by a constant, not
  by the ledger. A pass that read part of the ledger draws only retention-safe conclusions from
  it. For GC, an unread mark leaves its fragment looking unmarked, which the conservative arm
  keeps (`gc.rs:206-210`). For restore, an unread mark would look absent and be re-stamped
  (`restore.rs:413-429`), so restore's "already marked" judgement must see every existing mark,
  on whatever page it sits; how it does so without a whole-ledger read is Do's call. Nothing is
  destroyed **or overwritten** because a record was absent from a partial read.
  Source: `0016:1359-1404` (with `0016:1392-1404`: "accessible at every cardinality"), and the
  C-1 rule that a permanent, self-sealing failure mode is a defect (`docs/principles.md` §5).
  SELF-TEST: guarding GC alone does not satisfy it — restore reads the same ledger with the same
  scan (`restore.rs:308`), and leg F fails if only `gc.rs` is fixed.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of #637's split. The only external prerequisite, #634 (`scan_page`,
  PR #645), is merged. #661's tracker body asks to schedule after #638; #638 is merged
  (PR #770). child-2 builds on this slice's accepted patch.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** GC's and restore's reads of the `orphan:` ledger, and the reclaim decision that
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
- **Repro instruction:** on `origin/main`, seed more than `SCAN_CAP` keys under `orphan:` in any
  backend whose cap is lowered, as `crates/metadata-redb/tests/scan.rs` does, and run one GC
  pass: `orphan_leases` returns `ScanCapExceeded` and the pass errors.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/gc_ledger_walk.rs` — a **NEW** file under `tests/`.
  The C4-verify gate earns its red only from an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:141-144`, `:390-392`), so a case appended to
  `crates/custodian/tests/gc.rs` would degrade to green-only. `wyrd-custodian`'s dev-dependencies
  already cover the doubles (`async-trait`, `bytes`, `tokio`, `wyrd-testkit`,
  `wyrd-coordination-mem`). Make **no** `Cargo.toml` change: it is reverted on the RED leg.
- **Production reach:** the passes under test are the production `reconcile_step` and
  `reconcile_after_restore`. Only the store and the D-server fleet are in-memory, as
  `crates/custodian/tests/gc.rs` already does it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/gc.rs:142-245` — the reclaim pass to restructure. Keep the safety gate
    (`:191`), the grace test (`:196-205`) and the conservative arm (`:206-210`) exactly as they
    judge.
  * `crates/custodian/src/gc.rs:522-538` — `orphan_leases`, the single scan to remove.
  * `crates/custodian/src/restore.rs:300-318` and `:406-440` — restore's `already` read and its
    bounded `MARK_BATCH` commit (`:103`), the idempotence the `already` check provides.
  * `crates/custodian/tests/gc.rs:56-130` — the `MemMeta` / `MemDServer` doubles to extend.
    Replace its `scan_page` delegation (`:80-87`) with a direct implementation in the new file.
  * `crates/traits/src/lib.rs:334`, `:414-562` — `ScanPage` and the shared page helpers
    (`page_limit`, `page_start`, `page_is_full`, `page_cursor`) a double should use.
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/gc.rs`,
  `crates/custodian/src/restore.rs`) across merged history and open PRs: `orphan_leases` has
  been a single scan since M3 (`af4ab65`), and no merged or open change pages it. Rejected prior
  art: #508's 7th attempt swapped the scan for an unbounded `loop { scan_page }` into one
  `HashMap`, which is why leg B pins the budget. #637 v1 (`results/issue_637/iteration-v1/`)
  paged it, but its bounds were not pinned (adversary findings on legs G and H2(c)).
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must cover the causal counter-cases — an orphan outside the current page must outrank an expired pending record, and deletion must lose to a concurrent mark refresh — because the current sequential examples exercise neither branch (`crates/custodian/src/gc.rs:273`, `crates/custodian/src/gc.rs:334`; `crates/custodian/tests/gc_ledger_walk.rs:569`).; T5 Judgment — Rebuild must add seeded Tier-0 coverage for the new destructive/concurrent path and assert intermediate commit count/size — the sequential suite leaves four batch-control mutants alive (`crates/custodian/tests/gc_ledger_walk.rs:439`; `crates/custodian/src/gc.rs:297`; `crates/custodian/src/gc.rs:337`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 38 mutants tested in 42s: 4 missed, 11 caught, 23 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: The slice is too big and bundles two hard problems that keep producing new implementation-shaped findings each round rather than converging: (1) paging the orphan-ledger reads (mechanically bounded, largely solved by this attempt) and (2) making the reclaim/sweep decision correct under concurrency and key-identity ambiguity (not solved — round 2 fixed round 1's unconditional-write races by adding guarded/conditional commits, but that new mechanism has its own bugs: a commit "conflict" is treated as proof another writer's record exists, when it can also mean a rival transaction that itself aborted, so a stray fragment can be wrongly reported as protected; and a hard error partway through a batch's one-at-a-time retry throws away the already-landed results from earlier writes in the same loop, silently dropping sweep/mark audit events). Separately, this slice's new fragment-less-sweep logic is the first code path exposed to a pre-existing, out-of-scope key-aliasing defect (two differently-formatted orphan keys decoding to the same fragment, tracked under #659) — the brief correctly scoped that out, but the new sweep logic isn't safe against it as written. Re-plan direction: split the bounded-paging mechanics (GC + restore reading the ledger in pages, budget/tail-starvation guarantees — legs A/B/C/F) from the reclaim-safety work (fragment-less sweep under the late-write deadline, guarded/conditional writes, and their interaction with key-identity aliasing — legs D/E plus the new Guarded/GuardedBatch machinery). Coordinate the second slice's scope with #659 (orphan-identity migration) rather than re-excluding it outright, since this round showed the exclusion doesn't hold once sweeping ships. Also open a size-signal/backstop review as a §10 Act note: this bundle's own size backstop (115 KB vs 100 KB threshold) correctly called for iterate-plan up front — worth checking why the earlier auto-iterate step chose iterate-do for round 2 instead of routing here immediately.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The slice is too big and bundles two hard problems that keep producing new implementation-shaped findings each round rather than converging: (1) paging the orphan-ledger reads (mechanically bounded, largely solved by this attempt) and (2) making the reclaim/sweep decision correct under concurrency and key-identity ambiguity (not solved — round 2 fixed round 1's unconditional-write races by adding guarded/conditional commits, but that new mechanism has its own bugs: a commit "conflict" is treated as proof another writer's record exists, when it can also mean a rival transaction that itself aborted, so a stray fragment can be wrongly reported as protected; and a hard error partway through a batch's one-at-a-time retry throws away the already-landed results from earlier writes in the same loop, silently dropping sweep/mark audit events). Separately, this slice's new fragment-less-sweep logic is the first code path exposed to a pre-existing, out-of-scope key-aliasing defect (two differently-formatted orphan keys decoding to the same fragment, tracked under #659) — the brief correctly scoped that out, but the new sweep logic isn't safe against it as written.
  Re-plan direction: split the bounded-paging mechanics (GC + restore reading the ledger in pages, budget/tail-starvation guarantees — legs A/B/C/F) from the reclaim-safety work (fragment-less sweep under the late-write deadline, guarded/conditional writes, and their interaction with key-identity aliasing — legs D/E plus the new Guarded/GuardedBatch machinery). Coordinate the second slice's scope with #659 (orphan-identity migration) rather than re-excluding it outright, since this round showed the exclusion doesn't hold once sweeping ships. Also open a size-signal/backstop review as a §10 Act note: this bundle's own size backstop (115 KB vs 100 KB threshold) correctly called for iterate-plan up front — worth checking why the earlier auto-iterate step chose iterate-do for round 2 instead of routing here immediately.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
