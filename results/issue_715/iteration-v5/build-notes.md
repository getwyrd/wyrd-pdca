# Build notes — #715, multipart admission ledger (`Budget` + `AdmissionRecord`)

Round 4 (iteration 5 of the bundle, first round on the re-authored brief's *fourth* Do pass).
Target branch `getwyrd/wyrd @ main`; built in `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`
at `92e1b4b` (`9dbcd72` is its ancestor; `crates/core/src/multipart.rs` is byte-identical to
`9dbcd72`'s 854-line file, so every `multipart.rs` citation in the brief resolves unchanged.
`crates/core/src/metadata.rs` gained 46 lines in `d2609b2`, so the brief's `metadata.rs`
line numbers shift by ~+28 above line 1500 — this patch cites the **worktree** numbers, which
are the ones a reviewer reading the patched tree will see).

Two files, exactly as `Scope` names them:

| File | Added (raw) | Added (semantic: no blanks, comments or attrs) |
|---|---|---|
| `crates/core/src/multipart.rs` | 513 | 225 |
| `crates/core/tests/multipart_budget_admission.rs` (new) | 448 | 218 |
| **total** | **961** | **443** (budget ≤ 450) |

---

## 1. What the carry-forward asked for, and what changed because of it

Iteration 4's failing gate was **C5-mutants: 59 tested, 1 missed** — and the reviewer named it
precisely: *"Rebuild must add a witness where the ceiling arm determines `U_ref`: replacing its
addition with multiplication survives all tests"* (`crates/core/src/multipart.rs:1046`,
`crates/core/tests/multipart_budget_admission.rs:80`, both patched-file-relative to round 3).

The diagnosis is exact. `U_ref = min(raw, ceiling)` (`0016:1469`). Every witness round 3
shipped was decided by the **raw** arm:

| round-3 witness | raw | ceiling | `min` |
|---|---|---|---|
| legal ledger `(72_600, 165, 100, 10, 20_000)` | 18_150 | 23_300 | raw |
| P2 `(330, 165, 1, 1, 10_000_000)` | 330 | 10_000_330 | raw |
| P3-above `(1_000_000, 500_000, 1, 1, 500_000)` | 1_000_000 | 1_500_000 | raw |
| P-arith-accept `(2, 1, 1, 1, u32::MAX)` | 2 | 4_294_967_297 | raw |

With the ceiling always discarded, its arithmetic was unmeasured: `msc + 2·mip·mpc` could be
`msc × 2·mip·mpc` and every assertion still passed. That is not a cosmetic mutation — a `U_ref`
that is too small derives a `max_sessions` that is too large, which is the exact fleet-wide
over-admission this record exists to prevent (`0016:2593`, X64).

Three changes answer it:

1. **A ceiling-bound witness** — `{ count: 0, max_sessions: 10, profile: { w_ref: 700,
   max_part_chunks: 10, max_parts_per_session: 100, max_inflight_parts: 3,
   max_staged_chunks: 10 } }`: raw `= (100+3)×10 = 1_030`, ceiling `= 10 + 2×3×10 = 70`, so the
   **ceiling determines** `U_ref = 70` and `max_sessions = min(700/70, 524_288) = 10`
   (`crates/core/tests/multipart_budget_admission.rs:136-155`). Every guard holds on it
   (G5: `3×10 = 30 ≤ 524_288`; G6: `10 ≥ 10`; G7: `700 ≥ 70`; G8 exact).
   The operands are picked so **every** single-operator slip in that term moves the value:
   `70` vs `600` (`+`→`×`), `60` (`2×mip` → `2+mip`), `36` (`(2·mip)×mpc` → `+`), `10`
   (either `×`→`/`), `16` (`×`→`%`), and `2−mip`/`msc−…` underflow-panic. `mip = 3` is chosen
   deliberately: at `mip ∈ {1, 2}` the `2 × mip` mutants `2/1`, `2+2` collide with `2×mip`.
2. **A `SCAN_CAP/2`-clamp witness** for `MAX_SESSIONS`' *second* term, which round 3 also never
   exercised (every witness had `⌊W_ref/U_ref⌋ < 524_288`, so the clamp could have been absent):
   `w_ref = 2_000_000`, `U_ref = 2` ⇒ quotient `1_000_000`, clamp ⇒ `524_288` accepted, and the
   record storing the *unclamped* `1_000_000` refused as `MaxSessionsNotDerived { stored:
   1_000_000, derived: 524_288 }` (`…/multipart_budget_admission.rs:165-180`). Without the clamp
   the two verdicts swap, so the pair binds it in both directions.
