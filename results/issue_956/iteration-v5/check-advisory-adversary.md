# Adversarial review — issue 956, iteration 5

I could not break the routing, the probes or the row reading. Four narrow findings remain: two doc sentences that claim too much, one claim no test backs, and one test that can still cause failures on a busy host. A fifth item asks the human to check the T4 gate. I ran everything on a scratch copy of `$PDCA_TARGET`, using the host's `cargo`.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/response.rs:45-46` still says "A PUT or DELETE success has no body at all, so one that sends a byte is refused at that byte", in the docs of the public `BUFFERED_BODY_BUDGET`. `response.rs:13-14` says the same ("a PUT's or a DELETE's success at nothing, since S3 sends neither with a body"). This diff makes both false: `Probe::CopyObject` is a PUT, and its `200` with `<CopyObjectResult>` is a success (`crates/validate/src/s3/verbs.rs:165`, pinned at `crates/validate/tests/s3_client_smoke_verbs.rs:4651`). Round 4 corrected this sentence in `s3.rs:27-31` and in the architecture doc, but not at its source, and brief (H) covers "the code's docs". The list at `response.rs:44` also leaves out probes and delete results. Fix: limit these sentences to the PUT and DELETE of one object, as `s3.rs` now does. No code changes.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:75-76` says a row's key is compared "exactly as sent once the document's XML escapes are read". That is false for a key holding a carriage return. Failing case against the real gateway: put `cr\rkey`, then call `delete_objects(&["cr\rkey"])`. The client sends `cr&#xD;key`, the gateway deletes the object (HEAD goes 200 → 404) and writes a raw CR in its row, because `xml_escape` (`crates/gateway-s3/src/lib.rs:3803`) does not escape it. The XML parser turns that CR into `\n`. The result then says `rows=[Deleted{key:"cr\nkey"}]`, `keys_without_a_row=["cr\rkey"]` and `rows_not_requested=[Deleted{key:"cr\nkey"}]`, so a key that was deleted looks unaccounted for. `crlf\r\nkey` does the same; `lf\nkey` is fine. The client reads the XML correctly, so the root cause is the gateway, which is out of scope here. In this diff, reword the doc sentence to name line-ending normalisation (XML turning a raw CR or CRLF into LF) and pin the case with a test. Open a separate issue for the gateway.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:31-32` and `docs/design/architecture/05-building-block-view.md:303` say the 64 KiB budget caps "a probe's" body, but no test sends a probe an over-budget body. The budget cases at `crates/validate/tests/s3_client_smoke_verbs.rs:2613-2660` run only the listing and delete verbs, and `BODY_FAULTS` (`:2133`) has no over-budget fault. I confirmed this with a mutation at `crates/validate/src/s3/verbs.rs:475`: turning a probe's `BodyFailed(OverBudget)` into `Err(NoResponse)`, which loses the 200. All 53 tests still passed. Brief (H) forbids a claim the tests do not show, and the case can happen: S3 can answer `ListBuckets` or `ListParts` with more than 64 KiB, and that conforming success comes back as a `200` beside `OverBudget`. Fix: add an over-budget leg and an exactly-the-budget leg for a document probe.

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:657-671` does 1,000 PUTs and 1,000 HEADs on fresh connections, which leaves 1,971 sockets in `TIME_WAIT` from this one test. The other 52 tests in the file leave about 745 in total. A full run costs about 2,700 of this host's 28,232 ephemeral ports (range 32768-60999), so about 10 runs a minute, across all jobs on the host, use them all. After that, `bind("127.0.0.1:0")` fails at `:1882`, `:1754` and in the relay. With 20 parallel loops, 152 of 200 runs failed, and every panic message was `AddrInUse`. That load is extreme, but it is the same "several copies on a loaded host" condition behind round 4's flake, and this cause remains after that fix. The test's own earlier block already samples with `step_by(50)` (`:646`), and sampling the PUTs and HEADs here would still show that the keys are gone. The figures the docs pin come from the `Content-Length`, so they do not need all 1,000 objects to exist.

- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log:10` is the only evidence for the gating T4 row in `check-gates.json:104-112`, which claims a "3x codex" multi-pass review. It is one summary line, and the row took 27.29 s for a 197 KB diff. That is very quick for three deep passes, and the log shows no pass count. I cannot tell whether the passes really ran or the result was cached or empty, because `review-batch.md` is not among my inputs. This is a question about the gate, not evidence against the fix. Please check `results/issue_956/review-batch.md` before relying on "0 blocking".

## Evidence and gates

