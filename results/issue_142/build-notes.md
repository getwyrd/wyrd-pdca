# Build notes — issue 142 / m3.4-gc-custodian

Target: `getwyrd/wyrd` @ `main` (base `origin/main` @ `40c3413`). All edits in
`$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`). Line cites below are on that tree
(== base + this patch).

## What the brief asked for

Promote the test-invoked GC stand-in into a **running GC custodian loop** dispatched
through the real fenced control point (`custodian::reconcile_step`), reclaiming the two
GC inputs (expired pending-ledger lease bytes + orphaned fragments) via
`ChunkStore::delete_fragment` after a reader-safe grace window, never reclaiming a
referenced fragment, emitting on the `DurabilityTelemetry` seam. Read against proposal
0005 §GC (`0005:288-295`), Q3 (`0005:394-397`), graduation invariants (`0005:486-488`),
slice 4 (`0005:524-527`), grace-window open question (`0005:585-586`).

## The change (5 source/manifest files + 1 new test)

- **`crates/custodian/src/gc.rs` (new)** — the GC loop. `gc::reconcile(ctx, now)` is the
  one production entry; `GcContext` bundles the `&dyn MetadataStore`, the fleet
  `&[(DServerId, &dyn ChunkStore)]`, and the derived `grace_window_millis`. Algorithm:
  1. `referenced_fragments` — scan `inode:` keys, decode `InodeRecord`, keep only
     `state == Committed`, collect `(placement[i], FragmentId{chunk, i})`. This is the
     safety set (the invariant gate).
  2. `expired_pending_chunks` — scan `pending:`, decode `PendingEntry`, keep
     `lease_expiry_millis <= now` (input a; the lease TTL is its own grace).
  3. `orphan_leases` — scan a new `orphan:<d>:<chunk>:<idx>` ledger → `orphaned_at`
     (input b's grace record).
  4. For each fleet store's `list_fragments`: skip if referenced; else reclaim if an
     orphan whose `now >= orphaned_at + grace`, or an expired-pending chunk; else keep
     (no deadline ⇒ no reclaim — reader-safe by construction).
  5. `delete_fragment` the bytes, then one atomic `commit` retires the swept
     `pending:`/`orphan:` ledger entries.
  Plus `pub async fn mark_orphaned(...)` — the orphan-ledger write API the
  delete/reconstruction slices (5–7) will call; the test seeds with it.
- **`crates/custodian/src/reconciliation.rs`** — `reconcile_step` is now `async`, takes
  `gc: Option<&GcContext>` + `now_millis`, and dispatches to `gc::reconcile` after the
  fence check (`reconciliation.rs:60-66`). New `ReconcileError { Fenced, Store }` wraps
  the fence error and store faults.
- **`crates/custodian/src/lib.rs`** — export `gc`, `mark_orphaned`, `GcContext`,
  `ReconcileError`.
- **`crates/custodian/Cargo.toml`** — `async-trait` + `bytes` as dev-deps (already
  workspace crates; for the in-memory test stores). No new external dependency, so no
  `deny.toml` / cargo-deny audit triggered (posture (d)).
- **`crates/custodian/tests/skeleton.rs`** — the one existing caller of `reconcile_step`
  updated to the new signature (`None, 0` — the bare fence path). Still green.
- **`crates/custodian/tests/gc.rs` (new)** — the four-leg test.

## Why this shape (alternatives ruled out)

- **Why extend `reconcile_step` rather than add a `gc_reconcile_step`.** The brief's
  anti-#141 guard is binding: GC must be *the code `reconcile_step` dispatches to*, never
  a parallel test-only entry. A sibling function would be exactly the forbidden parallel
  entry. Cost of the chosen path: one signature change rippling to its **only** caller
  (skeleton.rs leg 1, a 2-line edit) — `rg "wyrd_custodian" crates --glob '!custodian'`
  confirms no external user. So the blast radius is 1 test file; a parallel entry would
  have left the real control point returning `Satisfied` forever (the #141 defect).
- **Why `Option<&GcContext>` not a mandatory context.** The fence scaffold (skeleton leg
  1) tests *only* the fence; `None` keeps that honest without forcing it to construct
  stores. Production always passes `Some`. When `Some`, dispatch is unconditional — GC
  is not bypassable on the real path.
- **Why an `orphan:` ledger for the grace window, not a magic constant in GC.** The
  invariant "readers are never torn" requires a per-orphan eligibility instant; the
  window length is explicitly out of scope (`0005:585-586`). Modeling the orphaning
  instant as a ledger record (the architecture-§5 pending-ledger sweep pattern the brief
  cites) lets GC own a *flippable* window gate (`now >= orphaned_at + grace`) while the
  *length* stays a caller-supplied, derived `grace_window_millis`. The reconstruction
  slice (6) will write these records at its commit point; this slice ships the read side
  + the `mark_orphaned` write seam. Rejected alternative: deriving "orphaned-at" from
  inode version/tombstones — there is no such record on base (a delete removes the
  dirent/inode with no timestamp), so it cannot be reader-safe.
- **Why GC leaves `core::sweep_expired_leases` untouched.** It has live callers
  (`server/tests/*`, `dst/tests/network.rs`); the brief says *extend* reclamation to the
  bytes, not rewrite the ledger sweep. GC reads `pending:` directly and removes the entry
  alongside the bytes, subsuming the sweep for the custodian path without disturbing the
  existing one.
- **Boundary (criterion 5 / ADR-0010).** `gc.rs` imports only `wyrd_core::metadata`,
  `wyrd_traits`, and `tracing`. No concrete backend. Verified by clippy + the dep list.

## Telemetry (criterion 4)

GC emits via `tracing` `monotonic_counter.gc_fragments_{reclaimed,skipped}` events
(bridged to OTel by `DurabilityTelemetry::metrics_layer`, exactly as the skeleton's
first metric) plus separate append-only audit events (`target:
"wyrd.custodian.gc.audit"`). The test wires the seam with `.with_subscriber(...)` across
the async step and reads the counters back via `gather_prometheus` in-process. A live
Prometheus/OTLP scrape is supplementary off-Check evidence (posture (b)).

## Red→green evidence

- Full custodian suite green (`cargo test -p wyrd-custodian`: 4 gc + 3 skeleton).
- **Flippable leg 2** (never-reclaim-referenced): negating
  `if referenced.contains(...)` → `!referenced.contains(...)` makes
  `never_reclaims_a_referenced_fragment` FAIL (a referenced fragment gets deleted).
  Restored → green.
- **Flippable leg 3** (grace window): flipping `now >= orphaned_at + grace` to `<` makes
  `honours_the_reader_safe_grace_window` FAIL (within-grace orphan reclaimed early).
  Restored → green.
- **Net-new legs 1 & 4** (posture (b), criterion-absence red): the GC loop, `GcContext`,
  `mark_orphaned`, and the GC dispatch in `reconcile_step` do not exist on base, so the
  test does not compile/pass against base — legitimate born-at-tier red.
- Whole workspace (excl. `wyrd-dst`) green; `cargo fmt --all --check` clean; `cargo
  clippy -p wyrd-custodian --all-targets -D warnings` clean (commit-ready for the
  target's hooks).

## Deferred (declared, per brief Scope / posture (c))

No deployed custodian process drives the loop (Option A) — green is the reconciler
exercised in-process through the real `reconcile_step`. Scrub/reconstruction/rebalance
(slices 5–7), the exact grace-window length, and the DST campaign (slice 8) are out of
scope. The reconstruction slice will call `mark_orphaned` at its commit point.
