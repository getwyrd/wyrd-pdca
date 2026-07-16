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
