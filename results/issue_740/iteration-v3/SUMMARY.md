# Result — issue 740 / validate-crate-skeleton-and-blackbox-lint

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) There is no `crates/validate` and no dependency-closure
  guard. `xtask/src/repo_guard.rs` today carries exactly two invariants — the stray-gitlink
  scan (`scan_gitlinks`, `repo_guard.rs:238`) and the `#![forbid(unsafe_code)]` crate-root
  scan (`scan_roots`, `repo_guard.rs:500`) — verified on `main` at `65ca4fd`. Nothing
  prevents a future slice from linking `wyrd-core` into the validator and quietly
  destroying the only property that makes its verdict mean anything.
- Success criterion: BINDING (demonstrable by C4-verify at Check, every leg in
  `cargo xtask ci`):
  1. `cargo xtask ci` is green with `crates/validate` a workspace member, and the new guard
     RUNS INSIDE IT — asserted, not assumed. The guard's registration moves into the `xtask`
     **lib** target as data (the repo's own precedent: the feature-gated check list "lives
     in `xtask::feature_gated_checks` (the lib target) so `xtask/tests/fdb_harness.rs` can
     assert its content directly", `xtask/src/main.rs:1504-1505`), and the test asserts the
     blackbox guard is in the list `run_ci` executes. Without that, a guard that is defined,
     tested and never called passes every assertion in this brief — which is exactly the
     wiring hazard the repo already engineered against with `run_ci_steps`' injected `exec`
     (`main.rs:1486-1498`).
  2. The guard is **flippable, not vacuous**: fed a synthetic `cargo metadata` document in
     which `wyrd-validate` has a **normal** dependency on a `wyrd-*` crate, the pure scan
     function returns exactly one violation naming that crate; fed the same document with
     that edge marked `"kind": "dev"`, it returns none; fed one where the `wyrd-*` edge is a
     normal dependency that is **`optional` and off by default**, it returns the violation
     (see Design — this is the case a default-feature resolve hides); and run over the REAL
     workspace metadata it returns none.
  3. The CLI surface is bound flag by flag, not by sample: `wyrd-validate` invoked with ALL
     of `--endpoint --region --bucket --scenario --duration --workers --seed --out --run-id
     --driver-placement` exits 0 and echoes a resolved-configuration block in which EACH of
     the ten flags appears with the value it was given (assert per flag, so an implementation
     that parses three and ignores seven fails); a missing required argument exits non-zero
     with the offending flag named on stderr; and an UNRECOGNISED flag exits non-zero naming
     it (a deliberate departure from the peer parser — see Design and the Plan-review
     response).
  4. Credential resolution is bound in both directions, over the injected lookup so no
     process env is mutated: with `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` present the
     resolved id is the AWS one and the reported source says so; with those absent and
     `WYRD_S3_ACCESS_KEY`/`WYRD_S3_SECRET_KEY` present the resolved id is the Wyrd one and
     the source says so; with BOTH present AWS wins; with neither the run exits non-zero
     naming what to set. In every case the echoed block contains the access-key **id** and
     never the secret — asserted by searching the whole output for the secret's value.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) new workspace member `crates/validate` — package `wyrd-validate`, a lib
  target holding the pure decisions and a thin `[[bin]] wyrd-validate` over it, both crate
  roots carrying `#![forbid(unsafe_code)]`; (b) the argument surface `--endpoint --region
  --bucket --scenario --duration --workers --seed --out --run-id --driver-placement`,
  parsed, validated for presence, and echoed as a resolved-configuration block; (c)
  credential resolution from `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, falling back to
  `WYRD_S3_ACCESS_KEY` / `WYRD_S3_SECRET_KEY`, over an injected lookup; (d) a third
  invariant in `xtask/src/repo_guard.rs` — the package's **normal** dependency closure,
  resolved with `--all-features`, plus its declared normal (incl. optional) dependencies,
  contain no `wyrd-*` crate — wired into `run_ci` through a lib-side registration a test can
  read; (e) its flippable test, including the optional-dependency case; (f) **strict
  unknown-flag rejection** — deliberately different from the peer parser, declared here as
  intentional scope rather than left as a Design aside, and asserted by criterion 3.
  **/ out of scope:** any S3 call whatsoever (that is #741 — this binary parses, echoes and
  exits); the capability matrix and `smoke` (#743); scenarios, oracle, pools, verdict; ANY
  new third-party crate (see Design — the arg parsing is hand-rolled precisely so this
  slice lands without an ADR-0003 dependency audit); tarball packaging (#742, wave 2 of this
  batch — it consumes this crate, it is not built here); extending the guard to any package
  other than `wyrd-validate`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny check` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 93.6% — 366 of 391 instrumentable changed lines executed (floor 80%); 391 of 941 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.92s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add the `wyrd-validate` crate skeleton and operator-facing CLI while enforcing that its normal dependency closure remains blackbox.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The target design fixes a separate binary whose normal closure contains no `wyrd-*` crate, and the living architecture fixes the ten-flag/credential contract (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:559`; `docs/design/architecture/05-building-block-view.md:253`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the patch removed `wyrd-validate` and made the focused command exit 101; restored, its normal/dev/optional discriminators and all 50 focused tests pass (`xtask/tests/blackbox_dependency_guard.rs:118`; `xtask/tests/blackbox_dependency_guard.rs:137`). |
| C3 Change | PASS | The patch stays within the declared crate/CLI/credential/guard/documentation slice and adds no third-party dependency (`crates/validate/Cargo.toml:1`; `docs/design/architecture/05-building-block-view.md:259`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The base-owned `h2` advisory must be cleared or dispositioned before merge — focused red→green and 93.6% diff coverage are green, but frozen CI fails RUSTSEC-2026-0258 and the base already carries `h2 0.4.15` (`Cargo.lock:1535`; `gate-logs/C4-verify.log:10`; `gate-logs/C4-diff-cov.log:117`; `gate-logs/C4-ci.log:5346`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must reject a reached package whose identity cannot be decoded — missing `id`/`name` can hide the forbidden prefix; mutation evidence is unavailable because its unmutated temp copy lacked a git index (`xtask/src/repo_guard.rs:729`; `xtask/src/repo_guard.rs:794`; `gate-logs/C5-mutants.log:472`). |
| T1 Structure | PASS | The validator is a workspace member with separate lib/bin roots, both forbidding unsafe code, and its manifest has no normal dependencies (`Cargo.toml:30`; `crates/validate/src/lib.rs:44`; `crates/validate/src/main.rs:10`). |
| T2 Shape | FAIL | Metadata decoding is not structurally fail-closed: incomplete package records are dropped and later treated as opaque names, contradicting the guard's stated refusal semantics (`xtask/src/repo_guard.rs:702`; `xtask/src/repo_guard.rs:729`). |
| T3 Runtime | PASS | The real CLI, real workspace metadata scan, registration path, docs, fmt, clippy, build, and workspace tests execute successfully before the unrelated advisory step (`xtask/tests/blackbox_dependency_guard.rs:469`; `xtask/src/main.rs:1565`). |
| T4 Contribution | FAIL | The batched review's three reports collapse to the grounded fail-closed defect above; the separate contribution-artifact audit is N/A until its mandatory publish rerun (`xtask/src/repo_guard.rs:729`; `gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add a regression for reached package records missing `id` or `name` and remove the opaque-ID fallback, or the suite overclaims malformed-metadata coverage (`xtask/tests/blackbox_dependency_guard.rs:293`; `xtask/src/repo_guard.rs:819`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether strict flag rejection, credential precedence, and the no-internal-crate boundary are the right first-slice operator contract — automated evidence cannot establish product fitness (`docs/design/architecture/05-building-block-view.md:257`; `docs/design/architecture/05-building-block-view.md:274`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — The base-owned `h2` advisory must be cleared or dispositioned before merge — focused red→green and 93.6% diff coverage are green, but frozen CI fails RUSTSEC-2026-0258 and the base already carries `h2 0.4.15` (`Cargo.lock:1535`; `gate-logs/C4-verify.log:10`; `gate-logs/C4-diff-cov.log:117`; `gate-logs/C4-ci.log:5346`).
- [ ] C5 Causal adequacy — Rebuild must reject a reached package whose identity cannot be decoded — missing `id`/`name` can hide the forbidden prefix; mutation evidence is unavailable because its unmutated temp copy lacked a git index (`xtask/src/repo_guard.rs:729`; `xtask/src/repo_guard.rs:794`; `gate-logs/C5-mutants.log:472`).
- [ ] T5 Judgment — Rebuild must add a regression for reached package records missing `id` or `name` and remove the opaque-ID fallback, or the suite overclaims malformed-metadata coverage (`xtask/tests/blackbox_dependency_guard.rs:293`; `xtask/src/repo_guard.rs:819`).
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether strict flag rejection, credential precedence, and the no-internal-crate boundary are the right first-slice operator contract — automated evidence cannot establish product fitness (`docs/design/architecture/05-building-block-view.md:257`; `docs/design/architecture/05-building-block-view.md:274`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- [ ] The proposed guard does not enforce the brief's claimed *whole* normal dependency closure across feature sets. The design runs `cargo metadata --format-version 1 --locked` and follows that resolve graph (`brief.md:189-198`), but never enables all features; a non-default feature can therefore hide an optional normal `wyrd-*` dependency from both the real-workspace check and the planted normal/dev fixture. This is a concrete pattern in the target: `crates/metadata-tikv/Cargo.toml:11-27` declares off-by-default optional normal dependencies, and `xtask/src/lib.rs:40-47` explicitly records that default workspace commands do not cover non-default features. Revise the guard/criterion to inspect every feature-enabled normal edge (or explicitly forbid optional `wyrd-*` declarations), otherwise the invariant promised at `brief.md:52-58` is false.
- [ ] The tracker requires that “the lint is green, and a deliberately added `wyrd-core` dependency turns it **red**” (`notes.json:1`), but the brief substitutes a unit test that feeds synthetic JSON directly to the pure scan function (`brief.md:28-31`, `brief.md:37-51`). That can pass while the production path fails to invoke the scanner, supplies different metadata, or discards its violations. The target calls out exactly this wiring hazard and uses injected execution so a wrong `run_ci` call site is test-visible (`xtask/src/main.rs:1486-1498`). Make the red criterion exercise the guard/`cargo xtask ci` wiring, not only its parser.
- [ ] The binding CLI criterion does not verify the CLI surface promised by the tracker. The issue requires eleven flags to be “parsed and echoed” plus AWS credentials falling back to the `WYRD_S3_*` pair (`notes.json:1`; repeated at `brief.md:70-76`), while the only stated behavior check supplies just `--endpoint`, `--bucket`, and `--scenario` with unspecified credentials “in the environment” (`brief.md:33-36`). An implementation that ignores the other eight flags and never implements the fallback can satisfy the criterion. Bind the criterion to each flag's resolved output and to both precedence/fallback credential cases.
- [ ] The brief adds an observable argument-contract change not present in the tracker: rejecting every unknown flag (`brief.md:170-174`). The tracker asks for the named surface to be parsed and echoed (`notes.json:1`), and the cited peer parser deliberately accepts arbitrary `--flag value` pairs (`crates/server/src/cli.rs:2501-2522`). Strict unknown-flag rejection therefore adds a separate compatibility policy and tests to this crate-plus-lint slice; remove it from this brief or have the human explicitly accept that extra scope.
- [ ] size backstop — this slice is behaving oversized: patch is 114 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Size backstop is tripped on its own terms: patch is 114 KB against a 100 KB threshold, and 2 build/iterate rounds have already been spent — the bundle's own recommendation is to re-split rather than attempt a third Do round. That call is reinforced by the shape of the findings, which read as several separable concerns bundled into one slice rather than one coherent defect: - the dependency-closure guard's fail-closed gap on malformed/incomplete `cargo metadata` records (missing `id`/`name` silently skipped or opaque-ID-fallback at repo_guard.rs:729/730/751) — the guard's actual load-bearing purpose; - the guard's own red/wiring criterion tests only the pure scan function, not that `cargo xtask ci` actually calls it; - the CLI surface criterion covers 3 of 11 flags and doesn't bind the credential fallback per the tracker's own ask; - the strict-unknown-flag-rejection behavior is scope not present in the tracker and needs an explicit accept/drop call. Re-plan should weigh whether the crate skeleton + CLI surface + credential resolution + dependency guard truly belong in one slice, or split along those seams (e.g., guard correctness as its own child) before another Do attempt.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- C5 (cargo mutants) could not run at all: its isolated scratch build copy fails `scan_gitlinks_is_green_over_the_real_index` (`xtask/tests/repo_hygiene_guards.rs:137`, `git ls-files -s -z must succeed`) because that copy isn't a real git working tree/index. This is a pre-existing repo-hygiene test assumption colliding with `cargo mutants`' baseline build environment, unrelated to this patch's content — worth a fix so C5 gives real signal on future bundles instead of erroring out entirely.
