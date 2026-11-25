# Build notes — #955, iteration 2: the S3 client's read verbs and listings (first child of #938)

## Where this starts

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD **`376b106`** (`main`). The brief's base
  `955b9cb` is an ancestor; nothing under `crates/validate/` differs between them (iteration 1's check).
- Salvage hashes checked first, both match the brief (`b50cc4f5…d7d69`, `f2c4fcf9…632f7`). No stop condition.
- The carry-forward asks for fixes, not a new design, so I applied `iteration-v1/patch.diff` (applies cleanly) and changed
  it. Iteration 1's notes on what was kept from the salvage and what was cut for #956 still hold
  (`iteration-v1/build-notes.md`, sections "What this child keeps" and "Where I departed"), except for its item 1:
  `SuccessBody` is now gone altogether (item 4 below).
- `crates/validate/src/lib.rs` is untouched; no doc under `docs/` is touched (the architecture doc is #956's, per the
  brief). The three existing client test files are untouched. The worktree is left uncommitted at `376b106`.

## The carry-forward, item by item (line numbers are in the patched tree)

**1. v1 listing, empty last key.** `crates/validate/src/s3/document.rs:134`: the continuation point goes through
`.filter(|from| !from.is_empty())` after the fallback to the last key, so an empty key can no longer become `marker=`.
The filter sits after the `or_else`, so it covers both sources. New test
`a_truncated_v1_page_whose_last_key_is_empty_is_refused_and_asks_for_no_next_page`
(`crates/validate/tests/s3_client_smoke_verbs.rs:1239`): four cases (the only key or the last key empty, `<NextMarker>`
absent or empty). Each must be `ListingError::Page { page: 1, Answered }`, status 200, `Unreadable` with the body as sent,
and exactly one request at the endpoint.

**2. Checksum wrapper.** `crates/validate/src/s3/verbs.rs:34` `body_as_sent()` gives a per-call
`response_checksum_validation(WhenRequired)`, applied by `.customize().config_override(...)` on `get_object_range`
(`:74`) and `get_object_if_none_match` (`:97`). I read the SDK before using it: `ConfigOverrideRuntimePlugin::new`
(`aws-sdk-s3` 1.155.0 `src/config.rs:1732`) layers only what the override sets, so timeouts, retries and the connector stay
the client's, and the tests against the real gateway confirm the calls still work. `HeadObject` has no checksum handling
(`src/operation/head_object.rs` has no `Checksum` in it) and the listings have no response checksum, so neither needs it.
New tests:
- `a_304_or_an_error_carrying_a_checksum_header_keeps_its_code_and_asks_for_no_checksum` (`:1725`): both verbs, each with
  a `304`, a `404 NoSuchKey`, a `403 AccessDenied` and a bodiless `403`, all carrying `x-amz-checksum-crc32`. Asserts the
  status, the header as sent, the code and message or `NoBody`, the verb's own request, and no `x-amz-checksum-mode` on it.
- `a_get_2xx_carrying_a_checksum_header_that_does_not_match_streams_the_body_as_sent` (`:1784`): `206` and `200` with a
  checksum that does not match. The body comes back as sent and the header stays in the head.

The base `get_object` has the same flaw, as the sign-off says; it is out of scope here and I did not touch it. **It
needs its own issue; none exists yet that I know of.**

**3. Empty 200 listing.** A new `"empty"` case (`Content-Length: 0`) in
`listing_pages_that_are_not_whole_documents_are_refused_never_partial` (`:1025`), which loops over both listing versions.
New `Refused::UnreadableEmpty` asserts `Outcome::Unreadable`, an empty body, and status 200 on page 1. Red CF3 below is
the surviving mutant at `settle`'s guard (`crates/validate/src/s3/answer.rs:339`).

**4. The remaining C5 survivors.**
- `SuccessBody` is **removed**, not pinned. Its failing branch cannot be reached in this child: `fetched()` sends only
  non-2xx answers through it, and a HEAD's body is always empty or an overrun. `answer()` (`answer.rs:350`) now reports a
  2xx with the body as sent. The alternative was a unit test on a one-variant enum whose error branch no caller reaches:
  18 lines of production kept (the enum and `read`, iteration 1's `answer.rs:317-334`), plus about 8 of test, to pin dead code.
  **Cost of removal to #956:** the salvage's `Probe::success_body` uses `SuccessBody::{Empty, Document}`, and a probe's
  `200` *can* carry an unexpected body, so #956 has to add the enum back with its probes. That is about 25 lines it takes
  from the salvage anyway (`Document` was already removed in iteration 1), plus one extra argument on `answer()`.
  To back the claim that the branch was unreachable, I added
  `a_head_answered_with_a_body_keeps_its_200_and_reports_the_bytes_as_an_overrun` (`:1582`), with and without a
  `Content-Length`. If the connector ever stopped doing that, this test goes red. (`Outcome::Success` carries the body
  either way, so nothing would be hidden.)
- `Capture::name` (`answer.rs`, the `impl Intercept`): the trait requires the method, so there is no surface to drop. It is
  pinned by a unit test (`answer.rs:585`). That test only repeats a literal, which is what "pin the names" asks for; the
  name has no other observer, because every `Capture` hook returns `Ok`.

## One defect the carry-forward did not list: fixed here

While writing the `304` checksum case I probed its neighbour, and it failed:

A `304` with `Content-Length: 5000` to `get_object_if_none_match` came back as
`BodyFailed(Transport { received: 0, detail: "Invalid Content-Length: Expected 5000 bytes but 0 bytes were received" })`.

Same class as item 2. The SDK's `EnforceContentLengthInterceptor` (`aws-smithy-runtime` 1.16.2
`client/http/body/content_length_enforcement.rs:143`) wraps every GET's body at the hook where `Capture` takes it, and
runs first. RFC 9110 §8.6 allows that header on a `304` and `Framing::of` already lets it through
(`crates/validate/src/s3/response.rs:113-117`). Criterion (G) says "a GET's or HEAD's 304 may carry framing headers with no
body", and the Invariant says nothing is downgraded, so I fixed it inside this child's hunks, as the brief tells me to when
a test shows the production does not behave as read.

The fix: `refused_by_the_sdks_length_check` (`answer.rs:565`), called from `read_whole` (`answer.rs:537`, which now
takes the status). A `304`, with a declared length, nothing received, and the SDK's exact words for that length, reads as
the empty body it is. **This matches an error message, which is the part I like least; please weigh it at sign-off.** What I
ruled out:
- *Turning the wrapper off:* `disable_interceptor::<T>` needs the type, and `EnforceContentLengthInterceptor` is private.
  No config option exists for it (`defaults.rs:415` installs it unconditionally).
- *Running `Capture` before it:* default plugins run first, and `read_after_transmit` has no mutable access to the body.
- *Downcasting `ContentLengthError`:* needs `aws-smithy-runtime` as a direct dependency, which the brief's Scope forbids
  ("any new production dependency").
- *Skipping the read on a `304`:* one line, but it loses the `Overrun` check on bytes after a `304`, because the connector
  reports those through this read (`wire.rs:279-285`). The new test's second leg pins that it stays reported.
- *Treating any non-overrun failure on a `304` as empty:* no string match, but it would also swallow the connector's
  `State::Lost` ("nothing shows the response whole"), which upgrades an answer.

The match fails safe: if an SDK update rewords the message, the `304` reads as the failure it was reported as, and
`a_304_to_a_conditional_get_that_declares_a_content_length_is_not_refused` (`:1509`) goes red on that dependabot PR.
The cases no endpoint can produce (other words, wrong length, another status) are unit-tested at `answer.rs:594`.

I did not check whether the base `get_object` has a similar problem with this wrapper; it sends no conditional header, so a `304` is not an answer it expects, and it is out of scope.

## The test file

`crates/validate/tests/s3_client_smoke_verbs.rs` (NEW; a copy is in the bundle). 21 tests, plus 2 unit tests in
`answer.rs`. New since iteration 1: `:1239`, `:1509`, `:1582`, `:1623`, `:1725`, `:1784`, the `"empty"` case at `:1025`.
Unchanged from iteration 1: the other 15, including all those the brief names, and (H) at `:1991`.

## Running it

The project's runner is `cargo xtask` through `engine/xtask.sh`; it has no single-test subcommand. For the quick passes I
ran the command the brief's Falsifiability line names, `cargo test -p wyrd-validate --test s3_client_smoke_verbs`, always
under `timeout`. Check's gates run the real `cargo xtask ci`.

**Green:** `cargo test -p wyrd-validate`: lib 6, `cli_surface` 21, `payload_and_sizes` 26,
`s3_client_nonconforming_responses` 28, `s3_client_roundtrip` 15, `s3_client_smoke_verbs` 21, `s3_client_upload_peers` 12.
All passed. That is (E): the three named files pass unmodified.

## Red → green

Verification posture (a) net-new, as the brief pre-declares. **Full revert:** with `s3.rs`, `body.rs`, `error.rs`,
`response.rs` back at base and only the test and its dev-dependency left, the test does not build: exit 101, 34 errors
(`E0432` unresolved imports, 33 × `E0599` no method named `head_object`, `get_object_range`, …). So C4-verify reports
UNVERIFIABLE (77), as pre-declared.

**Demonstrated reds.** Each is one edit to production and a run of the lib tests plus the test file, then the file restored
(the script compared every `crates/validate/src/s3/*.rs` and `s3.rs` after the run: `RESTORED`). Each compiled and exited
101. The script was scratch (`$PDCA_SCRATCH/pdca-builder-955-reds/reds.py`) and is not kept.

Required by the brief:
- **(1)** a 2xx body timeout returned as only the timeout (`page`) → (C) RED: `2 case(s) lost or misreported the 200`.
- **(2)** accept a cut listing (`listing_page` returns an empty final page) → RED in (D), the later-page test, and (C):
  `8 case(s) not refused as they must be`.
- **(4)** stop after the first page → (A) RED: `ListObjectsV2 left: [("list/a", 0), ("list/b", 1)] right: [… 5 keys]`, and
  the same for `ListObjects` by the matching edit in the v1 loop.
- **(15)** every non-2xx error reported as `NoBody` (`settle`) → (H) RED: `20 case(s) lost the error's status, headers, code
  or message`; the checksum test too (`4 case(s)`).
- **(G)'s 304** refused by its framing (`Capture::new(false)` in `get_object_if_none_match`) → RED:
  `BodyFailed(Framing { transfer_encoding: Some("chunked"), content_length: None })`.

For this iteration's changes:
- **CF1** the empty-key filter removed → the new v1 test RED: `only-key-no-marker: page 1 must be refused: Err(Page { page: 2,
  … })`. That is the reviewer's finding: a second request was sent.
- **CF2a / CF2b** `config_override` removed from one verb at a time → both checksum tests RED, each time:
  `4 case(s) did not read the answer as sent`, and `body checksum mismatch. expected body checksum to be 3610a686 …`.
- **CF3** `settle`'s `!body.is_empty()` guard removed → `2 case(s) not refused as they must be` (v2 and v1).
- **CF4** `Capture::name` → `"xyzzy"` → the unit test RED.
- **New** the length-check match disabled → `left: BodyFailed(Transport { received: 0, detail: "Invalid Content-Length:
  Expected 5000 bytes but 0 bytes were received" }) right: Error { code: NoBody, message: None }`.
- **New** a HEAD's bytes not reported as an overrun → the HEAD test RED: `Success { body: b"" }`.

Two of my first edits did not compile (red 1's `Outcome` not in scope in `verbs.rs`, and the length-check edit leaving the
function dead under `warnings = "deny"`). I corrected them and reran; the results above are the compiled runs. The checksum
test originally stopped at its 2xx leg, hiding the `304`/`403`/`404` reds, so I split it into the two tests above and
reran CF2a and CF2b.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted, the test does not build (34 errors). Each single edit above turns named
  assertions red with the code compiling; none stayed green.
- **(b) Production path? Yes.** Every client is built through the production `resolve_config` and
  `S3Client::with_deadlines`, so each call runs the real `aws-sdk-s3` orchestrator, with its real checksum and
  `Content-Length` interceptors, then the production `Capture`, `read_whole`, `listing_page` and `settle`, over real
  loopback TCP. (A) and half of (H) hit a real `wyrd_gateway_s3::S3Gateway`. The scripted endpoint only writes fixed bytes.
  The two unit tests in `answer.rs` call the production functions directly.
- **(c) Fixture includes the fault? Yes.** The checksum header really does not match the body and the SDK's real validator
  reports it when not turned off (CF2 shows that message). The `304`'s `Content-Length` meets the SDK's real check. The
  empty key is a real `<Key></Key>`; the second request is really sent when the filter is gone (CF1 shows `page: 2`).

## Mutation testing (C5)

Run through the gate's own script, `scripts/mutants-in-diff`, on this bundle's `patch.diff` (the final one):

`107 mutants tested in 31m: 73 caught, 34 unviable`, exit 0. **Zero missed, zero timeouts.** Iteration 1 was 5 missed.

The first run of this iteration, before the last test was added, was `1 missed, 72 caught, 34 unviable`:
`answer.rs:494: delete match arm Framing::Untrusted{..} in read_whole`. That was a real test gap and not an equivalent
mutant: no test in this file drove framing the client cannot trust through the read path. I added
`an_answer_whose_framing_cannot_mark_its_end_is_refused_beside_its_status` (`crates/validate/tests/s3_client_smoke_verbs.rs:1623`):
each of the four read verbs, an unknown coding and both framing headers at once, on a `403`, and on a listing's `200`.
Each must be `BodyFailed(Framing { .. })` with the status and request id kept. Its red is that mutant, shown by the second
run catching it. I did not run it as a separate hand edit.

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean.
- `typos crates/validate`: clean.
- `cargo doc -p wyrd-validate --no-deps --document-private-items`, with `-D warnings` and `rustdoc::private_intra_doc_links`
  allowed: clean.
- I did not run the full `cargo xtask ci` (workspace build and test, DST included); Check's C4-ci runs it.

## External dependencies

`typos`, as the brief lists: present and run. Nothing else was needed: no Docker, no network beyond `127.0.0.1`. No
NEEDS-HUMAN external dependency.

## For the human at sign-off

1. The `304`-with-`Content-Length` fix matches an SDK error message (see above). It fails safe, but it is a string match.
2. The base `get_object` keeps the checksum flaw item 2 names. It needs an issue of its own.
3. #956 has to add `SuccessBody` back with its probes (about 25 lines from the salvage).
4. The iteration-1 adversary's fourth finding (the T4 batch-review row finishing in 30 s with no findings, so possibly
   cached or empty) was not in the carry-forward, and I did nothing about it. It is a harness question, not this patch's.
