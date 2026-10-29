# Build notes — #853, iteration 5

Base: `pdca-integration/main-r42c5348f203d` at `61e53418bb3e73674eef9352733f0e985796d582`
(`stack-base-tip`; #852's client is in it: `crates/validate/src/s3.rs`, `s3/body.rs`,
`s3/error.rs`). The worktree started clean; I applied `iteration-v4/patch.diff` (applies
cleanly) and changed only what the iteration-4 carry-forward names. Everything else in
`patch.diff` is the v4 patch, byte for byte apart from the hunks listed below (I diffed the
two patches to check: the delta is `wire.rs`, the test file, and nothing else).

Locked versions differ from the brief's: the lock pins `aws-sdk-s3` 1.152.0 (brief says
1.148.0) and `aws-smithy-runtime` 1.16.0 (brief says 1.15.0). Citations below use the locked
versions; the cited lines are the same in both SDK versions.

## The bug (carry-forward, iteration 4)

A chunked GET whose terminal chunk carries a trailer section, with any `x-amz-checksum-*`
response header, panicked with "Data frame should have data" instead of returning a verdict;
a GET 503 with the same framing panicked too, while PUT and DELETE returned
`Service{SlowDown}`.

Cause, on the locked sources:

- The SDK's `ResponseChecksumInterceptor` on GetObject (`aws-sdk-s3` 1.152.0
  `operation/get_object.rs:140`) wraps the response body in `ChecksumBody` whenever a
  supported `x-amz-checksum-*` header is present and validation is enabled
  (`http_response_checksum.rs:130-140`). Validation is enabled by default
  (`ResponseChecksumValidation::WhenSupported`, `:112-127`); the client does not set it
  (`crates/validate/src/s3.rs:135-163` sets no checksum config). The wrap does not look at the
  status, so a 503 to a GET is wrapped too; PutObject and DeleteObject carry no response
  checksum interceptor, which is why PUT and DELETE never panicked.
- `ChecksumBody::poll_inner` does `frame.data_ref().expect("Data frame should have data")`
  on every frame (`aws-smithy-checksums` 0.65.0 `body/validate.rs:57`).
- v4's `WholeResponse` held hyper's trailers frame until the connection had answered and then
  released it (`iteration-v4` `wire.rs:322-325`, `:347-349`). That frame reached
  `ChecksumBody` and tripped the `expect`.

## The fix (`crates/validate/src/s3/wire.rs`)

As the carry-forward directs: the trailer section is dropped, never handed over, and the
body still ends only after the connection's extra-bytes check.

- `wire.rs:325` — `Ok(_section) => continue`: the trailers frame is dropped where hyper hands
  it over, and the loop polls hyper's body on to its end (`hyper` 1.10.1
  `body/incoming.rs:229-233`: after the one trailers frame the channel reads closed and the
  body ends with `None`), at which point the connection's task is asked what followed, exactly
  as before.
- `wire.rs:347` — `State::Ended => return Poll::Ready(None)`: the body's end, nothing else.
- The `trailers: Option<HeaderMap>` field and its initialisation are gone
  (`wire.rs:302-305`, `:139-144`), and the `HeaderMap` import moves into the test module
  (`wire.rs:366`).
- Docs: module doc `wire.rs:14-17`; `State::Ended` `wire.rs:269-270`; the `WholeResponse`
  doc `wire.rs:287-297` now records both reasons the section goes no further (a reader that
  stops at the frame; a reader that cannot take it at all, with the SDK citations).

