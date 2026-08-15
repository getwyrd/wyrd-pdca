# Build notes — issue #691 (multipart key grammar + validated identity types)

## What shipped

Three files, exactly as scoped:

- `crates/core/src/multipart.rs` (new, 783 lines / 454 semantic) — `RecordError`,
  `UploadId`/`AttemptId` (token grammar), `PartNumber`/`SlotIndex` (validating
  constructors + `Deserialize`), `Digest` + `hex_lower`, every key prefix constant,
  `fixed_width_u32`/`canonical_decimal`/`split_key`, the key constructors/range
  prefixes/parsers for all seven keyed classes, and the `RetireMode`/`RetireToken`/
  `parse_retire_mode`/`parse_retire_key`/`retire_key`/`retire_session_range` grammar.
- `crates/core/src/lib.rs:13` — one line, `pub mod multipart;`.
- `crates/core/tests/multipart_keys.rs` (new, 604 lines / 497 semantic) — the five legs.

Semantic-line total (blank + comment-only lines excluded): 454 + 497 = 951, under the
≤1,150 budget (module guideline ≈600, actual 454; test guideline ≈550, actual 497 —
both under, since the salvage was trimmed of everything out-of-scope, see below).

## Why salvage, not re-derivation

The brief names `results/issue_654/iteration-v2/patch.diff`, `multipart.rs` lines
~86–845, as "the primary lever". I opened it (the one cited peer artifact) and used
it as the base for §§1–4 (`RecordError` through the `retire:` key grammar) rather than
re-deriving the grammar from proposal 0016 from scratch — that patch had already been
through one Check round (see `results/issue_654/iteration-v2/check-review.md`) and its
key-grammar shape (fixed-width padding, canonical-decimal helpers, `split_key`,
`RetireToken`'s `s:`/`g:` tags) matches 0016 §1 and the `metadata.rs` house shape
(`seg_key`/`parse_seg_key`, `metadata.rs:1219-1306`) I independently verified against
on this branch. Re-deriving the same grammar from the prose would have cost ~400 lines
of restatement to land at the identical shape, for zero behavioral difference — not a
defensible use of the budget.

What I did NOT carry over from the salvage, and why (this is the actual editing work,
not a copy-paste):

1. **`PART_NUMBER_WIDTH`: 5 → 6.** The brief pins this explicitly (protocol-neutral
   headroom over both S3's 10,000-part cap and Azure's 50,000/100,000 block cap,
   ADR-0046 decision 6 — `docs/design/adr/0046-bucket-model-real-namespace.md:54-57`,
   mirroring `SEG_INDEX_WIDTH`/`MAX_ROOT_SEGMENTS`, `metadata.rs:270-321`). Every
   fixed-width literal that follows from it (`MAX_PART_NUMBER = 999_999`, every test
   table) was recomputed, not just the constant.
2. **Dropped `encode_record`/`decode_record`, `Budget` (the admission-ledger profile
   tuple and its derivations), and the `DuplicatePart`/`NoPartsNamed`/`Structural`
   `RecordError` variants.** The brief's Scope explicitly reserves "every record
   **value** type... the outcome enums... `Budget`... `sha2`... `Cargo.toml`" for
   child-2/child-3. Those three error variants exist in the salvage only to serve
   record-value validation (a named part list, a decoded session record) that this
   child does not ship; keeping them would have been dead, untested surface — and
   `cargo-machete`/dead-code lints would flag it in `cargo xtask ci` even if `pub`
   items don't trip local `cargo check` warnings.
3. **Dropped `Digest::of` (the `sha2`-backed hasher).** The brief is explicit: "`sha2`
   the *dependency* is child-3's, so no `Cargo.toml` change in this child." Shipping
   `Digest::of` would either add the dependency now (violating scope) or fail to
   compile. `Digest` keeps `from_bytes`/`from_hex`/`as_bytes`/`to_hex` — the *shape*
   the next child's `of` populates — matching the brief's "`Digest` the type is here"
   line.
4. **Rewrote the module doc comment** from the salvage's whole-protocol overview (which
   references symbols this child does not ship — `encode_record`, `Answer`,
   `multipart_etag`) to state only this child's actual scope, with the record-value
   material named as "the next child's" rather than described as if present.
