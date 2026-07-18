# Build notes — issue 504 / copy-object-empty-overwrite-guard (iteration 2)

## Carry-forward from iteration 1 — what it was, how this addresses it

The previous attempt was **auto-iterated**, not rejected on merit. The sign-off rationale
records: *"Check found implementation-level items only, no architectural judgment
required."* The two named items were both **verification-environment** limitations, not
defects in the fix or its approach:

1. **C4 Verification** — *"both patched tests passed, but the asserted aggregate
   `./engine/xtask.sh ci` runner is absent from the supplied target and could not be
   independently rerun."* In **this** worktree that runner is **present and resolves
   correctly**: `xtask/Cargo.toml` exists, `cargo xtask` lists `ci` as a subcommand, and
   the driver wrapper `engine/xtask.sh` (with `$PDCA_WORKTREE` set, the path the driver
   exports) `cd`s here and execs it — verified this iteration. So the aggregate gate the
   driver's `C4-ci` runs is exercisable; the iteration-1 "absent" reading was a
   Check-side environment artifact. I demonstrated the **binding red→green** through the
   focused target `cargo test -p wyrd-server --test s3_copy_object_guard` (see
   Verification below); the full `cargo xtask ci` (fmt + clippy + build + whole-tree test
   + `cargo deny` + conformance) is heavy and is the driver's own gate to run, not a Do
   sanity pass.
2. **T4 Contribution** — *"closed/rejected forge state was unavailable mechanically."*
   This is a Check triage question (does a closed/rejected upstream PR supersede this?),
   answerable only against live forge state — it is **not a Do artifact** and nothing in
   the patch can resolve it. The brief's own Prior-art check already covered local/merged
   history and `-S x-amz-copy-source` across all branches (no prior or in-flight
   copy-object work). Forge-state confirmation is a §6 NEEDS-HUMAN for sign-off, not a
   code change.

There is **no alternative approach to switch to**: the brief's *Invariant to restore*
(“a request form the gateway does not implement is refused, never silently mishandled”)
and *Scope* (“Refuse it up front in the PUT arm, before any body is consumed, with `501
NotImplemented` via the existing `error_response` helper — mirroring the subresource
guard”) prescribe **exactly** the minimal guard below. Re-attempting the same minimal,
brief-prescribed guard is correct; the axis here is *smallest change that restores the
invariant*, and that is a single pre-body `if`.

## What I changed

`crates/gateway-s3/src/lib.rs` — inside the `Method::PUT` arm of `dispatch` (arm opens at
`crates/gateway-s3/src/lib.rs:564`), added a guard as the **first statement** of the arm,
before the `content_type` header read and before the request body stream is ever touched
(`crates/gateway-s3/src/lib.rs:577-585`):

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

This mirrors the subresource guard the brief cites (`crates/gateway-s3/src/lib.rs:548-561`)
in both shape (same `error_response` call, same `StatusCode::NOT_IMPLEMENTED` /
`"NotImplemented"` pair) and rationale (a form this floor does not implement is refused,
never silently mishandled). `parts.headers` (from `req.into_parts()`) is checked directly —
`http::HeaderMap` lookups are case-insensitive by construction, and the brief is explicit
that the header need not be in the client's SigV4 signed-header set for the guard to apply,
so no signature-verification code is touched.

I placed the guard **inside** the `Method::PUT` arm rather than beside the subresource
guard (which runs for every method, before the `match`). `x-amz-copy-source` is only a
hazard on PUT — an ordinary GET/DELETE carrying a stray copy-source header stores/deletes
nothing new — so scoping the check to the PUT arm is the narrower, more precise match for
the actual defect, without touching the GET/DELETE arms or the subresource guard's
query-based denylist.

## Test

New file `crates/server/tests/s3_copy_object_guard.rs` — a NEW `tests/*.rs` file, the shape
the C4-verify classifier keys on (`engine/scripts/run-verify.sh:93`; a co-located test
would degrade to green-only). Built on the peer harness the brief names,
`crates/server/tests/s3_http_wire.rs` (`start_gateway`, `signed_headers`, `send`,
`parse_response`, `dechunk`) — **duplicated, not imported**, since each `tests/*.rs`
compiles as its own crate and those helpers are private to `s3_http_wire.rs`. Two tests:

- `copy_source_put_is_refused_and_destination_survives` — the brief's exact repro: PUT
  `"precious"`, then a signed PUT of the same path with `x-amz-copy-source` and an empty
  body must return `501` with an S3 `<Code>NotImplemented</Code>` body, and a following GET
  must still return `"precious"` byte-identical (not empty).
- `ordinary_put_without_copy_source_still_stores` — an ordinary PUT with no such header is
  unaffected by the guard.

