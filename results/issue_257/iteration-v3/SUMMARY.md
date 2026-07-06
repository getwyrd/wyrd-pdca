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

# Check review — issue 257 / M4.6 (Tier-1 integration + Jepsen + Tier-2 across the redb→TiKV metadata swap)

**Task under review:** extend the realism-ladder Tier-1 (integration + Jepsen) and Tier-2 legs across the metadata backend swap (redb→TiKV, behind the *unchanged* `MetadataStore` trait) so the same system is proven correct on real TiKV under real faults, and promote at least one real-cluster discovery back into DST as a committed seed. The **Check-observable flippable** is the pure xtask runner dispatch/routing + the `testkit` real-TiKV fault-seam decision logic (RED when negated, GREEN on the tree); the load-bearing live tier green is pre-declared DEFERRED / privileged-off-Check.

**Iteration 3.** Iterations 1–2 were rejected on a gating C4 failure + hollow-evidence findings (tautological routing test, decorative DST seed, no real nemesis, wrong-tier fault, unsynchronized load, discarded compose results, non-deterministic ci panic). `check-gates.json` now reports **overall pass**, with both gating rows green: `C4-ci` PASS ("xtask ci: all checks passed") and `C4-verify` PASS ("red without the fix, green with it"). Nothing blocks accept mechanically; my advisory verdicts follow.

