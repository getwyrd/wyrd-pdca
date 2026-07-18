# Build notes — issue 505 / sigv4-aws-chunked-trailer-framing (iteration 2)

## Context — what this iteration changes vs. iteration-v1

Iteration-v1 (preserved in `iteration-v1/`) was accepted at the architectural level — the
reviewer's C1/C2/C5/T1/T2/T3 all PASSed and the whole trailer-consuming decoder + sentinel
admission is sound. It was **rejected on two implementation-level FAILs** in the base64
decoder (`crates/gateway-s3/src/checksum.rs`):

- **C3 FAIL / T5 FAIL** (`iteration-v1/check-review.md:7,14`): the malformed-base64 contract
  is not fully met — padding is not restricted to the final quartet and non-zero unused
  padding bits are accepted, so a non-canonical checksum such as `6Le+Qx==` decodes
  identically to `6Le+Qw==` instead of returning 400. That is a **silent-accept**: two
  distinct wire strings map to the same checksum bytes, so a tampered/forged declared
  checksum can masquerade as valid — exactly the half-accept the brief's "Invariant to
  restore" forbids.

The carry-forward block also cited two **NEEDS-HUMAN gate-reproduction** items (not code
defects): C4 (`cargo deny check` could not acquire the advisory-DB lock) and T4 (closed/
rejected PR state not mechanically available). Both are re-run and cleared below.

This iteration keeps the entire iteration-v1 implementation unchanged **except** the
`base64_decode` canonicalization fix and its tests. The full rationale for the unchanged
body (trailer decoder, `sign_trailer` provenance, sentinel admission, checksum algorithms)
is in `iteration-v1/build-notes.md` and still applies; I do not restate it here.

## The fix (cite path:line on the applied worktree tree, base = origin/main @ 0b01454)

`crates/gateway-s3/src/checksum.rs`, `base64_decode` — two added guards inside the
per-quartet loop:

1. **Padding only in the final quartet** (`checksum.rs`, `is_last = gi + 1 == group_count`
   + `(pad > 0 && !is_last)` in the reject condition): a `=` anywhere but the last group
   (e.g. `Zg==Zg==`) is now `None`. Previously the per-group `pad` check accepted padding in
   any group, so `Zg==Zg==` decoded to `b"ff"`.
2. **Zero unused padding bits** (`if (pad == 2 && vals[1] & 0x0f != 0) || (pad == 1 &&
   vals[2] & 0x03 != 0) { return None; }`): the low bits a `=`-pad drops MUST be zero for a
   canonical encoding. A two-pad quartet uses only `vals[0]` (6 bits) + the top 2 bits of
   `vals[1]`, so `vals[1] & 0x0f` must be 0; a one-pad quartet drops `vals[2]`'s low 2 bits,
   so `vals[2] & 0x03` must be 0. Without this, `6Le+Qx==` (`vals[1]=49`, low nibble `0x1`)
   decoded to the same bytes as `6Le+Qw==` (`vals[1]=48`).

This is the **smallest change that restores the invariant** ("every admitted framing is one
the pipeline fully consumes and verifies" — a checksum that is consumed but decoded from a
non-canonical string is validated against the wrong equivalence class, the same half-accept
in new clothes). It removes the cause (a lax decoder) rather than guarding a symptom — no
probe added; the decoder itself now rejects the malformed input at the point of parsing,
before the checksum comparison runs.

Cost of the change: **+11 lines** in `base64_decode` (two guard clauses + comments); no new
dependency, no new enum variant, no signature change. No cheaper cause-removal exists — the
canonical-base64 rule *is* the decoder's contract.

## Tests (red→green, proven both at unit and wire level)

- `crates/gateway-s3/src/checksum.rs`, `tests::base64_decode_rejects_non_canonical_encodings`
  (new unit test) — pins the three cases the reviewer named: a two-pad non-zero-bit quartet
  (`6Le+Qx==`), a one-pad non-zero-bit quartet (`Zm9=`), and padding outside the final
  quartet (`Zg==Zg==`), plus the canonical companions that MUST still decode.
