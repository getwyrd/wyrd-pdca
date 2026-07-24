# Build notes — issue 509 / delete-objects-bulk

## What changed (citations on the target branch = the `$PDCA_WORKTREE` off `origin/main`
with 507 folded, HEAD `99ef6e6 pdca-integrate: issue_507`)

Two files, purely additive (`git diff` shows **0 removals**):

- `crates/gateway-s3/src/lib.rs`
  - **Routing intercept** — `crates/gateway-s3/src/lib.rs:1484`: inside the bucket-scoped
    dispatch block (507's split, `lib.rs:1463`), `POST /bucket?delete` is routed to the new
    `bulk_delete` handler **before** the subresource denylist runs.
  - **`has_query_key`** — `crates/gateway-s3/src/lib.rs:466`: decoded-key membership test for the
    `delete` subresource, next to `query_param`.
  - **`bulk_delete` handler** — `crates/gateway-s3/src/lib.rs:1803`: buffers the body under a byte
    cap, verifies the signed digest, parses, fans out over `ObjectGateway::delete_object`, renders
    `<DeleteResult>`.
  - **`buffer_capped`** — `:1907`; **`render_delete_result`** — `:1938`; the minimal XML parser
    (`tokenize_xml` `:2008`, `xml_unescape` `:2074`, `parse_delete` `:2120`); the caps
    `MAX_DELETE_KEYS = 1000` `:1760` and `MAX_DELETE_BODY_BYTES = 2 MiB` `:1771`.
  - **In-crate parser unit tests** — `crates/gateway-s3/src/lib.rs:3878-3960` (5 tests: query
    match, key/quiet parse, entity unescape, malformed rejection, quiet/escape render).
- `crates/server/tests/s3_delete_objects.rs` (NEW) — the wire test the brief names.

The per-key delete reuses the existing single-object path exactly as the DELETE arm does
(`crates/gateway-s3/src/lib.rs:1713` → `ObjectGateway::delete_object`,
`crates/gateway-core/src/lib.rs:196`, impl `crates/server/src/lib.rs:426`) — CAS unlink + orphan
grace + idempotency (`Ok(true)` removed / `Ok(false)` absent), so this slice is a wire-level
fan-out plus XML in/out, no seam/on-disk change and no new dependency.

## Design decisions

1. **Intercept placement — before the denylist, not after.** `delete` is on
   `UNSUPPORTED_SUBRESOURCES` (`lib.rs:343`) and the bucket route consults that denylist first
   (`lib.rs:~1490`), so a `POST /bucket?delete` placed *after* it answers `501 NotImplemented`
   (this is exactly what the red leg reproduced). The one implemented form (`method == POST &&
   ?delete`) is therefore special-cased ahead of the fence; every other spelling
   (`GET /bucket?delete`, other bucket methods) still falls to the `501` fence, and the OBJECT
   path keeps `delete` refused via `unsupported_subresource` (`lib.rs:1519`) — `PUT/DELETE
   /b/k?delete` unchanged.

2. **In-crate XML parser, not a dependency.** The workspace has no XML crate and adding one is a
   human-gated dependency/license decision (ADR-0003 three-test audit + `deny.toml`). The
   `<Delete>` schema is tiny and fixed, so a ~110-line event tokenizer + stack walk
   (`tokenize_xml`/`parse_delete`) covers it: skips the XML declaration/comments/DOCTYPE, treats
   CDATA as literal text, ignores unknown elements (`<VersionId>`, a namespaced root), requires a
   balanced tree with a `<Delete>` root, and reports a structurally-broken body as `MalformedXml`.
   Cost of the rejected `quick-xml` alternative: a new crate in the graph + the ADR-0003 audit +
   a `deny.toml` allowlist entry, all human-gated — versus the ~110 lines here, unit-tested
   directly. Left as an explicit sign-off question per the brief: if the maintainer prefers one
   shared XML approach when multipart (#508) also needs parsing, say so and I'll switch.

3. **Byte cap + signed-digest verify (bound by construction, not by trust).** Unlike an object
   PUT (which streams), the delete body is small and buffered whole, so `buffer_capped` refuses
   past `MAX_DELETE_BODY_BYTES` (2 MiB — generous over the ~1 MiB a full 1000×1024-byte-key
   request occupies) *as it reads*, before the body is fully resident. For a `Signed` payload the
   buffered bytes are then re-hashed and compared to `x-amz-content-sha256` in constant time,
   mirroring the PUT path's content-hash check — a tampered body is rejected before any key is
   touched. **Verified what the stock SDK sends** (brief ask): a temporary probe showed
   `aws-sdk-s3 1.138 delete_objects()` sends `PayloadHash::Signed(<hex>)` (single-shot signed
   body), so the digest-verify path is genuinely exercised (the happy-path test passes *with*
   verification on). The `Streaming` arm (de-frame via `streaming::decode`) is defensive — a
   stock SDK never aws-chunks this small in-memory body.

4. **Body keys are XML-unescaped, NOT percent-decoded.** The bucket name comes off the URL path
   and is percent-decoded exactly as the object path does (`percent_decode_utf8(bucket)`), but the
   per-key values come from the **XML body**, which is not a URL — the SDK emits the literal key,
   XML-entity-escaped (`a&b` → `<Key>a&amp;b</Key>`). So keys are `xml_unescape`d only, and the
   composed `object_key = format!("{decoded_bucket}/{unescaped_key}")` is byte-identical to what
   the single-object PUT/DELETE stores. Percent-decoding the body key would be wrong (it would
   corrupt a key literally containing `%`). This is my reading of the brief's "percent-decoding /
   XML-unescaping … consistent with the object path": same *discipline* (decode what the wire form
   encodes), applied to the right wire form for each field. The entity-escaped-key wire test
   (`a&b/weird<key>`) locks this in end-to-end.

5. **Per-key `<Error>` vocabulary (brief open question — Do decides).** A per-key gateway error is
   mapped through the **same** `classify()` the single-object error path uses
   (`crates/gateway-s3/src/lib.rs:2071` after this patch), so a per-key `<Code>`/`<Message>` reads
   identically to the equivalent single-object failure (e.g. a commit-unknown → `InternalError`) —
   no second, divergent vocabulary. A failed key becomes a per-key `<Error>`; the batch is not
   failed (S3's per-key contract that `sync --delete` relies on). Rejected: failing the whole
   request on the first bad key (contradicts that contract).

6. **`Content-MD5` / checksum headers — accept-and-ignore at Alpha (brief ask).** The probe and
   the passing real-SDK tests confirm the stock SDK (default integrity) sends
   `x-amz-checksum-crc32` + `x-amz-sdk-checksum-algorithm` **headers** (not `Content-MD5`); they
   ride inside `SignedHeaders`, and the gateway's denylist is query-key-only, so auth passes and
   the gateway accepts-and-ignores them. Enforcing `Content-MD5`/checksum-header integrity is out
   of scope here (the signed-digest check already binds body integrity for the `Signed` form).

7. **Sequential per-key execution.** Correctness first at Alpha; concurrency is an optimization the
   acceptance does not require.

## Scope adherence

Out-of-scope items left untouched: no `NoSuchBucket` precondition on delete (#511), no
`Content-MD5` enforcement, no `<VersionId>` versioned deletes, no change to single-object
PUT/GET/HEAD/DELETE. Additive routing only: `POST /bucket?delete` moves from 501→`<DeleteResult>`;
the `delete` subresource stays refused on OBJECT paths.

## Refutation (forced, recorded)

- **(a) Genuine red?** YES. With the routing intercept disabled (`if false && …`) the wave base
  answers `501 NotImplemented` (the `delete` denylist), and all **5** wire tests fail by assertion
  (`left: 501, right: 400` for the raw-HTTP cases; `ServiceError … NotImplemented` for the SDK
  cases) — captured in the red run. Restored → all 5 green. This is assertion-red, not
  compile-red: the test imports no new production symbol.
- **(b) Production path?** YES. The test drives the real `S3Gateway::serve` loopback listener with
  a stock `aws-sdk-s3` client (and signed raw HTTP for the two inputs the typed SDK can't
  produce). It exercises the shipped `dispatch` → `bulk_delete` → `ObjectGateway::delete_object`
  over a real redb + FsChunkStore + MemCoordination stack — no mock, no re-implementation. The
  digest-verify runs (SDK sends `Signed`), and deletions are re-checked by a follow-up GET
  answering `404 NoSuchKey`.
- **(c) Fixture includes the fault?** YES. The happy-path fixture stores real objects AND names at
  least one absent key in the same request, proving the idempotent `<Deleted>`-for-absent
  behaviour rather than curating the absent key out; the quiet test asserts the objects are
  *actually gone* (follow-up GET 404) rather than trusting the omitted-`<Deleted>` response; the
  entity-escaped test stores and deletes a key with real XML-special bytes.

## Carry-forward addressed (iterations 1 & 2)

Both prior iterations were auto-iterated on **process/environment** grounds, not a code defect
(the wire suite went 0/5 → 5/5 both times):

- *C4 Verification — "the asserted aggregate wrapper couldn't be independently rerun (scripts
  absent from the reviewer's clone)."* From Do I cannot fix Check's environment, but I ran the
  substantive gate checks locally in `$PDCA_WORKTREE` through the project's own cargo (the same
  `cargo` `engine/xtask.sh` execs), each bounded by the harness timeout: red→green on
  `crates/server/tests/s3_delete_objects.rs` (5/5, proven both directions), `cargo fmt -- --check`
  clean, `cargo clippy --all-targets -- -D warnings` clean on both touched crates, and the 5 new
  in-crate parser unit tests green (72/72 lib tests). The full `./engine/xtask.sh ci` sweep
  (build + deny + conformance across the whole workspace) remains Check's to run — I did not run it
  to avoid a many-minute unbounded sweep, and nothing in this additive slice touches `deny.toml`,
  the on-disk format, or a dependency.
- *T4 Contribution — "prior-art / duplicate-work coverage."* This is fundamentally a human sign-off
  judgment. From the tree: there is **no** existing bulk-delete handler — `POST /bucket?delete`
  resolved to the `delete` denylist's `501` (the red leg proves it), and no
  `crates/server/tests/s3_delete_objects.rs` existed. The change is additive and collides with
  nothing merged. Establishing that no *closed/rejected* upstream attempt exists is the reviewer's
  call at sign-off.

## Known limitations (adversary-noted, out of the fixed schema)

- The tokenizer discards attributes and finds a tag's end at the first `>`. A `>` **inside an
  attribute value** would be mis-split — but the S3 `<Delete>` schema puts keys in element *text*,
  never attributes, so this cannot arise from a conforming client. Stated here rather than
  hidden.
- `<Quiet>` truthiness is `true`/`1` (case-sensitive `true`), matching what the SDK emits; any
  other value is verbose.

## Test evidence

- RED (intercept disabled): `5 failed` — `501`/`NotImplemented` where `200`/`400` expected.
- GREEN (shipped): `crates/server/tests/s3_delete_objects.rs` `5 passed`; `wyrd-gateway-s3 --lib`
  `72 passed` (incl. 5 new parser tests); `fmt --check` exit 0; `clippy --all-targets -D warnings`
  exit 0.
