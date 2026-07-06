# Build notes — issue 257 / m4.6-tier1-scenario-tier2

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend`
(worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `5d87cc4` ==
`origin/feat/m4-production-metadata-backend`). Trait **byte-for-byte unchanged**
(`git diff crates/traits/src/lib.rs` empty).

## What the Success criterion actually demands, and where each piece lands

The brief splits into three layers. The **binding, at-Check, flippable** layer is the
load-bearing evidence the four prior iterations lacked; the **binding, deferred** layer
is built (compiles + skips clean) but observed only in the privileged Tier job; the
**out** layer (literal Jepsen) is not attempted (ADR-0039).

### 1. The DST seed — the one on-Check red→green (Success criterion §1.1)

`crates/dst/tests/tikv_await_commit_interleaving.rs` (new) drives the **production**
`wyrd_core::write` four-phase commit path (mirroring the proven
`crates/dst/tests/concurrency.rs:35-94`) against a new simulated-TiKV backend,
`wyrd_dst::sim_tikv::SimTikvMetadataStore` (`crates/dst/src/sim_tikv.rs`, new).

**Root cause it closes.** `concurrency.rs`'s rationale ("each `commit()` is internally
synchronous … no `await` inside", now corrected at `crates/dst/tests/concurrency.rs:3-16`)
is redb-specific (`crates/metadata-redb/src/lib.rs:6-10`). madsim interleaves only at
`.await` boundaries, so a synchronous commit is atomic w.r.t. the scheduler and a whole
class of interleavings is **unmodelled** — the gap proposal 0015 §"Pinning the trait with
the second implementation" (`…0015…revised.md:546-555`) names. A TiKV commit awaits
network I/O between prewrite and commit, so those interleavings are reachable.

**The model.** `SimTikvMetadataStore::commit` (`crates/dst/src/sim_tikv.rs:190-243`):
snapshot generation + `network_round_trip().await` (a `YieldOnce`, one scheduler-visible
suspension) + **re-check preconditions at the commit point** and apply atomically. The
commit-point re-check is the optimistic CAS that keeps the trait contract intact under
the interleaving. An `interleavings` counter records commits that observed the generation
move under them across their await — the **positive materialisation oracle**.

**The three assertions** (the seed, `…interleaving.rs:225-249`):

1. **Materialisation oracle** (`interleavings_observed() >= 1`) — the primary flip. RED
   under a synchronous commit (nothing interleaves → counter `0`), GREEN once the
   await-inside-commit model is exercised.
2. **Read-after-commit / linearizability** (`assert_contract_survived`) — the object
   reads back as one writer's *whole* payload; applied generations are contiguous
   monotonic; version bumps once per committed overwrite.
3. **No lost update, framed as backend-equivalence** (`assert_backend_equivalence`) — the
   await run reaches the **same** final version + committed-count as the synchronous
   reference run of the identical scenario. This is the "consistency-over-the-swap"
   thesis, and it is deliberately **NOT** "exactly-one-winner goes red" (which the
   Invariant forbids — re-proving atomicity DST already owns).

**Demonstrated reds** (ran, then restored — this is the load-bearing proof, not a claim):

- Remove the `network_round_trip().await` (or `with_await_inside_commit(false)`): the
  materialisation oracle fails —
  `panicked … no await-inside-commit interleaving materialised (seed 0xc011715eed)`.
  This is the exact red the uncorrected synchronous-commit assumption produces, and the
  seed's sibling test `no_interleaving_reachable_under_synchronous_commit` asserts it
  directly in the same green run.
- Make the store trust its **prewrite** precondition check instead of re-checking at the
  commit point (the naive CAS): `assert_backend_equivalence` fails — a second stale writer
  commits under the interleaving, diverging the version from the reference. This proves
  the contract assertion is load-bearing, not vacuous, and that the bug it catches ONLY
  manifests once the interleaving is reachable.

Green under the project runner: `cargo xtask dst` (the whole DST suite, 50-seed sweep)
passes including the seed (3/3). The seed itself is seed-independent (`interleaving_sweep`
over seeds 0..16) and pins a canonical `MADSIM_TEST_SEED = 0xC011715EED` asserted on, not
`eprintln`'d (Success criterion §1.1).

Why the model lives in `src/` (a library item, not the test file): proposal 0015
§"Crate touch-points" puts "a deterministic simulated-TiKV model" in `dst`, and #258 folds
the seed in — so it must be reusable. That required `wyrd-traits`/`async-trait`/`bytes` as
ordinary deps of `dst` (`crates/dst/Cargo.toml:15-25`); they were already dev-deps, so the
graph is unchanged, only the scope. `YieldOnce` is hand-rolled on `core::task` alone so
the model compiles in the non-`madsim` build too, and the pure CAS decision
(`preconditions_hold`) is unit-tested with no runtime (`crates/dst/src/sim_tikv.rs`
`#[cfg(test)]`, 4 tests, green in ordinary `cargo test`).

### 2. Pure decision logic — dispatch + fault-effect oracle + quorum arithmetic (§1.2)

Mirrors the existing `xtask/src/faults.rs` `jepsen_dispatch` pattern
(`xtask/src/faults.rs:160-189`) and the `xtask::disk_faults` verdict pattern
(`xtask/src/disk_faults.rs:116-140`), so each is RED when negated, GREEN on the tree, no
tautology (an independent oracle, not the literal the function returns).

