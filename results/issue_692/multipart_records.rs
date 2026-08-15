//! Issue #692 — slice 2/3 of #654's own re-split (itself slice 1/7 of #636, proposal 0016).
//! The multipart commit protocol's **record family and its validating decoders**, built on
//! slice 1's key grammar (issue #691, `crates/core/tests/multipart_keys.rs`). **Pure**: no
//! store, no async runtime, no fixture beyond literals — every symbol under test is a pure
//! function or a `serde` codec.
//!
//! The success criterion's three legs, in order:
//!
//! 1. **every record decode is validating** — each record type round-trips
//!    `encode`/`decode`, and a structurally invalid value is a typed error at decode. Five
//!    relational identity checks are BINDING (carried-forward MUST-FIXes from the #654-v2
//!    sign-off and its batch review), each proven with a hand-authored torn value: (a)
//!    `AdmissionRecord.max_sessions` vs. its own `profile`'s derivation; (b)
//!    `decode_owned_entry` takes the `sidx:` key and checks `owner` against it; (c)
//!    `SessionRecord.publish_target.parent`/`name` vs. the session's own `parent`/`object`;
//!    (d) `decode_retire_obligation` takes the `retire:` key and checks the payload's
//!    session/generation identity against the token; (e) a `PendingEntry` with exactly one
//!    of `owner`/`staged` is torn, under both a `pending:` and a `sidx:` reading;
//! 2. **decode -> encode is the identity on a legacy `pending:` value** (no new fields);
//! 3. **the identity/occupancy boundary holds** — a decoded `AdmissionRecord` whose `count`
//!    exceeds its `max_sessions` still decodes (occupancy, not identity).

#![forbid(unsafe_code)]

use serde_json::json;

use wyrd_core::metadata::{self, ChunkRef, EcScheme, PendingEntry};
use wyrd_core::multipart::{
    decode_admission_record, decode_owned_entry, decode_part, decode_part_summary,
    decode_retire_obligation, decode_session, decode_slot, encode_record, retire_key, sidx_key,
    AdmissionRecord, AttemptId, Budget, Completion, Digest, OwnedEntry, PartNumber, PartNumberSet,
    PartRecord, PartSummary, PublishTarget, RecordError, RetireMode, RetirePayload, RetireToken,
    SessionRecord, SessionState, SlotRecord, StagedPlacement, UploadId, MAX_PART_NUMBER,
    TOKEN_HEX_LEN,
};

// ===========================================================================
// Literals and small helpers — mirrors `multipart_keys.rs`'s style.
// ===========================================================================

fn hex_token(pattern: &str) -> String {
    let mut s = String::new();
    while s.len() < TOKEN_HEX_LEN {
        s.push_str(pattern);
    }
    s.truncate(TOKEN_HEX_LEN);
    s
}

fn upload(pattern: &str) -> UploadId {
    UploadId::new(hex_token(pattern)).expect("a well-formed token is an upload id")
}

fn attempt(pattern: &str) -> AttemptId {
    AttemptId::new(hex_token(pattern)).expect("a well-formed token is an attempt id")
}

fn part(n: u32) -> PartNumber {
    PartNumber::new(n).expect("a part number inside the key space")
}

fn digest(byte: u8) -> Digest {
    Digest::from_bytes([byte; 32])
}

fn chunk_ref(id: u128, len: u64) -> ChunkRef {
    ChunkRef {
        id,
        scheme: EcScheme::None,
        len,
        placement: Vec::new(),
    }
}

/// A minimal, otherwise-valid `Open` session, so each test only spells the one field it
/// means to vary.
fn open_session() -> SessionRecord {
    SessionRecord {
        bucket: "b".to_string(),
        parent: 1,
        object: "o".to_string(),
        content_type: None,
        created_at_millis: 10,
        clock_source: "logical".to_string(),
        state: SessionState::Open,
        epoch: 0,
        attempts: 0,
        group_nonce: metadata::SegmentNonce::new(hex_token("ab"))
            .expect("a well-formed nonce token"),
        fenced_at_millis: None,
        segments_written: None,
        publish_target: None,
        completion: None,
    }
}

/// The reference profile: `U_ref` binds on the **raw** part-number term.
/// `U_ref = min( (400 + 32) * 8, 4000 + 2*32*8 ) = min(3456, 4512) = 3456`.
fn budget() -> Budget {
    Budget::new(1_000_000, 8, 400, 32, 4_000).expect("a valid profile")
}

