## Summary
**User impact:** an operator looking at a running `wyrd s3` gateway has no way to tell
which build it is. The release tarball has a `VERSION` file, but the binary itself knows
nothing about where it came from, so "is this gateway running the release I installed?"
cannot be answered from the process or its logs.

This PR bakes a build identity into the `wyrd` binary and logs it when the `s3` role
starts. A binary built by `cargo xtask dist` reports the same string as its tarball's
`VERSION`; a hand-built binary reports the commit it was built from.

## What to look at
The change has three parts that only make sense together:

- **The binary learns its identity at compile time.** A new build script for the server
  crate works it out and bakes it in.
- **The release pipeline hands its version in.** The image build cannot see `.git`, so
  `cargo xtask dist` now passes the version it already writes to `VERSION` into the image
  build. The binary and the tarball get the same string from one place.
- **The `s3` role logs it** as a new `version` field on its `role started` event.

To try it on any checkout:

```
cargo build --bin wyrd
target/debug/wyrd s3 --s3-listen 127.0.0.1:0 --data-dir "$(mktemp -d)" \
  --access-key k --secret-key s --log-format json
```

The `role started` line on stderr now has a `version` field. On an untagged commit it
looks like `0.0.0+git.<short sha>`. Building with `WYRD_VERSION=1.2.3` set reports
`1.2.3` instead.

## Root cause
Nothing compiled into `wyrd` carried any version: `crates/proto/build.rs` was the tree's
only build script, and the `s3` role's `role started` event recorded only `role`,
`listen`, `region` and `dservers`. `dist` derived a version (`derive_version` →
`normalize_describe`) but used it only for the tarball and image labels, and the image
build that compiles the shipped binary excludes `.git/` (`.dockerignore`), so the binary
could not derive the same thing on its own.

## Fix
- **`crates/server/build.rs`** (new) bakes `WYRD_BUILD_IDENTITY`, read back as
  `wyrd_server::version::BUILD_IDENTITY` (`crates/server/src/version.rs:48`). It resolves
  the identity in this order:
  1. `WYRD_VERSION` from the build environment, used as-is after validation (a value that
     is not UTF-8 or not a legal Docker tag fails the build);
  2. otherwise `git describe --tags --always` of the workspace's **own** repository
     (discovery is stopped at the workspace root and inherited `GIT_DIR`-style overrides
     are cleared, so a checkout nested in another repo never picks up the outer commit).
     There is **no `--dirty`**: a hand-built binary names its base commit, not a claim
     that the tree was clean;
  3. otherwise `0.0.0+git.unknown`, with a `cargo:warning`. A build in this state
     re-checks on every build, so it recovers as soon as git can answer.
- **One shared definition.** `normalize_describe`, the validator and the resolver now
  live in `crates/server/src/version/derivation.rs`, which the build script, the server
  crate and xtask all compile (`xtask/src/dist.rs:181`, a `#[path]` module, so the
  dependency runs tooling → product and never the other way).
- **The hand-off.** `image_build_args` (`xtask/src/dist.rs:207`) is pulled out of the
  image build as a pure function and adds `--build-arg WYRD_VERSION=<version>`
  (`:226`), using the same `version` binding written to `VERSION` (`:715`). The `--host`
  build sets the same variable (`:614`). The Dockerfile declares `ARG WYRD_VERSION=""`
  (`deploy/docker/wyrd/Dockerfile:49`) and re-declares it right before `cargo build`
  (`:89`) so a new version only invalidates that layer. An empty value falls through to
  the next step and never bakes an empty identity.
- **Rebuild cost.** The build script re-runs only when `HEAD`, the refs, or the shallow
  boundary change. It does not watch the index or source files, so editing code or
  running `git status` does not relink `wyrd-server` and its test binaries.
- **The log line.** `cmd_s3` adds `version = crate::version::BUILD_IDENTITY` to the
  `role started` event (`crates/server/src/cli.rs:2398`). The `d-server` and `custodian`
  roles are unchanged here.
- `docs/design/architecture/07-deployment-view.md` describes the identity and what it
  does and does not claim.

