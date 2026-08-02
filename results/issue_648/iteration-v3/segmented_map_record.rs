//! Issue #648, slice 1/6 of the #635 re-slicing (0016 decision 7(a)): the segmented
//! chunk-map **record shape and its codec** — `ChunkMap::{Flat, Segmented}`, the
//! decode-time structural invariants, and the `seg:`/`seggrp:` key helpers. This
//! slice lands no resolver and no producer (#649 onward); it binds entirely through
//! **base-visible** API (`wyrd_core::metadata::{encode, decode, InodeRecord}`) over
//! **raw stored bytes**.
//!
//! Every segmented case here is a hand-authored JSON byte string, decoded through
//! `decode::<InodeRecord>`. This file therefore imports nothing this patch adds
//! (no `ChunkMap`, `SegmentedMap`, `SegmentRef`, `SegmentGroup`, `seg_key`,
//! `ChunkMapError`, …) — only symbols already visible on `origin/main`. On a revert
//! of `crates/core/src/metadata.rs` (the RED leg), `chunk_map` is back to a bare
//! `Vec<ChunkRef>`, so this file still COMPILES (it names nothing new) and instead
//! fails as an **assertion**: a segmented root's raw bytes no longer decode
//! (criterion 2's positive case and criterion 3 both fail as assertions), which is
//! the falsifiability this test binds — never a compile error.
//!
//! The one place a number from the patched module is needed — criterion 3's
//! `MAX_ROOT_SEGMENTS`, `MAX_VALUE_BYTES` and the `MAX_ROOT_VALUE_BYTES` budget the
//! table is sized against (`0016:1467`) — reads it out of the module's **source text**
//! ([`production_constant`]) instead of copying it, so the ceiling this test measures is
//! the ceiling that ships, and a later slice moving any of those constants is judged
//! here rather than in production.

#![forbid(unsafe_code)]

use wyrd_core::metadata::{decode, encode, inode_key, InodeRecord};
use wyrd_traits::{CommitOutcome, MetadataStore, WriteBatch};

/// A well-formed 32-lowercase-hex segment-group nonce (`SEG_NONCE_HEX_LEN` on the
/// patched branch).
const NONCE: &str = "0123456789abcdef0123456789abcdef";

// ===========================================================================
// Criterion 1 — legacy round-trips byte-identically, and CAS still commits.
// ===========================================================================

/// A hand-authored legacy `inode:` value in **exactly** the shape `origin/main`
/// emits today: `chunk_map` a bare JSON array (mixed `EcScheme` wire shapes — a
/// unit-variant `"None"` and a struct-variant `{"ReedSolomon":{...}}` — to exercise
/// both), and `etag`/`content_type`/`modified` **absent** (they decode to `None` and
/// carry `skip_serializing_if`, so a legacy writer never emitted them,
/// `crates/core/src/metadata.rs:1015-1028` pre-patch numbering). Field order matches
/// `InodeRecord`'s declaration order (`size, chunk_map, state, version`), which is
/// also `serde_json`'s struct-field emission order.
///
/// This exact byte string was captured from `wyrd_core::metadata::encode` of the
/// equivalent record on the **patched** branch (`ChunkMap::Flat` serialises as a
/// bare array, unchanged from the pre-patch `Vec<ChunkRef>` derive) — so it is, by
/// construction, what `origin/main` already writes.
const LEGACY_BYTES: &[u8] = br#"{"size":11,"chunk_map":[{"id":7,"scheme":{"ReedSolomon":{"k":6,"m":3}},"len":11,"placement":[1,2,3,4,5,6,7,8,9]},{"id":8,"scheme":"None","len":3,"placement":[]}],"state":"Committed","version":3}"#;

#[test]
fn legacy_flat_record_round_trips_byte_identically() {
    let prior: InodeRecord =
        decode(LEGACY_BYTES).expect("a pre-existing legacy record must still decode");
    let re_encoded = encode(&prior);
    assert_eq!(
        re_encoded.as_ref(),
        LEGACY_BYTES,
        "decode -> encode must be the identity on a byte sequence origin/main already wrote, \
         or every CAS over a pre-existing record turns into a permanent Conflict"
    );
}

#[test]
fn legacy_flat_record_cas_still_commits_against_the_original_bytes() {
    pollster::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let store =
            wyrd_metadata_redb::RedbMetadataStore::open(dir.path().join("meta.redb")).unwrap();
        let key = inode_key(1);

        // Seed the store with the ORIGINAL legacy bytes, unconditionally — standing in
        // for a record `origin/main` already wrote before this patch existed.
        store
            .commit(WriteBatch::new().put(key.clone(), LEGACY_BYTES))
            .await
            .unwrap();

        let prior: InodeRecord = decode(LEGACY_BYTES).unwrap();

        // The precondition every CAS commit in `metadata.rs` uses:
        // `require(key, encode(prior))`. If decode -> encode were not the identity on
        // this record, this precondition would never match the bytes the store holds
        // and this commit would return `Conflict` instead.
        let outcome = store
            .commit(
                WriteBatch::new()
                    .require(key.clone(), encode(&prior))
                    .put(key, encode(&prior)),
            )
            .await
            .unwrap();
        assert_eq!(
            outcome,
            CommitOutcome::Committed,
            "require(key, encode(prior)) must commit against a store holding the original bytes"
        );
    });
}

