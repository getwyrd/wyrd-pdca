# Design proposal — issue 141 / m3.3-custodian-skeleton

> Implementation slice of the **already-accepted** proposal 0005 (Milestone 3 —
> custodians), PR-sequence step 3. The normative design lives in
> `docs/design/proposals/accepted/0005-milestone-3-custodians.md` §"Single active
> custodian", §"Failure-domain-aware placement", §"The durability plane", §"Crate
> touch-points" — an Accepted proposal, immutable (INTEGRATION §2). This brief points
> at 0005 and scopes the one foundation slice for Do; it does not re-decide the design.

- **Slug:** m3.3-custodian-skeleton
- **Kind:** enhancement (design proposal — implements accepted proposal 0005, step 3)
- **Goal:** Stand up the new **`custodian` crate** (L4) — the scaffold every later M3
  loop stands on: a single active custodian per zone, leader-elected and fenced via the
  existing `Coordination::elect_leader`; the reconciliation control-loop skeleton; the
  zone-local **failure-domain-aware selector** (opaque domain id per D server + the
  distinctness invariant), shared by the write fan-out and custodian re-placement; and the
  OpenTelemetry seam wired (Prometheus endpoint + OTLP push, no hardcoded backend).
- **Success criterion:** The new `custodian` crate builds and demonstrates three things at
  C4-verify: (1) a single active custodian is **elected and fenced** — a deposed leader's
  coordination action is **rejected** (stale fencing token); (2) the failure-domain
  selector places a chunk's `n` fragments across `n` **distinct** domains where topology
  allows ≥ n domains; (3) the exporter emits a **first custodian metric** with **no backend
  hardcoded**. BINDING: the crate depends only on `traits` / `core` / `proto` (+ tracing),
  **never** a concrete backend (ADR-0010). (Using `Coordination::elect_leader`'s existing
  fenced `Leadership` token, an *opaque* domain id, and OTel via `tracing` +
  `tracing-opentelemetry` exposing both Prometheus and OTLP are BINDING — accepted design
  in 0005 / ADR-0012. The selector's internal algorithm is ILLUSTRATIVE.)
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2)
- **Depends on:** 139, 140
- **Conflicts with:** 139
- **Ordering note:** needs the placement record + stable D-server id (#139) and the enumerate/delete affordances (#140); 0005 marks steps 1–3 the foundation. Conflicts with #139 on the write fan-out / selector seam (`chunkstore-grpc` + `core`), but `Depends on` already serialises them.
- **Surfaces:** data   (new backend crate + selector + telemetry seam; no GUI; dashboards/UI deferred per ADR-0013)
- **Scope:** create the `custodian` crate and add it to the workspace; implement
  single-active leadership over `Coordination::elect_leader` with the fenced `Leadership`
  token; the reconciliation control-loop skeleton; the zone-local failure-domain-aware
  selector (opaque domain id + n-distinct-domains invariant) wired so the **write fan-out**
  shares it; and the OTel exporter seam (Prometheus + OTLP). / out of scope: the four
  running loops' *behaviour* — GC, scrub, reconstruction, rebalance (0005 slices 4–7); the
  version-conditional repair location-update; the declarative drain/decommission surface
  beyond the reconciliation skeleton; the full DST property campaign + Tier-1/Tier-2 fault
  injection (0005 slice 8); dashboards / alerting / management CLI (ADR-0013, deferred).
- **Test file:** `crates/custodian/tests/skeleton.rs` (new) — asserts the elected leader is
  fenced and a deposed leader is rejected; the selector yields n-distinct-domain placement
  for a topology with ≥ n domains; the exporter emits a first metric. (The crate-boundary
  dependency rule is checked by the build + `cargo xtask ci`.)
- **Verification posture:** NET-NEW infrastructure (template posture (a)) — a brand-new
  crate with no prior failing assertion to flip, so "red" is **criterion-absence** (the
  crate / its API does not yet exist). Do should make the seams demonstrably load-bearing
  rather than resting green on non-existence: the **fenced-deposed-leader** assertion is a
  genuine red→green property (a deposed leader *must* be rejected — negate the fencing
  check to show the test fails), and the **selector distinctness** assertion likewise. The
  exporter-emits-a-metric leg is observable in-process (assert on the exported metric /
  span) at C4-verify; backend-agnosticism is confirmed by the dependency-only-on-traits/core/proto
  build + `cargo xtask ci`. Full leadership/selector behaviour under fault is DST-validated
  in slice 8 — supplementary evidence, off-Check.
- **Citations expected:** Do must cite path:line on `origin/main` for every change
  (`crates/traits/src/lib.rs:209-211` `elect_leader`/`Leadership`; new `crates/custodian/*`;
  `Cargo.toml` workspace members; the write fan-out in `crates/chunkstore-grpc/src/fanout.rs`
  that adopts the shared selector).
- **Prior-art check (triage cycles):** searched merged history and all PRs by file path —
  `crates/custodian` does **not** exist on `origin/main` and appears in **no** PR; the
  workspace `members` list (Cargo.toml) has no `custodian` entry; `Coordination::elect_leader`
  + `Leadership` already exist (`crates/traits/src/lib.rs:209-211`). Net-new crate.
- **Disposition hint:** new-feature

## Motivation
M3 is "the self-maintaining durability plane" — GC, scrub, reconstruction, rebalance + the
durability telemetry that makes a single zone trustworthy and gates the Step-2 release. All
four loops, plus the telemetry that is itself a graduation criterion (ADR-0011: metrics
emitted from the custodians' first commit), need a home and a shared scaffold. This slice
builds that scaffold so the later, largely-independent loops (the natural parallel split for
multiple contributors) each layer on rather than re-invent leadership, placement, and the
telemetry seam.

## Design
Per 0005 (authoritative), this slice is the **foundation** of the crate:
- **Single active custodian, fenced** (§"Single active custodian, fenced"): one active
  leader per zone, elected via the **existing** `Coordination::elect_leader`, which returns
  a fenced `Leadership` token (`crates/traits/src/lib.rs:209-211`). A deposed-but-running
  custodian is made safe by the fencing token (its coordination actions are rejected) and —
  for later mutating slices — version-conditional commits. Sharded scrub/repair is an Open
  question, **not** M3 scope.
- **Failure-domain-aware selector** (§"Failure-domain-aware placement"): each D server
  carries an opaque failure-domain label (rack/power/switch, §7.3) from config, surfaced
  through registration (the stable-id registration from #139). A thin domain-aware selector,
  **shared by the write fan-out and custodian re-placement**, enforces n fragments across n
  distinct domains where topology allows. The M3 abstraction is kept thin (opaque domain id
  + distinctness invariant + per-domain utilization) so M6's placement *policy* layers on.
- **OTel seam** (§"The durability plane", ADR-0012): instrumentation via `tracing` +
  `tracing-opentelemetry`, exposing **both** a Prometheus-scrapeable endpoint and OTLP push,
  hardcoding no backend; this slice wires the seam + emits a first custodian metric so every
  later loop emits from its first commit.
- **Crate boundary** (§"Crate touch-points", ADR-0010): `custodian` deps `traits`, `core`,
  `proto`, `tracing`/`tracing-opentelemetry` — **never** a concrete backend.

## Alternatives considered
Settled in 0005 §"Alternatives considered" and not reopened: multi-active / sharded
custodians (deferred — single active is the M3 choice; sharding is an Open question); a
hardcoded telemetry backend (rejected — ADR-0012 requires backend-agnostic OTel); folding
the selector into the fan-out only (rejected — it must be shared with custodian re-placement).
0005 is Accepted; a change requires a superseding proposal (INTEGRATION §2).

## Impact & compatibility
Adds a workspace member (`Cargo.toml` `members` + `[workspace.dependencies]`) and the
`tracing-opentelemetry` / `opentelemetry-otlp` (+ Prometheus exporter) **dependencies** —
these must clear the `cargo-deny` allowlist (ADR-0003 three-test audit). **A new dependency
is a project-defined human-only item (INTEGRATION §4 / §10) → expect a NEEDS-HUMAN at
sign-off**, not a model accept. The write fan-out changes to consume the shared selector
(coordinate with #139, which retires `index % n`). No on-disk-format change.

## Open questions
- Exact selector API shared with the fan-out (where it lives — `core` vs. `custodian`,
  re-exported) — a Do call within the BINDING criterion; flag at sign-off if it forces a
  trait change.
- The `tracing-opentelemetry` / `opentelemetry-otlp` / Prometheus-exporter versions and
  their `deny.toml` allowlist entries — **NEEDS-HUMAN** (new-dependency audit, ADR-0003).
- `server`'s `custodian` subcommand/role wiring (ADR-0014/0016) is named in 0005 but is
  beyond this skeleton slice — keep the crate runnable in-process under test; defer the
  server role unless sign-off pulls it in.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Re-plan the brief; review #139 first before re-issuing. The builder's central scope decision rests on a false premise. Build-notes (base 3ca818b) state "#139 is not merged on the target base" and on that basis deliberately left the write fan-out routing `index % n` (chunkstore-grpc/src/fanout.rs:25) and only co-located the selector in core::placement, deferring the rewire to #139. Human confirms #139 was already implemented. Consequences to resolve at Plan: - Re-establish the correct target base (one that includes #139); the patch appears built on a stale base. - The brief's stated scope ("selector wired so the write fan-out shares it", "retires index % n") was achievable and expected here, not deferrable. - Reconcile with #139's own selector / domain-aware fan-out so this slice does not duplicate or conflict with it. - Revisit the "Depends on / Conflicts with #139" framing in the brief now that #139 is in. §6 items left UNCLEARED (not an accept): C4 negation-red, C5/T4 fan-out sharing, T5 telemetry seam, Validation new-dep audit.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — ./engine/scripts/run-verify.sh
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
