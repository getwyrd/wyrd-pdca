//! Issue #743: the capability matrix, pure (no network). Proposal 0017 §3 (the matrix and its
//! version keying) and §7 (the failure taxonomy).
//!
//! * **(A) The table.** The shipped profile declares exactly the rows of the brief, each with a
//!   citation: nine required (PUT, GET in three forms, HEAD, DELETE, bulk DELETE, both
//!   listings) and twelve unsupported, each with the issue, the status and the S3 error code it
//!   must be refused with.
//! * **(B) Classification of unsupported rows.** A declared refusal answered exactly passes; a
//!   wrong status or code is `protocol`; **any success is `unexpected-support`**; both are
//!   fatal, and one of them makes the run FAIL whatever else passed.
//! * **(B′) Required rows have checks of their own**, not "any success": each passes only its
//!   own answer, and each negative fixture of the brief fails in its named class.
//! * **(C) Version keying.** Two test-local profiles over different ranges: each range selects
//!   its own profile and so its own verdict, build metadata changes nothing, and a version no
//!   profile covers, one that does not parse and `unknown` select nothing.
//!
//! RED before #743: `wyrd_validate::matrix` does not exist, so the file does not compile.

#![forbid(unsafe_code)]

use wyrd_validate::matrix::{
    judge, select, Class, Declared, Exchange, NoProfile, ObjectAnswer, Operation, Profile, Refusal,
    Row, RunVerdict, State, Tally, Verdict, Version, Versions, PRE_0_1, PROFILES,
};
use wyrd_validate::s3::{DeleteError, DeleteResult, ListedObject, Listing};
use wyrd_validate::{Answered, BodyError, ErrorCode, ResponseHead, S3Error};

// ---- fixtures -------------------------------------------------------------------------------

/// A head with `status` and nothing else.
fn head(status: u16) -> ResponseHead {
    ResponseHead {
        status,
        ..ResponseHead::default()
    }
}

/// An object answer with `head` and `body`.
fn object(head: ResponseHead, body: &[u8]) -> ObjectAnswer {
    Ok(Answered {
        head,
        value: body.to_vec(),
    })
}

/// An S3 error document answer.
fn service<T>(status: u16, code: ErrorCode) -> Result<Answered<T>, S3Error> {
    Err(S3Error::Service {
        status,
        code,
        message: None,
        request_id: None,
    })
}

fn code(text: &str) -> ErrorCode {
    ErrorCode::Code(text.to_string())
}

/// A probe that the endpoint answered with `status` and nothing else.
fn probed(operation: Operation, status: u16) -> Exchange {
    Exchange::Probe {
        operation,
        answer: Ok(Answered {
            head: head(status),
            value: (),
        }),
    }
}

