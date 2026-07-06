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

# Check review — issue 257 / m4.6-tier1-scenario-tier2

**Task under review:** a test-evidence slice (accepted proposal 0015 PR-item 6 / ADR-0039) that
(a) authors the first compounding-loop DST seed modelling a TiKV-shaped *await-inside-commit*
interleaving that redb never exhibits, and (b) extends the realism-ladder Tier-1 (integration +
consistency-over-the-swap, as an in-repo Rust scenario) and Tier-2 lines across the redb→TiKV
metadata backend swap, each live leg carrying a fault-effect oracle. At-Check binding evidence is
the DST seed + pure `xtask`/`testkit` decision logic; the live TiKV legs are pre-declared
DEFERRED/off-Check.

**Grounding.** Target `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, patch **applied**
(sim_tikv.rs, metadata_faults.rs, deploy/metadata-3replica/ all present). Citations grounded on
that tree. `crates/traits/src/lib.rs` is untouched by the patch (Invariant held — verified). The
production write/read API the seed drives exists (`crates/core/src/write.rs:157-275`,
`read.rs:43,316`). Gate re-runs recorded in check-gates.json: C4-ci **pass**, C4-verify **pass**
(red→green). `cargo` re-run blocked by sandbox here; relied on recorded deterministic gate runs +
source grounding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief scope (dst seed + testkit fault-seam + xtask metadata runners + deploy compose + metadata-tikv tier tests; NOT traits/core/custodian/metadata-tikv/src) matches the diff exactly; trait byte-for-byte unchanged (`crates/traits/src/lib.rs:338-351`). Spec is unambiguous for this slice. |
| C2 Reproduction (red pre-fix) | PASS | Pre-declared MIXED-posture item. The red is the synchronous-commit assumption: `SimTikvMetadataStore::with_await_inside_commit(false)` makes `interleavings_observed==0`, asserted directly by `no_interleaving_reachable_under_synchronous_commit` (`crates/dst/tests/tikv_await_commit_interleaving.rs:686-697`); gate C4-verify recorded red→green. Red is shown via an in-file toggle rather than stash-the-patch, which is sound for a determinism-rationale test slice. |
| C3 Change | PASS | `SimTikvMetadataStore::commit` snapshots generation → `network_round_trip().await` → re-checks preconditions at the commit point (`crates/dst/src/sim_tikv.rs:275-325`), faithfully mirroring the redb in-transaction CAS (`crates/metadata-redb/src/lib.rs:72-95`). xtask/testkit changes mirror the existing `run_jepsen`/`jepsen_dispatch` patterns (`xtask/src/faults.rs`). Coherent, minimal, additive. |
| C4 Verification (red→green) | PASS | Deterministic gates: `xtask ci` green with no TiKV/privilege (C4-ci), per-fix red→green (C4-verify). At-Check flippable evidence (DST seed + pure oracle/dispatch/seam tests) is intact. Caveat (not a blocking FAIL): the DEFERRED live *consistency* leg cannot be exercised by the shipped runner — see T4/Validation. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause = the DST harness's "commit is internally synchronous, no `await` inside" rationale being unsound for a networked backend; the seed *removes/exposes* that gap (models the await-inside-commit interleaving) rather than guarding a capability — the C5 symptom-guard smell-test does **not** fire (feature/endpoint gating is standard, no capability probe over a present capability). Decision owed: confirm `SimTikvMetadataStore` is a faithful model of a real TiKV await-inside-commit and not a self-serving strawman that trivially flips — i.e. the interleaving it makes reachable is the one real TiKV exhibits. Pre-declared sign-off item. |
| T1 Structure | PASS | Tests sit in conventional homes and mirror siblings: madsim seed in `crates/dst/tests/`, pure decision tests in `xtask/tests/metadata_faults_orchestration.rs`, seam arithmetic in `crates/testkit/src/lib.rs` `#[cfg(test)]`, privileged legs in `crates/metadata-tikv/tests/tier1_*/tier2_*` (endpoint-gated like `tests/conformance.rs`). |
| T2 Shape | PASS | Non-tautological: oracle tests assert both directions (`metadata_faults_orchestration.rs:leg_fails_when_*`); dispatch test discriminates the *variant* (`false`→InRepoScenario, `true`→LiteralJepsenTool), so a default-flip regression goes red — not iter-1's assert-the-returned-literal. The DST seed's binding assertion is the materialisation oracle + contract-survival + backend-equivalence, explicitly **not** "exactly one winner" (Invariant honoured, `tikv_await_commit_interleaving.rs:601-654`). |
| T3 Runtime | PASS | Pure tests run under ordinary `cargo test --workspace` (inside `xtask ci`); the seed runs under `--cfg madsim` (`cargo xtask dst`); the privileged legs `#[ignore]` + endpoint-gate → clean skip, keeping the gate honest with no TiKV. Gates confirm green. |
| T4 Contribution | NEEDS-HUMAN | The at-Check tests add genuine, load-bearing coverage. But the shipped metadata runner injects **no** fault and exports **no** fault descriptor: `run_metadata_scenario` sets only `WYRD_TIKV_PD_ENDPOINTS` (`xtask/src/faults.rs`), whereas the consistency leg requires `WYRD_METADATA_FAULT_BEFORE/AFTER/REPLICAS/PARTITIONED` (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:778-815`) and skips without them — unlike the sibling `run_jepsen`, which does `docker pause`/`unpause` and exports `WYRD_TIER1_PARTITION_CONTAINER` (`faults.rs:211-212,266,314`). Decision owed: is the unwired fault-injection/reachability-probe glue an acceptable deferral, or does it undercut the "BUILT, not unbuilt" claim for the binding consistency-over-the-swap leg (which, as shipped, clean-skips even in the privileged job rather than asserting under a materialised fault)? |
| T5 Judgment | NEEDS-HUMAN | Methodology call reserved to the architecture board (brief §"Known NEEDS-HUMAN"): TiKV runs its own Raft, so `docker pause` ≡ network-partition (ADR-0039's equivalence for the *dumb* M3 D-servers) is not obviously sound for a live-but-partitioned Raft node — decide whether the metadata leg needs a metadata-specific ADR refinement (partition-of-a-live-node) or rides an additive follow-up (#399-style). Also confirm the in-repo Rust scenario adequately realises the consistency "Jepsen" leg (literal Jepsen deferred to #329). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off owed on: (1) accept the DST await-inside-commit seed + pure oracle/dispatch/seam tests as the binding on-Check red→green (the four prior iterations' missing evidence); (2) the DEFERRED privileged Tier-1 integration + consistency + Tier-2 legs — named CI/eval Tier job must confirm live green, **and** the T4 gap must be closed (the runner currently cannot drive the consistency leg under a fault); (3) the static-endpoints reduced bar until #365 (Deployment-prerequisite note); (4) the metadata-nemesis ADR question (T5). Fitness-to-purpose is human-only by construction. |

### Advisory — adversary

# Adversarial review — issue 257 / m4.6-tier1-scenario-tier2

Advisory only; never gates. Attacked the at-Check red→green (the DST seed + the pure
xtask/testkit unit logic). Grounded on target source at `$PDCA_TARGET`.

## The at-Check flip rests on a self-authored toggle, not a defect

- **NEEDS-HUMAN — the seed's only flippable oracle is a property of a fixture this patch
  authored, toggled by a constructor argument.** The load-bearing red→green is
  `interleavings_observed >= 1` (`crates/dst/tests/tikv_await_commit_interleaving.rs:244-249`).
  The "red under the uncorrected assumption" is produced by passing `false` to
  `SimTikvMetadataStore::with_await_inside_commit` (`crates/dst/src/sim_tikv.rs:168`), a
  parameter added in this same patch — not a pre-existing bug in `wyrd_core`,
  `metadata-redb`, or the `concurrency.rs` harness. Nothing in production code fails
  before the patch and passes after; the "flip" is a boolean the fixture reads about
  itself. This is close to the compile-level / self-referential flip shape that got v3
  rejected ("the on-Check green was a compile-level flip"). Whether a toggle-produced red
  inside brand-new test-only code satisfies "genuine flippable red→green" is the human's
  call. (The `run-verify.sh` "red without the fix" almost certainly reverts the whole new
  file — a file-absence red — which I could not inspect; worth confirming the red is
  behavioural, not "the test didn't exist yet.")

- **`assert_backend_equivalence` is vacuous — it can never fail for any reachable input.**
  `crates/dst/tests/tikv_await_commit_interleaving.rs:217-233` (called at :256) compares
  two runs of the **same** `SimTikvMetadataStore` CAS: await-on vs await-off. Both modes
  re-check preconditions against *current* state at the commit point
  (`crates/dst/src/sim_tikv.rs:238`) — there is **no** mode that "trusts the prewrite
  check," so the lost-update the doc-comment claims to guard ("a store that skipped the
  commit-point re-check would let a second stale writer commit") is not representable.
  Unlike `await_inside_commit`, there is no toggle for prewrite-trust. Both runs
  deterministically yield `committed_writers == 1` / `final_version == 2`, so the two
  `assert_eq!`s always hold. Success-criterion §1.1 item 3 ("No lost update — the swap is
  observationally equivalent") is therefore decorative: it proves nothing the code could
  violate.

- **The "contract survived" assertions test the model, not the production path, and are
  near-definitional.** `assert_contract_survived`
  (`crates/dst/tests/tikv_await_commit_interleaving.rs:178-215`) can fail only if the
  hand-written in-memory `SimTikvMetadataStore` CAS is itself buggy — and that store is
  authored in this patch. The real `metadata-tikv/src` CAS is out of scope and never
  exercised by the seed. `final_version == PRIOR_VERSION + committed_writers` (:210) is
  satisfied by the store's own coupled bookkeeping (generation and version bumped together
  inside the one guarded block), so it re-derives the exactly-one-winner atomicity DST
  already owns — the very thing the Invariant says the seed must not rest on. The seed
  *demonstrates* an interleaving; it does not *test* that any production abstraction
  matches TiKV.

## The "fault-effect oracle" the brief touts is unwired on the live path

- **`metadata_leg_passes` / `MetadataLegVerdict` are dead relative to every live leg.**
  `xtask/src/metadata_faults.rs:110` and `:136` are referenced **only** by
  `xtask/tests/metadata_faults_orchestration.rs` (confirmed by grep: no other consumer).
  No live tier test and no `run_metadata_tier1` path ever constructs a `MetadataLegVerdict`
  from real observations. The unit tests set the five struct fields by hand and assert
  `a && b && c && d && e` — i.e. they test boolean AND, a tautology in the iter-1 sense
  ("asserts the same literal the function returns"). The *actual* Invariant-B enforcement
  on the live path comes from `wyrd_testkit::partition_materialized`
  (`crates/testkit/src/lib.rs:415`) and `MetadataQuorumPlan::is_valid_minority_fault`
  (`:465`), which the consistency test does wire in
  (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs`). So the oracle the brief
  advertises as the Check-time flippable fault-effect guard is not the oracle doing the
  work; `metadata_leg_passes` is redundant scaffolding that could be deleted with no live
  behaviour change.

