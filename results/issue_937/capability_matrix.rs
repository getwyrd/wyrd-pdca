//! The capability matrix (proposal 0017 §3, §7; #937): today's profile, the classification of
//! every row state, version keying, and the run verdict. Pure: every leg calls the production
//! `wyrd_validate::matrix` functions with a fixed input. No network, no second process.

#![forbid(unsafe_code)]

use wyrd_validate::matrix::{
    classify, is_strong_etag, parse_version, select, select_reported, verdict, Answer, Arrived,
    Body, Bound, DeclaredCode, DeleteFailure, DeleteResult, Exercise, Expected, FailureClass,
    Listed, NoProfile, Operation, Outcome, PreId, Profile, Release, ReportedVersion, Response, Row,
    RowResult, RunVerdict, Unproved, VersionError, VersionRange, PRE_0_1, PROFILES,
};
use wyrd_validate::ErrorCode;
use FailureClass::{Integrity, Oracle, Protocol, UnexpectedSupport};
use Operation::*;

// --- Fixtures -------------------------------------------------------------------------------

const OBJECT: &[u8] = b"0123456789abcdef";
const ETAG: &str = "\"d41d8cd98f00b204e9800998ecf8427e\"";
/// A strong entity-tag the object does not have.
const OTHER: &str = "\"0123456789abcdef0123456789abcdef\"";
const NIMPL: DeclaredCode = DeclaredCode::Code("NotImplemented");
const ANY: VersionRange = range(None, None);

const fn range(from: Option<Bound>, below: Option<Bound>) -> VersionRange {
    VersionRange { from, below }
}

fn resp(status: u16) -> Response {
    Response::new(status)
}

fn ans(response: Response) -> Answer {
    Answer::Response(response)
}

fn just(status: u16) -> Answer {
    ans(resp(status))
}

fn body(status: u16, body: Body) -> Answer {
    ans(resp(status).with_body(body))
}

fn none() -> Answer {
    Answer::NoAnswer {
        why: "timed out before a response head".to_string(),
    }
}

fn code(c: &str) -> ErrorCode {
    ErrorCode::Code(c.to_string())
}

fn refusal(status: u16, c: &str) -> Answer {
    ans(resp(status).with_error_code(code(c)))
}

fn row(op: Operation) -> &'static Row {
    PRE_0_1.row(op).expect("pre-0.1 declares the row")
}

fn class_of(result: &RowResult) -> Option<FailureClass> {
    match result {
        RowResult::Fatal { class, .. } => Some(*class),
        _ => None,
    }
}

fn fatal(class: FailureClass, detail: &str) -> RowResult {
    RowResult::Fatal {
        class,
        detail: detail.to_string(),
    }
}

fn is_unsettled(result: &RowResult) -> bool {
    matches!(result, RowResult::Unsettled { .. })
}

fn bytes(b: &[u8]) -> Body {
    Body::Bytes(b.to_vec())
}

fn whole() -> Body {
    bytes(OBJECT)
}

fn part() -> Body {
    bytes(&OBJECT[2..=5])
}

fn cut(arrived: Arrived) -> Body {
    Body::Cut {
        arrived,
        why: "connection reset after the head".to_string(),
    }
}

fn cutb(b: &[u8]) -> Body {
    cut(Arrived::Bytes(b.to_vec()))
}

fn cutn(n: u64) -> Body {
    cut(Arrived::Count(n))
}

fn bad() -> Body {
    Body::Unreadable {
        why: "not an XML document".to_string(),
    }
}

fn put(answer: Answer, ended: bool) -> Exercise {
    Exercise::PutObject {
        answer,
        request_body_ended: ended,
    }
}

fn good_put() -> Exercise {
    put(ans(resp(200).with_etag(ETAG)), true)
}

fn other(operation: Operation, answer: Answer) -> Exercise {
    Exercise::Other { operation, answer }
}

/// `HeadBucket` answered its declared 400, the client reading no body as its code, with `b`.
fn hb400(b: Body) -> Exercise {
    let r = resp(400).with_error_code(ErrorCode::NoBody).with_body(b);
    other(HeadBucket, ans(r))
}

fn get(answer: Answer) -> Exercise {
    Exercise::GetObject {
        object: OBJECT.to_vec(),
        answer,
    }
}

fn ranged_at(first: u64, last: u64, answer: Answer) -> Exercise {
    Exercise::GetObjectRanged {
        object: OBJECT.to_vec(),
        first,
        last,
        answer,
    }
}

fn ranged(answer: Answer) -> Exercise {
    ranged_at(2, 5, answer)
}

fn r206(content_range: &str, body: Body) -> Exercise {
    ranged(ans(resp(206)
        .with_content_range(content_range)
        .with_body(body)))
}

fn r206_ok(body: Body) -> Exercise {
    r206("bytes 2-5/16", body)
}

/// The matching conditional GET, having sent `If-None-Match: sent` for the object (tag `ETAG`).
fn hit_sending(sent: &str, answer: Answer) -> Exercise {
    Exercise::GetObjectIfNoneMatchHit {
        sent: sent.to_string(),
        etag: ETAG.to_string(),
        answer,
    }
}

fn hit(answer: Answer) -> Exercise {
    hit_sending(ETAG, answer)
}

/// The non-matching conditional GET, having sent `If-None-Match: sent`.
fn miss_sending(sent: &str, answer: Answer) -> Exercise {
    Exercise::GetObjectIfNoneMatchMiss {
        sent: sent.to_string(),
        etag: ETAG.to_string(),
        object: OBJECT.to_vec(),
        answer,
    }
}

fn miss(answer: Answer) -> Exercise {
    miss_sending(OTHER, answer)
}

/// A `HEAD` of the object (16 bytes, `ETAG`) answered with `answer`.
fn head(answer: Answer) -> Exercise {
    Exercise::HeadObject {
        size: 16,
        etag: ETAG.to_string(),
        answer,
    }
}

fn head_of(answer: Response) -> Exercise {
    head(ans(answer.with_content_length(16).with_etag(ETAG)))
}

fn del(answer: Answer, follow_up: Answer) -> Exercise {
    Exercise::DeleteObject { answer, follow_up }
}

fn listed(key: &str, size: u64) -> Listed {
    Listed {
        key: key.to_string(),
        size,
    }
}

