# Brief (pointer) — issue 146 / m3.8-dst-campaign

> Plan artifact for an implementation slice whose design ALREADY lives in an
> accepted, immutable host artifact — proposal 0005 (Milestone 3 — custodians),
> PR-sequence slice **8** (`0005:541-545`). This brief POINTS at 0005 (it does not
> restate or re-decide the design; INTEGRATION §2/§6) and carries the fields the
> driver/Do parse, plus the structural-slice fields (invariant, posture, prior-art)
> this category needs. Do reads the **Planning artifact** as authoritative.
>
> This is the **verification-campaign slice** — net-new test infrastructure (a Tier-0
> property suite, testkit fault seams, Tier-1/Tier-2 xtask runners), not a behavioural
> bug fix. The invariant is stated wide and Scope names no mechanism. It passes the
> category-gated Plan-exit gate: Scope names no probe/guard/helper, and the invariant
> is not satisfiable by guarding one module (it spans the dst property suite + the
> testkit fault seams + the xtask Tier-1/Tier-2 runners across all four custodian loops).

- **Slug:** m3.8-dst-campaign
- **Planning artifact:** `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
  — §"DST and tests (the heart of M3)" (`0005:369-411`): the Tier-0 properties 1–6
  (`0005:378-403`), the Tier-1 disk-fault (dm-flakey/dm-error) + Jepsen and Tier-2
  single-node clauses (`0005:405-411`); the crate touch-points for `testkit` / `dst` /
  `xtask` (`0005:434-438`); the graduation criteria (`0005:500-502`); the PR-sequence
  DoD (slice 8, `0005:541-545`). Backed by **ADR-0009** (deterministic simulation is the
  correctness authority; every real-world discovery is promoted back into DST as a
  permanent seeded regression) and architecture §13 (the tier ladder) + §10 Q1–Q3 (the
  quality scenarios). AUTHORITATIVE — Do treats the proposal as the spec; this brief
  adds no design of its own.
- **Defect / goal:** M3's four custodian loops ship with **per-slice** tests
  (`tests/gc.rs`, `tests/scrub.rs`, and the reconstruction / rebalance tests of #144 /
  #145), but the **consolidated verification campaign** that is M3's graduation gate
  (`0005:500-502`) does **not** exist: there is no **Tier-0 custodian property suite**
  sweeping seeds for the six §13/§10 properties, no **Tier-1** disk-fault
  (dm-flakey / dm-error) + **Jepsen** consistency runners, and no **Tier-2** single-node
  kill-and-reconstruct job. The `testkit` fault model has network/disk fault-injection
  scaffolding (`FaultInjector`, `NetFault`, `DiskError` in `crates/testkit/src/lib.rs`)
  but **no D-server-kill seam** and no consolidated bit-rot/fragment-loss campaign seam
  (`0005:434-435`); `crates/dst/tests/` has only `concurrency.rs` + `network.rs`
  (M0–M2); `cargo xtask` exposes `ci/conformance/gen-vectors/dst/integration/bench`
  (`xtask/src/main.rs:31-36`) with **no** dm-flakey/Jepsen or kill-reconstruct runner.
  Realize the campaign and the testkit fault seams it needs.
- **Success criterion:** Two-posture, split by where the green is observable:
  **BINDING at C4-verify** — a **Tier-0 custodian property suite** runs **inside the
  deterministic simulator** (`0005:371-372`) and is **green across a committed seed
  sweep**, asserting all six properties (`0005:378-403`): (Q1) kill a D server →
  reconstruct to **full redundancy** in distinct failure domains with **no read errors
  during repair**; (2) **commit-point-atomic repair under crash** at **every** pipeline
  step — the chunk is always fully old-or-fully-new, **never a hybrid**, and a
  placed-but-uncommitted fragment is **collectable garbage, not corruption**; (Q2) scrub
  **detects an injected bit-flip**, excludes it, and reconstruction restores redundancy
  (a checksum-failing shard is **never** decoded); (Q3) GC reclaims **only true orphans**
  — **never** a referenced fragment, **never** a torn in-flight reader; (5) a **fenced
  stale leader** lands **no** location update; (6) **durability-plane emission** validated
  by **assertion** — under-replicated count **rises then returns to zero**, repair-queue
  depth + time-to-repair emitted and correct. The bug-finding **seeds are committed** as
  permanent regressions (ADR-0009). BINDING; the suite's file/module layout is
  ILLUSTRATIVE.
  **DEFERRED (off-Check) posture** — the **Tier-1** dm-flakey/dm-error scrub +
  checksum-path runner and the **Jepsen** consistency runner over repair, and the
  **Tier-2** single-node kill-and-reconstruct job, exist and are **green in their
  environments** (`0005:405-411`, `0005:501-502`). These are NOT demonstrable in the
  C4-verify worktree (see Verification posture).
- **Invariant to restore:** Every M3 custodian **correctness property** — the §10 Q1–Q3
  quality scenarios plus commit-point-atomicity-under-crash, fenced-leader safety, and
  durability-telemetry truth — is **continuously machine-checked in the deterministic
  simulator and seed-reproducible**, and **every real-world discovery is promoted back
  into DST as a permanent seeded regression** (the ADR-0009 / M0 rule). (Stated over the
  custodian-verification CATEGORY, not any one property; spans the dst property suite +
  the testkit fault seams + the xtask Tier-1/Tier-2 runners across all four loops — NOT
  satisfiable by guarding one module.) Source: proposal 0005 §"DST and tests"
  (`0005:369-411`), the graduation criteria (`0005:500-502`); ADR-0009; architecture
  §13 + §10.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single line, no
  maintenance branches; host suggests `feat/m3.8-dst-campaign`, `0005:508-509`)
- **Depends on:** 142, 143, 144, 145
- **Ordering note:** the campaign **verifies all four** custodian loops, so it schedules
  **last** in M3 — the binding in-flight predecessors are #144 (reconstruction, the
  subject of properties Q1/2/Q2) and #145 (rebalance); #142 (GC, property Q3) and #143
  (scrub, property Q2) are already merged. All four must be COMPLETE before this runs.
- **Conflicts with:** none
- **Surfaces:** data (DST / test infrastructure; no GUI)
- **Scope:** the M3 verification campaign and the testkit fault seams it needs, as
  proposal 0005 §"DST and tests" + the crate touch-points (`0005:434-438`) specify —
  i.e. (a) the **Tier-0 custodian property suite** in `dst` (the six properties, swept
  over seeds, seeds committed); (b) the `testkit` **bit-rot / fragment-loss fault seam**
  and the **D-server-kill seam** the suite drives; (c) the `xtask` **Tier-1**
  dm-flakey/dm-error scrub + **Jepsen** consistency runners and the **Tier-2**
  single-node kill-and-reconstruct integration. / **out of scope:** **Tier-3**
  multi-region hardware (M5, `0005:148`, `0005:375-376` "no Tier 3"); any **new
  custodian behaviour** — this slice only *verifies* the loops #142/#143/#144/#145 build,
  it adds no production logic; the deferred **replication-lag-per-zone** metric
  (single-zone has no zone pair, `0005:334-335`); dashboards / alerting / UI (ADR-0013).
- **Repro instruction:** On `main` at `../wyrd` (after #144/#145 land),
  `crates/dst/tests/` holds only `concurrency.rs` + `network.rs` — **no** custodian
  property suite; `crates/testkit/src/lib.rs` has `FaultInjector`/`NetFault`/`DiskError`
  but **no D-server-kill seam**; `cargo xtask` (`xtask/src/main.rs:31-36`) has no
  dm-flakey/Jepsen/kill-reconstruct subcommand. The six Tier-0 properties are not swept
  as a campaign and no seeds are committed for them; Tier-1/Tier-2 jobs do not exist.
- **Test file:** the suite **is** the deliverable. Tier-0 ships as
  `crates/dst/tests/custodian.rs` (the custodian property campaign, swept under
  `--cfg madsim`, with committed seeds, `0005:436`); the testkit fault seams in
  `crates/testkit/src/`; the Tier-1/Tier-2 runners as `cargo xtask` subcommands in
  `xtask/src/` (e.g. a `faults` / Jepsen / kill-reconstruct module, `0005:437-438`). Do
  names the exact files in `build-notes.md`. The Tier-0 suite is the
  red-before/green-after surface; Tier-1/Tier-2 are the deferred-posture deliverables.
- **Verification posture:** **NET-NEW coverage / infrastructure** — the property suite
  is born here (born-at-tier; no prior failing assertion to flip), so "red" is partly
  criterion-ABSENCE on new files. (a) For each **Tier-0** property, Do MUST capture a
  **demonstrated** assertion-level red where feasible — a temporary negation/stub that
  makes the property fail (e.g. negate the version-conditional CAS so a fenced stale
  leader's update lands; negate the GC grace-window so a referenced fragment is
  reclaimed; suppress the under-replicated-count decrement) — proving each assertion is
  load-bearing rather than resting red on non-existence; record these in `build-notes.md`.
  (b) **Tier-1 and Tier-2 are INERT at Check** — their green is observable only
  **off-Check**: Tier-1 dm-flakey/dm-error needs the device-mapper / loop-device block
  layer (root, real I/O) and Jepsen needs its harness; Tier-2 needs a **single real node**
  with real NVMe/fsync (`0005:409-411`). Neither runs in the C4-verify worktree (and DST
  determinism forbids containerizing them, INTEGRATION §3). WHO/WHERE confirms the
  deferred green: the **host CI Tier-1/Tier-2 jobs** ("the container/Tier-2 job green in
  CI", `0005:544`) and the **maintainer** at sign-off — declared HERE so C2/C4 land as a
  pre-declared sign-off item, not a surprise NEEDS-HUMAN. Whole-gate confirmation for the
  Tier-0 leg: `./engine/xtask.sh ci` (incl. the DST sweep) exits 0 in `$PDCA_WORKTREE`.
- **Citations expected:** Do must cite path:line on the target branch (`main`) AND the
  Planning artifact (0005 line refs, e.g. `0005:378-403`, `0005:405-411`, `0005:434-438`)
  for every design claim and change.
- **Prior-art check (triage cycles):** searched by file path — merged history / open /
  closed PRs — `crates/dst/tests/custodian.rs` does **not** exist (only `concurrency.rs`
  + `network.rs`, the M0–M2 DST); `crates/testkit/src/lib.rs` has fault-injection
  scaffolding (`FaultInjector`, `NetFault`, `DiskError`) but **no D-server-kill seam** and
  no consolidated custodian campaign seam; `cargo xtask` exposes no dm-flakey / dm-error /
  Jepsen / kill-reconstruct subcommand (`xtask/src/main.rs:31-36`, `58`). No prior or
  in-flight verification-campaign work. Net-new slice 8.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
