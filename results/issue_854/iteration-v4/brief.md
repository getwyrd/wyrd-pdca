# Brief — validate-s3-client-upload-against-misbehaving-peers

- **Slug:** validate-s3-client-upload-against-misbehaving-peers
- **Track:** blackbox
- **Goal:** A PUT against a server that answers before the client has handed over the whole
  body, or that stops reading, ends as a failure, never as a receipt while the caller's
  source still had bytes to give. When the call returns, the caller's source has been
  dropped and the client has closed the connection.
- **Defect:** #852's client is expected to trust the SDK's outcome for an upload. Each case
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
- **Success criterion:** BINDING. The production client runs against a scripted loopback peer.
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
- **Falsifiability:** RED for scenario 1 is expected on #852's client by assertion (`cargo
  test -p wyrd-validate --test s3_client_upload_peers`, loopback only), on one condition that
  is checked, not assumed: the test names only #852's public API and dev-dependencies, so
  the C4 red leg compiles and runs. **Establish the baseline first, before any production
  change**, on the base this bundle builds on (the run's integration branch with #852 folded
  in; record the commit): (1) if #852's client is not in the base, STOP and report; (2)
  build the test against that commit unchanged and confirm it compiles; (3) run each
  scenario and record red or green in `build-notes.md`. For scenario 2, Do must **try to
  reproduce the retention on #852's client** with large pieces. If it reproduces, that is the
  red. If it does not after a real attempt (piece size, the peer's socket buffer sizes,
  early-ack timing), record the setups and results, keep scenario 2 as a regression guard,
  and say it is green-only. Scenario 3 is likely green on #852 (round 3's setup 1 released
  the source at once) and is then a guard. That posture is declared here, so Check expects
  it.
- **Invariant to restore:** *An upload's reported outcome never claims more than the client
  handed over: a receipt requires that the caller's source gave its whole declared length to
  the HTTP stack before the response arrived. No upload outlives its call: when
  `put_object` returns, the caller's source has been dropped and the client has closed the
  connection.* What lies beyond the client's sight, whether the peer read what was handed
  over, is the stated limit above, not a claim. Sources: `AGENTS.md` "Absent or unsupported
  entries" ("never silent success"), and "Await discipline" ("every await on external work
  is bounded … spawned helper tasks are aborted on drop"). Self-test: waking the body poller
  is not enough, because a connection blocked on socket writes never polls the body again.
  That was round 3's finding.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 852
- **Conflicts with:** 853
- **Ordering note:** builds on #852's upload path. `Conflicts with` #853 because both edit
  `crates/validate/src/client.rs`; the scheduler puts them in separate waves.
- **Surfaces:** data
- **Difficulty:** medium. One function's lifecycle plus one test file, but it reaches into how
  the SDK's connector owns the connection.
- **Do model:** opus
- **Scope:** the PUT path's outcome and its resource lifetime against a peer that acknowledges
  early or stops reading, with or without answering. Remove #852's `// deferred:` markers
  for this issue as each case lands. **If releasing a connection blocked mid-write cannot be
  done through the SDK's public connector or runtime API without forking it, STOP and report
  with the evidence.** Choosing a different HTTP seam is a Plan question, not something to
  paper over.
  **/ out of scope:** response classification and byte budgets (#853); GET; the 5 GiB
  property (#761); TLS; any edit outside `crates/validate/`.
- **Repro instruction:** on the base (#852 folded), serve a loopback peer that reads the
  request head, replies `200 OK`, `Content-Length: 0`, `x-amz-request-id: early`, and stops
  reading. Expected, per `iteration-v2/SUMMARY.md:138`: a #852 `put_object` with a 64 MiB
  generator returns `Ok` with that request id.
- **External dependencies:** none. Base Rust toolchain; loopback sockets only. The
  backpressure leg depends on socket buffer behaviour, so size the pieces to overwhelm it
  rather than tuning the kernel.
- **Test file:** `crates/validate/tests/s3_client_upload_peers.rs` (a NEW file,
  self-contained).
- **Citations expected:** `path:line` on the base for every change. Peer: #852's
  `put_object` and its source-body adapter, and the `// deferred:` markers it left.
- **Prior-art check (triage cycles):** the upload path exists only in #852. No merged or
  closed PR touches it. The findings come from #741's unpublished attempts (rounds 2 and 3),
  cited per case in Defect.
- **Disposition hint:** new-feature

## Plan-review response (#301 revision pass, 2026-10-02)

Four findings; all four revised the brief.

* **"The cause and the red are not grounded."** Correct: #852 is not built yet. Defect now
  calls each case a hypothesis for #852 and cites where it was seen in `results/issue_741/`,
  including the round 3 review that could not reproduce the retention. Falsifiability makes
  the baseline Do's first step: build the test unchanged against #852's folded commit and
  record red or green per scenario.
* **"The success exception lets through what the goal forbids."** Correct: "finished
  producing" did not cover a final piece taken but not yet written. The goal and invariant
  are narrowed to what the client can see, "the source gave up its whole declared length
  before the response arrived", and the brief now states that a large final piece still in
  buffers gets a receipt, why nothing client-side can do better, and pins the simple case
  with a test.
* **"The lifetime criterion has no fixed deadline and contradicts the invariant."** Correct.
  One contract now: `T_op = 3 s`, return within `T_op + 1 s` or fail, source already dropped
  at return with no grace period, peer sees the close within 1 s. The invariant says the
  same thing.
* **"Stops reading and never replies is not tested."** Correct. Scenario 3 adds it: expected
  outcome is the typed operation timeout plus the same lifetime contract.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Move the outcome boundary early enough to exclude bytes yielded after response consumption — the SDK interceptor snapshots too late and misclassifies a late final piece as complete; `crates/validate/src/s3.rs:393`, `crates/validate/src/s3/body.rs:104`, `crates/validate/src/s3/body.rs:125`.; T4 Contribution — Confirm closed/rejected and unpublished prior art for the four affected paths — merged-history and closed-PR text queries returned no hits, but text search cannot exclude matching closed diffs or inspect #741’s unpublished attempts; `pdca-reviewer-854-evidence/prior-art.log:1`, `brief.md:109`; contribution-artifact audit is separately N/A until publish.; T5 Judgment — Establish the source-failure test’s claimed event ordering — kernel TCP acknowledgement does not prove the SDK processed the response before the failure gate opens, so this oracle cannot certify its named interleaving; `crates/validate/tests/s3_client_upload_peers.rs:409`, `crates/validate/tests/s3_client_upload_peers.rs:487`.; **A receipt comes back after the client has already read the early `200`.** `crates/validate/src/s3.rs:387-395` stamps the response's arrival in the SDK's `read_after_transmit` hook. That hook runs in the SDK's future, after the connector future resolves (`aws-smithy-runtime` 1.15.0 `client/orchestrator.rs:503-511`). hyper parses the head in its *connection task*, and in that same poll it goes on to poll the request body (`hyper` 1.10.1 `proto/h1/dispatch.rs:172-175`, read then write; `:394` `poll_frame`). The body stops pulling only once `answer` is set (`crates/validate/src/s3/body.rs:176-178`), which hasn't happened yet, so the source keeps giving. **Concrete failing case:** a 10-byte source gives 4 bytes, then waits on a gate. The peer reads the head, writes `200 OK / Content-Length: 0 / x-amz-request-id: early`, then opens the gate. That is the test's scenario 1b with the source *succeeding* instead of failing, and without the 1 ms `wait_delivered` poll. Result: **200 of 200 runs returned `Ok(PutOutcome)`**. In every run, a probe inside the source read `/proc/net/tcp` at the poll that gave bytes 5-10. It found the client socket's receive queue empty and the peer's send queue empty, so hyper had already read the `200` when only 4 of 10 bytes had been given. When the peer opened the gate just *before* writing the `200`, 27 of 200 runs showed the same violation. This breaks the brief's invariant ("a receipt requires that the source gave its whole declared length … before the response arrived"). It also disproves the doc claims at `s3.rs:161-162`, `body.rs:139-141` and `error.rs:96` that the source "is not polled again" after the answer. The tests can't see it. The only post-ack source case fails rather than succeeds (`crates/validate/tests/s3_client_upload_peers.rs:678`). Its gate also waits for `wait_delivered` (`:409`, `:487`), which gives the PUT thread time to run the hook first. A pinning test is `gated_ack_then_open` in the repro. Fix options: (a) stamp the arrival before the source can be polled again. For example, the body could yield once (`tokio::task::yield_now`, which defers the wake) before each source poll, so the SDK future, already woken by hyper's dispatch, runs its hook first. That depends on tokio's current-thread scheduling order and costs a yield per piece, so it is fragile. (b) Stamp it on the socket read. `aws-smithy-http-client` 1.4.2 doesn't expose that (`wrap_connector` is `pub(crate)`, `src/client.rs:208`). That means a custom `HttpConnector`, which is a new HTTP seam and so a Plan question under the brief's Scope. If no in-seam fix holds, STOP and report. This is the same mechanism as the T4 review's `s3.rs:393 [BUG]`. This bullet adds a reproducer and a hit rate.; `crates/validate/src/s3.rs:393`: **The response snapshot can include bytes produced after the response head arrived.** Hyper can dispatch the head and continue polling the request body before yielding to this SDK interceptor; using one current-thread runtime prevents simultaneous polls, but does not prevent that ordering. A source releasing its final piece in this interval makes `answer.produced == declared`, allowing an early success to become a receipt. This is also identified in the frozen `T4-batch-review` evidence. Capture the count at the transport's response boundary, and cover a final piece becoming ready between head dispatch and interceptor execution.; `crates/validate/tests/s3_client_upload_peers.rs:409`: **The failure-after-acknowledgement test has a scheduling race.** An empty peer TCP send queue proves kernel delivery, not that the client processed the response. Opening the source gate immediately afterward can let Hyper poll the source failure before `ResponseArrival` runs, yielding `SourceFailed` without the request ID required by the assertion at line 654. Make the intended response-before-failure ordering observable and synchronized; the current fixture can fail depending on scheduling even though the frozen green runs passed.; `crates/validate/src/s3.rs:348`: **Runtime-start failure publishes the outcome before releasing the source.** If runtime construction fails, `request` still owns the upload body when `outcome_tx.send(...)` wakes the caller. The caller can return `RequestNotBuilt` before the worker exits and drops that request, violating the new source-dropped-at-return guarantee. Explicitly drop `request` before sending this error, matching the cleanup-before-publication order of the normal path.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 36 mutants tested in 2m: 2 missed, 8 caught, 26 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Establish the claimed response-before-source ordering — the released source can advance before the peer writes its answer, so the regression can reject a valid receipt and cannot reliably certify the response snapshot boundary; `crates/validate/tests/s3_client_upload_peers.rs:170`, `crates/validate/tests/s3_client_upload_peers.rs:409`, `crates/validate/tests/s3_client_upload_peers.rs:467`, `pdca-reviewer-854-evidence/held-instrumented.log:36`.; T4 Contribution — Resolve the unpublished #741 prior-art comparison — exact-path searches found no matches in merged history or any of 356 closed/merged PRs, but the earlier unpublished diffs are absent; contribution-artifact auditing is separately N/A until publish; `brief.md:109`, `pdca-reviewer-854-evidence/prior-art.log:10`, `gate-logs/T4-contribution.log:10`.; T5 Judgment — Add a committed cancellation regression — all six supplied cases await completion, leaving the newly promised cleanup on dropping an in-flight PUT unprotected despite the independent probe passing; `crates/validate/src/s3.rs:169`, `crates/validate/src/s3.rs:365`, `crates/validate/tests/s3_client_upload_peers.rs:577`, `pdca-reviewer-854-evidence/cancellation.log:15`.; **Two of the six new tests are flaky. They assert an ordering the fixture cannot guarantee.** `crates/validate/tests/s3_client_upload_peers.rs:170-176` claims that once the hold is released "the first moment the source can move on is the poll that follows the client reading the answer". `release_when_parked` (`:459-467`) releases the hold first and writes the answer second. That claim is false. I re-ran the frozen test binary. `a_final_piece_ready_as_the_acknowledgement_arrives_is_the_body_error` (`:712`) failed 1 of 300 times on its own. When I ran 300 copies of the three held tests in parallel, 21 runs failed: 12 times `a_final_piece…` got `Ok(PutOutcome { etag: None })`, and 10 times `a_source_that_fails…` (`:700`) got `SourceFailed { produced: 4 }` with no request id. Every failure had `parked: ReleasedBeforeAnswer`. Then I copied the fixture and instrumented the source. In every receipt, the poll that took the final piece came 5–36 ms after the release, and the answer's 63 bytes were still **unread** in the client socket's receive queue (`/proc/net/tcp` rx_queue = 63). So hyper had not read the answer yet. Production behaved exactly as its documented rule says (`crates/validate/src/s3.rs:155-158`, "the moment hyper reads it off the socket"); the test assertion is the part that is wrong. CI runs tests in parallel under load, so `cargo xtask ci` can go red at random. A fix that I measured: open the source only once the client has *consumed* the answer. The source self-wakes until the peer has written the ACK and the client socket's rx_queue is back to 0, then gives its last piece. With the patch, that version gave 0 receipts in 1800 runs under 60-way parallel load. With the `body_waits` guard disabled (`crates/validate/src/s3/body.rs:294`), it gave 1634 receipts in 1800. So it is reliable and still goes red without the fix. (If the human picks the stricter boundary in the next bullet, production has to change instead of this test.); **Docs currency (a MUST in the rubric) is still missing.** `docs/design/architecture/05-building-block-view.md:255` describes the validator's S3 client but says nothing of the PUT receipt rule, `BodyError::AcknowledgedEarly` (`crates/validate/src/s3/error.rs` new variant), or the per-PUT thread, runtime and unpooled connection. This changes an API operation's contract. The T4 batch review already blocks on it; I checked and the finding is real, not noise.; `crates/validate/src/s3.rs:365`: **The new cancellation branch is untested.** Every added scenario awaits `put_object` to completion (`crates/validate/tests/s3_client_upload_peers.rs:587`), so none exercises `Either::Right` when the caller drops the future. Ignoring abandonment would leave the detached worker, source, and connection alive until the operation deadline while all six tests still pass. Add a backpressure case that drops an in-flight PUT before that deadline and checks bounded source destruction and socket closure while the peer remains open. This corroborates the existing frozen T4 finding.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b. 9 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 52 mutants tested in 5m: 2 missed, 10 caught, 36 unviable, 4 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Resolve the comparison with #741's unpublished attempts — exact-path merged-history and all 356 closed/merged PR checks found no prior code/test implementation, but those unpublished diffs are absent from the supplied evidence; `brief.md:109`, `pdca-reviewer-854-evidence/prior-art.log:1`, `pdca-reviewer-854-evidence/prior-art-details.log:1`.; `crates/validate/src/s3/body.rs:289-291` (the "request has seen the response → `Poll::Pending`" check) is not covered by any test. With those three lines deleted, all 7 tests still pass (300 runs of the four small tests, 30 runs of the full suite). Even an added `assert_eq!(run.given_at_return, 4)` in the two held tests (`crates/validate/tests/s3_client_upload_peers.rs:811`, `:825`) stays green. The reason: the test's acknowledgement is `Content-Length: 0` (`s3_client_upload_peers.rs:108`). So the SDK request finishes in the same poll that records the answer, and the runtime shuts down before hyper can poll the body again. The check only matters when the response has a body that is still arriving. **Concrete failing case with the check removed** (scratch probe): the peer reads the head and sends `200 OK / Content-Length: 2 / x-amz-request-id: early`, then sends `ok` 200 ms later. The source gives 4 of 10 bytes, then fails once released. The result is `Err(Body(SourceFailed { produced: 4 }))` with **no request id**, which breaks brief scenario 1b ("a source that fails after the acknowledgement … carrying the acknowledgement's request id"). The same setup with an early `403 AccessDenied` whose XML body arrives late gives `SourceFailed`, and the server's 403 is lost. With a source that succeeds, the source is polled after the answer (given 10, not 4). That contradicts the doc claims at `crates/validate/src/s3.rs:163-165`, `crates/validate/src/s3/body.rs:251-254` and `crates/validate/src/s3/error.rs:97-99`. The patched code gets all three right (`AcknowledgedEarly{produced:4, early}`, `Service{403, AccessDenied, early}`, given stays 4), so this is a test gap, not a production bug. Fix: add a held case whose acknowledgement has a non-empty body sent after a delay (ideally one 2xx and one 4xx), and assert `given_at_return == 4` in the held tests.. 11 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 52 mutants tested in 5m: 2 missed, 10 caught, 36 unviable, 4 timeouts
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: The remaining findings come from the brief's own rules, so another Do round would repeat them (rounds 1-3 already did). Revise the brief: 1. Receipt rule: a receipt requires the source to have ENDED (polled to completion), not just to have given its declared length. This closes (a) the #852 regression where a source that overruns its declared length now gets a receipt instead of SourceLength, and (b) the "held its end" receipt where the server got 0 body bytes (the SDK's aws-chunked layer holds small bodies until the source ends). Update the s3.rs:155-161 rationale to match. 2. Per-PUT cost: closing the connection on every return forces a fresh TCP connection + OS thread + runtime per PUT. That caps uploads near ~470/s to one gateway (port exhaustion via TIME_WAIT), which the validator would then count against the server's availability, short of what proposal 0017's churn/endurance scenarios need. Decide: close only after abnormal endings (early ack, timeout, drop) and keep pooling for clean receipts, and/or release per connection with a custom connector (a new HTTP seam). Consider splitting this redesign off. 3. Decide whether the window between the answer reaching the client socket and hyper reading it falls within the stated inherent limit or needs the custom-connector seam. 4. Record seeded Tier-0 DST coverage as out of scope for crates/validate (no madsim build, no wyrd-* dependency; the mechanism is a real thread, runtime and socket), so the T4 gate stops blocking on it. 5. Carry a small test fix: the connection-close oracle should measure the 1 s bound from the call's actual return timestamp (s3_client_upload_peers.rs:606, :683, :820).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The remaining findings come from the brief's own rules, so another Do round would repeat them (rounds 1-3 already did). Revise the brief:
  1. Receipt rule: a receipt requires the source to have ENDED (polled to completion), not just to have given its declared length. This closes (a) the #852 regression where a source that overruns its declared length now gets a receipt instead of SourceLength, and (b) the "held its end" receipt where the server got 0 body bytes (the SDK's aws-chunked layer holds small bodies until the source ends). Update the s3.rs:155-161 rationale to match.
  2. Per-PUT cost: closing the connection on every return forces a fresh TCP connection + OS thread + runtime per PUT. That caps uploads near ~470/s to one gateway (port exhaustion via TIME_WAIT), which the validator would then count against the server's availability, short of what proposal 0017's churn/endurance scenarios need. Decide: close only after abnormal endings (early ack, timeout, drop) and keep pooling for clean receipts, and/or release per connection with a custom connector (a new HTTP seam). Consider splitting this redesign off.
  3. Decide whether the window between the answer reaching the client socket and hyper reading it falls within the stated inherent limit or needs the custom-connector seam.
  4. Record seeded Tier-0 DST coverage as out of scope for crates/validate (no madsim build, no wyrd-* dependency; the mechanism is a real thread, runtime and socket), so the T4 gate stops blocking on it.
  5. Carry a small test fix: the connection-close oracle should measure the 1 s bound from the call's actual return timestamp (s3_client_upload_peers.rs:606, :683, :820).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 52 mutants tested in 5m: 2 missed, 10 caught, 36 unviable, 4 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
