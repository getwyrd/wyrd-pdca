## Summary
**User impact:** `wyrd-validate` is meant to check a running Wyrd deployment
through its S3 endpoint, but today it cannot talk to S3 at all: it reads its
flags and credentials, prints them and exits. Nothing it is supposed to
validate can be checked yet.

This PR adds the S3 client the validator's later checks all go through. It
uploads and downloads objects without ever holding a whole object in memory,
gives up after bounded waits, and reports every failure as a structured error
(status, S3 error code, message and request id) rather than a string.

**Depends on #845 and #849**, which add the `wyrd-validate` crate and its
no-Wyrd-crates lint. This branch was built on top of both. Merge them first.

## What to look at
- The client and its error type, in `crates/validate/src/s3.rs` and
  `crates/validate/src/s3/`. The error has one variant per way a call can fail,
  so the later validator checks can match on it instead of parsing messages.
- How the request and response bodies are streamed, in
  `crates/validate/src/s3/body.rs`. This is where the "never hold the whole
  object" promise lives.
- The dependency change: `aws-sdk-s3` becomes a shipped dependency (it was
  dev-only), and the RUSTSEC-2026-0253 waiver is deleted.

To try it: `cargo test -p wyrd-validate --test s3_client_roundtrip`. It runs the
real client against a Wyrd S3 gateway started inside the test process on
loopback. No Docker, no network. The whole file takes about 5 s in a debug
build.

## Root cause
`crates/validate` had no S3 client. Its manifest deliberately had no
dependencies and left the client and its dependency audit to a later change.
This is that change.

## Fix
- **Client** (`s3.rs:75-180`): `aws-sdk-s3` on the configured `--endpoint`,
  path-style, static credentials, `build_http()` connector, retries and
  stalled-stream protection disabled. This copies the existing test setup in
  `crates/server/tests/s3_gateway_cluster.rs:100-115`. An explicit
  `TimeoutConfig` sets the connect and operation deadlines and turns off the
  SDK's other timeouts, so no deadline comes from SDK defaults.
- **Deadlines**: connect (10 s), operation (15 min) and body-idle (60 s, per
  piece), each reported as `S3Error::Timeout { phase, limit }`.
- **Typed error** (`s3/error.rs:14-104`): `Service`, `Unreadable`,
  `NoResponse`, `Timeout`, `RequestNotBuilt` and `Body(BodyError)`.
  `ErrorCode` keeps "this code", "XML without a code" and "no body" apart.
- **Request id** (`s3.rs:262`): read from the `x-amz-request-id` header
  directly. The SDK's generic `RequestId` accessor prefers `x-amzn-requestid`
  when both are present.
- **PUT body** (`s3/body.rs:84-146`): passes each caller piece on by value,
  keeps no byte buffer, and fails the upload if the source ends short, runs
  long or errors. An empty piece ends the poll with a self-wake
  (`s3/body.rs:107-117`). Without this, a source that keeps yielding empty
  pieces spins inside one poll: on a current-thread runtime the operation
  deadline never fires, and on a multi-thread runtime the connection is never
  closed.
- **GET body** (`s3/body.rs:166-249`): hands back pieces as they arrive and
  counts only their length. A connection cut mid-body is a `Body` error, never
  a shorter object.
- **Dependencies**: `aws-sdk-s3 = "1.144.0"` floor in `[workspace.dependencies]`.
  1.144.0 is the first release that requires `lru ^0.18.2`, and the lock
  resolves `aws-sdk-s3 1.148.0` and `lru 0.18.4`. `Cargo.lock` gains only
  `wyrd-validate`'s edges.
- **Waiver removed**: RUSTSEC-2026-0253 from `deny.toml:77-86` and
  `deny-all-features.toml:104-111` on `main`. The entry's own removal trigger
  has fired: `cargo tree -i lru -e normal` shows only
  `lru 0.18.4 ← aws-sdk-s3 1.148.0 ← wyrd-validate`. The advisory is not in
  the graph. This does not accept an exposure.
- **Docs**: `docs/design/architecture/05-building-block-view.md:253` no longer
  says the client is still to come.

### Dependency audit (ADR-0003 §2)
`cargo tree -p wyrd-validate -e normal` lists 141 packages, 81 of them
third-party crates new to the shipped graph.
- **Licences**: all are on the `deny.toml` allowlist (MIT / Apache-2.0
  variants, Unicode-3.0 for ICU4X, Zlib for `foldhash`, BSL-1.0, the Boost
  licence, for `ryu`).
- **Notable new crates**: the 18 `aws-*` crates, `http 0.2` / `http-body 0.4`,
  the checksum stack (`crc-fast`, `crc32fast`, `md-5`, `sha1`), `lru`,
  `url`/`idna`/ICU4X, and through `default-client`, `rustls-native-certs`,
  `rustls-pki-types` and (Unix) `openssl-probe`. `rustls`, `ring` and `aws-lc`
  are **not** in the tree.
- **Unsafe**: `aws-sdk-s3` is `#![forbid(unsafe_code)]`. The smithy crates
  have a handful of small sites. The heavier unsafe code is in `crc-fast`
  (SIMD CRC), `lru` (its linked list, at the fixed 0.18.4), `bytes-utils` and
  `openssl-probe`.
