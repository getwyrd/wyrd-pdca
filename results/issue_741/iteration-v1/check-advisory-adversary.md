# Adversarial review — issue #741 (validate-s3-client-layer)

Method: copied `$PDCA_TARGET` (patch applied) into a scratch dir, ran
`cargo test -p wyrd-validate --test s3_client_roundtrip` (4/4 pass), then ran mutations
against `crates/validate/src/client.rs` and probe tests that point the production
`S3Client` at hand-written loopback responses and at the real in-process gateway.
Line numbers below are on the target checkout.

## What I could not break

- **Streaming oracles hold.** Tried to refute criterion 3 by mutation; could not.
  Buffering the PUT body before `send()` (collect the stream at `crates/validate/src/client.rs:158`)
  fails the PUT leg with "relay had forwarded only 0 request bytes" (`crates/validate/tests/s3_client_roundtrip.rs:368`).
  Collecting the GET body inside `get_object` (`client.rs:206`) fails the GET leg with
  "no GET response within 15s" (`s3_client_roundtrip.rs:416`). A lazy "collect on the first
  `next_chunk`, then hand out 64 KiB pieces" variant (`client.rs:321`) also fails, with
  "no body piece within 15s" (`s3_client_roundtrip.rs:428`). Both legs run the production
  path through a real relay. One weak spot: the `held <= GET_PREFIX` assertion at
  `s3_client_roundtrip.rs:438` can never fail, because the relay cannot forward past the limit
  before release. The 15 s bound is what actually catches a collecting client.
- **PUT body length checks hold.** Against the real gateway, a source that overruns
  (declared 5, yields 10) and one that runs short (declared 10, yields 5) both return
  `S3Error::Body`, and a later GET returns `NoSuchKey`, so nothing was stored. A GET body cut
  short of its declared `Content-Length` returns `S3Error::Body`. The guards at
  `client.rs:327`, `:338` and `:407` are never the thing that fires, though: hyper or the SDK's
  chunked-upload layer catches it first. That is why C5 lists them as missed mutants. T4
  already has this test gap, so I am not raising it again.
- **Gate shape.** C4-verify `UNVERIFIABLE` is the outcome the brief predicted. The log shows
  the reverted base failing to compile because the symbols this patch adds are missing. C4-diff-cov
  "does not apply on origin/main" means the patch was built on the merged wave-0 base, not
  `origin/main`. Neither counts as evidence against the fix. The gate fails overall because of
  T4-batch-review's 9 blocking findings. C4-ci being green proves little about the error mapping:
  its tests never reach any of the cases below.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:249-252`: an empty-body 404 is reported as
  a `<Code>` the server never sent. Reproduced: a loopback server answering
  `404, Content-Length: 0, x-amz-request-id: …` gives
  `Service{status: 404, code: Code("NotFound"), message: None}` for both GET and DELETE.
  The SDK makes this code up for any empty 404 (`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`),
  and `describe` trusts `meta.code()` before it checks `has_body`. That breaks the type's own
  rule (`crates/validate/src/error.rs:61`, "The body carried this `<Code>`") and the brief's
  Open Question 4 (keep "no body" apart). The result should be `ErrorCode::NoBody`. This
  confirms T4's finding with a reproduction.
- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:241-262`: a success response the SDK
  cannot parse comes back as an S3 *error response*, and the only diagnostic is thrown away.
  Reproduced: `200 OK, Content-Length: 5, Last-Modified: not-a-date`, body `hello`, gives
  `Service{status: 200, code: NoBody, message: None}`. That is three wrong facts. It is a
  success status filed as an error response. It says "no body" when 5 bytes arrived (the SDK's
  deserializer had already swapped the body out, so `bytes()` returns `None`). And it drops the
  SDK's "Failed to parse LastModified from header" (`aws-sdk-s3-1.148.0/src/protocol_serde/shape_get_object.rs:136`),
  because `describe` reads only `meta()`. `error.rs:23-24` names this exact case ("a malformed
  header") as `Unreadable`, and `docs/design/architecture/05-building-block-view.md:253` claims
  the two are kept apart. This is the defect a validator most needs to name: a gateway emitting
  a timestamp that fails its grammar is a class the rubric lists. The same arm turns an HTML 500
  into `Service{500, NoCodeInBody, request_id: None}` (T4's finding). Fix: route the operation's
  `Unhandled` service errors, and any 2xx, to `Unreadable`, and keep `DisplayErrorContext` as `detail`.
- NEEDS-HUMAN [impl] — `crates/validate/src/client.rs:326-333`: a torn GET body with no declared
  length is silently accepted as a complete, shorter object. Reproduced: `200 OK` with no
  `Content-Length`, no chunked encoding, `Connection: close`, body `hel`, then close, gives
  `content_length() == None`, then `"hel"`, then `Ok(None)`. No error at any point. The rubric's
  protocol-input rule says torn or truncated input must be an error or indeterminate, never
  silently accepted. The doc at `client.rs:317-319` promises "never a shorter object" but only
  checks it when a length was declared. S3 GetObject always declares `Content-Length`, so a body
  framed only by connection close should be refused, or at least flagged so the layer above
  cannot mistake it for a clean read.
- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_roundtrip.rs:328-336` (and `:252`):
  criterion 2 asks for "the `x-amz-request-id` the gateway stamps", but the tests only check the
  format (32 lowercase hex characters), and the PUT receipt only checks `is_some()`. Mutation:
  replace `client.rs:258-261` with `request_id: Some("0123456789abcdef0123456789abcdef".to_string())`.
  All 4 tests still pass. So the "asserted field by field" claim is not backed for this field.
  Fix: the relay already sees the response bytes, so have it capture the `x-amz-request-id`
  header and assert the typed error's id equals it.
- NEEDS-HUMAN [human] — `deny.toml` / `deny-all-features.toml` (the old RUSTSEC-2026-0253 entry,
  base `deny.toml:86`): the brief said to keep the waiver, rewrite its rationale, and record that
  the maintainer accepted an unsound `lru` in a shipped binary. The patch deletes the waiver
  instead. Checked: the deletion is correct. The base lockfile already resolves `aws-sdk-s3`
  1.148.0 (`Cargo.lock:210-211`) and `lru` 0.18.4 (`Cargo.lock:2139-2140`). `aws-sdk-s3` 1.144.0+
  requires `lru ^0.18.2`, while 1.142.0 still required `^0.16.3`. So the advisory matched nothing
  before this patch, and the waiver's own removal trigger had already fired. (I could not check
  1.143.0, so the "first release" claim at root `Cargo.toml:94` is unverified. The 1.144.0 floor
  is safe either way.) What a human must decide: the brief's header asks sign-off §9 to confirm
  and post to #741 that the RUSTSEC-2026-0253 exposure was *accepted*. That sentence would now be
  false. No unsound `lru` ships. The tracker note should say the exposure is moot, and the
  departure from the brief should be recorded as deliberate.
- NEEDS-HUMAN [impl] — root `Cargo.toml:92` (audit prose for the shipped dependency): it says
  "no rustls", but the `default-client` feature chosen at `Cargo.toml:100` pulls
  `rustls-native-certs`, `rustls-pki-types` and, on Unix, `openssl-probe` into the shipped graph
  (`cargo tree -p wyrd-validate -e normal`). The `rustls` crate itself is not pulled in, and
  `cargo deny` passes, so this is only an accuracy fix to the transitive-surface line of the
  audit the brief asked for. Name these crates.
