//! Issue #715 — slice 2/3 of #654's own re-split (itself slice 1/7 of #636, proposal 0016).
//! The multipart **admission ledger**: `wyrd_core::multipart::{Budget, AdmissionRecord,
//! decode_admission_record}` — the `mpuctl` singleton's record value, its budget profile and
//! the two derivations that profile establishes (`U_ref`, `MAX_SESSIONS`). **Pure**: no
//! store, no async, no fixture beyond hand-authored JSON bytes and the production codec.
//!
//! Every witness here is **decoded, never constructed**: the bytes are written out by hand
//! and fed to the two surfaces one validation serves —
//!
//! * **S2** `decode_admission_record`, which attributes a rejection to **one** typed
//!   [`RecordError`] variant (every guard assertion below is made through it);
//! * **S1** `metadata::decode::<AdmissionRecord>`, the store-wide codec any consumer holding
//!   the type uses, where the same rules apply but the failure arrives untyped.
//!
//! `decode_both` runs both on every witness and asserts they agree, so no rule can be
//! enforced on one surface and missed on the other.
//!
//! The legs:
//! * **round trip** — every field survives decode→encode→decode, and the re-encode is
//!   byte-identical to the stored bytes (`mpuctl` is CAS'd whole, `0016:348`);
//! * **G1–G8** — one isolating witness per guard, each violating only its own rule, so a
//!   guard that stopped being enforced shows up here and nowhere else;
//! * **P1–P3** — the other direction: a decoder that refuses a stored record on a number
//!   this deployment merely chose is as wrong as one that admits a torn record;
//! * **P-arith** — decode's verdict equals the verdict exact (unbounded) integer arithmetic
//!   gives, at the field maxima, with no panic and no wrap.
//!
//! No `#![cfg(...)]` here (the gate's vacuous-green hazard) — this file always compiles and
//! always runs.

#![forbid(unsafe_code)]

use wyrd_core::metadata;
use wyrd_core::multipart::{decode_admission_record, AdmissionRecord, RecordError};

/// `SCAN_CAP/2` — the seam constant two of the record's rules are stated against
/// (`crates/traits/src/lib.rs:286`, `SCAN_CAP = 1 << 20`; `0016:1470`, `:1471`).
const SCAN_HALF: u64 = (wyrd_traits::SCAN_CAP as u64) / 2;

/// One `mpuctl` value's **stored bytes**, hand-authored: field names and order exactly as
/// `0016:348` states them, no spaces, so an equality against `metadata::encode` is a
/// byte-identity claim and not a JSON-shape one.
fn ledger(
    count: u64,
    max_sessions: u64,
    w_ref: u64,
    mpc: u32,
    mpps: u32,
    mip: u32,
    msc: u32,
) -> Vec<u8> {
    format!(
        "{{\"count\":{count},\"max_sessions\":{max_sessions},\"profile\":{{\"w_ref\":{w_ref},\
         \"max_part_chunks\":{mpc},\"max_parts_per_session\":{mpps},\
         \"max_inflight_parts\":{mip},\"max_staged_chunks\":{msc}}}}}"
    )
    .into_bytes()
}

/// Decode `bytes` through **both** surfaces and assert they agree — same verdict, and on
/// success the same value. Returns S2's typed result, which every guard assertion reads.
fn decode_both(bytes: &[u8]) -> Result<AdmissionRecord, RecordError> {
    let typed = decode_admission_record(bytes);
    let untyped = metadata::decode::<AdmissionRecord>(bytes);
    assert_eq!(
        typed.is_ok(),
        untyped.is_ok(),
        "S1 and S2 disagree on {}: S2={typed:?}, S1={untyped:?}",
        String::from_utf8_lossy(bytes)
    );
    if let (Ok(from_s2), Ok(from_s1)) = (&typed, &untyped) {
        assert_eq!(
            from_s2, from_s1,
            "the two surfaces decoded different values"
        );
    }
    typed
}

