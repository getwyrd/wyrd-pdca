# Build notes — issue 649 / shared-segmented-map-resolver-and-read-paths

Withheld from the reviewer; for the human at sign-off.

## What shipped

- `crates/core/src/metadata.rs` — the resolution result type (`MapResolution`), the
  bounded per-group range read with its ceiling check (`read_group_range`, paged via
  `MetadataStore::scan_page`, refusing `> MAX_ROOT_SEGMENTS` before the first page),
  the resolve-retry arbiter (`retired_or`), and the resolve-against-the-live-root
  entries (`resolve_chunk_map`, `resolve_current_chunk_map`, `resolve_live_chunk_map`),
  plus the six new `ChunkMapError` variants they raise (`TooManySegments`,
  `SegmentUnknown`, `SegmentAbsent`, `SegmentBoundsMismatch`,
  `SegmentRecordUndecodable`, `MapResolutionUnstable`).
- `crates/custodian/src/resolve.rs` (new) + `crates/custodian/src/lib.rs` (`pub mod
  resolve;`) — the thin wrapper (`chunks_of`, over `resolve_current_chunk_map`) and the
  root-classification arm (`classify_root`/`Root`/`ChunkMapFault`) every
  `scan("inode:")` loop will use; own unit coverage (5 tests). Its pass consumers are
  #650/#651, per brief scope.
- `crates/core/src/read.rs` — `read_object_from` renamed to `read_object_chunks`,
  taking an already-resolved `&[ChunkRef]` + `size` instead of `&InodeRecord`
  (`read_chunks_collecting` likewise); `read_object` and `committed_inode` resolve
  first through `metadata::resolve_live_chunk_map` / return `(InodeId, InodeRecord)`.
- `crates/server/src/lib.rs` — `get_object_streaming` and `get_object_range` resolve
  through `metadata::resolve_live_chunk_map` instead of `.chunk_map.as_flat().ok_or(..)`;
  `head_object` updated for `committed_inode`'s new return shape.
- Two **new** test files (the Check discriminators):
  `crates/core/tests/segmented_map_resolution.rs` (11 tests) and
  `crates/server/tests/segmented_object_read.rs` (4 tests).
- `crates/dst/tests/custodian.rs` — a ninth Tier-0 property,
  `prop_segmented_resolve_never_tears` (`segmented_resolve_never_tears` under
  `dst_campaign_test!`), plus the mechanical `read_object_from` → `read_object_chunks`
  rename at its two call sites.
