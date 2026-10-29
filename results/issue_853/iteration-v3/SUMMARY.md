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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (23 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 150 mutants tested in 14m: 4 missed, 88 caught, 51 unviable, 7 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.06s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

One trailer-overrun defect remains in #853's task to reject malformed and over-budget S3 responses while preserving valid streamed GETs.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Explicit response classifications, budget boundaries, and valid large-GET controls make the invariant testable; the human also authorized connection-level overrun detection (`brief.md:33`, `brief.md:183`). |
| C2 Reproduction (red pre-fix) | PASS | The unchanged regression file compiled against the stashed production baseline and produced 16 failing tests and 7 passing guards, establishing real defects rather than a compile failure (`review-red.log:437`). |
| C3 Change | PASS | The connector and dependency changes support the human-authorized connection inspection; the supplied patch matches the disposable target and was restored after reproduction (`brief.md:183`, `target/crates/validate/src/s3/wire.rs:16`, `review-integrity.log:4`). |
| C4 Verification (red→green) | FAIL | The supplied suite genuinely goes from 16 failures to 23 passes, but an independently executed trailer-overrun regression still accepts an invalid GET response; F1 is a verified patch defect (`review-red.log:437`, `review-ci.log:3000`, `review-trailers.log:272`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Close the trailer escape from end-of-response validation—a peer can append already-arrived junk to a chunked GET and still receive a clean result; add the missing regression while preserving valid trailers (F1; `target/crates/validate/src/s3/wire.rs:306`). |
| T1 Structure | PASS | The client retains its blackbox dependency boundary, and parsing, transport ownership, and public error classification have distinct homes; the dependency guard and compilation passed (`review-ci.log:19`, `target/crates/validate/src/s3.rs:39`). |
| T2 Shape | PASS | Typed framing/overrun errors and the updated living architecture describe the changed public behavior; the human's size waiver remains settled (`target/crates/validate/src/s3/error.rs:125`, `target/docs/design/architecture/05-building-block-view.md:297`, `brief.md:181`). |
| T3 Runtime | PASS | Exact 64 KiB boundary tests and larger valid GETs passed; external waits retain deadlines and connection tasks abort on drop, with F1's premature completion assessed separately above (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1566`, `target/crates/validate/src/s3/body.rs:464`, `target/crates/validate/src/s3/wire.rs:230`). |
| T4 Contribution | FAIL | The standing completion rule is not satisfied while the batch review's confirmed F1 remains unresolved; the contribution-artifact audit itself is N/A until publish (`target/AGENTS.md:206`, `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The approved scope/size decisions are respected, path-based merged and closed-work checks completed, and no capability probe masks a load-time cause; F1 is an implementation issue rather than a contested root cause (`brief.md:181`, `review-prior-art.log:1`, `target/crates/validate/src/s3/response.rs:369`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether inspecting only excess bytes already received at body completion and opening a fresh connection for every request fit the intended validator workloads—late-arriving bytes and connection churn limit what a clean result establishes (`target/crates/validate/src/s3/wire.rs:22`, `target/crates/validate/src/s3.rs:215`). |

**F1 — A successful chunked GET can bypass overrun detection through trailers.** This independently confirms the frozen batch finding, narrowed to the streamed GET path. A scripted peer sends a 3,000-byte object followed by `0\r\nX-Test: value\r\n\r\njunk`, in the same response write. The production client returns the complete object and clean end-of-stream instead of a body error (`review-trailers.log:272`). The extra bytes are sent with the response, so this is not the documented limitation concerning bytes arriving later.

`WholeResponse::poll_frame` forwards every frame immediately, including trailers, at `target/crates/validate/src/s3/wire.rs:306`; it asks the connection about excess bytes only when a later poll returns no frame, at `target/crates/validate/src/s3/wire.rs:310`. The streamed data consumer treats trailers as the end of data and reaches the clean-completion arm at `target/crates/validate/src/s3/body.rs:505` without that connection check. The existing trailing-junk test ends with an empty trailer section and therefore misses this route (`target/crates/validate/tests/s3_client_nonconforming_responses.rs:1923`). Ensure connection validation completes before exposing any terminal result to that consumer, including a trailer frame, and cover both valid trailers and trailers followed by junk.

The probe exercised twelve combinations. All six clean-trailer controls passed. Trailers followed by junk were correctly rejected for 503 responses on GET/PUT/DELETE and for 200 responses on PUT/DELETE; only the successful GET was incorrectly accepted (`review-trailers.log:263`). The complete runnable probe is retained as `review-trailer-probe.rs`; it copies the supplied test helpers and appends one reviewer test. To reproduce inside a disposable target:

```sh
cp review-trailer-probe.rs "$PDCA_TARGET/crates/validate/tests/reviewer_853_trailers.rs"
(cd "$PDCA_TARGET" && cargo test --offline --locked -p wyrd-validate \
  --test reviewer_853_trailers reviewer_trailers_must_not_bypass_overrun -- --nocapture)
```

The probe was moved out of the target after execution. No production fix was made.

**The supplied regression evidence is genuine, but incomplete for the claimed invariant.** The disposable target identifies its pre-fix base as `61e53418bb3e73674eef9352733f0e985796d582`. I stashed only production files and manifests, retaining the new test; it compiled and failed 16 of 23 tests. After `git stash pop`, the CI rerun passed all 23 again (`review-integrity.log:2`, `review-red.log:437`, `review-red.log:462`, `review-ci.log:3000`). This agrees with the frozen verification log, which shows 7 passing and 16 failing tests on its red leg, not 23 failures (`gate-logs/C4-verify.log:426`). Reverse-application checking and `git diff --check` passed on the restored patch (`review-integrity.log:4`).

Gate adjudication:

- **C4-ci:** The frozen run reports all checks passed (`gate-logs/C4-ci.log:4367`). I reran `cargo xtask ci`: formatting, clippy, builds, workspace tests, documentation, conformance, guards, and DST passed. The aggregate command exited nonzero solely because its two advisory checks could not lock the read-only home-directory database (`review-ci.log:3600`). Both checks subsequently passed against a writable database with configurations differing only in `advisories.db-path` (`review-deny.log:8`, `review-deny.log:11`, `review-integrity.log:21`). These were host restrictions, not patch defects. The exact `cargo machete` command also passed after reviewer-generated test fixtures were moved outside the source tree (`review-machete.log:2`).
- **C4-verify:** Independently reproduced, as detailed above; its green does not cover F1 (`gate-logs/C4-verify.log:14`, `review-trailers.log:272`).
- **C4-diff-cov:** Coverage was not measured because the patch did not apply to that gate's `origin/main` base (`gate-logs/C4-diff-cov.log:10`). The supplied disposable target does match the patch and compiles. This is a coverage/base-state caveat, not a compile or patch-applicability finding against this target.
- **C5-mutants:** Frozen evidence records 150 mutants: 88 caught, 4 missed, 51 unviable, and 7 timeouts (`gate-logs/C5-mutants.log:24`). Two misses change only the interceptor's diagnostic name; the others broaden socket-error handling and remove explicit task abortion (`gate-logs/C5-mutants.log:15`, `gate-logs/C5-mutants.log:22`). The log alone does not establish a second reachable defect. The instance-scoped mutation wrapper was not rerun.
- **T4-batch-review:** Its one finding is confirmed by F1's independent execution (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution:** **N/A**: the artifacts are absent by design at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is requested for that deferral.
- **host-tikv:** The frozen log shows both requested feature compilations completed successfully; this is log-backed compilation evidence, not a claim to have exercised a live TiKV service (`gate-logs/host-tikv.log:209`).

**Prior art was checked by the actual affected paths.** I queried `main` history for all eleven changed paths and complete changed-file lists for all 399 closed/merged PRs. The existing S3 source files match merged PRs #859, #860, and #865, with no closed-unmerged matches. The new response module, connector module, and nonconforming-response test have no merged history or closed-PR matches (`review-prior-art.log:5`, `review-prior-art.log:8`). This updates the brief's older `src/client.rs` lookup. The full query records are retained in `review-prior-art-history.json` and `review-prior-art-closed.json`.

The brief's Rust and loopback prerequisites were exercised with the real compiler and production client; no tool alias or replacement client underlies the reproduction. Human validation remains necessary for the documented observation boundary and connection cost, after F1 is addressed. No additional human-only integration document was present among the supplied target files.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue #853 (validate-s3-client-nonconforming-responses)

**Verdict: refuted on one path.** A GET object followed by bytes after its end is still accepted when the chunked body carries a trailer section. I reproduced this on the production client. The T4 gating blocker is real, not reviewer noise. Everything else I attacked held up.

## Findings

- NEEDS-HUMAN [impl] — **A chunked GET with a trailer section skips the check for bytes after the end, so it is accepted as a whole object.** `crates/validate/src/s3/wire.rs:305-306` passes hyper's trailers frame straight through. The check for what came after the body only runs on the *next* poll (`wire.rs:308-322`), and the GET path never makes that poll. `ObjectBody::read` reads through `ByteStream::next()` (`crates/validate/src/s3/body.rs:464`). That calls `SdkBody::poll_next`, which stores a trailers frame and returns `None` (`aws-smithy-types` 1.8.1 `src/body.rs:220-226`). `ObjectBody` treats that `None` as the end: with `declared == None` it sets `ended` and returns `Ok(None)` (`body.rs:500-506`). I added scratch probes to a copy of `tests/s3_client_nonconforming_responses.rs` and ran them against the production `S3Client`:
  - `200`, `Transfer-Encoding: chunked`, a 3000-byte chunk, then `0\r\nX-Test: value\r\n\r\njunk`: **accepted as a 3000-byte object** (expected a body error saying "the body ended where its framing says").
  - The same with a whole second response (`HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n`) after the trailer section: **accepted**.
  - The same with more chunks (`5\r\nhello\r\n0\r\n\r\n`) after the trailer section: **accepted**.
  - Controls, all classified correctly: a trailer with nothing after it (accepted, which is right); a `503` chunked `<Error>` with trailer + junk on GET, PUT and DELETE, and a PUT/DELETE `200` with trailer + junk (all unreadable "after the end of the response", because the SDK's `collect` polls the outer body until `None`, which reaches the check); `Content-Length: 0` + junk on all three operations.

  So the hole is only on the streamed GET path. It breaks the invariant and the human's iteration-2 direction ("including bytes after a chunked body's terminal chunk"). It also makes the module doc's "anything at all fails the response" (`crates/validate/src/s3.rs:112-114`) and the matching sentence in `05-building-block-view.md` overclaim.

  **Suggested fix:** in `WholeResponse::poll_frame`, hold a trailers frame back. Poll `Incoming` to its `None`, ask the connection task for its count, and only then release the trailers, or `Overrun` if anything followed. An alternative is to make `ObjectBody` drive the body to its true end. **Tests to add:** a GET trailer + junk case and a GET trailer + clean end guard. The test file has no trailer case at all today, which is why C4-verify and C5 could not see this.

## Attempts that did not refute the fix

- **The red→green evidence holds.** The red leg in `gate-logs/C4-verify.log` compiles the new test against #852's client with the production change reverted, and it fails by assertion: 16 of 23 tests red, 0 compile errors. Green: 23/23. The tests drive the real `S3Client::with_deadlines` through the production argument parser (`tests/...:522-560`) against a raw-socket endpoint, not a copy of the client. The 7 tests that pass on the red leg are guards the brief allows (bodiless error, non-XML body, cut-short chunked framing, close-delimited GET, a GET over budget, a non-`chunked` coding on GET).
- **One overstated claim in `check-gates.json`:** the C4-verify row's "23 test(s) ran red" is the harness's wording. The log shows `7 passed; 16 failed` on the red leg. The red leg is still genuine, so this does not change the verdict.
- **The budget is now pinned.** The iteration-1 finding (a 16 MiB budget passed) is closed. There is an exact `BUDGET` / `BUDGET + 1` pair, sent with a `Content-Length` and chunked, on all three operations (`tests/...:1565-1614`). Removing the streaming cap in `Budgeted` (`response.rs:333-344`) would also fail the generated-body bytes-written oracle, which reads the whole ~69 MB body on the red leg.
- **Depth scanner against roxmltree.** I tried comments, CDATA, processing instructions, quoted `>` and `/>` inside attributes, unmatched or mismatched end tags, and `<!DOCTYPE`. I found no input where `nests_deeper_than` (`response.rs:453-488`) counts shallower than the parser would recurse before the parser stops on an error.
- **Framing headers.** `chunked\u{a0}`, `gzip, chunked`, a repeated `Transfer-Encoding`, `Content-Length: +5` or `5,`, and both headers at once are refused or match hyper's own reading (`response.rs:80-151`, hyper 1.10.1 `role.rs:1285-1306`). GetObject has no `force_error` path (`aws-sdk-s3` 1.152.0 `get_object.rs:334-341`), so no GET `200` is buffered outside the budget.
- **Not raised:**
  - Bytes that arrive after the client has checked the connection. The patch states this as a known limit at `wire.rs:22-24`.
  - The C4 diff-coverage row (the patch does not apply on `origin/main`). That is base staleness against #852, not a defect in this patch.
  - The surviving mutants at `wire.rs:217` and `wire.rs:234`. The first sits on a peek-error branch that loopback tests cannot reach. The second only removes the `abort()`, and dropping the `stop` sender already ends `drive`.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **Check for extra bytes before forwarding trailers** (`crates/validate/src/s3/wire.rs:305`). `WholeResponse::poll_frame` forwards every frame immediately, but asks the connection for unread bytes only after a subsequent poll returns `None`. The SDK's body adapter treats a trailer frame as end-of-stream, so that subsequent poll need not happen. A chunked response ending in `0\r\nX-Test: value\r\n\r\njunk` can therefore be accepted without checking the trailing junk, bypassing the new overrun detection. The frozen `gate-logs/T4-batch-review.log` identifies this same failure; the added overrun tests use an empty trailer section. Hold trailers until the connection check succeeds, and add regression cases for GET success and error responses on all three operations, alongside valid-trailer acceptance guards.

No additional actionable reuse, simplification, or efficiency findings. Reviewed target source and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Close the trailer escape from end-of-response validation—a peer can append already-arrived junk to a chunked GET and still receive a clean result; add the missing regression while preserving valid trailers (F1; `target/crates/validate/src/s3/wire.rs:306`).
- [ ] Validation — fitness-to-purpose — Decide whether inspecting only excess bytes already received at body completion and opening a fresh connection for every request fit the intended validator workloads—late-arriving bytes and connection churn limit what a clean result establishes (`target/crates/validate/src/s3/wire.rs:22`, `target/crates/validate/src/s3.rs:215`).
- [ ] **A chunked GET with a trailer section skips the check for bytes after the end, so it is accepted as a whole object.** `crates/validate/src/s3/wire.rs:305-306` passes hyper's trailers frame straight through. The check for what came after the body only runs on the *next* poll (`wire.rs:308-322`), and the GET path never makes that poll. `ObjectBody::read` reads through `ByteStream::next()` (`crates/validate/src/s3/body.rs:464`). That calls `SdkBody::poll_next`, which stores a trailers frame and returns `None` (`aws-smithy-types` 1.8.1 `src/body.rs:220-226`). `ObjectBody` treats that `None` as the end: with `declared == None` it sets `ended` and returns `Ok(None)` (`body.rs:500-506`). I added scratch probes to a copy of `tests/s3_client_nonconforming_responses.rs` and ran them against the production `S3Client`:
- [ ] **Check for extra bytes before forwarding trailers** (`crates/validate/src/s3/wire.rs:305`). `WholeResponse::poll_frame` forwards every frame immediately, but asks the connection for unread bytes only after a subsequent poll returns `None`. The SDK's body adapter treats a trailer frame as end-of-stream, so that subsequent poll need not happen. A chunked response ending in `0\r\nX-Test: value\r\n\r\njunk` can therefore be accepted without checking the trailing junk, bypassing the new overrun detection. The frozen `gate-logs/T4-batch-review.log` identifies this same failure; the added overrun tests use an empty trailer section. Hold trailers until the connection check succeeds, and add regression cases for GET success and error responses on all three operations, alongside valid-trailer acceptance guards.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_853/review-b
- [x] **The claimed reproductions are not established for #852.** `brief.md:8-23` attributes defects to “#852's client” using #741's unpublished attempts, and `brief.md:50-57` promises an assertion-only red against #852. Neither `notes.json` nor `sources/` is supplied. The repo and `main` resolve, but `dependency-state.json:2-5` records #852 as existing and `PLANNED`; the resolved target at `36f006d` has no `crates/validate/`, consistent with `$PDCA_TARGET/Cargo.toml:9-32`. This does not disprove the defects, but it prevents verifying the root cause, public API, dev-dependencies, or prior-attempt constraints. Revise these claims to hypotheses pending reproduction on the folded #852 commit, and identify the evidence needed to establish that baseline.
- [x] **The budget oracle can pass without enforcing a streamed-byte limit.** `brief.md:40-44` requires a lazily streamed oversized error and a “remove the budget” mutation, but leaves its framing unspecified. A fixture advertising an over-budget `Content-Length` permits a header-only rejection to satisfy both checks while unknown-length bodies remain unbounded. Require an otherwise-valid, oversized `<Error>` document sent chunked without `Content-Length`, with a long `<Message>` crossing the budget. Remove the streaming limiter while retaining any header precheck for the mutation. That makes the assertion distinguish bounded reading from merely trusting the advertised size.
- [x] **Chunked completeness has no negative acceptance case.** `brief.md:45-49` checks close-delimited refusal, a correctly terminated chunked GET, and declared-length disagreement; none requires rejection of an incomplete chunked body without `Content-Length`. Yet the cited invariant explicitly requires the terminal CRLF (`$PDCA_TARGET/AGENTS.md:161-164`). Add missing-terminal-chunk and truncated-terminal-CRLF cases: body error for GET success, and unreadable on all three operations for error responses whose XML is otherwise complete. These isolate framing failure from XML failure and prevent the positive chunked case from standing in for truncation detection.
- [x] **The all-response budget promise exceeds the scoped change.** `brief.md:5-7,58-61` promises bounded reading/memory for every response, but `brief.md:40-44,80-84` specifies an error-document budget and excludes changes to well-formed responses. A total-byte cap sized for error documents would reject legitimate larger GETs; an error-only cap cannot establish the universal memory claim. The target proposal explicitly includes large valid objects (`$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:283-291`). Narrow the guarantee to buffered error bodies, or separately define bounded-memory streaming for successful GETs and require a valid object larger than the error budget to remain accepted. Otherwise Do inherits an unstated second obligation beyond the named budget test.
- [x] size backstop — this slice is behaving oversized: patch is 166 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] C1 Spec — Decide whether the length guarantee includes surplus wire bytes beyond `Content-Length` — a declared 3-byte GET carrying `abcdef` returns `abc` successfully, while the SDK exposes only the declared body; this determines whether lower-level transport work belongs in scope (`brief.md:79`, `target/crates/validate/src/s3/body.rs:223`, `review-probes.log:5`).
- [x] **A body longer than its declared `Content-Length` is never detected, on any operation.** `crates/validate/tests/s3_client_nonconforming_responses.rs:921-935` (`length-under-sent`) goes green only because the declared length ends inside `<Message>`, so the XML check catches it. No length check is involved. Move the declared end to the `</Error>` close tag and send more bytes after it (`Content-Length: doc.len()`, body `doc + "<html>proxy junk…</html>"`). Then GET, PUT and DELETE each report a clean `Service { 503, Code("SlowDown"), "Please reduce your request rate." }` (probe run). A GET 200 with `Content-Length: 2000` and 3000 bytes sent is accepted as a 2000-byte object (probe run). The brief makes both cases binding ("an error body whose received length differs from its declared `Content-Length` … → unreadable"; "a GET whose received length disagrees with its declared length → body error"). The patch's own comment admits the gap: the length checks "cannot fire on any framing a peer can send" (`crates/validate/src/s3/body.rs:223-229`). The GET test leaves this direction out entirely (`:1255-1290`). hyper stops reading at the declared length and never reports the extra bytes. Catching them needs a connector-level check, so this is a scope call: build that check, or narrow the brief and the test names to "declared more than sent".
- [x] **An empty `<Code></Code>` is reported as a clean S3 error with code `""`.** On all three operations the result is `Service { 503, Code(""), Some("m") }` (probe run; `crates/validate/src/s3/response.rs:327-339`). The brief says "never a code the body did not carry", and an empty string is not an S3 code. Someone needs to decide whether an empty or whitespace-only `<Code>` should be `MissingInXml`, `Unreadable`, or stay as it is.

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
- Iteration delta (if iterating): One bug left: a successful chunked GET whose trailer section is followed by junk bytes is accepted as a clean object (F1; batch review, reviewer and adversary all reproduced it). Everything else holds; the size waiver from round 2 still stands. - Fix: in `WholeResponse::poll_frame` (crates/validate/src/s3/wire.rs:305-306), do not forward a trailers frame straight away. Poll the inner body to its end, run the connection's extra-bytes check, then release the trailers, or return Overrun if anything followed. (Alternative: make ObjectBody drive the body to its true end.) The module doc (s3.rs:112-114) and 05-building-block-view.md must then be true as written. - Tests to add: GET 200 chunked + trailer + junk -> body error; GET 200 chunked + trailer + clean end -> accepted byte-identical; trailer + junk on a 503 error on GET, PUT and DELETE -> unreadable; PUT/DELETE 200 with trailer + junk -> unreadable. Also cover a trailer followed by a second response and by more chunks. - Do not change anything else; the rest of the patch passed review.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