/// The same shape with the **staged-ceiling** term binding instead:
/// `U_ref = min( (400 + 32) * 8, 1000 + 2*32*8 ) = min(3456, 1512) = 1512`.
/// Both branches of the `min` need a fixture, or half the derivation is unexercised.
fn ceiling_bound_budget() -> Budget {
    Budget::new(1_000_000, 8, 400, 32, 1_000).expect("a valid profile")
}

fn segment_group() -> metadata::SegmentGroup {
    metadata::SegmentGroup::new(hex_token("cd"), 1).expect("a well-formed segment group")
}

/// Every relational leg below asserts the same shape: a torn value is a **typed** structural
/// error naming its own record class — never a value, and never an untyped parse failure.
/// Returns the reason, so a leg can also pin *which* contradiction was caught.
#[track_caller]
fn structural<T: std::fmt::Debug>(result: Result<T, RecordError>, record: &str) -> String {
    match result {
        Err(RecordError::Structural {
            record: seen,
            reason,
        }) if seen == record => reason,
        other => panic!("expected a structural `{record}` rejection, got {other:?}"),
    }
}

fn owned_entry() -> OwnedEntry {
    OwnedEntry {
        owner: upload("a1"),
        lease_expiry_millis: 500,
        staged: StagedPlacement {
            scheme: EcScheme::None,
            placement: vec![7],
        },
    }
}

// ===========================================================================
// Leg 1 — every record decode is validating.
// ===========================================================================

#[test]
fn encode_record_and_decode_record_round_trip_every_class() {
    // `Budget`/`AdmissionRecord` — via `mpuctl`'s own decoder.
    let admission = AdmissionRecord::new(3, budget());
    let bytes = encode_record(&admission);
    assert_eq!(decode_admission_record(&bytes).unwrap(), admission);

    // `SessionRecord` — via `mpu:`'s own decoder, one per reachable state.
    let open = open_session();
    assert_eq!(decode_session(&encode_record(&open)).unwrap(), open);

    let completing = SessionRecord {
        state: SessionState::Completing,
        epoch: 1,
        fenced_at_millis: Some(20),
        segments_written: Some(0),
        publish_target: Some(PublishTarget {
            parent: 1,
            name: "o".to_string(),
            fence_epoch: 1,
        }),
        ..open_session()
    };
    assert_eq!(
        decode_session(&encode_record(&completing)).unwrap(),
        completing
    );

    let completed = SessionRecord {
        state: SessionState::Completed,
        completion: Some(Completion {
            inode: 1,
            version: 2,
            etag: "\"deadbeef\"".to_string(),
            completed_at_millis: 30,
            complete_fingerprint: digest(1),
        }),
        ..open_session()
    };
    assert_eq!(
        decode_session(&encode_record(&completed)).unwrap(),
        completed
    );

    // `SlotRecord` — via `slot:`'s own decoder.
    let slot = SlotRecord {
        part_number: part(1),
        attempt_id: attempt("b2"),
        reserved_at_millis: 10,
        lease_expiry_millis: 20,
    };
    assert_eq!(decode_slot(&encode_record(&slot)).unwrap(), slot);

    // `PartRecord`/`PartSummary` — via `part:`/`psum:`'s own decoders.
    let part_record = PartRecord {
        chunks: vec![chunk_ref(1, 100), chunk_ref(2, 50)],
        len: 150,
        digest: digest(2),
        committed_at_millis: 40,
        session_epoch: 0,
    };
    assert_eq!(
        decode_part(&encode_record(&part_record)).unwrap(),
        part_record
    );

    let summary = PartSummary {
        chunks: 2,
        len: 150,
        digest: digest(2),
        committed_at_millis: 40,
    };
    assert_eq!(
        decode_part_summary(&encode_record(&summary)).unwrap(),
        summary
    );

    // `OwnedEntry` — via `sidx:`'s own decoder, key-taking.
    let owned = owned_entry();
    let key = sidx_key(&owned.owner, part(1), 9);
    assert_eq!(
        decode_owned_entry(&key, &encode_record(&owned.to_pending())).unwrap(),
        owned
    );

    // `RetirePayload` — via `decode_retire_obligation`, key-taking, every payload shape.
    let session_token = RetireToken::Session {
        upload_id: upload("c3"),
        epoch: 5,
        part: None,
    };
    for payload in [
        RetirePayload::Session {},
        RetirePayload::Parts {
            parts: PartNumberSet::from_numbers([part(1), part(2)]),
        },
        RetirePayload::Chunks {
            chunks: vec![chunk_ref(1, 10)],
        },
    ] {
        let key = retire_key(RetireMode::Bytes, &session_token);
        assert_eq!(
            decode_retire_obligation(&key, &encode_record(&payload)).unwrap(),
            (session_token.clone(), payload)
        );
    }
    let records_payload = RetirePayload::Records {
        parts: Some(PartNumberSet::from_numbers([part(3)])),
        segments: None,
    };
    let records_key = retire_key(RetireMode::Records, &session_token);
    assert_eq!(
        decode_retire_obligation(&records_key, &encode_record(&records_payload)).unwrap(),
        (session_token.clone(), records_payload)
    );

    let generation_token = RetireToken::Generation {
        inode: 7,
        version: 3,
    };
    let generation_payload = RetirePayload::Generation {
        inode: 7,
        version: 3,
        chunks: vec![chunk_ref(1, 10)],
        segments: None,
    };
    let generation_key = retire_key(RetireMode::Bytes, &generation_token);
    assert_eq!(
        decode_retire_obligation(&generation_key, &encode_record(&generation_payload)).unwrap(),
        (generation_token, generation_payload)
    );
}

