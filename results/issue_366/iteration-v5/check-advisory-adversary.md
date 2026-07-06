# Adversarial review — issue 366 / obs-floor keystone (iteration 5)

Scope: this diff only. Patch confirmed applied at `$PDCA_TARGET`
(`feat/obs-floor.1-keystone`); `cargo test -p wyrd-server --test custodian_day_one`
→ 6 passed. Attacks below ground on the target source, not the diff.

## Findings

- **NEEDS-HUMAN — `Malformed` chunks are counted on `reconstruction_under_replicated`,
  which both raises false durability alarms and defeats the binding "returns to ZERO"
  shape.** `crates/custodian/src/reconstruction.rs:164-167` adds `under_replicated += 1`
  for `Assessment::Malformed`. But `assess` returns `Malformed` from
  `reconstruction.rs:263` *before any fragment is fetched* — purely because
  `checked_fragments()` found a wrong-length placement vector. Such a chunk may have
  **all its fragments physically present** (full redundancy), yet it is reported on a
  gauge literally named *under-replicated*. Worse, a malformed placement is never
  auto-repaired (the code explicitly refuses to "rebuild over a fabricated identity
  vector"), so the obligation stays queued and is re-counted on **every** pass. Concrete
  failing case: a store containing one pre-existing malformed chunk plus a healthy fleet.
  An operator runs the §7.4 day-one step-4 drill (kill a D-server, watch the gauge "rise
  then return to zero"). The gauge floors at ≥1 forever, so it returns to **1, never 0** —
  the brief's BINDING success criterion ("rises and then returns to ZERO",
  `brief.md:52-59`) is unobservable. This is the direct side effect of the iteration-2
  Adv-2 request; whether "cover the worst losses" should override "returns to zero" is a
  human call, not silently resolvable in the counting line.

- **NEEDS-HUMAN — the process advertises a single-active safety property that is false on
  the production (TiKV) backend.** `crates/server/src/cli.rs:549` prints "host-local
  single-active via the store lock", and the docstring `cli.rs:497-503` justifies it by
  "the redb store's exclusive file lock keeps a second custodian off the same
  `--data-dir`". But the production path is `--metadata-backend tikv` →
  `open_tikv_meta()` (`cli.rs` tikv arm) — **no `--data-dir`, no local file lock**. Each
  process constructs its own process-local `MemCoordination` (`cli.rs:698` in diff /
  target `cmd_custodian`) which always self-grants leadership. Concrete case: two
  `wyrd custodian --metadata-backend tikv` on one host both elect, both log
  "single-active", and both run reconstruction concurrently against the same TiKV store.
  The CAS commit prevents corruption but not the wasted double-repair, and the printed
  claim is simply untrue for the deployment (#367) this gates. Carried from iteration-4
  §coordination and still unaddressed for the tikv arm.

- **The binding evidence never drives the real binary entry `cmd_custodian` — only the
  factored-out `run_reconstruction_over_backend`.** `crates/server/src/cli.rs:504`
  (`cmd_custodian`) is referenced by tests only in comments (`custodian_day_one.rs:11,438`);
  its body — arg parse, `resolve_backend`, `GrpcChunkStore::connect_with_timeout`
  (`cli.rs:577`), and the `id: ids[i]`/`failure_domain: domains[i]` fleet build
  (`cli.rs:587-593`) — is exercised by **no** test. This is the exact surface iteration-3
  (wrong backend) and iteration-4 (fabricated topology) were rejected on; a regression
  there could re-enter with every gate green. `require_aligned_topology` is unit-tested,
  but the `connect_with_timeout` dial and the client→`ConfiguredDServer` assembly are not.
  iteration-4 asked specifically for "a backend-driven process test **through
  cmd_custodian**"; the builder added one through the helper below it, leaving the
  binary's own glue uncovered.

- **NEEDS-HUMAN — `live_reconstruction_view` keeps `Health::Unhealthy` peers as survivor
  read / re-placement targets.** `crates/server/src/custodian.rs:128` drops a server only
  when `health().await` is `Err`; a reachable server returning `Health::Unhealthy`
  (e.g. a failing/degraded disk) stays in the live fleet and topology
  (`custodian.rs:129-130`). Concrete case: a rebuilt fragment is placed onto an
  Unhealthy-but-reachable node, or that node is trusted as a survivor read, so the repair
  "restores" redundancy onto a dying device. The docstring (`custodian.rs:112-118`)
  acknowledges this as deliberate; carried from iteration-4 §C5 — a human must decide it
  is acceptable-for-#367 or require unreachable-during-reconstruction to be treated as
  missing.

- **The gauge is a level over the repair *queue*, not the chunk population — the docstring
  overstates it.** The tally iterates only `queue = repair::queued_repairs(...)`
  (`reconstruction.rs:128,145`), yet `emit_under_replicated`'s docstring
  (`reconstruction.rs:540`, "EVERY chunk this pass found below its scheme's fragment
  count") and the inline comment (`reconstruction.rs:135-143`) claim total coverage.
  Concrete case: a chunk that has lost redundancy but has **not yet been enqueued** by a
  scrub/read-path finding contributes 0 — the "silent non-zero durability failure" the
  metric is sold as catching is still silent until something enqueues it. Flagged in
  iteration-4 and unaddressed; the docstring language is stronger than the code.

- **Evidence gap — the per-fix red→green was never mechanically demonstrated.**
  `check-gates.json` C4-verify = **fail** (`pathspec 'crates/telemetry/src/lib.rs' did
  not match any file(s) known to git`): the crate-extraction rename breaks the harness's
  checkout-of-pre-state, so red→green was reasoned, not run. It *is* logically sound for
  the gauge counting change (pre-fix `emit_under_replicated(plans.len())` excludes
  `Unrepairable`, so `gauge_counts_a_loss_beyond_tolerance` would read 0 → red), but the
  survival/run-loop tests depend on brand-new types (`CustodianService`,
  `live_reconstruction_view`) that do not exist pre-fix, so their "red" is a compile
  error, not a defect exhibition. A confirmatory reviewer accepting "red→green
  established" for those tests is on thin ice.

## Attempted but could not refute

- **The callsite-interest race (iteration-4's RED gate).** Ran `cargo test -p
  wyrd-custodian --tests` 5× — all green. Audited every test file: every reconcile-driving
  (metric-firing) test calls `enable_metric_callsites()` first; the only unguarded tests
  (`skeleton.rs:91` selector, `reconstruction.rs` `repair_priority_rises_as_redundancy_falls`)
  are pure functions that never hit a metric callsite. Could not reproduce the flake. The
  guard is per-test opt-in and thus fragile against a future metric-firing test that
  forgets the call, but no such gap exists in this diff.

- **The clean-store rise-then-zero signal.** `gauge_rises_then_returns_to_zero_*` drives
  the real `reconcile_step` seam (via `CustodianService::reconcile_pass`), not a parallel
  re-implementation, and the redb-backend variant drives the real `open_local_meta_redb`
  path. On a single-chunk store the 1→0 shape holds. Could not break it (the Malformed/
  Unrepairable floor above only bites on a *populated* store, which is why it is filed as
  a NEEDS-HUMAN tension rather than a refutation of the at-Check test).
