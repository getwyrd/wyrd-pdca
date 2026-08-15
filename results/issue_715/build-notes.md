# Build notes — issue #715, multipart admission ledger (`Budget` + `AdmissionRecord`)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt`, HEAD `92e1b4b` — which is
`origin/main` at this run. `crates/core/src/multipart.rs` is **byte-identical to the brief's
base `9dbcd72`** there (`git diff 9dbcd72 HEAD -- crates/core/src/multipart.rs` is empty; the
854-line file the brief describes), so every `multipart.rs:NNN` citation to *pre-existing*
code resolves the same on both. `crates/core/src/metadata.rs` **has moved** (+46 lines since
`9dbcd72`), so metadata citations below are given at their **worktree** lines with the brief's
base line in parentheses where the brief named one.

## 1. What this round changes relative to `iteration-v5` (the carry-forward)

Iteration 5 failed on the **gating T4 batched review** with two blocking findings (the same
finding seen by two passes, `review-batch.md:3-4`) and carried a **C3 FAIL** on size. Both are
addressed; nothing else about v5's approach was re-submitted unchanged.

**(1) T4 blocker — "the decoder accepts reordered or whitespace-varied JSON that re-encodes to
different bytes … despite the claimed decode→encode byte identity" (`multipart.rs:1351`,
patched-file-relative in v5).** The finding was *correct about the claim*, not about the
decoder. v5's round-trip test asserted `encode(decode(bytes)) == bytes` over hand-authored
bytes and its comment read "decode→encode must be the identity **on stored bytes**" — a claim
over *arbitrary* JSON, which no `serde_json`-backed record can honour (field order and
whitespace are the encoder's, not the record's).

Fixed by making the claim true and pinning the behaviour that was previously unstated:

* production now states the property **with its domain** — for bytes *this codec wrote*,
  decode→encode is byte-identical because no field is optional, defaulted or skipped and the
  shape is closed; and explicitly **not** a canonicalisation check, with the consequence spelled
  out for the slices that add the writer (`crates/core/src/multipart.rs:1249-1272`);
* the test now has a dedicated leg, `re_encoding_a_record_this_codec_wrote_is_byte_identical`
  (`crates/core/tests/multipart_budget_admission.rs:109-124`), which asserts the byte identity
  **and** decodes a deliberately foreign spelling (fields reordered, whitespace and newlines
  inserted) showing it yields the *same value* and re-encodes to this codec's spelling.

I considered and rejected two alternatives (costs in §5): rejecting non-canonical bytes at
decode (a ninth rejection rule the brief forbids — "exactly G1–G8, no more, no less" — and an
ADR-0045 liberal-on-read violation), and retaining the raw bytes in the value to make identity
unconditional.

**(2) C3 FAIL — 468 semantic lines against the brief's 450.** This patch is **444**
(production 222 + test 222; §7 shows the count and the method, which is the reviewer's:
non-blank, non-comment added lines). The reduction came from the `Display` arms, one
`checked_rules` extraction that mirrors `InodeRecord::checked_shape`, a single-field G5 payload,
and folding the five malformed-bytes assertions into one loop — not from dropping a leg. All
eighteen legs of v5 are still here, plus the new byte-identity leg.

**(3) The round-3 C5 finding** ("no witness where the ceiling arm determines `U_ref`") stays
fixed: `u_ref_takes_whichever_of_its_two_arms_binds`
(`crates/core/tests/multipart_budget_admission.rs:147-159`) decides `U_ref` by the ceiling arm
on the second witness (`70`, against a raw term of `1_030`), with operands chosen so every
single-operator slip changes the value.

## 2. The shape, and why this one

Two surfaces over **one** validation, exactly as the brief's S1/S2 require:

* **S1** `impl Deserialize for AdmissionRecord` via `#[serde(try_from = "AdmissionRecordWire")]`
  (`crates/core/src/multipart.rs:1273-1274`, `:1315-1330`) — the `InodeRecord` model the brief
  cites (`crates/core/src/metadata.rs:1377`, `:1439`; base `:1349`, `:1411`). A value that
  decodes cannot be malformed whichever surface read it.
