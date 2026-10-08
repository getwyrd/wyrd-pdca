# Build notes — issue 854, revision 5, iteration 5 (round 6 of Do)

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `df68932`.
One `patch.diff` against the base: 5 files, +1923 / −50.

Line numbers are in the patched tree unless marked "base".

## What this iteration is

The previous attempt (`iteration-v5/`) built revision 5 of the brief on top of round 4's patch.
Its gate was green and its production code drew no blocking finding. It was sent back for one
thing, stated in the carry-forward: **where `/proc/net/tcp` cannot be read, four fixture checks
pass without testing anything**, and one of them (`ClientClose::Unobservable => true`) counts as
proof that the client closed its connection.

So this iteration changes **only the test file**. The production files and the architecture
paragraph are byte-identical to the previous attempt (checked: same blob hashes). Against the
previous attempt's test the change is +212 / −32 lines (1283 → 1463).

I did not re-submit the rejected approach: no check degrades any more. The details follow.

## The fix to the test fixture

All in `crates/validate/tests/s3_client_upload_peers.rs`.

### 1. No degraded branch is left

- `BackedUp::Unobservable` is gone (enum at `:475`). `wait_backed_up` (`:719`) no longer sleeps a
  second and answers when the table is unreadable. Scenario 2 (`:1291-1292`) and scenario 5
  (`:1443-1444`) accept `BackedUp::Yes(_)` only.
- `ClientClose::Unobservable` is gone (enum at `:499`). `wait_client_close` (`:748`) no longer
  returns early, so the peer never starts draining before it has watched the client's socket.
  `assert_connection_closed` (`:1076`) has no arm that passes without a sighting.
- `tcp_table_readable()` is gone. The one reader, `tcp_table()` (`:802`), panics with a message
  when the file cannot be read. Before, `tcp_socket` turned a read error into `None`, and to
  `wait_client_close` `None` means "the socket is gone", which is a pass.

### 2. A check that runs before anything else: `require_socket_observation` (`:887`)

`Peer::start` calls it first (`:559`). Every one of the 12 tests starts a peer, so no test runs
without it, and a test added later cannot forget it.

The carry-forward asked for a failure when `tcp_table_readable()` is false. I went one step
further, because a readable file is not the real requirement. A table that is readable but
lists none of this process's sockets (or shows every queue as empty) fails the same checks open:
`wait_client_close` sees "not listed" and reports a close; the hold never opens. So the check is
a positive control. It opens a loopback connection of its own and requires, through the same
lookups the tests use:

- both ends listed as `ESTABLISHED` while open (`:898`);
- 6 bytes sent and not read show as 6 in the reader's receive queue (`:909`), and
  `Hold::answer_read()` says "not read" (`:921`);
- once they are read, `Hold::answer_read()` says "read" (`:929`). This is the real function the
  held cases depend on, not a copy. It proves a hold can open on this host, which is what makes
  `assert_not_polled_past_the_hold` mean something;
- after one end is dropped, it is no longer listed as `ESTABLISHED` (`:936`).

Each wait is bounded by `OBSERVATION_WAIT = 2 s` (`:778`); each socket step is bounded the same
way (`step`, `:855`), so the check cannot hang a test. Both sockets close with a reset, like the
peer's, so the check leaves nothing in the table. Cost: a few table reads per test; the suite
still finishes in 3.03 s.

Every failure of this check says the host cannot run the test and that nothing was tested
(`unsupported`, `:781`).

### 3. A sighting on the PUT's own connection: `PeerReport::seen_open` (`:531-535`)

Right after it has read the request head, the peer looks the client's socket up and records
whether the table lists it as `ESTABLISHED` (`:621`). `watched` (`:1060`) requires that sighting.
It is called by the two checks that later read "not listed" in their own favour:
`assert_connection_closed` (`:1077`) and `assert_not_polled_past_the_hold` (`:1151`). So "the
socket is no longer listed" counts as a close only for a socket the same lookup had shown open.

This is belt and braces on top of item 2. It costs one field, one helper and one table read per
test. I kept it because it makes the close claim checkable on the connection under test, not
by inference from a different connection.

### 4. Docs in the test file

- New module section "What the host must offer" (`:76-89`).
- The `Hold` doc's last paragraph (`:296-300`) used to say "where `/proc/net/tcp` cannot be read
  the source never goes on, which still holds…". That is the sentence the finding is about. It
  now says why a hold that cannot open is not acceptable and where that is ruled out.
- `wait_client_close` (`:744-747`) and `assert_not_polled_past_the_hold` (`:1146-1149`) each say
  what their claim rests on.

