Review of #656’s multipart Create admission and Abort fencing: one reproduced Abort commit-uncertainty defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Admission, fence atomicity, counter ownership, and original-ID recovery have concrete acceptance criteria consistent with proposal 0016; `brief.md:23`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:977`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared net-new verification posture — the stashed base cannot compile the test because the API is absent; three independently executed behavioral mutant reds support discrimination but do not constitute a pre-fix behavioral red; `brief.md:133`, `reviewer-red.log:3`. |
| C3 Change | PASS | The change stays within the approved core API, shared restore batch, tests, and living-doc scope; existing restore behavior passes 30 tests, and no production Create/Abort caller was found; `crates/custodian/src/restore.rs:1052`, `docs/design/architecture/05-building-block-view.md:202`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the sign-off disposition of the inherited all-features dependency gate outside this slice — local tests pass, but full CI remains red on unchanged rustls 0.21.12 advisories; `gate-logs/C4-ci.log:6525`, `Cargo.lock:3294`, `brief.md:356`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve earlier in-flight uncertainty across Abort retries and add a mixed-outcome regression — a later settled error currently conceals a fence that can still land after return (R1); `crates/core/src/multipart/store.rs:751`, `reviewer-abort-probe.log:20`. |
| T1 Structure | PASS | Runtime services remain caller-owned, metadata access stays on the existing trait seam, and both fence writers share one atomic batch builder; `crates/core/src/multipart/store.rs:87`, `crates/core/src/multipart/store.rs:775`, `crates/custodian/src/restore.rs:1052`. |
| T2 Shape | PASS | Formatting, spelling, documentation rendering, and statics checks pass; creation reserves encoded size for later states without weakening canonical decoding; `reviewer-scanners.log:1`, `reviewer-statics.log:3`, `crates/core/src/multipart.rs:2368`. |
| T3 Runtime | FAIL | R1 violates the metadata outcome contract: Abort reports that one reread settles the outcome while an earlier fence can still commit; the independent real-redb probe demonstrates the late state change; `crates/traits/src/lib.rs:240`, `crates/core/src/multipart/store.rs:731`, `reviewer-abort-probe.log:20`. |
| T4 Contribution | FAIL | The batch review’s two entries are one unresolved reproduced defect, requiring one disposition; affected-path prior art was checked, and contribution-artifact auditing is N/A until its mandatory publish rerun; `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`, `reviewer-rejected-paths.json:1`. |
| T5 Judgment | PASS | No additional scope or root-cause dispute was found: the prior size and identity cases pass, the #511 deferral is retained, and caller-owned timeouts remain the recorded decision; `crates/core/src/multipart/store.rs:245`, `crates/core/src/multipart/store.rs:358`, `crates/core/src/multipart/store.rs:564`, `brief.md:342`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this isolated API slice meets the intended milestone — core behavior is exercised, but production verbs and reclamation remain the deliberately separate #668/#659/#625 work; `brief.md:146`, `crates/core/src/multipart/store.rs:27`. |

R1: Abort must retain uncertainty from every fence that can still land. At `crates/core/src/multipart/store.rs:751`, each new unknown error replaces the previous one; exhaustion returns that last error at line 731. A later `may_still_commit: false` describes only its own transaction. It cannot settle an earlier transaction with `may_still_commit: true`, as the seam explicitly states at `crates/traits/src/lib.rs:240`.

The independent probe held the first Abort batch in flight, returned three subsequent unapplied unknown outcomes with `may_still_commit: false`, and left every recovery read at `Open`. Abort returned the last false-valued error. Applying the first held batch **after Abort returned** then committed successfully: the session became `Aborting`, its retirement obligation appeared, and the admission counter stayed unchanged (`reviewer-abort-probe.log:20`). This can lead a caller to treat a negative reread as final even though the upload is subsequently fenced. Preserve outstanding ambiguity until confirmed success or session state invalidates the earlier batch; Create already retains it at `crates/core/src/multipart/store.rs:541`. Add the mixed sequence to the regression suite. The two frozen batch-review entries describe this same defect, not two defects.

The probe runs against the unchanged reviewed implementation and real redb preconditions. From this report’s directory:

```sh
CARGO_TARGET_DIR="$PWD/reviewer-build" TMPDIR="$PWD/pdca-reviewer-656-scratch" \
  cargo test --offline --manifest-path pdca-reviewer-656-probe/Cargo.toml \
  --test abort_uncertainty reviewer_abort_preserves_earlier_in_flight_uncertainty \
  -- --nocapture
```

