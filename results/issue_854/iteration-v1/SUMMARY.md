# Result — issue 854 / validate-s3-client-upload-against-misbehaving-peers

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: #852's client is expected to trust the SDK's outcome for an upload. Each case
  below is a **hypothesis for #852 until Do reproduces it on #852's folded commit** (see
  Falsifiability). Each was seen on one of #741's attempts, whose client #852 rebuilds, and
  the cause sits in hyper, which #852 keeps. The records are in this harness repo under
  `results/issue_741/`:
  - **Early acknowledgement (round 2), reproduced.** A server that reads only the request
    head and answers `200` gets a receipt. With a declared 64 MiB body, only 2.8 MiB had been
    produced. With a 10-byte body whose source fails after the ack, a receipt came back too.
    hyper returns the response as soon as its head arrives and keeps polling the body in the
    background. `iteration-v2/SUMMARY.md:138`.
  - **Upload retained under backpressure (round 3), disputed.** The setup: a 512 MiB
    generated upload in 32 MiB pieces, and a peer that stops reading, lets the socket buffers
    fill, then sends an early `200`. The PUT returned, but the source stayed alive four
    seconds past a three-second operation limit, and was released only when the peer closed.
    `iteration-v3/SUMMARY.md:112` (F2) and `:251`. A second reviewer could NOT reproduce it
    in three setups with smaller pieces (`iteration-v3/SUMMARY.md:178-193`).
