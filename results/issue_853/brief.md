# Brief — validate-s3-client-nonconforming-responses

- **Slug:** validate-s3-client-nonconforming-responses
- **Track:** blackbox
- **Goal:** The validator reports every response a conforming S3 server would not send as what
  it is, and never buffers more of a response body than a byte budget the client fixes, not
  the endpoint. The one body it does not buffer, a successful GET, streams under #852's
  bound and is never cut short by the budget.
- **Defect:** #852's client is expected to trust the SDK's reading of a response. Each case
  below is a **hypothesis for #852 until Do reproduces it on #852's folded commit** (see
  Falsifiability). Each was observed on one of #741's three attempts, whose client #852
  rebuilds; the cause of each sits in the SDK or hyper, which #852 keeps; and #852 defers
  every one of them to this issue by name. The records are in this harness repo under
  `results/issue_741/`:
  - an empty-body 404 becomes `Code("NotFound")`, a code the SDK makes up (aws-sdk-s3
    `protocol_serde.rs:24-28`). `iteration-v1/SUMMARY.md:187-195`;
  - an HTML 500 becomes a clean S3 error. `iteration-v1/SUMMARY.md:206-207`;
  - `<Error><Code>SlowDown</Code>` with no closing tag, a `<Message>` cut off mid-text, or
    junk after `</Error>` (inside a correctly sized body) all become clean S3 errors with a
    code. `iteration-v2/SUMMARY.md:139`;
  - a `200` whose `Last-Modified` fails its grammar becomes an *error response*, with the
    SDK's diagnostic dropped. `iteration-v1/SUMMARY.md:196-208`;
  - on PUT and DELETE, an error body that disagrees with its declared `Content-Length`
    (including chunked framing that also declares a length) is trusted, while GET calls the
    same bytes unreadable, because the SDK enforces length on `GetObject` only.
    `iteration-v3/SUMMARY.md:114` (F3) and `:146-162`;
  - response bodies are collected whole before any client code runs: a lazily streamed,
    chunked 768 MiB error body raised peak memory from 19 MiB to 794 MiB.
    `iteration-v3/SUMMARY.md:110` (F1) and `:164-175`. The cause, checked in the locked
    sources: for every response the SDK does not stream (every error, and every PUT or
    DELETE response including a `2xx`), it reads the whole body before deserializing
    (`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:527-536`). Only a `GetObject`
    success streams (`aws-sdk-s3-1.148.0/src/operation/get_object.rs:337-343`);
  - a GET body framed only by connection close is accepted as a complete, shorter object.
    `iteration-v1/SUMMARY.md:209-217`. Round 3's fix also refused complete chunked GETs,
    which was wider than needed (`iteration-v2/SUMMARY.md:140`).
