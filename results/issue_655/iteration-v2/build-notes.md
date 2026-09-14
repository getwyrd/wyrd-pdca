# Build notes — issue 655 (multipart knob constants + derivations), iteration 2

Base: `origin/main` @ `605b33a` (the #691–#693 chain merged; the worktree HEAD equals
`origin/main`). Two files, as the brief allows: `crates/core/src/multipart.rs` (modified) and
`crates/core/tests/multipart_knobs.rs` (new). `metadata.rs`, `lib.rs`, docs and manifests are
untouched.

## 1. What changed, and where (line numbers are the patched file's)

`crates/core/src/multipart.rs`

| Change | Lines |
|---|---|
| Module header: one paragraph naming section 14 | `:21-25` |
| Import `MAX_ROOT_SEGMENTS`, `MAX_ROOT_VALUE_BYTES`, `MAX_VALUE_BYTES` from `metadata` (consumed, never re-spelled) | `:131-134` |
| `Budget::inflight_owned_refs`, `u_ref_exact`, `u_ref`, `max_sessions`, `checked_rules` made `const fn` — same logic; `u128::from` → `as u128` (trait calls are not allowed in `const fn`), `.min()` → `if`, `try_from().expect()` → an exactness `assert!` with the same message | `:1598`, `:1627`, `:1647`, `:1674`, `:1705` |
| `checked_rules` doc: the stale "those constants have no definition on this base" replaced by a pointer to `knob_clamps_hold` | `:1687-1703` |
| Section 14: the knob constants, `KnobSet`, `KnobClamp`, `knob_clamps_hold`, `admission_backoff_millis` | `:4357-5045` |

Section 14 items: `SIZING_SCHEME` `:4384`, `MAX_CHUNKREF_BYTES` `:4401`,
`max_chunkref_bytes_for` `:4407`, `value_chunk_capacity` `:4428`, `VALUE_CHUNK_CAPACITY`
`:4437`, `MAX_MAP_CHUNKS`/`MAX_SEG_CHUNKS`/`MAX_PART_CHUNKS` `:4446-4459`, `max_part_bytes`
`:4466`, `MAX_PARTS_PER_SESSION` `:4475`, `MAX_INFLIGHT_PARTS` `:4493`, `MAX_STAGED_CHUNKS`
`:4504`, `MAX_BATCH_BYTES` `:4524`, `MAX_BATCH_OPS` `:4548`, `W_REF` `:4591`,
`SHIPPED_PROFILE` `:4597`, `U_REF` `:4622`, `MAX_SESSIONS` `:4629`, `MAX_OWNED_FLEET` `:4637`,
`R_PUBLISH` `:4653`, `MAX_COMPLETE_ATTEMPTS` `:4661`, `MAX_UPLOAD_ID_ATTEMPTS` `:4669`,
`MAX_ADMISSION_CAS_ATTEMPTS` `:4683`, backoff base/cap `:4687-4691`,
`admission_backoff_millis` `:4709`, `KnobSet` `:4729` (`DEPLOYED` `:4766`), `KnobClamp`
`:4801`, `knob_clamps_hold` `:4933`. Compile-time ties: `:4441`, `:4507`, `:4528`, `:4605`,
`:4639`, `:4693`.

`crates/core/tests/multipart_knobs.rs` — 8 tests: leg 1 `:142`, leg 2 `:157`, leg 3 `:432`,
`:477`, `:519`, leg 4 `:537`, leg 5 `:583`, `:616`.

## 2. The carry-forward, item by item

