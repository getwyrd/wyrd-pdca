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
- **Depends on:** 661
- **Ordering note:** wave 2. child-1 (#661) rewrites the same reclaim path into a paged,
  mark-driven walk, and this slice's reclaim-intent write lands inside it, so this is a
  build-on dependency, not only a shared file. The tracker body also names #654 (the record
  types); #654 was split and its record types landed in #691, #715, #716, #771 and #772, all
  merged. child-3 and child-4 build on this slice, and so does #800 (the sweep of marks with no
  fragment, split out of #661 on 2026-09-13).
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
  (child-4); scrub and reconstruction (child-3); the ledger walk's paging (child-1, #661 — keep
  its rules intact); the sweep of marks with no fragment (#800, split out of #661 on
  2026-09-13, which builds on this slice); `desired_state.rs`, `rebalance.rs`,
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

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized: 188 KB patch vs. the 100 KB threshold, with four distinct GC mechanisms (staged protection class, keyed pending-retirement protection, reclaim-intent ordering with restart recovery, three-shape mark decoding) landing in one gc.rs change. Re-slice in Plan along the brief's own defect groupings: (1) staged-bytes protection class — legs A/B/C/D, the piece that stops live-upload data loss; (2) reclaim ordering + mark decoding + pending-retirement protection — legs E/F/G, the crash-safety/format-completeness piece. Each half still needs its own DST coverage. Run `pdca split 662` then `pdca split 662 --accept` to file the child briefs.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Slice is oversized: 188 KB patch vs. the 100 KB threshold, with four distinct GC mechanisms
  (staged protection class, keyed pending-retirement protection, reclaim-intent ordering with
  restart recovery, three-shape mark decoding) landing in one gc.rs change. Re-slice in Plan along
  the brief's own defect groupings: (1) staged-bytes protection class — legs A/B/C/D, the piece
  that stops live-upload data loss; (2) reclaim ordering + mark decoding + pending-retirement
  protection — legs E/F/G, the crash-safety/format-completeness piece. Each half still needs its
  own DST coverage. Run `pdca split 662` then `pdca split 662 --accept` to file the child briefs.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 85 mutants tested in 2m: 1 missed, 40 caught, 44 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_662/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
