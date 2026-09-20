# custodian: GC records reclaim intent before deletion + orphan-mark value shapes (662.2)

> Child 2 of 2 of #662's split (637.2). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `78f9859` (verified 2026-09-15); #803 and then #664
> build first and move lines in `gc.rs` and `06-runtime-view.md` — re-locate by symbol or
> section. 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; background: `:1190-1338`.

- **Slug:** gc-reclaim-intent-and-mark-shapes
- **Kind:** enhancement
- **Defect:** three gaps in how GC reads and consumes `orphan:` marks.
  1. **Destroy first, record second.** GC calls `delete_fragment`
     (`crates/custodian/src/gc.rs:314`) before queueing the key delete (`:321`, committed
     `:346`), so an adoption CAS on a pre-mark's original bytes can land after the fragment is
     gone — a placement over deleted bytes (`0016:1293-1320`).
  2. **One shape decodes.** Only a bare decimal reads (`gc.rs:811-817`); 0016's structured and
     `reclaiming` shapes (`0016:1190-1211`, `:1321-1338`) read as unreadable and are kept
     forever.
  3. **A draining retirement protects nothing.** A mark naming a pending `retire:bytes:`
     obligation's event is reclaimed on its own stale grace (`0016:1226-1248`, X97 `:2626`).
- **Success criterion:** the NEW file `crates/custodian/tests/gc_reclaim_intent.rs` passes over
  in-memory doubles built as `crates/custodian/tests/gc_ledger_walk.rs` builds them, a fresh
  `GcContext` each pass; structured values are raw JSON. Every leg seeds a control the pass does
  reclaim. Legs:
  **(A) Three shapes decode; a fourth fails closed.** One mark each: the bare decimal
  `mark_orphaned` writes (`gc.rs:181-193`), `{"orphaned_at_millis":N,"event":"E"}`, and
  `{"orphaned_at_millis":N,"event":"E","reclaiming":true}` (event optional). GC honours each
  (the third per B(iv)). A value that is none of them: GC leaves it byte-identical, never
  reclaims its fragment, and names it on its audit seam (as `gc.rs:1010-1023` does today). Guard,
  green on the base: restore counts all four `already_marked` and leaves each byte-identical —
  existence is its whole judgement (`gc.rs:862-897`), and no reader rewrites a mark
  (`0016:1208-1211`). Base red: a structured mark past grace licenses nothing.
  **(B) Recorded before destroyed (`0016:1312-1338`).** (i) The double errors on the commit
  recording reclaim intent: the fragment is still present. (ii) The mark changes between GC's
  read and its intent commit: that intent loses, its fragment survives, the others in the pass
  are still reclaimed; and a pass whose only candidate loses does not answer `Satisfied`.
  (iii) The double's `delete_fragment` hook commits an adoption CAS `require(orphan:<pos> ==
  <bytes GC read>)` as GC deletes: it gets `Conflict`. (iv) A `reclaiming` mark over a present
  fragment, stamped recently, is finished next pass — fragment deleted once, then the key —
  with no grace test. (v) A store fault ends a pass after it deleted fragments: their keys are
  gone afterwards and the error still propagates. Base: (i)/(iii) delete first, (ii) reclaims
  on a changed mark, (iv) does not decode, (v) drops the queued deletes.
  **(C) Intents are batched.** With 1,001 reclaimable marks (`CLEANUP_BATCH` + 1, `gc.rs:101`,
  written as a number as `gc_ledger_walk.rs:88-90` writes `W`), count the commits that carry a
  precondition or a put on an `orphan:` key: exactly 2, and neither carries more than 1,000.
  Base red: no commit carries one. (v1's surviving mutant committed each intent alone and
  passed every test.)
  **(D) A draining retirement protects by keyed lookup.** A structured mark past grace whose
  `event` is a `RetireToken`'s canonical string (`crates/core/src/multipart.rs:1446-1462`):
  while `retire:bytes:<event>` (`retire_key`, `:1465`) exists the fragment survives; delete it
  and the next pass reclaims. The double records every `scan`/`scan_page`; none reads
  `retire:`. Base red: the second half.
  **(E) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs` (no new DST
  file): a mover's adoption CAS on its pre-mark races GC's reclaim over a delete spanning a
  simulated hop, and never publishes a placement naming a deleted fragment (outcome (c)); a
  coverage property proves both outcomes are reached, as `:2139` does.
  **(F) `cargo xtask ci` green.**
