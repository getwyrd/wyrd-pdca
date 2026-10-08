# Adversarial review — issue 742 (dist ships `wyrd-validate`, two-binary tarball)

**Bottom line:** I tried to refute the evidence, the fix, and the verdict and could not find
a defect that would ship. There are two findings: one human call about the README wording
for the stub window, and one checker gap that is low severity and should not trigger a
rebuild. The checker gap is not tagged on purpose; reasons are in its bullet.

## Findings

- NEEDS-HUMAN [human] — `deploy/dist/README.md:17` tells operators, in the present tense,
  that `wyrd-validate` "checks that your hardware and configuration are sound by driving a
  running deployment". On this base the binary does neither. It echoes its configuration and
  says so: `crates/validate/src/lib.rs:122` prints "configuration resolved; no requests were
  issued and nothing was validated". So a `v*` tag cut before #743 ships a README that
  promises a check the shipped binary does not do. The brief's Alternative D already makes
  "packaging lands before the tool works" a §9 sign-off item, so this is not a new decision.
  The point is that the README text is part of what the maintainer accepts there. If that is
  not wanted, the fix is one sentence, e.g. "will check … once its scenarios land (#743); in
  this release it resolves and prints its configuration only". That is a judgment about the
  stub window, not a build defect.

- (Low, untagged on purpose: below the bar for a rebuild.) The release smoke checker still
  gives a false green for a step-level `continue-on-error: true`.
  `xtask/tests/dist_two_binary_layout.rs:369-436` rejects a container script without `sh -e`
  or with a `set +e`, and `xtask/tests/dist_templates.rs:727-733` pins both negative cases.
  But it never reads the step's YAML keys. I added `continue-on-error: true` under
  `- name: smoke the installer in a bookworm container` (`.github/workflows/release.yml:60`)
  in a scratch copy, and all 30 tests in `dist_templates` + `dist_two_binary_layout` passed.
  A step-level `if: false` is the same case. The release would then publish even when the
  smoke fails. This is the same "failure swallowed" class the checker hardens against
  inside the script. But it weakens every assertion in the step equally, the pre-existing
  `wyrd` ones included, so it is a property of the release workflow, not of the binary-set
  invariant this slice owns. This is the fourth round of lexical bypasses on this checker.
  The rubric says "do not iterate review rounds chasing silence", so I recommend recording
  it, or adding a one-line "the step carries no `continue-on-error` / `if:`" check in a
  follow-up, rather than spending a Do rebuild on it.

## Refutation attempts that failed (each re-run, not assumed)

- **Red→green evidence.** I re-ran both legs in scratch copies of `$PDCA_TARGET`.
  - Green: with the patch, `cargo test -p xtask --test dist_two_binary_layout --test
    dist_templates` passes 30/30.
  - Red: with every tracked file reverted (`git checkout -- .`) and only the untracked
    `xtask/tests/dist_two_binary_layout.rs` kept, the test compiles and fails at `:577`. It
    names the Dockerfile build and COPY, both install.sh sites, the smoke run and the
    absence check, and the README rows. That matches `gate-logs/C4-verify.log`.
  - The test reads the real Dockerfile, installer, workflow and README, which ARE the
    production artifacts for those stages. It is not a mirror of them and not a tautology.
- **The join to the production table.** `dist_templates.rs` runs the same checker over
  `dist::shipped_binaries()` and pins it equal to `EXPECTED_BINARIES`. The `bin` field the
  equality test skips is pinned by `the_shipped_binary_table_is_consistent_and_collision_free`
  (`image_path == /usr/local/bin/{bin}`, `tarball_dest == bin/{bin}`). Renaming the cargo
  target in `crates/validate/Cargo.toml` would also break
  `crates/validate/tests/cli_surface.rs:78` (`CARGO_BIN_EXE_wyrd-validate`) at compile time.
- **The one-line two-package build** (`deploy/docker/wyrd/Dockerfile:72`). `cargo tree
  --locked -p wyrd-server -p wyrd-validate --features fdb,etcd` resolves cleanly. The same
  features on `-p wyrd-validate` alone are refused. So the Dockerfile comment ("they reach
  `wyrd`") is accurate, and `FEATURES=""` / `tikv,etcd` take the same path.
- **The new smoke lines** (`.github/workflows/release.yml:74-77`). I built
  `wyrd-validate --release` on the host (43 s cold, so the image and compose builds grow
  only a little). With no arguments it exits 2 and prints `usage: wyrd-validate --endpoint
  …` on stderr, which the `2>&1` capture and `grep -q 'usage: wyrd-validate'` match. `ldd`
  shows only `libgcc_s` and `libc`, so running it before the FDB client is installed is
  correct and will not fail at release time. The new lines contain no `$`, backtick or `"`,
  so nothing expands early on the runner inside the outer `sh -eu -c "…"`.
- **Extraction and staging** (`xtask/src/dist.rs:525-626`, `:635-663`).
  - The container is removed whatever `extract_binaries` returns.
  - `target/dist/extracted/` is cleared before the copies.
  - `docker cp <cid>:<file> <dir>/<name>` with an existing parent creates the file.
  - Every mutation I could build in the untested `extract_binaries` loop (e.g. `.take(1)`)
    or the `--host` return path fails loudly, because `stage_binaries` errors on a missing
    source. None produces a silently smaller tarball. The earlier `assemble` mutation is
    now caught by `the_tarball_tree_stages_the_plan_and_every_shipped_binary`.
  - The release signing, provenance and upload globs are `*.tar.gz` / `SHA256SUMS`
    (`release.yml:103-123`), so the new `extracted/` directory cannot leak into them.
- **Installer.** Both sites are top-level/branch-level and unconditional. `ROLES` is
  unchanged, so there is no unit, no env file and no `systemctl` for the validator.
  `--uninstall` uses the same `$BINDIR` (read from `install-prefix`). The summary labels line
  up again (the iteration-2 nit is fixed).
- **Gate reds that are not about the patch.**
  - `C5-mutants` failed its unmutated baseline on
    `repo_hygiene_guards::scan_gitlinks_is_green_over_the_real_index`
    (`xtask/tests/repo_hygiene_guards.rs:137`, "git ls-files -s -z must succeed"). That is
    the mutants temp tree having no git index, an environment fault.
  - `C4-diff-cov` 0% is structural: it measures only the red file, which by design calls no
    `xtask::dist` API. The pure functions and staging are run by `dist_templates.rs`.
    `extract_binaries` and the `assemble`/`tar` path stay unexecuted because they need
    Docker, as the brief declares.
- **The deferred runtime half** (real image build, real tarball, privileged install on a
  host) is the brief's pre-declared §9 item. I am not re-raising it.
- **Path-filter blind spot.** `.github/workflows/fdb-image.yml:18-43` does not trigger on
  `crates/validate/**` and does not smoke the validator in the image. I could not turn this
  into a failing case: the crate is default-compiled by `cargo xtask ci`, takes none of the
  image's features, links only glibc, and its bin name and usage line are pinned by its own
  tests.
