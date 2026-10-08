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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 90.0% — 262 of 291 instrumentable changed lines executed (floor 80%); 291 of 682 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.04s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #740's new `wyrd-validate` crate/CLI and the `cargo xtask ci` guard that keeps its normal dependency closure free of `wyrd-*` crates.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief binds the workspace member, ten-flag CLI, credential precedence, fail-closed registered guard, and zero new dependencies precisely enough to judge (`brief.md:12`, `brief.md:24`, `brief.md:101`, `brief.md:124`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, `crates/validate` and the guard were absent and the exact two-suite command exited 101 for unknown package `wyrd-validate`; restored, the declared net-new green-only suites pass 30/30 (`brief.md:57`, `gate-logs/C4-verify.log:10`). |
| C3 Change | PASS | The patch stays within the declared crate/CLI/guard/docs scope, registers the member, adds no validator dependency, and updates the living architecture contract (`Cargo.toml:30`, `crates/validate/Cargo.toml:1`, `docs/design/architecture/05-building-block-view.md:253`). |
| C4 Verification (red→green) | PASS | The targeted suites independently pass 30/30, the registered scanner independently runs green, and frozen evidence shows the full gate including deny/conformance finishing successfully; the verify row's green-only posture is the brief's declared net-new case (`gate-logs/C4-ci.log:27`, `gate-logs/C4-ci.log:2884`, `gate-logs/C4-ci.log:3463`, `gate-logs/C4-verify.log:20`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the proof around the actual trust boundary: add successful multi-hop closure, malformed declared-`kind`, non-UTF-8 credential, and control-character output cases, because current planted tests exercise only direct normal/dev/optional edges and UTF-8 fixtures (`xtask/tests/blackbox_dependency_guard.rs:81`, `crates/validate/tests/cli_surface.rs:318`). |
| T1 Structure | PASS | The workspace member has separate lib/bin roots with `#![forbid(unsafe_code)]`, pure decisions stay in the lib, and `run_ci` iterates the lib-side guard registration (`crates/validate/src/lib.rs:35`, `crates/validate/src/main.rs:5`, `xtask/src/main.rs:1563`). |
| T2 Shape | FAIL | A missing or non-null/non-string declared dependency `kind` is treated as non-normal and silently skipped, so malformed metadata can hide a forbidden dependency despite the guard's fail-closed contract (`xtask/src/repo_guard.rs:756`). |
| T3 Runtime | FAIL | Direct process probes exited 0 while non-UTF-8 AWS credentials silently selected the Wyrd identity and while newline-bearing values forged extra configuration rows; the lossy environment lookup and raw interpolation are at `crates/validate/src/main.rs:11` and `crates/validate/src/lib.rs:296`. |
| T4 Contribution | FAIL | The required three-pass review remains red with the same three grounded blockers; affected-path history found no prior validator/closure implementation, while the contribution-artifact audit is N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must fail closed on unreadable metadata and credentials, emit an unambiguous configuration record, and prove transitive traversal; otherwise the operator-identity and blackbox-boundary judgments are not trustworthy (`crates/validate/src/main.rs:11`, `xtask/src/repo_guard.rs:757`, `crates/validate/src/lib.rs:302`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the corrected CLI and guard are fit to become the foundation for dependent slices #741/#743, because those slices inherit this identity surface and blackbox boundary (`brief.md:90`, `brief.md:114`). |

Gate evidence caveats:

- `C5-mutants` is not evidence of a patch defect: its unmutated baseline failed an existing git-index test because `git ls-files` could not run in the cargo-mutants temporary tree, so no mutant was tested (`gate-logs/C5-mutants.log:443`, `gate-logs/C5-mutants.log:466`).
- My full local `cargo xtask ci` rerun passed the changed scanners, formatting, clippy, build, and workspace tests, then stopped at `cargo deny` because this sandbox exposes Cargo's advisory database lock read-only; the frozen gate exercised that leg successfully (`gate-logs/C4-ci.log:2884`, `gate-logs/C4-ci.log:2897`).
- Prior-art was rechecked by every affected path through merged commit history and across all 11 closed-unmerged PRs; no prior `crates/validate/**` or blackbox-closure implementation was found.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild the proof around the actual trust boundary: add successful multi-hop closure, malformed declared-`kind`, non-UTF-8 credential, and control-character output cases, because current planted tests exercise only direct normal/dev/optional edges and UTF-8 fixtures (`xtask/tests/blackbox_dependency_guard.rs:81`, `crates/validate/tests/cli_surface.rs:318`).
- [ ] T5 Judgment — Rebuild must fail closed on unreadable metadata and credentials, emit an unambiguous configuration record, and prove transitive traversal; otherwise the operator-identity and blackbox-boundary judgments are not trustworthy (`crates/validate/src/main.rs:11`, `xtask/src/repo_guard.rs:757`, `crates/validate/src/lib.rs:302`).
- [ ] Validation — fitness-to-purpose — Human must decide whether the corrected CLI and guard are fit to become the foundation for dependent slices #741/#743, because those slices inherit this identity surface and blackbox boundary (`brief.md:90`, `brief.md:114`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- [ ] The proposed guard does not enforce the brief's claimed *whole* normal dependency closure across feature sets. The design runs `cargo metadata --format-version 1 --locked` and follows that resolve graph (`brief.md:189-198`), but never enables all features; a non-default feature can therefore hide an optional normal `wyrd-*` dependency from both the real-workspace check and the planted normal/dev fixture. This is a concrete pattern in the target: `crates/metadata-tikv/Cargo.toml:11-27` declares off-by-default optional normal dependencies, and `xtask/src/lib.rs:40-47` explicitly records that default workspace commands do not cover non-default features. Revise the guard/criterion to inspect every feature-enabled normal edge (or explicitly forbid optional `wyrd-*` declarations), otherwise the invariant promised at `brief.md:52-58` is false.
- [ ] The tracker requires that “the lint is green, and a deliberately added `wyrd-core` dependency turns it **red**” (`notes.json:1`), but the brief substitutes a unit test that feeds synthetic JSON directly to the pure scan function (`brief.md:28-31`, `brief.md:37-51`). That can pass while the production path fails to invoke the scanner, supplies different metadata, or discards its violations. The target calls out exactly this wiring hazard and uses injected execution so a wrong `run_ci` call site is test-visible (`xtask/src/main.rs:1486-1498`). Make the red criterion exercise the guard/`cargo xtask ci` wiring, not only its parser.
- [ ] The binding CLI criterion does not verify the CLI surface promised by the tracker. The issue requires eleven flags to be “parsed and echoed” plus AWS credentials falling back to the `WYRD_S3_*` pair (`notes.json:1`; repeated at `brief.md:70-76`), while the only stated behavior check supplies just `--endpoint`, `--bucket`, and `--scenario` with unspecified credentials “in the environment” (`brief.md:33-36`). An implementation that ignores the other eight flags and never implements the fallback can satisfy the criterion. Bind the criterion to each flag's resolved output and to both precedence/fallback credential cases.
- [ ] The brief adds an observable argument-contract change not present in the tracker: rejecting every unknown flag (`brief.md:170-174`). The tracker asks for the named surface to be parsed and echoed (`notes.json:1`), and the cited peer parser deliberately accepts arbitrary `--flag value` pairs (`crates/server/src/cli.rs:2501-2522`). Strict unknown-flag rejection therefore adds a separate compatibility policy and tests to this crate-plus-lint slice; remove it from this brief or have the human explicitly accept that extra scope.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild the proof around the actual trust boundary: add successful multi-hop closure, malformed declared-`kind`, non-UTF-8 credential, and control-character output cases, because current planted tests exercise only direct normal/dev/optional edges and UTF-8 fixtures (`xtask/tests/blackbox_dependency_guard.rs:81`, `crates/validate/tests/cli_surface.rs:318`).; T5 Judgment — Rebuild must fail closed on unreadable metadata and credentials, emit an unambiguous configuration record, and prove transitive traversal; otherwise the operator-identity and blackbox-boundary judgments are not trustworthy (`crates/validate/src/main.rs:11`, `xtask/src/repo_guard.rs:757`, `crates/validate/src/lib.rs:302`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
