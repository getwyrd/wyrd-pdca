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
