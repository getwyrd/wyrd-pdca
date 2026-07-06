# Build notes — issue 364 / s3-http-wire-surface (iteration 3)

_Withheld from the reviewer. Rationale, alternatives, and the pre-declared NEEDS-HUMAN
calls for the human at sign-off._

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (M4 integration base).
Built in `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`; all `path:line` citations are
against that worktree at tip `5d87cc4` (= `origin/feat/m4-production-metadata-backend`).

This iteration **starts from the iteration-2 patch as the working floor** (it applied cleanly
to `5d87cc4`) and then **corrects** every item the iteration-2 sign-off said must not be
re-deferred. Everything the sign-off said to keep — signed PUT→GET→DELETE byte-identical,
unsigned/bad-sig → 403, fail-closed auth, auth-before-body, AWS-published-vector pinning,
concurrent-DELETE idempotency, real streaming — is kept and re-verified. The whole gate
(`cargo xtask ci`: fmt + clippy `-D warnings` + build + test incl. DST + `cargo deny` +
conformance) is **green** on the final tree.

## Iteration-2 carry-forward — disposition (each corrected, none re-deferred)

### T5-a — Crypto provenance: hand-rolled SHA-256/HMAC **replaced** with vetted RustCrypto
The bespoke SHA-256/HMAC that sat on the auth boundary is gone. `crates/server/src/s3/crypto.rs`
is now a thin wrapper over the **RustCrypto `sha2` / `hmac`** crates (`crypto.rs:16-17`,
`crypto.rs:24-72`), keeping the same in-crate API (`Sha256` incremental / `sha256` /
`hmac_sha256` / `hex` / `constant_time_eq`) so the SigV4 layer is unchanged.

ADR-0003 §2 three-test dependency audit (the reason this is a defensible adoption, not a silent
new dep):
1. **Necessary?** Yes — a MAC on a security boundary must be a maintained implementation, which
   is exactly what iteration-2 demanded.
2. **License?** `sha2`, `hmac`, and their transitive deps (`digest`, `crypto-common`,
   `block-buffer`, `generic-array`, `typenum`, `cpufeatures`, `subtle`) are all
   `MIT OR Apache-2.0` — already on the `deny.toml` allowlist. **No allowlist edit is needed**,
   and `cargo deny check licenses bans advisories` is **green** (verified). This is why the
   earlier "ring/rustls licenses are outside the allowlist" objection does **not** apply to
   RustCrypto: it was never a license problem, it was a maturity/effort choice, now made.
3. **Behaviour pinned?** The KATs still run and pass against the vetted crate: NIST FIPS-180-4
   (`crypto.rs` `sha256_known_answers`), RFC 4231 (`hmac_sha256_rfc4231_case2/4`), the
   incremental-equals-one-shot property, and — end to end — the two **AWS published SigV4
   signatures** (`sigv4.rs` `sigv4_get_vanilla_known_answer` `5fa00fa3…`,
   `sigv4_aws_docs_example_sorts_query` `5d672d79…`). A green now proves the maintained crate
   reproduces AWS's reference answers, so it is an independent oracle, not self-consistency.

No `deny.toml` change ships. That is deliberate and is the whole point — the vetted crates are
already blessed by the existing allowlist, so adopting them is `cargo deny`-neutral.

