# Build notes — #853 validate-s3-client-nonconforming-responses (iteration 3)

Target: getwyrd/wyrd @ main, built on the run's stack base `pdca-integration/main-r42c5348f203d`
at **`61e5341`** ("pdca-integrate: issue_744"), the same base as iteration 2. Removed lines are
cited on `61e5341`, added lines on the patched tree. `patch.diff` is `git diff 61e5341` of the
final tree (`git apply --check --reverse` against the tree: clean).

Starting point: iteration 2's `patch.diff`, which applies cleanly to `61e5341`, applied
unchanged; then the iteration-2 carry-forward on top. Nothing of iteration 2 is reverted except
where a point below says so.

## Step 0 — the base has #852's client

`crates/validate/src/s3.rs` with `s3/body.rs` and `s3/error.rs` on `61e5341`. No STOP. The four
`// deferred: #853` markers (base `s3.rs:264`, `:313`, `:343`, `s3/body.rs:442`) stay removed:
`grep -rn "deferred.*853" crates/validate` finds none.

## Step 1 — baseline on the unchanged base

Production files put back to `61e5341` (`git checkout 61e5341 --` on `Cargo.toml`, `Cargo.lock`,
`crates/validate/Cargo.toml`, `s3.rs`, `s3/body.rs`, `s3/error.rs`, the roundtrip test and the 05
doc; `s3/response.rs` and `s3/wire.rs` stay on disk but are not compiled, since the base `s3.rs`
declares neither), only the new test kept: the split C4-verify makes. It **compiles** (only
#852/#854 public API and existing dependencies; no dev-dependency added). `cargo test -p
wyrd-validate --test s3_client_nonconforming_responses` on the final test: 23 tests, **16 fail
by assertion**, 7 pass, exit 101. 112 case/operation pairs red, 51 green (163 in all; a few GET
case names are shared by the error suite and the GET-200 suite and are merged). With the fix:
23/23, 163 pairs green.

| Case | GET | PUT | DELETE | On the base, where red |
|---|---|---|---|---|
| `404-content-length-0`, `503-chunked-empty` | g | g | g | — |
| `error-without-code` | R | R | R | `Unreadable` (SDK "unhandled") |
| **`empty-code-element`, `empty-code`, `whitespace-code`** | R | R | R | `Code("")`, `Code(" \n\t ")` |
| `html-500`, `html-doctype-500`, `plain-text-500` | g | g | g | — |
| `unclosed`, `cut-mid-message`, `junk-after-error`, `element-after-error`, `pi-after-error`, **`pi-before-error`** | R | R | R | clean `Service{SlowDown}` |
| `comments-around-error` (guard; now a comment before the root too) | g | g | g | — |
| `two-codes`, `element-in-code`, `comment-in-code`, `element-in-message`, `escaped-text` | R | R | R | the SDK's lenient reading |
| **`nested-4000-deep`, `nested-past-the-limit`, `-past-the-limit-empty`, `-past-the-limit-dressed`** | R | R | R | clean `Service{SlowDown}` (the SDK's reader does not recurse) |
| **`nested-at-the-limit`, `nested-at-the-limit-empty`** (guards) | g | g | g | — |
| `length-over-sent` | g | g | g | — |
| `length-under-sent` (now must say the length reason) | R | R | R | clean `Service` |
| **`whole-document-then-junk`, `whole-document-then-a-second`, `chunked-document-then-junk`** | R | R | R | clean `Service{SlowDown}` |
| `chunked-and-length-agreeing` | R | R | R | `Service`; GET accepted as an object |
| `chunked-and-length-over` / `-under` | g | R | R | PUT/DELETE `Service` |
| `chunked-no-terminal-chunk`, `chunked-no-final-crlf` | g | g | g | — hyper `UnexpectedEof` |
| `gzip-coded-error` | R | R | R | `Service` |
| `chunked-nbsp-error` | g | g | g | — |
| `unreadable-success` | g | R | R | PUT/DELETE `Service{NoBody}` |
| `204-chunked`, `204-content-length-5`, **`204-chunked-nothing-sent`, `204-content-length-5-nothing-sent`** | g | R | R | `Ok(())` |
| `204-content-length-0`, `204-no-framing` (guards) | g | g | g | — |
| **`200-content-length-4`, `200-close-delimited`** | — | R | R | `Ok(())` |
| **`204-then-junk`** | — | R | R | `Ok(())` |
| **`304-transfer-encoding`** | g | R | R | PUT/DELETE `Service{304, NoBody}` |
| **`304-content-length`** (guard) | g | g | g | — (GET is the SDK's `ContentLengthError`; see "For the human") |
| `budget-less-1-*`, `budget-*` (guards) | g | g | g | — |
| `budget-plus-1-content-length`, `budget-plus-1-chunked` | R | R | R | clean `Service` with the 65,5xx-byte message |
| `error-over-budget` | R | R | R | read whole: 69,304,453 body bytes written |
| `success-over-budget` | — | R | R | read whole, `Ok` |
| `length-framed-16x-budget` | g | — | — | — |
| `chunked-4x-budget`, `chunked-empty` | R | — | — | `Body(LengthUndeclared)` |
| `close-delimited`, `chunked-cut-in-data`, `chunked-nbsp-cut`, `gzip`, `gzip-chunked` | g | — | — | — |
| **`length-2000-sent-3000`** | R | — | — | accepted as a 2000-byte object |
| **`chunked-then-junk`** | R | — | — | `LengthUndeclared` (the base refuses every chunked GET), not the overrun |

Bold rows are new this iteration. The brief's repro (a bodiless 404 as `Code("NotFound")`) does
not reproduce on #852: it checks the empty body first.

## The carry-forward, point by point

### 1. The crash: deep nesting overflows roxmltree's stack

Confirmed in the parser: `parse_element` (`roxmltree` 0.21.1 `tokenizer.rs:584`) calls
`parse_content` for an open element (`:638-640`), which calls `parse_element` for each child
(`:662`, `:683`): one stack frame pair per level, no limit. Fix: a byte scan before the parse,
`nests_deeper_than` (`s3/response.rs:444-489`), called at `:369-378`, refusing any element past
level `MAX_ERROR_DOCUMENT_DEPTH = 16` (`:438-442`, the root is level one) with an unreadable
whose detail names the limit. The scan never counts shallower than the parser goes: comments,
CDATA and PIs are skipped to their first terminator, as `parse_comment`/`parse_cdata`/`parse_pi`
do (`end_of`, `:490-496`); quotes are honoured inside a start tag (`start_tag_end`,
`:497-510`), where roxmltree allows `>` in attribute values (`tokenizer.rs:628`); any other `<!`
stops the scan (roxmltree refuses it; DTDs are off); every start tag is an element one level
below the open ones and opens that level unless it ends `/>`; every `</` closes one. Where the
two could disagree, the document is malformed and roxmltree stops there, so nothing past that
point can recurse deeper. 16 because an S3 error document is two deep, which leaves room for an
extension element inside a field; debug-build roxmltree uses about 10 KiB of stack per level
(the adversary's ~200 levels per 2 MiB), so 16 levels cost about 160 KiB.

Tests (`tests/…:1079-1159`), each on GET, PUT and DELETE: 4000 levels (28 KB, under the budget);
the limit exactly (a clean `Service`); one past it; empty elements at the limit (parsed); an
empty element one past it (refused); and one past it with a comment, a CDATA section and a PI
holding end tags halfway down and attribute values holding `/>` (refused). Mutation: with the
scan disabled, the test process dies, "thread … has overflowed its stack / fatal runtime error:
stack overflow, aborting" (SIGABRT), the adversary's failure.

Rejected: a non-recursive reader. The only one in the graph is `xmlparser` (the SDK's), a
tokenizer: end-tag matching, entity handling and the text-only checks would be hand-rolled on
top, where the rubric prefers the shared parser.

### 2. Bytes past the declared length: the connection-level check

The human chose to build it. Why it needs a connector: hyper reads exactly what the framing says
and leaves the rest in its read buffer or the socket. The SDK's connector
(`aws-smithy-http-client`, over hyper-util's pooled client) exposes neither, and its hook for a
custom TCP connector is `pub(crate)` (`aws-smithy-http-client` 1.5.0 `client.rs:245`
`wrap_connector`, `:516` `hyper_builder`). hyper-util spawns each connection with its error
mapped to a `debug!` (`hyper-util` 0.1.20 `client/legacy/client.rs:553`), so even hyper's own
"unexpected bytes on an idle connection" (`hyper` 1.10.1 `proto/h1/conn.rs:458`) never reaches
the caller, and comes after the response has been handed back as whole anyway.

So the client has its own connector, `crates/validate/src/s3/wire.rs` (module docs `:1-29`):

- `WireClient` implements the SDK's `HttpClient` (`:60-74`) and is the client's `http_client`
  (`s3.rs:160`). Its connector takes the connect deadline from the SDK's settings (`:76-88`);
  `connect_to` (`:156-178`) bounds the connect with `tokio::time::timeout` (tokio's runtime
  clock, as the operation deadline's sleep) and reports expiry as `ConnectorError::timeout`, so
  `classify`'s connect-phase arm works as before (the roundtrip suite's connect-deadline test
  passes).
- `exchange` (`:90-144`): a fresh TCP connection per request on
  `hyper::client::conn::http1`; the `Host` header (`host_header`, `:146-155`) and origin-form
  target as hyper-util sets them (`client/legacy/client.rs:298-319`, `:890-903`, `:978-984`);
  the connection driven by a task spawned on the caller's runtime, as hyper-util does.
- `drive` (`:180-202`) polls the connection with `poll_without_shutdown` until the body's reader
  asks, then takes it back with `into_parts` and counts `read_buf` ("bytes that have been read
  but not processed as HTTP", `hyper` 1.10.1 `client/conn/http1.rs:36-44`) plus a non-blocking
  `peek` of the socket (`unread`, `:204-221`). hyper keeps the bytes even after it has failed
  the idle connection over them, and `into_parts` works after an error, so both count. The task
  polls the connection before the question, so bytes the reactor has already signalled are read
  first. A socket that cannot be checked is an error, never a clean count.
- `WholeResponse` (`:276-330`) hands every frame over and, at hyper's end, asks the task; a
  nonzero count is `Overrun` (`:238-259`), a task with no count an error. The task answers from
  what it holds, so this wait is not on the endpoint. `ConnectionTask` aborts the task on drop
  (`:223-236`).
- GET: `ObjectBody::read` maps an `Overrun` in the error chain to the new
  `BodyError::Overrun { declared, received, after }` (`s3/body.rs:486-500`, `source_of`
  `:353-363`; `s3/error.rs:125-133`, Display `:256-269`). Buffered bodies: the SDK's read fails,
  so the response is `ResponseError`, `classify`'s last arm: `Unreadable` with status and
  request id.
- `length-under-sent` now fails for the length reason: its detail must say "after the end of the
  response" (`tests/…:1182-1199`). With the check disabled it reads "the root node was opened
  but never closed", the XML reason (mutation 2).
- New cases: a whole document then junk, then a second `<Error>`, a chunked document then junk
  (all three operations, `tests/…:1201-1232`); GET `200` declaring 2000 and sending 3000, and a
  chunked GET then junk (`:1905-1934`); `204` then junk on PUT/DELETE (`:1480-1497`).

Decisions:

- **No connection is reused.** A connection with bytes past a response cannot carry another
  request; a clean-looking one may still have extra bytes in flight; and reusing idle ones needs
  hyper-util's retry of a request that hit a connection the server had closed
  (`retry_canceled_requests`, `client/legacy/client.rs:1034`), without which a stale pooled
  connection is a spurious `NoResponse`, counted against the server by proposal 0017's taxonomy.
  Cost: GETs and DELETEs now pay what #854 already documented for PUTs, one fresh connection and
  one client `TIME_WAIT` socket each, so the ~470/s cap per client and gateway address pair
  (Linux defaults) now covers all requests together. Stated in `s3.rs:217-228`, `wire.rs:26-29`
  and the 05 doc. The separate no-pool `uploads` SDK client (base `s3.rs:100`, `:145-152`) is
  gone, since every request is now like it.
- **The task is spawned, not driven inline.** Driving hyper's connection from the request future
  and the body's `poll_frame` would poll a PUT's body inside the request's own poll; #854's
  `Turn` then sees the request never caught up with its wakes and the body waits forever, a
  livelock (traced through `body.rs` `Turn::body_waits` and `RequestFirst::poll`). Spawned, the
  connection task is again "the only one that polls the body" (#854's premise, `body.rs:177`).
- **The body does not wait for the connection to finish.** Awaiting `without_shutdown` at the
  body's end would hang a PUT answered early: its request body is never polled again (#854), so
  the connection never goes idle, and the error body would wait for the operation deadline.
  Asking the task and calling `into_parts` at once needs no idle connection.
- **The limit:** bytes that reach the client after it has read the response's end are not seen
  (`wire.rs:22-24`). The scripted endpoint writes head, body and surplus in one `write_all`; 8
  consecutive runs of the suite were green. The peek covers bytes that arrived but hyper has not
  read; it cannot be staged through the client deterministically (hyper reads whatever has
  arrived first), so `unread` has unit tests over real loopback sockets (`wire.rs:332-385`): two
  bytes in hyper's buffer plus four on the socket count six; a quiet and a closed socket count
  nothing. `host_header` has one too (the default port is left out).

Rejected, with cost: **a tap under hyper that parses the framing itself** (`httparse` for heads,
chunk sizes and trailers by hand) to know where the response ends: a second HTTP/1.1 response
parser (heads, 1xx interims, length, chunk-size lines, extensions, trailers) that must agree with
hyper byte for byte, roughly 150 lines, and it still needs the same custom connector (because
of `wrap_connector` above). Reading hyper's own leftover buffer uses hyper's parser instead.

Dependencies, the scope consequence of building the check: `crates/validate/Cargo.toml` drops
`aws-smithy-http-client` and adds `aws-smithy-runtime-api`, `hyper`, `hyper-util`; the root
`Cargo.toml` replaces the `aws-smithy-http-client` pin (only `wyrd-validate` used it;
`crates/server/Cargo.toml:132` declares its own version for its dev-dependency) with pins for the
three, minimal features; `Cargo.lock` changes only `wyrd-validate`'s dependency list. `cargo tree
-p wyrd-validate -e normal`: 142 crates before, 129 after, **none added**; removed:
`aws-smithy-http-client`, `base64`, `fnv`, `h2`, `indexmap`, `ipnet`, `openssl-probe`,
`rustls-native-certs`, `rustls-pki-types`, `socket2` 0.5, `tower`, `tower-layer`,
`tower-service`. Those edits are outside `crates/validate/`, which the brief keeps out of scope;
the human's decision is what brings them in, and the workspace pins every third-party crate in
the root manifest.

### 3. A PUT or DELETE 2xx with a body is not a success

`ResponseGuard` caps a buffered success at no byte (`Cap::NoBody`, `s3/response.rs:268`,
`:279-303`); its first data byte is `Refusal::BodyOnSuccess` (`:174-177`, Display `:196-200`),
an unreadable with status and request id. Every buffered success is a PUT's or a DELETE's: a GET
success streams and returns earlier (`:248-252`). Tests (`tests/…:1461-1499`): `Content-Length:
4` "junk", and a close-delimited "<Err", on PUT and DELETE. The `success-over-budget` cases now
expect this refusal; the bytes-written oracle is unchanged and still holds.

This broke two of #854's tests (`s3_client_upload_peers.rs:1263-1278`): their early
acknowledgement is a `200` with a 2-byte body sent late, and they expect `AcknowledgedEarly`.
Both outcomes are failures; the question was which to report. `put_object` now judges earliness
by the head (`s3.rs:249-265`): a success head that arrived before the source ended is
`AcknowledgedEarly` whether its body is then read whole (`Ok`) or refused while being read
(`SdkError::ResponseError` with a 2xx raw response). A head the SDK cannot read (`ServiceError`
with a 2xx) is no success head and stays `Unreadable` with the SDK's diagnostic, as the brief's
criterion asks. The base checked `Ok` only (`s3.rs:235`). All 12 upload-peer tests pass.

### 4. The small gaps

- **`304` with `Transfer-Encoding` on PUT/DELETE**: `Framing::of` takes whether the call is a
  GET (`s3/response.rs:80`); a `304` with any `Transfer-Encoding` to anything but a GET is
  untrusted (`:101-116`, RFC 9112 §6.1). `Content-Length` on a `304` stays allowed (RFC 9110
  §8.6). Tests `tests/…:1502-1561`.
- **A processing instruction before the root**: the check now covers every child of the
  document node except the root, not only the root's following siblings (`:388-401`). The XML
  declaration is no node in roxmltree, so `<?xml …?>` stays accepted (every document in the
  suite starts with one). Case `pi-before-error`.
- **An empty or whitespace-only `<Code>`** is `MissingInXml` (`:425-434`), trimming XML's `S`
  only. Three cases on all three operations.

## Other changes

- **The tests' detail checks matched the request id.** Every request id names its case
  (`853-error-over-budget-GET`) and the SDK's `DisplayErrorContext` quotes the response headers,
  so iteration 2's `detail_has: Some("budget")` passed on the request id alone (found when the
  new `BodyOnSuccess` text, which has no "budget", still passed). The checks now use phrases only
  the client's own reports carry (`tests/…:103-115`), and the module docs say why.
- `Expect::BodyErrorSaying` (`tests/…:631`): a GET body error whose text names the overrun, so
  the `ObjectBody` mapping to `BodyError::Overrun` is pinned.
- Docs updated to match: `BUFFERED_BODY_BUDGET` (`s3/response.rs:35-47`), the module docs of
  `s3.rs` (now seven things, `:1-37`), `response.rs` and `wire.rs`; `put_object` (`s3.rs:169-228`:
  three sentences described the SDK's connector) and `on_own_runtime` (`:416-420`); the 05 doc's
  `validate` paragraph (connector, new refusals, per-request connection cost, shipped crates).

## Mutation demonstrated (the brief's: remove the streaming limit, keep any declared-length check)

1. `s3/response.rs:340` `if this.received > this.cap.bytes()` → `if false && …` (there is no
   separate declared-length check to keep): 3 tests fail. `error-over-budget` on GET, PUT and
   DELETE: `Service{SlowDown, <69 MB message>}` and "the endpoint wrote the whole 69304453-byte
   body"; `success-over-budget` on PUT and DELETE: `Ok(())` and the whole 69,304,320 bytes;
   `budget-plus-1-*` on all three. With the fix the endpoint wrote 135,650 body bytes before the
   close, bound 34,652,160 (dominated by `tcp_rmem[2]` = 32 MiB on this host; the boundary pair
   pins the 64 KiB itself).
2. The overrun disabled (`wire.rs`: every count read as zero): 3 tests fail and all 16 overrun
   pairs go red (`length-2000-sent-3000` "accepted as a 2000-byte object of 3000",
   `chunked-then-junk` accepted whole, `204-then-junk` `Ok(())`, `whole-document-then-*` and
   `chunked-document-then-junk` a clean `Service{SlowDown}`, `length-under-sent` unreadable for
   the XML reason only).
3. The nesting scan disabled: the test process aborts with a stack overflow.

Each restored from the checkpoint commit afterwards; `git status` clean.

## C5 — `cargo mutants --in-diff` (as `scripts/mutants-in-diff`: `--no-shuffle --cap-lints=true`)

Full run on the patch before the last test additions: **149 mutants, 77 caught, 7 timeouts
(detected), 50 unviable, 15 missed.** The 15, and what became of each:

- `Framing::of`: delete the `204` arm. The overrun check now catches every `204` that sends a
  body, so only a `204` that frames one and sends nothing tells. Added
  `204-chunked-nothing-sent` and `204-content-length-5-nothing-sent` (`tests/…:1403-1404`); with
  the arm deleted they fail (GET "accepted as a 0-byte object of 5", PUT/DELETE `Ok`).
- `nests_deeper_than` `end - 2` → `end / 2`, `end_of` (×3) and `start_tag_end` (×5): no test had
  an empty element with a long name near the limit, end tags inside skipped markup, or `/>` in an
  attribute value. Added the `-empty` and `-dressed` nesting cases (the dressed one ends each
  construct with `</` just before its terminator, so a skip that stops short reads an end tag).
  While adding them I made the rule exact: an empty element past the limit is refused too (it is
  an element there), where the first version let it through.
- `exchange` `port != 80` → `true`: the endpoint never listens on port 80. Moved into
  `host_header` with a unit test.
- `unread` `+=` → `-=`: rewritten (`read_buf.len() + pending`) and unit-tested.
- `ResponseGuard::name` → `""` / `"xyzzy"` (×2): the label the SDK prints for the interceptor;
  nothing depends on its text. As in iteration 2.

Re-run on the final patch, restricted to the functions above plus `Framing::of`, `unread`,
`host_header`, `ConnectionTask` and `WholeResponse` (`--re`): **69 mutants, 47 caught, 3
timeouts, 17 unviable, 2 missed.** The 2:

- `unread`: the peek-error guard → `true` (every peek error counted as nothing). Only a peek
  error other than `WouldBlock`, `ConnectionReset` or `ConnectionAborted` reaches that branch,
  which no real connected socket produces on demand; the branch is the fail-closed answer to it.
- `Drop for ConnectionTask` → `()` (no abort). Equivalent in effect: dropping the
  `ConnectionTask` also drops the question's sender, which wakes the task and ends it on its next
  poll, closing the connection. The abort is kept so the task's end does not rest on that, and
  so a reader sees the rubric's "aborted on drop" directly.

The other timeouts are mutants that make the nesting scan loop forever or a request hang
(`+=` → `*=`, `end_of → Some(0)`, `drive` never polling, `exchange` refusing every URL), which
the suite catches by its timeout.

## Refuting my own test

- **(a) Genuine red? Yes.** Production reverted to `61e5341`, the new test kept (Step 1): it
  compiles and fails by assertion, 16 of 23 tests, 112 pairs, exit 101. With the fix: 23/23.
  Each new check also goes red on its own when its production line is disabled (mutations 1-3,
  the C5 cases).
- **(b) Production path? Yes.** The client is built as the binary builds it (`resolve_config` →
  `S3Client::with_deadlines`) and the test calls `get_object` / `put_object` / `delete_object`:
  every response passes through the real SDK orchestrator, the production `WireClient` and
  hyper over real loopback TCP, `ResponseGuard`, `classify` and `ObjectBody`. Nothing is mocked;
  the test side is only the endpoint, the fault source. The `unread` and `host_header` unit tests
  call the production functions, `unread` on real sockets.
- **(c) Fixture includes the fault? Yes.** The endpoint writes the exact non-conforming bytes:
  4000-deep and dressed nesting, a document then junk or a second document past its
  `Content-Length`, junk after a chunked terminal chunk, 3000 bytes against a declared 2000, a
  body on a PUT/DELETE success, a `304` with `Transfer-Encoding`, a PI before the root, empty
  codes, and the earlier cases; the budget cases generate their body lazily and count what was
  written until the client's close fails a write.

## Gates

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-validate --all-targets -- -D
  warnings`: clean. `typos` on the changed files: clean (it first flagged "DELETEs", reworded).
- `cargo test -p wyrd-validate`: 100 passed (unit 3, cli_surface 21, payload_and_sizes 26,
  nonconforming 23, roundtrip 15, upload_peers 12).
- **`cargo xtask ci` in the worktree: "xtask ci: all checks passed", exit 0.** Every stage ran:
  typos, docs lint and site render, gitlink/unsafe/blackbox guards (the blackbox guard confirms
  `wyrd-validate`'s normal closure holds no `wyrd-*`), `cargo fmt --check`, workspace clippy,
  build, workspace tests (220 binaries ok, 0 failed), `cargo-machete` (no unused dependency),
  `cargo deny` (advisories, all-features advisories, licences, bans, sources), conformance
  vectors, statics and deploy guards, madsim clippy and the DST suite.
- **One new `cargo deny` warning** (not an error): `license-not-encountered` for the `"ISC"`
  allowance (`deny.toml:111`) in the default-feature `cargo deny check`. Checked on the base with
  `cargo deny check licenses`: no warning there. Cause: `rustls-native-certs` ("Apache-2.0 OR ISC
  OR MIT") left `wyrd-validate`'s shipped graph with `aws-smithy-http-client`; it is still in
  the workspace only as `wyrd-server`'s dev-dependency, which that check does not count. The
  `--all-features` licence check, which `deny.toml:100-110` says the allowance exists for, passes
  without the warning. `deny.toml` is outside this patch's scope, so I did not touch it.
- The repo configures no other commit hooks (no `.pre-commit-config.yaml`, no
  `core.hooksPath`); fmt and clippy are its commit-time checks.

## For the human

- **GET `304` with a nonzero `Content-Length` is unreadable; PUT and DELETE take it as
  `Service{304, NoBody}`.** HTTP allows the header on a `304` (RFC 9110 §8.6), and the
  carry-forward kept it allowed. On a GET the SDK's own interceptor checks every GET response's
  body against `Content-Length`, bodiless statuses included (`aws-smithy-runtime` 1.16.0
  `client/http/body/content_length_enforcement.rs:131-173`), and reports the empty body as a
  `ContentLengthError`. The base does the same (the case is green there). The client cannot turn
  that check off (its marker type is private) short of rewriting the response's headers, which I
  did not do. The client sends no conditional GET, so a `304` is never a conforming answer to it
  either way. The test records the difference (`tests/…:1530-1561`) rather than hiding it.
- **Every request now has a fresh connection** (point 2): a throughput cost on GETs and DELETEs
  that #854 accepted for PUTs.
- **An early success head outranks a refused body** (point 3): `AcknowledgedEarly` rather than
  `Unreadable` for an early PUT acknowledgement whose body is then refused.
- **The `deny.toml` ISC warning** above: either accept it, or a one-line follow-up there.
- **C4 diff coverage** reported "patch.diff does not apply on origin/main" in both earlier
  rounds. This patch is written against the stack base `61e5341`, like iteration 2's; it edits
  lines issue_744 (folded into that base) also edits, so it cannot apply to bare `origin/main`. A
  gate base-resolution question, not a patch defect.

No external dependency beyond the brief's (none); no NEEDS-HUMAN external-dependency item.
