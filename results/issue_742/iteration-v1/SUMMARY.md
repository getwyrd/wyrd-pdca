# Result — issue 742 / dist-ship-wyrd-validate-two-binary-tarball

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) The distribution pipeline is single-binary end to end, in
  five places, verified on `main` at `65ca4fd`: the Dockerfile builds `--bin wyrd` (`:66`),
  calls its runtime stage "a minimal image hosting just the `wyrd` binary" (`:68`) and
  copies one path (`:122`); `dist::obtain_binary` extracts that one path
  (`IMAGE_BINARY_PATH = "/usr/local/bin/wyrd"`, `xtask/src/dist.rs:40`) and `assemble`
  copies it to one destination (`:559-564`); `deploy/dist/install.sh` installs
  `$HERE/bin/wyrd` and removes `$BINDIR/wyrd`; `deploy/dist/README.md` describes one
  binary; and `.github/workflows/release.yml` smoke-tests `/usr/local/bin/wyrd` (`:75`) and
  asserts its absence after uninstall (`:85`). So a validation tool an operator is meant to
  run against their own deployment has no way to reach them.
- Success criterion: BINDING (demonstrable by C4-verify at Check, container-free, inside
  `cargo xtask ci`): the shipped-binary set becomes DATA — one pure function in
  `xtask::dist` mapping each binary's in-image path to its tarball destination — the Rust
  side of the pipeline READS that table, and every other stage's spelling of the binary set
  is PINNED to it by an assertion. A new test asserts, by reading the real repo files, that
  every one of the five places above names **both** binaries:
  1. the Dockerfile builds each binary and copies each into the runtime stage;
  2. `install.sh` installs each from `bin/<name>` and removes each on `--uninstall`;
  3. the release workflow's smoke step exercises each installed path and asserts each is
     gone after uninstall;
  4. `README.md` distinguishes the two roles of the two binaries;
  5. those four assertions are **one checker over a binary set, not written out per binary**,
     and that checker runs over **both** sets: the red file's local expected set AND the
     production table. Mind the split the gate forces (see Falsifiability): the red-earning
     file may not name a net-new symbol, so it holds the checker as a plain function of a
     binary set (returning the list of files that disagree) and calls it on its OWN local
     expected set. The existing `dist_templates.rs` then runs **that same checker** on
     `xtask::dist`'s production table — include the red file as a module
     (`#[path = "dist_two_binary_layout.rs"] mod layout;`), so the checker exists once; do NOT
     put it in a new helper file under `xtask/tests/`, because the C4 classifier treats every
     added `tests/*.rs` as a discriminator test (`engine/scripts/run-verify.sh:144`). Keep the
     red file module-includable (no crate-only inner attributes). `dist_templates.rs` also
     pins the local set EQUAL to the production table. The diagnostic this buys, stated
     exactly: add a third entry to the production table and change nothing else, and
     `dist_templates.rs` fails naming **every pipeline file that lacks the new binary**, while
     the equality assertion fails telling you to update the red file's local set. Two files,
     one checker, one declaration — the table stays the source and the text test still
     compiles against a reverted tree. This is the honest version of
     "single source": Docker, shell, YAML and Markdown cannot read a Rust function, so what
     the slice buys is *declared once, duplication checked by the gate* — the same shape the
     repo already uses for the FDB pin (`xtask/tests/fdb_image.rs` pins one `ARG FDB_VERSION`
     across three files). Adding a third binary stays a multi-file edit; what changes is that
     the gate names every file you missed instead of the release doing it.
  6. AND one observation beyond file text, so the criterion is not purely lexical: the
     binary-staging step is extracted as a **`pub`** callable that takes the table plus a
     source directory and populates a staging tree (today it is four hard-coded lines inside
     the private `assemble`, `xtask/src/dist.rs:559-564`, unreachable from any integration
     test). A test runs it over a tempdir holding two dummy "binaries" with **different
     contents** (e.g. `roles-binary` and `validator-binary`) and asserts the staging tree ends
     up with `bin/wyrd` and `bin/wyrd-validate`, both `0755`, and that **each destination is
     byte-for-byte equal to its own source** — so copying one source to both destinations, or
     swapping them, fails. The mapping that feeds staging is checked too: where `obtain_binary`
     extracts each in-image path on the packaging host is computed by a pure function of the
     table, and a test asserts those host paths are pairwise distinct (no two entries extract
     onto one file), as are the table's in-image paths and its tarball destinations. That is
     the real staging code, exercised, with no container. These assertions name new API, so
     they live in `dist_templates.rs`, not in the red-earning file.
  DEFERRED and named as such (see Verification posture): "a real tarball contains both
  binaries, and `install.sh` places both on a real host" — the release workflow's to prove.
  Not a choice: nothing in `cargo xtask ci` can build a tarball (that needs Docker and a
  network, `xtask/src/dist.rs:26-28`), and `install.sh` cannot be executed by a test at all —
  it refuses to run as non-root (`deploy/dist/install.sh:90`), writes `/etc/wyrd`, creates
  users and installs units. Verified by reading it, not assumed.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) the binary set becomes a pure data table in `xtask::dist` — in-image path →
  tarball destination — read by `obtain_binary` and `assemble`; (b) the Dockerfile builds
  and copies both binaries; (c) `dist` extracts both out of the image and stages both under
  `bin/`; (d) `install.sh` installs and uninstalls both; (e) `deploy/dist/README.md`
  distinguishes the roles binary from the validation tool; (f) `release.yml`'s smoke step
  covers both; (g) the container-free layout contract — the new red-earning text test (its
  assertions ITERATED from the table, per criterion 5) plus the pure-function and
  binary-staging assertions in `dist_templates.rs` (criterion 6); (h) the `--host` branch
  builds BOTH binaries — `cargo build --release --locked --bin wyrd --bin wyrd-validate
  --features …` — and returns both paths, so a `--host` tarball has the same contents as an
  image-built one. It does NOT refuse: removing the local-build path
  (`xtask/src/dist.rs:412-430`) is not this slice's to do, and a `--host` tarball silently
  missing the validator is the very bug class being removed. Assert it as a pure function:
  the cargo argv for the host branch is built from the table and named-both-bins is a
  container-free assertion like the rest.
  **/ out of scope:** making `wyrd-validate` a `wyrd` subcommand (it would share the roles'
  dependency closure and make #740's blackbox lint meaningless — the issue says so, and
  proposal 0017 §2 and §9 say so twice); a systemd unit for the validator (it is not a
  role — it is run by hand or by a launcher, and `install.sh`'s `ROLES` list must NOT grow
  an entry); a `/etc/wyrd/validate.env`; registry publication of the image (a named
  follow-up slice — the release still ships the OCI archive as a signed blob, not a `ghcr`
  push); multi-arch; the `tikv` flavor; changing what `wyrd-validate` DOES.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (2 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 74 instrumentable changed lines executed (below the 80% floor); 74 of 137 changed lines were i
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.97s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #742: ship `wyrd-validate` beside `wyrd` in the OCI image and operator tarball, with installation, removal, and a container-free layout contract.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding contract is measurable and distinguishes packaging structure from deferred real installation; the image choice and early-release risk are explicit sign-off decisions (brief.md:45, brief.md:218, brief.md:472). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new test produced two assertion failures naming the missing validator across the pipeline, with successful compilation (xtask/tests/dist_two_binary_layout.rs:247; pdca-reviewer-742-evidence/red.log:37). |
| C3 Change | PASS | The eight changed files cover the authorized packaging surfaces without expanding the validator into a service; the living deployment description stays consistent (xtask/src/dist.rs:287; deploy/dist/install.sh:143; docs/design/architecture/07-deployment-view.md:42). |
| C4 Verification (red→green) | PASS | Restoring the fix produced 24 passing layout/staging tests and a passing `dist --check`; the complete CI pass is frozen evidence, while the local CI rerun stopped at a read-only advisory-database lock, not a patch failure (pdca-reviewer-742-evidence/restored-green.log:30; pdca-reviewer-742-evidence/dist-check.log:2; gate-logs/C4-ci.log:3964; pdca-reviewer-742-evidence/ci.log:3350). |
| C5 Causal adequacy | PASS | For the binding contract, drift, colliding paths, swapped contents, wrong modes, and missing sources have concrete assertions; no added capability probe masks an eager cause. Full packaging execution remains the T3 decision (xtask/tests/dist_templates.rs:477, xtask/tests/dist_templates.rs:539, xtask/tests/dist_templates.rs:604). |
| T1 Structure | PASS | The existing packaging layer owns the declaration and its Rust consumers; one shared checker joins the independent red test to the production table without adding dependencies or crossing runtime trait seams (xtask/src/dist.rs:306; xtask/src/dist.rs:505; xtask/src/dist.rs:656; xtask/tests/dist_templates.rs:17). |
| T2 Shape | PASS | Both binaries have distinct image, extraction, and staging paths; the validator stays outside the role/unit list, and release triggers retain their existing tag/manual coverage (xtask/tests/dist_templates.rs:539; deploy/dist/install.sh:140; .github/workflows/release.yml:21). |
| T3 Runtime | NEEDS-HUMAN | Accept deferring real image build, extraction, tarball assembly, and privileged install/uninstall to the release workflow, or require that run before sign-off — those dependencies were not exercised here; evidence is file contracts plus real staging of dummy bytes (brief.md:237; .github/workflows/release.yml:54; pdca-reviewer-742-evidence/coverage-summary.txt:3). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent and their substantive audit must rerun at publish; the supplied batch-review log and independent affected-path prior-art check are recorded below (gate-logs/T4-contribution.log:10; gate-logs/T4-batch-review.log:10; pdca-reviewer-742-evidence/prior-art.json:141). |
| T5 Judgment | NEEDS-HUMAN | Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated container-free contract is sufficient to accept this operator-delivery slice — actual two-binary release contents and host installation remain unobserved, so the literal operator outcome is not yet established (brief.md:255; .github/workflows/release.yml:60). |

No implementation defect was established. The binding red→green contract is independently reproduced; the remaining decisions concern release evidence and the explicitly planned sequencing, not a requested rebuild.

All source citations above are grounded in the supplied `$PDCA_TARGET`. Its self-contained base plus applied patch was readable. After the stash/pop experiment, the tracked diff matched its original bytes, `git diff --check` passed, and `git apply --reverse --check patch.diff` passed. No production fix was edited.

The evidence supports the contract within its stated limits:

- **Independent behavior:** `cargo test --offline --locked -p xtask --test dist_two_binary_layout` failed two tests with the tracked fix stashed. After restoration, that suite plus `dist_templates` passed all 24 tests, including distinct contents, mode 0755, missing-source refusal, table equality, collision checks, host argv, and third-binary diagnostics. Logs: `pdca-reviewer-742-evidence/red.log:1` and `pdca-reviewer-742-evidence/restored-green.log:1`.
- **CI and validators:** the local `cargo xtask ci` passed typos, documentation lint/render, repository guards, formatting, clippy, workspace build/tests, and cargo-machete. Cargo-deny then could not obtain an exclusive lock on its read-only advisory database (`pdca-reviewer-742-evidence/ci.log:3350`). This host limitation does not establish a patch defect. The frozen run includes the successful deny, conformance, statics, deployment, and DST checks (`gate-logs/C4-ci.log:3360`, `gate-logs/C4-ci.log:3964`). `cargo xtask dist --check` and installer shell syntax also passed independently. The frozen TiKV gate shows successful checking of both requested feature selections (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`); it does not prove an image build.
- **Coverage:** the frozen advisory C4-diff-cov result remains **FAIL: 0/74 changed instrumentable lines**. Its command selected only the deliberately text-only new test (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:109`). Independently measuring both intended suites executed `shipped_binaries` seven times, `binary_source_path` eight, `host_build_args` once, and `stage_binaries` twice; `obtain_binaries`, `extract_binaries`, and `assemble` remained unexecuted (`pdca-reviewer-742-evidence/coverage-summary.txt:1`). This demonstrates the production helper coverage omitted by the frozen selection, without claiming the 80% diff gate passed.
- **Mutation testing:** the frozen failure was an unmutated baseline failure at `git ls-files`, with no mutants tested (`gate-logs/C5-mutants.log:443`). An independent copied-tree run with `--copy-vcs=true` passed that baseline. A further run with `--cap-lints=true` avoided denied-unused-variable artifacts: six mutations were caught, one was unviable, and five survived in `obtain_binaries`, `extract_binaries`, `assemble`, and `run_dist` (`pdca-reviewer-742-evidence/mutants-capped.log:4`). These survivors demonstrate the declared gap in packaging orchestration; they do not demonstrate incorrect current behavior or replace the T3 sign-off decision. The source tree was not mutated by these copied-tree runs.

The contribution and prior-art evidence is accounted for. The frozen batch log reports zero blocking findings; it provides no individual review reasoning to independently affirm (`gate-logs/T4-batch-review.log:10`). The contribution-artifact row is **N/A**, because its publish-time audit is explicitly deferred (`gate-logs/T4-contribution.log:10`). An independent GitHub read checked merged commit history for all eight affected paths and the file lists of all 19 closed, unmerged PRs; none of those PRs touched an affected path, and no query reached its pagination limit (`pdca-reviewer-742-evidence/prior-art.json:1`). The path history includes the original #570 pipeline and later installer/template changes, with no earlier two-binary layout test.

The real release remains the required external observation. The locally built validator was exercised without arguments: it printed `usage: wyrd-validate` and exited 2; `ldd` showed no `libfdb_c` dependency (`pdca-reviewer-742-evidence/validator-usage.log:2`, `pdca-reviewer-742-evidence/validator-linkage.log:2`). These observations do not establish bookworm image compatibility or privileged installation. To discharge that gap, the maintainer can dispatch `release.yml` against a branch containing this patch using `gh workflow run release.yml --repo getwyrd/wyrd --ref <branch>`, then inspect the build and installer-smoke steps. Require both tarball binaries, validator usage before the FDB client is installed, successful repeat installation, and absence of both binaries after uninstall (`.github/workflows/release.yml:54`, `.github/workflows/release.yml:72`, `.github/workflows/release.yml:86`). A branch dispatch exercises the pipeline without satisfying the tag-only publication condition; it does not substitute for #743 or the endurance verdict.

### Advisory — adversary

# Adversarial review — #742 (ship `wyrd-validate` in the two-binary tarball)

**Bottom line: I could not refute the fix.** The red→green is real and runs over the real
pipeline files, and the five stages are consistent with each other. What I found: two weak
spots in the new tests (both cheap builder fixes), one claim in the brief that overstates
what Check exercises, and one existing CI gap that makes the stated invariant narrower than
it reads.

## What I tried and could not break

- **Red→green evidence** (`gate-logs/C4-verify.log`). The red leg fails with nine specific
  disagreements across all five places (Dockerfile build and copy, install.sh install and
  uninstall, release smoke run and absence check, README rows and install path). The test
  reads the real repo files (`xtask/tests/dist_two_binary_layout.rs:27-30`). It is not a
  tautology, not a mirror of production, and it does not fail for an unrelated reason.
- **Partial reverts** (scratch copy, `cargo test -p xtask --test dist_templates --test
  dist_two_binary_layout`). Removing only the uninstall `rm -f`
  (`deploy/dist/install.sh:118`) or only `test ! -e /usr/local/bin/wyrd-validate`
  (`.github/workflows/release.yml:93`) turns both checker runs red with the right message.
- **The diagnostic the brief promised.** I added a third entry to `shipped_binaries()`
  (`xtask/src/dist.rs:287`) and changed nothing else.
  `every_pipeline_stage_ships_the_production_binary_set` named all 8 missing spellings
  across the 4 files, and the equality test said to update `EXPECTED_BINARIES`.
- **Mutations of `binary_source_path`** (`xtask/src/dist.rs:306`): `join(tarball_dest)` and
  an added suffix. Both were killed by `stage_binaries_copies_each_binary_to_its_own_destination`,
  because it hard-codes the source path in its `remove_file` step
  (`xtask/tests/dist_templates.rs:647`). Swapped or duplicated sources are killed by the
  byte-for-byte check.
- **Cargo accepts the two-bin build.** `cargo check --release --locked --bin wyrd --bin
  wyrd-validate --features fdb,etcd` exits 0 on this tree. So `deploy/docker/wyrd/Dockerfile:72`
  and the `host_build_args` argv (`xtask/src/dist.rs:312`) do not fail on
  features that `wyrd-validate` lacks.
- **The smoke step matches the real binary.** With no arguments, `wyrd-validate` returns
  `ArgError::Missing`, prints `usage: wyrd-validate …` to stderr (`crates/validate/src/args.rs:184`)
  and exits 2, which is what `.github/workflows/release.yml:74-77` expects.
  `cargo tree -p wyrd-validate -e normal` shows no openssl, aws-lc or ring. So running it
  before libfdb_c is installed is sound, and it adds no runtime library the bookworm smoke
  container lacks.

## Findings

- NEEDS-HUMAN [impl] — **The release-workflow checker ignores comments and order.**
  `xtask/tests/dist_two_binary_layout.rs:194-196` treats any line in the smoke step that
  contains `/usr/local/bin/<name>` as a token as proof the binary gets run, and that
  includes a comment line. `:202` only checks that `test ! -e /usr/local/bin/<name>` appears
  somewhere in the step, even though its failure message says "after --uninstall". I ran two
  concrete false greens in a scratch copy, and all 24 tests passed in both:
  (a) replace the whole validator invocation at `.github/workflows/release.yml:74-77` with
  `# TODO smoke /usr/local/bin/wyrd-validate later`;
  (b) move `test ! -e /usr/local/bin/wyrd-validate` from `:93` to just after
  `cd /tmp/wyrd-*` (`:69`), before `./install.sh`. The release run would also pass, so the
  absence check would quietly stop proving anything.
  Fix: skip lines starting with `#`, and require each absence line to come after the
  `./install.sh --uninstall` line. The install.sh split at `:127` has the same weakness at
  lower risk: anything above the `# ── install` banner counts as "the uninstall path".
- NEEDS-HUMAN [impl] — **The brief's "BUILT AND EXERCISED AT CHECK" list includes
  "`obtain_binary`'s extraction list", but no test exercises it.** No test reaches
  `extract_binaries` (`xtask/src/dist.rs:593-612`, the `docker cp` loop the release
  actually uses) or the `assemble` call at `:656` (C4-diff-cov MISS 585-609, 656). Concrete
  surviving mutation: change `:656` to `stage_binaries(&shipped_binaries()[..1], …)` and all
  24 tests still pass. Only the path helper (`:306`) and the `--host` argv (`:312`) are
  pinned. Every break of this kind I could construct fails loudly at release time, not
  silently. `stage_binaries` errors on a missing source. A swapped `image_path` is caught by
  the smoke step, because `wyrd` cannot load before libfdb_c is installed. But that only
  happens on a `v*` tag, which is the expensive place to find it. Cheap fix in the
  `host_build_args` style: a pure function that returns the `docker cp` argv for each binary
  (`<cid>:<image_path>` → `binary_source_path(extracted, b)`), checked without a container.
- NEEDS-HUMAN [human] — **The invariant "no stage may disagree … without the gate saying
  so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every
  `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits
  only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`),
  merges green, and the drift then fails the next unrelated code PR. This gap predates the
  patch (it already affects `readme_dev_section.rs` and the existing README checks), so
  under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue
  rather than an in-PR fix. Either way, the sign-off should not read the invariant as
  covering README-only changes.