#[test]
fn budget_rejects_a_zero_component_and_derives_u_ref_and_max_sessions() {
    for zero in [
        Budget::new(0, 1, 1, 1, 1),
        Budget::new(1, 0, 1, 1, 1),
        Budget::new(1, 1, 0, 1, 1),
        Budget::new(1, 1, 1, 0, 1),
        Budget::new(1, 1, 1, 1, 0),
    ] {
        assert!(zero.is_err(), "a zero profile component must be refused");
    }
    // Both branches of the `min` need a fixture, or half the derivation goes unexercised:
    // the RAW part-number term, then the staged-CEILING term.
    assert_eq!(budget().u_ref(), 3_456);
    assert_eq!(budget().max_sessions(), 289); // floor(1_000_000 / 3456)
    assert_eq!(ceiling_bound_budget().u_ref(), 1_512);
    assert_eq!(ceiling_bound_budget().max_sessions(), 661); // floor(1_000_000 / 1512)
}

/// A profile is refused unless it satisfies the **valid ranges 0016 settles** for the tuple
/// (`0016:1463-1480`) — the knob *values* are #655's, but a knob a safety property depends on
/// has its range settled by the proposal, so an out-of-range profile is never a persisted
/// value (issue #692's review of the out-of-range profile). Every rejection is
/// paired, in the same order, with the **boundary** value that must still be accepted, so a
/// range check cannot silently become an off-by-one.
#[test]
fn budget_rejects_a_profile_outside_the_ranges_0016_settles() {
    let scan_half = u32::try_from(wyrd_traits::SCAN_CAP / 2).expect("SCAN_CAP/2 fits a u32");
    for (out_of_range, boundary, marker) in [
        // (i) MAX_INFLIGHT_PARTS <= MAX_PARTS_PER_SESSION (`0016:1476`): a session cannot
        // have more parts in flight than it may ever hold.
        (
            Budget::new(1_000_000, 8, 32, 33, 4_000),
            Budget::new(1_000_000, 8, 32, 32, 4_000),
            "max_inflight_parts",
        ),
        // (ii) MAX_STAGED_CHUNKS >= MAX_PART_CHUNKS (`0016:1468`): at least one maximal part
        // must remain stageable.
        (
            Budget::new(1_000_000, 8, 400, 32, 7),
            Budget::new(1_000_000, 8, 400, 32, 8),
            "max_staged_chunks",
        ),
        // (iii) MAX_PARTS_PER_SESSION <= MAX_PART_NUMBER: the knob may not promise a
        // part-number space the key grammar cannot address (`multipart.rs:293-299`).
        (
            Budget::new(u64::MAX, 1, MAX_PART_NUMBER + 1, 1, 1),
            Budget::new(u64::MAX, 1, MAX_PART_NUMBER, 1, 1),
            "key space",
        ),
        // (iv) MAX_INFLIGHT_PARTS * MAX_PART_CHUNKS <= SCAN_CAP/2 (`0016:1476`'s bounding
        // invariant): the per-session owned `sidx:` range is exactly this product, and the
        // teardown scan must be able to walk it empty. The boundary case is a maximal legal
        // profile — exactly SCAN_CAP/2 owned entries, the whole part-number key space.
        (
            Budget::new(u64::MAX, 1_024, 1_024, 1_024, 1_048_576),
            Budget::new(u64::MAX, scan_half, MAX_PART_NUMBER, 1, u32::MAX),
            "scan",
        ),
        // (v) W_ref >= U_ref (`0016:1479`): a budget below one session's worst-case footprint
        // could admit nobody. U_ref is 1512 for this tuple.
        (
            Budget::new(1_511, 8, 400, 32, 1_000),
            Budget::new(1_512, 8, 400, 32, 1_000),
            "w_ref",
        ),
    ] {
        let reason = structural(out_of_range, "mpuctl profile");
        assert!(reason.contains(marker), "{reason}");
        assert!(boundary.is_ok(), "the boundary value for {marker} is legal");
    }
    // ...and the W_ref == U_ref boundary admits exactly one session.
    let exactly_one = Budget::new(1_512, 8, 400, 32, 1_000).expect("W_ref == U_ref is legal");
    assert_eq!(exactly_one.max_sessions(), 1);
}

