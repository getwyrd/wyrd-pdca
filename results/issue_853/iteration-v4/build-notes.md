# Build notes — #853 validate-s3-client-nonconforming-responses (iteration 4)

Target: getwyrd/wyrd @ main, built on the run's stack base `pdca-integration/main-r42c5348f203d`
at **`61e5341`** ("pdca-integrate: issue_744"), the same base as iterations 2 and 3. Removed lines
are cited on `61e5341`, added lines on the patched tree. `patch.diff` is `git diff 61e5341` of the
final tree. Checked: it applies to `61e5341` (`git apply --cached --check` against a scratch index
read from `61e5341`), and `git apply --check --reverse` against the tree is clean.

Starting point: iteration 3's `patch.diff`, which applies cleanly to `61e5341`, applied unchanged.
Then the iteration-3 carry-forward on top, and nothing else: "Do not change anything else; the rest
of the patch passed review." What this round changed on top of iteration 3 (measured with a scratch
index holding `61e5341` + iteration 3's patch, `git diff --numstat`): `s3/wire.rs` +132/−14 (about
20 lines of code; the rest is docs and one unit test), the test file +138/−6, `s3/error.rs` +2/−2
(one doc sentence), the 05 doc +1/−1 (four words). 273 insertions, 23 deletions in all.

## Step 0 — the base has #852's client

`crates/validate/src/s3.rs` with `s3/body.rs` and `s3/error.rs` on `61e5341`. No STOP.
`grep -rn "deferred.*853" crates/validate` finds nothing: the four `// deferred: #853` markers
(base `s3.rs:264`, `:313`, `:343`, `s3/body.rs:442`) stay removed, as in iterations 2 and 3.

## The bug (carry-forward F1), confirmed in the sources and on the production path

A successful chunked GET whose terminal chunk carries a trailer section, followed by junk, was
accepted as a clean object. Why:

- hyper hands the trailer section over as the body's last frame (`hyper` 1.10.1
  `body/incoming.rs:229-233`).
- Iteration 3's `WholeResponse::poll_frame` passed every frame straight on, trailers included
  (iteration 3 `wire.rs:305-306`). It asked the connection what followed the body only on the
  *next* poll, the one where hyper's body returns `None`.
- The GET path never makes that poll. `ObjectBody::read` reads through `ByteStream::next`, which
  calls `SdkBody::poll_next`. That stores a trailers frame and returns `None` (`aws-smithy-types`
  1.8.1 `src/body.rs:220-226`), and after that it returns `None` without polling the inner body
  again (`:208-211`). `ObjectBody` takes the `None` as the end (`s3/body.rs:500-508`).
- The buffered path (every error, every PUT/DELETE response) was not affected. The SDK collects
  through `SdkBody`'s http-body `poll_frame`, which hands out the cached trailers and then polls the
  inner body again (`body.rs:311-318`, `body/http_body_1_x.rs:28-44`). That reaches the `None` and
  the check. This matches what the reviewer and adversary saw: only the GET 200 was accepted.

**Red on iteration 3's code**, before any production change, with this round's test (run
`cargo test -p wyrd-validate --test s3_client_nonconforming_responses`): 24 passed, 1 failed:

```
RED   chunked-trailer-then-junk [GET]: accepted as a 3000-byte object of 3000
RED   chunked-trailer-then-a-second-response [GET]: accepted as a 3000-byte object of 3000
RED   chunked-trailer-then-more-chunks [GET]: accepted as a 3000-byte object of 3000
```

Every other new case was green on iteration 3, as the probes said: the error and PUT/DELETE
trailer + junk cases (the buffered path above) and all the clean-trailer guards.

## The fix

The carry-forward's first option: `WholeResponse` holds the trailer section back until the
connection has answered (`crates/validate/src/s3/wire.rs`).

- A new field, `trailers: Option<HeaderMap>` (`wire.rs:300-301`, set to `None` at `:143`;
  `HeaderMap` imported at `:51`).
