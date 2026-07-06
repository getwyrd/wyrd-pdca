# Brief (pointer) — issue 257 / m4.6-tier1-scenario-tier2

> Plan-pointer brief: the plan lives in Wyrd's **accepted proposal 0015** (INTEGRATION §6,
> superseding 0007) and the **accepted ADR-0039** (the testing-methodology decision) — this
> file references them and carries the driver-parsed fields, this slice's constraints, and
> the verified facts so Do doesn't re-derive them. Do reads the **Planning artifacts** as the
> authoritative plan; this brief does not restate them. This is a **re-plan after four
> rejected iterations** (v1–v4 in the bundle); the carry-forward at the end is load-bearing —
> the earlier failures trace to a framing conflict that THIS brief resolves. Read it.

- **Slug:** m4.6-tier1-scenario-tier2
- **Planning artifact:** `../wyrd/docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  (authoritative) **and** `../wyrd/docs/design/adr/0039-tier1-consistency-in-repo-scenario.md`
  (the methodology decision that governs how the "Jepsen" leg is realized). Read specifically:
  - **Proposal 0015 §"DST and tests (the heart of M4)"** — the **Tier-1** (software-defined
    faults; integration + consistency-over-the-swap), **Tier-2** (single owned machine), the
    **realism-ladder Numbering note**, the **compounding-loop** paragraph, and the
    **"Pinning the trait with the second implementation"** paragraph (which NAMES the
    await-inside-commit determinism gap this slice's seed closes); §"Crate touch-points"
    (`dst`, `testkit`, `xtask`, `deploy/`); §"Suggested PR sequence" **item 6** (this slice)
    vs **item 7** (#258 — the second-impl harness, a SEPARATE slice).
  - **ADR-0039 (Accepted)** — literal public Jepsen/Elle **does not fit** Wyrd's immutable
    single-write-per-key model (eight #250 iterations produced only vacuous histories) and is
    **deferred to #329**; the sanctioned realization is an **in-repo Rust scenario driving the
    production path** against a real containerized cluster, asserting the ADR-0015 contract
    directly. A stronger network-level partition of a *live* node is an **additive upgrade**
    (#399 for the chunkstore path), not the contract asserted.
  - Ground against the design corpus, read in place under `../wyrd` (never copied):
    architecture **§13.1** ("a real environment is never used to test correctness the
    simulation already covers … that is DST's job") and **§13.4** (the realism ladder;
    Tier-3/multi-region does **not** begin until M9); **ADR-0009** (DST — correctness
    authority; "complements, does not replace" the real tiers); **ADR-0015** (the consistency
    contract — clause 2 is the load-bearing single-zone demonstration); **ADR-0006** (pin the
    trait with two implementations); **ADR-0016** (privileged tiers stay out of `cargo xtask
    ci`).

- **Defect / goal:** M4 swaps the metadata backend from the deterministic redb/in-memory fake
  to real distributed TiKV behind an **unchanged** `MetadataStore` trait
  (`crates/traits/src/lib.rs`). Tier-0 DST proves the *commit protocol / system* on the
  deterministic backend, but structurally **cannot** show that the abstractions it simulates
  **match the real store** — that evidence only exists on a real cluster under real faults.
  Today's realism-ladder tiers exercise the **M2/M3 chunkstore + repair path**
  (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`, `.../tier2_integration.rs`,
  `crates/custodian/tests/tier1_disk_faults.rs`, `xtask/src/faults.rs`), **not** the metadata
  swap. This slice (**proposal 0015 PR-sequence item 6**) extends the Tier-1 (integration +
  consistency-over-the-swap, realized as an **in-repo Rust scenario** per ADR-0039) and Tier-2
  lines across the redb→TiKV metadata backend swap, driving the **real production
  `metadata-tikv` commit path behind the unchanged trait**, and authors the **first
  compounding-loop DST seed** as this slice's own producer deliverable.

