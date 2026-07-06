# Build notes — issue 366 (obs-floor keystone, iteration 8)

Withheld from the reviewer. Rationale, alternatives, and the red→green story.

## What this iteration is

Iteration 7 was signed off as **"close"**: the telemetry-crate extraction, the runnable
custodian role, the backend-open path, and the rise→return-to-zero gauge all landed and the
governance/scope calls were ratified. It was **rejected** on three integrity defects in the
*binding day-one signal*. I took the whole iteration-7 patch as the base (it applied cleanly to
`feat/m4-production-metadata-backend`) and made **only** the three targeted corrections plus
their tests. Nothing else in the v7 approach changed — the ratified deferrals stand (see below).

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l0`, base commit `5d87cc4`).

## The three MUST-FIX items (iteration-7 carry-forward)

### 1. Transient-vs-permanent at the membership/classification seam (false data-loss page)

**Defect:** `live_reconstruction_view` (server/src/custodian.rs) drops any D-server whose health
probe errs. `reconstruction::assess` then saw those fragments as missing; when survivors fell
below `k` it returned `Unrepairable` and fired `emit_data_loss` — a **false permanent-data-loss
alarm** on physically-intact fragments during a rolling restart / partition.

**Fix (the invariant to restore: a below-`k` shortfall must be classified data-loss only when the
missing fragments are *confirmed gone*, not merely *unreachable this pass*):**
- Added `ReconstructionContext.unreachable: &'a [DServerId]` — the configured-but-unreachable set
  the role dropped this pass (`reconstruction.rs`, struct + doc). It is `&[]` for every M3/library
  context, so those keep **identical** semantics (an empty set ⇒ every missing fragment is
  "confirmed gone", exactly the prior behaviour).
- `live_reconstruction_view` now returns that set as a third tuple element
  (`server/src/custodian.rs`); the run loop threads it into the ctx.
- `assess` counts `transient_missing` (missing fragments whose placed server ∈ `unreachable`) and,
  in the below-`k` branch, returns the new **`Assessment::Unreachable`** when
  `survivors + transient_missing >= k` (recoverable), else `Unrepairable` (confirmed loss). The
  `Unreachable` case is raised on a **distinct, lower-severity** gauge
  `reconstruction_unreachable`, NOT the data-loss counter, and the obligation stays queued.

**Why the distinction is keyed on the dropped-server set, not on per-fragment fetch errors:** the
M3 test `reconstruction.rs::a_transient_fault_is_not_turned_into_a_spurious_re_placement` requires
that a transient fault on an **in-fleet (reachable)** server *propagates* (unwinds the pass).
That contract is preserved. The new distinction is precisely "server the role dropped as
unreachable this pass" vs "reachable server whose fragment is confirmed gone" — information only
the deployable role has, hence the explicit context field.

### 2. All-unreachable startup fleet must FAIL LOUD (§6.5)

**Defect:** `cli.rs` empty-fleet path returned `Ok(())`. A long-running custodian would exit 0 on
a total fleet outage / bad `--endpoints`; the supervisor would not restart it.

**Fix:** the empty-fleet case now **panics** (non-zero exit + diagnostic naming the endpoint
count). The per-peer start-degraded policy inside `connect_fleet` (skip ONE down peer, repair
around it) is unchanged — only the ALL-unreachable case fails loud.

### 3. "Returns to ZERO" for a repairable chunk with no free domain (never-completable repair)

**Defect:** a `Repairable` chunk (survivors ≥ `k`) whose rebuild has no free distinct domain to
land on (minimal cluster at exactly `n`, one domain down) was counted on
`reconstruction_under_replicated`, then `repair_chunk`'s `select_distinct_domains_excluding` erred
and unwound the pass — flooring the backlog gauge at ≥1 forever.

**Fix:** `assess` now runs the same domain selector up front; if no free distinct domain remains it
returns the new **`Assessment::Blocked`**, routed to a distinct `reconstruction_repair_blocked`
gauge and kept OFF the repairable-backlog gauge (so the binding rise→zero signal is never floored)
and out of the repair loop (so the pass no longer unwinds). It clears when capacity returns.
This is deliberately narrow: it only fires when the selector finds **no free domain**. The existing
`RepairOutcome::Aborted` path (selector picks a server outside the fleet view — a *different*
condition the M3 test `an_aborted_repair_is_not_counted_as_a_successful_repair` covers, where a
ghost domain G *is* free) is untouched, and that test still passes.

## Design alternatives weighed

