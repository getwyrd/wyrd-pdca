# Build notes — issue 505 / sigv4-aws-chunked-trailer-framing (iteration 4)

## The one substantive change vs. iterations 1–3: the RED is now BEHAVIOURAL, not a compile error

Iterations 1–3 were auto-iterated three times. Every substantive Check cell (C1 Spec, C3
Change, C5 Causal adequacy, T1 Structure, T2 Shape, T3 Runtime, T5 Judgment) PASSed. The
only ever-open cells were **C2 / C4 "unreproduced red"** and a **T4 closed-PR** provenance
item — and the C2/C4 recurrence had a real, fixable root cause the prior notes named but did
not remove: **the shipped test's RED leg was a *compile* error, not a *behavioural* one.**

The prior test built its trailer bodies by calling the fix's *new* production surface
(`StreamingContext { trailer, .. }`, `streaming::sign_trailer`,
`checksum::ChecksumAlgorithm`, `TrailerDeclaration`) and by pulling a new `crc32c` dev-crate.
So when `C4-verify` reverts production and re-runs the test, the file **could not compile**
— 5 unresolved-symbol errors — and the "red" was the absence of those symbols, not a wire
403. That is exactly the shape a reviewer distrusts and cannot cheaply reproduce: it looks
identical to a test that simply doesn't build.

**Iteration 4 rewrites the test so it drives the production wire path as a pure S3 *client*,
using only public primitives that already exist on `origin/main`** —
`sign_with_payload_hash`, `signing_key_for`, and `wyrd_gateway_s3::crypto::{sha256,
hmac_sha256, hex}` — and computes its own chunk / trailer signatures, CRC-32, CRC-32C and
base64 in-file. It references **none** of the fix's new surface and pulls **no** new
dependency. Therefore it **compiles unchanged against the base tree**, and with production
reverted every trailer PUT reaches the gateway and gets the base **403** — a red an
`assert_eq!(status, 200)` catches at runtime.

Measured through the project's own `C4-verify` oracle (`engine/scripts/run-verify.sh`, the
`pdca.toml` gate) against the shipped `patch.diff`:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test s3_streaming_trailer (fix applied)
  test result: ok. 11 passed; 0 failed; ...
