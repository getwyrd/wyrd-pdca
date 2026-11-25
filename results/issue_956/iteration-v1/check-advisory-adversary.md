# Adversarial review: issue 956

The bucket routing and the probe requests held up under attack. The delete result did not: a `200` that accounts for none of the requested keys comes back as a clean `Ok`. All results below come from a scratch copy of `$PDCA_TARGET` with the patch applied, where `cargo test -p wyrd-validate --test s3_client_smoke_verbs` passes 45 of 45 before any change.

## Refutations

- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:288`: **a delete result that accounts for no key is reported as success.** `delete_objects(&["a", "b"])` answered with `200` and `<DeleteResult/>` returned `Ok([])`, so `deleted()` and `errors()` are both empty. A result with only `<Deleted><Key>a</Key></Deleted>` returned `Ok`, with `b` unaccounted for. A result naming only `c`, which was never requested, also returned `Ok`. The doc comment says "the result names every key deleted as well as every key not", and that is false for these answers. `docs/design/architecture/05-building-block-view.md:303` says "never a partial listing or a short delete result", and these are short delete results. This is the rubric's "absent entries" class, and it is the same defect the gating T4 row still blocks on (`gate-logs/T4-batch-review.log:10`). The brief leaves judging answers to 743.1, so the client need not refuse. It can report instead, for example with the requested keys that have no row and the rows that name no requested key, which fills in nothing on the server's behalf. Either way the two sentences need correcting and the case needs a test.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:221`: **a failed key can vanish in a row that goes unread.** `_ => {}` skips any child it does not recognise. `<Deleted><Key>a</Key></Deleted><error><Key>b</Key><Code>AccessDenied</Code></error>` returned `Ok([Deleted { key: "a" }])`, so `errors()` is empty for a key the server said it did not delete. The brief says the `<Error>` rows are the only evidence of a failed key (`brief.md:40`), and the rubric rejects silent skips. The test that pins the skip (`crates/validate/tests/s3_client_smoke_verbs.rs:2635`) uses `<RequestCharged>`, which the SDK reads from a header for this operation (`aws-sdk-s3` 1.155.0 `protocol_serde/shape_delete_objects.rs:38`). Its deserializer reads only `Error` and `Deleted` as elements (same file, `:143`, `:160`), so that test does not show a real element that needs skipping. Refuse unknown children, or report them.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3/document.rs:209`: **an empty `<Key/>` counts as naming a key.** `<Deleted><Key/></Deleted><Error><Key></Key><Code>x</Code></Error>` returned `Ok([Deleted { key: "" }, Error(KeyError { key: "", .. })])`. The check at `:209` and `:214` catches only a missing `<Key>`, although its refusal reads "names no `<Key>`". An empty `<Code>` is already read as absent (`named_code`), so the two fields are treated differently.
- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:56`: **the probe docs describe request bodies that no test checks.** The relay and the request checks record heads only (`crates/validate/tests/s3_client_smoke_verbs.rs:661`, `:709`). Three mutations each left all 45 tests passing:
  - `UploadPart` sending a 5-byte `hello` (doc at `:56`, "with an empty part")
  - `CompleteMultipartUpload` sending no parts (doc at `:58`, "the one empty part")
  - `CreateBucket` sending a `us-west-2` `<CreateBucketConfiguration>` (doc at `:69`, "with no")

  The docs are true today: on the wire, `UploadPart` has `content-length: 0`. But brief (H) says no doc sentence may claim more than the tests show. The body also affects the answer: a server that checks the body before the upload id could answer an empty `CompleteMultipartUpload` with a different code, which 743.1 would then judge. The brief excuses body *framing* from the checks, not body content. Record the bodies and assert these three.
- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:303`: **the safety sentence claims more than the client does.** "a probe can name a bucket those rules forbid, and so cannot create or delete one it does not own" reads as a property of the client, but the client checks no name. `Probe::DeleteBucket { bucket: "victim" }` is sent as given, and the safety comes only from 743.3 choosing a forbidden name. The sentence also drops the exception in `crates/validate/src/s3/verbs.rs:44`: I ran `..` through the relay and it was sent as `DELETE /../`, which a server that resolves dot segments reads as the endpoint's parent. That verbs.rs sentence is accurate but has no test, because `MORE_FORBIDDEN_BUCKETS` (`crates/validate/tests/s3_client_smoke_verbs.rs:150`) has no `.` or `..` row. Reword the sentence so the safety depends on the caller's name, carry the dot-segment exception over, and add both rows.

## Evidence I tried to refute and could not

- `gate-logs/C4-verify.log:60`: the row says `pass`, but the gate only shows green ("green-only"), and `brief.md:181` predicted UNVERIFIABLE (exit 77). So that row is no red→green proof. I ran the reds myself, and each failed the tests that should catch it:
  - Red 1 fails `a_probe_answered_200_keeps_its_200…`.
  - Red 3 fails 4 tests, including `bucket_probes_send_every_name…`.
  - Red 5 fails `a_mixed_delete_result…`.
  - Red 6 fails `a_request_stopped_before…`.
  - Red 16 fails both probe deadline tests.
  - Red 17 fails `every_unsupported_operation…` and the success test.

  These tests go through the real `S3Client::probe` and `delete_objects`, not a copy. `build-notes.md` is withheld from me, so these mutations are my own reading of the brief's descriptions, not the builder's exact edits.
- `gate-logs/C5-mutants.log:13`: the three timeouts do not point to a weak test here. With `path_style_url` returning `Ok("xyzzy")` (`crates/validate/src/s3.rs:506`), all 45 tests in this file fail in 20 seconds with no hang. I did not run the other two mutants, or find which test file reaches the 61-second limit. By reading, those two refuse every endpoint.
- `gate-logs/C4-diff-cov.log:11`: diff coverage was never measured, because the patch does not apply on `origin/main`. That row says nothing either way about coverage.
- `brief.md:8`: "found without defect" should not count as evidence for this child. The first refutation above sits in the salvage's `delete_objects`, and the first two come from code the brief says to take as written.

## Fix attacks that failed

- `crates/validate/src/s3.rs:546`: bucket names `...`, ` x`, `x\n`, `%`, `a?b`, `a#b` and NUL each arrived as one encoded path segment. `U+3000` and `U+00A0` were `RequestNotBuilt` with nothing sent. The encoded set matches `aws-sdk-s3` 1.155.0 `endpoint_lib/uri_encode.rs`.
- `crates/validate/src/s3.rs:506`: endpoints `HTTP://`, `/base/`, `/a%20b`, a fragment, an empty query and no scheme all behaved as documented. The cited `merge_paths` and `parse_url` lines match the SDK source.
- `crates/validate/src/s3/verbs.rs:58`: an upload id needing encoding reached the gateway encoded. An empty key, an empty upload id and a newline in `source_key` were each `RequestNotBuilt` with nothing sent.
- `crates/validate/src/s3/verbs.rs:288`: sending `<Quiet>` true is caught by the gateway test.
- `crates/validate/src/s3.rs:191`: removing `disable_s3_express_session_auth` changes no test, and the comment above it already says it only guards against a later SDK. This is not a defect.
- Not filed, as they predate this patch:
  - An endpoint with userinfo (`http://u:p@host`) gets a `403` `SignatureDoesNotMatch` here and also with the SDK's own resolver back in place.
  - `get_object` and `delete_object` report a refused `ftp://` endpoint as `NoResponse`. They use the older `classify` path this patch does not change; I confirmed that by reading, not by running the base.

<!-- pdca:leaf-complete -->
