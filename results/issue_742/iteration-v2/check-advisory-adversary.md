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
