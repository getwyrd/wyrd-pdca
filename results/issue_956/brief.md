# Brief — issue 956 / validate-s3-client-deletes-and-bucket-probes

> Do reads ONLY this file. Second of two children of #938 (743.2, child of #743). On #955's
> reply-capture core it adds bulk `DeleteObjects`, one `Probe` per operation the matrix may
> declare unsupported, and the client's own endpoint resolution (743.1 judges, 743.3 runs).
>
> **Salvage is the starting point, not a hint.** `results/issue_938/iteration-v3/patch.diff`
> was built three times, reviewed adversarially and found without defect; it was split for
> size. Take this child's hunks as they are; do not re-derive the design. The human's sign-off
> of that iteration: "The reviewer found no defect in the patch, and the code is worth keeping
> as the starting point for the children"
> (`results/issue_938/iteration-v3/session-carry-forward`).
>
> **Where the salvage is (revised 2026-10-23).** On the harness host, at the absolute path
> `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/patch.diff` (4,461 lines, sha256
> `b50cc4f5a2bc116fd8e64938a7dc903692148f4ddecbb5f96bcd00df0f8d7d69`) with its rationale
> `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/build-notes.md` (sha256
> `f2c4fcf95040204e7e3eb3542c17a5bd4c515063df8e7e5d23ba8cf16bb632f7`). The harness root is the
> Do leaf's working directory. Both files are untracked in the harness repo today, so a
> reviewer's sandbox and the target checkout do not have them. Check both hashes first. If a
> file is missing or its hash differs, stop: say so in `build-notes.md` as an item for the
> human and ship no patch. Do not re-derive the design from this brief.
>
> **The bucket-safety rule (B) serves**, as the human's review of #743's first attempt put it
> (`results/issue_743/review-batch.md:3`): "DeleteBucket is sent without proving ownership of
> the scratch bucket, so an unexpectedly supported operation can delete a pre-existing bucket,
> including `--bucket` when it matches the derived name." 743.3 (#939) answers it with a
> forbidden probe name plus an absence check; this child's part is that a name reaches the
> endpoint exactly as given.

