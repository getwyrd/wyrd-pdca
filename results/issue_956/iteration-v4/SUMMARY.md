# Result — issue 956 / validate-s3-client-deletes-and-bucket-probes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: with #955 in, the client still cannot send bulk `DeleteObjects` or one request
  for each operation the matrix may declare unsupported. And a probe does not reach the
  endpoint it was given with the bucket name it was given: the SDK's endpoint rules reroute
  or refuse some names (S3 Express `--x-s3` / `--xa-s3`, Outposts-alias `--op-s3`, `arn:`)
  before anything is sent — exactly what 743.3's bucket-safety rule cannot tolerate (a
  `DeleteBucket` probe must name a bucket S3's rules forbid, so it can never delete one it
  does not own). A mixed `<DeleteResult>` has no evidence of a failed key but its `<Error>`
  rows (the gateway answers `200` either way, `crates/gateway-s3/src/lib.rs:2293-2296`).
- Success criterion: the tests this child adds to `crates/validate/tests/s3_client_smoke_verbs.rs`
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: bulk delete, the probes and their typed results in `crates/validate/src/s3/*`;
  the endpoint resolution in `crates/validate/src/s3.rs`; the tests added to the test file;
  the `validate` paragraph of `docs/design/architecture/05-building-block-view.md`. / out of
  scope: #955's core and read verbs beyond extending their docs; judging answers (743.1);
  the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart success
  path (#508, #750); any change to the existing verbs beyond what the shared resolution
  implies; any `wyrd-*` crate in `[dependencies]`; any new production dependency;
  `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.9% — 280 of 286 instrumentable changed lines executed (floor 80%); 286 of 662 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 64 mutants tested in 18m: 47 caught, 14 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.56s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No confirmed patch defect found in #956’s addition of bulk S3 deletion, operation probes, and literal endpoint routing; prior-art coverage and fitness for the downstream runner remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The observable wire contract, failure classification, and ownership split are explicit; the accepted follow-up preserves the 64 KiB limit and requires accurate failure-row sizing (`brief.md:43`, `brief.md:264`; `crates/validate/src/s3/verbs.rs:339`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality: retaining the new tests while stashing production changes produces missing `Probe`/delete APIs, not a behavioral reproduction (`reviewer-red.log:3`); this limitation is pre-declared in `brief.md:179`. |
| C3 Change | PASS | The six-file patch stays within the client, its tests, and living architecture; it leaves `lib.rs`, dependency manifests, judging, and the runner untouched (`crates/validate/src/s3.rs:188`, `crates/validate/src/s3/verbs.rs:361`, `docs/design/architecture/05-building-block-view.md:303`). |
| C4 Verification (red→green) | PASS | All 192 crate tests pass independently after restoring the patch; the negative leg is the expected missing-API compile failure, so this establishes new-feature green verification rather than a behavioral red→green (`reviewer-red.log:3`, `reviewer-restored-green.log:230`, `gate-logs/C4-verify.log:60`). |
| C5 Causal adequacy | PASS | Endpoint rule substitution addresses rerouting directly, and wire assertions distinguish operations and preserve failed-key evidence; the URI length check validates input rather than probing an assumed capability (`crates/validate/src/s3.rs:188`, `crates/validate/tests/s3_client_smoke_verbs.rs:945`, `crates/validate/src/s3/document.rs:238`). |
| T1 Structure | PASS | The client continues through the existing capture, bounded body reader, and strict document parser, with no new production dependency or backend coupling (`crates/validate/src/s3/verbs.rs:375`, `crates/validate/src/s3/verbs.rs:519`, `gate-logs/C4-ci.log:28`). |
| T2 Shape | PASS | Typed request/response boundaries remain clear, and the test-heavy bundle respects the human’s explicit decision against another split (`crates/validate/src/s3/verbs.rs:69`, `crates/validate/src/s3/document.rs:80`, `brief.md:264`). |
| T3 Runtime | PASS | Independent loopback tests exercise actual gateway deletion, precise request forms, connect/operation deadlines, body failures, and budget refusal while retaining the response head (`crates/validate/tests/s3_client_smoke_verbs.rs:562`, `crates/validate/tests/s3_client_smoke_verbs.rs:1773`, `crates/validate/tests/s3_client_smoke_verbs.rs:2369`; `reviewer-restored-green.log:230`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive audit must re-run at publish, while the frozen batch review reports zero findings (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm affected-path prior-art coverage, including the smoke-test and architecture files — the brief records source-path searches and rejected parent attempts, but the supplied single-commit target has no remote/history with which to settle merged and closed/rejected work for all six paths (`brief.md:207`, `reviewer-provenance.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the scoped library as fit for #939’s smoke runner — the executable wire evidence is green, while exercising that downstream consumer is outside this child’s scope (`brief.md:189`; `crates/validate/src/s3/verbs.rs:343`). |

The requested carry-forward corrections are supported by real gateway behavior. The new fault test corrupts directory entries in the actual metadata store, checks the gateway’s `InternalError` row, accepts 41 failures at 65,518 bytes, and refuses 42 at 65,601 bytes beside HTTP 200. It also confirms that a successful deletion has already happened despite losing the response rows (`crates/validate/tests/s3_client_smoke_verbs.rs:681`). The over-budget assertion now checks `received > BUFFERED_BODY_BUDGET` independently of the exact declared length, matching the body reader’s frame-dependent byte count (`crates/validate/tests/s3_client_smoke_verbs.rs:611`, `crates/validate/src/s3/answer.rs:617`). Both tests passed independently (`reviewer-restored-green.log:215`, `reviewer-restored-green.log:228`).

Verification has these specific limits and results:

- Independent execution: `cargo test --offline --locked -p wyrd-validate --test s3_client_smoke_verbs` passed all 53 tests. Stashing only the four changed production files left the new tests in place and produced exit 101 for absent APIs. After `git stash pop`, `cargo test --offline --locked -p wyrd-validate` passed all 192 tests, including the three unchanged client suites. The restored diff is byte-identical to the original, and the supplied patch reverse-checks cleanly (`reviewer-stash.log:2`, `reviewer-restored-green.log:150`, `reviewer-restored-green.log:171`, `reviewer-restored-green.log:248`, `reviewer-provenance.log:19`).
- Independent formatting, spelling, docs lint, and docs rendering passed; the renderer built 101 pages and passed its link audit. Thus both named external dependencies, `typos` and `docs-renderer`, were exercised (`reviewer-provenance.log:25`, `reviewer-docs.log:2`). The full workspace CI and TiKV feature-check verdicts additionally rest on their frozen logs, not on a claim that I re-ran the instance-scoped wrappers (`gate-logs/C4-ci.log:4659`, `gate-logs/host-tikv.log:211`).
- Frozen coverage records 280/286 instrumentable changed lines executed, 97.9%, with 376 changed lines unscored (`gate-logs/C4-diff-cov.log:296`). Mutation testing returned exit 3: 47 caught, 14 unviable, and three resolver mutations timed out. It reports no surviving mutant; the timeouts are incomplete mutation evidence, not demonstrated production defects. I did not reproduce that mutation campaign or the six specific mutation reds requested in the brief, and the supplied logs do not establish those six separately (`gate-logs/C5-mutants.log:7`, `gate-logs/C5-mutants.log:13`; `brief.md:183`).

All code citations above refer to the supplied `$PDCA_TARGET`, which contains the patch and compiled successfully; input and execution-log citations refer to this review directory. The target’s standing rubric was applied. Its available history contains only the synthetic pre-fix baseline, so the prior-art limitation is an evidence limitation, not a defect in the patch. No implementation-fixable concern or additional symptom-versus-root-cause finding is raised.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 956

I could not break the production code. I found one flaky new test and one set of doc sentences that the new verbs made wrong. I ran everything in a scratch copy of `$PDCA_TARGET`, so the toolchain was available and this verdict is not provisional.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1683-1686`: the "refused connection" leg of `a_probe_that_reached_the_connector…` is flaky. It binds a listener, drops it, then assumes the freed port stays closed while other listeners are binding ports. Another test can take that port. Then the probe gets a real answer, and the other test loses the connection its script was waiting for.
  - **Seen:** 7 failed runs out of 140, all with several copies of the test binary running at once on a loaded host (16 runs: 1 failed; 54 runs: 0 failed; 70 runs: 6 failed).
  - **Failure message (line 1689):** `a refused connection: Ok(Answer { head: Head { status: 200, fields: [("x-amz-request-id", … "get-framing")…`. Other runs were answered with the ids `head-body` and `region-case`.
  - **Innocent victims:** in the last batch, the three tests that own those ids failed in the same rounds with `Connection reset by peer`: `a_get_2xx_whose_framing…`, `a_head_answered_with_a_body…` and `the_region_is_signed_as_given…`.
  - **Not measured:** the rate for one binary alone, which `cargo test` runs one at a time. It should be much lower, but 52 other tests in that binary still bind ports in parallel.
  - **Fix:** the repo already has one at `crates/validate/tests/s3_client_roundtrip.rs:1426-1432`, whose comment says "bound but not listening … no other test can take the port meanwhile". I tried that form here: it gives `NoResponse` ("Connection refused", os error 111) and a second bind on the port fails. `Relay::start(refused_addr)` at `:1697` keeps working with it.

- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:303` and `crates/validate/src/s3.rs:29-30`: brief (H) says no sentence may claim more than the tests show, and this new paragraph keeps sentences that are now false or broken. All of these are one-line wording fixes, for example limiting the first two to "the older PUT and DELETE of one object".
  - **"A PUT or DELETE success that carries a body, which S3 never sends, is unreadable."** `Probe::CopyObject` is a PUT, and `a_2xx_carrying_the_operations_whole_s3_success_is_a_success` requires its 200 with a `<CopyObjectResult>` body to be `Outcome::Success`. The same paragraph later says S3 sends bodies on CopyObject.
  - **"Every body the SDK buffers whole (every error response, and every PUT or DELETE response) … a PUT or DELETE success's at nothing".** The same claim is at `s3.rs:29-30`. It is no longer true of every response: the probes' PUTs and DELETEs and their error responses, and the bulk delete's, are read by the client under the 64 KiB budget (`Capture`), and a CopyObject success is not capped at nothing.
  - **"a `Transfer-Encoding` on a `304` to a PUT or a DELETE".** The code refuses it for anything but a GET or HEAD (`crates/validate/src/s3/response.rs:117`). The new tests pin POST too: CreateMultipartUpload, CompleteMultipartUpload and bulk delete.
  - **"every row lost. So does a delete result that holds anything but…".** The sizing text added in iteration 3 separated "So does" from what it refers to ("fails the whole call"). It now reads as if a delete result loses rows.

- `check-gates.json:44-48`: this row's claim is not earned. It is titled "red->green" and says `pass`, but `gate-logs/C4-verify.log` ends in `PASS (green-only)`, so the gate showed no red. The brief (`brief.md:180-181`) also predicted `UNVERIFIABLE (exit 77)`, which is not what happened. I ran the reds myself, so the evidence holds (see below). Only the row's label is misleading, and the builder has nothing to fix.

- `check-gates.json:66-70`: the C5 `fail` is not a test gap. The 3 timeouts (`s3.rs:509`, `:514`) all break endpoint resolution for every request. I ran the `Ok("xyzzy")` one: 52 tests in this one file went red, so the whole-suite timeout is most likely many slow failures rather than a hang. The two guard mutants were not run; by reading, they refuse every request the same way.

- Outside this diff, for information: `crates/gateway-s3/src/lib.rs:3803` (`xml_escape`) does not escape a carriage return. Against the real gateway, `delete_objects(&["a\rb", "other"])` reports a row `Deleted { key: "a\nb" }`, and lists `a\rb` under `keys_without_a_row` and that row under `rows_not_requested`. The client reads the XML correctly (`crates/validate/src/s3/document.rs:75`), and the fault is the gateway's. It deserves its own issue because 743.1 will judge those two lists.

## Red→green, re-run on the production path

All of these ran against `s3_client_smoke_verbs`, using the real gateway, relay or scripted endpoint. The tests call the production `S3Client`, not a copy of it.

- **Reds the brief names, all red:**
  - red 1: `a_probe_answered_200_keeps_its_200…`
  - red 3 (`.endpoint_resolver(PathStyle)` removed, so the SDK's own rules apply): `bucket_probes_send_every_name…` reports 42 failed cases, and three other tests go red
  - red 5: 4 tests
  - red 6: 3 tests
  - red 16: both deadline tests
  - red 17: 4 tests
- **Carry-forward items, all red when undone:**
  - processing instruction skipped
  - unknown root child skipped
  - empty `<Key>` accepted
  - `Capture::new(true)` on bulk delete
  - body timeout returned as `NoAnswer`
  - non-empty `UploadPart` body
  - `CreateBucket` with a configuration
  - wrong `CompleteMultipartUpload` part number
- **`too_long` and the resolver, all red:** `URI_ROOM` at 40 and at 65, no `/` credit, raw key length, upload id ignored, `%` or `/` left unencoded, lower-case hex, trailing slash kept, endpoint query accepted, scheme not checked.
- **Overall:** 41 mutations went red and 3 stayed green. None of the 3 is a test gap:
  - Removing `.content_length(0)` (`crates/validate/src/s3/verbs.rs:436`) changes nothing on the wire, because the body is empty anyway.
  - A one-byte body with the length left at 0 stayed green. A one-byte body with a correct length went red.
  - Removing `.disable_s3_express_session_auth(true)` (`s3.rs:193`) changes nothing today, and the comment there says it only guards against a later SDK.

## Attempted to refute, could not

- **Panics:** no panic from any key, upload id or bucket length at or around the URI limit. This covered every probe with a key at exactly 65,470 encoded bytes, using plain, `/`, `é`, `&` and 4-byte characters, and bucket names from 65,440 to 65,557 bytes. `URI_ROOM` covers the longest thing the SDK writes (40 bytes). The SDK's encoding set matches `path_segment` (`aws-smithy-http` 0.64.3 `urlencode.rs:9-42`, `label.rs:14`).
- **Unsendable input:** an empty key, an empty upload id, a newline in a source key, and Unicode-white-space bucket names are all `RequestNotBuilt` with nothing sent. `a\0b` goes out as `/a%00b/`. `/a` and `../a` as keys are sent unnormalised.
- **Bulk delete against the real gateway:**
  - Empty key list, 1,001 keys, and control-character, `\u{fffe}` and empty keys: the gateway's 400 is kept, and the other key in the request survives.
  - Leading or trailing space, tab, `\n`, `\u{85}`, `\u{2028}` and `]]>` in keys: rows match the requested keys exactly.
- **Budget test:** the 41/42 boundary comes from a real store fault, so the gateway's own `<Error>` rows set the sizes. The numbers add up: 62,115 + 41×83 = 65,518 and 62,115 + 42×83 = 65,601. `received > BUFFERED_BODY_BUDGET` no longer depends on how TCP splits the response.
- **#955's tests:** the only test name removed from the file is the rename to `listing_and_delete_result_documents_that_are_not_whole…`. The listing legs still run there.
- **Endpoint forms:** an endpoint's `#fragment` is dropped silently. The SDK's rules parse the same way (`aws-sdk-s3` 1.155.0 `endpoint_lib/parse_url.rs:49-62`), so this is no regression and the `PathStyle` doc's claim holds.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed the patch against the read-only target source and supplied frozen gate evidence. The smoke-verbs suite passed all 53 tests. The mutation run recorded 47 caught, 14 unviable, and 3 timed-out mutations, with no surviving mutants reported. Tests were not rerun in this review sandbox.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm affected-path prior-art coverage, including the smoke-test and architecture files — the brief records source-path searches and rejected parent attempts, but the supplied single-commit target has no remote/history with which to settle merged and closed/rejected work for all six paths (`brief.md:207`, `reviewer-provenance.log:3`).
- [ ] Validation — fitness-to-purpose — Accept the scoped library as fit for #939’s smoke runner — the executable wire evidence is green, while exercising that downstream consumer is outside this child’s scope (`brief.md:189`; `crates/validate/src/s3/verbs.rs:343`).
- [ ] `crates/validate/tests/s3_client_smoke_verbs.rs:1683-1686`: the "refused connection" leg of `a_probe_that_reached_the_connector…` is flaky. It binds a listener, drops it, then assumes the freed port stays closed while other listeners are binding ports. Another test can take that port. Then the probe gets a real answer, and the other test loses the connection its script was waiting for.
- [ ] `docs/design/architecture/05-building-block-view.md:303` and `crates/validate/src/s3.rs:29-30`: brief (H) says no sentence may claim more than the tests show, and this new paragraph keeps sentences that are now false or broken. All of these are one-line wording fixes, for example limiting the first two to "the older PUT and DELETE of one object".
- [x] **The rejection criterion can pass for the wrong multipart operation.** Brief lines 27–30 require six multipart probes to return `501 NotImplemented`, but the target rejects any recognized bucket subresource through the same branch (`crates/gateway-s3/src/lib.rs:1797–1802`) and any recognized object subresource through another shared branch (`:1836–1847`). A `ListParts` probe accidentally sending `ListMultipartUploads` can therefore satisfy that outcome. Criterion G (brief lines 58–60) supplies success responses but does not require checking the request that elicited them. Require captured method/path/query/header/body assertions for each probe, so a wrong operation or missing operation parameter cannot pass merely by hitting a rejection guard.
- [x] **Criterion B contradicts the existing timeout contract.** Brief lines 37–39 say a request that reached the connector and got no answer “stays `NoResponse`.” The target explicitly reports SDK operation timeouts as `Timeout { phase: Operation }` and connector timeouts as `Timeout { phase: Connect }` (`crates/validate/src/s3.rs:346–355`); these also receive no answer. Qualify the criterion to preserve typed deadline failures, reserve `NoResponse` for non-timeout transport failures, and require probe assertions for both deadline phases. Otherwise the stated acceptance rule permits losing the measured timeout phase.
- [x] **The mandatory salvage prerequisite is unresolved in the supplied material.** Brief lines 7–9 require taking the prior patch's hunks unchanged and forbid re-deriving the design; lines 67, 89–95 and 135–137 also depend on its exact architecture paragraph, implementation and SDK evidence. Neither the review cwd nor `$PDCA_TARGET` contains `results/issue_938/iteration-v3/patch.diff` or its `build-notes.md`; `notes.json` and `sources/` are also absent. This does not prove those artifacts are missing elsewhere, but their availability and the claimed prior red/green evidence cannot be established here. Revise the brief to make the required salvage an explicitly supplied, resolvable input, or include the necessary contract and remove the exclusive instruction to transplant unavailable hunks. Include the relevant tracker excerpts so the bucket-safety constraint and prior-review disposition can be checked.
- [ ] size backstop — this slice is behaving oversized: patch is 190 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Keep the routing, probe and row-reading code; it held up. Do not split: the human overrode the size warning again, since this round's findings are narrow. Fix these only: 1. The "refused connection" leg of a_probe_that_reached_the_connector... (crates/validate/tests/s3_client_smoke_verbs.rs:1683-1686) is flaky: it binds a listener, drops it, and assumes the freed port stays closed, but another test can take that port, which also fails that other test. The adversary saw 7 failures in 140 runs, with several copies of the test binary running on a loaded host. Use the bound-but-not-listening socket from crates/validate/tests/s3_client_roundtrip.rs:1426-1432, so no other test can take the port. The Relay::start(refused_addr) at :1697 must keep working with it. 2. Correct four sentences in docs/design/architecture/05-building-block-view.md:303, and the same claim at crates/validate/src/s3.rs:29-30, which claim more than the tests show (brief (H)): - "A PUT or DELETE success that carries a body, which S3 never sends, is unreadable": CopyObject is a PUT and its success carries <CopyObjectResult>. Limit it to the older PUT and DELETE of one object. - "Every body the SDK buffers whole (every error response, and every PUT or DELETE response) ... a PUT or DELETE success's at nothing": the probes and bulk delete are read by the client under the 64 KiB budget (Capture), and a CopyObject success is not capped at nothing. Limit it the same way. - "a Transfer-Encoding on a 304 to a PUT or a DELETE": the code refuses it for anything but a GET or HEAD (crates/validate/src/s3/response.rs:117), and the tests pin POST too. Say so. - "every row lost. So does a delete result that holds anything but...": the sizing text separated "So does" from "fails the whole call". Reword so the sentence says what it refers to. Nothing else changes.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
