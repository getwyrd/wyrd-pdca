# Build notes — #854 validate-s3-client-upload-against-misbehaving-peers

Base: `df68932f2c633586fc2ce60cc418f878f8010d7d` (the integration branch
`pdca-integrate/r-a834e98f67b952c61db7a03084c626e8/main`, with #852 folded in at `d9c6225`).
All `path:line` below are on the patched tree unless marked "base".

Artifacts: `patch.diff` (3 production files + the new test), `s3_client_upload_peers.rs` (a
copy of `crates/validate/tests/s3_client_upload_peers.rs`), this file.

## Decision for the human (read first)

The brief says: if releasing a connection blocked mid-write "cannot be done through the SDK's
public connector or runtime API without forking it, STOP and report". The SDK's **connector**
API cannot do it. I did it through the **runtime** the SDK runs on (tokio), and I want that
interpretation checked at sign-off.

- What the SDK's HTTP client keeps private, in `aws-smithy-http-client` 1.4.2: the TCP-connector
  hook `ConnectorBuilder::wrap_connector` is `pub(crate)` (`src/client.rs:208`), so is the hyper
  builder override `hyper_builder`/`set_hyper_builder` (`:472`, `:483`) and
  `build_with_tcp_conn_fn` (`:1100`). The executor is hard-coded: `TokioExecutor::new()`
  (`:541`). So no public way to wrap the socket, or to choose where the connection task runs.
- Why nothing short of that works: after an early `200` with `Content-Length: 0`, hyper's
  connection is in `Reading::KeepAlive` / `Writing::Body`. With a full write buffer it only
  calls `poll_flush` (`hyper-1.10.1/src/proto/h1/dispatch.rs:377-378`) and never polls the
  body. Its read side waits on the socket (`conn.rs:491-507`, `mid_message_detect_eof`). The
  only wakers it holds are socket readiness, so neither waking the body (round 3's attempt)
  nor dropping the source reaches it. (A response with a body would let us close it by
  dropping the response body, `dispatch.rs:224-235` → `is_done` `:470-472`, but an empty
  body has no channel, `dispatch.rs:306-307`.)
- What I used: hyper-util's `TokioExecutor::execute` is `tokio::spawn`
  (`hyper-util-0.1.20/src/rt/tokio.rs:110-115`), so the connection task lands on whatever
  runtime the request is polled on. `put_object` now runs each PUT on a current-thread
  runtime of its own, on a thread of its own, and shuts that runtime down before returning.
  tokio drops every task of a current-thread runtime synchronously on shutdown
  (`tokio-1.52.3/src/runtime/runtime.rs:497-505` → `scheduler/current_thread/mod.rs:277-297`,
  with `assert!(owned.is_empty())`). That drops hyper's task, its `TcpStream` (fd closed →
  FIN) and the request body (→ the caller's source).
- The risk: this leans on the SDK spawning onto the *current* runtime. If a later SDK spawned
  on a stored handle, the guarantee would break. The new test would catch it (scenario 2 goes
  red on "source alive at return" and "client socket still ESTABLISHED").
- The alternative that removes the dependency on that detail (own `HttpConnector` on hyper's
  low-level `client::conn::http1`, owning the connection future) needs `hyper` and
  `hyper-util` as direct dependencies: root `Cargo.toml` `[workspace.dependencies]`,
  `crates/validate/Cargo.toml` and `Cargo.lock` edits, plus an ADR-0003 audit note. That is
  outside `crates/validate/` (out of scope) and a different HTTP seam, which the brief calls a
  Plan question. Not done.

If you read "the SDK's runtime API" as the SDK's own runtime traits only (not tokio), this
should have been a STOP, and the evidence above is the report.

## Baseline (before any production change) — Falsifiability steps 1-3

1. #852's client is in the base (`crates/validate/src/s3.rs` base:126 `put_object`, base:148
   the `// deferred: #854` marker).
2. The test file compiles against the base unchanged: it names only #852's public API
   (`resolve_config`, `Deadlines`, `Phase`, `PutOutcome`, `PutSource`, `ResolvedConfig`,
   `S3Client`, `S3Error`) and existing dev-dependencies (tokio, bytes, futures-util).
3. Each scenario on the base (`cargo test -p wyrd-validate --test s3_client_upload_peers`,
   current-thread test runtime). All five are RED:

| Test | Base outcome | Why red |
|---|---|---|
| 1a `an_acknowledgement_before_the_source_is_done_is_the_body_error` | `Ok(PutOutcome { etag: None })` with 3 MiB of 64 MiB given; source alive at return; client socket ESTABLISHED 1 s later | receipt for an incomplete upload (reproduces round 2) |
| 1b `a_source_that_fails_after_the_acknowledgement_is_the_body_error` | `Err(Body(SourceFailed { produced: 4 }))`, no request id | not a receipt here (round 2 saw one), but the acknowledgement's request id is lost |
| 2 `an_acknowledgement_under_backpressure_is_the_body_error_and_releases_the_upload` | `Ok` receipt after ~314 ms; 32 MiB of 512 MiB given; source alive at return; client socket ESTABLISHED 1 s later; fixture `BackedUp::Yes(2595177)` | **retention reproduced** on #852 (round 3 F2) |
| 3 `a_peer_that_stops_reading_and_never_answers_is_the_operation_timeout` | `Err(Timeout { Operation, 3s })` (right), but source alive at return; connection closed ~1.6 ms after | red on the no-grace "dropped at return" check |
| boundary `a_source_fully_taken_before_the_acknowledgement_is_a_receipt` | `Ok` receipt (right: the boundary did not move); client socket ESTABLISHED 1 s later | red only on the lifetime part: #852 returns the connection to the keep-alive pool |

Notes on the posture the brief declared:

- **Scenario 2 is a real red, not green-only.** It reproduced on the first setup tried:
  32 MiB pieces, 512 MiB object, peer `SO_RCVBUF` 64 KiB (kernel doubles it, autotuning
  off), and the `200` sent only after the client socket's send queue (`/proc/net/tcp`
  `tx_queue`) held still and non-empty for 200 ms. No other setups were needed. Earlier in
  the session the drain-only version of the test showed the same retention (source alive at
  return, connection still writing 72 MB a second later once the peer drained).
