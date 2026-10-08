# Adversarial review — issue #777 (segmented repair through `repoint_chunk`)

Short verdict: I could not break the production change. The red→green reproduces, the tests go
through the real `reconcile_step` path, and the named negations for legs 3–5 hold. The weak spot
is the **new containment path** the patch adds: no test runs it, and I showed that removing it
leaves every shipped test green. Two further items need a human scope call (DST coverage, the
docs deferral).

## Findings

- NEEDS-HUMAN [impl] — **The new move-time containment arm has no test at all.**
  `crates/custodian/src/reconstruction.rs:448-454` (with `:1157-1167` and `Reading::contain`'s
  once-per-object dedupe at `:541-546`) is how a typed `ChunkMapError` from the move becomes
  `Blocked` rather than a clean pass. C4-diff-cov lists all of these lines as MISS. C5 reports
  "0 missed" only because cargo-mutants made no mutant for an `if let` body. I disabled the
  containment by hand (the `if let Target::Committed(site)` at `:449` never matches) and ran
  the **whole** `wyrd-custodian` suite (27 test binaries, including the 5 new legs and the
  rewritten `segmented_map_reconstruction.rs` leg 2): **everything still passed.** What that
  mutant does: if a `seg:` record becomes undecodable between the resolve and the move while
  the root still names it, the pass answers `Satisfied` instead of `Blocked`, which tells the
  operator redundancy is fine when it is not (C-1). The current code handles this correctly.
  My attack test (arm `Race::AfterSegmentPage` with bytes `{not a segment record` at `seg:…:1`)
  gets `Blocked`, the obligation stays queued, the record is byte-identical, the root is
  untouched and no orphan mark is written. Fix: add that leg to `segmented_map_repoint.rs`, and
  add a two-obligations-in-one-object variant that checks exactly one `unresolvable` row for
  `inode:1`. That covers guard (a), "once per OBJECT", on the path that replaced the refusal.

- NEEDS-HUMAN [impl] — **Leg 1 checks the orphan mark by count only**
  (`crates/custodian/tests/segmented_map_repoint.rs:420-424`, `orphans(&meta).await.len() == 1`).
  The rubric's *Absent or unsupported entries* class names count-based assertions, and the
  T4 batch review raised this twice. The production code is right: I checked, and the single
  key is `orphan:1:41472:1` = `metadata::orphan_key(LOST, FragmentId { chunk: CHUNK, index: 1 })`.
  But a bug that marked the survivor or the destination would still pass this assertion.
  Assert equality on that key instead. `orphan_key` is a base symbol, so the red leg still
  compiles.

- NEEDS-HUMAN [human] — **No seeded Tier-0 DST coverage for the new concurrent write path.**
  This diff is the first time reconstruction writes `seg:` records. The sequence is: prepare
  the move, which re-reads the segment (`reconstruction.rs:1142-1150`); write the fragments;
  then commit the segment CAS, the obligation delete and the orphan marks together. The rubric
  (*Test fidelity*) requires seeded Tier-0 coverage for a new destructive or concurrent path.
  Today only fixed, scripted Tokio races cover it. `crates/dst/tests/custodian.rs` has
  segmented resolve and GC properties (`:1537`, `:1605`) but nothing for segmented
  reconstruction. The brief puts that file off-limits (#722). T4-batch-review fails (a gating
  gate) on this finding three times. A human has to choose: accept a `// deferred: #722`
  marker, which this diff does not add yet, or widen the scope.

- NEEDS-HUMAN [human] — **The docs-currency deferral lands on this issue, and this diff drops
  it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living
  architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing
  calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring.
  It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out
  both the doc update and a fix to the marker, whose "nothing calls this yet" is now false.
  After merge, the deferral points at a closed issue. The rubric makes docs currency a merge
  requirement, and its protocol says to raise the tracking issue when a deferral looks wrong.
  Here the tracking issue is this one. A human decides: add the doc update to this PR, or
  file a new issue and re-point the marker.

- NEEDS-HUMAN [impl] — **Comments the patch made false and left in place.**
  `reconstruction.rs:316` still says "Like the `seg:` refusal", but that refusal is gone.
  `reconstruction.rs:410-419` still says each commit is "conditioned on the generation THE SCAN
  returned" and that "a second obligation inside the same object still loses the CAS it always
  lost". Neither holds for segmented objects any more: the snapshot is now the *resolved*
  generation, and two owed chunks in the **same** `seg:` record both land in one pass. My
  attack test got `Changed`, placements `[[0,2],[0,2]]`, an empty queue and 2 orphans, and
  `segmented_map_reconstruction.rs:490` asserts the same across segments. Also, the new test's
  module doc (`segmented_map_repoint.rs`, "Legs 3–4 fail on the base only on their `Satisfied`
  verdict") is wrong for leg 4. On the base, leg 4 fails at `assert!(meta.raced())` (`:357`,
  "the race never landed"), because the base never commits. Harmless, but the doc should say so.

## Informational (not tagged: settled, or too minor to spend a rebuild)

- **#698 (settled, not raised against this diff):** a segmented root stored under a
  non-canonical key such as `inode:01` now fails silently. I seeded one: on the **base** every
  pass answers `Blocked`. **Patched**, every pass answers `Satisfied`, the obligation stays
  queued forever, and each pass writes a rebuilt fragment to the free server that is then
  stranded. That happens because `parse_inode_key` (`reconstruction.rs:650`) maps the key to
  `1`, and the root pin (`metadata.rs:3257-3258`) CASes against `inode:1`, which does not
  exist. The flat arm has always behaved this way, the writer is unreachable today, and the
  brief assigns this to #698. Suggest noting on #698 that the segmented arm now shares the
  flat arm's silent failure and lost the `Blocked` signal the refusal gave it.
- `Repoint::Refused`'s contract (`metadata.rs:3134-3138`) asks the caller to confirm the
  generation is still current before escalating. The caller escalates straight away
  (`reconstruction.rs:455-458`: ceiling counter plus `Blocked`). Worst case: one spurious
  `reconstruction_ceiling_refused` tick and one `Blocked` pass when a supersede lands between
  the resolve and the move. The base flat arm did the same.
- The file budget said 3; the patch touches 4. The `staged.rs` change is a one-line removal
  forced by dropping `RepairPlan::chunk_index`; keeping the field would be dead code under
  clippy `-D warnings`.

## Attempted refutations that did not land

- **Red→green, re-run myself** (scratch copy, base+#776 production, the new test kept):
  4 failed and 1 passed (leg 5) on the base; 5 passed with the patch. This matches the
  C4-verify log. The tests go through `reconcile_step` and assert on the store, not on a
  parallel copy of the logic.
- **Named negations** (a mutation that removes one pin; the named leg must then fail), each
  applied to `metadata.rs` in scratch: comparing only the chunk id in `chunk_at` (`:3374`)
  → leg 3 fails; dropping the root pin in the segmented arm (`:3325`) → leg 4 fails;
  skipping `weighed` in the segmented arm → leg 5 fails. So the "bound by named negation"
  claims hold.
- **Supersede onto a flat generation mid-resolve** (a flat root at `inode:1`, v2, raced in
  after the segment page): the resolve restarts, and the move repoints the *live* flat root
  (v3, CHUNK → `[0,2]`, queue drained). The switch of `Object::prior` to
  `ResolvedChunkMap::record` (`reconstruction.rs:683-689`) is correct.
- **Guard (b), no new silent skip:** a segmented object whose key does not parse is contained,
  not skipped (`reconstruction.rs:676-682`), and the base's flat `continue` (`:655-657`) is
  kept, as the brief requires.
