# Build notes — issue 257, iteration 13 (the evidence-architecture fix)

**Withheld from the reviewer.** Rationale for the human at sign-off.

## What this iteration is

Iterations 1–12 failed on one root defect: the slice never produced **executed behavioural
evidence** that a real commit-point regression is caught — at Check by declared Option-B
posture, off-Check because the privileged run was a perpetually-owed NEEDS-HUMAN and the
tier-1 leg had no teeth (iteration-12 adversary: strictly sequential single-writer +
minority-voter cut ⇒ deleting the `get_for_update` re-check leaves everything green).

This iteration implements the third brief revision (brief §"Iteration 12 — carry-forward +
THIRD plan revision"): teeth for the Tier-1 leg, and the **four-leg mutation acceptance
run executed and captured in `evidence/` BEFORE Check submission** (live-evidence-first).

## What changed (all cited on `feat/m4.5-deploy-tikv-pd-etcd` + this patch)

1. **Contention teeth** — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`:
   ≥2 concurrent writers (own connections, `tokio::sync::Barrier`-released) race the SAME
   CAS on the version cell across the fault window; plus a deliberately stale CAS probe.
   New independent signal `no_lost_update` (4th `ConsistencySignals` clause,
   `crates/testkit/src/lib.rs`) = exactly-one-winner AND stale-probe-rejected. The
   `get_for_update` re-check (`crates/metadata-tikv/src/lib.rs:555-574`, untouched) is what
   makes both hold; neutralizing it flips exactly this clause (evidence leg 3).
2. **Leader isolation** — `WYRD_TIER1_ISOLATE=leader`: the scenario resolves the region
   LEADER from PD at runtime (pure at-Check-tested parsers
   `wyrd_testkit::parse_first_region_leader_store_id` / `parse_store_ip`) and cuts THAT
   node, forcing an election mid-scenario; a minority-follower cut is outcome-neutral
   against a linearizable store (the iteration-12 refutation).
3. **The netns cut (found live this iteration)** — the loopback topology's host-side
   per-IP cut was a **provable no-op**: all host-networked nodes source outbound traffic
   from `127.0.0.1`, so PD kept receiving the "isolated" store's heartbeats
   (`evidence/leg0-loopback-noop-refutation.log` — the iteration-12 heartbeat oracle
   correctly refused the fake fault; Invariant B held). Fix:
   `deploy/tikv-multi-replica/docker-compose.yml` moves to a bridge network with one netns
   per node (static IPs 172.30.57.10-13), and `SymmetricPartition` applies the `-s/-d`
   DROP rules **inside the target's netns** (`docker run --network container:<node>` with
   the `deploy/tikv-multi-replica/iptables-agent/` image, mapped via the runner-exported
   `WYRD_TIER1_NETNS_MAP`, pure parser `wyrd_testkit::parse_netns_map`). Bidirectional by
   construction; a leaked HOST firewall rule is structurally impossible.
4. **Runner** — `xtask/src/faults.rs`: builds the fault-agent image, exports the
   leader-mode + netns-map + contender env, per-tier PD endpoints (Tier-2 single-node
   stays host-networked).
5. **Kept intact** (ratified iteration-12 pieces): heartbeat-freshness oracle,
   `parse_store_last_heartbeat`/`heartbeat_is_fresh`, the `WYRD_TIKV_TOOLCHAIN`-gated
   feature compile step, the non-tautological `run_ci` guard test, the coverage-labelled
   DST seed, and the no-`metadata-tikv/src`/no-`traits` invariants (verify:
   `git diff crates/metadata-tikv/src crates/traits` is empty).

## Executed evidence (see `evidence/README.md` for the full table)

- Leg 1 real leader cut → **GREEN**, `fault_materialized=true`.
- Leg 2 no-op control → **RED** on exactly `fault_materialized=false`.
- Leg 3 scratch mutation of the re-check (`evidence/leg3-mutation.diff`, never committed)
  → **RED** on exactly `no_lost_update=false`, fault still materialized.
- Leg 4 restored → **GREEN**; `metadata-tikv/src` byte-identical to branch.
- Leg 5 Tier-2 single-node real-I/O → **GREEN**.
- Gate: full `cargo xtask ci` **green** (`evidence/c4-gate-iteration13.log`); the
  iteration-12 gating exit-101 adjudicated non-reproducing
  (`evidence/c4-gate-adjudication.md`).

Legs 1+3+4 are the behavioural red→green Option B defers to — executed, not owed.

## Honest caveats for the reviewer / sign-off

- The privileged runs happened on this dev box (docker-group privileges via the fault
  agent, no root), not a dedicated Tier CI job; the logs are the evidence. A named owner
  re-running `WYRD_TIER1=1 cargo xtask metadata-tier1` on the sanctioned job reproduces
  them end-to-end (the runner now builds the agent image itself).
- Leader resolution uses "first region's leader" — correct for a fresh test cluster
  (empirically all 5 system regions co-led by one store), documented as a limitation for
  long-lived multi-region clusters.
- With the mutation applied, TiKV's own prewrite conflict still serialized the two
  barrier-released contenders in the observed run; the admitted-stale evidence came from
  the stale-CAS probe clause (deterministic, cannot be dodged). Both inputs feed the same
  `no_lost_update` signal, so the leg is red under the mutation either way.
- Tier-2 remains a single-node harness-passing run on one real machine, per 0015's
  reduced Tier-2 rung; its dedicated-machine placement stays a logistics question.
