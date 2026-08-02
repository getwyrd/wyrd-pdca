# Adversarial review — issue #635 (segmented-chunk-map)

Advisory only; the human decides at sign-off. Every citation is against the target working
tree at `$PDCA_TARGET` (patch applied on `9120f7a`).

## Refutations

- **NEEDS-HUMAN [impl] — the containment rule is keyed on the error *type*, and two of the
  three ways a damaged segmented object presents are not that type, so one damaged object
  still blanks the drain surface fleet-wide.** `crates/custodian/src/gc.rs:309` contains an
  unresolvable map only when `err.downcast_ref::<ChunkMapError>().is_some()`; everything else
  re-raises at `:315`. But the decode-time invariants this slice adds surface as
  `serde_json::Error`, never as `ChunkMapError`: `SegmentRecord`'s `byte_len` check goes
  through `D::Error::custom` (`crates/core/src/metadata.rs:1331`), the root's size-vs-table
  check through `try_from` (`:1511`), and `metadata::decode` boxes the serde error as-is
  (`:1587`). Concrete failing case, a one-line edit to the patch's own fixture at
  `crates/custodian/tests/rebalance.rs:1538-1557`: publish the damaged root with `size: 9`
  against an 8-byte segment table (or leave `size: 8` and seed
  `seg:0123…:7:000000` = `{"chunks":[],"byte_offset":0,"byte_len":8}`, valid JSON that fails
  `SegmentRecord`'s own decode at `metadata.rs:2181`). Then `reconciliation_status(&meta, 3)`
  returns **`Err`** — for *every* D server in the store — instead of `PendingUnresolvable`.
  That is verbatim the outcome `gc.rs:296-301` says must not happen ("one damaged object would
  blank the drain-status surface fleet-wide") and the outcome the brief's containment table
  forbids for the `reconciliation_status` row. `high_water_marks` contains exactly these bytes
  correctly (`metadata.rs:4097`, `:4213`), so the patch is internally inconsistent about the
  same damage. Leg A(vii) and `a_drain_stays_blocked_and_attributed_while_a_map_cannot_be_resolved`
  only ever exercise the **absent-record** spelling, which is the one spelling that *is* a
  `ChunkMapError` — so the green is real but narrower than the claim it is offered for.

- **NEEDS-HUMAN [impl] — the `seggrp:<nonce>` reservation the brief calls "the corrective
  rule" is never written by anything, and the committer forbids the caller from writing it.**
  `SEGGRP_MARKER` (`crates/core/src/metadata.rs:282`) has no production writer: `flip_batch`
  (`:3114-3126`) and `assemble_segment_batches` (`:3052-3079`) emit only the root CAS and the
  `seg:` puts, while `owned_keys` (`:3087`) claims the marker key so a caller's contribution
  that puts it is refused with `ContributionCollides` — pinned as *intended* behaviour by the
  co-located test at `:5197`. So `require_absent(seggrp:<nonce>) + put(marker)` is unreachable
  through the shipped API in either direction, yet `:899-900` and `:3441-3443` both document
  the guard as if it existed. It is not a marked deferral (contrast the explicit
  `deferred: #636` markers at `:2566`, `:2577`), and the brief settles it here
  ("**Implement the corrective rule:** … reserved by `require_absent(seggrp:<nonce>)` plus the
  marker record"). Concrete failing case: publish object A under group `(N,7)` with 2 segments,
  then publish object B under the *same* `(N,7)` with 3 segments and `resume_from: 0`.
  `verify_durable_range` compares nothing (`required = claimed = 0`, `metadata.rs:3269-3273`)
  and the tail check at `:3257` passes because both durable indices are `< 3`, so phase 1
  overwrites A's segment records; A's root still names `(N,7)`, and A resolves to
  `SegmentUnknown` — permanently unresolvable — or, if B's byte extents coincide, to **B's
  chunks**, which is the "hybrid map … nothing downstream ever notices" the patch's own doc at
  `:3203-3211` calls worse than the unresolvable one. Note the fix has a wrinkle the builder
  must handle: a resumed attempt must not re-require the marker absent.

- **NEEDS-HUMAN [impl] — the id-floor's "two readers" rationale is asserted in prose and by no
  test; the C5 row is pointing at exactly that.** `raw_chunk_id_floor`
  (`crates/core/src/metadata.rs:3797-3830`) justifies a JSON-parsing reader beside the byte
  scanner, but all 11 surviving mutants land inside it: deleting *both* arms of `json_chunk_id`
  (`:3873`, `:3874`) and flipping `id < ceiling` to `id > ceiling` (`:3851`) are all unnoticed,
  i.e. the whole parsed reader can be disabled and the suite stays green because the scanner
  covers every input the tests use. Either add the case only the parse can read (and pin it),
  or drop the reader — this is code on `Gateway::recover`'s startup path whose stated purpose
  is not to under-approximate the floor (#364), and right now nothing would notice if it
  stopped contributing.

## Attempted and could not refute

- The round-8 findings appear genuinely fixed, not papered over: `flip` now verifies
  `DurableRange::WholePlan` (`metadata.rs:3346-3350`), `check_fence_transitioned` refuses *any*
  put that restores a pinned value (`:3576-3585`), and `widest_id_with_prefix` caps each range
  at `ceiling - 1` instead of walking the nines — traced by hand at prefix `18`, ceiling `2^64`,
  it returns `18446744073709551615` (`:4037-4063`).
- Flat byte-identity: `InodeRecord`'s field order and `ChunkRef`'s encoding are untouched by
  the diff (only `chunk_map`'s *type* changed, `metadata.rs:1453`), and `ChunkMap`'s
  discriminate-by-JSON-type `Serialize` (`:1144-1150`) re-emits a flat map as the bare array,
  so `require(key, encode(prior))` still matches stored legacy bytes.
- Key-range ambiguity: `seg:<nonce>:<epoch>:` cannot alias a neighbouring epoch or group —
  the nonce is fixed-width hex, the epoch is canonical decimal (`parse_canonical_u64`,
  `:1405-1417`) and the index is fixed width, all rejected strictly in `parse_seg_key`.
- Repoint identity: `SegmentRecord::repoint` takes a placement vector, not a `ChunkRef`
  (`:1262-1272`), and `repoint_chunk` binds the home's key to the live root's group before
  building either precondition (`:2627-2646`) — I could not construct a repoint that changes a
  chunk's id/len or lands in a retired generation's record.
- Reader restart framing: `read_object` and both gateway paths take `size`/ETag from the
  generation the chunks came from, not the stale snapshot (`core/src/read.rs:520`,
  `server/src/lib.rs:366`, `:371`, `:453-465`).
- Restore/GC agreement: both walk the same `resolve::chunks_of` and both gate deletion on the
  single `ReferenceSet::protects` predicate (`gc.rs:269`, `restore.rs:222`), so I could not
  reproduce the #508-attempt-4 split-brain the slice exists to prevent.
- The O(N) extra root `get` per maintenance pass (`resolve.rs:81`, `:101`) and the
  `find_chunk`-per-repair amplification (`reconstruction.rs:604-615`) are real, but they are
  already carried as a §6 human item on this bundle — not re-filed here.
