# Build notes — #771 `multipart-retire-obligation` (iteration 2)

Withheld from the reviewer; written for the human at sign-off.

Target: `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`
at `a801997`. Three files, as the brief scopes:

| File | Added semantic lines (non-blank, non-comment) |
|---|---|
| `crates/core/src/multipart.rs` | 468 (brief's shape: ≈ 500) |
| `crates/core/tests/multipart_retire_obligation.rs` (**new**) | 497 (≈ 480) |
| `docs/design/architecture/05-building-block-view.md` | 1 (≈ 20) |
| **total** | **966** — inside the brief's ≤ 1,000 budget (round 1 was 1,060, the T1 finding) |

---

## 1. What this iteration changes relative to the rejected round 1

Round 1 was rejected on a gating batch review with four blocking findings, all grounding to
two defects, plus a size finding. Both defects are fixed at the *seam*, not papered over.

### (a) The `parts:"all"` wildcard was mode- and shape-neutral — it is now one writer row

Round 1 modelled `parts` as a single component with `mode: None` (both modes legal), so
`{"parts":"all"}` decoded under `retire:records:` and without the `session` teardown. The
review's read of the writer table is correct and I have taken it as binding: the wildcard has
**exactly one** row in `0016` — the reaper's `Open` teardown `put retire:bytes:{session, all}`
(`0016:2187`). Everything else that names parts names an **explicit set** (`0016:662`, `:665`,
`:823`, `:919-921`, `:2193`).

The rebuild splits the relation into two table rows instead of one
(`crates/core/src/multipart.rs:2689` `PARTS_COMPONENT`, `:2704` `ALL_PARTS_COMPONENT`), so a
wildcard can never inherit an explicit set's permissions
(`present_components`, `:2869`), plus one cross-component rule for the half that is not a
key relation (`checked_against_key`, `:2964`, and the new
`RecordError::RetireAllPartsWithoutSession`, `:343`):

* `all` under `retire:records:` → `RetireModeMismatch { key_mode: Records, component: "parts:all" }`;
* `all` without `session` → `RetireAllPartsWithoutSession`.

Why the *session* half matters as much as the mode half: `all` is an instruction to enumerate
the session's own `part:<id>:` range **at drain time**, and only the teardown fence that
installs it makes that range immutable (`0016:664`, `:2187`, `:673`). Without the teardown
component, the same obligation names whatever a still-live session happens to hold when the
drain arrives — a set no writer chose.

### (b) The same rebuild tightened `session` to bytes-mode, which round 1 had as mode-neutral

Reading the whole writer table for the wildcard forced the neighbouring question. `0016:356`'s
*value column* reuses the `{session, parts}` shorthand for `retire:records:`, but its own prose
and **every** batch row give that namespace exactly two contents: the **published** parts'
records and one rolled-back attempt's segments (`:356`, `:662`, `:663`, `:665`, `:823`,
`:2194`). `session` means the session's own staged residue — its owned `sidx:` fragments,
*orphan-marked and then deleted* (`0016:355`, `:2587` states the marking explicitly). Under
`retire:records:` that would mean "delete those staging records without marking their bytes",
which is outcome (a) — durable fragments left with no record naming them and no orphan
evidence. So `SESSION_COMPONENT.mode = Some(Bytes)` (`:2680`), and R3's leg asserts it.

This also corrects a wrong doc claim in round 1's `RetirePayload::session()` accessor (it said
the obligation owes the `mpu:`/`slot:` records; those are the terminal delete's, and the
terminal delete is *gated on this obligation having drained* — `0016:673`, so an obligation
naming them would be circular). Fixed at `:2833`.

### (c) `RetirePayload` no longer implements `Deserialize` — the bypass is now a compile error

