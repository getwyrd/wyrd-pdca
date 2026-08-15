//! Issue #715 — slice 2/3 of #654's own re-split (itself slice 1/7 of #636, proposal 0016).
//! The multipart **admission ledger**: `wyrd_core::multipart::{Budget, AdmissionRecord}`, the
//! shared `encode_record`/`decode_record` envelope, and the format maxima the profile's
//! settled ranges (`0016:1463-1480`) are checked against. **Pure**: no store, no async, no
//! fixture beyond literals and the production codec.
//!
//! The legs, in the order the issue states them:
//!
//! * **leg 1** — round-trip: `encode_record`/`decode_record` are inverses, byte-for-byte, and
//!   no second spelling of one record decodes;
//! * **leg 1a** — a stored `max_sessions` disagreeing with what its own `profile` derives is an
//!   error;
//! * **leg 1f** — `Budget::new` enforces both ends of every settled range a FORMAT constant can
//!   decide, each bound falsified by a torn value that violates **only** it: (i) the
//!   value-ceiling rule on `max_part_chunks`, (ii) `max_staged_chunks ≥ max_part_chunks`,
//!   (iii) `max_staged_chunks ≤` the publishable ceiling, (iv) `max_inflight_parts ≤
//!   max_parts_per_session`;
//! * **leg 1g** — the per-session staged-reference set — committed chunks **and** in-flight
//!   ones — fits `SCAN_CAP/2`;
//! * **leg 3** — occupancy above a lowered cap still DECODES (`count > max_sessions` is live
//!   state, not a torn identity).
//!
//! Two things this file deliberately does **not** do. It does not recompute a bound from the
//! production formula and compare the two — every arithmetic expectation below is a
//! hand-computed literal with its own derivation in a comment, so a wrong operator in the
//! module cannot agree with a wrong operator here. And it does not take anyone's word for the
//! worst-case encoded width of a chunk-ref: `format_maxima_bound_the_widest_encoded_chunk_list`
//! builds the widest chunk list the format max admits and **measures it through the production
//! codec** (`metadata::encode`), the method `crates/core/tests/segmented_map_record.rs:506-527`
//! applies to the segment table.
//!
//! No `#![cfg(...)]` here (the gate's vacuous-green hazard) — this file always compiles and
//! always runs.

#![forbid(unsafe_code)]

use wyrd_core::metadata::{self, ChunkRef, EcScheme};
use wyrd_core::multipart::{
    chunkref_bytes, decode_admission_record, encode_record, max_chunks_per_value, AdmissionRecord,
    Budget, RecordError, CHUNKREF_FRAGMENTS_REFERENCE, MAX_CHUNKREF_BYTES,
    MAX_PART_CHUNKS_FORMAT_MAX, MAX_PART_NUMBER, MAX_PUBLISHABLE_CHUNKS, MAX_SEG_CHUNKS_FORMAT_MAX,
    SCAN_HALF, VALUE_CEILING_HALF,
};

// ===========================================================================
// The reference profile every leg perturbs one field of
// ===========================================================================

/// `W_ref`: chunk-refs the reconcile pass may hold. Large enough that the `W_ref ≥ U_ref`
/// range end never fires for the perturbations below, so each leg falsifies its own bound.
const W_REF: u64 = 4_000_000;
/// `MAX_PART_CHUNKS` at exactly the value-ceiling format maximum — the upper end of leg 1f-i.
const PART_CHUNKS: u32 = 156;
/// `MAX_PARTS_PER_SESSION` — S3's 10,000-part protocol cap, inside the key space's 999,999.
const PARTS: u32 = 10_000;
/// `MAX_INFLIGHT_PARTS`.
const INFLIGHT: u32 = 32;
/// `MAX_STAGED_CHUNKS` at exactly the publishable ceiling — the upper end of leg 1f-iii.
const STAGED: u32 = 79_872;

/// `U_ref` of the reference profile, by hand (`0016:1469`):
/// `min((10_000 + 32) × 156, 79_872 + 2 × 32 × 156) = min(1_564_992, 89_856)`. The ceiling
/// branch binds, which is the whole point of the 2026-07-24 tightening.
const REFERENCE_U_REF: u64 = 89_856;
/// `MAX_SESSIONS` of the reference profile, by hand (`0016:1470`):
/// `min(⌊4_000_000 / 89_856⌋, 524_288) = min(44, 524_288)`.
const REFERENCE_MAX_SESSIONS: u64 = 44;
/// The reference profile's staged-reference set, by hand: `79_872 + 32 × 156`.
const REFERENCE_STAGED_SCAN: u64 = 84_864;

