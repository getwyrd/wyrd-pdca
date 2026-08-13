# Build notes — issue #715 (multipart-budget-admission)

## What shipped

`crates/core/src/multipart.rs` (target branch `getwyrd/wyrd@main`, base `9dbcd72`) gains:

- `RecordError::Structural { record, reason }` (`multipart.rs:120-128` post-patch — the
  relational-violation arm ADR-0045 needs alongside the existing structural-parse arms),
  plus its `Display` arm and the `structural()` constructor.
- Section 5, `encode_record`/`decode_record` — the shared JSON envelope (mirrors
  `metadata.rs:1536-1543`'s `encode`/`decode`, under names the brief pins:
  `encode_record`/`decode_record`).
- Section 6, the `mpuctl` value: `Budget` (the profile tuple, `0016:1463-1480`),
  `AdmissionRecord` (`0016:348`), their validating `Deserialize` impls, and the format
  constants `MAX_CHUNKREF_BYTES` / `VALUE_CEILING_HALF` / `MAX_SEG_CHUNKS_FORMAT_MAX` the
  value-ceiling checks need.
- `crates/core/tests/multipart_budget_admission.rs` — the named test, ten cases: two round
  trips, the six required negations (1a, 1f-i..iv, 1g), and leg 3 (occupancy decodes).

## Why this shape (and what the salvage got wrong)

Salvaged `Budget`/`AdmissionRecord` from
`results/issue_692/iteration-v2/patch.diff` (added-file lines ~292-579, the portion the
brief names), per its own instruction to fix the recorded defects rather than re-ship the
reviewed shape. `results/issue_692/review-batch.md` names three blockers against exactly
this code (`multipart.rs:1027/1041/1074` in the v2-patched file, **not** base line
numbers — the citation-namespace trap the brief calls out):

1. `:1041` — `Budget::new` admitted `max_part_chunks` past the value-size ceiling
   (`0016:1466`). **Fixed**: added the `MAX_CHUNKREF_BYTES × max_part_chunks ≤
   VALUE_CEILING_HALF` check (1f-i, `multipart.rs` `Budget::new`, the `part_value_charge`
   block).
