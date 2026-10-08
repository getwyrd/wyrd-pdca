# Build notes — issue 854, revision 5, iteration 6 (round 7 of Do)

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `df68932`.
One `patch.diff` against the base: 5 files, +1960 / −50.

Line numbers are in the patched tree unless marked "base".

## What this iteration is

The sign-off on the previous attempt (`iteration-v6/`) accepted it in substance and asked for one
fix: the dropped-PUT test could hang. So this iteration changes **only the test file, and in it
only the dropped-PUT test and three constants for it**.

- The four production and doc files are byte-identical to the previous attempt. Checked by blob
  hash against `df68932` + `iteration-v6/patch.diff`: `crates/validate/src/s3.rs`,
  `s3/body.rs`, `s3/error.rs`, `docs/design/architecture/05-building-block-view.md`.
- The test file against the previous attempt: +41 / −4 lines (1463 → 1500). The other eleven
  tests and the shared fixture (`Peer`, `serve`, `Hold`, `Source`, the asserts) are untouched.

## The fix

All in `crates/validate/tests/s3_client_upload_peers.rs` (a new file against the base).

**The hang.** `a_put_dropped_while_blocked_mid_write_releases_the_upload` (`:1427`) waits, before
it drops the PUT, for one of two things: the PUT returning (a failure), or the peer saying
whether the client's writes backed up. The peer says so only after it has accepted a connection
(`:629`) and read a request head (`:635`); from there its wait is bounded by
`DROPPED_BACKED_UP_WAIT` (`:662-663`). If the request never reaches the peer and the PUT never
returns, both stay pending and nothing ends the wait.

**The change.**

- `:1437-1445`: the `select!` is wrapped in `tokio::time::timeout(DROPPED_SETUP_BOUND, …)`, on
  the test's own runtime. Same shape as the bound every other test already has in `put_against`
  (`:1031-1044`).
- `:1446-1457`: on expiry the test panics with a message that says what the bound is made of,
  what was still pending, and what the source had done:
  > the PUT never got as far as the drop. 15s after the call began (5s for its request head to
  > reach the peer, then the peer's 10s wait for the client's writes to back up), put_object had
  > not returned and the peer had not said how its wait ended, which it does, whatever it saw,
  > once it has read a request head. The source had given 0 of 536870912 bytes and had not been
  > dropped.
- `:202-206`: `DROPPED_HEAD_WAIT = 5 s`, the allowance for connection setup: the client's
  connection, the request head, and the peer's read of it.
- `:207-211`: `DROPPED_SETUP_BOUND = DROPPED_HEAD_WAIT + DROPPED_BACKED_UP_WAIT` = 15 s, computed
  from the two constants so it cannot drift from them.
