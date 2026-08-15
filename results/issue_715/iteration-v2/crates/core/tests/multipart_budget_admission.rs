//! Issue #715 — the first **record value** child of #692 (proposal 0016, itself slice 1/7
//! of #636). The `mpuctl` admission ledger: `wyrd_core::multipart::{Budget,
//! AdmissionRecord, encode_record, decode_record, decode_admission_record}` and the format
//! maxima their decode-time rules are computed against. **Pure**: no store, no async, no
//! fixture beyond literals and `metadata::encode`.
//!
//! Legs, in the order the issue states them:
//! 1. **encoding-derived constants** — `MIN_CHUNKREF_BYTES` / `MIN_SEGREF_BYTES` are what
//!    `metadata::encode` actually emits for the narrowest legal `ChunkRef` / `SegmentRef`,
//!    and every wider spelling costs strictly more, so both value ceilings are tied to the
//!    encoder rather than to prose;
//! 2. **round trips** — `Budget` and `AdmissionRecord` through `encode_record` /
//!    `decode_record`, with decode → encode byte-identical (the CAS-identity rule);
//! 3. **the derivations** — `U_ref` and `MAX_SESSIONS` against **independent numeric
//!    oracles** (expected integers computed by hand in the comments, never re-derived from
//!    the production expression), on both branches of each `min`;
//! 4. **the six one-per-bound negations** — 1a, 1f-i, 1f-ii, 1f-iii, 1f-iv, 1g. Each torn
//!    tuple violates **exactly one** bound, and its legal twin sits *on* that bound, so the
//!    red proves that guard is load-bearing and not a neighbour's;
//! 5. **the identity/occupancy boundary** — a `count` above `max_sessions` still decodes.
//!
//! Every refusal is asserted as the typed [`RecordError::Structural`] **and** its reason,
//! never as a bare `is_err()`: a negation that only asserts "some error" stays green when
//! the guard under test is deleted and a neighbour refuses the same tuple.
//!
//! No `#![cfg(...)]` here (the gate's vacuous-green hazard) — this file always compiles
//! and always runs.

#![forbid(unsafe_code)]

use std::fmt::Debug;

use wyrd_core::metadata::{self, ChunkRef, EcScheme, SegmentRef};
use wyrd_core::multipart::{
    decode_admission_record, decode_record, encode_record, AdmissionRecord, Budget, RecordError,
    MAX_PART_CHUNKS_FORMAT_MAX, MAX_PUBLISHABLE_CHUNKS_FORMAT_MAX, MAX_ROOT_SEGMENTS_FORMAT_MAX,
    MAX_SEG_CHUNKS_FORMAT_MAX, MIN_CHUNKREF_BYTES, MIN_SEGREF_BYTES, SCAN_HALF, VALUE_CEILING_HALF,
};

// ===========================================================================
// Fixtures and helpers
// ===========================================================================

/// The profile 0016's own settled pairing describes (`0016:1463-1480`): 10,000 parts of at
/// most 165 chunks, 64 in flight, a `512 × 165 = 84_480`-chunk staged ceiling and a
/// million-chunk-ref reference budget. Legal on every rule.
fn baseline() -> Budget {
    Budget::new(1_000_000, 165, 10_000, 64, 84_480).expect("0016's own settled profile is legal")
}

/// The stored spelling of a profile, as `mpuctl` carries it.
fn budget_json(w_ref: u64, chunks: u32, parts: u32, inflight: u32, staged: u32) -> String {
    format!(
        "{{\"w_ref\":{w_ref},\"max_part_chunks\":{chunks},\"max_parts_per_session\":{parts},\
         \"max_inflight_parts\":{inflight},\"max_staged_chunks\":{staged}}}"
    )
}

/// The stored spelling of the whole record, over [`baseline`]'s profile.
fn admission_json(count: u64, max_sessions: u64) -> String {
    format!(
        "{{\"count\":{count},\"max_sessions\":{max_sessions},\"profile\":{}}}",
        budget_json(1_000_000, 165, 10_000, 64, 84_480)
    )
}

