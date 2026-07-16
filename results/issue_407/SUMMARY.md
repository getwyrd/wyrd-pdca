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

Review of issue #407: add a reusable three-leg metadata nemesis (partition, clock skew, and process pause) with materialization and healing guarantees over the M4 FoundationDB cluster.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-ready: the default-compiled seam/oracles are owed now while the real privileged three-node campaign is explicitly deferred, and the public lifecycle contract exposes that boundary at `crates/metadata-fault-conformance/src/nemesis.rs:240`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the supplied harness result is sufficient evidence of pre-fix failure — `engine/scripts/run-verify.sh` is absent from `$PDCA_TARGET`, so I could not independently reproduce red, although both added suites are non-vacuous imports/tests (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:21`, `xtask/tests/nemesis_orchestration.rs:20`). |
| C3 Change | PASS | The review must accept a public reusable lifecycle plus thin runnable dispatch as the intended scope; the workload is enclosed and cleanup is enforced centrally at `crates/metadata-fault-conformance/src/nemesis.rs:295`, while the opt-in entry point is wired at `xtask/src/fdb_faults.rs:441`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept provisional verification or rerun on a capable host — both named suites passed locally (14 + 4 tests), but red could not be rerun because the harness is absent and `cargo xtask ci` was stopped by sandbox-denied loopback binding in an unrelated test, not by this patch (`crates/chunkstore-grpc/tests/list_delete.rs:55`). |
| C5 Causal adequacy | PASS | The prior lifecycle defects are removed at their cause: the workload executes before healing and panic/early exits share verified cleanup (`crates/metadata-fault-conformance/src/nemesis.rs:308`, `crates/metadata-fault-conformance/src/nemesis.rs:344`); the diff adds no capability probe or runtime guard of the symptom-guard kind. |
| T1 Structure | PASS | Dependency direction is coherent: reusable fault lifecycle lives in the conformance crate and xtask owns only campaign routing, with the seam exported at `crates/metadata-fault-conformance/src/lib.rs:65` and dispatch exported at `xtask/src/lib.rs:20`. |
| T2 Shape | PASS | The public shape makes all three legs and their typed materialization evidence explicit, so downstream #408 can compose without reopening lifecycle policy (`crates/metadata-fault-conformance/src/nemesis.rs:53`, `crates/metadata-fault-conformance/src/nemesis.rs:91`). |
| T3 Runtime | NEEDS-HUMAN | Witness `WYRD_TIER1=1 cargo xtask metadata-nemesis` on the privileged ≥3-process FDB topology and confirm all three legs materialize and heal — this host lacks the required `libfaketime` preload, so runtime confidence currently rests on pure tests and code-read; the bind-mounted dependency is declared at `deploy/fdb-multi-replica/docker-compose.faketime.yml:44`. |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected work already implements these affected paths — local `git log --all -- <affected paths>` establishes merged history (including #442/#257) but the offline checkout cannot mechanically settle closed/rejected PR history, which matters to whether the contribution is genuinely additive. |
| T5 Judgment | PASS | Within the declared enhancement boundary, the remaining uncertainty is isolated to external live evidence rather than an evident design or implementation defect; zero-test dispatch is rejected at `xtask/src/fdb_faults.rs:568` and the central inconclusive/heal rules are exercised at `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:375`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a witnessed live partition/skew/pause campaign demonstrates Jepsen/Elle credibility for #408 — Check proves orchestration and oracle arithmetic, but only the real privileged topology can show that each fault bites the intended FoundationDB process and the majority workload remains meaningful (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:52`). |

### Advisory — adversary

# check-advisory-adversary.md — issue #407, iteration 4

## Refutation attempts against the evidence (red→green)

- Re-ran both named tests at `$PDCA_TARGET`: `cargo test -p xtask --test nemesis_orchestration`
  (4 passed) and `cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles`
  (14 passed). The red side is structural: both test files import the new production modules
  (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:22-26` imports
  `wyrd_metadata_fault_conformance::nemesis::*`; `xtask/tests/nemesis_orchestration.rs:19-22`
  imports `xtask::nemesis::*`), so reverting the production change while keeping the tests is a
  compile failure, not a green-only vacuum. Could not refute.
- Tautology / parallel-reimplementation attack: the oracle arithmetic the tests exercise IS the
  production decision path — `drive_leg` dispatches through `MaterializationEvidence::materialized()`
  (`crates/metadata-fault-conformance/src/nemesis.rs:346`) and the live legs construct the same
  evidence structs (`nemesis.rs:687-692`, `:821-825`, `:992-995`); `fdb_cluster_fully_recovered`
  is the very function `ClockSkewLeg::wait_cluster_recovered` polls (`nemesis.rs:947`). Verified
  the mock-guard tests are mutation-sensitive by assertion structure: reverting `heal_and_report`
  to `let _ = leg.heal()` drops the leak clause the early-path tests assert on
  (`tests/nemesis_oracles.rs:495-498`, `:517-520`); deleting the inconclusive bail runs the
  workload and trips `!ran.get()` (`:424-427`). Could not refute.
- Iteration-2/3 carry-forward items verified actually fixed on the target: name-based (not id)
  resolution survives the force-recreate (`xtask/src/fdb_faults.rs:406-448` `container_name_of`
  checks `out.status` and surfaces stderr, `:454-460` `nemesis_netns_map`); the leak verdict now
  runs on every exit path including the three early ones and the panic path
  (`crates/metadata-fault-conformance/src/nemesis.rs:327-332`, `:369-393`, `:402-409`); the skew
  leg gates on cluster re-replication, not exec-ability (`nemesis.rs:977-986`, `:943-961`), with
  the parse oracle Check-tested (`tests/nemesis_oracles.rs:1248+`). `parse_tests_run` tightened
  (`xtask/src/nemesis.rs:294-308`) and pinned against the "testbeds" lookalike
  (`xtask/tests/nemesis_orchestration.rs` name-drift test). Could not refute these.

## Findings

- NEEDS-HUMAN — Toolchain unavailable; the fdb-feature half is unverifiable here (verdict
  provisional, issue #236): `cargo check -p wyrd-metadata-fdb --features fdb --tests` fails in
  this sandbox at `foundationdb-gen` (`/usr/include/foundationdb/fdb.options` absent), so the
  `#[cfg(feature = "fdb")]` bodies of `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:59-206`
  were not compiled by me and are compiled by no default gate this cycle (the brief pins this
  boundary, but it means their first compile may be the witnessed run). Static API check found no
  mismatch (`WriteBatch::new().put/.delete` builder and `commit -> CommitOutcome` match
  `crates/traits/src/lib.rs:588,659-689`; `open`/`with_prefix` match
  `crates/metadata-fdb/src/lib.rs:1260,1275`). The brief's pinned sign-off open question — one
  witnessed `WYRD_TIER1=1` three-leg run — is the only thing that can discharge the two live risks
  I could not refute by reading: (a) the skew leg force-recreates a volume-less COORDINATOR twice
  per leg (all three fdb nodes are coordinators, `deploy/fdb-multi-replica/docker-compose.yml:78-81`;
  no `volumes:` at `:66-106`), and `fdb_cluster_fully_recovered` gates on
  available/data-healthy/fully_recovered (`nemesis.rs:474-482`), none of which reports
  coordinator-state health after a wiped coordinator rejoins empty; (b) libfaketime preloaded
  into `fdbserver` (`deploy/fdb-multi-replica/docker-compose.faketime.yml:59-68`) may keep the
  node from rejoining at all — that fails loudly at `wait_cluster_recovered` (safe direction),
  but then the three-leg run is not satisfiable as the brief requires.
- NEEDS-HUMAN — Partition-leg materialization oracle is fail-open on a survivor-probe failure:
  `PartitionLeg::peers_see_target_live` maps a FAILED probe (`survivor_status_json` → `None`,
  e.g. the fdbcli 10s timeout or a docker hiccup) to "target not live"
  (`crates/metadata-fault-conformance/src/nemesis.rs:634-639`), so during the 45s confirm window
  (`nemesis.rs:672-686`) one transient probe failure produces the `during=false` flip and
  MATERIALIZES a cut that never bit. Concrete failing case: an iptables image whose DROP silently
  no-ops (the exact #399 host-networking regression class this oracle exists to catch) + one
  fdbcli timeout during the window ⇒ the leg reports materialized and the regression passes.
  Mitigations are real (the `before` sample uses the same probe and must be `true`,
  `tests/nemesis_oracles.rs` pins that; the pause leg is immune because `inspected_paused_during`
  independently proves the freeze) and the mapping deliberately mirrors the blessed #442
  semantics (`crates/testkit/src/lib.rs:607-619`, `tier1_metadata_consistency.rs:262-268`) — and
  requiring `Some(false)` instead might make REAL master cuts unmaterializable (a survivor's
  fdbcli can legitimately fail mid-recovery). Whether to accept the mirrored precedent or
  strengthen the during-sample (e.g. require at least one successful post-flip status parse) is a
  judgment call, not a mechanical fix — hence no `[impl]`.
- NEEDS-HUMAN [impl] — `PartitionLeg::heal` aborts on the FIRST failed `iptables -D` and never
  attempts the remaining rules (`crates/metadata-fault-conformance/src/nemesis.rs:694-711`
  returns via `?` at the first `self.iptables(&args)?` failure), unlike the peer technique it
  re-implements: `MasterIsolation::heal` attempts every rule, collects the first error, AND
  retries residue in a `Drop` guard (`crates/metadata-fdb/tests/tier1_metadata_consistency.rs:293-316,336+`).
  Concrete case: `-D` on rule 1-of-4 fails transiently; rules 2-4 — whose removal would have
  succeeded — are left in place, and for a #408 importer (no `compose down -v` backstop,
  `nemesis.rs:401` names exactly this) the cluster stays maximally cut. The leak IS surfaced as
  an error (not silent — `heal_incomplete_reason`), but the module's own claim "no leg may leave
  a cut cluster … behind" (`nemesis.rs:50-51`) is best-effort here where the mirrored precedent
  does strictly more. Builder-fixable: continue past per-rule failures, return the partial
  `healed` list with the first error.

## Minor (not lifted)

- Test-fixture realism nit: `tests/nemesis_oracles.rs` "re_replicating" fixture pairs
  `"name":"healthy_repartitioning"` with `"healthy":false` — in real FDB output the
  `healthy_*`-named states carry `healthy:true` (the degraded re-replication states are
  `healing`/`missing_data`). The oracle only reads the `healthy` bool so nothing is wrong in
  production; the fixture just illustrates a status shape FDB never emits.
- `fdb_cluster_fully_recovered`'s anchored substring parse (`nemesis.rs:488-510`) was attacked
  for mis-anchoring (`"data":` vs `"moving_data":`, `client` vs `cluster` sections): the
  quote-inclusive needles and FDB's alphabetical key order make each anchor land on the intended
  object; could not construct a realistic status body that fools it.

## Verdict

Attempted to refute: the red→green proof (re-ran both sides' logic), tautology/mock-vacuum in the
guard tests, all five iteration-1..3 carry-forward defect classes, the compose project/name
resolution across the force-recreate, the `--exact` name-drift guard, and the status-json parse
anchors — could not. What remains is above: two live-run/judgment items for the human (the
witnessed `WYRD_TIER1=1` run is genuinely load-bearing this cycle) and one builder-fixable heal
robustness gap. The Check-core half is the strongest of the four iterations.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Decide whether the supplied harness result is sufficient evidence of pre-fix failure — `engine/scripts/run-verify.sh` is absent from `$PDCA_TARGET`, so I could not independently reproduce red, although both added suites are non-vacuous imports/tests (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:21`, `xtask/tests/nemesis_orchestration.rs:20`).
- [x] C4 Verification (red→green) — Decide whether to accept provisional verification or rerun on a capable host — both named suites passed locally (14 + 4 tests), but red could not be rerun because the harness is absent and `cargo xtask ci` was stopped by sandbox-denied loopback binding in an unrelated test, not by this patch (`crates/chunkstore-grpc/tests/list_delete.rs:55`).
- [x] T3 Runtime — Witness `WYRD_TIER1=1 cargo xtask metadata-nemesis` on the privileged ≥3-process FDB topology and confirm all three legs materialize and heal — this host lacks the required `libfaketime` preload, so runtime confidence currently rests on pure tests and code-read; the bind-mounted dependency is declared at `deploy/fdb-multi-replica/docker-compose.faketime.yml:44`.
- [x] T4 Contribution — Confirm no closed/rejected work already implements these affected paths — local `git log --all -- <affected paths>` establishes merged history (including #442/#257) but the offline checkout cannot mechanically settle closed/rejected PR history, which matters to whether the contribution is genuinely additive.
- [x] Validation — fitness-to-purpose — Decide whether a witnessed live partition/skew/pause campaign demonstrates Jepsen/Elle credibility for #408 — Check proves orchestration and oracle arithmetic, but only the real privileged topology can show that each fault bites the intended FoundationDB process and the majority workload remains meaningful (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:52`).
- [x] Toolchain unavailable; the fdb-feature half is unverifiable here (verdict
- [x] Partition-leg materialization oracle is fail-open on a survivor-probe failure:
- [x] `PartitionLeg::heal` aborts on the FIRST failed `iptables -D` and never

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
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_407 sign-off blocked on missing host package `foundationdb-clients` 7.3.x (libfdb_c / fdb_c.h / fdb.options / fdbcli) — the witnessed `WYRD_TIER1=1` run cannot compile `--features fdb` without it; consider provisioning + a doctor row tied to the witnessed-run path.
- issue_407 sign-off blocked on missing host package `libfaketime` (skew-leg bind-mount `WYRD_TIER1_SKEW_SO`) — consider provisioning + a doctor row so the Tier-1 nemesis prerequisite is preflighted, not discovered at sign-off.
