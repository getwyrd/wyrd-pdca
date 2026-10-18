# Build notes — issue 778 / wyrd-build-identity-derived-and-logged (iteration 3)

Target: getwyrd/wyrd. Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0` at
`df68932`, the stacked integration base named in `stack-base`
(`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`, which carries #738 and #742 on
top of `origin/main`). Line numbers are on that tree with the patch applied unless marked
"base".

This round starts from the v2 patch (`iteration-v2/patch.diff`, applies cleanly to
`df68932`) and changes only the build script's re-run behaviour, which is what every
round-2 finding was about. The rest of the design (one `#[path]`-shared derivation file,
three rungs, the `dist` hand-off, the Dockerfile `ARG`, the discriminator test) is
unchanged; its rationale is in `iteration-v1/build-notes.md` and `iteration-v2/build-notes.md`
and still holds. In short: the shared resolver is `crates/server/src/version/derivation.rs`,
compiled by `crates/server/build.rs` (`#[path]`, `build.rs:46-48`), by the crate
(`version.rs:35`), and by `xtask/src/dist.rs:181-186` (`#[path]`, so the dependency runs
tooling→product); leg 4 uses the extracted pure function `image_build_args`
(`xtask/src/dist.rs:207`, `WYRD_VERSION={version}` at `:226`), not the file-read fallback.

## Carry-forward findings → what changed

| Finding (source) | Fix | Where |
|---|---|---|
| A full clone made shallow keeps the old tag-derived identity: the absent `shallow` file was filtered out of the watch set (T4 ×2 blocking; C5 adversary, `reviewer-evidence/shallow-probe.log:44`) | Absent paths are no longer dropped. A path that exists is watched directly (catches change and deletion). A path that does not exist yet is watched for its **appearance**: the script keeps a symlink to it in a private `OUT_DIR/git-paths-not-yet-present/` directory and watches that directory. | `crates/server/build.rs:98-124` (`watch`), `:126-159` (`link_absent_paths`), `:161-174` (`symlink`, unix / fallback); docs `build.rs:10-38`, `derivation.rs:228-246`, `version.rs:26-30` |
| Once a build lands on rung 3 it stays there until `cargo clean` (adversary, `build.rs:52` in v2) | Rung 3 now emits a watch on a path that is never created, so cargo re-runs the script on every build until git can answer, plus a warning saying so. Covers no `.git`, `git` missing from `PATH`, a refused checkout, and an unborn `HEAD`. | `crates/server/build.rs:84-94`; docs `version.rs:17-20`, `build.rs:32-37` |
| "the pure watch-list assertions do not exercise the existence filter … add an incremental-build regression" (C5) | New `#[cfg(all(test, unix))]` module that runs **real cargo builds** of a probe crate whose build script IS `crates/server/build.rs`, across the real git transitions. | `crates/server/src/version/build_script_tests.rs` (declared at `version.rs:37-39`) |
| "patch.diff does not apply on `origin/main` … something newer has moved. Rebase" (`gate-logs/C4-diff-cov.log:10`) | Not a patch defect; nothing moved. See "Gate notes" below. | — |

## Decision: how to notice a file that does not exist yet

Facts I measured before choosing (scratch crates under `$PDCA_SCRATCH/pdca-builder-778-*`):

1. **A missing watched path re-runs the script AND recompiles the crate on every build.**
   Toy crate with `rerun-if-changed=/nonexistent-path-for-toy`: second build printed
   `Dirty toy …: the file '/nonexistent-path-for-toy' is missing`, then rebuilt the lib and
   the test binary. So the round-2 adversary's "only cargo-level fix" (always emit the
   absent `shallow` path) would make every build in every full clone recompile `wyrd-server`
   and relink its test binaries — the cost Decision 1 rules out.
2. **What a shallow fetch touches.** Full clone one commit past `v1.2.3`, then
   `git fetch --depth=1 --no-tags origin HEAD`; `find .git -newer marker` lists only
   `.git` (directory mtime), `.git/FETCH_HEAD` (created), `.git/objects` (directory mtime),
   `.git/shallow` (created). No ref moves. `describe` went `v1.2.3-1-gafc7b09` → `afc7b09`.
3. **Cargo's directory scan counts the directory's own mtime** (creating and deleting an
   entry in a watched dir made the next build Dirty), **follows symlinks, and skips a
   dangling one** (a watched dir holding a dangling link stayed Fresh; creating the link's
   target made the next build Dirty; the build after was Fresh again).
4. **Creating the link inside the script costs one extra re-run** on the next build (the
   directory's mtime is newer than cargo's reference stamp). Setting the directory's mtime
   back with std's `File::set_times` removes it: second build Fresh.

Alternatives ruled out:

- **(A) Watch the absent path directly** — fact 1: re-run + recompile on every build of
  every full clone, forever. Rejected on Decision 1's cost axis.
