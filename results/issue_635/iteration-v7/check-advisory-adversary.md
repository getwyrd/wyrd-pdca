# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Attacked: the leg-A red→green claim, the staged-publication guard that carry-forward item 3
added this round, the containment table's totality row, and the resolver's fail-closed
arms. Two refutations land, two are judgment calls for the human. Findings are grounded on
the target tree at `$PDCA_TARGET` (patch applied in the worktree).

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3010` (`verify_resume_prefix`) with
  `:2129` (`read_segments`' `SegmentUnknown` arm): a same-epoch republication whose
  re-derived plan is SHORTER than a previous attempt's publishes a `Committed` root that no
  consumer can ever resolve.** The guard added this round verifies only `planned[..resume_from]`
  (`:3037`) and short-circuits entirely at `resume_from == 0` (`:3011`); nothing anywhere in the
  committer deletes `seg:<nonce>:<epoch>:<i>` rows for `i >= planned.len()` — `assemble_segment_batches`
  (`:2875`) and `flip_batch` (`:2931`) emit puts only — and the caller cannot delete them
  either, because `merge_contribution` refuses any contribution that names the group's range
  (`:3299`, `OwnedKeys::owner_of` `:3171`). `read_segments` then fails closed on the orphaned
  tail (`:2129`), permanently. **Reproduced** against the patched tree (scratch crate over
  `wyrd-core`, in-memory `MetadataStore`): attempt 1 `write_segments` a 1200-chunk list (2
  segments) at `(nonce, epoch=1)` and crash before the flip; attempt 2 `publish` a 300-chunk
  list (1 segment) at the *same* `(nonce, epoch)` with `resume_from = 0` →
  `publish` returns `Ok(Committed)`, and afterwards
  `resolve_chunk_map` / `resolve_live_chunk_map` both return
  `segment 1 exists under seg:0123…:1 but the root does not name it` — every read, GC,
  restore, rebalance and reconstruction pass over that store now fails, forever. This is
  exactly the "silent at publication, terminal at read" shape leg B(v) / carry-forward item 3
  exists to prevent; the patch closed the *differing-prefix* half of it and left the
  *shorter-plan* half open. Cheapest fix in the same place: after building `planned`, refuse
  (or delete in the flip batch) any durable index `>= planned.len()` in the group's own range —
  `verify_resume_prefix` already has that range in hand at `:3027`.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3559-3560`: the totality claim the
  containment table turns on is unwarranted as written.** The doc asserts "No arrangement of
  store contents makes it return `Err`" (echoed verbatim in
  `docs/design/architecture/06-runtime-view.md:29`, "That floor is **total**: no arrangement of
  stored records can make it refuse"), but three of the four walks in the body are unpaged
  `scan`s that fail loud at `SCAN_CAP` (`crates/traits/src/lib.rs:286`, `1 << 20`):
  `:3571` (`inode:`), `:3617` (`pending:`), `:3628` (`orphan:`). Concrete case: a store with
  more than 1 048 576 inode records — or an orphan ledger past the same cap, and this patch's
  own comment at `:1664` prices one segmented retirement at "~1.78 M fragment orphans" —
  makes `high_water_marks` return `Err(ScanCapExceeded)`, so `Gateway::recover`
  (`crates/server/src/lib.rs:123-124`) refuses to start and every healthy object loses
  availability. That is precisely the blast radius the containment row was written against.
  The `seg:` half was correctly paged (`segment_chunk_floor`, `:3490`) for exactly this
  reason, which makes the remaining three the inconsistency. The scans are pre-existing; the
  absolute claim is new, so the minimum honest fix is to bound the claim ("total against an
  undecodable record") or to page the other three the way `:3490` already shows.

## Judgment calls

- **NEEDS-HUMAN [human] — `crates/server/src/lib.rs:448` + `:479`: the ranged read resolves
  the entire map, discarding the root's own byte index.** The settled encoding gives every
  `SegmentRef` a `byte_offset`/`byte_len` precisely "so the root alone answers 'which segment
  covers byte N' without reading any segment record" (brief, *The settled record encoding*).
  `get_object_range` instead calls `resolve_live_chunk_map` (`:448`) and only then selects
  covering chunks (`:479`). At the shipped ceilings (`MAX_ROOT_SEGMENTS = 512`,
  `SEGMENT_TARGET_BYTES = 50 000`, `metadata.rs:288`,`:330`) a 1-byte `Range:` GET on a
  maximal object reads ~25 MB of `seg:` records and materialises a ~190 000-element
  `Vec<ChunkRef>` before sending a byte. Correct, but the affordance the design added for it
  is unused; whether a range-scoped resolver belongs here or in #508 is a scope decision, not
  a builder nit.

- **NEEDS-HUMAN [human] — `crates/custodian/src/gc.rs:265` (via
  `crates/core/src/metadata.rs:2181`): one damaged segmented object halts the whole
  maintenance plane indefinitely, not just itself.** `chunks_of`'s `?` propagates
  `SegmentAbsent` out of `referenced_fragments`, which is the single reference build behind GC
  (`gc.rs:132`), restore (`restore.rs:183`), scrub (`scrub.rs:75`) and `reconciliation_status`
  (`desired_state.rs:157`) — so until an operator repairs that one record, *no* object's
  garbage is ever reclaimed anywhere in the store and no drain can ever be certified. The
  brief's containment table pre-authorises this ("Aborting the pass is acceptable"), and the
  brief's own invariant ("Its failure, when it does fail, is scoped to the object that
  failed") argues for the other permitted shape (continue, treating the damaged object as
  fully referenced). Cheap confirm-or-redirect at sign-off; note also that leg A(vii)(d)
  (`crates/custodian/tests/segmented_map_consumers.rs:1171-1188`) discards both pass results
  with `let _ =`, so it cannot distinguish "protected" from "aborted before doing anything".