- **Success criterion:** three layers — one BINDING and demonstrable **at Check**, one BINDING
  but DEFERRED to the privileged Tier job, one explicitly OUT.
  - **BINDING, at-Check (the flippable red→green — this is the load-bearing on-Check evidence
    the four earlier iterations lacked):**
    1. A committed **compounding-loop DST seed** modelling a **TiKV-shaped `await`-inside-`commit`**
       (a commit that suspends on network I/O, which redb never does): it is **RED under the
       current "commit is internally synchronous / no `await` inside" determinism assumption**
       and **GREEN once the newly-reachable interleaving is exercised** in the deterministic
       harness. It runs in **madsim — no TiKV, no privileged host** — so it is a genuine
       red→green at Check. Its load-bearing assertion is that the **trait contract survives the
       newly-reachable interleavings** the synchronous-commit assumption wrongly excluded — it
       **MUST NOT** assert "exactly-one-winner holds" (that re-proves atomicity DST already
       owns; see Invariant). Seed committed with its `MADSIM_TEST_SEED` **asserted on**, not
       `eprintln`'d.
    2. The **pure decision logic** for the new metadata Tier runners — the `xtask`
       dispatch/routing AND the **fault-effect oracle** (the logic that decides "did the
       injected fault actually take effect") and the `testkit` fault-seam quorum/plan
       arithmetic — is RED when negated, GREEN on the tree, mirroring the existing pure
       `jepsen_dispatch(..)` (`xtask/src/faults.rs`). No tautologies (an independent oracle,
       not the same literal the function returns — the iter-1 defect).
  - **BINDING, DEFERRED off-Check (confirmed by the privileged CI/eval Tier job — see
    Verification posture):** against a real containerized **≥3-replica TiKV Raft group** (the
    `deploy/` stack) under software-defined faults, the **in-repo Rust scenario** legs land
    green: (a) **Tier-1 integration** — end-to-end PUT/GET + **multi-key atomic
    create/rename/delete** on the real `metadata-tikv` path behind the unchanged trait,
    all-or-nothing across the fault; (b) **Tier-1 consistency-over-the-swap** — the ADR-0015
    single-zone contract (**read-after-commit, no torn/stale reads, commit-point atomicity that
    converges exactly once across the heal**) holds under a fault that **provably materialized**;
    (c) **Tier-2** — a run on **one real machine** (real `fsync`, real NVMe, real OS) green for
    honest single-node I/O. Every leg carries a **positive fault-effect oracle** (RED if the
    fault was a no-op) and **self-heals on every path** (Invariant B). Fault mechanisms
    (`tc netem` / `iptables` / cgroup / `libfaketime` / `docker pause`; container topology) are
    **ILLUSTRATIVE**; the binding conditions are "the metadata swap holds the ADR-0015
    single-zone contract on real TiKV under a fault that provably took effect," "Tier-2 real-I/O
    is green," and "the fault materialized (the leg cannot pass with the fault absent)."
  - **OUT (deferred, per ADR-0039):** the **literal public Jepsen/Elle credibility artifact** —
    deferred to **#329**; this slice realizes the consistency leg as an in-repo Rust scenario
    and MUST NOT re-attempt literal Jepsen (iter-4 demanded it; ADR-0039 forbids it here).

- **Invariant to restore:**
  - **DST determinism rationale must be sound for the backend it proves.** The `dst`
    concurrency harness argues each `commit()` is "internally synchronous (one redb write
    transaction, no `await` inside)"; this is **false for a TiKV commit that awaits network
    I/O**, so interleavings redb never exhibits are unmodelled. The deterministic harness must
    exercise those interleavings and the committed seed must be **red under the uncorrected
    assumption** (proposal 0015 §"Pinning the trait with the second implementation"; ADR-0009).
  - **DST keeps correctness authority — M4 must NOT re-prove atomicity against TiKV.** The real
    tiers (and this seed) prove the *backend matches the store*, not the commit protocol: "a
    real environment is never used to test correctness the simulation already covers … that is
    DST's job" (§13.1; ADR-0009 "complements, does not replace"). Concretely: no leg or seed may
    rest its bindingness on "exactly-one-winner goes red" — a minority partition against a
    linearizable store never changes that outcome, and `metadata-tikv/src` is out of this slice,
    so such an assertion is unfalsifiable-here by construction (the recurring iter-1..4 hollow
    flip).
  - **(Invariant B — fault-soundness, accepted with the human at Plan.)** A fault-injection leg
    is evidence **only if the fault provably materialized** — every leg carries a positive
    fault-effect oracle that is **red when the fault is a no-op** — **and it never leaks host
    state** (it self-heals on every path, including panic/interrupt/failed-heal; no host-wide
    mutation survives the run). This is the invariant the iter-2/3/4 nemesis bugs violated
    (asymmetric no-op partition; whole-host clock skew; leaked `iptables` rule; assertion
    insensitive to whether the fault fired).
  - **The trait stays unchanged** (`crates/traits/src/lib.rs`) — byte-for-byte; any trait edit
    is a failure of M4's thesis (proposal 0015; ADR-0006).

- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (INTEGRATION §2 — M4 slices target the M4 integration branch, NOT `main`; it already carries
  slices 1–5, incl. the #256 `deploy/` stack). The slice's own branch is
  `feat/m4.6-tier1-scenario-tier2`, PR'd **into** this integration base; commit subject
  `feat(testkit): … (M4.6, #257)`.
- **Conflicts with:** 258
- **Ordering note:** #257 and #258 both edit `crates/dst/` (this slice authors the
  await-inside-commit seed; #258 builds the second-implementation contract/property harness that
  folds the seed in). They must **never** build blind on the same base → `Conflicts with: 258`.
  The real dependency runs **#258 → #257** (#258 builds on #257's seed), so this slice sets **no**
  `Depends on`; the ordering constraint is recorded on #258's brief. #256 (the `deploy/` stack)
  is **already merged** on the integration base — no longer a dependency. The consistency
  **checker substrate** (#329 / #404–#409) and the **network-partition-of-a-live-node** upgrade
  (#399-style) are NOTED, deferred couplings — this slice consumes/aligns-with, it does not build
  them.
- **Surfaces:** data (backend/test infrastructure + DST; no frontend).
- **Difficulty:** high — the widest-surface M4 slice: a real-TiKV fault seam in `testkit`, new
  `xtask` metadata Tier runners + pure dispatch/fault-effect-oracle logic, a ≥3-replica TiKV
  Raft-group `deploy/` compose, the in-repo scenario tier tests, **and** an await-inside-commit
  DST seed. Blast radius spans `dst` + `testkit` + `xtask` + `deploy/` + new test targets; it
  does **not** touch `traits`, `core`, `custodian`, or `metadata-tikv/src` (proven, not
  re-proven here).
- **Do model:** opus-xhigh
- **Scope:** extend the realism-ladder **Tier-1** (integration + consistency-over-the-swap,
  realized as an **in-repo Rust scenario** per ADR-0039) and **Tier-2** lines across the
  redb→TiKV metadata backend swap — driving the real production `metadata-tikv` commit path
  behind the unchanged trait against a real ≥3-replica TiKV Raft group under software-defined
  faults, each leg carrying a fault-effect oracle and self-healing — **and** author this slice's
  producer deliverable: **one compounding-loop DST seed** exposing the TiKV-shaped
  await-inside-commit interleaving. / **out of scope:** `metadata-tikv/src`, `traits`, `core`,
  `custodian` (proven, not re-proven); the second-implementation contract/property harness
  (**#258**); the literal public Jepsen/Elle credibility artifact (**#329**); the
  network-partition-of-a-live-node nemesis upgrade if it needs its own ADR (deferred — see
  NEEDS-HUMAN). Do MUST NOT name/seat a specific nemesis mechanism as the deliverable — restore
  the invariants above; the mechanism is Do's to choose within Invariant B.
- **Repro instruction:** on `feat/m4-production-metadata-backend`, the metadata realism-ladder
  tiers do not exist and the `dst` concurrency harness assumes synchronous commit (no
  await-inside-commit interleaving is modelled). At Check: the DST seed is red under the
  uncorrected assumption and green once the interleaving is exercised; the pure
  dispatch/fault-effect-oracle/seam tests are red when negated, green on the tree. The live
  tier legs skip cleanly without `WYRD_TIER1`/`WYRD_TIER2` + endpoints (like the existing
  chunkstore-path tier tests).
- **Test file:** the **at-Check binding, flippable** artifact is the **DST seed** —
  `crates/dst/tests/tikv_await_commit_interleaving.rs` (path confirms at build) — plus the
  pure `xtask` dispatch/fault-effect-oracle unit tests (`xtask/tests/…`, mirroring
  `xtask/src/faults.rs`'s existing `jepsen_dispatch` test) and the `testkit` fault-seam
  arithmetic tests. The live tier targets (e.g. `crates/metadata-tikv/tests/tier1_*` and a
  Tier-2 metadata I/O target; paths confirm at build) are **privileged/off-Check** and skip
  cleanly, mirroring `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` /
  `tier2_integration.rs`.
- **Verification posture:** MIXED, declared so C2/C4/C5 land as pre-declared sign-off items
  rather than surprise NEEDS-HUMAN.
  - **Built AND exercised at Check (the honest red→green):** the **DST await-inside-commit seed**
    (madsim, no privileged host — a genuine flip: red under the uncorrected determinism
    assumption, green once the interleaving is modelled) and the **pure** dispatch /
    fault-effect-oracle / fault-seam decision logic (red when negated). This is the load-bearing
    on-Check evidence the earlier iterations lacked. `cargo xtask ci` stays green with **no**
    TiKV and **no** privileged fault injection (the tier targets skip; gate honesty).
  - **DEFERRED / off-Check (needs a privileged Docker host — root + `tc`/`iptables`/cgroup/
    `libfaketime` + a ≥3-replica TiKV cluster):** the live Tier-1 integration + consistency +
    Tier-2 green, opt-in via `WYRD_TIER1=1` / `WYRD_TIER2=1`, confirmed by the **privileged
    CI/eval Tier job** (name who confirms at sign-off). This is BUILT (seam + runners + scenario
    tier tests compile in the whole-tree gate and are exercised by the pure decision tests) and
    its live green observable only off-Check — **not** an unbuilt deliverable. Do SHOULD capture
    a *demonstrated* red where feasible (a temporary negation proving the fault-effect oracle is
    load-bearing — i.e. the leg goes red when the fault is a no-op).
- **Production reach:** the live consistency leg runs against a **static-endpoints** cluster
  (the `--endpoints` path) until L5 discovery (#365) lands — proposal 0015's Deployment-
  prerequisite note states this is sufficient to prove the metadata risk. The in-repo scenario
  drives the **real production commit path** (`metadata-tikv` behind the trait), not a
  test-double — so the seam is load-bearing at the live run; only endpoint *discovery* is stood
  in for.
- **Citations expected:** Do must cite `path:line` on `feat/m4-production-metadata-backend` AND
  the Planning artifacts for every change — the existing tier scaffolding it extends
  (`xtask/src/faults.rs` incl. the `jepsen_dispatch` pattern; `crates/chunkstore-grpc/tests/
  tier1_jepsen_consistency.rs`, `.../tier2_integration.rs`; `crates/custodian/tests/
  tier1_disk_faults.rs`; `crates/testkit/src/lib.rs`; `deploy/small-multi-node/
  docker-compose.yml`), the `dst` concurrency harness whose determinism rationale the seed
  corrects (`crates/dst/…concurrency.rs`), the unchanged trait (`crates/traits/src/lib.rs`),
  and proposal 0015 §"DST and tests" / §"Pinning the trait…" / PR-sequence item 6 + ADR-0039 /
  ADR-0009 / ADR-0015 where they bind. Do **not** invent "verified backend facts" — cite
  `path:line` or mark "confirm at build."
- **Disposition hint:** likely-fix (implement — accepted-plan test-evidence slice behind
  Accepted proposal 0015 / ADR-0039 / ADR-0009 / ADR-0015; mints no new ADR unless the
  metadata-nemesis question below is answered "yes").

## Known NEEDS-HUMAN (expect the reviewer to flag; not blockers to building)

- **Privileged-off-Check binding legs:** the live Tier-1 integration + consistency + Tier-2
  green is observable only in the privileged CI/eval Tier job, not in the Check worktree. The
  Check-observable red→green is the DST seed + the pure dispatch/fault-effect-oracle/seam tests.
  A pre-declared C2/C4 sign-off item, not a surprise.
- **Metadata-store nemesis methodology — possible ADR refinement (architecture-board
  authority).** ADR-0039 rules `docker pause` ≡ partition **for the M3 D-servers**, because
  they are *dumb storage that initiate no commits*. TiKV is **not** dumb storage — it runs its
  own Raft consensus, so a live-but-partitioned node is not observably equivalent to a paused
  one. This slice follows ADR-0039's in-repo-scenario methodology and Invariant B (the fault
  must provably materialize) with a software-defined fault; whether the metadata leg needs a
  **new metadata-specific ADR refinement** (network-partition-of-a-live-node vs process-freeze)
  or can be carried as an additive follow-up (#399-style) is the human's / architecture board's
  call. Do NOT author a new ADR unless the human directs it.
- **Static-endpoints reduced bar (#365):** the multi-replica cluster runs on **static
  endpoints** until #365 lands L5 discovery (Deployment-prerequisite note). Human confirms the
  reduced-bar posture is acceptable for this slice.
- **Realism-ladder vs code-taxonomy naming clash:** proposal 0015 and the existing
  `tier1_*`/`tier2_*` filenames use the **realism ladder** (Tier 0 DST · Tier 1 software faults ·
  Tier 2 single machine · Tier 3 multi-region); the 0004 CI/code taxonomy uses a different
  "Tier-1/Tier-2" scheme. This brief and the tests mean the **realism ladder**.

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be marked
ready before sign-off accepts.

## Carry-forward — why v1–v4 were rejected, and what THIS re-plan changes

Four prior attempts are preserved in `iteration-v1/`…`iteration-v4/`. Do **must not** re-attempt
the rejected shapes. The rejections converged on a **framing conflict now resolved at Plan**:

- **v1:** tautological routing test (asserted the same literal it returned); a decorative DST
  "seed" that re-proved redb's own atomicity (no racer interleaved mid-commit under madsim);
  nemesis computed but never applied. → **Fixed here:** independent oracles required (no
  tautology); the seed models the **await-inside-commit** interleaving so it is genuinely
  load-bearing; Invariant B requires the fault to provably materialize.
- **v2:** gating gate FAIL + hollow-evidence relocated; wrong-tier nemesis (paused a PD minority
  while the data plane was a single node); nemesis/load not synchronized; pause result discarded.
  → **Fixed here:** ≥3-replica Raft group; fault-effect oracle (Invariant B); gate honesty
  required (`cargo xtask ci` green with no TiKV/privilege).
- **v3:** single TiKV data replica could only go *unavailable*, never split-brain → the property
  was unfalsifiable and the on-Check green was a compile-level flip. → **Fixed here:** the
  binding on-Check flip is the **DST seed**, not a compile flip; the live legs are explicitly
  DEFERRED and assert the **ADR-0015 contract under a materialized fault**, not exactly-one-winner.
- **v4:** reviewer demanded a **literal Jepsen tool** and a network-partition-of-a-live-node, and
  ruled the seed cannot be deferred to #258. → **Resolved at Plan (with the human):** ADR-0039
  **defers literal Jepsen to #329** and sanctions the in-repo scenario — the v4 demand
  contradicted an Accepted ADR; and the seed is now **#257's own producer deliverable** (the
  await-inside-commit seed), with #258 depending on #257. The three concrete v4 nemesis bugs
  (asymmetric no-op partition; whole-host clock skew; leaked `iptables` rule) are exactly what
  **Invariant B** forbids.

**Do the end result the Success criterion names — do not re-litigate the framing.** If the
metadata-nemesis ADR question (above) blocks a sound live leg, that is a pre-declared NEEDS-HUMAN,
not a reason to weaken the at-Check DST seed or the fault-effect oracle.
</content>
</invoke>

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected (iter-5): the at-Check evidence is real but its bindingness still leans on constructs the patch authored to pass, and the live runner can false-green. Fix all three before this counts as the honest red→green the four prior iterations lacked. (1) Self-referential flip. The load-bearing red→green (`interleavings_observed >= 1`, `crates/dst/tests/tikv_await_commit_interleaving.rs:244-249`) is produced by passing `false` to `SimTikvMetadataStore::with_await_inside_commit` (`crates/dst/src/sim_tikv.rs:168`) — a constructor arg added in this same patch. Nothing in production code (`wyrd_core`, `metadata-redb`, the `concurrency.rs` harness) fails before the patch and passes after; the "flip" is a boolean the fixture reads about itself (the v3-reject shape). Make the red behavioural against a real production/harness assumption, not a self-toggled fixture property; confirm the `run-verify.sh` red is behavioural, not mere file-absence of a brand-new test. (2) Vacuous assertions. `assert_backend_equivalence` (`tikv_await_commit_interleaving.rs:217-233`, called :256) compares await-on vs await-off runs of the SAME store — both re-check preconditions at the commit point, so no mode can produce a lost update; the "No lost update / observationally equivalent" criterion is decorative. `assert_contract_survived` (:178-215) can fail only if the hand-written in-patch CAS is buggy and re-derives atomicity the store's own coupled bookkeeping guarantees — the very thing the Invariant says the seed must not rest on. Either give these assertions a mode that can actually violate them (e.g. a prewrite-trust toggle that skips the commit-point re-check) or drop the decorative criteria; do not present near-definitional checks as binding fault-effect oracles. (3) Live runner false-greens (codex). `xtask/src/faults.rs:659` runs the tier tests with `cargo test -p wyrd-metadata-tikv --test ...` but never `--features tikv`, so each live test takes its `#[cfg(not(feature = "tikv"))]` skip branch and returns success without connecting to TiKV (`crates/metadata-tikv/tests/tier1_metadata_integration.rs:103`). And `faults.rs:662` exports only `WYRD_TIKV_PD_ENDPOINTS`, never injecting a minority fault or exporting `WYRD_METADATA_FAULT_BEFORE/AFTER/REPLICAS/PARTITIONED`, so the consistency leg treats the missing descriptor as a skip (`tier1_metadata_consistency.rs:75`) and `run_metadata_tier1` reports the consistency-over-the-swap leg passed with no fault materialized. Wire `--features tikv` and the fault-descriptor injection so the privileged job actually drives the binding leg under a materialised fault, and cannot report green on a clean skip. Also fold in the advertised-but-unwired `metadata_leg_passes`/`MetadataLegVerdict` oracle so it derives from real observations rather than hand-set struct fields. Not re-litigated here (standing human calls owed at the next Check, not blocking the rebuild): C5 sim-fidelity acceptance, the ADR-0039 partition-of-a-live-Raft-node methodology question (T5), the static-endpoints reduced bar until #365. The `partition_materialized` oracle (`crates/testkit/src/lib.rs:415`) and the quorum arithmetic held under adversarial probing — keep them.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected (plan-level): after six iterations the binding at-Check red->green is still vacuous, and the vacuity is STRUCTURAL — not a Do defect another rebuild can fix. The binding assertion asks "does the correct model survive the interleaving?" but correctness is encoded as a mode flag whose "correct" branch hard-codes `admit = commit_point_ok` (sim_tikv.rs:366), so the stale-commit oracle reduces to `admitted_stale = commit_point_ok && !commit_point_ok` (sim_tikv.rs:374) — identically false by boolean algebra, before any schedule runs. The builder even documents this (patch lines 371-372). The v6 PrewriteTrust "negative control" only proves a deliberately-broken, patch-authored branch is broken; it does not give the positive binding claim teeth. A self-authored DST sim whose correctness is a "good branch" is structurally incapable of a non-vacuous BINDING assertion about its own correct path ("the branch I wrote to be correct is correct" is always a tautology). Do has now re-instantiated this same shape five times; the fix is to redefine WHAT the binding at-Check evidence is — a Plan decision. What the re-plan must decide (redefine the binding evidence, don't ask Do to retry): 1. Separate the correctness from the check so an INDEPENDENT system can genuinely fail. Two candidate directions for the plan to choose between: (a) Drive the REAL production commit path (metadata-tikv/src CAS, or wyrd_core::write) under the interleaving, so the assertion checks code written without knowledge of the test — a real missing commit-point re-check then produces a real lost update the oracle catches; or (b) Make the sim's commit logic GENERAL (no correct-vs-broken mode flag) and let the schedule itself determine whether a stale commit slips through, so a subtly-wrong scheduling model can leak one and the assertion has teeth. 2. Re-decide whether the binding bar for this slice belongs at Check at all, or moves to the live Tier-1/Tier-2 legs (currently pre-declared off-Check), given that the at-Check DST seed cannot self-certify sim-fidelity. 3. Fold the standing Plan-level calls into the re-brief: C5 sim-fidelity acceptance; the ADR-0039 partition-of-a-live-Raft-node methodology question (the concrete mechanism ships an ASYMMETRIC iptables DROP, faults.rs:2107-2109 — the shape Invariant B forbids); static-endpoints reduced bar until #365. Trivial/mechanical (note for whoever rebuilds, but NOT the reason for rejection): - C4-ci is red only on `cargo fmt --all -- --check` at one over-length match arm (crates/dst/tests/sim_tikv_await_commit.rs:84). A `cargo fmt --all` clears it. Keep (held under adversarial probing, do not discard in the re-plan): - testkit quorum arithmetic and partition_materialized are genuine non-tautological oracles. - iter-5 --features tikv + fault-descriptor-injection false-green is genuinely fixed. Also carry (live-runner issues to address once the binding evidence is redefined): - Live consistency oracle collapses read_after_commit_holds and converged_once into one scenario.is_ok() exit bit (faults.rs:771,773) — restore independent signals. - Heal probes only 20182 though the fault DROPs 20162+20182 (faults.rs:761); no store-port readiness wait (races cluster formation); assertions checked before heal. - Reduced integration leg: only create/read/duplicate-create; brief asks multi-key create/rename/delete all-or-nothing across the fault.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo fmt --all -- --check` failed with exit status: 1
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