- `poll_frame` (`wire.rs:308-354`): a trailers frame from hyper goes into that field and the loop
  polls hyper's body again (`:317-325`). Once the section is out, hyper's trailers channel reads as
  closed and its body returns `None` at once (`body/incoming.rs:229-233`; the channel is
  `futures-channel` 0.3.34's oneshot, whose `recv` returns `Canceled` after its value has been
  taken, `src/oneshot.rs:256-300`, so polling past it neither waits nor panics). That `None` asks
  the connection task, exactly as before. Data frames and errors are passed on as before
  (`:326-328`).
- `State::Ended` (`wire.rs:347-349`) now hands out the held section, then `None`. `Overran` and
  `Lost` are unchanged, so with an overrun the section is never handed over: it is dropped with the
  body.
- No new wait. Polling hyper past the section returns at once, and the task answers "on its next
  poll, from what it already holds", as before. The GET body's deadlines (`s3/body.rs`) and the
  operation deadline bound every await around it, unchanged.
- Docs: `WholeResponse`'s doc says data frames go out as they arrive and the trailer section is
  held, and why (`wire.rs:279-296`); `State::Ended` (`:269`); the module doc (`:6-17`) now names
  the trailer section as part of where a chunked body ends.

With the fix, `s3.rs:9-10` ("counts what the endpoint sent after each response's end: anything at
all fails the response") and the `ObjectBody::read` comment ("At that end the connector checks
that nothing followed it", `s3/body.rs:471-473`) are true as written, as the carry-forward asked.
Three doc sentences said "bytes after a chunked body's terminal chunk". A trailer section legally
comes right after the terminal chunk, so read strictly those sentences would claim a trailer
section is refused. Each now says "terminal chunk and trailer section": `wire.rs:10`,
`s3/error.rs:127` (`BodyError::Overrun`'s public doc), and the 05 doc's `validate` paragraph
(`docs/design/architecture/05-building-block-view.md:297`). No other 05 change: what the client
promises did not change, only the code now keeps the promise.

### Rejected: the alternative, make `ObjectBody` drive the body to its true end

`ObjectBody` reads through `ByteStream::next`, which can never reach the end once `SdkBody` has
cached the trailers (`body.rs:208-211`). So the alternative means replacing that read in
`ObjectBody::read` (`s3/body.rs:464`) with an http-body frame loop over the inner `SdkBody`,
which hands out the cached trailers and then polls on (`body.rs:311-318`). That is about 15-20
changed lines in `body.rs`'s read path, against about 20 here. And it fixes one reader only: the
check would still run only for readers that happen to poll past the trailers. Holding the section
in `WholeResponse` puts the check before the section for every reader of a response body, which is
the invariant ("never reported as one ... not as a shorter object").

## Tests added

All in the brief's test file, `crates/validate/tests/s3_client_nonconforming_responses.rs`, except
one unit test.

- Helpers: `chunked_with_trailers` (`:184-192`), a whole chunked body whose terminal chunk carries
  a two-field trailer section; `AFTER_A_TRAILER_SECTION` (`:194-203`), what follows it: `junk`, a
  whole second response (`HTTP/1.1 200 OK … Content-Length: 0`), and more chunks
  (`5\r\nhello\r\n0\r\n\r\n`). These are the adversary's three variants.
- Error responses, GET, PUT and DELETE (`an_error_body_with_bytes_after_its_trailer_section_is_unreadable`,
  `:1283-1326`): a 503 chunked `SlowDown` + trailer section + each variant → unreadable, detail
  "after the end of the response". Guard: the same with nothing after it → clean
  `Service{503, SlowDown, message}`.
- PUT/DELETE 200 (`a_put_or_delete_success_with_bytes_after_its_trailer_section_is_unreadable`,
  `:1569-1606`): an empty chunked body whose terminal chunk carries the section, + each variant →
  unreadable, "after the end of the response". The body is empty so the overrun is what refuses it,
  not the no-body-on-success cap. Guard: nothing after it → `Ok`.
- GET 200: `chunked-trailer` → accepted byte for byte (`:1872-1882`); `chunked-trailer-then-*` →
  body error that names the overrun (`:2051-2064`).
- Unit test in `wire.rs` (`:358-460`): `frames` drives the production `exchange` against a loopback
  endpoint and reads the response body frame by frame through `SdkBody`.
  `a_trailer_section_is_handed_over_only_after_the_connection_shows_nothing_followed` checks a clean
  end gives `[data "hello", trailers {x-trailer: value}]`, and junk gives
  `[data "hello", Overrun(4)]` with no trailers. Why a unit test: the client's public calls never
  show trailers, so no integration case can tell "held, then handed over" from "held, then
  dropped". Mutation 2 below shows this test is the only one that catches the second. Both awaits
  are bounded (10 s), so a regression fails the test rather than hanging it.

The GET-200 cases `chunked-trailer` and `chunked-trailer-then-*` share their names with the
error-suite cases on GET, as several earlier cases already do (`chunked-no-terminal-chunk`,
`length-over-sent`, …). Each runs on its own endpoint, so nothing collides.

Test-file module docs updated to name trailer sections (`:11-19`, `:33-38`).

## Baseline on the unchanged base (`61e5341`), the C4-verify red leg

Production files put back to `61e5341` (`git checkout 61e5341 --` on `Cargo.toml`, `Cargo.lock`,
`crates/validate/Cargo.toml`, `s3.rs`, `s3/body.rs`, `s3/error.rs`, the roundtrip test and the 05
doc; `s3/response.rs` and `s3/wire.rs` stay on disk but are not compiled, because the base `s3.rs`
declares neither). Only the new test kept. It **compiles**: it names only #852/#854 public API and
existing dev-dependencies; no dev-dependency added. Result: 25 tests, **18 fail by assertion**, 7
pass, exit 101. Counted per case/operation run in the log: **218 runs, 137 red, 81 green**.
(Iteration 3 counted 163 pairs by merging shared names; this count does not merge.) Of this round's
24 new runs, 19 are red and 5 green. With the fix: 25/25, every run green. Tree restored from a
snapshot afterwards and checked byte-identical (`cmp` of `git diff 61e5341` before and after).

New rows this round (the rows from iterations 1-3 are unchanged; iteration 3's notes have them):

| Case | GET | PUT | DELETE | On the base, where red |
|---|---|---|---|---|
| `chunked-trailer-then-junk`, `-a-second-response`, `-more-chunks` (503 error) | R | R | R | clean `Service{SlowDown}` |
| `chunked-trailer` (503 error, guard) | g | g | g | — |
| `200-chunked-trailer-then-junk`, `-a-second-response`, `-more-chunks` | — | R | R | `Ok(())` |
| `200-chunked-trailer` (guard) | — | g | g | — |
| `chunked-trailer-then-*` (GET 200) | R | — | — | `Body(LengthUndeclared)`, not the overrun (the base refuses every chunked GET) |
| `chunked-trailer` (GET 200, guard) | R | — | — | `Body(LengthUndeclared)` |

On iteration 3 (the immediate pre-fix), only the three GET-200 `chunked-trailer-then-*` runs are
red (above).

## Mutations

1. **The old behaviour back**: a trailers frame handed over at once
   (`Ok(trailers) => return Poll::Ready(Some(Ok(Frame::trailers(trailers))))`). Both tests go red.
   The integration test fails the three GET `chunked-trailer-then-*` runs ("accepted as a 3000-byte
   object of 3000"). The unit test fails with `left: [Data(b"hello"), Trailers({"x-trailer":
   "value"}), Overrun(4)]`, `right: [Data(b"hello"), Overrun(4)]`: the section went out before the
   check.
2. **Hold the section, never hand it over** (`State::Ended` → `Poll::Ready(None)`): only the unit
   test goes red (`left: [Data(b"hello")]`), and the integration test stays 25/25. That is expected
   (no public call shows trailers), and it is why the unit test exists.

Each restored from the snapshot afterwards.

C5: `cargo mutants --list --file crates/validate/src/s3/wire.rs` (cargo-mutants 27.1.0) offers no
mutant inside the lines this round added. The only `WholeResponse` mutants replace the whole of
`poll_frame` (`wire.rs:312`), and iteration 3's `--re WholeResponse` run caught them. Making the
body hold its trailers does not make any of those replacements harder to catch, so I did not spend
a rebuild per mutant on them again.

## Refuting my own test

- **(a) Genuine red? Yes.** With the fix reverted to iteration 3's `poll_frame`, the three GET
  `chunked-trailer-then-*` runs fail by assertion (above). With all production reverted to
  `61e5341`, the test compiles and fails by assertion: 18 of 25 tests, 137 of 218 runs, exit 101.
  Mutations 1 and 2 above each go red on their own.
- **(b) Production path? Yes.** The integration test builds the client the way the binary does
  (`resolve_config` → `S3Client::with_deadlines`) and calls `get_object` / `put_object` /
  `delete_object`. Every response goes through the real SDK orchestrator, the production
  `WireClient` and `WholeResponse`, and hyper over real loopback TCP, then `ResponseGuard`,
  `classify` and `ObjectBody`. The unit test calls the production `exchange` and reads the
  production `SdkBody`/`WholeResponse` pair. Nothing is mocked; the only test-side code is the
  endpoint, which is the source of the fault.
- **(c) Fixture includes the fault? Yes.** The endpoint writes the exact bytes the reviewer and
  adversary used: `0\r\n` + trailer field lines + `\r\n`, then junk, a whole second response, or
  more chunks, in the same `write_all` as the response. The clean-trailer guards send the same
  section with nothing after it.

## Gates

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-validate --all-targets -- -D
  warnings`: clean (forced a re-check after the last edit). `typos crates/validate
  docs/design/architecture/05-building-block-view.md`: clean.
- `cargo test -p wyrd-validate`: 103 passed (unit 4, cli_surface 21, payload_and_sizes 26,
  nonconforming 25, roundtrip 15, upload_peers 12). The unit tests and the nonconforming suite ran
  10 times in a row: green every time.
- **`cargo xtask ci`, through the project's runner (`PDCA_WORKTREE=… ./engine/xtask.sh ci`): "xtask
  ci: all checks passed", exit 0.** 220 test-result lines ok, 0 failed. It ran on the tree before
  two last edits: a rewrap of one doc-comment line in `wire.rs` (my edit had left it at 114
  characters) and a 10 s timeout around the unit test's body read. After those, fmt, clippy, typos
  and the whole `wyrd-validate` suite were run again on the final tree (above). Neither edit
  touches non-test code.
- The same `cargo deny check` warning as iteration 3: `license-not-encountered` for `"ISC"`
  (`deny.toml:111`). It is not an error, and its cause is unchanged (see iteration 3's notes and
  "For the human").
- The repo has no other commit hooks (no `.pre-commit-config.yaml`, no `core.hooksPath`); fmt and
  clippy are its commit-time checks.
- I ran the single test as `cargo test -p wyrd-validate --test …` (the brief's own command) under
  a shell `timeout`. `cargo xtask` has no single-test subcommand, and the C4-verify script works in
  a `../wyrd-verify` worktree outside my write roots.

## For the human

- Carried from iteration 3, unchanged by this round:
  - a GET `304` with a nonzero `Content-Length` is unreadable, while PUT/DELETE take it as
    `Service{304, NoBody}`;
  - every request now opens a fresh connection;
  - an early success head outranks a refused body (`AcknowledgedEarly`);
  - the `deny.toml` ISC warning;
  - C4 diff coverage reporting "patch.diff does not apply on origin/main". This patch is against
    the stack base `61e5341`, where issue_744 is folded in, so it cannot apply to bare
    `origin/main`. That is a question of which base the gate resolves, not a patch defect.
- Same limit as before, now covering trailers too: bytes that reach the client after it has read
  the body's end are not seen (`wire.rs:24-26`). The fix makes "the body's end" include the trailer
  section. It does not widen what is seen beyond what has arrived.

No external dependency beyond the brief's (none); no NEEDS-HUMAN external-dependency item.
