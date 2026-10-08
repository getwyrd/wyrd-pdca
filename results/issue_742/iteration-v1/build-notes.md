# Build notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball

Target: getwyrd/wyrd @ main, worktree base `d9c6225` (pdca-integrate of #852 on top of the
earlier waves). Line numbers below are on the patched tree unless marked "base".

## What changed, and where

**The binary set is data** — `xtask/src/dist.rs`
- `ShippedBinary { bin, image_path, tarball_dest }` (`:269`) and `shipped_binaries()`
  (`:287`), written the same way as `staging_plan()` (`:201`). Two entries: `wyrd` and
  `wyrd-validate`. The lone `IMAGE_BINARY_PATH` const (base `:41`) is gone.
- `binary_source_path(source_dir, &b)` (`:306`) — the pure function that says where each
  binary sits on the packaging host. Extraction (`extract_binaries`, `:593`) writes to it
  and staging (`stage_binaries`) reads from it, so they cannot disagree.
- `host_build_args(&table, features)` (`:312`) — the `--host` argv: one `cargo build
  --release --locked --bin wyrd --bin wyrd-validate --features …` (scope (h); it builds
  both, it does not refuse). `obtain_binaries` (`:504`) uses it and returns
  `target/release` as the source dir.
- `stage_binaries(&table, source_dir, stage)` (`:328`) — `pub`, replaces the four
  hard-coded lines in the private `assemble` (base `:559-564`); `assemble` now calls it
  (`:656`). Copies each to `tarball_dest`, mode 0755; a missing source is an error.
- Image path: one `docker create`, then `extract_binaries` does one `docker cp` per entry
  into a freshly cleared `target/dist/extracted/`. The container is removed once
  (`:586`) whatever `extract_binaries` returned — same "remove regardless" discipline as
  base `:511-513`, now covering every copy and the directory setup.
- Base check: #736's `--build-arg WYRD_VERSION` is NOT present in `obtain_binary` on this
  base (`d9c6225`) — grep finds no `WYRD_VERSION` in `xtask/src/dist.rs`. So there was
  nothing of #736's to preserve in that function. #738's `--chunk-size` assertion in
  `dist_templates.rs` (`:188-214`) is present and untouched.

**Pipeline files**
- `deploy/docker/wyrd/Dockerfile:72` builds `--bin wyrd --bin wyrd-validate`; `:131`
  copies the validator to `/usr/local/bin/wyrd-validate`; runtime-stage description
  (`:74-76`) and a header paragraph (`:16-19`) updated. `ENTRYPOINT ["wyrd"]` unchanged.
- `deploy/dist/install.sh:142` installs `$HERE/bin/wyrd-validate`; `:118` removes it on
  `--uninstall`; messages at `:122,124,202` and header `:5-10` updated. `ROLES` unchanged
  (`:51`), no unit, no env file, no new `systemctl`.
- `deploy/dist/README.md:6-18` — the roles sentence is kept as-is; a new "Binaries" table
  gives each binary its own row (roles binary vs validator: optional, blackbox, no
  `libfdb_c`, no unit/config, refuses without `--endpoint` + credentials). Install
  section `:36-37` names both paths.
- `.github/workflows/release.yml:72-77` runs `wyrd-validate` with no args and expects a
  non-zero exit plus `usage: wyrd-validate` — placed BEFORE the FDB client is installed,
  which also shows on a real host that it needs no `libfdb_c`. `:93` asserts it is gone
  after uninstall. The no-args behaviour is from `crates/validate/src/lib.rs:98-131`
  (parse error → `EXIT_USAGE` = 2, message carries `usage()` from `args.rs:179-185`).
- `docs/design/architecture/07-deployment-view.md:42` — the living doc listed the tarball
  as `bin/wyrd` only; updated (rubric "docs currency").

## Tests — the split the brief asked for

- `xtask/tests/dist_two_binary_layout.rs` (NEW, red-earning). Names no `xtask::dist`
  symbol. Holds `EXPECTED_BINARIES: [(&str, &str); 2]` (`:23`) and ONE checker,
  `pipeline_disagreements(&[(in-image path, tarball dest)]) -> Vec<String>` (`:54`), which
  returns one message per disagreement, each prefixed with the file at fault. It checks,
  for every entry: Dockerfile `RUN cargo build` has `--bin <name>` and a `COPY
  --from=build /src/target/release/<name> <image_path>` line; `install.sh` has the
  install line after the `# ── install` banner and the `rm -f` line before it, and the
  name is not in `ROLES`; the release smoke step runs `/usr/local/bin/<name>` and has
  `test ! -e /usr/local/bin/<name>`; the README table has a non-empty row for
  `bin/<name>` and names `<prefix>/bin/<name>`. It also reports binaries a stage ships
  BEYOND the set (extra `--bin`, `COPY`, install/rm lines, README rows) — "disagree" in
  both directions. A second test pins the README's kept roles sentence. No crate-only
  inner attributes (`#![forbid(unsafe_code)]` is valid as a module attribute too).
- `xtask/tests/dist_templates.rs` includes it (`:20-21`, `#[path] mod layout;`) and adds:
  the same checker over the production table (`:477`); local set == production table
  (`:489`); a non-vacuity test (`:501`) that adds a fake third binary and asserts all four
  files are named, and drops `wyrd-validate` and asserts the extra is reported; table
  shape + pairwise-distinct bins / image paths / tarball dests / host extraction paths
  (`:539`); the `--host` argv (`:580`); and the real `stage_binaries` over a tempdir
  (`CARGO_TARGET_TMPDIR`) with `roles-binary` / `validator-binary` dummies, asserting
  byte-equality per destination, 0755, and that a missing source errors (`:604`).