## Notes on the gate evidence (not refutations)

- **C5-mutants "fail" is an environment fault, not a test result.** The baseline run on the
  unmodified tree failed at `xtask/tests/repo_hygiene_guards.rs:129` (`git ls-files -s -z
  must succeed`), because the cargo-mutants copy has no `.git`. No mutant was ever run
  against this patch. The hand-run mutations above cover only part of that gap.
- **C4-diff-cov's 0.0% overstates the gap.** That gate ran only `dist_two_binary_layout`,
  which by design calls no `xtask::dist` function. The new pure functions are run by
  `dist_templates.rs`, whose 22 tests pass in `gate-logs/C4-ci.log`. The lines that are
  actually never run are the docker/assemble lines named in the second finding.
- **Deferred, as the brief declares:** no image was built and `install.sh` was not run.
  Docker is present on this host, but I did not try the image build (it needs network
  access and a full release build). The release workflow is still the only place where the
  tarball's contents are observed.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `xtask/tests/dist_two_binary_layout.rs:194`: The release smoke checker counts comments and command arguments as binary execution. Commenting out the entire `wyrd-validate` invocation and usage-check block in `release.yml` still satisfies this predicate: `# if /usr/local/bin/wyrd-validate ...` contains the required token. I reproduced this in memory; deleting the block instead correctly reports the missing invocation. Consequently, the layout tests can remain green after the validator’s runtime smoke coverage disappears. Ignore comments, require the binary in command position, and add a negative test for a commented-out smoke block.

