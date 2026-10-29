# Build notes — #938 (743.2), iteration 3: the S3 client verbs smoke sends

## Base

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, HEAD **`93a6f3715e8785a22fd1d1ab40206bcf51b3eef4`**
  (`pdca-integrate: issue_932`), the bundle's `stack-base-tip` on `pdca-integration/main-r42c5348f203d`.
- #853's code is on it, as the brief requires: `582da99 pdca-integrate: issue_853` is an ancestor;
  `crates/validate/src/s3/response.rs`, `crates/validate/src/s3/wire.rs` and
  `crates/validate/tests/s3_client_nonconforming_responses.rs` exist. Nothing of #853 was recreated.
- Starting point: `iteration-v2/patch.diff` applied cleanly; its test file was byte-identical to the bundle's copy.
  The sign-off said "keep the attempt-2 design unchanged ... fix only the bucket-name routing bug and the claims
  that overstate it", so this iteration changes the endpoint resolver, the comments and doc that describe it, and
  the tests for it. Nothing else in the design moved.
- `crates/validate/src/lib.rs` is untouched. `patch.diff` is the whole change against `93a6f37` (11 files); it
  applies to the base (`git apply --cached --check` with the index at `93a6f37`). The worktree is left with HEAD at
  `93a6f37` and the change uncommitted; the temporary WIP commit I used to diff against iteration 2 is dropped.

## The carry-forward, item by item (line numbers are in the patched tree)

### 1. The bug: the client now resolves endpoints itself

`WithoutS3Express` (the previous attempt's wrapper around the SDK's `DefaultResolver`) is gone. In its place is
`PathStyle` (`crates/validate/src/s3.rs:456-487`, impl `:489-500`, wired at `:186`), with two helpers:
`path_style_url` (`:504-539`) and `path_segment` (`:544-555`).

What it does, exactly as the sign-off asked: every request goes to `{scheme}://{authority}{endpoint path without its
final '/'}/{bucket as one percent-encoded segment}`, and a request that names no bucket (ListBuckets) to the endpoint
itself. The SDK's endpoint rules are never consulted, so no bucket name is read. The encode set is the SDK's own
(`aws-sdk-s3` 1.152.0 `endpoint_lib/uri_encode.rs:11-40`: CONTROLS plus every printable ASCII byte outside
RFC 3986's unreserved set, which I checked byte by byte), written out in twelve lines because `percent-encoding` is
not a direct dependency and a new one is out of scope. The SDK joins the operation's path on with one `/`
(`aws-smithy-runtime-api` 1.19.0 `http/request.rs:149-167`).

Signing: the endpoint names no auth scheme, so the SDK picks plain SigV4 for `s3` in the configured region with
each operation's own options: `double_uri_encode = false`, `normalize_uri_path = false`
(`operation/create_bucket.rs:106-110`). That matches what the rules give a path-style bucket on a custom endpoint
(`config/endpoint.rs:1866-1894`: `sigv4`, `signingName s3`, `disableDoubleEncoding true`). S3 Express session auth is
picked only from an endpoint's auth schemes (`s3_express.rs:913-934`), so it can no longer be triggered;
`.disable_s3_express_session_auth(true)` stays as a one-line guard against a later SDK, and its comment now says only
that (`s3.rs:187-191`).

The endpoint is checked as the rules checked it (`endpoint_lib/parse_url.rs:49-62`): parsed with `hyper::Uri`
(`hyper` is already a dependency; `wire.rs` uses `hyper::Uri`), scheme `http` or `https`, no query. Same acceptance
as before, so the existing verbs see no new refusals.

What the sign-off's three reported cases do now (all proven on `http://localhost:{port}` and on the IP endpoint,
against the real gateway through the relay):
- `wyrd-validate--x-s3` / `wyrd-validate--xa-s3` (host-safe): `PUT /<name>/`, `Host: localhost:{port}`, signed
  `/s3/aws4_request`, gateway's own 400 `InvalidRequest`. Before: `PUT /` with `Host: <name>.localhost:{port}`.
