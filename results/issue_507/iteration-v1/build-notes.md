# Build notes — issue 507 / list-objects-v2

## What I built

ListObjectsV2 (`GET /bucket?list-type=2`) + a v1 ListObjects compat shim (`GET /bucket`)
end-to-end, replacing today's `400 InvalidRequest` for a bucket-only GET, per the brief's
design proposal (ADR-0046 normative).

Five source edits + one new test, all in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`), cited `path:line` against the target branch
(`getwyrd/wyrd @ main`, base `07d0244`):

1. **`crates/gateway-core/src/lib.rs`** — the protocol-neutral listing seam. Added
   `ListedObject { key, size, etag, modified }` (`:83`) and a default-provided
   `ObjectGateway::list_container(&self, container) -> Result<Option<Vec<ListedObject>>>`
   (`:196`). The seam speaks **container/key**, never bucket/delimiter/token — S3 vocabulary
   stays in the S3 crate (ADR-0010, ADR-0046 decision 6). `None` ⇒ no such container (wire
   maps to `NoSuchBucket`); `Some(vec![])` ⇒ existing-but-empty. The method has a **default
   of `Ok(None)`** so the crate's three existing `ObjectGateway` test doubles
   (`gateway-s3` tests) and any non-container gateway need no bespoke impl; the real
   `Gateway` overrides it. (Chosen over editing the three doubles: a default is a 3-line seam
   addition vs. ~15 lines of churn across reviewer-visible test doubles, and it is
   semantically honest — a gateway with no container concept genuinely has no containers.)

2. **`crates/core/src/metadata.rs`** — `bucket_key(name) -> "bucket:{name}"` (`:44`) and the
   immutable `BucketRecord { name, created_millis }` (`:341`), ADR-0046 decisions 1–2. This
   issue only **reads** the marker (existence); #511 writes it. Defining the key+record here
   is the "NoSuchBucket record read" the brief scopes in, and keeps #511 on the same shape.

3. **`crates/server/src/lib.rs`** — `Gateway::list_container` (`:459`): the bucket-existence
   read (`meta.get(bucket_key(container))`) **first** (ADR-0046 decision 4), then a scan of
   the per-bucket dirent prefix `dirent:{ROOT}/{container}/` of the flat `{bucket}/{key}`
   encoding (derived from how the PUT path composes the dirent name — not a new shape),
   resolving each dirent's inode for size/etag/modified, skipping missing or still-`Pending`
   inodes (matches `committed_inode`'s "only committed content is readable"), and sorting
   lexicographically (the `scan` contract is order-unspecified).

4. **`crates/gateway-s3/src/checksum.rs`** — `base64_encode` (`:144`), the inverse of the
   existing `base64_decode`, for the opaque continuation token. RFC-4648 KAT added
   (`:266`), symmetric with the existing decode vectors.

5. **`crates/gateway-s3/src/lib.rs`** — the wire layer. `bucket_scoped_path` (`:379`) splits
   bucket-scoped paths off **before** the object-path guard, leaving `split_bucket_key`
   untouched (it stays load-bearing for object verbs). A bucket GET consults the subresource
   **denylist first** (so `?acl`/`?policy`/`?versions`/`?uploads` stay `501`, protecting
   `get-bucket-acl` and 508's ListMultipartUploads red — ADR-0046 decision 1), then routes to
   `list_objects`. Grouping (`delimiter` → `<CommonPrefixes>`), the **combined** `max-keys`
   slice (Contents + CommonPrefixes counted together), and pagination all happen in **one
   place — the wire layer** (`compute_page`), over the seam's single sorted view (ADR-0046
   codex finding). XML is emitted by string building (`render_list_v2`/`render_list_v1`), no
   XML dependency. `iso8601` shares `civil_from_days` with `http_date`, so no date dep.

## Pagination / continuation-token design (the crux)

`compute_page` walks the sorted, prefix-filtered keys. A `delimiter` common-prefix is
emitted for a **whole contiguous group at once** (advancing past every key under it), so a
rollup is never split across pages and can't be duplicated. The continuation token encodes
the **last underlying object key consumed** (opaque base64 for v2; the raw key as `NextMarker`
for v1), so resuming strictly after it skips an entire already-rolled-up group cleanly —
"start after the last returned item" without the common-prefix aliasing bug. `max-keys`
defaults to 1000 and clamps to it; a non-numeric value is `400 InvalidArgument`; an
undecodable v2 token is `400 InvalidArgument` (never a silent restart from the top).

## Scope decisions stated in the brief

- **`start-after` / `fetch-owner` omitted.** Stock `aws-cli` `ls`/`sync` and the aws-sdk-s3
  `list_objects_v2` paginator drive pagination via `continuation-token` only; neither is
  required, and the SDK-driven pagination test passes without them. (The brief permits
  omitting them if unneeded — stated here as required.) `encoding-type=url` is out of scope
  per the brief.
- **v1 `NextMarker` emitted whenever truncated** (not only with a delimiter, as strict AWS
  does): a benign superset that lets a v1 client resume without reconstructing the last key.
  It's my production choice, and the v1 test asserts it.

## Test — `crates/server/tests/s3_list_objects.rs`

Drives the **wire only** via a stock `aws-sdk-s3` client over a real loopback listener on the
in-process redb-in-memory + fs-tempdir + `MemCoordination` stack (mirrors
`s3_object_metadata.rs` / `s3_gateway_cluster.rs`). Nine tests cover the whole success
criterion: sorted `<Contents>` with correct `Size` and an **independently-computed** SHA-256
`ETag`; `prefix`; `delimiter=/` → `<CommonPrefixes>` (top-level and one level deep);
`max-keys=2` pagination chaining until every key is returned **exactly once**;
`NoSuchBucket` for a marker-less bucket; empty-bucket-with-marker → empty `200`; XML-special
key round-trip; invalid `continuation-token` → `400 InvalidArgument`; v1 `Marker`-based paging.

The `bucket:{name}` marker is seeded **directly** on the store as **raw** key/value bytes
(committed before the store is moved into `Gateway::new` — the store is private and moved, so
no post-construction handle exists, per the brief's corrected seeding note). Raw bytes ⇒ the
test imports **no new production symbol**, so the C4-verify red leg fails by **assertion**,
not a compile error.

## Refute-your-own-test (forced)

- **(a) Genuine red?** YES. `git stash`-reverted all five production files (test kept) and
  re-ran: the test **compiled** and all 9 tests failed by assertion — the SDK received
  `400 InvalidRequest` ("expected a bucket-scoped object path") from the un-patched
  `split_bucket_key`/dispatch path. Restored with `git stash pop`. Green again after.
- **(b) Production path?** YES. The test hits the shipping `S3Gateway` HTTP surface over a
  loopback TCP listener with a real aws-sdk-s3 client → production `dispatch` → `list_objects`
  → `Gateway::list_container` → real redb `scan`/`get`. No mock, copy, or re-implementation.
- **(c) Fixture includes the fault?** YES. The `NoSuchBucket` test seeds a marker for `real`
  and queries `ghost` — the fixture **includes the absent bucket** rather than curating it
  out. Objects are really PUT over the wire (not fabricated in the store), and the invalid-
  token / empty-bucket / special-key cases each include the exact element under test.

## Verification run

Project runner is `./engine/xtask.sh ci` (full `cargo xtask ci`, heavy). For the fast
red→green sanity I used the pinned toolchain (1.96.0) directly:
`cargo test -p wyrd-server --test s3_list_objects` (9/9 green post-fix; 9/9 red pre-fix),
`cargo test -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-core --lib` (all green, incl. the
existing gateway-s3 test doubles compiling via the new default method), and the checksum KAT.
Commit-ready: `cargo fmt -- --check` clean and `cargo clippy --all-targets -D warnings` clean
on all touched crates (one `doc_lazy_continuation` fixed). The full `cargo xtask ci`
(fmt/clippy/build/test/deny/conformance across the whole workspace) is Check's gate.

## No new dependency

No crate added (`base64`/XML/date all hand-rolled in-tree), so no ADR-0003 audit / `deny.toml`
change — nothing human-gated here.
