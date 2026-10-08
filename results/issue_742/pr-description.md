## Summary
**User impact:** `wyrd-validate` is the tool an operator runs against their
own Wyrd deployment to check that their hardware and configuration are sound.
Today operators cannot get it: the release tarball and the container image
contain only `wyrd`, and the installer knows about nothing else.

This PR ships `wyrd-validate` as the second binary in the production image
and the operator tarball, installs and uninstalls it with `install.sh`, and
adds a test that fails CI if any part of the release pipeline stops naming
both binaries.

**Depends on #845, #849 and #859**, which add the `wyrd-validate` crate, its
no-Wyrd-crates lint, and its S3 client. This branch is built on top of all
three. Merge them first.

## What to look at
- **What an operator gets.** The tarball gains `bin/wyrd-validate`. The
  installer puts it next to `wyrd` and removes it on `--uninstall`. It gets no
  systemd unit, no config file, and nothing is enabled or started: it is a
  tool you run by hand, not a service. The image still starts `wyrd` by
  default. The validator is present, not run.
- **One list of shipped binaries.** The set of binaries is now a single table
  in `xtask/src/dist.rs`. The Rust parts of the pipeline read it. The
  Dockerfile, installer, release workflow and README cannot read Rust, so they
  keep their own spelling, and a test compares each of them to the table on
  every `cargo xtask ci`.
- **The README** keeps "one `wyrd` binary serves every role" (still true: the
  validator is not a role) and adds a section on the validator, including that
  it runs on a host with no FoundationDB client.

To try it without Docker: `cargo test -p xtask --test dist_two_binary_layout
--test dist_templates --test fdb_image`. To see it fail the way the old
pipeline does, revert any one of the pipeline files to `main` and rerun the
first test. It names the file and the line that disagrees.

**Before the first release, this still needs a real run.** Nothing in
`cargo xtask ci` can build an image or run the installer as root, so those
two steps are not covered here. They are covered by:
1. the `fdb-image` workflow on this PR, which builds the image and runs both
   binaries' usage smokes inside it (it now triggers on `crates/validate/**`);
2. a manual release-workflow run before the first `v*` tag:
   `gh workflow run release.yml --repo getwyrd/wyrd --ref <this branch>`. Its
   installer smoke should show the validator running before the FoundationDB
   client is installed, `wyrd` running after, operator config kept across a
   reinstall, and both binaries gone after `--uninstall`.

## Root cause
The distribution pipeline was written for one binary and named it in five
separate places with nothing comparing them: the Dockerfile build and copy
(`deploy/docker/wyrd/Dockerfile:66`, `:122` on `main`), the image path and
staging in `xtask/src/dist.rs` (`IMAGE_BINARY_PATH` at `:41`, the hard-coded
`bin/wyrd` copy at `:559`), the installer (`deploy/dist/install.sh:115`,
`:136`), the release smoke step (`.github/workflows/release.yml:75`, `:85`),
and the README. Adding a second binary meant five edits with five chances to
miss one, and a miss would only show up at release time, or not at all (for
example an uninstall that leaves a binary behind).

## Fix
- **Table** (`xtask/src/dist.rs`): `ShippedBinary` and `shipped_binaries()`
  replace `IMAGE_BINARY_PATH`. Readers: `host_build_args` (the `--host` cargo
  argv, now `--bin wyrd --bin wyrd-validate`), `docker_cp_args` and
  `extracted_binary_path` (one `docker cp` per entry from one throwaway
  container), and `stage_binaries` (a `pub` step called from `assemble`,
  replacing the private hard-coded copy). It refuses a missing source rather
  than producing a short tarball. The extraction directory is now prepared
  before `docker create`, and `docker rm -f` still runs whatever the copies
  did, so a failed step cannot leak a container.
- **Dockerfile**: one `cargo build` naming both bins, one `COPY --from=build`
  per binary. `ENTRYPOINT ["wyrd"]` unchanged.
- **install.sh**: installs and removes both binaries; `ROLES` unchanged; the
  install summary lists the validator. The `--help` range grows with the
  header, which also fixes an old off-by-one that printed a stray `set -eu`.
