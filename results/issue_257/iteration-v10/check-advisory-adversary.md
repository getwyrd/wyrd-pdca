# Adversarial review — issue_257 (iteration 10)

Skeptic's pass. Grounded on the target at `feat/m4-production-metadata-backend`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`, patch not yet applied — production anchors read
on the base; new-in-patch paths cited by their intended target path). Scope = this diff.

## Findings

- **NEEDS-HUMAN — the iter-9-directed `feature_gated_checks()` now compiles the pre-1.0
  `tikv-client` tree inside the unprivileged `cargo xtask ci`, contradicting a documented
  workspace invariant.** The patch inserts `cargo check -p wyrd-metadata-tikv --features tikv
  --tests` into `run_ci` (patch `xtask/src/main.rs` `feature_gated_checks()` + the
  `for check in feature_gated_checks() { cargo(&check)?; }` loop, spliced after
  `xtask/src/main.rs:817`'s `cargo test --workspace --exclude wyrd-dst`). But the root
  `Cargo.toml:80-85` states in as many words that the `tikv` feature is off-by-default
  *precisely so* "the default `cargo xtask ci` on a laptop/worktree with no TiKV **never
  compiles or audits this tree** and stays green," and `crates/metadata-tikv/Cargo.toml:12-18`
  repeats it ("compiles this crate as an empty skeleton, never touches the `tikv-client`
  tree"). `cargo check --features tikv` *does* compile that dependency tree (it is a real
  ~grpcio-bearing tree — `Cargo.lock:2829`). **Concrete failing case:** on the exact "no TiKV /
  no networked toolchain" runner the comments describe, the previously-green gate now fails at
  the new check step — a portability/gate-honesty regression. iter-9 directed the *type-check*;
  the side effect of pulling the pre-1.0 tree (whose ADR-0003 audit is an open NEEDS-HUMAN,
  `Cargo.toml:83-85`) into every `cargo xtask ci` was not weighed against this invariant and
  needs explicit sign-off. The reviewer's "C4 ci: all checks passed" only proves it passed in a
  *toolchain-complete* environment.

- **NEEDS-HUMAN — the unit test that is supposed to lock in iter-9's fix is a tautology and does
  not test the load-bearing wiring.** `ci_type_checks_feature_gated_metadata_scenario` (patch
  `xtask/src/main.rs`, new test ~1596-1612) asserts only that `feature_gated_checks()` returns a
  vector containing its own hard-coded literal `["check","-p","wyrd-metadata-tikv","--features
  tikv","--tests"]`. It never calls `run_ci` and never asserts that `run_ci` iterates
  `feature_gated_checks()`. **Concrete failing case:** delete the `for check in
  feature_gated_checks() { cargo(&check)?; }` loop from `run_ci` — the scenario is no longer
  type-checked at Check (iter-9's exact regression returns), yet this test stays GREEN because
  the data source is untouched. This re-instantiates the iter-1 "assert the literal the function
  returns" shape the Success criterion forbids, now in the gate-wiring the reviewer credited as
  closing iter-9.

- **The C4-verify "PASS — red without the fix, green with it" (`check-gates.json:46`) carries no
  production-code weight and should not be read as one.** This iteration takes exit (b): the
  flagship seed `crates/dst/tests/tikv_await_commit_interleaving.rs` is honestly relabelled
  "pure redb coverage … NO correctness weight," and `crates/metadata-tikv/src` is byte-for-byte
  unchanged (the commit path at `crates/metadata-tikv/src/lib.rs:539-603`, incl. the
  `get_for_update` re-check, is not touched). Therefore **no perturbation of the real
  `TikvMetadataStore::commit` ordering can flip any at-Check artifact** — the very
  acceptance-test iter-8 named. Whatever red→green `run-verify.sh` shows is a flip of a pure
  oracle (e.g. `quorum`/`converged_exactly_once`) or of test-file presence, not the behavioural
  production flip the brief's Success-criterion §1 demanded. That is the ratified Option-B
  posture, not a defect — but the confirmatory phrasing in `check-gates.json` should not let a
  human read it as "a production commit defect would be caught at Check." It would not.

## Attempted refutations that did not land

- Tried to break the seed's `committed == 1 / conflicted == 1` assertion
  (`tikv_await_commit_interleaving.rs`) as schedule-dependent — could not: redb serialises
  commits, so the stale-`prior` writer is deterministically `Conflict` under every madsim seed.
  Its scope is honestly labelled redb-only, so no over-claim to refute there.
- Tried to find a compile hole in the feature-gated tier tests — could not: `WriteBatch::new/
  require/require_absent/put/delete` (`crates/traits/src/lib.rs:385-416`) and the two-variant
  `CommitOutcome` (`:355-360`, match is exhaustive) all exist, so `cargo check --features tikv
  --tests` genuinely type-checks the scenario (that half of iter-9's fix does work).
- Tried to show the live Tier-1 fault-effect oracle can pass on a no-op partition — could not at
  the pure-logic level: `consistency_passes` requires `fault_materialized`, which requires
  `partition_took_effect(before, during)` observed from PD's side; a receive-only/no-op cut
  leaves PD `Up` and fails it. (The live leg remains off-Check and human-adjudicated; a region
  leader pinned on the isolated voter could still make the rename leg flaky-RED, not falsely
  GREEN.)