- **(B) Watch the git common dir** — cargo scans a directory recursively; the common dir
  holds the main checkout's `index` (rewritten by `git status`) and all of `objects/`
  (rewritten by `git add`, and a stat per loose object on every build). That is exactly
  the over-watch the v4 adversary measured. Rejected.
- **(C) Watch `FETCH_HEAD` as a stand-in** — `git clone` does not write it (absent in the
  fresh clone of fact 2), so the first-ever `fetch --depth` creates both files unseen — the
  same hole, one step later. `--no-write-fetch-head` also evades it. Rejected.
- **(D) Record the T4 findings as rejected**, as the round-2 adversary recommended. The
  auto-iterate carried the finding forward as implementation work ("Make full→shallow
  transitions refresh the identity"), and (E) fixes it without the cost the adversary was
  worried about, so I fixed it rather than argued it.

Chosen **(E), the appearance watch**. Cost: `build.rs` grows +122/−15 lines against v2
(code lines, excluding comments and blanks, 94 → 164). Runtime cost: none on the edit loop
— measured on this worktree: a no-op `cargo build -p wyrd-server -v` printed
`Fresh wyrd-server`, and so did one after `git status` + `git update-index --refresh`.

What it depends on, and how that is guarded: facts 3–4 are cargo behaviour, not documented
API. Both halves are pinned by the new incremental tests (a change that stops following
links fails the shallow test; one that treats a dangling link as missing fails every
"no-op rebuild must not re-run" assertion), so a toolchain bump that changes it turns CI red
instead of going stale. All facts were re-confirmed on the pinned toolchain: the worktree
build and the unit tests ran under `1.96.0` (rustup path in the build log, and the tests'
nested cargo inherits `RUSTUP_TOOLCHAIN`). Symlinks are made only on unix
(`build.rs:161-174`); elsewhere the script warns and watches the absent paths directly —
slower, never stale. Every CI runner and the release build are `ubuntu-latest`.

What the OUT_DIR holds on this worktree (a linked worktree) after a build:
`0 -> …/.git/worktrees/wyrd.pdca-wt-l0/reftable`, `1 -> …/.git/reftable`,
`2 -> …/.git/shallow`, directory mtime `1970-01-01`; direct watches on the worktree `HEAD`,
`.git/refs`, `.git/packed-refs`; baked `WYRD_BUILD_IDENTITY=0.0.0+git.df68932`.

## Decision: rung 3 re-probes on every build

The reviewer's suggestion, taken. Alternative: watch `<root>/.git` for appearance. It only
covers "no `.git`", not "`git` not on `PATH`" or "git refuses the checkout" (both have a
`.git`), and a link to an existing `.git` directory would make cargo walk all of it, index
included. A never-created path covers every cause with one line, and only builds that
already print a warning pay for it (the one-shot image build pays nothing extra).

## The incremental-build tests

`crates/server/src/version/build_script_tests.rs`:

- `a_full_clone_made_shallow_and_back_rebuilds_with_each_new_identity` (`:236`): full clone
  one commit past `v1.2.3` → `1.2.3+git.1.<sha>`; no-op rebuild Fresh; `git fetch
  --depth=1 --no-tags origin HEAD` (fixture asserts `shallow` exists, `git for-each-ref`
  unchanged, `describe` now the bare sha) → `0.0.0+git.<sha>`; no-op rebuild Fresh;
  `--unshallow` → back to `1.2.3+git.1.<sha>`; no-op rebuild Fresh.
- `an_edit_and_the_index_do_not_rerun_the_build_script_but_a_commit_does` (`:311`):
  Decision 1's cost axis. Edit a tracked file, `git status`, `git add` (fixture asserts the
  index mtime moved) → Fresh, same identity; then a commit → new identity.
- `a_build_with_no_repository_reprobes_until_one_exists` (`:359`): no `.git` →
  `0.0.0+git.unknown`, and the next build re-runs; `git init` + commit →
  `0.0.0+git.<sha>`; then Fresh.

How: the probe manifest sets `build = '<abs>/crates/server/build.rs'` and sits at
`<tmp>/workspace/crates/probe`, so the script's `../..` root is the fixture repo. Each build
is `$CARGO build --offline --message-format=json`; the test reads the bin artifact's
`fresh` flag and runs the binary to read the baked value. Git runs with
`GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1` (also inherited by the build
script's git), repository overrides removed; coverage/caching flags (`RUSTFLAGS`,
`RUSTC_WRAPPER`, `LLVM_PROFILE_FILE`, …) are shed for the probe build. Every step runs
under a 300 s budget and is killed past it (`:82-108`). A 1.1 s sleep before each repository
change (`:227`) keeps a change from landing in the same mtime tick as cargo's reference
stamp (cargo treats equal as unchanged). The 15 `version::` tests take ~3 s.

