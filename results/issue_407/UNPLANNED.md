# Issue 407 — UNPLANNED (no brief.md this cycle) — **SUPERSEDED 2026-07-15**

> **SUPERSEDED by this bundle's `brief.md` (Plan, 2026-07-15).** Both blocking gaps below
> have closed since 2026-07-07: gap 1 — #257, #256 and #406 are all CLOSED on the tracker
> (#406 via getwyrd/wyrd PR #479; the #442 battery, commit 60469a4, additionally landed the
> shared fault scenario + live FDB multi-replica runner); gap 2 — the brief scopes the
> binding criterion to the Check-testable core (`xtask/tests/nemesis_orchestration.rs`,
> the established `metadata_faults.rs` flippable-seam pattern) with the live legs opt-in
> off-Check, so RED is producible on the environment Do gets. Kept for the record.

> This is **not** a brief. The driver keys off `brief.md`; its absence leaves 407
> unplanned. This note records *why*, per the Plan leaf's instruction to leave a
> listed id unplanned and say why. Decided at Plan, 2026-07-07.

**Issue:** #407 — M329.4 — partition/skew/pause nemesis over the M4 cluster (reuse #257).
Slice 4 of #329. Milestone M4.

## Why it is not briefed now — two Plan-blocking gaps

1. **Its foundation does not exist yet (unmet dependency).** 407's whole premise is to
   **reuse #257's** real multi-node M4 cluster and its `tc netem` / `iptables` partition +
   `libfaketime` clock-skew + process-pause **nemesis**, plus #256's `deploy/` stack.
   Tracker state (checked via `gh issue view`): **#257 is OPEN** with its definition of
   done unmet (no green Tier-1 Jepsen consistency run against a real containerized TiKV
   cluster under fault injection yet), and **#256 is OPEN**. The pure Check-testable core
   #257 *has* landed (`xtask/src/metadata_faults.rs` dispatch + the testkit oracles) is the
   "deferred ≠ unbuilt" half; the **live fault run** 407 layers on does not yet exist to be
   reused. 407 also runs the checked **workload/models of #406** under the nemesis — and
   #406 is only being briefed this same cycle, not yet built. So 407 sits atop both #257
   (live cluster+nemesis) and #406 (checker), neither present.

2. **Its binding criterion cannot go RED on any environment Do gets (Falsifiability).** The
   forbidden failure 407 must catch — an ADR-0015 consistency violation (torn/stale read,
   version regression, two-winners, lost create) **under a real network partition / clock
   skew / process pause on a real ≥3-node cluster** — can only be exhibited on that real
   multi-node cluster under nemesis. That environment is **off-Check by design**: `cargo
   xtask ci` (the C4-verify / merge gate) is unprivileged and container-free (ADR-0016), and
   the DST simulator "structurally cannot show" real-store fault behavior (#257's own
   framing). This is precisely the Plan-blocking Falsifiability case the brief template
   names (the #257/#365 topology-cannot-exhibit-the-failure case): briefing 407 now would
   ship a success criterion that cannot fail on the environment Do is pointed at, and a
   foundation that isn't built — burning Do cycles on an un-falsifiable, un-buildable slice.

## Recommendation — sequence, don't brief yet

Brief 407 **after** #257 lands (the real cluster + nemesis, with the live fault-injected
run green in a privileged off-Check job) **and** #406 lands (the checker models + workload +
recorder this cycle briefs). At that point 407 has a real foundation to reuse and its
criterion is falsifiable on the provisioned cluster (as an off-Check / nightly job, with a
pure Check-testable core in `cargo xtask ci` per the established `metadata_faults.rs`
pattern). Also fold in the #399 live-partition upgrade and the ADR-0024 clock-trust vs
checker-real-time-order reconciliation the issue calls out. Provisioning the ≥3-node
cluster + nemesis environment (via #257/#256) is the prerequisite to make its RED possible.
