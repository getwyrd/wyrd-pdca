# Build notes — issue 742 (iteration 9, answering the iteration 8 carry-forward)

Line numbers are for the patched tree in the cycle worktree unless marked `base:` (the
target branch at `d9c6225`, the integration base this bundle builds on).

## Result

`shellcheck deploy/dist/install.sh` reports nothing. I changed nothing.

- `patch.diff` is byte-identical to `iteration-v8/patch.diff` (`cmp` clean; both sha256
  `2f16dd4eebaa3052b971d02d03b7ee8c6a59ed2d9864584af8f7151d37c61b69`; 1,831 lines).
- `dist_two_binary_layout.rs` in the bundle is byte-identical to last round's copy.
- The external-dependency flag I raised last round for shellcheck is withdrawn. The tool is
  on the host now (`/usr/bin/shellcheck`, version 0.11.0) and I ran it.

The carry-forward said: run shellcheck on the installer and change nothing else unless it or
Check finds something. The v7 rework, the production change and the v7 sign-off decisions
are settled. So this round is a verification round, and everything below the "Carried
forward" heading describes a patch that has not moved.

## shellcheck: what I ran and what came back

All commands run from the worktree root.

| command | result |
|---|---|
| `shellcheck deploy/dist/install.sh` — the exact command the release runs (`.github/workflows/release.yml:43`; no flags, and the repo has no `.shellcheckrc`) | exit 0, no output |
| `shellcheck -f json deploy/dist/install.sh` | `[]`, exit 0 |
| `shellcheck --severity=style --shell=sh deploy/dist/install.sh` | exit 0, no output |
| same command on the base file (`git show HEAD:deploy/dist/install.sh`) | exit 0, no output |
| a scratch copy with `rm -f $BINDIR/wyrd-validate` (unquoted) appended | SC2086, exit 1 |
| `sh -n deploy/dist/install.sh` | exit 0 |

The fifth row is there to show the tool really checks this file and this dialect
(`#!/bin/sh`): a clean result is a real "nothing found", not a skipped file. The fourth row
shows the patch neither added nor hid a finding.

The lines the patch touches in the installer, for reference: header `:5-10`; the `--help`
range `:38`; the two `rm -f` lines `:117-118`; the two uninstall messages `:122`, `:124`; the
comment `:139-141`; the two `install -m 0755` lines `:142-143`; the summary row `:203`.

## Extra checks I ran with shellcheck now available (none led to a change)

These were not asked for. They cost a minute each and cover the scripts the installer is
exercised by.

1. **Every `run:` block of the two workflows the patch edits.** I parsed `release.yml` and
   `fdb-image.yml` with a YAML library, wrote each step's `run:` text to a scratch file with
   a `#!/bin/bash` first line (GitHub runs `run:` under `bash -e` on Linux), and linted
   each. Ten blocks, all exit 0: release steps 1 to 5 (including the smoke step,
   `release.yml:74`) and fdb-image steps 1 to 5 (including the new validator smoke,
   `fdb-image.yml:102`). The smoke step's only keys are still `name` and `run`.
2. **The script the smoke container receives.** I ran the smoke step's `run:` text under
   `bash -e` with a fake `docker` on `PATH` that prints its last argument. The container
   would get 34 lines ending at `test -d /etc/wyrd`, with every check in it; `sh -n` passes.
   So the iteration 5 defect (a bare `"` ending the script early) is still absent.
   shellcheck on that text gave one warning, and it is not a finding:
   - SC2164 on `cd /tmp/wyrd-*` ("use `cd … || exit`"). It appears only when the text is
     linted without the `-eu` flags the container shell is started with
     (`release.yml:79`, `sh -eu -c "…"`). With `set -eu` as the first line, shellcheck
     reports nothing, because `-e` already stops the script on a failed `cd`.
   - The line is on the base (`base: .github/workflows/release.yml:68`) and the patch only
     carries it as context. The repo's CI does not lint this inner script.
   I left it alone.
3. **`install.sh --help`.** The header grew by two lines and the `sed` range by one
   (`'2,16p'` → `'2,17p'`, `:38`). That is right: I ran both. The patched script prints the
   header and stops at its last line. The base script printed one line too many, a stray
   `set -eu`, so the base range was off by one and the patch happens to correct it.

## Starting point

