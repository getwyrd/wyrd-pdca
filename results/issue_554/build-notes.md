# Build notes — #554 deployed-custodian-runs-gc (iteration 5)

Target branch: `getwyrd/wyrd @ main` (base `dc503cd`). All `path:line` below are on the patched
worktree (`$PDCA_WORKTREE = /home/eddie/development/wyrd/wyrd.pdca-wt-l0`, the diff's `b/` side)
unless stated as base.

## TL;DR of this iteration

Iteration-4 sign-off **rejected on two confirmed findings, NOT on the approach** — the GC wiring,
the `operator_fleet_size` whole-fleet gate, and the red→green evidence were told to STAND (do not
rework). So this iteration reproduces the accepted iteration-4 patch unchanged and does exactly the
two mechanical things the rejection asked for, plus pins the first with a test:

1. **Duplicate `--endpoints` / `--ids` reached the deleting GC sweep unvalidated** (adversary
   [impl], confirmed). The uniqueness refusals existed ONLY inside the `--reconcile-after-restore`
   block; the run-loop path this bundle arms with a *deleting* GC pass performed none. **Fix:
   hoist the two refusals so they guard EVERY path**, and add a test that drives the run-loop entry
   with a duplicate and asserts refusal.
2. **Stale grace rationale** (reviewer C3 FAIL, an iteration-1 obligation still open). The
   `RESTORE_GRACE_WINDOW_MILLIS` doc-comment still claimed "the one timescale the system already
   trusts" (there are two: 60 s CLI, 30 s gateway) and still deferred the derivation to "#554's
   job" — false once this patch lands. **Fix: correct that doc-comment.**

The C4 environment evidence is carried forward (below): the gating `C4-ci` exit-101 is a gate-host
`/tmp`/quota fault (`os error 122`, EDQUOT), independently reproduced at the target by the
iteration-4 adversary and by issue-430's sign-off host — not a patch defect.

## Finding 1 — hoist the fleet-identity refusal to guard the run-loop (GC) path

**The confirmed hole.** On base + the iteration-4 patch, the uniqueness refusals lived inside
`if reconcile_after_restore { … }` (base `cli.rs:940-967`). The DEPLOYED run-loop path
(`cli.rs:1037-1055`, the `run_reconstruction_over_backend` call this bundle activates GC on)
performed NO uniqueness check. Two identities for one physical box — a duplicated `--endpoints`, or
two `--ids` naming the same server — fuse it into a phantom fleet: a LIVE fragment protected as
`(A, frag)` is unreferenced when the same bytes are seen as `(B, frag)`, so the newly-armed GC pass
reclaims and DELETES it. The whole-fleet gate does not save us: it counts the duplicate on both
sides, so `fleet.len() == operator_fleet_size` still holds. Live-data-loss route.

**The fix (mechanical, minimal).** Hoist the two existing refusals — endpoint uniqueness and id
uniqueness — to run once, unconditionally, right after `--endpoints` is parsed and BEFORE
`connect_fleet` (`cli.rs:875-911` on the patched side). The `--reconcile-after-restore` block keeps
only its OWN remaining refusal, the whole-fleet-reachable check (`cli.rs:973-992` patched), which
the run loop deliberately does not want (the loop reads around a down box; its GC pass gates on
`operator_fleet_size` instead). Failing fast before any dial is a bonus: a fused fleet is now
rejected without a network round-trip.

**Why hoist rather than duplicate the block into the run-loop path.** Duplicating would re-add
~28 lines of the same two `HashSet` refusals in a second place and leave the two copies free to
drift (the exact drift that caused this bug — a check that lived in one branch only). Hoisting is a
net *reduction* in the restore block (its two refusals collapse to a 4-line comment) and puts the
invariant "fleet identities are unique" where it belongs: once, on every path. This is the smallest
change that restores the invariant (`docs/principles.md` §1.2), not merely the smallest diff.

**The test.** `deployed_run_loop_refuses_duplicate_endpoints` and
`deployed_run_loop_refuses_duplicate_ids` (`crates/server/tests/custodian_gc.rs:866,901`) drive the
REAL production entry `cli::cmd_custodian` on the run-loop path (**no** `--reconcile-after-restore`)
with a duplicate and assert it returns `Err` naming the duplicate — refusing BEFORE it dials. Plain
`#[test]` (not `#[tokio::test]`): `cmd_custodian` builds its own multi-thread runtime and
`block_on`s it, which panics if called from within an ambient tokio runtime.

## Finding 2 — correct the grace-window rationale

`RESTORE_GRACE_WINDOW_MILLIS`'s doc-comment (`cli.rs:78-96` patched, const at `:97`) now:
- names **both** trusted pending-lease timescales (CLI 60 s `LEASE_TTL_MILLIS`; gateway 30 s
  `DEFAULT_LEASE_TTL_MILLIS`, `lib.rs:49`) and states it reuses the **longer**;
- calls the value a conservative **FLOOR, not a proven reader-safety bound** (no reader
  version-hold mechanism exists to prove one — `0005:585-586` "a measurement question");
- **removes** the false "#554's job" deferral and instead points at the deployed GC pass that now
  shares the derivation (`custodian::GC_GRACE_WINDOW_MILLIS`).
This matches the two-timescale derivation the iteration-4 `custodian.rs` doc already carried.

## What this patch does (the accepted iteration-2/3/4 design, carried UNCHANGED)

Wires a **distinct, fenced GC pass** into the deployed run loop, gated on seeing the whole
operator-configured fleet:

- `run_reconstruction_until` gains `operator_fleet_size: usize` (`custodian.rs:454` patched),
  threaded from `cmd_custodian` (`endpoints.len()`, `cli.rs:1060` patched) through
  `run_reconstruction_over_backend`, mirroring how the #551 restore pass reads the operator
  endpoint count.
- After scrub + reconstruction, a third `reconcile_pass` runs GC **iff**
  `fleet.len() == operator_fleet_size` (`custodian.rs` GC-pass block), else it **defers and
  preserves all evidence**. Gating on the operator count (not `unreachable.is_empty()`) closes both
  the runtime-unreachable and the startup-partial hole (iteration-2 correction).
- Grace window derived, never a magic constant: `GC_GRACE_WINDOW_MILLIS = cli::LEASE_TTL_MILLIS`
  (`custodian.rs:110` patched) — the same derivation the shipped restore pass uses; doc-comment
  names both lease timescales and marks it a floor (iteration-4 correction, now mirrored in
  cli.rs).
- Distinct pass, not folded into scrub/reconstruction (fault isolation, Codex #461); ordered last,
  after reconstruction commits placement rewrites, so GC cannot race or reclaim a just-re-placed
  fragment (`gc::referenced_fragments` never reclaims a referenced fragment).
- Pause-under-outage trade-off stated in the run-loop doc.

## Red→green proof (refutation, recorded per the Do beat)

- **(a) Genuine red? YES.** For the two NEW refusal tests: I reverted ONLY the hoisted refusal
  block (neutralized it to a comment, simulating the base run-loop path) and reran — BOTH tests
  went **red**: `cmd_custodian` reached `connect_fleet`, failed to dial the unreachable fused
  fleet, and panicked on the empty fleet instead of refusing the duplicate up front (captured
  panic: "cmd_custodian did NOT refuse duplicate --endpoints before dialing (RED on base)"). Fix
  restored → both **green**. For the GC-wiring tests: the behavioural red carried from iteration-4
  stands (reverting the gate `fleet.len() == operator_fleet_size` → `unreachable.is_empty()` fails
  the startup-partial test; a full revert of `custodian.rs`+`cli.rs` keeping the test is an E0061
  compile-error red, per the brief's Test-file note). C4-verify's own red leg reverts the
  production and keeps the test file.
- **(b) Production path? YES.** The refusal tests drive the REAL `cli::cmd_custodian` entry — the
  exact `wyrd custodian` command wiring — not a copy of the check. The GC-wiring tests drive the
  real `CustodianService::run_reconstruction_until` over in-memory `MetadataStore`/`ChunkStore`
  fleets + a logical clock, with the real `wyrd_custodian::gc` and `metadata::unlink` — no mock of
  the loop.
- **(c) Fixture includes the fault? YES.** The refusal fixtures contain the fused fleet itself (a
  literally-duplicated `--endpoints`, and two `--ids` = `7,7`), and assert refusal against THAT.
  The GC fixtures put→delete→advance-past-grace and assert the orphaned fragments are physically
  gone; the startup-partial fixture builds a real missing server holding a fragment under a
  chunk-wide pending lease; the skipped-server fixture keeps a killed node in the fleet across an
  outage. The failing element is IN each fixture, never curated out.

Full local evidence on this host (all green):

| Check | Command | Result |
|---|---|---|
| custodian_gc (8 tests, incl. 2 new refusal) | `cargo test -p wyrd-server --test custodian_gc` | 8 passed |
| Refusal tests RED with hoist reverted | (neutralize hoist) same cmd, filter `refuses` | 2 failed (RED) → restored 2 passed |
| Sibling: closed_write_path | `cargo test -p wyrd-server --test closed_write_path` | 1 passed |
| Sibling: custodian_day_one | `cargo test -p wyrd-server --test custodian_day_one` | 15 passed |
| Server lib unit tests (cli edits) | `cargo test -p wyrd-server --lib` | 28 passed |
| rustfmt | `cargo fmt -p wyrd-server -- --check` | EXIT 0 |
| clippy `-D warnings` | `cargo clippy -p wyrd-server --tests -- -D warnings` | EXIT 0 |

## Pre-declared sign-off items (SUMMARY §6 — maintainer's calls, carried forward)

1. **Grace VALUE (T5).** `GC_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS` (60 s) is a conservative FLOOR
   reused from the shipped restore-pass precedent — **not a proven reader-safety bound**. The
   checkout has no reader version-hold / max-read-duration mechanism, so no derivation can prove a
   value reader-safe (`0005:585-586` calls the exact value "a measurement question"). This bundle
   ships the MECHANISM (never reclaim before the recorded deadline; reclaim after) + the honest
   floor. The value is the maintainer's call.
2. **Pause-under-outage trade-off (item 7).** The whole-fleet GC gate pauses ALL reclamation
   fleet-wide while ANY single configured D-server is unreachable OR decommissioned-but-still in
   `--endpoints`, indefinitely. Deliberate and byte-safe (a false "collected" is a permanent leak;
   a paused reclaim recovers), but the maintainer should confirm this posture and that
   decommissioned servers are promptly dropped from `--endpoints`.
3. **Lease-liveness hazard (adversaries 1 & 2) — accepted document-and-ship (do NOT rework, per
   iteration-2 sign-off).** The deployed collector's expired-pending input treats the bare lease
   TTL as grace; a lease that only *looks* expired against the custodian's clock (a
   born-at-logical-zero CLI lease, a slow >30 s gateway PUT) is a mid-flight-write hazard on a
   **shared write-taking backend**. NOT closed here. #490's lease-conditional commit fail-closes
   both (refused write, never a torn committed object); residual mid-pass race tracked as **#557**.
   **PR sequencing note (must land in the PR description): do not run `wyrd custodian` against a
   shared write-taking backend before #490 merges.** No out-of-scope lease mechanism is built here;
   the `cli.rs` NOW_MILLIS caveat comment surfaces it.
4. **Contribution/grace-input overlap (T4).** Whether the reachable-fleet-only reclaim posture and
   the pending-input grace are ultimately correct is entangled with the #490 re-plan; the
   whole-fleet gate here is the conservative interim that never strands bytes.

## NEEDS-HUMAN — the C4 gate host is out of disk quota (environment, not the patch)

The gating whole-workspace `C4-ci` (`cargo xtask ci` → `cargo test --workspace --exclude wyrd-dst`)
fails independently of this patch at `crates/server/tests/dst_commit.rs` with
`Os { code: 122, kind: QuotaExceeded }` (EDQUOT) on a redb/DST **disk write** — a per-user quota
exhausted by accumulated `../wyrd-*/target` dirs and lane worktrees (filesystem free space is
ample; the quota is not). Those DST commit-atomicity tests never touch the custodian run loop or
GC. Independently reproduced at the target by the iteration-4 adversary (full gate exit 0 when the
quota had headroom) and by issue-430's sign-off host. The patch's own suite, fmt, and clippy are
green (table above).

NEEDS-HUMAN external dependency: disk-quota-headroom — the C4 gate fails at `dst_commit.rs` with
`Os { code: 122, QuotaExceeded }`; the user's disk quota is exhausted by accumulated
`../wyrd-*/target` dirs and lane worktrees (> 200 G), so redb/DST tests cannot write. Not a patch
defect. A human must prune stale worktrees/`target` dirs (or raise the quota) and re-run the gate.

```toml
[[doctor.checks]]
id    = "disk-quota-headroom"
cmd   = "f=$(mktemp -p \"${PDCA_WORKTREE:-.}\") && dd if=/dev/zero of=\"$f\" bs=1M count=200 2>/dev/null; rc=$?; rm -f \"$f\"; exit $rc"
hint  = "cargo xtask ci's redb/DST tests hit EDQUOT (per-user disk quota, errno 122); filesystem free space is ample but the user's quota is exhausted by ../wyrd-*/target dirs and lane worktrees. Prune stale worktrees/target dirs or raise the quota."
level = "MISSING"
```

## External dependencies

None for the fix itself: the deployed-role tests run in-process over trait stores + a logical clock
(no Docker, no live backend), as `custodian_day_one.rs` does. The refusal tests dial nothing on the
green path (they refuse before `connect_fleet`). The one environment blocker is the host disk-quota
exhaustion above (NEEDS-HUMAN), which affects the whole-workspace gate, not the contribution.