**Grounding caveat:** `$PDCA_TARGET` is unreachable in this sandbox and no target worktree is present, so citations ground on `patch.diff`; I could not re-run cargo myself and trust the driver's gate re-runs for the red→green claim.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Scope matches the brief's Check-observable clause (brief.md:52-56): a pure dispatch/seam red→green with the live tiers deferred. Patch delivers exactly that — `xtask/src/meta_dispatch.rs` (pure routing + nemesis plan) + `crates/testkit/src/lib.rs:637` `MetaFault`/`MetaClusterFaultPlan` seam, with live tiers gated off-Check. |
| C2 Reproduction (red pre-fix) | PASS | On-Check flippable reproduces red: the added integration tests `use xtask::meta_dispatch::…` / `use wyrd_testkit::MetaClusterFaultPlan` fail to resolve when the production module/seam is reverted (patch.diff:1524, meta_fault_seam.rs:860); C4-verify re-run confirms red→green. Note: this is a *structural* (module-presence) red, not behavioral — the load-bearing tier repro is DEFERRED/privileged (see Validation). |
| C3 Change | PASS | Coherent and scoped: net-new `testkit` seam, pure `meta_dispatch`, three xtask runners (faults.rs:1007-1052) with real `docker compose pause tikv` nemesis + handshake, three metadata-tikv tier targets, a DST seed registry. No diff hunk touches `crates/traits`, `core`, `custodian`, or metadata-backend logic — the "trait stays byte-for-byte unchanged" invariant (brief.md:129) holds. |
| C4 Verification (red→green) | PASS | Gating `C4-ci` and non-gating `C4-verify` both PASS in check-gates.json (the two rows iterations 1–2 failed). The seam test carries a genuinely *independent* oracle — `survivors*2 > n` (meta_fault_seam.rs:870) catches a majority-faulting regression (n=4→2 faulted fails), not a literal mirror. Could not re-run locally (no checkout); trusting the driver re-run. Residual: the tikv-feature tier bodies (using `futures_util::future::join_all`, tier1_jepsen_metadata.rs:201) are `#[cfg(feature="tikv")]` and no `crates/metadata-tikv/Cargo.toml` dev-dep/feature change is in the diff — the privileged `--features tikv` build is unverified at Check. |
| C5 Causal adequacy | NEEDS-HUMAN | Decision owed: does the **compounding-loop DoD** (a *committed executable* DST regression promoted from a real-cluster discovery) count as met by a **Markdown known-gap registry** (`crates/dst/tests/tikv_surfaced_seeds.md`, seed 17/29, status `known-gap`), which is data no test asserts on? The builder argues an executable seed here would either re-prove redb's own atomicity (the iteration-1 anti-pattern) or need forbidden backend edits; brief Known-NEEDS-HUMAN #5 explicitly routes this to the human. Impact: this is a mandatory DoD bullet, not optional — the human must accept the doc-as-hypothesis posture or require a live discovery. |
| T1 Structure | PASS | Files land where the design corpus and precedent put them: `testkit` seam beside `SeededStorageFaults`, pure `meta_dispatch` exposed via the xtask lib target and tested by `xtask/tests/meta_dispatch_orchestration.rs` (mirrors `disk_faults_orchestration.rs`), metadata-tikv tier targets beside the existing `conformance.rs`. |
| T2 Shape | PASS | APIs mirror the established seams (`MetaFaultInjector` parallels `NetFaultInjector`/`StorageFaultInjector`, testkit lib.rs:685; `MetaClusterFaultPlan::from_seed` parallels `SeededStorageFaults`); tier tests use the same `#[ignore]` + `WYRD_TIKV_PD_ENDPOINTS` skip guard as the chunkstore tier precedents. |
| T3 Runtime | PASS | Unprivileged runtime is honest: tests skip cleanly with no TiKV and `cargo xtask ci` stays green (C4-ci PASS), satisfying the gate-honesty invariant (brief.md:149-152) — the iteration-2 non-deterministic panic is resolved (orchestration test reads only patch-added files, meta_dispatch_orchestration.rs:1538-1541). Caveat noted under C4: the tikv-feature tier bodies' compile is exercised only in the off-Check privileged job. |
| T4 Contribution | PASS | Genuinely fills the gap the brief names — the pre-M4 tiers exercise the chunkstore repair path; these legs are net-new against the metadata swap, pinned away from the chunkstore crate by an explicit `assert_ne!(d.package, CHUNKSTORE_PACKAGE)` oracle (meta_dispatch_orchestration.rs:1599). No same-file conflict expected (brief.md:70). Prior-art check by affected path could not be mechanically settled here (no repo history) — folded into Validation. |
| T5 Judgment | NEEDS-HUMAN | Decision owed on the enumerated posture/tooling judgments: (a) the **reduced-bar static-endpoints** posture pending #256 (deploy stack) and #365 (L5 discovery) — accept as sufficient to prove the metadata risk? (brief.md:81-83,166-168); (b) **fault-kind fidelity** — the brief's Jepsen expectation names "partitions, clock skew, and process pauses," but only `Pause` is wired into the runner; `Partition`/`Latency`/`ClockSkew` are modeled in `MetaFault` (testkit lib.rs:638-657) yet never injected. The builder's rationale (single TiKV data node ⇒ pause is the honest single-replica nemesis) is defensible but is a judgment the human owns. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The **binding** Success criterion (brief.md:36-51) — the metadata swap holds atomicity + single-zone consistency (clause-2 linearizability, exactly-one-winner) on real TiKV under real faults, Tier-2 real-I/O green, and a real-store discovery promoted to a committed DST seed — is demonstrable ONLY in the privileged CI/eval Tier job (Docker + root + tc/iptables/cgroup/libfaketime + a TiKV cluster), not at Check. Decision owed: confirm that job's recorded run (fault schedule, history-check verdict, committed seed) actually landed green and that the reduced-bar posture satisfies the DoD. Runnable check for the human: on a privileged Docker host with the #256 stack, `WYRD_TIER1=1 cargo xtask meta-integration`, `WYRD_TIER1=1 WYRD_TIER1_SEED=<seed> cargo xtask meta-jepsen`, `WYRD_TIER2=1 cargo xtask meta-tier2` — expect exactly-one-winner + stable commit-point reads across the injected `docker compose pause tikv` window, and a non-vacuous "N/M rounds committed" recovery line. |

### Advisory — adversary

