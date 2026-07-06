# Brief (pointer) — issue 258 / m4.7-dst-pin-second-impl

> A Plan artifact that is a **pointer**: the planning decision already lives in an
> accepted, governed proposal (0015, superseding 0007) — this file references it and
> carries the fields the driver parses. Do reads the **Planning artifact** as the
> authoritative plan; this brief does not restate it.

- **Slug:** m4.7-dst-pin-second-impl
- **Planning artifact:** `docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  — authoritative. Read specifically: §"DST and tests (the heart of M4)" (the four-tier
  ladder, esp. the **"TiKV does not go inside the deterministic simulator"** boundary,
  lines 481–544), §"Pinning the trait with the second implementation" (lines 546–555 — the
  redb-shaped determinism rationale to revisit), §"Crate touch-points" (the **`dst`** bullet,
  lines 576–579), §"Definition of done" (lines 655–657), §"Suggested PR sequence" **item 7**
  (M4.7, lines 720–725, this slice), and the Open question **"DST model fidelity for an
  await-inside-commit backend"** (lines 798–801 — the crux, issue #264). Ground it against:
  **ADR-0009** (deterministic-simulation-testing — the correctness authority; DST
  "complements, does not replace" the real tiers), and the **two-implementations-pin
  discipline** stated in **ADR-0006** (`docs/design/adr/0006-etcd-for-coordination.md:20`:
  a trait's semantics "pinned by two implementations … before an embedded backend is
  trusted under DST") and echoed in **ADR-0020** (`0020-global-namespace-store.md:40`).
- **Defect / goal:** M4 is the **second** `MetadataStore` implementation, so it must **pin
  and harden** the trait — not merely use it (0015 lines 115–120, 546–555; ADR-0006's
  two-implementations rule). The one remaining gap **after #257 lands**: the DST property
  suite is **not yet driven through both backends inside the deterministic simulator**. Today
  the shared `crates/metadata-conformance` suite runs over redb *in DST* but over TiKV only
  *endpoint-gated, outside the simulator*. #257 (which lands first) integrates the deterministic
  third-party **`madsim-tikv-client`** (driving the *real* `commit()` under `--cfg madsim`),
  authors the await-inside-commit **seed**, and corrects the `concurrency.rs:3-6` redb-shaped
  rationale — **this slice INHERITS all three via the wave fold and must NOT re-author them.**
  #258's job is the **pin**: drive the **identical** property/conformance suite over **both**
  backends — redb *and* #257's `madsim-tikv-client` second implementation — **inside** the
  deterministic simulator, green and seed-reproducible, and **grow the seed set**.
- **Success criterion:**
  - **BINDING (demonstrable at Check):** (a) the DST drives **both** backends — the redb
    deterministic backend **and** #257's `madsim-tikv-client` second implementation (the real
    `commit()` over the deterministic third-party sim) — through the **identical** shared
    property/conformance suite, **green and seed-reproducible** (0015 lines 655–657, 723–725;
    ADR-0006). The flippable at-Check evidence: the shared suite is **RED over the second backend
    before it is wired into the simulator** (today it runs only redb-in-DST / TiKV-endpoint-gated)
    and **GREEN once driven over both** in-simulator — an independent flip, since the suite is
    redb-and-#257 code, not written to pass this wiring. (b) **new seed(s) committed**, including
    at least one that reproduces forever (0015 lines 539, 660) — folding in #257's
    await-inside-commit seed and adding property/contract coverage for the pin. The second
    implementation **is #257's `madsim-tikv-client`** (not a fresh hand-written model) — this
    slice **adopts** it, it does not rebuild it.
  - **INHERITED FROM #257 — NOT this slice's deliverable:** the `concurrency.rs:3-6` rationale
    correction, the `madsim-tikv-client` integration, and the await-inside-commit seed itself.
    #258 builds **on top of** them (wave fold); re-authoring any is out of scope.
  - **INVARIANT (must not regress):** Tier-0 DST stays green and seed-reproducible on the
    **deterministic** backend; the `MetadataStore` trait stays **unchanged** (see Invariants).
- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (the M4 integration branch — resolves to `origin/feat/m4-production-metadata-backend`,
  head `225d3bd`; it already carries slices 1–3, i.e. the `metadata-tikv` crate with its
  conformance/contention/scan tests, and slice 4 (#255) is an open draft PR into it). The
  slice's own branch is `feat/m4.7-dst-pin-second-impl`, PR'd **into** this integration
  base, not `main`.
- **Depends on:** **#253, #257** — #253 is the TiKV backend (the second `MetadataStore`
  implementation whose semantics this slice pins; without a second backend there is nothing to
  drive the identical suite against; the M4.2 commit/CAS semantics must be settled for the pin to
  be meaningful). **#257 lands FIRST and this slice builds on its accepted result:** the #257
  re-plan authors the await-inside-commit **DST seed**, integrates the deterministic third-party
  **`madsim-tikv-client`** (driving the *real* `commit()` under `--cfg madsim`), and corrects the
  `concurrency.rs:3-6` redb-shaped rationale. This slice **folds that seed in** and drives the
  identical property suite over both backends **on top of** #257's integration — so #258 lands in
  a **LATER wave** than #257 (both edit `crates/dst/`), building on #257's accepted diff via the
  wave fold.
- **Conflicts with:** **#257** — the #257 re-plan now **also edits `crates/dst/`** (the
  await-inside-commit seed, the `concurrency.rs:3-6` rationale correction, and the
  `madsim-tikv-client` integration), so the two **do** collide on shared `crates/dst/` files —
  #258's earlier "no sibling edits them" no longer holds. The `Depends on: #257` edge already
  sequences them into different waves (this slice builds on #257's accepted result); `Conflicts
  with` is the belt-and-suspenders guard that they are **never co-scheduled in one wave**.
- **Scope:** in `crates/dst` and the shared `crates/metadata-conformance` suite that redb and
  TiKV already share (see Citations): **drive the identical property/conformance suite over both
  backends inside the deterministic simulator**, wiring #257's already-integrated
  `madsim-tikv-client` second implementation into the in-simulator property runs so the same
  assertions redb passes are run against it; **grow the seed set** (fold in #257's seed + add
  pin coverage). All under ADR-0009 (madsim deterministic, single-threaded, seed-reproducible).
- **Out of scope:** **#257's deliverables it now inherits** — the `madsim-tikv-client`
  cfg-alias/integration, the `concurrency.rs:3-6` rationale correction, and authoring the
  await-inside-commit seed (all land in #257's earlier wave; #258 builds on them and must not
  re-author them); **hand-writing a fresh simulated-TiKV model** (the second impl is #257's
  `madsim-tikv-client` — adopt it, don't rebuild); the **real-cluster tests** (#257 — Tier-1
  software-faults, Jepsen, Tier-2 single-machine); the **deploy stack** (#256 — the `deploy/`
  production topology); the **`metadata-tikv` backend code** itself (#253 / slices 1–3); the
  **`server` composition** change (#255 / slice 4); **any change to the `MetadataStore` trait**
  (0015 lines 572–573, 740–743 — a trait edit is a failure of M4's thesis); putting a
  **real/containerized TiKV inside DST** (explicitly rejected, 0015 lines 484–499,
  600–603 — ADR-0009 forbids it).
- **Ordering note:** slice 7 is the **tail** of the M4 sequence (0015 line 729): it needs
  #253 landed (the second impl exists) and now **builds on #257** (which lands first — see
  Depends on / Conflicts with). Per ADR-0009 "DST never cedes authority to the real tiers"
  (0015 line 544) the pin proceeds on the **deterministic** simulated-TiKV model / contract
  harness independently of the *live* cluster; it does not wait on a live TiKV — but it **does**
  wait on #257's **at-Check deterministic** deliverables (the `madsim-tikv-client` integration +
  the corrected `concurrency.rs` rationale + the seed), which land in the earlier wave.
  **OVERLAP TO RESOLVE AT PLAN (see NEEDS-HUMAN):** #257's re-plan now also corrects
  `concurrency.rs:3-6` and authors an await-inside-commit seed — work this brief also claims. On
  the current edges #258 inherits #257's already-corrected file, so #258 must **not** re-do the
  correction; and #257's `madsim-tikv-client` may already **be** the "second implementation
  inside the deterministic simulator" this slice is chartered to build. Whether #258's
  deliverable narrows to "drive the identical property suite over both backends on top of #257's
  integration" is a re-scoping call the human owns.
- **Do model:** opus-xhigh
- **Difficulty:** **high** — de-risked from "hard" by this re-scope: the proposal-flagged open
  design point (issue #264 / 0015 lines 798–801 — how faithfully to model 2PC/TSO
  await-inside-commit interleavings) is **largely answered by adopting #257's third-party
  `madsim-tikv-client`** as the deterministic second impl, rather than designing fidelity from
  scratch here. Remaining blast radius is real but bounded: wiring the shared property/conformance
  suite over both backends inside DST across `crates/dst` + `crates/metadata-conformance`, plus
  seed growth. A NEEDS-HUMAN ratification that the adopted sim's fidelity satisfies the pin still
  stands (below) — but it is no longer a from-scratch fidelity design.
- **Test file:** the in-simulator property/conformance driver over both backends — a DST test
  target driving `crates/metadata-conformance` over redb **and** #257's `madsim-tikv-client`
  second impl (location ILLUSTRATIVE — `crates/dst/tests/…` or the shared suite wired under
  `--cfg madsim`; **confirm at build**), plus the grown seed(s). The **flippable, at-Check
  regression:** the shared property suite is **RED over the second backend before it is driven
  in-simulator** (today it runs only redb-in-DST / TiKV-endpoint-gated) and **GREEN once wired
  over both** — an independent flip (the suite is redb-and-#257 code, not written to pass this
  wiring). #257's await-inside-commit seed (`crates/dst/tests/…`, inherited) must stay green over
  the folded base; #258 does not re-author it. If a *demonstrated* red is infeasible, declare it
  under Verification posture rather than skip it.
- **Citations expected:** Do must cite `path:line` on the target branch AND the Planning
  artifact for every change. Anchor points already on branch: the rationale #257 corrects
  (`crates/dst/tests/concurrency.rs:3-6`, inherited context — not #258's to edit) and the winner
  test it guards
  (`concurrency.rs:36` `exactly_one_concurrent_writer_wins`, which today runs over
  `RedbMetadataStore::in_memory` at `concurrency.rs:38`); the shared conformance suite crate
  `crates/metadata-conformance/` (`wyrd-metadata-conformance`), already driven over redb
  (`crates/metadata-redb/tests/conformance.rs`) and TiKV
  (`crates/metadata-tikv/tests/conformance.rs`, endpoint-gated); the trait contract at
  `crates/traits/src/lib.rs`.
- **Disposition hint:** likely-fix (the two-implementations **pin**: wire the identical property
  suite over #257's `madsim-tikv-client` second impl inside DST + grow the seed set; the rationale
  correction, integration, and await-inside-commit seed are inherited from #257), with a **MIXED**
  verification posture declared so the fidelity/pin judgment lands as a pre-declared sign-off item,
  not a surprise NEEDS-HUMAN.

## Invariants to hold

- **Tier-0 DST stays green and seed-reproducible on the deterministic backend** (seeds
  committed) — ADR-0009; 0015 lines 655–657. The pin adds coverage; it never yellows the spine.
- **Exactly-one-winner (version-conditional CAS) holds over BOTH backends** — the property
  passes over redb *and* #257's `madsim-tikv-client` second impl inside the simulator. (The
  await-inside-commit *rationale correction* and the interleaving seed are #257's, inherited;
  #258 proves the property survives over the second backend, it does not re-earn the rationale.)
  Do not "pass" the suite by weakening any assertion.
- **The `MetadataStore` trait is UNCHANGED** — pinning **hardens** the trait, it does not
  evolve it. Any edit to `crates/traits/src/lib.rs`'s contract is a failure of M4's thesis
  (0015 lines 572–573, 740–743).
- **TiKV does NOT go inside the deterministic simulator** — the "second implementation" here
  is a **deterministic simulated-TiKV model** or a **trait-level contract harness**, NEVER a
  containerized/real TiKV in DST (0015 lines 484–499, 600–603; ADR-0009).
- **The property suite is IDENTICAL across both backends (shared, not forked)** — the
  assertions redb passes are the assertions the second implementation passes (0015 lines
  548, 656; slice 1's "shared, not forked" rule).
- **A bug-finding seed reproduces forever** — determinism is the whole point; a committed
  seed must replay the same interleaving on every run (0015 lines 539, 660).

## Known NEEDS-HUMAN

- **Scope overlap with the #257 re-plan — RESOLVED at Plan (this re-scope).** #258's brief has
  been narrowed so it **inherits** #257's `madsim-tikv-client` integration, `concurrency.rs:3-6`
  correction, and await-inside-commit seed (via the wave fold), and delivers only the **pin**:
  the identical property/conformance suite over both backends inside DST + seed growth. Do must
  **not** re-author any of the three inherited items — if any is missing from the folded #257
  base at build, that is a NEEDS-HUMAN (an ordering failure), not a cue to rebuild it here.
- **HARD DEPENDENCY on #257's direction (a) — the second backend may not exist.** #258's second
  implementation **is** #257's `madsim-tikv-client`. But #257 carries a declared **Option-B
  fallback**: if `madsim-tikv-client` does not track `tikv-client = "0.4"` / does not model the
  commit-conflict / cannot integrate the madsim runtime, #257 lands **without** it. **If #257's
  Option B fired, #258 has no second backend to pin against** — Do must **stop and raise a
  blocking NEEDS-HUMAN**, NOT substitute a hand-written model (that reopens the self-authored-sim
  trap #257's re-plan exists to avoid). Confirm #257's landed direction at the start of #258's Do.
- **The crux — DST model fidelity for an await-inside-commit backend (issue #264; 0015 lines
  798–801) — reframed by the re-scope.** The fidelity choice is no longer "hand-model TiKV vs
  contract harness"; it is **"is #257's third-party `madsim-tikv-client` an accepted faithful
  deterministic second implementation for the pin?"** — the same sim-fidelity question #257
  carries at its C5, now applied to the whole property suite rather than one seed. The **human
  ratifies** that adopting it satisfies ADR-0006's two-implementations pin. Do not resolve #264
  unilaterally.
- **Interleaving-coverage adequacy.** The exactly-one-winner invariant holds under CAS
  regardless — so is the required change a *new set of interleavings* (the await boundary
  makes new schedules reachable) or *only* a corrected comment? This is a judgment on
  whether the redb-shaped harness under-covered the await-inside-commit case; flag it for
  human sign-off, don't assert it silently.
- **`tikv-client` futures `Send + Sync` for the object-safe, simulator-driven trait**
  (0015 lines 778–779) — a build-time confirmation the model's async shape can stand in for.
  Mark **confirm at build**; do not assert it as a verified backend fact.

## STOP discipline

Do reads **`brief.md` only**. Produce `patch.diff`, the test(s) the brief names, and
`build-notes.md`. The named test must fail pre-fix and pass post-fix (or, where a
demonstrated red is infeasible for the chosen fidelity model, the posture is declared under
Verification, not skipped). Cite `path:line` on the target branch for **every** change.
**Do NOT fabricate "verified backend facts"** about TiKV's transaction/await behaviour or
`tikv-client` signatures — cite `path:line` or mark **"confirm at build"** (the standing
lesson from #253). You MAY push to a feature/draft branch and open a **draft** PR into
`feat/m4-production-metadata-backend`; you MUST NOT mark a PR ready or merge it — that is the
human's sign-off step.
