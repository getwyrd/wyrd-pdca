<!-- pdca:split-proposal v1 -->
# Split proposal — issue 637

> **Intake-cap override (wyrd-pdca-P1).** Granted by the human (Eduard Ralph) in this Plan
> session, 2026-09-12, for **issue 637's split only**. The count at the override:
> `planned 21/6 (cap) — room for 0, need 1: Plan intake closed`. Writing this parent's brief made
> 637 itself PLANNED (`planned 22/6 (cap) — room for 0, need 4: Plan intake closed`), and it
> stays counted until the split is signed off. Accepting materialises four bundles, taking the
> count to 26.
>
> **The children already exist on the tracker.** #661–#665 were filed as sub-issues of #637
> (637.1–637.5) in the 2026-07-31 re-slicing, and iteration v1 built 637 whole regardless.
> Accept with `--ids` so nothing is filed twice:
> `pdca split 637 --accept --ids 661,662,663,664` (child-1 → #661 … child-4 → #664).
> **#665 is deliberately not a child here** — see *Why*.
>
> **Design call settled in this session (2026-09-12): option (i).** A `Completing` session
> stores its segment-group nonce on its session record, so the restore fence can name that
> attempt's `seg:` records. It lands in child-4 (#664).

## Why this slice is oversized

Iteration v1 delivered all of 637 as one patch: 334 KB across 20 files. Sign-off sent it back
to Plan for three reasons, and each one maps onto a seam below.

1. **Four outcomes, each able to ship alone.** (a) The `orphan:` ledger becomes readable at any
   size. (b) The staged class exists and GC and restore protect it. (c) Repair covers staged
   chunks. (d) Drain and restore handle live uploads correctly. They share one reference set,
   but each has its own oracle in 0016's failure table (`0016:874-890`) and its own failure mode.
2. **v1's findings fell along those seams.** Check found unrelated gaps: re-place safety
   (undecodable source mark, a destination mark that should be re-stamped, the write deadline
   never exercised), a drain that never had to answer `Satisfied`, per-pass bounds the tests did
   not pin, and a restore fence that installed no deleter for a `Completing` session's `seg:`
   records. Each gap now belongs to exactly one child, as a named leg.
3. **An unsettled design call was buried in the build.** A `Completing` session record does not
   carry its segment-group nonce (`crates/core/src/multipart.rs:1842-1849`), so restore could not
   name the records it had to retire. That was settled here — option (i), above — and child-4
   carries it.

**Scope moved relative to the tracker bodies, each for a stated reason:**

- **#665 (637.5, DST race cases) is not materialised.** It depends on #659 (the retire drain),
  which has no bundle yet. Two of its cases now live elsewhere: X29 (re-place against a session
  fence) is child-3's own leg, next to the code it tests, and X59 (drain request against the
  upload intent) is the upload writer's fence, which #657's body already owns. What remains —
  the full-plane observable, and "the `seg:` range drains empty" — needs #659 and stays on #665.
- **The fragment-less mark sweep goes to child-1**, not child-2. 0016 states the sweep and the
  paged walk as one rule set (`0016:1359-1404`). The sweep is the sharpest case of "what a
  partial walk may conclude", which #661 owns.
- **The orphan-identity migration gate (X92) leaves 637.** `0016:1259-1273` gates "the new
  retirement paths" on a completed cleanup pass, and #659 is the slice that turns them on.
  Before #659 nothing writes identity-carrying marks, so the gate would protect nothing. Adding
  keyed protection (child-2) without the gate only ever keeps more bytes, never fewer. This also
  removes v1's worst adversary finding from 637: its stale-mark cleanup left a deleted object's
  bytes with no way to be reclaimed.
- **Evacuating a committed segmented object stays with #653/#722**, as v1 already deferred it.

## Wave sketch

Three waves. Every child depends on everything to its left:

```
wave 1: child-1 (#661)  — ledger walk: paged, budgeted, cap-safe, sweeps marks with no fragment
wave 2: child-2 (#662)  — staged set + protection, keyed retire protection, reclaim intent, 3 value shapes
wave 3: child-3 (#663) ┐  scrub + staged re-place (+ X29 in DST)
        child-4 (#664) ┘  drain status + rebalance + restore protect/fence/generation
```

- **child-2 depends on child-1.** Both rewrite GC's reclaim path in `crates/custodian/src/gc.rs`,
  and child-2's reclamation-intent write lands in the mark-driven walk child-1 builds. That is a
  real build-on dependency, not just a shared file.
- **child-3 and child-4 depend on child-2.** Both consume the staged set, and child-3's
  adoption CAS relies on child-2's reclaim-intent ordering (`0016:1312-1336`).
