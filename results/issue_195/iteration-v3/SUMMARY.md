# Result — issue 195 / tier1-disk-fault-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: M3.8 (#146) shipped only the Tier-0 DST campaign. The Tier-1 disk-fault leg
- Success criterion: Real in-repo Tier-1 disk-fault **harness** code exists and is
- Repo + branch target: getwyrd/wyrd @ main   (no maintenance branches today; INTEGRATION §2)
- Scope (one logical fix) / out of scope: Build the Tier-1 disk-fault harness as **real in-repo Rust**, mirroring the

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

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['claude', '-p', '--agent', 'reviewer', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Write,Grep,Glob', '--add-dir', '/home/eddie/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.).

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

### Advisory — codex

- `xtask/src/disk_faults.rs:120` — All device-table and verdict logic is inside `#[cfg(test)]`; the real scenario independently builds its tables at `crates/custodian/tests/tier1_disk_faults.rs:215` and never calls either verdict helper. The unit tests therefore validate a shadow implementation and can remain green when the runtime harness is removed or broken, matching the reported C4-verify no-red result. Move the shared plan/verdict types into normal code and have the scenario consume them.
- `crates/custodian/tests/tier1_disk_faults.rs:300` — The scrub leg corrupts the fragment with a direct `std::fs::write`; the only runtime device-mapper transitions are linear setup and `dm-error` at line 375. The advertised `dm-flakey` phase exists only in the test-only helper, so the privileged campaign never exercises the required flakey block-layer fault.
- `crates/custodian/tests/tier1_disk_faults.rs:369` — Cache eviction is explicitly best-effort, and failure is accepted because reconstruction can pass through the already-cached `IntegrityFault` path. Since scrub just read the corrupt fragment, `get_fragment` can be served from page cache after switching to `dm-error`; the scenario can therefore pass without observing any block-layer EIO or exercising #251's EIO read-around. Make eviction/proof of EIO mandatory before accepting the reconstruction verdict.
- `crates/custodian/tests/tier1_disk_faults.rs:33` — The source says a dedicated `tier1-disk-faults.yml` job performs the privileged run, but no such workflow exists under `.github/workflows` and the patch adds none. Consequently the ignored real-device test has no in-repo CI execution path, so the required off-Check green cannot be produced automatically.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

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
- Iteration delta (if iterating): Rejected: the named test does not go red without the fix (C4-verify FAIL), and no primary advisory review exists (Claude reviewer leaf failed to run). The codex cross-vendor review independently confirms the harness validates a shadow implementation. Next attempt must produce a real advisory review AND address all four codex concerns: 1. xtask/src/disk_faults.rs:120 — device-table and verdict logic live entirely in #[cfg(test)]; the real scenario (tier1_disk_faults.rs:215) rebuilds its own tables and never calls the verdict helpers, so unit tests validate a shadow implementation and stay green when the runtime harness is removed/broken (matches the C4-verify no-red result). Move the shared plan/verdict types into normal code and have the scenario consume them — and make the test red pre-fix. 2. tier1_disk_faults.rs:300 — scrub leg corrupts the fragment with a direct std::fs::write; the only runtime dm transitions are linear setup + dm-error at line 375. The advertised dm-flakey phase exists only in the test-only helper, so the privileged campaign never exercises the required flakey block-layer fault. 3. tier1_disk_faults.rs:369 — cache eviction is best-effort and failure is accepted; get_fragment can be served from page cache after switching to dm-error, so the scenario passes without observing any block-layer EIO (the #251 read-around it must prove). Make eviction/proof of EIO mandatory before accepting the reconstruction verdict. 4. tier1_disk_faults.rs:33 — references a dedicated tier1-disk-faults.yml privileged workflow that does not exist under .github/workflows and the patch adds none; the ignored real-device test has no in-repo CI execution path, so the off-Check green cannot be produced automatically.
- By / date: Eduard Ralph / 2026-06-25

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
