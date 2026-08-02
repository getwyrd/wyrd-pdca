# Adversarial review — issue 635 / segmented-chunk-map (advisory; never gates)

Method: re-ran the asserted red→green from a pristine `git archive HEAD` of `$PDCA_TARGET`
plus the one added test file (base) against the patched worktree, in throwaway scratch
target dirs; then wrote three probe tests against the **patched production API** looking for
the input that breaks it. Two probes broke it.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5646` (and `:5659`): the id floor
  UNDER-approximates for a segmented root whose named `seg:` record is absent — the exact
  damage leg A(vii) is written about.** The `inode:` walk folds in chunk ids only for
  `ChunkMap::Flat` (`:5646`) and deliberately does not resolve a segmented root; the ids are
  recovered from the `seg:` namespace instead (`:5659`). A segment record the root *names*
  but that is not in the store is therefore seen by neither walk. Concrete, reproduced:
  a committed root naming segments 0 and 1, with `seg:<n>:11:000000` durable (chunk ids
  10, 11) and `seg:<n>:11:000001` absent (chunk ids 9 000 000/9 000 001, whose fragments are
  still on disk) ⇒ `high_water_marks` returns a chunk floor of **11**. That contradicts this
  module's own stated rule at `:5064-5072` ("The floor … is not allowed to under-approximate
  … an allocator resuming there re-mints 900 over fragments that are still on disk") and the
  brief's containment row ("must **never** under-approximate the floor"). The brief's *letter*
  ("≥ every chunk id present in any `seg:` record") is met, which is likely how the reviewer
  passed it. Mitigation the human should weigh: `Gateway::recover` discards the chunk half
  today (`crates/server/src/lib.rs:124`, `let (max_inode, _max_chunk)`), so this is latent,
  not live. Mechanical fix available without resolving any root: a committed segmented root
  whose group range yields fewer rows than `segment_count` contributes the same conservative
  `ceiling - 1` that `RecoveredIds::contribution` (`:5078`) already gives an unreadable record.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5162`: `RecoveredIds::complete` reads
  "no unreadable `id` field" as "every id this record names was seen", which is false for the
  very shape this slice adds.** A segmented root carries no `id` field at all — its ids live in
  `seg:` records — so `json_chunk_id_floor` returns `floor = 0, unreadable = 0, complete = true`
  and the conservative `ceiling - 1` arm (`:5078-5084`) never fires. Reproduced: an `inode:`
  value that is valid JSON, fails `decode` structurally (`segment_count: 9` vs one segment) and
  whose `seg:` records are absent ⇒ `high_water_marks` reports a floor of **0** for a record it
  admits it could not read. (Contrast: the same bytes made non-JSON *do* get the conservative
  contribution — so the safe path exists and this record class slips past it.) Same latency
  caveat as above. Note this sits in the code where 4 of the 12 surviving C5 mutants live
  (`:5192`, `:5198`, `:5244`, `:5454`) — the suite cannot see this region.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:307` and `:339`: nothing in the suite
  distinguishes a *store* fault from an *object* fault in `referenced_fragments`.** Both
  surviving mutants replace the `err.downcast_ref::<ChunkMapError>().is_some()` guard with
  `true` and stay green (C5 `missed.txt`). The module's contract is explicit that the two must
  differ (`:283-289`: "Anything that is **not** the object's own fault (a store error, an
  undecodable record) still propagates"), and the difference is operator-visible: under the
  mutant a transient backend I/O error on one `inode:` read is contained as
  `unresolvable`, so `reconciliation_status` answers `PendingUnresolvable { objects: [inode:N] }`
  — naming an innocent, healthy object as the thing to repair — while GC silently reclaims
  nothing. Missing test: a `MetadataStore` double whose `get`/`scan_page` returns a non-
  `ChunkMapError` error for one object, asserting `referenced_fragments` (and hence
  `reconcile_after_restore`, `reconciliation_status`) returns `Err`. Related, same class:
  `crates/core/src/metadata.rs:2084` — the `span != size` match guard mutates to `true`
  unseen, so `SizeSpanMismatch` attribution vs the `SegmentedRootMalformed` fallback is pinned
  by no test.

- **NEEDS-HUMAN [human] — `crates/custodian/src/gc.rs:269` + `crates/core/src/metadata.rs:2315`:
  one damaged segmented object halts **all** reclamation cluster-wide, and this slice ships no
  way to clear it.** `ReferenceSet::protects` answers `true` for every `(dserver, fragment)` in
  the fleet while `unresolvable` is non-empty, so after a single lost `seg:` row GC reclaims
  nothing anywhere — including the orphan-marked bytes of ordinary *flat* deleted objects,
  which then accumulate without bound — and `restore`'s strand-marking marks nothing. The
  brief's containment table does sanction "an incomplete reference set may not authorize any
  reclamation", which is presumably how the reviewer cleared it; what the table does not
  address is the **exit**: `unlink` refuses *any* segmented root (`metadata.rs:2315`,
  `SegmentedRetirementUnsupported`), so an operator cannot delete the damaged object to
  unblock the fleet, and no repair tool ships here. Note the blast radius is strictly wider
  than the precedent it cites: `PendingMalformed` protection is chunk-scoped
  (`gc.rs:271`), this one is fleet-scoped. A human should decide whether shipping the halt
  without an eject path is acceptable for this wave or wants a tracked follow-up.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:207`: an inode value is put with
  plain `metadata::encode`, bypassing the guarded `encode_inode`.** The patch's own claim
  (`crates/core/src/metadata.rs:1888-1893`, and the added prose in
  `docs/design/architecture/08-crosscutting-concepts.md`: "a record this system refuses to read
  is one no committer in it will store") holds only *inside* `core::metadata` — `encode_inode`
  is private, so `wyrd-custodian`'s committer cannot use it. Latent, not exploitable today
  (backfill only rewrites a `Flat` map, for which `checked_shape` is a no-op), but the
  invariant is asserted in shipped docs one crate wider than it is enforced.

## Attempted and could not refute

- **The red→green is real, and the red is assertions, not a build error.** Base
  (`git archive HEAD` + only `crates/custodian/tests/segmented_map_consumers.rs`) **compiles**
  and fails 9/9 on `Result::unwrap()` / decode assertions; the patched tree passes 9/9. This
  specifically rebuts the failure mode the brief itself flags (`run-verify.sh:416-433` falling
  through to an unconditional PASS when the RED leg fails to build): it did not happen. The
  test file names no symbol this slice adds (checked: `ChunkMap`, `SegmentRef`,
  `PendingUnresolvable`, `resolve_*`, `repoint_chunk` all absent), and the leg A(ii) drain
  assertion is discriminating, not tautological — the segmented and flat fixtures sit on
  disjoint halves of the fleet, so `Pending` can only come from reading the `seg:` range.
- **Production-scale publish → resolve round trip.** 3 000 chunks through
  `SegmentedPublication::publish` with the *real* constants produced 6 segments; every `seg:`
  value stayed inside the 100 KB ceiling, `resolve_chunk_map` returned the chunk list in exact
  input order, and `high_water_marks` covered every live id. No off-by-one in `plan_with`'s
  budget arithmetic or `SEGMENT_ENVELOPE_BYTES`.
- **Re-publication over a live segmented generation** (same `(nonce, epoch)`, shorter chunk
  list) is refused (`SegmentedRetirementUnsupported`) with the live map intact — no hybrid
  range, no torn map.
- Order-independence of the resolver (`metadata.rs:2850-2880` sorts by *parsed* index into a
  `BTreeMap`, and the `Shuffling` double at `:9818` proves the fixture really shuffles);
  epoch-prefix disjointness (`seg:<n>:7:` vs `seg:<n>:70:`) and `seg:` vs `seggrp:`
  non-overlap; `batch_ranges` (`:4928`) never emits an empty range, so
  `assemble_segment_batches`' `pending[range.end - 1]` cannot underflow; flat
  decode→encode identity (`ChunkMap::Serialize` passes the array through unchanged);
  every `.chunk_map` consumer routed through the shared resolver (grep leaves only shape
  decisions and tests). Rebalance/reconstruction/backfill propagating a damaged object's
  fault with `?` is **explicitly sanctioned** by the brief's containment table
  (deletion-capable class: "aborting the pass is acceptable"), so I did not score it.
