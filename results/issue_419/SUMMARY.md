# Result — issue 419 / read-consistency-conformance-properties

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the read-consistency level decided in #261 (fresh-TSO snapshot per op; **one snapshot held across all internal pages of a single `scan()`**) is pinned by **no dedicated test**. What the shared suite (`crates/metadata-conformance/src/lib.rs` — four `contract_*` fns) has today: `contract_commit_and_get` (`:24-37`) already asserts **sequential, single-handle** get-after-commit, and `contract_require_value_gates` (`:83-111`) already asserts **sequential** stale-`require` → `Conflict`. What it does **not** have: any property pinning the **snapshot/temporal** dimension #261 decided — a **fresh-TSO `get` observing a prior committed write** (read-your-writes / the anti-stale-read guarantee, ADR-0015 clause 3, "Per-session read-your-writes and monotonic reads") — any **concurrent rename-race** (a move landing between a read-then-commit's read and its `commit` → `Conflict`), or any **scan-consistency** property at all. So the read contract's *concurrency/snapshot* semantics are documented-only, and the M4.3 TiKV scan swap (#254) has no backend-agnostic discriminator to preserve.
- Success criterion: three new **backend-agnostic** conformance properties — **read-after-commit** (a fresh-TSO `get` observes a prior committed write — the read-your-writes / anti-stale dimension *beyond* the existing sequential `contract_commit_and_get`), **rename-race** (a mutation interleaved between a read-then-commit's read and its `commit` → `Ok(Conflict)`, never a torn binding), **scan-is-consistent-cut** (a concurrent rename appears in exactly one scan position) — exist in the shared suite and **pass against redb**, driven by `crates/metadata-redb/tests/conformance.rs`; `cargo xtask ci` exits 0; and **each property is demonstrably NON-VACUOUS and NON-REDUNDANT** — Do shows a deliberately-violating store makes each one go **red** (so it catches something the four existing `contract_*` fns do not), recorded in build-notes. Because the properties touch only the `MetadataStore` trait (no redb/tikv-specific API), TiKV's suite inherits them unchanged. **BINDING:** the three properties exist, pass on redb, and each is demonstrated to catch a violation the existing suite misses. **ILLUSTRATIVE:** the exact interleaving mechanism (a `Sim` seed vs. deterministically-ordered ops) is Do's call.
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`  (the shared `metadata-conformance` crate lives **only** on the integration branch — created by #252/PR #421, not on `main`; INTEGRATION §2. Feature branch `feat/m4-read-consistency-conformance`; commit subject `test(metadata-conformance): read-consistency properties, redb baseline (#419)`)
- Scope (one logical fix) / out of scope: add three backend-agnostic property functions to `crates/metadata-conformance/src/lib.rs` — `contract_read_after_commit`, `contract_rename_race_yields_conflict`, `contract_scan_is_consistent_cut` (names illustrative) — modeled on the existing `contract_*` + `pollster::block_on` pattern, using `wyrd_testkit::Sim` seeds for the race/interleaving properties exactly as `redb/tests/conformance.rs::version_cas_rejects_a_stale_writer` already does; drive all three from `crates/metadata-redb/tests/conformance.rs::trait_contract` (or a sibling test) so redb passes them; and add whatever violating-store test double the demonstrated-red requires. / **out of scope:** wiring the **TiKV** conformance driver to run these (that is #254 — "runs these against TiKV's native scan"); any **production** code change (`metadata-redb/src`, `metadata-tikv/src`, `core`, `traits` stay untouched — this slice only adds test code); #254's paging/cap implementation and its at-scale/cap-breach tests; the **module-level read-consistency doc** (the *documentation* half of 261.b — that lands in #254, not here).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — decided properties from #261; backend-agnostic test-suite hardening; no new ADR minted).
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 419 / read-consistency-conformance-properties

**Task under review:** the decided-but-unpinned #261 read-consistency contract (fresh-TSO snapshot per op; one snapshot across a `scan()`) has *no executable test*. This slice adds three backend-agnostic conformance properties — `contract_read_after_commit`, `contract_rename_race_yields_conflict`, `contract_scan_is_consistent_cut` — to the shared `metadata-conformance` suite, wires them into the redb driver, and ships a demonstrated-red harness (three deliberately-violating `MetadataStore` stubs) proving each property is load-bearing. Test-only; no production code changes.

**Grounding / caveat:** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt`, readable and **already carrying the patch** (source == `patch.diff`), so it is current, not stale. I grounded every citation on that source. The sandbox blocked me from re-executing `cargo`/`git` (both require interactive approval unavailable in this session), so for C4/T3 I rely on the deterministic gate results (`check-gates.json`: `C4-ci` pass, `C4-verify` pass — "red without the fix, green with it") plus full static re-derivation of the stub logic, and I say so rather than claim a re-run I did not perform.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion enumerates the three properties + the non-vacuous/non-redundant obligation; grounded in #261 / ADR-0015 clause 3 / proposal 0015 §Open questions. Spec is executable and unambiguous (brief.md:11-12,32). |
| C2 Reproduction (red pre-fix) | PASS | Posture-a net-new: "red" = criterion absence. Reproduction is materialized as the three `#[should_panic]` counter-store tests (`tests/demonstrated_red.rs:311-317,383-389,464-470`); each targets a distinct failure mode the four pre-existing `contract_*` clauses miss. Gate `C4-verify` independently records red→green. |
| C3 Change | PASS | Adds 3 property fns (`src/lib.rs:136-280`), 3 violating stores + paired should-panic/passes-existing tests (`tests/demonstrated_red.rs`), redb wiring (`metadata-redb/tests/conformance.rs:37-39`), dev-deps only (`Cargo.toml:23-30`). No production file touched — invariant held. |
| C4 Verification (red→green) | PASS | Deterministic gates: `cargo xtask ci` pass and per-fix `run-verify.sh` PASS (red pre-fix, green post-fix), `check-gates.json:33-48`. I could not independently re-run cargo (sandbox-blocked), but source logic re-derives to the asserted outcomes; not a stale-target failure. |
| C5 Causal adequacy | NEEDS-HUMAN | No symptom-guard smell (test-only, no capability probe). Decision owed: `contract_read_after_commit` (`src/lib.rs:136-154`) re-reads the **same** store handle across 4 overwrites, whereas the brief's read-your-writes rationale emphasized the **fresh-handle / cross-TSO** dimension that bites TiKV (brief.md:38). Confirm the same-handle repeated-overwrite property genuinely pins the decided fresh-TSO semantics rather than a weaker restatement — it is non-vacuous (StaleCacheStore catches it) but may not be the *intended* cause. |
| T1 Structure | PASS | Properties live in the shared crate on the `MetadataStore` trait surface only (backend-agnostic, ADR-0016); violating stubs correctly scoped to `tests/` dev-only, never in the library (`tests/demonstrated_red.rs:228-236`). Placement matches the brief's NEEDS-HUMAN guidance. |
| T2 Shape | PASS | Mirrors the existing `contract_*` + `pollster::block_on` idiom; violating stubs use only public `WriteBatch`/`Precondition` fields (`traits/src/lib.rs:365-382`), and the rename property faithfully models `core/src/metadata.rs:276,284,288`. |
| T3 Runtime | PASS | Deterministic (no real threads/Sim needed; ordered ops), un-containerized — consistent with `cargo xtask ci` green (`check-gates.json:34-37`). Independent local re-run blocked by sandbox; flagged, not claimed. |
| T4 Contribution | PASS | Every property is invoked by the redb driver; every violating store has both a should-panic (catches new property) and a passes-existing-sequential test (proves non-redundant) — no dead code, each artifact earns its place (`tests/demonstrated_red.rs:319-329,391-401,472-482`). |
| T5 Judgment | NEEDS-HUMAN | Decision owed on `contract_scan_is_consistent_cut` (`src/lib.rs:244-280`): on redb's atomic `scan` it passes **trivially** (brief-acknowledged, brief.md:18,36). Non-vacuity rests entirely on LeakyScanIndexStore. Human must confirm the redb baseline is a real discriminator worth landing here vs. deferring to #254's TiKV at-scale test. Also confirm the deterministic (non-`Sim`) interleaving in the rename-race property is the right call (brief permits it as ILLUSTRATIVE). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | #261 is still **OPEN and unratified** — the decision lives only in a maintainer comment, proposal 0015 still lists it under §Open questions, no ADR ratifies it (brief.md:37). Human must confirm freezing this decision into the *shared* conformance suite (which #254/TiKV inherits unchanged) is appropriate before it is governed. Prior-art check by file path confirmed clean (brief.md:24). |

## Notes for the human (§6 candidates)
1. **C5 — read-after-commit intent:** verify the same-handle repeated-overwrite property captures the fresh-TSO / cross-handle read-your-writes dimension #261 decided, not a weaker same-session restatement (brief.md:38).
2. **T5 — scan-consistent-cut on redb:** vacuous-green on redb by construction; confirm the demonstrated-red (LeakyScanIndexStore) makes it a real inherited discriminator for #254, or move it to #254's TiKV at-scale test (brief.md:36).
3. **Validation — #261 not ratified:** confirm folding the decision into proposal 0015 / an ADR before freezing it into the shared suite is not a prerequisite (brief.md:37).
4. **Violating-store placement (informational, satisfied):** the three stubs are dev/test-scope only (`tests/demonstrated_red.rs`, dev-dependencies) — never shipped, never a real backend; matches the brief's placement note (brief.md:39).

### Advisory — codex

- NEEDS-HUMAN — `contract_scan_is_consistent_cut` does not mutate during a single `scan()`; it performs `scan` before the rename, commits the rename, then performs a second `scan`, so it may only pin post-rename delete/put visibility rather than the specified "one snapshot held across all internal pages of a single `scan()`" behavior that #254's paged TiKV scan must preserve. `crates/metadata-conformance/src/lib.rs:253`
- NEEDS-HUMAN — `contract_read_after_commit` remains a single-store, sequential overwrite/read loop, so it is not clearly exercising the brief's fresh-TSO / cross-handle read-your-writes dimension beyond ordinary same-handle get-after-commit semantics; the demonstrated-red double proves this exact repeated-overwrite shape is non-redundant, but a human should decide whether that is the intended contract pin. `crates/metadata-conformance/src/lib.rs:136`

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — No symptom-guard smell (test-only, no capability probe). Decision owed: `contract_read_after_commit` (`src/lib.rs:136-154`) re-reads the **same** store handle across 4 overwrites, whereas the brief's read-your-writes rationale emphasized the **fresh-handle / cross-TSO** dimension that bites TiKV (brief.md:38). Confirm the same-handle repeated-overwrite property genuinely pins the decided fresh-TSO semantics rather than a weaker restatement — it is non-vacuous (StaleCacheStore catches it) but may not be the *intended* cause.
- [x] T1 Structure
- [x] T5 Judgment — Decision owed on `contract_scan_is_consistent_cut` (`src/lib.rs:244-280`): on redb's atomic `scan` it passes **trivially** (brief-acknowledged, brief.md:18,36). Non-vacuity rests entirely on LeakyScanIndexStore. Human must confirm the redb baseline is a real discriminator worth landing here vs. deferring to #254's TiKV at-scale test. Also confirm the deterministic (non-`Sim`) interleaving in the rename-race property is the right call (brief permits it as ILLUSTRATIVE).
- [x] Validation — fitness-to-purpose — #261 is still **OPEN and unratified** — the decision lives only in a maintainer comment, proposal 0015 still lists it under §Open questions, no ADR ratifies it (brief.md:37). Human must confirm freezing this decision into the *shared* conformance suite (which #254/TiKV inherits unchanged) is appropriate before it is governed. Prior-art check by file path confirmed clean (brief.md:24).
- [x] `contract_scan_is_consistent_cut` does not mutate during a single `scan()`; it performs `scan` before the rename, commits the rename, then performs a second `scan`, so it may only pin post-rename delete/put visibility rather than the specified "one snapshot held across all internal pages of a single `scan()`" behavior that #254's paged TiKV scan must preserve. `crates/metadata-conformance/src/lib.rs:253`
- [x] `contract_read_after_commit` remains a single-store, sequential overwrite/read loop, so it is not clearly exercising the brief's fresh-TSO / cross-handle read-your-writes dimension beyond ordinary same-handle get-after-commit semantics; the demonstrated-red double proves this exact repeated-overwrite shape is non-redundant, but a human should decide whether that is the intended contract pin. `crates/metadata-conformance/src/lib.rs:136`

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Decide whether executable conformance pins may be frozen into the shared, multi-backend suite before the decision they encode is ratified into a governed doc (#261/#419: pin landed while #261 open and proposal 0015 still lists it under §Open questions).
