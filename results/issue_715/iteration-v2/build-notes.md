# Build notes — issue #715 (multipart-budget-admission), iteration 2

Target branch `getwyrd/wyrd@main`, base `9dbcd72`. All line citations below are **post-patch**
line numbers in `$PDCA_WORKTREE` unless marked "(base)".

## What shipped

`crates/core/src/multipart.rs` (+554 raw / **262 semantic** lines) gains:

- `RecordError::Structural { record, reason }` (`multipart.rs:130-137`), its `Display` arm
  (`:164`) and the `structural()` constructor (`:178`) — the relational arm ADR-0045 needs
  beside the existing structural-parse arms.
- **Section 5**, the record envelope: `encode_record` (`:892`) / `decode_record` (`:904`),
  mirroring `metadata::encode`/`decode` (base `metadata.rs:1536-1543`) but mapping every
  serde failure to the typed `RecordError::Structural`. Later record children add an arm the
  way `decode_admission_record` (`:1400`) does.
- **Section 6**, the `mpuctl` value: the format constants (`:923-1013`), `u_ref_of` (`:1034`),
  `Budget` (`:1060`) with its validating `new` (`:1114`), `staged_upper_bound` (`:1220`),
  accessors, `u_ref` (`:1257`), `max_sessions` (`:1279`), the `deny_unknown_fields` wire
  structs (`:1287`, `:1371`) and `AdmissionRecord` (`:1335`).
- `crates/core/tests/multipart_budget_admission.rs` — the named test, 20 cases (+388 semantic).

Salvaged from `results/issue_692/iteration-v2/patch.diff` (added-file lines ~292-579): the
`u_ref_of` exact-`u128` derivation, the accessor set, the identity/occupancy split and their
prose. Everything the two reviews found defective was rebuilt, not re-shipped — below.

## The two structural changes this iteration makes (the carry-forward's `[impl]` findings)

### C5 — "Rebuild with an encoding-derived ceiling"

Iteration 1 pinned `MAX_CHUNKREF_BYTES = 302` from 0016's prose (`0016:1053`, `:2844`) and used
it as an **upper** bound on a `ChunkRef`'s encoding. The reviewer falsified the number: a
legal-width 32 MiB RS(6,3) `ChunkRef` encodes to **303** bytes, so 165 of them are 50,161 bytes
— past the very `V/2` budget the constant was supposed to guarantee. Worse, the direction was
wrong on principle: a decode-time bound derived from the *widest* chunk-ref rejects a record a
deployment with narrower chunk-refs could legally have written, which is exactly what
`0016:390-402` forbids ("every record ever written under a legal configuration stays decodable
under every later one").

This iteration derives every ceiling from the **narrowest** encoding, measured against the real
encoder:

- `MIN_CHUNKREF_BYTES = 47` (`:951`) — `metadata::encode` of
  `{"id":0,"scheme":"None","len":0,"placement":[]}`;
- `MIN_SEGREF_BYTES = 40` (`:956`) — the same for `SegmentRef`;
- `MAX_SEG_CHUNKS_FORMAT_MAX = ⌊50_000/47⌋ = 1_063` (`:966`), `MAX_PART_CHUNKS_FORMAT_MAX`
  the same number (`0016:1466` states the rule as *identical*, `:972`),
  `MAX_ROOT_SEGMENTS_FORMAT_MAX = ⌊50_000/40⌋ = 1_250` (`:986`),
  `MAX_PUBLISHABLE_CHUNKS_FORMAT_MAX = 1_328_750` (`:993`);
- five `const _: () = assert!(...)` ties (`:1000-1013`): each maximum satisfies the rule it came
  from **and** is the largest count that does; the deployment's `metadata::MAX_ROOT_SEGMENTS`
  (512) stays inside the format maximum; the format maximum stays inside the `seg:` key space.

The constants are no longer prose: `min_chunkref_bytes_is_the_measured_encoding_minimum`
(test `:145`) encodes the witness through **production** `metadata::encode` and requires the
constant to equal it, then widens each dimension in turn (id, len, EC scheme, placement) and
requires each to cost strictly more — so the witness is the *minimum*, not one small case, and
a field added to `ChunkRef` breaks the test rather than silently invalidating both ceilings.
The same test re-measures the reviewer's own worst case (a full-width RS(6,3) 32 MiB chunk with
a 9-server placement) and asserts that the knob **that** width derives, `⌊50_000/width⌋`, sits
inside our format maximum — the property that makes the bound safe to enforce at decode.

