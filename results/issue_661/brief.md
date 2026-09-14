# Brief — issue 661 / gc-orphan-ledger-paged-walk

> Child 1 of #637's split (637.1), **re-planned 2026-09-13** after two build rounds. Do reads
> ONLY this file. Keep the `- **Label:** value` lines. Every `path:line` below is in the target,
> getwyrd/wyrd at `origin/main` @ `605b33a`, re-verified 2026-09-13 (none of the cited files moved
> since `3969a3a`), **except two paths that live in this harness repo** (wyrd-pdca @ `b15e0d8`,
> not the target): `docs/principles.md`, the invariant catalogue, and
> `engine/scripts/run-verify.sh`, the script the C4-verify gate runs. Background: proposal 0016
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1392-1408`, the paged-walk rule
> this slice implements.
>
> **What changed at the re-plan.** Round 2 got the paging right (legs A, B, C and F green, no
> surviving mutants). All four blocking review findings it left open came from the sweep of
> marks with no fragment, and from the conditional-write-and-retry scheme round 2 added for the
> sweep and reused in restore. This brief is the paging walk alone. The sweep, its late-write
> deadline and its deletes that must lose to a newer mark moved to **#800**, filed 2026-09-13.
> Two records back that narrowing. The human's sign-off on round 2 directed it
> (`results/issue_661/iteration-v2/SUMMARY.md` §9, "Iteration delta": split the paging from the
> sweep). And #800's tracker body states it: "Split from #661 at its re-plan (2026-09-13) … #661
> now carries the paged walk only, and this issue carries the sweep." #661's tracker body
> predates the split and still lists the sweep; a comment on #661 (2026-09-13,
> `issuecomment-5655549346`) records the re-scope there.
> Legs D, E and G answer findings from rounds 1 and 2 before review can raise them again.

- **Slug:** gc-orphan-ledger-paged-walk
- **Kind:** enhancement
- **Defect:** the `orphan:` ledger is read with **one `scan`**, in two places: GC
  (`orphan_leases`, `crates/custodian/src/gc.rs:522-537`, called at `:177`) and the post-restore
  pass (`crates/custodian/src/restore.rs:308`). `scan` fails whole past
  `SCAN_CAP = 1 << 20` (`crates/traits/src/lib.rs:286`) and returns no partial result
  (`:273-278`). One maximum segmented-object retirement installs ~1.78 M marks
  (`0016:1392-1398`; `crates/traits/src/lib.rs:1362-1368`), so a single large delete takes GC down
  on every pass from then on. The deployed loop runs GC in its own `reconcile_step` call
  (`crates/server/src/custodian.rs:587-611`), so GC stops, and the post-restore command can never
  finish. The failure seals itself: the pass that should shrink the ledger is the pass that
  cannot start. Second, a GC pass commits all of its key deletes in **one batch sized by the
  pass** (`gc.rs:180`, `:216`, `:231`), so the commit grows with the ledger. A pass that reclaims
  many marks hands the backend a transaction past its envelope (10 MB and 5 s per transaction,
  `crates/traits/src/lib.rs:1326-1331`) after it has already deleted the fragments, so their
  marks stay behind. This slice caps that commit at a constant `W`. It does **not** prove that a
  `W`-write commit fits the 5-second half on the slowest backend. That is a calibration, and leg
  B states what its test shows and what it does not.
- **Success criterion:** the NEW file `crates/custodian/tests/gc_ledger_walk.rs` passes. It runs
  over in-memory doubles. The metadata double's `scan` enforces a **lowered** cap: it returns
  `wyrd_traits::ScanCapExceeded` when the result would exceed the cap, shaped after
  `crates/metadata-redb/tests/scan.rs:9-19`. Its `scan_page` is implemented directly over its
  map with the shared page helpers, **not** through `wyrd_testkit::test_double_scan_page`, which
  pages over `scan` and inherits the cap (`crates/traits/src/lib.rs:1431-1433`). Its pages are
  capped below `B` (below), as a real backend's are, so one pass needs several pages to read
  its budget. The double counts the `orphan:` entries each pass receives, records any `scan`
  that reaches the `orphan:` prefix, and logs every commit. Every pass builds a fresh
  `GcContext`, exactly as the deployed loop does (`crates/server/src/custodian.rs:600-608`). Each
  test asserts on the pass's `Result` before anything else. Legs:
  **(A) GC survives a ledger past the cap.** Seed an `orphan:` population larger than the lowered
  cap, every mark actionable: fragment present, unreferenced, past grace. Every pass of
  `reconcile_step` with a `GcContext` returns `Ok`. Running passes until the ledger is empty
  reclaims every fragment and removes every consumed key. On the base the first pass returns
  `Err(ReconcileError::Store)` (the `?` at `gc.rs:177`) — the red.
  **(B) One pass reads and writes a bounded, pinned amount.** No pass receives more than `B`
  `orphan:` entries, counted across all of its pages. `B` is a named constant with its derivation
  in its doc comment, at most 65,536 (1/16 of `SCAN_CAP`). The test **hard-codes the same
  literal**. While at least `B` entries remain, a pass receives **exactly** `B`, so a budget that
  silently under-reads fails. A population `P > B` drains in exactly `⌈P / B⌉` passes, not fewer.
  No `scan` of the `orphan:` prefix happens anywhere in the step (`0016:1398-1399`: `orphan:` is
  "never read by a single scan"). The walk's own writes are bounded too. No commit it makes
  carries more than `W` writes. `W` is a named constant, at most restore's `MARK_BATCH` of 1,000
  (`crates/custodian/src/restore.rs:96-103`, the base's existing bounded commit against the same
  envelope). The test hard-codes it and asserts that no commit exceeds `W`, and that the commits
  carrying a pass's `n` key deletes number exactly `⌈n / W⌉`. So a batch that is flushed late,
  or never, fails. Round 1 left four batch-control mutants alive for want of this. **What this
  proves, and what it does not.** It proves the commit size is a constant, independent of the
  ledger and of the pass. It does not prove that a `W`-write commit finishes inside 5 s. On TiKV
  each delete takes its own lock round trip, one after another inside the transaction
  (`crates/metadata-tikv/src/lib.rs:1407-1422`), so the time depends on the deployment's
  round-trip time, and 0016 says a batch of ~1,000 small `orphan:` marks can exceed it
  (`0016:630-636`). Fitting that half is 0016's calibrated `B_ops` knob (`0016:640-643`; X98 at
  `:2627`), out of scope here. `W`'s doc comment states its byte bound (the keys are small, so
  `W` of them sit far inside 10 MB) and says plainly that the operation-count half is not
  calibrated, citing `0016:640-643`.
  **(C) The tail does not starve, and the walk wraps.** Seed a **retention-safe head** — more
  than `B` marks still inside their grace window, placed first in key order — followed by an
  **actionable tail**, head plus tail above the lowered cap. Run passes with a fresh `GcContext`
  each time. Every tail fragment is reclaimed within `⌈(head + tail) / B⌉ + 1` passes, and no
  head fragment or head mark is touched. Then age the head past grace: every head fragment is
  reclaimed within `⌈(head + tail) / B⌉ + 1` further passes. So the walk returns to the start
  of the ledger once it reaches the end. An implementation that restarts at the first key every
  pass never reaches the tail, and one that stops at the end never reaches the head again. A
  key written behind the walk's position may be missed until the next lap, and that is
  allowed; a key present throughout the walk is never skipped (`scan_page` clause 4,
  `crates/traits/src/lib.rs:1390-1398`).
  **(D) An unread mark outranks every other reason to reclaim, and no pass strips the last
  evidence from bytes it keeps.** Under `ExpiredPendingPolicy::Reclaim`:
  (i) a fragment whose chunk carries an **expired** `pending:` lease (seeded with
  `metadata::put_pending`, as `crates/custodian/tests/gc.rs:202-216` does) and whose own
  `orphan:` mark is still inside its grace survives every pass until that mark's grace elapses.
  The ledger is several windows long (a window is the up-to-`B` entries one pass reads), so in
  most passes the mark sits **outside** the window being read — ahead of it in some passes and
  behind it in others. Round 1's patch reclaimed it on the lease (three blocking findings).
  (ii) Seed a chunk under an expired lease, with unmarked fragments on several servers whose
  positions fall in different windows. Fill the ledger past the lowered cap with marks that do
  not touch that chunk. Between any two passes, every fragment of that chunk still on disk has
  its `pending:` entry or an `orphan:` mark. The entry is deleted only once no fragment remains
  for it to account for, and every fragment is reclaimed within `⌈P / B⌉ + 1` passes.
  (iii) An unreadable mark counts as a mark. Set up as (i), but the fragment's own mark (as
  `orphan_key` spells it) holds a value that is not the decimal instant `mark_orphaned` writes.
  The fragment survives every pass through two full laps of the walk. The mark's key and value
  bytes are never deleted or changed. The mark is named at least once on the GC audit seam
  (target `wyrd.custodian.gc.audit`), carrying its key, the way `emit_unreadable_pending` names
  an unreadable `pending:` value (`gc.rs:600`). Read the audit lines back as
  `crates/custodian/tests/gc.rs:895-918` and `:1056-1063` do, with the #214 global-default guard
  (`:1143-1146`) installed first. Only this leg seeds an unreadable mark, so no other test in the
  binary fires that callsite. The base treats such a value as no mark at all (`gc.rs:527-533`),
  so its fragment falls to the expired-lease arm (`:204-206`) and the cleanup deletes the mark
  (`:216`). A paged walk that keeps that behaviour fails here.
  On the base all three cases error at the first pass (the ledger is past the cap) — the red.
  **(E) Only a fragment's own key licenses a reclaim.** `parse_orphan_key` reads each field as a
  plain integer (`crates/core/src/metadata.rs:78-85`), so `orphan:5:01:0` and `orphan:5:1:0`
  decode to the same position. Every writer spells the key through `orphan_key`
  (`metadata.rs:72-74`). Seed a fragment whose own mark (as `orphan_key` spells it) is inside
  grace, and a differently spelled key for the same position carrying an old stamp, placed in
  an earlier window (more than `B` keys before the mark; chunk ids between the two spellings,
  such as `orphan:5:10:0` onwards, sort between them). The fragment survives until its own
  mark's grace elapses. The differently spelled key is never deleted, rewritten or acted on. On
  an ordered backend the base's single scan happens to keep this fragment, because the key that
  sorts later wins in its map (`gc.rs:532`). A paged walk that judges an early window alone
  would not. On the base the leg errors past the cap — the red. #800 applies the same rule to
  its sweep.
  **(F) Restore survives the same ledger, and never re-stamps a mark it did not read.**
  `reconcile_after_restore` over a store whose `orphan:` population exceeds the lowered cap
  returns `Ok`. Restore writes a fresh stamp for any stranded fragment it did not find marked
  (`restore.rs:413-416`, the `put` at `:426-429`). So a restore that judged "already marked" from
  part of the ledger would silently restart the grace clock of every mark beyond it. Seed
  pre-marked stranded fragments whose marks sort **after** both the first `B` `orphan:` keys and
  the first lowered-cap's worth, one of them the ledger's last key, each stamped older than the
  pass's clock, and one more whose mark holds an unreadable value, as in D(iii). Every one keeps
  its value bytes unchanged and is counted in `RestoreReport::already_marked`, the field for a
  fragment that "already carried an `orphan:` record" (`restore.rs:112-114`). The base would
  re-stamp the unreadable one, since its read drops the value (`restore.rs:413-429`). A genuine
  stray with no mark **is** marked, so the leg cannot pass on a pass that did nothing. On the
  base `restore.rs:308` returns `Err` — the red.
  **(G) Seeded DST for the paged walk**, appended to the **existing** `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53` — keep that attribute, and add no new DST file). The rubric requires
  it: "a new destructive or concurrent path lands with seeded Tier-0 DST coverage", and round 1
  drew three findings for its absence. GC passes, each with a fresh `GcContext`, walk an
  `orphan:` ledger several **pages** long over the simulated-TiKV store, with a page cap the seed
  picks. The ledger stays below `B`: `B` is a production constant, and a test cannot lower it
  without a context field or a global, both ruled out below. A concurrent task unlinks further
  objects at a seed-chosen instant, writing fresh marks (inside grace) at positions both ahead
  of and behind the walk's current page. Afterwards: no fragment that is referenced, or whose
  own mark is inside grace, has been deleted, and every fragment actionable at the start has
  been reclaimed by the end of the run. A coverage leg proves the mid-walk landing is reached
  rather than assumed, as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2132-2165`). It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1616`). Record the seed count in
  `build-notes.md`.
  **(H) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on this bundle's base (`origin/main`). No
  container or cluster is needed. Legs A to F fail by **assertion** there. Each seeds an
  `orphan:` population past the double's lowered cap, and the base's single scan errors on the
  first pass (`gc.rs:177` via `:526`; `restore.rs:308`), so each test asserts on the pass's
  `Result` first. Legs B, D and E also bite on the paged design itself: a walk that under-reads,
  lets a lease outrank an unread mark, treats an unreadable mark as none, or acts on a
  differently spelled key fails them. That is where rounds 1 and 2 were caught. Leg G is a modified file, so it is outside C4-verify's
  invocation, and C4-ci runs it under `--cfg madsim`. The test file may name only symbols
  present on `origin/main`: `wyrd_custodian::{reconcile_step, reconcile_after_restore,
  mark_orphaned, GcContext, ExpiredPendingPolicy, Custodian, FencedZone, Reconciled,
  ReconcileError, RestoreReport}` (`crates/custodian/src/lib.rs:38-43`),
  `wyrd_core::metadata::{orphan_key, parse_orphan_key, put_pending, PendingEntry, ORPHAN_PREFIX}`
  (`crates/core/src/metadata.rs:62-85`, `:1594`, `:2092`),
  `wyrd_traits::{MetadataStore, ChunkStore, ScanPage, ScanCapExceeded, WriteBatch, CommitOutcome,
  page_limit, page_start, page_is_full, page_cursor}`, the `tracing` and `tracing-subscriber`
  crates for leg D(iii)'s audit read-back (already a dependency and a dev-dependency of
  `wyrd-custodian`), and nothing this slice adds. `B` and `W` appear in the test as literals, and
  D(iii) matches the audit line by text, not by a new symbol. If the file fails to compile with the production change
  reverted, the RED leg reports **UNVERIFIABLE**, not red (harness repo,
  `engine/scripts/run-verify.sh:521-541`),
  so a build error is a defect in the test. Record in `build-notes.md` how many tests ran red and
  that each failure was an assertion.
- **Invariant to restore:** C-1 — no permanent or data-losing failure mode is an acceptable
  cost: every durable byte is, at every instant, protected by a record that names it **or**
  evidenced for reclamation, and every state has an actor that exits it in bounded time. For
  this slice: the `orphan:` ledger is readable at every size, and a pass's footprint (the
  entries it holds, the writes in each commit) is bounded by a constant, not by the ledger. A
  pass that read part of the ledger draws only retention-safe conclusions from it. Nothing is
  destroyed, overwritten or stripped of its last evidence because a record was absent from a
  partial read, and only a record a writer could have written licenses a destructive step. A
  record that does not decode is classified, skipped and surfaced, never acted on. Sources:
  `docs/principles.md` (harness repo) §5 C-1 and the §6 storage-lifecycle row (maintainer's rule 2026-07-25;
  `0016:2802-2813`; `crates/custodian/src/gc.rs:22-25`); `0016:1392-1408` ("accessible at every
  cardinality"); ADR-0045 decision 3 (`docs/design/adr/0045-metadata-validation-boundaries.md`,
  applied to GC at `gc.rs:484-497`). SELF-TEST: guarding GC alone does not satisfy it. Restore
  reads the same ledger with the same scan (`restore.rs:308`), and leg F fails if only `gc.rs`
  changes. Replacing the read alone fails legs D and E, because the reclaim decision that
  consumes the read must change with it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 722
- **Ordering note:** wave 1 of #637's chain. There is no unmerged prerequisite: #634
  (`scan_page`, PR #645) and #638 (named in #661's tracker body) are merged. #662 builds on this
  slice. #800 builds on #662. It holds the sweep, split out of this slice at its re-plan on
  2026-09-13. **#625 comes after this slice**, as #661's tracker body requires: #625 widens
  `reconcile_step`'s signature (`crates/custodian/src/reconciliation.rs:103-112`), and all five
  #637 slices precede it. That order is already set on #625's side. Its brief's `Depends on:`
  names #663 and #664, which depend on #662, which depends on this slice, so #625 lands in a
  later wave. Listing #625 under `Conflicts with:` here would add nothing, and this slice leaves
  the signature alone (Scope). Conflicts with **#722**: both append a DST property to
  `crates/dst/tests/custodian.rs`. The re-plan narrowed this brief in place, which is exempt from
  the intake cap as a repair of an ITERATE_PLAN brief (INTEGRATION §11). The one new id, #800,
  went in over the cap by the human's explicit override, recorded in #800's brief.
- **Surfaces:** data
- **Difficulty:** medium
- **Do model:** opus-max
- **Scope:** GC's and restore's reads of the `orphan:` ledger, and GC's reclaim decision that
  consumes its read. (1) GC reads at most `B` entries per pass and resumes where the last pass
  stopped, across the per-pass rebuild of `GcContext` the deployed loop does
  (`crates/server/src/custodian.rs:600-608`), returning to the start at the end of the ledger.
  (2) What a pass may conclude from a partial read: legs D and E. That includes what an
  unreadable mark means, because leg D rewrites the expired-lease arm that decides it: a mark
  value that does not parse counts as a mark. Its fragment is kept, neither GC nor restore
  deletes or rewrites the mark, and GC names it on the audit seam, as ADR-0045 decision 3
  requires of a decode site (legs D(iii) and F). Today GC skips it silently, so its fragment
  looks unmarked and falls to an expired lease (`gc.rs:527-533`, `:204-206`, `:216`), and
  restore re-stamps it (`restore.rs:413-429`). (3) The walk's own writes commit in batches of at
  most `W`, never one batch sized by the pass. (4) Restore's "already marked" judgement sees
  every existing mark, with a footprint bounded by a constant. Keep the safety gate
  (`gc.rs:191`), the grace test (`:196-203`) and the conservative arm (`:207-211`) judging
  exactly as they do. The expired-lease arm between them (`:204-206`) is the one leg D changes:
  it may fire only where the fragment has no mark, read or unread. **Writes keep the base's shape.**
  This slice adds no conditional-write-and-retry scheme for `orphan:` marks. Round 2's scheme
  produced both of its findings outside the sweep, and it moves to #800 with the sweep. No
  conclusion about a mark may rest on a commit's `Conflict`, which says only that a
  precondition lost (`crates/traits/src/lib.rs:1461-1465`). Whether the walk also survives a
  leader change is Do's call, stated in `build-notes.md`. Must NOT change `reconcile_step`'s or
  `reconcile_after_restore`'s signature, and must NOT add a field to `GcContext` (`gc.rs:72-82`),
  to any other context, or to `RestoreReport`. The test builds them with struct literals, so
  either change breaks its base compile and turns every red into UNVERIFIABLE. Decoding is
  unchanged: marks remain the bare decimal `mark_orphaned` writes (`gc.rs:117-129`). Two existing
  tests assume restore's whole-ledger scan and may be adjusted, keeping their purpose.
  `crates/custodian/tests/segmented_map_restore.rs:642-656` poisons the `orphan:` scan to prove a
  record already known unreadable is named before a later read fails: keep its `pending:` leg
  and the property. The DST restore campaign's timing (`crates/dst/tests/custodian.rs:1784-1789`,
  coverage leg `:2132-2165`) assumes the pass's two `inode:` readings are three hops apart with
  the `orphan:` scan between them: retune the timing only, never the invariants it asserts.
  Docs currency (`AGENTS.md:154-157`): a persisted continuation is a new persisted record, so
  describe it and the paged walk in `docs/design/architecture/06-runtime-view.md` §6.7 step 2
  (`:74`). / out of scope: the sweep of marks with no fragment, the late-write deadline and its
  relation to the grace window, and deletes that must lose to a refreshed mark (#800);
  calibrating `W` against the 5-second half of the envelope on the slowest backend (0016's
  `B_ops`, `0016:640-643`); decoding or repairing an unreadable mark value (this slice only keeps
  and names it); the staged reference set, reclamation intent, the three `orphan:` value shapes and keyed retire
  protection (#662); the orphan-identity migration gate (X92, #659); pre-marking in
  reconstruction and rebalance (#723); the `pending:` and `inode:` scans (`gc.rs:503`,
  `referenced_fragments`), which are other namespaces; `scrub.rs`, `reconstruction.rs`,
  `rebalance.rs`, `desired_state.rs`, `crates/server/src/*`; any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed more keys under `orphan:` than a lowered `scan`
  cap allows, as `crates/metadata-redb/tests/scan.rs:9-19` lowers it, and run one GC pass
  (`reconcile_step` with a `GcContext`): `orphan_leases` (`gc.rs:526`) returns `ScanCapExceeded`
  and the pass errors. `reconcile_after_restore` errors the same way at `restore.rs:308`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/gc_ledger_walk.rs` — a **NEW** file under `tests/`.
  Confirmed with the harness repo's `engine/scripts/run-verify.sh --classify` (re-run
  2026-09-13 at `b15e0d8`) on a synthetic patch of this slice's
  expected files (`gc.rs`, `restore.rs`, the new file, `segmented_map_restore.rs`,
  `crates/dst/tests/custodian.rs`, the doc). Only the new file is `ADDED_TEST`, so C4-verify runs
  `cargo test -p wyrd-custodian --test gc_ledger_walk` (`run-verify.sh:404-409`), with no cfg to
  set. The two modified test files are reverted on the RED leg and are not in its invocation
  (`:510-517`); C4-ci covers them. A case appended to `crates/custodian/tests/gc.rs` would be a
  modified file and earn no red. `wyrd-custodian`'s dev-dependencies already cover the doubles
  (`async-trait`, `bytes`, `tokio`, `wyrd-testkit`, `wyrd-coordination-mem`). Make **no**
  `Cargo.toml` change: it is reverted on the RED leg.
- **Production reach:** the passes under test are the production `reconcile_step` and
  `reconcile_after_restore`. Only the store and the D-server fleet are in-memory, as
  `crates/custodian/tests/gc.rs` already does it, and the DST leg drives the same entry points
  over the simulated-TiKV model. The deployed loop reaches the walk on every GC pass
  (`crates/server/src/custodian.rs:600-611`).
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/gc.rs:142-247` — the reclaim pass to restructure. Keep the safety gate
    (`:191`), the grace test (`:196-203`) and the conservative arm (`:207-211`) exactly as they
    judge. The expired-lease arm (`:204-206`) is the one leg D changes.
  * `crates/custodian/src/gc.rs:522-537` — `orphan_leases`, the single scan to remove (restore
    reads it at `restore.rs:308`).
  * `crates/custodian/src/gc.rs:484-518` — `expired_pending_chunks`: classify, skip and emit for
    a record that does not decode (ADR-0045 decision 3); `emit_unreadable_pending` (`:600`) is
    the emit to mirror for an unreadable mark.
  * `crates/custodian/tests/gc.rs:895-918`, `:1056-1063`, `:1143-1146` — the audit `Capture`,
    its subscriber, and the #214 global-default guard: the read-back leg D(iii) mirrors.
  * `crates/custodian/src/restore.rs:96-103`, `:339-346`, `:426-450` — `MARK_BATCH` and the
    bounded commit loop that claims evidence only once it is durable: the pattern for the walk's
    own writes.
  * `crates/custodian/tests/gc.rs:52-135` — the `MemMeta` / `MemDServer` doubles to extend.
    Replace the `scan_page` delegation (`:77-87`) with a direct implementation in the new file.
    `:202-216` seeds an expired pending lease.
  * `crates/traits/src/lib.rs:334`, `:414-562` — `ScanPage` and the shared page helpers
    (`page_limit`, `page_start`, `page_is_full`, `page_cursor`); `:1377-1398` — the four
    `scan_page` clauses.
  * `crates/dst/tests/custodian.rs:883-1000` (the GC Q3 property to sit beside) and
    `:2111-2165` (the two-readings campaign and its coverage leg).
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/gc.rs`,
  `crates/custodian/src/restore.rs`, `crates/dst/tests/custodian.rs`) across merged history, open
  PRs and closed PRs. `orphan_leases` has been one `scan` since it landed in M3 (`af4ab65`,
  PR #188, 2026-06-21). No merged, open or closed PR pages it: `scan_page` itself landed in
  PR #645, and no open PR touches these files. Rejected prior art: #508's 7th attempt swapped the
  scan for an unbounded `loop { scan_page }` into one `HashMap`, which is why leg B pins the
  budget. #637 v1 paged it without pinning its bounds. #661 v1 (`results/issue_661/iteration-v1/`)
  had restore accumulate every page into one `HashMap` (T2 FAIL), let an unread mark fall
  through to the expired-lease arm (three findings; leg D), and shipped no seeded DST (three
  findings; leg G). #661 v2 (`results/issue_661/iteration-v2/`) got the paging right. The sweep
  and its conditional-write retry drew all four of its findings, and both moved to #800.
- **Disposition hint:** likely-fix

## Plan-review response (revision pass, 2026-09-13 — `plan-advisory-plan-reviewer.md`)

Five findings. Four revised the brief; F1 stands, with its authority now cited in the brief.
Claims re-checked against `origin/main` @ `605b33a`, the live tracker, and this harness repo @
`b15e0d8`. The reviewer's sandbox held only this brief and `notes.json`, so it could not see
#800's tracker body or the round-2 sign-off.

- **F1 — the sweep moved to #800 with no tracker record.** *Stands, now cited.* The human
  directed the narrowing at the round-2 sign-off (`iteration-v2/SUMMARY.md` §9), and #800's
  tracker body records it. The header quotes both. #661's tracker body predates the split; at
  the human's request, a comment on #661 now records the re-scope and points at #800.
- **F2 — the 5-second claim has no test.** *Revised, finding upheld.* The Defect and leg B now
  claim only what the count test proves: each commit is capped at a constant `W`. Calibrating
  `W` against the time half (0016's `B_ops`) is out of scope, and `W`'s doc comment must say so.
- **F3 — unreadable-mark handling is a separate fix.** *Revised; one point stands.* It stays in
  scope because leg D rewrites the expired-lease arm that decides what an unreadable mark means.
  If the brief says nothing, Do can keep the base's "no mark" reading and reclaim bytes whose mark
  exists. The finding is right that nothing tested it: leg D(iii) and a case in leg F now do.
  Decoding or repairing such values is out of scope.
- **F4 — the #625 ordering was dropped.** *Revised in the Ordering note; no field change.* #625
  is already ordered after this slice by its own `Depends on: 663, 664`, which reach this slice
  through #662. Adding #625 to `Conflicts with:` here would add nothing.
- **F5 — two cited paths are not on the target.** *Revised.* `docs/principles.md` and
  `engine/scripts/run-verify.sh` are harness-repo paths. The header and each citation now say
  so, with the harness commit, and their line ranges were re-checked there.
- **Also fixed in the verify pass.** The "keep unchanged" ranges for the grace test and the
  conservative arm (`gc.rs:196-205`, `:206-210`) took in the expired-lease arm, which leg D
  changes. They are now `:196-203` and `:207-211`, and the lease arm (`:204-206`) is named as
  the one leg D changes.

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild to close the C5 causal-adequacy gap: the surviving mutant on the expired-lease/within-grace boundary (gc.rs:745, gc.rs:795, a > vs >= flip) is not caught by any current test, and that boundary is safety-relevant (a wrong boundary authorizes a destructive reclaim). Add a regression whose own mark sits exactly at the persisted lower cursor boundary so the mutant is killed. Other §6 items (sweep moved to #800 without a tracker comment, dropped #625 ordering conflict, bad doc citations, transaction-envelope claim wording, malformed-record scope, rebalance/reconstruction W_repoint scope, size backstop) are tracker-authority/scope/process questions, not implementation defects, and are explicitly out of scope for this iterate-do. Oversize flag is waived per human instruction.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild to close the C5 causal-adequacy gap: the surviving mutant on the expired-lease/within-grace
  boundary (gc.rs:745, gc.rs:795, a > vs >= flip) is not caught by any current test, and that boundary
  is safety-relevant (a wrong boundary authorizes a destructive reclaim). Add a regression whose own
  mark sits exactly at the persisted lower cursor boundary so the mutant is killed.

  Other §6 items (sweep moved to #800 without a tracker comment, dropped #625 ordering conflict, bad
  doc citations, transaction-envelope claim wording, malformed-record scope, rebalance/reconstruction
  W_repoint scope, size backstop) are tracker-authority/scope/process questions, not implementation
  defects, and are explicitly out of scope for this iterate-do. Oversize flag is waived per human
  instruction.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 47 mutants tested in 89s: 2 missed, 26 caught, 18 unviable, 1 timeouts
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
