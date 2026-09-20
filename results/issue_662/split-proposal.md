<!-- pdca:split-proposal v1 -->
# Split proposal — issue 662

**Intake-cap override (wyrd-pdca-P1).** Granted by the human (Eduard Ralph) in #662's re-plan
session, 2026-09-15, for this split only. `scripts/plan-cap --need 2` at the override:
`planned 22/6 (cap) — room for 0, need 2: Plan intake closed`. That 22 did not count #662
itself (its brief was archived by `iterate-plan`, so it read UNPLANNED). After acceptance the
readout is `planned 25/6`: #803 and #804 are PLANNED, and #662 reads BUILT on its split marker
until the flow reaps it as split — so the split's lasting effect is +2 on the 22 readout, or +1
against the count from before #662 was iterated, when it was still AWAITING_SIGNOFF.

## Why this slice is oversized

v1 built all of #662 as one 188 KB patch (threshold 100 KB). Every gate but the batched review
was green and the adversary review could not refute it, yet it carried four mechanisms in one
`gc.rs` change: the staged protection class, the reclaim-intent ordering with its restart, the
three `orphan:` value shapes, and keyed protection while a retirement drains. They are two
outcomes that each ship alone, and sign-off drew the seam along the brief's own defect groups:

- **child-1 — staged protection.** Stops live-upload data loss: a committed part's and an
  in-flight upload's fragments join the shared reference set GC and restore gate on. It reads
  existing records and writes no new persisted shape.
- **child-2 — how GC consumes a mark.** Crash-safety and format completeness: record the
  reclaim before destroying the bytes, decode all three mark shapes, and keep a mark whose
  retirement is still draining. It adds a persisted value shape. It no longer touches
  `restore.rs` (v1 had restore decode marks too; existence is restore's whole judgement, and
  GC already names an unreadable value on every pass).

v1 measured ~75 KB for the first half and ~100 KB for the second (the codec with its unit
tests was 20 KB of that), so child-2 may still read near the threshold; the size signal is
advisory and the human weighs it at sign-off. Each child keeps its own DST property.

## Wave sketch

The two children share `crates/custodian/src/gc.rs`, `crates/dst/tests/custodian.rs` and
`docs/design/architecture/06-runtime-view.md`, but neither needs the other's result, so they
declare **`Conflicts with`**, not a dependency. `compute_waves` orients a conflict pair by id,
so child-1 (filed first, lower id) builds first; under `wave_mode = "merge"` with
`auto_merge = true` the driver merges child-1's PR before child-2 builds. If child-1 stalls,
child-2 is not held.

**This run is `pdca flow 662 663 664 800`.** #663, #664 and #800 all declare `Depends on: 662`,
and the driver levels a dependent of a split parent by its own edges, not by the children
(`src/pdca_harness/flow.py:1256-1260`) — so once #662 closes as split, all three would land in
the SAME wave as child-1, on a base without it, and fail. Their fields are therefore re-pointed
in this session, right after `--accept` and before it ends (editing an existing brief's fields
is cap-exempt): **#664** → `Depends on: <child-1>`; **#800** → `Depends on: <child-2>`;
**#663** → `Depends on: <child-1>, <child-2>`. The resulting order: child-1 · then child-2 and
#664 in parallel · then #663 · then #800 (it conflicts with #663).

Outside the batch: both children conflict with **#722** (appends a DST property to
`crates/dst/tests/custodian.rs`, as #661 and #800 declared), and child-2 with **#776** (owns
`crates/core/src/metadata.rs`). A proposal's ordering fields may name only siblings
(`src/pdca_harness/split.py:308-318`), so these ids go into the materialised children's
`Conflicts with` after acceptance.

<!-- pdca:child child-1 -->
# custodian: staged protection class in the shared reference set (662.1)

> Child 1 of 2 of #662's split (637.2). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `78f9859` (verified 2026-09-15). 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; background: its decision 2
> (`:765-893`).

