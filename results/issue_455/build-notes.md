# Build notes — issue 455 / e2e-closed-write-path (iteration 2)

## What the brief asked, and what iteration 1 got wrong

Success criterion: an in-process test drives a **gateway S3 PUT** into a **shared** cluster
metadata store, a **custodian opened over that same store** observes the object as a
**non-zero repair obligation** after a D-server loss (gauge ≥1 → returns to 0), and a GET
round-trips **byte-identical**.

Iteration 1 shipped a test-only patch that **hand-enqueued** the repair obligation
(`repair::enqueue_repair(&meta, chunk_id, "health")`). The reviewer rejected it: the
custodian must *derive* the obligation from the gateway-written placement, not be handed a
queue entry — otherwise `under_replicated == 1.0` can pass even when the gateway→custodian
derivation is broken (the exact "empty store" failure the issue exists to catch). C4-verify
also failed: a test-only patch has no production change to revert, so the "red without the
fix" leg could never go red.

## Root cause (the real production gap — why this is a fix, not just a test)

The deployable custodian role runs **reconstruction only, never scrub**:
`cmd_custodian` (`crates/server/src/cli.rs:606`) → `run_reconstruction_over_backend`
(`cli.rs:758`) → `run_reconstruction_until` (`crates/server/src/custodian.rs:309`), whose
loop built only a `ReconstructionContext` and called
`reconcile_pass(.., None /*scrub*/, Some(&ctx) /*reconstruction*/, ..)`.

