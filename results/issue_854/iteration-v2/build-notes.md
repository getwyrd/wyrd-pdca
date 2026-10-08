# Build notes — #854 validate-s3-client-upload-against-misbehaving-peers (iteration 2)

Base: `df68932f2c633586fc2ce60cc418f878f8010d7d`, the integration branch
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (the bundle's `stack-base`), with
#852 folded in at `d9c6225`. `patch.diff` applies cleanly on it (`git apply --cached --check`
against that tree). All `path:line` below are on the patched tree unless marked "base".

Artifacts: `patch.diff` (3 production files + the new test), `s3_client_upload_peers.rs` (a
copy of `crates/validate/tests/s3_client_upload_peers.rs`), this file.

## What changed since iteration 1 (the carry-forward)

| Finding | What I did |
|---|---|
| **C5 / adversary: a receipt after the client has read the early `200`.** hyper hands the response to the request and, in the same poll of its connection task, asks the body for more; the SDK hook that recorded the arrival only runs later, in the request's own poll. A final piece taken in that gap made `produced == declared` and a receipt. | Fixed at the cause, without a new HTTP seam: the body now waits whenever the request has a wake it has not been polled for (`Turn`, `crates/validate/src/s3/body.rs:167`). The response's wake is one of them, so the request always sees and records the response (`ResponseArrival` → `Upload::answered`, `s3.rs:395`, `body.rs:118`) before the body can take another piece. Details below. Reproduced first on iteration 1's code with a new deterministic test (`a_final_piece_ready_as_the_acknowledgement_arrives_is_the_body_error`, test file `:712`): red 3/3 on iteration 1's code, green with the fix. |
| **T5: the failure-after-ack test raced** (`wait_delivered` proved kernel delivery, not that the client had read the answer). | `wait_delivered` and the waker-based gate are gone. Both post-ack cases now use a *held* source (test file `:177` `Hold`, `:459` `release_when_parked`): the source parks after 4 bytes and keeps no waker; the peer waits until it has parked, releases it **without waking anyone**, and only then writes the `200`. The only thing left that can wake the connection is the `200` arriving, so the source's next poll, if any, can only come after hyper has read the answer. That ordering is built into the fixture, not timed. The fixture asserts it happened (`Parked::ReleasedBeforeAnswer`, `:657`). 50/50 runs green. |
| **`s3.rs:348`: runtime-start failure published the outcome while `request` (and the source) was still alive.** | `drop(request)` before sending the error (`s3.rs:359`). For the other start-failure path (thread spawn), std already drops the closure before `spawn` returns `Err`: `library/std/src/sys/thread/unix.rs:98-105` (rustc 1.96.0) drops the boxed closure when `pthread_create` fails, and the stack-size error returns before the box is leaked. The doc says so (`s3.rs:333-336`). |
| Doc claims that the source "is not polled again" after the answer (`s3.rs`, `body.rs`, `error.rs`). | These are true now and the wording is exact: "from the moment hyper hands the response over" (`s3.rs:155-166`, `body.rs:251-254`, `error.rs:95-99`). |
| **T4 prior art.** | Checked by file path, not text: `gh pr list --repo getwyrd/wyrd --state all --limit 400` (368 PRs, all states) filtered to PRs whose diff touches any of the four files. Only two hits, both open: #859 (#852's own draft PR, the base this builds on) and #860 (#742, stacked on #852). No closed or merged PR touches the upload path. #741's unpublished attempts are the ones the brief cites (`results/issue_741/`). |
| **C5 mutants: 2 missed, both `ResponseArrival::name` → `""` / `"xyzzy"`.** | Not fixed, on purpose. The SDK shows an interceptor's name only in the error it builds when that interceptor fails; this one always returns `Ok` (`s3.rs:402-410`). A test asserting the string would only restate it. Recorded as an accepted survivor. |
| **C4 diff coverage: "patch.diff does not apply on origin/main".** | Not a patch defect: this bundle is stacked on #852, which is not on `origin/main`. The patch applies on the stack base. The gate fell back to `origin/main`; that looks like a harness-side base-resolution issue for stacked bundles (`run-diff-cov.sh:685-689` falls back when the base ref isn't found on origin). Flagging for the human; not mine to change. |

