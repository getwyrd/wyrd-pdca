# Build notes — issue 366 / obs-floor-observability (keystone items 1–2, iteration 3)

**Target branch:** `getwyrd/wyrd @ feat/m4-production-metadata-backend`
(worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`, detached at the current M4
integration tip — M4.4/#427 + M4.5/#428 are merged in). All `path:line` citations are
against that tree, post-edit.

## Scope of this bundle

The keystone slice (0010 items 1–2 + the day-one durability signal). This iteration keeps
the iteration-2 shape that was accepted at C1–C5 — the extracted shared `wyrd-telemetry`
crate (`crates/telemetry/src/lib.rs`, moved out of `custodian`) and the deployable
`wyrd custodian` role in the `server` crate — and **corrects the three defects the
iteration-2 sign-off rejected** (brief.md:200). Each correction is a *cause* fix with its
own red→green leg, not a probe over a symptom.

## Correction 1 — Adv-2: the gauge must cover the WORST losses, not just repairable ones

**Defect (rejected):** `emit_under_replicated(plans.len())` counted only
`Assessment::Repairable` chunks. A chunk below `k` (`Unrepairable`) or with a corrupt
placement (`Malformed`) emitted **0** — so an RS(2,1) chunk that lost two fragments read a
healthy `0` on the very signal an operator watches, while it sat un-reconstructable.

**Fix (cause):** the under-replicated count is now the **full degraded set**. `reconcile`
tallies `under_replicated` across `Repairable` **and** `Unrepairable` **and** `Malformed`
(only `Drain` — a deleted/already-full chunk — is excluded) and emits that total.
- `crates/custodian/src/reconstruction.rs:144` (the tally), `:159` (`Unrepairable` counts),
  `:165` (`Malformed` counts), `:192` (`emit_under_replicated(under_replicated)`).
- Doc updated at `:522`/`:540` (0010 §"DST and tests"; `0005:326-329`).

This is the smallest change that restores the invariant "the under-replicated *level*
reflects every degraded chunk". It is purely a change to the *value* emitted — no loop
logic, no commit-protocol touch (brief.md:153 invariant).

**Red→green (verified through `cargo test`):** the new test
`under_replicated_gauge_counts_a_loss_beyond_tolerance`
(`crates/server/tests/custodian_day_one.rs:456`) kills two of three RS(2,1) fragments
(survivors 1 < k 2 → `Unrepairable`) and asserts the gauge reads `1`. Reverting the tally
to the pre-fix "repairable only" semantics reproduces the RED (`left: Some(0.0), right:
Some(1.0)`); the fix is GREEN. Reproduced and restored in this session.

## Correction 2 — Adv-1: the deployable role must SURVIVE a killed D-server (real path)

**Defect (rejected):** an unreachable/timed-out fetch is classified *transient*
(`reconstruction.rs:341`, `is_permanent_read_fault` → false) and propagated
(`reconstruction.rs:274`, `Err(e) => return Err(e)`). In production `cmd_custodian` built
the fleet from **all** `--endpoints` including the node that dies, so the first assessment
fetch after a §7.4-step-4 kill unwound the pass → `run_until` `.await?` → `cmd_custodian`
→ **the process exited on the fault it exists to repair around.** The iteration-2 test
dodged this by hand-building a `healthy_fleet` that *excluded* the dead node.

**Fix (cause, at the role boundary — NOT loop logic):** the role derives its **live fleet
by probing reachability every pass**. A D-server whose health probe errs (unreachable) is
dropped, so the reconstruction plane reads *around* it (its fragments resolve as missing
and are rebuilt from the ≥k survivors) and the role keeps running.
- `live_reconstruction_view` (`crates/server/src/custodian.rs:106`, the drop at `:115`
  `if d.store.health().await.is_ok()`) returns the `(fleet, topology)` built from only the
  reachable subset. `ConfiguredDServer` (`:78`) is the configured-endpoint input.
- `run_reconstruction_until` (`crates/server/src/custodian.rs:253`) rebuilds that live view
  each pass and **additionally logs-and-continues** on a per-pass `ReconcileError::Store`
  (`:285`) — a server that dies *after* the probe degrades the pass, not the process — while
  a `ReconcileError::Fenced` (superseded term) correctly stops.
- `cmd_custodian` now builds `ConfiguredDServer`s from every `--endpoints` entry (the dead
  one included) and drives `run_reconstruction_until` (`crates/server/src/cli.rs:494`,
  `:572` the build, `:583` the call). The dead node is handled at the boundary, not curated
  out of the input.

Why the fix is at the role boundary and not in `reconstruction::assess`: the brief invariant
(brief.md:153-156) is "the custodian loops gain emission points, **not new logic**." Changing
`assess`'s permanent-vs-transient classification (so an unreachable server is read-around)
would be a durability-semantics change to the M3 loop (it deliberately does not convert a
reachable-but-flaky fragment into a re-placement). The reachability probe lives in the *new*
`server`-crate wiring instead, reusing the same `health()` signal 0010 item 7's readiness
probe reflects — so "is this server serving?" has one answer, and the loop logic is untouched.

**Red→green:** the binding test
`under_replicated_gauge_rises_then_returns_to_zero_surviving_a_killed_dserver`
(`crates/server/tests/custodian_day_one.rs:345`) hands the role the **real production fleet
including the dead node** (an unreachable `DeadDServer`, `:139`, whose every call errs with a
transient transport error — not an integrity fault, not `EIO` — the exact shape a dead gRPC
endpoint raises). It drives the production `live_reconstruction_view` (`:395`), asserts the
dead server is dropped (`live fleet == [0,2,3]`), runs two `reconcile_pass`es, and reads the
gauge back off the role's own `gather_prometheus`: `1` after the kill, `0` after repair — the
pass never errors (`.expect("the role survives the killed D-server")`). Reverting
`live_reconstruction_view` to keep unreachable servers reproduces the RED (the live-fleet
assertion fails `left: [0,1,2,3]`, and the pass would then error on the dead server); the fix
is GREEN. Reproduced and restored in this session.

This is the *same* `live_reconstruction_view` + `reconcile_pass` path `cmd_custodian` drives,
so the test exercises production, not a parallel re-implementation. `MemMeta` / `MemDServer` /
`Fleet` are the standard in-memory trait doubles the whole custodian suite already uses.

## Correction 3 — §6.3: honest, genuine single-active (no false advertisement)

**Defect (rejected):** the binary advertised unqualified "single-active for zone" while
constructing a process-local `MemCoordination` whose `elect_leader` always grants
(`crates/coordination-mem/src/lib.rs:184`). Two deployed custodians would not fence.

**What I found:** cross-process coordination is **not built anywhere in the tree** — `wyrd
d-server` also constructs a per-process `MemCoordination::new()`
(`crates/server/src/cli.rs`, the d-server branch) and registers into its *own* process's
map; nobody else sees it. The etcd-backed `Coordination` is the explicit **out-of-scope
"other half" of 0015's deployment prerequisite** (brief.md:106: "the etcd-backed
`Coordination` + gateway process role … its own body of work"). So a full cross-host
election is out of this bundle's scope, and half-building an etcd/flock backend would add a
new dependency + module that itself needs review (and contradicts brief.md:106).

**Fix (honest + genuine where it is real):** stop the false claim, and state the *real*
guarantees:
- **Host-local single-active IS real and enforced** — `open_local_meta_redb`
  (`crates/server/src/cli.rs:511`) opens the metadata store under redb's exclusive OS file
  lock, so a second `wyrd custodian` on the same `--data-dir` cannot start. For the day-one
  Tier-2 **single node** (brief.md:69) this is genuine single-active, at zero new dependency.
- **No corruption is possible even without cross-host fencing** — the reconstruction repoint
  is a version-conditional (CAS) commit (`reconstruction.rs:454`), so two racing custodians
  never both win; the loser's rebuilt fragments are collectable garbage. The adversary's
  "both run reconstruction" is wasted work, not a durability hazard.
- The advertisement now says exactly this (`crates/server/src/cli.rs:534`: "leader … host-
  local single-active via the store lock, cross-host fencing pending the etcd Coordination
  backend"), and the doc (`:482`) records the scope. Cross-host election is surfaced as a
  NEEDS-HUMAN below (it is a system-wide gap, not a custodian-specific bug).

I judged that genuinely implementing cross-host election here would be scope creep against
brief.md:106; the maintainer/human should confirm this disposition (it is the one place the
carry-forward and the brief scope pull in different directions — flagged, not worked around).

## Minor adversary/codex notes addressed

- **False subscriber comment (rejected note):** the iteration-2 module doc claimed the
  binary installs a process-global `set_global_default`, which it does not. Corrected to
  state honestly that the role installs a **metrics-only** scoped bridge; the
  `--log-level`/`RUST_LOG` `fmt`/`EnvFilter` subscriber (0010 item 3) is a follow-on slice,
  so the loops' non-metric events (audit lines, malformed-placement warning) are emitted but
  captured by no log sink — a documented gap (`crates/server/src/custodian.rs:27-52`).
- **DServerId keyed by `--endpoints` index (rejected note):** kept positional keying for the
  keystone (deriving stable ids from each D-server's registration record needs cross-process
  discovery — out of scope), but documented the hazard and the day-one mitigation (pin
  `--endpoints` to the registered id order) at `crates/server/src/cli.rs:565` and
  `crates/server/src/custodian.rs:74`.

## Commit-readiness

- `cargo fmt -p wyrd-telemetry -p wyrd-custodian -p wyrd-server` → applied, `--check` clean.
- `cargo clippy -p wyrd-telemetry -p wyrd-custodian -p wyrd-server --all-targets` → 0 warnings.
- `cargo build --workspace` + `cargo test --workspace --no-run` → clean.
- The extracted `wyrd-telemetry` crate adds **no new third-party dependency** — it carries the
  same already-approved OTel/prometheus/tracing deps `custodian` already had; `server` gains
  only workspace crates + `tracing`/`tracing-subscriber` (already in-graph). ADR-0003 /
  `deny.toml` surface unchanged — no NEEDS-HUMAN for a dependency.
- The full `cargo xtask ci` (C4-ci: fmt/clippy/build/test/deny/conformance/statics + 50-seed
  DST sweep) was not run end-to-end here; the targeted fmt/clippy/build/test + the DST
  `durability_emission_rises_then_returns_to_zero` (under `--cfg madsim`, GREEN) cover the
  touched surface. The gate re-runs it at Check.

## Pre-existing flake to be aware of (NOT introduced here) — #214

The `custodian` `reconstruction` test binary carries `emits_the_three_repair_metrics_on_the_
durability_seam` alongside sibling tests that hit the same `emit_*` callsites under **no**
subscriber. `tracing` caches per-callsite *interest* in process-global state, so under
multi-thread `cargo test` a no-subscriber sibling can register "not interested" first and
drop this test's metric (issue #214 — why `gc_telemetry.rs`/`backfill_telemetry.rs` are
already isolated into their own binaries). I saw it fail 1×, then pass 3× in a row, and pass
in isolation — the classic race signature. My change touches only the *value* emitted, never
the callsite interest, so it neither introduces nor worsens the race (the flake pre-dates the
counter→gauge work). Flagging it so a single red C4-ci run on that test is recognised as this
known flake, not a regression; the #214 test-isolation cleanup is a separate slice. The
new day-one test lives in its **own** binary (`crates/server/tests/custodian_day_one.rs`) and
is not subject to it.

## Known NEEDS-HUMAN / carry-forward for later slices

- **Cross-HOST single-active (etcd `Coordination`).** Genuine cross-node fencing is the
  out-of-scope other half of 0015 (brief.md:106); host-local single-active + the CAS commit
  are real today. The human should confirm this disposition (the carry-forward §6.3 asked for
  "cross-process leader election"; the brief scopes etcd out — the one genuine tension).
- **Milestone decomposition** (items 3–7 as follow-on slices) — unchanged.
- **Typed-errors × M4.4 (#255) sequencing** (0010 item 6) — #255 is now MERGED into the base;
  the enum must target its final `MetadataStore` shape. Untouched here; human call to record.
- **Shared `crates/telemetry` extraction** — done (maintainer decision T5(a)); confirm at
  sign-off it is the intended home.
- **Live-exporter evidence is off-Check** — the real Prometheus scrape / OTLP run on a Tier-2
  node is the pre-agreed sign-off item; this bundle proves in-process `gather_prometheus`
  read-back + ships the OTLP push wiring.
</content>
</invoke>