fn listing() -> Vec<Listed> {
    vec![listed("p/a", 16), listed("p/b", 3)]
}

fn list_of(listed: Vec<Listed>) -> Answer {
    body(200, Body::Listing(listed))
}

fn list_v2(answer: Answer) -> Exercise {
    Exercise::ListObjectsV2 {
        expected: listing(),
        answer,
    }
}

fn list_v1(answer: Answer) -> Exercise {
    Exercise::ListObjects {
        expected: listing(),
        answer,
    }
}

fn strings(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|k| k.to_string()).collect()
}

fn result(deleted: &[&str], errors: Vec<DeleteFailure>) -> Answer {
    body(
        200,
        Body::DeleteResult(DeleteResult {
            deleted: strings(deleted),
            errors,
        }),
    )
}

/// A `<DeleteResult>` naming `p/a` and `p/b` under `<Deleted>`.
fn all() -> Answer {
    result(&["p/a", "p/b"], vec![])
}

fn heads(a: Answer, b: Answer) -> Vec<(String, Answer)> {
    vec![("p/a".to_string(), a), ("p/b".to_string(), b)]
}

fn hs(a: u16, b: u16) -> Vec<(String, Answer)> {
    heads(just(a), just(b))
}

/// A `DeleteObjects` of `p/a` and `p/b`.
fn bulk(answer: Answer, follow_ups: Vec<(String, Answer)>) -> Exercise {
    Exercise::DeleteObjects {
        keys: strings(&["p/a", "p/b"]),
        answer,
        follow_ups,
    }
}

/// A `DeleteObjects` of `keys`, every one under `<Deleted>`, then one `HEAD` per `(key, status)`.
fn bulk_of(keys: &[&str], heads: &[(&str, u16)]) -> Exercise {
    Exercise::DeleteObjects {
        answer: result(keys, vec![]),
        keys: strings(keys),
        follow_ups: heads
            .iter()
            .map(|(k, s)| (k.to_string(), just(*s)))
            .collect(),
    }
}

/// One passing exercise per row of `pre-0.1`; each test replaces one entry to isolate a leg.
fn passing_run() -> Vec<Exercise> {
    let mut run = vec![
        good_put(),
        get(body(200, whole())),
        r206_ok(part()),
        hit(just(304)),
        miss(body(200, whole())),
        head_of(resp(200)),
        del(just(204), just(404)),
        bulk(all(), hs(404, 404)),
        list_v2(list_of(listing())),
        list_v1(list_of(listing())),
    ];
    for r in PRE_0_1.rows() {
        if let Expected::Unsupported { status, code, .. } = r.expected {
            let response = match code {
                DeclaredCode::Code(c) => resp(status).with_error_code(ErrorCode::Code(c.into())),
                DeclaredCode::NoBody => resp(status).with_error_code(ErrorCode::NoBody),
            };
            run.push(other(r.operation, ans(response)));
        }
    }
    run
}

fn replace(run: &mut [Exercise], exercise: Exercise) {
    let op = exercise.operation();
    let slot = run
        .iter_mut()
        .find(|e| e.operation() == op)
        .expect("the run exercises the row");
    *slot = exercise;
}

fn verdict_with(exercise: Exercise) -> RunVerdict {
    let mut run = passing_run();
    replace(&mut run, exercise);
    verdict(&PRE_0_1, &run)
}

fn fails(exercise: Exercise, class: FailureClass) {
    let op = exercise.operation();
    let v = verdict_with(exercise);
    assert_eq!(v.outcome, Outcome::Fail, "{v:?}");
    assert_eq!(v.fatal.len(), 1, "{v:?}");
    assert_eq!(
        (v.fatal[0].operation, v.fatal[0].class),
        (op, class),
        "{v:?}"
    );
}

/// INCONCLUSIVE, no fatal result, and exactly one unproved row, the exercised one, for `why`.
fn unproved_as(exercise: Exercise, why: fn(&Unproved) -> bool) {
    let op = exercise.operation();
    let v = verdict_with(exercise);
    assert_eq!(v.outcome, Outcome::Inconclusive, "{v:?}");
    assert!(v.fatal.is_empty(), "{v:?}");
    assert_eq!(v.unproved.len(), 1, "{v:?}");
    assert_eq!(v.unproved[0].0, op, "{v:?}");
    assert!(why(&v.unproved[0].1), "{v:?}");
}

fn unproved(exercise: Exercise) {
    unproved_as(exercise, |u| matches!(u, Unproved::Unsettled { .. }));
}

fn no_answer(exercise: Exercise) {
    unproved_as(exercise, |u| matches!(u, Unproved::NoAnswer { .. }));
}

// --- (A) Today's profile --------------------------------------------------------------------

#[test]
fn pre_0_1_declares_exactly_todays_rows() {
    let unsupported = |issue, status, code| Expected::Unsupported {
        issue,
        status,
        code,
    };
    let (r, n508) = (Expected::Required, unsupported(508, 501, NIMPL));
    let invalid = unsupported(511, 400, DeclaredCode::Code("InvalidRequest"));
    let mut want: Vec<(Operation, Expected)> = vec![
        (PutObject, r),
        (GetObject, r),
        (GetObjectRanged, r),
        (GetObjectIfNoneMatchHit, r),
        (GetObjectIfNoneMatchMiss, r),
        (HeadObject, r),
        (DeleteObject, r),
        (DeleteObjects, r),
        (ListObjectsV2, r),
        (ListObjects, r),
        (CopyObject, unsupported(766, 501, NIMPL)),
        (CreateMultipartUpload, n508),
        (UploadPart, n508),
        (CompleteMultipartUpload, n508),
        (AbortMultipartUpload, n508),
        (ListParts, n508),
        (ListMultipartUploads, n508),
        (GetBucketLocation, unsupported(511, 501, NIMPL)),
        (CreateBucket, invalid),
        (DeleteBucket, invalid),
        (ListBuckets, invalid),
        (HeadBucket, unsupported(511, 400, DeclaredCode::NoBody)),
    ];
    // Exactly these rows, in any order: none `supported, not checked` today.
    let rows = PRE_0_1.rows();
    let mut got: Vec<_> = rows.iter().map(|r| (r.operation, r.expected)).collect();
    got.sort_by_key(|(op, _)| *op);
    want.sort_by_key(|(op, _)| *op);
    assert_eq!(got, want, "pre-0.1 declares exactly today's rows");
    for r in rows {
        assert!(
            r.cites.starts_with("crates/gateway-s3/src/lib.rs:"),
            "{r:?}"
        );
    }
    assert_eq!(PRE_0_1.name(), "pre-0.1");
}

