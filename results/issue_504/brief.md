# Brief — issue 504 / copy-object-empty-overwrite-guard

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** copy-object-empty-overwrite-guard
- **Defect:** `CopyObject` (`PUT /dst-bucket/key` with an `x-amz-copy-source` header) is
  silently mishandled as an ordinary `PutObject`: the gateway never reads
  `x-amz-copy-source` (zero mentions anywhere in `crates/gateway-s3/` — verified by grep on
  `origin/main`), so the PUT arm (`crates/gateway-s3/src/lib.rs:558-598`) streams the
  request **body** — which a copy request sends empty — into the destination key and
  answers `200`. A client that issues a copy gets a success response and a **destroyed
  destination object** (data loss).
- **Success criterion:** a SigV4-signed `PUT /bucket/key` carrying an `x-amz-copy-source`
  header is refused with `501 NotImplemented` (S3 error body, code `NotImplemented`) and
  the destination object's prior content is left byte-identical; an ordinary PUT (no such
  header) still stores normally. Demonstrable by C4-verify: the shipped test is red on the
  base (today the request returns 200 and the destination becomes empty) and green with
  the patch.
- **Falsifiability:** RED is demonstrable today on the in-process loopback harness
  (`crates/server/tests/s3_http_wire.rs` pattern: real TCP listener, redb in-memory +
  fs chunk store in a tempdir, production `sigv4::sign`): PUT an object, then send a
  signed PUT with `x-amz-copy-source` and an empty body — on `origin/main` the response
  is 200 and a subsequent GET returns zero bytes. No external service or topology needed.
- **Invariant to restore:** a request form the gateway does not implement is **refused,
  never silently mishandled** — the gateway's own precedent for exactly this class:
  the subresource guard refuses `?uploadId`/`?tagging` forms with `501 NotImplemented`
  precisely because dispatch-by-method-only would otherwise "silently OVERWRITE the whole
  object … both returning 2xx" (`crates/gateway-s3/src/lib.rs:542-555`). `x-amz-copy-source`
  is the same hazard expressed in a header instead of a query.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 503, 506
- **Ordering note:** no build-on dependency, but 503 (metadata/ETag: edits the PUT/GET arms)
  and 506 (adds a HEAD arm) edit the SAME `match method` dispatch in
  `crates/gateway-s3/src/lib.rs:557-631` this fix adds its guard to — schedule into
  different waves so the patches don't collide on a shared hunk. This is the wave-0
  data-loss guard; the full server-side copy (issue step 2) comes later, after #503's
  metadata model.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** the one defect — a PUT carrying `x-amz-copy-source` must not fall through to
  the store-the-body path. Refuse it up front in the PUT arm, before any body is consumed,
  with `501 NotImplemented` via the existing `error_response` helper — mirroring the
  subresource guard's shape and rationale. / out of scope: implementing server-side copy
  (issue #504 step 2 — resolve source dirent→inode, alias the chunk map; it depends on the
  #503 metadata model for the returned ETag and is a separate slice), multipart copy
  (`x-amz-copy-source-range`), and any change to SigV4 verification (the header need not be
  in the client's signed-header set for the guard to apply — detect it on the request
  headers regardless).
- **Repro instruction:** on `origin/main`, bring up the loopback S3 gateway as
  `crates/server/tests/s3_http_wire.rs` does; signed `PUT /b/k` with body `"precious"`
  (200); then signed `PUT /b/k` with header `x-amz-copy-source: /b/other` and an empty
  body → observe `200`; signed `GET /b/k` → body is now empty. Equivalent CLI repro:
  `aws s3api copy-object --copy-source src/k --bucket dst --key k` against a running
  `wyrd s3` gateway destroys `dst/k`.
- **External dependencies:** none — the C4 test runs the in-process loopback stack (redb
  in-memory, fs chunk store in a tempdir); an optional off-Check aws-cli confirmation is
  already covered by the registered "aws cli (S3 gateway round-trip)" doctor row.
- **Test file:** crates/server/tests/s3_copy_object_guard.rs (a NEW file under a `tests/`
  dir — the shape the C4-verify classifier keys on; a co-located test would degrade to
  green-only, engine/scripts/run-verify.sh:93)
- **Citations expected:** Do must cite path:line on `origin/main` for every change.
  Composition cues (each MAY be opened, nothing else): mirror the subresource refusal —
  `crates/gateway-s3/src/lib.rs:548-555` (guard placement + `error_response(request_id,
  StatusCode::NOT_IMPLEMENTED, "NotImplemented", …)` shape); build the test on the peer
  harness `crates/server/tests/s3_http_wire.rs` (`start_gateway_with_handle`,
  `signed_headers`, `send`, `parse_response` — drive the real wire path with the
  production `sigv4::sign`; these helpers are PRIVATE to that test crate — each
  `tests/*.rs` compiles as its own crate, so adapt/duplicate them into the new file,
  do not try to import them).
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log --all`
  over `crates/gateway-s3/` and grep for `CopyObject`/`x-amz-copy-source` across all
  branches: no prior or in-flight copy-object work; the crate's history is the #448
  PUT/GET/DELETE surface plus the #529/#532 logging series. No open PR touches the PUT arm.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether targeted red→green plus passing fmt/clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).; T4 Contribution — Confirm no closed/rejected remote work supersedes this contribution — affected-path merged/local-ref history and `-S x-amz-copy-source` found no prior implementation, but closed/rejected forge state was unavailable mechanically (`crates/gateway-s3/src/lib.rs:577`).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus passing diff-check, fmt, and affected-package clippy is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).; T4 Contribution — Confirm no closed or rejected forge work supersedes this contribution — affected-path history across available refs and `-S x-amz-copy-source` showed no prior implementation, but closed/rejected forge state was not mechanically available (`crates/gateway-s3/src/lib.rs:577`).
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether independently confirmed targeted red→green plus clean fmt, affected-package Clippy, and diff checks is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the target and therefore could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`).
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
