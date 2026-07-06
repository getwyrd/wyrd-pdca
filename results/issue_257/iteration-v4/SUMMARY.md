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

# Check review — issue 257 / m4.6-tier1-jepsen-tier2 (iteration 4)

**Task under review:** extend the realism-ladder **Tier-1 (integration + Jepsen)** and
**Tier-2** test lines across the redb→TiKV **metadata** backend swap behind the *unchanged*
`MetadataStore` trait — net-new `testkit` fault seam, `xtask` `meta-*` runners + pure dispatch,
a 3-store TiKV Raft-group deploy stack, and the metadata-swap tier tests — with the
**Check-observable flippable** being the pure dispatch/seam decision logic (the load-bearing
live tier green is privileged/off-Check by design). This is iteration 4; iters 1–3 were rejected
for hollow evidence (tautological routing test, decorative DST seed, unapplied/mis-targeted
nemesis, single-replica data plane that could never split-brain).

**Grounding note (target-state caveat, not a patch defect):** `$PDCA_TARGET` is not resolvable
in this sandbox (env inspection blocked); per protocol I ground citations on `patch.diff` and
did not re-run the workspace. Deterministic gates in `check-gates.json` (C4-ci PASS, C4-verify
PASS) are trusted and cross-checked against the patch's oracle structure, not re-executed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The Check-observable target is specified unambiguously (brief.md:52-56, 97-100): the pure xtask dispatch/routing + testkit fault-seam decision logic, with the live tier green pre-declared DEFERRED/off-Check. The patch's on-Check surface matches that spec (`xtask::meta_dispatch`, `wyrd_testkit::meta_faults`). |
| C2 Reproduction (red pre-fix) | PASS | On-Check red is genuine and negatable: `meta_fault_seam.rs:962-981` pins `max_quorum_safe_faults` against an *independent* majority oracle `2*(n-f)>n`, so a wrong body reddens (not a tautology — the iter-1/2 defect). `meta_dispatch_orchestration.rs:2138-2158` reds if `deploy/tikv-raft-3` reverts to <3 stores. The load-bearing *behavioral* repro (two-winners under real partition) is off-Check → see Validation. |
| C3 Change | PASS | Net-new only: seam (`crates/testkit/src/lib.rs:376-925`), dispatch (`xtask/src/meta_dispatch.rs`), runners (`xtask/src/faults.rs:1352-1773`), deploy compose, tier tests. Invariant held — trait untouched; no edits to `traits`/`core`/`custodian`/`metadata-tikv/src` (patch touches none of those paths). |
| C4 Verification (red→green) | PASS | Both gates PASS (check-gates.json:32-49). Re-derived from the patch: the pure oracles are load-bearing (independent majority arithmetic; scenario-file existence; real-compose store count), so the advisory C4-verify red→green is credible, unlike the iter-1/2 hollow flips. Could not re-execute (target unavailable) — trusting gate + oracle structure. Behavioral green is deferred (Validation). |
| C5 Causal adequacy | NEEDS-HUMAN | Decision owed: the **mandatory compounding-loop DoD seed** is NOT delivered — `crates/dst/tests/tikv_surfaced_seeds.md:63-65` is an intentionally EMPTY registry, executable seed deferred to #258, and no privileged Tier run has yet surfaced a discovery to promote. Iter-3 ruled "a known-gap doc alone does not close the bullet." Human must decide whether the honest deferral is acceptable absent a live run. (Symptom-guard smell-test does NOT fire: the `#[ignore]`/`WYRD_TIKV_PD_ENDPOINTS` skips are sanctioned tier-gating, not a capability probe over a load-time side effect.) |
| T1 Structure | PASS | Pure decision logic correctly separated from privileged orchestration (seam in `testkit`, dispatch in `xtask/src/meta_dispatch.rs`, runners in `faults.rs`), mirroring the existing `jepsen_dispatch`/`disk_faults` pattern; deploy stack lives under `deploy/` outside the workspace (ADR-0010). |
| T2 Shape | PASS | Idiomatic: `#[must_use]` pure fns, typed `TopologyTooSmall` error with an explanatory `Display`, every injection result `?`-checked (`faults.rs:1550,1560-1584`) — closes iter-2 "pause result discarded". |
| T3 Runtime | PASS | C4-ci PASS confirms `cargo test --workspace` green with tier tests skipping cleanly (gate honesty, brief.md:149-152). Iter-2 flakiness (test hard-reading an absent compose / env leak) is addressed: `deploy/tikv-raft-3/docker-compose.yml` is created in-patch so `meta_dispatch_orchestration.rs:2143` reads a present file, and every tier body early-returns when `WYRD_TIKV_PD_ENDPOINTS` is unset. Not re-run here (target unavailable). |
| T4 Contribution | PASS | Delivers proposal-0015 PR-sequence item 6: three metadata-swap tier legs + the falsifiable 3-store topology that iter-3 demanded (`deploy/tikv-raft-3`, replication factor 3, `iptables` data-plane partition of a quorum-safe minority). |
| T5 Judgment | NEEDS-HUMAN | Decision owed on posture + one off-Check mechanism risk: (a) static-endpoints reduced bar pending #365 and #256 cluster staging (brief Known-NEEDS-HUMAN); (b) Jepsen-proper vs in-repo Rust scenario harness — confirm which the metadata leg runs; (c) **mechanism risk:** the ClockSkew nemesis runs `date -s +8min` via `docker exec` on a `network_mode: host` container (`faults.rs:1580`); Docker does not namespace the clock by default, so this can skew the **host** wall clock during the privileged Tier run — human should confirm the Tier job isolates/uses a time namespace or a container-scoped skew. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: the binding criteria — exactly-one-winner + clause-2 linearizability under a REAL partition (a,b) and honest single-node I/O (c) — are observable only in the privileged Tier job, not at Check. I could not drive it (no privileged Docker host + no target worktree here). Human confirms at sign-off from the recorded live run. Runnable steps below. |

