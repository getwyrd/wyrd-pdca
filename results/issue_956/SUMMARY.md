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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 97.9% — 280 of 286 instrumentable changed lines executed (floor 80%); 286 of 685 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 64 mutants tested in 13m: 47 caught, 14 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_956/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.55s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #956 bulk-delete reporting, unsupported-operation probes, and S3 endpoint routing: no actionable patch defect found; fitness-to-purpose remains for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance criteria distinguish wire requests, server replies, and body failures; judging replies and choosing safe bucket names remain assigned to #937/#939 (brief.md:45, brief.md:129, brief.md:152). |
| C2 Reproduction (red pre-fix) | N/A | This is declared new functionality; removing production changes makes the retained tests fail on absent APIs, not a runtime assertion, as anticipated (brief.md:179; reviewer-red.log:3). |
| C3 Change | PASS | The changes stay within the agreed client/test/documentation surface, with no new production dependency or lib.rs change; the latest requested corrections are present (brief.md:288; crates/validate/src/s3/response.rs:13; crates/validate/src/s3/document.rs:75). |
| C4 Verification (red→green) | PASS | Independent restoration yields 194 passing tests, including 55 smoke tests; pinned fmt/clippy and documentation checks pass, while the pre-fix result proves API absence only (reviewer-verification.txt:4; reviewer-restored.log:232). |
| C5 Causal adequacy | PASS | Replacing endpoint resolution removes the rerouting cause; request-identity checks and typed body-failure assertions exercise the intended boundary, without a capability-probe workaround (crates/validate/src/s3.rs:189; crates/validate/tests/s3_client_smoke_verbs.rs:1242, crates/validate/tests/s3_client_smoke_verbs.rs:4946). |
| T1 Structure | PASS | Reporting remains separate from higher-level judgment, and shared capture/document parsing handles the added verbs without another transport or parser (crates/validate/src/s3/verbs.rs:385; crates/validate/src/s3/document.rs:243; crates/validate/src/s3/answer.rs:382). |
| T2 Shape | PASS | The explicitly accepted test-heavy size is settled; the implementation retains focused resolver, result, and probe boundaries rather than expanding into the runner (brief.md:291; crates/validate/src/s3.rs:495; crates/validate/src/s3/verbs.rs:404). |
| T3 Runtime | PASS | Exercised deadlines, framing, URI limits, and the 64 KiB boundary preserve status and distinguish unsent requests from transport loss; sampled bulk-delete tests avoid the prior connection churn (crates/validate/tests/s3_client_smoke_verbs.rs:718, crates/validate/tests/s3_client_smoke_verbs.rs:1847, crates/validate/tests/s3_client_smoke_verbs.rs:4894). |
| T4 Contribution | PASS | All seven affected paths were checked against merged and closed-unmerged PR history; no conflicting rejected PR was found, and architecture documentation is current; the publish-artifact subgate is N/A until publish (reviewer-prior-art.txt:9; docs/design/architecture/05-building-block-view.md:303; gate-logs/T4-contribution.log:10). |
| T5 Judgment | PASS | The latest four requested corrections are supported by source and passing tests; the gateway's CR normalization and the approved size/budget decisions are not reopened (brief.md:291; crates/validate/tests/s3_client_smoke_verbs.rs:614, crates/validate/tests/s3_client_smoke_verbs.rs:723, crates/validate/tests/s3_client_smoke_verbs.rs:4894). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the client contract for #939 — callers still own bucket safety and delete-batch sizing, so successful client tests alone do not establish the future runner's operational safety (crates/validate/src/s3/verbs.rs:45, crates/validate/src/s3/verbs.rs:348; brief.md:189). |

Independent evidence supports the current implementation, with a specific limit on the red leg. I stashed the five changed production files while retaining the extended tests: compilation failed with missing `Probe`/delete-result types and methods. After `git stash pop`, `cargo test --offline -p wyrd-validate` passed all 194 tests. This does not establish a behavioral pre-fix red or independently reproduce the brief's named manual mutations. The frozen verification log likewise explicitly reports green-only verification (gate-logs/C4-verify.log:60). Commands and toolchain details are recorded in reviewer-verification.txt:4; raw outputs are reviewer-red.log and reviewer-restored.log.

