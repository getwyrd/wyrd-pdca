# Build notes — issue 742 (iteration 8, answering the iteration 7 carry-forward)

Line numbers are for the patched tree in the cycle worktree unless marked `base:` (the
target branch at `d9c6225`, the integration base this bundle builds on).

## What this round changes, in one paragraph

The sign-off on iteration 7 accepted the production change and rejected the test design:
replace the hand-written shell model with an exact-text pin, do not forbid loops, and apply
the same pin to the installer's install/uninstall lines. That is what this round does. The
layout checker no longer reads shell at all. It builds the text each file must contain from
the binary set and compares. Test weight is roughly halved: the patch is 1,831 lines against
iteration 7's 2,996.

| file | iteration 7 | now |
|---|---|---|
| `xtask/tests/dist_two_binary_layout.rs` | 1,004 lines | 501 lines |
| `xtask/tests/dist_templates.rs` (added) | +1,197 | +525 |
| `xtask/tests/fdb_image.rs` (net) | +128 | +128 (untouched this round) |

About 100 of the 501 lines are the pinned text itself (literal copies of the installer region
and the smoke step), and 35 are module docs.

## Starting point: I re-applied iteration 7's patch

The worktree was clean at the base. The carry-forward says the production change is accepted
and "only the test design changes", so I applied `iteration-v7/patch.diff` (the brief's
carry-forward block names that directory) and reworked from there, rather than rebuild the
accepted production change from memory. I did not read iteration 7's build notes or reviews.

Production files I left exactly as iteration 7 had them: `xtask/src/dist.rs` (except one doc
sentence, below), `deploy/docker/wyrd/Dockerfile`, `deploy/dist/README.md`,
`.github/workflows/fdb-image.yml`, `docs/design/architecture/07-deployment-view.md`,
`xtask/tests/fdb_image.rs`.

## Production edits this round (small, and why each was needed)

A generated pin needs the per-binary lines to have one shape. Three small edits make that so.

1. `.github/workflows/release.yml:91-94` and `:99-102` — the capture files are now named
   after the binary (`/tmp/wyrd-validate-usage.txt`, `/tmp/wyrd-usage.txt`; they were
   `/tmp/validate-usage.txt` and `/tmp/usage.txt`). Nothing else in the two blocks changed.
2. `.github/workflows/release.yml:86-90` and `:107` — two in-script comments reworded to talk
   about "a tool shipped beside wyrd" and "every shipped binary" instead of "the validator"
   and "both binaries", so the fixed text around the generated lines stays true for a third
   binary. No quote or backtick was added.
3. `.github/workflows/release.yml:61-73` — the YAML comment above the step is rewritten. It
   used to describe the old checker's rules (no `if`, no loop, no group). It now says the step
   is pinned as exact text, that a loop is fine as long as both places change, and it keeps
   the warning about a bare `"` inside the `sh -c "…"` word.
4. `deploy/dist/install.sh:139-143` — the validator comment moved from between the two
   `install -m 0755` lines to above them, so the per-binary install lines are contiguous.
5. `xtask/src/dist.rs:76-78` — one doc sentence on `shipped_binaries()`: `wyrd` stays first,
   because the release smoke runs every later entry before the FoundationDB client is
   installed. The existing test `the_shipped_binary_table_is_well_formed`
   (`xtask/tests/dist_templates.rs:521`) already asserts `wyrd` is first.

I checked edit 1–3 with a real shell, because a text pin cannot tell a right script from a
wrong one (see "Limits"). I parsed `release.yml` with a YAML library, took the step's `run:`
text, and ran it under `bash` with a fake `docker` on `PATH` that prints its last argument.
The script the container would receive is 35 lines, ends at `test -d /etc/wyrd`, contains every
check, and passes `sh -n`. The step's only keys are `name` and `run`. So the iteration 5 defect
(a bare `"` ending the script early) is not present.

## The test design

### The red-earning file: `xtask/tests/dist_two_binary_layout.rs` (new)

Names no `xtask::dist` symbol, so it compiles against a reverted tree.

- `EXPECTED_BINARIES` (`:43`) — the local `[(&str, &str); 2]` set the brief asks for.
- `pipeline_disagreements(texts, binaries)` (`:99`) — the one checker, a plain function of a
  binary set, returning `<file>: <what>` lines.
- `after_pin` (`:132`) — finds a pinned text as whole lines in a file. On a mismatch it names
  the first line where the file stops matching: `line 143: expected …, found …`.