/// The overflow case issue #692's review named: with a *saturating*
/// `U_ref`, a profile whose true footprint is ~2^65 chunk-refs saturated to `u64::MAX` and
/// then derived `MAX_SESSIONS = 1` against `W_ref = u64::MAX` — admitting a session whose
/// worst case is larger than the entire budget. The arithmetic is exact now, and the tuple is
/// refused outright rather than approximated.
#[test]
fn budget_refuses_the_profile_whose_true_footprint_overflows_its_own_budget() {
    let overflowing = Budget::new(u64::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX);
    assert!(
        overflowing.is_err(),
        "a profile whose worst-case footprint exceeds u64 must never derive a limit"
    );
    // And no profile that IS constructible derives a limit its own budget cannot hold:
    // `W_ref >= U_ref` at construction means `max_sessions() >= 1` always charges a real one.
    for profile in [budget(), ceiling_bound_budget()] {
        let charged = u128::from(profile.max_sessions()) * u128::from(profile.u_ref());
        assert!(profile.max_sessions() >= 1);
        assert!(charged <= u128::from(profile.w_ref()), "{charged} > W_ref");
    }
}

/// `MAX_SESSIONS = min(floor(W_ref/U_ref), SCAN_CAP/2)` — the second term is a **clamp the
/// implementation applies**, not a range check left to the operator (`0016:1470`): a legal
/// pairing (a large `W_ref` with small parts) makes the quotient exceed `SCAN_CAP` and breaks
/// the reaper's one `scan("mpu:")`. Unclamped, this profile would admit 1,000,000 sessions
/// against a scan that caps at 1,048,576 records.
#[test]
fn max_sessions_is_clamped_to_half_the_scan_cap() {
    let scan_half = u64::try_from(wyrd_traits::SCAN_CAP / 2).expect("SCAN_CAP/2 fits a u64");
    // U_ref = min( (1+1)*1, 1 + 2*1*1 ) = 2, so the quotient is 1,000,000 — well past the
    // clamp, and still short of an unclamped `SCAN_CAP * 2`.
    let small_parts = Budget::new(2_000_000, 1, 1, 1, 1).expect("a valid profile");
    assert_eq!(small_parts.u_ref(), 2);
    assert!(small_parts.w_ref() / small_parts.u_ref() > scan_half);
    assert_eq!(small_parts.max_sessions(), scan_half);
}

/// Leg 1a (BINDING): a torn `mpuctl` value whose `max_sessions` disagrees with what its own
/// `profile` derives is rejected at decode (`0016:1469-1470`: `U_ref` and
/// `MAX_SESSIONS = floor(W_ref/U_ref)` are functions of the tuple). A torn or rolled-back
/// ledger naming a larger `max_sessions` than its own profile derives would admit sessions
/// past the memory bound the reconcile pass is sized for (C-1).
#[test]
fn a_torn_admission_record_disagreeing_with_its_own_profile_is_rejected() {
    let profile = budget();
    let derived = profile.max_sessions();
    assert_ne!(derived, 0);

    // The honest value decodes.
    let honest = json!({ "count": 1, "max_sessions": derived, "profile": {
        "w_ref": profile.w_ref(), "max_part_chunks": profile.max_part_chunks(),
        "max_parts_per_session": profile.max_parts_per_session(),
        "max_inflight_parts": profile.max_inflight_parts(),
        "max_staged_chunks": profile.max_staged_chunks(),
    }});
    assert!(decode_admission_record(honest.to_string().as_bytes()).is_ok());

    // The torn value — max_sessions inflated past what the SAME profile derives — is
    // refused: two stored spellings of one quantity (the limit) may never disagree.
    let mut torn = honest.clone();
    torn["max_sessions"] = json!(derived + 1);
    let reason = structural(
        decode_admission_record(torn.to_string().as_bytes()),
        "mpuctl",
    );
    assert!(reason.contains("derived, never chosen"), "{reason}");
}

