# Adversarial review — issue 778 (wyrd-build-identity-derived-and-logged)

The toolchain was available (cargo 1.96.0, git 2.53.0), so every claim below was re-run against
the patch's own `build.rs` and `derivation.rs` in throwaway probe crates. Overall: the red→green
proof holds and the core design survived every correctness attack I tried. Three findings remain,
and one of them is the same defect T4 already reported.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/build.rs:53` (used at `:55`): `std::env::var("WYRD_VERSION").ok()`
  turns a **non-UTF-8 override into "unset"**, so the build silently drops to rung 2 with no
  error and no `cargo:warning`. I reproduced it with the patch's build script in a probe
  crate: `WYRD_VERSION='bad value'` panics with a clear message ("WYRD_VERSION is not usable …"),
  but `WYRD_VERSION=$'\xff\xfe'` exits 0 and bakes `0.0.0+git.1f0a62e`. That breaks the
  contract stated at `crates/server/src/version/derivation.rs:137` ("an invalid one is an `Err`:
  … a silent substitute would break exactly the equality it exists for"). It is also the
  rubric's *absent or unsupported entries → never a silent skip* class. This **confirms T4's two
  blocking items** (`gate-logs/T4-batch-review.log`); they are one defect, not new. Fix: read
  `std::env::var_os`, and when the value is present but not UTF-8, panic the same way an invalid
  value does. The pure `resolve` takes `&str`, so it cannot pin this. A unix-only case in
  `src/version/build_script_tests.rs` can: build with `OsStr::from_bytes(b"\xff")` and assert
  the build fails.

- NEEDS-HUMAN [human] — `gate-logs/C4-diff-cov.log:10`: the bundle **still does not apply on
  `origin/main`**. This is the third round in a row; the round-2 carry-forward already said
  "rebase and re-run the gates". The target tree is the patch on the "pre-fix base df68932f",
  and the `cmd_s3` hunk sits at `crates/server/src/cli.rs:2388` (the brief cites `:2199`). So the
  C4-ci and C4-verify greens are evidence for that old base, not for the merge result, and diff
  coverage was never measured. The builder has now failed to move off this base twice when told
  to, which suggests the driver is handing Do a stale base. A human should rebase, or tell the
  driver to, before sign-off trusts these gates.

- NEEDS-HUMAN [human] — `crates/server/src/version/derivation.rs:257` (`common_dir.join("refs")`,
  watched recursively) against the doc claim at `crates/server/build.rs:13` ("re-runs when an
  input to that answer can have moved, **and at no other time**"). That claim is false. The
  whole `refs/` tree is watched, so a ref that cannot change `git describe --tags` of *this*
  HEAD still re-runs the script and recompiles the crate. I reproduced this with the patch's
  build script, a main repo, and two linked worktrees, building the probe in `wtA`:
  - no-op rebuild → `fresh:true`
  - `git commit` on branch `b` in sibling worktree `wtB` → `fresh:false`, identity unchanged
    (`0.0.0+git.1f0a62e`)
  - `git update-ref refs/remotes/origin/feature HEAD` (what `git fetch` does) → `fresh:false`,
    identity unchanged

  In `wyrd-server` each of these means recompiling the crate and relinking its ~41
  integration-test binaries. That is the relink churn Decision 1 was meant to remove, now moved
  from `git status` to sibling-worktree commits and fetches. It hits hardest in a
  shared-common-dir, many-worktree setup like the harness's own `wyrd.pdca-wt-*` worktrees.
  The brief's wording ("covers exactly that [HEAD/refs] and nothing wider") can be read either
  way, so this is a judgment call:
  - accept the churn and fix the doc's "at no other time" (an impl nit), or
  - narrow the watch to `HEAD`, the branch ref HEAD names, `refs/tags/`, `packed-refs`,
    `reftable/` and `shallow`. A branch switch already rewrites `HEAD`, which would re-resolve
    the branch ref.

- No action needed for this slice (informational): the `--host` hand-off at
  `xtask/src/dist.rs:614` (`.env("WYRD_VERSION", version)`) is not asserted by any test. Deleting
  it would survive `ci`, and a `--host` tarball built on a dirty tree would then carry a
  binary without `.dirty` next to a `VERSION` that has it. Brief leg 4 names only the image
  build, so this is outside the required criterion.

## Refutation attempts that failed (the fix held)

- **Red→green is genuine and uses the production path.** `gate-logs/C4-verify.log`: with
  production reverted, the test panics at `crates/server/tests/build_identity_startup_log.rs:205`
  (leg 1, "carries no string `version` field") against the real spawned `CARGO_BIN_EXE_wyrd`'s
  `role started` JSON. With the fix, it passes. The green log has no `SKIP provenance leg` and
  no `WYRD_VERSION=` note. `note()` writes straight to stderr, bypassing libtest capture (`:46`),
  so it would have shown even on a passing run. That means leg 2's sha check actually ran;
  it was not skipped.
- **The shallow-boundary regression test really depends on the appearance watch.** I made a
  mutant of `build.rs` that drops the `rerun-if-changed=<appear_dir>` line. On a full clone one
  commit past `v1.2.3`, after `git fetch --depth=1 --no-tags origin HEAD`, the mtimes of
  `HEAD`, `packed-refs` and `refs` did not change, `describe` printed `cd57dec`, and the mutant
  still baked `1.2.3+git.1.cd57dec`. `a_full_clone_made_shallow_and_back_rebuilds_with_each_new_identity`
  would go red against that mutant, so the test is not a tautology.
- **Rung 1 / Docker hand-off.** `deploy/docker/wyrd/Dockerfile:49` (global `ARG WYRD_VERSION=""`)
  plus `:89` (stage-scoped `ARG WYRD_VERSION`, before the `RUN cargo build` at `:90`): an unset
  argument inherits `""`, which falls through to rung 2, then to rung 3 with a warning. It never
  bakes an empty identity. `dist` hands the image build (`xtask/src/dist.rs:638`) and the
  `VERSION` file (`:715`) the one `version` binding from `:759`; there is no re-derivation.
- **Single source (leg 3).** `normalize_describe_covers_all_three_shapes` is untouched by the diff
  and passes (`gate-logs/C4-ci.log:3100`). `xtask` includes the product file by `#[path]`, so the
  dependency runs tooling→product only.
- **Scope.** Nothing in `crates/gateway-s3`, no `S3Config` change, and only the `version` field
  is added to the `s3` role's event. `build.rs` carries `#![forbid(unsafe_code)]`, and the
  architecture doc is updated.
- **C5-mutants** fails on its unmutated baseline at `xtask/tests/repo_hygiene_guards.rs:137`
  (`git ls-files` in a scratch copy with no `.git`). The brief declared this as environment
  noise, and it is not evidence against this patch.
