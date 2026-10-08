# Build notes — issue 854, revision 5 (round 5)

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `df68932`.
Starting point: `iteration-v4/patch.diff`, applied unchanged, then this revision's changes on top.
One `patch.diff` against the base: 5 files, +1743 / −50.

Line numbers below are in the patched tree unless marked "base".

## What this revision changes on top of round 4

Production diff over round 4 is small (`git diff --numstat` against round 4's tree):
`s3/body.rs` +41/−27, `s3/error.rs` +10/−7, `s3.rs` +49/−17 and doc comments only (no code line
changed), one paragraph of the architecture doc. The test file is 1283 lines (round 4: 1153).

### 1. Receipt rule: the source must have ended (Standing decision 1)

- `crates/validate/src/s3/body.rs:90` — `Upload` gains `ended: AtomicBool`. It replaces the
  body's own private `ended: bool` (round 4 `body.rs:265`), so there is one record of the
  source's end and both the body and the outcome read it.
- `body.rs:337` — the body sets it in the one place the source reports its end at exactly the
  declared length (round 4 `body.rs:323`). `body.rs:296` reads it where the body used to read
  its own flag.
- `body.rs:101-106` — `Answer` now records `source_ended` (was `produced`).
- `body.rs:121-129` — `Upload::answered` records whether the source had ended when hyper handed
  the response over. It runs inside the request's poll, and `Turn` (round 4, unchanged) keeps
  the body from taking anything until that poll ends, so the flag is as of the hand-over.
- `body.rs:149-159` — `Upload::acknowledged_early` returns the error unless the record says the
  source had ended. Round 4 compared `produced < declared` (round 4 `body.rs:138-148`).
- `crates/validate/src/s3/error.rs:95-107` — the `AcknowledgedEarly` doc states the new rule
  ("never a receipt, even when `produced` equals `declared`"). `error.rs:204-205` — the
  `Display` text now says "before its source had ended, when it had yielded N of M bytes".
  The old text ("when its source had yielded 10 of 10 bytes") would read as nonsense for the
  held-end case. The request id suffix (`x-amz-request-id <id>`) is unchanged.

No change to `put_object`'s code (`s3.rs:209-245`): it already calls `acknowledged_early()` on a
success, before anything else.

Why "ended" is always reachable for an honest upload: the SDK's aws-chunked layer wraps this
body and keeps polling it until it reports its end, because it cannot write the final chunk and
the checksum trailer before that (`aws-runtime-1.10.0/src/content_encoding/body/http_body_1_x.rs:56-83`,
with `body.rs:137-170` the brief cites). So a server that really has the whole upload can only
answer after the source ended. I read that code to check the claim and cite it on
`DeclaredLengthBody` (`body.rs:258-264`). The existing round-trip tests against the in-process
gateway (`crates/validate/tests/s3_client_roundtrip.rs`, 5 `put_object` call sites) are the
guard if an SDK bump ever changes that: every PUT would then fail loudly, not pass falsely.

### 2. Docs: the rule, the limit, the cost

- `s3.rs:17-19` — module doc: "had ended before the answer".
- `s3.rs:155-166` — **A receipt.** The rule, why the declared length is not enough, and that an
  overrun taken before the answer is still `SourceLength`.
- `s3.rs:168-176` — **The limit.** The two things the client cannot see: whether the server
  read the bytes, and an answer in its own kernel receive buffer that hyper has not read.
- `s3.rs:178-186` — the backstop, worded as the brief requires: nothing detects it today; the
  read-back check will catch one only on a single-writer key read back before its next
  successful overwrite; not on the contention pool; not a single-writer write overwritten
  before a read. No claim that every false receipt is caught. No issue number.
- `s3.rs:198-208` — **The cost, per PUT.** Fresh connection, `TIME_WAIT`, about 470/s with Linux
  defaults (28,232 ports ÷ 60 s), `connect` fails → `S3Error::NoResponse` → counted as
  `availability`; a host default, per address pair, shared by all workers; the three ways to
  raise it. No issue number.
- `docs/design/architecture/05-building-block-view.md:255` — same rule, a one-sentence limit,
  and the cost with its consequence. I left the backstop out of this paragraph on purpose: the
  brief requires it only on `put_object`, and a compressed version here would be the easiest
  place to overstate it.
- The `// deferred: #854` marker (base `s3.rs:148-149`) is gone (round 4 removed it; checked
  with a grep over `crates/validate/`).

### 3. Test: the close oracle (Standing decision 5)

`crates/validate/tests/s3_client_upload_peers.rs`:

- `:813-830` `AtReturn` — the return stamp, the drop flag, bytes given and "source ended" are
  read **inside the same poll** in which `put_object` returns (`:853-858`, the async block
  wrapped by the `T_op + 1 s` timeout). Round 4 read them after the `timeout(..).await`; same
  thread, but this removes any question about what could run in between.
- `:562` `Peer::after_call(ended_at)` hands that stamp to the peer; `:645-650` the peer derives
  one absolute deadline `ended_at + CLOSE_BOUND` for both sightings. Round 4 started the clock
  when the peer got the signal (round-4 test `:606`).
- `:721-742` `wait_client_close` reports `After(d)` with `d` counted from the stamp and taken
  after the `/proc/net/tcp` read finished (round 4 counted from the start of the wait, `:681-683`).
- `:787-805` `drain_until_closed` stamps every read; `Eof` and `Reset` now carry `after`.
- `:898-919` `assert_connection_closed` fails unless `After(d)` has `d <= 1 s` **and** the
  peer's EOF/reset has `after <= 1 s`. Round 4 accepted any `After(_)` and any EOF/reset (`:820`).
- `:161` a compile-time check that the late-body delay is at most half the close bound. The
  peer cannot hear "the call ended" while it sleeps out that delay; now that the deadline is
  absolute, a longer delay could fail a client that closed in time.
- Scenario 5 (`:1244`): the drop stamp is taken just before `drop(put)`, so the 1 s covers the
  drop itself.
- If the test ends without telling the peer when the call ended, the peer sends no report
  (`:645`), and `assert_connection_closed` then fails with "the peer never reported". Fail-closed.

### 4. Test: scenario 4

- `:1148` 4(a) `a_source_that_ended_before_the_acknowledgement_is_a_receipt` (round 4 `:1121`,
  renamed; now also asserts the source had reported its end).
- `:1184` 4(b) `a_source_that_gave_its_whole_length_but_held_its_end_is_not_a_receipt`
  (round 4 `:1137`, flipped).
- `:1201` 4(c) `a_source_that_gave_its_declared_length_and_held_an_extra_byte_is_not_a_receipt`
  (new): `Source::new(11, 11)` held at 10, declared 10.
- Both use round 4's `Hold` unchanged (`:283`, `answer_read` `:298`, `held_at` `:363`,
  `Answer::WhenHeld` `:443`) and name both fixture asserts in the test body:
  `assert_parked_before_answer` (`:932`) and `assert_not_polled_past_the_hold` with `at = 10`
  (`:969`).
