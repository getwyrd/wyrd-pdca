# Result — issue 196 / tier2-kill-reconstruct-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: M3.8 (#146) shipped only the Tier-0 DST campaign. The Tier-2 leg of the
- Success criterion: Real in-repo Tier-2 kill-and-reconstruct harness code exists and
- Repo + branch target: getwyrd/wyrd @ main   (no maintenance branches today; INTEGRATION §2)
- Scope (one logical fix) / out of scope: Build the Tier-2 single-node kill-and-reconstruct harness as **real in-repo

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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

# Check review — issue_196 / tier2-kill-reconstruct-harness (iteration 3, advisory)

**Grounding caveat.** `$PDCA_TARGET` could not be resolved (env access blocked in this
sandbox) and ~18 sibling `wyrd` checkouts exist under `/home/eddie/wyrd` — grounding into
an arbitrary one is forbidden. Citations below are therefore grounded on `patch.diff`.
The deterministic `C4-ci` gate (`./engine/xtask.sh ci: all checks passed`,
check-gates.json:33-39) is the harness's own re-run against the real target and confirms
the patch compiles/type-checks there — so the cross-crate `crate::*` references
(`DSERVER_COUNT`, `TIER2_PROJECT`, `compose_up`, `resolve_endpoints`, `finalize_panic_safe`)
and the `wyrd_custodian` API signature resolve on the target; a missing symbol would have
failed `cargo test --workspace`. I trust that re-run; I did not independently rebuild.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion (brief.md:18-33) is "real in-repo Tier-2 harness replacing the `WYRD_TIER2_CMD` external-command bypass, test-exercised at Check." Patch removes the `execute(..., "WYRD_TIER2_CMD")` shell-out and replaces it with in-repo compose-up/kill/scenario orchestration (patch.diff:61-148). Matches spec intent. |
| C2 Reproduction (red pre-fix) | PASS | "Red" is criterion-absence + a *demonstrated* red (stub a helper → unit test fails), per Verification posture (brief.md:120-123). Gate `C4-verify` re-ran it: "red without the fix, green with it" (check-gates.json:42-48). Helper unit tests (patch.diff:550-661) are the flippable seam. |
| C3 Change | PASS | Single coherent contribution: new xtask orchestration module + scenario test + privileged CI workflow + deps, all serving the one feature (patch.diff:33-148, 245-1029, 1030-1101, 163-244). New-feature/infra work — minimalism does not govern (brief.md:45-47). |
| C4 Verification (red→green) | PASS | Both gating/advisory C4 rows pass per harness re-run: `C4-ci` "all checks passed" (check-gates.json:33-39) and `C4-verify` "red→green" (42-48). Per posture, the live privileged run is off-Check supplementary evidence, not the Check-gating condition (brief.md:27-31). No stale-target FAIL fabricated. |
| C5 Causal adequacy | PASS | Root cause (Tier-2 leg shipped as inert dispatch shelling to an undefined `WYRD_TIER2_CMD`) is genuinely removed: `run_kill_reconstruct` now stands up the cluster, kills a D server, and drives the production `reconcile_step → reconstruction::reconcile` path (patch.diff:79-108, 848-862). Smell-test: the retained `tool_available("docker")`/`opted_in("WYRD_TIER2")` early-return (71-77) is the brief-mandated deferred-posture gate (ADR-0016), not a probe papering over a load-time side effect — smell-test does not fire. Adequacy of the in-memory metadata seam is raised under T5/Validation, not here. |
| T1 Structure | PASS | `mod kill_reconstruct;` wired into xtask (patch.diff:159); scenario test sits as sibling to `tier2_integration.rs` (245-249); workflow added under `.github/workflows/` (163-167); Cargo deps (`wyrd-custodian`, `wyrd-coordination-mem`) added with Cargo.lock updated (5-29). Coheres with the cited container precedent. |
| T2 Shape | PASS | Mirrors the verified Tier-2 container precedent (connect-retry 668-680 ↔ tier2_integration; compose/finalize plumbing reused 79-108). Born-at-tier coverage is two-part as the brief requires (brief.md:98-111): #[ignore]d scenario type-checked at Check + non-ignored helper/orchestration unit tests. Iter-1 orphaned-helper defect resolved — helpers re-homed and called (patch.diff:915,970,982). |
| T3 Runtime | PASS | Helper + orchestration unit tests are non-`#[ignore]`d and run inside `cargo xtask ci` (patch.diff:550-661, 1084-1100); scenario body is `#[ignore]`d (705) so it only type-checks at Check. Gate re-run is green (check-gates.json:33-48). Live `--ignored` runtime outcome is off-Check → see Validation. |
| T4 Contribution | PASS | This was the iter-1 reject axis. `assert_garbage_not_corruption` is now correctly oriented: `Err` only when `!committed_placement_has_victim` (patch.diff:475-482) — passes when the victim remains in committed placement (fully-old inode), matching the scenario assert (912-916) and the DST property the brief cited. Its three unit tests encode the corrected truth table, incl. `(true,true).is_ok()` (551-558) and the `hybrid` failure (573-586). `select_victim_index`/`victim_container_name` genuinely wired (84-86) and unit-tested (1084-1100). |
| T5 Judgment | NEEDS-HUMAN | Decision owed: ratify the fidelity reinterpretation. The harness stores metadata via in-process `MemMeta`/`CrashMeta` (patch.diff:345-436) while proposal 0005 §13.2 mandates Tier-2 over **real NVMe/fsync**. Fragments do hit real gRPC D-server containers, but the durability/commit metadata does not. Author must confirm this seam still honours the §13.2 mandate, AND that `CrashMeta`'s "any commit with a positive precondition" crash model (429-435) truly singles out only the production version-conditional repoint commit — if intent/enqueue/drain commits also carry positive preconditions, the crash fires on the wrong commit. Neither is mechanically settleable from the artifacts. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human-only at sign-off (check-gates.json:104-112). Confirm: (1) the privileged `WYRD_TIER2=1` job (patch.diff:163-244) runs the `#[ignore]`d scenario **green on a real node** (NVMe/fsync, docker) — the deferred deliverable's only execution evidence; (2) the gate base actually contains **merged #195** (brief.md:51-61: shared edits to `faults.rs`/`main.rs`) so this builds on merged code, not a collision — an ordering/target-base question this advisory cannot settle; (3) validation is against clean upstream `main`, not a drifted worktree (docs/fork-discipline.md §4), given the multiple sibling checkouts observed. |