/// The class of a failing verdict; panics on anything else.
fn class_of(verdict: &Verdict) -> Class {
    match verdict {
        Verdict::Fail { class, .. } => *class,
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// The shipped profile's row for `operation`.
fn row(operation: Operation) -> Row {
    *PRE_0_1
        .row(operation)
        .unwrap_or_else(|| panic!("no row for {operation}"))
}

/// A tally in which every row of `profile` passed three times.
fn all_passed(profile: &Profile) -> Tally {
    let mut tally = Tally::new();
    for row in profile.rows {
        for _ in 0..3 {
            tally.record(row.operation, Verdict::Pass);
        }
    }
    tally
}

// The bytes a run wrote, and what it expects of each required row.
const WRITTEN: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const ETAG: &str = "\"9e107d9d372bb6826bd81d3542a419d6\"";

// ---- (A) the table --------------------------------------------------------------------------

/// One row as the brief names it: the operation, and `None` for required, else (issue, status,
/// code).
type BriefRow = (Operation, Option<(u32, u16, Declared)>);

/// The rows the brief names, in order.
const EXPECTED: [BriefRow; 21] = {
    use Operation::*;
    const NOT_IMPLEMENTED: Declared = Declared::Code("NotImplemented");
    const INVALID_REQUEST: Declared = Declared::Code("InvalidRequest");
    [
        (PutObject, None),
        (GetObject, None),
        (GetObjectRange, None),
        (GetObjectConditional, None),
        (HeadObject, None),
        (DeleteObject, None),
        (DeleteObjects, None),
        (ListObjectsV2, None),
        (ListObjects, None),
        (CopyObject, Some((766, 501, NOT_IMPLEMENTED))),
        (CreateMultipartUpload, Some((508, 501, NOT_IMPLEMENTED))),
        (UploadPart, Some((508, 501, NOT_IMPLEMENTED))),
        (CompleteMultipartUpload, Some((508, 501, NOT_IMPLEMENTED))),
        (AbortMultipartUpload, Some((508, 501, NOT_IMPLEMENTED))),
        (ListParts, Some((508, 501, NOT_IMPLEMENTED))),
        (ListMultipartUploads, Some((508, 501, NOT_IMPLEMENTED))),
        (GetBucketLocation, Some((511, 501, NOT_IMPLEMENTED))),
        (CreateBucket, Some((511, 400, INVALID_REQUEST))),
        (DeleteBucket, Some((511, 400, INVALID_REQUEST))),
        (ListBuckets, Some((511, 400, INVALID_REQUEST))),
        (HeadBucket, Some((511, 400, Declared::NoBody))),
    ]
};

#[test]
fn a_the_current_profile_declares_exactly_the_briefs_rows_each_cited() {
    let declared: Vec<BriefRow> = PRE_0_1
        .rows
        .iter()
        .map(|row| {
            let state = match row.state {
                State::Required => None,
                State::Unsupported(Refusal {
                    issue,
                    status,
                    code,
                }) => Some((issue, status, code)),
            };
            (row.operation, state)
        })
        .collect();
    assert_eq!(declared, EXPECTED.to_vec());
    // Every operation the validator knows has its row, exactly once.
    assert_eq!(
        PRE_0_1.rows.iter().map(|r| r.operation).collect::<Vec<_>>(),
        Operation::ALL.to_vec()
    );
    for row in PRE_0_1.rows {
        assert!(
            row.citation.contains("crates/gateway-s3/src/lib.rs:"),
            "{} cites no gateway line: {:?}",
            row.operation,
            row.citation
        );
        if let State::Unsupported(refusal) = row.state {
            assert!(
                row.citation.contains(&format!("#{}", refusal.issue)),
                "{} does not cite its issue #{}: {:?}",
                row.operation,
                refusal.issue,
                row.citation
            );
        }
    }
    // The citations the brief pins (verified against the gateway by `smoke_loopback.rs`).
    for (operation, lines) in [
        (Operation::CopyObject, "lib.rs:1865-1871"),
        (Operation::UploadPart, "lib.rs:1842-1849"),
        (Operation::ListMultipartUploads, "lib.rs:1797-1804"),
        (Operation::GetBucketLocation, "lib.rs:1797-1804"),
        (Operation::CreateBucket, "lib.rs:1810-1815"),
        (Operation::HeadBucket, "lib.rs:1810-1815"),
        (Operation::ListBuckets, "lib.rs:1819-1826"),
    ] {
        assert!(row(operation).citation.contains(lines), "{operation}");
    }
    // One profile, for every build before 0.1.0, shipped, and able to judge a whole sweep.
    assert_eq!(
        PRE_0_1.versions,
        Versions {
            from: None,
            below: Some(Version::release(0, 1, 0))
        }
    );
    assert_eq!(PROFILES.to_vec(), vec![PRE_0_1]);
    assert_eq!(PRE_0_1.gaps(), Vec::<String>::new());
}

#[test]
fn a_a_profile_with_a_missing_row_or_an_uncheckable_required_row_has_gaps() {
    let mut rows = PRE_0_1.rows.to_vec();
    rows.retain(|r| r.operation != Operation::HeadBucket);
    for r in &mut rows {
        if r.operation == Operation::CopyObject {
            r.state = State::Required;
        }
    }
    let profile = Profile {
        name: "gappy",
        versions: PRE_0_1.versions.clone(),
        rows: Vec::leak(rows),
    };
    let gaps = profile.gaps();
    assert!(
        gaps.iter().any(|g| g.contains("no row for HeadBucket")),
        "{gaps:?}"
    );
    assert!(
        gaps.iter()
            .any(|g| g.contains("CopyObject is declared required")),
        "{gaps:?}"
    );
}

// ---- (B) unsupported rows -------------------------------------------------------------------

/// The declared refusal of `row`, as the client reads it.
fn refusal_of(row: &Row) -> Result<Answered<()>, S3Error> {
    let State::Unsupported(refusal) = row.state else {
        panic!("{} is required", row.operation);
    };
    let read = match refusal.code {
        Declared::Code(c) => code(c),
        Declared::NoBody => ErrorCode::NoBody,
    };
    service(refusal.status, read)
}

#[test]
fn b_a_declared_refusal_answered_exactly_passes() {
    for row in PRE_0_1
        .rows
        .iter()
        .filter(|r| matches!(r.state, State::Unsupported(_)))
    {
        let exchange = Exchange::Probe {
            operation: row.operation,
            answer: refusal_of(row),
        };
        assert_eq!(judge(row, &exchange), Verdict::Pass, "{}", row.operation);
        assert_eq!(PRE_0_1.judge(&exchange), Verdict::Pass, "{}", row.operation);
    }
}

#[test]
fn b_a_wrong_status_or_code_is_protocol_and_fatal() {
    let copy = row(Operation::CopyObject);
    let head_bucket = row(Operation::HeadBucket);
    let create_bucket = row(Operation::CreateBucket);
    for (case, row, answer) in [
        (
            "CopyObject 400 for 501",
            copy,
            service(400, code("NotImplemented")),
        ),
        (
            "CopyObject 501 with another code",
            copy,
            service(501, code("InternalError")),
        ),
        ("CopyObject 500", copy, service(500, code("InternalError"))),
        (
            "CopyObject 501 with a document naming no code",
            copy,
            service(501, ErrorCode::MissingInXml),
        ),
        (
            "CopyObject 501 unreadable",
            copy,
            Err(S3Error::Unreadable {
                status: 501,
                request_id: None,
                detail: "cut off".into(),
            }),
        ),
        (
            "HeadBucket 400 with a code where none is declared",
            head_bucket,
            service(400, code("InvalidRequest")),
        ),
        (
            "CreateBucket 501 where 400 is declared",
            create_bucket,
            service(501, code("NotImplemented")),
        ),
        (
            "CreateBucket 409",
            create_bucket,
            service(409, code("BucketAlreadyOwnedByYou")),
        ),
    ] {
        let verdict = judge(
            &row,
            &Exchange::Probe {
                operation: row.operation,
                answer,
            },
        );
        assert_eq!(class_of(&verdict), Class::Protocol, "{case}: {verdict:?}");
    }
    assert!(Class::Protocol.is_fatal());
}

#[test]
fn b_any_success_to_an_unsupported_row_is_unexpected_support_and_fatal() {
    for row in PRE_0_1
        .rows
        .iter()
        .filter(|r| matches!(r.state, State::Unsupported(_)))
    {
        for status in [200, 201, 204, 206, 299] {
            let verdict = judge(row, &probed(row.operation, status));
            assert_eq!(
                class_of(&verdict),
                Class::UnexpectedSupport,
                "{} answered {status}: {verdict:?}",
                row.operation
            );
        }
        // A success the client cannot read is still a success.
        let unreadable = Exchange::Probe {
            operation: row.operation,
            answer: Err(S3Error::Unreadable {
                status: 200,
                request_id: None,
                detail: "the SDK could not read it".into(),
            }),
        };
        assert_eq!(
            class_of(&judge(row, &unreadable)),
            Class::UnexpectedSupport,
            "{}",
            row.operation
        );
    }
    // A PUT acknowledged early came after a success head too.
    let mut rows = PRE_0_1.rows.to_vec();
    for r in &mut rows {
        if r.operation == Operation::PutObject {
            r.state = State::Unsupported(Refusal {
                issue: 1,
                status: 501,
                code: Declared::Code("NotImplemented"),
            });
        }
    }
    let put_unsupported = rows[0];
    let early = Exchange::PutObject {
        answer: Err(S3Error::Body(BodyError::AcknowledgedEarly {
            declared: 10,
            produced: 4,
            request_id: None,
        })),
    };
    assert_eq!(
        class_of(&judge(&put_unsupported, &early)),
        Class::UnexpectedSupport
    );
    assert!(Class::UnexpectedSupport.is_fatal());
}

#[test]
fn b_no_answer_to_an_unsupported_row_is_availability_not_a_pass() {
    let copy = row(Operation::CopyObject);
    let verdict = judge(
        &copy,
        &Exchange::Probe {
            operation: Operation::CopyObject,
            answer: Err(S3Error::NoResponse {
                detail: "connection refused".into(),
            }),
        },
    );
    assert_eq!(class_of(&verdict), Class::Availability);
    assert!(!Class::Availability.is_fatal());
}

#[test]
fn b_one_fatal_result_makes_the_run_fail_whatever_else_passed() {
    let pass = all_passed(&PRE_0_1);
    assert_eq!(pass.verdict(&PRE_0_1), RunVerdict::Pass);

    let mut fail = all_passed(&PRE_0_1);
    let copy = row(Operation::CopyObject);
    fail.record(
        Operation::CopyObject,
        judge(&copy, &probed(Operation::CopyObject, 200)),
    );
    assert_eq!(fail.verdict(&PRE_0_1), RunVerdict::Fail);
    assert!(fail.findings()[0]
        .detail
        .contains("CopyObject answered 200"));

    // A protocol failure likewise, and a fatal failure outranks an availability one.
    let mut protocol = all_passed(&PRE_0_1);
    protocol.record(
        Operation::CopyObject,
        judge(
            &copy,
            &Exchange::Probe {
                operation: Operation::CopyObject,
                answer: service(400, code("InvalidRequest")),
            },
        ),
    );
    assert_eq!(protocol.verdict(&PRE_0_1), RunVerdict::Fail);
    let mut both = fail.clone();
    both.record(
        Operation::HeadBucket,
        Verdict::Fail {
            class: Class::Availability,
            detail: "timeout".into(),
        },
    );
    assert_eq!(both.verdict(&PRE_0_1), RunVerdict::Fail);

    // Availability alone, an unchecked exchange, or a row that never passed: never a pass.
    let mut unavailable = all_passed(&PRE_0_1);
    unavailable.record(
        Operation::GetObject,
        Verdict::Fail {
            class: Class::Availability,
            detail: "503".into(),
        },
    );
    assert_eq!(unavailable.verdict(&PRE_0_1), RunVerdict::Inconclusive);
    let mut unchecked = all_passed(&PRE_0_1);
    unchecked.record(
        Operation::CopyObject,
        Verdict::Unchecked {
            detail: "no check".into(),
        },
    );
    assert_eq!(unchecked.verdict(&PRE_0_1), RunVerdict::Inconclusive);
    let mut partial = Tally::new();
    partial.record(Operation::PutObject, Verdict::Pass);
    assert_eq!(partial.verdict(&PRE_0_1), RunVerdict::Inconclusive);
    assert_eq!(partial.unexercised(&PRE_0_1).len(), 20);
}

// ---- (B′) required rows: each its own check -------------------------------------------------

/// Judge `exchange` by the shipped profile.
fn required(exchange: &Exchange) -> Verdict {
    PRE_0_1.judge(exchange)
}

/// `verdict` is a failure of `class`, and it makes a run in which everything else passed FAIL
/// (or, for availability, never PASS).
fn fails_as(operation: Operation, verdict: Verdict, class: Class) {
    assert_eq!(class_of(&verdict), class, "{operation}: {verdict:?}");
    let mut tally = all_passed(&PRE_0_1);
    tally.record(operation, verdict);
    let expected = if class.is_fatal() {
        RunVerdict::Fail
    } else {
        RunVerdict::Inconclusive
    };
    assert_eq!(tally.verdict(&PRE_0_1), expected, "{operation}");
}

fn with_etag(status: u16, etag: &str) -> ResponseHead {
    ResponseHead {
        status,
        etag: vec![etag.to_string()],
        ..ResponseHead::default()
    }
}

#[test]
fn b_prime_put_object_passes_only_a_200_with_one_etag() {
    let put = |answer| Exchange::PutObject { answer };
    assert_eq!(
        required(&put(object(with_etag(200, ETAG), b""))),
        Verdict::Pass
    );
    let op = Operation::PutObject;
    fails_as(op, required(&put(object(head(200), b""))), Class::Protocol);
    fails_as(
        op,
        required(&put(object(
            ResponseHead {
                status: 200,
                etag: vec![ETAG.into(), ETAG.into()],
                ..ResponseHead::default()
            },
            b"",
        ))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&put(object(with_etag(201, ETAG), b""))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&put(service(501, code("NotImplemented")))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&put(service(403, code("SignatureDoesNotMatch")))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&put(service(503, code("SlowDown")))),
        Class::Availability,
    );
}

#[test]
fn b_prime_a_whole_get_passes_only_the_bytes_written() {
    let get = |answer| Exchange::GetObject {
        written: WRITTEN.to_vec(),
        answer,
    };
    let op = Operation::GetObject;
    assert_eq!(required(&get(object(head(200), WRITTEN))), Verdict::Pass);
    // A GET body one byte off.
    let mut off = WRITTEN.to_vec();
    off[17] ^= 0x01;
    fails_as(
        op,
        required(&get(object(head(200), &off))),
        Class::Integrity,
    );
    fails_as(
        op,
        required(&get(object(head(200), &WRITTEN[..WRITTEN.len() - 1]))),
        Class::Integrity,
    );
    fails_as(
        op,
        required(&get(object(head(200), &[WRITTEN, b"!"].concat()))),
        Class::Integrity,
    );
    fails_as(
        op,
        required(&get(object(head(206), WRITTEN))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&get(service(404, code("NoSuchKey")))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&get(Err(S3Error::Body(BodyError::Transport {
            declared: Some(36),
            received: 9,
            detail: "reset".into(),
        })))),
        Class::Availability,
    );
}

#[test]
fn b_prime_a_ranged_get_passes_only_a_206_with_its_content_range_and_bytes() {
    let (first, last) = (10_u64, 19_u64);
    let slice = &WRITTEN[10..=19];
    let range = |answer| Exchange::GetObjectRange {
        written: WRITTEN.to_vec(),
        first,
        last,
        answer,
    };
    let partial = |content_range: &str| ResponseHead {
        status: 206,
        content_range: vec![content_range.to_string()],
        ..ResponseHead::default()
    };
    let op = Operation::GetObjectRange;
    assert_eq!(
        required(&range(object(partial("bytes 10-19/36"), slice))),
        Verdict::Pass
    );
    // A 200 with the whole body to the ranged GET.
    fails_as(
        op,
        required(&range(object(head(200), WRITTEN))),
        Class::Protocol,
    );
    // A 206 with the wrong Content-Range.
    for wrong in [
        "bytes 10-19/35",
        "bytes 10-20/36",
        "bytes 0-35/36",
        "bytes 10-19/*",
        "bytes  10-19/36",
        "bytes +10-19/36",
        "bytes 10-19/36 ",
        "bits 10-19/36",
        "bytes=10-19/36",
    ] {
        fails_as(
            op,
            required(&range(object(partial(wrong), slice))),
            Class::Protocol,
        );
    }
    // The unit is case-insensitive (RFC 9110 §14.1), and a number is any `1*DIGIT`.
    for right in ["BYTES 10-19/36", "Bytes 10-19/36", "bytes 010-19/36"] {
        assert_eq!(
            required(&range(object(partial(right), slice))),
            Verdict::Pass,
            "{right}"
        );
    }
    fails_as(
        op,
        required(&range(object(head(206), slice))),
        Class::Protocol,
    );
    // The right Content-Range over the wrong bytes.
    fails_as(
        op,
        required(&range(object(partial("bytes 10-19/36"), &WRITTEN[11..=20]))),
        Class::Integrity,
    );
    // A range outside the object written describes no request: never a pass.
    let outside = Exchange::GetObjectRange {
        written: WRITTEN.to_vec(),
        first: 30,
        last: 40,
        answer: object(partial("bytes 30-40/36"), b""),
    };
    assert!(matches!(required(&outside), Verdict::Unchecked { .. }));
}

#[test]
fn b_prime_a_conditional_get_passes_only_a_304_with_no_body() {
    let conditional = |answer| Exchange::GetObjectConditional { answer };
    let op = Operation::GetObjectConditional;
    assert_eq!(
        required(&conditional(service(304, ErrorCode::NoBody))),
        Verdict::Pass
    );
    // A 200 to the conditional GET.
    fails_as(
        op,
        required(&conditional(object(head(200), WRITTEN))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&conditional(service(304, code("NotModified")))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&conditional(service(412, code("PreconditionFailed")))),
        Class::Protocol,
    );
}

#[test]
fn b_prime_a_head_passes_only_the_size_and_the_puts_etag() {
    let size = WRITTEN.len() as u64;
    let head_of = |answer| Exchange::HeadObject {
        size,
        etag: ETAG.to_string(),
        answer,
    };
    let answered = |length: &str, etag: &str| ResponseHead {
        status: 200,
        content_length: vec![length.to_string()],
        etag: vec![etag.to_string()],
        ..ResponseHead::default()
    };
    let op = Operation::HeadObject;
    assert_eq!(
        required(&head_of(object(answered("36", ETAG), b""))),
        Verdict::Pass
    );
    fails_as(
        op,
        required(&head_of(object(answered("35", ETAG), b""))),
        Class::Integrity,
    );
    fails_as(
        op,
        required(&head_of(object(answered("36", "\"another\""), b""))),
        Class::Oracle,
    );
    fails_as(
        op,
        required(&head_of(object(answered("+36", ETAG), b""))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&head_of(object(
            ResponseHead {
                status: 200,
                content_length: vec!["36".into()],
                ..ResponseHead::default()
            },
            b"",
        ))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&head_of(service(404, ErrorCode::NoBody))),
        Class::Protocol,
    );
}

/// A listing answer of `objects`.
fn listing(objects: &[(&str, u64)], is_truncated: bool) -> Result<Answered<Listing>, S3Error> {
    Ok(Answered {
        head: head(200),
        value: Listing {
            objects: objects
                .iter()
                .map(|(key, size)| ListedObject {
                    key: (*key).to_string(),
                    size: *size,
                })
                .collect(),
            common_prefixes: Vec::new(),
            is_truncated,
        },
    })
}

#[test]
fn b_prime_both_listings_pass_only_exactly_the_keys_written_with_their_sizes() {
    // Written in another order than key order: the listing is in key order.
    let written = vec![
        ("r/s/obj-4096".to_string(), 4096),
        ("r/s/obj-0".to_string(), 0),
    ];
    let exact = [("r/s/obj-0", 0), ("r/s/obj-4096", 4096)];
    for op in [Operation::ListObjectsV2, Operation::ListObjects] {
        let list = |answer| match op {
            Operation::ListObjectsV2 => Exchange::ListObjectsV2 {
                written: written.clone(),
                answer,
            },
            _ => Exchange::ListObjects {
                written: written.clone(),
                answer,
            },
        };
        assert_eq!(
            required(&list(listing(&exact, false))),
            Verdict::Pass,
            "{op}"
        );
        for (case, answer) in [
            ("a key missing", listing(&exact[..1], false)),
            (
                "a key nobody wrote",
                listing(&[exact[0], exact[1], ("r/s/obj-9", 9)], false),
            ),
            (
                "a size changed",
                listing(&[exact[0], ("r/s/obj-4096", 4095)], false),
            ),
            ("out of key order", listing(&[exact[1], exact[0]], false)),
            ("truncated", listing(&exact, true)),
        ] {
            assert_eq!(
                class_of(&required(&list(answer.clone()))),
                Class::Oracle,
                "{op}: {case}"
            );
            fails_as(op, required(&list(answer)), Class::Oracle);
        }
        fails_as(
            op,
            required(&list(service(404, code("NoSuchBucket")))),
            Class::Protocol,
        );
    }
}

#[test]
fn b_prime_a_delete_passes_only_a_204_after_which_the_key_is_gone() {
    let delete = |answer, head_after| Exchange::DeleteObject { answer, head_after };
    let op = Operation::DeleteObject;
    assert_eq!(
        required(&delete(
            object(head(204), b""),
            service(404, ErrorCode::NoBody)
        )),
        Verdict::Pass
    );
    // Still readable after its 204.
    fails_as(
        op,
        required(&delete(object(head(204), b""), object(head(200), b""))),
        Class::Oracle,
    );
    fails_as(
        op,
        required(&delete(
            object(head(200), b""),
            service(404, ErrorCode::NoBody),
        )),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&delete(
            object(head(204), b""),
            service(403, ErrorCode::NoBody),
        )),
        Class::Protocol,
    );
}

#[test]
fn b_prime_a_bulk_delete_passes_only_every_key_deleted_and_no_error_row() {
    let keys = vec!["r/a".to_string(), "r/b".to_string()];
    let result = |deleted: &[&str], errors: &[(&str, &str)]| {
        Ok(Answered {
            head: head(200),
            value: DeleteResult {
                deleted: deleted.iter().map(|k| (*k).to_string()).collect(),
                errors: errors
                    .iter()
                    .map(|(key, code)| DeleteError {
                        key: (*key).to_string(),
                        code: (*code).to_string(),
                    })
                    .collect(),
            },
        })
    };
    let bulk = |answer| Exchange::DeleteObjects {
        keys: keys.clone(),
        answer,
    };
    let op = Operation::DeleteObjects;
    assert_eq!(required(&bulk(result(&["r/b", "r/a"], &[]))), Verdict::Pass);
    // A DeleteObjects 200 carrying one <Error> row.
    fails_as(
        op,
        required(&bulk(result(&["r/a"], &[("r/b", "InternalError")]))),
        Class::Protocol,
    );
    fails_as(op, required(&bulk(result(&["r/a"], &[]))), Class::Protocol);
    fails_as(
        op,
        required(&bulk(result(&["r/a", "r/b", "r/a"], &[]))),
        Class::Protocol,
    );
    fails_as(
        op,
        required(&bulk(service(400, code("MalformedXML")))),
        Class::Protocol,
    );
}

#[test]
fn b_prime_a_required_row_with_no_check_of_its_own_never_passes_on_success_alone() {
    let mut rows = PRE_0_1.rows.to_vec();
    for r in &mut rows {
        if r.operation == Operation::CopyObject {
            r.state = State::Required;
        }
    }
    let copy_required = *rows
        .iter()
        .find(|r| r.operation == Operation::CopyObject)
        .expect("a CopyObject row");
    assert!(matches!(
        judge(&copy_required, &probed(Operation::CopyObject, 200)),
        Verdict::Unchecked { .. }
    ));
}

// ---- (C) version keying ---------------------------------------------------------------------

const NOT_IMPLEMENTED: Refusal = Refusal {
    issue: 9001,
    status: 501,
    code: Declared::Code("NotImplemented"),
};

static EARLY_ROWS: [Row; 1] = [Row {
    operation: Operation::HeadObject,
    state: State::Unsupported(NOT_IMPLEMENTED),
    citation: "test: unsupported before 0.2.0",
}];

static LATE_ROWS: [Row; 1] = [Row {
    operation: Operation::HeadObject,
    state: State::Required,
    citation: "test: required from 0.2.0",
}];

/// Two profiles over different ranges, in which HeadObject flips from unsupported to required.
fn two_profiles() -> [Profile; 2] {
    [
        Profile {
            name: "early",
            versions: Versions {
                from: None,
                below: Some(Version::release(0, 2, 0)),
            },
            rows: &EARLY_ROWS,
        },
        Profile {
            name: "late",
            versions: Versions {
                from: Some(Version::release(0, 2, 0)),
                below: Some(Version::release(0, 3, 0)),
            },
            rows: &LATE_ROWS,
        },
    ]
}

fn selected(profiles: &[Profile], version: &str) -> Option<&'static str> {
    select(profiles, version).ok().map(|profile| profile.name)
}