- `Wyrd_Validate--x-s3` (not host-safe): same. Before: refused unsent ("S3Express bucket name is not a valid virtual
  hostable name").
- the Outposts alias `test-accessp-o0b1d075431d83bebde8xz5w8ijx1qzlbp3i3kuse10--op-s3` (the SDK's own test name,
  `config/endpoint.rs:6438`): same. Before: resolved to `https://...s3-outposts.us-east-1.amazonaws.com/`, then
  refused by the plain-HTTP connector and reported `NoResponse` with nothing sent.
- `arn:` names, the previous "remaining exception", are now sent too: `PUT /arn%3Aaws%3As3%3A%3A%3Awyrd-validate-probe/`.

### 2. Names refused before sending are `RequestNotBuilt`, never `NoResponse`

With the SDK's rules gone, two kinds of name still cannot be sent. I found both by testing, and both are named in
the docs and tested:
- **Empty or all white space.** Every operation's generated `...EndpointParamsInterceptor` filters
  `bucket.trim().is_empty()` and fails `read_before_execution` with "bucket was missing"
  (`operation/create_bucket.rs:338-343`). That is a `ConstructionFailure`, which `classify` already reports as
  `RequestNotBuilt`. This check is in generated per-operation code and cannot be turned off from config; an empty
  name cannot name a bucket on any S3, so 743.3's bucket-safety rule is unaffected.
- **A URL longer than an `http` URI may be (65,534 bytes, `http` 1.4.2 `uri/mod.rs:145`).** `set_endpoint` refuses
  it while applying the endpoint, before `read_before_transmit`, as a dispatch failure; the existing
  "never reached the connector" flag (`verbs.rs:412`) turns that into `RequestNotBuilt` (red 14 below shows it would be
  `NoResponse` without the flag).

Every other name is one segment. `.` and `..` are sent as those segments (`PUT /s3-prefix/./`, `DELETE
/s3-prefix/../`, tested); what a server does with dot segments is the server's, and the `Probe` doc says so.

The converse, which the sign-off flagged in `answer.rs` ("handed to the connector" = sent): reaching the connector
does not mean anything was sent. The docs now say so (`answer.rs:26-36`, `:429-431`): a refused connection and an
`https` endpoint (the connector speaks plain HTTP only, `wire.rs:103`) send nothing but stay `NoResponse`, as they
are for the older verbs. Both are tested (`https` is new this iteration).

### 3. Comments and docs now match the code

- `crates/validate/src/s3.rs:3-9` (module intro): "at most one request on the wire, to the endpoint it was given",
  and which name shapes the client no longer reads; `:51-54`: what `RequestNotBuilt` covers now.
- `crates/validate/src/s3.rs:456-485`: the `PathStyle` doc, every claim cited to the SDK source.
- `crates/validate/src/s3/verbs.rs:35-47` (`Probe` doc): one request, IP or host name, path then segment, signed
  `s3`; holds for `--x-s3`, `--xa-s3`, Outposts alias, `arn:`; the two exceptions named; dot segments.
- `crates/validate/src/s3/verbs.rs:406-411`: the transmitted-flag comment names what now fails before the connector.
- `crates/validate/src/s3/answer.rs:26-36`, `:389`, `:429-431`: "calls the connector", and the converse.
- `docs/design/architecture/05-building-block-view.md:299`: the client sentence (own endpoint resolution, at most one
  request, to that endpoint) and the bucket-probe passage (IP or host name, the names the rules treat otherwise, the
  two exceptions, the region change). Searched the tree for `WithoutS3Express`, `exactly one request on the wire`,
  `handed to the connector`, `refused by the SDK's endpoint rules`: no match left.

### Effect on the existing verbs (PUT, GET, DELETE)

The resolver is client-wide, as `WithoutS3Express` was. For an ordinary bucket name on an endpoint with no path,
the URL and signature are what the SDK's rules produced; (E) passes unmodified. What changes for them:
- an endpoint with a path and no trailing slash: the rules put the bucket straight after the path
  (`/s3-prefixvalidate/...`, seen in red 6 below); now there is one `/` between. This is a fix.
- the region is only the signing region now. The rules signed `aws-global` as `us-east-1` and refused a region that
  is not a DNS label (`config/endpoint.rs:3206-3218`); now both are signed as given, and the server judges them.
  Named in the `PathStyle` doc and the architecture doc, tested
  (`the_region_is_signed_as_given_whether_or_not_it_could_name_a_host`).
- a configured bucket ending in `--x-s3` etc.: sent like any other.

### Rejected alternatives

- **Keep the SDK's rules and patch their output (the previous design).** `WithoutS3Express` was 24 lines of code and
  still missed three name shapes the sign-off found, plus the path-joining bug above. To fix it in place, the
  wrapper would have to undo the rules' virtual-host URLs, catch the Outposts and ARN branches, and turn their
  refusals into sends, each keyed on how the SDK labels its output. That keeps an SDK decision the client does not
  want and treats its symptoms. `PathStyle` removes the cause in 59 lines of code (`s3.rs:489-555`, comments
  and blank lines not counted, rustfmt line breaks counted) and deletes the 24-line wrapper.
- **Copy the rules' region checks** (`aws-global` → `us-east-1`, DNS-label check): about 10 lines of re-implemented
  SDK logic for a value that is never part of a host name here. Declined and documented. An operator who passes
  `aws-global` against a gateway that expects `us-east-1` gets the gateway's own signature refusal, reported as sent.
- **Refuse `https` at resolution**, so it would be `RequestNotBuilt` instead of `NoResponse`: one line, but it changes
  the existing verbs' error path and the rules accepted `https`. Out of scope; the converse is now documented.

## Failing gates from iteration 2

- **C4 diff coverage** "patch.diff does not apply on origin/main": not a patch defect. The brief requires the #853
  integration line as the base, and the coverage script diffs against `origin/main`. Same as iterations 1 and 2.
- **C5 surviving mutants: 2 missed** — `Capture::name` → `""` / `"xyzzy"` (`answer.rs`). Unchanged and equivalent:
  the name is an interceptor label the SDK shows only when that interceptor's hook fails, and every `Capture` hook
  returns `Ok`. I did not add them to `.cargo/mutants.toml`'s `exclude_re` (repo tooling, outside this slice).

## Red → green

Verification posture (a) net-new, as the brief pre-declares: with production reverted to the base the test file
cannot build, so C4-verify reports UNVERIFIABLE (77).

Green: `cargo test -p wyrd-validate --test s3_client_smoke_verbs` → `23 passed; 0 failed`, about 1.7 s.
**(E):** `cargo test -p wyrd-validate` green, the three named files byte-identical to the base (`git diff 93a6f37`
is empty for them): `s3_client_roundtrip` 15, `s3_client_upload_peers` 12, `s3_client_nonconforming_responses` 28
(plus `cli_surface` 21, `payload_and_sizes` 26, lib unit 4).

**Reds on the previous attempt's code.** Before writing the fix, I ran the new legs on the iteration-2 tree
(this iteration's test file, v2's production). Three tests failed; the bucket-name test listed 22 failing cases,
including exactly the ones the sign-off reported (`PUT /` with `host: wyrd-validate--x-s3.localhost:…`; `Wyrd_Validate--x-s3`
refused; the Outposts alias sent to `https://…amazonaws.com/` and reported `NoResponse`, `sent []`).

**Demonstrated reds.** Each is one edit to production, the filtered test run, then the file restored and checked
byte for byte, by a helper script in `$PDCA_SCRATCH/pdca-builder-938-reds/` (removed with the rest of my scratch
files when done). Every one exited 101 with the named test failing and no build error. The five the brief requires, re-run against this iteration's test file:
1. *Only the timeout when a body stalls after a 2xx* (`probe` turns `BodyFailed(Timeout)` into `Err(Timeout)`) →
   (C) RED: `22 case(s) lost or misreported the 200: CopyObject Stall: the 200 head was lost: Timeout { phase:
   BodyIdle, limit: 500ms }`.
2. *Accept a listing cut short* (`listing_page` returns an empty final page for an unreadable document) → (D) RED:
   `6 case(s) not refused as they must be: ListV2 cut-short: not refused as Unreadable: Listed([])`.
3. *Let the SDK's bucket-name handling refuse or rewrite the name.* (a) The SDK's `DefaultResolver` wired in place
   of `PathStyle` → (B) RED, `42 case(s) not sent as one path segment`, e.g. `wyrd-validate--x-s3` not signed for
   `s3`. (b) Rewrite (`trim '-'` + lower case) → (B) RED: `reached the gateway as PUT /wyrd-validate-probe/, not PUT
   /wyrd-validate-probe-/`.
4. *Stop listing after the first page* → (A) RED against the real gateway: `left: [("list/a", 0), ("list/b", 1)]`.
5. *Drop the `<Error>` rows* → (F) RED: `left: [Deleted { key: "mixed/a" }, Deleted { key: "mixed/c" }]`.

For this iteration's change:
6. *The previous attempt's whole `s3.rs`* (`git show HEAD:…`, `WithoutS3Express`) → 4 tests RED, 19 pass: the
   bucket-name test (22 cases), the endpoint-path test (`PUT /s3-prefixwyrd-validate-probe-/`), the region test
   (`aws-global` not signed as given), the stopped-before-sending test (the rules' reason is "Invalid region", not
   the query).
7. *No percent-encoding* → RED: `wyrd validate` refused unsent ("failed to apply endpoint `…/wyrd validate`").
8. *`-` encoded too* → RED: `PUT /wyrd%2Dvalidate%2Dprobe%2D/`.
9. *Endpoint path's trailing slash kept* → RED: `/s3-prefix//wyrd-validate-probe-/`.
10. *No-bucket URL drops the endpoint path* → RED: ListBuckets at `GET /`, not `/s3-prefix/`.
11. *Endpoint query accepted* → RED: ListBuckets on `…/?x=1` was sent and answered.
12. *Any scheme accepted* → RED: `ftp://` became `NoResponse` from the connector, not `RequestNotBuilt`.
13. *`https` refused at resolution* → RED: the `https` leg became `RequestNotBuilt`.
14. *The transmitted flag ignored* → RED: the 70,000-byte name came back `NoResponse`.

## Mutation testing (C5, advisory)

Run as the C5 gate runs it (`cargo mutants --in-diff … --no-shuffle --cap-lints=true`, plus `-j 2` and a scratch
`TMPDIR`/`--output`), on this iteration's `s3.rs` diff (the only production file with code changes this round;
`verbs.rs` and `answer.rs` changed only in comments): **10 mutants, 5 caught, 2 unviable, 3 timeouts, 0 missed.**

