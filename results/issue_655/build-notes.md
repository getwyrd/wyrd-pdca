# Build notes — issue 655 (multipart knob constants + derivations), iteration 3

Base: `origin/main` @ `605b33a` (the #691–#693 chain merged; the worktree HEAD equals
`origin/main`). Two files, as the brief allows: `crates/core/src/multipart.rs` (modified) and
`crates/core/tests/multipart_knobs.rs` (new). `metadata.rs`, `lib.rs`, docs and manifests are
untouched. Line numbers below are the patched files'.

## 1. What changed, and where

`crates/core/src/multipart.rs`

| Change | Lines |
|---|---|
| Module header: one paragraph naming section 14 | `:21-25` |
| Import `MAX_ROOT_SEGMENTS`, `MAX_ROOT_VALUE_BYTES`, `MAX_VALUE_BYTES` from `metadata` (consumed, never re-spelled) | `:131-134` |
| `Budget::inflight_owned_refs`, `u_ref_exact`, `u_ref`, `max_sessions` made `const fn`, same results | `:1600`, `:1629`, `:1645`, `:1668` |
| `checked_rules` doc: the stale "those constants have no definition on this base" now points at `knob_clamps_hold`; the fn itself is unchanged | `:1680-1689` |
| Section 14: constants, `KnobSet`, `KnobClamp`, `knob_clamps_hold`, `admission_backoff_millis` | `:4346-4913` |

Section 14 items: `SIZING_SCHEME` `:4372`, `MAX_CHUNKREF_BYTES` `:4390`,
`max_chunkref_bytes_for` `:4396`, `widest_chunk_ref` `:4401`, `value_chunk_capacity` `:4422`,
`VALUE_CHUNK_CAPACITY` `:4431`, `MAX_MAP_CHUNKS`/`MAX_SEG_CHUNKS`/`MAX_PART_CHUNKS`
`:4440-4453`, `max_part_bytes` `:4461`, `MAX_PARTS_PER_SESSION` `:4470`, `MAX_INFLIGHT_PARTS`
`:4488`, `MAX_STAGED_CHUNKS` `:4499`, `MAX_BATCH_BYTES` `:4523`, **`MAX_SEGMENT_PUT_BYTES`
`:4536` (new this round)**, `SLOWEST_OP_MILLIS` `:4544`, **`DEADLINE_OPS` `:4549` (new)**,
`MAX_BATCH_OPS` `:4562`, `PART_COMMIT_FIXED_OPS` `:4569`, `FENCE_FIXED_OPS` `:4576`,
`SLOT_PIN_BYTES` `:4582`, `W_REF` `:4594`, `SHIPPED_PROFILE` `:4603`, `U_REF` `:4617`,
`MAX_SESSIONS` `:4624`, `MAX_OWNED_FLEET` `:4632`, `R_PUBLISH` `:4648`,
`MAX_COMPLETE_ATTEMPTS` `:4656`, `MAX_UPLOAD_ID_ATTEMPTS` `:4664`,
`MAX_ADMISSION_CAS_ATTEMPTS` `:4678`, backoff base/cap `:4682-4686`,
`admission_backoff_millis` `:4704`, `KnobSet` `:4724` (`DEPLOYED` `:4761`, `profile` `:4782`),
`KnobClamp` `:4797`, `ensure` `:4850`, `knob_clamps_hold` `:4866`. Compile-time ties: `:4435`,
`:4502`, `:4527`, `:4634`, `:4688`.

`crates/core/tests/multipart_knobs.rs` — 8 tests: leg 1 `:166`, leg 2 `:181`, leg 3 `:274`,
`:305`, `:332`, leg 4 `:355`, leg 5 `:384`, `:407`. The leg-2 table is the `verdicts!` macro
(`:78`): one `set => verdict;` row per line, each failing with its own source text as the message.

## 2. The carry-forward, item by item