The required external tools were exercised. Independent `typos`, docs lint, and docs rendering passed; rendering produced 101 pages and passed the link audit (reviewer-docs-lint.log:1; reviewer-docs-render.log:2). Pinned Rust 1.96.0 formatting and clippy with warnings denied passed. The tests used installed Rust 1.96.1. The frozen full CI also shows actual typos/render execution and completed dependency, conformance, and DST checks (gate-logs/C4-ci.log:11, gate-logs/C4-ci.log:4028, gate-logs/C4-ci.log:4661). No undischarged tool or service dependency was identified for this scope.

The remaining frozen gate evidence has bounded meaning:

- Diff coverage reports 280/286 instrumentable changed lines executed (97.9%); 399 additional changed lines were unscored, not demonstrated covered (gate-logs/C4-diff-cov.log:279, gate-logs/C4-diff-cov.log:298).
- The advisory mutation row is red because three resolver mutations timed out; its totals are 47 caught, 14 unviable, and 3 timeouts, with no reported surviving mutant. This is neither a production verification failure nor a clean mutation pass (gate-logs/C5-mutants.log:11).
- The batch-review log reports zero blocking findings, without exposing its individual review passes (gate-logs/T4-batch-review.log:10). The TiKV feature log shows both requested clippy builds completing (gate-logs/host-tikv.log:112, gate-logs/host-tikv.log:211).
- T4-contribution is **N/A**: contribution artifacts are intentionally absent at Check, and their substantive audit is owed at publish (gate-logs/T4-contribution.log:10).

Prior art was rechecked through GitHub file lists for 200 closed PRs spanning the client's introduction and all 19 closed-unmerged PRs. Relevant merged predecessors include #859, #865, #934, #942, and #1034 (#955); none of the closed-unmerged PRs touches the seven affected paths (reviewer-prior-art.txt:3). The parent's rejected local attempts and settled dispositions are identified in brief.md:207 and brief.md:288; no builder notes were read.

All source citations above resolve in the supplied `$PDCA_TARGET`; brief, gate-log, and reviewer-log citations resolve in this report's directory. The disposable target is current, its restored diff matches patch.diff byte for byte, and `git diff --check` passes. No patch edits remain from review.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 956, iteration 6

**Result: I could not refute the fix.** I found no failing input and no test that passes for the wrong reason. There are no `NEEDS-HUMAN` items. The bullets below are one test gap, three notes on how far the gate evidence can be trusted, and what I tried.

I ran everything on a scratch copy of the patched tree, using the pinned toolchain. To let mutated code compile, that copy adds one line to `lib.rs`, `#![allow(dead_code, unused)]`. This does not change behaviour. I did not re-run `cargo xtask ci`; for that gate I rely on `gate-logs/C4-ci.log`.

## Findings (none blocks; none needs a human)

- **Test gap, `~` is not pinned:** `crates/validate/src/s3.rs:552`. Removing `b'~'` from `path_segment`'s unreserved set passes all 55 tests, because no bucket name in the tables holds a `~` (`crates/validate/tests/s3_client_smoke_verbs.rs:168`). It is the only survivor of the 38 mutations I ran. A server should decode `%7E` to the same name, so I see no wrong behaviour. `crates/validate/src/s3/verbs.rs:51` does claim the exact set, so one more table row would close the gap if another round happens. It does not justify a round alone.
- **The C4-verify row claims more than its log shows:** `check-gates.json`, row `C4-verify`, is titled "red pre-fix, green post-fix" and says `pass`, but `gate-logs/C4-verify.log` ends in `PASS (green-only)`, so that gate proved no red. The brief's verification posture also predicted `UNVERIFIABLE (exit 77)`, not this result. The fix is unaffected: I ran the reds myself (below).
- **The C5 `fail` row looks like timeouts, not surviving mutants:** `crates/validate/src/s3.rs:510` and `:515`. The log lists 3 timeouts and no missed mutants. I re-ran one of the three, the match guard at `:515` set to false. It failed 54 tests in 20 s, so it is caught, and the gate's 60 s limit probably cut the run short on a loaded host. I did not re-run the other two.
- **Ports can still run out under heavy parallel runs:** `crates/validate/tests/s3_client_smoke_verbs.rs:1947`. Carry-forward item 4 is done, but one run still leaves 826 sockets in `TIME_WAIT` (measured from a count of 1 before the run). With the 28,232 ports here, that is about 34 runs a minute across the host. With 16 copies at once, 48 runs in rounds 1 to 3 all passed. Rounds 4 and 5 failed, with `AddrInUse` as the only panic cause. That is 80 runs in 12 s, with another issue's `cargo xtask ci` also on the host, far above normal use. Each socket is a real test request, so I would not change this now.
- **T4 pass, not verified:** `gate-logs/T4-batch-review.log` reports 0 blocking in 31.94 s for three review passes over a 203 KB patch. That is fast for a multi-pass review. I cannot tell whether it was a full review, because `review-batch.md` is outside my inputs. This is a doubt about the evidence, not a defect in the code.

