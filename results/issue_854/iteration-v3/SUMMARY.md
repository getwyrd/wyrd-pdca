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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 52 mutants tested in 5m: 2 missed, 10 caught, 36 unviable, 4 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.13s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #854 fix for premature PUT receipts and upload cleanup passes independent verification; prior-art, concurrency-coverage policy, and fitness decisions remain for sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The receipt boundary and immediate source-drop/one-second connection-close requirements are explicit and falsifiable, including the fully-produced-body exception; `brief.md:25`, `brief.md:47`. |
| C2 Reproduction (red pre-fix) | PASS | The unchanged new test compiles against the supplied #852-containing base and fails six assertions, including premature receipts and retained sources under actual backpressure; `pdca-reviewer-854-evidence/provenance.log:2`, `pdca-reviewer-854-evidence/red.log:269`, `pdca-reviewer-854-evidence/red.log:307`. |
| C3 Change | PASS | The PUT contract remains within the existing SDK connector/runtime APIs, with the architecture update required by the standing rubric and the carry-forward review; `crates/validate/src/s3.rs:137`, `crates/validate/src/s3.rs:341`, `docs/design/architecture/05-building-block-view.md:255`, `AGENTS.md:154`. |
| C4 Verification (red→green) | PASS | Restoring the patch passes all 40 validator tests and the independent static checks; full CI is supported by its frozen log, while diff coverage remains unmeasured because its remote base lagged; `pdca-reviewer-854-evidence/green.log:38`, `pdca-reviewer-854-evidence/green.log:58`, `pdca-reviewer-854-evidence/green.log:71`, `pdca-reviewer-854-evidence/checks.log:1`, `gate-logs/C4-ci.log:3981`, `gate-logs/C4-diff-cov.log:10`. |
| C5 Causal adequacy | PASS | Late source bytes cannot turn the observed early response into a receipt, and blocked connection ownership ends before publication of the outcome; the response-ordering and cleanup regressions pass, with no capability-probe workaround; `crates/validate/src/s3/body.rs:159`, `crates/validate/src/s3/body.rs:294`, `crates/validate/src/s3.rs:359`, `crates/validate/src/s3.rs:372`, `pdca-reviewer-854-evidence/green.log:64`. |
| T1 Structure | PASS | The client retains its blackbox dependency boundary and the SDK transport seam; shared state is confined to one upload and its clock ownership is documented; `pdca-reviewer-854-evidence/blackbox.log:3`, `crates/validate/src/s3/body.rs:63`, `crates/validate/src/s3.rs:51`. |
| T2 Shape | PASS | The typed early-acknowledgement outcome and its public contract agree with the living architecture description, and formatting/Clippy/docs checks pass; `crates/validate/src/s3/error.rs:95`, `crates/validate/src/s3.rs:155`, `docs/design/architecture/05-building-block-view.md:255`, `pdca-reviewer-854-evidence/checks.log:1`. |
| T3 Runtime | PASS | The real TCP runs satisfy return, source-release, connection-close and cancellation bounds, including the allowed complete-source receipt; Linux socket observations were available, so the tests did not rely on their unobservable fallback; `crates/validate/tests/s3_client_upload_peers.rs:712`, `crates/validate/tests/s3_client_upload_peers.rs:894`, `pdca-reviewer-854-evidence/provenance.log:7`, `pdca-reviewer-854-evidence/green.log:63`. |
| T4 Contribution | NEEDS-HUMAN | Resolve the comparison with #741's unpublished attempts — exact-path merged-history and all 356 closed/merged PR checks found no prior code/test implementation, but those unpublished diffs are absent from the supplied evidence; `brief.md:109`, `pdca-reviewer-854-evidence/prior-art.log:1`, `pdca-reviewer-854-evidence/prior-art-details.log:1`. |
| T5 Judgment | NEEDS-HUMAN | Decide whether loopback-only concurrency coverage is an approved exception for this standalone validator — the new worker/cancellation and request/body scheduling paths have no seeded Tier-0 test despite the standing requirement; `AGENTS.md:188`, `crates/validate/src/s3.rs:349`, `crates/validate/src/s3/body.rs:230`, `crates/validate/tests/s3_client_upload_peers.rs:894`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the socket-read receipt boundary and one thread/fresh TCP connection per PUT for intended validator workloads — correctness is exercised, but workload suitability and connection churn remain product/architecture decisions; `crates/validate/src/s3.rs:155`, `crates/validate/src/s3.rs:175`, `docs/design/architecture/05-building-block-view.md:255`. |

