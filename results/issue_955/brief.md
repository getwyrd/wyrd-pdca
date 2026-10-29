# Brief — issue 955 / validate-s3-client-reads-and-listings

> Do reads ONLY this file. First of two children of #938 (itself 743.2, child of #743). It
> builds the reply-capture core the S3 client needs beyond PUT/GET/DELETE and the verbs that
> only read: HEAD, ranged GET, conditional GET, ListObjectsV2 and ListObjects. It decides
> nothing about whether an answer is right (743.1 judges, 743.3 runs). #956 (bulk delete,
> probes, endpoint resolution) builds on this one.
>
> **Salvage is the starting point, not a hint.** `results/issue_938/iteration-v3/patch.diff`
> was built three times, reviewed adversarially and found without defect; it was split for
> size. Take this child's hunks from it as they are; do not re-derive the design. The human's
> sign-off of that iteration: "The reviewer found no defect in the patch, and the code is
> worth keeping as the starting point for the children"
> (`results/issue_938/iteration-v3/session-carry-forward`).
>
> **Where the salvage is (revised 2026-10-23).** On the harness host, at the absolute path
> `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/patch.diff` (4,461 lines, sha256
> `b50cc4f5a2bc116fd8e64938a7dc903692148f4ddecbb5f96bcd00df0f8d7d69`) with its rationale
> `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/build-notes.md` (sha256
> `f2c4fcf95040204e7e3eb3542c17a5bd4c515063df8e7e5d23ba8cf16bb632f7`). The harness root is the
> Do leaf's working directory. Both files are untracked in the harness repo today, so a
> reviewer's sandbox and the target checkout do not have them. Check both hashes first. If a
> file is missing or its hash differs, stop: say so in `build-notes.md` as an item for the
> human and ship no patch. Do not re-derive the design from this brief.