/// The witness both surfaces must accept, and the one every guard witness is a single-field
/// perturbation of: `U_ref = min((100+10)x165, 20_000 + 2x10x165) = min(18_150, 23_300)`, so
/// `max_sessions = min(72_600 / 18_150, 524_288) = 4`.
fn legal_ledger() -> Vec<u8> {
    ledger(3, 4, 72_600, 165, 100, 10, 20_000)
}

// ===========================================================================
// Round trip — every field preserved, and decode->encode is byte-identical
// ===========================================================================

#[test]
fn round_trip_preserves_every_field() {
    let bytes = legal_ledger();
    let record = decode_both(&bytes).expect("the legal ledger decodes");

    assert_eq!(record.count(), 3);
    assert_eq!(record.max_sessions(), 4);
    let profile = record.profile();
    assert_eq!(profile.w_ref(), 72_600);
    assert_eq!(profile.max_part_chunks(), 165);
    assert_eq!(profile.max_parts_per_session(), 100);
    assert_eq!(profile.max_inflight_parts(), 10);
    assert_eq!(profile.max_staged_chunks(), 20_000);

    // The two derivations, from the record's OWN stored profile (`0016:1469`, `:1470`).
    assert_eq!(profile.u_ref(), 18_150);
    assert_eq!(profile.max_sessions(), 4);

    // `mpuctl` is CAS'd whole, so decode->encode must be the identity on stored bytes: a
    // re-encode that differed would either conflict forever or silently rewrite the record.
    let encoded = metadata::encode(&record);
    assert_eq!(encoded.as_ref(), bytes.as_slice());
    assert_eq!(
        decode_admission_record(encoded.as_ref()).expect("re-encoded bytes decode"),
        record
    );
}

// ===========================================================================
// G1–G8 — one isolating witness per guard, asserted through S2's typed variant
// ===========================================================================

/// **G1**, the totality precondition (`0016:1466`): at `max_part_chunks = 0` the `U_ref` of
/// `0016:1469` is `0`, so `MAX_SESSIONS`' quotient has no divisor. Unlike G2–G8 this one
/// cannot be isolated — no value violates G1 while satisfying G8, because G8 is undefined
/// there — so what it pins is that the record is refused *before* the derivation runs.
#[test]
fn g1_zero_part_chunks_is_refused_before_the_derivation() {
    let bytes = ledger(0, 1, 2, 0, 1, 1, 1);
    assert_eq!(decode_both(&bytes), Err(RecordError::MaxPartChunksZero));

    // The same record with the single field made legal decodes: `U_ref = min(2, 3) = 2`,
    // `max_sessions = min(2 / 2, 524_288) = 1`.
    let repaired = ledger(0, 1, 2, 1, 1, 1, 1);
    assert!(decode_both(&repaired).is_ok());
}

/// **G2** (`0016:1471`): at `max_inflight_parts = 0` no slot can ever be reserved, so no part
/// can be committed. Isolating — `U_ref = min(1x1, 1 + 0) = 1` and `max_sessions =
/// min(1 / 1, 524_288) = 1`, so G1 and G3–G8 all hold on this witness.
#[test]
fn g2_zero_inflight_parts_is_refused() {
    let bytes = ledger(0, 1, 1, 1, 1, 0, 1);
    assert_eq!(decode_both(&bytes), Err(RecordError::MaxInflightPartsZero));
}

/// **G3** — the `part:` key space (`multipart.rs`'s `MAX_PART_NUMBER`), the only bound
/// `0016`'s knob table leaves for `MAX_PARTS_PER_SESSION`. Isolating: `U_ref = min(1_000_001,
/// 3) = 3`, `max_sessions = min(3 / 3, 524_288) = 1`.
#[test]
fn g3_parts_per_session_past_the_key_space_is_refused() {
    let past = ledger(0, 1, 3, 1, 1_000_000, 1, 1);
    assert_eq!(
        decode_both(&past),
        Err(RecordError::MaxPartsPerSessionUnaddressable {
            max_parts_per_session: 1_000_000,
        })
    );

    // The largest addressable value is accepted — the bound is the key space, not a knob.
    let at_bound = ledger(0, 1, 3, 1, 999_999, 1, 1);
    assert!(decode_both(&at_bound).is_ok());
}

