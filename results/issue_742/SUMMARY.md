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
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 81 instrumentable changed lines executed (below the 80% floor); 81 of 179 changed lines were i
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.13s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #742: shipping `wyrd` and `wyrd-validate` through one checked distribution pipeline has no new implementation defect; the binding red→green is reproduced, with real-artifact evidence still owed at the agreed milestones.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding container-free contract is falsifiable, and the latest sign-off settles option A, sequencing, wording, and verification timing; no Plan decision needs reopening (`brief.md:637`, `brief.md:651`; `target/xtask/tests/dist_two_binary_layout.rs:490`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the new test produces an assertion failure naming all four pipeline files, rather than a compilation failure (`pdca-reviewer-742-evidence/red.log:31`; `target/xtask/tests/dist_two_binary_layout.rs:496`). |
| C3 Change | PASS | Both build paths and staging consume the declared binary set; missing input fails explicitly, and extraction preparation precedes container creation (`target/xtask/src/dist.rs:559`, `target/xtask/src/dist.rs:635`, `target/xtask/src/dist.rs:698`). |
| C4 Verification (red→green) | PASS | Restoring the identical patch passes 35 targeted tests, `dist --check`, and ShellCheck; broader coverage is 66/81 changed executable lines; full CI is supported by the frozen pass, with the local advisory-cache restriction disclosed below (`pdca-reviewer-742-evidence/green.log:36`, `pdca-reviewer-742-evidence/coverage-summary.log:2`, `gate-logs/C4-ci.log:3968`). |
| C5 Causal adequacy | PASS | The shared set checker detects a third binary missing from every pipeline file, and real staging checks distinct bytes and modes; the fix addresses unchecked duplication without a capability probe or symptom guard (`target/xtask/tests/dist_templates.rs:626`, `target/xtask/tests/dist_templates.rs:775`). |
| T1 Structure | PASS | The data table and pure argument builders fit the existing staging-plan seam, with external commands confined to the runner and one shared checker serving both test sets (`target/xtask/src/dist.rs:79`, `target/xtask/src/dist.rs:387`, `target/xtask/tests/dist_templates.rs:23`). |
| T2 Shape | PASS | Exact-text pins follow the accepted test redesign; workflow filters include the validator, and the living deployment document describes the two-binary layout (`target/xtask/tests/dist_two_binary_layout.rs:11`, `target/xtask/tests/fdb_image.rs:323`, `target/docs/design/architecture/07-deployment-view.md:42`). |
| T3 Runtime | NEEDS-HUMAN | Record successful `fdb-image.yml` PR evidence and the manual `release.yml` run before first release, as already agreed — Docker/buildx extraction and privileged installation were not exercised here; host CLI usage and staging dummy bytes prove narrower properties (`brief.md:644`; `target/.github/workflows/fdb-image.yml:77`, `target/.github/workflows/release.yml:54`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must run at publish, so the deferred row creates no human clearance item (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The affected-path prior-art check covers merged history and 356 closed/merged PRs, including complete pagination of large diffs; no closed-unmerged PR touches these paths, and settled sign-off decisions remain settled (`pdca-reviewer-742-evidence/prior-art-summary.log:35`; `brief.md:644`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the operator-facing fitness of the two-binary distribution and its maintained text contracts — automated consistency checks cannot establish that the delivered artifacts meet the operator need (`target/deploy/dist/README.md:9`; `target/xtask/tests/dist_two_binary_layout.rs:21`). |

The independent evidence supports the container-free contract. The disposable target compiled against both states, and the tracked diff and new-test bytes were identical after stash/pop. The green run contains 27 `dist_templates`, two `dist_two_binary_layout`, and six `fdb_image` tests, including planted drift, third-entry diagnostics, missing-source refusal, byte identity, and executable modes (`pdca-reviewer-742-evidence/green.log:8`). `dist --check`, `shellcheck deploy/dist/install.sh`, and `sh -n deploy/dist/install.sh` each exit zero; the host-built validator prints its usage and exits 2 with no arguments (`pdca-reviewer-742-evidence/dist-check.log:3`, `pdca-reviewer-742-evidence/shellcheck.log:4`, `pdca-reviewer-742-evidence/shell-syntax.log:4`, `pdca-reviewer-742-evidence/validator-usage.log:3`).

The two frozen advisory failures need narrower interpretations than their headline results:

- **Coverage:** the frozen command selects only `dist_two_binary_layout`, deliberately a text-only test, and records 0/81 production lines (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:117`). Independently running LLVM coverage with `dist_templates` and `fdb_image` included reaches 66/81, or 81.5% (`pdca-reviewer-742-evidence/coverage-summary.log:2`). The remaining misses include the external build/extraction and assembly orchestration; this does not prove that orchestration ran.
- **Mutation testing:** the frozen baseline aborts at `git ls-files -s -z`, before testing mutants (`gate-logs/C5-mutants.log:451`). A rerun with `--copy-vcs true` and `--cap-lints true` passes the baseline and yields 12 caught, one unviable, and four missed mutants (`pdca-reviewer-742-evidence/mutants.log:3`, `pdca-reviewer-742-evidence/mutants.log:21`). The survivors replace `obtain_binaries`, `assemble` (two variants), and `run_dist`; they expose the already-declared orchestration coverage limit, not a newly demonstrated production defect. Real-artifact verification remains owed under T3.

The local full-CI attempt stopped on a host restriction, after passing spelling, docs lint/render, repository guards, formatting, workspace Clippy/build/tests, and dependency-use scanning: Cargo Deny could not take an exclusive lock on its read-only advisory database (`pdca-reviewer-742-evidence/ci.log:3358`). I do not claim that local run completed. The frozen log explicitly records all three dependency-wall checks, statics/deployment guards, DST, and final success (`gate-logs/C4-ci.log:3364`, `gate-logs/C4-ci.log:3968`). The separate TiKV log records both feature compilations finishing (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). The batch-review log reports zero blocking findings; only that summary, not its separate review artifact, is present (`gate-logs/T4-batch-review.log:10`).

The remaining runtime action follows the existing agreement. On the reviewed PR, retain the successful `fdb-image` run showing both usage smokes. Before first release, run `gh workflow run release.yml --repo getwyrd/wyrd --ref <reviewed-branch>` and inspect its distribution-build and installer-smoke jobs. The smoke must run the validator before installing the FDB client, run `wyrd` afterward, preserve operator configuration across reinstall, and confirm both binaries disappear after uninstall (`target/.github/workflows/release.yml:91`, `target/.github/workflows/release.yml:99`, `target/.github/workflows/release.yml:104`, `target/.github/workflows/release.yml:108`). These results are absent from this bundle; the accepted timing, option A, README wording, and pre-#743 packaging decision are not reopened.

All source citations above resolve inside the supplied `$PDCA_TARGET` (`target/`). Review evidence and scratch outputs remain inside this leaf's cwd for harness disposal; no implementation files were edited.

### Advisory — adversary

# Adversarial review — issue 742 (two-binary tarball), iteration 9

Verdict: I could not refute the fix. The red→green proof reproduces, the test reads the
real pipeline files, and the production change works when I exercise it. One test-reach
gap goes to the human (it stands in for the C5 mutation evidence that never ran). The
rest are notes.

## Evidence

- **Red→green reproduced at the target, and it is real.** I copied `$PDCA_TARGET` to
  scratch. Green: `dist_templates` 27/27, `dist_two_binary_layout` 2/2, `fdb_image` 6/6.
  Red: I reverted every modified file and kept the new test. Then
  `every_pipeline_file_names_every_expected_binary` fails at
  `xtask/tests/dist_two_binary_layout.rs:496`, naming the Dockerfile (the `--bin` and
  `COPY` lines), `install.sh` (`rm -f` missing at line 116), `release.yml` (the smoke
  step differs at line 71) and the README. The test reads the real repo files through
  `PipelineTexts::read` (`:81`). Nothing is mocked, it imports nothing from production
  code, and it is not a tautology: deleting any one of those lines turns it red.
- **Unwarranted claim in `check-gates.json` (harness, not builder):** the C4-verify row
  says "2 test(s) ran red". Only one did. The red leg's own output is `1 passed; 1
  failed`. `the_expected_set_is_well_formed` (`xtask/tests/dist_two_binary_layout.rs:466`)
  checks only the file's local constant, so it passes on both legs. The verdict still
  holds; the count is wrong.
- **C5-mutants and C4-diff-cov reds are tooling, not refutations.** C5 never tested a
  mutant: its unmutated baseline failed at `xtask/tests/repo_hygiene_guards.rs:137`
  ("git ls-files -s -z must succeed" — the mutant copy has no git index). C4-diff-cov is
  0% by construction, because the red-earning file is barred from naming any
  `xtask::dist` symbol. The `dist_templates.rs` tests do execute `stage_binaries`,
  `host_build_args`, `docker_cp_args`, `extracted_binary_path` and
  `prepare_extraction_dir`.

## Refutation attempts that landed (test reach, not current behaviour)

- NEEDS-HUMAN [human] — **No test reaches the call sites that hand the table to the real
  pipeline, and C5 did not run, so nothing else would notice.** I applied three one-line
  mutants at once, and all 35 tests in the three suites still passed:
  (a) `xtask/src/dist.rs:698` `stage_binaries(&shipped_binaries()[..1], …)`;
  (b) `:644` `binaries.iter().take(1).try_for_each(…)`;
  (c) `:564` `host_build_args(&binaries[..1], …)`.
  How each would show up: (b) fails loudly at `cargo xtask dist`, because
  `prepare_extraction_dir` (`:635`) empties the directory and `stage_binaries` then
  refuses the missing source. (a) makes `cargo xtask dist` exit 0 with a tarball that
  holds only `bin/wyrd`. It is caught only by the release smoke, where
  `install -m 0755 "$HERE/bin/wyrd-validate"` (`deploy/dist/install.sh:143`) fails under
  `set -eu`, so on a `v*` tag or a manual dispatch, never in `ci`. (c) in `--host` mode
  stages whatever `target/release/wyrd-validate` an earlier build left behind (`:568`).
  That is silent if such a file exists, which is exactly the "`--host` tarball missing
  the validator" class the brief names. The brief lists "`obtain_binary`'s extraction
  list, the `--host` argv" as BUILT AND EXERCISED AT CHECK (`brief.md:233-235`). What is
  actually exercised is the pure argv builders, not that `obtain_binaries` and `assemble`
  pass them the whole table. Today's code is correct, so this is a coverage gap, not a
  bug. The human's call: accept it, since the release smoke and `fdb-image.yml` are the
  settled real-artifact proof (v7 sign-off), or ask for one more test. A test would mean
  making the template-plus-binary staging half of `assemble` (everything before the tar)
  a `pub` function and running it over the real templates with dummy binaries.
  Tagged `[human]` rather than `[impl]` because production was accepted as sound at v7
  and v8, and whether another rebuild at iteration 9 is worth it is a cost decision.

## Notes (no action asked in this PR)

- **Pre-existing, now extended to a second binary:** `--host` reads from
  `<root>/target/release` (`xtask/src/dist.rs:94`, `:568`). HEAD had the same
  `root.join("target/release/wyrd")` (`git show HEAD:xtask/src/dist.rs:430`). With
  `CARGO_TARGET_DIR` set, cargo writes elsewhere, and staging silently picks up old
  `target/release/{wyrd,wyrd-validate}` if they exist. This is outside #742's scope, so
  it should get a follow-up issue (ask `cargo metadata` for `target_directory`, or pass
  `--target-dir`), not a fix here.
- **Limits of the text pin, already declared:** edits outside the pinned regions pass
  the gate. Examples: a later `rm -f "$BINDIR/wyrd-validate"` below
  `install.sh:143`, a job-level `continue-on-error`, or a new final Dockerfile stage
  without the `COPY`. The test documents this (`dist_two_binary_layout.rs:21-24`) and the
  v7 sign-off accepted it. Each would fail loudly in `fdb-image.yml` or the release
  smoke, so I am not raising it.

## Attempted to refute the production change, and could not

- **Release smoke quoting.** I fed `release.yml:76-113` through bash, with `docker run …
  sh -eu -c "` replaced by `printf`, to see exactly what the container receives. The
  in-container script arrives whole (33 lines, through `test -d /etc/wyrd`) and parses
  under `sh -n`. No new line has a bare `"`, a backtick or a `$`.
- **The validator's real no-args behaviour.** I built `wyrd-validate` from the target. It
  writes `usage: wyrd-validate --endpoint <ENDPOINT> …` to **stderr** and exits **2**.
  Both smokes therefore pass on a good binary: `release.yml:91-94` captures `2>&1`, and
  `fdb-image.yml:106-111` captures stderr only. A binary that cannot load (exit 127,
  nothing in the file) fails both greps.
- **One build for both binaries.** `cargo check --release --locked --bin wyrd --bin
  wyrd-validate --features fdb,etcd` (the shape of `Dockerfile:78`) resolves features
  and starts compiling, with no "feature not found" error. The validator's normal
  dependency tree has no openssl, native-tls or aws-lc, even when unified with
  `wyrd-server/fdb,etcd`. So the bookworm-slim runtime stage needs no new library, and
  "links no `libfdb_c`" is checked for real by running it before the FDB client install
  (`release.yml:91`).
- **Container lifetime.** `prepare_extraction_dir` runs before `docker create`
  (`xtask/src/dist.rs:635`). Nothing between `create` and `docker rm -f` returns early;
  the `docker cp` result is collected and only propagated at `:652`.
- **Installer.** `shellcheck deploy/dist/install.sh` is clean. The removals
  (`install.sh:117-118`) and installs (`:142-143`) are unconditional top-level
  statements. `ROLES` did not grow. The `--help` range `sed -n '2,17p'` (`:38`) now
  covers the header exactly. HEAD's `2,16` also printed the `set -eu` line, so this
  fixes a small old bug.
- **Workflow path filter.** `crates/validate/**` is a real `paths:` entry
  (`fdb-image.yml:45`). The new parser in `xtask/tests/fdb_image.rs` reads list items
  only, and a commented-out entry is pinned as not counting.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `xtask/tests/fdb_image.rs:343`: The validator smoke pin does not cover the whole YAML step. Appending `        if: false` or `        continue-on-error: true` after the pin's trailing blank line leaves `wf.contains(VALIDATOR_SMOKE_STEP)` true, while YAML attaches the key to that same step, allowing the smoke to be skipped or its failure ignored. Confirmed with an in-memory edit and YAML parse. Reuse the following-step boundary check at `xtask/tests/dist_two_binary_layout.rs:400` and extend the regression at `xtask/tests/fdb_image.rs:439` to cover keys after blank lines/comments.

No other introduced correctness bugs or actionable reuse, simplification, or efficiency findings. Installer `shellcheck` and shell syntax checks passed; both workflow smoke scripts passed syntax checks. Frozen CI and red-to-green evidence passed. The coverage run selected only the text-only test, and mutation testing stopped at an unrelated Git-index baseline failure; neither establishes a production defect here. Real artifact execution remains deferred as settled in the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T3 Runtime — Record successful `fdb-image.yml` PR evidence and the manual `release.yml` run before first release, as already agreed — Docker/buildx extraction and privileged installation were not exercised here; host CLI usage and staging dummy bytes prove narrower properties (`brief.md:644`; `target/.github/workflows/fdb-image.yml:77`, `target/.github/workflows/release.yml:54`).
- [x] Validation — fitness-to-purpose — Accept the operator-facing fitness of the two-binary distribution and its maintained text contracts — automated consistency checks cannot establish that the delivered artifacts meet the operator need (`target/deploy/dist/README.md:9`; `target/xtask/tests/dist_two_binary_layout.rs:21`).
- [x] **No test reaches the call sites that hand the table to the real pipeline, and C5 did not run, so nothing else would notice.** I applied three one-line mutants at once, and all 35 tests in the three suites still passed: (a) `xtask/src/dist.rs:698` `stage_binaries(&shipped_binaries()[..1], …)`; (b) `:644` `binaries.iter().take(1).try_for_each(…)`; (c) `:564` `host_build_args(&binaries[..1], …)`. How each would show up: (b) fails loudly at `cargo xtask dist`, because `prepare_extraction_dir` (`:635`) empties the directory and `stage_binaries` then refuses the missing source. (a) makes `cargo xtask dist` exit 0 with a tarball that holds only `bin/wyrd`. It is caught only by the release smoke, where `install -m 0755 "$HERE/bin/wyrd-validate"` (`deploy/dist/install.sh:143`) fails under `set -eu`, so on a `v*` tag or a manual dispatch, never in `ci`. (c) in `--host` mode stages whatever `target/release/wyrd-validate` an earlier build left behind (`:568`). That is silent if such a file exists, which is exactly the "`--host` tarball missing the validator" class the brief names. The brief lists "`obtain_binary`'s extraction list, the `--host` argv" as BUILT AND EXERCISED AT CHECK (`brief.md:233-235`). What is actually exercised is the pure argv builders, not that `obtain_binaries` and `assemble` pass them the whole table. Today's code is correct, so this is a coverage gap, not a bug. The human's call: accept it, since the release smoke and `fdb-image.yml` are the settled real-artifact proof (v7 sign-off), or ask for one more test. A test would mean making the template-plus-binary staging half of `assemble` (everything before the tar) a `pub` function and running it over the real templates with dummy binaries. Tagged `[human]` rather than `[impl]` because production was accepted as sound at v7 and v8, and whether another rebuild at iteration 9 is worth it is a cost decision.
- [x] `xtask/tests/fdb_image.rs:343`: The validator smoke pin does not cover the whole YAML step. Appending `        if: false` or `        continue-on-error: true` after the pin's trailing blank line leaves `wf.contains(VALIDATOR_SMOKE_STEP)` true, while YAML attaches the key to that same step, allowing the smoke to be skipped or its failure ignored. Confirmed with an in-memory edit and YAML parse. Reuse the following-step boundary check at `xtask/tests/dist_two_binary_layout.rs:400` and extend the regression at `xtask/tests/fdb_image.rs:439` to cover keys after blank lines/comments.
- [x] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [x] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [x] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.

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
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- issue_742: Docker extraction and a real install of the two-binary tarball still need to be tested (fdb-image.yml on the PR + manual release.yml run before first release).
