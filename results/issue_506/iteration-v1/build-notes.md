# Build notes — issue 506 / head-object

## What changed (path:line on the target base, `origin/pdca-integration/main` @ 669384c
— the wave-folded base: {504, 505} → {503} → {506}, per the brief's "Ordering note" and
the bundle's own `stack-base` file)

1. **`crates/gateway-core/src/lib.rs:67-83`** — new `ObjectMeta` struct: the metadata a
   HEAD answers (`size`, `etag`, `content_type`, `modified`), deliberately its own type
   rather than reusing `ObjectRead` (`:45-60`) with the `stream` field ignored — see
   "Alternatives ruled out" below.
2. **`crates/gateway-core/src/lib.rs:167-172`** — new `head_object` seam method on
   `ObjectGateway`, alongside `get_object_streaming` (`:141-144`) and `delete_object`
   (`:148`). Doc-comment states the contract the brief's Scope demands: metadata only,
   no fragment stream.
3. **`crates/server/src/lib.rs:40`** — import `ObjectMeta`.
4. **`crates/server/src/lib.rs:373-390`** — `Gateway::head_object` impl: calls
   `read::committed_inode(&self.meta, ROOT, key)` (the exact metadata-only lookup
   `get_object_streaming` already does at `:327` before it spawns the fragment-reader
   task at `:352-363`) and returns `None`/`Some(ObjectMeta{..})` directly — no channel, no
   `tokio::spawn`, no chunk read. This is the "stat-like read that does NOT open the
   fragment stream" the brief's Scope asks for, built by **reusing** the existing
   `committed_inode` primitive rather than adding a new one to `crates/core`.
5. **`crates/gateway-s3/src/lib.rs:75`** — import `ObjectMeta`.
6. **`crates/gateway-s3/src/lib.rs:697-738`** — new `Method::HEAD` dispatch arm, inserted
   between the GET and DELETE arms. Mirrors the GET arm's `Ok(None)` → `NoSuchKey` mapping
   (brief's citation, GET arm now at `:650-696`) exactly: same `error_response(request_id,
   StatusCode::NOT_FOUND, "NoSuchKey", ...)` call. On `Ok(Some(ObjectMeta{..}))` it builds
   the same four headers GET sets (`content-type` via the same `content_type_header`
   fallback, `content-length` from the real `size`, optional `etag` via the same
   `etag_header` degrade-not-panic helper, optional `last-modified` via the same
   `http_date`) and answers `Body::empty()` instead of `Body::from_stream(stream)`.
7. **`crates/gateway-s3/src/lib.rs:750`** — the `_` fallback's message updated from "only
   object PUT, GET, and DELETE are supported" to "...PUT, GET, HEAD, and DELETE...", since
   HEAD is no longer unsupported.
8. **`crates/gateway-s3/src/lib.rs`** (three test-only mock `ObjectGateway` impls: the two
   `NoGateway` structs and `StoredMetaGateway`, used by the request-id and
   malformed-metadata unit tests) — each gets a `head_object` method, or the crate fails to
   compile once the trait grows a new required method. `StoredMetaGateway::head_object`
   mirrors its `get_object_streaming` (same stored `etag`/`content_type`, same fixed
   `modified`, `size` hard-coded to the fixture body's length) so the malformed-etag /
   malformed-content-type unit tests it backs stay meaningful if a future HEAD unit test
   is added against it.
9. **New test file: `crates/server/tests/s3_head_object.rs`** (a NEW file under `tests/`,
   the shape C4-verify's classifier keys on, per the brief's "Test file" field).

No change to PUT/GET/DELETE behaviour, no route table change (the router is a single
`.fallback(handle::<G>)`, `crates/gateway-s3/src/lib.rs:166` — HEAD reaches `dispatch`
exactly as every other verb does), no logging change (the brief's "already classifies HEAD
as body-less" citation, `finish_response`'s `bodyless` check at `crates/gateway-s3/src/
lib.rs:394-410` (the `method == Method::HEAD` arm of the check is at `:406`), already
matches on HEAD — nothing to touch there).

## Why this shape (and what I considered, with cost)

- **Reusing `committed_inode` instead of adding a new core primitive.** `get_object_streaming`
  (`server/src/lib.rs:326-371`) already splits cleanly into "resolve the inode" (`:327-329`,
  cheap) and "stream the fragments" (`:341-363`, expensive) — HEAD needed only the first
  half. Extending the seam with `head_object` and wiring it straight to the existing
  `read::committed_inode` is a ~25-line addition (trait method + one impl) versus inventing
  a second core-crate function that duplicates `resolve` + `read_inode` + the
  `InodeState::Committed` filter `committed_inode` already does (`crates/core/src/
  read.rs:523-538`) — strictly more code for identical behaviour, so ruled out.
- **`ObjectMeta` as its own type vs. reusing `ObjectRead` with a dummy/`Option` stream field.**
  Making `ObjectRead.stream` an `Option<ObjectStream>` (or unioning the two) would force
  every existing GET call site (`crates/gateway-s3/src/lib.rs:650-696`, and the 3 mock impls)
  to handle a stream that might not be there, widening the change to files this brief's
  Scope explicitly keeps untouched ("out of scope: ... any change to PUT/GET/DELETE
  behaviour"). A new 4-field struct is +17 lines in `gateway-core` and touches no GET code
  at all — smaller AND keeps GET's contract exactly as it was, so this is not a "heavier
  vs. lighter" trade-off, it's strictly the smaller diff that also satisfies the brief's
  explicit out-of-scope line.
- **Not opening a stream and dropping it (the brief's stated fallback).** Considered and
  rejected: `get_object_streaming` unconditionally spawns a `tokio::spawn` reader task
  (`server/src/lib.rs:352-363`) the moment it resolves a present key, so "open then drop"
  would still pay a channel allocation and a spawned task per HEAD, and still needs a
  chunk_map walk if the drop happens after the task started reading — the brief's Scope
  names this as "the fallback, not the target" specifically because it does not avoid the
  cost HEAD exists to avoid. The `head_object` seam method costs one `committed_inode` call
  and nothing else, which is strictly cheaper and was no harder to build.
- **Registering a HEAD route on the router.** Not needed — checked the citied composition:
  `Router::new().fallback(handle::<G>)` (`crates/gateway-s3/src/lib.rs:166`) has no
  per-method route table; `dispatch`'s `match method` (now including `Method::HEAD`) is the
  only place a verb is recognized. No router change was made or needed.

## Three refutation questions (forced, per the Do contract)

**(a) Genuine red?** Yes — proven by the project's own `C4-verify` gate
(`engine/scripts/run-verify.sh`), not a hand-rolled command. Ran:
```
PDCA_BUNDLE=results/issue_506 PDCA_VERIFY_BASE=origin/pdca-integration/main \
  ./engine/scripts/run-verify.sh
