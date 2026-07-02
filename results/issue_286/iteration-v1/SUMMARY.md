# Result — issue 286 / dserver-container-non-root-user

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The D-server runtime container image runs as root. The runtime stage of
- Success criterion: The runtime image declares a dedicated unprivileged user and sets
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: The D-server runtime image runs privileged because it never drops to a non-root

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['claude', '-p', '--agent', 'reviewer', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Write,Grep,Glob', '--add-dir', '/home/eddie/wyrd/wyrd.pdca-wt-l1', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.).

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

### Advisory — codex

# Advisory review — codex — NOT COMPLETED

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-codex.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'codex' did not produce findings (leaf failed: Command '['codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check']' returned non-zero exit status 1.); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [ ] advisory leaf 'codex' did not produce findings (leaf failed: Command '['codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

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
- Iteration delta (if iterating): Primary reason: the bundle has NO advisory review — both Check reviewer leaves crashed on this run (claude `reviewer` exit 1 / no output; codex exit 1 / no output), so no independent verdict exists and the bundle cannot be accepted. The rebuild's Check must yield a working advisory review before sign-off. The patch itself looked sound on inspection and is not the reason for iterating: Dockerfile adds a --system dserver:dserver user (10001:10001), pre-creates and chowns /data, and sets USER before ENTRYPOINT (correct ordering); the dserver_image.rs test enforces the non-root-USER-before-ENTRYPOINT seam and C4-verify passed red->green. Keep this approach unless the (now-running) reviewer surfaces a real defect. Note: the gating C4-ci failure (cargo test --workspace exit 101) was base-drift / transient, NOT this patch — re-running the identical command on the applied patch (base now at #402 merge) is green, exit 0, zero failures, and the new dserver_image.rs test runs. Not a reason to iterate.
- By / date: Eduard Ralph / 2026-07-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