- `:212-216`: a compile-time check that `DROPPED_SETUP_BOUND + CLOSE_BOUND < DROPPED_T_OP` (16 s
  against 60 s). If the bound could outlast the PUT's own deadline, the PUT would return on that
  deadline first and the test would fail with the wrong message ("returned before the test
  dropped it").
- `:74-76`: one sentence in the module doc's scenario 5, so "never a hang" is stated for this
  test too.

**Why 5 s for connection setup.** Over loopback the head reaches the peer in milliseconds. The
client's own connect deadline in this test is 2 s (`:258`), so a connect that fails on its
deadline ends the wait through the PUT's own arm before the test's bound does. 5 s leaves room
on top of that for a busy host, and for the peer's backpressure wait running one sample over
its 10 s. In the passing runs the peer reports in well under a second (the whole suite takes
3.03 s). A longer allowance would only delay a failure; a shorter one risks a false failure
under load.

## Decisions, and what I ruled out

**Report the peer's stage in the message (did it accept? did it read the head?).** It would make
the message say directly what it now leaves the reader to conclude. Cost, as a sketch:

```
struct Peer { …, head_read: Arc<AtomicBool> }                    // +2 (field, doc)
Peer::start: let head_read = Arc::new(AtomicBool::new(false));   // +1
             serve(listener, answer, ended_rx, Arc::clone(&head_read))   // 1 changed
             Self { …, head_read }                               // +1
serve(…, head_read: Arc<AtomicBool>)                             // 1 changed
    read_head(&mut stream).await; head_read.store(true, …);      // +1
the test's message reads peer.head_read                          // +2
```

About 9 lines, 6 of them in `Peer` and `serve`, which all 12 tests run through. The sign-off said
to keep the other eleven tests as they are, so I did not touch their fixture. The message
instead states the facts the test itself holds: the PUT had not returned, the peer had said
nothing, and the source had given N bytes. "0 bytes given, not dropped" already says the request
never got as far as its body. If you want the stage flag, it is the sketch above.

**Bound the peer instead** (a timeout in `serve` around `accept` and `read_head`). About 6 lines,
again in the shared `serve`. The failure would then show up in the test as a closed channel
("the peer says whether the client backed up"), which says less than the message above, and
the sign-off asked for a timeout on the test's own runtime.

**A third `select!` arm with `sleep(DROPPED_SETUP_BOUND)`** instead of a wrapping `timeout`. Three
lines against eight. Same behaviour. I used the wrapper because the sign-off says "wrap", and
because it matches `put_against` (`:1031`), so the file has one way of bounding a call.

**A literal `15 s`.** One line shorter. Rejected: the sign-off defines the bound as "connection
setup plus `DROPPED_BACKED_UP_WAIT`", and a literal would go stale if either changes.

**Nothing else.** I did not act on the two advisory gates in the carry-forward (see "For the
human"), and I did not read or act on the findings deferred to sign-off.

## Evidence

All runs go through the project's own gate script, `./engine/scripts/run-verify.sh`, with
`PDCA_LANE=0`, under `timeout` and the Bash tool's own timeout. Logs are in `$PDCA_SCRATCH`
(`pdca-builder-854-r6-*.log`).

### Gate, on the wave base (`PDCA_VERIFY_BASE=origin/pdca-integration/r-a834…/main`)

```
run-verify.sh: GREEN — cargo test -p wyrd-validate --test s3_client_upload_peers (fix applied)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.03s
run-verify.sh: RED — cargo test -p wyrd-validate --test s3_client_upload_peers (production reverted, test kept)
test result: FAILED. 1 passed; 11 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.04s
run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
```

This is the same result as round 5: 12 of 12 green with the fix; on the base 11 fail and the
dropped-PUT test passes. That one pass is the brief's declared green-only guard (hyper releases
a request dropped before its answer through its own cancel path), not a gap. So "all 12 red" is
not what happens, and was not what happened in round 5 either: it is 11 red plus the one
declared exception.

Red on the base (#852's production), per test, read from this run's log. Every failure is an
assertion on the behaviour under test; the file compiles; none is a "cannot run this test"
failure; every report shows `seen_open: true` (11 of 11).

| Test | Outcome on #852 | Fails on |
|---|---|---|
| `an_acknowledgement_before_the_source_is_done_…` | `Ok(PutOutcome)`, 3 MiB of 64 MiB given | never a receipt |
| `a_source_that_fails_after_the_acknowledgement_…` | `Body(SourceFailed { produced: 4 })` | not `AcknowledgedEarly` |
| `a_final_piece_ready_once_the_client_has_read_…` | `Ok(PutOutcome)` | never a receipt |
| `a_source_that_fails_while_the_acknowledgements_body_…` | `Body(SourceFailed { produced: 4 })` | not `AcknowledgedEarly` |
| `a_final_piece_ready_while_the_acknowledgements_body_…` | `Ok(PutOutcome)` | never a receipt |
| `a_rejection_whose_body_is_on_its_way_…` | `Body(SourceFailed { produced: 4 })` | not the `403 AccessDenied` service error |
| `an_acknowledgement_under_backpressure_…` | `Ok(PutOutcome)`, 32 MiB of 512 MiB, `BackedUp::Yes(2500849)` | never a receipt |
| `a_peer_that_stops_reading_and_never_answers_…` | `Timeout { Operation, 3s }` (right) | source still alive at return |
| 4(a) `a_source_that_ended_before_…_is_a_receipt` | `Ok(PutOutcome)` (right) | `client_close: Not`, `StillOpen` (lifetime only, as declared) |
| 4(b) `…_held_its_end_is_not_a_receipt` | `Ok(PutOutcome)` | never a receipt |
| 4(c) `…_held_an_extra_byte_is_not_a_receipt` | `Body(SourceLength { declared: 10, produced: 11 })` | not `AcknowledgedEarly` |
| scenario 5 `a_put_dropped_while_blocked_mid_write_…` | passes | declared green-only |

### The new bound does its job (a probe, not shipped)

The dropped-PUT test is green on the base, so the gate's red leg says nothing about the new
bound. To show the bound binds, I broke the request's wake-up on purpose: one line removed from
this patch's production code in a scratch copy of the patch, `self.request.wake()` in
`Turn::wake_by_ref` (`crates/validate/src/s3/body.rs:218`). The request is then never polled
again after its first pending poll, so it never sends its head, and its own deadline never
fires either (the deadline's wake goes through the same waker). This is the case the sign-off
names: a request-wakeup regression before the peer reads the head.

| Test file | Result with the wake-up broken |
|---|---|
| Round 5's test (`iteration-v6/patch.diff`) | **Hangs.** The test process was still alive at every sample up to 133 s, and went away only when my 150 s cap killed the run (`timeout` exit 124). A second run recorded thread names: alive at 84 s with three threads, `a_put_dropped_w…` (the test), `wyrd-validate-p…` (the PUT's thread) and the main thread; killed by a 90 s cap. Both runs outlast the PUT's own 60 s deadline, so that deadline does not end the wait. |
| This patch's test | **Fails in 15.03 s**, 12 of 12. The dropped-PUT test panics at `:1447` with the message quoted above ("… The source had given 0 of 536870912 bytes and had not been dropped."). The other eleven fail on their own bound ("put_object did not return within 4s"), as before. |

Limits of this probe, stated plainly:

- "Hangs" means "did not end within 150 s". I did not wait longer.
- The gate script prints the test output only when the run ends, so the killed runs left no
  test output. That the hanging test is the dropped-PUT one comes from the thread names in the
  second run, read from `/proc/<pid>/task/*/comm`.
- One fault was injected. Any other fault that leaves both arms pending ends the same way,
  because the bound does not look at the cause.
- A PUT that blocks the test's thread without yielding would not be caught by any timeout on
  that runtime. Nothing in the client does that; the request runs on its own thread
  (`crates/validate/src/s3.rs:372`).

No test process was left behind after either killed run (checked with `ps`).

### Second red: against round 4's production code, unchanged (required by the brief)

Base for this run: commit `4536591` = `df68932` + round 4's production and doc files, no test.
An earlier round made it; I checked it before reuse (the four files' blob hashes equal
`df68932` + `iteration-v4/patch.diff`, and it holds no test file). The gate ran with that commit
as the base and a scratch patch from it to this tree, so its red leg is "round 4's production +
this test".

```
run-verify.sh: GREEN — … (fix applied)
test result: ok. 12 passed; 0 failed; … finished in 3.03s
run-verify.sh: RED — … (production reverted, test kept)
a_source_that_gave_its_declared_length_and_held_an_extra_byte_is_not_a_receipt --- FAILED
a_source_that_gave_its_whole_length_but_held_its_end_is_not_a_receipt --- FAILED
test result: FAILED. 10 passed; 2 failed; … finished in 3.04s
```

Both fail on "never a receipt": `outcome: Ok(PutOutcome { etag: None })`, `given_at_return: 10`,
`ended_at_return: false`, `parked: BeforeAnswer`, `seen_open: true`. Against round 4, 4(b) and
4(c) are each a receipt and the other ten pass. The rule change is what turns them.

### Stability

The gate six times on the final test: six of six were 12/12 green and 11/12 red, each leg in
3.03 to 3.05 s. Five of those ran while `xtask ci` was building and testing the workspace in
the other worktree (load about 1; I added no synthetic load). The new bound was never reached
in a passing run: it only fires 15 s in, and the suite takes 3 s.

Not re-run this iteration: round 5's four probes for a host that hides `/proc/net/tcp`
(`iteration-v6/build-notes.md:189-204`). The code they exercise is unchanged.

### Whole-tree gate

`./engine/xtask.sh ci` (the project's `cargo xtask ci`, in `$PDCA_WORKTREE`, on the final tree):
`xtask ci: all checks passed`, exit 0. It ran `typos`, docs lint, docs render check,
gitlink-guard, unsafe-guard, blackbox-guard, `cargo fmt --all -- --check`, clippy on all
targets, the workspace build and tests, `cargo-machete`, `cargo deny`, conformance (5 valid + 6
invalid vectors), the statics gate, deploy-guard, and clippy + tests for `wyrd-dst` under
`--cfg madsim`. Inside it `s3_client_upload_peers` ran 12/12 in 3.04 s and `s3_client_roundtrip`
14/14.

The target repo has no commit hooks (no `.pre-commit-config.yaml`, no `core.hooksPath`, only
sample files in `.git/hooks`) and no `rustfmt.toml`, so the default formatter plus `xtask ci`
is the commit bar. Both are clean.

## Refuting my own test

- **(a) Genuine red?** Yes, on three counts, all run on this final test. With production
  reverted to the base, 11 of 12 fail by assertion; the twelfth is the brief's declared
  green-only guard. With production reverted only to round 4, exactly the two new-rule cases
  fail. And for this iteration's one change: with the request's wake-up broken, round 5's test
  hangs and this one fails in 15 s with the new message.
- **(b) Production path?** Yes. Every test goes through `resolve_config` →
  `S3Client::with_deadlines` → `S3Client::put_object`, the real `aws-sdk-s3` and hyper
  connector, over real loopback TCP. Only the peer is scripted, and the peer is the fault. The
  new bound wraps that same production call; the probe's fault was put into the production
  code, not into a stand-in.
- **(c) Fixture includes the fault?** Yes, and each test asserts it: the peer answered
  (`assert_answered`); the source was at its hold before the answer
  (`assert_parked_before_answer`, `:1125`); it was never polled past it
  (`assert_not_polled_past_the_hold`, `:1167`, `at = 10`); the backpressure cases require
  `BackedUp::Yes` (`:1481` for the dropped PUT); the late-body cases require the body written
  after the head. The dropped-PUT test still requires a client that was seen blocked mid-write
  with bytes left before the drop; the new bound only decides what happens when the test never
  gets that far.

## What the earlier rounds built (unchanged here, restated so the citations are in one place)

- Receipt rule, "the source ended before the response arrived":
  `crates/validate/src/s3/body.rs:88-90` (`Upload::ended`), `:121-129` (`Upload::answered`
  records it), `:149-159` (`Upload::acknowledged_early`), `:296` and `:337` (where the body
  reads and sets it). Base hunks: `s3/body.rs:55-67`, `:75` and `:86-143`.
- The order between request and body (`Turn`, `RequestFirst`): `s3/body.rs:162-254` (added at
  base `:67`).
- The error: `crates/validate/src/s3/error.rs:95-108` (`AcknowledgedEarly`, base `:92`),
  `:197-208` (`Display`, base `:180`), `:49-51` (`RequestNotBuilt` doc, base `:49`).
- `put_object`: `crates/validate/src/s3.rs:154-208` (doc: receipt, limit, partial future
  backstop, lifetime, per-PUT cost; base `:125`), `:209-245` (code; base `:133-149`),
  `:372-417` (`on_own_runtime`, base `:264`), `:427-443` (`ResponseArrival`). The uploads
  client with no idle pool: `:92-94`, `:132-146` (base `:76`, `:111-118`). The
  `// deferred: #854` marker (base `s3.rs:148-149`) is removed.
- `docs/design/architecture/05-building-block-view.md:255` (base `:255`): the rule, the limit,
  the cost.
- The test fixture's requirement that the host shows its sockets in `/proc/net/tcp`
  (`require_socket_observation`, `:904`; `tcp_table`, `:819`; `PeerReport::seen_open`, `:552`):
  round 5's work, unchanged.

## For the human at sign-off

- **The only change is the bounded setup wait in the dropped-PUT test.** If the 5 s setup
  allowance looks too tight or too loose for CI, it is one constant (`:206`); the compile-time
  check at `:215-216` keeps any value honest against the PUT's 60 s deadline.
- **"All 12 red pre-fix" is 11 red plus the declared green-only guard**, as in round 5 and as
  the brief's Falsifiability section says. Nothing changed there.
- The message on expiry does not say how far the peer got; see the first ruled-out option for
  what adding that costs.
- The two advisory gates in the carry-forward are not something Do can move. C4 diff coverage
  says the patch does not apply on `origin/main`: correct, it builds on #852, which is on the
  integration branch only. C5's two missed mutants are the `ResponseArrival::name` ones the
  brief rules out of scope.
- The kernel-buffer window is documented, not tested (Standing decision 3).
- No external dependency was missing.

## Scratch

Everything throwaway is under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_854`),
named `pdca-builder-854-r6-*`: gate logs, temporary git index files, one 12-line mutation diff,
and three small directories each holding one scratch patch (`-hang-old`, `-hang-new`, `-r4`).
Under 1 MB in all, no checkouts and no build dirs. `xtask ci` also left its own docs render
output there (`wyrd-docs-build-*`). I did not delete any of it. The builder instructions disagree here: one
section says to `rm -rf` scratch, another says no `rm`-style command is ever warranted and that
the harness reclaims its roots. These files sit inside the harness's own per-bundle scratch
dir, so I left them for the harness.

Also left: a few unreferenced git objects in the target repo's object store (blobs and trees
from the scratch index files). No ref points at them; `git gc` drops them. The lane's verify
worktree (`../wyrd-verify-l0`) is in the state the gate always leaves it in. The cycle worktree
holds the final tree, uncommitted, and matches `patch.diff` (the patch reverse-applies cleanly
on it). I did not push, and opened no PR.
