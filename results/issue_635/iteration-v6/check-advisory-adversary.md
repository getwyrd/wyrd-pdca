# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Method note: I did **not** re-execute the red→green. `cargo` is present, but `$PDCA_TARGET`
is read-only (its 63 GB `target/` is the only warm cache) and a cold rebuild of the
workspace in scratch was disproportionate to the budget. Everything below is grounded by
reading the target source at `$PDCA_TARGET` with the patch applied; where a claim depends
on execution I say so. This is a *scope* choice, not a missing toolchain, so it does not
change any verdict below.

## Refutations

- **NEEDS-HUMAN [impl] — `high_water_marks` is still not total against the record class this
  patch introduces, so the containment leg proves less than it claims.**
  `crates/core/src/metadata.rs:3461` still decodes every `inode:` value with a strict `?`,
  while `crates/core/src/metadata.rs:1421-1428` (`SizeSpanMismatch`) and
  `SegmentedMap::new`/`SegmentGroup::new` (`:900-925`, `:828-838`) make a whole family of
  *new* segmented-root shapes undecodable. Concrete failing case: store
  `inode:4 = {"size":1,"chunk_map":{"group":{"nonce":"<32 hex>","epoch":1},"segment_count":1,
  "segments":[{"index":0,"byte_offset":0,"byte_len":2}]},"state":"Committed","version":1}`
  (size 1, table spans 2 — or equally an uppercase nonce digit, or a `segment_count` that
  disagrees). `high_water_marks` returns `Err`, `Gateway::recover`
  (`crates/server/src/lib.rs:124`) returns `Err`, and every *healthy* object in the store
  loses availability — the exact blast radius the brief's containment table calls
  impossible ("No arrangement of store contents may make it return `Err`"). Leg A(vii)(a)
  (`crates/custodian/tests/segmented_map_consumers.rs:1107`) and the co-located
  `the_id_floor_is_total_over_a_damaged_segmented_object`
  (`crates/core/src/metadata.rs:6442`) only seed the one damage mode where the root itself
  decodes cleanly, so both pass while the property fails. Not a regression from base (a
  segmented value fails `decode` there too) — but establishing this property is the *only*
  reason leg A(vii)(a) exists, and it is not established. The fix is mechanical and
  provably lossless here: `recover` consumes only `max_inode`, which is parsed from the
  **key** at `:3458`, so skipping-and-attributing an undecodable `inode:` value — the shape
  the patch already chose one loop down at `:3397` — costs the floor nothing.

- **NEEDS-HUMAN [human] — the new full-namespace `seg:` walk feeds a value no production
  caller reads.** `segment_chunk_floor` (`crates/core/src/metadata.rs:3389-3419`) pages
  through and JSON-decodes **every** `seg:` record in the store on each call. Its only
  consumer is `high_water_marks`'s `max_chunk`, and the only production caller of
  `high_water_marks` discards it: `crates/server/src/lib.rs:124` binds `_max_chunk` (also on
  base — chunk ids are coordination-free per ADR-0019, as the doc comment there states).
  So this slice adds, to every gateway start, a decode of the entire segment corpus
  (~50 KB per record, up to `MAX_ROOT_SEGMENTS`=512 records per 10 GiB-class object —
  ~25 MB per object, per start) whose result is dropped, and leg A(vii)(a)'s "floor ≥ every
  `seg:` id" assertion (`crates/custodian/tests/segmented_map_consumers.rs:1120`) pins a
  number nothing consumes. Related: the brief's containment row also demands the floor
  "never under-approximate", yet `:3397` skips an unreadable `seg:` record and the
  co-located test *enshrines* that departure (`crates/core/src/metadata.rs:6503-6517`)
  without a `review-rejected.md` entry. Human call: keep the floor (and record the
  under-approximation rejection), or stop computing `max_chunk` and delete the walk.

- **NEEDS-HUMAN [impl] — `reconciliation_status` gets the wrong containment arm, and nothing
  tests it.** The brief's containment table settles this row as the `PendingMalformed`
  shape ("refuse to certify, **attribute** the blocker, keep going"). As shipped,
  `crates/custodian/src/desired_state.rs:157` calls `referenced_fragments`, which propagates
  the resolver's `Err` at `crates/custodian/src/gc.rs:265`. Concrete failing case: seed the
  damaged object of `seed_damaged` (`crates/custodian/tests/segmented_map_consumers.rs:457`)
  plus `desired:dserver:5`, then call `reconciliation_status(meta, 5)` for a server that
  holds nothing of the damaged object — it returns `Err` instead of
  `PendingMalformed { chunks }`, i.e. the operator's drain surface goes dark **store-wide**
  for every server because one object is damaged, and the blocker is never attributed to an
  inode. No test covers it: leg 2 (`:707`) runs on a healthy store only, and leg 7 (`:1098`)
  never calls `reconciliation_status`. Either implement the row (an "unresolvable" bucket
  beside `ReferenceSet::malformed`, `gc.rs:241-247`) or record-reject it with a reason.

