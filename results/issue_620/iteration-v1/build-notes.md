# Build notes — issue 620 / s3-object-key-length-limit

Target: getwyrd/wyrd @ main, `7bf7eb9`. Line numbers below are on the patched tree
(`crates/gateway-s3/src/lib.rs`), with the base location where it differs.

## What changed

1. `crates/gateway-s3/src/lib.rs:489-494` — new `const MAX_KEY_BYTES: usize = 1024`, with a
   doc comment naming the source (AWS S3 User Guide, "Naming Amazon S3 objects") and stating
   where it is and is not enforced.
2. `:1773-1775` (base `:1765-1769`) — the decoded bucket and key are bound to `bucket_name`
   / `key_name` before being joined into `object_key`. Same values as before; the split lets
   the PUT arm measure them separately. GET/HEAD/DELETE still use `object_key` unchanged.
3. PUT arm, `:1814-1837`, right after the CopyObject guard (base `:1800-1806`, the peer the
   brief names), before `content-type` is read and before the body stream is built:
   - `bucket_name.contains('/')` → `400 InvalidBucketName` (criterion 5). Placed first, so
     `/b%2Fa/<1023>` gets the bucket error rather than slipping through on its short key.
   - `key_name.len() > MAX_KEY_BYTES` → `400 KeyTooLongError` via the new helper. `len()` on
     the decoded `String` is UTF-8 bytes (criterion 2). Only the key is measured, not
     `bucket/key`.
4. `:2017-2027` — `fn key_too_long`, next to `malformed_xml` and in the same shape.
5. Bulk delete: new `DeleteRequestError::KeyTooLong` (`:1977-1980`); the parser refuses an
   entry with `key.len() > MAX_KEY_BYTES` (`:2263-2268`), right after the empty-key check
   (base `:2209-2211`, the pattern the brief names); the handler maps it to `key_too_long`
   (`:2109-2110`). Like the other parser errors it returns before any `delete_object` call,
   so the whole request is refused and no key is touched. The key is measured as `char_data`
   yields it: XML-entity-decoded once, not percent-decoded.
6. Comments kept true: the `MAX_DELETE_BODY_BYTES` sizing comment (`:501-503`) now says
   1024 bytes is the longest legal key and the parser refuses longer ones; the
   `DeleteRequestError` doc said "Both variants", now "Every variant" (`:1965-1966`); the
   `parse_delete_request` doc lists the new refusal (`:2209-2210`).

Single-object GET, HEAD and DELETE do not check the limit (criterion 4, invariant: every
existing object stays deletable). CopyObject still answers 501 before the new checks, so its
behaviour does not change.

## Why this shape

The invariant is "no over-limit key can be *created* through S3, and every object stays
*deletable*". So the refusal sits on PUT, the only creating path today. Bulk delete gets the
per-entry bound only so that the 8 MiB cap's sizing assumption holds; single DELETE stays
the way out for any over-limit object written before this fix or through `wyrd put --key`.

The `/` in decoded bucket check is on the PUT arm only, as the brief scopes it. GET/HEAD/
DELETE on `/b%2Fa/k` keep today's behaviour (they create nothing).

## Ruled out

- Checking the limit at the top of the object route (before `match method`): it would also
  refuse DELETE of an over-limit key and strand it. That is the #612 failure the brief warns
  about. Rejected on the invariant, not on cost.
- Measuring `object_key.len() - bucket.len() - 1` instead of splitting out `key_name`: same
  result, harder to read, and the bucket check needs `bucket_name` anyway.
- Mapping the bulk-delete refusal to `MalformedXML`: the brief requires `KeyTooLongError`,
  and the body is well-formed, so `MalformedXML` would mislead the client.
- Rejecting `/` in the bucket on every verb: out of scope per the brief; GET/HEAD/DELETE on
  such a path do not create anything.

## Test