/// Assert a refusal is the typed [`RecordError::Structural`] for the `mpuctl` record **and**
/// that its reason cites `expected` — never a bare `is_err()`, which a *neighbouring* guard
/// would satisfy while the guard under test is gone.
#[track_caller]
fn assert_refused<T: Debug>(result: Result<T, RecordError>, expected: &str) {
    match result {
        Err(RecordError::Structural { record, reason }) => {
            assert_eq!(record, "mpuctl", "the error must name the record class");
            assert!(
                reason.contains(expected),
                "the refusal must cite {expected:?}, got {reason:?}"
            );
        }
        other => panic!("expected a typed Structural mpuctl error, got {other:?}"),
    }
}

/// Both gates on one tuple: the constructor and the **decode** path. A rule is only enforced
/// if a *stored* record cannot smuggle it past, so every negation asserts on this pair.
fn gates(
    w_ref: u64,
    chunks: u32,
    parts: u32,
    inflight: u32,
    staged: u32,
) -> (Result<Budget, RecordError>, Result<Budget, RecordError>) {
    (
        Budget::new(w_ref, chunks, parts, inflight, staged),
        decode_record::<Budget>(
            "mpuctl",
            budget_json(w_ref, chunks, parts, inflight, staged).as_bytes(),
        ),
    )
}

/// Assert a tuple is refused by **both** gates, each for `expected_reason`.
#[track_caller]
fn rejected_by_both(
    tuple: (Result<Budget, RecordError>, Result<Budget, RecordError>),
    expected_reason: &str,
) {
    assert_refused(tuple.0, expected_reason);
    assert_refused(tuple.1, expected_reason);
}

/// Assert a tuple is accepted by **both** gates — the legal twin sitting exactly *on* a
/// bound, which is what makes the negation beside it an isolation rather than a coincidence.
#[track_caller]
fn accepted_by_both(tuple: (Result<Budget, RecordError>, Result<Budget, RecordError>)) -> Budget {
    let constructed = tuple.0.expect("the tuple on the bound must be accepted");
    assert_eq!(
        tuple.1.expect("the stored tuple on the bound must decode"),
        constructed,
        "the constructor and the decoder must agree on the same tuple"
    );
    constructed
}

/// The narrowest `ChunkRef` the format admits: every field at its shortest spelling
/// (`metadata.rs:128-140`).
fn narrowest_chunkref() -> ChunkRef {
    ChunkRef {
        id: 0,
        scheme: EcScheme::None,
        len: 0,
        placement: Vec::new(),
    }
}

fn chunkref_width(chunk: ChunkRef) -> usize {
    metadata::encode(&chunk).len()
}

// ===========================================================================
// 1. The format constants are what the encoder actually emits
// ===========================================================================

