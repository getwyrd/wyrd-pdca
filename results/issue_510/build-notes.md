# Build notes — issue 510 / range-conditional-get (Iteration 5)

Target: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`) on the folded base
`0fed4c8 pdca-integrate: issue_507` (507 is folded in; `origin/main` is the pre-507
`07d0244`). All line citations below are the **post-fix** worktree (verified with the patch
applied). The brief's own cited line numbers (`:909`/`:968`) are pre-507 and shifted; the
live GET/HEAD dispatch is `crates/gateway-s3/src/lib.rs:1654`/`:1749`.

## What this iteration is — and why the approach is unchanged

The iteration-4 patch (12 wire assertions + the seam-level version-skew guards) was
substantively **accepted**: the reviewer passed every C/T row, the adversary attempted to
refute the red→green proof, the single-resolve atomicity, the range/date parsers and the
finish-path accounting and **could not**, and the human sign-off's carry-forward for this
round contains **no implementation defect**. The two open §6 rows that forced the
auto-iterate were both **environmental / process**, not the code:

1. **C4 Verification** — the full `cargo xtask ci` rerun ended at `cargo deny check`
   because the host could not acquire the read-only Cargo advisory-DB lock; the patched
   wire suite itself passed 12/12. *This is a host lock, not a patch fault.*
2. **T4 Contribution** — prior-art collision could not be mechanically discharged because
   this checkout exposes no authoritative closed/rejected review history. *A checkout
   limitation, not a patch fault.*

The Do rule is "do not re-submit the **rejected approach** unchanged." The approach here
was **not rejected** — it was accepted-pending-environment. So this iteration re-ships that
same accepted change (regenerated cleanly against the current folded base) and **closes the
iteration-4 blocker directly**: I re-ran the exact step that failed.

### Iteration-4 blocker (1) resolved live — the dependency wall now passes

I ran all three `cargo deny` invocations the wall makes (`xtask/src/lib.rs:157-176`,
`dependency_wall_invocations`) in the worktree:

- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (exit 0).
- `cargo deny --all-features --config deny-all-features.toml check advisories` → `advisories ok` (exit 0).
- `cargo deny --all-features check licenses bans sources` → `bans ok, licenses ok, sources ok` (exit 0).

The advisory-DB lock was available this run; the wall is green. (The one `warning[license-not-encountered]`
for `ISC` at `deny.toml:111` is a pre-existing unused-allowance warning, exit 0, unrelated
to this patch — my change adds **no dependency**, so the graph deny audits is unchanged.)

Iteration-4 blocker (2) is a checkout property no builder edit can change; it is recorded
for the human at sign-off, not something the patch can discharge.

## The change (all four iterations' fixes preserved)

Two ordered gates in the GET/HEAD arms — conditionals first (they answer 304/412 with no
body work), then range resolution — with the seam carrying byte offsets so only covering
chunks are fetched (never wire-side discard).

### Seam (protocol-neutral, `crates/gateway-core/src/lib.rs`)
- `ObjectRead` (`:45`), `ObjectMeta` (`:67`), `RangeRead` (`:110`), `RangeOutcome` (`:121`)
  — one resolve yields meta + satisfiable/unsatisfiable range outcome (the atomicity seam).
- `resolve_byte_range` (`:144`) — pure `bytes=a-b`/`a-`/`-N` → `(offset,len)` math; `n==0`
  / `size==0` unsatisfiable (so `bytes=-0` → 416, iteration-1 item).
- `get_object_range` (`:341`) ships a **correctness-preserving default** (full streaming
  resolve + `slice_object_stream`, meta and bytes from the ONE resolve — atomic), so a
  non-overriding gateway answers a ranged GET of an existing object with a correct 206/416,
  never a 404 after `Accept-Ranges: bytes` was advertised (iteration-2 item 4). `head_object`
  (`:380`) is the metadata-only seam (PR #607).

### Server override (`crates/server/src/lib.rs`)
- `get_object_range` (`:401`) walks the chunk map and fetches **only** the chunks overlapping
  `[a,b]`, trimming first/last (`covering` at `:431`, fragment fetch at `:454`) — the
  anti-wire-side-discard implementation. `get_object_streaming` (`:343`) and `head_object`
  (`:491`) are the peer resolves.

### Wire arms (`crates/gateway-s3/src/lib.rs`)
- `serve_get` (`:1654`) — evaluates conditionals against the metadata of the **same** resolve
  that yields the body (`RangeRead.meta` for ranged, `ObjectRead` fields for unranged); no
  separate `head_object` snapshot, so the If-Match fence is judged against the served version
  (TOCTOU closed, iteration-2 item 3 / iteration-3 item 2). 206/`Content-Range`/416/`Accept-Ranges`.
- `serve_head` (`:1749`) — honors `Range`: satisfiable → body-less 206 with span
  `Content-Length` + `Content-Range` (`head_partial_content_response`, `:1909`), unsatisfiable
  → 416; conditionals first (iteration-3 item 3).
- `precondition_response` (`:1893`) — shared 304/412 mapping (304 carries ETag/Last-Modified
  validators, iteration-1 item), used by both arms.
- Conditionals: `Conditionals::from_headers` (`:2045`), `evaluate_conditionals` (`:2079`) —
  RFC 9110 §13.2.2 precedence with **S3 semantics**: exact opaque ETag equality + `*`;
  `If-Match` uses **strong** comparison so a `W/`-weak tag is rejected (iteration-1 item).
- Range grammar: `digits_or_empty` (`:2021`) rejects any non-ASCII-digit byte in either
  position, so `bytes=+8-+15` and interior-whitespace specs are **malformed → full 200**
  (iteration-2 item 2), matching real S3.
- HTTP-date: `parse_http_date` (`:2161`) dispatches across all three RFC 9110 §5.6.7 formats
  — `parse_imf_fixdate` (`:2172`), `parse_rfc850_date` (`:2197`), `parse_asctime_date` (`:2222`)
  — sharing `month_index` (`:2245`) and `ymd_hms_to_epoch` (`:2260`, pre-1970 clamp so a
  pre-epoch `If-Unmodified-Since` fires 412 not fail-open, iteration-2 item 1; leap-year/time
  validation). A genuinely malformed value returns `None` → ignored (RFC 9110 §13.1.3-4).
  Obsolete RFC-850/asctime dates are now honored (iteration-3 item 1 — the fail-OPEN fix).

## Tests

### `crates/server/tests/s3_range_conditional.rs` — wire-only red→green discriminator
12 `#[tokio::test]`s over a real loopback `S3Gateway::serve` on the production
`Gateway<RedbMetadataStore, CountingChunkStore, MemCoordination>`. **No new production symbol
is imported**, so it compiles on the bare base and fails by **assertion**, not a compile
error (the C4-verify red leg). The counting store (`CountingChunkStore`, `:95`; `get_fragment`
counter `:106`) **wraps the real `FsChunkStore`** — the anti-discard oracle
(`narrow_range_fetches_only_the_covering_chunks`, `:706`). Carry-forward wire cases:
`pre_epoch_if_unmodified_since_fires_412_not_ignored` (`:523`),
`obsolete_http_date_formats_are_honored_on_conditionals` (`:603`),
`head_honors_range_with_206_and_416` (`:653`),
`out_of_scope_and_malformed_ranges_answer_full_200_with_accept_ranges` (`:354`).