- `:984` `assert_body_error_carrying_the_answer` now pins the variant through the `Debug` text
  (`starts_with("Body(AcknowledgedEarly {")`) and the request id through `Display`. It never
  names the variant, so the file still compiles on the base. I use the one helper for every
  body-error case (scenarios 1, 2, 4), not just 4(b)/(c): only this variant can carry the
  answer's id, so it is the same claim, stated more exactly.

Round 4's fixture fixes are untouched: zero-linger peer socket, receive-queue-first hold check,
clock-timed backed-up wait, 500 ms late-body delay.

## Decisions, and what I ruled out

**A success with no arrival record is not a receipt (my call, not in the brief).** Round 4's
`acknowledged_early` fell back to the live counter when the arrival hook had not run. The same
fallback under the new rule would be "no record → judge on whether the source has ended by
now", which can hand out a receipt without knowing the order of end and answer. That breaks the
invariant by construction, and AGENTS.md "Absent or unsupported entries … never silent success"
points the other way. So no record → the error, with `request_id: None`. Cost comparison, both
forms written out and counted:

- lenient (round-4 shape): a `match` on the record with a `None` arm that reads the live state,
  a 9-line function body, the `None` arm unreachable, plus a `produced` field on `Answer`;
- fail-closed (shipped, `body.rs:150-158`): `answer.is_some_and(|a| a.source_ended)` and one
  `and_then`, a 9-line function body, no unreachable line, one field fewer on `Answer`.

Same size, so cost does not decide it; the invariant does.

The branch cannot be reached through the SDK today: the orchestrator calls `read_after_transmit`
for every response the connector returns (`aws-smithy-runtime-1.15.0/src/client/orchestrator.rs:504-511`).
So this changes no reachable behaviour; it only fixes which way the code falls if that ever
stops being true. Not unit-tested for that reason: `Upload` is private and no public call can
produce a success without the hook. The crate has no in-module unit tests today (no `cfg(test)`
under `crates/validate/src`), and I did not start that pattern for an unreachable branch. If the
human wants it pinned, it is about 30 lines in `body.rs`.