## Red → green (run with `timeout … cargo test -p xtask …`, the brief's named command)

- GREEN (patch applied): `dist_two_binary_layout` 2/2 pass; `dist_templates` 22/22 pass
  (includes the 2 included layout tests). Full `cargo xtask ci`: "all checks passed"
  (the first run caught a clippy `redundant_guards` in my test, fixed). After that run I
  made comment/wrap-only edits to the Dockerfile and README; I re-ran `cargo test -p
  xtask` (all suites ok), `cargo fmt --all -- --check` (ok) and `cargo xtask dist
  --check` (ok) on the final tree, but not the whole `ci` again.
- RED (production files reverted, new test kept — what `run-verify.sh` does): 2 tests
  ran, 2 FAILED. Output named all four files: Dockerfile build + copy, install.sh install
  + uninstall, release.yml run + absence, README rows + install path. Not a compile
  failure — the file touches no new API.
- I did not run `engine/scripts/run-verify.sh` myself: it creates its own `../wyrd-verify`
  worktree outside the roots I'm allowed to write. Check runs it.

## Refuting my own test

- **(a) Genuine red?** Yes. With `Dockerfile`, `install.sh`, `README.md`, `release.yml`,
  `dist.rs`, `dist_templates.rs` and the arch doc reverted to base and only
  `dist_two_binary_layout.rs` kept: `test result: FAILED. 0 passed; 2 failed`, with nine
  per-file disagreements printed. Re-applying the patch: green.
- **(b) Production path?** Yes. The text test reads the real repo files the release builds
  from. The `dist_templates.rs` tests call the real `xtask::dist::shipped_binaries`,
  `binary_source_path`, `host_build_args` and `stage_binaries` — the same functions
  `obtain_binaries` / `assemble` call (`dist.rs:504-520`, `:656`). No copy of the logic.
- **(c) Fixture includes the fault?** Yes. The fault is "a stage is missing a binary"; the
  red leg runs against the real single-binary files. The staging test uses two binaries
  with different bytes, so "same source copied twice" or "swapped" fails; the non-vacuity
  test injects an unshipped third binary and checks every file reports it.

## Deferred — not demonstrated in this cycle (sign-off item, per the brief)

**Not observed:** that a real tarball contains both binaries and that `install.sh` puts
both on a real host — the tracker's literal definition of done. Nothing in `cargo xtask
ci` can build a tarball (needs Docker + network, `xtask/src/dist.rs:33-35`, base
`:26-28`), and `install.sh` refuses non-root (`install.sh:92`) and writes `/etc/wyrd`,
users and units. **What IS observed:** the table; its Rust consumers including the real
staging step; and every pipeline file pinned to it by the gate. The release workflow's
smoke step is extended to cover the second binary, but it only runs on a `v*` tag or a
`workflow_dispatch` (`release.yml:20-23`). If the maintainer wants the literal DoD seen
before accepting, the way is to run `release.yml` via `workflow_dispatch` — the
maintainer's call. I did not build an image (the brief forbids it).

Also unverified here: `shellcheck deploy/dist/install.sh` — shellcheck is not installed on
this host. `sh -n` parses it fine, the change is two lines in the style of the existing
ones, and the release workflow runs shellcheck (`release.yml:41-42`). The workflow YAML
parses (`python3 yaml.safe_load`).

## Image posture (the brief asked this be stated)

The production image now carries a tool that deletes objects. Bounded by: (1) it is not
the `ENTRYPOINT` (`Dockerfile:144`, still `["wyrd"]`); (2) it refuses to run without an
explicit `--endpoint` and credentials — all ten flags are required
(`crates/validate/src/args.rs:16`, `:173`); (3) proposal 0017 §15 requires run-id-scoped
keys, "it must never delete anything it did not create". If the image should be slimmed
later, the change starts at `shipped_binaries()` and the gate names the files that follow.

## Dependency checks

- `cargo check --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd`
  succeeds on this tree — the exact two-package, `--features` shape the Dockerfile and
  `--host` now use.
- `cargo tree -p wyrd-validate -e normal`: the only TLS-adjacent crates are
  `rustls-native-certs`, `rustls-pki-types`, `openssl-probe` — all pure Rust, no
  `openssl-sys` / `ring` / `aws-lc`. Matches the brief's corrected note: no native toolchain
  needed beyond the build stage's.

## Alternatives I ruled out

- **A shell loop in install.sh over a `BINARIES="wyrd wyrd-validate"` variable.** Would
  cut the shell's own duplication to one list, but the checker would then need to parse
  the variable AND confirm the loops sit in the right branches — more fragile text
  matching for a two-entry set. The brief describes the sites as per-binary lines; I kept
  literal lines and check them by exact match on the correct side of the `# ── install`
  banner.
- **Keeping `IMAGE_BINARY_PATH` and adding a second const.** That is the shape the brief's
  invariant says to remove (`brief.md` "SELF-TEST").
- **Putting the checker in a helper under `xtask/tests/`.** The brief notes the C4
  classifier treats every added `tests/*.rs` as a discriminator test; included as a module
  from `dist_templates.rs` instead.

## Open items for the human (not NEEDS-HUMAN blockers)

- Confirm option A at §9 and mirror it onto issue #742 (brief header).
- Accept the deferred DoD above and the "packaging lands before the tool works" risk
  (brief, Alternative D) — or reject back to Plan.