* **S2** `pub fn decode_admission_record` (`crates/core/src/multipart.rs:1349-1356`) — the peer
  of `decode_segment_record` (`crates/core/src/metadata.rs:2536-2547`; base `:2504-2517`). It
  reaches `AdmissionRecordWire` through the store-wide `metadata::decode`
  (`crates/core/src/metadata.rs:1564-1571`; base `:1536-1543`) and then applies the record's
  rules **directly**, so the typed `RecordError` never passes through serde's `Error::custom`
  funnel. This is deliberately *not* the base's dead `downcast` branch
  (`crates/core/src/metadata.rs:2542-2545`), which the brief warned about: after `custom` the
  domain error is already a `serde_json::Error` and the downcast cannot succeed. No guard
  identity is recovered from a message string anywhere.

`AdmissionRecordWire.profile` is the **unvalidated** `BudgetWire`
(`crates/core/src/multipart.rs:1305-1310`) — that is what keeps a *profile* violation typed
through S2. If it were a `Budget`, its nested validating `Deserialize` would have been
stringified before S2 could see it.

One distinct `RecordError` variant per rule (`crates/core/src/multipart.rs:135-206`), so the
test can tell G4's rejection from G7's without parsing prose. Rule order and sites:
`Budget::checked_rules` (`:1145-1193`) for G1–G7, `TryFrom<AdmissionRecordWire>` (`:1315-1330`)
for G8. G1 and G2 are checked **first, before any derivation is computed** — they are what make
`u_ref`/`max_sessions` total.

Exact arithmetic: `inflight_owned_refs` and `u_ref_exact` compute in `u128`
(`crates/core/src/multipart.rs:1062-1064`, `:1091-1096`); `u_ref()` narrows to `u64` behind
G7's guarantee (`:1106-1108`). One definition of the owned-`sidx:` product is shared by the G5
rule and the `U_ref` ceiling term, so the rule and the charge cannot drift — the argument
`checked_chunk_bytes` makes for the other cross-checked quantity in this repo
(`crates/core/src/metadata.rs:1208-1218`).

Scope held exactly: **two files**, no `docs/` file, no `metadata.rs` edit, no `Cargo.toml`
change, no `encode_record` envelope, no invented constant (`max_chunkref_bytes`, `B_ops`,
`MAX_SEG_CHUNKS`, `MAX_PUBLISHABLE_CHUNKS`, `WIDEST_SCHEME_BYTES` appear nowhere in the patch —
`grep` them in `patch.diff` and only the prose that explains *why they are absent* matches).
The only constants consulted at decode are `MAX_PART_NUMBER` (`crates/core/src/multipart.rs:277`,
pre-existing) and `SCAN_CAP` (`crates/traits/src/lib.rs:286`), both format/seam constants that
pass the brief's scope rule.

## 3. The twelve demonstrations (binding: leg → kind → negation → pasted output)

Each negation was applied to production **only**, the whole test target re-run through
`cargo test -p wyrd-core --test multipart_budget_admission`, the output captured, and the file
restored byte-for-byte (`git diff` after the runs is byte-identical to the shipped
`patch.diff`; the throwaway harness and its per-leg logs lived under `$PDCA_SCRATCH` and were
deleted, so the excerpts below are the record). Line numbers in the pasted output are the
shipped files'. To reproduce any row: apply the negation named in the "Negation applied"
column and run `cargo test -p wyrd-core --test multipart_budget_admission`.

