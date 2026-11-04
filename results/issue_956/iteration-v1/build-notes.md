# Build notes — #956 (743.2, second child of #938): bulk delete, probes, endpoint resolution

## Base and salvage

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `635e0bd` (`pdca-integrate: issue_955`), the
  bundle's `stack-base-tip` on `pdca-integration/main-rdfeb36496b2d`. `patch.diff` is the whole change against that
  commit: 6 files. The worktree is left with HEAD at `635e0bd` and the change uncommitted.
- Stop rule checked first. `sha256sum` of `results/issue_938/iteration-v3/patch.diff` = `b50cc4f5…7d69` (4,461 lines)
  and of its `build-notes.md` = `f2c4fcf9…32f7`, both as the brief states. Design not re-derived.
- `crates/validate/src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `deny*.toml` and the three older `s3_client_*` test files
  are untouched.

## What changed (line numbers are in the patched tree)

| Path | Change |
| --- | --- |
| `crates/validate/src/s3.rs:186`, `:191` | The two builder lines. |
| `crates/validate/src/s3.rs:455-489`, `:491-503`, `:506-541`, `:546-557` | `PathStyle`, its `ResolveEndpoint` impl, `path_style_url`, `path_segment`. |
| `crates/validate/src/s3.rs:3-9`, `:39-51`, `:84-89` | Module doc, re-exports of `Probe` and the delete-result types. |
| `crates/validate/src/s3/document.rs:71`, `:98`, `:107`, `:197` | `DeleteResult`, `DeleteRow`, `KeyError`, `delete_rows`. |
| `crates/validate/src/s3/verbs.rs:30`, `:50`, `:296`, `:331`, `:529` | `EMPTY_PART_ETAG`, `Probe`, `delete_objects`, `probe`, `not_built`. |
| `crates/validate/src/s3/verbs.rs:426` | `captured`'s comment: #955's two `deferred: #956` markers are resolved and gone. |
| `crates/validate/src/s3/answer.rs:336`, `:382` | `SuccessBody`, and `answer()` takes it (see "Where the salvage did not fit"). |
| `docs/design/architecture/05-building-block-view.md:303` | The `validate` paragraph: the salvage's, plus the additions below. |
| `crates/validate/tests/s3_client_smoke_verbs.rs` | This child's tests. Bundle copy: `results/issue_956/s3_client_smoke_verbs.rs`. |

## Where the salvage did not fit #955's tree, and what I did

The salvage was one patch; #955 took its half and changed it afterwards. Each difference:

1. **`SuccessBody` was gone.** #955 removed it from `answer.rs` because its verbs don't need it. The brief's list of
   owned hunks doesn't name it, but `Probe::success_body` (owned) returns it, and (C)(iv) and (G) cannot hold without
   it. I put it back as the salvage had it (`answer.rs:336`) and `answer()` takes it again. `head_object` and
   `fetched` pass `SuccessBody::Empty` (`verbs.rs:164`, and in `fetched`): no change in behaviour, since a HEAD's body
   is always empty (the connector reports any byte after its head as an overrun) and `fetched` only calls this for a
   non-2xx, which never reads `success`. Rejected: leave `answer` alone and add a second function for probes, 7 lines
   that repeat the body of `answer`, to spare 2 changed call sites.
2. **`read_whole` and `captured` differ.** They are #955's now and I kept #955's. The salvage's `captured` comment is
   carried over without the code around it. `captured` needed no code change: a `Timeout` passes through it, as the
   brief read. Red 16 shows the test would catch it if it did not.
