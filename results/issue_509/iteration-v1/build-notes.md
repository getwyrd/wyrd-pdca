# Build notes — issue 509 / delete-objects-bulk

## What changed (target: getwyrd/wyrd, base `pdca-integration/main` = `99ef6e6`
"pdca-integrate: issue_507", the wave-folded base carrying 507's bucket/object routing
split)

`crates/gateway-s3/src/lib.rs`:

1. `has_delete_subresource` (new, inserted after `UNSUPPORTED_SUBRESOURCES` at
   `lib.rs:372`) — a percent-decoded match for the `delete` query key, mirroring
   `unsupported_subresource_decoded`'s decode discipline so an encoded `?%64elete`
   can't dodge routing.
2. `dispatch`'s bucket-scoped arm (`lib.rs:1463` pre-patch) gets one new branch, inserted
   **before** the subresource-denylist check: `if method == Method::POST &&
   has_delete_subresource(&query) { return delete_objects(...).await; }`. Every other
   method/path form naming `delete` (an object-scoped `PUT/DELETE /b/k?delete`, or a
   non-POST bucket-scoped `?delete`) falls through unchanged to the existing denylist,
   which still lists `delete` (`lib.rs:343`) — confirmed unchanged by the full
   `s3_list_objects.rs` suite still passing, including
   `get_bucket_versioning_is_501_not_a_listing_document` and the multipart/subresource
   guard tests.
3. New handler `delete_objects` (inserted after `list_response`) plus its supporting
   pieces: `DELETE_OBJECTS_MAX_KEYS`/`DELETE_OBJECTS_MAX_BODY_BYTES` constants,
   `DeleteRequest`, a minimal `XmlNode`/`XmlParser` tree parser scoped to the `<Delete>`
   schema, `xml_unescape` (the exact inverse of the existing `xml_escape`),
   `parse_delete_request`, and `buffer_bounded` (a capped stream-to-`Vec<u8>` collector).

`crates/server/tests/s3_delete_objects.rs` (new): 5 tests driving the wire with a stock
`aws-sdk-s3` client (`delete_objects()`), mirroring the peer harnesses the brief cited
(`s3_gateway_cluster.rs:98`'s `sdk_client`, `s3_object_metadata.rs:43-71`'s
`build_gateway`/`start_gateway`).

## Design decisions and why

- **Routing order, not denylist removal.** `delete` stays in `UNSUPPORTED_SUBRESOURCES`
  (it must — the brief's Impact & compatibility: object-scoped `PUT/DELETE /b/k?delete`
  and non-POST bucket forms keep their existing 501/400). I considered removing `delete`
  from the list and adding a method-aware check inside `unsupported_subresource_decoded`
  itself — rejected: that function is also used by the **raw** (un-decoded)
  `unsupported_subresource` for the OBJECT path via a shared list, so threading a
  method parameter through it to carve out one case would touch a function shared by two
  call sites for a distinction only one of them needs. A single `if` in `dispatch`,
  checked first, is an ~12-line net addition versus reshaping a shared helper.
- **Payload-hash handling covers all three `PayloadHash` variants**, not just `Signed`
  (which the brief's Design section explicitly expects a real SDK to send for this op —
  confirmed empirically: all 5 tests exercise the real `aws-sdk-s3` client and pass,
  meaning it does send `Signed`, never `Streaming`). I still added the `Streaming` arm
  (de-framing via the existing `streaming::decode`, the same decoder the PUT path uses)
  rather than leaving it a `todo!()`/`500` — the cost was ~6 lines reusing an existing
  production primitive, not a new implementation, so the extra safety was cheap. It is
  unexercised by the test suite (no SDK path drives it for this op); flagged here rather
  than left silent.
- **In-crate XML parser, not `quick-xml`.** Matches the brief's explicit instruction (no
  new dependency; ADR-0003 audit is human-gated) and the Alternatives-considered section.
  The parser is a small generic element/text tree walker (not special-cased per element
  name) so an unknown nested child (a future `<VersionId>`) is skipped correctly by
  falling out of the recursion, not by a bespoke "skip N lines" hack.
