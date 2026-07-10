# Adversarial review — issue 470 (wyrd-fdb-oci-image)

Advisory only; never gates. Target: `/home/eddie/wyrd/wyrd.pdca-wt` @ `b1ccca3`
(origin/main, the `C4-verify` base). Binding gate is
`cargo test -p xtask --test fdb_image` (container-free).

## What I could NOT refute (attempted, failed)

- **Ran the binding gate green.** All 4 tests in `xtask/tests/fdb_image.rs` pass on
  target. The red is legitimate for a new-file bundle: pre-fix `read(DOCKERFILE)` panics
  because the files do not exist.
- **The consistency check is genuinely load-bearing, not vacuous.**
  `consistency_check_is_red_on_a_mismatched_fdb_version`
  (`xtask/tests/fdb_image.rs:491`) drives the *same* `check_fdb_version_consistency`
  (`:314`) the green test uses, against the *real* compose + `Cargo.toml`, with only a
  planted `FDB_VERSION=7.1.99`, and asserts the `Err` names `7.1.99`. Confirmed passing.
  I tried to find an equal-but-wrong drift that slips through: the `major_minor(v)=="7.3"`
  sanity assert (`:434`) and the three-way `df==tag`/`df_mm==tag_mm==crate_mm` checks
  (`:327`,`:336`) close that hole.
- **The `cargo build --bin wyrd --features "$FEATURES"` form is accepted at the virtual-
  workspace root** (`deploy/docker/wyrd/Dockerfile:158`). I feared a "--features not allowed
  in root of a virtual workspace" break; probing `cargo build --bin wyrd --features <x>`
  returned a feature-name error, not a workspace error, and `fdb`/`etcd` are real features on
  `crates/server/Cargo.toml:31,` so resolution holds.
- **The workflow smoke grep is correct, not the bug it looks like.**
  `.github/workflows/fdb-image.yml:81` `grep -Eq 'redb\|tikv\|fdb'`: under `-E`, `\|` is a
  *literal* pipe, and `crates/server/src/cli.rs:268` prints the literal
  `redb|tikv|fdb`, so it matches. Not a defect.

## Findings a human must adjudicate

- **NEEDS-HUMAN — the image's headline capability (multi-version `libfdb_c` loading via
  #441's `ExternalClientDirectory`) is inert on the tested base, and the binding gate never
  touches it.** `deploy/docker/wyrd/Dockerfile:186-194` bakes
  `/var/lib/wyrd/fdb/external-clients` and sets `ENV WYRD_FDB_EXTERNAL_CLIENT_DIR=...`, but
  on target `b1ccca3` **no wyrd source reads that env var or sets any
  `NetworkOption::ExternalClientDirectory`** — `crates/metadata-fdb/src/lib.rs:867`
  calls bare `foundationdb::boot()` with no options. The brief concedes this ("without 441
  the directory would be inert decoration") and defers the coupling to the #441 wave fold.
  Since the fold is not present in this checkout, confirm at sign-off that #441 actually
  landed in the folded base; if it did not, the image ships a labelled, ENV-advertised
  external-client directory that nothing loads — the exact silent-decoration failure the
  brief warns about. `xtask/tests/fdb_image.rs` asserts none of this.

- **NEEDS-HUMAN (toolchain unavailable; verdict provisional) — the runtime `cp
  /usr/lib/libfdb_c.so` (`deploy/docker/wyrd/Dockerfile:191`) assumes the
  `foundationdb-clients` `.deb` installs `libfdb_c.so` at exactly that path.** If the deb
  places it under a multiarch dir (`/usr/lib/x86_64-linux-gnu/`), the `cp` fails and the
  whole `docker build` errors — but the binding gate is a file-parse and never builds, so it
  would stay green. I cannot run `docker build` here to settle it; this must be confirmed by
  the deferred `.github/workflows/fdb-image.yml` run / builder leg (a), not scored as a
  refutation.

- **NEEDS-HUMAN — latent bash bug in the workflow's failure branch.**
  `.github/workflows/fdb-image.yml:78` `echo "expected \`wyrd\` with no subcommand..."`
  uses back-ticked ``\`wyrd\``` inside a double-quoted string, so bash runs command
  substitution on `wyrd` (not on PATH on the runner → "command not found" on stderr, empty
  substitution) instead of printing the literal word. The step still `exit 1`s correctly, so
  the guard's *outcome* is intact; only the diagnostic message is corrupted. Low impact,
  concrete, and in the deferred (ungated) workflow.

## Weak spots (not defects, noted for completeness)

- `workflow_exists_resolves_and_filters_the_fdb_surface` (`xtask/tests/fdb_image.rs:452`)
  iterates `cargo_xtask_subs`, but `fdb-image.yml` names **no** `cargo xtask` command, so the
  "every `cargo xtask <sub>` resolves" half of brief item 3 is satisfied *vacuously*. The
  `docker build -f <path>` half is exercised and does bind to `DOCKERFILE`.

## Bottom line

I attempted to refute (a) the red→green evidence, (b) the vacuity of the version-consistency
check, (c) the cargo build invocation, and (d) the smoke-grep — and could not break the
**binding gate**. Every substantive residual risk lives in the **deferred** half (the actual
`docker build`, the runtime `libfdb_c` path, and the #441-dependent external-client loading),
which the brief pre-declares as off-Check sign-off items. These are for the human to
adjudicate, not deterministic refutations of the patch.