- **Scenario 3 is red, not green-only, on a current-thread runtime.** #852 does close the
  connection on its own (hyper sees the cancelled callback, `dispatch.rs:292-300`), but on the
  dispatcher task, which on a current-thread runtime cannot run before the test reads the drop
  flag. On a multi-thread runtime that read would race. The brief's contract says "no grace
  period", so the test uses a current-thread runtime to make the read strict. The fix makes it
  green on any runtime because the release happens before `put_object` returns.

## The change

### `crates/validate/src/s3.rs`

- `:50-57` `Deadlines` doc: a PUT's connect and operation deadlines now sleep on the timer of
  the PUT's own runtime. Same monotonic clock, one source per request (ADR-0009 rubric item).
- `:89-97` `S3Client` gains `uploads`: the same SDK config with
  `pool_max_idle_per_host(0)` (`:137-143`). With the pool size 0, hyper-util builds no pool at
  all (`hyper-util-0.1.20/src/client/legacy/pool.rs:114-117`, `:127-141`), so no PUT
  connection is ever parked for reuse, and no idle-reaper task is spawned on a runtime that is
  about to die. GET/DELETE keep the pooled client (`sdk`), unchanged.
- `:152-172` `put_object` doc: the receipt rule and its stated limit (the brief's "inherent
  limit"), and the lifetime guarantee with its mechanism and cost.
- `:173-209` `put_object`: builds the request with an interceptor (`ResponseArrival`), runs it
  through `on_own_runtime` (`:191`), then decides:
  1. SDK success and the source had **not** given its whole length when the response head
     arrived → `S3Error::Body(BodyError::AcknowledgedEarly { declared, produced, request_id })`
     (`:193-197`). This outranks a source failure, so "a source that fails after the
     acknowledgement" also carries the request id.
  2. otherwise a recorded source failure outranks, as #852 had it (`:198-202`, base:143-147).
  3. otherwise the SDK's result, as before.
  The `// deferred: #854` marker (base:148-149) is removed. It was the only #854 marker.
- `:319-374` `on_own_runtime`: thread `wyrd-validate-put`, current-thread runtime built **on
  that thread** (building it here and dropping it on the thread-spawn error path would drop a
  runtime inside the caller's async context, which tokio refuses with a panic,
  `tokio-1.52.3/src/runtime/blocking/shutdown.rs:44-56`). `block_on(select(request,
  abandoned))`, then `shutdown_background()` (`:360`, drops all tasks, does not wait on a
  stuck `getaddrinfo`), then sends the outcome. If `put_object`'s future is dropped, the
  `_abandon` sender drops, `abandoned` resolves, the thread stops polling and shuts the
  runtime down the same way. Runtime or thread start failure → `S3Error::RequestNotBuilt`
  (nothing was sent; doc widened at `error.rs:49-50`).
- `:376-396` `ResponseArrival` (`Intercept::read_after_transmit`): records the moment the
  response head arrives, with its `x-amz-request-id` (read with #852's own `request_id`
  helper, so the decoy-header rule still holds). The orchestrator calls this hook right after
  the connector returns the response and before it reads the response body
  (`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:502-511`, `:528`).

### `crates/validate/src/s3/body.rs`

- `:56-74` `into_body` returns `Arc<Upload>` instead of the bare failure slot.
- `:76-131` `Upload`: `declared`, `produced` (atomic, written only by the body), `failure`
  (the #852 `OnceLock`, same semantics), `answer` (set once by the interceptor:
  `produced` at arrival + request id). `acknowledged_early` (`:116-131`) is the one place the
  receipt rule is computed. If no arrival was recorded, the final count stands in. That cannot
  happen on an SDK success (the hook runs on every response), and in that case the final
  count is never less than the count at arrival, so the check could only be more lenient by
  the gap, never stricter.
- `:171-177`: once the answer is recorded the body returns `Pending` and never polls the
  source again. Not an error: an error makes hyper fail the connection
  (`dispatch.rs:128-140`), which would also cut a response body the SDK is still reading, and
  turn an early `403 AccessDenied` into an unreadable/dispatch error instead of the typed
  service error. hyper copes with a pending body without spinning (`dispatch.rs:199-210`).
- `:178-216` the length/failure logic is #852's, unchanged in meaning, now reading
  `declared`/`produced` from `Upload`.

### `crates/validate/src/s3/error.rs`

- `:94-103` new `BodyError::AcknowledgedEarly { declared, produced, request_id }`.
- `:192-203` its `Display`: "the endpoint acknowledged the PUT when its source had yielded N of
  M bytes, x-amz-request-id <id>", reusing `write_request_id` so the id prints the way every
  other error prints it.

## The test — `crates/validate/tests/s3_client_upload_peers.rs`

A scripted loopback peer (`serve`, `:376`) reads the request head and then follows its script
(`Answer`). The source type (`Source`) raises a `dropped` flag in `Drop`. Each test runs on a
current-thread runtime. The lifetime contract (`assert_lifetime`, `:605`):

- the call is wrapped in `timeout(T_OP + 1 s)` (`:85`, `put_against` `:567`); no return is a
  panic with a message;
- `probe.dropped` is read on the very next line after the call returns;
- within 1 s after return: (i) the client's own socket must leave `ESTABLISHED` in
  `/proc/net/tcp` **while the peer reads nothing** (`wait_client_close`, `:467`), then (ii) the
  peer reads until EOF or reset (`drain_until_closed`, `:536`).

Why (i) exists: my first version only did (ii). Draining unblocks a client stuck mid-write and
lets hyper finish or fail on its own, which is not "the client closed the connection". I
proved the gap with a throwaway variant (inline request as #852 + drop the source from a
shared slot at return, no runtime): it passed the drain-only check in scenario 2. With (i) it
fails scenarios 1a and 2 with "the client's socket was still ESTABLISHED 1s after put_object
returned". The variant was reverted (patch hash checked).

Interpretation of the brief's "EOF or reset on its next read": with the peer not reading, the
client's FIN sits behind up to ~2.7 MB of queued data, so the peer's literal next read returns
data. Getting EOF/RST on the very next read would need `SO_LINGER 0` on the client socket,
which the SDK connector does not expose. So the test checks the close from the client side
first (i), then lets the peer drain to EOF (ii), all inside the 1 s.

Fixture checks, so a passing run cannot be a curated one:

- scenario 2 asserts `BackedUp::Yes(_)` (the `200` was sent with the client's send queue
  non-empty and still for 200 ms, `wait_backed_up` `:440`). Observed: 2,595,177 bytes.
- 1a, 1b and 2 assert the source had **not** given its whole length at return and that the
  peer did answer; 3 asserts the peer never answered; the boundary asserts all 10 bytes were
  given before the answer.
- 1b opens the source's failure gate only after the peer's own socket shows the `200`
  acknowledged by the client's kernel (`wait_delivered`, `:487`). Without that, the gate's
  wake-up can reach the client before the `200` is readable there, and the outcome flips to
  `SourceFailed` (I saw exactly that flip in the throwaway inline variant).

Body error check: the test reads the request id from `Display`
(`contains("x-amz-request-id early")`) and matches `S3Error::Body(_)`. It cannot name
`AcknowledgedEarly` without failing to compile on the base, which would turn C4's red leg into
"nothing ran". The module doc says so.

`/proc/net/tcp` is Linux-only. Without it, (i) and the back-up wait degrade to "unobservable"
with fixed waits rather than failing; on Linux (CI) they are enforced.

## Refute your own test (forced)

- **(a) Genuine red? Yes.** Reverted only `crates/validate/src` to the base
  (`git checkout -- crates/validate/src`, test kept), ran the final test: 5/5 FAILED, each on
  the assertion in the table above. Re-applied the saved production diff (md5
  `fda395c7479f3173198787240a08b341` before and after), 5/5 green. Done twice: once after the
  first green, once with the final test file.
- **(b) Production path? Yes.** The test builds the client with `resolve_config` +
  `S3Client::with_deadlines` (the binary's path) and calls `S3Client::put_object` with a
  `PutSource`: the real `aws-sdk-s3`, the real hyper connector, real loopback TCP. No mock, no
  copy. The only scripted part is the peer, which is the fault being injected.
- **(c) Fixture includes the fault? Yes.** The peer really answers early / stops reading /
  never answers; scenario 2 asserts the client's writes were backed up when the `200` went out
  (observed 2.6 MB queued); the close check observes the client's real socket. The weaker-fix
  experiment above shows the fixture catches the round-3 trap class.

Green-run observations (fix applied, `--test-threads=1`):

| Test | Outcome | Given at return | Dropped at return | Client socket left ESTABLISHED | Peer |
|---|---|---|---|---|---|
| 1a | `AcknowledgedEarly { 67108864, 3145728, "early" }` | 3 MiB | yes | 1.8 ms | EOF after 2.7 MB |
| 1b | `AcknowledgedEarly { 10, 4, "early" }` | 4 | yes | 1.1 ms | EOF |
| 2 | `AcknowledgedEarly { 536870912, 33554432, "early" }` | 32 MiB | yes | 1.4 ms | EOF after 2.7 MB |
| 3 | `Timeout { Operation, 3s }` at 3.006 s | 32 MiB | yes | 1.3 ms | EOF after 2.7 MB |
| boundary | `Ok(PutOutcome { etag: None })` | 10 | yes | 1.4 ms | EOF after 207 B |

Stability: 5 consecutive runs green; 3 copies of the test binary in parallel, all green.
Scenario 2 takes ~335 ms to back up, against the 2 s back-up wait and the 3 s `T_op`.

## Gates run locally (commit-readiness)

- `cargo fmt --all -- --check`: clean (ran `cargo fmt -p wyrd-validate`).
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints deny warnings): clean.
- `RUSTDOCFLAGS="-D warnings --document-private-items" cargo doc -p wyrd-validate --no-deps`:
  clean.
- `typos crates/validate`: clean.
- `./engine/xtask.sh statics` (ADR-0035) and `./engine/xtask.sh blackbox-guard` (#775): pass.
- `cargo test -p wyrd-validate`: 38/38 (cli_surface 19, s3_client_roundtrip 14 — including
  #852's deadline and always-ready-empty-source tests, which now run on the PUT's own
  runtime — and the 5 new).
- No dependency change, so cargo-deny / cargo-machete are unaffected. No commit hooks are
  configured in the target (`core.hooksPath` unset, no non-sample hooks).
- Not run: the full `cargo xtask ci` (workspace tests + DST). C4-ci covers it. Nothing outside
  `crates/validate/` depends on the library API changed here (xtask references
  `wyrd-validate` only as a package/binary name).

## Rubric self-review

- One clock per lifecycle: a PUT's connect and operation deadlines both sleep on its own
  runtime's timer; doc updated (`s3.rs:50-57`). No new clock reads.
- Narrow seams / dependency direction: no new dependencies; still no `wyrd-*` in the normal
  graph (blackbox guard passes).
- `#![forbid(unsafe_code)]`: the new test file has it; no new crate.
- Docs currency: no port, API operation, RPC, CLI flag or persisted field changed. The living
  architecture doc names `wyrd-validate` only at crate level
  (`docs/design/architecture/05-building-block-view.md:251`); proposal 0017 §7
  (`0017-blackbox-validation-tool.md:478-489`) classifies failures by class, not by client
  variant. No doc edit (it would also be outside scope).
- Absent/unsupported entries ("never silent success"): this is the fix. An early success is
  now an error.
- Await discipline: the `outcome_rx.await` is bounded by the SDK operation deadline that runs
  inside the PUT's runtime; the helper thread is abandoned on drop through `_abandon`. Residual
  (pre-existing, not new): a caller source whose `poll_next` blocks the thread would hold the
  PUT thread; before this change it blocked the caller's runtime instead.
- Test fidelity / DST: `wyrd-validate` is an out-of-process tool on real tokio and the AWS SDK,
  not part of the DST build (`xtask dst` builds `wyrd-dst`). The "new concurrent path lands
  with seeded Tier-0 DST coverage" rule does not fit a client-side helper thread; flagging it
  so the human can confirm that reading.

## Costs of the chosen approach (stated, not hidden)

- One OS thread and one current-thread tokio runtime for the length of each PUT.
- No connection reuse for PUTs: each PUT opens a fresh TCP connection, because the invariant
  requires the connection closed at return. GET/DELETE are unchanged.
- The `expect` at `s3.rs:370-373` panics in the caller only if the PUT thread itself panicked
  (its panic is printed on that thread). I chose a panic over mapping a bug onto an
  `availability`-class error that the validator would budget.

## Alternatives ruled out

| Alternative | Why not |
|---|---|
| Wake the body poller at return (round 3) | A connection blocked mid-write never polls the body (`dispatch.rs:377-378`). The brief's self-test. |
| Drop the source at return from a shared slot, leave the connection | Tried as a throwaway: source dropped, but the client socket stays ESTABLISHED (scenarios 1a, 2 red on check (i)). Restores half the invariant. |
| Wrap the TCP stream in the SDK connector (abort handle in the IO) | `wrap_connector` is `pub(crate)` (`aws-smithy-http-client-1.4.2/src/client.rs:208`). Needs a fork. |
| Custom hyper executor that tracks connection tasks | `hyper_builder` is `pub(crate)` (`:472`); the executor is fixed at `:541`. Needs a fork. |
| Own `HttpConnector` on hyper's `client::conn::http1` | Direct `hyper`/`hyper-util` deps: edits to root `Cargo.toml`, `crates/validate/Cargo.toml`, `Cargo.lock`, plus an ADR-0003 note; a different HTTP seam. Out of scope; Plan question per the brief. |
| Return an error from the body after the answer (instead of `Pending`) | hyper would then fail the connection and the response body with it, degrading early error responses (e.g. `403`) from typed service errors to dispatch errors. |
| Judge the receipt after `send()` returns (no interceptor) | Lenient by the time the SDK spends reading and parsing the response body, and the raw `x-amz-request-id` is not reachable from `PutObjectOutput` (its accessor prefers `x-amzn-requestid`, the #852 decoy rule). |

## Scratch

Two files in `$PDCA_SCRATCH` (`pdca-builder-854-prod.diff`, `pdca-builder-854-test.rs.bak`),
used to revert/restore during the red leg. Left for the harness to reclaim with the scratch
root; no other files created outside the worktree and this bundle.
