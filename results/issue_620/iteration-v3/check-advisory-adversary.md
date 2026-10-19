# Adversarial review — issue 620 (S3 object-key length limit)

**Bottom line:** I tried to refute the evidence and the fix and could not. The two NEEDS-HUMAN items below are a design question the brief settled on purpose, and a CI failure that sits outside this patch. I found no builder-fixable (`[impl]`) defect.

## The evidence (red→green), re-run independently

- **Red→green reproduces, on the production path.** I copied the target into scratch and ran `cargo test -p wyrd-server --test s3_key_length_limit`. With the fix: 5/5 pass. With `crates/gateway-s3/src/lib.rs` reverted to the base commit and the test kept: `1 passed; 4 failed`. All four fail at `crates/server/tests/s3_key_length_limit.rs:231` (`assert_refused`, which got `200`, not `400`). This matches `gate-logs/C4-verify.log`. The test drives the real `S3Gateway` router over a loopback socket against the real redb + fs backend (`s3_key_length_limit.rs:97-132` in the new file), so it is not a re-implementation that mirrors production. The negative checks can't pass on an empty or broken backend either: leg 1 requires an in-limit control object in the listing (`s3_key_length_limit.rs:260`), and leg 4 brackets the `204` with HEAD `200` before and HEAD `404` after (`s3_key_length_limit.rs:355`).
- **Hand-made mutations are all caught.** Each change below was applied to the fixed `lib.rs`, then I ran both the wire test and the crate's unit tests. Every one turned at least one test in each suite red:
  - bulk parser counts characters, not bytes (`lib.rs:2270`): this was the gap reported in round 1, and the 342 × `€` entry at `s3_key_length_limit.rs:316` now closes it;
  - PUT counts characters (`lib.rs:1838`);
  - PUT measures the flat `bucket/key` name;
  - PUT measures the raw wire `key`;
  - the bucket check reads the raw `bucket`, not the decoded `bucket_name` (`lib.rs:1828`);
  - `>` changed to `>=` on PUT;
  - `>` changed to `>=` in bulk delete.

## Attacks on the fix (could not break it)

- **Other spellings that store an over-limit key under some bucket.** I tried:
  - lowercase `%2f` in the bucket segment: `to_digit(16)` decodes it, so it is refused (`lib.rs:3304-3307`);
  - `//b/k`, which resolves to the same bucket and key;
  - `..` segments: the backend stores flat dirent names (directory-entry records) with no path normalization (`crates/server/src/lib.rs:709-723`), and `..` only lengthens the measured key;
  - invalid UTF-8 such as `%FF`: it is decoded lossily to U+FFFD, which is 3 bytes, and the check measures that same `String`, which is exactly what gets stored (`lib.rs:3298-3316`, `:1776-1778`).

  Because `bucket_name` cannot contain `/`, the first `/` of the stored flat key always falls right after the bucket. So the listing in `crates/server/src/lib.rs:714-723` can only show the measured `key_name` as that object's key. I found no alias.
- **Other ways to create an object.** Only the PUT arm calls `put_object_streaming` (`lib.rs:1866`, `:1879`, `:1891`). Multipart, POST-object and CopyObject are refused earlier (`lib.rs:1786`, `:1754`, `:1809`). The length check runs before the payload-mode match (`lib.rs:1838` vs `:1855`), so signed, unsigned and aws-chunked (streaming-signature) uploads are all covered.
- **Gates the frozen CI run skipped.** The run stopped at the advisory check, so the `cargo xtask conformance` step never ran. I ran it on the patched tree: `xtask conformance: 5 valid + 6 invalid vectors pass`. The patch does not touch `Cargo.lock`, `deny.toml` or `deny-all-features.toml`.

## Findings for the human

- NEEDS-HUMAN [human] — **One stored over-limit key blocks bulk cleanup of up to 999 other keys.** `crates/gateway-s3/src/lib.rs:2270` (with `:2114`) refuses a whole DeleteObjects (S3 bulk delete) request if any entry is over 1,024 bytes. Over-limit objects can still exist: those stored before this fix, or written with `wyrd put --key`. `list_container` lists them without filtering (`crates/server/src/lib.rs:714-741`). Concrete case:
  1. Bucket `b` holds `b/<'a' × 1025>` plus 999 ordinary objects.
  2. A list-then-bulk-delete client (boto3 `bucket.objects.all().delete()`, `mc rm --recursive`) sends one 1,000-key DeleteObjects for that listing page.
  3. It gets `400 KeyTooLongError`, none of the 999 ordinary keys is deleted, and the tool stops.

  Nothing is stranded, because single-object DELETE still works (leg 4). But a bucket holding such a key cannot be emptied with standard bulk tools. Criterion (3) of the brief asks for this whole-request refusal, so changing it is a brief-level decision. The alternatives are a per-key `<Error><Code>KeyTooLongError</Code>` row inside a `200` DeleteResult, or no per-entry bound at all (the 8 MiB body cap already bounds memory). This may duplicate the round-1 finding "moot if the human drops the bulk bound" that was held for the human.
- NEEDS-HUMAN [human] — **C4-ci is red for a reason outside this patch.** The `cargo deny --all-features … check advisories` step fails on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (security advisories) in `rustls v0.21.12`, which comes in through `tikv-client v0.4.0` → `wyrd-metadata-tikv` (`gate-logs/C4-ci.log:3480-3557`; the same at `:6484-6561` on retry). Every earlier stage passed: typos, docs lint/render, guards, fmt, clippy, build, the full test run (including all 5 new wire tests and both new unit tests), machete, and the default `cargo deny check`. This is the third round with the same red, and rebuilding this patch cannot change it. It needs a separate dependency bump or a deny exception. This is not evidence against the fix.

## Claims in `check-gates.json` I would not take at face value

- `C4-verify` says "5 test(s) ran red", but only 4 did. Both the frozen log and my run show `1 passed; 4 failed`, and leg 4 is green on both sides by design (brief, Falsifiability). The patch is fine; the harness summary overcounts.
- `T4-batch-review` reports `0 blocking` after 11.87 s. That is fast for three review passes over a ~38 KB diff, and the round before reported 7 blocking at the same results path. `review-batch.md` is not among my inputs, so I can't confirm the review actually re-ran on this diff rather than reusing earlier output. Worth a glance before sign-off.

<!-- pdca:leaf-complete -->