- `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — the resolver paragraph
  only, per the brief's Docs-currency line.
- Mechanical migration (declared, per brief, counted separately from the line/file
  budget): 11 files whose only change is `read_object_from(&s, &r)` →
  `read_object_chunks(&s, r.chunk_map.as_flat().expect(..), r.size)` —
  `crates/core/benches/throughput.rs`, `crates/core/tests/placement_record.rs`,
  `crates/chunkstore-grpc/tests/tier2_integration.rs`,
  `crates/server/tests/{erasure_path,read_path,read_fanout,dst_commit,dst_erasure,
  dst_read_fanout,write_fanout}.rs`, and the same rename (plus an import/doc-comment
  fix) in `crates/dst/tests/custodian.rs`.

## Design choices and what I ruled out

**Scope cut vs. salvage.diff.** The bundle's `sources/salvage.diff` (from #647,
closed unmerged on reviewability) carries #653's staged-publication committer
(`SegmentedPublication`), #651's repoint machinery (`ChunkHome`/`HomedChunk`/
`resolve_chunk_homes`/`resolve_current_chunk_homes`/`resolve_live_chunk_homes`/
`repoint_chunk`), and #650/#651's full custodian containment surface (5 pass
constants, the uncommitted-vs-blocker classification needing a `decode()` rewrite
that re-derives structural faults from raw bytes via `structural_chunk_map_fault`).
None of that has a caller in *this* slice (brief: "no behaviour flip and no producer
of segmented maps"; "Out of scope: ... restore, reconstruction, backfill, rebalance,
desired_state, repoint_chunk"). Carrying it anyway would be ~1,200+ extra semantic
lines with zero callers — a caller-first violation and the exact over-budget shape
the brief's "prune the co-located resolver tests to the binding cases" line warns
against. Cost check: salvage's `custodian/src/resolve.rs` alone is 367 lines; mine is
120 (production) + 170 (tests) = 290, because it drops `ChunkHome`/`homes_of`/the
5-pass-constant/uncommitted-split machinery entirely.

**`scan` vs `scan_page` for the group range.** salvage.diff's resolver (written before
#634/PR #645 landed `scan_page`) calls `store.scan(seg_range_prefix(..))` directly. The
brief's own citation set names `crates/traits/src/lib.rs:275-324` ("scan is
complete-or-fail-loud at SCAN_CAP, which is why a group range must be paged") and the
`scan_page` doc lines — so I did not copy salvage's `scan` call; `read_group_range`
pages via `scan_page` with `limit = accounted + 1`, refusing before the first page when
`accounted > MAX_ROOT_SEGMENTS`. Rejected alternative: trust `scan_page`'s documented
byte-lex order and concatenate segments in page-arrival order — cheaper (no
intermediate `BTreeMap`), but it would silently depend on a contract the resolver
doesn't have to lean on, and the brief explicitly wants the ordering **proved** against
a shuffling double, which only holds if the code sorts on parsed index. Kept the
`BTreeMap<u32, SegmentRecord>` (indices are `u32`, never more than
`MAX_ROOT_SEGMENTS` + 1 entries, so the cost is bounded and small).

**`decode_segment_record` without `structural_chunk_map_fault`.** Salvage's version
re-parses raw bytes with a permissive `serde_json::Value` fallback so a `serde`-boxed
opaque error can be re-derived into a typed `ChunkMapError` even when it came through
`#[serde(try_from = ...)]`'s `Error::custom` (which discards the concrete type). That
machinery exists to make `custodian::resolve::classify_root`'s `downcast_ref` reliably
fire for #650/#651's containment. My `classify_root` never needs that: it classifies
bytes it has just read successfully (the `scan` already succeeded), so ANY decode
failure on those bytes is unambiguously that object's own fault — no downcast, no
re-parse, `Result<Root>` collapsed to an infallible `Root`. `decode_segment_record`
(metadata.rs) still needs *a* typed error for the resolve-retry arbiter, so it wraps
whatever `decode::<SegmentRecord>` returns (a `ChunkMapError` when the value itself
fails a structural check, `.to_string()` of the raw error otherwise) into
`SegmentRecordUndecodable { index, detail }` — one variant, not four, since nothing in
this slice needs to distinguish *why* a segment didn't decode.

