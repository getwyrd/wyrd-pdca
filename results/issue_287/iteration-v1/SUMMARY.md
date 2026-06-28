# Result — issue 287 / gc-identity-placement-fallback

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The read path tolerates pre-M3 / mixed-era chunk maps with empty or short
- Success criterion: A committed inode whose `ChunkRef` has `placement: vec![]` has its
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Make GC's (and the shared scrub) reference-set computation honor the

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

# Check review — issue 287 / gc-identity-placement-fallback

**Advisory, artifact-only.** Inputs: `patch.diff`, `brief.md`, `check-gates.json`
(`build-notes.md` withheld by design). `$PDCA_TARGET` is **not resolvable** from this
sandbox (env unreadable, no `getwyrd/wyrd` checkout in tree). Per protocol I ground all
citations on `patch.diff` alone and treat the missing target as a **target-state caveat,
not a patch defect** — I do not raise a "cannot apply / does not compile" blocker, and I
do not search unrelated checkouts. The two gating/per-fix gates (`C4-ci`, `C4-verify`)
were run by the harness and report `pass`; I record those results but could not
independently re-run them without a buildable target.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch targets the brief's exact criterion — a committed `ChunkRef` with `placement: vec![]` now contributes its identity-fallback fragments to the GC reference set (`patch.diff` gc.rs hunk @186, `tests/gc.rs:72-128`). Spec is well-formed; scope matches "GC reference set, not a refactor." |
| C2 Reproduction (red pre-fix) | PASS | Re-derived red from the diff: pre-fix `referenced_fragments` iterated `placement.iter().enumerate()` (empty → empty set) so the expired orphan reclaims `frag(chunk,0)` and `d0.get_fragment` returns `None`, failing the new assert (`tests/gc.rs:102,124-127`). Test self-describes the flip (`tests/gc.rs:69-70`). No standalone C2 gate; basis is the diff logic. |
| C3 Change | PASS | Change replaces placement-only iteration with scheme-derived count `n` (None→1, RS{k,m}→k+m) + identity fallback `placement.get(i).copied().unwrap_or(u64::from(i))` (`patch.diff` gc.rs @189). Mirrors the read path the brief cites; single logical change, no unrelated edits. |
| C4 Verification (red→green) | PASS | Harness gates report `pass`: `C4-ci` (gating, full xtask ci) and `C4-verify` (per-fix red→green) in `check-gates.json:33-48`. Could not re-run locally (no target build); decision rests on the harness gate, not a claim in the diff. Target unreadability is a caveat, not a C4 FAIL. |
| C5 Causal adequacy | PASS | Root-cause fix, not a guard: it **transforms** the reference-set computation to equal the read path's resolved closure, removing the cause rather than probing around it — symptom-guard smell-test does **not** fire (no capability probe / fallback over a present capability). Caveat folded into T5/§notes: the read-path *fragment-count* equivalence (that read iterates exactly `n` per scheme) is taken from the brief's spec, not independently re-derived against `read.rs:fragment_dserver` (target unavailable). |
| T1 Structure | PASS | New test lands in the brief-named file `crates/custodian/tests/gc.rs` (`patch.diff:48`), reuses the existing harness (`MemMeta`, `MemDServer`, `reconcile_step`, `mark_orphaned`), added as its own `#[tokio::test]` (`tests/gc.rs:71-72`). |
| T2 Shape | PASS | Asserts both the reconcile outcome (`Reconciled::Satisfied`) and the durable effect (`d0.get_fragment(...).is_some()`) — it pins the *fragment survives* invariant, not just a return code (`tests/gc.rs:119-127`). |
| T3 Runtime | PASS | `C4-verify` per-fix gate reports the test actually runs and flips red→green (`check-gates.json:42-48`); the diff's revert recipe (`tests/gc.rs:68-70`) makes the red reachable. |
| T4 Contribution | PASS | Genuinely new coverage for "criterion 4" distinct from the adjacent `never_reclaims_a_referenced_fragment` test — exercises the empty-placement identity-fallback path that had none (`tests/gc.rs:55,72`). |
| T5 Judgment | NEEDS-HUMAN | Decision owed: is single-case coverage adequate for the brief's stated category? The fix handles the whole "committed-but-fallback-placed" category — `EcScheme::ReedSolomon{k,m}` and **short (non-empty but < n)** placement vectors — but the only regression exercises `EcScheme::None` + `placement: vec![]`, index 0 (`tests/gc.rs:84-88`). The RS `k+m` expansion and the partial-placement merge (`.get(i).unwrap_or(i)` where some `i` are present) ship untested; a future regression there would pass CI silently. Human should decide whether to require an RS / short-vector case. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does protecting the *full scheme-derived* fragment set (vs. only what the read path actually dereferences) leave GC over-conservative — retaining true garbage for committed records whose real placement is shorter than `k+m` — and is that the intended durability/space trade-off? Also confirm the inlined fallback in `gc.rs` (now a 3rd copy alongside `read.rs` and `reconstruction.rs:227-235`) is acceptable vs. the brief's *preferred* centralized helper; the drift risk between copies is the long-term re-bug surface. Human sign-off per `check-gates.json:105-111`. |

