# Build notes — #956 (743.2, second child of #938), iteration 5: bulk delete, probes, endpoint resolution

## Base and starting point

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `635e0bd` (`pdca-integrate: issue_955`).
  `patch.diff` is the whole change against that commit: 6 files. The worktree is left at `635e0bd` with the change
  uncommitted.
- Salvage hashes rechecked, both as the brief states: `patch.diff` `b50cc4f5…7d69`, `build-notes.md` `f2c4fcf9…32f7`.
- Starting point: `iteration-v4/patch.diff`, which applied cleanly (`git apply --check` exit 0), because the sign-off says
  "Keep the routing, probe and row-reading code; it held up… Fix these only… Nothing else changes."
- **No production logic changed this round.** The only production-file change is the module doc comment at the top of
  `crates/validate/src/s3.rs`. Everything else is one leg of one test and five sentences of the architecture doc.
- `crates/validate/src/lib.rs`, `Cargo.toml`, `Cargo.lock`, `crates/validate/Cargo.toml` and the three older
  `s3_client_*` test files are untouched (`git diff --stat` on them is empty). `deny*.toml` is not in the diff.
- The whole change since v4, checked by rebuilding the v4 versions of the three files I touched and diffing them: the
  test hunk at `tests:1682-1695`, the five doc sentences, and the three doc-comment sentences in `s3.rs`. Nothing else.

## Iteration 4's carry-forward, item by item

Line numbers are in the patched tree. `tests` is `crates/validate/tests/s3_client_smoke_verbs.rs`.

| # | What the sign-off asked | What changed |
| --- | --- | --- |
| 1 | The "refused connection" leg binds a listener, drops it, and assumes the port stays closed; use the bound-but-not-listening socket from `s3_client_roundtrip.rs:1426-1432`; `Relay::start(refused_addr)` must keep working | `tests:1682-1695`: a `TcpSocket` bound to `127.0.0.1:0` and never listened on, held in `holder` to the end of the test. One new assertion beside it: a second bind on that port fails. The relay leg (`tests:1706`) is unchanged and still points at the held port. |
| 2 | Correct four sentences in `05-building-block-view.md:303`, and the same claim at `s3.rs:29-30` | The four sentences, below, and the budget sentence in `s3.rs:27-32`. Also the two sentences three lines above it (`s3.rs:25-27`), which are the doc's sentences (a) and (c) word for word; see "One step past the named lines". |

### Item 1: why the held socket, and why the extra assertion

The v4 leg freed its port before the probe used it. Between the drop and the probe's `connect`, any other test in the same
binary (they run in parallel) can bind that port for its own `Endpoint` or `Relay`, and the probe then gets that test's
scripted answer: the adversary saw `Ok(Answer { head: Head { status: 200, … "get-framing" …` seven times in 140 runs, and
the other test lost the request its script was waiting for.

A socket that is bound but never put into `listen` holds the port (no other `bind` to it succeeds) while the kernel answers
every `connect` to it with a reset, so the probe's connect fails with `ECONNREFUSED` at once. That is the form at
`crates/validate/tests/s3_client_roundtrip.rs:1426-1433`, copied here (`tests:1687-1691`). `holder` is a local of the test
function, so it lives until the function returns, through the relay leg.

`Relay::start(refused_addr)` (`tests:1706`) keeps working: the relay only connects to its upstream when it has accepted a
connection (`forward`, `tests:838-841`), the `https` leg asserts it accepted none (`tests:1713`), so it never connects to
the held port at all; and if it did, that connect would be refused as the probe's is, and `forward` returns.

The new assertion (`tests:1692-1695`), `std::net::TcpListener::bind(refused_addr).is_err()`, pins the property the fix
gives: the port cannot be taken while the test runs. It is what makes this fix provable red→green rather than
"fewer failures in N runs". The holder has no `SO_REUSEADDR` (`tokio::net::TcpSocket::new_v4` sets none), and Linux
refuses a second `bind` to a port whose current holder lacks it, even when the second socket sets it, as `std`'s
`TcpListener::bind` does. With the v4 form the port is free when the assertion runs, the bind succeeds, and the assertion
fails. That is the red below.

### Item 2: the five sentences, and what in the code each now matches

`doc` is `docs/design/architecture/05-building-block-view.md:303`.