- **Slug:** validate-s3-client-reads-and-listings
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on `main` 955b9cb). Smoke (743.3) also
  needs HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a
  prefix, followed across pages. Each must report what the server actually sent (status, every
  header field line as sent; for an S3 error its `<Code>` and `<Message>`, or which of the
  base's `ErrorCode` cases says why there is none; otherwise the body as sent) with #853's
  bounded, typed reading.
  A 2xx head must never be lost: when a body fails after a 2xx head, the report still carries
  the 2xx. The SDK deserializes these answers itself, so today a listing cut short, a body that
  stalls after a 200, or a 304 with no body cannot be reported as what the server sent.
- **Success criterion:** the NEW test file `crates/validate/tests/s3_client_smoke_verbs.rs`
  passes, against an in-process Wyrd gateway and, for the fault legs, a scripted loopback
  endpoint (peers under *Citations*):
  - **(A) The read verbs against the real gateway.** HEAD of a written key → 200 with its size
    and `ETag`; of a missing key → 404 with no body (`ErrorCode::NoBody`). Ranged GET
    `bytes=a-b` → 206, the `Content-Range` header as sent, exactly those bytes. GET with
    `If-None-Match: <its ETag>` → 304, no body; with a different valid tag → 200 and the whole
    body. ListObjectsV2 and v1 under a prefix → every key with its size, across more than one
    page (the client follows the continuation).
  - **(C) A 2xx head survives its body, shown on a listing page.** A scripted endpoint answers
    a ListObjectsV2 request with a `200` head and then (i) stalls the body past the body-idle
    deadline, (ii) trickles it past the whole-body deadline, (iii) cuts the connection
    mid-body, (iv) sends a body that is not a `ListBucketResult`. Short deadlines via
    `S3Client::with_deadlines`. In each case the result carries status 200 and the body
    failure beside it, never only a timeout or a transport error.
  - **(D) Unreadable listing pages are refused as #853 refuses an error document.** A page
    that is cut short, followed by junk or a second document, followed by bytes past its
    `Content-Length`, or past `BUFFERED_BODY_BUDGET` (with a `Content-Length` and chunked) is a
    typed refusal that keeps the 200, never a partial listing. A later page that fails discards
    the earlier good pages (no partial listing). A page of exactly the budget is read. A
    `<Size>` that is not plain decimal digits (`+7`, a space, a fraction, past `u64`) is refused.
  - **(E)** `s3_client_roundtrip.rs`, `s3_client_upload_peers.rs` and
    `s3_client_nonconforming_responses.rs` pass unmodified.
  - **(G) Listing continuation and headers are pinned, not only the failures.** Against the
    scripted endpoint: `ListObjects` (v1) follows a `<NextMarker>` that differs from the last
    key, and the last key where a truncated page names none; a truncated page that names
    nothing to continue from is refused; a listing still truncated after `MAX_LISTING_PAGES`
    stops there with a typed result; a header sent twice comes back as both lines, a header
    that is not text included; a 304 to a conditional GET that frames a chunked body is not
    refused (a GET's or HEAD's 304 may carry framing headers with no body).
  - **(H) An S3 error through every new verb keeps its status, headers and code** (added
    2026-10-23; the existing nonconforming suite covers only GET, PUT and DELETE,
    `s3_client_nonconforming_responses.rs:634-641`). Against the real gateway:
    `list_objects_v2` and `list_objects_v1` of a bucket with no record →
    `ListingError::Page { page: 1, cause: NoResult::Answered(_) }`, status 404,
    `Outcome::Error` with `ErrorCode::Code("NoSuchBucket")`
    (`crates/gateway-s3/src/lib.rs:935-944`); `get_object_range` and
    `get_object_if_none_match` of a missing key → `Fetched::Other`, 404,
    `ErrorCode::Code("NoSuchKey")` (`:2834-2841`). Against the scripted endpoint, one `403`
    carrying an `x-amz-request-id` line, a header sent twice, and the body
    `<Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>` answers each of
    `get_object_range`, `get_object_if_none_match`, `list_objects_v2` and `list_objects_v1`:
    each reports status 403, every header line as sent (the repeated one as both lines),
    `ErrorCode::Code("AccessDenied")` and the message `Access Denied`. The same `403` with a
    well-formed `<Error>` that has no `<Code>` → `ErrorCode::MissingInXml`; with a body that
    is not one whole `<Error>` document (cut short; another root) → `Outcome::Unreadable`
    carrying the body as sent, never an `Error` with a code filled in; `head_object` answered
    `403` with no body → `ErrorCode::NoBody`. `ErrorCode` and its three cases are the base's
    (`crates/validate/src/s3/error.rs:64-76`); `Outcome`, `Fetched`, `NoResult` and
    `ListingError` are the salvage's.
- **Falsifiability:** RED is reachable on `cargo test -p wyrd-validate --test
  s3_client_smoke_verbs`, which `cargo xtask ci` also runs: a real gateway on loopback for
  (A) and (H), a scripted endpoint with short deadlines for (C)/(D)/(G)/(H). A client that
  drops the 2xx head on a body failure, stops listing after one page, accepts a cut-off page,
  or reports a new verb's error without its code fails an assertion. No Docker, no network
  beyond `127.0.0.1`. **Inherited evidence covers the salvage's tests only:** every test taken
  from the salvage was shown red and green on iteration 3's code
  (`results/issue_938/iteration-v3/build-notes.md` "Red → green"). The three tests new here
  — (C) on a listing page, (G)'s conditional-GET 304, and (H) — have no salvage evidence;
  their reds are shown in this cycle (reds 1 and 15 below, and (G)'s 304 by a temporary edit
  that refuses its framing).
- **Invariant to restore:** what the client reports is what the server sent, from the request
  the caller asked for. No answer is upgraded, none downgraded, nothing filled in on the
  server's behalf (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never
  erases the 2xx (proposal 0017 §7, the failure taxonomy the typed errors serve).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality; the verbs do not exist on the base.
- **Ordering note:** independent of #956 and runs first; #956 reports through this
  child's core. Base: `main` 955b9cb (#853 PR #934 and #744 PR #931 merged; `s3/response.rs`,
  `s3/wire.rs`, `roxmltree` under `[dependencies]` at `crates/validate/Cargo.toml:44`); in the
  live run the integration line `pdca-integration/main-r42c5348f203d`. Runs in parallel with
  743.1 (#937) and does not depend on it; **must not touch `crates/validate/src/lib.rs`**
  (743.1 adds a line there, 743.3 rewrites it); new types are reachable as
  `wyrd_validate::s3::…`. **Salvage hunks this child owns** (iteration-v3 `patch.diff`):
  `Cargo.lock` + `crates/validate/Cargo.toml` (`wyrd-traits` dev-dependency); `s3.rs` module
  wiring (`mod answer; mod document; mod verbs;`, the `pub use` lines for `answer`, `document`,
  `error`, the `Deadlines` doc, `object_length`) but NOT the `PathStyle` resolver, its imports
  or the two builder lines `.endpoint_resolver(PathStyle)` /
  `.disable_s3_express_session_auth(true)`; `s3/answer.rs`, `s3/body.rs`, `s3/error.rs`,
  `s3/response.rs` whole; `s3/document.rs` without `DeleteResult`/`DeleteRow`/`KeyError`/
  `delete_rows`; `s3/verbs.rs` without `EMPTY_PART_ETAG`, `Probe`, `delete_objects`, `probe`,
  `not_built`. In the test file: the gateway fixture, the scripted `Endpoint`/`Script`/`serve`,
  the listing builders and the tests under *Test file* — not the `Relay`, the probe cases or the
  delete tests. The `s3.rs` module doc describes only what this child ships; #956 extends it.
- **Scope:** the reply-capture core and the read verbs in `crates/validate/src/s3.rs` (wiring
  only) and `crates/validate/src/s3/{answer,document,response,verbs,body,error}.rs`;
  `wyrd-traits` under `[dev-dependencies]`; the new test file. / out of scope: bulk delete,
  the probes, the endpoint resolution, and the architecture doc (#956); judging answers
  (743.1); the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart
  success path (#508, #750); any change to the existing verbs' behaviour; any `wyrd-*` crate
  in `[dependencies]`; any new production dependency; `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/validate/tests/s3_client_smoke_verbs.rs` (NEW). From the salvage:
  `head_reports_a_written_keys_size_and_etag_and_a_missing_keys_bare_404`,
  `a_ranged_get_is_206_with_its_content_range_and_exactly_those_bytes`,
  `a_conditional_get_is_304_on_its_etag_and_the_whole_object_on_another`,
  `both_listings_return_every_key_under_a_prefix_with_its_size_across_pages`, the listing
  legs of `listing_and_delete_result_documents_that_are_not_whole_are_refused_never_partial`,
  `a_listing_whose_later_page_is_unreadable_reports_no_listing_at_all`,
  `a_truncated_listing_page_that_names_nothing_to_continue_from_is_refused`,
  `a_head_reports_every_header_line_a_repeated_one_and_one_that_is_not_text_included`,
  `list_objects_v1_follows_a_next_marker_and_the_last_key_where_a_page_names_none`,
  `a_listing_still_truncated_after_max_listing_pages_pages_stops_there`,
  `a_listing_size_that_is_not_plain_decimal_digits_is_refused`.
  New here (three tests the salvage does not have; by reading, its production already behaves
  so — if a test shows otherwise, fix it inside this child's hunks and record the change in
  `build-notes.md`). First, (H): one test (Do names it) driving the four non-HEAD read verbs
  through the real gateway's `NoSuchBucket` / `NoSuchKey` and the scripted `403` cases, and
  `head_object` through the bodiless `403`. Then:
  (C) on a listing page — the salvage's `BodyFault`/`fault_script` take a `Probe` only to build
  the cut document (`cut_document`, iteration-v3 patch line 3363); give them a
  `ListBucketResult` fragment instead and send a ListObjectsV2 request. And (G)'s 304 leg — the
  salvage's `a_304_that_frames_a_chunked_body_…` test drives probes only; here a `304` with
  `Transfer-Encoding: chunked` answers `get_object_if_none_match`, which captures as a GET
  (`Capture::new(true)` in the salvage's `verbs.rs`), so the framing is not refused.
- **Verification posture:** (a) net-new. The verbs do not exist on the base, so with production
  reverted the test cannot compile and C4-verify reports UNVERIFIABLE (exit 77): pre-declared.
  Built and exercised at Check: every read verb against the real gateway, every fault path
  against the scripted endpoint. **Demonstrated reds (required, in `build-notes.md`):**
  (1) return only the timeout when a body stalls after a 2xx head → (C); (2) accept a listing
  document cut short → (D); (4) stop listing after the first page → (A); (15) report every
  new verb's error response as `ErrorCode::NoBody` → (H).
- **Production reach:** consumed by #956 (every delete and probe reports through this
  core) and by 743.3 (#939) in this same run; until then only the tests call the new verbs.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, response reading:** the existing verbs' `.customize()` hooks
    (`crates/validate/src/s3.rs:244`, `:288`, `:330`) and `S3Client::classify` (`:339`); the
    strict document reader `error_document` (`crates/validate/src/s3/response.rs:363`) and the
    `ResponseGuard` interceptor (`:209`, `impl Intercept` `:231`). Read listings by the same
    rules (UTF-8, one well-formed document under the expected root, bounded depth and size).
  - **Peer, in-process gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323`. A listing needs the bucket's marker
    seeded first (ADR-0046 decision 1): do it as `seed_bucket` does,
    `crates/server/tests/s3_list_objects.rs:82-88`, before the store goes into `Gateway::new`.
  - **Peer, faults:** the scripted endpoint
    `crates/validate/tests/s3_client_nonconforming_responses.rs:354` (`serve` `:489`, `head()`
    `:247`, `error_doc()` `:220`).
  - **Deadlines:** `Deadlines` `crates/validate/src/s3.rs:81-96`, defaults `:98-107` (body idle
    60 s, whole body 15 min): use `S3Client::with_deadlines` for (C).
  - **Salvage:** `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/patch.diff` and
    its rationale `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/build-notes.md`
    (hashes and the stop rule in the header).
  - **Peer, error codes:** the base's `ErrorCode` (`crates/validate/src/s3/error.rs:64-76`)
    and the nonconforming suite's `Expect::Service` checks
    (`crates/validate/tests/s3_client_nonconforming_responses.rs:686-692`), which (H) mirrors
    for the new verbs.
- **Prior-art check (triage cycles):** searched by path `crates/validate/src/s3.rs`,
  `crates/validate/src/s3/` (2026-10-23): merged #859, #865, #934 (#853); open: #942 (stacked
  on the integration line only); closed-unmerged: none. Rejected attempts are the parent's
  own `results/issue_938/iteration-v1..v3` (no defect in v3; split for size). Of their
  findings only (G)'s listing legs touch this child; the rest are #956's.
- **Disposition hint:** new-feature

Plan-review response (2026-10-23): both findings are addressed in place. (1) Error codes: new
criterion (H) sends real `NoSuchBucket` / `NoSuchKey` answers and a scripted `403
AccessDenied` through every new read verb and asserts status, header lines, code and message,
plus the `MissingInXml`, `Unreadable` and `NoBody` cases; red 15 is the reviewer's own
mutation. The Defect now says exactly what an error report carries (code and message, not the
raw body, which is what the salvage's `Outcome::Error` keeps). (2) Salvage: the file exists on
the harness host, where Do runs, but it is untracked, so the reviewer's sandbox could not see
it. The brief now gives absolute paths and sha256 hashes, and tells Do to stop and flag the
human if either file is missing or changed, never to re-derive. The falsifiability line now
separates inherited red/green evidence (salvage tests) from the three tests new here. The
salvage instruction itself stands: the human accepted the split that set it.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
