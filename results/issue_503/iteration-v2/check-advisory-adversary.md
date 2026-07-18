# Adversarial review — issue #503 (object-metadata-model), iteration 2

Verdict: the core fix stands — I re-ran the green proof locally and verified the red
structurally against the base — but the carry-forward's overwrite-freshness demand is only
**half**-tested, and the patch introduces one new panic edge on the GET path.

## Findings

- NEEDS-HUMAN [impl] — **`Last-Modified` overwrite-freshness is untested — carry-forward
  item 2 is only half-satisfied.** The sign-off rationale demanded "a second PUT of new
  content must stamp a NEW ETag/**Last-Modified**". The shipped wire test
  (`crates/server/tests/s3_object_metadata.rs:346-352`) asserts ETag and Content-Type
  freshness but only shape-validates `Last-Modified` with `is_imf_fixdate` — it cannot
  assert freshness because both PUTs land within the same wall-clock second. No unit test
  covers it either: the only `modified` assertions in the tree are the repair-*preservation*
  tests (where the stale value IS the expected value). Concrete failing case: regress
  `crates/core/src/metadata.rs:581` (or the leased twin at `:641`) from
  `modified: meta.modified` to `modified: prior.modified` — the prior value is
  `Some(first-PUT millis)`, still a valid IMF-fixdate, so **every gate stays green** while
  overwritten objects serve the first publication's `Last-Modified` forever. Fix by
  iterating: add a unit test on `commit_chunk_map_superseding{,_leased}` seeding a prior
  record with a distinct `modified` and asserting the commit stamps `meta.modified`
  (mirror of the preservation test at `crates/core/tests/mutation_regressions.rs:320-364`).

- NEEDS-HUMAN [impl] — **new panic edge: a stored `content_type` that is not a valid HTTP
  header value makes every wire GET of that object panic instead of degrading.** The GET
  arm now feeds a persisted string into the response builder and still unwraps with
  `.expect("streaming response is always valid")`
  (`crates/gateway-s3/src/lib.rs:659`; header set at `:645-650`) — an expectation that was
  true when the headers were constants, and is no longer true in general. The S3 wire PUT
  is guarded (`to_str().ok()` at `lib.rs:1042-1046` keeps values visible-ASCII), but the
  seam is not: `ObjectGateway::put_object_streaming` accepts an arbitrary
  `Option<String>` (`crates/gateway-core/src/lib.rs:119-128`) and `crates/server`
  commits it verbatim (`crates/server/src/lib.rs:310-314`). Concrete failing case:
  `put_object_streaming(key, src, ContentHash::Unverified, Some("text/plain\u{7f}".into()))`
  commits fine; every subsequent wire GET of that key panics the connection task. #504
  (server-side copy) and #506 (HeadObject) will call this seam next. Cheap hardening:
  fall back to `application/octet-stream` when `HeaderValue::from_str` fails, or document
  + enforce the constraint at the seam.

- NEEDS-HUMAN — **ADR-0047 acceptance** (pre-declared, expected — not a defect):
  `docs/design/adr/0047-object-metadata-model.md:4` ships `status: Accepted` before the
  maintainer — the accepting authority — has signed off, and the index row
  (`docs/design/adr/README.md:61`) already lists it Accepted. Frontmatter shape matches
  the ADR-0046 peer; content matches the brief's decided points (SHA-256 opaque ETag,
  flat optional fields, repair-preserves). Human confirms acceptance at sign-off per the
  brief's pre-declared item.

## Refutation attempts that failed (evidence the fix holds)

- **Red→green evidence**: re-ran the shipped tests green at `$PDCA_TARGET`
  (`cargo test -p wyrd-server --test s3_object_metadata` → 2 passed;
  `-p wyrd-core --test mutation_regressions commit_chunk_map_preserves` → 1 passed).
  Red verified structurally against base `HEAD` (0b01454): the base PUT arm answers
  `Ok(()) => empty_response(StatusCode::OK)` and GET hardcodes
  `application/octet-stream` (base `crates/gateway-s3/src/lib.rs:594-611`), and the new
  test file uses only base-era APIs (`sigv4::sign`/`format_amz_date` pre-exist; `sha2` is
  already a `[dependencies]` entry of `wyrd-server`, Cargo.toml:89) — so it compiles on
  base and fails on the ETag assertion, a genuine assertion-red, not a compile-error red.
- **Tautology check**: the ETag oracle is an independent SHA-256 computed in-test
  (`s3_object_metadata.rs:243-249`), not an echo of the server's value; a wire layer
  returning an arbitrary string fails. The test drives the real loopback listener
  (redb + fs tempdir), not a double.
- **Carry-forward item 1 (repair preservation)**: the four new preservation tests seed a
  non-`None` trio and drive the real commits (`mutation_regressions.rs:320`,
  `custodian/tests/backfill.rs:496`, `rebalance.rs:730`, `reconstruction.rs:842`) — each
  asserts the repair *fired* (`Reconciled::Changed` / version bump) before asserting
  preservation, so none is vacuous. `..prior.clone()` regressing to `..Default::default()`
  is now caught. Could not refute.
- **In-tree date formatter**: cross-checked `civil_from_days` + weekday math
  (`gateway-s3/src/lib.rs:727-772`) against Python `datetime` over 200k random instants
  up to year 9999 — zero mismatches; epoch-millis `u64` keeps `days` non-negative so the
  `(days%7+4)%7` weekday is exact. Could not refute.
- **ETag freshness / stale-content-type on overwrite**: wire test 2 asserts both against
  independent oracles after a real overwrite through `commit_chunk_map_superseding_leased`
  (the production path via `commit_written` → `commit_overwrite`). Could not refute.

## Non-blocking observation

- The buffered `put_object` / CLI path (`crates/server/src/lib.rs:158-166`,
  `cli.rs:1432`) commits `ObjectMeta::default()` — a CLI overwrite of a wire-PUT object
  drops its ETag/Content-Type/Last-Modified to `None`. This is the documented degrade
  path (`crates/core/src/write.rs:63-69`) and stale metadata over new bytes would be
  worse, so not filed as a defect; noting it so #506 (HeadObject) doesn't assume the trio
  is always present on committed objects.
