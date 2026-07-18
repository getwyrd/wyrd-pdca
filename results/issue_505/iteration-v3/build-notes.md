# Build notes — issue 505 / sigv4-aws-chunked-trailer-framing (iteration 3)

## What iteration 3 changes vs. iteration 2

Nothing in the **implementation**. Iteration-v2 (preserved in `iteration-v2/`) was accepted
at every substantive Check cell — C1 Spec, C3 Change, C5 Causal adequacy, T1 Structure,
T2 Shape, T3 Runtime and T5 Judgment all PASSed (`iteration-v2/check-review.md`). The only
open cells were **NEEDS-HUMAN gate-reproduction** items, and every one of them was an
*environmental* limitation of the reviewer's sandbox, **not a code defect**:

- **C2 / C4** — the codex reviewer could not run the red→green oracle: its shared Git
  metadata was read-only (so the requested stash/re-run was blocked) and `run-verify.sh`
  was reported unavailable to it; `cargo deny check` could not acquire the read-only
  advisory-DB lock.
- **T4** — closed/rejected PR state "not mechanically available" to the reviewer.

The brief's carry-forward tells me to *address* these — not to re-open a rejected design.
There is no rejected design here; the approach is sound. So iteration 3 re-ships the exact
iteration-v2 patch **unchanged** and does the one thing the reviewer could not: it runs the
project's own oracle and the blocked gate legs myself, from an environment that can, and
records the concrete results below for the human at sign-off.