/// The reference profile. Every leg below calls [`profile`] with one field perturbed.
fn reference() -> Budget {
    profile(W_REF, PART_CHUNKS, PARTS, INFLIGHT, STAGED).expect("the reference profile is legal")
}

fn profile(
    w_ref: u64,
    part_chunks: u32,
    parts: u32,
    inflight: u32,
    staged: u32,
) -> Result<Budget, RecordError> {
    Budget::new(w_ref, part_chunks, parts, inflight, staged)
}

/// The `reason` of a [`RecordError::Structural`], for the one-bound-per-negation assertions.
/// Any other variant is a different fault than the leg claims to provoke, so it fails here.
fn structural_reason(error: RecordError) -> String {
    match error {
        RecordError::Structural { reason, .. } => reason,
        other => panic!("expected a structural record error, got {other}"),
    }
}

/// Assert `result` is the rejection this leg's bound produces — and **only** it: `needle` is a
/// phrase unique to that one guard's message, so a torn value that tripped a *different* bound
/// fails this assertion instead of passing on a surviving guard.
fn rejected_for(result: Result<Budget, RecordError>, needle: &str) {
    let reason = structural_reason(result.expect_err("this profile must be refused"));
    assert!(
        reason.contains(needle),
        "the rejection must name the bound this leg falsifies ({needle:?}); got {reason:?}"
    );
}

// ===========================================================================
// Leg 1 — the round trip, and the one canonical spelling of a record
// ===========================================================================

#[test]
fn budget_and_admission_record_round_trip() {
    let record = AdmissionRecord::new(7, reference());
    let bytes = encode_record(&record);

    let decoded = decode_admission_record(&bytes).expect("a record this codec wrote must decode");
    assert_eq!(decoded, record, "decode(encode(record)) is the record");
    assert_eq!(
        encode_record(&decoded),
        bytes,
        "encode(decode(bytes)) is the bytes: every mutation of `mpuctl` is a whole-record CAS \
         whose precondition is exactly these bytes (`0016:348`)"
    );

    // The three fields the schema pins (`0016:348`), and the profile tuple behind the third.
    assert_eq!(decoded.count(), 7);
    assert_eq!(decoded.max_sessions(), REFERENCE_MAX_SESSIONS);
    let stored = decoded.profile();
    assert_eq!(stored.w_ref(), W_REF);
    assert_eq!(stored.max_part_chunks(), PART_CHUNKS);
    assert_eq!(stored.max_parts_per_session(), PARTS);
    assert_eq!(stored.max_inflight_parts(), INFLIGHT);
    assert_eq!(stored.max_staged_chunks(), STAGED);
    // The stored spelling is the one 0016 states, field for field.
    let expected = format!(
        r#"{{"count":7,"max_sessions":{REFERENCE_MAX_SESSIONS},"profile":{{"w_ref":{W_REF},"max_part_chunks":{PART_CHUNKS},"max_parts_per_session":{PARTS},"max_inflight_parts":{INFLIGHT},"max_staged_chunks":{STAGED}}}}}"#
    );
    assert_eq!(String::from_utf8(bytes).expect("JSON is UTF-8"), expected);
}

