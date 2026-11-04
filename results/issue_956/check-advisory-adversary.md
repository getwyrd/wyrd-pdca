# Adversarial review: issue 956, iteration 6

**Result: I could not refute the fix.** I found no failing input and no test that passes for the wrong reason. There are no `NEEDS-HUMAN` items. The bullets below are one test gap, three notes on how far the gate evidence can be trusted, and what I tried.

I ran everything on a scratch copy of the patched tree, using the pinned toolchain. To let mutated code compile, that copy adds one line to `lib.rs`, `#![allow(dead_code, unused)]`. This does not change behaviour. I did not re-run `cargo xtask ci`; for that gate I rely on `gate-logs/C4-ci.log`.

## Findings (none blocks; none needs a human)

- **Test gap, `~` is not pinned:** `crates/validate/src/s3.rs:552`. Removing `b'~'` from `path_segment`'s unreserved set passes all 55 tests, because no bucket name in the tables holds a `~` (`crates/validate/tests/s3_client_smoke_verbs.rs:168`). It is the only survivor of the 38 mutations I ran. A server should decode `%7E` to the same name, so I see no wrong behaviour. `crates/validate/src/s3/verbs.rs:51` does claim the exact set, so one more table row would close the gap if another round happens. It does not justify a round alone.
- **The C4-verify row claims more than its log shows:** `check-gates.json`, row `C4-verify`, is titled "red pre-fix, green post-fix" and says `pass`, but `gate-logs/C4-verify.log` ends in `PASS (green-only)`, so that gate proved no red. The brief's verification posture also predicted `UNVERIFIABLE (exit 77)`, not this result. The fix is unaffected: I ran the reds myself (below).
- **The C5 `fail` row looks like timeouts, not surviving mutants:** `crates/validate/src/s3.rs:510` and `:515`. The log lists 3 timeouts and no missed mutants. I re-ran one of the three, the match guard at `:515` set to false. It failed 54 tests in 20 s, so it is caught, and the gate's 60 s limit probably cut the run short on a loaded host. I did not re-run the other two.
- **Ports can still run out under heavy parallel runs:** `crates/validate/tests/s3_client_smoke_verbs.rs:1947`. Carry-forward item 4 is done, but one run still leaves 826 sockets in `TIME_WAIT` (measured from a count of 1 before the run). With the 28,232 ports here, that is about 34 runs a minute across the host. With 16 copies at once, 48 runs in rounds 1 to 3 all passed. Rounds 4 and 5 failed, with `AddrInUse` as the only panic cause. That is 80 runs in 12 s, with another issue's `cargo xtask ci` also on the host, far above normal use. Each socket is a real test request, so I would not change this now.
- **T4 pass, not verified:** `gate-logs/T4-batch-review.log` reports 0 blocking in 31.94 s for three review passes over a 203 KB patch. That is fast for a multi-pass review. I cannot tell whether it was a full review, because `review-batch.md` is outside my inputs. This is a doubt about the evidence, not a defect in the code.

## Evidence I tried to break

- **The brief's six reds all go red, in the tests the brief expects:** each row mutates production code at `crates/validate/src/s3.rs:189` or in `crates/validate/src/s3/verbs.rs` and `document.rs`.
  - Red 1, a stalled probe returns only the timeout: `a_probe_answered_200_keeps_its_200…` fails.
  - Red 3, SDK default endpoint resolution: 4 tests fail, including `bucket_probes_send_every_name…`.
  - Red 5, `<Error>` rows dropped: 4 tests fail, including `a_mixed_delete_result…`.
  - Red 6, unsent reported as `NoResponse`: 3 tests fail, including `a_request_stopped_before…`.
  - Red 16, a deadline turned into `NoResponse`: both probe deadline tests fail.
  - Red 17, a different `uploadId`: 3 tests fail, including `every_unsupported_operation…`.
- **The fixes for iterations 2 to 5 are real:** each of these turns a test red:
  - a probe's `OverBudget` turned into `NoResponse`
  - a delete's body failure turned into `NoAnswer`
  - `Capture::new(true)` at `crates/validate/src/s3/verbs.rs:375`
  - a processing instruction skipped at `crates/validate/src/s3/document.rs:257`
  - unknown children skipped at `crates/validate/src/s3/document.rs:275`
  - an empty `<Key>` accepted at `crates/validate/src/s3/document.rs:287`
- **The tests use the production path:** the 55 tests drive the real `S3Client`, through the real gateway, the relay or the scripted endpoint. Mutations of `reads()`, `success_body()`, `URI_ROOM`, the request body, `part_number`, `x-amz-copy-source`, `quiet`, percent-encoding case and the endpoint path join each turned tests red. In total, 37 of 38 mutations were caught.
- **Flakiness:** the 48 runs in rounds 1 to 3 were all green, so the refused-port flake from iteration 4 did not recur (`crates/validate/tests/s3_client_smoke_verbs.rs:1752`).

## Inputs I tried to break the fix with

- **Panics in `Probe::too_long`** (`crates/validate/src/s3/verbs.rs:131`): I got no panic with:
  - `CopyObject` and `CreateMultipartUpload` with a key exactly at the limit
  - `UploadPart`, `CompleteMultipartUpload`, `AbortMultipartUpload` and `ListParts` with an upload id exactly at the limit
  - keys `/lead` and `//`

  The SDK's encoding set matches `path_segment` (`aws-smithy-http` 0.64.3 `src/urlencode.rs:9-42`), so the count cannot come up short for any byte.
- **Bucket names changed on the way out** (`crates/validate/src/s3.rs:549`): the SDK only tests for white space and does not trim the value. ` spaced ` arrives as `/%20spaced%20/`, a leading non-breaking space as `/%C2%A0nbsp/`, and `a\0b` as `/a%00b/`.
- **Rows lost after the server acted** (`crates/validate/src/s3/document.rs:243`):
  - Keys with U+0001, U+000B, NUL, U+FFFE or U+FFFF: the gateway answers `400 MalformedXML` and deletes nothing, sibling keys included. The client reports the 400.
  - Keys with tab, LF, NEL, U+2028, U+007F, U+0080, a literal `&#13;`, `]]>`, or leading and trailing spaces: each comes back exactly as sent, with its row.
  - Only the carriage return rewrites a key, which is documented and tested.
- **Empty and white-space keys:** `Probe` with key `""` or `" "`, or with upload id `""`, is `RequestNotBuilt` and sends nothing. `crates/validate/src/s3.rs:53-57` does not list these cases, but the report is correct, so "not sent" and "server refused" stay separate. Bulk delete does send `" "`, which is right for a request-body key.
- **Non-ASCII copy source:** `source_key: "é"` is sent as raw bytes and the gateway answers 403. `crates/validate/src/s3/verbs.rs:70-71` says such a key must come encoded, so this is the documented behaviour.
- **Doc figures in `crates/validate/src/s3/verbs.rs:350-359`:** 62,115, 70,115, 65,518 and 65,601 all follow from the stated row sizes, and the tests pin them. The 32-byte figures and the 40-byte 70,115 use real gateway `Content-Length` values.
- **Scope:** the patch does not touch `lib.rs`, `Cargo.toml` or `deny*.toml`. The other three `s3_client_*` test files are unchanged.
<!-- pdca:leaf-complete -->
