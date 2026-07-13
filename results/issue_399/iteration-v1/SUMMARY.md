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

Task under review: add a Tier-1 Jepsen live network-partition nemesis distinct from the existing process-freeze leg, with check-time decision/oracle coverage and deferred privileged live execution.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief scopes this as additive test/xtask infrastructure for a live `:partition` nemesis while keeping the `:pause` leg, with the live end-to-end job explicitly deferred off Check (`brief.md:19`, `brief.md:87`, `brief.md:131`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The decision owed is whether the builder's temporary-negation red is accepted, because I could run the planted `"paused"` negative-control tests but could not independently stash/negate the fix without the withheld red artifact (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:604`, `xtask/src/faults.rs:749`). |
| C3 Change | PASS | The target now has a distinct pure `IsolationNemesis::NetworkPartition` value, exports the compose-network input, adds a live-partition scenario, and keeps the workflow wired to the privileged Tier-1 job (`xtask/src/faults.rs:200`, `xtask/src/faults.rs:236`, `xtask/src/faults.rs:399`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1203`, `.github/workflows/tier1-jepsen.yml:32`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The decision owed is whether to accept provisional green evidence: targeted tests passed locally, but the exact `./engine/xtask.sh ci` and `./engine/scripts/run-verify.sh` harnesses are absent here and `cargo xtask ci` hits sandbox loopback `PermissionDenied` in unrelated `list_delete_over_grpc`, so I cannot independently affirm full red-to-green. |
| C5 Causal adequacy | PASS | The fix addresses the named cause by adding a real network disconnect/connect path plus a liveness oracle that rejects `paused`, rather than guarding over the old pause behavior (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:388`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1447`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1523`). |
| T1 Structure | PASS | The change stays on the cited surfaces only: Tier-1 workflow, `xtask` fault orchestration, and the Tier-1 scenario/oracle test (`.github/workflows/tier1-jepsen.yml:32`, `xtask/src/faults.rs:191`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:377`). |
| T2 Shape | PASS | The implementation exposes the born-at-tier decision as a test-observable value with both alternatives and distinct scenario functions, matching the brief's shape requirement (`xtask/src/faults.rs:218`, `xtask/src/faults.rs:236`, `xtask/src/faults.rs:766`). |
| T3 Runtime | NEEDS-HUMAN | The decision owed is whether the privileged runner actually exercises `docker network disconnect/connect` against the 10-container cluster, because Check can compile and unit-test the hooks but cannot prove the live Docker topology here (`brief.md:115`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1447`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1466`). |
| T4 Contribution | PASS | Affected-file history shows the prior pause harness but no merged prior #399 live-partition implementation on these paths, so this reads as the additive contribution rather than duplicate work (`brief.md:168`, `xtask/src/faults.rs:200`). |
| T5 Judgment | NEEDS-HUMAN | The decision owed is maintainer acceptance of the declared split posture: the code-read/unit-test evidence covers the Check slice, while the real live-partition-and-heal semantics must be confirmed by the privileged `WYRD_TIER1=1` job (`brief.md:138`, `.github/workflows/tier1-jepsen.yml:62`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether node-liveness during isolation is sufficient fitness for this slice given the brief's explicit admission that today's in-process coordination cannot exhibit the stronger stale-action-on-heal behavior (`brief.md:52`, `brief.md:144`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — The decision owed is whether the builder's temporary-negation red is accepted, because I could run the planted `"paused"` negative-control tests but could not independently stash/negate the fix without the withheld red artifact (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:604`, `xtask/src/faults.rs:749`).
- [ ] C4 Verification (red→green) — The decision owed is whether to accept provisional green evidence: targeted tests passed locally, but the exact `./engine/xtask.sh ci` and `./engine/scripts/run-verify.sh` harnesses are absent here and `cargo xtask ci` hits sandbox loopback `PermissionDenied` in unrelated `list_delete_over_grpc`, so I cannot independently affirm full red-to-green.
- [ ] T3 Runtime — The decision owed is whether the privileged runner actually exercises `docker network disconnect/connect` against the 10-container cluster, because Check can compile and unit-test the hooks but cannot prove the live Docker topology here (`brief.md:115`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1447`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1466`).
- [ ] T5 Judgment — The decision owed is maintainer acceptance of the declared split posture: the code-read/unit-test evidence covers the Check slice, while the real live-partition-and-heal semantics must be confirmed by the privileged `WYRD_TIER1=1` job (`brief.md:138`, `.github/workflows/tier1-jepsen.yml:62`).
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether node-liveness during isolation is sufficient fitness for this slice given the brief's explicit admission that today's in-process coordination cannot exhibit the stronger stale-action-on-heal behavior (`brief.md:52`, `brief.md:144`).

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
- Iteration delta (if iterating): Live `WYRD_TIER1=1 cargo xtask jepsen` run (human, at sign-off): ProcessFreeze leg passes end-to-end; NetworkPartition leg FAILS at Phase 3 heal (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:1547`) — `Store(Unavailable("tcp connect error" … 127.0.0.1:<port> … Connection refused))` dialing the reconnected isolated node's host-published port. Root cause is the chosen partition mechanism, not a Wyrd consistency violation: `docker network disconnect` tears down the container's published-port proxy and `docker network connect` does not restore it on heal (reconnect commonly reassigns the IP and does not re-establish the original host-port forwarding), so the isolated node is unreachable at the endpoint the test holds after "heal." The leg breaks its own reachability before the ADR-0015-across-heal assertions can run — i.e. the live partition-and-heal deliverable the issue exists to add is not actually demonstrated. What to change next (keep scope as briefed): - Replace `docker network disconnect`/`connect` with a partition mechanism that keeps the container's network identity and host-published port mapping intact across the fault window — e.g. an in-container `iptables`/`tc` packet drop (a brief-named alternative), OR re-resolve + re-dial the endpoint on heal so Phase 3 uses the restored route rather than the torn-down mapping. - Verify the fix on the privileged live runner: the NetworkPartition leg must reach Phase 3+ green (node stays `running` during isolation AND every ADR-0015 property holds across the heal), captured as bundle evidence so Check T3/T5/Validation can be cleared next pass. - Preserve what is sound and green: the pure `IsolationNemesis` decision + its unit test, the `assert_node_live_during_isolation` oracle + negative controls, and the ProcessFreeze leg (unaffected — do not disturb).
- By / date: Eduard Ralph / 2026-07-08

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