- **Maintenance**: AWS's official SDK, already tracked here as a
  dev-dependency.

### Known limits (follow-ups, not in this PR)
- Plain HTTP only. Deployment puts TLS in front of the gateway, and an
  `https://` endpoint currently fails as `NoResponse`.
- A malformed `--endpoint` (no scheme, `ftp://`, `https://`) is also reported
  as `NoResponse`, the same as a refused connection, although nothing reached
  the wire (`s3.rs:249-255`). It should become `RequestNotBuilt`, or be
  rejected when the flags are parsed, before the scenario layer counts it as an
  availability failure.
- GET has no whole-transfer deadline: body-idle resets on every piece, so a
  peer that trickles bytes can keep a GET open indefinitely. The scenario layer
  should own an overall deadline, and the comment at `s3.rs:51-52` should stop
  suggesting the body is bounded.
- Non-conforming servers (odd error bodies, chunked or close-delimited GETs)
  and upload peers that answer early or stop reading are out of scope. Those
  sites carry `// deferred: #853` and `// deferred: #854` markers.
- The proof runs against redb + in-memory coordination + local-filesystem
  chunks, not a production backend. The S3 wire code is the same.

## Verification
Paths below are on this PR's branch (`main` plus #845/#849 plus this change)
unless marked `main`.

- **Claim: PUT → GET → DELETE round-trips byte-identical, an empty object
  too, and a GET after DELETE is a typed 404 `NoSuchKey`.**
  Test: `put_get_delete_round_trips_byte_identical`,
  `empty_object_round_trips` (`tests/s3_client_roundtrip.rs:888`, `:906`).
- **Claim: an error response's status, code, message and request id are
  exactly what the gateway sent.**
  Test: `error_response_fields_equal_what_the_gateway_sent` (`:956`). The relay
  captures the gateway's `x-amz-request-id` and also adds a decoy
  `x-amzn-requestid`; the typed id must equal the captured one. Swapping in the
  SDK's generic accessor makes this test fail with the decoy id.
- **Claim: the PUT never runs more than a window `W` ahead of what reached the
  wire, and never keeps more than `K` of the caller's pieces alive.**
  Test: `put_streams_within_the_window_at_every_pull` (`:969`). This is checked
  at every pull for a payload of 8 × `W` (about 37 MiB). The relay decodes
  aws-chunked framing and counts only payload bytes, and the test checks that
  count equals the object length exactly. The derivation of `W` and `K` is at
  `:34-72` and `:147-158`.
- **Claim: the GET hands pieces over as they arrive, never more than `W`
  behind the wire.**
  Test: `get_hands_over_pieces_within_the_window` (`:1028`). The relay paces
  the response, so a client that waits for more than `W` stalls and the
  bounded wait reports it.
- **Claim: neither body path keeps a copy of bytes already passed on.**
  This is checked by review, not by a test (a process-wide allocation counter
  can't separate client from in-process gateway). `DeclaredLengthBody` and
  `ObjectBody` have no byte container (`s3/body.rs:84-90`, `:166-173`). Each
  piece is returned by value (`:122`, `:232`), and `put_object` / `get_object`
  never touch the bytes (`s3.rs:126-180`).
- **Claim: bad PUT sources and cut GETs fail with the body error and store or
  return nothing partial.**
  Test: `put_source_that_ends_short_runs_long_or_fails_stores_nothing`
  (`:1108`), `get_cut_mid_body_is_the_body_error_never_a_shorter_object`
  (`:1171`).
- **Claim: every deadline fires as a typed timeout naming its phase, and a
  refused connection is `NoResponse`, not a timeout.**
  Test: `connect_deadline_expires_as_a_connect_timeout` (`:1223`, a full accept
  queue), `operation_deadline_expires_as_an_operation_timeout` (`:1245`, a peer
  that never answers), `body_idle_deadline_expires_as_a_body_idle_timeout`
  (`:1276`, a held GET tail), `a_refused_connection_is_no_response_not_a_timeout`
  (`:1311`).
- **Claim: a source of endless empty pieces cannot block the deadline.**
  Test: `an_always_ready_empty_source_yields_to_the_deadline_on_a_current_thread_runtime`
  and `…_multi_thread_runtime` (`:1449`, `:1454`). With the old skip-in-a-loop
  code, the first hangs past its 30 s guard and the second leaves the
  connection open after the PUT gives up. Both pass with the fix.
- **Claim: an empty key is reported as a request never built.**
  Test: `empty_key_is_a_request_never_built` (`:1463`).
- **Fails before, passes after.** This is a new API, so before the change the
  test file does not compile (37 errors, e.g. unresolved
  `wyrd_validate::S3Client`). The streaming checks were shown to bite by
  breaking the client on purpose, once per way: buffer the whole PUT; forward
  2 MiB, then collect the rest; keep every forwarded PUT piece; collect the
  whole GET; hand over 2 MiB, then collect the rest. Each of these failed its
  test with a message naming the bound it broke. All 14 tests pass on the final
  tree.
- **Gates.** `cargo xtask ci` passes on the final tree, including all three
  `cargo-deny` runs with the waiver gone. `cargo mutants --in-diff` leaves 1
  of 65 mutants alive: a GET length guard that only a non-conforming response
  can reach (marked `// deferred: #853`).

Fixes #852
