# Build notes — issue 778 / wyrd-build-identity-derived-and-logged (iteration 4)

Target: getwyrd/wyrd @ `main`. Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`,
detached at `0fba76c` = `origin/main` (zero commits either side). Line numbers below are on
that tree with the patch applied unless marked "base".

This round starts from the round-3 patch (`iteration-v3/patch.diff`). It applied to
`origin/main` cleanly this time (one 5-line offset in `cli.rs`): #742 has landed on `main`,
so the stacked-base mismatch of rounds 1–3 is gone. On top of it, this round changes only how
the build script reads its two raw byte inputs, which is what the round-3 carry-forward was
about. The rest of the design is unchanged and its rationale is in
`iteration-v{1,2,3}/build-notes.md`. In short: the shared resolver is
`crates/server/src/version/derivation.rs`, compiled by `crates/server/build.rs` (`#[path]`
module, not `include!`), by the crate (`version.rs:39`), and by `xtask/src/dist.rs:181-182`
(`#[path]`, so the dependency runs tooling→product). Leg 4 uses the extracted pure function
`image_build_args` (`xtask/src/dist.rs:207`, `WYRD_VERSION={version}` at `:226`), not the
file-read fallback.

## Carry-forward findings → what changed

| Finding | Fix | Where |
|---|---|---|
| A `WYRD_VERSION` that is not UTF-8 reads as unset: `std::env::var(..).ok()` drops `VarError::NotUnicode`, so the build bakes the git identity with no error (T4 ×2 blocking, C5 adversary; same defect) | Read with `std::env::var_os` and pass it through a new pure function in the shared module that returns `Err` for a set value that is not UTF-8. The build script panics on it, the same way it does for an invalid UTF-8 value. | `build.rs:69-71`; `derivation.rs:142-150` (`override_from_env`) |
| An undecodable `git describe` output discards a readable sha: `git_output`'s strict `String::from_utf8(..).ok()?` turns a tag name holding a byte ≥ 0x80 into "no describe", and `resolve` then went to rung 3, `0.0.0+git.unknown` (C5, `build.rs:216` / `derivation.rs:204` in v3) | (1) The build script reads describe as raw bytes (`git_stdout`) and decodes them with a new shared `describe_text` (lossy). The replacement character fails the validator, so `resolve` fails closed to the sha form and the warning quotes what git printed. (2) `resolve` also uses a readable sha when describe gave NO answer, so rung 3 is reached only when the commit cannot be named. | `build.rs:84-88`, `:233-247`; `derivation.rs:152-162` (`describe_text`), `:204-248` (`resolve`) |
| "Distinguish NotPresent from NotUnicode and add a regression at the environment-reading boundary" / "a unix-only case in `build_script_tests.rs`" | Two new real-cargo-build tests in the existing `#[cfg(all(test, unix))]` module, plus pure unit tests in `derivation.rs`. | below |
| C4-diff-cov: "patch.diff does not apply on origin/main" (rounds 1–3) | Nothing to change in the patch: the worktree is on `origin/main` this round. `git apply --cached --check` against a throwaway index read from `origin/main` succeeds. | — |

One item from the round-3 deferred list was a plain wording error, so I fixed the sentence
without changing design: `build.rs` claimed the script re-runs "at no other time" than when
the answer can move, but it watches the whole `refs/` tree. `build.rs:25-28` now says any ref
change re-runs it, whether or not it changes `describe` of this `HEAD`. Whether to narrow the
watch is still the human's call.

## Decisions and what I ruled out

**Env boundary.** Chosen: a pure `override_from_env(Option<&OsStr>)` in the shared module
(+9 code lines with its doc), called from `build.rs`. Ruled out:
- inline `match std::env::var(..) { Err(VarError::NotUnicode(_)) => panic!(..), .. }` in
  `build.rs`: about 5 lines, but then the decision is only testable through a real build;
  the shared function gets unit tests in both consumers (`version::derivation::tests` and
  `dist::identity::tests`) for the same cost;
- changing `resolve` to take `Option<&OsStr>`: changes the signature and every test call
  (`grep -o 'resolve(\|resolve_ok(' derivation.rs | wc -l` → 23, the two definitions
  included) for the same result.

