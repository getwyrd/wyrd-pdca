# Build notes — #852 validate-s3-client-core (iteration 2)

Withheld from the reviewer. For the human at sign-off.

## Base

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l2`, HEAD `0b48ab7` on
  `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (the bundle's `stack-base`). The
  brief named tip `022d76f`; the branch has since folded #842 (`0b48ab7`). Both carry #775.
- STOP checks from the brief, all passed: `git merge-base --is-ancestor 343be83 HEAD` → ok;
  `ls crates/validate` → `Cargo.toml src tests`; `grep -n validate Cargo.toml` → `:33`. #774's
  crate and #775's lint were not recreated.
- Starting point: iteration 1's `patch.diff`, applied cleanly to `0b48ab7`. The carry-forward
  findings cite that patch's lines, so this iteration fixes them in place rather than starting
  over. The dependency move, the audit and the error type are unchanged from iteration 1.
- `patch.diff` is `git diff HEAD` with the four new files added intent-to-add, against
  `0b48ab7`. `git apply --check -R patch.diff` on the patched tree → clean.

## What this iteration changed, finding by finding

`path:line` is on the patched tree.

| Carry-forward finding | Fix | Proof |
|---|---|---|
| **R1** (C5 causal adequacy, T4 3× blocking, code-review): `body.rs:101` skipped empty pieces in a `loop`, so an always-ready empty source spun inside one `poll_frame` | `crates/validate/src/s3/body.rs:107-117`: an empty piece now ends the poll: `cx.waker().wake_by_ref(); return Poll::Pending`. One poll handles at most one source item (doc `:77-79`). | New tests `an_always_ready_empty_source_yields_to_the_deadline_on_a_{current,multi}_thread_runtime` (`tests/s3_client_roundtrip.rs:1449`, `:1454`), with exactly the reviewer's source (`stream::repeat_with(\|\| Ok(Bytes::new()))`, declared length 10, a peer that reads and never answers, 1 s operation deadline). Mutation viii below. |
| **R2** (T5): the PUT lag oracle credited aws-chunked framing as forwarded payload | The relay now decodes `aws-chunked` as it forwards and credits only chunk data: `HeadScan::header` (`tests/…:476`), `Framing` (`:522`, `of_request` `:538`, `payload` `:549`), used in `pump_up` (`:682`). | The streaming PUT test asserts the relay credited **exactly** the object's length (`:1006-1018`). Mutation t1 (credit framing again) fires that assertion: 39,064,150 credited against 39,010,304. |
| `s3.rs:212`: guard `failure.is_timeout()` → `true` survived; no test produced `NoResponse` | New test `a_refused_connection_is_no_response_not_a_timeout` (`tests/…:1311`). It uses a socket that is bound but not listening, instead of the suggested "bind, read port, drop": the kernel refuses connections to it at once, and no parallel test can grab the port in between. | Mutation vii fires: `Some(Timeout { phase: Connect, limit: 5s })`. |
| `s3.rs:225`: swapping in the SDK's generic `RequestId` accessor survived; nothing sent both headers | The relay can add a decoy `x-amzn-requestid` after the status line (`with_decoy` `tests/…:707`, `pump_down` `:719`, `Relay::start_with` `:629`). `error_matches_wire` (`:926`) always uses it and asserts the gateway's id is not the decoy (`:938`). | Mutation vi fires: typed id `decoy-x-amzn-requestid-852` against the wire's `b8d1167d95abd3700000000000000000`. |
| C5: 11 missed mutants | See "C5 missed mutants" below. | — |
| C4 verification: "pre-fix discriminator never executes" | Not changeable: the brief declares this net-new API, so the pre-fix red is a compile failure and C4-verify scores `UNVERIFIABLE`. Re-confirmed below. | — |
| C4 diff coverage: "patch.diff does not apply on origin/main" | Not a patch defect. See "For the human" at the end. | — |

### Why the empty-piece fix works (and why the connection now closes)

- Passing empty frames on to the SDK would not help. `aws-runtime 1.10.0
  src/content_encoding/body.rs:147-161` buffers an empty data frame and returns
  `Ready(Ok(true))`, and its caller `content_encoding/body/http_body_1_x.rs:79` `continue`s, so the
  SDK spins the same way. The body has to stop the loop itself.
- Returning `Pending` with a self-wake hands control back to the scheduler on each empty piece.
  So tokio's timer driver runs, and the SDK's operation deadline fires, even on a
  current-thread runtime.
- When the deadline drops the request, hyper's client dispatcher sees its callback cancelled:
  `hyper 1.10.1 src/proto/h1/dispatch.rs:292-300` (`poll_read_head` → `poll_ready` →
  `poll_canceled`, `:745-753`). It then closes, and the socket is dropped. While the request
  body is being written the connection can read a head (`conn.rs` `can_read_head`), so this check
  runs on the next poll. Before the fix that next poll never came. The test checks the result:
  the peer reads EOF within `CLOSE_WAIT` (`tests/…:1339`).
- The test also checks that the run's runtime shuts down (`tests/…:1435-1445`). For a multi-thread
  runtime that needs every worker back from the task it is polling.

### How the empty-source test stays bounded when the bug is present

Inside one non-yielding poll, no tokio timer can fire, the test's own `tokio::time::timeout`
included. So `empty_piece_put` (`tests/…:1360`) runs the PUT on a thread of its own, and the test
thread waits on a `std::sync::mpsc` channel with `recv_timeout(STALL)` (`:1397`). If the run
spins, the test panics with a message and leaves that thread behind; the test binary still
exits, because it does not wait for other threads. If the run reports, the test joins the thread
with a bounded wait (`:1435-1445`). If the run panics before reporting, its panic is re-raised
(`:1409-1413`). Measured with the bug present: current-thread fails after 31 s, multi-thread after
11 s, and both processes exit.

## The client, as built (unchanged from iteration 1 except where noted)

- **Config** (`s3.rs:89-122`) copies the cited peer `crates/server/tests/s3_gateway_cluster.rs:100-115`:
  `behavior_version_latest`, region, `endpoint_url`, static `Credentials`, explicit
  `aws_smithy_http_client::Builder::new().build_http()`, `force_path_style(true)`,
  `RetryConfig::disabled()`, `StalledStreamProtectionConfig::disabled()`. On top of that, an
  explicit `TimeoutConfig` (`:93-98`) sets connect and operation, and disables read and
  operation-attempt timeouts, so no timeout comes from behaviour-version defaults.
- **Deadlines** (`s3.rs:44`, defaults `:56`): connect 10 s, operation 15 min, body-idle 60 s.
  Connect is the connector's only timeout, so a `DispatchFailure` timeout is the connect phase
  (`:209`). Operation is the SDK orchestrator's (`SdkError::TimeoutError`). Body-idle is
  `tokio::time::timeout` around each piece (`body.rs:211`). All three run on tokio's runtime
  clock (rubric: one clock per lifecycle). The SDK's SigV4 date stamp reads the wall clock, but
  that belongs to the server's freshness check, not to a client deadline. There is no new clock
  read in this iteration.
- **`classify`** (`s3.rs:195-258`). **Changed:** the explicit `DispatchFailure(_)` and
  `ResponseError` arms are gone. Both computed exactly what the fallback computes
  (`SdkError::raw_response()` is `Some` for `ResponseError`, `None` for `DispatchFailure`), which
  is why cargo-mutants could delete each without effect. The fallback (`:244-257`) now carries the
  comment and a `// deferred: #853` marker for the unreadable-response side, which only a
  non-conforming server reaches.
- **Request id** read from the `x-amz-request-id` header itself (`s3.rs:262`), never via the
  SDK accessor, which prefers `x-amzn-requestid` (`aws-types 1.6.0 src/request_id.rs:44-48`).
- **Typed error** (`error.rs`): `Service`, `Unreadable`, `NoResponse`, `Timeout { phase, limit }`,
  `RequestNotBuilt`, `Body(BodyError)`, and `ErrorCode::{Code, MissingInXml, NoBody}`. Unchanged.
- **PUT body** (`body.rs:32-146`). **Changed:** the empty-piece fix above, plus the
  `is_end_stream` and `size_hint` overrides are removed (see "What I ruled out").
- **GET body** (`body.rs:166-249`): unchanged, plus a `// deferred: #853` marker at `:217` on the
  two length checks that only non-conforming framing can trigger.
- **Removed:** `S3Client::deadlines()` (4 lines). It had no caller, and its only test coverage
  would have been a getter assertion. A later slice that needs it can add it with its caller.

## Criterion 3: `W` and `K`

Derivation table: test module docs, `tests/s3_client_roundtrip.rs:34-72`. Computed at `:147-158`.

| Term | Bound | Source |
|---|---|---|
| SDK re-chunking | 64 KiB + one 16 KiB piece | `aws-runtime-1.10.0/src/content_encoding.rs:26`, body loop `content_encoding/body/http_body_1_x.rs:55-80` |
| hyper write buffer | 417,792 + one 64 KiB signed chunk + ≤128 B framing | `hyper-1.10.1/src/proto/h1/io.rs:23`, `:575-582` |
| client socket send queue | `tcp_wmem[2]` (4 MiB here, read from `/proc`, never set) + one 64 KiB loopback segment | the SDK socket autotunes up to `tcp_wmem[2]` |
| relay socket receive queue | 2 × 16 KiB (`SO_RCVBUF`, doubled by the kernel) | the relay's own socket, set before `listen` |
| relay read buffer | 16 KiB | the relay's own |

On this host **W = 4,874,368 bytes (≈4.65 MiB)**. **Payload = 8 × W rounded up to whole 16 KiB
pieces = 39,010,304 bytes (≈37.2 MiB).** **K = 64 KiB / 16 KiB = 4.**

**What "forwarded" means now.** Payload bytes only. The relay finds the request head, reads its
`Content-Encoding`, and for `aws-chunked` credits only chunk data. The head, size lines, chunk
signatures, CRLFs and the checksum trailer are never credited. So lag = payload produced − payload
forwarded, exactly, and the reviewer's `W + H` slack is gone. Framing still takes space in the
buffers of the table, so payload in flight is at most their sum, `W`. The decoder is checked at
the end of the PUT: credited must equal the object length (`:1006-1018`).

Measured with the exact count (3 runs, temporary print, then removed and the file `cmp`-checked):

| Run | max PUT lag | at (pull, produced, forwarded) | max live pieces |
|---|---|---|---|
| 1 | 2,646,822 | (586, 9,601,024, 6,954,202) | 4 |
| 2 | 2,736,842 | (952, 15,597,568, 12,860,726) | 4 |
| 3 | 2,826,048 | (2012, 32,964,608, 30,138,560) | 4 |

That is 54-58% of W. Live pieces reach the derived K exactly.

**GET.** The relay writes at most W response-body bytes past what the test has taken
(`Downstream::Paced`). Every wait is bounded by `STALL = 30 s`, and a stall panics with the
offset, W and the relay's written count.

## Mutations (each applied to a copy, run, recorded, restored; restore checked with `cmp`)

Runner: `cargo test -q -p wyrd-validate --test s3_client_roundtrip <test> -- --exact` under
`timeout 400`. The mutant files are kept under `$PDCA_SCRATCH/pdca-builder-852-mut/v2-*`.

| # | Mutation (where) | Test | Result |
|---|---|---|---|
| i | Buffer the whole PUT into a `BytesMut`, send one frame (`body.rs` `poll_frame`) | `put_streams_within_the_window_at_every_pull` | **RED**: "PUT lag: at pull 2381 the source had produced 39010304 bytes but the relay had forwarded 0; 39010304 bytes held between them exceeds W = 4874368" |
| ii | Forward the first 2 MiB, then collect the rest | same | **RED**: "… produced 39010304 … forwarded 1835008; 37175296 bytes held … exceeds W". Retention passes here, so only the lag oracle catches it. |
| iii | Keep every forwarded piece (`std::mem::forget(piece.clone())`) | same | **RED**: "PUT retention: 2381 of the source's pieces were alive at one pull; K = 4". Lag passes here, so only the retention oracle catches it. |
| iv | Collect the whole GET (`output.body.collect()`), hand it over (`s3.rs` `get_object`) | `get_hands_over_pieces_within_the_window` | **RED**: "GET lag: the client handed over nothing for 30s at offset 0 while the relay had written 4874368 body bytes, W = 4874368 …". Chunk counts are not used, so "collect, then re-chunk" cannot pass. |
| v | Hand over pieces until 2 MiB, then collect the rest (`body.rs` `read`) | same | **RED**: "GET lag: … at offset 2097152 while the relay had written 6971520 body bytes …" (2 MiB taken + W). |
| vi | Request id from the SDK's generic `RequestId` accessor (`s3.rs` `classify`) | `error_response_fields_equal_what_the_gateway_sent` | **RED**: `request_id: Some("decoy-x-amzn-requestid-852")` against the wire's `Some("b8d1167d95abd3700000000000000000")` |
| vii | `DispatchFailure(_) if true` (every dispatch failure is a connect timeout) | `a_refused_connection_is_no_response_not_a_timeout` | **RED**: "a refused connection is no response: Some(Timeout { phase: Connect, limit: 5s })" |
| viii | Iteration 1's code: skip empty pieces in a `loop` | `…_current_thread_runtime` | **RED** after 31 s: "a PUT from an always-ready source of empty pieces neither finished nor timed out within 30s: its body spins inside one poll, where the 1s operation deadline cannot fire" |
| viii | same | `…_multi_thread_runtime` | **RED** after 11 s: "the connection was still open 10s after the PUT gave up: the abandoned body is still being driven" (the PUT itself returned the timeout, as the reviewer measured) |
| ix | `S3Error::source` → `None` (`error.rs`) | `put_source_that_ends_short_runs_long_or_fails_stores_nothing` | **RED**: "the body error is the error's source", left `None`, right `Some(SourceLength { declared: 262144, produced: 131072 })` |
| t1 | Test side: the relay credits aws-chunked framing as payload (iteration 1's oracle) | `put_streams_within_the_window_at_every_pull` | **RED**: "the relay credited exactly the payload as forwarded", left 39064150, right 39010304 (the 53,846 B of framing) |

Mutations i–v are the brief's five; vi–ix answer the carry-forward; t1 shows the decoder check is
what binds R2. Mutation viii was re-run after the final test edit (thread ownership), with the
same two results.

## C5 missed mutants from iteration 1: what happened to each

| Iteration-1 mutant | Now |
|---|---|
| `s3.rs:126` `S3Client::deadlines` → `Default::default()` | accessor removed |
| `s3.rs:216` delete arm `DispatchFailure(_)` | arm removed (it matched the fallback exactly) |
| `s3.rs:217` delete arm `ResponseError` | arm removed (same reason) |
| `s3.rs:212` guard `is_timeout()` → `true` | killed by the refused-connection test (vii) |
| `body.rs:137` `is_end_stream` → `true` / `false` | method removed |
| `body.rs:142` `size_hint` → `Default` / `-` → `+` | method removed |
| `body.rs:232` guard `received == declared` → `true` | still unreachable with a conforming response (now `body.rs:239`); `// deferred: #853` at `body.rs:217` |
| `error.rs:204`, `:205` `source` → `None` / delete arm | killed by the source assertion (ix) |

The C5 gate's own command was re-run on this iteration: 1 missed of 65 (see "cargo mutants on
this iteration" below).

## Criterion 3, reviewed not tested: no accumulating buffer in either body path

`grep -nE "collect|aggregate|into_bytes|Vec<|Vec::|BytesMut|extend|push|SegmentedBuf|copy_from_slice|to_vec|clone\(\)"`
over `crates/validate/src/s3.rs` and `crates/validate/src/s3/*.rs` matches only:
- `body.rs:7`, a doc comment;
- `body.rs:153-154`, `error_chain`, which joins error-message strings, not body bytes;
- `.clone()` of config strings (`s3.rs:108`, `:119`) and of `BodyError`/`S3Error` values
  (`s3.rs:146`, `body.rs:102`, `:143`, `:196`, `:203`), none of them bytes.

- **PUT path.** `DeclaredLengthBody`'s fields are `pieces, declared, produced, ended, failure`
  (`body.rs:84-90`). There is no byte container. `poll_frame` passes each piece on by value
  (`body.rs:122`) and keeps no reference. An empty piece is dropped where it is matched
  (`:114`). `put_object` (`s3.rs:126-156`) hands the body to the SDK and never touches bytes.
- **GET path.** `ObjectBody`'s fields are `stream, declared, received, idle, ended, failed`
  (`body.rs:166-173`). Only the length is counted, and the piece is returned by value
  (`body.rs:232`). `get_object` (`s3.rs:160-180`) moves `output.body` into `ObjectBody` unread.
- Below the client, the SDK's only buffering is the bounded 64 KiB aws-chunked buffer (PUT). The
  GET response path has none, because stalled-stream protection is off.

## Refuting my own test

- **(a) Genuine red? Yes.** Reverse-applied `patch.diff` to the worktree (back to `0b48ab7`),
  put back only `tests/s3_client_roundtrip.rs`, and ran it. It fails to compile with 37 errors,
  among them `E0432 unresolved imports wyrd_validate::BodyError, … PutOutcome, PutSource,
  S3Client, S3Error`. That is the brief's predicted criterion-absence red, which C4-verify scores
  `UNVERIFIABLE`. Re-applied the patch; the test file `cmp`s equal. The demonstrated reds are the
  11 mutation runs above, each firing the assertion it targets. After restoring, the file is
  green: 14/14 on every run (9 runs across the edits, the last 3 on the final tree) and inside both
  `cargo xtask ci` runs (the second on the final tree).
- **(b) Production path? Yes.** Every test drives `wyrd_validate::S3Client`, built through the
  production `resolve_config` (`tests/…:322`). It runs the real `aws-sdk-s3` over real loopback
  TCP into the real `S3Gateway` (`Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>`,
  256 KiB chunks, `:290`), copied from the cited `s3_http_wire.rs:56-92` without
  `serve_s3_role`. There are no mocks. The relay forwards bytes; the only thing it adds is the
  decoy header, and the oracle compares against what the gateway sent before that was added.
- **(c) Fixture includes the fault? Yes.**
  - The empty-piece source is the reviewer's exact source, against a real peer that reads and
    never answers, on both runtime flavours.
  - The refused connection is a real kernel refusal (a bound socket with no listener).
  - The decoy is a real second header on the wire, in front of the gateway's own.
  - The cut is a real close after 262,157 body bytes. The held tail is a real stalled connection.
    The full accept queue is a real `listen(1)` with queued connections. The silent peer really
    accepts and never answers. The bad source pieces are real. The 403 comes from a real
    wrong-secret signature.

## The dependency move: ADR-0003 §2 audit

Unchanged from iteration 1. This iteration touched no manifest, lockfile or deny file.

Decision record: per the brief's table (#741 `notes.json`, `iteration-v3/brief.md:10-24`,
`iteration-v3/SUMMARY.md:293-297`). Not re-opened here.

**Floor.** `aws-sdk-s3 = "1.144.0"` (`Cargo.toml:91`). From the local registry index cache:

| Version | `lru` requirement |
|---|---|
| 1.141.0 – 1.143.0 | `^0.16.3` |
| 1.144.0, 1.145.0, 1.148.0 | `^0.18.2` |

So 1.144.0 is the first release whose `lru` is past the fix. The lock resolves `aws-sdk-s3
1.148.0`, `lru 0.18.4`, `aws-smithy-http-client 1.4.2` (`Cargo.toml:94`). `Cargo.lock` gains only
`wyrd-validate`'s 13 dependency edges (one hunk). `crates/server/Cargo.toml:131-135` still
declares its own dev-dependency versions; not edited (out of scope), and they resolve to the
same locked versions.

**Waiver deletion** (`deny.toml` base `:77-86`, `deny-all-features.toml` base `:104-111`).
- Before: on the base, cargo-deny printed `warning[advisory-not-detected]` at
  `"RUSTSEC-2026-0253"`; the entry's own removal trigger had fired.
- After: inside `cargo xtask ci`, all three cargo-deny invocations pass ("advisories ok, bans ok,
  licenses ok, sources ok"; "advisories ok"; "bans ok, licenses ok, sources ok").
- `cargo tree -i lru -e normal` → `lru 0.18.4 ← aws-sdk-s3 1.148.0 ← wyrd-validate`. The
  advisory is not in the graph; no exposure was accepted. **Tracker note for #741 (sign-off
  should mirror it):** "the RUSTSEC-2026-0253 waiver is deleted because the advisory is not in
  the graph: the `aws-sdk-s3 >= 1.144.0` floor requires `lru ^0.18.2`, and the lock resolves
  `lru 0.18.4`."

**Transitive surface.** `cargo tree -p wyrd-validate -e normal` lists 141 packages. New to the
shipped normal graph (that set minus the rest of the workspace's normal graph): **81 third-party
crates.**
- **Licences** (all on the `deny.toml` allowlist): 27 `MIT OR Apache-2.0`; 18 `Apache-2.0` (all
  `aws-*`); 18 `Unicode-3.0` (ICU4X, `litemap`, `tinystr`, `writeable`, `yoke*`, `zerofrom*`,
  `zerotrie`, `zerovec*`); 8 `MIT` (`base64-simd`, `generic-array`, `http-body 0.4`, `lru`,
  `outref`, `spin`, `synstructure`, `vsimd`); 5 `Apache-2.0 OR MIT`; and one each of
  `Apache-2.0/MIT` (`bytes-utils`), `Zlib` (`foldhash`), `Apache-2.0 OR ISC OR MIT`
  (`rustls-native-certs`), `Apache-2.0 OR BSL-1.0` (`ryu`; BSL-1.0 is the Boost licence, not the
  Business Source licence deny.toml bans), `MIT/Apache-2.0` (`xmlparser`).
- **Named new crates:** the 18 `aws-*` crates; `http 0.2` and `http-body 0.4`
  (`aws-runtime/http-02x`); the checksum stack `crc-fast`, `crc32fast`, `md-5`, `sha1`,
  `digest 0.10`; `lru`, `bytes-utils`; `url`/`idna`/ICU4X; `time`, `uuid`, `regex-lite`,
  `xmlparser`, `base64-simd`; and through `default-client`, **`rustls-native-certs`,
  `rustls-pki-types` and (Unix) `openssl-probe`, but not `rustls`** (no `rustls`, `ring` or
  `aws-lc` in validate's normal tree). This is the point round 1 got wrong.
- **Unsafe posture:** `aws-sdk-s3` is `#![forbid(unsafe_code)]`. Smithy crates: `aws-smithy-json`
  2 sites and `aws-smithy-types` 1 (`from_utf8_unchecked` after validation),
  `aws-smithy-eventstream` 2 (an `unsafe impl BufMut`), `aws-smithy-schema` 4 (pointer reborrow
  and lifetime transmute in its serializer), `aws-smithy-http-client` 1 (in `test_util`, feature
  off). The heavier unsafe is outside smithy: `crc-fast` (361 sites, SIMD CRC), `lru` (94, its
  linked list, now at the fixed 0.18.4), `bytes-utils` (12), `openssl-probe` (5).
- **Maintenance:** AWS's official SDK, generated by smithy-rs, versioned in lockstep; already
  tracked as a dev-dependency. Release dates were not checked (no network).

## Production reach: the two limits the brief asks to record

1. **Plain HTTP.** The client is built on `build_http()` (no TLS). Deployment puts TLS in front
   of the gateway (proposal 0017 §10; `crates/gateway-s3/src/lib.rs:50-57`). An `https://`
   endpoint fails as `NoResponse`. TLS is out of scope.
2. **Composition.** The proof runs against redb + `MemCoordination` + local `FsChunkStore` (the
   `s3_http_wire.rs` composition), not production's FDB/TiKV + etcd + remote D-servers. The S3
   wire surface and its streaming are the same `S3Gateway` code; backend performance and failure
   shapes are not.

## Deferred markers (answer review findings on them with "Deferred — tracked in #853/#854")

- `s3.rs:148` `// deferred: #854`: a peer that acknowledges a PUT before reading the whole body,
  or stops reading.
- `s3.rs:169` `// deferred: #853`: close-delimited and chunked GET framing (refused as
  `LengthUndeclared` meanwhile).
- `s3.rs:217` `// deferred: #853`: non-conforming error responses (no error-body budget;
  empty-body errors classified `NoBody` but exercised only there; code-less `<Error>` vs non-XML).
- `s3.rs:247` `// deferred: #853` (new): the `Unreadable` side of the fallback arm.
- `body.rs:217` `// deferred: #853` (new): the GET length checks that only non-conforming framing
  can trigger (iteration 1's surviving `received == declared` guard mutant).

## Gates run (all in the worktree)

- `./engine/xtask.sh ci` (the C4-ci gate) → **exit 0**, twice: once after the main changes and
  once on the final tree. It ran typos, docs lint and render, the unsafe, blackbox and gitlink
  guards, `cargo fmt --check`, workspace clippy (all targets), build, `cargo test --workspace`
  (s3_client_roundtrip: 14 passed), cargo-machete, all three cargo-deny invocations, statics,
  deploy-guard, and the madsim DST clippy and test.
- `cargo fmt --all` applied; `cargo clippy -p wyrd-validate --all-targets -- -D warnings` clean.
  The target's commit hook has nothing to change.
- Test runtime: the whole file takes about 5 s in a debug build.

## Rubric self-review

- **One clock per lifecycle:** no new production clock read. The fix adds a self-wake, not a
  timer.
- **Narrow seams, dependency direction:** unchanged; blackbox guard green.
- **No DST-reachable globals:** the test adds `const`s only; statics gate green.
- **forbid(unsafe_code):** unchanged; the test file carries it.
- **Docs currency:** the public surface lost `S3Client::deadlines()`, which the architecture doc
  never mentioned. `05-building-block-view.md:253` still describes the client correctly.
- **Await discipline:** this is the class R1 was about. Every await on the endpoint is bounded,
  and now the bound holds for any source, on any runtime flavour. The abandoned connection
  closes after the deadline. Production spawns no task. Test fixtures: every tokio task is in a
  `JoinSet`. The empty-source run's thread is joined on every path that reports (bounded wait),
  and is left behind only when it spins and cannot be joined, which is the failure being tested.
- **Absent or unsupported entries:** a refused connection is now proven to be `NoResponse`, not
  a fabricated timeout (invariant (b): every reported fact is real).
- **Test fidelity:** the relay's payload count is checked for exactness, so the PUT oracle
  cannot pass on framing credit.
- **Out of scope honoured:** no edit under `crates/server/`, no gateway change, no TLS, no
  `smoke` or capability matrix, no 5 GiB property, nothing from #853/#854 implemented.

## What I tried or ruled out (with costs)

- **Capping skipped empties at N per poll (the reviewer's wording) instead of one.** Cap-1 is
  the arm at `body.rs:114-117`: 2 statements, no counter, no constant. Cap-N adds a counter
  local, a constant and a comparison (about 6 lines) and a tuning value that no test can tell
  apart from 1. The only cost of cap-1 is one task reschedule per empty piece, which matters
  only for sources that yield many empty pieces. Chose cap-1.
- **Passing empty pieces through as empty frames.** Does not fix it: the SDK's aws-chunked layer
  spins on them (`aws-runtime 1.10.0 content_encoding/body.rs:147-161`).
- **Keeping `size_hint`/`is_end_stream` and testing them.** The SDK takes the decoded length from
  the `Content-Length` header first and reads the body hint only when the header is absent
  (`aws-sdk-s3 1.148.0 src/aws_chunked.rs:108-113`). `put_object` always sets the header
  (`s3.rs:139`), so the hint is never read. Testing it would need a `#[cfg(test)]` module in
  `body.rs` (about 25 lines) for a value nothing reads. Removing it deleted 8 lines and two
  surviving mutants. The http-body defaults (`false`, unbounded) are always correct.
- **Keeping the explicit `DispatchFailure(_)`/`ResponseError` arms in `classify` for
  readability.** They were exact duplicates of the fallback, so no test can bind them. The
  fallback's comment now names both cases. Net −10 lines.
- **Approximating framing in the relay instead of decoding it.** Iteration 1 rejected decoding
  as about 40 lines. The reviewer showed the approximation leaves `W + H` slack. Decoding cost
  `HeadScan::header` (11 lines) plus `Framing` (about 75 lines), and it is exact. Turning
  checksums or chunking off to get raw bytes on the wire was ruled out again: it changes
  production behaviour (drops the CRC32 trailer the gateway validates) to suit a test.
- **"Bind, read the port, drop the listener" for the refused-connection test (the reviewer's
  wording).** Between the drop and the connect, a parallel test can bind the same ephemeral
  port. A socket that is bound but not listening holds the port and is still refused by the
  kernel. Same line count.

## cargo mutants on this iteration

The C5 gate's command, `cargo mutants --in-diff patch.diff --no-shuffle` (output kept in
scratch, not in the worktree), on the final `patch.diff`:

```
Found 65 mutants to test
MISSED   crates/validate/src/s3/body.rs:239:21: replace match guard self.received == self.declared with true in ObjectBody::read
65 mutants tested in 2m: 1 missed, 20 caught, 44 unviable
```

Iteration 1 had 11 missed of 75. The one left is the guard on a clean end of a GET body short of
its `Content-Length`. hyper never produces that for a length-delimited response (it reports a
cut as a transport error, which `get_cut_mid_body_is_the_body_error_never_a_shorter_object`
covers). Only #853's non-conforming framings reach it, so it carries the `// deferred: #853`
marker at `body.rs:217`.

## For the human (no NEEDS-HUMAN external dependency)

The build used the base toolchain only: no container, no MinIO, no network.

1. **Mirror the waiver sentence onto #741** at sign-off (the brief asks for this; text above).
2. **C4 diff coverage cannot measure this bundle.** `engine/scripts/run-diff-cov.sh` applies
   `patch.diff` to `origin/main`. This bundle's base is the integration branch (`stack-base`),
   whose `crates/validate` does not exist on `origin/main` yet (PRs #845/#849), so the patch
   cannot apply there. That is the gate's choice of base, not a stale patch. It looks like a
   harness gap (the diff-coverage gate should use the bundle's `stack-base` like Do does). Per
   project practice that goes upstream to eduralph/pdca-harness.
3. **C4-verify will again score `UNVERIFIABLE`.** That is the brief's declared posture for
   net-new API. The pre-fix red is a compile failure (37 errors). The demonstrated reds are the
   11 mutation runs.
4. Scratch: mutant files and logs are under `$PDCA_SCRATCH/pdca-builder-852-mut/`. I left the
   directory for the harness to reclaim instead of deleting it.
