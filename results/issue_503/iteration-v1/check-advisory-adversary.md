# Adversarial review — issue #503 (object-metadata-model)

## Evidence attack: red→green independently re-run — could not refute

Re-executed the proof myself in a scratch copy of `$PDCA_TARGET` (patched tree, then the
patch reverse-applied with only the new test kept):

- **GREEN** (patched): `cargo test -p wyrd-server --test s3_object_metadata` → 1 passed.
- **RED** (base + test): compiles **cleanly** against the base API and fails at exactly the
  asserted point — `panicked at crates/server/tests/s3_object_metadata.rs:225: a PutObject
  response must carry an ETag header (ADR-0047); pre-fix it has none`. The red is
  behavioral, not a compile artifact.
- The test drives the **production path** (real TCP loopback listener, real SigV4, redb +
  fs-tempdir stack, 8-byte chunks so the body spans chunks) and its ETag oracle is an
  **independent** SHA-256 computed in the test (`s3_object_metadata.rs:1172-1179`,
  `:1238`) — not an echo of the server's value, so a wire layer inventing a token fails.
  The IMF-fixdate validator (`:1184-1224`) is a strict shape check, not a substring match.
- Attempted to refute via: unsigned `content-type` slipping past SigV4 (no — the verifier
  honors the client-declared `SignedHeaders` list, `crates/gateway-s3/src/sigv4.rs:413-446`,
  and the in-tree signer signs `host;x-amz-content-sha256;x-amz-date` only, `sigv4.rs:594`,
  matching how the harness sends it); struct-update precedence dropping metadata in
  `commit_chunk_map` (no — listed fields win, `..prior.clone()` fills only the metadata
  trio, `crates/core/src/metadata.rs:527-536`); `http_date`/`civil_from_days` arithmetic
  (hand-checked against the RFC-7231 exemplar `Sun, 06 Nov 1994 08:49:37 GMT` including the
  weekday offset — correct, `crates/gateway-s3/src/lib.rs:869-908`); commit atomicity
  (metadata is stamped on the plan before `commit_written`, landing in the same CAS batch
  as the chunk map, `crates/server/src/lib.rs:310-315`). **Could not refute.**

## Fix attacks — findings

- NEEDS-HUMAN [impl] — The brief's **load-bearing** repair-preservation invariant
  ("a repair must not move `Last-Modified` or drop the content type", brief Design §"Which
  commits set metadata") is implemented in four places (`crates/core/src/metadata.rs:535`,
  `crates/custodian/src/backfill.rs:127`, `crates/custodian/src/rebalance.rs:285`,
  `crates/custodian/src/reconstruction.rs:576` — the `..prior.clone()` lines) but has
  **zero test coverage**: every custodian/core test seeds records via `..Default::default()`
  (all-`None` metadata), so preservation is vacuously true. Concrete failing case: delete
  any one of those `..prior.clone()` lines and every gate in `check-gates.json` still
  passes — the load-bearing half of ADR-0047 is unguarded. A small test (seed a record with
  `etag`/`content_type`/`modified` set, run a repair/backfill commit, assert the trio
  survives) closes it.
- NEEDS-HUMAN [impl] — No **overwrite** test: the freshness half of the same invariant
  (a second PUT stamps a NEW ETag/Last-Modified, `crates/core/src/metadata.rs:579-581`)
  is likewise untested — the shipped test does exactly one PUT + one GET. Concrete failing
  case: replace `meta.etag.clone()` with `prior.etag.clone()` in
  `commit_chunk_map_superseding` and all gates stay green while GET serves a stale ETag
  for rewritten content.
- The buffered/CLI write path commits **no metadata**: `Gateway::put_object`
  (`crates/server/src/lib.rs:158-166`, called from `cli.rs:1432`) leaves
  `WritePlan::object_meta` at its all-`None` default, so a CLI **overwrite of an
  S3-written object erases** its stored ETag/content-type/modified. This degrades to
  `None` (no header) rather than serving a stale ETag — the correct failure direction, and
  explicitly sanctioned by the `WritePlan::object_meta` doc comment
  (`crates/core/src/write.rs:63-69`) — but note the CLI path buffers the whole body and
  could trivially compute the digest; left as observed asymmetry, not a defect.
- No test decodes an **old-format record** (pre-metadata JSON) at the wire — the
  `#[serde(default)]` degrade path (brief: "never to an error") is trivially correct by
  construction but unexercised; minor, subsumed by the two [impl] bullets above if a
  seeded-record test lands.
- Cosmetic: the in-crate test doubles return `Ok(String::new())` for
  `put_object_streaming` (`crates/gateway-s3/src/lib.rs:1186`, `:1470`), so their PUT
  responses carry `etag: ""` — test-only, no production effect.

## Verdict attacks

- NEEDS-HUMAN — ADR-0047 (`docs/design/adr/0047-object-metadata-model.md`) ships with
  `status: Accepted` before the maintainer's sign-off, and the ADR + README index change is
  a project-defined human-only item. This is **pre-declared in the brief** (Scope: "the
  reviewer WILL route it to §6"), so it is expected, not a defect — but acceptance of the
  ADR (including the SHA-256-not-MD5 ETag decision and `Last-Modified` = commit time) is
  the human's call, not the gates'.
- `check-gates.json` C4-verify's "red without the fix, green with it" claim: **verified
  independently above; warranted.** No rationalization found in the gate rows — the only
  unguarded claims are the two untested invariants filed under [impl].
