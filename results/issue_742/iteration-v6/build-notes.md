# build-notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball (iteration 6)

Target: getwyrd/wyrd @ `d9c6225` (the lane worktree's base). Every `path:line` below is
against the patched worktree.

**How this round was built.** The sign-off said "fix in place, same plan". The worktree was
clean, so I applied `iteration-v5/patch.diff` (the brief's carry-forward names it) and made
the four fixes on top. I read `iteration-v5/patch.diff` and `iteration-v5/build-notes.md` —
my own previous outputs — and nothing else from the earlier rounds (no review or check
files). Everything outside the four fixes is unchanged from iteration 5.

## The four rejected items, each fixed

### 1. `release.yml`: the quoted phrase that cut the smoke script short

The smoke step runs `docker run … sh -eu -c "<script>"`. The whole script is one
double-quoted word of the step's shell, and inside a quoted word `#` is not a comment. My
iteration-5 comment `would read as "refused correctly"` therefore closed the script at the
first `"`; the space after `refused` ended the word, and everything after it became spare
arguments.

Proved with a real shell, not by reading (`pdca-builder-742-real-shell.log`): I took the
step's `run:` block from the file, stubbed `docker` with a shell function that records the
argument after `-c`, and ran it under `bash -e`.

| file | script the container gets | lines naming a binary check | step exit |
|---|---|---|---|
| iteration 5 (rejected) | 12 lines, ends at `…would read as refused` | 0 | 0 |
| this patch | 35 lines, ends at `test -d /etc/wyrd` | 7 | 0 |

So the rejected step really was green while running none of its checks. The fixed script
also parses under `sh -n` and `dash -n` (same log).

The fix: the comment has no quotes now (`.github/workflows/release.yml:78-82`), and a YAML
comment above the step (`:61-65`, outside the script, where quoting is free) says why
double quotes and backticks must stay out of the script. The `fdb-image.yml` comment had
the same phrase; it sat in a YAML comment and was harmless, but it is reworded too
(`.github/workflows/fdb-image.yml:100-101`).

### 2. The layout checker now delimits the script the way the shell does

`smoke_script` (`xtask/tests/dist_two_binary_layout.rs:277`) finds the line ending in
`-c "`, then `quoted_word_end` (`:415`) scans forward for the `"` that closes that word.
The per-binary checks (`release_smoke_disagreements`, `:483`) read only lines inside that
boundary, minus the script's own comment lines.

What the scan models: backslash escapes; single quotes outside double quotes; `$(…)` and
`${…}` nesting (the real script has `$(basename "$tarball")`, whose quotes belong to the
substitution). What it refuses instead of modelling: a backtick (the runner's shell would
run it as a command substitution, `:311`), a script that never closes (`:330`), an opener
whose quote is not really an opener.

**One deliberate difference from the shell, as the sign-off asked.** A real shell only
truncates when there is a space inside the stray quotes; `"quoted"` as one word just loses
its quote marks and the script carries on (`pdca-builder-742-quote-cases.log`, case
`oneword`). The sign-off's rule is "an unescaped `"` before the closing line must count as
the end of the script", and that is what the checker does: the first unescaped `"` ends
the script for the checker, and anything non-blank after it in the step is a disagreement
(`:364`). Nobody should have to count spaces to know whether a smoke still runs. The
message says both outcomes plainly rather than claiming the shell always truncates.

A boundary problem is reported once per file (`:83-84`), and each binary's missing checks
are reported after it. On the rejected file the red-earning test fails with
(`pdca-builder-742-red-rejected-file.log`):

- `release.yml: line 82: an unescaped " closes the smoke step's sh -c "…" script here, but
  the rest of that line and 23 more non-blank line(s) … come after it`
- `never runs /usr/local/bin/wyrd in command position …`, and the same for `wyrd-validate`
- `never runs ./install.sh --uninstall, so nothing shows /usr/local/bin/wyrd is gone …`,
  and the same for `wyrd-validate`

The planted regressions are in `xtask/tests/dist_templates.rs:986`
(`the_layout_checker_ends_the_smoke_script_at_an_unescaped_quote`), each one edit to a
copy of the REAL workflow:

1. `:1007` the shipped defect itself (quoted phrase in the comment before the validator
   smoke): the close is named with its line number, and both binaries' invocation and
   uninstall checks are reported as never run.
2. `:1047` the sign-off's wording — a comment with `"quoted"` words before the validator
   invocation: named, and names `wyrd-validate`.
3. `:1063` a quote after every binary check: still named (the smoke is truncated), and it
   is the only disagreement, so the checks before it still count.
4. `:1084` text after the real closing quote (`" || true`): named.
5. `:1096` a backtick in a script comment: named.
6. `:1119` the closing quote deleted: named.
7. `:1127` the same phrase with escaped quotes: NO disagreement. The model follows the
   shell; it does not just ban a character.

The unplanted file is asserted clean with the `$(basename "$tarball")` quotes in it
(`:997-1005`).

**Rejected alternative, with its cost: run the step through a real shell inside the test.**
That would remove the gap between "my model of quoting" and "what the shell does" instead
of narrowing it. Cost: about 45 lines (pull the `run:` block out of the YAML and dedent it,
a `docker` stub, a fixture directory with a fake tarball and the Dockerfile because the
block runs `ls` and `sed` under `set -eu`), plus `bash` on the gate host. I did not ship
it for a reason that is not about size: the test would EXECUTE workflow text on whatever
machine runs `cargo xtask ci`. Today that text is `ls`, `sed` and `basename`; the next edit
to the step could be anything. These test files are "file read + substring/parse" by
stated design (`xtask/tests/dist_templates.rs:5-9`, `xtask/tests/fdb_image.rs:13-17`).
I used the real shell once, by hand, as the evidence above. If the maintainer would
rather have it as a standing test, it is a small follow-up.

### 3. `fdb-image.yml` path filter

`crates/validate/**` is now a `pull_request.paths` entry (`.github/workflows/fdb-image.yml:47`),
with a comment saying why a featureless crate is listed: this job is the only one that
runs the validator inside the built image, and its smoke greps that crate's usage line.
This reverses my iteration-5 choice not to widen the filter.

Pinned in `xtask/tests/fdb_image.rs:323-337`. The old check was `wf.contains(entry)`, which
a comment naming the glob would satisfy, so the list is now parsed
(`pull_request_path_filters`, `:358`) and every entry — old and new — must be a real list
item. `a_commented_out_path_filter_is_not_a_filter` (`:373`) comments the new entry out of
the real file and shows the parser stops counting it. The same test also asserts the
validator smoke the entry exists for is still there (`:340-354`).

### 4. `dist.rs`: no container leak on a failed cleanup

The clean-and-create of `target/dist/extracted` moved into `prepare_extraction_dir`
(`xtask/src/dist.rs:416`), which `obtain_binaries` calls BEFORE `docker create` (`:633`,
create at `:634-637`). Between create and `docker rm -f` (`:649`) there is now only the
`docker cp` loop, whose results are collected and returned after the `rm` (`:642-650`).

`prepare_extraction_dir` is `pub` and tested without a container
(`xtask/tests/dist_templates.rs:709` — stale binaries from an earlier run are gone, a
second call and a first-ever call both work; `:745` — a path it cannot make a directory is
an error naming it).

Not done, with the cost: a `Drop` guard around the container id (about 10 lines) would
make removal automatic on every exit path, including a future `?` someone adds. I left it
out because nothing can exercise it without Docker, so it would be 10 untested lines in
place of a comment at `:639-640` that states the rule. Say so if you want it.

**What is not proven here:** that the call order is right at run time. No test can observe
"prepare ran before create" without Docker. It is a five-line read at `:631-637`.

## Red → green, and the three refutation questions

Runner: the project's gate wrapper `./engine/xtask.sh ci` for the whole gate, and for the
quick pass the brief's own command, `cargo test -p xtask --test dist_two_binary_layout`
(plus `--test dist_templates --test fdb_image`), each under `timeout`.

**Gate result on the final tree: `xtask ci: all checks passed`, exit 0**
(`pdca-builder-742-ci-v6c.log`; the three xtask targets ran in it: 32 + 2 + 5 tests). The
bundle's `patch.diff` is byte-identical to the worktree diff that run tested, and the
bundle's test file to the worktree's. `./engine/xtask.sh dist --check` also passes.

**It took three runs to get that pass, and the two failures matter for Check.** Neither is
in code this patch touches (the diff is `xtask/`, `.github/`, `deploy/`, `docs/` only, and
no crate `include_str!`s any of those files), but C4-ci can hit both:

1. Run 1 (`ci-v6.log`) stalled in `crates/server/tests/custodian_gc.rs`. The test binary
   sat at 0% CPU for 12 minutes with every test thread blocked in a futex wait (a full
   gate run takes about 2 minutes here). I stopped that one process, so the run ended
   with exit 101. This was not a pass and I do not count it as one. The same target run
   alone straight after: 10 passed in 0.17 s (`custodian-gc-alone.log`). I did not find
   the cause. The host was not busy: the 15-minute load average right after was 0.7 on 32
   cores.
2. Run 2 (`ci-v6b.log`) failed `wyrd-gateway-s3`'s unit test
   `a_bodyless_response_is_recorded_complete_not_aborted` (`crates/gateway-s3/src/lib.rs:4259`,
   the captured log row came back empty). Re-running that unchanged suite eight times:
   7 passes, 1 failure on the same test (`gateway-s3-repeat.log`). It is intermittent on
   this base.

Both look like tests that are sensitive to timing. They are outside this slice; I am
reporting them, not fixing them. If C4-ci goes red at Check on either name, it is this.

**(a) Genuine red?** Yes, shown two ways.

- The brief's red leg: reverted every tracked change (`git apply -R`), kept the new test
  file, ran `cargo test -p xtask --test dist_two_binary_layout` → exit 101, 2 tests ran, 1
  failed, eight disagreements naming all four pipeline files for `wyrd-validate` and none
  for `wyrd` (`pdca-builder-742-red-v6.log`). Re-applied; the worktree diff is
  byte-identical to before. `engine/scripts/run-verify.sh --classify patch.diff` →
  `ADDED_TEST xtask/tests/dist_two_binary_layout.rs` + `CRATE xtask`.
- This round's defect: put the iteration-5 `release.yml` back and nothing else → the same
  test fails naming line 82 and every check after it
  (`pdca-builder-742-red-rejected-file.log`). The sign-off says it passed on that file in
  iteration 5.

I did not run `run-verify.sh` itself: it creates a worktree and force-moves a branch in
the host's primary checkout, which is outside the roots I may write to. Check runs it.

**(b) Production path?** Yes. The text checker reads the real `Dockerfile`, `install.sh`,
`README.md` and `release.yml` from the workspace root. The Rust half calls the real
`xtask::dist` functions: `stage_binaries` over two dummy binaries with different contents,
`prepare_extraction_dir` over a real temp directory, `host_build_args`, `docker_cp_args`,
`extracted_binary_path`, and the table itself. No stand-in and no mock.

**(c) Fixture includes the fault?** Yes. Every planted case copies the four real files and
makes exactly one edit; each needle is asserted present in the real file first, so a later
edit cannot turn a case into a no-op; the unplanted copy is asserted clean. Case 1 above is
the actual rejected text.

## Still open — maintainer questions the sign-off said NOT to assume an answer to

Nothing in this patch decides these. They are sign-off items.

1. **Option A, and packaging before #743 with no early `v*` tag.** The patch builds the
   image with both binaries and extracts both from it. If option A is not confirmed, the
   brief says reject back to Plan; this patch cannot be turned into option B. Until #743,
   per the brief, `wyrd-validate` only echoes its configuration and exits, and
   `release.yml` publishes on any `v*` tag, so the one thing keeping a stub off operators'
   hosts is not cutting a tag.
2. **`deploy/dist/README.md:11-12` says, in the present tense, that the validator "drives
   a deployment through its S3 front door exactly the way a client does".** It does not do
   that yet. I left the wording as it was, because the question is open; changing it either
   way would be assuming an answer.
3. **Whether a manual `release.yml` `workflow_dispatch` run is required as real-artifact
   proof.** Not run, and not mine to run. See the next section for what is and is not
   observed without it.

## Verification posture — what was NOT observed in this cycle

**Deferred, and to whom:** that a real tarball CONTAINS both binaries, and that
`install.sh` PLACES and REMOVES both on a real host. Nothing in `cargo xtask ci` can build
a tarball (needs Docker and a network), and `install.sh` refuses to run as non-root, writes
`/etc/wyrd`, creates a user and installs units, so no test on the gate host can run it.
What stands in: the text contract over every pipeline file, the real staging step run over
dummy binaries, and the release smoke step, which only runs on a `v*` tag or a
`workflow_dispatch`. The brief asked for this to be stated as an acceptance at §9, not
assumed: the tracker's literal definition of done is not observed here.

Also not observed: any Docker build, the `docker cp` extraction loop, `tar`, and the
`fdb-image.yml` smoke of the validator inside the image. That last one should run on the
draft PR, since the PR touches `deploy/docker/wyrd/**` and the workflow file itself; it
would be the first real observation of `wyrd-validate` inside the built image.

The release smoke step has still never run for real. This round's evidence for it is the
stubbed-`docker` shell run above: it shows the right script reaches the container, not that
the script passes inside one.

Other sign-off items carried from the brief: the image now carries a tool that deletes
objects. Bounds: it is not the `ENTRYPOINT` (unchanged in the Dockerfile); it refuses to
run without `--endpoint` and the other required flags (I ran it in iteration 5: exit 2,
usage; not re-run this round); run-id-scoped keys are proposal 0017 §15's stated safety
rule.

## The two advisory gates that have read red every round

Unchanged by this round, so in short:

- **C4-diff-cov 0.0% comes from the brief's file split plus the gate's rule.** The gate runs
  only the added test target (`-p xtask --test dist_two_binary_layout`), and the brief
  forbids that file from naming any new production symbol so that it compiles on the red
  leg. Under that one target no changed `dist.rs` line can execute. Measured with both dist
  test targets in iteration 5: 57 of 77 changed lines (74%); the misses were all lines that
  need `docker` or `cargo` to run. This round moves the directory cleanup out of that
  untested set into `prepare_extraction_dir`, which is tested. I did not re-measure.
- **C5-mutants "cargo test failed in an unmutated tree" predates this patch.** As diagnosed
  in iteration 5: cargo-mutants copies the tree without `.git`, and
  `xtask/tests/repo_hygiene_guards.rs` needs `git ls-files`, so the baseline fails before
  any mutant runs. Not re-run this round. Out of this slice's scope.

## Things I checked and things I could not

- `cargo fmt -p xtask -- --check` clean; `cargo clippy -p xtask --all-targets` clean under
  the workspace's deny-warnings; `typos` clean over every touched file.
- **`shellcheck` and `actionlint` are not installed on this host.** `install.sh` is
  unchanged since iteration 5 and the release workflow shellchecks it; the two workflow
  files were not linted by a tool here.
- I found no pre-commit hook configuration in the target repo (no `.pre-commit-config.yaml`,
  `.githooks`, `lefthook.yml`, `.husky`, and no `core.hooksPath`); its commit bar is
  `cargo xtask ci`, which passed as above.

## Hand mutations of this round's code

The C5 gate's baseline does not run here (the iteration-5 diagnosis above), and
cargo-mutants does not mutate test files anyway, so I broke the new code by hand, one
change at a time, ran the three xtask targets, and restored the file
(`pdca-builder-742-hand-mutations.log`). All seven made a test fail. The log records which
tests failed, not which assertion inside them.

| change | tests that failed |
|---|---|
| scanner ignores backslash escapes | `the_layout_checker_ends_the_smoke_script_at_an_unescaped_quote` |
| scanner does not nest `$(…)` | that test, both real-file layout tests, and the clean-fixture test |
| early close never reported | `the_layout_checker_ends_the_smoke_script_at_an_unescaped_quote` |
| per-binary checks read past the close | same |
| backtick not refused | same |
| `prepare_extraction_dir` keeps stale files | `prepare_extraction_dir_discards_what_an_earlier_run_left` |
| `fdb-image.yml` loses the `crates/validate/**` entry, comment kept | `workflow_exists_resolves_and_filters_the_fdb_surface`, `a_commented_out_path_filter_is_not_a_filter` |

Two of these first failed to compile instead of failing a test (the workspace denies dead
code, and deleting the line left an enum variant unused); I redid them in a form that
compiles, and those are the results in the table.

## Scratch

Under `$PDCA_SCRATCH`, prefix `pdca-builder-742-`: this round's logs are `ci-v6.log`
(stalled run), `ci-v6b.log` (flaky failure), `ci-v6c.log` (the pass),
`custodian-gc-alone.log`, `gateway-s3-repeat.log`, `red-v6.log`, `red-rejected-file.log`,
`real-shell.log`, `quote-cases.log`, `hand-mutations.log` and `.py`, the `shell/` directory
with the stub scripts, and `tracked.diff`. Earlier rounds' logs are still there. I removed
nothing: the harness owns that directory, and the instructions I was given disagree on
whether the builder should delete from it, so I took the option that cannot lose evidence.
