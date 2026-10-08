# Adversarial review — #778 (build identity derived and logged)

**Bottom line:** the red→green proof holds. I could not break the binding criterion (legs 1–2) in the gate's environment. I did break three secondary claims with concrete inputs; two of them I reproduced locally. All file references are to the patched tree at `$PDCA_TARGET`.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/build.rs:119-127` (`watch_paths`) leaves the baked identity **stale on a reftable repository**. Reftable is git's newer ref storage format (`git init --ref-format=reftable`; Git 3.0 plans to make it the default). In such a repo, commits and tags only update `<common_dir>/reftable/`: `HEAD` stays the stub `ref: refs/heads/.invalid`, `refs/` holds only a stub `heads` file, and there is no `packed-refs`. None of the three watched paths ever changes, so the build script never re-runs. I reproduced this with git 2.53, using the patch's own `build.rs` and `derivation.rs` in a probe crate nested two levels deep:
  - files backend: after a second commit the binary reports `0.0.0+git.7c1dc14`, which is HEAD. Correct.
  - reftable: after a second commit the binary still reports `0.0.0+git.a6cd325` while HEAD is `71e5555`.

  This contradicts `crates/server/src/version.rs:22` ("the identity cannot go stale"). On such a checkout the new integration test also goes red after the first commit, for a reason unrelated to the code. Fix: also watch `<common_dir>/reftable` when it exists. It is a directory, and cargo scans watched directories recursively. This is separate from the batch review's `shallow` finding (same function, different file).

- NEEDS-HUMAN [impl] — `crates/server/src/version/derivation.rs:161`: rung 2 can still produce a **`.dirty` identity**, which the doc at `:138` says it never does ("this rung never produces `.dirty`"). The guard only checks for the `-dirty` suffix that `git describe --dirty` would add. A tag whose own name carries the marker passes straight through as `Rung::Describe`, with no warning. I compiled the shared file on its own and called `resolve(None, Some(d), Some("abc12de"), "0.0.0")`:
  - `v0.1.0+git.dirty` → `0.1.0+git.dirty`
  - `v0.1.0.dirty` → `0.1.0.dirty`
  - `v0.1.0.dirty-3-gabc12de` → `0.1.0.dirty+git.3.abc12de`

  The first result is byte-identical to what `dist` writes to `VERSION` for a *dirty* tree on tag `v0.1.0` (`normalize_describe("v0.1.0-dirty", "0.0.0")` → `0.1.0+git.dirty`). So a clean hand build and a dirty release can share one identity. That breaks the brief's "distinct inputs must not become one identity" rule and leg 5's "a rung-2 derivation never produces one". It needs someone to cut an odd tag, so severity is low, but the fix is cheap:
  - refuse a rung-2 value whose `.`- or `+`-separated parts include `dirty`, and fall back to the sha form;
  - add these three describe strings to `the_describe_rung_never_produces_a_dirty_identity` (`:285`). Its hand-picked list only covers `-dirty` suffixes, which is why it stays green.

- NEEDS-HUMAN [impl] — `crates/server/tests/build_identity_startup_log.rs:162` and `:169`: **the SKIP reason is invisible.** The brief allows leg 2 to skip only when the probe fails, "and then the leg must print why". The test prints with `eprintln!`, and libtest captures and discards that output when a test passes. I checked with a one-line `#[test]` that calls `eprintln!`: the output is just `test t ... ok`. So wherever the probe can't see a repo, leg 2 is skipped and the run shows a plain `ok`. Examples: cargo-mutants' scratch copy (no `.git`), an unpacked source tree, or a container where git refuses the checkout over ownership. That is the "silently green with the only provenance check off" pattern the brief's v3 carry-forward warns about. This does not affect the C4-verify evidence: the gate tree is a git worktree and the probe succeeded there. Fix: write the reason with `writeln!(std::io::stderr(), …)`. I verified that this bypasses capture and shows on a passing run. Alternatively, fail unless an explicit opt-out env var is set.