- **Falsifiability:** RED in-process on the bundle's base — `origin/main`, which under
  `wave_mode = "merge"` already holds #803 and #664 when this child builds (earlier waves are
  merged first; `engine/scripts/run-verify.sh:247-266` resolves the same base). No container.
  A–D fail by assertion: neither changes mark handling — #664's restore fence writes `retire:`
  obligations, but no mark shape and nothing in GC. E edits an existing file (C4-ci runs
  it). The test names NO symbol this slice adds — not the codec, not a new `Reconciled` reading;
  everything it uses exists on `main` today, e.g. `wyrd_custodian::{reconcile_step,
  reconcile_after_restore, mark_orphaned, GcContext, ExpiredPendingPolicy, Custodian,
  FencedZone, Reconciled, RestoreReport}`; `wyrd_core::metadata::{orphan_key,
  parse_orphan_key, ORPHAN_PREFIX}`; `wyrd_core::multipart::{retire_key, RetireMode,
  RetireToken, UploadId, PartNumber}`; `wyrd_traits` store types. A red leg that fails to
  compile is UNVERIFIABLE (`run-verify.sh:522-541`). Record in `build-notes.md` how many ran
  red, all by assertion.
- **Invariant to restore:** C-1 — no permanent or data-losing failure mode is an acceptable
  cost: every durable byte is, at every instant, protected by a record that names it or
  evidenced for reclamation, and every state has an actor that exits it (`docs/principles.md`
  §5 C-1, §6 storage-lifecycle row; `0016:2802-2813`; `gc.rs:30-33`). Here: a byte reclaimed on
  a mark is destroyed only after the reclamation is durable as an exact-value transition of that
  mark, so any precondition on its earlier bytes sees it (`0016:1312-1320`); every shape decodes
  the same way for every reader, and one matching none is kept and surfaced, never acted on
  (ADR-0045 decision 3, `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`); a mark
  naming a draining retirement is not reclaimed. Orphan-mark path only: the expired-lease arm
  (`ExpiredPendingPolicy::Reclaim`, off in deployment, `gc.rs:148-175`) has no mark and keeps
  its order (#557/#490). SELF-TEST: a decoder private to `gc.rs` passes the GC legs while the
  later mark writers (#659, #663, #800) spell marks their own way — hence one codec beside
  `orphan_key`, for the reason the key lives there (`crates/core/src/metadata.rs:64-71`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 803, 722, 776, 808, 809, 810
- **Ordering note:** second of the pair: `Conflicts with` #803 (shared `gc.rs`, DST file,
  `06-runtime-view.md`; no build-on); the scheduler builds #803 first. Also conflicts with
  #776 (owns `crates/core/src/metadata.rs`, where the codec goes) and #722 (appends to the DST
  file); added to the field at acceptance (2026-09-15), since proposal ordering fields name
  only siblings. It does not touch `restore.rs`, but it shares
  `docs/design/architecture/06-runtime-view.md` with **#664** (this slice §6.7 step 2; #664 the
  restore fence), so it also conflicts with #664 — added 2026-09-15 at the human's direction,
  in #662's plan-review revision. **RE-POINTED 2026-09-18 — `664` → `808, 809, 810`:** #664 was
  split at its re-plan (`results/issue_664/split-proposal.md`) and all three children edit the
  same paragraph at `06-runtime-view.md:78`, so the conflict edge fans out to each of them
  rather than landing on a parent that is now `close-disposition = split`. #803 has since merged
  (PR #807, `14646e3`) and no longer constrains the order.
  The scheduler orders a conflict pair by id, so the run is
  this slice · #808 · #809 · #810 · #663 · #800 (three children in place of #664). Downstream in
  this run: #800 depends on this child (it decodes these shapes and
  mirrors the lost-precondition handling); #663 too (its adoption CAS is sound only because GC
  records `reclaiming` first) — both re-pointed from `662` at acceptance. **Intake-cap override
  (wyrd-pdca-P1):** granted by Eduard Ralph in #662's re-plan session, 2026-09-15, for #662's
  two-child split, at `planned 22/6 (cap) — room for 0`.