Its expected assertion is red on this patch. The existing DST tests run 1021 and 1031 separately (`crates/dst/tests/multipart_admission.rs:324`, `crates/dst/tests/multipart_admission.rs:339`), so their green results do not discharge R1.

Independent verification supports the rest of the scoped behavior:

- Stashing the tracked fix while retaining the new test reproduced the missing-API compile failure. Popping the stash restored a 29/29 green integration run (`reviewer-red.log`, `reviewer-restored-green.log`). This is the declared enhancement limitation, not a stale-target or compile defect in the patch.
- All four multipart DST tests passed with `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50`, including their explicit seed sweeps (`reviewer-dst.log`). The three restore suites passed 30 tests (`reviewer-restore.log`).
- The three brief-required mutations were executed in a separate scratch copy. Capping contention at two attempts produced `AdmissionContention { attempts: 2 }`; decrementing on Abort failed the counter assertion with 0 versus 1; re-minting after an unknown Create produced two sessions instead of one. Each test executed and failed. Restoring that copy passed all 29 tests (`reviewer-mutant-small-contention-budget.log`, `reviewer-mutant-abort-decrement.log`, `reviewer-mutant-unknown-remint.log`, `reviewer-mutants-restored-green.log`).
- Formatting, typos, docs lint, rendering/link audit, the statics scanner, and the 5-valid/6-invalid conformance vectors passed (`reviewer-scanners.log`, `reviewer-statics.log`, `reviewer-conformance.log`). Thus both declared external tools, `typos` and `docs-renderer`, were actually exercised. No live distributed-backend run is claimed.

The frozen gates are adjudicated from their captured output where the instance-scoped wrapper is unavailable here:

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-ci | NEEDS-HUMAN | Both attempts fail the all-features advisory audit for RUSTSEC-2026-0346/0349 through tikv-client’s rustls 0.21.12; preceding workspace build/tests pass. The unchanged dependency failure is inherited and explicitly outside this slice, not a new patch defect; `gate-logs/C4-ci.log:6525`, `brief.md:356`. |
| C4-verify | NEEDS-HUMAN | Green executes 29 core and 4 DST tests; the reverted production cannot compile the test, so no behavioral pre-fix red was established; `gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:21`. |
| C4-diff-cov | PASS | The captured run reports 362/363 instrumentable changed lines covered, 99.7%; 590 other changed lines are unscored, so this is not coverage of every changed line; `gate-logs/C4-diff-cov.log:603`. |
| C5-mutants | PASS | Captured baseline passes; 73 mutants yield 62 caught and 11 unviable, with none reported missed. This aggregate result does not test every error sequence; `gate-logs/C5-mutants.log:10`. |
| T4-batch-review | FAIL | Two duplicate reports of R1, independently reproduced here; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | PR/commit contribution artifacts are drafted after Check; the substantive audit must rerun at publish, exactly as the deferred evidence states; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS | Captured output shows both requested feature-enabled clippy builds finishing successfully; it establishes compilation, not live TiKV behavior; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`. |

The CI failure occurs before its conformance/statics/DST stages (`xtask/src/main.rs:1683`); those stages must not be inferred green from that log. This review separately ran conformance/statics and the affected multipart DST target, not the entire DST suite. No gate log was missing. No applicable `INTEGRATION.md` was present in the provided target.

Prior-art checking was performed by every affected path. GitHub path-history queries and closed-PR path searches covered all six changed files (`reviewer-prior-art.json`). An additional file-list audit of all 19 closed, unmerged PRs found only #647 overlapping, at `crates/custodian/src/restore.rs`; its changes concern segmented-map resolution, not this session-fence batch (`reviewer-rejected-paths.json`, `reviewer-prior-647-restore.diff`). [Issue #508](https://github.com/getwyrd/wyrd/issues/508) records the rejected monolithic attempts, and [issue #636](https://github.com/getwyrd/wyrd/issues/636) is closed as not planned. The patch’s scoped split and shared fence builder do not revive those rejected approaches. The #511 tracked deferral and the recorded timeout decision were not re-raised.

Source citations above resolve against `$PDCA_TARGET`, whose recorded base is `7bf7eb906c634d57d28f2ae26fd0b412732948d5`; local evidence paths resolve in this report’s directory. Reverse-apply checking confirms the supplied patch is restored exactly, and dependency manifests/lockfiles are unchanged. This report is advisory; it does not grant or gate acceptance.

<!-- pdca:leaf-complete -->
