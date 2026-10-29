# Result — issue 853 / validate-s3-client-nonconforming-responses

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: #852's client is expected to trust the SDK's reading of a response. Each case
  below is a **hypothesis for #852 until Do reproduces it on #852's folded commit** (see
  Falsifiability). Each was observed on one of #741's three attempts, whose client #852
  rebuilds; the cause of each sits in the SDK or hyper, which #852 keeps; and #852 defers
  every one of them to this issue by name. The records are in this harness repo under
  `results/issue_741/`:
  - an empty-body 404 becomes `Code("NotFound")`, a code the SDK makes up (aws-sdk-s3
    `protocol_serde.rs:24-28`). `iteration-v1/SUMMARY.md:187-195`;
  - an HTML 500 becomes a clean S3 error. `iteration-v1/SUMMARY.md:206-207`;
  - `<Error><Code>SlowDown</Code>` with no closing tag, a `<Message>` cut off mid-text, or
    junk after `</Error>` (inside a correctly sized body) all become clean S3 errors with a
    code. `iteration-v2/SUMMARY.md:139`;
  - a `200` whose `Last-Modified` fails its grammar becomes an *error response*, with the
    SDK's diagnostic dropped. `iteration-v1/SUMMARY.md:196-208`;
  - on PUT and DELETE, an error body that disagrees with its declared `Content-Length`
    (including chunked framing that also declares a length) is trusted, while GET calls the
    same bytes unreadable, because the SDK enforces length on `GetObject` only.
    `iteration-v3/SUMMARY.md:114` (F3) and `:146-162`;
  - response bodies are collected whole before any client code runs: a lazily streamed,
    chunked 768 MiB error body raised peak memory from 19 MiB to 794 MiB.
    `iteration-v3/SUMMARY.md:110` (F1) and `:164-175`. The cause, checked in the locked
    sources: for every response the SDK does not stream (every error, and every PUT or
    DELETE response including a `2xx`), it reads the whole body before deserializing
    (`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:527-536`). Only a `GetObject`
    success streams (`aws-sdk-s3-1.148.0/src/operation/get_object.rs:337-343`);
  - a GET body framed only by connection close is accepted as a complete, shorter object.
    `iteration-v1/SUMMARY.md:209-217`. Round 3's fix also refused complete chunked GETs,
    which was wider than needed (`iteration-v2/SUMMARY.md:140`).
