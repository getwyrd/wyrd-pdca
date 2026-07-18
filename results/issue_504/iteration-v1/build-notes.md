# Build notes — issue 504 / copy-object-empty-overwrite-guard

## What I changed

`crates/gateway-s3/src/lib.rs` — inside the `Method::PUT` arm of `dispatch` (the arm
starts at `crates/gateway-s3/src/lib.rs:564` on the pre-fix worktree HEAD), added a
guard as the FIRST statement of the arm, before the `content_type` header read and
before the request body stream is ever touched:

```rust
if parts.headers.contains_key("x-amz-copy-source") {
    return error_response(
        request_id,
        StatusCode::NOT_IMPLEMENTED,
        "NotImplemented",
        "CopyObject (x-amz-copy-source) is not supported",
    );
}
```

This mirrors the subresource guard the brief cites
(`crates/gateway-s3/src/lib.rs:548-561` pre-fix) both in shape (same `error_response`
call, same `StatusCode::NOT_IMPLEMENTED` / `"NotImplemented"` pair) and in rationale
(the comment quotes that guard's own reasoning: "a form this floor does not implement
is refused, never silently mishandled"). `parts.headers` (from `req.into_parts()` at
`lib.rs:487`) is checked directly — `http::HeaderMap` lookups are case-insensitive by
construction, and the brief is explicit that the header need not be in the client's
SigV4 signed-header set for the guard to apply, so no signature-verification code was
touched.

I placed the guard *inside* the `Method::PUT` arm rather than beside the subresource
guard (which runs for every method, before the `match`). The subresource guard is
method-agnostic (`?tagging` is dangerous on both PUT and DELETE); `x-amz-copy-source`
is only dangerous on PUT — an ordinary GET/DELETE carrying a stray copy-source header
isn't a hazard, so scoping the check to the PUT arm is the narrower, more precise
match for what's actually broken, without touching the GET/DELETE arms or the
subresource guard's own denylist (which is a query-string mechanism, not a header
one — bolting a header check onto it would be shoehorning two different mechanisms
into one function for no benefit; the PUT-arm placement costs one `if` and 0 lines of
change anywhere else).

## Test

New file `crates/server/tests/s3_copy_object_guard.rs` (shipped in this bundle as
`s3_copy_object_guard.rs`, per the C4-verify classifier's `tests/*.rs` naming
requirement, `engine/scripts/run-verify.sh:93`). Built on the peer harness the brief
names — `crates/server/tests/s3_http_wire.rs` (`start_gateway`, `signed_headers`,
`send`, `parse_response`, `dechunk`) — duplicated rather than imported, since each
`tests/*.rs` file compiles as its own crate and the helpers are private to
`s3_http_wire.rs`. Two tests:

- `copy_source_put_is_refused_and_destination_survives` — the brief's exact repro:
  PUT `"precious"`, then a signed PUT of the same path with `x-amz-copy-source` and an
  empty body must come back `501` with an S3 `<Code>NotImplemented</Code>` body, and a
  following GET must still return `"precious"` byte-identical (not empty).
