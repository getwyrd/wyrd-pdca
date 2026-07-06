# Build notes — issue 364 / s3-http-wire-surface (iteration 2)

_Withheld from the reviewer. Rationale, alternatives, and the pre-declared NEEDS-HUMAN
calls for the human at sign-off._

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (M4 integration base).
Built in `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`; all `path:line` citations are
against that worktree at tip `5d87cc4` (= `origin/feat/m4-production-metadata-backend`).

This is a **rebuild** addressing the 6 carry-forward items. Everything the previous
sign-off said to keep (signed PUT→GET→DELETE byte-identical; unsigned/bad-sig → 403;
fail-closed auth boundary; crypto pinned to published vectors) is kept and hardened.

## Carry-forward disposition (all six)

### 1. "Stream, don't buffer" — INVERTED before; now genuinely streams (named invariant)
The write path no longer materialises the object. New `wyrd_core::write::stream_write_data`
(`crates/core/src/write.rs:406`) re-chunks the incoming body buffers into `chunk_size`
pieces and leases + writes each chunk **as it arrives** (`intent_and_write_chunk`,
`write.rs:349`), keeping the fragment bytes only for one chunk at a time (it returns a
`WritePlan` whose `PlannedChunk.fragments` are emptied — `write.rs:381-388` — so only the
chunk *map* survives to the commit). `Gateway::put_object_streaming`
(`crates/server/src/lib.rs:144`) drives it and the handler feeds it the axum body data
stream directly (`crates/server/src/s3/mod.rs:196`). GET streams too:
`Gateway::get_object_streaming` (`lib.rs:255`) reads one chunk at a time
(`read::read_chunk_verified`, `crates/core/src/read.rs:345`) over a **bounded** channel
(depth 4 → backpressure), so peak resident bytes are O(chunk_size), not O(object). The
GET response is `Transfer-Encoding: chunked` (`s3/mod.rs:215`, `Body::from_stream`).
The in-process `Gateway::put_object`/`get_object` (`lib.rs:123,205`) stay for existing
callers — the OOM cliff (0015:789) is the *network* path, which now streams.

### 2. Real SigV4 canonicalization + independent oracle (was self-consistent only)
`sigv4::canonical_query` (`crates/server/src/s3/sigv4.rs:157`) now percent-decodes then
re-URI-encodes each parameter and **sorts** them (the step a real `aws-sdk`/`boto3`
request relies on); `uri_encode` (`sigv4.rs:103`) is RFC-3986. The independent oracle is
`sigv4_aws_docs_example_sorts_query` (`sigv4.rs` tests): it feeds the query
**out of order** and asserts canonicalization sorts it to
`Action=ListUsers&Version=2010-05-08`, then that the whole chain reproduces AWS's
**published** signature `5d672d79…` (AWS docs "Create a signed request" worked example) —
so a green is anchored to AWS, not to our own `sign`. `sigv4_get_vanilla_known_answer`
(published `5fa00fa3…`), `uri_encode_follows_rfc3986`, and `canonical_query_encodes_and_sorts`
back it up. A live botocore/aws-sdk interop test would need a dev-dependency (a §4 human
dependency decision, see item 4); the AWS-published KATs are the honest independent oracle
used instead — noted as a NEEDS-HUMAN nicety, not a gap.

For the **path**: S3 uses single URI-encoding, so the canonical URI is the request-target
path exactly as received (hyper does not decode `Uri::path()`), which is what a compliant
SDK signs. `verify` uses the received path as-is and `sign` puts the same encoded path on
the wire; `uri_encode` is exposed + unit-tested for the future presigned/odd-key surface.

