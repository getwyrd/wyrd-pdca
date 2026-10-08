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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.98s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #742, shipping `wyrd-validate` alongside `wyrd`: the container-free contract passes red→green, with one checker defect and release decisions still outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The binding contract distinguishes checked pipeline consistency and real staging from deferred release execution; it is falsifiable without Docker (`brief.md:49`, `brief.md:78`, `brief.md:238`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the added test produced an assertion failure naming the missing validator across all four pipeline files (`xtask/tests/dist_two_binary_layout.rs:688`; `pdca-reviewer-742-work/red.log:28`). |
| C3 Change | PASS | The production edits cover image build/copy, extraction, staging, host argv, installation/removal and both smoke workflows without adding a validator service; the prior quote and container-cleanup defects are corrected (`deploy/docker/wyrd/Dockerfile:78`, `xtask/src/dist.rs:633`, `.github/workflows/release.yml:83`, `.github/workflows/fdb-image.yml:47`). |
| C4 Verification (red→green) | PASS | Restoring the patch passed 39 relevant tests, fmt, xtask clippy and dist-check; combined changed-line coverage is 66/81 (81.5%); full CI is supported by the frozen log, with the independent run stopping at a read-only advisory-database lock (`pdca-reviewer-742-work/green.log:38`, `pdca-reviewer-742-work/coverage-summary.txt:1`, `gate-logs/C4-ci.log:3972`, `pdca-reviewer-742-work/ci.log:3357`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Repair the usage-assertion checker — it accepts suppressed or inverted greps, allowing a loader failure to pass the smoke while the layout checker reports no disagreement (`xtask/tests/dist_two_binary_layout.rs:595`; `pdca-reviewer-742-work/checker-probes.log:2`, `pdca-reviewer-742-work/checker-probes.log:16`). |
| T1 Structure | PASS | One production table feeds the Rust consumers, and one shared checker audits both expected sets; a third binary names every missing pipeline file, addressing independent spellings without a capability fallback (`xtask/src/dist.rs:77`, `xtask/tests/dist_templates.rs:492`, `xtask/tests/dist_templates.rs:506`; `pdca-reviewer-742-work/checker-probes.log:4`). |
| T2 Shape | PASS | Tests exercise distinct payloads, executable modes, missing-source refusal and extraction cleanup; workflow filters cover validator changes, the new test forbids unsafe code, and the living deployment document is updated (`xtask/tests/dist_templates.rs:625`, `xtask/tests/dist_templates.rs:687`, `xtask/tests/dist_templates.rs:709`, `xtask/tests/fdb_image.rs:345`, `xtask/tests/dist_two_binary_layout.rs:21`, `docs/design/architecture/07-deployment-view.md:42`). |
| T3 Runtime | NEEDS-HUMAN | Accept deferring real artifact proof or require a release-workflow run before sign-off — Docker/buildx, image extraction and privileged installation were not exercised; evidence rests on compiled contracts, staging dummy bytes and a host CLI invocation (`.github/workflows/release.yml:54`, `xtask/tests/dist_templates.rs:625`; `pdca-reviewer-742-work/mutants-capped.log:3`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; the substantive audit must rerun at publish and is not a missing Check gate (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Reconfirm option A and landing packaging before #743 — any early v* tag can publish the configuration-only validator, so release sequencing remains the maintainer's responsibility (`brief.md:472`, `brief.md:622`, `.github/workflows/release.yml:23`, `crates/validate/src/lib.rs:120`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the operator-facing description is fit to ship at this stage — the README promises S3 validation while the current CLI explicitly issues no requests and validates nothing (`deploy/dist/README.md:11`, `crates/validate/src/lib.rs:120`; open decision recorded in `brief.md:622`). |

The remaining implementation finding is a test-fidelity defect, not a broken submitted workflow. `is_usage_assertion` checks only that a line begins with `grep`, contains `usage:`, and names the captured file (`xtask/tests/dist_two_binary_layout.rs:595`). In a scratch fixture copied from the actual pipeline files, both of these replacements for `.github/workflows/release.yml:86` returned **zero disagreements** from the patch's own checker:

- `grep -q 'usage: wyrd-validate' /tmp/validate-usage.txt || true`
- `grep -vq 'usage: wyrd-validate' /tmp/validate-usage.txt`

With a loader-error message as input, the ordinary positive grep exited 1, but both replacements exited 0 under `sh -eu` (`pdca-reviewer-742-work/checker-probes.log:16`). Thus a binary that cannot load can again satisfy the supposedly asserted smoke. Require a positive usage assertion whose failure propagates, and add planted regressions for these cases alongside the existing deleted-grep case (`xtask/tests/dist_templates.rs:911`). This is a bounded checker repair; it does not require a general shell interpreter. No production symptom-guard smell was found.

The independent evidence establishes the declared container-free result, with explicit limits:

- **Red→green reproduced.** `cargo test --offline -p xtask --test dist_two_binary_layout` compiled and failed with the tracked production changes stashed; `git stash pop` restored them. The restored tree passed `dist_templates` (32), `dist_two_binary_layout` (2), and `fdb_image` (5). Distinct staged bytes, modes, missing-source refusal and the earlier quote regression all ran (`pdca-reviewer-742-work/green.log:5`). The real outer shell also delivered the complete current smoke script as one argument, including both invocations and all uninstall checks; shell syntax checks passed (`pdca-reviewer-742-work/checker-probes.log:13`). That capture verifies quoting only, not container execution.
- **Coverage is sensitive to test selection.** The frozen 0/81 result is accurate for its command, which runs only the deliberately text-only discriminator (`gate-logs/C4-diff-cov.log:10`). Rerunning `cargo llvm-cov test -p xtask --test dist_two_binary_layout --test dist_templates --test fdb_image` measured 66/81 changed instrumented lines, with the same 96 unscored changed lines. The uncovered lines are chiefly packaging orchestration, plus a copy-error diagnostic (`pdca-reviewer-742-work/coverage-summary.txt:1`). This does not retroactively turn the frozen advisory row green.
- **Mutation evidence is now available, but incomplete runtime coverage remains.** The frozen run failed because its copied tree could not run `git ls-files`, before testing any mutant (`gate-logs/C5-mutants.log:454`). Rerunning with `--copy-vcs true` restored the baseline. Fourteen initial mutants were unviable, mostly because denied unused-variable warnings stopped compilation; a further run with `--cap-lints true` yielded 12 caught, 4 missed, and 1 unviable. The four survivors replace `obtain_binaries`, `assemble` (two variants), or `run_dist` with success values (`pdca-reviewer-742-work/mutants-capped.log:3`). They substantiate the declared orchestration gap in T3; they do not demonstrate that today's packaging code is wrong. The unviable mutant requires a nonexistent `ShippedBinary: Default` implementation.
- **Full CI has frozen evidence and a local host caveat.** The independent run passed typos, docs lint/render, repository guards, workspace fmt/clippy/build/tests and cargo-machete, then stopped because cargo-deny could not acquire a lock under the read-only Cargo advisory cache (`pdca-reviewer-742-work/ci.log:3357`). The frozen log explicitly records successful dependency checks, conformance, statics/deployment guards and DST, ending in all checks passed (`gate-logs/C4-ci.log:3368`, `gate-logs/C4-ci.log:3972`). The frozen TiKV log shows both requested clippy checks completing (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). These are log-supported results, not independently completed full-gate reruns. The batch-review log reports zero blocking findings but supplies no individual review narratives (`gate-logs/T4-batch-review.log:10`); it does not discharge the independently reproduced checker finding above.

The runtime decision has a concrete resolution path. Run `.github/workflows/release.yml` through **workflow_dispatch** on the branch carrying this patch, observe `build distribution artifacts` and `smoke the installer in a bookworm container`, and require both installed binaries to print usage, reinstall to preserve operator configuration, and uninstall to remove both paths (`.github/workflows/release.yml:54`, `.github/workflows/release.yml:66`). This review did not dispatch a workflow or build an image. The host-built validator did run with no arguments, printing its expected usage and exiting 2 (`pdca-reviewer-742-work/validate-usage.log:1`). Its normal dependency tree includes the expected pure-Rust certificate/probe packages; that is dependency evidence, not proof of runtime compatibility inside bookworm (`pdca-reviewer-742-work/validate-tree.log:1`).

Prior art was independently checked by **all ten affected file paths**, not solely by a title search. Remote main's per-path histories and all 356 closed/merged PR file lists were inspected, including the two lists needing additional pagination. Relevant merged predecessors include #572 (distribution), #497 (FDB image), and #618 (crate-root safety); no unmerged closed PR touched these paths in the enumerated results (`pdca-reviewer-742-work/prior-art-paths.log:1`, `pdca-reviewer-742-work/prior-art-paths.log:32`). No prior-art decision remains outstanding.

All source citations above resolve in the supplied disposable `$PDCA_TARGET`; evidence paths resolve in this review directory. The patch was restored and `git apply --reverse --check patch.diff` succeeded. One base-state caveat remains: the supplied pre-fix tree lacks the #736 `WYRD_VERSION` build argument the brief expected; this patch does not remove it (`brief.md:175`, `xtask/src/dist.rs:586`). That discrepancy is not a patch-regression or compilation finding. The #738 chunk-size contract is present and retained (`xtask/tests/dist_templates.rs:212`). No judged source was repaired, and the review remains advisory.

### Advisory — adversary

# Adversarial review — issue 742 (two-binary tarball), round 6

**Bottom line: I could not refute the fix.** The red→green is real and for the right reason, the pipeline
change works against the real validator binary, and the last round's four rejection items are fixed. What
is left is two low-severity test gaps and one open maintainer question. Under the rubric's definition of done
(one deep review; do not iterate rounds chasing silence), neither gap justifies another rebuild by itself,
so I did not tag them `[impl]`. A human can promote them.

## Refutation attempts that failed (the fix held)

- **Red→green, re-run by me** (scratch copy of `$PDCA_TARGET`): green leg, 39/39 pass across `dist_two_binary_layout`, `dist_templates`, `fdb_image`. Red leg (every modified file reverted, new test kept): `every_pipeline_file_names_every_expected_binary` fails at `xtask/tests/dist_two_binary_layout.rs:688` with 8 disagreements, **all** about `wyrd-validate`. The `wyrd` rows are clean on the base tree, so the red comes from the missing binary and not from a checker that fails on anything. The test reads the real pipeline files, so it is not a copy of production.
- **The validator's behaviour matches both smokes.** I built `wyrd-validate` and ran it with no arguments: exit 2, nothing on stdout, `usage: wyrd-validate …` on stderr. So the `2>` capture in `.github/workflows/fdb-image.yml` and the `>… 2>&1` + `grep` at `.github/workflows/release.yml:83-86` both pass for the right reason. `ldd` shows only libc and libgcc.
- **No native library sneaks in through Cargo feature unification** (building both bins in one `cargo build --features fdb,etcd`, `deploy/docker/wyrd/Dockerfile:78`). `cargo tree -p wyrd-server -p wyrd-validate -F wyrd-server/fdb,wyrd-server/etcd -e normal`: the validator subtree (282 lines) has **zero** `-sys` crates, the same as when it is built alone. So the README claim "no `libfdb_c`" (`deploy/dist/README.md:15`) holds. Running the validator before the client install (`release.yml:83`, before `:89`) is a real check of it. The exact two-`--bin` `cargo check --features fdb,etcd` command also resolves and finishes cleanly.
- **`install.sh --help` range** (`deploy/dist/install.sh` `sed -n '2,17p'`): correct for the new 16-line header. It also fixes a base off-by-one that printed `set -eu` as the last help line (I ran both versions).
- **Container leak on the extraction path** (round-5 item 4): `prepare_extraction_dir` now runs before `docker create` (`xtask/src/dist.rs:633`), and the `docker cp` results are collected before `docker rm -f`. I found no early return between create and rm.
- **Quote-boundary model** (`quoted_word_end`, `xtask/tests/dist_two_binary_layout.rs:415-475`): I tried `$(echo ")")`, `${v:-"x"}`, `\\"`, and apostrophes in script comments. The model matched shell behaviour in every case.

## Findings

- **(low; not tagged; fold into any other rebuild) The release-smoke checker requires the usage `grep` but does not require that its failure is observed.** `in_command_position` rejects `|`, `||`, `&&` and `&` on the *invocation* line (`xtask/tests/dist_two_binary_layout.rs:564`), but `is_usage_assertion` (`:595`) accepts any line that starts with `grep` and mentions `usage:` and the capture file. Yet the module's own comment (and `release.yml:80-82`) says that grep is what catches a binary that cannot load (exit 127 skips the if-branch). Concrete false green, which I ran: change `release.yml:86` to `grep -q 'usage: wyrd-validate' /tmp/validate-usage.txt || true`, and update the matching search string `GREP` at `xtask/tests/dist_templates.rs:889` (the planted-drift helper `replaced()`, `:787`, insists the old line still exists). Result: 32 + 2 + 5 tests pass, while the real smoke would now pass with an unloadable validator. Without that one-line test update the suite does go red, but only through the search-string tripwire ("planted-drift needle no longer in the real file — update the case"), which tells the editor to update the test, not that the smoke got weaker. `grep -qv 'usage: …'` passes the checker too. Fix: apply the same `||`/`&&`/`|`/`&` rejection to the assertion line (and require `toks[1]` not to be `-v`/`-qv`), and add `|| true` on the grep as a planted case beside "usage assertion deleted" (`dist_templates.rs:912`). Related and smaller: the then-branch `echo …; exit 1` (`release.yml:84`) can become `:` with every test green, so "exits non-zero" (`README.md:13-14`) is no longer checked. I confirmed this one too.
- **(low; not tagged; inside the already-accepted release deferral) `assemble`'s call into the staging step is still unexercised.** The round-1 surviving mutation still survives: changing `xtask/src/dist.rs:696` to `stage_binaries(&shipped_binaries()[..1], binaries_dir, &stage)` leaves **every** `cargo test -p xtask` binary green (I ran the whole crate). The brief's "BUILT AND EXERCISED AT CHECK … the Rust consumers that read it" (`brief.md:232-235`) covers `stage_binaries` and the per-entry `docker_cp_args`, but not the loop at `dist.rs:642` or this call site. The break is loud, but only on a tag: the tarball would lack `bin/wyrd-validate`, and `install.sh:142` would fail under `set -eu`. That is after `useradd` and after `bin/wyrd` is placed, so it leaves a partial install inside the smoke container. If anyone wants it closed container-free: move everything in `assemble` before `tar` into a `pub` function over `(root, stage, binaries_dir, version, fdb_version, flavor)` and drive it over a tempdir like `stage_binaries_copies_each_binary_to_its_own_destination`.
- NEEDS-HUMAN [human] — **The operator README describes a capability the shipped binary does not have yet.** This is the round-5 open maintainer question, still unchanged; it is not new. `deploy/dist/README.md:11-12` says the validator "drives a deployment through its S3 front door exactly the way a client does". The crate's own manifest says the binary "still resolves its configuration, echoes it, and exits" (`crates/validate/Cargo.toml:3`), and I confirmed it does nothing else today. This ties to the brief's §9 "stub ships before #743" acceptance (`brief.md:455-476`). If that is accepted, the README sentence should either be future-tense or say "today it only checks its arguments". That wording choice belongs to the maintainer, not to Do.

## Gate readings a human might misread (not refutations)

- **C4-diff-cov "0.0% — 0 of 81"** is an artifact of what the gate measures, not a coverage hole: it runs only `--test dist_two_binary_layout`, which by design names no `xtask::dist` symbol (`gate-logs/C4-diff-cov.log:10`). The new `dist.rs` functions are driven by `dist_templates.rs`. The real uncovered remainder is the Docker-only part of `obtain_binaries` and the `assemble` call above.
- **C5-mutants "cargo test failed in an unmutated tree"** is an environment fault, not this patch: `scan_gitlinks_is_green_over_the_real_index` panics with "git ls-files -s -z must succeed" because the mutants scratch copy has no git index (`gate-logs/C5-mutants.log:446-462`). The same test passes in a checkout that has `.git`.
- **C4-verify "2 test(s) ran red"** (`check-gates.json:48`) overstates the log: on the red leg 2 tests *ran* and **1** failed. `the_expected_set_is_well_formed` passes on both legs (`gate-logs/C4-verify.log:15-34`). The red is still genuine; this is the harness's wording.

### Advisory — code-review

No findings on either advisory lens: no introduced correctness defects or actionable reuse, simplification, or efficiency issues found in this diff.

Reviewed the patch against the read-only target and frozen gate evidence. The full CI gate passed the staging and layout tests; the red/green check passed. Additional shell-expansion and syntax checks confirmed that both binary smoke tests and the uninstall assertions remain inside the release container script.

Evidence limits: the 0% diff-coverage run selected only the file-text test; mutation testing stopped on the unchanged Git-index baseline test before testing mutants. Real image builds and privileged installation remain deferred as specified in the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Repair the usage-assertion checker — it accepts suppressed or inverted greps, allowing a loader failure to pass the smoke while the layout checker reports no disagreement (`xtask/tests/dist_two_binary_layout.rs:595`; `pdca-reviewer-742-work/checker-probes.log:2`, `pdca-reviewer-742-work/checker-probes.log:16`).
- [ ] T3 Runtime — Accept deferring real artifact proof or require a release-workflow run before sign-off — Docker/buildx, image extraction and privileged installation were not exercised; evidence rests on compiled contracts, staging dummy bytes and a host CLI invocation (`.github/workflows/release.yml:54`, `xtask/tests/dist_templates.rs:625`; `pdca-reviewer-742-work/mutants-capped.log:3`).
- [ ] T5 Judgment — Reconfirm option A and landing packaging before #743 — any early v* tag can publish the configuration-only validator, so release sequencing remains the maintainer's responsibility (`brief.md:472`, `brief.md:622`, `.github/workflows/release.yml:23`, `crates/validate/src/lib.rs:120`).
- [ ] Validation — fitness-to-purpose — Decide whether the operator-facing description is fit to ship at this stage — the README promises S3 validation while the current CLI explicitly issues no requests and validates nothing (`deploy/dist/README.md:11`, `crates/validate/src/lib.rs:120`; open decision recorded in `brief.md:622`).
- [ ] **The operator README describes a capability the shipped binary does not have yet.** This is the round-5 open maintainer question, still unchanged; it is not new. `deploy/dist/README.md:11-12` says the validator "drives a deployment through its S3 front door exactly the way a client does". The crate's own manifest says the binary "still resolves its configuration, echoes it, and exits" (`crates/validate/Cargo.toml:3`), and I confirmed it does nothing else today. This ties to the brief's §9 "stub ships before #743" acceptance (`brief.md:455-476`). If that is accepted, the README sentence should either be future-tense or say "today it only checks its arguments". That wording choice belongs to the maintainer, not to Do.
- [ ] **Criterion 5 promises coverage the mandated test split does not provide.** `brief.md:56-67` says adding a third production-table entry makes all four file assertions demand it and identify disagreeing files. But `brief.md:94-99` requires those assertions to iterate a separate, fixed two-entry constant; the existing test only compares that constant with the production table. Add a third production entry without editing anything else: the four file checks still pass, and only the table-equality check fails. Require the existing test to also run the file checker with the production table, while preserving the independent expected set for the red test, or narrow the binding diagnostic promise.
- [ ] **The behavioral staging criterion can pass with the wrong executable installed as the validator.** `brief.md:69-75` requires two destination names and modes of `0755`, but no source-content comparison. The target currently carries one extracted path (`xtask/src/dist.rs:500`) into one copy (`xtask/src/dist.rs:559-564`); converting this to multiple sources introduces a mapping that the proposed assertions do not check. Copying the roles binary to both destinations satisfies those assertions. Require distinct dummy input contents and byte-for-byte equality between each staged destination and its corresponding source; include review of distinct extraction destinations feeding that mapping. This remains container-free.
- [ ] **The answer to the maintainer's sequencing concern overstates release enforcement.** The tracker comment says the current ordering would “ship a stub to operators” (`notes.json`, eduralph, 2026-08-16). `brief.md:432-436` dismisses that as “a release that cannot happen.” Yet the target proposal explicitly says no machine gate exists and identifies a human release-runbook step (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013-1019`); the workflow builds on a `v*` push and publishes with only a tag condition (`.github/workflows/release.yml:20-23`, `:53-54`, `:106-115`). Revise the sequencing rationale to identify the responsible human checkpoint and required committed endurance verdict, and make acceptance of landing packaging before a working tool explicit. A milestone dependency is not proof that early publication is prevented.
- [ ] T5 Judgment — Confirm option A and accept landing packaging before #743/endurance completion, with the maintainer withholding release tags — otherwise return to Plan; the current tool still issues no validation requests (brief.md:23; brief.md:472; crates/validate/src/lib.rs:118; .github/workflows/release.yml:115).
- [ ] **The invariant "no stage may disagree … without the gate saying so" does not hold for the README stage.** `.github/workflows/ci.yml:76` treats every `*.md` file as docs-only and skips the `rust` job (`cargo xtask ci`). So a PR that edits only `deploy/dist/README.md`, for example deleting the `bin/wyrd-validate` row (`:17`), merges green, and the drift then fails the next unrelated code PR. This gap predates the patch (it already affects `readme_dev_section.rs` and the existing README checks), so under the rubric's out-of-scope rule it is most likely a decline plus a follow-up issue rather than an in-PR fix. Either way, the sign-off should not read the invariant as covering README-only changes.
- [ ] T5 Judgment — Reconfirm option A and accept packaging before validator scenarios/endurance are ready — the decision is session-recorded, and a tag can currently ship an echo-only validator without a machine readiness gate (`brief.md:17`, `brief.md:471`, `crates/validate/src/lib.rs:118`, `docs/design/proposals/draft/0017-blackbox-validation-tool.md:1013`).
- [ ] T5 Judgment — Accept landing packaging before functional scenarios and retain the human no-early-tag checkpoint, or return sequencing to Plan—the exercised CLI reports that nothing was validated, while any `v*` tag can publish it (crates/validate/src/lib.rs:120; .github/workflows/release.yml:23; .github/workflows/release.yml:115; brief.md:472).
- [ ] **Addendum to the pre-declared T3 deferral: half of the "release-only" evidence will show up on this PR for free.** The brief says the image half can only be observed by a `v*` tag or a `workflow_dispatch` run (`brief.md:255-263`). `.github/workflows/fdb-image.yml:22` path-filters on `deploy/docker/wyrd/**`, which this patch edits, so that job will run on the PR. It runs `docker build --build-arg FEATURES=fdb,etcd` against the new Dockerfile (`:72-78`). A green run proves that `cargo build --release --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` and both `COPY` lines (`deploy/docker/wyrd/Dockerfile:72`, `:130-131`) work. It does not run `wyrd-validate` inside the image (its smoke is `wyrd` usage plus `fdbcli`, `:83-95`). It also does not observe extraction, the tarball, or install. At sign-off, check that job's result before accepting the deferral. Whether to add a one-line `docker run --entrypoint wyrd-validate wyrd:fdb` smoke there is a scope call; the brief did not ask for it.
- [ ] T5 Judgment — Accept landing packaging before #743 and retaining the maintainer's release checkpoint — the executable currently reports that it validates nothing, while a tag can publish it without a scenario/endurance gate (`crates/validate/src/lib.rs:118`, `.github/workflows/release.yml:115`, `pdca-reviewer-742-evidence/validator-config-run.log:15`).
- [ ] `deploy/dist/README.md:17` tells operators, in the present tense, that `wyrd-validate` "checks that your hardware and configuration are sound by driving a running deployment". On this base the binary does neither. It echoes its configuration and says so: `crates/validate/src/lib.rs:122` prints "configuration resolved; no requests were issued and nothing was validated". So a `v*` tag cut before #743 ships a README that promises a check the shipped binary does not do. The brief's Alternative D already makes "packaging lands before the tool works" a §9 sign-off item, so this is not a new decision. The point is that the README text is part of what the maintainer accepts there. If that is not wanted, the fix is one sentence, e.g. "will check … once its scenarios land (#743); in this release it resolves and prints its configuration only". That is a judgment about the stub window, not a build defect.

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
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — C5 Causal adequacy — Repair the usage-assertion checker — it accepts suppressed or inverted greps, allowing a loader failure to pass the smoke while the layout checker reports no disagreement (`xtask/tests/dist_two_binary_layout.rs:595`; `pdca-reviewer-742-work/checker-probes.log:2`, `pdca-reviewer-742-work/checker-probes.log:16`).; T3 Runtime — Accept deferring real artifact proof or require a release-workflow run before sign-off — Docker/buildx, image extraction and privileged installation were not exercised; evidence rests on compiled contracts, staging dummy bytes and a host CLI invocation (`.github/workflows/release.yml:54`, `xtask/tests/dist_templates.rs:625`; `pdca-reviewer-742-work/mutants-capped.log:3`).. 12 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
