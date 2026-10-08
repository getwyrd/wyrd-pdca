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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 113 instrumentable changed lines executed (below the 80% floor); 113 of 201 changed lines were
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.07s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #742: shipping `wyrd` and `wyrd-validate` through one distribution contract passes the binding container-free checks; real release validation and packaging timing still require human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines a falsifiable two-binary contract and explicitly assigns real packaging to release validation; option A is the recorded Plan decision (brief.md:43, brief.md:237, brief.md:331). |
| C2 Reproduction (red pre-fix) | PASS | With tracked changes stashed and the new test retained, the test compiled and failed for the missing validator across all four pipeline files; this reproduces the specified gap (pdca-reviewer-742-evidence/red.log:13; xtask/tests/dist_two_binary_layout.rs:444). |
| C3 Change | PASS | The patch covers the declared packaging surfaces, preserves the validator's separation from service roles, and updates the living deployment description; the prerequisite-base caveat below is not a deletion by this patch (patch.diff:1; deploy/dist/install.sh:140; docs/design/architecture/07-deployment-view.md:42). |
| C4 Verification (red→green) | PASS | Restoring the patch passed 28 template/staging tests plus the discriminator and `dist --check`; full CI is supported by the frozen passing log, while the independent CI rerun stopped at a read-only advisory-database lock (pdca-reviewer-742-evidence/green-after.log:37, pdca-reviewer-742-evidence/dist-check.log:2, pdca-reviewer-742-evidence/ci.log:3356, gate-logs/C4-ci.log:3966). |
| C5 Causal adequacy | PASS | The scoped cause—independent binary-set declarations drifting silently—is addressed by one table, shared checks over both sets, and tests that reject missing/extra entries and wrong staged bytes; no capability probe masks a load-time cause (xtask/tests/dist_templates.rs:527, xtask/tests/dist_templates.rs:551, xtask/tests/dist_templates.rs:1019). |
| T1 Structure | PASS | Distribution ownership remains in xtask, with the shared checker reused by both test targets and no new production dependency or service seam (xtask/src/dist.rs:289; xtask/tests/dist_templates.rs:17; xtask/tests/dist_two_binary_layout.rs:85). |
| T2 Shape | PASS | The table consistently identifies both binaries, host build/extraction paths are distinct, and the validator adds no unit/config role; the installer summary also passes its alignment regression (xtask/tests/dist_templates.rs:842; xtask/tests/dist_templates.rs:906; deploy/dist/install.sh:140; xtask/tests/dist_templates.rs:309). |
| T3 Runtime | NEEDS-HUMAN | Accept deferring the real release build/install or require that evidence before sign-off—Docker/buildx, image extraction, archive creation, and privileged bookworm install/uninstall were not exercised; compiled contracts and real staging of dummy bytes establish a narrower result (.github/workflows/release.yml:54; xtask/tests/dist_templates.rs:1042; pdca-reviewer-742-evidence/mutants-lint-capped.log:5). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit is owed at publish, so the deferred row requires no human clearance (gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN | Accept landing packaging before functional scenarios and retain the human no-early-tag checkpoint, or return sequencing to Plan—the exercised CLI reports that nothing was validated, while any `v*` tag can publish it (crates/validate/src/lib.rs:120; .github/workflows/release.yml:23; .github/workflows/release.yml:115; brief.md:472). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm option A and its suitability for operators—the production image gains the separate validator, and layout/usage evidence alone does not demonstrate validation of an operator deployment (brief.md:20; deploy/docker/wyrd/Dockerfile:131; deploy/dist/README.md:17). |

No implementation defect was established within the binding scope. Three decisions remain: accept the deferred real-release evidence, accept the sequencing checkpoint, and confirm the packaging choice and operator fitness. These are advisory sign-off items, not requests for a rebuild.

Source citations are relative to `$PDCA_TARGET`; brief, patch, frozen logs, and reviewer evidence are relative to this review directory. The supplied target is a self-contained disposable Git repository. The stash was restored, `git diff --check` passed, and `git apply --reverse --check patch.diff` confirmed the target still matches the supplied patch (`pdca-reviewer-742-evidence/target-state.log:14`). No implementation files were edited.

The verification evidence supports the scoped verdict with the following limits:

- **Independent red→green is reproduced.** `cargo test --locked -p xtask --test dist_two_binary_layout` failed with exit 101 after stashing tracked changes, naming missing Docker build/copy, install/removal, smoke/absence, and README entries. After restoration, both test targets passed all 29 tests. The real staging tests check each binary's distinct contents, mode 0755, missing-source errors, and the complete template/binary/VERSION tree (`pdca-reviewer-742-evidence/red.log:13`, `pdca-reviewer-742-evidence/green-after.log:37`; `xtask/tests/dist_templates.rs:1019`, `xtask/tests/dist_templates.rs:1042`).
- **The full-CI rerun hit a host restriction, not a patch failure.** Typos, docs lint/render, repository guards, fmt, workspace clippy/build/tests, and cargo-machete passed before cargo-deny could not lock `/home/eddie/.cargo/advisory-dbs/db.lock`. The frozen log actually records all three deny passes, subsequent guards/DST, and final success; those remaining stages are log-backed, not independently rerun successes (`pdca-reviewer-742-evidence/ci.log:3356`; `gate-logs/C4-ci.log:3363`, `gate-logs/C4-ci.log:3966`). The frozen TiKV log records both requested compilation checks completing (`gate-logs/host-tikv.log:209`).
- **The reported zero coverage is accurate for the gate's selected test, but excludes the required staging tests.** The frozen command runs only `dist_two_binary_layout`, whose deliberate text-only design executes none of the changed production lines (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:147`). An independent LLVM-coverage run including `dist_templates` passes 29 tests and records calls to `stage_binaries` (3), `stage_tarball_tree` (1), both argv helpers, and the source-path/table functions. Container orchestration remains unexecuted; this does not turn the frozen advisory coverage row green (`pdca-reviewer-742-evidence/coverage-summary.txt:14`).
- **Mutation evidence has been investigated beyond the frozen baseline failure.** The frozen run tested no mutants because its copy lacked a usable Git index (`gate-logs/C5-mutants.log:449`). Rerunning with `--copy-vcs true` gives a passing baseline, one caught mutant, and 16 unviable mutations. A supplementary run with `RUSTFLAGS=--cap-lints=warn`, explicitly allowing mutation-created unused-variable warnings, yields 11 caught, one unviable, and five survivors. All five replace the unexercised `obtain_binaries`, `extract_binaries`, `assemble`, or `run_dist` orchestration; none defeats the tested table/staging contract. They substantiate the T3 release-validation limitation, not a newly demonstrated implementation defect (`pdca-reviewer-742-evidence/mutants.log:3`; `pdca-reviewer-742-evidence/mutants-lint-capped.log:5`).

The human decisions have concrete evidence and completion steps:

- **T3:** Run `cargo xtask dist --oci-archive` on the candidate in a Docker/buildx-capable environment, then execute the complete bookworm smoke block at `.github/workflows/release.yml:60`. Verify both archive entries, validator usage before installing the FDB client, roles-binary usage afterward, preserved operator config after reinstall, and absence of both installed binaries after uninstall. The workflow's `workflow_dispatch` path performs this without its tag-only release publication step. Locally, `cargo xtask dist --check` and shell syntax passed; the actual host-built validator returned exit 2 with usage for no arguments. This is not a substitute for the image/install run (`pdca-reviewer-742-evidence/dist-check.log:2`, `pdca-reviewer-742-evidence/probes.log:6`; `.github/workflows/release.yml:74`, `.github/workflows/release.yml:91`).
- **T5:** A real invocation with complete arguments and dummy credentials exited 0 while explicitly reporting that no requests were issued and nothing was validated (`pdca-reviewer-742-evidence/probes.log:14`; `crates/validate/src/lib.rs:120`). The proposal requires a committed seven-day Hetzner endurance verdict before the Alpha tag and explicitly describes this as a human runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`). The supplied tree contains no dedicated release runbook; release publication still depends only on a tag. The maintainer must accept that sequencing responsibility or defer packaging as the brief specifies.
- **Validation:** Confirm the session's option-A decision at sign-off and the operator-facing suitability of carrying the validator in the production image. Mirroring that decision onto #742 is requested by the brief; no tracker message was sent (`brief.md:20`, `brief.md:331`).

Prior art was checked by every affected file path against merged `main` history, and against file lists for all 19 closed, unmerged PRs. All queries succeeded; none of those rejected/closed PRs overlaps an affected path. The existing distribution work is the one-binary pipeline, not a prior rejected two-binary implementation (`pdca-reviewer-742-evidence/prior-art-summary.txt:1`, `pdca-reviewer-742-evidence/rejected-paths.json:1`). The frozen batch-review log reports zero blocking findings but contains no individual review narratives; no additional review agents were launched (`gate-logs/T4-batch-review.log:10`).

There is a prerequisite-base caveat: the supplied base and patched tree both lack the #736 `WYRD_VERSION` wiring that `brief.md:173` expects, while #738's chunk-size contract is present. The patch does not remove that wiring; this snapshot cannot certify integration with that prerequisite. Refresh/check the intended dependency base during integration rather than treating the absence as a C4 patch defect (`pdca-reviewer-742-evidence/target-state.log:17`; `xtask/src/dist.rs:553`; `deploy/docker/wyrd/Dockerfile:42`; `xtask/tests/dist_templates.rs:194`).

### Advisory — adversary

# Adversarial review — issue 742 (dist-ship-wyrd-validate-two-binary-tarball)

I tried to refute the red→green proof, the staging code, the image build line and the
layout checker. The proof and the Rust staging stand up. The one real hole is in the
release-smoke part of the checker.

## Findings

- NEEDS-HUMAN [impl] — **The release-smoke checker pins the invocation but not the
  assertion on its result, so the smoke can stop proving anything while every test stays
  green.** `xtask/tests/dist_two_binary_layout.rs:351-355` counts a binary as "run with its
  exit status checked" when it is first in command position and its line has no `||`/`&&`.
  It never looks at the `grep -q 'usage: …'` lines (`.github/workflows/release.yml:77`,
  `:85`), and those greps are what actually fail the smoke. Without them,
  `if /usr/local/bin/wyrd-validate …; then exit 1; fi` passes when the binary cannot run at
  all (a loader error or wrong arch exits 127, so the `if` branch is skipped). I ran four
  mutations in a scratch copy, and all 29 tests in `dist_templates` +
  `dist_two_binary_layout` passed for each one:
  (1) delete `release.yml:77`;
  (2) delete `:77` and `:85` and turn both `exit 1` bodies into `:`;
  (3) replace `:74-77` with `/usr/local/bin/wyrd-validate 2>&1 | cat >/dev/null`;
  (4) replace `:74-77` with `/usr/local/bin/wyrd-validate &`.
  So the failure message at `:359` ("with its exit status checked") claims more than the
  check does, and the negative cases at `xtask/tests/dist_templates.rs:658` only try
  `|| true` and `&& echo ok`. Cheapest fix that closes the case that matters: for each
  binary, also require a `grep -q 'usage: <name>'` (or an equivalent output assertion)
  after the invocation and before `./install.sh --uninstall`, and add mutation (1) as a
  negative case. Treating a pipeline (`|`) or background (`&`) as "not run" is a
  one-line addition. Low severity: today's `release.yml` is correct. The gap only matters
  if a later edit weakens the smoke, but catching that kind of drift is this checker's whole job.

- NEEDS-HUMAN [human] — **Addendum to the pre-declared T3 deferral: half of the "release-only"
  evidence will show up on this PR for free.** The brief says the image half can only be
  observed by a `v*` tag or a `workflow_dispatch` run (`brief.md:255-263`).
  `.github/workflows/fdb-image.yml:22` path-filters on `deploy/docker/wyrd/**`, which this
  patch edits, so that job will run on the PR. It runs `docker build --build-arg
  FEATURES=fdb,etcd` against the new Dockerfile (`:72-78`). A green run proves that
  `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` and
  both `COPY` lines (`deploy/docker/wyrd/Dockerfile:72`, `:130-131`) work. It does not run
  `wyrd-validate` inside the image (its smoke is `wyrd` usage plus `fdbcli`, `:83-95`). It also
  does not observe extraction, the tarball, or install. At sign-off, check that job's result
  before accepting the deferral. Whether to add a one-line `docker run --entrypoint
  wyrd-validate wyrd:fdb` smoke there is a scope call; the brief did not ask for it.

- Stale wording the diff created (not worth a rebuild on its own; fold it into any rebuild):
  `deploy/README.md:14-15` still says the pipeline "extracts the binary from it, so tarball
  and image ship the identical binary" (singular). Every other description was updated to
  name both binaries (`deploy/dist/README.md`, `07-deployment-view.md:42`,
  `xtask/src/main.rs:93-97`).

## Refutation attempts that failed

- **Red→green is genuine.** I reproduced it myself rather than trusting
  `gate-logs/C4-verify.log`. Green: 1/1 on the patched tree. Red: I reverted all 8 modified
  files with `git checkout HEAD` and kept `xtask/tests/dist_two_binary_layout.rs`. It still
  compiles, because it names no `xtask::dist` API. It fails at `:446` with 9 disagreements,
  one per missing pipeline spelling in all four files (Dockerfile build and `COPY`,
  install.sh install and remove, release.yml run and absence check, README rows and install
  path). The test reads the real repo files through the same checker that `dist_templates.rs`
  runs on the production table (`xtask/tests/dist_templates.rs:16-21`, `#[path]` include).
  There is no parallel copy.
- **The staging code is the production path, and the tests catch real breakage.** I re-ran
  `cargo mutants --in-diff` myself (the gate's C5 run died on an unrelated test,
  `repo_hygiene_guards.rs:137`, which needs `git ls-files` in a tree that is not a git repo.
  That is an environment fault, not a patch fault). Of 16 viable mutants, 11 were caught.
  Every mutant of `shipped_binaries`, `binary_source_path`, `host_build_args`,
  `image_extraction_args`, `stage_binaries` and `stage_tarball_tree` died. The 5 survivors
  are all in the Docker-only runner: `obtain_binaries` (`xtask/src/dist.rs:525`),
  `extract_binaries` (`:613`), `assemble` (`:674`) and `run_dist` (`:733`). The brief
  declares those unreachable without a container. Each one fails loudly at release time
  (for example, `stage_binaries` errors on a missing source).
- **C4-diff-cov at 0.0% is structural, not evidence against the fix.** That gate measures
  only the added test file (`gate-logs/C4-diff-cov.log:10`). The brief requires that file to
  name no new API, so it can never execute `dist.rs` lines. The lines are exercised by
  `dist_templates.rs` (28/28 green; `C4-ci.log:3057`).
- **The image build line resolves.** In the scratch copy, `cargo build --locked --release
  --bin wyrd --bin wyrd-validate --features fdb,etcd` accepted both targets and the features
  and started compiling. A misspelled bin is refused at once ("no bin target named …"). The
  workspace has no `default-members`, so `crates/validate`'s bin is in scope. The
  validator's normal dependency tree has no native or TLS crates (`libc`, `openssl-probe`,
  `rustls-native-certs`, `rustls-pki-types` only), so the default `FEATURES=""` build stage
  needs nothing new.
- **The new release smoke matches the binary's real behaviour.** With no arguments,
  `wyrd-validate` returns `ArgError::Missing` and prints `usage: wyrd-validate …` to stderr
  with exit 2 (`crates/validate/src/args.rs:172-174`, `:179-184`; `lib.rs:127-129`). So
  `release.yml:74-77` passes on a good binary and fails on one that exits 0.
- **Install.sh, Dockerfile and README checkers:** I tried comment-outs, moving lines into the
  wrong branch or stage, `|| true` on the install line, `--chmod` on the `COPY`, and a
  repeated README description. Every one is either reported, or is a false red that fails
  loudly at build time. I found no false green worth filing.
- **Earlier slices preserved:** #738's `--chunk-size` assertion is still present. The base
  never carried a `WYRD_VERSION` build-arg in `obtain_binary`, so its absence is not a
  regression this patch introduced.

### Advisory — code-review

No findings on either advisory lens: no patch-introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified.

Reviewed the diff against the source at `$PDCA_TARGET`, including extraction cleanup, binary-to-destination mapping, staging failures, installer symmetry, release smoke checks, and regression tests. Frozen evidence confirms CI and red→green pass. The 0% diff-coverage result measures only the standalone text test; CI also ran the staging tests successfully. Mutation testing stopped on an unmutated Git-index test failure and produced no mutation results. Real image building and privileged installation remain deferred as specified in the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Accept deferring the real release build/install or require that evidence before sign-off—Docker/buildx, image extraction, archive creation, and privileged bookworm install/uninstall were not exercised; compiled contracts and real staging of dummy bytes establish a narrower result (.github/workflows/release.yml:54; xtask/tests/dist_templates.rs:1042; pdca-reviewer-742-evidence/mutants-lint-capped.log:5).
- [ ] T5 Judgment — Accept landing packaging before functional scenarios and retain the human no-early-tag checkpoint, or return sequencing to Plan—the exercised CLI reports that nothing was validated, while any `v*` tag can publish it (crates/validate/src/lib.rs:120; .github/workflows/release.yml:23; .github/workflows/release.yml:115; brief.md:472).
- [ ] Validation — fitness-to-purpose — Confirm option A and its suitability for operators—the production image gains the separate validator, and layout/usage evidence alone does not demonstrate validation of an operator deployment (brief.md:20; deploy/docker/wyrd/Dockerfile:131; deploy/dist/README.md:17).
- [ ] **The release-smoke checker pins the invocation but not the assertion on its result, so the smoke can stop proving anything while every test stays green.** `xtask/tests/dist_two_binary_layout.rs:351-355` counts a binary as "run with its exit status checked" when it is first in command position and its line has no `||`/`&&`. It never looks at the `grep -q 'usage: …'` lines (`.github/workflows/release.yml:77`, `:85`), and those greps are what actually fail the smoke. Without them, `if /usr/local/bin/wyrd-validate …; then exit 1; fi` passes when the binary cannot run at all (a loader error or wrong arch exits 127, so the `if` branch is skipped). I ran four mutations in a scratch copy, and all 29 tests in `dist_templates` + `dist_two_binary_layout` passed for each one: (1) delete `release.yml:77`; (2) delete `:77` and `:85` and turn both `exit 1` bodies into `:`; (3) replace `:74-77` with `/usr/local/bin/wyrd-validate 2>&1 | cat >/dev/null`; (4) replace `:74-77` with `/usr/local/bin/wyrd-validate &`. So the failure message at `:359` ("with its exit status checked") claims more than the check does, and the negative cases at `xtask/tests/dist_templates.rs:658` only try `|| true` and `&& echo ok`. Cheapest fix that closes the case that matters: for each binary, also require a `grep -q 'usage: <name>'` (or an equivalent output assertion) after the invocation and before `./install.sh --uninstall`, and add mutation (1) as a negative case. Treating a pipeline (`|`) or background (`&`) as "not run" is a one-line addition. Low severity: today's `release.yml` is correct. The gap only matters if a later edit weakens the smoke, but catching that kind of drift is this checker's whole job.
- [ ] **Addendum to the pre-declared T3 deferral: half of the "release-only" evidence will show up on this PR for free.** The brief says the image half can only be observed by a `v*` tag or a `workflow_dispatch` run (`brief.md:255-263`). `.github/workflows/fdb-image.yml:22` path-filters on `deploy/docker/wyrd/**`, which this patch edits, so that job will run on the PR. It runs `docker build --build-arg FEATURES=fdb,etcd` against the new Dockerfile (`:72-78`). A green run proves that `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` and both `COPY` lines (`deploy/docker/wyrd/Dockerfile:72`, `:130-131`) work. It does not run `wyrd-validate` inside the image (its smoke is `wyrd` usage plus `fdbcli`, `:83-95`). It also does not observe extraction, the tarball, or install. At sign-off, check that job's result before accepting the deferral. Whether to add a one-line `docker run --entrypoint wyrd-validate wyrd:fdb` smoke there is a scope call; the brief did not ask for it.
- [ ] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [ ] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [ ] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.
- [ ] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [ ] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.
- [ ] T5 Judgment — Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring the real release build/install or require that evidence before sign-off—Docker/buildx, image extraction, archive creation, and privileged bookworm install/uninstall were not exercised; compiled contracts and real staging of dummy bytes establish a narrower result (.github/workflows/release.yml:54; xtask/tests/dist_templates.rs:1042; pdca-reviewer-742-evidence/mutants-lint-capped.log:5).; **The release-smoke checker pins the invocation but not the assertion on its result, so the smoke can stop proving anything while every test stays green.** `xtask/tests/dist_two_binary_layout.rs:351-355` counts a binary as "run with its exit status checked" when it is first in command position and its line has no `||`/`&&`. It never looks at the `grep -q 'usage: …'` lines (`.github/workflows/release.yml:77`, `:85`), and those greps are what actually fail the smoke. Without them, `if /usr/local/bin/wyrd-validate …; then exit 1; fi` passes when the binary cannot run at all (a loader error or wrong arch exits 127, so the `if` branch is skipped). I ran four mutations in a scratch copy, and all 29 tests in `dist_templates` + `dist_two_binary_layout` passed for each one: (1) delete `release.yml:77`; (2) delete `:77` and `:85` and turn both `exit 1` bodies into `:`; (3) replace `:74-77` with `/usr/local/bin/wyrd-validate 2>&1 | cat >/dev/null`; (4) replace `:74-77` with `/usr/local/bin/wyrd-validate &`. So the failure message at `:359` ("with its exit status checked") claims more than the check does, and the negative cases at `xtask/tests/dist_templates.rs:658` only try `|| true` and `&& echo ok`. Cheapest fix that closes the case that matters: for each binary, also require a `grep -q 'usage: <name>'` (or an equivalent output assertion) after the invocation and before `./install.sh --uninstall`, and add mutation (1) as a negative case. Treating a pipeline (`|`) or background (`&`) as "not run" is a one-line addition. Low severity: today's `release.yml` is correct. The gap only matters if a later edit weakens the smoke, but catching that kind of drift is this checker's whole job.. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
