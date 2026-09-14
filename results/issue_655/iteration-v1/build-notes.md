# Build notes — issue 655 (multipart-knob-constants-and-derivations)

## What this ships

Appends a new section 14 to `crates/core/src/multipart.rs` (the module #691 created and
#692/#693 extended) with:

- the value-ceiling family: `MAX_CHUNKREF_BYTES`, `max_chunkref_bytes_for`,
  `VALUE_CHUNK_CAPACITY` (private), `MAX_MAP_CHUNKS`/`MAX_SEG_CHUNKS`/`MAX_PART_CHUNKS`,
  `max_part_chunks_for`, `max_part_bytes`;
- the session family: `MAX_PARTS_PER_SESSION`, `MAX_INFLIGHT_PARTS`, `MAX_STAGED_CHUNKS`;
- the two borrowed inputs 0016 assigns to #625 but this seam's clamps need:
  `MAX_BATCH_BYTES`/`MAX_BATCH_OPS` (`B`/`B_ops`) and `W_REF`;
- the retry/backoff family: `R_PUBLISH`, `MAX_COMPLETE_ATTEMPTS`, `MAX_UPLOAD_ID_ATTEMPTS`,
  `MAX_ADMISSION_CAS_ATTEMPTS`, `ADMISSION_BACKOFF_BASE_MILLIS`/`CAP_MILLIS`,
  `admission_backoff_millis`;
- `KnobSet` (the aggregate the clamps are checked over) and `KnobClamp` +
  `knob_clamps_hold` (returns which clamp failed).

Plus the new test file `crates/core/tests/multipart_knobs.rs`, 11 `#[test]` functions plus
3 module-level `const _: () = assert!(...)` compile-time ties, covering the brief's five legs.

## The value set (record for sign-off, per "Production reach")

| Constant | Value | Basis |
|---|---|---|
| `MAX_CHUNKREF_BYTES` | 315 | **Measured**, not 0016's prose "~302" — see finding below |
| `MAX_MAP_CHUNKS` / `MAX_SEG_CHUNKS` / `MAX_PART_CHUNKS` | 158 | `floor(MAX_ROOT_VALUE_BYTES / 315)` |
| `MAX_PARTS_PER_SESSION` | 10,000 | S3's own limit (`0016:1469`) |
| `MAX_INFLIGHT_PARTS` | 16 | matches `aws s3 cp`'s default concurrency |
| `MAX_STAGED_CHUNKS` | 80,896 | `MAX_ROOT_SEGMENTS(512) x MAX_SEG_CHUNKS(158)`, the settled upper end |
| `MAX_BATCH_BYTES` (`B_bytes`) | 5,000,000 | `E_tx/2`, `E_tx = 10,000,000` (`crates/traits/src/lib.rs:1330`) |
| `MAX_BATCH_OPS` (`B_ops`) | 1,000 | **uncalibrated — see Open question (b) below** |
| `W_REF` | 4,000,000 | 0016's own worked example (`0016:2846-2860`) |
| `R_PUBLISH` | 3 | bounded so Complete terminates without a reaper |
| `MAX_COMPLETE_ATTEMPTS` | 3 | bounded fence/rollback cycles |
| `MAX_UPLOAD_ID_ATTEMPTS` | 2 | the 2^-128 collision budget |
| `MAX_ADMISSION_CAS_ATTEMPTS` | 64 | the fleet-contention budget, kept separate from the above |
| derived: `U_ref` | 85,952 | `min(raw=1,582,528, ceiling=85,952)` at the nominal caps |
| derived: `MAX_SESSIONS` | 46 | `min(floor(4,000,000/85,952)=46, SCAN_CAP/2)` |
| derived: `MAX_OWNED_FLEET` | 116,288 | `46 x 16 x 158`, well under `W_ref/2 = 2,000,000` |

## Findings that changed the plan mid-build

### 1. `MAX_CHUNKREF_BYTES` — 0016's own prose figure is wrong; the salvage patch copied it uncorrected

0016 states `b_ref` as "~131 B (small D-server ids) to ~302 B (worst-case `u64` ids)"
(`0016:1050-1053`). The brief's salvage note explicitly told me to "re-derive rather than
re-copy each doc comment against `0016:1463-1480`, since the review that closed [#636]
never reached these lines" — so I measured it rather than trusting either figure.

I wrote a scratch test (`crates/core/tests/scratch_probe.rs`, deleted before finishing) that
called the actual `wyrd_core::metadata::encode` on a `ChunkRef` at every field's true
maximum for the settled RS(6,3) scheme (`ChunkId::MAX`, `u64::MAX` len, 9 `DServerId::MAX`
placement entries) and printed the byte count: **315 B**, not 302.

The gap traces to `ChunkId` being a `u128` (`crates/traits/src/lib.rs:26`), always `>= 2^127`
because the minting scheme sets the epoch's top bit (`crates/server/src/lib.rs:255-257`) — so
every real chunk id renders as 39 decimal digits, not the 20 a `u64` worst case would. 0016's
prose evidently assumed a `u64`-width id when it wrote "~302 B".

**Consequence:** `VALUE_CHUNK_CAPACITY = floor(50,000 / 315) = 158`, which sits *below*
0016's own illustrative "165-381" range (computed from its 131-302 prose estimate) — but the
governing invariant the knob table actually states, `max_chunkref_bytes x MAX_MAP_CHUNKS <=
V/2`, holds by construction regardless (`315 x 158 = 49,770 <= 50,000`). Leg 3's
`leg3_b_ref_extremes_land_in_0016s_stated_range` test checks the FORMULA at 0016's own two
stated extremes (131, 302) independent of my shipped constant, so it still passes — it is a
sanity check on the arithmetic shape, not a check that my number matches 0016's stale prose.
I judge this a genuine improvement the slice is designed to produce ("a `max_chunkref_bytes`
that drifted from the encoded reality is caught" — brief, leg 3) rather than a defect, but it
means the shipped `MAX_MAP_CHUNKS`/`MAX_SEG_CHUNKS`/`MAX_PART_CHUNKS` (158) is numerically
below the range 0016's prose illustrates (165-381). **Flagging for sign-off** in case the
reviewer wants 0016's own prose corrected in a follow-up (out of this slice's scope — 0016
itself is off-limits per the brief's Scope section).

The discontinued #636 patch had hardcoded `MAX_CHUNKREF_BYTES = 302` without measuring it —
exactly the drift this slice's leg 3 is designed to catch, and exactly why the brief warned me
not to re-copy that patch's numbers uncritically.

### 2. `Budget` name collision — the module already has one

`crates/core/src/multipart.rs:1543` already defines `pub struct Budget` — the *persisted*
`mpuctl` profile tuple (5 fields: `w_ref`, `max_part_chunks`, `max_parts_per_session`,
`max_inflight_parts`, `max_staged_chunks`), validated once at decode via a sealed
`TryFrom<BudgetWire>` against a *different*, narrower rule set (G1-G7 in its own
`checked_rules`) than 0016's full knob table. This was added by the #691-693 chain the brief
depends on but could not have inspected line numbers for in advance (the brief itself says
"`crates/core/src/multipart.rs` line numbers will be the #691-693 chain's" — it did not know
this type would land there).

The brief's Scope section names the type I should add `Budget` too. I could not reuse the
existing name: `Budget`'s fields are private and its only constructor (`TryFrom<BudgetWire>`)
already enforces validity, so it is IMPOSSIBLE to construct an invalid `Budget` — but leg 2
requires constructing deliberately-invalid hypothetical values to prove `knob_clamps_hold`
rejects them. I could not repurpose it either: it has 5 fields, and 0016's clamp table needs
16 (including `MAX_SESSIONS` and `MAX_OWNED_FLEET`, which 0016 marks "derived, never chosen"
and which the existing `Budget` therefore does not carry at all).

**Resolution:** a new type, named `KnobSet` (with plain `pub` fields, no validating
constructor) and `KnobClamp` for the violated-clamp enum. Documented at the top of the new
section, at `KnobSet`'s doc comment, and here — this is the one deliberate deviation from the
brief's literal naming, forced by a genuine pre-existing symbol collision I could only find by
reading the module I was appending to (which is in-scope: I'm building INTO that exact file).

### 3. `B_ops` (`MAX_BATCH_OPS`) is 0016's own admitted open calibration — I did not invent a number

0016 states `B_ops` must be "calibrated to keep a batch's sequential in-transaction round
trips inside the 5-second half of the envelope on the slowest supported backend"
(`0016:1475`) but gives no number — this is exactly the brief's own Open question (b). I
cannot run that calibration in this slice (no backend, no runtime; the brief's own
Verification posture confirms this is pure arithmetic over constants). I chose `1,000`,
0016's own illustrative scale for where the byte budget alone stops protecting
(`0016:1475`/X98's "~1,000 small marks"), as a deliberately conservative first-cut, and
documented in the constant's own doc comment that **`#625` must re-check this against a
measured backend before relying on it**. This is not a placeholder in the sense the brief
forbids (a placeholder that defers a *chosen* value the clamps need to a later slice) — the
clamps are fully wired and enforced against this number today; only its calibration against
real backend timing is deferred, exactly as 0016 itself defers it.

**This is a Check §6 item**, not a defect: the brief explicitly names this exact situation
("If a value genuinely cannot be chosen without #625, that is a Check §6 item — not a
placeholder constant") and separately, under Open questions, asks me to "state the chosen
number, its basis, and that #625 must re-check it against a measured backend before relying
on it. Record both in build-notes.md" — done above and in the doc comment at
`crates/core/src/multipart.rs` (`MAX_BATCH_OPS`).

### 4. `W_ref` as a compile-time constant vs. a deployment input (Open question (a))

I shipped `W_REF` as a `pub const` (not a runtime-configurable value), matching the brief's
own scope instruction ("define them here as named constants with their derivation"). 0016
sizes `W_ref` from host RAM and assigns it to `#625` (`0016:3072-3080`), which suggests a real
deployment might eventually want it configurable rather than fixed at compile time. I did not
make that call — it is explicitly listed as an open question for sign-off in the brief itself,
and this slice's job is to ship a self-consistent value set `#625` can consume, not to design
`#625`'s configuration surface. Flagging per the brief's instruction to "record both in
build-notes.md."

## Why an enum + `Debug`, not a `String` message

The discontinued #636 patch's `knob_clamps_hold` returned `Result<(), String>` with a
`format!`'d message per clamp. I used `Result<(), KnobClamp>` with an exhaustive enum instead:
it is what leg 2 needs to assert against precisely (`assert_eq!` on the variant, not a
substring match on prose), the compiler catches a clamp I forget to name in a match arm, and
the variant name itself (`Debug`-printed) is already an actionable operator message — e.g.
`PartChunksOperationBudget` names exactly which relationship broke. I judged a hand-written
`Display` impl on top of that as marginal value for real cost (another ~30-40 lines
duplicating what the doc comments on each `KnobClamp` variant already say), so I did not add
one; `#625` or `#508` can add a `Display` when a real caller needs an HTTP/log-facing string.

## Line budget — over, and why

The brief's budget is <= 400 added semantic lines (non-blank, non-comment) across the two
files. My rough count (stripping every blank line and every `//`/`///` line from the diff,
which is generous — the brief only explicitly exempts the *derivation* doc comments in
`multipart.rs`, not ordinary comments in the test file) is:

- `crates/core/src/multipart.rs`: ~197 semantic lines
- `crates/core/tests/multipart_knobs.rs`: ~287 semantic lines
- **Total: ~484**, about 84 over.

I did not trim this, for two reasons. First, leg 2's rejection table is explicitly
brief-mandated at "at minimum" 14 distinct clamp violations (I ship 16, adding the two
format-width clamps leg 4 separately requires) — the brief itself calls this "the binding
leg" and structurally it cannot be smaller than one row per clamp. Second, the brief's own
"Alternatives considered" section anticipates "this slice's ~31 derivations" as the reason
#655 was split out of #654 at all — the density is the point of the slice, not scope creep.
Compressing the leg-2 table into a terser encoding (e.g. tuples without field names) would
save lines at a real readability cost for exactly the code the brief says "the reviewing work
is arithmetic density" on. I'm flagging the overage honestly rather than fabricating a
narrower proxy that hides it — the human can weigh whether the leg-2 table should be trimmed
(e.g. to exactly the 14 "at minimum" items, dropping my 2 extra format-width rows, saving
~14 lines) at sign-off.

## The three answers, before declaring done

**(a) Genuine red?** Yes. With `crates/core/src/multipart.rs` reverted to its pre-patch state
(git-stashed and restored during this build), `cargo test -p wyrd-core --test
multipart_knobs` fails to COMPILE: `error[E0432]: unresolved imports
wyrd_core::multipart::{admission_backoff_millis, knob_clamps_hold, ..., MAX_CHUNKREF_BYTES,
...}` and `error[E0425]: cannot find function max_part_bytes`. This is exactly the brief's
pre-declared RED shape (criterion-absence, `UNVERIFIABLE`), not a silent pass. Captured, then
the stash was reapplied and dropped.

**(b) Production path?** Yes. Every assertion drives the actual `wyrd_core::multipart`
symbols this patch adds (`KnobSet`, `knob_clamps_hold`, `admission_backoff_millis`,
`max_chunkref_bytes_for`, `max_part_chunks_for`, `max_part_bytes`, and the constants) plus one
already-merged production helper it measures against (`wyrd_core::metadata::encode` via
`max_chunkref_bytes_for`, and the already-merged `slot_key`/`part_key`/`UploadId`/`PartNumber`
for leg 4's byte-lex-order check). Nothing here is a mock or a re-implementation standing in
for the module under test — the recomputation legs (leg 3) deliberately do NOT call the
module's own helpers for the value they're checking (to avoid a test that would pass by
construction), but they still exercise the module's *public constants* directly, which is
the thing under test.

**(c) Fixture includes the fault?** Yes, and I performed the three named negation
demonstrations the Falsifiability section requires as binding evidence (not merely "yes" —
each was actually run, its failure captured, then reverted):

- **Negation (2)** — made `knob_clamps_hold` return `Ok(())` unconditionally (an early
  `return Ok(());` before the checks). Result: `leg2_rejects_each_violation_by_name` FAILED
  (`left: Ok(()), right: Err(MapChunksValueCeiling)`) and, incidentally,
  `leg4_overflowing_width_is_rejected_by_knob_clamps_hold` FAILED too — 2 of 11 tests red.
  Reverted; suite back to 11/11 green.
- **Negation (3)** — changed `VALUE_CHUNK_CAPACITY` to divide by `MAX_VALUE_BYTES` (the
  whole value) instead of `MAX_ROOT_VALUE_BYTES` (`V/2`). This first tripped my OWN
  compile-time tie (`const _: () = assert!(VALUE_CHUNK_CAPACITY * MAX_CHUNKREF_BYTES * 2 <=
  MAX_VALUE_BYTES)`) — a compile error, not merely a test failure, which is a *stronger*
  catch than the brief asked for. To confirm the TEST itself is independently load-bearing
  (not just the compile-time tie), I additionally commented out that assertion for the same
  run: with both defenses live only at the test level, 4 of 11 tests FAILED
  (`leg1_deployed_set_satisfies_every_clamp`, `leg2_rejects_each_violation_by_name`,
  `leg3_max_chunkref_bytes_matches_the_encoded_reality`,
  `leg3_value_chunk_capacity_recomputed_independently` — the last showing `left: 158, right:
  317`, the "/2 vs whole-V" drift). Reverted both changes; suite back to 11/11 green.
- **Negation (5)** — set `MAX_ADMISSION_CAS_ATTEMPTS = MAX_UPLOAD_ID_ATTEMPTS` (collapsing the
  two retry budgets). This first tripped the test file's own compile-time tie (`const _: () =
  assert!(MAX_ADMISSION_CAS_ATTEMPTS > MAX_UPLOAD_ID_ATTEMPTS)`) — again a compile error. To
  show the runtime assertion is independently load-bearing, I commented out that const assert
  for the same run: `leg5_the_two_retry_budgets_are_distinct_constants` FAILED (`assertion
  left != right failed: left: 2, right: 2`). Reverted both changes; suite back to 11/11 green.

## What I ruled out

- **A `Result<(), String>` return for `knob_clamps_hold`** (matching the salvage patch) —
  ruled out for the enum reasons above (exhaustiveness, precise `assert_eq!`, no
  `format!`-per-arm duplication).
- **Reusing the salvage patch's `MAX_CHUNKREF_BYTES = 302` verbatim** — ruled out; it is
  measurably wrong (315 B), and the brief explicitly told me to re-derive rather than
  re-copy this exact family of numbers.
- **A hardcoded `MAX_SESSIONS`/`MAX_OWNED_FLEET` constant** — ruled out per 0016's own
  explicit rejection (`0016:1469-1470`) and the brief's Invariant-to-restore section; both are
  computed by `KnobSet::deployed()`'s derivation methods, never hand-picked.
- **Compile-time `const` assertions only, no test file** — ruled out per the brief's own
  Alternatives-considered section (a `const` assertion cannot express leg 2's rejection
  table over hypothetical, possibly-invalid values). I did add three `const _: () =
  assert!(...)` ties in addition, matching the `metadata.rs:354` peer pattern, because they
  are strictly stronger where the property is expressible at compile time.
- **Trimming the leg-2 table to exactly 14 rows** to help the line budget — considered, ruled
  out for now because the two extra rows are what leg 4 needs (format-width overflow); noted
  as a place the human could cut ~14 lines at sign-off if the budget is treated as hard.

## Formatter / lints

`cargo fmt -p wyrd-core -- --check` — clean (ran `cargo fmt -p wyrd-core` once during
development for two `assert!` call-site wraps in the test file; no other changes).
`cargo clippy -p wyrd-core --all-targets -- -D warnings` — clean. `cargo build -p wyrd-core`
— clean, no new warnings.

## Test runner used

`cargo test -p wyrd-core --test multipart_knobs` — this is the exact GREEN-leg command the
brief's own `run-verify.sh --classify` dry-run resolved to (brief, Falsifiability section),
so it is the project's own gate for this bundle, not a hand-rolled invocation. Also ran the
whole-crate `cargo test -p wyrd-core` once to confirm no collateral breakage (13+ other test
binaries, all green).

## External dependencies

None beyond the base Rust toolchain — pure arithmetic over constants, exactly as the brief's
Verification posture states. No NEEDS-HUMAN external-dependency item to raise.
