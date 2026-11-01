# Adversarial review — issue 955 (validate-s3-client-reads-and-listings)

I built the patched tree in a scratch copy and ran probe tests and temporary edits against the
production `S3Client`. I found one concrete failing case. The rest of the evidence held up.

## Findings

- NEEDS-HUMAN [impl] — **A non-2xx answer to the new GET verbs that carries an `x-amz-checksum-*` header loses its code, or turns into a body failure.** `crates/validate/src/s3/verbs.rs:50-62` (`get_object_range`) and `:72-84` (`get_object_if_none_match`) go through the SDK's `GetObject` operation. Its `ResponseChecksumInterceptor` comes from the operation plugin, which is applied before the `.customize()` override plugin (`aws-sdk-s3` 1.155.0 `src/operation/get_object.rs:72-82`). So its `modify_before_deserialization` wraps the body in a checksum validator (`src/http_response_checksum.rs:130-139`) *before* `Capture::modify_before_deserialization` takes it (`crates/validate/src/s3/answer.rs:473-482`). `read_whole` then reads the SDK's validator, not the body as sent. That contradicts the module doc's claim (`answer.rs:17-18`) that the SDK's handling is "never consulted". Under the default `WhenSupported`, the SDK also adds `x-amz-checksum-mode: ENABLED` to the request (`get_object.rs:182-185`), which the caller never asked for. **Reproduced** with a probe test against the patched client and a scripted loopback endpoint:
  - `304 Not Modified` + `ETag` + `x-amz-checksum-crc32: NhCmhg==`, no body, to `get_object_if_none_match` → `Fetched::Other`, outcome `BodyFailed(Transport { received: 0, detail: "body checksum mismatch. expected body checksum to be 3610a686 but it was 00000000" })`. Expected `Error { code: NoBody }`.
  - `404` + `x-amz-checksum-crc32` + body `<Error><Code>NoSuchKey</Code><Message>gone</Message></Error>` to `get_object_range` → `BodyFailed(Transport { received: 60, detail: "body checksum mismatch…" })`. Expected `Error { code: Code("NoSuchKey"), message: Some("gone") }`. That is criterion (H)'s property ("keeps its status, headers and code") failing for a header the tests never send (`crates/validate/tests/s3_client_smoke_verbs.rs:1517-1599`).
  - The request head the probe endpoint captured contains `x-amz-checksum-mode: ENABLED`, and that header is in the SigV4 `SignedHeaders` list.

  The Wyrd gateway sends no response checksum headers, so (A) and (H) against it can't see this. The client targets any `--endpoint`, though. I don't know whether AWS S3 itself repeats checksum headers on a 304. A likely fix: give these two verbs a per-operation `response_checksum_validation(ResponseChecksumValidation::WhenRequired)` through `.customize().config_override(…)`. That stops both the wrapper and the extra request header. Then add a scripted 304 and 403 case that carry a checksum header. The base `get_object` (`crates/validate/src/s3.rs:300-325`) behaves the same way, but changing it is out of this brief's scope, so it needs its own issue.

- NEEDS-HUMAN [impl] — **No test sends a 2xx listing page with an empty body, so the "an empty body names no error" guard is unguarded.** `crates/validate/src/s3/answer.rs:349` (`Ok((code, message)) if !body.is_empty()`) is the only thing that stops a bodiless `200` listing page from being reported as `Outcome::Error { code: NoBody }`, an S3 error the server never sent (this goes against the doc at `answer.rs:148`). The C5 log shows the mutant `replace match guard !body.is_empty() with true in settle` **MISSED**. My probe shows the current code is correct (`200` + `Content-Length: 0` to `list_objects_v2` → `Unreadable { body: b"", detail: "the listing body is not a well-formed XML document: …" }`), so only the test is missing. Fix: add an `"empty"` case (`ok_with_length(id, 0, b"")`, `Refused::Unreadable`) to `listing_pages_that_are_not_whole_documents_are_refused_never_partial` (`s3_client_smoke_verbs.rs:1027-1062`).

- **C5 "fail" (5 missed) is mostly equivalent mutants. Not a defect, recorded so nobody chases them.** The two `SuccessBody::read` survivors (`answer.rs:326`, `:327`) can't be reached through the client. hyper never gives a HEAD response a body, and the connector reports a HEAD that sends one as an overrun. My probe: `200` + `Content-Length: 5` + `hello` to `head_object` → `BodyFailed(Overrun { received: 0, after: 5 })`. `fetched()` sends only non-2xx answers through `SuccessBody::Empty` (`verbs.rs:231-232`), where `read` is never called. So the non-empty branch at `answer.rs:328-331` is dead code. The two `Capture::name` survivors (`answer.rs:449`) are trivially equivalent. Of the five, only the `settle` guard above is a real gap.

- NEEDS-HUMAN [human] — **The T4 batch-review row's evidence is too thin to check.** `check-gates.json` row `T4-batch-review` claims a "3x codex, union, triaged" review of this ~4,400-line diff that finished in 29.97 s with "0 blocking, 0 recorded-rejected, 0 noise-dropped" (`gate-logs/T4-batch-review.log`). Three independent review passes over a diff this size, with not even one noise-dropped item, in half a minute, suggests a cached or empty review. `review-batch.md` isn't in my inputs, so I can't tell. Someone should confirm the review ran on this bundle before it counts toward "one deep, multi-pass review".

## What I tried to refute and could not

- **The red→green evidence.** C4-verify is UNVERIFIABLE, as the brief declared up front (the tests call API the patch adds). I checked two of the claimed reds myself with temporary edits to the production `verbs.rs` in a scratch copy:
  - Red 1: map a 2xx body failure to `NoAnswer(NoResponse)` in `page()`. The (C) test failed on exactly Stall, Trickle and Cut ("the 200 head was lost").
  - Red 4: return after the first listing page. `both_listings_…`, `a_listing_whose_later_page_…` and `…max_listing_pages…` failed.

  The tests drive the production client through the real gateway and real loopback sockets. They don't use a re-implementation, and they fail for the stated reason. The 64 caught mutants agree.
- **(E).** `s3_client_roundtrip` (15), `s3_client_nonconforming_responses` (28) and `s3_client_upload_peers` (12) pass in `gate-logs/C4-ci.log:3261-3353`, and the patch doesn't touch them.
- **The fixes I attacked and couldn't break:**
  - Listing continuation: a repeated or never-ending token stops at `MAX_LISTING_PAGES`; an empty `<NextMarker>` falls back to the last key; a truncated V1 page with no keys and no marker is refused.
  - A body with no declared length: XML ends itself, so a cut page still fails to parse.
  - The budget boundary (exactly 64 KiB is read; one byte more is refused).
  - Deadlines: a deadline tie goes to the whole-body deadline; a huge limit falls back to the idle deadline only.
  - A request refused before it is sent vs. one sent and unanswered (`RequestNotBuilt` vs `NoResponse`).
  - Non-UTF-8 and repeated header lines.
  - Clock: `read_whole` names tokio's runtime clock as the one source for its lifecycle, as the rubric requires (`answer.rs:513-515`).
- **Not raised:** the architecture doc. It is assigned to #956 in the brief, so under the rubric's deferral rule that point is settled.

<!-- pdca:leaf-complete -->
