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
- **Conflicts with:** 804, 722
- **Ordering note:** first of the pair: `Conflicts with` #804 (shared `gc.rs`, DST file,
  `06-runtime-view.md`; no build-on), and the scheduler builds the lower id — this one — first.
  Also conflicts with #722 (appends to `crates/dst/tests/custodian.rs`, as #661 and #800
  declared); added to the field at acceptance (2026-09-15), since proposal ordering fields name
  only siblings. #661 is merged (PR #802). Downstream in this run: #664 depends on this child,
  #663 on both children (both re-pointed from `662` at acceptance). **Intake-cap override (wyrd-pdca-P1):** granted by
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

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Size backstop (106 KB vs 100 KB threshold) waived by the human: the overage is minor and does not warrant a re-plan/split. Required fix for the rebuild: close the test-fidelity gap the adversarial reviewer demonstrated — every fixture seeds sessions in the `Open` state, but real publication handoffs never happen from `Open` (they require `Completing`/`Completed`); a one-line filter dropping non-`Open` sessions passed all 11 tests plus both DST properties, proving the gap is real. Seed `Completing`, `Aborting`, and `Completed` sessions in legs A and B (valid shapes in `crates/core/tests/multipart_session_records.rs:81-141`); in leg C(ii) and DST property 13, move the session to `Completing` before the publication batch and to `Completed` within it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Size backstop (106 KB vs 100 KB threshold) waived by the human: the overage is minor and does not warrant a re-plan/split.
  Required fix for the rebuild: close the test-fidelity gap the adversarial reviewer demonstrated — every fixture seeds sessions in the `Open` state, but real publication handoffs never happen from `Open` (they require `Completing`/`Completed`); a one-line filter dropping non-`Open` sessions passed all 11 tests plus both DST properties, proving the gap is real. Seed `Completing`, `Aborting`, and `Completed` sessions in legs A and B (valid shapes in `crates/core/tests/multipart_session_records.rs:81-141`); in leg C(ii) and DST property 13, move the session to `Completing` before the publication batch and to `Completed` within it.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Required fixes for the rebuild (both are the reason for this iteration, size overage is not a factor — waived by the human): 1. Fitness-to-purpose: fail-closed retention (one unreadable staged record can stall cleanup fleet-wide until fixed or drained) — keep this behavior for now, but the human wants the rebuild to close the second item so this tradeoff is fully covered rather than silently possible to break. 2. Test-fidelity gap (adversary review): no test pins "scrub and drain status keep today's answers" (brief.md:96-99). Add `unresolvable.extend(staged.unresolvable.clone())` at the exact site named (`crates/custodian/src/gc.rs:641`, just before `Ok(ReferenceSet {`) as the concrete mutant this must catch, and add the assertion in `e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore` (`crates/custodian/tests/staged_protection.rs:1043`) or `assert_blocks_both_passes` (`:990`) confirming scrub is not `Blocked` in the torn-`sidx:` (E(i)) case. Leave the drain-status half out or under `// deferred: #664` per the adversary's note — it pins an interim answer #664 may change.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Required fixes for the rebuild (both are the reason for this iteration, size overage is not a factor — waived by the human):
  1. Fitness-to-purpose: fail-closed retention (one unreadable staged record can stall cleanup fleet-wide until fixed or drained) — keep this behavior for now, but the human wants the rebuild to close the second item so this tradeoff is fully covered rather than silently possible to break.
  2. Test-fidelity gap (adversary review): no test pins "scrub and drain status keep today's answers" (brief.md:96-99). Add `unresolvable.extend(staged.unresolvable.clone())` at the exact site named (`crates/custodian/src/gc.rs:641`, just before `Ok(ReferenceSet {`) as the concrete mutant this must catch, and add the assertion in `e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore` (`crates/custodian/tests/staged_protection.rs:1043`) or `assert_blocks_both_passes` (`:990`) confirming scrub is not `Blocked` in the torn-`sidx:` (E(i)) case. Leave the drain-status half out or under `// deferred: #664` per the adversary's note — it pins an interim answer #664 may change.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Fail-closed retention tradeoff accepted as-is (one unreadable staged record stalls GC/restore fleet-wide until fixed or drained — deliberate, keep it). Size backstop overridden: patch is 122 KB vs 100 KB threshold, this is round 3 — human explicitly chose iterate-do over iterate-plan, ignore the size count. Required fix for the rebuild: close the store-error propagation test gap the adversarial reviewer demonstrated. Replacing the `?` at `crates/custodian/src/gc.rs:791` (`mpu:`), `:799` (`sidx:<id>:`) and `:802` (`part:<id>:`) with `.unwrap_or_default()` still passes all 11 tests plus the DST suite — a transient backend read failure silently becomes "nothing staged," which is the exact data-loss shape this feature exists to prevent. Add a test that makes the `Meta` double (`crates/custodian/tests/staged_protection.rs:201-208`) fail `scan` for a chosen prefix, then for each of `mpu:`, `sidx:<id>:` and `part:<id>:` assert both `reconcile_step` (GC) and `reconcile_after_restore` return `Err`, and that the staged fragment is still on disk and unmarked.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Fail-closed retention tradeoff accepted as-is (one unreadable staged record stalls GC/restore fleet-wide until fixed or drained — deliberate, keep it).
  Size backstop overridden: patch is 122 KB vs 100 KB threshold, this is round 3 — human explicitly chose iterate-do over iterate-plan, ignore the size count.
  Required fix for the rebuild: close the store-error propagation test gap the adversarial reviewer demonstrated. Replacing the `?` at `crates/custodian/src/gc.rs:791` (`mpu:`), `:799` (`sidx:<id>:`) and `:802` (`part:<id>:`) with `.unwrap_or_default()` still passes all 11 tests plus the DST suite — a transient backend read failure silently becomes "nothing staged," which is the exact data-loss shape this feature exists to prevent. Add a test that makes the `Meta` double (`crates/custodian/tests/staged_protection.rs:201-208`) fail `scan` for a chosen prefix, then for each of `mpu:`, `sidx:<id>:` and `part:<id>:` assert both `reconcile_step` (GC) and `reconcile_after_restore` return `Err`, and that the staged fragment is still on disk and unmarked.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Root cause is one design flaw, not implementation slips: staged-record reading (for the new GC/restore protection class) was put into code shared by consumers (scrub, drain-status) that never asked for it and discard the answer. That one placement decision produced three findings: (1) scrub/drain-status now fail on a transient staged-record read fault, though the brief required them to keep today's answers; (2) every consumer now pays for reading records most of them don't need (up to 93 scans/server); (3) restore's own fix is still incomplete for a narrow upload-timing window (restore.rs:339). Patch is 128 KB (100 KB threshold) and this is round 3 (2-round threshold) — re-plan and split so "protect staged fragments in GC/restore" is scoped separately from any shared-builder change that touches scrub/drain-status, and so restore's remaining gap gets its own slice with a regression test.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Root cause is one design flaw, not implementation slips: staged-record reading (for the new
  GC/restore protection class) was put into code shared by consumers (scrub, drain-status) that
  never asked for it and discard the answer. That one placement decision produced three findings:
  (1) scrub/drain-status now fail on a transient staged-record read fault, though the brief required
  them to keep today's answers; (2) every consumer now pays for reading records most of them don't
  need (up to 93 scans/server); (3) restore's own fix is still incomplete for a narrow upload-timing
  window (restore.rs:339). Patch is 128 KB (100 KB threshold) and this is round 3 (2-round threshold)
  — re-plan and split so "protect staged fragments in GC/restore" is scoped separately from any
  shared-builder change that touches scrub/drain-status, and so restore's remaining gap gets its own
  slice with a regression test.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
