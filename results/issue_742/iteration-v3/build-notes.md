# Build notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball (iteration 3)

Target: getwyrd/wyrd @ main, built on the integration base `d9c6225` (= `stack-base`,
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, which carries #775 and #852).
All `path:line` references are to the patched worktree unless marked "base".

This round re-applies the iteration-2 patch (which the review passed on every
implementation row) and changes only what the round-2 findings asked for, plus the three
checker holes the adversary listed. The v2→v3 delta is small and is listed first.

## Iteration 2 carry-forward — what changed

1. **The dropped space in the install summary** (the one implementation finding). The
   `units` row is back to its base text, four spaces after the label
   (`deploy/dist/install.sh:202`), so all five summary paths start in one column again.
   The patch no longer touches that line at all. New test
   `install_sh_summary_paths_line_up` (`xtask/tests/dist_templates.rs:309`) pins that
   every summary row's `$VAR` starts in the same column. It checks only the path column:
   the note column after an expanded variable moves at runtime anyway, and on base it is
   already uneven (`units` note at column 59, `config`/`data` at 60).
2. **Three false greens the adversary listed (A1–A3).** The adversary suggested declining
   them. I fixed them instead, because each is an instance of the rubric's "never silent
   success, silent skip" defect class, and each fix is a few lines:
   - **A1, `/usr/local/bin/wyrd-validate || true` counted as a smoke run.** A run now
     counts only if no word of the command contains `||` or `&&`
     (`xtask/tests/dist_two_binary_layout.rs:351-355`). Under `sh -eu`, a failure inside
     either list does not stop the script, so both swallow the result. Negative tests for
     `|| true` and `&& echo ok`, per binary (`dist_templates.rs:657-663`).
   - **A2, absence check moved after the closing `"` of `sh -eu -c "…"`.** That line
     runs on the runner, where it is always true. The checker now finds the container
     script that runs `./install.sh`: the lines indented at least as deep as it, ending
     at the first less-indented non-blank line, which is the closing `"`
     (`dist_two_binary_layout.rs:333-345`). `./install.sh --uninstall` must sit inside
     that script, and `test ! -e` is searched only between it and the script's end
     (`:363`). Negative tests: absence moved after the script (`dist_templates.rs:665-668`)
     and uninstall moved after the script (`:613-623`).
   - **A3, install wrapped in `if [ -f "$HERE/bin/wyrd-validate" ]; then … fi`.** The
     installer would skip the binary silently. The required install and removal commands
     now count only at their region's own indentation: column 0 for the install path,
     the branch-body indentation for `--uninstall` (`dist_two_binary_layout.rs:219-234`).
     Anything deeper is inside an `if`, loop or function. The "binary outside the set"
     check still looks at every depth (`:255-257`), since a nested install still ships a
     binary. Negative tests: install under `[ -f … ]`, removal under
     `[ "$PURGE" = 1 ]` (`dist_templates.rs:724-746`).

   This is indentation standing in for nesting. It matches the house style of both
   files. A text checker can always be fooled one level deeper (for example
   `if /usr/local/bin/wyrd-validate; then :; fi` still passes). I stopped at the
   realistic drifts: someone silencing a failing step, a check pasted in the wrong
   place, a defensive `[ -f ]`.

### Hand mutations for the new rules (each applied, run, reverted)

| # | Mutation | Result |
|---|---|---|
| H1 | drop the `||` / `&&` filter (`layout.rs:354`) | red: `the_release_smoke_check…` |
| H2 | absence searched to end of step, not `script_end` (`:363`) | red: same test |
| H3 | install commands accepted at any depth (`:234`) | red: `the_install_sh_check…` |
| H4 | removals accepted at any depth (`:233`) | red: same test |
| H5 | uninstall-outside-script early return disabled (`:340`) | red: same release test |
| H6 | v2's `units   ` (one space short) restored in `install.sh:202` | red: `install_sh_summary_paths_line_up` |

The iteration-2 mutation table (M1–M9, in `iteration-v2/build-notes.md`) still applies.
The patch is otherwise unchanged.

## What the patch does (unchanged from iteration 2)

**The binary set is data.** `IMAGE_BINARY_PATH` (base `xtask/src/dist.rs:41`) is gone.

- `ShippedBinary { bin, image_path, tarball_dest }` (`xtask/src/dist.rs:271`) and
  `shipped_binaries()` (`:289`): the one declaration, `wyrd` and `wyrd-validate`, in the
  `staging_plan()` style.
- `binary_source_path()` (`:308`): where a binary sits on the packaging host. Extraction
  target and staging source both go through it.
- `host_build_args(features)` (`:315`): the `--host` cargo argv, one build naming every
  table entry (scope (h)), used at `:529`.
- `image_extraction_args(cid, dir)` (`:331`): the `docker cp` argv per entry, run by
  `extract_binaries` (`:613`).
- `stage_binaries(table, source_dir, stage)` (`:349`): the `pub` staging callable
  (criterion 6). A missing source is an error, never a smaller tarball.
- `stage_tarball_tree(...)` (`:635`): everything `assemble` did except `tar`, including
  `stage_binaries(&shipped_binaries(), …)` at `:663`. `assemble` (`:674`) calls it, then
  tars.
- `obtain_binary` → `obtain_binaries` (`:525`): one container, every binary extracted,
  the container removed once whatever happened (`:605-606`), keeping base `:511-513`'s
  "remove regardless" rule for all copies. Extraction goes to `target/dist/extracted/`,
  cleared first so a leftover can't stand in for a failed copy.

**The four pipeline files name both binaries.**

- `deploy/docker/wyrd/Dockerfile:72` builds `--bin wyrd --bin wyrd-validate` in one
  `RUN`; `:131` adds the second `COPY`; runtime-stage description `:74-76` (base `:68`);
  header note `:16-19`. `ENTRYPOINT ["wyrd"]` unchanged (`:144`).
- `deploy/dist/install.sh:141` installs `bin/wyrd-validate`; `:117` removes it inside the
  `--uninstall` branch; summary row `:201`; uninstall messages `:121,123`. `ROLES`
  (`:49`) untouched: no unit, no env file, no `systemctl` for the validator.
- `.github/workflows/release.yml:74-77` runs `/usr/local/bin/wyrd-validate` with no
  arguments **before** the FDB client is installed (so the release also checks the
  README's "needs no `libfdb_c`" claim), expects non-zero and `usage: wyrd-validate`;
  `:93` asserts it is gone after `./install.sh --uninstall` (`:91`).
- `deploy/dist/README.md`: roles sentence kept (`:5-7`); validator introduced beside it
  (`:9-10`); `## Binaries` table, one row per binary (`:12-17`); install, upgrade and
  verify sections updated (`:36-40`, `:60-62`, `:77-79`).

**Docs currency:** `docs/design/architecture/07-deployment-view.md:42` lists both
binaries; `xtask/src/main.rs:92-96` (the `dist` help) too.

## The tests

`xtask/tests/dist_two_binary_layout.rs` (NEW, red-earning, names no new API):
`EXPECTED_BINARIES` (`:30`) and ONE checker over a binary set, `pipeline_disagreements`
(`:94`), built from four per-file checkers over file text: `dockerfile_disagreements`
(`:111`), `install_sh_disagreements` (`:199`), `release_yml_disagreements` (`:300`),
`readme_disagreements` (`:395`). One test (`:444`) runs it over the local set. Each
message starts with the file at fault; a binary a file lacks and a binary a file ships
beyond the set both count.

`xtask/tests/dist_templates.rs` includes it as `mod layout` (`:20-21`) and adds:

- `every_pipeline_stage_ships_the_production_binary_set` (`:527`): the same checker over
  `shipped_binaries()`.
- `the_text_tests_local_set_equals_the_production_table` (`:538`).
- `the_layout_checker_names_every_file_that_disagrees_with_the_set` (`:551`): the
  brief's exact diagnostic. Add a third entry and all four files are named, only for the
  new binary; drop an entry and every file is named for still shipping it.
- Negative tests per file, mutating the REAL file text, iterated over the production
  table (`:597`, `:690`, `:767`, `:806`).
- `the_shipped_binary_table_is_consistent_and_collision_free` (`:842`): pairwise
  distinct cargo targets, image paths, tarball destinations and host source paths.
- `the_host_build_argv_names_every_shipped_binary` (`:885`).
- `the_image_extraction_copies_each_binary_to_its_own_host_file` (`:906`).
- `stage_binaries_copies_each_binary_to_its_own_destination` (`:1019`): distinct dummy
  bytes per binary, exactly `bin/wyrd` + `bin/wyrd-validate`, each byte-equal to its own
  source, mode 0755; a missing source is an error naming it.
- `the_tarball_tree_stages_the_plan_and_every_shipped_binary` (`:1042`): the real
  `stage_tarball_tree` over the real templates and dummy binaries.
- `install_sh_summary_paths_line_up` (`:309`): new this round, see above.

## The two advisory gates that failed in iterations 1 and 2

Neither can go green on this patch without breaking the brief's own design or touching
code outside this slice. Stated so the sign-off reads them correctly:

- **C4-diff-cov reports ~0%.** Per its own header, the gate measures "under the patch's
  OWN test" (`engine/scripts/run-diff-cov.sh`), which is `--test
  dist_two_binary_layout`, and it excludes test files from the count. The brief requires
  that file to name **no** symbol this patch introduces, so it compiles on the red leg
  (Falsifiability). So it cannot reach any new line in `dist.rs`. The new `dist.rs` code
  is driven from `dist_templates.rs`. The reviewer measured that last round: 68 of 113
  instrumentable changed lines (60.2%); the misses are runner code that needs Docker, a
  host `cargo build --release`, or `tar` (`obtain_binaries`, `extract_binaries`,
  `assemble`, the `run_dist` call sites). This round adds no `dist.rs` lines.
- **C5 mutants: "cargo test failed in an unmutated tree".** Not caused by the patch.
  The baseline failure is `repo_hygiene_guards::scan_gitlinks_is_green_over_the_real_index`
  (`git ls-files` inside cargo-mutants' copy, which has no `.git`). The reviewer re-ran
  it with `--copy-vcs true --cap-lints true`: 11 caught, 5 missed, 1 unviable. The five
  survivors replace whole runner functions (`obtain_binaries`, `extract_binaries`,
  `assemble` ×2, `run_dist`) that need Docker. This is a harness configuration issue
  (cargo-mutants needs `--copy-vcs true` for this repo) for the human to route upstream
  (eduralph/pdca-harness), not something this patch can fix.

**Why I did not make the Docker runner testable.** A cheap-looking option is a
command-runner seam: `extract_binaries` takes a closure instead of calling `docker cp`,
and a test passes a fake that copies files. That would kill the `extract_binaries` and
`take(1)` survivors. Cost: about 15 changed lines in `dist.rs` (a new `pub` signature
plus the caller's closure) and about 40 lines of test. I ruled it out on the brief, not
on cost: "There is no test double standing in for a real build, and none should be
invented" (Production reach). A fake `docker cp` is that test double. Every break of
this kind also fails loudly, not silently: `stage_binaries` errors on a missing source,
and the extraction directory is cleared first, so no stale file can stand in.

## Self-refutation

- **(a) Genuine red? Yes.** `engine/scripts/run-verify.sh` with
  `PDCA_BUNDLE=results/issue_742` and
  `PDCA_VERIFY_BASE=origin/pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`
  (= `d9c6225`): GREEN with the patch (1 test ran, passed), then RED with every
  production file reverted and the new test kept. `every_pipeline_stage_ships_the_expected_binaries`
  FAILED with nine disagreements naming all four files: Dockerfile build and `COPY`,
  install.sh install and uninstall, release.yml run and absence, README table rows and
  install path. Verdict line: "run-verify.sh: PASS — red without the fix, green with it
  (1 test(s) ran red)."
- **(b) Production path? Yes.** The red file reads the real Dockerfile, `install.sh`,
  README and `release.yml` from the repo, the files that ship. The `dist_templates.rs`
  tests call the real `xtask::dist` functions, including `stage_tarball_tree`, the exact
  code `assemble` runs before `tar`. No copy, mock or re-implementation.
- **(c) Fixture includes the fault? Yes.** The red leg is the real single-binary
  pipeline (the reverted tree). The negative tests start from the real files and inject
  each fault class found in review, including this round's A1–A3. The staging tests use
  two dummy binaries with different bytes, so "one source copied to both" and "sources
  swapped" are both in the fixture. What the fixture does NOT include is a real image or
  a real host install. See below.

## Deferred, and the sign-off items (as the brief requires)

**Not observed in this cycle:** that a real tarball contains both binaries and that
`install.sh` puts both on a real host. Nothing in `cargo xtask ci` can build a tarball
(Docker + network, base `xtask/src/dist.rs:26-28`), and `install.sh` cannot run in a
test: it exits unless `id -u` is 0 (`deploy/dist/install.sh:90`), creates a user, writes
`/etc/wyrd` and installs units.

**Observed instead:** the table; its Rust consumers (`--host` argv, `docker cp` argv,
and real staging over dummy binaries); and every pipeline file checked against the
table. The pipeline change is fully written: the release smoke step runs the validator
and checks it is removed (`release.yml:74-77, 93`).

**At §9 the human is accepting that trade.** If it is not acceptable, the fix is not a
weaker test here. It is to run the release workflow (`workflow_dispatch`,
`release.yml:24`; e.g. `gh workflow run release.yml --repo getwyrd/wyrd --ref <branch>`)
or cut a `v*` tag, and watch the smoke step. Only the maintainer can decide that.

**Second sign-off item (brief, Alternative D):** from this merge until #743 lands (and
the endurance verdict exists), `wyrd-validate` resolves and echoes its configuration and
exits; it issues no requests. A `v*` tag cut in that window ships that stub to
operators. Nothing mechanical prevents it; the maintainer not cutting a tag does. The
README row describes what the tool is **for**, not what this build does yet. I kept
"currently a stub" out of the shipped README because that line would go stale the day
#743 lands.

**Option A** (the image carries both binaries) is a session decision recorded in the
brief and `PACKAGING-DECISION.md`, not on issue #742. Sign-off must confirm it, and
should mirror one sentence onto #742.

## Impact: the image now carries a tool that deletes objects

Three things bound it:
1. It is not the `ENTRYPOINT`, which stays `["wyrd"]` (`Dockerfile:144`). The image never
   starts it.
2. It refuses to run without an explicit `--endpoint` and without S3 credentials (no
   anonymous access, no profile or instance-metadata fallback; the adversary confirmed
   this last round against `crates/validate/src/access_keys.rs`).
3. Proposal 0017 §15 makes run-id-scoped keys a safety requirement ("it must never
   delete anything it did not create"). Today no scenario runs, so it deletes nothing.

If the maintainer later wants a slimmer image, `shipped_binaries()` is where that change
starts, and the gate then names every file that still ships the validator.

**Dependency check the brief asked for, re-run this round:** `cargo tree -p wyrd-validate
-e normal,build` on this base shows `rustls-native-certs`, `rustls-pki-types` and
`openssl-probe` (all pure Rust) and no `openssl-sys`, `ring`, `aws-lc-*`, `cc`, `cmake`,
`bindgen` or `pkg-config`. So the image's build stage needs nothing new.

## Other choices, and what I ruled out

- **install.sh header kept at the same line count.** `--help` prints a fixed range,
  `sed -n '2,16p' "$0"` (`install.sh:36`). I rewrote the 4-line block in 4 lines rather
  than change the range.
- **A `BINARIES="wyrd wyrd-validate"` loop in install.sh**: ruled out. The checker would
  have to read a variable, two loop bodies and expand `$bin` (roughly 25 more lines of
  parsing) to prove what two literal lines prove now.
- **Table as a parameter of every pure function**: ruled out for `host_build_args` /
  `image_extraction_args` (a subset at the call site would be invisible to tests); kept
  for `stage_binaries`, per the brief.
- **Making `dist --check` check the binary table**: ruled out; the brief says `--check`
  stays unchanged.
- **Pinning the validator's exit code (2) in the smoke step**: ruled out. It would tie
  the workflow to `EXIT_USAGE`'s value; "non-zero + `usage: wyrd-validate`" matches how
  the `wyrd` check is written (`release.yml:82-85`).
- **Checking the install summary names every binary**: ruled out as scope creep. The
  brief's five sites are build, copy, install, uninstall, smoke and README; the summary
  is cosmetic and now has its alignment test.

## Commit-readiness

- `cargo fmt --all` applied (no further changes); `cargo clippy -p xtask --all-targets
  -- -D warnings` clean; `typos` clean on the touched test files and `install.sh`;
  `sh -n` clean on `install.sh`.
- `shellcheck` is not installed on this host, so I could not run it. The release
  workflow runs it (`release.yml:42-43`). The only `install.sh` change this round
  restores one space inside a heredoc.
- Full gate: `./engine/xtask.sh ci` with `PDCA_WORKTREE` on the patched worktree →
  "xtask ci: all checks passed", exit 0. It ran typos, docs lint and render,
  gitlink/unsafe/blackbox guards,
  `cargo fmt --check`, clippy, build, workspace tests (1,682 passed, 0 failed, 15
  ignored across all `test result` lines, including `dist_templates` 28/28 and
  `dist_two_binary_layout` 1/1), cargo-machete, cargo-deny (advisories, bans, licenses,
  sources), conformance vectors, statics, deploy-guard and the DST tier.
- C4-verify: `engine/scripts/run-verify.sh` → PASS (see Self-refutation (a)).

No external dependency beyond the brief's "none" was needed to build or check this
slice; no NEEDS-HUMAN external-dependency marker applies.
