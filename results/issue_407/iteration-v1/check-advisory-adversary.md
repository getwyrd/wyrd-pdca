# check-advisory-adversary.md — issue #407 (m4-metadata-nemesis-partition-skew-pause)

Adversarial pass. Target: `$PDCA_TARGET` = `/home/eddie/development/wyrd/wyrd.pdca-wt` (patch applied). I re-ran both
named Check tests green (`cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles` → 5 passed;
`cargo test -p xtask --test nemesis_orchestration` → 3 passed); the red leg is a genuine compile-red (both test files
import the reverted `…::nemesis` modules — `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:12-15`,
`xtask/tests/nemesis_orchestration.rs:18-20` — no parallel re-implementation). The evidence itself holds for the pinned
Check-core scope. The findings below are where the fix, its live legs, and one reviewer-adjacent claim break.

- NEEDS-HUMAN [impl] — **The live clock-skew leg cannot materialize with its own defaults — service/override/probe
  triple-mismatch.** `deploy/fdb-multi-replica/docker-compose.faketime.yml:30` hardcodes the override to service
  `fdb1`, but `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:117` defaults `WYRD_TIER1_SKEW_SERVICE` to
  `"fdb2"`, and :118-121 probes `all.last()`'s container (= fdb2, per the runner's netns-map order
  `xtask/src/fdb_faults.rs:39-43`) *independently of `service`*. Concrete failing case: default env →
  `docker compose -f base -f faketime up -d --force-recreate fdb2` recreates fdb2 with **no** `LD_PRELOAD`/`FAKETIME`
  (the override declares only `fdb1`) → the `date +%s` probe reads a true clock → offset ≈ 0 < floor 60 →
  `SkewEvidence::materialized()` false (`crates/metadata-fault-conformance/src/nemesis.rs:194`) → every live skew run
  fails inconclusive. Setting `WYRD_TIER1_SKEW_SERVICE=fdb1` instead skews fdb1 while still probing fdb2. The comment
  at :115 ("Skew a NON-master node") is also unenforced — nothing checks `all.last()` isn't the master.

- NEEDS-HUMAN [impl] — **The pause leg's live sampling is exactly the "single probe" its own contract forbids.**
  `ProcessPauseLeg::confirm_materialized` samples `served_during` once, immediately after `docker pause`
  (`crates/metadata-fault-conformance/src/nemesis.rs:588`), with no settle window — but a survivor's
  `fdbcli status json` keeps reporting the frozen target live until FDB's failure detector times out (seconds).
  Concrete failing case: pause bites, probe lands at t+~1-2s while the survivor still lists the target →
  `served_during=true` → `PauseEvidence::materialized()` false → the live leg is near-deterministically inconclusive.
  Contrast the same file's `PartitionLeg::confirm_materialized`, which polls the flip for 45s (nemesis.rs:470), and
  the peer `peers_still_see_target_live_after` window poll
  (`crates/metadata-fdb/tests/tier1_metadata_consistency.rs:279-288`). The `nemesis_oracles` test named
  "…not_a_single_probe" pins the *arithmetic*, not the *sampling*, so Check stays green over this.