Placement: a `#[cfg(test)]` module inside the production crate, NOT a `*/tests/*.rs`, so the
verify classifier still sees exactly one added test file (`run-verify.sh --classify` on the
final patch: `ADDED_TEST crates/server/tests/build_identity_startup_log.rs`, `CRATE
crates/server`, `CRATE xtask`). It is reverted with production on the red leg, like legs 3–5.

**Red against the previous build script** (v2's `build.rs` swapped in, everything else
kept, then restored and `cmp`-checked):

```
test version::build_script_tests::a_build_with_no_repository_reprobes_until_one_exists ... FAILED
test version::build_script_tests::a_full_clone_made_shallow_and_back_rebuilds_with_each_new_identity ... FAILED
test version::build_script_tests::an_edit_and_the_index_do_not_rerun_the_build_script_but_a_commit_does ... ok
…build_script_tests.rs:356:5: a build that could not name its commit must re-run the build script
…build_script_tests.rs:270:5: the clone became shallow and `git describe` changed, but the build kept the old identity
  left: "1.2.3+git.1.02ab521"
 right: "0.0.0+git.02ab521"
```

(Line numbers from before `cargo fmt`; now `:368` and `:282`.) The edit/index test is green
on both scripts by design: it guards against over-watching, which v2 did not do either.

## Discriminator evidence (C4-verify, through the project gate)