| # | Leg | Kind | Negation applied | Pasted output (excerpt) |
|---|-----|------|------------------|--------------------------|
| 1 | **G1** `max_part_chunks ≥ 1` | 2 — totality, not isolable | dropped the `if self.max_part_chunks == 0` guard (`multipart.rs:1148-1150`) | `panicked at crates/core/src/multipart.rs:1122:9: attempt to divide by zero` — `test result: FAILED. 17 passed; 1 failed` |
| 2 | **G2** `max_inflight_parts ≥ 1` | 1 — isolating | dropped `if self.max_inflight_parts == 0` (`:1152-1154`) | `left: Ok(AdmissionRecord { count: 0, max_sessions: 1, profile: Budget { w_ref: 1, max_part_chunks: 1, max_parts_per_session: 1, max_inflight_parts: 0, max_staged_chunks: 1 } })` / `right: Err(MaxInflightPartsZero)` — `17 passed; 1 failed` |
| 3 | **G3** `max_parts_per_session ≤ MAX_PART_NUMBER` | 1 | dropped the `> MAX_PART_NUMBER` guard (`:1156-1160`) | `left: Ok(… max_parts_per_session: 1000000 …)` / `right: Err(PartsPerSessionUnaddressable { max_parts_per_session: 1000000 })` — `17 passed; 1 failed` |
| 4 | **G4** `max_inflight_parts ≤ max_parts_per_session` | 1 | dropped the `>` guard (`:1162-1167`) | `left: Ok(… max_parts_per_session: 10, max_inflight_parts: 20 …)` / `right: Err(InflightPartsExceedParts { max_inflight_parts: 20, max_parts_per_session: 10 })` — `17 passed; 1 failed` |
| 5 | **G5** `mip × mpc ≤ SCAN_CAP/2` | 1 | dropped the `owned_sidx > SCAN_HALF` guard **and its binding** (`:1170-1173`; the binding must go too or the crate fails `warnings = "deny"`) | `left: Ok(… max_parts_per_session: 524289, max_inflight_parts: 524289 …)` / `right: Err(StagingRangeUnscannable { owned_sidx: 524289 })` at `tests/…:252` — `16 passed; 2 failed` |
| 6 | **G6** `max_staged_chunks ≥ max_part_chunks` | 1 | dropped the `<` guard (`:1175-1180`) | `left: Ok(… max_part_chunks: 165, max_staged_chunks: 164 …)` / `right: Err(StagedChunksBelowPart { max_staged_chunks: 164, max_part_chunks: 165 })` — `17 passed; 1 failed` |
| 7 | **G7** `w_ref ≥ u_ref` | 1 | dropped the `u128::from(self.w_ref) < u_ref` guard and its binding (`:1184-1190`) | `left: Ok(… w_ref: 1 … max_sessions: 0 …)` / `right: Err(BudgetBelowFootprint { w_ref: 1, u_ref: 2 })` — `17 passed; 1 failed` |
| 8 | **G8** `max_sessions == derived` | 1 | replaced the `stored != derived` guard with `let _ = derived;` (`:1322-1324`; the binding must stay used under `warnings = "deny"`) | `left: Ok(… count: 3, max_sessions: 5 …)` / `right: Err(MaxSessionsNotDerived { stored: 5, derived: 4 })` — `16 passed; 2 failed` |
| 9 | **P1** occupancy decodes | 3 — inverted | **added** `if wire.count > stored { return Err(MaxSessionsNotDerived …) }` to `TryFrom<AdmissionRecordWire>` | `panicked at tests/…:323:10: occupancy above the cap is not a decode error: MaxSessionsNotDerived { stored: 9000, derived: 4 }` — `17 passed; 1 failed` |
| 10 | **P2** large `max_staged_chunks` decodes | 3 | **added** a stand-in publishable ceiling `if self.max_staged_chunks > 1_000_000 { … }` to `checked_rules` | `panicked at tests/…:340:10: a large staged ceiling is not a decode error: StagedChunksBelowPart { max_staged_chunks: 10000000, max_part_chunks: 165 }` — `16 passed; 2 failed` |
| 11 | **P3** `max_part_chunks` outside 165–381 decodes | 3 | **added** the proposal window `if !(165..=381).contains(&self.max_part_chunks) { … }` to `checked_rules` | `panicked at tests/…:353:61: a small cap is not an error: StagedChunksBelowPart { max_staged_chunks: 1, max_part_chunks: 1 }` — `10 passed; 8 failed` |
| 12 | **P-arith** (both halves) | 3 | replaced the exact spelling with the **naive same-width** one: `u128::from(self.max_inflight_parts * self.max_part_chunks)` and `u128::from(self.max_staged_chunks + 2 * self.max_inflight_parts * self.max_part_chunks)` | **accept half:** `p_arith_accept…` `panicked at crates/core/src/multipart.rs:1095:24: attempt to add with overflow`; **reject half:** `p_arith_reject…` `panicked at crates/core/src/multipart.rs:1063:20: attempt to multiply with overflow` — i.e. it stops naming G5 — `16 passed; 2 failed` |