### T2/CONVENTION — a deployment knob at decode, and a constructible-invalid record

Two review findings, one root cause. Iteration 1 enforced
`max_staged_chunks ≤ metadata::MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS_FORMAT_MAX` — the brief's
literal 1f-iii wording — but `MAX_ROOT_SEGMENTS` is a **capacity** number this deployment
chooses, and its own doc says so in as many words: "deliberately **not** at decode: rejecting a
stored record on a derived capacity constant would turn a durable object unreadable if the
constant ever moved" (base `metadata.rs:312-321`). `0016:390-402` names the very same constant
in the very same role ("lowering `MAX_ROOT_SEGMENTS` would likewise make already-published roots
unreadable"). So the brief's literal wording contradicts the brief's own binding principle; I
built to the principle and recorded the deviation here (see "Deviation from the brief", below).

`Budget`'s and `AdmissionRecord`'s fields are now **private** with derived `max_sessions`
(`:1345`), so a contradictory record is unrepresentable rather than merely refused —
parse-don't-validate, ADR-0045 (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`),
mirroring `metadata::SegmentedMap` (base `metadata.rs:836-848`). Both wire structs carry
`#[serde(deny_unknown_fields)]`, matching `SegmentRef`/`SegmentGroup` (base `metadata.rs:801`,
`:814`) — a `mpuctl` value is CAS'd whole, so a field silently dropped on decode is a field
silently **deleted** by the next `decode → mutate → encode`.

### T5 — "Rebuild tests with independent numeric oracles"

Iteration 1: 46 mutants, **25 missed** — every `u_ref_of` operator, both `max_sessions`
quotient alternatives and every accessor survived, because the tests re-derived their
expectations from the production expressions.

This iteration's oracles are hand-computed integers with the arithmetic written out beside them
(test `:346`, `:373`): `U_ref = 350` where the raw term binds, `154` where the ceiling term
binds, `105_600` on 0016's own settled pairing; `MAX_SESSIONS = 2` at `W_ref = 1_000` (the floor
pinned from both sides at 1_049 → 2 and 1_050 → 3) and `524_288` where the `SCAN_CAP/2` clamp
binds. Every guard also gets a legal twin sitting exactly **on** its bound, which is what kills
the `>` ↔ `>=` class.

Result on the final diff (`scripts/mutants-in-diff`): **57 mutants — 39 caught, 18 unviable, 0
missed.** The 18 unviable are all "replace with `Default::default()`" on types with no `Default`
(the two `Deserialize` impls, `structural`, `decode_record`, `decode_admission_record`,
`u_ref_of`, `staged_upper_bound`, `AdmissionRecord::profile`).

## Deviation from the brief's literal wording (one, deliberate, load-bearing)

**Leg 1f-iii.** The brief says `max_staged_chunks ≤ MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS_FORMAT_MAX`
with `MAX_ROOT_SEGMENTS` taken from `metadata.rs:322`. I enforce (`:1220`)

```
max_staged_chunks ≤ min( MAX_PUBLISHABLE_CHUNKS_FORMAT_MAX ,           // 1_328_750, format
                         max_parts_per_session × max_part_chunks )      // this record's own fields
```

Two reasons, both binding:

1. **The deployment knob had to go** (above): using it at decode is the ADR-0045 violation both
   `metadata.rs:312-321` and `0016:390-402` name explicitly, and it was a review finding against
   iteration 1 (`review-batch.md`, `multipart.rs:1088` CONVENTION).