- **NEEDS-HUMAN [impl] — the T4 gate is red for the sixth round on the same cause**
  (`check-gates.json`: "review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped"), and
  the brief's carry-forward item 5 gives the exact disposition and format for five of them
  (record-reject in `review-rejected.md` as `<file:line> | <CLASS> | <MATCH> | <reason>`,
  citing the flat peer `crates/custodian/src/rebalance.rs:274-296`, maintainer-confirmed at
  Plan). Not a diff finding — raised because it is mechanically dischargeable by the builder
  and is the sole gating failure in the bundle.

## Attempted and could not refute

- **Flat byte-identity (leg B(i)).** `ChunkMap::serialize` delegates the flat arm straight to
  `Vec<ChunkRef>` (`metadata.rs:1063`) and `#[serde(try_from = "InodeRecordWire")]`
  (`:1362`) affects only `Deserialize`, so decode→encode on a legacy record is unchanged; the
  `skip_serializing_if` CAS rationale survives.
- **Scan-order independence.** `read_segments` keys a `BTreeMap` on the *parsed* index
  (`:2085`,`:2098`) and the `Shuffling` double (`:5717`) returns every scan reversed.
- **Epoch/nonce scoping (F18).** `seg_range_prefix` pins both (`:1270`), `read_segments`
  re-checks them per row (`:2091`), and `parse_seg_key` rejects non-canonical epochs and
  non-fixed-width indices (`:1300`,`:1311`) — I could not smuggle a second spelling of one
  segment key past it.
- **Repoint identity.** `repoint_chunk`'s segmented arm binds the home to the root's
  generation and to a named index before building either precondition (`:2470-2485`), and
  `SegmentRecord::repoint` makes id/scheme/len unspellable (`:1178`).
- **Leg A's red.** I could not re-run `run-verify.sh` here, but the file names only base
  symbols (imports at `crates/custodian/tests/segmented_map_consumers.rs:53-73`) and a
  segmented value is a JSON object where the base's `chunk_map: Vec<ChunkRef>` demands an
  array, so every leg's red is an assertion/`Err`, not a build error — the claim checks out on
  inspection.
