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
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 36 mutants tested in 8m: 20 caught, 13 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.41s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #956’s bulk-delete/probe client and endpoint routing: no confirmed implementation defect; prior-art confirmation and fitness-to-purpose remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | A–H define observable wire, response, deadline, and compatibility requirements with explicit ownership boundaries; `brief.md:42`, `brief.md:129`, `brief.md:152`. |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality, as predeclared; retaining the tests while stashing production yields missing-API compilation errors, not a behavioral reproduction of an existing verb; `brief.md:135`, `brief.md:179`, `reviewer-red.log:3`. |
| C3 Change | PASS | The six changed paths support the specified client surface and living documentation; the existing public entry point and dependency manifests are untouched, and the shared success-body extension preserves existing tests; `target/crates/validate/src/s3/answer.rs:334`, `target/docs/design/architecture/05-building-block-view.md:303`. |
| C4 Verification (red→green) | PASS | Independently observed 184 passing validator tests, six assertion-level semantic reds, and restored green; the frozen coverage failure measured no coverage and is a base-state caveat; `reviewer-restored-green.log:15`, `reviewer-restored-green.log:222`, `reviewer-semantic-summary.log:1`, `gate-logs/C4-diff-cov.log:10`. |
| C5 Causal adequacy | PASS | The resolver removes SDK name interpretation at its source, and intentional losses of routing, error rows, head evidence, timeout classification, and request identity are detected; `target/crates/validate/src/s3.rs:491`, `reviewer-semantic-summary.log:1`. |
| T1 Structure | PASS | Response parsing and transport remain separate from scenario judgment, without a new production dependency or backend coupling; `target/crates/validate/src/s3/document.rs:4`, `target/crates/validate/Cargo.toml:23`. |
| T2 Shape | PASS | Typed evidence retains row order, missing-code information, and the original response head; operation success bodies use the existing strict document reader; `target/crates/validate/src/s3/document.rs:69`, `target/crates/validate/src/s3/answer.rs:343`, `target/crates/validate/tests/s3_client_smoke_verbs.rs:2494`. |
| T3 Runtime | PASS | Real loopback execution demonstrates bounded connect/operation/body waits, preserved 2xx heads, and refusal of truncated or over-budget documents; `target/crates/validate/tests/s3_client_smoke_verbs.rs:1345`, `target/crates/validate/src/s3/verbs.rs:451`, `reviewer-restored-green.log:206`. |
| T4 Contribution | N/A | Contribution artifacts are absent by design and require their substantive audit at publish; the batch-review finding and TiKV evidence are adjudicated below; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN | Confirm the merged and closed/rejected prior-art disposition for all six affected paths — the brief records source-path searches, but this one-commit snapshot cannot independently establish history and supplies no test/doc-path search evidence; `brief.md:207`, `reviewer-prior-art.log:10`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this client-only deliverable for #939’s runner — wire evidence passes, while scenario judgment and scratch-bucket ownership checks belong to #937/#939 and are not demonstrated by this child; `brief.md:24`, `brief.md:141`, `brief.md:189`. |

**The batch finding should be rejected within #956’s scope.** The behavior reported at `gate-logs/T4-batch-review.log:10` is real, but its interpretation as silent deletion success is not supported by this API’s contract. An independent loopback countercase requested `expected-key`, returned HTTP 200 with `<DeleteResult/>`, and observed `rows=[]`, zero deleted keys, and zero error rows (`reviewer-countercase.log:3`). The returned value asserts that the response was read; it does not assert that the requested keys were deleted. The same `Ok(DeleteResult)` deliberately carries explicit failure rows (`target/crates/validate/src/s3/verbs.rs:321`, `target/crates/validate/tests/s3_client_smoke_verbs.rs:2523`).

The document layer explicitly assigns judgments about which keys and how many to its caller (`target/crates/validate/src/s3/document.rs:9`); the brief excludes judging answers and assigns that work to 743.1/#937 (`brief.md:155`). Requested-versus-returned key reconciliation belongs to that work and its #939 consumer. Applying the rubric’s absent-entry rule here as a requirement to make a scenario verdict would cross that boundary. Record this as a scope-based rejection with those issue references, consistent with `target/AGENTS.md:219`. This advisory disposition does not rewrite the frozen failing gate result.

