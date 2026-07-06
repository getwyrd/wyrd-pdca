# Serve S3 PUT/GET/DELETE over an HTTP endpoint

> One logical fix per PR.

## Summary
**User impact:** There was no way to talk to the gateway over the network. Storing or
fetching an object meant calling the gateway's Rust methods in-process; a deployed
gateway had no client-facing endpoint at all, so no S3 client, tool, or SDK could PUT
or GET an object against it. The day-one goal of "PUT an object and GET it back
byte-identical" could not be exercised over the wire.

This PR gives the gateway a real client-facing HTTP endpoint: bucket-scoped object
**PUT / GET / DELETE** over S3-compatible HTTP, with mandatory AWS SigV4 request
signing and streamed bodies, mapping onto the existing in-process write/read paths.

## What to look at
- **`crates/gateway-s3/`** — the new S3 wire crate: request routing (`src/lib.rs`,
  `handle()`), SigV4 verification (`src/sigv4.rs`), and streaming/`aws-chunked`
  decoding (`src/streaming.rs`). This is where the auth boundary and the wire framing
  live.
- **`crates/gateway-core/`** — a small neutral `ObjectGateway` seam the wire crate is
  generic over, so the S3 layer never calcifies inside the server's composition root
  and other gateways can implement the same seam.
- **`crates/server/src/lib.rs`** — the composition root: `cmd_s3` picks the concrete
  backends and binds the listener; `put_object`/`get_object`/`delete_object` are the
  in-process methods the wire verbs map onto; `recover()` rebuilds the id allocators at
  startup.
- **How to exercise it:** `cargo test -p wyrd-server --test s3_http_wire` drives a
  signed PUT → GET → DELETE round-trip over a loopback listener, an unsigned/forged
  request being refused, a real `aws-sdk-s3` client interop, and restart recovery. To
  drive it by hand: run the gateway with `--s3-listen 127.0.0.1:8080` and point
  `aws --endpoint-url http://127.0.0.1:8080 s3api put-object/get-object/delete-object`
  at it with matching static credentials.

## Root cause
PUT and GET existed only as in-process methods on the gateway type
(`crates/server/src/lib.rs:114,146`), and the crate itself recorded that "the HTTP/S3
wire surface is a later milestone" (`crates/server/src/lib.rs:7-9`). Nothing bound a
network listener, so a deployment had no way for a client to reach an object over the
network, and DELETE did not exist at all.

## Fix
Add an S3-compatible HTTP listener in the gateway role, in a dedicated `gateway-s3`
crate generic over a neutral `ObjectGateway` seam (`gateway-core`):

- **Bucket-scoped PUT / GET / DELETE** mapping onto the existing client write/read
  paths; DELETE is net-new.
- **Fail-closed SigV4 auth** — every request's signature is verified *before* its body
  is read; an unsigned or wrong-signature request is refused with no anonymous access.
- **Streaming bodies** — request and response bodies stream chunk-by-chunk; a large
  object is never buffered whole in the gateway's heap.
- **Durable, reader-safe deletes** — DELETE and overwrite leave superseded fragments
  under a durable orphan grace record so the custodian reclaims them only after its
  reader-safe window, leaving a concurrent streaming read intact and never stranding
  bytes on a crash. Startup recovery resumes the id allocators above every id still
  live on disk — including ids the orphan ledger still holds — so a restart cannot
  re-mint a chunk id whose bytes are still present.

At review the listener runs over plaintext loopback; serving public S3 over TLS on a
deployed host depends on the first-deployment work and is out of scope here.

## Verification
- **Claim:** a signed, bucket-scoped `PUT → GET → DELETE` round-trips byte-identical
  over a real HTTP listener, and an unsigned or wrong-signature request is refused.
  - **Checked:** `crates/gateway-s3/src/lib.rs` — `handle()` verifies the SigV4
    signature before materialising the request body; `crates/gateway-s3/src/sigv4.rs`
    — canonicalisation + signature comparison pinned to AWS known-answer vectors.
  - **Test:** `crates/server/tests/s3_http_wire.rs` — signed round-trip returns the PUT
    bytes byte-identical; `unsigned_put_is_refused_before_its_body_is_read`; a stock
    `aws-sdk-s3` client (its own signer and framer, not the gateway's) round-trips and a
    forged credential gets `InvalidAccessKeyId`. Fails pre-fix (no listener to dial),
    passes post-fix.
- **Claim:** the in-process seam is now reachable over the network, retiring the
  "later milestone" marker.
  - **Checked:** `crates/server/src/lib.rs:7-9` (marker retired), `:114`/`:146` — the
    `put_object`/`get_object` methods the wire verbs now front; `cmd_s3` binds the
    listener at the composition root.
- **Claim:** DELETE/overwrite neither leak fragments nor tear a concurrent read, and a
  restart never re-mints a still-live chunk id.
  - **Checked:** `crates/core/src/metadata.rs` — `unlink` writes an orphan grace record
    in the same atomic commit that unbinds the object, and `high_water_marks` scans the
    orphan ledger so `recover()` resumes above ids it still holds.
  - **Test:** `crates/server/tests/s3_http_wire.rs` —
    `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss` fails pre-fix
    (a restart re-mints the deleted object's chunk id and the ledger reclaim then
    destroys the new object) and passes post-fix.
- **Gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, test, `cargo deny`,
  conformance) passes on the change.

Fixes #364
