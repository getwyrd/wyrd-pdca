# build-notes.md — issue 507 / list-objects-v2 (iteration 3)

## Scope of this iteration

The Iteration-2 carry-forward says the mainline is sound (red→green independently
corroborated; pagination/token/encoding probes unrefuted) — **keep the approach, fix only
three things**. I applied the preserved `iteration-v2/patch.diff` to the worktree unchanged,
then made exactly the three targeted fixes below. Nothing else in the design changed.

Base: `getwyrd/wyrd @ main`, worktree at `$PDCA_WORKTREE` = `07d0244` (wave base). Patch
regenerated with `git diff` over the full change set (iteration-v2 + the three fixes).

## Fix 1 — the gating C4-ci red: `typos` on `mis-decoding`

`typos` flagged `mis-decoding` in the `encoding-type` comment
(`crates/gateway-s3/src/lib.rs`, the `list_objects` fn). This was the **sole** deterministic
gate failure (C4-ci), caused by this patch, and it blocked the whole `cargo xtask ci` gate.

Reworded the comment to "…fails cleanly rather than **silently corrupting keys**" — no
hyphenated `mis-` token. I chose the reword over a `typos.toml` exception because the word
was gratuitous: a `typos.toml` entry would permanently whitelist a real misspelling class
for the whole repo, a larger and worse change than deleting one word. Verified:
`typos crates/gateway-s3/src/lib.rs` → clean; `typos` over the whole worktree → clean.

## Fix 2 — denylist gap: bucket subresource GETs answered with a listing document

`UNSUPPORTED_SUBRESOURCES` (`crates/gateway-s3/src/lib.rs:330`) listed `versions` (object
versions) but not `versioning` (bucket versioning config) and four other bucket-level
subresources whose query key differs from the object-level spelling. The bucket route
(`list_objects` dispatch) routes anything not on the denylist to a listing, so
`GET /bucket?versioning` was answered with `<ListBucketResult>` — a stock client parsing
`get_bucket_versioning()` dies with `XmlDecodeError: expected VersioningConfiguration but
got ListBucketResult` instead of a clean `501`.

Added the five entries the adversary named: `versioning`, `intelligent-tiering`,
`ownershipControls`, `policyStatus`, `metadataTable`. This is a pure denylist extension —
each is a bucket-level subresource this issue does not implement, so `501 NotImplemented` is
the correct answer, matching every other unsupported subresource. Added a probe test
`get_bucket_versioning_is_501_not_a_listing_document` driving the stock SDK
`get_bucket_versioning()` over the wire.

## Fix 3 — v1 `NextMarker` value + common-prefix marker resume

Two coupled defects in the v1 shim:

1. `render_list_v1` emitted `page.next_key` — the last *consumed* raw key (`a/2`) — where
   AWS's `<NextMarker>` for a delimiter rollup is the last *returned* item, the common
   prefix `a/`.
2. The resume filter was a pre-filter `o.key > marker`. A client that stores the AWS-style
   `NextMarker = "a/"` and resends it as `marker` hits `a/1 > "a/"` = true, so the whole
   `a/…` group is kept and the `a/` rollup is **re-emitted** — a duplicate.

Fix, minimal and in one place (`compute_page`):
- Added a `next_marker` field to `ListPage` = the last **returned** item (common prefix for
  a rollup, raw key for a content row). `render_list_v1` now emits `next_marker`.
- **v2 is unchanged**: `render_list_v2` still uses `next_key` (the raw last-consumed key) as
  the opaque continuation token payload — the v2 semantics the adversary probed and could not
  refute stay exactly as they were.
- Moved the resume skip from a raw-key pre-filter into the loop, applied per emitted
  item/group: a group whose common prefix `cp` satisfies `cp <= resume_after` is advanced
  past **without counting or emitting**. `cp <= resume_after` covers **both** resume shapes
  uniformly — a v1 marker that names the common prefix (`a/`) *and* a v2 token that decodes
  to the group's last raw key (`a/2`), because `"a/"` sorts at or before every `"a/…"`. So
  the single loop serves both paths and the v2 chaining behaviour is preserved (re-verified
  by the existing `list_v2_delimiter_and_max_keys_chain_without_double_emitting_a_prefix`
  test still passing).

Strengthened the test (renamed to
`list_v1_next_marker_is_common_prefix_and_resumes_without_double_emit`) to assert the
NextMarker **value** (`Some("a/")`, not merely `is_some()`) and to resume from `marker="a/"`
and assert the next page is `["b/"]` — the group is skipped, never re-emitted. Uses two keys
per group (`a/1,a/2,b/1,b/2,c/1`) so a common-prefix marker is distinct from any raw key.

## Carry-forward items already satisfied in iteration-2 (left intact)

max-keys=0 → empty non-truncated page; percent-decoded denylist on the bucket route;
delimiter+max-keys chaining under truncation; `start-after` honoured (not ignored);
`encoding-type` refused `501` (not ignored); `list-type≠2` → `400`; the seam N+1 cost
documented as explicit bounded Alpha debt. `start-after`/`encoding-type` question (brief
open q): stock `aws-cli ls`/`sync` do not require them; `start-after` is implemented,
`encoding-type` is refused loudly.

## Refute-your-own-test (forced, recorded)

- **(a) Genuine red?** Yes, both new/strengthened assertions verified red with their fix
  reverted:
  - Reverting the 5 denylist entries → `get_bucket_versioning_is_501…` fails with the exact
    adversary symptom: `left: None, right: Some("NotImplemented")` and the SDK
    `XmlDecodeError: expected VersioningConfiguration but got ListBucketResult`.
  - Reverting `render_list_v1` to `next_key` → `list_v1_next_marker_is_common_prefix…` fails:
    `left: Some("a/2"), right: Some("a/")`. (The whole file is additionally red on the wave
    base — bucket GET → 400 `InvalidRequest` — as established in iterations 1–2.)
- **(b) Production path?** Yes. All assertions drive the shipping HTTP surface through a
  stock `aws-sdk-s3` client over a real loopback listener (`get_bucket_versioning()`,
  `list_objects().delimiter().marker()`) against the production `S3Gateway` + `Gateway` +
  `RedbMetadataStore` stack. No production symbol is re-implemented in the test; the ETag
  oracle is an independent SHA-256.
- **(c) Fixture includes the fault?** Yes. The versioning test seeds a real bucket marker
  and PUTs a real object, then issues the subresource GET that previously slipped the fence.
  The v1 test seeds two-per-group keys so the common-prefix marker (`a/`) is a genuinely
  distinct value from any raw key, and the resume leg actually re-sends that marker.

## Verification run (project runner)

- `cargo build -p wyrd-gateway-s3` → clean.
- `cargo test -p wyrd-server --test s3_list_objects` → 15 passed / 0 failed (green).
- `cargo fmt -- --check` → clean; `cargo clippy -p wyrd-gateway-s3 -p wyrd-server
  -p wyrd-gateway-core -p wyrd-core --tests` → clean; `typos` (whole worktree) → clean.
  These are the fmt/clippy/typos legs of the previously-red `cargo xtask ci` gate; the sole
  prior finding (typos) is resolved.

Commit-readiness: fmt/clippy/typos all green over the touched files, so the target's own
commit hooks (which `cargo xtask ci` mirrors) have nothing to reject.
