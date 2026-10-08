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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 113 instrumentable changed lines executed (below the 80% floor); 113 of 201 changed lines were
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #742: ship `wyrd` and `wyrd-validate` through one distribution pipeline — the container-free contract passes; real-release evidence and packaging policy remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding acceptance criterion separates observable layout/staging contracts from deferred real installation, so the Check result has a defined limit (`brief.md:43`, `brief.md:237`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the discriminator produced an assertion failure naming the missing validator in all four pipeline files, rather than a compile failure (`xtask/tests/dist_two_binary_layout.rs:575`; `pdca-reviewer-742-evidence/red-green.log:29`). |
| C3 Change | PASS | The change stays within the authorized two-binary packaging scope, preserves the existing role set, and updates the living deployment description (`deploy/dist/install.sh:138`, `docs/design/architecture/07-deployment-view.md:42`). |
| C4 Verification (red→green) | PASS | Restoring the fix yielded 30 passing layout/staging tests and `dist --check` passed; the frozen full CI passed, while the independent CI rerun stopped at a read-only advisory-database lock (`pdca-reviewer-742-evidence/red-green.log:119`, `pdca-reviewer-742-evidence/extra-checks.log:9`, `gate-logs/C4-ci.log:3967`). |
| C5 Causal adequacy | PASS | A shared declaration plus checks over both binary sets directly addresses independent pipeline drift; staging and extraction-argument mutations are caught, with the unexercised orchestration covered by T3's explicit decision (`xtask/src/dist.rs:289`, `xtask/tests/dist_templates.rs:560`, `pdca-reviewer-742-evidence/mutants.log:13`). |
| T1 Structure | PASS | Packaging remains in xtask and deployment templates; extraction cleanup covers setup/copy errors, missing binaries fail staging, and no clock, backend-trait, or persisted-data convention is changed (`xtask/src/dist.rs:363`, `xtask/src/dist.rs:605`). |
| T2 Shape | PASS | The existing staging-plan pattern and a single module-included checker keep the production declaration testable without sacrificing the pre-fix discriminator (`xtask/src/dist.rs:281`, `xtask/tests/dist_templates.rs:18`, `xtask/tests/dist_templates.rs:573`). |
| T3 Runtime | NEEDS-HUMAN | Accept deferring real image build/extraction, archive creation, and privileged install/uninstall to release, or require that run before sign-off — Docker/buildx and the installation environment were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and host CLI checks (`.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/coverage-summary.txt:16`, `pdca-reviewer-742-evidence/extra-checks.log:11`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and must be audited at publish; the frozen batch review reports zero blocking findings and the independent path-based prior-art check completed (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`, `pdca-reviewer-742-evidence/prior-art-summary.txt:2`). |
| T5 Judgment | NEEDS-HUMAN | Accept landing packaging before #743 and retaining the maintainer's release checkpoint — the executable currently reports that it validates nothing, while a tag can publish it without a scenario/endurance gate (`crates/validate/src/lib.rs:118`, `.github/workflows/release.yml:115`, `pdca-reviewer-742-evidence/validator-config-run.log:15`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm option A is fit for operators: include the optional validator in every production image and tarball, accepting its added contents and the session-only decision's provenance (`brief.md:17`, `brief.md:331`, `deploy/docker/wyrd/Dockerfile:130`). |

No new implementation defect was established. Source citations above resolve against `$PDCA_TARGET`; brief, gate-log, and reviewer-evidence citations resolve in this review directory. The patch was restored after the red leg and matches a fresh application byte-for-byte (`pdca-reviewer-742-evidence/target-integrity.txt:1`).

The red→green evidence is independently reproduced. `cargo test --offline --locked -p xtask --test dist_two_binary_layout` failed after `git stash`, identifying the absent build, copy, install, uninstall, smoke, and README entries. After `git stash pop`, that test and `dist_templates` passed: 1 + 29 tests, including distinct source bytes, mode 0755, missing-source refusal, the complete staging tree, a third binary naming every inconsistent pipeline file, and the smoke-check negative cases. `dist --check`, installer shell syntax, and cargo-machete also passed (`pdca-reviewer-742-evidence/red-green.log:73`, `pdca-reviewer-742-evidence/extra-checks.log:1`).

The frozen advisory failures do not establish a production defect. C4-diff-cov really recorded 0/113 instrumentable changed lines, but selected only the deliberately text-only discriminator (`gate-logs/C4-diff-cov.log:10`). Re-running LLVM coverage with both test files executed `stage_binaries` three times, `stage_tarball_tree` once, and the table/path/argv helpers; `obtain_binaries`, `extract_binaries`, `assemble`, and `run_dist` remained unexecuted (`pdca-reviewer-742-evidence/coverage-summary.txt:14`). This supplements the frozen result without declaring its coverage threshold met. The frozen mutation run failed its unmutated Git-index check and tested no mutants (`gate-logs/C5-mutants.log:445`). A rerun in a self-contained Git clone with `--cap-lints true` tested 17: 11 caught, five missed in those orchestration functions, and one unviable because `ShippedBinary` lacks `Default` (`pdca-reviewer-742-evidence/mutants.log:5`). Those survivors delimit the release-evidence decision in T3; they do not contradict the narrower contract proved here.

The full-CI evidence has a host caveat. The independent run passed prose checks, repository guards, formatting, workspace clippy/build/tests, then cargo-deny could not lock `/home/eddie/.cargo/advisory-dbs/db.lock` on the read-only host. Temporary test fixtures initially placed inside the source tree also caused cargo-machete scan diagnostics; relocating them and rerunning machete passed. The captured local log is explicitly partial (`pdca-reviewer-742-evidence/ci-rerun-partial.log:1`). The complete frozen CI log records the deny checks, statics/deployment guards, DST checks, and final success (`gate-logs/C4-ci.log:3363`, `gate-logs/C4-ci.log:3967`); the separate TiKV log records both feature-enabled clippy commands completing (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). Shellcheck was unavailable locally; `sh -n` checks syntax only, and release still runs shellcheck (`.github/workflows/release.yml:42`).

Prior art was checked by affected path, beyond the brief's keyword search. I enumerated 356 closed PRs, intersected their changed files with all nine patch paths, and queried merged commit history separately for every path. No closed-unmerged PR touched those paths. The relevant merged predecessors are [the original distribution pipeline, #572](https://github.com/getwyrd/wyrd/pull/572), [the production image, #497](https://github.com/getwyrd/wyrd/pull/497), and later workflow/deployment maintenance; no prior two-binary packaging attempt appeared (`pdca-reviewer-742-evidence/prior-art-summary.txt:2`).

The deferred runtime check has concrete execution steps. If required before sign-off, dispatch the release workflow on the reviewed branch with `gh workflow run release.yml --repo getwyrd/wyrd --ref <reviewed-branch>`, then inspect the “build distribution artifacts” and “smoke the installer in a bookworm container” steps. Both installed binaries must emit usage with the expected status, the validator must run before the FDB client is installed, reinstall must retain operator configuration, and uninstall must remove both binary paths (`.github/workflows/release.yml:70`, `.github/workflows/release.yml:91`). Independently, the host-built validator exited 2 with usage when given no arguments, and exited 0 with an explicit “nothing was validated” notice when given dummy credentials and a complete configuration (`pdca-reviewer-742-evidence/extra-checks.log:11`, `pdca-reviewer-742-evidence/validator-config-run.log:1`). The latter confirms T5's sequencing concern; proposal 0017 explicitly assigns the endurance release checkpoint to a human (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).

One target/brief-state caveat is separate from the patch verdict: the brief assumes #736's `WYRD_VERSION` plumbing is already present, but neither the supplied pre-fix base nor patched files contain it. The diff did not remove it, so this is not a patch regression or C4 failure (`brief.md:165`, `pdca-reviewer-742-evidence/target-integrity.txt:3`). #738's live `--chunk-size` assertions remain present (`xtask/tests/dist_templates.rs:195`).

### Advisory — adversary

# Adversarial review — issue 742 (dist ships `wyrd-validate`, two-binary tarball)

**Bottom line:** I tried to refute the evidence, the fix, and the verdict and could not find
a defect that would ship. There are two findings: one human call about the README wording
for the stub window, and one checker gap that is low severity and should not trigger a
rebuild. The checker gap is not tagged on purpose; reasons are in its bullet.

## Findings

- NEEDS-HUMAN [human] — `deploy/dist/README.md:17` tells operators, in the present tense,
  that `wyrd-validate` "checks that your hardware and configuration are sound by driving a
  running deployment". On this base the binary does neither. It echoes its configuration and
  says so: `crates/validate/src/lib.rs:122` prints "configuration resolved; no requests were
  issued and nothing was validated". So a `v*` tag cut before #743 ships a README that
  promises a check the shipped binary does not do. The brief's Alternative D already makes
  "packaging lands before the tool works" a §9 sign-off item, so this is not a new decision.
  The point is that the README text is part of what the maintainer accepts there. If that is
  not wanted, the fix is one sentence, e.g. "will check … once its scenarios land (#743); in
  this release it resolves and prints its configuration only". That is a judgment about the
  stub window, not a build defect.

- (Low, untagged on purpose: below the bar for a rebuild.) The release smoke checker still
  gives a false green for a step-level `continue-on-error: true`.
  `xtask/tests/dist_two_binary_layout.rs:369-436` rejects a container script without `sh -e`
  or with a `set +e`, and `xtask/tests/dist_templates.rs:727-733` pins both negative cases.
  But it never reads the step's YAML keys. I added `continue-on-error: true` under
  `- name: smoke the installer in a bookworm container` (`.github/workflows/release.yml:60`)
  in a scratch copy, and all 30 tests in `dist_templates` + `dist_two_binary_layout` passed.
  A step-level `if: false` is the same case. The release would then publish even when the
  smoke fails. This is the same "failure swallowed" class the checker hardens against
  inside the script. But it weakens every assertion in the step equally, the pre-existing
  `wyrd` ones included, so it is a property of the release workflow, not of the binary-set
  invariant this slice owns. This is the fourth round of lexical bypasses on this checker.
  The rubric says "do not iterate review rounds chasing silence", so I recommend recording
  it, or adding a one-line "the step carries no `continue-on-error` / `if:`" check in a
  follow-up, rather than spending a Do rebuild on it.

## Refutation attempts that failed (each re-run, not assumed)

- **Red→green evidence.** I re-ran both legs in scratch copies of `$PDCA_TARGET`.
  - Green: with the patch, `cargo test -p xtask --test dist_two_binary_layout --test
    dist_templates` passes 30/30.
  - Red: with every tracked file reverted (`git checkout -- .`) and only the untracked
    `xtask/tests/dist_two_binary_layout.rs` kept, the test compiles and fails at `:577`. It
    names the Dockerfile build and COPY, both install.sh sites, the smoke run and the
    absence check, and the README rows. That matches `gate-logs/C4-verify.log`.
  - The test reads the real Dockerfile, installer, workflow and README, which ARE the
    production artifacts for those stages. It is not a mirror of them and not a tautology.
- **The join to the production table.** `dist_templates.rs` runs the same checker over
  `dist::shipped_binaries()` and pins it equal to `EXPECTED_BINARIES`. The `bin` field the
  equality test skips is pinned by `the_shipped_binary_table_is_consistent_and_collision_free`
  (`image_path == /usr/local/bin/{bin}`, `tarball_dest == bin/{bin}`). Renaming the cargo
  target in `crates/validate/Cargo.toml` would also break
  `crates/validate/tests/cli_surface.rs:78` (`CARGO_BIN_EXE_wyrd-validate`) at compile time.
- **The one-line two-package build** (`deploy/docker/wyrd/Dockerfile:72`). `cargo tree
  --locked -p wyrd-server -p wyrd-validate --features fdb,etcd` resolves cleanly. The same
  features on `-p wyrd-validate` alone are refused. So the Dockerfile comment ("they reach
  `wyrd`") is accurate, and `FEATURES=""` / `tikv,etcd` take the same path.
- **The new smoke lines** (`.github/workflows/release.yml:74-77`). I built
  `wyrd-validate --release` on the host (43 s cold, so the image and compose builds grow
  only a little). With no arguments it exits 2 and prints `usage: wyrd-validate --endpoint
  …` on stderr, which the `2>&1` capture and `grep -q 'usage: wyrd-validate'` match. `ldd`
  shows only `libgcc_s` and `libc`, so running it before the FDB client is installed is
  correct and will not fail at release time. The new lines contain no `$`, backtick or `"`,
  so nothing expands early on the runner inside the outer `sh -eu -c "…"`.
- **Extraction and staging** (`xtask/src/dist.rs:525-626`, `:635-663`).
  - The container is removed whatever `extract_binaries` returns.
  - `target/dist/extracted/` is cleared before the copies.
  - `docker cp <cid>:<file> <dir>/<name>` with an existing parent creates the file.
  - Every mutation I could build in the untested `extract_binaries` loop (e.g. `.take(1)`)
    or the `--host` return path fails loudly, because `stage_binaries` errors on a missing
    source. None produces a silently smaller tarball. The earlier `assemble` mutation is
    now caught by `the_tarball_tree_stages_the_plan_and_every_shipped_binary`.
  - The release signing, provenance and upload globs are `*.tar.gz` / `SHA256SUMS`
    (`release.yml:103-123`), so the new `extracted/` directory cannot leak into them.
- **Installer.** Both sites are top-level/branch-level and unconditional. `ROLES` is
  unchanged, so there is no unit, no env file and no `systemctl` for the validator.
  `--uninstall` uses the same `$BINDIR` (read from `install-prefix`). The summary labels line
  up again (the iteration-2 nit is fixed).
- **Gate reds that are not about the patch.**
  - `C5-mutants` failed its unmutated baseline on
    `repo_hygiene_guards::scan_gitlinks_is_green_over_the_real_index`
    (`xtask/tests/repo_hygiene_guards.rs:137`, "git ls-files -s -z must succeed"). That is
    the mutants temp tree having no git index, an environment fault.
  - `C4-diff-cov` 0% is structural: it measures only the red file, which by design calls no
    `xtask::dist` API. The pure functions and staging are run by `dist_templates.rs`.
    `extract_binaries` and the `assemble`/`tar` path stay unexecuted because they need
    Docker, as the brief declares.
- **The deferred runtime half** (real image build, real tarball, privileged install on a
  host) is the brief's pre-declared §9 item. I am not re-raising it.
- **Path-filter blind spot.** `.github/workflows/fdb-image.yml:18-43` does not trigger on
  `crates/validate/**` and does not smoke the validator in the image. I could not turn this
  into a failing case: the crate is default-compiled by `cargo xtask ci`, takes none of the
  image's features, links only glibc, and its bin name and usage line are pinned by its own
  tests.

### Advisory — code-review

No findings. This diff is clean on both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed the affected source at `$PDCA_TARGET` and the frozen gate evidence. CI passed all 29 `dist_templates` tests and the standalone layout test; the red→green check passed. A read-only `sh -n` check of the installer also passed. The coverage run selected only the text-contract test; mutation testing stopped on an unrelated baseline Git-index failure. Real image and privileged installer execution remain deferred as recorded in the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Accept deferring real image build/extraction, archive creation, and privileged install/uninstall to release, or require that run before sign-off — Docker/buildx and the installation environment were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and host CLI checks (`.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/coverage-summary.txt:16`, `pdca-reviewer-742-evidence/extra-checks.log:11`).
- [ ] T5 Judgment — Accept landing packaging before #743 and retaining the maintainer's release checkpoint — the executable currently reports that it validates nothing, while a tag can publish it without a scenario/endurance gate (`crates/validate/src/lib.rs:118`, `.github/workflows/release.yml:115`, `pdca-reviewer-742-evidence/validator-config-run.log:15`).
- [ ] Validation — fitness-to-purpose — Confirm option A is fit for operators: include the optional validator in every production image and tarball, accepting its added contents and the session-only decision's provenance (`brief.md:17`, `brief.md:331`, `deploy/docker/wyrd/Dockerfile:130`).
- [ ] `deploy/dist/README.md:17` tells operators, in the present tense, that `wyrd-validate` "checks that your hardware and configuration are sound by driving a running deployment". On this base the binary does neither. It echoes its configuration and says so: `crates/validate/src/lib.rs:122` prints "configuration resolved; no requests were issued and nothing was validated". So a `v*` tag cut before #743 ships a README that promises a check the shipped binary does not do. The brief's Alternative D already makes "packaging lands before the tool works" a §9 sign-off item, so this is not a new decision. The point is that the README text is part of what the maintainer accepts there. If that is not wanted, the fix is one sentence, e.g. "will check … once its scenarios land (#743); in this release it resolves and prints its configuration only". That is a judgment about the stub window, not a build defect.
- [ ] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [ ] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [ ] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.
- [ ] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [ ] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.
- [ ] T5 Judgment — Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).
- [ ] T5 Judgment — Accept landing packaging before functional scenarios and retain the human no-early-tag checkpoint, or return sequencing to Plan—the exercised CLI reports that nothing was validated, while any `v*` tag can publish it (crates/validate/src/lib.rs:120; .github/workflows/release.yml:23; .github/workflows/release.yml:115; brief.md:472).
- [ ] **Addendum to the pre-declared T3 deferral: half of the "release-only" evidence will show up on this PR for free.** The brief says the image half can only be observed by a `v*` tag or a `workflow_dispatch` run (`brief.md:255-263`). `.github/workflows/fdb-image.yml:22` path-filters on `deploy/docker/wyrd/**`, which this patch edits, so that job will run on the PR. It runs `docker build --build-arg FEATURES=fdb,etcd` against the new Dockerfile (`:72-78`). A green run proves that `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` and both `COPY` lines (`deploy/docker/wyrd/Dockerfile:72`, `:130-131`) work. It does not run `wyrd-validate` inside the image (its smoke is `wyrd` usage plus `fdbcli`, `:83-95`). It also does not observe extraction, the tarball, or install. At sign-off, check that job's result before accepting the deferral. Whether to add a one-line `docker run --entrypoint wyrd-validate wyrd:fdb` smoke there is a scope call; the brief did not ask for it.

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring real image build/extraction, archive creation, and privileged install/uninstall to release, or require that run before sign-off — Docker/buildx and the installation environment were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and host CLI checks (`.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/coverage-summary.txt:16`, `pdca-reviewer-742-evidence/extra-checks.log:11`).. 10 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