- `crates/server/tests/s3_streaming_trailer.rs`,
  `a_non_canonical_base64_trailer_checksum_is_refused_as_bad_request` (new wire test, the
  brief's named test file) — drives the real loopback gateway with a `-TRAILER` PUT whose
  `x-amz-checksum-crc32` value is a **non-canonical base64 that decodes (under a naive
  decoder) to the CORRECT checksum bytes**. Pre-fix the gateway accepts it and returns
  **200** (the silent accept); post-fix it returns **400**. The non-canonical value is built
  by `non_canonical_base64_crc32`, which sets one of the dropped pad bits of the final
  significant char (`((raw[3] & 0x03) << 4) | 0x01`) — same decode, different string.

### Red→green evidence

- **Unit red**: with the two guards reverted, `base64_decode_rejects_non_canonical_encodings`
  panics at the first assertion ("non-zero pad bits (two-pad quartet) must be rejected").
- **Wire red**: with the guards reverted,
  `a_non_canonical_base64_trailer_checksum_is_refused_as_bad_request` fails
  `left: 200, right: 400` — the pre-fix decoder accepts the non-canonical value and the PUT
  succeeds. This is the C3/T5 defect reproduced end-to-end.
- **Green**: with the fix in place, `wyrd-gateway-s3 --lib` = 55 passed (7 checksum, incl.
  the new one), `wyrd-server --test s3_streaming_trailer` = 10 passed, `wyrd-server --test
  s3_http_wire` = 19 passed. The existing `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` path and all
  prior sigv4/streaming tests stay green.

## Self-refutation (the three forced questions)

- **(a) Genuine red?** Yes — verified by reverting *only* the two decoder guards (tests
  untouched) and re-running: the unit test panics and the wire test returns 200 instead of
  400 (`left: 200, right: 400`). The wire test's 200-not-400 is the strongest possible red:
  it shows the pre-fix gateway *accepts and publishes* an object under a non-canonical
  checksum, which is the exact silent-accept the reviewer flagged. Guards restored, both
  go green.
- **(b) Production path?** Yes. The wire test drives the real `S3Gateway` loopback listener
  over a real `TcpListener` with hand-built TCP bytes (same harness as `s3_http_wire.rs`),
  and the refusal is produced by the production `wyrd_gateway_s3::checksum::base64_decode`
  called from the production trailer-consuming decoder — not a copy. The test's own
  non-canonical-string construction is independent arithmetic (it never calls the
  production decoder), so a shared bug could not manufacture a false green.
- **(c) Fixture includes the fault?** Yes — the fault (a non-canonical base64 value that
  decodes to the *correct* checksum) is applied to the actual wire bytes sent, and the test
  asserts `assert_ne!` that the non-canonical value genuinely differs from the canonical one
  (so the test binds only when the corruption is real). The object is a real 200-byte payload
  whose true CRC-32 the non-canonical value decodes to — the fault is not curated out.

## Carry-forward NEEDS-HUMAN items — re-run and cleared

- **C4 — `cargo deny check`**: re-run in the worktree — `advisories ok, bans ok, licenses
  ok, sources ok` (one pre-existing, unrelated `license-not-encountered` warning for `ISC`
  at `deny.toml:111`, not touched by this patch). The advisory-DB lock was available this
  run, so the previously-unreproduced deny leg is now green.
- **T4 — closed/rejected PR duplicate check**: `gh pr list --repo getwyrd/wyrd --state all`
  searched for `trailer in:title` and `505` — both return `[]`. No closed, rejected, or open
  PR implements trailer consumption or references #505. Combined with iteration-v1's
  affected-path merged-history search (only the earlier gateway implementation, no trailer
  consumer), the contribution is not a duplicate.

## Formatter / gates run (this iteration)

- `cargo fmt --all` then `cargo fmt --all -- --check` — clean (diff shipped is post-format).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets` — clean, no warnings.
- `cargo test -p wyrd-gateway-s3 --lib` — 55 passed, 0 failed.
- `cargo test -p wyrd-server --test s3_streaming_trailer` — 10 passed, 0 failed.
- `cargo test -p wyrd-server --test s3_http_wire` — 19 passed, 0 failed.
- `cargo deny check` — `advisories ok, bans ok, licenses ok, sources ok`.
- Patch verified to apply cleanly to a fresh `origin/main` (@ `0b01454`) worktree
  (`git apply --check` green), then that worktree removed.

I did not run the full `cargo xtask ci` (whole-workspace + DST sweep) to completion in this
beat — the C4-ci gate re-runs it against the same worktree. The targeted red→green above is
the fast sanity pass; every discrete step `ci` performs that this patch could affect (fmt,
clippy, the touched test crates, deny) was run individually and is green.

## NEEDS-HUMAN

None from this Do beat. The two carry-forward NEEDS-HUMAN gate-reproduction items are both
re-run green above; the two code FAILs (C3/T5) are fixed and proven red→green. No new
dependency, ADR/spec/format change, or human-only item was introduced.