#[test]
fn min_chunkref_bytes_is_the_measured_encoding_minimum() {
    assert_eq!(
        chunkref_width(narrowest_chunkref()),
        MIN_CHUNKREF_BYTES,
        "MIN_CHUNKREF_BYTES must be what `metadata::encode` emits for the narrowest legal \
         ChunkRef, not a number from prose — a field added to ChunkRef must break this test"
    );

    // Every dimension that can widen, widened one at a time: each costs strictly more, which
    // is what makes the witness above the *minimum* rather than one small case.
    let mut wider = narrowest_chunkref();
    wider.id = u128::MAX;
    assert!(chunkref_width(wider) > MIN_CHUNKREF_BYTES, "a wider id");
    let mut wider = narrowest_chunkref();
    wider.len = 32 * 1024 * 1024;
    assert!(chunkref_width(wider) > MIN_CHUNKREF_BYTES, "a longer chunk");
    let mut wider = narrowest_chunkref();
    wider.scheme = EcScheme::ReedSolomon { k: 6, m: 3 };
    assert!(chunkref_width(wider) > MIN_CHUNKREF_BYTES, "an EC scheme");
    let mut wider = narrowest_chunkref();
    wider.placement = vec![7];
    assert!(chunkref_width(wider) > MIN_CHUNKREF_BYTES, "a placement");

    // The real worst case the #715 iteration-1 review measured — a legal-width 32 MiB RS(6,3)
    // chunk with a full placement — is far *above* the minimum, and that is the point of
    // deriving the maximum from the narrowest width: a deployment sizing its own
    // `MAX_PART_CHUNKS` against **its** `b_ref` (`0016:1063`) can never land past the format
    // maximum, so no legally written record is ever refused at decode (`0016:390-402`).
    let realistic = chunkref_width(ChunkRef {
        id: u128::MAX,
        scheme: EcScheme::ReedSolomon { k: 6, m: 3 },
        len: 32 * 1024 * 1024,
        placement: (0..9).map(|i| u64::MAX - i).collect(),
    });
    assert!(realistic > MIN_CHUNKREF_BYTES);
    assert!(
        (VALUE_CEILING_HALF / realistic) as u32 <= MAX_PART_CHUNKS_FORMAT_MAX,
        "a deployment's own knob, floor({VALUE_CEILING_HALF} / {realistic}), must sit inside \
         the format maximum {MAX_PART_CHUNKS_FORMAT_MAX}"
    );
}

#[test]
fn min_segref_bytes_is_the_measured_encoding_minimum() {
    let width = |segment: SegmentRef| metadata::encode(&segment).len();
    assert_eq!(
        width(SegmentRef {
            index: 0,
            byte_offset: 0,
            byte_len: 0
        }),
        MIN_SEGREF_BYTES,
        "MIN_SEGREF_BYTES must be what `metadata::encode` emits for the narrowest SegmentRef"
    );
    assert!(
        width(SegmentRef {
            index: 999_999,
            byte_offset: u64::MAX,
            byte_len: u64::MAX
        }) > MIN_SEGREF_BYTES
    );
}

#[test]
fn the_value_ceilings_are_the_largest_counts_their_rule_admits() {
    // `max_chunkref_bytes x N <= V / 2` (`0016:1466-1467`), at the format-minimal width.
    assert_eq!(VALUE_CEILING_HALF, 50_000);
    assert_eq!(MAX_SEG_CHUNKS_FORMAT_MAX, 1_063); // floor(50_000 / 47)
    assert_eq!(MAX_PART_CHUNKS_FORMAT_MAX, MAX_SEG_CHUNKS_FORMAT_MAX);
    assert_eq!(MAX_ROOT_SEGMENTS_FORMAT_MAX, 1_250); // floor(50_000 / 40)
    assert_eq!(MAX_PUBLISHABLE_CHUNKS_FORMAT_MAX, 1_250 * 1_063);

    // Each is the LARGEST count its rule admits: one more breaches the budget it came from.
    let seg = MAX_SEG_CHUNKS_FORMAT_MAX as usize;
    let root = MAX_ROOT_SEGMENTS_FORMAT_MAX as usize;
    assert!(seg * MIN_CHUNKREF_BYTES <= VALUE_CEILING_HALF);
    assert!((seg + 1) * MIN_CHUNKREF_BYTES > VALUE_CEILING_HALF);
    assert!(root * MIN_SEGREF_BYTES <= metadata::MAX_ROOT_VALUE_BYTES);
    assert!((root + 1) * MIN_SEGREF_BYTES > metadata::MAX_ROOT_VALUE_BYTES);

    // The format maximum is reachable, not a fantasy: a whole table of that many narrowest
    // chunk-refs still encodes inside the value ceiling every backend inherits
    // (`metadata.rs:327`). It spends more than the V/2 *budget* only because 0016's rule
    // counts refs, not the array's separators — the deliberate slack that keeps the maximum
    // permissive, the direction a decode-time bound must err in (`0016:390-402`).
    let full_segment: Vec<ChunkRef> = (0..seg).map(|_| narrowest_chunkref()).collect();
    let encoded = metadata::encode(&full_segment).len();
    assert!(
        encoded <= metadata::MAX_VALUE_BYTES,
        "{seg} narrowest chunk-refs encode to {encoded} bytes, past the {} the tightest \
         backend accepts",
        metadata::MAX_VALUE_BYTES
    );

    // The deployment's capacity number stays inside the format maximum it is chosen under
    // (`metadata.rs:302-322`), and the format maximum stays inside the `seg:` key space
    // (`metadata.rs:275-286`).
    assert!(metadata::MAX_ROOT_SEGMENTS <= root);
    assert!(u64::from(MAX_ROOT_SEGMENTS_FORMAT_MAX) <= u64::from(metadata::MAX_SEGMENT_INDEX) + 1);

    // The scan clamp is SCAN_CAP/2, the store-seam constant — not a multipart knob.
    assert_eq!(SCAN_HALF, (wyrd_traits::SCAN_CAP / 2) as u64);
    assert_eq!(SCAN_HALF, 524_288);
}

