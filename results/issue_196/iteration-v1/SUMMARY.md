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

# Check review — issue_196 / tier2-kill-reconstruct-harness

Advisory, artifact-only. Grounded on the target worktree `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt`), which already carries the patch. Re-derived
findings below; deterministic gates (C4-ci, C4-verify) re-confirmed green via
`check-gates.json` (`overall: pass`). build-notes.md withheld by design.

## Grounding performed
- `WYRD_TIER2_CMD` external-command bypass **fully removed** — `grep` over
  `xtask/` + `crates/` returns no matches; this was the core deliverable.
- Production reconstruction path is real, not reimplemented: scenario test calls
  `wyrd_custodian::reconcile_step` → `reconstruction::reconcile`
  (`crates/custodian/src/reconciliation.rs:65`, `lib.rs:39`); all imported symbols
  (`ReconstructionContext`, `Topology`, `FencedZone`, `Custodian`, `Reconciled`)
  resolve in the target (`lib.rs:37-50`). reconcile_step's 7-arg signature matches
  the test's call site.
- #195 dependency ordering held: `run_disk_faults`/`run_jepsen` are present in the
  base (`xtask/src/faults.rs:114,127`), so Do built on merged 195, no collision.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief deliverable is unambiguous (real in-repo Tier-2 harness replacing the `WYRD_TIER2_CMD` shell-out); patch targets exactly it. `faults.rs:153` now orchestrates compose/kill/test instead of `execute(...,"WYRD_TIER2_CMD")`. |
| C2 Reproduction (red pre-fix) | PASS | C4-verify gate re-ran red→green (`check-gates.json` C4-verify pass). Pre-fix state = criterion-absence (no harness, bypass shell-out); the demonstrated-red (stub a helper → unit test fails) lives in withheld build-notes but the gate confirms the seam is load-bearing. |
| C3 Change | PASS | Change is the harness: orchestration `faults.rs:153-237`, scenario test `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`, unit-tested helpers `xtask/src/kill_reconstruct.rs`, privileged CI `.github/workflows/tier2-kill-reconstruct.yml`. All referenced compose/finalize plumbing exists (`xtask/src/main.rs:229,299,201,174,252,243`). |
| C4 Verification (red→green) | PASS | Both deterministic gates green: C4-ci (`cargo xtask ci` — fmt/clippy/build/test/deny/conformance/dst) and C4-verify. `cargo test --workspace` compiles+type-checks the `#[ignore]`d scenario, proving it is real API-bound Rust, not inert dispatch. |
| C5 Causal adequacy | PASS | Harness drives the **real** production reconstruction control point (`reconcile_step`→`reconstruction::reconcile`), not a parallel reimplementation (ADR-0009 satisfied). Symptom-guard smell-test: the `WYRD_TIER2` opt-in + `WYRD_DSERVER_ENDPOINTS` unset→skip are **deferred-tier posture** (ADR-0016), not a capability probe papering a load-time side effect — does not trigger. |
| T1 Structure | PASS | New `mod kill_reconstruct` (`main.rs:28`), new sibling test file, new workflow; dev-deps `wyrd-custodian`/`wyrd-coordination-mem`/`async-trait` added to `crates/chunkstore-grpc/Cargo.toml` and used (machete-clean). Mirrors the verified `run_integration`/`tier2_integration.rs` precedent. |
| T2 Shape | PASS (with defect to fix) | Mirrors sibling precedents cleanly, BUT broken intra-doc link `xtask/src/faults.rs:149` references `crate::kill_reconstruct_test::tier2_kill_reconstruct` — no such module (the scenario lives in a *different* crate). `broken_intra_doc_links = "deny"` (root `Cargo.toml:170`) would fail `cargo doc`; non-gating only because no CI step runs rustdoc. Fix the link. |
| T3 Runtime | PASS | What runs at Check is green: xtask unit tests (`kill_reconstruct.rs` tests) pass; scenario compiles. The live containerized run (NVMe/fsync/docker) is **deferred off-Check** and is unobserved from this worktree — see T5/Validation. |
| T4 Contribution | NEEDS-HUMAN | The three assertion helpers `assert_redundancy_outcome`/`assert_distinct_domains`/`assert_garbage_not_corruption` (`xtask/src/kill_reconstruct.rs:42,73,109`) are `#[cfg(test)]`-only and called by **nothing** but their own unit tests — the scenario test re-derives those assertions inline and cannot reach them (separate crate). Decide: does this literal satisfaction of the brief's "assertion helpers unit-tested" constitute load-bearing born-at-tier coverage, or is it test-only duplication that proves nothing about the real harness? (`select_victim_index`/`victim_container_name` ARE genuinely wired into `faults.rs:175` and tested — that coverage is real.) |
| T5 Judgment | NEEDS-HUMAN | Deferred-posture call the brief itself flags: the harness is *built and compiled* at Check, but the off-Check privileged green (real node, docker, `cargo xtask kill-reconstruct`) is **unobserved here**. Human must confirm the privileged Tier-2 job actually runs green before treating this as "deferred-verified" rather than "separate unbuilt work item" (brief §Verification-posture). Fork-discipline: validate against clean upstream `main`, not a drifted branch. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Does this harness, as exercised, actually honour the M3 Tier-2 mandate (proposal 0005 §13.2) — i.e. a real D-server kill driving real reconstruction with the durability outcome asserted on a live cluster — vs. only proving compile-time wiring + isolated helper logic at Check? Owner: human at sign-off (privileged-run evidence + ADR-0009 promote-to-regression intent). |

