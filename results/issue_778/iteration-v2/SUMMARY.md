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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.70s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #778 embeds a shared build identity and logs it at S3 startup; the startup criterion passes, but one incremental-build defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit: local builds identify their commit, dist builds preserve the supplied artifact version, and the wire surface belongs to #779; `brief.md:24`, `brief.md:324`, `brief.md:339`. |
| C2 Reproduction (red pre-fix) | PASS | Keeping the independent test while stashing production compiled and failed specifically on the missing startup `version`, establishing the original observable defect; `reviewer-evidence/red-green.log:283`, `crates/server/tests/build_identity_startup_log.rs:205`. |
| C3 Change | PASS | The requested public identity, S3 field, packaging hand-off and living documentation are present within the declared scope; `crates/server/src/version.rs:38`, `crates/server/src/cli.rs:2393`, `xtask/src/dist.rs:767`, `docs/design/architecture/07-deployment-view.md:42`. |
| C4 Verification (red→green) | PASS | Independent red→green, full repository CI and both TiKV compilation checks passed; coverage and mutation adequacy remain unmeasured for the separately documented gate-environment reasons; `reviewer-evidence/red-green.log:325`, `reviewer-evidence/ci-rerun.log:3797`, `reviewer-evidence/host-tikv-rerun.log:205`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make full→shallow transitions refresh the identity — filtering absent watch inputs leaves an incremental build advertising an obsolete tag-derived value after tag reachability changes; independently reproduced at `crates/server/build.rs:43`, `reviewer-evidence/shallow-probe.log:44`. |
| T1 Structure | PASS | A shared pure module preserves tooling→product dependency direction without linking the product to xtask; `xtask/src/dist.rs:181`, `crates/server/build.rs:25`, `crates/server/src/version/derivation.rs:152`. |
| T2 Shape | PASS | Identity validation rejects unusable explicit input and falls back with warnings for unusable derived tags; new crate roots forbid unsafe code, and formatting/lint checks pass; `crates/server/src/version/derivation.rs:74`, `crates/server/src/version/derivation.rs:158`, `crates/server/build.rs:17`, `reviewer-evidence/ci-rerun.log:3797`. |
| T3 Runtime | PASS | The serving process only logs a compiled constant; Git probing stays at build time and the child-process test has a deadline and kill/reap guard; `crates/server/src/cli.rs:2393`, `crates/server/build.rs:48`, `crates/server/tests/build_identity_startup_log.rs:35`, `crates/server/tests/build_identity_startup_log.rs:160`. |
| T4 Contribution | FAIL | Required review disposition remains open for the single confirmed C5 defect, reported twice by the batch gate; affected-path prior-art checks completed, and the later contribution-artifact audit is N/A at Check; `gate-logs/T4-batch-review.log:10`, `reviewer-evidence/prior-art.log:116`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The actual binary supplies the discriminator's evidence without new production symbols, while existing packaging tests and resolver tests cover the declared green-only criteria; the incremental-invalidation coverage gap is recorded once under C5; `crates/server/tests/build_identity_startup_log.rs:19`, `xtask/tests/dist_templates.rs:336`, `xtask/tests/dist_templates.rs:411`, `crates/server/src/version/derivation.rs:286`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated identity is sufficient for operational diagnosis at this slice's scope — Docker-built binary/tarball equality was not exercised, so packaging evidence is coupling tests and source inspection pending #779's declared release smoke; `brief.md:201`, `xtask/src/dist.rs:767`, `deploy/docker/wyrd/Dockerfile:89`. |

The remaining implementation finding is **P2: Cargo can retain a stale identity when an initially complete clone becomes shallow**. At `crates/server/build.rs:43`, `.filter(|p| p.exists())` omits `.git/shallow` from the initial watch set. A later `git fetch --depth=1 origin main` can create that file without changing any watched HEAD/ref path. The resolver never runs, although `git describe` now has a different answer. The pure watch-set test at `crates/server/src/version/derivation.rs:477` checks candidates before this filtering step, so it does not cover that transition.