fn selected(table: &[Profile], v: &str) -> Result<&'static str, NoProfile> {
    select_reported(table, v).map(Profile::name)
}

#[test]
fn pre_0_1_covers_every_version_below_0_1_0() {
    let below = "0.0.0 0.0.0+git.abc1234 0.0.0+git.abc1234.dirty 0.0.9 0.0.99-alpha 0.1.0-rc.1";
    for v in below.split(' ') {
        assert_eq!(selected(PROFILES, v), Ok("pre-0.1"), "{v}");
    }
    for v in "0.1.0 0.1.0+git.dirty 0.1.0+git.3.abc1234 0.2.0 1.0.0".split(' ') {
        let no_match = matches!(select_reported(PROFILES, v), Err(NoProfile::NoMatch(_)));
        assert!(no_match, "{v} is not covered by pre-0.1");
    }
}

#[test]
fn a_full_passing_run_against_pre_0_1_passes() {
    let v = verdict(&PRE_0_1, &passing_run());
    assert_eq!(v.outcome, Outcome::Pass, "{v:?}");
    assert!(v.fatal.is_empty() && v.unproved.is_empty() && v.not_covered.is_empty());
    let rows: Vec<Operation> = PRE_0_1.rows().iter().map(|r| r.operation).collect();
    assert_eq!(v.proved, rows);
}

// --- (B1) Unsupported rows ------------------------------------------------------------------

#[test]
fn unsupported_row_passes_only_on_the_declared_status_and_code() {
    let copy = row(CopyObject);
    let ex = |answer| other(CopyObject, answer);
    assert_eq!(
        classify(copy, &ex(refusal(501, "NotImplemented"))),
        RowResult::Pass
    );
    for wrong in [
        refusal(501, "InvalidRequest"),
        refusal(400, "NotImplemented"),
        refusal(500, "InternalError"),
        refusal(503, "SlowDown"),
        body(503, cutn(0)),
        just(501),
        ans(resp(501).with_error_code(ErrorCode::NoBody)),
        ans(resp(501).with_error_code(ErrorCode::MissingInXml)),
    ] {
        assert_eq!(
            class_of(&classify(copy, &ex(wrong.clone()))),
            Some(Protocol),
            "{wrong:?}"
        );
    }
    assert_eq!(
        classify(copy, &ex(refusal(500, "InternalError"))),
        fatal(Protocol, "answered 500; declared 501 NotImplemented")
    );
    fails(ex(refusal(503, "SlowDown")), Protocol);
}

#[test]
fn head_bucket_passes_on_a_400_with_no_body_only() {
    let hb = row(HeadBucket);
    let ex = |answer| other(HeadBucket, answer);
    assert_eq!(classify(hb, &hb400(Body::Empty)), RowResult::Pass);
    assert_eq!(classify(hb, &hb400(bytes(b""))), RowResult::Pass);
    assert_eq!(
        classify(hb, &ex(refusal(400, "InvalidRequest"))),
        fatal(
            Protocol,
            "answered 400 with code InvalidRequest; declared 400 no body"
        )
    );
    let no_body_404 = ex(ans(resp(404).with_error_code(ErrorCode::NoBody)));
    assert_eq!(class_of(&classify(hb, &no_body_404)), Some(Protocol));
    // The declared 400 with a body, whole or cut off after a byte, is protocol whatever code
    // the client read, in both arrival forms; a cut after no byte settles nothing.
    for b in [bytes(b"x"), cutb(b"x"), cutn(1), bad()] {
        assert_eq!(class_of(&classify(hb, &hb400(b.clone()))), Some(Protocol));
        fails(hb400(b), Protocol);
    }
    assert_eq!(
        classify(hb, &hb400(bytes(b"xy"))),
        fatal(Protocol, "answered 400 with a body of 2 bytes")
    );
    for b in [cutb(b""), cutn(0)] {
        assert!(is_unsettled(&classify(hb, &hb400(b.clone()))), "{b:?}");
        unproved(hb400(b));
    }
    // The same with no code read at all (cut off before any).
    fails(ex(body(400, cutn(3))), Protocol);
    unproved(ex(body(400, cutn(0))));
}

#[test]
fn any_2xx_to_an_unsupported_row_is_unexpected_support() {
    for op in [CopyObject, UploadPart, CreateBucket, HeadBucket] {
        // A 2xx head whose body then failed or was unreadable is still support.
        for answer in [
            just(200),
            just(204),
            just(206),
            body(200, cutn(0)),
            body(200, cutb(b"<xml")),
            body(200, bad()),
        ] {
            let result = classify(row(op), &other(op, answer.clone()));
            assert_eq!(
                class_of(&result),
                Some(UnexpectedSupport),
                "{op:?}: {answer:?}"
            );
        }
    }
    fails(other(CompleteMultipartUpload, just(200)), UnexpectedSupport);
    fails(other(CopyObject, body(200, cutn(3))), UnexpectedSupport);
}

#[test]
fn an_unsupported_row_with_no_response_head_has_no_verdict() {
    let result = classify(row(CopyObject), &other(CopyObject, none()));
    assert!(matches!(result, RowResult::NoAnswer { .. }), "{result:?}");
    no_answer(other(CopyObject, none()));
}

#[test]
fn unsupported_row_with_its_error_body_cut_off_is_not_proved() {
    // Cut off before its code was read, after some or no bytes; a wrong status cut off the
    // same way is protocol anyway.
    for n in [7, 0] {
        unproved(other(CopyObject, body(501, cutn(n))));
    }
    fails(other(CopyObject, body(400, cutn(7))), Protocol);
}