No reproducible implementation defect remains in the reviewed patch. The complete patch reverse-applies cleanly to the restored disposable target; all source citations above refer to that target. Its synthetic baseline records original base `df68932f2c633586fc2ce60cc418f878f8010d7d`; the reviewer did not inspect builder notes or other checkouts (`pdca-reviewer-854-evidence/provenance.log:2`, `pdca-reviewer-854-evidence/provenance.log:13`).

The independent red run demonstrates the forbidden behavior directly. A 64 MiB upload returned a receipt after yielding only 3 MiB, with its source still alive. Under backpressure, a 512 MiB upload returned a receipt after one 32 MiB piece, with 2,500,849 bytes stalled in the send queue and its source still alive. The silent peer returned the typed three-second timeout but retained its source at return. The complete-source case had the correct receipt but failed connection cleanup. Cancellation alone was already green on the base; it remains a regression guard (`pdca-reviewer-854-evidence/red.log:274`, `pdca-reviewer-854-evidence/red.log:292`, `pdca-reviewer-854-evidence/red.log:302`, `pdca-reviewer-854-evidence/red.log:307`, `pdca-reviewer-854-evidence/red.log:312`).

The revised ordering fixture survives the previously problematic scheduling test. Its source checks peer delivery and client receive-queue consumption in that order before continuing; the unmodified base returned a receipt when the late final piece was taken, whereas the patch returned the required body error. All four acknowledgement cases passed 300 process runs at concurrency 12, totaling 1,200 test executions. This supports the fixture correction without claiming exhaustive scheduling coverage (`crates/validate/tests/s3_client_upload_peers.rs:233`, `pdca-reviewer-854-evidence/red.log:297`, `pdca-reviewer-854-evidence/stress.log:2`). The committed cancellation test drops the call well before its 60-second operation deadline and verifies bounded source/socket release (`crates/validate/tests/s3_client_upload_peers.rs:894`).

The frozen gates support the following limited conclusions; their instance-scoped wrappers are absent from the supplied target, so the logs supplement the independent runs above:

- **C4 CI: PASS from captured evidence.** The log records spelling, docs lint/render, dependency and source guards, formatting, Clippy, builds, tests, dependency audits, conformance and DST checks ending successfully; this review independently repeated the validator suite, validator Clippy, formatting, spelling, docs lint and blackbox guard (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:3372`, `gate-logs/C4-ci.log:3981`).
- **C4 verification: PASS, independently reproduced.** The frozen result is six failing assertions out of seven executed tests before the fix, then seven passes, matching this review (`gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:68`).
- **C4 coverage: unavailable, target-state caveat.** The gate could not apply the stacked patch to `origin/main`; it measured no coverage. This is not evidence of a compile/applicability defect in the supplied target, which independently built both legs (`gate-logs/C4-diff-cov.log:10`).
- **C5 mutation result: inspected, no demonstrated behavioral survivor.** Both missed mutants replace only `ResponseArrival::name`; ten mutations were caught, four timed out, and 36 were unviable. The timeout/unviable groups do not establish coverage, and the two diagnostic-name survivors do not establish a correctness defect (`gate-logs/C5-mutants.log:13`, `crates/validate/src/s3.rs:398`).
- **T4 batch review: PASS as recorded.** The supplied log reports zero blocking findings; the underlying individual reports are not among these inputs (`gate-logs/T4-batch-review.log:10`).
- **T4 contribution-artifact audit: N/A.** The PR description is intentionally drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- **TiKV feature compilation: PASS from captured evidence.** Both requested Clippy feature builds finish successfully; this unrelated feature matrix was not rerun locally (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`).

The prior-art scan used affected paths, including full pagination for the two PRs exceeding 100 files. The four code/test paths have no merged-main commits or closed/merged PR matches. The architecture path has 15 main commits and 14 matching PRs; their captured document diffs contain no validator-specific implementation. The remaining contribution decision concerns only the unavailable unpublished #741 work (`pdca-reviewer-854-evidence/prior-art-details.log:1`, `pdca-reviewer-854-evidence/prior-art-docs-summary.log:1`).