## Notes for §6 (human must clear)
- **T5 fidelity:** in-memory `MemMeta`/`CrashMeta` metadata seam vs proposal 0005 §13.2 "real NVMe/fsync" Tier-2 mandate — author must ratify the reinterpretation (carried from iter-2 §6, still open).
- **T5 crash-model fidelity:** verify `CrashMeta`'s single-positive-precondition crash model (patch.diff:429-435) matches the production reconstruction commit sequence.
- **Validation:** confirm the privileged `WYRD_TIER2=1` job runs the scenario green on a real node.
- **Validation / ordering:** confirm the gate base contains merged #195 (shared `faults.rs`/`main.rs` edits) before this lands.

## Re-derivation summary
The two prior rejection axes are addressed in this iteration:
- iter-1 (T4): the inverted `assert_garbage_not_corruption` is corrected and its unit tests
  re-encode the corrected truth table; the previously orphaned helpers are re-homed into the
  scenario crate and are now called by the scenario + covered by non-ignored unit tests.
- iter-2 (broken intra-doc links): `kill_reconstruct.rs` now names the three helpers as plain
  code spans (patch.diff:1045-1049), not `[…]` links; the scenario file's `[…]` links
  (patch.diff:282-284, 271) now resolve intra-file since the helpers live there; the old
  `faults.rs` dangling link is gone. No `cargo doc` step gates `ci`, so this class is also
  non-gating — but it now also resolves. I could not run `cargo doc` to be categorical.
No grounds for a blocking FAIL: both C4 rows are green per the harness re-run and the named
defects are fixed. The remaining risk is entirely in the off-Check / semantic-fidelity space,
surfaced above as NEEDS-HUMAN.

### Advisory — codex

- NEEDS-HUMAN — crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:95: the Tier-2 scenario still uses an in-memory `MetadataStore`/`CrashMeta` for the authoritative inode/CAS path and crash injection while the D-server fragments run through real containers. This may be an acceptable seam reuse, but it needs human ratification against the brief's "single real node, real NVMe/fsync" mandate because the metadata durability/commit path is not itself backed by a real fsyncing store.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Decision owed: ratify the fidelity reinterpretation. The harness stores metadata via in-process `MemMeta`/`CrashMeta` (patch.diff:345-436) while proposal 0005 §13.2 mandates Tier-2 over **real NVMe/fsync**. Fragments do hit real gRPC D-server containers, but the durability/commit metadata does not. Author must confirm this seam still honours the §13.2 mandate, AND that `CrashMeta`'s "any commit with a positive precondition" crash model (429-435) truly singles out only the production version-conditional repoint commit — if intent/enqueue/drain commits also carry positive preconditions, the crash fires on the wrong commit. Neither is mechanically settleable from the artifacts.
- [x] Validation — fitness-to-purpose — Human-only at sign-off (check-gates.json:104-112). Confirm: (1) the privileged `WYRD_TIER2=1` job (patch.diff:163-244) runs the `#[ignore]`d scenario **green on a real node** (NVMe/fsync, docker) — the deferred deliverable's only execution evidence; (2) the gate base actually contains **merged #195** (brief.md:51-61: shared edits to `faults.rs`/`main.rs`) so this builds on merged code, not a collision — an ordering/target-base question this advisory cannot settle; (3) validation is against clean upstream `main`, not a drifted worktree (docs/fork-discipline.md §4), given the multiple sibling checkouts observed.
- [x] crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:95: the Tier-2 scenario still uses an in-memory `MetadataStore`/`CrashMeta` for the authoritative inode/CAS path and crash injection while the D-server fragments run through real containers. This may be an acceptable seam reuse, but it needs human ratification against the brief's "single real node, real NVMe/fsync" mandate because the metadata durability/commit path is not itself backed by a real fsyncing store.

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
- By / date: Eduard Ralph / 2026-06-25

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