- **Slug:** staged-reference-set
- **Kind:** enhancement
- **Defect:** staged bytes have no protection class. `ReferenceSet` holds committed placements
  only (`crates/custodian/src/gc.rs:383-413`), built from the `inode:` scan alone (`:478-573`).
  A committed part's fragments (`part:`) and an upload's in-flight owned fragments (`sidx:`,
  #772) are in no protected set, so GC reclaims one as soon as it carries an `orphan:` mark past
  grace (`:272`, `:277-326`). Restore gates on the same predicate
  (`crates/custodian/src/restore.rs:385`) and its pending skip (`:435-438`) no longer sees owned
  entries, so it marks a live upload's fragments stranded (`:440-443`) and the next GC pass
  deletes them.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Records are seeded as raw JSON the base decoders accept (`SessionRecord`
  and `PartRecord` have no writer-side constructor, `crates/core/src/multipart.rs:2127`,
  `:2492`; shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Every protection leg also seeds an unprotected control the pass does reclaim or mark.
  Legs:
  **(A) GC protects both staged classes.** An `Open` session with a committed `part:` record
  (fragment `F1`) and an owned `sidx:` entry (`F2`) on D-server doubles, each with an `orphan:`
  mark past grace: after `reconcile_step`, both survive. (Unmarked, GC's conservative arm keeps
  any fragment, `gc.rs:307-310`, so the mark is what makes the leg bite.) Base: both reclaimed.
  **(B) Restore protects them through the same predicate.** `reconcile_after_restore` over the
  same store, unmarked, writes no `orphan:` key for `F1`/`F2` and `stranded_marked` excludes
  them; a GC pass past grace then keeps both. Base: marked, then deleted. (Staged counters are
  #664's.)
  **(C) Source before destination, both handoffs (`0016:782-800`, X67 `:2596`).** A double
  performs a handoff atomically after the first of the two reads involved completes — the
  source range or the destination range/scan, whichever the builder issues first: (i) a part
  commit (one batch deletes the chunk's `sidx:` entry and writes its `part:` record; source
  `sidx:<id>:`, destination `part:<id>:`); (ii) a publication (the committed inode naming the
  chunk is written and its `part:` record removed; source `part:<id>:`, destination the
  `inode:` scan). The fragment is marked past grace and is not reclaimed. Base: reclaimed.
  **(D) Bounded per-session reads (`0016:890`).** With the `scan` cap lowered, more
  sessions-with-parts than a global `scan("part:")` could return: `reconcile_step` succeeds and
  the double records no `scan`/`scan_page` of the bare `part:` or `sidx:` prefix. A guard.
  **(E) What it cannot read or trust fails closed (ADR-0045 decision 3,
  `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).** (i) A `part:` value that
  will not decode, an `sidx:` key naming no chunk, or an `mpu:` key naming no upload makes the
  set incomplete for GC and restore: GC reclaims nothing and answers `Reconciled::Blocked` (as
  `gc.rs:348-355`); restore marks nothing and names the record in `RestoreReport::unresolvable`.
  (ii) A staged placement of the wrong length, or an undecodable owned value under an `sidx:`
  key that names its chunk, holds that whole chunk in both passes and is named on each audit
  seam, while unrelated fragments are still judged. Base: reclaimed or marked.
  **(F) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53`; no new DST file): a concurrent part commit then publication at
  seed-chosen instants during GC's staged build never gets the chunk reclaimed; a coverage
  property proves landings between and outside the builder's reads are both reached, as `:2139`
  does.
  **(G) `cargo xtask ci` green.**
- **Falsifiability:** RED in-process on `origin/main` @ `78f9859`, no container. A, B, C, E fail
  by assertion: every seeded record class exists on `main` (#691, #715, #716, #771, #772) and
  nothing in the maintenance plane reads it. D is a guard; F edits an existing file (C4-ci runs
  it). The test names NO symbol this slice adds — no new `ReferenceSet` member, reason string,
  or report field; everything it uses exists on `main` today, e.g. `wyrd_custodian::{reconcile_step,
  reconcile_after_restore, mark_orphaned, GcContext, ExpiredPendingPolicy, Custodian,
  FencedZone, Reconciled, RestoreReport}`; `wyrd_core::multipart::{mpu_key, part_key,
  part_range, sidx_key, sidx_range, UploadId, PartNumber, OwnedEntry, StagedPlacement,
  decode_session_record, decode_part_record, decode_owned_entry}`;
  `wyrd_core::metadata::{orphan_key, inode_key, encode, decode, InodeRecord, PendingEntry,
  EcScheme, ORPHAN_PREFIX}`; `wyrd_traits` store types. A red leg that fails to compile is
  UNVERIFIABLE (`engine/scripts/run-verify.sh:522-541`). Record in `build-notes.md` how many
  tests ran red, all by assertion.
- **Invariant to restore:** C-1 — no permanent or data-losing failure mode is an acceptable
  cost: every durable byte is, at every instant, protected by a record that names it or
  evidenced for reclamation (`docs/principles.md` §5 C-1, §6 storage-lifecycle row;
  `0016:2802-2813`; `gc.rs:30-33`; 0016 invariant (2), `:869-871`). Here: a staged byte is
  protected by the SHARED reference set every destructive pass reads, as a member disjoint from
  committed placements so each consumer decides for itself (`0016:767-782`, `:881`); protection
  overlaps across handoffs — "no gaps", never a partition (`0016:2911-2922`). SELF-TEST: a
  filter inside GC alone passes A and fails B.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** child-2
- **Ordering note:** first of the pair: `Conflicts with` child-2 (shared `gc.rs`, DST file,
  `06-runtime-view.md`; no build-on), and the scheduler builds the lower id first. Also
  conflicts with #722 (appends to `crates/dst/tests/custodian.rs`, as #661 and #800 declared) —
  added to the field after acceptance, since proposal ordering fields name only siblings. #661
  is merged (PR #802). Downstream in this run: #664 depends on this child, #663 on both children
  (re-pointed from `662` after acceptance). **Intake-cap override (wyrd-pdca-P1):** granted by
  Eduard Ralph in #662's re-plan session, 2026-09-15, for #662's two-child split, at
  `planned 22/6 (cap) — room for 0`.
- **Surfaces:** data
- **Difficulty:** high — the shared builder is read by GC, restore, scrub
  (`crates/custodian/src/scrub.rs:88`) and drain status (`desired_state.rs:188`).
- **Do model:** opus-max
- **Scope:** the staged protection class in the shared reference set: the committed `part:`
  records and owned `sidx:` entries of the sessions listed under `mpu:`, read through bounded
  per-session ranges, `sidx:` before `part:` before the `inode:` scan; its own member, disjoint
  from `placed`, honoured by the shared predicate (`gc.rs:424-451`) under its own audit reason.
  Never less than 0016's set; covering every listed session whatever its state is fine (it only
  keeps more). Scrub and drain status keep today's answers — they read `placed` and the
  committed `unresolvable` only — so an unreadable staged record makes the set incomplete for
  GC and restore alone. Restore names each staged record it holds or cannot read on its audit
  seam, and its report fields stay as they are (`restore.rs:105-170`): whether a held staged
  record sets `needs_human()` is #664's, marked `// deferred: #664` at the site. No change to
  `reconcile_step`'s or `reconcile_after_restore`'s signature; no new field on a context struct
  or `RestoreReport`. Docs: one paragraph in `docs/design/architecture/06-runtime-view.md` §6.7
  step 2 — GC never reclaims, and restore never marks, a staged fragment. / out of scope: mark
  shapes, reclaim intent, retirement protection (child-2); drain status, rebalance, restore's
  staged counters and fence (#664); scrub, reconstruction (#663); the fragment-less sweep
  (#800); `desired_state.rs`, `rebalance.rs`, `scrub.rs`, `reconstruction.rs`,
  `crates/core/src/metadata.rs`; 0016 and the ADRs.
- **Repro instruction:** on `origin/main`, seed an `Open` session, one `part:` record and one
  owned `sidx:` entry whose fragments sit on a D server, mark each `orphan:` older than grace,
  run one GC pass: both fragments are deleted.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_protection.rs` — **NEW**. C4-verify earns its
  red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`, `:390-392`);
  keep other test edits in existing files. No `Cargo.toml` change.
- **Production reach:** the production `reconcile_step` and `reconcile_after_restore` over
  in-memory stores. No client creates a session before the S3 verbs (#508), so the test seeds
  every staged record — the intended state.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY
  open: `gc.rs:383-452` (`ReferenceSet`, `protection()`, `protects()`); `gc.rs:478-573`
  (`referenced_fragments`; contain an unreadable record as `:496-533` does);
  `multipart.rs:1212-1329` (`mpu:`/`part:`/`sidx:` keys and ranges), `:2578`, `:3544-3568`,
  `:3730-3757` (the decoders and `StagedPlacement`); `crates/dst/tests/custodian.rs:2116-2173`
  (the pattern for F).
- **Prior-art check (triage cycles):** by path across merged, open and closed work: no merged
  staged class (`git log -S'sidx' origin/main -- crates/custodian/src` is empty), no open PR on
  these files, closed #647 unrelated. Rejected: #508's 4th attempt (a read-path-only resolver —
  restore stranded parts, GC deleted them; leg B); #637 v1 (334 KB); #662 v1 (188 KB, with
  child-2's work; its review found restore holding a malformed staged record silently — the
  audit-seam rule and `deferred: #664` above).
- **Disposition hint:** likely-fix
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# custodian: GC records reclaim intent before deletion + orphan-mark value shapes (662.2)

> Child 2 of 2 of #662's split (637.2). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `78f9859` (verified 2026-09-15); child-1 usually
> builds first and moves `gc.rs` lines — re-locate by symbol. 0016 =
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
  `wave_mode = "merge"` already holds child-1 when this child builds (the earlier wave is
  merged first; `engine/scripts/run-verify.sh:247-266` resolves the same base). No container.
  A–D fail by assertion: child-1 changes no mark handling. E edits an existing file (C4-ci runs
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
- **Conflicts with:** child-1
- **Ordering note:** second of the pair: `Conflicts with` child-1 (shared `gc.rs`, DST file,
  `06-runtime-view.md`; no build-on); the scheduler builds child-1 first. Also conflicts with
  #776 (owns `crates/core/src/metadata.rs`, where the codec goes) and #722 (appends to the DST
  file) — added to the field after acceptance, since proposal ordering fields name only
  siblings. It does not touch `restore.rs`, so it shares no file with #664 and can build beside
  it. Downstream in this run: #800 depends on this child (it decodes these shapes and mirrors
  the lost-precondition handling); #663 too (its adoption CAS is sound only because GC records
  `reclaiming` first) — both re-pointed from `662` after acceptance. **Intake-cap override
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
<!-- pdca:end child-2 -->
