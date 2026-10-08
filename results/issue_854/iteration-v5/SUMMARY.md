# Result — issue 854 / validate-s3-client-upload-against-misbehaving-peers

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: #852's client trusts the SDK's outcome for an upload. Reproduced on the base
  (`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `df68932`, #852 folded at
  `d9c6225`): with round 4's test and #852's production code, 10 of 11 cases fail by assertion
  (`results/issue_854/iteration-v4/gate-logs/C4-verify.log`; table in
  `iteration-v4/build-notes.md:91-103`):
  - **Early acknowledgement.** A peer that reads the request head and answers `200` gets a
    receipt: 3 MiB of a declared 64 MiB had been given. A source that fails after the answer
    gives `SourceFailed` with no request id.
  - **Upload retained under backpressure.** 512 MiB in 32 MiB pieces, a peer that stops reading
    and sends an early `200`: a receipt after one piece, with the source still alive at return.
  - **Stops reading, never answers.** The typed operation timeout is right, but the source is
    still alive at return.

  Round 4's own receipt rule ("the source gave its whole declared length before the response
  arrived") left two receipts the human ruled wrong at sign-off (`iteration-v4/SUMMARY.md:162-163`):
  - **Held end.** A 10-byte source gave all 10 bytes but had not reported its end when the peer
    answered. Round 4 returns a receipt, yet the peer received **0** body bytes: the SDK's
    aws-chunked layer holds data until a full 64 KiB chunk or the source's end
    (`aws-runtime-1.10.0/src/content_encoding/body.rs:137-170`), so the bytes never reached
    hyper and the final chunk and checksum trailer were never produced.
  - **Overrun.** A source declared at 10 that gives 10, holds until the peer answers, then would
    give 1 more byte (round 4's adversary probe). #852 reports `SourceLength { declared: 10, produced: 11 }`; round 4
    returns a receipt, because the source is not polled after the answer.
- Success criterion: BINDING. The production client (`resolve_config` +
  `S3Client::with_deadlines` + `S3Client::put_object`, the real SDK and hyper connector, real
  loopback TCP) runs against a scripted loopback peer.
  **One lifetime contract, with fixed bounds, applies to every scenario that returns.** The
  test sets the operation timeout to `T_op = 3 s` and wraps each call in a `T_op + 1 s` timeout
  of its own:
  - the call returns within `T_op + 1 s`; not returning is a test failure with a message, never
    a hang;
  - at return, the source has **already been dropped**, observed through the test's own source
    type (a drop flag, not a new API), checked right after the call returns, with no grace
    period;
  - the peer sees its connection **closed** (EOF or reset on its next read) within 1 s of the
    call's **actual return**. The test takes one timestamp when the call returns, hands it to
    the peer, derives one absolute deadline from it, and fails on any close observed after
    that deadline, including an EOF or reset (the round-4 oracle restarted the clock when the
    peer got the signal and accepted any `After(_)`; see Standing decision 5).

  The scenarios:
  1. **No receipt while the source had not ended.** A PUT answered while its source still has
     data to give fails with the body error carrying the answer's request id. So does a source
     that fails after the answer, including when the answer's body arrives after its head (a
     late `200` body, and a late `403 AccessDenied` body, which must come back as that exact
     `S3Error::Service` error).
  2. **Early acknowledgement under backpressure.** The peer reads the head, waits until the
     client's writes back up, sends `200`, keeps the socket open and never reads again; the
     source yields pieces of 32 MiB or more. Outcome: the body error, plus the lifetime contract.
  3. **Stops reading, never answers.** The peer reads the head, stops reading, never answers and
     keeps the socket open; pieces of 32 MiB or more. Outcome: the typed operation timeout, plus
     the lifetime contract.
  4. **The receipt rule (Standing decision 1).** A receipt requires that the source has
     **ended**, meaning it gave its whole declared length and then reported end-of-stream,
     before hyper handed the response over:
     - (a) a 10-byte source that has ended before the peer answers `200` → **receipt** (pins the
       boundary from the receipt side);
     - (b) a 10-byte source that has given all 10 bytes and holds its end when the peer answers
       `200` → **not a receipt**: `BodyError::AcknowledgedEarly` with the answer's request id;
     - (c) a source declared at 10 that gives 10 and holds, and would give 1 more byte if it
       went on → **not a receipt**: `BodyError::AcknowledgedEarly` with the answer's request
       id. The extra byte is never taken, so the overrun cannot be seen; an overrun taken
       *before* the answer is still `SourceLength`, as in #852.

     **The hold in (b) and (c) is keyed to what the client has seen, never to what the peer has
     sent**, so each case has exactly one legal outcome whatever the scheduling. Use round 4's
     `Hold` unchanged (round-4 test `:228-304`, `Source::held_at` `:340`, `Answer::WhenHeld`
     `:420`): the peer answers only after the source has reached its hold (a signal from the
     source to the peer), and the source may go on only once the client has **read** the answer
     off its socket (`Hold::answer_read` `:275-303`: the peer's send queue drained, then the
     client's receive queue empty, read in that order from `/proc/net/tcp`), never just because
     the peer wrote it. hyper reads the answer and hands it to the request in one poll of its
     connection task, and with round 4's `Turn` (`s3/body.rs:151-198`) the body takes nothing
     from the source after that. So a source held this way is never polled past its hold while
     the call runs: it neither ends nor gives the extra byte before the response is handed over
     (where `/proc/net/tcp` cannot be read it never goes on at all, which changes nothing). A
     release keyed to the peer's write would instead let the source end, or overrun, in the gap
     between the answer reaching the client's receive buffer and hyper reading it, which is the
     stated limit's window and is not tested. Both (b) and (c) assert their fixture by name, as
     round 4's held cases do: `assert_parked_before_answer` (`:843`) and
     `assert_not_polled_past_the_hold` (`:880`, with `at = 10`).
  5. **Dropped mid-write.** A PUT future dropped while its connection is blocked mid-write
     releases the source and the connection within 1 s of the drop.

  **The stated limit, documented on `put_object`, not tested as a failure.** The client sees
  what it handed to the HTTP stack and the moment hyper reads the answer off the socket. It
  cannot see whether the peer read the bytes it handed over, and it cannot see an answer that
  has reached its own kernel receive buffer but that hyper has not yet read. The second is a
  real window: round 4's adversary measured a receipt in 419 of 1800 runs of a forcing fixture
  under 60-way parallel load (`iteration-v4/SUMMARY.md:176`). Closing it needs access to the
  socket, which the SDK's connector does not expose (`aws-smithy-http-client-1.4.2/src/client.rs:208`,
  `wrap_connector` is `pub(crate)`), so it is part of the stated limit (Standing decision 3).
  **The only backstop is partial and not built yet, so the doc must not promise detection.**
  Nothing drives the client today (`crates/validate/src/lib.rs:5-9`; the binary tells the
  operator nothing was validated, `:118-123`); the scenarios arrive with #743. Once they do,
  proposal 0017's oracle catches a false receipt only on a **single-writer** key that is read
  back before its next successful overwrite: the read then hashes to a prior generation, or
  `404`s, a fatal `integrity` or `oracle` failure
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:403-410`, taxonomy `:485-486`).
  It does not catch every one. On the **contention pool** a read may return any value written to
  the key, and quiesce may settle on any of them (`:413-422`, `:435-438`), so a concurrent PUT
  the server never got can leave another permitted value and no mismatch. A single-writer write
  that is overwritten before anything reads it back leaves no trace either. The `put_object` doc
  states the window and names the read-back check as a future, conditional safeguard in those
  terms, with no claim that every false receipt is caught. No test asserts on that window in
  either direction.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the PUT path's reported outcome and its resource lifetime, against a peer that
  answers early or stops reading, with or without answering, and on every way the call ends.
  In this revision:
  - the receipt rule becomes "the source ended before the response arrived" (scenario 4), and
    every place that states the rule says so: the `put_object` doc (round 4 `s3.rs:155-165`),
    the `AcknowledgedEarly` doc in `s3/error.rs`, and the architecture paragraph; the
    `put_object` doc also states the limit and its partial, future backstop exactly as the
    success criterion words it;
  - the `put_object` doc and the architecture paragraph state the per-PUT cost with its
    consequence: each PUT uses a fresh TCP connection that the client closes, so each leaves a
    client socket in `TIME_WAIT`, which caps sustained PUTs per client address and gateway
    address at about (ephemeral ports ÷ `TIME_WAIT` seconds), about 470/s with Linux defaults
    (28,232 ports, 60 s). Past that, `connect` fails and the client reports
    `S3Error::NoResponse`, which proposal 0017's taxonomy would count against the server as
    `availability`. Also say that this is a host default, not a hard ceiling: the cap is per
    client-address and gateway-address pair, shared by every worker on that client, and an
    operator raises it with more client or gateway addresses, a wider
    `net.ipv4.ip_local_port_range`, or `net.ipv4.tcp_tw_reuse = 1` (Linux's default `2` reuses
    ports on loopback only). Say this plainly; give no issue number;
  - the connection-close oracle fix (Standing decision 5), and the test changes scenario 4 needs.

  Constraints: work within the SDK's public connector API and the dependencies `crates/validate`
  already has. Reusing connections across PUTs is **not** required in this slice (Standing
  decision 2). The `// deferred: #854` marker #852 left (base `crates/validate/src/s3.rs:148-149`)
  is removed.
  **/ out of scope:** a custom HTTP connector or any new HTTP seam, and with it connection
  pooling for PUTs and closing the kernel-buffer window (a follow-up the human files, Standing
  decision 2); seeded Tier-0 DST coverage (Standing decision 4); response classification and
  byte budgets (#853); GET; the 5 GiB property (#761); TLS; `.cargo/mutants.toml` (the two
  surviving `ResponseArrival::name` mutants are equivalent and the C5 gate is advisory); any
  edit outside `crates/validate/` except the one paragraph in
  `docs/design/architecture/05-building-block-view.md` (line 255 on the base), which
  `AGENTS.md:154` ("Docs currency … a merge requirement") requires.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — PASS on confirm — first run failed transiently: xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit stat
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 49 mutants tested in 5m: 2 missed, 7 caught, 36 unviable, 4 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.12s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #854: require source EOF before a PUT receipt and release upload resources when the call ends; the behavior is reproduced, with one fixable connection-close oracle gap.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The receipt boundary, lifetime bounds, and accepted observation limits are explicit and mutually consistent (brief.md:37; brief.md:242; crates/validate/src/s3.rs:154). |
| C2 Reproduction (red pre-fix) | PASS | Stashing production changes while retaining the test produced 11 assertion failures and the declared green-only cancellation case; held EOF returned a receipt and held overrun returned SourceLength (reviewer-red.log:30; reviewer-red.log:52; reviewer-red.log:93). |
| C3 Change | PASS | The patch stays within the three production files, new test, and permitted architecture paragraph; the documented receipt rule and conditional future backstop match the brief (crates/validate/src/s3.rs:175; docs/design/architecture/05-building-block-view.md:255). |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 12 peer tests and all 45 validate tests; the frozen full CI pass supplements the sandbox-limited CI rerun, with no diff-coverage claim (reviewer-restored-green.log:76; gate-logs/C4-ci.log:5310; reviewer-ci-pinned.log:3373). |
| C5 Causal adequacy | PASS | Source EOF at response observation determines receipt eligibility, and runtime teardown releases the blocked connection owner; this addresses the causes without a capability-probe workaround (crates/validate/src/s3/body.rs:126; crates/validate/src/s3/body.rs:337; crates/validate/src/s3.rs:403). |
| T1 Structure | PASS | The upload lifecycle remains private to the existing S3 adapter and uses existing dependencies and the public SDK connector API (crates/validate/src/s3.rs:135; crates/validate/src/s3.rs:372). |
| T2 Shape | PASS | The new public body error carries the response request id, its EOF boundary is documented, and the living architecture reflects the behavior and cost (crates/validate/src/s3/error.rs:95; docs/design/architecture/05-building-block-view.md:255). |
| T3 Runtime | PASS | Actual loopback runs cover backpressure, operation expiry, delayed response bodies, and abandonment with bounded release; the accepted thread/connection cost is stated (crates/validate/tests/s3_client_upload_peers.rs:881; crates/validate/tests/s3_client_upload_peers.rs:1231; crates/validate/src/s3.rs:199). |
| T4 Contribution | FAIL | The two unresolved batch findings share the observation gap below; contribution-artifact checking is N/A until its mandatory publish rerun (gate-logs/T4-batch-review.log:10; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN [impl] | Require socket observation for the close and post-answer polling claims, or report them unsupported — missing observation can silently weaken the regression oracle (crates/validate/tests/s3_client_upload_peers.rs:727; crates/validate/tests/s3_client_upload_peers.rs:904). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Judge fitness within the already accepted limits and inspect the separately required round-4 red evidence — supplied gate logs prove the comparison against #852 only (brief.md:132; crates/validate/src/s3.rs:165). |

Source citations are relative to the supplied `$PDCA_TARGET`; evidence-log citations are relative to this review directory. The target matches the supplied patch: reverse-apply checking succeeded, and the production changes were restored after the red leg. No builder notes were read.

**The connection-close oracle can report success without its essential observation.** When the TCP table is unreadable, `wait_client_close` returns `Unobservable` (crates/validate/tests/s3_client_upload_peers.rs:727), which the assertion accepts as true (:904). The peer then drains (:650). That drain can release backpressure and let a connection finish, so subsequent timely EOF does not establish that the client closed while the peer remained non-reading. Require the observation capability or explicitly report this case unsupported; do not count it as a verified close. This is a test implementation finding, not evidence of a production failure. The related acceptance of unobserved backpressure at :1112 and :1265 should follow the same evidence policy.

**The held-source batch finding is another instance of the same evidence gap.** An unreadable TCP table keeps `Hold::answer_read` false (crates/validate/tests/s3_client_upload_peers.rs:319), so the source remains pending and `went_on` stays false even if it is polled after the answer. The assertion at :969 therefore cannot prove its post-answer polling claim on that fallback. Preserve Hold as the brief requires (brief.md:86); enforce observation availability or identify unsupported assertions around it, rather than changing its release ordering. The early-receipt boundary alone remains testable with a permanently held source. This limitation did not invalidate this host's red→green: a live probe found both connection endpoints (reviewer-proc.log:1), and the red leg actually advanced held sources, producing both the extra byte and post-answer source failures (reviewer-red.log:24; reviewer-red.log:30). Both batch findings are consolidated under T5. The recorded DST rejection remains settled by brief.md:258; no new DST finding is raised.

**Independent checks support the production change, with explicit limits on the remaining evidence.**

- `cargo test --offline -p wyrd-validate --test s3_client_upload_peers`, with production changes stashed and then restored, used the real Rust compiler, SDK, hyper connector, loopback sockets, and source-drop observations. The base failed 11 of 12 tests by assertion; the restored patch passed 12 of 12. The broader crate run passed 19 CLI tests, 14 existing client tests, and 12 new peer tests (reviewer-restored-green.log:38; :58; :76).
- The pinned-toolchain CI rerun passed typos, docs lint/render, repository guards, formatting, workspace clippy/build/tests, and cargo-machete. It stopped at the read-only advisory-database lock (reviewer-ci-pinned.log:3373), a reviewer-host restriction, not a patch defect. Independent conformance and statics reruns also passed (reviewer-conformance.log:1; reviewer-statics.log:3). An initial CI attempt used the default toolchain without rustfmt; the pinned rerun supersedes that attempt.
- The frozen C4-ci log records a complete pass, including the dependency checks and DST steps (gate-logs/C4-ci.log:4706; :4965; :5310). The TiKV feature check likewise compiled both requested selections (gate-logs/host-tikv.log:209). These are captured gate results, not claims that I independently completed those checks. No fix-specific external dependency was absent from the actual loopback verification.
- Diff coverage was not measured because that gate applied the stacked patch to origin/main (gate-logs/C4-diff-cov.log:10). This is a base-selection caveat, not a compile/apply defect in the supplied target. The mutation log reports 7 caught, 4 timeouts, 36 unviable, and 2 missed mutants; the two misses only change the interceptor name and are the equivalents already identified in scope (gate-logs/C5-mutants.log:13; :19; crates/validate/src/s3.rs:431).
- T4-contribution is **N/A**: the artifacts are intentionally drafted after Check, and the substantive audit is mandatory at publish (gate-logs/T4-contribution.log:10). Their absence requires no human clearance.
- The extra red run against unchanged round-4 production is required in withheld build notes, but appears in none of the supplied gate logs. I neither affirm it nor infer that it was omitted; sign-off must inspect that separate evidence (brief.md:132).

**The published prior-art check was independently repeated by every affected path.** Main has no history for the four upload code/test paths. File lists for all 368 PRs, including closed PRs, identify only open #859 and #860 for those paths; these are the base and stacked work named in the brief (reviewer-prior-art-history.jsonl:1; reviewer-prior-art-pr-files.jsonl:1). The architecture path has 15 main-history commits and 25 matching PRs; its closed matches are merged documentation or other-subsystem changes, with no rejected alternative for this upload fix (reviewer-prior-art-history.jsonl:5; reviewer-prior-art-doc-prs.jsonl:12). The brief separately records the different paths of #741's unpublished rejected attempts (brief.md:231); those artifacts are not supplied, so that part remains the recorded triage check, not an independent replay.

### Advisory — adversary

# Adversarial review — issue #854 (validate-s3-client-upload-against-misbehaving-peers)

Verdict: **I could not refute the fix.** The receipt rule, the request-before-body ordering
(`Turn`) and the own-runtime lifetime all held up against every attack below. Two findings
remain: a test-fidelity gap (partly already raised by T4) and an overstated lifetime claim in
the docs.

## Evidence re-run

- Green leg re-run in a scratch copy of `$PDCA_TARGET` (`cargo test -p wyrd-validate --test
  s3_client_upload_peers`): 12/12 green, 3 runs in a row, plus **64/64 runs green with 32 copies
  of the test binary running at once**. No flakiness showed up. Red leg taken from the frozen
  `gate-logs/C4-verify.log`: 11/12 red by assertion (not by compile error). The one green is
  scenario 5, which the brief declares green-only. 4(b) is red on the base as a receipt with
  `ended_at_return: true` (the hold released and the source ended), and 4(c) is red as
  `SourceLength { declared: 10, produced: 11 }`. That matches the brief's Falsifiability text.
- The test drives the production path: `resolve_config` → `S3Client::with_deadlines` →
  `put_object`, with the real SDK, hyper and loopback TCP (`tests/s3_client_upload_peers.rs`).
  It does not re-implement production.
- Hand mutant, Turn removed (`crates/validate/src/s3/body.rs:201-208`, `body_waits` made to
  always return `false`): **7 tests fail** (all held cases plus the late-403 case). So the
  tests guard the ordering mechanism, and it is not decoration.
- SDK and hyper claims the fix depends on, checked against the locked crate versions:
  `read_after_transmit` runs with no await after the connector future resolves
  (`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:483-511`). The upload-throughput
  wrapper does nothing extra when stalled-stream protection is off (`minimum_throughput.rs:395`).
  The connector's async block does no await after hyper's response (`aws-smithy-http-client-1.4.2/src/client.rs:615-625`).
  hyper-util drops the pooled sender in the same step when the pool is off
  (`hyper-util-0.1.20/src/client/legacy/client.rs:357-358`). The aws-chunked layer polls the
  inner body until it ends (`aws-runtime-1.10.0/src/content_encoding/body/http_body_1_x.rs:56-83`).
- I did not verify the "second red against round 4's production". It lives in the withheld
  `build-notes.md`, and I was not given round 4's patch. This is not a refutation.

## Findings

- NEEDS-HUMAN [impl] — **Where `/proc/net/tcp` can't be read, four fixture checks pass
  without testing anything, not two.** T4 already flagged the hold
  (`crates/validate/tests/s3_client_upload_peers.rs:319`) and the close check (`:904`,
  `ClientClose::Unobservable => true`). The same pattern appears twice more:
  `BackedUp::Unobservable` is accepted as "the client backed up" in scenario 2 (`:1112`) and
  scenario 5 (`:1265`). On such a host, `wait_backed_up` (`:693-697`) just sleeps 1 s and
  answers, so the backpressure tests pass even if the client never backed up. And because the
  hold never releases, `assert_not_polled_past_the_hold` (`:969`) passes for a client that
  keeps polling the body after the answer. Concrete failing case: run the suite in a sandbox
  that hides `/proc/net` against a fix that drops the `upload.answer.get().is_some()` check
  (`src/s3/body.rs:303`). The late-body tests still go green, because their source never gets
  the chance to go on. Fix: at the top of each test that depends on it, fail with a message (or
  skip loudly) when `tcp_table_readable()` (`:757`) is false, instead of degrading in four
  separate places. CI on Linux can read the table, so this is about the test being honest, not
  a live red.

- NEEDS-HUMAN [human] — **"The connection closed" and "no PUT outlives its call" overstate
  what happens on the abnormal paths.** The client calls `close()`, but against a peer that
  has stopped reading, the kernel keeps the connection alive with the upload's unsent bytes
  queued. Concrete probe (scratch test, not part of the patch): a 512 MiB source in 32 MiB
  pieces, a peer that reads the head and then never reads, answers or drains, `T_op` = 2 s.
  The call returns `Err(Timeout { phase: Operation, .. })`. Afterwards the client's socket in
  `/proc/net/tcp` is **state `04` (FIN_WAIT1) with `tx_queue` = 2,595,178 bytes at +0 ms,
  +100 ms, +1 s, +5 s and +15 s** after the return. If the peer starts reading again, it gets
  those bytes and then EOF. If the source had ended before the deadline, the queue holds the
  complete aws-chunked body, trailer included, so the object can be stored **after**
  `put_object` returned `Timeout`. (A timeout is indeterminate anyway, so the oracle is not
  misled. The literal claim is still wrong.) The test contract accepts this by design: the
  socket only has to leave ESTABLISHED, and the peer then drains whatever was queued (test
  `:16-22`, `:495-497`). So the brief's success criterion is met. But the wording at
  `crates/validate/src/s3.rs:18` ("neither the source nor the connection left behind"),
  `:188-189` ("the connection closed, whatever the server did") and
  `docs/design/architecture/05-building-block-view.md:255` ("No PUT outlives its call") reads
  stronger than that. The cost paragraph (`s3.rs:198-209`) also names only `TIME_WAIT`, not
  the abnormal-path orphan socket, which holds up to a send buffer of kernel memory for each
  stalled PUT until the peer reads or the kernel's orphan-probe limit aborts it. Under #743's
  stalled-gateway scenarios that adds up per worker. Making the close abortive (`SO_LINGER 0`)
  needs socket access, which Standing decision 2 puts out of scope. So the human's call is:
  reword the claim and add one sentence on the orphan cost, or accept it as is.

- (Advisory, no action required) **The `tokio::task::unconstrained` wrapper at
  `src/s3/body.rs:112` has no test.** I removed it in scratch (field type `Pin<Box<F>>`,
  construction `Box::pin(request)`) and got 12/12 green, 3 runs. The reasoning in
  `body.rs:224-227` holds up: when tokio's per-task work budget runs out, a ready response
  reads as pending and its wake is put off, which would let the body run ahead of the
  recorded answer. So keep the wrapper. A small unit test of `RequestFirst` with an inner
  future that uses up the budget (`tokio::task::coop::consume_budget`) would pin it. I'm not
  raising this as a rebuild item.

## Attacks tried that did not land

- **False `AcknowledgedEarly` against a correct server.** The SDK wraps the streaming body in
  aws-chunked with a checksum trailer under this config: checksum calculation is on by
  default and there's no user checksum header (`aws-sdk-s3-1.148.0/src/http_request_checksum.rs:183-199`,
  `:230-242`). The trailer is only written after the inner body reports its end, which is
  exactly when `ended` is set (`body.rs:337`). So a correct server cannot answer before
  `ended`. This only holds while aws-chunked stays on. A future
  `request_checksum_calculation(WhenRequired)` or a caller-set checksum header would switch to
  plain `Content-Length`, and the reasoning would need another look. Nothing in this diff
  does that.
