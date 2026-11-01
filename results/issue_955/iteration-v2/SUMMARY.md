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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.1% — 539 of 585 instrumentable changed lines executed (floor 80%); 585 of 1279 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 107 mutants tested in 31m: 73 caught, 34 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_955/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 32.45s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #955: add faithful S3 HEAD, ranged/conditional GET and paginated listing capture; one builder-fixable boundary-test gap remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success and failure outcomes are falsifiable, and the child scope explicitly separates capture from judging answers and running scenarios; `brief.md:36`, `brief.md:123`. |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no pre-existing verbs to reproduce; independently reverting production confirms missing APIs before any assertion executes, as predeclared in `brief.md:156`; `reviewer-verification.log:7`, `reviewer-verification.log:402`. |
| C3 Change | PASS | The ten changed paths stay within the authorized client/test slice; existing-verb framing is extracted without changing its branches, and the three existing integration suites pass unchanged; `crates/validate/src/s3.rs:318`, `crates/validate/src/s3.rs:417`, `reviewer-verification.log:475`, `reviewer-verification.log:496`, `reviewer-verification.log:541`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the predeclared new-API verification exception — the independent pre-fix run fails compilation, so no behavioral red was established; all 82 selected patched tests pass, with green frozen CI and mutation evidence; `reviewer-verification.log:402`, `reviewer-verification.log:427`, `gate-logs/C4-verify.log:412`, `gate-logs/C5-mutants.log:13`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Require exact listing contents at the 64 KiB boundary — the success assertion accepts any two entries, allowing duplicate keys or corrupt sizes/ETags to pass despite the rubric's property-check requirement; `crates/validate/tests/s3_client_smoke_verbs.rs:1118`, `AGENTS.md:190`. |
| T1 Structure | PASS | The production blackbox dependency boundary remains intact: the new Wyrd trait dependency is test-only, and the independent dependency guard passes; `crates/validate/Cargo.toml:55`, `crates/validate/Cargo.toml:65`, `reviewer-scanners.log:4`. |
| T2 Shape | PASS | Shared framing and strict XML reading keep the new verbs consistent with the existing client; formatting and targeted Clippy pass without introducing production dependencies; `crates/validate/src/s3/response.rs:462`, `crates/validate/src/s3/document.rs:94`, `reviewer-clippy.log:220`. |
| T3 Runtime | PASS | Scoped runtime obligations are exercised: body failures preserve the 200 head, continuations are bounded, and deadlines use one Tokio clock; independent smoke tests pass; `crates/validate/src/s3/answer.rs:506`, `crates/validate/src/s3/verbs.rs:120`, `crates/validate/tests/s3_client_smoke_verbs.rs:818`, `reviewer-verification.log:523`. |
| T4 Contribution | NEEDS-HUMAN | Confirm the recorded path-based prior-art check is sufficient for sign-off — the brief covers merged and closed/rejected work, but this artifact-only target contains one synthetic base commit and no remotes, preventing independent corroboration; `brief.md:187`, `reviewer-scanners.log:13`. |
| T5 Judgment | PASS | The prior iteration's empty-marker and checksum defects have focused regressions, and the checksum interference is removed at its source; no capability/load-time symptom guard was added, while tracked #956 deferrals remain settled; `crates/validate/src/s3/document.rs:134`, `crates/validate/src/s3/verbs.rs:34`, `crates/validate/tests/s3_client_smoke_verbs.rs:1239`, `crates/validate/tests/s3_client_smoke_verbs.rs:1725`, `AGENTS.md:215`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve these capture semantics and the 64 KiB/100-page limits for the downstream smoke workflow — gateway and fault behavior is exercised, but suitability for the later runner remains a human decision; `brief.md:163`, `crates/validate/src/s3/document.rs:21`, `crates/validate/src/s3/response.rs:54`. |

The retained finding is a test defect, not a demonstrated production failure. At `crates/validate/tests/s3_client_smoke_verbs.rs:1118`, replacing the expected results with two copies of one entry would still satisfy the assertion. The fixture names `list/a` and `list/b`, each of size 7 with a quoted ETag (`crates/validate/tests/s3_client_smoke_verbs.rs:884`, `crates/validate/tests/s3_client_smoke_verbs.rs:947`). Compare the complete expected entries for both listing versions. This is the grounded TEST-GAP from `gate-logs/T4-batch-review.log:10`; the mutation summary does not establish that this particular oracle is adequate.