`crates/server/tests/s3_key_length_limit.rs` (new file). Five `#[tokio::test]`s, one per
success-criterion leg, all over the in-process gateway on a loopback port with SigV4-signed
raw HTTP (the harness copies `s3_delete_objects.rs::send`/`post_delete_raw` and the
bucket-marker seed from `s3_list_objects.rs::seed_bucket`). Raw HTTP is used everywhere so
the test controls the exact request path (full percent-encoding, `%2F` in the bucket).
Leg 4 seeds `b/<'a' × 1025>` with `Gateway::put_object` on an `Arc` clone kept before
`S3Gateway::new`, then checks HEAD 200 → DELETE 204 → HEAD 404. Only pre-existing API is used.

## Red → green (project runner)

Run with the bundle's own C4-verify script, `engine/scripts/run-verify.sh`, against
`WYRD_VERIFY_BASE=7bf7eb9`. The default `../wyrd-verify` branch `pdca-verify` was held by
another lane's worktree (`fatal: 'pdca-verify' is already used by worktree`), so I ran it
with a private lane suffix (`PDCA_LANE=b620`) and `WYRD_VERIFY` under `$PDCA_SCRATCH`, so it
could not touch the other lanes' verify worktrees.

Result (`run-verify.sh: PASS — red without the fix, green with it`):

- GREEN, fix applied: `test result: ok. 5 passed; 0 failed`.
- RED, production reverted, test kept: `4 failed, 1 passed`. Failures, each on an assertion
  at `s3_key_length_limit.rs:194` (`assert_refused`), `left: 200 right: 400`:
  `put_of_an_over_limit_key_is_refused_and_stores_nothing` (leg 1),
  `limit_counts_utf8_bytes_of_the_decoded_key_alone` (leg 2, the `€` key),
  `bulk_delete_refuses_an_over_limit_key_and_touches_nothing` (leg 3 — base answered a
  `<DeleteResult>` reporting `ok` and the 1,025-byte key deleted),
  `put_with_a_slash_in_the_decoded_bucket_is_refused` (leg 5).
  `an_existing_over_limit_object_stays_deletable` (leg 4) passed on both legs, as the brief
  says it should: it pins the decision, it is not the red.

Also run in the cycle worktree: `cargo fmt --all` (no further changes),
`cargo clippy -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings` (clean), and
`typos` on both touched files (clean). I did not run the full `cargo xtask ci`; Check's
C4-ci does that.

## Self-refutation

- **(a) Genuine red?** Yes. The run-verify RED leg reverted the production change and
  kept the test; 4 of 5 tests failed on assertions (not a compile error — the test compiled
  against the base and 5 tests ran). Leg 4 is green on both sides by design.
- **(b) Production path?** Yes. Every request goes over TCP into the real `S3Gateway::serve`
  → object route / bulk-delete handler in `crates/gateway-s3/src/lib.rs`, backed by the real
  `Gateway` (redb + fs chunk store). No mocks; the test imports no new symbol.
- **(c) Fixture includes the fault?** Yes. The over-limit keys (1,025 `a`s, 342 × `€`), the
  `%2F` bucket alias and the over-limit bulk-delete entry are sent as-is; leg 4's fixture is a
  real over-limit object in the store, and the HEAD-before leg proves it exists, so the
  DELETE 204 is not the absent-key answer.

## Rubric self-review

- Protocol input: an over-limit key is now an explicit 400 on PUT and on bulk delete, never
  silently stored. Measured on decoded UTF-8, the form that is stored.
- Absent or unsupported entries: bulk delete refuses the whole request, no partial delete.
- Await discipline / transactions / clocks: no new awaits, no transactions, no clock reads.
- Docs currency: no port, API operation, RPC, CLI flag or persisted field is added; this
  tightens validation on existing operations. No architecture doc describes S3 request
  validation (`grep` of `docs/design/architecture/` for DeleteObjects/CopyObject/MalformedXML
  finds nothing), so no doc update.
- New crate root: none. The new test file carries `#![forbid(unsafe_code)]` like its peers.
- Test fidelity: no new destructive or concurrent path, so no DST leg needed; the change only
  refuses earlier.
