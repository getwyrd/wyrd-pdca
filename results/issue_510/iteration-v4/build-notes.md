# Build notes — issue 510 / range-conditional-get (Iteration 4)

Target: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`) off the folded base
`0fed4c8 pdca-integrate: issue_507`. 507 is already on this base, so the brief's cited line
numbers are shifted (the GET/HEAD dispatch is `crates/gateway-s3/src/lib.rs:~1654`/`~1749`, not
the brief's `:909`/`:968`). All line citations below are the **post-fix** worktree.

## What this iteration is

Iterations 1–3 landed the range/conditional feature; the adversary and human sign-off accepted the
architecture and red→green as sound. The v3 patch was rejected at human sign-off for **exactly three
remaining findings** (brief `## Iteration 3 — carry-forward`). This iteration rebuilds the feature
from the folded base (starting from v3's accepted patch) and addresses **only those three**, keeping
every v1/v2/v3 fix and test the sign-off confirmed and did NOT re-open (strong `If-Match`, leap-year
date validation, pre-1970 IUS clamp, `+`-signed malformed range, the atomic ranged seam / item-3 v3,
the correctness-preserving trait default / item-4 v3, the typos discipline).

The three carry-forward items and how each is fixed:

### 1. Obsolete HTTP-date formats are honoured — `If-Unmodified-Since` no longer fails OPEN
v3's `parse_http_date` accepted only the 29-char IMF-fixdate. RFC 9110 §5.6.7 requires a recipient to
accept **all three** HTTP-date formats; an `If-Unmodified-Since` carrying an obsolete RFC-850 or
asctime date went unparsed → ignored → served `200` where `412` is conformant (a fail-OPEN — for IUS,
"ignore" inverts the answer). Fix: `parse_http_date` (`crates/gateway-s3/src/lib.rs:2161`) now
dispatches across the three formats — `parse_imf_fixdate` (`:2172`), `parse_rfc850_date` (`:2197`,
two-digit year disambiguated the way the `httpdate` crate does — pivot at 70, a fixed rule with no
clock dependency), `parse_asctime_date` (`:2222`, space-padded day) — sharing `month_index` (`:2245`)
and `ymd_hms_to_epoch` (`:2260`, which keeps the v2 pre-1970 clamp and the leap-year/time validation).
A *genuinely* malformed value (unknown format, `30 Feb`, out-of-range time) still returns `None` so
the caller ignores it (RFC 9110 §13.1.3-4). Bound by
`obsolete_http_date_formats_are_honored_on_conditionals`
(`crates/server/tests/s3_range_conditional.rs:603`): RFC-850 and asctime, each on IUS (past → 412) and
IMS (future → 304).

### 2. The residual check-then-act window on conditionals is closed — one resolve
v3 evaluated conditionals against a **separate** `head_object` snapshot, then read the body from a
**second** resolve (`get_object_streaming` / `get_object_range`). A racing overwrite between the two
could let an `If-Match` pass against the stale head while a self-coherent `206`/`200` of a *different*
version went out under it — the precondition never authorised that version. Fix: `serve_get`
(`crates/gateway-s3/src/lib.rs:1654`) no longer calls `head_object` at all; it evaluates the
conditionals against the metadata of the **same** resolve that yields the body — `RangeRead.meta` from
`get_object_range` for a ranged request (`:1690-1702`), and `ObjectRead`'s fields from
`get_object_streaming` for the unranged path (`:1723-1733`). A firing precondition drops the
(possibly-open) body stream unread. This costs a body resolve on a request that turns out 304/412 —
the "a 304/412 costs no body work" trade the sign-off explicitly judged not worth piercing the fence
for. The now-dead `Conditionals::is_present` guard is removed. Bound by
`conditionals_are_judged_against_the_served_version_not_a_stale_head`
(`crates/gateway-s3/src/lib.rs:4350`, a gateway double whose `head_object` reports `stalehead` and
whose body seams report `freshbody` — the deterministic stand-in for a racing overwrite; a stale-head
`If-Match` must 412 on both the ranged and unranged paths).

### 3. HEAD honours `Range` — 206 / 416, not a blanket 200
v3's `serve_head` advertised `Accept-Ranges: bytes` but ignored `Range` (a satisfiable `bytes=8-15`
served 200 with the full size; an unsatisfiable `bytes=999-` served 200 too). Fix: `serve_head`
(`crates/gateway-s3/src/lib.rs:1749`) parses the header (`:1761`), and after the conditionals gate
resolves a satisfiable range from the metadata size alone (`resolve_byte_range`) into a body-less
`206` (`head_partial_content_response`, `:1909`) with `Content-Range` and the SPAN `Content-Length`,
mirroring GET; an unsatisfiable range answers `416` (`:1780-1785`). Metadata-only throughout — a HEAD
never carries a body, so no chunk is read for the span. `finish_response` already records every HEAD as
body-less/complete (`crates/gateway-s3/src/lib.rs:1290`), so the declared span length raises no
truncation flag. Bound by `head_honors_range_with_206_and_416`
(`crates/server/tests/s3_range_conditional.rs:653`): `bytes=8-15` → 206 CL=8 + `Content-Range: bytes
8-15/64`, no body, ETag present; `bytes=999-` → 416 + `Content-Range: bytes */64`.