The two documentation findings in that same batch log are declined as a settled scope deferral. The brief explicitly assigns the architecture document to #956 (`brief.md:126`), and the standing reviewer protocol forbids re-raising tracked deferrals (`AGENTS.md:215`). The SDK endpoint/session issue is likewise explicitly deferred to #956 in `crates/validate/src/s3/verbs.rs:204`. Neither is a new patch defect here. The frozen batch gate remains recorded as failed; this advisory review does not change its result.

Independent verification succeeded on the patched disposable target. I stashed production and its new files while retaining the smoke test, observed 36 compile errors with no test execution, restored the stash, and verified all ten patch paths byte-for-byte (`reviewer-verification.log:426`). The subsequent run passed 6 library tests, 21 smoke tests, 28 nonconforming-response tests, 15 roundtrip tests and 12 upload-peer tests. I also reran `typos` (real typos-cli 1.48.0), `cargo fmt --all -- --check`, the documentation linter, `cargo clippy --offline -p wyrd-validate --all-targets`, the blackbox dependency guard, the statics scanner and `git diff --check`; all passed. The external dependency named in the brief, `typos`, was exercised, and the gateway/fault tests used the actual compiled client rather than a tool shim or code-read substitute.

The remaining gate judgments use the frozen evidence, not a claim of independently rerunning the instance-root wrappers:

- C4-ci: PASS — its log includes typos, build/test/lint, dependency and DST checks and ends with all checks passed (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4518`).
- C4-verify: unverifiable behavioral red — patched tests pass, but reverted production fails compilation before assertions; independently reproduced above (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:412`).
- C4-diff-cov: PASS — 539/585 instrumentable changed lines covered (92.1%); 694 unscored lines are not evidence of execution (`gate-logs/C4-diff-cov.log:53`, `gate-logs/C4-diff-cov.log:113`).
- C5-mutants: PASS for the recorded run — 107 tested, 73 caught, 34 unviable, no survivors; the log is an aggregate and does not identify the brief's individual hand-written red mutations (`gate-logs/C5-mutants.log:13`).
- T4-batch-review: FAIL as recorded — retain the grounded test gap, decline the two duplicate documentation findings for the recorded #956 deferral (`gate-logs/T4-batch-review.log:10`).
- T4-contribution: N/A — contribution artifacts are absent by design at Check; the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`).
- host-tikv: PASS for compilation — both requested feature Clippy commands finished successfully; this establishes no live TiKV-service behavior (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

Source citations are relative to `$PDCA_TARGET`; brief and evidence-log citations are relative to this review sandbox. The target is readable and the patch builds; no stale-target or host-toolchain defect was inferred. No `INTEGRATION.md` was present in the supplied target. No builder rationale or other checkout was consulted.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 955 (S3 client reads and listings)

The core design held: I could not make the client lose a 2xx head, accept a cut page, or report the wrong error code. I did find one wrong report on a real input and two groups of refusal rules that no test protects. I tested in a scratch copy of the target, with toolchain 1.96.0 available, and have removed that copy.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/answer.rs:493-502` with `crates/validate/src/s3/response.rs:106`: a bodiless answer that carries a multi-coding `Transfer-Encoding` is reported as a failed body. **Failing case 1:** `304 Not Modified` with `Transfer-Encoding: gzip, chunked`, in answer to `get_object_if_none_match`, gives `Outcome::BodyFailed(Framing { transfer_encoding: Some("gzip, chunked"), .. })`. A plain `chunked` 304 gives `Error { NoBody }`. **Failing case 2:** `head_object` answered `200` with the same header gives the same `BodyFailed`. I ran both. `response.rs:115-117` lets this 304 past its own refusal check, but `:106` has already marked it `Untrusted`, and `read_whole` returns on that before it reads anything. Brief (G) says a GET's or HEAD's 304 "may carry framing headers with no body", and RFC 9112 §6.1 allows the header on a 304 to a GET. So a conforming answer is downgraded, against the brief's invariant. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1456` covers only exactly `chunked`. Fix it in the new read path, not in `Framing::of`, because the older verbs share `Framing::of` and must not change. The HEAD `200` case is the weaker of the two: if the builder treats it as intended, it should say so in a comment and pin it in a test.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:102-111` and `:157-161`: the rules that keep a listing from reading as complete when it isn't have no test. Production is correct today: I sent a page with no `<IsTruncated>`, one with `TRUE`, one with the field twice, and one with a comment inside it, and all four were refused. But I applied these mutations together and every test in `cargo test -p wyrd-validate` still passed:
  - a missing `<IsTruncated>` reads as `false`
  - any other value reads as `false`
  - a repeated field is accepted, last one wins
  - markup in a field is read as its first text

  With those mutations, a page with no `<IsTruncated>`, or with `<IsTruncated>TRUE</IsTruncated>`, ends the listing as a success with its entries so far, which is the partial listing brief (D) forbids. This is the rubric's "absent entries" class. Add scripted cases for all four through both listing verbs.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:271` with `crates/validate/src/s3.rs:424-431`: no test checks the framing refusal on a ranged or conditional GET's 2xx. Production is correct today: a `206` with no framing gives `body: Err(LengthUndeclared)`, and one with `Transfer-Encoding: gzip` gives `body: Err(Framing)`. But changing `:271` to `object_length(framing).unwrap_or(None)` left the whole crate's tests passing, as part of the same combined mutation run. That change would accept a close-delimited `206`, so a cut connection could read as the end of the object. The older `get_object` tests can't catch this because they don't go through `fetched`, and the framing test skips this case on purpose (`s3_client_smoke_verbs.rs:1627`). Add `Fetched::Object` cases for no framing and for an untrusted coding, plus a chunked 2xx and a body cut after a `206`; `s3.rs:421` shows chunked also never runs through the new test file.