# Check — adversarial review (issue #257, iteration 3) — advisory, non-gating

Skeptic's pass over `patch.diff` / `brief.md` / `check-gates.json`, grounded on the target
source at `/home/eddie/wyrd/wyrd.pdca-wt-l0`. The two genuinely-independent oracles from
iteration 2 survived scrutiny; the load-bearing *behavioral* evidence still does not.

## Refutations a human must adjudicate

- **NEEDS-HUMAN — The Jepsen leg's headline property (`exactly-one-winner`) cannot fail on
  this topology, so binding criterion (b) is not demonstrable and verges on the forbidden
  re-proof.** `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:170` asserts
  `winners <= 1`, but the metadata store is a **single** TiKV node
  (`deploy/small-multi-node/docker-compose.yml:147` declares exactly one `tikv` service) and
  the injected fault is a `docker compose pause tikv` — a whole-node `SIGSTOP`, **not a
  partition**. A single serializing node can *never* admit two CAS winners regardless of
  concurrency or the pause: while paused every op `Err`s (skipped by the
  `continue` at `:181`), while up TiKV's Percolator txn serializes them. So `winners <= 1`
  is true by construction — it cannot go red. Brief binding condition (b) demands
  "exactly-one-winner **under genuine concurrency** / **real partition**"; this leg creates
  no partition and the property is unfalsifiable here. Worse, the only bug it *could* catch
  (a non-atomic CAS in `wyrd-metadata-tikv`) is precisely the atomicity the brief's invariant
  says "M4 must NOT re-prove … that is DST's job." The leg is caught between an impossible
  partition and a forbidden re-proof. Human must judge whether a single-node pause satisfies
  the Tier-1 Jepsen success criterion.

- **NEEDS-HUMAN — The compounding-loop DoD is still satisfied only by a Markdown doc; the
  "seeds" are asserted on by nothing.** `crates/dst/tests/tikv_surfaced_seeds.md:40,59`
  register seeds `17`/`29` as `status: known-gap` prose. Iteration 1 explicitly required
  `PROMOTED_SEED=17` be *asserted on, not just eprintln'd*, and iteration 2 required "a
  committed **executable** DST regression … or explicit human sign-off." This iteration
  removed even the `eprintln` and ships pure documentation — no `.rs` references seed 17 or
  29. The brief's mandatory (non-optional) DoD bullet ("promoted back into DST as a new
  seeded regression, with the seed committed") is met by a registry file only. This is
  Known-NEEDS-HUMAN #5; a human must decide the known-gap doc suffices, because deterministic
  evidence for it does not exist.

## Unwarranted claims in the patch

- **`crates/metadata-tikv/tests/tier1_metadata_integration.rs:14-15` claims the Check build
  "COMPILES and type-checks the body — an API regression on `MetadataStore` would fail to
  build." This is false.** The real body (`fn run`) and `TikvMetadataStore` itself live under
  `#[cfg(feature = "tikv")]` (`crates/metadata-tikv/src/lib.rs:299` gates the whole `store`
  module), and `cargo xtask ci` never enables that feature (default = `[]`;
  `crates/metadata-tikv/Cargo.toml`). At Check the `#[cfg(not(feature="tikv"))]` stub compiles
  instead, so a signature change to `MetadataStore::commit/get`/`WriteBatch` would **not**
  break the Check build. The same over-claim recurs in `tier1_jepsen_metadata.rs` and
  `tier2_metadata_io.rs`. The tier bodies get zero Check-time type-safety net.

## Attacks on the C4-verify red→green

