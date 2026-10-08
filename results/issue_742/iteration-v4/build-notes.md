# Build notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball (iteration 4)

Target: getwyrd/wyrd @ main, built on the integration base `d9c6225` (= `stack-base`,
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, which carries #775 and #852).
All `path:line` references are to the patched worktree unless marked "base".

This round re-applies the iteration-3 patch (it applied cleanly to `d9c6225`) and changes
only the release-smoke checker and its tests, which is what the round-3 finding asked
for. No production file changed between iteration 3 and this one: `xtask/src/dist.rs`,
the Dockerfile, `install.sh`, the README, `release.yml`, `main.rs` and the architecture
doc are byte-identical to `iteration-v3/patch.diff`. The v3→v4 delta is in
`xtask/tests/dist_two_binary_layout.rs` and `xtask/tests/dist_templates.rs` only.

## Iteration 3 carry-forward — what changed

**The finding:** the smoke checker counted a binary as smoke-tested when its run line
was in command position with no `||`/`&&`, but never looked at the `grep -q 'usage: …'`
lines (`.github/workflows/release.yml:77`, `:85`). Those greps are what actually fail the
smoke when a binary cannot run: `if <path> …; then exit 1; fi` skips its body when the
binary dies with a loader error (exit 127), so without the grep the step passes. The
reviewer listed four mutations that left all 29 tests green.

**The fix** (`release_yml_disagreements`, `xtask/tests/dist_two_binary_layout.rs:369`).
For each binary, the smoke step must now:

1. **Run it and keep the exit status** (`:447-470`). The run is a line at the container
   script's own level, between `./install.sh` and `./install.sh --uninstall`, that is
   either `<path> >FILE …` (bare: `sh -e` checks the status) or `if [!] <path> >FILE …;
   then` whose body has `exit N` with N ≠ 0 (`:460-466`). A `|` pipeline, a `||`/`&&`
   list, or an `&` job does not count (`foreground`, `:94`). A bare `! <path>` does not
   count either, because POSIX `sh -e` ignores the status of a `!` pipeline (`:450-455`:
   only `if !` is unwrapped). The run must capture stdout to a file (`stdout_file`,
   `:102`).
2. **Assert on that run's output** (`:473-477`, `:486-492`). After the run there must be
   a bare `grep -q 'usage:…' FILE` (`greps_usage`, `:128`) on the same file, at the
   script's own level. It must come before `./install.sh --uninstall` and before any
   other line writes FILE. The pattern must start with `usage:`. A pattern that only
   names the binary is not enough, because a loader error ("`<path>`: error while loading
   shared libraries") names it too.
3. **Run under errexit throughout** (`:416-437`). The line that opens the container
   script (`docker run … sh -eu -c "`, `release.yml:65`) or a `set -e` before
   `./install.sh` must turn errexit on, and no `set +e` may turn it off. Every bare check
   above depends on this, so it is checked first.
4. The absence check `test ! -e <path>` must also sit at the script's own level now
   (`:494-496`), not only after the uninstall and inside the script.

**Why the rule is `usage:…` and not `usage: <name>`:** `wyrd` prints a bare `usage:` line
and then its subcommands (`crates/server/src/cli.rs:480`), while `wyrd-validate` prints
`usage: wyrd-validate …` (`crates/validate/src/args.rs:184`). The base smoke for `wyrd`
greps `'usage:'` (`release.yml:85`, unchanged from base). Requiring `usage: <name>` would
force a `wyrd` grep that fails on `wyrd`'s real output.

**The reviewer's four mutations, applied to the real `release.yml` and run** (scratch
copy, then restored byte-for-byte):

| # | Mutation (reviewer's wording) | Result now |
|---|---|---|
| 1 | delete `release.yml:77` | red: "never asserts on what `/usr/local/bin/wyrd-validate` printed" |
| 2 | delete `:77` and `:85`, both `exit 1` bodies → `:` | red: "never runs … with its exit status checked", both binaries |
| 3 | `:74-77` → `/usr/local/bin/wyrd-validate 2>&1 \| cat >/dev/null` | red: "never runs …" |
| 4 | `:74-77` → `/usr/local/bin/wyrd-validate &` | red: "never runs …" |
| 5 | (mine) `sh -eu -c` → `sh -u -c` | red: "does not run under `sh -e` throughout" |

Each made 5 of 29 `dist_templates` tests fail, plus the red file's own test.

**New negative tests** (`the_release_smoke_check_wants_each_run_kept_and_its_output_asserted`,
`xtask/tests/dist_templates.rs:717`). They start from the real file and run per binary:
the reviewer's mutations 1–4 (`:771-772`, `:802-813`); the grep made non-fatal, pointed
at `/dev/null`, looking for the binary's name instead of the usage (`:787`), nested under
`if [ -s FILE ]`, or preceded by a forged `echo 'usage:' >FILE` (`:799`); the `if` body
without an `exit` (`:803-806`); a bare `! <path>` run (`:815`); no `-e`, and a `set +e`
(`:724-733`). There is
also one positive case: a bare `<path> >FILE 2>&1` run followed by its grep is accepted.
`replace_lines` (`:527`) is the range-edit helper these need. The existing release test
gained "absence check nested under an `if`" (`:692-699`).

### Hand mutations of the new checker rules (each applied, run, reverted)

| # | Mutation of `dist_two_binary_layout.rs` | Result |
|---|---|---|
| H7 | `foreground` ignored (`:455`) | red |
| H8 | output assertion always satisfied (`:486`) | red |
| H9 | `if` body never checked (`:465`) | red |
| H10 | errexit check disabled (`:431`) | red |
| H11 | grep accepted on any file (`:131`) | red |
| H12 | a write to FILE no longer stops the grep search (`:476`) | red |
| H13 | a nested grep counts (`:475`) | red |
| H14 | bare `! <path>` accepted as a run (`:453`) | red |
| H15 | `set +e` ignored (`:429`) | red |
| H16 | any grep pattern accepted (`:134`) | **survived at first**, so I added the "grep by name" negative case; now red |
| H17 | absence check accepted at any depth (`:495`) | **survived at first**, so I added the "nested absence" negative case; now red |

My first run of H7, H8, H9 and H11 printed nothing. They had failed to compile, because
workspace lints make warnings errors (`Cargo.toml:251`) and those edits left a variable
unused. I rewrote them to keep every variable used, re-ran them, and the table above
shows the real results.

**What the text checker still cannot see.** It reads indentation as nesting and words as
commands. A determined edit can still fool it one level down, for example
`true || exit 1` as the `if` body, a `cp other FILE` forging the capture, or a body that
only exits on one branch of a nested `case`. I stopped at the drifts that come from
ordinary edits: silencing a failing step, deleting the assertion, piping or
backgrounding a run, pasting a check in the wrong place, adding a defensive `if`, and
turning errexit off.

## What the patch does (unchanged from iteration 3)

**The binary set is data.** `IMAGE_BINARY_PATH` (base `xtask/src/dist.rs:41`) is gone.

- `ShippedBinary { bin, image_path, tarball_dest }` (`xtask/src/dist.rs:271`) and
  `shipped_binaries()` (`:289`): the one declaration, `wyrd` and `wyrd-validate`, in the
  `staging_plan()` style.
- `binary_source_path()` (`:308`): where a binary sits on the packaging host. The
  extraction target and the staging source both go through it.
- `host_build_args(features)` (`:315`): the `--host` cargo argv, one build naming every
  table entry (scope (h)), used at `:529`.
- `image_extraction_args(cid, dir)` (`:331`): the `docker cp` argv per entry, run by
  `extract_binaries` (`:613`).
- `stage_binaries(table, source_dir, stage)` (`:349`): the `pub` staging callable
  (criterion 6). A missing source is an error, never a smaller tarball.
- `stage_tarball_tree(...)` (`:635`): everything `assemble` did except `tar`, including
  `stage_binaries(&shipped_binaries(), …)` at `:663`. `assemble` (`:674`) calls it, then
  runs `tar`.
- `obtain_binary` → `obtain_binaries` (`:525`): one container, every binary extracted,
  the container removed once whatever happened (`:605-606`). This keeps base
  `:511-513`'s "remove regardless" rule for every copy. Extraction goes to
  `target/dist/extracted/`, cleared first so a leftover can't stand in for a failed copy.

**The four pipeline files name both binaries.**

- `deploy/docker/wyrd/Dockerfile:72` builds `--bin wyrd --bin wyrd-validate` in one
  `RUN`; `:131` adds the second `COPY`; runtime-stage description `:74-76` (base `:68`);
  header note `:16-19`. `ENTRYPOINT ["wyrd"]` is unchanged (`:144`).
- `deploy/dist/install.sh:141` installs `bin/wyrd-validate`; `:117` removes it inside the
  `--uninstall` branch; summary row `:201`; uninstall messages `:121`, `:123`. `ROLES`
  (`:49`) is untouched: no unit, no env file, no `systemctl` for the validator.
- `.github/workflows/release.yml:74-77` runs `/usr/local/bin/wyrd-validate` with no
  arguments **before** the FDB client is installed, so the release also checks the
  README's "needs no `libfdb_c`" claim. It expects a non-zero exit and
  `usage: wyrd-validate`. `:93` asserts the binary is gone after
  `./install.sh --uninstall` (`:91`).
- `deploy/dist/README.md`: the roles sentence is kept (`:5-7`); the validator is
  introduced beside it (`:9-10`); a `## Binaries` table has one row per binary
  (`:12-17`); the install, upgrade and verify sections are updated (`:36-40`, `:60-62`,
  `:77-79`).

**Docs currency:** `docs/design/architecture/07-deployment-view.md:42` lists both
binaries, and so does `xtask/src/main.rs:92-96` (the `dist` help).

## The tests

`xtask/tests/dist_two_binary_layout.rs` (NEW, earns the red, names no new API):
`EXPECTED_BINARIES` (`:31`) and ONE checker over a binary set, `pipeline_disagreements`
(`:153`). It is built from four per-file checkers over file text:
`dockerfile_disagreements` (`:170`), `install_sh_disagreements` (`:258`),
`release_yml_disagreements` (`:369`) and `readme_disagreements` (`:526`). One test
(`:575`) runs it over the local set. Each message starts with the file at fault. A
binary a file lacks and a binary a file ships beyond the set both count. The file has no
crate-only inner attribute (only the `forbid(unsafe_code)` lint), so `dist_templates.rs`
can include it as a module.

`xtask/tests/dist_templates.rs` includes it as `mod layout` (`:21-22`) and adds:

- `every_pipeline_stage_ships_the_production_binary_set` (`:542`): the same checker
  over `shipped_binaries()`.
- `the_text_tests_local_set_equals_the_production_table` (`:553`).
- `the_layout_checker_names_every_file_that_disagrees_with_the_set` (`:566`): the
  brief's exact diagnostic. Add a third entry and all four files are named, only for
  the new binary. Drop an entry and every file is named for still shipping it.
- Negative tests per file, mutating the REAL file text, iterated over the production
  table: release (`:613`, `:717`), install.sh (`:830`), Dockerfile (`:907`), README
  (`:946`).
- `the_shipped_binary_table_is_consistent_and_collision_free` (`:982`): pairwise
  distinct cargo targets, image paths, tarball destinations and host source paths.
- `the_host_build_argv_names_every_shipped_binary` (`:1025`).
- `the_image_extraction_copies_each_binary_to_its_own_host_file` (`:1046`).
- `stage_binaries_copies_each_binary_to_its_own_destination` (`:1159`): distinct dummy
  bytes per binary, exactly `bin/wyrd` + `bin/wyrd-validate`, each byte-equal to its own
  source, mode 0755. A missing source is an error naming it.
- `the_tarball_tree_stages_the_plan_and_every_shipped_binary` (`:1182`): the real
  `stage_tarball_tree` over the real templates and dummy binaries.
- `install_sh_summary_paths_line_up` (`:310`): from iteration 3 (the dropped space).

## The two advisory gates that failed in iterations 1–3

Neither can go green on this patch without breaking the brief's own design or changing
code outside this slice. I'm stating it here so the sign-off reads them correctly.

- **C4-diff-cov will report 0% again. That follows from the brief's design and is not a
  reach gap.** The gate measures "under the patch's OWN test"
  (`engine/scripts/run-diff-cov.sh:54-55`), which is `--test dist_two_binary_layout`, and
  it leaves test files out of the count (`:25-26`). The brief requires that file to name
  **no** symbol this patch adds, so that it compiles on the red leg (Falsifiability).
  So it cannot reach any new line in `dist.rs`. The only base `xtask::dist` entry point
  it could call, `run_dist(["--check"])`, never touches the binary table (brief, Impact).
  The new `dist.rs` code is driven from `dist_templates.rs`. I measured that this round
  with `cargo llvm-cov test -p xtask --test dist_templates`: **68 of 113**
  instrumentable changed `dist.rs` lines (60.2%). The missed spans are `:525-533`
  (`obtain_binaries --host`, needs a host release build), `:605-608` and `:613-625`
  (`docker cp` extraction), `:674-700` (`assemble`'s triple, cleanup and `tar`) and
  `:751-752` (`run_dist` call sites). Every one of them needs Docker, a host
  `cargo build --release`, or `tar`. The remaining `:362` is the closing brace of an
  `if let Some(parent)` whose `None` arm cannot happen.
- **C5 mutants: "cargo test failed in an unmutated tree".** The patch does not cause
  this. The baseline failure is
  `repo_hygiene_guards::scan_gitlinks_is_green_over_the_real_index`
  (`xtask/tests/repo_hygiene_guards.rs:137`, `git ls-files -s -z must succeed`).
  cargo-mutants copies the tree without `.git`, so `git ls-files` fails there
  (`iteration-v3/gate-logs/C5-mutants.log`). In round 2 the reviewer re-ran it with
  `--copy-vcs true --cap-lints true`: 11 caught, 5 missed, 1 unviable. The five
  survivors replace whole runner functions that need Docker. This is a harness
  configuration issue (cargo-mutants needs `--copy-vcs true` for this repo) for the
  human to route upstream (eduralph/pdca-harness), not something this patch can fix.

**Why I did not make the Docker runner testable** (unchanged reasoning). A
command-runner seam (`extract_binaries` takes a closure instead of calling `docker cp`)
would cover `:613-625`. It would cost about 15 changed lines in `dist.rs` plus about 40
lines of test. I ruled it out on the brief, not on cost: "There is no test double
standing in for a real build, and none should be invented" (Production reach). A fake
`docker cp` is that test double. The pure argv it would run is already pinned
(`the_image_extraction_copies_each_binary_to_its_own_host_file`), and any break in the
runner fails loudly: `stage_binaries` errors on a missing source, and the extraction
directory is cleared first.

## Self-refutation

- **(a) Genuine red? Yes.** I ran `engine/scripts/run-verify.sh` with
  `PDCA_BUNDLE=results/issue_742` and
  `PDCA_VERIFY_BASE=origin/pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`
  (= `d9c6225`) on this round's final `patch.diff`. It was GREEN with the patch (1 test
  ran and passed), then RED with every production file reverted and the new test kept:
  `every_pipeline_stage_ships_the_expected_binaries` FAILED with nine disagreements
  naming all four files (Dockerfile build and `COPY`, install.sh install and uninstall,
  release.yml run and absence, README rows and install path). Verdict line:
  "run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red)." The
  base `wyrd` smoke block produced no release message on the red leg, so the stricter
  rules accept the shape the base already uses.
- **(b) Production path? Yes.** The red file reads the real Dockerfile, `install.sh`,
  README and `release.yml` from the repo, which are the files that ship. The
  `dist_templates.rs` tests call the real `xtask::dist` functions, including
  `stage_tarball_tree`, the exact code `assemble` runs before `tar`. No copy, mock or
  re-implementation is involved.
- **(c) Fixture includes the fault? Yes.** The red leg runs against the real
  single-binary pipeline (the reverted tree). The negative tests start from the real
  files and inject each fault class found in review, including all four of this round's
  mutations, which I also applied to the real `release.yml` (table above). The staging
  tests use two dummy binaries with different bytes, so "one source copied to both" and
  "sources swapped" are both in the fixture. The fixture does NOT include a real image or
  a real host install. See below.

## Deferred, and the sign-off items (as the brief requires)

**Not observed in this cycle:** that a real tarball contains both binaries, and that
`install.sh` puts both on a real host. Nothing in `cargo xtask ci` can build a tarball
(it needs Docker and a network, base `xtask/src/dist.rs:26-28`). `install.sh` cannot run
in a test: it exits unless `id -u` is 0 (`deploy/dist/install.sh:90`), creates a user,
writes `/etc/wyrd` and installs units.

**Observed instead:** the table; its Rust consumers (the `--host` argv, the `docker cp`
argv, and real staging over dummy binaries); and every pipeline file checked against the
table. The pipeline change is fully written: the release smoke step runs the validator,
asserts its usage, and checks that it is removed (`release.yml:74-77`, `:93`).

**At §9 the human is accepting that trade.** If it is not acceptable, the fix is not a
weaker test here. It is to run the release workflow (`workflow_dispatch`,
`release.yml:24`; e.g. `gh workflow run release.yml --repo getwyrd/wyrd --ref <branch>`)
or cut a `v*` tag, and watch the smoke step. Only the maintainer can decide that. Part of
it will show up on the PR anyway: `fdb-image.yml` path-filters on `deploy/docker/wyrd/**`
(deferred finding from round 3), so the PR runs a real `docker build` of the two-binary
Dockerfile. Check that job's result at sign-off.

**Second sign-off item (brief, Alternative D):** from this merge until #743 lands (and
the endurance verdict exists), `wyrd-validate` resolves and echoes its configuration,
then exits; it sends no requests. A `v*` tag cut in that window ships that stub to
operators. Nothing mechanical prevents it; only the maintainer not cutting a tag does.
The README row describes what the tool is **for**, not what this build does yet. I kept
"currently a stub" out of the shipped README because that line would go stale the day
#743 lands.

