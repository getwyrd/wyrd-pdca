# Build notes — issue 554 / deployed-custodian-runs-gc (iteration 2)

## What the fix does

The deployable custodian role's run loop `run_reconstruction_until`
(`crates/server/src/custodian.rs`) ran scrub + reconstruction every interval but passed
`None` where the `GcContext` goes for BOTH `reconcile_pass` calls, so GC — the only thing
that reclaims fragment bytes — never ran in the deployed role. Every delete/overwrite leaked
its displaced bytes into the ledger forever.

The change adds a **third, distinct fenced `reconcile_pass`** for GC after scrub +
reconstruction, constructing a `GcContext` over the metadata store and the pass's fleet, with
a grace window derived inside the role — **gated so it runs only when the FULL configured
fleet is reachable this pass** (the iteration-1 fleet-view correction, below).

### Changes (path:line on the `main` worktree `wyrd.pdca-wt-l0`, base `dc503cd`)

- `crates/server/src/custodian.rs:86-110` — new `GC_GRACE_WINDOW_MILLIS` const, derived
  `= crate::cli::LEASE_TTL_MILLIS`, doc-comment **corrected** to state the system trusts TWO
  lease timescales (60 s CLI, 30 s gateway) and this reuses the LONGER as a conservative floor,
  not a proven reader-safety bound (adversary 3).
- `crates/server/src/custodian.rs:496-566` — the GC pass appended to the run loop after the
  reconstruction pass, **inside `if unreachable.is_empty()`**, building
  `GcContext { meta, fleet: &fleet, grace_window_millis: GC_GRACE_WINDOW_MILLIS }` and calling
  `reconcile_pass(zone, custodian, Some(&gc_ctx), None, None, None, clock())`, with the same
  Fenced→stop / Store→log-and-continue isolation the other two passes use; the `else` branch
  logs a defer.
- `crates/server/src/custodian.rs:388-405` — the method doc updated from "two fenced passes" to
  "up to three", describing the GC pass and the fleet-view defer.
