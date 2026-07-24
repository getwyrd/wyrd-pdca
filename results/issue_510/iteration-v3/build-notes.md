# Build notes — issue 510 / range-conditional-get (Iteration 3)

Target: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`) off the folded base
`0fed4c8 pdca-integrate: issue_507`. 507 is already on this base, so the brief's cited line
numbers are shifted (the GET/HEAD dispatch is `crates/gateway-s3/src/lib.rs:~1615`/`~1674`, not
the brief's `:909`/`:968`). All line citations below are the **post-fix** worktree.

## What this iteration is

Iterations 1 & 2 landed the range/conditional feature; the adversary accepted the architecture
and red→green as sound. The v2 patch was rejected only at **human sign-off** for four confirmed
findings (brief `## Iteration 2 — carry-forward`). This iteration rebuilds the feature from the
base and addresses **exactly those four**, plus keeps the two v1/v2 fixes the sign-off did not
re-open (strong `If-Match`, leap-year date validation) and the iteration-1 gating typos discipline.

The overall shape from v2 is preserved (thin verb table → `serve_get`/`serve_head`; conditionals
before range; span `Content-Length` for the #364 truncation invariant; raw-wire test). The four
carry-forward items forced two of those pieces to be **re-shaped**, described below.

## The four sign-off carry-forward fixes

### 1. Pre-epoch `If-Unmodified-Since` fires 412, not silently ignored
`parse_http_date` computed epoch seconds as an `i64` (correct across the epoch) but returned
`u64::try_from(total).ok()` — `None` for any pre-1970 date. A `None` makes the caller **ignore**
the conditional, and for If-Unmodified-Since "ignore" inverts the answer: an object modified after
a pre-epoch instant must **fail** the precondition (412), not serve 200. Fix
(`crates/gateway-s3/src/lib.rs:2119`): `Some(total.max(0) as u64)` — a pre-1970 IMF-fixdate is
well-formed (RFC 9110 puts no floor on the date), so it clamps to epoch 0 rather than failing the
parse. Clamped to 0, every real object (`modified > 0`) compares as "modified after" → IUS fires
412 and IMS serves 200, both correct. Bound by `pre_epoch_if_unmodified_since_fires_412_not_ignored`
(`crates/server/tests/s3_range_conditional.rs:527`).

### 2. `+`-signed / interior-whitespace range spec is malformed → full 200
`u64::from_str` tolerates a leading `+` (`"+8".parse()` == `Ok(8)`), and v2's `parse_range` also
`trim()`-med each position, so `bytes=+8-+15` and `bytes=8 -15` were wrongly honoured as 206. Real
S3 treats any such non-digit byte as malformed and serves the full 200 (the brief's decided
behaviour for malformed values). Fix: a new `digits_or_empty` helper
(`crates/gateway-s3/src/lib.rs:2018`) accepts a position only when it is **empty** or **all ASCII
digits** — no sign, no whitespace, no other byte — and `parse_range`
(`crates/gateway-s3/src/lib.rs:1980`) rejects the whole range (→ full 200) otherwise, with **no**
interior-`trim()` tolerance. `bytes=+8-+15`, `bytes=8 -15`, `bytes=-+5` added to the malformed-forms
test set (`crates/server/tests/s3_range_conditional.rs:381-388`).

### 3. Closed the ranged-GET TOCTOU — meta + bytes from ONE resolve
v2 built the `206` headers from a `head_object` resolve (size for `Content-Range`, ETag,
Last-Modified) and the body from a **separate** `get_object_range(offset, len)` resolve, so a racing
overwrite between the two could emit a version-mixed 206 (fresh bytes labelled with the stale
size/ETag) that poisons an ETag-keyed cache. **The seam was reshaped** so a ranged read returns
its metadata *and* its stream from one inode resolve:

- `get_object_range` now takes an **unresolved** `ByteRange` (byte-offset math, no HTTP vocabulary)
  and returns `Option<RangeRead>` where `RangeRead { meta, outcome }` carries the object's metadata
  and a `RangeOutcome::{Satisfiable{offset,len,stream} | Unsatisfiable}` (`gateway-core/src/lib.rs`
  new types `:82`–`:212`; trait method `:238`). The implementer resolves the `ByteRange` against
  **its own freshly-read size** (`resolve_byte_range`, `gateway-core/src/lib.rs:145`), so the span,
  the `Content-Range` total, and the bytes are all one version. A pre-resolved `(offset, len)` from
  a separate lookup is exactly what would reopen the window, so the seam refuses to carry it.
- `serve_get` frames the `206` (and the `416`'s `bytes */size`) entirely from
  `get_object_range`'s returned `meta` — never a separate `head_object`
  (`crates/gateway-s3/src/lib.rs:1720`). The server override reads the inode once and derives both
  `meta` and the covering-chunk stream from it (`crates/server/src/lib.rs:389`).

Conditionals still run **before** the body (off a metadata-only `head_object`, so a 304/412 costs
no chunk read — the brief's "no read at all" property), and I deliberately did **not** fold the
conditional evaluation into `get_object_range`: that would either pull HTTP conditional vocabulary
into the neutral seam or spawn the chunk reader before the 304/412 decision (losing "no read at
all"). The residual — a passing conditional evaluated against `head_object`'s snapshot while the
206 comes from `get_object_range`'s snapshot — is **benign**: the 206 is internally coherent (bytes
match their labelled ETag/size), so no cache is poisoned; only the precondition-vs-read timing can
differ, which real S3 also exhibits under concurrency. The version-mixed 206 the human named is
eliminated. Bound by `ranged_206_is_framed_from_the_range_seam_not_a_separate_head_resolve`
(gateway-s3 unit test, a double whose `head_object` and `get_object_range` report divergent
versions — the deterministic stand-in for the race).

### 4. Removed the `Ok(None)` trait-default landmine
v2's `get_object_range` default was `Ok(None)`, so a gateway that did not override it answered a
ranged GET of an **existing** object with `404 NoSuchKey` after the wire layer had advertised
`Accept-Ranges: bytes`. The tree has several non-overriding `ObjectGateway` doubles
(`request_capacity_planes.rs`, gateway-s3 tests) plus any future gateway. Fix: a
**correctness-preserving** default (`gateway-core/src/lib.rs:238`): read the whole object through
`get_object_streaming` (one resolve → meta + full stream) and slice the span off wire-side
(`slice_object_stream`, `gateway-core/src/lib.rs:180`). That is the whole-object read the real
gateway avoids for large objects — but as a *default* it is correct and atomic (meta+bytes from the
one streaming resolve), never a 404. `wyrd-server`'s `Gateway` overrides it with the chunk-map walk
that fetches only the covering chunks (the anti-discard oracle runs against that override). Bound by
`a_non_overriding_gateway_serves_ranges_via_the_correctness_preserving_default` (gateway-s3 unit
test using a double that does NOT override the method).

### Explicitly NOT re-opened (per sign-off)
Wildcard `If-Match`/`If-None-Match` on pre-ADR-0047 records without a stored ETag — dismissed at
sign-off as a legacy-only §10 Act candidate. Untouched.

## Tests

### `crates/server/tests/s3_range_conditional.rs` — wire-only (base red→green discriminator)
10 `#[tokio::test]`s over a real loopback `S3Gateway::serve` on the production
`Gateway<RedbMetadataStore, CountingChunkStore, MemCoordination>` composition. **No new production
symbol is imported** (per the brief's falsifiability design), so it compiles on the bare base and
fails by **assertion**, not a compile error — confirmed: on the base 9/10 fail by assertion
(206/416/304/412 where a full-200 base can't answer them, and Accept-Ranges absent), the 10th
(`invalid_conditional_date_is_ignored_not_misparsed`) is green-on-base by design (an ignored
conditional and a base that ignores everything both yield 200). Adds the two carry-forward wire
cases: `pre_epoch_..._fires_412` (item 1) and the `+`/whitespace malformed forms (item 2).

### `crates/gateway-s3/src/lib.rs` `#[cfg(test)] mod tests` — seam-level guards (ship with the fix)
Items 3 & 4 test the ranged **seam** directly, so they must reference the new `ByteRange`/
`RangeRead`/`RangeOutcome` symbols — which means they cannot live in the wire-only discriminator
(that would make the base red leg a compile error). They drive the REAL router
(`S3Gateway::router()` → `dispatch` → `serve_get`) against hand-crafted `ObjectGateway` doubles, so
they exercise production wiring, and they ship with the fix as regression guards (not base
discriminators):
- `a_non_overriding_gateway_serves_ranges_via_the_correctness_preserving_default` (item 4).
- `ranged_206_is_framed_from_the_range_seam_not_a_separate_head_resolve` (item 3).

## Refuting my own test (forced, recorded)

- **(a) Genuine red?** YES, proven two ways.
  - *Canonical base-red:* stashed the three production `lib.rs` files back to the folded base
    (keeping the untracked wire test), re-ran `s3_range_conditional.rs`: **9/10 fail by assertion**,
    compiling cleanly — an assertion red, not a compile error (satisfies the C4-verify red leg).
  - *Per-fix targeted reds* (each fix reverted to its buggy form, its guarding assertion re-run):
    - item 1 → `u64::try_from(total).ok()`: `pre_epoch_..._fires_412` goes red (200 vs 412).
    - item 2 → lenient `trim()+parse`: the malformed test goes red (`bytes=+8-+15` → 206 vs 200).
    - item 3 → frame the 206 from a separate `head_object`: the seam test goes red (`bytes 8-17/100`
      stale head vs `bytes 8-17/40` fresh range seam).
    - item 4 → `Ok(None)` default: the default test goes red (404 vs 206).
    All four restored; every test green again (10 wire + 2 seam).
- **(b) Production path?** YES. The wire tests sign real requests to a real loopback
  `S3Gateway::serve` over the real `Gateway` composition; the seam tests drive the real
  `S3Gateway::router()` dispatch → `serve_get`. `parse_range`/`digits_or_empty`, `resolve_byte_range`,
  `evaluate_conditionals`, `etag_matches`, `parse_http_date`/`days_from_civil`, the `get_object_range`
  override and its trait default are the shipped code, not a copy. The counting store *wraps* the
  real `FsChunkStore`.
- **(c) Fixture includes the fault?** YES. The anti-discard oracle uses a genuine 64-byte/8-chunk
  object (`with_chunk_size(8)` + `EcScheme::None`) and asserts a `bytes=8-15` GET touches exactly
  the one covering chunk through the wrapped real store, with a full-GET baseline proving all 8 are
  otherwise fetched. The item-3 fixture INCLUDES the fault it guards — a gateway whose `head_object`
  and `get_object_range` report divergent versions (size 100/`stalehead` vs size 40/`freshbody`),
  the deterministic stand-in for a racing overwrite — and asserts the 206 names the body's version.
  The item-4 fixture is a gateway that genuinely does NOT override the method (uses the default).

## Gates run locally (worktree)

- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-server`: clean (after
  `cargo fmt`).
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings`:
  clean.
- `typos` **tree-wide**: clean (exit 0). The iteration-1 gating class recurred once — `typos` split
  the camelCase double name `TocTou…` into a `Tou`→`You` flag — so it was renamed
  `VersionSkewGateway`; the all-caps acronym `TOCTOU` in prose is not flagged. No `typos.toml`
  exception added.
- New tests red→green: green with fix (10 wire + 2 seam), red without (canonical 9/10 base-red +
  four per-fix targeted reds), proven above.
- No regressions: `wyrd-gateway-s3` lib (69), `s3_http_wire` (19), `s3_object_metadata` (2),
  `request_capacity_planes` (its non-overriding `FaultyGateway`/`MidStreamFaultGateway` now use the
  new default) — all pass.
- `patch.diff` verified to `git apply --check` cleanly onto the stashed clean base `0fed4c8` (all
  four files).

I did **not** run the full `cargo xtask ci` (build+deny+conformance+full DST sweeps — many minutes):
I verified the specific sub-checks that gate a commit (fmt/clippy/typos/the touched test suites).
The driver's C4-ci and C4-verify re-run the real suite at Check.

## No NEEDS-HUMAN
The whole success criterion is exercised headlessly in-process; no irreducibly-GUI/IO behaviour, no
external dependency beyond the base toolchain. The brief's off-Check `aws s3api get-object --range`
acceptance is already a registered doctor row and is not required to prove the criterion.