/// Leg 1b (BINDING): `decode_owned_entry` takes the `sidx:` key and rejects a payload whose
/// `owner` differs from the key's upload id — the API shape itself makes the cross-check
/// possible; a value-only decode cannot express it at all.
#[test]
fn a_torn_owned_entry_disagreeing_with_its_own_key_is_rejected() {
    let key_owner = upload("a1");
    let payload_owner = upload("b2");
    assert_ne!(key_owner, payload_owner);

    let key = sidx_key(&key_owner, part(1), 9);
    let mut torn = owned_entry();
    torn.owner = payload_owner;
    let bytes = encode_record(&torn.to_pending());

    let reason = structural(decode_owned_entry(&key, &bytes), "sidx:");
    assert!(reason.contains("must agree with the key"), "{reason}");

    // The honest pairing — payload owner == key owner — decodes.
    let honest_key = sidx_key(&torn.owner, part(1), 9);
    assert!(decode_owned_entry(&honest_key, &bytes).is_ok());
}

/// Leg 1c (BINDING): a `SessionRecord` whose `publish_target.parent`/`name` disagrees with
/// the session's own `parent`/`object` is rejected — a malformed completing record could
/// otherwise resume publication against a different dirent than the one the session's own
/// fields name.
#[test]
fn a_torn_session_disagreeing_publish_target_with_its_own_object_is_rejected() {
    let honest = SessionRecord {
        state: SessionState::Completing,
        epoch: 1,
        fenced_at_millis: Some(5),
        segments_written: Some(0),
        publish_target: Some(PublishTarget {
            parent: 1,
            name: "o".to_string(),
            fence_epoch: 1,
        }),
        ..open_session()
    };
    assert!(decode_session(&encode_record(&honest)).is_ok());

    // Torn: publish_target names a DIFFERENT parent, or object, than the session's own.
    for (parent, name) in [(1, "not-o"), (2, "o")] {
        let torn = SessionRecord {
            publish_target: Some(PublishTarget {
                parent,
                name: name.to_string(),
                fence_epoch: 1,
            }),
            ..honest.clone()
        };
        let reason = structural(decode_session(&encode_record(&torn)), "mpu:");
        assert!(reason.contains("the same dirent"), "{reason}");
    }
}

/// The other half of a session's state contract (`0016:404-408`): a state-scoped field is
/// present **iff** the state defines it, in both directions. A `Completing`-only
/// `fenced_at_millis` on an `Open` session, or a `Completing` session missing the
/// `publish_target` it must resume from, is a record no writer in this protocol produces.
#[test]
fn a_session_carries_exactly_the_fields_its_state_defines() {
    let completing = SessionRecord {
        state: SessionState::Completing,
        epoch: 1,
        fenced_at_millis: Some(5),
        segments_written: Some(0),
        publish_target: Some(PublishTarget {
            parent: 1,
            name: "o".to_string(),
            fence_epoch: 1,
        }),
        ..open_session()
    };
    for torn in [
        // A `Completing`-only field on an `Open` session.
        SessionRecord {
            fenced_at_millis: Some(5),
            ..open_session()
        },
        // A `Completed`-only field on a `Completing` session.
        SessionRecord {
            completion: Some(Completion {
                inode: 1,
                version: 2,
                etag: "\"e\"".to_string(),
                completed_at_millis: 30,
                complete_fingerprint: digest(1),
            }),
            ..completing.clone()
        },
        // A `Completing` session missing the target it must resume publication against.
        SessionRecord {
            publish_target: None,
            ..completing.clone()
        },
    ] {
        let reason = structural(decode_session(&encode_record(&torn)), "mpu:");
        assert!(reason.contains("its state defines"), "{reason}");
    }
}