The full rationale for the implementation body (trailer-consuming decoder, `sign_trailer`
provenance, sentinel admission, checksum algorithms, the canonical-base64 fix that closed
iteration-v1's C3/T5 FAIL) is in `iteration-v1/build-notes.md` and
`iteration-v2/build-notes.md` and still applies verbatim; I do not restate it.

## The change (cited against origin/main @ 0b01454, the cycle worktree base)

One logical change in `crates/gateway-s3`, plus its wire test in `crates/server`:

- `crates/gateway-s3/src/streaming.rs` — the `aws-chunked` decoder now consumes the
  trailer section after the terminating zero-length chunk (`consume_trailer`,
  `MAX_TRAILER_SIZE`-bounded), verifies the trailer signature for the signed variant
  (`sign_trailer`, pinned to AWS's published `aws-c-auth` trailing-header worked example),
  and validates the declared `x-amz-checksum-*` against the streamed bytes. Base decoder
  "returns after the zero chunk and neither consumes nor rejects remaining bytes" was
  `streaming.rs:221-250` on base; that silent-drop is now closed fail-closed.
- `crates/gateway-s3/src/checksum.rs` (new) — `ChecksumAlgorithm` (`crc32`/`crc32c`/
  `sha256`), a `RunningChecksum` over the *decoded* bytes, an in-tree table-driven IEEE
  CRC-32, and a canonical-only base64 decoder.
- `crates/gateway-s3/src/sigv4.rs` — `streaming_variant` (base `:510-515`, the closed set
  returning `Some(true)` only for the classic sentinel, `_ => None`) now admits the two
  `-TRAILER` sentinels, but *only once the decoder can fully consume them*; the base
  refusal tests (`:1013-1023`) become acceptance tests. `TrailerDeclaration` is parsed from
  `x-amz-trailer` / `x-amz-decoded-content-length` before any body is read.
- `crates/gateway-s3/src/lib.rs` — `StreamingError::ChecksumMismatch` → 400 `BadDigest`
  (base 403 mapping was `lib.rs:514-519`).
- `crates/server/tests/s3_streaming_trailer.rs` (new, the brief's named test file) — drives
  the real loopback S3 gateway over `TcpListener` with stock-SDK trailer bytes.

## Red→green — run through the project's own oracle (the C4-verify gate)

I ran `engine/scripts/run-verify.sh` (the `C4-verify` gate, `pdca.toml:875`) with
`PDCA_BUNDLE` pointed at this bundle. It applies `patch.diff` to a **fresh `origin/main`**
worktree, runs the shipped test with the fix, then reverts production (keeping the test) and
re-runs. Verbatim tail:

```
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.48s
run-verify.sh: RED — cargo test -p wyrd-server --test s3_streaming_trailer (production reverted, test kept)
error[E0432]: unresolved import `wyrd_gateway_s3::checksum` ...
error[E0560]: struct `StreamingContext` has no field named `trailer` ...
run-verify.sh: PASS — red without the fix, green with it.
```

Exit 0. This is the exact oracle the reviewer could not run; it now reproduces the red→green
contract cleanly. **This directly clears the C2/C4 "unreproduced red" carry-forward items.**

### On the *shape* of the RED (read this — it is the honest caveat)

The RED leg is a **compile-red**, not a 403-response-red. That is **inherent**, not a
weakness I chose to leave: the wire test builds valid signed chunk/trailer bodies by calling
the *production* `sign_chunk` / `sign_trailer` and constructing a *production*
`StreamingContext` — exactly as the brief's `Citations expected` directs ("`sign_chunk`,
pub at `:98`, for constructing valid chained signatures in the test"). Those symbols, and
the `trailer` field, do not exist on base, so with production reverted the test cannot
compile. A single test file cannot compile against *both* the base `StreamingContext`
(5 fields) and the fixed one (6 fields) from one struct literal. The alternative — hand-
re-implementing `sign_chunk`/`sign_trailer`/the context in the test so it compiles on base —
is explicitly forbidden ("must still drive the production code, not a copy"): a green run
against a re-implementation would be worse than no test.

The **behavioral** side of the red is nonetheless established, two ways, so the human is not
asked to take the compile-red on faith:

1. The GREEN leg's 10 tests assert the *positive behavior* — `status == 200` and
   **byte-identical** GET round-trip for `crc32`/`crc32c`/`sha256` over both sentinels, plus
   the exact refusal codes (400 `BadDigest`, 403, 400) on every fail-closed edge. None of
   those can pass unless the 403→accept behavior genuinely changed; a mis-wired admission
   that still refused would fail them.
2. The behavioral **403-on-base** is pinned in-tree, independently of this wire test, by the
   base `sigv4` refusal tests at `sigv4.rs:1013-1023` (which this patch flips into
   acceptance) — the reviewer confirmed this on `PDCA_TARGET` in iteration 2's C2. I re-read
   the base source directly this iteration: base `streaming_variant`
   (`sigv4.rs:510-515`) is `Option<bool>` returning `Some(true)` only for
   `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, `_ => None`, so both `-TRAILER` sentinels fall to
   the `AuthError::Malformed("unsupported x-amz-content-sha256 …")` arm (`sigv4.rs:484-490`),
   which `lib.rs:514-519` maps to 403 — a stock trailer PUT is refused before any body is
   read, precisely the brief's falsifiability claim.

## Self-refutation (the three forced questions)

- **(a) Genuine red?** Yes. With the fix reverted (production out, test kept) the shipped
  test fails — `run-verify.sh` re-ran it and it did not pass (5 compile errors), so the
  gate reports "red without the fix, green with it." The red is a compile-red (see the
  caveat above); it is genuine (revert ⇒ fails) and non-vacuous (the file references the new
  production surface, so it cannot silently pass on base). The *behavioral* red is
  additionally pinned by the base `sigv4.rs:1013-1023` refusal tests the patch flips.
- **(b) Production path?** Yes. The GREEN test drives the real `S3Gateway` over a real
  `TcpListener` with hand-built TCP bytes — the production `sigv4::verify` +
  `streaming::decode` + `checksum` path, not a copy. The test's independent checksum oracle
  (a bit-by-bit `crc32_ieee`, the dev-only `crc32c` crate, `crypto::sha256`) is *distinct*
  from the gateway's own table-driven implementation, so a shared bug cannot manufacture a
  false green.
- **(c) Fixture includes the fault?** Yes. Each fail-closed test injects the actual fault on
  the wire — a wrong checksum for bytes other than those streamed
  (`a_tampered_trailer_checksum_…`), a flipped trailer-signature hex digit
  (`a_forged_trailer_signature_…`), a `crc64nvme` the gateway cannot verify, a non-canonical
  base64, an undeclared trailer name, trailing garbage, a wrong decoded-content-length — and
  asserts the specific refusal, not merely "some error." The success fixtures carry real
  multi-chunk objects whose true checksum the declared value must match.

## Carry-forward items — re-run and cleared this iteration

- **C2 / C4 (red evidence + oracle)** — `run-verify.sh` run here: **PASS, exit 0** (output
  above). The oracle the reviewer lacked now reproduces red→green.
- **C4 (deny leg)** — `cargo deny check` in the worktree: `advisories ok, bans ok, licenses
  ok, sources ok`, exit 0. The one warning is a pre-existing, unrelated
  `license-not-encountered` for `ISC` at `deny.toml:111`, untouched by this patch. The
  advisory-DB lock was available this run.
- **T4 (duplicate/closed-PR check)** — `gh pr list --repo getwyrd/wyrd --state all --search
  "trailer in:title"` → `[]`, and `--search "505"` → `[]`. No open, closed, or merged PR
  implements trailer consumption or references #505. Combined with the brief's affected-path
  history search (only the original gateway commit, no trailer consumer), this is not a
  duplicate.

## Commit-readiness (the target's own hooks, which no PDCA gate models)

Run in the cycle worktree over every touched file:

- `cargo fmt --all -- --check` — clean (the shipped diff is post-format).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets -- -D warnings` — exit 0,
  no warnings.
- `cargo deny check` — ok (above).
- `run-verify.sh` (C4-verify) — PASS.

I did not run the whole-workspace `cargo xtask ci` (fmt+clippy+build+test+DST+deny+
conformance) to completion in this beat — the `C4-ci` gate re-runs it against the same
worktree. Every discrete step of `ci` this patch could affect was run individually and is
green; the DST/conformance legs are untouched by this data-surface change.

## NEEDS-HUMAN

None from this Do beat. The three carry-forward NEEDS-HUMAN gate-reproduction items are all
re-run green above. No new dependency (crc32c was already workspace-vetted; the IEEE CRC-32
is in-tree precisely to avoid an ADR-0003 audit), no ADR/spec/format change, no human-only
item introduced. The one residual judgment the human still owns — the standing `Validation —
fitness-to-purpose` cell (is in-process protocol coverage representative of a stock `aws s3
cp`?) — is by-design a sign-off call; the supplementary `aws s3 cp` field check is the
registered "aws cli (S3 gateway round-trip)" doctor row, off-Check.
