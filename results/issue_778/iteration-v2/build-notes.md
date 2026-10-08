# Build notes — issue 778 / wyrd-build-identity-derived-and-logged (iteration 2)

Target: getwyrd/wyrd, worktree `$PDCA_WORKTREE` at `df68932` — the stacked integration base
`pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (it already carries #738 and #742).
Line numbers are on that tree with the patch applied unless marked "base".

This round starts from the v1 patch (`iteration-v1/patch.diff`, which still applies cleanly
to `df68932`) and changes only what the carry-forward and the T4 batch review flagged, plus
one adversary nit. The v1 design (one `#[path]`-shared derivation file, three rungs, the
`dist` hand-off, the Dockerfile `ARG`) is unchanged; its rationale is in
`iteration-v1/build-notes.md` and still holds.

## Carry-forward findings → what changed

| Finding (source) | Fix | Where |
|---|---|---|
| Test probe inherits `GIT_DIR` and finds enclosing repos (T5 [impl]; batch ×3, lines 46/47/48) | The test's own `git` helper now removes every repository-selection variable (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_NAMESPACE`, `GIT_DISCOVERY_ACROSS_FILESYSTEM`), sets `GIT_CEILING_DIRECTORIES` to the workspace root's parent, and `own_head_short_sha` requires `--show-toplevel` to canonicalize to the workspace root. Independent code — it names no production symbol. | `crates/server/tests/build_identity_startup_log.rs:64-93`, `:99-114` |
| SKIP reason invisible under libtest capture (adversary) | `note()` writes with `writeln!(std::io::stderr(), …)`, which libtest does not capture. Proven visible on passing runs (evidence below). | `build_identity_startup_log.rs:45-47`, used at `:217`, `:233` |
| SHA assertion rejects a valid `WYRD_VERSION=1.2.3` build (batch ×3, lines 180/181/183; T5 C1 deferred) | `BUILD_OVERRIDE = option_env!("WYRD_VERSION")` (`:25`). When the test — and so the binary built in the same cargo invocation — was compiled with a non-empty override, the test asserts the logged `version` EQUALS it (`:216-227`), prints a visible note, and does not run the git leg. With no override (every gate), leg 2 runs exactly as the brief states. | `build_identity_startup_log.rs:20-25`, `:216-228` |
| Exact tag: no check on the value (batch ×2, TEST-GAP, lines 180/181) | When `git describe --tags --exact-match HEAD` succeeds, the identity must END WITH the tag minus a leading `v`, or contain the short sha (the fail-closed form). An unrelated constant fails. A failing exact-match probe falls into the stricter sha branch. | `build_identity_startup_log.rs:249-268` |
| `describe` failure while HEAD resolves was a skip | Now a hard failure: `--always` prints the sha whenever HEAD resolves, so this is a broken repository, not "no repository visible". | `build_identity_startup_log.rs:240-243` |
| Reftable repository: identity goes stale (adversary) | Watch set now includes `<git_dir>/reftable` and `<common_dir>/reftable`. | `crates/server/src/version/derivation.rs:249-271`, called from `crates/server/build.rs:41-46` |
| Shallow clone deepened: identity goes stale (batch, `build.rs:120`; T5 C5 deferred) | Watch set now includes `<common_dir>/shallow`. See "Decision: shallow" below. | same |
| Rung 2 can emit a `dirty` identity from a tag name (adversary) | Rung 2 refuses ANY derived identity containing `dirty` (ASCII case-insensitive) and fails closed to `0.0.0+git.<sha>` with a warning. Replaces v1's `ends_with("-dirty")` check, which it covers. | `derivation.rs:171-176` |
| Rung 3 is silent; compose-built gateways log `0.0.0+git.unknown` with no warning (adversary, deferred to human) | Rung 3 now always carries a warning that names the value and says to pass `WYRD_VERSION`. Docs and Dockerfile comment softened (no longer "every `wyrd` names the checkout"); they name the compose case. Compose itself NOT changed — out of this slice's scope (c). | `derivation.rs:203-217`; `deploy/docker/wyrd/Dockerfile:42-49`; `docs/design/architecture/07-deployment-view.md:42` |
| `run_dist` builds the versioned tag by hand (adversary nit) | Uses `versioned_image_tag`. | `xtask/src/dist.rs:786` (base `:775-779`) |

