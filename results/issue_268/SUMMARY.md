# Result — issue 268 / grpc-block-read-fault-distinct-wire-category

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: A block-layer read fault (`EIO` / dead sector) raised by a *remote* D
- Success criterion: Over a real gRPC client↔server channel, a `get_fragment` that
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Make a remote block-layer read fault travel the gRPC seam as its OWN distinct

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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

### Advisory — codex

- `crates/custodian/src/reconstruction.rs:327` still uses its private `is_block_read_fault`/`EIO` predicate even though this patch adds `wyrd_traits::is_block_read_fault` as the shared seam classifier at `crates/traits/src/lib.rs:206`. The current remote path works only because `BlockReadFault::source()` synthesizes an `EIO`, but the duplicate predicate leaves the local reconstruction consumer able to drift from the new single source of truth; simplify `is_permanent_read_fault` to call the trait classifier directly.
- NEEDS-HUMAN — `crates/traits/src/lib.rs:141` documents a new gRPC/seam contract category (`FAILED_PRECONDITION` -> `BlockReadFault`) distinct from the existing ADR-0010 integrity/transient split. This matches the brief direction, but the architecture/ADR acceptance still needs human sign-off.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — DECISION OWED: introducing a THIRD seam fault category extends the `IntegrityFault` seam contract (traits/src/lib.rs:64-84, ADR-0010; telemetry ADR-0011). Per brief §Plan note this is architecture-board authority — confirm (1) the seam-doc edit is the accepted contract wording, (2) the ADR amendment/companion is authored separately by the human (Do correctly did NOT author one), and (3) the errno-5-only closure (#251 §6 item 2) is the intended scope, not a wider dead-sector class.
- [x] Validation — fitness-to-purpose — DECISION OWED: the test asserts the trait-level `is_block_read_fault`/`is_integrity_fault`, which is a DIFFERENT function from the actual consumer `reconstruction::is_block_read_fault` (reconstruction.rs:338) and the `scrub.rs:102` branch; remote==local at the real consumers holds ONLY via the synthetic-EIO `source()` bridge, which I verified statically but the test does not exercise end-to-end at the consumer. Human confirms this design adequately demonstrates the production behavior (read-around + no corruption finding over the gRPC seam) and that prior-art/no-conflicting-open-work holds at sign-off.
- [x] `crates/traits/src/lib.rs:141` documents a new gRPC/seam contract category (`FAILED_PRECONDITION` -> `BlockReadFault`) distinct from the existing ADR-0010 integrity/transient split. This matches the brief direction, but the architecture/ADR acceptance still needs human sign-off.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-06-28

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_268: open a follow-up issue to author the ADR amendment/companion for the new BlockReadFault seam category (ADR-0010 / telemetry ADR-0011); seam-doc shipped this cycle, ADR authored separately by the human.