## Verification
- **Claim:** a spawned `wyrd s3` logs a `version` that is real and came from git, not a
  constant.
  - **Checked:** `crates/server/src/cli.rs:2390-2399` on `main` + this patch — the field
    sits on the same `role started` event the role already emits.
  - **Test:** `crates/server/tests/build_identity_startup_log.rs`
    (`the_s3_role_logs_the_build_identity_derived_from_git`, `:199`). It spawns the real
    `wyrd` binary, reads the ephemeral port from stderr, and kills the child on every
    exit path. It requires `version` to be non-empty, not `unknown`, not `0.0.0`, and to
    contain the short sha from its own independent `git rev-parse`. **Fails without the
    fix** (production reverted, test kept: *"the `role started` event carries no string
    `version` field"*), **passes with it**:
    `cargo test -p wyrd-server --test build_identity_startup_log`.
- **Claim:** `dist` and the binary use one normalizer.
  - **Checked:** `xtask/src/dist.rs:181` compiles `crates/server/src/version/derivation.rs`;
    `dist.rs` no longer defines its own `normalize_describe`.
  - **Test:** the existing `normalize_describe_covers_all_three_shapes`
    (`xtask/tests/dist_templates.rs:336`) passes **unmodified**;
    `the_build_identity_derivation_is_one_file_with_two_consumers` (`:377`).
- **Claim:** the image build receives the same string `dist` writes to `VERSION`, and the
  Dockerfile accepts it in the build stage.
  - **Checked:** `xtask/src/dist.rs:207-226`, `:638`, `:715`;
    `deploy/docker/wyrd/Dockerfile:49`, `:89-90`.
  - **Test:** `the_image_build_hands_the_version_to_the_binary` (`:403`, asserts on the
    pure argument list, no Docker), `the_dockerfile_declares_the_version_arg_in_the_build_stage`
    (`:443`), `dist_validates_its_version_with_the_shared_rule` (`:476`).
- **Claim:** an override is used verbatim (`.dirty` included), git-derived values never
  carry `.dirty`, and bad input fails loudly instead of becoming another identity.
  - **Checked:** `crates/server/src/version/derivation.rs:77` (`validate_identity`),
    `:142` (`override_from_env`), `:160` (`describe_text`), `:189` (`resolve`).
  - **Test:** 17 unit tests in `derivation.rs`, run in both the server crate and xtask.
- **Claim:** the build script re-runs when it should and not otherwise.
  - **Test:** real cargo builds of a probe crate that uses this exact `build.rs`
    (`crates/server/src/version/build_script_tests.rs`, unix only):
    an edit and `git status` do not re-run it but a commit does (`:383`); a full clone
    made shallow and back rebuilds each time (`:308`); a build with no repository
    re-checks until one exists (`:426`); a tag that is not UTF-8 still names the commit
    by its sha (`:459`); an override is baked verbatim and a non-UTF-8 one fails the
    build (`:496`).
- `cargo xtask ci` passes on the patched tree.

## Known limits (not addressed here)
- **Tarball/binary equality is shown as wiring, not end to end.** That needs a full
  `cargo xtask dist` (Docker and network), which is outside `ci`. What is tested is that
  `dist` passes the very string it writes to `VERSION` into the build. A local
  `cargo xtask dist`, then comparing `target/dist/wyrd-*/VERSION` with the extracted
  binary's logged `version`, would show the pair directly.
- **Compose-built images report `0.0.0+git.unknown`.**
  `deploy/small-multi-node-fdb/docker-compose.yml` builds this Dockerfile without
  `WYRD_VERSION`. The build warns, but passing the argument through compose is left as a
  follow-up.
- **Tag names with Unicode whitespace.** The describe output is trimmed with `str::trim`,
  so a deliberately odd tag such as `v1.2.3` followed by a no-break space reports as
  `1.2.3`, the same as a real `v1.2.3`. Accepted as a known gap; a follow-up will strip
  only git's line ending.
- On an untagged checkout the identity has the `0.0.0+git.<sha>` shape. That is a real
  build identity, but not a release version; release-shaped values appear once a tag
  exists.
- A non-UTF-8 `WYRD_VERSION` set on an *incremental* build that previously had it unset
  is not noticed, because cargo's own rerun check reads it as unset and never calls the
  build script. Fresh builds (every image build) do fail on it, and `dist` only passes
  validated values.

Fixes #778