- Success criterion: BINDING, demonstrable by C4-verify. The production client runs
  against a scripted loopback endpoint that writes exact bytes. The request id in every
  expectation is the `x-amz-request-id` the endpoint sent. Each case below classifies as
  stated, asserted field by field, on **GET, PUT and DELETE** wherever the response can occur:
  - a non-success response with an empty body → S3 error with status and "no body", never a
    code the body did not carry;
  - a non-success response with well-formed XML but no `<Code>` → S3 error, "no code in
    body";
  - a non-success response with a non-XML body (an HTML page) → unreadable;
  - an `<Error>` document that is unclosed, cut off, or followed by anything but whitespace or
    comments, inside a correctly sized body → unreadable;
  - an error body whose received length differs from its declared `Content-Length`, or one
    that is chunked and also declares a length → unreadable, **with the same classification on
    all three operations**;
  - an error response whose XML is complete but whose **chunked framing is not**: (a) the
    connection closes with no terminal `0` chunk, (b) it closes after `0\r\n` with the final
    `\r\n` missing → unreadable, on all three operations. These separate a framing failure
    from an XML failure;
  - a success status the SDK cannot read → unreadable, with the SDK's diagnostic kept in the
    detail and the real status and request id;
  - **the byte budget, on a body with no declared length.** The endpoint sends a
    non-success response with `Transfer-Encoding: chunked` and **no `Content-Length`**, whose
    body is an otherwise valid `<Error>` document — `<Code>SlowDown</Code>`, then a
    `<Message>` whose text the endpoint generates lazily until it is far larger than the
    budget plus socket buffers → unreadable. The endpoint must have written **at most the
    budget plus a stated, bounded slack** before the client closed the connection. The oracle
    is bytes written at the endpoint, not process RSS. Do demonstrates the mutation: remove
    the **streaming** limit while keeping any check on a declared `Content-Length`, and the
    assertion fires. A response that DECLARES an over-budget length may be refused from the
    header alone; that is allowed, but it does not satisfy this case;
  - **the same budget on a PUT or DELETE success body** (the SDK buffers those too): a `200`
    to PUT or DELETE whose chunked body is generated lazily past the budget → unreadable,
    with the same bytes-written oracle;
  - **a valid GET object larger than the budget is NOT cut short:** a GET `200` with a
    correct `Content-Length` of at least four times the budget → **accepted**,
    byte-identical. This guards against a cap applied to every response;
  - a GET `200` with no `Content-Length` and no chunked framing (close-delimited) → body
    error;
  - a GET `200` with chunked framing and a proper terminal chunk → **accepted**,
    byte-identical;
  - a GET `200` with chunked framing that is cut short — (a) no terminal `0` chunk, (b) the
    final `\r\n` after `0\r\n` missing → body error, never a shorter object;
  - a GET whose received length disagrees with its declared length → body error.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: classification and bounded reading of every response a conforming S3 server does
  not send, on all three operations. Plan decisions already taken, not Do's to re-open:
  - **Chunked GET bodies are accepted** when their framing is complete. Chunked framing marks
    its own end, and the rubric names the chunked terminal CRLF as acceptable framing. Only
    close-delimited bodies, and chunked bodies cut short, are refused. This is narrower than
    round 3's "refuse any GET without `Content-Length`".
  - **The byte budget is a fixed default sized for S3 error documents**, which are small. It
    applies to every body the client buffers, and to no successful GET body. Choose and
    state the value, and say in the docs that it exists. Remove the `// deferred:` markers
    #852 left for this issue as each case lands.
  **/ out of scope:** the upload path, including early acknowledgements and peers that stop
  reading (#854); any change to how a well-formed response is handled (the larger-than-budget
  GET case exists to prove there is none); the 5 GiB property (#761); TLS; any edit outside
  `crates/validate/` except the `05-building-block-view.md` paragraph if what the client
  promises changes.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (19 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 71 mutants tested in 6m: 2 missed, 40 caught, 27 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.98s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #853: classify malformed S3 responses consistently across GET, PUT, and DELETE, cap buffered bodies at 64 KiB, and preserve complete streamed GET objects.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding cases distinguish malformed responses, bounded buffering, and valid streamed objects; they are executable without a service dependency (`brief.md:37`, `brief.md:134`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:1264`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the new test produced assertion failures after successful compilation: 12 failed, 7 passed; already-green cases are guards, not reproductions (`reviewer-red.log:543`). |
| C3 Change | PASS | The patch stays within the validator and permitted architecture paragraph, with no upload-path redesign or added test dependency; the restored target matches the supplied patch (`target/crates/validate/Cargo.toml:35`, `target/crates/validate/src/s3.rs:248`, `target/docs/design/architecture/05-building-block-view.md:297`). |
| C4 Verification (red→green) | PASS | Restoring the fix independently made all 19 regression tests pass; the complete validator suite passed 93 tests, and frozen CI supplies the unavailable local lint checks; diff coverage remains unmeasured, not a patch failure (`reviewer-green.log:162`, `reviewer-validate.log:136`, `gate-logs/C4-ci.log:4354`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | Exact boundary cases and socket-write measurements distinguish early bounded reading from collecting then rejecting; all nine independently selected correctness mutants were caught, and no capability/load-time workaround was introduced (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1264`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:1356`, `reviewer-mutants.log:3`). |
| T1 Structure | PASS | Response validation remains local to the client; normal dependencies preserve the blackbox boundary, with independent dependency and global-state guards passing (`target/crates/validate/src/s3/response.rs:218`, `reviewer-guards.log:10`, `reviewer-statics.log:3`). |
| T2 Shape | PASS | Chunked objects expose an absent declared length rather than inventing one, and malformed input retains a typed failure; the changed API and response policy are documented (`target/crates/validate/src/s3/body.rs:418`, `target/crates/validate/src/s3/error.rs:127`, `target/docs/design/architecture/05-building-block-view.md:297`). |
| T3 Runtime | PASS | Real loopback runs reject oversized buffered responses and preserve large GETs; existing deadline, roundtrip, and upload-peer tests also pass, with no new clock source or unbounded production await (`reviewer-green.log:24`, `reviewer-green.log:89`, `reviewer-validate.log:118`, `reviewer-validate.log:136`, `target/crates/validate/src/s3/body.rs:438`). |
| T4 Contribution | N/A | Commit and PR drafts are absent by design; their substantive contribution audit is owed to the mandatory publish-time rerun, not human clearance now (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The prior implementation findings are addressed and no remaining in-scope defect was found; affected-path history and all 19 closed-unmerged PR file sets were checked, with no rejected-work overlap (`target/crates/validate/src/s3/response.rs:91`, `target/crates/validate/src/s3/response.rs:355`, `reviewer-prior-art-paths.jsonl:2`, `reviewer-prior-art-paths.jsonl:9`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the fixed 64 KiB policy and explicit tolerance of `204` with `Content-Length: 0` for the intended validator use — tests establish these behaviors, while their suitability requires owner judgment (`target/crates/validate/src/s3/response.rs:34`, `target/crates/validate/src/s3/response.rs:97`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:1230`). |

No remaining implementation defect was found within the brief's scope. Independent red→green execution, targeted mutations, and the existing validator suites support the change; the outstanding decision is fitness-to-purpose.

- **The regression evidence is independently reproduced.** The disposable target identifies its original base as `61e53418bb3e73674eef9352733f0e985796d582`. I stashed only the production manifest/source changes, including the new response module, retained the regression file, and ran `cargo test --offline --locked -p wyrd-validate --test s3_client_nonconforming_responses` through the target manifest. The pre-fix suite compiled and returned 12 failing tests and seven passing guards. After `git stash pop`, the same suite passed all 19 tests. A subsequent `cargo test --offline --locked -p wyrd-validate` passed 93 tests: 21 argument, 26 payload, 19 malformed-response, 15 roundtrip, and 12 upload-peer tests (`reviewer-red.log:543`, `reviewer-green.log:162`, `reviewer-validate.log:40`, `reviewer-validate.log:72`, `reviewer-validate.log:97`, `reviewer-validate.log:118`, `reviewer-validate.log:136`). The reverse-patch applicability check and `git diff --check` passed after restoration; no reviewer production changes remain.
- **The earlier test gaps are closed.** Exact 65,535/65,536/65,537-byte error documents run with both declared-length and chunked framing on all three operations. In the independent green run, generated-body peers wrote 135,650–229,369 body bytes before closure, below the stated 34,652,160-byte budget-plus-slack bound; without the fix they wrote the entire roughly 69.3 MB bodies (`reviewer-green.log:24`, `reviewer-green.log:27`, `reviewer-green.log:89`, `reviewer-red.log:500`, `reviewer-red.log:513`). Nine focused `cargo mutants --in-diff` mutations covering the budget constant/comparison, transfer-coding guard, 204 framing, and trailing processing instructions were all caught (`reviewer-mutants/mutants.out/caught.txt:1`, `reviewer-mutants.log:3`). The broader frozen run's two survivors change only the interceptor's diagnostic name; its two timeouts are recorded as timeouts, not asserted as successful tests (`gate-logs/C5-mutants.log:13`, `target/crates/validate/src/s3/response.rs:215`). Input validation here addresses untrusted responses directly; the capability-probe smell-test does not apply.
- **Evidence limits are accounted for without manufacturing patch failures.** Independent docs lint, typos, gitlink, unsafe-root, blackbox-dependency, and statics checks passed (`reviewer-docs.log:1`, `reviewer-guards.log:4`, `reviewer-guards.log:7`, `reviewer-guards.log:10`, `reviewer-statics.log:3`; `reviewer-typos.log` is empty on success). Local fmt and Clippy attempts could not run because those toolchain components are absent (`reviewer-fmt.log:1`, `reviewer-clippy.log:1`). The frozen CI log actually records their execution plus build, tests, dependency scans, and DST completion (`gate-logs/C4-ci.log:30`, `gate-logs/C4-ci.log:3715`, `gate-logs/C4-ci.log:4354`). Frozen TiKV compilation also completed (`gate-logs/host-tikv.log:209`), and the batched review reports zero blocking findings without supplying individual review text (`gate-logs/T4-batch-review.log:10`). Diff coverage was never measured because its `origin/main` base rejected the patch; this is a gate-base caveat, while the supplied target applied and compiled successfully (`gate-logs/C4-diff-cov.log:10`). No missing gate log or unmet external dependency remains for this fix.

The prior-art check used every affected file path, not only the brief's obsolete `client.rs` name. GitHub's merged history identifies the existing client and upload work, including [PR #859](https://github.com/getwyrd/wyrd/pull/859) and [PR #865](https://github.com/getwyrd/wyrd/pull/865); the two new response/test paths have no merged history. Listing all 399 closed PRs and comparing the changed-file lists of all 19 unmerged ones produced zero affected-path matches (`reviewer-prior-art-paths.jsonl:1`, `reviewer-prior-art-paths.jsonl:5`, `reviewer-prior-art-paths.jsonl:6`, `reviewer-prior-art-paths.jsonl:9`). The brief's older no-prior-PR assertion is superseded by this check, not treated as a new implementation defect.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #853 (validate: non-conforming S3 responses)

**Verdict: refuted on one serious point.** The patch adds a crash: an error body nested a few thousand levels deep aborts the whole validator process. The pre-patch client handled the same body. The red→green evidence itself holds. Every probe below ran against the production `S3Client` on a loopback endpoint that writes exact bytes (the test file's own harness), in a scratch copy of `$PDCA_TARGET`.

## Findings

- NEEDS-HUMAN [impl] — **A deeply nested error body aborts the process; this is new in this patch.** `crates/validate/src/s3/response.rs:328` passes every buffered error body (up to the 64 KiB budget) to `roxmltree::Document::parse`. roxmltree's tokenizer recurses once per nesting level and has no depth limit (`roxmltree-0.21.1/src/tokenizer.rs:639` → `:683`). A stack overflow in Rust is an abort, not a panic: nothing can catch it, and the validator loses the whole run. Failing case: `503`, correct `Content-Length`, body `<Error><Code>SlowDown</Code>` + `<a>`×4000 + `</a>`×4000 + `</Error>` (28,036 bytes, under the budget). In a release build on a 2 MiB stack (Rust's test-thread default and tokio's worker default; `classify` runs on the caller's thread, `crates/validate/src/s3.rs:266`, `:283`, `:326`), the patched client dies with "thread has overflowed its stack" on the first call (GET). The pre-patch client (`git checkout` of the base) returns `Service{503, Code("SlowDown")}` on GET, PUT and DELETE for this body and for an 8,000-deep, 56 KB one, because the SDK's XML reader does not recurse. roxmltree alone on 2 MiB in release parses 2,000 levels and overflows at 4,000. A debug build overflows at about 200 levels, a 1.4 KB body. Fix: refuse nesting past a small depth before parsing (S3 error documents are two levels deep; a byte scan is enough), or use a reader that does not recurse. Add a nested-body case on all three operations. (Context outside this diff: the gateway parses DeleteObjects request bodies with the same call at `crates/gateway-s3/src/lib.rs:2330`. That deserves its own issue.)

- NEEDS-HUMAN [impl] — **The bodyless-status rule covers 204 but not 304.** `crates/validate/src/s3/response.rs:101` refuses body framing only on a `204`. But hyper gives a `304` no body either (`hyper-1.10.1/src/proto/h1/role.rs:1268` reads `204 | 304`, the same line the comment cites). Failing case: PUT or DELETE answered `304 Not Modified` with `Transfer-Encoding: chunked`, followed by a whole chunked `<Error><Code>InternalError</Code>…</Error>` (171 body bytes written). The result is `Service { status: 304, code: NoBody }` on all three operations: "no body" for a response that sent one. `5\r\nhello\r\n` with no terminal chunk gives the same result. This is never reported as a success, so the stakes are lower than for the 204 case. Still, RFC 9112 §6.1 allows `Transfer-Encoding` on a 304 only in reply to a GET, so on PUT and DELETE this framing is non-conforming and should be unreadable. `Content-Length` on a 304 must stay allowed (RFC 9110 §8.6).

- NEEDS-HUMAN [impl] — **The "nothing but whitespace or comments" rule is enforced after the root but not before it.** `crates/validate/src/s3/response.rs:337-340` refuses a processing instruction after `</Error>` (test `pi-after-error`), on the stated ground that a processing instruction is "no part of an S3 error document". Yet `<?xml version="1.0"?><?proxy x?><Error><Code>SlowDown</Code></Error>` comes back as a clean `Service{503, Code("SlowDown")}` (probe, GET). In the same function, `<Code/>` and `<Code></Code>` give `Code("")`, and `<Code>  </Code>` gives `Code("  ")`. That is a clean S3 error whose code is not a code; `MissingInXml` ("no code in body", `response.rs:370`) is the closer reading. Low severity: each is a one-line check plus one test case.

- NEEDS-HUMAN [human] — **The `length-under-sent` case passes for a reason other than its name.** `crates/validate/tests/s3_client_nonconforming_responses.rs:1027-1041` declares a `Content-Length` that ends inside `<Message>`. It goes red→green only because the XML reader now refuses a cut document; no code compares the bytes sent with the declared length. Move the cut to a document boundary and the client reports a clean error: `503` with `Content-Length` equal to the length of a whole `SlowDown` document, followed by a second `<Error><Code>InternalError</Code>…</Error>`, gives `Service{503, Code("SlowDown")}` on GET, PUT and DELETE (the endpoint wrote 236 body bytes against 165 declared). Extra bytes after a chunked body's `0\r\n\r\n` behave the same. In HTTP terms those bytes are not part of the message, and hyper never passes them up, so catching them probably needs the socket, which the SDK connector does not expose. A human should decide two things. First, whether the brief's "received length differs from its declared Content-Length → unreadable" is met. Second, whether the test module docs (`:14-15`) and the `s3.rs` module docs should state this limit instead of implying full coverage.

- NEEDS-HUMAN [human] — **A PUT or DELETE success with an unexpected body is still a receipt.** Probe: PUT or DELETE answered `200`, `Content-Length: 4`, body `junk` gives `Ok(())`. A `200` with no framing and the cut body `<Err` (close-delimited) also gives `Ok(())`. S3 sends no body on these successes, and the brief's invariant says a non-conforming response is never "reported as a success". But the scope line rules out "any change to how a well-formed response is handled". Whether a non-empty PUT or DELETE success body falls in scope is a human call. A `200` carrying an `<Error>` document is already `Unreadable`, which is correct.

- NEEDS-HUMAN [human] — **The T4 pass is weak evidence.** `check-gates.json` reports T4 as "0 blocking, 0 recorded-rejected, 0 noise-dropped", from a 36-second run of a "3x" review over a 2,467-line diff. The frozen `gate-logs/T4-batch-review.log` holds only that summary line, with no list of findings. The previous round's T4 found 4 blockers. Given the abort above, do not treat the T4 pass as a check of the new XML reader.

## Attacks that failed

- **Red→green evidence:** genuine. `gate-logs/C4-verify.log` shows the production revert compiling against #852's API, with 12 of 19 tests failing by assertion and all 19 passing with the fix. The tests drive the public `S3Client`, not a copy of it. The 7 tests that pass on the base are guards (bodiless error, HTML page, incomplete chunked error framing, a GET 16× the budget, a close-delimited GET, a chunked GET cut short, a GET in a non-chunked coding), which matches the brief's prediction.
- **Budget:** the exact boundary cases (`budget-less-1`, `budget`, `budget-plus-1`, both framings, all three operations, `s3_client_nonconforming_responses.rs:1264-1312`) now pin 64 KiB, which closes round 1's "a 16 MiB budget also passes" gap. The generated-body cases still catch a check that runs only after the whole body is collected. A 60 KiB document sent in one-byte chunks (368,645 wire bytes) is read correctly. hyper caps chunk extensions and trailers at 16 KiB each (`hyper-1.10.1/src/proto/h1/decode.rs:20`, `:25`), so nothing outside the counted data frames grows without bound.
- **Framing agrees with hyper** (`response.rs:85-106`): tab whitespace around the value, uppercase `CHUNKED`, repeated or comma-listed equal `Content-Length` values, a `205` with cut chunked framing, a GET with trailers, and `gzip` or TE plus CL on a 304 all classify the way hyper reads them.
- **Panic risk at `crates/validate/src/s3.rs:289`** (`.expect` on the framing slot): the guard's success test matches the SDK's streaming rule (`aws-sdk-s3-1.152.0/src/operation/get_object.rs:339`), and `modify_before_deserialization` runs before any output exists (`aws-smithy-runtime-1.16.0/src/client/orchestrator.rs:512`). I found no path to the panic.
- **C5 and C4-diff-cov:** the two surviving mutants change `Intercept::name`, which no test can observe. The two timeouts are mutants the suite does catch, only slowly. C4-diff-cov's red comes from the bundle not applying on `origin/main` (#852 is not there), not from missing coverage.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed the changed source and tests at `$PDCA_TARGET` against the brief and frozen gate evidence. The recorded CI passed; all 19 regression tests passed with the patch, and 12 failed with production changes reverted. The two surviving mutants only change the interceptor diagnostic name (`crates/validate/src/s3/response.rs:215`). Diff coverage was not measured because the patch did not apply to `origin/main`. No tests were rerun for this advisory review.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept the fixed 64 KiB policy and explicit tolerance of `204` with `Content-Length: 0` for the intended validator use — tests establish these behaviors, while their suitability requires owner judgment (`target/crates/validate/src/s3/response.rs:34`, `target/crates/validate/src/s3/response.rs:97`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:1230`).
- [ ] **A deeply nested error body aborts the process; this is new in this patch.** `crates/validate/src/s3/response.rs:328` passes every buffered error body (up to the 64 KiB budget) to `roxmltree::Document::parse`. roxmltree's tokenizer recurses once per nesting level and has no depth limit (`roxmltree-0.21.1/src/tokenizer.rs:639` → `:683`). A stack overflow in Rust is an abort, not a panic: nothing can catch it, and the validator loses the whole run. Failing case: `503`, correct `Content-Length`, body `<Error><Code>SlowDown</Code>` + `<a>`×4000 + `</a>`×4000 + `</Error>` (28,036 bytes, under the budget). In a release build on a 2 MiB stack (Rust's test-thread default and tokio's worker default; `classify` runs on the caller's thread, `crates/validate/src/s3.rs:266`, `:283`, `:326`), the patched client dies with "thread has overflowed its stack" on the first call (GET). The pre-patch client (`git checkout` of the base) returns `Service{503, Code("SlowDown")}` on GET, PUT and DELETE for this body and for an 8,000-deep, 56 KB one, because the SDK's XML reader does not recurse. roxmltree alone on 2 MiB in release parses 2,000 levels and overflows at 4,000. A debug build overflows at about 200 levels, a 1.4 KB body. Fix: refuse nesting past a small depth before parsing (S3 error documents are two levels deep; a byte scan is enough), or use a reader that does not recurse. Add a nested-body case on all three operations. (Context outside this diff: the gateway parses DeleteObjects request bodies with the same call at `crates/gateway-s3/src/lib.rs:2330`. That deserves its own issue.)
- [ ] **The bodyless-status rule covers 204 but not 304.** `crates/validate/src/s3/response.rs:101` refuses body framing only on a `204`. But hyper gives a `304` no body either (`hyper-1.10.1/src/proto/h1/role.rs:1268` reads `204 | 304`, the same line the comment cites). Failing case: PUT or DELETE answered `304 Not Modified` with `Transfer-Encoding: chunked`, followed by a whole chunked `<Error><Code>InternalError</Code>…</Error>` (171 body bytes written). The result is `Service { status: 304, code: NoBody }` on all three operations: "no body" for a response that sent one. `5\r\nhello\r\n` with no terminal chunk gives the same result. This is never reported as a success, so the stakes are lower than for the 204 case. Still, RFC 9112 §6.1 allows `Transfer-Encoding` on a 304 only in reply to a GET, so on PUT and DELETE this framing is non-conforming and should be unreadable. `Content-Length` on a 304 must stay allowed (RFC 9110 §8.6).
- [ ] **The "nothing but whitespace or comments" rule is enforced after the root but not before it.** `crates/validate/src/s3/response.rs:337-340` refuses a processing instruction after `</Error>` (test `pi-after-error`), on the stated ground that a processing instruction is "no part of an S3 error document". Yet `<?xml version="1.0"?><?proxy x?><Error><Code>SlowDown</Code></Error>` comes back as a clean `Service{503, Code("SlowDown")}` (probe, GET). In the same function, `<Code/>` and `<Code></Code>` give `Code("")`, and `<Code>  </Code>` gives `Code("  ")`. That is a clean S3 error whose code is not a code; `MissingInXml` ("no code in body", `response.rs:370`) is the closer reading. Low severity: each is a one-line check plus one test case.
- [ ] **The `length-under-sent` case passes for a reason other than its name.** `crates/validate/tests/s3_client_nonconforming_responses.rs:1027-1041` declares a `Content-Length` that ends inside `<Message>`. It goes red→green only because the XML reader now refuses a cut document; no code compares the bytes sent with the declared length. Move the cut to a document boundary and the client reports a clean error: `503` with `Content-Length` equal to the length of a whole `SlowDown` document, followed by a second `<Error><Code>InternalError</Code>…</Error>`, gives `Service{503, Code("SlowDown")}` on GET, PUT and DELETE (the endpoint wrote 236 body bytes against 165 declared). Extra bytes after a chunked body's `0\r\n\r\n` behave the same. In HTTP terms those bytes are not part of the message, and hyper never passes them up, so catching them probably needs the socket, which the SDK connector does not expose. A human should decide two things. First, whether the brief's "received length differs from its declared Content-Length → unreadable" is met. Second, whether the test module docs (`:14-15`) and the `s3.rs` module docs should state this limit instead of implying full coverage.
- [ ] **A PUT or DELETE success with an unexpected body is still a receipt.** Probe: PUT or DELETE answered `200`, `Content-Length: 4`, body `junk` gives `Ok(())`. A `200` with no framing and the cut body `<Err` (close-delimited) also gives `Ok(())`. S3 sends no body on these successes, and the brief's invariant says a non-conforming response is never "reported as a success". But the scope line rules out "any change to how a well-formed response is handled". Whether a non-empty PUT or DELETE success body falls in scope is a human call. A `200` carrying an `<Error>` document is already `Unreadable`, which is correct.
- [ ] **The T4 pass is weak evidence.** `check-gates.json` reports T4 as "0 blocking, 0 recorded-rejected, 0 noise-dropped", from a 36-second run of a "3x" review over a 2,467-line diff. The frozen `gate-logs/T4-batch-review.log` holds only that summary line, with no list of findings. The previous round's T4 found 4 blockers. Given the abort above, do not treat the T4 pass as a check of the new XML reader.
- [x] **The claimed reproductions are not established for #852.** `brief.md:8-23` attributes defects to “#852's client” using #741's unpublished attempts, and `brief.md:50-57` promises an assertion-only red against #852. Neither `notes.json` nor `sources/` is supplied. The repo and `main` resolve, but `dependency-state.json:2-5` records #852 as existing and `PLANNED`; the resolved target at `36f006d` has no `crates/validate/`, consistent with `$PDCA_TARGET/Cargo.toml:9-32`. This does not disprove the defects, but it prevents verifying the root cause, public API, dev-dependencies, or prior-attempt constraints. Revise these claims to hypotheses pending reproduction on the folded #852 commit, and identify the evidence needed to establish that baseline.
- [x] **The budget oracle can pass without enforcing a streamed-byte limit.** `brief.md:40-44` requires a lazily streamed oversized error and a “remove the budget” mutation, but leaves its framing unspecified. A fixture advertising an over-budget `Content-Length` permits a header-only rejection to satisfy both checks while unknown-length bodies remain unbounded. Require an otherwise-valid, oversized `<Error>` document sent chunked without `Content-Length`, with a long `<Message>` crossing the budget. Remove the streaming limiter while retaining any header precheck for the mutation. That makes the assertion distinguish bounded reading from merely trusting the advertised size.
- [x] **Chunked completeness has no negative acceptance case.** `brief.md:45-49` checks close-delimited refusal, a correctly terminated chunked GET, and declared-length disagreement; none requires rejection of an incomplete chunked body without `Content-Length`. Yet the cited invariant explicitly requires the terminal CRLF (`$PDCA_TARGET/AGENTS.md:161-164`). Add missing-terminal-chunk and truncated-terminal-CRLF cases: body error for GET success, and unreadable on all three operations for error responses whose XML is otherwise complete. These isolate framing failure from XML failure and prevent the positive chunked case from standing in for truncation detection.
- [x] **The all-response budget promise exceeds the scoped change.** `brief.md:5-7,58-61` promises bounded reading/memory for every response, but `brief.md:40-44,80-84` specifies an error-document budget and excludes changes to well-formed responses. A total-byte cap sized for error documents would reject legitimate larger GETs; an error-only cap cannot establish the universal memory claim. The target proposal explicitly includes large valid objects (`$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:283-291`). Narrow the guarantee to buffered error bodies, or separately define bounded-memory streaming for successful GETs and require a valid object larger than the error budget to remain accepted. Otherwise Do inherits an unstated second obligation beyond the named budget test.
- [x] size backstop — this slice is behaving oversized: patch is 111 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C1 Spec — Decide whether the length guarantee includes surplus wire bytes beyond `Content-Length` — a declared 3-byte GET carrying `abcdef` returns `abc` successfully, while the SDK exposes only the declared body; this determines whether lower-level transport work belongs in scope (`brief.md:79`, `target/crates/validate/src/s3/body.rs:223`, `review-probes.log:5`).
- [ ] **A body longer than its declared `Content-Length` is never detected, on any operation.** `crates/validate/tests/s3_client_nonconforming_responses.rs:921-935` (`length-under-sent`) goes green only because the declared length ends inside `<Message>`, so the XML check catches it. No length check is involved. Move the declared end to the `</Error>` close tag and send more bytes after it (`Content-Length: doc.len()`, body `doc + "<html>proxy junk…</html>"`). Then GET, PUT and DELETE each report a clean `Service { 503, Code("SlowDown"), "Please reduce your request rate." }` (probe run). A GET 200 with `Content-Length: 2000` and 3000 bytes sent is accepted as a 2000-byte object (probe run). The brief makes both cases binding ("an error body whose received length differs from its declared `Content-Length` … → unreadable"; "a GET whose received length disagrees with its declared length → body error"). The patch's own comment admits the gap: the length checks "cannot fire on any framing a peer can send" (`crates/validate/src/s3/body.rs:223-229`). The GET test leaves this direction out entirely (`:1255-1290`). hyper stops reading at the declared length and never reports the extra bytes. Catching them needs a connector-level check, so this is a scope call: build that check, or narrow the brief and the test names to "declared more than sent".
- [ ] **An empty `<Code></Code>` is reported as a clean S3 error with code `""`.** On all three operations the result is `Service { 503, Code(""), Some("m") }` (probe run; `crates/validate/src/s3/response.rs:327-339`). The brief says "never a code the body did not carry", and an empty string is not an S3 code. Someone needs to decide whether an empty or whitespace-only `<Code>` should be `MissingInXml`, `Unreadable`, or stay as it is.

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
- Iteration delta (if iterating): Gates pass, but the patch adds a process-killing crash; fix it and close the remaining gaps. Keep the slice as is (size warning waived by the human). - Crash: roxmltree recurses per nesting level with no limit, so a deeply nested error body (e.g. <a>x4000, 28 KB; ~200 levels in debug) overflows the stack and aborts the validator (response.rs:328). Refuse nesting past a small depth before parsing (S3 error documents are two levels deep), or use a non-recursive reader. Add a nested-body case on GET, PUT and DELETE. - Extra bytes past the declared length: the human chose to BUILD the connection-level check (in scope by the human's decision). A body longer than its Content-Length must be unreadable on error responses and a body error on a GET success, including bytes after a chunked body's terminal chunk. Add the adversary's cases: whole SlowDown document then trailing junk/second <Error> on all three operations; GET 200 declaring 2000 and sending 3000. Make length-under-sent fail for the length reason, not the XML reason. - A PUT or DELETE 2xx carrying an unexpected body (e.g. Content-Length: 4 "junk", or a close-delimited "<Err") is NOT a success: report it as unreadable. Add tests. - Small gaps: a 304 with Transfer-Encoding on PUT/DELETE is unreadable, not NoBody (Content-Length on a 304 stays allowed); refuse a processing instruction before the root as well as after it; an empty or whitespace-only <Code> is MissingInXml, not Code(""). One test case each.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