3. **`Capture` now also tracks HEAD requests** (#955). That helped: `Capture::new(probe.reads())` (`verbs.rs:332`)
   gets #955's `framing_of_no_body` for free. The 304 leg passes unchanged.
4. **SDK citations moved.** The salvage cited `aws-sdk-s3` 1.152.0 and `aws-smithy-runtime-api` 1.19.0; `Cargo.lock`
   has 1.155.0 and 1.19.2. I opened every cited line in the locked versions. All hold; the only shift is
   `config/endpoint.rs`, whose test starts at `:1867`, not `:1866`. The docs now cite the locked versions.
5. **Test fixture.** #955 rewrote `fault_script` around `cut_listing`. It now takes the document (`fault_script(document,
   fault, id)`), and I moved #955's checking logic out of its listing test into `misreported_fault` (`tests:1700`), so
   the listing and probe tests share about 55 lines. #955's assertions are the same, moved not changed. `SPOILED_PAGES`
   loops over `LISTING_VERBS`, as a delete result has no `<IsTruncated>` to spoil.
6. **A `#955` test comment** (`a_read_verb_stopped_before…`) said the SDK's endpoint rules refuse the query. Now
   `PathStyle` does; comment only. The `answer.rs` test comment that said "all of which are GETs or HEADs" was false
   once probes exist; reworded.
7. **`Probe`'s variant docs** said `GET /{bucket}?uploads`. The bytes on the wire are `/{bucket}/?uploads`, the brief's
   form, so the docs now say that.

## The brief's additions, and what I added beyond the salvage

Required by the brief, no salvage evidence, reds in this cycle:

- **(B)/(G) full request checks.** `ProbeCase::request_fault` (`tests:709`) compares method, path, the exact sorted set
  of `uploads`/`uploadId`/`partNumber`/`location`/`delete` with decoded values (`operation_params`, `tests:680`), and
  `x-amz-copy-source`, which must be absent on every probe but `CopyObject`. `probe_reaches_the_gateway` and the (G)
  success legs both go through it, and the 304 leg too. `x-id` is ignored.
- **Probe deadlines.** `a_probe_under_the_connect_deadline…` (`tests:1345`) and `a_probe_under_the_operation_deadline…`
  (`tests:1366`), on every probe at once rather than one. The saturated listener is `s3_client_roundtrip.rs:1258`,
  copied; that peer file is unmodified, and sharing it would need a new `tests/common` module for one 14-line helper.

Added by me, each a gap I could name against the brief's words:

- The brief lists "refused, reset, closed unanswered" for `NoResponse`. The salvage had no reset leg. Added `Then::Reset`
  (`set_zero_linger`, an RST on the wire).
- "One percent-encoded path segment": added `wyrd/validate` → `wyrd%2Fvalidate`, `wyrd%41validate` →
  `wyrd%2541validate` (encoded once) and a non-ASCII name, against the real gateway on both endpoint forms.
- "Signed for `s3` in the configured region": the salvage only checked `/s3/aws4_request`; it now checks the region too.
- (D) "never a short delete result": `a_delete_result_row_that_cannot_be_read…` (`tests:2596`), since the salvage tested
  whole-document faults but no row fault. `a_delete_error_row_with_no_code…` (`tests:2630`) pins `MissingInXml` and a
  missing message, so nothing is filled in on the server's behalf.
- `a_2xx_whose_whole_document_is_under_another_root…` (`tests:4039`): the salvage's fault was a cut document only.
- (Scope line) "The existing verbs change only as the shared resolver implies": the endpoint-path test now also sends
  PUT, GET and DELETE, and the region test sends DELETE, so the doc's claim about the older verbs is shown.

Architecture doc beyond the salvage's paragraph, all to satisfy (H) "no sentence claims more than the tests show":

- #955 changed what "refused on every operation" means (a `Transfer-Encoding` alone is no longer refused where no body
  follows). That sentence now names the exception, and a new one states it.