## Notes for §6 (human must clear)
1. **T4** — orphaned `#[cfg(test)]` assertion helpers: load-bearing coverage or test-only duplication of the scenario test's inline asserts?
2. **T5** — confirm the privileged `WYRD_TIER2=1` job (`cargo xtask kill-reconstruct`) is observed green off-Check; otherwise this is a separate unbuilt work item, not a deferred line.
3. **Validation** — fitness-to-purpose of the Tier-2 kill-and-reconstruct campaign against clean upstream `main`.
4. **T2 (advisory, non-gating)** — fix broken intra-doc link at `xtask/src/faults.rs:149` (`crate::kill_reconstruct_test::tier2_kill_reconstruct` resolves to nothing; would fail `cargo doc` under the deny lint).

## Prior-art / fork-discipline (mechanically settled where possible)
- Prior-art by affected file path documented in brief and corroborated: `run_kill_reconstruct` from #146 is the scaffolding being replaced; #195's Tier-1 runners co-present in base (`faults.rs:114,127`), so the "Depends on (merged): 195" ordering held — no merge collision.
- Cross-version / fork-discipline correctness on the target cannot be fully mechanically settled (clean-upstream validation) → folded into T5 / Validation NEEDS-HUMAN.

### Advisory — codex

- `xtask/src/kill_reconstruct.rs:123` — `assert_garbage_not_corruption` encodes the crash invariant backwards for the committed placement: it fails when `committed_placement_has_victim` is true, but a crash before the version-conditional commit should leave the inode fully old, which means the victim is still in the committed placement. The new live scenario asserts that same expected state at `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:414`, and the existing DST property does likewise at `crates/dst/tests/custodian.rs:617`. As written, the Check-time helper/unit test accepts a post-crash placement that no longer references the victim, weakening the born-at-tier coverage for the garbage-not-corruption invariant.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — The three assertion helpers `assert_redundancy_outcome`/`assert_distinct_domains`/`assert_garbage_not_corruption` (`xtask/src/kill_reconstruct.rs:42,73,109`) are `#[cfg(test)]`-only and called by **nothing** but their own unit tests — the scenario test re-derives those assertions inline and cannot reach them (separate crate). Decide: does this literal satisfaction of the brief's "assertion helpers unit-tested" constitute load-bearing born-at-tier coverage, or is it test-only duplication that proves nothing about the real harness? (`select_victim_index`/`victim_container_name` ARE genuinely wired into `faults.rs:175` and tested — that coverage is real.)
- [ ] T5 Judgment — Deferred-posture call the brief itself flags: the harness is *built and compiled* at Check, but the off-Check privileged green (real node, docker, `cargo xtask kill-reconstruct`) is **unobserved here**. Human must confirm the privileged Tier-2 job actually runs green before treating this as "deferred-verified" rather than "separate unbuilt work item" (brief §Verification-posture). Fork-discipline: validate against clean upstream `main`, not a drifted branch.
- [ ] Validation — fitness-to-purpose — Does this harness, as exercised, actually honour the M3 Tier-2 mandate (proposal 0005 §13.2) — i.e. a real D-server kill driving real reconstruction with the durability outcome asserted on a live cluster — vs. only proving compile-time wiring + isolated helper logic at Check? Owner: human at sign-off (privileged-run evidence + ADR-0009 promote-to-regression intent).

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
- Iteration delta (if iterating): Rejected on T4 (born-at-tier coverage must be correct, and one helper is not). What to change next (issue_196): 1. `assert_garbage_not_corruption` (xtask/src/kill_reconstruct.rs:109) is logically inverted. After a crash before the version-conditional commit the inode is FULLY OLD, so the victim IS still in the committed placement. The helper must PASS when committed_placement_has_victim == true (not false), matching the live scenario test (tier2_kill_reconstruct.rs:537) and the DST property (crates/dst/tests/custodian.rs:617). Update the helper AND its unit test (kill_reconstruct.rs:1043) together — both currently encode the same inversion, so the green is vacuous. 2. Resolve the orphaned-helper architecture: the three #[cfg(test)] assert_* helpers are unreachable from the real harness (separate crate) and merely duplicate the scenario test's inline asserts. Either wire them into the real scenario/harness path so the born-at-tier coverage is load-bearing, or drop the dead duplication — do not keep both. (select_victim_index / victim_container_name are genuinely wired and fine.) 3. While in here, fix the non-gating broken intra-doc link at faults.rs:149 (crate::kill_reconstruct_test::tier2_kill_reconstruct resolves to nothing; the scenario lives in a different crate; would fail cargo doc under broken_intra_doc_links = "deny"). T5 / Validation (privileged WYRD_TIER2=1 green, fitness-to-purpose) were not reached this pass — re-evaluate after the T4 fix lands.
- By / date: Eduard Ralph / 2026-06-25

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
