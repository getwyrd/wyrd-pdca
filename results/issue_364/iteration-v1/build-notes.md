# Build notes — issue 364 / s3-http-wire-surface

_Withheld from the reviewer. Rationale, alternatives, and the pre-declared NEEDS-HUMAN
calls for the human at sign-off._

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (the M4 integration
base). Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`; all `path:line`
citations are against that worktree (tip 5d87cc4).

## What the brief asked (Success criterion, BINDING)

A gateway HTTP listener that accepts a **bucket-scoped, SigV4-signed** request and
completes **PUT → GET → DELETE** with GET byte-identical to PUT, and **refuses
unsigned/bad-sig** requests. Wire encoding illustrative; live public-TLS-on-a-deployed-host
is DEFERRED to #367.

## What I built

- **`crates/server/src/s3/` (new module, `pub mod s3;` at `lib.rs:15`)** — the
  S3-compatible HTTP wire surface, generic over `Gateway<M, C, Co>` (holds no concrete
  backend; ADR-0010 wiring stays at the composition root, `lib.rs:44`):
  - `s3/mod.rs` — an axum listener whose fallback handler verifies SigV4 **before any
    gateway work** (fail-closed, §14 `14-threat-model.md:86`), maps `/{bucket}/{key}` onto
    the flat namespace as the single key `"{bucket}/{key}"` (invariant: no directory tree
    invented; ROOT at `lib.rs:28`), and dispatches PUT/GET/DELETE onto the **existing**
    `Gateway::put_object`/`get_object`/`delete_object` (reuse-the-client-path invariant).
  - `s3/sigv4.rs` — real `AWS4-HMAC-SHA256` **header-based** verification + a `sign`
    counterpart; the canonical-request/string-to-sign/signing-key construction is shared by
    `verify` and `sign` so they cannot drift.
  - `s3/crypto.rs` — dependency-free SHA-256 + HMAC-SHA256 (see the crypto call below).
- **`Gateway::delete_object` (`lib.rs`)** — net-new (DELETE did not exist; confirmed absent
  in the pre-fix `lib.rs`). Maps onto a new `wyrd_core::metadata::unlink`
  (`crates/core/src/metadata.rs`) that CAS-removes the dirent **and** inode, mirroring
  `create`/`rename`. Idempotent (absent key → `Ok(false)` → HTTP 204), fail-closed on a
  concurrent-writer race (`GatewayError::Conflict` → 409). Orphaned chunk fragments are left
  as collectable garbage for the custodian GC — the same treatment an overwrite gives
  superseded chunks (`write.rs` sweep note); a chunk-store delete on the delete path is a
  later-milestone concern.
- **`wyrd s3` CLI subcommand (`cli.rs`)** — the **runnable gateway server role** (the
  blueprint's "Stateless S3 front door", blueprint:59; `--s3-listen 0.0.0.0:8080` mirrors
  blueprint:620-623). Composes redb + fs + mem-coordination and serves. This is what #367's
  day-one runbook dials.
- **`crates/server/tests/s3_http_wire.rs`** — the flippable regression: an S3 client drives
  PUT→GET→DELETE over a **real loopback listener**, asserting (a) byte-identical GET of a
  **multi-chunk** object, (b) DELETE removes it, (c) unsigned → 403, (d) wrong-sig → 403
  **and stores nothing**. RED before the module exists (unresolved `crate::s3` import →
  compile failure, demonstrated), GREEN after.

## Red→green evidence

- Demonstrated RED: moving `s3/` aside → `error[E0432]: unresolved import crate::s3`.
- GREEN: `cargo test -p wyrd-server --test s3_http_wire` → 3 passed.
- Unit KATs GREEN: SHA-256 (FIPS-180-4), HMAC-SHA256 (RFC 4231 cases 2 & 4), and the **AWS
  SigV4 test-suite `get-vanilla`** known-answer (`sigv4_get_vanilla_known_answer`) — this
  pins the whole signing chain to AWS's published signature, so the e2e is not
  self-referential even though its client signer shares the production canonicalization.
- No regressions: full `wyrd-server` + `wyrd-core` suites pass; `cargo fmt --all --check`
  clean; `cargo clippy --workspace --all-targets` clean; `cargo deny check licenses bans`
  → `ok`.

## The crypto-provenance call (a decision the human should confirm)

SigV4 needs SHA-256 + HMAC-SHA256. I implemented them **in-crate** (`s3/crypto.rs`, ~150
lines, KAT-pinned) rather than pulling a crypto crate. Concrete reason, not a preference:

- The only SHA-256 providers already in `Cargo.lock` are **`ring`** and **`rustls`**. Their
  licences are **not** in the `deny.toml` allowlist (`deny.toml:25-38` — ISC/OpenSSL-family
  is absent). `cargo deny check` runs inside the gating `cargo xtask ci`, so depending on
  either turns the gate **RED**. (Verified: `cargo tree -i ring` is empty on the default
  build — ring is lock-only via a non-default path, so it is not currently licence-checked;
  promoting it to a built dep *would* pull it into the check and fail it.)
- The RustCrypto path (`sha2` + `hmac`, MIT/Apache — allowlisted) would pass `cargo deny`
  but is a **new external dependency** = an ADR-0003 three-test audit → NEEDS-HUMAN, and it
  churns the workspace `Cargo.toml` + `Cargo.lock` (+~8 transitive crates: digest,
  block-buffer, crypto-common, generic-array, typenum, cpufeatures, …).
- The in-crate route keeps the gate green with **zero** dependency/lock churn and is fully
  machine-checked against published vectors. The cost is ~150 lines of well-specified,
  deterministic hash code on the auth boundary.

**NEEDS-HUMAN (pre-declared, brief §"SigV4 scope"):** confirm the crypto provenance. If you
prefer a vetted crate over in-crate SHA-256, the clean follow-up is `sha2`+`hmac` (add to
the allowlist-friendly deps, swap `crypto::{sha256,hmac_sha256}` for them — the KATs stay as
the guard). I chose in-crate to ship a **gate-green, self-contained** slice; the swap is a
one-function change behind the same tests.

## Alternatives considered / rejected

- **axum vs raw hyper.** Used `axum` (already a `cargo deny`-vetted transitive dep via
  `tonic`; promoting it to a direct dep added **no** new crate to the lock — verified: the
  only `Cargo.lock` delta is the `wyrd-server → axum` edge + axum feature-unification of
  crates already present). Raw hyper 1.x would have been ~2× the handler plumbing for no
  gain. `default-features = false, features = ["http1","tokio"]` keeps it minimal.
- **True streaming bodies (invariant "stream, don't buffer", 0015:789).** The existing seam
  `Gateway::put_object(&self, key, data: &[u8])` (`lib.rs:114`) **buffers by contract** — it
  takes a full slice. Genuine end-to-end streaming needs a core write-path change (a
  `put_object` that consumes an async body), which the brief puts out of scope ("reuse the
  client path, don't reimplement"; "no change to traits"). I bounded the buffered body
  (`MAX_BODY_BYTES = 256 MiB`, `s3/mod.rs`) so a hostile `Content-Length` can't drive the OOM
  cliff, and documented the tension. **NEEDS-HUMAN-adjacent:** streaming PUT is a real
  follow-up gated on a core-seam change, not a defect of this slice.
- **TLS wiring.** The binding criteria at Check don't require TLS (the brief sanctions
  loopback plaintext), and live public-TLS is DEFERRED to #367. Fully wiring rustls here
  would (a) pull a crypto provider — the same licence wall as above (aws-lc-rs adds C code;
  ring is disallowed) → RED gate or a new-dep NEEDS-HUMAN, and (b) need a cert generator
  (`rcgen`, not in-tree) to test headlessly. I instead modelled the **public S3 TLS identity
  as a distinct config type** (`TlsIdentity` in `s3/mod.rs`), explicitly separate from the
  internal step-ca mTLS fabric (invariant "two distinct TLS identities", blueprint:620-623) —
  and left `serve` taking an already-bound `TcpListener` so #367 fronts it with the public
  cert. This honours the "don't conflate" invariant without a licence/dep rabbit hole.

## Known NEEDS-HUMAN (from the brief, carried forward)

1. **Crate boundary** — I landed inside `crates/server` (M0's "combined server, split later",
   ADR-0016), not a new `gateway-s3` crate. This fixed the test path to
   `crates/server/tests/s3_http_wire.rs`. If you want the `gateway-s3` crate that §5:132
   names, the module lifts out cleanly (it depends only on `Gateway` + traits).
2. **SigV4 scope** — header-based `AWS4-HMAC-SHA256`, single static credential, `s3` service.
   **No** presigned-query (out of scope), **no** clock-skew/replay-window enforcement on
   `x-amz-date` (a hardening left for the auth milestone). Confirm this floor + the crypto
   provenance above.
3. **Sequencing** — pinned to the M4 integration branch per the item note (vs. its own
   sequence between M4 and M7). Human call.
4. **Error-code floor** — returns a minimal S3-compatible `<Error>` set (AccessDenied,
   SignatureDoesNotMatch, InvalidAccessKeyId, NoSuchKey, …). The full conformance sweep is
   out of scope (pre-M8).

## Manual validation (for the runnable role / #367 hand-off)

```
# from $PDCA_WORKTREE
cargo run -p wyrd-server --bin wyrd -- s3 \
  --access-key AKIAIOSFODNN7EXAMPLE \
  --secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY \
  --s3-listen 127.0.0.1:8080 --data-dir /tmp/wyrd-s3
# then, with any SigV4 client (awscli/boto3/s3cmd) pointed at http://127.0.0.1:8080,
# region us-east-1: aws s3api put-object / get-object / delete-object round-trips;
# an unsigned curl gets 403.
```
Public-TLS-on-a-deployed-host is observed at #367 (needs the coordination prerequisite,
0015:443-463) — pre-declared off-Check by the brief.