- **NEEDS-HUMAN [impl] — leg A(vii)(d) is vacuous: it passes on the pre-fix base and cannot
  fail.** At `crates/custodian/tests/segmented_map_consumers.rs:1171-1181` both passes are
  invoked as `let _ = reconcile_after_restore(...)` / `let _ = reconcile_step(...)`. With the
  damaged object present, `referenced_fragments` (`crates/custodian/src/gc.rs:265`) and
  `committed_chunks` (`crates/custodian/src/restore.rs:375-386`) both return `Err` at the
  *first* segmented record, so no pass ever reaches a reclamation decision — and on the base
  the same call errors at `metadata::decode`. The subsequent "fragments still present"
  assertions (`:1183-1197`) therefore hold for any implementation that errors anywhere, and
  contribute nothing to the red. As written the leg cannot distinguish "an incomplete
  reference set authorized no reclamation" from "nothing ran at all". Pin the arm the pass
  actually took (assert the `Err` type, or assert that a store *without* the damaged object
  does reclaim on the same fixture) so the assertion has a way to fail.

- **NEEDS-HUMAN [impl] — the C5 survivors are in the loop this patch restructured and are a
  one-line fixture away.** All three missed mutants sit on
  `crates/core/src/metadata.rs:3473` (`chunk.id < IN_PROCESS_CHUNK_CEILING`, now nested
  under the new `if let ChunkMap::Flat` at `:3471`): `<` → `>`, `==`, `<=` all survive, i.e.
  no test in the suite pins that a **flat** record's sub-2^64 chunk id raises the floor
  while an above-2^64 one does not. `the_high_water_scan_sees_segmented_ids_and_ignores_the
  _out_of_range_ones` (`:6362`) covers the segmented half only. A single record carrying one
  id below and one above the ceiling closes it.

- **NEEDS-HUMAN [human] — a ranged GET now reads the whole map, and the encoding's stated
  reason for `byte_offset`/`byte_len` is unused.** `get_object_range`
  (`crates/server/src/lib.rs:447-479`) resolves the *entire* chunk list through
  `resolve_live_chunk_map` before trimming to the requested span, so a 1-byte
  `Range: bytes=0-0` on a max-size segmented object reads all 512 `seg:` records
  (~25 MB of metadata) to return one byte. The root's segment table exists precisely so
  that "which segment covers byte N" is answerable without reading a segment record (brief,
  *Design § the settled record encoding*), but `SegmentRef::byte_offset`/`byte_len` are read
  nowhere except the validation checks at `crates/core/src/metadata.rs:908` and `:2111` — no
  consumer selects by span. The brief did mandate the ranged walk go through the one
  resolver, so this is a fitness-to-purpose call for sign-off (resolver API gains a byte-range
  arm now, or the amplification is accepted and tracked), not a builder slip.

## Attempted and could not refute

- **Flat byte-identity (the CAS contract).** `Serialize for ChunkMap`
  (`crates/core/src/metadata.rs:1057-1064`) emits `Flat` as the bare array, `InodeRecord`
  keeps its derived field order and `skip_serializing_if` (`:1358-1398`), and the new
  `InodeRecordWire` (`:1403-1415`) is field-for-field identical with no
  `deny_unknown_fields`. I could not construct a legacy record whose decode→encode moves.
- **A `.chunk_map` consumer left un-routed** (the recorded #508-attempt-4 failure). A
  workspace-wide grep leaves no production reader of the field outside the resolver except
  `backfill.rs:163`'s deliberate `is_segmented()` skip and `write.rs:274`'s construction.
- **Fooling the bounded range read.** Nonce/epoch prefix confusion is blocked by the fixed
  32-char nonce plus the trailing `:` in `seg_range_prefix` (`:1267`); a foreign row inside
  the range is rejected at `:2088`; ordering never trusts `scan` (BTreeMap keyed by the
  parsed index, `:2082`,`:2147`); an index past the table fails closed at `:2126`.
- **Writing a `seg:` record before a deterministic refusal** (iteration-5 refutation 1).
  `publish` assembles both phases before any I/O (`:3069-3070`) and `verify_resume_prefix`
  is the only pre-write read (`:2997-3026`); I found no path from `publish` that commits a
  segment ahead of an `Unfenced`/`ContributionCollides`/ceiling/envelope refusal. (Note only:
  `write_segments` (`:2971`) does not validate the flip, so a caller composing the two phases
  by hand re-opens that order — out of this diff's control.)
- **Retirement/supersede races.** `root_still_names` (`:2153`) settles a flat, absent or
  re-grouped root as `Retired`; `resolve_live_*` re-resolve the replacement rather than
  answering "no chunks" (`:2252-2281`, `:2345-2374`); `repoint_chunk` binds the home's group
  to the live root before building either precondition (`:2463-2482`). I could not build an
  interleaving that yields a torn map or a silently-lost repoint.