No other introduced correctness bugs or actionable reuse/simplification/efficiency findings identified. Review used the frozen gate evidence; no container build or installer execution was attempted.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Accept deferring real image build, extraction, tarball assembly, and privileged install/uninstall to the release workflow, or require that run before sign-off — those dependencies were not exercised here; evidence is file contracts plus real staging of dummy bytes (brief.md:237; .github/workflows/release.yml:54; pdca-reviewer-742-evidence/coverage-summary.txt:3).
- [ ] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated container-free contract is sufficient to accept this operator-delivery slice — actual two-binary release contents and host installation remain unobserved, so the literal operator outcome is not yet established (brief.md:255; .github/workflows/release.yml:60).
- [ ] **The release-workflow checker ignores comments and order.** `xtask/tests/dist_two_binary_layout.rs:194-196` treats any line in the smoke step that contains `/usr/local/bin/<name>` as a token as proof the binary gets run, and that includes a comment line. `:202` only checks that `test ! -e /usr/local/bin/<name>` appears somewhere in the step, even though its failure message says "after --uninstall". I ran two concrete false greens in a scratch copy, and all 24 tests passed in both: (a) replace the whole validator invocation at `.github/workflows/release.yml:74-77` with `# TODO smoke /usr/local/bin/wyrd-validate later`; (b) move `test ! -e /usr/local/bin/wyrd-validate` from `:93` to just after `cd /tmp/wyrd-*` (`:69`), before `./install.sh`. The release run would also pass, so the absence check would quietly stop proving anything. Fix: skip lines starting with `#`, and require each absence line to come after the `./install.sh --uninstall` line. The install.sh split at `:127` has the same weakness at lower risk: anything above the `# ── install` banner counts as "the uninstall path".
- [ ] **The brief's "BUILT AND EXERCISED AT CHECK" list includes "`obtain_binary`'s extraction list", but no test exercises it.** No test reaches `extract_binaries` (`xtask/src/dist.rs:593-612`, the `docker cp` loop the release actually uses) or the `assemble` call at `:656` (C4-diff-cov MISS 585-609, 656). Concrete surviving mutation: change `:656` to `stage_binaries(&shipped_binaries()[..1], …)` and all 24 tests still pass. Only the path helper (`:306`) and the `--host` argv (`:312`) are pinned. Every break of this kind I could construct fails loudly at release time, not silently. `stage_binaries` errors on a missing source. A swapped `image_path` is caught by the smoke step, because `wyrd` cannot load before libfdb_c is installed. But that only happens on a `v*` tag, which is the expensive place to find it. Cheap fix in the `host_build_args` style: a pure function that returns the `docker cp` argv for each binary (`<cid>:<image_path>` → `binary_source_path(extracted, b)`), checked without a container.
- [ ] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.
- [ ] `xtask/tests/dist_two_binary_layout.rs:194`: The release smoke checker counts comments and command arguments as binary execution. Commenting out the entire `wyrd-validate` invocation and usage-check block in `release.yml` still satisfies this predicate: `# if /usr/local/bin/wyrd-validate ...` contains the required token. I reproduced this in memory; deleting the block instead correctly reports the missing invocation. Consequently, the layout tests can remain green after the validator’s runtime smoke coverage disappears. Ignore comments, require the binary in command position, and add a negative test for a commented-out smoke block.
- [ ] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [ ] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [ ] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring real image build, extraction, tarball assembly, and privileged install/uninstall to the release workflow, or require that run before sign-off — those dependencies were not exercised here; evidence is file contracts plus real staging of dummy bytes (brief.md:237; .github/workflows/release.yml:54; pdca-reviewer-742-evidence/coverage-summary.txt:3).; **The release-workflow checker ignores comments and order.** `xtask/tests/dist_two_binary_layout.rs:194-196` treats any line in the smoke step that contains `/usr/local/bin/<name>` as a token as proof the binary gets run, and that includes a comment line. `:202` only checks that `test ! -e /usr/local/bin/<name>` appears somewhere in the step, even though its failure message says "after --uninstall". I ran two concrete false greens in a scratch copy, and all 24 tests passed in both: (a) replace the whole validator invocation at `.github/workflows/release.yml:74-77` with `# TODO smoke /usr/local/bin/wyrd-validate later`; (b) move `test ! -e /usr/local/bin/wyrd-validate` from `:93` to just after `cd /tmp/wyrd-*` (`:69`), before `./install.sh`. The release run would also pass, so the absence check would quietly stop proving anything. Fix: skip lines starting with `#`, and require each absence line to come after the `./install.sh --uninstall` line. The install.sh split at `:127` has the same weakness at lower risk: anything above the `# ── install` banner counts as "the uninstall path".; **The brief's "BUILT AND EXERCISED AT CHECK" list includes "`obtain_binary`'s extraction list", but no test exercises it.** No test reaches `extract_binaries` (`xtask/src/dist.rs:593-612`, the `docker cp` loop the release actually uses) or the `assemble` call at `:656` (C4-diff-cov MISS 585-609, 656). Concrete surviving mutation: change `:656` to `stage_binaries(&shipped_binaries()[..1], …)` and all 24 tests still pass. Only the path helper (`:306`) and the `--host` argv (`:312`) are pinned. Every break of this kind I could construct fails loudly at release time, not silently. `stage_binaries` errors on a missing source. A swapped `image_path` is caught by the smoke step, because `wyrd` cannot load before libfdb_c is installed. But that only happens on a `v*` tag, which is the expensive place to find it. Cheap fix in the `host_build_args` style: a pure function that returns the `docker cp` argv for each binary (`<cid>:<image_path>` → `binary_source_path(extracted, b)`), checked without a container.; `xtask/tests/dist_two_binary_layout.rs:194`: The release smoke checker counts comments and command arguments as binary execution. Commenting out the entire `wyrd-validate` invocation and usage-check block in `release.yml` still satisfies this predicate: `# if /usr/local/bin/wyrd-validate ...` contains the required token. I reproduced this in memory; deleting the block instead correctly reports the missing invocation. Consequently, the layout tests can remain green after the validator’s runtime smoke coverage disappears. Ignore comments, require the binary in command position, and add a negative test for a commented-out smoke block.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
