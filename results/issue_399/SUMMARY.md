# Result — issue 399 / tier1-jepsen-live-network-partition

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 Jepsen consistency leg (ADR-0039;
- Success criterion: Demonstrable at C4-verify (Check, container-free): the leg gains a
- Repo + branch target: getwyrd/wyrd @ main   (single-slice test-infra enhancement; milestone-9 "Foundations", independent of the M4 metadata backend — INTEGRATION §2)
- Scope (one logical fix) / out of scope: Add a **network-level partition nemesis** to the Tier-1 Jepsen leg that keeps

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
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

Review of issue 399: add a Tier-1 Jepsen live network-partition nemesis distinct from the existing process-freeze leg, with Check-time routing/oracle coverage and deferred privileged live execution.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: keep the freeze leg while adding a live `:partition` leg with Check-time pure routing/oracle coverage and deferred privileged live run (`brief.md:19`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The temporary-negation red was not independently reproducible here because `./engine/scripts/run-verify.sh` was not present and the reviewer role did not patch/stash target source; human must confirm the claimed red evidence rather than infer it from green tests. |
| C3 Change | PASS | The patch changes the relevant surfaces only: `xtask` selects both nemesis legs, the scenario adds a live partition body, and the D-server image supplies the sidecar `iptables` binary (`xtask/src/faults.rs:245`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1277`, `crates/chunkstore-grpc/tests/dserver/Dockerfile:30`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Narrow green was reproduced (`cargo test -p xtask tier1_jepsen_isolation_legs`, `cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency node_liveness_during_isolation`, and `--no-run` compile), but full `cargo xtask ci` failed on sandbox loopback bind permission and the configured engine wrapper was missing, so the full red→green gate remains provisional. |
| C5 Causal adequacy | PASS | The cause addressed is the nemesis mechanism: the new leg uses an in-netns gRPC-port DROP and asserts Docker state `running`, so the decision owed is whether this mechanism models the live isolation the brief requires rather than the old freeze (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1502`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1513`). |
| T1 Structure | PASS | The change is localized to Tier-1 test orchestration, the test scenario, the test container image, and the Tier-1 workflow; no production repair-path code is touched (`xtask/src/faults.rs:289`, `.github/workflows/tier1-jepsen.yml:58`). |
| T2 Shape | PASS | The routing is a pure value with both alternatives representable and tested, matching the born-at-tier pattern rather than hiding the new leg in argv plumbing (`xtask/src/faults.rs:207`, `xtask/src/faults.rs:755`). |
| T3 Runtime | NEEDS-HUMAN | The live runtime dependency was not exercised here: Docker with `NET_ADMIN` sidecar capability must run `WYRD_TIER1=1 cargo xtask jepsen` and show the NetworkPartition leg reaches Phase 3+ green after the `iptables -D` heal (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1572`). |
| T4 Contribution | PASS | The contribution preserves the cheaper process-freeze leg and wires the new live-partition leg into the scheduled Tier-1 job, so the added cost and coverage are visible to maintainers (`xtask/src/faults.rs:245`, `.github/workflows/tier1-jepsen.yml:60`). |
| T5 Judgment | NEEDS-HUMAN | Prior-art by affected path shows the existing pause leg on merged history and an M4 metadata partition pattern on another branch, but this review could not mechanically settle closed/rejected issue history; human must confirm no superseding #399 attempt or ADR constraint is being bypassed. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human: decide whether the delivered evidence is fit for sign-off, especially because the load-bearing live Docker partition remains deferred to the privileged runner rather than reproduced in this sandbox. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — The temporary-negation red was not independently reproducible here because `./engine/scripts/run-verify.sh` was not present and the reviewer role did not patch/stash target source; human must confirm the claimed red evidence rather than infer it from green tests.
- [x] C4 Verification (red→green) — Narrow green was reproduced (`cargo test -p xtask tier1_jepsen_isolation_legs`, `cargo test -p wyrd-chunkstore-grpc --test tier1_jepsen_consistency node_liveness_during_isolation`, and `--no-run` compile), but full `cargo xtask ci` failed on sandbox loopback bind permission and the configured engine wrapper was missing, so the full red→green gate remains provisional.
- [x] T3 Runtime — The live runtime dependency was not exercised here: Docker with `NET_ADMIN` sidecar capability must run `WYRD_TIER1=1 cargo xtask jepsen` and show the NetworkPartition leg reaches Phase 3+ green after the `iptables -D` heal (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1572`).
- [x] T5 Judgment — Prior-art by affected path shows the existing pause leg on merged history and an M4 metadata partition pattern on another branch, but this review could not mechanically settle closed/rejected issue history; human must confirm no superseding #399 attempt or ADR constraint is being bypassed.
- [x] Validation — fitness-to-purpose — Always-human: decide whether the delivered evidence is fit for sign-off, especially because the load-bearing live Docker partition remains deferred to the privileged runner rather than reproduced in this sandbox.
- [x] external dependency: Docker + privileged tier1-jepsen runner (WYRD_TIER1=1) — cannot run the live network-partition-and-heal E2E at Check; the iteration-1 heal failure and this mechanism fix are only observable on that runner. Confirm the NetworkPartition leg reaches Phase 3+ green.

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
- By / date: Eduard Ralph / 2026-07-08

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
