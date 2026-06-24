# Check review — issue 205 / dserver-grpc-admission-control (iteration 3)

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json` (build-notes.md withheld by design).

## §0 Grounding

- **Target source** read read-only at `/home/eddie/wyrd/wyrd` (the granted working
  checkout for this review). It is **clean `main`, pre-patch**: `dserver.rs` is 253
  lines with the bare `Server::builder().add_service(...)` at
  `crates/server/src/dserver.rs:160` — matching the defect in `brief.md:9-11`. The
  patch is therefore reviewed *against* the pre-fix target, not a patched tree.
- Every `path:line` below was re-derived from that target source or from `patch.diff`
  directly; I did not rely on build-notes.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Success criterion is concrete and binding: overload → retryable shed status, hung handler → deadline cut, limit operator-configurable (`brief.md:21-34`); BINDING vs ILLUSTRATIVE split is explicit (`brief.md:29-34`). |
| C2 — C2 Reproduction (red pre-fix) | PASS | Red is criterion-absence by construction: target `dserver.rs:160` sets no limit/timeout, so conn-B queues behind A's held slot and the 5 s bound elapses → `excess.expect(...)` panics (`patch.diff:538-544`); `C4-verify` gate ran the per-fix red→green and passed (`check-gates.json:42-49`). No standalone C2 gate configured (`check-gates.json:15-22`). |
| C3 — C3 Change | PASS | Server-wide bound restored: `LoadShedLayer` + `GlobalConcurrencyLimitLayer` (one shared `Arc<Semaphore>`) applied via `Server::layer` at `patch.diff:344-347`, plus per-conn cap/timeout/h2/tcp knobs (`patch.diff:352-362`); config surfaced via `AdmissionControl` + `with_admission_control` (`patch.diff:244-318`) and CLI flags (`patch.diff:145-157`). Note: bundle also carries an unrelated DST de-flake (`crates/dst/tests/custodian.rs`, `patch.diff:31-117`) — second logical change; see §6. |
| C4 — C4 Verification (red→green) | PASS | Gating `C4-ci` (`cargo xtask ci`: fmt/clippy/build/test/deny/conformance) = pass (`check-gates.json:33-39`); non-gating `C4-verify` per-fix red→green = pass (`check-gates.json:42-49`); `overall: pass` (`check-gates.json:3`). |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Re-derived as sound: `GlobalConcurrencyLimitLayer::new(n)` holds a single `Arc<Semaphore>` constructed once, so per-connection cloning of the layer stack still shares one server-wide bound — this fixes iteration-1's per-connection scope defect (`patch.diff:329-347`). BUT (a) C5 is the twice-contested root cause whose oracle is "reviewer + human sign-off" (`check-gates.json:96-99`), and (b) the bundle folds a *second*, independent causal claim — the DST flake is the `tracing` global per-callsite interest cache poisoned to `never` (`patch.diff:39-67`) — which is plausible and matches the scoped `with_subscriber` design (`custodian.rs:319-325,952,975`) but is a distinct root-cause story a human must confirm is the true cause, not a masking workaround. |
| T1 — T1 Structure | PASS | Tests live in the brief-named file `crates/server/tests/dserver.rs` (`brief.md:66`); a gating `GateStore` + `serve_gated`/`status_code` helpers structure the two cases cleanly (`patch.diff:399-479`). |
| T2 — T2 Shape | PASS | Assertions match the binding criterion: excess shed with `ResourceExhausted \| Unavailable` (`patch.diff:547-551`); hung handler cut with `Cancelled \| DeadlineExceeded` (`patch.diff:609-612`). |
| T3 — T3 Runtime | PASS | Both are real-runtime integration tests over the actual tonic transport (`#[tokio::test(multi_thread, worker_threads=4)]`, real `GrpcChunkStore::connect`, `patch.diff:498,523-526,572,592`); executed green under `C4-ci`/`C4-verify`. |
| T4 — T4 Contribution | PASS | Net-new, non-tautological coverage that directly answers iteration-1's T5 critique: the shed test drives **two separate connections** with `max_concurrent_requests:1`, so a per-connection-only limit would admit B — only the shared bound sheds it (`patch.diff:481-558`). Deterministic ordering via `entered_rx.recv()` before issuing B (`patch.diff:531-538`). |
| T5 — T5 Judgment | PASS | Guards against false-green/flake: closed `Semaphore(0)` gate pins A's slot; bounded 5 s wait turns a pre-fix hang into a failure not a hang; long 60 s timeout in the shed test isolates the shed path from the timeout path, and a wide limit in the timeout test isolates the timeout from the shed path (`patch.diff:509-518,584-589`). `entered` signal proves the timeout cut a genuinely in-flight handler (`patch.diff:597-601`). |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Whether the restored behaviour genuinely satisfies architecture §8.9 "fail closed under pressure / shed or slow load predictably, never trade correctness for admission" (`docs/.../08-crosscutting-concepts.md:98-107`, verified on target) and the chosen defaults (e.g. server-wide 64, `patch.diff:194`) fit real HDD/SSD queue depths is an operational judgment, not a gate. |

## §6 Items the human must clear

1. **C5 — contested root cause (admission control).** Confirm the server-wide bound
   is truly server-wide: `GlobalConcurrencyLimitLayer` shares one `Arc<Semaphore>`
   across all per-connection clones of the layer stack (`patch.diff:329-347`). This is
   the exact point iteration 1 was rejected on; the re-derivation says it is now fixed,
   but it is the always-human contested item. The cross-connection shed test
   (`patch.diff:481-558`) is the supporting evidence.

2. **C5 / scope — a second logical change is bundled.** The patch also de-flakes
   `durability_emission_rises_then_returns_to_zero` by installing a process-global
   permissive `tracing` default (`patch.diff:39-117`). The stated root cause (global
   per-callsite interest cached `never` when the first thread hits a callsite under
   `NoSubscriber`, short-circuiting later scoped `with_subscriber` captures) is
   credible and consistent with the scoped-capture design (`custodian.rs:319-325,950-995`).
   A human must confirm (a) this is the real cause vs. a workaround that masks a
   different DST nondeterminism (ADR-0009 requires *deterministic* DST), and (b) that
   bundling this DST fix with the admission-control change is acceptable here — it *was*
   explicitly directed by the iteration-2 carry-forward (`brief.md:97`), but it violates
   the one-logical-change-per-bundle norm and should be acknowledged, not silently
   accepted.

3. **V — validation / fitness-to-purpose.** Sign off that the fail-closed behaviour and
   the operator-tunable defaults meet architecture §8.9 intent for real devices.

## Notes (advisory, non-gating)

- The `C4-ci` gate green is a green-mechanical signal that the suite passes *once*; it
  does not by itself prove the de-flaked DST test is now deterministic across seeds —
  the determinism claim rests on the interest-cache root cause above (item 2), which is
  why C5 is NEEDS-HUMAN rather than PASS.
- Error-code plumbing checks out against the target: `TransportError` maps
  `ResourceExhausted`→`Rpc(status)` (preserving the code) and `DeadlineExceeded`→
  `Timeout(status)` (`crates/chunkstore-grpc/src/error.rs:33-35`), so `status_code`
  (`patch.diff:469-479`) reads the wire code the assertions expect.
