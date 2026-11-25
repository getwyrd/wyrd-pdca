Review of #1029: settle an unknown multipart part commit from reads without another batch; the main behavior passes red→green, but one confirmed implementation defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted contract distinguishes proof of this attempt's commit from mere part presence, while forbidding further batches after an unknown outcome; `brief.md:254`, `crates/core/src/multipart/commit.rs:116`. |
| C2 Reproduction (red pre-fix) | PASS | An independently compiled probe finds the part durable but receives `InDoubt` on the stashed base, establishing the missing success classification; `reviewer-red.log:10`, `crates/core/tests/multipart_upload_part_commit.rs:4305`. |
| C3 Change | PASS | The three changed paths stay within the commit, its existing test suite and living architecture; the behavior and availability cost are documented together; `crates/core/src/multipart/commit.rs:1357`, `docs/design/architecture/05-building-block-view.md:250`. |
| C4 Verification (red→green) | PASS | The same probe passes after exact patch restoration, and all 640 core tests pass independently; the uncovered fault combination is adjudicated under C5, not hidden by these greens; `reviewer-redgreen-restored.log:9`, `reviewer-core.log:1`, `gate-logs/C4-ci.log:4812`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve irreversible evidence across a later read fault and cover that combination — otherwise callers are told a batch can still land after successful reads proved it cannot; R1, `crates/core/src/multipart/commit.rs:905`, `reviewer-probes-patched.log:26`. |
| T1 Structure | PASS | Settlement stays behind the existing metadata trait and shares the classification reads without adding a backend or runtime dependency; `crates/core/src/multipart/commit.rs:848`, `crates/core/src/multipart/commit.rs:894`. |
| T2 Shape | PASS | The bounded settlement helper and explicit error cause fit the existing API, with living documentation updated and independent formatting/spelling checks passing; `crates/core/src/multipart/commit.rs:393`, `docs/design/architecture/05-building-block-view.md:250`. |
| T3 Runtime | PASS | Unknown outcomes exit the write loop into bounded reads, preserving the no-retry/no-compensation safety rule and the existing clock lifecycle; `crates/core/src/multipart/commit.rs:316`, `crates/core/src/multipart/commit.rs:955`, `crates/core/src/multipart/commit.rs:1358`. |
| T4 Contribution | FAIL | One substantive review finding still needs resolution: all three frozen findings describe R1, independently reproduced here; affected-path prior-art checks are clear, and contribution-artifact validation is deferred to publish; `gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.log:1`. |
| T5 Judgment | PASS | The already accepted availability tradeoff does not need reopening; no capability probe masks an eager cause, and the tracked DST/compensation deferrals remain settled; `brief.md:249`, `crates/core/src/multipart/commit.rs:265`, `crates/core/src/multipart/commit.rs:1151`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected library-only slice is fit for its intended integration — store-level tests establish protocol behavior, while the production S3 caller remains #669; `brief.md:175`, `crates/core/src/multipart/commit.rs:260`. |

R1 — **Preserve successful observations when a subsequent read fails (P2).** `read_lap` returns only an error if any of its four reads fails (`crates/core/src/multipart/commit.rs:848`). Consequently, `settle` returns at line 907 before applying the part observation at line 915 or the first session observation at line 922. This violates the public flag's promise that no read has shown the batch unable to land (`crates/core/src/multipart/commit.rs:398`). It overstates possible future writes; the reproduced cases do not resend, compensate or corrupt a part.

The scratch probe exercised both histories against the real redb store and the submitted wrapper:

- Hold the commit, advance the session to a higher epoch, successfully read that session, then fail the slot read.
- Hold a commit requiring an absent part, commit a same-number rival, successfully read that part, then fail the first session read.

Both return `may_still_commit: true`; releasing each held batch returns `Conflict`. Both retain one slot and send exactly one commit and zero compensations (`reviewer-probes-patched.log:26`, `reviewer-probes-patched.log:28`). The existing fault test deliberately keeps the held batch landable and therefore does not cover either combination (`crates/core/tests/multipart_upload_part_commit.rs:5147`). Preserve the validated observations before later awaits, or carry partial observations with the read error, while retaining `InDoubt` and the read fault as its cause. Add regression coverage for both histories. This consolidates the three frozen batch-review findings into one actionable issue.

Reproduce R1 from this review directory:

```sh
TMPDIR="$PWD/pdca-reviewer-1029-tmp" CARGO_TARGET_DIR="$PWD/pdca-reviewer-1029-build" \
  cargo test --offline --locked --manifest-path pdca-reviewer-1029-probes/Cargo.toml \
  --test reviewer reviewer_irreversible_evidence_survives_later_read_fault -- --exact --nocapture
```

Verification evidence supports the main feature, with the following limits:

- **Independent execution:** `cargo test -p wyrd-core` passed 640 tests, including all 75 multipart commit tests. A separate probe compiled against both versions: stashing the whole patch produced an assertion failure on the missing `Committed` answer, and restoring it produced success for both `may_still_commit` values. The target's diff was byte-identical before and after restoration. `cargo fmt --all -- --check`, `typos`, docs lint, docs render/link audit, and `git diff --check` passed; rendering produced 101 pages (`reviewer-docs-render.log:2`). The brief's `typos` and `docs-renderer` dependencies were exercised.
- **Frozen host evidence:** instance-scoped wrapper gates were adjudicated from their supplied logs, not rerun or inferred from their summary status. C4-ci ends with all checks passed (`gate-logs/C4-ci.log:4812`); host-tikv records both feature compilations, ending successfully (`gate-logs/host-tikv.log:211`). C4-verify itself was explicitly green-only (`gate-logs/C4-verify.log:161`); the independent probe supplies a genuine red→green for the main feature, not every mutation claimed in the brief.
- **Coverage and mutations:** C4-diff-cov measured no coverage because its origin/main base lacks the prerequisite patch (`gate-logs/C4-diff-cov.log:10`). The supplied target contains the correct stacked base and compiles; the coverage failure is an ordering caveat, not a patch defect. C5-mutants reports 16 caught, 12 unviable and three timeouts, with no reported surviving mutant (`gate-logs/C5-mutants.log:11`). The timed-out mutations alter the settlement loop's progress/bound; that log does not establish an unmutated runtime failure or complete mutation verification.
- **Prior art and deferrals:** independently queried main-branch history for all three affected paths and inspected file lists for all 19 closed, unmerged PRs; none touches those paths (`reviewer-prior-art.log:1`). The core implementation and test history both identify merged #1027. Seeded DST remains explicitly deferred to #660, and compensation settlement to #1030; neither is re-raised. **T4-contribution: N/A** — its artifacts are intentionally absent at Check and the substantive audit runs at publish (`gate-logs/T4-contribution.log:10`).

Source citations above are grounded in the supplied `$PDCA_TARGET`; review and gate evidence paths are relative to this review directory. No product source was edited, and all reviewer-created probes, logs and build artifacts remain inside the harness-owned directory.
<!-- pdca:leaf-complete -->
