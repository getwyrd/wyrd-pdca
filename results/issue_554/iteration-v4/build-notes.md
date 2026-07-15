# Build notes — #554 deployed-custodian-runs-gc (iteration 4)

Target branch: `getwyrd/wyrd @ main`. All `path:line` below are on the patched worktree
(`$PDCA_WORKTREE = /home/eddie/development/wyrd/wyrd.pdca-wt-l1`, the diff's `b/` side)
unless stated as base.

## TL;DR of this iteration

The iteration-3 sign-off was **rejected on process evidence, not on the approach**:
1. the advisory reviewer produced no verdict, and
2. the gating C4 check failed (`cargo test --workspace --exclude wyrd-dst`, exit 101) while
   iteration-3's build-notes claimed a fully green `cargo xtask ci`.
The fleet-completeness gate design (`operator_fleet_size`) was explicitly **not** rejected —
"do not discard it blind." So this iteration **keeps that design unchanged** and does the two
things the rejection asked for that are the builder's to do: **reproduce the accepted patch**
and **diagnose the C4 exit-101 vs builder-green discrepancy** (name the failing test; check the
gate-host environment). (Re-running the Check *reviewer* to produce a verdict is the Check
beat's step, not the builder's — flagged for the driver, see §"Reviewer verdict".)

## The C4 exit-101 diagnosis (the iteration-3 carry-forward's central ask)

**Root cause: host per-user disk-quota exhaustion (`EDQUOT`, errno 122), NOT a patch defect.**

I reproduced the gate's exact failing sub-step on this worktree:

```
cargo test --workspace --exclude wyrd-dst        → WORKSPACE_EXIT=101
```

The only failures are two tests in `crates/server/tests/dst_commit.rs`:

```
test exactly_one_commit_wins_across_seeds ... FAILED
test reader_never_sees_a_hybrid_across_seeds ... FAILED

thread '...' panicked at crates/server/tests/dst_commit.rs:94:57:
called `Result::unwrap()` on an `Err` value:
  Os { code: 122, kind: QuotaExceeded, message: "Disk quota exceeded" }
thread '...' panicked at crates/server/tests/dst_commit.rs:132:54: (same EDQUOT)
```

- `dst_commit.rs:94` / `:132` are `write::write_fragments(&chunks, plan).await.unwrap()` — a
  **disk write** in the commit-atomicity DST property tests. The error is an OS `EDQUOT`, which
  no source change can synthesize; it is the host refusing a write because the **user's disk
  quota is full**.
- The filesystem itself is **not** full: `df` shows `/` at 39 % used, 1.1 T free. The limit is a
  **per-user quota**, exhausted by the accumulated Wyrd worktrees + `target/` dirs
  (`du`: `wyrd.pdca-wt-l1` 29 G, `wyrd.pdca-wt-l0` 18 G, plus `wyrd-443` 20 G, `wyrd-543` 19 G,
  `wyrd-551` 18 G, `wyrd-541/547/548` 12 G each, … > 200 G total).
- These two tests exercise **commit atomicity via DST** — they never touch the custodian run
  loop, `run_reconstruction_until`, or GC. My patch changes only `crates/server/src/{cli.rs,
  custodian.rs}` (the run loop) and the tests that call it. So the failure is **causally
  independent** of this contribution.

This is the "builder-green vs gate-red" discrepancy explained: the gate host's disk quota fills
as worktrees accumulate; a run when the quota had headroom is green, a later run is red at the
first redb-backed DST write — a resource race, not a code regression. It is the exact
"check gate-host environment, e.g. loopback availability" the rejection anticipated (here it is
**disk quota**, not loopback — loopback binding works on this host, verified:
`python3 -c "socket.bind(('127.0.0.1',0))"` → ok).

Because this blocks the whole-workspace gate independently of the patch, it is a **NEEDS-HUMAN
environment declaration** (below), not something to work around by pruning other lanes' worktrees.

### What DID pass (the patch's own evidence, all green on this host)

| Check | Command | Result |
|---|---|---|
| Server crate + all its tests compile | `cargo test -p wyrd-server --no-run` | EXIT 0 |
| New GC test (6 tests) | `cargo test -p wyrd-server --test custodian_gc` | 6 passed |
| Sibling tests I touched | `cargo test -p wyrd-server --test custodian_day_one --test closed_write_path` | 1 + 15 passed |
| rustfmt | `cargo fmt -p wyrd-server -- --check` | EXIT 0 |
| clippy `-D warnings` | `cargo clippy -p wyrd-server --tests -- -D warnings` | EXIT 0, no warnings |

So the patch is compile-clean, lint-clean, format-clean, and its own suite is green. The only
red in the workspace is the disk-quota EDQUOT in unrelated DST tests.

## What this patch does (the accepted design, carried from iteration-2/3)

Wires a **distinct, fenced GC pass** into the deployed run loop, gated on seeing the whole
operator-configured fleet:

- `run_reconstruction_until` gains `operator_fleet_size: usize` (`custodian.rs:454`), threaded
  from `cmd_custodian` (`endpoints.len()`, `cli.rs:1050`) through
  `run_reconstruction_over_backend` (new param, `cli.rs:1118`) — mirroring how the #551 restore
  pass reads the operator endpoint count (`cli.rs:961-975`).
- After the scrub + reconstruction passes, a third `reconcile_pass` runs GC **iff**
  `fleet.len() == operator_fleet_size` (`custodian.rs:582`), else it **defers and preserves all
  evidence** (`custodian.rs:603-615`, the `gc pass deferred` eprintln at `:610`). Gating on the
  operator count (not `unreachable.is_empty()`) closes both the runtime-unreachable and the
  **startup-partial** hole (`connect_fleet` starts degraded, `custodian.rs:150-179`), the
  iteration-2 correction.
- Grace window derived, never a magic constant:
  `GC_GRACE_WINDOW_MILLIS = cli::LEASE_TTL_MILLIS` (`custodian.rs:110`), the same derivation the
  shipped restore pass uses (`RESTORE_GRACE_WINDOW_MILLIS`, `cli.rs:83`). Doc-comment states this
  is a **floor**, not a proven reader-safety bound, and correctly names **both** trusted lease
  timescales (CLI 60 s `LEASE_TTL_MILLIS`; gateway 30 s `DEFAULT_LEASE_TTL_MILLIS`, `lib.rs:49`),
  reusing the longer (adversary-3 correction).
- Distinct pass, not folded into scrub/reconstruction (fault isolation, Codex #461); ordered
  last, after reconstruction commits placement rewrites, so GC cannot race or reclaim a
  just-re-placed fragment (`gc::referenced_fragments` never reclaims a referenced fragment) —
  rationale in the GC-pass body comment (`custodian.rs:575-580`).
- Pause-under-outage trade-off stated in the run-loop doc (`custodian.rs:421-427`): any single
  unreachable/decommissioned-but-configured server pauses ALL reclamation until the fleet is
  whole (deliberate — a false "collected" is a permanent leak; a paused reclaim recovers).

## Red→green proof (refutation, recorded per the Do beat)

- **(a) Genuine red? YES.** With the GC pass disabled in production code
  (`if false && fleet.len() == operator_fleet_size`, then rebuilt), `custodian_gc` goes
  **5 of 6 red** — every reclaim/pending/boundary/defer-then-reclaim assertion fails because no
  bytes are ever reclaimed; only `deployed_role_keeps_orphaned_bytes_within_the_grace_window`
  stays green (correctly — with no GC nothing is reclaimed, which is what "within grace" also
  asserts). Restored → **6 of 6 green**. This is a *behavioural* red that binds the GC wiring.
  (The brief's Test-file note: a *full* revert of `custodian.rs` + `cli.rs` that keeps the test
  is a **compile-error** red, E0061 — the signature gained `operator_fleet_size`, which closing
  the startup-partial hole requires. The behavioural red above is the stronger evidence that the
  test binds the fix, not merely the presence of the new parameter.)
- **(b) Production path? YES.** Every test drives the real
  `CustodianService::run_reconstruction_until` (the exact `cli::cmd_custodian` →
  `run_reconstruction_over_backend` → `run_reconstruction_until` wiring), over in-memory
  `MetadataStore`/`ChunkStore` trait fleets + a logical clock, as `custodian_day_one.rs` does.
  The GC library (`wyrd_custodian::gc`) and the delete-orphaning path (`metadata::unlink`) are
  the real ones — no mock/copy/re-implementation of the loop.
- **(c) Fixture includes the fault? YES.** The reclaim tests put→delete→advance-past-grace and
  assert the orphaned fragments are physically gone. `deployed_role_defers_gc_when_the_operator_
  fleet_is_startup_partial` builds the startup-partial fault in: the doomed chunk's fan-out is on
  servers 0,1,2; the operator wired 3 endpoints; server 2 (holding fragment index 2) is the
  startup-omitted one, so the loop's `configured` holds only 0,1 while `operator_fleet_size == 3`.
  The failing element (the missing server holding a real fragment under a chunk-wide pending
  lease) is IN the fixture. `deployed_role_defers_gc_and_preserves_a_skipped_servers_evidence`
  keeps server 2 unreachable during pass window 1 (evidence preserved), then reachable in
  window 2 (reclaimed) — the killed node is in the fleet, not curated out.

## Pre-declared sign-off items (SUMMARY §6 — maintainer's calls, not settled here)

1. **Grace VALUE (T5).** `GC_GRACE_WINDOW_MILLIS = cli::LEASE_TTL_MILLIS` (60 s) is a
   conservative **floor** reused from the shipped restore-pass precedent — **not a proven
   reader-safety bound**. The checkout has no reader version-hold / max-read-duration mechanism,
   so no derivation can prove a value reader-safe (proposal 0005:585-586 calls the exact value
   "a measurement question"). This bundle ships the **mechanism** (never reclaim before the
   recorded deadline; reclaim after) + an honest floor. The value is the maintainer's call.
2. **Pause-under-outage trade-off (item 7).** The whole-fleet GC gate pauses ALL reclamation
   fleet-wide while ANY single configured D-server is unreachable OR decommissioned-but-still in
   `--endpoints`, indefinitely. Deliberate and byte-safe, but the maintainer should confirm this
   operational posture and that decommissioned servers are promptly removed from `--endpoints`.
3. **Lease-liveness hazard (adversaries 1 & 2) — accepted document-and-ship (do NOT rework, per
   iteration-2 sign-off).** The deployed collector's expired-pending input treats the bare lease
   TTL as grace; a lease that only *looks* expired against the custodian's clock (a
   born-at-logical-zero CLI lease, a slow >30 s gateway PUT) is a mid-flight-write hazard on a
   **shared write-taking backend**. NOT closed here. #490's lease-conditional commit (obligation
   d) fail-closes both (refused write, never a torn committed object); residual mid-pass race is
   tracked as **#557**. **PR sequencing note (must land in the PR description): do not run `wyrd
   custodian` against a shared write-taking backend before #490 merges.** No out-of-scope lease
   mechanism is built here (the cli.rs:65-76 caveat comment now surfaces this).
4. **Contribution/grace-input overlap (T4).** Whether the reachable-fleet-only reclaim posture
   and the pending-input grace are ultimately correct is entangled with the #490 re-plan; the
   whole-fleet gate here is the conservative interim that never strands bytes.

## NEEDS-HUMAN — the C4 gate host is out of disk quota

The gating whole-workspace test cannot go green on this host until the per-user disk quota has
headroom. This is independent of the patch (see the diagnosis above).

NEEDS-HUMAN external dependency: disk-quota-headroom — the C4 gate (`cargo test --workspace
--exclude wyrd-dst`, via `cargo xtask ci`) fails at `dst_commit.rs` with `Os { code: 122,
QuotaExceeded }`; the user's disk quota is exhausted by accumulated `../wyrd-*/target` dirs and
lane worktrees (> 200 G), so redb/DST tests cannot write. Not a patch defect. A human must prune
stale worktrees/`target` dirs (or raise the quota) and re-run the gate; the patch's own suite,
fmt, and clippy are green.

Proposed doctor check so this is caught before a cycle burns on it:

```toml
[[doctor.checks]]
id    = "disk-quota-headroom"   # what Plan/External-dependencies should register
cmd   = "f=$(mktemp -p \"${PDCA_WORKTREE:-.}\") && dd if=/dev/zero of=\"$f\" bs=1M count=200 2>/dev/null; rc=$?; rm -f \"$f\"; exit $rc"   # exits 0 iff a 200 MiB write succeeds (catches EDQUOT even when df shows free space)
hint  = "cargo xtask ci's redb/DST tests hit EDQUOT (per-user disk quota, errno 122); filesystem free space is ample but the user's quota is exhausted by ../wyrd-*/target dirs and lane worktrees. Prune stale worktrees/target dirs or raise the quota."
level = "MISSING"
```

## Reviewer verdict (for the driver, not a builder artifact)

Iteration-3 was also rejected because "the advisory reviewer produced no verdict." Producing that
verdict is the **Check reviewer** leaf's step, not the builder's. The builder deliverables
(patch.diff, the named test red→green, build-notes) are complete; the driver should ensure the
Check reviewer runs so a verdict exists at sign-off.

## External dependencies

None for the fix itself: the deployed-role tests run in-process over trait stores + a logical
clock (no Docker, no live backend), as `custodian_day_one.rs` does. The one environment blocker
is the host disk-quota exhaustion above (NEEDS-HUMAN), which affects the whole-workspace gate,
not the contribution.