| Carry-forward finding (iteration 2) | What this build does |
|---|---|
| **C5 / T4 blocking** — `B_bytes` only checked against slot pins, so `batch_bytes = slot_bytes` passed while a segment batch of `⌊B_bytes / V⌋` puts was zero and publication could never advance (`multipart.rs:4980` of v2) | New clamp `KnobClamp::BatchBytesBelowSegmentPut` (`multipart.rs:4887`): `B_bytes ≥ MAX_SEGMENT_PUT_BYTES` = the widest `seg:` key (64 B) + a value at the ceiling (100,000 B), the per-put charge of `0016:661`. It is the largest item any splittable batch carries, so a budget that holds one holds one of each kind. Test rows: on the floor passes (`multipart_knobs.rs:198`), one byte under is refused by name (`:239`). The 64 is measured on `metadata::seg_key` by the test (`:156`, asserted at `:301`). Negation N6 below removes the check and leg 2 goes red naming exactly that row. |
| **T2 Shape FAIL** — 587 content lines vs the 400 cap | 388 by the same method (§7). How: `KnobClamp` variants no longer carry numbers (except `Profile(RecordError)`, which already does), so each clamp is one `ensure(cond, Variant)?` line and each leg-2 row is one line; the table is a small macro rustfmt leaves alone; the compile-time G1–G7 check on `SHIPPED_PROFILE` was dropped (§4.5), which also leaves `Budget::checked_rules` byte-for-byte as it was; the test imports `multipart::*`. No clamp was removed. Three v2 rows that repeated a clamp already covered were dropped (`max_sessions = 0`, `max_owned_fleet = 0` and `max_owned_fleet + 1`, each beside a one-step row for the same clamp), and the leg-4 parse-back of the cap keys (the grammar's own round trip, #691's tests) went. |
| **C5 mutants** — 2 survivors, both `<` → `<=` in the `if`-spelled `min`s of `u_ref_exact`/`max_sessions` | `min(a, b)` is now `a - a.saturating_sub(b)` (`:1634`, `:1671`) — no comparison operator, so no equivalent mutant exists. `scripts/mutants-in-diff`: **94 mutants, 61 caught, 0 missed, 33 unviable, 0 timeouts** (§6). |
| C1, C2, C4, T3, T5, fitness — deferred to sign-off | Not resolvable in code; restated in §5 with what this build shows. |

## 3. The value set (review it as a set)

| Constant | Value | Basis |
|---|---|---|
| `SIZING_SCHEME` | RS(6,3) | server's `DEFAULT_DURABILITY` (`crates/server/src/lib.rs:49`) |
| `MAX_CHUNKREF_BYTES` (`b_ref`) | 315 | measured: `u128::MAX` id, `u64::MAX` len, 9 × `u64::MAX` placement ids |
| `VALUE_CHUNK_CAPACITY` = `MAX_MAP_CHUNKS` = `MAX_SEG_CHUNKS` = `MAX_PART_CHUNKS` | 158 | ⌊50,000 / 315⌋ |
| `max_part_bytes(1 MiB)` | 165,675,008 B (158 MiB) | 158 × 1 MiB |
| `MAX_PARTS_PER_SESSION` | 10,000 | S3's per-upload limit |
| `MAX_INFLIGHT_PARTS` | 16 | 0016's worked value (`0016:2847`) |
| `MAX_STAGED_CHUNKS` | 80,896 | 512 × 158 |
| `MAX_BATCH_BYTES` (`B_bytes`) | 5,000,000 | `E_tx/2`; range now `[100,064, 5,000,000]` |
| `MAX_SEGMENT_PUT_BYTES` (new) | 100,064 | 64 B widest `seg:` key + 100,000 B value |
| `DEADLINE_OPS` (new, private) | 1,000 | 5,000 ms / 5 ms — top of `B_ops`' range |
| `MAX_BATCH_OPS` (`B_ops`) | 500 | half of `DEADLINE_OPS` — **5 ms per op assumed** |
| `SLOT_PIN_BYTES` (private) | 199 | 44 B widest slot key + 155 B widest slot value |
| `W_REF` | 4,000,000 | 0016's worked figure (`0016:2847`) |
| `U_REF` (derived) | 85,952 | min(raw 1,582,528, ceiling 80,896 + 2 × 16 × 158) — ceiling arm |
| `MAX_SESSIONS` (derived) | 46 | min(⌊4,000,000 / 85,952⌋, 524,288) |
| `MAX_OWNED_FLEET` (derived) | 116,288 | 46 × 16 × 158 ≤ 2,000,000 |
| `R_PUBLISH` / `MAX_COMPLETE_ATTEMPTS` | 3 / 3 | `[1, small]` |
| `MAX_UPLOAD_ID_ATTEMPTS` | 2 | one re-mint after a 2^-128 collision |
| `MAX_ADMISSION_CAS_ATTEMPTS` (derived) | 46 | = `MAX_SESSIONS` |
| backoff base / cap | 2 ms / 50 ms | windows [2,4], [2,8], [2,16], [2,32], then [2,50] |

