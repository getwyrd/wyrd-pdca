# Result — issue 330 / scrub-detect-missing-placed-fragment

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: On `origin/main` **no production path enqueues a repair obligation for a
- Success criterion: A scrub reconciliation pass (the production `reconcile_step` →
- Repo + branch target: getwyrd/wyrd @ main   (Wyrd has no maintenance branches; INTEGRATION §2)
- Scope (one logical fix) / out of scope: Close the missing-fragment detection gap: a committed-referenced fragment that

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — ./engine/xtask.sh: line 30: exec: cargo: not found
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied (not green).
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

# Check review — issue 330 / scrub-detect-missing-placed-fragment

**Task under review:** On `main`, no production maintenance path turns a *simply-absent*
committed-referenced fragment into a repair obligation — scrub only inspects fragments a
D server's `list_fragments()` enumerates, so a missing fragment is never even visited and
its `Ok(None)` fetch arm silently `continue`s. The fix must make a production scrub pass
enqueue a durable repair obligation for a placed-but-absent fragment, without false
positives for in-flight writes, pending-GC/expired-lease windows, or orphan fragments.

**Review conditions (caveats, not patch defects):**
- `$PDCA_TARGET` could not be resolved in this sandbox (env access blocked; the per-cycle
  worktree is not present in the review dir). Per protocol I ground citations on
  `patch.diff` alone and did not wander into other checkouts.
- No Rust toolchain is reachable here (`which cargo rustc` → non-zero; gate `C4-ci` reports
  `cargo: not found`). I therefore could **not** independently re-run red→green. Both C4
  gate failures trace to `cargo: not found` — a toolchain-absent environment, **not** a
  patch defect — so I do not raise them as a blocking C4 FAIL (that would fabricate an
  ordering-gate blocker). The red→green claim is assessed by inspection and handed to the
  human to execute with a real toolchain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch targets exactly the briefed gap: scrub now drives off the committed reference set and enqueues on placed-but-absent (`patch.diff` scrub.rs `Ok(None)` arm, +113–117; grouping +75–78). Out-of-scope killed/partitioned D-server case is naturally excluded because the outer loop still iterates `ctx.fleet`, so a referenced dserver absent from the fleet is never probed — matches brief scope. |
| C2 Reproduction (red pre-fix) | PASS | New test `detects_a_missing_placed_fragment...` (`patch.diff` +174–207) is genuinely fix-coupled: d0 holds no bytes, so pre-fix the `list_fragments()`-driven loop never visits the frag and asserts `Changed`/`vec![chunk]` fail; reverting only the `Ok(None)` arm to `continue` also flips it red (comment +170–173). Credible by inspection; not executed (no toolchain). |
| C3 Change | PASS | Minimal, coherent: group reference set by placed dserver, fetch each by id via existing `get_fragment`, treat `Ok(None)` as loss, new `emit_missing` mirrors `emit_corruption` (`patch.diff` +142–152). No signature churn beyond a `HashMap` import (+43). |
| C4 Verification (red→green) | NEEDS-HUMAN | Gate `C4-ci` and `C4-verify` both FAIL on `cargo: not found` (check-gates.json:37, :46) — toolchain-absent, not a patch defect; I confirmed no cargo/rustc here so I could not re-run. **Decision owed:** run `run-verify.sh` / `cargo xtask ci` on a machine with the Rust toolchain and confirm the two new tests are red pre-fix and green post-fix; the reported "RED with fix" is a build-can't-run artifact of the missing toolchain, not observed test failure. |
| C5 Causal adequacy | PASS | Symptom-guard smell-test does **not** fire: the fix adds no capability probe / runtime guard around a present capability — it removes the root gap (iteration bounded by `list_fragments()`) by driving detection off the committed reference set, so an absent fragment is now asked-for by id. Root cause, not a papered-over probe. |
| T1 Structure | PASS | Detection lands in the existing `scrub::reconcile` seam; reuses `repair::enqueue_repair` and the shared queue; `emit_missing` parallels the existing emit helpers (`patch.diff` +137–152). No layering violation (custodian gains no chunk-format knowledge). |
| T2 Shape | PASS | No public API/signature change; `ChunkStore::get_fragment` already existed. `traits/src/lib.rs` doc updated to reflect scrub driving off the reference set (`patch.diff` +265–284). Consistent shape. |
| T3 Runtime | PASS | By inspection: `HashMap` grouping and by-dserver lookup are total, no unwrap/panic on the new path, `Ok(None)` handled explicitly. Rests on C4 execution (above) for empirical confirmation — not run here. |
| T4 Contribution | PASS | Two tests add real coverage: the missing-detection leg plus a false-positive guardrail (`does_not_flag_an_in_flight_pending_writes_fragment...`, `patch.diff` +211–260). Not tautological — coupled to the changed arm. |
| T5 Judgment | NEEDS-HUMAN | The fix treats **any** committed-referenced `Ok(None)` as durable loss. Brief requires guardrails for in-flight writes, **pending/expired-lease GC**, and orphan fragments; only the in-flight case has a test, and both the in-flight and pending-GC guarantees rest on `gc::referenced_fragments` filtering committed-only + `write.rs:220` commit-after-ack — neither visible in the patch nor verifiable here (target unreadable). **Decision owed:** confirm the reference set truly excludes pending-GC/expired-lease and orphan fragments so those windows can't yield false-positive repair obligations, and decide whether the untested pending-GC window warrants added coverage before ship. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed:** confirm this scrub-path detector actually restores the durability invariant in production (not merely the `MemStore`/`MemMeta` harness), that shipping with the killed/partitioned D-server case deliberately out of scope is acceptable with a follow-up tracked, and that leaving the #250/#196 `enqueue_repair` test stand-ins in place is the intended disposition. Fitness-to-purpose is human-owned at sign-off. |

### Advisory — codex

# Advisory review — codex — NOT COMPLETED

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-codex.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'codex' did not produce findings (leaf failed: [Errno 2] No such file or directory: 'codex'); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Gate `C4-ci` and `C4-verify` both FAIL on `cargo: not found` (check-gates.json:37, :46) — toolchain-absent, not a patch defect; I confirmed no cargo/rustc here so I could not re-run. **Decision owed:** run `run-verify.sh` / `cargo xtask ci` on a machine with the Rust toolchain and confirm the two new tests are red pre-fix and green post-fix; the reported "RED with fix" is a build-can't-run artifact of the missing toolchain, not observed test failure.
- [ ] T5 Judgment — The fix treats **any** committed-referenced `Ok(None)` as durable loss. Brief requires guardrails for in-flight writes, **pending/expired-lease GC**, and orphan fragments; only the in-flight case has a test, and both the in-flight and pending-GC guarantees rest on `gc::referenced_fragments` filtering committed-only + `write.rs:220` commit-after-ack — neither visible in the patch nor verifiable here (target unreadable). **Decision owed:** confirm the reference set truly excludes pending-GC/expired-lease and orphan fragments so those windows can't yield false-positive repair obligations, and decide whether the untested pending-GC window warrants added coverage before ship.
- [ ] Validation — fitness-to-purpose — **Decision owed:** confirm this scrub-path detector actually restores the durability invariant in production (not merely the `MemStore`/`MemMeta` harness), that shipping with the killed/partitioned D-server case deliberately out of scope is acceptable with a follow-up tracked, and that leaving the #250/#196 `enqueue_repair` test stand-ins in place is the intended disposition. Fitness-to-purpose is human-owned at sign-off.
- [ ] advisory leaf 'codex' did not produce findings (leaf failed: [Errno 2] No such file or directory: 'codex'); re-run it or adjudicate by hand.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — ./engine/xtask.sh: line 30: exec: cargo: not found

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
- Iteration delta (if iterating): issue_330: Patch content accepted as-is — no change of approach needed. The only blocker is that the C4 gates never actually ran: cargo/rustc were absent in the Do sandbox, so C4-ci ("cargo: not found") and C4-verify ("test RED with fix") are toolchain-absence artifacts, not observed defects. Re-run Do in an environment with the Rust toolchain so `cargo xtask ci` and `run-verify.sh` execute and confirm both new scrub tests are red pre-fix / green post-fix. Also make the `codex` advisory leaf available (or accept its absence) so the second reviewer produces findings.
- By / date: Eduard Ralph / 2026-07-01

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
