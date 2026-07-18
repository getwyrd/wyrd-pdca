# Brief — issue 505 / sigv4-aws-chunked-trailer-framing

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** sigv4-aws-chunked-trailer-framing
- **Defect:** the SigV4 layer accepts only the classic streaming sentinel
  `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` — `streaming_variant`
  (`crates/gateway-s3/src/sigv4.rs:510-515`) is a deliberately closed set that returns
  `None` for `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER` and
  `STREAMING-UNSIGNED-PAYLOAD-TRAILER`, so `verify` refuses them as
  `AuthError::Malformed` (`sigv4.rs:484-490`), which the wire layer maps to **403**
  (`crates/gateway-s3/src/lib.rs:514-519`); the refusal is pinned by tests at
  `sigv4.rs:1013-1023`. Its own doc comment records why: the `aws-chunked` decoder
  (`streaming::Decoder::next_chunk`) requires an immediate CRLF after the terminating
  zero-length chunk and cannot consume trailer bytes, so admitting the sentinel would be
  a half-accept (`sigv4.rs:501-509`). But modern boto3 / aws-sdk / aws-cli **default** to
  the checksum-trailer framing (`x-amz-checksum-*` + a `-TRAILER` sentinel), so a
  default-configured current SDK PUT gets 403 — the gateway is not S3-compatible for a
  stock client.
- **Success criterion:** a `STREAMING-UNSIGNED-PAYLOAD-TRAILER` PUT and a
  `STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER` PUT — each declaring its trailer
  (`x-amz-trailer`) and carrying an `x-amz-checksum-crc32` trailer (CRC32/IEEE, the
  aws-cli/boto3 default) after the zero-length chunk, the signed variant with valid
  chained chunk signatures — are accepted and round-trip **byte-identical** through GET.
  The accepted algorithm set is at minimum `crc32`, `crc32c`, and `sha256`. Fail-closed
  edges, each asserted: a tampered declared trailer checksum is refused (the object is
  not published); an UNKNOWN `x-amz-checksum-<algo>` the gateway cannot verify is
  refused cleanly (never consumed-and-ignored); a malformed trailer section (bad base64,
  a trailer name not declared in `x-amz-trailer`, or garbage after the trailer block) is
  a 400, never a silent accept. And the
  existing `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` path stays green
  (`sigv4.rs`/`streaming.rs` existing tests unchanged and passing). Demonstrable by
  C4-verify: the shipped test is red on the base (403 today) and green with the patch.
  A default-configured `aws s3 cp` succeeding is supplementary off-Check evidence, not
  the binding criterion.
- **Falsifiability:** RED is demonstrable today on the in-process loopback harness
  (`crates/server/tests/s3_http_wire.rs` pattern): send a PUT with
  `x-amz-content-sha256: STREAMING-UNSIGNED-PAYLOAD-TRAILER` on `origin/main` → 403
  before any body is read. No external service, Docker, or SDK install needed to make
  the criterion go red→green.
