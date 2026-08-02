# Adversarial review — issue 635 / segmented-chunk-map

Attacked: (1) the asserted red→green base, (2) the staged-publication committer's inputs,
(3) the resolver's fail-closed blast radius, (4) the leg-B oracles. Two attacks landed with
a reproduced failing case; two more are conformance-level. What I tried and could **not**
refute is listed at the end.

## Findings

- **NEEDS-HUMAN [impl] — the committer accepts a same-epoch re-publication with a shrunk
  plan, and that permanently bricks the published object.** `SegmentedPublication::flip_batch`
  (`crates/core/src/metadata.rs:2294`) requires only the root's prior bytes; neither
  `write_segments` (`:2311`) nor `publish` (`:2338`) deletes, or requires the absence of,
  segment keys past the current plan's last index. `read_segments` then fails closed on any
  index `>= segment_count` (`crates/core/src/metadata.rs:1775`). **Reproduced** against the
  patched tree (probe test in a throwaway copy, `cargo test -p wyrd-core --lib`): publish
  1 500 chunks at `group(nonce,7)` → 2 segments, `Committed`; then `publish` again at the
  **same** group/epoch with 20 chunks, superseding the just-published root → also
  `Committed`; the store still holds `seg:…:7:000001` from attempt 1 while the new root names
  one segment. Result, verbatim:
  `resolve of the published object => Some("segment 1 exists under seg:0123456789abcdef0123456789abcdef:7 but the root does not name it")`.
  The object is now unreadable **forever** — no code path in this slice can delete the stray
  key. This is not a hypothetical call shape: the patch's own DST property
  (`crates/dst/tests/custodian.rs`, "*the recovery re-runs the WHOLE publication at the same
  epoch*") establishes same-epoch re-publication as the sanctioned `CommitUnknownResult`
  recovery, and it is asserted only for an *identical* chunk list. Leg B(v) only covers the
  cross-epoch disjointness case (`seg:<n>:1:*` vs `:2:*`), so nothing in the bundle exercises
  the same-epoch shrink. Fixable by iterating: have `flip_batch` add
  `require_absent(seg_key(&group, plan.len() as u32))` (and/or delete the tail), plus the
  missing test.

- **NEEDS-HUMAN [impl] — one stray segment record now fails gateway startup and halts the
  whole maintenance plane, not just the affected object.** In the same probe run,
  `high_water_marks` — the function `Gateway::recover()` calls before serving
  (`crates/server/src/lib.rs:124`) — returned the same error store-wide:
  `high_water_marks => Some("segment 1 exists under seg:…:7 but the root does not name it")`,
  because the patch added an unconditional `resolve_live_chunk_map(...)?` inside its `inode:`
  walk (`crates/core/src/metadata.rs:2480`). So the node **never starts**. The identical `?`
  at `crates/custodian/src/gc.rs:265` propagates out of `referenced_fragments` →
  `gc::reconcile` → `reconcile_step`, so GC, scrub, rebalance, reconstruction and
  `reconcile_after_restore` stop for **every** object in the store, and
  `reconciliation_status` stops answering for every drain. Fail-closed on a torn map is the
  right *direction* (rubric: *Absent or unsupported entries*), and a corrupt `inode:` value
  already halted GC before this patch — but the new producer is a committer this slice ships,
  the reach is now cluster-wide from one record, and neither blast radius has a test or an
  operator-visible signal: `resolve.rs`'s emitters cover only the `None`/restart arms
  (`crates/custodian/src/resolve.rs:74`,`:86`), never the `Err` arm. Note the aggravating
  detail: `recover()`'s only use of the scan is `max_inode`; it discards `_max_chunk`
  (`crates/server/src/lib.rs:124`), so the resolve that can refuse the boot is done for a
  value the caller throws away.

- **NEEDS-HUMAN [human] — the red→green and `xtask ci` rows were earned on a base that is
  not the brief's normative base, and the carried-forward blocker is unaddressed.** The brief
  declares the base `origin/main` **plus #634** and instructs, verbatim, that the in-test
  double "must implement `scan_page` (#634's required method, one delegating line)"
  (brief.md, *Falsifiability* corollary); iteration 1 was rejected for exactly this
  ("*Rebuild after adding the stack-base method … `crates/custodian/tests/segmented_map_consumers.rs:77`*",
  brief.md:487). On `origin/enhancement/634-scan-page-seam`, `MetadataStore::scan_page` is
  **required with no default body** (deliberately — "*this method is required*"). This patch
  adds five `MetadataStore` impls, none of which has a `scan_page`:
  `crates/custodian/tests/segmented_map_consumers.rs:83`, `crates/core/src/metadata.rs:3329`,
  `:3621`, `:3857`, `crates/server/src/lib.rs:882`. `scan_page` appears **nowhere** in the
  target tree, and `origin/pdca-integration/main` does not resolve in this checkout (`git
  rev-parse` fails; both wave worktrees sit on plain `main` @ `b0cd199`). So on the normative
  base every one of those five is `E0046` — `cargo xtask ci` would not build and C4-verify's
  RED leg would be a *build* error, which the brief itself warns prints "PASS — red without
  the fix" over a run that executed nothing. On *this* base the red is genuine (I traced the
  file: it names only base symbols, and a segmented `inode:` value makes `metadata::decode`
  fail so `gc.rs`'s `?` propagates into leg A(i) as an assertion failure). This is a human
  call, not a builder iteration: on the un-folded base the required `scan_page` line **cannot
  compile**, so the fix is to fold #634 and re-run, or to accept the C4 rows as
  base-provisional. Marked per issue #236 — my verdict on stack-green is provisional.

- **NEEDS-HUMAN [impl] — `segment_group_adopted` takes an unvalidated `&str` nonce, so the
  predicate #636 will gate marker deletion on can silently answer "not adopted" for a live
  group.** `crates/core/src/metadata.rs:2084` (`nonce: &str`), over `seg_group_prefix(nonce)`
  (`:943`) and `seggrp_key(nonce)` (`:948`) — all three bypass `SegmentGroup::new`, the
  validating constructor the module introduced precisely so "a nonce that could not key a
  reproducible `seg:` range is never representable" (`:566-580`). Concrete case: a caller that
  passes an uppercased or truncated nonce (`"0123456789ABCDEF…"`, or the first 16 chars) gets
  `Ok(false)` — the scan prefix simply matches nothing — and #636's two-arm lifetime rule
  then deletes the `seggrp:` marker of a group whose segments are live, re-opening the nonce
  for reuse. Reuse overwriting a live object's segment records is the exact hazard
  iteration-14 finding 2 introduced the nonce to prevent (`0016:499-527`, quoted in the
  type's own doc at `:561`). Cheap fix: take `&SegmentGroup` (or return
  `Result<_, ChunkMapError>` via `SegmentGroup::new`), and add the negative case — the
  bundle's only test here (`segment_group_adoption_is_one_bounded_range_read`,
  `crates/core/src/metadata.rs:3970`) passes the valid `NONCE` both times.

