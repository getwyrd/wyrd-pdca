# Build notes — issue 648 / chunkmap-flat-segmented-record-shape

Withheld from the reviewer by the driver; for the human at sign-off.

## What shipped, and why this shape

`crates/core/src/metadata.rs` gains the segmented chunk-map **record shape and its
codec** only, per the brief's scope line: `ChunkMapError` (14 variants — only the ones
this slice's own invariants raise, not the ~50-variant resolver/publication superset
in `sources/salvage.diff`, `:308`), `SegmentGroup`/`SegmentRef`/`SegmentedMap`/
`ChunkMap`/`SegmentRecord` (`:480`, `:529`, `:548`, `:684`, `:780` in the patched
file), the `seg:`/`seggrp:` key helpers + strict parser (`:909-975`), the capacity
constants (`SEG_INDEX_WIDTH`, `SEG_NONCE_HEX_LEN`, `MAX_ROOT_SEGMENTS`,
`MAX_VALUE_BYTES`, `:257-289`), and `InodeRecord`'s `#[serde(try_from =
"InodeRecordWire")]` cross-field size-vs-span check (`:1005`, `checked_shape` at
`:1088`).

**Salvage extraction, trimmed to scope.** `sources/salvage.diff`'s `metadata.rs` region
carries the *whole* #635 design — resolver (`resolve_chunk_map`), staged publication
(`SegmentedPublication`), repoint, batch-budget enforcement — none of which this slice
owns (brief: "Out of scope: the resolver and its consumers (#649–#651)... the
staged-publication committer (#653)"). I extracted only the shape+codec region the brief
names, and inside `ChunkMapError` kept only the variants my own decode invariants or the
mechanical caller migration raise: `NonceNotHex`, `NoSegments`, `SegmentCountMismatch`,
`SegmentIndexOutOfOrder`, `SegmentsNotContiguous`, `EmptySegment`, `SegmentSpanOverflow`,
`EmptySegmentRecord`, `SegmentSpanUnrepresentable`, `SegmentLengthMismatch`,
`SegmentLengthOverflow`, `SegmentKeyMalformed`, `SizeSpanMismatch`, plus one I added,
`SegmentedMapUnsupported` (below). Dropped ~35 salvage variants (`SegmentAbsent`,
`TooManySegments`, `BatchOverBudget`, `Unfenced`, `FenceCycled`, …) that belong to the
resolver/publication slices. Cost if I'd kept them: dead code today (no caller raises
or matches them until #649+) and a bigger surface for `cargo-machete`/clippy
`dead_code` to flag — the salvage region is ~990 semantic lines; mine is 578
(`git diff crates/core/src/metadata.rs | grep '^\+' | grep -v '^\+\+\+' | grep -vE
'^\s*(//|$)' | wc -l` → 578), comfortably under the ≤1,500 budget with headroom to
spare rather than needing it.

**Dropped the typed-error re-derivation at `decode()` entirely** (salvage's
`structural_chunk_map_fault` / `inode_chunk_map_fault` / `decodes_without` /
`decode_segment_record`, ~250 lines). Salvage's rationale for it is GC's downcast-based
fault containment (`crates/custodian/src/gc.rs:298,:317` in salvage's numbering) — a
#649+ resolver concern. Without a resolver, nothing in this slice downcasts on
`ChunkMapError` from a generic `decode::<T>()` call; every decode-invariant negative
case in scope only needs `Err`, not a *specific* error type, and `serde`'s own
`D::Error::custom` stringification already turns every constructor failure
(`SegmentedMap::new`, `SegmentRecord::checked`, `SegmentGroup::new`) into a decode
`Err` through the ordinary `serde_json::Error` path. Cost of keeping it: ~250 lines of
code with zero callers until #649, and a `decode()` whose behavior for `T != InodeRecord
| SegmentRecord` this slice cannot even test (no resolver calls it). Left as a `#649`
concern; `crates/core/tests/segmented_map_record.rs` proves `Err` is what happens, not
which named variant produces it — matching the brief's criterion 2 wording ("...has its
raw-byte negative case that is `Err`").

