# Adversarial review — issue_366 (iteration 8, keystone: telemetry seam + runnable custodian + durability signal)

Attacked the three iteration-7 MUST-FIXes the sign-off turns on (transient-vs-permanent
classification, all-unreachable fail-loud, no-free-domain "returns to zero") plus the
red→green evidence. Two findings I could not talk myself out of, one evidence weakness,
and several attempts that failed (the fix held).

## Findings

- **NEEDS-HUMAN — the new transient/permanent split makes `reconstruction_data_loss` blind to
  the most common real permanent loss: a node that dies and stays dead.**
  `crates/custodian/src/reconstruction.rs:396` classifies any below-`k` shortfall as
  `Assessment::Unreachable` (no alarm) whenever `survivors.len() + transient_missing >= k`,
  where `transient_missing` counts every missing fragment whose placed server is in
  `ctx.unreachable` (`reconstruction.rs:377`). `ctx.unreachable` is rebuilt **each pass** purely
  from the current health probe in `crates/server/src/custodian.rs:200-222` — there is **no
  aging / last-seen / grace** (grepped: none), so a *permanently dead* server is in
  `unreachable` on every pass, forever. Concrete failing case: RS(2,1), n=3, two of the three
  D-servers are killed and never come back (the ordinary "two nodes died" permanent loss).
  survivors=1, transient_missing=2, sum=3 ≥ k=2 → `Unreachable` on every pass →
  `emit_data_loss` (`reconstruction.rs:297,490`) **never fires**. The `data_loss` counter can
  only be reached when fragments are missing on a *reachable* server (fragment absent but
  `health()` = Ok). The test that "proves" data-loss
  (`crates/server/tests/custodian_day_one.rs:2735`, `a_loss_beyond_tolerance_…`) manufactures
  the below-`k` condition exactly that way — it `delete_fragment`s bytes off two *live*
  `MemDServer`s (`custodian_day_one.rs:2764-2775`) rather than killing the servers. So the
  signal the brief frames as "a loss beyond tolerance… DATA LOSS" is demonstrated only for
  on-disk deletion under a live server, and is silent for node death — the dominant durability
  incident and precisely the §7.4 day-one fault. This false-negative is *introduced by this
  diff* (pre-diff, below-`k` was unconditionally `Unrepairable`); real membership/lease that
  would tell "dead" from "blip" is deferred to #365. A human must decide whether shipping a
  data-loss alarm that cannot see a dead server is acceptable, or whether the deferral note
  must say so explicitly.

- **NEEDS-HUMAN — the binding "kill a D-server → under-replicated rises then returns to zero"
  does not hold on a bare, exactly-`n` deployment; it requires spare failure-domain capacity.**
  `crates/custodian/src/reconstruction.rs:409` diverts a `Repairable`-in-principle chunk to
  `Assessment::Blocked` (off the `reconstruction_under_replicated` gauge, onto
  `reconstruction_repair_blocked`, `reconstruction.rs:288,468`) whenever
  `select_distinct_domains_excluding` finds no free domain distinct from the survivors. Because
  `live_reconstruction_view` registers the topology only from the *reachable* subset
  (`crates/server/src/custodian.rs:210-212`), killing one node of a minimal RS(2,1) 3-node
  cluster leaves topology = {survivor A, survivor C} with no free domain → `Blocked`. The
  builder's own test `a_repair_with_no_free_domain_is_blocked_off_the_backlog_gauge`
  (`custodian_day_one.rs:3001`, a 3-server cluster) asserts exactly this:
  `under_replicated == 0.0`, `reconstruction_repair_blocked == 1.0` (`:3064-3074`), and it
  **stays** blocked across passes (`:3086-3096`) — it never returns to zero until an operator
  adds a domain. Every drill that *does* show the brief's under-replicated rise→zero
  (`custodian_day_one.rs:2631`, `:2735`, `:3305`) hands the role a **4th spare server (domain
  D)**. So the load-bearing at-Check signal, as literally stated in the brief
  (`brief.md:52-59`), only manifests on the `under_replicated` gauge when spare capacity
  exists; on a real minimum-width cluster the same kill surfaces on a *different* gauge that
  does not return to zero. The #367 operator runbook needs to know which metric to watch is a
  function of spare capacity — a human should ratify this as the intended contract rather than
  let "kill a D-server, watch under-replicated rise then settle to zero" stand unqualified.

- **NEEDS-HUMAN (evidence) — the per-fix red→green was never mechanically run; "RED pre-fix"
  is asserted in prose, and it would be a compile error, not a behavioral failure.**
  `check-gates.json:41-49` records C4-verify **fail**: `pathspec 'crates/telemetry/src/lib.rs'
  did not match any file(s) known to git` (the `git mv custodian/src/telemetry.rs → new
  crates/telemetry` rename artifact, recurring since iteration 3). The entire day-one suite
  (`crates/server/tests/custodian_day_one.rs`, new file) depends on symbols this diff
  introduces — `Assessment::{Unreachable,Blocked}`, `emit_data_loss`,
  `reconstruction_repair_blocked`, `run_reconstruction_over_backend`, `ConfiguredDServer`,
  `ReconstructionContext.unreachable` — so it cannot compile against the pre-fix tree. The
  asserted "pre-fix reads 2 / no data_loss metric / floors at 1" (e.g. `custodian_day_one.rs:2823`,
  `:2833`) is therefore reasoned, not observed. C4-verify being non-gating means nothing
  mechanically proved these tests fail for the *defect* rather than for missing API. A human
  should either accept the code-derived red explicitly or have run-verify taught the
  new-crate-rename so the proof is real.

## Attempts that failed (the fix held)

- Tried to show the iteration-7 `RepairOutcome::Aborted` gauge-floor survives: assess's
  `Blocked` pre-check (`reconstruction.rs:409`) calls the *same* `select_distinct_domains_excluding`
  that `repair_chunk` uses (`reconstruction.rs:530`), and topology/fleet/`stores` are built from
  one reachable set (`custodian.rs:210-212`), so the second abort path (`reconstruction.rs:542`,
  target outside the fleet) cannot fire in the deployable role — a no-domain chunk is diverted
  before it ever becomes a plan. Could not floor the backlog gauge this way.
- Tried to break the all-unreachable fail-loud: `crates/server/src/cli.rs:1497-1511` panics on an
  empty fleet *inside* `runtime.block_on` on the calling thread, so it unwinds to `main` →
  non-zero exit; the redb open happens only afterward in `run_reconstruction_over_backend`, so
  the panic is not gated behind I/O. The `#[should_panic]` test (`custodian_day_one.rs:3617`)
  and the misaligned-topology reject test (`:3597`) drive `cmd_custodian` end-to-end. Could not
  make it exit 0.
- Tried to find a false `Blocked` that under-counts real backlog (assess more conservative than
  repair): the two selector calls are identical in arity and inputs, so assess and repair agree.
  Could not produce a Repairable-that-assess-calls-Blocked divergence on the production path.
