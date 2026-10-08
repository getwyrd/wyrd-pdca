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
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.17s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #854: reject early S3 upload receipts, release upload resources when the call ends, and bound the dropped-PUT regression test's setup wait.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The latest instruction isolates the remaining hang and requires a test-runtime bound covering connection setup plus backpressure; the earlier receipt and resource-lifetime decisions remain settled (`brief.md:293`; `crates/validate/tests/s3_client_upload_peers.rs:202`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the regression test yields 11 assertion failures and the declared green-only cancellation case; this reproduces incorrect receipts and retained resources (`review-red.log:360`; `brief.md:137`). |
| C3 Change | PASS | The dropped-PUT setup now fails diagnostically after 15 seconds, below its 60-second operation deadline, preserving the cancellation test's purpose and the authorized scope (`crates/validate/tests/s3_client_upload_peers.rs:211`; `crates/validate/tests/s3_client_upload_peers.rs:1437`). |
| C4 Verification (red→green) | PASS | Restoring the patch independently gives 12/12 passing tests; full frozen CI is green, while the independent CI retry stops only at the sandbox's read-only advisory-database lock (`review-restored-green.log:19`; `gate-logs/C4-ci.log:3986`; `review-ci.log:3185`). |
| C5 Causal adequacy | PASS | Lost request wakes now produce the intended 15-second diagnostic instead of an indefinite setup wait; all five ordering/wake mutations are caught, directly exercising the remaining defect (`review-wake-mutants.log:8`; `crates/validate/tests/s3_client_upload_peers.rs:1447`). |
| T1 Structure | PASS | The change retains the existing SDK connector boundary and per-call ownership model, with no new dependency or HTTP seam; the living architecture description covers the public behavior (`crates/validate/src/s3.rs:137`; `crates/validate/src/s3.rs:372`; `docs/design/architecture/05-building-block-view.md:255`). |
| T2 Shape | PASS | Named setup bounds, a compile-time relationship to the operation deadline, and a diagnostic carrying source progress make a stalled setup distinguishable from cancellation failure (`crates/validate/tests/s3_client_upload_peers.rs:202`; `crates/validate/tests/s3_client_upload_peers.rs:215`; `crates/validate/tests/s3_client_upload_peers.rs:1448`). |
| T3 Runtime | PASS | Real SDK/hyper loopback execution exercises source release, socket closure and backpressure; socket observation is required rather than silently weakened, and teardown aborts the peer task (`review-restored-green.log:19`; `crates/validate/tests/s3_client_upload_peers.rs:904`; `crates/validate/tests/s3_client_upload_peers.rs:616`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit must rerun at publish; the supplied batch-review and TiKV compilation rows separately pass on their captured evidence (`gate-logs/T4-contribution.log:10`; `gate-logs/T4-batch-review.log:10`; `gate-logs/host-tikv.log:209`). |
| T5 Judgment | NEEDS-HUMAN | Accept the brief's affected-path inventory for unpublished rejected #741 attempts — repository history and all-state PR checks are independently complete, but those unpublished diffs are absent, so their non-overlap cannot be independently settled here (`review-prior-art.md:15`; `brief.md:231`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this slice for its intended validator use — real loopback evidence supports the bounded upload contract, while the already accepted kernel-buffer visibility limit and per-PUT resource cost remain material to later high-rate scenarios (`crates/validate/src/s3.rs:168`; `crates/validate/src/s3.rs:198`; `brief.md:245`). |

No implementation defect was found. The independent red→green run and targeted wake-loss mutations support the requested timeout fix. The remaining human decisions concern fitness for the intended use and the evidence available for unpublished prior work; they do not gate acceptance in this advisory review.

Source citations above are relative to `$PDCA_TARGET` (`target/` in this bundle); evidence citations are relative to this directory. The disposable target contains the stated #852 base with the patch applied and was restored after the red run. No builder notes were read.

- **The timeout addresses the reported hang.** `cargo test --offline -p wyrd-validate --test s3_client_upload_peers` produced 11 failures without the production fix and 12 passes after restoration. The frozen log agrees (`gate-logs/C4-verify.log:105`). Its “12 test(s) ran red” means 12 executed, not 12 failed: cancellation is explicitly green-only in the brief. Running cargo-mutants against the five `Turn::caught_up`/`Wake for Turn` mutations caught all five. Both removed-wake mutations ended in the new diagnostic at about 15 seconds, before the scanner's 40-second timeout (`pdca-reviewer-854-mutants/mutants.out/log/crates__validate__src__s3__body.rs_line_213_col_9.log:667`; `review-wake-mutants.log:6`).
- **Verification limits are environmental or already scoped.** The independent `cargo xtask ci` passed spelling, docs lint/render, repository guards, formatting, clippy, build, workspace tests and dependency-use scanning before cargo-deny could not acquire its read-only cache lock (`review-ci.log:3185`). The frozen output actually records successful advisory/license checks, conformance and the remaining guards/DST checks (`gate-logs/C4-ci.log:3382`; `gate-logs/C4-ci.log:3986`); this is log-supported evidence, not an independently completed full CI run. Diff coverage was not measured because its wrapper used `origin/main`, which lacks the stacked prerequisite (`gate-logs/C4-diff-cov.log:10`); that is a base-state caveat, not a patch defect. The original mutation log has two equivalent interceptor-name survivors, 36 unviable mutations and four timeouts (`gate-logs/C5-mutants.log:13`); the focused rerun establishes only the five ordering/wake cases, not a replacement full mutation score. The round-4 comparison tree was not supplied, so no independent second-red claim is made.
- **Prior art was checked by affected path.** Exact-path queries against `main` and all 372 PRs, including 19 closed/unmerged PRs, found no merged or closed-PR overlap with the four S3 upload paths. Only open prerequisite #859 and dependent #860 carry the existing client files; the architecture paragraph has older unrelated history. Truncated file lists for two large PRs were fully paginated. The supplied brief records the unpublished rejected #741 attempts, but those diffs are outside the supplied evidence (`review-prior-art.md:3`; `review-prior-art.md:15`). Accepted decisions on connection pooling, the kernel-buffer window, and DST scope are not reopened (`brief.md:245`; `brief.md:258`). The socket capability check fails unsupported tests explicitly; it does not mask a production load-time cause.

### Advisory — adversary

# Adversarial review — issue #854, round 6 (dropped-PUT setup bound)

**Verdict: I tried to refute the fix and could not.** I re-ran the red→green proof myself, applied seven targeted mutants to the production code and the round-6 change, and stress-ran the suite under parallel load. Every attack either failed or was caught by the tests. The one bullet that needs a human is about wording, not about the fix.

## What I re-ran (toolchain present; scratch copy of `$PDCA_TARGET`)

- **Green leg:** 12/12 pass in 3.0 s (`cargo test -p wyrd-validate --test s3_client_upload_peers`).
- **Red leg** (base `s3.rs`, `s3/body.rs`, `s3/error.rs` from HEAD `35f4eb8`, new test kept): 11 fail by assertion and `a_put_dropped_while_blocked_mid_write_releases_the_upload` passes. That matches `gate-logs/C4-verify.log` exactly, and each failure is for the stated reason (receipt, `SourceLength`, `SourceFailed`, source alive at return, or connection not closed for 4(a)).
- **Flakiness:** 36 runs at 12-way parallel load and 96 runs at 48-way parallel load on 32 cores: **132/132 pass**, no flakes.

## Refutation attempts

- **Round-6 change, `crates/validate/tests/s3_client_upload_peers.rs:1434-1457` (bound at `:202-216`).** The attack was to bring back the exact regression the human named: a request wake that never reaches the request. I made `Turn::wake` a no-op (`crates/validate/src/s3/body.rs:212-214`). The dropped-PUT test now **fails at 15 s with the new diagnostic** ("the PUT never got as far as the drop … had given 0 of 536870912 bytes and had not been dropped") instead of hanging. The other 11 tests fail at their 4 s `CALL_BOUND` (`:1031-1044`). I found no remaining unbounded await in this file. `Peer::start` is bounded by `require_socket_observation` (`:870-881`, `:904-960`), `after_call` by `:609`, and the release loop by `:1464`. The compile-time check `DROPPED_SETUP_BOUND + CLOSE_BOUND < DROPPED_T_OP` (`:215-216`) holds (16 s < 60 s). Could not refute.
- **C5's 4 TIMEOUT mutants (`crates/validate/src/s3/body.rs:195`, `:196`, `:213`, `:217`) are not hangs in this diff's tests.** Under the `wake` mutant, the whole-package run stalls past 60 s in #852's `crates/validate/tests/s3_client_roundtrip.rs`: `empty_object_round_trips`, `put_get_delete_round_trips_byte_identical` and others, which use a 300 s `TRANSFER` bound (`s3_client_roundtrip.rs:135`). This diff's file catches the same mutants within 15 s. That older file is not in this diff, so I am not filing it here. It is only how the C5 row should be read.
- **Does the evidence exercise the production path, and would it go red on the defects that matter?** Each mutant below is one-line, applied to the production code, and run against the test as shipped:
  - The body keeps polling the source after the answer (deleted `body.rs:303-305`): **3 failed**, the three late-body tests ("polled past its hold" / `SourceFailed`).
  - No request-first wait (`Turn` check at `body.rs:308` disabled): **7 failed, in 5 out of 5 runs**.
  - Round 4's receipt rule, `produced == declared` instead of `ended`, at `body.rs:126`: **4(b) and 4(c) fail**, so the rule change does real work.
  - Outcome handed back before the runtime is shut down (swapped `s3.rs:403-406`): **8 failed, in 3 out of 3 runs** (lifetime contract).
  - Abandon signal ignored on drop (`s3.rs:396`): **scenario 5 fails** ("source outlived the dropped PUT by more than 1s"). So the green-only guard does guard the patch's own drop path.
- **Can concurrent PUTs on one `S3Client` share a connection across their private runtimes?** If they could, a PUT would end up on a connection whose task dies with another PUT's runtime. They can't. `pool_max_idle_per_host(0)` (`crates/validate/src/s3.rs:137-143`) turns the hyper-util pool off entirely (`hyper-util-0.1.20/src/client/legacy/pool.rs:115-116`, `max_idle_per_host > 0`). So no checkout waiter exists that a finished connection could be handed to. Could not refute.
- **Is anything awaited between hyper handing over the response and `ResponseArrival` recording it (`s3.rs:419-443`)?** If so, the source could end in that gap. I checked `aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:497-511`: the connector future resolves and `read_after_transmit` runs in the same poll. Stalled-stream protection is off, so `MaybeUploadThroughputCheckFuture` just passes through. Could not refute.
- **Edge inputs I considered and found handled:**
  - 0-length PUT: `empty_object_round_trips` passes in `gate-logs/C4-ci.log`.
  - Always-pending source: the operation deadline fires, then the runtime shuts down.
  - Future dropped before the thread starts: `abandoned` has already resolved at `s3.rs:396`.
  - Thread or runtime fails to start: the request is dropped before the error comes back (`s3.rs:388-392`, `:408-410`).
  - Early 4xx/5xx: comes back as an error, never a receipt.

## Unwarranted claims

- NEEDS-HUMAN [human] — **"12 tests must go red pre-fix" is not what happened, and was never expected to be.** Your round-6 sign-off note says "All 12 tests must still go red pre-fix and green post-fix as in round 5". The C4-verify row in `check-gates.json` says "12 test(s) ran red". Both the gate log and my own re-run show **11 red and 1 green on base**. The green one is `a_put_dropped_while_blocked_mid_write_releases_the_upload` (`crates/validate/tests/s3_client_upload_peers.rs:1426`), which passes on #852 because hyper releases a dropped request itself. The brief already declares this test green-only (Falsifiability). The gate's "12 ran red" means 12 tests ran in the red leg, not 12 failed. Nothing needs rebuilding. Just confirm that a green-only scenario 5 is what you meant to accept. My drop-path mutant above shows it still catches a broken abandon path in this patch.

## Not checked

- I did not diff round 6 against round 5's patch to confirm that only the dropped-PUT test changed ("keep the production code, the docs, and the other eleven tests as they are"). Round 5's patch is not among my inputs. The red/green split and failure messages are the same as round 5's description.
- C4 diff coverage was not measured. The patch stacks on #852 and does not apply on `origin/main` (`gate-logs/C4-diff-cov.log`). The mutants above cover the lines that decide the outcome and the lifetime.

### Advisory — code-review

No findings on either lens: introduced correctness bugs, or actionable reuse, simplification, and efficiency issues. The brief's accepted production design and explicit deferrals remain settled.

The dropped-PUT setup wait is bounded on the test runtime at `crates/validate/tests/s3_client_upload_peers.rs:1437`, with a diagnostic failure at `crates/validate/tests/s3_client_upload_peers.rs:1447`. Its 15-second bound covers connection setup plus the backpressure wait; unwinding drops the PUT and aborts the peer through `crates/validate/tests/s3_client_upload_peers.rs:616`.

Validation evidence: frozen C4 verify reports 12/12 passing with the fix and 11 assertion failures on the base, with the declared cancellation guard passing. C4 CI passed. Tests were not rerun in this read-only review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Accept the brief's affected-path inventory for unpublished rejected #741 attempts — repository history and all-state PR checks are independently complete, but those unpublished diffs are absent, so their non-overlap cannot be independently settled here (`review-prior-art.md:15`; `brief.md:231`).
- [x] Validation — fitness-to-purpose — Accept this slice for its intended validator use — real loopback evidence supports the bounded upload contract, while the already accepted kernel-buffer visibility limit and per-PUT resource cost remain material to later high-rate scenarios (`crates/validate/src/s3.rs:168`; `crates/validate/src/s3.rs:198`; `brief.md:245`).
- [x] **"12 tests must go red pre-fix" is not what happened, and was never expected to be.** Your round-6 sign-off note says "All 12 tests must still go red pre-fix and green post-fix as in round 5". The C4-verify row in `check-gates.json` says "12 test(s) ran red". Both the gate log and my own re-run show **11 red and 1 green on base**. The green one is `a_put_dropped_while_blocked_mid_write_releases_the_upload` (`crates/validate/tests/s3_client_upload_peers.rs:1426`), which passes on #852 because hyper releases a dropped request itself. The brief already declares this test green-only (Falsifiability). The gate's "12 ran red" means 12 tests ran in the red leg, not 12 failed. Nothing needs rebuilding. Just confirm that a green-only scenario 5 is what you meant to accept. My drop-path mutant above shows it still catches a broken abandon path in this patch.
- [x] **The accepted response-window limitation relies on an overstated backstop.** `brief.md:86-89` promises that a body the server never received “still surfaces as a fatal `integrity` failure” and requires that promise in the `put_object` documentation. At `$PDCA_TARGET`, `crates/validate/src/lib.rs:5-9` says no scenario drives the client yet, and `:118-123` explicitly reports that nothing was validated. Even the proposed oracle is narrower: `docs/design/proposals/draft/0017-blackbox-validation-tool.md:403-407` puts the recorded-digest check under single-writer keys; `:413-422` permits concurrent reads to return any value actually written, and `:435-438` permits quiescence to settle on one of those values. A discarded concurrent PUT can therefore leave another permitted value without triggering a digest mismatch. Revise the brief and required documentation to identify the safeguard as future, conditional single-writer/read-back coverage, and acknowledge that it does not guarantee detection of every false receipt. This does not require reopening the accepted connector deferral.
- [x] **The new receipt tests synchronize on peer transmission, while the contract synchronizes on client observation.** `brief.md:63-65` requires source EOF before hyper hands over the response, but `:68-74` requires `AcknowledgedEarly` when EOF/an extra byte is held until the peer answers; the reproduction explicitly releases the hold after the peer writes `200` (`:160-162`). The brief itself admits that hyper may not yet have read that response (`:78-85`). In that permitted interval, the source can report EOF, making a receipt legal, or yield the extra byte, making `SourceLength` legal. The target adapter records EOF at `crates/validate/src/s3/body.rs:133-135` and rejects an available excess piece at `:118-127`; peer transmission does not order those polls against client response observation. Thus the mandatory exact outcome and claimed red result (`brief.md:94-98`) can depend on scheduling or test the expressly excluded window. Specify a fixture that keeps EOF/the extra byte unavailable until the PUT completes, or explicitly synchronizes with client response observation, then restate the red expectations for that fixture.
- [x] size backstop — this slice is behaving oversized: patch is 104 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
