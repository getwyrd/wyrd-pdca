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

# Check review — issue 257 / m4.6-real-commit-over-madsim-tikv (iteration 9)

**Task under review:** under the ratified Option-B posture, land the redb→TiKV metadata-swap test evidence for M4.6 — an honestly-relabeled DST coverage seed (iteration-8's enforced exit (b)), pure testkit/xtask fault-seam oracles wired into the live scenario, Tier-1/Tier-2 metadata runners + a ≥3-replica TiKV compose stack — while fixing the two v8 codex advisories (lossy heal flag; missing cluster-readiness wait) and holding the no-`traits`/no-`metadata-tikv/src` invariants.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch touches exactly the declared surfaces (dst seed, testkit seam, xtask dispatch/runners, deploy/tikv-multi-replica, metadata-tikv tests); `crates/traits` and `crates/metadata-tikv/src` untouched — the only metadata-tikv edit is a `[dev-dependencies]` line (crates/metadata-tikv/Cargo.toml:46), so both byte-for-byte invariants hold. |
| C2 Reproduction (red pre-fix) | PASS | The evidence gap reproduces on the base: `concurrency.rs:3-4` claims commit has "no await inside" — false for `TikvMetadataStore::commit`, which awaits between the `get_for_update` re-check (crates/metadata-tikv/src/lib.rs:560) and `txn.commit()` (:597) — and `concurrency.rs:73-83` asserts winners-only, never loser classification; run-verify recorded red without the fix (check-gates.json C4-verify). |
| C3 Change | PASS | Change matches iteration-8's enforcement: the seed takes exit (b) — its docstring disclaims ALL correctness weight for TiKV and concedes the "newly-reachable interleaving" claim off-Check (crates/dst/tests/tikv_await_commit_interleaving.rs module docs); `heal()` sets `healed=true` only when every rule came out so `Drop` retries residue (v8 advisory 1); `wait_metadata_cluster_ready` gates PD + every store port inside the panic-safe closure (v8 advisory 2); the pure oracles are consumed by the live scenario, not dead code. |
| C4 Verification (red→green) | PASS | Both gates recorded green: gating `xtask ci` (fmt/clippy/build/test/deny/conformance) and non-gating run-verify "red without the fix, green with it" (check-gates.json). Caveat honestly stated: the sandbox denied my local `cargo` re-run, and the artifacts do not show WHICH perturbation run-verify used — given v3/v7 history (compile-flip reds), sign-off may ask the builder to name the red. The pure-oracle tests are statically sound: hand-computed quorum/heal expectations, not the returned literal (crates/testkit/src/lib.rs tests). |
| C5 Causal adequacy | NEEDS-HUMAN | By design (contested root-cause across 8 iterations) plus one re-derived live-run risk the human must carry into the Tier job: under `network_mode: host`, tikv-1's self-initiated connections to PD/peers will likely carry source 127.0.0.1 (kernel loopback source-selection), so the `-s 127.0.0.2` OUTPUT rules (tier1_metadata_consistency.rs `SymmetricPartition::rules`) may not match its outbound traffic — isolation could prove effectively one-way in practice. No false green is reachable (the PD-side oracle then leaves `fault_materialized=false` and `consistency_passes` fails red), but the first Tier run may be an honest red. Decision owed: accept the guarded-red posture, or require netns-based isolation before scheduling the privileged job. |
| T1 Structure | PASS | Layout mirrors the repo taxonomy: pure dispatch in xtask/src/metadata_faults.rs as the sibling of `jepsen_dispatch` (xtask/src/faults.rs:179); seam arithmetic in crates/testkit/src/lib.rs; compose stack under deploy/ outside the workspace (ADR-0010 shape); runners registered in xtask/src/main.rs dispatch + usage line. |
| T2 Shape | PASS | Shapes copy standing precedents: endpoint-gated `#[ignore]` tier tests mirror the chunkstore-grpc tier pair; the runner reuses `wait_for_port` / `finalize_panic_safe` / `finish_integration` (xtask/src/main.rs:341,416,443); both tier test files compile with and without `--features tikv` via the cfg-gated `run()` split. |
| T3 Runtime | PASS | Everything at-Check executes at Check: the seed runs via `xtask ci` → `run_dst` (`cargo test -p wyrd-dst` under `--cfg madsim`, xtask/src/main.rs:836-857 — the new tests/ target is included); testkit + xtask dispatch tests run in the workspace test; tier tests skip cleanly with no `WYRD_TIKV_PD_ENDPOINTS`. (Iter-7 sign-off already ratified this execution shape.) |
| T4 Contribution | PASS | Genuine increments over the tree: the loser-classification assertion `concurrency.rs` never had (winners-only at concurrency.rs:73-83 vs the seed's `conflicted == 1`); seam oracles wired into the live scenario (partition_took_effect / heal_is_complete / consistency_passes consumed in tier1_metadata_consistency.rs — the v7 "dead code" defect closed); tier runners + ≥3-replica stack are new. Prior art = the eight preserved iterations the brief enumerates per affected path; a mechanical git-history re-check was blocked by the sandbox — nothing in the tree contradicts the brief's account. |
| T5 Judgment | PASS | The enforced iteration-8 choice was taken cleanly and exactly: exit (b), with the Option-B "newly-reachable interleaving" line explicitly conceded off-Check **in the seed's labeling** — the literal alternative iteration-8's sign-off permitted ("or explicitly conceded as off-Check in the seed's labeling"); no ratified item (Option B, T3 shape, #258 ordering, no-ADR) is re-litigated. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | With Option B, ALL binding correctness evidence for the redb→TiKV swap lives in the deferred privileged job; at Check this slice certifies only pure arithmetic + an honestly-labeled redb seed. Decision owed: (1) run the Tier job and confirm green with `fault_materialized=true` — concretely, on a Docker host with iptables privileges: `WYRD_TIER1=1 cargo xtask metadata-tier1` and `WYRD_TIER2=1 cargo xtask metadata-tier2` (the runner stands up `deploy/tikv-multi-replica`, exports the isolation target, tears down on every path); (2) decide whether the reduced at-Check bar satisfies M4.6's purpose ("evidence the abstractions match the real store") before merge; (3) the brief's standing human items ride along: static-endpoints reduced bar (#365), metadata-nemesis ADR question (architecture board), tikv-client pre-1.0 Act flag. |

## Notes beyond the table

- **Target state:** `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`) is current — the patch is applied there and every cited `path:line` grounds; no staleness caveat.
- **What I could not re-run:** the sandbox denied `cargo test` / `git log` execution, so the green suites and the red→green flip are grounded on the recorded deterministic gates (both `pass`) plus static re-derivation, not a local re-run. The gating `xtask ci` gate is the stronger of the two and is green.
- **C5 finding in detail (for the Tier-job operator):** the bidirectional cut relies on matching tikv-1's traffic by IP 127.0.0.2 in both directions. Inbound (`-d 127.0.0.2`) is sound — peers/PD dial the advertised address. Outbound (`-s 127.0.0.2`) matches only if tikv-1 *binds* its outgoing sockets to 127.0.0.2, which TiKV does not generally do; loopback-routed connections will typically source from 127.0.0.1. If so, PD keeps receiving store heartbeats, `pd_still_sees_target_up_after(45s)` returns true, `partition_took_effect` → false, and the leg fails RED with `fault_materialized: false` — the honest outcome Invariant B mandates, but a red the operator should anticipate. A netns-per-node topology (or forcing TiKV's outbound bind) is the likely upgrade if it fires.
- **C5 symptom-guard smell-test:** no capability probe papering over a load-time cause. The endpoint-gating skips are the repo's established privileged-tier pattern (guarding code that by design only runs with a cluster present), and the `cfg(feature = "tikv")` split mirrors the existing conformance test — the smell-test does not fire.

### Advisory — adversary

# Adversarial review — issue 257 (iteration 9), advisory only

Posture note: iter-8 ratified Option-B and pre-authorized exit (b) for the flagship seed
(pure coverage + honest relabel). I did not re-litigate the posture; I attacked whether the
patch's *evidence* and the reviewer's *claims* actually hold under (b).

## Findings

- **NEEDS-HUMAN — The whole live-scenario body is `#[cfg(feature = "tikv")]`, which
  `cargo xtask ci` never compiles — so the "compiles + type-checks in the whole-tree gate"
  claim is false for the code that matters.** `run_ci` builds/tests `--workspace`
  with **no** `--features tikv` (`xtask/src/main.rs:805-819`: `clippy`/`build --all-targets`
  and `test --workspace`, all default-features), and `tikv` is off by default
  (`crates/metadata-tikv/Cargo.toml: default = []`). `--all-targets` selects target *kinds*,
  not features, so every `#[cfg(feature = "tikv")]` item in
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:79` onward — `SymmetricPartition`,
  `apply`/`heal`, the PD oracle `pd_store_state`, and the *entire* consumption of
  `partition_took_effect` / `heal_is_complete` / `consistency_passes` /
  `converged_exactly_once` — is excluded from the Check gate; only the
  `#[cfg(not(feature="tikv"))]` stub (`:511`) is compiled. The docstring's claim that
  `cargo test --workspace` "still **compiles and type-checks** it"
  (`tier1_metadata_consistency.rs:45-46`) and the brief's "scenario tests compile in the
  whole-tree gate" (brief.md:230) are therefore **unwarranted**. Concrete failing case: swap
  the argument order in the heal check to `heal_is_complete(&healed, &p.applied_rules(), ...)`
  (`:392`), or introduce any type error inside `SymmetricPartition` — `cargo xtask ci` stays
  **green**. The reviewer's C4-ci pass does not cover this code.

- **NEEDS-HUMAN — iter-7 must-fix-2 ("wire the pure oracles into the scenario, not dead
  code") is only *nominally* satisfied: at the Check boundary the oracles are still consumed
  by nothing but their own unit tests.** Their sole real consumer
  (`tier1_metadata_consistency.rs:127-137`, `:388-392`) sits behind `--features tikv`, which
  Check never builds (see above). So a regression that stopped calling
  `partition_took_effect` in the live leg, or re-pointed it, would flip **no** Check artifact
  — the exact "computed/wired, never applied at Check" shape the earlier iterations were
  rejected for, relocated one `cfg` deep.

- **NEEDS-HUMAN — The C4-verify "red without the fix, green with it" cannot be a behavioural
  flip against production commit code, because this patch adds no at-Check production
  behavioural surface.** The only Check-reachable code the patch adds is *pure arithmetic*
  (`crates/testkit/src/lib.rs:931-1033`) and *pure dispatch* (`xtask/src/metadata_faults.rs`);
  the redb seed explicitly disclaims correctness weight (below); the TiKV commit path
  (`crates/metadata-tikv/src/lib.rs:540-601`) is untouched and its scenario is off-Check.
  A red produced by deleting/mutating a just-added pure function is either a **compile-flip**
  (the v3 / iter-7 must-fix-5 rejection, brief.md:290) or proves only that the quorum/version
  arithmetic matches its own hand-computed table — **not** that the redb→TiKV swap upholds
  ADR-0015. run-verify.sh / build-notes.md are not in my inputs, so a human must confirm the
  recorded red was a genuine *assertion* failure and not the recurring compile-flip.

- **The flagship deliverable named by the brief does not exist at Check.** The slice's
  identity (brief.md:42-56, 194) is "a DST seed exercising the await-inside-commit
  interleaving **against the real `metadata-tikv` commit code**." Under (b) the seed
  (`crates/dst/tests/tikv_await_commit_interleaving.rs:26-32,58-71`) concedes it drives
  **redb only**, carries **no** correctness weight, exhibits **no** newly-reachable
  interleaving, and pushes the determinism-gap assertion off-Check. This is *allowed* by
  iter-8, so it is not a refutation — but the human at sign-off should confirm that "redb
  coverage + off-Check live legs + arithmetic" is accepted as "the end result the Success
  criterion names," since the on-Check behavioural proof the brief keeps demanding is, by
  construction, absent.

- **The redb seed's incremental value over `concurrency.rs` is thin and near-forced.** The
  seed (`tikv_await_commit_interleaving.rs:124`) is a 2-writer clone of
  `crates/dst/tests/concurrency.rs:35` whose only new assertion is `conflicted == 1`
  (`:179-185`). Given the seed already asserts `committed == 1` and `.unwrap()`s each outcome
  (so an `Err` panics rather than counting), and `CommitOutcome` has exactly two variants
  (`crates/traits/src/lib.rs:355`), `conflicted == 1` is arithmetically forced by
  `committed == 1` — it adds almost nothing a mutation could independently flip. Honest as
  coverage, but the reviewer should not credit it as meaningful new signal.

- **`partition_materialized` is inert in the live leg (redundant, not wrong).**
  `tier1_metadata_consistency.rs:136` calls
  `partition_materialized(p.total_replicas, p.isolated)` where the operands come from
  xtask-hardcoded env `WYRD_TIER1_REPLICAS=3` / `WYRD_TIER1_ISOLATED=1`
  (`xtask/src/faults.rs:1437-1438`), i.e. always `(3,1)` → always `true`. So the leg's entire
  fault-effect gate reduces to `partition_took_effect(connected_before, connected_during)`
  fed by a hand-rolled HTTP/1.0 + whitespace-strip + substring parse of PD's `/stores`
  (`pd_store_state`, `:465-489`). This fails *safe* (a parse miss → `connected_before=false`
  → leg fails), so it is not a false-green vector, but the reviewer should not read
  `partition_materialized`'s presence as an independent live check — it is a compile-time
  constant here.

## Could not refute
- Attempted to break the testkit pure oracles (`quorum`, `partition_outcome`,
  `converged_exactly_once`, `consistency_passes`, `heal_is_complete`,
  `partition_took_effect`) as tautologies — could not; each unit test asserts a
  hand-computed expectation distinct from the returned literal, and a boundary mutation
  (`total/2` vs `total/2+1`; `+1` vs `+2`) flips them red. These are genuine.
- Attempted to find a *false-green* path in the off-Check tier1 leg (a no-op partition that
  still passes) — could not; the fault-effect gate and heal checks fail safe. The residual
  risk is that none of that code is compiled at Check (finding 1), not that it passes
  vacuously when it does run.

### Advisory — codex

- NEEDS-HUMAN — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:136` treats the configured topology (`WYRD_TIER1_REPLICAS=3`, `WYRD_TIER1_ISOLATED=1`) plus PD losing the target store as proof that the test isolated a minority voter for the data being exercised, but the scenario only waits for store ports and never verifies the written region has a peer on `127.0.0.2` before applying the partition. If PD has not yet replicated the relevant region to that store, the create/rename/read-after-heal path can pass while isolating a non-voter for the tested keys, weakening the Tier-1 “minority voter” evidence described in `deploy/tikv-multi-replica/docker-compose.yml:11`.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — By design (contested root-cause across 8 iterations) plus one re-derived live-run risk the human must carry into the Tier job: under `network_mode: host`, tikv-1's self-initiated connections to PD/peers will likely carry source 127.0.0.1 (kernel loopback source-selection), so the `-s 127.0.0.2` OUTPUT rules (tier1_metadata_consistency.rs `SymmetricPartition::rules`) may not match its outbound traffic — isolation could prove effectively one-way in practice. No false green is reachable (the PD-side oracle then leaves `fault_materialized=false` and `consistency_passes` fails red), but the first Tier run may be an honest red. Decision owed: accept the guarded-red posture, or require netns-based isolation before scheduling the privileged job.
- [ ] Validation — fitness-to-purpose — With Option B, ALL binding correctness evidence for the redb→TiKV swap lives in the deferred privileged job; at Check this slice certifies only pure arithmetic + an honestly-labeled redb seed. Decision owed: (1) run the Tier job and confirm green with `fault_materialized=true` — concretely, on a Docker host with iptables privileges: `WYRD_TIER1=1 cargo xtask metadata-tier1` and `WYRD_TIER2=1 cargo xtask metadata-tier2` (the runner stands up `deploy/tikv-multi-replica`, exports the isolation target, tears down on every path); (2) decide whether the reduced at-Check bar satisfies M4.6's purpose ("evidence the abstractions match the real store") before merge; (3) the brief's standing human items ride along: static-endpoints reduced bar (#365), metadata-nemesis ADR question (architecture board), tikv-client pre-1.0 Act flag.
- [ ] `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:136` treats the configured topology (`WYRD_TIER1_REPLICAS=3`, `WYRD_TIER1_ISOLATED=1`) plus PD losing the target store as proof that the test isolated a minority voter for the data being exercised, but the scenario only waits for store ports and never verifies the written region has a peer on `127.0.0.2` before applying the partition. If PD has not yet replicated the relevant region to that store, the create/rename/read-after-heal path can pass while isolating a non-voter for the tested keys, weakening the Tier-1 “minority voter” evidence described in `deploy/tikv-multi-replica/docker-compose.yml:11`.

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
- Iteration delta (if iterating): Rejected on a confirmed claim/gate gap, NOT on the ratified Option-B posture (keep Option B; do not re-open it). Directive: make the live scenario type-check at Check. Add a `cargo check -p wyrd-metadata-tikv --features tikv --tests` step to `run_ci` in xtask/src/main.rs so the `#[cfg(feature = "tikv")]` scenario code (SymmetricPartition, its impl/Drop, the PD oracle, and the partition_took_effect/heal_is_complete/consistency_passes consumption in tier1_metadata_consistency.rs) is compiled and type-checked by the whole-tree gate. Why: run_ci currently builds/tests --workspace with default features (tikv off); --all-targets selects target kinds, not features, so none of the load-bearing scenario is compiled at Check today. Confirmed in the patched target: a type error inside SymmetricPartition leaves `xtask ci` green. This makes the docstring's "compiles and type-checks it" and brief.md:230's "scenario tests compile in the whole-tree gate" false, and leaves iter-7 must-fix-2 (oracles wired into the scenario, not dead code) only nominally satisfied — the oracles' sole real consumer sits behind a feature Check never builds, so a live-leg regression flips no Check artifact. Scope guard: this is a CI/gate change (xtask) plus fixing the two now-false compile-at-Check claims (seed docstring + brief line). It does NOT require editing crates/metadata-tikv/src or crates/traits (invariants hold) and does NOT re-litigate Option B, exit (b), the seed's off-Check labelling, or the off-Check Tier-1/Tier-2 legs.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
