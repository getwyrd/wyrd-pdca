# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc (iteration 2)

Skeptic's pass. Grounded on `$PDCA_TARGET` = `/home/eddie/development/wyrd/wyrd.pdca-wt-l0`
(worktree has the patch applied; base = `dc503cd`).

## Refutations that landed

- NEEDS-HUMAN [impl] — **The whole-fleet GC gate is defeated by the start-degraded fleet
  assembly: a custodian started while one D-server is down runs GC over a partial fleet on
  its very first pass — the exact stranding the deferral exists to prevent.** The gate
  checks `unreachable.is_empty()` (`crates/server/src/custodian.rs:539`), but `unreachable`
  comes from probing only the servers that made it into `configured` — and `connect_fleet`
  **silently omits** any peer unreachable at startup (`custodian.rs:197-204`; production
  call `cli.rs:876-884`, which even prints "N reachable of M configured", `cli.rs:900-904`).
  Concrete failing case: crashed fan-out leaves `pending:CHUNK` with fragments on d0 and d1;
  d1 is down when the custodian (re)starts — the day-one incident `connect_fleet` is
  explicitly built to start through (`cli.rs:872-874`); `configured = {d0,d2,d3}` →
  `unreachable = []` → GC runs, sweeps d0's copy, and **retires the chunk-wide `pending:`
  entry** (`crates/custodian/src/gc.rs:155-157,165-167`); when d1 returns, its copy is
  unreferenced, has no orphan record and no pending entry, so GC conservatively keeps it
  **forever** (`gc.rs:146-148`) — permanently leaked bytes. This falsifies the doc claim "GC
  is DEFERRED whenever any server is unreachable this pass" (`custodian.rs:408-411`): true
  only for mid-run unreachability, not startup unreachability. The #551 restore pass names
  and defuses this exact `connect_fleet` trap by requiring the whole fleet (`cli.rs:906-918`);
  the GC gate does not. The T3 test cannot catch it because it hand-assembles `configured`
  with all four servers and only toggles `health()` (`crates/server/tests/custodian_gc.rs:738-807`)
  — it never exercises the `connect_fleet` boundary the gate's correctness depends on.
  Builder-iterable fix: gate GC on live-fleet == operator-configured fleet (thread the
  endpoint count / a completeness flag into `run_reconstruction_until`), plus a test that
  drives the loop with `configured` shorter than the operator fleet.

- NEEDS-HUMAN — **Carry-forward Adversaries 1 & 2 are documented, not closed — and shipping
  this fix ACTIVATES the hazard they name.** On base, GC never ran, so a born-expired CLI
  lease was latent; with this patch a deployed custodian on a shared backend will actually
  collect it: `wyrd put` stamps `NOW_MILLIS = 0` / `lease_expiry = 60_000` logical
  (`cli.rs:75-76`), the deployed loop's clock is `wall_clock_millis` (`cli.rs:847,1210`),
  `expired_pending_chunks` compares them directly (`gc.rs:254`), and an in-flight put's
  fragments are unreferenced (committed-only reference set, `gc.rs:210-213`) — so the GC
  pass deletes a live write's fan-out mid-flight and retires its lease. Likewise the pending
  input still gets zero grace beyond the bare TTL (`gc.rs:142-144`): a >30 s gateway PUT
  (`DEFAULT_LEASE_TTL_MILLIS`, `lib.rs:49`) is collectable the instant its unrenewed lease
  expires — now every ≤ interval, not never. The iteration-1 sign-off said "the deployed
  collector must not reclaim in-flight CLI writes" and to **stop and route back** if closing
  needed the out-of-scope lease-liveness change; the builder shipped with doc-comments
  (`cli.rs:64-74`, `custodian.rs:105-109`) and a build-notes routing instead. Note a
  scope-compatible mitigation existed and was not taken: lag the `now_millis` handed to the
  GC pass by a fixed headroom (no `gc.rs` change, no signature change) — conservative for
  both inputs and it gives the pending input grace beyond the bare TTL. Whether
  document-and-ship satisfies the carry-forward, versus holding for #490, is the human's call.

- NEEDS-HUMAN — **Fitness trade-off: full deferral means any single-server outage pauses ALL
  reclamation fleet-wide** (`custodian.rs:539,561-570`). During a long outage — precisely when
  reconstruction is orphaning displaced fragments — #554's leak resumes for the duration; a
  decommissioned-but-still-configured server pauses GC indefinitely. The brief's FLEET VIEW
  paragraph steered toward reachable-fleet reclaim with per-server evidence preservation;
  the shipped shape deviates (defensibly: the pending input is chunk-wide and `gc::reconcile`
  is out of scope), and declares it (`custodian.rs:521-524`). Maintainer should confirm
  pause-under-outage is acceptable operationally.

## Minor (not gating anyone's attention)

- The exact grace boundary (`now == orphaned_at + grace` reclaims, `gc.rs:136`) is unpinned:
  the tests probe only ±1 ms around it (`custodian_gc.rs:591,651`). Conformance nit.

## Refutations attempted that did NOT land

- **Red→green honesty**: the new test uses only base-visible symbols (`orphan_key`/
  `pending_key`/`encode`/`PendingEntry`, `crates/core/src/metadata.rs:40,60,267,275`;
  `run_reconstruction_until` signature unchanged) and defines its own `GRACE_MILLIS`
  (`custodian_gc.rs:502`), so the reverted-base red is an honest assertion failure, not a
  compile error; the loop under test is the same entry `cmd_custodian` drives
  (`cli.rs:1121-1141`). Could not refute.
- **GC racing/reclaiming reconstruction's re-placed fragments**: passes are sequential and
  the reference set is rebuilt inside the GC pass (`gc.rs:96-100`); a just-committed
  placement protects its fragments. Could not refute.
- **Mid-pass store fault retiring evidence**: `gc::reconcile` commits its cleanup batch only
  after sweeping the whole fleet (`gc.rs:163-170`); an error before that preserves every
  pending/orphan record (at worst a consumed-orphan metadata record leaks — pre-existing
  library behavior, not this diff). Could not refute.
- **Premature reclaim inside the grace window**: pinned by
  `deployed_role_keeps_orphaned_bytes_within_the_grace_window` (`custodian_gc.rs:621-660`).
  Could not refute.
