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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 89.4% — 507 of 567 instrumentable changed lines executed (floor 80%); 567 of 1207 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 103 mutants tested in 28m: 5 missed, 64 caught, 34 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_955/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 32.41s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #955’s S3 read verbs and faithful reply capture: one pagination defect and one regression-test gap need builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required read/error outcomes are falsifiable and the new-API verification exception is explicit; `brief.md:37`, `brief.md:156`, `crates/validate/tests/s3_client_smoke_verbs.rs:18`. |
| C2 Reproduction (red pre-fix) | N/A | These APIs do not exist on the base: independently stashing production while retaining the test produces 30 compilation errors, not an executed behavioral red; `reviewer-red.log:345`, `brief.md:156`. |
| C3 Change | PASS | The change stays within the authorized read-client slice, preserves existing test files and production dependencies, and leaves the tracked #956 work separate; `crates/validate/Cargo.toml:65`, `crates/validate/src/s3/verbs.rs:186`, `brief.md:125`. |
| C4 Verification (red→green) | PASS | The advertised cases have executable evidence: 70 client tests pass, five targeted behavioral mutants fail assertions, and the original 15-test suite passes afterward; the base compilation limitation is recorded under C2, and the uncovered defect under C5/T3; `reviewer-restored-tests.log:100`, `reviewer-required-reds.log:26`, `reviewer-post-mutation-green.log:19`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Reject an empty fallback continuation and pin empty-200 classification—otherwise pagination can restart and still report a complete listing, while a meaningful taxonomy mutation escapes every shipped test; `crates/validate/src/s3/document.rs:133`, `crates/validate/src/s3/answer.rs:349`, `reviewer-probes.log:19`, `reviewer-probes.log:57`. |
| T1 Structure | PASS | Shared framing/XML readers preserve the existing client boundary, and mutable capture state belongs to individual requests rather than global state; `crates/validate/src/s3/response.rs:462`, `crates/validate/src/s3/answer.rs:405`, `crates/validate/Cargo.toml:20`. |
| T2 Shape | PASS | Typed outcomes retain response heads alongside body failures; page and body budgets bound accumulation and no new production dependency is introduced; `crates/validate/src/s3/answer.rs:131`, `crates/validate/src/s3/document.rs:21`, `crates/validate/src/s3/answer.rs:550`. |
| T3 Runtime | FAIL | A truncated v1 page whose last key is empty and whose marker is absent sends `marker=` and can return `Ok(Listing)` after a second page instead of refusing page 1; `crates/validate/src/s3/document.rs:133`, `crates/validate/src/s3/verbs.rs:144`, `reviewer-probes.log:16`. |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; the substantive audit must rerun at publish, so the deferred row is neither green nor a missing-evidence concern; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Path-based merged history and all 19 closed-unmerged PR file lists show no competing read-client implementation; #956 endpoint/docs deferrals remain settled, and no capability-probe symptom guard was introduced; `reviewer-prior-art.json:2`, `brief.md:125`, `crates/validate/src/s3/verbs.rs:186`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the read API and failure taxonomy satisfy the forthcoming #939/#956 consumers after the implementation findings are addressed—component tests cannot establish fitness of those future integrations; `brief.md:163`, `crates/validate/src/s3/answer.rs:138`. |

The pagination defect is reproducible against the unmodified patched library. All source citations beginning `crates/` refer to `$PDCA_TARGET`; the target matches `patch.diff`, reverses cleanly, and its production hashes remained unchanged after the stash/pop check. The following findings are advisory, not acceptance gates.