Reconstruction only ever **drains** the shared repair queue
(`crates/custodian/src/reconstruction.rs:141`, `let queue = repair::queued_repairs(..)`). It
does not scan placement. The one shipping loop that *derives* obligations from the committed
(gateway-written) placement is **scrub** — it walks every referenced fragment and enqueues
any it finds absent/corrupt (`crates/custodian/src/scrub.rs:64`, the `Ok(None)` arm at
`scrub.rs:138`, issue #330). Because scrub was never wired into the deployable role, a
custodian opened over the store a gateway wrote drained an **empty** queue and computed
**zero** repair work from the placement — the "`--metadata-backend tikv` custodian opens a
store nothing wrote and sees zero repair work" symptom the issue names, in the form that
survives #454. This is the brief's **Invariant to restore**: the write path and the repair
scan must share one placement contract — the placement a gateway PUT records must be
*sufficient for the custodian to compute the obligation*, which requires the custodian to
actually run the scan.

## The fix (smallest change that restores the invariant)

Wire scrub into the deployable reconstruction loop so each pass first **scrubs** the live
fleet (deriving obligations from the gateway-written placement) then **reconstructs** them —
`crates/server/src/custodian.rs:347` builds a `ScrubContext` over the same live fleet and
`custodian.rs:364` passes `Some(&scrub_ctx)` to `reconcile_pass`. `reconcile_step`
(`crates/custodian/src/reconciliation.rs:65`) runs scrub before reconstruction in one pass,
so scrub's enqueue is visible to reconstruction's `queued_repairs` read in the *same* pass.
Doc updated at `custodian.rs:297`.

This is a composition change in the one crate that may know concretes (ADR-0008: "backend
choice becomes a composition concern in `server`, not a refactor") — it composes the
existing shipping `scrub` and `reconstruction` loops, adding no new logic to the custodian
library. Scrub runs over the **live** (reachable) fleet (`live_reconstruction_view`), so a
transient scrub fault degrades the pass (logged-and-continued, `custodian.rs:344`), never the
process — the same survival policy the loop already had.

Diff size: `crates/server/src/custodian.rs` +36/−3 (doc + a `ScrubContext` and one changed
arg); the test is the rest. No custodian-library or gateway change was needed — the placement
contract is already identity-consistent on both sides (gateway `write_fragments` places
fragment *i* at `placement[i]` = *i*, `crates/core/src/write.rs:221-243`; `FanoutChunkStore`
routes `dserver % n`, `crates/chunkstore-grpc/src/fanout.rs:72`; the custodian resolves
`placement[i]` → D-server *i*). The gap was purely that the deployable role never *ran* the
scan.

## Alternatives considered and rejected (with costs)

1. **Keep the hand-enqueue (iteration 1).** Rejected by the reviewer and re-rejected here:
   it does not prove derivation and gives C4-verify no production change to revert. Cost of
   keeping it: the load-bearing assertion is vacuous (0-line production change → no red).

2. **Model the loss as a process-killed D-server (like `custodian_day_one`'s
   `DeadDServer`).** A wholly-unreachable peer is dropped by the reachability probe
   (`live_reconstruction_view`, `custodian.rs:200`), so scrub never scans its fragments and
   cannot derive the obligation; deriving obligations for a process-dead node needs the
   desired-state/registration detector that `custodian.rs:54-60` documents as **out of
   scope**. So a genuine process-kill is *incompatible* with "derive via the shipping scan"
   — the reviewer's own requirement forces modelling the loss as a **scrub-detectable data
   loss** (a reachable D-server whose fragment is gone), which is exactly issue #330's case.
   I delete the fragment the gateway placed on D-server 1 over gRPC (a real, reachable
   `Ok(None)`, `crates/chunkstore-grpc/src/client.rs:126-128`) and frame it as the durable
   loss a D-server failure causes.

3. **Change `reconstruction::reconcile` to scan all chunks instead of draining a queue.**
   Rejected: that rewrites the custodian library's obligation model (a cross-crate change to
   `crates/custodian/src/reconstruction.rs`, blast radius across every reconstruction
   caller + the DST campaign) to do what the existing `scrub` loop already does. Wiring the
   shipping scrub loop is the smaller change (server-crate composition only, +36 lines) and
   uses code already proven by `crates/custodian/tests/scrub.rs`.

## Refuting my own test (forced answers)

- **(a) Genuine red?** YES. `run-verify.sh` reverts the `custodian.rs` scrub wiring (keeps
  the test) and the load-bearing assertion goes RED: `under_replicated` reads `Some(0.0)`,
  not `Some(1.0)` — the deployable role drains an empty queue. Verified through the project's
  own per-fix runner: "PASS — red without the fix, green with it."
- **(b) Production path?** YES. The test drives `Gateway::put_object`
  (`crates/server/src/lib.rs:147`, the real S3 PUT composition `serve_s3_role`/`cmd_s3` use)
  and `run_reconstruction_over_backend` (`cli.rs:758`, the exact `cmd_custodian` deployable
  backend-open path), over **real loopback gRPC D-servers** and a **real redb** store shared
  between the PUT and the custodian. No mock, copy, or re-implementation.
- **(c) Fixture includes the fault?** YES. The fault is the real deleted fragment on the
  reachable D-server 1; the custodian derives the obligation from that actual loss over the
  shared store, not a curated-in queue entry. The failing element is present, not excluded.

## Two demonstrated reds (proving the count derives from the gateway write)

1. **Revert the scrub wiring (production fix):** gauge reads `Some(0.0)` → RED. This is the
   C4-verify red leg (run through `engine/scripts/run-verify.sh`).
2. **Drop the loss** (temporarily deleted a never-placed fragment index 99 instead of the
   real index 1): scrub finds all placed fragments present → enqueues nothing → gauge reads
   `Some(0.0)` → RED. Demonstrated locally, then reverted; the test file is unchanged from
   the shipped version (0-line diff vs staged).

Either negation is the "empty store sees zero repair" symptom, proving the obligation count
derives from the gateway-written placement + the observed loss.

## Verification run (project runner)

- `engine/scripts/run-verify.sh` (C4-verify, base `origin/feat/m4-production-metadata-backend`):
  **PASS** — green with fix, red without.
- `cargo test -p wyrd-server` (whole crate, incl. all loopback-gRPC suites): **all green**;
  `custodian_day_one.rs` (11 tests) unchanged — the scrub wiring does not regress the
  deployable-role peers (a killed/unreachable peer is dropped from the live fleet so scrub
  scans nothing new on it; a manual enqueue is still drained by reconstruction).
- `cargo fmt --check`: clean. `cargo clippy -p wyrd-server --tests -- -D warnings`: clean
  (commit-ready for the target's fmt/clippy hooks).

The patch is generated against `origin/feat/m4-production-metadata-backend` (applies clean;
the M4.5 worktree base is identical for every file touched) — the base the slice PR opens
against per INTEGRATION §2.

## Scope / off-Check (unchanged from the brief)

The live `deploy/small-multi-node/` demonstration (real TiKV, 9 Docker D-servers, live
Prometheus exporter) is off-Check under #367/#366; this slice delivers the in-process,
regression-guarded proof that the loop's logic closes over one shared store. No external
dependency beyond the base Rust toolchain was needed.
