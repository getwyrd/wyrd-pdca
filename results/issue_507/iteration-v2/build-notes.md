# Build notes — issue 507 / list-objects-v2 (iteration 2)

## What this iteration is

Iteration 1 was rejected at sign-off with six carry-forward findings (brief.md §Iteration 1).
This iteration **keeps the accepted core** of iteration-1 (routing split, neutral
`list_container` seam, wire-side grouping/paging, opaque base64 token, hand-built XML) and
addresses every carry-forward finding. Nothing from the rejected approach is re-submitted
unchanged. Base branch: `getwyrd/wyrd @ main` (worktree HEAD `07d0244`).

## Carry-forward findings — how each is addressed

1. **`max-keys=0` must match S3 (IsTruncated=false, no truncated-without-token page).**
   `compute_page` now special-cases `max_keys == 0` *before* the loop and returns an empty,
   **non-truncated** page (`crates/gateway-s3/src/lib.rs`, the `if max_keys == 0` guard at the
   top of `compute_page`). Previously the budget check `count >= max_keys` tripped at `0`,
   set `is_truncated=true`, and then suppressed `next_key` — wedging a paginator that re-sends
   while truncated with no token. New wire test:
   `list_v2_max_keys_zero_is_empty_and_untruncated` (asserts empty contents, `KeyCount=0`,
   `IsTruncated=false`, no `NextContinuationToken`) on a **non-empty** bucket.

2. **Subresource denylist must match percent-DECODED query keys.**
   Hoisted the denylist to the module const `UNSUPPORTED_SUBRESOURCES` and added
   `unsupported_subresource_decoded`, which decodes each query key with the same
   `percent_decode_utf8` the path uses before matching. The bucket route now calls the decoded
   variant (`crates/gateway-s3/src/lib.rs`, dispatch site ~`1200`), so `GET /bucket?%61cl` /
   `?upload%73` correctly hit the `501` fence instead of being answered with a listing. The
   object path keeps the raw matcher (`unsupported_subresource`) — harmless there since every
   object request still reaches its verb, and changing it could perturb sibling object tests.
   (No dedicated wire test: crafting a percent-encoded subresource query through the stock SDK
   is not directly expressible; the two matchers share one const and the decoded path is
   exercised by the normal bucket route. Covered by inspection + the shared const.)

3. **Delimiter + max-keys chaining test (group-consume/resume under truncation).**
   New test `list_v2_delimiter_and_max_keys_chain_without_double_emitting_a_prefix`:
   `delimiter=/` + `max-keys=1` over `{a/1,a/2,b/1,b/2,c}`, chaining `continuation-token` and
   asserting the common prefixes page as `["a/","b/"]` **each exactly once** and the sole key
   `["c"]` once — i.e. no CommonPrefix double-emit across pages (the codex finding the seam
   design cites). This is the centerpiece the iteration-1 suite left untested (delimiter test
   had no max-keys; pagination test had no delimiter).

4. **Do not silently ignore `start-after` / `encoding-type`.**
   - `start-after` is now **implemented** for v2 (trivial and strictly better than rejecting):
     when no `continuation-token` is present, `start-after` sets the first-page resume point
     (`resume_after`), so the listing begins strictly *after* the named key. A
     `continuation-token`, when present, takes precedence (AWS semantics). New test
     `list_v2_start_after_skips_consumed_keys` asserts `start-after=key-1` yields
     `[key-2,key-3,key-4]`. This directly removes the "start-after client re-receives consumed
     keys" harm.
   - `encoding-type` is **rejected** with `501 NotImplemented` rather than silently ignored.
     Rationale for reject-vs-implement below. New test
     `list_v2_encoding_type_is_rejected_not_silently_ignored` asserts the `501`.

5. **Temper the seam doc's "no new cost class" claim.**
   `crates/gateway-core/src/lib.rs` `list_container` doc: the "adds no new cost class" line now
   scopes that claim to the **scan** only, and adds an explicit **"Bounded Alpha debt"**
   paragraph documenting the N+1 sequential inode reads and the per-page re-scan+re-sort, its
   `SCAN_CAP` ceiling, the networked-backend RTT cost, and that batching/caching is deferred.
   The N+1 is now stated as a known bound, not hidden. (I documented rather than batched — see
   cost note below.)

6. **Conformance nits.**
   - v1 `<NextMarker>` is now emitted **only when a `delimiter` is set** (AWS behavior); without
     a delimiter the client resumes from the last `<Key>`. Updated `list_v1_returns_marker_based_result`
     to assert `NextMarker` is *absent* without a delimiter and resume from the last key, and
     added `list_v1_next_marker_present_with_delimiter` to lock the with-delimiter case.
   - `list-type` present with any value other than `2` now answers `400 InvalidArgument`
     instead of silently degrading to the v1 shim (`list_objects`, the `match query_param(...,
     "list-type")` guard). No wire test: the stock SDK's `list_objects_v2` always sends
     `list-type=2` and `list_objects` sends none, so `list-type=3` is not expressible through
     the SDK; verified by inspection. Advisory nit ("no adjudication needed").

