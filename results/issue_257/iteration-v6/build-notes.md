# Build notes — issue 257 / m4.6-tier1-scenario-tier2 (iteration 6)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend`
(worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `5d87cc4` ==
`origin/feat/m4-production-metadata-backend`). Trait **byte-for-byte unchanged**
(`git diff HEAD -- crates/traits/src/lib.rs` → 0 lines; Invariant "trait stays unchanged").

This iteration **keeps the iter-5 structure the reviewer called "real"** (the pure
dispatch/quorum oracles, the deploy stack, the scenario tier tests) and surgically fixes
the **three** load-bearing defects the iter-5 sign-off named. Each fix is below with the
concrete before/after.

## Defect 1 — the self-referential flip (was: `interleavings_observed >= 1` toggled by a fixture bool)

**Iter-5:** the load-bearing red→green was `interleavings_observed() >= 1`, produced by
passing `false` to `with_await_inside_commit` — a boolean the fixture read back about
itself. Nothing behavioural failed before / passed after.

**Fix:** the model now has two **[`CommitMode`]s** that both await inside commit (the
interleaving is reachable in either) and differ **only** at the commit point
(`crates/dst/src/sim_tikv.rs:130-146` the enum; `:271-305` the commit):

- `AtomicCommit` — re-validates preconditions at the commit point (a correct percolator
  CAS); honours the `MetadataStore::commit` contract.
