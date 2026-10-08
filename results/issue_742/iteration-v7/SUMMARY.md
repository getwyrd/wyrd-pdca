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
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 81 instrumentable changed lines executed (below the 80% floor); 81 of 177 changed lines were i
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_742/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.08s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #742's two-binary distribution contract passes advisory review; real-artifact proof, release readiness, and the production-image packaging decision still require human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit and falsifiable: shared binary-set contracts plus real staging, with image/host-install proof explicitly deferred (brief.md:45; brief.md:218; brief.md:237). |
| C2 Reproduction (red pre-fix) | PASS | Stashing tracked changes while retaining the new discriminator produced a compiled assertion failure naming all four pipeline files; the patch was restored byte-for-byte (xtask/tests/dist_two_binary_layout.rs:999; pdca-reviewer-742-evidence/red-green.log:34; pdca-reviewer-742-evidence/red-green.log:74). |
| C3 Change | PASS | Both distribution paths and install/uninstall cover the declared set without adding a validator role, unit, or configuration; the living deployment description is updated (xtask/src/dist.rs:558; xtask/src/dist.rs:642; xtask/src/dist.rs:696; deploy/dist/install.sh:118; deploy/dist/install.sh:142; docs/design/architecture/07-deployment-view.md:42). |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 41 targeted tests and dist --check; the frozen full-CI pass is supported by its log, while the independent full-CI attempt stopped only at a read-only advisory-database lock (pdca-reviewer-742-evidence/red-green.log:115; pdca-reviewer-742-evidence/dist-check.log:1; gate-logs/C4-ci.log:3974; pdca-reviewer-742-evidence/ci.log:3362). |
| C5 Causal adequacy | PASS | The fix addresses the independent binary declarations directly; shared checking and distinct-byte staging assertions catch set/mapping drift, with no capability probe masking a load-time cause; runner reach remains the T3 limitation (xtask/tests/dist_templates.rs:492; xtask/tests/dist_templates.rs:506; xtask/tests/dist_templates.rs:625; pdca-reviewer-742-evidence/mutants.out/caught.txt:1). |
| T1 Structure | PASS | Packaging logic stays in xtask, the validator retains its independent dependency boundary, and extraction preparation precedes container creation with cleanup after copy failures (xtask/src/dist.rs:633; xtask/src/dist.rs:649; pdca-reviewer-742-evidence/ci.log:18). |
| T2 Shape | PASS | The same checker serves the local and production sets; the restricted smoke grammar and planted regressions now reject suppressed/inverted assertions and broken outer quoting (xtask/tests/dist_templates.rs:22; xtask/tests/dist_templates.rs:903; xtask/tests/dist_templates.rs:1467; xtask/tests/dist_two_binary_layout.rs:869). |
| T3 Runtime | NEEDS-HUMAN | Decide whether real release-workflow evidence is required before sign-off — Docker/buildx, image extraction, tarball assembly, and privileged installation were not exercised; compiled contracts and dummy-byte staging do not prove those paths (.github/workflows/release.yml:55; xtask/tests/dist_templates.rs:625; pdca-reviewer-742-evidence/mutants.log:4). |
| T4 Contribution | PASS | Affected-path history and all 356 closed PRs were checked, including pagination for large PRs; no closed-unmerged overlap or earlier two-binary packaging attempt was found; contribution-artifact auditing is N/A until publish (pdca-reviewer-742-evidence/prior-art.log:1; pdca-reviewer-742-evidence/prior-art.log:63; gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN | Decide whether packaging may land before #743, who withholds premature release tags, and whether the README must qualify its present-tense capability claim — the exercised CLI validates nothing, while a v* tag can publish it (deploy/dist/README.md:11; crates/validate/src/lib.rs:122; .github/workflows/release.yml:23; pdca-reviewer-742-evidence/validator-stub.log:16). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Reconfirm option A is suitable for operators — adding the standalone validator to the production image is an architectural/product choice recorded only in the planning session, and test success cannot supply that acceptance (brief.md:16; deploy/docker/wyrd/Dockerfile:137). |

Source citations are relative to `$PDCA_TARGET`; brief, frozen-log, and reviewer-evidence citations are relative to this review directory. No implementation changes were made. The disposable target matches the supplied patch, and stash/pop preserved it exactly (pdca-reviewer-742-evidence/target-state.log:4; pdca-reviewer-742-evidence/red-green.log:74).

The evidence supports the container-free claim, with these limits:

- **Red→green reproduced:** `cargo test --offline -p xtask --test dist_two_binary_layout` failed with one assertion failure before the fix. After restoration, `dist_templates`, `dist_two_binary_layout`, and `fdb_image` passed 33 + 2 + 6 tests. The failure names the missing validator build/copy, install/removal, smoke/absence checks, and README description; this was not a compile-error red (pdca-reviewer-742-evidence/red-green.log:34).
- **CI independently rerun as far as the sandbox permits:** spelling, docs lint/render, repository/blackbox guards, fmt, workspace clippy/build/tests, and cargo-machete passed. Cargo-deny then could not lock its read-only advisory database. This is a host caveat, not a patch defect. The frozen log explicitly records successful deny checks, conformance, statics/deployment guards, and DST checks through completion (pdca-reviewer-742-evidence/ci.log:3362; gate-logs/C4-ci.log:3370; gate-logs/C4-ci.log:3974). Independent statics and conformance reruns also passed; installer/workflow shell syntax and `dist --check` passed (pdca-reviewer-742-evidence/scanners.log:1; pdca-reviewer-742-evidence/shell-syntax.log:1; pdca-reviewer-742-evidence/dist-check.log:1). Shellcheck was unavailable locally; shell syntax checking is not represented as shellcheck.
- **Coverage's frozen failure is real but selection-limited:** its command selects only the deliberately text-only discriminator and measures 0/81 changed instrumentable production lines (gate-logs/C4-diff-cov.log:10; gate-logs/C4-diff-cov.log:117). An independent LLVM coverage run selecting all three affected suites executes 66/81, or 81.5%. The remaining misses include the actual extraction/assembly runner and an error closure; they are not claimed exercised (pdca-reviewer-742-evidence/coverage-summary.txt:1).
- **Mutation evidence was recovered:** the frozen run tested zero mutants because its baseline could not read a Git index (gate-logs/C5-mutants.log:456). Rerunning with `--copy-vcs true --cap-lints true` passed the baseline and tested 17 mutants: 12 caught, one unviable because `ShippedBinary` lacks `Default`, and four missed. The survivors replace `obtain_binaries`, `assemble` (two variants), and `run_dist`; they expose the declared runner verification gap, not a demonstrated defect in the current implementation (pdca-reviewer-742-evidence/mutants.log:3; pdca-reviewer-742-evidence/mutants.out/unviable.txt:1).
- **Remaining frozen rows were read:** the TiKV log shows both requested clippy invocations finished successfully; this is compile evidence, not a running-backend test (gate-logs/host-tikv.log:110; gate-logs/host-tikv.log:209). The batch-review log reports zero blocking findings but supplies no review detail (gate-logs/T4-batch-review.log:10). Contribution artifacts are **N/A**: they are intentionally absent at Check, and the substantive audit reruns at publish (gate-logs/T4-contribution.log:10).

The unresolved release decisions have concrete evidence and a runnable completion path. The host-built validator exits 2 and prints usage with no arguments. With complete arguments, dummy credentials, and endpoint `http://127.0.0.1:1`, it exits 0 and explicitly reports that no requests were issued and nothing was validated (pdca-reviewer-742-evidence/validator-usage.log:1; pdca-reviewer-742-evidence/validator-stub.log:1). This confirms the README/readiness question; implementing scenarios remains tracked in #743 and is not requested as an in-PR expansion. The proposal explicitly leaves endurance-before-tag enforcement to a human runbook step (docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013).

If the maintainer requires real-artifact proof, dispatch `gh workflow run release.yml --repo getwyrd/wyrd --ref <reviewed-branch>`, then inspect the distribution-build and bookworm-installer-smoke steps. Require both binary usage checks, validator execution before FDB client installation, successful reinstall/config preservation, and absence of both binaries after uninstall. A branch dispatch does not execute the tag-only release-attachment step (.github/workflows/release.yml:92; .github/workflows/release.yml:101; .github/workflows/release.yml:109; .github/workflows/release.yml:133). No workflow was dispatched during this advisory review.

The supplied target also differs from one brief assumption: the #736 `WYRD_VERSION` transport described as already present is absent in both its synthetic base and patched files. The supplied patch applies consistently and does not remove that transport; this is a target/base-state caveat, not a #742 verification defect (brief.md:175; pdca-reviewer-742-evidence/target-state.log:21; patch.diff:519).

### Advisory — adversary

# Adversarial review — issue 742 (two-binary tarball), iteration 7

Verdict: the production change holds up. I could not break the pipeline change itself. I did find real holes in the test checker, but they are holes in a guard against future deliberate weakening edits. They are not defects in what ships.

## What I tried to refute and could not

- **Red→green is genuine.** I re-ran it in a scratch copy: every modified file reverted, only `xtask/tests/dist_two_binary_layout.rs` kept. `every_pipeline_file_names_every_expected_binary` fails, naming all 8 missing places (Dockerfile build + COPY, install.sh install + rm, release.yml run block + absence check, README ×2). With the patch it passes. The test reads the real repo files, not a copy, so it cannot pass as a tautology.
- **Criterion 5's diagnostic works as stated.** I added a third entry (`wyrd-foo`) to `shipped_binaries()` (`xtask/src/dist.rs:78`) and changed nothing else. `every_pipeline_file_names_every_shipped_binary` then names all four pipeline files, and `the_shipped_binary_table_is_the_text_tests_local_set` says to update `EXPECTED_BINARIES`.
- **The round-5 quote defect is really gone from the real file.** I fed the actual smoke step (`.github/workflows/release.yml:76-114`) through bash with `docker` stubbed out. The container receives the whole 34-line script, ending at `test -d /etc/wyrd`, and `dash -n` parses it.
- **The validator's output matches both smoke greps.** `wyrd-validate` with no arguments exits 2 and prints `usage: wyrd-validate …` on **stderr**. So the stderr-only capture at `.github/workflows/fdb-image.yml:105` and the `2>&1` capture at `.github/workflows/release.yml:92` both match.
- **One cargo build with two `--bin`s works** (`deploy/docker/wyrd/Dockerfile:78`). With `--features fdb,etcd`, cargo's unit graph gives `wyrd-server/wyrd` the features `[default, etcd, fdb]` and `wyrd-validate` none. Building both together only adds a few features to the validator's dependencies (futures `io`/`sink`, tokio `io-std`) and no `-sys` (native library) crates, so the README's "does not link `libfdb_c`" claim holds.
- **Mutation testing** (my own run; see the gate notes below for why C5 produced nothing): all 12 mutants in the pure helpers are caught (`name`, `shipped_binaries`, `extracted_binary_path`, `host_build_args`, `docker_cp_args`, `prepare_extraction_dir`, `stage_binaries`). The 4 survivors are the Docker-only paths: `obtain_binaries` (`xtask/src/dist.rs:557`), `assemble` (`:657`, including the `stage_binaries(&shipped_binaries(), …)` call at `:696`) and `run_dist` (`:736`). That is exactly the deferral the brief names.

## Findings

- NEEDS-HUMAN [human] — **The release-smoke checker still has a concrete false green after seven rounds: `set -n` / `set -o noexec`.** `xtask/tests/dist_two_binary_layout.rs:697-702` only rejects `set +…`. `set -n` makes a non-interactive shell read the rest of the script without running it, and exit 0. I confirmed this with both dash and bash. I planted (a) `set -n` on its own line above `.github/workflows/release.yml:92`, (b) `set -o noexec` above `:85` and (c) `set -o noexec` in the runner shell above `docker run` (`:80`). Both `dist_two_binary_layout` tests stayed green each time. A real run would execute nothing after that line (in case (c), not even the container) and the step would pass. The fix is one line: treat `-n`/`noexec` like `set +`. **But this is the pattern the rubric's reviewer protocol warns about** ("Do not iterate review rounds chasing silence"). Every round has found one more shell built-in the checker misses, and there will be another. My recommendation: decide a stopping rule rather than spend round 8 on this alone. Either record it as a known limit, or fold the one-liner into any rebuild that happens for another reason.
- NEEDS-HUMAN [human] — **The installer check is far weaker than the workflow check, and lets the installer silently skip a missing validator.** `xtask/tests/dist_two_binary_layout.rs:227-234` only asks that some non-comment line after the uninstall path starts with `install` and carries the right tokens. Two plausible edits to `deploy/dist/install.sh:142` both left the gate green in my probe: `install -m 0755 "$HERE/bin/wyrd-validate" "$BINDIR/wyrd-validate" || true`, and wrapping that line in `if [ -f "$HERE/bin/wyrd-validate" ]; then … fi`. The second edit is tempting because the README calls the validator "optional". It is the rubric's "silent skip" class: a tarball without the validator installs without complaint. Other safeguards catch it before operators see it: `stage_binaries` refuses a missing source (`xtask/src/dist.rs:430`), and the release smoke runs the binary. But that happens only on a `v*` tag. Same stopping-rule question as above.
- NEEDS-HUMAN [human] — **The test code is now heavier than the problem.** This slice adds about 2,300 lines of tests (1,004 in `dist_two_binary_layout.rs`, 1,197 in `dist_templates.rs`, 124 in `fdb_image.rs`) against about 180 lines of production change. Most of that weight is a hand-written model of shell quoting and statement shape (`dist_two_binary_layout.rs:254-821`) and 70+ planted-drift cases. It also constrains `release.yml` itself: no loop, `if` or group in the smoke step (`.github/workflows/release.yml:67-74`). So the natural shell way to iterate a binary set (`for b in …`) is now banned by the gate meant to protect that set. The same patch shows a much cheaper alternative. For `fdb-image.yml` it pins the whole validator step as exact text in about 10 lines (`xtask/tests/fdb_image.rs:358`). Pinning the release smoke step the same way would close the `set -n` hole and every hole like it by construction. The block could be generated per binary from the table. The cost is updating the pin whenever the step is edited on purpose. Whether to accept the current weight or ask for that simplification is a maintainability call for the maintainer.
- NEEDS-HUMAN [human] — **The brief's claim that the image build can only be observed on a `v*` tag is partly wrong, and the gap is cheap to close before sign-off.** `fdb-image.yml` triggers on this PR: its path filters include `deploy/docker/wyrd/**` (`.github/workflows/fdb-image.yml:22`) and `crates/validate/**` (`:47`). That job builds the changed two-binary Dockerfile with `FEATURES=fdb,etcd` (`:77-83`) and runs `wyrd-validate` inside the built image (`:102-109`). So "the image builds and carries both binaries" will be observed on the PR's own CI. Read that job's result at sign-off instead of accepting it as deferred. Only the tarball and the installer still need the release workflow (`workflow_dispatch`, `.github/workflows/release.yml:24`).
- NEEDS-HUMAN [human] — **Already on the maintainer's open list from round 5, repeated so it isn't lost:** `deploy/dist/README.md:9-14` says in the present tense that the validator "drives a deployment through its S3 front door exactly the way a client does". The shipped binary "still resolves its configuration, echoes it, and exits" (`crates/validate/Cargo.toml:3`). If a `v*` tag is cut before #743, operators get a README describing behaviour the binary doesn't have.

## Gate notes (not refutations)

- **C5-mutants red is an environment fault, not a patch defect.** cargo-mutants' temp copy has no `.git`, so `xtask/tests/repo_hygiene_guards.rs:137` (`git ls-files` must succeed) fails the unmutated baseline. Even with `.git` present, `warnings = "deny"` (`Cargo.toml:251`) made 14 of 17 mutants fail to compile. My numbers above come from `--in-place` on a scratch copy with `RUSTFLAGS=--cap-lints=warn`. The gate needs both adjustments to say anything useful about this crate.
- **C4-diff-cov 0% is built into the design, not a gap.** The gate measures only the red file, which by design calls no `xtask::dist` code. `dist_templates.rs` exercises those lines, and the mutation run above confirms it.
- **One overstated claim in `check-gates.json`:** the C4-verify row says "2 test(s) ran red", but the red log shows `1 passed; 1 failed`. `the_expected_set_is_well_formed` passes on both legs. This is harness wording only; the red leg itself is real.

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed the target source and frozen gate evidence. CI and red→green checks passed; shell syntax checks also passed. The coverage run selected only the file-text test, and mutation testing stopped at an unrelated repository-index assertion before testing mutants. Real image build and privileged installation remain deferred as stated in the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T3 Runtime — Decide whether real release-workflow evidence is required before sign-off — Docker/buildx, image extraction, tarball assembly, and privileged installation were not exercised; compiled contracts and dummy-byte staging do not prove those paths (.github/workflows/release.yml:55; xtask/tests/dist_templates.rs:625; pdca-reviewer-742-evidence/mutants.log:4).
- [x] T5 Judgment — Decide whether packaging may land before #743, who withholds premature release tags, and whether the README must qualify its present-tense capability claim — the exercised CLI validates nothing, while a v* tag can publish it (deploy/dist/README.md:11; crates/validate/src/lib.rs:122; .github/workflows/release.yml:23; pdca-reviewer-742-evidence/validator-stub.log:16).
- [x] Validation — fitness-to-purpose — Reconfirm option A is suitable for operators — adding the standalone validator to the production image is an architectural/product choice recorded only in the planning session, and test success cannot supply that acceptance (brief.md:16; deploy/docker/wyrd/Dockerfile:137).
- [ ] **The release-smoke checker still has a concrete false green after seven rounds: `set -n` / `set -o noexec`.** `xtask/tests/dist_two_binary_layout.rs:697-702` only rejects `set +…`. `set -n` makes a non-interactive shell read the rest of the script without running it, and exit 0. I confirmed this with both dash and bash. I planted (a) `set -n` on its own line above `.github/workflows/release.yml:92`, (b) `set -o noexec` above `:85` and (c) `set -o noexec` in the runner shell above `docker run` (`:80`). Both `dist_two_binary_layout` tests stayed green each time. A real run would execute nothing after that line (in case (c), not even the container) and the step would pass. The fix is one line: treat `-n`/`noexec` like `set +`. **But this is the pattern the rubric's reviewer protocol warns about** ("Do not iterate review rounds chasing silence"). Every round has found one more shell built-in the checker misses, and there will be another. My recommendation: decide a stopping rule rather than spend round 8 on this alone. Either record it as a known limit, or fold the one-liner into any rebuild that happens for another reason.
- [ ] **The installer check is far weaker than the workflow check, and lets the installer silently skip a missing validator.** `xtask/tests/dist_two_binary_layout.rs:227-234` only asks that some non-comment line after the uninstall path starts with `install` and carries the right tokens. Two plausible edits to `deploy/dist/install.sh:142` both left the gate green in my probe: `install -m 0755 "$HERE/bin/wyrd-validate" "$BINDIR/wyrd-validate" || true`, and wrapping that line in `if [ -f "$HERE/bin/wyrd-validate" ]; then … fi`. The second edit is tempting because the README calls the validator "optional". It is the rubric's "silent skip" class: a tarball without the validator installs without complaint. Other safeguards catch it before operators see it: `stage_binaries` refuses a missing source (`xtask/src/dist.rs:430`), and the release smoke runs the binary. But that happens only on a `v*` tag. Same stopping-rule question as above.
- [ ] **The test code is now heavier than the problem.** This slice adds about 2,300 lines of tests (1,004 in `dist_two_binary_layout.rs`, 1,197 in `dist_templates.rs`, 124 in `fdb_image.rs`) against about 180 lines of production change. Most of that weight is a hand-written model of shell quoting and statement shape (`dist_two_binary_layout.rs:254-821`) and 70+ planted-drift cases. It also constrains `release.yml` itself: no loop, `if` or group in the smoke step (`.github/workflows/release.yml:67-74`). So the natural shell way to iterate a binary set (`for b in …`) is now banned by the gate meant to protect that set. The same patch shows a much cheaper alternative. For `fdb-image.yml` it pins the whole validator step as exact text in about 10 lines (`xtask/tests/fdb_image.rs:358`). Pinning the release smoke step the same way would close the `set -n` hole and every hole like it by construction. The block could be generated per binary from the table. The cost is updating the pin whenever the step is edited on purpose. Whether to accept the current weight or ask for that simplification is a maintainability call for the maintainer.
- [x] **The brief's claim that the image build can only be observed on a `v*` tag is partly wrong, and the gap is cheap to close before sign-off.** `fdb-image.yml` triggers on this PR: its path filters include `deploy/docker/wyrd/**` (`.github/workflows/fdb-image.yml:22`) and `crates/validate/**` (`:47`). That job builds the changed two-binary Dockerfile with `FEATURES=fdb,etcd` (`:77-83`) and runs `wyrd-validate` inside the built image (`:102-109`). So "the image builds and carries both binaries" will be observed on the PR's own CI. Read that job's result at sign-off instead of accepting it as deferred. Only the tarball and the installer still need the release workflow (`workflow_dispatch`, `.github/workflows/release.yml:24`).
- [x] **Already on the maintainer's open list from round 5, repeated so it isn't lost:** `deploy/dist/README.md:9-14` says in the present tense that the validator "drives a deployment through its S3 front door exactly the way a client does". The shipped binary "still resolves its configuration, echoes it, and exits" (`crates/validate/Cargo.toml:3`). If a `v*` tag is cut before #743, operators get a README describing behaviour the binary doesn't have.
- [x] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [x] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [x] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.
- [ ] size backstop — this slice is behaving oversized: patch is 131 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [x] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.
- [x] T5 Judgment — Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).
- [x] T5 Judgment — Accept landing packaging before functional scenarios and retain the human no-early-tag checkpoint, or return sequencing to Plan—the exercised CLI reports that nothing was validated, while any `v*` tag can publish it (crates/validate/src/lib.rs:120; .github/workflows/release.yml:23; .github/workflows/release.yml:115; brief.md:472).
- [x] **Addendum to the pre-declared T3 deferral: half of the "release-only" evidence will show up on this PR for free.** The brief says the image half can only be observed by a `v*` tag or a `workflow_dispatch` run (`brief.md:255-263`). `.github/workflows/fdb-image.yml:22` path-filters on `deploy/docker/wyrd/**`, which this patch edits, so that job will run on the PR. It runs `docker build --build-arg FEATURES=fdb,etcd` against the new Dockerfile (`:72-78`). A green run proves that `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` and both `COPY` lines (`deploy/docker/wyrd/Dockerfile:72`, `:130-131`) work. It does not run `wyrd-validate` inside the image (its smoke is `wyrd` usage plus `fdbcli`, `:83-95`). It also does not observe extraction, the tarball, or install. At sign-off, check that job's result before accepting the deferral. Whether to add a one-line `docker run --entrypoint wyrd-validate wyrd:fdb` smoke there is a scope call; the brief did not ask for it.
- [x] T5 Judgment — Accept landing packaging before #743 and retaining the maintainer's release checkpoint — the executable currently reports that it validates nothing, while a tag can publish it without a scenario/endurance gate (`crates/validate/src/lib.rs:118`, `.github/workflows/release.yml:115`, `pdca-reviewer-742-evidence/validator-config-run.log:15`).
- [x] `deploy/dist/README.md:17` tells operators, in the present tense, that `wyrd-validate` "checks that your hardware and configuration are sound by driving a running deployment". On this base the binary does neither. It echoes its configuration and says so: `crates/validate/src/lib.rs:122` prints "configuration resolved; no requests were issued and nothing was validated". So a `v*` tag cut before #743 ships a README that promises a check the shipped binary does not do. The brief's Alternative D already makes "packaging lands before the tool works" a §9 sign-off item, so this is not a new decision. The point is that the README text is part of what the maintainer accepts there. If that is not wanted, the fix is one sentence, e.g. "will check … once its scenarios land (#743); in this release it resolves and prints its configuration only". That is a judgment about the stub window, not a build defect.
- [x] T5 Judgment — Reconfirm option A and landing packaging before #743 — any early v* tag can publish the configuration-only validator, so release sequencing remains the maintainer's responsibility (`brief.md:472`, `brief.md:622`, `.github/workflows/release.yml:23`, `crates/validate/src/lib.rs:120`).
- [x] **The operator README describes a capability the shipped binary does not have yet.** This is the round-5 open maintainer question, still unchanged; it is not new. `deploy/dist/README.md:11-12` says the validator "drives a deployment through its S3 front door exactly the way a client does". The crate's own manifest says the binary "still resolves its configuration, echoes it, and exits" (`crates/validate/Cargo.toml:3`), and I confirmed it does nothing else today. This ties to the brief's §9 "stub ships before #743" acceptance (`brief.md:455-476`). If that is accepted, the README sentence should either be future-tense or say "today it only checks its arguments". That wording choice belongs to the maintainer, not to Do.

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
- Iteration delta (if iterating): Production change is accepted as sound; only the test design changes. - Replace the hand-written shell-quoting/statement-shape model in xtask/tests/dist_two_binary_layout.rs (and its 70+ planted-drift cases in dist_templates.rs) with the cheaper exact-text pin already used for fdb-image.yml (xtask/tests/fdb_image.rs:358): pin the release.yml smoke step as exact expected text, with the per-binary block generated from the shipped-binary table. This closes the `set -n` / `set -o noexec` false green and every similar hole by construction, and cuts the test weight (~2,300 test lines vs ~180 production). - Do NOT forbid `for` loops, `if` or groups in the release smoke step — banning the natural way to iterate the binary set is wrong. A loop over the binaries is fine. - Apply the same exact-text approach to the install.sh install/uninstall lines so `|| true` or an `if [ -f … ]` guard around the validator install cannot pass silently. - Keep: the binary table in xtask::dist, the shared checker over both the local and production sets, byte-for-byte staging test, distinct-path assertions, the red-earning text test. - Settled at sign-off, do not revisit: shipping before #743 is OK (nothing released yet); README present-tense wording stays; validator in the production image (option A) is fine; real image/tarball/install proof comes from fdb-image.yml on the PR and a manual release.yml run before first release.
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up issue: `.github/workflows/ci.yml:76` treats `*.md` as docs-only and skips `cargo xtask ci`, so a README-only PR (e.g. dropping the `bin/wyrd-validate` row in `deploy/dist/README.md`) merges green and breaks the next code PR (pre-existing; from issue_742 sign-off).