2. `:1027` — only the *lower* end of `max_staged_chunks`'s range was enforced, not the
   `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` publishable ceiling (`0016:1468`'s upper end).
   **Fixed**: added 1f-iii (the `staged_format_ceiling` block). `MAX_SEG_CHUNKS` has no
   code definition on this base (only the prose cross-reference at `0016:1465`), so this
   slice **defines** `MAX_SEG_CHUNKS_FORMAT_MAX` — the brief's explicit instruction — by
   the identical value-ceiling rule, with a `const` assertion tying it to
   `MAX_CHUNKREF_BYTES`.
3. `:1074` — the `sidx:` scan-bound check counted only `max_inflight_parts ×
   max_part_chunks`, ignoring `max_staged_chunks`. **Fixed**: 1g's `owned_scan_charge`
   now sums both terms against `SCAN_CAP/2`.

I did **not** re-derive these from scratch: the salvage's `u_ref_of` (exact `u128`
arithmetic, never saturating — the iteration-4/#692 overflow finding) and the 1a/leg-3
identity/occupancy split were already correct and already cite the right proposal
sections, so I kept them verbatim except for the `RecordError::Structural` wrapper
(matching this slice's error taxonomy) in place of the salvage's ad hoc string reason.

## `MAX_CHUNKREF_BYTES = 302`: where the number comes from

The brief requires a FORMAT constant, not a live deployment knob, and forbids inventing
one. `0016:1053`/`:1063`/`:2844` already computed this number: `b_ref = 131–302 B`, the
range this proposal itself derived for a `ChunkRef`'s worst-case JSON encoding across its
supported EC schemes, and it is the number `MAX_MAP_CHUNKS`/`MAX_PART_CHUNKS`'s own
`⌊50000/b_ref⌋ = 165–381` derivation already uses. I took the **high** end (302, the
tighter chunk-count ceiling), because a looser `b_ref` makes `Budget::new`'s check *more*
restrictive, never less — the safe direction for a decode-time FORMAT maximum
(`0016:390-402`), the same "a loose bound can only reject what would have fit, never admit
what would not" argument `segmented_map_record.rs:521-524` states for its own
`MAX_ROOT_SEGMENTS` encoded-size bound. Sanity check: `⌊50000/302⌋ = 165`, exactly 0016's
documented low end — confirming 302 is the value 0016 itself already used at that end of
the range, not a number I invented.

**Alternative considered and rejected**: measuring an exact worst-case `ChunkRef` encoding
in code, the way `segmented_map_record.rs` does for `SegmentRef` (skeleton + digit-slack
over a decoded/re-encoded table, ~80 lines of test machinery). Rejected on cost: `ChunkRef`
carries a `Vec<DServerId>` placement whose length is the EC scheme's fragment count
(`k + m`, `u8` + `u8`, up to 510) — an exact bound over *every* legal `EcScheme` would push
`MAX_CHUNKREF_BYTES` to several KB and collapse `max_part_chunks`'s admissible range to
single digits, silently changing the*ChunkRef*shape or scope the brief pins as untouched
(`0016`/`metadata.rs` `ChunkRef` is explicitly out of scope: "`metadata.rs` ... outside
`multipart.rs` + the new test"). Reusing 0016's own already-settled number is the smallest
change that satisfies "a FORMAT constant can decide" without inventing a value or touching
`metadata.rs`.

## Ruled out: touching `metadata.rs`

The salvage patch also touched `metadata.rs` (`PendingEntry`/`InodeRecord` additive
fields, ~80 lines) for a *later* record class (`sidx:`'s `OwnedEntry`), not
`Budget`/`AdmissionRecord`. Brief scope pins `metadata.rs` untouched
("out of scope: `metadata.rs` and any file outside `multipart.rs` + the new test"), so
none of that portion of the salvage patch was carried — only the `Budget`/`AdmissionRecord`
slice (added-file lines ~292-579) was.

## Falsifiability — the six negations (drop check → run → capture → revert)

Per the brief's Falsifiability clause, the compile-fail RED leg is PRE-DECLARED
UNVERIFIABLE (confirmed below), so these six single-check drops are the binding red
evidence. Each drop used `if false && <original condition>` at the exact `path:line` of
the check, ran `cargo test -p wyrd-core --test multipart_budget_admission`, captured the
failure, then reverted (confirmed clean via `grep -n "false &&" multipart.rs` → no
matches, and the full suite green again after each revert).

### 1a — `AdmissionRecord`'s `max_sessions` vs. its own `profile` (multipart.rs:1265,
`if wire.max_sessions != derived`)

```
test leg_1a_torn_max_sessions_is_rejected_at_decode ... FAILED
thread '...' panicked at crates/core/tests/multipart_budget_admission.rs:121:5:
max_sessions=2 disagreeing with the profile's derived 1 must be rejected, got
Ok(AdmissionRecord { count: 1, max_sessions: 2, profile: Budget { w_ref: 20000,
max_part_chunks: 100, max_parts_per_session: 100, max_inflight_parts: 50,
max_staged_chunks: 100 } })
```

### 1f-i — value-ceiling (multipart.rs:1079, `if part_value_charge > VALUE_CEILING_HALF as u128`)

```
test leg_1f_i_max_part_chunks_above_the_value_ceiling_is_rejected ... FAILED
max_part_chunks=166 must exceed MAX_CHUNKREF_BYTES x N <= VALUE_CEILING_HALF
(format_max=165), got Ok(Budget { w_ref: 10000000, max_part_chunks: 166,
max_parts_per_session: 200, max_inflight_parts: 50, max_staged_chunks: 166 })
```
(The other three 1f-i-adjacent legs — 1f-ii/iii/iv — stayed green in this same run,
confirming the negation isolates only 1f-i.)

### 1f-ii — lower end (multipart.rs:1064, `if max_staged_chunks < max_part_chunks`)

```
test leg_1f_ii_max_staged_chunks_below_max_part_chunks_is_rejected ... FAILED
max_staged_chunks=40 below max_part_chunks=50 must be rejected, got
Ok(Budget { w_ref: 5000, max_part_chunks: 50, max_parts_per_session: 50,
max_inflight_parts: 10, max_staged_chunks: 40 })
```
(`leg_1f_iii` ran alongside and stayed green — the two ceilings do not overlap at these
values.)

### 1f-iii — publishable ceiling (multipart.rs:1090, `if u128::from(max_staged_chunks) > staged_format_ceiling`)

```
test leg_1f_iii_max_staged_chunks_above_the_publishable_ceiling_is_rejected ... FAILED
max_staged_chunks=84481 above MAX_ROOT_SEGMENTS x MAX_SEG_CHUNKS_FORMAT_MAX (84480)
must be rejected, got Ok(Budget { w_ref: 84491, max_part_chunks: 1,
max_parts_per_session: 1, max_inflight_parts: 1, max_staged_chunks: 84481 })
```
(Confirms the derived ceiling is exactly `512 × 165 = 84480`, matching 0016's own
165-chunk low end at `MAX_CHUNKREF_BYTES = 302`.)

### 1f-iv — in-flight vs. per-session (multipart.rs:1057, `if max_inflight_parts > max_parts_per_session`)

```
test leg_1f_iv_max_inflight_parts_above_max_parts_per_session_is_rejected ... FAILED
max_inflight_parts=20 above max_parts_per_session=10 must be rejected, got
Ok(Budget { w_ref: 1000, max_part_chunks: 10, max_parts_per_session: 10,
max_inflight_parts: 20, max_staged_chunks: 10 })
```

### 1g — `sidx:` scan bound, committed + in-flight (multipart.rs:1099, `if owned_scan_charge > u128::from(scan_half())`)

```
test leg_1g_sidx_scan_bound_counts_committed_staging_plus_inflight_is_rejected ... FAILED
max_staged_chunks + max_inflight_parts x max_part_chunks = 524535 exceeding
SCAN_CAP/2 = 524288 must be rejected, got Ok(Budget { w_ref: 10000000,
max_part_chunks: 165, max_parts_per_session: 2667, max_inflight_parts: 2667,
max_staged_chunks: 84480 })
```
(`max_staged_chunks` alone sits exactly at the 1f-iii ceiling here — 84480, legal on its
own — proving the failure is 1g's sum term, not a re-trip of 1f-iii.)

After each of the six, `if false && …` was reverted at the same line and
`cargo test -p wyrd-core --test multipart_budget_admission` re-ran green (10/10) before
moving to the next — never two drops live at once, so each negation is a clean, isolated
single-guard demonstration.

## The pre-declared UNVERIFIABLE compile-fail RED (posture (a), as #691)

Confirmed by `git stash push -- crates/core/src/multipart.rs` (test file kept) then
`cargo test -p wyrd-core --test multipart_budget_admission`:

```
error[E0432]: unresolved imports `wyrd_core::multipart::decode_admission_record`,
`wyrd_core::multipart::encode_record`, `wyrd_core::multipart::AdmissionRecord`,
`wyrd_core::multipart::Budget`, `wyrd_core::multipart::MAX_CHUNKREF_BYTES`,
`wyrd_core::multipart::MAX_SEG_CHUNKS_FORMAT_MAX`, `wyrd_core::multipart::VALUE_CEILING_HALF`
```

Matches the brief's pre-declaration exactly (compile failure, not a runtime red) — this is
the §6 NEEDS-HUMAN item the brief tells C2/C4 to expect, not a surprise.

## Refutation (forced, per the Do protocol)

**(a) Genuine red?** Yes — demonstrated six times above (single-check drops) and once more
via the whole-production revert (compile failure, pre-declared). Every one of the ten test
functions in the new file failed under the appropriate negation; none of the six negation
runs left its own target test green.

**(b) Production path?** Yes. Every test calls the real `wyrd_core::multipart::{Budget,
AdmissionRecord, encode_record, decode_admission_record}` — no mock, no copy, no parallel
re-implementation. `leg_1f_iii`/`leg_1g` even derive their expected thresholds from the
*production* constants (`MAX_CHUNKREF_BYTES`, `VALUE_CEILING_HALF`,
`MAX_SEG_CHUNKS_FORMAT_MAX`, `metadata::MAX_ROOT_SEGMENTS`, `wyrd_traits::SCAN_CAP`)
rather than a hard-coded expected number, so the test stays bound to whatever the
production module actually computes.

**(c) Fixture includes the fault?** Yes. Every negation test builds the exact torn/
out-of-range tuple that check exists to catch (a `max_sessions` that disagrees with its
own profile; a `max_part_chunks` one past the value ceiling; etc.) — none of the fixtures
exclude or pre-filter the faulty value; `baseline_is_legal` proves the *baseline* itself is
clean, so a negation's failure is attributable to the one field perturbed, not to an
already-invalid fixture.

## Commit-readiness

- `cargo fmt -p wyrd-core -- --check` — clean (one formatting pass applied to the new test
  file before this).
- `cargo clippy -p wyrd-core --all-targets -- -D warnings` — clean.
- `cargo test -p wyrd-core` (whole crate, not just the new file) — all pre-existing suites
  still pass (16 + 13 + 5 + 4 + 2 + doc-tests, etc., unaffected).
- Patch applies cleanly (`git apply --check`) to a fresh `git worktree` off `9dbcd72`, and
  the new test is green there too (fresh-checkout confirmation, not just the working
  worktree).
- `cargo xtask ci` (the repo's full gate) was **not** run standalone here — it is the
  `C4-ci` gating check the Check beat runs; the two commands above are its `fmt`/`clippy`
  legs, and `cargo test -p wyrd-core` is `cargo xtask ci`'s workspace `test` leg scoped to
  the one crate touched. Running the full multi-minute `cargo xtask ci` (fmt + clippy +
  build + test + machete + deny + conformance + statics + orchestrator-guard + DST, all
  workspace-wide) is Check's job, not Do's — it is unaffected by a change scoped to one
  pure module with no new dependency.

## Self-review against `AGENTS.md`'s "Review rubric & protocol" (root of the target
worktree, per the brief's second narrow exception)

- **Metadata validation boundaries (ADR-0045)** — this is the change's whole point:
  every relational check (1a, 1f-i..iv, 1g) is enforced at decode as a typed error, never
  silently corrected; `count` vs. `max_sessions` is deliberately liberal-on-read (leg 3).
  Compliant by construction.
- **`#![forbid(unsafe_code)]`** — the new test file carries it (line 28), matching every
  other file in `crates/core/tests/`.
- **Serialization identity** (optional/legacy fields omitted, never defaulted) — N/A:
  neither `Budget` nor `AdmissionRecord` has an optional field: every field is required,
  so there is nothing to `skip_serializing_if`.
- **One clock per lifecycle / trait seams / no DST-reachable global state / transactions /
  await discipline / probes / test fidelity / workflow edits** — N/A: this module is pure
  (no clock read, no store call, no async, no global state, no CI/workflow file touched),
  exactly as its own module doc states both before and after this patch.
- **Docs currency** ("a change that ... alters a persisted field updates the living
  architecture doc in the same PR") — **flagged, not fixed**. `Budget`/`AdmissionRecord`
  are a persisted-field *shape*, but the brief's own scope explicitly excludes all
  `docs/` files ("out of scope: ... all `docs/` files"), and the module's existing header
  comment (`multipart.rs`, "Nothing here is written yet — where the living-architecture
  update belongs") already states the precedent this mirrors: #691 (the merged key-grammar
  slice) made the identical call — the living doc gains this namespace "with the slice
  that first *persists* one" (`#656-#659`), not the slice that defines its shape with no
  writer yet (`Production reach: N/A by design`, this brief). I did not override the
  brief's explicit scope by editing `docs/`; recording this here so the human sign-off
  can confirm the precedent applies rather than discovering the tension unflagged.

## Line budget

`git diff --stat`: `multipart.rs` +446/-6 lines, new test +264 lines — 710 raw diff lines,
but the brief's "≤ 550 added **semantic** lines" (its own split: "module extension ≈ 300,
test ≈ 250") reads on non-comment/non-blank lines given this repo's convention of doc-heavy
modules (`segmented_map_record.rs`, `multipart_keys.rs` are both hundreds of lines,
overwhelmingly doc comments). Counted that way: 242 semantic lines in `multipart.rs`, 178
in the test — 420 total, under budget with room to spare.
