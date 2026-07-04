# Result — issue 253 / atomic-conditional-commit-conflict-semantics

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a forced write-write race over a real TiKV surfaces as **exactly one** `Ok(Committed)` and the rest `Ok(Conflict)` — **zero `Err`** — with the final stored value equal to the winner's; a genuine fault still surfaces as `Err`. The shared conformance suite (#252) still passes. `cargo xtask ci` stays green on a machine with **no** TiKV (the tikv module + the new contention test skip cleanly). The behavioral proof runs under `cargo xtask tikv-conformance` against the throwaway `deploy/` single-node TiKV.
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: the one logical change is proposal 0007 §"Suggested PR sequence" item 2 — harden `commit()`'s conflict semantics and prove them under contention:

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan feature slice behind Accepted ADR-0008; no new ADR is minted).
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

# Check review — issue 253 / atomic-conditional-commit-conflict-semantics

**Task under review:** Harden `TikvMetadataStore::commit()` so a *real* TiKV write-write race surfaces as `Ok(CommitOutcome::Conflict)` (a losing writer), not `Err` — while genuine faults stay `Err` — and prove it under contention with a new endpoint-gated property test, wiring `xtask tikv-conformance` to run it.

Reviewed against target source at `/home/eddie/wyrd/wyrd.pdca-wt` (patch applied there; tikv-client pinned 0.4.0, `Cargo.lock:2830`). `build-notes.md` withheld by design.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief gives a precise, self-consistent spec: the `Ok(Conflict)`-vs-`Err` partition (`crates/traits/src/lib.rs:346-361` `CommitOutcome`), the exact classification rule §(a), the two property tests §(b), and the xtask wiring §(c). Success criterion is measurable (one `Committed`, rest `Conflict`, zero `Err`, final=winner). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The red state is **endpoint-gated**: without `WYRD_TIKV_PD_ENDPOINTS` the test returns early and passes on both old and new `commit()`, so no red is observable in-sandbox (this is exactly what non-gating `C4-verify` reported — a designed artifact, not a defect). Decision owed: confirm that on the pre-#253 `commit()` the losing writers returned `Err` (test red) under `cargo xtask tikv-conformance` against `deploy/` TiKV — I have no TiKV/docker access to drive it. |
| C3 Change | PASS | `is_write_conflict` (lib.rs:149-159) folds only `KeyError.conflict.is_some()`, recursing into `MultipleKeyErrors`/`ExtractedErrors` (any) and `PessimisticLockError{inner,..}`; `conflict_or_err` (lib.rs:173-183) best-effort rolls back then maps. Routed through the `get_for_update` arm (lib.rs:277) and final `commit()` arm (lib.rs:306); precondition-miss cleanup made best-effort (lib.rs:286); `put`/`delete` keep `rollback_then`→`Err` (lib.rs:292,297). Byte-for-byte the brief §(a) contract. |
| C4 Verification (red→green) | NEEDS-HUMAN | Gating `C4-ci` (`cargo xtask ci`) PASSED per check-gates.json — but it builds **default features only** (`xtask/src/main.rs:689-696`), so it never compiles `--features tikv` and does not exercise the race; the non-gating `C4-verify` "fail" is the endpoint-gated skip, **not** a blocking defect. Decision owed: run `cargo xtask tikv-conformance` and confirm the tally (1 `Committed`, 7 `Conflict`, 0 `Err`, final=winner) — I cannot (no docker/TiKV, `cargo --features tikv` network-blocked in sandbox). |
| C5 Causal adequacy | PASS | Genuine root-cause fix, not a symptom guard: the cause is `rollback_then` mapping *every* backend error to `Err` (lib.rs:128-131), mislabeling a legitimate write-conflict as a fault; the patch *transforms* the classification rather than probing/guarding a capability, so the C5 symptom-guard smell-test does not fire. Completeness of the conflict-error taxonomy (are `ExtractedErrors`/`PessimisticLockError` the only shapes a prewrite/lock race takes?) is a distributed-semantics judgment — carried to T5. |
| T1 Structure | PASS | New `crates/metadata-tikv/tests/contention.rs` mirrors the existing `tests/conformance.rs` harness (same `pd_endpoints()` gate, same `#[cfg(feature="tikv")]` / `#[cfg(not)]` split, fresh-namespace isolation); helpers are cohesive and single-purpose. Prior-art by path: file is new, no colliding precedent (brief notes no in-repo distributed-txn test precedent). |
| T2 Shape | PASS | Exactly the two tests the brief §(b) names — `write_write_race_exactly_one_winner` (seeds `v0`, then `require(k,v0)+put`) and `require_absent_race` — each fanning `WRITERS=8` independent connections over one namespace via `join_all`, asserting `committed==1`, `conflicts==WRITERS-1`, panic-on-`Err`, and final value = winner (contention.rs:278-306,312-325). Matches the success criterion shape. |
| T3 Runtime | PASS | Skip-path runtime is proven: default-feature `cargo test` in the passing `C4-ci` runs both `#[test]` fns, which early-return cleanly with no endpoint (contention.rs:169-176,183-191). The **contended** runtime (the substantive assertion) is un-driveable here and folds into the C4/Validation live-TiKV NEEDS-HUMAN. |
| T4 Contribution | PASS | All three parts land coherently: classification (a), the two property tests (b), and `run_tikv_conformance_test` looping `["conformance","contention"]` through `run_tikv_test` (`xtask/src/main.rs:400-405`). The `MetadataStore` trait is byte-for-byte untouched — the diff never enters `crates/traits` — honoring M4's premise. |
| T5 Judgment | NEEDS-HUMAN | The load-bearing call is which tikv-client 0.4.0 error shapes a write-conflict actually takes and whether high pessimistic-lock contention can instead yield a lock-timeout/deadlock (→ `Err`), which would fail the "zero Err" assertion for reasons unrelated to the fix. Decision owed: a maintainer must judge (via the real run's output) that the `is_write_conflict` taxonomy is exhaustive and the test is not flaky against `deploy/` TiKV — a pre-1.0-client distributed-semantics call I cannot settle in-sandbox. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Owed at sign-off: run `cargo xtask tikv-conformance` (brings up `deploy/tikv-single-node`, rebuilds `--features tikv`, runs `--test conformance` and `--test contention`) and confirm both tests pass with the M4.2 tally, that `cargo xtask ci` stays green on a no-TiKV machine (it does — gate PASS), and that the ADR-0003 tikv-client audit remains deferred (unresolved by this slice, per brief). I could not exercise this: sandbox has no docker/TiKV and blocks `cargo --features tikv`. |

### Advisory — codex

- NEEDS-HUMAN — `crates/metadata-tikv/tests/contention.rs:143` starts the racers by concurrently calling the public `store.commit(batch)`, but each commit first goes through the normal `get_for_update` precondition path and can serialize there before returning the already-existing precondition-miss `Conflict` path in `crates/metadata-tikv/src/lib.rs:283`. That means the test can pass without proving the new `txn.commit()`/write-conflict classification, which is consistent with the non-gating red→green check reporting no red; sign-off should decide whether the withheld TiKV run actually demonstrated the intended write-conflict error path or whether the test needs a stronger synchronization/transaction-shape proof.
- `xtask/src/main.rs:183` still reports only “passed the shared MetadataStore conformance suite” after `run_tikv_conformance_test` now runs both `conformance` and `contention`. This is a low-risk cleanup, but it can mislead release/sign-off logs about whether the new contention binary was part of the successful TiKV job.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — The red state is **endpoint-gated**: without `WYRD_TIKV_PD_ENDPOINTS` the test returns early and passes on both old and new `commit()`, so no red is observable in-sandbox (this is exactly what non-gating `C4-verify` reported — a designed artifact, not a defect). Decision owed: confirm that on the pre-#253 `commit()` the losing writers returned `Err` (test red) under `cargo xtask tikv-conformance` against `deploy/` TiKV — I have no TiKV/docker access to drive it.
- [x] C4 Verification (red→green) — Gating `C4-ci` (`cargo xtask ci`) PASSED per check-gates.json — but it builds **default features only** (`xtask/src/main.rs:689-696`), so it never compiles `--features tikv` and does not exercise the race; the non-gating `C4-verify` "fail" is the endpoint-gated skip, **not** a blocking defect. Decision owed: run `cargo xtask tikv-conformance` and confirm the tally (1 `Committed`, 7 `Conflict`, 0 `Err`, final=winner) — I cannot (no docker/TiKV, `cargo --features tikv` network-blocked in sandbox).
- [x] T3 Runtime
- [x] T5 Judgment — The load-bearing call is which tikv-client 0.4.0 error shapes a write-conflict actually takes and whether high pessimistic-lock contention can instead yield a lock-timeout/deadlock (→ `Err`), which would fail the "zero Err" assertion for reasons unrelated to the fix. Decision owed: a maintainer must judge (via the real run's output) that the `is_write_conflict` taxonomy is exhaustive and the test is not flaky against `deploy/` TiKV — a pre-1.0-client distributed-semantics call I cannot settle in-sandbox.
- [x] Validation — fitness-to-purpose — Owed at sign-off: run `cargo xtask tikv-conformance` (brings up `deploy/tikv-single-node`, rebuilds `--features tikv`, runs `--test conformance` and `--test contention`) and confirm both tests pass with the M4.2 tally, that `cargo xtask ci` stays green on a no-TiKV machine (it does — gate PASS), and that the ADR-0003 tikv-client audit remains deferred (unresolved by this slice, per brief). I could not exercise this: sandbox has no docker/TiKV and blocks `cargo --features tikv`.
- [x] `crates/metadata-tikv/tests/contention.rs:143` starts the racers by concurrently calling the public `store.commit(batch)`, but each commit first goes through the normal `get_for_update` precondition path and can serialize there before returning the already-existing precondition-miss `Conflict` path in `crates/metadata-tikv/src/lib.rs:283`. That means the test can pass without proving the new `txn.commit()`/write-conflict classification, which is consistent with the non-gating red→green check reporting no red; sign-off should decide whether the withheld TiKV run actually demonstrated the intended write-conflict error path or whether the test needs a stronger synchronization/transaction-shape proof.

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
- By / date: Eduard Ralph / 2026-07-03

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_253: stale log at `xtask/src/main.rs:183` says "passed the shared MetadataStore conformance suite" though `run_tikv_conformance_test` now also runs the `contention` binary — cleanup so sign-off/release logs aren't misleading.
