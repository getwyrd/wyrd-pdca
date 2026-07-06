# Build notes — issue 364 / s3-http-wire-surface (iteration 7)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree base
`5d87cc4`). All edits made in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`);
`path:line` citations are against that tree.

This iteration starts from the **accepted-on-merits v6 floor** (RustCrypto sha2/hmac
provenance; AWS-KAT-pinned SigV4; auth-before-body; streaming PUT/GET; DELETE crash-leak
backstop + orphan-grace reclaim; overwrite reclaim; GET-during-DELETE grace window; the
real `aws-sdk-s3` interop oracle — all of which the iter-6 sign-off said to **keep**) and
addresses **only** the four iter-6 carry-forward items. Everything the iter-6 rationale
marked "ratified / accepted — do not re-litigate" (C5 orphan-ledger design, plaintext-loopback
TLS posture, header-only SigV4 scope, M4 sequencing, the aws-sdk-s3 dev-dep) is left exactly
as it was. I applied `iteration-v6/patch.diff` onto `5d87cc4`, then made the changes below.

## Item 1 (the reject basis) — crate boundary: extract to `gateway-s3`, factor a shared seam

The wire surface was a module inside `crates/server` (`crates/server/src/s3/`). S3 is one of
several planned gateways (§5 building-block view names `gateway-s3`; `14-threat-model.md`
external principals), so it must not calcify in the composition root. Two new crates:

- **`crates/gateway-core`** (`wyrd-gateway-core`) — the **shared gateway seam**
  ([`ObjectGateway`], `crates/gateway-core/src/lib.rs:92`) every client-facing front-door is
  generic over, plus the neutral vocabulary the seam and a wire layer share: `ContentHash`
  (`:46`) and `GatewayError` (`:59`). It is neutral on purpose — no SigV4 / aws-chunked /
  bucket words leak in — so a *second* front-door (`gateway-nfs`, …) implements the same seam
  without depending on the S3 crate. Deps: `wyrd-traits`, `bytes`, `futures-util`.
- **`crates/gateway-s3`** (`wyrd-gateway-s3`) — the S3 wire surface moved out of server
  (`crypto.rs` / `sigv4.rs` / `streaming.rs` / `lib.rs`, formerly `mod.rs`). `S3Gateway<G>` is
  now generic over `G: ObjectGateway` (`crates/gateway-s3/src/lib.rs:112`), naming **no**
  concrete backend. Deps: `wyrd-gateway-core`, `wyrd-traits`, `axum`, `hmac`, `sha2`, `tokio`,
  `tokio-stream`, `futures-util`.

**Breaking the coupling that forced "keep it in server".** `Gateway::put_object_streaming`
used to take `sigv4::PayloadHash` (an S3 type), so server could not compile without the s3
module — a cycle. I moved the payload-integrity concept to the neutral `ContentHash`
(`Expected(hex)` / `Unverified`): `Gateway` now implements `ObjectGateway`
(`crates/server/src/lib.rs:182`) mapping the seam onto its write/read/delete paths, and the
S3 handler converts `PayloadHash → ContentHash` at the edge
(`crates/gateway-s3/src/lib.rs:225-255`). Server's `HashingSource` now hashes with `sha2`
directly (`crates/server/src/lib.rs:24`), so server drops its direct `axum`/`hmac` deps
(those live in `gateway-s3` now) and `GatewayError` is re-exported from `gateway-core`
(`crates/server/src/lib.rs:34`) for source-compatible callers. Dependency direction:
`server → gateway-s3 → gateway-core` and `server → gateway-core` — **no cycle**. Composition
root unchanged in spirit: `cli::cmd_s3` still binds the listener (`crates/server/src/cli.rs`).

The brief's integration test stays at `crates/server/tests/s3_http_wire.rs` (its named path):
it composes a **real** `Gateway` (redb + fs + mem-coord), which only `server` wires (ADR-0010
composition root), and drives it through the now-external `wyrd_gateway_s3::{S3Gateway, sigv4,
streaming}` + `wyrd_gateway_core::{ObjectGateway, ContentHash}`. Keeping it in `server`
(rather than `gateway-s3/tests/`) is deliberate — it avoids a dev-dependency cycle
(`gateway-s3` would otherwise dev-depend on `server`) while still exercising the extracted
wire crate end-to-end. `gateway-s3`'s own unit tests (sigv4 / streaming / crypto KATs) travel
with the crate.

## Item 2 — streaming fail-open: bound the declared chunk size before buffering

`aws-chunked` decode buffered a whole chunk (`size` bytes) to verify its signature *before*
any size check (`crates/gateway-s3/src/streaming.rs`, the old `while self.buf.len() < size + 2`
loop), so a header declaring gigabytes was a pre-auth memory-amplification lever (0015:789).
Fix: a `MAX_CHUNK_SIZE` bound (16 MiB, `streaming.rs:57`) checked **immediately after parsing
the header and before the buffering loop** (`streaming.rs:215`); an over-cap declared size is
refused with a framing error → HTTP 400, never read-to-EOF nor a silent truncated 200. 16 MiB
is far above any stock SDK chunk (the real-SDK interop test passes) yet caps a hostile one, and
also removes a latent `size + 2` overflow. A malformed (non-hex) header already errored; a test
pins that too. **Why bound, not "verify before buffering" literally:** a chunk signature is
computed *over* the chunk bytes, so it cannot be verified before the bytes exist — the resident
bound *is* the mitigation, and it fires on the declared size before allocation.

New unit tests (`streaming.rs`): `an_oversized_chunk_header_is_refused_on_the_declared_size`
and `a_malformed_chunk_header_is_refused`. **RED demonstrated:** with the bound neutralised
(`size > usize::MAX`) the oversized test fails with `body ended mid-chunk` instead of the
declared-size refusal; GREEN with the bound.

## Item 3 — trailer framing: no half-accept (closed sentinel set)

`verify` accepted **any** `x-amz-content-sha256` starting with `STREAMING-`
(old `sigv4.rs` `starts_with("STREAMING-")`), including framings the decoder cannot de-frame —
a half-accept. Now the classification is a **closed** set (`sigv4.rs`, `streaming_variant`):
the three real aws-chunked sentinels (`…-PAYLOAD`, `…-PAYLOAD-TRAILER`,
`STREAMING-UNSIGNED-PAYLOAD-TRAILER`) map to their signed/unsigned discipline; a literal
64-hex digest is `Signed`; `UNSIGNED-PAYLOAD` is `Unsigned`; **anything else is refused
cleanly** (`AuthError::Malformed`, HTTP 403) rather than half-accepted. The `-TRAILER` variants
are genuinely accepted (their data-chunk signature chain is identical to the non-trailer form;
the post-terminator trailer carries only a checksum, a documented residual — the chunk
signatures already authenticate the body). This also closes a latent bug: a non-hex `Signed`
claim was previously accepted and only failed later at the body-hash compare.

New unit test: `verify_rejects_unsupported_content_sha256_sentinels`. **RED demonstrated:**
restoring the open `starts_with("STREAMING-")` accept makes it fail on an unknown sentinel;
GREEN with the closed set. The real `aws-sdk-s3` round-trip test still passes, proving the
closed set covers what a stock SDK actually emits.

## Item 4 — quarantine the `gateway_lease_expiry.rs` wall-clock flake

The pre-existing mutant regression (`crates/server/tests/gateway_lease_expiry.rs`) compared a
lease stamped from the gateway's *internal* clock read against a *separately-sampled* `started`
— an NTP backward step between the two reads could push a correct `now + ttl` lease under a
tight `>= started` lower bound and flake the gate (the reviewers' "exit 101, no failing test
name"). Fix (`gateway_lease_expiry.rs`): slacken the lower bound by a 20s `SKEW_ALLOWANCE_MILLIS`
and take the upper bound from a `finished` sample. This absorbs a backward clock step of up to
~50s **while still killing both mutants**: the `+ -> -` mutant lands ≥ `2*ttl` (~60s) below the
correct lease (well under `started - 20s`), and `+ -> *` overshoots the upper bound by
millennia. No coverage lost; the flake source (cross-read clock non-monotonicity) is removed.

## Gate

`./engine/xtask.sh ci` (== `cargo xtask ci`: fmt, clippy `-D warnings`, build, whole-suite test
incl. the DST tier + the aws-sdk-s3 interop + the two new gateway crates, `cargo deny`,
conformance) → **all checks passed** in `$PDCA_WORKTREE`. The historically-flaky
`gateway_lease_expiry.rs` ran green in the same pass. `cargo deny` stays green: the two new
crates add **no** external dependency (every dep — bytes/futures-util/axum/hmac/sha2/tokio/
tokio-stream — was already in the workspace graph and vetted).

## NEEDS-HUMAN (for §6 at sign-off)

Standing calls carried from the brief, **not** re-litigated (iter-6 ratified them): SigV4 scope
(header-only, presigned out, minimal auth-failure/not-found error floor — already answered by
the brief); minimal S3 error-code floor; M4 sequencing (target is M4); public-TLS deferral to
#367 (plaintext-loopback-at-Check accepted; the rustls-provider deny.toml/license decision when
TLS is wired); the `aws-sdk-s3` dev-dep (accepted, `cargo deny` clean).

New this iteration:
- **Two new crates (`gateway-core`, `gateway-s3`) in the workspace.** They add no new external
  dependency and `cargo deny` is green, but a workspace-shape change (a new `[workspace.members]`
  entry + the seam-crate factoring) is worth a maintainer glance to confirm the seam's home
  (`gateway-core`) is where they want the shared gateway seam to live long-term vs. folding it
  into `wyrd-traits`.

## Key citations

- Seam: `crates/gateway-core/src/lib.rs:92` (`ObjectGateway`), `:46` (`ContentHash`),
  `:59` (`GatewayError`); implemented at `crates/server/src/lib.rs:182`.
- Wire layer generic over the seam: `crates/gateway-s3/src/lib.rs:112` (`S3Gateway<G>`),
  `:173` (`handle<G>`), `:225-255` (PayloadHash→ContentHash at the edge).
- Item 2: `crates/gateway-s3/src/streaming.rs:57` (`MAX_CHUNK_SIZE`), `:215` (the pre-buffer bound).
- Item 3: `crates/gateway-s3/src/sigv4.rs` (`streaming_variant` / `is_hex_sha256`, the closed
  classification in `verify`).
- Item 4: `crates/server/tests/gateway_lease_expiry.rs` (`SKEW_ALLOWANCE_MILLIS`, `finished`).
- Spec: blueprint:59, 698-699 (S3 front door, byte-identical round-trip); `05-building-block-view.md:132`
  (`gateway-s3` named crate); `07-deployment-view.md:72` (HTTP/S3, SigV4);
  `14-threat-model.md:86` (fail-closed external auth); 0015:789 (OOM cliff / streaming).