**`ChunkMapError::SegmentedMapUnsupported`** is the one variant salvage does not have,
because in salvage's finished design each call site got a bespoke variant
(`SegmentedPublicationBypassed`, `SegmentedRetirementUnsupported`, …) tied to that
site's own future semantics (a create refusing to publish an unverified segment range,
a retirement fan-out `#636` owns). This slice ships **no such site-specific behavior** —
scope says "nothing that reads or writes one" — so those bespoke variants would be dead
until their owning slice lands. `SegmentedMapUnsupported { operation: &'static str }` is
the one generic, reusable shape every mechanical migration call site raises today; a
later slice can introduce its own richer variant per call site without this one being in
the way (it is additive, never matched on by name anywhere yet).

## The 37-file mechanical ripple (Budget: counted separately, "allowed on top")

`InodeRecord.chunk_map: Vec<ChunkRef> -> ChunkMap` is a breaking type change reaching
every construction and read site in the workspace — 43 files matched
`git grep -rl "chunk_map" -- crates/` on the base; 37 needed an actual code edit (6
matched only in doc comments / already-`ChunkRef`-typed helper functions and needed
nothing). Every edit is one of exactly two mechanical shapes, per the brief's own
framing:

- **construction sites** — `chunk_map: <vec-expr>` gains `.into()` (`Vec<ChunkRef>` →
  `ChunkMap::Flat` via the `impl From<Vec<ChunkRef>> for ChunkMap`,
  `crates/core/src/metadata.rs:723-730`);
- **read sites** — `.chunk_map` gains `.as_flat()`. In production code
  (`crates/core/src/read.rs:94-99`, `crates/core/src/metadata.rs` commit helpers,
  `crates/custodian/src/{backfill,gc,rebalance,reconstruction,restore}.rs`,
  `crates/server/src/lib.rs`) this is `.as_flat().ok_or(ChunkMapError::
  SegmentedMapUnsupported { operation: "..." })?` — the brief's "Caller-first" line:
  "every existing `.chunk_map` site must therefore treat the `Segmented` variant as a
  typed error, not an empty list." In test code it is `.as_flat().unwrap()` (every
  test-constructed record is a hand-built `Flat` map, so the `None` arm is
  unreachable by construction, not a defensive slot).