/// **G4** (`0016:1471` clamp 1, iteration-13 finding 2). Isolating: `U_ref = min((10+20)x1,
/// 100 + 2x20x1) = 30` and `max_sessions = min(1000 / 30, 524_288) = 33`, so only G4 fails.
#[test]
fn g4_more_parts_in_flight_than_the_session_may_hold_is_refused() {
    let bytes = ledger(0, 33, 1_000, 1, 10, 20, 100);
    assert_eq!(
        decode_both(&bytes),
        Err(RecordError::InflightPartsExceedSessionParts {
            max_inflight_parts: 20,
            max_parts_per_session: 10,
        })
    );
}

/// **G5** (`0016:1471`, `:2098`) — owned `sidx:` per session must stay inside one
/// complete-or-fail scan. Isolating at one past the ceiling: `U_ref = min(1_048_578,
/// 1_048_579) = 1_048_578`, `max_sessions = min(1, 524_288) = 1`.
#[test]
fn g5_owned_staging_range_past_scan_cap_half_is_refused() {
    let past = ledger(0, 1, 1_048_578, 1, 524_289, 524_289, 1);
    assert_eq!(
        decode_both(&past),
        Err(RecordError::OwnedStagingRangeUnscannable {
            max_inflight_parts: 524_289,
            max_part_chunks: 1,
        })
    );

    // Exactly at `SCAN_CAP/2` the record is legal — the rule is `<=`, not `<`.
    let at_bound = ledger(0, 1, 2 * SCAN_HALF, 1, 524_288, 524_288, 1);
    assert!(decode_both(&at_bound).is_ok());
}

/// **G6** (`0016:1468`, the lower end of `MAX_STAGED_CHUNKS`' settled range): at least one
/// maximal part must remain stageable. Isolating: `U_ref = min(2x165, 164 + 2x165) = 330`,
/// `max_sessions = min(330 / 330, 524_288) = 1`.
#[test]
fn g6_staged_ceiling_below_one_maximal_part_is_refused() {
    let below = ledger(0, 1, 330, 165, 1, 1, 164);
    assert_eq!(
        decode_both(&below),
        Err(RecordError::StagedChunksBelowPartChunks {
            max_staged_chunks: 164,
            max_part_chunks: 165,
        })
    );

    // Equality is the lower end of the range, so it is accepted.
    let at_bound = ledger(0, 1, 330, 165, 1, 1, 165);
    assert!(decode_both(&at_bound).is_ok());
}

/// **G7** (`0016:1473`, `W_ref`'s range `[U_ref, deployment RAM]`). Isolating: `U_ref =
/// min(2, 3) = 2` against `w_ref = 1`, and the stored `max_sessions = 0` is exactly what that
/// pairing derives — so the record satisfies G8 and fails only G7.
#[test]
fn g7_reference_budget_below_one_session_is_refused() {
    let bytes = ledger(0, 0, 1, 1, 1, 1, 1);
    assert_eq!(
        decode_both(&bytes),
        Err(RecordError::ReferenceBudgetBelowFootprint { w_ref: 1, u_ref: 2 })
    );
}

