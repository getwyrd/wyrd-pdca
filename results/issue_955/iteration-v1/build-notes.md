# Build notes — #955: the S3 client's read verbs and listings (first child of #938)

## Base and salvage

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l2`, HEAD **`376b106`** (`main`, "Merge pull request
  #990 … aws-sdk-s3-1.155.0"). The brief's base `955b9cb` is an ancestor (74 commits back). Between the two,
  nothing under `crates/validate/` changed (`git diff 955b9cb HEAD -- crates/validate` is empty); only `Cargo.lock`
  moved (dependabot bumps). So every base line the brief cites (`s3.rs:229`, `:281`, `:325`, `:339`,
  `response.rs:209`, `:231`, `:363`, `error.rs:64-76`) holds on `376b106` too.
- Salvage hashes checked before anything else, both match the brief:
  `iteration-v3/patch.diff` sha256 `b50cc4f5…d7d69` (4,461 lines), `iteration-v3/build-notes.md` sha256
  `f2c4fcf9…632f7`. No stop condition.
- The salvage applied cleanly to `376b106` (`git apply --check`; one offset hunk in `Cargo.lock`). I applied it
  whole, then removed what the brief gives to #956.
- `crates/validate/src/lib.rs` is untouched. No doc under `docs/` is touched. The worktree is left with HEAD at
  `376b106` and the change uncommitted. `patch.diff` (10 files, 3,345 lines, sha256 `e19be246…76100`) applies to
  the base (`git apply --cached --check` against a throwaway index read from `376b106`).

## What this child keeps and what it cut (line numbers are in the patched tree)

Kept as the salvage has them:
- `Cargo.lock:5229` and `crates/validate/Cargo.toml:63-65`: `wyrd-traits` under `[dev-dependencies]` only.
  `cargo xtask blackbox-guard` passes (no `wyrd-*` crate in the normal dependency closure).
- `crates/validate/src/s3.rs`: `mod answer; mod document; mod verbs;` (`:51`, `:53`, `:56`); the `pub use` lines
  (`:75`, `:77`); the `Deadlines` docs (`:106-114`); `object_length` (`:414-433`, which `get_object` now calls at
  `:318`, replacing the base's inline match at base `s3.rs:299-315`); the "every verb" cost line (`:239`).
- `crates/validate/src/s3/body.rs:354`, `:366` (`source_of`, `error_chain` made `pub(super)`),
  `crates/validate/src/s3/error.rs:86-89` (the `Phase` docs), `crates/validate/src/s3/response.rs` (the shared
  `whole_document` `:462`, `NotADocument`, `named_code` `:413`, `text_only` `:496`, `MAX_DOCUMENT_DEPTH` `:508`, and
  `error_document` rewritten onto them).
- `crates/validate/src/s3/answer.rs` (`Head` `:64`, `Outcome` `:138`, `BodyFailure` `:165`, `Fetched` `:263`,
  `NoResult` `:296`, `settle` `:340`, the `Capture` interceptor and its `read_before_transmit` `:452`, `read_whole`
  `:497`).
- `crates/validate/src/s3/document.rs` (`MAX_LISTING_PAGES` `:21`, `ListingError` `:46`, `listing_page` `:93`,
  `fields` `:145`, `decimal` `:168`).
- `crates/validate/src/s3/verbs.rs`: `head_object` `:26`, `get_object_range` `:44`, `get_object_if_none_match`
  `:67`, `list_objects_v2` `:95`, `list_objects_v1` `:129`, `captured` `:162`, `read` `:205`, `answer`, `fetched`
  `:230`, `page` `:260`.

Cut, as the brief says (#956's):
- `s3.rs`: the `PathStyle` resolver, `path_style_url`, `path_segment` (salvage `s3.rs:456-555`), their imports
  (`std::fmt::Write`, `aws_sdk_s3::config::endpoint::{…}`), the two builder lines
  `.endpoint_resolver(PathStyle)` / `.disable_s3_express_session_auth(true)`, and the `pub use` of `Probe`,
  `DeleteResult`, `DeleteRow`, `KeyError`. The module intro is back to the base's wording (base `s3.rs:3-6`), with
  "seven" → "eight" (`:5`) for the one new bullet.
- `verbs.rs`: `EMPTY_PART_ETAG`, `Probe` and its `impl`, `delete_objects`, `probe`, `not_built`, and the imports
  only they used (`BuildError`, `CompletedMultipartUpload`, `CompletedPart`, `Delete`, `ObjectIdentifier`,
  `delete_rows`, `DeleteResult`).
- `document.rs`: `DeleteResult`, `DeleteRow`, `KeyError`, `delete_rows`, `DELETE_RESULT`, and the
  `ErrorCode`/`named_code` imports only `delete_rows` used.
- The salvage's architecture-doc hunk (`docs/design/architecture/05-building-block-view.md:299`): the brief puts
  the architecture doc out of scope (#956).
- Test file: the `Relay`, every probe case and probe test, the bucket-name and endpoint-path tests, the region test,
  `delete_objects_reports_every_key_deleted_and_the_keys_are_gone`, the `(F)` mixed delete-result test, the probe
  `(C)` test, the probe success and `304` tests, and the probe/delete legs of the `200`-carrying-an-error test.

## Where I departed from "take the hunks as they are", and why

Each change is the smallest one that keeps the child true and compiling without #956's parts.

1. **`SuccessBody::Document` removed** (`answer.rs:317-334`). Only `Probe::success_body` built it. With the
   probes gone, the workspace's `warnings = "deny"` fails the build: `error: variant 'Document' is never
   constructed`. Options I weighed:
   - Remove the variant and its one match arm (−5 lines; the `whole_document` import in `answer.rs` goes too).
     #956 adds back the same 5 lines with the probes that need them.
   - Keep it under `#[allow(dead_code)]` (+1 line). Rejected: it leaves an allowance in production for code that
     no caller reaches, which a reviewer flags.
   I took the first.
