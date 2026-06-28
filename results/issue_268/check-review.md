# Check review — issue 268 / grpc-block-read-fault-distinct-wire-category

> Advisory, artifact-only, decorrelated from the builder. Inputs: patch.diff,
> brief.md, check-gates.json (build-notes.md withheld). Citations grounded on the
> target source at `$PDCA_TARGET=/home/eddie/wyrd/wyrd.pdca-wt` (patch applied),
> read-only. NOTE on re-runs: the sandbox blocked `cargo` and `git` against the
> target, so I could not independently re-execute the red→green flow. C4 below
> therefore rests on the recorded deterministic gates (C4-ci gating=pass,
> C4-verify=pass) **plus** a full static re-derivation of the cross-seam path on
> the target source — not a fabricated re-run.

## Re-derived seam path (all grounded on target source, patch applied)

- Server `server.rs:94-96`: `is_integrity_fault` → `Code::DataLoss` is checked
  FIRST, then `is_block_read_fault` → `Code::FailedPrecondition`, else
  `Status::internal`. `FailedPrecondition` occurs nowhere else in the crate and
  `classify_get_status` is wired only to `get_fragment` (`client.rs:125`) — so the
  new code cannot misclassify another RPC path.
- Client `client.rs:40`: `Code::FailedPrecondition` → `BlockReadFault::new(id, msg)`.
- Seam `traits/src/lib.rs:389-429`: `BlockReadFault::source()` returns a synthetic
  `io::Error::from_raw_os_error(5)`.
- Consumer half (a) `reconstruction.rs:327-349`: `is_permanent_read_fault` →
  private `is_block_read_fault` walks `source()`, finds synthetic EIO, returns
  `true` → reconstruction reads around (permanent). Verified the consumer is the
  UNCHANGED local chain-walker; the synthetic source is what makes remote==local
  without touching it.
- Consumer half (b) `scrub.rs:102,108`: `is_integrity_fault(BlockReadFault)` is
  `false` (chain is `io::Error`, never `IntegrityFault`) → falls to `scrub.rs:108`
  `Err(e) => return Err(e)`, never `emit_corruption` — identical to a local EIO.
- v1 rejected approach (DataLoss=IntegrityFault carrier) is NOT reused; EIO gets
  its own wire code. Do shipped code + seam-doc + test only, authored no ADR file
  (diff touches `traits/src/lib.rs` only, no `docs/adr/*`) — matches the human's
  this-cycle decision (brief §Plan note).

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief states a two-part success criterion (permanent read-around AND not-corruption) + the cross-seam invariant; well-bounded and testable (brief.md:24-53). |
| C2 Reproduction (red pre-fix) | PASS | No own gate; red rests on C4-verify (pass) + my reading: pre-fix client had no `FailedPrecondition` arm, so EIO→`Status::internal`→`TransportError`, `is_block_read_fault`=false (classified transient). Could not re-run cargo to re-confirm red myself — caveat noted. |
| C3 Change | PASS | Diff is coherent and minimal across the three named layers; cites the brief's expected path:lines (server.rs:83-89, client.rs:25-33, traits seam type). |
| C4 Verification (red→green) | PASS | Deterministic gates recorded green: C4-ci (gating) pass, C4-verify red→green pass (check-gates.json:33-48). Sandbox blocked my own cargo re-run; I corroborated by full static path re-derivation on target source, not a re-execution — do not read this as an independently re-run green. |
| C5 Causal adequacy | PASS | Root cause = the wire seam flattening EIO to `internal`; fix gives EIO its own wire code + reconstructs a distinct seam type, so the category survives the round-trip — cause removed, not guarded. Symptom-guard smell-test does NOT fire (no capability probe / runtime guard around a present capability; `is_block_read_fault` is classification, not a probe). Verified across the seam on target (reconstruction.rs:327-349, scrub.rs:102-108). |
| T1 Structure | PASS | Code lands where it belongs: category def + classifier in the seam crate (traits), wire mapping in grpc server/client, test in chunkstore-grpc/tests — no consumer-crate edits needed (relies on the existing chain-walker). |
| T2 Shape | PASS | `BlockReadFault` mirrors `IntegrityFault`'s shape and `is_block_read_fault` mirrors `is_integrity_fault`'s chain walk; client `match` arm is idiomatic; single errno-5 closure constant avoids per-site re-derivation (traits/src/lib.rs:365). |
| T3 Runtime | PASS | No runtime/perf concern: synthetic `io::Error` construction is cheap and only on the error path; test drives a real in-process tonic loopback (no Docker), aborts the server per test. |
| T4 Contribution | PASS | One logical change (remote block-read-fault as its own wire category) + its regression test; no scope creep into local classification / IntegrityFault semantics / reconstruction accounting. Prior-art check documented by affected file path (brief.md:98-105); see V row for sign-off confirmation. |
| T5 Judgment | NEEDS-HUMAN | DECISION OWED: introducing a THIRD seam fault category extends the `IntegrityFault` seam contract (traits/src/lib.rs:64-84, ADR-0010; telemetry ADR-0011). Per brief §Plan note this is architecture-board authority — confirm (1) the seam-doc edit is the accepted contract wording, (2) the ADR amendment/companion is authored separately by the human (Do correctly did NOT author one), and (3) the errno-5-only closure (#251 §6 item 2) is the intended scope, not a wider dead-sector class. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | DECISION OWED: the test asserts the trait-level `is_block_read_fault`/`is_integrity_fault`, which is a DIFFERENT function from the actual consumer `reconstruction::is_block_read_fault` (reconstruction.rs:338) and the `scrub.rs:102` branch; remote==local at the real consumers holds ONLY via the synthetic-EIO `source()` bridge, which I verified statically but the test does not exercise end-to-end at the consumer. Human confirms this design adequately demonstrates the production behavior (read-around + no corruption finding over the gRPC seam) and that prior-art/no-conflicting-open-work holds at sign-off. |

## Notes / non-blocking observations

- Coverage gap (advisory, folded into V): the regression asserts the seam
  classifiers, not the custodian consumers (`reconstruction::is_permanent_read_fault`,
  the `scrub.rs:102` branch). The remote==local guarantee is carried entirely by
  `BlockReadFault::source()` returning synthetic EIO. I verified the consumer code
  on the target makes this hold, but a consumer-level test would pin it directly.
- No blocking defects found. The two NEEDS-HUMAN rows are by-design sign-off items
  (seam-contract/ADR authority and fitness-to-purpose), not patch faults.
