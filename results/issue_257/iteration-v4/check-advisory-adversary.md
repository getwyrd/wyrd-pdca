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
