# Result — issue 257 / m4.6-real-commit-over-madsim-tikv

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: three layers — one BINDING and demonstrable **at Check**, one BINDING but
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: (i) add the **`madsim-tikv-client` cfg-alias** to `crates/metadata-tikv/Cargo.toml`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan test-evidence slice behind Accepted
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
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

# Check review — issue 257 (M4.6 real-commit-over-madsim-tikv), **iteration 10**

**Task under review:** extend the realism-ladder Tier-1/Tier-2 lines across the redb→TiKV metadata
swap and author the DST determinism-gap seed — proving (or, under the ratified **Option-B** posture,
honestly conceding off-Check) that the production `TikvMetadataStore::commit` await-inside-commit
window upholds the ADR-0015 single-zone contract, **without editing `metadata-tikv/src` or
`traits`**. This is the 10th pass after nine rejections; the single ratified iter-9 ask was: *add a
`cargo check -p wyrd-metadata-tikv --features tikv --tests` step to `run_ci` so the `#[cfg(feature="tikv")]`
live-scenario bodies are actually compiled/type-checked at Check, and fix the two now-false
compile-at-Check claims* (seed docstring + brief line). Option B, exit-(b) seed relabelling, the
pure testkit oracles, the xtask dispatch, and the tier1/2 scenario rework were all **ratified — not
to be re-litigated**.

**Grounding note (target state).** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0` is the
**base** checkout of `feat/m4-production-metadata-backend`; the patch is not applied there (the new
files are absent — expected, this is base, not staleness). I grounded the pre-existing citations
(`crates/metadata-tikv/src/lib.rs`, `crates/dst/tests/concurrency.rs`, `deploy/tikv-single-node`) on
the target and the patch-added files on `patch.diff`. Both C4 gates are green in `check-gates.json`
(`C4-ci` pass, `C4-verify` pass); I did not re-run the full `cargo xtask ci` (it needs the `tikv`
feature + container toolchain), so C4 rows rest on the recorded gate results.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The narrow iter-9 spec is well-formed and grounded: add the feature-gated compile step + relabel the seed. Invariant surfaces (no `traits`, no `metadata-tikv/src`) are pinned in brief.md:149-152 and hold in the patch. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | At Check the only red→green is the pure-oracle negation (arithmetic mutation flips testkit/xtask unit tests — genuine, non-tautological). The **binding correctness reproduction** (a real TiKV commit-point regression → lost update) is *conceded off-Check* per Option B; the fault-effect-oracle "red when the fault is a no-op" is observable only in the privileged Tier job. Decision owed: the Tier-1/Tier-2 job must confirm the live legs actually reproduce red — it cannot be shown at Check. |
| C3 Change | PASS | Additive only: new DST seed, tier1/tier2 scenarios, testkit oracles, xtask dispatch+runners, deploy compose, one dev-dep. No `crates/traits` and no `crates/metadata-tikv/src` edit (target lib.rs:540-601 commit path is untouched) — the M4 thesis invariant (brief.md:149-152) holds byte-for-byte. |
| C4 Verification (red→green) | PASS | `check-gates.json` records `C4-ci` pass and `C4-verify` pass. The iter-9 gap is closed: `feature_gated_checks()` (xtask/src/main.rs, patch) adds `cargo check -p wyrd-metadata-tikv --features tikv --tests`, wired into `run_ci` and guarded by `ci_type_checks_feature_gated_metadata_scenario`, so the `#[cfg(feature="tikv")]` scenario bodies now compile in the whole-tree gate (green ⇒ they compile). Caveat: binding-correctness red→green is off-Check (ratified Option B), not a Check gate. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause is real and correctly diagnosed — `concurrency.rs:3-4` ("commit() internally synchronous, no await inside") is false for `TikvMetadataStore::commit`, which awaits between `get_for_update` (target lib.rs:560) and `txn.commit().await` (lib.rs:597). The C5 capability-probe smell-test does **not** fire (no `hasattr`/try-import guard in production src; src is untouched). What the human owes: accept that the fix's *binding* causal evidence lives off-Check and the at-Check layer is pure oracles + a coverage seed carrying no TiKV correctness weight — the contested symptom-vs-root-cause axis that drove nine rejections. |
| T1 Structure | PASS | Files sit where their siblings do: seed under `crates/dst/tests/`, scenarios under `crates/metadata-tikv/tests/`, pure dispatch in `xtask/src/metadata_faults.rs` mirroring `faults.rs::jepsen_dispatch`, oracles in `testkit`, compose under `deploy/` (outside the workspace, ADR-0010). |
| T2 Shape | PASS | Pure functions (`quorum`, `partition_outcome`, `partition_took_effect`, `heal_is_complete`, `consistency_passes`, `converged_exactly_once`) take independent inputs and return typed verdicts; `ConsistencySignals` keeps read-after-commit / converged-once / fault-materialized as separate fields (fixes the v6 collapsed-bit defect). Dispatch modelled as a two-variant enum, mirroring the sanctioned precedent. |
| T3 Runtime | PASS | Executed at Check: the redb seed runs under `cargo xtask ci → run_dst --cfg madsim`; the testkit + xtask dispatch unit tests run in `cargo test --workspace`; the tier scenarios are compiled (not run) via the new feature-gated check. Live scenarios `#[ignore]` + endpoint-gate → clean skip with no TiKV. Not committed-but-unexecuted. |
| T4 Contribution | PASS | The at-Check contribution is real and now enforced: a regression in the feature-gated live-scenario bodies flips the gate (the iter-9 hole), and mutating the quorum/oracle arithmetic flips the testkit unit tests with hand-computed expectations (not the literal returned). Honest scope: the seed's docstring disclaims all TiKV correctness weight, so it adds redb CAS-classification coverage only — no false teeth. |
| T5 Judgment | NEEDS-HUMAN | My re-derivation: the tautology that killed v1–v8 is **gone** — `tikv_await_commit_interleaving.rs` explicitly concedes "NO correctness weight for `TikvMetadataStore::commit`" and "a production regression cannot turn it red," satisfying iter-8 exit (b); the Option-B line's "newly-reachable interleaving" is conceded off-Check in the seed's own labelling. Decision owed: whether relabelling the flagship seed as pure coverage (rather than delivering an at-Check behavioural flip) is an acceptable final resolution for #257 after nine iterations — the sign-off axis the gate routes to human. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The entire binding correctness proof (ADR-0015 contract on real ≥3-replica TiKV under a symmetric partition, Tier-2 real-I/O) is exercised only by the privileged `WYRD_TIER1`/`WYRD_TIER2` Tier job, and the new `SymmetricPartition` (distinct-loopback-IP `-s/-d` bidirectional cut + PD-side peer oracle) can only be shown to genuinely isolate the node **live** — I cannot drive it here (no container host / privileged netns in this worktree). Human/Tier-job must confirm: (a) the live legs land green; (b) the partition provably isolates (PD loses the store's heartbeat, `partition_took_effect` true) and heals with no leaked host firewall state; (c) the reduced Option-B at-Check bar + static-endpoints (#365) posture is acceptable as the deliverable; (d) the metadata-nemesis ADR question routes to the architecture board (patch correctly mints no ADR). |