Headroom of the shipped set against each clamp: part commit 166 ops ≤ 500; idle fence 22 ops ≤
500 and 3,184 B ≤ 5 MB; one segment batch holds ⌊5,000,000 / 100,064⌋ = **49** puts (0016 quotes
50 because it charges `V` without the key); `46 × 85,952 = 3,953,792 ≤ 4,000,000`; owned fleet
116,288 ≤ 2,000,000; owned `sidx:` per session 2,528 ≤ 524,288. Worst-case create wait with the
whole contention budget spent: 46 × 50 ms ≈ 2.3 s.

Every value is the same as iteration 2. What moved is the **range** of two knobs: `B_bytes` got
its floor, and `B_ops`' ceiling went from half the deadline to the whole deadline (§4.1).

## 4. Decisions and what I ruled out

### 4.1 `B_ops`' ceiling moved to the whole deadline; the shipped value stays at half

The new byte floor made v2's ceiling self-defeating. With `B_ops ≤ 500` the idle fence has at most
500 − 6 = 494 slots, whose pins weigh at most 494 × 199 = 98,306 B — below the 100,064 B floor
every accepted set now has. So `SlotRangeOverBytes` (the brief's leg-2 row "`MAX_INFLIGHT_PARTS`
whose whole-range fence/terminal-delete batch exceeds the mutation-byte budget") could never fire
on its own: no one-inequality row, no on-bound set for its `<=`, a guaranteed surviving boundary
mutant, and dead code in `knob_clamps_hold`.

Chosen: the clamp is the deadline itself — `B_ops ≤ DEADLINE_OPS = ⌊5,000 ms / 5 ms⌋ = 1,000`
(`multipart.rs:4549`, `:4888`) — and `MAX_BATCH_OPS = DEADLINE_OPS / 2` keeps the margin in the
value (`:4562`). That matches 0016's own split: the `B` row's range is "inside the 5-second half
of the envelope" (`0016:1475`, the time axis of `10 MB / 5 s`), and the margin is part of the
calibration ("fit the transaction deadline with margin", `0016:640-642`). With slots up to 994
the fence's pins can exceed the floor again, and leg 2 pins the clamp from both sides
(`multipart_knobs.rs:202`, `:246`: 502 slots pass at a 100,064 B budget, 503 are refused).

Ruled out:
* **Keep the ceiling at half.** The cost is the dead clamp above. The only honest way to keep it
  would be to delete `SlotRangeOverBytes` and prove at compile time that the ops clamp implies it
  — a scoping call against the brief's leg-2 list, so not mine to make silently.
* **No ceiling on `B_ops` at all.** Iteration 1's reviewers flagged the byte-axis twin of that
  (no upper bound on `B_bytes`) as a BUG three times.
* **Charge the fence's three fixed puts to `B_bytes`** (so pins + 3V ≤ `B_bytes` binds at
  budgets ≥ 300 KB). It changes `0016:1471`'s formula (`⌊(E_tx/2) / bytes per slot key⌋`) and the
  per-item model every other batch uses.

The trade the human should see: a deployment may now configure `B_ops` up to 1,000 — the whole
five seconds at the assumed 5 ms, with no margin left in the configuration. See §5 item 2.

### 4.2 The segment-put charge is key + a whole `V`, as a literal tied by the test

