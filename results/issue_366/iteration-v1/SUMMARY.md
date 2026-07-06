# Result — issue 366 / obs-floor-observability

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: 
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: the minimum operational-visibility floor (0010 §"Scope boundary" items 1–7):

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (for the keystone slice — items 1–2 delivering the
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

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

### Advisory — adversary

# Adversarial review — issue 366 (obs-floor keystone: counter→gauge + `CustodianRole`)

Ran the asserted evidence at `$PDCA_TARGET`: the diff is applied to the worktree;
`cargo test -p wyrd-custodian --test reconstruction_telemetry` → **green** (1 passed).
Confirmed the gauge mechanism is real: `tracing-opentelemetry 0.33` maps the `gauge.`
prefix to a sync `Gauge::record` (metrics.rs:24,176,205,226), and `opentelemetry-prometheus
0.32` appends `_total` only to *monotonic* sums (lib.rs:122-124,196). So the fix is
mechanically sound. Findings below are the places it is thinner than the verdict implies.

## Findings

- **NEEDS-HUMAN — the binding "day-one" gauge reads `0` during the *worst* durability loss.**
  `crates/custodian/src/reconstruction.rs:170` emits `emit_under_replicated(plans.len())`,
  and `plans` holds only `Assessment::Repairable` chunks; a chunk that has lost more than
  `m` fragments is `Assessment::Unrepairable` and the arm is a no-op
  (`reconstruction.rs:145` `=> {}`) that emits nothing. **Concrete failing case:** kill 2 of
  the 3 fragments of the test's RS(2,1) chunk (survivors = 1 < k = 2). The chunk is on the
  brink of *permanent* loss, yet `emit_under_replicated(0)` fires and
  `reconstruction_under_replicated` reads **0** off `gather_prometheus`. This diff is what
  *promotes* this metric to "the day-one durability signal", so it now owns the semantics
  "gauge = 0 ⇒ zone healthy" — which is false for below-k chunks. The single test only
  exercises the recoverable (1-of-3-lost) case, so this inversion is never seen. Whether the
  floor's binding signal may silently read healthy while data is being lost is a human call.

- **The red→green discriminates for a *different* reason than the patch documents.**
  The code comment (`reconstruction.rs:510-513`) and the test docstring
  (`crates/custodian/tests/reconstruction_telemetry.rs:198-203`) claim a monotonic counter
  "reads back 1 then stays pinned at 1", making the pass-2 *returns-to-zero* assertion RED.
  That is not what happens: a counter is exported as `reconstruction_under_replicated_total`
  (opentelemetry-prometheus lib.rs:196), and `gauge_value()` queries the un-suffixed name, so
  a counter is simply **absent** → the *pass-1* `Some(1.0)` assertion
  (`reconstruction_telemetry.rs:536`) fails first; the test never reaches the pass-2 leg the
  narrative rests on. The fix works and the emit-line-level red is genuine, but the reviewer
  should not credit the elaborate "accumulating counter" causal story — it describes a path
  the assertion never walks.

- **The DST change is cosmetic and carries zero independent evidence.**
  `crates/dst/tests/custodian.rs:1023,1046` only renames the capture key
  `monotonic_counter.` → `gauge.`. `MetricCapture.values()` reads the *raw per-event tracing
  field value* (`count as u64` = 1 then 0), which is identical for a counter or a gauge — it
  is exactly the "bespoke per-event capture layer" the new test's own docstring disparages
  (`reconstruction_telemetry.rs:182-189`). It would stay green under either instrument type
  as long as the key matches. So the entire red→green rests on the *single* new test; there
  is no second, independent oracle for the gauge behaviour.

- **NEEDS-HUMAN — "the wired, runnable custodian role (not the library alone)" is met by
  relabeling, not by a runnable process.** `CustodianRole` is constructed **nowhere** but the
  test (`grep`: only `crates/custodian/src/lib.rs:44` re-exports it); the `server` crate still
  has no `custodian` dependency and `custodian` remains a `dst`-only dep
  (`crates/dst/Cargo.toml:44`, unchanged). `CustodianRole` (`crates/custodian/src/role.rs`)
  is a library struct whose sole added behaviour over calling `reconcile_step` by hand is
  owning a `Dispatch` and wrapping one pass in `.with_subscriber(...)` (`role.rs:158-168`);
  there is no run loop, leadership lifecycle, or binary. This is consistent with the brief's
  "keystone slice, deployment half deferred" disposition, but the brief's binding phrasing
  "not the library alone" is satisfied nominally — the human should confirm the keystone is
  accepted on that basis and that item 2's deployment half is booked as a follow-on slice.

## Attempted refutations that FAILED (fix survives)

- *Scoped subscriber loses emissions from spawned children.* `with_subscriber` only covers
  the wrapped future, so a `tokio::spawn` inside a loop would emit into the global no-op
  dispatcher. Grepped `crates/custodian/src` — **no** `spawn`/`join!`/`block_in_place`; every
  loop is sequential `.await`. Holds for this diff (but is a latent trap for the deferred
  continuous run loop / any future loop that spawns — worth a comment there).
- *Gauge doesn't actually return to zero through the pull exporter.* Verified green: the
  `Dispatch` (and its `Instruments`/gauge) is built once in `role.rs:120-121` and shared, so
  pass-2's `record(0)` overwrites the single (label-less) series; `flush()` + `gather()` reads
  0. Confirmed empirically.
- *Per-chunk label leaves a stale `1` series after repair.* `emit_under_replicated` carries no
  attributes (`reconstruction.rs:515-516`), so both passes hit the same series — no staleness.

## Caveat on the red→green gate

`run-verify.sh` and `build-notes.md` are withheld here, so I could not see *which* lines the
harness reverted to establish "red". If it reverted the whole diff, the red is a
*compile* failure (missing `CustodianRole`/`gauge.`), which proves the API is new, not that
the assertion catches the defect. I independently established the stronger claim — reverting
*only* the `emit_under_replicated` line to `monotonic_counter.` yields an *assertion* red
(the `_total`-suffix name miss → pass-1 `Some(1.0)` fails). A human confirming the gate should
ensure the red was taken at that emit-line level, not at compile time.

### Advisory — codex

- NEEDS-HUMAN — `crates/custodian/src/role.rs:111` wraps the entire reconciliation future in the role's private dispatch, while `crates/custodian/src/role.rs:69` builds that dispatch from only `Registry::default().with(telemetry.metrics_layer())`. That makes durability metrics flow, but it also shadows any caller/global subscriber for all tracing events emitted inside `reconcile_step`; the deferred structured-stderr / `RUST_LOG` slice may not see custodian-loop logs unless this role can accept or compose a caller-provided subscriber/dispatch.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Decision owed: **which 0010 slice this bundle carries.** The patch delivers the *library-seam* half of items 1–2 (`CustodianRole` + in-process read-back) but explicitly DEFERS the deployable `wyrd custodian` binary in the `server` crate (role.rs:26–32). Whether the library-seam role satisfies the brief's "runnable custodian role... as its own deployable process" (brief.md:47,51) for the #367 gate, and whether prior-art/closed work on this seam was cleared, are the maintainer's milestone-decomposition call (brief.md:170–174).
- [ ] T5 Judgment — Two recorded-decision items 0010 requires but the patch settles unilaterally / leaves open: (1) shared `crates/telemetry` extraction **vs keep-in-`custodian`** — patch keeps it in `custodian` (role.rs in that crate) without a recorded sign-off (brief.md:178–179); (2) typed-errors × M4.4 (#255) sequencing is untouched (item 6 deferred) and 0010 requires the decision be *recorded* before parallel work (brief.md:175–177). Human must ratify both.
- [ ] Validation — fitness-to-purpose — Off-Check evidence owed: the live Prometheus-scrape / OTLP-collector day-one run on a Tier-2 single node against the blueprint checklist (brief.md:67–70,180–182) — the ultimate #367 first-deployment gate. Also a fitness call: the `monotonic_counter`→`gauge` semantic change ripples to any dashboard/alert that treated `reconstruction_under_replicated` as cumulative; confirm no downstream consumer regresses. In-process rise-then-zero is demonstrated at Check; the live scrape is not, by design.
- [ ] `crates/custodian/src/role.rs:111` wraps the entire reconciliation future in the role's private dispatch, while `crates/custodian/src/role.rs:69` builds that dispatch from only `Registry::default().with(telemetry.metrics_layer())`. That makes durability metrics flow, but it also shadows any caller/global subscriber for all tracing events emitted inside `reconcile_step`; the deferred structured-stderr / `RUST_LOG` slice may not see custodian-loop logs unless this role can accept or compose a caller-provided subscriber/dispatch.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rebuild to (1) extract the telemetry seam into a shared `crates/telemetry` crate — `DurabilityTelemetry`/`ExporterConfig`/`metrics_layer` must not be anchored in `custodian`; the request-plane (item 4) and capacity-plane (item 5) consumers need it flexible, and extracting now avoids the painful refactor once `server`/gateway depend on it (maintainer decision: T5(a), 0010 Open questions -> extract). (2) Deliver the deployable custodian process — wire `wyrd custodian` in the `server` crate (server depends on `custodian`/new `telemetry`, runs the leader-elected loop, installs the telemetry handle), not just the library `CustodianRole` seam; #366 is the sole owner of this half (2026-07-04 decision) and #367's day-one runbook needs it runnable. Keep the day-one signal (under-replicated rises->zero via `gather_prometheus`) green through the wired binary. </content> </invoke>
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_366: file an issue to close the gap between #366's library-seam keystone and a deployable `CustodianRole`/`wyrd custodian` process — no tracked issue currently owns the binary half (2026-07-04 decision folds it into #366), yet #367's step-4 runbook needs it runnable.