### Advisory — adversary

# Adversarial review — issue #854 (advisory)

**Summary.** I re-ran the red→green proof on a scratch copy of `$PDCA_TARGET`. It holds. With
base production code and the new test kept: 6 red, 1 green (the dropped-PUT test). With the patch:
7 green. The tests drive the production `S3Client::put_object` over real loopback TCP, not a copy.
The fixture is no longer flaky: 0 failures in 2,000 runs of the four small tests at 96-way
parallelism, and 0 in 80 runs of the full suite at 16-way. The key ordering guard has real test
coverage: with `Turn::body_waits` forced to `false`, the final-piece test goes red in 200 of 200
runs. Sending the outcome before `shutdown_background()` goes red in 40 of 40 runs. I could not
make the patched code give a wrong outcome: early 200 with a delayed body, early 403 with a
delayed body, a source failing after the answer, and a source holding its end-of-stream all came
out as the brief's rule says. One guard has no test, and the per-PUT connection design has a
throughput cost a human should weigh.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/body.rs:289-291` (the "request has seen the response → `Poll::Pending`" check) is not covered by any test. With those three lines deleted, all 7 tests still pass (300 runs of the four small tests, 30 runs of the full suite). Even an added `assert_eq!(run.given_at_return, 4)` in the two held tests (`crates/validate/tests/s3_client_upload_peers.rs:811`, `:825`) stays green. The reason: the test's acknowledgement is `Content-Length: 0` (`s3_client_upload_peers.rs:108`). So the SDK request finishes in the same poll that records the answer, and the runtime shuts down before hyper can poll the body again. The check only matters when the response has a body that is still arriving. **Concrete failing case with the check removed** (scratch probe): the peer reads the head and sends `200 OK / Content-Length: 2 / x-amz-request-id: early`, then sends `ok` 200 ms later. The source gives 4 of 10 bytes, then fails once released. The result is `Err(Body(SourceFailed { produced: 4 }))` with **no request id**, which breaks brief scenario 1b ("a source that fails after the acknowledgement … carrying the acknowledgement's request id"). The same setup with an early `403 AccessDenied` whose XML body arrives late gives `SourceFailed`, and the server's 403 is lost. With a source that succeeds, the source is polled after the answer (given 10, not 4). That contradicts the doc claims at `crates/validate/src/s3.rs:163-165`, `crates/validate/src/s3/body.rs:251-254` and `crates/validate/src/s3/error.rs:97-99`. The patched code gets all three right (`AcknowledgedEarly{produced:4, early}`, `Service{403, AccessDenied, early}`, given stays 4), so this is a test gap, not a production bug. Fix: add a held case whose acknowledgement has a non-empty body sent after a delay (ideally one 2xx and one 4xx), and assert `given_at_return == 4` in the held tests.

- NEEDS-HUMAN [human] — `crates/validate/src/s3.rs:131-143` and `:341-386` (no idle pool for PUTs, plus one OS thread and one runtime per PUT) change the validator's load profile. The docs say only "each one leaves a client socket in `TIME_WAIT`" (`s3.rs:175-177`, `docs/design/architecture/05-building-block-view.md:255`). Measured on this host: 2,000 sequential 1 KiB PUTs against a keep-alive loopback peer ran at 17,284 PUT/s over 1 TCP connection on base, and at 3,055 PUT/s over 2,000 connections with the patch, leaving 2,000 client sockets in `TIME_WAIT`. Here `ip_local_port_range` is `32768 60999` (28,232 ports), `TIME_WAIT` lasts 60 s, and `tcp_tw_reuse=2` (port reuse on loopback only). So a validator PUTting to one non-loopback gateway address tops out near **~470 new connections/s** before `connect()` starts failing with `EADDRNOTAVAIL`, and every concurrent PUT holds an OS thread. Clean receipts pay this too, not just the misbehaving-peer paths. The brief's goal does ask that "the client has closed the connection" on every return, so this follows from the brief. A human should still decide whether the throughput scenarios coming in #743 can live with this ceiling, or whether the close should apply only to early-ack and timeout outcomes, which would need a different runtime or connector design (a Plan question per the brief's Scope).

- `crates/validate/tests/s3_client_upload_peers.rs:58` (and `:950-963`): the comment says the receipt pin means "the boundary cannot move silently", but the pin only covers one side. The peer answers only after the source has reported its end (`Answer::WhenExhausted`, `:953`), so the whole body is written before the `200`. A rule that moved to "the source must have reported its end" or "hyper must have flushed" would pass this test too. I checked the real edge in a scratch probe: the source gives all 10 bytes, holds its end-of-stream, and the peer answers early. The patch returns a receipt (`Ok(PutOutcome)`), as the brief's stated limit says. Note that with the SDK's default aws-chunked encoding those 10 bytes sit in the SDK's chunk buffer and never reach hyper. The brief only asked for the simple form, so I'm not flagging this as a defect. A test of the given-all/end-held case would pin the actual edge.

- `check-gates.json` C4-verify row: the headline "7 test(s) ran red" is wrong. `gate-logs/C4-verify.log` shows "1 passed; 6 failed" on the red leg, and I reproduced that. `a_put_dropped_while_blocked_mid_write_releases_the_upload` is green on base, because base hyper already closes the connection when the response future is dropped. So it is a regression guard, not red→green proof. It does guard the new `Either::Right` abandon branch at `s3.rs:365-368`: without that branch the PUT would run to its 60 s deadline. Diff coverage (C4-diff-cov) was again never measured ("does not apply on origin/main"), so nothing shows coverage of the error-only paths at `s3.rs:357-361` (runtime build fails) and `s3.rs:377-379` (thread spawn fails).

**Refutation attempts that failed** (no finding):
- Ordering between response delivery and the `read_after_transmit` snapshot. Checked `aws-smithy-runtime` 1.15.0 `orchestrator.rs` (connector future resolves, then `set_response`, then interceptors, with no await between) and the aws-chunked wrapper (`aws-runtime` 1.10.0, which polls the inner body inside the same connection-task poll).
- Lost wakeups in `Turn` (single thread, `seen` read before `wakes`).
- Early non-2xx responses.
- Source panic (caught by the per-PUT runtime as before).
- Thread-spawn and runtime-build failure ordering (std drops the closure before `spawn` returns `Err`).
- `_abandon` dropping early: it lives until `outcome_rx` resolves.
- Removing `pool_max_idle_per_host(0)` or `tokio::task::unconstrained` leaves the tests green, but neither change produced a wrong outcome I could find, so both read as defensive rather than load-bearing.

### Advisory — code-review

No findings in either advisory lens: no introduced correctness bug or actionable reuse, simplification, or efficiency issue found.

Reviewed response ordering and body accounting (`crates/validate/src/s3/body.rs:230`, `crates/validate/src/s3/body.rs:273`), outcome classification and cleanup (`crates/validate/src/s3.rs:196`, `crates/validate/src/s3.rs:341`), and the peer fixtures and cancellation regression (`crates/validate/tests/s3_client_upload_peers.rs:233`, `crates/validate/tests/s3_client_upload_peers.rs:894`).

Validation used the frozen gate evidence, without rerunning builds: CI passed, including 14 existing S3 tests and all seven new peer tests; the unchanged new tests compiled and failed against reverted production code. The two surviving mutants only change the interceptor name. Diff coverage was unavailable because the patch did not apply to origin/main.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Resolve the comparison with #741's unpublished attempts — exact-path merged-history and all 356 closed/merged PR checks found no prior code/test implementation, but those unpublished diffs are absent from the supplied evidence; `brief.md:109`, `pdca-reviewer-854-evidence/prior-art.log:1`, `pdca-reviewer-854-evidence/prior-art-details.log:1`.
- [ ] T5 Judgment — Decide whether loopback-only concurrency coverage is an approved exception for this standalone validator — the new worker/cancellation and request/body scheduling paths have no seeded Tier-0 test despite the standing requirement; `AGENTS.md:188`, `crates/validate/src/s3.rs:349`, `crates/validate/src/s3/body.rs:230`, `crates/validate/tests/s3_client_upload_peers.rs:894`.
- [ ] Validation — fitness-to-purpose — Accept the socket-read receipt boundary and one thread/fresh TCP connection per PUT for intended validator workloads — correctness is exercised, but workload suitability and connection churn remain product/architecture decisions; `crates/validate/src/s3.rs:155`, `crates/validate/src/s3.rs:175`, `docs/design/architecture/05-building-block-view.md:255`.
- [ ] `crates/validate/src/s3/body.rs:289-291` (the "request has seen the response → `Poll::Pending`" check) is not covered by any test. With those three lines deleted, all 7 tests still pass (300 runs of the four small tests, 30 runs of the full suite). Even an added `assert_eq!(run.given_at_return, 4)` in the two held tests (`crates/validate/tests/s3_client_upload_peers.rs:811`, `:825`) stays green. The reason: the test's acknowledgement is `Content-Length: 0` (`s3_client_upload_peers.rs:108`). So the SDK request finishes in the same poll that records the answer, and the runtime shuts down before hyper can poll the body again. The check only matters when the response has a body that is still arriving. **Concrete failing case with the check removed** (scratch probe): the peer reads the head and sends `200 OK / Content-Length: 2 / x-amz-request-id: early`, then sends `ok` 200 ms later. The source gives 4 of 10 bytes, then fails once released. The result is `Err(Body(SourceFailed { produced: 4 }))` with **no request id**, which breaks brief scenario 1b ("a source that fails after the acknowledgement … carrying the acknowledgement's request id"). The same setup with an early `403 AccessDenied` whose XML body arrives late gives `SourceFailed`, and the server's 403 is lost. With a source that succeeds, the source is polled after the answer (given 10, not 4). That contradicts the doc claims at `crates/validate/src/s3.rs:163-165`, `crates/validate/src/s3/body.rs:251-254` and `crates/validate/src/s3/error.rs:97-99`. The patched code gets all three right (`AcknowledgedEarly{produced:4, early}`, `Service{403, AccessDenied, early}`, given stays 4), so this is a test gap, not a production bug. Fix: add a held case whose acknowledgement has a non-empty body sent after a delay (ideally one 2xx and one 4xx), and assert `given_at_return == 4` in the held tests.
- [ ] `crates/validate/src/s3.rs:131-143` and `:341-386` (no idle pool for PUTs, plus one OS thread and one runtime per PUT) change the validator's load profile. The docs say only "each one leaves a client socket in `TIME_WAIT`" (`s3.rs:175-177`, `docs/design/architecture/05-building-block-view.md:255`). Measured on this host: 2,000 sequential 1 KiB PUTs against a keep-alive loopback peer ran at 17,284 PUT/s over 1 TCP connection on base, and at 3,055 PUT/s over 2,000 connections with the patch, leaving 2,000 client sockets in `TIME_WAIT`. Here `ip_local_port_range` is `32768 60999` (28,232 ports), `TIME_WAIT` lasts 60 s, and `tcp_tw_reuse=2` (port reuse on loopback only). So a validator PUTting to one non-loopback gateway address tops out near **~470 new connections/s** before `connect()` starts failing with `EADDRNOTAVAIL`, and every concurrent PUT holds an OS thread. Clean receipts pay this too, not just the misbehaving-peer paths. The brief's goal does ask that "the client has closed the connection" on every return, so this follows from the brief. A human should still decide whether the throughput scenarios coming in #743 can live with this ceiling, or whether the close should apply only to early-ack and timeout outcomes, which would need a different runtime or connector design (a Plan question per the brief's Scope).
- [ ] The asserted cause and guaranteed red test are not grounded in the supplied record. `brief.md:8` attributes #741's historical behavior to #852, and `brief.md:32` / `brief.md:38` promise an assertion failure and a compiling red leg. Neither `notes.json` nor `sources/` is present. #852 exists but is still `PLANNED` (`dependency-state.json:2–5`); the resolved target at `36f006db6bf1ab4fe850b0a2698f145bb53b0892` has no `crates/validate/` (workspace membership: `$PDCA_TARGET/Cargo.toml:9–32`). The repository and local `main` resolve correctly, so this is an unavailable implementation prerequisite, not a phantom dependency. Revise the cause and red-test claims as hypotheses conditional on inspecting/reproducing against the folded #852 base, and provide the original tracker evidence before treating the prior attempts or their constraints as verified.
- [ ] The success exception permits behavior the goal forbids. `brief.md:5–6` promises no receipt for bytes not sent, but `brief.md:29–31` grants a receipt whenever the source finished producing, reasoning about bytes “it already wrote.” No evidence establishes that production means completed socket writes; the brief itself describes large pieces and blocked writes (`brief.md:25–26`, `brief.md:45–46`). A final large piece produced before the acknowledgement but still awaiting transport writes is not excluded by the exception. Specify the expected outcome and regression for that case, or narrow the goal/invariant to incomplete source production and explicitly acknowledge the remaining transport uncertainty.
- [ ] The binding lifetime criterion has no fixed deadline and contradicts the invariant. `brief.md:26–28` permits release “within a stated small bound” after return, while `brief.md:40–42` requires nothing to retain the source or connection once the call returns. No numeric grace period or maximum call duration is specified; a call that never returns never reaches the post-return assertion. Choose one lifetime contract, fix the operation and cleanup bounds, and require the test to fail on non-return as well as late source drop or connection closure.
- [ ] A promised failure path is absent from acceptance coverage. The goal and scope include a peer that “stops reading” (`brief.md:5–7`, `brief.md:56–57`), but both binding scenarios require an acknowledgement, including an explicit `200` in the backpressure case (`brief.md:21–25`). Those cases cannot establish cleanup when the peer stops reading and never replies. Add a bounded no-response scenario that checks failure and resource release on the operation timeout, or explicitly narrow the promised behavior to early-acknowledgement paths.
- [ ] C1 Spec — Resolve the crate-only scope versus mandatory architecture documentation — the changed PUT contract requires a same-PR living-doc update that the brief forbids; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`.
- [ ] **Every PUT is now a new TCP connection that the client closes, plus a new OS thread and tokio runtime.** `crates/validate/src/s3.rs:137-143` turns connection reuse off for uploads. That is required by this design, since a pooled connection's task dies with its runtime. `s3.rs:331-374` spawns a thread per PUT, and the doc names only the thread as the cost (`s3.rs:172`). Measured: 300 sequential 10-byte PUTs against a keep-alive peer gave 300 accepted connections and 300 client sockets left in `TIME_WAIT`. This host has 28,232 ephemeral ports (`32768-60999`), and `TIME_WAIT` lasts 60 s. For a non-loopback endpoint, where `tcp_tw_reuse=2` does not apply, that caps sustained PUTs at about 470/s per client IP and endpoint address. Past that, `connect` fails with `EADDRNOTAVAIL`. Proposal 0017's gating `endurance` scenario is mostly small objects, many workers, and long runs (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:532-542`). It counts transport failures against the *deployment's* `availability` budget (`:489`), so the validator's own port shortage could be reported as the server failing. PUT latency figures also now include a TCP handshake that GET and DELETE latencies don't. Two calls for a human: is that cost acceptable for the validator? And should the release mechanism be per connection (a custom connector) instead of per runtime?
- [ ] C3 Change — Resolve the crate-only scope versus the mandatory architecture update — the public PUT receipt/lifetime contract changed, but its living description is unchanged and the brief forbids editing it; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`, `crates/validate/src/s3.rs:155`.
- [ ] **The receipt boundary is "hyper parsed the head", not "the answer reached the client". Under load the gap between the two is tens of milliseconds.** The `Turn` guard (`crates/validate/src/s3/body.rs:151-165`) closes the gap between hyper reading the head and the SDK hook running. That was round 1's defect, and it is fixed: with the guard removed, `a_final_piece…` was red 20 of 20 times. The guard cannot close the earlier gap, between the answer landing in the client's socket and hyper reading it. A concrete case: the peer reads the head and the first 4 of 10 bytes, then writes `200` with `x-amz-request-id: early`. The answer sits in the client's receive buffer. The connection task is polled before tokio has marked the socket readable, so it polls the body, takes the last 6 bytes, and the PUT returns `Ok(PutOutcome)`. In my fixture that happened in **419 of 1800 runs (23%)** under 60-way parallel load. Each time the answer was already in the client's kernel buffer while the source still had 6 bytes to give. The brief's Goal (`brief.md:5-7`, "never as a receipt while the caller's source still had bytes to give") and its invariant (`brief.md:71-73`, "before the response arrived") read most naturally as "arrived at the client". The patch narrows "arrived" to "hyper read it" (`s3.rs:155-158`). Closing the extra window needs visibility into the socket, such as a custom connector that checks for unread bytes before each piece. That is the new HTTP seam the brief's Scope (`brief.md:90-95`) and the iteration-1 carry-forward (option b) call a Plan question. Someone has to decide whether this window belongs to the stated limit or breaks the Goal.
- [ ] **Each PUT now costs a thread, a runtime and a fresh TCP connection. At high PUT rates that can turn into client-made availability errors.** The relevant code is `crates/validate/src/s3.rs:137-143` (pool off), `:167-177` and `:341-386` (one thread and runtime per PUT). Measured on loopback: 500 back-to-back 16-byte PUTs took 911 ms with the patch against 448 ms on the base, about twice the cost per small PUT. Every PUT also adds a TCP handshake to its latency and leaves a client socket in `TIME_WAIT`. Arithmetic, not reproduced, because loopback reuses `TIME_WAIT` sockets here (`tcp_tw_reuse=2`): this host has 28,232 ephemeral ports (32768–60999) and `TIME_WAIT` lasts 60 s. So PUTs to one non-loopback gateway endpoint top out near 470/s. Past that, `connect` fails with `EADDRNOTAVAIL`, which the client reports as `S3Error::NoResponse`. That lands in proposal 0017's budgeted `availability` class, an error the client made itself. The `churn` and `listing` scenarios (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:534-535`) ask for very high small-object rates, and PUT latency is reported as evidence (`:26-27`). The brief's own invariant ("the client has closed the connection" after every PUT) forces the per-PUT close, so this is a trade-off at the brief level. The patch documents the cost honestly; a human should confirm it is acceptable before #743 builds scenarios on it.

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Resolve the comparison with #741's unpublished attempts — exact-path merged-history and all 356 closed/merged PR checks found no prior code/test implementation, but those unpublished diffs are absent from the supplied evidence; `brief.md:109`, `pdca-reviewer-854-evidence/prior-art.log:1`, `pdca-reviewer-854-evidence/prior-art-details.log:1`.; `crates/validate/src/s3/body.rs:289-291` (the "request has seen the response → `Poll::Pending`" check) is not covered by any test. With those three lines deleted, all 7 tests still pass (300 runs of the four small tests, 30 runs of the full suite). Even an added `assert_eq!(run.given_at_return, 4)` in the two held tests (`crates/validate/tests/s3_client_upload_peers.rs:811`, `:825`) stays green. The reason: the test's acknowledgement is `Content-Length: 0` (`s3_client_upload_peers.rs:108`). So the SDK request finishes in the same poll that records the answer, and the runtime shuts down before hyper can poll the body again. The check only matters when the response has a body that is still arriving. **Concrete failing case with the check removed** (scratch probe): the peer reads the head and sends `200 OK / Content-Length: 2 / x-amz-request-id: early`, then sends `ok` 200 ms later. The source gives 4 of 10 bytes, then fails once released. The result is `Err(Body(SourceFailed { produced: 4 }))` with **no request id**, which breaks brief scenario 1b ("a source that fails after the acknowledgement … carrying the acknowledgement's request id"). The same setup with an early `403 AccessDenied` whose XML body arrives late gives `SourceFailed`, and the server's 403 is lost. With a source that succeeds, the source is polled after the answer (given 10, not 4). That contradicts the doc claims at `crates/validate/src/s3.rs:163-165`, `crates/validate/src/s3/body.rs:251-254` and `crates/validate/src/s3/error.rs:97-99`. The patched code gets all three right (`AcknowledgedEarly{produced:4, early}`, `Service{403, AccessDenied, early}`, given stays 4), so this is a test gap, not a production bug. Fix: add a held case whose acknowledgement has a non-empty body sent after a delay (ideally one 2xx and one 4xx), and assert `given_at_return == 4` in the held tests.. 11 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
