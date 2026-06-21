# Brief (pointer) — issue 141 / m3.3-custodian-skeleton

> Plan artifact for an implementation slice whose design ALREADY lives in an
> accepted, immutable host artifact — proposal 0005 (Milestone 3 — custodians),
> PR-sequence step 3. This brief POINTS at 0005 (it does not restate or re-decide
> the design; INTEGRATION §2/§6) and carries the fields the driver/Do parse, plus
> the structural-slice fields (invariant, posture, prior-art) this category needs.
> Do reads the **Planning artifact** as authoritative.
>
> **Iteration 2.** The first attempt is preserved in `iteration-v1/` and was
> re-planned — see the carry-forward at the foot. Scope settled with the human as
> **Option B (full production wiring)**: this slice also pulls in slice-1's
> undelivered "retire `index % n`" + registration failure-domain label.

- **Slug:** m3.3-custodian-skeleton
- **Planning artifact:** `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
  — §"Single active custodian, fenced", §"Failure-domain-aware placement", §"The
  durability plane", and the PR-sequence (slice **3** at `0005:518-523`, plus the
  slice-1 leftovers `retire index % n` + registration failure-domain label at
  `0005:510-513`). Supporting: **ADR-0010** (crate-boundary), **ADR-0011** (telemetry
  from first commit), **ADR-0012** (backend-agnostic OTel), architecture §7.3
  (failure-domain labels). Authoritative; Do cites it for every design claim.
- **Defect / goal:** M3 has no `custodian` crate and no failure-domain-aware
  placement: chunk placement is still M2's stateless identity `index % n`
  (`core/write.rs:73`, `fanout.rs:58-60`), so a chunk's `n` fragments are **not**
  guaranteed across `n` distinct failure domains; there is no single-active fenced
  custodian and no durability-telemetry seam. Stand up the crate and make placement
  failure-domain-aware end-to-end.
- **Success criterion:** Demonstrable at C4-verify on the new base:
  (1) the `custodian` crate builds and a single active custodian is **elected** via
  `Coordination::elect_leader` and **fenced** — a deposed leader's coordination
  action is **rejected** (stale fencing token). BINDING.
  (2) the failure-domain-aware **selector** places a chunk's `n` fragments across
  `n` **distinct** domains where topology offers ≥ n domains, and **refuses**
  (errors) when domains < n. BINDING (the distinctness invariant; the selector's
  internal algorithm is ILLUSTRATIVE).
  (3) **production write path wired (Option B):** a D server's registration carries
  an **opaque failure-domain label**; the committed placement record reflects the
  selector's distinct-domain choice — a write into a topology where `index % n`
  would collide domains records a **distinct-domain** placement (NOT the identity
  vector), and the read resolves fragments from that record. BINDING — this is the
  "retire `index % n` at the write" of Option B.
  (4) the OTel seam emits a **first custodian metric** via `tracing` +
  `tracing-opentelemetry` exposing **both** a Prometheus endpoint and OTLP push,
  with **no backend hardcoded** (the dual-export surfaces are BINDING per ADR-0012;
  the in-process assertion mechanism is ILLUSTRATIVE).
  (5) BINDING boundary: `custodian` depends only on `traits`/`core`/`proto` (+
  `tracing`) — **never** a concrete backend (ADR-0010).
- **Invariant to restore:**
  • **Failure-domain durability** — every placement decision (initial write AND
  custodian re-placement) puts a chunk's `n` fragments on `n` **distinct** failure
  domains wherever topology offers ≥ n domains; placement is never the domain-blind
  `index % n`. Source: 0005 §"Failure-domain-aware placement" (`0005:235-245`,
  invariant `0005:491`), architecture §7.3, ADR-0011. (Spans the selector + the
  write commit + registration topology — not satisfiable by guarding one module.)
  • **Single-active safety** — at most one custodian acts per zone; a superseded
  leader's actions are rejected by the monotonic fencing token. Source: 0005
  §"Single active custodian, fenced" (`0005:358-383`); fencing token
  (`traits/src/lib.rs:39,326-328`).
  • **Telemetry from first commit** — durability telemetry is emitted through a
  backend-agnostic OTel seam from the custodian's first commit; no backend
  hardcoded. Source: ADR-0011, ADR-0012, 0005 §"The durability plane".
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — single line, no
  maintenance branches)
- **Surfaces:** data   (new backend crate + selector + registration/write rewire +
  telemetry seam; no GUI — dashboards/UI deferred per ADR-0013)
- **Scope:** **Option B (full production wiring).** Create the `custodian` crate
  (workspace member; deps `traits`/`core`/`proto`/`tracing` only); single-active
  fenced leadership over `Coordination::elect_leader`; the reconciliation
  control-loop scaffold; the zone-local **failure-domain-aware selector** in `core`
  (opaque domain id per D server + the n-distinct-domains invariant + per-domain
  utilization, kept thin per `0005:251`), **shared by the write fan-out and
  custodian re-placement**; add the **opaque failure-domain label** to D-server
  registration/discovery so the write path builds a topology; **rewire the write**
  so the committed placement record reflects the selector's distinct-domain choice
  (retiring identity `index % n` as the write's placement choice); and the OTel
  exporter seam (Prometheus + OTLP) emitting a first custodian metric.
  / **out of scope:** the four running loops' *behaviour* — GC, scrub,
  reconstruction, rebalance (0005 slices 4–7); the version-conditional
  commit-point-atomic location update for custodian *relocation* of an existing
  fragment (slice 6); declarative drain/decommission beyond the reconciliation
  skeleton; the full DST property campaign + Tier-1/Tier-2 fault injection (slice
  8); dashboards/alerting/management CLI (ADR-0013); M6 placement *policy*.
- **Test file:**
  `crates/custodian/tests/skeleton.rs` (new) — elected leader is fenced and a
  deposed leader is rejected; the selector yields `n`-distinct-domain placement for
  a topology with ≥ n domains and refuses when domains < n; the exporter emits a
  first metric. AND
  `crates/core/tests/domain_placement.rs` (new) — a write into a topology where
  `index % n` would collide domains records a **distinct-domain** placement (not the
  identity vector) and the read resolves from it: the flippable red→green that
  retires `index % n` at the write.
- **Verification posture:** Mixed.
  (a) **Flippable red→green** (default) for the behavioural legs: the
  domain-aware write placement (`crates/core/tests/domain_placement.rs`) is a
  genuine red→green — pre-change the write records identity `index % n`
  (`core/write.rs:73`) which collides domains, post-change it spreads; the
  leadership-fence and selector-distinctness/refusal assertions are flippable
  (negate the fence check / distinctness to show red).
  (b) **NET-NEW infrastructure** for the crate-existence legs (no prior assertion to
  flip — "red" is criterion-absence): Do must make seams load-bearing via a
  demonstrated negation-red (negate fencing / distinctness), not rest green on
  non-existence. The OTLP+Prometheus dual-export is observable **in-process** at
  C4-verify (assert the exported metric/span); a *live* Prometheus scrape / OTLP
  collector run is supplementary evidence **off-Check** (the human/CI confirms).
  (c) **Project human-only NEEDS-HUMAN at sign-off:** the new dependencies
  (`tracing-opentelemetry` / `opentelemetry-otlp` / a Prometheus exporter) require
  the cargo-deny three-test audit + `deny.toml` allowlist (ADR-0003, INTEGRATION
  §4/§10) regardless of a green `deny` gate.
- **Citations expected:** Do cites `path:line` on `origin/main` AND proposal 0005
  for every change. Pre-existing seams: `traits/src/lib.rs:53` (`DServerId`, opaque,
  doc already names #141), `:274`/`:285` (`register`/`discover`), `:288`/`:326-328`
  (`elect_leader`/`Leadership`/`token`); `coordination-mem/src/lib.rs:188`
  (token rises every grant); `core/write.rs:73` (identity placement to retire);
  `core/read.rs:74-101` (read resolves from the record); `fanout.rs:58-60`,`:120`
  (`index % n` route + `PlacementChunkStore` impl); `server/src/cli.rs:276`,`:429`
  (D-server register + fan-out construction); `Cargo.toml:9-21` (workspace members).
- **Prior-art check (triage cycles):** searched merged history + open/closed PRs by
  file path. `crates/custodian` does **not** exist on `origin/main` and is in **no**
  PR; no `failure_domain`/selector exists in source. **#139** (PR #185, **merged**)
  added `DServerId` + `PlacementChunkStore` + the placement record but left the write
  at identity `index % n` (`core/write.rs:73`) and added **no** registration
  failure-domain label — so 0005 slice-1's "retire `index % n`" + the label remain
  undone and are pulled into this slice per Option B. **#140** (PR #186, **merged**)
  added `list_fragments`/`delete_fragment`. **Iteration-1 of THIS bundle**
  (`iteration-v1/`) built the skeleton on a **stale base** (`3ca818b`, before
  #139/#140) and **deferred** the fan-out wiring — rejected; do not repeat.
- **Disposition hint:** likely-fix
- **Ordering note:** #139 (PR #185) and #140 (PR #186) are **already merged into
  `origin/main`** (this slice's base, `ae6be66`), so their prerequisites are
  satisfied in-base — no `Depends on`/`Conflicts with` scheduling lines are needed
  (the iteration-1 `Conflicts with #139` framing is now moot). By the human's Plan
  decision this slice (Option B) **absorbs** slice-1's undelivered "retire
  `index % n`" + registration failure-domain-label work.

## Iteration 2 — carry-forward (from the previous attempt)

- **Why iteration-1 was re-planned:** the builder built on a **stale base**
  (`3ca818b`, before #139/#140 merged) and, on the false premise "#139 is not
  merged," deferred wiring the selector into the write fan-out and left
  `index % n` in place. The base is now correct (`origin/main` @ `ae6be66`
  includes #139/#140).
- **What changed this iteration:** scope settled with the human as **Option B** —
  examining #139 as actually merged revealed it did **not** retire `index % n` and
  added **no** registration failure-domain label (the selector's production input).
  So full production wiring here must also add that label + the write rewire; both
  are in-scope above.
- **§6 items left UNCLEARED in iteration-1, to satisfy now:** C4 negation-red
  (demonstrate the fence/distinctness reds); C5/T4 selector **actually shared by the
  write fan-out** (not just co-located) — the binding leg (3) above; T5 telemetry
  seam fidelity (dual Prometheus+OTLP, not a bare in-memory stub); Validation
  new-dependency cargo-deny audit (ADR-0003) — a sign-off NEEDS-HUMAN.
- Do **not** re-attempt the rejected (deferred-fan-out) approach. Satisfy the
  Success criterion end-to-end.

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