1. **P2 — Refuse an empty last-key continuation in v1.** At `crates/validate/src/s3/document.rs:130`, the explicit marker is filtered for emptiness, but the fallback last key is not. A valid XML page with `<IsTruncated>true</IsTruncated><Contents><Key></Key><Size>7</Size></Contents>` and no `<NextMarker>` therefore yields an empty continuation. The independent loopback probe observed a second request containing `marker=` and then `Ok(Listing)` with both pages when the endpoint answered that request with an ordinary final page (`reviewer-probes.log:16`, `reviewer-probes.log:19`). This violates the specified refusal when a truncated page names nothing to continue from and the parser's own stated contract (`brief.md:65`, `crates/validate/src/s3/document.rs:88`). Filter the fallback key for emptiness and add a regression covering an absent or empty marker with an empty final key. The refusal must retain the first page's 200 head and must not issue a continuation request.

2. **P3 — Protect empty 200 listing bodies from being classified as S3 errors.** Current production correctly reports `Outcome::Unreadable`; the shipped tests do not distinguish that from `Outcome::Error { code: NoBody, message: None }`. Removing only the nonempty-body guard at `crates/validate/src/s3/answer.rs:349` leaves all 15 existing smoke-verb tests green (`reviewer-probes.log:57`), independently reproducing the meaningful survivor in `gate-logs/C5-mutants.log:15`. An added test sending a complete 200 response with `Content-Length: 0` passes against production and fails against this mutant (`reviewer-probes.log:9`, `reviewer-probes.log:66`). Add this case for both listing versions, asserting the retained 200 and empty `Unreadable` body. This is a test gap, not a claim that current production misclassifies that response.

The reproductions are runnable in this leaf without editing the reviewed source:

```sh
./pdca-reviewer-955-probes/baseline reviewer_v1_empty_last_key_cannot_continue --exact --nocapture
./pdca-reviewer-955-probes/baseline reviewer_empty_200_listing_is_unreadable --exact --nocapture
./pdca-reviewer-955-probes/mutant reviewer_empty_200_listing_is_unreadable --exact --nocapture
```

The first command fails on the pagination defect; the second passes; the third fails on the isolated taxonomy mutation. `reviewer-probes.py` records the test construction and exact compiler dependencies. Mutations and added probes exist only in the reviewer scratch directory, not in the patch.

Independent verification supports the other advertised behavior. The restored cargo run passed 28 nonconforming-response tests, 15 roundtrip tests, 15 new smoke-verb tests and 12 upload-peer tests (`reviewer-restored-tests.log:37`, `:58`, `:79`, `:97`). Scratch-only mutations that lose the 200 on a timeout, accept a cut listing, stop after page one, erase S3 error codes, or refuse the conditional GET's 304 framing each compiled and failed the appropriate assertion (`reviewer-required-reds.log:26`, `:62`, `:88`, `:133`, `:159`). The original smoke suite then passed again (`reviewer-post-mutation-green.log:19`). These are behavioral mutation reds, not a fabricated pre-fix red. The declared external dependency `typos` was present and executed successfully; formatting, docs lint and validate clippy with warnings denied also passed (`reviewer-scanners.log:1`, `:4`, `:7`, `:232`).

The frozen evidence supports the remaining gate dispositions. The instance-root wrappers are not supplied here, so their complete logs were inspected rather than treated as missing target scripts. C4-ci records workspace checks, dependency audits and DST finishing successfully (`gate-logs/C4-ci.log:4510`); C4-verify records the same green/compile-only-red limitation independently reproduced above (`gate-logs/C4-verify.log:14`, `:355`); diff coverage records 507/567 instrumentable changed lines executed, 89.4%, with 640 changed lines unscored (`gate-logs/C4-diff-cov.log:152`). The full 103-mutant campaign was not repeated: its log records 64 caught, 34 unviable and five missed (`gate-logs/C5-mutants.log:18`), and the consequential survivor was rerun directly. The two `SuccessBody::Empty` survivors are not independently observable through this slice's successful HEAD path, whose unexpected wire bytes are rejected before that predicate; the two interceptor-name changes affect diagnostics. They are not additional defects.

