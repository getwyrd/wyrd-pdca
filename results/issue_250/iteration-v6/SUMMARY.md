# Result — issue 250 / tier1-jepsen-consistency-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 consistency-over-repair leg of proposal 0005 §13.2 (`0005:408`) was
- Success criterion: **STRUCTURAL DECISION (the maintainer chose Option B, mirroring the
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
- Scope (one logical fix) / out of scope: Build the Tier-1 consistency-over-repair leg as an in-repo Rust scenario

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
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

# Check review — issue 250 / tier1-jepsen-consistency-harness

Advisory, artifact-only, decorrelated from the builder (build-notes.md withheld).
Target grounded at `$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt` — patch is applied
there and readable; gating `cargo xtask ci` recorded green (not stale/unreadable).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief success criterion is well-formed and decomposed into three *binding-at-Check* parts (dispatch rewire, real buildable scenario driving the production reconcile path, privileged workflow) with the live-green explicitly carved out as deferred — Check has a concrete, demonstrable target. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Born-at-tier net-new: the only red is the pre-fix *non-existence* of `jepsen_required_tool` (a compile error), and `run-verify.sh` reports the routing test PASSES without the fix — there is no behavioral assertion-flip. The flippable test (`faults.rs:426`) only asserts a string constant `== "docker"`, which would stay green even if `run_jepsen` still shelled out. Decision owed: accept criterion-absence as the red (pre-declared posture), or require a routing test that behaviorally proves the external-cmd path is gone — why it matters: a tautological red is the #146 "deferred ≠ unbuilt" failure mode the brief is meant to close. |
| C3 Change | PASS | `faults.rs:188-260` rewires `run_jepsen` from `execute(…, "WYRD_TIER1_JEPSEN_CMD")` to in-repo `cargo test --ignored` dispatch, and makes `execute`/`run_shell` `#[cfg(test)]` (`faults.rs:76,~165`) — the compiler now *enforces* the dead external shell-out is unreachable in production. Mirrors the merged #195/#196 sibling shape; grounds on target. |
| C4 Verification (red→green) | PASS | Gating `cargo xtask ci` recorded green (check-gates `C4-ci`) — the brief's binding Check evidence: the `#[ignore]`d harness compiles/type-checks against real, exported production APIs (`reconcile_step`, `ReconstructionContext`, `reconstruction::reconcile`, `repair::{enqueue_repair,intact_shard}` all confirmed present on target). The non-gating per-fix red→green FAIL (`C4-verify`) is the pre-declared born-at-tier artifact, surfaced at C2 — not a patch defect, and not treated as a blocking ordering FAIL. |
| C5 Causal adequacy | PASS | Root cause = the leg was never built (inert dispatch to an absent external cmd); fix builds a real scenario that genuinely traverses production `reconcile_step → reconstruction::reconcile` and asserts ADR-0015. Symptom-guard smell-test does NOT fire: the `docker` probe + `WYRD_TIER1` opt-in is the sanctioned ADR-0016 tier-gating pattern (mirrors `run_kill_reconstruct`), not a capability probe papering over a load-time side effect. Caveat (→ Validation): the "partition" is modeled by `CrashMeta` commit-intercept over an *in-memory* metadata store; D-servers are real containers but the metadata partition is simulated. |
| T1 Structure | PASS | New scenario `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` sits as a sibling to `tier2_kill_reconstruct.rs`; new `tier1-jepsen.yml` modeled on `tier2-kill-reconstruct.yml`; dispatch counts are internally consistent (`KR_DSERVER_COUNT` = `DSERVER_COUNT`(9)+1 = 10 = scenario `JC_DSERVER_COUNT`). |
| T2 Shape | PASS | `#[ignore]`d three-phase scenario plus six non-`#[ignore]` helper unit tests, each with an explicit negative control (`faults`/scenario `:418-491`) proving the oracle is load-bearing per ADR-0009 — matches the brief's forcing-function honesty requirement and the sibling precedent. |
| T3 Runtime | NEEDS-HUMAN | The live run (real cluster, `docker kill`, crash/partition mid-repair) is `#[ignore]`d and executes only in privileged `tier1-jepsen.yml`; nothing exercises the compose reuse, victim-container naming, or 10-endpoint resolution at Check. Decision owed: maintainer must confirm the first `workflow_dispatch` run is green — why it matters: the only place the assembled cluster wiring (vs. compile-checked types) is actually proven. |
| T4 Contribution | NEEDS-HUMAN | Option B (in-repo Rust scenario) changes the *how* of accepted proposal 0005, which names "Jepsen" literally (`0005:408`). Decision owed: maintainer weighs a clarifying/superseding ADR and files the two pre-declared follow-ons (literal-Jepsen credibility artifact; missing-fragment **detection** product gap) — why it matters: without the ADR the repo's literal-Jepsen credibility commitment is silently redefined, and the enqueue stand-in's production gap stays untracked. |
| T5 Judgment | NEEDS-HUMAN | Two contested judgment calls the artifact cannot mechanically settle: (a) that an in-repo Rust scenario fulfils proposal 0005 §13.2 after 5 rejected Option-A iterations; (b) that `repair::enqueue_repair("tier1-jepsen-test")` (scenario `:642`) is an acceptable sanctioned stand-in for the absent production missing-fragment detection path (per #196 precedent). Decision owed: maintainer confirms both are the intended substance, not scope-shrink. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Deferred live green — ADR-0015 holding over the repair path (read-after-commit, no torn/stale reads, commit-point-atomic) — is confirmed only off-Check by the maintainer reviewing the first `tier1-jepsen.yml` run. The maintainer must also judge whether the `CrashMeta`-modeled partition over an in-memory metadata store is a faithful enough proxy for "real network partitions" to count as the leg being genuinely exercised. |

## Notes
- Fork-discipline (`docs/fork-discipline.md` §3/§4) does not bite: target is `getwyrd/wyrd @ main`, net-new on a clean upstream `main` (INTEGRATION §2, no maintenance branches) — not a cross-version cherry-pick.
- Prior-art check ran by affected file path: `faults.rs` history (#195 `0b5fea3`, #196 `02983aa`), and the two net-new files do not pre-exist on the target. The *semantic* duplication question (Option B vs the literal-Jepsen this supersedes) is the T4/T5 human item above, not a mechanical duplicate.
- Minor (off-Check, non-blocking): `tier1-jepsen.yml` uploads `target/tier2-logs/` — correct only because `compose_logs` writes there by default; the comment acknowledges the borrowed path.

### Advisory — codex

- NEEDS-HUMAN — xtask/src/faults.rs:426: The dispatch regression test only asserts that `jepsen_required_tool()` returns `"docker"`; it does not exercise `run_jepsen`'s `Plan::Run` branch or assert that the command path reaches `run_jepsen_consistency_test` instead of an external env-var shellout. That leaves the brief's C4 red/green dispatch criterion weak, matching the non-gating verify report that the test is not red pre-fix.
- crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:739: The scenario comment says it performs the same total-fragment and distinct-domain check as `tier2_kill_reconstruct`, but the code only checks length and that slot 0 moved to the spare. It does not reuse the sibling `assert_distinct_domains` oracle, so a placement with duplicate failure domains among the surviving slots would still pass the new Tier-1 consistency harness.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Born-at-tier net-new: the only red is the pre-fix *non-existence* of `jepsen_required_tool` (a compile error), and `run-verify.sh` reports the routing test PASSES without the fix — there is no behavioral assertion-flip. The flippable test (`faults.rs:426`) only asserts a string constant `== "docker"`, which would stay green even if `run_jepsen` still shelled out. Decision owed: accept criterion-absence as the red (pre-declared posture), or require a routing test that behaviorally proves the external-cmd path is gone — why it matters: a tautological red is the #146 "deferred ≠ unbuilt" failure mode the brief is meant to close.
- [x] T3 Runtime — The live run (real cluster, `docker kill`, crash/partition mid-repair) is `#[ignore]`d and executes only in privileged `tier1-jepsen.yml`; nothing exercises the compose reuse, victim-container naming, or 10-endpoint resolution at Check. Decision owed: maintainer must confirm the first `workflow_dispatch` run is green — why it matters: the only place the assembled cluster wiring (vs. compile-checked types) is actually proven.
- [x] T4 Contribution — Option B (in-repo Rust scenario) changes the *how* of accepted proposal 0005, which names "Jepsen" literally (`0005:408`). Decision owed: maintainer weighs a clarifying/superseding ADR and files the two pre-declared follow-ons (literal-Jepsen credibility artifact; missing-fragment **detection** product gap) — why it matters: without the ADR the repo's literal-Jepsen credibility commitment is silently redefined, and the enqueue stand-in's production gap stays untracked.
- [x] T5 Judgment — Two contested judgment calls the artifact cannot mechanically settle: (a) that an in-repo Rust scenario fulfils proposal 0005 §13.2 after 5 rejected Option-A iterations; (b) that `repair::enqueue_repair("tier1-jepsen-test")` (scenario `:642`) is an acceptable sanctioned stand-in for the absent production missing-fragment detection path (per #196 precedent). Decision owed: maintainer confirms both are the intended substance, not scope-shrink.
- [x] Validation — fitness-to-purpose — Deferred live green — ADR-0015 holding over the repair path (read-after-commit, no torn/stale reads, commit-point-atomic) — is confirmed only off-Check by the maintainer reviewing the first `tier1-jepsen.yml` run. The maintainer must also judge whether the `CrashMeta`-modeled partition over an in-memory metadata store is a faithful enough proxy for "real network partitions" to count as the leg being genuinely exercised.
- [ ] xtask/src/faults.rs:426: The dispatch regression test only asserts that `jepsen_required_tool()` returns `"docker"`; it does not exercise `run_jepsen`'s `Plan::Run` branch or assert that the command path reaches `run_jepsen_consistency_test` instead of an external env-var shellout. That leaves the brief's C4 red/green dispatch criterion weak, matching the non-gating verify report that the test is not red pre-fix.

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
- Iteration delta (if iterating): Rejected on C2 (red pre-fix) and the codex faults.rs:426 note — the same defect: the dispatch regression test is tautological. It only asserts `jepsen_required_tool() == "docker"`, which stays green even if `run_jepsen` still shelled out to the external `WYRD_TIER1_JEPSEN_CMD` command. `run-verify.sh` confirms the test PASSES without the fix — no behavioral red, the #146 "deferred ≠ unbuilt" trap. What to change next: add a behavioral routing test that exercises `run_jepsen`'s `Plan::Run` branch and proves the command path reaches `run_jepsen_consistency_test` (the in-repo `cargo test --ignored` dispatch) rather than the external env-var shell-out. It must be genuinely red pre-fix (fail when the dispatch still routes to the external command) and green post-fix, so the per-fix red→green (C4-verify) holds. Not in dispute / accepted this round (carry forward, do not re-litigate): T3 Runtime and Validation accepted on the deferred-posture (first `tier1-jepsen.yml` workflow_dispatch run to be verified green once checked in); T4 Contribution and T5 Judgment confirmed fine (Option B substance + the `enqueue_repair` #196 stand-in).
- By / date: Eduard Ralph / 2026-06-27

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
