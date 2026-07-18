# Build notes — issue 504 / copy-object-empty-overwrite-guard (iteration 4)

## What the fix does

`CopyObject` is a `PUT /dst-bucket/key` carrying an `x-amz-copy-source` header whose
**body is empty** (the payload is the copy-source reference, not object bytes). The gateway
never read that header, so the PUT arm streamed the empty body into the destination key and
answered `200` — silently destroying the destination object (data loss).

The fix adds a guard at the very top of the `Method::PUT` arm, **before any body byte is
read**: if the request carries `x-amz-copy-source`, refuse with `501 NotImplemented`
(S3 error body, code `NotImplemented`) via the existing `error_response` helper.

- Guard: `crates/gateway-s3/src/lib.rs:577-584` (worktree; `parts.headers.contains_key(
  "x-amz-copy-source")` → `error_response(request_id, StatusCode::NOT_IMPLEMENTED,
  "NotImplemented", …)`), placed inside `Method::PUT =>` (`lib.rs:564`) ahead of the
  `content_type` read and the body stream.
- Mirrors the precedent the brief cites — the subresource guard at `lib.rs:554-561`, which
  refuses `?uploadId`/`?tagging` forms with the same `501 NotImplemented` shape and the same
  rationale ("a form we do not implement is refused, never silently mishandled"). The
  copy-source hazard is that invariant expressed in a header instead of a query.
- The header is read directly off `parts.headers` — it need **not** be in the client's
  SigV4 signed-header set for the guard to apply (brief §Scope: "detect it on the request
  headers regardless"). No change to SigV4 verification.
- Server-side copy (resolve source dirent→inode, alias the chunk map, return the source
  ETag) is issue #504 step 2, gated on #503's metadata model — out of scope (brief §Scope).

## Test (red→green)

`crates/server/tests/s3_copy_object_guard.rs` — a NEW file under `tests/` (the shape the
C4-verify classifier keys on; a co-located test would degrade to green-only,
`engine/scripts/run-verify.sh:16-19`). It drives the **real wire path** over an in-process
loopback TCP listener (redb in-memory + fs chunk store in a tempdir, production
`sigv4::sign`) — the `s3_http_wire.rs` pattern. Those harness helpers are private to each
`tests/*.rs` crate, so they are duplicated into this file (the brief explicitly instructs
adapt/duplicate, do not import).

Two tests:
1. `copy_source_put_is_refused_and_destination_survives` — the brief's core repro: PUT
   `"precious"` (200); signed PUT of the same path with `x-amz-copy-source` + empty body
   must be `501`/`NotImplemented`; a follow-up GET must still return `"precious"`
   byte-identical.
2. `ordinary_put_without_copy_source_still_stores` — a PUT with no such header still stores
   and round-trips (guard does not regress the normal path).

## Refuting my own test (forced)

- **(a) Genuine red?** Yes — verified mechanically. `engine/scripts/run-verify.sh` reverts
  the production change (keeps the test) and reran: `copy_source_put_is_refused_and_
  destination_survives` FAILS with `left: 200, right: 501` (the pre-fix data-loss behaviour),
  then GREEN with the fix applied. Gate verdict: "PASS — red without the fix, green with it."
- **(b) Production path?** Yes — the test starts the real `S3Gateway::serve` listener and
  sends raw signed HTTP/1.1 over a `TcpStream`, so the refusal comes from the production
  `handle`/PUT-arm code the patch edits. No mock, copy, or re-implementation of the guard.
- **(c) Fixture includes the fault?** Yes — the fixture PUTs a real object first, then sends
  the actual `x-amz-copy-source` request against that same key; the RED leg shows the
  destination being overwritten (200) exactly as the defect describes, so the failing
  request is present, not curated out.

## Verification evidence (addresses the iteration 1–3 carry-forward)

The recurring sign-off blocker across iterations 1–3 was **not** a code or test defect
(gates were green each time); it was that Check could not *independently rerun* the
aggregate `./engine/xtask.sh ci`, and could not mechanically inspect closed/rejected forge
state. I ran the aggregate here and record the evidence for the human at sign-off:

- `./engine/xtask.sh ci` (cd's into `$PDCA_WORKTREE`, execs `cargo xtask ci`:
  fmt `--check`, clippy `-D warnings`, build, full test suite incl. DST, `cargo deny`,
  conformance) → **`xtask ci: all checks passed`, exit 0.**
- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-server` → exit 0.
- `cargo clippy -p wyrd-gateway-s3 --all-targets -- -D warnings` → exit 0.
- `cargo clippy -p wyrd-server --test s3_copy_object_guard -- -D warnings` → exit 0.
- `engine/scripts/run-verify.sh` (red→green) → **PASS** (see §Refuting above).

I did not change the code approach from the prior iterations because the approach is correct
against the brief's Success criterion and Invariant, and gates confirmed it every round; the
open items were evidence/environment gaps, which this iteration closes by producing the
aggregate-ci run. Changing a correct, invariant-restoring guard merely to be "different"
would be wrong (`docs/principles.md` §1.2 — smallest change that restores the invariant).

## Base / wave note (important for Check)

This bundle is a **later wave**: the brief's "Repo + branch target" is `getwyrd/wyrd @ main`,
but 504 conflicts with 503 and 506 on the same `match method` PUT/GET dispatch (brief
§Ordering note), so the driver stacked it on the folded integration branch. The supplied
`$PDCA_WORKTREE` base is `pdca-integration/main` (`63a1f48`, = `origin/pdca-integration/main`),
which already contains #503 (ADR-0047 `content_type` on the PUT arm; `put_object_streaming`
now takes 4 args) and #505 (checksum-trailer framing).

Consequences:
- `patch.diff` is generated against — and its context matches — `origin/pdca-integration/main`
  (index `1855c74`), **not** raw `origin/main`. Applying it on raw `origin/main` fails
  ("patch does not apply") because #503's `content_type` block is absent there. This is the
  known wave-fold gap (harness #273, live here under `wave_mode = "stack"`;
  `docs/INTEGRATION.md` §2). The remedy is the driver exporting `$PDCA_VERIFY_BASE=
  origin/pdca-integration/main` for a wave>0 bundle; I confirmed C4-verify passes with
  `WYRD_VERIFY_BASE=origin/pdca-integration/main`.
- **For the reviewer:** if C4-verify reports "does not apply on origin/main", that is the
  #273 base-resolution gap, not a stale/broken patch — rerun with
  `WYRD_VERIFY_BASE=origin/pdca-integration/main` (or `$PDCA_VERIFY_BASE`). The fix itself is
  independent of #503/#505 (it only reads a request header and returns early); the context
  overlap is purely that all three edit the same PUT arm.

## Manual / off-Check confirmation (optional)

`aws s3api copy-object --copy-source src/k --bucket dst --key k` against a running `wyrd s3`
gateway now returns a `501 NotImplemented` instead of destroying `dst/k` (covered by the
registered "aws cli (S3 gateway round-trip)" doctor row; not required for Check).