## Decision for the human (read first; carried over from iteration 1, still open)

The brief says: if releasing a connection blocked mid-write "cannot be done through the SDK's
public connector or runtime API without forking it, STOP and report". The SDK's **connector**
API cannot do it. I do it through the **runtime** the SDK runs on (tokio), unchanged from
iteration 1, and the reviewers did not object to it. If you read "the SDK's runtime API" as
the SDK's own runtime traits only, this should have been a STOP and the evidence below is the
report.

- `aws-smithy-http-client` 1.4.2 keeps the hooks private: `wrap_connector` is `pub(crate)`
  (`src/client.rs:208`), so are `hyper_builder`/`set_hyper_builder` (`:472`, `:483`); the
  executor is fixed to `TokioExecutor::new()` (`:541`). hyper-util's `TokioExecutor::execute`
  is `tokio::spawn` (`hyper-util-0.1.20/src/rt/tokio.rs:110-115`), so the connection task lands
  on whatever runtime polls the request.
- So `put_object` runs each PUT on a current-thread runtime of its own, on a thread of its own
  (`on_own_runtime`, `s3.rs:341`), and shuts that runtime down before handing back the outcome
  (`s3.rs:372`). That drops hyper's connection task, its socket (FIN) and the request body
  (the source).
- PUTs use a client with the pool off (`pool_max_idle_per_host(0)`, `s3.rs:137-143`), so no PUT
  connection is parked for reuse on a runtime that is about to die.

**Still open from the iteration-1 adversary (NEEDS-HUMAN there): the per-PUT connection cost.**
Every PUT now opens a TCP connection the client closes, so each leaves a client socket in
`TIME_WAIT`. The adversary measured 300 PUTs → 300 `TIME_WAIT` sockets and estimated a cap of
~470 PUTs/s per client IP and endpoint for a non-loopback endpoint (28,232 ephemeral ports, 60 s
`TIME_WAIT`). That cost follows from the brief's invariant ("when `put_object` returns … the
client has closed the connection"), so I did not try to remove it. The `put_object` doc now
names it (`s3.rs:175-177`) instead of naming only the thread. Whether that is acceptable for
the validator's `endurance` scenario is a human call.

## Baseline (before any production change) — Falsifiability steps 1-3

1. #852's client is in the base: base `crates/validate/src/s3.rs:126` `put_object`, base
   `:148-149` the only `// deferred: #854` marker.
2. The test compiles against the base unchanged: it names only #852's public API
   (`resolve_config`, `Deadlines`, `Phase`, `PutOutcome`, `PutSource`, `ResolvedConfig`,
   `S3Client`, `S3Error`) and existing dependencies (tokio, bytes, futures-util).
3. Run on the base (`cargo test -p wyrd-validate --test s3_client_upload_peers`, wrapped in
   `timeout`): **6 of 6 red**, each on the assertion the brief predicts.

| Test | Outcome on the base | Why red |
|---|---|---|
| 1a `an_acknowledgement_before_the_source_is_done_is_the_body_error` | `Ok(PutOutcome { etag: None })`, 3 MiB of 64 MiB given, source alive at return | receipt for an incomplete upload (round 2, reproduced) |
| 1b `a_source_that_fails_after_the_acknowledgement_is_the_body_error` | `Err(Body(SourceFailed { produced: 4 }))`, no request id | the acknowledgement's request id is lost |
| 1c `a_final_piece_ready_as_the_acknowledgement_arrives_is_the_body_error` | `Ok(PutOutcome { etag: None })`, all 10 bytes given after the client had the `200` | receipt; this is the iteration-1 adversary's case |
| 2 `an_acknowledgement_under_backpressure_is_the_body_error_and_releases_the_upload` | `Ok` after 289 ms; 32 MiB of 512 MiB given; source alive at return; fixture `BackedUp::Yes(2500849)` | **retention reproduced** on #852 (round 3 F2): a real red, not green-only |
| 3 `a_peer_that_stops_reading_and_never_answers_is_the_operation_timeout` | `Err(Timeout { Operation, 3s })` (right), but source alive at return | red on the no-grace "dropped at return" check |
| boundary `a_source_fully_taken_before_the_acknowledgement_is_a_receipt` | `Ok` receipt (right), client socket still `ESTABLISHED` 1 s later | red only on the lifetime part: #852 keeps the connection for reuse |