- **Topology heuristic instead of an explicit `unreachable` field** (a missing fragment whose
  server isn't in the live topology ⇒ transient): rejected. It conflates "configured-but-down" with
  "decommissioned / never-configured", giving a **false negative** — a genuinely-gone server's
  below-`k` loss would be silently classified `Unreachable` and never page. The explicit field is
  the only source that can tell down from gone, and only the role has it.
- **Sentinel "unreachable store" kept in the fleet** (assess buckets a marker error): rejected —
  it introduces a cross-crate error-marker seam and changes `assess`'s transient-error contract,
  which the M3 propagation test depends on.
- **Cost of the explicit field:** it is a `pub` field on a struct 28 external/test call-sites
  construct, so all 28 gained `unreachable: &[],` (a mechanical, semantically-inert default — an
  empty set cannot change any M3 outcome). That footprint is the honest price of a correct,
  role-supplied distinction; it touches 3 M3 test files v7 did not (tier1_disk_faults +1,
  chunkstore-grpc ×2 +6), purely to satisfy the new field with the prior-behaviour default.

## Red → green

The bundle's named test is `crates/server/tests/custodian_day_one.rs` (the brief's path is marked
ILLUSTRATIVE; this is the established v7 location). Three assertions pin the three fixes:

- `a_transient_below_k_outage_does_not_false_alarm_data_loss_and_recovers` — two RS(2,1)
  D-servers transiently down (survivors < k). Asserts `data_loss == 0` and the distinct
  `reconstruction_unreachable` gauge = 1, then full recovery (all gauges 0, obligation drained)
  when the nodes return. Old logic ⇒ `Unrepairable` ⇒ `data_loss ≥ 1` (would fail).
- `a_repair_with_no_free_domain_is_blocked_off_the_backlog_gauge` — minimal 3-server cluster, one
  killed. Asserts `under_replicated == 0` across two passes (never floored) and
  `reconstruction_repair_blocked == 1`. Old logic ⇒ the pass unwinds (the `.expect` would panic)
  and the backlog gauge floors at 1.
- `cmd_custodian_fails_loud_when_the_whole_fleet_is_unreachable_at_startup` — `#[should_panic]`.
  Old logic returned `Ok(())` (guard unsatisfied ⇒ red); new logic panics ⇒ green.

**Mechanical run** (via the project's cargo toolchain in `$PDCA_WORKTREE`):
- `cargo test -p wyrd-server --test custodian_day_one` → **11 passed** (incl. the 3 above).
- `cargo test -p wyrd-custodian` → all suites green (reconstruction 14/14 incl. the Aborted +
  transient tests; tier1_disk_faults compiles with the new field).
- `cargo test --workspace --exclude wyrd-dst` → 84 suites, **0 failures**.
- `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=5 cargo test -p wyrd-dst --test custodian` → 10 passed.
- `cargo fmt --all --check` → clean; `cargo clippy --workspace --exclude wyrd-dst --all-targets`
  → clean (the `--all-features` tikv-arm lint at `cli.rs:133` is pre-existing code behind
  `#[cfg(feature="tikv")]`, which the gate's clippy — no `--all-features` — does not compile).

The "red" side is structural: the new tests reference the runnable role, the telemetry crate, the
`unreachable` field and the `reconstruction_unreachable`/`_repair_blocked` gauges — none of which
exist on the bare target branch — so they cannot compile there. The harness `C4-verify` gate has a
known limitation reproducing this (the `git mv custodian/src/telemetry.rs → telemetry/src/lib.rs`
rename yields `pathspec … did not match`), flagged for the human in prior iterations; unchanged
here — it is a run-verify limitation, not a code defect.

## Ratified deferrals carried forward (do NOT re-litigate — per iteration-7 sign-off)

- §6.1 membership/fencing (probe-and-drop + warning-only tikv single-active) → the #365 etcd
  `Coordination` backend. The `cli.rs` fencing log/docstring stays HONEST about what the tikv arm
  does/does not enforce.
- §6.2 keystone = 0010 PRs 1–2; telemetry extraction into `crates/telemetry`; typed-errors × #255
  lands after M4.
- §6.3 in-process `gather_prometheus` read-back is the at-Check evidence; live-exporter run → #367
  (collector substrate getwyrd/wyrd#446).
- §6.4 the two-metric split (repairable backlog gauge + dedicated `reconstruction_data_loss`) is
  the accepted operator contract — this iteration ADDS two more distinct levels
  (`reconstruction_unreachable`, `reconstruction_repair_blocked`) in the same spirit: keep the
  backlog gauge a true return-to-zero level, surface every non-repairable-now condition on its own
  signal.

## Commit-readiness

fmt + clippy clean on every touched crate; `Cargo.lock`/`Cargo.toml` updated for the telemetry
crate (from the v7 base). Patch is `git diff` against the target branch base and applies to it.
