# Build notes — #715 multipart-budget-admission (attempt v4, the re-authored brief)

**Withheld from the reviewer; written for the human at sign-off.**

Target branch `getwyrd/wyrd @ main`, resolved to `origin/main = 92e1b4b` (the merge of #718).
The brief's base `9dbcd72` is its parent-of-parent; `crates/core/src/multipart.rs` is
**byte-identical** on both (854 lines, `git diff 9dbcd72 92e1b4b` touches only `metadata.rs`,
`custodian/`), so every `multipart.rs` citation in the brief resolves unchanged. `metadata.rs`
citations are quoted **against `92e1b4b`** below (it gained +46 lines from #718, so the
brief's base-relative `metadata.rs` numbers sit ~28–46 lines earlier).

---

## 0. READ FIRST — an orphaned second builder is writing this bundle

While I worked, a **second `claude --agent builder` process (PID 1514337)** was running against
the *same* worktree (`--add-dir /home/eddie/wyrd/wyrd.pdca-wt-l1`) and the *same* bundle. Its
parent is `systemd --user` (PID 12256) — i.e. it is an **orphan of a driver run that has since
exited**; my own driver is alive (`wyrd-pdca flow 711 715 716 717`, PID 1525711 → me, 1525804).

Observed, not inferred: at 00:45 it had `crates/core/src/multipart.rs` in `$PDCA_WORKTREE` at
992 lines mid-edit (the base is 854), the file grew to 1277 lines *between two of my reads*, and
at 01:03 it **overwrote `results/issue_715/patch.diff`** seconds after I wrote mine (49 453 B /
1 023 lines, with a test file ~500 lines long — not my 48 172 B / 970 lines).

Two consequences, both deliberate on my side:

1. **I did not edit `$PDCA_WORKTREE`.** All target-source edits were made in a private git
   worktree cut from the same commit
   (`$PDCA_SCRATCH/pdca-builder-715-tree`, detached at `92e1b4b`), so my patch could not be
   interleaved with the orphan's. `pdca.toml` states the lane is reconstructed as base +
   `patch.diff` before every gating read (#296 — "the lane is a warm checkout cache, not a
   trusted content cache"), so `patch.diff` — not the lane's file state — is the authoritative
   artifact, and nothing is lost by this. Citations still resolve in `$PDCA_WORKTREE` because
   both trees are the same commit.
2. **I verified my own patch in an isolated bundle copy** (`$PDCA_SCRATCH/pdca-builder-715-bundle`,
   brief + my patch.diff) with `WYRD_VERIFY`/`PDCA_LANE` overridden, so the C4-verify evidence in
   §5 is evidence about *my* patch, not the orphan's.

**Check before you sign off** that `results/issue_715/patch.diff` still hashes to the value in
§8 — if it does not, the orphan overwrote it *after* I finished and the bundle's patch is not the
one these notes describe. I did not kill the other process: it is another agent's work and killing
it is the human's call, not mine.

```
NEEDS-HUMAN external dependency: exclusive ownership of the bundle + lane worktree — an orphaned
builder process (PID 1514337, reparented to systemd, from a driver run that already exited) was
writing results/issue_715/patch.diff and $PDCA_WORKTREE concurrently; confirm the shipped
patch.diff matches the sha256 in build-notes §8 before accepting, and reap the orphan.
```

```toml
[[doctor.checks]]
id    = "orphaned leaf process"   # the token Plan should have put in `External dependencies`
cmd   = "test -z \"$(ps -eo pid=,ppid=,args= | awk '/--agent (builder|reviewer|adversary|planner)/ && !/awk/ {print $2}' | sort -u | xargs -r -I{} sh -c 'ps -o comm= -p {} 2>/dev/null | grep -qE \"^(systemd|init)$\" && echo {}')\""
hint  = "A leaf from a dead driver run is still alive and will overwrite the live run's bundle. Find it with `ps -ef | grep -- '--agent builder'`, confirm its parent is systemd/init, and kill it before starting a flow."
level = "MISSING"
```

---

## 1. What this patch is

Two files, exactly as the brief's size budget names them:

| File | Change |
|---|---|
| `crates/core/src/multipart.rs` | +520 / −10 (854 → 1 354 lines) |
| `crates/core/tests/multipart_budget_admission.rs` | new, 392 lines |

The module extension, by anchor **in the patched file**:

| Item | Line | What |
|---|---|---|
| section header | `multipart.rs:997` | `5. The admission ledger — the mpuctl record VALUE` |
| `SCAN_HALF` | `:1012` | `SCAN_CAP/2`, private |
| `u_ref_of` | `:1037` | `U_ref`, exact in `u128` (`0016:1469`) |
| `Budget` | `:1079` | the five-field profile, private fields |
| `Budget::u_ref` / `::max_sessions` | `:1119` / `:1143` | the two derivations (`0016:1469`, `:1470`) |
| `BudgetWire` | `:1153` | closed wire shape |
| `TryFrom<BudgetWire> for Budget` | `:1180` | **G1–G7**, at `:1184`, `:1189`, `:1193`, `:1199`, `:1206`, `:1214`, `:1221` |
| `AdmissionRecord` | `:1271` | the `mpuctl` singleton |
| `AdmissionRecordWire` | `:1301` | closed wire shape, `profile: BudgetWire` (unvalidated — see §2) |
| `TryFrom<AdmissionRecordWire>` | `:1311` | **G8** at `:1317` |
| `decode_admission_record` (**S2**) | `:1347` | the typed surface |
| `RecordError` new variants | `:130-207` | one per guard + `MalformedRecordValue` |
| doc corrections | `:5-20`, `:63-65`, `:87-92` | header (the `encode_record` forward reference **withdrawn**), the living-architecture block, the `RecordError` doc sentence |

**S1** is `metadata::decode::<AdmissionRecord>` — serde's `#[serde(try_from = …)]` derive
(`multipart.rs:1270`), i.e. the same `TryFrom` above, so both surfaces run one validation.

## 2. The three design decisions worth arguing about

**(a) Why the wire struct nests `BudgetWire`, not `Budget`.** This is the brief's named trap.
`decode_segment_record` recovers its type with `err.downcast::<ChunkMapError>()` after
`metadata::decode` (`metadata.rs:2536-2547`), and that downcast is **dead code** — by then serde's
`Error::custom` has already turned the domain error into a `serde_json::Error`
(`metadata.rs:1244` is the funnel for `SegmentRecord`). Had I put a validating `Budget` inside
`AdmissionRecordWire`, G1–G7 would have fired *inside* `serde_json::from_slice` and S2 could only
have reported a string — the twelve demonstrations below need G4's rejection to be distinguishable
from G7's. So the wire type is raw all the way down, `metadata::decode::<AdmissionRecordWire>`
does the parse, and the rules run **after** it in plain Rust where `RecordError` survives
(`multipart.rs:1347-1355`). The brief's "mirror the base's structure, not that dead branch" is
exactly what this is.

**(b) No constructor at all — the type is decode-only.** The brief forbids a
configuration-validation constructor and permits at most "a plain data constructor over the same
eight record guards". I first wrote `Budget::new` / `AdmissionRecord::new` (mirroring
`UploadId::new`, `multipart.rs:342`) with decode delegating to them; that cost **18 more semantic
lines** (267 vs 249 in the module: a 7-line `pub fn new(` signature + an 8-line delegating
`try_from` for `Budget`, and 6 more for `AdmissionRecord`) and bought nothing this slice can use —
there is no writer until #656–#659, and a public `new` is precisely the surface a reviewer would
have to re-litigate against "ships no constructor". Dropped. The rules live in the one conversion
every surface funnels through, which is what makes "a value that decodes cannot be malformed"
true by construction. #656–#659 add the writer-side constructor with the writer.

**(c) `u128` for `U_ref`, `u64` for the G5 product, and no saturation anywhere.** Both `U_ref`
terms are computed from values the decoder has **not yet judged**, so both can be driven to the
field maxima. `min` makes the overflowing term irrelevant in the accept case (P-arith-accept), so
saturating would have been wrong in the other direction: a saturated `U_ref` at `w_ref = u64::MAX`
derives `⌊W_ref/U_ref⌋ = 1` and admits a session whose true footprint is the entire budget. In
`u128` every operand here is ≤ `2^65`, so the arithmetic is exact and cannot itself overflow. G5's
product is two `u32`s, exact in `u64` — no `u128` needed, and no wrap can defeat it.
`Budget::u_ref()` returns `u64` with an `expect` whose precondition (G7) is enforced in the only
constructor, so the panic is unreachable for every `Budget` that exists (`multipart.rs:1119-1131`).

**What I did NOT build**, on purpose: no `encode_record`/`decode_record` envelope (the brief's
adjudication (B); the module header now says so in the file, `multipart.rs:14-20`); no
`chunkref_bytes` / `WIDEST_SCHEME_BYTES` / `MAX_SEG_CHUNKS_FORMAT_MAX` / `MAX_PUBLISHABLE_CHUNKS`
(the v3 invented-constant apparatus — the salvage instruction was explicit); no `docs/` change
(leg D reversed; the in-file policy at `multipart.rs:63-72` is the specific rule and it stands,
now reading "the key **grammar** plus the admission ledger's record **shape**"); no `metadata.rs`
change (this slice only reads its constants).

## 3. The twelve demonstrations (leg → kind → negation → pasted output)

Method: negate **one** leg in `crates/core/src/multipart.rs`, run
`cargo test -p wyrd-core --test multipart_budget_admission`, paste, restore from a pristine copy.
Driven by `$PDCA_SCRATCH/demos.py`; raw output in `$PDCA_SCRATCH/demo-output.txt`. The tree was
byte-compared against the pristine copy afterwards (`RESTORED-OK`) and re-run green.

| Leg | Kind | Negation | Red it produced |
|---|---|---|---|
| **G1** | 2 (totality — **not** isolable) | drop `wire.max_part_chunks == 0` | `g1_… FAILED`, `panicked at crates/core/src/multipart.rs:1144:9: attempt to divide by zero` — removing G1 breaks **totality**, it does not flip an assertion: the record is no longer refused before `MAX_SESSIONS`' quotient runs, and the quotient has no divisor. Exactly the shape the brief demands for G1. |
| **G2** | 1 | drop `wire.max_inflight_parts == 0` | `left: Ok(AdmissionRecord { count: 0, max_sessions: 1, profile: Budget { w_ref: 1, max_part_chunks: 1, max_parts_per_session: 1, max_inflight_parts: 0, max_staged_chunks: 1 } })` / `right: Err(MaxInflightPartsZero)` — 15 passed, 1 failed |
| **G3** | 1 | drop `> MAX_PART_NUMBER` | `left: Ok(… max_parts_per_session: 1000000 …)` / `right: Err(MaxPartsPerSessionUnaddressable { max_parts_per_session: 1000000 })` — 15 passed, 1 failed |
| **G4** | 1 | drop `mip > mpps` | `left: Ok(… max_sessions: 33, w_ref: 1000, mpc: 1, mpps: 10, mip: 20, msc: 100 …)` / `right: Err(InflightPartsExceedSessionParts { max_inflight_parts: 20, max_parts_per_session: 10 })` — 15 passed, 1 failed |
| **G5** | 1 | drop `mip × mpc > SCAN_HALF` | `left: Ok(… mpps: 524289, mip: 524289 …)` / `right: Err(OwnedStagingRangeUnscannable { max_inflight_parts: 524289, max_part_chunks: 1 })` — 14 passed, **2** failed: `p_arith_reject` also reds (`left: Err(MaxSessionsNotDerived { stored: 18446744073709551615, derived: 2147 })`). Expected and pre-authorised: P-arith is explicitly **not** an isolating leg (a maximal record cannot violate exactly one rule). The G5 *witness* violates only G5. |
| **G6** | 1 | drop `msc < mpc` | `left: Ok(… max_part_chunks: 165, max_staged_chunks: 164 …)` / `right: Err(StagedChunksBelowPartChunks { max_staged_chunks: 164, max_part_chunks: 165 })` — 15 passed, 1 failed |
| **G7** | 1 | drop `w_ref < u_ref` (+ `_u_ref` binding, see note) | `left: Ok(… w_ref: 1, max_sessions: 0 …)` / `right: Err(ReferenceBudgetBelowFootprint { w_ref: 1, u_ref: 2 })` — 15 passed, 1 failed |
| **G8** | 1 | drop `wire.max_sessions != derived` (+ `_derived`) | `left: Ok(AdmissionRecord { count: 3, max_sessions: 5, … })` / `right: Err(MaxSessionsNotDerived { stored: 5, derived: 4 })` — 15 passed, 1 failed |
| **P1** | 3 (inverted) | **add** `count > max_sessions ⇒ reject` | `p1_… FAILED: occupancy above the cap is not a decode error: MaxSessionsNotDerived { stored: 9000, derived: 4 }` — 15 passed, 1 failed |
| **P2** | 3 (inverted) | **add** the `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS` staged ceiling (512 × 381) | `p2_… FAILED: a large staged ceiling is not a decode error: StagedChunksBelowPartChunks { max_staged_chunks: 10000000, max_part_chunks: 165 }` — 13 passed, 3 failed (it also reds P3-above and P-arith-accept, whose witnesses exceed the same invented ceiling — the point of the leg) |
| **P3** | 3 (inverted) | **add** the proposal's 165–381 window on `max_part_chunks` | `p3_… FAILED: a small part-chunk cap is not a decode error: MaxPartChunksZero` — 10 passed, 6 failed (the window rejects most legal witnesses; that breadth *is* the evidence that inventing a chunk-ref width here would break the record class) |
| **P-arith** | 3 (inverted) | replace the exact `u128` derivation with the naive same-width spelling (`msc + 2·mip·mpc`, `mip × mpc` in the field's own width) | **both halves red, as the brief requires**: `p_arith_accept_… panicked at crates/core/src/multipart.rs:1044:19: attempt to add with overflow`  *(both locations are lines of the **negated** tree — the naive spelling — not of the shipped file)*; `p_arith_reject_… panicked at crates/core/src/multipart.rs:1206:22: attempt to multiply with overflow` — i.e. the reject leg **stops naming G5** (it panics instead). 14 passed, 2 failed |

*Note on G7/G8:* dropping only the `if` leaves the `let u_ref` / `let derived` binding unused, and
`[workspace.lints.rust] warnings = "deny"` (root `Cargo.toml:227`) turns that into a **compile
error** — which would have measured the lint, not the guard. The negation therefore also renames
the binding `_u_ref` / `_derived`. Nothing else changed.

**P-arith-reject — the exact-arithmetic check, and one correction to the brief.** The witness is
every numeric field at its wire maximum except the two the key space bounds
(`mpps = mip = MAX_PART_NUMBER = 999_999`, so G3 and G4 hold; `mpc = msc = u32::MAX`, so G6 holds
at equality; `count = max_sessions = w_ref = u64::MAX`). In unbounded integers:

* **G5 is genuinely violated**: `999_999 × 4_294_967_295 = 4_294_963_000_032_705 > 524_288`. My
  evaluation order reaches G5 first, so decode returns `OwnedStagingRangeUnscannable` — the test
  asserts exactly that, and the test completing is itself the "no panic, no wrap" evidence (debug
  overflow checks are on).
* **G7 in fact HOLDS here**, contrary to the brief's parenthetical "no representable `w_ref`
  reaches that `U_ref`": `U_ref = min(8_589_926_000_065_410, 8_589_930_295_032_705) =
  8_589_926_000_065_410`, and `w_ref = u64::MAX = 18_446_744_073_709_551_615 ≥ U_ref`. The brief's
  claim would be true only if `w_ref` were a `u32`; it is a `u64` (`0016:1473` sizes it from host
  RAM, so `u64` is the right width). **G8 is violated too** (`u64::MAX ≠ 2147`, the derived value).
  So the maximal record violates {G5, G8}, not {G5, G7} — and naming G5 is a verdict exact
  arithmetic agrees with, which is all the leg requires ("a variant naming a guard that exact
  arithmetic says HOLDS" is what is forbidden, and G5 does not hold).
* **P-arith-accept** is the plan-review witness promoted by the brief:
  `U_ref = min(2, 4_294_967_297) = 2`, so G1–G8 all hold and it decodes `Ok` with
  `max_staged_chunks = u32::MAX` preserved. `msc + 2 = 4_294_967_297` leaves `u32`, which is why
  the naive spelling panics on it.

## 4. Alternatives rejected, with their cost

* **A validating `Deserialize` on `Budget` nested inside `AdmissionRecordWire`** (the "obvious"
  shape). Rejected on *correctness*, not cost: every G1–G7 rejection would reach S2 as a
  `serde_json::Error` string, so seven of the twelve demonstrations could not be written at all —
  the test would have to match on message text, which the brief forbids in as many words.
* **Reusing `decode_segment_record`'s downcast shape** (`metadata.rs:2536-2547`) for S2: same
  defect — that branch is dead in the base for exactly this reason (§2a). Mirrored its *structure*
  (one `pub fn` per record class, failure attributed to the record) and not its dead arm.
* **Keeping `Budget::new` / `AdmissionRecord::new`**: +18 semantic module lines, no consumer this
  slice or its test has (§2b).
* **Saturating arithmetic instead of `u128`**: same line count (`saturating_add`/`_mul` are drop-in),
  rejected because a saturated `U_ref` is a *false* answer, not a fail-closed one (§2c).
* **`#[serde(default)]` / open wire shape** instead of `deny_unknown_fields`: rejected per the
  brief's settled decision; the two CAS shapes that punish a dropped field are both live in this
  repo today — `require(key, encode(prior))` at `metadata.rs:1794`, `:1919` (re-encoded prior:
  permanent `Conflict`) and `batch.require(key, current).put(key, encode(entry))` at
  `metadata.rs:2012` (raw bytes: silent data loss). Recorded in the `Budget` doc so the
  forward-compatibility cost is visible at the type.

## 5. How it was verified (project runner only)

* **Red→green through the project's own configured gate cmd**,
  `./engine/scripts/run-verify.sh` (`pdca.toml` `[[gates.checks]] id="C4-verify"`), against an
  isolated bundle copy and an isolated verify worktree (`$PDCA_SCRATCH/pdca-builder-715-verify`,
  `PDCA_LANE=715v`) so the orphan in §0 could not perturb it. Full log:
  `$PDCA_SCRATCH/run-verify.log`.
  * `GREEN — cargo test -p wyrd-core --test multipart_budget_admission (fix applied)` →
    `running 16 tests` … `test result: ok. 16 passed; 0 failed`.
  * `RED — (production reverted, test kept)` → **UNVERIFIABLE, exit 77**: the test cannot compile
    against the reverted base (`no variant named MaxSessionsNotDerived found for enum RecordError`,
    etc.). This is the **pre-declared** §6 item in the brief (born-at-tier posture (a)): every
    symbol under test is net-new, so there is no pre-patch tree in which the discriminator builds.
    The twelve demonstrations in §3 are what replace the flippable red, exactly as briefed.
* **Gate-shaped local checks** in the private worktree: `cargo fmt --all -- --check` (clean),
  `cargo clippy --workspace --exclude wyrd-dst --all-targets` (clean — the workspace denies
  `warnings` and `clippy::all`), `cargo test -p wyrd-core` (**all** targets green, including the
  peer `multipart_keys.rs`, 21 tests — the new `RecordError` variants break no existing matcher),
  and `typos` over both changed files (clean; `typos` is a `cargo xtask ci` step). `cargo doc`
  is **not** a gate here — it already fails on the base (`erasure.rs:2`, `metadata.rs:1371`) — but
  I kept the new docs free of private-item links anyway.
* Not run locally: the full `cargo xtask ci` (DST + conformance + deny, ~7 min+) — that is the
  driver's `C4-ci` gate and it runs over the reconstructed lane.

## 6. The three refutation questions

* **(a) Genuine red? YES**, twelve times over, each recorded in §3 with its pasted output. Every
  one of G2–G8 is *isolating*: with that single check dropped the record decodes `Ok` and only its
  own test fails (15 passed / 1 failed), which is the proof the guard is load-bearing rather than
  riding on a neighbour. G1's revert produces a **divide-by-zero panic**, and the four inverted
  legs fail their accepting assertion when the bound they deny is added. With the *whole*
  production change reverted the test does not merely fail — it does not compile (§5), which is
  the born-at-tier case the brief pre-declared, not a silent green.
* **(b) Production path? YES.** The test drives `wyrd_core::multipart::decode_admission_record`
  and `wyrd_core::metadata::decode::<AdmissionRecord>` — the two production surfaces this patch
  adds — over the production codec `wyrd_core::metadata::{encode, decode}`
  (`metadata.rs:1564-1571`). There is no mock, no stand-in decoder and no re-implementation of the
  rules in the test: the expected values are hand-computed constants (`U_ref = 18_150`,
  `max_sessions = 4`, …), never recomputed by test-side arithmetic that could drift with the code.
  `decode_both` asserts the two surfaces agree on **every** witness, so a rule enforced on one and
  missed on the other is itself a failure.
* **(c) Fixture includes the fault? YES.** Every guard witness *contains* the torn field —
  `max_inflight_parts: 0`, `max_parts_per_session: 1_000_000`, `max_staged_chunks: 164` against
  `max_part_chunks: 165`, `max_sessions: 5` against a profile deriving 4 — rather than a curated
  "healthy" record. P-arith uses the actual wire maxima (`u32::MAX`, `u64::MAX`), which is the only
  input class that can exhibit the overflow, and the unknown-field and non-JSON cases feed real
  malformed bytes. Nothing is excluded to make a leg pass.

## 7. Size accounting (against the brief's ≤ 450 semantic lines / exactly 2 files)

Files: **2**, exactly the two the brief names — no third file, so no STOP condition.
Semantic lines (non-blank, non-comment), measured on the final patch:

| | semantic | brief's estimate |
|---|---|---|
| `crates/core/src/multipart.rs` | 249 | ≈ 200 |
| `crates/core/tests/multipart_budget_admission.rs` | 226 | ≈ 250 |
| **total** | **475** | **≤ 450** |

25 over (+5.5%), and I could not find an honest way to close it: the brief's own "one distinct
`RecordError` variant per guard" costs ~29 lines of variants plus ~52 lines of `Display` arms
before a single rule is checked, and the eight rules with their typed payloads cost another ~50.
Total patch is 902 added lines against v3's 561-line overrun — the reduction the re-plan predicted
is real; what is left is the irreducible cost of the typed-variant requirement. (Line-count is
also why I dropped the two constructors, §2b: −18.)

## 8. Artifact hashes (check these at sign-off — see §0)

```
sha256  patch.diff                                    31386302ffd92e7ef3b792914210bfa0f5311a8adcdb676d1cfb8c8c73897883
        (48 172 bytes, 970 lines, 2 files:
         crates/core/src/multipart.rs, crates/core/tests/multipart_budget_admission.rs)
sha256  crates/core/tests/multipart_budget_admission.rs (the bundle's mirror of the added test)
                                                      d2872a4dd221af4f2af1df0434ee5c40fe0c66dc58fd52fd78da3cf96f3397ed
```

Byte-compared after writing: the shipped `patch.diff` is **identical** to the copy C4-verify
measured in §5 (`cmp` clean), so the GREEN/RED evidence is evidence about this exact artifact.

## 9. Scratch

Everything throwaway lived under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_715/`):
`pdca-builder-715-tree` (the private git worktree + its cargo target dir),
`pdca-builder-715-verify` (the C4-verify worktree), `pdca-builder-715-bundle` (the isolated bundle
copy), plus `demos.py`, `demo-output.txt`, `run-verify.log`, `multipart.pristine.rs`. The two git
worktrees are removed and pruned at the end of the run; the logs are left in place deliberately so
the human can re-read the twelve reds, and are attributable by their `pdca-builder-715-*` names.
