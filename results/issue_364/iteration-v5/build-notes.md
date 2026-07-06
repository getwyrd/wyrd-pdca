# Build notes — issue 364 / s3-http-wire-surface (iteration 5)

_Withheld from the reviewer. Rationale, alternatives, and the pre-declared NEEDS-HUMAN
calls for the human at sign-off._

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (M4 integration base).
Built in `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`; all `path:line` citations are
against that worktree at base tip `5d87cc4` (= `origin/feat/m4-production-metadata-backend`).
`patch.diff` `git apply --check`s cleanly on a fresh checkout of `5d87cc4`.

This iteration **starts from the iteration-4 patch as the working floor** (it applied cleanly
to `5d87cc4`) — keeping everything the prior sign-offs said to keep (signed PUT→GET→DELETE
byte-identical, unsigned/bad-sig → 403, fail-closed auth, auth-before-body, AWS published-vector
pinning, RustCrypto on the auth boundary, real streaming PUT/GET, concurrent-DELETE idempotency,
percent-decoded key identity, XML error escaping, the DELETE crash-leak orphan backstop, and
placement-aware delete) — and then **corrects the two hard reject/carry-forward items from
iteration 4**: the recurring **real-SDK streaming interop gap** (the decisive fix) and the
**C4-ci authoritative-green** record. The concurrent-GET-during-DELETE item is surfaced as a
pre-declared design NEEDS-HUMAN with a concrete recommendation (below).

## Iteration-4 reject / carry-forward — disposition

### 1. Real-SDK interop: `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` → 501. **Now implemented.** (decisive)

