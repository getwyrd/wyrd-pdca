# Brief (pointer) — issue 367 / first-deployment-gate-day-one-runbook

> A Plan artifact that is a **pointer**: the plan already lives in the governed
> design corpus — the [M4 first-deployment blueprint][blueprint]'s day-one runbook,
> architecture [§7.4][s74], and accepted proposal [0015][p15]. This file references
> them and carries the fields the driver parses; it does not restate the runbook.
> This is a **GATE / RUNBOOK umbrella**, not a single code change (issue #367): the
> deliverable is making the blueprint's day-one verification sequence **executable
> end to end** against the `deploy/` stack.

- **Slug:** first-deployment-gate-day-one-runbook

- **Planning artifact:** the day-one verification runbook is the authoritative plan,
  read in place under `../wyrd` (never copied):
  - `docs/design/architecture/m4-first-deployment-blueprint.md` — **the runbook**.
    Read specifically **"Day-one verification (both variants) — do this before
    trusting it"** (`m4-first-deployment-blueprint.md:688-724`, the ordered 7-step
    sequence where "each gates the next"), and **"Day-one operations (both
    variants)"** (`:287-319`). These are the exact steps this gate must automate.
  - `docs/design/architecture/07-deployment-view.md` **§7.4** ("Worked example —
    first single-zone deployment (M4–M5)", `07-deployment-view.md:51-64`) — the
    normative framing: day-one verification "gates trust, in order" — (1) label
    failure domains, (2) verify spread across distinct domains, (3) under-replicated
    count must sit at **zero** in steady state, (4) kill one D server and watch the
    reconstruction loop return the count to zero. §7.4 declares the blueprint the
    operational detail behind it ("operational guidance, not a normative spec").
  - `docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
    — the accepted M4 plan of record. Read **"Graduation criteria"** ("The
    production deployment stands up from `deploy/`" — TiKV-small + PD + 3-node etcd +
    local-disk D servers, no crate importing an orchestrator API) and the
    **"Deployment prerequisite"** note (the "peers discovered through L5" bar is
    **gated** on an etcd-backed `Coordination` + runnable gateway/custodian process
    roles; until that lands, clusters run with static endpoints). Cross-ref ports
    **§7.5** (`07-deployment-view.md:66-88`): D-server gRPC `50051`
    (`crates/server/src/cli.rs:32`), Coordination etcd `2379/2380`, all internal
    dials mTLS with no plaintext fallback (ADR-0025).

- **Defect / goal:** the day-one verification runbook (blueprint `:688-724`; §7.4)
  exists as prose but is **not executable** — there is no script/harness that drives
  it against the `deploy/` stack and reports pass/fail. Make it executable end to
  end: (a) bring the `deploy/` stack up, (b) apply failure-domain labels to the D
  servers (runbook step 1), (c) verify a written chunk's fragments spread **across
  distinct failure domains** (step 3), (d) assert the durability plane's
  **under-replicated count reads ZERO** in steady state (step 4), (e) **kill one
  D server** and confirm the reconstruction loop (reads keep serving from survivors →
  under-replicated rises → custodian rebuilds → count returns to zero) completes
  (step 5, "the one that matters"). This is the last item of the first-deployment
  gate (#367 checklist): the single point that says the first real deployment can
  actually succeed.

- **Success criterion:** **the runbook script exits `0` end-to-end on the `deploy/`
  stack** — i.e. steps (a)–(e) above run in order, each gating the next, and the
  kill-a-D-server reconstruction loop drives the under-replicated durability plane
  back to zero, all without manual intervention. Equivalently: the blueprint's
  day-one verification (`:688-724`) is expressed as an executable check whose green
  is the gate's pass. Component/tooling identities (bash vs `xtask` vs compose
  healthchecks; exact metric names) are **ILLUSTRATIVE** — the binding condition is
  "the ordered day-one runbook runs to completion, zero-exit, against a stood-up
  multi-domain stack, including the reconstruction-to-zero loop."

- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (the M4 integration branch — INTEGRATION §2; resolves to
  `refs/heads/feat/m4-production-metadata-backend`, commit `225d3bd`). This gate is
  an M4-deployment concern; its dependencies (#256 stack, #365 Coordination, #364 S3
  wire) all target this integration base, so the runbook lands here too, PR'd **into**
  this branch, not `main`. Its own working branch, e.g.
  `feat/m4-first-deployment-gate-runbook`, PR's into this integration base.

- **Depends on:** **ALL of** —
  - **#256** — the `deploy/` tree that stands the "Small multi-node Production" stack
    up (TiKV-small + PD + 3-node etcd + local-disk D servers). *(Not yet on branch:
    `deploy/` currently holds only `README.md` + slice-1's throwaway
    `tikv-single-node/` — confirm at build.)*
  - **#365** — the etcd-backed `Coordination` backend, selected by config. Required
    for D-server registration/discovery and custodian leader-election (the blueprint's
    "peers discovered through L5"; 0015 Deployment-prerequisite note).
  - **#366** — the observability floor. The runbook's step 4 (durability plane at
    zero) and step 5 (watch under-replicated rise/return) need the five M3 metrics
    live; #366's day-one checklist must be executable as written.
  - **#364** — the S3 HTTP wire floor on the gateway. The runbook's round-trip
    (step 2, PUT→GET) and write-then-inspect steps need a real client endpoint.
  - **#286** — D-server container runs as non-root. **Already done** (dependency
    satisfied); listed for completeness of the gate checklist.

- **Conflicts with:** none known. This gate composes the above deliverables and adds
  a verification harness on top; it does not modify `crates/`, the `MetadataStore`
  contract, the on-disk format, or the coordination trait. It shares the `deploy/`
  surface with #256 (builds on its stack) but is additive to it.

- **Scope:** author an executable day-one verification runbook that drives the
  `deploy/` stack through the blueprint's ordered sequence and gates on each step —
  (a) bring the stack up, (b) apply failure-domain labels to the D servers, (c) write
  a test object and assert fragment spread **across** distinct failure domains, (d)
  assert the under-replicated / durability plane reads **zero** in steady state, (e)
  kill one D server and assert the reconstruction loop returns under-replicated to
  zero (reads serve from survivors throughout). Wire it as a runnable check
  (CI/eval-runner, mirroring the existing `tikv-conformance` / `xtask` pattern named
  in the #256 brief) that exits `0` on full pass. Document it in the `deploy/` README.

- **Out of scope:** building the dependencies themselves — the `deploy/` stack
  (#256), the etcd `Coordination` backend (#365), the observability floor (#366), the
  S3 wire (#364); the `metadata-tikv` / `server` backend code (0015 slices 1–4);
  Jepsen / Tier-1 / Tier-2 (slice 6); the DST second-implementation pin (slice 7);
  the Helm chart / operator (deferred, 0015); any change to `crates/`, `traits`, the
  `MetadataStore` contract, or the on-disk format; the multi-location / cross-zone
  (M9) story; the honest-performance benchmark run (blueprint §B.3). Backup/DR
  drilling (runbook steps 6–7) is out of the executable-loop scope for this gate —
  the binding loop is steps 1–5 (label → spread → zero → kill → back-to-zero).

- **Ordering note:** this is the **LAST** item of the first-deployment gate (#367)
  and must land **after** its dependencies (#256, #365, #366, #364; #286 done). It
  cannot fully pass until they are in place — it is the composition/verification cap,
  not a leaf. Within the runbook the steps are strictly ordered ("each gates the
  next", blueprint `:691`): labels precede spread-verification precedes zero-assertion
  precedes the kill-and-reconstruct loop.

- **Do model:** opus-max
- **Difficulty:** high — an umbrella/integration gate, not a confined code change.
  Its blast radius is the composition of five prior deliverables plus a
  multi-step, timing-sensitive verification loop (the reconstruction step waits on
  the custodian's async rebuild); the harness must be robust to convergence timing
  and needs a live Docker host with the full stack. Much of its surface depends on
  unbuilt items (see NEEDS-HUMAN).

- **Test file:** the executable runbook **is** the test — e.g.
  `xtask/tests/day_one_first_deployment.rs` or a `deploy/day-one-verify` runner
  invoked by an `xtask` check (path/mechanism **ILLUSTRATIVE**; Do may house it as an
  `xtask` subcommand mirroring `tikv-conformance`). The at-Check regression: the
  script drives label → spread → under-replicated-zero → kill-D-server →
  back-to-zero and **exits non-zero if any gate fails** (e.g. fragments not spread
  across domains, or under-replicated does not return to zero after the kill). Its
  full green requires the live stack (deferred posture — see NEEDS-HUMAN).

- **Citations expected:** Do must cite `path:line` on the target branch AND the
  blueprint/§7.4/0015 for every claim — e.g. the day-one runbook steps
  (`m4-first-deployment-blueprint.md:688-724`), the §7.4 gate ordering
  (`07-deployment-view.md:62`), the D-server `--failure-domain` label flag and
  `50051` bind (blueprint `:588-608`; `crates/server/src/cli.rs:32` per §7.5), and
  the `deploy/` stack it composes (from #256). Where a config/flag name is not yet
  fixed in code (the blueprint's **[wyrd-config]** markers), cite the marker and mark
  **"confirm at build"** — do NOT fabricate flag strings (standing lesson from #253).

- **Disposition hint:** likely NEEDS-HUMAN at first pass — this gate cannot fully
  pass until its dependencies land; expect the human to sign off the deferred/live
  posture rather than a clean at-Check green.

## Invariants to hold

- **The steps are ordered and each gates the next** (blueprint `:691`; §7.4 `:62`):
  label → verify spread → under-replicated-zero → kill-and-reconstruct. Do not
  reorder or short-circuit; a later step must not run if an earlier one fails.
- **Failure-domain labels must reflect actual independence** — the label is "the
  single most important config in the whole deployment" (blueprint `:590-591`,
  `:607-608`; §7.4 references `07-deployment-view.md:47`). The spread check
  (fragments across **distinct** domains) is what makes RS(6,3) real; a passing
  spread check on dishonest labels is a false green.
- **Under-replicated count is the load-bearing signal** — it must read **zero** in
  steady state, and the kill-a-D-server loop's success is defined as the count
  **returning to zero** after the custodian rebuilds (blueprint `:302-303`,
  `:706-717`; §7.4 `:62`). "Reads keep succeeding from survivors" must hold
  throughout the failure window.
- **No crate imports an orchestrator API** — the gate composes `deploy/` artifacts
  outside the Cargo workspace (0015 graduation criterion; ADR-0010); the runbook
  harness must not couple a crate to a k8s/orchestrator API.
- **Internal dials are mTLS, no plaintext fallback** (§7.5 `:84`; ADR-0025) — the
  runbook must not weaken transport security to make a step pass.

## Known NEEDS-HUMAN

- **This brief is the LAST gate item and CANNOT fully pass until its dependencies
  land.** #256 (stack), #365 (etcd Coordination), #364 (S3 wire), and #366
  (observability) are prerequisites; #286 (D-server non-root) is done. At the time of
  writing, the target branch's `deploy/` holds only `README.md` + the slice-1
  throwaway `tikv-single-node/` (confirm at build) — the full multi-node stack is not
  yet present. Until the dependencies are on the branch, the runbook is **BUILT and
  structurally exercisable** (the harness + step logic exist, dry-run/compose-config
  validate) but its **live end-to-end green is observable only once the stack is
  real**. The human must sign off this deferred/live posture; the gate's true pass is
  the day the dependencies land and the script exits `0` on the stood-up stack.
- **OPEN QUESTION carried from #367 — dev-CA first vs waiting for M5.3.** The
  blueprint's stack includes M5 trust-fabric scope (step-ca; `:517-561`), and mTLS is
  fail-closed (ADR-0025) — so the CA must be up before any dial. **Decision needed:**
  run the first-deployment campaign now behind the **built-in self-signed dev-CA**
  (the same `CertificateAuthority` seam; blueprint `:451`, `:524-525`), or **wait for
  M5.3 (#304, the step-ca backend)**. **Recommendation: dev-CA first** — it unblocks
  the day-one runbook against the M4 data plane immediately behind the same seam —
  then **re-run the security-relevant subset** (CA reachability, cert issuance/renewal,
  plaintext-dial-refused: runbook step 1 / blueprint `:557-560`, `:694`) after M5.3
  lands. The human confirms this sequencing at sign-off.
- **Timing/convergence tolerances are not specified** — how long the harness waits
  for the custodian to drive under-replicated back to zero after the kill is an
  operational tuning knob the blueprint does not pin; Do must choose a bound and flag
  it for human review (a too-short bound flakes; a too-long one hides a stalled
  repair). Confirm at build.

## STOP discipline

- Do reads **`brief.md` only**; produces the runbook harness + its named test/check +
  `build-notes.md`; cites `path:line` on the target branch and the blueprint/§7.4/0015
  for every change. Where a flag/config name is a **[wyrd-config]** marker, cite the
  marker and write **"confirm at build"** — never fabricate a flag string (#253).
- Do MAY push to a feature/draft branch and open a **draft** PR into
  `feat/m4-production-metadata-backend` (CI). Do MUST NOT `gh pr ready` or
  `gh pr merge` — that is the human's sign-off step (enforced by `builder_guard.py`).
- Do does not build the dependencies (#256/#365/#366/#364) and does not weaken an
  invariant (label honesty, under-replicated-zero, mTLS, no-orchestrator-import) to
  force a green. If a step cannot pass because a dependency is absent, that is a
  NEEDS-HUMAN to record, not a step to stub past.

[blueprint]: ../wyrd/docs/design/architecture/m4-first-deployment-blueprint.md
[s74]: ../wyrd/docs/design/architecture/07-deployment-view.md
[p15]: ../wyrd/docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md
