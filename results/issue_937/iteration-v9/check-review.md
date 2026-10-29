Review of #937’s pure, versioned S3 capability matrix: one reproduced GET-size classification defect needs implementation follow-up.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The pure scope, three row states, version selection and fatal-answer policy are explicit and testable; the remaining downstream contract decision is recorded under Validation (brief.md:26; brief.md:189; docs/design/proposals/draft/0017-blackbox-validation-tool.md:505). |
| C2 Reproduction (red pre-fix) | N/A | The accepted net-new exception applies: stashing production while retaining the test produces an unresolved matrix import, not an executed behavioral red (brief.md:215; review-red-green.log:7). |
| C3 Change | PASS | The four changed files stay inside the approved scope and the 99,636-byte patch meets the 100,000-byte limit; the HeadBucket carry-forward is addressed with row and run assertions (crates/validate/src/matrix.rs:827; crates/validate/tests/capability_matrix.rs:468; brief.md:261). |
| C4 Verification (red→green) | PASS | The agreed net-new verification posture is met: restoration passes all 39 tests and targeted checks; full CI remains red solely on the already-accepted inherited advisories, so this is not an all-gates-green claim (review-red-green.log:601; brief.md:186; gate-logs/C4-ci.log:9338). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make a declared GET size disagreement fatal before treating a cut as unsettled — a known-wrong Content-Length currently yields INCONCLUSIVE for whole, ranged and conditional-miss GETs (crates/validate/src/matrix.rs:889; review-probes.log:6). |
| T1 Structure | PASS | The classifier stays within the permitted pure library seam, with immutable profiles and no new dependency edge; the existing ErrorCode reuse is explicitly authorized (crates/validate/src/matrix.rs:18; crates/validate/src/matrix.rs:182; brief.md:179). |
| T2 Shape | PASS | The plain integration target exercises production functions, strict tag/range grammar and version boundaries; formatting, spelling and draft-doc checks pass (crates/validate/tests/capability_matrix.rs:7; crates/validate/tests/capability_matrix.rs:706; review-docs-render.log:3). |
| T3 Runtime | PASS | No external waits, clocks, shared mutable globals or destructive execution are introduced; actual compilation and execution succeed, with grading correctness qualified by C5 (crates/validate/src/matrix.rs:983; crates/validate/src/matrix.rs:1297; review-green.log:303). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design and their substantive audit is owed at publish; prior art was independently checked by affected path, and the batch-review defect is consolidated under C5 (gate-logs/T4-contribution.log:10; review-prior-art.json:2; gate-logs/T4-batch-review.log:10). |
| T5 Judgment | PASS | No capability-probe symptom guard or scope expansion was found; the sole surviving mutant removes failure-class display text and does not alter classification, so it remains advisory (crates/validate/src/matrix.rs:739; gate-logs/C5-mutants.log:13). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm the observation contract is fit for #939’s smoke integration, including ownership of the recorded PUT/DELETE unreadable-versus-cut mapping decision — those facts determine FAIL versus INCONCLUSIVE (brief.md:288; brief.md:303; crates/validate/src/s3.rs:399). |

The implementation follow-up is one defect, independently reproduced in four cases. Source citations above and below resolve under `$PDCA_TARGET`; evidence paths resolve in this review directory. The target is the disposable snapshot of base `93a6f37` with the patch applied. All four patch-file checksums were identical after the stash/pop cycle (`review-red-green.log:553`).

