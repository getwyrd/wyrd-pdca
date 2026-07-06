# Adversarial review — issue_366 / obs-floor-observability (iteration 4)

Skeptic's pass. Attacked the red→green evidence, the fix's edges, and the reviewer's
verdict. Grounded on the target worktree at `$PDCA_TARGET` (patch applied). Findings below.

## Attack on the evidence

- **NEEDS-HUMAN — the gating C4-ci failure is a REAL, reproducible flake in the exact
  read-back seam the keystone's binding evidence rides on — not pre-existing noise to
  wave away.** `check-gates.json` reports C4-ci `fail` (exit 101) while C4-verify reports
  `pass`. I reproduced it: `cargo test --workspace --exclude wyrd-dst` passed twice then
  **failed exit 101 on the third run** at `crates/custodian/tests/rebalance.rs:884`
  (`emits_per_failure_domain_utilization_on_the_durability_seam`). The panic shows
  `gather_prometheus()` returned only `target_info` — the metric event was silently dropped
  because `tracing` caches per-callsite *interest* process-globally, and under the gate's
  parallel binary/test scheduling a sibling test evaluated the callsite first with no
  `MetricsLayer` active (`Interest::never`), so nobody records it. This is the **same
  `DurabilityTelemetry` → `tracing`→OTel → `gather_prometheus` read-back mechanism** the
  patch's own binding day-one signal asserts through (`crates/server/tests/custodian_day_one.rs:1372`
  `under_replicated()` → `gauge_value("reconstruction_under_replicated")`). The patch does
  **not** fix this seam — it changes the under-replicated callsite from `monotonic_counter.`
  to `gauge.` and adds *more* read-back tests on the same fragile mechanism. So the
  C4-verify "green" is non-deterministic: it passes in isolation (I ran `custodian_day_one`
  5× and `-p wyrd-custodian` 6× green) but the *gate's* full-workspace parallel run is
  precisely where it breaks. The deterministic gate is correctly RED; treating it as
  unrelated flake is the unwarranted rationalization to guard against.

- **The binding day-one signal is asserted through a parallel re-implementation, never the
  production entry `cmd_custodian`.** `crates/server/src/cli.rs:518` (`fn cmd_custodian`) is
  the real `wyrd custodian` binary path — arg parse, `resolve_backend`,
  `open_local_meta_redb`/`open_tikv_meta`, `GrpcChunkStore::connect_with_timeout`, and the
  `ConfiguredDServer` fleet build. **No test drives it** (confirmed: no `cli::run` /
  `cmd_custodian` / `"custodian"` reference anywhere under `crates/**/tests`). Tests 1–2 in
  `custodian_day_one.rs` call `service.reconcile_pass(...)` directly over a hand-built
  `MemMeta` + hand-built `configured([...])`; test 3 calls `run_reconstruction_until` but
  still with `MemMeta` and hand-built fleet. The header comment claims this is "the same
  production wiring the `wyrd custodian` binary runs (`cli::cmd_custodian` → …)"
  (`custodian_day_one.rs:1017-1020`) — but the binary's own backend routing, connect path,
  and fleet construction execute in **no** test. The iteration-3 primary rejection (custodian
  opening the wrong metadata backend) could regress verbatim and every gate here would stay
  green.

## Attack on the fix — a concrete input that breaks it

- **`crates/server/src/cli.rs:585,589` — the default branch fabricates topology, the exact
  defect iteration-3 rejected, and the brief's canonical bring-up command triggers it.**
  `--ids`/`--failure-domains` are optional (default empty: `cli.rs:531-532`). When absent,
  `id: ids.get(i).copied().unwrap_or(i as u64)` keys D-servers **positionally by endpoint
  index**, and `failure_domain: … unwrap_or_else(|| endpoints[i].clone())` uses the **endpoint
  URL** as the domain. The day-one command the brief cites — `wyrd custodian --otlp-endpoint …`
  (brief L30-31) — carries no `--failure-domains`, so it lands in this branch. Concrete break:
  two D-servers physically in the same rack/failure-domain but reached at different URLs get
  **distinct** fabricated domains → `Topology` believes them independent → a rebuilt fragment
  can be re-placed into the same real failure domain as a survivor, defeating the durability
  invariant. The `ConfiguredDServer` docstring asserts these are "NOT fabricated positionally
  from the `--endpoints` order" (`crates/server/src/custodian.rs:810-813`) — the default path
  does exactly that. NEEDS-HUMAN: is "operator must hand-align `--ids`/`--failure-domains`"
  acceptable when the documented day-one command omits them and the default silently invents
  topology?

- **NEEDS-HUMAN — `crates/server/src/cli.rs:541` per-process `MemCoordination::new()`
  "single-active" is unsound for the *production* backend the keystone gates.** The startup
  log advertises "leader for zone … host-local single-active" (`cli.rs:546-548`) and the
  docstring justifies it with "the redb store's exclusive file lock keeps a second custodian
  off the same `--data-dir`" (`cli.rs:496-498`). But the brief pins production to **TiKV**
  (redb is dev/eval only), and TiKV is a shared networked store with **no `--data-dir` file
  lock**. Two `wyrd custodian` processes on one host pointed at the same TiKV backend each
  build their own process-local `MemCoordination`, which always grants leadership
  (`coordination-mem` lone-process grant), so **both run reconstruction concurrently** — the
  file-lock safety argument does not hold on the very deployment (#367) this floor gates. The
  CAS-commit argument bounds corruption but not the false "single-active" advertisement or
  the duplicated work. iteration-3 re-scoped cross-process fencing out; the human should
  confirm the deferral knowing the host-local guarantee is redb-only, not TiKV.

## Attack on the verdict

- **`crates/custodian/src/reconstruction.rs:135-143,192` — the gauge measures "queued
  obligations that are degraded," not "every under-replicated chunk," despite the comment.**
  The tally iterates only `queue = repair::queued_repairs(ctx.meta)` (`:128`). A chunk that
  has genuinely lost redundancy but whose repair obligation was never enqueued (no scrub /
  read-repair has flagged it yet) contributes **0**. The docstring claim "EVERY chunk this
  pass found below its scheme's fragment count" (`:135-137`, `:210-213`) overstates: the
  day-one rise depends on something first calling `enqueue_repair` (the tests do so by hand
  at `custodian_day_one.rs:1416`). Any reviewer statement that the gauge is a true durability
  *level* is unwarranted — it is a level over the *repair queue*, not the chunk population.

## Attempted but could not refute

- The gauge-vs-counter rationale for "returns to zero" (`reconstruction.rs:519-540`) holds:
  I confirmed in isolation the gauge reads 1 then 0; a monotonic counter would pin at 1.
- Test 2's red→green is mechanically real: pre-fix `plans.len()` is 0 for an `Unrepairable`
  chunk and the counter exports as `..._total`, so `gauge_value("reconstruction_under_replicated")`
  returns `None ≠ Some(1.0)` (red); post-fix the gauge matches (green).
- Test 3 (`run_loop_survives_a_dead_dserver_and_keeps_running`) genuinely drives the real
  `run_reconstruction_until` spine over a fleet that includes the dead node — the survive-the-
  kill loop is exercised (though only over `MemMeta`, not through `cmd_custodian`).
