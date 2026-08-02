# PR description

> One logical fix per PR.

## Summary
**User impact:** Objects larger than roughly 165-381 MiB (at default chunk size)
cannot exist at all today, because the record that lists an object's chunks is
capped by the smallest backend's value-size limit. That is well short of the
project's stated goal of supporting objects over 10 GiB, and every maintenance
process that keeps data safe - garbage collection, scrubbing, restore,
rebalancing, and repair - as well as both read paths, would need to agree on
any new shape or risk silently losing or stranding data for large objects.

This change adds a second, segmented representation for an object's chunk
list that is not bounded by a single value's size, publishes it safely (so a
crash mid-publish can never expose a half-written map), and routes every one
of those maintenance and read consumers through one shared resolver so none
of them can fall out of step with the others. Existing (flat) objects are
completely unaffected - they are stored and read exactly as before.

No tracker issue URL is configured for this fork, so there is no report link
to include here.

## What to look at
- `crates/core/src/metadata.rs` - the new `Flat | Segmented` chunk-map shape,
  its encoding, and the resolver that turns either shape into an ordered
  chunk list.
- The staged-publication committer (same file) - writes the segment records
  first, then flips the object's root to point at them in a single
  compare-and-swap batch.
- Every existing consumer of the chunk map - GC (`crates/custodian/src/gc.rs`),
  scrub (`crates/custodian/src/scrub.rs`), restore (`.../restore.rs`),
  rebalance (`.../rebalance.rs`), reconstruction (`.../reconstruction.rs`),
  backfill (`.../backfill.rs`), and both read paths
  (`crates/core/src/read.rs`, `crates/server/src/lib.rs`) - now goes through
  that one resolver instead of reading the chunk list directly.
- To exercise it: `crates/custodian/tests/segmented_map_consumers.rs` seeds a
  segmented object directly (there is no production writer of one yet - see
  Root cause) and runs it through the real maintenance passes and read paths.

## Root cause
A published chunk map was one JSON value, and every backend's value-size
ceiling (FoundationDB's 100 KB is the tightest, `crates/traits/src/lib.rs:997`)
limited it to a few hundred chunks - a few hundred MiB of object at default
chunk size, far under the >10 GiB requirement. Every maintenance pass and both
read paths decoded that same flat shape directly, so a naive fix that only
changed the shape used by reads would leave GC, restore, and the others
misreading or stranding the new shape.

## Fix
`InodeRecord.chunk_map` now discriminates on JSON type: a JSON array stays the
existing flat shape, byte-identical for every pre-existing record; a JSON
object is a segmented map naming a group plus an ordered set of `seg:`
records holding the real chunks. A segmented map is published by writing its
segments in bounded batches and then flipping the root in one CAS batch that
may also carry a caller's own precondition and mutations, so a session's
state transition and the map's publication commit together or not at all. A
resumed publication re-verifies the segments it is trusting rather than
taking a caller's resume point on faith, and every zero-I/O refusal is
decided before any segment is written, so a refused publication leaves no
partial trace.

One resolver (store + record + the object's root identity) answers every
consumer, ordering segments by their own index rather than store scan order,
and re-reading the root before treating a mid-resolve missing segment as
corruption rather than a concurrent retirement. Every existing `.chunk_map`
consumer - GC's reference build (and therefore scrub), restore, rebalance,
reconstruction, backfill, the gateway and core read paths, and the chunk-id
floor computed before a gateway starts serving - is routed through it, each
with an explicit rule for a single damaged object: the gateway still starts,
healthy objects still read and are not reclaimed, and only the damaged object
fails closed. Scrub, which previously reported a clean pass over a store it
could only partially verify, now reports a distinct "blocked" outcome and
emits an operator-visible signal instead of silently certifying an
incomplete pass.

No production code path can yet create a segmented map - that requires the
multipart-upload session (a separate change) that supplies the fence and
group identity - so this ships the shape, the publication protocol, and the
resolution contract ahead of that wiring, exercised directly against the
real maintenance and read code through a raw-seeded fixture.

## Verification
- **Claim:** every maintenance pass and read path resolves a segmented chunk
  map instead of erroring or silently treating it as empty.
  **Checked:** `crates/custodian/src/gc.rs:251-256` (`referenced_fragments`
  decoded every value with a strict `metadata::decode(&value)?`) and the
  equivalent strict decode in restore (`restore.rs:373-376`), rebalance
  (`rebalance.rs:147-148`), reconstruction (`reconstruction.rs:603-608`),
  backfill (`backfill.rs:76-81`), and `metadata::high_water_marks`
  (`crates/core/src/metadata.rs:847-857`) on the target branch - each fails
  or drops a segmented object today.
  **Test:** `crates/custodian/tests/segmented_map_consumers.rs` - on the
  target branch it compiles (it names only symbols that already exist there)
  and its 12 assertions fail with `Err`/panic against a raw-seeded segmented
  object; with this patch applied all 12 pass, driving the real
  `reconcile_step`, `reconcile_after_restore`, `reconciliation_status`, and
  `high_water_marks` code paths (not test doubles of them).

- **Claim:** one damaged segmented object does not take the rest of the
  store down, and nothing of it is silently reclaimed.
  **Checked:** the resolver and the per-consumer containment behaviour in
  `crates/core/src/metadata.rs` (chunk-id floor) and the consumer sites
  above; a scrub of a store holding one unresolvable object now reports a
  distinct blocked outcome (`crates/custodian/src/scrub.rs`,
  `crates/custodian/src/reconciliation.rs`) instead of the same "clean"
  result it previously gave.
  **Test:** `crates/custodian/tests/segmented_map_consumers.rs` seeds a
  third, damaged segmented object alongside a healthy flat and a healthy
  segmented one, and asserts the healthy objects keep reading
  byte-identically, the damaged object fails closed rather than returning
  torn bytes, and none of its fragments are reclaimed across a GC pass past
  the grace window. `crates/custodian/tests/scrub.rs` adds the
  blocked-vs-changed scrub case over a store with both a rotten fragment and
  an unwritten segment group.

- **Claim:** a pre-existing (flat) object's stored bytes and CAS behaviour
  are completely unchanged.
  **Checked:** `crates/core/src/metadata.rs:277-289` documents why
  decode-then-re-encode must be byte-identical for every existing CAS to
  keep working; the segmented encoding was designed to preserve that for
  flat records.
  **Test:** co-located unit tests in `crates/core/src/metadata.rs` decode
  and re-encode a pre-existing flat record (including one with optional
  fields absent) and assert byte equality, and assert
  `metadata::commit_chunk_map` against a legacy record still returns
  `Committed`. `segmented_map_consumers.rs` additionally asserts a flat
  object's stored bytes are unchanged after every maintenance pass runs
  alongside the segmented and damaged objects.

Fixes #635