- **child-3 and child-4 share a wave.** Their production files are disjoint: child-3 touches
  `scrub.rs`, `reconstruction.rs` and the DST file; child-4 touches `desired_state.rs`,
  `rebalance.rs`, `restore.rs`, `crates/core/src/multipart.rs`, `crates/server/src/cli.rs` and the
  docs. Each child's `Scope` lists the other's files as out of bounds, to keep it that way.
- **Outside this proposal** (the ordering fields can name only siblings, so these ids are added
  to the materialised briefs after acceptance): child-3 conflicts with **#777** (both edit
  `reconstruction.rs` and `crates/dst/tests/custodian.rs`). child-4 edits
  `crates/core/src/multipart.rs`, which **#693** (ITERATE_DO) also edits. **#508** and **#625**
  today name `637` in `Depends on`, and a split parent reaches COMPLETE at its own sign-off,
  before any child lands (`src/pdca_harness/cli.py:1060-1073`). Their edge is re-pointed to
  child-3 and child-4 after acceptance.

<!-- pdca:child child-1 -->
# Brief — issue 661 / gc-orphan-ledger-paged-walk

> Child 1 of 4 of #637's split (637.1). Do reads ONLY this file. Keep the `- **Label:** value`
> lines. Every `path:line` below is on `origin/main` @ `3969a3a`, re-verified 2026-09-12.
> Background: proposal 0016 `docs/design/proposals/draft/0016-multipart-commit-protocol.md`
> `:1359-1404` (read it before writing code — it is the design this slice implements).

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
  its derivation. The test hard-codes `D` too. Four cases: (i) a mark at a position no listed
  server reports, aged exactly `D`, with the listing taken at or after `orphaned_at + D`, is
  deleted; (ii) the same mark aged `D − 1` ms survives; (iii) a mark whose D server is not in the
  pass's fleet is never swept; (iv) a mark whose position **is** listed is never swept. On the
  base (i) survives forever — the red. Cases (ii) and (iv) pin the deadline and the listing rule
  (v1's leg checked 10 s and 50 s and pinned neither).
  **(E) `D` stays strictly inside the deployed grace.** `D <` the grace window the deployed pass
  actually uses (`GC_GRACE_WINDOW_MILLIS`, `crates/server/src/custodian.rs:114`, which is
  `LEASE_TTL_MILLIS = 60_000`, `crates/server/src/cli.rs:78`). Prove it against **that constant
  itself**, not a copy of its value, because `0016:1383-1391` requires `G_orphan > D` strictly.
  v1 compared against a hard-coded 60 000. Where the proof lives is Do's call: a
  `crates/server` test can read both, if the custodian constant is visible to it. Say which in
  `build-notes.md`.
  **(F) Restore survives the same ledger.** `reconcile_after_restore` over a store whose
  `orphan:` population exceeds the lowered cap returns `Ok`. A fragment that already carries a
  mark keeps its value bytes unchanged, so its grace clock is not restarted, and it is counted in
  `RestoreReport::already_marked`. On the base `restore.rs:308` returns `Err` — the red.
  **(G) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on this bundle's base (`origin/main`), no
  container or cluster needed. Legs A, C, D(i) and F fail by **assertion** there. Leg B's
  per-pass bound goes red as part of A: the base errors before reading a page. Leg D(ii)–(iv) and
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
  it: an unread mark leaves its fragment looking unmarked, which GC's conservative arm keeps
  (`gc.rs:206-210`). Nothing is destroyed because a record was absent from a partial read.
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# Brief — issue 662 / staged-reference-set-and-reclaim-intent

> Child 2 of 4 of #637's split (637.2). Do reads ONLY this file. Keep the `- **Label:** value`
> lines. `path:line` citations are on `origin/main` @ `3969a3a` (re-verified 2026-09-12). This
> bundle's base is `origin/main` **plus #661's accepted patch** (wave 2), so lines in
> `crates/custodian/src/gc.rs` and `restore.rs` will have moved: re-locate them by symbol on the
> base. Background: 0016 decision 2 (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:765-893`,
> table `:820-871`, failure table `:874-890`) and the ledger rules `:1189-1247`, `:1312-1336`.

- **Slug:** staged-reference-set-and-reclaim-intent
- **Kind:** enhancement
- **Defect:** staged bytes have no protection class, and GC destroys bytes before recording
  that it is doing so.
  1. **Staged bytes are unprotected.** `ReferenceSet` holds committed placements only
     (`crates/custodian/src/gc.rs:265-340`). A committed part's fragments (`part:` records) and an
     upload's in-flight fragments (owned `sidx:` entries, #772) are in no protected set. GC
     reclaims one as soon as it carries an `orphan:` mark past grace (`gc.rs:191-217`). Restore
     gates on the same predicate (`crates/custodian/src/restore.rs:383`), and since #772 moved
     owned entries out of `pending:`, its `pending:` skip (`restore.rs:420-424`) no longer sees
     them either. So restore marks a live upload's fragments stranded, and the next GC pass
     deletes them.
  2. **Nothing protects the bytes of a retirement still being drained.** A pending
     `retire:bytes:` obligation (`crates/core/src/multipart.rs:1059`, record types from #771)
     protects nothing today.
  3. **GC destroys first and records second.** It calls `delete_fragment` (`gc.rs:214`) before
     committing the key delete (`gc.rs:231`). An adoption CAS preconditioned on a pre-mark's
     original bytes can therefore still succeed after the fragment is gone (`0016:1300-1322`).
  4. **Only one mark format decodes.** The ledger reads only a bare decimal (`gc.rs:526-535`).
     0016's two structured shapes (`0016:1189-1204`, `:1323-1336`) would be misread or dropped.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Session, part and owned-entry records are seeded as the bytes the base
  decoders accept: `SessionRecord` and `PartRecord` have no writer-side constructor
  (`crates/core/src/multipart.rs:2005-2008`, `:2370`), so seed raw JSON in the shape
  `crates/core/tests/multipart_session_records.rs:81-145` builds, and round-trip each fixture
  through `decode_session_record` / `decode_part_record` / `decode_owned_entry` to prove it is
  valid. Legs:
  **(A) GC protects both staged classes, with the evidence present.** Seed an `Open` session
  with a committed `part:` record (fragment `F1`) and an in-flight owned `sidx:` entry
  (fragment `F2`), both placed on D-server doubles. Give each an `orphan:` mark aged past grace,
  then run `reconcile_step` with a `GcContext`: both survive. The mark is what makes the leg
  bite: without it GC's conservative arm keeps an unmarked fragment anyway (`gc.rs:206-210`).
  On the base both are reclaimed — the red.
  **(B) Restore protects them through the same predicate, and the loss it prevents is
  shown.** `reconcile_after_restore` over the same store, with no marks, writes no `orphan:` key
  for `F1` or `F2`, and `RestoreReport::stranded_marked` does not count them. Then advance past
  grace and run GC: both survive. On the base restore marks them and GC deletes them — the red.
  The separate `staged_skipped` counter is child-4's; do not assert it here.
  **(C) Source before destination, for both handoffs.** 0016 makes the read order
  `sidx:` → `part:` → committed inodes normative (`0016:782-800`). A build that reads a
  destination before its source can see a chunk in neither class. Drive each handoff with a
  store double that performs it atomically *between* the builder's two reads: it fires after the
  first read of either range for that session completes, whichever range comes first.
  (i) The part commit: one batch deletes the chunk's `sidx:` entry and writes the `part:`
  record. (ii) The publication: the committed inode naming the chunk is written, and the `part:`
  record is removed as the records drain would remove it, both between the builder's two reads.
  X67 is this variant. In both,
  the fragment is marked and past grace, and GC must not reclaim it. On the base the chunk is
  unprotected throughout, so it is reclaimed — the red.
  **(D) The staged build is bounded per session, never a global scan.** With the double's
  lowered `scan` cap, seed more sessions-with-parts than a global `scan("part:")` could hold
  (0016's `SCAN_CAP / MAX_PARTS_PER_SESSION` row, `0016:890`), each session's own ranges below
  the cap. `reconcile_step` still succeeds, and the double sees no `scan` of the `part:` or
  `sidx:` prefix as a whole. This guards the design and may be green on the base.
  **(E) A pending byte retirement protects its fragments, by keyed lookup (X97).** Seed a mark in
  the structured shape `{orphaned_at_millis, event}`, where `event` is a `RetireToken`'s
  canonical string (`crates/core/src/multipart.rs:1352-1368`). Its fragment is unreferenced and
  past grace. While `retire:bytes:<event>` is present the fragment survives. Delete that key and
  the next pass reclaims it. The protection is one keyed read per candidate, never the `retire:`
  namespace read as a range (`0016:1226-1247`): the double records any `scan` / `scan_page` of
  `retire:`, and the test asserts there is none. On the base the structured value does not
  decode, so the fragment is never reclaimed, and the second half is the red.
  **(F) Reclamation is recorded before destruction (`0016:1312-1336`).** Four cases:
  (i) the double **errors** on the commit that records reclaim intent, and the fragment is
  still present after the pass; (ii) the mark's bytes **change** between GC's read and its
  intent commit, so that commit is a `Conflict`, and that one fragment survives while the other
  fragments in the same pass are still reclaimed (v1's fallback path for a lost CAS had no
  test); (iii) the double's `delete_fragment` hook attempts an adoption CAS
  `require(orphan:<pos> == <the bytes GC read>)` at the instant GC deletes, and it gets
  `Conflict`; (iv) restart: a mark already in the `reclaiming` shape over a present fragment is
  finished on the next pass — the fragment deleted exactly once, then the key — with no second
  grace test, even when its stamp is recent. On the base (i) and (iii) show the fragment deleted
  first, and (iv)'s value does not decode — the reds.
  **(G) All three value shapes decode, and none is rejected (`0016:1189-1204`).** Seed one mark
  of each shape: legacy bare decimal (what `mark_orphaned` writes, `gc.rs:117-129`),
  `{orphaned_at_millis, event}`, and `{orphaned_at_millis, event?, reclaiming: true}`. GC
  honours each one's meaning. Restore treats all three as already marked, and their bytes are
  unchanged after a restore pass: a legacy value is never rewritten on read (the
  `already_marked` property). A value that decodes as **none** of the three fails closed: GC and
  restore leave it byte-identical, never reclaim its fragment, and surface it on the durability
  audit seam (ADR-0045, `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).
  **(H) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on this bundle's own base — `origin/main` +
  #661. When the waves run in one flow the driver exports that fold as `$PDCA_VERIFY_BASE`, which
  `engine/scripts/run-verify.sh` honours ahead of the brief's target (`:247-265`); once #661 has
  merged, `main` itself is that base. No container is needed. Legs A, B, C, E, F(i),
  F(iii), F(iv) and G fail by **assertion** there, because every record class they seed exists on
  `main` (#691, #715, #716, #771, #772) while nothing in the maintenance plane reads it. D is a
  guard. The test may name only base-visible symbols: `wyrd_custodian::{reconcile_step,
  reconcile_after_restore, GcContext, ExpiredPendingPolicy, Custodian, FencedZone, Reconciled,
  RestoreReport}`; `wyrd_core::multipart::{mpu_key, part_key, sidx_key, retire_key, UploadId,
  PartNumber, OwnedEntry, StagedPlacement, RetireMode, RetireToken, decode_session_record,
  decode_part_record, decode_owned_entry}`; `wyrd_core::metadata::{orphan_key, encode}`;
  `wyrd_traits` store types. It may not name anything this slice adds — no new
  `ReferenceSet`, context or report field, and no new codec type — so the structured `orphan:`
  values are written as raw JSON bytes. A compile failure on the RED leg reports UNVERIFIABLE,
  not red (`run-verify.sh:521-547`). Record in `build-notes.md` how many tests ran red, all by
  assertion.
- **Invariant to restore:** every durable byte is, at every instant, classifiable as
  committed-referenced, staged-with-a-named-exit, or garbage-with-a-sound-reclamation-path, and
  no pass destroys a byte before that destruction is durable in metadata. The class lives in the
  **shared** reference set every destructive pass reads. It is a disjoint member, not merged
  into `placed`, so each consumer can make its own decision (`0016:767-782`, `:881`). Protection
  deliberately **overlaps** across each handoff: the rule is "no gaps", never a partition
  (`0016:2911-2922`). Source: 0016 invariant (2) (`0016:869-871`); the custodian rule that a
  referenced fragment is never reclaimed (proposal 0005, `docs/design/proposals/accepted/0005-milestone-3-custodians.md:294-296`, enforced at `gc.rs:191`); ADR-0045.
  SELF-TEST: a filter inside GC alone passes leg A while restore strands the parts and the next
  GC pass deletes them. That is leg B, and why the class belongs in the shared set.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Ordering note:** wave 2. child-1 (#661) rewrites the same reclaim path into a paged,
  mark-driven walk, and this slice's reclaim-intent write lands inside it, so this is a
  build-on dependency, not only a shared file. The tracker body also names #654 (the record
  types); #654 was split and its record types landed in #691, #715, #716, #771 and #772, all
  merged. child-3 and child-4 build on this slice.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **Scope:** the staged protection class in the shared reference set, built from the
  committed `part:` records and owned `sidx:` entries of the sessions listed under `mpu:`. It
  uses bounded per-session ranges in source-before-destination order, and is exposed as its own
  member alongside committed placements. The protection predicate every destructive pass shares
  honours it. It must never cover less than 0016's set; covering every session's records
  whatever its state is acceptable, because it only keeps more. Also: keyed protection while a
  byte retirement is pending; the reclaim-intent ordering and its restart; the three `orphan:`
  value shapes, their dual-format decoding, and fail-closed handling of a value that matches
  none. The codec belongs where #659's drain and child-3's re-place can both reach it.
  `mark_orphaned`'s legacy output is unchanged. Must NOT change `reconcile_step`'s or
  `reconcile_after_restore`'s signature, and must NOT add a field to any context struct or to
  `RestoreReport` (the base-compiling test builds them with struct literals). Docs currency
  (`AGENTS.md:154-157`: new persisted value shapes): describe the staged protection class and the
  `orphan:` value shapes in `docs/design/architecture/08-crosscutting-concepts.md`, extending
  what #635 wrote there. / out of scope: the orphan-identity migration gate and its cleanup
  pass (X92, `0016:1249-1273` — it guards #659's retirement paths, and no identity-carrying
  mark exists before #659); drain status, rebalance, restore's counters and session fence
  (child-4); scrub and reconstruction (child-3); the ledger walk's paging and the sweep of marks
  with no fragment (child-1 — keep its rules intact); `desired_state.rs`, `rebalance.rs`,
  `scrub.rs`, `reconstruction.rs`; any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open` session (`mpu:`), one `part:` record
  and one owned `sidx:` entry whose fragments sit on a D server, give each an `orphan:` mark
  older than the grace window, and run one GC pass: both fragments are deleted.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_protection.rs` — a **NEW** file. The C4-verify
  gate earns its red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`,
  `:390-392`). The existing dev-dependencies suffice; make no `Cargo.toml` change.
- **Production reach:** the passes under test are the production `reconcile_step` and
  `reconcile_after_restore`, over in-memory stores. No client-created session exists until the
  S3 verbs (#508), so every staged record is seeded by the test. That is the intended state.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/gc.rs:265-340` — `ReferenceSet`, `protection()` and `protects()`: the
    structure to extend with a disjoint member and a `"staged"` reason.
  * `crates/custodian/src/gc.rs:360-455` — `referenced_fragments`, the shared builder (read by
    scrub `scrub.rs:88`, drain status `desired_state.rs:188`, restore `restore.rs:298`); the
    staged read goes **before** its `inode:` scan (`:365`).
  * `crates/core/src/multipart.rs:1118-1240` — the `mpu:` / `part:` / `sidx:` key and range
    helpers; `:3422-3450` `StagedPlacement` and `:3513-3620` `OwnedEntry` / `decode_owned_entry`.
  * `crates/core/src/multipart.rs:1352-1392` — `RetireToken`'s canonical string, `retire_key`,
    `parse_retire_key`.
  * `crates/core/tests/multipart_session_records.rs:81-145` — fixture builders for session and
    part JSON.
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/gc.rs`, `restore.rs`)
  across merged history and open PRs: no merged change adds a staged reference class
  (`git log -S'sidx' -- crates/custodian/src` is empty on `origin/main`), and no open PR
  touches the custodian. Rejected prior art: #508's 4th attempt added a resolver that only the
  read path used, while GC and restore walked maps directly, so restore stranded parts and GC
  deleted them. That is why leg B exists. #637 v1 built this whole class inside a 334 KB patch
  (`results/issue_637/iteration-v1/`), and its review found the lost-CAS fallback untested,
  which is leg F(ii).
- **Disposition hint:** likely-fix
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
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
- **Depends on:** child-2
- **Ordering note:** wave 3, beside child-4. The two touch disjoint files, so they share the
  wave. child-2 supplies the staged set and the reclaim-intent ordering. The adoption CAS's
  precondition on the pre-mark's original bytes is only sound because GC records `reclaiming`
  before deleting (`0016:1312-1336`). **Outside the proposal:** this slice conflicts with #777
  (segmented repair through `repoint_chunk`), which also edits `crates/custodian/src/reconstruction.rs`
  and `crates/dst/tests/custodian.rs`. That id is added to `Conflicts with` after acceptance.
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
<!-- pdca:end child-3 -->

<!-- pdca:child child-4 -->
# Brief — issue 664 / staged-drain-and-restore-fence

> Child 4 of 4 of #637's split (637.4). Do reads ONLY this file. Keep the `- **Label:** value`
> lines. `path:line` citations are on `origin/main` @ `3969a3a` (re-verified 2026-09-12). This
> bundle's base is `origin/main` **plus #661 and #662** (wave 3). #662 already makes restore's
> mark gate skip staged fragments through the shared predicate, so what is left here is
> restore's *accounting* and its *fence*. Background: the restore, rebalance and drain rows of
> 0016's decision-2 table (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`),
> the failure table `:874-890`, decision 1.4 (`:717-728`), and the fence rows of the batch table
> (`:664-665`).
>
> **Design call settled at Plan (2026-09-12, the human, option (i)):** a `Completing` session
> stores its **segment-group nonce** on its session record, alongside the fence epoch
> `PublishTarget` already carries (`crates/core/src/multipart.rs:1842-1849`). That lets the
> restore fence name the attempt's `seg:<nonce>:<E>:` records. Rejected: deriving the nonce from
> `(upload id, E)` as `0016:2333` says, because the code keeps the nonce independent of the
> upload id on purpose (`multipart.rs:3185-3190`, citing `0016:499-509`); and leaving the
> records without a deleter (v1's outcome).

- **Slug:** staged-drain-and-restore-fence
- **Kind:** enhancement
- **Defect:** four gaps, all on the operator-facing and post-restore side.
  1. **Drain status ignores staged bytes.** `reconciliation_status` answers `Satisfied` for a
     server holding only staged bytes, because `genuinely_holds` reads committed placements
     alone (`crates/custodian/src/desired_state.rs:188-196`). An operator is then told the
     server may be wiped under a live upload — the F6 trace. Its sharper form is an in-flight
     part with no `part:` record yet (`0016:827`).
  2. **Restore's report cannot tell staged skips apart.** Once #662 lands, restore skips staged
     fragments silently, through the shared gate (`restore.rs:383`). 0016 requires the report to
     say so: `staged_skipped` and `sessions_fenced` beside `pending_skipped` (`0016:823`;
     `RestoreReport`, `restore.rs:107-169`).
  3. **Restore fences no session.** A restored image can resurrect an `Open` or `Completing`
     session whose bytes are gone, and nothing stops it from completing over them (D-B,
     `0016:717-728`, F13). A `Completing` session that had already written segments needs its
     `seg:` records retired in the **same** batch as its fence, or they have no deleter anywhere
     in the design (X57, `0016:880`). Today the session record cannot even name them
     (`PublishTarget`, `multipart.rs:1842-1849`).
  4. **No durable record tells a gateway the fence has run.** 0016 requires the restore-fence
     generation to complete before any gateway serves multipart verbs on the restored image
     (`0016:723-728`, `:3017-3021`, X17b).
- **Success criterion:** the NEW file `crates/custodian/tests/staged_drain_restore.rs` passes
  over in-memory doubles, with records seeded as raw JSON the base decoders accept (the shapes in
  `crates/core/tests/multipart_session_records.rs:81-145`). A `Completing` fixture carries the
  new nonce field. Base decoding rejects it (`#[serde(deny_unknown_fields)]`), which is harmless
  there: base restore never reads `mpu:`. Legs:
  **(A) Drain counts an in-flight part as held.** Server `S` holds **only** an owned `sidx:`
  fragment, and `desired:dserver:<S>` is set: `reconciliation_status(S)` is `Pending`. On the
  base it is `Satisfied` — the red.
  **(B) Drain counts a committed part as held**, as its own case: `S` holds only a committed
  `part:` fragment, and the answer is `Pending`. An implementation counting only one class passes
  one of A and B and fails the other (`0016:883`).
  **(C) Drain still finishes when the uploads live elsewhere.** Staged fragments sit on servers
  0–2, and server 3 is draining and holds none of them and no committed reference:
  `reconciliation_status(3)` is `Satisfied`. v1's `*server != dserver` mutant survived every
  leg; this case kills it. It is green on the base as well — a guard.
  **(D) Rebalance and drain agree, and rebalance leaves staged bytes alone (`0016:881`).** For
  a draining server holding **only** staged fragments, a rebalance pass writes no fragment
  anywhere and rewrites no `part:` record, **and** `reconciliation_status` is `Pending`. The
  red comes from the `Pending` half. State in `build-notes.md` which `Reconciled` the pass
  returns there, and why it does not tell an operator the drain is done.
  **(E) Restore reports staged skips.** `reconcile_after_restore` over a store with two staged
  fragments reports them as staged-skipped, separately from `pending_skipped`. The test cannot
  name a field this slice adds, or it would not compile on the base, so assert it through the
  report's `Debug` rendering. The rendering must contain `staged_skipped: 2` (0016's name for
  the counter, `0016:823`); the base rendering has no such counter.
  **(F) Restore fences a resurrected `Open` session (D-B).** An `Open@E` session in the store
  ends as `Aborting@E+1`. In the same batch — assert atomicity with a double that fails that one
  commit, after which **none** of the writes are present — its byte-retirement obligation is
  installed. Round-trip every obligation the fence writes through `decode_retire_obligation`
  (`crates/core/src/multipart.rs:3333`) against the key it sits under, and assert it decodes.
  The fenced-session counter moves: the `Debug` rendering contains `sessions_fenced: 1` (0016's
  name, `0016:823`). A Complete retried against
  that session cannot fence it, since the Complete fence requires `Open@E` (`0016:660`). The
  client-visible `4xx` is #658's to answer.
  **(G) Restore fences a resurrected `Completing` session with its segments' deleter (X57).** A
  `Completing@E` session with `segments_written > 0`, its nonce on the record, and
  `seg:<nonce>:<E>:*` records present ends as `Aborting@E+1`. One batch installs
  `retire:bytes` naming the session and its parts, **and** `retire:records` naming exactly
  `seg:<nonce>:<E>` (`0016:665`, the "one shape for all three doors" row). Both decode through
  `decode_retire_obligation`, and the records obligation's `segments()` names that group. v1
  installed only the first and reported the records as residue. That draining empties the range
  is #659's drain to prove (it stays on #665).
  **(H) What cannot be fenced cleanly is never passed off as done.** Two cases.
  (i) A `Completing` record with **no** nonce — the pre-decision shape — fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds — a part record missing from the restored image — is still fenced, and still named as
  needing a human. v1 silently built the teardown from whatever `part:` keys were present
  (`results/issue_637/iteration-v1/review-batch.md`).
  In both cases `RestoreReport::needs_human()` is true (`restore.rs:197`), and the fence
  generation (leg I) is **not** marked complete.
  **(I) The restore-fence generation record.** Three arms, each on durable state: (i) before any
  post-restore pass, the record is absent; (ii) **during** a pass, read through a double hook at
  the first fence commit, it names the pass's generation and reads not-complete; (iii) after the
  pass, it reads complete for that generation, and only if leg H found nothing. A second pass
  **advances** the generation and reads not-complete until it finishes, so a later restore
  invalidates an earlier completion instead of being masked by it. "Complete" becomes
  observable only after every write the pass makes, the mark batches included. How a gateway
  acts on the record is #508's: document the record's key and shape for it, and keep one source
  of truth.
  **(J) The session record carries the nonce, and nothing else changes for it.** A `Completing`
  session record with the nonce round-trips byte-identically through its codec, and one without
  it is refused. Put this in the codec's own test module in `crates/core/src/multipart.rs`; it is
  green-only by nature, which is fine for a codec leg.
  **(K) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on this bundle's base — `origin/main` + #661 +
  #662. When the waves run in one flow that fold is exported as `$PDCA_VERIFY_BASE`, which
  `engine/scripts/run-verify.sh` honours (`:247-265`); once both have merged, `main` itself is
  that base. Legs A, B, D, E, F, G, H and I fail by **assertion** there: the base's drain
  status counts committed placements only, and its restore reads no `mpu:` record and writes no
  fence or generation. C is a guard, and J is green-only. The new test may name only base-visible
  symbols: `wyrd_custodian::{reconcile_after_restore, reconciliation_status, set_lifecycle,
  reconcile_step, RebalanceContext, GcContext, RestoreReport, ReconciliationStatus,
  DServerLifecycle}`; `wyrd_core::multipart::{mpu_key, part_key, sidx_key, retire_key,
  decode_retire_obligation, decode_session_record, RetireMode, RetireToken, ...}`;
  `wyrd_core::metadata::seg_key`; `wyrd_traits`. It names no field or type this slice adds —
  hence the `Debug` assertions in E and F, and raw keys for the generation record. A compile
  failure on the RED leg reports UNVERIFIABLE (`run-verify.sh:521-547`). Record in
  `build-notes.md` how many tests ran red, all by assertion.
- **Invariant to restore:** no answer the custodian gives about a server or a restored image
  claims more than is true. A drain is `Satisfied` only when no byte that can still become
  referenced — committed, committed-part or in-flight — lives on that server. A restored image
  is declared fenced only when every session it resurrected can no longer publish, and every
  record that session wrote has a named deleter. Source: 0016 decision 2's drain, rebalance and
  restore rows (`0016:823`, `:826-827`), D-B and decision 1.4 (`:717-728`), X57 (`:880`,
  `:2587`); the C-1 rule that a certification over an incomplete picture is a defect
  (`docs/principles.md` §5); ADR-0045. SELF-TEST: fencing `Open` sessions alone passes F and
  leaves G's segment records with no deleter. Counting only `part:` in the drain passes B and
  fails A.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-2
- **Ordering note:** wave 3, beside child-3; their files are disjoint. **Outside the
  proposal**, added after acceptance: this slice edits `crates/core/src/multipart.rs`, which
  **#693** (ITERATE_DO) also edits. The human decides at acceptance whether that becomes a
  `Conflicts with` or a `Depends on`. #658 (Complete) must write the nonce this slice adds when it
  fences a session into `Completing`, and #656 (Abort) should reuse the fence batch this slice
  builds rather than write a second one. Both are notes for those issues, not work here.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **Scope:** four things. (1) Drain status counting both staged classes as held. (2) Rebalance
  confirmed disjoint from the staged set: a code change only if it currently moves or rewrites
  staged records. (3) Restore's staged accounting and its session fence, both shapes, with the
  obligations 0016's rows name in one batch each. Any writer-side construction added for those
  obligations must produce only values `decode_retire_obligation` accepts against their key,
  since the module withholds a writer on purpose (`multipart.rs:3036-3060`). (4) The durable
  restore-fence generation record, the nonce on the `Completing` session record (option (i)),
  and the post-restore command's report of fenced and unfenceable sessions
  (`crates/server/src/cli.rs`, whose tests build `RestoreReport` literals at `:2885-2990`).
  `RestoreReport` gains its fields plainly, with no `#[non_exhaustive]` (decided at Plan: it
  derives `Default` and is only built inside the workspace). Record the nonce decision in the new
  field's doc comment, citing the `0016:354` / `:2333` disagreement it settles. Docs currency
  (`AGENTS.md:154-157`: new persisted fields and a new persisted record): describe the fence,
  the generation record and the nonce in `docs/design/architecture/06-runtime-view.md`, and the
  post-restore exit reasons in `docs/design/architecture/m4-first-deployment-blueprint.md`.
  Must NOT change the signatures of `reconcile_after_restore`, `reconciliation_status` or
  `reconcile_step`, and must NOT add a field to any context struct. / out of scope: scrub and
  reconstruction (child-3 — do not touch `scrub.rs` or `reconstruction.rs`); the staged set
  itself and the mark codec (child-2); the retire drain that empties the obligations (#659); the
  gateway's reading of the generation record (#508); evacuating committed segmented objects
  (#653/#722); client Abort and Complete (#656, #658); any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an owned `sidx:` entry whose fragment sits on
  server `S`, set `desired:dserver:<S>` with `set_lifecycle`, and call
  `reconciliation_status(S)`: it answers `Satisfied`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_drain_restore.rs` — a **NEW** file. The C4-verify
  gate earns its red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`).
  The existing dev-dependencies suffice; make no `Cargo.toml` change.
- **Production reach:** the passes under test are the production `reconciliation_status`, the
  rebalance loop and `reconcile_after_restore`. Every session is seeded by the test, because no
  client can create one until #508. The generation record has no reader until #508's gateway
  gate. Until then the guarantee rests on the deployment ordering 0016 also allows: run the
  post-restore pass before re-enabling gateways (`0016:3017-3021`). Declared so sign-off weighs
  it rather than discovering it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/desired_state.rs:181-247` — `reconciliation_status` and
    `genuinely_holds`, to test the union with the staged set.
  * `crates/custodian/src/restore.rs:280-460` — the pass, its gate (`:383`), its `pending_skipped`
    accounting (`:420-424`) and its bounded `MARK_BATCH` commit (`:103`).
  * `crates/core/src/multipart.rs:1842-1849` (`PublishTarget`), `:1886-1917` (`SessionState`),
    `:3015-3032` (the retire rows: `{session}` for the abort fence, `{session, parts}` plus
    `{seg}` for the `Completing` fence), `:2965-2979` (the `seg` component).
  * `crates/core/src/metadata.rs:763-830` — `SegmentNonce` and `SegmentGroup`.
  * `crates/server/src/cli.rs:1300-1330` and `:2880-2990` — the post-restore verdict and its
    report tests.
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/desired_state.rs`,
  `restore.rs`, `crates/core/src/multipart.rs`) across merged history and open PRs: no merged
  change fences sessions or adds a fence-generation record, and no open PR touches these paths.
  Rejected prior art: #637 v1 (`results/issue_637/iteration-v1/`) fenced `Completing` sessions
  without the `seg:` deleter, because the nonce was missing. That is the design call settled
  above, and legs G and H exist for its review findings.
- **Disposition hint:** likely-fix
<!-- pdca:end child-4 -->