- **Success criterion:** BINDING, demonstrable by C4-verify. The production client runs
  against a scripted loopback endpoint that writes exact bytes. The request id in every
  expectation is the `x-amz-request-id` the endpoint sent. Each case below classifies as
  stated, asserted field by field, on **GET, PUT and DELETE** wherever the response can occur:
  - a non-success response with an empty body → S3 error with status and "no body", never a
    code the body did not carry;
  - a non-success response with well-formed XML but no `<Code>` → S3 error, "no code in
    body";
  - a non-success response with a non-XML body (an HTML page) → unreadable;
  - an `<Error>` document that is unclosed, cut off, or followed by anything but whitespace or
    comments, inside a correctly sized body → unreadable;
  - an error body whose received length differs from its declared `Content-Length`, or one
    that is chunked and also declares a length → unreadable, **with the same classification on
    all three operations**;
  - an error response whose XML is complete but whose **chunked framing is not**: (a) the
    connection closes with no terminal `0` chunk, (b) it closes after `0\r\n` with the final
    `\r\n` missing → unreadable, on all three operations. These separate a framing failure
    from an XML failure;
  - a success status the SDK cannot read → unreadable, with the SDK's diagnostic kept in the
    detail and the real status and request id;
  - **the byte budget, on a body with no declared length.** The endpoint sends a
    non-success response with `Transfer-Encoding: chunked` and **no `Content-Length`**, whose
    body is an otherwise valid `<Error>` document — `<Code>SlowDown</Code>`, then a
    `<Message>` whose text the endpoint generates lazily until it is far larger than the
    budget plus socket buffers → unreadable. The endpoint must have written **at most the
    budget plus a stated, bounded slack** before the client closed the connection. The oracle
    is bytes written at the endpoint, not process RSS. Do demonstrates the mutation: remove
    the **streaming** limit while keeping any check on a declared `Content-Length`, and the
    assertion fires. A response that DECLARES an over-budget length may be refused from the
    header alone; that is allowed, but it does not satisfy this case;
  - **the same budget on a PUT or DELETE success body** (the SDK buffers those too): a `200`
    to PUT or DELETE whose chunked body is generated lazily past the budget → unreadable,
    with the same bytes-written oracle;
  - **a valid GET object larger than the budget is NOT cut short:** a GET `200` with a
    correct `Content-Length` of at least four times the budget → **accepted**,
    byte-identical. This guards against a cap applied to every response;
  - a GET `200` with no `Content-Length` and no chunked framing (close-delimited) → body
    error;
  - a GET `200` with chunked framing and a proper terminal chunk → **accepted**,
    byte-identical;
  - a GET `200` with chunked framing that is cut short — (a) no terminal `0` chunk, (b) the
    final `\r\n` after `0\r\n` missing → body error, never a shorter object;
  - a GET whose received length disagrees with its declared length → body error.