- Success criterion: BINDING. The production client runs against a scripted loopback peer.
  **One lifetime contract, with fixed bounds, applies to every scenario below.** The test
  sets the operation timeout to `T_op = 3 s` and wraps each call in a `T_op + 1 s` timeout of
  its own:
  - the call returns within `T_op + 1 s`; not returning is a test failure with a message,
    never a hang;
  - at return, the source has **already been dropped**, observed through the test's own
    source type (a drop flag, not a new API), checked right after the call returns, with
    no grace period;
  - the peer sees its connection **closed** (EOF or reset on its next read) within 1 s
    after the call returns.
  The scenarios:
  1. **No receipt while the source still had bytes.** A PUT acknowledged while its source
     still has data to produce fails with the body error, carrying the acknowledgement's
     request id. So does a source that fails after the acknowledgement.
  2. **Early acknowledgement under backpressure.** The peer reads the head, waits until the
     client's writes back up, sends `200`, keeps the socket open and never reads again; the
     source yields pieces of 32 MiB or more. Outcome: the body error, plus the lifetime
     contract.
  3. **Stops reading, never answers.** The peer reads the head, stops reading, never sends a
     response and keeps the socket open; the source yields pieces of 32 MiB or more.
     Outcome: the typed operation timeout, plus the lifetime contract.
  **The inherent limit, narrowed and stated, not left implicit.** The client can see only
  what it handed to the HTTP stack, not what the peer read. So the rule is: a receipt
  requires that the source gave up its **whole declared length** before the response
  arrived. If it did, the outcome is a **receipt**, even if some of those bytes were still in
  the client's or the kernel's buffers when the peer answered. That includes the case of a
  large final piece taken but not yet written. Nothing on the client side can tell those
  bytes from bytes the peer read and threw away. Do documents this rule on `put_object`, and
  one test pins its simple form (a 10-byte source fully taken, then an early `200` →
  receipt, as observed in `iteration-v3/SUMMARY.md:241-245`) so the boundary cannot move
  silently.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the PUT path's outcome and its resource lifetime against a peer that acknowledges
  early or stops reading, with or without answering. Remove #852's `// deferred:` markers
  for this issue as each case lands. **If releasing a connection blocked mid-write cannot be
  done through the SDK's public connector or runtime API without forking it, STOP and report
  with the evidence.** Choosing a different HTTP seam is a Plan question, not something to
  paper over.
  **/ out of scope:** response classification and byte budgets (#853); GET; the 5 GiB
  property (#761); TLS; any edit outside `crates/validate/`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 36 mutants tested in 2m: 2 missed, 8 caught, 26 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.13s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #854’s S3 PUT outcome/lifetime fix: the supplied tests pass, but an independently reproduced response-ordering defect still permits a false receipt.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Resolve the crate-only scope versus mandatory architecture documentation — the changed PUT contract requires a same-PR living-doc update that the brief forbids; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the test compiled and produced five assertion failures, including false receipts and source retention under backpressure; `pdca-reviewer-854-evidence/red-green.log:276`, `crates/validate/tests/s3_client_upload_peers.rs:690`. |
| C3 Change | PASS | The patch stays within the declared PUT lifecycle surface and existing SDK dependencies; GET/response-classification deferrals remain scoped to #853; `crates/validate/src/s3.rs:173`, `crates/validate/src/s3.rs:222`, `crates/validate/src/s3/body.rs:98`. |
| C4 Verification (red→green) | FAIL | The supplied suite independently changes from five failures to five passes, but a supplemental real-client test still gets a receipt after the HTTP stack consumed an early response; `pdca-reviewer-854-evidence/red-green.log:340`, `pdca-reviewer-854-evidence/response-race.log:18`, `crates/validate/src/s3.rs:393`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Move the outcome boundary early enough to exclude bytes yielded after response consumption — the SDK interceptor snapshots too late and misclassifies a late final piece as complete; `crates/validate/src/s3.rs:393`, `crates/validate/src/s3/body.rs:104`, `crates/validate/src/s3/body.rs:125`. |
| T1 Structure | PASS | Per-upload state and teardown remain local, use the existing public SDK/Tokio seams, and add no shared mutable global or Wyrd production dependency; `crates/validate/src/s3/body.rs:79`, `crates/validate/src/s3.rs:331`, `crates/validate/Cargo.toml:26`. |
| T2 Shape | PASS | The body retains bounded polling and moves pieces without aggregation; the source/acknowledgement record centralizes outcome facts and preserves typed errors; `crates/validate/src/s3/body.rs:181`, `crates/validate/src/s3/error.rs:98`. |
| T3 Runtime | PASS | Independent executions satisfy the supplied drop-at-return, bounded connection-close, and operation-timeout assertions, including 32 MiB pieces; `crates/validate/tests/s3_client_upload_peers.rs:603`, `crates/validate/tests/s3_client_upload_peers.rs:710`, `pdca-reviewer-854-evidence/red-green.log:333`. |
| T4 Contribution | NEEDS-HUMAN | Confirm closed/rejected and unpublished prior art for the four affected paths — merged-history and closed-PR text queries returned no hits, but text search cannot exclude matching closed diffs or inspect #741’s unpublished attempts; `pdca-reviewer-854-evidence/prior-art.log:1`, `brief.md:109`; contribution-artifact audit is separately N/A until publish. |
| T5 Judgment | NEEDS-HUMAN [impl] | Establish the source-failure test’s claimed event ordering — kernel TCP acknowledgement does not prove the SDK processed the response before the failure gate opens, so this oracle cannot certify its named interleaving; `crates/validate/tests/s3_client_upload_peers.rs:409`, `crates/validate/tests/s3_client_upload_peers.rs:487`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether one OS thread/runtime per PUT and disabled connection reuse fit the validator’s intended concurrency — lifetime tests exercise the behavior, but throughput/resource suitability remains a product decision after the correctness defect is fixed; `crates/validate/src/s3.rs:140`, `crates/validate/src/s3.rs:164`, `crates/validate/src/s3.rs:339`. |

Source citations above resolve under `$PDCA_TARGET` (`target/`); brief and evidence citations resolve in this review directory. This is advisory: deterministic gates retain their own authority.

1. **The false receipt is a demonstrated implementation defect.** The diagnostic uses the unchanged patched production library and real loopback TCP. A ten-byte source supplies four bytes, then withholds six. The peer sends `200` with `x-amz-request-id: early`. The final piece is released only after the peer’s transmit queue is empty (TCP acknowledgement) and the client’s receive queue is empty (the HTTP stack consumed the response). The PUT nevertheless returns `Ok(PutOutcome { etag: None })`; source drop and connection-close checks pass. Thus the defect concerns the receipt boundary, not resource retention. `ResponseArrival` records the count only at the later SDK hook, allowing `Upload::acknowledged_early` to see all ten bytes (`crates/validate/src/s3.rs:393`, `crates/validate/src/s3/body.rs:120`). Preserve the existing complete-before-response success case while adding this late-final-piece regression. The standalone diagnostic source is `pdca-reviewer-854-evidence/response-race.rs`; its compiled reproduction is runnable as `./pdca-reviewer-854-evidence/response-race reviewer_final_piece_released_only_after_reply --exact --nocapture`. The observed failure is recorded at `pdca-reviewer-854-evidence/response-race.log:15`.

2. **Documentation needs a Plan scope decision.** The public PUT success/error and lifetime contract changes at `crates/validate/src/s3.rs:155`, while the living client description remains unchanged at `docs/design/architecture/05-building-block-view.md:255`. The standing rule requires that update (`AGENTS.md:154`), but the brief excludes every edit outside `crates/validate/` (`brief.md:97`). Resolve that conflict explicitly; a rebuild constrained to the same brief cannot discharge it.

3. **The source-failure fixture does not guarantee its claimed ordering.** `wait_delivered` observes kernel delivery and then opens the source’s gate; the comment treats this as client-side response processing (`crates/validate/tests/s3_client_upload_peers.rs:407`). Those are different events. I repeated the existing patched test 300 times and all passed (`pdca-reviewer-854-evidence/repeat-after-ack.log:1`); I am reporting an unsupported ordering guarantee, not an observed flaky failure. Strengthen the event-order evidence without relying on a sleep, and cover the demonstrated late-final-piece case separately. The frozen batch review’s six entries reduce to these three classes, not six independent defects (`gate-logs/T4-batch-review.log:10`).

The independent red→green run used the supplied self-contained target’s synthetic base `7925fe6` (identified as pre-fix `df68932f2c633586fc2ce60cc418f878f8010d7d`). Stash/pop restored the original patch, and reverse-apply validation and `git diff --check` passed afterward. Scenario 2 reproduced a stationary 2,500,849-byte send queue with the source retained at return; scenario 3 also reproduced source retention despite returning the correct timeout (`pdca-reviewer-854-evidence/red-green.log:292`, `pdca-reviewer-854-evidence/red-green.log:296`). No missing tool, simulated service, or unexercised topology substitutes for these loopback results.

The remaining gate evidence is accounted for as follows:

- **CI: frozen PASS; independent rerun partially completed.** Typos, docs lint/render, repository guards, formatting, workspace clippy/build/tests excluding DST, and cargo-machete completed. Cargo-deny then stopped because the sandbox cannot lock its read-only advisory database (`pdca-reviewer-854-evidence/ci.log:3179`). This is a host limitation, not a patch defect; the frozen log records successful cargo-deny checks and the later DST checks (`gate-logs/C4-ci.log:3375`, `gate-logs/C4-ci.log:3979`).
- **Diff coverage: unmeasured, not a code failure.** The wrapper could not apply the dependent patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target contains the prerequisite and supports the independent red→green run. No coverage percentage is inferred, and this row is not the reason for C4’s FAIL.
- **Mutation testing: no actionable surviving behavior mutant shown.** Both survivors replace only the interceptor’s diagnostic name (`gate-logs/C5-mutants.log:13`, `crates/validate/src/s3.rs:384`). The log reports eight caught and 26 unviable mutants; it does not establish complete behavioral coverage.
- **Contribution artifacts: N/A.** The row deliberately defers because the publish artifacts are not drafted at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). This creates no human clearance item.
- **TiKV feature check: frozen PASS.** Its captured output shows both requested clippy compilations completing; it is unrelated to the PUT defect (`gate-logs/host-tikv.log:2`).

The prior-art investigation queried default-branch commit history separately for all four affected file paths and searched closed PR text for each exact path; every query returned zero results (`pdca-reviewer-854-evidence/prior-art.log:1`). That is narrower than proving no closed/rejected diff or unpublished attempt touched them, which is the outstanding T4 decision. Existing #853 deferrals and the previously accepted SDK dependency decision are not reopened.

### Advisory — adversary

# Adversarial review — #854 (S3 client PUT against misbehaving peers)

I re-ran the tests in a scratch copy of `$PDCA_TARGET` with the patch applied: the patch's
`s3_client_upload_peers` passes 5/5. The frozen red log (`gate-logs/C4-verify.log`) shows all 5
red on the base for the reasons the brief predicts. The red→green is genuine and goes through the
production `S3Client::put_object` over real loopback TCP. The findings below are about what those
tests don't cover. The repro used for every number below is saved next to this file as
`adversary_854_repro.rs` (drop it into `crates/validate/tests/`; it uses only the public API).

- NEEDS-HUMAN [impl] — **A receipt comes back after the client has already read the early `200`.**
  `crates/validate/src/s3.rs:387-395` stamps the response's arrival in the SDK's
  `read_after_transmit` hook. That hook runs in the SDK's future, after the connector future
  resolves (`aws-smithy-runtime` 1.15.0 `client/orchestrator.rs:503-511`). hyper parses the head
  in its *connection task*, and in that same poll it goes on to poll the request body (`hyper`
  1.10.1 `proto/h1/dispatch.rs:172-175`, read then write; `:394` `poll_frame`). The body stops
  pulling only once `answer` is set (`crates/validate/src/s3/body.rs:176-178`), which hasn't
  happened yet, so the source keeps giving. **Concrete failing case:** a 10-byte source gives 4
  bytes, then waits on a gate. The peer reads the head, writes
  `200 OK / Content-Length: 0 / x-amz-request-id: early`, then opens the gate. That is the test's
  scenario 1b with the source *succeeding* instead of failing, and without the 1 ms
  `wait_delivered` poll. Result: **200 of 200 runs returned `Ok(PutOutcome)`**. In every run, a
  probe inside the source read `/proc/net/tcp` at the poll that gave bytes 5-10. It found the
  client socket's receive queue empty and the peer's send queue empty, so hyper had already read
  the `200` when only 4 of 10 bytes had been given. When the peer opened the gate just *before*
  writing the `200`, 27 of 200 runs showed the same violation. This breaks the brief's
  invariant ("a receipt requires that the source gave its whole declared length … before the
  response arrived"). It also disproves the doc claims at `s3.rs:161-162`, `body.rs:139-141` and
  `error.rs:96` that the source "is not polled again" after the answer.
  The tests can't see it. The only post-ack source case fails rather than succeeds
  (`crates/validate/tests/s3_client_upload_peers.rs:678`). Its gate also waits for
  `wait_delivered` (`:409`, `:487`), which gives the PUT thread time to run the hook first. A
  pinning test is `gated_ack_then_open` in the repro.
  Fix options: (a) stamp the arrival before the source can be polled again. For example, the body
  could yield once (`tokio::task::yield_now`, which defers the wake) before each source poll, so
  the SDK future, already woken by hyper's dispatch, runs its hook first. That depends on tokio's
  current-thread scheduling order and costs a yield per piece, so it is fragile. (b) Stamp it on
  the socket read. `aws-smithy-http-client` 1.4.2 doesn't expose that (`wrap_connector` is
  `pub(crate)`, `src/client.rs:208`). That means a custom `HttpConnector`, which is a new HTTP
  seam and so a Plan question under the brief's Scope. If no in-seam fix holds, STOP and report.
  This is the same mechanism as the T4 review's `s3.rs:393 [BUG]`. This bullet adds a reproducer
  and a hit rate.

- NEEDS-HUMAN [human] — **Every PUT is now a new TCP connection that the client closes, plus a new
  OS thread and tokio runtime.** `crates/validate/src/s3.rs:137-143` turns connection reuse off
  for uploads. That is required by this design, since a pooled connection's task dies with its
  runtime. `s3.rs:331-374` spawns a thread per PUT, and the doc names only the thread as the cost
  (`s3.rs:172`). Measured: 300 sequential 10-byte PUTs against a keep-alive peer gave 300
  accepted connections and 300 client sockets left in `TIME_WAIT`. This host has 28,232 ephemeral
  ports (`32768-60999`), and `TIME_WAIT` lasts 60 s. For a non-loopback endpoint, where
  `tcp_tw_reuse=2` does not apply, that caps sustained PUTs at about 470/s per client IP and
  endpoint address. Past that, `connect` fails with `EADDRNOTAVAIL`. Proposal 0017's gating
  `endurance` scenario is mostly small objects, many workers, and long runs
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:532-542`). It counts transport
  failures against the *deployment's* `availability` budget (`:489`), so the validator's own port
  shortage could be reported as the server failing. PUT latency figures also now include a TCP
  handshake that GET and DELETE latencies don't. Two calls for a human: is that cost acceptable
  for the validator? And should the release mechanism be per connection (a custom connector)
  instead of per runtime?

- Attempted and could not refute:
  (a) **The red→green proof.** It is genuine. The receipt-limit test
  (`s3_client_upload_peers.rs:729`) was red on the base only through the lifetime contract
  (`client_close: Not`). The brief applies that contract to every scenario, so the red is
  legitimate.
  (b) **The untested claim that dropping the future releases everything promptly**
  (`s3.rs:166`, `:325-326`). I dropped the `put_object` future mid-upload against a peer that
  stops reading. The source was dropped within about 8 ms, and the client socket left
  `ESTABLISHED` within about 2 ms.
  (c) **A peer that answers early under backpressure, then drains everything.** I ran 4, 6, 8 and
  16 MiB in 64 KiB pieces, 10 runs each (40 total). All 40 returned `AcknowledgedEarly`: the
  client wakes on the `200` before the drain frees socket space, so the hook runs before the
  source is polled again.
  (d) **Concurrent PUTs handing each other a connection across runtimes.** This can't happen.
  `pool_max_idle_per_host(0)` turns hyper-util's pool off entirely (`hyper-util` 0.1.20
  `client/legacy/pool.rs:115-117`, `client.rs:396`), so every request connects fresh and no
  checkout race exists.
  (e) **The T4 review's `wait_delivered` race** (`s3_client_upload_peers.rs:409`). I did not see
  it flake. hyper reads before it polls the body within one task poll, and the `200` is already
  in the client's kernel buffer when the gate opens.
  (f) **The 2 surviving mutants** (`ResponseArrival::name`, `s3.rs:384`). These are an
  unobservable label, not a test gap.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:393`: **The response snapshot can include bytes produced after the response head arrived.** Hyper can dispatch the head and continue polling the request body before yielding to this SDK interceptor; using one current-thread runtime prevents simultaneous polls, but does not prevent that ordering. A source releasing its final piece in this interval makes `answer.produced == declared`, allowing an early success to become a receipt. This is also identified in the frozen `T4-batch-review` evidence. Capture the count at the transport's response boundary, and cover a final piece becoming ready between head dispatch and interceptor execution.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_upload_peers.rs:409`: **The failure-after-acknowledgement test has a scheduling race.** An empty peer TCP send queue proves kernel delivery, not that the client processed the response. Opening the source gate immediately afterward can let Hyper poll the source failure before `ResponseArrival` runs, yielding `SourceFailed` without the request ID required by the assertion at line 654. Make the intended response-before-failure ordering observable and synchronized; the current fixture can fail depending on scheduling even though the frozen green runs passed.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:348`: **Runtime-start failure publishes the outcome before releasing the source.** If runtime construction fails, `request` still owns the upload body when `outcome_tx.send(...)` wakes the caller. The caller can return `RequestNotBuilt` before the worker exits and drops that request, violating the new source-dropped-at-return guarantee. Explicitly drop `request` before sending this error, matching the cleanup-before-publication order of the normal path.

No additional reuse, simplification, or efficiency findings. Review used the target source and frozen gate evidence; no builds or tests were rerun. The two surviving mutants change only the interceptor's diagnostic name and do not establish a correctness defect.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Resolve the crate-only scope versus mandatory architecture documentation — the changed PUT contract requires a same-PR living-doc update that the brief forbids; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`.
- [ ] C5 Causal adequacy — Move the outcome boundary early enough to exclude bytes yielded after response consumption — the SDK interceptor snapshots too late and misclassifies a late final piece as complete; `crates/validate/src/s3.rs:393`, `crates/validate/src/s3/body.rs:104`, `crates/validate/src/s3/body.rs:125`.
- [ ] T4 Contribution — Confirm closed/rejected and unpublished prior art for the four affected paths — merged-history and closed-PR text queries returned no hits, but text search cannot exclude matching closed diffs or inspect #741’s unpublished attempts; `pdca-reviewer-854-evidence/prior-art.log:1`, `brief.md:109`; contribution-artifact audit is separately N/A until publish.
- [ ] T5 Judgment — Establish the source-failure test’s claimed event ordering — kernel TCP acknowledgement does not prove the SDK processed the response before the failure gate opens, so this oracle cannot certify its named interleaving; `crates/validate/tests/s3_client_upload_peers.rs:409`, `crates/validate/tests/s3_client_upload_peers.rs:487`.
- [ ] Validation — fitness-to-purpose — Decide whether one OS thread/runtime per PUT and disabled connection reuse fit the validator’s intended concurrency — lifetime tests exercise the behavior, but throughput/resource suitability remains a product decision after the correctness defect is fixed; `crates/validate/src/s3.rs:140`, `crates/validate/src/s3.rs:164`, `crates/validate/src/s3.rs:339`.
- [ ] **A receipt comes back after the client has already read the early `200`.** `crates/validate/src/s3.rs:387-395` stamps the response's arrival in the SDK's `read_after_transmit` hook. That hook runs in the SDK's future, after the connector future resolves (`aws-smithy-runtime` 1.15.0 `client/orchestrator.rs:503-511`). hyper parses the head in its *connection task*, and in that same poll it goes on to poll the request body (`hyper` 1.10.1 `proto/h1/dispatch.rs:172-175`, read then write; `:394` `poll_frame`). The body stops pulling only once `answer` is set (`crates/validate/src/s3/body.rs:176-178`), which hasn't happened yet, so the source keeps giving. **Concrete failing case:** a 10-byte source gives 4 bytes, then waits on a gate. The peer reads the head, writes `200 OK / Content-Length: 0 / x-amz-request-id: early`, then opens the gate. That is the test's scenario 1b with the source *succeeding* instead of failing, and without the 1 ms `wait_delivered` poll. Result: **200 of 200 runs returned `Ok(PutOutcome)`**. In every run, a probe inside the source read `/proc/net/tcp` at the poll that gave bytes 5-10. It found the client socket's receive queue empty and the peer's send queue empty, so hyper had already read the `200` when only 4 of 10 bytes had been given. When the peer opened the gate just *before* writing the `200`, 27 of 200 runs showed the same violation. This breaks the brief's invariant ("a receipt requires that the source gave its whole declared length … before the response arrived"). It also disproves the doc claims at `s3.rs:161-162`, `body.rs:139-141` and `error.rs:96` that the source "is not polled again" after the answer. The tests can't see it. The only post-ack source case fails rather than succeeds (`crates/validate/tests/s3_client_upload_peers.rs:678`). Its gate also waits for `wait_delivered` (`:409`, `:487`), which gives the PUT thread time to run the hook first. A pinning test is `gated_ack_then_open` in the repro. Fix options: (a) stamp the arrival before the source can be polled again. For example, the body could yield once (`tokio::task::yield_now`, which defers the wake) before each source poll, so the SDK future, already woken by hyper's dispatch, runs its hook first. That depends on tokio's current-thread scheduling order and costs a yield per piece, so it is fragile. (b) Stamp it on the socket read. `aws-smithy-http-client` 1.4.2 doesn't expose that (`wrap_connector` is `pub(crate)`, `src/client.rs:208`). That means a custom `HttpConnector`, which is a new HTTP seam and so a Plan question under the brief's Scope. If no in-seam fix holds, STOP and report. This is the same mechanism as the T4 review's `s3.rs:393 [BUG]`. This bullet adds a reproducer and a hit rate.
- [ ] **Every PUT is now a new TCP connection that the client closes, plus a new OS thread and tokio runtime.** `crates/validate/src/s3.rs:137-143` turns connection reuse off for uploads. That is required by this design, since a pooled connection's task dies with its runtime. `s3.rs:331-374` spawns a thread per PUT, and the doc names only the thread as the cost (`s3.rs:172`). Measured: 300 sequential 10-byte PUTs against a keep-alive peer gave 300 accepted connections and 300 client sockets left in `TIME_WAIT`. This host has 28,232 ephemeral ports (`32768-60999`), and `TIME_WAIT` lasts 60 s. For a non-loopback endpoint, where `tcp_tw_reuse=2` does not apply, that caps sustained PUTs at about 470/s per client IP and endpoint address. Past that, `connect` fails with `EADDRNOTAVAIL`. Proposal 0017's gating `endurance` scenario is mostly small objects, many workers, and long runs (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:532-542`). It counts transport failures against the *deployment's* `availability` budget (`:489`), so the validator's own port shortage could be reported as the server failing. PUT latency figures also now include a TCP handshake that GET and DELETE latencies don't. Two calls for a human: is that cost acceptable for the validator? And should the release mechanism be per connection (a custom connector) instead of per runtime?
- [ ] `crates/validate/src/s3.rs:393`: **The response snapshot can include bytes produced after the response head arrived.** Hyper can dispatch the head and continue polling the request body before yielding to this SDK interceptor; using one current-thread runtime prevents simultaneous polls, but does not prevent that ordering. A source releasing its final piece in this interval makes `answer.produced == declared`, allowing an early success to become a receipt. This is also identified in the frozen `T4-batch-review` evidence. Capture the count at the transport's response boundary, and cover a final piece becoming ready between head dispatch and interceptor execution.
- [ ] `crates/validate/tests/s3_client_upload_peers.rs:409`: **The failure-after-acknowledgement test has a scheduling race.** An empty peer TCP send queue proves kernel delivery, not that the client processed the response. Opening the source gate immediately afterward can let Hyper poll the source failure before `ResponseArrival` runs, yielding `SourceFailed` without the request ID required by the assertion at line 654. Make the intended response-before-failure ordering observable and synchronized; the current fixture can fail depending on scheduling even though the frozen green runs passed.
- [ ] `crates/validate/src/s3.rs:348`: **Runtime-start failure publishes the outcome before releasing the source.** If runtime construction fails, `request` still owns the upload body when `outcome_tx.send(...)` wakes the caller. The caller can return `RequestNotBuilt` before the worker exits and drops that request, violating the new source-dropped-at-return guarantee. Explicitly drop `request` before sending this error, matching the cleanup-before-publication order of the normal path.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- [ ] The asserted cause and guaranteed red test are not grounded in the supplied record. `brief.md:8` attributes #741's historical behavior to #852, and `brief.md:32` / `brief.md:38` promise an assertion failure and a compiling red leg. Neither `notes.json` nor `sources/` is present. #852 exists but is still `PLANNED` (`dependency-state.json:2–5`); the resolved target at `36f006db6bf1ab4fe850b0a2698f145bb53b0892` has no `crates/validate/` (workspace membership: `$PDCA_TARGET/Cargo.toml:9–32`). The repository and local `main` resolve correctly, so this is an unavailable implementation prerequisite, not a phantom dependency. Revise the cause and red-test claims as hypotheses conditional on inspecting/reproducing against the folded #852 base, and provide the original tracker evidence before treating the prior attempts or their constraints as verified.
- [ ] The success exception permits behavior the goal forbids. `brief.md:5–6` promises no receipt for bytes not sent, but `brief.md:29–31` grants a receipt whenever the source finished producing, reasoning about bytes “it already wrote.” No evidence establishes that production means completed socket writes; the brief itself describes large pieces and blocked writes (`brief.md:25–26`, `brief.md:45–46`). A final large piece produced before the acknowledgement but still awaiting transport writes is not excluded by the exception. Specify the expected outcome and regression for that case, or narrow the goal/invariant to incomplete source production and explicitly acknowledge the remaining transport uncertainty.
- [ ] The binding lifetime criterion has no fixed deadline and contradicts the invariant. `brief.md:26–28` permits release “within a stated small bound” after return, while `brief.md:40–42` requires nothing to retain the source or connection once the call returns. No numeric grace period or maximum call duration is specified; a call that never returns never reaches the post-return assertion. Choose one lifetime contract, fix the operation and cleanup bounds, and require the test to fail on non-return as well as late source drop or connection closure.
- [ ] A promised failure path is absent from acceptance coverage. The goal and scope include a peer that “stops reading” (`brief.md:5–7`, `brief.md:56–57`), but both binding scenarios require an acknowledgement, including an explicit `200` in the backpressure case (`brief.md:21–25`). Those cases cannot establish cleanup when the peer stops reading and never replies. Add a bounded no-response scenario that checks failure and resource release on the operation timeout, or explicitly narrow the promised behavior to early-acknowledgement paths.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Move the outcome boundary early enough to exclude bytes yielded after response consumption — the SDK interceptor snapshots too late and misclassifies a late final piece as complete; `crates/validate/src/s3.rs:393`, `crates/validate/src/s3/body.rs:104`, `crates/validate/src/s3/body.rs:125`.; T4 Contribution — Confirm closed/rejected and unpublished prior art for the four affected paths — merged-history and closed-PR text queries returned no hits, but text search cannot exclude matching closed diffs or inspect #741’s unpublished attempts; `pdca-reviewer-854-evidence/prior-art.log:1`, `brief.md:109`; contribution-artifact audit is separately N/A until publish.; T5 Judgment — Establish the source-failure test’s claimed event ordering — kernel TCP acknowledgement does not prove the SDK processed the response before the failure gate opens, so this oracle cannot certify its named interleaving; `crates/validate/tests/s3_client_upload_peers.rs:409`, `crates/validate/tests/s3_client_upload_peers.rs:487`.; **A receipt comes back after the client has already read the early `200`.** `crates/validate/src/s3.rs:387-395` stamps the response's arrival in the SDK's `read_after_transmit` hook. That hook runs in the SDK's future, after the connector future resolves (`aws-smithy-runtime` 1.15.0 `client/orchestrator.rs:503-511`). hyper parses the head in its *connection task*, and in that same poll it goes on to poll the request body (`hyper` 1.10.1 `proto/h1/dispatch.rs:172-175`, read then write; `:394` `poll_frame`). The body stops pulling only once `answer` is set (`crates/validate/src/s3/body.rs:176-178`), which hasn't happened yet, so the source keeps giving. **Concrete failing case:** a 10-byte source gives 4 bytes, then waits on a gate. The peer reads the head, writes `200 OK / Content-Length: 0 / x-amz-request-id: early`, then opens the gate. That is the test's scenario 1b with the source *succeeding* instead of failing, and without the 1 ms `wait_delivered` poll. Result: **200 of 200 runs returned `Ok(PutOutcome)`**. In every run, a probe inside the source read `/proc/net/tcp` at the poll that gave bytes 5-10. It found the client socket's receive queue empty and the peer's send queue empty, so hyper had already read the `200` when only 4 of 10 bytes had been given. When the peer opened the gate just *before* writing the `200`, 27 of 200 runs showed the same violation. This breaks the brief's invariant ("a receipt requires that the source gave its whole declared length … before the response arrived"). It also disproves the doc claims at `s3.rs:161-162`, `body.rs:139-141` and `error.rs:96` that the source "is not polled again" after the answer. The tests can't see it. The only post-ack source case fails rather than succeeds (`crates/validate/tests/s3_client_upload_peers.rs:678`). Its gate also waits for `wait_delivered` (`:409`, `:487`), which gives the PUT thread time to run the hook first. A pinning test is `gated_ack_then_open` in the repro. Fix options: (a) stamp the arrival before the source can be polled again. For example, the body could yield once (`tokio::task::yield_now`, which defers the wake) before each source poll, so the SDK future, already woken by hyper's dispatch, runs its hook first. That depends on tokio's current-thread scheduling order and costs a yield per piece, so it is fragile. (b) Stamp it on the socket read. `aws-smithy-http-client` 1.4.2 doesn't expose that (`wrap_connector` is `pub(crate)`, `src/client.rs:208`). That means a custom `HttpConnector`, which is a new HTTP seam and so a Plan question under the brief's Scope. If no in-seam fix holds, STOP and report. This is the same mechanism as the T4 review's `s3.rs:393 [BUG]`. This bullet adds a reproducer and a hit rate.; `crates/validate/src/s3.rs:393`: **The response snapshot can include bytes produced after the response head arrived.** Hyper can dispatch the head and continue polling the request body before yielding to this SDK interceptor; using one current-thread runtime prevents simultaneous polls, but does not prevent that ordering. A source releasing its final piece in this interval makes `answer.produced == declared`, allowing an early success to become a receipt. This is also identified in the frozen `T4-batch-review` evidence. Capture the count at the transport's response boundary, and cover a final piece becoming ready between head dispatch and interceptor execution.; `crates/validate/tests/s3_client_upload_peers.rs:409`: **The failure-after-acknowledgement test has a scheduling race.** An empty peer TCP send queue proves kernel delivery, not that the client processed the response. Opening the source gate immediately afterward can let Hyper poll the source failure before `ResponseArrival` runs, yielding `SourceFailed` without the request ID required by the assertion at line 654. Make the intended response-before-failure ordering observable and synchronized; the current fixture can fail depending on scheduling even though the frozen green runs passed.; `crates/validate/src/s3.rs:348`: **Runtime-start failure publishes the outcome before releasing the source.** If runtime construction fails, `request` still owns the upload body when `outcome_tx.send(...)` wakes the caller. The caller can return `RequestNotBuilt` before the worker exits and drops that request, violating the new source-dropped-at-return guarantee. Explicitly drop `request` before sending this error, matching the cleanup-before-publication order of the normal path.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
