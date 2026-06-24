# Check review — issue 205 / dserver-grpc-admission-control (iteration 2)

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json` (build-notes.md withheld by design). Citations
> re-derived against the target base tree at
> `/home/eddie/wyrd/wyrd` (granted working dir; `$PDCA_TARGET` was not
> readable from this sandbox — see *Grounding note* below) and against
> `patch.diff` for added lines.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:21-34` carries a binding, testable Success criterion (overload → retryable shed status, configurable limit, hung handler cut by timeout); the load-bearing field is concrete and falsifiable. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Pre-fix red is criterion-absence (no shed status exists because the base builder sets no limit — target `dserver.rs:160-162` bare `Server::builder().add_service(...).serve_with_incoming_shutdown(...)`); the C4-verify gate (`check-gates.json:42-49`, result `pass`) confirms the patch's test runs red pre-fix / green post-fix per `brief.md:69-77`. |
| C3 — C3 Change | PASS | Patch replaces the bare builder (target `dserver.rs:160`) with a server-wide `GlobalConcurrencyLimitLayer` + `LoadShedLayer` via `.layer()`, plus per-conn limit / `.timeout` / `max_concurrent_streams` / keepalive (`patch.diff` dserver.rs serve, lines 257-275), all behind a configurable `AdmissionControl` (`patch.diff` dserver.rs:157-193) wired through CLI (`patch.diff` cli.rs:58-70). Coherent single logical change on-target. |
| C4 — C4 Verification (red→green) | FAIL | **Gating** gate `C4-ci` (`./engine/xtask.sh ci`) FAILED — "madsim DST tests failed with exit status: 101" (`check-gates.json:33-40`, `gating:true`); `overall:"fail"`. The non-gating per-fix `C4-verify` passed (`check-gates.json:42-49`) but does not clear the gating CI. The DST suite runs the real wire code (`brief.md:16-19`); the new tower `.layer()`/timeout/keepalive stack is the prime suspect for the regression, but the binding fact is the gating failure regardless of cause. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Root cause was *contested* at iter 1 (per-connection-only limit did not restore the server-wide bound, `brief.md:92`). Iter 2 adopts the carry-forward's exact prescription — one shared `Arc<Semaphore>` via `GlobalConcurrencyLimitLayer` applied with `Server::layer` (`patch.diff` dserver.rs:257-260) — which is *plausibly* server-wide, but the bound holds only if tonic clones (not rebuilds) the layered stack per connection; I cannot confirm that from artifacts alone, and the gating DST failure means adequacy is unproven end-to-end. Oracle: reviewer + human sign-off. |
| T1 — T1 Structure | PASS | New tests land in the brief-prescribed file `crates/server/tests/dserver.rs` (`brief.md:66`), well-formed `#[tokio::test(multi_thread)]` with a `GateStore` admission-gate harness and a `serve_gated` helper (`patch.diff` tests/dserver.rs:311-471, 486-530). |
| T2 — T2 Shape | PASS | Assertions match the binding criterion: overload test drives a SECOND connection and asserts `ResourceExhausted \| Unavailable` (`patch.diff` tests/dserver.rs:460-464); timeout test asserts `Cancelled \| DeadlineExceeded` (`patch.diff` tests/dserver.rs:522-525). Shape pins shed-vs-served and deadline-cut. |
| T3 — T3 Runtime | PASS (qualified) | The patch's own tests execute green post-fix per `C4-verify` (`check-gates.json:42-49`). Qualifier: the broader runtime (madsim DST) fails (`check-gates.json:33-40`); that runtime breakage is scored under C4, not here — the patch's targeted tests themselves run. |
| T4 — T4 Contribution | PASS | `overload_across_connections_sheds_excess_with_a_retryable_status` (`patch.diff` tests/dserver.rs:411-471) is net-new coverage that exercises the SERVER-WIDE bound across separate connections — exactly the gap that let iter 1's per-connection-only fix pass falsely (`brief.md:92`). It would regress-catch a reversion to per-connection-only limiting. |
| T5 — T5 Judgment | NEEDS-HUMAN | Tests are genuinely adversarial (gate semaphore, bounded `timeout` waits, asserts error-not-value, two separate clients to force two connections). Caveat for the human: the carry-forward asked to *pin* the connection count so the shed path can't silently stop being exercised (`brief.md:92`); the patch forces two connections via two clients but does not assert each client opens exactly one connection. Oracle: reviewer + human sign-off. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Whether the change actually restores the §8.9 fail-closed posture in production (not just in the in-process test) is a human fitness judgment; it cannot be affirmed while the gating CI (`C4-ci`) is red. Oracle: human at sign-off. |

## §6 — Items the human must clear

1. **C4 (blocking).** The gating `cargo xtask ci` fails on the madsim DST suite
   (exit 101, `check-gates.json:33-40`). This alone blocks sign-off. The builder
   must make the DST suite green. Hypothesis worth checking first: the new
   `.layer(LoadShedLayer)` / `GlobalConcurrencyLimitLayer` / `.timeout` /
   `.http2_keepalive_interval` on the tonic `Server` (`patch.diff`
   dserver.rs:257-275) may be unsupported or non-deterministic under the
   madsim-tonic shim that the DST relies on (`brief.md:16-19`) — verify against
   the actual DST failure output before assuming the production code is otherwise
   correct.
2. **C5 — contested root cause.** Confirm the server-wide bound is real:
   verify that tonic 0.14 clones (does not rebuild) the `Server::layer` stack per
   connection so the single `Arc<Semaphore>` is shared across connections. The
   patch comment claims this (`patch.diff` dserver.rs:242-256) but it is the exact
   point that sank iter 1; it needs an authoritative source check (tonic
   `transport/server` MakeSvc), not the patch's self-assertion.
3. **C5 — shed→status mapping.** Confirm tower's `Overloaded` (from `LoadShedLayer`)
   maps to a *retryable* `RESOURCE_EXHAUSTED`/`UNAVAILABLE` at the tonic transport
   boundary (`patch.diff` dserver.rs:248-252 asserts "verified against tonic 0.14.6
   status.rs"). The C4-verify green is consistent with this but the claim should be
   pinned to tonic source.
4. **T5 — test robustness.** Decide whether the unpinned connection count
   (item T5 above) is acceptable, or require the test to assert exactly one
   connection per client so the shed path cannot silently stop being exercised.
5. **V — fitness-to-purpose.** Final human judgment that the §8.9 invariant is
   restored for real deployments (HDD/SSD-tunable, fails closed under
   many-connection overload), contingent on items 1–2 clearing.

## Grounding note

`$PDCA_TARGET` could not be read (this sandbox blocks env access). I grounded
target-source citations on `/home/eddie/wyrd/wyrd`, an explicitly granted
working directory whose `crates/server/src/dserver.rs` is the pre-fix base
(bare `Server::builder()` at lines 160-162) and whose unchanged context lines
match `patch.diff`'s base hunks exactly — so the patch applies to this tree and
the defect/repro citations are sound. I did not search any other checkout. If
the intended target differs, re-ground items C1–C3 accordingly; the C4 verdict
rests on `check-gates.json` and is independent of the source tree.
