# Brief (pointer) — issue 257 / m4.6-real-commit-over-madsim-tikv

> Plan-pointer brief: the plan lives in Wyrd's **accepted proposal 0015** (INTEGRATION §6,
> superseding 0007) and **accepted ADR-0039** (the testing-methodology decision) — this file
> references them and carries the driver-parsed fields, this slice's constraints, and the
> verified facts so Do doesn't re-derive them. Do reads the **Planning artifacts** as the
> authoritative plan; this brief does not restate them.
>
> **This is a re-plan after SIX rejected iterations (v1–v6 in the bundle).** The carry-forward
> at the end is load-bearing. The six failures were not Do defects — they were **one structural
> defect the earlier briefs baked in**, which THIS re-plan removes at Plan. Read the
> carry-forward before building; do **not** re-instantiate the rejected shape.

- **Slug:** m4.6-real-commit-over-madsim-tikv
- **Planning artifact:** `../wyrd/docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  (authoritative) **and** `../wyrd/docs/design/adr/0039-tier1-consistency-in-repo-scenario.md`
  (the methodology decision governing how the "Jepsen"/consistency leg is realized). Read specifically:
  - **Proposal 0015 §"DST and tests (the heart of M4)"** — **Tier-1** (software-defined faults;
    integration + consistency-over-the-swap), **Tier-2** (single owned machine), the
    **realism-ladder Numbering note**, the **compounding-loop** paragraph, and the
    **"Pinning the trait with the second implementation"** paragraph (which NAMES the
    await-inside-commit determinism gap this slice closes); §"Crate touch-points"
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
    authority; "complements, does not replace"); **ADR-0015** (the consistency contract —
    clause 2 is the load-bearing single-zone demonstration); **ADR-0006** (pin the trait with
    two implementations); **ADR-0016** (privileged tiers stay out of `cargo xtask ci`). The
    **madsim cfg-alias precedent** this slice extends: proposal 0004 §"DST and integration
    tests" + `crates/chunkstore-grpc/Cargo.toml` (real `GrpcChunkStore` wire code run over
    `madsim-tonic` under `--cfg madsim`, no source change).

- **Defect / goal:** M4 swaps the metadata backend from the deterministic redb/in-memory fake to
  real distributed TiKV behind an **unchanged** `MetadataStore` trait (`crates/traits/src/lib.rs`).
  Tier-0 DST proves the *commit protocol / system* on the deterministic backend, but structurally
  **cannot** show the abstractions it simulates **match the real store** — that evidence only
  exists when the **real production commit code** runs under interleavings the redb fake never
  exhibits. Concretely, the DST concurrency harness argues each `commit()` is "internally
  synchronous (one redb write transaction, no `await` inside)"
  (`crates/dst/tests/concurrency.rs:3-4`) — **false** for `TikvMetadataStore::commit`, which
  `await`s network I/O between the precondition re-read (`get_for_update`) and the terminal
  `commit()` (`crates/metadata-tikv/src/lib.rs:540+`), so interleavings TiKV admits are unmodelled.
  This slice (**proposal 0015 PR-sequence item 6**) extends the Tier-1 (integration +
  consistency-over-the-swap, realized as an in-repo Rust scenario per ADR-0039) and Tier-2 lines
  across the redb→TiKV swap, and authors the **first compounding-loop DST seed** exercising the
  await-inside-commit interleaving — **against the real `metadata-tikv` commit code**, not a
  self-authored fake.

- **Success criterion:** three layers — one BINDING and demonstrable **at Check**, one BINDING but
  DEFERRED to the privileged Tier job, one explicitly OUT. **The binding at-Check layer is
  redefined vs v1–v6 (see Invariant + carry-forward) — its oracle must be code that was not
  written to pass it.**

  - **BINDING, at-Check (the genuine, independent red→green the six iterations lacked):**
    1. A committed **compounding-loop DST seed** that drives the **real, unchanged production
       `TikvMetadataStore::commit`** (the code under test) over a **deterministic, third-party
       TiKV simulation** — `madsim-tikv-client` cfg-aliased as `tikv-client` under `--cfg madsim`,
       the exact `madsim-tonic` pattern `crates/chunkstore-grpc/Cargo.toml` already uses (proposal
       0004; ADR-0009), **added with NO `metadata-tikv/src` edit** (a `Cargo.toml` cfg-alias plus
       whatever madsim **runtime** integration a raw async store needs — see the feasibility note
       below; the chunkstore precedent aliases only a *network client* within an already-standing
       DST harness, so this is more than one line).** Under an adversarial madsim schedule that interleaves the
       await-inside-commit window, the seed asserts the **ADR-0015 commit-point contract holds**
       (no lost update / stale commit slips past the precondition re-check). Because the oracle is
       an **independent server model** and the commit logic is **production code written without
       knowledge of this test**, the assertion has teeth: a *real* missing/mis-ordered commit-point
       re-check produces a *real* lost update the seed catches. **RED→GREEN must be behavioural
       against production code**, demonstrated by a *temporary, discarded* perturbation of the
       commit ordering (e.g. trusting the prewrite read instead of the in-txn re-check) going red —
       **NOT** by toggling a patch-authored `CommitMode` flag, NOT by file-absence of a new test
       (the iter-1..6 tautology). Seed committed with its `MADSIM_TEST_SEED` **asserted on**.
       - **Fallback, declared now (Option B):** if `madsim-tikv-client` has **no release tracking
         `tikv-client = "0.4"`**, or its simulated TiKV **does not model the await-inside-commit
         percolator write-conflict** (both are confirm-at-build — see NEEDS-HUMAN), direction (a)
         via this route is blocked. Then the binding correctness bar **moves to the live Tier-1
         legs** (below) and the at-Check binding evidence becomes the **pure oracles** (2) **plus
         the DST seed redefined as a coverage/determinism-gap artifact** — asserting the real,
         checkable fact "the `concurrency.rs` synchronous-commit rationale is unsound; here is a
         newly-reachable interleaving" — which **MUST NOT** carry a "self-authored correct branch
         survives" tautology. Do MUST report at build which layer is live so Check knows which bar
         it is grading.
    2. The **pure decision logic** for the metadata Tier runners — the `xtask` dispatch/routing,
       the **fault-effect oracle** ("did the injected fault actually take effect"), and the
       `testkit` fault-seam quorum/reachability arithmetic — is RED when negated, GREEN on the
       tree, mirroring the existing pure `jepsen_dispatch(..)` (`xtask/src/faults.rs`). **These
       survived adversarial probing across v5/v6 — keep them; do not regress them.** No tautologies
       (an independent oracle, not the literal the function returns — the iter-1 defect).

  - **BINDING, DEFERRED off-Check (confirmed by the privileged CI/eval Tier job — see Verification
    posture):** against a real containerized **≥3-replica TiKV Raft group** (the `deploy/` stack)
    under software-defined faults, the in-repo Rust scenario legs land green: (a) **Tier-1
    integration** — end-to-end PUT/GET + **multi-key atomic create/rename/delete** (not merely
    create/read/duplicate-create — the v6 reduced leg is insufficient) on the real `metadata-tikv`
    path behind the unchanged trait, all-or-nothing across the fault; (b) **Tier-1
    consistency-over-the-swap** — the ADR-0015 single-zone contract (**read-after-commit, no
    torn/stale reads, commit-point atomicity that converges exactly once across the heal**), with
    **read-after-commit and exactly-once-convergence carried as INDEPENDENT signals** (not one
    collapsed `scenario.is_ok()` bit — the v6 defect), asserted **across the heal** (heal after,
    not before, the assertions). **(Iteration-12 amendment — the TEETH requirement.)** The
    consistency leg MUST drive **≥2 concurrent writers** (`spawn`/`join`) contending the **same
    version key across the fault window**, with **no-lost-update / version-monotonicity assertions
    over the interleaved outcome**: the `get_for_update` commit-point re-check
    (`crates/metadata-tikv/src/lib.rs:555-573`) only produces a `Conflict` under write-write
    contention, so a strictly sequential single-writer leg **cannot go red on a commit-point
    regression** (the adversary's iteration-12 refutation). And the partition MUST isolate the
    **region leader** or cut the **committing client from the Raft majority** — a minority-voter
    cut provably never changes the outcome against a linearizable store (the hollow flip this
    brief's invariant already forbids); (c) **Tier-2** — a run on **one real machine** (real
    `fsync`, real NVMe, real OS) green for honest single-node I/O. Every leg carries a **positive
    fault-effect oracle** (RED if the fault was a no-op) and **self-heals on every path**
    (Invariant B). Fault mechanisms are **ILLUSTRATIVE**; the binding conditions are "the swap
    holds the ADR-0015 single-zone contract on real TiKV under a fault that provably took effect,"
    "Tier-2 real-I/O is green," "the fault materialized," and (iteration-12) "a commit-point
    regression provably flips the contention leg red — see (d)."
    (d) **The executed mutation acceptance run (iteration-12 amendment — this IS the behavioural
    red→green that Option B defers to; it must be RUN and CAPTURED, not argued):** on the
    privileged box, four legs in order, each log captured to
    `results/issue_257/evidence/`: (1) real symmetric cut → consistency leg **GREEN** with
    `fault_materialized = true`; (2) no-op negative control (skip the cut) → leg **RED**
    (`fault_materialized = false`); (3) a *scratch, never-committed* mutation deleting/weakening
    the `get_for_update` re-check (`metadata-tikv/src/lib.rs:555-573`) → the contention leg goes
    **RED** (a lost update / version regression is observed); (4) mutation reverted → **GREEN**.
    The iter-8 acceptance criterion ("perturbing the re-check must flip an artifact") is satisfied
    by leg (3)'s captured log, or the slice does not pass Check.

  - **OUT (deferred, per ADR-0039):** the **literal public Jepsen/Elle credibility artifact** —
    deferred to **#329**; this slice realizes the consistency leg as an in-repo Rust scenario and
    MUST NOT re-attempt literal Jepsen (iter-4 demanded it; ADR-0039 forbids it here).

- **Invariant to restore:**
  - **The at-Check binding oracle must be independent of the code it certifies (the structural fix
    for v1–v6).** Six iterations failed because the binding red→green was a **self-authored DST sim
    whose "correct" branch is a hand-written good path** — `admit = commit_point_ok` ⇒
    `admitted_stale = commit_point_ok && !commit_point_ok`, **identically false by boolean
    algebra** before any schedule runs (v6 sign-off; `sim_tikv.rs`). "The branch I wrote to be
    correct is correct" is a tautology. The invariant: the binding assertion is validated by
    **production code and/or a third-party model** — a real defect must be able to flip it red.
    Driving the **real `TikvMetadataStore::commit` over the third-party `madsim-tikv-client`
    sim** restores this; a patch-authored correct-vs-broken mode flag violates it.
  - **DST determinism rationale must be sound for the backend it proves.** The `concurrency.rs`
    "no `await` inside commit" rationale (`:3-4`) is scoped to redb and false for a TiKV commit
    that awaits network I/O; the deterministic harness must exercise those interleavings
    (proposal 0015 §"Pinning the trait…"; ADR-0009).
  - **DST keeps correctness authority — M4 must NOT re-prove atomicity against *real* TiKV.** The
    live tiers prove the *backend matches the store*, not the commit protocol: "a real environment
    is never used to test correctness the simulation already covers … that is DST's job" (§13.1;
    ADR-0009 "complements, does not replace"). (Driving the real commit code under a *deterministic*
    madsim sim IS DST's job — this is not the live-tier over-proof the invariant forbids.) No leg
    or seed may rest its bindingness on "exactly-one-winner goes red" — a minority partition against
    a linearizable store never changes that outcome (the recurring iter-1..4 hollow flip).
  - **(Invariant B — fault-soundness, accepted with the human at Plan.)** A fault-injection leg is
    evidence **only if the fault provably materialized** — every leg carries a positive
    fault-effect oracle that is **red when the fault is a no-op** — **and it never leaks host
    state** (self-heals on every path, incl. panic/interrupt/failed-heal). The concrete v6 mechanism
    violated this with an **asymmetric `iptables -j DROP` on inbound-only** ports
    (`xtask/src/faults.rs:2107-2109`) — the exact "asymmetric no-op partition" shape the invariant
    forbids; the live partition must be **symmetric** (bidirectional), **heal every dropped port**
    (v6 healed only 20182 though it dropped 20162+20182), and **wait for store-port readiness**
    before asserting (v6 raced cluster formation).
  - **The trait stays unchanged** (`crates/traits/src/lib.rs`) — byte-for-byte; any trait edit is a
    failure of M4's thesis (proposal 0015; ADR-0006). **`metadata-tikv/src` stays unchanged** too —
    the madsim path is a **`Cargo.toml` cfg-alias only** (the chunkstore-grpc precedent); the store
    is *driven*, never *re-proven or edited* here.

- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (INTEGRATION §2 — M4 slices target the M4 integration branch, NOT `main`; it already carries
  slices 1–5, incl. the #256 `deploy/` stack). The slice's own branch is
  `feat/m4.6-real-commit-over-madsim-tikv`, PR'd **into** this integration base; commit subject
  `feat(dst): … (M4.6, #257)`.
- **Conflicts with:** 258
- **Ordering note:** #258 is an **active sibling in this same flow, still under development** (human,
  at Plan). #257 and #258 both edit `crates/dst/` (this slice authors the await-inside-commit seed;
  #258 builds the second-implementation contract/property harness that **folds this seed in**). The
  dependency is **#258 depends on #257** (#258 consumes #257's seed), so **#257 must land in the
  EARLIER wave** and #258 in a later one that builds on #257's accepted result. This slice therefore
  sets **no** `Depends on` (it is the prereq); the `Depends on: 257` is #258's to carry, and #258's
  brief carries the ordering constraint from its side. `Conflicts with: 258` is kept as the symmetric
  shared-`crates/dst/` guard so the two are **never co-scheduled in one wave** even if the
  dependency edge were dropped. #256 (the `deploy/` stack)
  is **already merged** on the integration base — no longer a dependency. The consistency
  **checker substrate** (#329 / #404–#409) and the **network-partition-of-a-live-node** upgrade
  (#399-style) are NOTED, deferred couplings — this slice consumes/aligns-with, it does not build them.
- **Surfaces:** data (backend/test infrastructure + DST; no frontend).
- **Difficulty:** high — the widest-surface M4 slice: the `madsim-tikv-client` cfg-alias + the DST
  seed driving the real commit over it, new `xtask` metadata Tier runners + pure
  dispatch/fault-effect-oracle logic, a `testkit` fault seam, a ≥3-replica TiKV Raft-group
  `deploy/` compose, and the in-repo scenario tier tests. Blast radius spans `dst` + `testkit` +
  `xtask` + `deploy/` + `metadata-tikv/Cargo.toml` (alias only) + new test targets; it does **not**
  touch `traits`, `core`, `custodian`, or `metadata-tikv/src` (proven, not re-proven here).
- **Do model:** opus-xhigh
- **Scope:** (i) add the **`madsim-tikv-client` cfg-alias** to `crates/metadata-tikv/Cargo.toml`
  (the chunkstore-grpc/`madsim-tonic` pattern; NO `src` edit) and author the **DST seed** driving
  the real, unchanged `TikvMetadataStore::commit` over the deterministic third-party sim under the
  await-inside-commit interleaving, asserting the ADR-0015 commit-point contract with a behavioural
  red demonstrated against production ordering; (ii) extend the realism-ladder **Tier-1**
  (integration + consistency-over-the-swap, in-repo Rust scenario per ADR-0039) and **Tier-2** lines
  across the swap against a real ≥3-replica TiKV Raft group under software-defined faults, each leg
  with a **symmetric**, self-healing fault + fault-effect oracle + independent ADR-0015 signals +
  multi-key create/rename/delete. / **out of scope:** `metadata-tikv/src`, `traits`, `core`,
  `custodian` (proven, not re-proven); the second-implementation contract/property harness (**#258**);
  the literal public Jepsen/Elle artifact (**#329**); the network-partition-of-a-live-node nemesis
  upgrade if it needs its own ADR (deferred — see NEEDS-HUMAN). Do MUST NOT name/seat a specific
  live-nemesis mechanism as the deliverable — restore the invariants; the mechanism is Do's within
  Invariant B (and must be symmetric).
- **Test file:** the **at-Check binding, flippable** artifact is the **DST seed** —
  `crates/dst/tests/tikv_await_commit_interleaving.rs` (path confirms at build) — driving the real
  `metadata-tikv` commit over `madsim-tikv-client` (or, on the declared fallback, the seed-as-
  coverage-artifact) — **plus** the pure `xtask` dispatch/fault-effect-oracle unit tests
  (`xtask/tests/…`, mirroring `xtask/src/faults.rs`'s `jepsen_dispatch` test) and the `testkit`
  fault-seam arithmetic tests. The live tier targets (`crates/metadata-tikv/tests/tier1_*`, a Tier-2
  metadata I/O target; paths confirm at build) are **privileged/off-Check** and skip cleanly,
  mirroring `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` / `tier2_integration.rs`.
- **Citations expected:** Do must cite `path:line` on `feat/m4-production-metadata-backend` AND the
  Planning artifacts for every change — the tier scaffolding it extends (`xtask/src/faults.rs` incl.
  `jepsen_dispatch`; `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`,
  `.../tier2_integration.rs`; `crates/custodian/tests/tier1_disk_faults.rs`;
  `crates/testkit/src/lib.rs`; `deploy/small-multi-node/docker-compose.yml`), the **cfg-alias
  precedent** (`crates/chunkstore-grpc/Cargo.toml`), the `dst` concurrency harness whose determinism
  rationale the seed corrects (`crates/dst/tests/concurrency.rs:3-4`), the **real commit under test**
  (`crates/metadata-tikv/src/lib.rs:540+`), the unchanged trait (`crates/traits/src/lib.rs`), and
  proposal 0015 §"DST and tests" / §"Pinning the trait…" / PR-sequence item 6 + ADR-0039 / ADR-0009 /
  ADR-0015 / proposal 0004 (madsim precedent) where they bind. Do **not** invent "verified backend
  facts" — cite `path:line` or mark "confirm at build."
- **Disposition hint:** likely-fix (implement — accepted-plan test-evidence slice behind Accepted
  proposal 0015 / ADR-0039 / ADR-0009 / ADR-0015; mints no new ADR unless the metadata-nemesis
  question below is answered "yes").

## Verification posture (MIXED — declared so C2/C4/C5 land as pre-declared sign-off items)

- **Built AND exercised at Check (the honest, independent red→green):** the **DST seed driving the
  real `metadata-tikv` commit over `madsim-tikv-client`** (a genuine flip: production commit code
  over a third-party sim — a real ordering defect goes red; demonstrate with a temporary, discarded
  perturbation) and the **pure** dispatch / fault-effect-oracle / fault-seam decision logic (red
  when negated). `cargo xtask ci` stays green with **no** TiKV and **no** privileged fault injection
  (the tier targets skip; the madsim seed needs no external cluster). **Gate honesty is mandatory —
  `cargo fmt --all` before finishing (v6's only gate failure was a fmt over-length match arm).**
- **Off-Check but EXECUTED IN DO (iteration-12 amendment — the live-evidence-first rule):** the
  privileged run is part of the **Do beat**, not a post-sign-off promise. Do runs the live Tier-1
  integration + consistency + Tier-2 legs (opt-in via `WYRD_TIER1=1` / `WYRD_TIER2=1`, privileged
  Docker host, ≥3-replica TiKV cluster) **plus the four-leg mutation acceptance run** (Success
  criterion (d)) and attaches every captured log under `results/issue_257/evidence/` **before
  submitting to Check**. Check and sign-off grade the **attached artifacts**; §6 NEEDS-HUMAN items
  verify evidence, they do not demand an unperformed run. Rationale: v11's wrong-PD-field oracle
  and iteration-12's no-teeth finding were only discoverable live, and each cost a full iteration.
  Gate runs additionally record an environment fingerprint and check for leaked partition rules
  (`iptables -S | grep 127.0.0.` empty) per `evidence/c4-gate-adjudication.md`.

## Known NEEDS-HUMAN (expect the reviewer to flag; not blockers to building)

- **`madsim-tikv-client` availability + fidelity (the direction-(a) contingency).** Confirm at build:
  (1) a `madsim-tikv-client` release tracks `tikv-client = "0.4"` (version-matrix risk like the
  `madsim-tonic 0.6.0+0.14 tracks tonic 0.14` note); (2) its simulated TiKV **models the
  await-inside-commit percolator write-conflict** faithfully enough that a missing commit-point
  re-check produces a lost update; **(3) madsim runtime integration** — the workspace aliases only a
  *network client* (`madsim-tonic`) within the standing DST tier harness and has **no `tokio`→madsim
  runtime aliasing anywhere**, so running the real async `commit()` deterministically under madsim is
  a harness lift for `metadata-tikv`, not a one-line dependency swap. If any of (1)–(3) fails, take
  the **declared Option-B fallback** and say so. Human accepts the third-party-sim-fidelity posture
  (this REPLACES the old self-authored-sim C5 question with an *independent*-model one). **Elevated
  risk (human, at Plan):** the Rust
  `tikv-client` is **pre-1.0 and only the Go client is considered upstream-stable**, so a
  `madsim-tikv-client` shim tracking `0.4` with faithful commit-conflict semantics is a **thin bet**
  — direction (a) is still the primary target, but Do should treat the confirm-at-build as
  genuinely likely-to-miss and fall to Option B cleanly rather than fight the dependency.
- **Rust-vs-Go TiKV client stability — architecture/Act flag, NOT this slice's call.** The human
  observed the Rust `tikv-client` is pre-1.0 while only the Go client is upstream-stable. That is an
  **M4-level decision already settled** in ADR-0008 / proposal 0015 (M4 drives the Rust client
  behind the unchanged trait); it is **out of scope** for #257 and must not be relitigated here.
  Logged as an **Act / architecture-board candidate** (see §10 Act candidates) — the realism-ladder
  live tiers this slice builds are precisely what surface the pre-1.0 client's behaviour.
- **Privileged-off-Check binding legs:** the live Tier-1 integration + consistency + Tier-2 green is
  observable only in the privileged CI/eval Tier job. A pre-declared C2/C4 sign-off item.
- **Metadata-store nemesis methodology — possible ADR refinement (architecture-board authority).**
  ADR-0039 rules `docker pause` ≡ partition for the M3 D-servers (dumb storage that initiate no
  commits) — and note ADR-0039 is literally titled "consistency-over-**repair**" and scoped to the
  chunkstore/proposal-0005 path; #257 applies its in-repo-scenario methodology to the *metadata swap*
  **by analogy**, which is precisely what this open question tests. TiKV runs its own Raft consensus,
  so a live-but-partitioned node is not observably equivalent to a paused one. Whether the metadata leg needs a **new metadata-specific ADR
  refinement** or can be an additive follow-up (#399-style) is the board's call. Do NOT author a new
  ADR unless the human directs it. The live partition here must be **symmetric** (Invariant B).
- **Static-endpoints reduced bar (#365):** the multi-replica cluster runs on **static endpoints**
  until #365 lands L5 discovery (Deployment-prerequisite note). Human confirms the reduced-bar
  posture is acceptable for this slice.
- **Realism-ladder vs code-taxonomy naming clash:** proposal 0015 and the `tier1_*`/`tier2_*`
  filenames use the **realism ladder** (Tier 0 DST · Tier 1 software faults · Tier 2 single machine ·
  Tier 3 multi-region); the 0004 CI/code taxonomy uses a different "Tier-1/Tier-2". This brief means
  the **realism ladder**.

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be marked ready
before sign-off accepts.

## Carry-forward — why v1–v6 were rejected, and what THIS re-plan changes

Six prior attempts are preserved in `iteration-v1/`…`iteration-v6/`. Do **must not** re-attempt the
rejected shapes. The rejections converged on **one structural defect** — now removed at Plan.

- **v1:** tautological routing test (asserted the literal it returned); a decorative DST "seed" that
  re-proved redb's atomicity (no racer interleaved mid-commit under madsim); nemesis computed, never
  applied. → independent oracles required; the seed models await-inside-commit; Invariant B.
- **v2:** gating FAIL + hollow evidence; wrong-tier nemesis; nemesis/load unsynchronized. → ≥3-replica
  Raft group; fault-effect oracle; gate honesty.
- **v3:** single TiKV data replica could only go unavailable, never split-brain → unfalsifiable; the
  on-Check green was a compile flip. → the binding on-Check flip is behavioural, not a compile flip.
- **v4:** reviewer demanded a literal Jepsen tool + network-partition-of-a-live-node, ruled the seed
  cannot defer to #258. → ADR-0039 defers literal Jepsen to #329 and sanctions the in-repo scenario;
  the seed is #257's own producer deliverable, #258 depends on #257.
- **v5:** the at-Check flip was a boolean the fixture read about itself; `assert_backend_equivalence`
  compared await-on vs await-off of the SAME store (no mode could lose an update). → make the red
  behavioural against a real assumption.
- **v6 (the decisive lesson):** the binding red→green remained a **self-authored DST sim whose
  correct branch hard-codes `admit = commit_point_ok`**, so `admitted_stale = commit_point_ok &&
  !commit_point_ok` is **identically false by boolean algebra** — vacuous before any schedule runs;
  the `PrewriteTrust` "negative control" only proves a deliberately-broken, patch-authored branch is
  broken. Your sign-off: **a self-authored sim whose correctness is a "good branch" is structurally
  incapable of a non-vacuous BINDING assertion about its own correct path** — no rebuild fixes it;
  Plan must redefine WHAT the binding evidence is.

**What THIS re-plan changes (the Plan decision made with the human):** the binding at-Check oracle is
no longer a self-authored sim. It is the **real, unchanged production `TikvMetadataStore::commit`
driven over the third-party `madsim-tikv-client` deterministic sim** (direction (a)), reachable via a
**`Cargo.toml` cfg-alias only** — the same `madsim-tonic` pattern the repo already uses for the gRPC
path (proposal 0004; ADR-0009), so **no `metadata-tikv/src` edit** and the invariant holds. A real
missing commit-point re-check now flips a real lost update — the independence the six iterations
lacked. **Option B is the declared fallback** if `madsim-tikv-client` doesn't track 0.4 / doesn't
model the conflict: move the binding correctness bar to the live Tier-1 legs and keep the at-Check
evidence to the surviving pure oracles + the seed-as-coverage-artifact (no self-certifying tautology).

**Keep (held under adversarial probing across v5/v6 — do NOT discard):** the `testkit` quorum
arithmetic and `partition_materialized` are genuine non-tautological oracles; the iter-5 `--features
tikv` + fault-descriptor-injection false-green is genuinely fixed.

**Also fold in (live-runner fixes the v5/v6 reviews were right about):** symmetric (not inbound-only)
partition per Invariant B; heal **every** dropped port (v6 dropped 20162+20182, healed only 20182);
**store-port readiness wait** before asserting (v6 raced cluster formation); assert **across** the
heal, not before it; **independent** `read_after_commit` / `converged_once` signals (v6 collapsed both
to one `scenario.is_ok()` bit); **multi-key create/rename/delete** integration leg (v6 covered only
create/read/duplicate-create).

**Do the end result the Success criterion names — do not re-litigate the framing.** If
`madsim-tikv-client` is unavailable, take the declared Option-B fallback and report it; do not
re-instantiate the self-authored-sim shape a seventh time.

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_257 (iteration 7) — the Option-B POSTURE is accepted (see ratified), but the at-Check + Tier-job evidence that remains is not sound: the fault the deferred proof rides on is a no-op behind a blind oracle, and two other legs re-instantiate previously-rejected shapes. Fix these before re-submitting; do NOT re-open Option-B. MUST FIX — the partition leg (load-bearing: the whole off-Check correctness proof depends on it): 1. Partition is one-way, not symmetric. `SymmetricPartition::apply` (crates/metadata-tikv/tests/tier1_metadata_consistency.rs:288-307) drops only `--dport 20161` on INPUT+OUTPUT; under `network_mode: host` both rules match the same direction (packets TO tikv-1), so tikv-1's self-initiated outbound Raft/PD links (dport 2379/20160/20162) and its established-connection replies survive — a receive-only blackout, not isolation (the v6 no-op-fault shape Invariant B forbids). Make it truly bidirectional: add `--sport 20161` / conntrack rules, or isolate the node's netns, so the node can neither send nor receive. 2. The fault-effect oracle is blind to (1). `fault_materialized` / the inline `before && during` (tier1_metadata_consistency.rs:114-127,124) TCP-probe the very dport the DROP guarantees to fail, so it verifies the rule blocks THE PROBE, not that the node is isolated — a one-way or probe-only cut passes the Invariant-B gate that must be RED for a no-op. Make the oracle observe real isolation (e.g. from the peer/PD side that tikv-1's raft/heartbeats stop), and WIRE the already-tested pure oracles `partition_took_effect` / `heal_is_complete` (xtask/src/metadata_faults.rs:109,119) into the scenario — they are currently dead code (consumed by nothing but their own tests), so regressing them flips nothing at Check (v1 "computed, never applied", applied to oracles). 3. Heal is silent-lossy (codex). The RAII heal (tier1_metadata_consistency.rs:310-336) `let _ =`s the `iptables -D` result and records no dropped/healed sets and never probes reachable-after-heal — a failed heal leaks host firewall state, the exact thing `heal_is_complete` was minted to catch. Surface iptables failures and verify the heal. MUST FIX — make the at-Check evidence behavioural, not tautological: 4. The flagship seed is an unflippable self-authored toy. `crates/dst/tests/tikv_await_commit_interleaving.rs` imports only std+madsim, never touches `TikvMetadataStore::commit` (crates/metadata-tikv/src/lib.rs:540-600); both halves are true BY CONSTRUCTION (Mutex semantics / a 1ms sleep), so no madsim seed and no production regression can flip it — if the real commit dropped its await window the seed stays green while its premise rots (the v6 "prove a patch-authored broken branch is broken" shape). Either bind the seed to the real commit path so a production regression can turn it red, or drop the "green under every madsim seed proves the swap" claim and label it honestly as pure coverage with no correctness weight. 5. C4-verify red is a compile-flip, not behavioural. run-verify's red is an unresolved-import compile error from deleting xtask/src/metadata_faults.rs while keeping its test; in the green phase the cfg(madsim) seed + #[ignore]d tiers run zero tests — precisely the v3 rejection ("the binding on-Check flip is behavioural, not a compile flip", brief.md:292). The at-Check flip must be behavioural. RATIFIED this iteration — do NOT re-litigate: - Option-B posture (axis 1): `madsim-tikv-client` genuinely does not exist (cargo search confirms no release tracking tikv-client 0.4), so the real ADR-0015-on-TiKV proof legitimately lives off-Check in the privileged Tier-1/Tier-2 job; the reduced at-Check bar (pure oracles + coverage seed) is accepted AS A POSTURE. The reason it still iterates is that the deferred-to Tier leg is itself broken (must-fix 1-3), not that Option-B is wrong. - T3: the seed IS executed at Check via `cargo xtask ci` -> `run_dst` under `--cfg madsim` (xtask/src/main.rs:825,836-857) — it is compiled and run, so it is NOT committed-but-unexecuted. (It is still unflippable — see must-fix 4.) - Invariants hold: no `crates/metadata-tikv/src` and no `crates/traits` edit (only a dev-dependency added to metadata-tikv/Cargo.toml). - #258 ordering (this slice in the earlier wave) and the metadata-nemesis ADR question routed to the architecture board (patch correctly mints no ADR) stand as pre-declared; the #365 static-endpoints reduced bar is accepted. Overarching note for the builder: this is iteration 7, and must-fix 1/4/5 each re-enter a shape the brief already rejected (v6 no-op-fault, v6 broken-toy, v3 compile-flip). The bar is BEHAVIOURAL evidence — a real bidirectional partition observed by an oracle that isn't probe-only, and a seed/flip a genuine production regression can turn red.
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the flagship at-Check seed, not the posture: Option B stays ratified (do NOT re-open it), and the pure testkit oracles, xtask dispatch, tier1/tier2 scenario rework (must-fixes 1-3), and the no-src/no-traits invariants all survive — keep them. The defect is tikv_await_commit_interleaving.rs: it instantiates RedbMetadataStore, never a TikvMetadataStore, so it is a near-clone of the pre-existing concurrency.rs (v1's rejected shape) while its docstring claims "the independence the six rejected iterations lacked." Acceptance test the next attempt must survive: perturbing the TiKV commit-point re-check (delete/weaken the get_for_update re-check at metadata-tikv/src/lib.rs:555-574) must flip an at-Check artifact — today the seed stays green under that perturbation, and the only perturbation that flips it also flips concurrency.rs identically (zero incremental red->green). Iter-7 must-fix 4's two exits are now ENFORCED, pick exactly one: (a) bind the seed so a behavioural perturbation of the code under swap (metadata-tikv/src, or a seam demonstrably equivalent to its await-inside-commit window) flips it at Check; or (b) keep it as pure coverage and rewrite the docstring to claim NO correctness weight and NO newly-reachable interleaving — no third option where it binds to redb but asserts teeth. If (b), the brief's Option-B line "assert the concurrency.rs rationale is unsound; here is a newly-reachable interleaving" must be satisfied by some other at-Check artifact or explicitly conceded as off-Check in the seed's labeling. While in there, fix the two codex advisories: tier1_metadata_consistency.rs:365 — heal() sets healed=true even when iptables -D failed and it returns Err, so a panic skips Drop cleanup and leaks host firewall rules; xtask/src/faults.rs:646 — the metadata Tier runner does not wait for PD/store readiness after docker compose up -d (the existing TiKV conformance runner shows the pattern).
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on a confirmed claim/gate gap, NOT on the ratified Option-B posture (keep Option B; do not re-open it). Directive: make the live scenario type-check at Check. Add a `cargo check -p wyrd-metadata-tikv --features tikv --tests` step to `run_ci` in xtask/src/main.rs so the `#[cfg(feature = "tikv")]` scenario code (SymmetricPartition, its impl/Drop, the PD oracle, and the partition_took_effect/heal_is_complete/consistency_passes consumption in tier1_metadata_consistency.rs) is compiled and type-checked by the whole-tree gate. Why: run_ci currently builds/tests --workspace with default features (tikv off); --all-targets selects target kinds, not features, so none of the load-bearing scenario is compiled at Check today. Confirmed in the patched target: a type error inside SymmetricPartition leaves `xtask ci` green. This makes the docstring's "compiles and type-checks it" and brief.md:230's "scenario tests compile in the whole-tree gate" false, and leaves iter-7 must-fix-2 (oracles wired into the scenario, not dead code) only nominally satisfied — the oracles' sole real consumer sits behind a feature Check never builds, so a live-leg regression flips no Check artifact. Scope guard: this is a CI/gate change (xtask) plus fixing the two now-false compile-at-Check claims (seed docstring + brief line). It does NOT require editing crates/metadata-tikv/src or crates/traits (invariants hold) and does NOT re-litigate Option B, exit (b), the seed's off-Check labelling, or the off-Check Tier-1/Tier-2 legs.
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 10 — carry-forward (from the previous attempt)
- Sign-off rationale: Everything ratified in iteration 9 stands (Option B, exit-(b) seed relabelling, pure testkit oracles, xtask dispatch, tier1/2 scenario rework) — §6 items 1,2,3,5 and the posture-part of 4 are not the reason for iterating. The rejection is narrowly the one piece of new-in-iter-10 code (the feature-gated compile step) and its guard. Fix exactly these two, change nothing else: 1. Gate-honesty regression (verified against target Cargo.toml:80-85). The new `feature_gated_checks()` step is wired UNCONDITIONALLY into `run_ci` (patch xtask/src/main.rs, the `for check in feature_gated_checks() { cargo(&check)?; }` loop). `cargo check -p wyrd-metadata-tikv --features tikv --tests` compiles the pre-1.0 grpcio-bearing `tikv-client` tree, contradicting the documented invariant that the default `cargo xtask ci` on a laptop/worktree with no TiKV "never compiles or audits this tree and stays green." The recorded C4-ci pass came from a toolchain-complete box and masked this. Fix: make the compile step conditional on toolchain/endpoint presence (e.g. a WYRD_TIKV_TOOLCHAIN / endpoint gate) so iter-9's type-check intent is preserved WITHOUT breaking the no-TiKV-CI invariant. Do not drop the step; gate it. 2. Tautology guard. `ci_type_checks_feature_gated_metadata_scenario` only asserts that `feature_gated_checks()` contains its own hard-coded literal; it never calls `run_ci` and never asserts `run_ci` iterates the function. Deleting the wiring loop leaves the test green — and it reinstates the "assert the literal the function returns" tautology shape that got the early iterations rejected. Fix: the test must actually exercise the run_ci wiring (assert run_ci invokes the feature-gated check), not restate the constant. No re-litigation of ratified posture; no reset of the bundle.
- Full previous attempt preserved in `iteration-v10/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 11 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the binding Tier-1 correctness leg FAILS in a live ≥3-replica run (privileged host, host-networked pingcap/tikv+pd v8.5.1). Empirically confirmed codex §6 item-6: ConsistencySignals { read_after_commit: true, converged_once: true, fault_materialized: false } — tier1_metadata_consistency.rs:203 panics. Root cause (not flaky infra): the fault-effect oracle keys off the WRONG PD field. pd_sees_target_up() (tier1 test:403) / pd_still_sees_target_up_after() (:412) treat PD /pd/api/v1/stores `state_name == "Up"` as "connected". For PD v8.5.1 `state_name` is the ADMINISTRATIVE state — it stays "Up" through a short partition and only flips to "Down" after max-store-down-time (default ~30min). The scenario's window is ~45s, so state_name never leaves "Up" → partition_took_effect() = false → fault_materialized = false → the leg can NEVER pass, regardless of a real cut. The iptables partition itself is sound (host networking, distinct loopback IPs 127.0.0.1/.2/.3 — the cut applies); only the oracle is wrong. What to change (Do rebuild): - Replace the state_name oracle with a transient-liveness signal: read the target store's `last_heartbeat` from /pd/api/v1/stores and assert it goes STALE (> a few heartbeat intervals) during the cut, or use pd-ctl's derived disconnected status / region leader-health. Do NOT rely on `state_name`. - Re-validate BOTH directions on the live cluster before claiming green: a real cut must set fault_materialized=true (leg passes), and the no-op negative control (skip iptables) must still classify as no-op (leg fails). Capture the logs. - Keep the ratified pieces intact (Option-B posture, the two iter-10 fixes: toolchain gating at run_ci_steps + the non-tautological guard test); no brief/plan change needed — this is a harness-implementation defect only. Not re-opened / still owed after the fix (unchanged NEEDS-HUMAN): item 5 — confirm a WYRD_TIKV_TOOLCHAIN=1 `cargo xtask ci` run actually type-checks the #[cfg(feature= "tikv")] bodies; items 3(a)/(b) — a named human confirms the fixed Tier-1/Tier-2 legs land green.
- Full previous attempt preserved in `iteration-v11/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 12 — carry-forward + THIRD plan revision (the evidence-architecture fix)

- Sign-off rationale: Rejected on evidence architecture, not on the iteration-12 code (keep all of it: the `last_heartbeat`-freshness oracle replacing `state_name`, the `parse_store_last_heartbeat`/`heartbeat_is_fresh` testkit extraction, the `WYRD_TIKV_TOOLCHAIN`-gated compile step, the non-tautological `run_ci` guard test — the adversary could not refute any of these). Three findings drive this revision:
  1. **The gating C4-ci exit-101 is ADJUDICATED non-reproducing** — see `evidence/c4-gate-adjudication.md`: adversary green twice; a third fingerprinted run (no `WYRD_*` vars, rustc 1.96.0, protoc present, and deliberately WITH a PD/TiKV cluster occupying 127.0.0.1:2379/20160) green. The port-squatting hypothesis is eliminated for the default-feature step; remaining suspects are leaked iptables state (the historic silent-lossy heal) or a differently-configured box. The iteration-12 rejection therefore rests on findings 2–3, NOT on the gate row. Consequence: gate runs now capture full output + env fingerprint + a leaked-rule check.
  2. **The sole binding leg had no teeth (adversary, confirmed by inspection):** `tier1_metadata_consistency.rs` drove strictly sequential single-writer commits and isolated a MINORITY voter — deleting the `get_for_update` re-check (`metadata-tikv/src/lib.rs:555-573`) leaves every assertion green, the exact hollow flip the brief forbids. FIX NOW IN THE BRIEF BODY (Success criterion (b), amended): ≥2 concurrent writers contending the same version key across the fault window with no-lost-update/version-monotonicity assertions; partition the region leader or cut the client from the majority.
  3. **Zero executed behavioural evidence existed anywhere** (at Check by declared Option-B posture; off-Check because the privileged run was a perpetually-owed NEEDS-HUMAN). FIX NOW IN THE BRIEF BODY: (a) Success criterion (d) — the four-leg mutation acceptance run (real-cut GREEN / no-op RED / mutated-re-check RED / restored GREEN), executed and captured, IS the behavioural red→green Option B defers to; (b) Verification posture — the live-evidence-first rule: the privileged run happens IN DO, its logs land in `evidence/` BEFORE Check submission, and §6 items verify artifacts instead of demanding unperformed runs.
- RATIFIED and NOT re-opened: Option-B posture; exit-(b) coverage labelling of the DST seed; the pure testkit oracles; the no-`metadata-tikv/src`/no-`traits` edit invariants; #258 ordering; the #365 static-endpoints reduced bar.
- Act candidates minted by this revision (for the next Act review, `process/act-log.md`): (i) Plan-time feasibility check — any third-party crate named as a binding mechanism is `cargo search`-verified before the brief freezes (madsim-tikv-client cost 6+1 iterations); (ii) escalation cap — two consecutive rejections on the same criterion force a return to Plan, not another Do; (iii) live-evidence-first as a standing rule for slices whose binding evidence is off-Check; (iv) gate output + env fingerprint retention in the driver.
- Full previous attempt preserved at the bundle top level (patch.diff, build-notes.md, SUMMARY.md, check-*) until the iteration-13 Do run re-populates it; freeze to `iteration-v12/` at that point per the established convention.
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
