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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

### Advisory — adversary

# Adversarial review — issue_257 (iteration 12), advisory only

Refutation attempts against the patch, the red→green evidence, and the reviewer's posture.
Grounded on `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt`). I never gate.

## The fix — a concrete input that the "binding" correctness evidence cannot catch

- **NEEDS-HUMAN — The off-Check Tier-1 consistency leg — now the *sole* binding correctness
  evidence for the redb→TiKV swap under the ratified Option-B posture — has no teeth for the
  exact defect the slice exists to guard.** `run()` in
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:101-236` drives **strictly
  sequential, single-writer** commits (create → partition → rename → heal → read/version →
  delete); there is **no second concurrent writer** contending the version key across the
  partition window (no `spawn`/`join` anywhere in the file). The production defect this whole
  M4.6 thesis targets — a missing/mis-ordered commit-point re-check — lives in
  `crates/metadata-tikv/src/lib.rs:555-573`, the `get_for_update` precondition loop, which only
  produces a `Conflict` **under concurrent write-write contention**. With a single sequential
  writer the value never changes between the snapshot read and `txn.commit()`
  (`lib.rs:597`), so **deleting or weakening the re-check would leave every assertion in this
  leg green.** Compounding this, the leg isolates a **minority** voter (`WYRD_TIER1_ISOLATED=1`
  of 3, `faults.rs` runner), and the majority side of a linearizable Raft group stays writable
  regardless — this is *precisely* the "minority partition against a linearizable store never
  changes that outcome" hollow flip the brief forbids (`brief.md:139`, and the iter-1..4
  rejection). Concrete failing case: a hypothetical `metadata-tikv/src` regression that drops
  the `get_for_update` re-check passes `tier1_metadata_consistency` and `tier2_metadata_io`
  identically to correct code. The iter-8 acceptance test ("perturbing the get_for_update
  re-check must flip an at-Check artifact") is not met even *off-Check*.

- **NEEDS-HUMAN — At the unprivileged Check gate the entire live-scenario code path is neither
  compiled nor executed, so no at-Check artifact flips on a regression in it.** The feature-gated
  type-check `cargo check -p wyrd-metadata-tikv --features tikv --tests` is emitted **only when
  `WYRD_TIKV_TOOLCHAIN` is set** (`xtask/src/main.rs:846-847,887`). On the default at-Check run
  the `#[cfg(feature = "tikv")]` bodies — `SymmetricPartition`, its `Drop` heal, the PD-side
  heartbeat oracle, and the `partition_took_effect`/`heal_is_complete`/`consistency_passes`
  wiring — are **not built** (confirmed: `cargo test -p wyrd-metadata-tikv` with default features
  compiles only the skeleton and passes). The tier1 docstring's claim that the code is
  "type-checked … in the whole-tree gate" (`tier1_metadata_consistency.rs:280-286`) is true only
  on a privileged box. Net effect combined with the finding above: **there is currently zero
  compiled-or-executed evidence, at Check, for the whole live-scenario code path**, and the
  privileged run that would exercise it is itself unconfirmed (an owed NEEDS-HUMAN). A type error
  or logic regression in `SymmetricPartition` flips no Check artifact.

## The evidence — the gating verdict does not reproduce

- **NEEDS-HUMAN — The gating C4-ci failure does not reproduce at `$PDCA_TARGET`.**
  `check-gates.json` records `C4-ci` = **fail**, `cargo test --workspace --exclude wyrd-dst`
  exit 101 (the only gating row). On the target I ran the full gate steps: `cargo fmt --all --
  --check` (exit 0), `cargo clippy --workspace --exclude wyrd-dst --all-targets` (clean), and
  `cargo test --workspace --exclude wyrd-dst` (**exit 0, all green**, twice). So the blocking
  signal is either environmental/state-dependent or was produced on a differently-configured box
  (e.g. `WYRD_TIKV_TOOLCHAIN` set, which would additionally compile the pre-1.0 `tikv-client`
  tree and can fail on missing protoc/grpcio — a *different* failure than the recorded test-step
  101). A human should establish the real cause before trusting the gate in *either* direction:
  a non-reproducing red is as untrustworthy as a rationalized green.

## The verdict — where the posture may be over-credited

- The `C4-verify` "red→green" row is marked **pass** (advisory), but note the at-Check flip it
  demonstrates can only be one of the **pure arithmetic oracles** (`testkit` quorum/heartbeat
  functions) or the **redb coverage seed** — none of which is behavioural against
  `TikvMetadataStore::commit`. That is the *declared* Option-B posture, but combined with the two
  findings above it means the patch ships **no executed behavioural evidence, at Check or in a
  confirmed off-Check run, that a real commit-point regression is caught.** The reviewer's
  acceptance of Option-B as a *posture* is reasonable; treating the tier-1 leg as the correctness
  bar it "defers to" is not, until the concurrency/teeth gap above is closed or the human
  explicitly accepts that the ADR-0015 commit-point contract remains **unproven by any artifact
  in this slice**.