Scenario 2 reproduced on the first setup (32 MiB pieces, 512 MiB object, peer `SO_RCVBUF`
64 KiB, the `200` sent once the client socket's send queue held still and non-empty for
200 ms). Scenario 3 is red rather than green-only for the reason iteration 1 found: #852 does
close the connection on its own, but on hyper's task, which on a current-thread runtime cannot
run before the test reads the drop flag; the brief's contract says "no grace period".

Also run against **iteration 1's production code** with this test: 5 green, 1c red, 3 of 3
runs. So the new test binds the carry-forward finding.

## The change

### `crates/validate/src/s3/body.rs` (new in this iteration: the `Turn`)

- `:167-198` `Turn`: two counters and two wakers. `wakes` counts the request's wakes; `seen` is
  `wakes` as it stood when the request's latest finished poll began. The body waits while they
  differ (`caught_up` `:181`, `body_waits` `:190`, which registers then re-checks so a poll that
  ends in between is not missed).
- `:200-209` `impl Wake for Turn`: a wake counts and passes on to the task that polls the
  request.
- `:220-242` `RequestFirst<F>`: polls the SDK's request future with the counting waker.
  Order per poll: register the outer waker (`:233`), read `wakes`, poll, store `seen` (`:239`),
  wake the body. Registering before reading means no wake can be counted and lost.
- `:107-113` `Upload::request_first` builds it, with the request wrapped in
  `tokio::task::unconstrained` (`:109`). Why: when tokio's cooperative budget runs out, a ready
  resource returns `Pending` and *defers* its wake (`tokio-1.52.3/src/task/coop/mod.rs:397`,
  `context::defer(cx.waker())`). The request would then end a poll with the response sitting in
  hyper's oneshot, unrecorded, and no wake counted yet, which is exactly the gap. On the PUT's
  own runtime nothing else competes, so turning the budget off costs nothing.
- `:294-296` the body's check, after the `answer` check (`:289`, unchanged from iteration 1).
- `:118-128` `answered`: comment now states why `produced` is exact (it runs inside a request
  poll, and the body takes nothing until that poll ends).

Why a counter and not a boolean cleared at the start of each request poll: with a boolean, on
a multi-thread runtime, the body could run between "cleared" and "recorded" in the same poll.
The counter keeps the body waiting for the whole poll. Same size of code.

Why this is the cause and not a guard: the defect is an ordering between two tasks (hyper's
connection task delivers the response, then polls the body; the request records the response
later). The `Turn` makes that order explicit. It doesn't add a probe or a timeout.

What it costs: during upload streaming the request future is only waiting for the response
and its deadline, so it is not woken and the body never waits. The only waits are around
connection setup (before the body is first polled) and the response itself.

Chain this relies on (checked in the locked versions): hyper sends the response through
`cb.send(Ok(res))` inside `poll_read` (`hyper-1.10.1/src/proto/h1/dispatch.rs:708`), and
`poll_loop` calls `poll_read` before `poll_write` (`:173-174`), which polls the body (`:394`).
The oneshot is `tokio::sync::oneshot` (`hyper-1.10.1/src/client/dispatch.rs:11`), registered
with our counting waker through the SDK's future chain (hyper-util `try_send_request`, then
`aws-smithy-http-client-1.4.2/src/client.rs:615-622`, then
`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:504`). There is no await between the
connector future resolving and `read_after_transmit` (`orchestrator.rs:504-511`), and
`MaybeUploadThroughputCheckFuture` is a direct pass-through with stalled-stream protection off
(`http/body/minimum_throughput.rs:405`). If a later SDK version moved the request onto a task
of its own, the response would wake that task instead of ours and the gap would come back; 1c
would catch it.

### `crates/validate/src/s3.rs`

- `:196` `put_object` passes `upload.request_first(request)` to `on_own_runtime`.
- `:359` `drop(request)` before reporting a runtime-start failure (carry-forward finding).
- `:333-336` `on_own_runtime` doc: every path drops `request` before the outcome is handed back.
- `:155-177` `put_object` doc: "arrived" is defined as hyper reading the response off the
  socket and handing it over; the "not polled again" claim is now exact; the cost names the
  per-PUT connection and `TIME_WAIT`, not just the thread.
