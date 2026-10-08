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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 49 mutants tested in 5m: 2 missed, 7 caught, 36 unviable, 4 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.16s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #854: reject PUT acknowledgements received before source EOF, and release the source and connection when the upload call ends.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted boundary and lifetime bounds are falsifiable, with connection cost, the kernel-buffer window, and DST scope explicitly settled; `brief.md:34`, `brief.md:242`, `brief.md:258`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Confirm the separately required red against unchanged round-4 production — #852 red is independently reproduced, but neither that round-4 tree nor its second-red output is supplied; `brief.md:132`, `reviewer-rerun.log:364`. |
| C3 Change | PASS | The changes stay within the authorized PUT behavior, regression fixture, and living architecture paragraph; the receipt rule and conditional future backstop agree with the brief; `crates/validate/src/s3.rs:155`, `crates/validate/src/s3.rs:178`, `docs/design/architecture/05-building-block-view.md:255`. |
| C4 Verification (red→green) | PASS | Independent stash/restore rerun changes 11 assertion failures into 12 passes; frozen CI supports the remaining checks after the local advisory-database permission failure; `reviewer-rerun.log:364`, `reviewer-rerun.log:404`, `reviewer-ci.log:3186`, `gate-logs/C4-ci.log:3383`. |
| C5 Causal adequacy | PASS | The tests distinguish source EOF from byte-count completion and exercise response ordering plus teardown under backpressure; the fix addresses those causes within the accepted visibility limit; `crates/validate/src/s3/body.rs:121`, `crates/validate/src/s3.rs:403`, `crates/validate/tests/s3_client_upload_peers.rs:1363`. |
| T1 Structure | PASS | Per-upload state and lifecycle ownership remain inside the existing client, using the SDK's public interceptor and connector APIs without changing dependency direction; `crates/validate/src/s3/body.rs:84`, `crates/validate/src/s3.rs:137`, `crates/validate/src/s3.rs:434`. |
| T2 Shape | PASS | The source-ended rule, error contract, and operating limits are consistent across implementation and documentation; formatting, Clippy, and docs checks passed locally; `crates/validate/src/s3/error.rs:95`, `docs/design/architecture/05-building-block-view.md:255`, `reviewer-ci.log:5`, `reviewer-ci.log:21`. |
| T3 Runtime | PASS | Real TCP tests cover early success, delayed success/error bodies, timeout, and cancellation; the accepted per-PUT thread/connection cost is documented rather than hidden; `reviewer-rerun.log:390`, `crates/validate/src/s3.rs:198`, `brief.md:245`. |
| T4 Contribution | NEEDS-HUMAN | Confirm the recorded exclusion of unpublished #741 attempts — all 368 public PRs were checked by affected path, but those rejected local patches are absent, so their comparison rests on the brief's account; `brief.md:231`, `reviewer-prior-art-paths.log:3`, `reviewer-prior-art-paths.log:372`. |
| T5 Judgment | PASS | The revised oracle no longer treats unavailable socket observation as success: it proves observation before each case, requires the actual socket to have been seen open, and measures closure from actual return; `crates/validate/tests/s3_client_upload_peers.rs:559`, `crates/validate/tests/s3_client_upload_peers.rs:887`, `crates/validate/tests/s3_client_upload_peers.rs:1060`, `crates/validate/tests/s3_client_upload_peers.rs:1076`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this bounded client guarantee is sufficient for the intended validation workload — live tests pass, but server consumption remains unknowable, read-back protection is future and conditional, and high-rate use awaits the already-agreed connection redesign; `crates/validate/src/s3.rs:168`, `crates/validate/src/s3.rs:178`, `brief.md:249`. |

No grounded implementation defect was found. The independent run confirms the base regression and patched behavior; the outstanding judgments concern unavailable historical evidence and fitness for the intended workload, not a demonstrated code failure. This report is advisory and does not gate acceptance.

All repository source citations above refer to the supplied `$PDCA_TARGET` (`target/`). It is a self-contained base snapshot identified as `df68932`, with the patch applied; it compiled and ran successfully. The tracked changes were stashed while retaining the new test, then restored before the green run. The patch was not edited. Builder notes were not read.