// ===========================================================================
// Criterion 2 — a well-formed segmented root decodes; each decode invariant has
// its raw-byte negative case.
// ===========================================================================

/// A well-formed segmented root: two segments contiguously tiling `size` (0..5,
/// 5..12) under one segment group.
const SEGMENTED_ROOT_OK: &[u8] = br#"{"size":12,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;

#[test]
fn well_formed_segmented_root_decodes() {
    let record: std::result::Result<InodeRecord, _> = decode(SEGMENTED_ROOT_OK);
    assert!(
        record.is_ok(),
        "a well-formed segmented root (contiguous tiling spanning exactly `size`) must decode; got {:?}",
        record.err()
    );
}

#[test]
fn segment_count_mismatching_segments_len_is_err() {
    // `segment_count` claims 3 segments; only 2 are present.
    let bytes = br#"{"size":12,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":3,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(
        record.is_err(),
        "segment_count disagreeing with segments.len() must be Err"
    );
}

#[test]
fn duplicate_segment_index_is_err() {
    // Both segments claim index 0.
    let bytes = br#"{"size":12,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":0,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(record.is_err(), "a duplicate segment index must be Err");
}

#[test]
fn segment_index_gap_is_err() {
    // Indices 0, 2 — index 1 is missing.
    let bytes = br#"{"size":12,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":2,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(record.is_err(), "a gap in segment indices must be Err");
}

#[test]
fn overlapping_segment_spans_is_err() {
    // Segment 0 covers [0, 5); segment 1 claims to start at byte 3 — inside segment
    // 0's span, not at its end.
    let bytes = br#"{"size":12,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":3,"byte_len":9}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(record.is_err(), "an overlapping segment span must be Err");
}

#[test]
fn non_monotonic_segment_span_is_err() {
    // Segment 0 covers [0, 5); segment 1 starts at byte 6 — a gap, not the required
    // byte 5 — so the tiling is not contiguous.
    let bytes = br#"{"size":13,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":6,"byte_len":7}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(
        record.is_err(),
        "a non-monotonic (gapped) segment span must be Err"
    );
}

#[test]
fn non_hex_nonce_is_err() {
    // 31 lowercase-hex characters — one short of `SEG_NONCE_HEX_LEN` (32).
    let bytes = br#"{"size":12,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcd","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(
        record.is_err(),
        "a nonce that is not exactly SEG_NONCE_HEX_LEN lowercase hex characters must be Err"
    );
}

/// Bonus coverage for the cross-field invariant criterion 2's parenthetical names
/// ("contiguous tiling spanning exactly `size`"): a segment table that tiles
/// contiguously and internally-consistently, but whose total span disagrees with
/// the inode's declared `size`, must also be `Err` — the table is the object's byte
/// index, so a disagreement there is structural corruption.
#[test]
fn segment_table_span_disagreeing_with_size_is_err() {
    // The table spans 0..12 (5 + 7), but `size` claims 99.
    let bytes = br#"{"size":99,"chunk_map":{"group":{"nonce":"0123456789abcdef0123456789abcdef","epoch":1},"segment_count":2,"segments":[{"index":0,"byte_offset":0,"byte_len":5},{"index":1,"byte_offset":5,"byte_len":7}]},"state":"Committed","version":1}"#;
    let record: std::result::Result<InodeRecord, _> = decode(bytes);
    assert!(
        record.is_err(),
        "a segment table whose span disagrees with the declared size must be Err"
    );
}

// ===========================================================================
// Criterion 3 — a segmented root stays inside the value ceiling.
// ===========================================================================

/// The module whose constants are under judgment, read as **text** at compile time.
///
/// The two numbers criterion 3 weighs — how many segments a root may name, and the value
/// ceiling it must fit — live in `crates/core/src/metadata.rs`, and they are symbols this
/// patch ADDS, so this file cannot `use` them without becoming a compile red on
/// `origin/main` (see the file doc comment). Reading the source text keeps the test
/// **patch-aware anyway**: it measures whatever `MAX_ROOT_SEGMENTS` and `MAX_VALUE_BYTES`
/// actually say today, so raising the ceiling past what fits fails HERE rather than in
/// production, and a hard-coded 512 can never drift away from the constant it stands for.
/// On `origin/main` the constants are absent and [`production_constant`] fails as an
/// assertion — the same red as the rest of this file.
const METADATA_SOURCE: &str = include_str!("../src/metadata.rs");