- `ordinary_put_without_copy_source_still_stores` — an ordinary PUT with no such header
  is unaffected by the guard (the brief's "an ordinary PUT (no such header) still
  stores normally").

## Alternatives ruled out

- **Checking the header before the `match method` block, alongside the subresource
  guard.** Rejected: it would run the same branch for GET/DELETE too, which is dead
  code there (no `Method::GET`/`Method::DELETE` arm reads `x-amz-copy-source`) — no
  correctness gain, and it conflates a header-based PUT-only hazard with the
  subresource guard's query-based, method-agnostic one. Cost if I'd done it: the same
  4-line `if`, just relocated 15 lines earlier and made to apply to 2 more request
  methods where it does nothing — no line-count win, only a scope widening with no
  matching benefit. Kept it in the PUT arm.
- **Returning `400 InvalidRequest` instead of `501`.** Rejected: the brief's Success
  criterion and the sibling subresource guard both specify `501 NotImplemented`
  explicitly — this is a form the gateway doesn't implement, not a malformed request.
- **Implementing actual server-side copy (resolve source dirent→inode, alias the
  chunk map).** Explicitly out of scope per the brief ("issue #504 step 2 ... depends
  on the #503 metadata model for the returned ETag"). Doing that here would mean
  touching the metadata layer and the ETag model landed by #503, a materially larger
  and differently-reviewed change — the brief scopes this bundle to the guard only.

## Environment note (wave-folded worktree)

`results/issue_504/stack-base` reads `pdca-integration/main`, and `$PDCA_WORKTREE`'s
HEAD (`63a1f48`) already carries `pdca-integrate: issue_505` and `pdca-integrate:
issue_503` on top of `origin/main`. That's expected for a wave≥1 bundle (docs
`docs/INTEGRATION.md:73-78`, harness gap tracked as eduralph/pdca-harness#273): I
edited and diffed against the worktree's actual HEAD, which is the same base
`C4-verify` resolves via `$PDCA_VERIFY_BASE` at gate time, not a hand-picked
`origin/main` checkout. All edits are confined to the one PUT-arm hunk the brief
names; I did not touch the subresource guard, the GET/DELETE arms, or anything #503
already changed.

## Verification (project's own runner)

Ran directly with `cargo test` inside `$PDCA_WORKTREE` (a sanity pass per the Do
brief's instructions — the driver's own `C4-verify`
(`engine/scripts/run-verify.sh`) re-runs the binding red→green check against its own
`../wyrd-verify` worktree):

- `cargo test -p wyrd-server --test s3_copy_object_guard` — **2 passed** with the fix
  applied.
- `git stash push -- crates/gateway-s3/src/lib.rs` (reverting ONLY the production
  change, keeping the new test file) then re-ran: `copy_source_put_is_refused_and_destination_survives`
  **FAILED** (`left: 200, right: 501` — the pre-fix 200-and-destroyed-object behavior),
  `ordinary_put_without_copy_source_still_stores` still passed (as expected — the
  guard shouldn't affect it either way). `git stash pop` restored the fix; re-ran
  green again (2 passed).
- No regressions: `cargo test -p wyrd-gateway-s3` (58 passed), and
  `cargo test -p wyrd-server --test s3_http_wire --test s3_object_metadata --test
  s3_streaming_trailer` (19 + 2 + 11 passed) — the PUT arm this patch touches is
  exercised by all three.
- `cargo fmt -p wyrd-gateway-s3 -p wyrd-server -- --check` — clean, no diff.
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings` — clean.

## Refutation (forced self-check)

- **(a) Genuine red?** Yes — reverting only the `lib.rs` guard (keeping the new test)
  makes `copy_source_put_is_refused_and_destination_survives` fail with
  `left: 200, right: 501` (see above); the sibling ordinary-PUT test stays green
  either way, which is correct (it should not depend on the guard).
- **(b) Production path?** Yes — the test drives the real wire path: a real
  `TcpListener`/`TcpStream`, the production `S3Gateway::router`/`dispatch` handler
  (`crates/gateway-s3/src/lib.rs`), production `sigv4::sign`/`verify`, a real
  `RedbMetadataStore` (in-memory) and `FsChunkStore` (tempdir) — the same composition
  `s3_http_wire.rs` uses, not a mock or a copy of the dispatch logic.
- **(c) Fixture includes the fault?** Yes — the fixture is the unmodified production
  PUT arm; the "fault" here is the absence of a guard, and the test reaches it by
  sending exactly the request shape the brief's repro names (a signed PUT with
  `x-amz-copy-source` and an empty body) over the real listener; nothing is curated
  out.

## External dependencies

None beyond what the brief lists — the test is the in-process loopback stack (redb
in-memory + fs chunk store in a tempdir), no Docker/FDB/etcd/aws-cli needed. No
NEEDS-HUMAN item to raise.
