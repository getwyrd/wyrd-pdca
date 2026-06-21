# Promote the GC stand-in into a running GC custodian loop

> Proposal 0005 (Milestone 3 — custodians), PR-sequence slice 4 (the GC custodian).
> Direct successor of #141 / PR #187 (the M3.3 reconciliation scaffold, merged).

## Root cause
GC was a test-invoked stand-in: `core::sweep_expired_leases` removed expired
`pending:` ledger entries but explicitly deferred reclaiming the fragment *bytes*
to "a later milestone", so the leased garbage from failed fan-out and the orphans
left by deletes and completed reconstructions accumulated unreclaimed. The fenced
reconciliation control point existed but dispatched no maintenance work — it
unconditionally reported a satisfied zone — so no running loop ever reclaimed
those bytes.

## Fix
A GC reconciler is now dispatched from the real fenced control point
(`reconcile_step`), reclaiming both GC inputs through `ChunkStore::delete_fragment`:
the bytes behind an expired pending lease, and orphaned fragments present in a D
server's `list_fragments` but referenced by no committed chunk map. GC is the one
production entry the control point runs (not a parallel test-only path — the
binding anti-#141 guard). Two invariants gate every reclaim: a fragment a
committed chunk map's placement record points at is never deleted (the
silent-corruption invariant), and an orphan is reclaimed only after a reader-safe
grace window has elapsed past the instant it was stranded — a caller-derived
window, not a baked-in constant. Each reclaim and skip is emitted on the existing
`DurabilityTelemetry` seam as a metric plus an append-only audit event. No
deployed custodian process drives the loop yet (a later cross-cutting slice for
all four loops); this makes GC correct over the trait stores and reachable
through the real control point.

## Verified against
- `crates/custodian/src/reconciliation.rs:28-33` (origin/main @ `40c3413`) — the
  scaffold `reconcile_step` returned `Reconciled::Satisfied` unconditionally and
  dispatched no loop; the patch makes it `async`, takes `Option<&GcContext>` +
  `now_millis`, and dispatches to `gc::reconcile` after the fence check.
- `crates/core/src/write.rs:330-332` — `sweep_expired_leases`'s own note that
  orphaned fragments are collectable garbage whose reclaim "needs a chunk-store
  delete (a later milestone)"; this is that reclaim, done in the custodian loop.
- `crates/core/src/metadata.rs:94` — `ChunkRef.placement`, the committed
  references the GC safety gate scans so a referenced fragment is never reclaimed.
- `crates/traits/src/lib.rs:100,108` — `ChunkStore::list_fragments` (orphan scan
  input) and `delete_fragment` (the byte reclaim), both added in #140 / PR #186.
- `crates/custodian/src/telemetry.rs:116,135` — `DurabilityTelemetry::metrics_layer`
  / `gather_prometheus`, the backend-agnostic seam GC actions are emitted on and
  read back in-process.

## Test
`crates/custodian/tests/gc.rs` (new), driving GC through the real `reconcile_step`
control point over in-memory trait stores:
- `reclaims_expired_lease_byte_and_orphan_through_reconcile_step` — both inputs are
  reclaimed; the committed reference and an unswept ledger entry are left intact.
- `never_reclaims_a_referenced_fragment` — flippable: a stale orphan record points
  at a referenced fragment, so only the reference check protects it; negating that
  check deletes it and the assertion fires.
- `honours_the_reader_safe_grace_window` — flippable: a within-grace orphan is not
  reclaimed and a reader still resolves it; it is reclaimed once the window elapses.
- `emits_gc_actions_on_the_durability_seam` — the reclaim and skip metrics are
  read back via `gather_prometheus`.

The reconciler, orphan scan, and GC dispatch are net-new code arriving with their
test (the scaffold previously returned `Satisfied` unconditionally), so legs 1 and
4 are born-at-tier coverage; legs 2 and 3 are genuine red→green flips. Whole-tree
`cargo xtask ci` is the gating check; the existing `reconcile_step` caller
(`tests/skeleton.rs`) was updated to the new signature and stays green.

Fixes #142