```
(`PDCA_VERIFY_BASE` set to `origin/pdca-integration/main` because this bundle's
`stack-base` file names that wave-folded branch — the same value `gates.py:341-350`
computes from it for a real driver run, and the same base the brief's "Ordering note"
says 506 is built on.) Output:
```
run-verify.sh: GREEN — cargo test -p wyrd-server --test s3_head_object (fix applied)
test result: ok. 3 passed; 0 failed; ...
run-verify.sh: RED — cargo test -p wyrd-server --test s3_head_object (production reverted, test kept)
test result: FAILED. 0 passed; 3 failed; ...
  left: 405  right: 404   (absent-key HEAD)
  left: 405  right: 200   (HEAD-then-GET/DELETE unaffected)
  left: 405  right: 200   (stored-object HEAD)
run-verify.sh: PASS — red without the fix, green with it.
```
All three failures are the exact `405` the brief's Falsifiability/Repro sections predict.

**(b) Production path?** Yes. The test drives the real loopback listener
(`S3Gateway::serve`, `crates/gateway-s3/src/lib.rs`) fronting the real `wyrd_server::Gateway`
— the same production `dispatch`/`head_object`/`committed_inode` path `s3_http_wire.rs`
drives for GET/PUT/DELETE. No mock gateway is used in the new test file (the mocks
touched in this patch are pre-existing unit-test fixtures in `gateway-s3/src/lib.rs`,
needed only so the crate keeps compiling against the widened trait).

**(c) Fixture includes the fault?** Yes. `signed_head_of_a_stored_object_...` PUTs a real
object through the real wire path first, then HEADs it — the "object present" case is not
curated away. `signed_head_of_an_absent_key_...` HEADs a key that was never PUT, so the
`Ok(None)` → `404` branch is genuinely exercised, not assumed.

## Other verification run

- `cargo fmt -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server` — reformatted the new
  test file to the project's style; no other file needed reformatting. Patch ships
  post-format.
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests --all-targets
  -- -D warnings` — clean, no warnings.
- `cargo test -p wyrd-gateway-s3` — all 58 pre-existing tests still pass (the 3 mock-impl
  edits didn't break any of them).
- `cargo test -p wyrd-server --test s3_http_wire` — all 19 pre-existing tests still pass
  (GET/PUT/DELETE behaviour unchanged, per the brief's out-of-scope line).
- `cargo test -p wyrd-server --test s3_object_metadata` — both #503 tests still pass.

## External dependencies

None beyond what the brief declares — the in-process loopback stack, no external service.

## Scratch discipline

No scratch checkouts were created for this Do beat: all edits were made directly in
`$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt`, the driver-provided worktree),
and the red→green proof ran through `./engine/scripts/run-verify.sh`, which manages its own
dedicated `../wyrd-verify` / `pdca-verify` branch (a standing project resource the script
resets each run, not a builder-chosen scratch path) — nothing to `rm -rf`.