The worktree was clean at the base. "Change nothing else" means the previous patch is the
input, so I applied `iteration-v8/patch.diff` (the directory the carry-forward names) with
`git apply`. I also read `iteration-v8/build-notes.md`, my own notes, so the unchanged
sections below stay complete for sign-off. I did not read `SUMMARY.md` or any `check-*` file
from any iteration.

## Red → green, and the three questions (re-run this round)

Run in the worktree with `timeout 900 cargo test -p xtask --test dist_two_binary_layout` (the
command the brief's Falsifiability names, with a timeout), and the whole gate through the
project wrapper, `engine/xtask.sh ci`.

- **(a) Genuine red? Yes.** I reverted the nine modified files (`git apply -R` of the diff
  minus the new test), kept the new test, and ran it: it compiled, 2 tests ran,
  `every_pipeline_file_names_every_expected_binary` FAILED, cargo exit 101. The failure names
  all four files: the Dockerfile build line and the validator `COPY` line; `install.sh`
  line 116 (expected the validator `rm -f`); `release.yml` line 71 (expected the validator
  block); the README twice. A failed assertion, not a build error. I then re-applied the
  change and confirmed `git diff HEAD` is byte-identical to `patch.diff`.
- **(b) Production path? Yes, with a stated gap.** The red test reads the real Dockerfile,
  `install.sh`, README and `release.yml`. `dist_templates.rs` calls the real
  `shipped_binaries`, `stage_binaries`, `host_build_args`, `docker_cp_args`,
  `extracted_binary_path` and `prepare_extraction_dir`. Nothing is mocked or copied. Not
  exercised: the Docker calls inside `obtain_binaries` (`xtask/src/dist.rs:559`) and the
  `assemble` call (`:698`), which need a container runtime.
- **(c) Fixture includes the fault? Yes.** The red leg reads the real single-binary files.
  The planted cases edit the real text, one edit each. The staging test uses a different
  payload per binary and has a missing-source case.

Green, with the fix applied: `dist_two_binary_layout` 2 passed, `dist_templates` 27 passed,
`fdb_image` 6 passed.

Whole gate, `engine/xtask.sh ci`, on the final tree: `xtask ci: all checks passed`, exit 0;
1,684 tests passed, 0 failed, 15 ignored. Steps that ran, none skipped: typos, docs lint,
docs render and link audit, the gitlink / unsafe / blackbox guards, `cargo fmt --all --
--check`, clippy, build, test, cargo-machete, cargo deny (three invocations), conformance,
statics, deploy-guard, and the DST clippy and tests. The formatter check is clean, and the
target has no commit hooks beyond what this gate runs.

## Carried forward from iteration 8 (the patch is unchanged, so this still holds)

Where a number or claim below was re-checked this round, it says so. The rest is restated
from last round's notes.

### What the patch does

- `xtask/src/dist.rs`: the shipped-binary set is one table, `shipped_binaries()` (`:79`,
  type `ShippedBinary` `:55`). Its Rust readers: the `--host` argv (`host_build_args`,
  `:387`, used at `:564`), the extraction (`docker_cp_args` `:403`,
  `extracted_binary_path` `:379`, one `docker cp` per entry at `:644`), and staging
  (`stage_binaries` `:432`, called from `assemble` at `:698`). The extraction directory is
  prepared before `docker create` (`prepare_extraction_dir` `:418`, called at `:635`), and
  `docker rm -f` runs whatever the `docker cp`s did (`:651`). All re-checked by grep.
- Dockerfile: one build naming both bins (`:78`), two `COPY --from=build` lines (`:134`,
  `:137`), `ENTRYPOINT ["wyrd"]` unchanged (`:150`). Re-checked.
- `install.sh`, `release.yml`, `fdb-image.yml`, README, and the architecture doc
  (`docs/design/architecture/07-deployment-view.md:42`): each names both binaries.

### The test design

The red-earning file, `xtask/tests/dist_two_binary_layout.rs` (new), names no `xtask::dist`
symbol, so it compiles against a reverted tree.

- `EXPECTED_BINARIES` (`:43`) — the local `[(&str, &str); 2]` set the brief asks for.
- `pipeline_disagreements(texts, binaries)` (`:99`) — the one checker, a plain function of a
  binary set, returning `<file>: <what>` lines.
- `after_pin` (`:132`) — finds a pinned text as whole lines in a file. On a mismatch it names
  the first line where the file stops matching.

| file | how it is held | where |
|---|---|---|
| Dockerfile | the `RUN cargo build … --bin a --bin b …` line and one `COPY --from=build` line per binary, each an exact whole line | `:178`, `:165` |
| `install.sh` | one exact region: the whole `--uninstall` path, then the install path down to the blank line after the binary installs. The `rm -f` lines, the two uninstall messages and the `install -m 0755` lines are generated from the set. Also: no binary is listed in `ROLES`, and a missing `ROLES=` line is itself a finding | `:246`, `:278` |
| `release.yml` | the whole smoke step as exact text. Per binary: the run block with its usage grep, and the `test ! -e` after the uninstall. Also: the next non-blank, non-comment line after the script must open the next step | `:362`, `:389` |
| README | names each `bin/<name>` and has a line about each binary alone | `:419` |

In the existing `xtask/tests/dist_templates.rs`: the module include (`:22-23`), the
local-set-equals-table assertion (`:492`), the same checker over the production table
(`:506`), the table's shape and pairwise-distinct paths (`:521`), the `--host` argv
(`:578`), the `docker cp` argv (`:598`), the byte-for-byte staging test with distinct
payloads (`:626`), the missing-binary refusal (`:688`), two extraction-directory tests
(`:710`, `:746`), the third-binary diagnostic (`:775`), and a table of 18 single edits to the
real files that the checker must each name (`:813`; count re-checked). Those 18 include
`|| true` on the validator install, an `if [ -f … ]` guard around it, the validator run
commented out, the usage grep deleted, the absence check moved before the uninstall, a
quoted word in a script comment, and `set -n`.

### Limits of an exact-text pin

- It holds the reviewed text still. It does not judge it. If someone edits the workflow and
  the pinned text together and gets the quoting wrong, the gate stays green. The YAML comment
  above the step warns about that (`release.yml:61-73`).
- It sees only the pinned regions. An early `exit` planted above the installer's uninstall
  path is caught by the release smoke, which runs the real installer, not by this test.
- A deliberate edit to a pinned region must be made twice. The failure message says so and
  names the line.

### Alternatives ruled out, with the cost

- **Keep the hand-written shell model** (iteration 7). Rejected at sign-off. 2,201 test lines
  against 1,026 now.
- **Run every per-binary smoke block after the client install.** Saves about 4 lines in
  `smoke_step`, but the release smoke would stop showing that the validator runs with no
  FoundationDB client, and its grep would weaken from `usage: wyrd-validate` to `usage:`.
- **Narrow installer pins** (each `rm -f` or install line with one neighbour). Saves 19
  literal lines, but an `exit 0` before the unit loop, or the removals moved under `--purge`,
  would pass. Taken instead: the whole region.
- **Pin all of `install.sh`.** 215 lines of literal text, about 170 of them unrelated to the
  binary set.

### The two advisory gates that failed in earlier rounds

Not re-run by me this round; the explanation is from iteration 8 and the patch has not moved.

- **C4 diff coverage 0%.** The gate counts only lines run by the patch's ADDED test. The
  brief requires that file to name no new API, so it cannot run a line of `dist.rs`. The
  changed lines are run by `dist_templates.rs`, a modified file the gate does not count.
- **C5 mutants "cargo test failed in an unmutated tree".** The baseline fails in
  `xtask/tests/repo_hygiene_guards.rs:137` (`git ls-files -s -z must succeed`), because
  cargo-mutants copies the tree without `.git`. Unrelated to this patch.

### Not demonstrated in this cycle (the deferred half)

- No image was built, no tarball assembled, and `install.sh` was not run as an installer. The
  brief says Do must not attempt it. shellcheck is a lint: it says the script has no known
  shell hazards, not that it places both binaries on a host.
- The tracker's literal definition of done ("the tarball contains both binaries; `install.sh`
  places both") is NOT observed here. The iteration 7 sign-off settled where that proof comes
  from: `fdb-image.yml` on the PR, and a manual `release.yml` run before the first release.

### Things the brief asked me to state

- **The image now carries a tool that deletes objects.** Three bounds, per the brief: it is
  not the `ENTRYPOINT` (`deploy/docker/wyrd/Dockerfile:150`, re-checked); it refuses to run
  without an explicit `--endpoint` and credentials; and run-id-scoped keys are a stated
  safety requirement (proposal 0017 §15). I verified only the first. The other two are the
  brief's statements about `crates/validate`, which this slice does not touch.
- **No native toolchain is added to the image build.** Re-checked: `cargo tree -p
  wyrd-validate -e normal` on this base lists 141 crates, with no `*-sys` crate, no `ring`,
  no `openssl`, no `aws-lc`. The only TLS-related entries are `openssl-probe`,
  `rustls-native-certs` and `rustls-pki-types`, all pure Rust. The gate's blackbox-guard
  also passed.
- **What the base carries.** Re-checked: #738's `--chunk-size` entry is on this base and is
  preserved (`xtask/tests/dist_templates.rs:212`). #736's `WYRD_VERSION` build-arg is NOT on
  this base (`git grep WYRD_VERSION` finds nothing under `xtask`, `deploy` or `.github`), so
  the patch neither carries nor reverts it. If #736 lands first, expect a textual conflict
  in `obtain_binaries`, as the brief's `Conflicts with: 736` says.
- **Settled at the iteration 7 sign-off and not revisited:** shipping before #743, the README
  wording, option A.

## Self-review against the target's rubric (re-read this round)

- Crate roots forbid unsafe: `xtask/tests/dist_two_binary_layout.rs:35`; the gate's
  unsafe-guard passed.
- No clock read added. No shared mutable global added (the statics gate passed).
- Absent entries are an explicit error: `stage_binaries` fails on a missing source
  (`xtask/src/dist.rs:432`); the checker reports a missing `ROLES=` line, an empty set, and a
  missing default `PREFIX=`.
- Docs currency: `docs/design/architecture/07-deployment-view.md:42` describes the two
  binaries and the layout test, in the same patch.
- Workflow edits: the `fdb-image.yml` path filter gained `crates/validate/**` (`:47`), pinned
  by `xtask/tests/fdb_image.rs`. `release.yml` triggers are untouched.
- Await discipline: no new unbounded wait. `docker rm -f` stays best-effort after the copies.

## One process suggestion (not a blocker)

`pdca.toml` has no doctor row for shellcheck. Adding one (detect command `shellcheck
--version`) would report a missing shellcheck before a cycle spends a round on it, which is
what cost this bundle a round.

## Scratch files

Everything I wrote outside the worktree and the bundle is under `$PDCA_SCRATCH`, named
`pdca-builder-742-*`: the base and planted installer copies, three diffs, ten extracted
workflow scripts, a `-smoke/` directory with the fake `docker`, the gate log and the
dependency list. A few hundred KB in total. I ran no `rm`; the harness owns that root.

## Citations (base → patched)

- `xtask/src/dist.rs` base:41 `IMAGE_BINARY_PATH` → `ShippedBinary` `:55`, `shipped_binaries`
  `:79`; base:414 `obtain_binary` → `obtain_binaries` `:559` (host argv `:564`, extraction dir
  prepared before `docker create` `:635`, one `docker cp` per entry `:644`, `docker rm`
  regardless `:651`); base:559-564 hard-coded `bin/wyrd` copy → `stage_binaries` `:432`, called
  at `:698`. Pure helpers: `extracted_binary_path` `:379`, `host_build_args` `:387`,
  `docker_cp_args` `:403`, `prepare_extraction_dir` `:418`.
- `deploy/docker/wyrd/Dockerfile` base:66 → `:78` (both `--bin`s); base:68 → `:80`; base:122 →
  `:134` and `:137` (two COPY lines).
- `deploy/dist/install.sh` base:115 → `:117-118` (two `rm -f`); base:119,121 → `:122,124`
  (messages name both); base:136 → `:142-143` (two installs); base:36 → `:38` (`--help` range
  follows the longer header); base:195 → `:202-203` (install summary).
- `.github/workflows/release.yml` base:41-42 → shellcheck step `:42-43`; base:59 → step at
  `:74`; base:75-78 → `:99-102`; validator block new at `:91-94`; base:85 → `:109-110`.
- `.github/workflows/fdb-image.yml` base:42 → `:47` (path filter); smoke step new at `:102`.
- `deploy/dist/README.md` base:1-8 kept; second binary introduced at `:9`; later mentions at
  `:38` and `:81`.
- `xtask/tests/fdb_image.rs` base:321-330 → exact `paths:` entries and the pinned validator
  smoke step.