2. **With the honest format constant, the leg cannot isolate — so the brief's own escape clause
   applies** ("A leg green under its own isolating negation must be rewritten"). Arithmetic: leg
   1g holds `max_staged_chunks + max_inflight_parts × max_part_chunks ≤ SCAN_CAP/2 = 524_288`,
   so `max_staged_chunks ≤ 524_287` always; the format publishable ceiling is 1_328_750. A value
   that violates only 1f-iii would need to be `> 1_328_750` **and** `≤ 524_287` — no such value
   exists, so with the format term alone the guard is unreachable dead code and its negation
   would stay green on 1g. The second term restores the invariant the leg exists for ("a session
   may stage no more than it could publish", `0016:1472`) as a relation between **the record's
   own fields**, which is precisely the ADR-0045 decode class and is independently falsifiable
   (negation captured below). Both terms ship: the format term is what ties
   `MAX_SEG_CHUNKS_FORMAT_MAX` to a stored field and it binds again the moment `SCAN_CAP` rises.

Soundness of the added term: a session holds at most `max_parts_per_session` committed parts of
at most `max_part_chunks` chunks each, so a larger staged ceiling names a state the profile
forbids reaching. 0016's own settled pairing has 1_650_000 of raw part space against an 84_480
publishable ceiling (`0016:1472`), i.e. a factor of ~20 of headroom — the term can only ever
catch a self-contradictory tuple, never a real one.

**Not** deviated from: `B_ops` is not invented and not enforced (`Budget::new`'s doc, `:1105-1110`
records why — it is #625's backend-calibrated knob, `0016:1475`, `:1487`).

## Cost of the alternatives I rejected

| Alternative | Concrete cost | Verdict |
|---|---|---|
| Keep `MAX_CHUNKREF_BYTES = 302` and just add a test for it | The test cannot pass: the reviewer's own witness encodes to 303 B, and an *exact* worst case over every legal `EcScheme` (`k + m ≤ 510` `u64` D-server ids) is ≈ 10.7 KB, which would set `MAX_PART_CHUNKS_FORMAT_MAX = ⌊50_000/10_700⌋ = 4` and refuse 0016's own settled 165 | rejected — the bound is falsified and the honest version of it is unusable |
| Bound the format max by the *widest* encoding rather than the narrowest | Same 4-chunk ceiling as above; refuses every real profile, including the one 0016 settles | rejected — inverted direction for a decode bound (`0016:390-402`) |
| Add `max_chunkref_bytes` as a sixth stored field so the rule is a product of stored values | +1 wire field on `mpuctl`, +1 range rule, and every writer in #656–#659 must source a measured width; changes the record shape the brief pins as "the profile tuple of `0016:1463-1480`" (five knobs) | rejected — a stored-format change to buy a rule the format-minimal width already gives |
| Drop the `max_staged_chunks` term from leg 1g so the format publishable ceiling binds | 3-line change (`:1172-1173`), but it re-opens the #692 batch-review blocker verbatim ("counts only in-flight chunks and ignores up to `max_staged_chunks` committed staging entries") which the brief pins as leg 1g | rejected — trades a pinned safety leg for a negation's convenience |
| Ship 1f-iii with only the format term (the brief's literal wording, minus the knob) | 1 line shorter, but provably unreachable (arithmetic above) — a dead guard whose negation is green | rejected — the brief requires the leg be falsifiable |

## The six demonstrated negations (drop one check → run → capture → revert)

Each drop was `if false && <original condition>` at the exact line, then
`cargo test -p wyrd-core --test multipart_budget_admission`, then revert (verified: the tree
holds **0** occurrences of `false &&` afterwards and the suite is green again). **Every run
failed exactly one test — 19 passed, 1 failed — which is the isolation the brief demands:** the
torn tuple violates only the bound under test, so no surviving guard can mask it.

### 1a — `max_sessions` vs. what its own profile derives (`multipart.rs:1381`)

```
test leg_1a_a_torn_max_sessions_is_refused_at_decode ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:461:5:
expected a typed Structural mpuctl error, got Ok(AdmissionRecord { count: 7, max_sessions: 10,
profile: Budget { w_ref: 1000000, max_part_chunks: 165, max_parts_per_session: 10000,
max_inflight_parts: 64, max_staged_chunks: 84480 } })
test result: FAILED. 19 passed; 1 failed
```

### 1f-i — the value ceiling on `max_part_chunks` (`multipart.rs:1135`)

```
test leg_1f_i_max_part_chunks_past_the_value_ceiling_is_refused ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:477:5:
expected a typed Structural mpuctl error, got Ok(Budget { w_ref: 4000, max_part_chunks: 1064,
max_parts_per_session: 2, max_inflight_parts: 1, max_staged_chunks: 1064 })
test result: FAILED. 19 passed; 1 failed
```

### 1f-ii — the lower end of `max_staged_chunks` (`multipart.rs:1157`)

```
test leg_1f_ii_a_staged_ceiling_below_one_maximal_part_is_refused ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:488:5:
expected a typed Structural mpuctl error, got Ok(Budget { w_ref: 5000, max_part_chunks: 50,
max_parts_per_session: 50, max_inflight_parts: 10, max_staged_chunks: 49 })
test result: FAILED. 19 passed; 1 failed
```

### 1f-iii — the upper end of `max_staged_chunks` (`multipart.rs:1165`)

```
test leg_1f_iii_a_staged_ceiling_past_the_publishable_one_is_refused ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:505:5:
expected a typed Structural mpuctl error, got Ok(Budget { w_ref: 1000, max_part_chunks: 10,
max_parts_per_session: 10, max_inflight_parts: 5, max_staged_chunks: 101 })
test result: FAILED. 19 passed; 1 failed
```

(101 against a 100-chunk own-part-space: 1f-ii is satisfied at 101 ≥ 10, 1g at 101 + 50 = 151,
1f-i at 10 ≤ 1_063 — only this guard can refuse it.)

### 1f-iv — in-flight vs. per-session (`multipart.rs:1150`)

```
test leg_1f_iv_more_parts_in_flight_than_a_session_may_hold_is_refused ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:516:5:
expected a typed Structural mpuctl error, got Ok(Budget { w_ref: 1000, max_part_chunks: 10,
max_parts_per_session: 10, max_inflight_parts: 11, max_staged_chunks: 100 })
test result: FAILED. 19 passed; 1 failed
```

### 1g — the staging scan bound, committed + in-flight (`multipart.rs:1174`)

```
test leg_1g_a_staging_scan_past_scan_cap_half_is_refused ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:537:5:
expected a typed Structural mpuctl error, got Ok(Budget { w_ref: 2000000, max_part_chunks: 1000,
max_parts_per_session: 523, max_inflight_parts: 523, max_staged_chunks: 1289 })
test result: FAILED. 19 passed; 1 failed
```

(The pair moves one field by **one** across exactly `SCAN_CAP/2`: 1_288 + 523 × 1_000 = 524_288
is accepted, 1_289 + 523 × 1_000 = 524_289 is refused — so the red is the scan bound, not a
neighbour. The same test also covers the batch-review blocker's own shape, a profile whose
*committed* term alone pushes the scan past the cap.)

### Leg 3, negated the other way — occupancy made a decode error

Inserted `if wire.count > wire.max_sessions { return Err(...) }` into the `AdmissionRecord`
decoder, ran, reverted:

```
test leg_3_occupancy_above_the_limit_still_decodes ... FAILED
panicked at crates/core/tests/multipart_budget_admission.rs:562:80:
a count above max_sessions is live occupancy, not a decode error — rejecting it would make the
very record the drain must read unreadable: Structural { record: "mpuctl", reason: "count above
max_sessions" }
test result: FAILED. 19 passed; 1 failed
```

## The pre-declared UNVERIFIABLE compile-fail RED (posture (a), born-at-tier, as #691)

`git checkout HEAD -- crates/core/src/multipart.rs` (test file kept), then the named test:

```
error[E0432]: unresolved imports `wyrd_core::multipart::decode_admission_record`,
`wyrd_core::multipart::decode_record`, `wyrd_core::multipart::encode_record`,
`wyrd_core::multipart::AdmissionRecord`, `wyrd_core::multipart::Budget`,
`wyrd_core::multipart::MAX_PART_CHUNKS_FORMAT_MAX`, ... `wyrd_core::multipart::VALUE_CEILING_HALF`
error[E0599]: no variant named `Structural` found for enum `RecordError`
```

Exactly the brief's pre-declaration: a compile failure, not a runtime red, so C4-verify's RED leg
is UNVERIFIABLE (exit 77) — a known §6 sign-off item, not a surprise. The six negations above are
the binding red evidence the brief substitutes for it. Production restored byte-identical
afterwards (`diff -q` against the pristine copy) and the suite is green.

## Refutation (forced, per the Do protocol)

**(a) Genuine red?** Yes — seven times, captured above: six single-check drops (one per
independently enforced bound) plus leg 3 negated the other way, and once more by reverting the
whole production change (compile failure, pre-declared). Each single-check run failed **exactly
one** test and left the other 19 green, so each guard is individually load-bearing. The
mutation sweep is the mechanical version of the same question: 0 survivors on the diff.

**(b) Production path?** Yes. Every assertion drives `wyrd_core::multipart::{Budget,
AdmissionRecord, encode_record, decode_record, decode_admission_record}` and
`wyrd_core::metadata::encode` — no mock, no copy, no parallel re-implementation. The negations
go through **both** gates (`gates()`, test `:88`): the constructor *and* the JSON decode path, so
a rule that existed only in the constructor would fail the decode half. The encoding constants
are measured with the production encoder, not recomputed by the test.

**(c) Fixture includes the fault?** Yes. Every negation builds the exact torn tuple the guard
exists to catch (a `max_sessions` disagreeing with its own profile; `max_part_chunks` one past
the ceiling; a staging scan one chunk-ref past `SCAN_CAP/2`; …), and each is paired with its
**legal twin sitting exactly on the bound**, proving the fixture is not curated to be invalid for
some other reason. `baseline()` (0016's own settled profile) is asserted legal, so a negation's
red is attributable to the one field perturbed.

## Self-review against the target's `AGENTS.md` "Review rubric & protocol"

- **Metadata validation boundaries (ADR-0045)** — the change's whole point: identity relations
  are typed errors at decode (1a, 1f-i…iv, 1g); the contextual/occupancy relation (`count` vs.
  `max_sessions`) stays liberal on read (leg 3); no bound at decode reads a deployment knob.
- **`#![forbid(unsafe_code)]`** — the new test crate root carries it (test `:29`).
- **Serialization identity** — no optional field exists on either record, so nothing can be
  emitted as a default; decode → encode byte-identity is asserted for both records (test `:305`,
  `:329`), and `deny_unknown_fields` keeps an unknown field from being silently dropped from a
  CAS'd value.
- **Protocol input (torn / truncated / oversize)** — torn, unknown-field and missing-field
  values are typed errors; every budget is sized against the worst-case **encoded**
  representation, measured (the class the rubric names at `AGENTS.md:161-164`).
- **One clock / trait seams / DST globals / transactions / await discipline / probes / test
  fidelity / workflow edits** — N/A: the module is pure (no clock, no store, no async, no global
  state, no workflow file), as its own header states.
- **Docs currency (`AGENTS.md:154`)** — **flagged, not fixed** (the same C1 NEEDS-HUMAN as
  iteration 1). This slice defines a persisted-field *shape* with no writer (`Production reach:
  N/A by design`; the first writers are #656–#659), and the brief's scope excludes all `docs/`
  files. `multipart.rs:55-64` (base) already records the precedent #691 set: the living
  architecture doc gains these namespaces with the slice that first *persists* one. I did not
  override the brief's explicit scope; the human should confirm the precedent at sign-off.

## Line budget

`git diff --stat`: `multipart.rs` +554/-6, new test +578. Semantic (non-comment, non-blank)
added lines: **262 module + 388 test = 650**, against the brief's "≤ 550 (module ≈ 300, test ≈
250)". The module half is inside its estimate; the test half is 138 over, and every line of the
overage is carry-forward work the brief's estimate predates: the encoding-measurement leg (C5,
~45 lines), the numeric oracles for `U_ref`/`MAX_SESSIONS` (T5, ~35), the legal twin *on* each
bound (the `>`/`>=` mutants, ~30) and the typed-reason assertions replacing `is_err()` (the
three review TEST-GAP findings, ~25). I compacted twice to get here (a shared `assert_refused`,
a `gates()` pair helper, an `admission_json` builder) — cutting further would delete evidence the
previous round was rejected for lacking. Patch is 60 KB, well under the 100 KB backstop.

## Commit-readiness

- `cargo fmt --all -- --check` — clean (the project formatter was run over both files).
- `cargo clippy -p wyrd-core --all-targets -- -D warnings` — clean.
- `cargo test -p wyrd-core` — whole crate green (42 unit + 20 new + 21 multipart-keys + all
  pre-existing suites unaffected).
- `./engine/xtask.sh ci` (the project's own gate runner: fmt + clippy + build + test + deny +
  machete + conformance + statics + DST) — run to completion; see §"Full gate" below.
- `scripts/mutants-in-diff` — 57 mutants, 39 caught, 18 unviable, **0 missed**.
- No `Cargo.toml` / `Cargo.lock` change; no new dependency; no file outside the two the brief
  names.

## Full gate

`./engine/xtask.sh ci` — the project's own single-sourced gate runner (ADR-0016), run over
`$PDCA_WORKTREE` with this patch applied — **exit 0**, closing line `xtask ci: all checks
passed`. It covers what no other check here does: `cargo fmt --check`, `clippy -D warnings`,
the workspace build and full test run, `cargo-deny`, `cargo-machete`, the conformance vectors
(5 valid + 6 invalid), the ADR-0035 statics gate, the unsafe/gitlink/deploy guards and the DST
sweep. So the patch is commit-ready against the target's own hooks, not merely gate-green
here; Check re-runs the identical command as the gating `C4-ci` row.

## Scratch

Everything throwaway lived in `${PDCA_SCRATCH}/pdca-builder-715-negations/` (the pristine copy
used to revert each negation, and the gate log); removed at the end of the beat.
