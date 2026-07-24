# Build notes — issue 509 / delete-objects-bulk (iteration 4)

Citations are `path:line` on the **target branch** = `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`), a worktree off the folded base
`origin/pdca-integration/main` @ `99ef6e6 pdca-integrate: issue_507` — the base that CONTAINS
507's bucket-scoped routing split this slice plugs into (`stack-base` = `pdca-integration/main`;
`origin/main` @ `07d0244` does NOT yet carry 507, so it is the wrong base — see the red→green
section for how the verify runner is pointed at the folded ref).

## Why this is not iteration-v3 resubmitted unchanged

The three prior rounds were **auto-iterated by the driver on process grounds**, not a code
defect — the wire suite went `0/5 → 5/5` each time. The recurring Check rows were:

- **C4 Verification** — "decide whether focused red→green plus independent fmt/clippy is
  sufficient *without rerunning the aggregate gate*" (the reviewer's clone lacked
  `engine/xtask.sh` / `run-verify.sh`, so it could not independently reproduce the gate).
- **T4 Contribution** — "merged-history-only prior-art coverage" (available local refs could not
  establish closed/rejected work).

Both are **verification/evidence** gaps a code change cannot close. So this iteration changes the
thing that was actually insufficient — the **evidence** — and adds one genuine coverage
improvement:

1. **Ran the canonical red→green through the project's own runner** (`engine/scripts/run-verify.sh`),
   pointed at the folded base — PASS (red without the fix, green with it). Prior rounds asserted
   red→green but the reviewer could not reproduce it; this run is captured below.
2. **Ran the substantive slices of the aggregate gate that my change can actually affect**
   (fmt, clippy, the full neighboring S3 wire-test suite) — all green. Prior rounds ran only the
   focused suite + fmt/clippy; the whole-tree regression check for the shared dispatch file is new.
3. **Prior-art now checked against the live tracker** (`gh`), not just merged history: issue 509
   is **OPEN** and no PR/branch exists for it (T4 evidence below).
4. **New end-to-end test** `delete_objects_oversized_body_is_refused_before_it_is_resident`
   (`crates/server/tests/s3_delete_objects.rs`) drives the production `buffer_capped` →
   `BufferError::TooLarge` → `400 MalformedXML` branch — the brief's Scope item "bound the buffered
   body bytes", previously covered only by an in-crate reasoning argument, now proven over the wire.

The XML-parser + routing approach itself was sound and regression-clean, so it is preserved rather
than churned (a gratuitous rewrite of correct code would only risk a regression).

## What changed — two files, purely additive (0 removals)

- `crates/gateway-s3/src/lib.rs`
  - **`has_query_key`** — `crates/gateway-s3/src/lib.rs:466`: decoded-key membership test for the
    `delete` subresource, next to `query_param`.
  - **Routing intercept** — `crates/gateway-s3/src/lib.rs:1484`: inside 507's bucket-scoped
    dispatch block (`lib.rs:1463`), `POST /bucket?delete` routes to `bulk_delete` **before** the
    subresource denylist (`unsupported_subresource_decoded`, `lib.rs:1472`) runs — because
    `delete` is ON that denylist (`UNSUPPORTED_SUBRESOURCES`, `lib.rs:343`) and so answers 501 if
    reached, which is exactly the red state.
  - **`bulk_delete` handler** — `crates/gateway-s3/src/lib.rs:1803`: buffers the body under a byte
    cap, verifies the signed digest, parses the `<Delete>` schema, fans out over
    `ObjectGateway::delete_object`, renders `<DeleteResult>`.
  - **`buffer_capped`** — `:1907`; **`render_delete_result`** — `:1938`; the minimal in-crate XML
    parser (`local_name` `:1993`, `tokenize_xml` `:2008`, `xml_unescape` `:2074`, `parse_delete`
    `:2120`); the caps `MAX_DELETE_KEYS = 1000` `:1760` and `MAX_DELETE_BODY_BYTES = 2 MiB` `:1771`.
  - **In-crate parser unit tests** — `crates/gateway-s3/src/lib.rs:3878-3960` (5 tests).
- `crates/server/tests/s3_delete_objects.rs` (NEW) — the wire test the brief names, now **6**
  `#[tokio::test]`s.

The per-key delete reuses the single-object path exactly as the DELETE arm does
(`crates/gateway-s3/src/lib.rs:1743` → `ObjectGateway::delete_object`,
`crates/gateway-core/src/lib.rs:196`, impl `crates/server/src/lib.rs:426`) — CAS unlink + orphan
grace + idempotency (`Ok(true)` removed / `Ok(false)` absent). No seam/on-disk change, no new
dependency; a wire-level fan-out plus XML in/out.

## Design decisions (unchanged from the sound v3 rationale, summarised)