#[test]
fn unsupported_row_with_an_unreadable_error_body_is_protocol() {
    // Arrived whole and not an <Error> document: an answer, not a cut. So is another document.
    assert_eq!(
        classify(row(CopyObject), &other(CopyObject, body(501, bad()))),
        fatal(
            Protocol,
            "501 error body is not an <Error> document: not an XML document"
        )
    );
    fails(other(CopyObject, body(501, bad())), Protocol);
    let Answer::Response(doc) = refusal(501, "NotImplemented") else {
        unreachable!()
    };
    fails(
        other(CopyObject, ans(doc.with_body(Body::Listing(listing())))),
        Protocol,
    );
}

// --- (B2) Required rows ---------------------------------------------------------------------

#[test]
fn strong_entity_tag_grammar() {
    for ok in ["\"abc\"", "\"\"", "\"a-b_c.1!#~\"", ETAG, OTHER] {
        assert!(is_strong_etag(ok), "{ok}");
    }
    // `*`, unquoted, weak, empty, unbalanced or inner quotes, a space outside or inside.
    let bad = r#"*;abc;W/"abc";;";"a"b";"abc;abc"; "abc";"a b""#;
    for bad in bad.split(';') {
        assert!(!is_strong_etag(bad), "{bad:?}");
    }
}

#[test]
fn put_passes_only_on_200_with_a_strong_etag_after_the_body_ended() {
    assert_eq!(classify(row(PutObject), &good_put()), RowResult::Pass);
    for etag in ["*", "abc", "W/\"abc\"", ""] {
        fails(put(ans(resp(200).with_etag(etag)), true), Protocol);
    }
    fails(put(just(200), true), Protocol);
    // A success before the request body ended is never a receipt.
    fails(put(ans(resp(200).with_etag(ETAG)), false), Protocol);
}

#[test]
fn a_5xx_to_a_required_row_is_protocol() {
    // An answer, and not the defined one: fatal on every row, never budgeted.
    for exercise in [
        put(just(503), true),
        get(just(500)),
        ranged(just(502)),
        hit(just(500)),
        miss(just(503)),
        head_of(resp(500)),
        del(just(500), just(404)),
        bulk(just(503), hs(404, 404)),
        list_v2(just(500)),
        list_v1(just(502)),
    ] {
        fails(exercise, Protocol);
    }
    // A 501 is wrong too.
    assert_eq!(
        classify(row(PutObject), &put(just(503), true)),
        fatal(Protocol, "answered 503; expected 200")
    );
    fails(put(refusal(501, "NotImplemented"), true), Protocol);
}

#[test]
fn a_404_to_a_read_of_a_written_key_is_oracle() {
    for exercise in [
        head(just(404)),
        head(refusal(404, "NoSuchKey")),
        get(just(404)),
        ranged(just(404)),
        hit(just(404)),
        miss(just(404)),
    ] {
        fails(exercise, Oracle);
    }
    assert_eq!(
        classify(row(HeadObject), &head(just(404))),
        fatal(Oracle, "404 to a key the run wrote; expected 200")
    );
    // A 404 to a write or a delete, and any other wrong status on a read, is protocol.
    fails(put(just(404), true), Protocol);
    fails(del(just(404), just(404)), Protocol);
    fails(head_of(resp(403)), Protocol);
    fails(get(body(206, whole())), Protocol);
}

#[test]
fn an_exercise_without_the_rows_check_settles_nothing() {
    // A generic exercise, or another operation's, carries nothing a required check reads.
    let generic = || other(PutObject, ans(resp(200).with_etag(ETAG)));
    unproved(generic());
    assert_eq!(
        classify(row(PutObject), &get(body(200, whole()))),
        RowResult::Unsettled {
            why: "an exercise of GetObject cannot settle the PutObject row".to_string()
        }
    );
    for r in [row(CopyObject), UNCHECKED.row(UploadPart).unwrap()] {
        assert!(is_unsettled(&classify(r, &good_put())), "{r:?}");
    }
}

#[test]
fn whole_get_with_the_wrong_bytes_is_integrity() {
    let mut off = OBJECT.to_vec();
    off[7] ^= 1;
    let mut long = OBJECT.to_vec();
    long.push(b'!');
    // One byte off, short, long, no body at all, an empty body; the report says where.
    let differs =
        |got: usize, at: usize| format!("{got} bytes differ from the expected 16 at offset {at}");
    for (b, detail) in [
        (bytes(&off), differs(16, 7)),
        (bytes(&OBJECT[..15]), differs(15, 15)),
        (bytes(&long), differs(17, 16)),
        (Body::Empty, differs(0, 0)),
        (bytes(b""), differs(0, 0)),
    ] {
        fails(get(body(200, b.clone())), Integrity);
        assert_eq!(
            classify(row(GetObject), &get(body(200, b))),
            fatal(Integrity, &detail)
        );
    }
}

#[test]
fn a_cut_off_object_body_is_judged_by_the_bytes_that_arrived() {
    // Bytes that are not a prefix of the object: integrity, cut or not; a correct prefix: unproved.
    let mut off = OBJECT[..8].to_vec();
    off[7] ^= 1;
    let mut long = OBJECT.to_vec();
    long.push(b'!');
    fails(get(body(200, cutb(&off))), Integrity);
    fails(get(body(200, cutb(&long))), Integrity);
    assert_eq!(
        classify(row(GetObject), &get(body(200, cutb(&off)))),
        fatal(
            Integrity,
            "8 bytes before the cut differ from the expected 16 at offset 7"
        )
    );
    for prefix in [&OBJECT[..8], OBJECT, b""] {
        unproved(get(body(200, cutb(prefix))));
    }
    // A count-only cut: more bytes than the object is still integrity.
    fails(get(body(200, cutn(17))), Integrity);
    unproved(get(body(200, cutn(16))));
    unproved(get(body(200, cutn(0))));
    // The same for the range and the non-matching GET.
    fails(r206_ok(cutb(&OBJECT[3..=5])), Integrity);
    unproved(r206_ok(cutb(&OBJECT[2..=3])));
    fails(miss(body(200, cutb(&off))), Integrity);
    unproved(miss(body(200, cutb(&OBJECT[..8]))));
}