## Evidence I tried to break

- **The brief's six reds all go red, in the tests the brief expects:** each row mutates production code at `crates/validate/src/s3.rs:189` or in `crates/validate/src/s3/verbs.rs` and `document.rs`.
  - Red 1, a stalled probe returns only the timeout: `a_probe_answered_200_keeps_its_200…` fails.
  - Red 3, SDK default endpoint resolution: 4 tests fail, including `bucket_probes_send_every_name…`.
  - Red 5, `<Error>` rows dropped: 4 tests fail, including `a_mixed_delete_result…`.
  - Red 6, unsent reported as `NoResponse`: 3 tests fail, including `a_request_stopped_before…`.
  - Red 16, a deadline turned into `NoResponse`: both probe deadline tests fail.
  - Red 17, a different `uploadId`: 3 tests fail, including `every_unsupported_operation…`.
- **The fixes for iterations 2 to 5 are real:** each of these turns a test red:
  - a probe's `OverBudget` turned into `NoResponse`
  - a delete's body failure turned into `NoAnswer`
  - `Capture::new(true)` at `crates/validate/src/s3/verbs.rs:375`
  - a processing instruction skipped at `crates/validate/src/s3/document.rs:257`
  - unknown children skipped at `crates/validate/src/s3/document.rs:275`
  - an empty `<Key>` accepted at `crates/validate/src/s3/document.rs:287`
- **The tests use the production path:** the 55 tests drive the real `S3Client`, through the real gateway, the relay or the scripted endpoint. Mutations of `reads()`, `success_body()`, `URI_ROOM`, the request body, `part_number`, `x-amz-copy-source`, `quiet`, percent-encoding case and the endpoint path join each turned tests red. In total, 37 of 38 mutations were caught.
- **Flakiness:** the 48 runs in rounds 1 to 3 were all green, so the refused-port flake from iteration 4 did not recur (`crates/validate/tests/s3_client_smoke_verbs.rs:1752`).

## Inputs I tried to break the fix with

- **Panics in `Probe::too_long`** (`crates/validate/src/s3/verbs.rs:131`): I got no panic with:
  - `CopyObject` and `CreateMultipartUpload` with a key exactly at the limit
  - `UploadPart`, `CompleteMultipartUpload`, `AbortMultipartUpload` and `ListParts` with an upload id exactly at the limit
  - keys `/lead` and `//`

  The SDK's encoding set matches `path_segment` (`aws-smithy-http` 0.64.3 `src/urlencode.rs:9-42`), so the count cannot come up short for any byte.
- **Bucket names changed on the way out** (`crates/validate/src/s3.rs:549`): the SDK only tests for white space and does not trim the value. ` spaced ` arrives as `/%20spaced%20/`, a leading non-breaking space as `/%C2%A0nbsp/`, and `a\0b` as `/a%00b/`.
- **Rows lost after the server acted** (`crates/validate/src/s3/document.rs:243`):
  - Keys with U+0001, U+000B, NUL, U+FFFE or U+FFFF: the gateway answers `400 MalformedXML` and deletes nothing, sibling keys included. The client reports the 400.
  - Keys with tab, LF, NEL, U+2028, U+007F, U+0080, a literal `&#13;`, `]]>`, or leading and trailing spaces: each comes back exactly as sent, with its row.
  - Only the carriage return rewrites a key, which is documented and tested.
