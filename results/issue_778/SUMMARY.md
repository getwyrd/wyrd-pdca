# Result — issue 778 / wyrd-build-identity-derived-and-logged

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Nothing compiled into the `wyrd` binary can report which checkout produced
  it. Verified on `origin/main` at `a801997`: `crates/proto/build.rs` is the tree's ONLY
  build script (`git ls-tree -r --name-only origin/main | grep build.rs`), and the `s3`
  role's `role started` event records `role`, `listen`, `region`, `dservers` and nothing
  else (`crates/server/src/cli.rs:2199-2206`). The packaging pipeline DOES derive a version
  — `derive_version` → `normalize_describe` (`xtask/src/dist.rs:356-364`, `:127-160`) — and
  writes it to the tarball's `VERSION` file (`:566-571`), but that string exists only
  outside the binary. Worse, it cannot be re-derived where the shipped binary is compiled:
  `.dockerignore:6` excludes `.git/`, and `dist` builds the binary inside the image so that
  "the tarball's `bin/wyrd` is bit-identical to the image's `/usr/local/bin/wyrd`"
  (`xtask/src/dist.rs:5-7`). So the artifact's `VERSION` and the binary's self-knowledge
  have no common source and no way to acquire one without an explicit hand-off.
- Success criterion: BINDING (demonstrable by C4-verify at Check — no container, no
  cluster, no feature flag): a new integration test spawns the **built `wyrd` binary** as
  an `s3` role (`--s3-listen 127.0.0.1:0 --data-dir <tmp> --access-key … --secret-key …
  --log-format json`, killed on every exit path), reads its stderr, and asserts against that
  one child process — the credential flags are not optional decoration: `cmd_s3` refuses to
  start without them (`crates/server/src/cli.rs:2126-2136`, "there is no anonymous access"),
  and `--data-dir` must be a temp dir because it otherwise defaults to a shared location
  (`cli.rs:2120`):
  1. **the identity is recorded** — the `role started` JSON event carries a **`version`**
     field whose value is non-empty, is not `unknown`, and is not EQUAL to the bare
     workspace placeholder `0.0.0` (equality, not a prefix test: the legitimate derived
     value on an untagged checkout is `0.0.0+git.<sha>`, which starts with `0.0.0` and is
     correct);
  2. **it came from git, not from a constant** — the test independently runs
     `git rev-parse --short HEAD` and `git describe --tags --always` from the workspace
     root. When the independent probe SUCCEEDS and `describe` does not resolve exactly onto
     a tag, the recorded value MUST CONTAIN that short sha. Containment rather than string
     equality: it binds the value to the real derivation while staying immune to tag shape,
     and it must not be weakened to a skip. **`unknown`/`0.0.0` while `git rev-parse`
     succeeds is a hard FAILURE, not a skipped leg** — v3's round found a self-minted skip
     predicate that silently disabled the only provenance check while staying green
     (`iteration-v3/` carry-forward, item 2). Skipping is permitted only when the
     independent probe itself fails (no repository visible at all), and then the leg must
     print why.
  The test MUST NOT call the production normalizer or reference any symbol this patch
  introduces — see Falsifiability.
  ALSO REQUIRED, gated by `cargo xtask ci` rather than by the discriminator (see
  Verification posture): (3) the derivation is single-sourced — the existing
  `normalize_describe_covers_all_three_shapes` test (`xtask/tests/dist_templates.rs:301`)
  keeps passing **unmodified**, which pins `dist`'s consumer to the one shared definition;
  and (4) the packaging hand-off is asserted container-free — the image build's argument
  vector carries `WYRD_VERSION=<the same string `dist` writes to `VERSION`>`, and the
  Dockerfile declares `ARG WYRD_VERSION` in the build stage; and (5) **the dirty-checkout
  contract is pinned by a pure-function test** — the rung resolution is extracted as a pure
  function (env value, describe output, fallback ⇒ identity) so the build script is a thin
  caller of it, and its unit tests assert BOTH halves of the scoping decision: a rung-1 value
  is used **verbatim, including a `.dirty` suffix** (this is what makes a dist build on a
  dirty tree byte-equal to its own `VERSION`), and a rung-2 derivation **never** produces
  one (no `--dirty` is passed and no `.dirty` reaches the identity, Decision 1). The same
  pure function is where the validator's rules (leading `-`/`.`/`+`, length cap, fail-closed
  to the sha form) get their tests. These are green-only assertions run by `cargo xtask ci`,
  in the production crate and reverted with it — they are NOT part of the discriminator test
  file and do not weaken its red leg. If the pure function must be shared between `build.rs`
  and the crate, an `include!`d module is the cheap idiom; say what you chose in
  `build-notes.md`.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) a build script for `crates/server` bakes the build identity into the
  binary and exposes it as a public crate-level constant, named EXACTLY
  `wyrd_server::version::BUILD_IDENTITY` (a `&str`) — the path and the name are fixed here,
  not left to taste, because #779 reads that constant and its Do never sees this brief;
  (b) the identity's derivation is single-sourced with `cargo xtask dist`'s
  — one definition of the normalizer, compiled by both consumers, with the dependency
  direction tooling→product and never the reverse; (c) `cargo xtask dist` passes the version
  it already derived INTO the image build as a build argument, and the Dockerfile declares
  it in the build stage and exports it to the `cargo build`, so the shipped binary carries a
  real identity rather than the fallback; (d) the `s3` role's `role started` event gains a
  **`version`** field (that field name is fixed here too — #779 asserts on it) carrying the
  same string. **/ out of scope:** any wire surface — no `Server:` header, no `S3Config`
  change, nothing in `crates/gateway-s3` (that is #779, and a patch touching it here will be
  rejected); `--dirty` or any working-tree claim (Decision 1); a `wyrd --version`
  subcommand; the `role started` events of the `d-server` and `custodian` roles
  (`cli.rs:1039`, `:1664`) — uniform and cheap, but a separate issue, not scope creep here;
  changing the workspace `version = "0.0.0"` placeholder (root `Cargo.toml`); the OCI
  `org.opencontainers.image.version` label, which `dist` already sets
  (`xtask/src/dist.rs:458`).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.71s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #778’s shared build identity, packaging hand-off, and S3 startup logging pass their normal-path checks, but two reproduced encoding defects remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the public identity contract and scopes artifact equality to a dist-built pair; Docker observation is explicitly deferred to #779, not claimed at Check (`brief.md:137`, `brief.md:205`, `brief.md:324`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while keeping the discriminator produced a compiled, running test failure on the missing startup `version` field (`reviewer-evidence/startup-red.log:281`; `crates/server/tests/build_identity_startup_log.rs:205`). |
| C3 Change | PASS | The submitted scope addresses binary provenance and packaging without entering #779’s wire surface; the required constant and living architecture documentation are present (`crates/server/src/version.rs:44`, `docs/design/architecture/07-deployment-view.md:42`, `patch.diff:258`). |
| C4 Verification (red→green) | PASS | Restoring the patch independently passed the startup test, 15 version tests, and 31 packaging tests on the supplied base; current-main diff coverage remains unmeasured (`reviewer-evidence/startup-green.log:10`, `reviewer-evidence/version-unit.log:23`, `reviewer-evidence/dist-templates.log:47`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve provenance at both byte-decoding boundaries — invalid explicit overrides silently become another identity, and an undecodable Git tag discards a readable SHA; both are reproduced implementation gaps (`crates/server/build.rs:53`, `crates/server/build.rs:216`, `crates/server/src/version/derivation.rs:204`). |
| T1 Structure | PASS | Product code owns the shared derivation and tooling consumes it without reversing dependencies; independent unsafe-root, dependency-boundary, and statics scans pass (`xtask/src/dist.rs:181`, `reviewer-evidence/repo-guards.log:7`, `reviewer-evidence/statics.log:4`). |
| T2 Shape | PASS | The discriminator remains independent of new production symbols, and existing normalizer behavior remains pinned; formatting, spelling, and documentation lint pass (`crates/server/tests/build_identity_startup_log.rs:19`, `xtask/tests/dist_templates.rs:338`, `reviewer-evidence/fmt.log:3`, `reviewer-evidence/typos.log:3`, `reviewer-evidence/docs-lint.log:2`). |
| T3 Runtime | PASS | Startup adds a compiled constant to the existing event; real incremental-build tests confirm shallow-boundary refresh, fallback recovery, and no rebuild on ordinary edits/index writes (`crates/server/src/cli.rs:2393`, `reviewer-evidence/version-unit.log:19`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and the substantive audit must rerun at publish; the separate batch-review defect is adjudicated under C5 below (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | PASS | Affected-path prior-art checks found no competing build-identity implementation or rejected approach; the patch removes the missing provenance source rather than hiding a runtime capability failure (`reviewer-evidence/prior-art-summary.log:1`, `crates/server/build.rs:74`, `xtask/src/dist.rs:767`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the declared container-free coupling proof is sufficient for this slice — Docker and the real binary/tarball pair were not exercised; end-to-end equality remains #779’s tracked release-smoke work (`brief.md:205`, `deploy/docker/wyrd/Dockerfile:89`, `xtask/src/dist.rs:767`). |

All source citations above and below resolve inside `$PDCA_TARGET` (`target/` in this sandbox). Evidence and brief citations resolve in the review directory. Review is advisory; these judgments do not replace deterministic gates.

Two implementation defects need correction:

1. **Reject a non-UTF-8 explicit override instead of substituting Git provenance.** At `crates/server/build.rs:53`, `.ok()` collapses `VarError::NotUnicode` into the same state as an absent variable. Executing the unchanged, compiled build script with `WYRD_VERSION` bytes `b'1.2.3\xff'` succeeds and emits `0.0.0+git.8c7dcfa`; an invalid ASCII override correctly fails, and a valid dirty override is retained verbatim (`reviewer-evidence/nonutf8-override.log:3`, `:10`, `:17`). A real `cargo build --offline -p wyrd-server --bin wyrd` with the invalid byte value also succeeds (`reviewer-evidence/nonutf8-cargo-build.log:1`). This contradicts the explicit-hand-off rejection contract at `crates/server/src/version/derivation.rs:133`. Distinguish absence from invalid Unicode and add a build-script boundary regression. The two entries in the frozen batch-review log describe this same defect, counted once.

2. **Retain a readable commit SHA when Git’s tag name cannot decode as UTF-8.** Git accepts a tag with bytes `b'v1.2.3-\xff'`. In an independent repository, the unchanged build script initially emits `0.0.0+git.1da2744`; adding that tag at the same HEAD changes its output to `0.0.0+git.unknown`, even though `git rev-parse --short HEAD` still succeeds (`reviewer-evidence/nonutf8-tag.log:1`, `:9`, `:11`, `:21`). `String::from_utf8(...).ok()?` drops the describe output at `crates/server/build.rs:216`, and the `None` branch at `crates/server/src/version/derivation.rs:204` never tries the available SHA. That violates the brief’s exotic-tag fallback and successful-probe provenance requirements (`brief.md:46`, `brief.md:307`). Preserve the decode failure and fall back to the validated SHA with a warning; exercise this with real Git output, since `&str` resolver tests cannot represent these bytes.

Independent verification supports the ordinary path. Production changes were stashed and restored in the disposable target; the added discriminator compiled in both states. Red failed on the missing field, green passed, and the source tree was restored. The version suite’s three real Cargo/Git incremental regressions all pass, including full→shallow→full transitions. The packaging suite passes the unchanged three-shape normalizer test and build-argument assertions. `cargo fmt --all -- --check`, `typos`, docs lint, real repository guards through `ci-dry-run`, and the statics scanner pass. The dry-run’s printed clippy/build/test commands were not counted as executions.

The frozen gates have these additional limits and outcomes:

- **C4-ci / host-tikv: PASS from captured execution.** The full CI log ends with all checks passed, and both TiKV feature clippy invocations finish successfully (`gate-logs/C4-ci.log:4006`, `gate-logs/host-tikv.log:110`, `:209`). Full workspace CI and those feature checks were not independently repeated; the targeted reruns above are separate evidence.
- **C4-diff-cov: no coverage measurement.** The wrapper reports that the patch does not apply to its `origin/main` base (`gate-logs/C4-diff-cov.log:10`). The supplied disposable base does compile and demonstrates red→green. This is a base-state caveat, not a verified patch compilation defect or a current-main merge-result proof.
- **C5-mutants: no mutants tested.** The unmutated baseline fails because `git ls-files` cannot run in its scratch repository context; the failure is in the existing repository-hygiene test (`gate-logs/C5-mutants.log:2015`, `:2029`). This supplies neither surviving-mutant evidence nor a patch defect.
- **T4-batch-review: one distinct confirmed defect.** Its two non-UTF-8 override findings are duplicates and independently reproduced above (`gate-logs/T4-batch-review.log:10`). **T4-contribution: N/A**, substantively deferred to publish, with nothing to clear at Check (`gate-logs/T4-contribution.log:10`).

Prior art was checked by all 11 affected paths against GitHub default-branch commit history, then against the changed-file lists of every closed-unmerged PR: 356 closed PRs, 19 unmerged, no pagination truncation. The only closed-unmerged path overlap was unrelated segmented-map work in PR #647. No build-identity predecessor appeared; the parent #736 attempts are already identified and superseded by the brief (`reviewer-evidence/prior-art-summary.log:1`, `brief.md:261`). No `INTEGRATION.md` was present in the supplied target. The standing rubric at `AGENTS.md:122` was applied, and tracked deferrals were not reopened.

### Advisory — adversary

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

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/build.rs:53`: Reject a non-UTF-8 `WYRD_VERSION` instead of treating it as absent. `std::env::var(...).ok()` discards `VarError::NotUnicode`, bypassing the explicit-override validator and silently substituting a Git-derived or fallback identity. Running the target build script with `WYRD_VERSION` bytes `1.2.3\xff` succeeded and emitted `0.0.0+git.unknown`; an invalid ASCII override correctly failed. Distinguish `NotPresent` from `NotUnicode` and add a regression at the environment-reading boundary. The two frozen T4 entries describe this same defect.

No additional correctness or actionable reuse, simplification, or efficiency findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve provenance at both byte-decoding boundaries — invalid explicit overrides silently become another identity, and an undecodable Git tag discards a readable SHA; both are reproduced implementation gaps (`crates/server/build.rs:53`, `crates/server/build.rs:216`, `crates/server/src/version/derivation.rs:204`).
- [ ] Validation — fitness-to-purpose — Decide whether the declared container-free coupling proof is sufficient for this slice — Docker and the real binary/tarball pair were not exercised; end-to-end equality remains #779’s tracked release-smoke work (`brief.md:205`, `deploy/docker/wyrd/Dockerfile:89`, `xtask/src/dist.rs:767`).
- [ ] `crates/server/build.rs:53` (used at `:55`): `std::env::var("WYRD_VERSION").ok()` turns a **non-UTF-8 override into "unset"**, so the build silently drops to rung 2 with no error and no `cargo:warning`. I reproduced it with the patch's build script in a probe crate: `WYRD_VERSION='bad value'` panics with a clear message ("WYRD_VERSION is not usable …"), but `WYRD_VERSION=$'\xff\xfe'` exits 0 and bakes `0.0.0+git.1f0a62e`. That breaks the contract stated at `crates/server/src/version/derivation.rs:137` ("an invalid one is an `Err`: … a silent substitute would break exactly the equality it exists for"). It is also the rubric's *absent or unsupported entries → never a silent skip* class. This **confirms T4's two blocking items** (`gate-logs/T4-batch-review.log`); they are one defect, not new. Fix: read `std::env::var_os`, and when the value is present but not UTF-8, panic the same way an invalid value does. The pure `resolve` takes `&str`, so it cannot pin this. A unix-only case in `src/version/build_script_tests.rs` can: build with `OsStr::from_bytes(b"\xff")` and assert the build fails.
- [ ] `gate-logs/C4-diff-cov.log:10`: the bundle **still does not apply on `origin/main`**. This is the third round in a row; the round-2 carry-forward already said "rebase and re-run the gates". The target tree is the patch on the "pre-fix base df68932f", and the `cmd_s3` hunk sits at `crates/server/src/cli.rs:2388` (the brief cites `:2199`). So the C4-ci and C4-verify greens are evidence for that old base, not for the merge result, and diff coverage was never measured. The builder has now failed to move off this base twice when told to, which suggests the driver is handing Do a stale base. A human should rebase, or tell the driver to, before sign-off trusts these gates.
- [ ] `crates/server/src/version/derivation.rs:257` (`common_dir.join("refs")`, watched recursively) against the doc claim at `crates/server/build.rs:13` ("re-runs when an input to that answer can have moved, **and at no other time**"). That claim is false. The whole `refs/` tree is watched, so a ref that cannot change `git describe --tags` of *this* HEAD still re-runs the script and recompiles the crate. I reproduced this with the patch's build script, a main repo, and two linked worktrees, building the probe in `wtA`:
- [ ] `crates/server/build.rs:53`: Reject a non-UTF-8 `WYRD_VERSION` instead of treating it as absent. `std::env::var(...).ok()` discards `VarError::NotUnicode`, bypassing the explicit-override validator and silently substituting a Git-derived or fallback identity. Running the target build script with `WYRD_VERSION` bytes `1.2.3\xff` succeeded and emitted `0.0.0+git.unknown`; an invalid ASCII override correctly failed. Distinguish `NotPresent` from `NotUnicode` and add a regression at the environment-reading boundary. The two frozen T4 entries describe this same defect.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- [ ] C1 Spec — Decide whether the SHA discriminator is restricted to builds without an override — the mandatory SHA assertion rejects a valid `WYRD_VERSION=1.2.3`, contradicting rung-1 precedence; `brief.md:48`, `brief.md:341`, `crates/server/tests/build_identity_startup_log.rs:181`, `reviewer-evidence/override.log:13`.
- [ ] C5 Causal adequacy — Decide whether to permit shallow-boundary invalidation or explicitly exclude it — real Git deepening changes the derived identity while Cargo retains the old value, but the binding plan restricts watches to HEAD/refs; `brief.md:287`, `crates/server/build.rs:119`, `reviewer-evidence/shallow-no-ref-update.log:35`.
- [ ] `deploy/docker/wyrd/Dockerfile:47` (`ARG WYRD_VERSION=""`): **compose-built `s3` gateways always log `version=0.0.0+git.unknown`.** `deploy/small-multi-node-fdb/docker-compose.yml:219-223` builds `wyrd` from this same Dockerfile, passes only `FEATURES`, and runs three `s3` gateways (`:389`, `:406`, `:423`). With no `WYRD_VERSION` and no `.git` in the build context, every gateway logs that value. Nothing warns about it: rung 3 emits no `cargo:warning`. The architecture doc lists docker-compose as a shipped artifact. The brief states its invariant over the whole build/packaging category ("violated equally by … a shipped binary whose build stage cannot see the repository"), but scope (c) only covers `dist`. The new sentence at `docs/design/architecture/07-deployment-view.md:42`, "Every `wyrd` names the checkout it was built from", overclaims for these builds. Human call: accept this as an out-of-scope follow-up (file an issue to pass `WYRD_VERSION: ${WYRD_VERSION:-}` through the compose `args`, and soften the doc sentence), or widen this slice.
- [ ] `crates/server/build.rs:43` / `crates/server/src/version/derivation.rs:256`: **the gating T4 block (2 findings, same issue) is real but much narrower than stated, and its obvious fix conflicts with Decision 1.** Reproduced: on a full clone 3 commits past `v0.1.0`, a plain `git fetch --depth=1 origin` *did* refresh the identity (the fetch touched `.git/refs`, which is watched). Only a fetch that updates no ref — `git fetch --depth=1 origin <sha>` — left the baked `0.1.0+git.3.67ea88c` while `git describe` now printed `67ea88c`. The stale value still names the same commit (the tag is still an ancestor; the shallow boundary only hides it), so under Decision 1 ("the identity names the commit") it is out of date in wording, not wrong about the commit. The only cargo-level fix is to always emit the missing `shallow` path, which makes cargo re-run the script — and recompile `wyrd-server` plus its integration-test binaries — on **every** build in every full clone. That is exactly the relink cost Decision 1 rules out. My recommendation: record T4's two findings as rejected, with this reason. It is a human call because T4 is a gating row.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome:
- Iteration delta (if iterating):
- By / date:

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
