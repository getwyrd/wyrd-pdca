<!-- pdca:split-proposal v1 -->
# Split proposal — issue 938

Intake cap (wyrd-pdca-P1), track `blackbox`, `scripts/plan-cap --track blackbox --need 2` run in
the Plan session on 2026-10-23: `planned 3/6 (cap, track blackbox) — room for 3, need 2: Plan
intake open` (937 ITERATE_DO; 938, 939 PLANNED). No override needed.

<!-- Each child is a COMPLETE brief, not a sketch. A filled Slug alone makes `state.state`
     classify the materialised file as PLANNED, so `pdca flow` skips Plan and sends it
     straight to Do — a child missing `Repo + branch target` then builds successfully and
     has nowhere to publish. Fill every field. -->

<!-- Delimiters are HTML comments, not headings, DELIBERATELY: each child body is a full
     draft brief and may contain arbitrary headings and fenced code blocks, so anything
     that could also appear inside a child cannot be its boundary. `pdca split --accept`
     parses these markers; keep them exactly as written. -->

## Why this slice is oversized

Two children, on the seam the sign-off of iteration 3 named: **reads and listings** (child-1)
and **deletes and bucket probes** (child-2), child-2 stacked on child-1. Nothing in the brief is
wrong; three rounds grew the patch 131 → 192 → 208 KB against a 100 KB backstop and no reviewer
found a defect, so the split is by ownership of the salvage's hunks, not by re-design.

The slice holds two shippable outcomes that share one core:

1. **The reply-capture core plus the verbs that only read.** `s3/answer.rs` (the interceptor
   that takes the body from the SDK, `Head`/`Answer`/`Outcome`/`BodyFailure`, the bounded
   `read_whole`), the strict document reader in `s3/response.rs` (`whole_document`,
   `text_only`, `named_code`), listing-page reading in `s3/document.rs`, and in `s3/verbs.rs`
   `head_object`, `get_object_range`, `get_object_if_none_match`, `list_objects_v2`,
   `list_objects_v1` with the private `captured`/`read`/`answer`/`fetched`/`page` core. This
   ships on its own: every criterion leg it owns — (A) reads and listings, (C) on a listing
   page, (D) for listings, (G)'s listing and header legs, (E) — is checkable against the real
   gateway and a scripted endpoint without a single probe or delete existing.
2. **Bulk delete, the probes, and the client's own endpoint resolution.** `delete_objects` and
   `delete_rows`/`DeleteResult`/`DeleteRow`/`KeyError`, the `Probe` enum and `S3Client::probe`,
   `not_built`, and the `PathStyle` resolver in `s3.rs` (`path_style_url`, `path_segment`, the
   `.endpoint_resolver(PathStyle)` / `.disable_s3_express_session_auth(true)` wiring). This is
   where every finding of iterations 1–3 landed ((B) routing, the `<Error>`-in-200 case, (F),
   (H)), and it is the only half that changes the existing verbs' behaviour (the endpoint path
   joined with one `/`, the region signed as given). It reports through child-1's core, which
   is why it stacks rather than runs beside.

The two halves do not interleave in the salvage: `verbs.rs` splits at the `delete_objects`
doc comment (patch line ~1700) and at `not_built`; `document.rs` splits at `delete_rows`
(patch line 1103) with the `fields`/`decimal` helpers staying with the listing reader;
`s3.rs` splits at the `PathStyle` doc comment (patch line 209). Only the module-doc
paragraph of `s3.rs` and the `pub use` lines are touched by both, and child-2 extends what
child-1 wrote.

Not a third child: the listings alone could be cut out of child-1 (the `document.rs` listing
reader, the two `list_objects_*` verbs, their ~700 test lines) to get child-1 under the 100 KB
backstop, but that is a third full cycle for ~40 KB, and the sign-off asked for two. The
estimate below says which child will likely trip the backstop so the size item is expected
rather than a surprise.

## Wave sketch