How each file is held:

| file | how | where |
|---|---|---|
| Dockerfile | the `RUN cargo build … --bin a --bin b …` line and one `COPY --from=build` line per binary, each an exact whole line (and not the tail of a `\`-continued line) | `:178`, `:165` |
| `install.sh` | one exact region: the whole `--uninstall` path (`if [ "$UNINSTALL" = 1 ]; then` … `exit 0` / `fi`), then the install path down to the blank line after the binary installs. The `rm -f` lines, the two summary `echo`s and the `install -m 0755` lines are generated from the set. Plus: no binary is listed in `ROLES`, and a missing `ROLES=` line is itself a finding | `:246`, `:278` |
| `release.yml` | the whole smoke step, from `- name:` to the script's closing quote, as exact text. Per binary: the run block with its usage grep, and the `test ! -e` after the uninstall. Also: the next non-blank, non-comment line after the script must open the next step | `:362`, `:389` |
| README | names each `bin/<name>` and has a line about each binary alone (unchanged from iteration 7; "README wording stays") | `:419` |

The smoke step keeps the order iteration 7 had: tools (every entry after the first) run
BEFORE the FoundationDB client is installed, `wyrd` after. `wyrd` greps for a bare `usage:`
because that is what it prints (`crates/server/src/cli.rs:480`); a tool greps for
`usage: <name>` (`crates/validate/src/args.rs:184`).

### What went into the existing `xtask/tests/dist_templates.rs`

Kept from iteration 7, untouched: the module include (`:22-23`), the local-set-equals-table
assertion (`:492`), the same checker over the production table (`:506`), the table's shape and
pairwise-distinct paths (`:521`), the `--host` argv (`:578`), the `docker cp` argv (`:598`),
the byte-for-byte staging test with distinct payloads (`:626`), the missing-binary refusal
(`:688`), and the two extraction-directory tests (`:710`, `:746`).

New this round, replacing about 890 lines of shell-model cases:

- `a_third_binary_is_named_in_every_pipeline_file_that_lacks_it` (`:775`) — the diagnostic
  criterion 5 promises, run for real: a third entry, real files, and every one of the four
  files is named with the new binary.
- `the_layout_checker_names_each_planted_drift` (`:813`) — one table of 18 single edits to
  the REAL files, plus one README case. It carries the regressions earlier rounds named:
  `|| true` on the validator install, an `if [ -f … ]` guard around it, the validator run
  commented out, the usage grep deleted or suppressed, the absence check moved before the
  uninstall, a quoted word in a script comment, `set -n`, `continue-on-error:` keyed on after
  the script, `if: false` on the step. I printed each case's finding once to confirm it fails
  for the intended reason, then removed the print.

`install_sh_reports_both_binaries_on_uninstall` is gone: the summary lines are now inside the
pinned installer region and generated from the set.

## Alternatives I ruled out, with the cost

- **Keep the shell model.** Rejected at sign-off. Cost shown above: 2,201 test lines against
  1,026 now.
- **One place for all per-binary smoke blocks (after the client install).** Saves about 4
  lines in `smoke_step` (no first/rest split, one grep pattern):

  ```
  -    for tool in tools { step.push_str(&block(tool, &format!("usage: {tool}"))); }
  -    step.push_str(SMOKE_CLIENT);
  -    step.push_str(&block(roles, "usage:"));
  +    step.push_str(SMOKE_CLIENT);
  +    for name in names { step.push_str(&block(name, "usage:")); }
  ```

  But it moves the validator's run after `dpkg -i`, so the release smoke would stop showing
  that the shipped validator runs with no FoundationDB client, and it weakens the validator's
  grep from `usage: wyrd-validate` to `usage:`. That changes what the accepted smoke proves,
  for 4 lines. Not taken.
- **Narrow installer pins** (the `rm -f` lines with one neighbour each side; the install
  lines likewise). Saves 19 literal lines (the purge prompt and the user-creation block). But
  an `exit 0` before the unit loop, or the removals moved under the `--purge` branch, would
  pass. The whole-region pin costs 19 lines and closes those. Taken: whole region.
- **Pin all of `install.sh`.** 215 lines of literal text, 170 of them about prefixes, units
  and config files, none about the binary set. Not taken; see "Limits".
- **A quote scanner on the smoke script** (iteration 5 asked the checker to model the
  `sh -c "` boundary). Iteration 7's sign-off replaces that model with the pin. With the pin, a
  quoted comment cannot be added to the workflow without the gate failing (planted case
  above). The one-off real-shell check above covers the text as it stands today.

## Limits of an exact-text pin (stated so nobody assumes more)

- It holds the reviewed text still. It does not judge it. If someone edits the workflow and
  the pinned text together and gets the quoting wrong, the gate stays green. The YAML comment
  above the step warns about exactly that.
- It sees only the pinned regions. Above the installer's uninstall path (argument parsing,
  prefix checks) and above the step (the job's own `if:`, the workflow triggers) is out of
  reach. An early `exit` planted up there is caught by the release smoke, which runs the real
  installer, not by this test.
- A deliberate edit to a pinned region must be made twice. The failure message says so and
  names the line.

## Red → green, and the three questions

Run in the worktree with `timeout 300 cargo test -p xtask --test dist_two_binary_layout`
(the command the brief's Falsifiability names), and the whole gate through the project
wrapper, `engine/xtask.sh ci`.

- **(a) Genuine red? Yes.** I reverted the nine modified files with `git apply -R`, kept the
  new test, and ran it: 2 tests ran, `every_pipeline_file_names_every_expected_binary` FAILED,
  cargo exit 101. The failure names all four files (Dockerfile build and COPY lines,
  `install.sh` line 116, `release.yml` line 71, README twice). The test compiled, so this is a
  failed assertion, not a build error. I then restored the change and confirmed the tree is
  byte-identical to `patch.diff`. Done twice: once mid-way, once on the final tree.
- **(b) Production path? Yes, with a stated gap.** The red test reads the real Dockerfile,
  `install.sh`, README and `release.yml`. `dist_templates.rs` calls the real
  `shipped_binaries`, `stage_binaries`, `host_build_args`, `docker_cp_args`,
  `extracted_binary_path` and `prepare_extraction_dir`. Nothing is mocked or copied. Not
  exercised: the Docker calls inside `obtain_binaries` (`xtask/src/dist.rs:559`) and `assemble`
  (`:659`), which need a container runtime.
- **(c) Fixture includes the fault? Yes.** The red leg reads the real single-binary files. The
  planted cases edit the real text, one edit each. The staging test uses a different payload
  per binary and has a missing-source case.

Green: `dist_two_binary_layout` 2 passed, `dist_templates` 27 passed, `fdb_image` 6 passed.
`cargo xtask ci` through `engine/xtask.sh`: "all checks passed" (typos, fmt, clippy, tests,
machete and deny all ran; none skipped). I ran it twice; the second run was on the final
tree: 1,684 tests ok, exit 0.

## The two advisory gates that failed in every earlier round

Neither is caused by the test design, and I could not fix either from inside this bundle.

- **C4 diff coverage 0%.** The gate counts only lines run by the patch's ADDED test. The brief
  requires that file to name no new API, so it cannot run a single line of `dist.rs`. The
  changed lines are run by `dist_templates.rs`, a modified file the gate does not count. The
  lines no test reaches at all are the Docker calls in `obtain_binaries` and the `assemble`
  call.
- **C5 mutants "cargo test failed in an unmutated tree".** The log shows the baseline fails in
  `xtask/tests/repo_hygiene_guards.rs:137` (`git ls-files -s -z must succeed`). cargo-mutants
  copies the tree without `.git`, so that existing test fails before any mutant runs. It is
  unrelated to this patch. That looks like a harness or target issue to raise separately.

## Not demonstrated in this cycle (the deferred half)

- No image was built, no tarball assembled, and `install.sh` was not run. The brief says Do
  must not attempt it. What this cycle shows is the table, its Rust consumers including real
  staging of dummy bytes, and every pipeline file pinned to the table.
- The tracker's literal definition of done ("the tarball contains both binaries; `install.sh`
  places both") is NOT observed here. The iteration 7 sign-off settled where that proof comes
  from: `fdb-image.yml` on the PR, and a manual `release.yml` run before the first release.
- `fdb-image.yml` gained `crates/validate/**` in its path filter and a validator usage smoke in
  iteration 5; both are unchanged and still pinned by `xtask/tests/fdb_image.rs`.

NEEDS-HUMAN external dependency: shellcheck — not installed on this host, so I could not run `shellcheck deploy/dist/install.sh`, which the release workflow runs before anything else (`.github/workflows/release.yml:42-43`). I ran `sh -n` instead (clean). The installer changes are one `rm -f` line, one `install -m 0755` line, two `echo` strings, one here-document line and comments.

```toml
[[doctor.checks]]
id    = "shellcheck"
cmd   = "shellcheck --version"
hint  = "apt-get install shellcheck — release.yml lints deploy/dist/install.sh with it; without it a builder cannot check an installer edit before the release does"
level = "WARN"
```

## Things the brief asked me to state

- **The image now carries a tool that deletes objects.** Three bounds, per the brief: it is
  not the `ENTRYPOINT` (`deploy/docker/wyrd/Dockerfile:150` is still `ENTRYPOINT ["wyrd"]`, I
  checked); it refuses to run without an explicit `--endpoint` and credentials; and run-id-
  scoped keys are a stated safety requirement (proposal 0017 §15). I verified only the first.
  The other two are the brief's statements about `crates/validate`, which this slice does not
  touch.
- **No native toolchain is added to the image build.** `cargo tree -p wyrd-validate -e normal`
  on this base lists 141 crates: no `*-sys` crate, no `ring`, no `openssl`, no `aws-lc`. The
  only TLS-related entries are `openssl-probe`, `rustls-native-certs` and `rustls-pki-types`,
  all pure Rust, as the brief's ordering note predicted.
- **What the base carries.** #738's `--chunk-size` entry is on this base and is preserved
  (`xtask/tests/dist_templates.rs:212`). #736's `WYRD_VERSION` build-arg is NOT on this base
  (`git grep WYRD_VERSION` finds nothing under `xtask`, `deploy` or `.github`), so the patch
  neither carries nor reverts it. If #736 lands first, expect a textual conflict in
  `obtain_binaries`, as the brief's `Conflicts with: 736` says.
- **Settled at the iteration 7 sign-off and not revisited:** shipping before #743, the README
  wording, option A.

## Self-review against the target's rubric

- New crate root has `#![forbid(unsafe_code)]` (`xtask/tests/dist_two_binary_layout.rs:35`).
- No clock read added (`fixture_root` uses pid plus a counter, `dist_templates.rs:477`).
- "Absent entries produce an explicit error, never a silent skip": the self-review found one
  in my own checker (a missing `ROLES=` line passed quietly). Fixed, with a planted case. An
  empty binary set, a missing default `PREFIX=`, and an unreadable pipeline file are each an
  explicit finding or a panic.
- Docs currency: `docs/design/architecture/07-deployment-view.md:42` already describes the two
  binaries and the layout test; its wording still matches.
- Workflow edits: the `fdb-image.yml` path filter change is pinned by the contract test.
- Formatter: `cargo fmt --all -- --check` clean. The target has no commit hooks beyond what
  `cargo xtask ci` runs.
- Worktree state: the new test file is marked intent-to-add (`git add -N`), so `git diff HEAD`
  in the worktree is byte-identical to `patch.diff`.

## Citations (base → patched)

- `xtask/src/dist.rs` base:41 `IMAGE_BINARY_PATH` → `ShippedBinary` `:55`, `shipped_binaries`
  `:79`; base:414 `obtain_binary` → `obtain_binaries` `:559` (host argv `:564`, extraction dir
  prepared before `docker create` `:635`, one `docker cp` per entry `:646`, `docker rm`
  regardless `:651`); base:559-564 hard-coded `bin/wyrd` copy → `stage_binaries` `:432`, called
  at `:698`. Pure helpers: `extracted_binary_path` `:379`, `host_build_args` `:387`,
  `docker_cp_args` `:403`, `prepare_extraction_dir` `:418`.
- `deploy/docker/wyrd/Dockerfile` base:66 → `:78` (both `--bin`s); base:68 → `:80`; base:122 →
  `:134` and `:137` (two COPY lines); base:118 → image description.
- `deploy/dist/install.sh` base:115 → `:117-118` (two `rm -f`); base:119,121 → `:122,124`
  (summary names both); base:136 → `:142-143` (two installs); base:36 → `:38` (`--help` range
  follows the longer header); base:195 → `:202-203` (install summary).
- `.github/workflows/release.yml` base:59 → step at `:74`; base:75-78 → `:99-102`; validator
  block new at `:91-94`; base:85 → `:109-110`.
- `.github/workflows/fdb-image.yml` base:42 → `:47` (path filter); smoke step new at `:102`.
- `deploy/dist/README.md` base:1-8 kept; second binary introduced after it; base:12-18, 27,
  49, 60, 67 updated to say "both binaries".
- `xtask/tests/fdb_image.rs` base:321-330 → exact `paths:` entries and the pinned validator
  smoke step (`:358`).