| Carry-forward finding | What this build does |
|---|---|
| **T4 review, 10 blocking** — zero capacities accepted | `ValueCeiling` checks `chunks == 0 \|\| chunks > capacity` for all three one-value caps; `MAX_PART_CHUNKS == 0` is also G1. Leg-2 rows for each zero. |
| — `b_bytes` never bounded by the envelope | `BatchBytesAboveHalfEnvelope` (`B_bytes > E_tx/2`); the shipped value sits exactly on it, and a row one past it is refused. The time-axis twin `BatchOpsAboveHalfDeadline` was added for the same reason (see §4.4). |
| — `max_sessions` / `max_owned_fleet` never checked against their formulas | `SessionsNotDerived` and `OwnedFleetNotDerived` (equality with the derivation), checked after the bounds so an over-high value is named by the bound it crosses. Rows for 0, one short and (fleet) one over. |
| — `W_ref >= U_ref` not required | G7, through `Budget::checked_rules` itself (`KnobClamp::Profile(RecordError::BudgetBelowFootprint{..})`). |
| **C5 Causal adequacy** — `KnobSet::u_ref` duplicated `Budget::u_ref_exact` | One authority: `Budget`'s derivations became `const fn` and are the only spelling. `U_REF`/`MAX_SESSIONS`/`MAX_OWNED_FLEET` are evaluated **by** them on a private `SHIPPED_PROFILE: Budget`, and `knob_clamps_hold` builds the set's candidate profile and runs `checked_rules`, `u_ref`, `max_sessions` on it. G1–G7 are therefore also shared code, not re-spelled (`KnobClamp::Profile(RecordError)`). The shipped profile is held to G1–G7 at compile time (`:4605`). |
| **T2 Shape** — derived values were raw fields, no constant surface | `pub const U_REF`, `MAX_SESSIONS`, `MAX_OWNED_FLEET`, `MAX_STAGED_CHUNKS`, `VALUE_CHUNK_CAPACITY` are named constants computed from their formulas; the `KnobSet` fields that carry derived values are checked for equality with the derivation. |
| **T5 Judgment** — range test used stale literals | Leg 3 now measures `b_ref` on the codec and ties the shipped constant to it; evaluates the **production** rule at 0016's own `b_ref` figures (131 → 381, 302 → 165); and asserts the shipped measurement is at least 0016's widest figure, so the shipped caps sit at or below the range's conservative end. The remaining gap (158 < 165) is the human's C1 call, below. |
| — `U_ref` only exercised on the deployed branch | Leg 3 drives the raw arm through production with 0016's own small-part example (`0016:2847`: `MAX_PART_CHUNKS = 5` ⇒ `U_ref = 50,080` ⇒ 79 sessions): production accepts 79 and, handed 78, returns `SessionsNotDerived { derived: 79 }`. The `SCAN_CAP/2` term is driven too. |
| — "growth" assertion rechecked the floor | Leg 5 checks, for every jitter in a 70-value sweep, that the delay never shrinks from attempt n to n+1; that the window's top (jitter `u64::MAX`) grows strictly until the cap; that both window ends are reached exactly; and that more than two distinct delays occur per attempt. The backoff was redesigned so this holds (jitter is scaled into the window, not reduced modulo it). |
| **C5 mutants, 21 missed** | Now 2 missed of 125 (94 caught, 29 unviable), both equivalent mutants (§6). |
| **T3 B_ops = 1,000 uncalibrated** | Still uncalibrated — no backend here — but no longer the number 0016 names as able to blow the deadline. Now derived: `⌊(5 s / 2) / 5 ms⌋ = 500`, with the 5 ms assumption a named constant #625 must replace. **NEEDS-HUMAN** (§5). |
| **C1 / C2 / C4 / fitness items deferred to sign-off** | Not resolvable in code; restated in §5 with what the build now shows. |

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
| `MAX_BATCH_BYTES` (`B_bytes`) | 5,000,000 | `E_tx/2` |
| `MAX_BATCH_OPS` (`B_ops`) | 500 | ⌊(5,000 ms / 2) / 5 ms⌋ — **5 ms assumed** |
| `W_REF` | 4,000,000 | 0016's worked figure (`0016:2847`) |
| `U_REF` (derived) | 85,952 | min(raw 1,582,528, ceiling 80,896 + 2 × 16 × 158) — ceiling arm |
| `MAX_SESSIONS` (derived) | 46 | min(⌊4,000,000 / 85,952⌋, 524,288) |
| `MAX_OWNED_FLEET` (derived) | 116,288 | 46 × 16 × 158 ≤ 2,000,000 |
| `R_PUBLISH` | 3 | `[1, small]` |
| `MAX_COMPLETE_ATTEMPTS` | 3 | `[1, small]` |
| `MAX_UPLOAD_ID_ATTEMPTS` | 2 | one re-mint after a 2^-128 collision |
| `MAX_ADMISSION_CAS_ATTEMPTS` (derived) | 46 | = `MAX_SESSIONS` (§4.5) |
| backoff base / cap | 2 ms / 50 ms | windows [2,4], [2,8], [2,16], [2,32], then [2,50] |
| per-slot fence pin (measured, private) | 199 B | 44 B key + 155 B widest slot value |
| fixed ops: part commit / idle fence (private) | 8 / 6 | counted from the batch inventory (`0016:659`, `:664`, `:675`) |

