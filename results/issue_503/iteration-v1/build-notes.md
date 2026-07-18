# Build notes — issue 503 / object-metadata-model

## What the change does

Implements ADR-0047 (authored in this patch): the S3 gateway now stores and returns object
metadata beyond byte size — an `ETag` (opaque lowercase-hex SHA-256 change-token), the
client's `Content-Type`, and `Last-Modified` — round-tripping through a real
`MetadataStore` commit on the `InodeRecord`.

Layers, matching the brief's Design:

1. **`InodeRecord` (`crates/core/src/metadata.rs:246-267`)** gains three flat, top-level
   `Option` fields — `etag` / `content_type` / `modified` — each `#[serde(default)]` for
   stored-record compatibility. A new `ObjectMeta` struct (`metadata.rs:246`) carries the
   trio at publication; `InodeRecord::object_meta()` collects them for the wire, and a
   `Default` impl (delegating to `new_empty()`) lets the ~50 non-metadata construction
   sites fill the new fields with `..Default::default()`.
2. **Publication vs repair (load-bearing).** Only content-publication commits set metadata:
   `commit_chunk_map_superseding{,_leased}` (`metadata.rs:499-590`) take an `&ObjectMeta`;
   create sets it via the record `commit_create` builds (`crates/core/src/write.rs:262`).
   The reconstruction/backfill paths **preserve** prior metadata via struct-update spread:
   `commit_chunk_map` (`metadata.rs:461`, `..prior.clone()`) and the three custodian
   commits (`custodian/src/backfill.rs`, `rebalance.rs`, `reconstruction.rs`,
   `..record/plan.prior.clone()`).
3. **Threading without a signature storm.** The publication metadata rides on a new
   `WritePlan.object_meta` field (`write.rs:56`), so `commit_create` / `commit_overwrite`
   keep their signatures — the ~35 test callers of those two functions are untouched. The
   composition root fills `plan.object_meta` after the data phase (the streaming digest is
   only final once the body has streamed) and before the commit.
4. **Seam (`crates/gateway-core/src/lib.rs`).** `ObjectRead` gains `etag`/`content_type`/
   `modified`; `put_object_streaming` gains a `content_type: Option<String>` param and
   returns the committed `ETag` (`Result<String>`) instead of `()`. No HTTP types leak into
   the neutral crate.
5. **Server impl (`crates/server/src/lib.rs:269-348`).** `put_object_streaming` computes the
   ETag from the SHA-256 `HashingSource` already streams (no second read), reuses it for the
   `Expected` integrity check, stamps `plan.object_meta` (digest + declared content type +
   `now_millis()` publication instant), commits, and returns the digest.
   `get_object_streaming` carries the stored metadata out on `ObjectRead`.
6. **Wire (`crates/gateway-s3/src/lib.rs`).** PUT reads the request `Content-Type` off the
   head before the body streams, threads it down, and answers `put_object_response` with the
   S3-quoted `ETag`. GET surfaces the stored content type (fallback `application/octet-stream`),
   the quoted `ETag`, and an RFC-7231 `Last-Modified`. An in-tree IMF-fixdate formatter
   (`http_date` / `civil_from_days`) avoids any HTTP-date/chrono dependency.

## Key decisions & alternatives ruled out

- **SHA-256-as-opaque-token, not MD5.** Per ADR-0047 (maintainer-confirmed at Plan): the
  digest is already streamed and vetted; MD5 would need a new dependency (ADR-0003 audit +
  `deny.toml`) and buys compatibility only with clients that violate S3's ETag-opacity rule.
- **Metadata on `WritePlan` vs. new `commit_create`/`commit_overwrite` parameters.** Adding a
  `meta` parameter to those two functions would have broken ~35 test call sites across
  `crates/{core,server,dst,custodian}/tests`; threading via `WritePlan.object_meta` (a field
  on a struct built by `plan_write`/`stream_write_data` and `Default`) limited the churn to
  the 2 production `WritePlan` literals plus the field itself. The `metadata.rs` superseding
  functions did take a new `&ObjectMeta` param — only `commit_overwrite` (production) and one
  test (`gc_delete_backstop.rs`) call them directly.