2. **The `captured()` comment rewritten, with a `deferred: #956` marker** (`verbs.rs:182-191`). The salvage said
   "never sends an S3 Express `CreateSession` of the SDK's own (`S3Client::with_deadlines`)". That held only
   because of `.disable_s3_express_session_auth(true)`, which this child must not carry. I checked the SDK
   source in the cargo registry: in `aws-smithy-runtime` 1.16.2, `resolve_identity` runs at
   `client/orchestrator.rs:486`, before `read_before_transmit` at `:514`, and the S3 Express identity provider
   sends `client.create_session()` (`aws-sdk-s3` 1.155.0 `s3_express.rs:471`). So, for a bucket the SDK's rules
   read as S3 Express on a host-name endpoint, the SDK can send a request of its own before the hook. The comment
   now says only what holds here (the request asked for was never sent) and names that gap with a
   `// deferred: #956` marker in the repo's own form (`crates/custodian/src/backfill.rs:112`). The mapping itself
   (`NoResponse` with nothing transmitted → `RequestNotBuilt`) is the salvage's and is unchanged.
3. **Docs trimmed to what this child ships.** The brief says the `s3.rs` module doc describes only this child; I
   applied the same rule to the other doc comments that named probes or delete results, so none describes code
   that is not here: `s3.rs:27-30`, `:39-49`; `response.rs:44-45`, `:461`, `:504-505`; `answer.rs:294`;
   `document.rs:1-2`; `verbs.rs:1-2`. The `s3.rs` bullet says "a listing page whose 2xx carries an S3 `<Error>`
   document" (not "a 2xx", which in the salvage covered probes), and lists one cause of `RequestNotBuilt` ("an
   endpoint the SDK's endpoint rules refuse, for one"), the one this child's tests prove. The salvage's other two
   causes (an empty bucket name, an overlong URL) were proven through probes, which are gone.
4. **The architecture doc is not updated.** The repo's rubric asks a new API operation to update the living
   architecture doc in the same PR; the brief assigns that doc to #956. I followed the brief. Nothing the doc says
   becomes false: it does not list the client's verbs, and its `304` framing sentence is about PUT and DELETE,
   which this child does not change. If a reviewer raises it, the answer is "Deferred — tracked in #956".
5. **One salvage test renamed.** `listing_and_delete_result_documents_that_are_not_whole_are_refused_never_partial`
   → `listing_pages_that_are_not_whole_documents_are_refused_never_partial` (`tests/…:1017`). It now has only its
   listing legs, as the brief asks, so the old name would claim delete legs it no longer has. #956 can rename it
   back when it adds them. Its helpers lost their `Delete` arms (`DocumentVerb`, `Read`, `refused_answer`,
   `refused_as`); nothing else in it changed.