- `:388-393` `ResponseArrival` doc: why the count it records is the count at arrival.
- Everything else (separate pool-less PUT client, own runtime per PUT, outcome ranking,
  removal of the base `// deferred: #854` marker at base `:148-149`) is as in iteration 1.

### `crates/validate/src/s3/error.rs`

- `:95-104` `BodyError::AcknowledgedEarly` (from iteration 1); doc now says "once the client
  has the answer" and covers a last piece as well as a failure.
- `:49-51` `RequestNotBuilt` doc names the thread as well as the runtime.

## The test — `crates/validate/tests/s3_client_upload_peers.rs`

Same lifetime contract and scripted peer as iteration 1 (`assert_lifetime` `:615`): return
within `T_OP + 1 s`; the drop flag read on the next line after return; within 1 s the client's
own socket leaves `ESTABLISHED` while the peer reads nothing (`wait_client_close` `:501`),
then the peer reads to EOF or reset (`drain_until_closed` `:552`).

New in this iteration:

- `Hold` (`:177`) and `Source::held_at`: the held source described above. It returns `Pending`
  without keeping a waker. That breaks the `Stream` wake contract on purpose: it is what pins
  the source's next poll to "after the client read the answer" without any timing. It cannot
  hang a run: the response wakes the connection, and a request never waits on its body to
  return a response. A broken client would still be bounded by `CALL_BOUND`.
- 1c (`:712`) is the iteration-1 adversary's case made deterministic: released to give its
  last 6 bytes.
- The fixture checks are now per test. 1a and 2 check that the source had not given its whole
  length; 1b and 1c check that the source parked and was released before the answer
  (`assert_released_before_answer` `:657`). So on a broken client, 1c fails with the "never a
  receipt" message, not a misleading fixture message.

Stress evidence for the wake-based shape (a throwaway test, run then removed; the shipped
file is byte-identical to the copy saved before it, checked with `cmp`). A source gives 4
bytes, waits on a gate that registers its waker, and the peer opens the gate either right
after or right before writing the `200`. At the source's final give it reads
`/proc/net/tcp` (the reviewer's probe): did the client still hold the `200` unread? 200 runs
per row:

| Gate opened | Fix | Outcome |
|---|---|---|
| after the `200` | with | 200 × `AcknowledgedEarly`, source never polled after the gate |
| after the `200` | `Turn` check disabled | 199 × receipt with the `200` already read by the client (the adversary saw 200/200) |
| before the `200` | with | 180 × receipt, **every one** with the `200` still unread in the client's socket (legitimate under the brief's rule); 20 × `AcknowledgedEarly` |
| before the `200` | `Turn` check disabled | 21 × receipt with the `200` already read (adversary: 27/200), 179 legitimate |
| after, source fails | with | 200 × `AcknowledgedEarly` |
| before, source fails | with | 188 × `SourceFailed` with the `200` unread (the source failed first, correctly), 12 × `AcknowledgedEarly` |

With the fix, zero receipts and zero source failures were observed after the client had read
the `200`, in 800 runs. (The probe's "read" side is conservative: "both queues empty" also
matches a `200` not yet sent, which would only overcount violations.)

## Refute your own test (forced)

- **(a) Genuine red? Yes.** Final test file, production reverted to the base
  (`git checkout -- crates/validate/src`): **6 of 6 FAILED**, each on the assertion in the
  baseline table. Restored the saved production diff (md5 `a81cb8b57e7a8c509004ad955e682c52`
  before and after). Separately, with only the `Turn` check disabled (`if false && …`), 1c
  failed 3 of 3 runs with a receipt; with iteration 1's production code, 1c failed 3 of 3. So
  the test binds the specific fix, not just "some change".
- **(b) Production path? Yes.** The test builds the client with `resolve_config` +
  `S3Client::with_deadlines` and calls `S3Client::put_object` with a `PutSource`: the real
  `aws-sdk-s3`, the real hyper connector, real loopback TCP. No mock, no copy. Only the peer
  and the source are scripted, and those are the injected fault.
