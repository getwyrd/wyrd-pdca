# Build notes — issue 554 / deployed-custodian-runs-gc

## What the fix does

The deployable custodian role's run loop `run_reconstruction_until`
(`crates/server/src/custodian.rs`) ran scrub + reconstruction every interval but passed
`None` where the `GcContext` goes for BOTH `reconcile_pass` calls, so GC — the only thing
that reclaims fragment bytes — never ran in the deployed role. Every delete/overwrite leaked
its displaced bytes into the ledger forever.

The change adds a **third, distinct fenced `reconcile_pass`** for GC after scrub +
reconstruction, constructing a `GcContext` over the metadata store and the pass's live
(reachable) fleet, with a grace window derived inside the role.

Changes (path:line on the `main` worktree `wyrd.pdca-wt-l1`):

- `crates/server/src/custodian.rs:86-104` — new `GC_GRACE_WINDOW_MILLIS` const, derived
  `= crate::cli::LEASE_TTL_MILLIS`, with a doc-comment stating it is a *floor*, not a proven
  reader-safety bound.
- `crates/server/src/custodian.rs` — the GC pass appended to the run loop after the
  reconstruction pass (was `custodian.rs:470`, the block before `tokio::select!`), building
  `GcContext { meta, fleet: &fleet, grace_window_millis: GC_GRACE_WINDOW_MILLIS }` and calling
  `reconcile_pass(zone, custodian, Some(&gc_ctx), None, None, None, clock())`, with the same
  Fenced→stop / Store→log-and-continue isolation the other two passes use.
- `crates/server/src/custodian.rs:361-` — the method doc updated from "two fenced passes" to
  "three", describing the GC pass.
- `crates/server/src/cli.rs:68` — `LEASE_TTL_MILLIS` made `pub(crate)` so the derivation has a
  single source of truth (the same timescale the shipped `RESTORE_GRACE_WINDOW_MILLIS =
  LEASE_TTL_MILLIS` uses, `cli.rs:68-83`) rather than re-inventing the `60_000` constant.