**Isolation, stated precisely.** Kind 1's rule is that the *witness* violates only its own rule
— which holds for all seven (each witness's other seven rules were re-checked by hand at this
round, and the arithmetic is in the test's own doc comments). Two negations knock out a
**second** test, and neither is an isolation failure:

* **G5** also fails `p_arith_reject`, whose entire content is "the maximal record is refused
  **by G5**". Its output under the G5 negation is itself informative — no panic, no wrap:
  `left: Err(MaxSessionsNotDerived { stored: 18446744073709551615, derived: 2147 })`, i.e. the
  `u128` arithmetic stayed exact with G5 gone.
* **G8** also fails `max_sessions_is_clamped…`, whose negative half is a *second G8 assertion*
  (the record naming the unclamped quotient `1_000_000` where the clamp derives `524_288`).

**Kind 2, in the brief's words.** G1 **cannot be isolated**: no value violates G1 while
satisfying G8, because at `max_part_chunks = 0` the `U_ref` of `0016:1469` is `0` and the
`MAX_SESSIONS` quotient of `0016:1470` is undefined. Dropping G1 therefore breaks **totality**,
not one assertion — the pasted output is a division-by-zero **panic** inside production
(`crates/core/src/multipart.rs:1122`, `Budget::max_sessions`), reached from a plain decode of
stored bytes. That is the demonstration: remove G1 and a stored record can crash the reader.

## 4. P-arith: evaluation order and the exact-arithmetic check

`decode_admission_record` reaches **G5 before G7** (`checked_rules` order,
`crates/core/src/multipart.rs:1170` then `:1185`), so the P-arith-reject witness comes back
`StagingRangeUnscannable`. Verified in exact integers that this is a genuine violation and not
an artefact:

* witness: `count = max_sessions = u64::MAX`, `w_ref = u64::MAX`, `max_part_chunks = u32::MAX`,
  `max_parts_per_session = max_inflight_parts = 999_999`, `max_staged_chunks = u32::MAX`;
* G1 ✓ (`4_294_967_295 ≠ 0`), G2 ✓, G3 ✓ (`999_999 ≤ 999_999`), G4 ✓ (equal), G6 ✓ (equal);
* **G5 ✗**: `999_999 × 4_294_967_295 = 4_294_963_000_032_705`, against `SCAN_CAP/2 = 524_288`
  — larger by a factor of ~8.2 × 10⁹. The test asserts that exact product as the payload, so
  the multiplication itself is pinned, not just the verdict;
* no panic, no wrap: the shipped run returns `Err(StagingRangeUnscannable { owned_sidx:
  4_294_963_000_032_705 })`, the mathematical value.

**One correction to the brief, made explicitly rather than quietly.** The brief states that at
this witness "**G7** is too (no representable `w_ref` reaches that `U_ref`)". In exact integers
that is **not so**, and the difference matters because the brief forbids "a variant naming a
guard that exact arithmetic says HOLDS": `U_ref = min((999_999 + 999_999) × (2³²−1),
(2³²−1) + 2 × 4_294_963_000_032_705) = min(8_589_926_000_065_410, 8_589_930_295_032_705) =
8_589_926_000_065_410 ≈ 8.59 × 10¹⁵`, which is far **below** `w_ref = u64::MAX ≈ 1.84 × 10¹⁹`
— so **G7 holds** here and only G5 is violated. Confirmed empirically by demonstration #5's log:
with G5 removed the same witness sails past G7 and is refused by G8 instead
(`Err(MaxSessionsNotDerived { stored: 18446744073709551615, derived: 2147 })`, and
`⌊u64::MAX / 8_589_926_000_065_410⌋ = 2147` checks out). The shipped decoder therefore names
the one rule that genuinely fails.

**P-arith-accept** (`max_staged_chunks = u32::MAX`, everything else minimal) is guard-legal in
exact integers — `U_ref = min(2, 4_294_967_297) = 2` — and decodes `Ok`. Under the naive
same-width spelling it **panics in debug** (pasted above). In **release** (wrapping) the same
spelling gives `ceiling = (u32::MAX + 2) mod 2³² = 1`, hence `U_ref = min(2, 1) = 1` and
`max_sessions = min(2/1, 524_288) = 2 ≠ 1` — the record would be **rejected** with
`MaxSessionsNotDerived { stored: 1, derived: 2 }`, a variant naming a rule exact arithmetic says
**holds**, which is the outcome the brief forbids. That release figure is hand-computed
arithmetic, not a pasted run (a release build of the dev-dependency tree costs minutes and adds
nothing the debug panic does not already prove).

## 5. Alternatives considered and rejected — with costs

1. **Reject non-canonical JSON at decode** (re-encode the decoded value and compare to the
   input, `Err` on mismatch) — would make the byte-identity claim unconditional and "fix" the
   T4 finding by construction. **Rejected**: it is a *ninth* rejection rule, and the Success
   criterion is "exactly the guard set — no more, no less"; it would also refuse a stored
   ledger the day `serde_json`'s number or escape formatting moved, which is precisely the
   liberal-on-read boundary ADR-0045 and `MAX_ROOT_SEGMENTS` draw
   (`crates/core/src/metadata.rs:302-322`) — an unreadable `mpuctl` wedges multipart fleet-wide.
   Cost is not the argument; correctness is (it would be ~6 lines: `let round = encode(&value);
   if round.as_ref() != value_bytes { return Err(…) }` plus a variant and a Display arm).
2. **Keep the stored bytes inside the value and re-emit them on encode** (identity for *any*
   input). **Rejected on cost and shape**: a `raw: Bytes` field excluded from `PartialEq`, a
   hand-written `Serialize` emitting it, loss of `Copy`, and a wire struct that no longer
   matches `0016:348`'s three fields — ≈ 45 added lines against a 450-line budget already spent,
   for a property no writer needs (every `mpuctl` writer is `metadata::encode`, and the
   alternative CAS shape — precondition on the raw bytes just read, `metadata.rs:2012` — is
   available to #656–#659 either way).
3. **Public fields instead of the eight accessors** (`InodeRecord` has `pub` fields,
   `crates/core/src/metadata.rs:1378-1420`) — would save exactly **24** semantic lines
   (8 accessors × 3 lines after rustfmt). **Rejected**: `Budget::max_sessions` divides by
   `u_ref` (`:1122`), so a hand-built `Budget { max_part_chunks: 0, .. }` would panic in a
   public API, and a hand-built `AdmissionRecord` could carry a `max_sessions` its profile does
   not derive — the exact tear this record exists to refuse. Private fields make both
   unrepresentable; `InodeRecord` can afford `pub` because nothing of its invariant is load-
   bearing for a *panic*.
4. **Splitting the slice** — considered at Plan and recorded there as rejected; nothing in the
   build changed that (the removed apparatus, not the slice, was the overrun).

## 6. Pre-declared sign-off items (§6) — both are the brief's, not patch defects

1. **C4-verify is UNVERIFIABLE (exit 77), by design.** Ran the project's own gate:
   `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → `EXIT=77`. GREEN leg:
   `cargo test -p wyrd-core --test multipart_budget_admission (fix applied)` → `running 18
   tests` / `test result: ok. 18 passed; 0 failed`. RED leg: production reverted, test kept →
   12 unresolved-symbol errors, **0 tests run**, so the gate records the *absence* of a
   measurement, not a failure. This is criterion-absence red (born-at-tier), exactly as the
   brief pre-declares; the twelve demonstrations in §3 are the substitute evidence.