- **Surfaces:** data
- **Difficulty:** high — a persisted value shape in `crates/core` read by GC and later written
  by #659 and #663, plus a restructured reclaim path.
- **Do model:** opus-max
- **Scope:** the `orphan:` value's three shapes and their one codec, beside `orphan_key` in
  `crates/core/src/metadata.rs` (`:62-85`); decode accepts exactly what encode writes, so a
  legacy value round-trips byte for byte; `mark_orphaned`'s output is unchanged. GC: every
  shape decodes; the reclaim intent — an exact-value CAS of the mark to `reclaiming` — commits
  before `delete_fragment`, the key deleted after as today; intents batched at most
  `CLEANUP_BATCH` per commit, a lost precondition costing only its own; `reclaiming` resumed
  with no grace test; a pass that lost an intent and reclaimed nothing is not `Satisfied`; a
  fault after deletes still commits their queued key deletes before the error propagates (best
  effort). A crash can still leave `reclaiming` over a deleted fragment, which a
  `list_fragments()`-driven walk never revisits: #800's sweep — mark it `// deferred: #800`. A
  pending `retire:bytes:` obligation named by a mark's event protects through one keyed `get`
  per candidate, never a range read of `retire:`. Restore is untouched: `marked_among`
  (`gc.rs:862-897`) keeps existence as its whole judgement. The codec doc states the
  writer-side rule 0016 leaves implicit: no writer overwrites a `reclaiming` mark (#663 and
  #659 inherit it). Existing tests stay green, or `build-notes.md` says which expectation
  changed and why. No signature change to `reconcile_step`; no new field on a context struct.
  Docs (a persisted value changes, `AGENTS.md:154-157`): the shapes and their dual-format rule
  in `docs/design/architecture/08-crosscutting-concepts.md` §8.7 after `:85`;
  record-before-destroy in `06-runtime-view.md` §6.7 step 2, for marked fragments only, saying a
  draining retirement's fragments are never reclaimed (its drain is what marks them). / out of
  scope: the staged class (child-1); `restore.rs`; the orphan-identity migration gate (X92/X111,
  `0016:1249-1280`) and the three-arm mark write (`0016:1218-1224`), both #659's; the
  fragment-less sweep (#800); the expired-lease arm's order; `scrub.rs`, `reconstruction.rs`,
  `rebalance.rs`, `desired_state.rs`; 0016 and the ADRs.
- **Repro instruction:** on `origin/main`, write `orphan:<pos>` =
  `{"orphaned_at_millis":0,"event":"g:1:1"}` for an unreferenced fragment and run GC past
  grace: it is never reclaimed and the mark is reported unreadable. With a legacy mark, a
  `delete_fragment` hook committing `require(orphan:<pos> == <its bytes>)` succeeds after the
  fragment is gone.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/gc_reclaim_intent.rs` — **NEW**. C4-verify earns its
  red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`, `:390-392`);
  the codec's unit tests may sit in `metadata.rs`. No `Cargo.toml` change.
- **Production reach:** the production `reconcile_step` (and `reconcile_after_restore` for A)
  over in-memory stores. No in-tree writer emits a structured mark before #659, so the test
  seeds them; `reclaiming` is GC's own write from this slice on.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY
  open: `gc.rs:264-346` (fleet loop, reclaim arm) and `:899-936` (`Cleanup`); `gc.rs:638-648`,
  `:794-819` (`ReadMark`, `classify_ledger_entry`); `metadata.rs:62-85`, `:1338`
  (`parse_canonical_u64`); `multipart.rs:1446-1512` (`RetireToken`, `retire_key`,
  `parse_retire_key`); `gc_ledger_walk.rs:84-90`, `:109-420`, `:545` (`B`/`W`, the doubles,
  `assert_bounded`).
- **Prior-art check (triage cycles):** by path across merged, open and closed work: no merged
  `reclaiming` state or structured mark (`git grep reclaiming origin/main` finds prose only),
  no open PR on these files, closed #647 unrelated. Rejected: #637 v1's review found the
  lost-CAS fallback untested (B(ii)); #662 v1 (188 KB, with child-1) left the delete-then-die
  window unmarked (`deferred: #800`, B(v)), a batching mutant alive (C), docs that claimed
  record-before-destroy for every pass and "never marked" for draining retirements, and the
  writer-side rule unstated (Scope).
- **Disposition hint:** likely-fix
