# Iteration-13 evidence — the executed behavioural red→green (brief Success criterion (d))

Live runs executed **in Do, before Check submission** (the live-evidence-first rule this
brief revision introduced), on this box: docker via the `wyrd-iptables:local` fault agent
(`deploy/tikv-multi-replica/iptables-agent/`), `deploy/tikv-multi-replica` bridge topology
(one netns per node), pingcap pd/tikv v8.5.1, rustc 1.96.0. Every leg's full test output is
in the named log; exit codes are recorded as the last line of each log.

## The four-leg mutation acceptance run (+ Tier-2)

| Leg | Log | Configuration | Expected | Observed |
|---|---|---|---|---|
| 1 | `leg1-real-cut.log` | real symmetric cut of the **region leader** (resolved from PD at runtime), applied inside the leader's netns; 2 concurrent CAS contenders | GREEN, `fault_materialized=true` | **GREEN** (exit 0, 25s; leader `172.30.57.13`) |
| 2 | `leg2-noop-control.log` | no cut configured (no-op negative control) | RED, `fault_materialized=false` the only failing clause | **RED** (exit 101; `{read_after_commit: true, converged_once: true, fault_materialized: false, no_lost_update: true}`) |
| 3 | `leg3-mutated-recheck.log` | `get_for_update` re-check neutralized by the scratch mutation `leg3-mutation.diff` (**never committed**); real leader cut | RED, `no_lost_update=false` the only failing clause | **RED** (exit 101; `{read_after_commit: true, converged_once: true, fault_materialized: true, no_lost_update: false}` — the stale CAS probe was admitted and caught) |
| 4 | `leg4-restored-green.log` | mutation reverted; `git diff crates/metadata-tikv/src` empty (byte-identical to branch) | GREEN | **GREEN** (exit 0, 29s; leader `172.30.57.12`) |
| 5 | `leg5-tier2-real-io.log` | Tier-2 single-node real-I/O (`deploy/tikv-single-node`, host networking) | GREEN | **GREEN** (exit 0) |

Leg 3 is the executed form of the iter-8 acceptance criterion ("perturbing the
`get_for_update` re-check must flip an artifact"): a real commit-point regression flips the
live leg red on exactly the `no_lost_update` clause, with the fault-effect gate proving the
cut was real at the same time. Legs 1+3+4 together are the behavioural red→green that
Option B defers to; leg 2 is the Invariant-B no-op control.

## The topology refutation this iteration surfaced and fixed

`leg0-loopback-noop-refutation.log`: the iteration-12 "distinct loopback IP" topology run
under a real host-side `iptables -s/-d 127.0.0.2` cut. PD kept receiving the "isolated"
store's heartbeats (all host-networked nodes source their outbound connections from
`127.0.0.1`, so a per-IP cut never matches them) — the leg failed HONESTLY with
`fault_materialized: false` while data signals stayed green. Two consequences:
1. The heartbeat fault-effect oracle (the iteration-12 fix) is empirically confirmed as a
   no-op detector — Invariant B held.
2. Every v7–v12 loopback-topology partition was structurally a no-op; the bridge-network /
   netns-cut topology in this patch is the fix (each node owns its netns, so its traffic
   carries its own IP; the cut runs inside the target's netns and cannot leak host state).

## Gate artifacts

- `c4-gate-adjudication.md` + `c4-gate-rerun.log`: the iteration-12 gating C4-ci exit-101
  adjudicated as non-reproducing/environmental (three green re-runs, fingerprinted, one
  deliberately with a PD/TiKV cluster squatting the loopback ports).
- `c4-gate-iteration13.log`: full `cargo xtask ci` on the iteration-13 tree (fingerprinted).

## Post-run box hygiene

Host firewall verified clean after all legs (`iptables -S` — no `127.0.0.*` /
`172.30.57.*` rules); the tier-1 and tier-2 compose stacks torn down (`down -v`); the
unrelated `client-rust-test` cluster this run had to displace (port overlap) restarted.
