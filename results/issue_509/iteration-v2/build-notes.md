# Build notes — issue 509 / delete-objects-bulk (iteration 2)

## Target / base

Target: getwyrd/wyrd @ `main`, built in the isolated worktree `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`), whose base is `99ef6e6`
"pdca-integrate: issue_507" — the wave-folded base carrying 507's bucket/object routing
split (the guard this slice extends). All `path:line` citations below are against that
worktree HEAD after the fix (the same lines the patch produces).

## What the iterate had to address (carry-forward)

The previous attempt (`iteration-v1/`) was **not** rejected on architecture — the reviewer
PASSed every C-item (C1 spec, C2 red, C3 change, C5 causal adequacy, T1/T2/T3/T5). It was
auto-iterated because two items were left **provisional**, both verification-completeness,
not design:

1. **C4 Verification — the aggregate gate was never run.** Round 1 ran only scoped
   `cargo test` + independent fmt/clippy; the reviewer noted "the asserted
   `./engine/xtask.sh ci` script was absent from the supplied target/clone, so its full
   checks remain provisional." → **Resolved this iterate: I ran the full `./engine/xtask.sh
   ci` aggregate gate end-to-end in `$PDCA_WORKTREE` and it PASSED** (see "Test run" below).
   The provisional flag is gone: the whole-workspace gate is green, not assumed.
2. **T4 Contribution — prior-art search incomplete.** Round 1 could only see merged history.
   → **Resolved this iterate: I searched all refs and the live tracker** (see "Prior-art"
   below): no open/closed/rejected bulk-delete work exists; issue #509 is OPEN with no linked
   PR.

The fix itself is the correct, minimal wire-level fan-out the brief specifies, so I
reproduced that approach (it was the right one) and closed the two verification gaps that
actually caused the iterate. Churning the design to look "different" would have been
change-for-change's-sake and is explicitly the wrong move when the approach was sound.

## What changed

`crates/gateway-s3/src/lib.rs`:

1. `has_delete_subresource` (new, `lib.rs:383`) — a **percent-decoded** match for the
   `delete` query key, mirroring `unsupported_subresource_decoded`'s decode discipline
   (`lib.rs:400`) so an encoded `?%64elete` can't dodge routing.
2. `dispatch`'s bucket-scoped arm (`lib.rs:1902`, inside the `bucket_scoped_path` block at
   `lib.rs:1896`) gets one new branch, inserted **before** the subresource denylist check
   (`lib.rs:1911`, `unsupported_subresource_decoded`): `if method == Method::POST &&
   has_delete_subresource(&query) { return delete_objects(...).await; }`. Every other
   method/path form naming `delete` (an object-scoped `PUT/DELETE /b/k?delete`, or a
   non-POST bucket-scoped `?delete`) falls through unchanged to the denylist, which still
   lists `delete` (`lib.rs:343`) → 501/400 exactly as before.
3. New handler `delete_objects` (`lib.rs:1359`) plus supporting pieces:
   `DELETE_OBJECTS_MAX_KEYS`/`DELETE_OBJECTS_MAX_BODY_BYTES` (`lib.rs:1066-1067`),
   `DeleteRequest`, a minimal `XmlNode`/`XmlParser` tree parser scoped to the `<Delete>`
   schema, `xml_unescape` (`lib.rs:1242`, exact inverse of the existing `xml_escape`,
   `lib.rs:1899`), `parse_delete_request` (`lib.rs:1281`), and `buffer_bounded`
   (`lib.rs:1334`, a capped stream→`Vec<u8>` collector).

Per-key delete reuses the existing single-object seam `ObjectGateway::delete_object`
(trait `crates/gateway-core/src/lib.rs:196`, impl `crates/server/src/lib.rs:426`), the same
seam the object-path DELETE arm calls (`lib.rs:2162`) — already idempotent (`Ok(_)` for a
removed OR an absent key), so S3's "delete is idempotent" contract holds for free.

`crates/server/tests/s3_delete_objects.rs` (new): 5 tests driving the wire with a stock
`aws-sdk-s3` client (`delete_objects()`), mirroring the peer harnesses the brief cited —
`s3_gateway_cluster.rs:98` (`sdk_client`) and `s3_object_metadata.rs:43-71`
(`build_gateway`/`start_gateway`/`signed_headers`).

## Design decisions and why

- **Routing order, not denylist removal.** `delete` STAYS in `UNSUPPORTED_SUBRESOURCES`
  (`lib.rs:343`) — it must, so object-scoped `PUT/DELETE /b/k?delete` and non-POST bucket
  forms keep today's 501/400 (brief Impact & compatibility). I rejected removing `delete`
  from the list + threading a method parameter through `unsupported_subresource_decoded`
  (`lib.rs:400`): that helper is shared with the OBJECT-path raw matcher
  `unsupported_subresource` (`lib.rs:386`) via the same list, so a method carve-out would
  reshape a function used by two callsites for a distinction only one needs. Cost shown: the
  chosen fix is a single ~12-line `if` in `dispatch` (`lib.rs:1902-1911`); the rejected one
  changes a shared helper's signature and every one of its two callsites plus the list
  semantics — strictly more surface for no behavioural gain. The confirmation that nothing
  else regressed: the full `s3_list_objects.rs` (24) and the `wyrd-gateway-s3` unit suite
  (67) still pass, including the subresource-denylist guard tests.
- **Payload-hash handling.** The single-shot signed body is verified AFTER buffering, before
  a byte is trusted — `sha256(buffered) == x-amz-content-sha256` via the crate's own
  `crypto::{sha256,hex,constant_time_eq}` (`crates/gateway-s3/src/crypto.rs:54,70,81`) — the
  same discipline the PUT path applies post-stream (`ContentHash::Expected(hex)`,
  `lib.rs:1585`). A `Streaming(ctx)` payload is de-framed with the SAME `streaming::decode`
  the PUT path uses (`crates/gateway-s3/src/streaming.rs:188`) then buffered. The `Streaming`
  arm is unexercised by the suite (the stock SDK sends `Signed` for this op — confirmed:
  all 5 tests use the real default-config `aws-sdk-s3` 1.138.1 client and pass) and I
  flag that here rather than leave it silent; I kept it (~6 lines reusing an existing
  production primitive, not a new implementation) so a streaming-signature delete body is
  handled rather than mis-parsed as MalformedXML, at negligible cost.
- **In-crate XML parser, not `quick-xml`.** Matches the brief's explicit instruction (no new
  dependency; ADR-0003 audit is human-gated). Cost of the rejected alternative: a whole XML
  crate + the human-gated three-test dependency audit + `deny.toml` allowlist entry, versus
  the ~190-line scope-limited generic element/text tree walker added here. The parser is
  generic (not special-cased per element name) so an unknown nested child (a future
  `<VersionId>`) is skipped by falling out of the recursion, not by a bespoke skip.
- **Buffer the whole body** (`buffer_bounded`, `lib.rs:1334`, cap `DELETE_OBJECTS_MAX_BODY_BYTES`
  = 2 MiB) — the one intentional exception to the crate's "stream, don't buffer" invariant
  (module docs), justified as the brief's Design states: the body is small (≤1000 keys),
  signed as one unit, and the cap fires **mid-stream, past-cap** (the `Err` arm of
  `buffer_bounded`) — the bound is by construction, not by trust-then-check.
- **Key transform = XML-entity-unescape only, NOT percent-decode.** The brief's Scope says
  "percent-decoding/XML-unescaping of keys consistent with the object path". Per the S3
  `DeleteObjects` XML protocol the key travels as literal UTF-8 text inside `<Key>` (only
  entity-escaped for XML specialness), never percent-encoded — confirmed by the
  entity-escaped-key test round-tripping a real SDK-built request end-to-end. Percent-decoding
  a `<Key>` would be *wrong* (it would mangle a key containing a literal `%`). I read the
  brief clause as "apply the analogous decode discipline the object path uses", and the
  success-criterion text ("an entity-escaped key…") asks for exactly unescaping.
- **Per-key error code** reuses the seam's own `classify(&BoxError)` (`lib.rs:2071`) — the
  same classifier `gateway_error_response` runs — so a per-key gateway fault gets the exact
  code/message a single-object DELETE would produce, no parallel vocabulary. This resolves
  the brief's first Open Question ("Do decides"): one classifier, no drift between the two
  S3 error surfaces.
- **Over-1000-keys and over-2-MiB both answer `400 MalformedXML`** — AWS documents both under
  the same practical XML-request constraint; `MalformedXML` is the code a client library
  already treats as non-retryable-until-fixed. Failing the whole request on the first bad key
  was rejected: it contradicts S3's per-key result contract `sync --delete` relies on (and is
  directly falsified by the present+absent test, which requires each key reported
  independently).

## Resolved open question (not open, per the brief)

The default-integrity stock SDK sends `x-amz-checksum-crc32` / `x-amz-sdk-checksum-algorithm`
as ordinary **headers** inside `SignedHeaders` (so auth passes), not a `Content-MD5` and not
a chunked trailer. The denylist is query-key-only (`UNSUPPORTED_SUBRESOURCES`), so those
headers ride through and this build **accepts-and-ignores** them — no code reads or validates
them for this op, matching the stated Alpha posture. Confirmed empirically: all 5 tests use
the real default-config client and pass without any handling of those headers. (Also
`Content-MD5` enforcement: accept-and-ignore at Alpha, per brief §Scope.)

## Prior-art search (T4 carry-forward — resolved)

- `git for-each-ref` — the only feature refs are `enhancement/507-list-objects-v2`,
  `main`, `pdca-integration/main`, `pdca-verify`. No `509`/`delete-objects` ref exists.
- `git log --all --grep DeleteObjects|bulk delete|#509` — empty.
- `grep -rn "DeleteObjects|DeleteResult|delete_objects|<Delete>" crates/` (excluding the new
  test) — empty; no pre-existing bulk-delete symbol.