The watch list moved from a `build.rs` method into the shared pure module as
`watch_candidates(git_dir, common_dir)`, so it is unit-tested (`derivation.rs:477-528`):
one test pins that every ref format and `shallow` are covered, one pins that the index,
config, logs, objects, worktrees, and the git/common dir as a whole are never watched (a
directory watch is recursive in cargo, so watching the git dir would pull in the index). The
existence filter stays in `build.rs:41-43` because it is I/O.

## Decision: shallow (deferred item C5) — I added it, and why it respects Decision 1

The T5 reviewer deferred this to the human because the brief says the watch set "covers
exactly [HEAD/refs] and nothing wider". The batch review (gating) listed it as blocking. I
added `shallow` because Decision 1's stated purpose is the COST axis: "do not watch the git
index; do not watch source files", so the edit→`git status`→test loop does not relink
`wyrd-server` and its ~41 test binaries. `shallow` never changes on that loop — only on a
fetch that changes history depth — and it is one of the inputs that decide
`git describe`'s answer for a fixed HEAD. Proven below: an edit plus `git status` leaves the
crate `Fresh`. If the human reads "HEAD/refs" literally, revert `common_dir.join("shallow")`
(`derivation.rs:255`) and its assertion (`:489`): two lines.

Known limits of the watch set, not fixed:
- A path that does not exist at build time cannot be watched (cargo would re-run the script
  on every build). So a FULL clone later made shallow (`git fetch --depth`) is not noticed
  until something else re-runs the script. The common direction — deepening or
  unshallowing — is caught: the file changes or disappears, and a missing watched path
  re-runs the script once.
- `config` is not watched, so changing `core.abbrev` does not re-run the script. Watching
  `config` would re-run on any `git config` write, for a near-zero benefit.
- `info/grafts` (deprecated, git warns on use) is not watched.

## Decision: override vs. SHA leg (deferred item C1)

The brief's leg 2 says the value MUST contain the short sha whenever the probe succeeds and
HEAD is not exactly on a tag. It does not cover a build that was given `WYRD_VERSION`, where
rung 1 wins by the brief's own rung order. Failing such a build (v1) contradicts rung 1. My
resolution: the override branch is a HARD equality assertion, not a skip, and it applies
only when the TEST's compile environment carried a non-empty `WYRD_VERSION` (`option_env!`,
tracked by cargo, so the test and the binary always see the same value). In every gate run
the variable is unset and leg 2 runs as written. The note is printed visibly so a reader of
a green log can see which leg ran. The human should confirm this reading at sign-off.

## Decision: `dirty` refusal is a substring test

`derivation.rs:171` refuses a rung-2 identity if it contains `dirty` anywhere, any case.
Alternative: refuse only when a `.`/`+`/`-`-separated component equals `dirty` (~3 more
lines). Rejected because it lets through `v0.1.0.DIRTY`-style variants unless I also
lowercase, and the brief's leg 5 asserts "no `.dirty` reaches the identity" — the substring
rule makes `!identity.contains("dirty")` true for every rung-2 input by construction. Cost of
over-refusal: a tag like `vdirtyfix` falls back to `0.0.0+git.<sha>` with a warning. That
value still names the commit correctly, and two such tags on different commits stay
distinct (different shas).

## Test evidence

Discriminator: `crates/server/tests/build_identity_startup_log.rs` — still the only added
`*/tests/*.rs`; names no symbol this patch adds (`BUILD_OVERRIDE` is the test's own
`option_env!`).