**Independent execution supports the scoped change.** All source citations above ground in `$PDCA_TARGET`, the supplied disposable `target/`. Its diff was byte-identical to `patch.diff` before and after review: SHA-256 `6f58dbd3771aec819cbab367035d90b0ea436443187908ab2a6e2b1100091290`. The production stash was popped successfully and no stash remains.

- `cargo test --offline --locked --manifest-path target/Cargo.toml -p wyrd-validate`: 184 tests passed, including all 45 smoke tests and the unchanged existing client suites (`reviewer-restored-green.log`). With the new tests retained and production stashed, compilation failed on absent `Probe`, `DeleteRow`, `KeyError`, and methods (`reviewer-red.log:3`), matching the declared net-new posture.
- Six deliberate regressions were compiled and exercised in a separate scratch copy inside this review directory. Each failed test assertions; none was merely a compilation failure. Exact changes and full outputs are retained in `reviewer-semantic-{1,3,5,6,16,17}.diff` and `.log`. Restoring the original code returned all 45 smoke tests to green (`reviewer-semantic-summary.log:7`).
- Formatting, changed-file spelling, documentation lint, and documentation rendering/link audit passed independently. The renderer produced 101 pages (`reviewer-docs.log:2`). The named external dependencies, `typos` and the docs renderer, were exercised without substitutes.

| Deliberate regression | Independently observed failure |
|---|---|
| 3: restore default SDK endpoint resolution | The endpoint-prefix request-path assertion fails; `reviewer-semantic-3.log:274`. This rerun uses the loopback prefix case, without sending SDK-rerouted requests to external hosts. |
| 5: discard bulk-delete error rows | Exact mixed-row comparison fails; `reviewer-semantic-5.log:20`. |
| 6: classify an unsent request as `NoResponse` | The pre-connector refusal assertion fails; `reviewer-semantic-6.log:20`. |
| 1: erase the 200 when a probe body times out | The retained-head/body-failure assertion fails; `reviewer-semantic-1.log:42`. |
| 16: classify deadlines as `NoResponse` | Both connect and operation deadline assertions fail; `reviewer-semantic-16.log:21`, `reviewer-semantic-16.log:29`. |
| 17: change `UploadPart`’s upload ID | Gateway request identity, scripted success, and 304 request checks fail; `reviewer-semantic-17.log:64`, `reviewer-semantic-17.log:93`, `reviewer-semantic-17.log:124`. |

The C5 symptom-guard rule does not trigger: endpoint resolution is replaced, and session authentication is disabled during client construction to prevent extra SDK requests (`target/crates/validate/src/s3.rs:186`). There is no added conditional capability probe masking a load-time cause.

**Frozen evidence has specific limits, not additional demonstrated patch defects.** The instance-root wrappers were not available in this artifact sandbox; their captured logs were used alongside the independent executions above.