#[test]
fn ranged_get_passes_only_on_206_with_the_right_content_range_and_bytes() {
    fails(ranged(body(200, whole())), Protocol);
    fails(r206_ok(bytes(&OBJECT[3..=6])), Integrity);
    // Wrong range, size or shape; signs (`u64::from_str` takes a `+`; AGENTS.md grammar rule);
    // a unit that is not `bytes` (RFC 9110 §14.4); none at all.
    let wrong = "bytes 2-6/16;bytes 2-5/17;bytes 0-3/16;bytes */16;bytes 2-5;\
                 bytes +2-+5/+16;bytes +2-5/16;bytes 2-+5/16;bytes 2-5/+16;items 2-5/16;2-5/16";
    for cr in wrong.split(';') {
        fails(r206(cr, part()), Protocol);
    }
    fails(ranged(body(206, part())), Protocol);
    // The unit is case-insensitive (RFC 9110 §14.1).
    assert_eq!(
        classify(row(GetObjectRanged), &r206("Bytes 2-5/16", part())),
        RowResult::Pass
    );
    // At the edges; an impossible range settles nothing (and cannot panic).
    for (first, last) in [(3, 3), (15, 15), (0, 15)] {
        let answer = ans(resp(206)
            .with_content_range(&format!("bytes {first}-{last}/16"))
            .with_body(bytes(&OBJECT[first as usize..=last as usize])));
        let result = classify(row(GetObjectRanged), &ranged_at(first, last, answer));
        assert_eq!(result, RowResult::Pass, "bytes={first}-{last}");
    }
    for (first, last) in [(5, 4), (2, 16)] {
        let answer = ans(resp(206).with_content_range(&format!("bytes {first}-{last}/16")));
        assert!(is_unsettled(&classify(
            row(GetObjectRanged),
            &ranged_at(first, last, answer)
        )));
    }
}

#[test]
fn conditional_get_with_the_wrong_status_or_a_body_on_its_304_is_protocol() {
    fails(hit(body(200, whole())), Protocol);
    fails(hit(body(304, bytes(b"x"))), Protocol);
    // Cut off after one body byte: the byte is the answer, not the cut.
    fails(hit(body(304, cutb(b"x"))), Protocol);
    fails(hit(body(304, cutn(1))), Protocol);
    fails(miss(just(304)), Protocol);
}

#[test]
fn conditional_get_settles_nothing_without_the_tag_its_form_needs() {
    // The negative control: an exercise that did not send the tag its form needs cannot pass
    // its row, whatever came back, so a mapping slip reads as INCONCLUSIVE, never PASS.
    for sent in [ETAG, "*", "abc", "W/\"abc\"", ""] {
        for answer in [body(200, whole()), just(304), just(500)] {
            let ex = miss_sending(sent, answer);
            unproved(ex);
        }
    }
    for sent in [OTHER, "*", "abc", "W/\"abc\"", ""] {
        for answer in [just(304), body(200, whole()), just(500)] {
            let ex = hit_sending(sent, answer);
            unproved(ex);
        }
    }
    // The reason names what was sent.
    let why = |result: RowResult| match result {
        RowResult::Unsettled { why } => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        why(classify(
            row(GetObjectIfNoneMatchMiss),
            &miss_sending("*", body(200, whole()))
        )),
        format!("sent If-None-Match: *; the row needs a strong tag other than {ETAG}")
    );
    assert_eq!(
        why(classify(
            row(GetObjectIfNoneMatchHit),
            &hit_sending(OTHER, just(304))
        )),
        format!("sent If-None-Match: {OTHER}; the row needs the object's tag {ETAG}")
    );
    // With the right tag sent, the row is judged.
    assert_eq!(
        classify(row(GetObjectIfNoneMatchHit), &hit(just(304))),
        RowResult::Pass
    );
    assert_eq!(
        classify(row(GetObjectIfNoneMatchMiss), &miss(body(200, whole()))),
        RowResult::Pass
    );
}

#[test]
fn head_with_the_wrong_size_or_etag_is_oracle() {
    let h = |len, etag| head(ans(resp(200).with_content_length(len).with_etag(etag)));
    fails(h(15, ETAG), Oracle);
    fails(h(16, OTHER), Oracle);
    fails(head(ans(resp(200).with_etag(ETAG))), Oracle);
}

#[test]
fn listing_that_is_not_exactly_the_written_keys_is_oracle() {
    let mut missing = listing();
    missing.pop();
    let mut extra = listing();
    extra.push(listed("p/c", 1));
    let mut wrong_size = listing();
    wrong_size[1].size = 4;
    let mut twice = listing();
    twice.push(listed("p/a", 16));
    for listed in [missing, extra, wrong_size, twice] {
        fails(list_v2(list_of(listed.clone())), Oracle);
        fails(list_v1(list_of(listed)), Oracle);
    }
    // Nothing written: proves nothing; a key listed there is still a disagreement.
    let list = |listed| Exercise::ListObjects {
        expected: vec![],
        answer: list_of(listed),
    };
    unproved(list(vec![]));
    fails(list(listing()), Oracle);
}

#[test]
fn a_200_without_the_document_the_check_reads_is_protocol() {
    // No document, raw bytes, the other document, or a body that is no document at all.
    let other_doc = || body(200, Body::DeleteResult(DeleteResult::default()));
    for exercise in [
        list_v2(just(200)),
        list_v2(body(200, bytes(b"<xml/>"))),
        list_v2(body(200, bad())),
        list_v1(other_doc()),
        list_v1(body(200, bad())),
        bulk(just(200), hs(404, 404)),
        bulk(body(200, bytes(b"<xml/>")), hs(404, 404)),
        bulk(list_of(listing()), hs(404, 404)),
        bulk(body(200, bad()), hs(404, 404)),
    ] {
        fails(exercise, Protocol);
    }
    assert_eq!(
        classify(row(ListObjectsV2), &list_v2(body(200, bad()))),
        fatal(Protocol, "200 body is not a listing: not an XML document")
    );
    // A GET answered a document or an unreadable body: the wrong shape, not a byte mismatch
    // (for an empty object no byte comparison could have caught it).
    let empty = Exercise::GetObject {
        object: vec![],
        answer: list_of(listing()),
    };
    assert_eq!(
        classify(row(GetObject), &empty),
        fatal(Protocol, "a document where object bytes were expected")
    );
    fails(get(list_of(listing())), Protocol);
    fails(get(body(200, bad())), Protocol);
    fails(miss(other_doc()), Protocol);
    fails(r206_ok(Body::Listing(listing())), Protocol);
}