**C4-verify, through the project gate** (`PDCA_LANE=0 PDCA_BUNDLE=results/issue_778
PDCA_BASE=df68932f2c633586fc2ce60cc418f878f8010d7d ./engine/scripts/run-verify.sh`):

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test build_identity_startup_log (fix applied)
test result: ok. 1 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-server --test build_identity_startup_log (production reverted, test kept)
panicked at crates/server/tests/build_identity_startup_log.rs:205:9:
the `role started` event carries no string `version` field: {"fields":{"dservers":0,"listen":"127.0.0.1:39775","message":"role started","region":"us-east-1","role":"s3"},...}
run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
```

The gate prints the test binary's full stderr, and the green leg shows no
`build_identity_startup_log:` line, so neither the skip nor the override branch fired: leg 2
ran against HEAD `df68932` and the binary logged `0.0.0+git.df68932`.

**Same gate with `WYRD_VERSION=1.2.3+git.override.dirty`** (exercises the override branch
and proves `note()` output is visible on a passing run):

```
run-verify.sh: GREEN — …
build_identity_startup_log: WYRD_VERSION=`1.2.3+git.override.dirty` was set for this build, so the binary must carry it verbatim; the git-derivation leg applies only to a build that derived its own
test result: ok. 1 passed; 0 failed
run-verify.sh: RED — … panicked at …:205:9: the `role started` event carries no string `version` field
run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
```

**Manual experiments** (scratch under `$PDCA_SCRATCH/pdca-builder-778-probe`; each a probe
crate at `<repo>/crates/probe` whose `build.rs` and `src/version/derivation.rs` are SYMLINKS
to the real worktree files, so the production build script ran, not a copy; v1's `build.rs`
was extracted verbatim from `iteration-v1/patch.diff` for the comparison):

| Scenario | v1 `build.rs` | this patch |
|---|---|---|
| reftable repo, second commit (HEAD `f41fcf0`→`ef225a9`) | `0.0.0+git.f41fcf0` — stale | `0.0.0+git.594bd44` = new HEAD; then tag on HEAD → `1.2.3`; one commit past → `1.2.3+git.1.07b5c27` |
| shallow clone with tag beyond the boundary, then `git fetch --unshallow --no-tags --refmap= origin HEAD` (no ref changes) | stays `0.0.0+git.9b738d6` while describe is `v1.2.3-1-g9b738d6` — stale | `1.2.3+git.1.9b738d6` |
| reftable repo: edit a file, `git status`, `cargo build -v` | — | `Fresh probe` (no re-run, no relink) |

Real-test runs (hand-run `cargo test` with an explicit `timeout`, because `run-verify.sh`
cannot target these setups; supplementary only — the red→green proof is the gate above):
- `GIT_DIR=<foreign reftable repo>/.git cargo test -p wyrd-server --test
  build_identity_startup_log` in the worktree → `1 passed`. v1's test failed exactly this
  (`reviewer-evidence/foreign-git.log`).
- Source tree copied WITHOUT `.git` into an unrelated repo (outer HEAD `224b48e`), fresh
  target dir → build warnings
  `wyrd-server build identity: neither \`WYRD_VERSION\` nor \`git describe\` of the workspace's own repository is available`
  and `… records \`0.0.0+git.unknown\`; a build whose context has no \`.git/\` should pass \`WYRD_VERSION\``;
  baked `WYRD_BUILD_IDENTITY=0.0.0+git.unknown` (not the outer sha); the test passed and printed
  `build_identity_startup_log: SKIP provenance leg: the workspace's own git repository is not visible (\`git rev-parse --show-toplevel\` exited exit status: 128: fatal: not a git repository …)`
  on the passing run.

Legs 3-5 are green-only under `cargo xtask ci` (results in the CI section).

### Refute-your-own-test

- **(a) Genuine red?** Yes. The verify gate reverted production (removed `build.rs`,
  `version.rs`, `version/derivation.rs`; restored `cli.rs`, `lib.rs`, `dist.rs`,
  Dockerfile, docs, `dist_templates.rs`), kept the test, compiled it, ran 1 test, and it
  failed on the missing `version` field (`:205`). Red also holds with `WYRD_VERSION` set.
- **(b) Production path?** Yes. The test spawns the real `CARGO_BIN_EXE_wyrd`, whose real
  `build.rs` computed the value and whose real `cmd_s3` (`cli.rs:2393`) logged it through
  the real JSON subscriber. The expectation comes from the test's own `git` calls, not
  from the production resolver. The staleness experiments ran the real `build.rs` and
  `derivation.rs` through symlinks.
- **(c) Fixture includes the fault?** Yes. The fixture is a real git worktree checkout; the
  provenance leg hard-fails on `unknown` or a missing sha whenever the workspace's own
  repository is visible, and a `describe` failure with HEAD visible is now a failure, not a
  skip. The skip fires only when the scoped probe cannot see the workspace's own repository,
  and then it prints why on a passing run. Every gate tree (Do worktree, verify worktree
  `wyrd-verify-l0`) is a git worktree; I found no gate environment where the probe fails.
  cargo-mutants' scratch copy (no `.git`) would take the skip, visibly.

