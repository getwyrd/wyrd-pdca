# build-notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball (iteration 7)

Target: getwyrd/wyrd @ `d9c6225` (the lane worktree's base). Every `path:line` below is
against the patched worktree.

**How this round was built.** The worktree was clean, so I applied
`iteration-v6/patch.diff` (the brief's carry-forward names it) and worked on top. I read
`iteration-v6/patch.diff` and `iteration-v6/build-notes.md` — my own previous outputs — and
no review or check file from any earlier round. I also read the header of
`engine/scripts/run-diff-cov.sh`, to see why that gate reads 0%.

**What differs from iteration 6** (checked file by file against the old patch): three test
files and one comment. Production code — `xtask/src/dist.rs`, `install.sh`, the Dockerfile,
the README, the deployment doc, `fdb-image.yml` — is byte-identical to iteration 6.

| file | iteration 6 | now |
|---|---|---|
| `xtask/tests/dist_two_binary_layout.rs` | 693 lines | 1004 lines |
| `xtask/tests/dist_templates.rs` | +716 | +1197 |
| `xtask/tests/fdb_image.rs` | +65 | +124 |
| `.github/workflows/release.yml` | +21 | +30 (nine comment lines, `:66-74`) |

## The finding, and what was actually wrong

The review said the usage-assertion check "accepts suppressed or inverted greps, allowing a
loader failure to pass the smoke while the layout checker reports no disagreement".

Reproduced before changing anything: I planted `grep -q 'usage: wyrd-validate'
/tmp/validate-usage.txt || true` in a copy of the real workflow and ran the iteration-6
checker. It returned no disagreement (`pdca-builder-742-v7-red-old-checker.log`: "the
checker stayed silent … got []").

The cause is wider than one function. The old checker **recognised** lines with loose tests
("first word is `grep`, the line mentions `usage:`, the file name appears"). Every earlier
round closed one more way of fooling a recogniser, and each next review found another. So I
checked what else a real shell lets through, not only what the review named.

**Real-shell evidence.** Each variant below is the smoke script with a "binary" that cannot
be run (exit 127, the same exit a loader failure gives), run the way the step runs it. A
correct smoke must fail. All of these exit 0 — the step would be green with a validator
that cannot start:

- `pdca-builder-742-v7-shell-cases.log` (17 false greens): `grep … || true`, `grep -qv`,
  `! grep`, `grep … | cat`, `grep … &`, `if grep …; then :; fi`, the whole block inside
  `if [ -x … ]`, `set +e` above it, the shell started without `-e`, `exit 0` above it, a
  comment ending in a backslash above the grep, a line ending in `||` above it, the grep
  inside a here-document, inside an open quote, an if-body with no `exit`, the capture file
  rewritten before the grep, and absence written as `! test -e` (never fatal under
  `sh -e`).
- `pdca-builder-742-v7-shell-enders.log` (9): `exec true`, top-level `return 0`,
  `eval 'exit 0'`, `trap 'exit 0' EXIT`, a sourced file that exits, and the check inside a
  function, a `( … ) || true`, a `{ … } || true`, a `while false` loop.
- `pdca-builder-742-v7-shell-step.log` (9, with a stub `docker` that runs the script it is
  given): `exit 0` above `docker run`, `echo docker run …` (script printed, not run),
  `true || docker run …`, `true ||` or `true \` on the line above, `docker run` inside
  `if false`, a quoted `'exit' 0`, the check inside a multi-line `$( … )`, and `run: >`
  (lines folded into one, so the first comment swallows the rest).

That is 35 forms a real shell passes. Measured against the checkers
(`pdca-builder-742-v7-probe-*.log`; the 59 planted cases this round ships, run against each
checker in turn, reporting instead of asserting):

| checker | flagged | silent |
|---|---|---|
| iteration 6 | 14 | 45 (10 per-binary, all 35 shape cases) |
| iteration 6 with only the `grep` line made exact | 19 | 40 (5 per-binary, all 35 shape) |
| shipped | 59 | 0 |

So fixing only what the review named would have left 40 known cases silent.

Not every planted case was run in a shell. The step-key cases (`if:`,
`continue-on-error:`) and the docker-argument cases (`--entrypoint true`, a command put in
front of `sh`) rest on how GitHub Actions and docker behave, which I did not run. Two
planted cases are not false greens at all in a real shell and are there for the diagnostic
only: the block moved after the uninstall (fails loudly), and the grep pointed at another
file (depends on that file).

## The fix: pin the checks, and hold the step to one shape

Two parts, both in `xtask/tests/dist_two_binary_layout.rs`. The model is written out in the
doc comment at `:254-290`.

**1. Each check is pinned as an exact line, not recognised** (`release_smoke_disagreements`,
`:837`). For each shipped binary the script must hold, indentation aside:

```
if <installed> ><capture> 2>&1; then        run_block_opener, :588
  echo '<message>'; exit 1                  is_fatal_body, :605
fi
grep -q 'usage: <name>' <capture>           the statement right after fi, :866-869
…
./install.sh --uninstall                    exact, :825
test ! -e <installed>                       exact, after the uninstall, :900
```

Only the capture path and the message are free. A silenced, inverted, piped, backgrounded,
re-pointed or commented-out line is simply not the pinned line, so there is nothing left to
fool. `grep -q 'usage:' <capture>` is accepted as well, because that is what the existing
`wyrd` line is and must stay: `wyrd` prints `usage:` alone on its first line (captured in
iteration 5, `pdca-builder-742-wyrd-usage.txt`), so tightening it to `'usage: wyrd'` would
have broken the release smoke.

The three old recognisers (`in_command_position`, `captured_output`, `is_usage_assertion`)
are gone.

**2. The step is held to a straight-line, fail-fast shape**, so that "the pinned line is in
the script" means "it runs, and the step fails if it does":

- the step has no key but `name` and `run: |` (`prelude_problems`, `:480`, key rule `:492`);
- the runner-shell lines above the script are plain statements, and the script is opened by
  exactly `docker run --rm -v "$PWD/target/dist:/dist:ro" debian:bookworm sh -eu -c "`
  (`SMOKE_OPENING`, `:334`, checked at `:524`);
- every script line is a plain statement or part of the three-line run block
  (`read_shape`, `:540`);
- a plain statement (`not_plain`, `:632`) has no `if`/loop/group/function, no here-document,
  no quote or `$(…)`/`${…}` left open, no trailing `|`/`||`/`&&`, no `set +…`, and none of
  `exit exec return eval trap . source` (`:619`). Built-ins are looked for with quoting
  removed, because `'exit' 0` still exits;
- no script line, comments included, ends in a backslash (`:461-469`).

The quote-boundary scan from iteration 6 is unchanged.

**What this does not claim**, and the doc comment says so (`:285-290`): it is not a shell
interpreter. It models which statements run and whether a failing one stops the step. It
does not model what a command does and follows no expansion. An `alias`, a changed `PATH`,
a command spelled through a variable, or a job-level `if:` is out of its reach.

**The real workflow needed no change to pass.** The base file's smoke step already has this
shape; on the red leg (base `release.yml`) the checker reports only the missing validator
lines and no shape finding. The only `release.yml` edit this round is a comment above the
step stating the rule (`:66-74`).

## The decision this makes for the maintainer, with its cost

| | checker change | planted cases | what stays open |
|---|---|---|---|
| Narrow: make only the `grep` line exact | about 6 lines (the body of `is_usage_assertion`) | about 60 lines (the grep cases) | 40 of the 59 planted cases stay silent (table above) |
| Shipped: pin every check + step shape | +311 lines in the layout file | +481 lines in `dist_templates.rs` | what the limits paragraph names |

I shipped the second because the narrow one knowingly leaves forms I had just watched a
real shell pass. But it has a cost a reviewer should weigh, and it is not about line count:

- **The smoke step is now restricted to a small subset of shell.** A future edit that wants
  a loop, a second kind of `if`, or a here-document in that step fails `cargo xtask ci`
  until the edit is reshaped or the checker is extended. The failure message names the
  line, the reason, and the rule.
- **The `docker run` line is pinned whole.** Changing the image tag or the mount means
  updating one constant (`SMOKE_OPENING`). I pinned it whole rather than parse it because
  telling the image from an option's argument (`--entrypoint true`) needs docker's own
  option table. The parsed alternative is about 20 lines and still has to allow-list
  options, so it frees only the image name.
- **Some harmless lines are refused.** An unquoted `echo all done`, or a message containing
  the word `exit` or `return` outside a run block, is reported. The message says to quote
  or reword. Five harmless edits are asserted to pass (`dist_templates.rs:1419-1455`): a
  plain statement in each shell, a mid-line pipe, quoted compound words, an escaped
  double-quoted string, and a second run block for another command.

If that restriction is not wanted, the fallback is the narrow row, and the open forms
should then be accepted in writing rather than rediscovered next round.

**Not done, with the cost:** running the step through a real shell inside the test. Same
answer as iteration 6: about 45 lines plus `bash` on the gate host, and it would execute
workflow text on whatever machine runs the gate. I used a real shell by hand instead (the
three logs above).

## The same defect in the sibling test

`xtask/tests/fdb_image.rs` matched the validator's in-image smoke with
`starts_with("grep -q 'usage: wyrd-validate'")`, which also accepts `… || true`. That is
the review's finding in another file of this patch, so I fixed it the same way: the whole
8-line step is pinned as exact text (`VALIDATOR_SMOKE_STEP`, `:358`, asserted at `:343`),
including the blank line after it, so nothing can be keyed onto the step.
`a_weakened_validator_smoke_is_not_the_pinned_step` (`:414`) plants seven weakenings in the
real file. `fdb-image.yml` itself is unchanged.

This goes beyond what the carry-forward named. It is 59 lines; say so if you would rather
it were left out.

## Planted cases (all against copies of the REAL files, one edit each)

- `the_layout_checker_names_a_release_smoke_that_stops_proving_a_binary`
  (`dist_templates.rs:903`, 24 cases): 15 new, the 7 from before, plus an `exit 0` smuggled
  into the block's own `if` line and the block moved after the uninstall. Each must be
  named with `wyrd-validate`.
- `the_layout_checker_holds_the_smoke_step_to_a_straight_line_fail_fast_shape` (`:1118`, 35
  cases plus 5 harmless): each must be named with its line number and reason.
- The iteration-6 quote test (`:1467`) is unchanged except one expected phrase.

One test-helper fix: `planted_line` (`:1101`) now finds the line the edit put there, not the
first line containing the text. My new workflow comment mentions `set +e`, and the old
helper matched the comment instead of the planted line.

## Red → green, and the three refutation questions

Runner: the project's gate wrapper `./engine/xtask.sh ci` for the whole gate, and for the
quick pass the brief's own command `cargo test -p xtask --test dist_two_binary_layout`
(plus `--test dist_templates --test fdb_image`), each under `timeout`.

**Gate result on the final tree: `xtask ci: all checks passed`, exit 0, first run**
(`pdca-builder-742-ci-v7.log`; the three xtask targets ran in it: 33 + 2 + 6 tests). The
tree did not change afterwards: `patch.diff` reverse-applies cleanly on the worktree,
applies forward on a clean `d9c6225` index, and the bundle's test file is identical to the
worktree's. `./engine/xtask.sh dist --check` passes.

Neither of the two intermittent failures I reported in iteration 6
(`crates/server/tests/custodian_gc.rs` stalling, the `wyrd-gateway-s3` unit test) showed up
in this run. They are still there on the base as far as I know. If C4-ci goes red on either
name, it is not this patch.

**(a) Genuine red?** Yes, three ways.

- The brief's red leg: every tracked change reverted (`git apply -R`), the new test file
  kept, `cargo test -p xtask --test dist_two_binary_layout` → exit 101, 2 tests ran, 1
  failed, eight disagreements naming all four pipeline files for `wyrd-validate` and none
  for `wyrd` (`pdca-builder-742-v7-red.log`). Re-applied; the worktree diff was
  byte-identical to before. `engine/scripts/run-verify.sh --classify patch.diff` →
  `ADDED_TEST xtask/tests/dist_two_binary_layout.rs` + `CRATE xtask`.
- This round's defect: the new planted cases run against the iteration-6 checker fail with
  "the checker stayed silent" (`pdca-builder-742-v7-red-old-checker.log`).
- Criterion 5's diagnostic: a third entry added to `shipped_binaries()` and nothing else →
  `dist_templates` fails naming all four pipeline files for it, and the equality assertion
  says to update the local set (`pdca-builder-742-v7-third-binary.log`). Restored after.

I did not run `run-verify.sh` itself: it creates a worktree and force-moves a branch in the
host's primary checkout, outside the roots I may write to. Check runs it.

**(b) Production path?** Yes. The text checker reads the real `Dockerfile`, `install.sh`,
`README.md` and `release.yml`. The Rust half calls the real `xtask::dist` functions:
`stage_binaries` over two dummy binaries with different contents, `prepare_extraction_dir`,
`host_build_args`, `docker_cp_args`, `extracted_binary_path`, and the table. No stand-in.

**(c) Fixture includes the fault?** Yes. Every planted case copies the real files and makes
one edit; each needle is asserted present in the real file first; the unplanted copy is
asserted clean. The planted forms are the ones the real-shell logs show passing.

## The two advisory gates that have read red every round

- **C4-diff-cov 0.0% is the brief's file split meeting the gate's rule**, and I can now
  show it with the gate's own numbers. The gate measures under the added test target only
  (`run-diff-cov.sh:54`), and the brief forbids that file from naming any new production
  symbol so that it compiles on the red leg. Measured this round with `cargo llvm-cov`:
  - red-earning file alone: **0 of 81** instrumentable changed lines (81 of 177 changed) —
    exactly what the gate printed;
  - both dist test targets: **66 of 81 (81.5%)**. The 15 misses are
    `xtask/src/dist.rs:443, 557-558, 562, 566, 633, 642-645, 647, 662, 696, 754-755`: one
    chmod error path, and the lines that need `docker`, `cargo` or `tar` to run.

  Nothing in the patch can change the gate's reading without breaking the brief's rule.
- **C5-mutants "cargo test failed in an unmutated tree" predates this patch.** As diagnosed
  in iteration 5: cargo-mutants copies the tree without `.git`, and
  `xtask/tests/repo_hygiene_guards.rs` needs `git ls-files`, so the baseline fails before
  any mutant runs. Not re-run. It is a gate-setup matter, not something a patch to the
  target can fix.

## Hand mutations of this round's code

cargo-mutants does not mutate test files and its baseline does not run here, so I broke the
checker by hand, one rule at a time, ran the three xtask targets, and restored the file
(`pdca-builder-742-v7-hand-mutations-final.log`, script beside it). **31 of 31 were caught**
on the final tree.

Two things the first pass taught me, both fixed before the final sweep:

- Two mutations survived: dropping the "capture must be a plain path" check, and not
  reporting a malformed block. Each exposed a real false green the cases did not cover
  (`if X >/tmp/f; exit 0 # 2>&1; then`, and a block whose body is `exit 0`). I added a
  planted case for each and both mutations are now caught.
- Seven mutations first failed to compile (the workspace denies unused code). I rewrote
  them in a form that compiles; the final log has no such entries.

The log records which test failed, not which assertion inside it.

## Still open — maintainer questions the iteration-5 sign-off said NOT to assume

Nothing in this patch decides these.

1. **Option A, and packaging before #743 with no early `v*` tag.** The patch builds the
   image with both binaries and extracts both. If option A is not confirmed, the brief
   says reject back to Plan. Until #743, per the brief, `wyrd-validate` only echoes its
   configuration and exits, and `release.yml` publishes on any `v*` tag, so the one thing
   keeping a stub off operators' hosts is not cutting a tag.
2. **`deploy/dist/README.md:11-12` says in the present tense that the validator "drives a
   deployment through its S3 front door exactly the way a client does".** It does not yet.
   Left as it was.
3. **Whether a manual `release.yml` `workflow_dispatch` run is required as real-artifact
   proof.** Not run, and not mine to run.

## Verification posture — what was NOT observed in this cycle

**Deferred, and to whom:** that a real tarball CONTAINS both binaries, and that
`install.sh` PLACES and REMOVES both on a real host. Nothing in `cargo xtask ci` can build
a tarball (needs Docker and a network), and `install.sh` refuses to run as non-root, writes
`/etc/wyrd`, creates a user and installs units. What stands in: the text contract over
every pipeline file, the real staging step over dummy binaries, and the release smoke step,
which runs only on a `v*` tag or a `workflow_dispatch`. The brief asks for this to be an
explicit acceptance at §9: the tracker's literal definition of done is not observed here.

Also not observed: any Docker build, the `docker cp` loop, `tar`, and the `fdb-image.yml`
smoke of the validator inside the image. That last one should run on the draft PR, since
the PR touches `deploy/docker/wyrd/**`; it would be the first real run of `wyrd-validate`
inside the built image.

**The release smoke step has still never run for real.** What I have is narrower: the
step's `run:` block, run under `bash -e` with a stub `docker` that records the script it is
handed, delivers all 35 lines ending at `test -d /etc/wyrd`, with the seven pinned lines in
it, and that script parses under `sh -n` and `dash -n` (`pdca-builder-742-v7-real-step.log`).
That shows the right script reaches the container, not that it passes inside one.

Carried from the brief: the image now holds a tool that deletes objects. Bounds: it is not
the `ENTRYPOINT`; it refuses to run without `--endpoint` and the other required flags (run
in iteration 5: exit 2 and usage; not re-run); run-id-scoped keys are proposal 0017 §15's
stated safety rule.

## Things I checked and things I could not

- `cargo fmt -p xtask -- --check` clean; `cargo clippy -p xtask --all-targets` clean;
  `typos` clean over every touched file.
- **`shellcheck` and `actionlint` are not installed on this host.** `install.sh` is
  unchanged since iteration 5 and the release workflow shellchecks it. The two workflow
  files were not linted by a tool here; `release.yml` changed only in a YAML comment this
  round.
- No pre-commit hook configuration in the target repo (no `.pre-commit-config.yaml`,
  `.githooks`, `lefthook.yml`, `.husky`, no `core.hooksPath`); its commit bar is
  `cargo xtask ci`, which passed.
- Rubric self-review: the workflow path filters were re-read (`fdb-image.yml:18-47`,
  `Cargo.lock` and `crates/validate/**` both listed); the new test crate root carries
  `#![forbid(unsafe_code)]`; the fixtures read no wall clock; the docs-currency edit from
  earlier rounds (`docs/design/architecture/07-deployment-view.md`) is unchanged.

## Scratch

Under `$PDCA_SCRATCH`, prefix `pdca-builder-742-v7`: the three real-shell logs and their
scripts (`-v7shell/`), `-v7-real-step.log`, `-v7-red-old-checker.log`, `-v7-red.log`,
`-v7-third-binary.log`, the hand-mutation scripts and logs, the two lcov files and
`pdca-builder-742-ci-v7.log`. Earlier rounds' files are still there. I removed nothing: the
harness owns that directory, and the instructions I was given disagree on whether the
builder should delete from it, so I took the option that cannot lose evidence. The coverage
run also left `target/llvm-cov-target/` inside the worktree (git-ignored).