#[test]
fn a_second_spelling_of_one_record_decodes_as_no_record() {
    let canonical = encode_record(&AdmissionRecord::new(7, reference()));
    let text = String::from_utf8(canonical.clone()).expect("JSON is UTF-8");

    // Reordered fields, inserted whitespace and a dropped-on-decode extra field each decode
    // to a value whose re-encoding is DIFFERENT bytes — so every `require(mpuctl == prior)`
    // against them would fail forever. One typed error at decode, not a wedged ledger.
    let reordered = text.replace(
        r#"{"count":7,"max_sessions""#,
        r#"{"max_sessions":44,"count":7,"unused""#,
    );
    for torn in [
        text.replace(r#"{"count":7"#, r#"{ "count": 7"#),
        text.replace(r#""count":7"#, r#""count":7,"generation":1"#),
        reordered,
    ] {
        assert_ne!(torn.as_bytes(), canonical.as_slice(), "a second spelling");
        structural_reason(
            decode_admission_record(torn.as_bytes())
                .expect_err("a non-canonical spelling of one record is no record"),
        );
    }
    decode_admission_record(&canonical).expect("the canonical spelling still decodes");
}

// ===========================================================================
// Leg 1a — `max_sessions` may not disagree with the profile that derives it
// ===========================================================================

/// A `mpuctl` value spelling `max_sessions` by hand, with the reference profile.
fn ledger_with_max_sessions(max_sessions: u64) -> String {
    format!(
        r#"{{"count":1,"max_sessions":{max_sessions},"profile":{{"w_ref":{W_REF},"max_part_chunks":{PART_CHUNKS},"max_parts_per_session":{PARTS},"max_inflight_parts":{INFLIGHT},"max_staged_chunks":{STAGED}}}}}"#
    )
}

#[test]
fn stored_max_sessions_disagreeing_with_its_profile_is_a_decode_error() {
    // The derived value decodes; one above and one below do not. Both directions matter: a
    // LARGER stored limit admits past the memory bound the reconcile pass is sized for, a
    // smaller one is still two spellings of one derived quantity (ADR-0045).
    let record =
        decode_admission_record(ledger_with_max_sessions(REFERENCE_MAX_SESSIONS).as_bytes())
            .expect("the derived limit is the only one this record may carry");
    assert_eq!(record.max_sessions(), REFERENCE_MAX_SESSIONS);

    for torn in [REFERENCE_MAX_SESSIONS + 1, REFERENCE_MAX_SESSIONS - 1] {
        let reason = structural_reason(
            decode_admission_record(ledger_with_max_sessions(torn).as_bytes())
                .expect_err("max_sessions is derived, never chosen"),
        );
        assert!(
            reason.contains(&format!("max_sessions={torn}"))
                && reason.contains(&format!("derives {REFERENCE_MAX_SESSIONS}")),
            "the rejection must name both spellings; got {reason:?}"
        );
    }
}

#[test]
fn the_derivations_match_hand_computed_oracles() {
    // Every expectation here is computed in this file's comments from 0016's formulae, never
    // from the module's own arithmetic — a wrong operator in one cannot agree with the other.
    let reference = reference();
    assert_eq!(reference.u_ref(), REFERENCE_U_REF);
    assert_eq!(reference.max_sessions(), REFERENCE_MAX_SESSIONS);
    assert_eq!(reference.staged_reference_scan(), REFERENCE_STAGED_SCAN);

    // The OTHER branch of `U_ref`: the raw part-number space binds when the staged ceiling is
    // far away. `min((5 + 2) × 2, 79_872 + 2 × 2 × 2) = min(14, 79_880)`, and
    // `MAX_SESSIONS = min(⌊1_000 / 14⌋, 524_288) = 71`.
    let raw_bound = profile(1_000, 2, 5, 2, STAGED).expect("a legal small-part profile");
    assert_eq!(raw_bound.u_ref(), 14);
    assert_eq!(raw_bound.max_sessions(), 71);

    // The `SCAN_CAP/2` term of `MAX_SESSIONS` is a CLAMP, not a range check (`0016:1470`): at
    // `U_ref = min((1 + 1) × 1, 1 + 2) = 2` the quotient is ⌊u64::MAX / 2⌋ ≈ 9.2e18, which
    // would break the reaper's one `scan("mpu:")` if it were the answer.
    let clamped = profile(u64::MAX, 1, 1, 1, 1).expect("a legal one-chunk-part profile");
    assert_eq!(clamped.u_ref(), 2);
    assert_eq!(clamped.max_sessions(), SCAN_HALF);
    assert_eq!(SCAN_HALF, 524_288, "half of the 2^20 SCAN_CAP");
}

// ===========================================================================
// Leg 1f — both ends of every settled range a FORMAT constant can decide.
// One negation per bound: each torn value violates ONLY the bound it names, so
// the red it produces proves THAT guard is load-bearing.
// ===========================================================================

#[test]
fn leg_1f_i_max_part_chunks_obeys_the_value_ceiling_rule() {
    // At the bound: accepted. `max_staged_chunks` (79_872) still clears both its own ends,
    // `max_inflight_parts` still clears the part space, and the staged-reference set is
    // 79_872 + 32 × 156 = 84_864, far inside 524_288 — so 157 trips this bound and no other.
    assert_eq!(reference().max_part_chunks(), MAX_PART_CHUNKS_FORMAT_MAX);
    rejected_for(
        profile(W_REF, PART_CHUNKS + 1, PARTS, INFLIGHT, STAGED),
        "exceeds the value-ceiling format maximum",
    );
}

#[test]
fn leg_1f_ii_max_staged_chunks_keeps_one_maximal_part_stageable() {
    // At the lower end: exactly one maximal part's worth of staged chunks (`0016:1468`).
    let at_the_end = profile(W_REF, PART_CHUNKS, PARTS, INFLIGHT, PART_CHUNKS)
        .expect("one maximal part must remain stageable");
    assert_eq!(at_the_end.max_staged_chunks(), PART_CHUNKS);
    // One below: 155 < 156 trips only this bound — 155 is still inside the publishable
    // ceiling, and the staged-reference set (155 + 4_992) is still far inside 524_288.
    rejected_for(
        profile(W_REF, PART_CHUNKS, PARTS, INFLIGHT, PART_CHUNKS - 1),
        "at least one maximal part must remain stageable",
    );
}

#[test]
fn leg_1f_iii_max_staged_chunks_stays_inside_the_publishable_ceiling() {
    // At the upper end: `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS_FORMAT_MAX` = 512 × 156 = 79_872,
    // the settled value of `MAX_STAGED_CHUNKS` (`0016:1468`).
    assert_eq!(
        u64::from(reference().max_staged_chunks()),
        MAX_PUBLISHABLE_CHUNKS
    );
    // One above: 79_873 still clears the lower end, and 79_873 + 4_992 is still inside
    // 524_288 — so it trips only the publishable ceiling.
    rejected_for(
        profile(W_REF, PART_CHUNKS, PARTS, INFLIGHT, STAGED + 1),
        "exceeds the publishable format ceiling",
    );
}

#[test]
fn leg_1f_iv_max_inflight_parts_fits_the_part_space_it_draws_from() {
    // At the bound: every part a session may hold could be in flight at once (`0016:1471`).
    let at_the_end = profile(W_REF, PART_CHUNKS, INFLIGHT, INFLIGHT, STAGED)
        .expect("in-flight may equal the part space");
    assert_eq!(
        at_the_end.max_inflight_parts(),
        at_the_end.max_parts_per_session()
    );
    // One past: 32 in flight against a 31-part space. Shrinking the part space only shrinks
    // `U_ref` (to 9_828) and leaves every other bound satisfied, so this trips only 1f-iv.
    rejected_for(
        profile(W_REF, PART_CHUNKS, INFLIGHT - 1, INFLIGHT, STAGED),
        "a session cannot have more parts in flight than it may ever hold",
    );
}

#[test]
fn leg_1g_the_staged_reference_set_counts_committed_chunks_too() {
    // The bound counts BOTH terms `0016:1447` charges: the committed chunks a session may
    // hold (≤ 79_872) plus its in-flight ones (`max_inflight_parts × max_part_chunks`). At
    // 2_848 in flight the set is 79_872 + 444_288 = 524_160 — just inside 524_288.
    let at_the_end = profile(W_REF, PART_CHUNKS, PARTS, 2_848, STAGED)
        .expect("a profile whose staged-reference set fits the scan bound");
    assert_eq!(at_the_end.staged_reference_scan(), 524_160);
    assert!(at_the_end.staged_reference_scan() <= SCAN_HALF);

    // One part more: 79_872 + 2_849 × 156 = 524_316, over the bound. Note the IN-FLIGHT term
    // alone (444_444) is still well inside it — that is exactly the profile an in-flight-only
    // bound admits and this one refuses (the #692 batch-review finding on the archived v2
    // `Budget::new`, whose scan bound counted only the in-flight term).
    assert!(
        2_849 * u64::from(PART_CHUNKS) < SCAN_HALF,
        "the in-flight term alone fits"
    );
    rejected_for(
        profile(W_REF, PART_CHUNKS, PARTS, 2_849, STAGED),
        "over the 524288-entry scan bound",
    );
}

#[test]
fn degenerate_profiles_are_refused_at_both_ends() {
    // The lower end of every range: a zero component derives `U_ref = 0`, and `MAX_SESSIONS`
    // would be a division by zero rather than a limit.
    for (field, (w, pc, p, i, s)) in [
        ("w_ref", (0, PART_CHUNKS, PARTS, INFLIGHT, STAGED)),
        ("max_part_chunks", (W_REF, 0, PARTS, INFLIGHT, STAGED)),
        ("max_parts_per_session", (W_REF, PART_CHUNKS, 0, 0, STAGED)),
        ("max_inflight_parts", (W_REF, PART_CHUNKS, PARTS, 0, STAGED)),
        (
            "max_staged_chunks",
            (W_REF, PART_CHUNKS, PARTS, INFLIGHT, 0),
        ),
    ] {
        rejected_for(profile(w, pc, p, i, s), &format!("`{field}` is zero"));
    }
    // `W_ref` below one session's own worst-case footprint admits nothing (`0016:1473`), and
    // at the extreme is the tuple whose true `U_ref` overflows `u64`.
    rejected_for(
        profile(REFERENCE_U_REF - 1, PART_CHUNKS, PARTS, INFLIGHT, STAGED),
        "is below the U_ref",
    );
    // The all-maxima tuple trips several bounds at once, so it is asserted only to be
    // refused — the one-bound-per-negation legs above are what prove each guard load-bearing.
    // Its point is that the `U_ref` arithmetic never *saturates* into an affordable-looking
    // value: `(u32::MAX + u32::MAX) × u32::MAX` overflows `u64` by 2^65.
    structural_reason(
        profile(u64::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX)
            .expect_err("the overflowing tuple is refused, never saturated into a limit"),
    );
    // A part space the `part:` key grammar cannot address (`multipart.rs:281-289`).
    rejected_for(
        profile(W_REF, PART_CHUNKS, MAX_PART_NUMBER + 1, INFLIGHT, STAGED),
        "exceeds the part-number key space",
    );
}

// ===========================================================================
// Leg 3, binding the other way — occupancy is not identity
// ===========================================================================

#[test]
fn occupancy_above_the_stored_cap_still_decodes() {
    // A profile lowered while sessions are live leaves the ledger over its new cap until the
    // population drains (`0016:390-402`). Rejecting that at decode would make the ledger
    // unreadable exactly when the teardown path must read it — the same liberal-on-read
    // boundary `metadata.rs:312-321` draws. So this DECODES, and carries the occupancy.
    let over_cap = REFERENCE_MAX_SESSIONS * 1_000;
    let bytes = format!(
        r#"{{"count":{over_cap},"max_sessions":{REFERENCE_MAX_SESSIONS},"profile":{{"w_ref":{W_REF},"max_part_chunks":{PART_CHUNKS},"max_parts_per_session":{PARTS},"max_inflight_parts":{INFLIGHT},"max_staged_chunks":{STAGED}}}}}"#
    );
    let record = decode_admission_record(bytes.as_bytes())
        .expect("occupancy above a lowered cap is live state, not a torn record");
    assert!(record.count() > record.max_sessions());
    assert_eq!(record.count(), over_cap);
}

// ===========================================================================
// The format maxima, measured — not asserted in prose
// ===========================================================================

/// The widest chunk-ref a **writer** can commit under `RS(k, m)`: every number at its widest
/// decimal rendering (`id` a `u128`, `len` a `u64`, one `u64::MAX` D-server id per fragment) and
/// a placement of exactly `k + m` entries, which is what the write path always commits
/// (`metadata.rs:196-197`).
fn widest_chunk_ref(k: u8, m: u8) -> ChunkRef {
    ChunkRef {
        id: u128::MAX,
        scheme: EcScheme::ReedSolomon { k, m },
        len: u64::MAX,
        placement: vec![u64::MAX; usize::from(k) + usize::from(m)],
    }
}

#[test]
fn format_maxima_bound_the_widest_encoded_chunk_list() {
    // The pinned values, hand-computed from the encoding (`0016:1466`):
    //   one chunk-ref  = `{"id":` 6 + 39 + `,"scheme":` 10 + 33 + `,"len":` 7 + 20
    //                    + `,"placement":` 13 + (`[` 1 + 9 × 21) + `}` 1        = 319 B
    //   chunks / value = ⌊50_000 / (319 + 1)⌋, the array separator charged        = 156
    //   publishable    = MAX_ROOT_SEGMENTS 512 × 156                          = 79_872
    assert_eq!(
        VALUE_CEILING_HALF, 50_000,
        "half the 100_000-byte value ceiling"
    );
    assert_eq!(MAX_CHUNKREF_BYTES, 319);
    assert_eq!(MAX_SEG_CHUNKS_FORMAT_MAX, 156);
    assert_eq!(MAX_PART_CHUNKS_FORMAT_MAX, MAX_SEG_CHUNKS_FORMAT_MAX);
    assert_eq!(MAX_PUBLISHABLE_CHUNKS, 79_872);
    assert_eq!(
        MAX_PUBLISHABLE_CHUNKS,
        metadata::MAX_ROOT_SEGMENTS as u64 * u64::from(MAX_SEG_CHUNKS_FORMAT_MAX)
    );

    // ---- The measurement, through the production codec. ------------------------------
    // The width arithmetic is EXACT, not a guess with slack: at `RS(255, 255)` — where every
    // number in the skeleton is spent at its full decimal width, `k` and `m` included — the
    // production codec emits exactly what `chunkref_bytes` predicts. A wrong field-name or
    // punctuation count would show up here as a mismatch of that many bytes.
    assert_eq!(
        metadata::encode(&widest_chunk_ref(255, 255)).len(),
        chunkref_bytes(510)
    );
    // At the reference width, `MAX_CHUNKREF_BYTES` must be an UPPER bound on what `encode`
    // emits — the direction that matters, since a bound below the true width would admit a
    // part value over the ceiling. (The slack is exactly the four digits `k` and `m` do not
    // spend at 6 + 3 fragments.)
    let widest = widest_chunk_ref(6, 3);
    assert_eq!(
        widest.placement.len(),
        CHUNKREF_FRAGMENTS_REFERENCE,
        "the reference scheme is the one the format's width is taken at"
    );
    let measured = metadata::encode(&widest).len();
    assert!(
        measured <= MAX_CHUNKREF_BYTES,
        "the widest chunk-ref a writer commits at {CHUNKREF_FRAGMENTS_REFERENCE} fragments \
         encodes to {measured} B, which MAX_CHUNKREF_BYTES ({MAX_CHUNKREF_BYTES}) must bound"
    );

    // The binding property: a chunk list at the format maximum, every element the widest
    // legal one, fits the V/2 budget as one encoded value — array separators included. This
    // is what a minimum-width derivation fails: at `⌊50_000/47⌋ = 1_063` refs this list
    // encodes to ~336 KB.
    let at_the_max = vec![widest.clone(); MAX_PART_CHUNKS_FORMAT_MAX as usize];
    let encoded = metadata::encode(&at_the_max).len();
    assert!(
        encoded <= VALUE_CEILING_HALF,
        "a maximal chunk list ({MAX_PART_CHUNKS_FORMAT_MAX} widest refs) encodes to {encoded} B \
         and must fit the {VALUE_CEILING_HALF}-byte budget of `0016:1466`"
    );
    // And the ceiling is not loose the other way: twice the format maximum does NOT fit, so a
    // derivation that shrank it to a handful of chunks fails here too.
    let doubled = vec![widest; 2 * MAX_PART_CHUNKS_FORMAT_MAX as usize];
    assert!(
        metadata::encode(&doubled).len() > VALUE_CEILING_HALF,
        "twice the format maximum must NOT fit one value, or the maximum is far below the \
         count the budget actually affords"
    );

    // The separator is charged PER ELEMENT, which the reference width alone cannot show
    // (⌊50_000/319⌋ and ⌊50_000/320⌋ are both 156): at a 99-byte ref the budget affords 500
    // refs, not the 505 an uncharged rule admits — and those 5 are how 165 × 303 B became
    // 50 161 stored bytes (issue #715 review, 2026-08-09).
    assert_eq!(max_chunks_per_value(99), 500);

    // A wider EC scheme charges a wider chunk-ref and therefore a SMALLER ceiling — the same
    // rule, computed where the deployment's scheme is known (`UploadPart`'s `max_part_bytes`
    // refusal, `0016:1466`), which is why the format constant is the loosest of the family.
    let wider = chunkref_bytes(CHUNKREF_FRAGMENTS_REFERENCE + 5);
    assert!(
        wider > MAX_CHUNKREF_BYTES && max_chunks_per_value(wider) < MAX_PART_CHUNKS_FORMAT_MAX,
        "a wider scheme must clamp the chunk count further, not admit more"
    );
}
