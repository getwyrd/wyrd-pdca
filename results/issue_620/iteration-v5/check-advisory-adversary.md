# Adversarial review — issue 620 (S3 object-key length limit)

**Verdict: I could not break the fix.** I re-ran the red→green proof myself and tried nine
alternate spellings of an over-limit PUT. All of them hit the new guard. I found one gap, in
how strong the test is: leg 3 does not catch a bulk-delete length bound set anywhere above
70,000 bytes.

## Findings

- NEEDS-HUMAN [impl] — **Leg 3 only protects keys up to 70,000 bytes.** The brief's
  Invariant and the patch's own doc (`crates/gateway-s3/src/lib.rs:498-500`, `:518-525`)
  promise that bulk delete has **no** length bound below the 8 MiB body cap. The tests only
  check that promise up to the 70,000-byte fixture (`crates/server/tests/s3_key_length_limit.rs:328`,
  `crates/gateway-s3/src/lib.rs:4787`). Concrete failing case: in a scratch copy I added
  `if key.len() > 100_000 { return Err(DeleteRequestError::Malformed); }` right after the
  empty-key check in `parse_delete_request` (`crates/gateway-s3/src/lib.rs:2331-2333`).
  That mutant passed every test: `s3_key_length_limit` (5/5), all 82 `wyrd-gateway-s3` lib
  tests, and `s3_delete_objects` (22/22). Yet it strands every key of 100,001 to 131,072 bytes
  that `wyrd put --key` can write. Those keys are past the `414` limit, so bulk delete is
  their only S3 exit, which is exactly the iteration-4 sign-off defect. The fix is cheap. Over
  the real wire, bulk delete of an 8,388,000-byte key seeded through `Gateway::put_object`
  works today (200, `<Deleted>`, no `<Error>`, gone afterwards), and three such deletes
  (131 KB, 3 MB, 8.4 MB) took about 1 s in total. So leg 3, or the in-crate test where
  `RecordingGateway` makes it nearly free, can add a key near the cap (about 8,388,000 bytes,
  or at least one over 131,072). The brief did accept 70,000 bytes as enough for criterion
  (3), so declining with that recorded reason is also a valid answer.

- **The C4-verify summary in `check-gates.json` overstates the evidence. This is a harness
  wording problem, not a defect in this patch.** The `path_line` says "5 test(s) ran red", but
  `gate-logs/C4-verify.log` shows `2 passed; 3 failed` on the reverted base. 3 red and 2 green
  is the correct, intended result: legs 3 and 4 are designed to pass both before and after the
  fix. The proof is sound; the harness's count is wrong.

- **The T4-batch-review row: I can't confirm the review ran on this diff.** The log is one
  line ("0 blocking …") and the row took 13.8 s for "3x codex" passes. If that was a cache hit
  from an earlier diff, the rubric's "one deep, multi-pass review" has not happened for this
  round. A human can check `review-batch.md`. This is informational only; I have no evidence
  of a problem.

## Refutation attempts that failed (evidence for the fix)

- **Red→green re-run** (scratch copy, toolchain present). With the fix, all 5 tests in
  `s3_key_length_limit` pass. With production `lib.rs` reverted and the test kept, legs 1, 2
  and 5 fail at `crates/server/tests/s3_key_length_limit.rs:247` (`200` where `400` was
  expected), and legs 3 and 4 pass. That matches the frozen log and the brief. The test
  drives the production router over a real loopback socket (not a copy of production code),
  and it seeds through the real `Gateway::put_object`.
- **Alternate spellings of an over-limit PUT** against the guards at
  `crates/gateway-s3/src/lib.rs:1851-1869`:
  - `//b/<1025>` → `400 KeyTooLongError` (`split_bucket_key`, `:590-596`, trims every
    leading `/`, so the key is still measured)
  - `/%62/<1025>` → `400 KeyTooLongError`
  - `?x-id=PutObject` → `400 KeyTooLongError`
  - `/b%2F/<1024 bytes>` → `400 InvalidBucketName`
  - `/b//<1023>` → `200` (correct: a 1,024-byte key `/yyy…`), and `/b//<1024>` → `400`
  - 256 × 😀 (1,024 bytes) → `200`, and one more byte → `400`

  No bypass. The guard runs before the payload `match` (`:1922`), so aws-chunked, signed and
  unsigned bodies all reach it, and before any body byte is read.
- **Measuring the wrong thing.** Each wrong way of measuring the key turns a specific leg red.
  Measuring the raw wire key fails the fully percent-encoded 1,024-byte key and 512 × `é`.
  Counting characters fails 342 × `€`. Measuring `bucket/key` fails the 1,024-byte acceptance.
  `cargo mutants` caught 5 of 5 mutants.
- **The new doc claims.** "The body cap is the ONLY limit on one `<Key>`" (`:518-525`, and
  `05-building-block-view.md`): confirmed up to 8,388,000 bytes, as above. "`414` past 65,534
  bytes" (`:498`, `:522-523`): confirmed. A 65,531-byte key (a 65,534-byte request target)
  gets HEAD `200` / DELETE `204`; a 65,532-byte key gets `414` / `414`. No other write
  surface exists: `crates/proto` has no object-key RPC, and the CLI's
  `crates/server/src/cli.rs:1905` goes through `Gateway::put_object`.
- **Not a finding (older behaviour, out of this diff's scope):** `PUT /b/%FF×342`, which is
  342 raw bytes of invalid UTF-8, gets `KeyTooLongError`. `percent_decode_utf8`
  (`:3365-3383`) decodes lossily, turning each bad byte into the 3-byte replacement character
  U+FFFD. The measured key is the key that would have been stored, and S3 rejects invalid
  UTF-8 keys anyway.

<!-- pdca:leaf-complete -->