/// **G8**, the identity this record exists for (`0016:1470`): the legal ledger with its
/// `max_sessions` alone moved off what its own profile derives.
#[test]
fn g8_stored_limit_disagreeing_with_its_own_profile_is_refused() {
    let bytes = ledger(3, 5, 72_600, 165, 100, 10, 20_000);
    assert_eq!(
        decode_both(&bytes),
        Err(RecordError::MaxSessionsNotDerived {
            stored: 5,
            derived: 4,
        })
    );

    // The untyped surface refuses it too — the funnel stringifies the reason, so the only
    // claim S1 can make is that a torn ledger does not decode into a value.
    let untyped = metadata::decode::<AdmissionRecord>(&bytes);
    assert!(untyped.is_err());
    assert!(untyped.unwrap_err().to_string().contains("max_sessions"));
}

// ===========================================================================
// P1–P3 — what decode must NOT refuse
// ===========================================================================

/// **P1** — occupancy above the cap is legitimate live state, not a torn identity: a profile
/// lowered while sessions are live leaves the ledger over its new cap until they drain, and
/// the ledger is the record every teardown path must read to decrement `count`
/// (`0016:390-402`).
#[test]
fn p1_count_above_its_own_max_sessions_still_decodes() {
    let bytes = ledger(9_000, 4, 72_600, 165, 100, 10, 20_000);
    let record = decode_both(&bytes).expect("occupancy above the cap is not a decode error");
    assert_eq!(record.count(), 9_000);
    assert_eq!(record.max_sessions(), 4);
}

/// **P2** — `max_staged_chunks` far above any publishable ceiling still decodes. That ceiling
/// is `MAX_ROOT_SEGMENTS x MAX_SEG_CHUNKS` (`0016:1468`), a *deployment-capacity* product
/// enforced at part commit as a `400 EntityTooLarge`; `MAX_SEG_CHUNKS` has no definition on
/// this base and `MAX_ROOT_SEGMENTS` is explicitly not a decode constant
/// (`metadata.rs:302-322`).
///
/// **This leg is superseded by #508**: when `max_chunkref_bytes` and `MAX_SEG_CHUNKS` land,
/// the *format* maxima derived from them become decode bounds and this leg must be rewritten
/// by that slice.
#[test]
fn p2_staged_ceiling_above_any_publishable_product_still_decodes() {
    let bytes = ledger(0, 1, 330, 165, 1, 1, 10_000_000);
    let record = decode_both(&bytes).expect("a large staged ceiling is not a decode error");
    assert_eq!(record.profile().max_staged_chunks(), 10_000_000);
    assert_eq!(record.profile().u_ref(), 330);
}

/// **P3** — `max_part_chunks` outside the proposal's 165–381 window still decodes, below it
/// and far above it. That window is `max_chunkref_bytes`-derived (`0016:1466`) and #508's;
/// `0016` names `UploadPart` as its enforcement site, not decode.
///
/// **This leg is superseded by #508**, for the same reason as P2.
#[test]
fn p3_part_chunks_outside_the_proposal_window_still_decodes() {
    // Below: `U_ref = min(2, 3) = 2`, `max_sessions = min(2 / 2, 524_288) = 1`.
    let below = ledger(0, 1, 2, 1, 1, 1, 1);
    let record = decode_both(&below).expect("a small part-chunk cap is not a decode error");
    assert_eq!(record.profile().max_part_chunks(), 1);

    // Far above: `U_ref = min(2x500_000, 500_000 + 2x500_000) = 1_000_000`, and G5 holds
    // because `1 x 500_000 <= 524_288`.
    let above = ledger(0, 1, 1_000_000, 500_000, 1, 1, 500_000);
    let record = decode_both(&above).expect("a large part-chunk cap is not a decode error");
    assert_eq!(record.profile().max_part_chunks(), 500_000);
    assert_eq!(record.profile().u_ref(), 1_000_000);
}

// ===========================================================================
// P-arith — decode's verdict equals exact integer arithmetic's, at the field maxima
// ===========================================================================

