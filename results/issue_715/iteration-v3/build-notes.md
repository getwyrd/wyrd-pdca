# Build notes — issue #715, `multipart-budget-admission` (iteration 3)

Target branch: `getwyrd/wyrd @ main`, base `9dbcd72`. Two files:
`crates/core/src/multipart.rs` (+592/−14) and the new
`crates/core/tests/multipart_budget_admission.rs` (+468). **285** semantic (non-blank,
non-comment) added lines in the module and **277** in the test = **562** against the brief's
≤ 550 budget (the previous attempt was 650 in the module alone). Patch 60 KB / 1,115 lines,
well under the 100 KB backstop. `git apply --check` against a pristine `9dbcd72` checkout
succeeds and reconstructs both files byte-identically (verified, then re-run green).

---

## 1. What the carry-forward demanded, and what changed

Iterations 1 and 2 both died on **one** finding, restated three ways (C3/C4/C5/T5 and 4 of the
7 batch-review blockers): the value-ceiling rule was derived from the **minimum** encoded
`ChunkRef` width (`MIN_CHUNKREF_BYTES = 47` ⇒ `MAX_PART_CHUNKS_FORMAT_MAX = 1_063`), which
"answers whether *some* spelling fits, not whether every admitted maximal record fits". That is
also a repo-rubric hard convention: *"cumulative (not per-line) section budgets sized for the
worst-case **encoded** representation of the input"* (`AGENTS.md`, Recurring defect classes).

This iteration replaces the derivation, root and branch:

| | iteration 2 (rejected) | now |
|---|---|---|
| width basis | `MIN_CHUNKREF_BYTES = 47` (narrowest spelling) | `MAX_CHUNKREF_BYTES = 319` — every field at its **widest** decimal rendering |
| separators | uncharged | `+1` per element (`max_chunks_per_value`) |
| `MAX_PART_CHUNKS_FORMAT_MAX` | 1 063 | **156** |
| `MAX_SEG_CHUNKS_FORMAT_MAX` | 1 063 | **156**, tied by `const` assertion |
| publishable ceiling | `MAX_ROOT_SEGMENTS_FORMAT_MAX × …` = 1 328 750 (a constant the brief did not ask for) | `metadata::MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS_FORMAT_MAX` = **79 872**, as leg 1f-iii names |
| test oracle | "the realistic 303-byte case implies a smaller quotient" | **measured** through the production codec: `encode(widest RS(255,255) chunk-ref).len() == chunkref_bytes(510)` **exactly**, and the widest chunk list at the format max is encoded by `metadata::encode` and compared to `V/2`, both directions |
| surviving mutants | 25 / 46 | **0 / 88** (64 caught, 24 unviable) |

`319` is derived, not guessed, in `chunkref_bytes(fragments)`: `{"id":` + 39 (`u128::MAX`) +
`,"scheme":` + 33 (`{"ReedSolomon":{"k":255,"m":255}}`) + `,"len":` + 20 (`u64::MAX`) +
`,"placement":` + `[` + 9 × 21 + `}`. `⌊50 000 / (319 + 1)⌋ = 156`; `157 × 320 = 50 240` does
not fit, and a `const` assertion pins both directions so neither a looser nor a tighter edit
compiles. The skeleton arithmetic is **exact, not conservative-by-guess**, and the test proves it
where every number is spent at full width: `encode(widest RS(255,255) ref).len()` equals
`chunkref_bytes(510)` = 10 840 to the byte, so a wrong field-name or punctuation count could not
hide. At the reference width the bound has exactly 4 bytes of slack (measured 315 vs 319) — the
four digits `k = 6` and `m = 3` do not spend — which is the only direction that is safe to be
loose in.

### Why the width is taken at the reference fragment count — the decision a reviewer will probe

`ChunkRef.placement` is `Vec<u64>` of length `fragment_count()`, and `EcScheme::ReedSolomon`
takes `k, m: u8` — so the widest chunk-ref *the type can express* is RS(255, 255): 510
placements, **10 840 B**, giving `⌊50 000/10 841⌋ = 4` chunks per part. I ruled that out, and
the reason is not cost:

* `0016:1466` settles `MAX_PART_CHUNKS` at **165–381** and `0016:1053` states the width range it
  comes from — "a `ChunkRef` encodes to ~131 B (small D-server ids) to ~302 B (worst-case `u64`
  ids)" — at the RS(6,3) reference scheme of `0016:1039`. A format maximum of **4** would refuse
  the entire settled range, i.e. `Budget::new` would reject every profile the proposal calls
  legal and multipart would be unconfigurable. A format max must be ≥ every value a legal
  deployment writes, or a durable record stops decoding — precisely the `0016:390-402` hazard
  this slice exists to respect.
* The residual is real and is **placed**, not ignored: a deployment running wider than the
  reference (RS(10,4) ⇒ 429 B ⇒ 116 chunks) needs a *tighter* bound than 156, and that bound is
  computable only where the EC scheme is known — `UploadPart`'s `max_part_bytes` refusal
  (`0016:1466`), #508's. So `chunkref_bytes` and `max_chunks_per_value` are **public `const
  fn`s**: the admitting slice calls the same rule with its own width instead of re-deriving it.
  This is exactly the boundary the brief itself draws for `B_ops` ("a backend-calibrated
  deployment knob owned by #625 … not a format constant, so by `0016:390-402` it does not belong
  at decode"), applied to the one other deployment-shaped factor in the same formula.
  The test pins the direction: `max_chunks_per_value(chunkref_bytes(14)) < 156`.
* Note 156 is **below** the proposal's stated lower end of 165, deliberately: 165 was computed
  from a 302-byte `b_ref` that predates `ChunkId: u128` (39 digits, `crates/traits/src/lib.rs:26`)
  and charges no array separators. The iteration-1 reviewer measured the consequence — 165 refs
  at 303 B = **50 161 stored bytes against a 50 000-byte budget**. 156 is that arithmetic
  corrected, and the correction is *measured in the shipped test*, not asserted here.

### Leg 1f-iii and `metadata::MAX_ROOT_SEGMENTS` (the C1 judgment cell from iteration 2)

The brief names `metadata::MAX_ROOT_SEGMENTS` (`metadata.rs:322`) for the publishable ceiling; a
reviewer flagged (NEEDS-HUMAN, not [impl]) that the same constant's doc calls itself deployment
*capacity*, deliberately not a decode invariant. I followed the brief, and the reason it is
sound here: it is a **compile-time** constant of this build, not a stored or live knob a rolling
change can lower under a record, which is what `0016:390-402` actually excludes; the *same*
`Budget::new` gates encode and decode, so no build can write a profile its own decode rejects;
and `0016:348` already requires a profile change to be "an explicit operator CAS gated on the
live population having drained", which is the migration a build that lowered the constant would
ride. The alternative — inventing `MAX_ROOT_SEGMENTS_FORMAT_MAX` — is what iteration 2 did, and
it drew the same C1 cell *plus* a "constant the brief did not ask for" surface. Left for the
human at sign-off; the code states the reasoning inline.

### The three canonicality blockers (review-batch findings at `:899`, `:901`, `:908`)

`decode_record` now accepts a value **only if** `encode_record(value)` reproduces the input
bytes. `mpuctl` is CAS'd whole (`require(mpuctl == prior)`, `0016:348`) with `encode(prior)` as
the precondition bytes (`metadata.rs:1766`), so a record stored with reordered fields, inserted
whitespace or a dropped-on-decode extra field would make **every** CAS against it fail forever —
the ledger silently wedged, no error anywhere. One typed error at the read that meets it instead.
This is the value-side of the two-spellings-of-one-record rule the module's own doc states for
keys, and of the rubric's *Serialization identity* class. Wire structs also carry
`deny_unknown_fields` for the better message.

### Leg 1g (`multipart.rs:1074` blocker) versus the iteration-2 "unit mixing" objection

The bound shipped is `max_staged_chunks + max_inflight_parts × max_part_chunks ≤ SCAN_CAP/2`,
named `staged_reference_scan()` — the **per-session staged-reference set one pass materializes**,
which is `0016:1447`'s own charge ("the true staged total never exceeds `MAX_STAGED_CHUNKS +
MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`"), not "the `sidx:` record count" (the phrasing the
iteration-2 reviewer read as mixing units — both terms are per-chunk of one session, so the sum
is dimensionally sound; the naming and the doc now say so explicitly). It **subsumes** the
in-flight-only bound `0016:1476` states, so I removed that separate check rather than shipping
both: a redundant guard can never be isolated by a negation (any value violating the product also
violates the sum), which would have left one of the six required negations unfalsifiable. Cost of
the strictness, measured: it tightens the admissible `max_inflight_parts` at the reference
profile from 3 216 to 2 848 — both orders of magnitude above any realistic value (tens).

---

## 2. Forced self-refutation (recorded per the Do beat's three questions)

**(a) Genuine red?** Yes. With production reverted and the test kept, the test target does not
compile:

```
error[E0432]: unresolved imports `wyrd_core::multipart::chunkref_bytes`,
  `wyrd_core::multipart::decode_admission_record`, … `wyrd_core::multipart::VALUE_CEILING_HALF`