- `crates/server/src/cli.rs:65-76` — `LEASE_TTL_MILLIS` made `pub(crate)` for the derivation,
  and the falsified `NOW_MILLIS`/`LEASE_TTL_MILLIS` rationale comment **reconciled** to record
  that a deployed custodian sharing the backend breaks the "lease expiry is moot" assumption
  (adversary 1's concrete falsified-comment ask).

Composition peers consulted (as the brief permits): the restore pass's `GcContext` assembly
`custodian.rs:reconcile_after_restore_pass` (the wiring shape generalised here) and its grace
derivation `cli.rs:68-83`; the day-one deployed-role test harness
`crates/server/tests/custodian_day_one.rs` (mirrored for the new test's in-memory fleet +
logical clock + reachability toggle).

## How each iteration-1 carry-forward item was addressed

### Fleet-view defect (reviewer C3/C5/T3) — FIXED in code

The v1 patch handed GC the **reachable-only** fleet. GC's expired-pending input retires
**chunk-wide** evidence: when it reclaims any copy of an expired-pending chunk it deletes the
`pending:<chunk>` ledger entry (`gc.rs:155-167`). Over a partial fleet that retires the sole
evidence for a fragment a **skipped** (unreachable) server still holds — stranding it forever
once the server returns. (Orphan records are per-`(dserver, frag)` and safe over a partial
fleet, but the pending record is not.)

**Fix (server-crate only, `gc.rs` untouched as the brief requires):** the run loop runs GC
**only when `unreachable.is_empty()`** — i.e. every configured server passed its reachability
probe this pass. If any server is unreachable, GC is **deferred** (logged) and every
orphan/pending record is preserved untouched for a later whole-fleet pass. "Skipped" is
therefore never mistaken for "collected", and the skipped server's garbage is reaped when the
fleet is whole again.

*Cost of the alternative I rejected here — reachable-fleet GC + preserve pending per-server:*
that requires teaching `gc::reconcile` to retire a `pending:` entry only when it swept **all**
placed copies of the chunk (it currently retires on the first swept copy, `gc.rs:155-167`) — a
change to the GC library loop the brief puts **out of scope** ("any change to the GC library
loop itself … `gc::reconcile` is tested and correct"). The full-fleet gate closes the exact
reviewer defect (no chunk-wide retirement while servers are skipped) in ~10 role-crate lines
without touching the reclaim logic. Its cost is conservatism: GC pauses during any outage. That
is strictly byte-safe (the brief itself says a skipped server's garbage is "reaped on a later
pass") and is the smallest change that restores the invariant "a partial pass never retires
evidence for garbage it did not sweep". Relaxing it to reclaim orphans over the reachable fleet
during an outage is tracked with the lease-liveness work below.

### Adversary 1 & 2 (expired-pending collection can reclaim in-flight writes) — ROUTED BACK

Both are the same class: the deployed GC's **expired-pending input** treats a lease as
collectable garbage the instant `lease_expiry_millis <= now` (`gc.rs:142-144`), but the current
lease mechanism cannot distinguish a genuinely-crashed fan-out from a still-in-flight write:

- **Adv 1:** `cmd_put` stamps leases at logical zero (`NOW_MILLIS = 0` → `lease_expiry = 60_000`,
  `cli.rs:67`), so against a deployed custodian's **wall clock** (`wall_clock_millis`,
  `cli.rs:839,1202`) an in-flight CLI put's lease is born-expired on a shared backend.
- **Adv 2:** the gateway stamps `now + 30_000` (`lib.rs:49`) with no renewal loop, so a
  streaming PUT outliving 30 s has its fan-out collectable mid-flight; commit re-checks no lease
  (`write.rs:245-287`).

Closing either **safely** needs one of: a **commit-time lease-liveness check** (architecture
change in `write.rs`), a **grace window applied to the pending input inside `gc::reconcile`**
(GC-library change), **CLI wall-clock lease stamping** (out-of-scope producer, and it breaks the
stated `wyrd put` reproducibility choice), or **gateway lease renewal** (out-of-scope producer).
Every one is outside this bundle's scope. Per the iteration-1 carry-forward's explicit
instruction — *"if closing adversary 1/2 turns out to require a commit-time lease-liveness check
(an architecture change the brief puts out of scope), stop and route back — that decision
overlaps the #490 re-plan"* — I have **not** built any of them and instead surface the hazard as
a NEEDS-HUMAN sign-off item (below). What this bundle DID do adjacent to it: reconciled the
falsified `cli.rs:65-66` comment (adv 1) and corrected the grace doc-comment's "one timescale"
claim (adv 3), so the code no longer *asserts* a safety it does not have.

Note this hazard rides only GC's **expired-pending** input; the **orphan** input (delete /
overwrite, with an explicit `orphaned_at` written atomically at the committing operation) has no
in-flight-write ambiguity and is fully safe — and it is the orphan input that satisfies the
brief's Success criterion (a deleted object's fragments reclaimed).

### Adversary 3 (grace doc-comment overstated "one timescale") — FIXED

`GC_GRACE_WINDOW_MILLIS`'s doc now states the system trusts two lease timescales (60 s CLI,
30 s gateway) and that this reuses the **longer** as a conservative floor, not a proven bound.

### Adversary 4 (pin both secondary obligations with tests) — DONE

`deployed_role_reclaims_expired_pending_lease_garbage` drives an expired `pending:` lease's
leased bytes to reclamation **through `run_reconstruction_until`** (adv 4a), and
`deployed_role_defers_gc_and_preserves_a_skipped_servers_evidence` drives the loop with one
server unreachable during GC and asserts its orphan record + fragment survive, then are
reclaimed when it returns (adv 4b / the fleet-view property). A regression in either goes red.

## Design decisions the brief called out

