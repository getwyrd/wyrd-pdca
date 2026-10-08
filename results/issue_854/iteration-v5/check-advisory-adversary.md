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