- **Falsifiability:** a real red is available on the ordinary harness (`cargo test -p
  wyrd-validate --test s3_client_nonconforming_responses`, no Docker, no network), on one
  condition that is checked, not assumed: the test names only #852's public API and #852's
  dev-dependencies. Then, when C4-verify reverts this patch's production change, the test
  **compiles** against #852's client and fails by assertion: the genuine `PASS` red leg. Two
  rules keep it that way. Exercise the byte budget at its **default**, so no new option is
  needed. Add no dev-dependency, because the red leg reverts manifests too.
  **Establish the baseline first, before any production change**, on the base this bundle
  builds on (the run's integration branch with #852 folded in; record the commit). (1) If
  #852's client is not in the base (`ls crates/validate/src/`, no client module), STOP and
  report. (2) Write the test, build it against that commit unchanged, and confirm it
  compiles. (3) Run every case and record the per-case result in `build-notes.md`: red
  (reproduced), or green (#852 already handles it). A green case stays as a guard and is
  not counted as red. The chunked-truncation and larger-than-budget GET cases may well be
  green on #852 (hyper reports a cut-off chunked body as `UnexpectedEof` in any chunked
  state, `hyper-1.10.1/src/proto/h1/decode.rs:259-269`); that is expected.
- **Invariant to restore:** *A response that is not a whole, well-formed, correctly framed S3
  response is never reported as one: not as a clean S3 error, not as a success, not as a
  shorter object. Every response body the client buffers (error bodies on all three
  operations, and PUT or DELETE success bodies) is capped by a budget the client fixes; the
  one body it streams, a successful GET, stays under #852's streaming bound and is never cut
  short by that budget.* Sources: `AGENTS.md` "Protocol input" ("torn, truncated, or
  oversize input is indeterminate or an error — never silently accepted. Enforce declared
  `Content-Length`, the chunked terminal CRLF …"); proposal 0017 §6-7 (the tool's verdicts
  are only as good as the facts it reports). Self-test: patching one operation does not
  satisfy this; the same bytes must classify the same way on GET, PUT and DELETE.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 852
- **Conflicts with:** 854
- **Ordering note:** builds on #852's client and calls its API. `Conflicts with` #854
  because both edit `crates/validate/src/client.rs`; neither builds on the other.
- **Surfaces:** data
- **Difficulty:** medium. Confined to `crates/validate/src/` and one new test file, with no
  cross-crate reach; the density is in edge cases, which the gates own.
- **Do model:** opus
- **Scope:** classification and bounded reading of every response a conforming S3 server does
  not send, on all three operations. Plan decisions already taken, not Do's to re-open:
  - **Chunked GET bodies are accepted** when their framing is complete. Chunked framing marks
    its own end, and the rubric names the chunked terminal CRLF as acceptable framing. Only
    close-delimited bodies, and chunked bodies cut short, are refused. This is narrower than
    round 3's "refuse any GET without `Content-Length`".
  - **The byte budget is a fixed default sized for S3 error documents**, which are small. It
    applies to every body the client buffers, and to no successful GET body. Choose and
    state the value, and say in the docs that it exists. Remove the `// deferred:` markers
    #852 left for this issue as each case lands.
  **/ out of scope:** the upload path, including early acknowledgements and peers that stop
  reading (#854); any change to how a well-formed response is handled (the larger-than-budget
  GET case exists to prove there is none); the 5 GiB property (#761); TLS; any edit outside
  `crates/validate/` except the `05-building-block-view.md` paragraph if what the client
  promises changes.
- **Repro instruction:** on the base (#852 folded), serve a scripted loopback endpoint that
  answers `404`, `Content-Length: 0`, `x-amz-request-id: r1`. Point #852's client at it.
  Expected, per `iteration-v1/SUMMARY.md:187-195`: a GET reports `Code("NotFound")` instead
  of "no body". The other cases reproduce the same way, against their records in Defect.
- **External dependencies:** none. Base Rust toolchain; the scripted endpoint is plain
  `tokio::net` loopback.
- **Test file:** `crates/validate/tests/s3_client_nonconforming_responses.rs` (a NEW file,
  self-contained: no `tests/common/` module).
- **Citations expected:** `path:line` on the base for every change. Peer: #852's own
  error-mapping site and its `// deferred:` markers. The rubric's protocol-input rule is in
  `AGENTS.md`, "Recurring defect classes".
- **Prior-art check (triage cycles):** the response-handling code exists only in #852. No
  merged or closed PR touches `crates/validate/src/client.rs`. The findings above come from
  #741's own unpublished attempts, cited per case in Defect.
- **Disposition hint:** new-feature

## Plan-review response (#301 revision pass, 2026-10-02)

Four findings; all four revised the brief.

* **"The reproductions are not established for #852."** Correct: #852 is not built yet, so
  nothing has been run against it. Defect now calls each case a hypothesis for #852, gives
  the file and lines in `results/issue_741/` where it was observed, and names the SDK or
  hyper source that causes it. Falsifiability now makes the baseline Do's first step: build
  the test unchanged against #852's folded commit, then record red or green per case.
* **"The budget test can pass on a header check alone."** Correct. The binding budget case
  is now an otherwise valid `<Error>` sent chunked with no `Content-Length`, its `<Message>`
  growing past the budget, and the mutation removes only the streaming limit.
* **"Truncated chunked framing has no negative case."** Correct. Added: no terminal chunk
  and a missing final CRLF, as a body error on GET and as unreadable on all three
  operations for error responses whose XML is complete. hyper already reports both as
  `UnexpectedEof` (`decode.rs:259-269`), so these may be guards rather than reds; Do says
  which.
* **"The all-response budget promise exceeds the scope."** Correct as written. The promise
  is now "every body the client buffers", which the SDK's own code defines: every error and
  every PUT or DELETE response (`orchestrator.rs:527-536`). That adds one case (an
  over-budget PUT or DELETE success body). A successful GET is outside the budget and stays
  under #852's streaming bound; a new case proves a valid GET four times the budget is
  accepted unchanged.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Fix the three reproduced classification holes before claiming the invariant — malformed coding, bodyless-status framing, and nested scalar XML still yield trusted results (F1–F3; `target/crates/validate/src/s3/response.rs:85`, `target/crates/validate/src/s3/response.rs:198`, `target/crates/validate/src/s3/response.rs:327`).; **The test does not pin the 64 KiB budget; a budget 256 times larger passes all 14 tests.** I set `crates/validate/src/s3/response.rs:45` to `16 * 1024 * 1024` and ran the suite: 14/14 green. The endpoint wrote about 16.9 MB against a bound of 34,652,160. The bound (`s3_client_nonconforming_responses.rs:113-120`) is set almost entirely by `tcp_rmem[2]`, which is 32 MiB on this host. With the real budget the endpoint wrote only 135–229 KB. So the bytes-written oracle proves "some streaming limit under about 33 MiB", not 64 KiB. C5 agrees: mutants at `response.rs:45` (`*`→`+`, a 1088-byte budget that would refuse a legitimate ~20 KiB `SignatureDoesNotMatch`), `:215` and `:262` (`>`→`>=`) all survived. Fix: add an exact boundary pair. A chunked `<Error>` of exactly `BUDGET` bytes must be a clean `Service`, and one of `BUDGET + 1` bytes must be `Unreadable`. Optionally add a ~20 KiB legitimate error document, which the probe showed is read correctly today. This adds no new option or dev-dependency, and the `+1` case is red on #852.; **Three of the T4 blockers reproduce on the production path, so they are not reviewer noise.** (a) A GET 200 with `Transfer-Encoding: chunked\u{00A0}` that sends 1000 of 3000 bytes and closes is **accepted as a 1000-byte object**. The cause is the Unicode `trim()` at `crates/validate/src/s3/response.rs:85`. hyper's `HeaderValue::to_str()` refuses the value (`hyper-1.10.1/src/headers.rs:130-139`) and reads the body to EOF (`proto/h1/role.rs:1296-1298`). The surviving mutant at `response.rs:85` (guard → `true`) shows that no test sends any non-`chunked` coding on a GET. `Transfer-Encoding: gzip` is refused correctly today, but nothing checks it. (b) A GET `204` with `Transfer-Encoding: chunked` and `5\r\nhello\r\n` with no terminal chunk is **accepted as a 0-byte object**. hyper forces a zero-length body for 204 (`role.rs:1268`), but `response.rs:198` records `Chunked` for any 2xx. (c) `<Code>Slow<b/>Down</Code>` becomes a clean `Code("SlowDown")` on all three operations (`response.rs:327-331`). Each needs a production fix plus a test case.; **The "nothing after `</Error>` but whitespace or comments" rule is enforced but untested.** The mutant deleting `!` at `crates/validate/src/s3/response.rs:312` survived. That mutant would accept `<Error>…</Error><?pi x?>` as a clean error. The current code refuses it correctly (probe run). The `:345` mutant also survived, and `is_blank` (`:344-346`) looks unreachable because roxmltree keeps no whitespace text after the root. Add a case with a processing instruction after the root element.; **Reject non-HTTP whitespace in transfer coding.** `crates/validate/src/s3/response.rs:85` uses Unicode-aware `trim()`, so `Transfer-Encoding: chunked\u{00A0}` becomes `Framing::Chunked`. Hyper treats this value as close-delimited, allowing a GET cut off without a terminal chunk to finish successfully. This discrepancy is also recorded in `gate-logs/T4-batch-review.log`. Validate the original bytes using only HTTP space/tab trimming and add a malformed-coding GET regression.; **Account for bodyless statuses before accepting chunked GETs.** `crates/validate/src/s3/response.rs:198` admits every successful status into the streaming path. For `204 No Content` with `Transfer-Encoding: chunked`, hyper suppresses the body, while `crates/validate/src/s3.rs:198` accepts the framing with no length check. Consequently, even a response with no terminal chunk becomes a successful empty object. The frozen batch review confirms this case. Reject forbidden framing for bodyless statuses and add this GET regression.; **Reject nested elements in scalar error fields.** `crates/validate/src/s3/response.rs:328` concatenates all descendant text, turning `<Error><Code>Slow<Unexpected/>Down</Code></Error>` into a clean `Code("SlowDown")`. The same normalization affects `<Message>`. XML well-formedness does not establish that these fields contain scalar text. Validate their children before collecting text and exercise nested elements on GET, PUT, and DELETE; the frozen batch review identifies the same defect.; **Test acceptance at the buffered-body limit.** The budget cases at `crates/validate/tests/s3_client_nonconforming_responses.rs:1055` only demand rejection of oversized bodies. Frozen `C5-mutants.log` shows that changing `64 * 1024` to `64 + 1024` at `crates/validate/src/s3/response.rs:45` survives, as does changing the streaming comparison from `>` to `>=`. Thus the suite accepts a client that rejects valid error documents above 1,088 bytes or exactly at the documented limit. Add valid error-document cases just below and at 64 KiB, with declared-length and chunked framing on all three operations.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 76 mutants tested in 3m: 17 missed, 15 caught, 44 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Gates pass, but the patch adds a process-killing crash; fix it and close the remaining gaps. Keep the slice as is (size warning waived by the human). - Crash: roxmltree recurses per nesting level with no limit, so a deeply nested error body (e.g. <a>x4000, 28 KB; ~200 levels in debug) overflows the stack and aborts the validator (response.rs:328). Refuse nesting past a small depth before parsing (S3 error documents are two levels deep), or use a non-recursive reader. Add a nested-body case on GET, PUT and DELETE. - Extra bytes past the declared length: the human chose to BUILD the connection-level check (in scope by the human's decision). A body longer than its Content-Length must be unreadable on error responses and a body error on a GET success, including bytes after a chunked body's terminal chunk. Add the adversary's cases: whole SlowDown document then trailing junk/second <Error> on all three operations; GET 200 declaring 2000 and sending 3000. Make length-under-sent fail for the length reason, not the XML reason. - A PUT or DELETE 2xx carrying an unexpected body (e.g. Content-Length: 4 "junk", or a close-delimited "<Err") is NOT a success: report it as unreadable. Add tests. - Small gaps: a 304 with Transfer-Encoding on PUT/DELETE is unreadable, not NoBody (Content-Length on a 304 stays allowed); refuse a processing instruction before the root as well as after it; an empty or whitespace-only <Code> is MissingInXml, not Code(""). One test case each.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Gates pass, but the patch adds a process-killing crash; fix it and close the remaining gaps. Keep the slice as is (size warning waived by the human).
  - Crash: roxmltree recurses per nesting level with no limit, so a deeply nested error body (e.g. <a>x4000, 28 KB; ~200 levels in debug) overflows the stack and aborts the validator (response.rs:328). Refuse nesting past a small depth before parsing (S3 error documents are two levels deep), or use a non-recursive reader. Add a nested-body case on GET, PUT and DELETE.
  - Extra bytes past the declared length: the human chose to BUILD the connection-level check (in scope by the human's decision). A body longer than its Content-Length must be unreadable on error responses and a body error on a GET success, including bytes after a chunked body's terminal chunk. Add the adversary's cases: whole SlowDown document then trailing junk/second <Error> on all three operations; GET 200 declaring 2000 and sending 3000. Make length-under-sent fail for the length reason, not the XML reason.
  - A PUT or DELETE 2xx carrying an unexpected body (e.g. Content-Length: 4 "junk", or a close-delimited "<Err") is NOT a success: report it as unreadable. Add tests.
  - Small gaps: a 304 with Transfer-Encoding on PUT/DELETE is unreadable, not NoBody (Content-Length on a 304 stays allowed); refuse a processing instruction before the root as well as after it; an empty or whitespace-only <Code> is MissingInXml, not Code(""). One test case each.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 71 mutants tested in 6m: 2 missed, 40 caught, 27 unviable, 2 timeouts
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: One bug left: a successful chunked GET whose trailer section is followed by junk bytes is accepted as a clean object (F1; batch review, reviewer and adversary all reproduced it). Everything else holds; the size waiver from round 2 still stands. - Fix: in `WholeResponse::poll_frame` (crates/validate/src/s3/wire.rs:305-306), do not forward a trailers frame straight away. Poll the inner body to its end, run the connection's extra-bytes check, then release the trailers, or return Overrun if anything followed. (Alternative: make ObjectBody drive the body to its true end.) The module doc (s3.rs:112-114) and 05-building-block-view.md must then be true as written. - Tests to add: GET 200 chunked + trailer + junk -> body error; GET 200 chunked + trailer + clean end -> accepted byte-identical; trailer + junk on a 503 error on GET, PUT and DELETE -> unreadable; PUT/DELETE 200 with trailer + junk -> unreadable. Also cover a trailer followed by a second response and by more chunks. - Do not change anything else; the rest of the patch passed review.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  One bug left: a successful chunked GET whose trailer section is followed by junk bytes is accepted as a clean object (F1; batch review, reviewer and adversary all reproduced it). Everything else holds; the size waiver from round 2 still stands.
  - Fix: in `WholeResponse::poll_frame` (crates/validate/src/s3/wire.rs:305-306), do not forward a trailers frame straight away. Poll the inner body to its end, run the connection's extra-bytes check, then release the trailers, or return Overrun if anything followed. (Alternative: make ObjectBody drive the body to its true end.) The module doc (s3.rs:112-114) and 05-building-block-view.md must then be true as written.
  - Tests to add: GET 200 chunked + trailer + junk -> body error; GET 200 chunked + trailer + clean end -> accepted byte-identical; trailer + junk on a 503 error on GET, PUT and DELETE -> unreadable; PUT/DELETE 200 with trailer + junk -> unreadable. Also cover a trailer followed by a second response and by more chunks.
  - Do not change anything else; the rest of the patch passed review.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 150 mutants tested in 14m: 4 missed, 88 caught, 51 unviable, 7 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: One bug left: a chunked GET whose trailer section comes with any `x-amz-checksum-*` header panics ("Data frame should have data") instead of returning a verdict, because the held trailers are handed to the SDK's checksum validator (crates/validate/src/s3/wire.rs:347-349). A GET 503 with the same framing also panics, while PUT and DELETE return Service{SlowDown}. - Fix: at `State::Ended`, drop the trailer section instead of returning it (`Poll::Ready(None)`), still after the connection's extra-bytes check. S3 defines no response trailers and nothing in the client reads them. - Rewrite the wire.rs:438-460 unit test, which asserts the trailers are handed over. Update any doc (s3.rs module doc, 05-building-block-view.md) that says trailers are released. - Tests to add: GET 200 chunked + trailer + correct checksum header -> accepted byte-identical; same with a wrong checksum -> body error (checksum mismatch); GET/PUT/DELETE 503 chunked SlowDown + trailer + checksum header -> the same Service{SlowDown} verdict on all three. - Change nothing else; the rest of the patch passed review. The size waiver still stands (waived again this round).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  One bug left: a chunked GET whose trailer section comes with any `x-amz-checksum-*` header panics ("Data frame should have data") instead of returning a verdict, because the held trailers are handed to the SDK's checksum validator (crates/validate/src/s3/wire.rs:347-349). A GET 503 with the same framing also panics, while PUT and DELETE return Service{SlowDown}.
  - Fix: at `State::Ended`, drop the trailer section instead of returning it (`Poll::Ready(None)`), still after the connection's extra-bytes check. S3 defines no response trailers and nothing in the client reads them.
  - Rewrite the wire.rs:438-460 unit test, which asserts the trailers are handed over. Update any doc (s3.rs module doc, 05-building-block-view.md) that says trailers are released.
  - Tests to add: GET 200 chunked + trailer + correct checksum header -> accepted byte-identical; same with a wrong checksum -> body error (checksum mismatch); GET/PUT/DELETE 503 chunked SlowDown + trailer + checksum header -> the same Service{SlowDown} verdict on all three.
  - Change nothing else; the rest of the patch passed review. The size waiver still stands (waived again this round).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 150 mutants tested in 11m: 4 missed, 90 caught, 51 unviable, 5 timeouts
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
