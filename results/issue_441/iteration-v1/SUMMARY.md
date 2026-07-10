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

Task under review: issue #441 makes FoundationDB version coupling explicit by adding a fail-closed preflight, multi-version client directory support, and packaging documentation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is clear: a guided protocol-skew error, external-client-directory support, and packaging contract are the scope, while the live wrong-version image check is deferred to #470 (`brief.md:9`, `brief.md:49`). |
| C2 Reproduction (red pre-fix) | PASS | Keeping the added integration test while reverse-applying the implementation made `cargo test -p wyrd-metadata-fdb --test preflight` fail with unresolved `wyrd_metadata_fdb::preflight`, exactly the absent-API red at `crates/metadata-fdb/tests/preflight.rs:18`. |
| C3 Change | PASS | The patch adds the requested pure preflight API, FDB external-client-directory configuration, connect-time probe, and deployment contract in the affected surfaces (`crates/metadata-fdb/src/lib.rs:835`, `crates/metadata-fdb/src/lib.rs:1118`, `crates/metadata-fdb/src/lib.rs:1243`, `docs/design/architecture/07-deployment-view.md:90`). |
| C4 Verification (red->green) | FAIL | The classifier red->green and `cargo xtask ci` passed, but a direct `--features fdb` CLI smoke panics before any guided error because `preflight()` blocks a nested Tokio runtime (`crates/server/src/cli.rs:360`, `crates/server/src/cli.rs:374`, `crates/metadata-fdb/src/lib.rs:1290`). |
| C5 Causal adequacy | FAIL | The human must decide a non-panicking production seam before sign-off: every `wyrd ... --metadata-backend fdb` path is meant to traverse `connect()`, but the new synchronous probe cannot run from the existing async CLI runtime (`crates/server/src/cli.rs:168`, `crates/metadata-fdb/src/lib.rs:1277`). |
| T1 Structure | PASS | The work stays within the metadata-fdb crate plus the living deployment document, matching the one logical FDB packaging/version-coupling change (`crates/metadata-fdb/Cargo.toml:22`, `docs/design/architecture/07-deployment-view.md:90`). |
| T2 Shape | PASS | The default-build test binds a public non-feature-gated pure module, while JSON/FDB dependencies remain under the optional `fdb` feature (`crates/metadata-fdb/tests/preflight.rs:18`, `crates/metadata-fdb/Cargo.toml:22`, `crates/metadata-fdb/src/lib.rs:835`). |
| T3 Runtime | FAIL | The relevant runtime path was exercised: `WYRD_FDB_CLUSTER_FILE=/tmp/pdca-review-fdb.cluster cargo run -p wyrd-server --features fdb -- put ... --metadata-backend fdb` exited 101 with Tokio's nested-runtime panic at `crates/metadata-fdb/src/lib.rs:1290`. |
| T4 Contribution | PASS | The operator-facing contribution is present in both code and docs: skew messages name protocol/multi-version remediation and the deployment page records container, bare-metal, upgrade, repro, and single-binary tradeoffs (`crates/metadata-fdb/src/lib.rs:931`, `docs/design/architecture/07-deployment-view.md:94`). |
| T5 Judgment | NEEDS-HUMAN | Closed/rejected-work prior art could not be mechanically settled because `gh pr list --state closed` could not connect to GitHub; local merged history and `git grep` found no prior `get_client_status`/`ExternalClientDirectory` handling on affected paths (`brief.md:135`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Fitness sign-off must account for the undischarged external legs: Docker was unavailable so `cargo xtask fdb-conformance` skipped the feature-gated FDB job, and the documented wrong-version cluster/image validation remains delegated to #470 (`brief.md:83`, `docs/design/architecture/07-deployment-view.md:120`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Closed/rejected-work prior art could not be mechanically settled because `gh pr list --state closed` could not connect to GitHub; local merged history and `git grep` found no prior `get_client_status`/`ExternalClientDirectory` handling on affected paths (`brief.md:135`).
- [ ] Validation — fitness-to-purpose — Fitness sign-off must account for the undischarged external legs: Docker was unavailable so `cargo xtask fdb-conformance` skipped the feature-gated FDB job, and the documented wrong-version cluster/image validation remains delegated to #470 (`brief.md:83`, `docs/design/architecture/07-deployment-view.md:120`).

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
- Iteration delta (if iterating): The plan and the brief stand; the implementation missed the async seam. Rebuild against the same brief — do NOT re-plan. WHAT IS WRONG `FdbMetadataStore::preflight()` builds its own current-thread Tokio runtime and calls `Runtime::block_on`. Tokio panics ("cannot start a runtime from within a runtime") when that happens inside an existing runtime context. All seven call sites of the production entry point `open_fdb_meta()` are already inside one: cli.rs:374, :460, :1552, :1600 (directly inside `runtime.block_on(async {...})`) and cli.rs:841, :1481, :1487 (inside an `async fn`). So every `wyrd ... --metadata-backend fdb` invocation panics with exit 101 — confirmed by the reviewer's live smoke run and re-verified at sign-off by enumerating the call sites. WHY THE GATES MISSED IT The pure `preflight` module has no runtime, so its unit tests pass; and `cargo xtask ci` never enables the `fdb` feature, so no gate ever compiled the panicking code. The green C4-ci is not evidence of a working production path. The brief's own "Production reach" section (brief.md:115) is what makes this total rather than marginal: it is correct that all production traffic traverses `connect()`. WHAT TO CHANGE Make the probe async rather than spawning a nested runtime. Mirror the peer that already does this correctly: `open_tikv_meta()` is `async` and is `.await`ed by its callers. Concretely — make `connect()` (and `preflight()`) `async`, make `open_fdb_meta()` `async`, and `.await` it at all seven call sites. Update the now-stale doc comment at cli.rs:165 which asserts "`connect()` is synchronous (unlike `open_tikv_meta`'s `.await`)" — that sentence is the trap the build fell into. Keep the polling loop's semantics (re-poll until settled or deadline); only the runtime ownership changes. `open()` must stay probe-free, as the brief requires, so tests/timeout.rs, scan.rs, contention.rs, conformance.rs are unaffected. Add coverage that would have caught this: the existing unit tests exercise only the pure classifier and cannot. A test that drives `connect()` from within a Tokio runtime is what closes the hole. RESOLVED AT SIGN-OFF — do not re-litigate Brief Open question 1 (superseding ADR for the single-binary / shared-library trade): NO ADR IS NEEDED. The human confirms the brief's position — ADR-0014 already scopes the single-binary profile to development and evaluation only, and `fdb` is a production tier, so the living deployment doc (docs/design/architecture/07-deployment-view.md) suffices. Do MUST NOT touch ADR-0014 or ADR-0042. CARRIED FORWARD UNCHANGED The deferred live wrong-version-cluster validation stays #470's, as the brief pre-declared (brief.md:49). Not a defect of this bundle.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
