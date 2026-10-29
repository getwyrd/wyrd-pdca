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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (28 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 150 mutants tested in 12m: 4 missed, 90 caught, 51 unviable, 5 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.42s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #853’s strict, bounded S3-response handling and checksum/trailer crash fix: no new implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is executable and the latest requested correction is explicit; prior scope and size decisions remain settled (`brief.md:205`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:1968`). |
| C2 Reproduction (red pre-fix) | PASS | Independently compiled against the stashed production baseline: 20 failures and 8 passing guards, including the SDK trailer panic (`pdca-reviewer-853-evidence/red.log:287`, `pdca-reviewer-853-evidence/red.log:741`). |
| C3 Change | PASS | The promised response contract and living architecture agree; the connector changes implement the previously authorized connection-level check (`target/crates/validate/src/s3/wire.rs:14`, `target/docs/design/architecture/05-building-block-view.md:297`, `brief.md:183`). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all 28 response tests pass; all 106 validator tests, clippy, formatting, docs lint and repository guards independently pass (`pdca-reviewer-853-evidence/summary.txt:6`). |
| C5 Causal adequacy | PASS | Removing trailer frames from the SDK’s input eliminates the demonstrated panic while preserving checksum failures and post-body checks; this is not an optional-capability probe or a load-time workaround (`target/crates/validate/src/s3/wire.rs:321`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:1986`). |
| T1 Structure | PASS | The validator remains independent of Wyrd production crates, and transport ownership stays within its S3 adapter; the dependency guard ran successfully (`pdca-reviewer-853-evidence/guards.log:10`, `target/crates/validate/src/s3/wire.rs:95`). |
| T2 Shape | PASS | Framing, bounded collection and typed classification remain distinct, with documented API semantics and no unrelated feature expansion (`target/crates/validate/src/s3/response.rs:80`, `target/crates/validate/src/s3/body.rs:429`). |
| T3 Runtime | PASS | Actual loopback runs preserve large GET streaming, enforce exact buffered-body boundaries, reject trailing bytes and retain upload cancellation/deadline behavior (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1754`, `pdca-reviewer-853-evidence/green-restored.log:348`, `pdca-reviewer-853-evidence/green-restored.log:387`). |
| T4 Contribution | N/A | Commit/PR artifacts are intentionally absent at Check; their substantive contribution audit must run at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The latest checksum/trailer regression is covered by positive and negative production-client tests; affected-path history and all closed-unmerged PR file lists reveal no competing response fix (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1378`, `pdca-reviewer-853-evidence/summary.txt:13`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the tested contract for the intended validation workload — extra-byte detection sees only bytes already received at response end, and fresh connections impose the documented throughput cost (`target/crates/validate/src/s3/wire.rs:24`, `target/crates/validate/src/s3.rs:217`). |

The independent evidence supports the requested correction. A correct checksum with chunked trailers returns the byte-identical object; a wrong checksum returns a checksum body error; the same checksummed 503 document returns `Service(SlowDown)` on GET, PUT and DELETE. Trailer-followed-by-junk cases still fail. These observations are recorded in `pdca-reviewer-853-evidence/green-restored.log:106`, `:128`, `:153`, `:175` and `:202`; the frame-level regression also passes at `:24`.

The red run used the supplied disposable target, retaining the new test while stashing production changes and manifests. Restoration succeeded and the complete source-file hash inventory matched; the restored tree also passes reverse patch applicability. Eight already-passing baseline tests are guards, not claimed reproductions. The Rust compiler, real SDK/hyper stack and real loopback sockets were exercised; no unmet external dependency or substitute tool is needed for the brief’s proof (`brief.md:134`, `pdca-reviewer-853-evidence/summary.txt:4`).

Frozen gate evidence has the following limits; none is converted into an unsupported patch defect:

| Gate | Adjudication | Evidence |
|------|--------------|----------|
| C4-ci | PASS from captured execution; full workspace CI was not independently replayed. | The log records fmt/clippy/build/tests, dependency checks, conformance and DST completion (`gate-logs/C4-ci.log:3733`, `gate-logs/C4-ci.log:4373`). |
| C4-verify | PASS, independently reproduced. | Frozen green and red runs agree with the independent result (`gate-logs/C4-verify.log:10`, `pdca-reviewer-853-evidence/red.log:741`). |
| C4-diff-cov | Measurement unavailable; the advisory gate is red, not proof of a code failure. | Its origin/main applicability check stopped before measuring coverage; this supplied target compiles and runs the patch (`gate-logs/C4-diff-cov.log:10`). |
| C5-mutants | Advisory FAIL retained; no claim of complete mutation coverage. | 150 tested: 90 caught, 4 missed, 51 unviable, 5 timeouts. Two misses change only the interceptor name; the other two concern socket-error handling and task abortion, so those mutation paths remain unproven (`gate-logs/C5-mutants.log:15`, `gate-logs/C5-mutants.log:20`). |
| T4-batch-review | PASS as reported by the captured summary. | The log reports zero blocking findings; individual review narratives are not bundled (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A, deferred by design. | Publish must rerun the substantive audit after artifacts exist (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS from captured compilation. | The actual metadata and server feature builds complete; this is compile evidence, not a live TiKV test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). |

Prior art was checked by every affected path against merged main history and by intersecting all 19 closed-unmerged PRs’ complete file lists with those paths. The relevant merged predecessors are [#859](https://github.com/getwyrd/wyrd/pull/859) and [#865](https://github.com/getwyrd/wyrd/pull/865). No closed-unmerged PR changes the validator files or architecture paragraph; the shared-manifest matches are dependency updates. Results are retained in `pdca-reviewer-853-evidence/prior-art-history.json` and `pdca-reviewer-853-evidence/closed-unmerged-file-audit.json`. This corrects the brief’s obsolete `src/client.rs` path without treating the rename as a defect.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #853, round 5 (trailer section dropped at the connector)

I re-ran the suite on the patched tree: 28/28 green. Then I attacked this round's change
(`crates/validate/src/s3/wire.rs:321-327`, where the trailer section is dropped and the body is
polled on to its end) and the claims around it. I built everything in a scratch copy of
`$PDCA_TARGET` with `cargo test --offline`, toolchain 1.96.0.

## This round's fix: attempted to refute, could not

- **The new tests do guard this round's fix.** I tried two wrong versions of
  `crates/validate/src/s3/wire.rs:325`. (a) Forward the section at once (the round-3 code).
  This fails 3 tests: both checksum tests panic at `aws-smithy-checksums-0.65.0
  validate.rs:57`, and three `chunked-trailer-then-*` GET cases are accepted as 3000-byte
  objects. (b) Hold the section until after the connection check, then hand it over (the
  round-4 code). This fails 2 tests, both with the same validator panic:
  `a_chunked_get_with_a_trailer_section_is_checked_against_its_checksum_header`
  (`crates/validate/tests/s3_client_nonconforming_responses.rs:1968`) and
  `an_error_body_with_a_trailer_section_and_a_checksum_header_is_the_error_it_says` (`:1378`).
  Both tests go through the public `S3Client` against the scripted loopback endpoint, so they
  test the real production code, not a copy.
- **The `continue` at `wire.rs:325` cannot hang or panic.** After the trailers frame, hyper's
  `Incoming` polls a `futures_channel` oneshot that has already been used. That returns
  `Err(Canceled)`, which becomes `Poll::Ready(None)` (`hyper-1.10.1 body/incoming.rs:229-233`).
  The unit test at `wire.rs:437-454` shows the body ends cleanly.
- **A checksum header plus junk after the trailer section.** I probed this. The result is
  `Body(Overrun { after: 4 })` on a GET 200, both chunked and `Content-Length` framed, and
  `Unreadable` with the overrun detail on a GET, PUT and DELETE 503. All correct.
- **Dropping the section unread is bounded.** hyper caps the trailer section at 16 KiB
  (`hyper-1.10.1 proto/h1/decode.rs:25`, `:181`).
- **C5 survivors.** Replacing `wire.rs:237` (`Drop` → `()`) gives an equivalent mutant.
  Dropping `ConnectionTask` also drops `stop`, so `stopped` resolves and `drive` returns
  (`wire.rs:200`), so the task still ends. `response.rs:233` is only the interceptor's
  name. `wire.rs:220` (guard → `true`) would need a peek error other than the three
  listed, which a loopback test cannot stage on demand. I'm not raising any of these.

## Findings

- NEEDS-HUMAN [human] — **The same 503 bytes classify differently on GET and on PUT/DELETE
  when an `x-amz-checksum-*` header does not match the body.** Concrete case:
  `503 Slow Down`, a complete `SlowDown` document, and `x-amz-checksum-sha256` set to the
  digest of `"other"`. With `Content-Length` framing, a GET returns
  `Unreadable { detail: "body checksum mismatch …" }`, while PUT and DELETE return
  `Service { code: Code("SlowDown") }`. The chunked-with-trailer framing gives the same split.
  The cause: the SDK wraps a GET body in its checksum validator whatever the status
  (`aws-sdk-s3-1.152.0 http_response_checksum.rs:130-140`), and no validator runs for PUT or
  DELETE. This breaks the brief's self-test ("the same bytes must classify the same way on
  GET, PUT and DELETE") and the test file's own promise
  (`s3_client_nonconforming_responses.rs:8-9`). This round's new 503 checksum case (`:1378-1407`)
  sends only a *matching* checksum, so it cannot see the split. The inconsistency likely
  predates this patch (#852's SDK path behaves the same), but this round's surface is
  exactly this interaction. I tag it `[human]` because the fix needs a decision: check
  checksum headers on error bodies on every operation (new checksum code or dependencies),
  or ignore them on error responses. The checksum interceptor wraps the body before
  `ResponseGuard` runs (`crates/validate/src/s3/response.rs:236-277`), so the guard cannot
  just strip the header.
- NEEDS-HUMAN [human] — **The 64 KiB budget counts decoded bytes, not wire bytes, so an
  endpoint can make the client read any amount of chunk framing.** Concrete case: a 503,
  chunked, with no `Content-Length`. The 134-byte `<Error><Code>SlowDown</Code><Message>x</Message>…`
  document is sent as 1-byte chunks, and each chunk-size line carries 65,536 leading zeros
  (`000…01`, valid `1*HEXDIG`). The endpoint wrote all 8,782,633 body bytes, 134 times the
  budget, with no close from the client. The verdict was a clean
  `Service { code: Code("SlowDown"), message: Some("x") }` on GET, PUT and DELETE, after about
  4 s each. The padding size has no limit except the 15-minute operation deadline
  (`crates/validate/src/s3.rs:102`). A second form, `1` plus 64 KiB of spaces before the CRLF,
  is also accepted on all three operations. That one is not RFC 9112 §7.1.1 grammar
  (whitespace is allowed only before `;`); hyper allows it (`hyper-1.10.1
  proto/h1/decode.rs:414-429`). Memory stays bounded, so the Goal's "never buffers more"
  holds. Two things do not hold: the doc claim that such a body "is cut off at the budget
  rather than read whole" (`s3.rs:26-28`, the `05-building-block-view.md` paragraph), and the
  rubric's "budgets sized for the worst-case **encoded** representation". The test oracle
  cannot see it, because it counts only document bytes and skips chunk framing
  (`s3_client_nonconforming_responses.rs:345-347`, `:564-588`). A fix means counting raw
  socket bytes in the connector (`wire.rs`) for buffered responses. That is an
  architecture and scope call, and the human froze the rest of the patch.
- NEEDS-HUMAN [human] — **A GET answered with a non-200 2xx is accepted as the whole
  object.** Concrete cases: `206 Partial Content`, `Content-Range: bytes 0-99/3000`,
  `Content-Length: 100` returns `Ok`, a 100-byte "object". `203` with 100 bytes returns `Ok`.
  `204` with `Content-Length: 0` returns `Ok`, an empty object. S3 sends none of these to a
  plain GET, and the patch's invariant says "never … a shorter object" (`s3.rs:15-17`). The
  guard streams any `is_success()` status (`response.rs:244-251`). The only 2xx check is for
  framing on a `204` (`response.rs:108-112`). Not one of the brief's listed cases, and likely
  inherited from #852, so this is a scope question: fix it here or in a follow-up issue.
- **The gate summary overstates the red leg** (informational, not the builder's to fix). The
  `C4-verify` row in `check-gates.json` says "28 test(s) ran red". `gate-logs/C4-verify.log`
  shows 20 failed and 8 passed on the reverted base: `base64_matches_the_rfc_4648_vectors`,
  the bodiless-404, non-XML, incomplete-chunked-error, close-delimited-GET, cut-short-GET,
  other-coding-GET and larger-than-budget-GET tests. The brief allows green guards, and
  every red is an assertion failure on a build that compiled, so the PASS verdict stands.
  Only the count in the row is wrong.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No actionable findings on either lens: introduced correctness bugs or reuse, simplification, and efficiency. At `crates/validate/src/s3/wire.rs:320`, trailers are discarded while polling continues to the body’s end; the connection check at `crates/validate/src/s3/wire.rs:337` still precedes successful completion. Regression tests cover checksum-bearing error responses on all three operations (`crates/validate/tests/s3_client_nonconforming_responses.rs:1378`) and successful GETs with correct and incorrect checksums (`crates/validate/tests/s3_client_nonconforming_responses.rs:1968`).

Validation used target-source inspection and frozen gate evidence; no builds were rerun. C4-verify records 28 passing tests with the patch, and CI and batch review pass. Diff coverage was not measured because the patch did not apply to the gate’s origin/main base. The surviving mutants do not establish an actionable defect.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the tested contract for the intended validation workload — extra-byte detection sees only bytes already received at response end, and fresh connections impose the documented throughput cost (`target/crates/validate/src/s3/wire.rs:24`, `target/crates/validate/src/s3.rs:217`).
- [x] **The same 503 bytes classify differently on GET and on PUT/DELETE when an `x-amz-checksum-*` header does not match the body.** Concrete case: `503 Slow Down`, a complete `SlowDown` document, and `x-amz-checksum-sha256` set to the digest of `"other"`. With `Content-Length` framing, a GET returns `Unreadable { detail: "body checksum mismatch …" }`, while PUT and DELETE return `Service { code: Code("SlowDown") }`. The chunked-with-trailer framing gives the same split. The cause: the SDK wraps a GET body in its checksum validator whatever the status (`aws-sdk-s3-1.152.0 http_response_checksum.rs:130-140`), and no validator runs for PUT or DELETE. This breaks the brief's self-test ("the same bytes must classify the same way on GET, PUT and DELETE") and the test file's own promise (`s3_client_nonconforming_responses.rs:8-9`). This round's new 503 checksum case (`:1378-1407`) sends only a *matching* checksum, so it cannot see the split. The inconsistency likely predates this patch (#852's SDK path behaves the same), but this round's surface is exactly this interaction. I tag it `[human]` because the fix needs a decision: check checksum headers on error bodies on every operation (new checksum code or dependencies), or ignore them on error responses. The checksum interceptor wraps the body before `ResponseGuard` runs (`crates/validate/src/s3/response.rs:236-277`), so the guard cannot just strip the header.
- [x] **The 64 KiB budget counts decoded bytes, not wire bytes, so an endpoint can make the client read any amount of chunk framing.** Concrete case: a 503, chunked, with no `Content-Length`. The 134-byte `<Error><Code>SlowDown</Code><Message>x</Message>…` document is sent as 1-byte chunks, and each chunk-size line carries 65,536 leading zeros (`000…01`, valid `1*HEXDIG`). The endpoint wrote all 8,782,633 body bytes, 134 times the budget, with no close from the client. The verdict was a clean `Service { code: Code("SlowDown"), message: Some("x") }` on GET, PUT and DELETE, after about 4 s each. The padding size has no limit except the 15-minute operation deadline (`crates/validate/src/s3.rs:102`). A second form, `1` plus 64 KiB of spaces before the CRLF, is also accepted on all three operations. That one is not RFC 9112 §7.1.1 grammar (whitespace is allowed only before `;`); hyper allows it (`hyper-1.10.1 proto/h1/decode.rs:414-429`). Memory stays bounded, so the Goal's "never buffers more" holds. Two things do not hold: the doc claim that such a body "is cut off at the budget rather than read whole" (`s3.rs:26-28`, the `05-building-block-view.md` paragraph), and the rubric's "budgets sized for the worst-case **encoded** representation". The test oracle cannot see it, because it counts only document bytes and skips chunk framing (`s3_client_nonconforming_responses.rs:345-347`, `:564-588`). A fix means counting raw socket bytes in the connector (`wire.rs`) for buffered responses. That is an architecture and scope call, and the human froze the rest of the patch.
- [x] **A GET answered with a non-200 2xx is accepted as the whole object.** Concrete cases: `206 Partial Content`, `Content-Range: bytes 0-99/3000`, `Content-Length: 100` returns `Ok`, a 100-byte "object". `203` with 100 bytes returns `Ok`. `204` with `Content-Length: 0` returns `Ok`, an empty object. S3 sends none of these to a plain GET, and the patch's invariant says "never … a shorter object" (`s3.rs:15-17`). The guard streams any `is_success()` status (`response.rs:244-251`). The only 2xx check is for framing on a `204` (`response.rs:108-112`). Not one of the brief's listed cases, and likely inherited from #852, so this is a scope question: fix it here or in a follow-up issue.
- [x] **The claimed reproductions are not established for #852.** `brief.md:8-23` attributes defects to “#852's client” using #741's unpublished attempts, and `brief.md:50-57` promises an assertion-only red against #852. Neither `notes.json` nor `sources/` is supplied. The repo and `main` resolve, but `dependency-state.json:2-5` records #852 as existing and `PLANNED`; the resolved target at `36f006d` has no `crates/validate/`, consistent with `$PDCA_TARGET/Cargo.toml:9-32`. This does not disprove the defects, but it prevents verifying the root cause, public API, dev-dependencies, or prior-attempt constraints. Revise these claims to hypotheses pending reproduction on the folded #852 commit, and identify the evidence needed to establish that baseline.
- [x] **The budget oracle can pass without enforcing a streamed-byte limit.** `brief.md:40-44` requires a lazily streamed oversized error and a “remove the budget” mutation, but leaves its framing unspecified. A fixture advertising an over-budget `Content-Length` permits a header-only rejection to satisfy both checks while unknown-length bodies remain unbounded. Require an otherwise-valid, oversized `<Error>` document sent chunked without `Content-Length`, with a long `<Message>` crossing the budget. Remove the streaming limiter while retaining any header precheck for the mutation. That makes the assertion distinguish bounded reading from merely trusting the advertised size.
- [x] **Chunked completeness has no negative acceptance case.** `brief.md:45-49` checks close-delimited refusal, a correctly terminated chunked GET, and declared-length disagreement; none requires rejection of an incomplete chunked body without `Content-Length`. Yet the cited invariant explicitly requires the terminal CRLF (`$PDCA_TARGET/AGENTS.md:161-164`). Add missing-terminal-chunk and truncated-terminal-CRLF cases: body error for GET success, and unreadable on all three operations for error responses whose XML is otherwise complete. These isolate framing failure from XML failure and prevent the positive chunked case from standing in for truncation detection.
- [x] **The all-response budget promise exceeds the scoped change.** `brief.md:5-7,58-61` promises bounded reading/memory for every response, but `brief.md:40-44,80-84` specifies an error-document budget and excludes changes to well-formed responses. A total-byte cap sized for error documents would reject legitimate larger GETs; an error-only cap cannot establish the universal memory claim. The target proposal explicitly includes large valid objects (`$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:283-291`). Narrow the guarantee to buffered error bodies, or separately define bounded-memory streaming for successful GETs and require a valid object larger than the error budget to remain accepted. Otherwise Do inherits an unstated second obligation beyond the named budget test.
- [x] size backstop — this slice is behaving oversized: patch is 182 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (from #853 sign-off): a 503 with a mismatched x-amz-checksum-* header is Unreadable on GET but Service(SlowDown) on PUT/DELETE — breaks the same-on-all-three promise.
- Follow-up (from #853 sign-off): the 64 KiB budget counts decoded bytes, not wire bytes — padded chunk-size lines let a peer make the client read ~unbounded framing; doc claim "cut off at the budget" (s3.rs:26-28, 05-building-block-view.md) is untrue as written.
- Follow-up (from #853 sign-off): a GET answered with a non-200 2xx (206/203/204) is accepted as the whole object (response.rs:244-251).