- **The demonstrated flip is compile-level, not behavioral.** `C4-verify` is marked `pass`
  because reverting the fix deletes the imported symbols — `use wyrd_testkit::MetaClusterFaultPlan`
  (`crates/testkit/tests/meta_fault_seam.rs`) and `use xtask::meta_dispatch::…`
  (`xtask/tests/meta_dispatch_orchestration.rs`) fail to *resolve*, so the targets go red by
  **failing to compile**, not by a failing assertion. Any symbol removal would flip it. The
  genuine independent oracle (`survivors * 2 > n` at `meta_fault_seam.rs:56`, floor-values at
  `:75`) **does** survive scrutiny and would catch a logic mutation (e.g. `n/2`), but the
  gate never exercises it: a mutation to `pd_quorum_safe_max`'s *body* (not its existence)
  was not shown to go red. The flippable proof is real but weaker than "the logic is
  correct."

## Lesser findings scoped to this diff

- **Dead nemesis wiring.** `xtask/src/faults.rs:834` exports `WYRD_TIER1_NEMESIS_NODES` to the
  scenario subprocess, but no code reads it (grep: sole writer, zero readers;
  `tier1_jepsen_metadata.rs` reads only `WYRD_TIKV_PD_ENDPOINTS` and `WYRD_TIER1_FAULT_SIGNAL`).
  The comment at `faults.rs:833` calls it "diagnostic reproduction," but the PD-minority plan
  is never consumed by the scenario — only the runner's own `docker compose pause` uses it.
  Vestigial; harmless but misleading.

- **The dispatch "resolves to a runnable target" oracle does not check the feature it runs
  with.** `xtask/tests/meta_dispatch_orchestration.rs` verifies the `--test` file exists and
  the package name matches `Cargo.toml`, but the real runner invokes
  `cargo test --features tikv` (`faults.rs`). A route naming a real file in a crate that
  lacked the `tikv` feature would pass this test green yet error at run. Low value given the
  fixed single target crate, but the "filesystem is the oracle, not a mirror" claim is only
  partial.

## Attempted but could not refute

- The `testkit` seam oracles (`pd_quorum_safe_max` → `survivors*2>n`, `window_ms`
  discrimination between `Pause/Latency` and `Partition/ClockSkew`, seed reproducibility and
  seed-drives-schedule) carry genuine, implementation-independent inequalities and run
  un-`#[ignore]`d at Check. Tried to find a seed or `n` where the invariant passes vacuously
  or the oracle restates the impl — could not.
- The determinism fix holds: the on-Check `meta_dispatch_orchestration.rs` reads only files
  this patch adds (never a `deploy/` compose), and the `#[ignore]`d tier tests never run
  under `cargo xtask ci` (no `--ignored`), so the iteration-2 flapping panic path is closed.
- The trait invariant holds: the diff does not touch `crates/traits/src/lib.rs`.

### Advisory — codex

