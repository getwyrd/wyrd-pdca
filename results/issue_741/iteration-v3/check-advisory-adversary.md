# Adversarial review — issue #741 (validate-s3-client-layer), round 3

How I checked: I copied `$PDCA_TARGET` (patch applied) to scratch and built it (cargo 1.96). The
19-test suite passed 3 of 3 runs. I then wrote throwaway probe tests against the production
`S3Client`, using the suite's own scripted-endpoint helpers. Every result below is from a run,
not from reading code.

## Refutations that landed

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:358-361`: on PUT and DELETE, an error body
  shorter than its declared `Content-Length` is reported as a clean S3 error. That breaks the
  type's own promise at `crates/validate/src/error.rs:29` ("an error body shorter than its
  declared length" is `Unreadable`). Repro: a scripted `503` with `Transfer-Encoding: chunked` and
  `Content-Length: 500`, whose chunked body is a complete 56-byte
  `<Error><Code>SlowDown</Code><Message>m</Message></Error>`.
  DELETE gives `Service{503, Code("SlowDown"), message: Some("m")}`, and PUT gives the same.
  GET on the same bytes gives `Unreadable{… ContentLengthError { expected: 500, received: 56 }}`,
  because the SDK checks length only on GetObject. So one response gets two different
  classifications depending on the operation. This falls under the rubric's protocol-input class
  ("enforce declared `Content-Length`"). T4 already raised it; this confirms it with a
  reproduction. Fix: in `service_error`, compare the collected body length with any declared
  `Content-Length` (or refuse a response that carries both TE and CL) before trusting the SDK's
  reading. Add DELETE and PUT cases next to `an_error_body_cut_short_is_unreadable`
  (`crates/validate/tests/s3_client_roundtrip.rs:872`), which covers only the length-framed
  shape.

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:298` / `:127`: error bodies are read into
  memory with no byte cap. The only bound is the operation deadline, which defaults to 300 s
  (`client.rs:79`). Repro: a scripted endpoint answers DELETE with a chunked `500` and then streams
  1 MiB chunks, stopping at 768 MiB. With `operation_timeout = 20s` the call ends as
  `Timeout{Operation}`, and the process's peak memory (VmHWM) went from **19 MiB to 794 MiB**: the
  SDK held all 768 MiB before `service_error` ever ran. At the default deadline on a fast link
  that is many GiB. A broken or hostile endpoint can take the validator down with memory
  exhaustion, which is the failure the brief's "never holds … whole" reasoning exists to prevent.
  It also falls under the rubric's protocol-input class ("oversize input is … an error"). T4
  flagged it; this measures it. Possible fix: an SDK interceptor (`modify_before_deserialization`)
  that wraps non-2xx response bodies in a size-limited body, so going over the cap surfaces as
  `Unreadable`. Add a regression that streams past the cap.

## T4 findings that did NOT reproduce (the human adjudicating the gating T4 row should know)

