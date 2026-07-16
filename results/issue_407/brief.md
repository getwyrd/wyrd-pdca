# Design proposal — issue 407 / m4-metadata-nemesis-partition-skew-pause

> The Plan artifact for getwyrd/wyrd#407 — "M329.4 — partition/skew/pause nemesis over the
> M4 cluster (reuse #257)". Slice 4 of #329 (the literal Jepsen/Elle credibility artifact),
> planned per ADR-0041 ("the two are sequenced: nemesis first, then the checked artifact").
> Authored at Plan 2026-07-15. This SUPERSEDES this bundle's 2026-07-07 `UNPLANNED.md`:
> both of its Plan-blocking gaps have since closed — #257, #256 and #406 are all CLOSED on
> the tracker, and the #442 fault battery (commit 60469a4) landed the shared backend-agnostic
> scenario + a live 3-process FDB cluster this slice extends.

- **Slug:** m4-metadata-nemesis-partition-skew-pause
- **Kind:** enhancement (design proposal)
- **Goal:** the metadata Tier-1 scenario can be driven under a composable **nemesis** with
  three fault classes over the real multi-node M4 metadata cluster — **network partition**
  (live-node, in-netns `iptables` DROP, the #399 technique), **clock skew** (`libfaketime`
  on cluster nodes), and **process pause** (freezer-cgroup `docker pause`/`unpause`) — each
  leg with a peer-side materialization oracle, so #408's checked workload has real faults
  to run under.
- **Success criterion:** the nemesis exposes three leg kinds (partition / clock-skew /
  process-pause) behind one **importable** seam in `wyrd-metadata-fault-conformance`
  (Design §1 — usable by both the Tier-1 scenario and #408's server-side scenario); each
  leg carries a **materialization oracle** that refuses to report a fault that did not
  bite (an un-materialized fault run FAILS as inconclusive, never passes silently — the
  #442 rule); and the host-independent logic is exercised **red→green at Check** by two
  added tests: `cargo test -p xtask --test nemesis_orchestration` (leg enumeration /
  dispatch / runner args) and `cargo test -p wyrd-metadata-fault-conformance --test
  nemesis_oracles` (lifecycle + oracle arithmetic over recorded observations). The live
  legs themselves run opt-in (`WYRD_TIER1=1`), off-Check.
- **Falsifiability:** RED is produced in the C4-verify worktree: the gate reverts the
  production change and keeps BOTH added test files
  (`xtask/tests/nemesis_orchestration.rs`,
  `crates/metadata-fault-conformance/tests/nemesis_oracles.rs` — the gate loops every
  added `*/tests/*.rs`), whose imports/assertions against the new modules then fail to
  compile/pass — the added-test classification was dry-run confirmed via
  `run-verify.sh --classify` (both emit `ADDED_TEST`; both crates pre-exist, so the red
  legs are real, not green-only). The *live* forbidden failure (an ADR-0015 violation under a
  real partition/skew/pause) is exhibitable only on the ≥3-process Docker cluster
  (`deploy/fdb-multi-replica`) and is deliberately off-Check (ADR-0016); the binding
  criterion is therefore scoped to the Check-testable core above, per the established
  `metadata_faults.rs` / `jepsen_dispatch` pattern (`xtask/src/lib.rs:1-14` names this
  exact born-at-tier flippable-seam contract).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 0 of this batch — #408 (`Depends on: 407`) composes the checked
  workload with this nemesis and both patches touch `xtask/src/lib.rs` / `xtask/src/main.rs`
  (and possibly `xtask/Cargo.toml` / command help), so 408 must build on this bundle's
  folded result, never blind beside it. This slice must therefore land the nemesis seam as
  a **stable public API** (public types in `wyrd-metadata-fault-conformance` + the thin
  xtask dispatch) that 408 consumes WITHOUT reopening its lifecycle logic.
- **Scope:** the three-leg nemesis seam + its materialization oracles + the pure
  plan/dispatch logic + the minimal deploy delta (`libfaketime` availability; the iptables
  agent already ships) + the two Check-time tests — the lifecycle/evidence/impls in
  `wyrd-metadata-fault-conformance`, one thin enum/dispatch module in the `xtask` lib
  (zero new xtask dependencies), nothing else. / out of
  scope: running the checked #406 workload under the nemesis and the report (that is #408);
  the scheduled privileged CI job (#409); fixing the pre-existing red TiKV tier1 rename
  timeout (go/no-go carve-out 1, its own issue); flipping the production backend default;
  any refactor of `wyrd-metadata-fault-conformance` / `wyrd-testkit` not strictly required
  by the new seam; dynamic mid-run skew control (v1 is a static per-leg offset).
- **Difficulty:** high
- **External dependencies:** Check-core red→green needs only the base Rust toolchain. The
  opt-in live legs additionally need `docker`, `libfdb_c loadable`, `fdb headers (bindgen)`
  (all registered doctor rows), and — plain prose, no detect command possible — a
  ≥3-process `deploy/fdb-multi-replica` cluster topology with a privileged in-network-namespace
  `iptables` sidecar (the partition technique) and `libfaketime` present in the node
  containers (the skew technique); that topology/privilege shape is (no-check: it is an
  environment shape, not an installable tool).
- **Test file:** xtask/tests/nemesis_orchestration.rs (primary; the lifecycle/oracle tests additionally ship at crates/metadata-fault-conformance/tests/nemesis_oracles.rs — both are ADDED test files, each earning its own C4-verify red)
- **Verification posture:** net-new coverage + deferred live green (template postures (a)+(b),
  pre-declared). What IS built AND exercised at Check: the leg enumeration, dispatch
  routing and runner args (the xtask test, mirroring
  `xtask/tests/metadata_faults_orchestration.rs`), and the lifecycle + typed evidence +
  materialization-oracle arithmetic (the conformance-crate test — pure decisions over
  recorded observations, no Docker) — both inside `cargo xtask ci`'s `cargo test
  --workspace`. What is deferred: the live three-leg runs against the real cluster —
  opt-in `WYRD_TIER1=1`, confirmed by the maintainer running the xtask leg locally now and
  by the #409 privileged CI job later (a SEPARATE work item, not waved through here).
  Deferred ≠ unbuilt: the live-leg impls must exist in the conformance crate
  (unconditionally compiled — they are docker shell-outs, no `libfdb_c` linkage) and their
  decision logic be the very code the named tests exercise — never inert dispatch
  scaffolding. **Type-check boundary, pinned:** everything the two named tests import MUST
  be default-compiled — no `#[cfg(feature…)]`, no FDB linkage, no Docker/Java dependency
  in either test's build graph, unconditional `#[test]` functions, the xtask module `pub`
  and wired from `xtask/src/lib.rs` (otherwise a C4-verify red degrades to a vacuum). Only
  the *wiring of the legs into the fdb-feature Tier-1 scenario* stays feature-gated under
  `crates/metadata-fdb/tests/` (its siblings are `#[cfg(feature = "fdb")]`-scoped, e.g.
  `tier1_metadata_consistency.rs:133`), type-checked under the privileged
  `WYRD_FDB_TOOLCHAIN` opt-in (`FDB_TOOLCHAIN_ENV`, `xtask/src/lib.rs:34`) — so "compiled
  by ci" is claimed ONLY for the default-compiled surface, not the fdb-feature bodies.
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites to mirror (composition slice — Do MAY open exactly these):
  - the backend-agnostic fault seam this generalizes: `pub trait ClusterFault`,
    `crates/metadata-fault-conformance/src/lib.rs:85` (apply / heal / peer-side oracle shape);
  - the FDB one-shot partition impl whose TECHNIQUE the partition leg re-implements in the
    conformance crate (it is test-binary-private — it CANNOT be imported or wrapped):
    `MasterIsolation`, `impl …ClusterFault` at
    `crates/metadata-fdb/tests/tier1_metadata_consistency.rs:232` (runtime target
    resolution from `status json`), with the in-netns `iptables` mechanics in the same
    file (`fn iptables`, `crates/metadata-fdb/tests/tier1_metadata_consistency.rs:216`);
    the TiKV sibling is `SymmetricPartition`,
    `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:167`; the opt-in runner
    shape is `run_fdb_metadata_tier1`, `xtask/src/fdb_faults.rs:286`;
  - the leg-enumeration pattern (freeze + partition legs, with the Check-time test that
    pins it): `tier1_jepsen_isolation_legs`, `xtask/src/faults.rs:245` and its test at
    `xtask/src/faults.rs:1013`;
  - the process-pause mechanics: `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:981`
    (the `docker pause` command construction, args at :982-984) and `xtask/src/faults.rs:209`
    (`:pause` freezer-cgroup doc);
  - the dispatch + orchestration-test pattern the new test must mirror:
    `xtask/src/metadata_faults.rs:53` (`metadata_tier_dispatch`) and
    `xtask/tests/metadata_faults_orchestration.rs:1-25`;
  - the harness-side history clock the skew leg MUST NOT touch:
    `crates/server/src/consistency_observable.rs:66-68` (`OpRecord.start/end`).
- **Prior-art check (by affected file path):** merged history — #399 (CLOSED) already landed
  the live-node network-partition upgrade on the *data-path* Jepsen legs
  (`xtask/src/faults.rs`, `tier1-jepsen.yml`); #442 (commit 60469a4) landed the *metadata*
  one-shot symmetric partition (master cut) + mid-commit kill in `xtask/src/fdb_faults.rs`
  and the shared `crates/metadata-fault-conformance`; #257 (commit 9374758) landed the TiKV
  metadata tier and `deploy/tikv-multi-replica/iptables-agent/`. No open or closed PR titled
  for #407; nothing already implements clock-skew (`libfaketime` appears nowhere in the
  tree) or metadata-cluster process-pause. Not superseded; genuinely additive.
- **Disposition hint:** new-feature

## Motivation

#329's credibility artifact needs the checked workload (#406, landed as
`crates/server/src/consistency_workload.rs`) to run **under real faults** on the real
multi-node metadata cluster. What exists today is piecemeal: the data-path Jepsen legs have
pause + live-node partition (#250/#399), and the metadata fault battery (#442) has a
*one-shot* symmetric partition and a mid-commit kill wired directly into its scenario. There
is no reusable, composable nemesis — and no clock-skew fault class at all — that #408 can
drive the checked workload under. ADR-0041 §Consequences sequences exactly this slice first:
"nemesis first, then the checked artifact."

## Design

1. **One nemesis seam, three legs — the seam decision is made HERE, not left to Do.** A
   new leg-lifecycle abstraction (a `NemesisLeg`-style trait: plan → apply → confirm
   materialized → heal → confirm healed, with **typed materialization evidence** per leg),
   placed by dependency direction, not taste:
   - **`wyrd-metadata-fault-conformance` owns the lifecycle trait, the evidence types, the
     oracle arithmetic (pure decisions over sampled observations), AND the three live-leg
     impls.** The impls are `std::process::Command` shell-outs (`docker`, `docker exec …
     fdbcli --exec 'status json'`, the compose faketime override) with **no `libfdb_c`
     linkage**, so they compile unconditionally in this ordinary lib crate — importable by
     the battery's tests AND by #408's `crates/server/tests/` scenario (dev-dependency).
     The existing `MasterIsolation` / `SymmetricPartition` are **test-binary-private and
     CANNOT be imported or wrapped** — the partition leg *re-implements their technique*
     here (in-netns `iptables` DROP + survivor-side `status json` oracle), and the
     battery's own private impls stay untouched (refactoring them is out of scope).
   - **The `xtask` library gets ONLY the leg-kind enum, the dispatch routing, and the
     runner-argument building** — mirroring `IsolationNemesis`
     (`xtask/src/faults.rs:245`), which keeps `xtask` at **zero new dependencies**
     (`xtask/Cargo.toml:11-14` today).
   The existing `ClusterFault` (`crates/metadata-fault-conformance/src/lib.rs:85`) is NOT
   generalized — it is partition-shaped by contract (`topology()`, peer-liveness,
   heal-rule completeness, lines 89–116) and remains the #442 battery's seam, untouched.
2. **Every leg proves it bit — per-leg oracle definitions.** An un-materialized leg makes
   the run FAIL as inconclusive (the #442 "a note is not a gate" rule,
   `docs/design/reviews/m4-fdb-go-no-go.md`). Concretely:
   - **partition** — survivor-side reachability flips (the existing peer-side oracle) while
     the target container/process stays `running`;
   - **pause** — the peers/client observe the target serving BEFORE the freeze, serving
     NOTHING during the window (while `docker inspect` reports `paused`), and serving again
     after unpause — three observed state transitions, never a single probe;
   - **skew** — before the workload proceeds, an in-container probe **sharing the target
     container's exact preload/env configuration** demonstrates the target's clock is
     offset from the harness clock by at least the configured floor.
3. **Clock-skew mechanism — pinned (the compose ships the stock image, no custom
   entrypoint: `foundationdb/foundationdb:7.3.77` at
   `deploy/fdb-multi-replica/docker-compose.yml:67,85,97`).** The skew leg applies a
   **static per-leg offset** by recreating exactly ONE fdbserver container with
   container-scoped `LD_PRELOAD` (libfaketime) + `FAKETIME` environment (compose
   override/profile; the preload artifact is added as a small image layer or bind-mount);
   heal = recreate without the override. The recreate is itself a restart-fault, so the
   leg's contract is: the recreate completes and the cluster re-stabilizes BEFORE the
   measured workload window opens — the leg measures skew, never the restart. Because the
   env is container-scoped, a `docker exec` probe in that container inherits the same fake
   clock — that is the materialization oracle above, not an unrelated process. No dynamic
   mid-run offset in v1 (out of scope). ADR-0024 / checker real-time-order interplay, explicit per the issue:
   the leg skews **cluster-node clocks only**, never the harness/client process — the #406
   history's real-time order is stamped by the harness (`OpRecord.start/end`,
   `crates/server/src/consistency_observable.rs:66-68`), so the recorded order stays
   trustworthy while the system under test runs on a lying clock. What the leg asserts is
   the weaker, defensible property (ADR-0015 signals hold; no fail-open lease behavior
   observed), with the stronger ADR-0024 assertion deferred to that ADR's implementation
   issue (ADR-0024 is still Proposed).
4. **Target stack: FDB multi-replica; seam stays backend-agnostic.** Per ADR-0042
   (FoundationDB is the production backend) and the GO verdict
   (`docs/design/reviews/m4-fdb-go-no-go.md`), the live legs run against
   `deploy/fdb-multi-replica`. Backend-agnosticism is carried by the NEW lifecycle seam
   (its backend-shaped parts — which container, which oracle query — are leg-impl data),
   NOT by widening `ClusterFault` (which stays the #442 battery's partition seam,
   untouched) — the eduralph comment on #408 pins this slice as "backend-agnostic harness
   pieces … stay in M4 and are reused". No TiKV live-leg green is owed here (the TiKV
   tier1 leg is red on unmodified `main` for a pre-existing rename-timeout, the go/no-go's
   carve-out 1; fixing it is its own issue).
5. **Deploy delta.** `deploy/fdb-multi-replica` already ships the in-netns iptables agent
   (`image: wyrd-iptables:local`, `deploy/fdb-multi-replica/docker-compose.yml:115`, the
   generic agent from `deploy/tikv-multi-replica/iptables-agent/`), so the remaining delta
   is mainly `libfaketime` availability in the node containers (plus anything the pause leg
   needs) — additive compose/profile changes only, per ADR-0043's fixture-profile
   discipline.

## Alternatives considered

- **Wire the two missing legs straight into the #442 scenario (no seam):** rejected — #408
  needs the same legs over the *checked workload*, so a second hard-wiring would duplicate
  fault logic the day after it lands; the seam is the reuse point ADR-0041 sequences this
  slice for.
- **Skew via `date -s` / CAP_SYS_TIME in containers:** rejected — mutates shared kernel
  clock state on the CI host in some runtimes and is not per-process; `libfaketime` is
  per-container, reversible, and is what the issue names.
- **Reuse Jepsen's own nemesis (Clojure):** rejected — ADR-0041 keeps the JVM strictly in
  the *checker* seat, off-Check; the fault driver stays Rust/xtask like every existing leg.

## Impact & compatibility

Additive harness/test/deploy code only — no production crate's runtime behavior changes.
`cargo xtask ci` stays unprivileged and container-free (new live legs are `WYRD_TIER1`-gated,
compile-checked in ci). Blast radius is cross-file but confined to `xtask`,
`crates/metadata-fault-conformance` (the seam, oracle arithmetic, and leg impls — Design
§1), the fdb-feature scenario wiring under `crates/metadata-fdb/tests/`, and
`deploy/fdb-multi-replica` — hence Difficulty high on reach, not on risk to production
paths.

## Open questions

- Maintainer confirmation at sign-off: one witnessed local `WYRD_TIER1=1` run of the three
  legs, since the #409 CI job does not exist yet. (The ADR-0024 skew-assertion strength
  question is resolved in Design §3: assert the weaker, defensible property; the stronger
  one belongs to ADR-0024's own implementation issue.)

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on reviewer + adversary feedback: the Check-core (oracle arithmetic, enumeration, red→green) is sound, but the live-leg half is defective. Fix in the next Do attempt: 1. Pause lifecycle must ENCLOSE the workload: `confirm_materialized` currently runs `docker unpause` (nemesis.rs:595) before `drive_leg` invokes the workload (:276), so nothing runs under the pause. Also replace the single post-pause probe with a settle-window poll (mirror PartitionLeg's 45s poll at nemesis.rs:470) — one immediate `served_during` sample is near-deterministically inconclusive. 2. Skew leg default triple-mismatch: compose override hardcodes fdb1 (docker-compose.faketime.yml:30), test defaults WYRD_TIER1_SKEW_SERVICE=fdb2, and the probe reads all.last()'s container independently of `service` — default runs can never materialize. Make service/override/probe agree (probe the skewed service), and enforce or drop the "non-master node" comment. 3. Wire an actual runnable entry point: no xtask command consumes `xtask::nemesis::*`; docs reference a nonexistent `run_metadata_nemesis` and falsely claim the legs run under `cargo xtask fdb-metadata-tier1`. Either wire the dispatch (brief anticipated xtask/src/main.rs) or, if runner wiring is deferred to #408/#409, correct every doc string — but the brief's sign-off open question (witnessed WYRD_TIER1=1 run of all three legs) must be satisfiable. 4. `drive_leg` must not leak fault state on non-happy paths: heal on `apply()` failure (partial iptables rules — mirror MasterIsolation's Drop guard, tier1_metadata_consistency.rs:309-316); don't apply the skew fault in `plan()` (plan failure leaves a permanently skewed node); catch_unwind around the workload so a panicking workload (the shipped one panics by design) still heals. 5. Guard the central rule with tests: add a mock-NemesisLeg test driving `drive_leg` so deleting the inconclusive bail (nemesis.rs:266-274) or the `heal_is_complete` check (:279-285) goes red — "un-materialized fault FAILS, never passes silently" is currently unguarded. Also fix or pin the false `--exact` rename-safety claim (a missing test fn exits 0 → silent green no-op). Minor: `survivor_status_json` should pass `--timeout 10` to fdbcli like support::status_json does.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the advisory review (C3/C5/T5 FAIL) and the adversarial pass. The Check-core half (oracle arithmetic, lifecycle seam, orchestration, red→green for the two named tests) withstood attack and should be preserved; the live-leg half is defective. Fix in the next Do attempt: 1. Clock-skew leg fails deterministically on every live run: the runner resolves the skew target as a container ID once, pre-campaign (`container_of` via `docker compose ps -q`, xtask/src/fdb_faults.rs:81-106, used at :389, exported at :454), but the leg's `apply()` force-recreates fdb2 (nemesis.rs:781,817), invalidating that ID — every `docker exec <old-id>` probe then fails and `wait_execable` times out (nemesis.rs:768,791-792,818). Probe by stable compose container NAME, or re-resolve `compose ps -q` after each recreate. This repeats iteration 1's carry-forward item 2 defect class (live skew leg cannot materialize on the default run) — do not resolve container identity before a recreate again. 2. Same root cause cross-leg: `netns_map` is resolved once pre-campaign (fdb_faults.rs:388) and the skew leg's apply AND heal both recreate fdb2 (nemesis.rs:781,831), so the later pause leg can receive a stale container ID from `resolve_role_holder` (metadata-fdb tests/support/mod.rs:81-82) and `docker pause <stale-id>` fails. Same fix: name-based or post-recreate re-resolution for ALL legs. 3. `drive_leg` must not drop a heal failure when the workload panics: `resume_unwind` (nemesis.rs:343) runs BEFORE `heal_result?` and the `heal_is_complete` check (nemesis.rs:346-353), so a failed unpause/recreate during a panicking workload leaks fault state silently — contradicting the module's own no-leaked-fault claim (nemesis.rs:50-51). Surface/record the heal failure before resuming the panic, and strengthen the guard test (nemesis_oracles.rs:383-401 currently asserts only heal_count >= 1) to pin the panic-plus-heal-failure case. 4. The fdb-feature Tier-1 wiring (crates/metadata-fdb/tests/tier1_metadata_nemesis.rs) earns no red and is compiled by no gate this cycle; its "structurally impossible" disagreement claim (:122-127) is falsified by item 1. After fixing 1-3, the brief's sign-off open question (a witnessed WYRD_TIER1=1 run of all three legs materializing and healing) must be satisfiable — today it would fail on the skew leg every time. Minor (fix opportunistically): `parse_tests_run`'s `tail.starts_with("test")` arm (xtask/src/nemesis.rs:135) subsumes the others and accepts non-test lines; tighten or align the comment.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected to correct the adversary's three implementation defects; the Check-core (seam, oracle arithmetic, orchestration, red→green — independently reproduced with mutation kills, and the iteration-2 container-identity fix confirmed) withstood attack and must be preserved unchanged. Fix in the next Do attempt: 1. Skew leg `apply` must wait for the CLUSTER to re-stabilize, not merely for `docker exec` to work: `wait_execable` (nemesis.rs:821, used at :848-854) succeeds long before the recreated fdbserver rejoins, and with no volumes on the fdb services (docker-compose.yml:65-107) `--force-recreate` wipes a storage/coordinator node — the workload window opens during re-replication, violating Design §3 ("the leg measures skew, never the restart"). After each recreate (apply AND heal), poll a survivor's `status json` for cluster health, as PartitionLeg::plan does (nemesis.rs:573-581). Do not redefine "re-stabilize" as "exec-able" in comments. 2. Stop dropping heal failures on the three early exit paths (`let _ = leg.heal();` at nemesis.rs:305,315,324 — apply-failed / confirm-failed / un-materialized): apply the same heal_incomplete_reason / confirm_healed leak verdict there as on the happy and panic paths. This is the same defect class iteration 2 was rejected for; it matters because #408 imports drive_leg directly and gets no `compose down -v` backstop (fdb_faults.rs:461). Guard it with a mock-leg test so removing the check goes red. 3. Minor: `container_name_of` (xtask/src/fdb_faults.rs:374-395) must check `out.status` and surface stderr instead of misreporting a failed `docker compose ps` as "cluster did not come up". Then the brief's pinned sign-off open question — the witnessed WYRD_TIER1=1 three-leg run (materialize + probe + heal) — must be performed AFTER fix 1, or it measures the restart, not the skew; expect non-Debian hosts to need an environment-specific WYRD_TIER1_SKEW_SO for the libfaketime bind-mount (docker-compose.faketime.yml:47).
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