### `crates/gateway-s3/src/lib.rs` `#[cfg(test)] mod tests` — version-skew seam guard
`VersionSkewGateway` (`:4204`) drives the REAL `S3Gateway` router/dispatch → `serve_get`
against a gateway whose `head_object` and body seams report **divergent** versions (the
deterministic stand-in for a racing overwrite) and asserts a stale-head `If-Match` 412s on
both the ranged and unranged paths. It references the new `RangeRead`/`ObjectRead` symbols, so
it ships with the fix (it cannot live in the base-red wire discriminator without turning the
red leg into a compile error).

## Refuting my own test (forced, recorded)

- **(a) Genuine red?** YES. `./engine/scripts/run-verify.sh` (the project's C4-verify runner,
  `WYRD_VERIFY_BASE=0fed4c8`) applied `patch.diff` to a **clean** worktree at the folded base,
  reverted only the production files (keeping the test), and re-ran the suite:
  **11/12 fail by assertion** (`left: 200, right: 206` at `s3_range_conditional.rs:300`;
  `left: 200, right: 412` for `If-Match`/`If-Unmodified-Since`; `left: None, right: Some("bytes")`
  for `Accept-Ranges`; etc.) — compiling cleanly, an assertion red not a compile error. The
  12th (`invalid_conditional_date_is_ignored_not_misparsed`) is green-on-base by design (an
  ignored conditional and a base that ignores everything both yield 200). Then GREEN with the
  fix. `run-verify.sh: PASS — red without the fix, green with it` (exit 0).