- NEEDS-HUMAN [human] — `check-gates.json`, row `T4-batch-review`: the overall result is `fail`, and two of its three blockers are docs-currency findings at `crates/validate/src/s3/verbs.rs:42`. The brief assigns the architecture doc to #956, but the patch carries no deferral marker for it, and the rubric calls docs currency "a merge requirement, not a follow-up". `docs/design/architecture/05-building-block-view.md:299` describes `validate`'s command line, not its client methods, so these methods may not count as an "API operation" at all. A human should either reject the finding with that reason or record the deferral to #956. Otherwise the gate stays red.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1118`: I agree with the third T4 blocker. The exact-budget case asserts `entries.len() == 2`, which is the count-only check the rubric rejects. Assert the keys and sizes instead, using the existing `keys_and_sizes` approach.

## Gate claims that overstate the evidence

These need no separate action; the findings above cover the fixes.

- **`C5-mutants` "107 tested: 73 caught, 34 unviable":** this is true but gives less comfort than it looks. My hand-applied mutations of the refusal arms all survived, so zero missed does not mean those refusals are tested.
- **`C4-diff-cov` 92.1%:** this clears the floor, but its MISS list includes the untested refusal lines above: `document.rs:105-110`, `document.rs:158-161` and `s3.rs:424-431`.

## Attempted to refute, could not

- **The required reds are real.** `C4-verify` is UNVERIFIABLE, as the brief declared in advance, because the test can't compile against the base. So I applied each failure to the fixed code by hand and ran the test file. These are mutations of the fixed code, not a pre-fix run.

  | Change | Tests that went red |
  |---|---|
  | Red 1: a timeout replaces the 2xx | the (C) listing test |
  | Red 2: a cut page is accepted | 3 tests, including the (D) test |
  | Red 4: listing stops after page one, V2 and V1 | 3 tests each, including the (A) test |
  | Red 15: every error reads as `NoBody` | the (H) test and the checksum test |
  | (G)'s 304 framing read as a non-GET | the chunked 304 test |
  | Checksum override removed, range and conditional GET separately | both checksum tests each time |