I dropped the section at arrival rather than at `State::Ended` (the carry-forward's wording)
because the two are equivalent for every observer — on `Ended` the section would have been
dropped anyway, and on `Overran`/`Lost` it was already dropped with the body — and dropping at
arrival removes a field and a state the type no longer needs. The connection check still runs
after hyper's body has ended, as the unit test shows.

Unit test rewritten (`wire.rs:436-456`): a clean chunked body with a trailer section yields
`[Data("hello")]` and nothing else; with four junk bytes after the section it yields
`[Data("hello"), Overrun(4)]`. `Seen::Trailers` stays as a variant that `frames` would record
if a section ever leaked, so the `assert_eq!` would name it.

Docs outside `wire.rs`: the carry-forward says to update any doc that says trailers are
released. Neither the `s3.rs` module doc nor `05-building-block-view.md` says that; the one
sentence in `05-building-block-view.md:297` that mentions a trailer section ("bytes after a
chunked body's terminal chunk and trailer section … unreadable, or on a GET object a body
error") is still true as written. `error.rs:127` likewise. So neither is changed this round;
the v4 `05` hunk stands.

## Tests added (`crates/validate/tests/s3_client_nonconforming_responses.rs`)

All three cases the carry-forward asks for, against the production client over loopback:

- `a_chunked_get_with_a_trailer_section_is_checked_against_its_checksum_header` (`:1968`):
  GET 200, chunked, trailer section, `x-amz-checksum-sha256` of the object → accepted
  byte-identical; the same with another object's checksum → body error whose text carries the
  SDK's "body checksum mismatch" (`BodyError::Transport.detail`, via
  `body.rs:494-498`). The wrong-checksum case is what proves the validator actually runs (a
  header the SDK could not decode would silently disable validation,
  `http_response_checksum.rs:202-205`, and the accepted case would then pass vacuously).
- `an_error_body_with_a_trailer_section_and_a_checksum_header_is_the_error_it_says`
  (`:1378`): 503 chunked `SlowDown`, trailer section, `x-amz-checksum-sha256` of the document
  → `Service{503, SlowDown, message}` on GET, PUT and DELETE, via `on_every_operation`.
- Helpers: `checksum_sha256` (`:274`) uses `sha2`, which is a production dependency of
  `wyrd-validate` on the base (`git show HEAD:crates/validate/Cargo.toml` line 37), so no
  dev-dependency is added and the red leg still compiles. `base64` (`:279`) is a 15-line
  RFC 4648 encoder, self-checked against the RFC's seven test vectors
  (`base64_matches_the_rfc_4648_vectors`, `:298`); the crate has no base64 dependency to use.
- Module doc: one bullet (`:39-43`).

## Runs (the project's runner: `engine/scripts/run-verify.sh`, the configured C4-verify cmd)

Invoked as the gate does, with `PDCA_LANE=0 PDCA_BASE=61e5341… PDCA_BUNDLE=<bundle>`; it
runs `cargo test -p wyrd-validate --test s3_client_nonconforming_responses` in
`wyrd-verify-l0` with its own timeouts.

1. Real bundle (`results/issue_853/patch.diff`):
   - GREEN leg (fix applied): `28 passed; 0 failed` (25 v4 tests + 2 new async tests + the
     base64 unit test).
   - RED leg (every production file reverted to #852, test kept): `8 passed; 20 failed`.
     Both new tests among the failures: the 503 GET panics at
     `aws-smithy-checksums-0.65.0/src/body/validate.rs:57` (on #852 the SDK's own connector
     hands the same trailers frame to the same validator), and the 200 cases get
     `Body(LengthUndeclared)` (#852 refuses chunked GETs).
   - Verdict: `run-verify.sh: PASS — red without the fix, green with it (28 test(s) ran red)`.
2. Refutation bundle (scratch `pdca-builder-853-refute/bundle/patch.diff`: v4's `wire.rs`
   with the new tests kept — i.e. only this iteration's fix reverted):
   - GREEN leg: `26 passed; 2 failed` — exactly the two new tests, both
     `panicked at …/aws-smithy-checksums-0.65.0/src/body/validate.rs:57:45: Data frame should
     have data`. Everything else green, as in v4.
   - Verdict: `run-verify.sh: FAIL — the bundle's test does not pass with the fix applied`,
     which is the red I wanted from this bundle.

Commit-readiness: `cargo fmt --all` (reformatted one line of the base64 helper), `cargo
clippy -p wyrd-validate --all-targets -- -D warnings` clean, `typos` (1.48.0) clean on the
two touched files. The whole-tree `cargo xtask ci` is Check's C4-ci and was not run here.

## Refuting my own test

- **(a) Genuine red?** Yes, twice over. With only the fix reverted (v4 `wire.rs`), the two new
  tests fail with the SDK panic and nothing else fails (26/28). With all production reverted
  to #852, they fail too (20/28 fail overall). With the fix, 28/28.
- **(b) Production path?** Yes. The tests build `S3Client` through `resolve_config` and
  `S3Client::with_deadlines` (production defaults but shorter deadlines), call
  `get_object`/`put_object`/`delete_object`, and the bytes go through the client's own
  connector (`wire.rs`), hyper, the SDK's real `ChecksumBody`, the response guard, and the
  client's error reader. No mock, no copy. The checksum header is computed with the same
  `sha2` the production payload code uses; the only test-side code is the base64 encoder,
  which is checked against the RFC vectors and, more to the point, by the wrong-checksum case
  (an undecodable header would disable validation and the mismatch case would go green).
- **(c) Fixture includes the fault?** Yes. Every case sends a chunked body whose terminal
  chunk carries a two-field trailer section (`chunked_with_trailers`) **and** an
  `x-amz-checksum-sha256` header — the exact pair that made the SDK's validator see a trailers
  frame. The 503 case runs on all three operations, including the GET that panicked.

## Not done / for the human

- `C4-diff-cov` will again report "patch.diff does not apply on origin/main": the patch is
  against the integration branch with #852 folded in (per the brief), not `origin/main`.
  Nothing in this bundle can change that.
- No NEEDS-HUMAN external dependency: the loopback endpoint is plain `tokio::net`, and the
  runner needed nothing Plan did not name.
