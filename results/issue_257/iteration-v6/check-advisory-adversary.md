# Adversarial review — issue 257 (m4.6-tier1-scenario-tier2), iteration 6

Advisory only; never gates. Grounded on the applied target at
`/home/eddie/wyrd/wyrd.pdca-wt-l0`. The patch is test/harness-only; no production
code (`metadata-tikv/src`, `wyrd_core`, `metadata-redb`, `traits`) changes. I
attacked the at-Check red→green, the DST seed's binding assertion, and the live
runner. The iter-5 rejection named three defects; below is where I judge each is
still live.

## Refutations (human must adjudicate)

- NEEDS-HUMAN — **The load-bearing "trait contract survives" assertion is a
  tautology, not a behavioural fact.** In `crates/dst/src/sim_tikv.rs:291-301`,
  `AtomicCommit` sets `admit = commit_point_ok`, then `admitted_stale = admit &&
  !commit_point_ok`. For the correct mode this is `commit_point_ok &&
  !commit_point_ok`, i.e. **identically `false`** — the module comment at
  `:298-299` says so outright. Therefore `stale_commits_admitted()` is provably `0`
  for an `AtomicCommit` store under **any** schedule, interleaving, or CAS bug that
  does not edit that very line. The seed's binding check
  `assert_eq!(report.stale_commits_admitted, 0, …)`
  (`crates/dst/tests/tikv_await_commit_interleaving.rs:191-192`, called from
  `:259` and `:321`) thus asserts boolean algebra, not that the modelled backend
  survives the interleaving. This is exactly the iter-5 "vacuous assertion / near-
  definitional check presented as a binding oracle" defect — made *more* vacuous,
  since it can no longer fail even for a buggy CAS. The only assertion with teeth is
  the **negative control** (`PrewriteTrust` admits ≥1), which proves a deliberately-
  broken, patch-authored mode is broken — not that the correct model is correct.
  Brief §1.1 requires the binding assertion to be "the trait contract survives the
  newly-reachable interleavings"; that assertion, as written, cannot go red.

- NEEDS-HUMAN — **Every remaining sub-assertion in `assert_contract_survived`
  is model bookkeeping, unfalsifiable by scheduling.** In
  `crates/dst/tests/tikv_await_commit_interleaving.rs:186-219`: (a) "no torn read /
  whole writer payload" cannot fail because the sim store holds whole `Bytes` in a
  `HashMap` (`sim_tikv.rs:305-311`) — an in-memory map has no partial write to tear;
  (b) "commit generations contiguous from 1" re-checks a counter the store itself
  increments by exactly 1 per applied commit (`sim_tikv.rs:312`) recording the post-
  increment value — 1,2,3… by construction; (c) `final_version == PRIOR + winners`
  is the same per-commit bookkeeping. None can be violated by any interleaving, so
  none tests the "does the abstraction match the real store" thesis the slice
  exists to prove. This is the iter-5 finding ("re-derives atomicity the store's own
  coupled bookkeeping guarantees") unresolved; `PrewriteTrust` was added beside it
  but the positive block stayed decorative.

- NEEDS-HUMAN — **The at-Check "red→green" is a patch-authored `CommitMode`
  toggle — a self-toggle in substance, the v3/iter-5 shape.** The only way to make
  the seed's binding block red is to switch the store from `AtomicCommit` to the
  deliberately-broken `PrewriteTrust`, both authored in this same patch
  (`sim_tikv.rs:205-219`, `:291-296`). Nothing in unchanged production code fails
  before and passes after; `wyrd_core::write` is merely *driven over* the fake,
  whose correct mode assumes (does not test) the commit-point atomicity in
  question. `check-gates.json` C4-verify asserts "red without the fix, green with
  it" via `./engine/scripts/run-verify.sh`, which is **not present** in the target
  worktree and could not be re-run here. A human must confirm that run-verify's red
  is behavioural against a genuine production/harness assumption — not (i) mere
  file-absence of a brand-new test, nor (ii) flipping the in-patch `CommitMode`. On
  the source available, I cannot find a non-self-authored red.

## Weaker / off-Check notes

- NEEDS-HUMAN — **Live consistency oracle: two "independent" ADR-0015 sub-checks
  are the same bit.** `xtask/src/faults.rs:771` and `:773` set both
  `read_after_commit_holds` and `converged_once` to `scenario.is_ok()` — the cargo
  test process exit code. `MetadataLegVerdict` (`xtask/src/metadata_faults.rs`)
  advertises granular ADR-0015 components, but the runner collapses read-after-
  commit and exactly-once convergence into one process-exit bit, so
  `metadata_leg_passes` cannot distinguish them. Off-Check and unverifiable here;
  the scenario test asserts both internally, so not a false-green by itself, but the
  oracle's advertised granularity is illusory.

- NEEDS-HUMAN — **The injected partition is asymmetric — the exact shape
  Invariant B warns against.** `xtask/src/faults.rs:2107-2109` (applied) injects
  `iptables -A INPUT -p tcp --dport <20162/20182> -j DROP`, dropping only *inbound*
  traffic to tikv2's ports; tikv2's outbound still flows. That is an asymmetric
  partition — the brief lists "asymmetric no-op partition" among the iter-2/3/4 bugs
  Invariant B forbids. The reachability probe (`probe_reachability`, dialling
  `20182`) will still read `Unreachable` and credit the fault as materialised, so
  the runner's oracle does not detect the asymmetry. Overlaps the pre-declared
  NEEDS-HUMAN (partition-of-a-live-Raft-node methodology); flagging that this diff's
  concrete mechanism is one-directional. Privileged/off-Check — not reproducible in
  the Check worktree.

## Attempted-but-could-not-refute

- The `testkit` pure oracles held under probing: `partition_materialized`
  (`crates/testkit/src/lib.rs`, INPUT before/after transition) rejects no-op,
  already-down, and heal cases; `MetadataQuorumPlan::is_valid_minority_fault`
  rejects majority, zero-node, and <3-replica shapes with real negative unit tests.
  These are genuinely non-tautological — no refutation found.
- The `PrewriteTrust` negative control genuinely has teeth: `admitted_stale =
  prewrite_ok && !commit_point_ok` (`sim_tikv.rs:301`) can be 0 or ≥1 depending on
  the schedule, so it is a real behavioural signal (a broken store loses an update).
  My objection is only that it is the *negative* control, not the *binding* claim.
- Attempted to show the live runner false-greens on a no-op partition: it does not —
  `fault_materialized` derives from independent probes and gates the verdict
  (`faults.rs`), and the scenario re-checks `partition_materialized`
  (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:1126`-equiv). The
  iter-5 `--features tikv` / descriptor-injection false-green is genuinely fixed.

(The gating `cargo fmt --all -- --check` failure in `check-gates.json` C4-ci is
already recorded by the deterministic gate; not re-filed here.)