## Attempted refutations that did NOT stick (reported for signal)

- Attacked the fault-effect oracle for a false-green: `fault_materialized =
  partition_materialized(3,1) && partition_took_effect(before, during)`
  (`tier1_metadata_consistency.rs:145-160`). A broken/parse-failing oracle yields
  `connected_before = false` → `partition_took_effect = false` → leg **fails**, not passes. This
  path **fails safe**; could not turn it into a false-green.
- Attacked `parse_store_last_heartbeat` / `heartbeat_is_fresh` (`testkit/src/lib.rs:1089-1125`)
  for a wrong-store or threshold false-positive; for the fixed `deploy/tikv-multi-replica`
  topology (distinct loopback IPs, unique `127.0.0.2` target) the substring match and strict-`<`
  age threshold are correct, and the unit tests use hand-computed expectations (non-tautological).
  Could not refute these as arithmetic.
- Attacked `ci_type_checks_feature_gated_metadata_scenario` (`xtask/src/main.rs:1130-1168`) as an
  iter-10-style tautology; it now genuinely drives `run_ci_steps` with a recording executor and
  would flip if the wiring loop or the toolchain gate were removed. Could not refute (the *gap*
  is that the gated step is off by default — see finding 2 — not that the test is hollow).

### Advisory — codex

- NEEDS-HUMAN — crates/dst/tests/tikv_await_commit_interleaving.rs:7 says the new DST seed is pure redb coverage with no TiKV correctness weight, and lines 35-38 state it exercises no newly reachable interleaving. That may be the honest Option-B posture, but it leaves the at-Check determinism-gap evidence below the brief’s requested fallback wording, so sign-off should explicitly decide whether the off-Check Tier-1 scenario plus pure oracles are sufficient.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Pre-declared MIXED posture (brief §"Verification posture"). At-Check the "red" is negation of the pure oracles (deterministic, `crates/testkit/src/lib.rs`), which I confirmed is non-tautological (hand-computed quorum table, not the returned literal). The *binding behavioural* red — a real cut vs a no-op negative control flipping `fault_materialized` — is only observable on a live cluster (off-Check). Human/privileged-job must confirm both directions on the real cluster.
- [ ] C5 Causal adequacy — This slice's entire raison d'être is contested evidence-soundness (11 prior rejections). Symptom-guard smell-test does NOT fire — no capability probe/runtime guard is added to production code (`metadata-tikv/src` is byte-unchanged). The open causal question is empirical: does PD v8.5.1's `last_heartbeat` (the iter-11 replacement for `state_name`, `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) actually go stale within the scenario window under a real symmetric cut, so `partition_took_effect` is RED on a no-op and GREEN on a real cut? Unverifiable at Check; human/privileged-job must confirm.
- [ ] T3 Runtime — Cannot confirm the new at-Check tests run **green in-gate**: the whole-tree `cargo test` is red (C4-ci) and cargo is approval-blocked here, so I verified them only by inspection. Once the C4-ci failure is localized/resolved, confirm the testkit + xtask tests and the `--cfg madsim` seed actually execute green.
- [ ] T5 Judgment — Judgment owed: is the ratified Option-B reduced at-Check bar (pure oracles + conceded-off-Check seed) plus the live-only binding legs acceptable for landing #257 in the earlier wave (before #258)? And is the iter-11 heartbeat-oracle fix sound enough to trust off-Check? Prior-art check: 11 rejected iterations were consulted; this attempt addresses iter-11's single must-fix (state_name→heartbeat), but the fix is unproven live.
- [ ] Validation — fitness-to-purpose — Human at sign-off must clear the pre-declared items: (a) privileged Tier-1/Tier-2 job (named owner) confirms the live legs land green on a real ≥3-replica cluster, both fault directions, with the new heartbeat oracle; (b) a `WYRD_TIKV_TOOLCHAIN=1 cargo xtask ci` run actually type-checks the `#[cfg(feature="tikv")]` bodies (iter-11 §6 item 5); (c) the metadata-nemesis ADR-refinement question (architecture board) and the #365 static-endpoints reduced bar; (d) the pre-1.0 `tikv-client` supply-chain audit. NONE of these override the gating C4-ci FAIL, which is the immediate blocker.
- [ ] crates/dst/tests/tikv_await_commit_interleaving.rs:7 says the new DST seed is pure redb coverage with no TiKV correctness weight, and lines 35-38 state it exercises no newly reachable interleaving. That may be the honest Option-B posture, but it leaves the at-Check determinism-gap evidence below the brief’s requested fallback wording, so sign-off should explicitly decide whether the off-Check Tier-1 scenario plus pure oracles are sufficient.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome:
- Iteration delta (if iterating):
- By / date:

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
