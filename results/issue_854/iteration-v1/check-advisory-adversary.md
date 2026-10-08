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