**Describe boundary.** Chosen: lossy decode via shared `describe_text` AND the sha fallback
when describe is absent. Ruled out:
- making `resolve` take `describe: Option<&[u8]>` and refuse invalid UTF-8 inside it: same
  observable result, but every test call site changes from `Some("…")` to bytes (the same
  23 matches);
- the resolver change alone: it fixes the identity, but the build log would then say
  "neither `WYRD_VERSION` nor `git describe` … is available", which is false (git did
  answer). The decode keeps the warning accurate; the resolver change covers describe
  failing for any other reason while `rev-parse` still answers.

Why lossy decode cannot merge two tags into one identity: U+FFFD is not ASCII, so
`validate_identity` refuses any identity containing it, and the result is always the
commit's sha form. Pinned by
`derivation.rs:487` (`an_undecodable_tag_fails_closed_to_the_sha_form_and_never_goes_missing`,
including two distinct undecodable tags → two identities).

`dist` was already fine: `capture` decodes lossily (`xtask/src/dist.rs:521`, base) and
`derive_version` then refuses the result through the shared validator (`:547`), so a release
on such a tag stops loudly before any build. Not changed.

Non-UTF-8 paths are not handled and do not need to be for the work tree: cargo itself refuses
to build there. Measured: a toy crate copied under a directory named `dir\xff` →
`error: path ".../dir\xFF/toy/build.rs" not utf-8`, exit 101 (cargo 1.96.1). A linked
worktree whose git dir sits at a non-UTF-8 path outside the work tree reads as "no
repository": rung 3, warned, re-probed every build, never a wrong identity
(`build.rs:238-242`).

## A gap cargo leaves (measured, documented, not fixable from the build script)

Toy crate with `rerun-if-env-changed=WYRD_VERSION` and a `var_os` check that panics on
non-UTF-8, on cargo 1.96.1:

```
== build unset                      → ok
== build non-utf8 after unset       → Finished … rc=0   (script NOT re-run)
== build non-utf8 again             → Finished          (still not re-run)
== set to valid (1.2.3)             → 1.2.3
== non-utf8 after valid             → panicked: non-utf8 WYRD_VERSION "1.2.3\xFF"
== non-utf8 different bytes         → panicked: non-utf8 WYRD_VERSION "9.9\xFE"
```

So cargo's own `rerun-if-env-changed` check reads a non-UTF-8 value as unset. An incremental
build going from unset to such a value never runs `build.rs` and keeps the identity it
had (a correct git identity, not a fabricated one). A fresh build (every image build) and
one coming from any set value do run it, and fail. `dist` only ever passes a value it has
validated (`xtask/src/dist.rs:547`), so the dist hand-off is not exposed. Stated in the module
doc at `build.rs:16-21`. I found no in-tree way to close it: the script is simply not called.
I did not measure whether rustc's `option_env!` tracking behaves the same. A human may want
to report this to cargo upstream; it is outside this repo.

The real-build test is ordered so it does not depend on that gap: its first build is fresh,
and its second non-UTF-8 build comes after a set value.

## Tests added this round