**Wave 1: child-1 alone (it needs nothing).** It builds on the integration line's current tip
(`main` 955b9cb equivalent: #853 and #744 are in), touches no file 743.1 (#937, now in
ITERATE_DO) touches, and does not read `crates/validate/src/lib.rs`. The `wyrd-traits`
dev-dependency (the bucket marker a listing needs) lands here, with `Cargo.lock`.

**Wave 2: child-2, `Depends on: child-1`.** Every delete and probe is reported through
child-1's `Capture`/`read_whole`/`answer` core and the `whole_document` reader; `delete_rows`
uses child-1's `fields` and `named_code`; the probe tests use child-1's gateway fixture and
scripted endpoint and add only the relay. Building it blind beside child-1 would mean
re-deriving ~900 lines of core in both and merging two copies of `verbs.rs`. The wave fold
gives child-2 child-1's accepted diff, so child-2's patch is only its own hunks.

**No `Conflicts with`.** The dependency already keeps them out of one wave; there is no
third child to keep apart.

**Expected sizes (from the salvage's hunk counts, not measured):** child-1 ≈ 65 KB production
+ ≈ 50 KB tests ≈ 115 KB — it will likely raise the 100 KB size item at Check; that item is
"a coherent large change", not a defect, and the human clears it as the pdca.toml note on the
backstop describes. Child-2 ≈ 20 KB production + ≈ 45 KB tests + ≈ 25 KB for the one-line
architecture paragraph (old + new line) ≈ 90 KB.

**743.3 (#939) consumes both.** Its `Depends on` names 938 today, and the driver does not
re-point a dependent of a split parent at its children (`src/pdca_harness/flow.py:1615-1619`):
left as is, #939 could share a wave with them. Decided in the Plan session (2026-10-23, human):
right after `--accept`, #939's `Depends on` is edited from `937, 938, 744, 853` to
`937, <child-1 id>, <child-2 id>, 744, 853`.

<!-- pdca:child child-1 -->
# Brief — issue <id> / validate-s3-client-reads-and-listings

> Do reads ONLY this file. First of two children of #938 (itself 743.2, child of #743). It
> builds the reply-capture core the S3 client needs beyond PUT/GET/DELETE and the verbs that
> only read: HEAD, ranged GET, conditional GET, ListObjectsV2 and ListObjects. It decides
> nothing about whether an answer is right (743.1 judges, 743.3 runs). child-2 (bulk delete,
> probes, endpoint resolution) builds on this one.
>
> **Salvage is the starting point, not a hint.** `results/issue_938/iteration-v3/patch.diff`
> was built three times, reviewed adversarially and found without defect; it was split for
> size. Take this child's hunks from it as they are; do not re-derive the design.

- **Slug:** validate-s3-client-reads-and-listings
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement
- **Defect:** the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on `main` 955b9cb). Smoke (743.3) also
  needs HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a
  prefix, followed across pages. Each must report what the server actually sent (status, every
  header field line as sent, the S3 error code, the body) with #853's bounded, typed reading.
  A 2xx head must never be lost: when a body fails after a 2xx head, the report still carries
  the 2xx. The SDK deserializes these answers itself, so today a listing cut short, a body that
  stalls after a 200, or a 304 with no body cannot be reported as what the server sent.
- **Success criterion:** the NEW test file `crates/validate/tests/s3_client_smoke_verbs.rs`
  passes, against an in-process Wyrd gateway and, for the fault legs, a scripted loopback
  endpoint (peers under *Citations*):
  - **(A) The read verbs against the real gateway.** HEAD of a written key → 200 with its size
    and `ETag`; of a missing key → 404 with no body (`ErrorCode::NoBody`). Ranged GET
    `bytes=a-b` → 206, the `Content-Range` header as sent, exactly those bytes. GET with
    `If-None-Match: <its ETag>` → 304, no body; with a different valid tag → 200 and the whole
    body. ListObjectsV2 and v1 under a prefix → every key with its size, across more than one
    page (the client follows the continuation).
  - **(C) A 2xx head survives its body, shown on a listing page.** A scripted endpoint answers
    a ListObjectsV2 request with a `200` head and then (i) stalls the body past the body-idle
    deadline, (ii) trickles it past the whole-body deadline, (iii) cuts the connection
    mid-body, (iv) sends a body that is not a `ListBucketResult`. Short deadlines via
    `S3Client::with_deadlines`. In each case the result carries status 200 and the body
    failure beside it, never only a timeout or a transport error.
  - **(D) Unreadable listing pages are refused as #853 refuses an error document.** A page
    that is cut short, followed by junk or a second document, followed by bytes past its
    `Content-Length`, or past `BUFFERED_BODY_BUDGET` (with a `Content-Length` and chunked) is a
    typed refusal that keeps the 200, never a partial listing. A later page that fails discards
    the earlier good pages (no partial listing). A page of exactly the budget is read. A
    `<Size>` that is not plain decimal digits (`+7`, a space, a fraction, past `u64`) is refused.
  - **(E)** `s3_client_roundtrip.rs`, `s3_client_upload_peers.rs` and
    `s3_client_nonconforming_responses.rs` pass unmodified.
  - **(G) Listing continuation and headers are pinned, not only the failures.** Against the
    scripted endpoint: `ListObjects` (v1) follows a `<NextMarker>` that differs from the last
    key, and the last key where a truncated page names none; a truncated page that names
    nothing to continue from is refused; a listing still truncated after `MAX_LISTING_PAGES`
    stops there with a typed result; a header sent twice comes back as both lines, a header
    that is not text included; a 304 to a conditional GET that frames a chunked body is not
    refused (a GET's or HEAD's 304 may carry framing headers with no body).
- **Falsifiability:** RED is reachable on `cargo test -p wyrd-validate --test
  s3_client_smoke_verbs`, which `cargo xtask ci` also runs: a real gateway on loopback for
  (A), a scripted endpoint with short deadlines for (C)/(D)/(G). A client that drops the 2xx
  head on a body failure, stops listing after one page, or accepts a cut-off page fails an
  assertion. No Docker, no network beyond `127.0.0.1`. Every leg was shown red and green on
  iteration 3's code (`results/issue_938/iteration-v3/build-notes.md` "Red → green").
- **Invariant to restore:** what the client reports is what the server sent, from the request
  the caller asked for. No answer is upgraded, none downgraded, nothing filled in on the
  server's behalf (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never
  erases the 2xx (proposal 0017 §7, the failure taxonomy the typed errors serve).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality; the verbs do not exist on the base.
- **Ordering note:** independent of child-2 and runs first; child-2 reports through this
  child's core. Base: `main` 955b9cb (#853 PR #934 and #744 PR #931 merged; `s3/response.rs`,
  `s3/wire.rs`, `roxmltree` under `[dependencies]` at `crates/validate/Cargo.toml:44`); in the
  live run the integration line `pdca-integration/main-r42c5348f203d`. Runs in parallel with
  743.1 (#937) and does not depend on it; **must not touch `crates/validate/src/lib.rs`**
  (743.1 adds a line there, 743.3 rewrites it); new types are reachable as
  `wyrd_validate::s3::…`. **Salvage hunks this child owns** (iteration-v3 `patch.diff`):
  `Cargo.lock` + `crates/validate/Cargo.toml` (`wyrd-traits` dev-dependency); `s3.rs` module
  wiring (`mod answer; mod document; mod verbs;`, the `pub use` lines for `answer`, `document`,
  `error`, the `Deadlines` doc, `object_length`) but NOT the `PathStyle` resolver, its imports
  or the two builder lines `.endpoint_resolver(PathStyle)` /
  `.disable_s3_express_session_auth(true)`; `s3/answer.rs`, `s3/body.rs`, `s3/error.rs`,
  `s3/response.rs` whole; `s3/document.rs` without `DeleteResult`/`DeleteRow`/`KeyError`/
  `delete_rows`; `s3/verbs.rs` without `EMPTY_PART_ETAG`, `Probe`, `delete_objects`, `probe`,
  `not_built`. In the test file: the gateway fixture, the scripted `Endpoint`/`Script`/`serve`,
  the listing builders and the tests under *Test file* — not the `Relay`, the probe cases or the
  delete tests. The `s3.rs` module doc describes only what this child ships; child-2 extends it.
- **Scope:** the reply-capture core and the read verbs in `crates/validate/src/s3.rs` (wiring
  only) and `crates/validate/src/s3/{answer,document,response,verbs,body,error}.rs`;
  `wyrd-traits` under `[dev-dependencies]`; the new test file. / out of scope: bulk delete,
  the probes, the endpoint resolution, and the architecture doc (child-2); judging answers
  (743.1); the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart
  success path (#508, #750); any change to the existing verbs' behaviour; any `wyrd-*` crate
  in `[dependencies]`; any new production dependency; `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/validate/tests/s3_client_smoke_verbs.rs` (NEW). From the salvage:
  `head_reports_a_written_keys_size_and_etag_and_a_missing_keys_bare_404`,
  `a_ranged_get_is_206_with_its_content_range_and_exactly_those_bytes`,
  `a_conditional_get_is_304_on_its_etag_and_the_whole_object_on_another`,
  `both_listings_return_every_key_under_a_prefix_with_its_size_across_pages`, the listing
  legs of `listing_and_delete_result_documents_that_are_not_whole_are_refused_never_partial`,
  `a_listing_whose_later_page_is_unreadable_reports_no_listing_at_all`,
  `a_truncated_listing_page_that_names_nothing_to_continue_from_is_refused`,
  `a_head_reports_every_header_line_a_repeated_one_and_one_that_is_not_text_included`,
  `list_objects_v1_follows_a_next_marker_and_the_last_key_where_a_page_names_none`,
  `a_listing_still_truncated_after_max_listing_pages_pages_stops_there`,
  `a_listing_size_that_is_not_plain_decimal_digits_is_refused`.
  New here (two tests the salvage does not have; its production already behaves so):
  (C) on a listing page — the salvage's `BodyFault`/`fault_script` take a `Probe` only to build
  the cut document (`cut_document`, iteration-v3 patch line 3363); give them a
  `ListBucketResult` fragment instead and send a ListObjectsV2 request. And (G)'s 304 leg — the
  salvage's `a_304_that_frames_a_chunked_body_…` test drives probes only; here a `304` with
  `Transfer-Encoding: chunked` answers `get_object_if_none_match`, which captures as a GET
  (`Capture::new(true)` in the salvage's `verbs.rs`), so the framing is not refused.
- **Verification posture:** (a) net-new. The verbs do not exist on the base, so with production
  reverted the test cannot compile and C4-verify reports UNVERIFIABLE (exit 77): pre-declared.
  Built and exercised at Check: every read verb against the real gateway, every fault path
  against the scripted endpoint. **Demonstrated reds (required, in `build-notes.md`):**
  (1) return only the timeout when a body stalls after a 2xx head → (C); (2) accept a listing
  document cut short → (D); (4) stop listing after the first page → (A).
- **Production reach:** consumed by child-2 (every delete and probe reports through this
  core) and by 743.3 (#939) in this same run; until then only the tests call the new verbs.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, response reading:** the existing verbs' `.customize()` hooks
    (`crates/validate/src/s3.rs:244`, `:288`, `:330`) and `S3Client::classify` (`:339`); the
    strict document reader `error_document` (`crates/validate/src/s3/response.rs:363`) and the
    `ResponseGuard` interceptor (`:209`, `impl Intercept` `:231`). Read listings by the same
    rules (UTF-8, one well-formed document under the expected root, bounded depth and size).
  - **Peer, in-process gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323`. A listing needs the bucket's marker
    seeded first (ADR-0046 decision 1): do it as `seed_bucket` does,
    `crates/server/tests/s3_list_objects.rs:82-88`, before the store goes into `Gateway::new`.
  - **Peer, faults:** the scripted endpoint
    `crates/validate/tests/s3_client_nonconforming_responses.rs:354` (`serve` `:489`, `head()`
    `:247`, `error_doc()` `:220`).
  - **Deadlines:** `Deadlines` `crates/validate/src/s3.rs:81-96`, defaults `:98-107` (body idle
    60 s, whole body 15 min): use `S3Client::with_deadlines` for (C).
  - **Salvage:** `results/issue_938/iteration-v3/patch.diff` and its rationale
    `results/issue_938/iteration-v3/build-notes.md`.
- **Prior-art check (triage cycles):** searched by path `crates/validate/src/s3.rs`,
  `crates/validate/src/s3/` (2026-10-23): merged #859, #865, #934 (#853); open: #942 (stacked
  on the integration line only); closed-unmerged: none. Rejected attempts are the parent's
  own `results/issue_938/iteration-v1..v3` (no defect in v3; split for size). Of their
  findings only (G)'s listing legs touch this child; the rest are child-2's.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# Brief — issue <id> / validate-s3-client-deletes-and-bucket-probes

> Do reads ONLY this file. Second of two children of #938 (743.2, child of #743). On child-1's
> reply-capture core it adds bulk `DeleteObjects`, one `Probe` per operation the matrix may
> declare unsupported, and the client's own endpoint resolution (743.1 judges, 743.3 runs).
>
> **Salvage is the starting point, not a hint.** `results/issue_938/iteration-v3/patch.diff`
> was built three times, reviewed adversarially and found without defect; it was split for
> size. Take this child's hunks as they are; do not re-derive the design.

- **Slug:** validate-s3-client-deletes-and-bucket-probes
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement
- **Defect:** with child-1 in, the client still cannot send bulk `DeleteObjects` or one request
  for each operation the matrix may declare unsupported. And a probe does not reach the
  endpoint it was given with the bucket name it was given: the SDK's endpoint rules reroute
  or refuse some names (S3 Express `--x-s3` / `--xa-s3`, Outposts-alias `--op-s3`, `arn:`)
  before anything is sent — exactly what 743.3's bucket-safety rule cannot tolerate (a
  `DeleteBucket` probe must name a bucket S3's rules forbid, so it can never delete one it
  does not own). A mixed `<DeleteResult>` has no evidence of a failed key but its `<Error>`
  rows (the gateway answers `200` either way, `crates/gateway-s3/src/lib.rs:2293-2296`).
- **Success criterion:** the tests this child adds to `crates/validate/tests/s3_client_smoke_verbs.rs`
  pass, against the in-process gateway and, for the fault and routing legs, a scripted loopback
  endpoint or relay:
  - **(A) Bulk delete against the real gateway.** DeleteObjects of N existing keys → every key
    under `<Deleted>`, no `<Error>` row; the keys are then gone (HEAD 404). Error rows are (F)'s.
  - **(B) Every unsupported operation reaches the gateway** and is reported with status and S3
    code: `CopyObject`, the six multipart operations (`Probe` names them) and
    `GetBucketLocation` 501 `NotImplemented`; `CreateBucket`, `DeleteBucket`, `ListBuckets`
    400 `InvalidRequest`; `HeadBucket` 400 with no body. **Bucket names reach the endpoint as given**, on an IP
    endpoint (`http://127.0.0.1:{port}`) AND a host-name endpoint (`http://localhost:{port}`):
    `CreateBucket` / `DeleteBucket` naming a bucket S3's rules forbid (one ending in `-`), a
    host-safe and a non-host-safe `--x-s3` name, an `--xa-s3` name, an Outposts-alias-shaped
    `--op-s3` name and an `arn:` name each arrive as ONE request, `PUT|DELETE /<name>/`, the
    name one percent-encoded path segment after the endpoint's own path, `Host:` the
    configured endpoint, signed for `s3` in the configured region. `ListBuckets` addresses the
    endpoint itself. A name that cannot be sent at all is `RequestNotBuilt` (nothing sent),
    never `NoResponse`; a request that reached the connector and got no answer stays
    `NoResponse`. The two unsendable names (empty or all white space; a URL past an `http`
    URI's 65,534 bytes) are named in the code's docs and tested.
  - **(C) A 2xx head survives its body, on every probe that can carry one.** A scripted
    endpoint answers `200` and then (i) stalls the body past the body-idle deadline, (ii)
    trickles it past the whole-body deadline, (iii) cuts the connection mid-body, (iv) sends a
    body that is not the operation's success document. Each report carries status 200 and the
    body failure beside it. A `200` whose body is an S3 `<Error>` document (S3 does this for
    `CopyObject` / `CompleteMultipartUpload`) keeps the 200 AND reports that document's
    `<Code>` and `<Message>` as typed fields.
  - **(D) An unreadable `<DeleteResult>` is refused** by the rules child-1 applies to a listing
    page (cut short, followed by junk or a second document, past `Content-Length`, past
    `BUFFERED_BODY_BUDGET`): a typed refusal that keeps the 200, never a short delete result.
    A document of exactly the budget is read.
  - **(E)** The three existing `s3_client_*` test files pass unmodified; child-1's tests still
    pass.
  - **(F) A mixed `<DeleteResult>` is reported row by row.** A scripted endpoint answers
    `POST ?delete` with `200`, two `<Deleted>` rows and one `<Error>` row (`<Key>`,
    `<Code>AccessDenied</Code>`, `<Message>`). The result lists exactly those two deleted keys
    and exactly one error row with that key, code and message as sent, in document order.
  - **(G) The success paths are pinned.** Against the scripted endpoint: a `200` with each
    probe's whole, correct success document (or no body where its success has none) is a
    success; a 304 that frames a chunked body is refused for a probe that is not a GET or HEAD.
  - **(H) The living architecture doc matches the code.** The `validate` paragraph,
    `docs/design/architecture/05-building-block-view.md:299` on `main` (`:301` on the
    integration line), describes the new verbs — child-1's and this child's — the 2xx head
    kept beside a body failure, listing continuation and its page cap, bulk-delete rows, the
    buffered budget's reach, and the endpoint resolution with its named exceptions and the
    two region changes. No sentence in it or in the code's docs claims more than the tests
    show. The salvage's new paragraph is the text to carry over.
- **Falsifiability:** RED is reachable on `cargo test -p wyrd-validate --test
  s3_client_smoke_verbs` (`cargo xtask ci` runs it too): a real gateway on loopback for
  (A)/(B), a scripted endpoint or relay with short deadlines for (B)-routing/(C)/(D)/(F)/(G).
  A client that lets the SDK reroute or refuse a bucket name, reports an unsent request as
  `NoResponse`, drops a `<DeleteResult>`'s error rows, or loses a 200 under a stalled probe
  body fails an assertion. No Docker, no network beyond loopback. Every leg was shown red and
  green on iteration 3's code (`results/issue_938/iteration-v3/build-notes.md`, reds 3, 5, 6–14).
- **Invariant to restore:** what the client reports is what the server sent, from the request
  the caller asked for: no request rewritten, rerouted or silently dropped on the way out, so
  "the server refused it" and "it was never sent" are never confused; nothing filled in on the
  server's behalf (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never
  erases the 2xx (proposal 0017 §7).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality; the verbs and the resolver do not exist on the
  base.
- **Depends on:** child-1
- **Ordering note:** stacks on child-1: every delete and probe reports through child-1's
  `Capture`/`read_whole`/`answer` core, `delete_rows` uses its `whole_document`/`fields`/
  `named_code`, and the tests use its gateway fixture and scripted endpoint; the wave fold
  hands child-1's accepted diff over. Independent of 743.1 (#937); **must not touch
  `crates/validate/src/lib.rs`**. 743.3 (#939) consumes this child and, through it, child-1.
  **Salvage hunks this child owns** (iteration-v3 `patch.diff`): in `s3.rs` the `PathStyle`
  resolver (patch lines 209–308), its imports, the two builder lines, the `Probe` and
  delete-result re-exports and the module-doc sentences on them; in `s3/document.rs`
  `DeleteResult`, `DeleteRow`, `KeyError`, `delete_rows`; in `s3/verbs.rs` `EMPTY_PART_ETAG`,
  `Probe`, `delete_objects`, `probe`, `not_built`; the architecture paragraph whole; in the
  test file the `Relay`, the probe and bucket-name tables, the probe fault/success scripts,
  the delete-result builder and the tests under *Test file*. The existing verbs change only as
  the shared resolver implies — an endpoint path joined with one `/`, the region signed as
  given — each named in the `PathStyle` doc and the architecture doc.
- **Scope:** bulk delete, the probes and their typed results in `crates/validate/src/s3/*`;
  the endpoint resolution in `crates/validate/src/s3.rs`; the tests added to the test file;
  the `validate` paragraph of `docs/design/architecture/05-building-block-view.md`. / out of
  scope: child-1's core and read verbs beyond extending their docs; judging answers (743.1);
  the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart success
  path (#508, #750); any change to the existing verbs beyond what the shared resolution
  implies; any `wyrd-*` crate in `[dependencies]`; any new production dependency;
  `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/s3_client_smoke_verbs.rs` (child-1's file, extended).
  This child's regression, from the salvage, by prefix:
  `delete_objects_reports_every_key_deleted…`, `every_unsupported_operation_reaches…`,
  `bucket_probes_send_every_name_as_one_path_segment…`, `an_endpoint_path_comes_before…`,
  `a_request_stopped_before_it_reaches_the_connector…`, `a_probe_that_reached_the_connector…`,
  `the_region_is_signed_as_given…`, `a_probe_answered_200_keeps_its_200…`, the
  `<DeleteResult>` legs of `listing_and_delete_result_documents_that_are_not_whole…`,
  `a_mixed_delete_result_is_reported_row_by_row…`, `a_2xx_carrying_the_operations_whole_s3_success…`,
  the probe leg of `a_304_that_frames_a_chunked_body…`,
  `a_200_whose_body_is_an_s3_error_document…`.
- **Verification posture:** (a) net-new. The verbs and the resolver do not exist on child-1's
  tree, so with this child's production reverted the test cannot compile and C4-verify reports
  UNVERIFIABLE (exit 77): pre-declared. Everything is built and exercised at Check against the
  real gateway, the scripted endpoint or the relay. **Demonstrated reds (required, in
  `build-notes.md`):** (3) put the SDK's default endpoint resolution back → (B); (5) drop a
  `<DeleteResult>`'s `<Error>` rows → (F); (6) report a request refused before sending as
  `NoResponse` → (B); (1) return only the timeout when a probe's body stalls after a 2xx → (C).
- **Production reach:** consumed by 743.3 (#939) in this same run; until then only the tests
  call the new verbs. The resolver is on every existing verb's live path from here on ((E)).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, the core this child reports through** (on the integration line after wave 1):
    child-1's `Capture`/`read_whole`/`answer` (`crates/validate/src/s3/answer.rs`),
    `captured`/`read`/`answer` (`s3/verbs.rs`), `whole_document`/`named_code`
    (`s3/response.rs`), `fields` (`s3/document.rs`).
  - **Peer, fixtures:** child-1's gateway fixture and scripted `Endpoint` in the test file; the
    relay's `Downstream` modes, `crates/validate/tests/s3_client_roundtrip.rs:465-474`.
  - **The resolver's SDK citations and rejected alternatives:**
    `results/issue_938/iteration-v3/build-notes.md` §1–2 and "Rejected alternatives" (every
    claim in the `PathStyle` doc is cited to `aws-sdk-s3` 1.152.0 source there).
- **Prior-art check (triage cycles):** searched by path `crates/validate/src/s3.rs`,
  `crates/validate/src/s3/` (2026-10-23): merged #859, #865, #934 (#853); open: #942 (stacked
  on the integration line only); closed-unmerged: none. Rejected attempts are the parent's
  own `results/issue_938/iteration-v1..v3` (v3: no defect; every v1/v2 finding is in (B),
  (C), (G), (H) above). The bucket-safety rule (B) serves is #743's
  (`results/issue_743/review-batch.md:3`); which forbidden name the probes use is 743.3's to
  choose (`results/issue_743/split-proposal.md:495`).
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->