- **Buffer the whole body** (`buffer_bounded`, capped at `DELETE_OBJECTS_MAX_BODY_BYTES` =
  2 MiB, AWS's own documented limit) rather than streaming it — this is the one
  intentional exception to the crate's "stream, don't buffer" invariant (module docs,
  `lib.rs:12`), justified exactly as the brief's Design section states: the body is
  small (≤1000 keys), signed as one unit, and the cap is enforced *before* any byte is
  trusted (the `Err` arm of `buffer_bounded` fires mid-stream, past-cap, not after
  buffering everything then checking).
- **Percent-decoding vs XML-unescaping of keys.** The brief's Scope line says
  "percent-decoding/XML-unescaping of keys consistent with the object path
  (`lib.rs:801-805`)". I implemented only XML-entity-unescaping, not percent-decoding,
  for `<Key>` text content: per the S3 `DeleteObjects` XML protocol the key travels as
  literal UTF-8 text inside the element (only entity-escaped for XML specialness), never
  percent-encoded — confirmed by reading the real `aws-sdk-s3` codegen
  (`_object_identifier.rs`, `shape_object_identifier.rs` in the vendored
  `aws-sdk-s3-1.138.1` source) and by the entity-escaped-key test passing end-to-end
  against a real SDK-built request. Percent-decoding a `<Key>` value would be *wrong* —
  it would mangle a key that legitimately contains a literal `%`. I read this brief
  clause as "apply the analogous decode discipline the object path uses", not "apply
  literally both transforms", and the success criterion text ("an entity-escaped key…")
  confirms only unescaping is being asked for.
- **Per-key error code**: reuses the seam's own `classify(&BoxError) -> (StatusCode, code,
  message)` (already used by `gateway_error_response`) so a per-key gateway fault gets the
  exact same code/message a single-object DELETE would have produced, rather than a
  parallel classifier. This resolves the brief's first Open Question ("Do decides"): no
  new vocabulary, one classifier, no drift risk between the two S3 error surfaces.
- **1000-key / body-size limits answer `400 MalformedXML`** for both violations (over-count
  and over-size) — chosen because AWS documents both under the same practical XML-request
  constraint (≤1000 keys, ≤2 MiB) and a bespoke code for "too many keys" isn't part of the
  DeleteObjects error vocabulary; `MalformedXML` is what a client library already knows how
  to treat as non-retryable-until-the-request-is-fixed.
- **Alternatives rejected** (mirroring the brief's own Alternatives-considered section, with
  cost shown): a `quick-xml` dependency — rejected, human-gated ADR-0003 audit, and the
  fixed schema doesn't warrant a general parser (a full crate vs. the ~190-line
  scope-limited parser added here); failing the whole request on the first bad key —
  rejected, contradicts S3's documented per-key result contract `sync --delete` depends on
  (this is also directly assertable: `bulk_delete_removes_present_and_absent_keys_...`
  would need every requested key to error together, which the brief's own success
  criterion rules out).

## Resolved open question (not open, per the brief)

The brief's second "open question" is marked resolved by prior adversarial review: a
default-integrity stock SDK sends `x-amz-checksum-crc32` / `x-amz-sdk-checksum-algorithm`
as ordinary **headers** (inside `SignedHeaders`, so auth passes), not a `Content-MD5` and
not a chunked trailer. This build accepts-and-ignores them — no explicit code reads or
validates those headers for this op, exactly matching the stated Alpha posture. Confirmed
empirically: all 5 tests use the real, default-configured `aws-sdk-s3` 1.138.1 client and
all pass, so whatever integrity headers it attaches did not need any handling here.

## Refutation (forced self-check)

- **(a) Genuine red?** Yes. With `crates/gateway-s3/src/lib.rs` reverted (`git stash` of
  just that file, test file kept), all 5 tests in `s3_delete_objects.rs` fail: 4 with a
  real `501 NotImplemented` / `<Code>NotImplemented</Code>` "the `delete` S3
  subresource/operation is not supported" (the `delete` denylist entry catching the
  request before any routing split, since on THIS folded base the subresource check
  fires before the bucket-scoped method match — not the `400 InvalidRequest` line the
  brief's Falsifiability paragraph cites, which described base line numbers that shifted
  once 507's listing code was folded in; the effect — never a `200 <DeleteResult>` — is
  the same) and one (`bulk_delete_malformed_xml_body_is_refused`) with `501` instead of
  the expected `400`. Restoring the patch turns all 5 green. Output of both runs is
  captured above in this session (not reproduced here to keep this file to the point).
- **(b) Production path?** Yes. The test drives the real `aws-sdk-s3` client against the
  real `S3Gateway`/`Gateway` composition (`wyrd_server::Gateway<RedbMetadataStore,
  FsChunkStore, MemCoordination>`) over a real loopback TCP listener — the exact
  `start_gateway`/`sdk_client` pattern the two cited peer test files use, not a
  stand-in. The one raw-TCP test (malformed XML) hand-signs a request with the
  production `sigv4::sign`, matching `s3_http_wire.rs`'s own precedent for cases a stock
  SDK will never construct.
- **(c) Fixture includes the fault?** Yes. Every test stands up its own gateway with
  actually-stored objects (via a real signed `PUT`) before deleting them, and asserts the
  post-delete state with a real signed `GET` (expecting `NoSuchKey`) rather than trusting
  the delete response alone. The "≥1 absent key" case is a key that was never PUT at all
  (`a/never-existed`), not a filtered-out one.

## Test run (through the project's runner)

`cargo test -p wyrd-server --test s3_delete_objects` (this project's own `cargo test`,
the same primitive `cargo xtask ci` uses for the workspace test phase — the driver's
`engine/xtask.sh` wraps `cargo xtask ci`, which is a whole-workspace, several-minute run;
I ran the scoped `cargo test` command directly per-crate/per-test-file as the fast
red/green sanity pass the Do brief calls for, then additionally ran the full existing
suites for the touched crates — see below — rather than the full multi-minute
whole-workspace `ci` gate, which Check's own gates re-run):

- `cargo test -p wyrd-gateway-s3` — 67/67 pass (no regression in the crate's own unit
  tests, including `unsupported_subresource_flags_multipart_and_subresource_forms`,
  `split_bucket_key_parses_object_paths`, `bucket_scoped_path_names_buckets_and_rejects_...`).
- `cargo test -p wyrd-server --test s3_http_wire --test s3_object_metadata --test
  s3_delete_objects` — 19 + 2 + 5 = 26/26 pass.
- `cargo test -p wyrd-server --test s3_list_objects --test s3_copy_object_guard --test
  s3_head_object --test s3_streaming_trailer` — 2 + 3 + 24 + 11 = 40/40 pass (confirms the
  507 listing/routing split and the other subresource-denylist behaviour are untouched).
- `cargo test -p wyrd-server --test s3_gateway_cluster` — 1/1 pass (the cluster-composed
  SDK interop test still passes).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets -- -D warnings` — clean.