#[test]
fn c_each_version_range_selects_its_own_profile_and_its_own_verdict() {
    let profiles = two_profiles();
    let size = WRITTEN.len() as u64;
    let success = Exchange::HeadObject {
        size,
        etag: ETAG.into(),
        answer: object(
            ResponseHead {
                status: 200,
                content_length: vec!["36".into()],
                etag: vec![ETAG.into()],
                ..ResponseHead::default()
            },
            b"",
        ),
    };
    let refused = Exchange::HeadObject {
        size,
        etag: ETAG.into(),
        answer: service(501, code("NotImplemented")),
    };
    let early = select(&profiles, "0.1.5").expect("0.1.5 is covered");
    let late = select(&profiles, "0.2.1").expect("0.2.1 is covered");
    assert_eq!((early.name, late.name), ("early", "late"));
    // The same answers, opposite verdicts.
    assert_eq!(class_of(&early.judge(&success)), Class::UnexpectedSupport);
    assert_eq!(late.judge(&success), Verdict::Pass);
    assert_eq!(early.judge(&refused), Verdict::Pass);
    assert_eq!(class_of(&late.judge(&refused)), Class::Protocol);
}

#[test]
fn c_build_metadata_does_not_change_the_selection_and_pre_releases_sort_below_their_release() {
    let profiles = two_profiles();
    for (version, profile) in [
        ("0.0.0", "early"),
        ("0.1.5", "early"),
        ("0.1.5+git.3.1a2b3c4", "early"),
        ("0.0.0+git.1a2b3c4.dirty", "early"),
        ("0.2.0-rc.1", "early"),
        ("0.2.0-rc.1+git.2.1a2b3c4", "early"),
        ("0.2.0", "late"),
        ("0.2.0+git.1a2b3c4", "late"),
        ("0.2.1+git.7.1a2b3c4.dirty", "late"),
        ("0.2.999", "late"),
        ("0.3.0-alpha", "late"),
    ] {
        assert_eq!(selected(&profiles, version), Some(profile), "{version}");
    }
    // The shipped profile covers every build so far, and nothing from 0.1.0 on.
    for version in [
        "0.0.0+git.1a2b3c4",
        "0.0.0+git.1a2b3c4.dirty",
        "0.0.0+git.unknown",
    ] {
        assert_eq!(selected(&PROFILES, version), Some("pre-0.1"), "{version}");
    }
    for version in ["0.1.0", "0.1.0+git.3.1a2b3c4", "1.0.0"] {
        assert_eq!(selected(&PROFILES, version), None, "{version}");
    }
    // Semver precedence, the example chain of semver.org §11.
    let chain = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];
    for pair in chain.windows(2) {
        let (a, b) = (
            Version::parse(pair[0]).expect("parses"),
            Version::parse(pair[1]).expect("parses"),
        );
        assert!(a < b, "{} < {}", pair[0], pair[1]);
    }
}

