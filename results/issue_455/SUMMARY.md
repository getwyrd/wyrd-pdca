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

Review task: close issue #455 by proving a gateway-written object over a shared metadata store becomes a custodian-derived repair obligation and survives D-server loss.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed decision is whether the slice proves the full in-process closed loop: gateway PUT over one shared store, custodian-visible non-zero obligation after a killed D-server, gauge return to zero, and byte-identical GET (`brief.md:24`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | I could not independently rerun the red proof because this sandbox denies loopback bind; the gate claims red-without-fix/green-with-fix, but that evidence remains provisional here (`check-gates.json:42`). |
| C3 Change | FAIL | The human must decide whether reachable fragment deletion is an acceptable substitute for the required killed-D-server failure; the test deletes one fragment while the D-server remains reachable and only aborts servers at teardown, so the killed-peer criterion is not implemented (`crates/server/tests/closed_write_path.rs:271`, `crates/server/tests/closed_write_path.rs:283`, `crates/server/tests/closed_write_path.rs:383`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Compile-only passed, but runtime verification could not be reproduced here: the focused test panicked at loopback bind with `Operation not permitted`, so the reported CI/run-verify pass needs an environment that permits local sockets (`crates/server/tests/closed_write_path.rs:83`). |
| C5 Causal adequacy | FAIL | The owed root-cause decision is whether #455 is about killed/unreachable D-server obligations or only scrub-detectable fragment loss; the implementation scrubs the live fleet and documents wholly-unreachable peers as read-around/out-of-scope, so it does not causally cover the brief's killed-server symptom (`crates/server/src/custodian.rs:332`, `crates/server/src/custodian.rs:344`, `crates/server/tests/closed_write_path.rs:31`). |
| T1 Structure | PASS | The change stays on the requested server/custodian integration surface: one production wiring edit and the new closed-write-path integration test, with no unrelated surfaces touched (`crates/server/src/custodian.rs:316`, `crates/server/tests/closed_write_path.rs:197`). |
| T2 Shape | PASS | The direct `Gateway` path is acceptable for the brief's in-process shared-store shape, and the test drives real gRPC D-server services rather than an in-memory chunk-store double (`crates/server/tests/closed_write_path.rs:78`, `crates/server/tests/closed_write_path.rs:230`). |
| T3 Runtime | NEEDS-HUMAN | Runtime behavior is unverified in this reviewer sandbox because local socket binding is blocked; an operator should rerun `cargo test -p wyrd-server --test closed_write_path -- --nocapture` where loopback binds are allowed (`crates/server/tests/closed_write_path.rs:83`). |
| T4 Contribution | PASS | The patch adds meaningful regression coverage for gateway PUT to shared redb, derived repair queueing, gauge rise/drain, and GET round-trip, even though it covers reachable data loss rather than the killed-peer criterion (`crates/server/tests/closed_write_path.rs:223`, `crates/server/tests/closed_write_path.rs:331`, `crates/server/tests/closed_write_path.rs:342`, `crates/server/tests/closed_write_path.rs:363`). |
| T5 Judgment | PASS | Affected-file history still shows prior custodian work but no existing `closed_write_path` test, so I found no duplicate prior art by affected path (`crates/server/tests/closed_write_path.rs:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether proving reachable fragment loss satisfies the product intent when the brief explicitly asks for killed-D-server backlog behavior; that decision affects whether this can close #455 or needs another iteration (`brief.md:27`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — I could not independently rerun the red proof because this sandbox denies loopback bind; the gate claims red-without-fix/green-with-fix, but that evidence remains provisional here (`check-gates.json:42`).
- [x] C4 Verification (red→green) — Compile-only passed, but runtime verification could not be reproduced here: the focused test panicked at loopback bind with `Operation not permitted`, so the reported CI/run-verify pass needs an environment that permits local sockets (`crates/server/tests/closed_write_path.rs:83`).
- [x] T3 Runtime — Runtime behavior is unverified in this reviewer sandbox because local socket binding is blocked; an operator should rerun `cargo test -p wyrd-server --test closed_write_path -- --nocapture` where loopback binds are allowed (`crates/server/tests/closed_write_path.rs:83`).
- [x] Validation — fitness-to-purpose — Human sign-off must decide whether proving reachable fragment loss satisfies the product intent when the brief explicitly asks for killed-D-server backlog behavior; that decision affects whether this can close #455 or needs another iteration (`brief.md:27`).

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
- By / date: Eduard Ralph / 2026-07-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_455: investigate why the reviewer/CI sandbox cannot bind loopback sockets (`Operation not permitted`), which blocks independent runtime verification of loopback-gRPC tests (recurs in C2/C4/T3 NEEDS-HUMAN).
- issue_455 follow-up: add a killed/unreachable-D-server fault scenario to closed_write_path — reachable-fragment-loss only for now; broader fault coverage is later hardening.