- NEEDS-HUMAN [human] — `deploy/docker/wyrd/Dockerfile:47` (`ARG WYRD_VERSION=""`): **compose-built `s3` gateways always log `version=0.0.0+git.unknown`.** `deploy/small-multi-node-fdb/docker-compose.yml:219-223` builds `wyrd` from this same Dockerfile, passes only `FEATURES`, and runs three `s3` gateways (`:389`, `:406`, `:423`). With no `WYRD_VERSION` and no `.git` in the build context, every gateway logs that value. Nothing warns about it: rung 3 emits no `cargo:warning`. The architecture doc lists docker-compose as a shipped artifact. The brief states its invariant over the whole build/packaging category ("violated equally by … a shipped binary whose build stage cannot see the repository"), but scope (c) only covers `dist`. The new sentence at `docs/design/architecture/07-deployment-view.md:42`, "Every `wyrd` names the checkout it was built from", overclaims for these builds. Human call: accept this as an out-of-scope follow-up (file an issue to pass `WYRD_VERSION: ${WYRD_VERSION:-}` through the compose `args`, and soften the doc sentence), or widen this slice.

## Notes (no action needed from a human)

- `xtask/src/dist.rs:786-790`: the patch adds `versioned_image_tag` (`:197`) and moves `obtain_binaries` onto it. But `run_dist`'s cleanup still builds the same `wyrd:{tag}-{flavor}` string by hand for `docker rmi`. That leaves two definitions of one tag. If they drift, `rmi` misses the tag and the failure is ignored (`let _ =`). Small nit: use the helper.
- **The proof ran on an old base.** `gate-logs/C4-diff-cov.log` says the patch does not apply on current `origin/main`. So C4-verify's red→green ran against the bundle's frozen base (`dd5b041`, "pre-fix base df68932…"), not today's main. The proof is valid for that base. Re-run it after the rebase: the brief declares conflicts with #738/#742 in the same `cmd_s3` event and in `dist.rs`.
- The `C5-mutants` failure is the pre-declared one: `repo_hygiene_guards.rs:137` fails because the scratch copy has no `.git` (confirmed in `gate-logs/C5-mutants.log`). I did not count it against the patch.
- Not raised again because the T4 batch review already has them: a `WYRD_VERSION` override vs leg 2, the test's git probe not being limited to the workspace's own repo, the `shallow` file, and no version check when HEAD is exactly on a tag.

## Attempted to refute; could not

- **The red leg fails for the right reason.** It panics on the missing `version` field (`gate-logs/C4-verify.log`, `build_identity_startup_log.rs:148`), not on a compile error. The test names no symbol the patch introduces.
- **The test drives the production path.** It runs the real `CARGO_BIN_EXE_wyrd` binary and parses its real stderr. `cli.rs:2394` is the event it reads.
- **The child process is always killed.** `RoleGuard` is declared after the temp dir, so it drops first on both the return path and the panic path. The reader thread ends when the pipe closes.
- **An empty Docker build arg never bakes an empty identity.** The global `""` default is inherited by the stage's `ARG` (`Dockerfile:87`). Empty falls through to rung 2, which finds no repo (`GIT_CEILING_DIRECTORIES` stops at `/src`'s parent), then to rung 3.
- **The `--host` path hands over the same string.** It passes the same `version` binding through the environment (`dist.rs:614`), and `rerun-if-env-changed` forces the build script to re-run.
- **Rung 1 rejects bad values.** Whitespace, newlines, `/`, `:` and leading `-`/`.`/`+` all panic the build with a clear message.
- **The shared file resolves and leg 3 holds.** The `#[path]` in `dist.rs` resolves to the shared file, and the unmodified `normalize_describe_covers_all_three_shapes` passes (`gate-logs/C4-ci.log:3096`).