/// The value of `pub const <name>: <ty> = <literal>;` in [`METADATA_SOURCE`], with `_`
/// digit separators removed. Panics with the constant's name when it is absent, which is
/// exactly what a run against a tree without this patch reports.
fn production_constant(name: &str) -> u64 {
    let needle = format!("pub const {name}:");
    let line = METADATA_SOURCE
        .lines()
        .find(|line| line.trim_start().starts_with(&needle))
        .unwrap_or_else(|| {
            panic!(
                "`pub const {name}` is not declared in crates/core/src/metadata.rs — the \
                 segmented chunk-map shape this test binds is not present in the tree"
            )
        });
    let literal = line
        .rsplit_once('=')
        .and_then(|(_, tail)| tail.split(';').next())
        .unwrap_or_else(|| panic!("`pub const {name}` has no `= <literal>;` value: {line}"));
    literal
        .trim()
        .replace('_', "")
        .parse()
        .unwrap_or_else(|e| panic!("`pub const {name}` is not an integer literal: {literal} ({e})"))
}

#[test]
fn segmented_root_at_max_root_segments_stays_inside_the_value_ceiling() {
    // Every bound comes from the production module itself, so this test judges the shape
    // that actually ships rather than a copy of its numbers.
    let max_root_segments = production_constant("MAX_ROOT_SEGMENTS");
    let max_value_bytes = production_constant("MAX_VALUE_BYTES");
    // The budget `MAX_ROOT_SEGMENTS` is actually sized against: HALF the value ceiling
    // (`0016:1467`, `max_segref_bytes × MAX_ROOT_SEGMENTS ≤ V / 2`). Asserting only the
    // full ceiling would pass a root at 50–100 KB — legal today, and unwritable after any
    // later field addition, which makes the placement of an already-published object
    // permanently unrepairable.
    let max_root_value_bytes = production_constant("MAX_ROOT_VALUE_BYTES");
    assert!(
        max_root_value_bytes * 2 <= max_value_bytes,
        "MAX_ROOT_VALUE_BYTES ({max_root_value_bytes}) must be at most HALF the \
         {max_value_bytes}-byte value ceiling, or the 2x headroom 0016:1467 requires of \
         MAX_ROOT_SEGMENTS is not what the root is measured against"
    );

    // The WORST CASE, not a convenient one: a root's encoded size is driven by the
    // decimal width of its numbers, so every segment is given the widest span the tiling
    // can carry — `byte_len` as large as `MAX_ROOT_SEGMENTS` equal segments allow before
    // the total leaves `u64`, which pushes the trailing `byte_offset`s to their maximum
    // width too — under a maximum-width epoch. The same table built from one-byte
    // segments encodes to 22 974 bytes against this one's 39 691: it would call a ceiling
    // of 2 000 segments "inside" (~92 KB) when the real encoding is ~154 KB, over.
    assert!(
        max_root_segments >= 2,
        "MAX_ROOT_SEGMENTS must name at least two segments to be a segmented map; got \
         {max_root_segments}"
    );
    let byte_len = u64::MAX / max_root_segments;
    let size = byte_len * max_root_segments;
    let mut segments = String::new();
    for i in 0..max_root_segments {
        if i > 0 {
            segments.push(',');
        }
        segments.push_str(&format!(
            r#"{{"index":{i},"byte_offset":{offset},"byte_len":{byte_len}}}"#,
            offset = i * byte_len,
        ));
    }
    let bytes = format!(
        r#"{{"size":{size},"chunk_map":{{"group":{{"nonce":"{NONCE}","epoch":{epoch}}},"segment_count":{max_root_segments},"segments":[{segments}]}},"state":"Committed","version":1}}"#,
        epoch = u64::MAX,
    );

    let record: InodeRecord = decode(bytes.as_bytes()).expect(
        "a table at the MAX_ROOT_SEGMENTS ceiling is not rejected at decode — the capacity \
         bound is enforced where the table becomes work (publication / ranged read), not here",
    );
    let re_encoded = encode(&record);
    assert!(
        re_encoded.len() as u64 <= max_value_bytes,
        "a root holding MAX_ROOT_SEGMENTS ({max_root_segments}) worst-case segments must stay \
         inside the {max_value_bytes}-byte value ceiling every backend inherits; got {} bytes",
        re_encoded.len()
    );
    // The binding bound: the ceiling with the headroom 0016:1467 sizes the segment table
    // against. A root that only just fits `MAX_VALUE_BYTES` is a root that cannot absorb
    // one more field — and a root that cannot be RE-written is an object whose placement
    // can never be repaired, which is the permanent failure mode this shape exists to
    // avoid.
    assert!(
        re_encoded.len() as u64 <= max_root_value_bytes,
        "a root holding MAX_ROOT_SEGMENTS ({max_root_segments}) worst-case segments must stay \
         inside the {max_root_value_bytes}-byte root budget (half the {max_value_bytes}-byte \
         value ceiling, 0016:1467); got {} bytes — lower MAX_ROOT_SEGMENTS rather than \
         spending the headroom",
        re_encoded.len()
    );
}