// ===========================================================================
// 2. Round trips through the shared record envelope
// ===========================================================================

#[test]
fn budget_round_trips_through_the_record_envelope() {
    let stored = encode_record(&baseline());
    assert_eq!(
        String::from_utf8(stored.clone()).expect("records are JSON text"),
        budget_json(1_000_000, 165, 10_000, 64, 84_480),
        "the stored spelling is the tuple's five fields in declaration order"
    );

    let decoded: Budget = decode_record("mpuctl", &stored).expect("a legal profile must decode");
    assert_eq!(decoded, baseline());
    assert_eq!(
        encode_record(&decoded),
        stored,
        "decode -> encode must be the identity, or every `require(mpuctl == prior)` CAS the \
         admission reservation depends on stops matching the bytes in the store"
    );
}

#[test]
fn admission_record_round_trips_through_its_own_arm() {
    let record = AdmissionRecord::new(3, baseline());
    assert_eq!(record.count(), 3);
    assert_eq!(record.max_sessions(), 9, "derived, never supplied");
    assert_eq!(*record.profile(), baseline());

    let stored = encode_record(&record);
    assert_eq!(
        String::from_utf8(stored.clone()).expect("records are JSON text"),
        admission_json(3, 9)
    );
    let decoded = decode_admission_record(&stored).expect("a legal mpuctl record must decode");
    assert_eq!(decoded, record);
    assert_eq!(decoded.count(), 3);
    assert_eq!(decoded.max_sessions(), 9);
    assert_eq!(*decoded.profile(), baseline());
    assert_eq!(
        encode_record(&decoded),
        stored,
        "decode -> encode must be the identity on the whole-record CAS value"
    );
}

#[test]
fn a_stored_record_with_an_unexpected_or_missing_field_is_refused() {
    // Whole-record CAS values: an unknown field silently dropped on decode would be silently
    // *deleted* by the next `decode -> mutate -> encode` cycle (the serialization-identity
    // class, `AGENTS.md:170-174`).
    let profile_with_extra = "{\"w_ref\":1000000,\"max_part_chunks\":165,\
                              \"max_parts_per_session\":10000,\"max_inflight_parts\":64,\
                              \"max_staged_chunks\":84480,\"b_ops\":900}";
    assert_refused(
        decode_record::<Budget>("mpuctl", profile_with_extra.as_bytes()),
        "b_ops",
    );
    let record_with_extra = format!(
        "{{\"count\":3,\"max_sessions\":9,\"profile\":{},\"reserved\":true}}",
        budget_json(1_000_000, 165, 10_000, 64, 84_480)
    );
    assert_refused(
        decode_admission_record(record_with_extra.as_bytes()),
        "reserved",
    );

    // ... and a missing field is not defaulted (ADR-0045: never a silently-corrected default).
    assert_refused(
        decode_admission_record(b"{\"count\":3,\"max_sessions\":9}"),
        "profile",
    );
    assert_refused(
        decode_record::<Budget>("mpuctl", b"{\"w_ref\":1000000}"),
        "max_part_chunks",
    );
}

