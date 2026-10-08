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