- **PASS PLACEMENT — distinct fenced pass, not folded in.** `reconcile_step` runs GC FIRST
  within a combined pass and short-circuits on the first `?` (`reconciliation.rs:77-105`), and
  the run loop classifies a non-fenced fault in the scrub pass as scrub degradation. Folding GC
  into scrub/reconstruction would mean a GC store fault suppresses that pass for the interval and
  is mislabelled — the exact fault-isolation rule that split scrub from reconstruction (Codex
  #461). GC is its own pass with its own `Store→log "gc pass degraded"` / `Fenced→stop` arm.
- **GC must not race the repair loop's placement rewrites.** The three passes run **sequentially**
  (awaited in order) — no concurrency within an interval. GC is ordered LAST, after reconstruction
  committed any placement rewrite, and GC gates every reclaim on the freshly-committed reference
  set (`gc::referenced_fragments`; a referenced fragment is NEVER reclaimed). So GC can neither
  race nor reclaim a fragment reconstruction just re-placed.
- **FLEET VIEW.** See the C3/C5/T3 fix above: reachable-only GC is byte-unsafe for the chunk-wide
  pending input, so the role defers GC entirely while any server is unreachable.

## Grace window — honest about what is and isn't proven (PRE-DECLARED SIGN-OFF ITEM)

The **deployed grace VALUE** is the maintainer's call. The checkout has NO reader version-hold /
maximum-read-duration mechanism, so NO derivation can *prove* a grace value reader-safe — this
bundle does not claim one does, nor builds such a mechanism (out of scope). It ships (a) the
MECHANISM — the role honours `grace_window_millis` for the orphan input (never reclaims before it
elapses, reclaims after), pinned by the two orphan tests; (b) a VALUE = the longer trusted lease
TTL (60 s), documented as a floor; (c) this flag: proposal `0005:585-586` calls the exact value
"a measurement question".

## NEEDS-HUMAN items for SUMMARY §6

NEEDS-HUMAN (sign-off decision): the **deployed grace VALUE** `GC_GRACE_WINDOW_MILLIS = 60_000`
is a maintainer measurement question (proposal 0005:585-586), documented as a floor not a proven
reader-safety bound. Confirm or override at sign-off.

NEEDS-HUMAN (route-back, overlaps #490 re-plan): the deployed GC's **expired-pending collection**
can reclaim an in-flight write whose lease only *appears* expired — CLI leases stamped at logical
zero read as born-expired against the custodian's wall clock (adv 1), and the gateway's 30 s
lease has no renewal so a slow PUT is collectable mid-flight (adv 2). Closing this safely requires
an out-of-scope mechanism (commit-time lease-liveness check / gateway lease renewal / CLI
wall-clock stamping / a grace window on GC's pending input) and a scope call that overlaps the
#490 re-plan. Decide before running `wyrd custodian` against a shared live backend that also takes
writes; this bundle surfaces the hazard (and stops asserting a safety it lacks) but does not close
it. Per the iteration-1 carry-forward, Do did NOT unilaterally build the lease-liveness check.

## Refuting my own test (forced check)

- **(a) Genuine red?** YES. With `crates/server/src/custodian.rs` + `cli.rs` reverted (test kept),
  `cargo test -p wyrd-server --test custodian_gc` fails: `..._reclaims_orphaned_bytes_after_grace`
  (custodian_gc.rs:420), `..._reclaims_expired_pending_lease_garbage` (custodian_gc.rs:547), and
  `..._defers_gc_and_preserves_a_skipped_servers_evidence` (custodian_gc.rs:619, the whole-fleet
  reclaim leg) all panic because the deployed loop never runs GC. `..._keeps_orphaned_bytes_within
  _the_grace_window` stays green both ways (it pins no-reclaim-before). Reverted base still
  COMPILES (the test changes no entry signature and references no symbol the revert removes), so
  the red is an honest ASSERTION failure, not a compile error — as the brief's "SHAPE THE RED
  HONESTLY" requires.
- **(b) Production path?** YES. The tests call `CustodianService::run_reconstruction_until` — the
  exact deployed entry `cli::cmd_custodian` drives via `run_reconstruction_over_backend` — not a
  copy. GC runs through the real `reconcile_step`/`gc::reconcile` fenced control point; the orphan
  marks are the real `metadata::unlink` output; the pending lease is the real `pending:` ledger
  record; the reclaim is a real `ChunkStore::delete_fragment` on the same store instances the write
  path populated.
- **(c) Fixture includes the fault?** YES. The doomed object's fragments are actually written to
  the D-servers and actually orphaned (asserted present immediately post-delete); the leased
  garbage is actually on the stores under a real expired lease; the fleet handed to the role is
  the real production fleet, and in the fleet-view test the killed server is the one that HOLDS the
  skipped orphan fragment (index 2), not curated out — its `health()` genuinely errs so
  `live_reconstruction_view` drops it. The tests observe the physical reclaim / physical survival,
  not a proxy metric.

## Gates run (in `$PDCA_WORKTREE`)

- `cargo test -p wyrd-server --test custodian_gc` — 4 passed (with fix); 3 fail + 1 pass with
  production reverted (test kept) — the honest red→green.
- `cargo test -p wyrd-server --test custodian_day_one` — 15 passed (no regression; the GC gate
  correctly defers on the day-one dead-server tests and degrades-and-continues on the faulty-server
  test).
- `cargo fmt -p wyrd-server -- --check` — clean.
- `cargo clippy -p wyrd-server --tests` — clean (workspace `-D warnings`).
