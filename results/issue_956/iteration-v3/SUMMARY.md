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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.9% — 280 of 286 instrumentable changed lines executed (floor 80%); 286 of 656 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 64 mutants tested in 17m: 47 caught, 14 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.71s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No patch defect found in #956’s bulk-delete reporting, unsupported-operation probes, and endpoint-preserving S3 routing; fitness-to-purpose remains for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract separates observations from the dependent runner’s judgments and bucket-safety responsibility, with falsifiable wire-level criteria (`brief.md:37`; `crates/validate/src/s3/verbs.rs:45`; `crates/validate/src/s3/document.rs:75`). |
| C2 Reproduction (red pre-fix) | N/A | This is pre-declared new functionality; stashing production changes while retaining the new tests reproduces missing APIs at compile time, not an existing runtime defect (`brief.md:179`; `review-red-base.log:4`). |
| C3 Change | PASS | The observed requests and replies satisfy the scoped contract and prior corrections, including URI limits, delete-body failures, and invalid rows (`crates/validate/tests/s3_client_smoke_verbs.rs:1074`; `crates/validate/tests/s3_client_smoke_verbs.rs:1418`; `crates/validate/tests/s3_client_smoke_verbs.rs:2270`; `crates/validate/tests/s3_client_smoke_verbs.rs:3197`). |
| C4 Verification (red→green) | PASS | Thirteen targeted behavioral mutations fail and all 191 validator tests pass after restoration; frozen full CI passes, with the independent full-CI snapshot caveat detailed below (`review-mutations.log:2`; `review-final-green.log:255`; `gate-logs/C4-ci.log:4658`; `review-ci.log:4042`). |
| C5 Causal adequacy | PASS | Replacing SDK endpoint rules removes the rerouting cause; the URI input guard prevents a reproduced SDK panic, and failures cannot silently discard delete evidence or a received head (`crates/validate/src/s3.rs:493`; `crates/validate/src/s3/verbs.rs:398`; `crates/validate/src/s3/document.rs:238`; `review-mutations.log:12`). |
| T1 Structure | PASS | Existing capture and document seams support the new operations without adding production dependencies or moving verdict policy into the client; the blackbox dependency guard passes (`crates/validate/src/s3/verbs.rs:379`; `crates/validate/src/s3/answer.rs:336`; `crates/validate/src/s3/document.rs:107`; `review-ci.log:20`). |
| T2 Shape | PASS | The test-heavy size was explicitly accepted, the living architecture documents the observable limits, and formatting, spelling, and docs rendering pass (`brief.md:252`; `docs/design/architecture/05-building-block-view.md:303`; `review-ci.log:4`; `review-ci.log:23`). |
| T3 Runtime | PASS | Real gateway and loopback runs exercise bounded connect/body waits, exact request identity, retained status, and the actual delete-budget boundary (`crates/validate/tests/s3_client_smoke_verbs.rs:602`; `crates/validate/tests/s3_client_smoke_verbs.rs:1674`; `crates/validate/tests/s3_client_smoke_verbs.rs:1695`; `crates/validate/src/s3/verbs.rs:513`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit is mandatory at publish, while the frozen batch review reports zero findings (`gate-logs/T4-contribution.log:10`; `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | PASS | No unresolved implementation concern or new scope decision remains; path-based merged history and all 19 closed-unmerged PR file lists show no conflicting rejected work, and the recorded earlier dispositions are respected (`review-prior-art.json:2`; `review-prior-art.json:258`; `brief.md:235`; `brief.md:252`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept whether these mechanically verified observations are sufficient for #939’s intended smoke scenarios — caller-owned bucket safety and request sizing remain part of that usage contract (`crates/validate/src/s3/verbs.rs:45`; `crates/validate/src/s3/verbs.rs:348`; `docs/design/architecture/05-building-block-view.md:303`). |

Source citations above are relative to `$PDCA_TARGET`; brief, gate-log, and reviewer-evidence citations are relative to the review directory. The supplied target is usable: its restored `git diff` matches `patch.diff` byte for byte. No production change is left beyond the supplied patch.

**Independent behavioral evidence supports the patch.** `cargo test --offline -p wyrd-validate --test s3_client_smoke_verbs` passed all 52 tests. After stashing the patch, retaining the test file, and confirming the expected missing-API compile failure, I restored the patch and exercised the six required regressions: default endpoint resolution, dropped error rows, unsent requests reported as no response, lost probe heads after body timeouts, erased connect/operation timeout classifications, and a changed upload ID. Each produced a runtime test failure. Removing the URI limit, skipping processing instructions, accepting invalid bulk-delete 304 framing, and losing the delete head after body timeouts also failed. Two initial mutations were unviable because warnings are denied; their symbol-preserving retries produced actual test failures (`review-mutations.log:1`). Finally, `cargo test --offline -p wyrd-validate` passed all 191 tests, including the three unchanged S3 suites (`review-final-green.log:1`).

**The local full-CI failure belongs to the disposable snapshot.** `cargo xtask ci` completed with two failed checks sharing one cause: `.github/advisory-db` is absent as a gitlink in both the snapshot’s HEAD and index. Its index-contract test and advisory-pin step therefore fail (`xtask/tests/advisory_db.rs:64`; `review-ci.log:3394`; `review-ci.log:4042`). Formatting, Clippy, build, validator tests, conformance, static-state checks, and DST completed without another reported failure. The frozen original-worktree log explicitly shows the gitlink test passing, the database pinned, advisory/license checks succeeding, and full CI passing (`gate-logs/C4-ci.log:3574`; `gate-logs/C4-ci.log:4014`; `gate-logs/C4-ci.log:4025`; `gate-logs/C4-ci.log:4658`). This caveat does not establish a patch defect. Both external dependencies named in the brief were actually exercised locally: `typos` and the renderer, including the 101-page link audit (`review-ci.log:4`; `review-ci.log:9`).

**Frozen evidence is used only for what it demonstrates.** The verification wrapper explicitly reports green-only, not a pre-fix behavioral red (`gate-logs/C4-verify.log:60`). Coverage is 280/286 instrumentable changed lines, with 370 lines unscored (`gate-logs/C4-diff-cov.log:295`). The mutation gate’s failure is three timeouts, alongside 47 caught and 14 unviable mutations, with no reported survivor (`gate-logs/C5-mutants.log:13`). I reproduced those three resolver perturbations against the focused endpoint-path test; all failed at runtime, without timing out (`review-mutations.log:13`). The host TiKV log shows both actual feature compilations finishing (`gate-logs/host-tikv.log:112`; `gate-logs/host-tikv.log:211`). The deferred contribution audit remains owed at publish, not to a human at this Check.

**Prior art and review boundaries were checked.** GitHub merged history was queried by each of the six affected file paths, and every closed-unmerged PR’s file list was compared with those paths: 19 PRs, zero overlap, no truncated file-list page (`review-prior-art.json:2`; `review-prior-art.json:258`). The prior rejected attempts and accepted scope/size decisions supplied in the brief were checked against the current tests. No capability probe or load-time workaround triggers the C5 smell test: the new guard validates an input that otherwise panics, as the prior sign-off requested. No additional `INTEGRATION.md` was supplied or present in the target; no unavailable project-specific human-only checklist is claimed verified.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 956

Result: I found one implementation defect, and it is in the docs, not the code. I could not break the routing, probe, row-reading or length-limit code. All runs used a scratch copy of the target with the full patch applied. The toolchain was available, so this verdict is not provisional.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:350-353` and `docs/design/architecture/05-building-block-view.md:303` say the gateway's delete result is "115 bytes plus 30 bytes of markup a row plus the key", so "1,000 keys of 32 bytes fit". That holds only when every row is `<Deleted>`. The gateway's `<Error>` row (`crates/gateway-s3/src/lib.rs:2702`, text from `classify` at `:3997`) has 113 bytes of markup, code and message for `InternalError`, or 83 more than a `<Deleted>` row. **Failing case:** `delete_objects` of 1,000 keys of 32 bytes, answered with 42 `InternalError` rows and 958 `<Deleted>` rows, in the gateway's row format from a scripted endpoint. The result is about 65,601 bytes and comes back as `200 BodyFailed(OverBudget)`, so all 1,000 rows are lost, including all 42 failures. With 41 error rows it fits and reads as 959 deleted and 41 errors. So the sizing advice fails when some keys fail, which is when the rows matter most. If every key fails, 32-byte keys fit only up to 451 keys (145 bytes a row), not 1,000. A caller such as #939 that sizes requests from this text can lose its failure evidence. The test `a_bulk_delete_whose_result_passes_the_budget…` only tries all-`<Deleted>` results, so the sentence claims more than the tests show, which brief (H) forbids. Fix: state the `<Error>` row's size in both places, say the 1,000-key figures assume no failures, and pin the 41/42 boundary in a test. The 64 KiB budget itself stays, as the iteration 2 sign-off decided.

## Notes that need no decision

- **C4-verify is labelled more strongly than its log supports.** `check-gates.json` shows `pass` on "per-fix red->green", but `gate-logs/C4-verify.log:60-62` says "PASS (green-only)", and the brief predicted UNVERIFIABLE (exit 77). The gate gave no red evidence, so I reran the reds myself (next note).
- **The reds hold.** Each mutation below failed tests in `s3_client_smoke_verbs`, and the unmutated patch passes all 52.
  - Brief's reds: 1 (probe stall returns only the timeout), 3 (SDK default endpoint rules), 5 (drop `<Error>` rows), 6 (unsent reported as `NoResponse`), 16 (deadline becomes `NoResponse`), 17 (wrong `uploadId` on `UploadPart`). For 3 I kept `PathStyle` compiled but unused.
  - Carry-forward: `Capture::new(true)` on bulk delete, `NoAnswer` on a body failure, processing instruction skipped, unknown root child skipped, empty `<Key>` accepted, `too_long` turned off.
  - My additions: `quiet(true)`, wrong part ETag, part number 2, a one-byte part body, copy source without its bucket, any body accepted as an empty success, lower-case hex, endpoint slash kept, `requested` sorted, `URI_ROOM` 40, slashes counted as three bytes.
  - Survivor: only turning off `disable_s3_express_session_auth` survived. `crates/validate/src/s3.rs:189-193` already describes it as a guard against a later SDK, so that is expected.
  - Not run: my "Complete names no part" mutation did not compile, so that one proves nothing. The wrong-ETag mutation does reach the same body check.
- **The C5 `fail` is not a surviving mutant.** `gate-logs/C5-mutants.log:13-16` lists 3 timeouts and no misses. I ran equivalents of two of them, `path_style_url` returning `"xyzzy"` and the scheme guard always false, and both failed many smoke tests in 22 s. The third (`||` to `&&`) has the same effect as the guard being false. I did not find out why the whole-package run timed out at 83 s.
- **Low: "none goes unread" is broader than the code.** `crates/validate/src/s3/document.rs:237` says this of the rows, but `fields` (`:293`) passes over anything inside a row. `<Deleted><Key>a</Key><Error><Key>b</Key><Code>AccessDenied</Code></Error></Deleted>` reads as one `Deleted a` row, and text or a processing instruction inside a row is also skipped. No S3 server sends this, the docs do allow unread elements in a row, and if `b` was requested it still shows under `keys_without_a_row`. I would not rebuild for it.
- **Low: refusals the docs do not list.** The list at `crates/validate/src/s3.rs:52-56` omits an empty or all-white-space key, an empty upload id and a copy source with a newline or NUL. Each came back correctly as `RequestNotBuilt` with nothing sent. `Probe { key: " " }` is refused even though `document.rs` says S3 allows such a key, so a caller may be surprised, but the report is honest.
- **Outside this diff.** A key holding a carriage return, `cr\rx`, comes back from the real gateway as a row for `cr\nx`, so it shows as both "no row" and "not requested". `xml_escape` (`crates/gateway-s3/src/lib.rs:3803-3816`) writes a carriage return raw, and an XML parser reads a raw one as a newline, so that is the likely cause. The client surfaces the mismatch rather than hiding it, and the cause is not in this patch.

## Tried and could not break

- **Probe keys and upload ids.** No panic and the correct classification for `/`, `//`, `/a`, `.`, `..`, `../x`, `?`, `#`, `*`, a NUL byte and non-ASCII keys, and for upload ids such as `a&b=c` and a newline. The `URI_ROOM` arithmetic matches the SDK's encoding set (`aws-smithy-http` 0.64.3 `urlencode.rs:9-42`). A key at the limit behind a 200-byte endpoint path is `RequestNotBuilt` with nothing sent, not a panic.
- **Endpoint forms.** An upper-case scheme, a fragment and an escaped path reach the configured host with the expected path; an empty `?` query and a missing scheme are refused before sending. Two oddities need odd input and I am not filing them: `http://h//` sends `//validate/`, and an endpoint with a user and password got a 403 (the same probes answered 400 without them). I did not compare that 403 with the base.
- **Signing.** With a wrong secret, the gateway answers 403 `SignatureDoesNotMatch` on bucket probes, the name `..` included, so the test's "a signature it refused would be a 403" is true. Auth runs before the 400.
- **Delete result reading.** A DOCTYPE and a processing instruction before the root are refused, CDATA text beside rows is refused, and exact-budget and over-budget documents behave as (D) asks. Empty, partial, repeated and unrequested-key answers are reported without being judged.
- **`not_built`.** The misses at `crates/validate/src/s3/verbs.rs:599-603` in `gate-logs/C4-diff-cov.log` look harmless: by reading, the builders that call it cannot fail, because the key and the object list are always set.
- **Test flakiness.** 18 runs of the test binary, in 3 rounds of 6 at once pinned to one CPU core, all passed 52 of 52, including the trickle and deadline tests.
- **Test fidelity.** The tests call the production `S3Client`, the real gateway checks signatures, and the relay records each request before forwarding it. I found no parallel re-implementation. (E) holds: the diff touches only `s3_client_smoke_verbs.rs` under `tests/`, and #955's `a_listing_page_answered_200…` assertions moved into `misreported_fault` with the same checks.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:638`: The new bulk-delete budget test requires `OverBudget { received: 70_115 }`, but `read_whole` returns the cumulative bytes at the first frame that crosses the budget (`crates/validate/src/s3/answer.rs:617`). TCP reads can split this 70,115-byte response so that the crossing frame ends before the body's end; correct behavior then fails this assertion. Match `received > BUFFERED_BODY_BUDGET` (as the existing `refused_as` helper does), and assert `head.header("content-length") == Some("70115")` separately to pin the documented response size. This is a potential CI flake identified by inspection; the frozen gate run passed.

No additional correctness or reuse/simplification findings in this diff. Review used the read-only target and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept whether these mechanically verified observations are sufficient for #939’s intended smoke scenarios — caller-owned bucket safety and request sizing remain part of that usage contract (`crates/validate/src/s3/verbs.rs:45`; `crates/validate/src/s3/verbs.rs:348`; `docs/design/architecture/05-building-block-view.md:303`).
- [ ] `crates/validate/src/s3/verbs.rs:350-353` and `docs/design/architecture/05-building-block-view.md:303` say the gateway's delete result is "115 bytes plus 30 bytes of markup a row plus the key", so "1,000 keys of 32 bytes fit". That holds only when every row is `<Deleted>`. The gateway's `<Error>` row (`crates/gateway-s3/src/lib.rs:2702`, text from `classify` at `:3997`) has 113 bytes of markup, code and message for `InternalError`, or 83 more than a `<Deleted>` row. **Failing case:** `delete_objects` of 1,000 keys of 32 bytes, answered with 42 `InternalError` rows and 958 `<Deleted>` rows, in the gateway's row format from a scripted endpoint. The result is about 65,601 bytes and comes back as `200 BodyFailed(OverBudget)`, so all 1,000 rows are lost, including all 42 failures. With 41 error rows it fits and reads as 959 deleted and 41 errors. So the sizing advice fails when some keys fail, which is when the rows matter most. If every key fails, 32-byte keys fit only up to 451 keys (145 bytes a row), not 1,000. A caller such as #939 that sizes requests from this text can lose its failure evidence. The test `a_bulk_delete_whose_result_passes_the_budget…` only tries all-`<Deleted>` results, so the sentence claims more than the tests show, which brief (H) forbids. Fix: state the `<Error>` row's size in both places, say the 1,000-key figures assume no failures, and pin the 41/42 boundary in a test. The 64 KiB budget itself stays, as the iteration 2 sign-off decided.
- [ ] `crates/validate/tests/s3_client_smoke_verbs.rs:638`: The new bulk-delete budget test requires `OverBudget { received: 70_115 }`, but `read_whole` returns the cumulative bytes at the first frame that crosses the budget (`crates/validate/src/s3/answer.rs:617`). TCP reads can split this 70,115-byte response so that the crossing frame ends before the body's end; correct behavior then fails this assertion. Match `received > BUFFERED_BODY_BUDGET` (as the existing `refused_as` helper does), and assert `head.header("content-length") == Some("70115")` separately to pin the documented response size. This is a potential CI flake identified by inspection; the frozen gate run passed.
- [x] **The rejection criterion can pass for the wrong multipart operation.** Brief lines 27–30 require six multipart probes to return `501 NotImplemented`, but the target rejects any recognized bucket subresource through the same branch (`crates/gateway-s3/src/lib.rs:1797–1802`) and any recognized object subresource through another shared branch (`:1836–1847`). A `ListParts` probe accidentally sending `ListMultipartUploads` can therefore satisfy that outcome. Criterion G (brief lines 58–60) supplies success responses but does not require checking the request that elicited them. Require captured method/path/query/header/body assertions for each probe, so a wrong operation or missing operation parameter cannot pass merely by hitting a rejection guard.
- [x] **Criterion B contradicts the existing timeout contract.** Brief lines 37–39 say a request that reached the connector and got no answer “stays `NoResponse`.” The target explicitly reports SDK operation timeouts as `Timeout { phase: Operation }` and connector timeouts as `Timeout { phase: Connect }` (`crates/validate/src/s3.rs:346–355`); these also receive no answer. Qualify the criterion to preserve typed deadline failures, reserve `NoResponse` for non-timeout transport failures, and require probe assertions for both deadline phases. Otherwise the stated acceptance rule permits losing the measured timeout phase.
- [x] **The mandatory salvage prerequisite is unresolved in the supplied material.** Brief lines 7–9 require taking the prior patch's hunks unchanged and forbid re-deriving the design; lines 67, 89–95 and 135–137 also depend on its exact architecture paragraph, implementation and SDK evidence. Neither the review cwd nor `$PDCA_TARGET` contains `results/issue_938/iteration-v3/patch.diff` or its `build-notes.md`; `notes.json` and `sources/` are also absent. This does not prove those artifacts are missing elsewhere, but their availability and the claimed prior red/green evidence cannot be established here. Revise the brief to make the required salvage an explicitly supplied, resolvable input, or include the necessary contract and remove the exclusive instruction to transplant unavailable hunks. Include the relevant tracker excerpts so the bucket-safety constraint and prior-review disposition can be checked.
- [x] size backstop — this slice is behaving oversized: patch is 185 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the routing, probe and row-reading code; it held up. Do not split: the human overrode the size warning. Fix these only: 1. The bulk-delete sizing claim at crates/validate/src/s3/verbs.rs:350-353 and docs/design/architecture/05-building-block-view.md:303 holds only when every row is <Deleted>, so it claims more than the tests show, which brief (H) forbids. 1,000 keys of 32 bytes with 42 gateway-format InternalError rows and 958 <Deleted> rows is about 65,601 bytes and comes back as 200 BodyFailed(OverBudget), losing every row; with 41 error rows it fits. In both places state the size of an <Error> row (113 bytes of markup, code and message for InternalError, 83 more than a <Deleted> row) and say the 1,000-key figures assume no failures. Pin the 41/42 boundary in a test. Keep the 64 KiB budget. 2. crates/validate/tests/s3_client_smoke_verbs.rs:638 requires OverBudget { received: 70_115 }, but read_whole returns the bytes held at the first frame that crosses the budget (crates/validate/src/s3/answer.rs:617), so a response split differently over TCP fails the test with correct behavior. Assert received > BUFFERED_BODY_BUDGET, as the refused_as helper does, and assert head.header("content-length") == Some("70115") separately to pin the documented size.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
