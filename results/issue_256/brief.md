# Brief (pointer) — issue 256 / m4.5-deploy-tikv-pd-etcd

> A Plan artifact that is a **pointer**: the planning decision already lives in an
> accepted, governed proposal (0015, superseding 0007) — this file references it and
> carries the fields the driver parses. Do reads the **Planning artifact** as the
> authoritative plan; this brief does not restate it.

- **Slug:** m4.5-deploy-tikv-pd-etcd
- **Planning artifact:** `docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  — authoritative. Read specifically: §"Deployment: TiKV/PD as a stateful, disk-affine,
  orchestrator-agnostic tier" (incl. the **Deployment prerequisite** note), §"Crate
  touch-points" (`deploy/`), and §"Suggested PR sequence" **item 5** (this slice). Ground
  it against the design corpus, read in place under `../wyrd` (never copied): architecture
  **§7.1** (the "Small multi-node Production" profile table), **§7.2** (pluggable substrate:
  "Kubernetes is available, never required"), **§7.5** (ports/protocols for the single-zone
  stack), the `m4-first-deployment-blueprint.md`, and **ADR-0010** (pluggable deployment
  substrate) + **ADR-0006** (etcd for Coordination).
- **Defect / goal:** the single-zone "Small multi-node Production" tier has no bring-up
  recipe — the only `deploy/` artifact is slice 1's *throwaway* single-node TiKV
  (`deploy/tikv-single-node/`, for the conformance suite). Ship the production topology's
  bring-up from `deploy/`: **TiKV (small) + its own PD cluster + a 3-node etcd ensemble for
  L5 Coordination + local-disk D servers** (no L2/L3/TiDB), as a docker-compose stack for
  CI/eval — outside the Cargo workspace, with no crate coupled to an orchestrator API.
- **Success criterion:**
  - **BINDING (demonstrable at Check):** (a) the `deploy/` artifacts for the "Small
    multi-node Production" stack exist and are structurally **valid** — a docker-compose
    stack composing **TiKV-small + a PD cluster + a 3-node etcd ensemble + local-disk D
    servers**, that `docker compose config` parses/validates; and (b) **no workspace crate
    imports a k8s/orchestrator API** — enforced by a guard that is RED when an orchestrator
    import is planted in a crate and GREEN on the real tree. Component identities
    (docker-compose / etcd / the guard's mechanism) are **ILLUSTRATIVE**; the binding
    conditions are "the production topology's bring-up exists under `deploy/` outside the
    workspace" and "no crate is coupled to an orchestrator API."
  - **DEFERRED (off-Check — see Verification posture):** the stack actually **stands up**
    and **peers are discovered through L5**. Per the proposal's Deployment-prerequisite
    note this is **gated** on work outside M4's metadata scope, concretely tracked as
    **#365** ("Coordination backend: etcd (L5) — … assumed by M4.5, required multi-node",
    OPEN) for the etcd-backed `Coordination` half, **plus** the runnable gateway/custodian
    **process roles** — which, per the **2026-07-04 maintainer decision**, are owned by
    **#364** (the S3 HTTP wire makes the gateway a runnable networked server) **+ #366** (the
    observability floor makes the custodian runnable as its own deployable process); no
    separate process-roles issue. Until #365 + #364 + #366 land the stack runs with **static
    endpoints** and the "peers discovered through L5" DoD moves with the prerequisite.
    Confirmed by an operator on a Docker host / CI-eval run, ultimately the first-deployment
    gate (#367). NOTE the pre-prerequisite bring-up is **partial**: TiKV + PD + a 3-node etcd
    + local-disk D servers on static endpoints; the long-running gateway/custodian services
    land with #364 / #366 (today `wyrd` exposes `d-server` as a role and the gateway only as
    put/get client mode).
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend
  (the M4 integration branch — INTEGRATION §2; it already carries slices 1–3, and slice 4
  (#255) is an open draft PR into it. The slice's own branch is `feat/m4.5-deploy-tikv-pd-etcd`,
  PR'd **into** this integration base, not `main`.)
- **Scope:** author the `deploy/` bring-up for the single-zone "Small multi-node Production"
  stack (docker-compose composing TiKV-small + PD cluster + 3-node etcd + local-disk D
  servers), a `deploy/README.md` section documenting it, its CI/eval wiring (an `xtask`
  runner, mirroring the existing `tikv-conformance` pattern), and preserving the ADR-0010
  invariant that **no crate couples to an orchestrator API** (Do chooses how to enforce and
  demonstrate it) — all **outside the Cargo workspace** (ADR-0010).
  The new stack lands as a **fresh `deploy/<small-multi-node>/docker-compose.yml`** (mirroring
  the existing `deploy/tikv-single-node/`), reusing the `wyrd-dserver:local` image and the
  `wyrd d-server` role — it is **not** an edit to the repo-root `docker-compose.yml` (a
  D-server-only dev stack).
  / out of scope: the **deployment prerequisite** — the etcd-backed `Coordination` backend
  (#365) and the runnable gateway/custodian process roles (owned by #364 + #366 per the
  2026-07-04 decision), all their own bodies of work outside M4 metadata scope; the Helm chart
  / operator (deferred); the `metadata-tikv` + `server` backend code
  (slices 1–4); Tier-1 integration + Jepsen + Tier-2 (slice 6); the DST second-implementation
  pin (slice 7); any change to `crates/`, `traits`, the on-disk format, or the `MetadataStore`
  contract.
- **Do model:** opus-xhigh (explicit per-bundle pin — OVERRIDES the difficulty `when`
  routing; issue #167)
- **Difficulty:** medium — the change is confined to `deploy/` (outside the workspace) plus
  `xtask` wiring and one guard test; by ADR-0010's structural design it deliberately does
  **not** propagate into the crates, so the diff-reviewer's blast-radius is contained to the
  bring-up artifacts and their runner.
- **Test file:** `xtask/tests/deploy_no_orchestrator_coupling.rs` (path + mechanism
  ILLUSTRATIVE — Do may house it as an `xtask` check instead). TWO at-Check signals ship,
  both here or split as Do sees fit: (1) the flippable regression asserting no workspace
  crate imports an orchestrator/k8s API — RED on a planted orchestrator import, GREEN on the
  tree; and (2) a `docker compose config` **validity** assertion over the new
  `deploy/<small-multi-node>/` stack (parses; declares the four component roles). The actual
  bring-up / L5 discovery is the deferred posture below.
- **Verification posture:** MIXED (net-new infrastructure + off-Check bring-up), declared so
  C2/C4 land as pre-declared sign-off items, not surprise NEEDS-HUMAN.
  - **Built AND exercised at Check:** the orchestrator-coupling guard (demonstrate its RED
    with a temporary planted `kube`/orchestrator import proving the guard is load-bearing,
    then GREEN on removal); a `docker compose config` structural validation of the new
    stack (parses; declares the four component roles). These are net-new coverage where
    "red" is criterion-absence, so Do must supply a *demonstrated* red where feasible.
  - **Deferred / off-Check (needs a Docker host + the deployment prerequisite):** the stack
    booting and L5 peer discovery. NAME who confirms: an operator / CI-eval run on a Docker
    host, and the first-deployment gate #367. This is BUILT (the `deploy/` artifacts + runner
    exist and are exercised by the compose-config check and the guard at Check) but its live
    green is observable only off-Check — it is not an unbuilt deliverable.
- **Production reach:** this slice builds the deployment SEAM ahead of its live consumer.
  The `deploy/` stack is authored and validated now, but the LIVE "peers discovered through
  L5" path still collapses to **static endpoints** because its production wiring — an
  etcd-backed `Coordination` (#365) + gateway/custodian process roles (#364 + #366, per the
  2026-07-04 decision) — does not exist yet (proposal 0015, Deployment-prerequisite note).
  WHERE the production wiring lands: #365 + #364 + #366 (sequenced as an explicit slice or a
  preceding coordination milestone — an open scope decision in the proposal), all of which
  must land before slice 5 can claim L5 discovery. The compose stack + guard exercise the
  seam load-bearingly (the guard is red on a planted import; the stack is a real, parseable
  multi-node topology), not dead scaffolding.
- **Citations expected:** Do must cite `path:line` on the target branch AND the Planning
  artifact for every change (e.g. the existing `deploy/README.md` and
  `deploy/tikv-single-node/docker-compose.yml`, and the `xtask` runner it mirrors).
- **Disposition hint:** likely-fix
</content>