- `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:123` signals the runner before the first round is seeded, and `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:143` skips every round whose seed commit hits the pause. That lets `meta-jepsen` spend the entire data-node pause on skipped seed attempts, then satisfy `committed_rounds > 0` with ordinary post-recovery CAS traffic; the Jepsen leg can pass without any racer/read assertion actually overlapping the injected fault.
- `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:163` counts only `Ok(Committed)` responses as winners, while `crates/metadata-tikv/src/lib.rs:593` documents commit-time faults/undetermined errors. If a paused/partitioned TiKV commit takes effect but returns `Err`, this test may count zero winners and skip the readback check at `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:175`, missing the “Err is unavailability, never a silent winner” case the new seed registry calls out.
- NEEDS-HUMAN — `crates/dst/tests/tikv_surfaced_seeds.md:7` explicitly says the committed seed registry is data, not an executable DST regression, and `crates/dst/tests/tikv_surfaced_seeds.md:31` asks sign-off to adjudicate whether that known-gap document satisfies the compounding-loop DoD. Human sign-off still needs to accept or reject that reduced artifact.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decision owed: does the **compounding-loop DoD** (a *committed executable* DST regression promoted from a real-cluster discovery) count as met by a **Markdown known-gap registry** (`crates/dst/tests/tikv_surfaced_seeds.md`, seed 17/29, status `known-gap`), which is data no test asserts on? The builder argues an executable seed here would either re-prove redb's own atomicity (the iteration-1 anti-pattern) or need forbidden backend edits; brief Known-NEEDS-HUMAN #5 explicitly routes this to the human. Impact: this is a mandatory DoD bullet, not optional — the human must accept the doc-as-hypothesis posture or require a live discovery.
- [ ] T5 Judgment — Decision owed on the enumerated posture/tooling judgments: (a) the **reduced-bar static-endpoints** posture pending #256 (deploy stack) and #365 (L5 discovery) — accept as sufficient to prove the metadata risk? (brief.md:81-83,166-168); (b) **fault-kind fidelity** — the brief's Jepsen expectation names "partitions, clock skew, and process pauses," but only `Pause` is wired into the runner; `Partition`/`Latency`/`ClockSkew` are modeled in `MetaFault` (testkit lib.rs:638-657) yet never injected. The builder's rationale (single TiKV data node ⇒ pause is the honest single-replica nemesis) is defensible but is a judgment the human owns.
- [ ] Validation — fitness-to-purpose — The **binding** Success criterion (brief.md:36-51) — the metadata swap holds atomicity + single-zone consistency (clause-2 linearizability, exactly-one-winner) on real TiKV under real faults, Tier-2 real-I/O green, and a real-store discovery promoted to a committed DST seed — is demonstrable ONLY in the privileged CI/eval Tier job (Docker + root + tc/iptables/cgroup/libfaketime + a TiKV cluster), not at Check. Decision owed: confirm that job's recorded run (fault schedule, history-check verdict, committed seed) actually landed green and that the reduced-bar posture satisfies the DoD. Runnable check for the human: on a privileged Docker host with the #256 stack, `WYRD_TIER1=1 cargo xtask meta-integration`, `WYRD_TIER1=1 WYRD_TIER1_SEED=<seed> cargo xtask meta-jepsen`, `WYRD_TIER2=1 cargo xtask meta-tier2` — expect exactly-one-winner + stable commit-point reads across the injected `docker compose pause tikv` window, and a non-vacuous "N/M rounds committed" recovery line.
- [ ] `crates/dst/tests/tikv_surfaced_seeds.md:7` explicitly says the committed seed registry is data, not an executable DST regression, and `crates/dst/tests/tikv_surfaced_seeds.md:31` asks sign-off to adjudicate whether that known-gap document satisfies the compounding-loop DoD. Human sign-off still needs to accept or reject that reduced artifact.

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
- Iteration delta (if iterating): Rejected: the binding Jepsen criterion (exactly-one-winner under real partition) is unfalsifiable on the available topology. Even the now-merged #256 `small-multi-node` stack has a single TiKV data replica (3-node PD/etcd/dserver, but one `tikv`), so the metadata store can only go *unavailable* (Err), never split-brain. The on-Check green is a compile-level flip, not behavioral. Direction for the rebuild: - Extend the TiKV data plane to a real ≥3-replica Raft group (deploy compose), and point the nemesis at a data-plane PARTITION, not a single-node pause — this is the load-bearing fix that makes the property able to go red. Deploy compose + xtask runner + testkit seam are in-scope for this slice (not on the forbidden traits/core/custodian/metadata-backend list). - Wire the real fault kinds the brief names (Partition/ClockSkew/Latency), not only Pause. - C5 compounding-loop seed: deferring the executable DST regression to #258 is acceptable, but only once a real-cluster discovery exists to promote; a known-gap hypothesis doc alone does not close the mandatory DoD bullet. - Reduced-bar/static-endpoints posture is now stale: #256 is merged, so the "pending #256" excuse no longer holds. §6 items: none ticked — all four are the reject reason or downstream of it.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_257: deploy corpus under-provisioned for consistency tests — #256 `small-multi-node` triplicates PD/etcd/dserver but ships a single TiKV data replica; future realism-ladder slices inheriting it can't falsify exactly-one-winner-under-partition.
