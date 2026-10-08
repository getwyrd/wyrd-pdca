# Brief — validate-s3-client-upload-against-misbehaving-peers

- **Slug:** validate-s3-client-upload-against-misbehaving-peers
- **Track:** blackbox
- **Goal:** A PUT against a server that answers before the caller's source has ended, or that
  stops reading, ends as a failure, never as a receipt. When the call returns, the caller's
  source has been dropped and the client has closed the connection. This is revision 5 of the
  brief: rounds 1-4 built and proved most of it, and the human's iterate-to-Plan decisions of
  2026-09-30 and 2026-10-04 (see "Standing decisions" below) change the receipt rule and settle
  what is in and out of this slice.
- **Defect:** #852's client trusts the SDK's outcome for an upload. Reproduced on the base
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
- **Success criterion:** BINDING. The production client (`resolve_config` +
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
- **Falsifiability:** RED is on the C4 verify gate's own contract: `./engine/scripts/run-verify.sh`
  reverts the production change, keeps the test, and runs `cargo test -p wyrd-validate --test
  s3_client_upload_peers` on the wave base (`$PDCA_VERIFY_BASE`, honoured at
  `engine/scripts/run-verify.sh:264-265`), loopback only. Round 4 earned that red on this base:
  10 of 11 failed by assertion. With scenario 4's client-observed hold, both new cases are red on
  the base by assertion, whatever the scheduling. 4(b): #852 keeps no record of the answer, so
  once hyper has read it the source goes on, ends, and the PUT is a receipt (or, where the hold
  never releases, the PUT is still a receipt, since #852 trusts the SDK). 4(c): #852 either takes
  the held byte after the answer and reports `SourceLength` (what round 4's adversary saw) or,
  where the hold never releases, returns a receipt; neither is `AcknowledgedEarly`, so the test
  asserts the exact variant. **Assert that variant through the error's `Debug` text
  (`Body(AcknowledgedEarly {`), never by naming `BodyError::AcknowledgedEarly` in code:** the
  variant does not exist on the base (`git show df68932:crates/validate/src/s3/error.rs`), so
  naming it stops the whole test binary compiling on the red leg, which the gate scores
  UNVERIFIABLE, not red (`engine/scripts/run-verify.sh:73-77`). Round 4's test names no new
  variant; keep it that way. **Second red, required in `build-notes.md`:** the gate only
  compares against #852, so Do must also show 4(b) and 4(c) failing against round 4's
  production code unchanged (both return a receipt there: the source had given 10 of 10 when
  the answer was recorded, and `Turn` keeps the hold shut), which proves the rule change does
  real work. Declared green-only
  guards: scenario 5 is green on #852 (hyper releases a request dropped before its answer
  through its own cancel path) and guards the abandonment handling; 4(a) is red on #852 only
  through the lifetime contract (#852 keeps the connection pooled).
- **Invariant to restore:** *An upload's reported outcome never claims more than the client
  handed over: a receipt requires that the caller's source ended (gave its whole declared length
  and reported its end) before the response arrived. No upload outlives its call: on every way
  the call can end (a receipt, an error, the operation timeout, a failure to start, the future
  being dropped), the caller's source is dropped and the client has closed the connection,
  before the call returns or, for a dropped future, within a fixed bound of the drop.* What lies
  beyond the client's sight is the stated limit above, not a claim. Sources: `AGENTS.md`
  "Absent or unsupported entries" ("never silent success") and "Await discipline" ("every await
  on external work is bounded … spawned helper tasks are aborted on drop"). Self-test: waking
  the body poller is not enough, because a connection blocked on socket writes never polls the
  body again (round 3's finding, still true).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 852
- **Conflicts with:** 853
- **Ordering note:** builds on #852's upload path (#852 is COMPLETE, draft PR #859, folded into
  the run's integration branch at `d9c6225`). `Conflicts with` #853 (a split sibling, now
  AWAITING_SIGNOFF) because both edit `crates/validate/src/s3.rs`, `s3/body.rs`, `s3/error.rs`
  and the `validate` paragraph of `05-building-block-view.md`; the scheduler keeps them in
  separate waves.
- **Surfaces:** data
- **Difficulty:** medium. Three production files in `crates/validate/src/` (`s3.rs`, `s3/body.rs`,
  `s3/error.rs`), the test file and one paragraph of one architecture doc. The change on top of
  round 4 is small; the whole diff against the base is round 4's size.
- **Do model:** opus
- **Scope:** the PUT path's reported outcome and its resource lifetime, against a peer that
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
- **Repro instruction:** (i) On the base, serve a loopback peer that reads the request head,
  replies `200 OK`, `Content-Length: 0`, `x-amz-request-id: early`, and stops reading. A
  `put_object` with a 64 MiB generator returns `Ok` with that request id. (ii) Against round 4's
  production code, with round 4's `Hold` (the source may go on only once the client has read the
  answer off its socket): a 10-byte source that gives all 10 bytes and holds its end when the
  peer answers `200` returns `Ok(PutOutcome)` (round 4's own test `:1137` asserts exactly this);
  the same source holding a further 1 byte instead of its end also returns `Ok(PutOutcome)`.
  Both are scenario 4 (b) and (c).
- **External dependencies:** none. Base Rust toolchain, loopback sockets, and the Linux
  /proc/net/tcp table the round-4 fixture already reads (present on the dev and CI hosts). The
  backpressure cases depend on socket buffer behaviour, so size the pieces to overwhelm the
  buffers rather than tuning the kernel.
- **Test file:** `crates/validate/tests/s3_client_upload_peers.rs` (a NEW file against the base;
  round 4's version, 11 tests, is the starting point).
- **Verification posture:** default (red pre-fix, green post-fix on the C4 gate), with the two
  declared exceptions in Falsifiability (scenario 5 green-only; 4(a) red only through the
  lifetime contract) and the required second red against round 4's production in
  `build-notes.md`.
- **Citations expected:** `path:line` on the base for every change. **Starting point:** round
  4's patch, `results/issue_854/iteration-v4/patch.diff` in this bundle. It applies cleanly on
  `df68932` (checked at Plan). Apply it, then make this revision's changes on top; ship one
  `patch.diff` against the base. Places to touch, as round-4 tree lines: `s3.rs:152-177` (the
  `put_object` doc, the receipt and cost paragraphs); `s3/body.rs:118` (`Upload::answered`),
  `:138` (`acknowledged_early`), `:265` and `:323` (where the body records the source's end);
  `s3/error.rs:100` (`AcknowledgedEarly`); test `:606`, `:681-683` and `:814-829` (the close
  oracle), `:1121` and `:1137` (the two receipt-boundary tests; `:1137` flips to scenario 4(b),
  and 4(c) is a new test on the same `Hold`, `Source::new(11, …)` held at 10 against a declared 10).
  Do not undo round 4's fixture fixes (zero-linger peer socket, receive-queue-first hold check,
  clock-timed backed-up wait, 500 ms late-body delay; `iteration-v4/build-notes.md:34-78`).
- **Prior-art check (triage cycles):** by affected path. Merged history on `origin/main`: no
  commit touches `crates/validate/src/s3.rs`, `crates/validate/src/s3/` or
  `crates/validate/tests/s3_client_upload_peers.rs` (they do not exist there). PRs, all states:
  only #859 (#852's draft PR, the base this builds on) and #860 (#742's PR, which carries
  #852's files under it) touch them; neither changes the upload outcome or lifetime. Closed or
  rejected work: #741's three unpublished attempts (`results/issue_741/iteration-v{1,2,3}/patch.diff`)
  touched a different layout (`crates/validate/src/client.rs`, `error.rs`, `lib.rs`,
  `tests/s3_client_roundtrip.rs`, new workspace dependencies, `deny.toml`) and none of this
  slice's files; their findings are the source of scenarios 1-3. Rounds 1-4 of this bundle are
  unpublished and are the starting point above.
- **Disposition hint:** new-feature

## Standing decisions (the human's, at iterate-to-Plan; Do and Check build and judge against these)

Recorded by Eduard Ralph at the round-4 sign-off (2026-09-30) and at Plan (2026-10-04, option A).

1. **Receipt rule: the source must have ENDED.** A receipt requires that the source gave its
   whole declared length and reported end-of-stream before the response arrived. Giving the
   declared length is not enough. This removes both receipts round 4 allowed (held end; overrun).
2. **Per-PUT cost: accepted for this slice, redesign later.** Each PUT keeps the round-4
   lifetime design (its own connection, closed when the call returns), with the cost documented
   as in Scope. Closing only after abnormal endings while pooling clean receipts needs a
   connector that owns the socket, so it is a separate follow-up issue that the human files.
   It must land before #743's high-rate scenarios (churn, endurance). This slice adds no new
   HTTP seam and no new dependency.
3. **The kernel-buffer window is part of the stated limit.** The window between the answer
   reaching the client's receive buffer and hyper reading it needs socket access (the follow-up
   in 2), and the read-back hash (`0017-blackbox-validation-tool.md:407`) is the backstop.
   (Plan review, 2026-10-04: that backstop is narrower than first written. It does not exist
   until #743, and it covers single-writer keys read back before their next overwrite, not the
   contention pool; see the stated limit in the success criterion. The decision to accept the
   window is unchanged.)
4. **Seeded Tier-0 DST coverage is out of scope for `crates/validate`.** The rule
   (`AGENTS.md:188-190`, "a new destructive or concurrent path lands with seeded Tier-0 DST
   coverage") cannot apply here: `crates/validate` has no `madsim` build, `crates/dst` does not
   depend on it, the blackbox guard keeps every `wyrd-*` crate out of its normal dependencies
   (proposal 0017 §9), and the mechanism is a real OS thread, tokio runtime and socket, none of
   which madsim models. The loopback tests are the coverage. The decision is also recorded for
   the T4 gate in this bundle's `review-rejected.md`.
5. **The connection-close oracle measures from the call's actual return.** Round 4 started the
   1 s clock when the peer received the "call ended" signal (test `:606`), measured from the
   start of the wait (`:681-683`), and accepted any `After(_)` (`:820`). Fix it as stated in the
   success criterion.
6. **The architecture paragraph is in scope.** `AGENTS.md:154` makes the living-doc update a
   merge requirement, so the one paragraph in `05-building-block-view.md` is allowed, overriding
   the earlier "nothing outside `crates/validate/`".

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts. If any
scenario above turns out to need a new HTTP seam after all, STOP and report with the evidence:
that is a Plan question (Standing decision 2), not something to work around.

## Plan review (2026-10-04, `plan-advisory-plan-reviewer.md`)

Plan-review response: finding 1 (overstated backstop) — accepted, brief revised. The stated limit, the Scope bullet and a note under Standing decision 3 now say the read-back check does not exist until #743, catches a false receipt only on a single-writer key read back before its next overwrite, and misses the contention pool (`0017-blackbox-validation-tool.md:403-410`, `:413-422`, `:435-438`). The `put_object` doc must say it in those terms. Accepting the window (Standing decision 3) is not reopened.
Plan-review response: finding 2 (receipt tests keyed to the peer's write) — accepted, brief revised. The wording was the problem: round 4's `Hold` already releases only once the client has read the answer off its socket, and with `Turn` the source is never polled past its hold during the call. Scenario 4 now requires that fixture and its two fixture asserts, the repro says so, and Falsifiability restates the red for each tree. It also adds one gap found while checking: the test must match `AcknowledgedEarly` by its `Debug` text, because naming the variant would stop the test compiling on the base, and the gate scores that UNVERIFIABLE, not red.

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T5 Judgment — Require socket observation for the close and post-answer polling claims, or report them unsupported — missing observation can silently weaken the regression oracle (crates/validate/tests/s3_client_upload_peers.rs:727; crates/validate/tests/s3_client_upload_peers.rs:904).; **Where `/proc/net/tcp` can't be read, four fixture checks pass without testing anything, not two.** T4 already flagged the hold (`crates/validate/tests/s3_client_upload_peers.rs:319`) and the close check (`:904`, `ClientClose::Unobservable => true`). The same pattern appears twice more: `BackedUp::Unobservable` is accepted as "the client backed up" in scenario 2 (`:1112`) and scenario 5 (`:1265`). On such a host, `wait_backed_up` (`:693-697`) just sleeps 1 s and answers, so the backpressure tests pass even if the client never backed up. And because the hold never releases, `assert_not_polled_past_the_hold` (`:969`) passes for a client that keeps polling the body after the answer. Concrete failing case: run the suite in a sandbox that hides `/proc/net` against a fix that drops the `upload.answer.get().is_some()` check (`src/s3/body.rs:303`). The late-body tests still go green, because their source never gets the chance to go on. Fix: at the top of each test that depends on it, fail with a message (or skip loudly) when `tcp_table_readable()` (`:757`) is false, instead of degrading in four separate places. CI on Linux can read the table, so this is about the test being honest, not a live red.; `crates/validate/tests/s3_client_upload_peers.rs:904`: `ClientClose::Unobservable` counts as successful closure. When `/proc/net/tcp` is unavailable, `wait_client_close` returns immediately and `serve` starts draining the socket (`:649-650`). Draining can unblock a writer that survived the call, allowing it to finish and close within the one-second bound; the test then passes despite the connection requiring peer cooperation to close. Require the closure-observation capability before running these cases, or explicitly report them as unsupported instead of accepting this branch as proof. This corroborates the second finding in the frozen T4 review log; it does not invalidate the recorded runs on hosts where observation succeeded.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b. 15 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 49 mutants tested in 5m: 2 missed, 7 caught, 36 unviable, 4 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_854/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Round 5 is accepted in substance: keep the production code, the docs, and the other eleven tests as they are. Only fix: the dropped-PUT test (crates/validate/tests/s3_client_upload_peers.rs:1417) can hang. Its select! waits only for the PUT or the peer's backpressure notification, and a request-wakeup regression before the peer reads the head leaves both pending forever. Wrap that setup wait in its own test-runtime timeout covering connection setup plus DROPPED_BACKED_UP_WAIT, and fail with a diagnostic message on expiry, never a hang. All 12 tests must still go red pre-fix and green post-fix as in round 5.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Round 5 is accepted in substance: keep the production code, the docs, and the other eleven tests as they are.
  Only fix: the dropped-PUT test (crates/validate/tests/s3_client_upload_peers.rs:1417) can hang. Its select! waits only for the PUT or the peer's backpressure notification, and a request-wakeup regression before the peer reads the head leaves both pending forever. Wrap that setup wait in its own test-runtime timeout covering connection setup plus DROPPED_BACKED_UP_WAIT, and fail with a diagnostic message on expiry, never a hang.
  All 12 tests must still go red pre-fix and green post-fix as in round 5.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 49 mutants tested in 5m: 2 missed, 7 caught, 36 unviable, 4 timeouts
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
