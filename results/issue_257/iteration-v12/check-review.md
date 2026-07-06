# Check review — issue_257 / m4.6-real-commit-over-madsim-tikv (iteration 12)

**Task under review:** an accepted-plan *test-evidence* slice (not a bug fix). Extend the
realism-ladder testing across the redb→TiKV metadata swap behind the unchanged `MetadataStore`
trait: (i) at-Check pure oracles (quorum/fault-effect arithmetic + xtask dispatch) and an
honestly-labelled coverage seed, and (ii) off-Check live Tier-1 (consistency-over-the-swap under a
symmetric partition) + Tier-2 legs against a real ≥3-replica TiKV cluster. This is the 12th attempt;
the ratified posture is **Option B** (the third-party `madsim-tikv-client` sim genuinely does not
exist, so the binding correctness proof lives off-Check). iteration-11 was rejected because the live
Tier-1 fault-effect oracle keyed off PD's administrative `state_name` (never flips in a ~45s window),
making the leg unpassable; this iteration replaces it with a `last_heartbeat`-freshness oracle.

**Grounding:** target `$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt` is readable with the patch
applied (new files timestamped 00:56; not stale) — the gating C4-ci failure is a real result on the
patched tree, not a stale-target artifact. `cargo` is approval-blocked in this review sandbox, so I
could not independently re-run the gates; I verified the patch's new default-feature tests by
inspection and trust the deterministic gate for the whole-tree run.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Spec is exhaustively pinned by the brief (proposal 0015 item 6 / ADR-0039, Option-B posture, Invariant B). The patch's shape — pure at-Check oracles + honest coverage seed + deferred live legs — matches the ratified success criterion. No spec ambiguity owed. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Pre-declared MIXED posture (brief §"Verification posture"). At-Check the "red" is negation of the pure oracles (deterministic, `crates/testkit/src/lib.rs`), which I confirmed is non-tautological (hand-computed quorum table, not the returned literal). The *binding behavioural* red — a real cut vs a no-op negative control flipping `fault_materialized` — is only observable on a live cluster (off-Check). Human/privileged-job must confirm both directions on the real cluster. |
| C3 Change | PASS | Change is correctly scoped and holds the load-bearing invariants: the diff touches only `Cargo.{toml,lock}`, `crates/testkit/src`, new test files, `xtask`, and `deploy/` — **no** `crates/metadata-tikv/src` and **no** `crates/traits` edit (verified against `patch.diff` and the target tree). `wyrd-testkit` added as a **dev**-dependency only (`crates/metadata-tikv/Cargo.toml:46`). |
| C4 Verification (red→green) | FAIL | GATING gate red: `check-gates.json` C4-ci — `cargo test --workspace --exclude wyrd-dst` exits 101 on the patched tree (`./engine/xtask.sh ci`). This blocks accept regardless of the advisory rows below. I could not localize it in-sandbox (cargo approval-blocked); the patch's own new default-feature tests (testkit oracles, xtask `run_ci_steps` wiring, tikv-off skeletons) are correct/compile-clean by inspection, so the failing target is **not** obviously the new code — the builder must surface the exact failing target so the human can judge whether it is patch-induced or a pre-existing/flaky whole-tree test. (Non-gating C4-verify per-fix red→green is recorded PASS, but the gating whole-tree run is red.) |
| C5 Causal adequacy | NEEDS-HUMAN | This slice's entire raison d'être is contested evidence-soundness (11 prior rejections). Symptom-guard smell-test does NOT fire — no capability probe/runtime guard is added to production code (`metadata-tikv/src` is byte-unchanged). The open causal question is empirical: does PD v8.5.1's `last_heartbeat` (the iter-11 replacement for `state_name`, `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) actually go stale within the scenario window under a real symmetric cut, so `partition_took_effect` is RED on a no-op and GREEN on a real cut? Unverifiable at Check; human/privileged-job must confirm. |
| T1 Structure | PASS | New code sits in the sanctioned places and mirrors precedent: pure seam+oracles in `crates/testkit/src/lib.rs`, pure dispatch in `xtask/src/metadata_faults.rs` (mirrors `faults.rs::jepsen_dispatch`), live scenarios in `crates/metadata-tikv/tests/tier1_*`/`tier2_*` (mirror `chunkstore-grpc/tests/tier1_*`), `deploy/tikv-multi-replica/`. Invariant (no src/traits) held. |
| T2 Shape | PASS | Tests are non-tautological: quorum asserts a hand-computed `⌊n/2⌋+1` table; `ci_type_checks_feature_gated_metadata_scenario` drives real `run_ci_steps` wiring (not the iter-10 restated constant) and checks both toolchain-present emits the tikv check and toolchain-absent does not (gate-honesty). The flagship seed `crates/dst/tests/tikv_await_commit_interleaving.rs` takes ratified exit-(b): docstring explicitly concedes "pure redb coverage / NO correctness weight / no newly-reachable interleaving." Honest labelling. |
| T3 Runtime | NEEDS-HUMAN | Cannot confirm the new at-Check tests run **green in-gate**: the whole-tree `cargo test` is red (C4-ci) and cargo is approval-blocked here, so I verified them only by inspection. Once the C4-ci failure is localized/resolved, confirm the testkit + xtask tests and the `--cfg madsim` seed actually execute green. |
| T4 Contribution | PASS | The at-Check contribution is genuine though modest: independent quorum/heartbeat/heal oracles + xtask routing dispatch that go red under negation, plus an honestly-scoped redb coverage seed. The *binding correctness* contribution is deferred off-Check (ratified Option B) — whether that reduced at-Check bar suffices is a T5/Validation judgment, not a defect here. |
| T5 Judgment | NEEDS-HUMAN | Judgment owed: is the ratified Option-B reduced at-Check bar (pure oracles + conceded-off-Check seed) plus the live-only binding legs acceptable for landing #257 in the earlier wave (before #258)? And is the iter-11 heartbeat-oracle fix sound enough to trust off-Check? Prior-art check: 11 rejected iterations were consulted; this attempt addresses iter-11's single must-fix (state_name→heartbeat), but the fix is unproven live. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human at sign-off must clear the pre-declared items: (a) privileged Tier-1/Tier-2 job (named owner) confirms the live legs land green on a real ≥3-replica cluster, both fault directions, with the new heartbeat oracle; (b) a `WYRD_TIKV_TOOLCHAIN=1 cargo xtask ci` run actually type-checks the `#[cfg(feature="tikv")]` bodies (iter-11 §6 item 5); (c) the metadata-nemesis ADR-refinement question (architecture board) and the #365 static-endpoints reduced bar; (d) the pre-1.0 `tikv-client` supply-chain audit. NONE of these override the gating C4-ci FAIL, which is the immediate blocker. |

## Decision-turning summary

1. **Blocker (deterministic, gating): C4-ci FAIL** — `cargo test --workspace --exclude wyrd-dst`
   exits 101 on the patched tree. The patch cannot be accepted until this is green. I could not
   localize it in-sandbox and the patch's own new default-feature tests are correct by inspection,
   so the builder must surface the exact failing target; the human should adjudicate patch-induced
   vs pre-existing/flaky.
2. **Everything else is advisory NEEDS-HUMAN** consistent with the pre-declared MIXED / Option-B
   posture: the binding correctness evidence is off-Check and the iter-11 heartbeat-oracle fix is
   plausible but unverified on a live cluster.
