# Result — issue 938 / validate-s3-client-smoke-verbs

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on `main` 955b9cb). Smoke (743.3) also needs
  HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a prefix;
  bulk DeleteObjects; and one request for each operation the matrix may declare unsupported.
  Each must report what the server actually sent (status, the headers checks read, the S3 error
  code, the body) with #853's bounded, typed reading. A 2xx head must never be lost: when a body
  fails after a 2xx head, the report still carries the 2xx, or a half-working unsupported
  operation reads as "no answer" instead of `unexpected-support`. And a probe must reach the
  endpoint it was given with the bucket name it was given: the SDK's own endpoint rules reroute
  or refuse some names (S3 Express `--x-s3` / `--xa-s3`, Outposts-alias `--op-s3`, `arn:`)
  before anything is sent, which is exactly what 743.3's bucket-safety rule cannot tolerate.
- Success criterion: the NEW test file(s) under `crates/validate/tests/` pass, against an
  in-process Wyrd gateway and, for the fault legs, a scripted loopback endpoint or relay (peers
  under *Citations*):
  - **(A) The verbs against the real gateway.** HEAD of a written key → 200 with its size and
    `ETag`; of a missing key → 404 with no body (`ErrorCode::NoBody`). Ranged GET `bytes=a-b` →
    206, the `Content-Range` header as sent, exactly those bytes. GET with `If-None-Match: <its
    ETag>` → 304, no body; with a different valid tag → 200 and the whole body. ListObjectsV2
    and v1 under a prefix → every key with its size, across more than one page (the client
    follows the continuation). DeleteObjects of N existing keys → every key under `<Deleted>`,
    no `<Error>` row; the keys are then gone (HEAD 404). Error rows are (F)'s, because the
    gateway cannot be made to fail one key on demand.
  - **(B) Every unsupported operation reaches the gateway** and is reported with status and S3
    code: `CopyObject` 501 `NotImplemented`; `CreateMultipartUpload`, `UploadPart`,
    `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads` 501;
    `GetBucketLocation` 501; `CreateBucket`, `DeleteBucket`, `ListBuckets` 400 `InvalidRequest`;
    `HeadBucket` 400 with no body. **Bucket names reach the endpoint as given**, on an IP
    endpoint (`http://127.0.0.1:{port}`) AND a host-name endpoint (`http://localhost:{port}`):
    `CreateBucket` / `DeleteBucket` naming a bucket S3's rules forbid (one ending in `-`), a
    host-safe and a non-host-safe `--x-s3` name, an `--xa-s3` name, an Outposts-alias-shaped
    `--op-s3` name and an `arn:` name each arrive as ONE request, `PUT|DELETE /<name>/` with the
    name as one percent-encoded path segment after the endpoint's own path, `Host:` the
    configured endpoint, signed for `s3` in the configured region. `ListBuckets` addresses the
    endpoint itself. A name that cannot be sent at all is reported `RequestNotBuilt` (nothing
    sent), never `NoResponse`; a request that reached the connector and got no answer stays
    `NoResponse`. The remaining unsendable names are named in the code's docs and tested.
  - **(C) A 2xx head survives its body.** A scripted endpoint answers a request that carries a
    body with a `200` head and then (i) stalls the body past the body-idle deadline, (ii)
    trickles it past the whole-body deadline, (iii) cuts the connection mid-body, (iv) sends a
    body that is not the operation's success document. Short deadlines via
    `S3Client::with_deadlines`. In each case the report carries status 200 and the body failure
    beside it, never only a timeout or a transport error. A `200` whose body is an S3 `<Error>`
    document (S3 does this for `CopyObject` / `CompleteMultipartUpload`) keeps the 200 AND
    reports that document's `<Code>` and `<Message>` as typed fields.
  - **(D) Unreadable documents are refused for the new verbs as #853 refuses them for the old.**
    A listing page or `<DeleteResult>` that is cut short, followed by junk or a second document,
    followed by bytes past its `Content-Length`, or past `BUFFERED_BODY_BUDGET` (with a
    `Content-Length` and chunked) is a typed refusal that keeps the 200, never a partial listing
    or a short delete result. A later page that fails discards the earlier good pages (no
    partial listing). A document of exactly the budget is read. A `<Size>` that is not plain
    decimal digits (`+7`, a space, a fraction, past `u64`) is refused.
  - **(E)** `s3_client_roundtrip.rs`, `s3_client_upload_peers.rs` and
    `s3_client_nonconforming_responses.rs` pass unmodified (the existing verbs keep their
    behaviour).
  - **(F) A mixed `<DeleteResult>` is reported row by row.** The gateway answers `200` even
    when some keys failed (`crates/gateway-s3/src/lib.rs:2293-2296`), so error rows are the only
    evidence of a failed key. A scripted endpoint answers `POST ?delete` with `200` and two
    `<Deleted>` rows plus one `<Error>` row (`<Key>`, `<Code>AccessDenied</Code>`,
    `<Message>`). The result lists exactly the two deleted keys and exactly one error row with
    that key, code and message as sent, in document order.
  - **(G) The success and continuation paths are pinned, not only the failures.** Against the
    scripted endpoint: a `200` with each operation's whole, correct success document (or no
    body where its success has none) is a success; `ListObjects` (v1) follows a `<NextMarker>`
    that differs from the last key, and the last key where a truncated page names none; a
    truncated page that names nothing to continue from is refused; a listing still truncated
    after the page cap stops there with a typed result; a header sent twice comes back as both
    lines.
  - **(H) The living architecture doc matches the code.**
    `docs/design/architecture/05-building-block-view.md:299` on `main` (the `validate`
    paragraph; `:301` on the integration line, where #932 adds two lines at `:243`) describes the new verbs, the 2xx head kept beside a body failure, listing continuation and
    its page cap, bulk-delete rows, the buffered budget's reach (listings, delete results,
    probes, non-2xx ranged/conditional GETs), and the client's own endpoint resolution with its
    named exceptions. No sentence in it or in the code's docs claims more than the tests show.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the new verbs and their typed results in `crates/validate/src/s3.rs` and
  `crates/validate/src/s3/*`; the client's endpoint resolution (so a bucket name reaches the
  configured endpoint as given); `wyrd-traits` (or what the bucket-seeding peer needs) under
  `[dev-dependencies]` in `crates/validate/Cargo.toml`; the new test file(s); the `validate`
  paragraph of `docs/design/architecture/05-building-block-view.md`. / out of scope:
  - judging answers (743.1); the runner, ledger, version source and arguments (743.3);
    `lib.rs`;
  - the multipart success path (#508, #750); changing the existing verbs' behaviour beyond
    what the shared endpoint resolution implies (an endpoint path joined with one `/`; the
    region signed as given), each of which must be named in the docs;
  - any `wyrd-*` crate in `[dependencies]`; any new production dependency; `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