I reproduced this using real Git and Cargo with byte-for-byte copies of the patch's build script and shared derivation in a minimal crate. Starting one commit beyond `v1.2.3`, the first build printed `1.2.3+git.1.72b2cc7`. After the depth-one fetch, all watched HEAD/ref/packed-ref mtimes were unchanged and `git describe` printed `72b2cc7`. The incremental build still printed `1.2.3+git.1.72b2cc7`; a fresh build printed `0.0.0+git.72b2cc7`. See `reviewer-evidence/shallow-probe.log:34` and the runnable fixture in `reviewer-evidence/shallow_probe.py`. This is a build-input invalidation defect, not a request to restore dirty-tree tracking. Correct the invalidation and add an incremental full→shallow regression while retaining the brief's exclusion of index/source watches. Both frozen batch findings describe this same defect; neither is a separate finding or a recorded tracked-issue deferral.

The independent checks support the startup and coupling claims, with these limits:

| Gate | Adjudication | Evidence |
|------|--------------|----------|
| C4-verify | PASS | Stashed production with the added test retained: one compiled test failed on the missing field; restored production: the same test passed. `reviewer-evidence/red-green.log:283`, `reviewer-evidence/red-green.log:325`; consistent with `gate-logs/C4-verify.log:25`. |
| C4-ci | PASS | Independently ran `cargo xtask ci`: spelling, docs lint/render, structural scanners, fmt, clippy, build, tests, dependency checks and DST completed successfully. `reviewer-evidence/ci-rerun.log:3797`; frozen result at `gate-logs/C4-ci.log:4003`. |
| host-tikv | PASS | Independently reran both recorded feature-specific clippy commands with `WYRD_TIKV_TOOLCHAIN=1`; both exited zero. `reviewer-evidence/host-tikv-rerun.log:205`; frozen result at `gate-logs/host-tikv.log:209`. |
| C4-diff-cov | N/A | Coverage was not measured: the gate could not apply the patch to its `origin/main` base. This is a gate-base caveat, not evidence of a compilation defect. The provided target matches the patch and compiled successfully. `gate-logs/C4-diff-cov.log:10`, `reviewer-evidence/target-integrity.log:5`. |
| C5-mutants | N/A | No mutants were tested: the unmutated scratch baseline failed because `git ls-files` could not run successfully in that copy. This supplies no mutation-adequacy verdict and is not a patch regression. `gate-logs/C5-mutants.log:1962`, `gate-logs/C5-mutants.log:1976`. |
| T4-batch-review | FAIL | Its two reports collapse to the one independently reproduced shallow-boundary defect above. `gate-logs/T4-batch-review.log:10`, `reviewer-evidence/shallow-probe.log:59`. |
| T4-contribution | N/A | `pr-description.md` is absent by design at Check; the substantive contribution audit must rerun at publish. `gate-logs/T4-contribution.log:10`. |

All repository-source citations were checked against `$PDCA_TARGET` (`target/`); evidence citations refer to the supplied logs or this review's saved outputs. The disposable target is consistent with `patch.diff`, including every added file, and the stash was fully restored (`reviewer-evidence/target-integrity.log:1`). Instance-scoped wrappers are absent from this target as expected; their frozen logs were read. No supplied gate log was missing. The capability-probe smell test does not add a finding: Git is explicitly optional in the build-identity contract, and the new runtime path logs a constant rather than guarding an assumed capability.

Prior art was checked by **all ten affected paths**, using GitHub merged commit histories and the file lists of all 356 closed PRs, with pagination completed for the two lists initially truncated at 100 files. Packaging #572 and telemetry #531 are relevant precedents. The only closed-unmerged overlap was #647's unrelated segmented-map change in `crates/server/src/lib.rs`; no closed PR touched the new build-script, version-module or discriminator paths (`reviewer-evidence/prior-art.log:51`, `reviewer-evidence/prior-art.log:110`). No duplicate or rejected identity implementation was found in that check.