- **`xtask::metadata_faults`** (`xtask/src/metadata_faults.rs`, new; `pub mod` in
  `xtask/src/lib.rs:18`): `metadata_consistency_route` — the consistency leg routes to the
  in-repo scenario (ADR-0039) and **never** the literal Jepsen tool (deferred to #329,
  representable only so a regression to it is caught — the iter-4 demand ADR-0039 forbids).
  `metadata_leg_passes` — the fault-effect oracle: ANDs `fault_materialized` (Invariant B:
  the partition provably took effect, not the injector's claim) + `retains_quorum` +
  read-after-commit + `converged_once` + `self_healed`. Tested by
  `xtask/tests/metadata_faults_orchestration.rs` (new, 9 tests) — each Invariant-B
  component flips the verdict red.
- **`wyrd_testkit`** (`crates/testkit/src/lib.rs`, additive):
  `partition_materialized(before, after)` — true only on an observed
  Reachable→Unreachable transition (the asymmetric-no-op-partition guard, iter-2/3/4);
  `MetadataQuorumPlan` — minority-partition arithmetic (`retains_quorum`, `is_effective`,
  `is_valid_minority_fault`, `max_faultable`) so a majority partition (takes the store
  down) and a zero-node partition (no-op) both fail. 6 new unit tests, green.

The live legs consume the **same** shared oracle: `tier1_metadata_consistency.rs` validates
its env-exported fault descriptor via `wyrd_testkit::partition_materialized` +
`MetadataQuorumPlan::is_valid_minority_fault`, so "the leg cannot pass with the fault
absent" is enforced by the identical pure check the Check-time unit tests pin.

### 3. Deferred / off-Check legs — built, compile, skip clean

- `crates/metadata-tikv/tests/tier1_metadata_integration.rs`,
  `tier1_metadata_consistency.rs`, `tier2_metadata_io.rs` (new) — drive the **production**
  `TikvMetadataStore` commit path behind the unchanged trait (mirroring
  `crates/metadata-tikv/tests/contention.rs`), endpoint-gated + `#[ignore]`d, so they
  compile in `cargo test --workspace` and skip cleanly with no `WYRD_TIKV_PD_ENDPOINTS`
  (verified: 3/3 skip green). The consistency leg is additionally fault-descriptor-gated
  (Invariant B).
- `xtask::faults::run_metadata_tier1 / run_metadata_tier2` (`xtask/src/faults.rs`, deferred
  via the existing `plan()` gate) + `metadata-tier1 / metadata-tier2` subcommands
  (`xtask/src/main.rs:62-63,85`). The runner routes via the pure `metadata_consistency_route`
  so the lib is load-bearing in the binary too.
- `deploy/metadata-3replica/docker-compose.yml` (new) — one PD + three TiKV stores so PD
  places every region across three replicas (a genuine Raft group a survivable minority
  partition can exercise). `deploy/README.md` documents it. Do does **not** seat a specific
  nemesis mechanism as the deliverable (brief §Scope) — the fault mechanism is the runner's
  choice within Invariant B; the compose only provides topology.

## Alternatives ruled out

- **Assert "exactly-one-winner" in the seed** — forbidden by the Invariant (re-proves
  atomicity DST owns; unfalsifiable-here since `metadata-tikv/src` is out of scope). The
  seed asserts materialisation + backend-equivalence instead, never a winner count.
- **A decorative seed re-proving redb atomicity (the v1 defect)** — avoided by forcing the
  racer to interleave *inside* commit (the `YieldOnce` suspension) and asserting the
  interleaving materialised; a synchronous commit makes it red.
- **A compile-only flip (the v3 defect)** — the flip is behavioural (the materialisation
  oracle + backend-equivalence go red at runtime with the await removed / the CAS naïve),
  demonstrated above, not merely a deleted-module compile error.
- **Put the sim model in the test file** — rejected: proposal 0015 puts it in `dst` as a
  library item and #258 folds it in, so it must be reusable; test-file-local would force
  #258 to re-implement it.

## Verification-posture caveat (pre-declared NEEDS-HUMAN)

The DST seed's red→green is behavioural and only observable under `--cfg madsim` (via
`cargo xtask dst`), exactly like the existing `concurrency.rs` seed. A C4-verify that runs
`cargo test` on the seed **without** `--cfg madsim` compiles it to nothing and cannot see
the red — a known DST-seed property, not specific to this slice. The demonstrated reds
above are the load-bearing evidence; the live Tier-1/Tier-2 green is confirmed only in the
privileged CI/eval Tier job (brief §Verification posture / §Known NEEDS-HUMAN). The
metadata-nemesis ADR-refinement question and the static-endpoints reduced bar are the
human's call (brief §Known NEEDS-HUMAN) — no ADR authored.

## Gates run (worktree)

- `cargo xtask dst` (project runner): full DST suite green incl. the seed (3/3).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy` (normal + `--cfg madsim`) on `wyrd-dst`, `wyrd-testkit`, `xtask`,
  `wyrd-metadata-tikv`, `--all-targets`: clean (`-D warnings`).
- `cargo test`: `wyrd-testkit` 15/15, `xtask` (all suites incl. deploy-guard,
  orchestration 9/9), `wyrd-dst --lib` 4/4, metadata-tikv tier tests skip clean 3/3.
- `cargo build --workspace` + `cargo xtask statics` (ADR-0035): clean.
