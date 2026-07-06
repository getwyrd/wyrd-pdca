# Result — issue 257 / m4.6-tier1-jepsen-tier2

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan test-evidence slice behind
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

# Check review — issue 257 / m4.6-tier1-jepsen-tier2

**Task under review:** extend the realism-ladder **Tier-1 (integration + Jepsen)** and **Tier-2** test lines across the redb→TiKV **metadata backend swap** (M4.6, #257) — a real-TiKV fault seam in `testkit` (`SeededMetaFaults`/`MetaFault`), three `xtask` runners + a pure `meta_dispatch` core, new `#[ignore]`d tier test targets in `metadata-tikv`, and a committed DST-promotion seed for the compounding loop. The load-bearing tier green is DEFERRED/privileged-off-Check; the **Check-observable deliverable** is the pure dispatch/seam red→green. This is **iteration 2** — iteration 1 was rejected for three hollow evidence artifacts (tautological routing test, decorative DST seed, no-op Jepsen nemesis).

> Grounding note: `PDCA_TARGET` was not readable from this sandbox (env access blocked) and I did not wander into other checkouts, so citations are grounded on `patch.diff` + the driver-recorded gate results in `check-gates.json`. The C4-ci failure below is the driver's own deterministic gate run against the target, not my re-derivation.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Plan-pointer brief pins the Check-observable flippable (pure dispatch/seam tests) vs the deferred live-tier green; success criteria + invariants are concrete (brief.md:52-56, 149-152). Residual ambiguity (known-gap seed, #256 posture) is routed to the human rows below, not a spec defect. |
| C2 Reproduction (red pre-fix) | PASS | Check-observable red is demonstrated: the added integration tests bind `wyrd_testkit::SeededMetaFaults` (`meta_fault_seam.rs:699`) and `xtask::meta_dispatch::*` (`meta_dispatch_orchestration.rs:1276`), so reverting the production module makes them fail to resolve → red; C4-verify gate confirms red→green. Live-tier reproduction is privileged/off-Check (pre-declared, see V). |
| C3 Change | PASS | Blast radius matches the brief: `testkit` seam, `xtask` runners + `meta_dispatch`, `metadata-tikv` tier tests, a DST seed registry, Cargo.lock. The load-bearing invariant holds — `crates/traits/src/lib.rs` (`MetadataStore`, line 338) is **not** in the diff; no `core`/`custodian`/backend-logic edits. |
| C4 Verification (red→green) | **FAIL** | Gating gate C4-ci recorded FAIL: `cargo test --workspace --exclude wyrd-dst` **exit 101** (check-gates.json:37) — a test-phase panic, not a stale-target apply/compile issue, so not a fabricated ordering blocker. Most plausible source: `meta_dispatch_orchestration.rs:1413` (`pd_service_exists`) hard-reads `deploy/small-multi-node/docker-compose.yml` — the #256 deploy artifact the brief flags as possibly-not-landed (brief.md:162-165) — and panics/asserts when absent. Decision owed: the human must confirm whether #256 is on the base **and** require the patch to keep this pure Check test resilient to a missing deploy file, because the brief's gate-honesty invariant (brief.md:149-152) forbids `cargo xtask ci` failing without the privileged deps. C4-verify itself passed (check-gates.json:46). |
| C5 Causal adequacy | NEEDS-HUMAN | No symptom-guard smell: the `WYRD_TIKV_PD_ENDPOINTS`/`#[cfg(feature="tikv")]` gates mirror existing tier-test skip precedent, not a capability probe over a load-time side effect. The open root-cause question is the **compounding loop**: the "promoted regression" is now a documented *known-gap markdown registry* (`crates/dst/tests/tikv_surfaced_seeds.md`, SEED-0001 seed=17), not an executable DST regression — `PROMOTED_SEED=17` is data, never asserted/flipped. Decision owed: does a documented known-gap seed satisfy the mandatory DoD "at least one real-cluster discovery promoted to a committed DST seed" (brief.md:46-51, 137-141, 175-177), or must a live-cluster discovery drive it? |
| T1 Structure | PASS | Pure host-independent logic extracted into `xtask/src/meta_dispatch.rs` behind the lib target (patch.diff:1113-1246), privileged runners kept in `faults.rs`; mirrors the existing `disk_faults_orchestration` / `jepsen_dispatch` layering the brief cites. |
| T2 Shape | PASS | `MetaFault`/`SeededMetaFaults`/`MetaFaultInjector`/`MetaDispatch` mirror the established `NetFault`/`SeededNetFaults`/`JepsenDispatch` shapes (testkit/src/lib.rs:551-670); `quorum_safe_max = ⌊(n-1)/2⌋` is a clean, independently-checkable API. |
| T3 Runtime | **FAIL** | Ties to C4-ci: a non-`#[ignore]` test panics at runtime in the unprivileged workspace run (exit 101, check-gates.json:37). The `testkit` math tests are FS-free and pass; the failure is the FS-reading `meta_dispatch_orchestration.rs` test (Cargo.toml at :1291 / deploy compose at :1413). The gate does not stay green on a plain box — the brief's explicit runtime contract (brief.md:112) is currently violated. |
| T4 Contribution | PASS | Real wiring, not dead code: runners dispatched from `main.rs` (`meta-integration`/`meta-jepsen`/`meta-tier2`, patch.diff:1098-1100); the Jepsen leg now *actually* injects a nemesis (`docker compose pause`/`unpause` a quorum-safe PD minority on a background thread, faults.rs:973-983), fixing iteration-1's no-op leg; the routing test resolves against the real filesystem, fixing iteration-1's tautology. |
| T5 Judgment | NEEDS-HUMAN | Judgment calls the reviewer cannot settle from artifacts: (a) the reduced-bar #256/#365 static-endpoints posture (brief.md:81-83, 166-168); (b) whether the metadata Jepsen leg runs Jepsen-proper or the in-repo scenario harness — confirm-at-build (brief.md:186-188); (c) that the 5s nemesis window (faults.rs:979) genuinely overlaps the scenario load window on a live cluster. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The BINDING success criteria — all three tier legs green on real TiKV under real faults + one real-cluster surprise promoted to a committed DST seed (brief.md:35-51) — are observable ONLY in the privileged CI/eval Tier job, not at Check. Decision owed: the human must confirm the privileged Tier job's recorded run (fault schedule, history-check verdict, committed seed) actually demonstrates the metadata swap holds atomicity + the single-zone clause-2/exactly-one-winner clauses, and that Tier-2 real-I/O is green. |

## Notes for the human (what must clear before accept)

1. **Blocking, deterministic (C4-ci / T3):** the workspace test run fails (exit 101). Diagnose the failing `meta_dispatch_orchestration.rs` case: if it is the `deploy/small-multi-node/docker-compose.yml` read (`pd_service_exists`, patch.diff:1411-1417) panicking/asserting because #256 has not landed, the patch has coupled a *pure Check gate* to a not-yet-landed dependency — a gate-honesty violation independent of TiKV. The fix must make the on-Check dispatch/nemesis test skip or soft-pass when the deploy compose file is absent, while keeping the real routing/quorum-safety oracles. (Alternative causes to rule out from the gate log: `crates/metadata-tikv/Cargo.toml` name resolution at :1291.) This gates accept regardless of the advisory verdicts above.
2. **C5 — compounding-loop seed:** decide whether the documented known-gap `tikv_surfaced_seeds.md` (seed=17, `status: known-gap`, never asserted) meets the mandatory DoD, or whether it must await a real live-cluster discovery replayed by slice-7/#258.
3. **V — fitness:** confirm the off-Check privileged Tier job's recorded green + committed seed actually satisfy the binding criteria; the Check worktree cannot show it.
4. **T5 — scope/posture:** confirm the #256/#365 reduced-bar static-endpoints acceptance and the Jepsen tooling shape (in-repo scenario vs Jepsen-proper).

**Advisory disposition:** the on-Check evidence is materially stronger than iteration 1 (real filesystem oracle, real injected nemesis, honest known-gap seed). But the gating C4-ci failure stands — accept is blocked until the workspace test is green (a pure Check test must not panic on a missing deploy artifact), and the human clears the C5/T5/V rows.

### Advisory — adversary

# Check — adversarial (skeptic's) pass, #257 / m4.6-tier1-jepsen-tier2

Scope: this diff only. Grounded on the applied patch at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Advisory — no gate.

## Attacks on the evidence (red→green)

- **NEEDS-HUMAN — The gating C4-ci FAIL could not be reproduced, and I cannot explain it away.**
  `check-gates.json:37` records the *gating* check failing: `cargo test --workspace
  --exclude wyrd-dst` exited 101 (a panic). Yet re-running the exact command against the
  applied patch here is clean (exit 0, 86 `test result: ok`, zero failures). A green slice
  cannot rest on a whole-tree gate that flips between FAIL and PASS on identical inputs —
  either a flaky/nondeterministic test entered the tree with this diff, or the gate ran
  against a transient state. This must be adjudicated before accept: the deterministic gate
  says the slice is red. (One plausible latent trigger: if the privileged job's env leaks
  `WYRD_TIKV_PD_ENDPOINTS` into a plain `cargo test` run, `tier1_jepsen_metadata.rs:68`'s
  `nemesis_nodes().unwrap_or_else(|| panic!(…))` panics with exit 101 — but only if the
  `#[ignore]` is bypassed; worth confirming the gate's environment.)

- **The Check-observable red→green is a compile-existence proof, not a behavioral one.**
  Both flippables (`crates/testkit/tests/meta_fault_seam.rs`, `xtask/tests/meta_dispatch_
  orchestration.rs`) go RED under `run-verify` only because reverting the production hunk
  deletes a symbol (`SeededMetaFaults` / `xtask::meta_dispatch`) so `use …` fails to
  *compile*. That proves the module exists, not that its logic is right. It is redeemed
  only by the *content* oracles inside — see next two bullets for where those oracles still
  have gaps.

- **Attempted to refute the `quorum_safe_max` oracle — could not.**
  `crates/testkit/tests/meta_fault_seam.rs:706` asserts `survivors * 2 > n` for `n=1..=12`,
  and the `n/2`-regression genuinely flips it red at `n=4` (fault 2, leaves 2). That is a
  real, implementation-independent oracle over `SeededMetaFaults::quorum_safe_max`
  (`crates/testkit/src/lib.rs:609`). This artifact survives scrutiny.

## Attacks on the fix

- **NEEDS-HUMAN — The Jepsen nemesis faults the wrong tier: it pauses PD, but the metadata
  store is a *single* TiKV node.** `xtask/src/meta_dispatch.rs:95`
  (`META_JEPSEN_PD_NODES = 3`) and `:126` (`jepsen_nemesis_services` → `pd{i}`) draw a
  "quorum-safe minority" over the **PD** ensemble, and the runner
  (`xtask/src/faults.rs:716`) `docker compose pause`s only `pd*` services — never `tikv`.
  But `deploy/small-multi-node/docker-compose.yml:147` defines exactly **one** `tikv`
  service holding all metadata. Pausing a PD minority (majority PD survives → timestamp
  oracle + placement stay fully functional) creates **no data-plane partition** of the
  metadata store. Concrete consequence: `commit_point_linearizable_and_exactly_one_winner_
  under_partition` (`tier1_jepsen_metadata.rs`) runs 8 CAS racers against a healthy single
  TiKV — exactly-one-winner then holds *trivially* from local transactional CAS, proving
  nothing about consistency under partition. This is iteration-1's "passes without the
  required real fault" defect relocated, not fixed: a fault is injected, but against a tier
  whose disruption the consistency clauses do not depend on.

- **NEEDS-HUMAN — The nemesis and the load are not synchronized; the partition likely heals
  before the load runs.** `xtask/src/faults.rs:681` `run_meta_jepsen_with_nemesis` spawns a
  thread that pauses the PD minority, `sleep(Duration::from_secs(5))` (`:719`), then
  unpauses — while `run_meta_scenario_test` (`:725`) launches a **fresh `cargo test …
  --features tikv`** subprocess. That subprocess must compile the tier target (pulling
  `tikv-client`) and only then connect, seed, and drive the racers. The 5-second fault
  window is wall-clock from *thread spawn*, i.e. before cargo even starts; on any cold/warm
  compile it elapses before the CAS load executes, so the load hits a healed cluster. There
  is no barrier/handshake ensuring the load overlaps the pause. Even granting the wrong-tier
  point above, the fault window and the load do not deterministically coincide.

- **`MetaFault::Partition` is injected as a *process pause*, conflating two distinct faults.**
  The plan is drawn as `MetaFault::Partition` (`xtask/src/meta_dispatch.rs:127`) but applied
  via `docker compose pause` (`xtask/src/faults.rs:716`), which `SIGSTOP`s the container —
  the `MetaFault::Pause` mechanism per the seam's own doc (`crates/testkit/src/lib.rs:562`).
  A frozen-then-resumed PD ≠ a network partition (no split-brain, no asymmetric reachability).
  Mechanism identities are "ILLUSTRATIVE" per the brief, so this is advisory, not fatal — but
  combined with the two findings above, the "real partition + clock skew + process pause"
  binding condition (`ClockSkew` is defined at `lib.rs:566` but *never wired into any
  runner*) is not met by what the runner actually injects.

- **The routing test does not pin per-leg correctness — only "some real metadata target".**
  `xtask/tests/meta_dispatch_orchestration.rs:62` checks each leg's `--test` file *exists*
  and the package isn't the chunkstore crate, but never that Integration/Jepsen/Tier2 route
  to their *own distinct* scenarios. If `meta_dispatch(Jepsen).test` were mis-set to
  `"tier1_metadata_integration"` (a real file in the same crate), the test stays green — a
  leg-crossing bug passes. This is materially better than iteration-1's literal tautology
  (the filesystem is now an oracle), but the "routing resolves to the *right* target" claim
  is only half-proven.

## Attacks on the compounding-loop / seed

- **NEEDS-HUMAN — The mandatory "seeded regression promoted back into DST" is a Markdown doc,
  not a seed anything runs.** The binding Success criterion and the "compounding loop is
  mandatory, not optional" invariant require a *committed seeded regression*. The patch
  ships `crates/dst/tests/tikv_surfaced_seeds.md` — prose that `cargo` ignores by design
  (`:24`), status `known-gap` "NOT yet confirmed by a live Tier-1 run" (`:39`). `seed: 17`
  (`:38`) is asserted on by nothing (iteration-1's explicit remediation "PROMOTED_SEED=17
  must be asserted on" is now moot because there is no test at all). The file argues an
  in-slice DST regression is impossible without violating "DST keeps correctness authority."
  That argument may be sound — but whether a documented hypothesis satisfies the DoD bullet
  is exactly the pre-declared human call (brief Known-NEEDS-HUMAN #5). Flagging so it is not
  silently treated as "met."

## What I could not refute

- The trait invariant holds: `crates/traits/` is byte-untouched (git shows no change) — the
  M4 thesis "same system behind the unchanged trait" is not violated by this diff.
- The `quorum_safe_max` / `survivors*2>n` oracle and the seeded-selection reproducibility
  tests carry genuine, implementation-independent oracles and survive scrutiny.
- The off-Check gating posture (tier tests `#[ignore]` + clean skip when
  `WYRD_TIKV_PD_ENDPOINTS` unset) is legitimate and matches the existing tier precedents; my
  objection is not that the tier green is deferred, but that the *deferred* Jepsen leg, as
  wired, would not demonstrate the property it claims even when it does run (findings above).

### Advisory — codex

- `xtask/src/faults.rs:716` discards the result of `docker compose pause` (and line 721 does the same for `unpause`), so `meta-jepsen` can run the consistency test with only `WYRD_TIER1_NEMESIS_NODES` set even if the nemesis was never applied; this recreates the previous hollow-evidence failure mode where the scenario can pass without a real fault.
- NEEDS-HUMAN — `xtask/src/faults.rs:716` implements the selected `MetaFault::Partition` as `docker compose pause` of a PD process, not a network partition, latency, or clock-skew injection; human should decide whether this reduced process-pause-only nemesis satisfies the brief's Tier-1 Jepsen expectation for real partitions / clock skew / process pauses.
- NEEDS-HUMAN — `crates/dst/tests/tikv_surfaced_seeds.md:29` explicitly records a documented known-gap seed rather than a behavior surfaced by a live Tier-1 cluster run; the brief pre-declares this as a sign-off judgment on whether the compounding-loop DoD is satisfied.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — No symptom-guard smell: the `WYRD_TIKV_PD_ENDPOINTS`/`#[cfg(feature="tikv")]` gates mirror existing tier-test skip precedent, not a capability probe over a load-time side effect. The open root-cause question is the **compounding loop**: the "promoted regression" is now a documented *known-gap markdown registry* (`crates/dst/tests/tikv_surfaced_seeds.md`, SEED-0001 seed=17), not an executable DST regression — `PROMOTED_SEED=17` is data, never asserted/flipped. Decision owed: does a documented known-gap seed satisfy the mandatory DoD "at least one real-cluster discovery promoted to a committed DST seed" (brief.md:46-51, 137-141, 175-177), or must a live-cluster discovery drive it?
- [ ] T5 Judgment — Judgment calls the reviewer cannot settle from artifacts: (a) the reduced-bar #256/#365 static-endpoints posture (brief.md:81-83, 166-168); (b) whether the metadata Jepsen leg runs Jepsen-proper or the in-repo scenario harness — confirm-at-build (brief.md:186-188); (c) that the 5s nemesis window (faults.rs:979) genuinely overlaps the scenario load window on a live cluster.
- [ ] Validation — fitness-to-purpose — The BINDING success criteria — all three tier legs green on real TiKV under real faults + one real-cluster surprise promoted to a committed DST seed (brief.md:35-51) — are observable ONLY in the privileged CI/eval Tier job, not at Check. Decision owed: the human must confirm the privileged Tier job's recorded run (fault schedule, history-check verdict, committed seed) actually demonstrates the metadata swap holds atomicity + the single-zone clause-2/exactly-one-winner clauses, and that Tier-2 real-I/O is green.
- [ ] `xtask/src/faults.rs:716` implements the selected `MetaFault::Partition` as `docker compose pause` of a PD process, not a network partition, latency, or clock-skew injection; human should decide whether this reduced process-pause-only nemesis satisfies the brief's Tier-1 Jepsen expectation for real partitions / clock skew / process pauses.
- [ ] `crates/dst/tests/tikv_surfaced_seeds.md:29` explicitly records a documented known-gap seed rather than a behavior surfaced by a live Tier-1 cluster run; the brief pre-declares this as a sign-off judgment on whether the compounding-loop DoD is satisfied.
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
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected: gating gate is FAIL and the hollow-evidence pattern iteration 1 was rejected for is relocated, not fixed. This is iteration 2 — the rebuild must resolve the gating failure AND address the adversary remarks below, not shuffle them again. Gating (blocks accept, must resolve): - C4-ci FAIL: `cargo test --workspace --exclude wyrd-dst` exit 101 (panic). A pure Check test is coupled to a not-yet-landed dependency — most likely meta_dispatch_orchestration.rs:1413 hard-reading deploy/small-multi-node/ docker-compose.yml (#256 artifact) and panicking when absent. The adversary could NOT reproduce it (re-ran clean, exit 0), so the gate is flapping FAIL/PASS on identical inputs — either a flaky test entered the tree or the gate ran against a transient/leaked-env state (e.g. WYRD_TIKV_PD_ENDPOINTS leaking past #[ignore] -> tier1_jepsen_metadata.rs:68 panics). Make the on-Check dispatch/nemesis test skip or soft-pass when the deploy compose file is absent, and make the gate deterministic. brief.md:149-152 forbids `cargo xtask ci` failing without the privileged deps. Adversary remarks that must be addressed: - Wrong-tier nemesis: the Jepsen leg pauses a PD minority (meta_dispatch.rs:95,126; faults.rs:716) but the metadata store is a SINGLE TiKV node (docker-compose.yml:147). Majority PD survives -> no data-plane partition, so exactly-one-winner holds trivially from local CAS and proves nothing under partition. Fault the TiKV data plane, not PD. - Nemesis/load not synchronized: 5s fault window is wall-clock from thread spawn (faults.rs:719), before the fresh `cargo test --features tikv` subprocess compiles/ connects (faults.rs:725) — the load likely hits a healed cluster. Add a barrier/handshake so the load provably overlaps the fault. - Pause result discarded (codex): faults.rs:716/721 ignore the `docker compose pause`/`unpause` result, so meta-jepsen can pass with the nemesis never applied. Check the result and fail loudly if injection did not happen. - MetaFault::Partition applied as `docker compose pause` (SIGSTOP) conflates partition with process-pause; ClockSkew (lib.rs:566) is defined but never wired into any runner. Wire the real fault kinds the brief's Tier-1 Jepsen expectation names. - Routing test only checks each leg's file EXISTS (meta_dispatch_orchestration.rs:62) — a leg-crossing bug (Jepsen routed to the integration scenario) stays green. Pin each leg to its own distinct scenario. - Compounding-loop seed is a Markdown doc, not an executable seed: tikv_surfaced_seeds.md seed=17 status known-gap, asserted on by nothing. The mandatory DoD wants a committed executable DST regression from a real-cluster discovery — make it real or get explicit human sign-off that the known-gap doc satisfies the DoD (§6 C5 / brief Known-NEEDS-HUMAN #5). Still-open judgment/validation rows for the next sign-off (not corrections): - T5 posture/tooling (#256/#365 static-endpoints; Jepsen-proper vs in-repo harness), and Validation fitness of the off-Check privileged Tier job. Revisit at next Check.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
