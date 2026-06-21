# Check review — issue 141 / m3.3-custodian-skeleton

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json` (build-notes.md withheld). Citations re-derived
> against the target source at `/home/eddie/wyrd/wyrd` (read-only) and the patch.
> The patch is **not** applied to the target — the custodian crate is absent there
> (net-new), so target citations ground the *pre-existing seams* the patch builds
> on; `patch.diff` grounds the additions.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:18-27` gives a concrete oracle: three demonstrable criteria (elected+fenced / n-distinct-domain selector / first metric, no backend hardcoded) plus BINDING constraints. Unambiguous "fixed" definition. |
| C2 — C2 Reproduction (red pre-fix) | N/A | Net-new infrastructure (`brief.md:46-55`): the crate/API does not exist on target (`crates/custodian` absent — confirmed), so there is no prior failing assertion to reproduce; "red" is criterion-absence. A literal pre-fix red does not apply. Whether Do demonstrated the negation-red is checked under C4. |
| C3 — C3 Change | PASS | Patch implements the described change coherently: workspace member added (`Cargo.toml:36`), shared selector in `core` (`crates/core/src/lib.rs:71` + new `placement.rs`), custodian modules `leadership`/`reconcile`/`selector`/`telemetry` (`crates/custodian/src/lib.rs:392-395`), re-export seam (`selector.rs:11`). Net-new, consistent with brief scope. |
| C4 — C4 Verification (red→green) | NEEDS-HUMAN | Split gate: `C4-ci` (fmt/clippy/build/test/deny/conformance) **PASSED** (`check-gates.json:33-39`), but the per-fix `C4-verify` red→green **FAILED** (`check-gates.json:41-48`). The fail is consistent with net-new infra (no pre-fix assertion to flip). Brief prescribes a manual red demonstration — negate the fencing check / selector distinctness to show the test fails (`brief.md:49-53`) — recorded in build-notes, which is **withheld** from me. Human must confirm the negation red→green was actually shown. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Two of three causal claims hold and are re-verified: fencing is genuine — `MemCoordination::elect_leader` increments the token every call (`coordination-mem/src/lib.rs:188-190`), so the deposed leader's stale token is strictly less and `LeadershipFence::guard` rejects it (`leadership.rs:346-355`); selector distinctness is enforced by `HashSet::insert` on the domain (`placement.rs:199`) with refusal on too-few domains (`placement.rs:206-211`). BUT the brief's claim that the selector is "**shared by the write fan-out**" (`brief.md:36-37,108-109`) is **not wired** — the fan-out on target still routes `index % n` (`chunkstore-grpc/src/fanout.rs:25`) and the patch does not touch it; sharing is structural (co-located in `core`) only. Scope adequacy of that gap is a human call (and is flagged as a Do/#139 coordination Open question, `brief.md:111-114`). |
| T1 — T1 Structure | PASS | Test lives at the brief-specified path `crates/custodian/tests/skeleton.rs` (`brief.md:42-44`); four well-formed `#[test]` fns covering the three criteria + the refusal leg (`tests/skeleton.rs:570,604,635,650`). |
| T2 — T2 Shape | PASS | Assertions target the right properties: deposed action `is_err()` and token strictly rises (`tests/skeleton.rs:586,596`); n placements in n DISTINCT domains via `HashSet` (`tests/skeleton.rs:618-623`); refusal when domains < n (`tests/skeleton.rs:641`); first metric present (`tests/skeleton.rs:655-660`). Shapes match the oracle. |
| T3 — T3 Runtime | PASS | Tests compile and run green: `C4-ci` includes `test` and passed (`check-gates.json:33-37`). Cannot re-run independently here (no build), but the CI gate is authoritative for runtime-green. |
| T4 — T4 Contribution | PASS | Tests are load-bearing, not vacuous: the fencing test fails if `guard` is relaxed; the selector tests fail if distinctness/refusal break. Note a coverage gap consistent with C5 — no test exercises the write fan-out actually consuming the shared selector (because it is not wired); that is the same scope item, not a test defect. |
| T5 — T5 Judgment | NEEDS-HUMAN | Gate oracle is "reviewer + human sign-off" (`check-gates.json:98`). Design-judgment questions remain: is `InMemoryExporter` + a `tracing` event sufficient evidence for the BINDING OTel/Prometheus+OTLP dual-export (`brief.md:25-27`), given the real exporter deps are deferred (`telemetry.rs:466-470`)? Is `MemCoordination`'s "always-leader, rising-token" a faithful fence model (`leadership.rs:268-271`)? Human judgment required. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always human (`check-gates.json:108`). Plus a project-defined human-only item: this patch adds a **new workspace dependency** `tracing` (`Cargo.toml:59`), which `brief.md:106-108` declares a NEEDS-HUMAN at sign-off (INTEGRATION §4/§10, cargo-deny audit ADR-0003) regardless of the green `deny` gate. The BINDING OTLP+Prometheus dual-export is deliberately deferred (stubbed behind `MetricExporter`); whether the skeleton's in-process seam satisfies the slice's purpose is the human's call. |

## §6 — NEEDS-HUMAN items the human must clear

1. **C4 red→green demonstration (net-new).** The automated `C4-verify` failed
   (`check-gates.json:41-48`) because a net-new crate has no pre-fix assertion to
   flip. The brief requires a manual negation-red (negate the fencing check /
   selector distinctness and show the test fails — `brief.md:49-53`). The evidence
   lives in the withheld build-notes; confirm it was actually performed.

2. **C5 / T4 scope — "selector shared by the write fan-out."** Brief Scope says the
   selector is "wired so the write fan-out shares it" (`brief.md:36-37`) and Impact
   says the fan-out "changes to consume the shared selector … retires `index % n`"
   (`brief.md:108-109`). The patch only co-locates the selector in `core` and
   re-exports it in `custodian`; the fan-out (`chunkstore-grpc/src/fanout.rs:25`) is
   untouched. Brief also flags this as a Do/#139 coordination Open question
   (`brief.md:111-114`). Ambiguous scope — decide whether structural co-location
   satisfies this slice or the fan-out rewire is required now.

3. **T5 judgment — telemetry seam fidelity.** The BINDING OTel via
   `tracing-opentelemetry` exposing both Prometheus and OTLP (`brief.md:25-27`) is
   deferred; only base `tracing` + an in-process `InMemoryExporter` is wired
   (`telemetry.rs:466-470`). Judge whether the in-process metric assertion is
   adequate evidence for the criterion at this slice.

4. **Validation — new-dependency audit (project human-only).** `tracing` added to
   workspace deps (`Cargo.toml:59`); `brief.md:106-108` mandates a NEEDS-HUMAN
   sign-off for any new dependency (cargo-deny allowlist, ADR-0003) despite the
   green `deny` gate.

5. **Validation — fitness-to-purpose.** Always human (`check-gates.json:108`):
   does this skeleton stand up the load-bearing seams the four M3 loops layer on,
   per proposal 0005 step 3?

## Notes

- Gating status as reported: `overall: pass`, with the one gating gate `C4-ci`
  green; `C4-verify` is non-gating and red (`check-gates.json:46-47`) — surfaced
  above rather than buried.
- Compile soundness of the additions is corroborated against target seams:
  `wyrd_traits::Result` is `Result<T, BoxError>` (`traits/src/lib.rs:49`), so the
  `format!(...).into()` error path in `leadership.rs:348-353` is valid; `Leadership`
  is `Copy` (`traits/src/lib.rs:248`), so `leadership()` by-value returns are sound.