#[test]
fn a_delete_follow_up_is_judged_on_its_status() {
    // A readable key is oracle; any status other than 404, a 5xx included, is protocol.
    fails(del(just(204), just(200)), Oracle);
    for status in [403, 503, 500] {
        fails(del(just(204), just(status)), Protocol);
        fails(bulk(all(), hs(404, status)), Protocol);
    }
    assert_eq!(
        classify(row(DeleteObject), &del(just(204), just(503))),
        fatal(Protocol, "HEAD of the key after its delete answered 503")
    );
    // No answer settles nothing; a fatal probe outranks an unsettled one.
    unproved(del(just(204), none()));
    unproved(bulk(all(), heads(just(404), none())));
    fails(bulk(all(), heads(none(), just(200))), Oracle);
}

#[test]
fn delete_objects_document_must_name_every_key_once_with_no_error_row() {
    let error = DeleteFailure {
        key: "p/b".to_string(),
        code: "InternalError".to_string(),
    };
    fails(bulk(result(&["p/a"], vec![error]), hs(404, 404)), Protocol);
    // <Deleted> names a key twice, or one of the two keys only.
    for deleted in [["p/a", "p/a", "p/b"].as_slice(), &["p/a", "p/a"], &["p/a"]] {
        fails(bulk(result(deleted, vec![]), hs(404, 404)), Protocol);
    }
    // No key requested: an empty <DeleteResult> would pass vacuously.
    let empty = || Exercise::DeleteObjects {
        keys: vec![],
        answer: result(&[], vec![]),
        follow_ups: vec![],
    };
    unproved(empty());
    // Every key under <Deleted>, and a HEAD that still reads one; or no HEAD at all.
    fails(bulk(all(), hs(404, 200)), Oracle);
    unproved(bulk(all(), vec![]));
}

#[test]
fn required_row_with_a_cut_off_document_is_not_proved() {
    for n in [0, 40] {
        unproved(list_v2(body(200, cutn(n))));
        unproved(list_v1(body(200, cutn(n))));
        unproved(bulk(body(200, cutn(n)), hs(404, 404)));
    }
}

#[test]
fn a_bodiless_answer_is_protocol_on_any_byte() {
    // No-body answers (a PUT's 200, a HEAD, a 304, a 204, the follow-up 404, HeadBucket's 400):
    // cut off after no byte settles nothing; a byte, before a cut or not, is a body.
    let cut_etag = |etag| ans(resp(200).with_etag(etag).with_body(cutn(0)));
    let put_b = |b| put(ans(resp(200).with_etag(ETAG).with_body(b)), true);
    unproved(put_b(cutn(0)));
    unproved(head_of(resp(200).with_body(cutn(0))));
    unproved(hit(body(304, cutn(0))));
    unproved(del(body(204, cutn(0)), just(404)));
    unproved(del(just(204), body(404, cutn(0))));
    unproved(hb400(cutn(0)));
    fails(put_b(cutn(1)), Protocol);
    fails(put_b(bytes(b"x")), Protocol);
    fails(head_of(resp(200).with_body(cutb(b"x"))), Protocol);
    fails(head_of(resp(200).with_body(bytes(b"x"))), Protocol);
    fails(hit(body(304, cutb(b"x"))), Protocol);
    fails(hit(body(304, Body::Listing(listing()))), Protocol);
    fails(hit(body(304, bad())), Protocol);
    fails(del(body(204, cutb(b"x")), just(404)), Protocol);
    fails(del(body(204, bytes(b"x")), just(404)), Protocol);
    fails(del(just(204), body(404, cutb(b"x"))), Protocol);
    fails(del(just(204), body(404, bytes(b"x"))), Protocol);
    fails(hb400(cutn(1)), Protocol);
    fails(hb400(cutb(b"x")), Protocol);
    fails(hb400(bytes(b"x")), Protocol);
    // The cut never hides a fatal fact the head already showed.
    fails(put(cut_etag(ETAG), false), Protocol);
    fails(put(cut_etag("*"), true), Protocol);
    fails(del(body(204, cutn(0)), just(200)), Oracle);
    // No bytes is no body, however the client spelled it.
    for exercise in [
        put_b(bytes(b"")),
        head_of(resp(200).with_body(bytes(b""))),
        hit(body(304, bytes(b""))),
        hb400(bytes(b"")),
    ] {
        let op = exercise.operation();
        assert_eq!(classify(row(op), &exercise), RowResult::Pass, "{op:?}");
    }
}

#[test]
fn every_follow_up_is_judged_against_the_key_it_asked_about() {
    // The readable key is the last: probes paired with the wrong keys would miss it.
    let abc = ["p/a", "p/b", "p/c"];
    fails(
        bulk_of(&abc, &[("p/a", 404), ("p/b", 404), ("p/c", 200)]),
        Oracle,
    );
    // A key with no HEAD of its own is not proved by the HEADs of the others.
    unproved(bulk_of(&abc, &[("p/a", 404), ("p/b", 404)]));
    // A fatal HEAD fails the row whatever another HEAD of the same key said.
    let ab = ["p/a", "p/b"];
    fails(
        bulk_of(&ab, &[("p/a", 404), ("p/a", 200), ("p/b", 404)]),
        Oracle,
    );
    fails(
        bulk_of(&ab, &[("p/a", 503), ("p/a", 404), ("p/b", 404)]),
        Protocol,
    );
    // A retry that answers 404 proves what a HEAD with no answer could not.
    let mut retried = heads(none(), just(404));
    retried.insert(1, ("p/a".to_string(), just(404)));
    assert_eq!(
        classify(row(DeleteObjects), &bulk(all(), retried)),
        RowResult::Pass
    );
    // A HEAD of a key the delete did not request settles nothing.
    unproved(bulk_of(&ab, &[("p/c", 404), ("p/c", 404)]));
    unproved(bulk_of(&ab, &[("p/a", 404), ("p/b", 404), ("p/c", 404)]));
    // A key requested twice cannot be matched to the document; its probe is still judged.
    unproved(bulk_of(&["p/a", "p/a"], &[("p/a", 404)]));
    fails(bulk_of(&["p/a", "p/a"], &[("p/a", 200)]), Oracle);
}