| Sentence | Was | Now | Why |
| --- | --- | --- | --- |
| (a) `doc`; `s3.rs:26-27` | "A PUT or DELETE success that carries a body, which S3 never sends, is unreadable" | "A success of the PUT or DELETE of one object that carries a body, which S3 never sends, is unreadable" | `CopyObject` is a PUT (`Probe::CopyObject`), and its `200` carries `<CopyObjectResult>`, read as a success (`a_2xx_carrying_the_operations_whole_s3_success…`). The "carries a body, so unreadable" rule is `ResponseGuard`'s (`crates/validate/src/s3/response.rs:182`, `:205`, `:292`), which sits only on the one-object PUT, GET and DELETE; the probes and the bulk delete go through `Capture` (`crates/validate/src/s3/verbs.rs:375`, `:408`). |
| (b) `doc`; `s3.rs:28-32` | "Every body the SDK buffers whole (every error response, and every PUT or DELETE response) is capped … a PUT or DELETE success's at nothing" | "Every body the SDK buffers whole (on the PUT, GET and DELETE of one object: every error response, and every PUT or DELETE response) is capped … a PUT or DELETE success's at nothing" | Same split. A probe's or a bulk delete's body, 2xx or not, is taken from the SDK by `Capture` and read by the client under the 64 KiB budget, which the next sentence of the doc already says ("The same 64 KiB budget caps every body the client reads whole itself (#938): a HEAD's, a probe's, …"). A `CopyObject` success is under that budget, not capped at nothing. |
| (c) `doc`; `s3.rs:25-26` | "a `Transfer-Encoding` on a `304` to a PUT or a DELETE" | `doc`: "a `Transfer-Encoding` on a `304` to anything but a GET or a HEAD (a PUT, a DELETE or a POST)"; `s3.rs`: "to anything but a GET or a HEAD" | The rule is `304 => !get && !transfer_encoding.is_empty()` (`crates/validate/src/s3/response.rs:117`), and `get` for a `Capture` means "a GET or a HEAD" (`crates/validate/src/s3/answer.rs:418-419`; `Probe::reads`, `crates/validate/src/s3/verbs.rs:118-127`, names the five GET/HEAD probes). Pinned for PUT, POST and DELETE probes by `a_304_that_frames_a_chunked_body_is_refused_only_for_a_probe_that_is_not_a_get_or_head` (`tests:4796`) and for the bulk delete's POST by `…_is_refused_for_a_bulk_delete_beside_its_status` (`tests:4849`). |
| (d) `doc` | "…are refused the same way, every row lost. So does a delete result that holds anything but `<Deleted>` and `<Error>` rows, … or a row with no `<Key>` or an empty one." | "…are refused the same way, every row lost. A delete result that holds anything but `<Deleted>` and `<Error>` rows, … or a row with no `<Key>` or an empty one, fails the whole call the same way." | "So does" had lost its referent ("fails the whole call", two sentences earlier) when the sizing text was inserted between them in iteration 3. The subject is now in the sentence. |

### One step past the named lines: `s3.rs:25-27`

