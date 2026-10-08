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
