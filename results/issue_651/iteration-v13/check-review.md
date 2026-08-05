Reviewing issue #651: contain and attribute unreadable chunk maps in restore/drain reporting while preserving fail-closed marking and an actionable non-zero operator verdict.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the observable safety outcomes, ordering, scope boundary, and base-visible discriminator, so the required implementation and operator decisions are unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | All five discriminator tests execute and fail against the base behavior, then pass with the patch; the cases begin at `crates/custodian/tests/segmented_map_restore.rs:461`. |
| C3 Change | PASS | The patch stays on the allocated restore, drain-status, CLI, test, and living-doc surfaces, and does not reintroduce the expressly dropped cross-object ambiguity apparatus. |
| C4 Verification (red→green) | PASS | Independent replay produced 5/5 red then 5/5 green, a clean `cargo xtask ci`, and 38/38 non-surviving mutants for the changed code; the discriminator's one-reading assertion is at `crates/custodian/tests/segmented_map_restore.rs:678`. |
| C5 Causal adequacy | PASS | Per-object faults are contained and both namespace readings are reconciled before any mark is authorized, addressing the whole-pass error and split-reading cause without a capability probe or symptom guard (`crates/custodian/src/restore.rs:277`, `crates/custodian/src/restore.rs:293`, `crates/custodian/src/restore.rs:358`). |
| T1 Structure | PASS | The change uses exactly the eight-file allocation and the added integration-test crate root carries the required unsafe prohibition (`crates/custodian/tests/segmented_map_restore.rs:48`); the iteration-12 size backstop was explicitly settled by the human. |
| T2 Shape | FAIL | The operator-facing dangling and misplaced paragraphs collapse existing chunk-ID vectors to counts and require an audit-log lookup, so their output shape omits the identifiers needed for repair (`crates/server/src/cli.rs:1282`, `crates/server/src/cli.rs:1290`). |
| T3 Runtime | FAIL | A terminal-only restore operator cannot identify which lost or misplaced chunks require action when the log collector is unavailable, despite the same actionability rule being enforced for unreadable records (`crates/server/src/cli.rs:1284`, `crates/server/src/cli.rs:1292`). |
| T4 Contribution | FAIL | Contribution artifacts and affected-path prior art are complete, but the red rubric result's two remaining blockers are independently grounded in the count-only dangling and misplaced output (`crates/server/src/cli.rs:1282`, `crates/server/src/cli.rs:1290`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must render each dangling/misplaced chunk ID inline and assert both identifiers; the current test creates IDs 1 and 2 but checks only generic status/text coupling, so the omission can recur (`crates/server/src/cli.rs:2720`, `crates/server/src/cli.rs:2755`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the rebuilt terminal report plus non-zero status is actionable enough to operate restore/decommission safely without depending on an audit collector — this is the production fitness boundary. |

Re-run notes:

- Target state was readable at `d50f0ca`, and reverse-apply checking confirmed that `patch.diff` exactly matches the applied target state.
- The base replay ran all five added tests and all five failed on behavior (not missing symbols); the patched replay ran the same five and all passed.
- `cargo xtask ci` completed with all checks passed after relocating Cargo's advisory-database lock to the named writable scratch directory; `typos`, docs lint/render, `cargo-machete`, and `cargo deny check` were all genuinely exercised. The first read-only Cargo-home lock error was a host caveat, not a patch defect.
- Mutation replay selected the asserted 38 diff mutants and completed with 24 caught and 14 unviable, with no survivor.
- Affected-path prior art was checked across merged `origin/main` history, all closed/unmerged pull requests, and all open pull requests: PR #647 is the sole closed/unmerged predecessor touching these files, and no open pull request overlaps them.
- The external `scripts/review-branch --bundle` orchestrator was not present in the bundle or target and could not be rerun; its two underlying source defects were nevertheless independently confirmed above rather than copied from the asserted gate result.