**Known size disagreement is lost after a body cut.** `expect_bytes` reads only the body and expected bytes, ignoring `Response::content_length`. A correct prefix therefore takes the unsettled return at `crates/validate/src/matrix.rs:918` even when the response head already contradicts the expected representation size. Whole GET, ranged GET and conditional-miss GET all call this helper (`crates/validate/src/matrix.rs:1009`, `:1037`, `:1062`). The taxonomy makes size disagreement fatal integrity (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:505`). The real streaming client exposes this header and reports premature closure separately (`crates/validate/src/s3/body.rs:431`, `:494`), so these are representable observations.

The standalone `review-probes.rs` links the actual Cargo-built library; it does not copy the classifier. With the repository’s pinned Rust 1.96.0, these assertions fail:

| Exercise | Declared length | Correct bytes before cut | Actual row / run |
|----------|-----------------|--------------------------|------------------|
| Whole GET of 16 bytes | 15 | First 8 bytes | Unsettled / INCONCLUSIVE |
| Whole GET of 16 bytes | 17 | All 16 bytes | Unsettled / INCONCLUSIVE |
| Range 2–5 of 16 bytes | 5 | All 4 requested bytes | Unsettled / INCONCLUSIVE |
| Conditional-miss GET of 16 bytes | 17 | First 8 bytes | Unsettled / INCONCLUSIVE |

Each should be fatal integrity / FAIL. The control with declared length 16 and an 8-byte correct prefix stays unsettled and passes its assertion. Results: four failures, one passing control (`review-probes.log:38`). Reproduce with the compile command at `review-probes.log:1`, then `./review-probes --nocapture --test-threads=1` from this directory. Compare a present declared length against the requested representation length before accepting the cut as unsettled; for a range that is the slice length. Add row/run regressions while preserving the legitimate correct-length and absent-length cut cases. The two reports in `gate-logs/T4-batch-review.log:10` describe this same omission, not two separate defects.

**The available verification supports the rest of the scoped change, with explicit limits.** I independently ran `cargo test --offline --locked -p wyrd-validate --test capability_matrix` before and after stashing production, `cargo clippy --offline --locked -p wyrd-validate --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`, targeted `typos`, docs lint, and the real docs renderer. The restored test target passes 39/39; clippy and the scanners pass; rendering produces 99 pages with a successful link audit (`review-red-green.log:601`, `review-clippy.log:222`, `review-docs-lint.log:1`, `review-docs-render.log:2`). Both external dependencies named in `brief.md:213` were exercised. The stashed leg exits 101 at compilation; it is not asserted as behavioral red→green.

The remaining gate results are adjudicated from their frozen logs, not represented as independent reruns:

- C4-ci: FAIL, with only the two cargo-deny advisory checks failing on RUSTSEC-2026-0385/-0386; this is the inherited base condition already accepted in the brief (`gate-logs/C4-ci.log:8473`, `:8555`, `:9338`; `brief.md:183`). It is not a patch defect.
- C4-verify: UNVERIFIABLE, because the reverted module’s API is absent; its green leg ran 39 tests, matching my rerun (`gate-logs/C4-verify.log:14`, `:544`).
- C4-diff-cov: PASS, 620/676 instrumentable changed lines executed, 91.7%, against an 80% floor (`gate-logs/C4-diff-cov.log:141`).
- C5-mutants: advisory FAIL, 140 caught, 30 unviable, one missed display-only mutant; the log supplies aggregate mutation evidence, not separate traces for all fifteen brief-prescribed edits (`gate-logs/C5-mutants.log:13`).
- T4-batch-review: FAIL; the reported Content-Length omission is reproduced and consolidated above (`gate-logs/T4-batch-review.log:10`).
- T4-contribution: N/A/deferred; substantive checking runs at publish (`gate-logs/T4-contribution.log:10`).
- host-tikv: PASS in the captured log, which shows successful completion of both requested feature compilations (`gate-logs/host-tikv.log:111`, `:209`).

**Prior art was checked mechanically by path.** GitHub main history was queried for each of the four affected files and for `crates/validate/`; neither new matrix file has prior merged history. I fetched changed-file lists for all 19 closed, unmerged PRs returned by the repository query and compared them with both affected path surfaces; none overlap. The queries, commit URLs and complete file lists are retained in `review-prior-art.json`. Existing client/payload work and the draft proposal are the relevant predecessors. Gateway/profile citations were also checked against the supplied target, including the still-active multipart denylist at `crates/gateway-s3/src/lib.rs:410`.

Fitness sign-off remains human-owned. The two new body states distinguish whole malformed documents from interrupted transport; the existing #939 handoff must preserve that distinction. The brief’s recorded PUT/DELETE ambiguity remains assigned to that downstream decision (`brief.md:303`); this review does not request a client change inside #937.
<!-- pdca:leaf-complete -->