- **Invariant to restore:** fail-closed, no-half-accept auth — the codebase's own rule
  (issue #364 carry-forward, iter-6 item 3, cited at `sigv4.rs:462-471` and `:501-509`):
  every sentinel the gateway *admits* must be a framing the pipeline *fully consumes and
  verifies*. Today that invariant is honoured by refusing the trailer variants; the fix
  restores SDK compatibility by making the trailer framing fully consumable — never by
  admitting a framing the decoder cannot verify. A declared trailer checksum that is
  consumed but not validated would be the same half-accept in new clothes.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** one logical change in `crates/gateway-s3`: (a) `streaming.rs` — extend the
  `aws-chunked` decoder to consume the trailer section after the terminating zero-length
  chunk (bounded, fail-closed on malformed framing — NOTE the current decoder simply
  returns after the zero chunk and neither consumes nor rejects remaining bytes,
  `streaming.rs:221-250`, so without explicit boundaries a trailer would be silently
  DROPPED, the half-accept in another form), verify the trailer signature for the
  signed variant (per AWS's published trailing-header string-to-sign, anchored to a
  published worked example where available — the module already pins chunk signing to
  AWS's published example, keep that discipline), and validate the declared
  `x-amz-checksum-*` value against the streamed content; (b) the trailer-mode
  DECLARATION headers are part of the contract, not decoration: consumed trailer names
  must match the request's `x-amz-trailer` declaration, and where the client sends
  `x-amz-decoded-content-length` the decoded byte count must match it; (c) `sigv4.rs` —
  admit the two trailer sentinels in `streaming_variant` / `StreamingContext` only once
  (a) makes them fully consumable, and flip the `:1013-1023` refusal tests into
  acceptance tests.
  Per-chunk signature verification for the signed variant already exists — reuse it —
  and `StreamingContext.signed` already models the framing-only (unsigned) case, its doc
  naming `STREAMING-UNSIGNED-PAYLOAD-TRAILER` explicitly (`sigv4.rs:82-86`): the
  plumbing anticipates both variants; what is missing is trailer consumption in the
  decoder and sentinel admission.
  / out of scope: any change to the object write path or `ObjectGateway` seam (the decoded
  stream still feeds `put_object_streaming` unchanged, `lib.rs:571-577`); persisting the
  checksum as object metadata (that is #503's model; validation here is transient);
  `Content-MD5`; multipart. NOTE for Do on checksum algorithms — most need NO new
  dependency: `x-amz-checksum-crc32c` is covered by the workspace's existing
  `crc32c = "0.6"` (root `Cargo.toml:83`, already used by `crates/chunk-format` — add
  `crc32c.workspace = true`), and `x-amz-checksum-sha256` by the vetted `sha2`. Plain
  `x-amz-checksum-crc32` (IEEE — what default aws-cli/boto3 send) is a different
  polynomial the `crc32c` crate does NOT compute: prefer a small table-driven in-tree
  implementation; a new crate (e.g. `crc32fast`) would trigger **Wyrd's ADR-0003
  three-test audit + `deny.toml` allowlist and is a human-only sign-off item**
  (INTEGRATION §4) — if genuinely warranted, declare it loudly in build-notes rather
  than slipping it in. `x-amz-checksum-crc64nvme` is out of scope for this slice:
  refuse it cleanly like any unconsumable claim, never half-accept it.
- **Repro instruction:** on `origin/main`, run the loopback gateway
  (`crates/server/tests/s3_http_wire.rs` harness) and send a signed PUT whose
  `x-amz-content-sha256` is `STREAMING-UNSIGNED-PAYLOAD-TRAILER` with an aws-chunked body
  `<hex-len>\r\n<data>\r\n0\r\nx-amz-checksum-crc32:<b64>\r\n\r\n` → 403
  (`Malformed` / "unsupported x-amz-content-sha256"). Equivalent field repro: a current
  default-configured `aws s3 cp` upload against `wyrd s3` fails with 403.
- **External dependencies:** none — the C4 test runs the in-process loopback stack; the
  off-Check `aws s3 cp` confirmation is covered by the registered
  "aws cli (S3 gateway round-trip)" doctor row.
- **Test file:** crates/server/tests/s3_streaming_trailer.rs (a NEW file under a `tests/`
  dir — the shape C4-verify's classifier keys on; unit tests inside `streaming.rs` /
  `sigv4.rs` are welcome additions but do not earn the red on their own)
- **Citations expected:** Do must cite path:line on `origin/main` for every change.
  Composition cues (each MAY be opened, nothing else): the decoder's framing/state
  machine and its fail-closed error mapping — `crates/gateway-s3/src/streaming.rs`
  (esp. `sign_chunk`, pub at `:98`, for constructing valid chained signatures in the
  test); the closed-set classification to extend — `crates/gateway-s3/src/sigv4.rs:462-515`;
  the wire-level test harness to mirror — `crates/server/tests/s3_http_wire.rs`
  (its stock-SDK streaming-PUT test is the direct peer; its helpers are PRIVATE to that
  test crate — adapt/duplicate them into the new file, do not import).
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log --all`
  over `crates/gateway-s3/` and grep for `TRAILER`/`trailer` across all branches: the
  trailer variants have only ever been *refused* (the #364-carry-forward closed-set
  hardening); no branch or PR implements trailer consumption. `streaming.rs`'s module doc
  (`:9-13`) already documents the trailer framing shape, unimplemented.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether to accept the unreproduced deny leg — focused red→green and the workspace fmt/clippy/build/test suite passed, but `cargo deny check` could not acquire the read-only advisory-database lock, so the full asserted gate was not independently reproduced.; T4 Contribution — Decide whether closed/rejected work duplicates this contribution — affected-path merged/all-ref history found only the earlier gateway implementation and no trailer consumer, but closed/rejected PR state was not mechanically available (`crates/gateway-s3/src/streaming.rs:137`).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C2 Reproduction (red pre-fix) — Decide whether the base-source refusal is sufficient red evidence — `PDCA_TARGET` proves both trailer sentinels were rejected at base `sigv4.rs:1013-1023`, but the read-only shared Git metadata prevented the requested stash/re-run and the configured verifier script was not supplied.; C4 Verification (red→green) — Decide whether to accept the unreproduced red and deny legs — the focused wire suite passed 10/10 and fmt/clippy/build/workspace tests passed, but stashing was blocked by read-only Git metadata and `cargo deny check` could not lock the read-only advisory database; the supplied `run-verify.sh` oracle was unavailable (`crates/server/tests/s3_streaming_trailer.rs:346`).; T4 Contribution — Decide whether closed/rejected work duplicates this contribution — affected-path all-ref history found only the original S3 gateway commit and no trailer consumer, but closed/rejected PR state was not mechanically available (`crates/gateway-s3/src/streaming.rs:390`).
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C2 Reproduction (red pre-fix) — Decide whether direct base-source rejection is sufficient red evidence — a scratch checkout passed the base test proving both trailer sentinels are refused, but the requested exact wire-test stash/re-run oracle was not supplied (`crates/gateway-s3/src/sigv4.rs:1104`).; C4 Verification (red→green) — Decide whether to accept the unreproduced exact red and deny legs — patched wire tests passed 10/10 and fmt/clippy/build/workspace/doc tests passed, but `run-verify.sh` is absent and `cargo deny check` could not lock the read-only advisory database (`crates/server/tests/s3_streaming_trailer.rs:346`).; T4 Contribution — Decide whether closed/rejected work duplicates this contribution — affected-path all-ref history contains only the original S3 gateway commit and no trailer consumer, but closed/rejected PR state is not mechanically available (`crates/gateway-s3/src/streaming.rs:137`).
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