- Caught: `path_style_url` → `Ok(String::new())`; the scheme match guard → `true`; `path_segment` → `String::new()`
  and → `"xyzzy"`; `||` → `&&` in `path_segment`.
- Unviable: `S3Client::with_deadlines` and `PathStyle::resolve_endpoint` → `Default::default()` (no `Default`).
- Timeouts: `path_style_url` → `Ok("xyzzy")`, the scheme guard → `false`, and its `||` → `&&`. Each makes every
  request fail before it is sent. They are failing, not surviving: in the `||` → `&&` log,
  `s3_client_nonconforming_responses` already shows four tests FAILED when the 59 s auto-timeout stops the run;
  that file's tests wait on their scripted endpoints before they report. cargo-mutants does not count a timeout as
  missed.

I did not rerun the whole-patch sweep (about 21 minutes in iteration 2). The rest of the production diff is
unchanged from iteration 2 apart from comments, so I expect Check's C5 to report iteration 2's two equivalent
`Capture::name` survivors and nothing new; that is an expectation, not a measurement.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted to the base, the test does not build (the pre-declared UNVERIFIABLE).
  Reverted to the previous attempt's production, the four tests above fail, reproducing the sign-off's bug report
  line for line. Each of the fourteen single edits above turns a named assertion red; none stayed green.
