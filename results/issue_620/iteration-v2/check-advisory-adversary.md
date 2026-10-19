# Adversarial review — issue 620 (S3 1,024-byte object-key limit)

I tried to refute the evidence, the PUT fix, the bulk-delete bound and the gate verdict. I could not break the PUT side. Two items need a human decision. Neither is a defect the builder can fix by rebuilding this patch.

## Evidence: reproduced and holds up

- `crates/server/tests/s3_key_length_limit.rs:252-398`: I re-ran the red→green proof myself in a scratch copy. With `crates/gateway-s3/src/lib.rs` reverted to base and the test kept, 4 of 5 tests fail (`200` where `400` was expected). Leg 4 passes on both sides, as the brief intends. With the patch, all 5 pass. The test drives a real loopback server through signed raw HTTP, so it exercises the production router. It is not a copy of the production logic.
- `crates/gateway-s3/src/lib.rs:1828`, `:1838`, `:2270`: I tried five plausible wrong fixes, and each one turns the wire test red: measuring `bucket/key` instead of the key, measuring the percent-encoded wire key, counting characters on PUT, counting characters in the bulk-delete parser, and checking the raw (undecoded) bucket for `/`. The test is not a tautology, and the round-1 gap (leg 3 used only ASCII keys) is closed by the `€` entry at `s3_key_length_limit.rs:319`.
- Bypass attempts that failed:
  - `PUT //b/<'z' × 1025>` → `400`.
  - `PUT /b/<'%FF' × 342>` → `400 KeyTooLongError`. Lossy decoding stores 342 × U+FFFD, which is 1,026 bytes, so the size the check measures equals the size that would be stored.
  - The only calls that create objects are the three `put_object_streaming` calls at `lib.rs:1866`, `:1879` and `:1891`, and all three sit after the check.
  - `POST /b%2Fa?delete` still deletes `b/a/<'x' × 1023>`. That is a delete, not a create, so it is consistent with the invariant.

## Findings

- NEEDS-HUMAN [human] — Refusing the whole bulk-delete request (`crates/gateway-s3/src/lib.rs:2270`, `:2114`) makes bucket-emptying tools fail on any batch that contains a legacy over-limit key. Concrete case, run as a probe against the patched tree: I seeded `b/<'a' × 1025>` through `Gateway::put_object` and PUT `k1`, `k2`, `k3`. ListObjectsV2 returns all 4 keys, including the long one. A `DeleteObjects` built from that listing answers `400 KeyTooLongError`, and `k1`, `k2` and `k3` all survive. Any client that lists keys and then deletes them in batches (`aws s3 rm --recursive`, `mc rm -r`, `rclone purge`) fails for every 1,000-key batch holding such a key, not just for that one key. The object itself is not stranded, because single-object `DELETE` still removes it (leg 4), so the brief's invariant holds as written. The patch does what criterion (3) asks. Whether whole-request refusal is the right design, or whether a per-key `<Error><Code>KeyTooLongError</Code>` row (or no bound at all) would be better, is a brief-level call. This probably overlaps the round-1 finding already held for the human ("moot if the human drops the bulk bound").
- NEEDS-HUMAN [human] — The only failing gating row, C4 `cargo xtask ci`, fails on `cargo deny --all-features ... check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls v0.21.12` (`gate-logs/C4-ci.log:3475-3557`, `Cargo.lock:3294-3295`). The patch touches only `crates/gateway-s3/src/lib.rs`, the new test file and the architecture doc. It changes no `Cargo.toml` or `Cargo.lock`. Every earlier stage in that log passed: fmt, clippy `--all-targets`, build, the full test run including the 3 new unit tests and the 5 wire tests, typos, and docs-check. No rebuild of this patch can clear the row. A dependency bump or a recorded advisory exception has to be decided separately.
- Non-blocking wording fix, not tagged: `docs/design/architecture/05-building-block-view.md:137` says "The limit is enforced only where an object is created." Two sentences later the same paragraph says bulk `DeleteObjects` refuses an over-limit key. The code comment at `crates/gateway-s3/src/lib.rs:493` gets this right ("Enforced where an object is CREATED … and per entry in a bulk DeleteObjects body"). Fold the correction into whatever edit the first finding produces. It is not worth a rebuild on its own.

I found no unwarranted claim in `check-gates.json`. C4-verify (4 red, then green), C5 (11 of 12 mutants caught, 1 unviable) and the 94.7% diff coverage all match what I reproduced or what the logs show. The `overall: fail` comes only from the `rustls` advisory row.

<!-- pdca:leaf-complete -->