The batch-review gate records zero findings (`gate-logs/T4-batch-review.log:10`), which does not override the concrete reproduction above. The TiKV log records both requested feature-check commands succeeding (`gate-logs/host-tikv.log:2`, `:211`); this is compilation evidence, not a live TiKV test. The contribution row remains deferred to publish. No unavailable tool, shim, or unexercised external dependency was used as proof of this client change.

Prior-art verification was performed by affected source path through GitHub: merged history for `crates/validate/src/s3.rs` and `crates/validate/src/s3/` confirms the client, deadline, upload and response-reader work associated with #859, #865 and #934. Changed-file lists for every one of the 19 closed-unmerged PRs contain no `crates/validate` changes; overlapping lockfile changes are dependency updates (`reviewer-prior-art.json`). The brief's recorded #938 split and #956 deferrals were respected; no builder rationale or other checkout was read. The standing rubric's clock, dependency, XML/framing, cancellation and global-state surfaces were checked; the new deadline reads explicitly use the existing Tokio clock (`crates/validate/src/s3/answer.rs:513`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 955 (validate-s3-client-reads-and-listings)

I built the patched tree in a scratch copy and ran probe tests and temporary edits against the
production `S3Client`. I found one concrete failing case. The rest of the evidence held up.

## Findings

- NEEDS-HUMAN [impl] — **A non-2xx answer to the new GET verbs that carries an `x-amz-checksum-*` header loses its code, or turns into a body failure.** `crates/validate/src/s3/verbs.rs:50-62` (`get_object_range`) and `:72-84` (`get_object_if_none_match`) go through the SDK's `GetObject` operation. Its `ResponseChecksumInterceptor` comes from the operation plugin, which is applied before the `.customize()` override plugin (`aws-sdk-s3` 1.155.0 `src/operation/get_object.rs:72-82`). So its `modify_before_deserialization` wraps the body in a checksum validator (`src/http_response_checksum.rs:130-139`) *before* `Capture::modify_before_deserialization` takes it (`crates/validate/src/s3/answer.rs:473-482`). `read_whole` then reads the SDK's validator, not the body as sent. That contradicts the module doc's claim (`answer.rs:17-18`) that the SDK's handling is "never consulted". Under the default `WhenSupported`, the SDK also adds `x-amz-checksum-mode: ENABLED` to the request (`get_object.rs:182-185`), which the caller never asked for. **Reproduced** with a probe test against the patched client and a scripted loopback endpoint:
  - `304 Not Modified` + `ETag` + `x-amz-checksum-crc32: NhCmhg==`, no body, to `get_object_if_none_match` → `Fetched::Other`, outcome `BodyFailed(Transport { received: 0, detail: "body checksum mismatch. expected body checksum to be 3610a686 but it was 00000000" })`. Expected `Error { code: NoBody }`.
  - `404` + `x-amz-checksum-crc32` + body `<Error><Code>NoSuchKey</Code><Message>gone</Message></Error>` to `get_object_range` → `BodyFailed(Transport { received: 60, detail: "body checksum mismatch…" })`. Expected `Error { code: Code("NoSuchKey"), message: Some("gone") }`. That is criterion (H)'s property ("keeps its status, headers and code") failing for a header the tests never send (`crates/validate/tests/s3_client_smoke_verbs.rs:1517-1599`).
  - The request head the probe endpoint captured contains `x-amz-checksum-mode: ENABLED`, and that header is in the SigV4 `SignedHeaders` list.

  The Wyrd gateway sends no response checksum headers, so (A) and (H) against it can't see this. The client targets any `--endpoint`, though. I don't know whether AWS S3 itself repeats checksum headers on a 304. A likely fix: give these two verbs a per-operation `response_checksum_validation(ResponseChecksumValidation::WhenRequired)` through `.customize().config_override(…)`. That stops both the wrapper and the extra request header. Then add a scripted 304 and 403 case that carry a checksum header. The base `get_object` (`crates/validate/src/s3.rs:300-325`) behaves the same way, but changing it is out of this brief's scope, so it needs its own issue.