Real cargo builds of a probe crate whose build script IS `crates/server/build.rs`
(`build_script_tests.rs`):
- `an_undecodable_tag_still_names_the_commit_by_its_sha` (`:459`): tag `v1.0\xff`; the
  fixture asserts `git describe` really printed non-UTF-8 bytes; the build must bake
  `0.0.0+git.<sha>` and its build log must quote `` `git describe` printed `v1.0 `` and say
  `sha-derived` (`:483`).
- `an_override_is_baked_verbatim_and_one_that_is_not_utf8_fails_the_build` (`:496`), in a
  repository where rung 2 WOULD answer: fresh build with `1.2.3\xff` must fail with
  "WYRD_VERSION is not usable … not valid UTF-8"; `0.1.0+git.3.abc12de.dirty` is baked
  verbatim; non-UTF-8 again fails; `bad value` fails ("Docker tag"); unset → `0.0.0+git.<sha>`.
- Harness changes: `step` returns status + raw stdout without asserting (`:95`); `try_build`
  takes an optional raw `WYRD_VERSION` and returns `Err(stderr)` on failure (`:219`);
  `git_in` takes `OsStr` args; `git_bytes` (`:153`) and `workspace_with_one_commit` (`:164`)
  helpers. Every step stays under the existing 300 s kill budget.

Pure unit tests (`derivation.rs`, run in both consumers):
`a_readable_sha_is_used_when_describe_gave_no_answer` (`:469`),
`an_undecodable_tag_fails_closed_to_the_sha_form_and_never_goes_missing` (`:487`),
`the_override_variable_is_passed_on_as_text_and_unset_is_none` (`:514`),
`an_override_that_is_not_utf8_is_an_error_never_unset` (`:526`, unix).

All of these sit in source files, not `*/tests/*.rs`, so the verify classifier still sees one
added test (`run-verify.sh --classify results/issue_778/patch.diff` →
`ADDED_TEST crates/server/tests/build_identity_startup_log.rs`, `CRATE crates/server`,
`CRATE xtask`). The discriminator test file is byte-identical to round 3's (`cmp` against the
bundle copy).

### Red against round 3

Round-3 `build.rs` and `derivation.rs` swapped in, new tests kept, then restored and
`cmp`-checked:

```
test version::build_script_tests::an_override_is_baked_verbatim_and_one_that_is_not_utf8_fails_the_build ... FAILED
test version::build_script_tests::an_undecodable_tag_still_names_the_commit_by_its_sha ... FAILED
…build_script_tests.rs:498:22: WYRD_VERSION="1.2.3\xFF" must fail the build, but it baked `0.0.0+git.c7f95c0`
…build_script_tests.rs:475:5: a tag that is not UTF-8 must not discard the readable sha
  left: "0.0.0+git.unknown"
 right: "0.0.0+git.c7f95c0"
test result: FAILED. 3 passed; 2 failed
```

(Line numbers from before the tightening edit below; the `must fail the build` panic is now
`:501`.) Those are exactly the two symptoms the reviewers reported. Split check, only `build.rs`
reverted: at first the tag test stayed green, because the resolver change alone also
produces the sha. I tightened it to require the warning to quote git's output (only the
decode path does that), and re-ran: both tests FAILED with only `build.rs` reverted
(`:501` "must fail the build"; `:482` "the fail-closed build must say why"). With the fix:
`test result: ok. 21 passed` for `version::`.

## Discriminator evidence (C4-verify, project gate, final patch)

`PDCA_LANE=1 PDCA_BUNDLE=results/issue_778 PDCA_BASE=0fba76c098a06e79c15a697c24b67958caef0ffe ./engine/scripts/run-verify.sh`:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test build_identity_startup_log (fix applied)
test result: ok. 1 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-server --test build_identity_startup_log (production reverted, test kept)
panicked at crates/server/tests/build_identity_startup_log.rs:205:9:
the `role started` event carries no string `version` field: {"fields":{"dservers":0,"listen":"127.0.0.1:36493","message":"role started","region":"us-east-1","role":"s3"},…}
run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
```

No `build_identity_startup_log:` note on the green leg, so neither the skip nor the override
branch fired: leg 2 ran against the verify worktree's own `HEAD`.

### Refute-your-own-test

- **(a) Genuine red?** Yes. The verify gate reverted production (deleted `build.rs`,
  `version.rs`, `version/*`; restored `cli.rs`, `lib.rs`, `dist.rs`, Dockerfile, docs,
  `dist_templates.rs`), kept the test, ran 1 test, and it failed on the missing `version`
  field (`build_identity_startup_log.rs:205`). For this round's behaviour, both new
  real-build tests failed against round 3's `build.rs` + `derivation.rs`, and against
  round 3's `build.rs` alone (above).
- **(b) Production path?** Yes. The discriminator spawns the real `CARGO_BIN_EXE_wyrd`,
  whose real `build.rs` computed the value and whose real `cmd_s3` logged it
  (`cli.rs:2398`); its expectation comes from its own `git` calls, not from production
  code. The new real-build tests compile the real `crates/server/build.rs` by absolute path
  with the cargo of the pinned toolchain; the pure tests call the production functions.
- **(c) Fixture includes the fault?** Yes. The tag test asserts `git describe` really
  printed non-UTF-8 bytes before building. The env test hands cargo the raw bytes
  `1.2.3\xff` in a repository where rung 2 would answer, so a value read as unset is
  visible as the git identity; its first build is fresh, so cargo's own env-check gap cannot
  hide the fault.

## CI (`cargo xtask ci`, project runner)

`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l1 ./engine/xtask.sh ci` → `xtask ci: all checks
passed`, exit 0. New/changed tests seen passing: all 17 `version::derivation::tests::*` and
the same 17 as `dist::identity::tests::*`, the 5 `version::build_script_tests::*`,
`the_s3_role_logs_the_build_identity_derived_from_git`,
`the_build_identity_derivation_is_one_file_with_two_consumers`,
`the_image_build_hands_the_version_to_the_binary`,
`the_dockerfile_declares_the_version_arg_in_the_build_stage`,
`dist_validates_its_version_with_the_shared_rule`, and the unmodified
`normalize_describe_covers_all_three_shapes` (`xtask/tests/dist_templates.rs:336`, base and
patched; the patch removes zero lines from that file). Zero `wyrd-server build identity`
warnings in the CI log (the worktree builds on rung 2).

CI ran before one comment-only edit (`build.rs:25-28`). On the final tree I re-ran
`cargo fmt --all -- --check` (clean), `typos` over every touched server file (clean),
`cargo clippy -p wyrd-server --all-targets -- -D warnings` (exit 0),
`cargo test -p wyrd-server --lib version::` (21 passed), and the verify gate above (it used
the final `patch.diff`).

Formatter / hooks: `cargo fmt --all` then `--check` clean. The repo has no
`.pre-commit-config.yaml`, `core.hooksPath` or `.githooks` (as in round 3). `patch.diff` is
`git diff HEAD` with new files added `-N` (index reset afterwards): 11 files, +2003/−93;
`git apply --cached --check` succeeds on a throwaway index read from `origin/main`.

## Gate notes for the human

- **C4-diff-cov** should now be measurable: the patch applies on `origin/main`.
- **C5-mutants**: the pre-declared baseline failure (`repo_hygiene_guards.rs:137` needs
  `git ls-files`; the mutants scratch copy has no `.git`). There, `wyrd-server` builds on
  rung 3 and the discriminator takes its visible skip.
- `review-batch.md`'s two T4 findings (`build.rs:53`, non-UTF-8 `WYRD_VERSION`) are fixed;
  no `review-rejected.md` written.

## What this slice does NOT prove (stated plainly, per the brief)

- **End-to-end equality of a shipped binary's identity with its tarball's `VERSION` is not
  demonstrated.** It needs `cargo xtask dist` (Docker + network), outside `ci` by design; I
  did not attempt an image build. What is proven is the coupling: one normalizer
  (`normalize_describe_covers_all_three_shapes`, unmodified) and `dist` passing the very
  `version` binding it writes to `VERSION` into the image build (`image_build_args`,
  `xtask/src/dist.rs:207`/`:226`, asserted container-free in `dist_templates.rs`) and into the
  `--host` build (`dist.rs:614`). Two routes exist if the human wants the pair observed
  before a tag: the release workflow's `workflow_dispatch`, or a local `cargo xtask dist`
  then comparing `target/dist/wyrd-*/VERSION`'s `version:` line with the extracted
  `bin/wyrd s3 … --log-format json` startup event's `version`. The release-smoke comparison
  is #779's.
- On an untagged checkout with no override the identity is `0.0.0+git.<sha>`: a real build
  identity, not a release version.
- A hand-built binary from an edited tree advertises its base commit (Decision 1).
- The cargo env-check gap above (incremental unset → non-UTF-8 `WYRD_VERSION`).

## Still for the human (deferred from earlier rounds, untouched here)

- C1: the discriminator asserts equality with `WYRD_VERSION` and skips the git leg only when
  the TEST was compiled with a non-empty `WYRD_VERSION` (`build_identity_startup_log.rs:25`,
  `:216`). Every gate run has it unset.
- Compose-built images (`deploy/small-multi-node-fdb/docker-compose.yml` builds this
  Dockerfile with only `FEATURES`) log `0.0.0+git.unknown`, warned at build time. Outside
  scope (c).
- Whether to narrow the `refs/` watch (only the doc sentence was corrected this round).
- Known watch-set limits, unchanged: `config` (`core.abbrev`) and `info/grafts` are not
  watched.

No external dependency beyond what the brief lists was needed (git and cargo were already
required). Scratch work is under `$PDCA_SCRATCH/pdca-builder-778-r4-*` (toy crates, the
round-3 file copies, the throwaway apply-check index, and the gate logs); left for the harness
to reclaim.