Composition peers consulted (as the brief permits): the restore pass's `GcContext` assembly
`custodian.rs:339-359` (the wiring shape generalised here) and its grace derivation
`cli.rs:68-83`; the day-one deployed-role test harness `crates/server/tests/custodian_day_one.rs`
(mirrored for the new test's in-memory fleet + logical clock).

## Design decisions the brief called out

- **PASS PLACEMENT — distinct fenced pass, not folded in.** `reconcile_step` runs GC FIRST
  within a combined pass and short-circuits on the first `?` (`reconciliation.rs:77-104`), and
  the run loop classifies a non-fenced fault in the scrub pass as scrub degradation
  (`custodian.rs` scrub match). Folding GC into scrub/reconstruction would mean a GC store
  fault suppresses that pass for the interval and is mislabelled — the exact fault-isolation
  rule that split scrub from reconstruction (Codex #461). GC is its own pass with its own
  `Store→log "gc pass degraded"` / `Fenced→stop` arm.
- **GC must not race the repair loop's placement rewrites.** The three passes run
  **sequentially** (awaited in order) — there is no concurrency within an interval. GC is
  ordered LAST, after reconstruction has committed any placement rewrite, and GC gates every
  reclaim on the freshly-committed reference set (`gc::referenced_fragments`; a referenced
  fragment is NEVER reclaimed, `gc.rs:96-160`). So GC can neither race nor reclaim a fragment
  reconstruction just re-placed.
- **FLEET VIEW — reachable fleet only, orphan marks preserved.** GC sweeps the pass's
  `live_reconstruction_view` fleet (unreachable servers dropped). This is safe for bytes: marks
  are idempotent and persist in the metadata ledger, so a skipped server's orphan records are
  untouched and its garbage is reaped on a later pass. The pass never deletes an orphan record
  for a server it did not sweep (GC only deletes the orphan key on the same pass it reclaims the
  fragment, `gc.rs:154`), so "skipped" is never mistaken for "collected", and a partial
  reachable-fleet sweep is not reported as fleet-wide convergence (the pass reports
  `Changed`/`Satisfied` for what it swept, nothing more).

## Grace window — honest about what is and isn't proven  (PRE-DECLARED SIGN-OFF ITEM)

NEEDS-HUMAN (sign-off decision, not a defect): the **deployed grace VALUE** is the maintainer's
call. The checkout has NO reader version-hold / maximum-read-duration mechanism today, so NO
derivation can currently *prove* a grace value reader-safe — and this bundle does not claim one
does, nor does it build such a mechanism (explicitly out of scope). What ships is:

- (a) the **MECHANISM** — the deployed role honours `grace_window_millis` relative to the
  recorded evidence (`orphaned_at` / lease expiry): never reclaims before it elapses, reclaims
  after. The shipped test pins exactly this (no-reclaim-before / reclaim-after), NOT reader
  safety.
- (b) a **VALUE** derived from the one timescale the system already trusts — the pending-lease
  TTL — exactly as the shipped restore-pass precedent does (`RESTORE_GRACE_WINDOW_MILLIS =
  LEASE_TTL_MILLIS`, `cli.rs:68-83`), documented as a floor.
- (c) this flag: proposal `0005:585-586` calls the exact value "a measurement question", so the
  maintainer should confirm/override `GC_GRACE_WINDOW_MILLIS` at sign-off rather than let it
  pass as settled.

## Test — `crates/server/tests/custodian_gc.rs` (new file, own test binary)

Drives the SAME production wiring `wyrd custodian` runs
(`CustodianService::run_reconstruction_until` → `live_reconstruction_view` + `reconcile_pass`)
over in-memory metadata + trait-store fleets with a logical clock, exactly as
`custodian_day_one.rs` does — no Docker, no live cluster (matches `External dependencies: none`).
Two tests:

- `deployed_role_reclaims_orphaned_bytes_after_grace_elapses` — put two RS(2,1) objects via the
  real write path (`write_new_object_placed`), delete one via `metadata::unlink` (writes orphan
  grace records atomically, `metadata.rs:369-415`), advance the clock past `orphaned_at + grace`,
  drive the deployed loop → asserts the doomed fragments are physically gone from the D-servers
  while the still-live object's referenced fragments survive.
- `deployed_role_keeps_orphaned_bytes_within_the_grace_window` — same setup, clock still INSIDE
  the grace window → asserts nothing is reclaimed (reader-safe no-reclaim-before). Green on base
  and with the fix; pins the mechanism half that does not distinguish.

The test wires GC WITHOUT changing `run_reconstruction_until`'s signature (grace derived inside
the role), so the red leg is a genuine ASSERTION failure on the reverted base, not a compile
error — as the brief's "SHAPE THE RED HONESTLY" instruction requires.

## Refuting my own test (forced check)

- **(a) Genuine red?** YES. C4-verify (`./engine/scripts/run-verify.sh`) reverts the production
  change (`custodian.rs` + `cli.rs`), keeps the test, and the reclaim-after test FAILS by
  assertion at `custodian_gc.rs:378` ("the deployed GC pass reclaimed the orphaned fragment on
  server 0 (RED on base: it never runs GC)"). Verdict: "PASS — red without the fix, green with
  it."
- **(b) Production path?** YES. The test calls `CustodianService::run_reconstruction_until` —
  the exact deployed entry `cli::cmd_custodian` drives via `run_reconstruction_over_backend`
  (`cli.rs:1112-1135`) — not a copy. GC runs through the real `reconcile_step`/`gc::reconcile`
  fenced control point. The orphan marks are the real `metadata::unlink` output, and the reclaim
  is a real `ChunkStore::delete_fragment` on the same store instances the write path populated.
- **(c) Fixture includes the fault?** YES. The doomed object's fragments are actually written to
  the D-servers and actually orphaned (asserted present immediately post-delete), and the fleet
  handed to the role is the real production fleet — the orphaned server is not curated out. The
  test observes the physical reclaim, not a proxy metric.

## Alternatives ruled out

- **Fold GC into the existing scrub or reconstruction pass** (0 new match arms): rejected on the
  fault-isolation cost above — a GC `Store` fault would abort the combined pass on its first `?`
  and be logged as the wrong pass's degradation. The chosen distinct pass adds ~11 lines (one
  `GcContext` + one `match` with two arms), the price of keeping GC's fault isolated (Codex #461).
- **Change `run_reconstruction_until` to take a `grace_window_millis` parameter** and thread it
  from `cmd_custodian`: rejected because it would degrade the test's red leg to a *compile error*
  on the reverted base (the brief warns against this) and touches every caller
  (`run_reconstruction_over_backend`'s three arms, `cli.rs:1112/1122/1131`, plus the day-one
  tests). Deriving the window inside the role — as `RESTORE_GRACE_WINDOW_MILLIS` already does —
  keeps the signature stable and the red honest.

## Gates run

- `cargo test -p wyrd-server --test custodian_gc` — 2 passed (with fix).
- `./engine/scripts/run-verify.sh` (C4-verify) — PASS (green with fix, red without).
- `cargo fmt -p wyrd-server -- --check` — clean.
- `cargo clippy -p wyrd-server --tests` — clean (workspace `-D warnings`).
