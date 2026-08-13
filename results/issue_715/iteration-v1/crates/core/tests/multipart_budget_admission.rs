//! Issue #715 — slice 1/3 of #692's own re-split (proposal 0016, `Budget`/`AdmissionRecord`
//! record values for the `mpuctl` singleton). `wyrd_core::multipart::{Budget,
//! AdmissionRecord, encode_record, decode_admission_record}` round-trip, and every
//! relational invariant `0016:1463-1480` and ADR-0045 settle for them surfaces as a typed
//! [`RecordError`] at decode, never as an admitted value.
//!
//! Six legs, each isolating exactly one independently-enforced bound (a torn value that
//! tripped two guards at once would stay red on the surviving one and prove nothing):
//! 1. **1a** — a stored `AdmissionRecord.max_sessions` that disagrees with what its own
//!    `profile` derives;
//! 2. **1f-i** — `max_part_chunks` above the value-ceiling rule
//!    `MAX_CHUNKREF_BYTES x max_part_chunks <= MAX_VALUE_BYTES/2`;
//! 3. **1f-ii** — `max_staged_chunks` below `max_part_chunks` (the lower end);
//! 4. **1f-iii** — `max_staged_chunks` above `MAX_ROOT_SEGMENTS x MAX_SEG_CHUNKS_FORMAT_MAX`
//!    (the publishable ceiling);
//! 5. **1f-iv** — `max_inflight_parts` above `max_parts_per_session`;
//! 6. **1g** — the `sidx:` scan bound, `max_staged_chunks + max_inflight_parts x
//!    max_part_chunks <= SCAN_CAP/2`, counting committed staging entries as well as
//!    in-flight chunks (batch-review `multipart.rs:1074`).
//!
//! Plus round-trip legs and **leg 3** (occupancy is not identity): a decoded
//! `AdmissionRecord` whose `count` exceeds its `max_sessions` still decodes — that is a
//! live occupancy state under a lowered profile, never a decode error (`0016:390-402`).
//!
//! No `#![cfg(...)]` (the gate's vacuous-green hazard) — this file always compiles and
//! always runs.

#![forbid(unsafe_code)]

use wyrd_core::metadata;
use wyrd_core::multipart::{
    decode_admission_record, encode_record, AdmissionRecord, Budget, RecordError,
    MAX_CHUNKREF_BYTES, MAX_SEG_CHUNKS_FORMAT_MAX, VALUE_CEILING_HALF,
};

// ===========================================================================
// A baseline profile satisfying every bound, and one-field perturbations of it.
//
// Every helper below is checked (in `baseline_is_legal`) to actually construct — so a
// negation test that changes a field is provably isolating *only* that field's bound,
// never silently also tripping a second one because the baseline itself was already on
// some other edge.
// ===========================================================================

struct Profile {
    w_ref: u64,
    max_part_chunks: u32,
    max_parts_per_session: u32,
    max_inflight_parts: u32,
    max_staged_chunks: u32,
}

/// A tuple comfortably inside every one of `Budget::new`'s six checks, with plenty of
/// slack on each so a single-field perturbation below cannot accidentally graze a second
/// bound.
fn baseline() -> Profile {
    Profile {
        w_ref: 20_000,
        max_part_chunks: 100,
        max_parts_per_session: 100,
        max_inflight_parts: 50,
        max_staged_chunks: 100,
    }
}

fn build(p: &Profile) -> Result<Budget, RecordError> {
    Budget::new(
        p.w_ref,
        p.max_part_chunks,
        p.max_parts_per_session,
        p.max_inflight_parts,
        p.max_staged_chunks,
    )
}

#[test]
fn baseline_is_legal() {
    build(&baseline()).expect("the baseline tuple must satisfy every bound");
}

// ===========================================================================
// Round trips
// ===========================================================================

#[test]
fn budget_round_trips_through_encode_and_decode_record() {
    let budget = build(&baseline()).unwrap();
    let bytes = encode_record(&budget);
    let decoded: Budget = serde_json::from_slice(&bytes).expect("a legal Budget must decode back");
    assert_eq!(decoded, budget);
}

#[test]
fn admission_record_round_trips_through_encode_and_decode_record() {
    let profile = build(&baseline()).unwrap();
    let record = AdmissionRecord::new(3, profile);
    let bytes = encode_record(&record);
    let decoded = decode_admission_record(&bytes).expect("a legal AdmissionRecord must decode");
    assert_eq!(decoded, record);
}

// ===========================================================================
// Leg 1a — AdmissionRecord.max_sessions must equal what its own profile derives.
// ===========================================================================

#[test]
fn leg_1a_torn_max_sessions_is_rejected_at_decode() {
    let profile = build(&baseline()).unwrap();
    let derived = profile.max_sessions();
    // Hand-author the wire shape directly — the one way to produce the torn value
    // `AdmissionRecord::new` itself can never construct, exactly what a bit-flip or a
    // rolled-back partial write could still leave on disk.
    let torn = serde_json::json!({
        "count": 1,
        "max_sessions": derived + 1,
        "profile": profile,
    });
    let bytes = serde_json::to_vec(&torn).unwrap();
    let result = decode_admission_record(&bytes);
    assert!(
        result.is_err(),
        "max_sessions={} disagreeing with the profile's derived {derived} must be rejected, \
         got {result:?}",
        derived + 1
    );
}

// ===========================================================================
// Legs 1f-i .. 1f-iv and 1g — Budget::new's independent range checks.
// ===========================================================================

