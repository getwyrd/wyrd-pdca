# Brief — issue 506 / head-object

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** head-object
- **Defect:** `HeadObject` (`HTTP HEAD /bucket/key`) returns **405 MethodNotAllowed** —
  the object dispatch matches only PUT/GET/DELETE and falls through to the 405 arm
  (`crates/gateway-s3/src/lib.rs:557-631`; the fallback at `:625-631` literally says
  "only object PUT, GET, and DELETE are supported"). This breaks the ubiquitous
  HEAD-before-GET/PUT pattern: `aws s3 cp` download fails (`405` on its HeadObject
  preflight), as do most SDK existence checks. (The issue cites `:344-348`; the file has
  since grown — current lines above, verified on `origin/main`.)
- **Success criterion:** through the real wire path, a signed `HEAD /bucket/key` for a
  stored object returns `200` with **no body** and the same metadata headers GET carries
  after #503 — `Content-Length` (the object's size, not 0), `ETag`, `Content-Type`,
  `Last-Modified`; a signed HEAD for an absent key returns `404` headers-only; GET/PUT/
  DELETE behaviour is unchanged. Demonstrable by C4-verify against the wave's folded base
  (which carries #503): red without this patch (405 today), green with it. `aws s3api
  head-object` / `aws s3 cp` succeeding is supplementary off-Check evidence.
- **Falsifiability:** RED is demonstrable today on the in-process loopback harness
  (`crates/server/tests/s3_http_wire.rs` pattern): a signed HEAD on `origin/main` (and on
  the folded base with #503/#504 applied, which add no HEAD arm) returns 405. No external
  service or topology needed.
- **Invariant to restore:** n/a — non-structural behavioural gap (a missing method arm);
  principles.md §1.1: minimal reviewable delta applies.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 503
- **Conflicts with:** 504
- **Ordering note:** depends on 503 — HEAD's response headers ARE the #503 metadata
  (ETag/Content-Type/Last-Modified on the seam); building it before 503 would ship a
  HEAD that can only say `Content-Length` and would be rewritten a wave later. Conflicts
  with 504 because both edit the same `match method` dispatch in
  `crates/gateway-s3/src/lib.rs:557-631`. Net wave order: {504, 505} → {503} → {506}.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** add a `HEAD` arm to the object dispatch that resolves the key's **metadata
  only** and answers headers-only: `Content-Length`, plus the #503 metadata headers with
  the same fallbacks GET uses; `404 NoSuchKey` (headers only, no XML body on the wire —
  hyper suppresses HEAD bodies anyway) when absent. Prefer a metadata-only lookup on the
  seam (whatever shape #503 landed — a stat-like read that does NOT open the fragment
  stream), so HEAD of a large object costs metadata round-trips, not data reads; if #503's
  seam only offers `get_object_streaming`, extending the seam with a head/stat method is
  in scope — dropping an opened fragment stream is the fallback, not the target. Note the
  access-log layer already classifies HEAD as body-less (`lib.rs:396-401`), so no logging
  changes are expected. / out of scope: HeadBucket (that is the #502/ADR-0046 bucket
  model), conditional requests (`If-None-Match`/`If-Modified-Since`), `Range` semantics,
  and any change to PUT/GET/DELETE behaviour.
- **Repro instruction:** on the folded base, run the loopback gateway
  (`crates/server/tests/s3_http_wire.rs` harness), signed PUT an object, then send a
  signed `HEAD /b/k` → 405 with code `MethodNotAllowed`. Equivalent CLI repro:
  `aws s3api head-object --bucket b --key k` → 405; `aws s3 cp s3://b/k out` fails on its
  HEAD preflight.
- **External dependencies:** none — the C4 test runs the in-process loopback stack; the
  off-Check aws-cli confirmation is covered by the registered
  "aws cli (S3 gateway round-trip)" doctor row.
- **Test file:** crates/server/tests/s3_head_object.rs (a NEW file under a `tests/` dir —
  the shape C4-verify's classifier keys on)
- **Citations expected:** Do must cite path:line on the target base for every change.
  Composition cues (each MAY be opened, nothing else): the dispatch to extend —
  `crates/gateway-s3/src/lib.rs:557-631` (mirror the GET arm's Ok(None)→`NoSuchKey`
  mapping at `:612-617`); the body-less response classification that already anticipates
  HEAD — `lib.rs:388-401`; the seam #503 landed (open the folded base's
  `crates/gateway-core/src/lib.rs`); the test harness peer —
  `crates/server/tests/s3_http_wire.rs` (`signed_headers`, `send`, `parse_response` —
  PRIVATE to that test crate, adapt/duplicate into the new file; extend the response
  parse to CAPTURE HEADERS, the peer's returns only status+body while this criterion
  asserts the metadata headers and the empty-body property).
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log --all`
  over `crates/gateway-s3/` and grep for `HeadObject`/`Method::HEAD` across all branches:
  no prior or in-flight HEAD-arm work; the only HEAD mentions are the #532 access-log
  body-less classification and its tests (`lib.rs:1731`). No open PR touches the dispatch.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Install and run `cargo machete`, then decide whether its unused-dependency result clears the full CI claim — focused red→green, fmt, Clippy, build, workspace tests, three cargo-deny audits, conformance, statics, and DST passed, but this host lacks that asserted scanner; wire assertions are at `crates/server/tests/s3_head_object.rs:231`.; T4 Contribution — Confirm closed/rejected PR history has no competing HeadObject work before contribution sign-off — affected-path and all-local-ref searches found no `head_object` implementation, but the supplied artifacts contain no closed/rejected PR metadata; the contribution seam is at `crates/gateway-core/src/lib.rs:167`.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
