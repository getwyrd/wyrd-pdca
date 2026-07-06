# Build notes — issue 366 / obs-floor-observability (keystone, iteration 4)

## What this bundle delivers

The observability-floor **keystone** (proposal 0010 items 1–2): the durability-telemetry
seam extracted into a shared `wyrd-telemetry` crate, and `wyrd custodian` wired as a
**runnable, deployable role** in `server`, proving the day-one signal — kill a D-server →
the under-replicated count **rises then returns to zero**, read back through the role's own
`gather_prometheus` surface.

Files (all cited on target branch `feat/m4-production-metadata-backend`):
- `crates/telemetry/{Cargo.toml,src/lib.rs}` — new crate; the seam moved out of
  `crates/custodian/src/telemetry.rs` (deleted).
- `crates/custodian/src/lib.rs`, `crates/custodian/Cargo.toml` — re-export from
  `wyrd_telemetry`; drop the OTel deps (now the telemetry crate's).
- `crates/custodian/src/reconstruction.rs` — the under-replicated **gauge** (was a
  monotonic counter) counting the **full degraded set**.
- `crates/server/src/custodian.rs` (new), `crates/server/src/lib.rs`,
  `crates/server/src/cli.rs`, `crates/server/Cargo.toml` — the deployable role + `wyrd
  custodian` subcommand.
- `crates/dst/tests/custodian.rs` — metric-name follow (counter→gauge).
- `crates/server/tests/custodian_day_one.rs` (new) — the at-Check regression.
- `Cargo.toml`, `Cargo.lock` — workspace member + dep.

## How each iteration-3 hard-reject is addressed

**Primary defect — backend routing.** iteration-3 was rejected because `cmd_custodian`
hardcoded `open_local_meta_redb(data_dir)`; on a TiKV cluster the custodian would open an
empty local redb, see zero chunks, and the gauge would never rise. The base tree has since
merged M4.4 (`resolve_backend` + `--metadata-backend`, `crates/server/src/cli.rs:110-120`).
`cmd_custodian` now routes through **exactly that seam** — `resolve_backend(&parsed)` and a
`match backend { Redb => open_local_meta_redb, #[cfg(feature="tikv")] Tikv => open_tikv_meta().await }`
mirroring `cmd_put`/`cmd_get` (`cli.rs` new `cmd_custodian`). The custodian opens the *same*
store the cluster wrote to.

**No-timeout hang.** The fleet clients dial with
`GrpcChunkStore::connect_with_timeout(endpoint, connect_timeout)`
(`crates/chunkstore-grpc/src/client.rs:79`), not the no-timeout `connect`, so a paused /
partitioned peer fails a fetch with a transient `DEADLINE_EXCEEDED` rather than hanging the
reconcile loop. `--connect-timeout-secs` (default 10) is operator-tunable.

**Fabricated failure domains.** iteration-3 rejected the synthetic `domain-{i}` keying. The
role now takes each D-server's **operator-supplied stable id + failure domain** (`--ids`,
`--failure-domains`, aligned to `--endpoints`) — matching each D-server's own registered
`--id`/`--failure-domain` — and never invents topology from the endpoint index. Deriving
these automatically from the registration record needs the cross-process discovery seam
(the out-of-scope etcd `Coordination`), so until then the operator supplies the real
topology; the default (`failure_domain = endpoint`) is at least distinct-per-endpoint, not a
fake domain collapse.

**Test drives the real run loop, incl. the dead node.** `custodian_day_one.rs` now exercises
the production `CustodianService::run_reconstruction_until` continuous loop (tests 3 + 3b +
4), not only `reconcile_pass`, and the survival policy iteration-3 said was untested:
- `run_loop_survives_a_dead_dserver_and_keeps_running` — the loop drives repair around an
  unreachable node and exits `Ok` at shutdown (never crashed);
- `run_loop_logs_and_continues_on_a_store_fault` — a **reachable-but-faulting** server makes
  each pass return `ReconcileError::Store`; the loop logs-and-continues and survives;
- `run_loop_stops_when_fenced` — a superseded custodian's loop returns `Err(Fenced)`.
The fleet input **includes** the killed node (a `DeadDServer` handed in, not curated out).

## Red→green (the project's own runner)