#[test]
fn leg_1f_i_max_part_chunks_above_the_value_ceiling_is_rejected() {
    // The largest legal max_part_chunks under MAX_CHUNKREF_BYTES x N <= VALUE_CEILING_HALF.
    let format_max = (VALUE_CEILING_HALF / MAX_CHUNKREF_BYTES) as u32;
    let mut p = baseline();
    p.max_part_chunks = format_max + 1;
    // Keep every other bound slack: staged >= part_chunks (1f-ii), staged still well
    // under the segment ceiling (1f-iii), the sidx: scan sum still well under SCAN_CAP/2
    // (1g), and W_ref recomputed generously above the resulting U_ref.
    p.max_staged_chunks = format_max + 1;
    p.max_parts_per_session = 200;
    p.max_inflight_parts = 50;
    p.w_ref = 10_000_000;
    let result = build(&p);
    assert!(
        result.is_err(),
        "max_part_chunks={} must exceed MAX_CHUNKREF_BYTES x N <= VALUE_CEILING_HALF \
         (format_max={format_max}), got {result:?}",
        p.max_part_chunks
    );
}

#[test]
fn leg_1f_ii_max_staged_chunks_below_max_part_chunks_is_rejected() {
    let mut p = baseline();
    p.max_part_chunks = 50;
    p.max_staged_chunks = 40; // below max_part_chunks — the lower-end violation
    p.max_parts_per_session = 50;
    p.max_inflight_parts = 10;
    p.w_ref = 5_000;
    let result = build(&p);
    assert!(
        result.is_err(),
        "max_staged_chunks=40 below max_part_chunks=50 must be rejected, got {result:?}"
    );
}

#[test]
fn leg_1f_iii_max_staged_chunks_above_the_publishable_ceiling_is_rejected() {
    let ceiling =
        u128::from(metadata::MAX_ROOT_SEGMENTS as u32) * u128::from(MAX_SEG_CHUNKS_FORMAT_MAX);
    let over = u32::try_from(ceiling + 1).expect("the ceiling fits u32 at these constants");
    let mut p = baseline();
    p.max_part_chunks = 1;
    p.max_staged_chunks = over;
    p.max_parts_per_session = 1;
    p.max_inflight_parts = 1;
    p.w_ref = u64::from(over) + 10;
    let result = build(&p);
    assert!(
        result.is_err(),
        "max_staged_chunks={over} above MAX_ROOT_SEGMENTS x MAX_SEG_CHUNKS_FORMAT_MAX \
         ({ceiling}) must be rejected, got {result:?}"
    );
}

#[test]
fn leg_1f_iv_max_inflight_parts_above_max_parts_per_session_is_rejected() {
    let mut p = baseline();
    p.max_parts_per_session = 10;
    p.max_inflight_parts = 20; // exceeds max_parts_per_session
    p.max_part_chunks = 10;
    p.max_staged_chunks = 10;
    p.w_ref = 1_000;
    let result = build(&p);
    assert!(
        result.is_err(),
        "max_inflight_parts=20 above max_parts_per_session=10 must be rejected, got {result:?}"
    );
}

#[test]
fn leg_1g_sidx_scan_bound_counts_committed_staging_plus_inflight_is_rejected() {
    // max_part_chunks pinned at the 1f-i format ceiling, so the product term is as large
    // as 1f-i alone allows, then max_inflight_parts is picked to push
    // `max_staged_chunks + max_inflight_parts * max_part_chunks` just over SCAN_CAP/2
    // while max_staged_chunks alone stays at (not above) the 1f-iii ceiling.
    let format_max_part_chunks = (VALUE_CEILING_HALF / MAX_CHUNKREF_BYTES) as u32;
    let staged_ceiling =
        u128::from(metadata::MAX_ROOT_SEGMENTS as u32) * u128::from(MAX_SEG_CHUNKS_FORMAT_MAX);
    let max_staged_chunks = u32::try_from(staged_ceiling).expect("fits u32");
    let scan_half = (wyrd_traits::SCAN_CAP / 2) as u128;
    // Solve for the smallest inflight count that pushes the sum over scan_half, given
    // the pinned max_part_chunks.
    let remaining = scan_half.saturating_sub(u128::from(max_staged_chunks));
    let max_inflight_parts =
        u32::try_from(remaining / u128::from(format_max_part_chunks) + 2).expect("fits u32");

    let mut p = baseline();
    p.max_part_chunks = format_max_part_chunks;
    p.max_staged_chunks = max_staged_chunks;
    p.max_inflight_parts = max_inflight_parts;
    p.max_parts_per_session = max_inflight_parts; // keep 1f-iv satisfied (equal is legal)
    p.w_ref = 10_000_000;

    let sum = u128::from(max_staged_chunks)
        + u128::from(max_inflight_parts) * u128::from(format_max_part_chunks);
    assert!(
        sum > scan_half,
        "test setup must actually exceed SCAN_CAP/2 ({scan_half}); got sum={sum}"
    );
    assert!(
        max_staged_chunks as u128 <= staged_ceiling,
        "test setup must not also trip 1f-iii"
    );

    let result = build(&p);
    assert!(
        result.is_err(),
        "max_staged_chunks + max_inflight_parts x max_part_chunks = {sum} exceeding \
         SCAN_CAP/2 = {scan_half} must be rejected, got {result:?}"
    );
}

// ===========================================================================
// Leg 3 — occupancy above a lowered cap is not a decode error (binding the other way).
// ===========================================================================

#[test]
fn leg_3_count_above_max_sessions_still_decodes() {
    let profile = build(&baseline()).unwrap();
    let max_sessions = profile.max_sessions();
    // A live occupancy state a lowered profile could leave behind — legitimate, not torn.
    let record = AdmissionRecord {
        count: max_sessions + 5,
        max_sessions,
        profile,
    };
    let bytes = encode_record(&record);
    let decoded = decode_admission_record(&bytes)
        .expect("count exceeding max_sessions is occupancy, not a decode error (`0016:390-402`)");
    assert_eq!(decoded.count, max_sessions + 5);
}
