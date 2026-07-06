# Adversarial review — issue #364 / s3-http-wire-surface

Skeptic's pass. I tried to refute the red→green proof, the fix, and the reviewer's
verdict. Findings below; `path:line` grounded on `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Advisory only — I gate nothing.

## Attacks that landed

- **NEEDS-HUMAN — the "Stream, don't buffer" invariant is inverted, not satisfied**
  (`crates/server/src/s3/mod.rs:51,167`). The brief lists streaming bodies **in Scope**
  ("**streaming** request/response bodies (no full-object buffering, per 0015:789)") and as
  a standing invariant ("Stream, don't buffer … no full-object buffering into the gateway
  heap"). The handler does the opposite: `to_bytes(body, MAX_BODY_BYTES)` materialises the
  **entire** request body into a `Bytes` before any gateway work, and GET buffers the whole
  object (`Gateway::get_object` returns `Vec<u8>`, `lib.rs:149`) into `Body::from(bytes)`.
  The 256 MiB cap does **not** address the cited OOM cliff — it is per-request, so a
  concrete failing case is *k* concurrent signed PUTs → up to *k*×256 MiB resident
  (e.g. 16 clients ≈ 4 GiB). The in-crate comment concedes the deferral, but the brief
  put streaming *inside* this issue's scope, not in the deferred set — a human must decide
  whether shipping a buffering-only floor satisfies the slice or is a scope miss.

- **NEEDS-HUMAN — the round-trip proves self-consistency, not S3 compatibility**
  (`crates/server/tests/s3_http_wire.rs:1239`; `crates/server/src/s3/sigv4.rs:163`). The
  binding success criterion is an "**S3-compatible** HTTP listener" that "an S3 client
  drives." The test signs with the gateway's **own** `sigv4::sign`, which shares
  `canonical_request` with `verify`, so any canonicalization bug is invisible to the
  round-trip — sign and verify would agree with each other while both diverging from AWS.
  The only *independent* anchor is `sigv4_get_vanilla_known_answer`, whose shape is `GET /`,
  **empty query**, two headers, empty body. That leaves the two canonicalization rules AWS
  actually requires — **sorted, percent-encoded canonical query string** and
  **URI-encoded** path — completely un-anchored: `canonical_request` interpolates the raw
  `query`/`uri` verbatim (`sigv4.rs:163`). Concrete failing case: a real `aws-sdk`/boto3
  request carrying query params (or unsorted/needing-encoding) computes its canonical query
  by sorting+encoding, the gateway computes it from the raw string → signatures diverge →
  the "S3-compatible" client is 403'd. No test exercises a non-empty query or a real SDK.
  A human should decide whether "S3-compatible" is met by self-consistent sign/verify at
  Check with real-SDK interop deferred to #367.

- **Concurrent DELETE is not idempotent, contradicting its own contract**
  (`crates/server/src/lib.rs:163-169`, `crates/core/src/metadata.rs:318-326`).
  `delete_object`'s doc-comment and the S3 handler (`mod.rs`, `Method::DELETE → 204`)
  promise idempotent success. But `unlink` CAS-requires the dirent unchanged
  (`metadata.rs:319`); two concurrent DELETEs of the **same existing key** both read the
  same dirent, the first commits (204), the second's `.require(dirent_key, dirent_bytes)`
  now fails → `CommitOutcome::Conflict` → `GatewayError::Conflict` → **HTTP 409
  OperationAborted**, not 204. A client that retries or races itself gets a spurious 409
  where real S3 returns 204. The single-threaded test never exercises this (only a
  same-key delete of a present-then-absent object, `s3_http_wire.rs:1323`).

- **UNSIGNED-PAYLOAD leaves the body outside the signature, and there is no TLS or
  freshness at M4** (`crates/server/src/s3/sigv4.rs:240`). With
  `x-amz-content-sha256: UNSIGNED-PAYLOAD` the canonical payload hash is the literal string,
  so the signature does **not** cover the body; `verify` also enforces no `x-amz-date`
  freshness/skew (conceded in the module doc). At M4 the listener is plaintext loopback
  (TLS deferred to #367), so a captured signed request is replayable indefinitely and, under
  UNSIGNED-PAYLOAD, its body is malleable. This is AWS-standard behaviour and pre-declared
  as deferred, but it is a real residual on the *M4 wire as shipped* worth naming at
  sign-off, since the "fail-closed auth" invariant is weaker than the prose implies once
  UNSIGNED-PAYLOAD is offered without a transport integrity layer.

- **The asserted RED is a compile-error red, not a behavioural one**
  (`check-gates.json` C4-verify "red without the fix"; `crates/server/tests/s3_http_wire.rs`
  new file importing `wyrd_server::s3`). Before the patch the `s3` module does not exist, so
  the test fails to **compile** — this proves the code is net-new, not that the assertions
  discriminate a correct implementation from a plausible-but-wrong one. C2 Reproduction is
  "none (no gate configured)." Acceptable for net-new coverage (the brief says so), but the
  reviewer's confidence in "red→green" should not be read as behavioural bisection.

## Attacks that did NOT land (attempted, could not refute)

- **Auth downgrade via header stripping** — I tried to bypass the signature by omitting
  `x-amz-content-sha256` or signing only `host`: `verify` forces `host` and `x-amz-date`
  into `SignedHeaders` (`sigv4.rs`, the downgrade guard), and the body hash is folded into
  the canonical request even in the `None` branch, so the body stays bound. Could not forge.
- **`unlink` CAS vs a concurrent overwrite** — an overwrite either rewrites the inode
  in place (caught by `.require(inode_key, …)`) or repoints the dirent (caught by
  `.require(dirent_key, …)`); both paths Conflict rather than dropping a live record. Sound.
- **Production-path exercise** — the round-trip genuinely goes through `axum::serve` →
  the real router → `handle` (`s3/mod.rs`), not a parallel harness. The path is real; my
  objection (above) is that the *signer* is in-tree, not that the *server* is bypassed.
- **SHA-256/HMAC correctness** — pinned to FIPS-180-4 / RFC 4231 / AWS get-vanilla vectors;
  I did not find a divergence.