- **release.yml**: the installer smoke runs the validator before the
  FoundationDB client install, `wyrd` after it, and checks both are gone after
  uninstall. The workflow's `shellcheck` step is unchanged.
- **fdb-image.yml**: a usage smoke for the validator in the built image, and
  `crates/validate/**` added to the PR path filter so the smoke runs when the
  validator changes.
- **Docs**: `deploy/dist/README.md` and
  `docs/design/architecture/07-deployment-view.md` describe both binaries.
- **Tests**:
  - `xtask/tests/dist_two_binary_layout.rs` (new) reads the real pipeline
    files and checks each against a binary set. It pins the relevant regions
    as exact text: the build and copy lines, the installer's uninstall and
    install region, the whole release smoke step, and a README line per
    binary. This means a `|| true`, an `if [ -f … ]` guard, a commented-out
    run or a moved absence check all fail. It uses no new API, so it compiles
    and fails against the old pipeline.
  - `xtask/tests/dist_templates.rs` runs that same checker over the
    production table and checks the test's local set equals it. It also runs
    the real staging step over two dummy binaries with different contents
    and checks each lands byte-for-byte at its own destination with mode
    `0755`. Further checks cover the extraction and `--host` argv, that no
    two entries share a path, and 18 single planted edits to the real files
    that must each be reported.
  - `xtask/tests/fdb_image.rs` pins the new path-filter entry and the
    validator smoke step.

## Verification
- **Claim:** every stage of the pipeline names both binaries, and drift in
  any one of them fails CI with the file named.
  - **Checked:** on this branch, `deploy/docker/wyrd/Dockerfile:78` (build,
    both bins), `:134` and `:137` (one copy each), `:150` (entrypoint
    unchanged); `deploy/dist/install.sh:117-118` (removals), `:142-143`
    (installs); `.github/workflows/release.yml:91-94` (validator smoke),
    `:99-102` (`wyrd` smoke), `:109-110` (absence after uninstall);
    `deploy/dist/README.md:9`.
  - **Test:** `xtask/tests/dist_two_binary_layout.rs`. It fails on `main`'s
    pipeline files (assertion failure naming the Dockerfile build and copy
    lines, `install.sh` line 116, `release.yml` line 71 and the README). It
    passes with this change.
- **Claim:** a third binary added to the table, with nothing else changed,
  is caught in every file that lacks it.
  - **Test:** `xtask/tests/dist_templates.rs:775`, plus the equality check at
    `:492` and the production-table run at `:506`.
- **Claim:** staging puts the right binary in the right place.
  - **Checked:** `xtask/src/dist.rs:432` (`stage_binaries`), called from
    `assemble` at `:698`.
  - **Test:** `xtask/tests/dist_templates.rs:626` (distinct payloads,
    byte-for-byte, `0755`) and `:688` (missing source refused).
- **Claim:** the image build and `--host` both produce both binaries, and
  extraction cannot leak a container.
  - **Checked:** `xtask/src/dist.rs:564` (`--host` argv from the table),
    `:635` (extraction directory prepared before `docker create`), `:644`
    (one `docker cp` per entry), `:651` (`docker rm -f` regardless).
  - **Test:** `xtask/tests/dist_templates.rs:578` (`--host` argv), `:598`
    (`docker cp` argv), `:710` and `:746` (extraction directory).
- **Whole gate:** `cargo xtask ci` passes (1,684 tests, none failed).
  `shellcheck deploy/dist/install.sh` and `sh -n` are clean, before and after.
  The validator's dependency tree adds no native library to the image build
  (no `*-sys`, `openssl`, `ring` or `aws-lc`).
- **Not covered by these tests:** the Docker calls inside `obtain_binaries`
  and the `assemble` call that hands the full table to staging. They need a
  container runtime. A change that passed only the first table entry there
  would pass CI and be caught by the release smoke (the installer's
  `install -m 0755 "$HERE/bin/wyrd-validate"` fails under `set -eu`). That
  is why the manual release run above is needed before the first tag.
- **Known limit, not in scope:** `--host` reads binaries from
  `<repo>/target/release`, as it did before this change. With
  `CARGO_TARGET_DIR` set elsewhere it can pick up stale binaries. That is
  existing behaviour, now extended to the second binary, and worth its own
  issue.

Fixes #742
