# Design proposal — issue 503 / object-metadata-model

> The Plan artifact for the exception: this slice decides and lands the **object-metadata
> data model** — an `InodeRecord` (persisted-record) change plus a public `ObjectGateway`
> seam change — the foundation HeadObject (#506), server-side copy (#504 step 2), and
> multipart ETags build on. That data-model + public-API impact is what earns the
> design-proposal form. Do reads ONLY this file and implements it; Check runs the regular
> gated check on the code.

- **Slug:** object-metadata-model
- **Kind:** enhancement (design proposal)
- **Goal:** the gateway stores and returns object metadata beyond byte size: an `ETag`
  (on the `PutObject` response and on GET), the client's `Content-Type` (round-tripped),
  and `Last-Modified`. Today: PUT answers an empty `200` with no ETag
  (`crates/gateway-s3/src/lib.rs:594-597`, `empty_response` at `:634-650`); GET hardcodes
  `content-type: application/octet-stream` and sets only `content-length`
  (`lib.rs:603-611`); the inode carries only `size`/`chunk_map`/`state`/`version`
  (`crates/core/src/metadata.rs:235-244`) and the seam returns only `size` + stream
  (`ObjectRead`, `crates/gateway-core/src/lib.rs:45-50`).
- **Success criterion:** through the real wire path, a signed `PutObject` response
  carries an `ETag` header; a subsequent `GetObject` returns the **same** `ETag`, the
  `Content-Type` the PUT request declared, and a valid RFC-7231 `Last-Modified`; the
  metadata round-trips through a real `MetadataStore` commit (the redb-backed loopback
  stack — stored on the record, not synthesized at the wire layer). Demonstrable by C4-verify: the shipped test is red on the base (no
  ETag header on PUT, `application/octet-stream` on GET) and green with the patch. An
  aws-cli/boto3 round-trip with checksum/ETag validation is supplementary off-Check
  evidence, not the binding criterion.
- **Falsifiability:** RED is demonstrable today on the in-process loopback harness
  (`crates/server/tests/s3_http_wire.rs` pattern): on `origin/main` the PUT response has
  no `ETag` header and GET's `content-type` is the hardcoded `application/octet-stream`
  regardless of what the PUT sent. No external service or topology needed.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** none
- **Conflicts with:** 504
- **Ordering note:** conflicts with 504 because both edit the object dispatch's PUT/GET
  arms in `crates/gateway-s3/src/lib.rs:557-631` (504 adds the copy-source guard at the
  top of the PUT arm this slice rewrites the response of) — schedule in a later wave than
  504 so this builds on the folded base carrying the guard. #506 (HeadObject) declares
  `Depends on: 503` and lands in a wave after this one.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** one logical change: the object-metadata model on the inode record + its
  wire surfacing (the Design section below), **plus the in-tree ADR recording the
  decision** — `docs/design/adr/0047-object-metadata-model.md` ships IN this patch
  (maintainer-directed at Plan, the ADR-in-slice variant of the #502/ADR-0046
  precedent). Distill it from the Design/Alternatives sections below: metadata fields
  live on `InodeRecord` (committed atomically with the chunk map), new fields are
  `Option` + `#[serde(default)]` for stored-record compatibility, ETag is the quoted
  lowercase-hex SHA-256 of the content treated as an opaque change-token (NOT MD5 —
  decision confirmed by the maintainer at Plan), multipart ETag-of-parts deferred.
  Follow the corpus conventions (`docs/design/adr/README.md` index + the ADR-0046
  frontmatter shape; status `Accepted` pending the maintainer's sign-off, which IS the
  accepting authority here). PRE-DECLARED sign-off item: an ADR change is a
  project-defined human-only item (INTEGRATION §4) — the reviewer WILL route it to §6
  NEEDS-HUMAN; that is expected, not a defect. / out of scope: `x-amz-meta-*`
  user metadata (model leaves room, nothing populated), HeadObject (#506), server-side
  copy (#504 step 2), multipart, conditional requests, and any edit to an EXISTING
  accepted ADR (immutability gate — 0047 is a new record, it supersedes nothing).
- **External dependencies:** none — the C4 test runs the in-process loopback stack (redb
  in-memory metadata, fs chunk store in a tempdir). If Do reaches for an HTTP-date or
  checksum crate, note that a **new dependency is a human-only sign-off item** (ADR-0003
  three-test audit + `deny.toml`, INTEGRATION §4) — an in-tree IMF-fixdate formatter is
  small and avoids it.
- **Test file:** crates/server/tests/s3_object_metadata.rs (a NEW file under a `tests/`
  dir — the shape C4-verify's classifier keys on; a co-located test degrades to
  green-only, engine/scripts/run-verify.sh:93)
- **Citations expected:** Do must cite path:line on `origin/main` for every change.
  Composition cues (each MAY be opened, nothing else): the record to extend —
  `crates/core/src/metadata.rs:235-256` (`InodeRecord`, `new_empty`) and its commit
  points `commit_chunk_map*` (`metadata.rs:461-587` — NOTE the publication-vs-repair
  split at `:481-495`; see Design: only the superseding commits set metadata); the
  digest already streamed —
  `HashingSource` (`crates/server/src/lib.rs:409-424`) inside `put_object_streaming`
  (`server/src/lib.rs:269-295`); the seam to widen — `ObjectRead` / `ObjectGateway`
  (`crates/gateway-core/src/lib.rs:45-50`, `:108-135`); the wire arms —
  `crates/gateway-s3/src/lib.rs:558-619`; the test harness peer —
  `crates/server/tests/s3_http_wire.rs` (its helpers are PRIVATE to that test crate —
  adapt/duplicate into the new file; and extend the response parse to CAPTURE HEADERS,
  the peer's `parse_response` returns only status+body while this criterion asserts
  ETag/Content-Type/Last-Modified headers); the ADR shape/frontmatter peers for the
  in-slice ADR-0047 (see Scope) — `docs/design/adr/0046-bucket-model-real-namespace.md`
  and the index `docs/design/adr/README.md` (update the index, per corpus convention).
- **Disposition hint:** new-feature

## Motivation

Many SDKs validate the `ETag` and round-trip `Content-Type`; their absence breaks
integrity checks and content typing, and blocks the rest of the 0.1-Alpha S3 epic:
HeadObject (#506) has nothing to return, server-side copy (#504 step 2) has no ETag to
echo, and multipart needs the ETag-of-parts convention. This slice is the design
foundation those build on — decide the model once, here.

## Design

**The metadata model (the decision this proposal records).** Extend `InodeRecord` with
three **top-level** optional fields — flat on the record, matching its existing flat
`size`/`state`/`version` shape (DECIDED: not a nested `metadata:` sub-struct; flat is
the simplest serde-default compat story and one fewer schema decision downstream):

- `etag: Option<String>` — the content digest, computed from the bytes the write path
  already streams through `HashingSource` (no second read of the object);
- `content_type: Option<String>` — the PUT request's `Content-Type` header, verbatim;
- `modified: Option<u64>` — content-publication time (epoch millis); rendered as
  RFC-7231 IMF-fixdate on the wire.

`x-amz-meta-*` user metadata is deliberately NOT a field in this slice; the flat-record
decision does not preclude adding one later.

**Which commits set metadata (load-bearing distinction).** The metadata fields are set
ONLY at **content publication** — object create (`metadata.rs` `create`) and overwrite
(`commit_chunk_map_superseding` / `commit_chunk_map_superseding_leased`). The plain
`commit_chunk_map` is the **reconstruction/backfill** path that re-commits the SAME
content (`metadata.rs:481-495` documents the split): it MUST PRESERVE the existing
`etag`/`content_type`/`modified` unchanged — a repair or placement-maintenance commit
must not move `Last-Modified` or drop the content type.

**Backward compatibility of persisted records (load-bearing).** `InodeRecord` is
serialized ONCE, centrally: `crates/core/src/metadata.rs:275-281` (`encode`/`decode` =
serde_json — ":11: Records are encoded as JSON for M0") over the byte-oriented
`MetadataStore` KV seam, so every backend (redb/tikv/fdb) stores the same JSON and one
compat rule covers them all; `metadata.rs:111` is the in-repo precedent for a field that
"decodes with an empty vector" when absent. Every new field MUST be `Option` +
`#[serde(default)]` so records written
before this change still deserialize — and a record missing metadata degrades on the wire
to today's behaviour (no ETag header, `application/octet-stream`), never to an error.
Do NOT bump or gate on `version` semantics (`metadata.rs:242-243` — that is the CAS
counter, not a schema version).

**Seam changes.** `put_object_streaming` gains the declared content type (an
`Option<String>` or a small params struct — Do's call, but keep the seam neutral: no
axum/HTTP types in `gateway-core`) and returns the committed ETag instead of `()`;
`ObjectRead` gains `etag`/`content_type`/`modified` alongside `size`. All three
`ObjectGateway` methods stay streaming; nothing buffers.

**Wire changes.** PUT arm: pass the request's `Content-Type` down; answer with the `ETag`
header (S3 quotes ETag values — `"..."`). GET arm: replace the hardcoded content-type
with the stored one (falling back to `application/octet-stream`), add `ETag` and
`Last-Modified`.

**ETag basis — decided here:** the lowercase-hex SHA-256 of the content (quoted), i.e.
the digest `HashingSource` already produces. S3's convention for simple PUTs is the MD5,
and some tooling assumes ETag==MD5; but S3 itself documents ETag as **not** guaranteed to
be MD5 (SSE-KMS/multipart objects), well-behaved clients treat it as an opaque
change-token, and Wyrd already carries vetted SHA-256 on this path — adding an MD5
dependency for a legacy equality would need the ADR-0003 dependency audit and buys
compatibility only with clients that violate the opacity rule. Multipart's
ETag-of-parts convention is explicitly deferred to the multipart slice.

## Alternatives considered

- **MD5 ETag (S3-classic):** maximal legacy compatibility, but requires a new MD5
  dependency (audit + deny.toml) and a second digest over every streamed byte;
  rejected — SHA-256-as-opaque-token is spec-legal and already streamed.
- **A separate metadata record keyed off the inode** (rather than fields on
  `InodeRecord`): avoids touching the record shape, but doubles the metadata round-trips
  and breaks the one-commit atomicity the write path has (`commit_chunk_map_superseding*`
  commits attributes + chunk map in one CAS); rejected.
- **Compute ETag lazily on GET:** re-reads the whole object per request; violates the
  stream-don't-buffer economics and gives PUT no ETag to return; rejected.

## Impact & compatibility

Public trait change (`ObjectGateway`) — every implementer and test double in-tree updates
in this slice (compiler-driven; `crates/server` is the production implementer). Persisted
records: forward-compatible via `#[serde(default)]` as above; old records read fine, new
records written by old code would drop the fields (acceptable pre-Alpha, no migration
tool). Wire behaviour change is additive (new headers); the GET content-type changes from
always-`octet-stream` to stored-or-`octet-stream`. DST determinism: take the commit
timestamp from `SystemTime::now()` at the commit call site (madsim virtualises it), not
from a new clock dependency.

## Open questions

- ~~Whether to also land an in-tree ADR~~ — **RESOLVED at Plan (maintainer decision,
  2026-07-18): ADR-in-slice.** Do authors `docs/design/adr/0047-object-metadata-model.md`
  in this patch (see Scope); the maintainer accepts it at sign-off, which is the
  accepting authority (founding-maintainer, GOVERNANCE). The ETag basis is likewise
  **decided: SHA-256, opaque** — not open.
- Whether `Last-Modified` should be commit time (chosen above) or request-receipt time —
  cosmetic at Alpha; flag if the reviewer disagrees.

## Prior-art check (triage cycles)

Searched by file path — `git -C ../wyrd log --all` over `crates/gateway-s3/`,
`crates/gateway-core/`, `crates/core/src/metadata.rs` and grep for `ETag`/`etag` across
all branches: no prior or in-flight metadata work; ADR-0046 (bucket model, #502) is the
adjacent accepted decision and is orthogonal (bucket records vs object attributes — this
slice does not touch the bucket/dirent namespace).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected only for the two adversary-flagged test-coverage gaps on ADR-0047's load-bearing invariants; the implementation itself stands — do not redesign it, add the missing tests: 1) Repair-preservation: seed a record with etag/content_type/modified set, run a repair/backfill commit, and assert the metadata trio survives — guards the `..prior.clone()` preservation lines (crates/core/src/metadata.rs:535, crates/custodian/src/backfill.rs:127, rebalance.rs:285, reconstruction.rs:576), currently vacuously true because all existing tests seed all-None metadata. 2) Overwrite freshness: a second PUT of new content must stamp a NEW ETag/Last-Modified (commit_chunk_map_superseding, crates/core/src/metadata.rs:579-581) — the shipped test does exactly one PUT + one GET, so a stale-ETag regression would pass every gate. Context already settled by the human (do not revisit): T4 prior-art cleared; T5 ADR-0047 decisions (SHA-256 opaque ETag, flat record) approved. The reviewer's partial CI rerun (cargo deny env issue) and the MD5-compat question are recorded as §10 Act candidates / a foundations tracking issue, not rebuild scope.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the two adversarial-review findings; the implementation and design stand — do not redesign, fix exactly these: 1) Last-Modified overwrite-freshness is still untested (carry-forward item 2 half-met): the wire test only shape-validates the header (both PUTs land in the same second) and no unit test asserts freshness — regressing `modified: meta.modified` to `modified: prior.modified` in `commit_chunk_map_superseding{,_leased}` (crates/core/src/metadata.rs:581 / :641) keeps every gate green. Add a unit test seeding a prior record with a distinct `modified` and asserting the superseding commit stamps the NEW `meta.modified` (mirror the preservation test at crates/core/tests/mutation_regressions.rs:320-364). 2) New panic edge on GET: a stored `content_type` that is not a valid HTTP header value makes every wire GET of that object panic via the `.expect("streaming response is always valid")` unwrap (crates/gateway-s3/src/lib.rs:659; header set at :645-650) — the seam (`ObjectGateway::put_object_streaming`, gateway-core/src/lib.rs:119-128) accepts arbitrary strings and server commits them verbatim. Harden: fall back to `application/octet-stream` when `HeaderValue::from_str` fails (or document + enforce the constraint at the seam). #504/#506 call this seam next. Context already settled by the human (do not revisit): T5/ADR-0047 decisions (SHA-256 opaque ETag, flat record) approved; T4 prior-art cleared; carry-forward item 1 (repair preservation) now satisfied and its four tests must be kept; reviewer's partial CI rerun (cargo deny host lock) is an environment issue, not rebuild scope.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected solely on the adversary's one genuinely new finding — the residual panic-hardening asymmetry; the implementation and design stand, do not redesign. Fix exactly this: the GET arm degrades a malformed stored `content_type` (crates/gateway-s3/src/lib.rs:642, `content_type_header` at :729) but still passes the stored `etag` UNGUARDED into the response builder (crates/gateway-s3/src/lib.rs:650, `quote_etag`), so a stored etag containing a non-header byte (e.g. CR/LF — reachable via store corruption / out-of-band edits, since decode is liberal per ADR-0045) panics every GET of that object at the `.expect("streaming response is always valid")` (:657). Apply the symmetric fallback: when the quoted etag is not a valid header value, omit the ETag header (degrade, never panic), and add a router-level test mirroring the malformed-content_type one (seed a stored etag with a CR/LF byte, assert 200 + body served without panic). Keep all existing tests — the iteration-1/2 carry-forward tests are verified non-vacuous (mutants killed) and must remain. Optional, non-blocking: a unit test pinning an http_date value (e.g. http_date(784_111_777_000) == "Sun, 06 Nov 1994 08:49:37 GMT") may ride along or fold into #506. Context already settled by the human (do not revisit): T5/ADR-0047 decisions (SHA-256 opaque ETag, flat optional record, status Accepted in-slice) approved; T4 prior-art cleared; the reviewer's `cargo deny` scratch-clone advisory-DB lock is an environment issue, not rebuild scope.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