#[test]
fn a_structural_error_says_which_record_and_why() {
    let error = RecordError::Structural {
        record: "mpuctl",
        reason: "max_sessions=2 disagrees with the 1 its own profile derives".to_string(),
    };
    assert_eq!(
        error.to_string(),
        "invalid mpuctl record: max_sessions=2 disagrees with the 1 its own profile derives"
    );
}

// ===========================================================================
// 3. The derivations, against independent numeric oracles
// ===========================================================================

#[test]
fn u_ref_is_the_smaller_of_the_raw_and_ceiling_terms() {
    // Oracle A — the RAW term binds. parts=40, inflight=10, part_chunks=7, staged=280:
    //   raw     = (40 + 10) x 7                    = 350
    //   ceiling = 280 + 2 x 10 x 7 = 280 + 140     = 420
    //   U_ref   = min(350, 420)                    = 350
    assert_eq!(
        Budget::new(100_000, 7, 40, 10, 280).expect("legal").u_ref(),
        350
    );

    // Oracle B — the CEILING term binds. parts=50, inflight=3, part_chunks=9, staged=100:
    //   raw     = (50 + 3) x 9                     = 477
    //   ceiling = 100 + 2 x 3 x 9 = 100 + 54       = 154
    //   U_ref   = min(477, 154)                    = 154
    assert_eq!(
        Budget::new(100_000, 9, 50, 3, 100).expect("legal").u_ref(),
        154
    );

    // Oracle C — 0016's own settled pairing (`0016:1463-1480`):
    //   raw     = (10_000 + 64) x 165                     = 1_660_560
    //   ceiling = 84_480 + 2 x 64 x 165 = 84_480 + 21_120 = 105_600
    //   U_ref   = min(1_660_560, 105_600)                 = 105_600
    assert_eq!(baseline().u_ref(), 105_600);
}

#[test]
fn max_sessions_is_the_floor_quotient_clamped_to_the_scan_bound() {
    // Oracle A — the QUOTIENT binds, and it truncates. U_ref = 350 (oracle A above):
    //   floor(1_000 / 350) = 2   (not 2.857..., and not 3)
    let sessions = |w_ref| {
        Budget::new(w_ref, 7, 40, 10, 280)
            .expect("legal")
            .max_sessions()
    };
    assert_eq!(sessions(1_000), 2);
    assert_eq!(sessions(1_049), 2, "one chunk-ref short of a third session");
    assert_eq!(sessions(1_050), 3, "exactly three sessions' worth");

    // Oracle B — the SCAN_CAP/2 CLAMP binds, and it is load-bearing: W_ref is sized from host
    // RAM and U_ref from the caps, so a legal pairing (a large W_ref, minimal parts) makes the
    // quotient exceed the cap on the reaper's one `scan("mpu:")` (`0016:1470`).
    //   U_ref = min((1 + 1) x 1, 1 + 2 x 1 x 1) = min(2, 3) = 2
    //   floor(4_000_000 / 2) = 2_000_000, clamped to SCAN_CAP/2 = 524_288
    let clamped = Budget::new(4_000_000, 1, 1, 1, 1).expect("legal");
    assert_eq!(clamped.u_ref(), 2);
    assert_eq!(clamped.max_sessions(), 524_288);
    assert_eq!(clamped.max_sessions(), SCAN_HALF);

    // 0016's settled pairing: floor(1_000_000 / 105_600) = 9, under the clamp.
    assert_eq!(baseline().max_sessions(), 9);
}

#[test]
fn the_accessors_return_the_tuple_the_profile_was_built_from() {
    let profile = Budget::new(100_000, 7, 40, 10, 280).expect("legal");
    assert_eq!(profile.w_ref(), 100_000);
    assert_eq!(profile.max_part_chunks(), 7);
    assert_eq!(profile.max_parts_per_session(), 40);
    assert_eq!(profile.max_inflight_parts(), 10);
    assert_eq!(profile.max_staged_chunks(), 280);
}