### Shared helper `precondition_response`
`crates/gateway-s3/src/lib.rs:1893` — maps an `evaluate_conditionals` verdict to a `304`/`412`, used
by the GET (ranged + unranged) and HEAD arms so a precondition answers identically everywhere, judged
against the same `etag`/`modified` the arm resolved.

### Explicitly NOT re-opened (per iteration-3 sign-off)
- The four iteration-2 items (kept their fixes + tests).
- The iteration-2 dismissal: wildcard `If-Match`/`If-None-Match` on pre-ADR-0047 records without a
  stored ETag (a legacy-only §10 Act candidate).
- The brief-sanctioned out-of-scope set: multi-range, `If-Range`, multi-ETag lists, conditional PUT.
- The minor advisory observations (case-insensitive `BYTES=`, multi-tag `If-Match` list, pre-epoch
  clamp blind spot) — left advisory, not chased.

## Tests

### `crates/server/tests/s3_range_conditional.rs` — wire-only (base red→green discriminator)
12 `#[tokio::test]`s over a real loopback `S3Gateway::serve` on the production
`Gateway<RedbMetadataStore, CountingChunkStore, MemCoordination>`. **No new production symbol is
imported**, so it compiles on the bare base and fails by **assertion**, not a compile error. Adds the
two carry-forward wire cases (item 1: `obsolete_http_date_formats_...`; item 3:
`head_honors_range_with_206_and_416`).

### `crates/gateway-s3/src/lib.rs` `#[cfg(test)] mod tests` — seam-level guard (ships with the fix)
Item 2 tests the conditional-vs-body-version fence directly against the new `RangeRead`/`ObjectRead`
seam, so it references those symbols and cannot live in the wire-only discriminator (that would make
the base red leg a compile error). It drives the REAL router
(`S3Gateway::router()` → `dispatch` → `serve_get`) against the existing `VersionSkewGateway` double, so
it exercises production wiring, and ships as a regression guard (not a base discriminator).

## Refuting my own test (forced, recorded)

