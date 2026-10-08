# Adversarial review — #742 (ship `wyrd-validate` in the two-binary tarball)

**Bottom line: I could not refute the fix.** The red→green is real and runs over the real
pipeline files, and the five stages are consistent with each other. What I found: two weak
spots in the new tests (both cheap builder fixes), one claim in the brief that overstates
what Check exercises, and one existing CI gap that makes the stated invariant narrower than
it reads.

## What I tried and could not break

- **Red→green evidence** (`gate-logs/C4-verify.log`). The red leg fails with nine specific
  disagreements across all five places (Dockerfile build and copy, install.sh install and
  uninstall, release smoke run and absence check, README rows and install path). The test
  reads the real repo files (`xtask/tests/dist_two_binary_layout.rs:27-30`). It is not a
  tautology, not a mirror of production, and it does not fail for an unrelated reason.
- **Partial reverts** (scratch copy, `cargo test -p xtask --test dist_templates --test
  dist_two_binary_layout`). Removing only the uninstall `rm -f`
  (`deploy/dist/install.sh:118`) or only `test ! -e /usr/local/bin/wyrd-validate`
  (`.github/workflows/release.yml:93`) turns both checker runs red with the right message.
- **The diagnostic the brief promised.** I added a third entry to `shipped_binaries()`
  (`xtask/src/dist.rs:287`) and changed nothing else.
  `every_pipeline_stage_ships_the_production_binary_set` named all 8 missing spellings
  across the 4 files, and the equality test said to update `EXPECTED_BINARIES`.
- **Mutations of `binary_source_path`** (`xtask/src/dist.rs:306`): `join(tarball_dest)` and
  an added suffix. Both were killed by `stage_binaries_copies_each_binary_to_its_own_destination`,
  because it hard-codes the source path in its `remove_file` step
  (`xtask/tests/dist_templates.rs:647`). Swapped or duplicated sources are killed by the
  byte-for-byte check.
- **Cargo accepts the two-bin build.** `cargo check --release --locked --bin wyrd --bin
  wyrd-validate --features fdb,etcd` exits 0 on this tree. So `deploy/docker/wyrd/Dockerfile:72`
  and the `host_build_args` argv (`xtask/src/dist.rs:312`) do not fail on
  features that `wyrd-validate` lacks.
- **The smoke step matches the real binary.** With no arguments, `wyrd-validate` returns
  `ArgError::Missing`, prints `usage: wyrd-validate …` to stderr (`crates/validate/src/args.rs:184`)
  and exits 2, which is what `.github/workflows/release.yml:74-77` expects.
  `cargo tree -p wyrd-validate -e normal` shows no openssl, aws-lc or ring. So running it
  before libfdb_c is installed is sound, and it adds no runtime library the bookworm smoke
  container lacks.

## Findings

- NEEDS-HUMAN [impl] — **The release-workflow checker ignores comments and order.**
  `xtask/tests/dist_two_binary_layout.rs:194-196` treats any line in the smoke step that
  contains `/usr/local/bin/<name>` as a token as proof the binary gets run, and that
  includes a comment line. `:202` only checks that `test ! -e /usr/local/bin/<name>` appears
  somewhere in the step, even though its failure message says "after --uninstall". I ran two
  concrete false greens in a scratch copy, and all 24 tests passed in both:
  (a) replace the whole validator invocation at `.github/workflows/release.yml:74-77` with
  `# TODO smoke /usr/local/bin/wyrd-validate later`;
  (b) move `test ! -e /usr/local/bin/wyrd-validate` from `:93` to just after
  `cd /tmp/wyrd-*` (`:69`), before `./install.sh`. The release run would also pass, so the
  absence check would quietly stop proving anything.
  Fix: skip lines starting with `#`, and require each absence line to come after the
  `./install.sh --uninstall` line. The install.sh split at `:127` has the same weakness at
  lower risk: anything above the `# ── install` banner counts as "the uninstall path".
- NEEDS-HUMAN [impl] — **The brief's "BUILT AND EXERCISED AT CHECK" list includes
  "`obtain_binary`'s extraction list", but no test exercises it.** No test reaches
  `extract_binaries` (`xtask/src/dist.rs:593-612`, the `docker cp` loop the release
  actually uses) or the `assemble` call at `:656` (C4-diff-cov MISS 585-609, 656). Concrete
  surviving mutation: change `:656` to `stage_binaries(&shipped_binaries()[..1], …)` and all
  24 tests still pass. Only the path helper (`:306`) and the `--host` argv (`:312`) are
  pinned. Every break of this kind I could construct fails loudly at release time, not
  silently. `stage_binaries` errors on a missing source. A swapped `image_path` is caught by
  the smoke step, because `wyrd` cannot load before libfdb_c is installed. But that only
  happens on a `v*` tag, which is the expensive place to find it. Cheap fix in the
  `host_build_args` style: a pure function that returns the `docker cp` argv for each binary
  (`<cid>:<image_path>` → `binary_source_path(extracted, b)`), checked without a container.
- NEEDS-HUMAN [human] — **The invariant "no stage may disagree … without the gate saying
  so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every
  `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits
  only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`),
  merges green, and the drift then fails the next unrelated code PR. This gap predates the
  patch (it already affects `readme_dev_section.rs` and the existing README checks), so
  under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue
  rather than an in-PR fix. Either way, the sign-off should not read the invariant as
  covering README-only changes.

## Notes on the gate evidence (not refutations)

- **C5-mutants "fail" is an environment fault, not a test result.** The baseline run on the
  unmodified tree failed at `xtask/tests/repo_hygiene_guards.rs:129` (`git ls-files -s -z
  must succeed`), because the cargo-mutants copy has no `.git`. No mutant was ever run
  against this patch. The hand-run mutations above cover only part of that gap.
- **C4-diff-cov's 0.0% overstates the gap.** That gate ran only `dist_two_binary_layout`,
  which by design calls no `xtask::dist` function. The new pure functions are run by
  `dist_templates.rs`, whose 22 tests pass in `gate-logs/C4-ci.log`. The lines that are
  actually never run are the docker/assemble lines named in the second finding.
- **Deferred, as the brief declares:** no image was built and `install.sh` was not run.
  Docker is present on this host, but I did not try the image build (it needs network
  access and a full release build). The release workflow is still the only place where the
  tarball's contents are observed.