- NEEDS-HUMAN [impl] — **No test sends a 2xx listing page with an empty body, so the "an empty body names no error" guard is unguarded.** `crates/validate/src/s3/answer.rs:349` (`Ok((code, message)) if !body.is_empty()`) is the only thing that stops a bodiless `200` listing page from being reported as `Outcome::Error { code: NoBody }`, an S3 error the server never sent (this goes against the doc at `answer.rs:148`). The C5 log shows the mutant `replace match guard !body.is_empty() with true in settle` **MISSED**. My probe shows the current code is correct (`200` + `Content-Length: 0` to `list_objects_v2` → `Unreadable { body: b"", detail: "the listing body is not a well-formed XML document: …" }`), so only the test is missing. Fix: add an `"empty"` case (`ok_with_length(id, 0, b"")`, `Refused::Unreadable`) to `listing_pages_that_are_not_whole_documents_are_refused_never_partial` (`s3_client_smoke_verbs.rs:1027-1062`).

- **C5 "fail" (5 missed) is mostly equivalent mutants. Not a defect, recorded so nobody chases them.** The two `SuccessBody::read` survivors (`answer.rs:326`, `:327`) can't be reached through the client. hyper never gives a HEAD response a body, and the connector reports a HEAD that sends one as an overrun. My probe: `200` + `Content-Length: 5` + `hello` to `head_object` → `BodyFailed(Overrun { received: 0, after: 5 })`. `fetched()` sends only non-2xx answers through `SuccessBody::Empty` (`verbs.rs:231-232`), where `read` is never called. So the non-empty branch at `answer.rs:328-331` is dead code. The two `Capture::name` survivors (`answer.rs:449`) are trivially equivalent. Of the five, only the `settle` guard above is a real gap.

- NEEDS-HUMAN [human] — **The T4 batch-review row's evidence is too thin to check.** `check-gates.json` row `T4-batch-review` claims a "3x codex, union, triaged" review of this ~4,400-line diff that finished in 29.97 s with "0 blocking, 0 recorded-rejected, 0 noise-dropped" (`gate-logs/T4-batch-review.log`). Three independent review passes over a diff this size, with not even one noise-dropped item, in half a minute, suggests a cached or empty review. `review-batch.md` isn't in my inputs, so I can't tell. Someone should confirm the review ran on this bundle before it counts toward "one deep, multi-pass review".

## What I tried to refute and could not

- **The red→green evidence.** C4-verify is UNVERIFIABLE, as the brief declared up front (the tests call API the patch adds). I checked two of the claimed reds myself with temporary edits to the production `verbs.rs` in a scratch copy:
  - Red 1: map a 2xx body failure to `NoAnswer(NoResponse)` in `page()`. The (C) test failed on exactly Stall, Trickle and Cut ("the 200 head was lost").
  - Red 4: return after the first listing page. `both_listings_…`, `a_listing_whose_later_page_…` and `…max_listing_pages…` failed.

  The tests drive the production client through the real gateway and real loopback sockets. They don't use a re-implementation, and they fail for the stated reason. The 64 caught mutants agree.
- **(E).** `s3_client_roundtrip` (15), `s3_client_nonconforming_responses` (28) and `s3_client_upload_peers` (12) pass in `gate-logs/C4-ci.log:3261-3353`, and the patch doesn't touch them.
- **The fixes I attacked and couldn't break:**
  - Listing continuation: a repeated or never-ending token stops at `MAX_LISTING_PAGES`; an empty `<NextMarker>` falls back to the last key; a truncated V1 page with no keys and no marker is refused.
  - A body with no declared length: XML ends itself, so a cut page still fails to parse.
  - The budget boundary (exactly 64 KiB is read; one byte more is refused).
  - Deadlines: a deadline tie goes to the whole-body deadline; a huge limit falls back to the idle deadline only.
  - A request refused before it is sent vs. one sent and unanswered (`RequestNotBuilt` vs `NoResponse`).
  - Non-UTF-8 and repeated header lines.
  - Clock: `read_whole` names tokio's runtime clock as the one source for its lifecycle, as the rubric requires (`answer.rs:513-515`).