The strongest evidence is an actual red-to-green run. `cargo test --locked --offline -p wyrd-validate --test s3_client_upload_peers` produced 11 assertion failures and one pass before the fix, then 12 passes after restoration (`reviewer-rerun.log:364`, `reviewer-rerun.log:404`). The held-end case wrongly returned a receipt on the base, while the held-extra-byte case returned `SourceLength`; both now return the required early-acknowledgement error (`reviewer-rerun.log:295`, `reviewer-rerun.log:323`, `crates/validate/tests/s3_client_upload_peers.rs:1173`). The broader CI rerun also passed the 14 existing S3 round-trip tests and all 12 peer tests (`reviewer-ci.log:2694`, `reviewer-ci.log:2712`). Rust, real loopback TCP, and the Linux socket table were exercised without a tool alias or substitute transport.

The frozen gate evidence supports the following dispositions:

- **C4-ci: PASS on frozen evidence; local rerun partial.** The rerun passed typos, docs lint/render, repository guards, formatting, workspace Clippy/build/tests, and cargo-machete. It stopped at the read-only advisory-database lock (`reviewer-ci.log:3186`), a host limitation. The frozen output explicitly records all three cargo-deny checks, conformance, statics, deployment guard, DST, and final success (`gate-logs/C4-ci.log:3382`, `gate-logs/C4-ci.log:3986`). The remaining stages are supported by that log, not claimed as independently rerun.
- **C4-verify: PASS, independently reproduced.** The frozen log also shows 11 assertion failures on the base and 12 green tests (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:105`). Neither this log nor the other supplied logs contains the separately mandated comparison against round-4 production. Sign-off should inspect that comparison's captured test output, without relying on a narrative claim; absence from these inputs does not establish that the builder omitted it.
- **C4-diff-cov: coverage unavailable, base-state caveat.** Its only substantive output is that the patch cannot apply to `origin/main`; it measured no coverage (`gate-logs/C4-diff-cov.log:10`). The dependency is present in the supplied target and the independent build succeeds. This is not a compilation or patch defect.
- **C5-mutants: advisory failure explained.** The only two reported survivors replace the interceptor's diagnostic name; neither changes the upload decision. The remaining results are seven caught, 36 unviable, and four timeouts, so the run is not comprehensive behavioral coverage (`gate-logs/C5-mutants.log:13`, `gate-logs/C5-mutants.log:19`, `crates/validate/src/s3.rs:430`). The brief explicitly settles these name mutants (`brief.md:193`).
- **T4-batch-review: PASS as recorded.** The log reports zero blocking findings and one recorded rejection; it supplies no individual finding text (`gate-logs/T4-batch-review.log:10`). The explicit DST scope decision is respected (`brief.md:258`); no DCO finding or settled deferral is reopened.
- **T4-contribution: N/A.** The artifacts do not yet exist by design; the substantive contribution audit must run at publish (`gate-logs/T4-contribution.log:10`). This deferred row creates no human-clearance request.
- **host-tikv: PASS on frozen compile evidence.** Both requested feature-enabled Clippy commands finish successfully (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). This establishes compilation, not a live TiKV integration run; live TiKV is not a dependency of this patch's loopback verification.

The prior-art check was performed by changed-file path, including closed PRs. All 368 public PRs were enumerated with paginated file lists; only open #859 and #860 touch the affected S3 paths, and the main-branch commits query returned no history for those paths (`reviewer-prior-art-summary.log:1`, `reviewer-prior-art-summary.log:5`). The shared architecture file has unrelated history. The brief additionally records a path comparison against three unpublished #741 attempts, but those artifacts cannot be independently checked here (`brief.md:231`); that specific remaining decision is T4, not a request to repeat the completed public scan.

The capability-probe smell test does not identify a concealed production cause. The new socket-observation preflight belongs to the test oracle and rejects unsupported hosts instead of continuing through a fallback (`crates/validate/tests/s3_client_upload_peers.rs:802`, `crates/validate/tests/s3_client_upload_peers.rs:887`). The production response checks enforce the explicitly specified lifecycle boundary; they do not mask an eager load-time side effect. The accepted kernel-buffer window, connection cost, and exclusion of DST remain settled scope decisions.

### Advisory — adversary

# Adversarial review — issue #854 (round 5)

**Verdict: I could not refute the fix.** I re-ran the proof, mutated the three mechanisms the
fix depends on, put the suite under heavy parallel load, and checked the SDK, hyper and
hyper-util sources the design rests on. All of it held. One judgment call on the cost docs and
one minor doc nit remain. Neither is a defect in the fix.

## Evidence re-run (does not refute)

- `crates/validate/tests/s3_client_upload_peers.rs:1885-2161` — I rebuilt the patched tree in
  scratch and ran the file 5 times: 12/12 green each run, about 3.0 s. Under load (3 rounds of
  32 parallel copies, then 2 rounds of 64) it ran 224 more times with zero failures. The held
  cases do not flake. The frozen C4-verify red leg (`gate-logs/C4-verify.log`) fails 11 tests
  by assertion, not by compile error, and the 12th (scenario 5) passes as declared green-only.
  4(a) is red only on the close check (`client_close: Not`), as the brief declares.
- `crates/validate/src/s3/body.rs:126` — **the brief's "second red", checked independently**
  (`build-notes.md` is withheld from me). I replaced
  `source_ended: self.ended.load(..)` with round 4's rule, `produced == declared`. Exactly
  4(b) and 4(c) go red (`a_source_that_gave_its_whole_length_but_held_its_end_is_not_a_receipt`,
  `a_source_that_gave_its_declared_length_and_held_an_extra_byte_is_not_a_receipt`), and the
  other 10 stay green. So the rule change does real work, and these two tests pin it.
- `crates/validate/src/s3/body.rs:303` — I disabled the stop-after-answer gate
  (`if false && upload.answer…`). The three late-body tests go red through `went_on`.
- `crates/validate/src/s3/body.rs:202` — I disabled the `Turn` ordering (`body_waits` never
  waits). 7 tests go red, and under 32-way load each held test still caught it in 32 of 32
  runs. The fixture's 100 ms wait for the answer flag (`ANSWER_WRITE_WAIT`) does not quietly
  turn the oracle off under load.

## Refutation attempts that failed

- `crates/validate/src/s3.rs:140` — **Cross-runtime sharing between PUTs.** Every PUT runs on
  its own runtime but shares one hyper-util `Client`. `pool_max_idle_per_host(0)` turns
  hyper-util's pool off entirely (`hyper-util-0.1.20/src/client/legacy/pool.rs:115-116`,
  `is_enabled` is `max_idle_per_host > 0`). No idle connection, and no checkout waiter, can be
  handed from a dead runtime to a live one.
- `crates/validate/src/s3.rs:434` (`ResponseArrival::read_after_transmit`) — **The answer is
  recorded late.** `aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:504-511` has no await
  between the connector future resolving and `read_after_transmit`. hyper 1.10.1 polls read
  before write in each loop (`proto/h1/dispatch.rs:173-174`), and the response's wake makes
  `Turn` hold the body until the request has recorded it.
- `crates/validate/src/s3/body.rs:283-330` — **A false `AcknowledgedEarly` for a well-behaved
  server.** This would happen if anything stopped polling the body before its `None`. It
  doesn't: `DeclaredLengthBody` keeps the default `is_end_stream` (false), and the aws-chunked
  layer polls its inner body until end before it can produce the terminal chunk
  (`aws-runtime-1.10.0/src/content_encoding/body.rs:146-174`). #852's 14 round-trip tests
  (full PUTs to the in-process gateway) still pass in `gate-logs/C4-ci.log`.
- `crates/validate/src/s3.rs:51-57` — **The clock rule** (`AGENTS.md` "One clock per
  correctness lifecycle"). A PUT's connect and operation deadlines both sleep on the PUT
  runtime's timer, so one lifecycle uses one source. No test in `crates/validate` uses
  `start_paused`, so moving PUT timers off the caller's runtime breaks no test-controlled time.
- `crates/validate/tests/s3_client_upload_peers.rs:1585-1641` — **The fixture failing open
  where `/proc/net/tcp` is hidden** (round 4's T5 finding). Every `Peer::start` now runs
  `require_socket_observation`, `tcp_table` panics on an unreadable table, and `watched`
  requires `seen_open`. Fixed.

## Findings

- NEEDS-HUMAN [human] — `crates/validate/src/s3.rs:198` ("**The cost, per PUT.** One thread
  for its length…") and `docs/design/architecture/05-building-block-view.md:255` ("The price
  is one thread and one fresh TCP connection per PUT"): **the stated per-PUT cost leaves out
  DNS.** `build_http()` resolves names with `GaiResolver`
  (`aws-smithy-http-client-1.4.2/src/client.rs:263`), which runs `getaddrinfo` on the
  runtime's blocking pool. Each PUT now has a fresh runtime and an unpooled connection. So with
  a hostname `--endpoint` (say `http://gateway.example:8080`), every PUT does its own DNS lookup
  on a second, new thread. At the documented cap of about 470 PUTs/s, that is about 470
  lookups/s per client, or several times that with resolver search domains. A failed lookup
  comes back as `S3Error::NoResponse` (`s3.rs:344`), and a slow one past the connect deadline
  as `Timeout { phase: Connect }` (`s3.rs:298`). Proposal 0017 would count either against the
  server as `availability`. That is the same client-side cost blamed on the server that the
  `TIME_WAIT` paragraph was written to warn about. This only applies to hostname endpoints;
  the repo's deploy examples use IP addresses. The human's options: add a sentence to both
  docs, resolve once through the SDK's public `build_with_resolver` (no new seam), or accept
  it as negligible.
- Minor doc nit, no rebuild needed on its own — `crates/validate/src/s3/error.rs:99`: "until
  the source reports its end the body's final chunk has not been written, **so the server
  cannot hold the object the caller sent**" claims too much. The SDK sends data in 64 KiB
  aws-chunked chunks (`aws-runtime-1.10.0/src/content_encoding.rs:26`). Take a source declared
  at 128 KiB that gives all of it and then holds its end: both data chunks reach hyper, and
  only the terminal chunk and checksum trailer are missing. A lax server can hold every byte.
  The rule itself (Standing decision 1) is unaffected. The sentence could say "the request is
  not complete (no terminal chunk or checksum trailer)". Fold it into any other rebuild.
- Not against the fix: the C4 diff-coverage "fail" (`gate-logs/C4-diff-cov.log`) means
  coverage was never measured, because the patch targets #852's integration branch, not
  `origin/main`. The new lines no test reaches are the thread and runtime start-failure paths
  in `on_own_runtime` (`s3.rs:372-415`) and the "no recorded answer" branch of
  `acknowledged_early` (`body.rs:149-159`). Both are hard-to-reach error paths that fail
  closed (an error, never a receipt). The C5 survivors are the two equivalent
  `ResponseArrival::name` mutants the brief already scopes out, and the four `Turn` timeouts
  are hangs (a detection), not misses.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_upload_peers.rs:1417`: Give the dropped-PUT test an independent timeout while waiting for backpressure. This `select!` waits only for the PUT or the peer's notification; the peer starts its bounded backpressure wait only after accepting and reading the request head (`crates/validate/tests/s3_client_upload_peers.rs:612`, `crates/validate/tests/s3_client_upload_peers.rs:645`). A request-wakeup regression before that point can leave both arms pending indefinitely, including the PUT's own deadline, so the regression test hangs before reaching its drop assertion. The frozen C5 log records 42-second timeouts for the `Turn` wake mutations, consistent with this gap. Wrap this setup wait in a test-runtime timeout covering connection setup and `DROPPED_BACKED_UP_WAIT`, with a diagnostic on expiry.

No additional correctness or reuse/simplification/efficiency findings. Reviewed the target source and frozen gate evidence; no builds were rerun and the target was not modified.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Confirm the separately required red against unchanged round-4 production — #852 red is independently reproduced, but neither that round-4 tree nor its second-red output is supplied; `brief.md:132`, `reviewer-rerun.log:364`.
- [ ] T4 Contribution — Confirm the recorded exclusion of unpublished #741 attempts — all 368 public PRs were checked by affected path, but those rejected local patches are absent, so their comparison rests on the brief's account; `brief.md:231`, `reviewer-prior-art-paths.log:3`, `reviewer-prior-art-paths.log:372`.
- [x] Validation — fitness-to-purpose — Decide whether this bounded client guarantee is sufficient for the intended validation workload — live tests pass, but server consumption remains unknowable, read-back protection is future and conditional, and high-rate use awaits the already-agreed connection redesign; `crates/validate/src/s3.rs:168`, `crates/validate/src/s3.rs:178`, `brief.md:249`.
- [x] `crates/validate/src/s3.rs:198` ("**The cost, per PUT.** One thread for its length…") and `docs/design/architecture/05-building-block-view.md:255` ("The price is one thread and one fresh TCP connection per PUT"): **the stated per-PUT cost leaves out DNS.** `build_http()` resolves names with `GaiResolver` (`aws-smithy-http-client-1.4.2/src/client.rs:263`), which runs `getaddrinfo` on the runtime's blocking pool. Each PUT now has a fresh runtime and an unpooled connection. So with a hostname `--endpoint` (say `http://gateway.example:8080`), every PUT does its own DNS lookup on a second, new thread. At the documented cap of about 470 PUTs/s, that is about 470 lookups/s per client, or several times that with resolver search domains. A failed lookup comes back as `S3Error::NoResponse` (`s3.rs:344`), and a slow one past the connect deadline as `Timeout { phase: Connect }` (`s3.rs:298`). Proposal 0017 would count either against the server as `availability`. That is the same client-side cost blamed on the server that the `TIME_WAIT` paragraph was written to warn about. This only applies to hostname endpoints; the repo's deploy examples use IP addresses. The human's options: add a sentence to both docs, resolve once through the SDK's public `build_with_resolver` (no new seam), or accept it as negligible.
- [ ] `crates/validate/tests/s3_client_upload_peers.rs:1417`: Give the dropped-PUT test an independent timeout while waiting for backpressure. This `select!` waits only for the PUT or the peer's notification; the peer starts its bounded backpressure wait only after accepting and reading the request head (`crates/validate/tests/s3_client_upload_peers.rs:612`, `crates/validate/tests/s3_client_upload_peers.rs:645`). A request-wakeup regression before that point can leave both arms pending indefinitely, including the PUT's own deadline, so the regression test hangs before reaching its drop assertion. The frozen C5 log records 42-second timeouts for the `Turn` wake mutations, consistent with this gap. Wrap this setup wait in a test-runtime timeout covering connection setup and `DROPPED_BACKED_UP_WAIT`, with a diagnostic on expiry.
- [x] **The accepted response-window limitation relies on an overstated backstop.** `brief.md:86-89` promises that a body the server never received “still surfaces as a fatal `integrity` failure” and requires that promise in the `put_object` documentation. At `$PDCA_TARGET`, `crates/validate/src/lib.rs:5-9` says no scenario drives the client yet, and `:118-123` explicitly reports that nothing was validated. Even the proposed oracle is narrower: `docs/design/proposals/draft/0017-blackbox-validation-tool.md:403-407` puts the recorded-digest check under single-writer keys; `:413-422` permits concurrent reads to return any value actually written, and `:435-438` permits quiescence to settle on one of those values. A discarded concurrent PUT can therefore leave another permitted value without triggering a digest mismatch. Revise the brief and required documentation to identify the safeguard as future, conditional single-writer/read-back coverage, and acknowledge that it does not guarantee detection of every false receipt. This does not require reopening the accepted connector deferral.
- [x] **The new receipt tests synchronize on peer transmission, while the contract synchronizes on client observation.** `brief.md:63-65` requires source EOF before hyper hands over the response, but `:68-74` requires `AcknowledgedEarly` when EOF/an extra byte is held until the peer answers; the reproduction explicitly releases the hold after the peer writes `200` (`:160-162`). The brief itself admits that hyper may not yet have read that response (`:78-85`). In that permitted interval, the source can report EOF, making a receipt legal, or yield the extra byte, making `SourceLength` legal. The target adapter records EOF at `crates/validate/src/s3/body.rs:133-135` and rejects an available excess piece at `:118-127`; peer transmission does not order those polls against client response observation. Thus the mandatory exact outcome and claimed red result (`brief.md:94-98`) can depend on scheduling or test the expressly excluded window. Specify a fixture that keeps EOF/the extra byte unavailable until the PUT completes, or explicitly synchronizes with client response observation, then restate the red expectations for that fixture.
- [x] size backstop — this slice is behaving oversized: patch is 102 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] C1 Spec — Resolve the crate-only scope versus mandatory architecture documentation — the changed PUT contract requires a same-PR living-doc update that the brief forbids; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`.
- [x] **Every PUT is now a new TCP connection that the client closes, plus a new OS thread and tokio runtime.** `crates/validate/src/s3.rs:137-143` turns connection reuse off for uploads. That is required by this design, since a pooled connection's task dies with its runtime. `s3.rs:331-374` spawns a thread per PUT, and the doc names only the thread as the cost (`s3.rs:172`). Measured: 300 sequential 10-byte PUTs against a keep-alive peer gave 300 accepted connections and 300 client sockets left in `TIME_WAIT`. This host has 28,232 ephemeral ports (`32768-60999`), and `TIME_WAIT` lasts 60 s. For a non-loopback endpoint, where `tcp_tw_reuse=2` does not apply, that caps sustained PUTs at about 470/s per client IP and endpoint address. Past that, `connect` fails with `EADDRNOTAVAIL`. Proposal 0017's gating `endurance` scenario is mostly small objects, many workers, and long runs (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:532-542`). It counts transport failures against the *deployment's* `availability` budget (`:489`), so the validator's own port shortage could be reported as the server failing. PUT latency figures also now include a TCP handshake that GET and DELETE latencies don't. Two calls for a human: is that cost acceptable for the validator? And should the release mechanism be per connection (a custom connector) instead of per runtime?
- [x] The asserted cause and guaranteed red test are not grounded in the supplied record. `brief.md:8` attributes #741's historical behavior to #852, and `brief.md:32` / `brief.md:38` promise an assertion failure and a compiling red leg. Neither `notes.json` nor `sources/` is present. #852 exists but is still `PLANNED` (`dependency-state.json:2–5`); the resolved target at `36f006db6bf1ab4fe850b0a2698f145bb53b0892` has no `crates/validate/` (workspace membership: `$PDCA_TARGET/Cargo.toml:9–32`). The repository and local `main` resolve correctly, so this is an unavailable implementation prerequisite, not a phantom dependency. Revise the cause and red-test claims as hypotheses conditional on inspecting/reproducing against the folded #852 base, and provide the original tracker evidence before treating the prior attempts or their constraints as verified.
- [x] The success exception permits behavior the goal forbids. `brief.md:5–6` promises no receipt for bytes not sent, but `brief.md:29–31` grants a receipt whenever the source finished producing, reasoning about bytes “it already wrote.” No evidence establishes that production means completed socket writes; the brief itself describes large pieces and blocked writes (`brief.md:25–26`, `brief.md:45–46`). A final large piece produced before the acknowledgement but still awaiting transport writes is not excluded by the exception. Specify the expected outcome and regression for that case, or narrow the goal/invariant to incomplete source production and explicitly acknowledge the remaining transport uncertainty.
- [x] The binding lifetime criterion has no fixed deadline and contradicts the invariant. `brief.md:26–28` permits release “within a stated small bound” after return, while `brief.md:40–42` requires nothing to retain the source or connection once the call returns. No numeric grace period or maximum call duration is specified; a call that never returns never reaches the post-return assertion. Choose one lifetime contract, fix the operation and cleanup bounds, and require the test to fail on non-return as well as late source drop or connection closure.
- [x] A promised failure path is absent from acceptance coverage. The goal and scope include a peer that “stops reading” (`brief.md:5–7`, `brief.md:56–57`), but both binding scenarios require an acknowledgement, including an explicit `200` in the backpressure case (`brief.md:21–25`). Those cases cannot establish cleanup when the peer stops reading and never replies. Add a bounded no-response scenario that checks failure and resource release on the operation timeout, or explicitly narrow the promised behavior to early-acknowledgement paths.
- [x] C3 Change — Resolve the crate-only scope versus the mandatory architecture update — the public PUT receipt/lifetime contract changed, but its living description is unchanged and the brief forbids editing it; `brief.md:97`, `AGENTS.md:154`, `docs/design/architecture/05-building-block-view.md:255`, `crates/validate/src/s3.rs:155`.
- [x] **The receipt boundary is "hyper parsed the head", not "the answer reached the client". Under load the gap between the two is tens of milliseconds.** The `Turn` guard (`crates/validate/src/s3/body.rs:151-165`) closes the gap between hyper reading the head and the SDK hook running. That was round 1's defect, and it is fixed: with the guard removed, `a_final_piece…` was red 20 of 20 times. The guard cannot close the earlier gap, between the answer landing in the client's socket and hyper reading it. A concrete case: the peer reads the head and the first 4 of 10 bytes, then writes `200` with `x-amz-request-id: early`. The answer sits in the client's receive buffer. The connection task is polled before tokio has marked the socket readable, so it polls the body, takes the last 6 bytes, and the PUT returns `Ok(PutOutcome)`. In my fixture that happened in **419 of 1800 runs (23%)** under 60-way parallel load. Each time the answer was already in the client's kernel buffer while the source still had 6 bytes to give. The brief's Goal (`brief.md:5-7`, "never as a receipt while the caller's source still had bytes to give") and its invariant (`brief.md:71-73`, "before the response arrived") read most naturally as "arrived at the client". The patch narrows "arrived" to "hyper read it" (`s3.rs:155-158`). Closing the extra window needs visibility into the socket, such as a custom connector that checks for unread bytes before each piece. That is the new HTTP seam the brief's Scope (`brief.md:90-95`) and the iteration-1 carry-forward (option b) call a Plan question. Someone has to decide whether this window belongs to the stated limit or breaks the Goal.
- [x] **Each PUT now costs a thread, a runtime and a fresh TCP connection. At high PUT rates that can turn into client-made availability errors.** The relevant code is `crates/validate/src/s3.rs:137-143` (pool off), `:167-177` and `:341-386` (one thread and runtime per PUT). Measured on loopback: 500 back-to-back 16-byte PUTs took 911 ms with the patch against 448 ms on the base, about twice the cost per small PUT. Every PUT also adds a TCP handshake to its latency and leaves a client socket in `TIME_WAIT`. Arithmetic, not reproduced, because loopback reuses `TIME_WAIT` sockets here (`tcp_tw_reuse=2`): this host has 28,232 ephemeral ports (32768–60999) and `TIME_WAIT` lasts 60 s. So PUTs to one non-loopback gateway endpoint top out near 470/s. Past that, `connect` fails with `EADDRNOTAVAIL`, which the client reports as `S3Error::NoResponse`. That lands in proposal 0017's budgeted `availability` class, an error the client made itself. The `churn` and `listing` scenarios (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:534-535`) ask for very high small-object rates, and PUT latency is reported as evidence (`:26-27`). The brief's own invariant ("the client has closed the connection" after every PUT) forces the per-PUT close, so this is a trade-off at the brief level. The patch documents the cost honestly; a human should confirm it is acceptable before #743 builds scenarios on it.
- [x] T5 Judgment — Decide whether loopback-only concurrency coverage is an approved exception for this standalone validator — the new worker/cancellation and request/body scheduling paths have no seeded Tier-0 test despite the standing requirement; `AGENTS.md:188`, `crates/validate/src/s3.rs:349`, `crates/validate/src/s3/body.rs:230`, `crates/validate/tests/s3_client_upload_peers.rs:894`.
- [x] `crates/validate/src/s3.rs:131-143` and `:341-386` (no idle pool for PUTs, plus one OS thread and one runtime per PUT) change the validator's load profile. The docs say only "each one leaves a client socket in `TIME_WAIT`" (`s3.rs:175-177`, `docs/design/architecture/05-building-block-view.md:255`). Measured on this host: 2,000 sequential 1 KiB PUTs against a keep-alive loopback peer ran at 17,284 PUT/s over 1 TCP connection on base, and at 3,055 PUT/s over 2,000 connections with the patch, leaving 2,000 client sockets in `TIME_WAIT`. Here `ip_local_port_range` is `32768 60999` (28,232 ports), `TIME_WAIT` lasts 60 s, and `tcp_tw_reuse=2` (port reuse on loopback only). So a validator PUTting to one non-loopback gateway address tops out near **~470 new connections/s** before `connect()` starts failing with `EADDRNOTAVAIL`, and every concurrent PUT holds an OS thread. Clean receipts pay this too, not just the misbehaving-peer paths. The brief's goal does ask that "the client has closed the connection" on every return, so this follows from the brief. A human should still decide whether the throughput scenarios coming in #743 can live with this ceiling, or whether the close should apply only to early-ack and timeout outcomes, which would need a different runtime or connector design (a Plan question per the brief's Scope).
- [x] **"The connection closed" and "no PUT outlives its call" overstate what happens on the abnormal paths.** The client calls `close()`, but against a peer that has stopped reading, the kernel keeps the connection alive with the upload's unsent bytes queued. Concrete probe (scratch test, not part of the patch): a 512 MiB source in 32 MiB pieces, a peer that reads the head and then never reads, answers or drains, `T_op` = 2 s. The call returns `Err(Timeout { phase: Operation, .. })`. Afterwards the client's socket in `/proc/net/tcp` is **state `04` (FIN_WAIT1) with `tx_queue` = 2,595,178 bytes at +0 ms, +100 ms, +1 s, +5 s and +15 s** after the return. If the peer starts reading again, it gets those bytes and then EOF. If the source had ended before the deadline, the queue holds the complete aws-chunked body, trailer included, so the object can be stored **after** `put_object` returned `Timeout`. (A timeout is indeterminate anyway, so the oracle is not misled. The literal claim is still wrong.) The test contract accepts this by design: the socket only has to leave ESTABLISHED, and the peer then drains whatever was queued (test `:16-22`, `:495-497`). So the brief's success criterion is met. But the wording at `crates/validate/src/s3.rs:18` ("neither the source nor the connection left behind"), `:188-189` ("the connection closed, whatever the server did") and `docs/design/architecture/05-building-block-view.md:255` ("No PUT outlives its call") reads stronger than that. The cost paragraph (`s3.rs:198-209`) also names only `TIME_WAIT`, not the abnormal-path orphan socket, which holds up to a send buffer of kernel memory for each stalled PUT until the peer reads or the kernel's orphan-probe limit aborts it. Under #743's stalled-gateway scenarios that adds up per worker. Making the close abortive (`SO_LINGER 0`) needs socket access, which Standing decision 2 puts out of scope. So the human's call is: reword the claim and add one sentence on the orphan cost, or accept it as is.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered

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
- Iteration delta (if iterating): Round 5 is accepted in substance: keep the production code, the docs, and the other eleven tests as they are. Only fix: the dropped-PUT test (crates/validate/tests/s3_client_upload_peers.rs:1417) can hang. Its select! waits only for the PUT or the peer's backpressure notification, and a request-wakeup regression before the peer reads the head leaves both pending forever. Wrap that setup wait in its own test-runtime timeout covering connection setup plus DROPPED_BACKED_UP_WAIT, and fail with a diagnostic message on expiry, never a hang. All 12 tests must still go red pre-fix and green post-fix as in round 5.
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Connection-reuse follow-up (Standing decision 2) must also end the per-upload DNS lookup. No hand-rolled DNS cache in wyrd-validate.
