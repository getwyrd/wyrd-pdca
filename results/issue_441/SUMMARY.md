# Result — issue 441 / fdb-packaging-and-version-coupling

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: Under `--features fdb`, Wyrd (a) **fails closed with a guided, actionable
- Success criterion: `cargo test -p wyrd-metadata-fdb --test preflight` passes on the
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: One logical change: make the FDB client's version coupling explicit — in code

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
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

Task under review: make the FoundationDB metadata backend fail closed with an actionable version-skew diagnosis, support a multi-version client directory, and document the packaging/upgrade contract.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The target behavior is explicit and traceable: the patch adds the default-build classifier at `crates/metadata-fdb/src/lib.rs:832`, wires production connect through it at `crates/metadata-fdb/src/lib.rs:1250`, and documents the FDB packaging contract at `docs/design/architecture/07-deployment-view.md:90`. |
| C2 Reproduction (red pre-fix) | PASS | Reversing the non-test changes while keeping the added test reproduced red: `cargo test -p wyrd-metadata-fdb --test preflight` failed on unresolved `wyrd_metadata_fdb::preflight` at `crates/metadata-fdb/tests/preflight.rs:25`. |
| C3 Change | PASS | The change reaches the operator path and the upgrade knob: `connect()` awaits `preflight()` before returning at `crates/metadata-fdb/src/lib.rs:1250`, and `ensure_network()` applies `WYRD_FDB_EXTERNAL_CLIENT_DIR` before boot at `crates/metadata-fdb/src/lib.rs:1114`. |
| C4 Verification (red→green) | PASS | Red→green was independently reproduced, `cargo test -p wyrd-metadata-fdb --test preflight` passed after restore, `cargo xtask ci` passed, and `cargo test -p wyrd-metadata-fdb --features fdb` passed the compile/unreachable-probe legs including `crates/metadata-fdb/tests/timeout.rs:96`. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether the runtime preflight guard is the accepted causal fix rather than a symptom guard: the production path now relies on `Database::get_client_status()` polling at `crates/metadata-fdb/src/lib.rs:1291`, but the live mismatched-cluster proof was not run here. |
| T1 Structure | PASS | The structure matches the brief: the binding test is its own integration binary at `crates/metadata-fdb/tests/preflight.rs:1`, while the editable living deployment document carries the packaging contract at `docs/design/architecture/07-deployment-view.md:4`. |
| T2 Shape | PASS | The classifier is non-feature-gated and pure at `crates/metadata-fdb/src/lib.rs:832`, while FDB-only JSON parsing remains behind the optional `fdb` graph via `serde_json` at `crates/metadata-fdb/Cargo.toml:22`. |
| T3 Runtime | NEEDS-HUMAN | Run the real Docker-backed FDB conformance/skew topology with socket access before sign-off: this sandbox had Docker CLI/compose installed but Docker API permission was denied, so `cargo xtask fdb-conformance` skipped the live matched-cluster path described at `xtask/src/main.rs:292`. |
| T4 Contribution | PASS | The dependency contribution is scoped: `serde_json` was already a workspace dependency at `Cargo.toml:126` and is added only under the optional FDB feature at `crates/metadata-fdb/Cargo.toml:33`. |
| T5 Judgment | NEEDS-HUMAN | Confirm closed/rejected PR prior art by affected path before sign-off: local merged history and `HEAD` grep found no existing `preflight`/`get_client_status`/`ExternalClientDirectory` handling, but `gh pr list --state closed` could not reach `api.github.com` from this sandbox. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Operator fitness still needs the live/manual skew decision: run the documented `foundationdb:7.1.61` repro and check for the guided message and cluster protocol named at `docs/design/architecture/07-deployment-view.md:127`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Decide whether the runtime preflight guard is the accepted causal fix rather than a symptom guard: the production path now relies on `Database::get_client_status()` polling at `crates/metadata-fdb/src/lib.rs:1291`, but the live mismatched-cluster proof was not run here.
- [x] T3 Runtime — Run the real Docker-backed FDB conformance/skew topology with socket access before sign-off: this sandbox had Docker CLI/compose installed but Docker API permission was denied, so `cargo xtask fdb-conformance` skipped the live matched-cluster path described at `xtask/src/main.rs:292`.
- [x] T5 Judgment — Confirm closed/rejected PR prior art by affected path before sign-off: local merged history and `HEAD` grep found no existing `preflight`/`get_client_status`/`ExternalClientDirectory` handling, but `gh pr list --state closed` could not reach `api.github.com` from this sandbox.
- [x] Validation — fitness-to-purpose — Operator fitness still needs the live/manual skew decision: run the documented `foundationdb:7.1.61` repro and check for the guided message and cluster protocol named at `docs/design/architecture/07-deployment-view.md:127`.

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
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- T3 recurs as NEEDS-HUMAN across FDB bundles: the reviewer sandbox has Docker CLI/compose but no Docker API access, so the live `cargo xtask fdb-conformance` skew/topology leg can never run at Check — worth an Act look at a Docker-capable reviewer env or a standard pre-declared deferred-confirmer path (here, #470).
- The codex reviewer leaf runs `codex exec --sandbox workspace-write` with no network grant, so it cannot reach `api.github.com` — forcing the closed/rejected-PR prior-art check to NEEDS-HUMAN on every bundle (T5/T4). Act: grant the reviewer sandbox scoped network access to github.com (or a read-only `gh` proxy) so prior-art can be cleared mechanically.