- **C4-ci — PASS from the frozen log:** it records spelling, docs lint/render, formatting, clippy, build, workspace tests, dependency checks, guards, and DST, ending with “all checks passed” (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4651`). I did not claim an independent rerun of the entire workspace CI campaign.
- **C4-verify — green-only in the frozen log:** its final lines explicitly say the red leg was not isolated (`gate-logs/C4-verify.log:60`). Independent stashing and semantic regressions supply the additional evidence described above.
- **C4-diff-cov — coverage unmeasured:** the wrapper stopped because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The brief declares the #955 prerequisite (`brief.md:137`), and the supplied target compiles with this patch. This is a base-state caveat, not evidence of a patch compilation or coverage defect; no coverage percentage is claimed.
- **C5-mutants — incomplete frozen campaign:** 20 caught, 13 unviable, three timeouts; no surviving mutant is reported (`gate-logs/C5-mutants.log:16`). Independently rerunning those three with the focused gateway probe test caught two; the `xyzzy` replacement was unviable because denied unused-variable/dead-code warnings prevented compilation (`reviewer-mutants.log:1`, `pdca-reviewer-956-mutations/mutants.out/log/crates__validate__src__s3.rs_line_507_col_5.log:334`). That third result is not an assertion-level kill. The six separate semantic regressions above all reached failing assertions.
- **T4-batch-review — reported FAIL, advisory rejection recorded above:** the log contains exactly the empty-result allegation; it does not supply a failing reproduction (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution — N/A:** intentionally deferred until the mandatory publish audit; no human clearance is required for that deferral (`gate-logs/T4-contribution.log:10`).
- **host-tikv — PASS from the frozen log:** both requested feature builds finish successfully; this unrelated feature compilation was not repeated (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

Prior-art remains the only historical-evidence gap: the brief records merged #859/#865/#934, no closed-unmerged work, and rejected parent iterations by source path. The local investigation found only the harness’s synthetic base commit and no remote or independent closed-PR evidence (`reviewer-prior-art.log:10`). The human decision is whether that record sufficiently covers all changed paths, including the test and architecture document. No visual/manual reproduction is being handed off: the available loopback behavior was exercised directly.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 956

The bucket routing and the probe requests held up under attack. The delete result did not: a `200` that accounts for none of the requested keys comes back as a clean `Ok`. All results below come from a scratch copy of `$PDCA_TARGET` with the patch applied, where `cargo test -p wyrd-validate --test s3_client_smoke_verbs` passes 45 of 45 before any change.

## Refutations

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:288`: **a delete result that accounts for no key is reported as success.** `delete_objects(&["a", "b"])` answered with `200` and `<DeleteResult/>` returned `Ok([])`, so `deleted()` and `errors()` are both empty. A result with only `<Deleted><Key>a</Key></Deleted>` returned `Ok`, with `b` unaccounted for. A result naming only `c`, which was never requested, also returned `Ok`. The doc comment says "the result names every key deleted as well as every key not", and that is false for these answers. `docs/design/architecture/05-building-block-view.md:303` says "never a partial listing or a short delete result", and these are short delete results. This is the rubric's "absent entries" class, and it is the same defect the gating T4 row still blocks on (`gate-logs/T4-batch-review.log:10`). The brief leaves judging answers to 743.1, so the client need not refuse. It can report instead, for example with the requested keys that have no row and the rows that name no requested key, which fills in nothing on the server's behalf. Either way the two sentences need correcting and the case needs a test.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:221`: **a failed key can vanish in a row that goes unread.** `_ => {}` skips any child it does not recognise. `<Deleted><Key>a</Key></Deleted><error><Key>b</Key><Code>AccessDenied</Code></error>` returned `Ok([Deleted { key: "a" }])`, so `errors()` is empty for a key the server said it did not delete. The brief says the `<Error>` rows are the only evidence of a failed key (`brief.md:40`), and the rubric rejects silent skips. The test that pins the skip (`crates/validate/tests/s3_client_smoke_verbs.rs:2635`) uses `<RequestCharged>`, which the SDK reads from a header for this operation (`aws-sdk-s3` 1.155.0 `protocol_serde/shape_delete_objects.rs:38`). Its deserializer reads only `Error` and `Deleted` as elements (same file, `:143`, `:160`), so that test does not show a real element that needs skipping. Refuse unknown children, or report them.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:209`: **an empty `<Key/>` counts as naming a key.** `<Deleted><Key/></Deleted><Error><Key></Key><Code>x</Code></Error>` returned `Ok([Deleted { key: "" }, Error(KeyError { key: "", .. })])`. The check at `:209` and `:214` catches only a missing `<Key>`, although its refusal reads "names no `<Key>`". An empty `<Code>` is already read as absent (`named_code`), so the two fields are treated differently.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:56`: **the probe docs describe request bodies that no test checks.** The relay and the request checks record heads only (`crates/validate/tests/s3_client_smoke_verbs.rs:661`, `:709`). Three mutations each left all 45 tests passing:
  - `UploadPart` sending a 5-byte `hello` (doc at `:56`, "with an empty part")
  - `CompleteMultipartUpload` sending no parts (doc at `:58`, "the one empty part")
  - `CreateBucket` sending a `us-west-2` `<CreateBucketConfiguration>` (doc at `:69`, "with no")

  The docs are true today: on the wire, `UploadPart` has `content-length: 0`. But brief (H) says no doc sentence may claim more than the tests show. The body also affects the answer: a server that checks the body before the upload id could answer an empty `CompleteMultipartUpload` with a different code, which 743.1 would then judge. The brief excuses body *framing* from the checks, not body content. Record the bodies and assert these three.
- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:303`: **the safety sentence claims more than the client does.** "a probe can name a bucket those rules forbid, and so cannot create or delete one it does not own" reads as a property of the client, but the client checks no name. `Probe::DeleteBucket { bucket: "victim" }` is sent as given, and the safety comes only from 743.3 choosing a forbidden name. The sentence also drops the exception in `crates/validate/src/s3/verbs.rs:44`: I ran `..` through the relay and it was sent as `DELETE /../`, which a server that resolves dot segments reads as the endpoint's parent. That verbs.rs sentence is accurate but has no test, because `MORE_FORBIDDEN_BUCKETS` (`crates/validate/tests/s3_client_smoke_verbs.rs:150`) has no `.` or `..` row. Reword the sentence so the safety depends on the caller's name, carry the dot-segment exception over, and add both rows.

## Evidence I tried to refute and could not

- `gate-logs/C4-verify.log:60`: the row says `pass`, but the gate only shows green ("green-only"), and `brief.md:181` predicted UNVERIFIABLE (exit 77). So that row is no red→green proof. I ran the reds myself, and each failed the tests that should catch it:
  - Red 1 fails `a_probe_answered_200_keeps_its_200…`.
  - Red 3 fails 4 tests, including `bucket_probes_send_every_name…`.
  - Red 5 fails `a_mixed_delete_result…`.
  - Red 6 fails `a_request_stopped_before…`.
  - Red 16 fails both probe deadline tests.
  - Red 17 fails `every_unsupported_operation…` and the success test.

  These tests go through the real `S3Client::probe` and `delete_objects`, not a copy. `build-notes.md` is withheld from me, so these mutations are my own reading of the brief's descriptions, not the builder's exact edits.
- `gate-logs/C5-mutants.log:13`: the three timeouts do not point to a weak test here. With `path_style_url` returning `Ok("xyzzy")` (`crates/validate/src/s3.rs:506`), all 45 tests in this file fail in 20 seconds with no hang. I did not run the other two mutants, or find which test file reaches the 61-second limit. By reading, those two refuse every endpoint.
- `gate-logs/C4-diff-cov.log:11`: diff coverage was never measured, because the patch does not apply on `origin/main`. That row says nothing either way about coverage.
- `brief.md:8`: "found without defect" should not count as evidence for this child. The first refutation above sits in the salvage's `delete_objects`, and the first two come from code the brief says to take as written.

## Fix attacks that failed

- `crates/validate/src/s3.rs:546`: bucket names `...`, ` x`, `x\n`, `%`, `a?b`, `a#b` and NUL each arrived as one encoded path segment. `U+3000` and `U+00A0` were `RequestNotBuilt` with nothing sent. The encoded set matches `aws-sdk-s3` 1.155.0 `endpoint_lib/uri_encode.rs`.
- `crates/validate/src/s3.rs:506`: endpoints `HTTP://`, `/base/`, `/a%20b`, a fragment, an empty query and no scheme all behaved as documented. The cited `merge_paths` and `parse_url` lines match the SDK source.
- `crates/validate/src/s3/verbs.rs:58`: an upload id needing encoding reached the gateway encoded. An empty key, an empty upload id and a newline in `source_key` were each `RequestNotBuilt` with nothing sent.
- `crates/validate/src/s3/verbs.rs:288`: sending `<Quiet>` true is caught by the gateway test.
- `crates/validate/src/s3.rs:191`: removing `disable_s3_express_session_auth` changes no test, and the comment above it already says it only guards against a later SDK. This is not a defect.
- Not filed, as they predate this patch:
  - An endpoint with userinfo (`http://u:p@host`) gets a `403` `SignatureDoesNotMatch` here and also with the SDK's own resolver back in place.
  - `get_object` and `delete_object` report a refused `ftp://` endpoint as `NoResponse`. They use the older `classify` path this patch does not change; I confirmed that by reading, not by running the base.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No actionable findings on either lens: no introduced correctness bug or worthwhile reuse, simplification, or efficiency change found in the resolver (`crates/validate/src/s3.rs:491`), probe dispatch (`crates/validate/src/s3/verbs.rs:331`), success-body handling (`crates/validate/src/s3/answer.rs:343`), delete-row parser (`crates/validate/src/s3/document.rs:197`), or new request assertions (`crates/validate/tests/s3_client_smoke_verbs.rs:709`).
- The frozen T4 empty-`DeleteResult` concern is not repeated as an implementation finding. At `crates/validate/src/s3/verbs.rs:321`, `Ok(DeleteResult)` reports parsed rows, including error rows; it does not certify that the requested deletions succeeded. The client explicitly leaves answer judgment to 743.1 (`crates/validate/src/s3/verbs.rs:7`), which the brief excludes from this diff.

Validation evidence: frozen CI passed; verification was green-only; diff coverage was not measured; mutation testing reported 20 caught, 13 unviable, and 3 timeouts, with no surviving mutants reported. This review inspected the target source without modifying it or rerunning builds.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm the merged and closed/rejected prior-art disposition for all six affected paths — the brief records source-path searches, but this one-commit snapshot cannot independently establish history and supplies no test/doc-path search evidence; `brief.md:207`, `reviewer-prior-art.log:10`.
- [ ] Validation — fitness-to-purpose — Accept this client-only deliverable for #939’s runner — wire evidence passes, while scenario judgment and scratch-bucket ownership checks belong to #937/#939 and are not demonstrated by this child; `brief.md:24`, `brief.md:141`, `brief.md:189`.
- [ ] `crates/validate/src/s3/verbs.rs:288`: **a delete result that accounts for no key is reported as success.** `delete_objects(&["a", "b"])` answered with `200` and `<DeleteResult/>` returned `Ok([])`, so `deleted()` and `errors()` are both empty. A result with only `<Deleted><Key>a</Key></Deleted>` returned `Ok`, with `b` unaccounted for. A result naming only `c`, which was never requested, also returned `Ok`. The doc comment says "the result names every key deleted as well as every key not", and that is false for these answers. `docs/design/architecture/05-building-block-view.md:303` says "never a partial listing or a short delete result", and these are short delete results. This is the rubric's "absent entries" class, and it is the same defect the gating T4 row still blocks on (`gate-logs/T4-batch-review.log:10`). The brief leaves judging answers to 743.1, so the client need not refuse. It can report instead, for example with the requested keys that have no row and the rows that name no requested key, which fills in nothing on the server's behalf. Either way the two sentences need correcting and the case needs a test.
- [ ] `crates/validate/src/s3/document.rs:221`: **a failed key can vanish in a row that goes unread.** `_ => {}` skips any child it does not recognise. `<Deleted><Key>a</Key></Deleted><error><Key>b</Key><Code>AccessDenied</Code></error>` returned `Ok([Deleted { key: "a" }])`, so `errors()` is empty for a key the server said it did not delete. The brief says the `<Error>` rows are the only evidence of a failed key (`brief.md:40`), and the rubric rejects silent skips. The test that pins the skip (`crates/validate/tests/s3_client_smoke_verbs.rs:2635`) uses `<RequestCharged>`, which the SDK reads from a header for this operation (`aws-sdk-s3` 1.155.0 `protocol_serde/shape_delete_objects.rs:38`). Its deserializer reads only `Error` and `Deleted` as elements (same file, `:143`, `:160`), so that test does not show a real element that needs skipping. Refuse unknown children, or report them.
- [ ] `crates/validate/src/s3/document.rs:209`: **an empty `<Key/>` counts as naming a key.** `<Deleted><Key/></Deleted><Error><Key></Key><Code>x</Code></Error>` returned `Ok([Deleted { key: "" }, Error(KeyError { key: "", .. })])`. The check at `:209` and `:214` catches only a missing `<Key>`, although its refusal reads "names no `<Key>`". An empty `<Code>` is already read as absent (`named_code`), so the two fields are treated differently.
- [ ] `crates/validate/src/s3/verbs.rs:56`: **the probe docs describe request bodies that no test checks.** The relay and the request checks record heads only (`crates/validate/tests/s3_client_smoke_verbs.rs:661`, `:709`). Three mutations each left all 45 tests passing:
- [ ] `docs/design/architecture/05-building-block-view.md:303`: **the safety sentence claims more than the client does.** "a probe can name a bucket those rules forbid, and so cannot create or delete one it does not own" reads as a property of the client, but the client checks no name. `Probe::DeleteBucket { bucket: "victim" }` is sent as given, and the safety comes only from 743.3 choosing a forbidden name. The sentence also drops the exception in `crates/validate/src/s3/verbs.rs:44`: I ran `..` through the relay and it was sent as `DELETE /../`, which a server that resolves dot segments reads as the endpoint's parent. That verbs.rs sentence is accurate but has no test, because `MORE_FORBIDDEN_BUCKETS` (`crates/validate/tests/s3_client_smoke_verbs.rs:150`) has no `.` or `..` row. Reword the sentence so the safety depends on the caller's name, carry the dot-segment exception over, and add both rows.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- [ ] **The rejection criterion can pass for the wrong multipart operation.** Brief lines 27–30 require six multipart probes to return `501 NotImplemented`, but the target rejects any recognized bucket subresource through the same branch (`crates/gateway-s3/src/lib.rs:1797–1802`) and any recognized object subresource through another shared branch (`:1836–1847`). A `ListParts` probe accidentally sending `ListMultipartUploads` can therefore satisfy that outcome. Criterion G (brief lines 58–60) supplies success responses but does not require checking the request that elicited them. Require captured method/path/query/header/body assertions for each probe, so a wrong operation or missing operation parameter cannot pass merely by hitting a rejection guard.
- [ ] **Criterion B contradicts the existing timeout contract.** Brief lines 37–39 say a request that reached the connector and got no answer “stays `NoResponse`.” The target explicitly reports SDK operation timeouts as `Timeout { phase: Operation }` and connector timeouts as `Timeout { phase: Connect }` (`crates/validate/src/s3.rs:346–355`); these also receive no answer. Qualify the criterion to preserve typed deadline failures, reserve `NoResponse` for non-timeout transport failures, and require probe assertions for both deadline phases. Otherwise the stated acceptance rule permits losing the measured timeout phase.
- [ ] **The mandatory salvage prerequisite is unresolved in the supplied material.** Brief lines 7–9 require taking the prior patch's hunks unchanged and forbid re-deriving the design; lines 67, 89–95 and 135–137 also depend on its exact architecture paragraph, implementation and SDK evidence. Neither the review cwd nor `$PDCA_TARGET` contains `results/issue_938/iteration-v3/patch.diff` or its `build-notes.md`; `notes.json` and `sources/` are also absent. This does not prove those artifacts are missing elsewhere, but their availability and the claimed prior red/green evidence cannot be established here. Revise the brief to make the required salvage an explicitly supplied, resolvable input, or include the necessary contract and remove the exclusive instruction to transplant unavailable hunks. Include the relevant tracker excerpts so the bucket-safety constraint and prior-review disposition can be checked.
- [ ] size backstop — this slice is behaving oversized: patch is 138 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Keep the routing and probe code; it held up. Fix these: 1. document.rs:221: do not skip unknown children of <DeleteResult>. Refuse them or report them. Replace the <RequestCharged> test, since the SDK reads that from a header. 2. Report requested keys with no row and rows naming no requested key, without judging them (judging stays with #937). Test empty, partial and unrequested-key answers. Correct verbs.rs:288 and the 05-building-block-view.md:303 wording. This clears T4's finding at verbs.rs:325; if T4 still raises it, record a scope-based rejection in review-rejected.md. 3. document.rs:209: treat an empty <Key/> as no key, matching named_code. 4. Record request bodies in the relay and assert UploadPart, CompleteMultipartUpload and CreateBucket. 5. Reword the bucket-safety sentence so safety depends on the caller's name, carry over the dot-segment exception, and add . and .. rows to MORE_FORBIDDEN_BUCKETS.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