## Notes the human should weigh (non-gating)

- **Prior-art / scope:** brief documents the prior-art check by affected file path
  (`gc.rs`, `read.rs` history; `093732d`, `af4ab65`; no open PR) — mechanically settled,
  no NEEDS-HUMAN needed there. The patch touches only `gc.rs` + `tests/gc.rs` and does
  **not** edit `crates/core/src/read.rs`, so it sidesteps the brief's "conflicts-with 288"
  collision — a sound scope choice. No scope-creep observed.
- **Target-state caveat:** `$PDCA_TARGET` unresolved here; the `read.rs:99-105` /
  `reconstruction.rs:227-235` equivalence claims in the diff comments were not re-grounded
  on target source. If the read path's fragment *count* (not just dserver resolution)
  diverges from the scheme-derived `n` this patch uses, GC's protected set would not
  exactly equal the read closure — the crux the C5/Validation rows leave to the human.

### Advisory — codex

- NEEDS-HUMAN — `crates/custodian/tests/gc.rs:313` covers only `EcScheme::None`, so the regression never exercises the new Reed-Solomon `k + m` expansion in `referenced_fragments` at `crates/custodian/src/gc.rs:194`. The brief calls out scheme-aware expansion for empty/short RS placement vectors; an RS case with an orphan at index >0 would catch regressions in the count/fallback logic.
- NEEDS-HUMAN — `crates/custodian/src/gc.rs:192` re-implements the placement expansion that the read/reconstruction paths already encode (`crates/core/src/read.rs:99`, `crates/custodian/src/reconstruction.rs:227`). Since GC safety depends on matching readable placement exactly, consider moving this into a shared `ChunkRef` helper so future placement semantics cannot drift across callers.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Decision owed: is single-case coverage adequate for the brief's stated category? The fix handles the whole "committed-but-fallback-placed" category — `EcScheme::ReedSolomon{k,m}` and **short (non-empty but < n)** placement vectors — but the only regression exercises `EcScheme::None` + `placement: vec![]`, index 0 (`tests/gc.rs:84-88`). The RS `k+m` expansion and the partial-placement merge (`.get(i).unwrap_or(i)` where some `i` are present) ship untested; a future regression there would pass CI silently. Human should decide whether to require an RS / short-vector case.
- [ ] Validation — fitness-to-purpose — Decision owed: does protecting the *full scheme-derived* fragment set (vs. only what the read path actually dereferences) leave GC over-conservative — retaining true garbage for committed records whose real placement is shorter than `k+m` — and is that the intended durability/space trade-off? Also confirm the inlined fallback in `gc.rs` (now a 3rd copy alongside `read.rs` and `reconstruction.rs:227-235`) is acceptable vs. the brief's *preferred* centralized helper; the drift risk between copies is the long-term re-bug surface. Human sign-off per `check-gates.json:105-111`.
- [ ] `crates/custodian/tests/gc.rs:313` covers only `EcScheme::None`, so the regression never exercises the new Reed-Solomon `k + m` expansion in `referenced_fragments` at `crates/custodian/src/gc.rs:194`. The brief calls out scheme-aware expansion for empty/short RS placement vectors; an RS case with an orphan at index >0 would catch regressions in the count/fallback logic.
- [ ] `crates/custodian/src/gc.rs:192` re-implements the placement expansion that the read/reconstruction paths already encode (`crates/core/src/read.rs:99`, `crates/custodian/src/reconstruction.rs:227`). Since GC safety depends on matching readable placement exactly, consider moving this into a shared `ChunkRef` helper so future placement semantics cannot drift across callers.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected: the reviewer's two substantive flags must be addressed before this can ship. (1) Test breadth — the only regression covers EcScheme::None + placement: vec![] at index 0; the RS{k,m} k+m expansion and the short/partial-placement merge (.get(i).unwrap_or(i) where some i are present) ship untested and a regression there would pass CI silently. Add an RS case (orphan at index > 0) and a short-vector case. (2) Centralize the placement expansion — gc.rs now holds a 3rd copy of the expansion/identity-fallback logic alongside read.rs and reconstruction.rs:227-235. GC safety depends on matching the read path's protected set exactly, so factor this into a single shared ChunkRef helper (the brief's preferred approach) instead of inlining a 3rd copy, so placement semantics cannot drift across callers. The fix's core logic is sound; rebuild from the same brief with these addressed.
- By / date: Eduard Ralph / 2026-06-28

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
