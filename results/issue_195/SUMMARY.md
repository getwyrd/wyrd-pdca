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

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['claude', '-p', '--agent', 'reviewer', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Write,Grep,Glob', '--add-dir', '/home/eddie/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.).

Failure class: **transient infra — safe to re-run.** The leaf exited non-zero with no output and retries did not recover, so it almost certainly hit a usage/rate limit or a transient API/network error rather than reviewing the diff; a sibling advisory leaf of a different family may already have covered it. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

### Advisory — codex

- .github/workflows/tier1-disk-faults.yml:66 runs the privileged harness as the default GitHub Actions user, but the test performs root-only operations (`losetup`, `dmsetup`, `mount`, and `/proc/sys/vm/drop_caches`). On hosted Ubuntu runners that user has passwordless sudo, not root, so the off-Check Tier-1 job will fail before exercising the harness unless this step runs the xtask under `sudo` or the test shells out through `sudo`.
- NEEDS-HUMAN — crates/custodian/tests/tier1_disk_faults.rs:353 still injects the scrub-leg fault by directly rewriting the fragment file, and the only device-mapper transition in the runtime scenario is the later `dm-error` load at crates/custodian/tests/tier1_disk_faults.rs:425. If the carried-forward requirement still means the real privileged campaign must exercise a `dm-flakey`/device-mapper fault for the scrub leg, this patch has not addressed that prior advisory concern.
- crates/custodian/tests/tier1_disk_faults.rs:180 keeps local copies of the dm-table helpers "to avoid cross-crate dep", and the runtime verdict is asserted directly at crates/custodian/tests/tier1_disk_faults.rs:604 instead of consuming `xtask::disk_faults` helpers. That leaves the Check-running orchestration tests partially decoupled from the ignored scenario, so they can validate helper logic that the privileged harness does not actually use.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [x] crates/custodian/tests/tier1_disk_faults.rs:353 still injects the scrub-leg fault by directly rewriting the fragment file, and the only device-mapper transition in the runtime scenario is the later `dm-error` load at crates/custodian/tests/tier1_disk_faults.rs:425. If the carried-forward requirement still means the real privileged campaign must exercise a `dm-flakey`/device-mapper fault for the scrub leg, this patch has not addressed that prior advisory concern.

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
- tier1-disk-faults.yml:66 runs the privileged harness as the default Actions user without sudo — off-Check Tier-1 job may fail before exercising the harness (codex advisory, non-blocking).