- **(c) Fixture includes the fault? Yes.** The peer really answers early, stops reading, or
  never answers. Scenario 2 asserts the client's writes were backed up when the `200` went out
  (observed ~2.5 MB queued). 1b and 1c assert the source was parked and released before the
  answer was written. The close check reads the client's real socket.

Green, with the fix: 6/6 on 5 sequential runs; 3 copies of the test binary in parallel, all
green; the two held tests 50/50.

## Gates run locally (commit-readiness)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets` (forced re-check; workspace lints): clean.
- `RUSTDOCFLAGS="-D warnings --document-private-items" cargo doc -p wyrd-validate --no-deps`:
  clean.
- `typos crates/validate`: clean.
- `cargo xtask statics` (ADR-0035) and `cargo xtask blackbox-guard` (#775): pass.
- `cargo test -p wyrd-validate`: 39/39 (cli_surface 19, s3_client_roundtrip 14 against the real
  Wyrd gateway, the 6 new).
- No dependency change (`futures-util`'s `AtomicWaker` is in its `alloc` feature;
  `tokio::task::unconstrained` is in `rt`, already enabled). No commit hooks in the target
  (`core.hooksPath` unset, no non-sample hooks).
- Not run: the full `cargo xtask ci`; C4-ci covers it. Nothing outside `crates/validate/`
  uses the library API changed here.

## Rubric self-review

- One clock per lifecycle: no new clock reads. A PUT's connect and operation deadlines both
  sleep on its own runtime's timer (`s3.rs:51-57`).
- Narrow seams / dependency direction: no new dependencies; still no `wyrd-*` in the normal
  graph (blackbox guard passes). No new HTTP seam: the SDK's public interceptor plus a wrapper
  around the SDK's own request future.
- `#![forbid(unsafe_code)]`: the new test crate root has it; no `unsafe` added (`Wake` is the
  safe waker API).
- Docs currency: no port, API operation, RPC, CLI flag or persisted field changed. A new
  error variant of an out-of-process client library is none of these, and any doc edit outside
  `crates/validate/` is out of scope.
- Absent/unsupported entries ("never silent success"): this is the fix.
- Await discipline: no new awaits. The PUT thread is abandoned on drop through `_abandon`.
  The body's new wait is always released at the end of the request's next poll, and the
  request is polled whenever it is woken.
- Test fidelity / DST: `wyrd-validate` runs on real tokio and the AWS SDK and is not part of
  the DST build. I read the "new concurrent path lands with seeded Tier-0 DST coverage" rule as
  not applying to a client-side helper; flagging that reading for the human.

## Alternatives ruled out (with cost)

| Alternative | Why not |
|---|---|
| Yield once before each source poll (adversary's option a) | Relies on tokio running the request's task before re-polling the connection, which is a scheduler detail, not a guarantee; costs a yield per piece (16 extra polls for a 512 MiB / 32 MiB upload, one per 1 MiB piece in 1a). The `Turn` gets the same order from the wake itself, for any scheduler, and costs nothing while streaming. |
| Stamp the arrival on the socket read (adversary's option b) | Needs our own `HttpConnector` on hyper directly: `hyper` and `hyper-util` as direct dependencies (root `Cargo.toml` `[workspace.dependencies]`, `crates/validate/Cargo.toml`, `Cargo.lock`) plus an ADR-0003 note. Outside `crates/validate/` and a new HTTP seam, which the brief makes a Plan question. Not needed now that an in-seam fix holds. |
| Wrap the SDK's `HttpConnector` future (public trait) and stamp when it resolves | Resolves at the same moment as the interceptor (the request's poll), so it doesn't close the gap on its own; with the `Turn` it adds nothing over the interceptor. |
| A sticky flag: any wake of the request stops the body for good | One unrelated wake would stall the upload until the operation deadline. |
| A boolean cleared at the start of each request poll | Leaves a window inside the poll on a multi-thread runtime (described above). The counter is the same size. |

## Scratch

Files left in `$PDCA_SCRATCH/pdca-builder-854-v2/` (`prod.diff`, `test.rs.bak`, `idx` — a
temporary git index for the apply check). Left for the harness to reclaim with the scratch
root, per the harness's filesystem rule. Nothing else was created outside the worktree and
this bundle.