5. **`SLOT_INDEX_WIDTH` doc comment**: added the citation the brief calls for
   (`0016:1471`'s clamp arithmetic reaching ≈524,288 at `MAX_PART_CHUNKS = 1`) as the
   concrete reason 5 digits (99,999) is insufficient and 6 (999,999) is pinned — the
   salvage's version didn't carry this number.

## The carried-forward MUST-FIX (v2 review, `multipart_records.rs:162`)

`results/issue_654/iteration-v2/check-review.md:14` flags: "the fixed-width table omits
the required `007` case." `crates/core/tests/multipart_keys.rs`'s `NUMERIC_ADVERSITIES`
table explicitly includes both `"7"` (bare short) and `"007"` (padded-but-short) — see
`multipart_keys.rs:56-61` — applied to every fixed-width class (`slot:`, `part:`,
`psum:`, and the part-number field inside `sidx:`). Negation (a) below demonstrates this
table is load-bearing, not decorative.

## Round-trip + rejection coverage (leg 1) — per adversity, not per class

The brief's adversity list (`+`, short widths incl. bare `7`/padded `007`, over-wide,
non-decimal body, empty upload id, upload id containing `:`, malformed/truncated,
trailing component, non-UTF-8) applies differently by class shape:

- `slot:`/`part:`/`psum:` (upload id + one fixed-width field): full battery via
  `NUMERIC_ADVERSITIES` (13 spellings) + 5 structural cases (`check_scoped_numeric_key`,
  `multipart_keys.rs:172-215`).
- `sidx:` (upload id + fixed-width part number + canonical-decimal chunk id): the same
  `NUMERIC_ADVERSITIES` battery at the part-number field, PLUS a canonical-decimal
  battery at the chunk-id field (`"007"`, `"+7"`, `"-7"`, `" 7"`, `"7 "`, `"abc"`, `""`)
  — the chunk id is deliberately NOT fixed-width (0016 doesn't bound chunk-id digit
  count), so it is checked against the *canonical-decimal* rule instead, and `"007"` is
  rejected there for the same reason it is in the fixed-width fields: two spellings of
  one record.
- `mpu:` (upload id only, no numeric field): empty/colon-embedded upload id,
  malformed/wrong-prefix, non-UTF-8. The `+`/width/non-decimal-body adversities don't
  literally apply (no numeric field exists to spell them against) — noted in a comment
  at the test rather than silently skipped.
- `retire:bytes:`/`retire:records:` (token grammar: `s:`/`g:` tag + upload/inode ids +
  canonical-decimal epoch/version/part-number + attempt id): unknown mode, a token of
  neither `s:` nor `g:` form, `+` on the epoch, a leading-zero (non-canonical) epoch, a
  non-decimal epoch body, empty/colon-embedded upload id, truncated, two shapes of
  trailing component (3-field and 5-field), a non-decimal generation version, non-UTF-8.

## The two named negation demonstrations (binding, per the brief's Falsifiability)

Both run against the FIXED test with the change reverted immediately after capture.

**(a) One fixed-width parser accepts a short `007` spelling.**
Change: `crates/core/src/multipart.rs`, `fixed_width_u32` — `text.len() != width` →
`text.len() > width` (accepts anything up to and including the canonical width, so `"7"`
now passes the length gate). Ran `cargo test -p wyrd-core --test multipart_keys`:

```
test psum_key_round_trips_and_rejects_noncanonical ... FAILED
test part_key_round_trips_and_rejects_noncanonical ... FAILED
test sidx_key_round_trips_and_rejects_noncanonical ... FAILED
test slot_key_round_trips_and_rejects_noncanonical ... FAILED

---- psum_key_round_trips_and_rejects_noncanonical stdout ----
thread 'psum_key_round_trips_and_rejects_noncanonical' panicked at crates/core/tests/multipart_keys.rs:68:5:
psum: non-canonical body "7": expected rejection for key "psum:0123456789abcdef0123456789abcdef:7", got Ok((UploadId("0123456789abcdef0123456789abcdef"), PartNumber(7)))

---- part_key_round_trips_and_rejects_noncanonical stdout ----
thread 'part_key_round_trips_and_rejects_noncanonical' panicked at crates/core/tests/multipart_keys.rs:68:5:
part: non-canonical body "7": expected rejection for key "part:0123456789abcdef0123456789abcdef:7", got Ok((UploadId("0123456789abcdef0123456789abcdef"), PartNumber(7)))

---- sidx_key_round_trips_and_rejects_noncanonical stdout ----
thread 'sidx_key_round_trips_and_rejects_noncanonical' panicked at crates/core/tests/multipart_keys.rs:68:5:
sidx: non-canonical part number "7": expected rejection for key "sidx:0123456789abcdef0123456789abcdef:7:7", got Ok((UploadId("0123456789abcdef0123456789abcdef"), PartNumber(7), 7))

---- slot_key_round_trips_and_rejects_noncanonical stdout ----
thread 'slot_key_round_trips_and_rejects_noncanonical' panicked at crates/core/tests/multipart_keys.rs:68:5:
slot: non-canonical body "7": expected rejection for key "slot:0123456789abcdef0123456789abcdef:7", got Ok((UploadId("0123456789abcdef0123456789abcdef"), SlotIndex(7)))

test result: FAILED. 12 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Reverted (`text.len() != width` restored); re-ran → 16/16 green.

**(b) One prefix spelled without its trailing separator (`mpu` for `mpu:`).**
Change: `crates/core/src/multipart.rs`, `MPU_PREFIX: &[u8] = b"mpu:"` → `b"mpu"`. Ran
`cargo test -p wyrd-core --test multipart_keys`:

```
test scan_mpu_does_not_reach_the_mpuctl_singleton ... FAILED

---- scan_mpu_does_not_reach_the_mpuctl_singleton stdout ----
thread 'scan_mpu_does_not_reach_the_mpuctl_singleton' panicked at crates/core/tests/multipart_keys.rs:427:5:
assertion failed: !MPUCTL_KEY.starts_with(MPU_PREFIX)

test result: FAILED. 15 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Note: `no_key_prefix_is_a_prefix_of_another` (the general leg-3 sweep) stayed GREEN
under this negation — it only compares the seven-plus-eight *prefixes* pairwise, and
`MPUCTL_KEY` is a singleton key, not a prefix, so it isn't in that list. Only the
brief's specifically-named near-miss test caught it, exactly as the brief anticipated
("the two named near-misses" get their own assertions, not just the general sweep).
This is why I kept `scan_mpu_does_not_reach_the_mpuctl_singleton` and
`sidx_is_not_reachable_from_a_pending_scan` as separate tests from
`no_key_prefix_is_a_prefix_of_another` rather than folding the near-misses into the
general sweep.

Reverted (`b"mpu:"` restored); re-ran → 16/16 green.

## The pre-declared RED leg (posture (a), born-at-tier)

Confirmed directly: with the test file kept and `multipart.rs` deleted + the `lib.rs`
mod line reverted, `cargo test -p wyrd-core --test multipart_keys` fails to compile:

```
error[E0432]: unresolved import `wyrd_core::multipart`
  --> crates/core/tests/multipart_keys.rs:20:16
   |
20 | use wyrd_core::multipart::{
   |                ^^^^^^^^^ could not find `multipart` in `wyrd_core`
error: could not compile `wyrd-core` (test "multipart_keys") due to 1 previous error
```

Exit 101 (a compile failure), matching the brief's pre-declared posture: C4-verify
reports this as UNVERIFIABLE (exit 77) rather than a false RED, and that's the expected,
pre-declared §6 item — not a defect. Restored `multipart.rs` + the `lib.rs` line; re-ran
→ 16/16 green again.

## Three refutation questions (forced self-check before declaring done)

**(a) Genuine red?** Yes — demonstrated three ways above: the declared compile-failure
RED (multipart.rs + lib.rs line reverted), and the two negation demonstrations (a
narrower, more informative red than a blanket revert since each isolates exactly the
property it defeats). Every one of the 5 legs has at least one test that goes red under
a targeted defect in the area it claims to cover.

