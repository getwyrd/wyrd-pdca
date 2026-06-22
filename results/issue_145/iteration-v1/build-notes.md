# Build notes — issue 145 / m3.7-rebalance-drain-decommission

Target: `getwyrd/wyrd @ main` (worktree `$PDCA_WORKTREE`). Planning artifact:
`docs/design/proposals/accepted/0005-milestone-3-custodians.md` (slice 7, `0005:537-540`),
read as authoritative. #144 (reconstruction) is merged on the worktree base
(`git log`: `5fb905c custodian: reconstruct under-replicated chunks…`), so the
commit-point-atomic re-place machinery the slice reuses is present.

## What the slice required (the three BINDING legs + the invariant)

1. Drain/decommission **evacuation**: an operator marks a D server draining; the custodian
   moves its referenced fragments to healthy servers in **distinct failure domains** via
   the **same commit-point-atomic, version-conditional `commit` re-place as a
   reconstruction** (`0005:298-299`, `0005:486`); after which the drained server holds no
   referenced fragment, full redundancy + spread preserved (**spread wins**, `0005:302-303`).
2. **"Policy changed" vs "policy satisfied"** as distinct observable moments (`0005:351-352`).
3. **Per-failure-domain utilization** emitted on the `DurabilityTelemetry` seam (`0005:341-343`).

Invariant to restore (proposal 0005 §Rebalance / §Declarative hook): every custodian
re-placement — reconstruction **or** rebalance — preserves failure-domain distinctness and
is commit-point-atomic; declarative management is observably reconciling. Stated over the
re-placement / declarative-reconciliation *category*; spans the desired-state surface + the
rebalance dispatch + the shared failure-domain selector + the telemetry seam.

## Change shape (smallest change that restores the invariant, per surface it spans)

- **Shared failure-domain selector** (`crates/core/src/placement.rs`): two thin additions to
  the existing thin domain model — `Topology::excluding(&self, exclude)` (the re-placement
  pool that omits draining servers, so an evacuation never lands back on a draining server)
  and `Topology::domain_utilization()` (per-domain capacity aggregation for leg 3). The
  distinctness contract itself is unchanged — rebalance reuses `select_distinct_domains_excluding`
  **verbatim**, the same call reconstruction uses (`reconstruction.rs:318`), so distinctness
  holds identically on write, reconstruction repair, and drain evacuation.
- **Desired-state surface** (`crates/custodian/src/desired_state.rs`, new): the declarative
  hook — `set_lifecycle`/`clear_lifecycle`/`draining_servers` over a `desired:dserver:<id>`
  metadata ledger (mirrors the `pending:`/`orphan:`/`repair:` pattern, no new backend), plus
  `reconciliation_status` → `{NotRequested, Pending, Satisfied}`. "Satisfied" is computed
  from the **committed** placement records (`gc::referenced_fragments`), i.e. reality, which
  makes "changed" (record exists, still referenced ⇒ `Pending`) and "satisfied" (no longer
  referenced) two distinct, observable moments.
- **Rebalance dispatch** (`crates/custodian/src/rebalance.rs`, new): the loop —
  read desired state → find committed chunks with a fragment on a draining server → for each,
  pick a non-draining server in a domain distinct from the survivors → **copy** the intact
  fragment there FIRST → **one** version-conditional `commit` that repoints the placement +
  orphans the displaced fragment. Wired into the fenced control point by adding a
  `rebalance: Option<&RebalanceContext>` slot to `reconcile_step` (`reconciliation.rs:62-71`),
  the same additive shape #144 used for `reconstruction`.
- **Telemetry seam**: `emit_domain_utilization` emits a `gauge.capacity_domain_utilization`
  per domain (with a `domain` label) every pass; evacuations/conflicts emit counters + audit
  events, matching the gc/scrub/reconstruction idiom.

## Why copy, not erasure-rebuild (and the cost of the alternative)

For a **drain** the fragment is intact on the alive (draining) server, so the move **copies**
the bytes — it does not run `erasure::reconstruct` + `erasure::encode`. The proposal's "same
commit-point-atomic re-place" (`0005:298-299`) is about the *atomic repoint*, not the decode;
the literal shared piece is the failure-domain selector (`select_distinct_domains_excluding`)
and the write-then-one-conditional-commit pattern, both reused. Re-deriving the shard through
erasure would add a full decode+encode per evacuated fragment for **zero** durability benefit
(the bytes are already verified intact via `repair::fragment_intact`) — and would *fail* a
drain of a chunk reduced to exactly `k` survivors elsewhere only to move one healthy fragment.
I did **not** refactor `reconstruction::repair_chunk` into a shared helper: the two differ in
their gather step (decode vs. copy) and a forced merge would couple two loops for ~30 lines of
superficially-similar commit-building; the genuinely shared logic already lives in `core`
(the selector) where both call it.

## Spread wins (durability is gate-zero)

`evacuate_chunk` selects from `topology.excluding(draining)` with the survivors' domains
excluded. If no free distinct domain remains, `select_distinct_domains_excluding` returns
`InsufficientDomains` and the move is **aborted** — the fragment stays on the draining server
rather than collapse the chunk onto fewer than `n` domains. The drain then stays `Pending`
(surfaced, not silently satisfied). Proven by `spread_wins_when_no_free_distinct_domain_remains`.

## Red→green evidence

- Test file `crates/custodian/tests/rebalance.rs` (mirrors `tests/gc.rs`/`scrub.rs`): three
  tokio in-process tests through the **real** `reconcile_step`. Pre-fix it cannot compile
  (the `rebalance`/`desired_state` modules and `RebalanceContext` are net-new) — the
  criterion-absence red the brief's NET-NEW posture expects on an added file.
- **Demonstrated assertion-level red** for the load-bearing evacuation leg (brief requires it,
  à la scrub's `fragment_intact` negation): I temporarily added `draining.clear();` after the
  desired-state read in `rebalance::reconcile` (treat the drain as absent). Result:
  `drains_a_d_server_and_evacuates_to_a_distinct_domain_through_reconcile_step` failed at
  `rebalance.rs:308` — `left: Satisfied, right: Changed` ("the draining server's fragment was
  evacuated"). Reverted immediately; the seam is load-bearing, not resting red on non-existence.
- Post-fix green: `cargo test -p wyrd-custodian` (rebalance 3/3 + gc/scrub/reconstruction/skeleton
  unchanged) and `cargo test -p wyrd-core --lib placement` (6/6, incl. the two new aggregation
  tests) all pass.
- Whole gate: `./engine/xtask.sh ci` in `$PDCA_WORKTREE` → **`xtask ci: all checks passed`**
  (fmt --check, clippy -D warnings, build, test, cargo-machete, cargo-deny, conformance, DST sweep).
- Commit-readiness: `cargo fmt --check` and `cargo clippy --all-targets` clean on both touched
  crates (the target's own pre-commit gates).

## Notes / scope boundaries honoured

- `reconcile_step` gained one `Option` slot; all 15 existing call sites (gc/scrub/reconstruction/
  skeleton tests) updated to pass `None` — mechanical, one logical change (the rebalance slot).
- Out of scope, untouched: hot-spot rebalance (`0005:301-302`), the API-first management
  surface + CLI (ADR-0013), multi-zone placement, dashboards. No on-disk-format change; the
  desired state is a metadata-ledger entry only (ADR-0010 dependency boundary kept — custodian
  stays over `traits`/`core`).
- The DST campaign (slice 8) is separate; these are in-process tokio loop tests, the same shape
  the merged gc/scrub/reconstruction slices ship.
