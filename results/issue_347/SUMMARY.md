# Result — issue 347 / chunkref-fragments-placement-expansion-helper

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: `ChunkRef::fragments()` exists (BINDING: signature & semantics per
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`;
- Scope (one logical fix) / out of scope: add `ChunkRef::fragments()` (ADR-0040 decision 2) in `crates/core/src/metadata.rs`

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

# Check review — issue 347 / chunkref-fragments-placement-expansion-helper

**Reviewer:** Check (advisory, artifact-only, decorrelated). Inputs: `patch.diff`,
`brief.md`, `check-gates.json`. `build-notes.md` deliberately withheld.

**Grounding.** `$PDCA_TARGET` was not readable from this sandbox (env access blocked),
but a `getwyrd/wyrd @ main` checkout at `/home/eddie/wyrd/wyrd` was in the allowed set and
its tree matches the patch **pre-image exactly** at all four hunks (metadata.rs:110-124,
gc.rs:188-205, rebalance.rs:155-167, reconstruction.rs:226-232) — the patch applies cleanly,
so the base is **not stale**. Citations below are grounded there; ADR-0040 grounded at
`docs/design/adr/0040-mixed-era-placement-expansion.md` in the same tree.

**Independent re-derivation performed.** Confirmed (1) the ADR-0040 decision-2 signature the
success criterion binds to matches the patch impl; (2) scope completeness — exactly three
full `0..fragment_count()`->`placed_dserver` read-expansion walks exist in the target and all
three are routed; (3) each rewrite is value-for-value behaviour-preserving; (4) the red
mechanism (no `fragments()` pre-fix -> net-new test won't compile) and (5) the test's
concrete-value assertions match `fragments()`'s semantics. I could **not** re-run
`cargo xtask ci` live (read-only target sits at pre-patch state; the patch is not applied
there, and the Bash sandbox gates cargo/git) — so C4 leans on the recorded green gate plus
the static red->green derivation, not a fresh build.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is binding and checkable: `fragments()` signature/semantics per ADR-0040 decision 2 + the three named consumers routed + the scheme x length test matrix (`brief.md:32-42`); ADR authoritative at `docs/design/adr/0040-mixed-era-placement-expansion.md:60-73`. No spec defect to resolve. |
| C2 Reproduction (red pre-fix) | PASS | Criterion-absence red, legitimate for a net-new API: pre-fix `metadata.rs:97-125` has no `fragments()`, so the added test (new `fragments_matrix` mod in `placement_record.rs`) cannot compile -> red. Not a flipped prior assertion; matches the brief's stated posture (`brief.md:72-77`). |
| C3 Change | PASS | Diff adds `fragments()` with the exact ADR decision-2 signature (`patch.diff:38-40`) and routes GC (`gc.rs:197`), rebalance (`rebalance.rs:165`), reconstruction (`reconstruction.rs:230`); each rewrite yields identical values — pure centralization. `placed_dserver` doc updated to list rebalance. |
| C4 Verification (red->green) | PASS | Gate green: `C4-ci` (cargo xtask ci) and `C4-verify` (red pre-fix / green post-fix) both `pass` (`check-gates.json:32-49`). Independently confirmed the red cause (absent `fragments()`) and green semantics statically. Live re-run infeasible in-sandbox (read-only pre-patch target; cargo gated) — recorded green gate relied upon; **not** downgraded, no fabricated ordering blocker. |
| C5 Causal adequacy | PASS | Root cause is the *duplication* of the expansion across three open-coded sites (how #346's divergence hid); the fix **removes** it by centralizing into one helper and routing all three — cause transformed, not guarded. Symptom-guard smell-test does NOT fire: no capability probe / runtime guard added; the "liberal" identity fallback is pre-existing `placed_dserver:119-124` behaviour preserved. Scope complete — `read.rs:104` single-index use correctly left unrouted. |
| T1 Structure | PASS | Test in the designated file `crates/core/tests/placement_record.rs` as `mod fragments_matrix`, standard `#[test]` fns; constructs `ChunkRef` with all four public fields (matches struct `metadata.rs:84-95`), so it compiles against the post-fix API. |
| T2 Shape | PASS | Shape is exactly the ADR-mandated equality-vs-`placed_dserver` matrix across `EcScheme::None` and `ReedSolomon{k,m}` x empty/full/malformed vectors (`patch.diff:98-161`); the None "short" case is documented unreachable (count==1) and covered by a longer vector instead — defensible. |
| T3 Runtime | PASS | Each case invokes `fragments()` and asserts against concrete expected tuples; `C4-verify` confirms the suite runs red->green (`check-gates.json:41-49`). |
| T4 Contribution | PASS | Net-new coverage over the net-new `fragments()` API — not redundant; the behaviour-preserving consumer routing stays covered by the existing custodian gc/rebalance/reconstruction suites, which `C4-ci` green confirms still pass. |
| T5 Judgment | PASS | `assert_matches_placed_dserver` is partly self-referential (both sides derive from `placed_dserver`), but every case also pins independent concrete literals (e.g. `vec![(0,7)]`, explicit `want` vectors, `patch.diff:102/109/122/129-160`), giving a real oracle that would catch a wrong `placed_dserver` too. Coverage adequate. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human owes: confirm this behaviour-preserving centralization achieves its *purpose* — one enforced placement-expansion definition so a future #346-style divergence cannot recur — and that ordering intent holds (#360 CI grep-gate lands AFTER this migration; #348/#349 build on this helper). Also confirm the brief's prior-art claim (net-new `fragments()`, no dup PR — `brief.md:82-86`), NOT mechanically re-settleable here, and that leaving `read.rs:104` + the write/repoint sites unrouted is the intended scope boundary (ADR-0040 decision 5). |

## Notes for the human (§6 candidates)
- **Validation / fitness-to-purpose** — see the NEEDS-HUMAN row: purpose-achievement + wave-ordering (#360 after; #348/#349 on top) + scope-boundary sign-off.
- **Prior-art** — brief asserts the search ran by affected file path across merged/open/closed
  history with no duplicate (`brief.md:82-86`); I could not re-run the GitHub query here, so the
  human should confirm before ready-for-review.
- No FAIL findings. The patch is a clean, ADR-conformant, scope-complete, behaviour-preserving
  centralization; the only open items are the human-only validation/prior-art confirmations above.

### Advisory — codex

- `crates/custodian/src/rebalance.rs:91` still documents `EvacPlan::placement` as resolved through `ChunkRef::placed_dserver`, even though this patch moves the expansion site to `chunk.fragments()` at `crates/custodian/src/rebalance.rs:166`. Update the comment to name `ChunkRef::fragments()` so the rebalance documentation matches the new single expansion helper.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Human owes: confirm this behaviour-preserving centralization achieves its *purpose* — one enforced placement-expansion definition so a future #346-style divergence cannot recur — and that ordering intent holds (#360 CI grep-gate lands AFTER this migration; #348/#349 build on this helper). Also confirm the brief's prior-art claim (net-new `fragments()`, no dup PR — `brief.md:82-86`), NOT mechanically re-settleable here, and that leaving `read.rs:104` + the write/repoint sites unrouted is the intended scope boundary (ADR-0040 decision 5).

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
- By / date: Eduard Ralph / 2026-07-01

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
