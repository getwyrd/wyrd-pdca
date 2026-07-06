# Check review — issue 366 / obs-floor-observability (keystone slice, 0010 items 1–2)

**Task under review:** deliver the observability-floor keystone — make the M3 library
durability plane *observable through a running custodian role* so that after an injected
fragment loss the under-replicated count RISES then RETURNS TO ZERO, read back off the real
Prometheus export surface (`DurabilityTelemetry::gather_prometheus`). The patch (a) changes
`emit_under_replicated` from a `monotonic_counter.` to a `gauge.` tracing field, and (b) adds
a new `CustodianRole` seam (`crates/custodian/src/role.rs`) that owns a telemetry handle and
runs the fenced `reconcile_step` with that handle's metrics bridge installed per-pass, plus a
new day-one-signal test and a field-name update to the existing DST property.

**Grounding note:** `$PDCA_TARGET` resolved to the per-cycle worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l0` (an `-l0` stacked-cycle level; `role.rs` present only
there — patch applied). Target is readable and consistent with `patch.diff`; every citation
below grounds on that source. Direct `cargo test`/`xtask ci` re-execution was blocked by
sandbox approval-gating (no human in loop), so C4/T3 rest on the recorded gate results
(`check-gates.json`: C4-ci **pass**, C4-verify **pass**) plus source inspection and an
analytical red→green re-derivation, not an independent re-run.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's BINDING criterion (brief.md:51–59) — rise-then-zero via `gather_prometheus` through the wired role — is a concrete, testable oracle; patch targets exactly it. Scope of *which* slice is a human call (see T4), but the spec for this bundle is unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | Re-derived the flip: pre-fix `monotonic_counter.` → an accumulating OTel/Prometheus counter, so pass-2 `add(0)` leaves it pinned at 1; the `Some(0.0)` "returns to zero" assert (reconstruction_telemetry.rs:562–568) reads 1 → RED. The line is genuinely flippable. Could not execute (sandbox); logic + gate C4-verify concur. |
| C3 Change | PASS | Additive: new `role.rs` (custodian/src/role.rs:1–118, module wired at lib.rs:9,17), one-line emit change (reconstruction.rs:515–516), new test + a DST field-name update (custodian.rs:578,587). No commit-protocol / on-disk / consistency-contract surface touched — invariants (brief.md:153–168) hold. |
| C4 Verification (red→green) | PASS | Gate C4-ci **pass** and C4-verify **pass** (check-gates.json:33–48). Post-fix `gauge.` sets a level → pass-1 reads 1, pass-2 reads 0 → GREEN; the fix flips exactly the asserted leg. `gauge.`/`monotonic_counter.` prefixes and `gather_prometheus`/`flush`/`metrics_layer` all exist on the target (telemetry.rs:116,125,135). Independent re-run blocked by sandbox — verdict rests on gate + derivation. |
| C5 Causal adequacy | PASS | Root cause addressed, not masked: the counter→gauge change fixes *why* the signal never returned to zero (a level modelled as a cumulative), and the role wires emission into a real export surface rather than ad-hoc test capture. Symptom-guard smell-test: **no** capability probe / try-fallback / runtime guard around an optional capability — does not fire. `reconstruction_repaired` correctly stays a monotonic counter (reconstruction.rs:527). |
| T1 Structure | PASS | `role.rs` lives in the `custodian` crate beside the seam it wires; delegates to the real `reconcile_step` (the anti-#141 single-control-point guard, reconciliation.rs:65–73) rather than forking a parallel entry; test in its own binary is justified by `tracing` per-callsite interest caching (reconstruction_telemetry.rs:205–207). |
| T2 Shape | PASS | `reconcile_pass` mirrors `reconcile_step`'s exact argument shape (role.rs:96–117 vs reconciliation.rs:66–73) and adds only `.with_subscriber(dispatch.clone())`; bridge installed scoped, not as a global default (ADR-0035) — matches the existing telemetry seam conventions. |
| T3 Runtime | PASS | Gate xtask ci (fmt/clippy/build/test/deny/conformance) reported pass (check-gates.json:33–39); dispatch built once at construction and cheap-cloned per pass. Not independently re-executed (sandbox approval-gated). |
| T4 Contribution | NEEDS-HUMAN | Decision owed: **which 0010 slice this bundle carries.** The patch delivers the *library-seam* half of items 1–2 (`CustodianRole` + in-process read-back) but explicitly DEFERS the deployable `wyrd custodian` binary in the `server` crate (role.rs:26–32). Whether the library-seam role satisfies the brief's "runnable custodian role... as its own deployable process" (brief.md:47,51) for the #367 gate, and whether prior-art/closed work on this seam was cleared, are the maintainer's milestone-decomposition call (brief.md:170–174). |
| T5 Judgment | NEEDS-HUMAN | Two recorded-decision items 0010 requires but the patch settles unilaterally / leaves open: (1) shared `crates/telemetry` extraction **vs keep-in-`custodian`** — patch keeps it in `custodian` (role.rs in that crate) without a recorded sign-off (brief.md:178–179); (2) typed-errors × M4.4 (#255) sequencing is untouched (item 6 deferred) and 0010 requires the decision be *recorded* before parallel work (brief.md:175–177). Human must ratify both. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Off-Check evidence owed: the live Prometheus-scrape / OTLP-collector day-one run on a Tier-2 single node against the blueprint checklist (brief.md:67–70,180–182) — the ultimate #367 first-deployment gate. Also a fitness call: the `monotonic_counter`→`gauge` semantic change ripples to any dashboard/alert that treated `reconstruction_under_replicated` as cumulative; confirm no downstream consumer regresses. In-process rise-then-zero is demonstrated at Check; the live scrape is not, by design. |
