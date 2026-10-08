# Build notes — #854 validate-s3-client-upload-against-misbehaving-peers (iteration 4)

Base: `df68932f2c633586fc2ce60cc418f878f8010d7d`, the integration branch
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (the bundle's `stack-base`), with
#852 folded in at `d9c6225`. `patch.diff` applies cleanly on it (`git apply --cached --check`
against a temporary index of that tree). All `path:line` below are on the patched tree unless
marked "base".

Artifacts: `patch.diff` (3 production files, the test, one architecture doc), the test copied
as `s3_client_upload_peers.rs`, this file.

## What this iteration changed, in one paragraph

Only the test. The production files and the doc are **byte-identical to iteration 3**
(per-file comparison of the two `patch.diff`s: `s3.rs`, `s3/body.rs`, `s3/error.rs`,
`05-building-block-view.md` all "same"; only `tests/s3_client_upload_peers.rs` differs). The
carry-forward said the production code was right and one guard was untested. This iteration
adds the missing tests (three cases where the early answer's body arrives late, a
"never polled past the hold" check on every held case, and a receipt pin for "whole length
given, end not reported"). While stress-testing them I found the hold fixture could stall the
client under heavy load, and I fixed that in the fixture (details below).

## The carry-forward, finding by finding

| Finding (iteration 3) | What I did |
|---|---|
| **`s3/body.rs:289-291` (the "request has seen the response → `Poll::Pending`" guard) has no test.** With `Content-Length: 0` the request ends in the poll that records the answer, so hyper never polls the body again. | Three new held cases whose answer's body the peer sends `LATE_BODY_DELAY` after the head (test `:138`, `Reply` `:398`, peer `serve` `:549`): a `200` to a source ready to fail (`:965`), a `200` to a source ready with its last 6 bytes (`:974`), and a `403 AccessDenied` whose XML arrives late, to a source ready to fail (`:983`, expects the exact `S3Error::Service { 403, AccessDenied, "Access Denied", early }`). Every held case now also asserts `assert_not_polled_past_the_hold` (`:880`): the source never went on past its hold and `given_at_return == 4`. **With the guard deleted, all three new tests go red 300/300** (table below). The adversary's exact failing case (late `ok`, source fails → `SourceFailed` with no request id) is the first one. |
| Adversary note (non-blocking): the receipt pin covers only one side of the boundary. | New `a_source_that_gave_its_whole_length_but_held_its_end_is_a_receipt` (`:1137`): the source has given all 10 bytes and holds its end-of-stream when the peer answers; the outcome must be a receipt. A rule moved to "the source must have reported its end" or "the bytes must have left the client" fails it. Green on the patch; red on the base only on the lifetime part (#852 pools the connection), like the simple-form pin. |
| **T4 Contribution: #741's unpublished diffs were not in the reviewer's evidence.** | Not something the builder can supply without reading another issue's bundle (the narrow-input rule). Paths, for the human: `results/issue_741/iteration-v{1,2,3}/patch.diff` in this harness repo. Iteration 3 listed the files those touch (a rebuilt client in a different layout: `client.rs`, `error.rs`, new workspace dependencies, `deny.toml`, `Cargo.lock`); this patch builds on #852's `s3.rs`/`s3/body.rs` and adds no dependency. This needs the harness to hand those diffs to the reviewer, or the human to judge. |
| **C4 diff coverage: "patch.diff does not apply on origin/main".** | Not a patch defect: the bundle is stacked on #852, which isn't on `origin/main`. It applies on the stack base (checked). Same as iterations 2-3. |
| **C5 mutants: 2 missed, both `ResponseArrival::name` (`s3.rs:398`).** | Equivalent mutants: the SDK shows an interceptor's name only in an error it builds when that interceptor fails, and this one always returns `Ok`. The repo's place for those is `.cargo/mutants.toml` `exclude_re` (each entry "verified equivalent by hand"), which is outside `crates/validate/`, the brief's scope. Suggested entry for the human: `'s3\.rs:398:.*: replace <impl Intercept for ResponseArrival>::name'`. |
| Adversary: the C4-verify headline "7 test(s) ran red" was wrong (one test was green on base). | Harness reporting, not the patch. Now: 10 of 11 red on base, 1 green (the dropped-PUT test, a declared guard). |

## The fixture fix (found this iteration, not in the carry-forward)

**What failed.** Stress-testing the new tests (2,400 runs of the six held tests, 120 processes
at once, 32 busy loops on this 32-core host) gave 33 failures, every one `Timeout { Operation }`
with the peer's answer still unread in the client's socket (the client closed with a reset,
`drained: 0`), some calls not returning by 4 s. The iteration-3 test binary passed the same
load for its two held tests (0/1200), but with a much smaller TCP table (2,274 entries vs
18,082).

**Cause, measured.** The hold check (`Hold::answer_read`, `:275`) reads `/proc/net/tcp`, the
kernel's whole TCP table, inside the source's poll, which runs on the client's only thread.
hyper's connection task is sometimes polled after the answer has arrived but before tokio has
seen it arrive (logged timelines show a second body poll at the hold, from another wake of the
connection; I believe socket write-readiness, not proven). In that poll the check finds the
answer unread, but while it reads the table hyper cannot read the socket. Logged: this
happened in **~11% of held runs even with no load** (405 of 3,600, each finding 63 or 72
unread bytes), harmless at ~1 ms per read. Under my stress the table grew to 18,000 entries
because each test connection left a client socket in `TIME_WAIT` for 60 s; reads then took a
median 203 ms, up to 957 ms, and 211 of 425 such checks took over 1 s. That blocks the client
past `T_OP`.

**What I changed (test file only):**

1. The peer sets zero linger on its socket (`serve`, `:554`, `TcpStream::set_zero_linger`).
   It drops the socket only after it has watched the call end and the connection close, so
   nothing the test checks changes; the reset just means neither end stays in `TIME_WAIT`.
   Measured: 0 `TIME_WAIT` entries after 5 full runs (55 PUTs), where before every PUT left one.
2. The check reads the client's receive queue first and stops if it is non-empty
   (`:296`): unread bytes can only be the answer. The usual "polled before hyper read it" case
   now costs one table read instead of two. The success path still requires delivered-then-read
   in that order, so the ordering guarantee is unchanged.
3. `wait_backed_up` (`:651`) times "held still for 200 ms" by the clock, not by counting 8
   samples. With a large table, 8 samples of a 350 ms read don't fit in the 2 s wait; that made
   scenario 2's fixture check fail (116 of 240 whole-file runs, outcome correct in all of them)
   when another program had filled the table. This code is from iteration 3; same weakness.
4. `LATE_BODY_DELAY` is 500 ms, not 200 ms. On a crowded table one check can outlast 200 ms and
   find the late body unread; the source then stays held, so the guard-off mutant is missed
   (measured 82-85% catch per late test while a ~12k-entry table was still draining). That can
   only hide a broken client, never fail a correct one. No wall-time cost: the 3 s
   timeout test sets the file's duration.

`set_zero_linger` was added in tokio 1.50.0; the lockfile pins 1.52.3 and no gate builds with
minimal versions (searched the repo: no `minimal-versions`). The workspace declares
`tokio = "1"`; if that matters to you, the deprecated `set_linger(Some(Duration::ZERO))` with
`#[allow(deprecated)]` works on every 1.x.

## Baseline (Falsifiability steps 1-3), final test file

1. #852's client is in the base: base `crates/validate/src/s3.rs:126` `put_object`, base
   `:148-149` the only `// deferred: #854` marker (removed by the patch).
2. The test compiles against the base unchanged. It names only #852's public API
   (`resolve_config`, `Deadlines`, `ErrorCode`, `Phase`, `PutOutcome`, `PutSource`,
   `ResolvedConfig`, `S3Client`, `S3Error`; `ErrorCode` and `S3Error::Service` are in base
   `lib.rs:29-31` and `s3/error.rs`) and existing dependencies.
3. `cargo test -p wyrd-validate --test s3_client_upload_peers` (under `timeout`), production
   reverted with `git checkout -- crates/validate/src docs`: **10 red, 1 green**.

| Test | On the base | Verdict |
|---|---|---|
| 1a `an_acknowledgement_before_the_source_is_done_is_the_body_error` (`:909`) | `Ok(PutOutcome)`, 3 MiB of 64 MiB given | red: receipt (round 2, reproduced) |
| 1b `a_source_that_fails_after_the_acknowledgement_is_the_body_error` (`:939`) | `SourceFailed { produced: 4 }`, no request id | red |
| 1c `a_final_piece_ready_once_the_client_has_read_…` (`:949`) | `Ok(PutOutcome)`, 10 given | red: receipt |
| 1b-late `a_source_that_fails_while_the_acknowledgements_body_is_on_its_way_…` (`:965`) | `SourceFailed { produced: 4 }`, no request id | red |
| 1c-late `a_final_piece_ready_while_the_acknowledgements_body_is_on_its_way_…` (`:974`) | `Ok(PutOutcome)`, 10 given | red: receipt |
| 403-late `a_rejection_whose_body_is_on_its_way_…` (`:983`) | `SourceFailed { produced: 4 }`; the 403 is lost | red |
| 2 `an_acknowledgement_under_backpressure_…` (`:1006`) | `Ok` with 32 MiB of 512 MiB given, `BackedUp::Yes(2500849)` | red: retention reproduced (round 3 F2) |
| 3 `a_peer_that_stops_reading_and_never_answers_…` (`:1032`) | `Timeout { Operation, 3s }` (right), source alive at return | red on "dropped at return" |
| dropped `a_put_dropped_while_blocked_mid_write_…` (`:1065`) | released promptly | **green**: guards the patch's abandonment branch (`s3.rs:365`) |
| pin `a_source_fully_taken_before_the_acknowledgement_is_a_receipt` (`:1121`) | receipt, socket still `ESTABLISHED` | red only on the lifetime part |
| pin `a_source_that_gave_its_whole_length_but_held_its_end_is_a_receipt` (`:1137`) | receipt, socket still `ESTABLISHED` | red only on the lifetime part |

## Refute your own test (forced)

- **(a) Genuine red? Yes.** Final test file, production reverted to the base: 10 of 11 red as
  above; the dropped-PUT test green there by design. Production restored from the saved diff
  (md5 `71d668816f3c3a8e254dfd63a50b81a1` before and after every revert). Targeted breakages of
  the fix, each built into its own test binary and then reverted, run on a normal-size table:

  | Breakage | Result |
  |---|---|
  | Answer guard off (`if false && upload.answer.get().is_some()`, `s3/body.rs:289`) | the three late-body tests red **300/300** each (30-way). The bodiless held tests stay green, as the carry-forward predicted. |
  | Turn guard off (`if false && upload.turn.body_waits(cx)`, `s3/body.rs:294`) | all five held cases red **300/300** each (30-way). |
  | Abandonment ignored (`select` against `future::pending()`, `s3.rs:365`) | dropped-PUT test red **40/40**. |
  | Outcome ranking swapped (source failure checked before the early acknowledgement, `s3.rs:198-207`) | all 11 green 100/100: with the guards on, the source is never polled after the answer, so it never fails then; the ranking is a second line of defence. Same as iteration 3. |

- **(b) Production path? Yes.** The tests build the client with `resolve_config` +
  `S3Client::with_deadlines` and call `S3Client::put_object` with a `PutSource`: the real
  `aws-sdk-s3`, the real hyper connector, real loopback TCP. Only the peer and the source are
  scripted; they are the injected fault.
- **(c) Fixture includes the fault? Yes.** The late-body cases assert the peer actually wrote
  the body after the head (`assert_body_sent_late`, `:865`) and that the source was held, with
  bytes left, before the peer answered (`assert_held_before_answer`, `:855`). The end-held pin
  asserts the source was parked at its declared length when the peer answered
  (`assert_parked_before_answer`, `:843`). Scenario 2 and the dropped test assert the client's
  writes backed up. The close check reads the client's real socket.

## Stress evidence (final binary unless stated)

Runs of the built test binary with `xargs -P`, each under `timeout 60`. "Loaded" = 32 busy
loops (each under `timeout`) on this 32-core host. "Big table" = a generator keeping 10,500-12,800
`TIME_WAIT` sockets in the table during the run (reads up to 360 ms).

| What | Runs | Parallel | Conditions | Failures |
|---|---|---|---|---|
| six held tests, first version (200 ms, no linger, two-read check) | 2400 | 120 | loaded | **33** (the stall above) |
| six held tests, final | 2400 | 120 | loaded | 0 |
| six held tests, final | 2400 | 120 | loaded, big table | 0 |
| whole file, linger fix but count-based backed-up wait | 240 | 40 | loaded, big table | 116 (scenario 2 fixture check only) |
| whole file, final | 240 | 40 | loaded | 0 |
| whole file, final | 240 | 40 | loaded, big table | 0 |
| six held tests, intermediate (linger + one-read check) | 600 / 2400 / 2400 | 60 / 120 / 120 | idle / loaded / loaded, big table | 0 / 0 / 0 |

## Gates run locally (commit-readiness)

- `cargo fmt --all -- --check`: clean (after `cargo fmt -p wyrd-validate` rewrapped one line).
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings` (files touched to force it): clean.
- `RUSTDOCFLAGS="-D warnings --document-private-items" cargo doc -p wyrd-validate --no-deps`: clean.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean.
- `cargo xtask statics` (ADR-0035) and `cargo xtask blackbox-guard` (#775): pass.
- `cargo test -p wyrd-validate`: 44/44 (cli_surface 19, s3_client_roundtrip 14, the 11 here).
- `./engine/xtask.sh ci` (the project's whole gate, `cargo xtask ci` in this worktree, under
  `timeout 5400`): **"xtask ci: all checks passed", exit 0**. All 18 steps ran, the prose gates
  included (`typos`, `lint_docs.py`, `render_site.py --check`), then fmt, workspace
  clippy/build/test (`s3_client_upload_peers`: 11 passed), machete, deny, statics, deploy-guard,
  DST clippy/test.
- No commit hooks in the target (`core.hooksPath` unset, no pre-commit config), as in iteration 3.

## Decisions for the human (carried from iteration 3, unchanged)

1. **STOP question.** The brief says: if releasing a connection blocked mid-write "cannot be
   done through the SDK's public connector or runtime API without forking it, STOP and report".
   The SDK's connector API cannot: `aws-smithy-http-client` 1.4.2 keeps `wrap_connector`,
   `hyper_builder`, `set_hyper_builder` `pub(crate)` (`src/client.rs:208`, `:472`, `:483`) and
   fixes the executor to `TokioExecutor::new()` (`:541`). The patch releases it through the
   **runtime** the SDK runs on: each PUT on a current-thread runtime of its own, on a thread of
   its own (`on_own_runtime`, `s3.rs:341`), shut down before the outcome is returned. No fork.
   If "the SDK's runtime API" means only the SDK's own runtime traits, this should have been a
   STOP, and this paragraph is the report.
2. **Per-PUT cost.** One thread, one runtime and one fresh TCP connection per PUT, each leaving
   a client socket in `TIME_WAIT`. The iteration-3 adversary measured 17,284 → 3,055 PUT/s on
   loopback for 1 KiB PUTs and estimated ~470 new connections/s per client IP and endpoint
   before `EADDRNOTAVAIL` for a non-loopback endpoint. Forced by the brief's "the client has
   closed the connection" on every return. Whether it fits #743's scenarios is your call.
3. **Receipt boundary.** "The response arrived" = "hyper read the response head off the socket"
   (`s3.rs:155-158`). An answer sitting unread in the client's kernel buffer while the source
   gives its last bytes is a receipt. Closing that needs a look at the socket per piece: a
   custom connector, a new HTTP seam (a Plan question per the brief). Not changed.
4. **Scope conflict: docs outside `crates/validate/`.** The brief puts "any edit outside
   `crates/validate/`" out of scope; the rubric makes the architecture-doc update a merge
   requirement. The patch keeps iteration 3's one paragraph at
   `docs/design/architecture/05-building-block-view.md:255`. Drop that hunk if you disagree.

## Rubric self-review

- One clock per lifecycle: no production change. The test reads `Instant::now()` only to time
  its own bounds (including the new wall-clock backed-up wait).
- Narrow seams / dependency direction: no new dependency; blackbox guard passes.
- `#![forbid(unsafe_code)]` on the test crate root; no `unsafe`.
- Docs currency: no API change this iteration; the doc paragraph from iteration 3 stands.
- Absent/unsupported entries: the new tests assert exact outcomes (`assert_eq!` on the full
  `S3Error::Service`, on `given_at_return`, on `went_on`), not counts.
- Await discipline: every PUT bounded by `CALL_BOUND`; the peer's late-body write is bounded
  by the call (the SDK waits for it); the hold's busy-wait by `ANSWER_WRITE_WAIT`.
- Test fidelity: the late-body cases reproduce what a real server can do (a response body that
  trails its head); no mock of the client.
- DST: `wyrd-validate` is an out-of-process client on real tokio and the AWS SDK, not part of
  the DST build; I read the Tier-0 DST rule as not applying, same as iteration 3.

## Alternatives ruled out (with cost)

| Alternative | Why not |
|---|---|
| Move the `/proc/net/tcp` reads off the client thread (a peer-side watcher sets a flag, the source checks an atomic) | Reasoned, not built. It removes the stall entirely, but the watcher samples every few ms and cannot see "hyper has read the head" in the microseconds between hyper's read and its next body poll. That poll is the one that binds the Turn guard (`s3/body.rs:294`, red 300/300 now). The guard round 1 was about would lose its test. |
| Count late-body bytes as allowed in the queues instead of raising `LATE_BODY_DELAY` | Reasoned, not built. For the 403 case the body (137 bytes) is longer than the head (72), so "receive queue ≤ late bytes written" can pass with the head still unread: a false "read" that would let the source go on before the answer and turn a correct client's run red. |
| Keep `LATE_BODY_DELAY = 200 ms` | Works on a normal host (300/300 catch), but a check on a crowded table can outlast it (82-85% catch measured). 500 ms costs no wall time. |
| Leave the fixture as iteration 3 had it | 33/2400 timeouts under heavy load once the table is large; a reviewer's parallel stress run fills the table the same way, because every PUT left a `TIME_WAIT` socket. |
| Production change | None needed: no finding was against production, and the adversary could not break it. |

## Scratch

Working files under `$PDCA_SCRATCH/pdca-builder-854-v4/`: saved production diff, test-binary
copies (fixed, mutants, instrumented), stress scripts and logs, the `TIME_WAIT` generator,
temporary git indexes. The instrumented builds were made in the worktree and reverted
(production md5 checked after each; the test file restored from a saved copy and compared with
`cmp`). Left for the harness to reclaim with the scratch root. Nothing else was created
outside the worktree and this bundle.
