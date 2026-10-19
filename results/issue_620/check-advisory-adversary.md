# Adversarial review — issue #620 (s3-object-key-length-limit)

**Result: could not refute.** I re-ran the red→green proof, wrote six mutants a weak test would
miss, and probed request-path spellings over the wire. Every attack was caught or behaved as the
brief requires. No NEEDS-HUMAN findings.

- **Red→green re-run (reproduced).** On a scratch copy of `$PDCA_TARGET`: with the fix, 7/7 pass.
  With only `crates/gateway-s3/src/lib.rs` reverted (test kept), 4 tests fail at
  `crates/server/tests/s3_key_length_limit.rs:446` (`assert_refused`): legs 1, 2, 5, 6
  (`:466`, `:498`, `:687`, `:708`), each with `left: 200, right: 400`. The 3 delete legs
  (`:535`, `:611`, `:662`) pass both ways, as the brief's Falsifiability says they should. The
  test drives the production path: the real `S3Gateway::serve` on a loopback socket, the real
  redb + fs backend, and raw signed HTTP. It is not a copy of production logic.
- **Gate claim that overstates (harness wording, not this patch).** `check-gates.json` C4-verify
  says "7 test(s) ran red". Its own log, `gate-logs/C4-verify.log:61`, says
  `3 passed; 4 failed`. Only 4 legs go red, and the other 3 are meant to stay green. The PASS
  verdict still holds (one red leg is enough), but the count is wrong. Nothing for the builder to
  do.
- **Hand-written mutants, each run against the wire test. All caught:**
  - measure the raw wire `key.len()` instead of the decoded key (`crates/gateway-s3/src/lib.rs:1864`)
    → red in `limit_counts_utf8_bytes_of_the_decoded_key_alone` (`s3_key_length_limit.rs:498`);
  - measure `key_name.chars().count()` → red in the same test (the `€` leg);
  - check the raw `bucket.contains('/')` instead of the decoded one (`lib.rs:1853`) → red in legs 5 and 6;
  - measure the flat `object_key.len()` → red in legs 2 and 6;
  - add `if key.len() > 8_000_000 { return Err(Malformed) }` after the empty-key check in
    `parse_delete_request` (`lib.rs:2333`) → red in
    `bulk_delete_removes_the_longest_key_the_body_cap_can_carry` (`s3_key_length_limit.rs:611`).
    This closes the iteration-5 finding: a bound anywhere below the cap now goes red;
  - run the bucket check only for `PayloadHash::Signed` → red in leg 6 (`s3_key_length_limit.rs:708`).
    This closes the iteration-6 finding.
- **Path spellings probed over the wire against the fixed gateway, all correct.**
  `/b%2fa/<1023>` (lowercase hex) → `400 InvalidBucketName`; `/%2Fb/k` → `400 InvalidBucketName`;
  `//b/<1025>` → `400 KeyTooLongError` (`split_bucket_key` trims every leading `/`,
  `lib.rs:591`); `/b/a%2F<1023>` → `400 KeyTooLongError`; `/b/<1025>?x-id=PutObject` →
  `400 KeyTooLongError`; `/b%252Fa/<1023>` → `200`, but it is stored as the literal `b%2Fa/…`,
  not as an alias of `b/a/…`. Routing reads only `parts.uri.path()` (`lib.rs:1675`), and there is
  no virtual-host routing, so the Host header cannot be used to get around the check. No other
  path creates objects: CopyObject answers `501` (`lib.rs:1831`), multipart query forms answer
  `501` through the denylist (`lib.rs:1808`), POST on an object path answers `405`, and
  `crates/proto` has no RPC that writes objects.
- **The 414 claim checked.** The new docs (`lib.rs:498`, and the paragraph in
  `docs/design/architecture/05-building-block-view.md`) say a request target over 65,534 bytes
  gets `414` before the gateway runs. Probed: HEAD with a 65,534-byte path → `404` (it reached the
  gateway); with 65,535 bytes → `414`; HEAD and DELETE of the 70,000-byte key → `414`. The claim is
  exact, so bulk delete really is the only S3 way to remove such a key.
- **Lossy decode (observation; existed before this patch, out of scope, not raised).**
  `percent_decode_utf8` (`lib.rs:3367-3385`) replaces invalid UTF-8 with U+FFFD, which is 3 bytes.
  So `PUT /b/<'%FF' × 342>` (342 raw bytes) is refused `KeyTooLongError`, because the key it
  would store is 1,026 bytes. That matches the Invariant, which limits what gets stored. The lossy
  decode predates #620.
- **Native backend key limits (observation; existed before this patch, not raised).** The new
  docs say a bulk-delete entry is "bounded only by" the 8 MiB body cap (`lib.rs:518-520`). That is
  true of the S3 layer, and the tests run only on redb. FDB and TiKV have their own key-size
  limits, but a key too long for them cannot be stored there at all, so "every stored key stays
  deletable" still holds. #893 tracks native key limits.
- **Test comment claims a little too much (no action needed).** `lib.rs:4903` says the in-crate
  at-limit PUT shows "the form itself is accepted … not a framing or signature failure". But
  `RecordingGateway::put_object_streaming` never reads its `_source` (`lib.rs:4633`), so the
  in-crate test never checks chunk signatures. It only checks the header-level seed signature. The
  wire test's leg 6 (`s3_key_length_limit.rs:708`) does prove the framing works on the real
  backend (HEAD `200` afterwards), so the property is covered.

Attempted to refute: the red→green evidence (re-run; checked that it uses the production path),
the PUT refusal (decoded vs. wire vs. character length, flat-name length, `%2F` and its spelling
variants, leading `//`, query params, all three payload forms, other create paths, Host routing),
and the promise that no delete path gains a length bound (an 8 MB bulk-delete bound mutant; the
414 ceiling). Could not.

<!-- pdca:leaf-complete -->
