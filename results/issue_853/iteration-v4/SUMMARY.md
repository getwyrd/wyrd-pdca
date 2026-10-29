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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (25 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 150 mutants tested in 11m: 4 missed, 90 caught, 51 unviable, 5 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.53s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No new implementation defect found in #853’s strict, bounded S3-response handling, including rejection of bytes after chunked trailers; fitness-to-purpose remains human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance criteria distinguish malformed responses from valid streamed objects and explicitly require the trailer counterexamples and clean controls (`brief.md:37`, `brief.md:192`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the regression test produced 18 assertion failures and 7 passing guards, establishing actual pre-fix defects rather than a compilation failure (`review-red.log:492`). |
| C3 Change | PASS | The response-validation work and connection-level inspection are authorized by the carry-forward decisions; the current trailer repair stays within that settled scope (`brief.md:183`, `brief.md:195`, `target/crates/validate/src/s3/wire.rs:318`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch produced 25/25 regression passes and 103/103 crate tests; CI’s two sandbox cache-lock failures passed separate identical-policy reruns, while diff coverage remains unmeasured (`review-restored-green.log:108`, `review-deny.log:7`, `review-deny-all-features.log:1`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | A trailer can no longer signal completion before the connection check; the frame-level test requires clean trailers to survive and overrun trailers to be withheld, directly addressing the premature-end cause (`target/crates/validate/src/s3/wire.rs:318`, `target/crates/validate/src/s3/wire.rs:437`, `review-restored-green.log:10`). |
| T1 Structure | PASS | All three operations share the response boundary, with per-call state and an owned connection task; no new backend coupling or shared mutable global is introduced (`target/crates/validate/src/s3/wire.rs:125`, `target/crates/validate/src/s3/response.rs:209`). |
| T2 Shape | PASS | The published client contract covers strict framing, the buffer budget, and the connector’s observation boundary; the living architecture description changes with the API behavior (`target/crates/validate/src/s3/wire.rs:24`, `target/crates/validate/src/s3/response.rs:35`, `target/docs/design/architecture/05-building-block-view.md:297`). |
| T3 Runtime | PASS | Trailer completion waits on the owned connection task rather than further peer traffic, and existing body deadlines still bound the read; the roundtrip and upload-lifetime suites remain green (`target/crates/validate/src/s3/wire.rs:190`, `target/crates/validate/src/s3/body.rs:464`, `review-restored-green.log:129`, `review-restored-green.log:147`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must rerun at publish, so the deferred row requires no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No unresolved implementation concern remains in the requested trailer revision; affected-path history and closed/unmerged file lists reveal no rejected equivalent approach, and prior scope decisions remain settled (`brief.md:197`, `review-prior-art.txt:39`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:2053`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve these response classifications for the validator’s intended deployment use — real client/loopback tests establish wire behavior, while whether those observations support useful deployment verdicts remains a product judgment (`brief.md:96`, `target/crates/validate/src/s3/wire.rs:24`). |

- **The requested trailer repair holds on the production path.** Junk, a second HTTP response, and additional chunks after trailers are exercised on GET success, errors on all three operations, and PUT/DELETE success. Matching clean trailers remain accepted, including byte-identical GET content (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1284`, `:1570`, `:1872`, `:2053`). The frame-level test also checks that an overrun produces an error before any trailer is released (`target/crates/validate/src/s3/wire.rs:437`). This removes the premature-completion cause; it is not a capability probe or a load-time fallback. No C5 symptom-guard trigger applies.

- **Independent execution establishes red→green.** The disposable target’s base is `a227aeb`, labelled `pre-fix base 61e53418bb3e73674eef9352733f0e985796d582`. I ran `cargo test -p wyrd-validate --test s3_client_nonconforming_responses`, stashed production/manifests while retaining the new test, reran it against the base, then restored the stash and ran `cargo test -p wyrd-validate`. The unchanged regression test compiled on the base and failed 18 tests; restored, all 25 passed. The complete crate run passed 103 tests, including 4 connector unit tests, 15 roundtrip tests and 12 upload-peer tests (`review-red.log:492`, `review-restored-green.log:12`, `review-restored-green.log:108`, `review-restored-green.log:129`, `review-restored-green.log:147`). Binary diff comparison and SHA-256 checks confirmed the restored tracked changes and all three new files match their original contents. The target was readable and usable; no target-state fallback was needed.

- **Verification caveats concern evidence coverage and sandbox storage, not a reproduced patch defect.** I reran `cargo xtask ci`: formatting, clippy, builds, workspace tests, docs, repository/dependency guards, conformance, and simulation checks completed successfully. The command exited 1 solely because both advisory checks tried to lock the read-only shared database (`review-ci.log:3604`, `review-ci.log:4241`). Both passed with the same policies and a writable database under this review directory; parsed configurations were identical after removing the added `advisories.db-path` (`review-deny.log:7`, `review-deny-all-features.log:1`). The scanner also encountered intentionally malformed temporary test fixtures; it returned success, and those fixtures were retained outside the target afterward. Frozen CI independently reports success (`gate-logs/C4-ci.log:4370`). Frozen diff coverage reports only that the patch would not apply to its `origin/main` baseline, so coverage was not measured; this does not contradict the executable supplied target (`gate-logs/C4-diff-cov.log:10`). The frozen mutation scan reports 90 caught, 51 unviable, 5 timeouts and 4 missed: two interceptor-name changes, the socket-error guard, and task-abort deletion (`gate-logs/C5-mutants.log:15`). The latter two leave fault/cancellation coverage gaps, but the current source retains fail-closed socket handling and abort-on-drop; they do not demonstrate a defect in this trailer revision (`target/crates/validate/src/s3/wire.rs:218`, `target/crates/validate/src/s3/wire.rs:233`). Mutation and TiKV feature checks were adjudicated from their supplied logs, not claimed as independently rerun; TiKV evidence establishes compilation only (`gate-logs/host-tikv.log:209`). No external dependency named by the brief remains unexercised.

- **Prior art and the standing rubric were checked without reopening settled findings.** I queried merged/default-branch history by every affected file path, including all pages of lockfile history, and checked changed-file lists for all 19 returned closed, unmerged PRs. Existing client/upload work is merged in [#859](https://github.com/getwyrd/wyrd/pull/859) and [#865](https://github.com/getwyrd/wyrd/pull/865); no closed/unmerged PR touches `crates/validate/`, and the manifest overlaps are dependency updates (`review-prior-history.json:728`, `review-prior-art.json:247`, `review-prior-art.txt:39`). The supplied batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). Protocol boundaries, parser strictness, deadline ownership, task lifetime, dependency direction and docs currency were checked against the target’s root rubric. No `INTEGRATION.md` was present in the supplied target, so no additional human-only items could be extracted from it. The size waiver and previously accepted scope remain settled; the only human verdict raised here is fitness-to-purpose.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #853, round 4 (trailer hold)

This round's fix is real: the test goes red without it, on the production path. But the trailer frame it now hands over crashes the SDK's checksum checker. One finding.

- NEEDS-HUMAN [impl] — **A chunked GET with a trailer section and any `x-amz-checksum-*` header makes the client panic instead of returning an error.** `crates/validate/src/s3/wire.rs:347-349` hands the held trailer section to the SDK once the connection check passes. On a GET, the SDK wraps the body in its response-checksum validator whenever the response has a checksum header (default `WhenSupported`, `aws-sdk-s3-1.152.0/src/http_response_checksum.rs:120-139`). The inner `SdkBody` turns the cached trailers back into a frame (`aws-smithy-types-1.8.1/src/body/http_body_1_x.rs:32-37`), and the validator calls `frame.data_ref().expect("Data frame should have data")` on every frame (`aws-smithy-checksums-0.65.0/src/body/validate.rs:57`). I reproduced this with the production client against a loopback endpoint (a scratch probe that uses only the public API):
  - `200`, `Transfer-Encoding: chunked`, `x-amz-checksum-crc32: NhCmhg==` (the correct CRC32 of `hello`), body `5\r\nhello\r\n0\r\nx-t: v\r\n\r\n`: `ObjectBody::next_piece` (`crates/validate/src/s3/body.rs:464`) panics with "Data frame should have data". A wrong CRC32 (`AAAAAA==`) or a made-up `x-amz-checksum-crc64nvme` panics the same way. The same response without the trailer line, or without the checksum header, is accepted.
  - `503`, same headers, a whole chunked `SlowDown` document with a trailer section: `get_object` panics, while PUT and DELETE return `Service { code: SlowDown }`. So the same bytes do not get the same verdict on all three operations, which the brief's self-test requires.
  - On the base (#852) the `200` case returns `Body(LengthUndeclared)`, because #852 refused every chunked GET. So the GET-success panic is new with this patch. The `503` panic already happens on the base, but the patch's wire layer is now the natural place to close it.
  - Fix, checked in scratch: at `State::Ended`, drop the trailer section instead of returning it (`return Poll::Ready(None)`). S3 defines no response trailers, and nothing in the client reads them. With that one change, the correct-checksum `200` is accepted, a wrong checksum becomes `Body(Transport { "body checksum mismatch …" })` (with trailers handed over, the checksum check never ran even before it panicked), the GET `503` is `Service { SlowDown }` like PUT and DELETE, and `s3_client_nonconforming_responses` stays 25/25. Only the unit test at `wire.rs:438-460` would need rewriting, because it asserts that the trailers are handed over. This departs from round 3's literal "then release the trailers", but keeps its purpose: the check still runs before the body ends.
  - Tests to add: a GET `200` and a GET/PUT/DELETE `503`, each with a checksum header plus a trailer section. The existing trailer cases (`crates/validate/tests/s3_client_nonconforming_responses.rs:1872-1882`, `:1308-1324`) send no checksum header. That is why the suite, the reviewer and T4's "0 blocking" (finished in 31 s) all missed this.

## Attacks that did not land

- **Red→green evidence.** In a scratch copy the suite passed 25/25 on three runs, plus 4/4 lib tests. I then put back round 3's behavior (trailers returned at once, `wire.rs:322-325`). All three `chunked-trailer-then-{junk,a-second-response,more-chunks} [GET]` cases went red ("accepted as a 3000-byte object") and the `wire.rs` unit test failed. So the test catches this round's bug, for the right reason. The PUT/DELETE trailer cases pass with or without the hold, because the SDK's `collect()` polls to the end anyway. The hold matters only for GET, and GET is covered.
- **Hang or spin after the held trailers.** After the trailers frame, hyper's `Incoming` returns `None`: the oneshot reads as cancelled once its value is taken (`hyper-1.10.1/src/body/incoming.rs:229-233`). So the `continue` at `wire.rs:324` cannot loop or hang.
- **Malformed trailer sections.** A field line with no colon, an obs-fold (an old-style folded header line) in a trailer, and bare-LF chunk lines all come back as hyper body errors. None is accepted.
- **Late junk.** Junk sent 100 ms after a clean trailer section is accepted. This is the limit documented at `wire.rs:24-26` and settled in round 2, so I did not raise it.
- **C5 survivors** (mutants, i.e. deliberate small code changes the tests did not catch). The `wire.rs:237` mutant (`Drop` → `()`) is equivalent: dropping `stop` closes the oneshot, so `drive` ends at `wire.rs:200` anyway. The `wire.rs:220` and `response.rs:233` mutants are low value and were already there last round. Not raised.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No actionable findings on either advisory lens: correctness bugs introduced by this diff, or reuse, simplification, and efficiency improvements.

The trailer handling at `crates/validate/src/s3/wire.rs:317` withholds trailers until the connection check completes. Regression cases cover clean trailers and trailers followed by junk, a second response, or more chunks (`crates/validate/tests/s3_client_nonconforming_responses.rs:1284`, `:1570`, `:1849`, `:2023`); the frame-level test also verifies that an overrun replaces the trailer frame (`crates/validate/src/s3/wire.rs:438`).

Validation used the frozen gate evidence, without rerunning builds: CI passed; all 25 response tests passed with the fix, and 18 failed without it. The four surviving mutants do not establish an actionable defect. Diff coverage was unavailable because the patch did not apply to origin/main.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Approve these response classifications for the validator’s intended deployment use — real client/loopback tests establish wire behavior, while whether those observations support useful deployment verdicts remains a product judgment (`brief.md:96`, `target/crates/validate/src/s3/wire.rs:24`).
- [ ] **A chunked GET with a trailer section and any `x-amz-checksum-*` header makes the client panic instead of returning an error.** `crates/validate/src/s3/wire.rs:347-349` hands the held trailer section to the SDK once the connection check passes. On a GET, the SDK wraps the body in its response-checksum validator whenever the response has a checksum header (default `WhenSupported`, `aws-sdk-s3-1.152.0/src/http_response_checksum.rs:120-139`). The inner `SdkBody` turns the cached trailers back into a frame (`aws-smithy-types-1.8.1/src/body/http_body_1_x.rs:32-37`), and the validator calls `frame.data_ref().expect("Data frame should have data")` on every frame (`aws-smithy-checksums-0.65.0/src/body/validate.rs:57`). I reproduced this with the production client against a loopback endpoint (a scratch probe that uses only the public API):
- [ ] **The claimed reproductions are not established for #852.** `brief.md:8-23` attributes defects to “#852's client” using #741's unpublished attempts, and `brief.md:50-57` promises an assertion-only red against #852. Neither `notes.json` nor `sources/` is supplied. The repo and `main` resolve, but `dependency-state.json:2-5` records #852 as existing and `PLANNED`; the resolved target at `36f006d` has no `crates/validate/`, consistent with `$PDCA_TARGET/Cargo.toml:9-32`. This does not disprove the defects, but it prevents verifying the root cause, public API, dev-dependencies, or prior-attempt constraints. Revise these claims to hypotheses pending reproduction on the folded #852 commit, and identify the evidence needed to establish that baseline.
- [ ] **The budget oracle can pass without enforcing a streamed-byte limit.** `brief.md:40-44` requires a lazily streamed oversized error and a “remove the budget” mutation, but leaves its framing unspecified. A fixture advertising an over-budget `Content-Length` permits a header-only rejection to satisfy both checks while unknown-length bodies remain unbounded. Require an otherwise-valid, oversized `<Error>` document sent chunked without `Content-Length`, with a long `<Message>` crossing the budget. Remove the streaming limiter while retaining any header precheck for the mutation. That makes the assertion distinguish bounded reading from merely trusting the advertised size.
- [ ] **Chunked completeness has no negative acceptance case.** `brief.md:45-49` checks close-delimited refusal, a correctly terminated chunked GET, and declared-length disagreement; none requires rejection of an incomplete chunked body without `Content-Length`. Yet the cited invariant explicitly requires the terminal CRLF (`$PDCA_TARGET/AGENTS.md:161-164`). Add missing-terminal-chunk and truncated-terminal-CRLF cases: body error for GET success, and unreadable on all three operations for error responses whose XML is otherwise complete. These isolate framing failure from XML failure and prevent the positive chunked case from standing in for truncation detection.
- [ ] **The all-response budget promise exceeds the scoped change.** `brief.md:5-7,58-61` promises bounded reading/memory for every response, but `brief.md:40-44,80-84` specifies an error-document budget and excludes changes to well-formed responses. A total-byte cap sized for error documents would reject legitimate larger GETs; an error-only cap cannot establish the universal memory claim. The target proposal explicitly includes large valid objects (`$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:283-291`). Narrow the guarantee to buffered error bodies, or separately define bounded-memory streaming for successful GETs and require a valid object larger than the error budget to remain accepted. Otherwise Do inherits an unstated second obligation beyond the named budget test.
- [x] size backstop — this slice is behaving oversized: patch is 177 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): One bug left: a chunked GET whose trailer section comes with any `x-amz-checksum-*` header panics ("Data frame should have data") instead of returning a verdict, because the held trailers are handed to the SDK's checksum validator (crates/validate/src/s3/wire.rs:347-349). A GET 503 with the same framing also panics, while PUT and DELETE return Service{SlowDown}. - Fix: at `State::Ended`, drop the trailer section instead of returning it (`Poll::Ready(None)`), still after the connection's extra-bytes check. S3 defines no response trailers and nothing in the client reads them. - Rewrite the wire.rs:438-460 unit test, which asserts the trailers are handed over. Update any doc (s3.rs module doc, 05-building-block-view.md) that says trailers are released. - Tests to add: GET 200 chunked + trailer + correct checksum header -> accepted byte-identical; same with a wrong checksum -> body error (checksum mismatch); GET/PUT/DELETE 503 chunked SlowDown + trailer + checksum header -> the same Service{SlowDown} verdict on all three. - Change nothing else; the rest of the patch passed review. The size waiver still stands (waived again this round).
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
