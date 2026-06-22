# custodian: rebalance loop — drain / decommission evacuation + capacity plane

## Root cause
The M3 custodian dispatched GC, scrub, and reconstruction off the fenced
`reconcile_step` control point but had **no rebalance loop and no declarative
drain/decommission surface**, so an operator could not retire a D server and have
its fragments proactively evacuated — a *planned* removal could not preserve
durability the way an *unplanned* loss (reconstruction) already did. There was also
no per-failure-domain capacity signal: `core::placement` tracked per-`DServerId`
used-bytes but nothing aggregated or emitted utilization per failure domain.

## Fix
Realizes proposal 0005 slice 7 (`0005:537-540`) — the rebalance custodian loop, its
declarative drain/decommission hook, and the capacity-plane emission:

- **Declarative hook** (`crates/custodian/src/desired_state.rs`): the operator writes
  desired state marking a D server `Draining` / `Decommissioning` (a
  `desired:dserver:<id>` metadata-ledger entry, single-zone — it folds into local
  metadata, `0005:353-354`). `reconciliation_status` makes **"policy changed"**
  (`Pending`) and **"policy satisfied"** (`Satisfied`, the server holds no referenced
  fragment) distinct, observable moments (`0005:351-352`).
- **Rebalance loop** (`crates/custodian/src/rebalance.rs`), dispatched only through the
  real fenced `reconcile_step`: it reads the desired state, finds every committed chunk
  with a fragment on a draining server, and evacuates each via the **same
  commit-point-atomic, version-conditional re-place as a reconstruction**
  (`0005:298-299`, `0005:486`) — copy the intact fragment to a healthy non-draining
  server in a **distinct** failure domain first, then **one** version-conditional
  `MetadataStore::commit` repoints the placement record and orphans the displaced copy.
  A crash mid-move leaves only collectable garbage (never a torn chunk); a racing writer
  loses the CAS rather than corrupting the record. Where no free distinct domain remains,
  **spread wins** and the move is refused (`0005:302-303`, durability is gate-zero).
- **Capacity plane**: `Topology::domain_utilization` aggregates per-failure-domain
  utilization, emitted each pass on the `DurabilityTelemetry` seam (`0005:341-343`,
  ADR-0011/0012); `Topology::excluding` gives the re-placement pool so an evacuation
  never lands back on a draining server.
- The `reconcile_step` signature gains the `rebalance` dispatch slot (existing GC / scrub
  / reconstruction call sites updated to pass `None`).

Out of scope, untouched: hot-spot rebalance (`0005:301-302`), the API-first management
surface + CLI (ADR-0013, deferred `0005:355-356`), multi-zone placement, dashboards. No
on-disk-format change; the operator-facing live write is Option-A in-process (no deployed
custodian runtime exists yet, `0005:519-523`).

## Verified against
- `crates/custodian/src/reconciliation.rs:55-115` — the fenced `reconcile_step` seam,
  previously "Reconstruction / rebalance (slices 6–7) are not yet dispatched"; rebalance
  is now wired as an independent loop that reports `Changed` if it converged.
- `crates/custodian/src/rebalance.rs:1-327` — the evacuation loop: the
  copy-then-version-conditional-commit re-place, the spread-wins refusal, the lost-CAS
  conflict path, and the per-failure-domain capacity emission.
- `crates/custodian/src/desired_state.rs:1-150` — the desired-state ledger and the
  `Pending`/`Satisfied` reconciliation status that make "changed" vs "satisfied" observable.
- `crates/core/src/placement.rs:125-165` — `domain_utilization` (per-domain sum, a
  domain with no recorded utilization maps to zero) and `excluding` (the
  evacuation re-placement pool); the move reuses `select_distinct_domains_excluding`
  from the slice-6 reconstruction machinery (#144).
- `crates/custodian/src/lib.rs:23-39` — the new `rebalance` / `desired_state` modules and
  re-exports.
- Whole gate: `./engine/xtask.sh ci` in `$PDCA_WORKTREE` → `xtask ci: all checks passed`
  (fmt `--check`, clippy `-D warnings`, build, full test suite incl. the DST
  network/concurrency sweep, cargo-deny, conformance).

## Test
`crates/custodian/tests/rebalance.rs` (new, mirrors `tests/gc.rs` / `tests/scrub.rs`),
driven through the real `reconcile_step` — **5/5 pass**:
- `drains_a_d_server_and_evacuates_to_a_distinct_domain_through_reconcile_step` — the
  central drain-and-evacuate leg: one version-conditional commit, spread preserved across
  n distinct domains, "changed" → "satisfied", displaced fragment orphaned, object reads
  back unchanged.
- `spread_wins_when_no_free_distinct_domain_remains` — the move is refused rather than
  collapse spread; the drain stays `Pending` (surfaced, not silently collapsed).
- `emits_per_failure_domain_utilization_on_the_durability_seam` — capacity gauge read
  back via the Prometheus surface.
- `evacuates_two_drained_servers_of_one_chunk_in_a_single_commit` — two fragments of one
  chunk evacuated in a **single** commit (`evac.len() > 1`), distinctness preserved.
- `a_racing_writer_loses_the_version_conditional_commit_and_leaves_only_garbage` — a
  concurrent inode mutation between read and commit makes the CAS miss: the record
  reflects the racing writer (not the custodian), the copied fragment is collectable
  garbage with no orphan record (atomic, no torn move), and the conflict is emitted on
  the durability seam.

Each new test was shown load-bearing by a demonstrated assertion-level red and reverted
(recorded in `build-notes.md`): `take(1)` on the evac loop fails the multi-fragment test;
dropping the `.require(prior)` precondition fails the lost-CAS test; the v1 `draining.clear()`
flip fails the central drain test. Per ADR-0009 the bug-finding seed promotes to a
permanent seeded regression.

Fixes #145