### T5-b — Crate boundary: **decided** (inside `crates/server`), recorded, not left open
Committed to landing the wire surface **inside `crates/server`** (the `s3` module), not a new
`gateway-s3` crate. Recorded as a first-class decision in the module docs
(`crates/server/src/s3/mod.rs:19-27`) with the rationale: M0's posture is "combined `server`,
split later" (0015:147, ADR-0016), and `server` is already the one crate that knows concretes
(ADR-0010) — the composition root the listener binds at. Cost of the rejected alternative
(a `gateway-s3` crate now, architecture §5:132's *future* home): a new crate + `Cargo.toml` +
a workspace-member line + a re-export seam, for zero behavioural gain at M4 — a clean later
refactor once the surface grows (multipart/list/presign), not now. This is a builder-level
decision (crate layout), still a **NEEDS-HUMAN ratification** at sign-off, but it is now a
concrete choice to accept/override, not an unanswered question.

### T5-c — SigV4 scope: corrected to the header-only floor + the two named real-SDK breaks fixed
- **Percent-encoded key identity (real-SDK break 1).** The wire path is percent-encoded (the
  form the SDK signs, so SigV4 keeps using it verbatim — the raw path is passed to `verify`
  at the top of `handle`), but the **stored object key is now the decoded identity**
  (`percent_decode_utf8`, `mod.rs:282`; used at `mod.rs:208`). So boto3's `my file.txt`
  (sent as `/bucket/my%20file.txt`) is stored under `bucket/my file.txt`, not the literal
  `%20` form. **Proven behaviourally:** the wire test PUTs `/wyrd-bucket/spaces%20in%20key.txt`
  and then reads the *production* in-process handle by the **decoded** key (present) and the
  **encoded** key (absent) — a plain round-trip alone would be self-consistent and hide the
  bug (`s3_http_wire.rs` `percent_encoded_key_is_stored_under_the_decoded_identity`).
- **`STREAMING-AWS4-HMAC-SHA256-PAYLOAD` misclassification (real-SDK break 2).** `verify` now
  classifies the `STREAMING-…` content-sha256 sentinels as `PayloadHash::Streaming`
  (`sigv4.rs:61` variant, classified at `sigv4.rs:372`) — the seed signature still authenticates
  (the canonical request uses the literal), but the handler refuses the request with
  **`501 NotImplemented`** (`mod.rs:218`)
  instead of streaming the `aws-chunked` framing to the store as object bytes and then 400-ing
  on a bogus hex-hash comparison. Unit-pinned by `verify_classifies_aws_chunked_streaming_payload`.
- **Error-code subset (the floor):** `AccessDenied` / `AuthorizationHeaderMalformed` /
  `InvalidAccessKeyId` / `SignatureDoesNotMatch` / `RequestTimeTooSkewed` (403, `sigv4.rs`),
  `NoSuchKey` (404), `InvalidRequest` (400), `NotImplemented` (501),
  `XAmzContentSHA256Mismatch` (400), `OperationAborted` (409), `MethodNotAllowed` (405),
  `InternalError` (500). Presigned-query auth stays out of scope — no `Authorization` header →
  `AuthError::Missing` → 403 (documented at `mod.rs`).

### Adversary — XML error escaping
`error_response` now escapes both the code and the message through `xml_escape`
(`mod.rs:306`, applied at `mod.rs:322`). This closes the injection vector where an
attacker-influenced signed-header name (echoed by `AuthError::Malformed("signed header
\`{name}\` absent")`) could inject markup into the `<Message>` of an *unauthenticated*
response. Unit-pinned by `xml_escape_neutralises_markup_injection`.

### Adversary — streaming demonstrated behaviourally (not a buffering-equivalent round-trip)
Added `streaming_put_writes_chunks_as_they_arrive_not_after_buffering` (`s3_http_wire.rs`),
which drives the production `Gateway::put_object_streaming` with a lazy 16-piece body source
and a recording chunk store that captures how many pieces had been pulled when the **first**
fragment was written. Streaming writes chunk 1 after pulling ~1 piece; a buffering impl drains
all 16 first. **Load-bearing proven by mutation:** temporarily rewriting
`write::stream_write_data` to buffer the whole source before writing made the test fail with
"first write saw 16 pieces pulled" (mutation run, then reverted). This is the > `chunk_size`,
per-chunk-observation test iteration-2 asked for.

### Adversary — DELETE fragment leak + false GC comment
- **False comment corrected.** `metadata::unlink`'s doc no longer claims the *pending-ledger
  sweep* reclaims committed fragments (it scans `pending:` lease keys only, and a committed
  object's fragments carry no pending entry). It now states accurately that the **caller**
  reclaims them and the custodian **GC** (`crates/custodian/src/gc.rs`, which diffs stored
  fragments against every *committed* chunk map) is the backstop — not the pending sweep
  (`crates/core/src/metadata.rs:295-330`).
- **Leak fixed.** `unlink` now returns the removed inode (`Unlinked { outcome, inode }`,
  `metadata.rs:295-330`), and `Gateway::delete_object` reclaims exactly that object's committed
  fragments via the idempotent `ChunkStore::delete_fragment` on a winning commit
  (`reclaim_fragments`, `crates/server/src/lib.rs:243-273` region). **Proven behaviourally:**
  `delete_reclaims_committed_fragments` (`s3_http_wire.rs`) observes fragments present after a
  PUT and **gone** after the DELETE via a second on-disk `FsChunkStore` view. Idempotent
  `delete_fragment` means a racing overwrite / double-delete is a no-op, not an error, so the
  concurrent-DELETE idempotency invariant is preserved (that test still passes).

## Accepted residuals (explicitly NOT re-worked, per iteration-2 sign-off)
- **TLS:** plaintext loopback at Check is accepted; TLS wiring is a later human decision. Not
  re-touched. `TlsIdentity` remains the carried public-S3 seam, distinct from internal mTLS
  (`mod.rs:65-75`).
- **Replay-within-15-min** and **UNSIGNED-PAYLOAD-on-plaintext** are pre-declared residuals
  tied to the accepted TLS deferral. The 15-minute `x-amz-date` freshness window is enforced
  (`sigv4.rs` `MAX_CLOCK_SKEW`, `verify_rejects_a_stale_signature`).

## Test / red→green
Named test: `crates/server/tests/s3_http_wire.rs` (path confirmed at build; the crate-boundary
decision keeps it in `crates/server`, per T5-b). Net-new coverage, so the pre-fix state is a
**compile-error red** (no `s3` module / no listener to dial) — acceptable for a net-new module
per the brief. The *assertions* are behavioural, not vacuous:
- `signed_put_get_delete_round_trip_is_byte_identical` — the binding round-trip.
- `unsigned_request_is_refused`, `unsigned_put_is_refused_before_its_body_is_read`,
  `wrong_signature_is_refused_and_stores_nothing` — the fail-closed auth boundary.
- `concurrent_delete_is_idempotent` — S3's 204 on a raced delete.
- `percent_encoded_key_is_stored_under_the_decoded_identity` — real-SDK break 1 (NEW).
- `streaming_put_writes_chunks_as_they_arrive_not_after_buffering` — streaming, proven by
  mutation (NEW).
- `delete_reclaims_committed_fragments` — the fragment-leak fix (NEW).
- module unit tests: `crypto` KATs, `sigv4` AWS-published KATs + streaming classification +
  stale-signature, `mod` percent-decode + xml-escape.

Verification run in `$PDCA_WORKTREE` with the project toolchain:
`cargo test -p wyrd-server --test s3_http_wire` → 8 passed; `--lib s3::` → 16 passed;
`cargo test -p wyrd-core` green; `cargo fmt --check` clean; `cargo clippy` clean;
`cargo deny check licenses bans advisories` ok; and the full gate via the project runner
`./engine/xtask.sh ci` → **"xtask ci: all checks passed"**. Patch re-verified to `git apply`
cleanly on base `5d87cc4`.

## NEEDS-HUMAN for sign-off
1. **Crate-boundary ratification (T5-b):** confirm "inside `crates/server`" (decided +
   documented) or direct a `gateway-s3` split now.
2. **RustCrypto adoption (T5-a):** ratify the `sha2`/`hmac` dependency addition (ADR-0003 §2
   audit recorded above; `cargo deny` green, no allowlist edit). This is the "new dependency"
   human-only item per INTEGRATION §4 — now a concrete, deny-clean choice to bless.
3. **TLS deferral / #367:** live public-TLS on a deployed host is the first-deployment gate
   (#367); Check runs loopback plaintext (accepted).
4. **Sequencing:** M4 integration branch (item-note pin) vs. its own M4→M7 sequence — human call.
5. **Error-code floor breadth:** the minimal subset above is implemented; the full S3
   error-code conformance sweep remains a pre-M8 rabbit-hole (out of scope).
