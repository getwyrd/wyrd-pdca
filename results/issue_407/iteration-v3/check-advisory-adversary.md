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
