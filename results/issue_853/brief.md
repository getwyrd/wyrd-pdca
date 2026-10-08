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
