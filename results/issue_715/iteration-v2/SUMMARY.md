# Result — issue 715 / multipart-budget-admission

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the key grammar exists (#691, merged at `9dbcd72`) but no record
  **values** do, and there is no envelope to encode/decode any. This child lands `Budget`
  (the profile tuple of `0016:1463-1480` and its pure derivations `U_ref` and
  **`MAX_SESSIONS = min( ⌊W_ref/U_ref⌋ , SCAN_CAP/2 )`** — `0016:1470` is explicit that the
  `SCAN_CAP/2` term is **a clamp the implementation applies**, not an operator range check,
  and it is load-bearing: `W_ref` is sized from host RAM and `U_ref` from the caps, so a
  legal pairing (large `W_ref`, small parts) makes `⌊W_ref/U_ref⌋` exceed `SCAN_CAP` and
  break the reaper's `scan("mpu:")`. `SCAN_CAP` is a base constant —
  `crates/traits/src/lib.rs:286`, `1 << 20`. **Both terms are binding; a derivation that
  omits the clamp is wrong** (plan-advisory finding, 2026-08-09)), `AdmissionRecord` (`mpuctl`,
  `0016:333-527`), and the shared `encode_record`/`decode_record` envelope that later
  record children extend with their own arms. Salvage from
  `results/issue_692/iteration-v2/patch.diff` (record types and decoders, added-file
  lines ~846–1818, the `Budget`/`AdmissionRecord` portion), fixing the recorded defects
  rather than re-shipping the reviewed shape.
- Success criterion: `Budget` and `AdmissionRecord` round-trip `encode`/`decode`, and
  each of the following hand-authored torn values is rejected with a typed error
  (ADR-0045) — one named negation per leg demonstrated in `build-notes.md` (drop the
  single check, paste the failing output, revert):
  **(1a)** an `AdmissionRecord` whose stored `max_sessions` disagrees with what its own
  stored `profile` tuple derives (the derivation functions are implemented HERE);
  **(1f)** `Budget::new` enforces BOTH ends of every knob range in `0016:1463-1480` that a
  **FORMAT** constant can decide. **Decode validates against stable format maxima, NEVER
  against live deployment knobs** (`0016:390-402` — the normative statement of this
  boundary; a decoder that enforced the current knob would make a durable record
  unreadable the day an operator lowers it). Four bounds, each **independently** enforced
  and independently falsified (see Falsifiability — one negation per bound, because a
  single out-of-range value can violate several at once and stay red on a surviving guard):
  **(1f-i)** `max_part_chunks` satisfies the value-ceiling rule
  `max_chunkref_bytes × max_part_chunks ≤ MAX_VALUE_BYTES / 2` (computable on the base:
  `metadata.rs:327`);
  **(1f-ii)** `max_staged_chunks ≥ max_part_chunks` (the lower end — at least one maximal
  part must remain stageable, `0016:1472`);
  **(1f-iii)** `max_staged_chunks ≤ MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS_FORMAT_MAX`, the
  publishable ceiling. `MAX_ROOT_SEGMENTS` is on the base (`metadata.rs:322`);
  **`MAX_SEG_CHUNKS` is NOT** — it has no code definition, only a prose reference. **This
  slice therefore DEFINES `MAX_SEG_CHUNKS_FORMAT_MAX`** as a compile-time constant of the
  encoding, derived exactly as the value-ceiling rule above
  (`max_chunkref_bytes × N ≤ MAX_VALUE_BYTES / 2`), with a `const` assertion tying the two.
  That is the format maximum `0016:390-402` demands; the *deployment* value stays #508's;
  **(1f-iv)** `max_inflight_parts ≤ max_parts_per_session` (`0016:1476`, iteration-13
  finding 2 — a session cannot have more parts in flight than it may ever hold);
  **(1g)** the `sidx:` scan bound counts **committed staging entries as well as** in-flight
  chunks, so an accepted profile can never exceed `SCAN_CAP/2` (batch-review
  `multipart.rs:1074`; `SCAN_CAP` at `crates/traits/src/lib.rs:286`);
  **(leg 3, binding the other way)** a decoded `AdmissionRecord` whose `count` exceeds
  its `max_sessions` still **decodes** — occupancy above a lowered cap is a legitimate
  live state, not a decode error (`0016:390-402`; the same
  liberal-on-read boundary at `metadata.rs:312-321`); assert it decodes. Identity relations
  are binding; occupancy relations are not.
  **NOT enforced here — `B_ops`, and this is deliberate.** The batch review's
  `multipart.rs:1041` blocker names the value-size **and** operation-envelope bounds
  together, but `B_ops` is a **backend-calibrated deployment knob owned by #625**
  (`0016:1475`, `:1487`) with no value on this base — it is not a format constant, so by
  `0016:390-402` it does **not** belong at decode. It is enforced where new work is
  *admitted* (the slot reserve / part commit / Complete fence), which is #508's and #625's.
  Do MUST NOT invent a `B_ops` value to satisfy 1f. (Plan-advisory finding, 2026-08-09:
  the previous wording made an unresolvable calibration a decode-time acceptance threshold.)
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: extend `crates/core/src/multipart.rs` with
  `Budget`, `AdmissionRecord`, their validating decoders, and the
  `encode_record`/`decode_record` envelope (arms for this child's records only). One new
  test file. Budget ≈ 550 added lines / 2 files. Cite `path:line` on `9dbcd72` for every
  change; sources Do MUST open: `0016:333-527`, `0016:1463-1480`, ADR-0045;
  `metadata.rs:312-321` and `:327-352` (the format maxima the ranges are computed
  against). / out of scope: every other record type (child-2, child-3); `metadata.rs`
  and any file outside `multipart.rs` + the new test; the knob **values** (#655); the
  outcome enums, answer table, `Verb`, `MultipartEtag`, digests, `sha2` (#693 — no
  `Cargo.toml`/`Cargo.lock` change); store round trips (#656–#659); all `docs/` files.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 57 mutants tested in 2m: 39 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add the multipart `Budget` and `AdmissionRecord` codecs and enforce their format-stable admission invariants, with round-trip and torn-value coverage.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether 1f-iii is bounded by the mutable `metadata::MAX_ROOT_SEGMENTS` named in the brief or by a new versioned format maximum—the target calls the former deployment capacity while decode must remain stable, and the choice changes durable-record readability (`crates/core/src/metadata.rs:302`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared born-at-tier exception—the clean `9dbcd72` checkout exits 101 because the named test target is absent, so it proves criterion absence rather than a behavioral red. |
| C3 Change | FAIL | Rebuild to the specified widest-encoding ceiling—the patch derives 1,063 from a 47-byte minimum although the target requires `max_chunkref_bytes × N` and states 165–381; otherwise over-budget part values are admitted (`crates/core/src/multipart.rs:951`, `crates/core/src/multipart.rs:966`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1466`); the patch also has 650 nonblank/noncomment additions against the 550-line budget. |
| C4 Verification (red→green) | FAIL | The required 1f-i property remains red despite full CI, 20 tests, and 57 mutants passing: an applied-patch probe measured one realistic ref at 303 bytes and 165 refs at 50,161 bytes versus the 50,000-byte ceiling while `Budget::new` accepted the profile; the shipped test only proves the realistic width is greater than the minimum (`crates/core/tests/multipart_budget_admission.rs:168`, `crates/core/tests/multipart_budget_admission.rs:179`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the causal bound around the worst-case encoded value—the minimum-width derivation answers whether some spelling fits, not whether every admitted maximal record fits, so it does not restore the value-ceiling invariant (`crates/core/src/multipart.rs:930`, `crates/core/src/multipart.rs:1135`). |
| T1 Structure | PASS | The contribution stays in the two requested files and preserves the deliberately pure module boundary, with no Cargo, docs, store, async, or runtime wiring (`crates/core/src/multipart.rs:7`, `crates/core/tests/multipart_budget_admission.rs:1`). |
| T2 Shape | PASS | Private fields, validating constructors, denied unknown fields, and the typed structural envelope make malformed stored shapes errors rather than values (`crates/core/src/multipart.rs:1059`, `crates/core/src/multipart.rs:1285`, `crates/core/src/multipart.rs:1334`, `crates/core/src/multipart.rs:1369`). |
| T3 Runtime | N/A | No runtime path exists in this slice—the module is pure and the records have no live writer or store call yet (`crates/core/src/multipart.rs:7`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the unavailable internal #654/#692 archived attempts or the reported seven batch-review blockers duplicate unresolved work—the permitted artifacts omit both `scripts/review-branch` output and those archives; target merged history and all GitHub closed-unmerged PRs were checked by both affected paths and found no duplicate. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild the test with an independent widest-record oracle—the current assertions hard-code 1,063 from the production minimum and merely require the 303-byte realistic case to imply a smaller quotient, so the unsafe admission arithmetic stays green (`crates/core/tests/multipart_budget_admission.rs:180`, `crates/core/tests/multipart_budget_admission.rs:208`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the durable format may admit profiles whose realistic part payload already exceeds `V/2`, and whether the oversized review surface is acceptable before later store writers make that boundary operationally irreversible (`crates/core/src/multipart.rs:925`, `crates/core/src/multipart.rs:1135`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether 1f-iii is bounded by the mutable `metadata::MAX_ROOT_SEGMENTS` named in the brief or by a new versioned format maximum—the target calls the former deployment capacity while decode must remain stable, and the choice changes durable-record readability (`crates/core/src/metadata.rs:302`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390`).
- [ ] C2 Reproduction (red pre-fix) — Accept the declared born-at-tier exception—the clean `9dbcd72` checkout exits 101 because the named test target is absent, so it proves criterion absence rather than a behavioral red.
- [ ] C5 Causal adequacy — Rebuild the causal bound around the worst-case encoded value—the minimum-width derivation answers whether some spelling fits, not whether every admitted maximal record fits, so it does not restore the value-ceiling invariant (`crates/core/src/multipart.rs:930`, `crates/core/src/multipart.rs:1135`).
- [ ] T4 Contribution — Decide whether the unavailable internal #654/#692 archived attempts or the reported seven batch-review blockers duplicate unresolved work—the permitted artifacts omit both `scripts/review-branch` output and those archives; target merged history and all GitHub closed-unmerged PRs were checked by both affected paths and found no duplicate.
- [ ] T5 Judgment — Rebuild the test with an independent widest-record oracle—the current assertions hard-code 1,063 from the production minimum and merely require the 303-byte realistic case to imply a smaller quotient, so the unsafe admission arithmetic stays green (`crates/core/tests/multipart_budget_admission.rs:180`, `crates/core/tests/multipart_budget_admission.rs:208`).
- [ ] Validation — fitness-to-purpose — Decide whether the durable format may admit profiles whose realistic part payload already exceeds `V/2`, and whether the oversized review surface is acceptable before later store writers make that boundary operationally irreversible (`crates/core/src/multipart.rs:925`, `crates/core/src/multipart.rs:1135`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- [ ] The defect states `MAX_SESSIONS = ⌊W_ref/U_ref⌋` (`brief.md:4-5`), but the target's normative formula is `min(⌊W_ref/U_ref⌋, SCAN_CAP/2)` and says the clamp is applied by the implementation (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1470`, reiterated at `:2118-2120`). Leg 1g later relies on that omitted term, so the brief gives two incompatible definitions of the identity relation that `AdmissionRecord` decode must enforce.
- [ ] The scope hides an unresolved policy/calibration prerequisite. `Budget::new` must enforce `MAX_PART_CHUNKS ≤ B_ops` (`brief.md:17-19`), while knob values are explicitly out of scope and `Depends on` is empty (`brief.md:58-60`, `:97`). The target proposal assigns `B_ops` to the backend-calibrated batch knob (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1475`) and requires a per-backend timing case before that value is established (`:2907-2909`). Likewise, the promised `MAX_STAGED_CHUNKS` upper bound needs `MAX_SEG_CHUNKS` (`brief.md:19-21`), but the target has no code definition for it (only a prose reference at `crates/core/src/metadata.rs:538`; the implemented related constant is `MAX_ROOT_SEGMENTS` at `:322`). The planner must either declare the prerequisite/value source or explicitly put those values and their calibration in scope; the stated two-file pure-record change cannot determine these acceptance thresholds as written.
- [ ] The falsifiability mapping cannot prove all of criterion 1f. The criterion requires independent enforcement of the value-size ceiling, the `B_ops` ceiling, the `max_staged_chunks` lower bound, and its upper bound (`brief.md:17-21`; the distinct rules are normative at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1466-1468`), but the demonstrated-red list collapses them into only `1f-lower` and `1f-upper` and directs Do to drop one check (`brief.md:35-37`). One upper case can violate multiple ceilings and stay red when any single guard remains, so the omitted guard is not falsified. Name a separately isolating case/negation for each independent bound.
- [ ] The claimed invariant citation is unresolved: `brief.md:40` cites `docs/principles.md:109` and `:137`, but that path does not exist at the resolved target. The available target authority is ADR-0045's structural-versus-contextual decode boundary (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`), which does not itself state the brief's C-1 wording. Replace the phantom citation with a resolvable source that actually supports the invariant.
- [ ] The tracker/prior-attempt claims cannot be checked from the review inputs: `notes.json` and `sources/` are absent, and the brief's required salvage/review artifacts (`brief.md:7-10`, `:76-89`) are also absent from the resolved target. Thus the assertions that the listed blockers came from the tracker/review and that no load-bearing thread constraint was omitted (`brief.md:90-95`) have no inspectable evidence. Put the relevant thread quote and prior-review evidence into the brief or supply the declared evidence bundle before relying on them.
- [ ] C1 Spec — Resolve Plan scope before landing — this slice adds persisted `mpuctl` fields, but the binding rubric requires a same-PR living-architecture update while the brief excludes docs, so compliance changes scope (`AGENTS.md:154`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared born-at-tier exception—the clean `9dbcd72` checkout exits 101 because the named test target is absent, so it proves criterion absence rather than a behavioral red.; C5 Causal adequacy — Rebuild the causal bound around the worst-case encoded value—the minimum-width derivation answers whether some spelling fits, not whether every admitted maximal record fits, so it does not restore the value-ceiling invariant (`crates/core/src/multipart.rs:930`, `crates/core/src/multipart.rs:1135`).; T4 Contribution — Decide whether the unavailable internal #654/#692 archived attempts or the reported seven batch-review blockers duplicate unresolved work—the permitted artifacts omit both `scripts/review-branch` output and those archives; target merged history and all GitHub closed-unmerged PRs were checked by both affected paths and found no duplicate.; T5 Judgment — Rebuild the test with an independent widest-record oracle—the current assertions hard-code 1,063 from the production minimum and merely require the 303-byte realistic case to imply a smaller quotient, so the unsafe admission arithmetic stays green (`crates/core/tests/multipart_budget_admission.rs:180`, `crates/core/tests/multipart_budget_admission.rs:208`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-09

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