6. **Two small additions beyond the brief's list:**
   - `(C)` has a fifth case, `BodyFault::ErrorDocument`: a `200` page whose body is an S3 `<Error>` document is
     reported with the `200`, `InternalError` and its message. That is the salvage's own listing leg of
     `a_200_whose_body_is_an_s3_error_document_…` (which I dropped with the probes), folded in here so the `s3.rs`
     doc claim it backs stays tested.
   - `a_read_verb_stopped_before_the_connector_is_request_not_built_and_reaches_no_endpoint` (`tests/…:1718`).
     The salvage tested the transmitted-flag mapping only through probes. Without a test here, that branch of
     `captured()` (`verbs.rs:192`) and `Taken::transmitted` would have no test in this child. The test uses an
     endpoint with a query, which the SDK's default rules refuse ("Custom endpoint `…/?x=1` was not a valid URI",
     seen in red 16 below). HEAD and ListObjectsV2 must come back `RequestNotBuilt`, and a listener on that port
     must have no connection waiting in its backlog. The reverse case is an endpoint that reads the request and
     closes without answering: both verbs must stay `NoResponse`, and both requests must reach it.

No production behaviour changed relative to the salvage apart from (1), which removes code nothing calls.

## The test file

`crates/validate/tests/s3_client_smoke_verbs.rs` (NEW; a copy is in the bundle as `s3_client_smoke_verbs.rs`).
15 tests:

From the salvage (unchanged apart from item 5 above):
`head_reports_a_written_keys_size_and_etag_and_a_missing_keys_bare_404` (`:276`),
`a_ranged_get_is_206_with_its_content_range_and_exactly_those_bytes` (`:319`),
`a_conditional_get_is_304_on_its_etag_and_the_whole_object_on_another` (`:357`),
`both_listings_return_every_key_under_a_prefix_with_its_size_across_pages` (`:431`),
`listing_pages_that_are_not_whole_documents_are_refused_never_partial` (`:1017`),
`a_listing_whose_later_page_is_unreadable_reports_no_listing_at_all` (`:1120`),
`a_truncated_listing_page_that_names_nothing_to_continue_from_is_refused` (`:1169`),
`a_head_reports_every_header_line_a_repeated_one_and_one_that_is_not_text_included` (`:1228`),
`list_objects_v1_follows_a_next_marker_and_the_last_key_where_a_page_names_none` (`:1307`),
`a_listing_still_truncated_after_max_listing_pages_pages_stops_there` (`:1355`),
`a_listing_size_that_is_not_plain_decimal_digits_is_refused` (`:1784`).

New here:
- **(H)** `an_s3_error_through_every_new_read_verb_keeps_its_status_headers_code_and_message` (`:1617`).
  The real gateway: `list_objects_v2` / `list_objects_v1` on a gateway with no bucket record
  (`start_gateway_seeding(&[])`, `:151`) → `ListingError::Page { page: 1, cause: NoResult::Answered(_) }`, 404,
  `Code("NoSuchBucket")`, message "the specified bucket does not exist" (`crates/gateway-s3/src/lib.rs:935-944`);
  `get_object_range` / `get_object_if_none_match` of a missing key → `Fetched::Other`, 404, `Code("NoSuchKey")`,
  "the specified key does not exist" (`:2834-2841`); each with the gateway's request id. The scripted endpoint: one
  `403` (`forbidden`, `:1524`) carrying `x-amz-request-id`, `x-amz-meta-twice` sent twice around a
  `Content-Type`, and a `Content-Length`, with four bodies through each of the four verbs (16 cases): AccessDenied
  → `Code("AccessDenied")` + "Access Denied"; no `<Code>` → `MissingInXml` + the message; cut short and another
  root (`<Fault>`) → `Outcome::Unreadable` carrying the body as sent. Each case checks status 403, the request id,
  every header line as sent (the repeated one as both lines, in order), and that the endpoint saw that verb's one
  request (method, `Range`, `If-None-Match`, `list-type` or not). Then `head_object` answered by the same `403`
  with no body → `NoBody`, with the same header lines. It mirrors the nonconforming suite's `Expect::Service`
  (`s3_client_nonconforming_responses.rs:686-692`): status, code, message.