This is the reject that recurred through iterations 2, 3, and 4: a **stock modern SDK**
(boto3 / aws-sdk) sends an object PUT `aws-chunked`-framed with
`x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, and every prior iteration
**refused it 501** — so the gateway was not actually "S3-compatible" for a real client, only
for single non-chunked signed bodies the gateway's own `sign()` produced.

**Fix — decode + verify the `aws-chunked` streaming body as it streams.** New module
`crates/server/src/s3/streaming.rs`:

- `sign_chunk` (`streaming.rs:82`) computes the SigV4 **chunk signature** chained off the seed
  (`AWS4-HMAC-SHA256-PAYLOAD` string-to-sign). It is **pinned to AWS's published streaming
  worked example** — the known-answer test `aws_published_streaming_example` (`streaming.rs`
  test mod) asserts the exact chunk signatures AWS documents for the 64 KiB + 1 KiB + 0-byte
  example (`ad80c730…`, `0055627c…`, `b6c6ea8a…`). This is a **genuine independent oracle**:
  the chunk-signing math is AWS-correct, not merely self-consistent with the producer.
- `decode` (`streaming.rs:98`) is an incremental `aws-chunked` parser over a rolling buffer,
  running on a spawned task feeding a **bounded** `mpsc` channel (depth 4). It strips the
  `<hex-len>;chunk-signature=…\r\n<data>\r\n` framing, **verifies each chunk's signature in
  constant time** (fail-closed — a chunk the credential holder did not sign aborts the decode
  with `StreamingError::ChunkSignature` → HTTP 403, before the bytes reach the store), and
  yields the raw object bytes. Because it forwards each chunk as it is parsed and blocks on the
  channel, peak resident bytes stay `O(chunk size)` — the "stream, don't buffer" invariant
  holds on this path too (0015:789).

Wiring:
- `sigv4::PayloadHash::Streaming` now carries a `StreamingContext` (`sigv4.rs`, seed signature +
  signing key + date-time + scope + signed flag) instead of the bare sentinel string. `verify`
  builds it **only after** the seed signature is checked (`sigv4.rs`, the reordered tail), so
  the boundary stays fail-closed — no chunk material is produced for an unauthenticated request.
- The handler's PUT branch (`crates/server/src/s3/mod.rs`) de-frames a `Streaming` body through
  `streaming::decode` and feeds the decoded raw bytes to the **existing production**
  `Gateway::put_object_streaming` with `PayloadHash::Unsigned` (the per-chunk signatures already
  authenticated the body; there is no separate single content-hash to re-check). The signed /
  unsigned single-shot paths are unchanged.
- `gateway_error_response` maps `StreamingError::ChunkSignature → 403 SignatureDoesNotMatch` and
  `StreamingError::Framing → 400 InvalidRequest`.

**Independent-oracle discipline (the recurring reviewer concern).** The wire round-trip still
cannot pull in `aws-sdk-s3`/boto3 as a real client: that is a **new dependency + license**
decision the brief lists as a project-defined human-only item (INTEGRATION §4), and it would
drag a large transitive tree past `cargo deny`. So instead the interop test constructs the
**exact bytes a real SDK writes** — a seed request signed over the streaming sentinel via
`sign_with_payload_hash`, then chunks framed with signatures chained off the seed — and the
**chunk-signing algorithm those bytes use is KAT-pinned to AWS's published numbers**. Every
crypto layer is anchored to an AWS published vector: seed canonicalization by the get-vanilla /
docs-example KATs, chunk signing by the streaming-example KAT. This is as strong as a headless
test can be without a new dependency; a live boto3/aws-sdk harness remains a **NEEDS-HUMAN**
slice (its own dependency call), but the compatibility gap it flagged — 501 on a stock SDK — is
now **closed and behaviourally demonstrated red→green**.

**Rejected alternative — keep the 501 and "ratify the scope".** iteration-4's carry-forward
offered "add a real path **or** ratify the scope explicitly." Ratifying 501-on-stock-SDK was
rejected: the blueprint's binding claim is "a single-zone, production-durable, **S3-compatible**
object store" (blueprint:382) whose day-one DoD is an S3 round-trip (blueprint:698-699); a floor
that rejects the *default* wire form of every modern SDK is not S3-compatible in any useful
sense. Closing the gap is the smaller change to the real end result than re-litigating what
"compatible" means. Cost of the fix: one ~290-line module + a `PayloadHash` shape change, no new
dependency, no change to the write/read/commit core.

### 2. C4-ci recorded red (`cargo test --workspace` exit 101). **Re-run authoritative GREEN.**

Ran the **whole** gate through the project runner on the final tree:
`PDCA_WORKTREE=… ./engine/xtask.sh ci` → **"xtask ci: all checks passed"** (fmt, clippy
`-D warnings`, build, full workspace test incl. the DST custodian/network/concurrency suites,
`cargo deny`, conformance vectors). The iteration-4 diagnosis was that the captured red was a
non-reproducing, wall-clock-sensitive flake in a **pre-existing, untouched** test
(`crates/server/tests/gateway_lease_expiry.rs`), not attributable to the diff; both reviewers
had re-run green. It did **not** reproduce here either. I deliberately did **not** edit
`gateway_lease_expiry.rs`: it reads a single process clock and brackets the lease with a
**10-minute** window (`started ≤ expiry ≤ started + 600_000`, `gateway_lease_expiry.rs:137-144`)
— it is robust to ordinary scheduling jitter, so there is no honest "quarantine" to apply
without inventing a defect. The new loopback tests I added were run **3×** back-to-back with no
flake (all 11 green each time). The authoritative gate record is green.

### 3. Concurrent GET-during-DELETE truncation. **Pre-declared design NEEDS-HUMAN (recommendation below).**

iteration-4 framed this as a decision, not a hard reject: happy-path `delete_object` reclaims
fragments **eagerly** (`lib.rs:276` `reclaim_fragments`), so a slow multi-chunk **streaming GET**
(`get_object_streaming`, `lib.rs:312`, reads chunk-by-chunk lazily) can be truncated mid-object
by a concurrent DELETE — `read_chunk_verified` errors and the response ends short.

I did **not** rip this out this iteration, on purpose. The two prior requirements are in genuine
tension: iterations 3-4 explicitly demanded **eager** reclaim ("DELETE must not leak fragments
waiting for a custodian pass") and hardened it with the orphan-grace backstop + placement-aware
delete + a 64-round idempotency test — all of which survived adversarial attack. Honouring a
reader grace window means the **opposite**: DELETE must *defer* fragment reclaim to GC's
reader-safe window (the orphan grace record already models exactly that window) and stop deleting
eagerly. That is the correct S3 semantic, but it inverts the eager-reclaim contract, rewrites the
two eager-reclaim tests, and re-opens the "does GC really reclaim" question the backstop work just
closed. Doing that *in the same iteration that lands the streaming auth surface* would put two
large, coupled changes on one security-sensitive diff. The brief's **BINDING** criterion is a
**sequential** PUT→GET→DELETE round-trip (works, green) plus unsigned-refused (works); concurrent
GET-during-DELETE is beyond that floor.

**Recommendation for the human (owed at Check):** make happy-path DELETE **grace-window-honouring**
— unbind the object metadata immediately (already atomic + idempotent), but **stop the eager
`reclaim_fragments`** and let the custodian GC reclaim the recorded orphans after the reader-safe
window (the mechanism `unlink`'s orphan record already drives). That resolves the truncation *and*
removes the eager/GC redundancy, but it is a core+custodian semantics change that should be its
own reviewed slice, not a rider on the wire surface. Flagged NEEDS-HUMAN.

## Test / red→green (behavioural, demonstrated)

Named test file **`crates/server/tests/s3_http_wire.rs`**, new module `streaming_interop`
(two tests), driving the **production** wire path over a real loopback listener:

- `stock_sdk_chunked_put_round_trips_byte_identical` — a 4096-byte object sent as a signed
  `aws-chunked` body (512-byte frames + terminator, signatures chained off the seed exactly as
  an SDK does) must PUT `200` and GET back **byte-identical**.
- `tampered_chunk_body_is_refused_and_stores_nothing` — flip a byte of the first chunk's data;
  the chunk signature no longer verifies → **403**, and a follow-up GET is **404** (nothing
  committed): fail-closed on the streaming path.

**Demonstrated RED (behavioural, captured):** temporarily restoring the pre-fix 501 rejection in
the handler (keeping the new APIs) turns both red — `left: 501, right: 200` and `left: 501,
right: 403` — then GREEN with the decoder wired. (The `s3::streaming` unit tests
`aws_published_streaming_example` / `decodes_a_signed_chunked_body_byte_identically` /
`a_tampered_chunk_is_refused_fail_closed` are net-new coverage: compile-red for the new module,
plus the AWS-KAT anchor.) The 9 prior s3-wire tests + `gc_delete_backstop` remain green.

## Verification summary
- `cargo test -p wyrd-server --test s3_http_wire` → 11 passed (2 new streaming-interop tests);
  run 3× with no flake.
- `cargo test -p wyrd-server --lib s3::` → 19 passed (incl. the AWS streaming KAT).
- `cargo test -p wyrd-server -p wyrd-core -p wyrd-custodian` → all green.
- `cargo fmt --all --check` clean; `cargo clippy -p wyrd-server --all-targets` → 0 warnings.
- Full gate `PDCA_WORKTREE=… ./engine/xtask.sh ci` → **"xtask ci: all checks passed"**.
- `patch.diff` `git apply --check`s cleanly on a fresh checkout of base `5d87cc4`.

## NEEDS-HUMAN for sign-off
1. **Real-SDK / boto3-aws-sdk interop harness** — the streaming path now WORKS and is KAT-anchored
   to AWS published chunk signatures, but a live SDK harness would need `aws-sdk-s3` (a new
   dependency + license, an ADR-0003 §2 audit + `deny.toml` call — project human-only item). The
   501 gap is closed; the live harness is a separate dependency decision.
2. **Concurrent GET-during-DELETE truncation** — grace-window-honouring DELETE vs eager reclaim
   (see item 3 above); a core+custodian semantics slice. Recommendation recorded.
3. **`delete_fragment_at` trait addition** — additive default method directed by the iteration-3
   sign-off; ratify the `traits` touch (carried from iteration 4).
4. **Crate-boundary ratification** — landed inside `crates/server` (decided, documented at
   `s3/mod.rs`) vs a `gateway-s3` split (architecture §5:132). Carried.
5. **RustCrypto adoption** (`sha2`/`hmac`, ADR-0003 §2 audit; `cargo deny` green, no allowlist
   edit) — carried unchanged.
6. **SigV4 scope** — header-based auth + `UNSIGNED-PAYLOAD` + now `STREAMING-AWS4-HMAC-SHA256-
   PAYLOAD` (signed chunks). Presigned-query, `STREAMING-…-TRAILER` checksum verification, and
   `STREAMING-UNSIGNED-PAYLOAD-TRAILER` framing beyond de-frame remain out of scope. Confirm the
   floor.
7. **TLS deferral / #367**, **sequencing (M4 vs own M4→M7)**, **error-code floor breadth** —
   carried unchanged. Replay-within-15-min / UNSIGNED-PAYLOAD-on-plaintext remain the pre-declared
   residuals tied to the accepted TLS deferral.