- **`metadata_consistency_route`'s `true` branch is dead outside the test.**
  `xtask/src/metadata_faults.rs:73` — the only production caller passes `false`
  (`xtask/src/faults.rs:592`); `true` is reached solely by the unit test at
  `metadata_faults_orchestration.rs:47`. The dispatch test thus guards a
  constant-returning function against a body edit only — marginal, and borderline the
  same "returns a literal the test asserts" shape flagged as the iter-1 defect. (It does
  mirror the accepted `jepsen_dispatch` pattern, so this is a weak point, not a blocker.)

## Attacked and could not refute

- The `testkit` quorum arithmetic (`crates/testkit/src/lib.rs`
  `retains_quorum`/`is_valid_minority_fault`) is sound across the boundaries I probed
  (replicas 2/3/4/5, partitioned 0..=replicas, saturating_sub guards underflow); no
  off-by-one found.
- `partition_materialized` (`:415`) is a genuine independent before/after oracle, not a
  tautology, and it *is* wired into the live consistency leg — the strongest artifact here.
- `no_interleaving_reachable_under_synchronous_commit` correctly stays `0`: with the await
  removed there is no `.await` between the generation snapshot and the commit-point lock,
  so madsim (single-threaded, yields only at awaits) cannot interleave. That red demo holds.