## Attacked and could not refute

- **The red→green evidence itself, on the base it ran on.** Every symbol
  `crates/custodian/tests/segmented_map_consumers.rs` names exists pre-patch, the fixture is
  raw record bytes (no new symbol), the segmented value is un-decodable pre-fix, and the
  observables are positive (`Pending`, `stranded_marked == 0`, fragments still present, bytes
  byte-identical) rather than "no error". Leg 4 genuinely stands alone (`:585`), and the
  backfill leg is driven by the one discriminating input — an empty `placement` inside a
  segmented map (`:376-389`) — so it cannot pass by short-circuit. I could not construct a
  way for these to pass pre-fix.
- **Consumer completeness.** I grepped every `chunk_map` reader under `crates/*/src` and found
  none left walking the field: the eight tabled sites plus two the brief's table omits —
  `high_water_marks` (`crates/core/src/metadata.rs:2480`) and `read_object_from`, which now
  refuses explicitly (`crates/core/src/read.rs:72`) instead of returning a short read.
- **Flat-path regression.** `resolve_chunk_map` returns a borrow for `Flat` before touching
  the store (`:1822`), so every flat consumer's read count, CAS shape and byte-identity are
  unchanged; `legacy_flat_records_round_trip_byte_identically` + the end-to-end
  `commit_chunk_map` CAS test pin the `skip_serializing_if`/CAS contract.
- **Publication envelope arithmetic.** I checked `SEGMENT_ENVELOPE_BYTES = 80` against the
  widest `u64` rendering (exactly 80), `SEGMENT_TARGET_BYTES` vs `MAX_VALUE_BYTES`, and
  `MAX_ROOT_SEGMENTS = 512` against 0016's derivation (`0016:2432-2440`, 312–520) and its
  constraint `max_segref_bytes × MAX_ROOT_SEGMENTS ≤ V/2` (`0016:1467`): the real worst-case
  `SegmentRef` encodes to ~88 B, so 512 × 88 = 45 056 ≤ 51 200 holds with margin even after
  the root's other fields. I tried to breach it via the unbounded client `content_type` and
  could not reach the 100 KB hard ceiling within HTTP header limits.
- **Repoint safety.** `ChunkHome::Segment` CASes the *raw bytes the resolve read*, not a
  re-encode, so it is strictly stronger than the flat `require(encode(prior))`; both callers
  only ever rewrite `placement`, so the `byte_len == Σ chunk.len` invariant that decode
  enforces cannot be broken by a repoint. `require(inode == prior)` on the segment arm closes
  X47 and is asserted on both interleavings in DST.
- **Scan-order independence.** `read_segments` keys a `BTreeMap` on the *parsed* index
  (`:1739-1758`), so the reversing `Shuffling` double is a real oracle, not decoration; the
  nonce/epoch re-check rejects a neighbour row even inside the bounded prefix.