`PDCA_LANE=0 PDCA_BUNDLE=results/issue_778 PDCA_BASE=df68932f2c633586fc2ce60cc418f878f8010d7d
./engine/scripts/run-verify.sh`, run on the FINAL `patch.diff`:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test build_identity_startup_log (fix applied)
test result: ok. 1 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-server --test build_identity_startup_log (production reverted, test kept)
panicked at crates/server/tests/build_identity_startup_log.rs:205:9:
the `role started` event carries no string `version` field: {"fields":{"dservers":0,"listen":"127.0.0.1:39319","message":"role started","region":"us-east-1","role":"s3"},…}
run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
```

The green leg printed no `build_identity_startup_log:` note, so neither the skip nor the
`WYRD_VERSION` override branch fired: leg 2 ran against the verify worktree's own HEAD.
The discriminator test file is byte-identical to v2's (`cmp` against the bundle copy).

### Refute-your-own-test

- **(a) Genuine red?** Yes. The verify gate reverted production (deleted `build.rs`,
  `version.rs`, `version/derivation.rs`, `version/build_script_tests.rs`; restored `cli.rs`,
  `lib.rs`, `dist.rs`, Dockerfile, docs, `dist_templates.rs`), kept the test, compiled it,
  ran 1 test, and it failed on the missing `version` field (`:205`). For this round's new
  behaviour, the incremental tests went red against v2's `build.rs` (above).
- **(b) Production path?** Yes. The discriminator spawns the real `CARGO_BIN_EXE_wyrd`,
  whose real `build.rs` computed the value and whose real `cmd_s3` (`cli.rs:2393`) logged
  it; its expectation comes from its own `git` calls, not from production code. The
  incremental tests compile the real `crates/server/build.rs` (by absolute path) with the
  real cargo of the pinned toolchain — no copy, no mock.
- **(c) Fixture includes the fault?** Yes. The discriminator runs in a real git worktree
  checkout and hard-fails on `unknown` or a missing sha whenever the workspace's own
  repository is visible. The shallow test asserts the fault is really present before it
  builds: the clone IS shallow, NO ref moved, and `describe` DID change. The no-repo test
  starts with no `.git`. The edit test asserts the index really was rewritten.

Gate environment: every tree the gates build in (Do worktree, verify worktree
`wyrd-verify-l0`) is a git worktree; the probe succeeded in both. cargo-mutants' scratch copy
has no `.git`, so there the discriminator takes its visible skip and `wyrd-server`'s build
lands on rung 3 (re-runs its script every build — cargo-mutants rebuilds per mutant anyway).
The incremental tests make their own repositories, so they do not depend on one.

## CI

`PDCA_WORKTREE=… ./engine/xtask.sh ci` (= `cargo xtask ci`) → `xtask ci: all checks passed`,
exit 0. Steps: typos, docs lint/render, gitlink guard, unsafe guard ("every crate root
forbids unsafe code" — scans `build.rs`), blackbox guard, `cargo fmt --check`, clippy,
build, workspace tests, cargo-machete, the three `cargo deny` runs, conformance, statics,
deploy guard, DST clippy + tests. New/changed tests seen passing: the 3
`version::build_script_tests::*`, the 12 `version::derivation::tests::*` and the same 12 as
`dist::identity::tests::*`, `the_s3_role_logs_the_build_identity_derived_from_git`,
`the_build_identity_derivation_is_one_file_with_two_consumers`,
`the_image_build_hands_the_version_to_the_binary`,
`the_dockerfile_declares_the_version_arg_in_the_build_stage`,
`dist_validates_its_version_with_the_shared_rule`, and the unmodified
`normalize_describe_covers_all_three_shapes` (`xtask/tests/dist_templates.rs:336`; the
patch removes zero lines from that file). Zero `wyrd-server build identity` warnings in
the CI log.

CI started about a minute before two comment-only edits (a doc line in `build.rs:26-28`
and one in `build_script_tests.rs:23-25`). I re-ran on the final tree: `typos` over the
touched server files, `cargo fmt --all -- --check`, `cargo clippy -p wyrd-server
--all-targets` (no warnings), and `cargo test -p wyrd-server --lib version::` (15 passed);
the verify run above used the final patch.

Formatter / hooks: `cargo fmt --all` then `--check` clean. The repo has no
`.pre-commit-config.yaml`, no `core.hooksPath`, no `.githooks`. `patch.diff` was generated
with `git diff HEAD` (new files via `git add -N`, index reset afterwards): 11 files,
+1717/−93, and `git apply --cached --check` succeeds on `df68932`.

## Gate notes for the human

- **C4-diff-cov will again report "patch.diff does not apply on `origin/main`".** Checked
  with a throwaway index: on `origin/main` the hunks for `deploy/docker/wyrd/Dockerfile`,
  `docs/design/architecture/07-deployment-view.md` and `xtask/src/dist.rs` fail — exactly
  the three files #742 changed; the same patch applies on `df68932`. `origin/main` is an
  ancestor of `df68932`, 24 commits behind it, and neither #738 nor #742 is on
  `origin/main`. So nothing "moved": the patch is written against the stacked base the
  driver gave Do and C4-verify (`PDCA_BASE`), while `run-diff-cov.sh:685` resolves its base
  through `run-verify.sh --print-base`, which without `PDCA_BASE` falls to the brief's
  `main`. That is a harness-side base mismatch, not something a rebase of this patch can
  fix while #742 is unmerged. (The brief says #742 expected THIS slice to land first; the
  driver stacked them the other way round.)
- **C5-mutants**: the pre-declared baseline failure (`repo_hygiene_guards.rs:137` needs
  `git ls-files`; the mutants scratch copy has no `.git`).
- `review-batch.md`'s two T4 findings are fixed (they should drop out of the next run); no
  `review-rejected.md` written.

## What this slice does NOT prove (stated plainly, per the brief)

- **End-to-end equality of a shipped binary's identity with its tarball's `VERSION` is not
  demonstrated.** It needs `cargo xtask dist` (Docker + network), outside `ci` by design; I
  did not attempt an image build. What is proven is the coupling: one normalizer
  (`normalize_describe_covers_all_three_shapes`, unmodified), and `dist` passing the very
  `version` binding it writes to `VERSION` into the image build (`dist.rs:638` →
  `image_build_args` `:226`, asserted container-free at `dist_templates.rs:403`) and into
  the `--host` build (`dist.rs:614`). Two routes exist if the human wants the pair observed
  before a tag: the release workflow's `workflow_dispatch`, or a local `cargo xtask dist`
  then comparing `target/dist/wyrd-*/VERSION`'s `version:` line with the extracted
  `bin/wyrd s3 … --log-format json` startup event's `version`. The release-smoke
  comparison is #779's.
- On an untagged checkout with no override the identity is `0.0.0+git.<sha>`: a real build
  identity, not a release version.
- A hand-built binary from an edited tree advertises its base commit (Decision 1).
- Compose-built images (`deploy/small-multi-node-fdb/docker-compose.yml` builds this
  Dockerfile with only `FEATURES`) log `0.0.0+git.unknown`, warned at build time. Not
  changed here (outside scope (c)); still a human call from round 2's deferred list.
- Known limits of the watch set, unchanged from v2: `config` (`core.abbrev`) and the
  deprecated `info/grafts` are not watched.

## Still for the human (deferred in round 2, untouched here)

- C1: the discriminator's `WYRD_VERSION` branch asserts equality with the override and
  skips the git leg only when the TEST was compiled with a non-empty `WYRD_VERSION`
  (`build_identity_startup_log.rs:25`, `:216`). Every gate run has it unset.
- The compose-fixture item above.
- The round-2 adversary's "reject T4 as out of Decision 1" recommendation is moot: the
  appearance watch fixes the finding without the per-build cost it was worried about.

No external dependency beyond what the brief lists was needed (git and cargo were already
required by the discriminator). Scratch work is under
`$PDCA_SCRATCH/pdca-builder-778-{shallow,rerun,redleg,applycheck}` and the two
`pdca-builder-778-{verify,verify2,ci}.log` files; left for the harness to reclaim.