- **(b) Production path? Yes.** Every leg builds the client through the production `resolve_config` and
  `S3Client::with_deadlines`, so it runs the production `PathStyle` inside the real `aws-sdk-s3` orchestrator
  (its real param interceptors, signer and endpoint application), the client's own `WireClient`, over real loopback
  TCP. (A)/(B) hit the real `S3Gateway` + `wyrd_server::Gateway`; the relay only forwards bytes and records request
  heads. No mock of the client, the SDK or the resolver.
- **(c) Fixture includes the fault? Yes.** The host-name endpoint is a real `http://localhost:{port}` URL resolved
  by the connector; the S3 Express, Outposts-alias and ARN names are the real shapes the SDK's rules act on (the
  Outposts name is the SDK's own test name); the endpoint paths, query, `ftp` and `https` endpoints, the empty,
  white-space and 70,000-byte names and the two regions are all real configuration passed through the production
  parser; "nothing was sent" is checked by the relay's connection count, not assumed.

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean.
- `python3 docs/publishing/tools/lint_docs.py`: OK. `render_site.py --check` (out dir in scratch): link audit OK.
- `cargo xtask ci-dry-run` (every repo-hygiene guard), `cargo xtask blackbox-guard`, `cargo xtask statics`: pass.
- `cargo doc -p wyrd-validate` (also with `--document-private-items`): no new warning; the one at
  `crates/validate/src/sizes.rs:340` is on the base.
- I did not run the full `cargo xtask ci` (workspace build/test incl. DST); Check's C4-ci runs it.

## External dependencies

`typos` and `docs-renderer` (`markdown_it` + `yaml`), as the brief lists: both present and run. Nothing unlisted
was needed: no Docker, no network beyond `127.0.0.1` (`localhost` resolves to loopback). No NEEDS-HUMAN
external-dependency item.