Worst-case create wait with the whole contention budget spent: 46 × 50 ms ≈ 2.3 s.

## 4. Decisions and what I ruled out

### 4.1 `b_ref` is 315, and last round's explanation of why was wrong

Last iteration said 0016's "~302 B" undercounted a `u128` chunk id. It did not. Recomputing
0016's two figures: `{"id":<39 digits>,"scheme":{"ReedSolomon":{"k":6,"m":3}},"len":<L>,
"placement":[<9 ids>]}` gives exactly 131 B with one-digit D-server ids and exactly 302 B
with twenty-digit ids **when `len` has seven digits** (a 1 MiB chunk). 0016 already counted
the 39-digit id. The 13 extra bytes are `len` rendered at `u64::MAX`. The doc comment
(`:4386-4400`) now says that.

I kept the type-maximum `len`, because (a) nothing bounds `len` — `chunk_size` is a
deployment knob and `chunk_size_max` exists nowhere in code; (b) the in-tree peer
`segmented_map_record.rs` widens every `u64` to 20 digits for exactly this kind of bound;
(c) a narrower `len` errs in the unsafe direction. Ruled out: bounding `len` to reach 0016's
range. Cost of that route: a `len` of 8 digits gives 303 B → 165 (in range), but 9 digits
(0016's own 102 MiB chunk example) gives 304 B → 164, still out of range — and it needs a
chunk-size ceiling constant nobody enforces. **C1 stays with the human** (§5).

### 4.2 One budget authority: const-ify `Budget`'s methods, not new free functions

Chosen: make the five `Budget` methods `const fn` and evaluate the shipped constants on a
private `const SHIPPED_PROFILE: Budget`. Section-5 diff: 4 bodies + 2 casts, 21 lines
removed / ~30 added. Considered: three free `const fn`s (`u_ref_of`, `max_sessions_of`,
`owned_refs_of`) that both `Budget` and the knobs call. Same one-spelling outcome, but it
moves the u128-width rationale (~25 doc lines) off the methods that decode untrusted bytes,
and leaves the formula on a function no record type owns. Option B keeps the derivation on
the type 0016:348 names and puts no second copy anywhere.

`SHIPPED_PROFILE` bypasses `TryFrom<BudgetWire>`, so it is checked by `checked_rules` at
compile time instead (`:4605`, `std::mem::forget` because a `const` may not run
`RecordError`'s destructor). Budget's "no invalid `Budget` exists" invariant holds.

### 4.3 The aggregate type is `KnobSet`, not `Budget`

`Budget` is taken by the persisted `mpuctl` profile (sealed, 5 fields, #715). Leg 2 needs
constructible invalid sets, so the knob aggregate has public fields and `knob_clamps_hold` as
its validator. Its five profile fields are validated by `Budget`'s own rules via a private
candidate (`KnobSet::profile`, `:4787`). No public writer-side `Budget` constructor was added;
`Budget`'s doc reserves that for #656–#659.

### 4.4 Clamps stricter than the table's shorthand, never looser

* **Part commit vs `B_ops`**: the table says `MAX_PART_CHUNKS ≤ B_ops`; the batch inventory
  (`0016:659`, `:675`) adds 4 preconditions + 4 mutations, so the clamp is
  `MAX_PART_CHUNKS + 8 ≤ B_ops`. The brief's literal row (`B_ops = MAX_PART_CHUNKS - 1`) is
  refused, and so is `B_ops = MAX_PART_CHUNKS + 7`.
* **Idle fence vs `B_ops`**: `MAX_INFLIGHT_PARTS + 6 ≤ B_ops` (the fence's 6 fixed ops exceed
  the terminal delete's 5). The literal row `MAX_INFLIGHT_PARTS = B_ops + 1` is refused too.
* **Idle fence vs `B_bytes`**: the table's form, `MAX_INFLIGHT_PARTS × bytes-per-slot ≤
  E_tx/2`, with bytes-per-slot **measured** (199 B = key + widest value the pin carries). The
  fence's three fixed puts (≤ 3 × V) are charged to the other half; a compile-time check shows
  they fit (`:4528`). I did not add them to the clamp: that deviates from 0016:1471's formula,
  and the other half exists for exactly this, as `V/2` does for a record's own fields.
* **`B_ops` ceiling** (`BatchOpsAboveHalfDeadline`): `B_ops × 5 ms ≤ 2.5 s`. Added because
  the last mutation run showed the `/2` in `MAX_BATCH_OPS` was unpinned, and it is the time
  twin of the byte bound the reviewers required.
* **`SchemeUnsupported`**: a set sized for an RS scheme `erasure::supported` rejects is refused
  (`rs(6,0)` parses at the CLI). Otherwise `knob_clamps_hold` would size caps against chunks no
  writer can produce — "silent success on an unsupported entry".

Not enforced, deliberately: an upper bound on `R_PUBLISH` / `MAX_COMPLETE_ATTEMPTS`. 0016 says
`[1, small]` without a number, and any finite value terminates; inventing a number would be a
second authority for something 0016 left open.

### 4.5 The contention budget is derived: `MAX_ADMISSION_CAS_ATTEMPTS = MAX_SESSIONS`

Among creates, a lost `mpuctl` CAS means another create was admitted. After `MAX_SESSIONS`
losses the ledger is full, so the refusal is the capacity bound. That is the brief's "a refusal
must be a real bound, not a lost race" as arithmetic. `knob_clamps_hold` enforces
`attempts ≥ max_sessions` (`AdmissionCasBelowSessions`). Scope stated in the doc: a CAS lost to
a teardown's decrement is churn, and a refusal under churn is backpressure. Trade-off for
sign-off: the worst-case create wait scales with `MAX_SESSIONS` (2.3 s now; at the
`SCAN_CAP/2` extreme it would be hours, though only under a storm that size). A cheaper chosen
number (the salvage's 64) was ruled out because it silently stops covering the room once
`W_ref` grows past 64 sessions.

### 4.6 Other choices

* `KnobClamp` has no `Display`. Its `Debug` output names the clamp and the numbers on both
  sides (for example `PartCommitOverOps { ops: 166, batch_ops: 165 }`). A `Display` +
  `std::error::Error` pair is about 40 lines; #508/#625 can add it where the message reaches
  an operator.
* `knob_clamps_hold` reports the **first** violation in a fixed order, the way
  `Budget::checked_rules` does. Returning every violation was ruled out: a derived-value row
  such as `MAX_SESSIONS = derived + 1` breaks both its bound and its equality by necessity, so
  "exactly one violation" cannot hold for it under either design.
* `SIZING_SCHEME` restates server's default because `core` cannot import `server`. If the
  default changes, a deployment on the new scheme gets `ValueCeiling` from `knob_clamps_hold`
  once #508 wires it, but nothing ties the two constants today. A test in `crates/server`
  would, and it is a third file, so it is out of this slice.
* `docs/principles.md` §5 C-1 is cited the way the target already cites it (e.g.
  `multipart.rs:85`, `metadata.rs:2246`), even though the file lives in the PDCA project.

## 5. NEEDS-HUMAN at sign-off

1. **C1 — 158 vs 0016's "165–381".** The measured worst-case chunk ref is 315 B, so the shipped
   caps are 158, below 0016's range. The test pins the production rule to 0016's endpoints at
   0016's own `b_ref` figures and pins the shipped measurement to the codec; it does **not**
   assert 158 ∈ [165, 381], because that is false. Options: accept 158 (conservative); bound
   `len` with an enforced `chunk_size_max` (#508's knob) and re-measure; or amend 0016's prose.
2. **T3 — `B_ops = 500` rests on an assumed 5 ms per sequential in-transaction operation.**
   Not calibrated on any backend. #625 must measure it (`SLOWEST_OP_MILLIS`, `:4534`).
3. **Open question (a) — `W_REF` is a compile-time constant (4,000,000).** The doc says how
   #625 turns it into a deployment input without re-deriving anything.
4. **C2 / C4 — red is criterion absence, as the brief pre-declared.** `run-verify.sh` on this
   patch: GREEN 8/8 on a clean `origin/main` + patch; RED leg `UNVERIFIABLE` (exit 77) — the
   test cannot compile without the symbols the patch adds. §6 has the negations that stand in
   for the red.
5. **Contention budget scaling** (§4.5) — a judgment call worth a look.

## 6. Evidence

### The three required negations (binding), run on the final code, each reverted after

**(2) `knob_clamps_hold` returns `Ok(())` unconditionally** (an `if true { return Ok(()); }`
first line) → 3 of 8 red; leg 1 stays green, as the brief predicts for a vacuous check:

```
test leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it ... FAILED
test leg3_u_ref_and_the_session_limit_follow_the_formulas_under_either_arm ... FAILED
test leg4_every_capacity_fits_its_key_space_in_byte_order_at_the_cap ... FAILED
  left: Ok(())
 right: Err(SchemeUnsupported { k: 6, m: 0 })
  left: Ok(())
 right: Err(SessionsNotDerived { stored: 78, derived: 79 })
  left: Ok(())
 right: Err(Profile(PartsPerSessionUnaddressable { max_parts_per_session: 1000000 }))