error[E0599]: no variant named `Structural` found for enum `RecordError`
error: could not compile `wyrd-core` (test "multipart_budget_admission") due to 2 previous errors
```

This is **criterion-absence, born-at-tier**, exactly as the brief pre-declares (posture (a)):
C4-verify's RED leg will report UNVERIFIABLE (exit 77) because the RED leg fails without running
a test. The binding evidence is therefore the **seven** one-check negations in §3, each of which
is a behavioural red on the shipped test.

**(b) Production path?** Yes. The test imports `wyrd_core::multipart::{Budget, AdmissionRecord,
encode_record, decode_admission_record, chunkref_bytes, max_chunks_per_value, …}` — the shipped
types and the shipped constants — and the widest-record oracle measures
`wyrd_core::metadata::encode` on a real `wyrd_core::metadata::ChunkRef`. There is no stand-in, no
copy of the derivation, and no re-implementation: the file recomputes **nothing** from the
production formula, every arithmetic expectation is a hand-computed literal with its derivation in
a comment beside it (`REFERENCE_U_REF = 89_856`, `REFERENCE_MAX_SESSIONS = 44`,
`REFERENCE_STAGED_SCAN = 84_864`, `u_ref = 14`/`max_sessions = 71` for the raw branch,
`max_sessions = SCAN_HALF` for the clamp branch, `MAX_CHUNKREF_BYTES = 319`, `156`, `79_872`,
`max_chunks_per_value(99) = 500`). The one place the test does read a production expression —
`encode(widest RS(255,255) ref).len() == chunkref_bytes(510)` — is an *equality between a
measurement and the formula*, which is the assertion's whole point, not a tautology: the left side
is `serde_json`'s output on the production type.

**(c) Fixture includes the fault?** Yes, per leg. Each 1f/1g negation feeds `Budget::new` a torn
tuple that violates **only** the bound it names (arithmetic in §3), so the guard under test is the
only thing standing between the test and a green; the 1a legs feed hand-authored `mpuctl` JSON
whose `max_sessions` is one above and one below what its own stored profile derives; leg 3 feeds a
`count` a thousand times its cap and requires it to **decode**. The widest-record oracle builds
the actually-widest writer-emitted chunk-ref (`u128::MAX` id, `u64::MAX` len, nine `u64::MAX`
D-server ids) — the case iteration 2's oracle curated out by measuring a "realistic" 303-byte ref.

---

## 3. The seven named negations (drop one check → run → paste → revert)

Runner: `cargo test -p wyrd-core --test multipart_budget_admission` (the brief's GREEN leg; the
whole-tree gate `./engine/xtask.sh ci` is green on the final tree, exit 0). Each negation removed
or disabled **exactly one** check and was reverted immediately after; the harness restored the
file byte-for-byte from a pristine copy each time. Green baseline: **12 passed, 0 failed**. All
seven runs below were re-executed against the **final** tree, so the pasted lines are current.

### 1f-i — `max_part_chunks ≤ MAX_PART_CHUNKS_FORMAT_MAX` dropped

```
thread 'leg_1f_i_max_part_chunks_obeys_the_value_ceiling_rule' panicked at
crates/core/tests/multipart_budget_admission.rs:99:43:
this profile must be refused: Budget { w_ref: 4000000, max_part_chunks: 157,
  max_parts_per_session: 10000, max_inflight_parts: 32, max_staged_chunks: 79872 }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```
Isolation: at `max_part_chunks = 157`, `max_staged_chunks` 79 872 still clears both its ends
(≥ 157, ≤ 79 872), in-flight 32 ≤ 10 000, staged-reference set 79 872 + 32×157 = 84 896 ≤ 524 288,
`U_ref` = 89 920 ≤ 4 000 000. **Exactly one** test fails.

### 1f-ii — `max_staged_chunks ≥ max_part_chunks` dropped

```
thread 'leg_1f_ii_max_staged_chunks_keeps_one_maximal_part_stageable' panicked at
crates/core/tests/multipart_budget_admission.rs:99:43:
this profile must be refused: Budget { w_ref: 4000000, max_part_chunks: 156,
  max_parts_per_session: 10000, max_inflight_parts: 32, max_staged_chunks: 155 }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```
Isolation: torn `max_staged_chunks = 155` is still ≤ 79 872, still leaves 155 + 4 992 ≤ 524 288
and `U_ref` = 10 139 ≤ 4 000 000.

### 1f-iii — `max_staged_chunks ≤ MAX_PUBLISHABLE_CHUNKS` dropped

```
thread 'leg_1f_iii_max_staged_chunks_stays_inside_the_publishable_ceiling' panicked at
crates/core/tests/multipart_budget_admission.rs:99:43:
this profile must be refused: Budget { w_ref: 4000000, max_part_chunks: 156,
  max_parts_per_session: 10000, max_inflight_parts: 32, max_staged_chunks: 79873 }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```
Isolation: torn 79 873 still ≥ 156, still 79 873 + 4 992 ≤ 524 288, `U_ref` = 89 857 ≤ 4 000 000.

### 1f-iv — `max_inflight_parts ≤ max_parts_per_session` dropped

```
thread 'leg_1f_iv_max_inflight_parts_fits_the_part_space_it_draws_from' panicked at
crates/core/tests/multipart_budget_admission.rs:99:43:
this profile must be refused: Budget { w_ref: 4000000, max_part_chunks: 156,
  max_parts_per_session: 31, max_inflight_parts: 32, max_staged_chunks: 79872 }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```
Isolation: 32 in flight against a 31-part space only *shrinks* `U_ref` (to 9 828); every other
bound holds.

### 1g — `staged_reference_scan ≤ SCAN_HALF` disabled (`> SCAN_HALF` → `> u64::MAX`; deleting the block leaves an unused binding and `-D warnings` refuses to compile, which is not a behavioural red)

```
thread 'leg_1g_the_staged_reference_set_counts_committed_chunks_too' panicked at
crates/core/tests/multipart_budget_admission.rs:99:43:
this profile must be refused: Budget { w_ref: 4000000, max_part_chunks: 156,
  max_parts_per_session: 10000, max_inflight_parts: 2849, max_staged_chunks: 79872 }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```
Isolation, and the *point* of the leg: the torn tuple's **in-flight term alone** is
2 849 × 156 = 444 444, comfortably inside 524 288 — an in-flight-only bound (the shape #692
shipped) admits it. Only the sum, 79 872 + 444 444 = 524 316, exceeds the bound. Every other
range end holds (2 849 ≤ 10 000; `U_ref` = 968 760 ≤ 4 000 000).

### 1a — `wire.max_sessions != derived` disabled

```
thread 'stored_max_sessions_disagreeing_with_its_profile_is_a_decode_error' panicked at
crates/core/tests/multipart_budget_admission.rs:190:18:
max_sessions is derived, never chosen: AdmissionRecord { count: 1, max_sessions: 45,
  profile: Budget { w_ref: 4000000, max_part_chunks: 156, max_parts_per_session: 10000,
  max_inflight_parts: 32, max_staged_chunks: 79872 } }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```
Isolation: the profile in that record is legal on its own; only the `max_sessions` field is torn
(45 against a derived 44).

### Leg 3, negated the other way (a check **added**: `count > max_sessions` made a decode error)

```
thread 'occupancy_above_the_stored_cap_still_decodes' panicked at
crates/core/tests/multipart_budget_admission.rs:366:10:
occupancy above a lowered cap is live state, not a torn record:
  Structural { record: "mpuctl", reason: "count exceeds max_sessions" }
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;
```

---

## 4. Gates run locally

| check | result |
|---|---|
| `cargo test -p wyrd-core --test multipart_budget_admission` | **12 passed, 0 failed** |
| `./engine/xtask.sh ci` (fmt / clippy `-D warnings` / build / whole-workspace test / deny / conformance / statics / prose) | **`xtask ci: all checks passed`, exit 0** |
| `cargo fmt --all -- --check` | clean (formatter run over both files; the patch is commit-hook ready) |
| `scripts/mutants-in-diff` (C5, advisory) | **88 mutants: 0 missed, 64 caught, 24 unviable** (was 25/46 missed) |
| `git apply --check` + apply onto a pristine `9dbcd72` tree | reconstructs both files byte-identically, and the reconstructed tree is green |

The whole-tree gate was re-run after **every** edit round; the run recorded above is on the exact
tree `patch.diff` reproduces.

The one mutant that survived the first mutants pass was
`replace + with * in max_chunks_per_value` — an *equivalent* mutant at the reference width
(⌊50 000/319⌋ = ⌊50 000/320⌋ = 156), i.e. the separator charge was untested where it is not
observable. Killed by pinning the rule at a width where it is: `max_chunks_per_value(99) == 500`
(an uncharged rule admits 505 — the same 5 refs by which 165 × 303 B became 50 161 stored bytes).
No other mutant survives.

## 5. One divergence from the brief's own citations (deliberate, verified)

The brief cites the `0016` knob table by line as `:1472` (`MAX_STAGED_CHUNKS`), `:1476`
(`MAX_INFLIGHT_PARTS`), `:1479` (`W_ref`) and `:1487` (`B_ops`). On the target tree those rows
are at **`:1468`**, **`:1471`**, **`:1473`** and **`:1475`** respectively — `:1472` is
`MAX_OWNED_FLEET` and `:1487` is a blank line. The patch cites the verified numbers, which also
agree with the *base* module's own pre-existing citation of `0016:1471` for `MAX_INFLIGHT_PARTS`
(`crates/core/src/multipart.rs:338`, merged with #691). Everything else the brief cites resolved
as written (`:348`, `:390-402`, `:1039`, `:1053`, `:1447`, `:1465`, `:1466`, `:1469`, `:1470`,
`:2593`, `:2605`; `metadata.rs:312-321`, `:322`, `:327`, `:329-352`; `traits/src/lib.rs:286`).
Flagging it because a reviewer diffing brief-against-patch will see the mismatch.

## 6. Deliberately not done

* **`B_ops`** — no value invented, per the brief. `Budget::new`'s doc names it and the
  `⌊(E_tx/2)/(bytes per slot key)⌋` clamp as #625's/#508's, enforced where work is admitted.
* **Every other record type** (`mpu:`, `part:`, `psum:`, `sidx:`, `retire:` payloads), the
  outcome enums, `MultipartEtag`, `sha2`, store round trips, `docs/`, `Cargo.toml` — child-2's,
  child-3's, #656–#659's. No file outside `multipart.rs` + the new test is touched; no
  `Cargo.toml`/`Cargo.lock` change.
* **`AdmissionRecord` mutators** (`with_count`, the increment path) — no writer exists until
  #656–#659, and unused API is review surface without a caller.

## 7. External dependencies

None beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
`typos` / `cargo-deny` / `cargo-machete` ran inside `cargo xtask ci` (green); `cargo-mutants` is
installed here and ran. Nothing to declare.

## 8. Scratch

Everything throwaway lived under `$PDCA_SCRATCH/pdca-builder-715-negations` (the pristine
production copy the negation harness restored from, the two negation scripts, the CI and mutants
logs) and is removed.