- **Not raised:** the architecture doc. It is assigned to #956 in the brief, so under the rubric's deferral rule that point is settled.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1027` omits an empty HTTP 200 listing body. The frozen `gate-logs/C5-mutants.log` confirms that replacing `!body.is_empty()` with `true` at `crates/validate/src/s3/answer.rs:349` survives: an empty successful response is then mislabeled `Outcome::Error { code: ErrorCode::NoBody, ... }` despite containing no error document. The current implementation is correct, but this distinction is unprotected. Add a scripted `200` with `Content-Length: 0` for both listing versions and assert `ListingError::Page` preserves status 200 and reports `Outcome::Unreadable` with the empty body.

No production correctness defects or actionable reuse, simplification, or efficiency findings identified. Review used target source and frozen gate evidence; gates were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Reject an empty fallback continuation and pin empty-200 classification—otherwise pagination can restart and still report a complete listing, while a meaningful taxonomy mutation escapes every shipped test; `crates/validate/src/s3/document.rs:133`, `crates/validate/src/s3/answer.rs:349`, `reviewer-probes.log:19`, `reviewer-probes.log:57`.
- [ ] Validation — fitness-to-purpose — Decide whether the read API and failure taxonomy satisfy the forthcoming #939/#956 consumers after the implementation findings are addressed—component tests cannot establish fitness of those future integrations; `brief.md:163`, `crates/validate/src/s3/answer.rs:138`.
- [ ] **A non-2xx answer to the new GET verbs that carries an `x-amz-checksum-*` header loses its code, or turns into a body failure.** `crates/validate/src/s3/verbs.rs:50-62` (`get_object_range`) and `:72-84` (`get_object_if_none_match`) go through the SDK's `GetObject` operation. Its `ResponseChecksumInterceptor` comes from the operation plugin, which is applied before the `.customize()` override plugin (`aws-sdk-s3` 1.155.0 `src/operation/get_object.rs:72-82`). So its `modify_before_deserialization` wraps the body in a checksum validator (`src/http_response_checksum.rs:130-139`) *before* `Capture::modify_before_deserialization` takes it (`crates/validate/src/s3/answer.rs:473-482`). `read_whole` then reads the SDK's validator, not the body as sent. That contradicts the module doc's claim (`answer.rs:17-18`) that the SDK's handling is "never consulted". Under the default `WhenSupported`, the SDK also adds `x-amz-checksum-mode: ENABLED` to the request (`get_object.rs:182-185`), which the caller never asked for. **Reproduced** with a probe test against the patched client and a scripted loopback endpoint:
- [ ] **No test sends a 2xx listing page with an empty body, so the "an empty body names no error" guard is unguarded.** `crates/validate/src/s3/answer.rs:349` (`Ok((code, message)) if !body.is_empty()`) is the only thing that stops a bodiless `200` listing page from being reported as `Outcome::Error { code: NoBody }`, an S3 error the server never sent (this goes against the doc at `answer.rs:148`). The C5 log shows the mutant `replace match guard !body.is_empty() with true in settle` **MISSED**. My probe shows the current code is correct (`200` + `Content-Length: 0` to `list_objects_v2` → `Unreadable { body: b"", detail: "the listing body is not a well-formed XML document: …" }`), so only the test is missing. Fix: add an `"empty"` case (`ok_with_length(id, 0, b"")`, `Refused::Unreadable`) to `listing_pages_that_are_not_whole_documents_are_refused_never_partial` (`s3_client_smoke_verbs.rs:1027-1062`).
- [ ] **The T4 batch-review row's evidence is too thin to check.** `check-gates.json` row `T4-batch-review` claims a "3x codex, union, triaged" review of this ~4,400-line diff that finished in 29.97 s with "0 blocking, 0 recorded-rejected, 0 noise-dropped" (`gate-logs/T4-batch-review.log`). Three independent review passes over a diff this size, with not even one noise-dropped item, in half a minute, suggests a cached or empty review. `review-batch.md` isn't in my inputs, so I can't tell. Someone should confirm the review ran on this bundle before it counts toward "one deep, multi-pass review".
- [ ] `crates/validate/tests/s3_client_smoke_verbs.rs:1027` omits an empty HTTP 200 listing body. The frozen `gate-logs/C5-mutants.log` confirms that replacing `!body.is_empty()` with `true` at `crates/validate/src/s3/answer.rs:349` survives: an empty successful response is then mislabeled `Outcome::Error { code: ErrorCode::NoBody, ... }` despite containing no error document. The current implementation is correct, but this distinction is unprotected. Add a scripted `200` with `Content-Length: 0` for both listing versions and assert `ListingError::Page` preserves status 200 and reports `Outcome::Unreadable` with the empty body.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The acceptance criteria can pass while the new capture core discards S3 error codes.** `brief.md:19-20` requires the server's S3 error code and body, but the specified cases (`brief.md:27-53`, `:93-111`) exercise successful reads/listings, malformed 200 listings, and bodiless 404/304 responses; none requires an XML error response through a new verb. Keeping the existing nonconforming-response suite green does not establish this: on `$PDCA_TARGET`, `crates/validate/tests/s3_client_nonconforming_responses.rs:634-641` limits its operations to Get/Put/Delete, and `:661-680` calls the existing methods. A mutation making every new-verb error report `ErrorCode::NoBody` could satisfy the specified assertions. Add a scripted new-verb case asserting the actual status, headers, XML code and captured body, plus the missing-code and malformed-document distinctions; keep those assertions in this child's acceptance criteria.
- [ ] **The mandatory salvage is an unresolved implementation dependency.** `brief.md:9-11` orders Do to take `results/issue_938/iteration-v3/patch.diff` hunks unchanged and forbids re-deriving the design; `:72-82` and `:105-111` depend on that artifact for the split, API wiring and adapted tests. Neither that patch nor its cited `build-notes.md` exists in the supplied cwd or `$PDCA_TARGET`, and the brief supplies no resolvable artifact location/revision. Revise the brief to point to an artifact actually made available to Do, or remove the mandatory-salvage instruction and include enough API/split detail to implement independently. Also distinguish inherited evidence from the two new tests: `brief.md:58-59` says “Every leg was shown red and green,” while `:105-111` explicitly says the listing-fault and conditional-GET 304 tests are absent from the salvage.
- [ ] size backstop — this slice is behaving oversized: patch is 135 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human override of the size backstop (138 KB vs 100 KB): do NOT split; rebuild this slice and fix every bug and test gap below. 1. v1 listing: a truncated page with an empty last key and no/empty <NextMarker> must be refused (keep page 1's 200, send no continuation request). Filter the fallback last key for emptiness (crates/validate/src/s3/document.rs:130-133). Add a regression test. 2. Checksum wrapper: get_object_range and get_object_if_none_match (verbs.rs:50-62, :72-84) must read the body as sent. Set per-operation response_checksum_validation(WhenRequired) via .customize().config_override(...) so the SDK neither wraps the body nor adds x-amz-checksum-mode. Add scripted 304 and 404/403 cases carrying an x-amz-checksum-crc32 header that keep their code/NoBody. (Base get_object has the same flaw; out of scope here.) 3. Empty-200 listing: add a 200 + Content-Length: 0 case for both list_objects_v2 and v1, asserting ListingError::Page keeps status 200 and reports Outcome::Unreadable with an empty body (kills the surviving mutant at answer.rs:349). 4. Remaining C5 survivors: kill them or remove the code. SuccessBody::read non-empty branch (answer.rs:326-331) is unreachable; remove it or pin it. Capture::name (answer.rs:449): pin the names in a test or drop the mutable surface, so C5 reports zero missed.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
