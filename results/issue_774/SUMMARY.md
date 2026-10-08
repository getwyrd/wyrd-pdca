# Result — issue 774 / validate-crate-skeleton-and-cli-surface

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) There is no `crates/validate`, so there is nowhere to put
  the blackbox validator proposal 0017 specifies — `ls crates/validate` on `main` @ `65ca4fd`
  → no such directory, and `git log --oneline -- crates/validate` is empty (it has never
  existed). Every later slice in milestone 17 (#741 the S3 client, #742 packaging, #743 the
  capability matrix) presumes the crate, its binary, and the argument surface an operator
  points at their own cluster.
- Success criterion: BINDING, every leg observable inside `cargo xtask ci`:
  1. `crates/validate` is a member of the root `[workspace] members`, the workspace builds,
     and `cargo xtask ci` exits 0. Registration is not bookkeeping:
     `unregistered_manifests` (`xtask/src/repo_guard.rs:421`) already fails the gate on a
     package under `crates/` that is not a member.
  2. **The CLI surface is bound flag by flag, not by sample.** `wyrd-validate` invoked with
     ALL TEN of `--endpoint --region --bucket --scenario --duration --workers --seed --out
     --run-id --driver-placement` exits 0 and echoes a resolved-configuration block in which
     **each** flag appears with the value it was given — asserted per flag, so an
     implementation that parses three and ignores seven FAILS. A missing required argument
     exits non-zero naming the offending flag on stderr.
  3. **Credential resolution is bound in all four directions**, over an injected lookup so no
     process env is mutated (process env is shared across parallel test threads and flakes):
     AWS pair present → the resolved id is the AWS one and the reported source says so;
     AWS absent and `WYRD_S3_ACCESS_KEY`/`WYRD_S3_SECRET_KEY` present → the Wyrd one, source
     says so; BOTH present → AWS wins; NEITHER → exit non-zero naming what to set. In every
     case the echoed block contains the access-key **id** and never the secret — asserted by
     searching the whole output for the secret's value.
  4. **Strict argument rejection, both halves** (see the DECISION note below): an
     unrecognised `--flag` exits non-zero naming it, AND a `--`-prefixed token appearing in a
     **value slot** exits non-zero naming both flags. The second half is explicit because it
     is exactly what the peer parser gets wrong and what cost iteration v1: `ParsedArgs::parse`
     takes `args[i+1]` as the value verbatim (`crates/server/src/cli.rs:2512-2515`), so
     `--bucket --typo` silently sets bucket to `"--typo"`.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) new workspace member `crates/validate` — package `wyrd-validate`, a **lib**
  target holding the pure decisions (so integration tests can reach them) and a thin
  `[[bin]] wyrd-validate` over it, **both** crate roots carrying `#![forbid(unsafe_code)]`
  (the existing guard scans every target kind including bins, `repo_guard.rs:380-386`, so a
  missing attribute on either root fails `cargo xtask ci` immediately); (b) the ten-flag
  argument surface, parsed, validated for presence, and echoed as a resolved-configuration
  block; (c) credential resolution AWS → `WYRD_S3_*` → refuse, over an **injected** lookup
  closure rather than reading `std::env` directly; (d) registration in the root
  `[workspace] members`; (e) strict argument rejection per criterion 4.
  **/ out of scope:** the dependency-closure guard (that is child-3 — do not touch
  `xtask/**`); any S3 call whatsoever, and any `aws-sdk-s3` dependency (#741 — this binary
  parses, echoes and exits); the capability matrix and `smoke` (#743); scenarios, oracle,
  pools, verdict; tarball packaging (#742); **ANY new third-party crate.**

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 223 of 223 instrumentable changed lines executed (floor 80%); 223 of 512 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 42 mutants tested in 11s: 26 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_774/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.12s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #774 validator skeleton meets the brief: a dependency-free workspace crate with a strict ten-flag CLI, credential resolution, and a configuration echo.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Four observable obligations and the settled strict-parser decision bound this skeleton; no external service is required at Check (`brief.md:10`, `brief.md:34`, `brief.md:94`). |
| C2 Reproduction (red pre-fix) | PASS | Criterion absence reproduced: stashing the patch removes the crate and Cargo exits 101 with package-not-found; this is the declared new-feature red, not a failing pre-existing assertion (`reviewer-red-green.log:4`, `brief.md:96`). |
| C3 Change | PASS | The change stays within the crate/CLI scope and updates the living architecture; no new third-party or internal Wyrd dependency is introduced (`crates/validate/Cargo.toml:21`, `docs/design/architecture/05-building-block-view.md:253`). |
| C4 Verification (red→green) | PASS | Real compilation and execution establish the workspace contract: restored tests pass 19/19 and the independent full CI exits 0 (`Cargo.toml:33`, `reviewer-red-green.log:51`, `reviewer-ci.log:3770`). |
| C5 Causal adequacy | PASS | Input-boundary rejection prevents silently absorbed flags or fallback to a different identity; no capability probe masks a load-time cause, and both incomplete-pair directions are exercised (`crates/validate/src/args.rs:130`, `crates/validate/src/access_keys.rs:139`, `crates/validate/tests/cli_surface.rs:305`). |
| T1 Structure | PASS | Blackbox isolation and injected I/O preserve independent testing without backend coupling; both crate roots forbid unsafe code (`crates/validate/Cargo.toml:21`, `crates/validate/src/lib.rs:1`, `crates/validate/src/main.rs:1`). |
| T2 Shape | PASS | The CLI gives named errors consistently with the product parser while enforcing the settled stricter contract; formatting and warnings-denied Clippy pass (`crates/validate/src/args.rs:92`, `reviewer-quality.log:2`, `reviewer-clippy.log:4`). |
| T3 Runtime | PASS | The real invocation echoes every supplied value, keeps the secret private, and discloses that nothing was validated; rejected writes and failed flushes cannot return success (`reviewer-cli.log:2`, `reviewer-cli.log:18`, `crates/validate/tests/cli_surface.rs:537`). |
| T4 Contribution | N/A | Contribution artifacts are deliberately drafted after Check; their substantive audit must rerun at publish, so the deferred row creates no human blocker (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Prior test gaps are now exercised, no tested mutant survives, and path-based history/closed-PR inspection finds no earlier validator implementation to reconcile (`crates/validate/tests/cli_surface.rs:261`, `reviewer-coverage-summary.log:1`, `reviewer-mutants.log:4`, `reviewer-prior-art.log:15`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the observed operator experience for this skeleton milestone — exit 0 certifies configuration resolution only, while actual S3 validation remains future work in #741/#743 (`reviewer-cli.log:18`, `crates/validate/src/lib.rs:8`). |

No implementation defect was found. The independent executions support the technical verdicts; fitness-to-purpose remains the required human decision.

- **Execution evidence:** `git stash --include-untracked` reproduced the missing-package red, and `git stash pop` restored identical source bytes before the 19-test green (`reviewer-red-green.log:4`, `reviewer-red-green.log:24`, `reviewer-red-green.log:51`). Full `cargo xtask ci` passed, including docs, workspace build/tests, dependency checks, conformance, repository guards, and DST (`reviewer-ci.log:3770`). Both TiKV feature compilations passed independently (`reviewer-tikv.log:103`, `reviewer-tikv.log:204`).
- **Evidence sensitivity:** independent LLVM coverage reports 232/232 instrumented production source lines covered across the four source files (`reviewer-coverage-summary.log:1`). The frozen diff scorer reports its distinct changed-line denominator, 223/223, with 289 unscored lines (`gate-logs/C4-diff-cov.log:51`). Independent mutation testing reproduces 42 tested: 26 caught, 16 unviable, none surviving (`reviewer-mutants.log:4`; frozen agreement at `gate-logs/C5-mutants.log:13`). Secret selection/redaction, both half-pair directions, empty credentials, usage text, and write/flush errors have direct assertions (`crates/validate/tests/cli_surface.rs:179`, `crates/validate/tests/cli_surface.rs:261`, `crates/validate/tests/cli_surface.rs:305`, `crates/validate/tests/cli_surface.rs:336`, `crates/validate/tests/cli_surface.rs:537`).
- **History and frozen gate evidence:** the main-branch history query for `crates/validate` returned no commits. All 356 closed/merged PR file lists were inspected, including full pagination for the two lists exceeding 100 files; none touched that crate (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:15`). The supplied batch-review log reports zero blocking findings; its instance wrapper was not rerun (`gate-logs/T4-batch-review.log:10`). The frozen C4 verification correctly records green-only for a newly created crate (`gate-logs/C4-verify.log:15`), and the contribution audit remains N/A until publish (`gate-logs/T4-contribution.log:10`).

Source citations resolve under `$PDCA_TARGET`; evidence citations resolve in this bundle. The patch reverses cleanly against the target, and all reviewed source hashes remain unchanged after verification (`reviewer-grounding.log:1`). No target-state caveat or undischarged external dependency was found.

### Advisory — adversary

# Adversarial review — issue #774 (`crates/validate` skeleton + CLI surface)

**Bottom line: I could not refute the fix on any of the four binding criteria.** Below is what I tried, followed by two small unwarranted claims and two scope questions for the maintainer.

## What I attacked and could not break

- **The evidence is real, not a mirror.** `gate-logs/C4-ci.log:2721-2744` shows `tests/cli_surface.rs` running all 19 tests inside `cargo xtask ci`, the one gating row. The binary tests spawn the real `CARGO_BIN_EXE_wyrd-validate` (`crates/validate/tests/cli_surface.rs:78`). The library tests call the production `run`/`resolve_config`, not a copy. `C4-verify` is green-only, but the brief declared that in advance for a crate this patch creates; it is not a hidden red leg.
- **The tests fail when the code is wrong.** I rebuilt the crate in scratch and applied hand-written mutations that `cargo mutants` does not generate. Each one turned the suite red:
  - Refusing only *known* flag names in a value slot (`crates/validate/src/args.rs:136`): caught by `a_flag_in_any_value_slot_is_refused_naming_both_flags`.
  - Silently skipping an unknown flag and its value: caught by `an_unrecognised_flag_is_refused_by_name`.
  - Swallowing a flush error (`crates/validate/src/lib.rs:103-105`): caught by `a_failed_write_or_flush_of_the_echo_exits_non_zero`.
  - Deleting the empty-value check (`args.rs:142`): caught by `each_empty_value_is_refused_by_name`.

  This adds to the frozen `C5-mutants` result (0 missed of 42).
- **Parser edge cases on the real binary are all refused by name, with exit 2:** `--bucket=x`, a bare `--`, `--bucket --typo`, `--bucket --duration 2m`, a repeated flag, and a stray positional.
- **Credential matrix:** all four brief directions pass. Half-pairs are refused in both directions (`access_keys.rs:153`, `:159`), an empty value counts as unset (`:176`), and the test checks the *resolved secret* and the `Debug` redaction (`cli_surface.rs:259-265`). Together these close the v1 carry-forward findings. The citations to `crates/server/src/cli.rs:2166/2168/2173/2550-2553` and `xtask/src/main.rs:1559` match this base.

## Findings

- **Unwarranted claim (low severity; not worth a rebuild on its own): `crates/validate/src/lib.rs:101-102`** says a failed write "is a failed run, not a success with nothing printed". That is false for the production binary when stdout is closed. `wyrd-validate <all ten flags> >&-` exits **0** and prints nothing, because Rust's `std::io::stdout` treats a closed fd 1 (EBADF) as a successful write. I reproduced it: `/dev/full` → exit 1 and a broken pipe → exit 1 behave as claimed, but a closed stdout → exit 0. The test `cli_surface.rs:537` only exercises an injected writer, so it cannot see this. Fix options: narrow the comment, or check `std::io::stdout().as_fd().try_clone_to_owned()` in `main.rs` (safe code). Impact is small, because later slices write the verdict to `--out`, not stdout.
- **Untested documented contract (low severity): `crates/validate/src/lib.rs:63`** says "Argument errors are reported before credential errors". Swapping the two `?` lines in `resolve_config` (`lib.rs:68-69`) still passes 19/19. No test combines bad arguments with no credentials. This is not a brief criterion; I note it only because the doc makes the promise.
- NEEDS-HUMAN [human] — **`AWS_SESSION_TOKEN` is silently ignored: `crates/validate/src/access_keys.rs:139-166`.** Concrete case: a shell holding temporary AWS STS credentials (`AWS_ACCESS_KEY_ID=ASIA…`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`) plus a deliberate `WYRD_S3_*` pair. It resolves to the AWS identity, exits 0, and the echo never mentions the token (reproduced). This is the same mistake the patch refuses for half-pairs ("an operator who set `AWS_ACCESS_KEY_ID` meant to use it", `access_keys.rs:93-95`): an STS pair cannot sign without its token, and nothing in the repo handles session tokens. The brief's "BOTH present → AWS wins" rule is settled. What is still open is whether a set `AWS_SESSION_TOKEN` should be refused or flagged in the echo now, or left to #741's signer. That is a scope call.
- NEEDS-HUMAN [human] — **Scope check against the settled "encoding cases are OUT" decision: `crates/validate/src/access_keys.rs:103`, `:178`, `crates/validate/tests/cli_surface.rs:358-425`.** The patch adds non-UTF-8 handling for *environment variables*: a `NotUnicode` error variant, two tests, and a sentence in the architecture doc. The brief bans "Non-UTF-8 arguments", and env vars are not arguments. The addition is also defensible: `std::env::var(..).ok()` would treat an unreadable AWS id as unset and sign as Wyrd. Meanwhile non-UTF-8 *argv* still panics at `main.rs:9` (`std::env::args()`), which is still a non-zero refusal and is correctly left alone per the brief. I judge the addition reasonable. Only the maintainer can say whether it re-opens what was closed.

## Not raised (per the brief and rubric)

- Control characters or newlines in echoed values, and non-UTF-8 argv: the brief puts these explicitly OUT.
- Value typing (`--duration 7` with no unit, a whitespace-only `--run-id " "`): the brief defers this to the slices that consume the values.

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found within this diff.

Reviewed argument parsing (`crates/validate/src/args.rs:122`), credential selection and redaction (`crates/validate/src/access_keys.rs:78`, `crates/validate/src/access_keys.rs:139`), output failure handling (`crates/validate/src/lib.rs:93`), and their integration tests against the read-only target. The prior implementation findings are addressed.

Validation evidence: the frozen logs record `cargo xtask ci` passing, all 19 CLI tests passing, 223/223 instrumentable changed lines covered, and 42 mutants tested (26 caught, 16 unviable). Tests were not rerun in this read-only review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the observed operator experience for this skeleton milestone — exit 0 certifies configuration resolution only, while actual S3 validation remains future work in #741/#743 (`reviewer-cli.log:18`, `crates/validate/src/lib.rs:8`).
- [x] **`AWS_SESSION_TOKEN` is silently ignored: `crates/validate/src/access_keys.rs:139-166`.** Concrete case: a shell holding temporary AWS STS credentials (`AWS_ACCESS_KEY_ID=ASIA…`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`) plus a deliberate `WYRD_S3_*` pair. It resolves to the AWS identity, exits 0, and the echo never mentions the token (reproduced). This is the same mistake the patch refuses for half-pairs ("an operator who set `AWS_ACCESS_KEY_ID` meant to use it", `access_keys.rs:93-95`): an STS pair cannot sign without its token, and nothing in the repo handles session tokens. The brief's "BOTH present → AWS wins" rule is settled. What is still open is whether a set `AWS_SESSION_TOKEN` should be refused or flagged in the echo now, or left to #741's signer. That is a scope call.
- [x] **Scope check against the settled "encoding cases are OUT" decision: `crates/validate/src/access_keys.rs:103`, `:178`, `crates/validate/tests/cli_surface.rs:358-425`.** The patch adds non-UTF-8 handling for *environment variables*: a `NotUnicode` error variant, two tests, and a sentence in the architecture doc. The brief bans "Non-UTF-8 arguments", and env vars are not arguments. The addition is also defensible: `std::env::var(..).ok()` would treat an unreadable AWS id as unset and sign as Wyrd. Meanwhile non-UTF-8 *argv* still panics at `main.rs:9` (`std::env::args()`), which is still a non-zero refusal and is correctly left alone per the brief. I judge the addition reasonable. Only the maintainer can say whether it re-opens what was closed.
- [x] **The T4 blocking finding is real, and a ruling is needed on scope.** `crates/validate/src/main.rs:10` (`std::env::var(name).ok()`) treats a variable that is set but not valid UTF-8 as unset. Reproduced: `env -i AWS_ACCESS_KEY_ID=$'\xff' AWS_SECRET_ACCESS_KEY=$'\xfe' WYRD_S3_ACCESS_KEY=wid WYRD_S3_SECRET_KEY=wsec wyrd-validate <all ten flags>` exits 0 and reports `access-key-id = wid`, source Wyrd. That is the silent identity switch the patch's own doc says it prevents (`crates/validate/src/access_keys.rs:92`), and it matches the rubric's "never silent skip" class. A second case: a valid AWS id with a non-UTF-8 secret is refused, but the message says `AWS_SECRET_ACCESS_KEY is set but ... is not`, and the secret *is* set. The brief puts "the encoding cases" out of scope, but names only non-UTF-8 argv and control characters in output, not environment values. A human should decide: record-reject this under that scope decision (which clears the gating T4 row), or take a small fix at `main.rs:10` (for example, refuse `VarError::NotUnicode` by name). It is unlikely in practice, since real AWS keys are ASCII.
- [x] **An empty value counts as present.** `--run-id ""` (for example `--run-id "$RUN_ID"` with `RUN_ID` unset) exits 0 and echoes `  --run-id = `. The same happens for every flag; `crates/validate/src/args.rs:136` stores whatever it is given, and the `Missing` check at `args.rs:163` only catches flags that never appeared. The credential side treats empty as unset (`access_keys.rs:127`), so the two halves disagree. Proposal 0017 §5 makes the run id the key prefix that keeps "delete only mine" safe, and an empty prefix covers the whole bucket. `args.rs:33` explicitly leaves value checks to later slices, so leaving this is defensible. Needs a call: refuse empty values now (a one-line check) or record it against

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- #774 → #741: `AWS_SESSION_TOKEN` is silently ignored by credential resolution (`crates/validate/src/access_keys.rs:139-166`); handle it in #741's signer.