**Option A** (the image carries both binaries) is a session decision recorded in the
brief and `PACKAGING-DECISION.md`, not on issue #742. Sign-off must confirm it, and
should mirror one sentence onto #742.

## Impact: the image now carries a tool that deletes objects

Three things limit the risk:
1. It is not the `ENTRYPOINT`, which stays `["wyrd"]` (`Dockerfile:144`). The image never
   starts it.
2. It refuses to run without an explicit `--endpoint` and S3 credentials: no anonymous
   access, and no fallback to a profile or instance metadata (the adversary confirmed
   this in round 2 against `crates/validate/src/access_keys.rs`).
3. Proposal 0017 §15 makes run-id-scoped keys a safety requirement ("it must never delete
   anything it did not create"). Today no scenario runs, so it deletes nothing.

If the maintainer later wants a slimmer image, `shipped_binaries()` is where that change
starts, and the gate then names every file that still ships the validator.

**Dependency check the brief asked for, re-run this round:** `cargo tree -p wyrd-validate
-e normal,build` on `d9c6225` lists 141 crates, including `rustls-native-certs`,
`rustls-pki-types` and `openssl-probe` (all pure Rust). It has no `openssl-sys`, `ring`,
`aws-lc-*`, `cc`, `cmake`, `bindgen` or `pkg-config`, and no `wyrd-*` crate other than
itself. So the image's build stage needs nothing new.

## Other choices, and what I ruled out

- **`grep -q 'usage: <name>'` as the required output assertion** (the reviewer's first
  suggestion): ruled out, because `wyrd`'s usage starts with a bare `usage:` line
  (`crates/server/src/cli.rs:480`), so the base `wyrd` grep (`release.yml:85`) would have
  to change to something that fails on real output. The rule is "a pattern starting
  `usage:`, on the file this run captured", which both binaries meet and a loader error
  cannot.
- **Pinning the `if` body to exactly `exit 1`:** ruled out in favour of "any `exit N`
  with N ≠ 0". The exit code value doesn't matter, only that the step fails.
- **install.sh header kept at the same line count.** `--help` prints a fixed range,
  `sed -n '2,16p' "$0"` (`install.sh:36`), so I rewrote the 4-line block in 4 lines
  rather than change the range.
- **A `BINARIES="wyrd wyrd-validate"` loop in install.sh**: ruled out. The checker would
  have to read a variable and two loop bodies and expand `$bin` (roughly 25 more lines of
  parsing) to prove what two literal lines prove now.
- **Making `dist --check` check the binary table**: ruled out; the brief says `--check`
  stays unchanged.

## Commit-readiness

- `cargo fmt --all --check` clean; `cargo clippy -p xtask --all-targets -- -D warnings`
  clean; `typos` clean on both test files, `release.yml` and `deploy/dist/`; `sh -n`
  clean on `install.sh`.
- `shellcheck` is not installed on this host, so I could not run it. The release
  workflow runs it (`release.yml:42-43`). `install.sh` is unchanged since iteration 3.
- Full gate on the final patch: `./engine/xtask.sh ci` with `PDCA_WORKTREE` on the
  patched worktree printed "xtask ci: all checks passed" and exited 0. Summing every
  `test result` line gives 1,683 passed, 0 failed, 15 ignored, including
  `dist_templates` 29/29 and `dist_two_binary_layout` 1/1.
- C4-verify: `engine/scripts/run-verify.sh` → PASS (see Self-refutation (a)).

No external dependency beyond the brief's "none" was needed to build or check this
slice, so no NEEDS-HUMAN external-dependency marker applies.
