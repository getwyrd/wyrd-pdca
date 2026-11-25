# Adversarial review: issue 956

**Verdict: the routing, probe and row-reading code held up, and I could not break the claimed reds. I did refute one sentence of the docs with a panic, and two other claims in the docs and tests.** Each finding below was run on a scratch copy of `$PDCA_TARGET`, unless it says "by reading".

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:52-54` says a URL longer than an `http` URI allows is `RequestNotBuilt`, but `S3Client::probe` (`crates/validate/src/s3/verbs.rs:343`) **panics** when the key or upload id makes the URL too long. Failing case: `client.probe(&Probe::CreateMultipartUpload { key: "k".repeat(70_000) })` unwinds at `aws-sdk-s3-1.155.0/src/operation/create_multipart_upload.rs:370` (`expect("valid request")`, `InvalidUri(TooLong)`) instead of returning `Err`. The same panic hit `CopyObject`, `UploadPart`, `CompleteMultipartUpload` and `ListParts` with a long key, and `UploadPart` and `AbortMultipartUpload` with a long upload id. Sweeping `ListParts` key length: 65,500 bytes is `RequestNotBuilt`, 65,510 and up panic. The tests only try a long bucket name (`a_request_stopped_before_it_reaches_the_connector…`), which goes through the endpoint and is reported correctly. The inputs come from the caller, and `verbs.rs:34` says the caller chooses the upload id, so no server can trigger this and S3's own 1,024-byte key limit makes it unlikely in a real run. Fix: refuse an overlong key or upload id before calling the SDK, as `RequestNotBuilt`, with a test. `head_object` panics the same way, but that comes from #955, so it is not counted here.

- NEEDS-HUMAN [impl] — No test shows that `delete_objects` keeps its 200 when the body fails on the wire. `docs/design/architecture/05-building-block-view.md:303` says a body that "stalls, is cut, overruns its framing or runs past the budget after a 2xx head is reported beside that 2xx", and that paragraph covers bulk delete, but only stalled, trickled and cut bodies on listings and probes are tested (`crates/validate/tests/s3_client_smoke_verbs.rs:1881`, `:1964`). I changed `verbs.rs` so a body timeout in `delete_objects` returns `NoResult::NoAnswer(Timeout)`, then separately so a cut body returns `NoResponse`. Both changes drop the 200, and all 47 tests still pass. Changing its `Capture::new(false)` to `true` (`verbs.rs:317`) also passes all 47, so a chunked `304` to this POST is untested too. The production code is correct today, through `settle`; the gap is in the tests, and brief (H) says no sentence may claim more than the tests show. Fix: run `BODY_FAULTS` and the `304` case through `delete_objects`.

- NEEDS-HUMAN [impl] — A processing instruction inside `<DeleteResult>` is skipped, while the docs say it is refused. `crates/validate/src/s3/document.rs:235-236` says "Anything else the root holds but whitespace and comments is refused", and `05-building-block-view.md:303` and the test header at `s3_client_smoke_verbs.rs:83-85` say the same. `document.rs:248` skips every non-element, so `<DeleteResult><?x y?></DeleteResult>` comes back as an empty result. A test pins that skipping (`s3_client_smoke_verbs.rs:2912`), while `crates/validate/src/s3/response.rs:481-482` calls a processing instruction "no part of an S3 document". The sign-off's first item was "do not skip unknown children". Fix: refuse it and replace the test, which fits that item, or correct the three sentences.

- NEEDS-HUMAN [human] — The 64 KiB budget refuses ordinary bulk deletes after the server has already done them. Against the real gateway, 1,000 keys of 40 bytes each returned `Err(Answered { status: 200, BodyFailed(OverBudget { received: 70115 }) })`, and the 50 keys I checked afterwards were all gone (HEAD 404). Every row is lost, including any `<Error>` row. 1,000 keys of 32 bytes fit; 800 keys of 60 bytes did not. `crates/validate/src/s3/verbs.rs:299-302` does disclose this, but "many long keys" undersells it: 40 bytes is not long, and S3 allows 1,000 keys of up to 1,024 bytes. The rubric's protocol-input rule asks for budgets sized for the worst-case encoded input, while brief (D) asks for this exact budget, so the builder followed the brief and this is not a builder error. A human should decide whether to raise the budget for this verb, or have #939 keep its batches small and state the limit.

## Claims in `check-gates.json` I would not rely on

- `C4-verify` shows `pass`, but its log (`gate-logs/C4-verify.log`) says "PASS (green-only)": the gate ran no red at all. Its stated reason, that the patch has no separate `tests/*.rs`, doesn't fit a patch that changes `crates/validate/tests/s3_client_smoke_verbs.rs`, and the brief predicted exit 77 here. So the row is not red→green evidence. My own reruns below fill that gap.
- `C5-mutants` shows `fail`, which overstates the problem. The log has three timeouts and no missed mutants. I reran two of the three (the `Ok("xyzzy")` return and the guard set to `false`) with a longer limit: all 47 tests fail by assertion in about 62 s, against the gate's 63 s limit. That is a slow failure, not a hang or a gap. By reading, the third (`||` to `&&`) refuses every scheme too, so it should behave the same.
- `T4-batch-review` reports 0 blocking in 25.8 s for "3x codex" on a 3,162-line diff, after one blocking finding in the last round. `review-batch.md` isn't in my inputs, so I can't say whether that review was real or reused. I make no claim either way.

## Attempted to refute, could not

- **The reds.** All six named reds go red in the expected tests:
  - Red 1 (a timeout replaces the 200): `a_probe_answered_200_keeps_its_200…`.
  - Red 3 (SDK resolver back): four tests, including `bucket_probes_send_every_name…`. Simply deleting the `.endpoint_resolver(PathStyle)` line does not compile (`-D dead-code`), so I swapped in `DefaultResolver`.
  - Red 5 (error rows dropped): three tests, including `a_mixed_delete_result…`.
  - Red 6 (unsent reported as `NoResponse`): `a_request_stopped…` and its #955 peer.
  - Red 16 (timeout becomes `NoResponse`): both probe deadline tests.
  - Red 17 (wrong `uploadId`): three tests.
- **The sign-off items.** Skipping unknown children or stray text, accepting an empty `<Key>`, hiding keys with no row, adding a `<CreateBucketConfiguration>`, sending bytes in `UploadPart`, naming the wrong part in `CompleteMultipartUpload`, and sending `quiet=true` each turn a test red. These tests exercise the production path through the real gateway, the relay or the scripted endpoint. I found no tautologies.
- **Bucket names.**
  - NBSP, U+3000, U+0085, `\n` and `\r\n` are refused as all white space, matching the docs.
  - `\0` and U+200B are sent, percent-encoded.
  - Sweeping the URL length around 65,534 bytes showed "sent" up to the limit and `RequestNotBuilt` after it, never `NoResponse`.
- **Other inputs.**
  - A newline in `source_key` is `RequestNotBuilt`.
  - An odd upload id (`a b&c=d+e/f`) is sent correctly encoded.
  - An endpoint with user info or a fragment still reaches the right host.
  - A row with a comment inside `<Key>` is refused, and CDATA reads correctly.
  - An empty key list, or a single empty key, reports the gateway's 400.

## Smaller notes, no rebuild needed

- Two small claims are not pinned by any test: no row in `MORE_FORBIDDEN_BUCKETS` has a `~` (`crates/validate/src/s3.rs:549`), and an endpoint path of `/p//` is sent as `/p//validate/`, against "one `/` between" at `s3.rs:463` (`:536` strips only one slash). I would not spend a round on either.
- Outside this diff: the gateway's `xml_escape` (`crates/gateway-s3/src/lib.rs:3803`) leaves `\r` unescaped. `delete_objects(&["a\rb"])` deletes the key, but XML line-ending rules turn the echoed key into `a\nb`, so the client reports the key as having no row and the row as not requested. Fed an escaped `&#13;`, the client reads the key correctly, so the client is behaving properly. This belongs in its own gateway issue, not this PR.

<!-- pdca:leaf-complete -->