- **(C)** on a listing page:
  `a_listing_page_answered_200_keeps_its_200_when_the_body_stalls_trickles_is_cut_or_is_unreadable` (`:777`). The
  salvage's `BodyFault` / `fault_script`, given a cut `ListBucketResult` (`cut_listing`, `:703`) in place of the
  probe's cut document, and sent as ListObjectsV2 with `short_deadlines()` through `S3Client::with_deadlines`
  (body idle 500 ms, whole body 1.5 s). Each case must come back `ListingError::Page { page: 1, cause:
  Answered(_) }` with the `200` and its request id, and the failure beside it: `Timeout { BodyIdle, 500ms,
  received 10 }`, `Timeout { Body, 1.5s, received > 0 }`, `Transport`, `Unreadable { body as sent, "the listing
  body …" }`, plus the `ErrorDocument` case from item 6.
- **(G)'s 304**: `a_304_to_a_conditional_get_that_frames_a_chunked_body_is_not_refused` (`:1401`). A
  `304 Not Modified` with `ETag` and `Transfer-Encoding: chunked` answers `get_object_if_none_match` → `Fetched::Other`,
  304, the `transfer-encoding` header as sent, `Outcome::Error { NoBody, None }`, never `BodyFailed(Framing)`. The
  request carried the tag as given.
- `a_read_verb_stopped_before_the_connector_is_request_not_built_and_reaches_no_endpoint` (item 6).

## Running it

The project's runner is `cargo xtask` through `engine/xtask.sh`; it has no single-test subcommand (`ci` runs the
whole workspace). So for the quick red/green passes I ran the command the brief's Falsifiability line names,
`cargo test -p wyrd-validate --test s3_client_smoke_verbs`, always wrapped in `timeout 900`. Check's gates run the
real `cargo xtask ci`.

**Green:** `15 passed; 0 failed`, about 1.6 s. With `--nocapture` every per-case GREEN line printed: 5 for (C), 21
for (H) (4 gateway + 16 scripted + 1 HEAD).

**(E):** `cargo test -p wyrd-validate`, all green, and the three named files are untouched (not in `git status`):
`s3_client_roundtrip` 15, `s3_client_upload_peers` 12, `s3_client_nonconforming_responses` 28; plus
`cli_surface` 21, `payload_and_sizes` 26, lib unit 4.

## Red → green

Verification posture (a) net-new, as the brief pre-declares. **Full revert:** with `s3.rs`, `body.rs`,
`error.rs`, `response.rs` put back to the base (`git show HEAD:…`; the new modules are then not compiled) and only
the test plus its dev-dependency left, the test does not build: exit 101, 28 errors (`E0432` unresolved imports
`Answer`, `Fetched`, `Listing`, `ListingError`, `NoResult`, `Outcome`, `MAX_LISTING_PAGES` …; `E0599` no method
`head_object`, `get_object_range`, `get_object_if_none_match`, `list_objects_v1`, `list_objects_v2`). So C4-verify
reports UNVERIFIABLE (77), as pre-declared.

**Demonstrated reds.** Each is one edit to production, a run of the whole test file, then the file restored and
checked byte for byte (sha256 of every `crates/validate/src/s3/*.rs` file compared before and after the whole
run: all restored). The helper script was in `$PDCA_SCRATCH/pdca-builder-955-reds/`. Every red compiled and
exited 101.

1. *Only the timeout when a body stalls after a 2xx head* (`page` turns `BodyFailed(Timeout)` into
   `NoAnswer(S3Error::Timeout)`) → (C) RED, 1 test: `2 case(s) lost or misreported the 200: ListObjectsV2 Stall:
   the 200 head was lost: Err(Page { page: 1, cause: NoAnswer(Timeout { phase: BodyIdle, limit: 500ms }) })`, and
   the same for Trickle (`phase: Body, limit: 1.5s`).
2. *Accept a listing document cut short* (`listing_page` returns an empty final page when the body is not a whole
   `<ListBucketResult>`) → (D) RED, 3 tests: `6 case(s) not refused as they must be: ListV2 cut-short: not refused
   as Unreadable: Listed([])`, …; the later-page test (`the failure in words names the page and the answer:
   None`); and (C)'s NotADocument and ErrorDocument cases (`the 200 head was lost: Ok(Listing { entries: [] … })`).
   My first form of this edit did not compile (`NotADocument::describe` left unused under `warnings = "deny"`); the
   recorded run keeps `describe` called.