**Seeding: raw `seg:` records, never a committer.** The brief's rework note is explicit
("rewrite it to seed raw `seg:` records"). Both new test files' helpers use the plain
single-shot write path (`wyrd_core::write::plan_write` /`intent`/`write_fragments`/
`commit_create`/`release`) to get **real, on-disk fragments** for a genuine flat object,
then splice the SAME `ChunkRef`s into segment records and a segmented root via one raw
`WriteBatch` commit — never `metadata::create`, `commit_chunk_map*`, or any committer
(there is none in this slice; #653 lands it). This matches the server test's
`Gateway` field-privacy constraint too: `Gateway<M,C,Co>`'s fields are private to
`wyrd_server`, so an external integration test cannot reach `gateway.meta` after
construction — seeding has to happen on a bare `RedbMetadataStore` handle *before*
`Gateway::new` takes ownership of it.

**Import discipline in the two new test files.** The brief's "Test file:" paragraph
names a minimum set of symbols per file, but the Falsifiability section is the
controlling constraint: "both files still compile because they import only
base-visible symbols" and "nothing from *this* patch." I read the named lists as
representative, not an exhaustive whitelist — `SegmentGroup`/`SegmentedMap`/
`SegmentRef`/`ChunkMap`/`EcScheme`/`InodeState` (all from #648, base) and
`wyrd_core::write` (untouched by this patch) are freely imported; what is genuinely
forbidden and genuinely respected is anything *this* patch adds
(`resolve_chunk_map`, `MapResolution`, `LiveChunkMap`, `CurrentChunkMap`,
`resolve_live_chunk_map`, `resolve_current_chunk_map`, and the six new
`ChunkMapError` variants) — none of those are named in either test file. Verified
directly: reverting `metadata.rs`/`read.rs`/`server/lib.rs` and removing
`custodian/resolve.rs` still **compiles** both test targets (see the red→green section
below) — the base tree.

**Strengthening the "must fail closed" tests against a vacuous pre-fix pass.** First
draft of the four `root_unchanged_*_fails_closed` tests (and the ceiling test) just
asserted `.is_err()`. On the reverted base tree those ALSO pass, for the wrong reason:
`.chunk_map.as_flat().ok_or(SegmentedMapUnsupported)?` refuses *any* segmented map
unconditionally, so "the read errors" is true before the fix (vacuously) and after it
(for the actual typed anomaly) — not a fix-discriminating assertion, exactly the
"green mechanical check on something adjacent" trap. Fixed by adding
`assert_resolved_typed_refusal`, which downcasts to `ChunkMapError` (base-visible) and
asserts it is **not** `SegmentedMapUnsupported` — true only once the new resolver's own
typed anomaly is what fired. Re-ran the RED leg after this change (see below): all 11
core-file tests now fail as assertions pre-fix, none vacuously pass.

**DST property: real, not a compile-only stub.** The brief explicitly excludes the DST
property from `C4-verify`'s red/green scoring but still requires it "built and
exercised in this cycle" by `C4-ci`/`dst`. I did not just add a property that compiles
under `--cfg madsim` and stop there — I ran it (see below): 50 seeds, alone and
alongside the other eight Tier-0 properties in the same file, all green. The property
(`prop_segmented_resolve_never_tears`) mirrors the file's own established idiom for
"prove a property across a race window" (`prop_reader_flips_atomically_across_commit`
takes an explicit before/after snapshot rather than spawning racing tasks against
purely-synchronous in-memory doubles that never yield at an await point) — a
`RetireMidResolve` double applies a pending root-supersede exactly at the first
`seg:`-prefixed `scan_page` call, the same technique the two new test files use.

## The three refutation questions

**(a) Genuine red?** Yes, actually reverted and reran, not inferred. Copied the four
patched production files aside, `git checkout HEAD -- crates/core/src/metadata.rs
crates/core/src/read.rs crates/server/src/lib.rs crates/custodian/src/lib.rs` (HEAD is
the unmodified base this bundle branched from) plus `rm crates/custodian/src/resolve.rs`,
then ran both test targets:
- `cargo test -p wyrd-core --test segmented_map_resolution`: compiles clean, **6/11
  failed** as `SegmentedMapUnsupported` panics on the first pass; after adding
  `assert_resolved_typed_refusal`, **11/11 failed** as assertions (still compiling
  clean) — no compile-red-scored-as-pass, no vacuous "0 tests … ok".
- `cargo test -p wyrd-server --test segmented_object_read`: compiles clean, **4/4
  failed** as `SegmentedMapUnsupported` panics.
Then restored the four files and reran both — **11/11** and **4/4** green. Also
verified `cargo check --workspace --all-targets` stays clean pre- and post-restore
(the reverted intermediate state was never left in place for more than the single
`cargo test` invocation each).

**(b) Production path?** Yes. Both files drive real production entry points —
`wyrd_core::read::{read_object, read_path}` (crate `wyrd_core`) and
`Arc<wyrd_server::Gateway<..>>::{get_object_streaming, get_object_range}` via the
`wyrd_gateway_core::ObjectGateway` trait — over a real `wyrd_metadata_redb::RedbMetadataStore`
and a real `wyrd_chunkstore_fs::FsChunkStore` (server file) / the same pair (core
file). The three instrumented doubles (`RecordingStore`, `SupersedeMidResolve` ×2,
`ShufflingStore`) each wrap that real backend and forward every call unchanged except
the one instrumented/mutated point named in each test's own doc comment — none of them
re-implements or stands in for the resolver itself; the resolver code under test is
always `wyrd_core::metadata::resolve_*`, reached only through `read_object`/`read_path`/
the gateway.

**(c) Fixture includes the fault?** Yes, in every case the anomaly is a real, present
condition of the store the resolver reads, never curated out: the absent segment is
genuinely never written; the undecodable bytes are genuinely garbage; the unnamed
segment record genuinely sits under the group's own range; the extent mismatch is a
genuinely different `byte_len` than the root's table claims; the ceiling-breaching root
genuinely decodes (per `MAX_ROOT_SEGMENTS` being a resolve-time, not decode-time, bound)
with zero backing `seg:` records; the mid-resolve supersede/delete genuinely lands (via
a real `meta.commit`) inside the resolver's own `scan_page` call, not before or after it.

## Verification run log (this cycle, not fabricated)

- `cargo test -p wyrd-core --test segmented_map_resolution` — 11 passed (post-fix);
  11 failed (reverted, see above).
- `cargo test -p wyrd-server --test segmented_object_read` — 4 passed (post-fix);
  4 failed (reverted).
- `cargo test -p wyrd-custodian --lib resolve::` — 5 passed.
- `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets`,
  `cargo fmt --all -- --check` — all clean.
- `cargo test -p wyrd-core --lib --tests`, `cargo test -p wyrd-server --tests`,
  `cargo test -p wyrd-custodian --all-targets` — all green (no regressions from the
  mechanical `read_object_chunks` rename or the `committed_inode` signature change).
- `cargo test -p wyrd-chunkstore-grpc --test tier2_integration --no-run` — compiles
  (the test itself needs Docker-backed D servers via `cargo xtask integration`, out of
  this cycle's reach per its own `#[ignore]` gate — unrelated to this patch).
- `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets` — clean.
- `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian`
  — **all 11 properties green** (including `segmented_resolve_never_tears`,
  9 seconds .. 12.22s total for the full file).
- `typos docs/design/architecture/06-runtime-view.md` — clean.
- `python3 docs/publishing/tools/render_site.py --check` — "link audit OK".
- `cargo deny check` — the `advisories` leg fails on a pre-existing `event-listener`
  RUSTSEC advisory transitively via `madsim` → `wyrd-dst`; confirmed **identical**
  failure on the unmodified base tree (`git stash` + rerun) — not introduced by this
  patch, no new dependency added (`git diff --stat -- '*Cargo.toml' Cargo.lock` is
  empty).

No `cargo xtask ci` wholesale run (fmt+clippy+build+test+deny+conformance as one
invocation) — ran its constituent pieces individually above, each clean or
pre-existing-red as noted, because a full `cargo xtask ci` run (including the 50-seed
DST sweep across every Tier-0 file, not just `custodian.rs`) is the kind of long-running
invocation the Do beat's own guidance warns against hand-rolling without the project's
own timeout; the pieces run here cover every gate `cargo xtask ci` composes.

## Budget

Production semantic lines (non-blank, non-comment), counted from the diff: metadata.rs
271, read.rs 27 (net), server/lib.rs 29, custodian/lib.rs 1, custodian/resolve.rs 52
(production half) = **380**, well under the brief's own ~660 salvage estimate (this
slice is smaller because #651's repoint/homes surface and #650's five-loop containment
detail are cut, per Design choices above). Test semantic lines: core test file 573,
server test file 252, resolve.rs's own test module 158 = **983**. Docs: 1 line. Total
**≈1,364** against the ~1,500 budget. Files: 8 substantive (metadata.rs, read.rs,
resolve.rs, custodian/lib.rs, server/lib.rs, the two new test files, the docs file) —
well under 15; the 11 mechanically-migrated call-site files are declared separately per
the brief's explicit "allowed on top" carve-out for the `read_object_from` →
`read_object_chunks` pattern. No Cargo.toml/Cargo.lock change; no new dev-dependency;
no conformance-vector change.

## review-rejected.md

Not created. This is the bundle's first Do pass; no Check review has run yet to
produce a finding matching the brief's four standing "Do-not-re-earn" rejections
(caller-side fan-out timeout; retraction of already-published bytes; `Completed`
releasing its admission slot; any settled decision in the issue body) — none of which
this patch's design touches in the first place (verified against each: no caller-side
timeout added, nothing retracts published bytes, no admission-slot logic touched).