- `gh issue view 509` — **OPEN**, milestone `0.1 Alpha`, **no linked PR**, 0 comments.
- `gh pr list --state all --search "delete objects in:title"` / `--search DeleteObjects` —
  the only "delete" PR is #340 ("Fix GC deleting live fragments", MERGED) — unrelated GC
  work, not a DeleteObjects implementation. `gh issue list --search DeleteObjects` — only
  #509 and the epic #513.

Conclusion: **no existing, closed, or rejected bulk-DeleteObjects contribution.** This is
net-new; there is no duplicate to collide with.

## Refutation (forced self-check)

- **(a) Genuine red?** YES — actually reverted and re-run this iterate. With
  `crates/gateway-s3/src/lib.rs` stashed (test kept), all 5 tests FAIL: the SDK receives
  `501 NotImplemented` / `<Code>NotImplemented</Code>` "the `delete` S3 subresource/operation
  is not supported" (the `delete` denylist catches the POST before any bulk route exists),
  and `bulk_delete_more_than_1000_keys_is_refused` asserts `left: Some("NotImplemented")` vs
  `right: Some("MalformedXML")`. Restoring the fix turns all 5 green. (Console captured in
  this session.)
- **(b) Production path?** YES — the tests drive the real `aws-sdk-s3` client against the real
  `S3Gateway`/`Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>` composition over a
  real loopback TCP listener — the exact `start_gateway`/`sdk_client` pattern the two cited
  peer files use, not a stand-in. The one raw-TCP test (malformed XML) hand-signs with the
  production `sigv4::sign`, matching `s3_object_metadata.rs`'s own precedent for a request a
  stock SDK will never construct.