- `PrewriteTrust` — trusts the *prewrite* check and applies after the await **without
  re-checking** (the naive translation of redb's synchronous-commit assumption into async);
  **admits a stale commit** = a lost update, reachable only because commit awaits inside.

The binding assertion is now a **production-observable behavioural fact**: the *store's own
stale-commit oracle* (`stale_commits_admitted()`, `crates/dst/src/sim_tikv.rs:236-244`),
which counts commits *admitted* even though their preconditions no longer held at the
commit point. It is not a fixture reporting a boolean about itself — it is the trait
contract (commit-point atomicity) measured against the store's actual behaviour.

## Defect 2 — vacuous assertions (was: two runs that could not differ)

**Iter-5:** `assert_backend_equivalence` compared await-on vs await-off of the *same*
re-checking store, so no mode could produce a lost update; `assert_contract_survived` could
fail only if the hand-written CAS was buggy. Near-definitional, presented as binding.

**Fix (exactly the reviewer's suggested shape — "a prewrite-trust toggle that skips the
commit-point re-check"):** the seed and the runtime-free test now assert the *behavioural
difference between the two modes*:

- `AtomicCommit` admits **zero** stale commits — the trait contract *survives* the
  interleaving (binding: `crates/dst/tests/tikv_await_commit_interleaving.rs` `assert_contract_survived`,
  and `sim_tikv_await_commit.rs:130-156`).
- `PrewriteTrust` admits **≥ 1** stale commit under the *same* schedule — proving the
  interleaving is *consequential* and the survival assertion is **not vacuous** (oracle:
  `tikv_await_commit_interleaving.rs` seed assertion (3); `sim_tikv_await_commit.rs:158-179`).

Both assertions are genuinely violable: drop the commit-point re-check and the survival
assertion goes red with a real lost update; make prewrite-trust re-check and the oracle
goes red. Neither rests on "exactly one winner" — the binding property is the commit-point
atomicity the trait promises (`admitted_stale`), which permits any number of *legitimate*
winners and forbids only a stale admit (Invariant: "no leg or seed may rest its bindingness
on 'exactly-one-winner goes red'").

### The behavioural red I actually ran (not a claim)

Regressing the commit-point re-check (`AtomicCommit => commit_point_ok` → `=> prewrite_ok`)
and running `cargo test -p wyrd-dst --test sim_tikv_await_commit`:

```
atomic_commit_survives_the_interleaving_no_lost_update ... FAILED
  assertion `left == right` failed: AtomicCommit must admit no stale write at the
  commit point (ADR-0015): [Committed, Committed]   left: 1  right: 0
```

A real lost update (two `Committed`s over the same prior version), asserted behaviourally —
**not** a self-toggled counter and **not** a compile error. Restored after the demo.

## Defect 3 — the live runner false-greened (was: no `--features tikv`, no fault descriptor)

**Iter-5:** `faults.rs` ran the tier tests with `cargo test -p wyrd-metadata-tikv --test …`
but **no `--features tikv`**, so each took its `#[cfg(not(feature = "tikv"))]` skip branch
and returned success without connecting; and it exported only `WYRD_TIKV_PD_ENDPOINTS`, never
a fault descriptor, so the consistency leg treated the missing descriptor as a skip and
`run_metadata_tier1` reported "passed" with no fault materialised. The `MetadataLegVerdict`
oracle was advertised but never wired.

**Fix (`xtask/src/faults.rs`, `xtask/src/metadata_faults.rs`):**

1. **`--features tikv` is now in the argv** (`metadata_faults.rs:86-104`,
   `metadata_scenario_args` returns `[&str; 10]` incl. `--features tikv`). Without it the
   scenario skips; with it the scenario connects and runs the ADR-0015 assertions. The
   Check-time `cargo test --workspace` still builds feature-off, so the tier tests skip
   cleanly there (gate honesty preserved — verified: 3/3 `ignored`). A regression that drops
   the feature turns the orchestration test red
   (`xtask/tests/metadata_faults_orchestration.rs:68-75`, new assertion).

2. **The consistency leg now runs under a materialised minority partition** with the fault
   descriptor exported (`faults.rs` `run_metadata_consistency_under_partition`): probe target
   reachability *before* → inject a one-of-three port partition via `iptables`
   (`PartitionGuard`, self-heals on **every** path incl. panic via `Drop`) → probe *after* →
   export `WYRD_METADATA_FAULT_{BEFORE,AFTER}` + `WYRD_METADATA_{REPLICAS,PARTITIONED}` →
   run the scenario under the fault → heal → probe recovered. The scenario no longer skips
   (the descriptor is present) and independently re-checks `partition_materialized`, so a
   no-op fault reds it too.

3. **The verdict derives from real observations** (`faults.rs`, building
   `MetadataLegVerdict` from the probes + scenario exit, then gating on
   `metadata_leg_passes`): `fault_materialized` from `wyrd_testkit::partition_materialized`
   over the *independent* probes, `retains_quorum` from `MetadataQuorumPlan::is_valid_minority_fault`,
   `read_after_commit_holds`/`converged_once` from the scenario's exit status, `self_healed`
   from the recovery probe. Not hand-set struct fields. This required adding
   `wyrd-testkit` as an `xtask` dependency (`xtask/Cargo.toml`).

The reviewer asked me to **keep** the `partition_materialized` oracle and the quorum
arithmetic (`crates/testkit/src/lib.rs`) — unchanged from iter-5, still 15/15 green.

## On C4-verify / run-verify.sh — the red is a *compile* red, and why

I ran the gate:

```
$ PDCA_BUNDLE=… WYRD_REPO=… ./engine/scripts/run-verify.sh
run-verify.sh: PASS — red without the fix, green with it.
```

It passes, but be clear about **what** the red is: run-verify reverts the modified/added
production files and re-runs the added test files. This slice's binding logic is a **net-new
module** (`crates/dst/src/sim_tikv.rs`) and a **net-new dep edge** (`metadata-tikv`/`xtask`
→ `wyrd-testkit`), so reverting them makes the added test files fail to **compile** (module /
crate unresolved) — a *structural* red, the same situation the harness itself carves out with
its `GREEN_ONLY` branch for net-new crates. It is **not** a behavioural red, and it cannot be
one for a net-new-module slice: the only production logic under test is the module the patch
adds, and the in-scope-frozen crates (`traits`, `core`, `metadata-tikv/src`) must not be
touched to manufacture one.

The **genuine behavioural** red→green therefore lives where it can: the two `CommitMode`s and
the stale-commit oracle, exercised (a) under madsim through production `wyrd_core::write`
(`tikv_await_commit_interleaving.rs`, green via `cargo xtask dst`) and (b) runtime-free on a
hand-rolled two-task scheduler in ordinary `cargo test` (`sim_tikv_await_commit.rs`, so
`cargo xtask ci` exercises the flip where the madsim seed compiles to nothing). The
demonstrated red above is the behavioural flip; it is real, violable both ways, and driven by
production code.

## Why the sweep is existential on the naive side (a correctness fix this iteration)

`interleaving_sweep` asserts the **correct** model survives **every** seed 0..16 (universal),
but the **naive** model exposes a lost update on **at least one** seed (existential): whether
two *overwrites* actually overlap at the commit point is a property of the individual madsim
schedule (the four-phase write also commits intents/creates, which interleave freely but
aren't the lost-update site). Asserting a stale overwrite on *every* seed was wrong and went
red on seed 0 in testing; the pinned `SEED` and the deterministic runtime-free test are the
witnesses that the interleaving *is* consequential.

## Scope / posture unchanged from the accepted plan

- **OUT:** literal Jepsen/Elle (ADR-0039 defers to #329) — the consistency leg is the in-repo
  Rust scenario; `MetadataConsistencyRoute::LiteralJepsenTool` is representable only so a
  regression to it is caught.
- **DEFERRED / off-Check:** the live Tier-1/Tier-2 green (needs a privileged Docker+iptables
  host + a ≥3-replica TiKV cluster) — BUILT (compiles feature-on: `cargo check -p
  wyrd-metadata-tikv --features tikv --tests` → clean; skips clean feature-off) and confirmed
  only in the privileged Tier job. A pre-declared C2/C4 sign-off item, not an unbuilt
  deliverable.
- No new ADR authored (the metadata-nemesis methodology question stays the human's call;
  the `iptables` port-partition here is one illustrative mechanism within Invariant B, not a
  seated deliverable).

## Gates run (worktree, all green)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy -p xtask -p wyrd-testkit -p wyrd-dst --all-targets -- -D warnings` — clean;
  `RUSTFLAGS=--cfg madsim` clippy on `wyrd-dst --all-targets` — clean; `wyrd-metadata-tikv
  --all-targets` — clean.
- `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test tikv_await_commit_interleaving` — 3/3
  green; `--test concurrency` (pre-existing seed) — 1/1 green.
- `cargo test -p wyrd-dst --lib --test sim_tikv_await_commit` — 4 lib + 2 behavioural green.
- `cargo test -p xtask --test metadata_faults_orchestration` — 9/9 green.
- `cargo test -p wyrd-metadata-tikv --test tier1_metadata_integration --test
  tier1_metadata_consistency --test tier2_metadata_io` — 3/3 skip clean (feature off).
- `cargo test -p wyrd-testkit` — 15/15 green.
- `cargo check -p wyrd-metadata-tikv --features tikv --tests` — clean.
- `cargo build --workspace` — clean; `cargo run -p xtask -- statics` (ADR-0035) — clean.
- `./engine/scripts/run-verify.sh` (C4-verify) — PASS (red without fix, green with it; the
  red is the compile red characterised above).

## Citations (target branch `feat/m4-production-metadata-backend`)

- Scaffolding extended: `xtask/src/faults.rs` (the `jepsen_dispatch`/`plan` pattern),
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` / `tier2_integration.rs`
  (endpoint-gated skip pattern), `crates/testkit/src/lib.rs` (`SeededNetFaults`),
  `crates/dst/tests/concurrency.rs` (the synchronous-commit rationale the seed corrects),
  `crates/traits/src/lib.rs` (the unchanged `MetadataStore::commit` contract),
  `crates/core/src/{write.rs,metadata.rs}` (the production `commit_overwrite` →
  `commit_chunk_map` CAS the seed drives), `deploy/small-multi-node/docker-compose.yml`.
- Plan: proposal 0015 §"DST and tests" / §"Pinning the trait with the second implementation"
  / PR-sequence item 6; ADR-0039 (in-repo scenario, literal Jepsen → #329); ADR-0009
  (DST correctness authority); ADR-0015 (single-zone contract); ADR-0006 (two implementations).
