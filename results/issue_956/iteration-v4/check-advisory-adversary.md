# Adversarial review — issue 956

I could not break the production code. I found one flaky new test and one set of doc sentences that the new verbs made wrong. I ran everything in a scratch copy of `$PDCA_TARGET`, so the toolchain was available and this verdict is not provisional.

## Findings

- NEEDS-HUMAN [impl] — `crates/validate/tests/s3_client_smoke_verbs.rs:1683-1686`: the "refused connection" leg of `a_probe_that_reached_the_connector…` is flaky. It binds a listener, drops it, then assumes the freed port stays closed while other listeners are binding ports. Another test can take that port. Then the probe gets a real answer, and the other test loses the connection its script was waiting for.
  - **Seen:** 7 failed runs out of 140, all with several copies of the test binary running at once on a loaded host (16 runs: 1 failed; 54 runs: 0 failed; 70 runs: 6 failed).
  - **Failure message (line 1689):** `a refused connection: Ok(Answer { head: Head { status: 200, fields: [("x-amz-request-id", … "get-framing")…`. Other runs were answered with the ids `head-body` and `region-case`.
  - **Innocent victims:** in the last batch, the three tests that own those ids failed in the same rounds with `Connection reset by peer`: `a_get_2xx_whose_framing…`, `a_head_answered_with_a_body…` and `the_region_is_signed_as_given…`.
  - **Not measured:** the rate for one binary alone, which `cargo test` runs one at a time. It should be much lower, but 52 other tests in that binary still bind ports in parallel.
  - **Fix:** the repo already has one at `crates/validate/tests/s3_client_roundtrip.rs:1426-1432`, whose comment says "bound but not listening … no other test can take the port meanwhile". I tried that form here: it gives `NoResponse` ("Connection refused", os error 111) and a second bind on the port fails. `Relay::start(refused_addr)` at `:1697` keeps working with it.

- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:303` and `crates/validate/src/s3.rs:29-30`: brief (H) says no sentence may claim more than the tests show, and this new paragraph keeps sentences that are now false or broken. All of these are one-line wording fixes, for example limiting the first two to "the older PUT and DELETE of one object".
  - **"A PUT or DELETE success that carries a body, which S3 never sends, is unreadable."** `Probe::CopyObject` is a PUT, and `a_2xx_carrying_the_operations_whole_s3_success_is_a_success` requires its 200 with a `<CopyObjectResult>` body to be `Outcome::Success`. The same paragraph later says S3 sends bodies on CopyObject.
  - **"Every body the SDK buffers whole (every error response, and every PUT or DELETE response) … a PUT or DELETE success's at nothing".** The same claim is at `s3.rs:29-30`. It is no longer true of every response: the probes' PUTs and DELETEs and their error responses, and the bulk delete's, are read by the client under the 64 KiB budget (`Capture`), and a CopyObject success is not capped at nothing.
  - **"a `Transfer-Encoding` on a `304` to a PUT or a DELETE".** The code refuses it for anything but a GET or HEAD (`crates/validate/src/s3/response.rs:117`). The new tests pin POST too: CreateMultipartUpload, CompleteMultipartUpload and bulk delete.
  - **"every row lost. So does a delete result that holds anything but…".** The sizing text added in iteration 3 separated "So does" from what it refers to ("fails the whole call"). It now reads as if a delete result loses rows.

- `check-gates.json:44-48`: this row's claim is not earned. It is titled "red->green" and says `pass`, but `gate-logs/C4-verify.log` ends in `PASS (green-only)`, so the gate showed no red. The brief (`brief.md:180-181`) also predicted `UNVERIFIABLE (exit 77)`, which is not what happened. I ran the reds myself, so the evidence holds (see below). Only the row's label is misleading, and the builder has nothing to fix.

- `check-gates.json:66-70`: the C5 `fail` is not a test gap. The 3 timeouts (`s3.rs:509`, `:514`) all break endpoint resolution for every request. I ran the `Ok("xyzzy")` one: 52 tests in this one file went red, so the whole-suite timeout is most likely many slow failures rather than a hang. The two guard mutants were not run; by reading, they refuse every request the same way.

- Outside this diff, for information: `crates/gateway-s3/src/lib.rs:3803` (`xml_escape`) does not escape a carriage return. Against the real gateway, `delete_objects(&["a\rb", "other"])` reports a row `Deleted { key: "a\nb" }`, and lists `a\rb` under `keys_without_a_row` and that row under `rows_not_requested`. The client reads the XML correctly (`crates/validate/src/s3/document.rs:75`), and the fault is the gateway's. It deserves its own issue because 743.1 will judge those two lists.

## Red→green, re-run on the production path

All of these ran against `s3_client_smoke_verbs`, using the real gateway, relay or scripted endpoint. The tests call the production `S3Client`, not a copy of it.

- **Reds the brief names, all red:**
  - red 1: `a_probe_answered_200_keeps_its_200…`
  - red 3 (`.endpoint_resolver(PathStyle)` removed, so the SDK's own rules apply): `bucket_probes_send_every_name…` reports 42 failed cases, and three other tests go red
  - red 5: 4 tests
  - red 6: 3 tests
  - red 16: both deadline tests
  - red 17: 4 tests
- **Carry-forward items, all red when undone:**
  - processing instruction skipped
  - unknown root child skipped
  - empty `<Key>` accepted
  - `Capture::new(true)` on bulk delete
  - body timeout returned as `NoAnswer`
  - non-empty `UploadPart` body
  - `CreateBucket` with a configuration
  - wrong `CompleteMultipartUpload` part number
- **`too_long` and the resolver, all red:** `URI_ROOM` at 40 and at 65, no `/` credit, raw key length, upload id ignored, `%` or `/` left unencoded, lower-case hex, trailing slash kept, endpoint query accepted, scheme not checked.
- **Overall:** 41 mutations went red and 3 stayed green. None of the 3 is a test gap:
  - Removing `.content_length(0)` (`crates/validate/src/s3/verbs.rs:436`) changes nothing on the wire, because the body is empty anyway.
  - A one-byte body with the length left at 0 stayed green. A one-byte body with a correct length went red.
  - Removing `.disable_s3_express_session_auth(true)` (`s3.rs:193`) changes nothing today, and the comment there says it only guards against a later SDK.

## Attempted to refute, could not

- **Panics:** no panic from any key, upload id or bucket length at or around the URI limit. This covered every probe with a key at exactly 65,470 encoded bytes, using plain, `/`, `é`, `&` and 4-byte characters, and bucket names from 65,440 to 65,557 bytes. `URI_ROOM` covers the longest thing the SDK writes (40 bytes). The SDK's encoding set matches `path_segment` (`aws-smithy-http` 0.64.3 `urlencode.rs:9-42`, `label.rs:14`).
- **Unsendable input:** an empty key, an empty upload id, a newline in a source key, and Unicode-white-space bucket names are all `RequestNotBuilt` with nothing sent. `a\0b` goes out as `/a%00b/`. `/a` and `../a` as keys are sent unnormalised.
- **Bulk delete against the real gateway:**
  - Empty key list, 1,001 keys, and control-character, `\u{fffe}` and empty keys: the gateway's 400 is kept, and the other key in the request survives.
  - Leading or trailing space, tab, `\n`, `\u{85}`, `\u{2028}` and `]]>` in keys: rows match the requested keys exactly.
- **Budget test:** the 41/42 boundary comes from a real store fault, so the gateway's own `<Error>` rows set the sizes. The numbers add up: 62,115 + 41×83 = 65,518 and 62,115 + 42×83 = 65,601. `received > BUFFERED_BODY_BUDGET` no longer depends on how TCP splits the response.
- **#955's tests:** the only test name removed from the file is the rename to `listing_and_delete_result_documents_that_are_not_whole…`. The listing legs still run there.
- **Endpoint forms:** an endpoint's `#fragment` is dropped silently. The SDK's rules parse the same way (`aws-sdk-s3` 1.155.0 `endpoint_lib/parse_url.rs:49-62`), so this is no regression and the `PathStyle` doc's claim holds.

<!-- pdca:leaf-complete -->
