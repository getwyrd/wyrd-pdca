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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.07s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #742: ship `wyrd-validate` alongside `wyrd` through the production OCI image, operator tarball, installer, and release workflow, with a container-free layout contract.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding contract is falsifiable and explicitly separates container-free acceptance from actual release validation; the remaining packaging decisions are assigned below (`brief.md:45`, `brief.md:227`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the new test produced a compiled assertion failure naming all four inconsistent pipeline files; restoring the patch made it pass (`xtask/tests/dist_two_binary_layout.rs:400`, `pdca-reviewer-742-evidence/red.log:25`). |
| C3 Change | PASS | The patch covers the declared distribution surfaces without adding a validator role or changing validator behavior; the living deployment description agrees with the resulting artifact contract (`deploy/dist/install.sh:138`, `deploy/docker/wyrd/Dockerfile:72`, `docs/design/architecture/07-deployment-view.md:42`). |
| C4 Verification (red→green) | PASS | The binding red→green contract was independently reproduced: 1 pre-fix failure, then 28 targeted passes; workspace build/tests also passed, while full CI completion relies on frozen evidence because local cargo-deny hit a read-only database lock; advisory coverage remains below its floor (`pdca-reviewer-742-evidence/red-green-summary.txt:1`, `pdca-reviewer-742-evidence/verification-summary.txt:1`, `gate-logs/C4-ci.log:3965`). |
| C5 Causal adequacy | PASS | The exercised contract detects missing/extra binaries, misplaced or commented commands, extraction-path mistakes, and incorrect staged bytes; it addresses independently drifting binary sets without a capability probe or symptom guard (`xtask/tests/dist_templates.rs:531`, `xtask/tests/dist_templates.rs:575`, `xtask/tests/dist_templates.rs:826`, `xtask/tests/dist_templates.rs:939`). |
| T1 Structure | PASS | One production declaration feeds the Rust consumers and one checker checks both expected sets; extraction cleanup still runs on failure, and staging rejects a missing binary (`xtask/src/dist.rs:289`, `xtask/src/dist.rs:605`, `xtask/src/dist.rs:663`, `xtask/tests/dist_templates.rs:507`). |
| T2 Shape | PASS | The change preserves the existing template/table pattern, distinct binary roles, dependency direction, and documentation currency; the new test root forbids unsafe code and remains module-includable (`xtask/tests/dist_two_binary_layout.rs:23`, `deploy/dist/README.md:16`, `docs/design/architecture/07-deployment-view.md:42`). |
| T3 Runtime | NEEDS-HUMAN | Accept deferring the real release build and installation, or require that evidence before sign-off — Docker/buildx extraction and privileged bookworm install/uninstall were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and a host-built CLI usage check (`brief.md:237`, `.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/validator-usage.log:1`). |
| T4 Contribution | N/A | Contribution artifacts are drafted after Check; the substantive audit must rerun at publish, as the deferred gate explicitly requires (`gate-logs/T4-contribution.log:10`); affected-path prior art was independently checked as detailed below. |
| T5 Judgment | NEEDS-HUMAN | Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must accept this evidence as sufficient for the operator-delivery slice — passing layout/staging contracts does not itself establish that the released artifacts install and serve operators as intended (`brief.md:252`, `.github/workflows/release.yml:60`). |

No new implementation defect was established. The previous implementation findings are addressed, the binding red→green result is reproducible, and the outstanding decisions concern release evidence and packaging policy. Source paths above are relative to `$PDCA_TARGET`; brief, gate-log, and reviewer-evidence paths are relative to this review directory.

The verification evidence supports the container-free claim, with explicit limits:

- **PASS — independent reproduction and staging.** The pre-fix test compiled and failed on the Dockerfile, installer, release workflow, and README. After `git stash pop`, all 27 `dist_templates` tests and the standalone layout test passed. Source hashes matched after restoration, and the complete input patch passes reverse-application checking. The staging tests checked each binary's own bytes, mode `0755`, the complete staging tree, and failure on a missing source (`pdca-reviewer-742-evidence/red-green-summary.txt:1`; `xtask/tests/dist_templates.rs:887`, `xtask/tests/dist_templates.rs:962`).
- **PASS with host caveat — CI evidence.** The local `cargo xtask ci` run passed typos, documentation lint/render/link audit, repository guards, fmt, workspace clippy/build/tests, and cargo-machete: 1,598 tests passed, 14 were ignored. It stopped at cargo-deny because its advisory-database lock is outside the writable sandbox; an offline retry reproduced that host restriction. The frozen log actually records all three dependency checks passing, subsequent conformance/DST checks, and full CI completion (`gate-logs/C4-ci.log:3361`, `gate-logs/C4-ci.log:3965`). Direct `dist --check`, conformance, and statics also passed locally (`pdca-reviewer-742-evidence/additional-checks.log:1`). The frozen TiKV log records successful feature compilation; that is recorded evidence, not an independent TiKV rerun (`gate-logs/host-tikv.log:7`).
- **FAIL — advisory coverage metric, not a demonstrated functional defect.** The frozen 0/113 result ran only the deliberately API-free discriminator (`gate-logs/C4-diff-cov.log:10`). Running both integration files under LLVM coverage executes 68 of those same 113 changed instrumentable lines, **60.2%**, still below 80%. Remaining misses are predominantly real-tool orchestration, with an error-formatting line also unexecuted. This is not full pipeline execution (`pdca-reviewer-742-evidence/coverage-summary.txt:3`).
- **FAIL — advisory mutation completeness, with the original host fault resolved.** The frozen run never tested mutants: its baseline failed because `git ls-files` could not read an index (`gate-logs/C5-mutants.log:445`). Rerunning with `--copy-vcs true` made the baseline pass. After adding `--cap-lints true` so unused-variable warnings did not falsely make useful mutants unbuildable, the result was **11 caught, 5 missed, 1 unviable**. The five survivors replace `obtain_binaries`, `extract_binaries`, `assemble` twice, and `run_dist`; the executable table/argv/staging contracts were caught. These survivors expose the declared unexercised orchestration boundary, retained for the T3 decision, rather than a new counterexample inside the binding criterion (`pdca-reviewer-742-evidence/mutants-capped.log:1`; `xtask/src/dist.rs:525`, `xtask/src/dist.rs:613`, `xtask/src/dist.rs:674`, `xtask/src/dist.rs:733`).

**PASS — prior art was checked by affected path, including closed work.** The disposable target has only a synthetic base commit, so its local log cannot answer historical questions. I queried all 356 merged/closed repository PRs, compared their changed-file lists with every path in this patch, and expanded both truncated lists. The packaging predecessor is merged PR #572; the Dockerfile predecessor is merged PR #497. No closed, unmerged PR touches an affected path, and no prior two-binary implementation was found. The query responses and per-path results are retained in `pdca-reviewer-742-evidence/prior-art.txt:1`. The frozen batched review reports zero blocking findings, but supplies only that summary (`gate-logs/T4-batch-review.log:10`). The publish-time contribution audit remains **N/A**, not a missing-evidence escalation.

The remaining acceptance work is concrete. On an authorized branch containing this patch, the maintainer can dispatch `.github/workflows/release.yml` with `gh workflow run release.yml --repo getwyrd/wyrd --ref <branch>`, then inspect the distribution-build and installer-smoke steps. They must demonstrate both tarball binaries, validator usage before installing the FDB client, roles-binary usage after installing it, preserved configuration on reinstall, and absence of both installed binaries after uninstall (`.github/workflows/release.yml:54`, `.github/workflows/release.yml:74`, `.github/workflows/release.yml:91`). No workflow was dispatched by this review. Before a release tag, the maintainer must also resolve the option-A/early-packaging decision and the proposal's human endurance-verdict checkpoint; the current validator explicitly reports that it has issued no requests and validated nothing (`crates/validate/src/lib.rs:122`).

