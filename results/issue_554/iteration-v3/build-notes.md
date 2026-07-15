# Build notes — #554 deployed-custodian-runs-gc (iteration 3)

Target branch: `getwyrd/wyrd @ main`, base `dc503cd`. All `path:line` below are on the
patched worktree (the diff's `b/` side) unless stated as base.

## What this iteration changes vs iteration-2 (and why)

Iteration-2 wired the GC pass into `run_reconstruction_until` and gated it on
`unreachable.is_empty()`. It was **rejected for a NEW adversary finding**: that gate is
defeated at STARTUP. `connect_fleet` starts DEGRADED — a D-server unreachable when the
custodian boots is silently dropped (`custodian.rs` `connect_fleet`, base `custodian.rs:150-179`;
`cli.rs` assembles the fleet through it, base `cli.rs:868-896`). So the `configured` slice
the loop receives is **already shorter** than the operator-wired fleet, and
`live_reconstruction_view(configured)` returns an EMPTY `unreachable` (it only probes the
servers that *did* connect). The first GC pass then passes `unreachable.is_empty()` and can
retire CHUNK-WIDE `pending:` evidence (`gc.rs:155-167`) for a chunk a never-connected server
still holds a fragment of — a permanent, silent leak, exactly the stranding the GC deferral
exists to prevent.

**Fix (the invariant to restore): GC runs only when the loop can see the WHOLE
operator-configured fleet this pass.** I threaded the operator endpoint count into the run
loop and changed the gate:

- `run_reconstruction_until` gains an `operator_fleet_size: usize` parameter
  (`custodian.rs:454`). This mirrors the #551 restore pass, which refuses a partial fleet by
  comparing the connected count against the operator endpoint count (`cli.rs:961-975`).
- The GC gate is now `if fleet.len() == operator_fleet_size` (`custodian.rs:582`), NOT
  `unreachable.is_empty()`. Since `fleet ⊆ configured` and `configured.len() ≤ operator_fleet_size`,
  the equality holds IFF every operator endpoint is both connected AND reachable this pass —
  closing BOTH the startup-omitted hole and the runtime-unreachable hole with one check
  (`custodian.rs:540-580` doc).
- `cmd_custodian` passes `endpoints.len()` (the operator count, NOT `configured.len()`)
  through `run_reconstruction_over_backend` (new `operator_fleet_size` param, `cli.rs:1118`)
  to the loop (`cli.rs:1050`).

The GC pass itself, the grace-window derivation (`GC_GRACE_WINDOW_MILLIS = cli::LEASE_TTL_MILLIS`,
`custodian.rs:110`), the distinct-fenced-pass placement, and the ordered-last-after-reconstruction
rationale are carried unchanged from iteration-2 — those were **accepted** at the iteration-2
sign-off ("do NOT rework"). Only the fleet-completeness gate changed.

### Other iteration-2 carry-forward items addressed
- **Pause-under-outage trade-off stated in the run-loop doc** (`custodian.rs:419-438` doc
  block): any single unreachable OR decommissioned-but-still-configured D-server pauses ALL
  reclamation, fleet-wide, until the fleet is whole. This is deliberate (a false "collected"
  is a permanent leak; a paused reclaim is fully recovered next whole-fleet pass) but it is a
  **maintainer-visible trade-off** — see §6 item below.
- **Exact grace boundary pinned** — new test `deployed_role_reclaims_at_the_exact_grace_boundary`
  drives the clock at PRECISELY `orphaned_at + grace` and asserts reclaim (gc.rs:136 is `>=`,
  inclusive). The ±1 ms probes left the boundary itself unpinned.
- **Full `cargo xtask ci` run on a loopback-permitting host** — ran green end to end
  (`./engine/xtask.sh ci` in `$PDCA_WORKTREE`): "xtask ci: all checks passed", including the
  real-loopback `closed_write_path` and `network` suites. The green is non-provisional.

## Signature change → red shape is honest (per the brief's Test-file note)

The brief preferred wiring GC WITHOUT changing `run_reconstruction_until`'s signature so the
new test would fail by ASSERTION on the reverted base. That is **not possible for the
iteration-2 fix**: closing the startup-partial hole *requires* the loop to know the operator
fleet size, which is not recoverable from the degraded `configured` slice — the iteration-2
carry-forward explicitly directs threading the operator count into the entry. So the entry
signature changed, and the new test file's red leg on a **fully-reverted** base is a COMPILE
ERROR (E0061 "too many arguments"), not an assertion. I verified this (see refutation (a)).

To keep the behavioural binding honest anyway, the startup-partial test is *also* refuted by
reverting ONLY the gate line (`fleet.len() == operator_fleet_size` → `unreachable.is_empty()`):
it then fails by ASSERTION at `custodian_gc.rs:774` (see refutation (c)). So the test binds the
actual fleet-completeness fix, not merely the presence of the new parameter.

Signature change also required updating the pre-existing callers (each is its own test binary,
but the whole suite must stay green): `custodian_day_one.rs` (4 `run_reconstruction_until`
calls + 2 `run_reconstruction_over_backend` calls) and `closed_write_path.rs` (1). The
`configured([...])`-built fleets pass `servers.len()`; the start-degraded day-one test passes
`endpoints.len()` (4) though `configured` holds 3 — which now correctly makes that test's GC
DEFER (a live demonstration of the fix on the existing harness).

## Refutation (recorded, per the Do beat)

- **(a) Genuine red?** YES. Reverted the source (`custodian.rs` + `cli.rs`) to base via
  `git stash`, kept the test file, ran `cargo test --test custodian_gc`: it fails to compile
  (E0061 / E0277 — the base 7-arg `run_reconstruction_until`). Red. Restored the fix → green.
  This is the compile-error red the brief anticipates for a required signature change (flagged
  above and in the test file header).
- **(b) Production path?** YES. Every test drives the REAL production entry
  `CustodianService::run_reconstruction_until` (the exact wiring `cli::cmd_custodian` →
  `run_reconstruction_over_backend` → `run_reconstruction_until` runs), over in-memory
  `MetadataStore` / `ChunkStore` trait fleets + a logical clock — the same harness shape
  `custodian_day_one.rs` uses. No mock/copy/re-implementation of the loop; the GC library
  (`wyrd_custodian::gc`) and the delete-orphaning path (`metadata::unlink`) are the real ones.
- **(c) Fixture includes the fault?** YES. `deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial`
  builds the fault in: the doomed chunk's fan-out is on servers 0,1,2; the operator wired 3
  endpoints; server 2 (holding fragment index 2) is the startup-omitted one, so `configured`
  holds only servers 0,1 while `operator_fleet_size == 3`. The failing element (the missing
  server that holds a real fragment under a chunk-wide pending lease) is IN the fixture, not
  curated out. Reverting only the gate to `unreachable.is_empty()` makes this test go red by
  assertion (verified) — proving the fixture exercises the exact leak.

## Pre-declared sign-off items (SUMMARY §6 — the maintainer's calls, not settled here)

1. **Grace VALUE (T5).** The deployed grace window is `GC_GRACE_WINDOW_MILLIS =
   cli::LEASE_TTL_MILLIS` (60 s), a conservative FLOOR reused from the shipped restore-pass
   precedent — **not a proven reader-safety bound**. The checkout has no reader version-hold /
   maximum-read-duration mechanism, so no derivation can prove a value reader-safe (proposal
   0005:585-586 calls the exact value "a measurement question"). Building that bound is a
   separate work item; this bundle ships the MECHANISM (never reclaim before the recorded
   deadline elapses, reclaim after) and an honest floor. The value is the maintainer's call.
2. **Pause-under-outage trade-off (item 7).** The whole-fleet GC gate pauses ALL reclamation
   fleet-wide while ANY single configured D-server is unreachable OR decommissioned-but-still
   -listed in `--endpoints`, indefinitely. Deliberate (byte-safe: a false "collected" is a
   permanent leak, a paused reclaim recovers), but a maintainer should confirm this is the
   desired operational posture, and that decommissioned servers are removed from `--endpoints`
   promptly. Relaxing it to reclaim orphans over the reachable subset needs GC to preserve
   pending evidence per-server (the #490 lease-liveness work), out of scope here.
3. **Lease-liveness hazard (adversaries 1 & 2) — accepted document-and-ship at iteration-2,
   carried forward.** The deployed collector's expired-pending input treats the bare lease TTL
   as grace; a lease that only *looks* expired against the custodian's clock (a born-at-logical
   -zero CLI lease, a slow >30 s gateway PUT) is a mid-flight-write hazard on a SHARED
   write-taking backend. This is NOT closed here. #490's lease-conditional commit (obligation d)
   fail-closes both scenarios (refused write, never a torn committed object); the residual
   mid-pass race is tracked as #557. **PR sequencing note (must land in the PR description): do
   not run `wyrd custodian` against a shared write-taking backend before #490 merges.** Do NOT
   build any of the out-of-scope lease mechanisms in this bundle.
4. **Contribution/grace-input overlap (T4).** Whether the reachable-fleet-only reclaim posture
   and the pending-input grace are ultimately correct is entangled with the #490 re-plan; the
   whole-fleet gate here is the conservative interim that never strands bytes.

## External dependencies

None beyond the base toolchain. The deployed-role tests run in-process over trait stores + a
logical clock (no Docker, no live backend), as `custodian_day_one.rs` does. `cargo xtask ci`
ran fully green on this host (loopback sockets available); no NEEDS-HUMAN external dependency.