**Why fail closed via `?` rather than skip-and-continue-the-scan in the custodian
loops.** `backfill::reconcile`, `gc::referenced_fragments`, `rebalance::
plan_evacuations`, `reconstruction::find_chunk`, and `restore::committed_chunks` are
per-store *scans* (`for (key, value) in meta.scan(b"inode:")...`), and every one of
them **already** aborts the whole scan on any single record's plain JSON decode
failure via `let record: InodeRecord = metadata::decode(&value)?;` — that line
predates this patch. Propagating `ChunkMapError::SegmentedMapUnsupported` the same
way is not a new failure mode; it is the existing one, extended to one more shape
under the same call. The alternative — `continue`-skip a segmented record inside the
loop — would be the exact anti-pattern the brief's Invariant-to-restore section rules
out ("A consumer that meets a shape it cannot resolve fails closed for that object —
never 'this object owns no chunks'"): GC's `referenced_fragments` in particular would
then build a reference set that does not protect that object's chunks, and a
`continue`-skip is silently "this object owns no chunks" by omission. Since no
producer exists yet in this slice, this path is unreachable in production either way;
the fail-closed shape is chosen for **when #653 lands a producer**, not for anything
this slice can trigger today.

## Docs currency

`docs/design/architecture/08-crosscutting-concepts.md`, one new paragraph in §8.7
(after the existing wire/on-disk compatibility paragraph) — the record-shape paragraph
only, per the brief's Docs-currency line. I did **not** port salvage's paragraph
verbatim: it also describes the staged two-phase publication and the committer's
shape-refusal (`#653`) and the resolver's containment behavior (`#649-651`), both
explicitly out of scope here ("the resolver/containment paragraphs belong to #649–#651
and staged publication to #653; do not write them here"). My paragraph covers exactly
what this patch implements: the two shapes, the byte-identity requirement, the new
`seg:`/`seggrp:` prefixes needing no migration, the decode-time structural invariants,
and the deliberate absence of a capacity check at decode.

## Alternatives ruled out

- **Adding all ~50 `ChunkMapError` variants now, so #649-653 don't need to touch the
  enum.** Rejected: every unused public enum variant is dead code no test in this
  slice can exercise (no resolver, no publication, no repoint), inflates the semantic
  line count for no present benefit, and the brief's own scope line is explicit —
  "take **only** the variants your invariants raise." Concrete cost: ~35 variants ×
  ~10 lines each (variant + doc comment + `Display` arm) ≈ 350+ lines of untested,
  unreferenced code, roughly 60% of my entire `metadata.rs` diff, for zero behavior
  this slice can prove.
- **Keeping salvage's `decode_segment_record` / containment re-derivation** — covered
  above; ~250 lines with no caller until #649's GC containment wiring.
- **A positional/enum-tag encoding for `ChunkMap` (e.g. `{"type":"flat","chunks":[...]}`
  always)** instead of JSON-type discrimination. Rejected on the Invariant-to-restore
  line itself: it is not byte-identical to any pre-existing record, so it turns every
  first CAS over an existing object into a permanent `Conflict` (demonstrated below).
  Salvage's design (discriminate on JSON type: array = flat, object = segmented) is the
  only one that satisfies criterion 1, and I kept it rather than re-deriving it — this
  is the peer pattern the brief names Do MAY open (`origin/main:crates/core/src/
  metadata.rs:277-286`, the ADR-0047 `skip_serializing_if` byte-identity precedent).
- **Field-wise mutable `SegmentRecord`** (public `chunks`/`byte_offset`/`byte_len`
  fields, or a `chunks_mut()`) so a future repoint could mutate in place. Rejected for
  *this* slice: repoint is `#649+`'s concern (the brief lists it as an
  out-of-scope-here consumer), and a mutable accessor would let a caller violate the
  `byte_len == sum(chunk.len)` invariant `SegmentRecord::checked` enforces at
  construction with no recheck. Kept private fields + validating constructor only.

## Falsifiability — RED confirmed, GREEN confirmed

Per the brief's exact recipe: reverted the **whole patch** (not just `metadata.rs` —
reverting only that file breaks compilation of the mechanically-migrated callers,
which is not what "RED leg" means; the intent is "this patch does not exist yet, plus
the new test file"), kept `crates/core/tests/segmented_map_record.rs`, ran
`cargo test -p wyrd-core --test segmented_map_record`:

```
running 11 tests
test duplicate_segment_index_is_err ... ok
test non_hex_nonce_is_err ... ok
test non_monotonic_segment_span_is_err ... ok
test segment_index_gap_is_err ... ok
test legacy_flat_record_round_trips_byte_identically ... ok
test overlapping_segment_spans_is_err ... ok
test segment_table_span_disagreeing_with_size_is_err ... ok
test segment_count_mismatching_segments_len_is_err ... ok
test well_formed_segmented_root_decodes ... FAILED
test segmented_root_at_max_root_segments_stays_inside_the_value_ceiling ... FAILED
test legacy_flat_record_cas_still_commits_against_the_original_bytes ... ok

test result: FAILED. 9 passed; 2 failed
```

Exactly as the brief's Falsifiability section predicts: criterion 2's positive decode
assertion and criterion 3 both fail as **assertions** (`invalid type: map, expected a
sequence` — `chunk_map` is still `Vec<ChunkRef>` on origin/main, so a JSON object
never decodes), the file still **compiles** (it imports nothing this patch adds), and
the 9 negative-case tests stay green because a bare `serde_json` array-vs-object
mismatch already fails to decode as `Vec<ChunkRef>` on origin/main too — they were
never red on the base, only the two positive-shape assertions are. Then restored the
patch and re-ran: all 11 green. Then diffed the restored tree against the shipped
`patch.diff` (`git diff crates/core/src/metadata.rs | diff - <(...patch.diff's
metadata.rs hunk...)`) to confirm the revert/restore cycle left no stray edits — exit
0.

Also ran the two co-located `metadata.rs` invariant tests
(`crates/core/src/metadata.rs:1775`, `mod segmented_shape_invariants`) that need
patch-added symbols (`parse_seg_key`, `SegmentRecord`) and so cannot live in the
base-visible-only test file: `cargo test -p wyrd-core --lib -- segmented_shape` → 2
passed (wrong-width `seg:` key index; `SegmentRecord` chunk lengths not summing to
`byte_len`).

## Criterion 1's demonstrated red (Verification posture)

Criterion 1 ("legacy round-trips byte-identically, and CAS still commits") is, per the
brief, "a property of the changed codec that is trivially true on the base" and cannot
be flipped red/green by reverting `metadata.rs` — a `Vec<ChunkRef>` trivially
round-trips. Per the brief: *"Do MUST instead record a demonstrated red in
build-notes.md — with the patch applied, temporarily serialize `ChunkMap` as a tagged
enum, show criterion (1)'s legacy-CAS assertion fail, revert."* Done exactly that:
temporarily rewrote `impl Serialize for ChunkMap` (`crates/core/src/metadata.rs:731`)
to wrap `Flat`/`Segmented` in an adjacently-tagged `{"kind":"Flat","data":[...]}"`
shape instead of a bare array, ran
`cargo test -p wyrd-core --test segmented_map_record legacy_flat_record`:

```
test legacy_flat_record_round_trips_byte_identically ... FAILED
  left:  [...,"chunk_map":{"kind":"Flat","data":[...]},...]
  right: [...,"chunk_map":[...],...]                        // the untagged original
test legacy_flat_record_cas_still_commits_against_the_original_bytes ... FAILED
  assertion `left == right` failed: require(key, encode(prior)) must commit against
  a store holding the original bytes
    left: Conflict
   right: Committed
```

This is exactly the failure mode the Invariant-to-restore section names: the tagged
encoding turns the CAS `Conflict` — the concrete, mechanical shape of "every overwrite
of a pre-existing object becomes a permanent conflict forever." Reverted the temporary
change immediately (`git diff` confirms `Serialize for ChunkMap` matches the shipped
patch byte-for-byte); re-ran the full `segmented_map_record` suite green (11/11).

## Refutation (forced, per the Do brief)

**(a) Genuine red?** Yes — see Falsifiability above: reverting the whole patch (keeping
only the new test file) turns 2 of 11 assertions red, exactly the two the brief's
Falsifiability section names, for exactly the reason it states (JSON object vs. `Vec`
type mismatch). Confirmed by direct run, not asserted.

**(b) Production path?** Yes on both fronts:
- `crates/core/tests/segmented_map_record.rs` drives `wyrd_core::metadata::{encode,
  decode}` directly — the same `decode<T: DeserializeOwned>` / `encode<T: Serialize>`
  every commit path in `metadata.rs` calls, and `InodeRecord`'s real
  `#[serde(try_from = "InodeRecordWire")]` decode path (`:1005`) — not a copy, not a
  parallel parser.
- The CAS test (`legacy_flat_record_cas_still_commits_against_the_original_bytes`)
  drives the real `wyrd_metadata_redb::RedbMetadataStore` and the real
  `wyrd_traits::MetadataStore::commit` trait method against an on-disk redb file in a
  tempdir — not an in-memory stand-in — so the `require(key, encode(prior))` CAS this
  module's commit functions all use is exercised through the actual backend
  conformance target names elsewhere in this crate (`crates/core/tests/
  placement_record.rs:29-33`, the peer the brief cites for this dev-dependency's
  precedent).
- The co-located `metadata.rs` tests drive `parse_seg_key` and `SegmentRecord`'s real
  `Deserialize` impl directly (same module, no test-only shadow).

**(c) Fixture includes the fault?** Yes: every negative case is a hand-authored raw
JSON byte string carrying the exact structural fault under test (a duplicate index, an
overlapping span, a 31-character nonce, a `segment_count` that disagrees with the
array, a wrong-width `seg:` key digit count, chunk lengths that don't sum) — not a
fixture that omits the offending shape. The legacy round-trip fixture
(`LEGACY_BYTES`) is captured from a real `encode()` call over an equivalent
`ChunkMap::Flat` record (documented in the constant's own doc comment) rather than
freehand-typed, so it cannot silently drift from what this codec actually emits.

## Verification run log (this session, all green unless noted)

- `cargo check --workspace --all-targets` — clean (0 errors) after the full mechanical
  migration; this is the strongest signal here, since the type change is breaking and
  the workspace has 43 files with a stake in the old shape.
- `cargo test -p wyrd-core --all-targets` — 28+ tests across lib/tests/benches, all
  green (the bench harnesses run as tests under `cargo test`, verified functionally,
  not timed).
- `cargo test -p wyrd-custodian --all-targets` — every test file green (`backfill`,
  `backfill_telemetry`, `gc`, `gc_delete_backstop`, `gc_telemetry`, `rebalance`,
  `reconstruction`, `restore_reconcile`, `scrub`, `skeleton`; `tier1_disk_faults`'s one
  test stays `ignored` — it needs root + device-mapper, unrelated to this patch).
- `cargo test -p wyrd-metadata-redb --all-targets` — green.
- `cargo test -p wyrd-server` (all test binaries except `custodian_day_one`, then
  `custodian_day_one` alone) — green; `custodian_day_one`'s 15 tests spin up real
  in-process gRPC D-servers and are slow under contention with other builds, but ran
  clean (0.22s once isolated).
- `cargo test -p wyrd-chunkstore-grpc --test tier2_integration --test
  tier1_jepsen_consistency --test tier2_kill_reconstruct` — every runnable test green;
  the container-backed legs stay `ignored` (need Docker, per `cargo xtask
  jepsen`/`kill-reconstruct`/`integration` — this project's own declared gate for
  those, unaffected by this patch).
- `RUSTFLAGS="--cfg madsim" cargo check -p wyrd-dst --all-targets` and `cargo test`
  (both `commit_ambiguity` and `custodian` test binaries) — `wyrd-dst` only compiles
  under `--cfg madsim` (`crates/dst/tests/commit_ambiguity.rs:67`,
  `#![cfg(madsim)]`), which is why a plain workspace check does not reach it; ran it
  explicitly, matching `xtask::run_dst`'s own build (`xtask/src/main.rs:1579-1614`).
  24 tests, all green.
- `cargo fmt --all -- --check` — clean after `cargo fmt --all` (the project's
  configured formatter; several of my multi-line `.ok_or(ChunkMapError::…)?` chains
  needed rustfmt's re-wrap).
- `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server -p wyrd-metadata-redb -p
  wyrd-chunkstore-grpc --all-targets` and `RUSTFLAGS="--cfg madsim" cargo clippy -p
  wyrd-dst --all-targets` — 0 warnings on every touched crate.
- `typos docs/design/architecture/08-crosscutting-concepts.md crates/core/src/
  metadata.rs crates/core/tests/segmented_map_record.rs` — clean.

**Not run**: the full `cargo xtask ci` (fmt/clippy/build/test/machete/deny/conformance/
madsim-DST-sweep in one job) end-to-end — it is explicitly "Supplementary, not
binding" per the brief, and includes legs unrelated to this slice's surface (the
50-seed DST sweep, `cargo-deny` licence/advisory scanning, `cargo-machete`). Every
constituent piece `cargo xtask ci` runs that touches this patch's surface (fmt,
clippy, build, the affected crates' tests, the two DST test binaries under
`--cfg madsim`, typos) was run directly above. `cargo-machete`/`cargo-deny` add no new
dependencies this patch could trip (no `Cargo.toml` changed). Check's `C4-ci` gate
runs the authoritative full pass.

**Python docs-renderer dependency** (`markdown_it`/`yaml`) is not installed in this
sandbox; `typos` is and passed. Per INTEGRATION §3 and the brief's own framing, `cargo
xtask ci`'s prose gates warn-and-skip when the renderer is absent locally rather than
failing the build — not a NEEDS-HUMAN, since the brief names this exact absence as
expected/tolerated, not a blocker.

## Review-rubric self-check (`AGENTS.md` §"Review rubric & protocol")

- **Metadata validation boundaries (ADR-0045)**: every structural invariant
  (`SegmentedMap::new`, `SegmentRecord::checked`, `SegmentGroup::new`,
  `InodeRecord::checked_shape`) is enforced at decode and surfaces as an error, never
  a half-built value; the one *contextual* limit (`MAX_ROOT_SEGMENTS`) is explicitly
  liberal at decode by design (doc comment on the constant explains why), matching the
  rubric's "contextual checks liberal on read" line precisely.
- **Serialization identity**: `legacy_flat_record_round_trips_byte_identically` is the
  round-trip test the rubric asks for by name ("add the round-trip test").
  `skip_serializing_if` behavior on `etag`/`content_type`/`modified` is untouched.
- **Absent or unsupported entries**: every mechanical-migration read site raises
  `ChunkMapError::SegmentedMapUnsupported` rather than silently skipping or
  count-based-asserting past a segmented map — no site returns success or an empty
  iteration for a shape it cannot resolve.
- **Docs currency**: the persisted-field change (`InodeRecord.chunk_map`'s type) is
  documented in the same patch (`docs/design/architecture/08-crosscutting-concepts.md`
  §8.7), not deferred.
- **Grammar strictness**: `parse_seg_key`'s epoch parsing rejects `+7`/`007`-style
  non-canonical decimal (`parse_canonical_u64`) — the exact class of finding this rule
  exists to catch, carried over unmodified from the salvage source's own
  rationale.
- **Transactions**: every new `?`-propagated `ChunkMapError` in the custodian/server
  call sites fires **before** any `WriteBatch` reaches `store.commit(...)` for that
  record (a pre-commit classification/read step) — nothing rolls back a live
  transaction because none was ever submitted.
- No new crate was added (N/A: `#![forbid(unsafe_code)]` on every new crate root); the
  new test file itself carries `#![forbid(unsafe_code)]` at its top consistent with
  the repo's existing test-file convention.

## Citations (target branch = `getwyrd/wyrd@main`, worktree at `9120f7a`)

- `crates/core/src/metadata.rs:257-1806` — the added shape+codec region and the
  mechanical edits to `unlink`/`commit_chunk_map`/`commit_chunk_map_superseding[
  _leased]`/`high_water_marks`/`InodeRecord`.
- `crates/core/src/read.rs:25,80,94-99` — `read_object_collecting`'s
  `.as_flat().ok_or(...)?`.
- `crates/core/src/write.rs:274` — `commit_create`'s `.into()`.
- `crates/custodian/src/{backfill,gc,rebalance,reconstruction,restore}.rs` — mechanical
  read-site migration, each documented inline with the same "fails closed exactly as
  an unreadable record already does" rationale.
- `crates/server/src/lib.rs:32,344,422` — `Gateway::get_object_streaming` /
  `get_object_range`.
- `docs/design/architecture/08-crosscutting-concepts.md` §8.7 — the added paragraph.
- `crates/core/tests/segmented_map_record.rs` — the brief's named test file (new).
- `sources/salvage.diff:1-1471` (the `metadata.rs` hunk) — extraction source, per
  Citations expected.
- `origin/main:crates/core/src/metadata.rs:277-286` — the ADR-0047 byte-identity/CAS
  precedent the brief names as the one peer callsite Do MAY open; the JSON-type
  discrimination in `ChunkMap`'s `Deserialize` (`crates/core/src/metadata.rs:740-770`
  patched) obeys the same rule for the segmented variant.

## STOP discipline

No PR opened, no branch pushed. `patch.diff` + `crates/core/tests/
segmented_map_record.rs` + this file are the complete Do output; Check picks up from
here.