#[test]
fn a_lost_or_unacknowledged_delete_result_never_hides_the_probes() {
    // After a 200 a key that still reads is fatal whatever the lost document said.
    fails(bulk(body(200, cutn(40)), hs(200, 404)), Oracle);
    fails(bulk(body(200, cutn(40)), hs(404, 200)), Oracle);
    fails(bulk(body(200, cutn(40)), hs(404, 403)), Protocol);
    // Every key proved gone and the document lost: not proved, never passed.
    unproved(bulk(body(200, cutn(40)), hs(404, 404)));
    unproved(bulk(body(200, cutn(40)), heads(just(404), none())));
    // Without a 200 no delete was acknowledged, so a readable key is not oracle: no head is
    // not proved, and a 503 is protocol on the status alone.
    no_answer(bulk(none(), hs(200, 200)));
    fails(bulk(just(503), hs(200, 200)), Protocol);
}

// --- (C) Version keying ---------------------------------------------------------------------

fn release(major: u64, minor: u64, patch: u64, pre: Vec<PreId>) -> ReportedVersion {
    ReportedVersion::Release(Release {
        major,
        minor,
        patch,
        pre,
    })
}

fn rel(major: u64, minor: u64, patch: u64) -> Result<ReportedVersion, VersionError> {
    Ok(release(major, minor, patch, vec![]))
}

#[test]
fn parse_reads_releases_and_ignores_build_metadata() {
    assert_eq!(parse_version("0.1.0"), rel(0, 1, 0));
    assert_eq!(parse_version("0.1.0+git.dirty"), rel(0, 1, 0));
    assert_eq!(parse_version("0.1.0+git.3.abc1234.dirty"), rel(0, 1, 0));
    assert_eq!(parse_version("0.0.0+git.abc1234"), rel(0, 0, 0));
    let rc1 = vec![PreId::Alpha("rc".to_string()), PreId::Numeric(1)];
    assert_eq!(
        parse_version("1.2.3-rc.1+build.5"),
        Ok(release(1, 2, 3, rc1))
    );
    assert_eq!(parse_version("10.20.30"), rel(10, 20, 30));
    assert_eq!(
        parse_version("18446744073709551615.0.0"),
        rel(u64::MAX, 0, 0)
    );
}

#[test]
fn unknown_is_never_read_as_release_0_0_0() {
    for v in ["unknown", "0.0.0+git.unknown", "0.0.0+git.unknown.dirty"] {
        assert_eq!(parse_version(v), Ok(ReportedVersion::Unknown), "{v}");
        assert_eq!(select_reported(PROFILES, v), Err(NoProfile::Unknown), "{v}");
    }
    // A sha that merely contains `unknown` is a real build.
    assert_eq!(parse_version("0.0.0+git.unknownx"), rel(0, 0, 0));
}

#[test]
fn parse_refuses_malformed_versions() {
    // Space-separated, then the three that are empty or carry whitespace.
    let malformed = "wyrd/0.1.0 v0.1.0 0.1 0.1.0.0 0..0 .0.0 0.0. 01.1.0 0.01.0 +1.0.0 0.-1.0 \
                     18446744073709551616.0.0 0.1.0-rc.18446744073709551616 0.1.0- 0.1.0-rc..1 \
                     0.1.0-01 0.0.0-r_c 0.1.0+ 0.1.0+git..x 0.1.0+git_x Unknown";
    for v in malformed.split(' ').chain(["", " 0.1.0", "0.1.0 "]) {
        assert!(parse_version(v).is_err(), "{v:?} must not parse");
        let unparsable = matches!(select_reported(PROFILES, v), Err(NoProfile::Unparsable(_)));
        assert!(unparsable, "{v:?}");
    }
}

const OLD_ROWS: &[Row] = &[
    Row::unsupported(PutObject, 1, 501, NIMPL, "test"),
    Row::required(HeadObject, "test"),
];
const NEW_ROWS: &[Row] = &[
    Row::required(PutObject, "test"),
    Row::required(HeadObject, "test"),
];
const OLD: Profile = Profile::new("old", range(None, Some(Bound::new(1, 0, 0))), OLD_ROWS);
const NEW: Profile = Profile::new(
    "new",
    range(Some(Bound::new(1, 0, 0)), Some(Bound::new(2, 0, 0))),
    NEW_ROWS,
);
const TABLE: &[Profile] = &[OLD, NEW];

#[test]
fn each_version_selects_its_own_profile_and_expectations() {
    let old = select_reported(TABLE, "0.9.3+git.2.abc").expect("0.9.3 selects");
    let new = select_reported(TABLE, "1.4.0").expect("1.4.0 selects");
    assert_eq!(old.name(), "old");
    assert_eq!(new.name(), "new");
    assert_eq!(selected(TABLE, "1.0.0-rc.1"), Ok("old"));
    assert_eq!(selected(TABLE, "1.0.0"), Ok("new"));
    // The same receipt passes where PutObject is required, unexpected support where not.
    let same = good_put();
    let put_row = |p: &Profile| p.row(PutObject).unwrap();
    assert_eq!(classify(put_row(new), &same), RowResult::Pass);
    assert_eq!(
        class_of(&classify(put_row(old), &same)),
        Some(UnexpectedSupport)
    );
    let head = head_of(resp(200));
    assert_eq!(
        verdict(new, &[same.clone(), head.clone()]).outcome,
        Outcome::Pass
    );
    assert_eq!(verdict(old, &[same, head]).outcome, Outcome::Fail);
}

