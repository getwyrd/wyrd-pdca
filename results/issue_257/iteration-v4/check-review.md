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