- `gate-logs/C4-verify.log:60-62` is not a red→green proof. The row says `pass`, but the log says "PASS (green-only)", and the brief had predicted UNVERIFIABLE (exit 77). I ran the required reds myself on `crates/validate/src/s3.rs` and `crates/validate/src/s3/{verbs,document}.rs`. Each one failed the tests the brief names, and the unmodified copy passed 53 of 53:
  - Red 1 (a probe returns only the timeout): `a_probe_answered_200…` failed.
  - Red 3 (the SDK's default endpoint rules): 4 tests failed, including the bucket-name and region tests.
  - Red 5 (drop `<Error>` rows): 4 tests failed.
  - Red 6 (unsent reported as `NoResponse`): 3 tests failed.
  - Red 16 (a deadline reported as `NoResponse`): both probe deadline tests failed.
  - Red 17 (wrong `uploadId`): 3 tests failed.
- The tests exercise the production client, not a copy of it. They call the real `S3Client` against the real gateway, the relay or the scripted endpoint. These further mutations were also caught: `UploadPart` with bytes, `CreateBucket` with a configuration, a wrong part number in `CompleteMultipartUpload`, a copy source without its bucket, `quiet` delete, `Capture::new(true)` on bulk delete, processing instructions no longer refused, no trailing-slash strip on the endpoint path, the `/` discount in `too_long`, and lower-case hex.
- `gate-logs/C5-mutants.log:13-15` is a false alarm for this diff. I ran the `Ok("xyzzy")` mutant on each test binary. `s3_client_smoke_verbs` fails 52 of 53 tests within about 61 s. The hang is in `s3_client_nonconforming_responses`, an existing file this diff does not touch. So the mutant is caught, and the timeout comes from that older test file. I ran only this mutant. The other two change the same scheme check at `s3.rs:515`, and I expect the same result but did not run them.
- Round 4's flake fix holds. The refused-connection leg (`crates/validate/tests/s3_client_smoke_verbs.rs:1682-1701`) keeps its port held by a bound socket that is not listening, and no test in the file frees a port any more. 60 of 60 runs passed with 6 parallel loops under 40 CPU hogs. To stay within the port limit, those runs skipped the three tests whose names contain `a_bulk_delete`, so they do not show that those three are free of timing flakes.

## Tried and could not break

- **Bucket names on the client itself:** `--x-s3`, `--xa-s3`, `--op-s3`, `arn:` and `a/b` as the client's own bucket, on both IP and host-name endpoints, through all 12 probes plus `DeleteObjects`, HEAD, `ListObjectsV2` and DELETE. All 192 requests were exactly one request, signed for `s3`, sent to the configured host.
- **The overlong-key guard (`crates/validate/src/s3/verbs.rs:131-159`):** I read the SDK's encoding set, and it matches `path_segment` exactly. No panic turned up.
- **Inputs that cannot be sent:** an empty or white-space key, an empty upload id, a copy source holding `\n` or NUL, and NBSP or U+3000 as a bucket name. All are `RequestNotBuilt` with nothing sent.
- **Inputs that can be sent:** `\n` and NUL in a bucket, key or upload id are percent-encoded and sent as one request.
- **Bulk delete edge cases against the gateway:** no keys, an empty key, a control character, U+FFFE, or 1,001 keys all come back as the gateway's 400 with the status kept. Duplicate keys, a 1,025-byte key and XML-special keys all read correctly.
- **Odd `<DeleteResult>` documents:** CDATA white space at the root, a row in a foreign namespace, an attribute on a row, and `<Code>` or `<Error>` nested inside a `<Deleted>` are all read as `Deleted`. The docs say a row's other fields are passed over, so I am not raising these. No real server sends them.
- **SDK citations:** the lines I spot-checked match `aws-sdk-s3` 1.155.0, `aws-smithy-runtime-api` 1.19.2, `aws-smithy-http` 0.64.3 and `http` 1.4.2. The sizing arithmetic also holds: 62,115 + 41×83 = 65,518, and + 42×83 = 65,601, on either side of the 65,536 budget.
- **Brief scope:** `lib.rs`, `Cargo.toml`, `deny*.toml` and the three older test files are not in the diff.

## Small gaps, not worth a rebuild alone

- `crates/validate/src/s3.rs:552`: taking `~` out of the unreserved set survives all 53 tests. No bucket name in `MORE_FORBIDDEN_BUCKETS` (`crates/validate/tests/s3_client_smoke_verbs.rs:161`) holds a `~`. If another round happens, add one row, such as `("a~b", "a~b")`.
- `crates/validate/src/s3.rs:194`: removing `.disable_s3_express_session_auth(true)` survives. The code comment says this is only a guard against a future SDK, so no test can show it today. I am not raising it.

## A side effect of my testing

My 20-loop run filled the host's `TIME_WAIT` table (155,490 sockets at about 13:16 host time). Other jobs that hit `AddrInUse` on this host in the following minute or so may have been caused by that run, not by their own code.

<!-- pdca:leaf-complete -->
