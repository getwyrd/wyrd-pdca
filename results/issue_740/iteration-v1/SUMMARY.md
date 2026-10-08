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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 90.1% — 218 of 242 instrumentable changed lines executed (floor 80%); 242 of 547 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.21s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #740: add the `wyrd-validate` crate/CLI skeleton and enforce its blackbox normal-dependency boundary inside `cargo xtask ci`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief binds the crate, ten-flag CLI, credential precedence, fail-closed all-feature closure guard, wiring proof, and explicit no-dependency scope, so the implementation target is decidable (`brief.md:12`, `brief.md:24`, `brief.md:101`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, the focused test command was red because `wyrd-validate` did not exist and no blackbox-guard symbol was present; restored planted cases independently distinguish normal, dev, and optional-off-by-default edges (`xtask/tests/blackbox_dependency_guard.rs:81`, `xtask/tests/blackbox_dependency_guard.rs:93`, `xtask/tests/blackbox_dependency_guard.rs:101`). |
| C3 Change | PASS | The change remains within the planned crate/CLI/guard surface, registers the new workspace member, and adds no normal or third-party dependency (`Cargo.toml:30`, `crates/validate/Cargo.toml:11`). |
| C4 Verification (red→green) | PASS | After restoring the patch, all 19 focused tests and the guard rerun passed; frozen CI also exercised the registered guard and completed green (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:27`, `gate-logs/C4-ci.log:3452`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the strict-flag proof to cover a flag-shaped value token — the present test checks unknown flags only in flag position, so it misses the parser consuming `--totally-bogus` as another flag's value (`crates/validate/tests/cli_surface.rs:117`, `crates/validate/src/lib.rs:79`). |
| T1 Structure | PASS | The conventional lib/bin roots both forbid unsafe code, pure decisions stay in the lib, and the CI loop consumes the lib-side guard registry (`crates/validate/src/lib.rs:25`, `crates/validate/src/main.rs:5`, `xtask/src/main.rs:1559`). |
| T2 Shape | PASS | The guard follows normal edges transitively, separately checks declared normal dependencies, fails closed for the specified absent graph/package states, and is registered as test-visible data (`xtask/src/repo_guard.rs:616`, `xtask/src/repo_guard.rs:676`, `xtask/src/repo_guard.rs:711`, `xtask/src/repo_guard.rs:826`). |
| T3 Runtime | FAIL | Strict unknown-flag rejection is bypassable: an independently run CLI with `--endpoint --totally-bogus` exited 0 and echoed that token as the endpoint because the parser accepts any next token as a value (`crates/validate/src/lib.rs:79`). |
| T4 Contribution | FAIL | Decide and add the canonical living-architecture description of the ten new CLI flags before merge — the standing same-PR docs-currency rule is mandatory and the patch updates no living architecture file (`AGENTS.md:154`, `gate-logs/T4-batch-review.log:10`); the publish-only artifact audit is N/A by design (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Route back to Do for the strict-parser regression and test above — accepting an unknown flag in a value slot contradicts the operator-safety decision in the brief and can silently misreport a run (`brief.md:217`, `crates/validate/src/lib.rs:79`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this skeleton plus dependency guard is a trustworthy foundation for dependent slices #741/#742 — those slices will rely on this CLI contract and on the validator remaining independent of Wyrd internals (`brief.md:90`, `brief.md:185`). |

Evidence caveats:

- The local full-CI rerun passed formatting, clippy, build, workspace tests, and the new guard, then stopped only because the sandbox exposed Cargo's advisory-database lock as read-only; the frozen run completed `cargo deny`, conformance, statics, and DST successfully (`gate-logs/C4-ci.log:3452`).
- The mutation row tested no mutants because its unmutated copied tree could not run the pre-existing real-git-index test (`gate-logs/C5-mutants.log:426`, `gate-logs/C5-mutants.log:455`); this is harness topology, not evidence of a patch regression.
- Prior-art was rechecked by substantive affected path: merged history contains the proposal and earlier #616 guard work, while the closed-unmerged PR file scan found no attempt touching `crates/validate/**`, `xtask/src/repo_guard.rs`, or `xtask/tests/blackbox_dependency_guard.rs`.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild the strict-flag proof to cover a flag-shaped value token — the present test checks unknown flags only in flag position, so it misses the parser consuming `--totally-bogus` as another flag's value (`crates/validate/tests/cli_surface.rs:117`, `crates/validate/src/lib.rs:79`).
- [ ] T5 Judgment — Route back to Do for the strict-parser regression and test above — accepting an unknown flag in a value slot contradicts the operator-safety decision in the brief and can silently misreport a run (`brief.md:217`, `crates/validate/src/lib.rs:79`).
- [ ] Validation — fitness-to-purpose — Decide whether this skeleton plus dependency guard is a trustworthy foundation for dependent slices #741/#742 — those slices will rely on this CLI contract and on the validator remaining independent of Wyrd internals (`brief.md:90`, `brief.md:185`).
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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild the strict-flag proof to cover a flag-shaped value token — the present test checks unknown flags only in flag position, so it misses the parser consuming `--totally-bogus` as another flag's value (`crates/validate/tests/cli_surface.rs:117`, `crates/validate/src/lib.rs:79`).; T5 Judgment — Route back to Do for the strict-parser regression and test above — accepting an unknown flag in a value slot contradicts the operator-safety decision in the brief and can silently misreport a run (`brief.md:217`, `crates/validate/src/lib.rs:79`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