## Validation — concrete steps for the privileged Tier job (since it can't be driven at Check)

On a privileged Docker host (root, `iptables`/`tc`/`libfaketime`, ≥3-core), from the target checkout:

1. `WYRD_TIER1=1 cargo xtask meta-integration` — expect the 3-store Raft group up, a real
   `iptables` partition of `tikv0` (20160) synchronized to live load via the signal-file
   handshake, and the multi-key rename advancing with **no torn pair** and **≥1** advance
   (non-vacuity assert, `tier1_metadata_integration.rs:513`).
2. `WYRD_TIER1=1 cargo xtask meta-jepsen` — expect Partition→ClockSkew→Pause injected in turn,
   each result checked, and `winners <= 1` every round plus stable commit-point reads
   (`tier1_jepsen_metadata.rs:235,252`). Confirm the run is NON-vacuous (`committed_rounds > 0`).
3. `WYRD_TIER2=1 cargo xtask meta-tier2` — read-your-writes, multi-key atomicity, prefix scan,
   CAS-conflict on real single-node I/O (`tier2_metadata_io.rs`).
4. **Capture** the fault schedule + exactly-one-winner verdict + any redb-unmodeled behaviour, and
   record it into `crates/dst/tests/tikv_surfaced_seeds.md` (feeds the #258 executable seed) —
   this is what the C5 DoD decision turns on.

## Prior-art / dependency note
Targets and seam are net-new (brief.md:70-72 declares no same-file conflict); the #256 (`deploy/`
cluster), #365 (L5 discovery), and #329/#404-409 (checker substrate) couplings are NOTED
dependencies the human must confirm are staged — folded into the T5 / Validation NEEDS-HUMAN rows.

### Advisory — adversary

# Adversarial review — issue #257 / m4.6-tier1-jepsen-tier2 (iteration 4)

Advisory only — I do not gate. I attacked the red→green evidence, the fix, and the
verdict. Grounded on the target source at `/home/eddie/wyrd/wyrd.pdca-wt-l0`.

## What I attempted to refute and could NOT (honest negative results)

- **The pure Check red→green is genuine and production-wired, not a parallel re-impl.**
  I re-ran the flippables green (`wyrd-testkit --test meta_fault_seam`: 7 passed;
  `xtask --test meta_dispatch_orchestration`: 6 passed). The runner actually consumes the
  same decision code the tests bind: `meta_leg()` calls `meta_dispatch` (`xtask/src/faults.rs:812`
  region) and `run_meta_partition_leg` derives its plan from `leg.partition_plan()` →
  `meta_faults::plan_data_plane_partition`. So the tested logic *is* the production decision path.
- **`max_quorum_safe_faults` has a real independent oracle.** `meta_fault_seam.rs:962`
  states `2*(n-f) > n` independently of the `(n-1)/2` body and checks maximality
  (`!survivors_keep_quorum(n, f+1)`). A wrong formula (`n/2`, etc.) reddens it. This is the
  iteration-1 "tautology" defect genuinely fixed. I could not break it.
- **The leg-crossing / topology-drift oracles are real.** `the_three_legs_are_pairwise_distinct_scenarios`
  and `the_deploy_stack_declares_a_real_three_store_raft_group` (`meta_dispatch_orchestration.rs:2138`)
  redden if a leg is re-routed or the compose is reverted to one store (`stores == META_JEPSEN.replicas`).
  I could not make a leg-crossing bug stay green.

