# Result — issue 455 / e2e-closed-write-path

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The write→durability loop's halves are each proven in isolation but never
- Success criterion: An in-process test drives a **gateway S3 PUT** that writes object
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4 integration
- Scope (one logical fix) / out of scope: Join the gateway **write path** and the custodian **repair path** over one shared

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

Review of issue 455 / e2e-closed-write-path: add an in-process closed-loop proof that a gateway-written object in a shared metadata store becomes a custodian-visible repair obligation and round-trips after D-server loss.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: shared-store gateway PUT, real loopback D-servers, custodian non-zero obligation, gauge rise-to-zero, and byte-identical GET are the acceptance context (`brief.md:24`). |
| C2 Reproduction (red pre-fix) | FAIL | The human must decide whether to accept absence-only red: the brief requires a demonstrated load-bearing red (`brief.md:37`), but the patch states red as no prior joined test (`crates/server/tests/closed_write_path.rs:38`). |
| C3 Change | PASS | The changed surface is the requested new integration test file and it drives gateway write, custodian reconstruction, gauge assertions, and GET in one scenario (`crates/server/tests/closed_write_path.rs:161`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The verification owed cannot be independently reproduced here: `./engine/xtask.sh ci` and `engine/scripts/run-verify.sh` are absent, and direct `cargo test -p wyrd-server --test closed_write_path` fails at loopback bind with `PermissionDenied` (`crates/server/tests/closed_write_path.rs:75`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether manually enqueueing the repair obligation still proves the root contract: the gateway writes placement, but the test injects the health repair queue entry before custodian observation (`crates/server/tests/closed_write_path.rs:290`). |
| T1 Structure | PASS | The implementation stays scoped to the requested server integration test instead of broadening production wiring (`crates/server/tests/closed_write_path.rs:1`). |
| T2 Shape | NEEDS-HUMAN | Decide whether a directly-held `Gateway::put_object` satisfies the required gateway S3 PUT surface, since the test bypasses the HTTP/AWS SDK wire path while using the same gateway core (`crates/server/tests/closed_write_path.rs:202`). |
| T3 Runtime | NEEDS-HUMAN | Runtime behavior remains unobserved in this host: the test needs loopback gRPC servers, and this sandbox rejects the bind before the scenario can run (`crates/server/tests/closed_write_path.rs:75`). |
| T4 Contribution | PASS | If runnable, the test contributes the missing cross-crate regression by composing gateway write, redb reopen, custodian sweep, repair persistence, and final readback (`crates/server/tests/closed_write_path.rs:306`). |
| T5 Judgment | NEEDS-HUMAN | Prior-art by affected path is mechanically empty in local git history, but closed/rejected PR history is not available here, so the duplicate-work decision is not fully discharged (`crates/server/tests/closed_write_path.rs:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether this in-process redb/loopback proof is fit for the production-risk claim while live TiKV/Docker/Prometheus demonstration remains explicitly off-Check (`brief.md:93`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — The verification owed cannot be independently reproduced here: `./engine/xtask.sh ci` and `engine/scripts/run-verify.sh` are absent, and direct `cargo test -p wyrd-server --test closed_write_path` fails at loopback bind with `PermissionDenied` (`crates/server/tests/closed_write_path.rs:75`).
- [ ] C5 Causal adequacy — Decide whether manually enqueueing the repair obligation still proves the root contract: the gateway writes placement, but the test injects the health repair queue entry before custodian observation (`crates/server/tests/closed_write_path.rs:290`).
- [x] T2 Shape — Decide whether a directly-held `Gateway::put_object` satisfies the required gateway S3 PUT surface, since the test bypasses the HTTP/AWS SDK wire path while using the same gateway core (`crates/server/tests/closed_write_path.rs:202`).
- [x] T3 Runtime — Runtime behavior remains unobserved in this host: the test needs loopback gRPC servers, and this sandbox rejects the bind before the scenario can run (`crates/server/tests/closed_write_path.rs:75`).
- [x] T5 Judgment — Prior-art by affected path is mechanically empty in local git history, but closed/rejected PR history is not available here, so the duplicate-work decision is not fully discharged (`crates/server/tests/closed_write_path.rs:1`).
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether this in-process redb/loopback proof is fit for the production-risk claim while live TiKV/Docker/Prometheus demonstration remains explicitly off-Check (`brief.md:93`).

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
- Iteration delta (if iterating): Why rejected (issue_455): the closed-loop join is not actually proven to close on its own. The test hand-feeds the repair obligation via `repair::enqueue_repair(&meta, chunk_id, "health")` (patch line 296) before the custodian observes it. The custodian must *derive* the obligation from the gateway-written placement, not be handed a pre-enqueued queue entry — otherwise the load-bearing assertion (`under_replicated == 1.0`) can pass even if the gateway→custodian derivation is broken. This is the exact "empty store" failure the issue exists to catch, and the manual enqueue papers over it. What to change next: - Remove the manual `enqueue_repair`. Have the custodian compute the under-replicated obligation from the placement the gateway PUT actually recorded + the observed D-server loss (drive the shipping obligation-discovery path, not an injected queue entry). - Capture a DEMONSTRATED red (brief.md:37-41, 90-92): a temporary negation — e.g. drop the D-server kill, or write a placement the custodian cannot read — must flip the load-bearing assertion to red, proving the obligation count derives from the gateway write. Absence-only red (test did not exist before) is not sufficient here; the C4-verify gate FAILED (test passes without a fix) and the reviewer flagged C2. Cleared at this sign-off (still hold on the rebuild): T2 Shape (directly-held `Gateway::put_object` is brief-permitted), T3 Runtime (xtask ci green in the gate env; only the reviewer sandbox could not bind loopback), T5 Judgment (no duplicate prior art). Deferred: §6 Validation — fitness-to-purpose to be re-reviewed after iterate-do, once the loop is shown to close without the manual enqueue.
- By / date: Eduard Ralph / 2026-07-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
