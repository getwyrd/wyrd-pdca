# Result — issue 742 / dist-ship-wyrd-validate-two-binary-tarball

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) The distribution pipeline is single-binary end to end, in
  five places, verified on `main` at `65ca4fd`: the Dockerfile builds `--bin wyrd` (`:66`),
  calls its runtime stage "a minimal image hosting just the `wyrd` binary" (`:68`) and
  copies one path (`:122`); `dist::obtain_binary` extracts that one path
  (`IMAGE_BINARY_PATH = "/usr/local/bin/wyrd"`, `xtask/src/dist.rs:40`) and `assemble`
  copies it to one destination (`:559-564`); `deploy/dist/install.sh` installs
  `$HERE/bin/wyrd` and removes `$BINDIR/wyrd`; `deploy/dist/README.md` describes one
  binary; and `.github/workflows/release.yml` smoke-tests `/usr/local/bin/wyrd` (`:75`) and
  asserts its absence after uninstall (`:85`). So a validation tool an operator is meant to
  run against their own deployment has no way to reach them.
- Success criterion: BINDING (demonstrable by C4-verify at Check, container-free, inside
  `cargo xtask ci`): the shipped-binary set becomes DATA — one pure function in
  `xtask::dist` mapping each binary's in-image path to its tarball destination — the Rust
  side of the pipeline READS that table, and every other stage's spelling of the binary set
  is PINNED to it by an assertion. A new test asserts, by reading the real repo files, that
  every one of the five places above names **both** binaries:
  1. the Dockerfile builds each binary and copies each into the runtime stage;
  2. `install.sh` installs each from `bin/<name>` and removes each on `--uninstall`;
  3. the release workflow's smoke step exercises each installed path and asserts each is
     gone after uninstall;
  4. `README.md` distinguishes the two roles of the two binaries;
  5. those four assertions are **one checker over a binary set, not written out per binary**,
     and that checker runs over **both** sets: the red file's local expected set AND the
     production table. Mind the split the gate forces (see Falsifiability): the red-earning
     file may not name a net-new symbol, so it holds the checker as a plain function of a
     binary set (returning the list of files that disagree) and calls it on its OWN local
     expected set. The existing `dist_templates.rs` then runs **that same checker** on
     `xtask::dist`'s production table — include the red file as a module
     (`#[path = "dist_two_binary_layout.rs"] mod layout;`), so the checker exists once; do NOT
     put it in a new helper file under `xtask/tests/`, because the C4 classifier treats every
     added `tests/*.rs` as a discriminator test (`engine/scripts/run-verify.sh:144`). Keep the
     red file module-includable (no crate-only inner attributes). `dist_templates.rs` also
     pins the local set EQUAL to the production table. The diagnostic this buys, stated
     exactly: add a third entry to the production table and change nothing else, and
     `dist_templates.rs` fails naming **every pipeline file that lacks the new binary**, while
     the equality assertion fails telling you to update the red file's local set. Two files,
     one checker, one declaration — the table stays the source and the text test still
     compiles against a reverted tree. This is the honest version of
     "single source": Docker, shell, YAML and Markdown cannot read a Rust function, so what
     the slice buys is *declared once, duplication checked by the gate* — the same shape the
     repo already uses for the FDB pin (`xtask/tests/fdb_image.rs` pins one `ARG FDB_VERSION`
     across three files). Adding a third binary stays a multi-file edit; what changes is that
     the gate names every file you missed instead of the release doing it.
  6. AND one observation beyond file text, so the criterion is not purely lexical: the
     binary-staging step is extracted as a **`pub`** callable that takes the table plus a
     source directory and populates a staging tree (today it is four hard-coded lines inside
     the private `assemble`, `xtask/src/dist.rs:559-564`, unreachable from any integration
     test). A test runs it over a tempdir holding two dummy "binaries" with **different
     contents** (e.g. `roles-binary` and `validator-binary`) and asserts the staging tree ends
     up with `bin/wyrd` and `bin/wyrd-validate`, both `0755`, and that **each destination is
     byte-for-byte equal to its own source** — so copying one source to both destinations, or
     swapping them, fails. The mapping that feeds staging is checked too: where `obtain_binary`
     extracts each in-image path on the packaging host is computed by a pure function of the
     table, and a test asserts those host paths are pairwise distinct (no two entries extract
     onto one file), as are the table's in-image paths and its tarball destinations. That is
     the real staging code, exercised, with no container. These assertions name new API, so
     they live in `dist_templates.rs`, not in the red-earning file.
  DEFERRED and named as such (see Verification posture): "a real tarball contains both
  binaries, and `install.sh` places both on a real host" — the release workflow's to prove.
  Not a choice: nothing in `cargo xtask ci` can build a tarball (that needs Docker and a
  network, `xtask/src/dist.rs:26-28`), and `install.sh` cannot be executed by a test at all —
  it refuses to run as non-root (`deploy/dist/install.sh:90`), writes `/etc/wyrd`, creates
  users and installs units. Verified by reading it, not assumed.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) the binary set becomes a pure data table in `xtask::dist` — in-image path →
  tarball destination — read by `obtain_binary` and `assemble`; (b) the Dockerfile builds
  and copies both binaries; (c) `dist` extracts both out of the image and stages both under
  `bin/`; (d) `install.sh` installs and uninstalls both; (e) `deploy/dist/README.md`
  distinguishes the roles binary from the validation tool; (f) `release.yml`'s smoke step
  covers both; (g) the container-free layout contract — the new red-earning text test (its
  assertions ITERATED from the table, per criterion 5) plus the pure-function and
  binary-staging assertions in `dist_templates.rs` (criterion 6); (h) the `--host` branch
  builds BOTH binaries — `cargo build --release --locked --bin wyrd --bin wyrd-validate
  --features …` — and returns both paths, so a `--host` tarball has the same contents as an
  image-built one. It does NOT refuse: removing the local-build path
  (`xtask/src/dist.rs:412-430`) is not this slice's to do, and a `--host` tarball silently
  missing the validator is the very bug class being removed. Assert it as a pure function:
  the cargo argv for the host branch is built from the table and named-both-bins is a
  container-free assertion like the rest.
  **/ out of scope:** making `wyrd-validate` a `wyrd` subcommand (it would share the roles'
  dependency closure and make #740's blackbox lint meaningless — the issue says so, and
  proposal 0017 §2 and §9 say so twice); a systemd unit for the validator (it is not a
  role — it is run by hand or by a launcher, and `install.sh`'s `ROLES` list must NOT grow
  an entry); a `/etc/wyrd/validate.env`; registry publication of the image (a named
  follow-up slice — the release still ships the OCI archive as a signed blob, not a `ghcr`
  push); multi-arch; the `tikv` flavor; changing what `wyrd-validate` DOES.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (2 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 77 instrumentable changed lines executed (below the 80% floor); 77 of 165 changed lines were i
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.25s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #742 — ship `wyrd-validate` beside `wyrd` throughout distribution: two workflow defects remain despite passing container-free checks.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief distinguishes the binding layout/staging contract from deferred real installation and identifies the packaging decision owed at sign-off; `brief.md:45`, `brief.md:238`, `brief.md:331`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the added test produced the intended missing-validator assertion, not a compile failure; `xtask/tests/dist_two_binary_layout.rs:465`, `pdca-reviewer-742-checks/red.log:24`. |
| C3 Change | PASS | The change covers the planned distribution surfaces and living deployment description without expanding the validator's behavior; `xtask/src/dist.rs:77`, `deploy/dist/install.sh:142`, `docs/design/architecture/07-deployment-view.md:42`. |
| C4 Verification (red→green) | PASS | Restoring the identical patch passed 29 template/staging tests plus 2 layout tests and `dist --check`; this verifies the declared container-free contract only, with workflow correctness failures recorded below; `pdca-reviewer-742-checks/green.log:38`, `pdca-reviewer-742-checks/dist-check.log:3`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Repair the smoke contract's false assurance: it accepts checks that the real shell never executes because it discards comment lines before considering outer-shell quoting; add a regression for the actual command boundary; `xtask/tests/dist_two_binary_layout.rs:268`, `.github/workflows/release.yml:76`. |
| T1 Structure | PASS | A shared binary table drives Rust extraction, host arguments and staging, while one checker serves both expected sets; this addresses independently drifting binary declarations without a capability probe; `xtask/src/dist.rs:385`, `xtask/src/dist.rs:630`, `xtask/src/dist.rs:684`, `xtask/tests/dist_templates.rs:509`. |
| T2 Shape | FAIL | Validator-only changes cannot trigger the newly added image smoke, leaving its runtime contract unchecked on precisely those PRs; `.github/workflows/fdb-image.yml:18`, `.github/workflows/fdb-image.yml:97`, `pdca-reviewer-742-checks/fdb-path-filter.log:2`. |
| T3 Runtime | FAIL | Unescaped comment quotes truncate the container's command before either binary smoke or uninstall verification, allowing the release step to succeed without them; `.github/workflows/release.yml:76`, `pdca-reviewer-742-checks/release-argv.json:5`. |
| T4 Contribution | FAIL | The required multi-pass review still has two distinct unresolved defects, independently confirmed here; its three quoting reports are duplicates; publication-artifact auditing is N/A until publish, not a missing-evidence escalation; `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN | Confirm option A and accept packaging before working scenarios/endurance: a tag can publish the configuration-only validator, so release restraint remains a maintainer decision; `brief.md:331`, `brief.md:472`, `crates/validate/src/lib.rs:7`, `.github/workflows/release.yml:23`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept deferred real-artifact proof or require a release workflow dispatch after the workflow fixes: the image build and clean-host installation were not exercised, so current proof rests on file checks and staging distinct dummy payloads; `brief.md:255`, `xtask/tests/dist_templates.rs:625`, `.github/workflows/release.yml:54`. |

Source citations are relative to `$PDCA_TARGET`; brief, frozen-log and reviewer-evidence citations are relative to this review directory. The supplied target was readable, the patch reverse-apply check passed, and the stash/pop rerun restored its original diff and status. No implementation changes were made.

**Two fixes are needed.** The release quoting defect defeats the deferred verification; the missing path filter prevents the image smoke from observing validator-only changes.

1. **P1 — Preserve the complete container script.** At `.github/workflows/release.yml:76`, the double quotes around `refused correctly` occur inside the outer double-quoted `sh -eu -c` argument opened at line 65. Bash produces **two arguments after `-c`**. The executable script ends with `# the if-branch alone would read as refused`; the remainder, including both binary invocations, FDB installation, idempotence checks and uninstall assertions, becomes the shell's positional `$0`. Capturing the real workflow command's arguments reproduced this without Docker or host installation (`pdca-reviewer-742-checks/release-argv.json:2`). Removing only those comment quotes in an in-memory control produced one script containing uninstall. Fix the quoting and make the checker reject this case: its current comment filtering passes both the real defective workflow and the supposedly clean fixture (`xtask/tests/dist_two_binary_layout.rs:270`, `xtask/tests/dist_templates.rs:752`). This is a shell-argument reproduction, not a simulated successful image/install run.

2. **P2 — Trigger the image smoke on validator changes.** The new smoke at `.github/workflows/fdb-image.yml:97` has no corresponding `crates/validate/**` PR path filter. Comparing actual filters with `crates/validate/src/main.rs`, `crates/validate/src/cli.rs` and `crates/validate/Cargo.toml` yielded no matches (`pdca-reviewer-742-checks/fdb-path-filter.log:1`). A validator CLI/usage change can therefore miss the image runtime assertion. Add the validator path and pin it in the workflow contract. This is within the standing workflow-review rule in `AGENTS.md` and the workflow surface this patch adds.

The gate evidence supports the following narrower conclusions; frozen outcomes have not been rewritten.

| Gate | Verdict | Basis |
|------|---------|-------|
| C4-ci | PASS | Frozen output records formatting, workspace compilation/tests, dependency checks, conformance and DST success, ending in `all checks passed`; `gate-logs/C4-ci.log:3364`, `gate-logs/C4-ci.log:3968`. Independently reran all xtask tests, xtask clippy, workspace formatting, typos, docs lint and the blackbox guard; all passed (`pdca-reviewer-742-checks/evidence-summary.log:4`). The full instance-scoped CI wrapper was not rerun. |
| C4-verify | PASS | Independently reproduced the same compiled assertion failure before the fix and green results afterward; `gate-logs/C4-verify.log:25`, `pdca-reviewer-742-checks/red.log:30`, `pdca-reviewer-742-checks/green.log:46`. |
| C4-diff-cov | FAIL | Frozen measurement is genuinely 0/77 changed instrumentable lines, but selects only the intentionally text-only test (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:112`). Independent coverage reproduced zero execution; adding the brief-mandated `dist_templates` executes staging twice and the table/argv helpers (`pdca-reviewer-742-checks/evidence-summary.log:14`). This selection gap does not establish absent staging tests; container orchestration remains unexercised. |
| C5-mutants | PASS | Frozen run tested no mutants because its unmutated copy failed `git ls-files`, a harness caveat (`gate-logs/C5-mutants.log:450`). Independent rerun preserving VCS metadata passed its baseline: 3 caught, 13 unviable, no survivors (`pdca-reviewer-742-checks/mutation-rerun.log:5`). The unviable mutations fail warnings-as-errors or a missing `Default` implementation; they are not claimed as test kills. |
| T4-batch-review | FAIL | Its four entries reduce to the two findings above, both independently confirmed; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | Contribution artifacts are drafted after Check; the substantive audit must rerun at publish, as the deferred log explicitly states; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS | Frozen output shows both requested feature clippy invocations completed successfully; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`. This unrelated feature compile was adjudicated from its log, not rerun or treated as image-build evidence. |

**Prior art was checked by affected file path.** The disposable target has only a synthetic base commit, so I queried upstream merged history for all nine affected paths and compared file lists for all 356 closed PRs, explicitly paginating the two large PRs. No closed-unmerged PR touched these paths; the relevant predecessors are the merged single-binary distribution PR #572 and image PR #497. Evidence: `pdca-reviewer-742-checks/evidence-summary.log:21`, `pdca-reviewer-742-checks/prior-art-history.json:1`, `pdca-reviewer-742-checks/prior-art-closed-summary.json:1`. No `INTEGRATION.md` was present in the supplied target; the provided rubric and target root `AGENTS.md` were applied.

**Real release validation is still owed.** After fixing both workflow defects, the maintainer can run `gh workflow run release.yml --repo getwyrd/wyrd --ref <branch-containing-the-fixes>` and inspect the distribution-build and Bookworm installer-smoke steps. Confirm both tarball binaries, validator usage before FDB-client installation, roles-binary usage afterward, preserved operator configuration on reinstall, and absence of both binaries after uninstall. Dispatch builds without publishing because publication requires a tag (`.github/workflows/release.yml:118`). No container build, root installation, workflow dispatch or publication was performed in this review.

### Advisory — adversary

# Adversarial review — issue 742 (two-binary tarball)

Bottom line: the red→green proof is genuine, but the patch breaks the release smoke step it
claims to extend, and the new layout checker reports that broken step as green. Bash was
used to reproduce the break directly; `cargo test` reproduced the false green.

- NEEDS-HUMAN [impl] — `.github/workflows/release.yml:76`: the comment `… would read as "refused correctly").` puts bare double quotes inside the `sh -eu -c "…"` string that opens at `:65`. The runner's bash closes the string at the first `"`. Reproduced with bash, using a `docker` stub that captures argv: `sh -c` receives an **11-line script that ends at that comment**. `sh -n` parses it cleanly, and after `./install.sh` (`:70`) and `systemd-analyze verify` (`:71`) it exits 0. Lines `:77-98` become `$0` and **never run**. That means the validator smoke (`:77-80`), the FDB client install and the existing `wyrd` usage smoke (`:83-88`), the idempotence check (`:90-92`) and every uninstall assertion (`:94-98`) are skipped, and the step still goes green on a `v*` tag. This breaks #570's existing checks too, not only the new ones. It also makes a sign-off item rest on a false premise: the brief defers "install.sh places both / uninstall removes both" to this exact step (`brief.md:238-244`), and today the step never reaches those lines. Fix: use `'refused correctly'` (or drop the quotes). With that change, the captured script is the full 34 lines, it passes `sh -n`, and it includes the validator invocation and `./install.sh --uninstall`. The T4 review already blocks on this. Nothing else in CI would catch it: no actionlint, and `shellcheck` covers only `install.sh` (`release.yml:43`).

- NEEDS-HUMAN [impl] — `xtask/tests/dist_two_binary_layout.rs:251-270` (`release_smoke_disagreements`) reads each YAML line of the step as if it were a shell line. It does not model the outer `sh -c "…"` quoting. On the tree above, where no smoke line after `:76` runs, all 31 tests pass (`cargo test -p xtask --test dist_two_binary_layout --test dist_templates`, re-run in scratch). So criterion 3 ("the smoke step exercises each installed path and asserts each is gone after uninstall") is green for the wrong reason. The C4-verify green leg proves less about `release.yml` than its PASS line suggests, and `docs/design/architecture/07-deployment-view.md` overstates what the gate catches. Fix: find the `sh … -c "` opener in the step, and treat any further unescaped `"` before the closing `"` line as the end of the script, so the existing "never runs … before `./install.sh --uninstall`" message fires. Or report the stray quote directly. Then add a planted case to `xtask/tests/dist_templates.rs:830` that inserts a comment line with `"quoted"` words before the invocation and expects the checker to name `wyrd-validate`.

- NEEDS-HUMAN [impl] — `.github/workflows/fdb-image.yml:18-43`: the path filter has no `crates/validate/**` entry, but the new step at `:97-104` greps `usage: wyrd-validate`, and that text comes from `crates/validate/src/args.rs:184`. The file's own rule at `:33-37` lists `crates/server/**` for exactly this reason ("the usage line the smoke greps"). Concrete case: a PR that changes `args.rs:184` (say, to `Usage:`) does not trigger fdb-image. `cargo xtask ci` never runs the image, so the break shows up only at the release smoke, and only after the bug above is fixed. This is the rubric's "Workflow edits: re-check path filters" class. The fix is one added line.

- Evidence re-run, and it holds. With the production files reverted (git stash, new test kept), `every_pipeline_file_names_every_expected_binary` fails and names all 8 gaps across the Dockerfile, install.sh, release.yml and README. With the patch it passes. The test reads the real repo files and uses no new API, so the red is earned, not declared.

- The two failing advisory gates say nothing against this fix. C4-diff-cov is 0% because it measures only `--test dist_two_binary_layout`, which by design calls no `dist.rs` code. The new `dist.rs` functions are run by `dist_templates.rs` (29 tests pass). C5-mutants failed because of the environment: `repo_hygiene_guards.rs:137` runs `git ls-files`, which fails in cargo-mutants' non-git temp copy, so no mutants were tested. One gap remains, and the brief did not ask for more: the private callers at `xtask/src/dist.rs:630` (the `docker cp` loop) and `:684` (`stage_binaries(&shipped_binaries(), …)`) are reached by no test. A `[..1]` slice there would pass every test. At release time, `./install.sh` (`release.yml:70`) would still fail under `set -eu` on a missing `bin/wyrd-validate`, and that line runs even with the quoting bug.

- Things I tried to break and could not:
  - **Two-package build.** `cargo tree --locked -p wyrd-server -p wyrd-validate --features fdb,etcd` resolves, and a bogus feature is rejected for that package pair. So one `cargo build --bin wyrd --bin wyrd-validate --features …` line is valid (`deploy/docker/wyrd/Dockerfile:78`). I did not attempt a full image build: no Docker, and the brief rules it out.
  - **Usage output.** With no arguments, the validator prints `usage: wyrd-validate …` to stderr and exits non-zero (`crates/validate/src/lib.rs:88`, `args.rs:172-173`, missing `--endpoint` etc.). Both the stderr-only grep in fdb-image and the `2>&1` grep in release match it.
  - **`install.sh --help`.** `sed -n '2,17p'` prints exactly the new header. The old `2,16p` also printed `set -eu`, so this change is a small fix.
  - **New `target/dist/extracted/` directory.** It does not leak into release artifacts. `SHA256SUMS` hashes an explicit file list (`dist.rs:710-720`), and the attest and upload steps glob `*.tar.gz` (`release.yml:113-126`).

### Advisory — code-review

- NEEDS-HUMAN [impl] — **[P1] Release smoke silently stops before either binary is exercised.** `.github/workflows/release.yml:76`: the unescaped double quotes around `"refused correctly"` split the enclosing double-quoted `sh -c` argument. Capturing Docker's arguments from the actual target script confirms that the command ends at `as refused`; everything afterward becomes an extra argument (`$0`), including both usage checks, FDB installation, idempotence checks, and uninstall verification. The step can therefore succeed without running them. Escape/remove these quotes and add a regression checking the actual shell argument: the current layout checker passes this broken workflow.

- NEEDS-HUMAN [impl] — **[P2] Validator-only changes never trigger the new image smoke.** `.github/workflows/fdb-image.yml:100`: the added validator invocation is behind `pull_request.paths` (`.github/workflows/fdb-image.yml:18`), which omits `crates/validate/**`. A PR touching only the newly shipped validator therefore skips its image build/runtime check; the container-free gate cannot establish that the resulting binary runs in bookworm. Add the validator directory to the filters and their contract test.

- NEEDS-HUMAN [impl] — **[P3] The new extraction cleanup can leak the throwaway container.** `xtask/src/dist.rs:625`: `remove_dir_all` runs after `docker create`, but its error propagates immediately through `?`, bypassing the container removal at `xtask/src/dist.rs:637`. If stale extraction contents cannot be removed, every retry leaves another created container behind. Prepare the extraction directory before creating the container, or put all subsequent fallible work under guaranteed best-effort container cleanup.

No additional actionable reuse, simplification, or efficiency findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Repair the smoke contract's false assurance: it accepts checks that the real shell never executes because it discards comment lines before considering outer-shell quoting; add a regression for the actual command boundary; `xtask/tests/dist_two_binary_layout.rs:268`, `.github/workflows/release.yml:76`.
- [ ] T5 Judgment — Confirm option A and accept packaging before working scenarios/endurance: a tag can publish the configuration-only validator, so release restraint remains a maintainer decision; `brief.md:331`, `brief.md:472`, `crates/validate/src/lib.rs:7`, `.github/workflows/release.yml:23`.
- [ ] Validation — fitness-to-purpose — Accept deferred real-artifact proof or require a release workflow dispatch after the workflow fixes: the image build and clean-host installation were not exercised, so current proof rests on file checks and staging distinct dummy payloads; `brief.md:255`, `xtask/tests/dist_templates.rs:625`, `.github/workflows/release.yml:54`.
- [ ] `.github/workflows/release.yml:76`: the comment `… would read as "refused correctly").` puts bare double quotes inside the `sh -eu -c "…"` string that opens at `:65`. The runner's bash closes the string at the first `"`. Reproduced with bash, using a `docker` stub that captures argv: `sh -c` receives an **11-line script that ends at that comment**. `sh -n` parses it cleanly, and after `./install.sh` (`:70`) and `systemd-analyze verify` (`:71`) it exits 0. Lines `:77-98` become `$0` and **never run**. That means the validator smoke (`:77-80`), the FDB client install and the existing `wyrd` usage smoke (`:83-88`), the idempotence check (`:90-92`) and every uninstall assertion (`:94-98`) are skipped, and the step still goes green on a `v*` tag. This breaks #570's existing checks too, not only the new ones. It also makes a sign-off item rest on a false premise: the brief defers "install.sh places both / uninstall removes both" to this exact step (`brief.md:238-244`), and today the step never reaches those lines. Fix: use `'refused correctly'` (or drop the quotes). With that change, the captured script is the full 34 lines, it passes `sh -n`, and it includes the validator invocation and `./install.sh --uninstall`. The T4 review already blocks on this. Nothing else in CI would catch it: no actionlint, and `shellcheck` covers only `install.sh` (`release.yml:43`).
- [ ] `xtask/tests/dist_two_binary_layout.rs:251-270` (`release_smoke_disagreements`) reads each YAML line of the step as if it were a shell line. It does not model the outer `sh -c "…"` quoting. On the tree above, where no smoke line after `:76` runs, all 31 tests pass (`cargo test -p xtask --test dist_two_binary_layout --test dist_templates`, re-run in scratch). So criterion 3 ("the smoke step exercises each installed path and asserts each is gone after uninstall") is green for the wrong reason. The C4-verify green leg proves less about `release.yml` than its PASS line suggests, and `docs/design/architecture/07-deployment-view.md` overstates what the gate catches. Fix: find the `sh … -c "` opener in the step, and treat any further unescaped `"` before the closing `"` line as the end of the script, so the existing "never runs … before `./install.sh --uninstall`" message fires. Or report the stray quote directly. Then add a planted case to `xtask/tests/dist_templates.rs:830` that inserts a comment line with `"quoted"` words before the invocation and expects the checker to name `wyrd-validate`.
- [ ] `.github/workflows/fdb-image.yml:18-43`: the path filter has no `crates/validate/**` entry, but the new step at `:97-104` greps `usage: wyrd-validate`, and that text comes from `crates/validate/src/args.rs:184`. The file's own rule at `:33-37` lists `crates/server/**` for exactly this reason ("the usage line the smoke greps"). Concrete case: a PR that changes `args.rs:184` (say, to `Usage:`) does not trigger fdb-image. `cargo xtask ci` never runs the image, so the break shows up only at the release smoke, and only after the bug above is fixed. This is the rubric's "Workflow edits: re-check path filters" class. The fix is one added line.
- [ ] **[P1] Release smoke silently stops before either binary is exercised.** `.github/workflows/release.yml:76`: the unescaped double quotes around `"refused correctly"` split the enclosing double-quoted `sh -c` argument. Capturing Docker's arguments from the actual target script confirms that the command ends at `as refused`; everything afterward becomes an extra argument (`$0`), including both usage checks, FDB installation, idempotence checks, and uninstall verification. The step can therefore succeed without running them. Escape/remove these quotes and add a regression checking the actual shell argument: the current layout checker passes this broken workflow.
- [ ] **[P2] Validator-only changes never trigger the new image smoke.** `.github/workflows/fdb-image.yml:100`: the added validator invocation is behind `pull_request.paths` (`.github/workflows/fdb-image.yml:18`), which omits `crates/validate/**`. A PR touching only the newly shipped validator therefore skips its image build/runtime check; the container-free gate cannot establish that the resulting binary runs in bookworm. Add the validator directory to the filters and their contract test.
- [ ] **[P3] The new extraction cleanup can leak the throwaway container.** `xtask/src/dist.rs:625`: `remove_dir_all` runs after `docker create`, but its error propagates immediately through `?`, bypassing the container removal at `xtask/src/dist.rs:637`. If stale extraction contents cannot be removed, every retry leaves another created container behind. Prepare the extraction directory before creating the container, or put all subsequent fallible work under guaranteed best-effort container cleanup.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- [ ] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [ ] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [ ] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.
- [ ] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [ ] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.
- [ ] T5 Judgment — Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).
- [ ] T5 Judgment — Accept landing packaging before functional scenarios and retain the human no-early-tag checkpoint, or return sequencing to Plan—the exercised CLI reports that nothing was validated, while any `v*` tag can publish it (crates/validate/src/lib.rs:120; .github/workflows/release.yml:23; .github/workflows/release.yml:115; brief.md:472).
- [ ] **Addendum to the pre-declared T3 deferral: half of the "release-only" evidence will show up on this PR for free.** The brief says the image half can only be observed by a `v*` tag or a `workflow_dispatch` run (`brief.md:255-263`). `.github/workflows/fdb-image.yml:22` path-filters on `deploy/docker/wyrd/**`, which this patch edits, so that job will run on the PR. It runs `docker build --build-arg FEATURES=fdb,etcd` against the new Dockerfile (`:72-78`). A green run proves that `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` and both `COPY` lines (`deploy/docker/wyrd/Dockerfile:72`, `:130-131`) work. It does not run `wyrd-validate` inside the image (its smoke is `wyrd` usage plus `fdbcli`, `:83-95`). It also does not observe extraction, the tarball, or install. At sign-off, check that job's result before accepting the deferral. Whether to add a one-line `docker run --entrypoint wyrd-validate wyrd:fdb` smoke there is a scope call; the brief did not ask for it.
- [ ] T5 Judgment — Accept landing packaging before #743 and retaining the maintainer's release checkpoint — the executable currently reports that it validates nothing, while a tag can publish it without a scenario/endurance gate (`crates/validate/src/lib.rs:118`, `.github/workflows/release.yml:115`, `pdca-reviewer-742-evidence/validator-config-run.log:15`).
- [ ] `deploy/dist/README.md:17` tells operators, in the present tense, that `wyrd-validate` "checks that your hardware and configuration are sound by driving a running deployment". On this base the binary does neither. It echoes its configuration and says so: `crates/validate/src/lib.rs:122` prints "configuration resolved; no requests were issued and nothing was validated". So a `v*` tag cut before #743 ships a README that promises a check the shipped binary does not do. The brief's Alternative D already makes "packaging lands before the tool works" a §9 sign-off item, so this is not a new decision. The point is that the README text is part of what the maintainer accepts there. If that is not wanted, the fix is one sentence, e.g. "will check … once its scenarios land (#743); in this release it resolves and prints its configuration only". That is a judgment about the stub window, not a build defect.

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
- Iteration delta (if iterating): Rejected: the patch breaks the release smoke step it extends, and the new layout test passes on the broken file. Fix in place, same plan: 1. .github/workflows/release.yml (~:76): the comment `would read as "refused correctly"` has bare double quotes inside the outer `sh -eu -c "..."` string. They end the script early, so every check after them (validator smoke, wyrd usage smoke, reinstall check, all uninstall assertions) never runs and the step still goes green. Use single quotes or drop the quotes. Do the same in the fdb-image.yml comment for consistency. 2. xtask/tests/dist_two_binary_layout.rs release_smoke_disagreements: model the outer `sh -c "` boundary. An unescaped `"` before the closing line must count as the end of the script, so the checker reports every check after it as never run. Add a planted regression in dist_templates.rs: a comment line with "quoted" words placed before the validator invocation must make the checker name wyrd-validate. 3. .github/workflows/fdb-image.yml: add `crates/validate/**` to the pull_request path filters, since the new smoke greps the validator's usage line. Pin it in the workflow contract test. 4. xtask/src/dist.rs (~:625): prepare and clean the extraction directory BEFORE `docker create`, or put every fallible step after create under guaranteed best-effort `docker rm`, so a failed cleanup can't leak containers. Maintainer questions still open (not decided this round, do not assume an answer): reconfirm option A / packaging before #743 with no early v* tag; README :17 present-tense claim about what wyrd-validate checks; whether a manual release.yml workflow_dispatch is required as real-artifact proof.
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