## Notes for the human (§6 candidates)

- **Off-Check binding legs (C2/C4/V, pre-declared).** Nothing at Check proves the redb→TiKV swap
  upholds ADR-0015; that evidence is entirely in the privileged Tier job. To exercise it a reviewer
  needs a Docker host: `cargo xtask metadata-tier1` (with `WYRD_TIER1=1`, stands up
  `deploy/tikv-multi-replica`, isolates `127.0.0.2` bidirectionally, asserts the independent
  signals across the heal) and `cargo xtask metadata-tier2` (`WYRD_TIER2=1`). Confirm both green and
  that the fault-effect oracle goes **red on a no-op cut**.
- **Symmetric-partition soundness (Invariant B, the iter-7 must-fix locus).** The mechanism is much
  improved over the v6/v7 one-way `--dport` cut — distinct loopback IPs let it drop `-s`/`-d` on
  INPUT+OUTPUT, and the oracle now reads PD's peer view rather than probing the dropped port. But
  "the node can neither send nor receive" is only *asserted* by the design; it must be **observed
  live** (PD reporting the store `Disconnected`). This is the exact axis that failed twice — verify
  it in the Tier job, not from the diff.
- **C5 posture (ratified, not re-opened).** Option B is accepted because `madsim-tikv-client` does
  not exist and `TikvMetadataStore` holds a concrete `TransactionClient` (target lib.rs:421) built
  only via `connect()` (lib.rs:435-436), so an at-Check third-party-sim flip would require editing
  `metadata-tikv/src` — forbidden. Do not re-open; the human decision is only whether the resulting
  reduced at-Check bar is acceptable.

### Advisory — adversary

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

### Advisory — codex