2. **Docs currency deferred.** No `docs/` file is touched. The general rule (`AGENTS.md:154-157`)
   fires on a change that alters a **persisted field**; the more specific, already-merged rule
   in the file this slice edits says the living architecture doc gains these namespaces with the
   slice that first *persists* one (`crates/core/src/multipart.rs:69-78`, kept verbatim except
   for the "key **grammar** only" → "key grammar plus the admission ledger's record **shape**"
   correction the brief requires). This slice persists nothing: no writer, no store call, no
   production consumer. If sign-off takes the general rule instead, the remedy the brief names
   is a one-paragraph follow-up commit on this branch plus the matching correction of that
   block — not a re-plan.

No **NEEDS-HUMAN external dependency**: nothing beyond the base Rust toolchain was needed
(`typos`, `cargo-deny`, `cargo-machete`, `docs-renderer` and `cargo-mutants` all ran locally as
part of `cargo xtask ci` / the C5 script).

## 7. Gates run here, and the size accounting

* `./engine/xtask.sh ci` (the C4-ci gate = typos → docs lint/render → gitlink/unsafe guards →
  `cargo fmt --all --check` → clippy `-D warnings` → build → test → machete → deny →
  conformance → DST): **`xtask ci: all checks passed`**, exit 0, on the patched tree. This is
  also the commit-hook readiness check — an earlier run caught one unformatted assertion, which
  is fixed (`cargo fmt --all -- --check` is clean).
