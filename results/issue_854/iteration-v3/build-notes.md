# Build notes — #854 validate-s3-client-upload-against-misbehaving-peers (iteration 3)

Base: `df68932f2c633586fc2ce60cc418f878f8010d7d`, the integration branch
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (the bundle's `stack-base`), with
#852 folded in at `d9c6225`. `patch.diff` applies cleanly on it (`git apply --cached --check`
against a temporary index of that tree). All `path:line` below are on the patched tree unless
marked "base".

Artifacts: `patch.diff` (3 production files, the new test, one architecture doc), the test
copied as `s3_client_upload_peers.rs`, this file.

## What this iteration changed, in one paragraph

The production code is **byte-identical to iteration 2** (`s3.rs`, `s3/body.rs`, `s3/error.rs`:
per-file comparison of the two `patch.diff`s, all three "same"). The adversary attacked it
directly and could not break it. Every iteration-2 finding was about the test or the docs:
two flaky held-source tests, no committed cancellation regression, and the missing
architecture-doc update. This iteration rewrites the held-source fixture so the ordering it
claims is built into it, adds the cancellation test, and updates
`docs/design/architecture/05-building-block-view.md:255`.

## The carry-forward, finding by finding

| Finding (iteration 2) | What I did |
|---|---|
| **C5 / adversary: the two held-source tests were flaky** (1/300 alone; 21/300 in parallel). The fixture released the source *before* writing the answer, so any other wake could let the source move on before the answer existed, and a receipt then was legitimate. | Fixed in the fixture. The peer no longer releases the source at all. The source checks, at every poll, whether the client has **read** the answer off the socket: the peer has written it, then the peer socket's send queue is empty (the client's kernel acknowledged every byte), then the client socket's receive queue is empty (`Hold::answer_read`, test file `:233`). Only hyper reads that socket, and hyper reads, parses and hands over a response head in one call before it polls the body (`hyper-1.10.1/src/proto/h1/io.rs:183-219`, `dispatch.rs:173-174`, checked in the locked source). So the source can go on only after hyper has handed the response over. Results below: 0 failures in 6,600 loaded runs with the fix; 100% red with the ordering guard turned off. |
| **T5 / code review: no committed cancellation regression** for the abandonment branch (`s3.rs:365-367`). | New test `a_put_dropped_while_blocked_mid_write_releases_the_upload` (`:894`): a 512 MiB upload in 32 MiB pieces, the peer never reads or answers; once the client's send queue is non-empty and steady, the test drops the `put_object` future and requires the source dropped within 1 s of the drop and the client socket closed within 1 s while the peer still reads nothing, then EOF or reset. It is green on the base (a guard, as the iteration-2 adversary predicted) and red when the abandonment branch is broken (below). |
| **Docs currency (MUST), 3 of the 6 T4 blocking findings.** | `docs/design/architecture/05-building-block-view.md:255` now states the PUT receipt rule, `AcknowledgedEarly` with the answer's request id, that the source is not polled after the answer, that given-but-unread bytes count (the client can't see what the server read), and the per-PUT thread, runtime and unpooled connection with their cost (`TIME_WAIT`). See "Scope conflict" below. |
| **T4 Contribution: #741's unpublished diffs were not available to the reviewer.** | Paths, for the human: `results/issue_741/iteration-v{1,2,3}/patch.diff` in this harness repo. I listed only the files each touches (not their content, per the builder's narrow-input rule): all three rebuild the whole client in a different layout (`crates/validate/src/client.rs`, `error.rs`, `lib.rs`, new workspace dependencies, `deny.toml`, `deny-all-features.toml`, `Cargo.lock`, the round-trip test, the architecture doc). This patch builds on #852's `s3.rs`/`s3/body.rs` and adds no dependency. Published prior art was already checked by file path in iteration 2 (no closed or merged PR touches the upload path). |
| **C4 diff coverage: "patch.diff does not apply on origin/main".** | Not a patch defect. The bundle is stacked on #852, which isn't on `origin/main`; the patch applies on the stack base. Same as iteration 2. |
| **C5 mutants: 2 missed, both `ResponseArrival::name` → `""` / `"xyzzy"` (`s3.rs:398`).** | Left as is. The SDK shows an interceptor's name only in the error it builds when that interceptor fails; this one always returns `Ok`. They are equivalent mutants. The repo's place for those is `.cargo/mutants.toml` `exclude_re`, which is outside `crates/validate/` (the brief's scope), so I did not add an entry. A human can add `'s3\.rs:398:.*: replace <impl Intercept for ResponseArrival>::name'` if wanted. |

## Decisions for the human (read first)

1. **STOP question, carried from iterations 1-2, still open.** The brief says: if releasing a
   connection blocked mid-write "cannot be done through the SDK's public connector or runtime
   API without forking it, STOP and report". The SDK's **connector** API cannot do it:
   `aws-smithy-http-client` 1.4.2 keeps `wrap_connector`, `hyper_builder` and
   `set_hyper_builder` `pub(crate)` (`src/client.rs:208`, `:472`, `:483`), and fixes the
   executor to `TokioExecutor::new()` (`:541`), whose `execute` is `tokio::spawn`
   (`hyper-util-0.1.20/src/rt/tokio.rs:110-115`). The patch releases the connection through
   the **runtime** the SDK runs on: each PUT runs on a current-thread tokio runtime of its own,
   on a thread of its own (`on_own_runtime`, `s3.rs:341`), shut down before the outcome is
   returned, which drops hyper's connection task, its socket and the request body. No fork. If
   you read "the SDK's runtime API" as the SDK's own runtime traits only, this should have been
   a STOP, and this paragraph is the report.
2. **Per-PUT cost (iteration-2 adversary, NEEDS-HUMAN).** One thread, one runtime and one fresh
   TCP connection per PUT, each leaving a client socket in `TIME_WAIT`. The adversary measured
   about 2x the cost per small PUT on loopback and estimated a ceiling near 470 PUTs/s per
   client IP and endpoint for a non-loopback endpoint. The brief's invariant ("the client has
   closed the connection" when `put_object` returns) forces the per-PUT close. The doc now
   names the cost; whether it fits #743's scenarios is a human call.
3. **Receipt boundary (iteration-2 adversary, NEEDS-HUMAN).** "The response arrived" means
   "hyper read the response head off the socket" (`s3.rs:155-158`). An answer that sits unread
   in the client's kernel buffer while the source gives its last bytes is a receipt. Closing
   that window needs a look at the socket before each piece, which means a custom connector, a
   new HTTP seam the brief makes a Plan question. Not changed.
4. **Scope conflict: docs outside `crates/validate/`.** The brief's scope says "any edit
   outside `crates/validate/`" is out of scope; the repo rubric makes the architecture-doc
   update a merge requirement for a changed API operation, and the iteration-2 carry-forward
   lists the missing doc as a finding to address. I followed the carry-forward and the rubric:
   one paragraph, `docs/design/architecture/05-building-block-view.md:255`, nothing else
   outside the crate. If you disagree, drop that hunk and record the T4 docs findings as
   rejected in `review-rejected.md`.

## Baseline (before any production change) — Falsifiability steps 1-3

1. #852's client is in the base: base `crates/validate/src/s3.rs:126` `put_object`, base
   `:148-149` the only `// deferred: #854` marker (removed by the patch).
2. The final test compiles against the base unchanged. It names only #852's public API
   (`resolve_config`, `Deadlines`, `Phase`, `PutOutcome`, `PutSource`, `ResolvedConfig`,
   `S3Client`, `S3Error`) and existing dev-dependencies.
3. Run on the base (`cargo test -p wyrd-validate --test s3_client_upload_peers`, wrapped in
   `timeout`), production files reverted with `git checkout -- crates/validate/src`:
   **6 red, 1 green**, each red on the assertion the brief predicts.

| Test | Outcome on the base | Verdict |
|---|---|---|
| 1a `an_acknowledgement_before_the_source_is_done_is_the_body_error` (`:790`) | `Ok(PutOutcome)`, 3 MiB of 64 MiB given, source alive at return | red: receipt for an incomplete upload (round 2, reproduced) |
| 1b `a_source_that_fails_after_the_acknowledgement_is_the_body_error` (`:805`) | `Err(Body(SourceFailed { produced: 4 }))`, no request id | red: the answer's request id is lost |
| 1c `a_final_piece_ready_once_the_client_has_read_the_acknowledgement_is_the_body_error` (`:819`) | `Ok(PutOutcome)`, all 10 bytes given after the client read the answer | red: receipt |
| 2 `an_acknowledgement_under_backpressure_is_the_body_error_and_releases_the_upload` (`:835`) | `Ok` after 308 ms; 32 MiB of 512 MiB given; source alive at return; fixture `BackedUp::Yes(2500849)` | red: **retention reproduced** on #852 (round 3 F2), a real red, not green-only |
| 3 `a_peer_that_stops_reading_and_never_answers_is_the_operation_timeout` (`:861`) | `Err(Timeout { Operation, 3s })` (right), source alive at return | red on the no-grace "dropped at return" check |
| boundary `a_source_fully_taken_before_the_acknowledgement_is_a_receipt` (`:950`) | receipt (right), client socket still `ESTABLISHED` 1 s later | red only on the lifetime part: #852 pools the connection |
| cancel `a_put_dropped_while_blocked_mid_write_releases_the_upload` (`:894`) | source dropped and socket closed promptly after the drop | **green**: a regression guard for the patch's abandonment branch, as declared above |

## The test changes

### The held-source fixture (1b, 1c)

- `Hold` (`:218`) and `Hold::answer_read` (`:233`): the source, once it has given 4 bytes, goes
  on only at a poll that finds (a) the peer has started its write (`answering`), (b) that write
  has returned (`answered`; a bounded busy-wait of up to 100 ms covers the peer thread being
  descheduled between its syscall and the flag store), (c) the peer socket's send queue is 0,
  read from `/proc/net/tcp`, and then, from a second read, (d) the client socket's receive
  queue is 0. (c) before (d) rules out "receive queue empty because the answer hasn't
  arrived". The peer sets (a) and (b) around its write (`:512`, `:519`).
- **The source does not wake itself.** My first version of this fixture did wake itself on
  every poll, to be polled as soon as possible. Stressed, it went red 3 times in 600 with the
  fix: `Timeout { Operation }`, given 4, and the peer saw a reset, so the client never read the
  answer. Cause: a single `/proc/net/tcp` read costs about 0.9 ms even on an idle host
  (measured, 50 reads, 38 lines), the source did two per poll, and a current-thread tokio
  runtime polls its I/O driver only every 61 task polls while a task keeps waking itself; under
  load that delayed hyper's read of the answer past the 3 s deadline. Without the self-wake no
  wake is needed: while the body waits, hyper is registered on the socket's read readiness, so
  the answer's arrival wakes the connection and hyper polls the body in that same poll after
  reading. Early polls for other reasons are harmless (they find the answer unread). The doc
  comment on `Hold` (`:194-216`) says this.
- Fixture assertion: `assert_held_before_answer` (`:758`) requires the source parked at its hold
  before the peer answered.

### The cancellation test

`a_put_dropped_while_blocked_mid_write_releases_the_upload` (`:894`), peer script
`Answer::NeverTellingBackedUp`. It runs with its own operation deadline, `DROPPED_T_OP = 60 s`
(`:98`), and a 10 s backup wait (`:126`). Why: the brief's fixed `T_op = 3 s` contract covers
scenarios 1-3; this test is extra, and with 3 s it flaked 3/120 under full CPU load because the
client took 1.5-1.7 s to back up and the fixture guard (drop at least 1.5 s before the deadline)
tripped. Production was fine in all three (source released 0.2-6 ms after the drop). With a
60 s deadline nothing but the drop can release the source within the 1 s bound. The guard is
kept (`:935`).

### Smaller changes

- `TcpEntry` gains `recv_queue` (`:618`); `client()` takes the operation deadline (`:166`);
  the probe records *when* the source was dropped (`dropped_at`), so the cancellation test can
  measure the release from the drop; `assert_connection_closed` (`:728`) is factored out of
  `assert_lifetime` (`:712`) for the dropped call.

## Stress evidence (fixed production unless stated)

Runs of the built test binary, each wrapped in `timeout`, with `xargs -P`. "Loaded" = 32
busy-loop processes on this 32-core host, each bounded by `timeout`.

| What | Runs | Parallel | Load | Failures |
|---|---|---|---|---|
| 1c alone | 600 | 60 | — | 0 |
| 1b + 1c per run | 1800 | 120 | — | 0 |
| 1b + 1c per run | 1800 | 120 | loaded | 0 |
| 1b + 1c per run, final file | 1200 | 120 | loaded | 0 |
| whole file (7 tests) | 60 | 20 | — | 0 |
| whole file, after the 60 s change | 240 | 40 | loaded | 0 |
| whole file, final file | 120 | 40 | loaded | 0 |

## Refute your own test (forced)

- **(a) Genuine red? Yes.** Final test file, production reverted to the base: 6 of 7 red as in
  the table above; the cancellation test green there by design (it guards the patch's own
  abandonment branch). Production restored from the saved diff (md5
  `a81cb8b57e7a8c509004ad955e682c52` before and after every revert). Targeted mutations of the
  patch, each run then reverted:

  | Mutation | Result |
  |---|---|
  | Ordering guard off (`if false && upload.turn.body_waits(cx)`, `body.rs:294`) | 1c red **600/600** serial and **1800/1800** at 120-way; every one a receipt with 10 bytes given. 1b stays green (below). |
  | Abandonment ignored (`select` against `future::pending()` instead of `abandoned`, `s3.rs:365`) | cancel test red: `source_released: None`, `client_close: Not`. The other 6 stay green. |
  | Outcome ranking swapped (source failure checked before the early acknowledgement, `s3.rs:198-207`) | 1b green: with the guard on, the source is never polled after the answer, so it never fails. |
  | Guard off **and** ranking swapped | 1b red **200/200**. |

  So 1b is held by two independent mechanisms (the guard stops the source being polled after
  the answer; the ranking puts the early acknowledgement ahead of a failure); it goes red on
  the base and when both break, not when one does. I left it that way: it states the brief's
  scenario-1 sentence ("So does a source that fails after the acknowledgement") as an outcome.
- **(b) Production path? Yes.** The tests build the client with `resolve_config` +
  `S3Client::with_deadlines` and call `S3Client::put_object` with a `PutSource`: the real
  `aws-sdk-s3`, the real hyper connector, real loopback TCP. No mock, no copy. Only the peer
  and the source are scripted, and those are the injected fault.
- **(c) Fixture includes the fault? Yes.** The peer really answers early, stops reading, or
  never answers. Scenario 2 and the cancellation test assert the client's writes were backed up
  (about 2.5 MB queued, `BackedUp::Yes`). 1b and 1c assert the source was held with bytes left
  when the peer answered, and the source can only go on after the client has read the answer.
  The close check reads the client's real socket.

## Gates run locally (commit-readiness)

- `cargo fmt --all -- --check`: clean (after `cargo fmt -p wyrd-validate` rewrapped one line).
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings` (forced re-check by touching
  the files; workspace lints apply): clean.
- `RUSTDOCFLAGS="-D warnings --document-private-items" cargo doc -p wyrd-validate --no-deps`:
  clean.
- `cargo xtask statics` (ADR-0035) and `cargo xtask blackbox-guard` (#775): pass.
- `typos` on the doc and `crates/validate`: clean.
- `python3 docs/publishing/tools/lint_docs.py`: OK; `render_site.py --check` (output to
  scratch): link audit OK. These are the docs-check workflow's two steps.
- `cargo test -p wyrd-validate`: 40/40 (cli_surface 19, s3_client_roundtrip 14 against the real
  in-process Wyrd gateway, the 7 new).
- No commit hooks in the target (`core.hooksPath` unset, no non-sample hooks, no pre-commit
  config).
- Not run: the full `cargo xtask ci`. Its steps are the three repo guards, fmt, workspace
  clippy/build/test and feature-gated checks; it has no docs step. The production files are
  identical to iteration 2's, whose C4-ci passed; C4-ci reruns it.

## Rubric self-review

- One clock per lifecycle: no new production clock reads. The test reads `Instant::now()` only
  to time its own bounds.
- Narrow seams / dependency direction: no new dependency; blackbox guard passes; no new HTTP
  seam.
- `#![forbid(unsafe_code)]`: on the test crate root; no `unsafe`.
- Docs currency: done this iteration (`05-building-block-view.md:255`).
- Absent/unsupported entries: the fix itself ("never silent success").
- Await discipline: every PUT and the dropped PUT are bounded by the test's own timeouts and
  `CLOSE_BOUND`. The busy-wait in `answer_read` is bounded by `ANSWER_WRITE_WAIT` (100 ms).
- Test fidelity / DST: `wyrd-validate` is an out-of-process client on real tokio and the AWS
  SDK, not part of the DST build. I read "a new destructive or concurrent path lands with
  seeded Tier-0 DST coverage" as not applying to it; flagging that reading for the human.
- Recurring classes "protocol input", "grammar strictness", "serialization identity",
  "transactions", "probes": not touched.

## Alternatives ruled out (with cost)

| Alternative | Why not |
|---|---|
| The adversary's suggested fixture: the source self-wakes until the client's receive queue is back to 0 | Tried as my first version (plus the delivered check). 3/600 timeouts under 60-way load, explained above. Dropping the self-wake is a 3-line change and removed the failures. |
| Keep the iteration-2 waker-less hold, but release it after the answer is written | The release would still be a peer-side timing decision: the peer sees "written", not "read by the client", so a release could still come before hyper read the answer, or after the request had recorded it (no binding). Checking "read" inside the source's own poll is what makes the ordering exact. |
| Cancellation test under the brief's `T_op = 3 s` | Flaked 3/120 under load on its own fixture guard (shown above). A 60 s deadline costs nothing (the test never waits for it) and removes the race. |
| Make `ResponseArrival::name` observable to kill the two survivors | No honest way: the name appears only in an error the interceptor never raises. A test asserting the string would restate it. |
| Production change this iteration | None needed; no finding was against production. |

## Scratch

Working files under `$PDCA_SCRATCH/pdca-builder-854-v3/` (saved production diff, test-binary
copies used for stress, stress logs, temporary git indexes, the rendered docs site). Left for
the harness to reclaim with the scratch root, per the harness's filesystem rule. Nothing else
was created outside the worktree and this bundle.
