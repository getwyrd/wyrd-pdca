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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (14 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 76 mutants tested in 3m: 17 missed, 15 caught, 44 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.75s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #853: classify nonconforming S3 responses consistently across GET, PUT and DELETE, cap buffered bodies, and preserve complete streamed GETs.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether the length guarantee includes surplus wire bytes beyond `Content-Length` — a declared 3-byte GET carrying `abcdef` returns `abc` successfully, while the SDK exposes only the declared body; this determines whether lower-level transport work belongs in scope (`brief.md:79`, `target/crates/validate/src/s3/body.rs:223`, `review-probes.log:5`). |
| C2 Reproduction (red pre-fix) | PASS | The unchanged regression test compiles against the pre-fix snapshot and reproduces eight failing tests, with six existing guards passing (`review-red-green.log:168`, `target/crates/validate/tests/s3_client_nonconforming_responses.rs:47`). |
| C3 Change | PASS | The change stays within the authorized validator and architecture surfaces, uses existing dependencies, and documents the changed response contract; upload behavior remains covered by the settled #854 deferral (`target/crates/validate/Cargo.toml:35`, `target/crates/validate/src/s3.rs:165`, `target/docs/design/architecture/05-building-block-view.md:255`). |
| C4 Verification (red→green) | PASS | Independent production stash/restore reproduces 8 failed + 6 passed → 14 passed; the frozen full CI also passes, while diff coverage remains unmeasured because its remote base lacks the stacked patch prerequisites (`review-red-green.log:168`, `review-red-green.log:211`, `gate-logs/C4-ci.log:3970`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Fix the three reproduced classification holes before claiming the invariant — malformed coding, bodyless-status framing, and nested scalar XML still yield trusted results (F1–F3; `target/crates/validate/src/s3/response.rs:85`, `target/crates/validate/src/s3/response.rs:198`, `target/crates/validate/src/s3/response.rs:327`). |
| T1 Structure | PASS | Response validation remains within the client seam and introduces no backend dependency or shared mutable global; the independent dependency guard passes (`target/crates/validate/src/s3/response.rs:158`, `review-blackbox-guard.log:3`). |
| T2 Shape | PASS | The optional GET length accommodates chunked bodies, callers are updated, and the living architecture paragraph describes the budget and framing contract; format, spelling and docs lint reruns pass (`target/crates/validate/src/s3/body.rs:194`, `target/crates/validate/tests/s3_client_roundtrip.rs:1048`, `target/docs/design/architecture/05-building-block-view.md:255`). |
| T3 Runtime | FAIL | Malformed GET responses can terminate as successful objects, so bounded memory alone does not make the validator's runtime verdict trustworthy (F1–F2; `target/crates/validate/src/s3.rs:198`, `review-probes.log:12`, `review-probes.log:34`). |
| T4 Contribution | FAIL | The required deep review still has three distinct reproduced defects without a recorded resolution; the path-based prior-art audit is complete, and contribution-text auditing is deferred to publish (`target/AGENTS.md:206`, `gate-logs/T4-batch-review.log:10`, `review-prior-art.log:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The core budget evidence exercises the real client and catches removal of only the streaming limit on all five buffered operation/status combinations, independently of header-length checks (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1053`, `review-streaming-mutation.log:8`, `review-streaming-mutation.log:21`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the documented 64 KiB limit and response taxonomy meet the validator's operational purpose after F1–F3 and the C1 scope decision are resolved — loopback protocol evidence establishes behavior but does not make that product decision (`target/crates/validate/src/s3/response.rs:34`, `target/docs/design/architecture/05-building-block-view.md:255`). |

Three implementation defects remain despite a genuine red→green result. The independent probes confirm the frozen review's three distinct concerns; its two 204 findings describe the same defect.

1. **F1 — FAIL: non-ASCII transfer coding permits a truncated GET to succeed.** At `target/crates/validate/src/s3/response.rs:85`, Unicode-aware `trim()` classifies `Transfer-Encoding: chunked\u{00A0}` as chunked. A real GET with that header and bytes `3\r\nabc\r\n`, without a terminal chunk, succeeds with all eight raw bytes as the object (`review-probes.log:34`). Header interpretation therefore disagrees with the transport. Restrict recognition to the HTTP coding grammar and add this exact wire regression.
2. **F2 — FAIL: a 204 response bypasses chunk completion.** At `target/crates/validate/src/s3/response.rs:198`, every successful status can record streaming framing; `target/crates/validate/src/s3.rs:198` then trusts chunked framing without a declared length. A GET answered `204 No Content`, `Transfer-Encoding: chunked`, and no body or terminal chunk succeeds as an empty object (`review-probes.log:12`). Validate status/framing compatibility before trusting the transport's end-of-body signal.
3. **F3 — FAIL: structured XML is normalized into a scalar error code/message.** At `target/crates/validate/src/s3/response.rs:327`, descendant text is concatenated without rejecting child elements. `<Error><Code>Slow<Unexpected/>Down</Code></Error>` becomes a clean `Code("SlowDown")` on GET, PUT and DELETE; a nested message becomes `"tryagain"` on all three operations (`review-probes.log:18`). Reject nested elements in these scalar fields and retain their exact values only when structurally valid.

The surplus-length experiment requires a scope decision, recorded in C1. `Content-Length: 3` followed by `abcdef` returns `abc`; the existing GET length test exercises a declaration larger than the transmitted body and conflicting chunked/length headers, but no plain length declaration smaller than the wire payload (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1256`). Because the SDK stops at the declared message boundary, this experiment does not establish that its delivered body exceeds its declared length. Decide whether the brief promises detection of those extra transport octets or only consistency of the body the transport exposes. Reproduce with `./review-probes reviewer_observes_declared_length_shorter_than_wire --nocapture`; the runnable source is `review-probes.rs:1329`.

Verification evidence supports the targeted fix, with these explicit limits:

- **Independent reruns:** `cargo test --offline --locked -p wyrd-validate --test s3_client_nonconforming_responses` ran against the disposable target, then with production sources and the manifest stashed while retaining the test, then after restoration. The red failures are assertions, not build errors (`review-red-green.log:1`, `review-red-green.log:168`, `review-red-green.log:211`). `cargo fmt --all -- --check`, `typos crates/validate docs/design/architecture/05-building-block-view.md`, docs lint, `git diff --check`, and `cargo xtask blackbox-guard` also pass. No full workspace CI or TiKV rerun is claimed.
- **Streaming-limit mutation:** a small source copy under this review directory was compiled with the pinned Rust 1.96.0 toolchain and the actual built dependencies. Disabling only `response.rs:262` while retaining the declared-length check makes both budget tests fail; the endpoint writes 69,304,320 bytes for success bodies and 69,304,453 for error bodies (`review-streaming-mutation.log:3`, `review-streaming-mutation.log:44`). No shim, absent compiler, or external service substitutes for the production client.
- **Frozen gates:** full workspace CI, scanners and simulation/conformance tests report success (`gate-logs/C4-ci.log:3374`, `gate-logs/C4-ci.log:3970`); TiKV/server feature compilation reports success (`gate-logs/host-tikv.log:209`). The mutation scanner reports 17 missed, 15 caught and 44 unviable mutations (`gate-logs/C5-mutants.log:30`). Survivors were inspected as coverage evidence; interceptor-name and transport-hint mutations are not independent defect findings. The three substantive batch-review concerns were independently reproduced above.
- **Coverage caveat:** `C4-diff-cov` did not measure coverage because the patch could not apply on `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target contains #852's client, compiles both legs, and passes `git apply --reverse --check ../patch.diff` (`review-target-integrity.log:1`). This is a remote-base/order caveat, not a C4 compile/applicability defect in the patch.
- **T4-contribution — N/A:** `pr-description.md` is absent by design; the substantive contribution-artifact audit runs at publish (`gate-logs/T4-contribution.log:10`). Nothing needs human clearance for this deferred row.
- **Prior art:** all eight affected paths were queried in merged/default-branch history, and all 356 closed PR file lists were compared with those paths and the brief's older `client.rs` path. No validator-file match was found; only the architecture document has merged history (`review-prior-art.log:1`). Detailed results are in `prior-art-by-path.json` and `prior-art-closed-by-path.json`.

All project-source citations above resolve in the supplied `$PDCA_TARGET` (`target/`). Its patch is restored, with no production edits from this review. The capability-probe smell test does not trigger: these checks validate untrusted responses rather than mask a missing capability or load-time side effect. The #854 upload deferral is settled, and no additional `INTEGRATION.md` was present in the supplied target.

### Advisory — adversary

# Adversarial review — #853 validate-s3-client-nonconforming-responses

**Method.** I re-read the red→green log and the C5 survivors. Then I ran 9 probe inputs and one budget mutation against the patched tree at `$PDCA_TARGET`, using a scratch copy, the production `S3Client`, and the patch's own scripted-endpoint helpers. 6 of the 9 probes broke the fix.

**Evidence: holds.** The C4-verify red leg compiles against #852 and fails by assertion: 8 tests fail and 6 pass as guards. It drives the production client through `resolve_config` and `S3Client::with_deadlines` (`crates/validate/tests/s3_client_nonconforming_responses.rs:479-516`). It is not a parallel copy of the client. The gate's summary "14 test(s) ran red" is wrong (only 8 of 14 failed). That is a harness counting slip, not a patch defect.

## Findings

- NEEDS-HUMAN — [human] **A body longer than its declared `Content-Length` is never detected, on any operation.** `crates/validate/tests/s3_client_nonconforming_responses.rs:921-935` (`length-under-sent`) goes green only because the declared length ends inside `<Message>`, so the XML check catches it. No length check is involved. Move the declared end to the `</Error>` close tag and send more bytes after it (`Content-Length: doc.len()`, body `doc + "<html>proxy junk…</html>"`). Then GET, PUT and DELETE each report a clean `Service { 503, Code("SlowDown"), "Please reduce your request rate." }` (probe run). A GET 200 with `Content-Length: 2000` and 3000 bytes sent is accepted as a 2000-byte object (probe run). The brief makes both cases binding ("an error body whose received length differs from its declared `Content-Length` … → unreadable"; "a GET whose received length disagrees with its declared length → body error"). The patch's own comment admits the gap: the length checks "cannot fire on any framing a peer can send" (`crates/validate/src/s3/body.rs:223-229`). The GET test leaves this direction out entirely (`:1255-1290`). hyper stops reading at the declared length and never reports the extra bytes. Catching them needs a connector-level check, so this is a scope call: build that check, or narrow the brief and the test names to "declared more than sent".

- NEEDS-HUMAN [impl] — **The test does not pin the 64 KiB budget; a budget 256 times larger passes all 14 tests.** I set `crates/validate/src/s3/response.rs:45` to `16 * 1024 * 1024` and ran the suite: 14/14 green. The endpoint wrote about 16.9 MB against a bound of 34,652,160. The bound (`s3_client_nonconforming_responses.rs:113-120`) is set almost entirely by `tcp_rmem[2]`, which is 32 MiB on this host. With the real budget the endpoint wrote only 135–229 KB. So the bytes-written oracle proves "some streaming limit under about 33 MiB", not 64 KiB. C5 agrees: mutants at `response.rs:45` (`*`→`+`, a 1088-byte budget that would refuse a legitimate ~20 KiB `SignatureDoesNotMatch`), `:215` and `:262` (`>`→`>=`) all survived. Fix: add an exact boundary pair. A chunked `<Error>` of exactly `BUDGET` bytes must be a clean `Service`, and one of `BUDGET + 1` bytes must be `Unreadable`. Optionally add a ~20 KiB legitimate error document, which the probe showed is read correctly today. This adds no new option or dev-dependency, and the `+1` case is red on #852.

- NEEDS-HUMAN [impl] — **Three of the T4 blockers reproduce on the production path, so they are not reviewer noise.** (a) A GET 200 with `Transfer-Encoding: chunked\u{00A0}` that sends 1000 of 3000 bytes and closes is **accepted as a 1000-byte object**. The cause is the Unicode `trim()` at `crates/validate/src/s3/response.rs:85`. hyper's `HeaderValue::to_str()` refuses the value (`hyper-1.10.1/src/headers.rs:130-139`) and reads the body to EOF (`proto/h1/role.rs:1296-1298`). The surviving mutant at `response.rs:85` (guard → `true`) shows that no test sends any non-`chunked` coding on a GET. `Transfer-Encoding: gzip` is refused correctly today, but nothing checks it. (b) A GET `204` with `Transfer-Encoding: chunked` and `5\r\nhello\r\n` with no terminal chunk is **accepted as a 0-byte object**. hyper forces a zero-length body for 204 (`role.rs:1268`), but `response.rs:198` records `Chunked` for any 2xx. (c) `<Code>Slow<b/>Down</Code>` becomes a clean `Code("SlowDown")` on all three operations (`response.rs:327-331`). Each needs a production fix plus a test case.

- NEEDS-HUMAN [impl] — **The "nothing after `</Error>` but whitespace or comments" rule is enforced but untested.** The mutant deleting `!` at `crates/validate/src/s3/response.rs:312` survived. That mutant would accept `<Error>…</Error><?pi x?>` as a clean error. The current code refuses it correctly (probe run). The `:345` mutant also survived, and `is_blank` (`:344-346`) looks unreachable because roxmltree keeps no whitespace text after the root. Add a case with a processing instruction after the root element.

- NEEDS-HUMAN — [human] **An empty `<Code></Code>` is reported as a clean S3 error with code `""`.** On all three operations the result is `Service { 503, Code(""), Some("m") }` (probe run; `crates/validate/src/s3/response.rs:327-339`). The brief says "never a code the body did not carry", and an empty string is not an S3 code. Someone needs to decide whether an empty or whitespace-only `<Code>` should be `MissingInXml`, `Unreadable`, or stay as it is.

- `crates/validate/src/s3/body.rs:223-229` claims the length checks "cannot fire on any framing a peer can send". A GET `204` with `Content-Length: 5` gets `Framing::Length(5)`, hyper delivers 0 bytes, and the end-of-body check does fire. The behaviour is right; the comment is not. Fix it together with the 204 work above.

**Tried and could not refute.** HTML and plain-text 500s; chunked framing cut before the terminal chunk or the final CRLF; a 69 MB generated chunked error or PUT/DELETE success body (the endpoint writes only 135–229 KB before the client closes); a close-delimited GET; `Transfer-Encoding: gzip` on a GET; a GET error with a declared length over the budget (refused from the header before any read); and the claim that the SDK's success diagnostic is kept on PUT and DELETE.

C4-diff-cov's "fail" means "not measured": the bundle does not apply on `origin/main` without #852. It is not evidence for or against the fix.

### Advisory — code-review

Advisory findings grounded in the target source and frozen gate evidence; no gates were rerun.

- NEEDS-HUMAN [impl] — **Reject non-HTTP whitespace in transfer coding.** `crates/validate/src/s3/response.rs:85` uses Unicode-aware `trim()`, so `Transfer-Encoding: chunked\u{00A0}` becomes `Framing::Chunked`. Hyper treats this value as close-delimited, allowing a GET cut off without a terminal chunk to finish successfully. This discrepancy is also recorded in `gate-logs/T4-batch-review.log`. Validate the original bytes using only HTTP space/tab trimming and add a malformed-coding GET regression.

- NEEDS-HUMAN [impl] — **Account for bodyless statuses before accepting chunked GETs.** `crates/validate/src/s3/response.rs:198` admits every successful status into the streaming path. For `204 No Content` with `Transfer-Encoding: chunked`, hyper suppresses the body, while `crates/validate/src/s3.rs:198` accepts the framing with no length check. Consequently, even a response with no terminal chunk becomes a successful empty object. The frozen batch review confirms this case. Reject forbidden framing for bodyless statuses and add this GET regression.

- NEEDS-HUMAN [impl] — **Reject nested elements in scalar error fields.** `crates/validate/src/s3/response.rs:328` concatenates all descendant text, turning `<Error><Code>Slow<Unexpected/>Down</Code></Error>` into a clean `Code("SlowDown")`. The same normalization affects `<Message>`. XML well-formedness does not establish that these fields contain scalar text. Validate their children before collecting text and exercise nested elements on GET, PUT, and DELETE; the frozen batch review identifies the same defect.

- NEEDS-HUMAN [impl] — **Test acceptance at the buffered-body limit.** The budget cases at `crates/validate/tests/s3_client_nonconforming_responses.rs:1055` only demand rejection of oversized bodies. Frozen `C5-mutants.log` shows that changing `64 * 1024` to `64 + 1024` at `crates/validate/src/s3/response.rs:45` survives, as does changing the streaming comparison from `>` to `>=`. Thus the suite accepts a client that rejects valid error documents above 1,088 bytes or exactly at the documented limit. Add valid error-document cases just below and at 64 KiB, with declared-length and chunked framing on all three operations.

No separate reuse, simplification, or efficiency finding.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether the length guarantee includes surplus wire bytes beyond `Content-Length` — a declared 3-byte GET carrying `abcdef` returns `abc` successfully, while the SDK exposes only the declared body; this determines whether lower-level transport work belongs in scope (`brief.md:79`, `target/crates/validate/src/s3/body.rs:223`, `review-probes.log:5`).
- [ ] C5 Causal adequacy — Fix the three reproduced classification holes before claiming the invariant — malformed coding, bodyless-status framing, and nested scalar XML still yield trusted results (F1–F3; `target/crates/validate/src/s3/response.rs:85`, `target/crates/validate/src/s3/response.rs:198`, `target/crates/validate/src/s3/response.rs:327`).
- [ ] Validation — fitness-to-purpose — Decide whether the documented 64 KiB limit and response taxonomy meet the validator's operational purpose after F1–F3 and the C1 scope decision are resolved — loopback protocol evidence establishes behavior but does not make that product decision (`target/crates/validate/src/s3/response.rs:34`, `target/docs/design/architecture/05-building-block-view.md:255`).
- [ ] **A body longer than its declared `Content-Length` is never detected, on any operation.** `crates/validate/tests/s3_client_nonconforming_responses.rs:921-935` (`length-under-sent`) goes green only because the declared length ends inside `<Message>`, so the XML check catches it. No length check is involved. Move the declared end to the `</Error>` close tag and send more bytes after it (`Content-Length: doc.len()`, body `doc + "<html>proxy junk…</html>"`). Then GET, PUT and DELETE each report a clean `Service { 503, Code("SlowDown"), "Please reduce your request rate." }` (probe run). A GET 200 with `Content-Length: 2000` and 3000 bytes sent is accepted as a 2000-byte object (probe run). The brief makes both cases binding ("an error body whose received length differs from its declared `Content-Length` … → unreadable"; "a GET whose received length disagrees with its declared length → body error"). The patch's own comment admits the gap: the length checks "cannot fire on any framing a peer can send" (`crates/validate/src/s3/body.rs:223-229`). The GET test leaves this direction out entirely (`:1255-1290`). hyper stops reading at the declared length and never reports the extra bytes. Catching them needs a connector-level check, so this is a scope call: build that check, or narrow the brief and the test names to "declared more than sent".
- [ ] **The test does not pin the 64 KiB budget; a budget 256 times larger passes all 14 tests.** I set `crates/validate/src/s3/response.rs:45` to `16 * 1024 * 1024` and ran the suite: 14/14 green. The endpoint wrote about 16.9 MB against a bound of 34,652,160. The bound (`s3_client_nonconforming_responses.rs:113-120`) is set almost entirely by `tcp_rmem[2]`, which is 32 MiB on this host. With the real budget the endpoint wrote only 135–229 KB. So the bytes-written oracle proves "some streaming limit under about 33 MiB", not 64 KiB. C5 agrees: mutants at `response.rs:45` (`*`→`+`, a 1088-byte budget that would refuse a legitimate ~20 KiB `SignatureDoesNotMatch`), `:215` and `:262` (`>`→`>=`) all survived. Fix: add an exact boundary pair. A chunked `<Error>` of exactly `BUDGET` bytes must be a clean `Service`, and one of `BUDGET + 1` bytes must be `Unreadable`. Optionally add a ~20 KiB legitimate error document, which the probe showed is read correctly today. This adds no new option or dev-dependency, and the `+1` case is red on #852.
- [ ] **Three of the T4 blockers reproduce on the production path, so they are not reviewer noise.** (a) A GET 200 with `Transfer-Encoding: chunked\u{00A0}` that sends 1000 of 3000 bytes and closes is **accepted as a 1000-byte object**. The cause is the Unicode `trim()` at `crates/validate/src/s3/response.rs:85`. hyper's `HeaderValue::to_str()` refuses the value (`hyper-1.10.1/src/headers.rs:130-139`) and reads the body to EOF (`proto/h1/role.rs:1296-1298`). The surviving mutant at `response.rs:85` (guard → `true`) shows that no test sends any non-`chunked` coding on a GET. `Transfer-Encoding: gzip` is refused correctly today, but nothing checks it. (b) A GET `204` with `Transfer-Encoding: chunked` and `5\r\nhello\r\n` with no terminal chunk is **accepted as a 0-byte object**. hyper forces a zero-length body for 204 (`role.rs:1268`), but `response.rs:198` records `Chunked` for any 2xx. (c) `<Code>Slow<b/>Down</Code>` becomes a clean `Code("SlowDown")` on all three operations (`response.rs:327-331`). Each needs a production fix plus a test case.
- [ ] **The "nothing after `</Error>` but whitespace or comments" rule is enforced but untested.** The mutant deleting `!` at `crates/validate/src/s3/response.rs:312` survived. That mutant would accept `<Error>…</Error><?pi x?>` as a clean error. The current code refuses it correctly (probe run). The `:345` mutant also survived, and `is_blank` (`:344-346`) looks unreachable because roxmltree keeps no whitespace text after the root. Add a case with a processing instruction after the root element.
- [ ] **An empty `<Code></Code>` is reported as a clean S3 error with code `""`.** On all three operations the result is `Service { 503, Code(""), Some("m") }` (probe run; `crates/validate/src/s3/response.rs:327-339`). The brief says "never a code the body did not carry", and an empty string is not an S3 code. Someone needs to decide whether an empty or whitespace-only `<Code>` should be `MissingInXml`, `Unreadable`, or stay as it is.
- [ ] **Reject non-HTTP whitespace in transfer coding.** `crates/validate/src/s3/response.rs:85` uses Unicode-aware `trim()`, so `Transfer-Encoding: chunked\u{00A0}` becomes `Framing::Chunked`. Hyper treats this value as close-delimited, allowing a GET cut off without a terminal chunk to finish successfully. This discrepancy is also recorded in `gate-logs/T4-batch-review.log`. Validate the original bytes using only HTTP space/tab trimming and add a malformed-coding GET regression.
- [ ] **Account for bodyless statuses before accepting chunked GETs.** `crates/validate/src/s3/response.rs:198` admits every successful status into the streaming path. For `204 No Content` with `Transfer-Encoding: chunked`, hyper suppresses the body, while `crates/validate/src/s3.rs:198` accepts the framing with no length check. Consequently, even a response with no terminal chunk becomes a successful empty object. The frozen batch review confirms this case. Reject forbidden framing for bodyless statuses and add this GET regression.
- [ ] **Reject nested elements in scalar error fields.** `crates/validate/src/s3/response.rs:328` concatenates all descendant text, turning `<Error><Code>Slow<Unexpected/>Down</Code></Error>` into a clean `Code("SlowDown")`. The same normalization affects `<Message>`. XML well-formedness does not establish that these fields contain scalar text. Validate their children before collecting text and exercise nested elements on GET, PUT, and DELETE; the frozen batch review identifies the same defect.
- [ ] **Test acceptance at the buffered-body limit.** The budget cases at `crates/validate/tests/s3_client_nonconforming_responses.rs:1055` only demand rejection of oversized bodies. Frozen `C5-mutants.log` shows that changing `64 * 1024` to `64 + 1024` at `crates/validate/src/s3/response.rs:45` survives, as does changing the streaming comparison from `>` to `>=`. Thus the suite accepts a client that rejects valid error documents above 1,088 bytes or exactly at the documented limit. Add valid error-document cases just below and at 64 KiB, with declared-length and chunked framing on all three operations.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- [ ] **The claimed reproductions are not established for #852.** `brief.md:8-23` attributes defects to “#852's client” using #741's unpublished attempts, and `brief.md:50-57` promises an assertion-only red against #852. Neither `notes.json` nor `sources/` is supplied. The repo and `main` resolve, but `dependency-state.json:2-5` records #852 as existing and `PLANNED`; the resolved target at `36f006d` has no `crates/validate/`, consistent with `$PDCA_TARGET/Cargo.toml:9-32`. This does not disprove the defects, but it prevents verifying the root cause, public API, dev-dependencies, or prior-attempt constraints. Revise these claims to hypotheses pending reproduction on the folded #852 commit, and identify the evidence needed to establish that baseline.
- [ ] **The budget oracle can pass without enforcing a streamed-byte limit.** `brief.md:40-44` requires a lazily streamed oversized error and a “remove the budget” mutation, but leaves its framing unspecified. A fixture advertising an over-budget `Content-Length` permits a header-only rejection to satisfy both checks while unknown-length bodies remain unbounded. Require an otherwise-valid, oversized `<Error>` document sent chunked without `Content-Length`, with a long `<Message>` crossing the budget. Remove the streaming limiter while retaining any header precheck for the mutation. That makes the assertion distinguish bounded reading from merely trusting the advertised size.
- [ ] **Chunked completeness has no negative acceptance case.** `brief.md:45-49` checks close-delimited refusal, a correctly terminated chunked GET, and declared-length disagreement; none requires rejection of an incomplete chunked body without `Content-Length`. Yet the cited invariant explicitly requires the terminal CRLF (`$PDCA_TARGET/AGENTS.md:161-164`). Add missing-terminal-chunk and truncated-terminal-CRLF cases: body error for GET success, and unreadable on all three operations for error responses whose XML is otherwise complete. These isolate framing failure from XML failure and prevent the positive chunked case from standing in for truncation detection.
- [ ] **The all-response budget promise exceeds the scoped change.** `brief.md:5-7,58-61` promises bounded reading/memory for every response, but `brief.md:40-44,80-84` specifies an error-document budget and excludes changes to well-formed responses. A total-byte cap sized for error documents would reject legitimate larger GETs; an error-only cap cannot establish the universal memory claim. The target proposal explicitly includes large valid objects (`$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:283-291`). Narrow the guarantee to buffered error bodies, or separately define bounded-memory streaming for successful GETs and require a valid object larger than the error budget to remain accepted. Otherwise Do inherits an unstated second obligation beyond the named budget test.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome:
- Iteration delta (if iterating):
- By / date:

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
