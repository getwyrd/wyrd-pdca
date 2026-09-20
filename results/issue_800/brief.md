# Brief — issue 800 / gc-fragment-less-mark-sweep

> Split from #661 at its re-plan (2026-09-13); a child slice of #637 (637.1b). Do reads ONLY this
> file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `605b33a` (verified 2026-09-13). This bundle's base is `origin/main` **plus #661 and #662**, so
> lines in `crates/custodian/src/gc.rs` will have moved: re-locate them by symbol on the base.
> Background: proposal 0016 `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1339-1408`
> (the deadline rules, the sweep, the paged walk) and X96 at `:2625`.
>
> **Why this is its own slice.** #661 built this sweep together with the paged walk, twice. The
> walk converged. The sweep, and the conditional-write scheme round 2 built for it, drew the
> blocking findings #661 no longer carries. Round 1: a blind sweep delete lost a concurrent
> refresh, and the deadline assumed writers that enforce it
> (`results/issue_661/iteration-v1/`). Round 2: a lost precondition was read as proof that a
> mark exists, audit outcomes were dropped after a partial failure, and two differently spelled
> keys that decode to the same position let one sweep the other
> (`results/issue_661/iteration-v2/`). Legs D and E exist for those findings, and the ordering
> note for the deadline one.

- **Slug:** gc-fragment-less-mark-sweep
- **Kind:** enhancement
- **Defect:** GC consumes an `orphan:` mark only while iterating a `list_fragments()` result
  (`crates/custodian/src/gc.rs:183-219`), so **a mark whose position holds no fragment is never
  visited and never deleted** (`0016:1359-1368`). `main` already writes such marks: every repair
  of a missing fragment marks that fragment's old position (`crates/custodian/src/reconstruction.rs:865-884`,
  committed at `:942-947`), where by definition no fragment is stored. 0016 adds producers by design:
  teardown marks a failed attempt's full planned placement, and a repoint pre-mark precedes its
  write. After #661 the paged walk survives the ledger's size, but each lap re-reads marks that
  nothing will ever remove, so a lap grows without bound.