- **Zero-length PUT never reaching `ended`.** `empty_object_round_trips` passes against the
  real in-process gateway in `gate-logs/C4-ci.log:4204`.
- **Source failure vs. early success, which one wins** (`s3.rs:229-238`). Both can't happen in
  one exchange. A body error makes hyper fail the exchange before any response is handed
  over, and once a response is handed over, Turn and the answer check stop the source from
  being polled.
- **Abandon path.** The sender is a named binding, `_abandon`, not `_` (`s3.rs:379`), so it
  lives until the caller's future drops. `future::select` polls the request first, so a
  request that finishes as the caller drops just sends to a closed channel. If the thread
  can't be spawned, std drops the closure, and `request` with it, before `spawn` returns.
- **100-continue reordering.** The SDK never sends `Expect: 100-continue` (no match in
  `aws-sdk-s3-1.148.0/src`).
- **Panic paths.** A source that panics inside hyper's connection task is caught by tokio,
  same as before the patch. A panic in the request future itself panics the caller through
  `s3.rs:415`, which matches the old behaviour.
- **Gate attribution.** C4-ci's first-attempt failure is in `crates/gateway-s3/src/lib.rs:4259`,
  which this diff doesn't touch. C4-diff-cov measured nothing because the bundle is stacked
  on #852 and doesn't apply on `origin/main`: there is no coverage evidence, but that is not a
  defect. The C5 misses are the two `ResponseArrival::name` mutants the brief rules
  equivalent. The four Turn mutants are caught by hanging tests (42 s timeouts), not by
  assertions.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_upload_peers.rs:904`: `ClientClose::Unobservable` counts as successful closure. When `/proc/net/tcp` is unavailable, `wait_client_close` returns immediately and `serve` starts draining the socket (`:649-650`). Draining can unblock a writer that survived the call, allowing it to finish and close within the one-second bound; the test then passes despite the connection requiring peer cooperation to close. Require the closure-observation capability before running these cases, or explicitly report them as unsupported instead of accepting this branch as proof. This corroborates the second finding in the frozen T4 review log; it does not invalidate the recorded runs on hosts where observation succeeded.

No additional production correctness or reuse/simplification findings within the brief's accepted scope. Review used the target source and frozen gate evidence; no checks were rerun and no target files were changed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Require socket observation for the close and post-answer polling claims, or report them unsupported — missing observation can silently weaken the regression oracle (crates/validate/tests/s3_client_upload_peers.rs:727; crates/validate/tests/s3_client_upload_peers.rs:904).
- [ ] Validation — fitness-to-purpose — Judge fitness within the already accepted limits and inspect the separately required round-4 red evidence — supplied gate logs prove the comparison against #852 only (brief.md:132; crates/validate/src/s3.rs:165).
- [ ] **Where `/proc/net/tcp` can't be read, four fixture checks pass without testing anything, not two.** T4 already flagged the hold (`crates/validate/tests/s3_client_upload_peers.rs:319`) and the close check (`:904`, `ClientClose::Unobservable => true`). The same pattern appears twice more: `BackedUp::Unobservable` is accepted as "the client backed up" in scenario 2 (`:1112`) and scenario 5 (`:1265`). On such a host, `wait_backed_up` (`:693-697`) just sleeps 1 s and answers, so the backpressure tests pass even if the client never backed up. And because the hold never releases, `assert_not_polled_past_the_hold` (`:969`) passes for a client that keeps polling the body after the answer. Concrete failing case: run the suite in a sandbox that hides `/proc/net` against a fix that drops the `upload.answer.get().is_some()` check (`src/s3/body.rs:303`). The late-body tests still go green, because their source never gets the chance to go on. Fix: at the top of each test that depends on it, fail with a message (or skip loudly) when `tcp_table_readable()` (`:757`) is false, instead of degrading in four separate places. CI on Linux can read the table, so this is about the test being honest, not a live red.
- [ ] **"The connection closed" and "no PUT outlives its call" overstate what happens on the abnormal paths.** The client calls `close()`, but against a peer that has stopped reading, the kernel keeps the connection alive with the upload's unsent bytes queued. Concrete probe (scratch test, not part of the patch): a 512 MiB source in 32 MiB pieces, a peer that reads the head and then never reads, answers or drains, `T_op` = 2 s. The call returns `Err(Timeout { phase: Operation, .. })`. Afterwards the client's socket in `/proc/net/tcp` is **state `04` (FIN_WAIT1) with `tx_queue` = 2,595,178 bytes at +0 ms, +100 ms, +1 s, +5 s and +15 s** after the return. If the peer starts reading again, it gets those bytes and then EOF. If the source had ended before the deadline, the queue holds the complete aws-chunked body, trailer included, so the object can be stored **after** `put_object` returned `Timeout`. (A timeout is indeterminate anyway, so the oracle is not misled. The literal claim is still wrong.) The test contract accepts this by design: the socket only has to leave ESTABLISHED, and the peer then drains whatever was queued (test `:16-22`, `:495-497`). So the brief's success criterion is met. But the wording at `crates/validate/src/s3.rs:18` ("neither the source nor the connection left behind"), `:188-189` ("the connection closed, whatever the server did") and `docs/design/architecture/05-building-block-view.md:255` ("No PUT outlives its call") reads stronger than that. The cost paragraph (`s3.rs:198-209`) also names only `TIME_WAIT`, not the abnormal-path orphan socket, which holds up to a send buffer of kernel memory for each stalled PUT until the peer reads or the kernel's orphan-probe limit aborts it. Under #743's stalled-gateway scenarios that adds up per worker. Making the close abortive (`SO_LINGER 0`) needs socket access, which Standing decision 2 puts out of scope. So the human's call is: reword the claim and add one sentence on the orphan cost, or accept it as is.
- [ ] `crates/validate/tests/s3_client_upload_peers.rs:904`: `ClientClose::Unobservable` counts as successful closure. When `/proc/net/tcp` is unavailable, `wait_client_close` returns immediately and `serve` starts draining the socket (`:649-650`). Draining can unblock a writer that survived the call, allowing it to finish and close within the one-second bound; the test then passes despite the connection requiring peer cooperation to close. Require the closure-observation capability before running these cases, or explicitly report them as unsupported instead of accepting this branch as proof. This corroborates the second finding in the frozen T4 review log; it does not invalidate the recorded runs on hosts where observation succeeded.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered
- [ ] **The accepted response-window limitation relies on an overstated backstop.** `brief.md:86-89` promises that a body the server never received “still surfaces as a fatal `integrity` failure” and requires that promise in the `put_object` documentation. At `$PDCA_TARGET`, `crates/validate/src/lib.rs:5-9` says no scenario drives the client yet, and `:118-123` explicitly reports that nothing was validated. Even the proposed oracle is narrower: `docs/design/proposals/draft/0017-blackbox-validation-tool.md:403-407` puts the recorded-digest check under single-writer keys; `:413-422` permits concurrent reads to return any value actually written, and `:435-438` permits quiescence to settle on one of those values. A discarded concurrent PUT can therefore leave another permitted value without triggering a digest mismatch. Revise the brief and required documentation to identify the safeguard as future, conditional single-writer/read-back coverage, and acknowledge that it does not guarantee detection of every false receipt. This does not require reopening the accepted connector deferral.
- [ ] **The new receipt tests synchronize on peer transmission, while the contract synchronizes on client observation.** `brief.md:63-65` requires source EOF before hyper hands over the response, but `:68-74` requires `AcknowledgedEarly` when EOF/an extra byte is held until the peer answers; the reproduction explicitly releases the hold after the peer writes `200` (`:160-162`). The brief itself admits that hyper may not yet have read that response (`:78-85`). In that permitted interval, the source can report EOF, making a receipt legal, or yield the extra byte, making `SourceLength` legal. The target adapter records EOF at `crates/validate/src/s3/body.rs:133-135` and rejects an available excess piece at `:118-127`; peer transmission does not order those polls against client response observation. Thus the mandatory exact outcome and claimed red result (`brief.md:94-98`) can depend on scheduling or test the expressly excluded window. Specify a fixture that keeps EOF/the extra byte unavailable until the PUT completes, or explicitly synchronizes with client response observation, then restate the red expectations for that fixture.
- [ ] C1 Spec — Resolve the crate-only scope versus mandatory architecture documentation — the changed PUT contract requires a same-PR living-doc update that the brief forbids; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`.
- [ ] **Every PUT is now a new TCP connection that the client closes, plus a new OS thread and tokio runtime.** `crates/validate/src/s3.rs:137-143` turns connection reuse off for uploads. That is required by this design, since a pooled connection's task dies with its runtime. `s3.rs:331-374` spawns a thread per PUT, and the doc names only the thread as the cost (`s3.rs:172`). Measured: 300 sequential 10-byte PUTs against a keep-alive peer gave 300 accepted connections and 300 client sockets left in `TIME_WAIT`. This host has 28,232 ephemeral ports (`32768-60999`), and `TIME_WAIT` lasts 60 s. For a non-loopback endpoint, where `tcp_tw_reuse=2` does not apply, that caps sustained PUTs at about 470/s per client IP and endpoint address. Past that, `connect` fails with `EADDRNOTAVAIL`. Proposal 0017's gating `endurance` scenario is mostly small objects, many workers, and long runs (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:532-542`). It counts transport failures against the *deployment's* `availability` budget (`:489`), so the validator's own port shortage could be reported as the server failing. PUT latency figures also now include a TCP handshake that GET and DELETE latencies don't. Two calls for a human: is that cost acceptable for the validator? And should the release mechanism be per connection (a custom connector) instead of per runtime?
- [ ] The asserted cause and guaranteed red test are not grounded in the supplied record. `brief.md:8` attributes #741's historical behavior to #852, and `brief.md:32` / `brief.md:38` promise an assertion failure and a compiling red leg. Neither `notes.json` nor `sources/` is present. #852 exists but is still `PLANNED` (`dependency-state.json:2–5`); the resolved target at `36f006db6bf1ab4fe850b0a2698f145bb53b0892` has no `crates/validate/` (workspace membership: `$PDCA_TARGET/Cargo.toml:9–32`). The repository and local `main` resolve correctly, so this is an unavailable implementation prerequisite, not a phantom dependency. Revise the cause and red-test claims as hypotheses conditional on inspecting/reproducing against the folded #852 base, and provide the original tracker evidence before treating the prior attempts or their constraints as verified.
- [ ] The success exception permits behavior the goal forbids. `brief.md:5–6` promises no receipt for bytes not sent, but `brief.md:29–31` grants a receipt whenever the source finished producing, reasoning about bytes “it already wrote.” No evidence establishes that production means completed socket writes; the brief itself describes large pieces and blocked writes (`brief.md:25–26`, `brief.md:45–46`). A final large piece produced before the acknowledgement but still awaiting transport writes is not excluded by the exception. Specify the expected outcome and regression for that case, or narrow the goal/invariant to incomplete source production and explicitly acknowledge the remaining transport uncertainty.
- [ ] The binding lifetime criterion has no fixed deadline and contradicts the invariant. `brief.md:26–28` permits release “within a stated small bound” after return, while `brief.md:40–42` requires nothing to retain the source or connection once the call returns. No numeric grace period or maximum call duration is specified; a call that never returns never reaches the post-return assertion. Choose one lifetime contract, fix the operation and cleanup bounds, and require the test to fail on non-return as well as late source drop or connection closure.
- [ ] A promised failure path is absent from acceptance coverage. The goal and scope include a peer that “stops reading” (`brief.md:5–7`, `brief.md:56–57`), but both binding scenarios require an acknowledgement, including an explicit `200` in the backpressure case (`brief.md:21–25`). Those cases cannot establish cleanup when the peer stops reading and never replies. Add a bounded no-response scenario that checks failure and resource release on the operation timeout, or explicitly narrow the promised behavior to early-acknowledgement paths.
- [ ] C3 Change — Resolve the crate-only scope versus the mandatory architecture update — the public PUT receipt/lifetime contract changed, but its living description is unchanged and the brief forbids editing it; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`, `crates/validate/src/s3.rs:155`.
- [ ] **The receipt boundary is "hyper parsed the head", not "the answer reached the client". Under load the gap between the two is tens of milliseconds.** The `Turn` guard (`crates/validate/src/s3/body.rs:151-165`) closes the gap between hyper reading the head and the SDK hook running. That was round 1's defect, and it is fixed: with the guard removed, `a_final_piece…` was red 20 of 20 times. The guard cannot close the earlier gap, between the answer landing in the client's socket and hyper reading it. A concrete case: the peer reads the head and the first 4 of 10 bytes, then writes `200` with `x-amz-request-id: early`. The answer sits in the client's receive buffer. The connection task is polled before tokio has marked the socket readable, so it polls the body, takes the last 6 bytes, and the PUT returns `Ok(PutOutcome)`. In my fixture that happened in **419 of 1800 runs (23%)** under 60-way parallel load. Each time the answer was already in the client's kernel buffer while the source still had 6 bytes to give. The brief's Goal (`brief.md:5-7`, "never as a receipt while the caller's source still had bytes to give") and its invariant (`brief.md:71-73`, "before the response arrived") read most naturally as "arrived at the client". The patch narrows "arrived" to "hyper read it" (`s3.rs:155-158`). Closing the extra window needs visibility into the socket, such as a custom connector that checks for unread bytes before each piece. That is the new HTTP seam the brief's Scope (`brief.md:90-95`) and the iteration-1 carry-forward (option b) call a Plan question. Someone has to decide whether this window belongs to the stated limit or breaks the Goal.
- [ ] **Each PUT now costs a thread, a runtime and a fresh TCP connection. At high PUT rates that can turn into client-made availability errors.** The relevant code is `crates/validate/src/s3.rs:137-143` (pool off), `:167-177` and `:341-386` (one thread and runtime per PUT). Measured on loopback: 500 back-to-back 16-byte PUTs took 911 ms with the patch against 448 ms on the base, about twice the cost per small PUT. Every PUT also adds a TCP handshake to its latency and leaves a client socket in `TIME_WAIT`. Arithmetic, not reproduced, because loopback reuses `TIME_WAIT` sockets here (`tcp_tw_reuse=2`): this host has 28,232 ephemeral ports (32768–60999) and `TIME_WAIT` lasts 60 s. So PUTs to one non-loopback gateway endpoint top out near 470/s. Past that, `connect` fails with `EADDRNOTAVAIL`, which the client reports as `S3Error::NoResponse`. That lands in proposal 0017's budgeted `availability` class, an error the client made itself. The `churn` and `listing` scenarios (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:534-535`) ask for very high small-object rates, and PUT latency is reported as evidence (`:26-27`). The brief's own invariant ("the client has closed the connection" after every PUT) forces the per-PUT close, so this is a trade-off at the brief level. The patch documents the cost honestly; a human should confirm it is acceptable before #743 builds scenarios on it.
- [ ] T5 Judgment — Decide whether loopback-only concurrency coverage is an approved exception for this standalone validator — the new worker/cancellation and request/body scheduling paths have no seeded Tier-0 test despite the standing requirement; `AGENTS.md:188`, `crates/validate/src/s3.rs:349`, `crates/validate/src/s3/body.rs:230`, `crates/validate/tests/s3_client_upload_peers.rs:894`.
- [ ] `crates/validate/src/s3.rs:131-143` and `:341-386` (no idle pool for PUTs, plus one OS thread and one runtime per PUT) change the validator's load profile. The docs say only "each one leaves a client socket in `TIME_WAIT`" (`s3.rs:175-177`, `docs/design/architecture/05-building-block-view.md:255`). Measured on this host: 2,000 sequential 1 KiB PUTs against a keep-alive loopback peer ran at 17,284 PUT/s over 1 TCP connection on base, and at 3,055 PUT/s over 2,000 connections with the patch, leaving 2,000 client sockets in `TIME_WAIT`. Here `ip_local_port_range` is `32768 60999` (28,232 ports), `TIME_WAIT` lasts 60 s, and `tcp_tw_reuse=2` (port reuse on loopback only). So a validator PUTting to one non-loopback gateway address tops out near **~470 new connections/s** before `connect()` starts failing with `EADDRNOTAVAIL`, and every concurrent PUT holds an OS thread. Clean receipts pay this too, not just the misbehaving-peer paths. The brief's goal does ask that "the client has closed the connection" on every return, so this follows from the brief. A human should still decide whether the throughput scenarios coming in #743 can live with this ceiling, or whether the close should apply only to early-ack and timeout outcomes, which would need a different runtime or connector design (a Plan question per the brief's Scope).

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — T5 Judgment — Require socket observation for the close and post-answer polling claims, or report them unsupported — missing observation can silently weaken the regression oracle (crates/validate/tests/s3_client_upload_peers.rs:727; crates/validate/tests/s3_client_upload_peers.rs:904).; **Where `/proc/net/tcp` can't be read, four fixture checks pass without testing anything, not two.** T4 already flagged the hold (`crates/validate/tests/s3_client_upload_peers.rs:319`) and the close check (`:904`, `ClientClose::Unobservable => true`). The same pattern appears twice more: `BackedUp::Unobservable` is accepted as "the client backed up" in scenario 2 (`:1112`) and scenario 5 (`:1265`). On such a host, `wait_backed_up` (`:693-697`) just sleeps 1 s and answers, so the backpressure tests pass even if the client never backed up. And because the hold never releases, `assert_not_polled_past_the_hold` (`:969`) passes for a client that keeps polling the body after the answer. Concrete failing case: run the suite in a sandbox that hides `/proc/net` against a fix that drops the `upload.answer.get().is_some()` check (`src/s3/body.rs:303`). The late-body tests still go green, because their source never gets the chance to go on. Fix: at the top of each test that depends on it, fail with a message (or skip loudly) when `tcp_table_readable()` (`:757`) is false, instead of degrading in four separate places. CI on Linux can read the table, so this is about the test being honest, not a live red.; `crates/validate/tests/s3_client_upload_peers.rs:904`: `ClientClose::Unobservable` counts as successful closure. When `/proc/net/tcp` is unavailable, `wait_client_close` returns immediately and `serve` starts draining the socket (`:649-650`). Draining can unblock a writer that survived the call, allowing it to finish and close within the one-second bound; the test then passes despite the connection requiring peer cooperation to close. Require the closure-observation capability before running these cases, or explicitly report them as unsupported instead of accepting this branch as proof. This corroborates the second finding in the frozen T4 review log; it does not invalidate the recorded runs on hosts where observation succeeded.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b. 15 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
