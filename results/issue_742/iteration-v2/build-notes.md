# Build notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball (iteration 2)

Target: getwyrd/wyrd @ main, built on the integration base `d9c6225` (= `stack-base`,
which carries #775 and #852). All `path:line` references are to the patched worktree
unless marked "base".

## What the patch does

**The binary set is data.** `IMAGE_BINARY_PATH` (base `xtask/src/dist.rs:41`) is gone.
In its place:

- `ShippedBinary { bin, image_path, tarball_dest }` (`xtask/src/dist.rs:271`) and
  `shipped_binaries()` (`xtask/src/dist.rs:289`): the one declaration, `wyrd` and
  `wyrd-validate`, in the `staging_plan()` style.
- `binary_source_path()` (`:308`): where a binary sits on the packaging host. Both the
  extraction target and the staging source go through it.
- `host_build_args(features)` (`:315`): the `--host` cargo argv, one build naming every
  table entry (scope (h)). Used at `:529`.
- `image_extraction_args(cid, dir)` (`:331`): the `docker cp` argv per table entry. Run by
  `extract_binaries` (`:613-628`, loop at `:621-626`).
- `stage_binaries(table, source_dir, stage)` (`:349`): the `pub` staging callable the brief
  asks for (criterion 6). Missing source → error, never a smaller tarball.
- `stage_tarball_tree(...)` (`:635`): everything `assemble` did except `tar` (template
  staging, `stage_binaries(&shipped_binaries(), …)` at `:663`, `VERSION`). `assemble`
  (`:674`) now calls it at `:693` and then tars.
- `obtain_binary` → `obtain_binaries` (`:525`). The image path creates one container,
  extracts every binary, and removes the container once whatever happened (`:605-607`),
  keeping base `:511-513`'s "remove regardless" rule for all copies. The extraction dir
  moved from the file `target/dist/wyrd.extracted` to a directory
  `target/dist/extracted/`, cleared before each run so a leftover can't stand in for a
  failed copy.

**The four pipeline files name both binaries.**

- `deploy/docker/wyrd/Dockerfile:72` builds `--bin wyrd --bin wyrd-validate` in one `RUN`;
  `:131` adds the second `COPY`; runtime-stage description updated (`:74-76`, base `:68`);
  header note `:16-19`. `ENTRYPOINT ["wyrd"]` unchanged (`:144`).
- `deploy/dist/install.sh:141` installs `bin/wyrd-validate`; `:117` removes it inside the
  `--uninstall` branch; summary line `:201`; uninstall messages `:121,123`. `ROLES`
  (`:49`) is untouched, so no unit, no env file, no `systemctl` for the validator.
- `.github/workflows/release.yml:72-77` runs `/usr/local/bin/wyrd-validate` with no
  arguments **before** the FDB client is installed (so the release also proves the
  README's "needs no `libfdb_c`" claim), expects non-zero and `usage: wyrd-validate`;
  `:93` asserts it is gone after `./install.sh --uninstall` (`:91`).
- `deploy/dist/README.md`: roles sentence kept (`:5-7`); validator introduced beside it
  (`:9-10`); a `## Binaries` table with one row per binary (`:12-17`); install, upgrade and
  verify sections updated (`:36-40`, `:60-62`, `:77-79`).

**Docs currency:** `docs/design/architecture/07-deployment-view.md:42` now lists both
binaries; `xtask/src/main.rs:92-96` (the `dist` help) too.

## The tests

`xtask/tests/dist_two_binary_layout.rs` (NEW, red-earning, names no new API):
`EXPECTED_BINARIES` (`:29`), and ONE checker over a binary set,
`pipeline_disagreements` (`:88`), built from four per-file checkers that take the file
text: `dockerfile_disagreements` (`:105`), `install_sh_disagreements` (`:191`),
`release_yml_disagreements` (`:272`), `readme_disagreements` (`:351`). One test (`:400`)
runs it over the local set. Each message starts with the file at fault. Both directions
count: a binary a file lacks, and a binary a file ships beyond the set.

`xtask/tests/dist_templates.rs` includes it as `mod layout` (`:20-21`) and adds:

- `every_pipeline_stage_ships_the_production_binary_set` (`:507`) — the same checker over
  `shipped_binaries()`.
- `the_text_tests_local_set_equals_the_production_table` (`:518`).
- `the_layout_checker_names_every_file_that_disagrees_with_the_set` (`:531`) — the exact
  diagnostic the brief describes: add a third entry and every one of the four files is
  named, and only for the new binary; drop an entry and every file is named for still
  shipping it.
- Negative tests per file, each mutating the REAL file text and asserting the checker
  reports it, iterated over the production table (`:575`, `:634`, `:687`, `:726`).
- `the_shipped_binary_table_is_consistent_and_collision_free` (`:762`) — the one literal pin
  of the names; pairwise distinct cargo targets, image paths, tarball dests, host paths.
- `the_host_build_argv_names_every_shipped_binary` (`:805`).
- `the_image_extraction_copies_each_binary_to_its_own_host_file` (`:826`).
- `stage_binaries_copies_each_binary_to_its_own_destination` (`:939`) — distinct dummy
  bytes per binary, exactly `bin/wyrd` + `bin/wyrd-validate`, each byte-equal to its own
  source, 0755; missing source is an error naming it.
- `the_tarball_tree_stages_the_plan_and_every_shipped_binary` (`:962`) — the real
  `stage_tarball_tree` over the real templates and dummy binaries: file set ==
  staging plan + table + `VERSION`.

## Iteration 1 carry-forward — what I changed

1. **Release checker counted comments and ignored order** (`iteration-v1` layout
   `:194-202`). The new checker (`dist_two_binary_layout.rs:272-348`):
   - a binary counts as run only when it is the **command** of a line (after leading
     `if`/`!`/`then`/…), so a comment (`# …` — first word is `#`) or an argument
     (`echo /usr/local/bin/x`) never counts;
   - the run must sit between the first `./install.sh` and `./install.sh --uninstall`;
   - `test ! -e <path>` must be an exact command **after** `./install.sh --uninstall`;
   - a missing install or uninstall line, or the two out of order, is itself reported.
   Both of the reviewer's false greens are now tests: "replace the invocation with
   `# TODO smoke /usr/local/bin/wyrd-validate later`" and "move the absence check to just
   after `cd /tmp/wyrd-*`" (`dist_templates.rs:575-628`), plus comment-out, argument-only,
   run-after-uninstall, and commented absence check — for each binary in the table.
2. **install.sh split on the `# ── install` banner.** Now the `--uninstall` branch is
   exactly `if [ "$UNINSTALL" = 1 ]; then` to the first unindented `fi`
   (`install.sh:98-127`), and the install path is everything after it
   (`dist_two_binary_layout.rs:191-214`). Comment lines never match a command. Tests:
   removal moved into the install path, install moved into the uninstall branch, install
   commented out, binary added to `ROLES` (`dist_templates.rs:634-681`).
3. **`obtain_binary`'s extraction list and the `assemble` call were untested.**
   - `image_extraction_args` (`dist.rs:331`) is the pure argv the runner executes; tested
     at `dist_templates.rs:826`.
   - `host_build_args` and `image_extraction_args` read `shipped_binaries()` themselves
     instead of taking the table as an argument. In iteration 1 the caller passed the
     table, so a subset at the call site (`&shipped_binaries()[..1]`) was invisible to
     tests. Now there is no call-site argument to get wrong.
   - The reviewer's surviving mutant was the `assemble` call. That call now lives in
     `stage_tarball_tree` (`dist.rs:663`), which a test drives. I applied that exact
     mutant (`stage_binaries(&shipped_binaries()[..1], …)`): red,
     `the_tarball_tree_stages_the_plan_and_every_shipped_binary` fails.
   - `stage_binaries` still takes the table, as the brief's criterion 6 says.
4. **Dockerfile checker** also tightened, though nobody flagged it: comment lines are
   dropped, `\` continuations are joined, `--bin` must be in a `RUN cargo build` of the
   `AS build` stage, and `COPY` must be in the last stage (`dist_two_binary_layout.rs:105-186`).
   Tests at `dist_templates.rs:687`.

### Hand mutations I ran (each applied, run, reverted)

| # | Mutation | Result |
|---|---|---|
| M1 | `words()` keeps comment text | survives — **equivalent for false greens**: a commented line's first word is `#`, so the command-position rule already rejects it; the cut only lets a trailing `# note` sit after a real command |
| M2 | release absence check searched in the whole step | red (`the_release_smoke_check…`) |
| M3 | release run check searched in the whole step | red (same test) |
| M4 | install.sh install/uninstall regions = whole file | red (`the_install_sh_check…`) |
| M5 | `stage_binaries(&shipped_binaries()[..1], …)` in the tree staging | red (`the_tarball_tree_stages…`) |
| M6 | `image_extraction_args` over `shipped_binaries()[..1]` | red (`the_image_extraction…`) |
| M7 | Dockerfile `COPY` accepted from any stage | red (`the_dockerfile_check…`) |
| M8 | README distinct-description check disabled | red (`the_readme_check…`) |
| M9 | `host_build_args` names only the first binary | red (`the_host_build_argv…`) |

### The two advisory gates that failed in iteration 1

- **C4-diff-cov 0.0%.** That gate measures only the added test:
  `cargo llvm-cov test -p xtask --test dist_two_binary_layout`
  (`iteration-v1/gate-logs/C4-diff-cov.log`, first line). The brief requires that file to
  name **no** symbol this patch introduces (Falsifiability) so it still compiles on the
  red leg. So it cannot reach any new line in `dist.rs`, and the gate will again report
  close to 0% for the Rust lines. That is the cost of the brief's own design, not a
  missing test. The new `dist.rs` lines are reached from `dist_templates.rs`. I measured
  that separately (`cargo llvm-cov test -p xtask --test dist_templates`, under a timeout,
  lcov compared against this patch's changed lines in `xtask/src/dist.rs`): **68 of 113
  instrumentable changed lines run (60.2%)**. All 45 misses are runner code that needs
  Docker, a host `cargo build --release`, or `tar`: `obtain_binaries` (`:525-533`,
  `:605-608`), `extract_binaries` (`:613-625`), `assemble` (`:674-700`, mostly lines that
  moved when the staging was split out), and the `run_dist` call sites (`:751-752`). Plus
  one closing brace (`:362`). Every pure function and both staging functions are fully
  executed. The `docker cp` argv the runner executes is pinned through
  `image_extraction_args`; the loop that runs it is three lines (`:621-626`).
- **C5 mutants: "cargo test failed in an unmutated tree".** Not caused by the patch. The
  baseline failure was `repo_hygiene_guards::scan_gitlinks_is_green_over_the_real_index`
  ("git ls-files -s -z must succeed", `iteration-v1/gate-logs/C5-mutants.log`):
  cargo-mutants copies the source tree without `.git`, so that test can't read the git
  index. Every dist test passed in that same baseline run. This will happen on any
  bundle that touches `xtask`. It looks like a harness issue (run mutants `--in-place`,
  or skip that one test in the mutants run) — for the human to route upstream, not
  something this patch can fix. The M1–M9 table above is the hand-run substitute.

## Self-refutation

- **(a) Genuine red?** Yes. `engine/scripts/run-verify.sh` with
  `PDCA_BUNDLE=results/issue_742`: GREEN with the patch, then RED with every production
  file reverted and the new test kept: `every_pipeline_stage_ships_the_expected_binaries`
  FAILED with nine disagreements naming all four files (Dockerfile build + copy,
  install.sh install + uninstall, release.yml run + absence, README rows + install path).
  Verdict: "PASS — red without the fix, green with it (1 test(s) ran red)".
- **(b) Production path?** Yes. The red file reads the real Dockerfile, `install.sh`,
  README and `release.yml` from the repo — the files that ship. The `dist_templates.rs`
  tests call the real `xtask::dist` functions, including `stage_tarball_tree`, which is
  the exact code `assemble` runs before `tar`. No copy, mock or re-implementation.
- **(c) Fixture includes the fault?** Yes. The red leg is the real single-binary
  pipeline (the reverted tree). The negative tests start from the real files and inject
  each fault class the reviewer found. The staging tests use two dummy binaries with
  different bytes, so "one source copied to both destinations" and "sources swapped"
  are both inside the fixture. What the fixture does NOT include is a real image or a
  real host install — see the deferral below.

## Deferred, and the sign-off item (as the brief requires)

**Not observed in this cycle:** that a real tarball contains both binaries and that
`install.sh` puts both on a real host. Nothing in `cargo xtask ci` can build a tarball
(Docker + network, base `xtask/src/dist.rs:26-28`), and `install.sh` cannot run in a test
at all: it exits unless `id -u` is 0 (`deploy/dist/install.sh:90`), creates a user, writes
`/etc/wyrd` and installs units.

**Observed instead:** the table; its Rust consumers (`--host` argv, the `docker cp` argv,
and the real staging run over dummy binaries); and every pipeline file checked against
the table. The pipeline change itself is fully written: the release smoke step now runs
the validator and checks it is removed (`release.yml:72-77, 93`).

**At §9 the human is accepting that trade.** If it is not acceptable, the fix is not a
weaker test here. It is to run the release workflow (`workflow_dispatch`,
`release.yml:24`) or cut a `v*` tag and watch the smoke step. Only the maintainer can
decide that.

**Second sign-off item (brief, Alternative D):** from this merge until #743 lands (and
the endurance verdict exists), `wyrd-validate` only resolves and echoes its
configuration and exits (`crates/validate/src/lib.rs:104-126`). A `v*` tag cut in that
window ships that stub to operators. Nothing mechanical prevents it; the maintainer not
cutting a tag does. The README row describes what the tool is **for**, not what this
build of it does yet. I chose not to put "currently a stub" in the shipped README
because that line would go stale the day #743 lands; flagging it here instead.

**Option A** (image carries both binaries) is a session decision recorded in the brief
and `PACKAGING-DECISION.md`, not on issue #742. Sign-off must confirm it.

## Impact: the image now carries a tool that deletes objects

Three things bound it:
1. It is not the `ENTRYPOINT` — that stays `["wyrd"]` (`Dockerfile:144`). The image never
   starts it.
2. It refuses to run without an explicit `--endpoint` (all ten flags are required,
   `crates/validate/src/args.rs:19-30`, parsed in `resolve_config`,
   `crates/validate/src/lib.rs:69-76`) and without S3 credentials (`NoneSet`,
   `crates/validate/src/access_keys.rs:109`; "there is no anonymous access").
3. Proposal 0017 §15 makes run-id-scoped keys a safety requirement ("it must never delete
   anything it did not create"). Today no scenario runs, so it deletes nothing.

If the maintainer later wants a slimmer image, `shipped_binaries()` is where that change
starts, and the gate then names every file that still ships the validator.

**Dependency check the brief asked for:** `cargo tree -p wyrd-validate -e normal` on this
base shows `rustls-native-certs`, `rustls-pki-types`, `openssl-probe` (all pure Rust), and
`-e normal,build` shows no `cc`, `cmake`, `bindgen`, `pkg-config`, `ring`, `aws-lc-*` or
`openssl-sys`. So the image's build stage needs nothing new. Also confirmed the build line
itself: `cargo check --release --locked --bin wyrd --bin wyrd-validate --features
"fdb,etcd"` exits 0 on this host, so a feature `wyrd-validate` doesn't declare is not an
error when both bins are named.

## Other choices, and what I ruled out

- **install.sh header kept at the same line count.** `--help` prints a fixed range,
  `sed -n '2,16p' "$0"` (`install.sh:36`). Iteration 1 grew the header by two lines,
  which would have cut the last two "Idempotent …" lines out of `--help`. I rewrote the
  4-line block in 4 lines instead of changing the range.
- **A `BINARIES="wyrd wyrd-validate"` loop in install.sh** — ruled out. It would make
  install and uninstall agree with each other, but the checker would then have to read
  a variable plus two loop bodies and expand `$bin` (roughly 25 more lines of parsing in
  the text checker) to prove the same thing two literal lines prove now. The brief
  specified per-entry lines, and the gate already catches the two drifting apart.
- **Table as a parameter of every pure function** (iteration 1's shape) — ruled out for
  `host_build_args` / `image_extraction_args`, see carry-forward item 3. Kept for
  `stage_binaries`, per the brief.
- **Making `dist --check` check the binary table** — ruled out; the brief says `--check`
  stays unchanged and never touches the table.
- **Pinning the validator's exit code (2) in the smoke step** — ruled out. It would tie
  the release workflow to `EXIT_USAGE`'s value; "non-zero + `usage: wyrd-validate`"
  matches how the `wyrd` check is written (`release.yml:82-85`).

## Commit-readiness

- `cargo fmt --all` applied; `typos` clean on every touched file; `sh -n` and `dash -n`
  clean on `install.sh`.
- `shellcheck` is not installed on this host, so I could not run it. The release workflow
  runs it (`release.yml:42-43`). My `install.sh` edits are two quoted `rm -f` / `install -m`
  lines, one heredoc line and two echo strings.
- Full gate: `PDCA_WORKTREE=… ./engine/xtask.sh ci` on the patched worktree →
  "xtask ci: all checks passed", exit 0 (typos, docs lint/render, gitlink/unsafe/blackbox
  guards, `cargo fmt --check`, clippy `-D warnings` on all targets, build, workspace
  tests, machete, deny ×3, statics, deploy-guard, DST clippy + tests). `dist_templates`
  ran 27 tests, `dist_two_binary_layout` 1, all passing.
- C4-verify: `engine/scripts/run-verify.sh` → PASS (see Self-refutation (a)).

No external dependency beyond the brief's "none" was needed to build or check this
slice; no NEEDS-HUMAN external-dependency marker applies.