The fitness decision does not require inventing a missing PR-time container gate: the brief expressly defers that observation. If the human elects to observe it before #779's release smoke, run `cargo xtask dist` on a Docker/network-capable host, unpack the resulting `target/dist/wyrd-*.tar.gz` into a disposable directory, and run its `bin/wyrd s3 --s3-listen 127.0.0.1:0 --data-dir <disposable-data-dir> --access-key k --secret-key s --log-format json`. Compare the startup event's `fields.version` byte-for-byte with that same tarball's `VERSION` file's `version:` value, then stop the child. This artifact-pair check was not run here, as instructed by `brief.md:201`.

### Advisory — adversary

# Adversarial review — issue 778 (build identity derived and logged)

Re-ran the proof in a scratch copy of `$PDCA_TARGET` (cargo 1.96.0, git 2.53.0) and attacked the
build script with a small probe crate that compiles the patch's own `crates/server/build.rs` and
`crates/server/src/version/derivation.rs` unchanged. The red→green holds. I found one concrete
defect in the build script, an adjudication for the gating T4 block, and a stale-base problem.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/build.rs:52` (with `:29`): **once a build lands on rung 3 it stays there.** When no repository is found, the script emits only `rerun-if-env-changed=WYRD_VERSION`, so nothing triggers a re-probe later. Repro with the probe crate: built with no `.git` → `0.0.0+git.unknown`; then `git init && git commit` → rebuild → still `0.0.0+git.unknown` while `git describe` prints `43076f1`. The same happens if `git` was missing from `PATH` on the first build, or if git refused the checkout ("dubious ownership") and the user later fixed `safe.directory`. On such a machine the binary keeps a wrong identity, and the patch's own test fails at `crates/server/tests/build_identity_startup_log.rs:245` ("the identity did not come from the repository") until someone runs `cargo clean -p wyrd-server`. Possible fix: on rung 3, emit a trigger that makes cargo re-probe, e.g. `rerun-if-changed=<root>/.git` (a missing path makes cargo re-run the script on every build — harmless in the one-shot image build, and only paid by builds that already warn). At minimum, put the `cargo clean -p wyrd-server` remedy in the rung-3 warning.

- NEEDS-HUMAN [human] — `crates/server/build.rs:43` / `crates/server/src/version/derivation.rs:256`: **the gating T4 block (2 findings, same issue) is real but much narrower than stated, and its obvious fix conflicts with Decision 1.** Reproduced: on a full clone 3 commits past `v0.1.0`, a plain `git fetch --depth=1 origin` *did* refresh the identity (the fetch touched `.git/refs`, which is watched). Only a fetch that updates no ref — `git fetch --depth=1 origin <sha>` — left the baked `0.1.0+git.3.67ea88c` while `git describe` now printed `67ea88c`. The stale value still names the same commit (the tag is still an ancestor; the shallow boundary only hides it), so under Decision 1 ("the identity names the commit") it is out of date in wording, not wrong about the commit. The only cargo-level fix is to always emit the missing `shallow` path, which makes cargo re-run the script — and recompile `wyrd-server` plus its integration-test binaries — on **every** build in every full clone. That is exactly the relink cost Decision 1 rules out. My recommendation: record T4's two findings as rejected, with this reason. It is a human call because T4 is a gating row.

- NEEDS-HUMAN [impl] — `gate-logs/C4-diff-cov.log:10`: **the bundle does not apply on `origin/main`** ("the bundle is stale; rebase Do"). This is the second round in a row. C4-ci and C4-verify ran against the pre-fix base `df68932f` (`f492a28` in the target), so the green evidence is for that base, not for the merge result. The declared conflicts (#738 `--chunk-size` in `cmd_s3`, #742 two-binary Dockerfile and dist) are already in the base, so something newer has moved. Rebase and re-run the gates.

## Attempted and could not refute

- **Red→green is genuine and runs the production path.** I re-ran both legs myself. Red (production reverted, test kept) fails at `crates/server/tests/build_identity_startup_log.rs:205` because the field is missing; the logged event matches `gate-logs/C4-verify.log`. Green: the real binary logs `"version":"0.0.0+git.f492a28"`, which equals `git rev-parse --short HEAD`. The test names no new symbol. It spawns `CARGO_BIN_EXE_wyrd` and kills it on every exit path (`RoleGuard`; the temp dir is dropped after the guard). Its sha expectation comes from its own `git` calls, so a constant cannot pass leg 2.
- **The skip is now visible, and it did not fire in the gate.** `gate-logs/C5-mutants.log:1134` shows the `SKIP provenance leg` note on a passing run (cargo-mutants' copy has no `.git`), so the previous round's "invisible skip" finding is fixed. `gate-logs/C4-ci.log` shows no SKIP note, so leg 2 ran there.
- **Scoping to the workspace's own repo works** (`crates/server/build.rs:81-127`). A probe workspace nested in an outer repo baked `0.0.0+git.unknown`, not the outer sha, with and without `GIT_DIR=<outer>/.git`.
- **The watch set refreshes when it should, and does not fire on edits.** With a reftable repo (`git init --ref-format=reftable`), tag + commit refreshed to `2.0.0+git.1.<sha>`. Deleting a loose tag refreshed to `0.0.0+git.<sha>` (cargo's directory scan sees the deletion). Editing a file, then `git status` and `git add`, did not re-run the script (`Fresh probe`), so Decision 1 holds for the edit→test loop.
- **The dirty guard, the validator, rung 1 used verbatim, the Dockerfile `ARG` scope (`deploy/docker/wyrd/Dockerfile:49`, `:89`, before `RUN cargo build`), and the pure `image_build_args`** — I found no input that breaks them. I tried `.DIRTY`/`Dirty` tags, non-`v` tags such as `archive/backup-premerge-signoff` (validation fails, so it falls back to the sha form), leading `-`/`.`/`+`, and over-long values.

## Weak spots in the evidence (advisory, not defects today)

- `crates/server/build.rs:24` + `xtask/tests/dist_templates.rs:390`: in the gate's tagless repo, leg 2 only proves "the identity contains HEAD's short sha". A `build.rs` that skipped `derivation::resolve` and printed `0.0.0+git.<rev-parse --short>` would pass every gate: the `#[allow(dead_code)]` on the included module hides the unused code, and the "one file, two consumers" test only greps for the `#[path]` string. Likewise, no test exercises the scoping in `build.rs:81-127`. I checked by hand that it works, but deleting the ceiling or the toplevel check would stay green. The brief accepted green-only legs 3-5, so I am not raising this as a defect.
- `crates/server/src/version/derivation.rs:253`: watching `<common_dir>/refs` recursively means a remote-tracking ref update re-runs the script and recompiles `wyrd-server`, even though `git describe --tags` of `HEAD` cannot change. Verified: `git update-ref refs/remotes/origin/other HEAD` → `Dirty probe … the file .git/refs has changed`. Any `git fetch` that moves `origin/*` does this, including an IDE's background fetch. Watching `refs/heads` + `refs/tags` (plus `packed-refs`/`reftable`) would avoid it. This is within the brief's "HEAD/refs" wording, so it is a tuning suggestion only.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/build.rs:43`: Filtering out an absent `shallow` file leaves a full checkout unable to detect a newly introduced shallow boundary. Reproduced with the target's unchanged build script and resolver in a small Cargo fixture: after `git fetch --depth=1 --no-tags origin HEAD`, every watched path retained its mtime, while `git describe --tags --always` changed from `v1.2.3-1-g7e56f1b` to `7e56f1b`. The next incremental build still baked `1.2.3+git.1.7e56f1b` instead of `0.0.0+git.7e56f1b`. Detect creation of the shallow boundary as well as changes/deletion, and add an incremental-build regression covering this transition; the pure watch-list assertions do not exercise the existence filter. This confirms the duplicated finding in the frozen T4 log as one defect.

No additional actionable reuse, simplification, or efficiency findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Make full→shallow transitions refresh the identity — filtering absent watch inputs leaves an incremental build advertising an obsolete tag-derived value after tag reachability changes; independently reproduced at `crates/server/build.rs:43`, `reviewer-evidence/shallow-probe.log:44`.
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated identity is sufficient for operational diagnosis at this slice's scope — Docker-built binary/tarball equality was not exercised, so packaging evidence is coupling tests and source inspection pending #779's declared release smoke; `brief.md:201`, `xtask/src/dist.rs:767`, `deploy/docker/wyrd/Dockerfile:89`.
- [ ] `crates/server/build.rs:52` (with `:29`): **once a build lands on rung 3 it stays there.** When no repository is found, the script emits only `rerun-if-env-changed=WYRD_VERSION`, so nothing triggers a re-probe later. Repro with the probe crate: built with no `.git` → `0.0.0+git.unknown`; then `git init && git commit` → rebuild → still `0.0.0+git.unknown` while `git describe` prints `43076f1`. The same happens if `git` was missing from `PATH` on the first build, or if git refused the checkout ("dubious ownership") and the user later fixed `safe.directory`. On such a machine the binary keeps a wrong identity, and the patch's own test fails at `crates/server/tests/build_identity_startup_log.rs:245` ("the identity did not come from the repository") until someone runs `cargo clean -p wyrd-server`. Possible fix: on rung 3, emit a trigger that makes cargo re-probe, e.g. `rerun-if-changed=<root>/.git` (a missing path makes cargo re-run the script on every build — harmless in the one-shot image build, and only paid by builds that already warn). At minimum, put the `cargo clean -p wyrd-server` remedy in the rung-3 warning.
- [ ] `crates/server/build.rs:43` / `crates/server/src/version/derivation.rs:256`: **the gating T4 block (2 findings, same issue) is real but much narrower than stated, and its obvious fix conflicts with Decision 1.** Reproduced: on a full clone 3 commits past `v0.1.0`, a plain `git fetch --depth=1 origin` *did* refresh the identity (the fetch touched `.git/refs`, which is watched). Only a fetch that updates no ref — `git fetch --depth=1 origin <sha>` — left the baked `0.1.0+git.3.67ea88c` while `git describe` now printed `67ea88c`. The stale value still names the same commit (the tag is still an ancestor; the shallow boundary only hides it), so under Decision 1 ("the identity names the commit") it is out of date in wording, not wrong about the commit. The only cargo-level fix is to always emit the missing `shallow` path, which makes cargo re-run the script — and recompile `wyrd-server` plus its integration-test binaries — on **every** build in every full clone. That is exactly the relink cost Decision 1 rules out. My recommendation: record T4's two findings as rejected, with this reason. It is a human call because T4 is a gating row.
- [ ] `gate-logs/C4-diff-cov.log:10`: **the bundle does not apply on `origin/main`** ("the bundle is stale; rebase Do"). This is the second round in a row. C4-ci and C4-verify ran against the pre-fix base `df68932f` (`f492a28` in the target), so the green evidence is for that base, not for the merge result. The declared conflicts (#738 `--chunk-size` in `cmd_s3`, #742 two-binary Dockerfile and dist) are already in the base, so something newer has moved. Rebase and re-run the gates.
- [ ] `crates/server/build.rs:43`: Filtering out an absent `shallow` file leaves a full checkout unable to detect a newly introduced shallow boundary. Reproduced with the target's unchanged build script and resolver in a small Cargo fixture: after `git fetch --depth=1 --no-tags origin HEAD`, every watched path retained its mtime, while `git describe --tags --always` changed from `v1.2.3-1-g7e56f1b` to `7e56f1b`. The next incremental build still baked `1.2.3+git.1.7e56f1b` instead of `0.0.0+git.7e56f1b`. Detect creation of the shallow boundary as well as changes/deletion, and add an incremental-build regression covering this transition; the pure watch-list assertions do not exercise the existence filter. This confirms the duplicated finding in the frozen T4 log as one defect.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- [ ] C1 Spec — Decide whether the SHA discriminator is restricted to builds without an override — the mandatory SHA assertion rejects a valid `WYRD_VERSION=1.2.3`, contradicting rung-1 precedence; `brief.md:48`, `brief.md:341`, `crates/server/tests/build_identity_startup_log.rs:181`, `reviewer-evidence/override.log:13`.
- [ ] C5 Causal adequacy — Decide whether to permit shallow-boundary invalidation or explicitly exclude it — real Git deepening changes the derived identity while Cargo retains the old value, but the binding plan restricts watches to HEAD/refs; `brief.md:287`, `crates/server/build.rs:119`, `reviewer-evidence/shallow-no-ref-update.log:35`.
- [ ] `deploy/docker/wyrd/Dockerfile:47` (`ARG WYRD_VERSION=""`): **compose-built `s3` gateways always log `version=0.0.0+git.unknown`.** `deploy/small-multi-node-fdb/docker-compose.yml:219-223` builds `wyrd` from this same Dockerfile, passes only `FEATURES`, and runs three `s3` gateways (`:389`, `:406`, `:423`). With no `WYRD_VERSION` and no `.git` in the build context, every gateway logs that value. Nothing warns about it: rung 3 emits no `cargo:warning`. The architecture doc lists docker-compose as a shipped artifact. The brief states its invariant over the whole build/packaging category ("violated equally by … a shipped binary whose build stage cannot see the repository"), but scope (c) only covers `dist`. The new sentence at `docs/design/architecture/07-deployment-view.md:42`, "Every `wyrd` names the checkout it was built from", overclaims for these builds. Human call: accept this as an out-of-scope follow-up (file an issue to pass `WYRD_VERSION: ${WYRD_VERSION:-}` through the compose `args`, and soften the doc sentence), or widen this slice.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Make full→shallow transitions refresh the identity — filtering absent watch inputs leaves an incremental build advertising an obsolete tag-derived value after tag reachability changes; independently reproduced at `crates/server/build.rs:43`, `reviewer-evidence/shallow-probe.log:44`.; `crates/server/build.rs:52` (with `:29`): **once a build lands on rung 3 it stays there.** When no repository is found, the script emits only `rerun-if-env-changed=WYRD_VERSION`, so nothing triggers a re-probe later. Repro with the probe crate: built with no `.git` → `0.0.0+git.unknown`; then `git init && git commit` → rebuild → still `0.0.0+git.unknown` while `git describe` prints `43076f1`. The same happens if `git` was missing from `PATH` on the first build, or if git refused the checkout ("dubious ownership") and the user later fixed `safe.directory`. On such a machine the binary keeps a wrong identity, and the patch's own test fails at `crates/server/tests/build_identity_startup_log.rs:245` ("the identity did not come from the repository") until someone runs `cargo clean -p wyrd-server`. Possible fix: on rung 3, emit a trigger that makes cargo re-probe, e.g. `rerun-if-changed=<root>/.git` (a missing path makes cargo re-run the script on every build — harmless in the one-shot image build, and only paid by builds that already warn). At minimum, put the `cargo clean -p wyrd-server` remedy in the rung-3 warning.; `gate-logs/C4-diff-cov.log:10`: **the bundle does not apply on `origin/main`** ("the bundle is stale; rebase Do"). This is the second round in a row. C4-ci and C4-verify ran against the pre-fix base `df68932f` (`f492a28` in the target), so the green evidence is for that base, not for the merge result. The declared conflicts (#738 `--chunk-size` in `cmd_s3`, #742 two-binary Dockerfile and dist) are already in the base, so something newer has moved. Rebase and re-run the gates.; `crates/server/build.rs:43`: Filtering out an absent `shallow` file leaves a full checkout unable to detect a newly introduced shallow boundary. Reproduced with the target's unchanged build script and resolver in a small Cargo fixture: after `git fetch --depth=1 --no-tags origin HEAD`, every watched path retained its mtime, while `git describe --tags --always` changed from `v1.2.3-1-g7e56f1b` to `7e56f1b`. The next incremental build still baked `1.2.3+git.1.7e56f1b` instead of `0.0.0+git.7e56f1b`. Detect creation of the shallow boundary as well as changes/deletion, and add an incremental-build regression covering this transition; the pure watch-list assertions do not exercise the existence filter. This confirms the duplicated finding in the frozen T4 log as one defect.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