- The two GETs turn off response-checksum validation (#955's, undocumented so far).
- A truncated page with nothing to continue from is refused.
- How a probe with no answer is classified (the brief's revised (B)).
- The endpoint path's join with one `/`, named with the two region changes, as the brief's ordering note requires. The
  salvage only said so in passing for bucket probes.

## Ruled out (salvage's, still standing)

`results/issue_938/iteration-v3/build-notes.md` "Rejected alternatives" holds, with its counts: keeping the SDK's rules
and patching the output, copying the rules' region checks, refusing `https` at resolution. I did not reopen them.

Left alone on purpose:

- `.force_path_style(true)` stays beside `PathStyle`, which ignores it. Only the SDK's resolver reads it. It is one
  harmless line in the salvage's hunk and the brief says to take the hunks as they are. Worth one look at review.
- `not_built` (`verbs.rs:529`) has no test. The SDK's builders only refuse a missing `key` or `objects`, and
  `delete_objects` always sets both, so no input reaches it. I kept it rather than an `expect`, so a later SDK that
  refuses more reports `RequestNotBuilt` and does not panic.
- `<DeleteResult>` elements other than `<Deleted>`/`<Error>` are passed over, and an empty result is an empty result.
  The rubric's "silent skip" class was the thing to check here. Such an element names no key, the module doc says
  counts are the caller's to judge (`document.rs:9-10`), and the test pins the behaviour.

## Red → green

Verification posture (a) net-new. Fully reverted to the base, the file does not build: `exit 101`, 25 errors, first
`error[E0432]: unresolved imports wyrd_validate::s3::DeleteRow, KeyError, Probe`. So C4-verify reports UNVERIFIABLE
(77), as the brief pre-declares.

Green: `cargo test -p wyrd-validate --test s3_client_smoke_verbs` → `45 passed; 0 failed`, about 1.7 s.
**(E):** `cargo test -p wyrd-validate` green: `s3_client_roundtrip` 15, `s3_client_upload_peers` 12,
`s3_client_nonconforming_responses` 28, all three files byte-identical to the base (`git diff` lists none of them).

**Demonstrated reds.** Each is one edit to production, the filtered test run, then the file restored and compared byte
for byte by a helper in scratch. Each exited 101 with the named test failing and no build error (three first attempts
were build errors from the crate's dead-code denial, not reds; I redid them so they compile). Quoted from this cycle's
runs, on the locked SDK:

The six the brief requires:

- **(1) Only the timeout when a probe's body stalls after a 2xx** → (C) RED, `22 case(s) lost or misreported the 200:
  CopyObject Stall: the 200 head was lost: Timeout { phase: BodyIdle, limit: 500ms }`.
- **(3) The SDK's default endpoint resolution back** (`DefaultResolver::new()` in place of `PathStyle`) → 4 tests RED:
  - bucket names: `42 case(s) not sent as one path segment`, e.g. `CreateBucket "wyrd-validate--x-s3" … not signed for s3
    in us-east-1`;
  - endpoint path: `("PUT", "/s3-prefixwyrd-validate-probe-/")`, `("HEAD", "/s3-prefixvalidate/dir/key")`, which is also
    my evidence for the doc's "straight against the bucket";
  - region: `aws-global: signed for /aws-global/s3/aws4_request` failed;
  - stopped before sending: the reason is `Invalid region`, not the query.
  - `a_probe_that_reached_the_connector…` stayed green under this edit. It does not depend on the resolver, so that is
    expected.
- **(3b) Rewrite the name** (`trim '-'` + lower case) → (B) RED, `reached the gateway as PUT /wyrd-validate-probe/, not
  PUT /wyrd-validate-probe-/`.
- **(5) Drop the `<Error>` rows** → (F) RED, `left: [Deleted { key: "mixed/a" }, Deleted { key: "mixed/c" }]`.
- **(6) A request refused before sending reported as `NoResponse`** → (B) RED, `CreateBucket of a 70,000-byte name: not
  refused as unsent: Err(NoResponse { … })`.
- **(16) A probe's expired deadline turned into `NoResponse` in `captured`** → both deadline tests RED,
  `left: Some(NoResponse { detail: "" }) right: Some(Timeout { phase: Connect, limit: 500ms })`, and likewise `Operation`.
- **(17) `UploadPart` with an `uploadId` other than the one named** → (B) and (G) RED, `its operation parameters are
  [("partNumber", "1"), ("uploadId", "wyrd-validate-probe-upload-other")], not […"wyrd-validate-probe-upload"]`.

Beyond those:

- `GetBucketLocation` sent as `ListMultipartUploads` → RED, `its operation parameters are [("uploads", "")], not
  [("location", "")]`. Both answers are the gateway's shared `501`, so only the request check can see this.
- Every probe's capture read as a GET → 304 test RED, `7 case(s) misread`.
- A 2xx probe's success body not checked → (C) RED, 11 cases, and the wrong-root test RED, 11 cases.
- `SuccessBody::Empty` accepting a body → (C) RED, 4 cases.
- No percent-encoding → RED, 28 cases. Endpoint path's trailing slash kept → RED, `/s3-prefix//wyrd-validate-probe-/`.

One edit stayed **green, and I expected it to**: removing `.disable_s3_express_session_auth(true)`. The line guards
against a later SDK only (`s3.rs:187-190` says so), since no endpoint `PathStyle` resolves names the S3 Express scheme.
No test can bind it on this SDK.

## Mutation testing (C5, advisory)

`cargo mutants --in-diff <production diff> --no-shuffle --cap-lints=true -j 2 -p wyrd-validate`, scratch output, on
`crates/validate/src` only: **36 mutants, 20 caught, 13 unviable, 3 timeouts, 0 missed**, in 5 minutes (exit 3 means
"some timed out", not "some survived").

The 3 timeouts are the ones the salvage saw (`s3.rs:507` → `Ok("xyzzy")`; `s3.rs:512` guard → `false`; its `||` → `&&`).
Each makes every request fail before it is sent, and tests that wait on a scripted endpoint then wait out their bound.
They are failing, not surviving, and cargo-mutants does not count a timeout as missed. I did not read their logs this
time; that reading is the salvage's, for the same three edits (`iteration-v3/build-notes.md`, "Mutation testing").

`.disable_s3_express_session_auth(true)` is a builder call with no return-value mutant, so this run does not exercise
the guard line I said no test can bind.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted, the file does not build, the pre-declared case. Each named behaviour was
  also broken on its own: every single edit above turned a named assertion red, apart from the guard line just named.
- **(b) Production path? Yes.** Every leg builds the client through `resolve_config` and `S3Client::with_deadlines`, so
  it runs the production `PathStyle` inside the real `aws-sdk-s3` orchestrator (its parameter interceptors, signer and
  endpoint application), the client's own `WireClient`, over real loopback TCP. (A)/(B) hit the real `S3Gateway` +
  `wyrd_server::Gateway`; the relay only forwards bytes and records heads. No mock of the client, SDK or resolver.
- **(c) Fixture includes the fault? Yes.** The host-name endpoint is a real `http://localhost:{port}`; the S3 Express,
  Outposts and ARN names are the real shapes the SDK's rules act on (the Outposts name is the SDK's own test name,
  `config/endpoint.rs:6438` in 1.155.0), and red 3 shows those rules do act on them. The saturated listener and the
  silent peer are real sockets; the reset is a real RST; "nothing was sent" is the relay's connection count, not an
  assumption.

## Commit-readiness

- `cargo fmt`: run on the crate; clean.
- `cargo clippy -p wyrd-validate --all-targets`: clean.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean.
- `python3 docs/publishing/tools/lint_docs.py`: OK. `render_site.py --check` (out dir in scratch): link audit OK.
- `cargo xtask ci-dry-run`, `blackbox-guard`, `statics`: pass.
- `cargo doc -p wyrd-validate --document-private-items --no-deps` stops on `crates/validate/src/sizes.rs:340`, a file
  this patch does not touch.
- Whole gate: `./engine/xtask.sh ci` (delegates `cargo xtask ci`, workspace build and test including DST) on this tree: `xtask ci: all checks passed`, exit 0.

## External dependencies

`typos` and `docs-renderer`, as the brief lists: both present and run. Nothing unlisted was needed: no Docker, no network
beyond `127.0.0.1` (`localhost` resolves to loopback). No NEEDS-HUMAN external-dependency item.