/// Leg 1d (BINDING): `decode_retire_obligation` takes the `retire:` key and rejects a
/// payload whose generation identity disagrees with the key's token — a generation-scoped
/// payload under a session token, and vice versa, are BOTH errors (the archived
/// `multipart_records.rs` v2 test affirmed the generation-under-session case as `Ok`; it
/// must now reject).
#[test]
fn a_retire_payload_disagreeing_with_its_own_token_identity_is_rejected() {
    let session_token = RetireToken::Session {
        upload_id: upload("c3"),
        epoch: 5,
        part: None,
    };
    let generation_token = RetireToken::Generation {
        inode: 7,
        version: 3,
    };
    let generation_payload = RetirePayload::Generation {
        inode: 7,
        version: 3,
        chunks: vec![chunk_ref(1, 10)],
        segments: None,
    };
    let session_payload = RetirePayload::Session {};

    // A generation payload naming a DIFFERENT (inode, version) than its own key's token.
    let mismatched_key = retire_key(
        RetireMode::Bytes,
        &RetireToken::Generation {
            inode: 7,
            version: 99,
        },
    );
    for (key, payload) in [
        // A GENERATION-scoped payload under a SESSION token — the case the archived v2 test
        // affirmed as `Ok` (see the module doc); it must now reject.
        (
            retire_key(RetireMode::Bytes, &session_token),
            &generation_payload,
        ),
        // A SESSION-scoped payload under a GENERATION token: also rejected.
        (
            retire_key(RetireMode::Bytes, &generation_token),
            &session_payload,
        ),
        // ...and the same generation payload under the wrong generation's token.
        (mismatched_key, &generation_payload),
    ] {
        structural(
            decode_retire_obligation(&key, &encode_record(payload)),
            "retire:",
        );
    }

    // The honest pairings decode (also exercised by the round-trip test above).
    assert!(decode_retire_obligation(
        &retire_key(RetireMode::Bytes, &generation_token),
        &encode_record(&generation_payload)
    )
    .is_ok());
}

/// Leg 1e (BINDING): a `PendingEntry` with exactly one of `owner`/`staged` is torn and
/// rejected at decode, under BOTH a `pending:` reading (`metadata::decode`, the ordinary
/// streaming-write path) and a `sidx:` reading (`decode_owned_entry`) — both routes decode
/// through the SAME type, so the rejection is a property of the type, not of either caller.
/// Both-absent (legacy) and both-present (owned) are the only valid shapes.
#[test]
fn a_pending_entry_with_exactly_one_of_owner_or_staged_is_torn_under_both_readings() {
    let owner_only = json!({ "lease_expiry_millis": 500, "owner": hex_token("a1") });
    let staged_only = json!({
        "lease_expiry_millis": 500,
        "staged": { "scheme": "None", "placement": [7] }
    });

    for torn in [&owner_only, &staged_only] {
        // The `pending:` reading: `metadata::decode::<PendingEntry>`.
        let result: Result<PendingEntry, _> = metadata::decode(torn.to_string().as_bytes());
        assert!(result.is_err(), "{torn} decoded as a PendingEntry");

        // The `sidx:` reading: `decode_owned_entry`.
        let key = sidx_key(&upload("a1"), part(1), 9);
        assert!(
            decode_owned_entry(&key, torn.to_string().as_bytes()).is_err(),
            "{torn} decoded as an OwnedEntry"
        );
    }

    // Both-absent (legacy) and both-present (owned) are the only valid shapes.
    let neither = json!({ "lease_expiry_millis": 500 });
    let both: PendingEntry =
        metadata::decode(encode_record(&owned_entry().to_pending()).as_slice()).unwrap();
    assert!(metadata::decode::<PendingEntry>(neither.to_string().as_bytes()).is_ok());
    assert_eq!(both.owner, Some(owned_entry().owner));

    // ...but a legacy value is not an OWNED entry, even read under a `sidx:` key: the two
    // valid shapes are valid for different records, and the owned view says which it is.
    let key = sidx_key(&upload("a1"), part(1), 9);
    let legacy = structural(
        decode_owned_entry(&key, neither.to_string().as_bytes()),
        "sidx:",
    );
    assert!(legacy.contains("not an owned staging entry"), "{legacy}");
}

