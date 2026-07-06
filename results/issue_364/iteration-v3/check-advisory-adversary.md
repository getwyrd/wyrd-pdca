# Adversarial review — issue #364 / s3-http-wire-surface (iteration 3)

Skeptic's pass. Inputs: `patch.diff`, `brief.md`, `check-gates.json`; every `path:line`
grounded on the target at `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`). Advisory
only — I gate nothing.

## Refutation attempts

- **NEEDS-HUMAN — the recorded gating C4 failure does not reproduce.** `check-gates.json`
  marks `C4-ci` **fail / gating=true** with `path_line` "xtask: `cargo test --workspace
  --exclude wyrd-dst` failed with exit status: 101". On the target that command passes
  cleanly: two full runs → exit 0, 82 test binaries green (including `s3_http_wire.rs`'s
  8 tests, `crates/server/tests/s3_http_wire.rs`); the s3 binary looped 20× with no
  failure; `cargo fmt --check`, `cargo clippy --workspace --exclude wyrd-dst --all-targets`
  and `cargo deny check` (advisories/bans/licenses/sources) all green. So the deterministic
  blocking gate is recorded **red** while the current tree is **green** — either the gate is
  stale (target advanced past gate-time) or a *pre-existing* timing-sensitive test
  (`gateway_lease_expiry.rs` / `gateway_cluster.rs`, not this diff) flaked to exit 101.
  Per the gate rules a deterministic gate recorded red **blocks**; it must be re-run green
  before sign-off. Do **not** treat C4 as satisfied on the strength of the per-fix
  `run-verify.sh` red→green alone (which only exercises the new test, not the workspace).

- **NEEDS-HUMAN — DELETE crash-leak: the "GC backstop" claim is unwarranted (same false-GC
  class the iteration-2 carry-forward asked to fix).** `crates/server/src/lib.rs:222-225`
  and `crates/core/src/metadata.rs:319-324` assert the custodian GC "would eventually
  reclaim any [fragments] the caller misses" after a crash between the metadata commit and
  the eager fragment reclaim. But `delete_object` (`crates/server/src/lib.rs:226-273`)
  reclaims only via `reclaim_fragments` and **never writes an orphan-ledger grace record**
  (no `mark_orphaned`; the `server` crate does not even depend on `custodian`). GC only
  reclaims a fragment that (a) has an orphan-ledger record past grace, or (b) belongs to an
  expired **pending** lease — `crates/custodian/src/gc.rs:146-161`; an unreferenced fragment
  with neither is *"conservatively kept"* (`gc.rs:157-160`). A committed object's fragments
  carry **no** pending entry (released after the PUT commit, `write.rs:274-278`) and DELETE
  writes **no** orphan record, so a crash after the `unlink` commit strands them
  **permanently** — GC never reclaims them. Concrete failing case: PUT an object → DELETE it
  → kill the process between `metadata::unlink` committing and `reclaim_fragments`
  finishing → the object's fragments leak forever. The "no leak" property holds only on the
  happy path, which is the *only* path `delete_reclaims_committed_fragments`
  (`s3_http_wire.rs:366-393`) tests; the crash-safety backstop the comment promises does not
  exist. The comment was reworded from the rejected "GC reclaims" wording but is still
  false.

- **NEEDS-HUMAN — real-SDK interop still absent despite two carry-forwards demanding it.**
  Iteration-1 asked for "ideally a real-SDK interop test"; iteration-2 said "Add a real-SDK
  interop path." The rebuild anchors canonicalization on the AWS published known-answer
  vectors (`crates/server/src/s3/sigv4.rs:557-621`) — a genuine independent oracle for the
  *signing chain* — but adds no boto3/aws-sdk request through the actual axum handler. The
  wire round-trip still signs with the gateway's own `sign()` (`s3_http_wire.rs:87`), which
  shares `canonical_request`/`canonical_query` with `verify`, so the **handler-level**
  path→canonical-URI pass-through (`mod.rs:180-192` feeds `parts.uri.path()` verbatim as the
  canonical URI, no re-encode/normalize) is never exercised against a real client. A real
  SDK that normalizes/encodes its canonical URI differently from the raw request target
  would 403, and no test would catch it. Reviewer should not accept "canonicalization is
  proven" beyond the KAT: the *end-to-end handler path* against an external signer is
  unproven.

- **NEEDS-HUMAN — a default-config recent SDK PUT is rejected 501, so the "S3-compatible
  round-trip" does not hold against an out-of-the-box client.** `verify` classifies any
  `x-amz-content-sha256` beginning `STREAMING-` as `PayloadHash::Streaming`
  (`sigv4.rs:370-376`) and the PUT handler returns `501 NotImplemented`
  (`mod.rs:218-227`). Recent boto3 / aws-sdk versions (2024+ flexible-checksums default)
  send `STREAMING-UNSIGNED-PAYLOAD-TRAILER` / `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` for
  `put_object` **by default** — every such upload gets 501. The binding
  PUT→GET→DELETE round-trip therefore succeeds only with a client specifically configured
  for a single-chunk signed / `UNSIGNED-PAYLOAD` body (exactly what the gateway's own signer
  produces). This is *declared* as real-SDK break 2 (reject-not-misstore), but it means the
  blueprint's day-one "S3-compatible" round-trip does **not** run against a default SDK
  client — a human should explicitly acknowledge this is the accepted M4 floor rather than a
  demonstrated S3 round-trip.

## Attempted but could not refute

- **Streaming is behaviourally demonstrated** (not a buffering impl passing identically):
  `streaming_put_writes_chunks_as_they_arrive_not_after_buffering` (`s3_http_wire.rs:500-561`)
  drives the production `put_object_streaming` with a lazy source + recording store and
  asserts the first fragment lands after ≤2 of 16 pieces are pulled. Legit; addresses
  carry-forward item 1 for PUT (GET side is only structurally bounded via the `channel(4)`
  in `lib.rs:300`, not asserted, but that's minor).
- **Crypto provenance (T5-a)**: `crypto.rs` now wraps RustCrypto `sha2`/`hmac`
  (`crates/server/src/s3/crypto.rs:17-18`); `cargo deny` is green; correctness pinned to
  FIPS-180-4 / RFC 4231 / AWS vectors. The hand-rolled implementation is gone.
- **Concurrent-DELETE idempotency**: the CAS-conflict → re-resolve → success branch
  (`lib.rs:243-252`) is sound; `concurrent_delete_is_idempotent` (`s3_http_wire.rs:324-360`)
  drives the production path over 64 racing rounds and I could not construct an interleaving
  that yields two removals or a 409.
- **percent-decode key identity** (`mod.rs:282-300`) and **XML error escaping**
  (`mod.rs:306-319`) are correct for the cases I tried, including the trailing-`%XX`
  boundary and the SignedHeaders-name injection vector.

Net: the fix's *demonstrated* floor is solid, but (1) a deterministic gate is recorded red
and must be re-run, (2) the DELETE crash-safety claim overreaches its implementation, and
(3) real-SDK compatibility remains asserted rather than proven — all human calls at sign-off.
