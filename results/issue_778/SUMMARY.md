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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.9% — 387 of 408 instrumentable changed lines executed (floor 80%); 408 of 1602 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.78s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #778: derive one build identity for `wyrd` and its distribution artifacts, and record it in every S3 startup event.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The observable startup criterion, exact public constant, commit-only local identity, and distribution hand-off are specified; artifact-pair equality is explicitly outside Check (`brief.md:33`, `brief.md:137`, `brief.md:205`). |
| C2 Reproduction (red pre-fix) | PASS | With production stashed and the discriminator retained, one test compiled and failed because the startup event lacked `version` (`crates/server/tests/build_identity_startup_log.rs:205`; `reviewer-evidence/startup-red.log:279`). |
| C3 Change | PASS | The footprint addresses the declared build/logging/packaging surfaces and updates the living architecture; no S3 wire change or unrelated role expansion (`crates/server/src/cli.rs:2398`, `xtask/src/dist.rs:226`, `docs/design/architecture/07-deployment-view.md:42`). |
| C4 Verification (red→green) | PASS | Restoring the patch makes the discriminator pass; 31 packaging tests also pass locally. Full CI is supported by the frozen log, with the independent full-run limitation recorded below (`reviewer-evidence/startup-green-restored.log:9`, `reviewer-evidence/packaging-guards.log:38`, `gate-logs/C4-ci.log:4036`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve Unicode tag bytes until validation — trimming lets different commits advertise the same release identity instead of warning and using their SHAs (`crates/server/src/version/derivation.rs:205`, `crates/server/src/version/derivation.rs:42`; `reviewer-evidence/unicode-tag.log:37`). |
| T1 Structure | PASS | One shared derivation keeps tooling dependent on product source without a reverse crate dependency; the build-script root forbids unsafe code and the public API is documented (`xtask/src/dist.rs:181`, `crates/server/build.rs:54`, `crates/server/src/version.rs:45`). |
| T2 Shape | PASS | Resolution, environment decoding, Git discovery, and packaging arguments have separate boundaries; the existing normalization contract remains unchanged and passes (`crates/server/src/version/derivation.rs:142`, `crates/server/build.rs:259`, `xtask/tests/dist_templates.rs:336`; `reviewer-evidence/packaging-guards.log:13`). |
| T3 Runtime | PASS | Startup reads a compiled constant; real incremental-build tests cover commit changes, shallow transitions, fallback recovery, and avoiding index-triggered rebuilds (`crates/server/src/cli.rs:2398`, `crates/server/src/version/build_script_tests.rs:308`; `reviewer-evidence/ci.log:2068`). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design; their substantive audit must run at publish. The separate batch-review defect is counted once in C5 (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | PASS | The prior-art check covers every affected path and all closed unmerged PRs; no competing identity implementation or rejected identity approach was found. The explicit fallback design does not mask a runtime load-time cause (`reviewer-evidence/prior-art-summary.log:1`, `reviewer-evidence/prior-art-summary.log:32`, `crates/server/build.rs:73`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept commit-only local identity and container-free coupling as sufficient for this slice, or require a release comparison — Docker/network image-to-tarball equality was not exercised (`brief.md:205`, `crates/server/src/version.rs:26`, `xtask/src/dist.rs:767`). |

One implementation defect remains: Unicode whitespace in a valid Git tag can silently erase the distinction between source commits. The binding startup regression is independently red→green, and the packaging coupling tests pass. Actual equality between a shipped binary and its tarball remains the declared human validation decision.

Source citations above and below are relative to `$PDCA_TARGET`; `brief.md`, `gate-logs/`, and `reviewer-evidence/` citations are relative to this review directory. The disposable target was usable, and the stash/pop check restored its original patch state (`reviewer-evidence/red-green.log:26`). No production fix was made.

**C5 implementation concern (P2): preserve tag bytes before validating them.** Both `resolve` and `normalize_describe` call Unicode-aware `trim()`. Git accepts Unicode whitespace in tag names, so these calls can delete meaningful tag bytes before `validate_identity` sees them. I compiled the target's unchanged build script and ran it against a real Git repository with three successive commits:

| Commit | Actual tag, escaped for visibility | Observed build identity |
|--------|-----------------------------------|-------------------------|
| `4ab9201` | `v1.2.3` | `1.2.3` |
| `5afade6` | `v1.2.3\u{00A0}` | `1.2.3` |
| `bbc8995` | `\u{2003}v1.2.3` | `1.2.3` |

The two exotic tags produced no warning and no SHA fallback (`reviewer-evidence/unicode-tag.log:24`, `reviewer-evidence/unicode-tag.log:37`, `reviewer-evidence/unicode-tag.log:41`, `reviewer-evidence/unicode-tag.log:54`). This violates the brief's prohibition on collapsing forbidden bytes into a shared identity (`brief.md:307`). It independently confirms the frozen batch-review finding; it is one defect, not an additional finding. Remove only Git's output delimiter at both normalization boundaries and add a regression proving that Unicode-whitespace tags retain distinct SHA identities with warnings. Fixing only the trim in `resolve` leaves the shared normalizer's trim active, including for `dist` (`xtask/src/dist.rs:546`).

**The startup criterion is demonstrated on the real binary.** With the production patch stashed, the retained integration test failed on the missing JSON field, not a compile error; after restoration it passed under the target's pinned Rust/Cargo 1.96.0. A separate launch observed `fields.version = "0.0.0+git.1f70bc8"`, matching this disposable target's HEAD, and the child was killed and reaped (`reviewer-evidence/startup-observation.log:5`). The original normalization test, Docker argument hand-off assertions, Dockerfile stage assertion, and all 29 repository-hygiene tests also passed (`reviewer-evidence/packaging-guards.log:13`, `reviewer-evidence/packaging-guards.log:21`, `reviewer-evidence/packaging-guards.log:24`, `reviewer-evidence/packaging-guards.log:73`).

**Gate evidence supports only the claims below.** Instance-scoped wrappers were not invoked from another checkout; their frozen logs were read where the exact wrapper could not be rerun here.

| Gate | Adjudication | Evidence and limits |
|------|--------------|---------------------|
| C4-ci | PASS, frozen full run | The frozen run ends with all checks passed (`gate-logs/C4-ci.log:4036`). Independently, typos, docs lint/render, initial scanners, fmt, clippy, build, and the identity unit/build tests passed; the full local run was interrupted during unchanged custodian tests after seven exceeded 60 seconds (`reviewer-evidence/ci.log:2190`). All ten custodian tests subsequently passed serially in 0.64 seconds (`reviewer-evidence/custodian-serial.log:18`). This is an incomplete independent full run, not a demonstrated patch defect. |
| C4-verify | PASS, independently reproduced | One genuine assertion failure before the fix and one pass afterward (`reviewer-evidence/startup-red.log:286`, `reviewer-evidence/startup-green-restored.log:9`), agreeing with `gate-logs/C4-verify.log:35`. |
| C4-diff-cov | PASS, frozen measurement | 387/408 instrumentable changed lines executed (94.9%); only 408/1602 changed lines were instrumentable. This is not 94.9% coverage of every changed line (`gate-logs/C4-diff-cov.log:442`, `gate-logs/C4-diff-cov.log:445`). |
| C5-mutants | N/A, baseline failure | The unmutated snapshot failed the `git ls-files` prerequisite; no mutants were tested. No survivor or patch defect can be inferred (`gate-logs/C5-mutants.log:2022`, `gate-logs/C5-mutants.log:2036`). The corresponding real-index test passed locally (`reviewer-evidence/packaging-guards.log:69`). |
| T4-batch-review | FAIL, finding confirmed | The one Unicode-trimming finding reproduces with real tags and is routed through C5 above (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A, deferred | The publish gate owes the substantive audit after contribution artifacts are drafted; nothing is missing unexpectedly (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS, frozen compile evidence | Both feature-specific clippy invocations completed; this establishes compilation, not live TiKV service behavior (`gate-logs/host-tikv.log:3`, `gate-logs/host-tikv.log:211`). |

**Prior-art checking is complete for this surface.** Remote default-branch commit histories were queried by all eleven affected paths. All 374 closed PRs were enumerated and the file lists of all 19 unmerged closed PRs checked. The only rejected-work overlap was PR #647's unrelated chunk-map changes in `crates/server/src/lib.rs`; it carried no identity changes (`reviewer-evidence/prior-art-summary.log:1`, `reviewer-evidence/prior-art-summary.log:22`, `reviewer-evidence/prior-art-summary.log:32`). The brief's earlier #736 attempts were treated as recorded prior decisions, not reopened findings.

**Human validation still concerns the artifact pair.** No image build was attempted, as the brief explicitly excludes it from this slice. The evidence proves a shared derivation and the argument hand-off, not an executed Docker build yielding a matching tarball. To observe the pair at sign-off, run `cargo xtask dist` in the release checkout, extract the generated `target/dist/wyrd-*.tar.gz`, and launch its `bin/wyrd s3 --s3-listen 127.0.0.1:0 --data-dir <new-empty-directory> --access-key review-key --secret-key review-secret --log-format json`. Compare the startup event's `fields.version` with the extracted `VERSION` file's `version:` line, then stop the process. This requires the Docker/network environment deliberately left outside the binding criterion (`brief.md:212`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 778 (build identity derived and logged)

**Verdict:** the red→green evidence holds. T4's one blocking finding is real. I reproduced it end to end with a real git tag and the patch's own `build.rs`. Fixing only the line T4 cites would not be enough. One lower-priority impersonation case is left for a human to decide.

## Refutations that landed

- NEEDS-HUMAN [impl] — `crates/server/src/version/derivation.rs:205` **and** `:42`: Rust's `str::trim` strips *Unicode* whitespace, and git accepts any byte from 0x80 up in a tag name. So a tag `v1.2.3<U+00A0>` (U+00A0 is the no-break space) becomes `1.2.3`, the same identity as the real `v1.2.3`. It gets `Rung::Describe` (the git-derived rung) with **no warning**. That breaks the brief's rule "distinct inputs must not become one identity". **Reproduced end to end:** I made two repos, one with `git tag v1.2.3` on commit `6d78c19` and one with `git tag $'v1.2.3\xc2\xa0'` on commit `57c56ba`. I built a probe crate whose `build =` is the patch's `crates/server/build.rs` in each repo. **Both binaries baked `1.2.3`**, and neither build printed a `cargo:warning`. The resolver alone gives the same result for `\u{3000}v1.2.3` (ideographic space) and `v1.2.3\u{0085}` (NEL). **Fixing only `:205` is not enough.** `resolve` hands the text to `normalize_describe`, which trims again at `:42`. I patched a copy with `trim_ascii()` at `:205` only, and it still returned `identity="1.2.3" rung=Describe`. With `trim_ascii()` at **both** `:205` and `:42`, rung 2 fails closed to `0.0.0+git.bbbb222`. All five inputs of the pinned `normalize_describe_covers_all_three_shapes` (`xtask/tests/dist_templates.rs:336-357`), plus a `\n`-terminated one, still normalize the same. On the dist side, `derive_version` (`xtask/src/dist.rs:546-554`) then refuses the tag with a clear error instead of silently writing `1.2.3` to `VERSION`. That is safe, because git forbids ASCII whitespace in ref names, so `trim_ascii` only drops the newline. **The discriminator test cannot catch this:** its probe also uses Unicode `trim` (`crates/server/tests/build_identity_startup_log.rs:94`). On such a tag its exact-match branch computes `tag_version = "1.2.3"` and accepts `version.ends_with("1.2.3")` (`:262-264`). Pin it with resolver unit cases in `derivation.rs`'s test module instead: NBSP, U+3000 and U+0085, leading and trailing, should all give `Rung::ShaFallback`.

- NEEDS-HUMAN [human] — `crates/server/src/version/derivation.rs:48-68` (used by rung 2 at `:208-225`): rung 2 trusts the *shape* of the describe text and never checks the sha it parses against HEAD's sha, even though `resolve` already receives `short_sha`. The resolver gave these results: tag `v0.0.0+git.abc12de` on commit `9999fff` → `0.0.0+git.abc12de` (`Rung::Describe`). That is byte-equal to an untagged build of commit `abc12de`. A non-`v` tag `deadbeef` on `1234abc` → `0.0.0+git.deadbeef`, an identity that names a commit the binary was not built from. By the same parse, a tag literally named `v1.0-3-gfoo` gives `1.0+git.3.foo`. The exact-match branch of the integration test (`build_identity_startup_log.rs:262-264`) accepts all of these. This is the same impersonation class the `dirty` refusal at `:209` was added for. A cheap guard: when the normalized form is the bare-sha or past-a-tag shape, require its sha part to match `short_sha`, otherwise fail closed. Every case needs a deliberately odd tag name, and the brief's validator list (leading `-`/`.`/`+`, length, tag-legal bytes) does not mention this. So it is a scope call whether to harden here, and it is fine to decline with a recorded reason. It should not trigger a rebuild on its own.

## Attempted and could not refute

- Red→green evidence: `gate-logs/C4-verify.log` goes RED at `crates/server/tests/build_identity_startup_log.rs:205` because the field is missing (the event shows `dservers, listen, message, region, role` only), and GREEN with the fix. It runs the production path: the built binary (`:19`, `CARGO_BIN_EXE_wyrd`) logs the field from `crates/server/src/cli.rs:2398`. Leg 2 really ran on the green leg. `note()` writes past libtest's capture (`:45-47`), and the verbatim log has no `SKIP provenance leg` line. Leg 2 binds to HEAD's short sha, so a constant could not pass.
- Hand-off coupling: `obtain_binaries` passes the same `version` binding to `image_build_args` (`xtask/src/dist.rs:638`) that `run_dist` uses for `VERSION` (`:767`, `:711-716`). The Dockerfile declares `ARG WYRD_VERSION=""` globally (`deploy/docker/wyrd/Dockerfile:49`) and again in the build stage right before `RUN cargo build` (`:89-90`). An empty value falls through to rung 3 with a warning, never an empty identity (`derivation.rs:195`).
- Leg 3: `normalize_describe_covers_all_three_shapes` is untouched by the diff (all hunks in `xtask/tests/dist_templates.rs` start at `:367`).
- Rerun set (`derivation.rs:292-312`, `build.rs:122-181`): I tried `core.abbrev` changes (config is unwatched), `git maintenance` prefetch writing under `refs/`, a permission-denied `exists()`, and shifting symlink indexes. They are either deliberate tradeoffs the brief chose (Decision 1, documented as "coarse" at `build.rs:25-28`) or fail safe. None is a concrete wrong identity.
- `C5-mutants` red: this is the declared environment noise (`xtask/tests/repo_hygiene_guards.rs:137` needs `.git`). The patch's own tests ran in that no-`.git` baseline and passed: the `wyrd-server` lib tests, including `build_script_tests` (`gate-logs/C5-mutants.log:1084-1162`), and the integration test (`:1187-1190`). The integration test printed its `SKIP provenance leg` reason visibly, which shows the skip path is not silent. So C5 produced no mutation signal on this diff, but the patch did not cause the red.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/version/derivation.rs:205` (also `crates/server/src/version/derivation.rs:42`): Preserve Unicode whitespace in tag names until validation. Git accepts `v1.2.3\u{00A0}` and `v1.2.3\u{3000}`, but `str::trim()` silently removes those characters. A probe compiled against the target resolver returned `1.2.3`, `Rung::Describe`, and no warnings for both tags and plain `v1.2.3`, despite supplying different commit SHAs. Distinct commits can therefore advertise the same identity instead of rejecting the exotic tag and using its SHA. Strip only Git's output terminator at both trimming sites, and add regression coverage for trailing Unicode whitespace. This independently confirms the existing frozen T4 finding.

No additional correctness, reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Preserve Unicode tag bytes until validation — trimming lets different commits advertise the same release identity instead of warning and using their SHAs (`crates/server/src/version/derivation.rs:205`, `crates/server/src/version/derivation.rs:42`; `reviewer-evidence/unicode-tag.log:37`).
- [x] Validation — fitness-to-purpose — Accept commit-only local identity and container-free coupling as sufficient for this slice, or require a release comparison — Docker/network image-to-tarball equality was not exercised (`brief.md:205`, `crates/server/src/version.rs:26`, `xtask/src/dist.rs:767`).
- [x] `crates/server/src/version/derivation.rs:205` **and** `:42`: Rust's `str::trim` strips *Unicode* whitespace, and git accepts any byte from 0x80 up in a tag name. So a tag `v1.2.3<U+00A0>` (U+00A0 is the no-break space) becomes `1.2.3`, the same identity as the real `v1.2.3`. It gets `Rung::Describe` (the git-derived rung) with **no warning**. That breaks the brief's rule "distinct inputs must not become one identity". **Reproduced end to end:** I made two repos, one with `git tag v1.2.3` on commit `6d78c19` and one with `git tag $'v1.2.3\xc2\xa0'` on commit `57c56ba`. I built a probe crate whose `build =` is the patch's `crates/server/build.rs` in each repo. **Both binaries baked `1.2.3`**, and neither build printed a `cargo:warning`. The resolver alone gives the same result for `\u{3000}v1.2.3` (ideographic space) and `v1.2.3\u{0085}` (NEL). **Fixing only `:205` is not enough.** `resolve` hands the text to `normalize_describe`, which trims again at `:42`. I patched a copy with `trim_ascii()` at `:205` only, and it still returned `identity="1.2.3" rung=Describe`. With `trim_ascii()` at **both** `:205` and `:42`, rung 2 fails closed to `0.0.0+git.bbbb222`. All five inputs of the pinned `normalize_describe_covers_all_three_shapes` (`xtask/tests/dist_templates.rs:336-357`), plus a `\n`-terminated one, still normalize the same. On the dist side, `derive_version` (`xtask/src/dist.rs:546-554`) then refuses the tag with a clear error instead of silently writing `1.2.3` to `VERSION`. That is safe, because git forbids ASCII whitespace in ref names, so `trim_ascii` only drops the newline. **The discriminator test cannot catch this:** its probe also uses Unicode `trim` (`crates/server/tests/build_identity_startup_log.rs:94`). On such a tag its exact-match branch computes `tag_version = "1.2.3"` and accepts `version.ends_with("1.2.3")` (`:262-264`). Pin it with resolver unit cases in `derivation.rs`'s test module instead: NBSP, U+3000 and U+0085, leading and trailing, should all give `Rung::ShaFallback`.
- [x] `crates/server/src/version/derivation.rs:48-68` (used by rung 2 at `:208-225`): rung 2 trusts the *shape* of the describe text and never checks the sha it parses against HEAD's sha, even though `resolve` already receives `short_sha`. The resolver gave these results: tag `v0.0.0+git.abc12de` on commit `9999fff` → `0.0.0+git.abc12de` (`Rung::Describe`). That is byte-equal to an untagged build of commit `abc12de`. A non-`v` tag `deadbeef` on `1234abc` → `0.0.0+git.deadbeef`, an identity that names a commit the binary was not built from. By the same parse, a tag literally named `v1.0-3-gfoo` gives `1.0+git.3.foo`. The exact-match branch of the integration test (`build_identity_startup_log.rs:262-264`) accepts all of these. This is the same impersonation class the `dirty` refusal at `:209` was added for. A cheap guard: when the normalized form is the bare-sha or past-a-tag shape, require its sha part to match `short_sha`, otherwise fail closed. Every case needs a deliberately odd tag name, and the brief's validator list (leading `-`/`.`/`+`, length, tag-legal bytes) does not mention this. So it is a scope call whether to harden here, and it is fine to decline with a recorded reason. It should not trigger a rebuild on its own.
- [x] `crates/server/src/version/derivation.rs:205` (also `crates/server/src/version/derivation.rs:42`): Preserve Unicode whitespace in tag names until validation. Git accepts `v1.2.3\u{00A0}` and `v1.2.3\u{3000}`, but `str::trim()` silently removes those characters. A probe compiled against the target resolver returned `1.2.3`, `Rung::Describe`, and no warnings for both tags and plain `v1.2.3`, despite supplying different commit SHAs. Distinct commits can therefore advertise the same identity instead of rejecting the exotic tag and using its SHA. Strip only Git's output terminator at both trimming sites, and add regression coverage for trailing Unicode whitespace. This independently confirms the existing frozen T4 finding.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- [x] size backstop — this slice is behaving oversized: patch is 101 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] C1 Spec — Decide whether the SHA discriminator is restricted to builds without an override — the mandatory SHA assertion rejects a valid `WYRD_VERSION=1.2.3`, contradicting rung-1 precedence; `brief.md:48`, `brief.md:341`, `crates/server/tests/build_identity_startup_log.rs:181`, `reviewer-evidence/override.log:13`.
- [x] C5 Causal adequacy — Decide whether to permit shallow-boundary invalidation or explicitly exclude it — real Git deepening changes the derived identity while Cargo retains the old value, but the binding plan restricts watches to HEAD/refs; `brief.md:287`, `crates/server/build.rs:119`, `reviewer-evidence/shallow-no-ref-update.log:35`.
- [x] `deploy/docker/wyrd/Dockerfile:47` (`ARG WYRD_VERSION=""`): **compose-built `s3` gateways always log `version=0.0.0+git.unknown`.** `deploy/small-multi-node-fdb/docker-compose.yml:219-223` builds `wyrd` from this same Dockerfile, passes only `FEATURES`, and runs three `s3` gateways (`:389`, `:406`, `:423`). With no `WYRD_VERSION` and no `.git` in the build context, every gateway logs that value. Nothing warns about it: rung 3 emits no `cargo:warning`. The architecture doc lists docker-compose as a shipped artifact. The brief states its invariant over the whole build/packaging category ("violated equally by … a shipped binary whose build stage cannot see the repository"), but scope (c) only covers `dist`. The new sentence at `docs/design/architecture/07-deployment-view.md:42`, "Every `wyrd` names the checkout it was built from", overclaims for these builds. Human call: accept this as an out-of-scope follow-up (file an issue to pass `WYRD_VERSION: ${WYRD_VERSION:-}` through the compose `args`, and soften the doc sentence), or widen this slice.
- [x] `crates/server/build.rs:43` / `crates/server/src/version/derivation.rs:256`: **the gating T4 block (2 findings, same issue) is real but much narrower than stated, and its obvious fix conflicts with Decision 1.** Reproduced: on a full clone 3 commits past `v0.1.0`, a plain `git fetch --depth=1 origin` *did* refresh the identity (the fetch touched `.git/refs`, which is watched). Only a fetch that updates no ref — `git fetch --depth=1 origin <sha>` — left the baked `0.1.0+git.3.67ea88c` while `git describe` now printed `67ea88c`. The stale value still names the same commit (the tag is still an ancestor; the shallow boundary only hides it), so under Decision 1 ("the identity names the commit") it is out of date in wording, not wrong about the commit. The only cargo-level fix is to always emit the missing `shallow` path, which makes cargo re-run the script — and recompile `wyrd-server` plus its integration-test binaries — on **every** build in every full clone. That is exactly the relink cost Decision 1 rules out. My recommendation: record T4's two findings as rejected, with this reason. It is a human call because T4 is a gating row.
- [x] `gate-logs/C4-diff-cov.log:10`: the bundle **still does not apply on `origin/main`**. This is the third round in a row; the round-2 carry-forward already said "rebase and re-run the gates". The target tree is the patch on the "pre-fix base df68932f", and the `cmd_s3` hunk sits at `crates/server/src/cli.rs:2388` (the brief cites `:2199`). So the C4-ci and C4-verify greens are evidence for that old base, not for the merge result, and diff coverage was never measured. The builder has now failed to move off this base twice when told to, which suggests the driver is handing Do a stale base. A human should rebase, or tell the driver to, before sign-off trusts these gates.
- [x] `crates/server/src/version/derivation.rs:257` (`common_dir.join("refs")`, watched recursively) against the doc claim at `crates/server/build.rs:13` ("re-runs when an input to that answer can have moved, **and at no other time**"). That claim is false. The whole `refs/` tree is watched, so a ref that cannot change `git describe --tags` of *this* HEAD still re-runs the script and recompiles the crate. I reproduced this with the patch's build script, a main repo, and two linked worktrees, building the probe in `wtA`:

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Follow-up issue: rung 2 / `normalize_describe` use Unicode `trim()` (`crates/server/src/version/derivation.rs:205`, `:42`), so a tag like `v1.2.3\u{00A0}` collapses to `1.2.3` with no warning; strip only git's line ending at both sites and add resolver tests (NBSP, U+3000, U+0085). Accepted as a known gap at #778 sign-off.
- Planning: #778's brief set an absolute rule ("distinct inputs never become one identity") that let reviewers chase rare git edge cases for 4 rounds (9→2→2→1 findings) and pushed a ~3-line feature past 100 KB; briefs should scope hardening to cases that realistically occur and say which edge cases are out of scope.