- **(a) Genuine red?** YES, proven per-fix AND canonically.
  - *Per-fix targeted reds* (each fix reverted to its v3/buggy form, its guarding test re-run):
    - item 1 → `parse_http_date` restricted to `parse_imf_fixdate` only:
      `obsolete_http_date_formats_...` goes red (RFC-850 IUS → 200 vs 412).
    - item 2 → conditionals off a separate `head_object` snapshot (the v3 check-then-act structure):
      `conditionals_are_judged_against_the_served_version_not_a_stale_head` goes red (stale-head
      `If-Match` → 206 vs 412).
    - item 3 → HEAD ignores `Range`: `head_honors_range_with_206_and_416` goes red (200 vs 206).
    All three restored; every test green again (12 wire + 70 gateway-s3 lib).
  - *Canonical base-red:* stashed the three production `lib.rs` files back to the folded base (keeping
    the untracked wire test), re-ran `s3_range_conditional.rs`: **11/12 fail by assertion**, compiling
    cleanly — an assertion red, not a compile error (satisfies the C4-verify red leg). Both new wire
    tests are in the failing set. The 12th (`invalid_conditional_date_is_ignored_not_misparsed`) is
    green-on-base by design (an ignored conditional and a base that ignores everything both yield 200).
- **(b) Production path?** YES. The wire tests sign real requests to a real loopback `S3Gateway::serve`
  over the real `Gateway` composition; the seam test drives the real `S3Gateway::router()` dispatch →
  `serve_get`. `parse_http_date`/`parse_rfc850_date`/`parse_asctime_date`/`ymd_hms_to_epoch`,
  `serve_get`/`serve_head`, `evaluate_conditionals`, `resolve_byte_range`,
  `head_partial_content_response` are the shipped code, not a copy. The counting store *wraps* the real
  `FsChunkStore`.
- **(c) Fixture includes the fault?** YES.
  - Item 1: the object is PUT "now" (2020s), so the past obsolete-format dates genuinely precede its
    `modified` (→ IUS 412) and the future ones genuinely follow it (→ IMS 304) — the fixture exercises
    the real second-resolution comparison, not a stubbed verdict.
  - Item 2: the `VersionSkewGateway` fixture INCLUDES the fault — a gateway whose `head_object`
    (`stalehead`/size100) and body seams (`freshbody`/size40) report divergent versions, the
    deterministic stand-in for a racing overwrite — and asserts the stale-head `If-Match` is rejected.
  - Item 3: the HEAD range fixture is a genuine 64-byte object; `bytes=8-15` is a real satisfiable span
    (asserts 206/CL=8/`bytes 8-15/64`) and `bytes=999-` a real unsatisfiable one (asserts 416).

## Gates run locally (worktree)

- `cargo fmt -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-server -- --check`: clean.
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings`: exit 0,
  clean. (Verified clippy genuinely re-analyses the edited file — a planted `unused_variable` probe
  was caught under `-D warnings`, then removed — so the clean pass is not a stale cache.)
- `typos` **tree-wide**: clean (exit 0). No new `typos.toml` exception; the all-caps acronym `TOCTOU`
  in prose is not flagged (only the camelCase `TocTou…` was, already renamed `VersionSkewGateway` in
  v3).
- New/changed tests red→green: green with fix (12 wire + 70 gateway-s3 lib), red without (canonical
  11/12 base-red + three per-fix targeted reds), proven above.
- No regressions: `wyrd-gateway-s3` lib (70), `s3_http_wire` (19), `s3_object_metadata` (2),
  `request_capacity_planes` (6), `wyrd-gateway-core` lib (compiles) — all pass.
- `patch.diff` verified to `git apply --check` cleanly onto a pristine base checkout of `0fed4c8`
  (scratch worktree, all four files).

I did **not** run the full `cargo xtask ci` (build+deny+conformance+full DST sweeps — many minutes): I
verified the specific sub-checks that gate a commit (fmt/clippy/typos/the touched test suites). The
driver's C4-ci and C4-verify re-run the real suite at Check.

## No NEEDS-HUMAN
The whole success criterion is exercised headlessly in-process; no irreducibly-GUI/IO behaviour, no
external dependency beyond the base toolchain (all present — cargo 1.96, clippy, typos). The brief's
off-Check `aws s3api get-object --range` acceptance is already a registered doctor row and is not
required to prove the criterion.