**The error's byte count is read live, not stored in `Answer`.** Round 4 stored `produced` in
the arrival record. With the fail-closed form that needs a fallback closure for the no-record
case, which is dead code. The live counter is the same number: the body takes nothing from the
source from the hand-over on (`body.rs:299-310`), and `acknowledged_early` runs after the PUT's
runtime is shut down. Documented at `body.rs:146-148`.

**No new field or variant on the error.** `AcknowledgedEarly { declared, produced, request_id }`
is unchanged in shape, so nothing that matches on it breaks. A `source_ended: false` field would
always be false.

**Probing the close oracle by delaying production's close** (send the outcome, sleep 1.5 s, then
shut the runtime down). Ruled out as a probe: the source and the socket are owned by the same
hyper task, so the source assertion fires first and the close asserts are never reached. The
stamp-shift probes below reach both close asserts directly.

**Not done, by the brief:** no new HTTP seam, no pooling, no DST coverage, no change to
`.cargo/mutants.toml`, nothing outside `crates/validate/` but the one doc paragraph.

## Evidence

All runs go through the project's own gate script, `./engine/scripts/run-verify.sh`, with
`PDCA_LANE=0` (this lane's verify worktree) and the Bash tool's timeout around it.

### Gate, on the wave base (`PDCA_VERIFY_BASE=origin/pdca-integration/r-a834…/main`)

```
run-verify.sh: GREEN — cargo test -p wyrd-validate --test s3_client_upload_peers (fix applied)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.03s
run-verify.sh: RED — cargo test -p wyrd-validate --test s3_client_upload_peers (production reverted, test kept)
test result: FAILED. 1 passed; 11 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.02s
run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
```

Red on the base (#852's production), per test. Every failure is an assertion; the file compiles.

| Test | Outcome on #852 | Fails on |
|---|---|---|
| `an_acknowledgement_before_the_source_is_done_…` | `Ok(PutOutcome)`, 3 MiB of 64 MiB given | never a receipt |
| `a_source_that_fails_after_the_acknowledgement_…` | `Body(SourceFailed { produced: 4 })` | not `AcknowledgedEarly` |
| `a_final_piece_ready_once_the_client_has_read_…` | `Ok(PutOutcome)` | never a receipt |
| `a_source_that_fails_while_the_acknowledgements_body_…` | `Body(SourceFailed { produced: 4 })` | not `AcknowledgedEarly` |
| `a_final_piece_ready_while_the_acknowledgements_body_…` | `Ok(PutOutcome)` | never a receipt |
| `a_rejection_whose_body_is_on_its_way_…` | `Body(SourceFailed { produced: 4 })` | not the `403 AccessDenied` service error |
| `an_acknowledgement_under_backpressure_…` | `Ok(PutOutcome)`, 32 MiB of 512 MiB, `BackedUp::Yes` | never a receipt |
| `a_peer_that_stops_reading_and_never_answers_…` | `Timeout { Operation, 3s }` (right) | source still alive at return |
| 4(a) `a_source_that_ended_before_…_is_a_receipt` | `Ok(PutOutcome)` (right) | `client_close: Not`, `StillOpen` (lifetime only, as declared) |
| 4(b) `…_held_its_end_is_not_a_receipt` | `Ok(PutOutcome)`; the source went on and ended | never a receipt |
| 4(c) `…_held_an_extra_byte_is_not_a_receipt` | `Body(SourceLength { declared: 10, produced: 11 })` | not `AcknowledgedEarly` |
| scenario 5 `a_put_dropped_while_blocked_mid_write_…` | passes | declared green-only |

### Second red: against round 4's production code, unchanged

Method: a scratch commit `4536591` = base + round 4's production and doc files, no test
(built with a temporary index, no ref, HEAD not moved). Then the same gate script with that
commit as the base and a scratch patch from it to this tree. Its RED leg is therefore
"round 4's production + this revision's test".

```
run-verify.sh: GREEN — … (fix applied)
test result: ok. 12 passed; 0 failed; …
run-verify.sh: RED — … (production reverted, test kept)
a_source_that_gave_its_whole_length_but_held_its_end_is_not_a_receipt --- FAILED
a_source_that_gave_its_declared_length_and_held_an_extra_byte_is_not_a_receipt --- FAILED
test result: FAILED. 10 passed; 2 failed; …
```

Both fail on "never a receipt" with `outcome: Ok(PutOutcome { etag: None })`,
`given_at_return: 10`, `ended_at_return: false`, `parked: BeforeAnswer`. So against round 4,
4(b) and 4(c) are each a receipt, exactly as the brief predicts, and the other ten pass. The
rule change is what turns them.

### Stability

The gate ten times in a row on the final patch: 10 of 10 were 12/12 green and 11/12 red.
Other lanes were building on the host at the time; I did not add synthetic load.

### The tightened close oracle is binding (two probes, test-only edits, not shipped)

- Probe A: hand the peer a stamp 2 s in the past. All 12 tests fail on the first close assert
  (`client_close: After(2.00…s)`). Round 4's oracle accepted any `After(_)`.
- Probe B: shift the stamp by 2 s for the peer's reads only. The first assert passes
  (`After(~2 ms)`), and all 12 fail on the second (`Eof { after: 2.00…s }`). So an EOF seen
  after the deadline fails too.

### Whole-tree gate

`./engine/xtask.sh ci` (the project's `cargo xtask ci`, run in `$PDCA_WORKTREE` on the final
tree): `xtask ci: all checks passed`, exit 0. Every step ran, none was skipped: `typos`, docs
lint, docs render check, gitlink-guard, unsafe-guard, blackbox-guard, `cargo fmt --check`,
clippy, build, the workspace tests, cargo-machete, the three `cargo deny` runs, conformance
(5 valid + 6 invalid vectors), the statics gate, deploy-guard, and clippy + tests for
`wyrd-dst` under `--cfg madsim`. Inside it, `s3_client_upload_peers` ran 12/12 green under the
load of the whole workspace suite, and `s3_client_roundtrip` 14/14 (the existing PUT/GET tests
against the in-process gateway, which is the check that the stricter rule does not fail an
honest upload).

The target repo has no pre-commit hooks configured (no `.pre-commit-config.yaml`, no
`core.hooksPath`, only sample hooks), so the formatter plus `xtask ci` is the commit bar.

## Refuting my own test

- **(a) Genuine red?** Yes. With the production change reverted to the base, 11 of 12 fail by
  assertion (table above); the twelfth is the brief's declared green-only guard. With the
  production change reverted only to round 4, exactly the two new-rule cases fail. Both reds
  were run, not reasoned.
- **(b) Production path?** Yes. Every test goes through `resolve_config` →
  `S3Client::with_deadlines` → `S3Client::put_object`, the real `aws-sdk-s3` and hyper
  connector, over real loopback TCP. Nothing is mocked or re-implemented; the only scripted
  part is the peer, which is the fault being injected.
- **(c) Fixture includes the fault?** Yes, and each test asserts it rather than assuming it:
  the peer did answer (`assert_answered`); the source was at its hold before the answer
  (`assert_parked_before_answer`); it was never polled past the hold
  (`assert_not_polled_past_the_hold`, `at = 10`); in 4(b) the source had not reported its end
  at return; the backpressure cases assert `BackedUp::Yes`; the late-body cases assert the body
  was written after the head. In the round-4 red, `parked: BeforeAnswer` and
  `ended_at_return: false` show the held cases really did answer a source that had not ended.

## For the human at sign-off

- The fail-closed no-record branch above is my addition. It changes no reachable behaviour. If
  you would rather keep round 4's fallback, it is a 9-line swap in `body.rs:150-158` plus the
  `produced` field back on `Answer`.
- `ClientClose::Unobservable` still passes the first close sighting where `/proc/net/tcp`
  cannot be read (round 4's choice, kept; the brief lists `/proc/net/tcp` as present on dev and
  CI hosts). The second sighting (the peer's EOF/reset within 1 s) is always enforced.
- The kernel-buffer window is documented, not tested, per Standing decision 3.
- No external dependency was missing. No NEEDS-HUMAN item from Do.

## Scratch

Everything throwaway is under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_854`),
named `pdca-builder-854-*`: gate logs, temporary git index files, the scratch patches for the
second red and the two probes. A few MB, no checkouts or build dirs. I did not delete them: the
builder instructions conflict here (one section says to `rm -rf` scratch, another says no
`rm`-style command is ever warranted and the harness reclaims its roots). Since they sit inside
the harness's own per-bundle scratch dir, I left them for the harness to reclaim.
Also left behind: one unreferenced commit (`4536591`) and two unreferenced blobs in the target
repo's object store, from the second red and the probes. No ref points at them; `git gc` drops
them. The lane's verify worktree (`../wyrd-verify-l0`) is in the state the gate always leaves
it in.