## What this slice does NOT prove (stated plainly, per the brief)

- **End-to-end equality of a shipped binary's identity with its tarball's `VERSION` is NOT
  demonstrated.** It needs `cargo xtask dist` (Docker + network), outside `ci` by design. I
  did not attempt an image build. This slice proves the coupling: one normalizer
  (`normalize_describe_covers_all_three_shapes`, `xtask/tests/dist_templates.rs:336`,
  unmodified — zero removed lines in that file), and `dist` passing the very `version`
  binding it writes to `VERSION` (`xtask/src/dist.rs:759` → `:767` `obtain_binaries` →
  `:638` `image_build_args` → `:226` `WYRD_VERSION={version}`; same binding → `:768`
  `assemble` → `:715` `VERSION`) into the image build, asserted container-free
  (`dist_templates.rs:403`), and into the `--host` build (`dist.rs:614`). Two routes exist
  if the human wants the pair observed before a tag: the release workflow's
  `workflow_dispatch` trigger, or a local `cargo xtask dist` then comparing
  `target/dist/wyrd-*/VERSION`'s `version:` line with the extracted `bin/wyrd s3 …
  --log-format json` startup event's `version`. The release-smoke comparison is #779's.
- On an untagged checkout with no override the identity is `0.0.0+git.<sha>`: a real build
  identity, not a release version.
- A hand-built binary from an edited tree advertises its base commit (Decision 1).
- Compose-built images (`deploy/small-multi-node-fdb/docker-compose.yml:219-223` builds this
  Dockerfile with only `FEATURES`) log `0.0.0+git.unknown`. Now warned at build time and
  documented, not fixed: passing `WYRD_VERSION` through compose `args` is outside scope (c).
  Suggested follow-up issue for the human.

## Gate notes for the human

- **C4-diff-cov will likely fail again with "patch.diff does not apply on origin/main".**
  In v1 the verify gate and the diff-cov gate share one base resolver
  (`run-diff-cov.sh:685` calls `run-verify.sh --print-base`), yet verify got the stacked
  base and diff-cov got `origin/main` (`iteration-v1/gate-logs/C4-diff-cov.log:10`). The
  patch must be written against the stacked base the driver gave me (it carries #738/#742,
  which the brief declares as conflicts); it cannot also apply to `origin/main`. This looks
  like a driver/env issue (harness-side), not a patch defect.
- **C5-mutants**: the pre-declared baseline failure (`repo_hygiene_guards.rs` needs
  `git ls-files`; the mutants scratch copy has no `.git`).
- No `review-rejected.md` written: every batch-review finding is fixed above.

## CI

`./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`) on the final tree →
`xtask ci: all checks passed`, exit 0. It ran: the #616 unsafe guard ("every crate root
forbids unsafe code", which scans the new `build.rs`), fmt, clippy, build, workspace tests,
`cargo deny check` (advisories/bans/licenses/sources ok), conformance, DST. New tests seen
passing in that log: all 12 `version::derivation::tests::*` (wyrd-server) and the same 12
as `dist::identity::tests::*` (xtask compiles the same file),
`the_s3_role_logs_the_build_identity_derived_from_git`,
`the_build_identity_derivation_is_one_file_with_two_consumers`,
`the_image_build_hands_the_version_to_the_binary`,
`the_dockerfile_declares_the_version_arg_in_the_build_stage`,
`dist_validates_its_version_with_the_shared_rule`, and the unmodified
`normalize_describe_covers_all_three_shapes`. Zero `wyrd-server build identity` warnings in
the CI build (it is a git worktree, so rung 2 resolves).

`patch.diff` was regenerated from `git diff HEAD` after CI and is byte-identical to the tree
CI ran on (10 files, +1215/−93).

Formatter: `cargo fmt --all` then `cargo fmt --all -- --check` clean on the final tree;
`cargo clippy -p wyrd-server -p xtask --all-targets -- -D warnings` clean. The repo has no
other commit hooks (no `.pre-commit-config.yaml`, no `core.hooksPath`).