- **Production path.** The tests call the public `S3Client` verbs against a real gateway or a scripted socket, and they check the requests that were sent. I found no parallel copy of the production logic.
- **Empty ETag.** `get_object_if_none_match(key, "")` still sends an `if-none-match:` header, so it does not silently become an unconditional GET.
- **SDK length check.** By reading `content_length_enforcement.rs`: it only wraps GETs, ignores a `Content-Length` it can't parse, and only runs its comparison after the inner body ends. So a `5, 5` length should not cause a false refusal, and bytes after a 304's head stay an overrun, which the existing test confirms.
- **Requests the caller did not ask for.** The listings add no `encoding-type`, and HEAD has no checksum validator, by reading SDK 1.155.0. The checksum override does stop the validator on both GET verbs, as the mutation runs above show.
- **Scope.** The patch does not touch `lib.rs` or the three existing test files, and `wyrd-traits` is only a dev-dependency.
- **S3 Express before the connector.** `verbs.rs:204` carries `// deferred: #956`, so I treated it as settled and did not attack it.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1118`: The exact-budget success case checks only `entries.len() == 2`. Two copies of `list/a`, or entries with incorrect sizes or ETags, would pass this assertion. Compare the returned entries with the fixture's expected `list/a` and `list/b`, each size 7 and the supplied ETag, to satisfy the rubric's requirement for property assertions rather than count-only checks.

No additional production correctness bugs or actionable reuse, simplification, or efficiency findings identified. Validation relied on the frozen gate evidence; tests were not rerun against the read-only target.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the predeclared new-API verification exception — the independent pre-fix run fails compilation, so no behavioral red was established; all 82 selected patched tests pass, with green frozen CI and mutation evidence; `reviewer-verification.log:402`, `reviewer-verification.log:427`, `gate-logs/C4-verify.log:412`, `gate-logs/C5-mutants.log:13`.
- [ ] C5 Causal adequacy — Require exact listing contents at the 64 KiB boundary — the success assertion accepts any two entries, allowing duplicate keys or corrupt sizes/ETags to pass despite the rubric's property-check requirement; `crates/validate/tests/s3_client_smoke_verbs.rs:1118`, `AGENTS.md:190`.
- [ ] T4 Contribution — Confirm the recorded path-based prior-art check is sufficient for sign-off — the brief covers merged and closed/rejected work, but this artifact-only target contains one synthetic base commit and no remotes, preventing independent corroboration; `brief.md:187`, `reviewer-scanners.log:13`.
- [ ] Validation — fitness-to-purpose — Approve these capture semantics and the 64 KiB/100-page limits for the downstream smoke workflow — gateway and fault behavior is exercised, but suitability for the later runner remains a human decision; `brief.md:163`, `crates/validate/src/s3/document.rs:21`, `crates/validate/src/s3/response.rs:54`.
- [ ] `crates/validate/src/s3/answer.rs:493-502` with `crates/validate/src/s3/response.rs:106`: a bodiless answer that carries a multi-coding `Transfer-Encoding` is reported as a failed body. **Failing case 1:** `304 Not Modified` with `Transfer-Encoding: gzip, chunked`, in answer to `get_object_if_none_match`, gives `Outcome::BodyFailed(Framing { transfer_encoding: Some("gzip, chunked"), .. })`. A plain `chunked` 304 gives `Error { NoBody }`. **Failing case 2:** `head_object` answered `200` with the same header gives the same `BodyFailed`. I ran both. `response.rs:115-117` lets this 304 past its own refusal check, but `:106` has already marked it `Untrusted`, and `read_whole` returns on that before it reads anything. Brief (G) says a GET's or HEAD's 304 "may carry framing headers with no body", and RFC 9112 §6.1 allows the header on a 304 to a GET. So a conforming answer is downgraded, against the brief's invariant. The test at `crates/validate/tests/s3_client_smoke_verbs.rs:1456` covers only exactly `chunked`. Fix it in the new read path, not in `Framing::of`, because the older verbs share `Framing::of` and must not change. The HEAD `200` case is the weaker of the two: if the builder treats it as intended, it should say so in a comment and pin it in a test.
- [ ] `crates/validate/src/s3/document.rs:102-111` and `:157-161`: the rules that keep a listing from reading as complete when it isn't have no test. Production is correct today: I sent a page with no `<IsTruncated>`, one with `TRUE`, one with the field twice, and one with a comment inside it, and all four were refused. But I applied these mutations together and every test in `cargo test -p wyrd-validate` still passed:
- [ ] `crates/validate/src/s3/verbs.rs:271` with `crates/validate/src/s3.rs:424-431`: no test checks the framing refusal on a ranged or conditional GET's 2xx. Production is correct today: a `206` with no framing gives `body: Err(LengthUndeclared)`, and one with `Transfer-Encoding: gzip` gives `body: Err(Framing)`. But changing `:271` to `object_length(framing).unwrap_or(None)` left the whole crate's tests passing, as part of the same combined mutation run. That change would accept a close-delimited `206`, so a cut connection could read as the end of the object. The older `get_object` tests can't catch this because they don't go through `fetched`, and the framing test skips this case on purpose (`s3_client_smoke_verbs.rs:1627`). Add `Fetched::Object` cases for no framing and for an untrusted coding, plus a chunked 2xx and a body cut after a `206`; `s3.rs:421` shows chunked also never runs through the new test file.
- [ ] `check-gates.json`, row `T4-batch-review`: the overall result is `fail`, and two of its three blockers are docs-currency findings at `crates/validate/src/s3/verbs.rs:42`. The brief assigns the architecture doc to #956, but the patch carries no deferral marker for it, and the rubric calls docs currency "a merge requirement, not a follow-up". `docs/design/architecture/05-building-block-view.md:299` describes `validate`'s command line, not its client methods, so these methods may not count as an "API operation" at all. A human should either reject the finding with that reason or record the deferral to #956. Otherwise the gate stays red.
- [ ] `crates/validate/tests/s3_client_smoke_verbs.rs:1118`: I agree with the third T4 blocker. The exact-budget case asserts `entries.len() == 2`, which is the count-only check the rubric rejects. Assert the keys and sizes instead, using the existing `keys_and_sizes` approach.
- [ ] `crates/validate/tests/s3_client_smoke_verbs.rs:1118`: The exact-budget success case checks only `entries.len() == 2`. Two copies of `list/a`, or entries with incorrect sizes or ETags, would pass this assertion. Compare the returned entries with the fixture's expected `list/a` and `list/b`, each size 7 and the supplied ETag, to satisfy the rubric's requirement for property assertions rather than count-only checks.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_955/review-b
- [ ] external dependency.
- [ ] **The acceptance criteria can pass while the new capture core discards S3 error codes.** `brief.md:19-20` requires the server's S3 error code and body, but the specified cases (`brief.md:27-53`, `:93-111`) exercise successful reads/listings, malformed 200 listings, and bodiless 404/304 responses; none requires an XML error response through a new verb. Keeping the existing nonconforming-response suite green does not establish this: on `$PDCA_TARGET`, `crates/validate/tests/s3_client_nonconforming_responses.rs:634-641` limits its operations to Get/Put/Delete, and `:661-680` calls the existing methods. A mutation making every new-verb error report `ErrorCode::NoBody` could satisfy the specified assertions. Add a scripted new-verb case asserting the actual status, headers, XML code and captured body, plus the missing-code and malformed-document distinctions; keep those assertions in this child's acceptance criteria.
- [ ] **The mandatory salvage is an unresolved implementation dependency.** `brief.md:9-11` orders Do to take `results/issue_938/iteration-v3/patch.diff` hunks unchanged and forbids re-deriving the design; `:72-82` and `:105-111` depend on that artifact for the split, API wiring and adapted tests. Neither that patch nor its cited `build-notes.md` exists in the supplied cwd or `$PDCA_TARGET`, and the brief supplies no resolvable artifact location/revision. Revise the brief to point to an artifact actually made available to Do, or remove the mandatory-salvage instruction and include enough API/split detail to implement independently. Also distinguish inherited evidence from the two new tests: `brief.md:58-59` says “Every leg was shown red and green,” while `:105-111` explicitly says the listing-fault and conditional-GET 304 tests are absent from the salvage.
- [ ] size backstop — this slice is behaving oversized: patch is 154 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human override of the size backstop (154 KB vs 100 KB): do NOT split; rebuild this slice and make the fixes below. This slice already came from splitting #938 for size, and this list is mostly tests. 1. Bug: a bodiless answer with a multi-coding Transfer-Encoding is reported as a failed body (crates/validate/src/s3/answer.rs:493-502, crates/validate/src/s3/response.rs:106). A 304 with `Transfer-Encoding: gzip, chunked` answering get_object_if_none_match must not give Outcome::BodyFailed(Framing). Fix it in the new read path, not in Framing::of, because the older verbs share Framing::of and must not change. For head_object answered 200 with the same header, either fix it or say it is intended in a comment and pin it in a test. Extend the test at crates/validate/tests/s3_client_smoke_verbs.rs:1456, which covers only exactly `chunked`. 2. Listing refusal tests (document.rs:102-111, :157-161): add scripted cases through both listing verbs for a page with no <IsTruncated>, one with `TRUE`, one with the field twice, and one with markup inside a field. Production is correct today; these rules currently have no test that fails when they break. 3. Ranged and conditional GET tests (verbs.rs:271, s3.rs:424-431): add Fetched::Object cases for no framing and for an untrusted coding, plus a chunked 2xx and a body cut after a 206. Changing verbs.rs:271 to object_length(framing).unwrap_or(None) must fail. 4. Exact-budget test (crates/validate/tests/s3_client_smoke_verbs.rs:1118): compare the full entries, list/a and list/b, each size 7 with the supplied ETag, for both listing versions, instead of checking entries.len() == 2. This clears the real T4 blocker. 5. T4 docs blockers (crates/validate/src/s3/verbs.rs:42): the brief assigns the architecture doc to #956, so do not edit the doc here. Add a `// deferred: #956` marker at verbs.rs:42 so the deferral is recorded in the patch.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
