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

#[test]
fn segmented_root_at_max_root_segments_stays_inside_the_value_ceiling() {
    // `MAX_ROOT_SEGMENTS` is 512 on the patched branch
    // (`crates/core/src/metadata.rs`). Hardcoded rather than imported: the constant
    // is a symbol this patch ADDS, and this file imports only symbols already
    // visible on `origin/main` (see the file doc comment).
    const MAX_ROOT_SEGMENTS: usize = 512;

    let mut segments = String::new();
    for i in 0..MAX_ROOT_SEGMENTS {
        if i > 0 {
            segments.push(',');
        }
        segments.push_str(&format!(
            r#"{{"index":{i},"byte_offset":{i},"byte_len":1}}"#
        ));
    }
    let bytes = format!(
        r#"{{"size":{n},"chunk_map":{{"group":{{"nonce":"{NONCE}","epoch":1}},"segment_count":{n},"segments":[{segments}]}},"state":"Committed","version":1}}"#,
        n = MAX_ROOT_SEGMENTS,
    );

    let record: InodeRecord = decode(bytes.as_bytes()).expect(
        "a table at the MAX_ROOT_SEGMENTS ceiling is not rejected at decode — the capacity \
         bound is enforced where the table becomes work (publication / ranged read), not here",
    );
    let re_encoded = encode(&record);
    assert!(
        re_encoded.len() <= 100_000,
        "a root holding MAX_ROOT_SEGMENTS segments must stay inside the 100 000-byte value \
         ceiling every backend inherits; got {} bytes",
        re_encoded.len()
    );
}