/// **P-arith-accept** — `max_staged_chunks` at its field maximum with everything else
/// minimal. In exact arithmetic `U_ref = min(2, u32::MAX + 2) = 2`, so every rule holds and
/// the record MUST decode: the term that leaves the field's width is the one the `min`
/// discards. A same-width `max_staged_chunks + 2 x mip x mpc` panics here in debug and wraps
/// in release — which is why the record is authored at the maximum rather than near it.
#[test]
fn p_arith_accept_maximal_staged_chunks_decodes() {
    let bytes = ledger(0, 1, 2, 1, 1, 1, u32::MAX);
    let record = decode_both(&bytes).expect("the min discards the overflowing term");
    assert_eq!(record.profile().max_staged_chunks(), u32::MAX);
    assert_eq!(record.profile().u_ref(), 2);
    assert_eq!(record.profile().max_sessions(), 1);
}

/// **P-arith-reject** — every numeric field at the maximum its wire type admits, except the
/// two the key space bounds (`max_parts_per_session` and `max_inflight_parts` at
/// `MAX_PART_NUMBER`, so G3 and G4 hold) — G1, G2, G3, G4 and G6 all hold, and the overflow
/// lands squarely on the arithmetic rules.
///
/// In exact arithmetic `max_inflight_parts x max_part_chunks = 999_999 x 4_294_967_295 =
/// 4_294_963_000_032_705`, astronomically past `SCAN_CAP/2 = 524_288`, so **G5 is genuinely
/// violated** and decode must say so — by name, not by panic, and not by an `Ok` reached
/// through a wrapped product. (A same-width `u32` product panics here in debug.)
#[test]
fn p_arith_reject_maximal_record_names_the_guard_exact_arithmetic_violates() {
    let bytes = ledger(
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u32::MAX,
        999_999,
        999_999,
        u32::MAX,
    );
    assert_eq!(
        decode_both(&bytes),
        Err(RecordError::OwnedStagingRangeUnscannable {
            max_inflight_parts: 999_999,
            max_part_chunks: u64::from(u32::MAX),
        })
    );
}

// ===========================================================================
// The wire shape is closed, and unreadable bytes are their own fault
// ===========================================================================

/// An unknown field is a loud typed error, at the record and inside the profile alike:
/// `mpuctl` is CAS'd whole, so a decoder that dropped one would either conflict forever
/// (a re-encoded precondition) or silently rewrite the record without it (a raw-bytes
/// precondition) — both durable, neither observable (`0016:348`).
#[test]
fn an_unknown_field_is_refused_rather_than_dropped() {
    let record_level =
        br#"{"count":0,"max_sessions":1,"epoch":7,"profile":{"w_ref":2,"max_part_chunks":1,"max_parts_per_session":1,"max_inflight_parts":1,"max_staged_chunks":1}}"#;
    assert!(matches!(
        decode_both(record_level),
        Err(RecordError::MalformedRecordValue { .. })
    ));

    let profile_level =
        br#"{"count":0,"max_sessions":1,"profile":{"w_ref":2,"max_part_chunks":1,"max_parts_per_session":1,"max_inflight_parts":1,"max_staged_chunks":1,"b_ops":9}}"#;
    assert!(matches!(
        decode_both(profile_level),
        Err(RecordError::MalformedRecordValue { .. })
    ));
}

/// Bytes that are not a ledger at all come back as this record class's own fault, so a
/// consumer can tell "this record is torn" from "the store is failing".
#[test]
fn unreadable_bytes_are_attributed_to_the_record() {
    for bytes in [
        &b"not json"[..],
        &b"{}"[..],
        // A field outside its wire type's range is a wire fault, not a rule violation.
        br#"{"count":0,"max_sessions":1,"profile":{"w_ref":2,"max_part_chunks":4294967296,"max_parts_per_session":1,"max_inflight_parts":1,"max_staged_chunks":1}}"#,
    ] {
        assert!(matches!(
            decode_both(bytes),
            Err(RecordError::MalformedRecordValue {
                namespace: "mpuctl",
                ..
            })
        ));
    }
}
