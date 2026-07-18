# Adversarial review — issue 503 / object-metadata-model (iteration 4)

Scope of this iteration's delta (per brief carry-forward §Iteration 3): guard the stored
`etag` on the GET arm symmetrically with `content_type` (degrade, never panic) + a
router-level malformed-etag test; keep all prior tests. I attacked the evidence, the fix,
and the verdict; the substantive attacks did not land.

## Evidence — re-run and probed

- **Delta red→green verified manually, because the machine gate does not cover it.** The
  C4-verify PASS row ("red without the fix, green with it") is carried by
  `crates/server/tests/s3_object_metadata.rs` — which was already green in iteration 3.
  This iteration's own test, `a_malformed_stored_etag_degrades_the_get_instead_of_panicking`
  (`crates/gateway-s3/src/lib.rs:2128-2172`), is **co-located** in `src/`, which the
  classifier degrades to green-only (brief.md:64-66). I reconstructed the pre-fix state in
  a scratch clone (reverted the guard at `crates/gateway-s3/src/lib.rs:653-655` to the
  unguarded `quote_etag` form): the test fails with the exact production panic —
  `panicked at 'streaming response is always valid: http::Error(InvalidHeaderValue)'` —
  and passes with the guard. It drives the REAL signed router dispatch (`oneshot` through
  `S3Gateway::new(...).router()`, lib.rs:1976-2013), not a re-implementation, and the
  oracle (200 + full body + ETag header absent) cannot pass for the wrong reason. Red→green
  for the delta is genuine; the check-gates row just should not be read as machine-proving it.
- Re-ran on the patched tree: `wyrd-gateway-s3 --lib` 40/40, `s3_object_metadata` wire
  tests 2/2, `wyrd-core mutation_regressions` 8/8 including all three carry-forward
  preservation/freshness tests. Worktree content matches patch.diff file-for-file.

## Fix — refutation attempts that did not land

- **Attempted: break `http_date`/`civil_from_days`** (`crates/gateway-s3/src/lib.rs:743-783`).
  Hand-checked the RFC-7231 example: `http_date(784_111_777_000)` → days=9075, weekday
  `(9075%7+4)%7=0`→Sun, civil_from_days(9075)→1994-11-06, sod=31777→08:49:37 — correct.
  Overflow: u64-millis max gives days≈2.1e11, well inside i64; `modified` is always
  non-negative so the weekday precondition holds. Could not refute. (The optional
  http_date pin test from the iteration-3 rationale was not added — it was explicitly
  non-blocking, "may fold into #506".)
- **Attempted: find another panic path in the GET builder.** `content_type` guarded
  (`content_type_header`, lib.rs:766), `etag` now guarded (`etag_header`, lib.rs:736),
  `last-modified` is `http_date` output (always visible-ASCII), `content-length` is a
  number. Could not refute.
- **Attempted: metadata trio staleness/loss.** Repair paths preserve via `..prior.clone()`
  (crates/core/src/metadata.rs:533, custodian backfill.rs:127 / rebalance.rs:285 /
  reconstruction.rs:576), each pinned by a non-vacuous seeded test (backfill.rs:361,
  rebalance tests `evacuation_preserves_object_metadata`, reconstruction
  `reconstruction_preserves_object_metadata_across_a_repair`); overwrite freshness pinned
  on BOTH superseding commits incl. the leased path the wire PUT drives
  (mutation_regressions.rs:385, :436) plus the two-PUT wire test with an independent
  SHA-256 oracle. Could not refute.
- **Residual asymmetry noted, judged unreachable (non-blocking, not a defect of this
  diff):** the PUT response still sets the etag unguarded —
  `put_object_response` at `crates/gateway-s3/src/lib.rs:709-716`
  (`.header("etag", quote_etag(etag))` + `.expect`). Unlike the GET case, this value is
  never liberal-decoded storage: it is the digest the production implementer just minted
  (`hex(&hashing.finalize())`, crates/server/src/lib.rs:298), always header-safe; no
  stored-corruption path reaches it. Flagging it would re-litigate the human-scoped
  iteration-3 fix. Worth remembering when #504's copy echoes a *stored* ETag into a
  response — a next-slice concern.

## Verdict / human items

- NEEDS-HUMAN — ADR-0047 acceptance (pre-declared, brief.md:52-55):
  `docs/design/adr/0047-object-metadata-model.md:4` ships `status: Accepted` in-slice with
  the maintainer's sign-off as the accepting authority; the README index row
  (`docs/design/adr/README.md:61`) likewise lands now. A project-defined human-only item —
  expected per the brief, not a defect.

**Summary:** attempted to refute the red→green evidence (co-located-test classifier gap —
closed by manual pre/post reproduction), the date formatter, the GET panic surface, and
the preservation/freshness invariants; **could not refute the fix.** The one lifted item
is the pre-declared ADR acceptance decision.
