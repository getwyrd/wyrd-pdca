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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.22s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #778: embed Wyrd’s build identity, carry dist’s version into the binary, and log it on S3 startup — default red→green is proven; two contract decisions and one test defect remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether the SHA discriminator is restricted to builds without an override — the mandatory SHA assertion rejects a valid `WYRD_VERSION=1.2.3`, contradicting rung-1 precedence; `brief.md:48`, `brief.md:341`, `crates/server/tests/build_identity_startup_log.rs:181`, `reviewer-evidence/override.log:13`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing production changes while retaining the discriminator reproduced a compiled, running test failure on the missing startup `version`; `crates/server/tests/build_identity_startup_log.rs:148`, `reviewer-evidence/startup-red.log:279`. |
| C3 Change | PASS | The patch addresses the declared build/logging scope and preserves the artifact version binding across build and assembly; public identity and deployment documentation are present; `crates/server/src/version.rs:35`, `xtask/src/dist.rs:767`, `docs/design/architecture/07-deployment-view.md:42`. |
| C4 Verification (red→green) | PASS | Restoring the patch made the same binary-level test pass; resolver and packaging tests also passed locally; complete CI is supported by the frozen log, with the local advisory-database lock limitation recorded below; `reviewer-evidence/startup-restored-green.log:6`, `reviewer-evidence/ci-rerun.log:2899`, `gate-logs/C4-ci.log:4000`. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether to permit shallow-boundary invalidation or explicitly exclude it — real Git deepening changes the derived identity while Cargo retains the old value, but the binding plan restricts watches to HEAD/refs; `brief.md:287`, `crates/server/build.rs:119`, `reviewer-evidence/shallow-no-ref-update.log:35`. |
| T1 Structure | PASS | Shared derivation preserves tooling→product dependency direction without adding the server dependency graph to xtask; the new build-script crate root forbids unsafe code; `xtask/src/dist.rs:181`, `crates/server/build.rs:15`. |
| T2 Shape | PASS | Exactly one added integration-test file retains a runnable pre-fix discriminator; the existing normalizer contract is byte-identical and packaging assertions use the production argv function; `crates/server/tests/build_identity_startup_log.rs:18`, `xtask/tests/dist_templates.rs:336`, `xtask/tests/dist_templates.rs:409`, `reviewer-evidence/target-integrity.log:11`. |
| T3 Runtime | PASS | Startup records a compiled constant without runtime Git work; the exercised child uses an ephemeral port, temporary data and bounded startup wait with kill/reap on unwind; `crates/server/src/cli.rs:2393`, `crates/server/tests/build_identity_startup_log.rs:28`, `crates/server/tests/build_identity_startup_log.rs:103`. |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit is owed at publish, not human clearance now; the affected-path prior-art audit completed separately below; `gate-logs/T4-contribution.log:10`, `reviewer-evidence/prior-art.log:1`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Correct the independent probe’s repository scope — inherited `GIT_DIR` makes the test demand a foreign SHA even though production correctly reports this workspace, yielding false regression failures; `crates/server/tests/build_identity_startup_log.rs:46`, `crates/server/build.rs:77`, `reviewer-evidence/foreign-git.log:12`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept container-free coupling as sufficient for this slice, or require an actual dist artifact/VERSION comparison before sign-off — Docker packaging was not exercised and the end-to-end observation is assigned to #779; `brief.md:205`, `deploy/docker/wyrd/Dockerfile:87`, `xtask/src/dist.rs:715`. |

All repository citations above and below refer to the supplied `$PDCA_TARGET` (`target/` under this review directory). All ten patched files match their `patch.diff` postimage blob hashes after the stash/restore experiment (`reviewer-evidence/target-integrity.log:1`). No production source was edited. The target has one synthetic base commit, so merged-history research used the repository’s read-only GitHub API, not another local checkout.

The actionable conclusions are grounded in three independent experiments:

1. **Shallow-history freshness requires a Plan decision.** Using unchanged copies of the actual build script and resolver in a minimal Cargo workspace, I made a real shallow Git clone with an already-fetched ancestor tag. `git fetch --unshallow --no-tags --refmap= origin HEAD` changed `git describe` from `2bdca0c` to `v1.2.3-1-g2bdca0c`, with the watched HEAD/refs mtimes unchanged. Cargo reported `Fresh` and retained `0.0.0+git.2bdca0c`; a fresh build produced `1.2.3+git.1.2bdca0c` (`reviewer-evidence/shallow-no-ref-update.log:26`). The commit itself remains correctly identified; the defect is inconsistent derived identity between incremental and fresh builds. An ordinary unshallow fetch happened to touch refs and passed (`reviewer-evidence/shallow.log:35`), so that control does not disprove the narrower failure. The C5 verdict is deliberately untagged: adding a shallow-file watch contradicts the explicit “exactly [HEAD/refs] and nothing wider” instruction in `brief.md:287`. Decide that contract before asking Do to change it. This does not reopen the settled prohibition on index/source watches or dirty-tree claims. Reproduction source: `reviewer-evidence/reproduce-shallow-no-ref-update.py:1`.

2. **The regression oracle can inspect the wrong repository.** Running the actual target integration test with `GIT_DIR` pointing at the separate fixture repository failed: it expected `a9bfb01` while the binary correctly logged `0.0.0+git.d2e4029` (`reviewer-evidence/foreign-git.log:12`). Production deliberately removes repository overrides and bounds discovery; the independent test does neither (`crates/server/build.rs:69`, `crates/server/tests/build_identity_startup_log.rs:44`). Keep the probe independent, but make it identify the workspace’s own repository and exclude enclosing/overridden repositories. A successful probe of that repository must still enforce provenance. This is the rebuild-addressable T5 defect.

3. **The override failure follows a contradictory test contract.** `WYRD_VERSION=1.2.3 cargo test --offline -p wyrd-server --test build_identity_startup_log` compiled successfully and the real binary logged `1.2.3`; the test failed because it demanded `d2e4029` (`reviewer-evidence/override.log:13`). Clearing the override restored green (`reviewer-evidence/restored-default.log:6`). Production follows rung 1 (`crates/server/src/version/derivation.rs:149`), while the test follows the brief’s unconditional successful-probe rule (`brief.md:48`). Decide whether the discriminator explicitly requires an override-free build, or whether the criterion should independently verify each permitted source. Do should not silently weaken the specified provenance assertion.

The frozen batch review’s nine entries collapse to four classes (`gate-logs/T4-batch-review.log:10`): the three override reports are covered by C1; the three Git-scope reports by T5; the shallow-history report by C5. The remaining two exact-tag reports are declined as a requirement expansion: the brief explicitly excludes exact tags from the SHA assertion (`brief.md:48`), and the unchanged normalizer test covers exact-tag normalization (`xtask/tests/dist_templates.rs:347`). No tracked-issue deferral was reopened. The capability-probe smell test does not fire: Git is intentionally optional for source archives/container contexts; the patch does not guard a capability that the guarded path necessarily possesses.

The gate evidence supports the following limits; no unavailable measurement is counted as passing:

| Gate | Adjudication and evidence |
|------|--------------------------|
| C4-verify | Independently reproduced: production stashed with the new discriminator retained, exit 101 on missing `version`; stash restored, exit 0 with one test passing. `reviewer-evidence/startup-red.log:279`, `reviewer-evidence/startup-restored-green.log:6`; agrees with `gate-logs/C4-verify.log:24`. |
| C4-ci | Local `cargo xtask ci` passed typos, docs lint/render, initial repository guards, fmt, workspace clippy/build/tests and cargo-machete, then stopped because `/home/eddie/.cargo/advisory-dbs/db.lock` is read-only (`reviewer-evidence/ci-rerun.log:3188`). This is a reviewer-host limitation. The frozen log records successful deny checks, conformance, statics/deployment guards and DST checks, ending in a full pass (`gate-logs/C4-ci.log:3395`, `gate-logs/C4-ci.log:4000`). Full CI was not independently completed. |
| C4-diff-cov | No coverage measurement: the wrapper reports that the patch did not apply to its `origin/main` base (`gate-logs/C4-diff-cov.log:10`). The supplied target compiles and its postimages match. This is a base-state caveat, not a demonstrated compile or patch defect. |
| C5-mutants | No mutants tested: the unmutated baseline failed because its scratch tree could not satisfy `git ls-files` (`gate-logs/C5-mutants.log:1953`, `gate-logs/C5-mutants.log:1967`). This is the brief’s declared harness limitation, not evidence of surviving mutants. |
| T4-batch-review | Frozen result remains FAIL; all four distinct finding classes are adjudicated above. The repeated entries are not nine independent defects (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A: deferred by design until the mandatory publish-time audit (`gate-logs/T4-contribution.log:10`). |
| host-tikv | Independently reran both exact clippy commands with `WYRD_TIKV_TOOLCHAIN=1`; both exited successfully using real compilation (`reviewer-evidence/host-tikv-rerun.log:1`). The frozen log also shows completion (`gate-logs/host-tikv.log:209`). |

The instance-scoped coverage/mutation/review wrappers are outside the supplied target; their captured logs were read instead of looking for wrappers in other checkouts. All configured gate logs were present, so none needs human escalation for missing evidence.

Prior art was checked by **all ten affected file paths**, across merged history and changed-file lists for **356 closed or merged PRs** (`reviewer-evidence/history-all-paths.json`, `reviewer-evidence/closed-pr-path-audit.json`). Two paginated file lists were completed separately: #618 (122 files) and #489 (102 files), recorded in `reviewer-evidence/pr-618-files.txt` and `reviewer-evidence/pr-489-files.txt`. The only closed-unmerged PR overlapping these paths was #647’s segmented-map work, unrelated to build identity. Existing dist derivation originates in `f5d4575d` / PR #572; no merged history exists for the newly added build script or identity files. No competing identity implementation or rejected identity proposal emerged; the brief already records #736 as this task’s superseded parent (`brief.md:261`).

The remaining external observation is the **real packaged artifact pair**. Pure argv/resolver tests and Dockerfile inspection establish coupling; they do not observe a Docker-built binary and its tarball. If sign-off requires that observation before #779, on a Docker/network-capable host run `cargo xtask dist`, unpack the newly generated `target/dist/wyrd-*.tar.gz`, launch its `bin/wyrd s3 --s3-listen 127.0.0.1:0 --data-dir <fresh-temp-directory> --access-key k --secret-key s --log-format json`, compare the startup event’s `fields.version` byte-for-byte with that same archive’s `VERSION` `version:` value, and stop the process. No image build was attempted here, as required by `brief.md:211`.

### Advisory — adversary

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

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/build.rs:120`: The rerun inputs omit Git’s `shallow` file. Deepening a shallow checkout can make an existing tag reachable without changing HEAD or refs, leaving the baked identity stale. Reproduced with this build script: `git describe` changed from a bare SHA to `v1.2.3-1-g<SHA>`, but a second Cargo build retained `0.0.0+git.<SHA>`. Track shallow-history changes and cover incremental rebuilds.

- NEEDS-HUMAN [impl] — `crates/server/tests/build_identity_startup_log.rs:181`: The unconditional SHA-containment assertion rejects a supported build override. On an untagged commit, `WYRD_VERSION=1.2.3 cargo test -p wyrd-server --test build_identity_startup_log` fails even though the binary correctly reports `1.2.3`. Assert the compile-time override when supplied, while retaining the independent Git assertion for ordinary builds.

- NEEDS-HUMAN [impl] — `crates/server/tests/build_identity_startup_log.rs:46`: The independent Git probe inherits repository overrides and searches enclosing repositories, unlike the build script. An unpacked source tree inside another repository correctly builds as `0.0.0+git.unknown`, but this probe finds the outer HEAD and fails the assertion at line 173; reproduced in a nested scratch fixture. Independently restrict discovery to the workspace’s own repository and clear repository-selection overrides.

No additional reuse, simplification, or efficiency findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether the SHA discriminator is restricted to builds without an override — the mandatory SHA assertion rejects a valid `WYRD_VERSION=1.2.3`, contradicting rung-1 precedence; `brief.md:48`, `brief.md:341`, `crates/server/tests/build_identity_startup_log.rs:181`, `reviewer-evidence/override.log:13`.
- [ ] C5 Causal adequacy — Decide whether to permit shallow-boundary invalidation or explicitly exclude it — real Git deepening changes the derived identity while Cargo retains the old value, but the binding plan restricts watches to HEAD/refs; `brief.md:287`, `crates/server/build.rs:119`, `reviewer-evidence/shallow-no-ref-update.log:35`.
- [ ] T5 Judgment — Correct the independent probe’s repository scope — inherited `GIT_DIR` makes the test demand a foreign SHA even though production correctly reports this workspace, yielding false regression failures; `crates/server/tests/build_identity_startup_log.rs:46`, `crates/server/build.rs:77`, `reviewer-evidence/foreign-git.log:12`.
- [ ] Validation — fitness-to-purpose — Accept container-free coupling as sufficient for this slice, or require an actual dist artifact/VERSION comparison before sign-off — Docker packaging was not exercised and the end-to-end observation is assigned to #779; `brief.md:205`, `deploy/docker/wyrd/Dockerfile:87`, `xtask/src/dist.rs:715`.
- [ ] `crates/server/build.rs:119-127` (`watch_paths`) leaves the baked identity **stale on a reftable repository**. Reftable is git's newer ref storage format (`git init --ref-format=reftable`; Git 3.0 plans to make it the default). In such a repo, commits and tags only update `<common_dir>/reftable/`: `HEAD` stays the stub `ref: refs/heads/.invalid`, `refs/` holds only a stub `heads` file, and there is no `packed-refs`. None of the three watched paths ever changes, so the build script never re-runs. I reproduced this with git 2.53, using the patch's own `build.rs` and `derivation.rs` in a probe crate nested two levels deep:
- [ ] `crates/server/src/version/derivation.rs:161`: rung 2 can still produce a **`.dirty` identity**, which the doc at `:138` says it never does ("this rung never produces `.dirty`"). The guard only checks for the `-dirty` suffix that `git describe --dirty` would add. A tag whose own name carries the marker passes straight through as `Rung::Describe`, with no warning. I compiled the shared file on its own and called `resolve(None, Some(d), Some("abc12de"), "0.0.0")`:
- [ ] `crates/server/tests/build_identity_startup_log.rs:162` and `:169`: **the SKIP reason is invisible.** The brief allows leg 2 to skip only when the probe fails, "and then the leg must print why". The test prints with `eprintln!`, and libtest captures and discards that output when a test passes. I checked with a one-line `#[test]` that calls `eprintln!`: the output is just `test t ... ok`. So wherever the probe can't see a repo, leg 2 is skipped and the run shows a plain `ok`. Examples: cargo-mutants' scratch copy (no `.git`), an unpacked source tree, or a container where git refuses the checkout over ownership. That is the "silently green with the only provenance check off" pattern the brief's v3 carry-forward warns about. This does not affect the C4-verify evidence: the gate tree is a git worktree and the probe succeeded there. Fix: write the reason with `writeln!(std::io::stderr(), …)`. I verified that this bypasses capture and shows on a passing run. Alternatively, fail unless an explicit opt-out env var is set.
- [ ] `deploy/docker/wyrd/Dockerfile:47` (`ARG WYRD_VERSION=""`): **compose-built `s3` gateways always log `version=0.0.0+git.unknown`.** `deploy/small-multi-node-fdb/docker-compose.yml:219-223` builds `wyrd` from this same Dockerfile, passes only `FEATURES`, and runs three `s3` gateways (`:389`, `:406`, `:423`). With no `WYRD_VERSION` and no `.git` in the build context, every gateway logs that value. Nothing warns about it: rung 3 emits no `cargo:warning`. The architecture doc lists docker-compose as a shipped artifact. The brief states its invariant over the whole build/packaging category ("violated equally by … a shipped binary whose build stage cannot see the repository"), but scope (c) only covers `dist`. The new sentence at `docs/design/architecture/07-deployment-view.md:42`, "Every `wyrd` names the checkout it was built from", overclaims for these builds. Human call: accept this as an out-of-scope follow-up (file an issue to pass `WYRD_VERSION: ${WYRD_VERSION:-}` through the compose `args`, and soften the doc sentence), or widen this slice.
- [ ] `crates/server/build.rs:120`: The rerun inputs omit Git’s `shallow` file. Deepening a shallow checkout can make an existing tag reachable without changing HEAD or refs, leaving the baked identity stale. Reproduced with this build script: `git describe` changed from a bare SHA to `v1.2.3-1-g<SHA>`, but a second Cargo build retained `0.0.0+git.<SHA>`. Track shallow-history changes and cover incremental rebuilds.
- [ ] `crates/server/tests/build_identity_startup_log.rs:181`: The unconditional SHA-containment assertion rejects a supported build override. On an untagged commit, `WYRD_VERSION=1.2.3 cargo test -p wyrd-server --test build_identity_startup_log` fails even though the binary correctly reports `1.2.3`. Assert the compile-time override when supplied, while retaining the independent Git assertion for ordinary builds.
- [ ] `crates/server/tests/build_identity_startup_log.rs:46`: The independent Git probe inherits repository overrides and searches enclosing repositories, unlike the build script. An unpacked source tree inside another repository correctly builds as `0.0.0+git.unknown`, but this probe finds the outer HEAD and fails the assertion at line 173; reproduced in a nested scratch fixture. Independently restrict discovery to the workspace’s own repository and clear repository-selection overrides.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — T5 Judgment — Correct the independent probe’s repository scope — inherited `GIT_DIR` makes the test demand a foreign SHA even though production correctly reports this workspace, yielding false regression failures; `crates/server/tests/build_identity_startup_log.rs:46`, `crates/server/build.rs:77`, `reviewer-evidence/foreign-git.log:12`.; `crates/server/build.rs:119-127` (`watch_paths`) leaves the baked identity **stale on a reftable repository**. Reftable is git's newer ref storage format (`git init --ref-format=reftable`; Git 3.0 plans to make it the default). In such a repo, commits and tags only update `<common_dir>/reftable/`: `HEAD` stays the stub `ref: refs/heads/.invalid`, `refs/` holds only a stub `heads` file, and there is no `packed-refs`. None of the three watched paths ever changes, so the build script never re-runs. I reproduced this with git 2.53, using the patch's own `build.rs` and `derivation.rs` in a probe crate nested two levels deep:; `crates/server/src/version/derivation.rs:161`: rung 2 can still produce a **`.dirty` identity**, which the doc at `:138` says it never does ("this rung never produces `.dirty`"). The guard only checks for the `-dirty` suffix that `git describe --dirty` would add. A tag whose own name carries the marker passes straight through as `Rung::Describe`, with no warning. I compiled the shared file on its own and called `resolve(None, Some(d), Some("abc12de"), "0.0.0")`:; `crates/server/tests/build_identity_startup_log.rs:162` and `:169`: **the SKIP reason is invisible.** The brief allows leg 2 to skip only when the probe fails, "and then the leg must print why". The test prints with `eprintln!`, and libtest captures and discards that output when a test passes. I checked with a one-line `#[test]` that calls `eprintln!`: the output is just `test t ... ok`. So wherever the probe can't see a repo, leg 2 is skipped and the run shows a plain `ok`. Examples: cargo-mutants' scratch copy (no `.git`), an unpacked source tree, or a container where git refuses the checkout over ownership. That is the "silently green with the only provenance check off" pattern the brief's v3 carry-forward warns about. This does not affect the C4-verify evidence: the gate tree is a git worktree and the probe succeeded there. Fix: write the reason with `writeln!(std::io::stderr(), …)`. I verified that this bypasses capture and shows on a passing run. Alternatively, fail unless an explicit opt-out env var is set.; `crates/server/build.rs:120`: The rerun inputs omit Git’s `shallow` file. Deepening a shallow checkout can make an existing tag reachable without changing HEAD or refs, leaving the baked identity stale. Reproduced with this build script: `git describe` changed from a bare SHA to `v1.2.3-1-g<SHA>`, but a second Cargo build retained `0.0.0+git.<SHA>`. Track shallow-history changes and cover incremental rebuilds.; `crates/server/tests/build_identity_startup_log.rs:181`: The unconditional SHA-containment assertion rejects a supported build override. On an untagged commit, `WYRD_VERSION=1.2.3 cargo test -p wyrd-server --test build_identity_startup_log` fails even though the binary correctly reports `1.2.3`. Assert the compile-time override when supplied, while retaining the independent Git assertion for ordinary builds.; `crates/server/tests/build_identity_startup_log.rs:46`: The independent Git probe inherits repository overrides and searches enclosing repositories, unlike the build script. An unpacked source tree inside another repository correctly builds as `0.0.0+git.unknown`, but this probe finds the outer HEAD and fails the assertion at line 173; reproduced in a nested scratch fixture. Independently restrict discovery to the workspace’s own repository and clear repository-selection overrides.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
