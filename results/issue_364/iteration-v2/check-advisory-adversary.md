# Adversarial review — issue #364 / s3-http-wire-surface (iteration 2)

Skeptic's pass. I tried to refute the red→green evidence and the six carry-forward
"fixed" claims. The deterministic C4 gates (xtask ci, run-verify) pass and I do not
dispute them; my quarrel is with what the green *proves*. Findings below are grounded on
`$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt-l1`.

## Refutations

- **NEEDS-HUMAN — The #1 prior-rejection ("stream, don't buffer") is implemented but never
  behaviorally demonstrated; the round-trip test cannot tell streaming from buffering.**
  The only wire round-trip (`crates/server/tests/s3_http_wire.rs:174`) PUTs a **55-byte**
  object with an **8-byte** chunk size. Every assertion in the suite (byte-identical
  round-trip, 200/204/404 codes) passes *identically* for a fully-buffering implementation
  — nothing bounds resident memory, and no large-object case exists. The structural
  streaming lives in `put_object_streaming`/`stream_write_data` and `get_object_streaming`
  (bounded channel, `crates/server/src/lib.rs`), but the *evidence* that iteration 1 was
  rejected for producing (a buffering floor) is not distinguished from the *evidence* that
  iteration 2 offers. A reviewer asserting "streaming delivered/demonstrated" is relying on
  code-reading, not the test. Concrete missing case: a PUT of an object ≫ `DEFAULT_CHUNK_SIZE`
  (1 MiB, `lib.rs:41`) with a peak-RSS or per-chunk-observation assertion — absent.

- **NEEDS-HUMAN — DELETE permanently leaks committed chunk fragments, and `unlink`'s own
  doc comment makes an unwarranted reclamation claim.** `crates/core/src/metadata.rs:305`
  states the orphaned fragments are "left as collectable garbage the pending-ledger sweep /
  GC reclaims." That is false for the delete path: the sweep (`sweep_expired_leases`,
  `crates/core/src/write.rs:461`) scans **only** `pending:` ledger keys, and a *committed*
  object's chunks were removed from the ledger at phase-4 release (`write.rs:275`). So a
  DELETE (net-new here) of a committed object orphans its fragments with **no** ledger entry
  and **no** mechanism that ever reclaims them — a monotonic on-disk leak per delete. The
  brief defers a chunk-store delete to a later milestone, but the *false* "GC reclaims" claim
  in the shipped comment is this diff's, and should be corrected or explicitly signed off.

- **NEEDS-HUMAN — Carry-forward item 2 ("real-SDK canonicalization") is proven only at the
  unit level; no request from a real SDK is ever exercised, and two concrete real-SDK inputs
  break.** The AWS query KAT (`sigv4.rs` `sigv4_aws_docs_example_sorts_query`) is a genuine
  independent oracle for `canonical_query` — credit where due — but the *wire* round-trip
  still signs with the gateway's own `sigv4::sign` and **never sends a query string**, so the
  end-to-end path is self-referential exactly as item 2 warned. Two concrete breaks a real
  client hits: (1) the canonical URI is the **raw** path (`crates/server/src/s3/mod.rs:165`
  → `canonical_request` `uri`, `sigv4.rs:254`) and `split_bucket_key` stores the key
  **still percent-encoded** (`mod.rs:192`), so a boto3 PUT to key `my file.txt` is stored/keyed
  as `my%20file.txt` — self-consistent round-trip, but a different object identity than S3;
  (2) aws-cli/boto3's default `x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD`
  (aws-chunked upload) is classified `PayloadHash::Signed("STREAMING-…")` (`sigv4.rs:356`),
  so the gateway would hash-mismatch (400) **and** store the chunk-framing bytes as object
  data. Item 2 asked "ideally a real-SDK interop test"; none was added.

- **NEEDS-HUMAN — Carry-forward item 5 (TLS) was not delivered: Check still runs plaintext
  loopback.** `TlsIdentity` is still an unbound seam — `S3Config::new` sets `tls: None`
  (`crates/server/src/s3/mod.rs:88`), `cmd_s3` never populates it, and `serve` binds a plain
  `tokio::net::TcpListener`. This is the same "modeled but unwired" state iteration 1 was
  dinged for. The rustls-provider-license NEEDS-HUMAN may be a legitimate blocker, but item 5
  asked to *wire the loopback TLS listener*, and it is not wired — the human should decide
  whether re-deferral is acceptable rather than treat item 5 as addressed.

- **NEEDS-HUMAN — Carry-forward item 4 (vendored crypto on the auth boundary) is re-deferred,
  not resolved.** The hand-rolled SHA-256/HMAC (`crates/server/src/s3/crypto.rs`) remains on
  the signature-verification path; the module doc re-declares it a pre-declared NEEDS-HUMAN.
  Item 4 asked to *either* run the ADR-0003 three-test RustCrypto audit *or* record explicit
  sign-off to keep the vendored code on a security surface. Neither the audit nor a recorded
  sign-off is in this diff — it is the same deferral with more prose. (The FIPS/RFC/AWS
  vectors are present and pass, so the implementation is plausibly correct; the open question
  is provenance/policy, which is exactly the human call.)

- **Replay within the 15-minute window is still open (residual, pre-declared).** The freshness
  bound (`MAX_CLOCK_SKEW = 15 min`, `sigv4.rs:32,338`) blocks stale signatures but there is no
  nonce/once-cache, so a captured signed PUT/DELETE is replayable for 15 minutes — and on the
  plaintext-loopback Check wire it is capturable. Brief flags this as a residual; noting it so
  the human weighs it, not as a surprise.

- **`UNSIGNED-PAYLOAD` leaves the body unauthenticated on the plaintext wire (residual,
  pre-declared).** For `PayloadHash::Unsigned` (`sigv4.rs:356`) `put_object_streaming` skips
  the body-hash check entirely, so on the plaintext Check wire an on-path attacker can
  substitute the body of an UNSIGNED-PAYLOAD PUT. Pre-declared in the brief; real until TLS
  is wired (see the TLS finding above — the two compound).

## Attempted but could not refute

- **Auth-before-body (item 6).** I tried to show the body is materialized before auth: it is
  not. `handle` runs `sigv4::verify` on request parts before touching `body`
  (`mod.rs` handler), and `unsigned_put_is_refused_before_its_body_is_read` genuinely proves
  it — a 1 GiB declared / 0-sent unsigned body gets a prompt 403 under a 10 s timeout. This
  assertion is behavioral and sound.
- **Concurrent DELETE idempotency (item 3).** I tried to find an interleaving that yields 409:
  `delete_object`'s Conflict→re-resolve→`Ok(false)` branch plus the dirent+inode double-CAS in
  `unlink` closes the delete-vs-delete race, and the overwrite race is retried (bounded 8x).
  The `concurrent_delete_is_idempotent` test races two spawned tasks on a multi-thread runtime
  over 64 rounds and asserts "exactly one removes, object ends gone" — correct for every
  interleaving I could construct. Sound.
- **SigV4 correctness of the crypto chain.** The `get-vanilla` and IAM worked-example KATs pin
  the full canonical-request→string-to-sign→signing-key→HMAC chain to AWS's published
  signatures; I could not construct a divergence for the header-signed, no-query floor.