## Net

The privileged legs are pre-declared off-Check (C2/C4 sign-off items) — not attacked as
"unbuilt." The refutable weakness is in the **at-Check** evidence: the binding flip is a
fixture toggling a property of itself, its "no lost update"/"backend equivalence"
assertions cannot fail, and the advertised fault-effect oracle (`metadata_leg_passes`) is
unwired from any real observation. The seed clears the iter-1 bar (real mid-commit
interleaving now materialises) but the *bindingness* still leans on constructs the patch
authored to pass. Human should adjudicate whether that is the honest red→green the four
prior iterations lacked, or a more sophisticated restatement of the same hollow flip.

### Advisory — codex

- `xtask/src/faults.rs:659` runs the new metadata tier scenarios with `cargo test -p wyrd-metadata-tikv --test ...` but never enables `--features tikv`; because `wyrd-metadata-tikv` keeps the real backend behind the non-default `tikv` feature (`crates/metadata-tikv/Cargo.toml:19`), each live test takes its `#[cfg(not(feature = "tikv"))]` branch and returns success after printing a skip (`crates/metadata-tikv/tests/tier1_metadata_integration.rs:103`). An opted-in privileged `cargo xtask metadata-tier1`/`metadata-tier2` can therefore go green without connecting to TiKV or driving the production metadata path.
- `xtask/src/faults.rs:662` exports only `WYRD_TIKV_PD_ENDPOINTS` to the metadata scenarios; it never injects a minority fault or exports `WYRD_METADATA_FAULT_BEFORE` / `WYRD_METADATA_FAULT_AFTER` / quorum counts. The consistency test treats a missing descriptor as a skip (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:75`), so `run_metadata_tier1` can report the consistency leg passed even though no fault materialized and the ADR-0015-under-fault contract was not exercised.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Root cause = the DST harness's "commit is internally synchronous, no `await` inside" rationale being unsound for a networked backend; the seed *removes/exposes* that gap (models the await-inside-commit interleaving) rather than guarding a capability — the C5 symptom-guard smell-test does **not** fire (feature/endpoint gating is standard, no capability probe over a present capability). Decision owed: confirm `SimTikvMetadataStore` is a faithful model of a real TiKV await-inside-commit and not a self-serving strawman that trivially flips — i.e. the interleaving it makes reachable is the one real TiKV exhibits. Pre-declared sign-off item.
- [ ] T4 Contribution — The at-Check tests add genuine, load-bearing coverage. But the shipped metadata runner injects **no** fault and exports **no** fault descriptor: `run_metadata_scenario` sets only `WYRD_TIKV_PD_ENDPOINTS` (`xtask/src/faults.rs`), whereas the consistency leg requires `WYRD_METADATA_FAULT_BEFORE/AFTER/REPLICAS/PARTITIONED` (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:778-815`) and skips without them — unlike the sibling `run_jepsen`, which does `docker pause`/`unpause` and exports `WYRD_TIER1_PARTITION_CONTAINER` (`faults.rs:211-212,266,314`). Decision owed: is the unwired fault-injection/reachability-probe glue an acceptable deferral, or does it undercut the "BUILT, not unbuilt" claim for the binding consistency-over-the-swap leg (which, as shipped, clean-skips even in the privileged job rather than asserting under a materialised fault)?
- [ ] T5 Judgment — Methodology call reserved to the architecture board (brief §"Known NEEDS-HUMAN"): TiKV runs its own Raft, so `docker pause` ≡ network-partition (ADR-0039's equivalence for the *dumb* M3 D-servers) is not obviously sound for a live-but-partitioned Raft node — decide whether the metadata leg needs a metadata-specific ADR refinement (partition-of-a-live-node) or rides an additive follow-up (#399-style). Also confirm the in-repo Rust scenario adequately realises the consistency "Jepsen" leg (literal Jepsen deferred to #329).
- [ ] Validation — fitness-to-purpose — Human sign-off owed on: (1) accept the DST await-inside-commit seed + pure oracle/dispatch/seam tests as the binding on-Check red→green (the four prior iterations' missing evidence); (2) the DEFERRED privileged Tier-1 integration + consistency + Tier-2 legs — named CI/eval Tier job must confirm live green, **and** the T4 gap must be closed (the runner currently cannot drive the consistency leg under a fault); (3) the static-endpoints reduced bar until #365 (Deployment-prerequisite note); (4) the metadata-nemesis ADR question (T5). Fitness-to-purpose is human-only by construction.

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
- Iteration delta (if iterating): Rejected (iter-5): the at-Check evidence is real but its bindingness still leans on constructs the patch authored to pass, and the live runner can false-green. Fix all three before this counts as the honest red→green the four prior iterations lacked. (1) Self-referential flip. The load-bearing red→green (`interleavings_observed >= 1`, `crates/dst/tests/tikv_await_commit_interleaving.rs:244-249`) is produced by passing `false` to `SimTikvMetadataStore::with_await_inside_commit` (`crates/dst/src/sim_tikv.rs:168`) — a constructor arg added in this same patch. Nothing in production code (`wyrd_core`, `metadata-redb`, the `concurrency.rs` harness) fails before the patch and passes after; the "flip" is a boolean the fixture reads about itself (the v3-reject shape). Make the red behavioural against a real production/harness assumption, not a self-toggled fixture property; confirm the `run-verify.sh` red is behavioural, not mere file-absence of a brand-new test. (2) Vacuous assertions. `assert_backend_equivalence` (`tikv_await_commit_interleaving.rs:217-233`, called :256) compares await-on vs await-off runs of the SAME store — both re-check preconditions at the commit point, so no mode can produce a lost update; the "No lost update / observationally equivalent" criterion is decorative. `assert_contract_survived` (:178-215) can fail only if the hand-written in-patch CAS is buggy and re-derives atomicity the store's own coupled bookkeeping guarantees — the very thing the Invariant says the seed must not rest on. Either give these assertions a mode that can actually violate them (e.g. a prewrite-trust toggle that skips the commit-point re-check) or drop the decorative criteria; do not present near-definitional checks as binding fault-effect oracles. (3) Live runner false-greens (codex). `xtask/src/faults.rs:659` runs the tier tests with `cargo test -p wyrd-metadata-tikv --test ...` but never `--features tikv`, so each live test takes its `#[cfg(not(feature = "tikv"))]` skip branch and returns success without connecting to TiKV (`crates/metadata-tikv/tests/tier1_metadata_integration.rs:103`). And `faults.rs:662` exports only `WYRD_TIKV_PD_ENDPOINTS`, never injecting a minority fault or exporting `WYRD_METADATA_FAULT_BEFORE/AFTER/REPLICAS/PARTITIONED`, so the consistency leg treats the missing descriptor as a skip (`tier1_metadata_consistency.rs:75`) and `run_metadata_tier1` reports the consistency-over-the-swap leg passed with no fault materialized. Wire `--features tikv` and the fault-descriptor injection so the privileged job actually drives the binding leg under a materialised fault, and cannot report green on a clean skip. Also fold in the advertised-but-unwired `metadata_leg_passes`/`MetadataLegVerdict` oracle so it derives from real observations rather than hand-set struct fields. Not re-litigated here (standing human calls owed at the next Check, not blocking the rebuild): C5 sim-fidelity acceptance, the ADR-0039 partition-of-a-live-Raft-node methodology question (T5), the static-endpoints reduced bar until #365. The `partition_materialized` oracle (`crates/testkit/src/lib.rs:415`) and the quorum arithmetic held under adversarial probing — keep them.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