- NEEDS-HUMAN [human] — T4's three blocking entries at `crates/validate/src/client.rs:496` claim
  that once hyper is stuck writing to a peer that stopped reading, the upload source and
  connection are held forever after the PUT returns. I could not reproduce this on the locked
  stack (hyper 1.10.1, aws-smithy-http-client 1.4.2), in three setups:
  1. Server reads only the head, never answers. Operation timeout of 1 s, 64 MiB generator: the
     source was released at once.
  2. Server reads the head, waits 1.5 s so the client fills the socket buffers (about 2.7 MiB
     sent), then sends `200` and never reads: the PUT returns `Body{request_id: Some(..)}` and
     the source is released within the 10 s window.
  3. Same as 2, but the server drains afterwards: it read about 3 MiB and then saw the client
     **close** the socket.

  So in my runs neither the source nor the socket outlived the PUT. That makes the patch's own
  caveat at `client.rs:170-171` ("a connection stuck writing … holds it until that server
  closes") look too pessimistic, and the T4 rows look like false positives on this dependency
  set. Whether to reject them, and whether to soften that doc line, is a sign-off call.

## Brief versus patch: a difference the reviewer could miss in either direction

- NEEDS-HUMAN [human] — base `deny.toml:77-86` and `deny-all-features.toml` (the
  RUSTSEC-2026-0253 waiver): the brief told Do to *rewrite* the waiver's rationale, keep its
  REMOVAL TRIGGER, and record the maintainer's acceptance of the exposure. The patch instead
  **deletes** the waiver from both files and pins `aws-sdk-s3 = "1.144.0"` (`Cargo.toml:94-98`),
  not the brief's `1.137.0`. This is correct on the facts. The **base** lockfile already resolves
  `aws-sdk-s3 1.148.0` → `lru 0.18.4`, and in the registry `aws-sdk-s3 1.142.0` depends on
  `lru ^0.16.3` while `1.144.0` depends on `^0.18.2`. So the waiver was already stale before this
  patch; its trigger had fired. `cargo deny check` is green (`gate-logs/C4-ci.log:3307-3311`).
  What follows from that: the RUSTSEC exposure the brief has sign-off §9 "confirm" no longer
  exists. The sentence mirrored onto #741 should say the advisory is not in the graph, not that
  an exposure was accepted. Deliverable (e) should be scored as superseded, not missing.

- NEEDS-HUMAN [human] — `crates/validate/src/client.rs:349` (and the disclosure at `:33`):
  request ids are read with the SDK's `RequestId` reader, which prefers `x-amzn-requestid` when it
  is present. Repro: a 404 carrying both `x-amz-request-id: scripted-request-741` and
  `x-amzn-requestid: from-a-proxy` gives `request_id: Some("from-a-proxy")`, and the id the
  gateway stamped is lost. Criterion 2 names "the `x-amz-request-id` the gateway stamps", and
  #529's join depends on that id. The Wyrd gateway never sends the `x-amzn` header, so this only
  matters if something in front of it adds one. The choice is disclosed; reading
  `raw.headers().get(REQUEST_ID_HEADER)` directly would remove the gap. Low priority.

## Tried to refute, could not

- **Criterion 3 evidence (the build notes were withheld from me, so I re-ran the mutations
  myself).** First I collected the PUT source in `put_object` before sending:
  `a_put_body_is_on_the_wire_before_the_generator_finishes` fails with "the relay had forwarded
  only 0 request bytes … held the 33554432-byte body". Then I collected `output.body` in
  `get_object`: `a_get_body_yields_while_the_response_tail_is_withheld` fails with "no GET
  response within 15s … collecting the body", and the round-trip test still passes, as it should.
  Both oracles go red on the real production path, and only for the reason they claim to test.
- **Criteria 1 and 2:** the request id is compared with the header the relay saw on the wire, so
  the iteration-1 mutation (a hard-coded id) can no longer pass. NoSuchKey after DELETE is
  asserted field by field.
- **The PUT outcome table (`client.rs:172-228`):** early ack while the tail is pending, a
  failure after the ack, refusal before the body, a timeout, and an empty object are all
  classified as documented. A 307 with no body gives `NoBody`; a BOM before `<Error>` is
  accepted; a whitespace-only body gives `Unreadable`.
- **Empty frames from the caller's source:** the SDK's aws-chunked encoder re-buffers into
  64 KiB chunks (`aws-runtime-1.10.0 …/http_body_1_x.rs:55-100`), so an empty piece cannot emit
  an early `0\r\n` terminator.
- **https endpoint downgraded to plaintext:** this cannot happen. `build_http()` uses hyper's
  `HttpConnector`, which refuses non-http schemes by default, and proxy env vars are not read
  (`ProxyConfig::disabled` unless configured).
- **Inherent limit, not a defect:** a server that reads only the head and answers `200` gets a
  **Receipt** if the source finished producing first. Repro: a 10-byte source gives
  `Ok(Receipt{… "early"})`. The client cannot see whether the peer read bytes it already wrote,
  and the patch's contract is worded as "produced", so this is consistent. The early-ack guard
  only catches servers that answer faster than the source produces.
- **C4-verify `UNVERIFIABLE`** is the compile-only red leg the brief declared in advance
  (`gate-logs/C4-verify.log`). It is not a defect.
