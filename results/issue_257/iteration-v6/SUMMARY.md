# Result — issue 257 / m4.6-tier1-scenario-tier2

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: three layers — one BINDING and demonstrable **at Check**, one BINDING
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: extend the realism-ladder **Tier-1** (integration + consistency-over-the-swap,

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan test-evidence slice behind
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo fmt --all -- --check` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 257 / m4.6-tier1-scenario-tier2

**Task under review:** Extend the realism-ladder Tier-1 (integration + consistency-over-the-swap, as an in-repo Rust scenario per ADR-0039) and Tier-2 lines across the redb→TiKV metadata backend swap, and author the first compounding-loop DST seed exposing a TiKV-shaped `await`-inside-`commit` interleaving — the at-Check binding red→green being the DST seed plus the pure `xtask` dispatch / fault-effect-oracle / `testkit` quorum-arithmetic tests; the live TiKV legs are pre-declared off-Check.

> **Grounding note.** `$PDCA_TARGET` could not be resolved from this review environment (no target worktree present; env expansion unavailable), so every citation below is grounded against `patch.diff` alone, per protocol. This is a target-state caveat, not a patch defect — no citation depends on the target's base state.

> **Advisory.** This review annotates; it does not gate. The one gating, deterministic block is the `cargo xtask ci` result recorded in `check-gates.json` (C4-ci FAIL on `cargo fmt --all -- --check`), which stands on its own regardless of the verdicts below.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch delivers exactly the brief's named artifacts: the await-inside-commit DST seed (`crates/dst/tests/tikv_await_commit_interleaving.rs`), the pure dispatch + fault-effect oracle (`xtask/src/metadata_faults.rs`), the `testkit` quorum/reachability seam (`crates/testkit/src/lib.rs` `partition_materialized`/`MetadataQuorumPlan`), the in-repo scenario tier tests and the ≥3-replica compose (`deploy/metadata-3replica/docker-compose.yml`). No literal-Jepsen re-attempt (ADR-0039 honoured). Decision hangs on nothing here — scope matches PR-sequence item 6. |
| C2 Reproduction (red pre-fix) | PASS | `check-gates.json` C4-verify PASS ("red without the fix, green with it"), and the red is now behavioural, not compile-absence: `CommitMode::PrewriteTrust` admits a stale commit under the same seed while `AtomicCommit` admits none (`sim_tikv.rs:364-377`, asserted `tikv_await_commit_interleaving.rs:951-956` and runtime-free at `sim_tikv_await_commit.rs:658-680`). I trusted the harness re-run (sandbox blocks `cargo`). Whether that behavioural red is a *production*-grounded regression vs. a self-contained model is the sim-fidelity call carried at C5. |
| C3 Change | PASS | Diff is coherent and correctly scoped: the model is a real lib item (`crates/dst/src/lib.rs:28` `pub mod sim_tikv`), deps moved dev→normal for it (`crates/dst/Cargo.toml:35-37`). Invariant held — no edit to `crates/traits/src/lib.rs` and no edit to `crates/metadata-tikv/src` (only its `Cargo.toml` + `tests/`), so the trait stays byte-for-byte and the store is driven, not re-proven. |
| C4 Verification (red→green) | FAIL | **Gating.** `check-gates.json` C4-ci: `cargo xtask ci` fails at `cargo fmt --all -- --check` (exit 1) — a deterministic formatting-hygiene blocker in the ~2.5k added lines; I could not pinpoint the offending span (sandbox blocks `cargo fmt`) but the whole-tree gate cannot pass until it is reformatted. The per-fix flip itself is sound (C4-verify PASS), so this is a hygiene block, not an evidence-logic defect — but it blocks accept and must be cleared. |
| C5 Causal adequacy | NEEDS-HUMAN | The seed corrects a real root cause — the `concurrency.rs` "no `await` inside commit" rationale is now scoped to redb (`crates/dst/tests/concurrency.rs:475-487`) and the newly-reachable interleaving is exercised. Symptom-guard smell-test does **not** fire (the `#[cfg(feature="tikv")]` / env gates are test-environment skips mirroring existing tier tests, not a capability probe over a load-time side effect). **Decision owed:** accept sim-fidelity — that `SimTikvMetadataStore`'s AtomicCommit/PrewriteTrust dichotomy (`sim_tikv.rs:206-219`) faithfully captures the real TiKV percolator commit risk. DST cannot self-certify the model matches the backend it stands in for; a human must sign that off (pre-declared, iter-5 standing call). |
| T1 Structure | PASS | New units land in the right places: the host-independent decision logic isolated in `xtask/src/metadata_faults.rs` (registered `xtask/src/lib.rs:18`, dispatched `xtask/src/main.rs:2167-2168`), the fault seam in `testkit`, the privileged scenarios under `crates/metadata-tikv/tests/`, the compose outside the workspace under `deploy/` (ADR-0010). |
| T2 Shape | PASS | Mirrors established patterns: pure `metadata_consistency_route` modelled as a value with both alternatives representable like `faults::jepsen_dispatch` (`metadata_faults.rs:2259-2270`); oracle factored as a pure `metadata_leg_passes` (`metadata_faults.rs:2328-2334`); tier tests endpoint-gated exactly like the chunkstore-path siblings. |
| T3 Runtime | PASS | The at-Check flippable artifacts run (C4-verify PASS; `testkit`/`xtask` unit tests are ordinary `cargo test`). Live legs skip cleanly without endpoints (`tier1_metadata_consistency.rs:1108-1121`) by design and are exercised only in the off-Check privileged job — their live runtime is unverifiable here and folded into Validation. Note: whole-tree `cargo test` result is unconfirmed because ci halted at the fmt step. |
| T4 Contribution | PASS | The three iter-5 hollowness defects are addressed: (1) self-referential flip → behavioural `PrewriteTrust` vs `AtomicCommit` difference + a runtime-free hand-rolled-scheduler proof (`sim_tikv_await_commit.rs`); (2) vacuous assertion → the contract assertion is now genuinely violable (`prewrite_trust_admits_a_lost_update_under_the_interleaving`, `tikv_await_commit_interleaving.rs:964-983`); (3) live-runner false-green → `--features tikv` is wired (`metadata_faults.rs:2279-2292`) and the fault descriptor is injected and the verdict built from real probes (`xtask/src/faults.rs:1982-2022`), pinned by `in_repo_route_assembles_the_ignored_scenario_argv`. Caveat: `read_after_commit_holds` and `converged_once` both derive from a single `scenario.is_ok()` (`faults.rs:2017-2019`) rather than independent signals — acceptable (the scenario asserts each internally) but not orthogonal. |
| T5 Judgment | NEEDS-HUMAN | **Decision owed (architecture board):** does the metadata leg need a new metadata-specific ADR refinement? TiKV runs its own Raft consensus, so ADR-0039's `docker pause` ≡ partition equivalence (derived for dumb M3 D-servers) may not transfer. The patch uses an `iptables` DROP network partition of one replica's ports (`faults.rs:2107-2127`), not a process freeze — which partly answers the concern — but whether that suffices or needs its own ADR (vs. #399-style follow-up) is the board's call. Also owed: acceptance of the **static-endpoints reduced bar** until #365 lands L5 discovery. Both pre-declared; do NOT author an ADR unless directed. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed:** the BINDING live legs — Tier-1 integration + consistency (ADR-0015 contract under a materialised minority partition) + Tier-2 real-I/O green against a real ≥3-replica TiKV cluster — are observable **only** in the privileged CI/eval Tier job, never in this Check worktree. The human must (a) name who runs that job and confirm the legs actually land green under a provably-materialised fault, and (b) judge whether this slice genuinely de-risks the redb→TiKV metadata swap as intended. To exercise it yourself off-Check: `WYRD_TIER1=1 cargo xtask metadata-tier1` and `WYRD_TIER2=1 cargo xtask metadata-tier2` on a privileged Docker host (root + `iptables` + the `deploy/metadata-3replica/` stack); confirm the consistency leg goes red when the partition is a no-op (`fault_materialized == false`). |

### Advisory — adversary

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

### Advisory — codex

- xtask/src/faults.rs:761 only proves recovery by probing `127.0.0.1:20182`, even though the injected fault adds DROP rules for both `20162` and `20182` at xtask/src/faults.rs:572. If deletion of the client-port rule fails while the status-port rule is removed, `self_healed` is reported true at xtask/src/faults.rs:775 with leaked host state still blocking TiKV traffic.
- xtask/src/faults.rs:670 waits only for the PD port before starting the live tier body. The new compose file starts three TiKV stores, but there is no readiness/registration/replication wait for the store ports at deploy/metadata-3replica/docker-compose.yml:41, deploy/metadata-3replica/docker-compose.yml:53, and deploy/metadata-3replica/docker-compose.yml:65, so the privileged leg can race cluster formation and fail the fault-materialization or consistency assertions before the intended 3-replica Raft group exists.
- xtask/src/faults.rs:757 runs the consistency scenario while the partition guard is still active, and xtask/src/faults.rs:761 heals only after the scenario exits. That means the test's read/convergence assertions at crates/metadata-tikv/tests/tier1_metadata_consistency.rs:165 and crates/metadata-tikv/tests/tier1_metadata_consistency.rs:177 are not actually checked "across the heal"; they are checked before healing happens.
- NEEDS-HUMAN — xtask/src/faults.rs:628 intentionally runs the Tier-1 integration scenario with no fault, and crates/metadata-tikv/tests/tier1_metadata_integration.rs:57 only covers create/read/duplicate-create. The brief asks for multi-key create/rename/delete and all-or-nothing across the fault, so a human should decide whether this reduced integration leg is acceptable for sign-off or needs broader live coverage.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The seed corrects a real root cause — the `concurrency.rs` "no `await` inside commit" rationale is now scoped to redb (`crates/dst/tests/concurrency.rs:475-487`) and the newly-reachable interleaving is exercised. Symptom-guard smell-test does **not** fire (the `#[cfg(feature="tikv")]` / env gates are test-environment skips mirroring existing tier tests, not a capability probe over a load-time side effect). **Decision owed:** accept sim-fidelity — that `SimTikvMetadataStore`'s AtomicCommit/PrewriteTrust dichotomy (`sim_tikv.rs:206-219`) faithfully captures the real TiKV percolator commit risk. DST cannot self-certify the model matches the backend it stands in for; a human must sign that off (pre-declared, iter-5 standing call).
- [ ] T5 Judgment — **Decision owed (architecture board):** does the metadata leg need a new metadata-specific ADR refinement? TiKV runs its own Raft consensus, so ADR-0039's `docker pause` ≡ partition equivalence (derived for dumb M3 D-servers) may not transfer. The patch uses an `iptables` DROP network partition of one replica's ports (`faults.rs:2107-2127`), not a process freeze — which partly answers the concern — but whether that suffices or needs its own ADR (vs. #399-style follow-up) is the board's call. Also owed: acceptance of the **static-endpoints reduced bar** until #365 lands L5 discovery. Both pre-declared; do NOT author an ADR unless directed.
- [ ] Validation — fitness-to-purpose — **Decision owed:** the BINDING live legs — Tier-1 integration + consistency (ADR-0015 contract under a materialised minority partition) + Tier-2 real-I/O green against a real ≥3-replica TiKV cluster — are observable **only** in the privileged CI/eval Tier job, never in this Check worktree. The human must (a) name who runs that job and confirm the legs actually land green under a provably-materialised fault, and (b) judge whether this slice genuinely de-risks the redb→TiKV metadata swap as intended. To exercise it yourself off-Check: `WYRD_TIER1=1 cargo xtask metadata-tier1` and `WYRD_TIER2=1 cargo xtask metadata-tier2` on a privileged Docker host (root + `iptables` + the `deploy/metadata-3replica/` stack); confirm the consistency leg goes red when the partition is a no-op (`fault_materialized == false`).
- [ ] **The load-bearing "trait contract survives" assertion is a
- [ ] **Every remaining sub-assertion in `assert_contract_survived`
- [ ] **The at-Check "red→green" is a patch-authored `CommitMode`
- [ ] **Live consistency oracle: two "independent" ADR-0015 sub-checks
- [ ] **The injected partition is asymmetric — the exact shape
- [ ] xtask/src/faults.rs:628 intentionally runs the Tier-1 integration scenario with no fault, and crates/metadata-tikv/tests/tier1_metadata_integration.rs:57 only covers create/read/duplicate-create. The brief asks for multi-key create/rename/delete and all-or-nothing across the fault, so a human should decide whether this reduced integration leg is acceptable for sign-off or needs broader live coverage.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo fmt --all -- --check` failed with exit status: 1

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Why rejected (plan-level): after six iterations the binding at-Check red->green is still vacuous, and the vacuity is STRUCTURAL — not a Do defect another rebuild can fix. The binding assertion asks "does the correct model survive the interleaving?" but correctness is encoded as a mode flag whose "correct" branch hard-codes `admit = commit_point_ok` (sim_tikv.rs:366), so the stale-commit oracle reduces to `admitted_stale = commit_point_ok && !commit_point_ok` (sim_tikv.rs:374) — identically false by boolean algebra, before any schedule runs. The builder even documents this (patch lines 371-372). The v6 PrewriteTrust "negative control" only proves a deliberately-broken, patch-authored branch is broken; it does not give the positive binding claim teeth. A self-authored DST sim whose correctness is a "good branch" is structurally incapable of a non-vacuous BINDING assertion about its own correct path ("the branch I wrote to be correct is correct" is always a tautology). Do has now re-instantiated this same shape five times; the fix is to redefine WHAT the binding at-Check evidence is — a Plan decision. What the re-plan must decide (redefine the binding evidence, don't ask Do to retry): 1. Separate the correctness from the check so an INDEPENDENT system can genuinely fail. Two candidate directions for the plan to choose between: (a) Drive the REAL production commit path (metadata-tikv/src CAS, or wyrd_core::write) under the interleaving, so the assertion checks code written without knowledge of the test — a real missing commit-point re-check then produces a real lost update the oracle catches; or (b) Make the sim's commit logic GENERAL (no correct-vs-broken mode flag) and let the schedule itself determine whether a stale commit slips through, so a subtly-wrong scheduling model can leak one and the assertion has teeth. 2. Re-decide whether the binding bar for this slice belongs at Check at all, or moves to the live Tier-1/Tier-2 legs (currently pre-declared off-Check), given that the at-Check DST seed cannot self-certify sim-fidelity. 3. Fold the standing Plan-level calls into the re-brief: C5 sim-fidelity acceptance; the ADR-0039 partition-of-a-live-Raft-node methodology question (the concrete mechanism ships an ASYMMETRIC iptables DROP, faults.rs:2107-2109 — the shape Invariant B forbids); static-endpoints reduced bar until #365. Trivial/mechanical (note for whoever rebuilds, but NOT the reason for rejection): - C4-ci is red only on `cargo fmt --all -- --check` at one over-length match arm (crates/dst/tests/sim_tikv_await_commit.rs:84). A `cargo fmt --all` clears it. Keep (held under adversarial probing, do not discard in the re-plan): - testkit quorum arithmetic and partition_materialized are genuine non-tautological oracles. - iter-5 --features tikv + fault-descriptor-injection false-green is genuinely fixed. Also carry (live-runner issues to address once the binding evidence is redefined): - Live consistency oracle collapses read_after_commit_holds and converged_once into one scenario.is_ok() exit bit (faults.rs:771,773) — restore independent signals. - Heal probes only 20182 though the fault DROPs 20162+20182 (faults.rs:761); no store-port readiness wait (races cluster formation); assertions checked before heal. - Reduced integration leg: only create/read/duplicate-create; brief asks multi-key create/rename/delete all-or-nothing across the fault.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
