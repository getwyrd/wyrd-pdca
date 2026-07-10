# Result — issue 440 / server-fdb-backend-selection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `MetadataBackend` (`crates/server/src/cli.rs:88`) offers only `Redb` and
- Success criterion: In the **default build** (no `fdb` feature, no `libfdb_c`, no
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Extend metadata-backend selection with an `Fdb` variant, compiled only under a

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

Reviewing issue 440: add server-side selection for the FoundationDB metadata backend while keeping default builds FDB-free.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: default builds must reject `fdb` with a feature hint, list `fdb` for unknown values and usage, and keep `cargo xtask ci` green (`brief.md:12`). |
| C2 Reproduction (red pre-fix) | PASS | A clean `HEAD` sandbox plus only `crates/server/tests/fdb_backend_selection.rs` failed 0/3 on the old messages and usage text, matching the message-content RED required by `brief.md:33`. |
| C3 Change | PASS | The patch makes `fdb` selectable only at the server composition root: enum/config arms, sync `open_fdb_meta`, usage text, dispatch arms, optional manifest dependency, lockfile, and regression test (`crates/server/src/cli.rs:97`, `crates/server/Cargo.toml:31`, `Cargo.lock:5183`). |
| C4 Verification (red→green) | PASS | Re-ran `cargo test -p wyrd-server --test fdb_backend_selection`, `cargo xtask ci`, `cargo check -p wyrd-server --features fdb --tests`, `cargo test -p wyrd-server --features fdb --test fdb_backend_selection`, and `cargo check -p wyrd-server --features fdb,etcd --tests`; all passed with the patch (`crates/server/tests/fdb_backend_selection.rs:41`, `crates/server/src/cli.rs:1485`). |
| C5 Causal adequacy | PASS | The fix removes the selection gap rather than guarding a load-time side effect: `from_config("fdb")` is feature-gated and production arms reach `FdbMetadataStore::connect()` without a new capability probe (`crates/server/src/cli.rs:119`, `crates/server/src/cli.rs:168`). |
| T1 Structure | PASS | The change preserves the documented composition-root boundary for metadata backend selection and does not refactor consumers (`crates/server/src/cli.rs:82`). |
| T2 Shape | PASS | The regression is a new added test file with cfg-paired assertions, and the optional dependency is recorded in both manifest and lockfile as required (`crates/server/tests/fdb_backend_selection.rs:30`, `crates/server/Cargo.toml:52`, `Cargo.lock:5183`). |
| T3 Runtime | NEEDS-HUMAN | Live FDB put/get remains owed because Docker daemon access failed with socket permission denied and `cargo xtask fdb-conformance` skipped locally, so only compile/pure-selection runtime evidence was exercised; this matters because `open_fdb_meta` has not been driven against a real cluster here (`brief.md:154`, `crates/server/src/cli.rs:168`). |
| T4 Contribution | NEEDS-HUMAN | Merged-history prior art by affected file path shows no prior server-side FDB selection, but `gh pr list` could not reach `api.github.com`, so open/closed/rejected work must be cleared by a human (`brief.md:199`). |
| T5 Judgment | PASS | Advisory judgment is that the patch satisfies the declared Check boundary, with human follow-up isolated to live-cluster evidence and remote prior-art reachability; ADR-0042 explicitly separates selection from deployment (`docs/design/adr/0042-production-metadata-backend-reevaluation.md:461`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether compiled-and-selected FDB support, without a locally reproduced live Docker round trip, is sufficient for the operator-facing goal before sign-off (`brief.md:169`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T3 Runtime — Live FDB put/get remains owed because Docker daemon access failed with socket permission denied and `cargo xtask fdb-conformance` skipped locally, so only compile/pure-selection runtime evidence was exercised; this matters because `open_fdb_meta` has not been driven against a real cluster here (`brief.md:154`, `crates/server/src/cli.rs:168`).
- [x] T4 Contribution — Merged-history prior art by affected file path shows no prior server-side FDB selection, but `gh pr list` could not reach `api.github.com`, so open/closed/rejected work must be cleared by a human (`brief.md:199`).
- [x] Validation — fitness-to-purpose — Human must decide whether compiled-and-selected FDB support, without a locally reproduced live Docker round trip, is sufficient for the operator-facing goal before sign-off (`brief.md:169`).

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
- issue_440: stale doc comment — `crates/metadata-fdb/src/lib.rs:895-896` still states "no `server`-side selection arm exists yet (that is a later, blocked issue)"; this bundle falsifies it. `metadata-fdb` was out of scope here, so it needs a follow-up.