- **Success criterion:** the NEW file `crates/custodian/tests/gc_mark_sweep.rs` passes. It runs
  over in-memory doubles built the way #661's `crates/custodian/tests/gc_ledger_walk.rs` builds
  them: a metadata double with a lowered `scan` cap and a `scan_page` of its own, and D-server
  doubles whose `list_fragments()` the test controls. Every pass builds a fresh `GcContext`, as the
  deployed loop does (`crates/server/src/custodian.rs:600-608`). Let `D` be the late-write
  deadline `W_repoint + W_write + δ_clock` (`0016:1381-1391`). It is a named constant whose parts
  are named constants, each with its derivation in a doc comment. No constant for `W_write` or
  `W_repoint` exists on `main` (since #638 the D server enforces a deadline the caller supplies,
  `crates/traits/src/lib.rs:808-815`), so this slice names them. The test **hard-codes `D`**.
  Legs:
  **(A) A fragment-less mark is swept once that is safe.** A mark at a position that no listed
  server reports, aged exactly `D`, where this pass's listing was taken at or after
  `orphaned_at + D`, is deleted, and the delete is audited on the durability seam and counted,
  as a reclaim is (`gc.rs:542-553`). On the base (`main` + #661 + #662) the mark survives every
  pass — the red.
  **(B) Only then.** Each case survives or is left alone: (i) the same mark aged `D − 1` ms;
  (ii) a mark whose D server is not in the pass's fleet; (iii) a mark whose position **is**
  listed — the reclaim path owns it, not the sweep; (iv) **a listing from an earlier pass never
  licenses a sweep** (X96, `0016:2625`): run a pass while the mark is aged `D − 1` ms, write a
  fragment into that position, then run the next pass with the mark aged at least `D` and still
  inside the grace window — it survives, because only this pass's own listing may show the
  position empty; (v) a fleet that names one server twice never makes a listed position look
  unlisted; (vi) no sweep while the reference set is incomplete (the pass answers `Blocked`,
  `gc.rs:234-241`), and none at a referenced position — the sweep answers to the same gate as a
  reclaim (`gc.rs:191`); (vii) a mark in any of #662's three value shapes is swept on its stamp,
  while a value that decodes as none of them is never swept, is left byte-identical, and is
  surfaced (ADR-0045 decision 3).
  **(C) `D` stays strictly inside the deployed grace.** `D <` the grace window the deployed pass
  uses (`GC_GRACE_WINDOW_MILLIS`, `crates/server/src/custodian.rs:114`, which is
  `LEASE_TTL_MILLIS = 60_000`, `crates/server/src/cli.rs:78`), proved against **that constant
  itself**, not a copy of its value, because `0016:1386-1388` requires `G_orphan > D` strictly.
  The proof must **not** be a new `*/tests/*.rs` file. A second added test file would join
  C4-verify's invocation, and on the base it cannot compile (`D` does not exist there), which
  turns the whole RED leg UNVERIFIABLE. A compile-time assertion beside the deployed constant, or
  a case in an existing server test file, both work. Say which in `build-notes.md`.
  **(D) A sweep never deletes a mark newer than the one it judged, and its accounting matches
  what landed.** (i) A mark re-stamped after the pass read it and before its delete survives,
  and is neither audited nor counted as swept (round 1's finding). (ii) When a commit fails
  partway through a pass's sweep writes, every delete that landed before the failure is still
  audited and counted, and none that did not land is. Evidence is claimed only once it is
  durable, as restore already does (`crates/custodian/src/restore.rs:340-346`). (iii) After a lost
  precondition, whatever the pass concludes about that mark rests on a fresh read of it, never on
  the `Conflict` alone. A `Conflict` says only that a precondition lost
  (`crates/traits/src/lib.rs:1461-1465`), and the key may since be absent. Drive it with a racing
  writer that rewrites and then deletes the mark between the pass's read and its delete: the pass
  claims neither a sweep nor a protecting mark.
  **(E) A differently spelled key never costs a mark.** `parse_orphan_key` reads each field as a
  plain integer (`crates/core/src/metadata.rs:78-85`), so `orphan:5:01:0` and `orphan:5:1:0`
  decode to the same position, while every writer spells keys through `orphan_key`
  (`metadata.rs:72-74`). The sweep never deletes a mark at its own key on the strength of a
  differently spelled key's stamp or of its listing flag, and it never deletes or rewrites the
  differently spelled key, which it surfaces instead. Two cases: the position is listed and the
  alias is old; the position is unlisted, the alias is aged past `D` and the mark is younger
  than `D`. In both, the mark survives (round 2's two findings). This is the rule #661 sets for
  reclaims (its leg E), applied to the sweep.
  **(F) Seeded DST**, appended to the **existing** `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53` — keep it; add no new DST file). While GC walks a paged ledger over
  the simulated-TiKV store, with a page cap the seed picks, a concurrent task re-stamps a sweep
  target at a seed-chosen instant. The refreshed mark always survives. A coverage leg proves that
  some landing point falls between the pass's read and its delete, and some does not, as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2132-2165`). It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1616`). Record the seed count in
  `build-notes.md`.
  **(G) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on this bundle's own base — `origin/main` + #661
  + #662. When the waves run in one flow the driver exports that fold as `$PDCA_VERIFY_BASE`,
  which `engine/scripts/run-verify.sh` honours ahead of the brief's target (`:263-269`); once
  both have merged, `main` itself is that base. No container is needed. Leg A fails by
  **assertion** there: neither #661 nor #662 sweeps (both briefs put the sweep out of scope), so
  a fragment-less mark survives every pass. Legs B, D and E guard against over-deletion and may
  already be green on the base. That is expected, but each rule's own branch must be reached in
  the green leg (the diff-coverage gate reports it). C is a build-time or server-side proof, and
  F is a modified file; neither joins C4-verify's invocation, and C4-ci runs both. #661's and
  #662's additions have no names yet at Plan, so the test may name only symbols present on
  `origin/main` today: `wyrd_custodian::{reconcile_step, mark_orphaned, GcContext,
  ExpiredPendingPolicy, Custodian, FencedZone, Reconciled, ReconcileError}`
  (`crates/custodian/src/lib.rs:38-43`), `wyrd_core::metadata::{orphan_key, parse_orphan_key,
  ORPHAN_PREFIX}` (`crates/core/src/metadata.rs:62-85`), and the `wyrd_traits` store types. It
  writes #662's structured mark values as raw JSON bytes, as #662's own test does. `D` appears
  as a literal. If the file fails to compile on the RED leg it reports UNVERIFIABLE, not red
  (`run-verify.sh:521-541`). Record in `build-notes.md` how many tests ran red, all by assertion.
- **Invariant to restore:** C-1 — no permanent or data-losing failure mode is an acceptable
  cost: every durable byte is, at every instant, protected by a record that names it **or**
  evidenced for reclamation, and every state has an actor that exits it in bounded time. For
  this slice: every `orphan:` mark has a deleter whether or not its position ever receives a
  fragment (`0016:1406-1408`), and no mark is deleted while a fragment may still land under it.
  Deletion is conditioned on an **observation** of absence made after the late-write deadline,
  never on the mark's age alone (`0016:1369-1379`, X96 `:2625`), and that deadline sits strictly
  inside the grace window (`0016:1386-1388`), so "no fragment is written after its evidence may
  have been reclaimed" (`0016:1356-1358`). Evidence of a delete is claimed only once the delete
  is durable. Sources: `docs/principles.md` §5 C-1 and the §6 storage-lifecycle row (maintainer's
  rule 2026-07-25; `0016:2802-2813`; `crates/custodian/src/gc.rs:22-25`); ADR-0045 decision 3
  (`docs/design/adr/0045-metadata-validation-boundaries.md`). SELF-TEST: a sweep that checks only
  the mark's age passes leg A and fails B(iv). A deadline not tied to the deployed grace fails
  leg C. And a change to GC alone cannot make `D` sound: every later writer of a mark ahead of
  its fragment must enforce `W_write` and `W_repoint` (`0016:1339-1349`, `:1551-1576`). That is
  why `D`'s doc comment names the obligation, and why the ordering note carries it to the
  writers' slices.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 804
- **Conflicts with:** 813, 814, 722
- **Ordering note:** **Re-pointed 2026-09-15:** #662 was split at its re-plan into #803 (the
  staged reference set) and #804 (reclaim intent before deletion, and the `orphan:` value
  shapes). Everything this slice takes from #662 — the value shapes, the reclaim-intent commit
  and its lost-precondition handling — is #804's, so `Depends on` names #804 in place of `662`.
  Read "#662" below as #804. Builds on #804, which builds on #661. The sweep deletes marks inside #661's
  paged walk, decodes them through #662's value shapes, and mirrors #662's handling of a lost
  precondition on a mark. Conflicts with **#663** and **#722**: each appends a DST property to
  `crates/dst/tests/custodian.rs`. #625 and #633 also edit `gc.rs` and that file, but they sit
  later through their own dependencies. **Writers' obligation:** no writer on `main` writes a
  mark before its fragment lands, so the sweep is sound against today's tree. Unlink marks the
  placed positions of a committed map (`crates/core/src/metadata.rs:1881-1898`), and a map
  commits only after all of its fragments are acknowledged (`crates/core/src/write.rs:227-232`).
  Reconstruction and rebalance write first and mark only the positions they vacate, in the
  repoint commit (`reconstruction.rs:931-947`, `rebalance.rs:530-547`). The staged re-place (#663,
  its legs D(iv) and D(v)), flat pre-marking (#723, not yet briefed) and multipart teardown each
  write such marks, and each must enforce `W_write` and `W_repoint` before it does. #723's brief
  should name this constant. **Intake-cap override (wyrd-pdca-P1):** granted by the human
  (Eduard Ralph) in #661's re-plan session, 2026-09-13, for **#800 only**. The count at the
  override was `planned 23/6 (cap) — room for 0`. #661's repaired brief (exempt) made it
  `planned 24/6 (cap) — room for 0, need 1: Plan intake closed`, and this brief makes it 25. **Re-pointed 2026-09-19:** #663 was split at its re-plan into #813 (scrub checks committed staged fragments; reconstruction keeps their repair queued) and #814 (reconstruction rebuilds a staged chunk); the field above names both in place of `663`.
- **Surfaces:** data
- **Difficulty:** medium
- **Do model:** opus-max
- **Scope:** GC's sweep of fragment-less marks: the deadline `D` and its named parts; the rule
  that an absence counts only when this pass observed it after `D`; the same reference-set gate
  a reclaim answers to; decoding through #662's value shapes, failing closed on a value that
  matches none of them; the differently spelled key rule; the sweep's own writes, in batches no
  larger than #661's write bound, with a delete that loses to a newer mark and accounting that
  matches what landed; the proof that `D` is inside the deployed grace; and the DST property.
  If the sweep changes what `docs/design/architecture/06-runtime-view.md` §6.7 step 2 (`:74`) says
  GC does, update that line. Must NOT change `reconcile_step`'s signature, and must NOT add a
  field to `GcContext` or any other context (the test builds them with struct literals). Keep
  #661's walk rules and #662's reclaim ordering exactly as they judge: consume them, do not
  reshape them. / out of scope: the paged walk and its budget (#661); the staged reference set,
  reclamation intent and the value codec itself (#662); deadline enforcement in any writer —
  the staged re-place (#663), flat pre-marking (#723) and multipart teardown — beyond naming the
  obligation next to `D`; tightening `D` per event kind (`0016:1388-1390`; the uniform bound is
  the safe default); restore; `scrub.rs`, `reconstruction.rs`, `rebalance.rs`, `desired_state.rs`;
  any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, `mark_orphaned` a position on a D server that holds no
  fragment there, advance the clock past any deadline and past the grace window, and run GC
  passes: the mark is never deleted, because GC visits a mark only through a `list_fragments()`
  result (`gc.rs:183-219`).
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/gc_mark_sweep.rs` — a **NEW** file. Confirmed with
  `engine/scripts/run-verify.sh --classify` on a synthetic patch of this slice's expected files
  (`gc.rs`, `crates/server/src/custodian.rs`, the new file, `crates/dst/tests/custodian.rs`, the
  doc). Only the new file is `ADDED_TEST`, so C4-verify runs
  `cargo test -p wyrd-custodian --test gc_mark_sweep` with no cfg to set. The DST file is modified,
  so C4-ci covers it. The existing dev-dependencies suffice; make **no** `Cargo.toml` change (it
  is reverted on the RED leg).
- **Production reach:** the pass under test is the production `reconcile_step`, over in-memory
  stores. The deployed loop reaches the sweep on every GC pass
  (`crates/server/src/custodian.rs:600-611`). The fragment-less marks are seeded by the test;
  in production they come from repairs of missing fragments today, and from teardown and
  repoint pre-marks once those slices land.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror (locate them by symbol on the base):
  * #661's paged walk in `crates/custodian/src/gc.rs` — where a window's marks are judged; the
    sweep joins that judgement.
  * #662's reclamation-intent commit in `gc.rs`, and its handling of a lost precondition (#662's
    leg F(ii)) — the shape for a delete that loses to a newer mark.
  * `crates/custodian/src/gc.rs:191` (the reference-set gate) and `:542-553` (`emit_reclaim`, the
    audit shape a sweep mirrors).
  * `crates/custodian/src/restore.rs:340-346` — evidence claimed only once it is durable.
  * `crates/server/src/custodian.rs:87-114` — `GC_GRACE_WINDOW_MILLIS` and its derivation.
  * `crates/dst/tests/custodian.rs:883-1000` (the GC Q3 property) and `:2111-2165` (the campaign
    and coverage-leg shape).
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/gc.rs`) across merged
  history, open PRs and closed PRs: no change has ever deleted a mark outside the reclaim loop,
  and GC has consumed marks only through `list_fragments()` since it landed (`af4ab65`, PR #188).
  No open PR touches `gc.rs`. Rejected prior art: #661 v1 (`results/issue_661/iteration-v1/`)
  swept with a blind delete, so a concurrent refresh lost its evidence; it assumed writes land
  within the deadline while live writers pass no deadline; and it had no seeded DST. #661 v2
  (`results/issue_661/iteration-v2/`) added conditional deletes with a per-write retry. It read a
  lost precondition as proof that a mark exists, dropped the audit of deletes that had landed
  when a later retry failed, and kept listing flags per raw key, so a differently spelled key
  could sweep the real mark. Legs D and E exist for these.
- **Disposition hint:** likely-fix