- **In-tree IMF-fixdate formatter vs. `httpdate`/`chrono`.** A new dependency is a human-only
  sign-off item (ADR-0003 + `deny.toml`); the formatter is ~25 lines (Howard Hinnant's
  civil-from-days) and DST-safe.
- **Buffered `put_object` left with default (no) metadata.** The wire path is
  `put_object_streaming`; the in-process buffered `put_object` is a test/CLI convenience.
  Stamping it too would change the stored record for every buffered-put test with no bearing
  on the Success criterion, so it is deliberately out of scope (the flat model leaves room).

## Churn note

Adding fields to `InodeRecord` breaks every struct-literal in the tree (Rust requires all
fields). This is the brief's acknowledged compiler-driven churn. Non-metadata sites (all
tests/benches) use `..Default::default()`; production preserve-paths use `..prior.clone()`;
publication paths set the fields. The bulk test-site fill was applied by a one-shot script
(brace-matched literal detection), then verified by `cargo check --workspace --all-targets`,
`cargo clippy --workspace --all-targets` (clean, `-D warnings`), and `cargo fmt --check`
(clean) — the same checks `cargo xtask ci` runs. One literal with `..` inside a *comment*
(`gc.rs:583`, "3..9 fall back") was fixed by hand.

## Refuting my own test (forced)

Test: `crates/server/tests/s3_object_metadata.rs`,
`put_answers_etag_and_get_round_trips_content_type_and_last_modified`.

- **(a) Genuine red?** YES. With the production changes reverted (`git stash` of all tracked
  source, the untracked test kept), the test compiles against the base public API and fails:
  `a PutObject response must carry an ETag header (ADR-0047); pre-fix it has none` at
  `s3_object_metadata.rs:225`. With the patch it passes (`1 passed`). Proven both directions
  via `cargo test -p wyrd-server --test s3_object_metadata` (bounded by the tool timeout).
- **(b) Production path?** YES. The test drives the real loopback HTTP listener
  (`S3Gateway::serve`) over a `TcpStream` — the same production wire path a stock SDK hits:
  SigV4 verify → `dispatch` PUT/GET arms → `Gateway::{put_object_streaming,
  get_object_streaming}` → real `commit_create`/`commit_chunk_map_superseding` on a real
  `RedbMetadataStore` + `FsChunkStore`. No mock/stand-in of the behaviour under test.
- **(c) Fixture includes the fault?** YES. The metadata genuinely round-trips through a real
  `MetadataStore` commit: the GET reads `ETag`/`Content-Type`/`Last-Modified` back off the
  record the PUT committed. The ETag is asserted equal to an *independently* computed
  SHA-256 of the object bytes (`sha256_hex`), so a wire layer echoing an arbitrary string
  would fail; the content type asserts the declared `text/plain; charset=utf-8` (not the
  hardcoded `application/octet-stream`); `Last-Modified` is checked against a strict
  29-char IMF-fixdate shape + field ranges.

## Verification run through the project's checks

- `cargo check --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets` — clean (`-D warnings` workspace policy).
- `cargo fmt --check` — clean (patch is commit-ready for the target's fmt hook).
- Targeted red→green via `cargo test -p wyrd-server --test s3_object_metadata` (tool-timeout
  bounded, no hang risk) — RED on base, GREEN with patch.

The whole-tree `cargo xtask ci` (C4-ci) and the bundle-scoped C4-verify gate re-run the real
suite at Check.

## Pre-declared sign-off item (expected, not a defect)

Per INTEGRATION §4 and the brief's Scope: this patch ships a **new ADR**
(`docs/design/adr/0047-object-metadata-model.md`, status `Accepted` pending sign-off) plus
the README index row. An ADR change is a project-defined human-only item — the reviewer will
route it to §6 NEEDS-HUMAN, and the maintainer is the accepting authority. 0047 is a *new*
record (supersedes nothing), so the ADR-immutability gate is not implicated.