`MAX_SEGMENT_PUT_BYTES = 64 + MAX_VALUE_BYTES` (`:4536`). 0016 charges each segment put a whole
`V` (`0016:661`); adding the key can only make the floor stricter. A `seg:` value is in fact at
most `V/2` plus a few fields (the `MAX_SEG_CHUNKS` rule), so charging `V` is conservative by about
half — deliberately, because the consumer (#658's segment writer) will size by 0016's formula.
The 64 is a literal measured by the test on `metadata::seg_key` at the widest nonce, epoch and
index, the pattern `metadata.rs:302-356` uses for `MAX_ROOT_SEGMENTS`. A production-side
measuring function was about 4 more lines for the same guarantee.

### 4.3 `SLOT_PIN_BYTES` is a private literal pinned exactly by leg 2

v2 measured it in production with a 9-line function. Now it is `199` (`:4582`), and the test
measures a pin on the codec itself (`multipart_knobs.rs:142`) and builds the fence rows from that
measurement: 502 × p ≤ 100,064 < 503 × p holds only for p = 199, so the on-bound row and its
one-past twin pin the production number exactly, from both sides.

### 4.4 `KnobClamp` names the clamp; only `Profile(RecordError)` carries numbers

v2's variants each carried the numbers on both sides. rustfmt expands every such literal
vertically, which cost about 45 production lines and about 150 test lines (the v2 leg-2 table was
most of its 313-line test). The brief asks for "which clamp failed, not a bare bool" and for leg
2 to assert "the clamp it broke"; a unit variant does both. The profile rules keep their numbers
because they come from `RecordError`, which already has them and a `Display`.

### 4.5 No compile-time G1–G7 check on the shipped profile

v2 made `checked_rules` a `const fn` and ran it in a `const` block with `std::mem::forget`
(8 lines, and a change to reviewed code). Dropped: the derivations `U_REF`/`MAX_SESSIONS`/
`MAX_OWNED_FLEET` still evaluate at compile time (a profile they are undefined for fails the
build), and leg 1 runs every one of `Budget`'s rules on `KnobSet::DEPLOYED`, whose profile is the
same five constants as `SHIPPED_PROFILE` (said in its doc, `:4596-4602`).

### 4.6 `min` without a comparison

`Ord::min` is not callable in a `const fn` on the pinned 1.96 toolchain (probed: E0658,
"`Ord` is not yet stable as a const trait"). `raw - raw.saturating_sub(ceiling)` is `min` for
unsigned integers, has a comment saying so, and leaves cargo-mutants nothing equivalent to
generate. Each `-` mutant is caught by the arm tests (`- → +` caught; `- → /` divides by zero in
const eval and does not build).

### 4.7 Kept from v2 (reviewed there without findings)

* The aggregate is `KnobSet`, not `Budget`: `Budget` is the persisted `mpuctl` profile (#715,
  sealed behind `TryFrom<BudgetWire>`). `KnobSet::profile` builds a candidate `Budget` that never
  leaves `knob_clamps_hold` and is judged by the same `checked_rules` a decoded ledger is — one
  budget authority.
* The contention budget is derived, `MAX_ADMISSION_CAS_ATTEMPTS = MAX_SESSIONS`, with the clamp
  `attempts ≥ max_sessions`: among creates, a lost CAS means another create was admitted, so the
  refusal after `MAX_SESSIONS` losses is the capacity bound, not a lost race.
* `b_ref` = 315 with `len` at `u64::MAX`; 0016's "~302 B" renders `len` in seven digits.

## 5. NEEDS-HUMAN at sign-off

1. **C1 — 158 vs 0016's "165–381".** Unchanged from v2. The measured worst-case chunk ref is
   315 B, so the shipped caps are 158 — inside 0016's actual rule (`b_ref × chunks ≤ V/2`) but
   below the band 0016 computed from a 302 B estimate. The test pins the production rule to
   0016's band ends at 0016's own figures (131 → 381, 302 → 165) and pins the shipped measurement
   to the codec; it does not claim 158 ∈ [165, 381]. Options: accept 158; bound `len` with an
   enforced `chunk_size_max` (#508) and re-measure (8 digits of `len` gives 303 B → 165); or amend
   0016's prose.
2. **T3 — the 5 ms per operation is assumed, and now sets a ceiling too.** `MAX_BATCH_OPS = 500`
   and `DEADLINE_OPS = 1,000` both rest on `SLOWEST_OP_MILLIS = 5`, anchored only to 0016's
   warning that ~1,000 small marks "can exceed" 5 s (`0016:633-635`). No measured metadata-backend
   latency exists in the repo (searched `docs/` and `crates/`). #625 must calibrate it. New
   question this round: should the **ceiling** also keep margin (e.g. 750)? If so, the fence
   byte clamp stops being independently reachable (§4.1) and should be dropped with a proof.
3. **T5 / open question (a) — `W_REF` is a compile-time 4,000,000.** The doc (`:4584-4594`) says
   how #625 turns it into a deployment input without re-deriving anything.
4. **C2 / C4 — red is criterion absence, as the brief pre-declared.** `run-verify.sh`: GREEN 8/8
   on a clean `origin/main` + patch; RED `UNVERIFIABLE` (exit 77, E0432/E0425 — the test cannot
   compile without the symbols the patch adds). §6 has the negations that stand in for the red.
5. **Contention budget scaling** — derived as `MAX_SESSIONS`, so the worst-case create wait
   grows with it (2.3 s now; far longer at the `SCAN_CAP/2` extreme, though only under a storm
   that size).
6. **C4-ci** — last round's gate run timed out in `custodian_gc` (pre-existing, not this
   patch's). My run of the same gate on this tree is in §6.

## 6. Evidence

### The required negations (binding), each run through `run-verify.sh` and reverted

Each negated patch was generated from the final code into its own scratch bundle and run with
`PDCA_LANE=0 PDCA_BUNDLE=<scratch> ./engine/scripts/run-verify.sh`; in every case the GREEN leg
(the fix applied, with the one negation) fails. Logs: `$PDCA_SCRATCH/pdca-builder-655-logs/`.

**(2) `knob_clamps_hold` returns `Ok(())` unconditionally** (`if true { return Ok(()); }` as its
first statement). Leg 1 stays green, as the brief predicts for a vacuous check; 4 of 8 go red:

```
leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it --- FAILED
assertion `left == right` failed: with(D, |k| k.scheme = EcScheme::ReedSolomon { k: 6, m: 0 })
  left: Ok(())
 right: Err(SchemeUnsupported)
leg3_u_ref_and_the_session_limit_follow_the_formulas_under_either_arm --- FAILED
  left: Ok(())   right: Err(SessionsNotDerived)
leg3_each_budget_takes_the_half_0016_states --- FAILED
  left: Ok(())   right: Err(OwnedFleetOverHalfRef)
leg4_every_capacity_fits_its_key_space_in_byte_order_at_the_cap --- FAILED
  left: Ok(())   right: Err(Profile(PartsPerSessionUnaddressable { max_parts_per_session: 1000000 }))
test result: FAILED. 4 passed; 4 failed
```

**(3) One budget sized against the whole `MAX_VALUE_BYTES` instead of half** — caught at two
layers, and a second budget for good measure:

```
(3a) value_chunk_capacity divides MAX_VALUE_BYTES:
     error[E0080]: evaluation panicked: assertion failed:
       VALUE_CHUNK_CAPACITY as usize * MAX_CHUNKREF_BYTES <= MAX_ROOT_VALUE_BYTES
(3c) same, with both compile-time ties removed (runtime assertions only):
     leg3_the_chunk_caps_are_half_a_value_over_the_measured_worst_chunk_ref --- FAILED
       left: 317
      right: 158
     test result: FAILED. 7 passed; 1 failed
(3d) MAX_BATCH_BYTES = E_TX_BYTES (whole envelope), its compile-time tie removed:
     leg1 / leg2 / leg3 x2 FAILED — left: Err(BatchBytesAboveHalfEnvelope), and
       left: 10000000  right: 5000000      test result: FAILED. 4 passed; 4 failed
```

**(5) The two retry budgets collapsed into one constant.**

```
(5a) MAX_ADMISSION_CAS_ATTEMPTS = MAX_UPLOAD_ID_ATTEMPTS  -> 5 of 8 red
     leg1: left: Err(AdmissionCasBelowSessions)  right: Ok(())
     leg5_the_two_retry_budgets_are_separate...: assertion `left != right` failed  left: 2  right: 2
     leg3 (DEPLOYED != derived(DEPLOYED): max_admission_cas_attempts 2 vs 46), leg2, leg3 halves
(5b) MAX_UPLOAD_ID_ATTEMPTS = MAX_ADMISSION_CAS_ATTEMPTS  -> leg5 red: left: 46  right: 46
```

**(N6, this round's finding) the segment-put floor removed** — leg 2 red, naming the row:

```
assertion `left == right` failed: with(D, |k| k.batch_bytes = seg - 1)
  left: Ok(())
 right: Err(BatchBytesBelowSegmentPut)
test result: FAILED. 7 passed; 1 failed
```

### Refuting my own test

* **(a) Genuine red?** In the only form this slice has. Reverting `multipart.rs` to `605b33a`
  and keeping the test: it does not compile (`run-verify.sh` RED leg, exit 77, "cannot find
  function `knob_clamps_hold`", "use of undeclared type `KnobSet`" …). The seven negations
  above each turn the discriminator red with everything else of the fix in place — including
  the one for this round's finding (N6).
* **(b) Production path?** Yes. Every assertion calls the real `wyrd_core::multipart` symbols
  (`knob_clamps_hold`, the constants, `max_chunkref_bytes_for`, `value_chunk_capacity`,
  `max_part_bytes`, `admission_backoff_millis`) and pre-existing production code (`slot_key`,
  `part_key`, `decode_slot_record`, `metadata::seg_key`, `metadata::encode`). The test's own
  arithmetic (`u_ref`, `derived`, `widest_chunkref_bytes`, `slot_pin_bytes`, `seg_put_bytes`)
  is an independent oracle compared **against** production, never a stand-in for it.
* **(c) Fixture includes the fault?** Yes. Each leg-2 row is a set that actually violates its
  clamp by one unit, paired with a set exactly on the bound that must pass — including a budget
  one byte under the new segment-put floor, a fence one pin over a floor-sized budget, a wider
  and an unsupported scheme, both `U_ref` arms and the `SCAN_CAP/2` term.

### Gates and hooks run here

* `cargo fmt -p wyrd-core -- --check`: clean. `cargo clippy -p wyrd-core --all-targets`: clean.
  `typos` on both files: clean.
* `run-verify.sh` (C4-verify): GREEN 8/8; RED `UNVERIFIABLE` (77) as pre-declared.
* `scripts/mutants-in-diff` (C5) on the final patch: **94 mutants — 61 caught, 0 missed, 33
  unviable, 0 timeouts.** The unviable ones are body replacements that leave a parameter unused
  (an error under the workspace's `warnings = "deny"`) or that break a compile-time derivation.
* `./engine/xtask.sh ci` (C4-ci) on the final tree, under a 90-minute `timeout`: **`xtask ci:
  all checks passed`** (exit 0) — typos, docs, guards, fmt, clippy, build, the whole test suite
  (including all 18 `multipart_budget_admission` tests, which pin `Budget`'s derivations and
  G1–G8 across the `const fn` change, the 8 new tests, and `custodian_gc`, which timed out last
  round's gate), machete, `cargo deny` (advisories included), conformance, statics, DST.
* `run-verify.sh` re-run on the final `patch.diff` (after the last doc-only edits): GREEN 8/8,
  RED `UNVERIFIABLE` (77, "cannot find type `KnobSet`").

## 7. Size

Counted over the patch's added lines, excluding blanks, comments, attributes, punctuation-only
lines and chain continuations (the method the v2 review used; my script gave 583 on v2's patch
where the review said 587): **185 production + 203 test = 388**, under the brief's 400. v2 was
270 + 313.

## 8. External dependencies and housekeeping

No missing dependency: base toolchain, `typos`, `cargo-mutants`, and the gate runners all ran.

Created under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_655`), all named
`pdca-builder-655-*`: `-probe` (two `rustc` const-fn probes), `-logs` (every runner log quoted
above), and seven `-neg*` scratch bundles holding one negated `patch.diff` each. The runner also
used its own lane-0 verify worktree (`../wyrd-verify-l0`), and cargo-mutants left its gitignored
`mutants.out/` in the worktree. Nothing was deleted, per the harness's filesystem rule. In the
worktree the new test file is marked intent-to-add (`git add -N`) so `git diff HEAD` includes it
in `patch.diff`.
