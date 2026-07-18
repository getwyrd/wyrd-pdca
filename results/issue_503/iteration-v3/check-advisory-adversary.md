# Adversarial review — issue #503 / object-metadata-model (iteration 3)

Skeptic's pass. I re-ran the red→green proof independently in a scratch copy of the
target (full worktree + cache copied to `$PDCA_SCRATCH`, cleaned up after) and planted
the exact mutants the iteration-2 rejection named. Findings:

- NEEDS-HUMAN — ADR-0047 ships in-slice with frontmatter `status: Accepted`
  (`docs/design/adr/0047-object-metadata-model.md:4`, indexed in
  `docs/design/adr/README.md:61`). This is the **pre-declared project-defined human-only
  sign-off item** the brief announces (Scope: "the maintainer's sign-off IS the accepting
  authority"); routing it here is expected, not a defect. Content matches the brief's
  settled decisions (SHA-256 opaque ETag, flat optional fields, repair-preserves split) —
  nothing in the ADR contradicts what the maintainer already approved at Plan.

- NEEDS-HUMAN — residual asymmetry in the panic-hardening the iteration-2 rejection
  demanded: the GET arm now degrades a malformed stored `content_type`
  (`crates/gateway-s3/src/lib.rs:642`, `content_type_header` at `:729`) but still passes
  the stored `etag` **unguarded** into the response builder
  (`crates/gateway-s3/src/lib.rs:650`, `quote_etag`), so a stored `etag` containing a
  non-header byte (e.g. CR/LF) panics every GET of that object at the same
  `.expect("streaming response is always valid")` (`:657`). Concrete case: an
  `InodeRecord` whose JSON `etag` field is `"x\r\ny"` — decode is liberal
  (ADR-0045 parse-don't-validate), so it reaches the builder. **Honest severity
  assessment:** reachability is materially LOWER than the content_type case that forced
  iteration 3 — `etag` is only ever *server-computed* lowercase hex
  (`crates/server/src/lib.rs:299,311`); no seam caller can inject it (the seam *returns*
  the etag, it does not accept one), so only store corruption / out-of-band edits hit
  this. Adjudicate: require the one-line symmetric fallback now, or defer to #506
  (HeadObject reads the same fields next). I deliberately do NOT mark this `[impl]` —
  the human scoped iteration 3 to exactly two items and an auto-rebuild for a
  corruption-only edge may not be worth the cycle.

- (non-blocking observation) `Last-Modified` **value** correctness is untested: the wire
  tests validate only IMF-fixdate *shape* (`crates/server/tests/s3_object_metadata.rs`,
  `is_imf_fixdate`) and both PUTs run at "now", so a wrong-but-well-formed date (e.g. a
  month-index slip in `crates/gateway-s3/src/lib.rs:739` `http_date` /
  `civil_from_days`) would pass every gate. I cross-checked the shipped algorithm
  against Python's datetime over 200,005 sampled epoch values including boundary dates:
  **zero mismatches** — the in-tree formatter is correct today; only the regression
  guard is absent. A unit test pinning e.g. `http_date(784_111_777_000) == "Sun, 06 Nov
  1994 08:49:37 GMT"` would close it; fine to fold into #506 rather than rebuild.

## Attempted refutations that FAILED (the fix survived)

- **Red→green evidence is genuine.** Reversed the patch (keeping only the shipped test
  file) in the scratch copy: both tests in `crates/server/tests/s3_object_metadata.rs`
  compile against the base and fail for the RIGHT reason — "a PutObject response must
  carry an ETag header … pre-fix it has none" (`s3_object_metadata.rs:225`, `:295`) —
  through the real loopback listener → sigv4 → redb + fs-tempdir stack, not a parallel
  re-implementation. Re-applied: 2/2 green. The ETag oracle is an independent SHA-256
  computed in-test (`s3_object_metadata.rs:263` `sha256_hex`), so an echo-anything wire
  layer cannot pass; not a tautology.
- **Carry-forward item 1 (Last-Modified overwrite freshness) really lands.** Planted the
  named mutant — `modified: meta.modified` → `modified: prior.modified` at
  `crates/core/src/metadata.rs:581` and `:641` — and both new unit tests failed
  (`crates/core/tests/mutation_regressions.rs:433`, `:510`); reverted, both pass. The
  leased variant covers the path the wire PUT actually drives
  (`commit_written` → `commit_overwrite` → superseding_leased, `crates/server/src/lib.rs:184`).
- **Carry-forward item 2 (content-type panic) really lands.** Reintroduced the
  pre-hardening raw pass-through at `crates/gateway-s3/src/lib.rs:642`: the new
  router-level test panicked at the production `.expect` (`lib.rs:657`) exactly as the
  rejection described; with the shipped `content_type_header` it degrades to
  `application/octet-stream` and still serves body + ETag. The test drives the real
  signed router dispatch, not a stand-in.
- **Repair-preservation tests (iteration-1 item, "must be kept") are present and green**
  and are non-vacuous (they seed a non-`None` trio): `mutation_regressions.rs`
  (`commit_chunk_map_preserves_object_metadata_across_a_repair`),
  `crates/custodian/tests/backfill.rs`, `rebalance.rs`, `reconstruction.rs` — 8 + 5 +
  10 + 15 tests re-run green in the scratch copy.
- **No new dependency smuggled in:** zero `Cargo.toml` hunks in the patch; the HTTP-date
  formatter is in-tree as the brief directed, and `sha2`/`tower`/`tempfile` were already
  workspace deps of the touched crates.
- Also probed and could not break: overwrite-via-plain-`ObjectMeta::default()` paths
  (CLI writes commit all-`None` metadata → wire degrades to pre-metadata behaviour, per
  design, and a CLI overwrite clearing a stale S3 ETag is the *correct* direction);
  empty-body PUT (digest of empty content still a valid ETag); the unsigned
  `content-type` header ride-along (axum's `HeaderValue` already bars CR/LF from the
  wire, so the verbatim-commit path cannot be poisoned by an HTTP client).

**Verdict:** could not refute the fix or the gate evidence; both prior rejection items
are demonstrably closed (mutants killed). The two NEEDS-HUMAN bullets above are the
pre-declared ADR acceptance and one low-reachability symmetry residue for adjudication.
