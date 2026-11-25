# Result — issue 955 / validate-s3-client-reads-and-listings

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on `main` 955b9cb). Smoke (743.3) also
  needs HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a
  prefix, followed across pages. Each must report what the server actually sent (status, every
  header field line as sent; for an S3 error its `<Code>` and `<Message>`, or which of the
  base's `ErrorCode` cases says why there is none; otherwise the body as sent) with #853's
  bounded, typed reading.
  A 2xx head must never be lost: when a body fails after a 2xx head, the report still carries
  the 2xx. The SDK deserializes these answers itself, so today a listing cut short, a body that
  stalls after a 200, or a 304 with no body cannot be reported as what the server sent.
- Success criterion: the NEW test file `crates/validate/tests/s3_client_smoke_verbs.rs`
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
  - **(H) An S3 error through every new verb keeps its status, headers and code** (added
    2026-10-23; the existing nonconforming suite covers only GET, PUT and DELETE,
    `s3_client_nonconforming_responses.rs:634-641`). Against the real gateway:
    `list_objects_v2` and `list_objects_v1` of a bucket with no record →
    `ListingError::Page { page: 1, cause: NoResult::Answered(_) }`, status 404,
    `Outcome::Error` with `ErrorCode::Code("NoSuchBucket")`
    (`crates/gateway-s3/src/lib.rs:935-944`); `get_object_range` and
    `get_object_if_none_match` of a missing key → `Fetched::Other`, 404,
    `ErrorCode::Code("NoSuchKey")` (`:2834-2841`). Against the scripted endpoint, one `403`
    carrying an `x-amz-request-id` line, a header sent twice, and the body
    `<Error><Code>AccessDenied</Code><Message>Access Denied</Message></Error>` answers each of
    `get_object_range`, `get_object_if_none_match`, `list_objects_v2` and `list_objects_v1`:
    each reports status 403, every header line as sent (the repeated one as both lines),
    `ErrorCode::Code("AccessDenied")` and the message `Access Denied`. The same `403` with a
    well-formed `<Error>` that has no `<Code>` → `ErrorCode::MissingInXml`; with a body that
    is not one whole `<Error>` document (cut short; another root) → `Outcome::Unreadable`
    carrying the body as sent, never an `Error` with a code filled in; `head_object` answered
    `403` with no body → `ErrorCode::NoBody`. `ErrorCode` and its three cases are the base's
    (`crates/validate/src/s3/error.rs:64-76`); `Outcome`, `Fetched`, `NoResult` and
    `ListingError` are the salvage's.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the reply-capture core and the read verbs in `crates/validate/src/s3.rs` (wiring
  only) and `crates/validate/src/s3/{answer,document,response,verbs,body,error}.rs`;
  `wyrd-traits` under `[dev-dependencies]`; the new test file. / out of scope: bulk delete,
  the probes, the endpoint resolution, and the architecture doc (#956); judging answers
  (743.1); the runner, ledger, version source and arguments (743.3); `lib.rs`; the multipart
  success path (#508, #750); any change to the existing verbs' behaviour; any `wyrd-*` crate
  in `[dependencies]`; any new production dependency; `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.2% — 573 of 602 instrumentable changed lines executed (floor 80%); 602 of 1341 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 113 mutants tested in 18m: 78 caught, 35 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_955/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.52s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #955: add faithful S3 HEAD, ranged/conditional GET, and paginated v1/v2 listing capture; no implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Real-gateway results and adversarial replies make faithful observation falsifiable, including status preservation after body failure; `brief.md:41`, `crates/validate/tests/s3_client_smoke_verbs.rs:782`. |
| C2 Reproduction (red pre-fix) | N/A | This is a declared new feature with no baseline verbs; independently reverting production yielded 43 compilation errors before any test ran, not a behavioral red (`brief.md:156`, `reviewer-verification.log:468`). |
| C3 Change | PASS | The approved read-only slice is delivered within its assigned files; architecture documentation remains explicitly deferred to #956 under the standing protocol (`crates/validate/src/s3/verbs.rs:42`, `reviewer-grounding.log:1`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API verification exception or require separately captured behavioral mutation reds — 90 tests pass after restoration, but the baseline cannot execute and the frozen mutation log supplies aggregate counts only (`reviewer-verification.log:468`, `reviewer-verification.log:596`, `gate-logs/C5-mutants.log:13`). |
| C5 Causal adequacy | PASS | Capturing the head before SDK transformation addresses lost observations directly; strict document reading and checksum configuration address their causes, with no capability-probe/load-time symptom guard found (`crates/validate/src/s3/answer.rs:457`, `crates/validate/src/s3/verbs.rs:34`, `crates/validate/src/s3/response.rs:462`). |
| T1 Structure | PASS | The production dependency boundary remains blackbox, with Wyrd coupling confined to gateway test setup; the independently executed dependency guard agrees (`crates/validate/Cargo.toml:55`, `reviewer-scanners.log:237`). |
| T2 Shape | PASS | Distinct no-answer, answered-failure, and streaming outcomes preserve the information callers need without making correctness judgments for them (`crates/validate/src/s3/answer.rs:274`, `crates/validate/src/s3/answer.rs:306`, `crates/validate/src/s3/document.rs:43`). |
| T3 Runtime | PASS | Stalls, trickles, malformed framing, and endless pagination terminate with typed results; the body deadlines share Tokio's clock and tested bounds prevent unbounded buffering or continuation (`crates/validate/src/s3/answer.rs:547`, `crates/validate/src/s3/document.rs:21`, `reviewer-verification.log:592`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit is owed to the mandatory publish rerun, not a human waiver (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the recorded 2026-10-23 path-based prior-art search is sufficient for this submission — the supplied target has one synthetic base commit and no remotes, so merged and closed/rejected history cannot be independently settled here (`brief.md:187`, `reviewer-grounding.log:11`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve this read slice for the planned #956/#939 consumers — exercised wire behavior supports the contract, while downstream end-to-end fitness remains a human decision (`brief.md:163`, `crates/validate/src/s3/verbs.rs:107`). |

The independent checks support the implementation; they do not establish a behavioral pre-fix red. In the disposable target, `git stash push` reverted tracked production changes while retaining the new test, the smoke command failed to compile, and `git stash pop` restored the patch. The restored run passed seven library tests, 28 smoke tests, and the three unchanged existing suites: 28 nonconforming-response, 15 roundtrip, and 12 upload-peer tests. Full commands, compiler errors, restoration, and results are captured in `reviewer-verification.log:1`, `reviewer-verification.log:492`, and `reviewer-verification.log:616`. The missing new APIs explain the red-leg limitation; it is not a patch compilation defect. The supplied evidence does not individually identify the four behavioral mutations required at `brief.md:159`, so the aggregate mutant result is not presented as proof of those specific reds.

The earlier carry-forward issues are addressed and exercised. Empty v1 continuation keys are refused on page 1 (`crates/validate/src/s3/document.rs:130`; test at `crates/validate/tests/s3_client_smoke_verbs.rs:1406`). Both GET variants preserve checksum-bearing replies without asking for checksum validation (`crates/validate/tests/s3_client_smoke_verbs.rs:2262`). Empty listing bodies, exact-budget entries including ETags, malformed listing fields, legal multi-coding HEAD/304 headers, forbidden framing, chunked GETs, and truncated GET bodies have substantive assertions (`crates/validate/tests/s3_client_smoke_verbs.rs:1029`, `:1121`, `:1258`, `:1647`, `:1783`, `:2075`, `:2130`, `:2182`). The shared parser extraction preserves the older error-reader behavior under the unchanged suites. The architecture-doc and S3 Express endpoint deferrals to #956 remain settled (`crates/validate/src/s3/verbs.rs:42`, `:206`).

Independent scanners passed: real `typos` 1.48.0, workspace formatting, `wyrd-validate` clippy for all targets with warnings denied, the repository guards executed by `cargo xtask ci-dry-run`, and docs lint (`reviewer-scanners.log:1`, `:226`, `:243`, `:246`). The dry run executes guards, not the full CI cargo sequence. The brief's external dependency, `typos`, was present and exercised.

The remaining gate conclusions are based on the frozen logs, not claimed as independent reruns. The instance-scoped wrapper invocations are recorded in their headers. Full CI shows workspace build/tests, dependency walls, conformance, statics, deployment guard, DST, and final success (`gate-logs/C4-ci.log:323`, `:3885`, `:3899`, `:4525`). Coverage reports 573/602 instrumentable changed lines executed, 95.2%, with 739 changed lines unscored (`gate-logs/C4-diff-cov.log:103`). Mutation evidence reports 113 tested: 78 caught, 35 unviable, zero missed (`gate-logs/C5-mutants.log:10`). The batched review reports zero blocking findings, without individual review detail (`gate-logs/T4-batch-review.log:10`). Both TiKV feature clippy commands completed (`gate-logs/host-tikv.log:112`, `:211`). Contribution checking is explicitly deferred to publish (`gate-logs/T4-contribution.log:10`).

Source citations above resolve against `$PDCA_TARGET`; brief and log citations resolve against this review directory. All ten patch postimages match the restored target, and reverse-apply checking succeeds (`reviewer-grounding.log:1`). The prior-art entry explicitly names affected paths, merged work, closed-unmerged results, and rejected parent iterations, but its historical results cannot be reconstructed from the isolated snapshot. No `INTEGRATION.md` was supplied or found in that target (`reviewer-grounding.log:16`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 955 (S3 client read verbs and listings)

**Result: partly refuted.** The red→green evidence holds, and I found no wrong answer for any input the brief lists. I did find one input outside the brief's list that loses a 2xx head, and three production behaviours that no test protects. I re-ran everything in a scratch copy with the toolchain present, so none of this is provisional.

## Findings

- NEEDS-HUMAN [human] — **A 200 head is lost when hyper (the HTTP library under the SDK) refuses its `Content-Length`.** `crates/validate/src/s3/answer.rs:309-310` says `NoAnswer` means no response head arrived, and `crates/validate/src/s3/error.rs:41-42` says the same of `NoResponse`. That is false for this case. I scripted `HTTP/1.1 200 OK` carrying a whole, valid listing page, with `Content-Length: abc`. `list_objects_v2` returned `Page { page: 1, cause: NoAnswer(NoResponse { detail: "dispatch failure: io error: invalid content-length parsed ..." }) }`. These variants gave the same result: `+440`, two differing lengths, a value past `u64`, and `HTTP/1.0` with `Transfer-Encoding`. A `206` and a `403` to `get_object_range` with `Content-Length: abc` also came back `Err(NoResponse)`. The same bad header on `head_object` keeps its status, as `Answer { status: 200, BodyFailed(Framing { content_length: Some("abc") }) }`, because hyper does not parse the length on a HEAD, `204` or `304` (`hyper` 1.12.0 `proto/h1/role.rs:1301-1307`, against `:1336-1337`). So one bad head gets two opposite reports, and `crates/validate/src/s3/answer.rs:176-178` lists this exact fault as a refusal kept beside the status. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1999-2002` only covers `gzip` and `Transfer-Encoding` plus `Content-Length`, so the gap never shows. It goes against the brief's invariant ("what the client reports is what the server sent"), though strictly the head fails here, not the body. It is not a regression, since the base's `get_object` goes through the same connector. A builder cannot simply fix it: hyper returns no status with this error, and telling it apart from "no response" means changing `crates/validate/src/s3/wire.rs:130-136`, which the older verbs share and the brief puts out of scope. **Decision needed:** accept this as a limit and correct the docs at `answer.rs:34-35`, `:176-178` and `:309-310`, or open an issue for a separate "response refused by the HTTP parser" report. Decide before 743.1 reads `NoResponse` as a network failure.

- NEEDS-HUMAN [impl] — **No test proves the deadlines reach a ranged or conditional GET's 2xx body.** `crates/validate/src/s3/verbs.rs:278-279` passes `body_idle` and `body` to `ObjectBody::new`. I replaced both with 3600 seconds and all 28 tests still passed; separately, replacing either one alone also passed. Production is correct today: a stalled `206` or `200` gave `Timeout { phase: BodyIdle, limit: 500ms }` at about 501 ms, and a trickled one gave `Timeout { phase: Body, limit: 1.5s }` at about 1.50 s. The rubric's await-discipline class applies, and C5 cannot catch this because it does not swap arguments. Failing case to add, for both `GET_VERBS`: `Content-Length: 100`, 10 bytes, `Then::Hold`, then a `Then::Trickle` version with `short_deadlines()`, asserting those two timeouts. With the 3600-second change my version of this test fails.

- NEEDS-HUMAN [impl] — **No test pins why the head is captured at `read_after_transmit`.** `crates/validate/src/s3/answer.rs:11-16` gives the reason: the SDK drops an `Expires` it cannot parse and adds its own `ExpiresString` at the next hook. I re-captured the head in `modify_before_deserialization` (`answer.rs:475-484`) and all 28 tests still passed, and the test file never mentions `Expires`. Production is correct today. Failing case to add: `head_object` answered with `Expires: not-a-date` must report `header_lines("expires") == ["not-a-date"]` and no `expiresstring` line. My version passes on the patch and fails with the head captured late, so it does break the brief's "every header field line as sent".

- NEEDS-HUMAN [impl] — **The empty-token leg of (G) has no test for `ListObjectsV2`.** Deleting `.filter(|token| !token.is_empty())` at `crates/validate/src/s3/document.rs:127` leaves all 28 tests passing. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1349` removes the token element and never sends an empty one. The empty case was pinned only for v1 (`:1406`, from the iteration 1 carry-forward). Production is correct today. Failing case to add: a truncated page built with `listing_v2(&["list/a"], Some(""))` must be `Page { page: 1, Answered(Unreadable) }` with status 200 and exactly one request. With the filter removed, the client sends a second request with an empty continuation token.

- NEEDS-HUMAN [human] — **Check that the T4 review really ran in full.** `check-gates.json` reports "3x codex, union, triaged" with 0 blocking, and `gate-logs/T4-batch-review.log` shows it took 29.76 s on a 183 KB patch. The same gate found 3 blockers last round. The log holds one summary line, and `review-batch.md` is not in my inputs, so I cannot tell whether this was a quick clean pass, a cached result or a failed run counted as clean. I have no evidence that it is wrong. Since the rubric's definition of done depends on a deep review, please open that file before relying on the 0. Its summary line counts no findings of any kind, so it did not count the three test gaps above as blocking; they are small, but they are the kind of gap it flagged last round.

## Weaker points, not flagged for action

- `crates/validate/src/s3/answer.rs:24-25` says only a `304` can reach the SDK's length check. A scripted `101` with `Content-Length: 5` also reaches it, and reports `Transport { detail: "Invalid Content-Length: ..." }` beside the kept status. That is a harmless inaccuracy on an answer no server should send.
- `Capture::new(false)` on `get_object_range` or either listing (`crates/validate/src/s3/verbs.rs:68`, `:123`, `:157`) survives the suite, because only the conditional GET and HEAD have a `304`/`Transfer-Encoding` case. Those requests carry no condition, so a real server has no reason to send that `304`.
- A `204` to a GET with no framing reports `Fetched::Object { status: 204, body: Err(LengthUndeclared) }` (`crates/validate/src/s3/verbs.rs:273`). The wording is odd for an answer that has no body, but the status is kept and the base's `object_length` gives the same result.

## Attacked and could not refute

- **Red→green.** C4-verify is `unverifiable` as the brief pre-declared, because the test cannot compile without the verbs. `build-notes.md` is withheld from me, so I made the required reds myself: red 1 (body failure reported as no answer) failed 3 tests, red 2 (cut listing accepted) failed 3, red 4 (stop after one page) failed 3 on each listing, and red 15 (every error is `NoBody`) failed 2. On the unchanged patch, the smoke suite passed 28 of 28 on 8 runs, and the (E) suites passed 15, 12 and 28 with their files unmodified. The test failures I saw came from real assertions, not build errors.
- **Production path.** The tests drive the real `S3Client`, SDK and connector against a real gateway or a loopback socket. Nothing is mocked or copied. Every added line of `patch.diff` is in the target tree, `lib.rs` is untouched, and the C4-verify compile errors cite the same test line numbers as this patch.
- **Other hand-made logic changes the suite did catch.** Budget off by one, budget not enforced, whole-body deadline never firing, HEAD not recognised, checksum wrapper left on for each GET, `max-keys` or prefix ignored on each listing, v1 last key preferred over `<NextMarker>`, ETag dropped, duplicate field accepted, overrun read as transport, and pages not recorded.
- **Awkward input, all correct.** Against the real gateway, across 1, 2 and 3 keys a page on both listings: keys with a space, `&`, `+`, `<>`, `ü`, `%41`, a tab, quotes and `?`. On a scripted endpoint: keys of only spaces, and `&#13;`. Also `Content-Length` repeated or as `5, 5` on a page and a `304`; a non-UTF-8 `Transfer-Encoding` on a `304`; a bad `Last-Modified` on HEAD and a ranged GET; a newline in an ETag and an empty key (both `RequestNotBuilt`); and `page_size` 0.
- **Rubric classes.** Listing size is bounded at 100 pages of 64 KiB each. A later bad page discards earlier ones. No new crate, no new clock source (the new read names its own), no shared mutable state. The architecture doc is deferred to #956 in the code, so I treat that as settled and did not raise it.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. No introduced correctness bugs or actionable reuse, simplification, or efficiency issues were identified in this diff.

Reviewed the response capture, body framing and deadlines, listing parsing and continuation, shared-helper refactors, and new tests against `$PDCA_TARGET`. Validation used the frozen gate evidence; tests were not rerun. CI passed all 28 new smoke tests, and mutation testing reported 78 caught, 35 unviable, and zero missed. The pre-fix compile failure matches the brief's declared verification limitation.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the declared new-API verification exception or require separately captured behavioral mutation reds — 90 tests pass after restoration, but the baseline cannot execute and the frozen mutation log supplies aggregate counts only (`reviewer-verification.log:468`, `reviewer-verification.log:596`, `gate-logs/C5-mutants.log:13`).
- [x] T5 Judgment — Confirm the recorded 2026-10-23 path-based prior-art search is sufficient for this submission — the supplied target has one synthetic base commit and no remotes, so merged and closed/rejected history cannot be independently settled here (`brief.md:187`, `reviewer-grounding.log:11`).
- [x] Validation — fitness-to-purpose — Approve this read slice for the planned #956/#939 consumers — exercised wire behavior supports the contract, while downstream end-to-end fitness remains a human decision (`brief.md:163`, `crates/validate/src/s3/verbs.rs:107`).
- [x] **A 200 head is lost when hyper (the HTTP library under the SDK) refuses its `Content-Length`.** `crates/validate/src/s3/answer.rs:309-310` says `NoAnswer` means no response head arrived, and `crates/validate/src/s3/error.rs:41-42` says the same of `NoResponse`. That is false for this case. I scripted `HTTP/1.1 200 OK` carrying a whole, valid listing page, with `Content-Length: abc`. `list_objects_v2` returned `Page { page: 1, cause: NoAnswer(NoResponse { detail: "dispatch failure: io error: invalid content-length parsed ..." }) }`. These variants gave the same result: `+440`, two differing lengths, a value past `u64`, and `HTTP/1.0` with `Transfer-Encoding`. A `206` and a `403` to `get_object_range` with `Content-Length: abc` also came back `Err(NoResponse)`. The same bad header on `head_object` keeps its status, as `Answer { status: 200, BodyFailed(Framing { content_length: Some("abc") }) }`, because hyper does not parse the length on a HEAD, `204` or `304` (`hyper` 1.12.0 `proto/h1/role.rs:1301-1307`, against `:1336-1337`). So one bad head gets two opposite reports, and `crates/validate/src/s3/answer.rs:176-178` lists this exact fault as a refusal kept beside the status. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1999-2002` only covers `gzip` and `Transfer-Encoding` plus `Content-Length`, so the gap never shows. It goes against the brief's invariant ("what the client reports is what the server sent"), though strictly the head fails here, not the body. It is not a regression, since the base's `get_object` goes through the same connector. A builder cannot simply fix it: hyper returns no status with this error, and telling it apart from "no response" means changing `crates/validate/src/s3/wire.rs:130-136`, which the older verbs share and the brief puts out of scope. **Decision needed:** accept this as a limit and correct the docs at `answer.rs:34-35`, `:176-178` and `:309-310`, or open an issue for a separate "response refused by the HTTP parser" report. Decide before 743.1 reads `NoResponse` as a network failure.
- [x] **No test proves the deadlines reach a ranged or conditional GET's 2xx body.** `crates/validate/src/s3/verbs.rs:278-279` passes `body_idle` and `body` to `ObjectBody::new`. I replaced both with 3600 seconds and all 28 tests still passed; separately, replacing either one alone also passed. Production is correct today: a stalled `206` or `200` gave `Timeout { phase: BodyIdle, limit: 500ms }` at about 501 ms, and a trickled one gave `Timeout { phase: Body, limit: 1.5s }` at about 1.50 s. The rubric's await-discipline class applies, and C5 cannot catch this because it does not swap arguments. Failing case to add, for both `GET_VERBS`: `Content-Length: 100`, 10 bytes, `Then::Hold`, then a `Then::Trickle` version with `short_deadlines()`, asserting those two timeouts. With the 3600-second change my version of this test fails.
- [x] **No test pins why the head is captured at `read_after_transmit`.** `crates/validate/src/s3/answer.rs:11-16` gives the reason: the SDK drops an `Expires` it cannot parse and adds its own `ExpiresString` at the next hook. I re-captured the head in `modify_before_deserialization` (`answer.rs:475-484`) and all 28 tests still passed, and the test file never mentions `Expires`. Production is correct today. Failing case to add: `head_object` answered with `Expires: not-a-date` must report `header_lines("expires") == ["not-a-date"]` and no `expiresstring` line. My version passes on the patch and fails with the head captured late, so it does break the brief's "every header field line as sent".
- [x] **The empty-token leg of (G) has no test for `ListObjectsV2`.** Deleting `.filter(|token| !token.is_empty())` at `crates/validate/src/s3/document.rs:127` leaves all 28 tests passing. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1349` removes the token element and never sends an empty one. The empty case was pinned only for v1 (`:1406`, from the iteration 1 carry-forward). Production is correct today. Failing case to add: a truncated page built with `listing_v2(&["list/a"], Some(""))` must be `Page { page: 1, Answered(Unreadable) }` with status 200 and exactly one request. With the filter removed, the client sends a second request with an empty continuation token.
- [x] **Check that the T4 review really ran in full.** `check-gates.json` reports "3x codex, union, triaged" with 0 blocking, and `gate-logs/T4-batch-review.log` shows it took 29.76 s on a 183 KB patch. The same gate found 3 blockers last round. The log holds one summary line, and `review-batch.md` is not in my inputs, so I cannot tell whether this was a quick clean pass, a cached result or a failed run counted as clean. I have no evidence that it is wrong. Since the rubric's definition of done depends on a deep review, please open that file before relying on the 0. Its summary line counts no findings of any kind, so it did not count the three test gaps above as blocking; they are small, but they are the kind of gap it flagged last round.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The acceptance criteria can pass while the new capture core discards S3 error codes.** `brief.md:19-20` requires the server's S3 error code and body, but the specified cases (`brief.md:27-53`, `:93-111`) exercise successful reads/listings, malformed 200 listings, and bodiless 404/304 responses; none requires an XML error response through a new verb. Keeping the existing nonconforming-response suite green does not establish this: on `$PDCA_TARGET`, `crates/validate/tests/s3_client_nonconforming_responses.rs:634-641` limits its operations to Get/Put/Delete, and `:661-680` calls the existing methods. A mutation making every new-verb error report `ErrorCode::NoBody` could satisfy the specified assertions. Add a scripted new-verb case asserting the actual status, headers, XML code and captured body, plus the missing-code and malformed-document distinctions; keep those assertions in this child's acceptance criteria.
- [x] **The mandatory salvage is an unresolved implementation dependency.** `brief.md:9-11` orders Do to take `results/issue_938/iteration-v3/patch.diff` hunks unchanged and forbids re-deriving the design; `:72-82` and `:105-111` depend on that artifact for the split, API wiring and adapted tests. Neither that patch nor its cited `build-notes.md` exists in the supplied cwd or `$PDCA_TARGET`, and the brief supplies no resolvable artifact location/revision. Revise the brief to point to an artifact actually made available to Do, or remove the mandatory-salvage instruction and include enough API/split detail to implement independently. Also distinguish inherited evidence from the two new tests: `brief.md:58-59` says “Every leg was shown red and green,” while `:105-111` explicitly says the listing-fault and conditional-GET 304 tests are absent from the salvage.
- [x] size backstop — this slice is behaving oversized: patch is 179 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #956 must carry from #955: add tests for the time limits on partial and conditional GET bodies, the early header capture with a bad `Expires`, and the empty ListObjectsV2 token.
- #956 must correct the `answer.rs` and `error.rs` comments: a reply the HTTP library rejects is reported as `NoResponse`.
- File an issue: report "response refused by the HTTP parser" separately from `NoResponse`, before 743.1 relies on it (`wire.rs:130-136`).