- **(c) Fixture includes the fault?** YES — every test stores real objects via a real signed
  `PUT` before deleting, and asserts post-delete state with a real signed `GET` (expecting
  `NoSuchKey`), not the delete response alone. The "≥1 absent key" case is `a/never-existed`,
  a key never PUT at all — the failing element is *included*, not curated out.

## Test run (through the project's runner)

Fast red→green sanity pass first (scoped `cargo test`), then the **full aggregate gate** to
close the carry-forward's C4 item:

- `cargo test -p wyrd-server --test s3_delete_objects` — **5/5 pass** (fix applied); **0/5,
  5 failed** with fix reverted (red leg above).
- `cargo test -p wyrd-gateway-s3` — **67/67 pass** (no unit-test regression).
- `cargo test -p wyrd-server --test s3_list_objects --test s3_http_wire --test
  s3_object_metadata --test s3_head_object --test s3_streaming_trailer --test
  s3_copy_object_guard --test s3_gateway_cluster` — 24+19+2+3+11+2+1 = **62/62 pass**
  (confirms 507's routing split and every other subresource-denylist behaviour is untouched).
- `cargo fmt -p wyrd-gateway-s3 -- --check` / `cargo fmt -p wyrd-server -- --check` — clean.
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets -- -D warnings` — clean.
- **`./engine/xtask.sh ci` (the whole gate, `cargo xtask ci` in `$PDCA_WORKTREE`) — PASSED
  end-to-end**: typos, docs lint + `render_site --check`, `cargo fmt --all --check`,
  `cargo clippy --workspace`, `cargo build --workspace`, the whole test suite (159
  `test result: ok` lines, 0 failures — including every conformance suite), `cargo-machete`
  ("no unused dependencies"), `cargo deny check` (advisories + licenses + bans + sources),
  `xtask conformance` (5 valid + 6 invalid vectors pass), and the madsim DST tier —
  terminating in `xtask ci: all checks passed`. Strict failure scan of the log
  (`test result: FAILED|^error|panicked at`) returned nothing.

The full-gate run is the concrete answer to round 1's provisional C4: this is no longer
"focused red→green plus assumed aggregate" — the aggregate itself is green in this tree.

## Commit-readiness

`cargo fmt --all --check` and `clippy -D warnings` are green (both run inside the passing
`cargo xtask ci`), so the patch is already in the target formatter's canonical shape and
would survive the target's own pre-commit hooks (`dco`/`require-issue` are trailer/metadata
rules the publish step supplies, not code-shape rules). No new dependency (`cargo-machete` +
`cargo deny` both clean), no on-disk format change, no ADR/spec change.

## Out of scope (per brief, not implemented)

- `NoSuchBucket` precondition on delete (ADR-0046 decision 4, lands with #511) — a
  DeleteObjects against a bucket with no record still executes per-key, exactly as
  single-object DELETE already behaves (idempotent, no bucket check) — no behavioural
  regression, just the pre-existing gap the brief defers.
- `Content-MD5` enforcement — accept-and-ignore (resolved open question above).
- Versioned deletes (`<VersionId>`) — the parser ignores an unknown `<Object>` child
  (which a future `<VersionId>` would be); a versioned-delete request deletes the current
  object under `key`, no version semantics.
- Any change to single-object `DELETE` — none; the object-path `Method::DELETE` arm
  (`lib.rs:2162`) is untouched (confirmed by `s3_http_wire.rs`'s DELETE tests still passing).