The sign-off names `s3.rs:29-30` (sentence (b)'s twin) and says nothing else changes. Three lines above it, the same
module doc has sentences (a) and (c) in the same words as the architecture doc ("a `Transfer-Encoding` on a `304` to a
PUT or a DELETE", "a PUT or DELETE success that carries a body"). They carry the same over-claim brief (H) forbids, so I
corrected them the same way: three lines of doc comment, no code. Leaving them would be the same (H) finding at the next
review. If you would rather hold to the named lines exactly, reverting `s3.rs:25-27` to the v4 text is a three-line edit and
nothing depends on it.

What I did **not** change, on purpose: `crates/validate/src/s3/response.rs:73` ("a `Transfer-Encoding` on a `304` to
anything but a GET") and `:82-83` ("a GET or, with `get` false, a PUT or a DELETE"). Those describe `Framing::of`'s own
`get` parameter, whose meaning the caller sets; `Capture` documents its `get` as "a GET or a HEAD" (`answer.rs:418`).
That file is #955's, the sign-off did not name it, and the sentence does not claim more than the code does at that level.
Noted for your attention below.

## The other gates from iteration 4

**C5 (advisory), still expected to show `fail`.** Iteration 4: 64 mutants, 47 caught, 14 unviable, 3 timeouts, 0 missed.
No production logic changed, so the mutant list is the same, and I did not re-run it (18 minutes). The three timeouts are
slow failures inside `s3_client_nonconforming_responses`, which brief (E) forbids changing (iteration 3's notes and
adversary).

**T4 and the code review** passed in iteration 4 with no finding; the adversary's two findings are items 1 and 2.

## Red → green

Verification posture (a), net-new. With production reverted, the test file does not build, as the brief pre-declares.
I did not re-run that revert. The build failure was shown in iteration 2 (`E0432` on the imports) and cannot have changed.

Both runs below are through the project's runner, `./engine/xtask.sh ci` (the configured `C4-ci` gate, which cd's into
`$PDCA_WORKTREE` and runs `cargo xtask ci`: fmt, clippy, build, the whole test suite, deny, conformance). Logs are in
`$PDCA_SCRATCH/pdca-builder-956-it5-*.log`.

### Red for item 1 (`it5-red1`)

One edit: the v4 form of the leg (a `TcpListener` bound and dropped) put back, with the new assertion kept after it.

Result (`pdca-builder-956-it5-red1-ci.log`): every other test binary green; `s3_client_smoke_verbs` `FAILED. 52 passed;
1 failed`, the one failure being this leg, at the new assertion:

```
thread 'a_probe_that_reached_the_connector_and_got_no_answer_is_no_response_never_unsent' panicked at
crates/validate/tests/s3_client_smoke_verbs.rs:1692:5:
the refused port is held, so no other listener can take it
```

So the v4 form does leave its port free for another socket to take, every time, not one run in twenty. The assertion is
what makes the flake a deterministic red.

### Green

The final tree, same runner.

Result (`pdca-builder-956-it5-green-ci.log`): `xtask ci: all checks passed`, exit 0. `s3_client_smoke_verbs` 53 passed,
0 failed (6.9 s). **(E):** in the same run `s3_client_nonconforming_responses` 28, `s3_client_roundtrip` 15,
`s3_client_upload_peers` 12, all passing, all three files unmodified. No `FAILED` or `panicked` anywhere in the log.
`git diff --binary HEAD` after the run has the same sha256 as `patch.diff` (`31677ca9…74cef`), so the gate changed no
tracked file.

Item 2 is documentation; it has no test. Its check is the reading above, sentence by sentence against the code and the
tests that pin each rule.

### Reds from earlier iterations

Iteration 1 to 4's `cf*`, `it1*`–`it5*` edits and the six reds the brief requires were not re-run this round. That code did
not change (the interdiff above is the whole change), and their tests still pass in the green run.

## Refuting my own test

- **(a) Genuine red? Yes.** The v4 form of the leg, with the new assertion, fails the assertion every time: the freed port
  can be bound again. A full revert of the production change does not build, the case the brief pre-declares.
- **(b) Production path? Yes.** The leg drives the production `S3Client::probe` (`verbs.rs:404`) through the real
  `aws-sdk-s3` orchestrator to the client's own connector, whose `connect` to the held port is refused by the kernel, and
  through `S3Client::classify` to `NoResponse`. The new bind assertion is about the fixture (that the port is held), not
  about production; the leg's existing `NoResponse` assertion (`tests:1697-1700`) is the one about production, and it is
  unchanged.
- **(c) Fixture includes the fault? Yes.** The port really has nothing listening on it and really refuses the connection
  (`ECONNREFUSED`, as the adversary saw: "Connection refused", os error 111); the probe really is sent to it. What the
  fixture no longer includes is the race the v4 form had, which is the point.

Flakiness: I did not run the fixed test in a loop. The property that failed under load, "the port stays closed", is now
asserted in the test itself rather than inferred from a run count, and a held port cannot be taken by another bind on
Linux (the CI matrix, `docs/INTEGRATION.md` §3) or macOS.

## Commit-readiness

- `cargo fmt --all -- --check`: clean. `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean.
- Clippy with `-D warnings` runs inside `cargo xtask ci`; see the green run.
- `patch.diff` is `git diff --binary HEAD` on the worktree, and it reverse-applies. The test file is also copied to the
  bundle as `s3_client_smoke_verbs.rs`, byte-identical to the worktree's.
- Scratch: my logs and the saved v4 files are in `$PDCA_SCRATCH` (`pdca-builder-956-it5-*`), left for the harness to
  reclaim, per the builder's filesystem rule.

## External dependencies

`typos` and `docs-renderer`, as the brief lists. No Docker, no network beyond `127.0.0.1`.
No NEEDS-HUMAN external-dependency item.

## For your attention at sign-off

1. **`s3.rs:25-27` changed beyond the lines you named** (above). Revert is three lines if you want it exact.
2. **`response.rs:73` and `:82-83` were left as they are** (above): #955's file, and the sentence is about the
   function's own parameter. If you want "GET or HEAD" there too, it is one word in each place.
3. **The bind assertion depends on the OS refusing a second bind** to a port held by a socket without `SO_REUSEADDR`. True
   on Linux and macOS. A platform where `std`'s `SO_REUSEADDR` let the second bind through would fail the test loudly, not
   pass it by accident.
4. Carried over, unchanged: `head_object`'s panic on an overlong key is #955's and still untracked; the budget test costs
   about 6 s; C5 will still show `fail` for the three known timeouts, with 0 missed; the low-severity notes from
   iteration 3's adversary were left alone.