run-verify.sh: RED — cargo test -p wyrd-server --test s3_streaming_trailer (production reverted, test kept)
  test result: FAILED. 2 passed; 9 failed; ...   <-- real HTTP 403s, e.g.
    assertion `left == right` failed: ... must be accepted (was 403 pre-#505)
      left: 403   right: 200
run-verify.sh: PASS — red without the fix, green with it.
```

The 9 RED failures are wire-level (403 where the fix makes 200/400); the 2 that pass on base
are `crc_reference_check_values` (a pure CRC known-answer unit test, variant-agnostic) and
`a_forged_trailer_signature_...` (it asserts 403 + "stored nothing", which the base blanket
refusal happens to satisfy — harmless, and still binding on the green side). This is the
end-result the brief's Falsifiability section asks for verbatim: "send a PUT with
`x-amz-content-sha256: STREAMING-UNSIGNED-PAYLOAD-TRAILER` on `origin/main` → 403".

The production body is **unchanged from the iteration-2 patch** that Check accepted at every
substantive cell (trailer-consuming decoder, `sign_trailer` pinned to AWS's trailing-header
string-to-sign, sentinel admission, in-tree IEEE CRC-32, canonical-only base64, the
`ChecksumMismatch → 400 BadDigest` mapping). I did not reopen a rejected design — there was
none; the rejections were gate-reproduction gaps. I removed the gap at its source.

## The change (cited against origin/main @ 0b01454, the cycle worktree base)

One logical change in `crates/gateway-s3`, plus its wire test in `crates/server`:

- `crates/gateway-s3/src/streaming.rs` — the `aws-chunked` decoder now consumes the trailer
  section after the terminating zero-length chunk (`consume_trailer`, `MAX_TRAILER_SIZE`-
  bounded), verifies the trailer signature for the signed variant (`sign_trailer`), and
  validates the declared `x-amz-checksum-*` against the streamed bytes. Base decoder
  "returns after the zero chunk and neither consumes nor rejects remaining bytes" was
  `streaming.rs:221-250`; that silent-drop half-accept is now closed fail-closed.
- `crates/gateway-s3/src/checksum.rs` (new) — `ChecksumAlgorithm` (`crc32`/`crc32c`/
  `sha256`), a `RunningChecksum` over the *decoded* bytes, an in-tree table-driven IEEE
  CRC-32, and a canonical-only base64 decoder.
- `crates/gateway-s3/src/sigv4.rs` — `streaming_variant` (base `:510-515`) now admits the
  two `-TRAILER` sentinels, *only once the decoder can fully consume them*; the base refusal
  tests (`:1013-1023`) become acceptance tests; `TrailerDeclaration` is parsed from
  `x-amz-trailer` / `x-amz-decoded-content-length` before any body is read.
- `crates/gateway-s3/src/lib.rs` — `StreamingError::ChecksumMismatch → 400 BadDigest`
  (base 403 mapping was `lib.rs:514-519` for the auth refusal).
- `crates/gateway-s3/Cargo.toml` + `Cargo.lock` — `crc32c.workspace = true` for the gateway
  (crc32c checksum). `crc32c` is **already in the base lockfile** (`Cargo.lock:933`, used by
  `wyrd-chunk-format`), so **no new external crate, no ADR-0003 audit** (INTEGRATION §4).
- `crates/server/tests/s3_streaming_trailer.rs` (new, the brief's named test file) — the
  behavioural client-driven wire test described above.
- `crates/server/tests/s3_http_wire.rs` — one `trailer: None` field added to the existing
  `StreamingContext` literal so the peer streaming test still compiles.

Note vs. iteration 3: `crates/server/Cargo.toml` is **no longer touched** (the `crc32c`
dev-dep is gone — the test computes CRC-32C in-file), and `Cargo.lock` is +1 line, not +2.

## Why guard-vs-cause / minimalism is not the axis here — the Invariant is

The brief names an **Invariant to restore** (fail-closed, no-half-accept auth). The target is
therefore the *smallest change that restores the invariant*, not the smallest diff. The fix
restores SDK compatibility by making the trailer framing **fully consumable and verified**
(decoder consumes the trailer, checks the signature and the checksum against the *streamed*
bytes), never by admitting a framing the decoder cannot verify. A cheaper "consume-and-trust
the declared checksum" would be the same half-accept in new clothes and is explicitly ruled
out by the brief; the extra decoder/checksum code is the invariant-restoring cost, not
avoidable diff.

## Self-refutation (the three forced questions)

- **(a) Genuine red?** YES — and now *behaviourally*, the key improvement. With production
  reverted (test kept), `run-verify.sh` re-runs the shipped test and it **fails 9/11 with
  real HTTP 403s** (`left: 403, right: 200/400`), not a compile error. Reverting the fix
  turns the objective red. The base-source refusal is independently pinned by the flipped
  `sigv4.rs:1013-1023` tests.
- **(b) Production path?** YES. The test drives the real `S3Gateway` over a real
  `TcpListener` with hand-built TCP bytes — the production `sigv4::verify` +
  `streaming::decode` + `checksum` path. The client-side signing/checksums in the test are a
  genuine *peer* (an independent implementation the gateway re-verifies), not a copy of the
  verifier: the gateway recomputes every chunk/trailer signature and every checksum itself,
  and the test's CRCs are anchored to published `"123456789"` check values, not to the
  gateway.
- **(c) Fixture includes the fault?** YES. Each fail-closed test injects the actual fault on
  the wire — a checksum for bytes other than those streamed, a flipped trailer-signature hex
  digit, a `crc64nvme` the gateway cannot verify, a non-canonical base64, an undeclared
  trailer name, trailing garbage, a wrong decoded-content-length — and asserts the specific
  refusal code (400 `BadDigest`, 403, 400), not merely "some error".

## Carry-forward items — resolved this iteration

- **C2 / C4 (unreproduced red / oracle)** — root cause (compile-red) removed; the RED is now
  a wire-level 403 the `C4-verify` oracle reproduces as a runtime test failure. `run-verify.sh`
  run here: **PASS, exit 0** (output above).
- **C4 (deny leg)** — `cargo deny check advisories bans` in the worktree: **`advisories ok,
  bans ok`**, exit 0. No new external crate entered the lockfile (`crc32c` pre-exists at
  `Cargo.lock:933`), so there is nothing new for deny to weigh.
- **T4 (duplicate/closed-PR)** — this is a provenance check the *reviewer* performs against
  GitHub PR state; the builder cannot manufacture that evidence. The affected-path history
  (only the original S3 gateway commit, no trailer consumer) and the brief's own prior-art
  search stand; whether closed/rejected PR state is mechanically reachable is a Check-side
  environment question, not a code defect.

## Commit-readiness (the target's own hooks, which no PDCA gate models)

Run in the cycle worktree over every touched file:

- `cargo fmt --all -- --check` — **clean** (the shipped diff is post-format).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets -- -D warnings` — **exit 0**,
  no warnings.
- `cargo test -p wyrd-gateway-s3 --lib` — **55 passed, 0 failed** (existing streaming/sigv4
  tests including `aws_published_streaming_example` and the flipped `:1013-1023` acceptance
  tests all green).
- `cargo deny check advisories bans` — **ok**.
- `run-verify.sh` (C4-verify) — **PASS** (red→green).

I did not run the whole-workspace `cargo xtask ci` to completion in this beat — the `C4-ci`
gate re-runs it. Every discrete `ci` leg this data-surface change could affect was run
individually and is green; DST/conformance are untouched.

## NEEDS-HUMAN

None from this Do beat. No new external dependency, no ADR/spec/format change. The one
residual is the standing `Validation — fitness-to-purpose` sign-off cell (is in-process
protocol coverage representative of a stock `aws s3 cp`?), which is by-design a human call;
the supplementary `aws s3 cp` field check is the registered "aws cli (S3 gateway round-trip)"
doctor row, off-Check.