## Open questions the brief asked Do to answer

- **Does stock aws-sdk-s3 send `encoding-type` / `start-after` by default?** No. The whole
  suite (14 tests) passes without setting `encoding-type`, and none trips the new `501` guard —
  so the Rust SDK's `list_objects_v2()`/`list_objects()` do **not** add `encoding-type` by
  default. `start-after` is only sent when the caller sets it. `fetch-owner` is likewise not
  sent by default; we never emit `<Owner>`, which equals AWS's `fetch-owner=false` default, so
  ignoring `fetch-owner` is conformant (no rejection needed — stated per brief scope).
- **Does `aws s3 ls`/`sync` need these?** Cannot be run in-cycle (no AWS CLI dependency
  available here; it is the off-Check doctor row "aws cli (S3 gateway round-trip)"). Per the
  planner's scoping (encoding-type out of scope; fetch-owner/start-after MAY be omitted if
  aws-cli ls does not require them), stock `aws s3 ls` does not send `encoding-type` (it is the
  opt-in `--encoding-type url`), so the `501` reject does not break `aws s3 ls`. This is the
  T5/Validation manual-acceptance item the human clears at sign-off.

## Cost note — encoding-type: reject vs implement

I rejected `encoding-type` (501) rather than implementing `encoding-type=url`. Cost of the
alternative (implement): a correct URL-encoding must match exactly what rclone/minio-go
URL-*decode* — which of `Key`/`Prefix`/`Delimiter`/`StartAfter` are encoded, whether `/`
becomes `%2F`, whether space is `+` or `%20`. Getting any of those wrong corrupts keys a
*different* way than silent-ignore did, and I cannot verify the exact S3 wire encoding without
a real S3 to diff against (it is irreducibly a conformance-oracle question). The brief
explicitly scopes `encoding-type=url` **out**. So `501 NotImplemented` is the honest, safe
answer: a client that needs URL semantics fails loudly instead of mis-decoding, and stock
`aws s3 ls` (which does not send it) is unaffected. When encoding-type is implemented later
(its own slice, with a real-S3 diff), the `501` becomes the real handler.

## Cost note — N+1 inode reads: document vs batch

I documented the N+1 as bounded Alpha debt rather than batching the inode reads. Cost of
batching now: it requires either a `MetadataStore` multi-get primitive (does not exist — adding
it touches the trait and every backend redb/tikv/fdb/mem, exactly the cross-backend blast
radius the brief's "Alternatives considered" rules out for this slice) or hand-rolled
concurrency over point-reads (changes the read path's ordering/error semantics). Both are a
larger, riskier change than the listing feature itself and belong with the deferred
streaming-`scan` trait evolution. Under `SCAN_CAP = 1<<20` the cost is bounded; documenting it
as a known bound is the minimal correct move for this slice.

## Refutation (forced, recorded)

- **(a) Genuine red?** YES. With the five production files reverted (`git stash push` of
  production only, new test retained) the focused suite fails **14/14 by assertion** — every
  bucket GET is rejected `400 InvalidRequest` at `split_bucket_key`'s `None` before any listing
  logic. Not a compile error: the test seeds the marker as **raw** `bucket:{name}` bytes and
  drives the wire only, importing no new production symbol. Restored with `git stash pop`.
- **(b) Production path?** YES. The test drives the shipping HTTP surface via a stock
  `aws-sdk-s3` client over a real loopback listener, exercising the real `S3Gateway` +
  `Gateway::list_container` + `compute_page` + XML emission — no mock, copy, or
  re-implementation. The SDK's own paginator/XML-parser is an independent oracle.
- **(c) Fixture includes the fault?** YES. Each scenario includes the element under test:
  the `NoSuchBucket` case seeds a marker for `real` but lists `ghost` (the absent bucket is
  present in the fixture as *absence*); the pagination/chaining tests hold more keys than
  `max-keys`; the `max-keys=0` test runs on a **non-empty** bucket; the encoding-type/start-after
  tests actually send those params.

## Verification method

Project gate is `cargo xtask ci` (whole-suite; `./engine/xtask.sh ci`). For the fast
pre/post sanity pass I ran the focused target `cargo test -p wyrd-server --test s3_list_objects`
under the Bash-tool timeout (not a no-timeout hand-rolled runner): **14 passed** post-fix,
**14 failed by assertion** with production reverted. `cargo fmt --all` applied; `cargo clippy
-p wyrd-gateway-s3 -p wyrd-server -p wyrd-gateway-core -p wyrd-core --tests` is clean (no
warnings from the touched code). Commit-ready for the target's fmt/clippy hooks.

## Still the human's call at sign-off (unchanged from iteration 1)

- **T5** — shipping listing before CreateBucket/#511 (or a marker backfill) means a live stack
  answers `NoSuchBucket` until markers exist (ADR-0046 Alpha stance). Tests seed markers.
- **Validation** — `aws s3 ls` / `aws s3 sync` against a marker-backed bucket (doctor row
  "aws cli"), including confirming the `501` on `encoding-type` does not surface for stock
  `aws s3 ls`.