`Hold`'s mechanism is unchanged, as the brief requires (fields at `:302`, `answer_read` at `:317`,
`Source::held_at` `:382`, `Answer::WhenHeld` `:462`). Round 4's fixture fixes are untouched
(zero-linger peer socket, receive-queue-first hold check, clock-timed backed-up wait, 500 ms
late-body delay).

### One sentence of the brief this overrides

The brief's scenario 4 says "(where `/proc/net/tcp` cannot be read it never goes on at all, which
changes nothing)". The carry-forward, which is later, shows that it does change something: on
such a host a client that keeps polling the body after the answer passes. I followed the
carry-forward. On such a host the tests now fail up front.

## Decisions, and what I ruled out

**Fail, not skip.** The carry-forward allows "fail with a message (or skip loudly)". Rust's test
runner has no skip: a test that returns early is reported `ok`, and its output is hidden unless
it fails. So a "skip" would be the silent pass the finding is about. The tests fail.

**Not a `#![cfg(target_os = "linux")]` gate.** It is one line, and on a non-Linux host the 12
tests would not exist instead of failing. I did not add it: the brief lists `/proc/net/tcp` as
present on dev and CI hosts, the review asked for a failure with a message, and a compiled-out
test says nothing at all. If someone develops on macOS and wants `cargo test` green there, the
gate is that one line at the top of the file; the up-front check should stay either way, because
a Linux sandbox can hide `/proc/net` too.

**Call site: `Peer::start`, not 12 copies at the top of each test.** The finding says "at the
top of each test". One call in the one function every test must go through gives the same
guarantee with nothing to forget. Cost of the alternative: 12 one-line calls, and a 13th test
could leave it out.

**A capability token threaded through the fixture** (a value only the check can make, required
by every table read). Ruled out on cost and on the brief: it needs a new field on `Hold` and a
new parameter on `put_against`, `put_held`, `put_held_at_its_declared_length` and `tcp_socket`
(about 40 changed lines), and the brief says to use round 4's `Hold` unchanged.

**A read error inside a run panics rather than being carried as a value.** `tcp_socket` is
called from the source's poll, on the client's thread. Carrying "unreadable" as a value from
there to an assert needs a new flag on `Hold` and a check in every caller. A panic fails the
test on every path: in the peer's task the report never arrives and
`watched` fails with "the peer never reported"; in the source's poll the connection task dies
and the outcome assert fails. Both print the "cannot run this test" message. After the up-front
check has passed this can only happen if the file stops being readable mid-test.

