# Build notes — issue 507 (list-objects-v2), iteration 6

## Scope of this iteration

The brief's `## Iteration 5 — carry-forward` block is the operative instruction:
the iteration-v5 mainline is **accepted** ("do NOT change the approach or regress the
23-test suite"), with **one** targeted defect to fix (sign-off §6 item 4, adversary
finding):

> `bucket_scoped_path` (`crates/gateway-s3/src/lib.rs:427-436`) uses
> `trim_start_matches('/')`, which strips ALL leading slashes, so the `Some(_) => None`
> "empty bucket segment (`//…`) → neither" arm is unreachable and its comment is false;
> a signed `GET //bucket?list-type=2` answers 200 instead of an error. Fix is a
> one-liner: use a single `strip_prefix('/')` … and add a wire test locking the chosen
> behaviour.

So this iteration = **re-apply `iteration-v5/patch.diff` verbatim** (the accepted
mainline) + the one-line `bucket_scoped_path` fix + tests locking it. Everything else in
v5 (Delta 1 `encoding-type=url`, Delta 2 `r == cp` v1-only collapse, the 23-test suite)
is preserved unchanged — I did not re-litigate the accepted approach.

## The fix

`crates/gateway-s3/src/lib.rs`, `bucket_scoped_path` (post-apply `:432`):

- `let trimmed = path.trim_start_matches('/');` → `let trimmed = path.strip_prefix('/')?;`

An HTTP origin-form request-target always carries exactly one leading `/`
(`parts.uri.path()`, `crates/gateway-s3/src/lib.rs:1408`). `trim_start_matches('/')`
greedily removed **every** leading slash, folding `//bucket` → `bucket`, so the empty
first segment vanished and the path was answered as a listing (`200
<ListBucketResult>`). `strip_prefix('/')` removes **exactly one**, so `//bucket` stays
`/bucket`, `split_once('/')` yields `("", "bucket")`, and the empty-bucket segment is
refused a listing — it falls through to the object-path guard's `400 InvalidRequest`.

Trace under the fix:
- `/bucket` → `Some("bucket")` (listing) ✓
- `/bucket/` → `Some("bucket")` (listing) ✓
- `/bucket/key` → `None` (object path) ✓
- `//bucket` → `None` (empty first segment; hits the `!key.is_empty()` object arm) ✓
- `//` → `None` (the `Some(_) => None` arm — **now reachable**, was dead) ✓
- `/` → `None` (root) ✓

I also corrected the two now-accurate comments: the object-path arm notes it now also
catches `//{key}`, and the `Some(_) => None` arm notes it is reachable once only the
single leading `/` is stripped (its `//…` claim was false before). The `strip_prefix`
returns `Option`, so `?` propagates `None` for a pathological path with no leading slash
(unreachable for real HTTP targets, but a safe partition — it falls to `split_bucket_key`
exactly as before).

Rejected the alternative "delete the dead arm + fix the comment" (the carry-forward's
other option): it changes the same number of lines but *loses* the explicit `//` →
neither rejection, leaving that case to fall through implicitly. Keeping the arm (now
live) documents the intent at the one place a reader checks it. `split_bucket_key`
(`:412`) is deliberately left untouched — it is load-bearing for object verbs and already
rejects empty buckets via its own `bucket.is_empty()` guard.

## Tests (both genuine red→green — refutation recorded below)

1. **Unit test** — `crates/gateway-s3/src/lib.rs`
   `tests::bucket_scoped_path_names_buckets_and_rejects_empty_segments` (post-apply
   `:984`). Drives the production `bucket_scoped_path` directly; the crisp discriminator
   for this defect. Asserts `//bucket`, `//`, `///` → `None` and the legitimate
   `/bucket`, `/bucket/` → `Some("bucket")`.

