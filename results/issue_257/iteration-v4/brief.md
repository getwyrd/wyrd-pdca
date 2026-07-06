# Brief (pointer) — issue 257 / m4.6-tier1-jepsen-tier2

> Plan-pointer brief: the plan lives in Wyrd's **accepted proposal 0015** (INTEGRATION §6,
> superseding 0007) — this file references it and carries the driver-parsed fields, the
> slice's constraints, and the verified backend facts so Do doesn't re-derive them. Do
> reads the **Planning artifact** as the authoritative plan; this brief does not restate it.

- **Slug:** m4.6-tier1-jepsen-tier2
- **Planning artifact:** `../wyrd/docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  — authoritative. Read specifically: §"DST and tests (the heart of M4)" — the **Tier-1**
  (software-defined faults + **Jepsen**) and **Tier-2** (single owned machine) paragraphs,
  the **realism-ladder Numbering note**, and the **"compounding loop"** (promote every
  real-cluster surprise back into DST) paragraphs; §"Crate touch-points" (`testkit`,
  `xtask`); §"Suggested PR sequence" **item 6** (this slice); the **Definition of done**
  bullets on Tier-1/Tier-2 + seeded-regression; and the **Deployment prerequisite** note
  (static endpoints until L5 discovery lands). Ground it against the design corpus, read in
  place under `../wyrd` (never copied): architecture **§13.1** (DST proves the system; the
  real tiers prove the backend; "a real environment is never used to test correctness the
  simulation already covers") and **§13.4** (the realism ladder; Tier-3/multi-region does
  **not** begin until M9); **ADR-0009** (DST — correctness authority; "complements, does not
  replace" the real tiers); **ADR-0015** (the five-clause consistency contract — clause 2 is
  the load-bearing demonstration); **ADR-0006** (pin the trait with two implementations).
- **Defect / goal:** the milestone swaps the metadata backend from the deterministic
  redb/in-memory fake to a real distributed store (TiKV) behind an **unchanged**
  `MetadataStore` trait (`crates/traits/src/lib.rs:338`). Tier-0 DST proves the *commit
  protocol/system* on the deterministic backend, but it structurally **cannot** show that
  the abstractions it simulates **match the real store** — that evidence only exists on a
  real cluster under real faults. Today the tree's realism-ladder tiers exercise the **M2/M3
  chunkstore + repair path**, not the metadata swap: `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`,
  `crates/chunkstore-grpc/tests/tier2_integration.rs`, `.../tier2_kill_reconstruct.rs`,
  `crates/custodian/tests/tier1_disk_faults.rs`, and the `xtask` fault runners
  (`xtask/src/faults.rs`) all predate M4. This slice **extends the Tier-1 and Tier-2 lines
  across the redb→TiKV metadata backend swap**: prove the same system behaves correctly on
  real TiKV under real trouble, and feed every surprise back into DST.
- **Success criterion:**
  - **BINDING (demonstrable in the privileged Tier job — see Verification posture):** against
    a real containerized TiKV-backed single-zone cluster (the `deploy/` stack from slice 5 /
    #256) under software-defined faults, **all three land green**: (a) **Tier-1 integration** —
    end-to-end PUT/GET + **multi-key atomic create/rename/delete** on TiKV behind the unchanged
    trait, all-or-nothing under partition/contention; (b) **Tier-1 Jepsen consistency** — a
    nemesis injecting **real** partitions, clock skew, and process pauses validates the
    single-zone consistency clauses (**clause-2 linearizability of the commit point** and
    **exactly-one-winner under genuine concurrency**); (c) **Tier-2** — a run on **one real
    machine** (real `fsync`, real NVMe, real OS) green for honest single-node I/O semantics.
    **And** at least one behavior the real cluster surfaces that the redb fake did not model is
    **promoted back into DST as a new seeded regression, with the seed committed** (the
    compounding loop — a DoD bullet, not optional). Component/mechanism identities (`tc netem`
    / `iptables` / cgroup / `libfaketime`; the Jepsen/Elle checker; container topology) are
    **ILLUSTRATIVE**; the binding conditions are "the metadata swap holds the atomicity + the
    single-zone consistency clauses on real TiKV under real faults," "Tier-2 real-I/O is
    green," and "a real-store discovery is turned into a committed DST seed."
  - **Check-observable (the flippable red→green):** the **xtask runner dispatch/routing**
    logic for the new metadata Tier-1 (integration + Jepsen) and Tier-2 runners, and the
    **`testkit` real-TiKV fault seam** (partition/latency/pause) decision logic — pure,
    unit-testable, RED when negated, GREEN on the tree (mirroring the existing pure
    `jepsen_dispatch(..)` in `xtask/src/faults.rs:179`).
  - **DEFERRED (off-Check):** the live Tier-1/Tier-2/Jepsen green itself, which needs a
    privileged Docker host (root + `tc`/`iptables`/cgroup/`libfaketime`) and is opt-in
    (`WYRD_TIER1=1` / `WYRD_TIER2=1`) — confirmed by the privileged CI/eval Tier job, not at
    Check.
- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (INTEGRATION §2 — M4 slices target the M4 integration branch, NOT `main`; it already
  carries slices 1–4. The slice's own branch is `feat/m4.6-tier1-jepsen-tier2`, PR'd **into**
  this integration base; commit subject `feat(testkit): … (M4.6, #257)`.)
- **Depends on:** 256 (the `deploy/` multi-node TiKV/PD + fault harness this slice runs the
  Tier-1 integration + Jepsen cluster against). The **L5-discovery** variant additionally
  depends on **365** (the etcd-backed `Coordination` + gateway/custodian process-role
  prerequisite); until it lands the clusters run with **static endpoints** per proposal 0015's
  Deployment-prerequisite note, which is sufficient to prove the metadata risk.
- **Conflicts with:** — (no same-file live conflict expected; the metadata Tier-1/Tier-2 test
  targets and the `testkit` fault seam are net-new alongside the existing chunkstore-path
  tier tests).
- **Ordering note:** slice **6** of 7 in proposal 0015's PR sequence — it presupposes the
  metadata backend (slices 1–4: `metadata-tikv` commit/scan + the `server` backend selector,
  already on the base) and the `deploy/` stack (slice 5 / **#256**) to have a cluster to fault.
  The **external consistency check** (the Jepsen/Elle analysis of recorded histories) aligns
  with the **#329 / #404–#409 consistency-checker substrate** — *note that dependency, do not
  fold the substrate in*: #257 consumes/aligns-with the checker, it does not build it. The DST
  **second-implementation contract harness** is a **separate later slice** (7 / **#258**) — the
  compounding-loop seeds this slice commits feed that harness but do not author it. **Human
  override:** if #256's stack or #365's discovery is not ready, the human may accept the
  reduced-bar posture (static endpoints; Tier jobs staged) rather than block this slice's
  seam + runner + dispatch-test work.
- **Do model:** opus-xhigh
- **Difficulty:** high — the widest-surface M4 slice: a real-TiKV fault seam in `testkit`
  (`crates/testkit/src/lib.rs`), two new `xtask` runners (a TiKV integration runner and a
  Jepsen-against-TiKV runner) wiring the `deploy/` compose into CI (`xtask/src/faults.rs`
  extends the existing Tier-1 Jepsen scaffolding), new Tier-1/Tier-2 metadata test targets,
  and the DST-promotion of a discovery. The blast radius spans test/deploy scaffolding + xtask
  + testkit; it does **not** touch `traits`, `core`, `custodian`, or the metadata backend
  logic (those are proven, not re-proven here).
- **Test file:** ILLUSTRATIVE — the load-bearing behavioral targets are privileged/off-Check
  (they skip cleanly without `WYRD_TIER1`/`WYRD_TIER2` + endpoints, exactly like the existing
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:604` / `tier2_integration.rs:71`
  skip guards). New metadata-swap targets — e.g. `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs`
  and a Tier-2 metadata I/O target (paths **confirm at build**, mirroring the chunkstore-path
  precedents). The **Check-observable flippable** is a **pure unit test** over the new xtask
  runner dispatch/routing + the `testkit` fault-seam decision logic (mirroring the existing
  `jepsen_dispatch` dispatch test at `xtask/src/faults.rs:179`); Do MUST extract that logic
  pure so a real red→green is demonstrable at Check without a TiKV or a privileged host.
- **Verification posture:** DEFERRED / privileged-off-Check for the load-bearing tier green
  (like the existing tier tests), declared so C2/C4 land as pre-declared sign-off items, not
  surprise NEEDS-HUMAN. **Built AND exercised at Check:** the new xtask runners + `testkit`
  fault seam compile in the whole-tree gate, and the **pure dispatch/seam decision unit
  tests** give a demonstrated red→green (Do SHOULD supply a temporary negation proving the
  route/seam is load-bearing). **Deferred / off-Check (needs a privileged Docker host + the
  #256 stack):** the live Tier-1 integration + Jepsen + Tier-2 green, opt-in via
  `WYRD_TIER1=1` / `WYRD_TIER2=1`; NAME who confirms — the privileged CI/eval Tier job. This
  is BUILT (seam + runners + test targets exist and are exercised by the compile gate and the
  dispatch/seam unit tests at Check), its live green observable only off-Check — not an unbuilt
  deliverable. `cargo xtask ci` stays green on a machine with **no** TiKV and **no** privileged
  fault injection (the tier targets skip).
- **Citations expected:** Do must cite `path:line` on `feat/m4-production-metadata-backend`
  AND the Planning artifact for every change — the existing tier scaffolding it extends
  (`xtask/src/faults.rs` incl. the Tier-1 Jepsen constants ~`:128`–`:182` and `run_jepsen`
  `:224`; `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`,
  `.../tier2_integration.rs`, `.../tier2_kill_reconstruct.rs`;
  `crates/custodian/tests/tier1_disk_faults.rs`; `crates/testkit/src/lib.rs`;
  `deploy/README.md`, `deploy/tikv-single-node/docker-compose.yml`), the unchanged trait
  (`crates/traits/src/lib.rs:338`), and proposal 0015 §"DST and tests" / §"Crate
  touch-points" / §"Suggested PR sequence" item 6 + ADR-0009 / ADR-0015 / §13.1 / §13.4
  where they bind. Do **not** invent "verified backend facts" — cite `path:line` or mark
  "confirm at build."
- **Disposition hint:** likely-fix (implement — accepted-plan test-evidence slice behind
  Accepted ADR-0009/ADR-0015; mints no new ADR).

## Invariants to hold (from the design corpus)

- **The trait stays unchanged:** `crates/traits/src/lib.rs:338` (`MetadataStore`) is
  byte-for-byte untouched — Tier-1 integration proves "the *same system* runs on TiKV behind
  the unchanged trait" (proposal 0015 §"DST and tests"); any trait edit is a failure of M4's
  thesis.
- **DST keeps correctness authority — M4 must NOT re-prove atomicity against TiKV:** the real
  tiers prove the *backend matches the store*, not the commit protocol; "a real environment is
  never used to test correctness the simulation already covers … that is DST's job"
  (proposal 0015 §"DST and tests"; §13.1; ADR-0009 "complements, does not replace").
- **The compounding loop is mandatory, not optional:** every behavior the real cluster
  surfaces that the redb fake did not model (a txn-conflict timing, a PD timestamp-oracle
  edge, a fault shape) is **promoted back into DST as a new seeded regression wherever it
  manifests through the trait contract** (the FoundationDB/TigerBeetle pattern, §13.1) — the
  seed is committed. DST never cedes authority to the real tiers.
- **Single-zone consistency only:** Jepsen validates the **single-zone subset** — **clause 2**
  (a file's writes linearizable at the commit point) is load-bearing, **clause 1** collapses
  into zonal linearizability (one zone, no L2). **Tier-3 / multi-region does NOT begin until
  M9** (§13.4) — this slice neither builds nor ratifies cross-zone/global linearizability.
- **Static endpoints, not L5 discovery:** until the deployment prerequisite (#365) ships, the
  Tier-1/Tier-2 clusters are wired with **static endpoints** (the `--endpoints` path), which
  the proposal states is sufficient to prove the metadata risk (Deployment-prerequisite note).
- **Gate honesty:** `cargo xtask ci` / the per-fix C4-verify gate remain green with **no**
  TiKV and **no** privileged fault injection; the Tier-1/Tier-2/Jepsen targets are **additive,
  opt-in (`WYRD_TIER1=1`/`WYRD_TIER2=1`), and skip cleanly** — matching the existing tier
  tests' skip guards.

## Known NEEDS-HUMAN (expect the reviewer to flag; not blockers to building)

- **Privileged-off-Check red→green:** the load-bearing Tier-1 integration + Jepsen + Tier-2
  green is observable only in the privileged CI/eval Tier job (Docker + root +
  `tc`/`iptables`/cgroup/`libfaketime` + a TiKV cluster), not in the Check worktree. The proof
  is that job's recorded run (fault schedule, history-check verdict, seed) captured in
  build-notes; the Check-observable red→green is the pure xtask-dispatch + testkit-seam unit
  tests. A pre-declared C2/C4 sign-off item, not a surprise.
- **#256 dependency (the cluster to fault):** the metadata-swap Jepsen/integration needs the
  `deploy/` multi-node stack from slice 5 / #256. If #256 has not landed on the base, the live
  Tier jobs have no cluster; the seam + runners + dispatch tests still build. Human confirms
  the staging.
- **#365 / L5-discovery reduced bar:** the multi-node stand-up runs on **static endpoints**
  until #365 lands the etcd-backed `Coordination` + gateway/custodian roles (Deployment-
  prerequisite note). Human confirms the reduced-bar posture is acceptable for this slice.
- **#329 / #404–#409 checker substrate is a NOTED dependency, not this slice's build:** the
  external consistency check (Elle/Jepsen analysis of recorded histories) aligns with the
  consistency-checker substrate. Human confirms whether #257 consumes that substrate or ships
  its own history analysis interim — do **not** fold the substrate (#404–#409) into this slice.
- **The seeded-regression DoD may not be forceable on the first run:** if the real cluster
  surfaces **no** redb-unmodeled behavior, "promote at least one discovery to a committed DST
  seed" has nothing to promote. Human judges whether a documented known-gap seed satisfies the
  bullet, or whether it is genuinely met by a real discovery. The seed feeds the slice-7 /
  #258 DST harness but is authored here.
- **Realism-ladder vs code-taxonomy naming clash:** proposal 0015 uses the **realism ladder**
  (Tier 0 DST · Tier 1 software faults + Jepsen · Tier 2 single machine · Tier 3 multi-region)
  throughout, but the CI/code taxonomy (0004) uses a **different** scheme where "Tier-1" is the
  in-process DST/wire suite and "Tier-2" is the container integration job — both schemes appear
  in the tree (the existing `tier1_*`/`tier2_*` filenames are realism-ladder). Reviewer may be
  confused; this brief and the tests mean the **realism ladder**.
- **Jepsen tooling shape:** the historical Jepsen line is external (Clojure/JVM), but
  post-#250 the in-repo `JepsenDispatch` routes to an in-repo Rust scenario test
  (`xtask/src/faults.rs`, the `tier1_jepsen_consistency` target) rather than an external
  shell-out. Confirm at build whether the metadata-swap Jepsen leg runs Jepsen-proper or the
  in-repo scenario harness, and note it — do not assume.

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be marked
ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversarial findings: the headline evidence artifacts are hollow. Rebuild to address: 1. `meta_dispatch` routing test (xtask/src/faults.rs:917) is a tautology — it asserts the same hardcoded string literals the function returns, and never checks the routed package/`--test` target actually resolves. Make it verify routing resolves to a real, runnable target (a matching typo in literal + expectation must not pass green), or drop the self-referential assertion for a real oracle. 2. The committed "promoted regression" (crates/dst/tests/tikv_surfaced_regressions.rs) models nothing and can never go red: `AwaitingStore::commit` yields before an inner redb `commit` that has no `.await`, so under madsim's single-threaded executor no racer interleaves mid-commit — it re-proves redb's own atomicity, violating the invariant "a real environment is never used to test correctness the simulation already covers." Either model the actual TiKV await-inside-commit interleaving so the seed is genuinely load-bearing (goes red without the fix), or remove the decorative seed. `PROMOTED_SEED=17` must be asserted on, not just eprintln'd. 3. The new Jepsen leg (xtask/src/faults.rs:703 -> tier1_jepsen_metadata.rs:52) applies no nemesis: it computes fault node indices, passes them as WYRD_TIER1_NEMESIS_NODES, and the test only logs them before running ordinary concurrent CAS/read checks. It can pass without the required real partition / clock-skew / process-pause fault. Wire the computed plan through to an actual injected nemesis against the cluster. Keep the genuine artifacts (testkit `quorum_safe_max` / `SeededMetaFaults` seam tests carry a real independent oracle `survivors*2>n` and survive scrutiny). Establish a real Check-observable red->green for the dispatch/seam surface (C4-verify currently FAIL — test passes without the fix). The off-Check privileged Tier-1/2 posture remains legitimate; the on-Check evidence is what must become load-bearing. </content>
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: gating gate is FAIL and the hollow-evidence pattern iteration 1 was rejected for is relocated, not fixed. This is iteration 2 — the rebuild must resolve the gating failure AND address the adversary remarks below, not shuffle them again. Gating (blocks accept, must resolve): - C4-ci FAIL: `cargo test --workspace --exclude wyrd-dst` exit 101 (panic). A pure Check test is coupled to a not-yet-landed dependency — most likely meta_dispatch_orchestration.rs:1413 hard-reading deploy/small-multi-node/ docker-compose.yml (#256 artifact) and panicking when absent. The adversary could NOT reproduce it (re-ran clean, exit 0), so the gate is flapping FAIL/PASS on identical inputs — either a flaky test entered the tree or the gate ran against a transient/leaked-env state (e.g. WYRD_TIKV_PD_ENDPOINTS leaking past #[ignore] -> tier1_jepsen_metadata.rs:68 panics). Make the on-Check dispatch/nemesis test skip or soft-pass when the deploy compose file is absent, and make the gate deterministic. brief.md:149-152 forbids `cargo xtask ci` failing without the privileged deps. Adversary remarks that must be addressed: - Wrong-tier nemesis: the Jepsen leg pauses a PD minority (meta_dispatch.rs:95,126; faults.rs:716) but the metadata store is a SINGLE TiKV node (docker-compose.yml:147). Majority PD survives -> no data-plane partition, so exactly-one-winner holds trivially from local CAS and proves nothing under partition. Fault the TiKV data plane, not PD. - Nemesis/load not synchronized: 5s fault window is wall-clock from thread spawn (faults.rs:719), before the fresh `cargo test --features tikv` subprocess compiles/ connects (faults.rs:725) — the load likely hits a healed cluster. Add a barrier/handshake so the load provably overlaps the fault. - Pause result discarded (codex): faults.rs:716/721 ignore the `docker compose pause`/`unpause` result, so meta-jepsen can pass with the nemesis never applied. Check the result and fail loudly if injection did not happen. - MetaFault::Partition applied as `docker compose pause` (SIGSTOP) conflates partition with process-pause; ClockSkew (lib.rs:566) is defined but never wired into any runner. Wire the real fault kinds the brief's Tier-1 Jepsen expectation names. - Routing test only checks each leg's file EXISTS (meta_dispatch_orchestration.rs:62) — a leg-crossing bug (Jepsen routed to the integration scenario) stays green. Pin each leg to its own distinct scenario. - Compounding-loop seed is a Markdown doc, not an executable seed: tikv_surfaced_seeds.md seed=17 status known-gap, asserted on by nothing. The mandatory DoD wants a committed executable DST regression from a real-cluster discovery — make it real or get explicit human sign-off that the known-gap doc satisfies the DoD (§6 C5 / brief Known-NEEDS-HUMAN #5). Still-open judgment/validation rows for the next sign-off (not corrections): - T5 posture/tooling (#256/#365 static-endpoints; Jepsen-proper vs in-repo harness), and Validation fitness of the off-Check privileged Tier job. Revisit at next Check.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the binding Jepsen criterion (exactly-one-winner under real partition) is unfalsifiable on the available topology. Even the now-merged #256 `small-multi-node` stack has a single TiKV data replica (3-node PD/etcd/dserver, but one `tikv`), so the metadata store can only go *unavailable* (Err), never split-brain. The on-Check green is a compile-level flip, not behavioral. Direction for the rebuild: - Extend the TiKV data plane to a real ≥3-replica Raft group (deploy compose), and point the nemesis at a data-plane PARTITION, not a single-node pause — this is the load-bearing fix that makes the property able to go red. Deploy compose + xtask runner + testkit seam are in-scope for this slice (not on the forbidden traits/core/custodian/metadata-backend list). - Wire the real fault kinds the brief names (Partition/ClockSkew/Latency), not only Pause. - C5 compounding-loop seed: deferring the executable DST regression to #258 is acceptable, but only once a real-cluster discovery exists to promote; a known-gap hypothesis doc alone does not close the mandatory DoD bullet. - Reduced-bar/static-endpoints posture is now stale: #256 is merged, so the "pending #256" excuse no longer holds. §6 items: none ticked — all four are the reject reason or downstream of it.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Binding criterion must be a REAL Jepsen setup (the tool), not the in-repo Rust fault-injection harness that already exists — this slice re-delivers existing scaffolding under a "Jepsen" label and does not raise the bar. (Live-deployment axis explicitly set aside per sign-off.) The compounding-loop DST seed is #257's OWN deliverable: #257 is the producer of the real-store discovery, #258 only receives it (per #258's brief). So it cannot be deferred to #258 — which is itself C4-verify RED and unaccepted. Re-scope so the seed's producer step lands in #257's own binding scope. Nemesis mechanism must actually take effect when the real setup runs; current wiring is unsound (advisory, concrete): (1) Partition is an INPUT-only iptables DROP on a host-networked leader — leader keeps heart-beating outbound, so it is a no-op; (2) ClockSkew uses `date -s` on a network_mode:host container with no time namespace — skews the whole host or fails (no CAP_SYS_TIME); (3) a failed heal / panic leaks a host-wide iptables DROP with no Drop guard (blackholes the port on the CI host). Also: the `winners <= 1` assertion is insensitive to whether the partition ever materialized (passes with or without a fault).
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
