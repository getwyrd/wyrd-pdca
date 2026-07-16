# PR description

## Summary
**User impact:** Wyrd's claim that the production metadata backend survives real-world
failures is only as strong as the faults it is actually tested under. Today the
multi-node metadata cluster can only be hit with one hard-wired network fault — there is
no way to test it under a lying clock or a frozen process at all, and a fault injection
that silently fails to take effect can still let the test pass green. Anyone relying on
the "survives real faults" evidence is getting less than promised.

This PR adds a reusable fault driver (a "nemesis") with three fault kinds — network
partition, clock skew, and process pause — where every injected fault must prove it
actually bit before the test counts, and must prove it was fully cleaned up afterwards.

Implements #407 (slice 4 of #329, sequenced by ADR-0041: nemesis first, then the checked
workload in #408, which composes on top of this seam).

## What to look at
The crux is one small lifecycle contract in the fault-conformance crate: a fault leg is
planned, applied, confirmed to have materialized, then healed and confirmed healed — and
the shared runner refuses two failure modes that would otherwise pass silently: a fault
that never took effect (the run fails as inconclusive) and a cleanup that leaked fault
state (a still-cut network, a still-paused container, a still-skewed clock).

To try it without any special setup, the decision logic is fully covered by two ordinary
test suites that run inside `cargo xtask ci`:

```
cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles
cargo test -p xtask --test nemesis_orchestration
```

The live three-fault campaign against the real 3-process FoundationDB cluster is opt-in
and privileged: `WYRD_TIER1=1 cargo xtask metadata-nemesis`. It needs docker, the
in-container iptables agent image, and libfaketime for the clock-skew leg (on non-Debian
hosts, point `WYRD_TIER1_SKEW_SO` at the local libfaketime shared object).

## Root cause
The existing fault implementations are private to individual test binaries
(`MasterIsolation` in `crates/metadata-fdb/tests/tier1_metadata_consistency.rs:232`,
`SymmetricPartition` in the TiKV sibling) and cannot be imported, and the shared
`ClusterFault` trait (`crates/metadata-fault-conformance/src/lib.rs:85`) is
partition-shaped by contract — so there was no seam #408 could drive the checked
workload through, and no clock-skew fault class existed in the tree at all.

## Fix
- `crates/metadata-fault-conformance/src/nemesis.rs` (new): the `NemesisLeg` lifecycle
  trait with typed per-leg materialization evidence, the central `drive_leg` runner that
  encloses the workload in the fault and enforces the inconclusive/heal rules on every
  exit path (including a panicking workload), and the three live-leg impls — in-netns
  `iptables` DROP partition, container-scoped libfaketime clock skew (cluster-node
  clocks only, never the harness clock, so recorded operation timestamps stay
  trustworthy), and freezer-cgroup `docker pause`. Plain `docker` shell-outs, no
  `libfdb_c` linkage, so the module compiles unconditionally and is importable.
- `xtask/src/nemesis.rs` (new) + `xtask/src/fdb_faults.rs`: the leg enumeration,
  dispatch routing and runner-argument building (zero new xtask dependencies), and the
  opt-in `cargo xtask metadata-nemesis` runner, which resolves containers by stable
  compose name (valid across the skew leg's forced recreates) and refuses a leg whose
  test selection ran zero tests.
- `deploy/fdb-multi-replica/docker-compose.faketime.yml` (new): the compose override
  that recreates exactly one fdbserver node with `LD_PRELOAD`/`FAKETIME`; heal recreates
  without it.
- `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs` (new): the fdb-feature wiring of
  the three legs around a minimal production-path commit round-trip; skips cleanly
  without a live cluster.
- Two added test suites covering the host-independent logic (below).

## Verification
- **Claim:** the nemesis exposes exactly the three fault classes behind one importable
  seam. **Checked:** `crates/metadata-fault-conformance/src/nemesis.rs:53`
  (`NemesisLegKind`), `:240` (`NemesisLeg`), exported at
  `crates/metadata-fault-conformance/src/lib.rs:65`; the set is pinned by
  `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:27`, so dropping a leg
  goes red, not merely absent.
- **Claim:** a fault that did not materialize fails as inconclusive — the workload never
  runs under a phantom fault. **Checked:** `drive_leg` at
  `crates/metadata-fault-conformance/src/nemesis.rs:295`, the bail at `:327-339`;
  guarded by a mock-leg test driving the production `drive_leg` at
  `tests/nemesis_oracles.rs:375` (deleting the bail turns it red).
- **Claim:** no exit path leaks fault state, and a failed heal is never silently
  dropped — not on the early exits (apply failed / confirm failed / un-materialized) and
  not under a panicking workload. **Checked:** the shared leak verdict at
  `crates/metadata-fault-conformance/src/nemesis.rs:346-374` (happy/panic paths) and
  `:383-390` (`heal_and_report`, early paths); guarded by the mock-leg tests at
  `tests/nemesis_oracles.rs:442`, `:474`, `:492` and `:524` (reverting the early-path
  heal to fire-and-forget turns two of them red).
- **Claim:** the clock-skew leg measures the skew, never the node restart its recreate
  causes (the fdb services carry no volumes, so a forced recreate wipes a node).
  **Checked:** the recovery gate `fdb_cluster_fully_recovered` at
  `crates/metadata-fault-conformance/src/nemesis.rs:455-463` — requires available AND
  data-healthy AND fully_recovered — polled from a survivor after every recreate
  (`:924-942`, called from apply/heal at `:958-967`); the gate's decision arithmetic is
  tested with recovered and not-yet-recovered status fixtures at
  `tests/nemesis_oracles.rs:216`.
- **Claim:** the live campaign has a real entry point that cannot silently no-op.
  **Checked:** `run_metadata_nemesis` at `xtask/src/fdb_faults.rs:441`; a leg whose
  `--exact` selection ran zero tests is rejected at `xtask/src/fdb_faults.rs:568`, with
  the parse/dispatch logic pinned by `xtask/tests/nemesis_orchestration.rs`.
- **Test:** `xtask/tests/nemesis_orchestration.rs` (4 tests) and
  `crates/metadata-fault-conformance/tests/nemesis_oracles.rs` (14 tests) — both fail
  pre-change (they import the new modules, so reverting the production code while
  keeping the tests fails to compile) and pass post-change; both run inside the
  unprivileged `cargo xtask ci`. The live three-leg run stays opt-in
  (`WYRD_TIER1=1 cargo xtask metadata-nemesis`) on the privileged
  `deploy/fdb-multi-replica` topology; a scheduled privileged CI job for it is #409.

Fixes #407
