# Result — issue 407 / m4-metadata-nemesis-partition-skew-pause

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the metadata Tier-1 scenario can be driven under a composable **nemesis** with
- Success criterion: the nemesis exposes three leg kinds (partition / clock-skew /
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the three-leg nemesis seam + its materialization oracles + the pure

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
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

Review of issue #407: add an importable three-leg metadata nemesis (partition, clock skew, and process pause), materialization/heal oracles, and an opt-in live FDB runner.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-ready: default-compiled orchestration/oracle behavior is separated from the explicitly opt-in live topology, and the runnable command is exposed at `xtask/src/main.rs:91`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the claimed pre-fix compile failure is acceptable without an independent rerun — the asserted `engine/scripts/run-verify.sh` is absent and the read-only target cannot be stashed manually, so only the green suites were reproduced (`xtask/tests/nemesis_orchestration.rs:23`, `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:298`). |
| C3 Change | PASS | The patch matches the target byte-for-byte and confines the behavior to the declared conformance seam, FDB Tier-1 scenario, deploy override, and xtask runner; the public lifecycle entry point is grounded at `crates/metadata-fault-conformance/src/nemesis.rs:295`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept provisional verification — both named suites pass (4/4 and 11/11), but red could not be rerun, and `cargo xtask ci` reached workspace tests then hit the sandbox's loopback-bind denial rather than a patch failure (`crates/chunkstore-grpc/tests/list_delete.rs:55`). |
| C5 Causal adequacy | PASS | The change supplies the missing reusable lifecycle rather than guarding an eager/load-time side effect: workload execution is enclosed by confirmed materialization and unconditional healing at `crates/metadata-fault-conformance/src/nemesis.rs:312` and `crates/metadata-fault-conformance/src/nemesis.rs:336`. |
| T1 Structure | PASS | Dependency direction remains coherent: reusable lifecycle code lives in the conformance crate while xtask owns only enumeration/argv routing at `xtask/src/nemesis.rs:80` and `xtask/src/nemesis.rs:102`. |
| T2 Shape | PASS | The public seam exposes typed evidence and all three stable leg kinds, while the runner guards exact test dispatch against silent zero-test success at `xtask/src/nemesis.rs:128` and `xtask/src/nemesis.rs:149`. |
| T3 Runtime | NEEDS-HUMAN | Witness `WYRD_TIER1=1 cargo xtask metadata-nemesis` on the privileged three-process FDB deployment and confirm all legs materialize, run the commit probe, and heal — Docker exists here but libfaketime is absent, so the external topology was not exercised (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:42`, `deploy/fdb-multi-replica/docker-compose.faketime.yml:35`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether prior art is fully cleared — affected-path merged history was inspected locally, but closed/rejected remote work could not be mechanically queried in this sandbox, so uniqueness beyond the brief's claim remains unconfirmed. |
| T5 Judgment | NEEDS-HUMAN | Decide whether pure oracle/lifecycle coverage is sufficient for this high-reach slice before a witnessed live campaign — the default tests validate decisions, but the production-path workload is only exercised by ignored FDB-feature tests at `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:184`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether one commit round-trip under each fault is adequate evidence for the intended reusable nemesis before #408 adds the checked Elle workload — this determines whether the seam is credible for its downstream purpose (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:188`). |

### Advisory — adversary

# check-advisory-adversary.md — issue #407, iteration 3 (adversarial pass)

Evidence re-run (toolchain available; all paths on `$PDCA_TARGET`):

- **Red→green independently reproduced, and the assertions bite.** Green: both named tests
  pass at the target (`cargo test -p xtask --test nemesis_orchestration`: 4/4;
  `-p wyrd-metadata-fault-conformance --test nemesis_oracles`: 11/11). Red: in a scratch copy,
  reverting only the production wiring (`pub mod nemesis;`,
  `crates/metadata-fault-conformance/src/lib.rs:65` / `xtask/src/lib.rs:20`) fails BOTH test
  binaries to compile (and the `xtask` bin itself, via the `metadata-nemesis` dispatch at
  `xtask/src/main.rs:91`) — not a vacuum. Mutation tests: disabling the inconclusive bail
  (`nemesis.rs:322`) flips `drive_leg_refuses_the_workload_when_the_fault_did_not_materialize`
  red; disabling `heal_incomplete_reason` (`nemesis.rs:373`) flips 3 tests red including
  `drive_leg_surfaces_a_leaked_fault_even_when_the_workload_panics` — iteration 2's item 3
  (heal failure dropped under a panicking workload) is genuinely fixed AND guarded. The mocks
  drive the production `drive_leg`, not a mirror.
- **Iteration 2's headline defect (container identity across `--force-recreate`) — attempted
  to refute, could not.** The runner now resolves stable compose NAMES
  (`container_name_of`, `xtask/src/fdb_faults.rs:364`; `nemesis_netns_map`, :401), the leg's
  un-`-p`'d recreate (`nemesis.rs:809-818`) resolves to the same compose project via
  `name: wyrd-fdb-tier1-metadata` (`deploy/fdb-multi-replica/docker-compose.yml:56`), static
  IPs (`docker-compose.yml:63-64`) keep the ip→name map valid post-recreate, and
  `docker exec|pause|inspect` and `--network=container:` all accept names. Service, override
  target (`docker-compose.faketime.yml:38`), `FDB_TIER1_SKEW_SERVICE` (`fdb_faults.rs:352`)
  and probe container all derive from one resolution — the triple-mismatch is closed.

Refutations that landed:

- NEEDS-HUMAN [impl] — **The skew leg's `apply` does not wait for the cluster to re-stabilize,
  only for `docker exec` to work — the workload window opens during the restart, which Design §3
  pins as forbidden ("the leg measures skew, never the restart").**
  `crates/metadata-fault-conformance/src/nemesis.rs:848-854`: `apply` = `recreate(true)` +
  `wait_execable(90s)`, and `wait_execable` (:821) only polls `docker exec <name> date +%s`,
  which succeeds ~instantly after `up -d` — long before the recreated `fdbserver` rejoins.
  Concrete failing case: `fdb2` keeps its data in the container FS (no `volumes:` on any fdb
  service, `docker-compose.yml:65-107`), so `--force-recreate` wipes a storage/coordinator
  node; the workload then runs while the `double`-redundancy cluster is still re-forming and
  re-replicating — the leg measures restart + skew, and #408's checked workload will inherit
  the transient the brief promised was excluded. The comment at :850 ("waits for the node to
  re-stabilize (exec-able)") redefines "re-stabilize" to mean exec-able — that is the
  rationalization. Fix: after each recreate (apply AND heal), poll a survivor's `status json`
  for cluster health, as `PartitionLeg::plan` does (:573-581).
- NEEDS-HUMAN [impl] — **Heal failures are still silently dropped on the three early exit
  paths, contradicting the module's own no-leaked-fault claim.**
  `nemesis.rs:305,315,324`: `let _ = leg.heal();` on the apply-failed / confirm-failed /
  un-materialized paths, with no `heal_incomplete_reason` / `confirm_healed` check there —
  yet the docs claim "heals on **every** exit path … no leg may leave a cut cluster … behind"
  (:47-51, :301-304). Concrete failing case: partition `apply` lands 4 DROP rules, `confirm`
  reports un-materialized, `heal`'s `iptables -D` fails at rule 2 → `drive_leg` returns only
  "did NOT materialize" (:325-330) and the leaked rules go unnamed. The run still fails (no
  silent green), but this is the same defect class iteration 2's item 3 was rejected for —
  fixed only on the workload-panic path. It matters beyond xtask: #408 imports `drive_leg`
  directly and does NOT get `run_metadata_nemesis`'s `compose down -v` backstop
  (`fdb_faults.rs:461`). Apply the same leak verdict on the early paths.
- NEEDS-HUMAN [impl] — **`container_name_of` never checks `out.status`**
  (`xtask/src/fdb_faults.rs:374-395`): a failed `docker compose ps` (daemon hiccup; a compose
  version without Go-template `--format` support) yields empty stdout and is reported as
  "compose service `X` has no running container — the FDB Tier-1 cluster did not come up",
  masking the real stderr. Every other shell-out in this file surfaces the failure. Minor
  conformance nit.
- NEEDS-HUMAN — **The witnessed `WYRD_TIER1=1` three-leg run (the brief's sign-off open
  question) is still the ONLY proof of the fdb-feature wiring, and the skew leg's preload
  mechanism is host-fragile.** The bind-mount defaults to the Debian/Ubuntu host path
  (`docker-compose.faketime.yml:47`); on a host where that path is absent Docker creates a
  *directory* at the mount point, and a host-built `.so` can fail to load against the
  `foundationdb:7.3.77` image's glibc — either way `LD_PRELOAD` no-ops, the probe reads real
  time, and the leg fails loudly-as-inconclusive (the oracle behaves correctly; no silent
  pass — verified this is the failure mode, not a green). Meanwhile the `--features fdb`
  bodies (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:60+`, `run_partition` /
  `run_clock_skew` / `run_process_pause`) earn no red and are compiled by no gate this cycle
  (pinned acceptable by the brief for the *wiring*, but it means nothing at Check proves they
  even type-check). Do not accept without the witnessed run — and expect it to need an
  environment-specific `WYRD_TIER1_SKEW_SO` on non-Debian hosts. Finding #1 above should be
  fixed first or the witnessed run measures the restart, not the skew.

Attempted and could not refute: the red→green proof (reproduced both halves + 3 mutations);
the `--exact` name-drift guard (`parse_tests_run` tightened arm covered at
`xtask/tests/nemesis_orchestration.rs:169-199`; the runner gates on it at
`fdb_faults.rs:530-543`); the container-identity-across-recreate fix; the pause-encloses-the-
workload lifecycle (unpause only in `heal`, `nemesis.rs:752-758`); project-name agreement for
the un-`-p`'d recreate; `fdbcli --timeout 10` now passed (`nemesis.rs:464-475`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the claimed pre-fix compile failure is acceptable without an independent rerun — the asserted `engine/scripts/run-verify.sh` is absent and the read-only target cannot be stashed manually, so only the green suites were reproduced (`xtask/tests/nemesis_orchestration.rs:23`, `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:298`).
- [ ] C4 Verification (red→green) — Decide whether to accept provisional verification — both named suites pass (4/4 and 11/11), but red could not be rerun, and `cargo xtask ci` reached workspace tests then hit the sandbox's loopback-bind denial rather than a patch failure (`crates/chunkstore-grpc/tests/list_delete.rs:55`).
- [ ] T3 Runtime — Witness `WYRD_TIER1=1 cargo xtask metadata-nemesis` on the privileged three-process FDB deployment and confirm all legs materialize, run the commit probe, and heal — Docker exists here but libfaketime is absent, so the external topology was not exercised (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:42`, `deploy/fdb-multi-replica/docker-compose.faketime.yml:35`).
- [ ] T4 Contribution — Decide whether prior art is fully cleared — affected-path merged history was inspected locally, but closed/rejected remote work could not be mechanically queried in this sandbox, so uniqueness beyond the brief's claim remains unconfirmed.
- [ ] T5 Judgment — Decide whether pure oracle/lifecycle coverage is sufficient for this high-reach slice before a witnessed live campaign — the default tests validate decisions, but the production-path workload is only exercised by ignored FDB-feature tests at `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:184`.
- [ ] Validation — fitness-to-purpose — Decide whether one commit round-trip under each fault is adequate evidence for the intended reusable nemesis before #408 adds the checked Elle workload — this determines whether the seam is credible for its downstream purpose (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:188`).
- [ ] **The skew leg's `apply` does not wait for the cluster to re-stabilize,
- [ ] **Heal failures are still silently dropped on the three early exit
- [ ] **`container_name_of` never checks `out.status`**
- [ ] **The witnessed `WYRD_TIER1=1` three-leg run (the brief's sign-off open
- [ ] external dependency: live deploy/fdb-multi-replica cluster (docker + libfdb_c + fdb headers + in-netns iptables + libfaketime) — blocks the witnessed WYRD_TIER1=1 three-leg run (materialize + heal for partition/skew/pause); the Check-core is exercised, the live legs are not

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
- Iteration delta (if iterating): Rejected to correct the adversary's three implementation defects; the Check-core (seam, oracle arithmetic, orchestration, red→green — independently reproduced with mutation kills, and the iteration-2 container-identity fix confirmed) withstood attack and must be preserved unchanged. Fix in the next Do attempt: 1. Skew leg `apply` must wait for the CLUSTER to re-stabilize, not merely for `docker exec` to work: `wait_execable` (nemesis.rs:821, used at :848-854) succeeds long before the recreated fdbserver rejoins, and with no volumes on the fdb services (docker-compose.yml:65-107) `--force-recreate` wipes a storage/coordinator node — the workload window opens during re-replication, violating Design §3 ("the leg measures skew, never the restart"). After each recreate (apply AND heal), poll a survivor's `status json` for cluster health, as PartitionLeg::plan does (nemesis.rs:573-581). Do not redefine "re-stabilize" as "exec-able" in comments. 2. Stop dropping heal failures on the three early exit paths (`let _ = leg.heal();` at nemesis.rs:305,315,324 — apply-failed / confirm-failed / un-materialized): apply the same heal_incomplete_reason / confirm_healed leak verdict there as on the happy and panic paths. This is the same defect class iteration 2 was rejected for; it matters because #408 imports drive_leg directly and gets no `compose down -v` backstop (fdb_faults.rs:461). Guard it with a mock-leg test so removing the check goes red. 3. Minor: `container_name_of` (xtask/src/fdb_faults.rs:374-395) must check `out.status` and surface stderr instead of misreporting a failed `docker compose ps` as "cluster did not come up". Then the brief's pinned sign-off open question — the witnessed WYRD_TIER1=1 three-leg run (materialize + probe + heal) — must be performed AFTER fix 1, or it measures the restart, not the skew; expect non-Debian hosts to need an environment-specific WYRD_TIER1_SKEW_SO for the libfaketime bind-mount (docker-compose.faketime.yml:47).
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