### Advisory — adversary

# Adversarial review — issue 742 (dist ships `wyrd-validate`)

Verdict: **I could not refute the fix.** The red→green is real, and the pipeline change is correct as written. I found one small regression the diff introduced (a one-space misalignment), a few ways to weaken the release smoke step that the layout checker would not catch, and two untested lines in the Docker/`--host` paths. All checked against the target at `$PDCA_TARGET`, in a scratch copy.

## Refutation attempts

- **Evidence: re-ran red→green myself. Could not refute.** Patched tree: `dist_templates` 27/27 and `dist_two_binary_layout` 1/1 pass. Then I reverted all 8 modified files and kept only the new test. `xtask/tests/dist_two_binary_layout.rs:402` fails with 9 disagreements: the Dockerfile build and `COPY`, both install.sh sites, both release.yml checks, and the README table and install path. Those are the right reasons. The test reads the real repo files, not a copy, and names no new API, so it compiles on the reverted tree.

- **Checker: three false greens remain, all needing a deliberate weakening edit.** (Not tagged NEEDS-HUMAN. I suggest declining these with a recorded reason rather than spending another round. Each needs a deliberate edit, two of the three still fail loudly at release, and a text checker can always be fooled one level deeper.)
  - (A1) Replace `.github/workflows/release.yml:74-77` with `/usr/local/bin/wyrd-validate || true`. All 28 tests pass. The cause is that `dist_two_binary_layout.rs:310-312` only requires the binary in command position, not that its result is checked. Once this edit lands, the release smoke step passes even if the validator is missing or can't load.
  - (A2) Move `test ! -e /usr/local/bin/wyrd-validate` from `release.yml:93` to just after the closing `"` at `:96`. That line then runs on the runner host, where it is always true. All tests pass, because the `:319` check looks at line order inside the step, not at whether the line sits inside the `docker run` script.
  - (A3) Wrap `deploy/dist/install.sh:141` in `if [ -f "$HERE/bin/wyrd-validate" ]; then … fi`. The checker (`dist_two_binary_layout.rs:221`) stays green. The only test that failed was a meta-test helper, by accident: `dist_templates.rs:495`, "the mutation matched no line". On its own, A3 is still caught at release by `release.yml:74-77`.

- **Rust side: two mutations survive, both fail loudly in a real run.** The mutants gate never ran (see below), so I mutated by hand.
  - (M1) Change `xtask/src/dist.rs:621` to `.into_iter().take(1)`, so only `wyrd` is extracted. All xtask tests pass.
  - (M2) Change `dist.rs:533` to return `target/debug` for `--host`. All xtask tests pass.
  - Both end in `stage_binaries` erroring on a missing file, because `extract_binaries` clears the extraction dir first (`:614-617`). But a third mutation, removing that clear, also survives. So the brief's "`obtain_binary`'s extraction list" is exercised only as an argv (the list of command-line arguments): the loop that runs it, and the `--host` return path, are not. This is the declared deferral to the release workflow. I'm noting it, not raising it.
  - (M3) Staging only the first binary (`dist.rs:663` → `[..1]`) **is** caught, by `the_tarball_tree_stages_the_plan_and_every_shipped_binary`.

- NEEDS-HUMAN [impl] — `deploy/dist/install.sh:202`: the patch dropped a space from the `units` label (`  units    ` → `  units   `, diff line 162). In the operator-facing install summary, the units path now starts one column left of the `binary` / `tool` / `config` / `data` rows at `:200-204`. Before this diff all five rows lined up. One-character fix: restore the fourth space.