#[test]
fn slot_part_and_summary_records_reject_their_own_structural_violations() {
    // `SlotRecord`: a lease that expired before the slot was even reserved.
    let slot = SlotRecord {
        part_number: part(1),
        attempt_id: attempt("b2"),
        reserved_at_millis: 100,
        lease_expiry_millis: 100,
    };
    assert!(decode_slot(&encode_record(&slot)).is_ok());
    let torn_slot = json!({
        "part_number": 1, "attempt_id": hex_token("b2"),
        "reserved_at_millis": 100, "lease_expiry_millis": 99
    });
    assert!(decode_slot(torn_slot.to_string().as_bytes()).is_err());

    // `PartRecord`: `len` disagreeing with the chunk list's own span — checked, so a
    // wrapped sum (`u64::MAX + 1 == 0`) cannot forge agreement.
    let part_of = |chunks: serde_json::Value, len: u64| {
        json!({ "chunks": chunks, "len": len, "digest": digest(1).to_hex(),
                "committed_at_millis": 1, "session_epoch": 0 })
    };
    let mismatched = part_of(json!([{ "id": 1, "scheme": "None", "len": 100 }]), 150);
    let error = decode_part(mismatched.to_string().as_bytes()).unwrap_err();
    assert!(format!("{error}").contains("150"), "{error}");
    let overflow = part_of(
        json!([
            { "id": 1, "scheme": "None", "len": u64::MAX },
            { "id": 2, "scheme": "None", "len": 1 }
        ]),
        0,
    );
    let error = decode_part(overflow.to_string().as_bytes()).unwrap_err();
    assert!(format!("{error}").contains("overflow"), "{error}");

    // `PartSummary`: `chunks`/`len` must agree on emptiness.
    let summary_of = |chunks: u32, len: u64| json!({ "chunks": chunks, "len": len, "digest": digest(1).to_hex(), "committed_at_millis": 1 });
    assert!(decode_part_summary(summary_of(0, 0).to_string().as_bytes()).is_ok());
    assert!(decode_part_summary(summary_of(0, 150).to_string().as_bytes()).is_err());
    assert!(decode_part_summary(summary_of(2, 0).to_string().as_bytes()).is_err());
}

#[test]
fn an_owned_entry_staged_placement_length_decodes_liberally_contextual_not_structural() {
    // Placement LENGTH is deliberately not a decode-time check (ADR-0045; `0016:416-432`):
    // a mismatch is quarantined downstream (GC's safety gate), not refused at decode.
    let short_placement = json!({
        "lease_expiry_millis": 500, "owner": hex_token("a1"),
        "staged": { "scheme": { "ReedSolomon": { "k": 4, "m": 2 } }, "placement": [1] }
    });
    let key = sidx_key(&upload("a1"), part(1), 9);
    assert!(decode_owned_entry(&key, short_placement.to_string().as_bytes()).is_ok());
}

#[test]
fn a_part_number_set_is_canonical_or_it_is_not_a_set() {
    let set = PartNumberSet::from_numbers([part(1), part(2), part(3), part(9), part(10)]);
    assert_eq!(set.len(), 5);
    assert_eq!(set.iter().collect::<Vec<_>>(), vec![1, 2, 3, 9, 10]);
    assert_eq!(
        String::from_utf8(encode_record(&set)).unwrap(),
        "[[1,3],[9,10]]"
    );

    for bad in [
        "[[3,1]]",       // reversed
        "[[1,3],[2,4]]", // overlapping
        "[[1,3],[4,5]]", // adjacent (a second spelling of [[1,5]])
        "[[0,3]]",       // part 0 is not a part
    ] {
        assert!(
            serde_json::from_str::<PartNumberSet>(bad).is_err(),
            "{bad} is not a canonical part-number set"
        );
    }
}

#[test]
fn a_retire_payload_is_refused_under_the_wrong_mode_prefix_and_when_it_owes_nothing() {
    let token = RetireToken::Session {
        upload_id: upload("c3"),
        epoch: 5,
        part: None,
    };
    let bytes_key = retire_key(RetireMode::Bytes, &token);
    let records_key = retire_key(RetireMode::Records, &token);
    let session_payload = RetirePayload::Session {};
    let records_payload = RetirePayload::Records {
        parts: Some(PartNumberSet::from_numbers([part(1)])),
        segments: None,
    };

    // The mode lives in the KEY: a records payload under `retire:bytes:` is refused, and
    // vice versa.
    assert!(decode_retire_obligation(&bytes_key, &encode_record(&records_payload)).is_err());
    assert!(decode_retire_obligation(&records_key, &encode_record(&session_payload)).is_err());

    // An obligation that owes nothing is a producer bug, not a value.
    for empty in [
        RetirePayload::Chunks { chunks: Vec::new() },
        RetirePayload::Parts {
            parts: PartNumberSet::default(),
        },
    ] {
        assert!(decode_retire_obligation(&bytes_key, &encode_record(&empty)).is_err());
    }

    // ...including a `retire:records:` obligation naming neither parts nor segments, while
    // EITHER alone is a legitimate one: the published parts of a flat publish, or the
    // dangling segments of one rolled-back `Completing` attempt.
    let records = |segments| RetirePayload::Records {
        parts: None,
        segments,
    };
    assert!(decode_retire_obligation(&records_key, &encode_record(&records(None))).is_err());
    let only_segments = records(Some(segment_group()));
    assert!(decode_retire_obligation(&records_key, &encode_record(&only_segments)).is_ok());
}