#[test]
fn a_zero_component_is_refused_by_both_gates() {
    // A zero derives U_ref = 0, and MAX_SESSIONS would be a division by zero, not a limit.
    rejected_by_both(gates(0, 7, 40, 10, 280), "`w_ref` is zero");
    rejected_by_both(gates(100_000, 0, 40, 10, 280), "`max_part_chunks` is zero");
    rejected_by_both(
        gates(100_000, 7, 0, 10, 280),
        "`max_parts_per_session` is zero",
    );
    rejected_by_both(
        gates(100_000, 7, 40, 0, 280),
        "`max_inflight_parts` is zero",
    );
    rejected_by_both(gates(100_000, 7, 40, 10, 0), "`max_staged_chunks` is zero");
}

#[test]
fn w_ref_below_one_session_s_footprint_is_refused() {
    // U_ref = 350 (oracle A). W_ref == U_ref admits exactly one session; one below admits
    // none, so the profile could never let a session in (`0016:1479`).
    assert_eq!(
        accepted_by_both(gates(350, 7, 40, 10, 280)).max_sessions(),
        1
    );
    rejected_by_both(gates(349, 7, 40, 10, 280), "is below the U_ref=350");
}

#[test]
fn a_part_number_space_past_the_key_grammar_is_refused() {
    // `MAX_PART_NUMBER` = 999_999 (`multipart.rs:271-277`) — a format bound of the key
    // grammar, not a capacity knob.
    accepted_by_both(gates(10_000_000, 1, 999_999, 1, 1));
    rejected_by_both(
        gates(10_000_000, 1, 1_000_000, 1, 1),
        "exceeds the part-number key space",
    );
}

// ===========================================================================
// 4. The six negations — one per independently enforced bound
// ===========================================================================

/// **1a** — a stored `max_sessions` that disagrees with what its own profile derives. Torn
/// **either** way: a higher stored limit admits sessions past the memory bound the reconcile
/// pass is sized for, and a lower one is equally a disagreement — the profile is what
/// establishes the limit, so the two spellings are one quantity (`0016:1470`).
#[test]
fn leg_1a_a_torn_max_sessions_is_refused_at_decode() {
    let decoded = decode_admission_record(admission_json(7, 9).as_bytes())
        .expect("the derived limit decodes");
    assert_eq!(decoded.max_sessions(), 9);

    assert_refused(
        decode_admission_record(admission_json(7, 10).as_bytes()),
        "max_sessions=10 disagrees with the 9",
    );
    assert_refused(
        decode_admission_record(admission_json(7, 8).as_bytes()),
        "max_sessions=8 disagrees with the 9",
    );
}

/// **1f-i** — the value-ceiling rule on `max_part_chunks`. Isolating: at both values the
/// staged ceiling, the scan bound, the in-flight cap and `W_ref ≥ U_ref` stay satisfied.
#[test]
fn leg_1f_i_max_part_chunks_past_the_value_ceiling_is_refused() {
    let on_the_bound = accepted_by_both(gates(4_000, 1_063, 2, 1, 1_063));
    assert_eq!(on_the_bound.max_part_chunks(), MAX_PART_CHUNKS_FORMAT_MAX);
    rejected_by_both(
        gates(4_000, 1_064, 2, 1, 1_064),
        "exceeds the value-ceiling format maximum 1063",
    );
}

/// **1f-ii** — the lower end of `max_staged_chunks`: at least one maximal part must remain
/// stageable (`0016:1472`).
#[test]
fn leg_1f_ii_a_staged_ceiling_below_one_maximal_part_is_refused() {
    accepted_by_both(gates(5_000, 50, 50, 10, 50));
    rejected_by_both(
        gates(5_000, 50, 50, 10, 49),
        "max_staged_chunks=49 is below max_part_chunks=50",
    );
}