* `./engine/scripts/run-verify.sh`: green 18/18 with the fix on a **clean checkout of
  `origin/main` + `patch.diff`** (so the patch applies to the target branch), exit 77 on the red
  leg as above.
* `scripts/mutants-in-diff` (advisory C5, `cargo mutants --in-diff patch.diff --no-shuffle`):
  **`60 mutants tested in 2m: 55 caught, 5 unviable`** — **0 missed**, exit 0. (Round 3's C5
  failure was 1 missed on the unmeasured `U_ref` ceiling arm; that witness is now leg
  `u_ref_takes_whichever_of_its_two_arms_binds`, and the G5 payload carrying the *product*
  `owned_sidx` pins the multiplication a second way.)
* `cargo doc -p wyrd-core --no-deps` (not a gate — `xtask ci` does not run it, and the repo
  carries 14 pre-existing `private_intra_doc_links` findings in `metadata.rs` / `read.rs` /
  `write.rs` / `erasure.rs`): the two this patch introduced were removed anyway
  (`Budget::checked_rules` and `Budget::u_ref_exact` are now plain code spans in the public
  docs, `crates/core/src/multipart.rs:1009`, `:1099`), so the patch adds **zero** rustdoc
  findings.
* Size: **444 semantic added lines** across exactly **2** files — production 222
  (`git diff` added lines of `crates/core/src/multipart.rs`, minus blank and comment lines),
  test 222 — against the brief's ≤ 450. Method is the C3 reviewer's from v5 ("non-blank,
  non-comment"); reproduce with
  `git diff | grep '^+' | grep -v '^+++' | sed 's/^+//' | grep -v '^\s*$' | grep -v '^\s*//'`.

## 8. Forced self-refutation (required answers)

* **(a) Genuine red?** **Yes.** Two independent forms. The gate's own RED leg reverts the
  production hunk and the test no longer compiles (12 unresolved symbols, 0 tests) — criterion
  absence, the brief's pre-declared shape. Stronger, because compile-absence is weak evidence:
  **each of the twelve legs was individually falsified** by negating *only* its production rule
  with the rest of the patch intact, and each went red with the pasted output in §3. Every
  negation was reverted and the tree re-verified (`git diff --stat` equals the shipped patch;
  18/18 green again).
* **(b) Production path?** **Yes.** The test imports only `wyrd_core::metadata` and
  `wyrd_core::multipart` and calls the shipped `decode_admission_record`,
  `metadata::decode::<AdmissionRecord>` and `metadata::encode` — no stand-in, no re-implemented
  arithmetic, no copy of a guard. That is verifiable in the negation runs: mutating *production*
  is what turned the tests red. The only thing the test computes for itself is
  `SCAN_CAP/2` (from the real `wyrd_traits::SCAN_CAP`), and it is used as an *expected* value,
  not as a substitute rule.
* **(c) Fixture includes the fault?** **Yes.** Every witness is hand-authored JSON **containing**
  the fault — the zero `max_part_chunks`, the `524_289` in-flight product one past `SCAN_CAP/2`,
  the `max_sessions: 5` that its own profile does not derive, `u32::MAX` in the field that
  overflows the naive ceiling — fed to the production decoder through both surfaces. Nothing is
  curated out: `decode_both` asserts S1 and S2 agree on *every* witness, so a rule enforced on
  one surface and missed on the other fails the run, and the malformed-bytes leg feeds bytes
  that are not a ledger at all.

## 9. Residual judgement for the human

* The planner's **re-scope** (no `encode_record`/`decode_record` envelope) is implemented as the
  brief instructs, including the module-header correction that withdraws the forward reference
  (`crates/core/src/multipart.rs:13-18`). If you disagree, the brief's remedy is a later slice
  (after #717), not a rebuild here.
* `#[serde(deny_unknown_fields)]` is a **durable forward-compatibility decision**: a future
  additive field to `mpuctl` becomes a versioned format change. The reasoning (both live CAS
  shapes in this repo punish a dropped field, differently but both durably) is in
  `crates/core/src/multipart.rs:1249-1272`.
* `Budget`/`AdmissionRecord` have **no constructor** — by the brief's instruction (no
  configuration-validation constructor in this slice). #656–#659 will need one for the
  absent-reads-as-`{count: 0}` bootstrap; that is theirs to add, through the same rules.