- **Empty and white-space keys:** `Probe` with key `""` or `" "`, or with upload id `""`, is `RequestNotBuilt` and sends nothing. `crates/validate/src/s3.rs:53-57` does not list these cases, but the report is correct, so "not sent" and "server refused" stay separate. Bulk delete does send `" "`, which is right for a request-body key.
- **Non-ASCII copy source:** `source_key: "é"` is sent as raw bytes and the gateway answers 403. `crates/validate/src/s3/verbs.rs:70-71` says such a key must come encoded, so this is the documented behaviour.
- **Doc figures in `crates/validate/src/s3/verbs.rs:350-359`:** 62,115, 70,115, 65,518 and 65,601 all follow from the stated row sizes, and the tests pin them. The 32-byte figures and the 40-byte 70,115 use real gateway `Content-Length` values.
- **Scope:** the patch does not touch `lib.rs`, `Cargo.toml` or `deny*.toml`. The other three `s3_client_*` test files are unchanged.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. Review of this diff against the read-only target found no introduced correctness bugs or actionable reuse, simplification, or efficiency issues.

Validation evidence: the frozen CI gate and all 55 smoke-verb tests passed. Mutation testing reported 47 caught, 14 unviable, and 3 timeouts; those timeouts remain inconclusive. Checks were not rerun for this advisory review.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the client contract for #939 — callers still own bucket safety and delete-batch sizing, so successful client tests alone do not establish the future runner's operational safety (crates/validate/src/s3/verbs.rs:45, crates/validate/src/s3/verbs.rs:348; brief.md:189).
- [x] **The rejection criterion can pass for the wrong multipart operation.** Brief lines 27–30 require six multipart probes to return `501 NotImplemented`, but the target rejects any recognized bucket subresource through the same branch (`crates/gateway-s3/src/lib.rs:1797–1802`) and any recognized object subresource through another shared branch (`:1836–1847`). A `ListParts` probe accidentally sending `ListMultipartUploads` can therefore satisfy that outcome. Criterion G (brief lines 58–60) supplies success responses but does not require checking the request that elicited them. Require captured method/path/query/header/body assertions for each probe, so a wrong operation or missing operation parameter cannot pass merely by hitting a rejection guard.
- [x] **Criterion B contradicts the existing timeout contract.** Brief lines 37–39 say a request that reached the connector and got no answer “stays `NoResponse`.” The target explicitly reports SDK operation timeouts as `Timeout { phase: Operation }` and connector timeouts as `Timeout { phase: Connect }` (`crates/validate/src/s3.rs:346–355`); these also receive no answer. Qualify the criterion to preserve typed deadline failures, reserve `NoResponse` for non-timeout transport failures, and require probe assertions for both deadline phases. Otherwise the stated acceptance rule permits losing the measured timeout phase.
- [x] **The mandatory salvage prerequisite is unresolved in the supplied material.** Brief lines 7–9 require taking the prior patch's hunks unchanged and forbid re-deriving the design; lines 67, 89–95 and 135–137 also depend on its exact architecture paragraph, implementation and SDK evidence. Neither the review cwd nor `$PDCA_TARGET` contains `results/issue_938/iteration-v3/patch.diff` or its `build-notes.md`; `notes.json` and `sources/` are also absent. This does not prove those artifacts are missing elsewhere, but their availability and the claimed prior red/green evidence cannot be established here. Revise the brief to make the required salvage an explicitly supplied, resolvable input, or include the necessary contract and remove the exclusive instruction to transplant unavailable hunks. Include the relevant tracker excerpts so the bucket-safety constraint and prior-review disposition can be checked.
- [x] size backstop — this slice is behaving oversized: patch is 198 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Test gap: no test uses `~` in a bucket name, so removing it from `path_segment`'s unreserved set passes (`crates/validate/src/s3.rs:552`).
- C4-verify is labelled `pass` / "red pre-fix, green post-fix" when its log says `PASS (green-only)`.
- T4 batch review took about 32 s for three passes over a 203 KB patch; check that it really reviews the whole patch.