- NEEDS-HUMAN [impl] — **`drive_leg` leaks fault state on every non-happy path, despite quoting "Invariant B forbids
  leaked fault state".** (a) `leg.apply()?` (`crates/metadata-fault-conformance/src/nemesis.rs:264`) returns without
  healing: `PartitionLeg::apply` (nemesis.rs:455-464) inserts 4 iptables DROP rules one at a time, so a failure at
  rule 3 leaks 2 rules into the cluster netns with no removal, no Drop guard — the peer `MasterIsolation` has exactly
  this guard (`Drop` retry of residue, `crates/metadata-fdb/tests/tier1_metadata_consistency.rs:309-316`) and it was
  not mirrored. (b) The skew leg applies its fault in `plan()` (nemesis.rs:706 `recreate(true)`), so a plan failure
  after the recreate (container never exec-able, :733) errors out of `drive_leg` **before** the heal path exists,
  leaving a permanently skewed node. (c) A panicking workload — and the shipped workload panics by design via
  `expect`/`assert_eq!` (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:1170-1196` region, `cluster_still_serves`)
  — unwinds past `workload()` (nemesis.rs:276) with no `catch_unwind`, skipping `heal()` entirely: a failed
  partition-leg assertion leaves the cluster cut for every subsequent leg. Since `drive_leg` is the seam #408 consumes
  (no compose-down teardown wraps it there), this is not covered by the xtask runner's unconditional teardown either.

- NEEDS-HUMAN — **Nothing can run the nemesis legs: the dispatch was built but never wired, and the in-tree docs claim
  otherwise.** `FDB_TIER1_LEGS` (`xtask/src/fdb_faults.rs:52-56`) still lists only the three #442 legs; no xtask
  command consumes `xtask::nemesis::{metadata_nemesis_legs, nemesis_scenario_args}` — their only caller is the
  orchestration test itself. Yet the new test binary's `#[ignore]` strings and module doc say the legs "run only
  under … `cargo xtask fdb-metadata-tier1`" (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:16,37,47,57,164`) —
  false on this tree. The brief's ordering note anticipated this patch touching `xtask/src/main.rs`; it doesn't, and
  the sign-off open question ("one witnessed local `WYRD_TIER1=1` run of the three legs") is unsatisfiable via xtask
  as landed — only a hand-built `cargo test --features fdb …` incantation with 6+ env vars can run them. Human call:
  is runner wiring in-scope for #407 (the brief's "runner-argument building" suggests it feeds *something*), or
  legitimately deferred to #408/#409? If deferred, the doc strings still need the [impl] fix.

- NEEDS-HUMAN [impl] — **The rename-safety claim is false: `--exact` on a missing function exits 0.** The doc at
  `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:21-23` claims "the runner selects each with `--exact`, so
  renaming one here without updating that dispatch would fail the leg". Verified on target:
  `cargo test -p wyrd-metadata-fdb --test tier1_metadata_nemesis -- --ignored --exact no_such_leg_fn` → "0 passed …
  ok", exit 0. So a renamed scenario fn (or a stale `scenario_fn` name, `xtask/src/nemesis.rs:44-49`) silently turns
  a leg into a green no-op; nothing pins the xtask names to the actual `#[test]` names. Fix: the (future) runner must
  assert exactly one test ran, or a Check test must pin the correspondence.

- NEEDS-HUMAN [impl] — **The brief's "lifecycle + oracle arithmetic" Check claim is only half-delivered: `drive_leg`'s
  two #442 gates are exercised by no test.** `nemesis_oracles.rs` covers the evidence arithmetic, enum and parse
  helpers only; no test drives `drive_leg` with a mock `NemesisLeg`. Deleting the inconclusive bail
  (`crates/metadata-fault-conformance/src/nemesis.rs:266-274`) or the `heal_is_complete` check (:279-285) flips
  nothing red at Check — the central "un-materialized fault FAILS, never passes silently" rule (success criterion,
  brief line 22-27; module doc nemesis.rs:15-23) is itself unguarded. A ~30-line mock-leg test closes this.

- **Minor (no adjudication needed):** `survivor_status_json` (`crates/metadata-fault-conformance/src/nemesis.rs:339-347`)
  drops the `--timeout 10` that the peer `support::status_json` passes to `fdbcli`
  (`crates/metadata-fdb/tests/support/mod.rs:55`); a wedged survivor probe can stall the 45s/60s poll loops well past
  their nominal windows.

**Attempted and could not refute:** the red→green evidence for the two named test files (re-ran green; red is a real
compile-red against the production modules, not a mirror copy); the C4 `xtask ci` pass; the oracle arithmetic itself
(tried boundary cases — `floor_secs=0` guarded at nemesis.rs:194, `unsigned_abs` handles `i64::MIN`, crash-vs-partition
and crash-vs-pause confusions are rejected by `target_running_during`/`inspected_paused_during`); the two mirrored
`NemesisLegKind` enums are per-brief (xtask zero-dep constraint), not a defect. The Check-core is sound; every finding
above lives in the live-leg half the gates never execute — which is precisely where a confirmatory review would
rationalize "deferred green" into "presumed green".