- `cargo fmt -p wyrd-gateway-s3 -- --check` / `cargo fmt -p wyrd-server -- --check` — clean
  (both crates were run through `cargo fmt` before the patch was cut, so the diff is
  already in the target's own formatter's canonical shape).

I did not run the full `cargo xtask ci` (DST sweep, `cargo deny`, conformance vectors,
whole-workspace build) inside this Do session — that is the multi-minute whole-repo gate
`engine/xtask.sh ci` runs at Check, and none of it is sensitive to this change (no
on-disk format touched, no new dependency, no ADR/spec change). The scoped runs above are
the fast red→green sanity pass this step calls for; Check's own gates re-run the complete
suite.

## Out of scope (per brief, not implemented)

- `NoSuchBucket` precondition on delete (ADR-0046 decision 4, lands with #511) — a
  `DeleteObjects` against a bucket with no record still executes (each key resolves via
  the existing per-object seam, which has no bucket-existence concept either); this
  matches how single-object `DELETE` already behaves today (idempotent, no bucket check),
  so there is no behavioural regression, just the pre-existing gap this brief explicitly
  defers.
- `Content-MD5` enforcement — accept-and-ignore, per the brief and the resolved open
  question above.
- Versioned deletes (`<VersionId>`) — the parser recognizes and ignores an `<Object>`
  child it doesn't know (which includes a future `<VersionId>`), but no version semantics
  exist; a versioned-delete request just deletes the current object under `key`.
- Any change to single-object `DELETE` — none made; `crates/gateway-s3/src/lib.rs`'s
  object-path `Method::DELETE` arm is untouched (confirmed by `s3_http_wire.rs`'s DELETE
  tests, `concurrent_delete_is_idempotent`, `signed_put_get_delete_round_trip_...`, all
  still passing).