test result: FAILED. 5 passed; 3 failed
```

**(3) One budget sized against the whole `MAX_VALUE_BYTES` instead of half**
(`value_chunk_capacity` divides `MAX_VALUE_BYTES`) — caught at three layers:

```
(3a) error[E0080]: evaluation panicked: assertion failed: VALUE_CHUNK_CAPACITY as usize * MAX_CHUNKREF_BYTES <= MAX_ROOT_VALUE_BYTES
     error: could not compile `wyrd-core` (lib)
(3b, production tie removed) error[E0080]: evaluation panicked: assertion failed: VALUE_CHUNK_CAPACITY <= 165
     error: could not compile `wyrd-core` (test "multipart_knobs")
(3c, the test's const check also disabled — runtime assertions only)
test leg3_the_chunk_caps_are_half_a_value_over_the_measured_worst_chunk_ref ... FAILED
  left: 317
 right: 158
test leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it ... FAILED
  left: Err(ValueCeiling { knob: "MAX_MAP_CHUNKS", chunks: 318, capacity: 317 })
 right: Err(ValueCeiling { knob: "MAX_MAP_CHUNKS", chunks: 318, capacity: 158 })
test result: FAILED. 6 passed; 2 failed
```

**(5) The two retry budgets collapsed into one constant.**
(5a) `MAX_ADMISSION_CAS_ATTEMPTS = MAX_UPLOAD_ID_ATTEMPTS` → 4 of 8 red:

```
test leg1_the_shipped_set_passes_every_clamp ... FAILED
  left: Err(AdmissionCasBelowSessions { attempts: 2, max_sessions: 46 })
test leg3_u_ref_and_the_session_limit_follow_the_formulas_under_either_arm ... FAILED
  left: 2
 right: 46
test leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it ... FAILED
test leg5_the_two_retry_budgets_are_separate_and_contention_outlasts_the_room ... FAILED
  assertion `left != right` failed  left: 2  right: 2
test result: FAILED. 4 passed; 4 failed
```

(5b) the other direction, `MAX_UPLOAD_ID_ATTEMPTS = MAX_ADMISSION_CAS_ATTEMPTS` → leg 5 red
(`left: 46 right: 46`), 7 passed / 1 failed.

### Refuting my own test

* **(a) Genuine red?** Yes, in the only form this slice has. With `multipart.rs` reverted to
  `605b33a` and the test kept, it fails to compile (`E0432: unresolved imports ...
  knob_clamps_hold ... KnobSet ...`), confirmed again by `run-verify.sh` (exit 77). The
  negations above show each binding leg goes red when the production code it covers is broken.
* **(b) Production path?** Yes. Every assertion calls the real `wyrd_core::multipart` symbols
  (`knob_clamps_hold`, the constants, `max_chunkref_bytes_for`, `value_chunk_capacity`,
  `max_part_bytes`, `admission_backoff_millis`) and pre-existing production code (`slot_key`,
  `part_key`, their parsers, `decode_slot_record`, `metadata::encode`). The test's own
  arithmetic (`u_ref`, `derived`, `widest_chunkref_bytes`, `slot_pin_bytes`) is an
  independent oracle compared **against** production, never a stand-in for it.
* **(c) Fixture includes the fault?** Yes. Each leg-2 row is a set that actually violates its
  clamp by one unit, paired with a set exactly on the bound that must pass, so both the check
  and its boundary are exercised. The raw `U_ref` arm, the `SCAN_CAP/2` term, a wider scheme
  and an unsupported scheme are all driven through production.

### Gates and hooks run here

* `cargo fmt -p wyrd-core -- --check`: clean. `cargo clippy -p wyrd-core --all-targets`:
  clean. `typos` on both files: clean.
* **`./engine/xtask.sh ci` (the C4-ci gate) on the final tree: `xtask ci: all checks passed`**
  — typos, docs lint/render, gitlink and unsafe guards, fmt, clippy, build, test, machete,
  cargo-deny, conformance vectors, statics, DST clippy + test.
* `run-verify.sh` (C4-verify): GREEN 8/8; RED `UNVERIFIABLE` (exit 77) as pre-declared.
* `scripts/mutants-in-diff` (C5): 125 mutants — 94 caught, 29 unviable, **2 missed**, both
  equivalent: `raw < ceiling` → `<=` in `Budget::u_ref_exact` (`:1632`) and
  `quotient < SCAN_HALF` → `<=` in `Budget::max_sessions` (`:1676`). Each is a `min` written
  as `if` because `Ord::min` is not callable in a `const fn` on 1.96 (probed); at equality both
  branches return the same number, so no test can tell them apart. Iteration 1 had 21 missed.
* `cargo doc -p wyrd-core --no-deps` (not part of `xtask ci`): fails with 20 link errors, all
  in `erasure.rs`, `metadata.rs`, `read.rs` and `write.rs` — files this patch does not touch.
  None is in `multipart.rs`, with or without the patch, so every new intra-doc link resolves.
  Out of scope here; worth an issue.
* All 22 `wyrd-core` test targets pass (lib, integration tests, doc-tests), including the 18 `multipart_budget_admission` tests
  that pin `Budget`'s derivations and G1–G8 — the `const fn` change kept their behaviour.

## 7. Size

Non-blank, non-comment lines: 345 production (15 removed) + 471 test. Without rustfmt's
one-field-per-line struct literals and bare closing brackets: about 205 + 210. The brief's
budget is ≤ 400 "non-mechanical"; the second count is close to it, the first is not. Most of
the test is the leg-2 table (32 refused sets + 8 on-bound sets), already rewritten once with a
`with(base, |k| …)` helper, which took the test from 592 to 471 lines.

## 8. External dependencies and housekeeping

No missing dependency: base toolchain, `typos`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
and the docs renderer all ran. Scratch files I created, all under `$PDCA_SCRATCH` and named
`pdca-builder-655-*`: file backups used for the negation runs, gate logs, and a `rustc` probe
directory. Not deleted, per the harness's filesystem rule. In the worktree, the new test file
is marked intent-to-add (`git add -N`) so `git diff HEAD` includes it in `patch.diff`.
