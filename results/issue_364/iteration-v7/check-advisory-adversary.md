# Adversarial review — issue 364 / s3-http-wire-surface (iteration 7)

Lens: refute the red→green evidence and the reviewer's verdict; find the input that breaks the fix.
Inputs used: `patch.diff`, `brief.md`, `check-gates.json`; every `path:line` grounded on the
target source at `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`). Advisory only — no gate.

## Evidence I attacked and could NOT refute (reported honestly)

- **The recorded gating gate (C4-ci) does not reproduce as a defect.** `check-gates.json`
  records `C4 Wyrd gate … cargo test --workspace --exclude wyrd-dst failed with exit status:
  101` (gating, fail). I re-ran it on the target and it passed clean (`EXIT=0`, no `FAILED`
  lines across ~60 binaries). `cargo deny check` → `advisories/bans/licenses/sources ok`;
  `cargo clippy -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-server --all-targets` → no
  warnings/errors. This matches the iter-3/4 diagnosis that the red is the historical
  wall-clock flake, not this diff. The patch's quarantine of that flake
  (`crates/server/tests/gateway_lease_expiry.rs:135-158`, +`SKEW_ALLOWANCE_MILLIS`) held over
  3 repeats; `s3_http_wire` (12 tests) green over 2 repeats. **I could not turn the recorded
  red into a real failure.**
- **Attempted to break overwrite-reclaim via empty (identity) placement — does not hold.**
  `commit_chunk_map_superseding` (`crates/core/src/metadata.rs:459`) writes orphan records
  over `prior.chunk_map … chunk.fragments()`; I suspected a pre-M3 empty `placement` vector
  would yield zero fragments and leak the prior object. It doesn't: `fragments()`
  (`metadata.rs:175`) expands the full `0..fragment_count()` index space through the identity
  fallback even for an empty vector, matching GC's `referenced_fragments`. The overwrite-
  reclaim integration test (`crates/custodian/tests/gc_delete_backstop.rs:196`) drives the real
  `commit_chunk_map_superseding` + real `reconcile_step`, so it exercises the production reclaim
  path, not a mirror.
- **Attempted the iter-5 SigV4 fail-closed erosions — fixed.** `verify` now Trimall-collapses
  internal whitespace (`crates/gateway-s3/src/sigv4.rs:433` `trim_all`) and uses the client's
  `SignedHeaders` verbatim in the string-to-sign (`sigv4.rs:439`) rather than re-sorting — so a
  client signing doubled spaces / non-sorted SignedHeaders no longer gets a spurious 403.
- **Attempted the iter-6 streaming fail-open — fixed.** The declared chunk size is bounded
  before any body is buffered (`crates/gateway-s3/src/streaming.rs:215`, `MAX_CHUNK_SIZE`),
  malformed headers → framing error (400), and `-TRAILER` sentinels are a closed accept-set
  (`sigv4.rs:508-511`), not a `starts_with("STREAMING-")` half-accept.
- **Real-SDK oracle is now genuine.** `real_aws_sdk_put_get_delete_round_trips_byte_identical`
  (`crates/server/tests/s3_http_wire.rs:791`) drives the real `aws-sdk-s3` (its own
  signer/canonicalizer/aws-chunked framer) at the loopback listener; nothing on that path calls
  the gateway's own `sigv4`/`streaming`. This closes the iter-2..5 self-reference refutation.

## Concrete residual findings a human should adjudicate

- **NEEDS-HUMAN — Streaming GET truncates silently under a mid-stream fragment fault.**
  `get_object_streaming` (`crates/server/src/lib.rs:238-256`) resolves the chunk map, then the
  spawned reader breaks the loop on the *first* `read_chunk_verified` error
  (`lib.rs:250-253`) — but the HTTP handler has already emitted `200 OK` with no
  `Content-Length` and no `ETag` (`crates/gateway-s3/src/lib.rs:262-266`). Concrete failing
  case: GET of a ≥2-chunk object whose second fragment is unavailable/checksum-fails (a genuine
  D-server fault, not DELETE — which now defers to the grace window) → the client receives
  `200 OK` + a **partial** body and no S3 error code; a single-chunk fault truncates to zero
  bytes. The buffered `get_object` (`lib.rs:168`) surfaces the same fault as a failed GET. A
  correct HTTP/1.1 client can detect the missing terminating chunk as a transport error, but
  the gateway has still promised success and emitted partial object bytes. This is net-new to
  this diff's streaming-GET surface. Is silent-partial-200 acceptable first-deployment GET
  semantics, or must GET buffer-to-first-error / send a trailer error / set Content-Length?

- **NEEDS-HUMAN — the gating record and reality disagree; the record, not the re-run, blocks.**
  `check-gates.json` is `overall: fail` with `C4-ci` red, while `C4-verify` (per-fix
  red→green) is green and my independent full-workspace re-run is green. The deterministic gate
  is what governs sign-off, and it is red. The quarantine widens the lease-expiry bounds by 20s
  (`gateway_lease_expiry.rs:150`), which is a *plausible* absorption of NTP skew but is proven
  only by green runs, not by injecting a backward clock step. The record must be re-run to an
  authoritative green (durably, under the CI host's clock) before accept — do not lean on the
  per-fix `run-verify` green, which only exercises the new test.

## Minor / lower-confidence (not blocking, noted for completeness)

- The real-SDK round-trip (`s3_http_wire.rs:799`, 9000-byte object) is a black-box PUT→GET; it
  proves a stock SDK round-trips but cannot assert *which* wire form (single-shot `Signed` vs
  `STREAMING-UNSIGNED-PAYLOAD-TRAILER`) the SDK chose, so the signed-aws-chunked decode path's
  real-SDK exercise rests on the hand-framed `stock_sdk_chunked_put_round_trips_byte_identical`
  (`s3_http_wire.rs:673`), not the SDK itself. Adequate, but the "stock SDK exercises the signed
  streaming decoder" claim is one step weaker than stated.
- GET responses carry no `ETag` and no `Content-Length` (`gateway-s3/src/lib.rs:262-266`); the
  reviewed SDK tolerates it, but this is an S3-compat gap (some clients require them). Likely
  covered by the brief's "wire encoding is ILLUSTRATIVE" scope — flagging so it is a *decision*,
  not an omission.