The test drives the **production wire path**: a real `TcpListener`/`TcpStream`, the
production `S3Gateway`/`dispatch` handler, production `sigv4::sign`, a real in-memory
`RedbMetadataStore` + tempdir `FsChunkStore` — the same composition `s3_http_wire.rs` uses.
It is import-light in the headless sense (no GUI/display), so the headless runner loads it
fine.

## Alternatives ruled out

- **Header check before the `match method` block, alongside the subresource guard.**
  Rejected: same 4-line `if`, just relocated ~15 lines earlier and applied to GET/DELETE
  where no arm reads `x-amz-copy-source` — dead code there, no line-count win, only a scope
  widening with no matching benefit.
- **`400 InvalidRequest` instead of `501`.** Rejected: the Success criterion and the
  sibling subresource guard both specify `501 NotImplemented` — this is an unimplemented
  form, not a malformed request.
- **Implementing actual server-side copy (resolve source dirent→inode, alias the chunk
  map).** Explicitly out of scope (issue #504 step 2; depends on #503's metadata model for
  the returned ETag). That would touch the metadata layer and ETag model — a materially
  larger, differently-reviewed change than the data-loss guard this bundle scopes.

## Environment note (wave-folded worktree)

`results/issue_504/stack-base` reads `pdca-integration/main`, and `$PDCA_WORKTREE`'s HEAD
(`63a1f48`) carries `pdca-integrate: issue_505` and `pdca-integrate: issue_503` on top of
`origin/main` — expected for a wave≥1 bundle. I edited and diffed against the worktree's
actual HEAD (the same base `C4-verify` resolves via `$WYRD_VERIFY_BASE` at gate time), and
confirmed the PUT arm this patch touches is unchanged by #503 (no `x-amz-copy-source`
present pre-patch; the guard applies to a clean PUT arm). All edits are confined to the one
PUT-arm hunk the brief names; the subresource guard, GET/DELETE arms, and anything #503
changed are untouched — so the "different waves so patches don't collide" ordering note
holds.

## Verification (project's runner in this worktree)

- Runner presence (addresses carry-forward C4): `cargo xtask` lists `ci`;
  `PDCA_WORKTREE=$PDCA_WORKTREE ./engine/xtask.sh` resolves into this worktree and execs
  `cargo xtask` — the aggregate gate is present and runnable here.
- Focused binding red→green, `cargo test -p wyrd-server --test s3_copy_object_guard`
  (timeout-bounded):
  - Fix applied: **2 passed**.
  - `git stash push -- crates/gateway-s3/src/lib.rs` (revert ONLY the production guard,
    keep the new test), re-run: `copy_source_put_is_refused_and_destination_survives`
    **FAILED** at `crates/server/tests/s3_copy_object_guard.rs:192` (`left: 200, right:
    501` — the pre-fix 200-and-destroyed-object behavior); `ordinary_put…` still passed.
    `git stash pop` restored the fix; re-ran **green** (2 passed).
- `cargo fmt -p wyrd-gateway-s3 -p wyrd-server -- --check` — clean (commit-ready; the
  target's own pre-commit fmt hook will pass).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings` — clean.

## Refutation (forced self-check)

- **(a) Genuine red?** Yes — reverting only the `lib.rs` guard (test kept) makes
  `copy_source_put_is_refused_and_destination_survives` fail `left: 200, right: 501`
  (captured above). The sibling ordinary-PUT test stays green either way, which is correct
  — it must not depend on the guard.
- **(b) Production path?** Yes — real `TcpListener`/`TcpStream`, the production
  `S3Gateway`/`dispatch` handler in `crates/gateway-s3/src/lib.rs`, production
  `sigv4::sign`, real `RedbMetadataStore` (in-memory) + `FsChunkStore` (tempdir). No mock
  or re-implementation of the dispatch logic.
- **(c) Fixture includes the fault?** Yes — the fixture is the unmodified production PUT
  arm; the "fault" is the absence of a guard, and the test reaches it by sending exactly
  the request shape the repro names (signed PUT with `x-amz-copy-source` + empty body) over
  the real listener. Nothing is curated out.

## External dependencies / NEEDS-HUMAN

- No new external dependency beyond what the brief lists — the test runs the in-process
  loopback stack (redb in-memory + fs chunk store in a tempdir); no Docker/FDB/etcd/aws-cli
  needed. No `NEEDS-HUMAN external dependency` marker.
- The **T4 forge-state** carry-forward item (does a closed/rejected upstream PR supersede
  this?) remains a sign-off check against live forge state — outside Do's artifacts. The
  brief's Prior-art check found no local/branch/merged prior copy-object work; the
  closed/rejected-PR confirmation is the human's §6 call at sign-off.