#[test]
fn a_version_no_profile_covers_selects_none_and_says_why() {
    let no_match = matches!(selected(TABLE, "2.0.0"), Err(NoProfile::NoMatch(_)));
    let unparsable = matches!(selected(TABLE, "two"), Err(NoProfile::Unparsable(_)));
    assert!(no_match, "2.0.0");
    assert!(unparsable, "two");
    assert_eq!(selected(TABLE, "unknown"), Err(NoProfile::Unknown));
    assert_eq!(
        select(TABLE, &ReportedVersion::Unknown),
        Err(NoProfile::Unknown)
    );
    let why = |v: &str| select_reported(TABLE, v).unwrap_err().to_string();
    assert_eq!(why("unknown"), "the server's version is unknown");
    assert_eq!(
        why("2.1.0-rc.1.x+git.4.abc"),
        "no profile covers release 2.1.0-rc.1.x"
    );
    assert_eq!(
        why("0.1"),
        "cannot read version \"0.1\": expected MAJOR.MINOR.PATCH"
    );
    // Two profiles covering one release: neither is picked.
    const OVERLAP: Profile =
        Profile::new("overlap", range(Some(Bound::new(0, 5, 0)), None), NEW_ROWS);
    const BOTH: &[Profile] = &[OLD, OVERLAP];
    let why = select_reported(BOTH, "0.6.0").unwrap_err();
    assert!(matches!(why, NoProfile::Ambiguous(_)), "{why:?}");
    assert_eq!(
        why.to_string(),
        "more than one profile covers release 0.6.0"
    );
}

// --- (D) Supported, not checked -------------------------------------------------------------

const UNCHECKED_ROWS: &[Row] = &[
    Row::required(PutObject, "test"),
    Row::not_checked(UploadPart, 750, "test"),
];
const UNCHECKED: Profile = Profile::new("unchecked", ANY, UNCHECKED_ROWS);

#[test]
fn a_not_checked_row_is_listed_not_covered_and_never_passes() {
    let v = verdict(&UNCHECKED, &[good_put()]);
    assert_eq!(v.outcome, Outcome::Pass, "{v:?}");
    assert_eq!(v.proved, vec![PutObject]);
    assert_eq!(
        (v.not_covered.len(), v.not_covered[0].operation),
        (1, UploadPart)
    );
    assert_eq!(v.not_covered[0].issue, 750);
    // An answer fed for it is never a pass, and it stays not covered.
    let row = UNCHECKED.row(UploadPart).unwrap();
    for answer in [just(200), refusal(501, "NotImplemented"), none()] {
        let result = classify(row, &other(UploadPart, answer));
        assert_eq!(result, RowResult::NotChecked { issue: 750 });
    }
    let v = verdict(&UNCHECKED, &[good_put(), other(UploadPart, just(200))]);
    assert_eq!(v.not_covered.len(), 1, "still not covered: {v:?}");
}

#[test]
#[should_panic(expected = "declares an operation twice")]
fn a_profile_cannot_declare_an_operation_twice() {
    const TWICE: &[Row] = &[Row::required(PutObject, "test"), OLD_ROWS[0]];
    let _ = Profile::new("twice", ANY, TWICE);
}

#[test]
fn a_run_that_proves_no_row_is_inconclusive() {
    // Nothing to prove: no fatal, no unproved row, and still not a PASS (0017's vacuity gate).
    const NOTHING: &[Row] = &[Row::not_checked(UploadPart, 750, "test")];
    let exercised = vec![other(UploadPart, just(200))];
    for (profile, run) in [
        (Profile::new("nothing", ANY, NOTHING), exercised),
        (Profile::new("empty", ANY, &[]), vec![]),
    ] {
        let v = verdict(&profile, &run);
        assert_eq!(v.outcome, Outcome::Inconclusive, "{v:?}");
        assert!(v.proved.is_empty() && v.fatal.is_empty() && v.unproved.is_empty());
    }
    // Every shipped profile has a row to prove.
    for p in PROFILES {
        let checked = |r: &Row| !matches!(r.expected, Expected::SupportedNotChecked { .. });
        assert!(p.rows().iter().any(checked), "{}", p.name());
    }
}

// --- (E) The run verdict --------------------------------------------------------------------

#[test]
fn any_fatal_fails_the_run_naming_every_fatal_row() {
    let mut run = passing_run();
    replace(&mut run, put(ans(resp(200).with_etag("*")), true));
    replace(&mut run, other(CopyObject, just(200)));
    // An unproved row does not soften a fatal one.
    replace(&mut run, del(just(204), none()));
    let v = verdict(&PRE_0_1, &run);
    assert_eq!(v.outcome, Outcome::Fail, "{v:?}");
    let fatal: Vec<_> = v.fatal.iter().map(|f| (f.operation, f.class)).collect();
    assert_eq!(
        fatal,
        [(PutObject, Protocol), (CopyObject, UnexpectedSupport)]
    );
    // Neither a fatal row nor the unproved one is proved.
    assert_eq!(v.proved.len(), PRE_0_1.rows().len() - 3, "{v:?}");
    assert!(
        matches!(v.unproved[..], [(DeleteObject, Unproved::Unsettled { .. })]),
        "{v:?}"
    );
}

#[test]
fn an_unexercised_undeclared_or_unanswered_row_is_inconclusive() {
    let mut run = passing_run();
    run.retain(|e| e.operation() != ListBuckets);
    let v = verdict(&PRE_0_1, &run);
    assert_eq!(v.outcome, Outcome::Inconclusive, "{v:?}");
    assert_eq!(v.unproved, vec![(ListBuckets, Unproved::NotExercised)]);
    // `unchecked` has no HeadObject row: a HeadObject answer is judged by nothing.
    let v = verdict(&UNCHECKED, &[good_put(), head_of(resp(200))]);
    assert_eq!(v.outcome, Outcome::Inconclusive, "{v:?}");
    assert_eq!(v.unproved, vec![(HeadObject, Unproved::Undeclared)]);
    // A required row exercised only with no answer; an unresolved delete names the delete row.
    no_answer(put(none(), true));
    unproved(del(just(204), none()));
    unproved(bulk(all(), heads(just(404), none())));
    // An empty run proves nothing.
    let v = verdict(&PRE_0_1, &[]);
    assert_eq!(v.outcome, Outcome::Inconclusive);
    assert_eq!(v.unproved.len(), PRE_0_1.rows().len());
    assert!(v.proved.is_empty());
}

#[test]
fn a_row_passes_only_when_every_exercise_of_it_passed() {
    let mut run = passing_run();
    run.push(put(none(), true));
    let v = verdict(&PRE_0_1, &run);
    assert_eq!(v.outcome, Outcome::Inconclusive, "{v:?}");
    assert_eq!(v.unproved[0].0, PutObject);
    assert!(!v.proved.contains(&PutObject), "{v:?}");
    assert_eq!(v.proved.len(), PRE_0_1.rows().len() - 1);
}
