# Build notes — issue 504 / copy-object-empty-overwrite-guard (iteration 3)

## What the fix does

`CopyObject` is `PUT /dst/key` carrying an `x-amz-copy-source` header. The gateway never
read that header, so the PUT arm treated it as an ordinary `PutObject` and streamed the
request **body** — which a copy request sends empty — into the destination key, answering
`200`: silent data loss.

The fix adds a guard as the **first statement** of the `Method::PUT` arm, before any body
byte is consumed: if `x-amz-copy-source` is present on the request headers, return
`501 NotImplemented` via the existing `error_response` helper.

- Change: `crates/gateway-s3/src/lib.rs:564-585` (worktree base `pdca-integration/main`,
  HEAD `63a1f48`). The guard sits immediately inside `match method { Method::PUT => {`
  (`:563-564`), before the `content_type`/body read (`:586` onward).
- It mirrors the existing subresource guard's shape and rationale
  (`crates/gateway-s3/src/lib.rs:548-561`): "refuse a form we do not implement rather than
  mishandle it." `x-amz-copy-source` is the same hazard expressed in a header rather than a
  query — this is the brief's named **Invariant to restore**, so the change is the smallest
  one that restores it (refuse up-front), not a body-path workaround.
- Read directly off `parts.headers` — the header need not be in the client's SigV4
  signed-header set for the guard to apply (brief §Scope: "detect it on the request headers
  regardless"). No change to SigV4 verification.

Server-side copy (issue #504 step 2 — resolve source dirent/inode, alias the chunk map,
return the source ETag; gated on #503's metadata model) is explicitly out of scope.

## Base note (why line numbers differ from the brief)

The brief cites `origin/main` line numbers (PUT arm `:558-598`, subresource guard
`:542-555`). This is a wave≥1 build: the worktree base `pdca-integration/main` already folds
in #503 (ETag/content-type, ADR-0047) and #505, which shifted the PUT arm down. Citations
above are against the **worktree base actually being patched**. The guard is still the first
statement of the PUT arm, ahead of the #503 content-type read, so no hunk collision with
#503's edits — consistent with the brief's ordering note (different waves).

## Test

`crates/server/tests/s3_copy_object_guard.rs` (new file — the `tests/*.rs` shape the
C4-verify classifier keys on, `engine/scripts/run-verify.sh:93`). It drives the **real wire
path**: an in-process loopback `S3Gateway::serve` over an ephemeral TCP listener, redb
in-memory metadata + fs chunk store in a tempdir, production `sigv4::sign`. The private
harness helpers (`start_gateway`, `signed_headers`, `send`, `parse_response`, `dechunk`) are
duplicated from the peer `crates/server/tests/s3_http_wire.rs` because each `tests/*.rs`
compiles as its own crate and cannot import them (brief §Citations expected).

- `copy_source_put_is_refused_and_destination_survives` — the core repro: PUT `"precious"`
  (200); signed PUT of the same path with `x-amz-copy-source` + empty body must be `501`
  with `<Code>NotImplemented</Code>`; a follow-up GET must return `"precious"` byte-identical.
- `ordinary_put_without_copy_source_still_stores` — an ordinary PUT (no header) still stores
  and round-trips, so the guard doesn't over-refuse.

Import-light: no GUI/display dependency; tokio TCP loopback completes in ~50 ms. Headless-safe.

## Red→green (refuting my own test — all three answers)

Run in the worktree (`$PDCA_WORKTREE`), the same tree `./engine/xtask.sh ci` gates:
`cargo test -p wyrd-server --test s3_copy_object_guard`.

- **(a) Genuine red?** YES. With the guard `git stash`-reverted, the suite is RED:
  `copy_source_put_is_refused_and_destination_survives` fails at the 501 assertion —
  `left: 200, right: 501` (the pre-fix behaviour: request accepted, destination destroyed).
  Restored → both tests pass. So the test binds the objective, not an adjacent proxy.
- **(b) Production path?** YES. The test connects a real TCP client to the production
  `S3Gateway::serve` loop and the production `dispatch`/PUT arm that the fix edits — no mock,
  copy, or re-implementation of the handler. `sigv4::sign` is the production signer.
- **(c) Fixture includes the fault?** YES. The fixture first PUTs the real destination object
  (`"precious"`); the copy request then targets that same live key. The assertion proves the
  *actual* destination survives — the failing element (the object that pre-fix gets
  overwritten to empty) is present, not curated out.

## Gates run locally (worktree, commit-readiness)

- `cargo test -p wyrd-server --test s3_copy_object_guard` → 2 passed (green post-fix; red
  pre-fix as above).
- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-server` → exit 0 (commit hook clean).
- `cargo clippy -p wyrd-gateway-s3 --tests -- -D warnings` → clean.
- `cargo clippy -p wyrd-server --tests -- -D warnings` → clean.

Full `cargo xtask ci` (fmt/clippy/build/all-tests/deny/conformance + DST) is Check's gating
run; the targeted red→green above is the Do-beat sanity pass.

## Addressing the iteration 1 & 2 carry-forward

Both prior rounds were auto-iterated on **verification-tooling** notes, not on the fix's
correctness or approach — the reviewer found "implementation-level items only, no
architectural judgment required." I did **not** re-attempt any rejected design (there was
none rejected on merits); the guard shape is the brief's specified minimal fix. I addressed
the two recorded blockers:

1. **C4 Verification — "the aggregate `./engine/xtask.sh ci` runner is absent from the
   supplied target and could not be independently rerun."** This was a Check-side base
   artifact. In *this* worktree base (`pdca-integration/main`, HEAD `63a1f48`) the runner is
   present: `engine/xtask.sh` delegates `cargo xtask` into `$PDCA_WORKTREE`, and the `xtask`
   crate with the `ci` subcommand exists at `xtask/src/main.rs:106` (`Some("ci") =>
   run_ci()`), a workspace member (`Cargo.toml:31`). So Check can now run the full
   `./engine/xtask.sh ci` against the same tree. I independently reran the targeted red→green
   plus fmt/clippy in that tree (above) to give Check a reproducible starting point.

2. **T4 Contribution — "closed/rejected forge state was not mechanically available."** This
   is a forge-access limitation of the automated Check environment, not something the Do beat
   can resolve from inside the worktree: I have no network/`gh` access to enumerate closed or
   rejected GitHub PRs. The brief's own prior-art check already searched affected-path history
   across all refs and `git log -S x-amz-copy-source` and found no prior or in-flight
   copy-object work (brief §Prior-art check). Clearing the *forge* closed/rejected state is a
   human sign-off step (INTEGRATION.md §10 — maintainer review), so it correctly remains a
   NEEDS-HUMAN for §6 rather than a Do-beat fabrication.

## Alternatives considered / ruled out

- **Implement server-side copy now.** Out of scope per the brief and gated on #503's metadata
  model for the returned ETag. Wave-0 here is only the data-loss guard.
- **Return `400 Bad Request` instead of `501`.** `501 NotImplemented` is the precedent the
  sibling subresource guard sets for "a form this floor does not implement"
  (`lib.rs:557`), and matches the S3 semantics for an unimplemented operation. Consistency
  with the existing guard wins.
- **Guard deeper (after body read) or in the write path.** Rejected: the brief requires
  refusing *before any body byte is consumed* so the destination is never touched. Guarding
  as the first PUT statement is both the minimal restore of the invariant and the only place
  that guarantees the body stream is never opened.