- **Gate evidence: what the two red rows in check-gates.json mean.**
  - C4-diff-cov's "0.0%" is built into the design, not a coverage hole. The gate measures only `--test dist_two_binary_layout`, which by the brief's design never calls `xtask::dist`, so it can't cover `dist.rs`. The `dist_templates.rs` tests do run the new code (M3 above was caught).
  - C5-mutants tested nothing. The baseline failed at `xtask/tests/repo_hygiene_guards.rs:137` (`git ls-files` inside cargo-mutants' non-git copy). That is a harness fault unrelated to this diff. Don't read "no surviving mutants" into either row. My hand mutations above are the only mutation evidence for this round.

- **Fix: things I tried to break and could not.**
  - The two-bin build line (`Dockerfile:72`): cargo accepted `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd`. I started it and it began compiling, with no target or feature selection error (the root is a virtual workspace).
  - `cargo tree -p wyrd-validate -e normal` shows no `openssl-sys`, `ring` or `aws-lc`. So the validator runs in bare bookworm before `libfdb_c` is installed, as `release.yml:72-77` assumes.
  - The smoke step expects a non-zero exit plus `usage: wyrd-validate`. That matches `crates/validate/src/args.rs:179-184` and `EXIT_USAGE = 2`, and `crates/validate/tests/cli_surface.rs:171` already pins it.
  - The README claim "refuses to run without … S3 credentials" holds. `crates/validate/src/access_keys.rs` takes only an `AWS_*` or `WYRD_S3_*` env pair and has no profile or IMDS (EC2 instance metadata) fallback.
  - `docker rm -f` still runs whatever `extract_binaries` returns.
  - The checker's exact word matches mean `wyrd-validate` can never satisfy a check meant for `wyrd`.
  - `ROLES` is unchanged, and the checker rejects any attempt to grow it.

### Advisory — code-review

No findings. This diff is clean on both advisory lenses: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found.

Reviewed the shared binary table, extraction and cleanup, staging, installer symmetry, release smoke commands, and regression assertions against the target source (`xtask/src/dist.rs:289`, `xtask/src/dist.rs:605`, `xtask/src/dist.rs:663`, `deploy/dist/install.sh:117`, `deploy/dist/install.sh:141`, `.github/workflows/release.yml:74`, `xtask/tests/dist_templates.rs:575`, `xtask/tests/dist_templates.rs:939`).

Frozen CI evidence records all 27 distribution-template tests and the standalone layout test passing. The coverage run selected only the text-based test; mutation testing stopped on an unrelated Git-index baseline failure. Real image builds and privileged installation remain deferred as specified in the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Accept deferring the real release build and installation, or require that evidence before sign-off — Docker/buildx extraction and privileged bookworm install/uninstall were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and a host-built CLI usage check (`brief.md:237`, `.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/validator-usage.log:1`).
- [ ] T5 Judgment — Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).
- [ ] Validation — fitness-to-purpose — The maintainer must accept this evidence as sufficient for the operator-delivery slice — passing layout/staging contracts does not itself establish that the released artifacts install and serve operators as intended (`brief.md:252`, `.github/workflows/release.yml:60`).
- [ ] `deploy/dist/install.sh:202`: the patch dropped a space from the `units` label (`  units    ` → `  units   `, diff line 162). In the operator-facing install summary, the units path now starts one column left of the `binary` / `tool` / `config` / `data` rows at `:200-204`. Before this diff all five rows lined up. One-character fix: restore the fourth space.
- [ ] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [ ] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [ ] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.
- [ ] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [ ] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T3 Runtime — Accept deferring the real release build and installation, or require that evidence before sign-off — Docker/buildx extraction and privileged bookworm install/uninstall were not exercised; the evidence is compiled contracts, real staging of dummy bytes, and a host-built CLI usage check (`brief.md:237`, `.github/workflows/release.yml:54`, `pdca-reviewer-742-evidence/validator-usage.log:1`).; `deploy/dist/install.sh:202`: the patch dropped a space from the `units` label (` units ` → ` units `, diff line 162). In the operator-facing install summary, the units path now starts one column left of the `binary` / `tool` / `config` / `data` rows at `:200-204`. Before this diff all five rows lined up. One-character fix: restore the fourth space.. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
