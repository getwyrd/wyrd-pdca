# Adversarial review — issue #853, round 5 (trailer section dropped at the connector)

I re-ran the suite on the patched tree: 28/28 green. Then I attacked this round's change
(`crates/validate/src/s3/wire.rs:321-327`, where the trailer section is dropped and the body is
polled on to its end) and the claims around it. I built everything in a scratch copy of
`$PDCA_TARGET` with `cargo test --offline`, toolchain 1.96.0.

## This round's fix: attempted to refute, could not

- **The new tests do guard this round's fix.** I tried two wrong versions of
  `crates/validate/src/s3/wire.rs:325`. (a) Forward the section at once (the round-3 code).
  This fails 3 tests: both checksum tests panic at `aws-smithy-checksums-0.65.0
  validate.rs:57`, and three `chunked-trailer-then-*` GET cases are accepted as 3000-byte
  objects. (b) Hold the section until after the connection check, then hand it over (the
  round-4 code). This fails 2 tests, both with the same validator panic:
  `a_chunked_get_with_a_trailer_section_is_checked_against_its_checksum_header`
  (`crates/validate/tests/s3_client_nonconforming_responses.rs:1968`) and
  `an_error_body_with_a_trailer_section_and_a_checksum_header_is_the_error_it_says` (`:1378`).
  Both tests go through the public `S3Client` against the scripted loopback endpoint, so they
  test the real production code, not a copy.
- **The `continue` at `wire.rs:325` cannot hang or panic.** After the trailers frame, hyper's
  `Incoming` polls a `futures_channel` oneshot that has already been used. That returns
  `Err(Canceled)`, which becomes `Poll::Ready(None)` (`hyper-1.10.1 body/incoming.rs:229-233`).
  The unit test at `wire.rs:437-454` shows the body ends cleanly.
- **A checksum header plus junk after the trailer section.** I probed this. The result is
  `Body(Overrun { after: 4 })` on a GET 200, both chunked and `Content-Length` framed, and
  `Unreadable` with the overrun detail on a GET, PUT and DELETE 503. All correct.
- **Dropping the section unread is bounded.** hyper caps the trailer section at 16 KiB
  (`hyper-1.10.1 proto/h1/decode.rs:25`, `:181`).
- **C5 survivors.** Replacing `wire.rs:237` (`Drop` → `()`) gives an equivalent mutant.
  Dropping `ConnectionTask` also drops `stop`, so `stopped` resolves and `drive` returns
  (`wire.rs:200`), so the task still ends. `response.rs:233` is only the interceptor's
  name. `wire.rs:220` (guard → `true`) would need a peek error other than the three
  listed, which a loopback test cannot stage on demand. I'm not raising any of these.

## Findings

- NEEDS-HUMAN [human] — **The same 503 bytes classify differently on GET and on PUT/DELETE
  when an `x-amz-checksum-*` header does not match the body.** Concrete case:
  `503 Slow Down`, a complete `SlowDown` document, and `x-amz-checksum-sha256` set to the
  digest of `"other"`. With `Content-Length` framing, a GET returns
  `Unreadable { detail: "body checksum mismatch …" }`, while PUT and DELETE return
  `Service { code: Code("SlowDown") }`. The chunked-with-trailer framing gives the same split.
  The cause: the SDK wraps a GET body in its checksum validator whatever the status
  (`aws-sdk-s3-1.152.0 http_response_checksum.rs:130-140`), and no validator runs for PUT or
  DELETE. This breaks the brief's self-test ("the same bytes must classify the same way on
  GET, PUT and DELETE") and the test file's own promise
  (`s3_client_nonconforming_responses.rs:8-9`). This round's new 503 checksum case (`:1378-1407`)
  sends only a *matching* checksum, so it cannot see the split. The inconsistency likely
  predates this patch (#852's SDK path behaves the same), but this round's surface is
  exactly this interaction. I tag it `[human]` because the fix needs a decision: check
  checksum headers on error bodies on every operation (new checksum code or dependencies),
  or ignore them on error responses. The checksum interceptor wraps the body before
  `ResponseGuard` runs (`crates/validate/src/s3/response.rs:236-277`), so the guard cannot
  just strip the header.
- NEEDS-HUMAN [human] — **The 64 KiB budget counts decoded bytes, not wire bytes, so an
  endpoint can make the client read any amount of chunk framing.** Concrete case: a 503,
  chunked, with no `Content-Length`. The 134-byte `<Error><Code>SlowDown</Code><Message>x</Message>…`
  document is sent as 1-byte chunks, and each chunk-size line carries 65,536 leading zeros
  (`000…01`, valid `1*HEXDIG`). The endpoint wrote all 8,782,633 body bytes, 134 times the
  budget, with no close from the client. The verdict was a clean
  `Service { code: Code("SlowDown"), message: Some("x") }` on GET, PUT and DELETE, after about
  4 s each. The padding size has no limit except the 15-minute operation deadline
  (`crates/validate/src/s3.rs:102`). A second form, `1` plus 64 KiB of spaces before the CRLF,
  is also accepted on all three operations. That one is not RFC 9112 §7.1.1 grammar
  (whitespace is allowed only before `;`); hyper allows it (`hyper-1.10.1
  proto/h1/decode.rs:414-429`). Memory stays bounded, so the Goal's "never buffers more"
  holds. Two things do not hold: the doc claim that such a body "is cut off at the budget
  rather than read whole" (`s3.rs:26-28`, the `05-building-block-view.md` paragraph), and the
  rubric's "budgets sized for the worst-case **encoded** representation". The test oracle
  cannot see it, because it counts only document bytes and skips chunk framing
  (`s3_client_nonconforming_responses.rs:345-347`, `:564-588`). A fix means counting raw
  socket bytes in the connector (`wire.rs`) for buffered responses. That is an
  architecture and scope call, and the human froze the rest of the patch.
- NEEDS-HUMAN [human] — **A GET answered with a non-200 2xx is accepted as the whole
  object.** Concrete cases: `206 Partial Content`, `Content-Range: bytes 0-99/3000`,
  `Content-Length: 100` returns `Ok`, a 100-byte "object". `203` with 100 bytes returns `Ok`.
  `204` with `Content-Length: 0` returns `Ok`, an empty object. S3 sends none of these to a
  plain GET, and the patch's invariant says "never … a shorter object" (`s3.rs:15-17`). The
  guard streams any `is_success()` status (`response.rs:244-251`). The only 2xx check is for
  framing on a `204` (`response.rs:108-112`). Not one of the brief's listed cases, and likely
  inherited from #852, so this is a scope question: fix it here or in a follow-up issue.
- **The gate summary overstates the red leg** (informational, not the builder's to fix). The
  `C4-verify` row in `check-gates.json` says "28 test(s) ran red". `gate-logs/C4-verify.log`
  shows 20 failed and 8 passed on the reverted base: `base64_matches_the_rfc_4648_vectors`,
  the bodiless-404, non-XML, incomplete-chunked-error, close-delimited-GET, cut-short-GET,
  other-coding-GET and larger-than-budget-GET tests. The brief allows green guards, and
  every red is an assertion failure on a build that compiled, so the PASS verdict stands.
  Only the count in the row is wrong.

<!-- pdca:leaf-complete -->
