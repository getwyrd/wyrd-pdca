# Build notes — issue 510 / range-conditional-get (Iteration 2)

Target branch: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`) off the folded base
`0fed4c8 pdca-integrate: issue_507`. 507 is already on this base, so the brief's cited line
numbers are shifted (GET/HEAD dispatch is `crates/gateway-s3/src/lib.rs:~1615`/`:~1674`, not the
brief's `:909`/`:968`). All line citations below are the **post-fix** worktree.

## What this iteration is

Iteration 1's design was accepted by the adversary as sound (red→green proof held, production
path real, anti-discard oracle sound — see `iteration-v1/check-advisory-adversary.md`). It was
rejected only for **five implementation-level defects**, one gating. This iteration keeps the v1
design **unchanged** (seam shape, ordered gates, span-carrying read) and fixes exactly those five,
each with a binding test. I re-applied v1's `patch.diff` to the folded base and made five surgical
edits on top.

The v1 rationale for the architecture (separate `get_object_range` seam with an `Ok(None)` default
mirroring `head_object`/`list_container`; conditionals-before-range; `head_object`-first metadata
resolve so a 304/412/416 costs no chunk read; span `Content-Length` for the #364 truncation
invariant; raw-wire test harness) is preserved verbatim from `iteration-v1/build-notes.md` and is
still the operative reasoning — I do not repeat it in full here; this file focuses on the five
deltas.

## The five carry-forward fixes

### 1. (GATING) typos: `unparseable` → `unparsable`, `mis-parsed` → `misparsed`
The v1 red was `cargo xtask ci` → `typos` (exit 2), caused by this patch's **own new comments**,
not the environment. The codebase already spells it `unparsable`
(`crates/gateway-s3/src/lib.rs:1354`,`:1377`, pre-existing), and `typos` (`typos-cli 1.48.0`) flags
`unparseable` and the `mis-` in `mis-parsed`. Reworded the four doc occurrences
(`gateway-s3/src/lib.rs:2006`,`:2056`,`:2112`,`:2176` doc region) plus two I introduced in the test
comments (`s3_range_conditional.rs:475`,`:495`). Verified `typos` runs clean **tree-wide** now (not
just the touched files). No `typos.toml` exception added — the words are genuine misspellings, so
the "encode the decision" discipline says fix them, not allowlist them.

### 2. Invalid HTTP-date is now **ignored**, not misparsed (RFC 9110 §13.1.4)
`days_from_civil` accepted `day <= 31` for *every* month, so `Mon, 30 Feb 2026 …` rolled through
the civil arithmetic into ≈2026-03-02 and fired the precondition (verified live in v1: a
`If-Unmodified-Since: 30 Feb 2026` answered 412, contradicting the code's own doc). Fix
(`crates/gateway-s3/src/lib.rs:2177`): validate the day against the month's **real length** via a
new leap-year-aware `days_in_month` (`:2192`) + `is_leap_year` (`:2203`), so an impossible calendar
date fails parsing → `parse_http_date` returns `None` → the conditional is ignored → 200. The check
lives in `days_from_civil` because its doc already promised `None` on an out-of-range day; this
makes the promise true rather than moving the concern.

### 3. `If-Match` uses **strong** comparison — a weak tag never matches (RFC 9110 §13.1.1)
`etag_matches` stripped `W/` before comparing on *both* legs, so `If-Match: W/"<stored>"` answered
200 (verified live in v1). RFC 9110 mandates strong comparison for If-Match. Fix: `etag_matches`
takes a `strong: bool` (`crates/gateway-s3/src/lib.rs:2112`); under strong comparison a
`W/`-prefixed value returns `false` immediately (`:2120`). Callsites:
`evaluate_conditionals` passes `true` for If-Match (`:2070`) and `false` for If-None-Match (`:2085`
— weak comparison, which is the RFC-correct function for If-None-Match anyway). This is a **narrow**
change: it does not add weak-tag *support* (still out of scope per the brief); it only refuses to
weak-match where the RFC forbids it. A positive control in the test proves the *strong* match still
serves the object (guards against over-rejection).

### 4. `Range: bytes=-0` now answers **416**, not 200
A zero-length suffix is grammatically valid but unsatisfiable (RFC 9110 §14.1.1/§15.5.17; real S3
answers `InvalidRange`), so it belongs on the 416 side of the brief's "mirror real S3" line, not the
malformed→200 side. v1 mapped suffix `0` to `None` → full 200. Fix: `parse_range` parses `bytes=-0`
to `RangeSpec::Suffix(0)` (`crates/gateway-s3/src/lib.rs:1952`) — distinct from a *malformed* value
which still parses to `None` → 200 — and `resolve_range` rejects `Suffix(0)` as `Unsatisfiable`
(`:1995`, `size == 0 || n == 0`). A truly malformed `bytes=-` (empty) still parses to `None` → 200,
so the malformed→200 contract is untouched.

### 5. The 304's cache validators are now **tested**
v1's 304 responses already carried `ETag`/`Last-Modified` (`not_modified_response`,
`gateway-s3/src/lib.rs:1860`), but both 304 tests asserted only status + empty body, so a regression
dropping the validators — which breaks every client's cache-revalidation loop — stayed green. Added
`header_value(...)` assertions to both 304 tests: the If-None-Match test asserts the 304 echoes the
object's ETag and carries Last-Modified (`s3_range_conditional.rs:410-421`, GET **and** HEAD); the
date-conditional test asserts the same on the If-Modified-Since 304 (`:528-540`).

## Test — `crates/server/tests/s3_range_conditional.rs` (9 tests, up from 7)

Unchanged harness (raw-wire signed requests over a real loopback `S3Gateway::serve` composed on the
real `Gateway<RedbMetadataStore, CountingChunkStore, MemCoordination>` — the production wire path,
no new production symbol imported so it compiles on the bare base and fails by **assertion**). Two
new tests + three strengthened assertions bind the four behavioural fixes (2–5):
- `unsatisfiable_range_returns_416_with_content_range_star` now also lists `bytes=-0` (fix 4).
- `if_match_uses_strong_comparison_weak_tag_rejected` — new (fix 3): strong-match → 200 (positive
  control), weak-of-same-etag → 412.
- `invalid_conditional_date_is_ignored_not_misparsed` — new (fix 2): `30 Feb 2026` → 200, not 412.
- the two 304 tests gained validator assertions (fix 5).

## Refuting my own test (forced, recorded)

- **(a) Genuine red?** YES, proven two ways.
  - *Canonical base-red:* reverted **all three** production files to the base (`git checkout HEAD --`
    the three `lib.rs`, keeping the untracked test) and ran the test on the bare folded base: **8 of
    9 fail by assertion** (206/416 where a full-200 base can't answer them, 304/412 → 200, oracle
    `distinct==1` → 8). It compiled cleanly (no new symbol imported), so these are assertion reds,
    not a compile error — satisfying the C4-verify red leg. The 9th
    (`invalid_conditional_date_is_ignored_not_misparsed`, asserts 200) is green on the base **by
    design**: an ignored conditional and a base that ignores everything both yield 200. That one is a
    regression guard for fix 2, not a base discriminator — so I bound it separately:
  - *Per-fix targeted reds:* reverted each of the four behavioural fixes to its buggy v1 form (kept
    the tests) and confirmed exactly the guarding assertion goes red: `bytes=-0` → 206 (fix 4),
    `If-Match: W/…` → 200 (fix 3), `30 Feb` → 412 (fix 2), and dropping the 304 validators →
    `etag: None` on the 304 (fix 5). Restored, all 9 green again. So every new/changed assertion
    fails without its fix, including the one that is green on the bare base.
- **(b) Production path?** YES. The test signs real requests to a real loopback `S3Gateway::serve`
  over the real `Gateway` composition (the `serve_s3` path the CLI runs). `evaluate_conditionals`,
  `etag_matches`, `parse_range`/`resolve_range`, `days_from_civil`/`days_in_month`,
  `not_modified_response`, and `get_object_range` are the shipped production code, not a copy. The
  counting store *wraps* the real `FsChunkStore` (delegates every op).
- **(c) Fixture includes the fault?** YES. The anti-discard oracle uses a genuine 64-byte/8-chunk
  object (`with_chunk_size(8)` + `EcScheme::None`) and asserts the narrow `bytes=8-15` GET touches
  exactly the one covering chunk, measured through the wrapped real store, with a full-GET baseline
  proving all 8 are otherwise fetched. The new fix-3/fix-2 fixtures use the object's *own* etag /
  Last-Modified read back off a real GET, so the weak-tag and impossible-date cases are exercised
  against the real stored metadata.

## Gates run locally (worktree)

- `typos` (tree-wide): clean — the gating v1 red is resolved.
- `cargo fmt --check -p wyrd-gateway-s3 -p wyrd-gateway-core -p wyrd-server`: clean.
- `cargo clippy -p wyrd-gateway-core -p wyrd-gateway-s3 -p wyrd-server --tests -- -D warnings`:
  clean.
- New test red→green: green with fix (9/9), red without (canonical 8/9 base-red + per-fix targeted
  reds), proven above.
- No regressions: `wyrd-gateway-s3` lib (67 unit tests), `s3_http_wire` (19), `s3_object_metadata`
  (2) all pass — the GET/HEAD refactor and the etag/date fixes did not disturb the access-log /
  metrics finish path or the metadata round-trip.
- `patch.diff` verified to `git apply --check` cleanly onto a fresh checkout of the folded base
  `0fed4c8`, and to match the restored worktree byte-for-byte.

I did **not** run the full `cargo xtask ci` (build+deny+conformance+full test+DST sweeps — many
minutes): v1's `check-gates.json` showed C4-ci's only red was the typos exit-2, now fixed, and I
verified the specific sub-checks that gate a commit (fmt/clippy/typos/tests). The driver's C4-ci and
C4-verify re-run the real suite at Check.

## Observation carried forward (no action — brief-conformant)
HEAD ignores `Range` (the brief scoped range to GetObject only); the adversary recorded this as a
known, chosen divergence from real S3's HeadObject. Unchanged.

## No NEEDS-HUMAN
The whole success criterion is exercised headlessly in-process; no irreducibly-GUI/IO behaviour, no
external dependency beyond the base toolchain. The brief's off-Check `aws s3api get-object --range`
acceptance is already a registered doctor row and is not required to prove the criterion.
