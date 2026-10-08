# Build notes — #852 validate-s3-client-core

Withheld from the reviewer. For the human at sign-off.

## Base

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l2`, HEAD `0b48ab7` on
  `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (the bundle's `stack-base`). The
  brief named tip `022d76f`; the branch has since folded #842 (`0b48ab7`). Both carry #775.
- STOP checks from the brief, all passed: `git merge-base --is-ancestor 343be83 HEAD` → ok;
  `ls crates/validate` → `Cargo.toml src tests`; `grep -n validate Cargo.toml` → `:33`
  (`"crates/validate"`). #774's crate and #775's lint were not recreated.
- `patch.diff` is `git diff HEAD` plus the four new files, against `0b48ab7`.
  `git apply --check -R patch.diff` on the patched tree → clean.

## What changed (`path:line` on the patched tree; deletions cite base lines)

| File | Change |
|---|---|
| `Cargo.toml:74-94` | `[workspace.dependencies]` pins `aws-sdk-s3 = "1.144.0"` (`:91`, no default features; `rt-tokio`, `http-1x`) and `aws-smithy-http-client = "1.4.2"` (`:94`, `default-client`), with the ADR-0003 §2 adoption comment (`:74-90`). |
| `crates/validate/Cargo.toml:21-49` | Normal deps: the two AWS pins plus workspace `bytes`, `futures-util`, `http-body`, `tokio` (all already in the shipped graph). Dev-deps: `roxmltree`, `tempfile`, and the Wyrd gateway composition (`wyrd-server` by path at `:49`, since it is not in `[workspace.dependencies]`). Replaced the "Deliberately NO dependencies" note (base `:21-24`). |
| `crates/validate/src/lib.rs:5-31`, `:121` | `pub mod s3;` (`:22`), re-exports (`:29-31`), module docs, and the stderr note now says "no scenario drives the S3 client yet". |
| `crates/validate/src/s3.rs` (new) | The client: `Deadlines` (`:44`, defaults `:56`), `S3Client::with_deadlines` (`:89`), `put_object` (`:131`), `get_object` (`:165`), `delete_object` (`:188`), `classify` (`:200`), `request_id` (`:268`). |
| `crates/validate/src/s3/error.rs` (new) | `S3Error` (`:14`), `ErrorCode` (`:63`), `Phase` (`:74`), `BodyError` (`:86`). |
| `crates/validate/src/s3/body.rs` (new) | `PutSource` (`:32`), `DeclaredLengthBody` (`:76`, `Body` impl `:84`), `ObjectBody` (`:164`, `next_piece` `:192`). |
| `deny.toml` (base `:77-86`) | Deleted the RUSTSEC-2026-0253 ignore. |
| `deny-all-features.toml` (base `:104-111`) | Deleted its mirror. |
| `docs/design/architecture/05-building-block-view.md:253` | Replaced "the S3 client arrives with #741" with a description of the client (scope item g). |
| `Cargo.lock` | Gains only `wyrd-validate`'s dependency edges (15 lines, one hunk at `:5274`). |
| `crates/validate/tests/s3_client_roundtrip.rs` (new) | The test (11 `#[tokio::test]`s). |

## Design, and why

**Client config** (`s3.rs:89-123`) copies the cited peer `crates/server/tests/s3_gateway_cluster.rs:100-115`:
`behavior_version_latest`, region, `endpoint_url`, static `Credentials`, explicit
`aws_smithy_http_client::Builder::new().build_http()` (`s3.rs:111`), `force_path_style(true)`,
`RetryConfig::disabled()`, `StalledStreamProtectionConfig::disabled()`. Added: an explicit
`TimeoutConfig` that sets connect and operation and **disables** read and operation-attempt
timeouts (`s3.rs:93-98`), so no timeout comes from the SDK's behaviour-version defaults.

**Connect vs operation vs body-idle** (`s3.rs:207-215`, `body.rs:209`). The SDK surfaces both
its connect timeout and its read timeout as `ConnectorError::timeout` through a `pub(crate)`
`HttpTimeoutError` (`aws-smithy-http-client-1.4.2/src/client/timeout.rs:22-44`, `client.rs:629-664`).
They can't be told apart from outside. So the connector gets only the connect deadline, and a
`DispatchFailure` timeout is reported as `Phase::Connect`. The operation deadline is the SDK
orchestrator's (`SdkError::TimeoutError`). The body-idle deadline is mine:
`tokio::time::timeout` around each `ByteStream::next()`. That is needed because the SDK's
operation timeout ends when the GET's response head is deserialized, before any body is read.

**Clock** (rubric "one clock per correctness lifecycle"). All three deadlines run on tokio's
runtime clock: the SDK's default sleep is `TokioSleep` (rt-tokio), and body-idle is
`tokio::time::timeout`. That is stated at `s3.rs:39-43` and `body.rs:207-208`. The SDK's
SigV4 date stamp reads the wall clock. That stamp belongs to the server's freshness check,
not to a client deadline, and the doc says so. There is no `SystemTime::now` in my code.

**Typed error** (`error.rs`): one variant per place a call fails, matching scope (b):
`Service` (an S3 error response), `Unreadable` (a response that can't be read as S3),
`NoResponse`, `Timeout { phase, limit }`, `RequestNotBuilt`, `Body(BodyError)`.
`ErrorCode::{Code, MissingInXml, NoBody}` keeps the three `<Code>` cases apart.
- The request id is read from the `x-amz-request-id` header itself (`s3.rs:268-270`). It does
  not use the SDK's `RequestId` accessor, which prefers `x-amzn-requestid` (scope c).
- `NoBody` is decided from the raw body **before** the SDK's code (`s3.rs:234`). The S3 SDK
  fills in `NotFound` for a bodiless 404, and reporting that would break invariant (b), "every
  fact is the server's".
- A code-less non-empty error body goes to `Unreadable`, not `MissingInXml` (`s3.rs:246`). This
  slice can't tell a well-formed code-less `<Error>` from a non-XML page; the SDK reports both
  as code `None`. That split is #853's, with a `// deferred: #853` marker at `s3.rs:226`.
  `MissingInXml` exists in the type so #853 needs no new variant.

**PUT streaming** (`body.rs:32-143`). `PutSource { length, pieces }` is turned into
`DeclaredLengthBody`, an `http_body::Body`. Each piece is passed by value into `Frame::data`
(`body.rs:111`). Length is enforced on both sides:
- a piece crossing the declared length fails before any of it is sent;
- once the declared length is reached the source is polled once more, so excess in a
  separate piece is caught before the body reports its end (scope "including excess in a
  separate piece");
- the source ending early is `SourceLength`, and a source error is `SourceFailed`.

Failures go into an `Arc<OnceLock<BodyError>>` (`body.rs:58-70`, `:132`). `put_object` checks it
first (`s3.rs:150`), so the body error is reported as itself however the SDK and hyper wrapped
the aborted request. `size_hint` is exact (`body.rs:140-143`), so the SDK's aws-chunked layer
knows the decoded length.

**GET streaming** (`body.rs:164-240`). `ObjectBody` hands each SDK piece to the caller by
value. It counts bytes against the declared `Content-Length`:
- a piece past the length → `BodyError::Length`;
- a transport error → `BodyError::Transport`;
- an early end → `BodyError::Length`;
- after the first error every call returns it again.

A response with no `Content-Length` is refused as `LengthUndeclared` (`s3.rs:174-179`,
deferred #853 for close-delimited and chunked framing). Without a length, a cut connection
looks the same as the end of the object.

**Checksums left at the SDK default** (`WhenSupported`). For a streaming body the SDK wraps it
in `ChecksumBody` (CRC32 trailer) and `AwsChunkedBody`, which is signed over `http:`
(`aws-sdk-s3-1.148.0/src/aws_chunked.rs:120`, `:146-151`). The gateway validates the trailer, which
gives an end-to-end integrity check. Neither layer buffers more than one 64 KiB chunk.

## Criterion 3: `W` and `K`

Derivation table: test module docs, `s3_client_roundtrip.rs:29-54`. Computed at `:136-147`.

| Term | Bound | Source |
|---|---|---|
| SDK re-chunking | 64 KiB + one 16 KiB piece | `aws-runtime-1.10.0/src/content_encoding.rs:26` (`DEFAULT_CHUNK_SIZE_BYTE`), body loop `content_encoding/body/http_body_1_x.rs:55-80` |
| hyper write buffer | 417,792 + one 64 KiB signed chunk + ≤128 B framing | `hyper-1.10.1/src/proto/h1/io.rs:23`, `:575-582` (`can_buffer`) |
| client socket send queue | `tcp_wmem[2]` (4 MiB here, read from `/proc`, never set) + one 64 KiB loopback segment | the SDK's socket autotunes; the kernel caps at `tcp_wmem[2]` |
| relay socket receive queue | 2 × 16 KiB (`SO_RCVBUF`, doubled by the kernel) | relay's own socket, set before `listen` so accepted sockets inherit it |
| relay read buffer | 16 KiB | relay's own |

On this host: **W = 4,874,368 bytes (≈4.65 MiB)**. **Payload = 8 × W, rounded up to whole
pieces = 39,010,304 bytes (≈37.2 MiB).**

**K = 64 KiB / 16 KiB = 4.** The aws-chunked buffer polls its source only while it holds less
than one chunk (≤ 3 pieces), and the piece being pulled is the fourth. Where 4 pieces make one
chunk, `SegmentedBuf::copy_to_bytes` copies, so the pieces are released
(`bytes-utils-0.1.4/src/segmented.rs:377-396`). Every later stage holds the SDK's signed copy,
never a source piece.

Measured (4 runs with a temporary print, since removed):

| Run | max PUT lag | at (pull, produced, forwarded) | max live pieces |
|---|---|---|---|
| 1 | 2,686,346 | (592, 9,699,328, 7,012,982) | 4 |
| 2 | 2,620,810 | (1564, 25,624,576, 23,003,766) | 4 |
| 3 | 2,612,618 | (808, 13,238,272, 10,625,654) | 4 |
| 4 | 2,686,346 | (1808, 29,622,272, 26,935,926) | 4 |

The lag is mostly the client socket's autotuned send queue (~2.5 MiB, which matches Linux's
`2 × init_cwnd × per_mss` at the loopback MSS). The live-piece count hits the derived K exactly.
The relay forwarded 39,064,150 body bytes for a 39,010,304-byte payload; the 53,846-byte
difference is aws-chunked framing (90 B per 64 KiB chunk).

**One honest limit of the PUT lag oracle.** The relay counts wire bytes after the request head.
That count includes aws-chunked framing (0.14%), so a measured lag ≤ W proves a payload lag
≤ W + 0.14% of the bytes forwarded (≈53 KiB at this payload). The brief's wording is "bytes the
relay has forwarded", which is what is counted. Excluding the head makes the check slightly
stricter than the literal reading. This is stated in the test docs (`:42-46`).

**GET.** The relay paces: it writes at most W response-body bytes past what the test has taken
(`s3_client_roundtrip.rs:604-624`). Pacing is credit-based: `Notify::notify_one` stores a permit,
so no wakeup is lost. Every wait in the test is bounded by `STALL = 30 s`, and a stall panics
with a message naming the offset, W, and the relay's written count.

## Criterion 3: the five mutations (each applied, run, recorded, reverted)

Applied to the production files from clean copies; restore checked with `cmp`. Each was run
against its streaming test:
`cargo test -p wyrd-validate --test s3_client_roundtrip <test>` under `timeout 900`.

| # | Mutation (where) | Test | Result |
|---|---|---|---|
| i | Buffer the whole PUT: drain the source into a `BytesMut`, send one frame (`body.rs` `poll_frame`) | `put_streams_within_the_window_at_every_pull` | **RED**: "PUT lag: at pull 2381 the source had produced 39010304 bytes but the relay had forwarded 0; 39010304 bytes held between them exceeds W = 4874368" |
| ii | Forward the first 2 MiB, then collect the rest into a `BytesMut` | same | **RED**: "PUT lag: at pull 2381 … produced 39010304 … forwarded 1837528; 37172776 bytes held … exceeds W". Retention passes here (pieces are copied, then dropped), so only the lag oracle catches it, as designed. |
| iii | Keep every forwarded piece: `kept.push(piece.clone())` | same | **RED**: "PUT retention: 2381 of the source's pieces were alive at one pull; K = 4". Lag passes here, so only the retention oracle catches it. |
| iv | Collect the whole GET (`output.body.collect().await.into_bytes()`), hand it over re-chunked into 16 KiB pieces (`s3.rs` `get_object`) | `get_hands_over_pieces_within_the_window` | **RED**: "GET lag: the client handed over nothing for 30s at offset 0 while the relay had written 4874368 body bytes, W = 4874368 past what the test had taken …". Chunk count or largest piece would not have caught this; pacing does. |
| v | Hand over pieces until 2 MiB, then collect the rest (`body.rs` `read`) | same | **RED**: "GET lag: … at offset 2097152 while the relay had written 6971520 body bytes, W = 4874368 …" (2 MiB taken + W). |

## Criterion 3, reviewed not tested: no accumulating buffer in either body path

`grep -nE "collect|aggregate|into_bytes|Vec<|Vec::|BytesMut|extend|push|SegmentedBuf|copy_from_slice|to_vec"`
over `crates/validate/src/s3.rs` and `crates/validate/src/s3/*.rs` matches only:
- `body.rs:7`, a doc comment;
- `body.rs:151-152`, `error_chain`, which joins error-message strings, not body bytes.

- **PUT path.** `DeclaredLengthBody`'s fields are `pieces, declared, produced, ended, failure`
  (`body.rs:76-82`). There is no byte container. The `OnceLock` holds at most one `BodyError`.
  `poll_frame` passes each piece on by value (`body.rs:111`) and keeps no reference.
  `put_object` (`s3.rs:131-161`) hands the body to the SDK and never touches bytes.
- **GET path.** `ObjectBody`'s fields are `stream, declared, received, idle, ended, failed`
  (`body.rs:164-171`). Only the length is counted (`body.rs:217-224`) and the piece is returned
  by value (`body.rs:225`). `get_object` (`s3.rs:165-185`) moves `output.body` into `ObjectBody`
  unread.
- Below the client, the SDK's only buffering is the bounded 64 KiB aws-chunked buffer (PUT);
  the GET response body path has none (stalled-stream protection is off, so no wrapper).

## Refuting my own test

- **(a) Genuine red? Yes.** I moved `src/s3.rs` and `src/s3/` out, reset `Cargo.toml`,
  `Cargo.lock`, `crates/validate/Cargo.toml`, `src/lib.rs`, both deny files and the doc to
  `HEAD`, and kept the test. `cargo test -p wyrd-validate --test s3_client_roundtrip` then
  fails to compile with 30 errors, among them "unresolved imports `wyrd_validate::BodyError`, …
  `S3Client`, `S3Error`" (E0432). That is the brief's predicted criterion-absence red, which
  C4-verify scores UNVERIFIABLE. The demonstrated reds are the five mutations above, each
  firing the oracle it targets. After restoring, the file is green (11/11, run 7 times).
- **(b) Production path? Yes.** Every test drives `wyrd_validate::S3Client`, the production
  type, built through the production `resolve_config` (argument parser plus credential
  resolution, `s3_client_roundtrip.rs:311-337`). It runs the real `aws-sdk-s3` over real
  loopback TCP into the real `S3Gateway` (`Gateway<RedbMetadataStore, FsChunkStore,
  MemCoordination>`, 256 KiB chunks, `:279-305`), copied from the cited
  `s3_http_wire.rs:56-92` with `serve_s3_role` not used. There are no mocks. The relay only
  forwards bytes; it never makes up a response.
- **(c) Fixture includes the fault? Yes.**
  - The cut is a real close after 262,157 body bytes (`CutAfter`).
  - The held tail is a real stalled connection (`HoldAfter`).
  - The full accept queue is a real `listen(1)` with queued connections (`:1025-1038`); SYNs
    are dropped by the kernel.
  - The silent peer really accepts and never answers.
  - The bad source pieces are real.
  - The 403 comes from a real wrong-secret signature.
  - The error-field oracle compares with bytes the relay captured off the wire, parsed with
    `roxmltree` (chunked framing decoded, because the gateway sends error bodies chunked).

## The dependency move: ADR-0003 §2 audit

Decision record: per the brief's table (#741 `notes.json`, `iteration-v3/brief.md:10-24`,
`iteration-v3/SUMMARY.md:293-297`). Not re-opened here.

**Floor.** `aws-sdk-s3 = "1.144.0"`. From the local registry index cache
(`~/.cargo/registry/index/.../.cache/aw/s-/aws-sdk-s3`):

| Version | `lru` requirement |
|---|---|
| 1.141.0 | `^0.16.3` |
| 1.142.0 | `^0.16.3` |
| 1.143.0 | `^0.16.3` |
| 1.144.0 | `^0.18.2` |
| 1.145.0 | `^0.18.2` |
| 1.148.0 | `^0.18.2` |

So 1.144.0 is the first release whose `lru` is past the fix. The lock resolves `aws-sdk-s3
1.148.0`, `lru 0.18.4`, `aws-smithy-http-client 1.4.2`. `crates/server/Cargo.toml:131-135`
still declares its own dev-dependency versions ("1.137.0", "1.1.13", …). Those are not edited
(out of scope), and they resolve to the same locked versions.

**Waiver deletion.**
- Before: on the base `deny.toml`, `cargo deny --config <base deny.toml> check advisories`
  prints `warning[advisory-not-detected]: advisory was not encountered` at
  `"RUSTSEC-2026-0253"`. The entry's own removal trigger had already fired.
- After: all three gate invocations pass: `cargo deny check` → "advisories ok, bans ok,
  licenses ok, sources ok"; `cargo deny --all-features --config deny-all-features.toml check
  advisories` → "advisories ok"; `cargo deny --all-features check licenses bans sources` → ok.
- `cargo tree -i lru -e normal` → `lru 0.18.4 ← aws-sdk-s3 1.148.0 ← wyrd-validate`. The
  advisory is not in the graph; no exposure was accepted. **Tracker note for #741 (sign-off
  should mirror it):** "the RUSTSEC-2026-0253 waiver is deleted because the advisory is not in
  the graph: the `aws-sdk-s3 >= 1.144.0` floor requires `lru ^0.18.2`, and the lock resolves
  `lru 0.18.4`."

**Transitive surface.** `cargo tree -p wyrd-validate -e normal` lists 141 packages. New to the
shipped normal graph (that set minus
`cargo tree --workspace --exclude wyrd-validate -e normal`, C-locale sorted): **81
third-party crates.**

- **Licences** (cargo metadata; all on the `deny.toml` allowlist, `cargo deny ... licenses ok`):
  - 27 `MIT OR Apache-2.0`
  - 18 `Apache-2.0`: all the `aws-*` crates
  - 18 `Unicode-3.0`: ICU4X, `litemap`, `tinystr`, `writeable`, `yoke*`, `zerofrom*`,
    `zerotrie`, `zerovec*`
  - 8 `MIT`: `base64-simd`, `generic-array`, `http-body 0.4`, `lru`, `outref`, `spin`,
    `synstructure`, `vsimd`
  - 5 `Apache-2.0 OR MIT`
  - 1 each: `Apache-2.0/MIT` (`bytes-utils`), `Zlib` (`foldhash`),
    `Apache-2.0 OR ISC OR MIT` (`rustls-native-certs`), `Apache-2.0 OR BSL-1.0` (`ryu`; BSL-1.0
    is the Boost licence, not the Business Source licence deny.toml bans),
    `MIT/Apache-2.0` (`xmlparser`)
- **Named new crates:**
  - the 18 `aws-*` crates;
  - `http 0.2` and `http-body 0.4` (pulled by aws-sdk-s3's `aws-runtime/http-02x` feature);
  - the checksum stack: `crc-fast`, `crc32fast`, `md-5`, `sha1`, `digest 0.10`;
  - `lru`, `bytes-utils`;
  - `url`/`idna`/ICU4X;
  - `time`, `uuid`, `regex-lite`, `xmlparser`, `base64-simd`;
  - through `default-client`: **`rustls-native-certs`, `rustls-pki-types` and (Unix)
    `openssl-probe`, but not `rustls`** (`grep -E "^rustls |^ring |^aws-lc"` over validate's
    normal tree → none). That fixes round 1's mistake.
- **Unsafe posture:**
  - `aws-sdk-s3` is `#![forbid(unsafe_code)]`.
  - Unsafe sites in the smithy crates' `src` (`grep -rE "unsafe (fn|impl|\{)"`):
    - `aws-smithy-json` 2: `from_utf8_unchecked` after its own validation;
    - `aws-smithy-types` 1: same;
    - `aws-smithy-eventstream` 2: an `unsafe impl BufMut`, required by the `bytes` trait;
    - `aws-smithy-schema` 4: pointer reborrow and lifetime transmute in its HTTP binding
      serializer;
    - `aws-smithy-http-client` 1, in `test_util` only (feature off).
  - The others have none.
  - The heavier unsafe is outside the smithy layer: `crc-fast` (361 sites, SIMD CRC; no build
    script), `lru` (94, its linked list; the RUSTSEC-2026-0253 crate, now at the fixed 0.18.4),
    `bytes-utils` (12), `openssl-probe` (5, environment setup).
- **Maintenance.** The AWS SDK for Rust is AWS's official SDK, generated by smithy-rs and
  versioned in lockstep. The workspace already tracked it as a dev-dependency. I didn't check
  release dates from the network.

## Production reach: the two limits the brief asks to record

1. **Plain HTTP.** The client is built on `build_http()` (no TLS provider). Deployment puts TLS
   in front of the gateway (proposal 0017 §10; `crates/gateway-s3/src/lib.rs:50-57`). An
   `https://` endpoint fails as `NoResponse`, because hyper's HTTP connector refuses the scheme.
   TLS is out of scope.
2. **Composition.** The proof runs against redb + `MemCoordination` + local `FsChunkStore`
   (the `s3_http_wire.rs` composition), not production's TiKV/FDB + etcd + remote D-servers. The
   S3 wire surface and its streaming are the same code (`S3Gateway`), but the backend
   performance and failure shapes behind it are not.

## Deferred markers (answer review findings on them with "Deferred — tracked in #853/#854")

- `s3.rs:153` `// deferred: #854`: a peer that acknowledges a PUT before reading the whole
  body, or stops reading.
- `s3.rs:174` `// deferred: #853`: close-delimited and chunked GET framing (refused as
  `LengthUndeclared` meanwhile).
- `s3.rs:226` `// deferred: #853`: non-conforming error responses: no error-body byte budget,
  empty-body errors (classified `NoBody` but exercised only by #853), XML without a code vs
  non-XML.

## Gates run (all in the worktree)

- `./engine/xtask.sh ci` (the C4-ci gate, `cargo xtask ci`) → **exit 0**. It ran: typos,
  `lint_docs.py`, `render_site.py --check`, gitlink/unsafe/blackbox guards, `cargo fmt --check`,
  clippy (workspace, all targets), build, `cargo test --workspace` (s3_client_roundtrip:
  11 passed, 5.07 s), cargo-machete, all three cargo-deny invocations, statics, deploy-guard,
  and the madsim DST clippy and test.
- `cargo xtask blackbox-guard` → "wyrd-validate's normal dependency closure holds no wyrd-*
  crate". `cargo xtask statics` → clean. `cargo fmt --all` was applied; the commit hook's
  formatter has nothing to change.
- Test runtime: the whole file takes about 5 s in a debug build, including two 37 MiB transfers
  each way.

## Rubric self-review

- **One clock per lifecycle:** see "Clock" above. Every new clock read states its source.
- **Narrow seams, dependency direction:** no `wyrd-*` normal dependency (guard green).
  `wyrd-server` is a dev-dependency only (proposal 0017 §9).
- **No DST-reachable globals:** no `static` anywhere (statics gate green). The test uses `Arc`
  counters.
- **forbid(unsafe_code):** the crate root `lib.rs:1` already had it. The test file carries it.
  No new crate.
- **Docs currency:** `05-building-block-view.md:253` updated in the same patch (new library API).
- **Protocol input:** a GET body is trusted only up to its declared `Content-Length`; cut or
  over-length → error. Without a length → refused. The error-body budget is deferred to #853
  with a marker.
- **Serialization identity / absent values:** `PutOutcome.etag` is `Option` and never made up.
  `NoBody` beats the SDK's invented `NotFound`. `message` is `Option`.
- **Absent or unsupported entries:** every unsupported case is an explicit error
  (`LengthUndeclared`, `RequestNotBuilt` for a length that doesn't fit `i64`, the
  non-exhaustive `SdkError` arm at `s3.rs:253-262`).
- **Await discipline:** every await on the endpoint is bounded (connect, operation, body-idle).
  Production spawns no task. Every test fixture task is in a `JoinSet` (aborted on drop); relay
  connection tasks are in a `JoinSet` owned by the accept task (scope h). Every test await on
  the client is wrapped in `timeout`.
- **Test fidelity / DST:** `wyrd-validate` is a client binary outside the madsim-modelled
  server. Its destructive call (`delete_object`) issues an S3 DELETE against someone else's
  deployment, not a Wyrd-internal destructive path, so no Tier-0 DST seed is added. The real-TCP
  fixtures are the coverage.
- **Out of scope honoured:** no edit under `crates/server/`, no gateway change, no TLS, no
  `smoke` or capability matrix (the binary still echoes and exits), no 5 GiB property.

## What I tried or ruled out (with costs)

- **Turning checksums down to `WhenRequired`, to get raw bytes on the wire.** That would make
  the relay's byte count exactly the payload, with no 0.14% framing. But it drops the CRC32
  trailer the gateway validates. It would also make long-source detection depend on hyper
  polling the body again after `Content-Length` is reached, which aws-chunked guarantees today
  (it polls the inner body to `None` before writing the trailer). Kept the SDK default.
- **Subtracting a framing allowance in the relay count, to make the PUT oracle exact.** A sound
  lower bound has to assume the SDK's 8 KiB minimum chunk (`aws_chunked.rs:35`), which
  over-subtracts about 1.5% (≈600 KiB at 37 MiB) and eats into W. Assuming the actual 64 KiB is
  only sound while the client leaves the chunk size alone. Parsing aws-chunked in the relay
  would cost about 40 lines of a framing parser in the fixture. I chose to count wire bytes, as
  the brief words it, and state the 0.14%.
- **Using the SDK's `read_timeout` for body-idle.** It wraps only the response-head future, and
  it shares the connector's timeout error with connect, so the phase would become ambiguous.
  Disabled.
- **`Mutex`-wrapping the caller's source to drop the `Sync` bound.** `ByteStream::from_body_1_x`
  needs `Send + Sync`
  (`aws-smithy-types-1.8.1/src/byte_stream/http_body_1_x.rs:16-19`). A never-locked `Mutex`
  would let `!Sync` sources in, at the cost of a lock type in the body path that a reviewer
  would have to reason about. The generated sources later slices use are `Sync`. I required
  `Sync` instead.
- **Making `S3Client::new` fallible, to reject `https://`.** TLS is out of scope, and the
  failure already surfaces as a typed `NoResponse` naming the scheme. Not added.
- **`PutObject` with no `content_length`.** The SDK would take the length from the body's exact
  `size_hint`. I set it explicitly (`s3.rs:144`) so the declared length visibly is the S3
  `Content-Length`. The `i64` conversion guard (`s3.rs:132-137`) is the only cost.

## NEEDS-HUMAN

None from external dependencies: the build used the base toolchain only, with no container, no
MinIO and no network. The one human item is the brief's own: at sign-off, mirror the waiver
sentence above onto #741.