**Production: nothing changed, and nothing needed to.** The carry-forward lists no production
finding. The 15 findings "deferred to sign-off" are the human's; I did not read or act on them
(the builder's input is the brief).

## Evidence

All runs go through the project's own gate script, `./engine/scripts/run-verify.sh`, with
`PDCA_LANE=0` and the Bash tool's timeout around it. Logs are in `$PDCA_SCRATCH`
(`pdca-builder-854-*.log`).

### Gate, on the wave base (`PDCA_VERIFY_BASE=origin/pdca-integration/r-a834…/main`)

```
run-verify.sh: GREEN — cargo test -p wyrd-validate --test s3_client_upload_peers (fix applied)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.03s
run-verify.sh: RED — cargo test -p wyrd-validate --test s3_client_upload_peers (production reverted, test kept)
test result: FAILED. 1 passed; 11 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.03s
run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
```

Red on the base (#852's production), per test. Every failure is an assertion on the behaviour
under test; the file compiles; **none is the new "cannot run this test" failure** (0 matches in
the log), and every report shows `seen_open: true`.

| Test | Outcome on #852 | Fails on |
|---|---|---|
| `an_acknowledgement_before_the_source_is_done_…` | `Ok(PutOutcome)`, 3 MiB of 64 MiB given | never a receipt |
| `a_source_that_fails_after_the_acknowledgement_…` | `Body(SourceFailed { produced: 4 })` | not `AcknowledgedEarly` |
| `a_final_piece_ready_once_the_client_has_read_…` | `Ok(PutOutcome)` | never a receipt |
| `a_source_that_fails_while_the_acknowledgements_body_…` | `Body(SourceFailed { produced: 4 })` | not `AcknowledgedEarly` |
| `a_final_piece_ready_while_the_acknowledgements_body_…` | `Ok(PutOutcome)` | never a receipt |
| `a_rejection_whose_body_is_on_its_way_…` | `Body(SourceFailed { produced: 4 })` | not the `403 AccessDenied` service error |
| `an_acknowledgement_under_backpressure_…` | `Ok(PutOutcome)`, 32 MiB of 512 MiB, `BackedUp::Yes(2595177)` | never a receipt |
| `a_peer_that_stops_reading_and_never_answers_…` | `Timeout { Operation, 3s }` (right) | source still alive at return |
| 4(a) `a_source_that_ended_before_…_is_a_receipt` | `Ok(PutOutcome)` (right) | `client_close: Not`, `StillOpen` (lifetime only, as declared) |
| 4(b) `…_held_its_end_is_not_a_receipt` | `Ok(PutOutcome)`; the source went on and ended | never a receipt |
| 4(c) `…_held_an_extra_byte_is_not_a_receipt` | `Body(SourceLength { declared: 10, produced: 11 })` | not `AcknowledgedEarly` |
| scenario 5 `a_put_dropped_while_blocked_mid_write_…` | passes | declared green-only |

### Second red: against round 4's production code, unchanged (required by the brief)

Base for this run: commit `4536591` = `df68932` + round 4's production and doc files, no test.
The previous attempt made it; I checked it before reuse (the four files' blob hashes equal
`df68932` + `iteration-v4/patch.diff`). The gate ran with that commit as the base and a scratch
patch from it to this tree, so its RED leg is "round 4's production + this test".

```
run-verify.sh: GREEN — … (fix applied)
test result: ok. 12 passed; 0 failed; …
run-verify.sh: RED — … (production reverted, test kept)
a_source_that_gave_its_whole_length_but_held_its_end_is_not_a_receipt --- FAILED
a_source_that_gave_its_declared_length_and_held_an_extra_byte_is_not_a_receipt --- FAILED
test result: FAILED. 10 passed; 2 failed; …
```

Both fail on "never a receipt": `outcome: Ok(PutOutcome { etag: None })`, `given_at_return: 10`,
`ended_at_return: false`, `parked: BeforeAnswer`, `seen_open: true`. Against round 4, 4(b) and
4(c) are each a receipt and the other ten pass. The rule change is what turns them.

### The new requirement is binding (four probes, one-line edits to a scratch copy of the patch, not shipped)

| Probe | Edit | Result |
|---|---|---|
| Table hidden | `TCP_TABLE = "/proc/net/tcp-hidden"` | 12 of 12 fail in 0.03 s: "this host cannot run this test: /proc/net/tcp-hidden cannot be read (No such file or directory …) … nothing was tested." |
| Table readable, lists nothing of ours | `TCP_TABLE = "/proc/net/udp"` | 12 of 12 fail in 2.03 s: "… does not list both ends of an open loopback connection of this test as established …" |
| Peer cannot see the PUT's socket | `seen_open = established(client, client)` | 12 of 12 fail: "the peer did not see the client's socket established in /proc/net/tcp once it had read the request head …" |
| The reviewer's mutant: drop `upload.answer.get().is_some()` (`s3/body.rs:303` → `if false`) | on this host, table visible | 3 of 12 fail, the three late-body tests: "the source was polled past its hold", `SourceFailed` instead of `AcknowledgedEarly`, `SourceFailed` instead of the `403` |

The first probe is the carry-forward's failing case: with the previous test those 12 went green
on a hidden table whatever the client did. The fourth shows the hold opens here and catches a
client that polls the body after the answer.

Limit of the first probe: I pointed the test at a path that does not exist. I did not run it in
a real sandbox that hides `/proc/net`. The code path is the same (`read_to_string` fails), but
the sandbox itself was not exercised.

### Stability

The gate 19 times on the final test: 19 of 19 were 12/12 green and 11/12 red. Eight of those
ran while `xtask ci` was building and testing the workspace in the other worktree (load about 2
on this host; I did not add synthetic load). Ten more runs on the version just before the final
one (the same test without the time bound on the up-front check's socket steps) were also 10 of
10. The second red and the four probes were re-run on the final test; the results above are
from those runs.

### Whole-tree gate

`./engine/xtask.sh ci` (the project's `cargo xtask ci`, in `$PDCA_WORKTREE`, on the final tree):
`xtask ci: all checks passed`, exit 0. It ran `typos`, docs lint, docs render check,
gitlink-guard, unsafe-guard, blackbox-guard, `cargo fmt --check`, clippy on all targets, the
workspace tests, `cargo deny`, conformance (5 valid + 6 invalid vectors), the statics gate,
deploy-guard, and clippy + tests for `wyrd-dst` under `--cfg madsim`. Inside it,
`s3_client_upload_peers` ran 12/12 and `s3_client_roundtrip` 14/14.

The target repo has no commit hooks (no `.pre-commit-config.yaml`, no `core.hooksPath`), so the
formatter plus `xtask ci` is the commit bar. `cargo fmt --all -- --check` is clean.

## Refuting my own test

- **(a) Genuine red?** Yes. With production reverted to the base, 11 of 12 fail by assertion
  (table above); the twelfth is the brief's declared green-only guard. With production reverted
  only to round 4, exactly the two new-rule cases fail. Both reds were run on this final test,
  not carried over from the previous attempt.
- **(b) Production path?** Yes. Every test goes through `resolve_config` →
  `S3Client::with_deadlines` → `S3Client::put_object`, the real `aws-sdk-s3` and hyper
  connector, over real loopback TCP. Only the peer is scripted, and the peer is the fault. The
  new up-front check calls the fixture's real `Hold::answer_read` and `tcp_socket`, not copies.
- **(c) Fixture includes the fault?** Yes, and each test asserts it: the peer answered
  (`assert_answered`); the source was at its hold before the answer
  (`assert_parked_before_answer`); it was never polled past it
  (`assert_not_polled_past_the_hold`, `at = 10`); the backpressure cases require
  `BackedUp::Yes`; the late-body cases require the body written after the head. New in this
  iteration: none of those can now be satisfied by a host that cannot see the sockets (probes 1
  to 3).

## What the earlier rounds built (unchanged here, restated so the citations are in one place)

- Receipt rule, "the source ended before the response arrived": `crates/validate/src/s3/body.rs:90`
  (`Upload::ended`), `:121-129` (`Upload::answered` records it), `:149-159`
  (`Upload::acknowledged_early`), `:296` and `:337` (where the body reads and sets it). Base
  hunks: `s3/body.rs:55-67` and `:86-143`.
- The order between request and body (`Turn`, `RequestFirst`): `s3/body.rs:162-254`.
- The error: `crates/validate/src/s3/error.rs:95-108` (`AcknowledgedEarly`, base `:92`),
  `:197-208` (`Display`, base `:180`), `:49-51` (`RequestNotBuilt` doc, base `:49`).
- `put_object`: `crates/validate/src/s3.rs:152-208` (doc: receipt, limit, partial future
  backstop, lifetime, per-PUT cost; base `:125`), `:209-245` (code; base `:133-149`),
  `:372-417` (`on_own_runtime`), `:426-443` (`ResponseArrival`). The uploads client with no idle
  pool: `:92-94`, `:137-146`. The `// deferred: #854` marker (base `s3.rs:148-149`) is removed.
- `docs/design/architecture/05-building-block-view.md:255`: the rule, the limit, the cost.

A success with no arrival record is not a receipt (`s3/body.rs:150-158`). That was the previous
builder's own call, kept: it cannot be reached through the SDK today, and it falls the safe way.

## For the human at sign-off

- **The tests now fail on a host without a usable `/proc/net/tcp`.** That is the requested
  behaviour. If you would rather they not exist on non-Linux hosts, add
  `#![cfg(target_os = "linux")]` at the top of the test file (one line).
- A `pdca.toml` doctor row would catch such a host before a cycle starts. The brief already
  names the dependency, so this is a suggestion, not a missing dependency:
  `id = "proc-net-tcp"`, `cmd = "test -r /proc/net/tcp"`, `level = "MISSING"`.
- One timing limit of the hold remains, unchanged from round 4: `Hold::answer_read` waits at
  most 100 ms for the peer's write to return (`ANSWER_WRITE_WAIT`, `:204`). If the peer's thread
  were starved longer than that at the one poll that matters, a client that polls past the
  answer could go unseen in a bodyless case. The late-body cases get a second poll 500 ms later,
  and the mutant probe above shows all three catch it on this host. I left it alone because the
  brief says to use round 4's `Hold` unchanged.
- The two advisory gates in the carry-forward are not something Do can move. C4 diff coverage
  says the patch does not apply on `origin/main`: correct, it builds on #852, which is on the
  integration branch only. C5's two missed mutants are the `ResponseArrival::name` ones the
  brief rules out of scope.
- The kernel-buffer window is documented, not tested (Standing decision 3).
- No external dependency was missing.

## Scratch

Everything throwaway is under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_854`),
named `pdca-builder-854-*`: gate logs, temporary git index files, and five small directories
holding scratch patches (`-r4`, `-hidden`, `-empty`, `-unseen`, `-mutant`). A few MB, no
checkouts and no build dirs. I did not delete them. The builder instructions disagree here: one
section says to `rm -rf` scratch, another says no `rm`-style command is ever warranted and that
the harness reclaims its roots. They sit inside the harness's own per-bundle scratch dir, so I
left them for the harness.

Also left: a few unreferenced git objects in the target repo's object store (blobs and two
trees from the scratch index files, plus the earlier commit `4536591`). No ref points at them;
`git gc` drops them. The lane's verify worktree (`../wyrd-verify-l0`) is in the state the gate
always leaves it in. I did not push, and opened no PR.
