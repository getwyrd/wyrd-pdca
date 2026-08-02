# Adversarial review — issue 635 / segmented-chunk-map (iteration 11)

Attacked at `$PDCA_TARGET` = `/home/eddie/development/wyrd/wyrd.pdca-wt-l0` (patch applied in the
worktree, base `9120f7a`). Two findings were reproduced by **running the patched production code**
from a throwaway crate under `$PDCA_SCRATCH` (path-deps on `wyrd-core`/`wyrd-custodian`, since I
must not write into the read-only target); the scratch dir has been removed.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:311` (and the claim at `:302-304`): the new
  per-object containment covers only damage that still parses as JSON, so one *torn* `inode:`
  value blanks the drain surface for the whole fleet — the exact blast radius this patch says it
  bounds.** `metadata::decode` re-derives a typed `ChunkMapError` only when the bytes re-parse
  (`crates/core/src/metadata.rs:1875`, `serde_json::from_slice(bytes).ok()?`); a value truncated
  mid-token yields a bare `serde_json::Error`, the downcast at `gc.rs:307` misses, and
  `referenced_fragments` returns `Err` — which `reconciliation_status` propagates verbatim at
  `crates/custodian/src/desired_state.rs:174`, directly contradicting its own comment at
  `desired_state.rs:189` ("Never `Err`: this surface is read per D server, and one damaged object
  may not blank it for the whole fleet"). **Reproduced**: a store holding one healthy flat object
  plus `inode:2` = a valid segmented root with its last 30 bytes missing →
  `reconciliation_status(3) = Err(EOF while parsing a string at line 1 column 161)` and
  `reconciliation_status(0) = Err(...)`; `high_water_marks` stayed `Ok`. Truncation is the likeliest
  physical corruption of exactly the record classes this slice introduces (a 512-entry root is
  ~25 KB, a `seg:` value up to 50 KB), and the patch already argues the fix elsewhere: a `seg:`
  value that "cannot be read at all" is contained *because its key names one object*
  (`crates/core/src/metadata.rs:1822-1856`) — `inode:<id>` names one object just as precisely, so
  the asymmetry is unjustified by the patch's own reasoning. The test that would have caught it
  asserts the opposite: `crates/custodian/tests/segmented_map_consumers.rs:1181-1185` calls its
  three fixtures "the three spellings damage has", and all three are valid JSON. Fix at the
  `inode:` walk, or record-reject with the reason.

- **NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:4400-4403` vs `crates/server/src/lib.rs:124`:
  the entire chunk-id-floor half of `high_water_marks` defends a hazard that cannot occur on this
  tree, and the ~700 lines of new production code + ~1,300 lines of tests spent on it are the
  slice's largest unreviewable surface.** Every doc block on `raw_chunk_id_floor` (`:4431`),
  `scavenged_chunk_id_floor` (`:4510`), `scanned_chunk_id` (`:4570`), `widest_id_with_prefix`
  (`:4725`) and `segment_chunk_floor` (`:4782`) justifies itself with "a floor below a live id lets
  the allocator re-mint that id and clobber its fragments (issue #364)" — but `Gateway::recover`
  **discards** the value (`let (max_inode, _max_chunk) = …`, `server/src/lib.rs:124`) and the
  gateway mints coordination-free ids ≥ 2^127 (`server/src/lib.rs:238`, `:251`); a repo-wide grep
  finds no other reader of `max_chunk`. Only the *totality* half of the requirement is live (an
  `Err` there does stop startup). Worse, the semantics deliberately chosen for the dead value would
  themselves be the bug if it were ever wired: **measured** on the patched code, a single record
  torn at `{"size":8,"chunk_map":[{"id":1` yields `max_chunk = 18446744073709551615` = `2^64 - 1`
  (`widest_id_with_prefix`'s cap at `ceiling - 1`, `:4750`), i.e. one torn byte-range exhausts the
  whole in-process id space. This is a Plan-level call, not a build defect: the brief itself
  mandated the floor property (leg A(vii)(a), asserted at
  `crates/custodian/tests/segmented_map_consumers.rs:1194-1211`), so a human must decide whether to
  wire the floor to a consumer, or drop the recovery machinery and bound the claim to totality.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:307` and `:323`: both containment guards
  survive mutation (`replace match guard err.downcast_ref::<ChunkMapError>().is_some() with true`,
  `mutants.out/missed.txt`), so no test pins the rule the doc states at `gc.rs:289-291` — that a
  fault which is *not* the object's own still propagates.** Concrete missing case: a
  `MetadataStore` double whose `scan_page` returns `Err` for the `seg:` prefix must still make
  `referenced_fragments`/`reconcile_step` fail; with the guard widened it would instead be filed as
  "this object is unresolvable", after which `protects` (`gc.rs:268-273`) silently protects every
  fragment in the fleet and `reconciliation_status` answers `PendingUnresolvable` — a transient
  backend fault rendered as permanent per-object corruption, with GC quietly reclaiming nothing.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_consumers.rs:1260` discards the
  result of `reconcile_after_restore`, and on this very fixture that call returns `Err`, so leg
  A(vii)(d)'s "nothing was reclaimed" is bought by a pass that aborted rather than by the
  containment the leg exists to demonstrate.** `reconcile_after_restore` contains the fault in its
  first walk (`restore.rs:183` → the contained `referenced_fragments`) and then propagates it in
  its third (`restore.rs:309` → `committed_chunks`, `restore.rs:373-386`, which calls
  `resolve::chunks_of(...)?` with no downcast) — for all three damaged fixtures (`SegmentAbsent`,
  `SegmentRecordMalformed`, and the root that fails `decode`). Two seams of one pass disagree about
  containment and neither the discarded `let _ =` nor any other assertion notices. Either assert
  the pass's outcome explicitly (aborting is permitted by the brief's table — then say so in the
  test), or give `committed_chunks` the same containment as the walk above it.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:4450-4489`: the `serde_json`-parsed id reader
  is dead weight against the current fixtures — five mutants inside it survive** (`:4463` `<`→`>`,
  `:4467` `+=`→`*=`, `:4473` delete the `Array` arm, `:4485`/`:4486` delete both `json_chunk_id`
  arms). Every case the suite feeds it is also readable by `scavenged_chunk_id_floor`, so
  `raw_chunk_id_floor`'s `parsed.floor.max(scanned.floor)` (`:4437`) is decided by the scanner
  alone. Either delete the parsed reader (its stated advantages — quoted ids, escaped field names —
  are all handled by the scanner at `:4510-4544`) or add a value only it can read, so the
  redundancy is a tested claim rather than an asserted one.

## Attempted and could not refute

- **The red→green evidence.** `crates/custodian/tests/segmented_map_consumers.rs` names only
  base-visible symbols (`:53-73`), seeds the segmented object as raw record bytes (`:285-311`), and
  drives the real `reconcile_step` / `reconcile_after_restore` / `reconciliation_status` /
  `read_object` — not doubles of them. On the base a segmented `inode:` value cannot decode, so the
  red is an assertion failure, not a build error, as claimed. The legs are not tautologies: leg 2
  carries its mirror (a server holding nothing is `Satisfied`, `:817-826`), leg 4 is re-asserted
  standing alone so it cannot inherit leg 3's pass (`:716-780`), and the containment leg's
  `PendingUnresolvable` attribution has a typed mirror in `crates/custodian/tests/rebalance.rs:1592-1628`
  including the "with the object repaired, the same server certifies" arm.
- **Both iteration-10 carry-forwards are genuinely fixed**, not papered over: `plan_with` now
  refuses an empty placement before anything is durable (`crates/core/src/metadata.rs:3360-3365`),
  and `read_group_range` **refuses** rather than trusts a root's declared `segment_count` past
  `MAX_ROOT_SEGMENTS` (`:2567-2572`), with the refuse-vs-clamp reasoning stated at `:2543-2557`.
- I tried to break the resolver on ordering (it sorts by parsed `index`, `:2643-2654`), on epoch
  reuse (`seg_range_prefix` pins nonce **and** epoch, and `seg:<n>:1:` cannot prefix-match
  `seg:<n>:11:…`), on key-spelling smuggling (`parse_seg_key` at `:1582-1611` is canonical-decimal
  and fixed-width), on the complete-but-stale resolve (settled by the root re-read at `:2688`), and
  on flat byte-identity (`ChunkMap::Serialize` delegates the flat arm unchanged, `:1330`). None of
  these gave a failing case.
