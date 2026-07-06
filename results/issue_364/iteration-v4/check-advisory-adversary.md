# Adversarial review — issue #364 / s3-http-wire-surface (iteration 4)

Advisory only; never gates. Grounded on the target at
`/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch applied to working tree). I re-ran the
evidence and tried to break the fix; below is what survived and what didn't.

## Findings

- **NEEDS-HUMAN — Concurrent GET-during-DELETE bypasses the reader-safe grace window the
  code itself cites.** `Gateway::delete_object` reclaims fragments *eagerly and immediately*
  (`crates/server/src/lib.rs:276`–`285`, `delete_fragment_at` on the winning commit), while
  `get_object_streaming` returns a `ReceiverStream` fed by a spawned reader over a
  bounded(4) channel (`crates/server/src/lib.rs:321`,`324`,`326`). Concrete failing case: a
  client GETs a multi-chunk object (>4 chunks) over a slow/backpressured socket, so the
  reader task blocks on `tx.send` with several chunks still unread; a second client DELETEs
  the same key; the eager reclaim removes *all* fragments of the not-yet-read chunks; the
  reader's next `read::read_chunk_verified` (`crates/core/src/read.rs:345`) errors
  (RS(6,3): all fragments gone ≫ m=3), so the already-`200`-status chunked GET terminates
  mid-object with an error / a body missing its terminating 0-chunk (silent truncation for
  a client that doesn't validate framing). The orphan **grace window** (`0005:291-294`) that
  exists precisely to keep in-flight reads safe *is* written to the ledger by `unlink` but
  the happy-path eager reclaim ignores it — GET takes no version-hold/lease. Human call
  whether truncating a concurrent read is acceptable S3 semantics for the first-deployment
  floor, or whether delete must honour the grace window it advertises.

- **NEEDS-HUMAN — "Real-SDK interop" is still not proven, and a stock aws-sdk streaming PUT
  is refused with 501.** Iteration-3 explicitly asked for a real-SDK interop path; the new
  round-trip test still signs with the gateway's *own* `sigv4::sign`
  (`crates/server/tests/s3_http_wire.rs:87`), so the only independent oracle is the AWS
  known-answer vectors (`crates/server/src/s3/sigv4.rs:558`,`588`) — which pin the signature
  *math* but not the wire plumbing (axum path/query extraction, header casing, chunked
  framing). Meanwhile a default modern aws-sdk / boto3 `put_object` that emits
  `x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD` is classified
  `PayloadHash::Streaming` (`crates/server/src/s3/sigv4.rs:372`) and rejected `501
  NotImplemented` (`crates/server/src/s3/mod.rs:218`–`226`). So "S3-compatible object store"
  (blueprint:382) holds only for clients configured to send a single non-chunked
  signed/UNSIGNED-PAYLOAD body. This is the pre-declared SigV4-scope NEEDS-HUMAN, but the
  brief/reviewer claim that the green "is not self-referential" rests entirely on KAT
  vectors — no actual SDK is exercised, so the wire-framing half of interop is unverified.

- **The recorded gating gate C4-ci (fail, exit 101) is NOT reproducible on the target and is
  not attributable to this diff.** I re-ran the exact gate command
  (`cargo test --workspace --exclude wyrd-dst`) → **exit 0, 83 test binaries green, 0
  failures**. I then stress-ran the diff's new concurrency tests to rule them out as the
  flake source: `s3_http_wire` 5× green, `concurrent_delete_is_idempotent` 15× green,
  `gc_delete_backstop` 3× green. `cargo build --workspace --exclude wyrd-dst` is also clean
  (exit 0), so it is not a compile red. This matches the iteration-3 adversary's read: the
  red is a stale/environmental flake, most likely in a *pre-existing, untouched* suite
  (`crates/server/tests/gateway_lease_expiry.rs` / `gateway_cluster.rs` — not in this
  patch). Advisory: this does **not** refute the fix; but the deterministic gate is still
  recorded red, so a human must re-run it green (and localize the flake) before accept — the
  gate cannot be waved off on the strength of the per-fix `run-verify` green alone, which
  only exercises the new test.

## Attempted to refute and could not (reported as strength)

- **DELETE crash-leak backstop is genuinely real now.** `metadata::unlink` writes an orphan
  grace record for every placed fragment in the *same atomic commit* that unbinds the object
  (`crates/core/src/metadata.rs:404`–`419`), and GC scans `ORPHAN_PREFIX` and reclaims after
  the grace window (`crates/custodian/src/gc.rs:37`–`40` re-uses the shared `orphan_key`;
  reconcile reads the ledger). The iteration-3 "no orphan record / false GC-backstop"
  complaint is resolved. Key protocol is single-sourced so writer/reader can't drift.
- **Delete idempotency holds under real races** — 64-round multi-thread test, 15× re-runs, 0
  failures; the CAS-conflict → re-resolve → `Ok(false)` branch (`lib.rs:250`+) is correct.
- **Crypto provenance corrected** — `crates/server/src/s3/crypto.rs` is now a thin wrapper
  over RustCrypto `sha2`/`hmac` (MIT/Apache, already allow-listed), still pinned to
  FIPS-180-4 / RFC-4231 / AWS KAT vectors. (Nit, not a finding: the `constant_time_eq`
  doc-comment at `crypto.rs:77` claims "length-independent" but the impl short-circuits on
  `a.len() == b.len()`; harmless here since hex signatures are fixed length.)
- **Auth precedes body materialisation** — `sigv4::verify` runs before the body stream is
  touched (`mod.rs:180`), and the "1 GiB declared, 0 sent → prompt 403" test confirms it.
- **Streaming is behaviourally demonstrated, not just present** — the `RecordingChunkStore`
  test asserts the first fragment lands after ≤2 pieces pulled of 16
  (`s3_http_wire.rs:501`), which genuinely fails for a buffering implementation.
- **XML error injection is closed** — `error_response` escapes both code and message
  (`mod.rs:322`–`323`), including the attacker-influenced streaming `sentinel`.
- **percent-decode has no off-by-one** — `i + 2 < bytes.len()` (`mod.rs:287`,
  `sigv4.rs:145`) is exactly the condition for a full trailing `%XX`; a truncated escape
  passes through literally, as documented.
- **`amz_date[..8]` (`sigv4.rs:339`) is not a panic vector** — `HeaderValue::to_str` only
  yields visible-ASCII (single-byte chars), so byte index 8 is always a char boundary.