3. *Stop listing after the first page* (both loops return after page 1) → (A) RED against the real gateway:
   `assertion 'left == right' failed: ListObjectsV2 left: [("list/a", 0), ("list/b", 1)] right: [… 5 keys]`; also
   red: the v1 marker test, the page-cap test, the later-page test (4 tests in all).
4. *(15) Report every new verb's error response as `ErrorCode::NoBody`* (`settle`'s non-2xx arm returns
   `Error { NoBody, None }`) → (H) RED: `20 case(s) lost the error's status, headers, code or message`, every
   gateway and scripted case; only the HEAD case, which is `NoBody` anyway, stays green.
5. *(G)'s 304: refuse its framing* (`get_object_if_none_match` captures with `Capture::new(false)`, as for a
   non-GET) → RED: `left: BodyFailed(Framing { transfer_encoding: Some("chunked"), content_length: None }) right:
   Error { code: NoBody, message: None }`.
6. *(16) Ignore the transmitted flag* (`if false && !taken.transmitted()`) → the no-answer test RED: `HEAD:
   Err(NoResponse { detail: "dispatch failure: other: Custom endpoint 'http://127.0.0.1:43275/?x=1' was not a valid
   URI …" })`.

Reds 1, 2, 4 and 15 are the four the brief requires; 5 is (G)'s 304 red by the edit the brief names. The other
salvage tests carry the inherited red/green evidence from `iteration-v3/build-notes.md`, and reds 2 and 4 re-show
it for (D) and (A) on this tree.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted to the base, the test does not build (28 errors above). Each of the six
  single edits above turns named assertions red with the code compiling; none stayed green. Each of the three
  tests new here has its own red: (C) red 1, (H) red 15, (G)'s 304 red 5; the no-answer test red 16.
- **(b) Production path? Yes.** Every client is built through the production `resolve_config` and
  `S3Client::with_deadlines`, so each call runs the real `aws-sdk-s3` orchestrator with the production `Capture`
  interceptor, `read_whole`, `listing_page`, `settle`, the client's own `WireClient`, over real loopback TCP. (A)
  and the gateway half of (H) hit a real `wyrd_gateway_s3::S3Gateway` over a `wyrd_server::Gateway` (redb +
  `MemCoordination` + `FsChunkStore`). The scripted endpoint only writes fixed bytes; nothing of the client is
  mocked or copied.
- **(c) Fixture includes the fault? Yes.** The stall, trickle and cut are real socket behaviour against real
  deadlines; the over-budget pages are real 64 KiB + 1 bodies, with a `Content-Length` and chunked; the
  `NoSuchBucket` comes from a gateway that really has no bucket record (not seeded), and `NoSuchKey` from a key that
  was never written; the `403` carries a real repeated header line; the refused endpoint is refused by the SDK's
  real endpoint rules, and "nothing reached it" is checked on a real listener's backlog, not assumed.

## Mutation testing (C5, advisory)

Not run this cycle (the whole-patch sweep took about 21 minutes in #938's iteration 2). What I expect, not
measured: the two equivalent `Capture::name` survivors the salvage reported (`answer.rs`, an interceptor label
the SDK shows only when that interceptor fails, and every `Capture` hook returns `Ok`). The transmitted-flag
mutants are now caught by the no-answer test (red 16 is one of them).

## Commit-readiness

- `cargo fmt --all -- --check`: clean (I ran `cargo fmt --all`; it reflowed one `assert_eq!` in the test file and
  removed a trailing blank line in `verbs.rs`).
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean.
- `typos crates/validate` (typos-cli 1.48.0): clean.
- `cargo doc -p wyrd-validate --no-deps`, with and without `--document-private-items`: the only error is
  `crates/validate/src/sizes.rs:340` (public docs linking a private item), which is on the base and untouched here.
  With that one lint allowed, there are no other warnings.
- `cargo xtask statics`, `cargo xtask blackbox-guard`, `cargo xtask ci-dry-run`: pass.
- I did not run the full `cargo xtask ci` (workspace build and test, DST included); Check's C4-ci runs it.

## External dependencies

`typos`, as the brief lists: present and run. Nothing else was needed: no Docker, no network beyond `127.0.0.1`.
No NEEDS-HUMAN item.
