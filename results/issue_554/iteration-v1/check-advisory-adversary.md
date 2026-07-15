# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc

## Evidence attack (re-run at $PDCA_TARGET)

- Green leg re-verified: `cargo test -p wyrd-server --test custodian_gc` → 2/2 pass in 0.11s at
  the target. Red-leg mechanism verified structurally read-only: `origin/main`'s
  `run_reconstruction_until` signature is byte-identical to the patched one
  (`crates/server/src/custodian.rs:411-424` vs `origin/main:crates/server/src/custodian.rs:390`),
  and the test references no symbol the revert removes (`GRACE_MILLIS` is its own constant,
  `crates/server/tests/custodian_gc.rs:404`), so the reverted base compiles and fails by
  assertion — the honest red the brief demanded. The test drives the real production entry
  (`run_reconstruction_until`, custodian.rs:411), not a re-implementation; on the fixed tree only
  the GC pass can delete the doomed fragments (the object is unlinked, so scrub/reconstruction
  have no obligations over it) — no wrong-reason green found. **Attempted to refute the
  red→green proof and the production-path claim; could not.**

## Refutations of the fix

- NEEDS-HUMAN — **This patch activates a reclaim of in-flight CLI writes on the shared
  production backends: the CLI's pending lease is born expired in the collector's clock
  domain.** `cmd_put` stamps leases with logical time zero (`NOW_MILLIS = 0`,
  `crates/server/src/cli.rs:67`, used at cli.rs:493-494 and `cluster_store_put`
  cli.rs:1555-1556), so every CLI put's pending entry carries `lease_expiry_millis = 60_000`.
  The deployed custodian's clock is wall time (`clock = wall_clock_millis`, cli.rs:839,
  cli.rs:1202), so GC's expired-lease input matches it instantly
  (`entry.lease_expiry_millis <= now_millis`, `crates/custodian/src/gc.rs:254`). Concrete
  failing case: `wyrd put --metadata-backend fdb <big-file>` racing a running
  `wyrd custodian --metadata-backend fdb` over the same store (nothing prevents this on
  tikv/fdb — the redb exclusive lock argument at cli.rs:800-802 does not apply, and the
  custodian doc *insists* it open the same store the writers use, cli.rs:699-704): a GC pass
  landing between the put's phase 2 and phase 3 deletes the just-written fragments *and* the
  pending entry (gc.rs:142-157, 165-166); the put's phase-3 commit checks no lease liveness
  (`crates/core/src/write.rs:245-287`) and commits an object whose bytes are all gone — silent
  data loss, created by this wiring, impossible before it because deployed GC never ran. The
  rationale comment "The CLI runs no custodian sweep, so lease expiry is moot"
  (cli.rs:65-66) is falsified by this very patch, which edits the adjacent line (cli.rs:68)
  without reconciling it. Whether the fix is stamping wall clock in the CLI put (touches an
  out-of-scope producer, breaks the stated reproducibility choice), a commit-time lease check,
  or an operator constraint, is a scope/architecture call — not routing to Do unilaterally.

- NEEDS-HUMAN — **The expired-pending-lease input has NO grace window beyond the TTL itself,
  and the gateway's writer never renews — a slow S3 PUT > 30 s is now collectable mid-flight.**
  The production gateway stamps `lease_expiry = now + 30_000`
  (`crates/server/src/lib.rs:49,162`) with no renewal loop; GC reclaims an unreferenced
  fragment the moment the lease expires ("the lease TTL is its grace", gc.rs:142-144), and
  commit does not re-check the lease (write.rs:245-287). With the custodian's default 30 s
  interval (cli.rs:684) a streaming multi-GB PUT whose fan-out outlives 30 s can have its
  fragments reclaimed and still commit. Pre-existing library semantics, but this diff is what
  makes the collector actually run against live gateways — the exact "deployed reality" class
  the brief targets. Fitness/architecture decision (lease renewal, TTL sizing, or commit-time
  lease check), pre-declared adjacent to the grace-value sign-off item.

- NEEDS-HUMAN — **The derivation claim behind the deployed grace value is overstated: there
  are TWO trusted lease timescales, and the patch silently picked the larger.**
  `GC_GRACE_WINDOW_MILLIS = crate::cli::LEASE_TTL_MILLIS` (60 s, custodian.rs:101) is
  documented as "the one timescale the system already trusts" (custodian.rs:94-96), but the
  production gateway's pending-lease TTL is 30 s (`DEFAULT_LEASE_TTL_MILLIS`, lib.rs:49).
  Picking 60 s is conservative in the safe direction for orphans, but the doc-comment's
  justification is factually wrong as stated, and the brief pre-declares the deployed VALUE as
  the maintainer's call (proposal 0005:585-586). Surface at sign-off regardless of what
  build-notes says (not visible to this reviewer).

- NEEDS-HUMAN [impl] — **Neither of the brief's two secondary GC obligations is pinned by the
  new deployed-role test.** (a) The expired-pending-lease input (brief: "expired pending
  leases are likewise GC's input") is never exercised through `run_reconstruction_until` —
  both tests reclaim only delete-orphans (custodian_gc.rs:467-521, 526-553). (b) The FLEET-VIEW
  requirement — "the pass MUST preserve orphan records for servers it skipped" — holds by
  construction in the library (`cleanup` only deletes keys for fragments actually reclaimed,
  gc.rs:151-158), but no test drives the deployed loop with an unreachable server and asserts
  its orphan record survives for a later pass. Both are additive test cases the builder can
  write against the existing harness (drop one `ConfiguredDServer`, or leave a pending entry
  unexpired/expired); a regression in either currently passes green.

## Attempted and could not refute

- GC-races-reconstruction claim (custodian.rs:506-513): passes are sequential, GC recomputes
  `referenced_fragments` after reconstruction's commit, and a just-orphaned displaced fragment
  carries a fresh `orphaned_at` inside the grace window — could not construct a reclaim of a
  re-placed or displaced-but-in-grace fragment.
- Fault-isolation claim: GC runs as a third distinct fenced pass ordered last
  (custodian.rs:523-537); a `ReconcileError::Store` in GC cannot suppress scrub or
  reconstruction (they already ran), and `Fenced` still stops the loop — matches the #461 rule.
- Test-constant drift (`GRACE_MILLIS` duplicating the pub(crate) TTL, custodian_gc.rs:404):
  drift in either direction breaks a test loudly, not silently — no refutation.
- Grace boundary: gc.rs:136 reclaims at exactly `orphaned_at + grace`; the tests pin ±1 ms
  around the deadline, which adequately pins the no-reclaim-before / reclaim-after mechanism
  the brief scoped.
