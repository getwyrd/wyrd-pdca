# build-notes.md — issue #407, iteration 4

## What this iteration is

The Check-core half of the seam (leg enumeration, per-leg materialization oracles, the
central `drive_leg` rule, orchestration argv, red→green) withstood three adversarial passes
and is **preserved unchanged** (iteration-3 sign-off: "the Check-core … withstood attack and
must be preserved unchanged"). This iteration addresses only the three live-leg
implementation defects the iteration-3 carry-forward names. I took the accepted iteration-v3
patch as the base and made three surgical fixes.

## The three fixes (iteration-3 carry-forward)

### Fix 1 — skew leg `apply`/`heal` wait for the CLUSTER to re-stabilize, not merely for `docker exec`

Carry-forward item 1 (and the adversary's landed refutation #1): `ClockSkewLeg::apply` was
`recreate(true)` + `wait_execable(90s)`, and `wait_execable` only polls
`docker exec <name> date +%s`, which succeeds ~instantly after `up -d` — long before the
recreated `fdbserver` rejoins. Because the fdb services carry **no `volumes:`**
(`deploy/fdb-multi-replica/docker-compose.yml`), `--force-recreate` wipes a
storage/coordinator node, so the workload window opened during re-replication — the leg
measured *restart + skew*, contradicting Design §3 ("the leg measures skew, never the
restart").

- Added a pure `fdb_cluster_fully_recovered(status_json)` helper
  (`crates/metadata-fault-conformance/src/nemesis.rs`, in the "Pure parse helpers" section,
  same style as `wyrd_testkit::fdb_peer_sees_target_live`). It requires **all three** of
  `client.database_status.available`, `cluster.data.state.healthy` (replication restored —
  `available` alone flips true while shards are still moving), and
  `cluster.recovery_state.name == "fully_recovered"`. Unit-tested at Check
  (`nemesis_oracles.rs::skew_recovery_gate_needs_available_healthy_and_fully_recovered_not_merely_available`)
  with a fully-recovered fixture (true) and three not-recovered fixtures (re-replicating /
  transaction-recovering / unavailable → false). This is the very function the live leg calls,
  so a decision regression flips both.
- `ClockSkewLeg` gained a `survivor_container` field + `wait_cluster_recovered(timeout)` that
  polls a **survivor's** `status json` (never the skewed node, which is mid-recreate) until
  `fdb_cluster_fully_recovered`. `apply` and `heal` both call it after their recreate. The
  `apply` comment no longer redefines "re-stabilize" as "exec-able" (the rationalization the
  adversary flagged).
- Runner plumbing: `xtask/src/fdb_faults.rs` resolves a survivor **by stable compose name**
  (`fdb_tier1_skew_survivor_service` → `container_name_of`) and exports
  `WYRD_TIER1_SKEW_SURVIVOR`; `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs` reads it
  and passes it to `ClockSkewLeg::new`. Name-based (not id) keeps it valid across the leg's own
  force-recreate — the iteration-2 container-identity fix the adversary confirmed still holds.

**Why not something cheaper (e.g. a fixed sleep):** a sleep neither proves recovery nor
adapts to a slow re-replication; polling the survivor's real health is the same
survivor-side discipline `PartitionLeg::plan` already uses (the carry-forward's prescription).
Cost of the chosen fix over a sleep: +1 struct field, +1 helper method, +1 pure parser
(~30 lines) — bounded, and it is the only shape that actually excludes the restart transient
the brief promised #408 would not inherit.

### Fix 2 — early exit paths apply the same heal-leak verdict (no dropped leak)

Carry-forward item 2 (landed refutation #2): the three early paths did `let _ = leg.heal();`
(apply-failed / confirm-failed / un-materialized), dropping a heal that itself failed/partial —
a leaked cut/pause/skew the primary error never named. Fixed only on the panic path in v3.

Introduced `heal_and_report(leg, primary)` which heals, runs the **same**
`heal_incomplete_reason` verdict the happy/panic paths use, and folds any leak into the
returned error (`"{primary}; ADDITIONALLY the heal leaked fault state — …"`). All three early
paths now call it. Matters beyond xtask: **#408 imports `drive_leg` directly and gets no
`compose down -v` backstop**, so this is its only guard.

Guarded with two new mock-leg tests (`nemesis_oracles.rs`):
`drive_leg_surfaces_a_leaked_heal_on_the_un_materialized_early_path` and
`…_when_apply_fails`. Reverting `heal_and_report` to `let _ = leg.heal()` makes both go red
(shown below).

### Fix 3 — `container_name_of` checks `out.status` and surfaces stderr

Carry-forward item 3 (refutation #3, minor): a failed `docker compose ps` (daemon hiccup; a
compose version without Go-template `--format`) yielded empty stdout and was misreported as
"the cluster did not come up". Added an `out.status.success()` check that returns the real
stderr, matching every other shell-out in the file.

## Refutation (forced, recorded)

- **(a) Genuine red?** YES. Reverting `heal_and_report` to the old `let _ = leg.heal();`
  behavior turned both new early-path tests red
  (`drive_leg_surfaces_a_leaked_heal_on_the_un_materialized_early_path` … FAILED;
  `…_when_apply_fails` … FAILED; `test result: FAILED. 12 passed; 2 failed`), then green again
  after restoring (`14 passed`). The Check-core red posture is unchanged from v3: reverting
  `pub mod nemesis;` fails both named test binaries to compile (falsifiability §, brief). For
  Fix 1, `fdb_cluster_fully_recovered` is new, so the recovery-gate test cannot pass without
  it; weakening it to check only `available` makes the `re_replicating` fixture return true →
  red.
- **(b) Production path?** YES. The mock-leg tests drive the **production** `drive_leg`
  (imported from the crate, not a copy) — the mock only stands in for the I/O-bound leg impls,
  exactly as the adversary confirmed for v3 ("the mocks drive the production `drive_leg`, not a
  mirror"). `fdb_cluster_fully_recovered` is the same function the live `ClockSkewLeg`
  calls; `skew_recovery_gate_…` exercises that production function directly.
- **(c) Fixture includes the fault?** YES. The recovery-gate test's not-recovered fixtures
  contain the actual failing states (data still re-replicating; transaction subsystem still
  recovering; unavailable) — it does not curate them out. The early-path leak tests use a mock
  configured to *leak* (heal into nothing, never recover), i.e. the fault under test is present.

## What is honestly NOT exercised at Check (unchanged from v3, pinned by the brief)

The `--features fdb` live-leg bodies in `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs`
(`run_partition`/`run_clock_skew`/`run_process_pause`) and the live `docker`/`fdbcli`
shell-outs in `nemesis.rs` are **off-Check** — they need a live 3-process FDB cluster +
`libfdb_c` + privileged in-netns `iptables` + `libfaketime`. The default `cargo xtask ci` gate
compiles the non-fdb skeleton (verified: `cargo test -p wyrd-metadata-fdb --test
tier1_metadata_nemesis --no-run` builds), and the pure decision logic those legs turn on is
unit-tested at Check. Per the brief's verification posture, "compiled by ci" is claimed only
for the default-compiled surface.

### NEEDS-HUMAN (the brief's pinned sign-off open question)

The witnessed `WYRD_TIER1=1 cargo xtask metadata-nemesis` run of all three legs (materialize +
probe + heal) on the privileged `deploy/fdb-multi-replica` topology remains the only proof of
the fdb-feature wiring, and MUST be performed AFTER Fix 1 or it measures the restart, not the
skew. Expect non-Debian hosts to need an environment-specific `WYRD_TIER1_SKEW_SO` for the
libfaketime bind-mount (`docker-compose.faketime.yml`). This is an irreducibly
topology/privilege-bound validation (an environment *shape*, not an installable tool — the
brief's External dependencies note it is "no-check"), so it belongs at human sign-off, not a
fabricated headless test.

## Gate evidence run locally (worktree `$PDCA_WORKTREE`)

- `cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles` → 14/14 pass.
- `cargo test -p xtask --test nemesis_orchestration` → 4/4 pass.
- `cargo clippy -p wyrd-metadata-fault-conformance -p xtask --all-targets` → clean (`-D warnings`).
- `cargo fmt --check -p wyrd-metadata-fault-conformance -p xtask -p wyrd-metadata-fdb` → clean
  (commit-ready for the target's rustfmt hook).
- Full `cargo xtask ci` not run to green here: the v3 review recorded it reaching workspace
  tests then hitting the sandbox's loopback-bind denial in an unrelated crate
  (`crates/chunkstore-grpc/tests/list_delete.rs`), a sandbox limitation, not a patch failure.
</content>
</invoke>