- **Slug:** validate-s3-client-deletes-and-bucket-probes
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** with #955 in, the client still cannot send bulk `DeleteObjects` or one request
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
    400 `InvalidRequest`; `HeadBucket` 400 with no body. **Each probe is also checked as the
    request it sent** (added 2026-10-23: the gateway answers every recognised bucket
    subresource from one `501` branch and every object subresource from another,
    `crates/gateway-s3/src/lib.rs:1797-1802`, `:1836-1847`, so the answer alone cannot tell
    which operation was sent). Through the relay, exactly one request per probe, whose method,
    path, operation parameters and distinguishing headers are those of its operation:
    `CopyObject` `PUT /<b>/<key>` with `x-amz-copy-source: <b>/<source>`;
    `CreateMultipartUpload` `POST /<b>/<key>?uploads`; `UploadPart` `PUT
    /<b>/<key>?partNumber=1&uploadId=<id>`; `CompleteMultipartUpload` `POST
    /<b>/<key>?uploadId=<id>`; `AbortMultipartUpload` `DELETE /<b>/<key>?uploadId=<id>`; `ListParts`
    `GET /<b>/<key>?uploadId=<id>`; `ListMultipartUploads` `GET /<b>/?uploads`;
    `GetBucketLocation` `GET /<b>/?location`; `CreateBucket` `PUT /<name>/`, `DeleteBucket`
    `DELETE /<name>/`, `HeadBucket` `HEAD /<b>/`; `ListBuckets` `GET /`. Of the operation
    parameters `uploads`, `uploadId`, `partNumber`, `location` and `delete`, each request
    carries exactly its own, with the values given (the `uploadId` as the probe named it),
    and none of another's; other query parameters the SDK adds are ignored, and so is the
    body framing (the SDK may frame a body with checksums, so no length is asserted). Method,
    path, that parameter set and `CopyObject`'s `x-amz-copy-source` together tell the twelve
    probes apart from each other and from the supported verbs. The salvage's
    `ProbeCase` checks one parameter per probe by presence only
    (`probe_reaches_the_gateway`, iteration-v3 patch lines 2761-2815); extend it here.
    **Bucket names reach the endpoint as given**, on an IP
    endpoint (`http://127.0.0.1:{port}`) AND a host-name endpoint (`http://localhost:{port}`):
    `CreateBucket` / `DeleteBucket` naming a bucket S3's rules forbid (one ending in `-`), a
    host-safe and a non-host-safe `--x-s3` name, an `--xa-s3` name, an Outposts-alias-shaped
    `--op-s3` name and an `arn:` name each arrive as ONE request, `PUT|DELETE /<name>/`, the
    name one percent-encoded path segment after the endpoint's own path, `Host:` the
    configured endpoint, signed for `s3` in the configured region. `ListBuckets` addresses the
    endpoint itself. A name that cannot be sent at all is `RequestNotBuilt` (nothing sent),
    never `NoResponse`. A request that reached the connector and got no answer is classified
    as the existing verbs classify it (`S3Client::classify`, `crates/validate/src/s3.rs:339-356`
    on `main`; revised 2026-10-23): the connect deadline expiring is `Timeout { phase:
    Connect }`, the operation deadline expiring is `Timeout { phase: Operation }`, and any
    other loss before a response head (refused, reset, closed unanswered) is `NoResponse`;
    none of them is ever `RequestNotBuilt`. A probe is shown under both deadlines, as
    `connect_deadline_expires_as_a_connect_timeout` and
    `operation_deadline_expires_as_an_operation_timeout` show it for `get_object`
    (`crates/validate/tests/s3_client_roundtrip.rs:1274`, `:1296`; the saturated listener
    `:1258`). The two unsendable names (empty or all white space; a URL past an `http`
    URI's 65,534 bytes) are named in the code's docs and tested.
  - **(C) A 2xx head survives its body, on every probe that can carry one.** A scripted
    endpoint answers `200` and then (i) stalls the body past the body-idle deadline, (ii)
    trickles it past the whole-body deadline, (iii) cuts the connection mid-body, (iv) sends a
    body that is not the operation's success document. Each report carries status 200 and the
    body failure beside it. A `200` whose body is an S3 `<Error>` document (S3 does this for
    `CopyObject` / `CompleteMultipartUpload`) keeps the 200 AND reports that document's
    `<Code>` and `<Message>` as typed fields.
  - **(D) An unreadable `<DeleteResult>` is refused** by the rules #955 applies to a listing
    page (cut short, followed by junk or a second document, past `Content-Length`, past
    `BUFFERED_BODY_BUDGET`): a typed refusal that keeps the 200, never a short delete result.
    A document of exactly the budget is read.
  - **(E)** The three existing `s3_client_*` test files pass unmodified; #955's tests still
    pass.
  - **(F) A mixed `<DeleteResult>` is reported row by row.** A scripted endpoint answers
    `POST ?delete` with `200`, two `<Deleted>` rows and one `<Error>` row (`<Key>`,
    `<Code>AccessDenied</Code>`, `<Message>`). The result lists exactly those two deleted keys
    and exactly one error row with that key, code and message as sent, in document order.
  - **(G) The success paths are pinned.** Against the scripted endpoint: a `200` with each
    probe's whole, correct success document (or no body where its success has none) is a
    success; a 304 that frames a chunked body is refused for a probe that is not a GET or HEAD.
    Each success leg first checks the request head the endpoint recorded against that probe's
    form in (B) (revised 2026-10-23), so a success answered to the wrong request cannot pass.
  - **(H) The living architecture doc matches the code.** The `validate` paragraph,
    `docs/design/architecture/05-building-block-view.md:299` on `main` (`:301` on the
    integration line), describes the new verbs — #955's and this child's — the 2xx head
    kept beside a body failure, listing continuation and its page cap, bulk-delete rows, the
    buffered budget's reach, and the endpoint resolution with its named exceptions and the
    two region changes. No sentence in it or in the code's docs claims more than the tests
    show. The salvage's new paragraph is the text to carry over.
- **Falsifiability:** RED is reachable on `cargo test -p wyrd-validate --test
  s3_client_smoke_verbs` (`cargo xtask ci` runs it too): a real gateway on loopback for
  (A)/(B), a scripted endpoint or relay with short deadlines for (B)-routing/(C)/(D)/(F)/(G).
  A client that lets the SDK reroute or refuse a bucket name, reports an unsent request as
  `NoResponse`, drops a `<DeleteResult>`'s error rows, or loses a 200 under a stalled probe
  body fails an assertion. No Docker, no network beyond loopback. **Inherited evidence covers
  the salvage's tests as the salvage wrote them:** each was shown red and green on iteration
  3's code (`results/issue_938/iteration-v3/build-notes.md`, reds 3, 5, 6–14). The two
  additions here — the full request checks in (B)/(G) and the probe deadline test in (B) —
  have no salvage evidence; their reds are shown in this cycle (reds 16 and 17 below).
- **Invariant to restore:** what the client reports is what the server sent, from the request
  the caller asked for: no request rewritten, rerouted or silently dropped on the way out, so
  "the server refused it" and "it was never sent" are never confused; nothing filled in on the
  server's behalf (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never
  erases the 2xx (proposal 0017 §7).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality; the verbs and the resolver do not exist on the
  base.
- **Depends on:** 955
- **Ordering note:** stacks on #955: every delete and probe reports through #955's
  `Capture`/`read_whole`/`answer` core, `delete_rows` uses its `whole_document`/`fields`/
  `named_code`, and the tests use its gateway fixture and scripted endpoint; the wave fold
  hands #955's accepted diff over. Independent of 743.1 (#937); **must not touch
  `crates/validate/src/lib.rs`**. 743.3 (#939) consumes this child and, through it, #955.
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
  scope: #955's core and read verbs beyond extending their docs; judging answers (743.1);
  the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart success
  path (#508, #750); any change to the existing verbs beyond what the shared resolution
  implies; any `wyrd-*` crate in `[dependencies]`; any new production dependency;
  `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/s3_client_smoke_verbs.rs` (#955's file, extended).
  This child's regression, from the salvage, by prefix:
  `delete_objects_reports_every_key_deleted…`, `every_unsupported_operation_reaches…`,
  `bucket_probes_send_every_name_as_one_path_segment…`, `an_endpoint_path_comes_before…`,
  `a_request_stopped_before_it_reaches_the_connector…`, `a_probe_that_reached_the_connector…`,
  `the_region_is_signed_as_given…`, `a_probe_answered_200_keeps_its_200…`, the
  `<DeleteResult>` legs of `listing_and_delete_result_documents_that_are_not_whole…`,
  `a_mixed_delete_result_is_reported_row_by_row…`, `a_2xx_carrying_the_operations_whole_s3_success…`,
  the probe leg of `a_304_that_frames_a_chunked_body…`,
  `a_200_whose_body_is_an_s3_error_document…`. Changed here: `probe_reaches_the_gateway` and
  the (G) success legs check each probe's full request form from (B). New here (Do names it):
  one test sending a probe under the connect deadline and one under the operation deadline,
  as the two `s3_client_roundtrip.rs` peers do for `get_object`. By reading, the salvage's
  `captured` already passes a `Timeout` through unchanged (iteration-v3 patch lines
  1848-1860); if the test shows otherwise, fix it inside this child's hunks and record the
  change in `build-notes.md`.
- **Verification posture:** (a) net-new. The verbs and the resolver do not exist on #955's
  tree, so with this child's production reverted the test cannot compile and C4-verify reports
  UNVERIFIABLE (exit 77): pre-declared. Everything is built and exercised at Check against the
  real gateway, the scripted endpoint or the relay. **Demonstrated reds (required, in
  `build-notes.md`):** (3) put the SDK's default endpoint resolution back → (B); (5) drop a
  `<DeleteResult>`'s `<Error>` rows → (F); (6) report a request refused before sending as
  `NoResponse` → (B); (1) return only the timeout when a probe's body stalls after a 2xx → (C);
  (16) turn a probe's expired deadline into `NoResponse` in `captured` → (B) deadlines; (17)
  send `UploadPart` with an `uploadId` other than the one the probe named (by reading, the
  salvage's presence-only check would pass it) → (B) request checks.
- **Production reach:** consumed by 743.3 (#939) in this same run; until then only the tests
  call the new verbs. The resolver is on every existing verb's live path from here on ((E)).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, the core this child reports through** (on the integration line after wave 1):
    #955's `Capture`/`read_whole`/`answer` (`crates/validate/src/s3/answer.rs`),
    `captured`/`read`/`answer` (`s3/verbs.rs`), `whole_document`/`named_code`
    (`s3/response.rs`), `fields` (`s3/document.rs`).
  - **Peer, fixtures:** #955's gateway fixture and scripted `Endpoint` in the test file; the
    relay's `Downstream` modes, `crates/validate/tests/s3_client_roundtrip.rs:465-474`.
  - **The resolver's SDK citations and rejected alternatives:**
    `/home/eddie/wyrd/wyrd-pdca/results/issue_938/iteration-v3/build-notes.md` §1–2 and
    "Rejected alternatives" (every claim in the `PathStyle` doc is cited to `aws-sdk-s3`
    1.152.0 source there; hashes and the stop rule in the header).
  - **Peer, deadlines:** `connect_deadline_expires_as_a_connect_timeout` and
    `operation_deadline_expires_as_an_operation_timeout`,
    `crates/validate/tests/s3_client_roundtrip.rs:1274`, `:1296` (`saturated_listener`
    `:1258`); the classification they pin, `S3Client::classify`,
    `crates/validate/src/s3.rs:339-356`.
- **Prior-art check (triage cycles):** searched by path `crates/validate/src/s3.rs`,
  `crates/validate/src/s3/` (2026-10-23): merged #859, #865, #934 (#853); open: #942 (stacked
  on the integration line only); closed-unmerged: none. Rejected attempts are the parent's
  own `results/issue_938/iteration-v1..v3` (v3: no defect; every v1/v2 finding is in (B),
  (C), (G), (H) above). The bucket-safety rule (B) serves is #743's
  (`results/issue_743/review-batch.md:3`); which forbidden name the probes use is 743.3's to
  choose (`results/issue_743/split-proposal.md:495`).
- **Disposition hint:** new-feature

Plan-review response (2026-10-23): all three findings are addressed in place. (1) Probe
requests: (B) now names each probe's method, path, operation parameters (with values) and
`CopyObject`'s copy-source header, and requires each probe to carry exactly its own
parameters, so a request landing on the shared `501` guard as the wrong operation fails; (G)'s
success legs check the request too; red 17 is a mutation the salvage's presence-only check
would let through, by reading. (By reading, the salvage already rejects the reviewer's
`ListParts`/`ListMultipartUploads` example, by path; the brief now says so explicitly.) (2) Timeouts: the reviewer is right.
(B) now keeps the base's typed deadlines (`Timeout { Connect }`, `Timeout { Operation }`),
keeps `NoResponse` for other losses, and adds a probe test under each deadline (red 16). (3)
Salvage: as for #955, the file exists on the harness host where Do runs but is untracked;
absolute paths, sha256 hashes and a stop-and-flag rule are in the header, and the
falsifiability line separates inherited evidence from this cycle's additions. The tracker
excerpts asked for are now quoted: #743's bucket-safety finding and the human's #938
sign-off.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