2. **Wire test** — `crates/server/tests/s3_list_objects.rs`
   `list_v2_double_slash_bucket_path_is_rejected_not_listed` (post-apply `:1103`). Drives
   a raw signed `GET //bucket?list-type=2` over the loopback gateway (mirrors the existing
   `get_raw` raw-HTTP harness and its SigV4-over-query signing, `s3_list_objects.rs:158`).
   Asserts the response body does **not** contain `<ListBucketResult` **and** the status
   is a 4xx — i.e. asserts the response *shape*, not the status alone. Rationale: the C4
   base answers `400` to every bucket GET, so a status-only check is vacuously green on
   the red leg; the `<ListBucketResult>` body is what cleanly separates the buggy `200`
   listing from a correct rejection. Confirmed hyper/axum preserves the `//bucket` path
   (does not collapse the double slash) — the reverted-fix run returned a full
   `<ListBucketResult>` for `//bucket`, proving the path reaches `bucket_scoped_path`
   unmolested.

### Forced refutation (recorded per Do discipline)

- **(a) Genuine red?** Yes — reverted the one-line fix (`strip_prefix('/')?` →
  `trim_start_matches('/')`) and re-ran both tests:
  - unit test FAILED: `bucket_scoped_path("//bucket")` returned `Some("bucket")`,
    assert `== None` panicked.
  - wire test FAILED: `//bucket?list-type=2` returned `200` with a full
    `<ListBucketResult>…<Contents><Key>a.txt</Key>…`.
  Restored the fix; both pass.
- **(b) Production path?** Yes — the unit test calls the real `bucket_scoped_path`; the
  wire test drives the real axum service (`Gateway::router()` fallback → `handle`
  dispatcher → `bucket_scoped_path`) over a real TCP socket with a real SigV4 signature.
  No copy/mock/re-implementation.
- **(c) Fixture includes the fault?** Yes — the wire test seeds a real `bucket:{name}`
  marker (`seed_bucket`) and PUTs a real object, so a *correct* listing WOULD succeed
  (200) for `/bucket`; the test's rejection is caused solely by the empty first segment
  of `//bucket`, not by an absent bucket. The unit test's fixture is the `//bucket` /
  `//` / `///` inputs themselves — the exact fault.

## Verification

Run through cargo in `$PDCA_WORKTREE` (Bash-tool timeout, no hang risk); the Check gate
re-runs the full `cargo xtask ci`:

- `cargo test -p wyrd-gateway-s3 --lib` → 67 passed (66 prior + 1 new).
- `cargo test -p wyrd-server --test s3_list_objects` → **24 passed** (23 v5-preserved +
  1 new), 0 failed — no regression to the accepted suite.
- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-server` → clean.
- `cargo clippy -p wyrd-gateway-s3 --lib --tests` and `-p wyrd-server --tests` →
  clean (no `-D warnings` violations).
- `typos crates/gateway-s3/src/lib.rs crates/server/tests/s3_list_objects.rs` → clean
  (iteration-2 lesson: `cargo xtask ci` runs `typos`).
- `patch.diff` applies cleanly (`git apply --check`) to base `07d0244` (origin/main,
  507's wave-0 verify base per the brief's Falsifiability field).

## Status-quo notes carried from v4/v5 (unchanged)

- `fetch-owner` is still omitted; the stock SDK paginator suite passes without it
  (brief Open questions / out-of-scope).
- No production accessor added for the test; `bucket:{name}` markers are seeded
  store-first via `RedbMetadataStore` before `Gateway::new` (brief External
  dependencies).
- No new dependency; XML is string-built.

## Headless / external-dependency status

No NEEDS-HUMAN external dependency for this change — it builds and tests in-process on
the base toolchain (redb-in-memory + fs-tempdir + loopback), the same stack v4/v5 used.
The off-Check `aws s3 ls` / `aws s3 sync` acceptance and its marker external-dependency
remain **explicitly deferred** to at least #511 (bucket-marker writes), per the sign-off
(§6 items 3 and 5 ticked) — not re-opened here.