The CONVENTION finding (round 1, `multipart.rs:2762`) and the T5 NEEDS-HUMAN both name the same
hazard: a public `Deserialize` lets any consumer do `metadata::decode::<RetirePayload>(value)` —
the exact idiom the sibling records use — and obtain a payload that has never met its key. That
is the value ADR-0045 decision 1 says must not exist, and the drain (#656–#659) is precisely the
consumer that must not have it. **Decided in the safe direction**: the derive is gone
(`:2810`), `Serialize` stays (the canonical-bytes gate and future writers need it), and
`decode_retire_obligation(key, value)` (`:3084`) is the only decode surface in existence.

**This deviates from one clause of the brief's `Test file` bullet** ("both surfaces asserted to
agree — … and the store-wide `metadata::decode`"). It cannot be honoured and the finding fixed
at the same time: with no `Deserialize` there is no S1 surface to agree with. I judged the
invariant ("a stored obligation's payload may not disagree with the token that names it") to
outrank the mirroring instruction — the sibling files' S1 leg exists because *those* records are
value-decodable, which is the property this one must not have. The test header states the
substitution explicitly (`crates/core/tests/multipart_retire_obligation.rs:13-20`) and the R9
identity leg — the part of `decode_both` that is load-bearing here — is kept file-wide.
If the human prefers S1 agreement, it is a one-line derive, but it re-opens the bypass.

---

## 2. The record-format decisions this child freezes (and where they are recorded in code)

* **Combined shapes are expressible** (R1). The payload is a *set of components*
  (`RetirePayloadWire`, `:2748`; `RetirePayload`, `:2811`), not a choice between them, because
  `0016` installs `{session, parts}` (`:665`, `:823`, `:2193`) and `{parts} + {seg}` (`:356`)
  as single values under single keys. A mutually exclusive arm shape would force a writer to
  install two records where the protocol installs one — two keys where `require_absent` and the
  session's emptiness gate expect one (`0016:369-380`). The writer table is transcribed as a
  doc table on `RetirePayload` so a reviewer can check the shape set against `0016` in one place.
* **One canonical token epoch** (R6). `token.epoch == seg.epoch`, exactly (`:2964`, the
  `RetireSegmentEpochMismatch` arm). The fence that ends an attempt is the batch that installs
  the obligation naming that attempt's segments and preconditions on
  `require(mpu == Completing@E)` (`0016:663-665`, `:2357-2362`), so `E` is both. Admitting
  `E ± 1` would give one obligation several legal keys and `require_absent` cannot see a key it
  never looked at. The **limit** is stated rather than over-claimed: the group *nonce* is
  deliberately independent of the upload id (`0016:499-509`), so a foreign group under your
  token is not detectable at decode; this binds the epoch component only. Round 1's inherited
  `checked_against_token` F18-containment over-claim is gone.
* **`0016` is not edited** (INTEGRATION §2 immutability): both decisions are recorded in the
  code's doc comments citing the proposal lines they resolve.

## 3. Alternatives considered, with their cost

* **Keep `Deserialize` but make it always fail** ("use `decode_retire_obligation`"): ~12 lines
  for a hand-written `Deserialize`. Rejected — a runtime refusal is strictly weaker than the
  compile error the absent impl already gives, and it would exist only to keep a test leg alive.
* **Model the wildcard as a third `PartScope` state carried on `session`** (e.g.
  `{"session":{"parts":"all"}}`): rewrites the accepted `{session, parts: <set>}` and
  `{parts: <set>}` shapes too — the wire spelling of three of the nine writer rows, ~40 lines of
  production and ~25 of test churn — to express a rule two table rows and one `if` express
  (11 added lines: `ALL_PARTS_COMPONENT` at `:2704` is 5, the guard at `:2974` is 3, the error
  variant's data-free arm 1 + Display 2).
* **`#[serde(untagged)]` for `PartScopeWire`** instead of the hand-written visitor (`:2517`):
  saves ~30 lines but degrades every third-spelling rejection to "data did not match any variant
  of untagged enum PartScopeWire", naming an internal type and no rule. Kept the visitor; the
  test asserts the operator-facing message ("range-encoded part-number set").
* **Dropping `PartNumberSet::from_numbers`** (the writer-side canonical minter, `:2439`): would
  save 14 production + 12 test lines, and was not needed for the budget once the test was
  compacted. Kept because it is the one thing that stops each writer in #656–#659 re-spelling
  the coalescing rule by hand — which is exactly how a second spelling of one obligation gets
  stored. `len()` and `iter()` **were** dropped (13 lines): no consumer, and the paging
  behaviour of an iterator over a 10,000-member run is the drain's decision, not this child's.
* **Size**: the 1,060 → 966 reduction is all test-side compaction (per-test local closures and
  one `for` loop over the R9 spellings) plus the two API drops above. No leg was deleted; three
  were added.

## 4. The negations — nine isolating, plus two extra (run, pasted, reverted)

Method: remove exactly one production check, run `cargo test -p wyrd-core --test
multipart_retire_obligation` (the brief's own GREEN leg command, which is what `C4-verify`
runs), record the failure, `git checkout --` the file. Where a plain deletion trips
`-D warnings` (dead code / unused binding) instead of producing a red test, the **rejection**
is neutered in place instead — the check still runs, it just no longer returns `Err`; that is
the same experiment with a compiling tree. The driver scripts ran under `$PDCA_SCRATCH` and were
removed afterwards (scratch discipline); their output is pasted verbatim below, and each
negation is a one-line edit at the cited symbol, reproducible by hand.

Every one of the nine failed **exactly one** test (`25 passed; 1 failed`).

**R2 — the obligation-owes-nothing checks** (`checked_shape`, `:2908-2921`)
```
test an_obligation_owing_nothing_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {}
  left: Ok((Session { … epoch: 7, part: None }, RetirePayload { session: false, parts: None, chunks: [], generation: None, seg: None }))
 right: Err(RetireObligationOwesNothing { component: "payload" })
test result: FAILED. 25 passed; 1 failed
```

**R3 — the per-component mode check** (`Component::checked_mode`, `:3005`; rejection neutered)
```
test a_component_under_the_wrong_mode_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":100,"placement":[5,6,7]}]}
  left: Ok((Session { … part: Some((PartNumber(3), AttemptId("b2…"))) }, RetirePayload { … chunks: [ChunkRef { id: 9, … }] }))
test result: FAILED. 25 passed; 1 failed
```

**R4a — the session-wide-under-a-per-part-token arm** (`checked_scope`, `:3017`)
```
test a_session_wide_component_under_a_per_part_token_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"session":true,"parts":"all"}
  left: Ok((Session { … part: Some((PartNumber(3), AttemptId("b2…"))) }, RetirePayload { session: true, parts: Some(All), … }))
 right: Err(RetireTokenSuffixMismatch { component: "session", token_names_part: true })
test result: FAILED. 25 passed; 1 failed
```

**R4b — the per-part-under-a-session-wide-token arm** (`checked_scope`, `:3017`)
```
test chunks_under_a_session_wide_token_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,…}]}
  left: Ok((Session { … part: None }, RetirePayload { … chunks: [ChunkRef { id: 9, … }] }))
 right: Err(RetireTokenSuffixMismatch { component: "chunks", token_names_part: false })
test result: FAILED. 25 passed; 1 failed
```

**R5 — the generation identity check** (`checked_against_key`, `:2983`; rejection neutered)
```
test a_generation_disagreeing_with_its_token_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"generation":{"inode":43,"version":4,"chunks":[…]}}
  left: Ok((Generation { inode: 42, version: 4 }, RetirePayload { … generation: Some(RetireGeneration { inode: 43, version: 4, … }) }))
test result: FAILED. 25 passed; 1 failed
```

**R6 — the canonical token-epoch check** (`checked_against_key`, `:2993`; rejection neutered)
```
test a_segment_group_epoch_other_than_the_tokens_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"seg":{"nonce":"c3c3…","epoch":6}}
  left: Ok((Session { … epoch: 7, part: None }, RetirePayload { … seg: Some(SegmentGroup { nonce: SegmentNonce("c3c3…"), epoch: 6 }) }))
test result: FAILED. 25 passed; 1 failed
```

**R7 — the nested chunk-geometry check** (`checked_shape`'s `checked_chunk_scheme` loop, `:2901`)
```
test an_unsupported_chunk_scheme_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"chunks":[{"id":9,"scheme":{"ReedSolomon":{"k":0,"m":1}},"len":100,"placement":[5]}]}
  left: Ok((… RetirePayload { … chunks: [ChunkRef { id: 9, scheme: ReedSolomon { k: 0, m: 1 }, … }] }))
 right: Err(ChunkSchemeUnsupported { chunk_id: 9, k: 0, m: 1 })
test result: FAILED. 25 passed; 1 failed
```

**R8 — the part-number set's coalescing check** (`PartNumberSet::from_runs`, `:2426`; rejection neutered)
```
test a_noncanonical_part_number_set_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"parts":[[1,2],[3,4]]}
  left: Ok((… RetirePayload { … parts: Some(Set(PartNumberSet([(1, 2), (3, 4)]))), … }))
test result: FAILED. 25 passed; 1 failed
```

**R9 — the canonical-bytes gate** (`decode_retire_obligation`'s `require_canonical`, `:3096`)
```
test a_foreign_spelling_of_an_accepted_payload_is_rejected ... FAILED
assertion `left == right` failed: decode->encode is not byte-identical for RetirePayload { session: true, parts: Some(Set(PartNumberSet([(1, 4)]))), … }
  left: "{\"session\":true,\"parts\":[[1,4]]}"
 right: "{\"parts\":[[1,4]],\"session\":true}"
test result: FAILED. 25 passed; 1 failed
```
Note this one trips the **identity** assertion inside the helper before the expected-error
assertion — i.e. without the gate the reordered spelling decodes and its re-encode is not what
was read, which is the precise hazard.

**Extra 1 — the wildcard rule** (the round-1 review finding; `:2974`)
```
test the_all_parts_wildcard_outside_a_session_teardown_is_rejected ... FAILED
assertion `left == right` failed: unexpected verdict for {"parts":"all"}
  left: Ok((Session { … part: None }, RetirePayload { session: false, parts: Some(All), … }))
 right: Err(RetireAllPartsWithoutSession)
test result: FAILED. 25 passed; 1 failed
```

**Extra 2 — R1's completeness, negated the other way.** Collapsing the payload to mutually
exclusive arms is simulated by refusing any payload with more than one component. Six tests
fail, and they are the right six — the three **combined** writer shapes plus three legs whose
witnesses carry two components:
```
test bytes_session_and_all_parts_decodes ... FAILED
test bytes_session_and_parts_decodes ... FAILED
test records_parts_and_seg_decodes ... FAILED
test a_session_wide_component_under_a_per_part_token_is_rejected ... FAILED
test a_generation_disagreeing_with_its_token_is_rejected ... FAILED
test a_foreign_spelling_of_an_accepted_payload_is_rejected ... FAILED
test result: FAILED. 20 passed; 6 failed
```

**Extra 3 — the identity property is file-wide, not one test's.** Removing
`skip_serializing_if = "is_absent"` from `session` (so an absent component re-encodes as
`"session":false`) fails **12** tests, at the helper's identity assertion. Deliberately
non-isolating: it perturbs the encoder rather than a check, which is what the brief's R9
"break the identity on one accepted witness" asks to demonstrate.
```
test result: FAILED. 14 passed; 12 failed
```

## 5. The three refutation questions

* **(a) Genuine red?** Yes, eleven times over — §4. Each of the nine binding rules, reverted
  one at a time, turns exactly one test red with the pasted output above. The *file itself* is
  also born-at-tier: on the base, `decode_retire_obligation`, `RetirePayload` and
  `PartNumberSet` do not exist, so with the production hunk reverted the test does not compile
  — the pre-declared **UNVERIFIABLE (exit 77)** RED the brief registers for `C4-verify`
  (Falsifiability bullet). Run here (`git checkout HEAD -- crates/core/src/multipart.rs`, test
  file kept, then restored):
  ```
  error[E0432]: unresolved imports `wyrd_core::multipart::decode_retire_obligation`,
      `wyrd_core::multipart::PartNumberSet`, `wyrd_core::multipart::PartScope`,
      `wyrd_core::multipart::RetirePayload`
  error[E0433]: cannot find `RetireGeneration` in `multipart`
  error[E0599]: no variant named `RetireObligationOwesNothing` found for enum `RecordError`
  error[E0599]: no variant, associated function, or constant named
      `RetireAllPartsWithoutSession` found for enum `RecordError` in the current scope
  ```
  The nine negations are what stands in for a behavioural red, as the brief's verification
  posture requires.
* **(b) Production path?** Yes. Every witness goes through
  `wyrd_core::multipart::decode_retire_obligation` — the shipped `pub fn` — and its identity
  half re-encodes through the shipped store-wide `wyrd_core::metadata::encode`. There is no
  mock, no copy of the decoder in the test, and no constructor: `RetirePayload` has none, so a
  value can only come into existence by decoding (`crates/core/tests/multipart_retire_obligation.rs`
  has no `RetirePayload { … }` literal anywhere).
* **(c) Fixture includes the fault?** Yes. The fixtures are hand-authored JSON bytes that
  *contain* the fault under test — the `k = 0` scheme, the `E ± 1` epoch, the adjacent runs
  `[[1,2],[3,4]]`, the wrong-inode generation, the reordered field spelling, the wildcard
  without its teardown — decoded under the very key the production writer would use
  (`retire_key(mode, &token)`, the shipped key minter). Nothing is curated out: the accepted
  set and the rejected set are asserted against the same helper, and the boundary cases the
  child deliberately does **not** reject (a length-mismatched placement, a foreign group nonce,
  a generation's unchecked segment epoch) are asserted to **decode**, so the rejection legs
  cannot be passing by over-rejecting.

## 6. Gates run here

* `./engine/xtask.sh ci` (the project's own whole-tree gate, delegating `cargo xtask ci` in
  `$PDCA_WORKTREE`) → **`xtask ci: all checks passed`, exit 0**, including `cargo fmt --check`,
  `clippy -D warnings`, `typos`, and the doc renderer (`render_site.py --check`, "wrote 99
  page(s)") — the last is the load-bearing external dependency for R10.
* `cargo fmt --all` run over every touched file, so the target's own commit hooks have nothing
  to reformat. The final gate run is over the exact tree `patch.diff` was generated from.
* `cargo test -p wyrd-core --test multipart_retire_obligation` (the brief's GREEN leg, and what
  `C4-verify` invokes): **26 passed, 0 failed**.
* No external dependency was missing: no NEEDS-HUMAN of that class to declare. `typos`,
  `docs-renderer` and the Rust toolchain were all present; `cargo-deny` ran inside the gate
  (its only output was the pre-existing `license-not-encountered` advisory, untouched by this
  patch).
* Nothing was pushed and no PR was opened or marked ready.

## 7. What is left for the human at sign-off

1. **The pre-declared UNVERIFIABLE `C4-verify` RED** (born-at-tier posture (a)) — expected per
   the brief's Falsifiability bullet, evidenced instead by §4's negations.
2. **The public API boundary** (round 1's T5): this patch decides it — no `Deserialize` on
   `RetirePayload`. Recorded here because it is a deliberate, narrow deviation from the brief's
   test-mirroring clause (§1c).
3. **The two writer-table tightenings** (`all` → the teardown shape only; `session` → bytes
   mode only) freeze the accepted shape set for #656–#659. They follow the batch rows rather
   than `0016:356`'s looser value-column shorthand; §1b gives the argument in full, and it is
   the one judgement in this patch a reader of `0016`'s value column alone might read
   differently.
