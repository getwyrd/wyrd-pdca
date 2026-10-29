# Adversarial review — #938 (743.2: the S3 client's smoke verbs)

**Verdict: I tried to refute the fix and could not.** I rebuilt the patched tree in a scratch
copy and ran the new test (23 of 23 pass). Then I took the fix apart five ways; each time the
test went red. Every attack input I tried on the production code came back reported as the
server sent it. The only gating red is environmental (see the one NEEDS-HUMAN item).

## The evidence (red→green, re-run by me)

- C4-verify is UNVERIFIABLE (exit 77). The brief declared that in advance: the test calls verbs
  the base does not have, so it cannot compile without the fix. I reproduced the five reds the
  brief demands by removing pieces of the fix in a scratch copy:
  1. Return only the timeout when a 2xx body stalls (`crates/validate/src/s3/verbs.rs:381`): the
     (C) test fails for every probe, e.g. `CopyObject Stall: the 200 head was lost: Timeout { phase: BodyIdle, limit: 500ms }`.
  2. Accept a listing body that is not a whole document as an empty page
     (`crates/validate/src/s3/document.rs:144-145`): (D) fails, e.g. `ListV2 cut-short: not refused as Unreadable: Listed([])`.
  3. Put the SDK's own endpoint rules (`DefaultResolver`) back in place of `PathStyle`
     (`crates/validate/src/s3.rs:186`): 4 tests fail, including
     `bucket_probes_send_every_name_as_one_path_segment_on_an_ip_or_a_host_name_endpoint`.
  4. Stop after the first page (`crates/validate/src/s3/verbs.rs:227`, `:261`): the real-gateway
     listing test, the v1 marker test and the page-cap test fail.
  5. Drop the `<Error>` rows (`crates/validate/src/s3/document.rs:211`):
     `a_mixed_delete_result_is_reported_row_by_row_in_document_order` fails.
- The test runs the production code, not a copy of it. It builds `S3Client` through the
  production argument parser (`crates/validate/tests/s3_client_smoke_verbs.rs:226-251`) and
  drives it against the real in-process gateway and a scripted loopback endpoint. Its
  assertions are exact (whole key/size lists, row-by-row equality, status plus request id), not
  counts that could pass for the wrong reason.

## Attacks on the fix that held

- HEAD and HeadBucket answered `200` with stray body bytes (after `Content-Length: 5`, after
  `Content-Length: 0`, or a chunked body): reported as `BodyFailed(Overrun { after: 5 / 15 })`
  beside the 200, never as a clean success (`crates/validate/src/s3/answer.rs:541-545`).
- A listing page that holds a whole document but declares 50 more bytes, then the connection is
  cut: page 1 is `BodyFailed(Transport { received: 440 })` beside the 200, never a listing.
- Odd bucket names: `a/b`, `a?b`, `a#b`, `%41`, ` x `, `é`, a tab, NUL, `../validate`. Each
  reached the endpoint as exactly one percent-encoded segment (`DELETE /..%2Fvalidate/`,
  `/%2541/`, `/%20x%20/`): nothing trimmed, nothing rewritten, and no name escapes into the
  client's own bucket (`crates/validate/src/s3.rs:544-556`).
- Losing the head between SDK hooks: `read_after_transmit` and `modify_before_deserialization`
  run in the same poll, with no await between them, and the body is read after `send()`
  returns (`crates/validate/src/s3/answer.rs:11-18`, `:503`). So the SDK's operation deadline
  cannot fire mid-body and erase the 2xx.
- An endpoint that answers every page with another continuation: bounded at
  `MAX_LISTING_PAGES` (`crates/validate/src/s3/document.rs:22`), and tested.

## Gates and claims

- NEEDS-HUMAN [human] — The gating C4-ci red is environmental, not this patch. `cargo deny`
  fails on RUSTSEC-2026-0385 in `ctutils` 0.4.2, which is already on the base
  (`Cargo.lock:1000-1002` at the base commit). It comes in through `hmac`/`sha2` from
  `aws-sdk-s3`, `wyrd-core` and `wyrd-gateway-s3`. The patch's only `Cargo.lock` change adds
  `wyrd-traits` to `wyrd-validate`'s list. The fix (`cargo update -p ctutils` to >= 0.4.4) is
  outside this slice's scope. Decide: bump the lockfile separately on `main`, or accept this red
  as not caused by the patch. This is not a refutation of the fix.
- C4-diff-cov "patch.diff does not apply on origin/main" is expected. The brief builds this
  slice on #853's integration line, not `main`. Not a refutation.
- C5: the 2 missed mutants are both `Capture::name` (`crates/validate/src/s3/answer.rs:455`),
  the interceptor's label string, which nothing reads. The 3 timeouts
  (`crates/validate/src/s3.rs:505`, `:510`) make every endpoint refused. The scripted-endpoint
  tests then wait out `Endpoint::heads()`'s 20 s bound
  (`crates/validate/tests/s3_client_smoke_verbs.rs:1244-1251`) for requests that never come, so
  the run passes 61 s instead of failing fast. Those mutants did not survive. Not raised.
- I found no unwarranted claim in `check-gates.json`. The T4 batch review's "0 blocking" matches
  what I found.

## Notes (not routed; no action needed in this slice)

- A 2xx body with no `Content-Length` and no `Transfer-Encoding` ends where the connection
  closes, and the client accepts it for documents and empty successes
  (`crates/validate/src/s3/answer.rs:497-502`). A cut document is still refused by the strict
  read. But a headers-only success (CreateBucket, DeleteBucket, UploadPart) cut before its first
  body byte cannot be told from an empty success. This is documented and built into HTTP/1.1
  (the GET object path refuses such framing instead). Not a defect.
- For 743.1: a `<DeleteResult>` that leaves a requested key out of both lists comes back as
  `Ok`. I sent `<DeleteResult/>` for two keys and got `Ok(DeleteResult { rows: [] })`. The brief
  makes judging 743.1's job (`crates/validate/src/s3/document.rs:9-10`). The rubric's
  "absent entries" class applies there: compare the requested keys with the rows, never just
  `errors().count() == 0`.
- Doc wording: `docs/design/architecture/05-building-block-view.md:299` says a probe "can name a
  bucket those rules forbid, and so cannot create or delete one it does not own". The client
  does not enforce that. `Probe::DeleteBucket { bucket }` sends any name
  (`crates/validate/src/s3/verbs.rs:71`, `:377`), so `"validate"` sends `DELETE /validate/`. The
  safety comes from 743.3 picking a forbidden name and the server refusing it, as
  `crates/validate/src/s3/verbs.rs:36-37` puts it. Worth rewording if the doc is touched again;
  not worth a round on its own.

Attempted to refute: (C) keeping the 2xx head, (D) strict listing and delete-result reading, (B)
bucket-name routing on IP and host-name endpoints, (A) following pages, (F) row order, and
RequestNotBuilt vs NoResponse. Could not.

<!-- pdca:leaf-complete -->