- NEEDS-HUMAN — `crates/dst/tests/tikv_await_commit_interleaving.rs:35` says the new Check-time DST seed runs only over redb, has no TiKV await-inside-commit interleaving, and makes no TiKV claim. That is honest, but it appears to leave the brief's Option-B at-Check artifact ("concurrency.rs rationale is unsound; here is a newly-reachable interleaving") unimplemented; sign-off should decide whether moving that proof entirely to the off-Check Tier-1 job is acceptable.
- NEEDS-HUMAN — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:171` heals the partition before the read-after-commit/no-stale-read assertions at `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:193`. If the intended ADR-0015 signal must be observed while the minority partition is still active, this can miss a backend that commits on the majority side but only becomes readable after heal.
- `crates/testkit/src/lib.rs:403` implements "exactly one version step" with `wrapping_add(1)`, so `u64::MAX -> 0` is treated as valid convergence. A stricter oracle such as `version_before.checked_add(1) == Some(version_after)` would avoid masking counter overflow in the reusable consistency helper.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — At Check the only red→green is the pure-oracle negation (arithmetic mutation flips testkit/xtask unit tests — genuine, non-tautological). The **binding correctness reproduction** (a real TiKV commit-point regression → lost update) is *conceded off-Check* per Option B; the fault-effect-oracle "red when the fault is a no-op" is observable only in the privileged Tier job. Decision owed: the Tier-1/Tier-2 job must confirm the live legs actually reproduce red — it cannot be shown at Check.
- [ ] C5 Causal adequacy — Root cause is real and correctly diagnosed — `concurrency.rs:3-4` ("commit() internally synchronous, no await inside") is false for `TikvMetadataStore::commit`, which awaits between `get_for_update` (target lib.rs:560) and `txn.commit().await` (lib.rs:597). The C5 capability-probe smell-test does **not** fire (no `hasattr`/try-import guard in production src; src is untouched). What the human owes: accept that the fix's *binding* causal evidence lives off-Check and the at-Check layer is pure oracles + a coverage seed carrying no TiKV correctness weight — the contested symptom-vs-root-cause axis that drove nine rejections.
- [ ] T5 Judgment — My re-derivation: the tautology that killed v1–v8 is **gone** — `tikv_await_commit_interleaving.rs` explicitly concedes "NO correctness weight for `TikvMetadataStore::commit`" and "a production regression cannot turn it red," satisfying iter-8 exit (b); the Option-B line's "newly-reachable interleaving" is conceded off-Check in the seed's own labelling. Decision owed: whether relabelling the flagship seed as pure coverage (rather than delivering an at-Check behavioural flip) is an acceptable final resolution for #257 after nine iterations — the sign-off axis the gate routes to human.
- [ ] Validation — fitness-to-purpose — The entire binding correctness proof (ADR-0015 contract on real ≥3-replica TiKV under a symmetric partition, Tier-2 real-I/O) is exercised only by the privileged `WYRD_TIER1`/`WYRD_TIER2` Tier job, and the new `SymmetricPartition` (distinct-loopback-IP `-s/-d` bidirectional cut + PD-side peer oracle) can only be shown to genuinely isolate the node **live** — I cannot drive it here (no container host / privileged netns in this worktree). Human/Tier-job must confirm: (a) the live legs land green; (b) the partition provably isolates (PD loses the store's heartbeat, `partition_took_effect` true) and heals with no leaked host firewall state; (c) the reduced Option-B at-Check bar + static-endpoints (#365) posture is acceptable as the deliverable; (d) the metadata-nemesis ADR question routes to the architecture board (patch correctly mints no ADR).
- [ ] `crates/dst/tests/tikv_await_commit_interleaving.rs:35` says the new Check-time DST seed runs only over redb, has no TiKV await-inside-commit interleaving, and makes no TiKV claim. That is honest, but it appears to leave the brief's Option-B at-Check artifact ("concurrency.rs rationale is unsound; here is a newly-reachable interleaving") unimplemented; sign-off should decide whether moving that proof entirely to the off-Check Tier-1 job is acceptable.
- [ ] `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:171` heals the partition before the read-after-commit/no-stale-read assertions at `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:193`. If the intended ADR-0015 signal must be observed while the minority partition is still active, this can miss a backend that commits on the majority side but only becomes readable after heal.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Everything ratified in iteration 9 stands (Option B, exit-(b) seed relabelling, pure testkit oracles, xtask dispatch, tier1/2 scenario rework) — §6 items 1,2,3,5 and the posture-part of 4 are not the reason for iterating. The rejection is narrowly the one piece of new-in-iter-10 code (the feature-gated compile step) and its guard. Fix exactly these two, change nothing else: 1. Gate-honesty regression (verified against target Cargo.toml:80-85). The new `feature_gated_checks()` step is wired UNCONDITIONALLY into `run_ci` (patch xtask/src/main.rs, the `for check in feature_gated_checks() { cargo(&check)?; }` loop). `cargo check -p wyrd-metadata-tikv --features tikv --tests` compiles the pre-1.0 grpcio-bearing `tikv-client` tree, contradicting the documented invariant that the default `cargo xtask ci` on a laptop/worktree with no TiKV "never compiles or audits this tree and stays green." The recorded C4-ci pass came from a toolchain-complete box and masked this. Fix: make the compile step conditional on toolchain/endpoint presence (e.g. a WYRD_TIKV_TOOLCHAIN / endpoint gate) so iter-9's type-check intent is preserved WITHOUT breaking the no-TiKV-CI invariant. Do not drop the step; gate it. 2. Tautology guard. `ci_type_checks_feature_gated_metadata_scenario` only asserts that `feature_gated_checks()` contains its own hard-coded literal; it never calls `run_ci` and never asserts `run_ci` iterates the function. Deleting the wiring loop leaves the test green — and it reinstates the "assert the literal the function returns" tautology shape that got the early iterations rejected. Fix: the test must actually exercise the run_ci wiring (assert run_ci invokes the feature-gated check), not restate the constant. No re-litigation of ratified posture; no reset of the bundle.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
