# Build notes — issue 778 / wyrd-build-identity-derived-and-logged

Target: getwyrd/wyrd, worktree `$PDCA_WORKTREE` at `df68932` (the stacked integration base
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, which already carries #742).
Line numbers below are on that tree with the patch applied unless marked "base".

**Base drift from the brief's citations.** The brief cites `origin/main@a801997`. This base
already has #742 merged, so the cited lines moved: `normalize_describe` was at
`xtask/src/dist.rs:176-212` (base), `derive_version` at `:499-508` (base), the image argv in
`obtain_binaries` at `:571-631` (base), the `VERSION` write at `:700-705` (base), the
Dockerfile `ARG FEATURES=""` at `:41` / `ARG FEATURES` in the build stage at `:47` / the
`RUN cargo build` at `:78` (base), and the `role started` event at
`crates/server/src/cli.rs:2389-2397` (base). #742 renamed `obtain_binary` → `obtain_binaries`
and builds two binaries in one `cargo build`; nothing in it conflicts with this slice.

## What changed and why

1. **One derivation file, three compiled consumers** — `crates/server/src/version/derivation.rs`
   (new). Holds `normalize_describe` (moved verbatim from `xtask/src/dist.rs`, base `:176-212`),
   `validate_identity` (`:71`), and the pure rung resolver `resolve` (`:143`), plus
   `FALLBACK_BASE = "0.0.0"` and `MAX_IDENTITY_LEN = 100`. Std-only, no I/O.
   * `crates/server/build.rs` compiles it with `#[path = "src/version/derivation.rs"] mod derivation;`
   * `crates/server/src/version.rs` declares it as `pub mod derivation;` (so its unit tests run in
     the product crate under `cargo test -p wyrd-server`)
   * `xtask/src/dist.rs:181-186` compiles it with
     `#[path = "../../crates/server/src/version/derivation.rs"] pub mod identity;` and
     `pub use identity::normalize_describe;`, so `dist::normalize_describe` keeps its path and the
     existing test `normalize_describe_covers_all_three_shapes` (`xtask/tests/dist_templates.rs:336`)
     passes **unmodified**.

   **Choice: `#[path]` module, not `include!`.** The brief suggested `include!`. I used
   `#[path = …] mod` because it is the same idea (one file compiled into several crates) but lets
   the shared file carry its own `//!` module doc and a `#[cfg(test)] mod tests` (an `include!`d
   file cannot hold inner attributes or inner docs). The repo already uses `#[path]` this way
   (`xtask/tests/dist_templates.rs:22`). Dependency direction: xtask reads a product source file;
   the product never names xtask. xtask does NOT gain a dependency on the `wyrd-server` crate
   graph. Rejected alternative: `xtask` depending on `wyrd-server` as a crate — that would compile
   tokio/tonic/axum/opentelemetry and every backend into xtask just to reach a 30-line function.
   Rejected alternative: a new tiny workspace crate (`wyrd-build-identity`) depended on by both
   `wyrd-server` (as build-dependency + dependency) and `xtask` — cleaner on paper, but it is a new
   workspace member with its own `Cargo.toml`, `#![forbid(unsafe_code)]` root, `Cargo.lock` entry
   and deny-graph row (~4 files, ~40 extra lines) for no behavioural gain over `#[path]`.

   Side effect: the derivation's unit tests also run a second time inside xtask's lib tests (the
   file is compiled there too). Harmless and cheap (pure functions).

2. **Build script** — `crates/server/build.rs` (new, `#![forbid(unsafe_code)]` at `:16` for the
   #616 crate-root guard). Thin caller: reads `WYRD_VERSION`; when it is unset/empty, runs
   `git describe --tags --always` (**no `--dirty`**, Decision 1) and `git rev-parse --short HEAD`;
   passes them to `derivation::resolve`; emits `cargo:rustc-env=WYRD_BUILD_IDENTITY=…` and any
   warnings as `cargo:warning=`. An invalid explicit `WYRD_VERSION` panics (build error).
   * **Scoped to the workspace's own repository**: every git call runs at the workspace root
     with `GIT_CEILING_DIRECTORIES=<root's parent>` and inherited `GIT_DIR`/`GIT_WORK_TREE`/…
     removed, AND `--show-toplevel` must canonicalize to the workspace root. Proven by hand: a
     copy of the workspace without `.git` placed inside an unrelated repo (outer HEAD `9a7414a`)
     baked `0.0.0+git.unknown`, not `0.0.0+git.9a7414a`.
   * **Re-run watch** = `WYRD_VERSION` + (when git is consulted) `<git-dir>/HEAD`,
     `<common-dir>/refs/`, and `<common-dir>/packed-refs` only if it exists (cargo treats a
     missing watched path as always stale). NOT the index, NOT sources. Proven by hand: after
     `git status`, `cargo build -v` reports `Fresh wyrd-server`.

3. **Exposed constant** — `crates/server/src/version.rs` (new): `pub const BUILD_IDENTITY: &str =
   env!("WYRD_BUILD_IDENTITY");` — the exact path `wyrd_server::version::BUILD_IDENTITY` #779
   needs. Module registered at `crates/server/src/lib.rs:22`. The module doc states the limit as
   the brief requires: "a hand-built binary names its base commit; a released one carries
   `dist`'s word", and that the identity is not an attestation of an unmodified tree.

4. **Logged** — `crates/server/src/cli.rs:2393`: `version = crate::version::BUILD_IDENTITY` added
   to the `s3` role's `role started` event, beside `role`/`listen`/`region`/`dservers`. d-server
   and custodian events untouched (out of scope).

5. **dist hand-off** — `xtask/src/dist.rs`:
   * `image_build_args` (`:207`, new, pure): the `docker buildx build` argv extracted from
     `obtain_binaries`, now with `--build-arg WYRD_VERSION={version}` (`:225-226`) next to
     `FEATURES`. All original comments kept. `versioned_image_tag` (`:197`) extracted so the argv
     builder and `docker create` share one spelling. `obtain_binaries` now calls these
     (`:627-638`); behaviour otherwise unchanged (the OCI dir is still created before the build).
   * `--host` path: the host `cargo build` gets `.env("WYRD_VERSION", version)` (`:614`). Not
     named in the brief's scope list, but the invariant says the binary's name must be the one
     its artifact carries, and a `--host` tarball is also an artifact. One line.
   * `derive_version` (`:539`) now validates its result with the shared `validate_identity` and
     errors early and legibly — the brief's "validated at derivation time, where the error is
     cheap". It still uses `--dirty`, so `VERSION` on a dirty tree still says `.dirty`, and rung 1
     passes that verbatim into the binary.
   * The `version` passed to `image_build_args` is the same binding `run_dist` passes to
     `assemble`, which writes it to `VERSION` (`xtask/src/dist.rs:759` `let version = derive_version(&root)?`
     → `:767` `obtain_binaries(&root, &cfg, &version)` and `:768` `assemble(…, &version, …)`). No re-derivation.

6. **Dockerfile** — `deploy/docker/wyrd/Dockerfile:47` global `ARG WYRD_VERSION=""` (mirrors
   `ARG FEATURES=""` at `:41`), and `:87` `ARG WYRD_VERSION` inside the `build` stage, placed
   directly before `RUN cargo build` (`:88`). An `ARG` in scope is in the `RUN` environment, so
   `cargo build` (and thus `build.rs`) sees it without changing the `RUN cargo build …` line —
   which matters because `xtask/tests/dist_two_binary_layout.rs:185` pins that line exactly.
   Placed late rather than at the top of the stage so a new version busts the cache only from the
   build step, not the apt/toolchain layer (Docker invalidates at the first *use* of a changed
   ARG, and every RUN after a stage-level ARG declaration uses it). Empty → rung 1 falls through;
   with `.git/` excluded by `.dockerignore:6`, rung 2 finds no repository and the result is
   `0.0.0+git.unknown`, never an empty identity.

7. **Docs currency** — `docs/design/architecture/07-deployment-view.md:42`: one sentence on the
   hand-off and the logged `version` field.

## Decisions I made that the brief left open

* **Rung 1 invalid → build error, not fail-closed to sha.** The brief's "fail closed to the sha
  form with a `cargo:warning`" is written about a *derived* value (an exotic tag). An explicit
  `WYRD_VERSION` is a hand-off from `dist`; silently substituting a different value would break
  exactly the `VERSION` equality the hand-off exists for. `dist` now validates before building,
  so in practice a bad value can only reach rung 1 by hand.
* **Rung 2 fail-closed form** = `<fallback>+git.<short sha>` from `git rev-parse --short HEAD`;
  if that sha is unusable too, rung 3. Bad bytes are never replaced (test
  `distinct_exotic_tags_are_never_collapsed_into_a_shared_replacement`).
* **Rung 2 and `-dirty`**: since `--dirty` is never passed, a describe ending in `-dirty` can only
  be a tag named that way; it fails closed to the sha form so no `.dirty` ever comes out of rung 2
  (`the_describe_rung_never_produces_a_dirty_identity`). A tag that merely *contains* "dirty"
  mid-name (e.g. `vfoo-dirty-3-gabc`) is reproduced as named — that is the tag, not a tree claim.
* **Rung 3 value** = `0.0.0+git.unknown`, i.e. `normalize_describe("", "0.0.0")` — the exact shape
  `dist`'s own normalizer gives an empty describe, so rung 3 is also single-sourced.
* **Length cap 100**: Docker tag max is 128 and dist tags `wyrd:<identity>-<flavor>`; 100 leaves
  room for `-` plus a flavor.
* **Leg 4 style**: the pure-function route (preferred by the brief), not the file-grep fallback.
  The Dockerfile half is necessarily a file read, but it parses stage boundaries and ordering
  rather than doing a substring check.
* I added a small structural pin, `the_build_identity_derivation_is_one_file_with_two_consumers`
  (`xtask/tests/dist_templates.rs:377`), that both `build.rs` and `dist.rs` `#[path]` the same
  file and that `dist.rs` no longer defines its own `normalize_describe`. It is a source grep; it
  guards against someone re-forking the normalizer later. Drop it if the reviewer objects.

## Test evidence

Discriminator: `crates/server/tests/build_identity_startup_log.rs` (the only added
`*/tests/*.rs`). Spawns `env!("CARGO_BIN_EXE_wyrd")` as `s3 --s3-listen 127.0.0.1:0 --data-dir
<tempdir> --access-key … --secret-key … --log-format json`, reads stderr on a reader thread with
a 60 s deadline, waits for the `serving S3-compatible HTTP on` line then the `role started` JSON
event, and kills+reaps the child in a `Drop` guard on every path. Names no symbol this patch adds.

Run through the project's gate, `engine/scripts/run-verify.sh` with
`PDCA_BUNDLE=results/issue_778 PDCA_BASE=df68932…` (the stacked base; with the default
`origin/main` the patch does not apply because the base carries #742):

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test build_identity_startup_log (fix applied)
test result: ok. 1 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-server --test build_identity_startup_log (production reverted, test kept)
panicked at crates/server/tests/build_identity_startup_log.rs:148:9:
the `role started` event carries no string `version` field: {"fields":{"dservers":0,"listen":"127.0.0.1:33373","message":"role started","region":"us-east-1","role":"s3"},...}
run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
```

The green leg ran the provenance leg (no `SKIP` line in the log): the gate's tree is a git
worktree. On this base the baked value is `0.0.0+git.df68932`; `git rev-parse --short HEAD` =
`df68932`; `git describe --tags --always` = `df68932`.

Manual checks (worktree, `cargo build -p wyrd-server --bin wyrd`):
* `WYRD_VERSION=0.1.0+git.3.abc12de.dirty` → the binary contains `0.1.0+git.3.abc12de.dirty`
  verbatim; unsetting it again → back to `0.0.0+git.df68932` (env watch works).
* `WYRD_VERSION=.bad` → build fails at `crates/server/build.rs:55` with the validator message.
* `git status` then `cargo build -v` → `Fresh wyrd-server` (no rerun, no relink).
* Nested-repo scoping, as above → `0.0.0+git.unknown`.

Legs 3-5 are green-only under `cargo xtask ci` (see CI result below).

### Refute-your-own-test

* **(a) Genuine red?** Yes. The gate reverted production (removed `build.rs`, `version.rs`,
  `version/derivation.rs`; restored `cli.rs`, `lib.rs`, `dist.rs`, Dockerfile, docs) and kept the
  test: it compiled, ran 1 test, and failed on the missing `version` field (output above).
* **(b) Production path?** Yes. The test runs the real `wyrd` binary cargo built for the test
  (`CARGO_BIN_EXE_wyrd`), whose real `build.rs` produced the value and whose real `cmd_s3`
  logged it through the real JSON subscriber. No mock, no copy of the derivation — the test
  derives its expectation with its own `git` calls.
* **(c) Fixture includes the fault?** Yes. The fault is "the binary does not know / does not
  log its source revision"; the fixture is the real binary built from a real git checkout, and
  the provenance leg hard-fails on `unknown` or a missing sha whenever `git rev-parse` succeeds.
  Skipping is only possible when the test's own `git rev-parse` fails (no repository visible).
  I found no gate environment where that happens: both the Do worktree and the C4-verify
  worktree are git worktrees.

## What this slice does NOT prove (stated plainly, per the brief)

* **End-to-end equality of a shipped binary's identity with its tarball's `VERSION` is NOT
  demonstrated.** That needs `cargo xtask dist` (Docker + network) and is outside `ci` by design.
  This slice proves the *coupling*: one normalizer, and `dist` passing the very `version` binding
  it writes to `VERSION` into the image build (`image_build_args`, asserted container-free) and
  into the `--host` build. I did not attempt an image build. The release smoke comparison is
  #779's. Two out-of-band routes exist if the human wants the pair observed before a tag: the
  release workflow's `workflow_dispatch` trigger (`.github/workflows/release.yml`), or a local
  `cargo xtask dist` and comparing `target/dist/wyrd-*/VERSION`'s `version:` line with the
  extracted binary's logged `version` (start `bin/wyrd s3 … --log-format json`).
* On an untagged checkout with no override the identity is `0.0.0+git.<sha>`: a real build
  identity, not a release version. The release-shaped value appears once a `v*` tag exists.
* A hand-built binary from an edited tree advertises its base commit, not a dirty marker
  (Decision 1, deliberate).
* Known gate noise (from the brief): `C5-mutants` fails its own baseline on this repo because
  the mutants scratch copy has no `.git`. Note that in such a copy this build script also falls
  to rung 3 (`0.0.0+git.unknown`) — that is correct behaviour, not a new failure.

## CI

`PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci` (= `cargo xtask ci`: repo guards incl. the
#616 unsafe-forbid scan over the new `build.rs`, fmt check, clippy `--all-targets` with
warnings-as-errors, build, workspace tests, DST, `cargo deny`) → `xtask ci: all checks passed`,
exit 0. The new tests ran in it: the derivation unit tests (as
`version::derivation::tests::*` in wyrd-server and again as `dist::identity::tests::*` in
xtask), `the_build_identity_derivation_is_one_file_with_two_consumers`,
`the_image_build_hands_the_version_to_the_binary`,
`the_dockerfile_declares_the_version_arg_in_the_build_stage`,
`dist_validates_its_version_with_the_shared_rule`, and the unmodified
`normalize_describe_covers_all_three_shapes`. The one-sentence docs edit to
`07-deployment-view.md` landed while CI was running; it is prose only and no gate reads it.
`cargo fmt --all -- --check` re-run clean on the final tree. The repo has no other commit hooks
(no `.pre-commit-config.yaml`, no `core.hooksPath`).