## Refutations that stand (concrete failing cases / unwarranted claims)

- **NEEDS-HUMAN — The "exactly-one-winner under real partition" assertion is insensitive to
  whether the partition ever materialized, so the *binding* Success-criterion (b) is not
  actually falsified even in the privileged run.** `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:165`
  asserts `winners <= 1` and `:194` asserts `committed_rounds > 0`. Both hold for any
  linearizable CAS against a serving majority **with or without a partition** — a minority
  partition, by construction, never changes the outcome. Concrete case: run the leg with the
  nemesis a complete no-op (see next finding) and the test is still green. The metadata commit
  path (`crates/metadata-tikv/src`) is *outside this slice* and unchanged, so this test can
  never go red on anything this slice touches; its green re-proves TiKV's own CAS atomicity.
  This is the iteration-2 "can pass without the fault" concern only *partially* closed: the
  runner checks the injection command's exit code, not that a partition took effect.

- **NEEDS-HUMAN — The `Partition` nemesis is an asymmetric, likely-ineffective cut, not a real
  data-plane partition.** `xtask/src/faults.rs:787` → `iptables_port(true, port)` (`:826`) runs
  `iptables -A INPUT -p tcp --dport <20160> -j DROP`. With every store on `network_mode: host`
  (`deploy/tikv-raft-3/docker-compose.yml:43,96`), this only drops *inbound* connections to
  tikv0's port; tikv0's own *outbound* Raft to tikv1/tikv2 (dports 20161/20162) and their
  replies (to tikv0's ephemeral source ports) are untouched. If tikv0 is the region leader it
  keeps heart-beating outbound, followers never time out, and no election/partition is observed.
  Concrete failing case: the leader-side "partition" is a no-op, the majority-serving CAS is
  identical to no-fault, and the leg passes having demonstrated nothing — exactly the
  iteration-3 rejection reason ("compile-level flip, not behavioral") re-manifesting at the
  injection layer instead of the topology layer.

- **NEEDS-HUMAN — `ClockSkew` cannot produce a per-store skew as its own doc claims; it skews
  the whole host or fails.** `xtask/src/faults.rs:797` injects skew via
  `docker compose exec <store> date -s +8min`. The seam doc at `crates/testkit/src/lib.rs:420`
  names `libfaketime` (a per-process shim) as the mechanism, but `date -s` sets the *shared
  kernel clock*: under `network_mode: host` with no time-namespace, it either fails (no
  CAP_SYS_TIME) or moves the clock for **all three TiKV stores + all three PD nodes + the test
  client at once** — the opposite of skewing "the targeted stores." The `+8min`/`-8min`
  inject/heal pair (`:797`/`:818`) is therefore not the fault Success-criterion (b) names.

- **NEEDS-HUMAN — The comment's claim "meta_partition_run heals its own rules on every path"
  is false; a failed heal or panic leaks a host-wide iptables DROP.** `xtask/src/faults.rs:689`
  asserts self-healing, but `apply_meta_nemesis` (`:747`) has no `Drop`/panic guard: if
  `heal_meta_fault(Partition)` returns `Err` (`:770`) — or the `thread::sleep` at `:769` is
  interrupted — the `-A INPUT ... DROP` rule on `127.0.0.1:20160` persists, and the
  `docker compose down` teardown (`faults.rs` `run_meta_partition_leg`) cannot remove a host
  iptables rule. Concrete consequence: a single failed heal silently blackholes port 20160 on
  the CI host for every subsequent run/job until manually flushed.

- **NEEDS-HUMAN — The mandatory compounding-loop DoD bullet is not satisfied: no executable DST
  seed is committed.** `crates/dst/tests/tikv_surfaced_seeds.md` is by its own text an *empty
  OPEN registry* ("no real-cluster discovery has been promoted yet … this document does not by
  itself close the compounding-loop DoD bullet"). The Success criterion is explicit that "at
  least one behavior … promoted back into DST as a new seeded regression, with the seed
  committed" is "a DoD bullet, not optional." The patch commits a Markdown placeholder and no
  `MADSIM_TEST_SEED`-bearing regression, so the binding criterion is open, not met. (This is the
  brief's Known-NEEDS-HUMAN #5 chicken-and-egg deferral — the human must decide whether the
  empty registry is an acceptable deferral to #258, not treat it as closed.)

## Where the verdict may be over-stated

- **`check-gates.json` C4-verify "red without the fix, green with it" over-reads what the flip
  proves.** The flippable exercises only the *pure decision* surface (`meta_faults` arithmetic +
  `meta_dispatch` routing) — net-new scaffolding for which there is no antecedent defect. It
  never exercises `inject_meta_fault`/`apply_meta_nemesis` (`xtask/src/faults.rs:775-820`), the
  code that would actually create the fault condition, and which carries the three concrete bugs
  above. A reader could take C4-verify PASS as evidence the *binding* Tier-1 Jepsen property is
  validated; it is not — that remains privileged/off-Check and, per the findings above, may pass
  with an ineffective or host-wide nemesis. The posture is legitimately DEFERRED, but the on-Check
  green should be read as "the routing/quorum-plan is correct," not "the metadata swap holds
  exactly-one-winner under a real partition."

### Advisory — codex

- `xtask/src/faults.rs:797` — `meta-jepsen` now injects `MetaFault::ClockSkew` by running `date -s` inside the TiKV container, but the `tikv-raft-3` services are ordinary host-networked containers with no `privileged: true` / `cap_add: [SYS_TIME]` / faketime wiring (`deploy/tikv-raft-3/docker-compose.yml:91`). In the privileged tier job this should fail at the clock-skew step rather than exercise the Jepsen leg, so the new off-Check green is likely not actually runnable as written.
- NEEDS-HUMAN — `crates/dst/tests/tikv_surfaced_seeds.md:30` — the committed compounding-loop artifact explicitly says no executable DST seed is committed and the registry is empty (`crates/dst/tests/tikv_surfaced_seeds.md:47`). Human sign-off still needs to decide whether deferring to #258 is acceptable for this slice, since the mandatory DoD asked for a real surfaced discovery promoted back into DST.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decision owed: the **mandatory compounding-loop DoD seed** is NOT delivered — `crates/dst/tests/tikv_surfaced_seeds.md:63-65` is an intentionally EMPTY registry, executable seed deferred to #258, and no privileged Tier run has yet surfaced a discovery to promote. Iter-3 ruled "a known-gap doc alone does not close the bullet." Human must decide whether the honest deferral is acceptable absent a live run. (Symptom-guard smell-test does NOT fire: the `#[ignore]`/`WYRD_TIKV_PD_ENDPOINTS` skips are sanctioned tier-gating, not a capability probe over a load-time side effect.)
- [ ] T5 Judgment — Decision owed on posture + one off-Check mechanism risk: (a) static-endpoints reduced bar pending #365 and #256 cluster staging (brief Known-NEEDS-HUMAN); (b) Jepsen-proper vs in-repo Rust scenario harness — confirm which the metadata leg runs; (c) **mechanism risk:** the ClockSkew nemesis runs `date -s +8min` via `docker exec` on a `network_mode: host` container (`faults.rs:1580`); Docker does not namespace the clock by default, so this can skew the **host** wall clock during the privileged Tier run — human should confirm the Tier job isolates/uses a time namespace or a container-scoped skew.
- [ ] Validation — fitness-to-purpose — Decision owed: the binding criteria — exactly-one-winner + clause-2 linearizability under a REAL partition (a,b) and honest single-node I/O (c) — are observable only in the privileged Tier job, not at Check. I could not drive it (no privileged Docker host + no target worktree here). Human confirms at sign-off from the recorded live run. Runnable steps below.
- [ ] `crates/dst/tests/tikv_surfaced_seeds.md:30` — the committed compounding-loop artifact explicitly says no executable DST seed is committed and the registry is empty (`crates/dst/tests/tikv_surfaced_seeds.md:47`). Human sign-off still needs to decide whether deferring to #258 is acceptable for this slice, since the mandatory DoD asked for a real surfaced discovery promoted back into DST.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Binding criterion must be a REAL Jepsen setup (the tool), not the in-repo Rust fault-injection harness that already exists — this slice re-delivers existing scaffolding under a "Jepsen" label and does not raise the bar. (Live-deployment axis explicitly set aside per sign-off.) The compounding-loop DST seed is #257's OWN deliverable: #257 is the producer of the real-store discovery, #258 only receives it (per #258's brief). So it cannot be deferred to #258 — which is itself C4-verify RED and unaccepted. Re-scope so the seed's producer step lands in #257's own binding scope. Nemesis mechanism must actually take effect when the real setup runs; current wiring is unsound (advisory, concrete): (1) Partition is an INPUT-only iptables DROP on a host-networked leader — leader keeps heart-beating outbound, so it is a no-op; (2) ClockSkew uses `date -s` on a network_mode:host container with no time namespace — skews the whole host or fails (no CAP_SYS_TIME); (3) a failed heal / panic leaks a host-wide iptables DROP with no Drop guard (blackholes the port on the CI host). Also: the `winners <= 1` assertion is insensitive to whether the partition ever materialized (passes with or without a fault).
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