#[test]
fn c_an_uncovered_a_malformed_and_an_unknown_version_select_nothing() {
    let profiles = two_profiles();
    for version in ["0.3.0", "1.0.0", "0.3.0+git.1a2b3c4"] {
        assert!(
            matches!(select(&profiles, version), Err(NoProfile::Uncovered { .. })),
            "{version}"
        );
    }
    for version in [
        "unknown",
        "",
        "0.1",
        "0.1.0.0",
        "v0.1.0",
        "0.01.0",
        "00.1.0",
        "+0.1.0",
        "0.1.0-",
        "0.1.0+",
        "0.1.0-01",
        "0.1.0-rc..1",
        "0.1.0+git..1",
        "0.1.0_rc",
        " 0.1.0",
        "0.1.0 ",
        "0.1.0+git.1a2b3c4+again",
        "18446744073709551616.0.0",
    ] {
        assert!(
            matches!(
                select(&profiles, version),
                Err(NoProfile::NotAVersion { .. })
            ),
            "{version:?}"
        );
    }
    // Overlapping ranges select nothing rather than one of them.
    let overlapping = [
        two_profiles()[0].clone(),
        Profile {
            name: "overlap",
            versions: Versions {
                from: Some(Version::release(0, 1, 0)),
                below: None,
            },
            rows: &LATE_ROWS,
        },
    ];
    assert!(matches!(
        select(&overlapping, "0.1.5"),
        Err(NoProfile::Ambiguous { .. })
    ));
}