/// A **generation** obligation names the bytes the superseded generation held in exactly one
/// of its two forms — a flat chunk list or a `seg:` generation. Both is a payload a drain
/// would half-evidence; **neither** is an obligation that owes nothing, which the drain would
/// clear while the generation's fragments stay on disk with no record naming them (the
/// issue #692 review's obligation-that-owes-nothing finding — this case used to decode).
#[test]
fn a_generation_obligation_names_its_bytes_in_exactly_one_form() {
    let token = RetireToken::Generation {
        inode: 7,
        version: 3,
    };
    let key = retire_key(RetireMode::Bytes, &token);
    let generation = |chunks: Vec<ChunkRef>, segments: Option<metadata::SegmentGroup>| {
        RetirePayload::Generation {
            inode: 7,
            version: 3,
            chunks,
            segments,
        }
    };

    let decode = |payload| decode_retire_obligation(&key, &encode_record(&payload));

    // Exactly one form: a flat generation owes its chunk list, a segmented one its `seg:`
    // generation. Both are legitimate obligations.
    assert!(decode(generation(vec![chunk_ref(1, 10)], None)).is_ok());
    assert!(decode(generation(Vec::new(), Some(segment_group()))).is_ok());

    // Both forms at once: half the payload would be evidenced, then the obligation deleted.
    let both = structural(
        decode(generation(vec![chunk_ref(1, 10)], Some(segment_group()))),
        "retire:",
    );
    assert!(both.contains("never both"), "{both}");

    // Neither: an obligation that owes nothing, admitted as a value until this leg.
    let neither = structural(decode(generation(Vec::new(), None)), "retire:");
    assert!(neither.contains("owes nothing"), "{neither}");
}

// ===========================================================================
// Leg 2 — decode -> encode is the identity on a legacy `pending:` value.
// ===========================================================================

#[test]
fn a_legacy_pending_value_round_trips_byte_identically() {
    // The `skip_serializing_if` identity every `require(key, encode(prior))` CAS depends on
    // (`metadata.rs:1370-1377`/`:1536-1556`, ADR-0047): `renew_pending` and
    // `live_lease_guards` compare the RE-ENCODED prior entry byte-for-byte against the
    // stored bytes, so emitting `"owner":null` would turn every renewal and every lease
    // guard on a pre-#654 entry into a permanent `Conflict`.
    let legacy = br#"{"lease_expiry_millis":1234}"#;
    let decoded: PendingEntry = metadata::decode(legacy).unwrap();
    assert_eq!(decoded.owner, None);
    assert_eq!(decoded.staged, None);
    assert_eq!(
        metadata::encode(&decoded).as_ref(),
        legacy.as_slice(),
        "a legacy pending: entry does not re-encode to its stored bytes"
    );

    // An OWNED entry re-encodes identically across its own renewals too — the same guard
    // path renews it.
    let owned = owned_entry().to_pending();
    let stored = metadata::encode(&owned);
    let decoded: PendingEntry = metadata::decode(&stored).unwrap();
    assert_eq!(metadata::encode(&decoded), stored);
    assert_eq!(decoded, owned);
}

// ===========================================================================
// Leg 3 — the identity/occupancy boundary: occupancy above a lowered cap DECODES.
// ===========================================================================

/// A decoded `AdmissionRecord` whose `count` exceeds its `max_sessions` still decodes:
/// occupancy above a lowered cap is a legitimate live state (admission, a later slice,
/// refuses to grow it further), not a decode error — rejecting it would make a durable
/// record unreadable the day the profile is lowered, the same boundary
/// `MAX_ROOT_SEGMENTS` draws (`metadata.rs:312-321`). Identity relations (leg 1a-1e) are
/// binding; occupancy relations are not.
#[test]
fn admission_record_count_above_max_sessions_still_decodes() {
    let profile = budget();
    let derived = profile.max_sessions();
    let over_cap = json!({ "count": derived + 50, "max_sessions": derived, "profile": {
        "w_ref": profile.w_ref(), "max_part_chunks": profile.max_part_chunks(),
        "max_parts_per_session": profile.max_parts_per_session(),
        "max_inflight_parts": profile.max_inflight_parts(),
        "max_staged_chunks": profile.max_staged_chunks(),
    }});
    let record = decode_admission_record(over_cap.to_string().as_bytes())
        .expect("count above max_sessions is occupancy, not a torn identity");
    assert!(record.count > record.max_sessions);
}