/// **1f-iii** — the upper end of `max_staged_chunks`: a session may stage no more than it
/// could publish. The isolating tuple sits far below the scan bound and the format
/// publishable ceiling, so the only rule it can break is the one between its own two fields
/// (`max_parts_per_session × max_part_chunks = 100`).
#[test]
fn leg_1f_iii_a_staged_ceiling_past_the_publishable_one_is_refused() {
    let on_the_bound = accepted_by_both(gates(1_000, 10, 10, 5, 100));
    assert_eq!(
        u64::from(on_the_bound.max_staged_chunks()),
        u64::from(on_the_bound.max_parts_per_session()) * u64::from(on_the_bound.max_part_chunks())
    );
    rejected_by_both(
        gates(1_000, 10, 10, 5, 101),
        "max_staged_chunks=101 exceeds the publishable ceiling 100",
    );
}

/// **1f-iv** — `max_inflight_parts ≤ max_parts_per_session` (`0016:1476`, iteration-13
/// finding 2): a session cannot have more parts in flight than it may ever hold.
#[test]
fn leg_1f_iv_more_parts_in_flight_than_a_session_may_hold_is_refused() {
    accepted_by_both(gates(1_000, 10, 10, 10, 100));
    rejected_by_both(
        gates(1_000, 10, 10, 11, 100),
        "max_inflight_parts=11 exceeds max_parts_per_session=10",
    );
}

/// **1g** — the per-session staging scan bound, counting **committed** staging entries as
/// well as in-flight chunks. The isolating pair moves `max_staged_chunks` by one across
/// exactly `SCAN_CAP/2`: at both values the tuple is legal on every other rule (staged
/// ceiling 1_288 against a publishable 523_000, in-flight cap equal to its part space, and
/// `W_ref` covering `U_ref`).
#[test]
fn leg_1g_a_staging_scan_past_scan_cap_half_is_refused() {
    // 1_288 + 523 x 1_000 = 524_288 = SCAN_CAP/2, exactly on the bound.
    let on_the_bound = accepted_by_both(gates(2_000_000, 1_000, 523, 523, 1_288));
    assert_eq!(
        u64::from(on_the_bound.max_staged_chunks())
            + u64::from(on_the_bound.max_inflight_parts())
                * u64::from(on_the_bound.max_part_chunks()),
        SCAN_HALF
    );
    rejected_by_both(
        gates(2_000_000, 1_000, 523, 523, 1_289),
        "= 524289 exceeds the per-session staging scan bound 524288",
    );

    // The in-flight term alone cannot be the whole charge: a profile whose *committed*
    // staging entries push the same scan past the cap is refused too (the #692 batch-review
    // blocker — counting only in-flight chunks admits a session whose teardown scan is
    // silently incomplete).
    rejected_by_both(
        gates(2_000_000, 1_000, 600, 600, 1_000),
        "exceeds the per-session staging scan bound",
    );
}

// ===========================================================================
// 5. The identity/occupancy boundary — binding the other way
// ===========================================================================

/// A decoded `AdmissionRecord` whose `count` exceeds its `max_sessions` **still decodes**:
/// occupancy above a lowered cap is a legitimate live state, not a torn record
/// (`0016:390-402`; the same liberal-on-read boundary `metadata.rs:312-321` draws). Identity
/// relations are binding; occupancy relations are not.
#[test]
fn leg_3_occupancy_above_the_limit_still_decodes() {
    let decoded = decode_admission_record(admission_json(4_096, 9).as_bytes()).expect(
        "a count above max_sessions is live occupancy, not a decode error — rejecting it \
         would make the very record the drain must read unreadable",
    );
    assert_eq!(decoded.count(), 4_096);
    assert_eq!(decoded.max_sessions(), 9);
    assert!(decoded.count() > decoded.max_sessions());

    // The record a fresh store initializes, and one at exactly its cap, decode too.
    let count_of = |json: String| {
        decode_admission_record(json.as_bytes())
            .expect("a legal ledger decodes")
            .count()
    };
    assert_eq!(count_of(admission_json(0, 9)), 0);
    assert_eq!(count_of(admission_json(9, 9)), 9);
}