3. **One definition of the owned-`sidx:` product**, `Budget::inflight_owned_refs`
   (`crates/core/src/multipart.rs:1084-1086`), used by **both** G5
   (`multipart.rs:1219`) and `U_ref`'s ceiling term (`multipart.rs:1116`). Round 3 spelled the
   same quantity twice (`u64::from(mip) * u64::from(mpc)` in G5, `2 * inflight * part_chunks` in
   `u_ref_of`), so the rule and the charge could drift apart. This is the rule
   `checked_chunk_bytes` states for the other cross-checked quantity in this repo
   ("One definition, used by both the constructor and the decode check, so the two can never
   disagree", `crates/core/src/metadata.rs:1208-1210`).

**Result: `scripts/mutants-in-diff` on this patch — `59 mutants tested in 2m: 54 caught,
5 unviable`, 0 missed** (was 51 caught / 7 unviable / **1 missed**).

The other two carry-forward items are explicitly human-judgment ones, not rebuild work: C4's
"humans must accept compile-only criterion absence as sufficient red evidence" (the brief
**pre-declares** the exit-77 UNVERIFIABLE, see §5) and T4's prior-art/contribution evidence
(publish-time artifacts + the archived iterations, none of which Do produces).

---

## 2. Shape, and why this shape

Three parts, all mirroring an existing peer, none of them new machinery (the brief's
`Citations expected` (i)–(iii)):

* **Codec** — the store-wide `metadata::encode` / `metadata::decode`
  (`crates/core/src/metadata.rs:1564`, `:1569`). No second encoder, no `encode_record`
  envelope (the brief adjudicates that out; §4 records what I did *not* build).
* **S1, validation inside `Deserialize`** — `#[serde(try_from = "AdmissionRecordWire")]` on
  `AdmissionRecord` (`multipart.rs:1269`) and `#[serde(try_from = "BudgetWire")]` on `Budget`
  (`multipart.rs:1042`), the `InodeRecord` / `InodeRecordWire` pattern
  (`metadata.rs:1377`, `:1439`). So `metadata::decode::<AdmissionRecord>` cannot yield a
  malformed value either.
* **S2, a per-record decode that attributes the failure** — `pub fn decode_admission_record`
  (`multipart.rs:1348-1355`), the peer of `decode_segment_record` (`metadata.rs:2536-2547`).

**The trap the brief names, and how I avoided it.** `decode_segment_record` recovers its type by
`err.downcast::<ChunkMapError>()` *after* `decode` (`metadata.rs:2541`) — a branch that can
never fire, because serde's `Error::custom` funnel has already turned the domain error into a
`serde_json::Error`. I do **not** mirror that dead branch and do **not** parse messages:
`decode_admission_record` decodes the **wire** struct (which has no domain rules and so no
funnel to cross) and then applies the record's own `TryFrom`, where the `RecordError` is
returned as itself. That is the whole reason `AdmissionRecordWire::profile` is a `BudgetWire`
rather than a `Budget` (`multipart.rs:1305`): a nested validating `Deserialize` would have been
stringified before S2 could see it.

*Cost of the alternative, concretely:* keeping the base's `downcast` shape would collapse all
eight guards into one `MalformedRecordValue { detail: String }` at S2 — 8 typed variants
(`multipart.rs:145-207`) become 1, and the eight `assert_eq!(decode_both(&w),
Err(RecordError::G…{…}))` assertions in the test become eight `.to_string().contains("…")`
substring matches, which cannot tell G4's rejection from G7's — exactly what the brief forbids
("a shared variant carrying a free-text detail is NOT sufficient").

**Guard → variant → site**, one distinct variant each:

| Rule | `RecordError` variant | Enforced at |
|---|---|---|
| G1 `max_part_chunks ≥ 1` | `MaxPartChunksZero` | `multipart.rs:1184` |
| G2 `max_inflight_parts ≥ 1` | `MaxInflightPartsZero` | `multipart.rs:1188` |
| G3 `max_parts_per_session ≤ MAX_PART_NUMBER` | `MaxPartsPerSessionUnaddressable` | `multipart.rs:1192` |
| G4 `mip ≤ mpps` | `InflightPartsExceedSessionParts` | `multipart.rs:1198` |
| G5 `mip × mpc ≤ SCAN_CAP/2` | `OwnedStagingRangeUnscannable` | `multipart.rs:1219` |
| G6 `msc ≥ mpc` | `StagedChunksBelowPartChunks` | `multipart.rs:1226` |
| G7 `w_ref ≥ U_ref` | `ReferenceBudgetBelowFootprint` | `multipart.rs:1235-1236` |
| G8 `max_sessions == derived` | `MaxSessionsNotDerived` | `multipart.rs:1318` |
| (wire fault, not a rule) | `MalformedRecordValue` | `multipart.rs:1350` |

G1 and G2 are checked **first and over the wire tuple**, before anything derives: they are what
make `u_ref_exact` and `max_sessions` total. G5–G7 are stated over a **candidate** `Budget`
(`multipart.rs:1210-1242`) — a local value returned only if every rule holds, so no caller can
hold a `Budget` that broke one. That is what lets G5 and G7 share one definition of the
quantities they judge with the derivations themselves; it also removed 21 semantic lines
(measured: production 246 → 225 semantic) versus the round-3 free-function spelling with four
explicit `u32` parameters and two six-line call sites.

**Exactness** is `u128` throughout (`multipart.rs:1084-1086`, `:1113-1118`). Alternatives
considered: saturating ops (legal per the brief, 0 lines cheaper, but `ReferenceBudgetBelowFootprint
{ u_ref }` would then report a saturated stand-in instead of the number the operator must act
on) and the division form of G5 (needs a `mpc ≠ 0` precondition that G1 supplies but which
couples the two rules). `u128` is the mathematical value at zero cost — every operand here is at
most `2^65`.

**`deny_unknown_fields`** sits on both wire structs (`multipart.rs:1150`, `:1301`), the brief's
settled durable-format decision; the doc records both live CAS shapes it forecloses
(`metadata.rs:1794`/`:1919` re-encoded prior; `metadata.rs:2012` raw bytes) and that a future
additive field is therefore a versioned format change — a §6-worthy consequence, flagged in the
`Budget` doc rather than buried.

---

## 3. The twelve demonstrations (leg → kind → negation → output)

Method: apply exactly one negation to the shipped `crates/core/src/multipart.rs`, run
`cargo test -p wyrd-core --test multipart_budget_admission`, paste, revert. The harness and full
log are in `$PDCA_SCRATCH/pdca-builder-715-negations/` (`negate.py`, `negations.log`; removed at
the end of the run — the pastes below are the whole content that mattered). Baseline: **17
passed, 0 failed**.

**Kind 1 — seven isolating negations. Each witness violates only its own rule**, which the
output proves twice over: with the rule dropped the witness decodes `Ok(...)` (so it satisfied
every *other* rule), and exactly one test flips.

| Leg | Negation | Result | Pasted output |
|---|---|---|---|
| **G2** | delete `if wire.max_inflight_parts == 0 {…}` | 16 passed, **1 failed** | `left: Ok(AdmissionRecord { count: 0, max_sessions: 1, profile: Budget { w_ref: 1, max_part_chunks: 1, max_parts_per_session: 1, max_inflight_parts: 0, max_staged_chunks: 1 } })` / `right: Err(MaxInflightPartsZero)` |
| **G3** | delete the `> MAX_PART_NUMBER` check | 16 passed, **1 failed** | `left: Ok(… max_parts_per_session: 1000000 …)` / `right: Err(MaxPartsPerSessionUnaddressable { max_parts_per_session: 1000000 })` |
| **G4** | delete the `mip > mpps` check | 16 passed, **1 failed** | `left: Ok(… max_parts_per_session: 10, max_inflight_parts: 20 …)` / `right: Err(InflightPartsExceedSessionParts { max_inflight_parts: 20, max_parts_per_session: 10 })` |
| **G5** | delete the `inflight_owned_refs() > SCAN_HALF` check | 15 passed, **2 failed** | `left: Ok(… max_inflight_parts: 524289, max_part_chunks: 1 …)` / `right: Err(OwnedStagingRangeUnscannable { max_inflight_parts: 524289, max_part_chunks: 1 })` — the second failure is **P-arith-reject**, whose witness the brief itself requires to be refused *by G5*, so it is the same rule, not a second one |
| **G6** | delete the `msc < mpc` check | 16 passed, **1 failed** | `left: Ok(… max_part_chunks: 165 … max_staged_chunks: 164 })` / `right: Err(StagedChunksBelowPartChunks { max_staged_chunks: 164, max_part_chunks: 165 })` |
| **G7** | delete `let u_ref = …; if w_ref < u_ref {…}` | 16 passed, **1 failed** | `left: Ok(… w_ref: 1 … })` / `right: Err(ReferenceBudgetBelowFootprint { w_ref: 1, u_ref: 2 })` |
| **G8** | delete `if wire.max_sessions != derived {…}` | 15 passed, **2 failed** | `left: Ok(… max_sessions: 5 …)` / `right: Err(MaxSessionsNotDerived { stored: 5, derived: 4 })`; and `left: Ok(… max_sessions: 1000000 …)` / `right: Err(MaxSessionsNotDerived { stored: 1000000, derived: 524288 })` — both are G8 assertions (the second is the new clamp leg's reject half) |

**Kind 2 — G1, the totality precondition, demonstrated as what it is.** No value violates G1
while satisfying G8, because at `max_part_chunks = 0` the derivation is **undefined**: `U_ref`
is `0` and `MAX_SESSIONS`' quotient has no divisor. So the demonstration is not "one assertion
flips" but "**totality breaks**":

```
---- g1_zero_part_chunks_is_refused_before_the_derivation stdout ----
thread 'g1_zero_part_chunks_is_refused_before_the_derivation' panicked at crates/core/src/multipart.rs:1142:9:
attempt to divide by zero
test result: FAILED. 16 passed; 1 failed
```

`multipart.rs:1142` is `(self.w_ref / self.u_ref()).min(SCAN_HALF)`. Removing G1 turns a typed
rejection into a **panic inside the decoder** — i.e. it breaks *totality*, not one assertion.
That is the isolation waiver the brief grants G1, honoured in its own shape.

**Kind 3 — four inverted legs, negated by ADDING the bound that would refuse the witness.**

| Leg | Negation (added bound) | Result | Pasted output |
|---|---|---|---|
| **P1** | `if wire.count > wire.max_sessions { Err(…) }` | 16 passed, **1 failed** | `occupancy above the cap is not a decode error: MaxSessionsNotDerived { stored: 9000, derived: 4 }` |
| **P2** | `if msc > 512 * 8_000 { Err(…) }` (an *invented* `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS`) | 14 passed, **3 failed** | `a large staged ceiling is not a decode error: StagedChunksBelowPartChunks { max_staged_chunks: 10000000, max_part_chunks: 165 }` |
| **P3** | `if mpc < 165 \|\| mpc > 381 { Err(…) }` (the knob window) | 8 passed, **9 failed** | `a small part-chunk cap is not a decode error: StagedChunksBelowPartChunks { max_staged_chunks: 1, max_part_chunks: 1 }` |
| **P-arith** | replace both exact `u128` spellings with the naive same-width ones (`msc + 2·mip·mpc`, `mip × mpc` in `u32`) | 15 passed, **2 failed** | accept half: `panicked at crates/core/src/multipart.rs:1116:13: attempt to add with overflow`; reject half: `panicked at crates/core/src/multipart.rs:1085:20: attempt to multiply with overflow` |

Both halves of P-arith go red under the naive spelling, as the brief requires: **P-arith-accept**
panics in debug (it would wrap in release), and **P-arith-reject** stops naming G5 — it panics
before reaching any verdict at all.

**P-arith-reject: which guard is named, and the exact-arithmetic check.** The implementation's
evaluation order reaches **G5** first, and G5 is a *genuine* violation under exact integers:
`mip × mpc = 999_999 × 4_294_967_295 = 4_294_963_000_032_705`, against `SCAN_CAP/2 = 524_288`.

One correction to the brief, recorded because it is arithmetic and checkable: the brief says the
same witness also violates **G7** ("no representable `w_ref` reaches that `U_ref`"). It does not.
Exactly, `U_ref = min(1_999_998 × 4_294_967_295, 4_294_967_295 + 2×999_999×4_294_967_295) =
min(8_589_926_000_065_410, 8_589_930_295_032_705) = 8_589_926_000_065_410`, which is ~4.7×10³
times *smaller* than `u64::MAX = 18_446_744_073_709_551_615` — so at `w_ref = u64::MAX` **G7
holds**. The second rule this witness breaks is **G8**: derived `max_sessions =
min(u64::MAX / 8_589_926_000_065_410, 524_288) = 2_147`, against a stored `u64::MAX`. The
G5-negation run prints exactly that (`Err(MaxSessionsNotDerived { stored:
18446744073709551615, derived: 2147 })`), which is independent confirmation of the number. The
briefed observable is unaffected: decode rejects, names G5, and exact arithmetic agrees G5 is
violated — no `Ok` via a wrapped product, no panic, no variant naming a rule that holds.

---

## 4. What I deliberately did **not** build

* **No `encode_record` / `decode_record` envelope.** The brief adjudicates it out (a stored value
  carries no type tag, so an arm would have nothing to dispatch on) and the module header's
  forward reference to one is withdrawn in this patch (`multipart.rs:8-16`). Cost of building it
  anyway, concretely: one `enum` + two functions with exactly **one** arm (~25 semantic lines)
  duplicating `metadata::encode`/`decode` — the duplication v3's T2 failed on.
* **No configuration-validation constructor.** No `Budget::new`, no knob-range check: no
  `max_chunkref_bytes`, no `B_ops`, no `MAX_SEG_CHUNKS`, no 165–381 window, no
  `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` product, no invented stand-in for any of them. P2 and P3
  are the tests that make that boundary falsifiable, and each carries the "**superseded by
  #508**" comment the brief requires (`…/multipart_budget_admission.rs:334-336`, `:349`).
* **No `docs/` change** (leg D, reversed at the brief's revision pass) and **no `metadata.rs`
  change** — this slice only reads its constants, which is what keeps the bundle out of
  #710/#711's conflict set.
* **Two accessors that would have been convenient are absent** (`Budget::new`, an
  `AdmissionRecord::with_count`): the first writers are #656–#659, and a constructor with no
  caller is an untested surface.

---

## 5. Verification, and the two pre-declared §6 items

* **Green leg**, through the project's own per-fix runner: `./engine/scripts/run-verify.sh`
  (the `C4-verify` gate) applied `patch.diff` to a **clean `origin/main` checkout** in the
  isolated `../wyrd-verify` worktree and reported
  `run-verify.sh: GREEN — cargo test -p wyrd-core --test multipart_budget_admission (fix
  applied)` → `test result: ok. 17 passed; 0 failed`. That is also the proof the patch applies
  cleanly to the branch target (`origin/main @ 92e1b4b`, fetched at build time).
* **Whole-tree gate:** `./engine/xtask.sh ci` (typos + docs + fmt + clippy at the workspace's
  `all = "deny"` + build + test incl. DST + machete + deny + conformance) over the patched
  worktree → `xtask ci: all checks passed`, exit 0. `typos` is installed on this host, so the
  prose gate genuinely ran rather than warn-skipping.
* **C5-mutants** (`scripts/mutants-in-diff`, the gate that failed round 3):
  **59 mutants tested in 2m: 54 caught, 5 unviable, 0 missed.**
* **Formatter / commit hooks:** `cargo fmt --all` applied; both files are rustfmt-clean, and
  clippy is clean at the workspace's `all = "deny"` level.
* **§6 item 1 — the C4-verify RED leg is UNVERIFIABLE (exit 77), pre-declared by the brief and
  now measured rather than predicted.** This is criterion-*absence* work (born-at-tier):
  reverting production removes the very symbols (`Budget`, `AdmissionRecord`,
  `decode_admission_record`, the eight `RecordError` variants) the test names, so the RED leg
  does not compile and runs 0 tests. Observed verbatim: `error[E0432]: unresolved imports
  wyrd_core::multipart::decode_admission_record, wyrd_core::multipart::AdmissionRecord … could
  not compile wyrd-core (test "multipart_budget_admission") due to 12 previous errors` →
  `run-verify.sh: UNVERIFIABLE — the RED leg's cargo run failed (status 101) WITHOUT running a
  test`, **exit 77** (`run-verify.sh:486-500`). The twelve demonstrations in §3 are the
  substitute the brief prescribes, and they are stronger than a whole-file revert: each isolates
  **one** rule.
* **§6 item 2 — docs currency, deferred by the brief** (leg D reversed). This slice persists
  nothing, so the more specific in-file rule (`multipart.rs:63-72` in the patched file: the
  living architecture doc gains these namespaces "with the slice that first *persists* one")
  governs over `AGENTS.md:154-157`'s general one. The in-file text is updated to stop calling the
  module "the key **grammar** only" while keeping that policy intact. If sign-off takes the
  general rule instead, the remedy the brief names is a one-paragraph follow-up commit on this
  branch **plus** the matching correction of that comment — not a re-plan.

---

## 6. The three refutation questions

**(a) Genuine red? — YES.** Not a claim, a measurement: every one of the twelve legs was
re-run against *the shipped code* with its own negation applied, and every one went red
(§3; baseline 17/17 green, each negation 16/1, 15/2, 14/3, 8/9 or a decoder panic). The
whole-fix revert is the weaker instrument here and is honestly reported as UNVERIFIABLE
(§5) rather than dressed up: with production reverted the test cannot compile, so it cannot
run — which is why the per-rule negations exist. Independently, `cargo mutants --in-diff`
mutated 59 points in this diff and the suite caught every viable one (54 caught, 5 unviable,
0 missed) — re-run against the *final* patch, not an earlier draft.

**(b) Production path? — YES.** The test imports only `wyrd_core::metadata` and
`wyrd_core::multipart` and drives the two production surfaces on every witness through
`decode_both` (`…/multipart_budget_admission.rs:58-71`, the helper both surfaces run through): `decode_admission_record` (S2,
`multipart.rs:1348`) **and** `metadata::decode::<AdmissionRecord>` (S1, `metadata.rs:1569`),
asserting they agree on verdict and value. The round-trip leg re-encodes through the production
`metadata::encode` (`metadata.rs:1564`) and asserts **byte identity** against the stored bytes.
There is no stand-in, no re-implementation of the guards in the test, and no constructor path:
every witness is hand-authored **JSON bytes**.

**(c) Fixture includes the fault? — YES.** Every negative witness *is* the torn record, fed as
bytes: `max_part_chunks: 0` (G1), `max_inflight_parts: 0` (G2), `max_parts_per_session:
1_000_000` (G3), `20 > 10` (G4), `524_289 × 1 > 524_288` (G5), `164 < 165` (G6), `w_ref: 1 <
U_ref: 2` (G7), `max_sessions: 5 ≠ 4` and `1_000_000 ≠ 524_288` (G8), plus the field maxima
(`u64::MAX`, `u32::MAX`) for P-arith and unknown/absent/out-of-range fields for the wire-shape
leg. Nothing is curated out: the G-witnesses are single-field perturbations of the *accepted*
ledger, and each one's isolation is proved by the fact that removing only its own rule makes
that same record decode `Ok` (the `left: Ok(...)` halves in §3).

---

## 7. Citations (all worktree/target-branch, `92e1b4b`)

Production, patched file: module header `multipart.rs:1-25`; living-architecture policy
`:63-72`; `RecordError` doc `:88-94` and the nine new variants `:135-207`; `Display` arms
`:235-286`; `SCAN_HALF` `:1012`; `Budget` `:1043-1148` (`inflight_owned_refs` `:1084`,
`u_ref_exact` `:1113`, `u_ref` `:1126`, `max_sessions` `:1141`); `BudgetWire` `:1149-1157`;
`TryFrom<BudgetWire>` `:1178-1243`; `AdmissionRecord` `:1269-1296`; `AdmissionRecordWire`
`:1297-1306`; `TryFrom<AdmissionRecordWire>` `:1312-1330`; `decode_admission_record`
`:1348-1355`.
Read-only peers: `metadata::encode`/`decode` `metadata.rs:1564`, `:1569`; `InodeRecord`
`try_from` `metadata.rs:1377`, `:1439`; `SegmentRecord`'s hand-written `Deserialize`
`metadata.rs:1240-1246`; `decode_segment_record` `metadata.rs:2536-2547`;
`checked_chunk_bytes` `metadata.rs:1208-1211`; `MAX_ROOT_SEGMENTS`'s liberal-on-read boundary
`metadata.rs:302-322`; the two CAS shapes `metadata.rs:1794`, `:1919`, `:2012`; `SCAN_CAP`
`crates/traits/src/lib.rs:273-286`; `MAX_PART_NUMBER` `multipart.rs:417`; proposal 0016
`:348`, `:390-402`, `:1466-1473`, `:2098`, `:2593`, `:2605`; ADR-0045
`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`, `:73-74`.