- **(b) Production path?** YES. The wire tests sign real requests to a real loopback
  `S3Gateway::serve` over the real `Gateway` composition; the seam test drives the real
  `S3Gateway` router → `serve_get`. `resolve_byte_range`, `get_object_range` (server override),
  `serve_get`/`serve_head`, `evaluate_conditionals`, `parse_http_date` &c. are the shipped
  code, not a copy. The counting store *wraps* the real `FsChunkStore`.
- **(c) Fixture includes the fault?** YES. The anti-discard fixture is a genuine 8-chunk
  object (`with_chunk_size(8)`) and asserts `bytes=8-15` touches exactly the covering chunks
  while the full GET touches all 8 — on the base the same oracle sees the full 200. The
  version-skew fixture INCLUDES the divergent-version fault. The obsolete-date fixture PUTs the
  object "now" so past dates genuinely precede its `modified` (→ IUS 412) and future ones
  follow it (→ IMS 304) — the real second-resolution comparison, not a stubbed verdict.

## Gates run locally (worktree, with the fix applied)

- `cargo fmt -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-server -- --check`: clean (exit 0).
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings`:
  clean (exit 0).
- `typos` tree-wide: clean (exit 0). No `typos.toml` exception added — the iteration-1
  gating typos failure does not recur (`TOCTOU` in prose is an all-caps acronym, not flagged;
  the camelCase form was renamed `VersionSkewGateway` back in v3).
- `cargo deny check` + the two `--all-features` invocations: all exit 0 (the iteration-4
  blocker — advisory-DB lock now available).
- Red→green: `run-verify.sh` PASS (11/12 base-red by assertion; 12/12 green with fix).
- No regressions: `wyrd-gateway-s3` lib (70/70, incl. the version-skew seam guards),
  `s3_http_wire` (19/19), `s3_object_metadata` (2/2), `request_capacity_planes` (6/6),
  `wyrd-gateway-core` lib (compiles, 0 unit tests) — all green.

I did not run the full multi-minute `cargo xtask ci` (build + DST sweeps of 50 seeds +
conformance vectors) here — I verified every commit-gating sub-check (fmt / clippy -D /
typos / the touched + adjacent suites / the whole `cargo deny` wall / the red→green
discriminator). The driver's C4-ci and C4-verify re-run the real suite at Check on this same
worktree.

## Explicitly NOT re-opened (per prior sign-offs)
- The iteration-2 dismissal: wildcard `If-Match`/`If-None-Match` on pre-ADR-0047 records
  without a stored ETag (all current objects carry SHA-256 ETags; a legacy-only §10 Act
  candidate).
- The brief-sanctioned out-of-scope set: multi-range, `If-Range`, multi-ETag lists,
  conditional PUT, CopyObject conditionals.
- The minor advisory observations left advisory by the iteration-3 sign-off (case-insensitive
  `BYTES=`, multi-tag `If-Match` list, pre-epoch clamp blind spot) and the iteration-4
  adversary nits (`parse_rfc850_date` unvalidated weekday token; repeated-`Range`-header
  `.get()` first-line behavior) — vanishingly-rare inputs, failure direction matches the
  parser's deliberate liberality; not chased.

## No NEEDS-HUMAN from Do
The whole success criterion is exercised headlessly in-process (real loopback gateway); no
irreducibly-GUI/IO behaviour, no external dependency beyond the base toolchain (cargo 1.96.0,
clippy, typos, cargo-deny 0.20.2 — all present). The brief's off-Check `aws s3api
get-object --range` acceptance is a registered doctor row ("aws cli (S3 gateway
round-trip)") and is not required to prove the criterion. The T4 prior-art-history row is a
checkout property for the human to weigh at sign-off, not a builder-resolvable item.