### 3. Concurrent DELETE is now idempotent (was 409 on the losing racer)
`Gateway::delete_object` (`lib.rs:219`) retries: on a CAS `Conflict` it re-resolves and,
if the key is now absent (a racing DELETE won), returns success (S3's 204) rather than 409;
a racing *overwrite* is retried against the new record (bounded to 8 attempts). Backed by
`metadata::unlink` (`crates/core/src/metadata.rs:308`). The test
`concurrent_delete_is_idempotent` runs two DELETEs as **separately-spawned tasks on a
multi-thread runtime** (an in-process `join!` never reaches the CAS-conflict branch because
in-memory redb resolves synchronously), 64 rounds, asserting both succeed and the object
ends gone. **Proven load-bearing:** reverting the branch to `Err(Conflict)` makes it fail
with `first delete must succeed: Conflict` (mutation run confirmed).

### 4. Crypto provenance — vendored, KAT-pinned; explicit sign-off requested (NEEDS-HUMAN)
Kept the in-crate SHA-256/HMAC (`crates/server/src/s3/crypto.rs`), now refactored into an
**incremental** `Sha256` (`crypto.rs:100`) the streaming PUT hashes bodies with
(`incremental_matches_one_shot` proves it agrees with one-shot for every split point).
Concrete reason it is not a vetted crate: the only SHA-256 providers reachable in the tree
are `ring`/`rustls`, whose licenses are **outside** the `deny.toml` allowlist
(`deny.toml:24-45` — ISC/OpenSSL-family absent), so promoting either turns `cargo deny`
(inside the gating `cargo xtask ci`) red; and adopting RustCrypto (`sha2`/`hmac`) is a
**new dependency + allowlist edit**, which `docs/INTEGRATION.md §4` reserves as a
**human-only** decision (ADR-0003 three-test audit). The carry-forward sanctioned exactly
this resolution ("…**or** record explicit sign-off to keep the vendored implementation").
**NEEDS-HUMAN:** bless the vendored KAT-pinned impl, or greenlight `sha2`+`hmac` (a
one-function swap behind the same KATs). Every primitive is machine-checked against
NIST FIPS-180-4 / RFC 4231 / AWS vectors.

### 5. TLS — the seam is real, but binding a listener is blocked on a §4 human decision
Wiring a real rustls listener needs a crypto provider (`ring`/`aws-lc-rs`) whose license is
outside the allowlist → `cargo deny` red, **or** an allowlist addition, which — like item 4
— `docs/INTEGRATION.md §4` makes **human-only** ("any new dependency or license"). As
builder I must not weaken the gate or make that call, so I cannot ship a green TLS test
without it (a dev-dep on `rcgen`/`rustls` also trips `cargo deny`, which checks dev-deps).
`TlsIdentity` (`crates/server/src/s3/mod.rs:56`) is carried as the distinct **public** S3
identity (separate from internal step-ca mTLS — blueprint:620-623, ADR-0036 req 5), and
`serve` takes an already-bound listener so a deployed host / #367 fronts it with the
public-TLS terminator. **NEEDS-HUMAN:** approve `rustls` + a provider and allowlist its
license (one `deny.toml` line + the ADR-0003 audit); then `serve` wraps the listener in a
`tokio_rustls::TlsAcceptor`. Check exercises loopback (the brief's DEFERRED clause sanctions
loopback; live public TLS is #367). This is a crisp escalation, not the prior hand-wave.

### 6. Pre-auth ordering + replay window (body was materialised before auth)
`sigv4::verify` (`sigv4.rs:307`) now takes **no body**: it authenticates from the headers +
the *claimed* `x-amz-content-sha256` and returns a `PayloadHash`, and the handler calls it
**before** touching the body (`s3/mod.rs:170`, verify precedes `body.into_data_stream()`).
So an unsigned request never forces a body allocation. **Proven load-bearing:**
`unsigned_put_is_refused_before_its_body_is_read` sends a 1-GiB `Content-Length` with **no**
body; with the fix it 403s promptly, and with the old buffer-then-verify order (mutation
run) it times out after 10 s → red. Replay: `verify` takes `now` and rejects an
`x-amz-date` outside a 15-minute window (`RequestTimeTooSkewed`), pinned by
`verify_rejects_a_stale_signature` and `amz_date_round_trips_through_unix_seconds`.
The signed body is checked against its running hash **after** it streams (leased,
uncommitted) and **before** commit, so a tampered body is rejected (400
`XAmzContentSHA256Mismatch`) without ever being published (`put_object_streaming`,
`lib.rs:144`, mismatch at `lib.rs:171`). Residual, documented: `UNSIGNED-PAYLOAD` leaves the body outside the
signature (S3 semantics; relies on the transport) — accepted for the M4 floor.

## Red → green evidence
- **RED (net-new module, brief-sanctioned compile-error red):** pre-fix there is no
  `wyrd_server::s3`, no listener, no SigV4 — the test cannot compile or dial. The brief
  states "Net-new coverage red is a compile-error red (acceptable for a net-new module)."
- **Behavioral RED proven by mutation** (the assertions the carry-forward required be
  behavioral, not vacuous): non-idempotent DELETE → `concurrent_delete_is_idempotent` fails
  (`Conflict`); buffer-before-auth → `unsigned_put_is_refused_before_its_body_is_read` times
  out. Both reverted; suite green.
- **GREEN:** `cargo test -p wyrd-server --test s3_http_wire` → 5 passed; `--lib s3::` → 13
  passed (KATs incl. the AWS published-query oracle + incremental-hash equivalence +
  skew).
- **Whole gate:** `./engine/xtask.sh ci` → **all checks passed** (fmt, clippy `-D warnings`,
  build, full test incl. DST/conformance, `cargo deny`). `cargo fmt --all --check` clean.

## Streaming caveat (honest)
A unit test cannot directly assert "no whole-object buffer" (memory is not observable in a
`#[test]`); streaming is enforced **by construction** (chunk-at-a-time PUT, bounded-channel
GET) and exercised by the multi-chunk round-trip. The pre-auth test is the closest
observable proxy for the amplification half (a body that is never read).

## Known NEEDS-HUMAN (brief-declared, carried)
1. **Crate boundary** — landed inside `crates/server` (M0 "combined server, split later",
   ADR-0016), fixing the test path to `crates/server/tests/s3_http_wire.rs`. The `s3` module
   lifts out cleanly to the `gateway-s3` crate §5:132 names (it depends only on `Gateway` +
   traits) if you want that.
2. **SigV4 scope** — header-based `AWS4-HMAC-SHA256`, single static credential, `s3` service,
   15-minute freshness window, signed-payload integrity. No presigned-query (out of scope).
   Confirm the floor + the crypto provenance (item 4).
3. **Sequencing** — pinned to the M4 integration branch per the item note (vs. its own
   sequence between M4 and M7). Human call.
4. **Error-code floor** — minimal S3 `<Error>` set (AccessDenied, SignatureDoesNotMatch,
   InvalidAccessKeyId, RequestTimeTooSkewed, NoSuchKey, XAmzContentSHA256Mismatch,
   OperationAborted). Full conformance sweep out of scope (pre-M8).
5. **Crypto/TLS dependency+license** (items 4 & 5 above) — the two decisions
   `docs/INTEGRATION.md §4` reserves for the human.

## Manual validation (runnable role / #367 hand-off)
```
# from $PDCA_WORKTREE
cargo run -p wyrd-server --bin wyrd -- s3 \
  --access-key AKIAIOSFODNN7EXAMPLE \
  --secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY \
  --s3-listen 127.0.0.1:8080 --data-dir /tmp/wyrd-s3
# then with any SigV4 client (awscli/boto3/s3cmd), region us-east-1, endpoint
# http://127.0.0.1:8080: aws s3api put-object / get-object / delete-object round-trips;
# an unsigned curl gets 403; a >15-min-old signed request gets RequestTimeTooSkewed.
```
Public-TLS-on-a-deployed-host is observed at #367 (needs the coordination prerequisite,
0015:443-463) — pre-declared off-Check by the brief.