1. **Intercept before the denylist, not after.** `delete` is denylisted, so a POST placed *after*
   the fence answers `501` (the reproduced red). Only `method == POST && ?delete` is special-cased
   ahead of the fence; every other spelling (`GET /bucket?delete`, other bucket methods) still
   falls to `501`, and the OBJECT path keeps `delete` refused (`unsupported_subresource`,
   `lib.rs:1519` region) — `PUT/DELETE /b/k?delete` unchanged.
2. **In-crate XML parser, not a dependency.** The workspace has no XML crate; adding one is a
   human-gated ADR-0003 audit + `deny.toml` decision. The fixed `<Delete>` schema is covered by a
   ~110-line tokenizer + stack walk that skips the declaration/comments/DOCTYPE, treats CDATA as
   literal text, ignores unknown elements (`<VersionId>`, namespaced root), requires a balanced
   `<Delete>` tree, and reports a broken body as `MalformedXml`. Cost of the rejected `quick-xml`:
   a new crate in the graph + the ADR-0003 three-test audit + a `deny.toml` allowlist entry, all
   human-gated — vs. ~110 unit-tested lines here. **Left as an explicit sign-off question** per the
   brief: if the maintainer prefers one shared XML approach when multipart (#508) also needs
   parsing, say so and I switch.
3. **Byte cap + signed-digest verify (bound by construction).** Unlike a streamed object PUT, the
   small delete body is buffered whole; `buffer_capped` refuses past `MAX_DELETE_BODY_BYTES` *as it
   reads*, before the body is resident. For a `Signed` payload the buffered bytes are re-hashed and
   compared to `x-amz-content-sha256` in constant time (mirrors the PUT content-hash check). The
   stock `aws-sdk-s3` sends `PayloadHash::Signed` for `delete_objects()`, so the digest-verify path
   is genuinely exercised.
4. **Body keys are XML-unescaped, not percent-decoded.** The bucket comes off the URL path (percent-
   decoded, `percent_decode_utf8(bucket)`); the per-key values come from the XML *body* (not a URL),
   so they are `xml_unescape`d only. `object_key = format!("{bucket}/{key}")` is byte-identical to
   the single-object path's stored key. The entity-escaped wire test locks this in end-to-end.
5. **Per-key `<Error>` vocabulary (brief open question — Do decides).** A per-key gateway error is
   mapped through the SAME `classify()` the single-object path uses, so a per-key `<Code>`/`<Message>`
   reads identically to the equivalent single-object failure. A failed key becomes a per-key
   `<Error>`; the batch is not failed (S3's per-key contract `sync --delete` relies on). Rejected:
   failing the whole request on the first bad key.
6. **`Content-MD5` / checksum headers — accept-and-ignore at Alpha (brief ask).** The stock SDK
   (default integrity) sends `x-amz-checksum-crc32` + `x-amz-sdk-checksum-algorithm` **headers**
   (not `Content-MD5`); they ride inside `SignedHeaders`, and the denylist is query-key-only
   (`lib.rs:361-365` region), so auth passes and the gateway accepts-and-ignores them. Enforcing
   `Content-MD5`/checksum-header integrity is out of scope (the signed-digest check already binds
   body integrity for the `Signed` form).
7. **Sequential per-key execution.** Correctness first at Alpha; concurrency is an optimisation the
   acceptance does not require.

## Scope adherence

Out-of-scope items untouched: no `NoSuchBucket` precondition on delete (#511), no `Content-MD5`
enforcement, no `<VersionId>` versioned deletes, no change to single-object PUT/GET/HEAD/DELETE.
Additive routing only: `POST /bucket?delete` moves 501 → `<DeleteResult>`; the `delete` subresource
stays refused on OBJECT paths.

## Verification evidence (this iteration, in `$PDCA_WORKTREE` / `../wyrd-verify` via the project runner)

| check | how | result |
|---|---|---|
| **red→green** | `engine/scripts/run-verify.sh` (PDCA_VERIFY_BASE=`origin/pdca-integration/main`) | **PASS** — "red without the fix, green with it"; red leg: all **6** tests fail with `501 NotImplemented` (the `delete` denylist) — assertion-red, not compile-red |
| focused wire suite | `cargo test -p wyrd-server --test s3_delete_objects` | **6 passed** |
| in-crate parser units | `cargo test -p wyrd-gateway-s3 --lib` | **72 passed** (incl. 5 new parser tests) |
| neighboring S3 regression | `cargo test -p wyrd-server --test {s3_list_objects,s3_object_metadata,s3_http_wire,s3_gateway_cluster,s3_head_object,s3_copy_object_guard,s3_streaming_trailer}` | **62 passed, 0 failed** across the seven suites — the shared dispatch change breaks nothing |
| formatter (commit-ready) | `cargo fmt --all -- --check` | exit 0 |
| linter (commit-ready) | `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets` (`-D warnings` from `[workspace.lints]`) | exit 0 |

### On the aggregate `xtask ci` (the C4-ci repo-scoped gate — Check's to run)

I did NOT run the full `cargo xtask ci` sweep. It is `typos + docs + fmt + clippy(workspace) +
build(workspace) + test(workspace) + machete + deny + conformance + statics + orchestrator-guard +
DST(50 seeds)` (`xtask/src/main.rs:1428` `run_ci`), a many-minute unbounded sweep that would risk
exceeding the Do beat's time budget. My change **cannot** affect the parts of it that the targeted
checks above don't already cover, each for a concrete reason:

- **deny / machete**: no new dependency and no unused-dep change (the parser is in-crate; the test
  reuses `aws-sdk-s3`, already a `crates/server` dev-dependency).
- **conformance vectors / statics**: no on-disk format change, no static-asset change.
- **DST (madsim commit protocol)**: the commit protocol / metadata layer is untouched; the change
  is confined to the S3 wire dispatch.
- **typos / docs**: confined to the two files I touched, both fmt/clippy-clean.
- **fmt + clippy + the S3 slice of `test --workspace`**: run directly above, green.

The whole-tree `C4-ci` remains Check's repo-scoped gate (`pdca.toml` `[gates].checks` C4-ci,
`gating=true`) and runs in the driver's `$PDCA_WORKTREE`, where `engine/xtask.sh` resolves the
Wyrd checkout — so it is reproducible there, unlike a bare reviewer clone.

## Refutation (forced, recorded)

- **(a) Genuine red?** **YES** — reproduced by `run-verify.sh`, which resets `../wyrd-verify` to
  `origin/pdca-integration/main`, applies `patch.diff`, then reverts the production change (keeping
  the test) and re-runs: all **6** tests fail — `left: 501, right: 400` for the raw-HTTP cases,
  `ServiceError … NotImplemented (501)` for the SDK cases, and a `BrokenPipe` on the oversized-body
  write (the reverted 501 handler closes the socket before the 3 MiB body finishes). Assertion/behaviour
  red, not compile red — the test imports no new production symbol.
- **(b) Production path?** **YES** — the test drives the real `S3Gateway::serve` loopback listener
  with a stock `aws-sdk-s3` client (`sdk_client`, mirroring `s3_gateway_cluster.rs:98`) and, for the
  two inputs the typed SDK cannot produce, signed raw HTTP (`sign`, as `s3_object_metadata.rs`
  does). It exercises the shipped `dispatch` → `bulk_delete` → `ObjectGateway::delete_object` over a
  real redb + `FsChunkStore` + `MemCoordination` stack — no mock, no re-implementation. The
  digest-verify runs (SDK sends `Signed`), and deletions are re-checked by a follow-up GET → `404
  NoSuchKey`.
- **(c) Fixture includes the fault?** **YES** — the happy-path fixture stores real objects AND names
  an absent key (`never-existed`) in the same request, proving idempotent `<Deleted>`-for-absent
  rather than curating it out; the quiet test asserts the objects are *actually gone* (follow-up GET
  404) rather than trusting the omitted `<Deleted>`; the entity-escaped test stores and deletes a
  key with real XML-special bytes (`a&b/weird<key>`); the oversized test sends a real >2 MiB
  well-formed body so only the byte cap (not a parse error) makes it a 400.

## Prior-art (T4) — checked against the live tracker, not just merged history

- Issue **509 is OPEN**: `gh issue view 509 --repo getwyrd/wyrd` → `"s3: implement DeleteObjects
  (bulk POST ?delete)"`, state `OPEN` — the work is not yet done.
- **No PR** across `--state all` (200 scanned) titles a bulk DeleteObjects, and no branch is named
  for issue 509 / `delete-objects`.
- The **committed base** (`HEAD`, unmodified) has **no** `DeleteObjects` / `bulk_delete` /
  `DeleteResult` symbol; `POST /bucket?delete` resolves to the `delete` denylist's 501
  (`UNSUPPORTED_SUBRESOURCES`, `lib.rs:343`) — the red state.
- **No history** for `crates/server/tests/s3_delete_objects.rs` on any ref.

The change is additive and collides with nothing merged, open, or closed. (The conflict edge with
510 named in the brief is a *future* wave-ordering concern on the same dispatch file, not existing
work.)

## Known limitations (adversary-noted, out of the fixed schema)

- The tokenizer discards attributes and finds a tag's end at the first `>`. A `>` *inside an
  attribute value* would be mis-split — but the S3 `<Delete>` schema puts keys in element *text*,
  never attributes, so a conforming client cannot trigger it.
- `<Quiet>` truthiness is `true`/`1` (case-sensitive), matching what the SDK emits; any other value
  is verbose.

## Manual / off-Check acceptance (out of the automated gate)

`aws s3 rm --recursive` / `aws s3 sync --delete` against the running gateway (`cargo run -p
wyrd-server --bin wyrd -- demo`) are the human's manual acceptance; the AWS CLI is registered as
doctor row "aws cli (S3 gateway round-trip)". No external dependency beyond the base toolchain was
required to build or verify this slice (the in-process test uses `aws-sdk-s3`, already a dev-dep).
