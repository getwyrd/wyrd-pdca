# Adversarial review — issue #721 (advisory, never gating)

Re-ran the asserted red→green on a **writable copy** of `$PDCA_TARGET` (scratch, not the target):
`crates/custodian/tests/segmented_map_repoint.rs` compiles against the base and goes
**2-of-6 red** with production reverted (`a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record`,
`a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged`, both `left: Blocked  right: Changed`),
**6-of-6 green** with the patch — identical to `gate-logs/C4-verify.log`. The evidence is real, drives the
production path (`reconcile_step` → `read_committed` → `repair_chunk` → `metadata::repoint_chunk`) and observes the
store, not a re-implementation. Two findings survive.

## Findings

- **NEEDS-HUMAN [human] — a lost segmented repair now certifies `Satisfied` where the base answered `Blocked`, and the
  test was weakened to accept it** (`crates/custodian/src/reconstruction.rs:344`;
  assertion at `crates/custodian/tests/segmented_map_repoint.rs:420-426`). The brief's success criterion for legs 3
  and 4 says the pass "does not certify"; the delivered assertion only excludes `Reconciled::Changed`. I replaced it
  with `assert_eq!(outcome, Reconciled::Blocked)` in the scratch copy and ran the file: legs 3, 4 **and** 6 all report
  `left: Satisfied`. So for a segmented object whose repair lost — including the two **new, prepare-time** conflict
  arms (`Repoint::Conflict` from the root/record extent check at `crates/core/src/metadata.rs:2893` and from the
  addressing pin at `:2944`) — `reconcile_step` answers `Satisfied` while the obligation is still queued and the chunk
  is still under-replicated, and the only trace is a per-chunk `reconstruction_conflict` tick
  (`reconstruction.rs:314`); the per-object audit line that named the object pre-fix (`emit_refused`) was deleted with
  the refusal. `hole` (`:344`) counts only `reading.incomplete || ceiling_refused`. This is a human call, not a build
  defect: folding `Conflict` into `hole` would also change the flat path's long-standing answer, which this slice's
  scope excludes — so either the brief's leg-3/4 wording is struck at sign-off, or a follow-up owns "a queued
  obligation the pass could not discharge is a hole".
- **NEEDS-HUMAN [impl] — a zero-length `ChunkRef` that ends a segment record is addressed to the *wrong* record, so its
  repair is refused on every pass, forever** (`crates/core/src/metadata.rs:2865-2868`, the `covers` selection, with
  `crates/custodian/src/reconstruction.rs:521-522` supplying the accumulated offset). Concrete case, run through
  `repoint_chunk` in the scratch copy: segment 0 = `[len 8, len 0]`, segment 1 = `[len 8]`, root table `0..8`, `8..16`
  — accepted by `SegmentRecord::new` (only the record *total* must be non-zero, `:1172`) and by `SegmentedMap::new`
  (only a *segment's* `byte_len`, `:923`), and accepted by the resolver. The zero-length chunk's accumulated object
  offset is `8`, which `covers` (`:2924`) attributes to **segment 1**, whose record does not hold it, so `chunk_at`
  misses and the move answers `Conflict`; my probe printed `zero-length chunk at a segment boundary answered
  Conflict`. Nothing changes between passes, so the obligation is planned and refused every pass — the
  "state with no actor that exits it" this issue exists to remove — and per the finding above it is reported as
  `Satisfied`. Cheap fixes: reject `ChunkRef.len == 0` in `SegmentRecord::new`/decode, or resolve the covering segment
  against the record that actually holds `prior` rather than the half-open span alone. Reachability caveat, stated
  honestly: no in-tree producer emits a zero-length chunk and none publishes a segmented map at all (#653), so this is
  a shape the stored format admits, not one this build writes today — the primitive's own doc claim that "one rule
  addresses either [tiling]" (`:2787`) is nonetheless false at that boundary.

## Calibration on the T4 blocking set (advisory, so the human weighs them correctly)

- `T4-batch-review`'s finding "a flat repoint on `version == u64::MAX` panics/wraps" (`crates/core/src/metadata.rs:2844`)
  is **not attributable to this diff**: the base's own `repair_chunk` computed `object.prior.version + 1` on the same
  record, and `commit_chunk_map` (`:1769-1797`) still does. The patch relocated the expression; the class is
  pre-existing repo-wide and needs 2^64 repairs of one inode to reach.
- `T4-batch-review`'s ADR-0045 finding — an undecodable freshly-read `seg:` row collapsing to `Repoint::Conflict`
  (`crates/core/src/metadata.rs:2883`) — is real as a conformance point but its blast radius is **one pass**: the row
  decoded during the resolve moments earlier, so reaching this arm needs a racing writer, and a *persistently*
  undecodable row is caught by `read_committed`'s resolve on the next pass (`crates/custodian/src/reconstruction.rs:481-494`),
  which contains the object and forces `Blocked`. It is not a silent-forever path.
- The `check-gates.json` row "run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red)" is a count
  of tests that **ran**, not that failed: I measured **2** discriminating legs (1 and 2), exactly as the brief
  predicted. Do not read the row as six red legs.

## Refutations attempted and failed

- **Tautology / mutation-proof check.** All four named negations are genuinely discriminating, verified by hand in the
  scratch copy: deleting the `chunk == prior` pin (`:2944`) turns leg 3 red and demonstrably reverts the racer's
  placement (`[0,7]` → `[0,2]`); dropping the root `require` (`:2916`) turns leg 4 red; deleting the extent check
  (`:2893`) turns leg 6 red; widening the segment ceiling to `MAX_VALUE_BYTES` (`:401`) turns leg 5 red. No leg passes
  for the wrong reason, and leg 2 proves the racing batch really lands inside the `scan_page`→`get` window (had it
  landed later, the seg-record CAS would have conflicted and leg 2 would fail).
- **Offset addressing vs. the old index addressing.** Tried to break the flat→offset change: `SegmentedMap::new`
  (`:891`) forces a contiguous tiling from 0 and `SegmentRecord::from_wire` (`:1193`) forces
  `sum(chunk.len) == byte_len`, so the reconstruction's accumulated offsets and the root's table can only agree.
  Duplicate ids, a chunk at a segment boundary, and the last chunk of the last segment all address correctly.
- **Serialization identity of the CAS precondition.** The segmented arm pins the seg row's *raw stored* bytes and
  re-encodes only the root, whose `Option` fields carry `skip_serializing_if` (`:1427-1440`), so decode→encode stays
  the identity — no permanent-conflict class introduced.
- **Two obligations inside one `seg:` record in one pass.** Probed directly (both `CHUNK` and `SIBLING` queued):
  `outcome=Changed placements=[[0,2],[0,2]] queued=[]` — the second move re-reads the record the first one just
  committed and merges, rather than losing the CAS the way two chunks of one flat record still do.
- **Stranded destination fragment on a lost race.** Not reproducible for the prepare-time conflicts: `repoint_chunk`
  runs *before* `put_fragment` (`crates/custodian/src/reconstruction.rs:878` vs `:898`), so legs 3/4/6 write no fragment at
  all; the remaining commit-time case is the tracked pre-existing leak (getwyrd/wyrd#723) and is settled.
- **Declined by the standing rubric:** the missing seeded Tier-0 DST case for this new concurrent write path is
  deferred to #722 by the brief and by the in-code `deferred: #682` markers (`crates/custodian/src/backfill.rs:112`),
  and deferrals are settled; the V/2 vs V ceiling choice was decided at Plan with both sides recorded.