`./engine/scripts/run-verify.sh` (C4-verify) → **PASS — red without the fix, green with it.**
The iteration-3 mechanical failure (`pathspec 'crates/telemetry/src/lib.rs' did not match`)
is gone: the patch is generated with `git diff --no-renames`, so the telemetry move is a
`+++ /dev/null` delete + `--- /dev/null` add, never a git *rename* — so the RED phase's
`git checkout -- <modified>` never targets a path absent from the base. `cargo test
--workspace --exclude wyrd-dst` (the C4-ci test phase) is green across all 29 binaries,
including `rebalance.rs` (the pre-existing tracing-callsite-race the iteration-3 note called
flaky — it passed here); `cargo fmt --check` and `cargo clippy -D warnings` clean.

The RED is a *compile* red (the test drives production modules — `wyrd_server::custodian`,
`wyrd_telemetry` — that do not exist without the fix, because the runnable role is the very
thing being introduced). The **behavioral** load-bearingness of the gauge fix is shown
separately: reverting only `emit_under_replicated(under_replicated)` back to
`emit_under_replicated(plans.len())` makes `gauge_counts_a_loss_beyond_tolerance` read 0
instead of 1 (a below-`k` loss blind-spots the gauge). That entanglement is inherent to a
keystone that introduces the process surface; it is not a weaker test, just a
new-infra red.

## Invariants held

- Purely additive instrumentation + wiring: the reconstruction loop gains a count tally +
  a gauge callsite; no commit-protocol / consistency-contract / on-disk-format change.
- No telemetry backend leaks into a leaf crate: the seam lives in `wyrd-telemetry` behind
  `ExporterConfig`; `custodian` only emits via `tracing`; `server` chooses the backend at
  role entry (ADR-0012).
- Reuse, don't rebuild: the request/capacity planes (items 4–5) will share the *same*
  `wyrd-telemetry` handle — the extraction exists precisely for that.
- The gauge (level) not a monotonic counter: only a gauge returns to zero through an
  accumulating Prometheus registry, which *is* the rise-then-zero day-one signal.

## Open — NEEDS-HUMAN (carried forward, not silently resolved)

These are genuine judgment / recorded-decision items per 0010's graduation criteria and the
prior sign-offs; they are flagged, not papered over:

1. **Survive-the-kill: probe stand-in vs cause-fix (iteration-3 §C5a).** The role survives a
   *permanently* dead D-server via a reachability **probe** in `live_reconstruction_view`
   (drop-and-read-around) + the run loop's log-and-continue. The durable answer to "which
   D-servers are live" is registration/lease-driven membership via the etcd `Coordination`
   discovery seam — the **out-of-scope other half** of 0015's prerequisite (brief §"Out of
   scope"). Whether the probe stand-in is acceptable for the first-deployment gate, or the
   `reconstruction::assess` classification should instead treat unreachable-during-
   reconstruction as *missing*, is a recorded human/proposal decision. Not resolved here.
2. **Cross-process leader election (iteration-2 §6.3).** `MemCoordination` is process-local:
   host-local single-active is real (the redb store lock), cross-host is not. No corruption
   is possible regardless (the reconstruction repoint is a version-conditional CAS). Genuine
   cross-host fencing awaits the etcd `Coordination` (out of scope). iteration-3 re-scoped
   this as open judgment "not blocking the do" — confirm the deferral or require it.
3. **Shared `crates/telemetry` extract vs keep-in-custodian.** Done as *extract* per the
   iteration-1 sign-off; confirm.
4. **Typed-errors × #255 (M4.4) sequencing (floor item 6)** and **live-exporter fitness** —
   off-Check, pre-agreed sign-off items; unchanged by this bundle.
5. **Milestone decomposition.** This is the keystone slice (items 1–2). Items 3–7 land as
   their own bundles.

## What I ruled out and why

- **Keeping telemetry in `custodian`** (smaller diff — no new crate, no `Cargo.lock` churn,
  ~90 fewer lines): rejected because the iteration-1 sign-off *required* the extract, and
  the request/capacity consumers (server) would otherwise anchor the export path in a leaf
  crate (0010 invariant). Cost of extract is the new crate + lock entry; it is the specified
  target, not the smallest diff.
- **A real in-process gRPC D-server fleet in the test** (spin `DServer::bind` on 127.0.0.1
  + real redb): rejected for the at-Check regression because the run loop is time-driven
  (interval + shutdown future), so asserting an exact gauge value mid-loop is racy/flaky.
  The in-memory trait stores drive the **same** production role methods
  (`run_reconstruction_until` → `live_reconstruction_view` → `reconcile_pass` →
  `reconcile_step`) deterministically. The backend-routing correctness (redb/TiKV) is a
  `resolve_backend` mirror of `cmd_put`, unit-covered by the M4.4 backend-selection tests;
  a live TiKV cluster is the off-Check first-deployment gate (#367).
