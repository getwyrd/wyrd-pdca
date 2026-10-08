<!-- pdca:split-proposal v1 -->
# Split proposal — issue 741

> **Intake-cap override (wyrd-pdca-P1), recorded for Act.** The maintainer waived the cap for
> this split in the Plan session of 2026-10-02 ("don't worry about intake cap and split it to
> as much make sense"). Count at the time: `planned 1/6 (cap, track blackbox) — room for 5`;
> this split adds 3. After accept, `plan-cap` reads `planned 5/6`: #742, #852, #853 and #854
> are PLANNED, and #741 itself still counts as BUILT until its split is closed. The waiver
> covers this split of #741 only.

## Why this slice is oversized

Three Do rounds did not converge, and every blocking finding was the same kind of thing.
Patches grew 56 KB → 88 KB → **103 KB** (over the 100 KB backstop), and blocking findings went
13 → 8 → 9 (`iteration-v{1,2,3}/`). The core never failed: round trip, typed errors and both
streaming oracles held from v1 on. Every buffering mutation turned red, and every reviewer
said so. What kept failing was the client's handling of servers that do not behave like S3:
- empty-body 404s getting a code the SDK made up;
- broken, truncated or trailing-junk XML being trusted;
- a success response the SDK could not parse, reported as an error response;
- error bodies whose length disagrees with `Content-Length`;
- GET bodies framed only by connection close;
- early acknowledgements of a PUT;
- error bodies collected with no byte cap (768 MiB body → 794 MiB peak);
- an upload left stuck behind a peer that stopped reading.

Each round fixed one batch and the review found the next. The growth shows where the size
went: `client.rs` went from 418 to 559 lines, but the test file went from **455 to 1359**, nearly
all of it scripted hostile responses.

So the seam is **who sends the response**, not "core vs the four items still open":

| child | the peer its tests run against | v1-3 evidence |
|---|---|---|
| 1. client core | the real in-process Wyrd gateway, plus a relay that may delay, withhold or cut bytes but **never invents or alters** them | proven in all three rounds |
| 2. non-conforming responses | a scripted endpoint that writes bytes no conforming S3 server sends | rounds 1-3 findings on reading responses |
| 3. misbehaving upload peers | a scripted peer that acknowledges a PUT early and/or stops reading | round-2 early-ack finding, round-3 stuck-upload finding |

Two things make this split pay for itself. First, child 1 lands at about v1's size: core plus
timeouts and source integrity, no scripted fixture. Its review can answer every
hostile-response finding with "deferred — tracked in #child-2/#child-3", which the Wyrd rubric
treats as settled (`AGENTS.md`, Reviewer protocol, "Deferrals are settled"). Second, children
2 and 3 earn a **real** C4 red. Child 1 defines the whole error type, so their test files
compile against child 1's API. When the red leg reverts their production change, the tests
fail by assertion and are not UNVERIFIABLE. (Dry-run of `engine/scripts/run-verify.sh
--classify` on synthetic patches: each child classifies `ADDED_TEST` + `CRATE crates/validate`,
and the crate exists on the base, so `GREEN_ONLY=0`.)

Children 2 and 3 are separate for a reason: they touch different paths of `client.rs` (the
response-reading path vs the upload path) and carry different risk. Child 3's backpressure leg
is the one finding the round-3 reviewers disagreed on: reproduced with 32 MiB frames, not
reproduced with small ones. It may hit a limit of the SDK's connector API. Keeping it apart
means the response hardening can land whatever child 3 finds.

## Wave sketch

```
#775 (in batch, COMPLETE) ──▶ child-1 ──▶ child-2 ──▶ child-3
                                  └──────────────▶ #742 (re-pointed to child-1)
```

- **child-2 and child-3 `Depends on` child-1.** This is a real build-on dependency: both
  harden child 1's `client.rs`, and their tests call child 1's API.
- **child-2 `Conflicts with` child-3.** Both edit `crates/validate/src/client.rs` (and likely
  `error.rs`'s docs and the `05-building-block-view.md` paragraph). Neither builds on the
  other, so they take separate waves and the later one builds on the earlier one's folded
  result.
- **child-1 on #775.** The proposal format accepts only sibling labels in ordering fields, so
  this edge is added to child 1's materialised brief after `--accept`, as `Depends on
  (merged): 775`. In this run #775 is in the batch and COMPLETE, so it is satisfied and child 1
  builds on the run's integration branch, which carries #774 and #775. In a later run outside
  this batch it holds child 1 until PR #849 merges, and does not let it build on a `main`
  that has no `crates/validate` (INTEGRATION §2: "declare `Depends on (merged)` when the
  dependency is on merged content").
- **#742 is re-pointed from `741` to child-1** after `--accept`. A split parent never reaches
  COMPLETE, so #742 would otherwise be skipped. #742 needs only child 1, because child 1 is
  what brings the aws SDK into `crates/validate`'s dependency set, and that is what #742's
  image build must compile. #742 shares no file with any child.

<!-- pdca:child child-1 -->
# Brief — validate-s3-client-core

- **Slug:** validate-s3-client-core
- **Track:** blackbox
- **Goal:** `wyrd-validate` gets the S3 client every later slice calls through, proven against
  a conforming endpoint. It is `aws-sdk-s3` on an arbitrary `--endpoint` (path-style, static
  SigV4 credentials, plain HTTP), with a typed error carrying status, `<Code>`, `<Message>`
  and `x-amz-request-id` as fields, bounded waits, and bodies that stream both ways.
- **Defect:** (the gap) On the base, `crates/validate` parses flags, resolves credentials,
  echoes them and exits (`crates/validate/src/lib.rs:5-8`). Its manifest says "Deliberately
  NO dependencies … the S3 client and its dependency audit arrive in their own slice (#741)".
- **Success criterion:** BINDING, shown by C4-verify in-process with no container and no
  network. The production client runs against a Wyrd S3 gateway served in-process on loopback
  from the crate's dev-dependencies:
  1. **Round trip.** PUT → GET → DELETE, the GET byte-identical. A GET after the DELETE
     reports the typed not-found error (404, `NoSuchKey`), checked field by field. An empty
     object also round-trips.
  2. **Typed error.** An error response surfaces as a value with the HTTP status, `<Code>`,
     `<Message>` and `x-amz-request-id`. The id must **equal** the header the relay saw the
     gateway send, not just have the right shape. No substring-matching of `Display`.
  3. **Streaming, both directions.** Each direction has an oracle that fails if the client
     aggregates the body. The oracles sit on a loopback relay between the client and the
     gateway; it counts bytes per direction and can withhold the response tail.
     - **PUT:** the generator is never materialised. When it is asked for its FINAL piece,
       the relay has already forwarded more than a stated floor of request bytes. The
       payload is tens of MiB, more than any socket or SDK buffer could hold.
     - **GET:** the relay forwards a prefix, then withholds the tail. The client must hand
       the test a body piece during the hold (bounded wait, so a failure is a message, not a
       hang). Then the tail is released and every byte is compared.
     - Do runs both mutations (buffer the PUT, collect the GET), records that each assertion
       fires, reverts both, and reports them in `build-notes.md`. Chunk counts or largest
       buffer seen are not acceptable for the GET leg: "collect, then re-chunk small" passes
       them.
  4. **Integrity and bounded waits.**
     - A PUT whose source ends short, runs long (including excess in a separate piece) or
       errors fails with the body error, and nothing is stored.
     - A GET whose connection the relay cuts mid-body fails with the body error, never as a
       shorter object.
     - Each deadline (connect, operation, body-idle) expires as a typed timeout naming its
       phase.
     - A request the SDK refuses to build (an empty key) is reported as exactly that.
- **Falsifiability:** RED is reachable on the plain harness: `cargo test -p wyrd-validate
  --test s3_client_roundtrip`, no Docker, no network. The pre-fix red is criterion-absence;
  criterion 3's two mutations are the demonstrated red.
  The base has `crates/validate` (from #774), so `run-verify.sh` skips `GREEN_ONLY`
  (`engine/scripts/run-verify.sh:410-412`). It reverts the production files, keeps the test,
  and the test fails to compile. That scores `UNVERIFIABLE` (§6 NEEDS-HUMAN, non-gating), the
  correct verdict for net-new API.
  **First, check the base:** `ls crates/validate`; `grep -n validate Cargo.toml`. If the
  crate is absent, STOP and report. Do not recreate #774's crate or #775's lint.
- **Invariant to restore:**
  - (a) *The validator never holds an object whole, in either direction.* This is the
    "stream, don't buffer" invariant the server side already holds:
    `crates/gateway-s3/src/lib.rs:12-17` and `crates/core/src/write.rs:530-535` ("Peak
    resident bytes are one `chunk_size` piece … independent of object size", the `0015:789`
    OOM cliff).
  - (b) *For a conforming response, every fact the typed error reports is the one the server
    sent, and no await on the endpoint is unbounded.* Sources: proposal 0017 §6-7; `AGENTS.md`
    "Await discipline" ("every await on external work is bounded … spawned helper tasks are
    aborted on drop").
  - Non-conforming responses are child-2's and child-3's invariant, not this one's.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** high. The diff is moderate, but it moves the aws SDK's ~100 crates into the
  shipped dependency graph.
- **Do model:** opus
- **Scope:**
  - (a) A client built from `ResolvedConfig`, public in the **library** (`crates/validate/src/`)
    so the integration test can reach it: endpoint, region, static credentials, path-style,
    plain-HTTP connector, retries and stalled-stream protection **off**.
  - (b) The typed error in its **final shape**, with one variant per place a call can fail:
    an S3 error response; a response that cannot be read as S3; no response; a deadline,
    naming its phase; a request never built; an object body that failed or cannot be trusted.
    Its `<Code>` keeps "this code", "XML without a code" and "no body" apart. Children 2 and 3
    must be able to test against it without adding a variant.
  - (c) The request id read from the `x-amz-request-id` header itself. The SDK's generic
    accessor prefers `x-amzn-requestid` when both are present.
  - (d) Streaming PUT from a caller's source with a declared length, and a GET body read
    piece by piece.
  - (e) Connect, operation and body-idle deadlines, with defaults. Their tests need no
    invented bytes: a peer that accepts and never answers, a listener with a full accept
    queue, and the relay holding a real GET's tail.
  - (f) The dependency move (below).
  - (g) Update `docs/design/architecture/05-building-block-view.md:253` ("the S3 client
    arrives with #741").
  - (h) Test fixtures own every task they spawn and abort it on drop, unlike the detached
    spawn at `crates/server/tests/s3_http_wire.rs:88-90`.

  **/ out of scope:**
  - Every response a conforming S3 server does not send: empty-body errors, XML without a
    code, non-XML pages, broken or trailing-junk `<Error>` documents, unreadable 2xx, error
    bodies whose length disagrees with their framing, close-delimited or chunked GET framing,
    and the error-body byte budget (child-2).
  - Every upload peer that acknowledges early or stops reading (child-3).
  - Mark each site where those belong with `// deferred: #<id>` naming the sibling issue
    (numbers filled in below after accept). Implementing them here is how this slice grew to
    103 KB.
  - Also out:
    - the 5 GiB property (#761);
    - TLS (`crates/gateway-s3/src/lib.rs:50-57`);
    - the capability matrix and `smoke` (#743), so the binary still echoes and exits;
    - any gateway change;
    - **any edit under `crates/server/`**.
- **Repro instruction:** on the base (`main` + #774 + #775), `grep -rn aws crates/validate/`
  returns nothing, and `crates/server/Cargo.toml:131-135` holds the aws crates only under
  `[dev-dependencies]` (`:114`).
- **External dependencies:** none. Base toolchain only, and that is a scope rule: the gateway
  is served in-process from dev-dependencies. Reaching for a container, MinIO, a `wyrd s3`
  process or the network means the fixture is wrong. Stop and declare it.
- **Test file:** `crates/validate/tests/s3_client_roundtrip.rs` (NEW, self-contained: no
  `tests/common/` module, which the gate's classifier would treat as a test target).
- **Verification posture:** DECLARED (net-new API). Criteria 1, 2 and 4 are red by absence,
  so C4-verify reports `UNVERIFIABLE`. **Built and exercised at Check:** the whole client,
  end to end against the real `S3Gateway` over real TCP, with no mocks. **Demonstrated red:**
  criterion 3's two mutations. Nothing is deferred to another environment.
- **Production reach:** traversed at Check (real SDK, listener and gateway). Record two
  limits in `build-notes.md`: plain HTTP, where deployment puts TLS in front (proposal 0017
  §10); and a redb + mem + local-FS composition, not production's.
- **Citations expected:** `path:line` on the base for every change. Peers Do MAY open:
  - **SDK config to copy:** `crates/server/tests/s3_gateway_cluster.rs:100-115`
    (`sdk_client`: `force_path_style(true)`, retries and stalled-stream protection disabled,
    explicit `http_client`).
  - **In-process gateway:** `crates/server/tests/s3_http_wire.rs:56-92`. Use a 256 KiB chunk
    size, not its `with_chunk_size(8)`, and never call `serve_s3_role`.
  - **Request id:** `crates/gateway-s3/src/request_id.rs:40` and the stamp at
    `crates/gateway-s3/src/lib.rs:1552-1557`.
  - **Dependency wall:** `deny.toml:1-20`, `:77-86`, and `deny-all-features.toml:111`.
  - **Adoption-audit standard:** root `Cargo.toml:92-101` (`hmac`), `:134-142` (`roxmltree`).
  - **The lint:** `xtask/src/repo_guard.rs:34-42`, `:648-654`. `aws-sdk-s3` as a normal
    dependency is fine; Wyrd crates are allowed as dev-dependencies only (proposal 0017 §9).
- **Prior-art check (triage cycles):** by path. `git log origin/main -- crates/validate` →
  empty (the crate exists via open PRs #845 and #849, folded on this run's integration
  branch). `gh pr list --state all --search wyrd-validate` → #845, #849, #765 (proposal 0017),
  nothing else. The waiver came from #726 (closed), commit `28ff7b3`, on a dev-only ground.
  #741's three unpublished attempts are this split's history.
- **Disposition hint:** new-feature

## The dependency move

A shipped dependency is a human-only call (INTEGRATION §4). **It was taken:** promotion was
accepted in the 2026-08-17 Plan session, and the 2026-09-30 sign-off of #741 accepted the
floor and the waiver deletion below. Do writes the record; it does not re-open the decision.

- **Pin in `[workspace.dependencies]`.** `aws-sdk-s3` gets a **floor of `1.144.0`**: in the
  registry, `1.142.0` needs `lru ^0.16.3` and `1.144.0` needs `^0.18.2`. Set the smithy crates
  to floors the current `Cargo.lock` satisfies; it already resolves `aws-sdk-s3 1.148.0` and
  `lru 0.18.4`. The lockfile should gain only `wyrd-validate`'s edges.
- **Delete the RUSTSEC-2026-0253 waiver** (`deny.toml:77-86`, `deny-all-features.toml:111`).
  Its own removal trigger has already fired, and with this floor no unsound `lru` can enter
  the shipped graph. The advisory is not in the graph; say that, not that an exposure was
  accepted. `cargo deny check` must stay green on both configs.
- **Write the ADR-0003 §2 audit** in `build-notes.md`, plus a short adoption comment by the
  pins: licence, unsafe posture, maintenance, and the transitive surface from `cargo tree -p
  wyrd-validate -e normal`. Name what is new to the shipped graph. Round 1 got this wrong:
  `default-client` brings `rustls-native-certs`, `rustls-pki-types` and, on Unix,
  `openssl-probe`, though not `rustls` itself.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# Brief — validate-s3-client-nonconforming-responses

- **Slug:** validate-s3-client-nonconforming-responses
- **Track:** blackbox
- **Goal:** The validator reports every response a conforming S3 server would not send as what
  it is, and reads every response within a byte budget fixed by the client, not by the
  endpoint.
- **Defect:** child-1's client trusts the SDK's reading of a response. Reproduced in the three
  earlier attempts at #741 (Check evidence of `iteration-v1..v3`):
  - an empty-body 404 becomes `Code("NotFound")`, a code the SDK makes up (aws-sdk-s3
    `protocol_serde.rs:24-28`);
  - an HTML 500 becomes a clean S3 error;
  - `<Error><Code>SlowDown</Code>` with no closing tag, a `<Message>` cut off mid-text, or
    junk after `</Error>` (inside a correctly sized body) all become clean S3 errors with a
    code;
  - a `200` whose `Last-Modified` fails its grammar becomes an *error response*, with the
    SDK's diagnostic dropped;
  - on PUT and DELETE, an error body that disagrees with its declared `Content-Length`
    (including chunked framing that also declares a length) is trusted, while GET calls the
    same bytes unreadable, because the SDK enforces length on `GetObject` only;
  - error bodies are collected whole before any client code runs (a lazily streamed 768 MiB
    error body raised peak memory from 19 MiB to 794 MiB);
  - a GET body framed only by connection close is accepted as a complete, shorter object.
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
  - a success status the SDK cannot read → unreadable, with the SDK's diagnostic kept in the
    detail and the real status and request id;
  - **an error body larger than the client's budget**, streamed lazily by the endpoint and far
    larger than budget plus socket buffers → unreadable. The endpoint must have written **at
    most the budget plus a stated, bounded slack** before the client closed the connection.
    The oracle is bytes written at the endpoint, not process RSS. Do demonstrates the
    mutation: remove the budget and the assertion fires;
  - a GET `200` with no `Content-Length` and no chunked framing (close-delimited) → body
    error;
  - a GET `200` with chunked framing and a proper terminal chunk → **accepted**,
    byte-identical;
  - a GET whose received length disagrees with its declared length → body error.
- **Falsifiability:** a real red is available on the ordinary harness (`cargo test -p
  wyrd-validate --test s3_client_nonconforming_responses`, no Docker, no network). The test
  uses only child-1's public API and child-1's dev-dependencies, so when C4-verify reverts
  this patch's production change the test **compiles** against child-1's client and fails by
  assertion: the genuine `PASS` red leg. Two rules keep it that way. Exercise the byte
  budget at its **default**, so no new option is needed. Add no dev-dependency, because the
  red leg reverts manifests too. Do confirms each case is red on child-1's client before
  fixing. A case child 1 already handles is reported in `build-notes.md`, not counted as red.
- **Invariant to restore:** *A response that is not a whole, well-formed, correctly framed S3
  response is never reported as one: not as a clean S3 error, not as a success, not as a
  shorter object. Reading any response costs the validator memory bounded independently of
  what the endpoint sends.* Sources: `AGENTS.md` "Protocol input" ("torn, truncated, or
  oversize input is indeterminate or an error — never silently accepted. Enforce declared
  `Content-Length`, the chunked terminal CRLF …"); proposal 0017 §6-7 (the tool's verdicts
  are only as good as the facts it reports). Self-test: patching one operation does not
  satisfy this; the same bytes must classify the same way on GET, PUT and DELETE.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:** child-3
- **Ordering note:** builds on child-1's client and calls its API. `Conflicts with` child-3
  because both edit `crates/validate/src/client.rs`; neither builds on the other.
- **Surfaces:** data
- **Difficulty:** medium. Confined to `crates/validate/src/` and one new test file, with no
  cross-crate reach; the density is in edge cases, which the gates own.
- **Do model:** opus
- **Scope:** classification and bounded reading of every response a conforming S3 server does
  not send, on all three operations. Plan decisions already taken, not Do's to re-open:
  - **Chunked GET bodies are accepted.** Chunked framing marks its own end, and the rubric
    names the chunked terminal CRLF as acceptable framing. Only close-delimited bodies are
    refused. This is narrower than round 3's "refuse any GET without `Content-Length`".
  - **The byte budget is a fixed default sized for S3 error documents**, which are small.
    Choose and state the value, and say in the docs that it exists. Remove the
    `// deferred:` markers child-1 left for this issue as each case lands.
  **/ out of scope:** the upload path, including early acknowledgements and peers that stop
  reading (child-3); any change to how a well-formed response is handled; the 5 GiB property
  (#761); TLS; any edit outside `crates/validate/` except the `05-building-block-view.md`
  paragraph if what the client promises changes.
- **Repro instruction:** on the base (child-1 folded), serve a scripted loopback endpoint that
  answers `404`, `Content-Length: 0`, `x-amz-request-id: r1`. Point child-1's client at it.
  A GET reports `Code("NotFound")` instead of "no body". The other cases above reproduce the
  same way.
- **External dependencies:** none. Base Rust toolchain; the scripted endpoint is plain
  `tokio::net` loopback.
- **Test file:** `crates/validate/tests/s3_client_nonconforming_responses.rs` (a NEW file,
  self-contained: no `tests/common/` module).
- **Citations expected:** `path:line` on the base for every change. Peer: child-1's own
  error-mapping site and its `// deferred:` markers. The rubric's protocol-input rule is in
  `AGENTS.md`, "Recurring defect classes".
- **Prior-art check (triage cycles):** the response-handling code exists only in child-1. No
  merged or closed PR touches `crates/validate/src/client.rs`. The findings above come from
  #741's own unpublished attempts.
- **Disposition hint:** new-feature
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# Brief — validate-s3-client-upload-against-misbehaving-peers

- **Slug:** validate-s3-client-upload-against-misbehaving-peers
- **Track:** blackbox
- **Goal:** A PUT against a server that answers before it has the body, or that stops
  reading, ends as a failure, never as a receipt for bytes that were not sent. When the call
  returns, the caller's source and the connection have been released.
- **Defect:** child-1's client trusts the SDK's outcome for an upload. Reproduced in the
  earlier attempts at #741:
  - **Early acknowledgement (round 2).** A server that reads only the request head and
    answers `200` gets a receipt. With a declared 64 MiB body, only 2.8 MiB had been produced.
    With a 10-byte body whose source fails after the ack, a receipt came back too. hyper
    returns the response as soon as its head arrives and keeps polling the body in the
    background.
  - **Upload retained under backpressure (round 3, disputed).** The setup: a 512 MiB generated
    upload in 32 MiB pieces, and a peer that stops reading, lets the socket buffers fill, then
    sends an early `200`. The PUT returned, but the source stayed alive four seconds past a
    three-second operation limit, and was released only when the peer closed. A second
    reviewer could not reproduce this with small pieces.
- **Success criterion:** BINDING. The production client runs against a scripted loopback peer:
  1. **No receipt for unsent bytes.** A PUT acknowledged while its source still has data to
     produce fails with the body error, carrying the acknowledgement's request id. So does a
     source that fails after the acknowledgement.
  2. **Nothing outlives the call.** The peer reads the head, waits until the client's writes
     back up, sends `200`, keeps the socket open and never reads again; the source yields
     pieces of 32 MiB or more. By the time `put_object` returns (or within a stated small
     bound after it), the source has been **dropped** (observed through the test's own source
     type, not a new API) and the peer observes its connection **closed**.
  The inherent limit is documented, not tested as a failure: a source that finished producing
  before an early `200` gets a receipt, because the client cannot see whether the peer read
  bytes it already wrote.
- **Falsifiability:** RED for (1) is available on child-1's client by assertion (`cargo test
  -p wyrd-validate --test s3_client_upload_peers`, loopback only). For (2), Do must first
  **reproduce the retention on child-1's client** with large pieces. If it reproduces, that
  is the red. If it does not after a real attempt (piece size, socket buffer sizes,
  early-ack timing), record the setups and results in `build-notes.md`, keep (2) as a
  regression guard, and say it is green-only. That posture is declared here, so Check expects
  it. Either way the test uses only child-1's public API and dev-dependencies, so the C4 red
  leg compiles and runs.
- **Invariant to restore:** *An upload's reported outcome never claims more than the peer
  could have received, and no upload outlives its call: once `put_object` returns, nothing the
  client started still holds the caller's source or the connection.* Sources: `AGENTS.md`
  "Absent or unsupported entries" ("never silent success"), and "Await discipline" ("every
  await on external work is bounded … spawned helper tasks are aborted on drop"). Self-test:
  waking the body poller is not enough, because a connection blocked on socket writes never
  polls the body again. That was round 3's finding.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:** child-2
- **Ordering note:** builds on child-1's upload path. `Conflicts with` child-2 because both edit
  `crates/validate/src/client.rs`; the scheduler puts them in separate waves.
- **Surfaces:** data
- **Difficulty:** medium. One function's lifecycle plus one test file, but it reaches into how
  the SDK's connector owns the connection.
- **Do model:** opus
- **Scope:** the PUT path's outcome and its resource lifetime against a peer that acknowledges
  early or stops reading. Remove child-1's `// deferred:` markers for this issue as each case
  lands. **If releasing a connection blocked mid-write cannot be done through the SDK's public
  connector or runtime API without forking it, STOP and report with the evidence.** Choosing
  a different HTTP seam is a Plan question, not something to paper over.
  **/ out of scope:** response classification and byte budgets (child-2); GET; the 5 GiB
  property (#761); TLS; any edit outside `crates/validate/`.
- **Repro instruction:** on the base (child-1 folded), serve a loopback peer that reads the
  request head, replies `200 OK`, `Content-Length: 0`, `x-amz-request-id: early`, and stops
  reading. A child-1 `put_object` with a 64 MiB generator returns `Ok` with that request id.
- **External dependencies:** none. Base Rust toolchain; loopback sockets only. The
  backpressure leg depends on socket buffer behaviour, so size the pieces to overwhelm it
  rather than tuning the kernel.
- **Test file:** `crates/validate/tests/s3_client_upload_peers.rs` (a NEW file,
  self-contained).
- **Citations expected:** `path:line` on the base for every change. Peer: child-1's
  `put_object` and its source-body adapter, and the `// deferred:` markers it left.
- **Prior-art check (triage cycles):** the upload path exists only in child-1. No merged or
  closed PR touches it. The findings come from #741's unpublished attempts (rounds 2 and 3).
- **Disposition hint:** new-feature
<!-- pdca:end child-3 -->
