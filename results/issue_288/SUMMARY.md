# Result — issue 288 / read-repair-enqueue-integrityfault

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The read path is documented to feed the shared repair queue when it excludes
- Success criterion: When `get_fragment_at` returns an `IntegrityFault` (the verifying
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Mirror scrub's classifier in `read.rs`: treat an `IntegrityFault` from

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
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

# Check review — issue 288 / read-repair-enqueue-integrityfault

**Advisory / artifact-only.** Inputs seen: `patch.diff`, `brief.md`, `check-gates.json`
(no `build-notes.md` — withheld by design). `$PDCA_TARGET` was not resolvable from this
sandbox (env + `cargo`/git history unavailable; per scope I did not search other
checkouts), so citations below are grounded on `patch.diff`. This is a **target-state
caveat**, not a patch defect — I do not raise it as a C4 FAIL. The two C4 gate rows in
`check-gates.json` (`C4-ci`, `C4-verify`) both report `pass`; I could not re-execute them
here and rely on the gate record plus the flippability argument visible in the test.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Defect, success criterion and invariant are concrete and testable: classify a typed `IntegrityFault` from `get_fragment_at` as a corruption finding at the two enumerated read-path fetch sites (brief.md:6-33). Decision turns on nothing further — spec is unambiguous and bounded. |
| C2 Reproduction (red pre-fix) | PASS | Two regressions added with an explicit flip story — reverting to `?` / `if let Ok(Some)` makes both `repair::queued_repairs == [chunk_id]` assertions fire (patch.diff:197-199, 267-271, 330-334); `C4-verify` gate = pass. I could not re-run red/green here, so the red claim rests on the gate + the legible flip, not my own re-execution. |
| C3 Change | PASS | Diff edits exactly the two cited fetch sites: `EcScheme::None` now matches `Err(e) if is_integrity_fault → corrupt.push; return Err` and a transient `Err(e) → return Err` (patch.diff:26-40); RS arm adds `Err(e) if is_integrity_fault → corrupt.push` (read around) and `Err(_) → {}` transient (patch.diff:102-108). Mirrors scrub's classifier; raw-bytes corrupt arms unchanged. In scope. |
| C4 Verification (red→green) | PASS | `check-gates.json`: `C4-ci` pass (fmt/clippy/build/test/deny/conformance) and `C4-verify` pass (per-fix red→green), both gating-eligible. Independent re-run not possible in this sandbox; verdict defers to the gate record. No stale-target FAIL fabricated. |
| C5 Causal adequacy | PASS | Root cause is the read path failing to classify the typed `IntegrityFault` (the `?` and `if let Ok(Some)` swallow it); fix removes the cause by classifying the variant uniformly at both sites, not by guarding a present capability. **Symptom-guard smell-test does NOT fire**: `is_integrity_fault` is error-variant classification mirroring scrub (patch.diff:33,102), not a `hasattr`/try-import capability probe over an optional capability — no load-time side effect being papered over. |
| T1 Structure | PASS | Regression lives in the designated file `crates/core/tests/read_repair.rs`; `IntegrityFaultingStore` is a focused decorator over `MemChunks` returning `IntegrityFault` for one fragment (patch.diff:140-178). |
| T2 Shape | PASS | Each test asserts BOTH the enqueue (`queued_repairs == [chunk_id]`, producer key `b"read"`) AND the read outcome (RS reconstructs from survivors; None fails) — pinning the enqueue obligation, not incidental read behaviour (patch.diff:258-276, 322-339). |
| T3 Runtime | PASS | Tests are `#[tokio::test]` and are covered by the green `C4-ci` (test) gate; no skip/ignore markers in the diff. |
| T4 Contribution | PASS | Net-new coverage for the `IntegrityFault`-at-fetch category (both EcScheme arms) absent from prior tests, which used raw corrupt bytes only (brief.md:53-60, patch.diff:187-340). |
| T5 Judgment | PASS | No scope creep — raw-bytes arms, scrub, and backends untouched; transient errors deliberately not reclassified (patch.diff:37-39,105-108), matching the brief's out-of-scope list. The faithfulness-of-double question is escalated under Validation rather than here. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human owes two decisions. (1) **Double-vs-integration fidelity:** the unit double relies on the default `PlacementChunkStore::get_fragment_at` delegating to `get_fragment` (patch.diff:175-178); the brief calls a real `FsChunkStore`/gRPC integration regression "desirable" but omitted (brief.md:53-60) — confirm the default delegation truly matches both real backends' fetch path, else the green is over-claimed. (2) **287 conflict / wave ordering:** brief flags a conflict with 287 on the same `crates/core` read/repair path with no build-on dependency (brief.md:35-39) — confirm scheduling so neither lands blind on the other's base. Prior-art (6a33a33, 5aece0e, scrub 8c2adcf; empty `gh pr list`) is asserted in the brief but I could not mechanically re-run it against target history from this sandbox. |

### Advisory — codex

No advisory findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Human owes two decisions. (1) **Double-vs-integration fidelity:** the unit double relies on the default `PlacementChunkStore::get_fragment_at` delegating to `get_fragment` (patch.diff:175-178); the brief calls a real `FsChunkStore`/gRPC integration regression "desirable" but omitted (brief.md:53-60) — confirm the default delegation truly matches both real backends' fetch path, else the green is over-claimed. (2) **287 conflict / wave ordering:** brief flags a conflict with 287 on the same `crates/core` read/repair path with no build-on dependency (brief.md:35-39) — confirm scheduling so neither lands blind on the other's base. Prior-art (6a33a33, 5aece0e, scrub 8c2adcf; empty `gh pr list`) is asserted in the brief but I could not mechanically re-run it against target history from this sandbox.

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
- By / date: Eduard Ralph / 2026-06-28

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