**(b) Production path?** Yes — every test calls `wyrd_core::multipart::{parse_*, *_key,
*_range, RetireToken, RetireMode, UploadId, AttemptId, PartNumber, SlotIndex, Digest}`
directly; there is no mock, stand-in, or re-implementation. The one place a test reaches
into `wyrd_core::metadata` is to import `metadata::{ORPHAN_PREFIX, SEG_PREFIX,
SEGGRP_PREFIX}` (leg 3's pre-existing prefixes) — also production code, not a copy.

**(c) Fixture includes the fault?** Yes — every rejection case constructs the exact
malformed byte string by hand (no builder that could accidentally exclude the bad
case), and the two negation demonstrations edit the production module directly (not a
test-side double) to prove the assertions are load-bearing against the real parser.

## Commit-readiness

- `cargo fmt -p wyrd-core -- --check` → clean (ran `cargo fmt -p wyrd-core` once to fix
  four wrapping diffs the initial draft had, then re-checked clean).
- `cargo clippy -p wyrd-core --all-targets -- -D warnings` → clean (fixed four
  `clippy::useless_format` findings in the test file — `format!("literal")` → the byte
  literal directly, since those four cases had no interpolation).
- `cargo test -p wyrd-core` (the whole crate, not just the new file) → all pre-existing
  tests + doctests still green; nothing else in the crate was touched.

## Self-review against the target's `AGENTS.md` "Review rubric & protocol" (read per
the repo-policy exception; only that section)

- *Metadata validation boundaries* (hard convention): this module IS the worked example
  — every structural violation is a typed `RecordError`, never a defaulted value, and
  the module doc's "Structural validity is a type, not a convention" section states the
  rule explicitly. Compliant by construction.
- *Every new crate root carries `#![forbid(unsafe_code)]`*: `multipart.rs` is a module
  inside the existing `wyrd-core` crate (whose root, `lib.rs:9`, already carries the
  forbid) — not a new crate root, so N/A for the module. The test file IS its own crate
  root (Rust integration-test convention) and carries `#![forbid(unsafe_code)]` at
  `multipart_keys.rs:19`, matching the sibling test files' pattern (e.g.
  `segmented_map_record.rs:32`).
- *No DST-reachable shared mutable global state*: no statics/globals introduced.
- *Docs currency* ("adds or alters a port, an API operation, an RPC, a CLI flag, or a
  persisted field"): this child adds no live-reachable surface — no store I/O, no
  consumer wired (brief's own "Production reach: N/A"). Nothing to update.
- *Grammar strictness* (recurring defect class): directly on point and directly
  followed — `fixed_width_u32`/`canonical_decimal` reject `+`/`-` signs, wrong digit
  widths, and non-canonical leading zeros exactly as the rubric's RFC-parser rule
  states for a different surface (dates/ranges); this module is the same discipline
  applied to key fields.
- *Serialization identity* (recurring defect class): N/A this child — no record
  **value** types ship here (child-2's), so there is no decode→encode round trip to
  test yet. The identity newtypes use `#[serde(transparent)]` (plain-string wire
  form, matching `SegmentNonce`'s existing pattern, `metadata.rs:735` on this branch).
- No other recurring defect class (transactions, await discipline, probes, test
  fidelity, workflow edits) applies — this slice has no store I/O, no async, no
  DST/sim path, and touches no CI workflow file.

## What I ruled out

- **Re-deriving the grammar from 0016 prose instead of salvaging**: ruled out on cost —
  the salvage's shape already matches the cited peer house-shape (`seg_key`/
  `parse_seg_key`) and 0016 §1; re-deriving would have spent ~400 lines reproducing an
  identical result for no behavioral gain, against a budget that already tightened once
  I trimmed the out-of-scope material.
- **Keeping `Digest::of`/`sha2` "for completeness"**: ruled out — the brief pins this to
  child-3 explicitly and scoping it out keeps `Cargo.toml` untouched, which the
  Falsifiability section's file classification depends on (`ADDED_TEST` +
  `CRATE crates/core`, no `Cargo.toml` diff).
- **A single "test everything against every class" matrix test**: ruled out in favor of
  one test per class (`slot_key_round_trips_and_rejects_noncanonical`, etc.) — a single
  giant test would still exercise the same assertions, but a failure inside it wouldn't
  name which class or adversity broke without reading the panic message closely; five
  small, class-scoped tests plus the two structural-property tests (order, disjointness)
  give a failing-test NAME that already says what regressed.
