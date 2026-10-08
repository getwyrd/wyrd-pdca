# build-notes — issue 742 / dist-ship-wyrd-validate-two-binary-tarball (iteration 5)

Target: getwyrd/wyrd @ `d9c6225` (the lane worktree's base: `main` + the batch's folded
waves). The brief's note that `dist.rs` would already carry #736's `--build-arg WYRD_VERSION`
did not hold on this base — that arg is not in `obtain_binary` here — so nothing of the kind
was touched or reverted. Every `path:line` below is against the patched worktree.

## What the patch does, in one paragraph

The shipped-binary set becomes data: `xtask::dist::shipped_binaries()` (`xtask/src/dist.rs:77`)
returns `[{image_path: /usr/local/bin/wyrd, dest: bin/wyrd}, {image_path:
/usr/local/bin/wyrd-validate, dest: bin/wyrd-validate}]`, and every Rust consumer reads it:
the `--host` argv (`host_build_args`, `:385`), the extraction list (`extracted_binary_path`
`:377`, `docker_cp_args` `:401`), and the staging step (`stage_binaries`, `:415`, a `pub`
callable that takes the table plus a source directory). `obtain_binary` became
`obtain_binaries` (`:542`): one `docker create`, one `docker cp` per entry, `docker rm -f`
regardless of the cp outcome (`:630-638`), returning the extraction DIRECTORY
(`target/dist/extracted/`, one file per binary named after it — the same `<dir>/<name>`
shape as the host build's `target/release/`, so `stage_binaries` reads one layout whichever
vehicle produced them; `assemble` calls it at `:684`, `run_dist` threads it at `:742-743`).
The four non-Rust stages keep their own literal spelling of the set — Dockerfile `:78` and
`:137`, `install.sh:118` and `:142`, `release.yml:77-80` and `:96`, the README — and the new
test `xtask/tests/dist_two_binary_layout.rs` pins each of them to it, with the SAME checker
run a second time over the production table from `xtask/tests/dist_templates.rs:506`.

## Why this shape (and what was ruled out)

- **Table rows are `(image_path, dest)` with a derived `name()`** (`dist.rs:67`) rather
  than three fields: the brief's red file holds `const EXPECTED_BINARIES: [(&str, &str); 2]`
  and `dist_templates.rs:492` pins it EQUAL to the production table, so the rows are the
  same two-string shape. The shape test pins `image_path` ends with `/<name>` and `dest ==
  bin/<name>` for every row, plus pairwise-distinct image paths, destinations, names and
  host extraction paths (`dist_templates.rs:520-575`), so the spellings cannot disagree.
- **An extraction directory, not per-binary `.extracted` files**: the brief asks for a
  staging callable that "takes the table plus a source directory". Extracting each binary
  onto `target/dist/extracted/<name>` makes the image path and the host build path the same
  shape. The directory is removed and recreated per run (`dist.rs:623-629`) so a stale file
  from an earlier run can never stand in for a binary this run failed to extract.
- **`--host` builds both** (brief scope (h)): `host_build_args` emits one `cargo build
  --release --locked --bin wyrd --bin wyrd-validate --features <f>`, asserted at
  `dist_templates.rs:577-595`. Verified cargo accepts that shape on this base: `cargo check
  --locked --bin wyrd --bin wyrd-validate --features fdb,etcd` exits 0
  (`pdca-builder-742-check-two-bins.log` in `$PDCA_SCRATCH`).
- **Installer: two literal lines, not a loop.** A `for bin in $BINARIES` loop would be DRYer,
  but the invariant is "each stage keeps its literal spelling and the gate checks it";
  literal lines keep the checker a token match and keep `shellcheck` trivially happy.
  `ROLES` is untouched (brief §4) and the checker refuses a binary listed in it
  (`dist_two_binary_layout.rs:167-180`).
- **Rejected: a `dist --print-binaries` CLI flag so the red file could reach the table.** It
  would have let the red-earning test execute production code (and lifted the structural 0%
  on C4-diff-cov — see below) at the cost of a new CLI surface that exists only for a test
  plus a stdout-parsing assertion: ~25 lines (arg parse + print + test parse) for an
  advisory metric. The brief puts new-API assertions in `dist_templates.rs`; they are there.

## Carry-forward items, each addressed

- **Iteration 1 — comments and order in the release checker.** The checker skips comment
  lines (`dist_two_binary_layout.rs:270`), requires the binary in COMMAND position with the
  exit status observable — leading `if`/`!` only; a `|`, `||`, `&&` or trailing `&`
  disqualifies (`in_command_position`, `:331-348`) — and requires the absence check AFTER
  the `./install.sh --uninstall` line (`:316-324`). The installer split is positional too:
  the uninstall site must sit between `if [ "$UNINSTALL" = 1 ]` and its `exit 0`, the
  install site after that `exit 0` (`:181-239`), so "anything above the install banner" no
  longer counts. The extraction list is a pure function (`docker_cp_args`) asserted
  container-free with distinct destinations (`dist_templates.rs:597-623`).
- **Iteration 2 — the `units` label column.** Untouched; the new `tool` row is padded to the
  same source column as its neighbours (`install.sh:202`: 2 + 9 + 49 columns like the rest).
- **Iteration 3 — invocation pinned but not the assertion on its result.** The checker finds
  the invocation's captured-output file (`captured_output`, `:350-370`: the `>` target) and
  requires a `grep … 'usage:' <that same file>` line AFTER the invocation and BEFORE
  `--uninstall` (`:300-314`). A pipeline, a background job and `|| true` all fail
  `in_command_position`. `wyrd`'s own usage text is multi-line (`usage:` alone, then `  wyrd
  put …` — ran `target/debug/wyrd`, exit 2), so the rule is "a usage grep over the file the
  invocation captured", not `usage: <name>` on one line; the validator's grep is the tighter
  `usage: wyrd-validate`, pinned by `crates/validate/tests/cli_surface.rs:108`. Every mutation
  named in that round is a planted-drift case now (`dist_templates.rs:830-904`): grep
  deleted, block commented, absence check moved before uninstall, piped, backgrounded,
  argument position, `|| true` — seven cases, each must yield a disagreement naming the
  workflow and the binary.
- **Iteration 4 — "the real image build was not exercised."** Still true, by the brief's
  instruction ("Do MUST NOT attempt one"). What changed: the PR-time image workflow
  `.github/workflows/fdb-image.yml:92-104` now runs `wyrd-validate` INSIDE the built image
  (`docker run --rm --entrypoint wyrd-validate wyrd:fdb` → usage, non-zero). That workflow
  fires on `deploy/docker/wyrd/**`, so the draft PR's CI produces an in-image observation
  the human can read at sign-off, before any `v*` tag. See "Sign-off items".

## Red → green, and the three refutation questions

Runner: the project's gate wrapper `./engine/xtask.sh ci` (the full `cargo xtask ci`) —
`xtask ci: all checks passed` (`pdca-builder-742-ci.log`). The quick per-target pass used
the same `cargo test -p xtask --test dist_two_binary_layout --test dist_templates` that
C4-verify computes (29 + 2 tests green); `cargo fmt -p xtask` applied and `cargo clippy -p
xtask --all-targets` clean under the workspace's `warnings = deny` / `clippy::all = deny`;
`typos` clean over every touched file; `cargo xtask dist --check` still passes.

**(a) Genuine red?** Yes. Reverted every tracked change (`git checkout -- .`), kept the
untracked test, ran `cargo test -p xtask --test dist_two_binary_layout`: exit 101, 1 failed,
eight disagreements naming all four pipeline files for `wyrd-validate` and none for `wyrd`
(`pdca-builder-742-red.log`): `Dockerfile: no RUN cargo build … --bin wyrd-validate`;
`Dockerfile: no COPY --from=build …/wyrd-validate /usr/local/bin/wyrd-validate`;
`install.sh: the --uninstall path (lines 56-124) never runs rm -f "$BINDIR/wyrd-validate"`;
`install.sh: the install path (after line 124) never runs install -m 0755 …`; `release.yml:
never runs /usr/local/bin/wyrd-validate in command position …`; `release.yml: no test ! -e
/usr/local/bin/wyrd-validate after --uninstall`; `README.md: never names bin/wyrd-validate`;
`README.md: no line describes wyrd-validate on its own`. Then re-applied the diff;
`git status` matches the patch. `engine/scripts/run-verify.sh --classify patch.diff` →
`ADDED_TEST xtask/tests/dist_two_binary_layout.rs` + `CRATE xtask`, as the brief predicted.

**(b) Production path?** Yes, in two layers. The text checker reads the REAL
`deploy/docker/wyrd/Dockerfile`, `deploy/dist/install.sh`, `deploy/dist/README.md` and
`.github/workflows/release.yml` from the workspace root. The non-lexical half drives the
real `xtask::dist` functions: `stage_binaries` over a tempdir with `wyrd-binary payload` vs
`wyrd-validate-binary payload`, asserting each `bin/<name>` is byte-identical to its OWN
source and 0755, and that `bin/` holds exactly the table's names
(`dist_templates.rs:625-685`); a source dir missing one binary → `Err` naming it
(`:687-709`). No stand-in, no mock.

**(c) Fixture includes the fault?** Yes. The planted-drift cases copy the REAL four files
into a tempdir and apply exactly one edit each (`with_planted_drift`, `:711-728`); every
needle is asserted present in the real file first (`replaced`, `:730-738`) so a later edit
cannot make a case vacuous, and the unplanted copy is asserted clean (`:751-755`). Seventeen
drifts across the four files each produce a disagreement naming the file and the binary
(`:757-917`).

## The two advisory gates that read red in every previous round — root causes

**C4-diff-cov 0.0% is structural, not a reach gap.** `engine/scripts/run-diff-cov.sh:759-764`
builds exactly one run spec for the test-owning crate — `-p xtask --test
dist_two_binary_layout` — and marks the package seen so its suite never joins. The brief
requires that file to "not name any symbol this patch introduces" (so it compiles on the red
leg), so under that one target zero changed `dist.rs` lines can execute. That is a property
of the brief's split and the gate's rule together, the same in every iteration. My own
measurement with both dist targets — `cargo llvm-cov -p xtask --test dist_templates --test
dist_two_binary_layout`, scored with the gate's own hook (`run-diff-cov.sh --score`) —
**57 of 77 instrumentable changed lines executed (74%)**. All 20 misses are lines that need
`docker` or `cargo` to run: `obtain_binaries`' host branch and extraction loop
(`dist.rs:542-551`, `:623-635`), the `stage_binaries` call inside `assemble` (`:684`), and
`run_dist`'s two calls (`:742-743`) — exactly the deferred half. (`pdca-builder-742-
lcov.info`, `-lines.txt` in scratch.)

**C5-mutants `ERROR cargo test failed in an unmutated tree` is pre-existing and independent
of this patch.** Ran the gate script itself (`scripts/mutants-in-diff`,
`pdca-builder-742-mutants.log`): cargo-mutants copies the tree WITHOUT `.git` (its default),
and `xtask/tests/repo_hygiene_guards.rs:137` (`scan_gitlinks_is_green_over_the_real_index`)
asserts `git ls-files -s -z must succeed` → the baseline `cargo test -p xtask` fails before
any mutant runs. Any patch touching `xtask/src` hits this. Out of this slice's scope (rubric:
decline-with-issue-reference); the one-line fix is `copy_vcs = true` in
`.cargo/mutants.toml`, or that one test tolerating a non-git tree. A second, smaller cause
sits behind it: the workspace's `warnings = deny` makes every "replace body with a constant"
mutant fail to compile on its now-unused parameters (13 of 16 `unviable` without
`--cap-lints`), so the gate would under-report even with the baseline fixed.

**Mutation run over this diff (my evidence, not the gate's):** `cargo mutants --in-diff
patch.diff --no-shuffle --cap-lints true -- --test dist_templates --test
dist_two_binary_layout` (`pdca-builder-742-mutants-dist2.log`, `-mutants-out2/`): 16 mutants,
**11 caught, 4 missed, 1 unviable**. Caught: `ShippedBinary::name` → `""`/`"xyzzy"`,
`shipped_binaries` → `vec![]`, `extracted_binary_path` → `Default`, `host_build_args` → each of
three stubs, `docker_cp_args` → each of three stubs, `stage_binaries` → `Ok(())`. Missed:
`obtain_binaries` → `Ok(Default)`, `assemble` → `Ok(…)` ×2, `run_dist` → `Ok(())` — the four
runner bodies that drive docker/cargo, unreachable container-free by the brief's own
posture. Unviable: `shipped_binaries` → `vec![Default::default()]` (the struct has no
`Default`; genuinely uncompilable).

## Verification posture (the brief's DEFERRED half, stated as it asked)

Built and exercised at Check: the table; its Rust consumers (host argv, `docker cp` argv,
extraction paths, real staging over dummy binaries); every pipeline file pinned to it; the
checker's own negatives; `dist --check`; the whole `cargo xtask ci`.

**Deferred — not observed in this cycle, and to whom:** that a real tarball CONTAINS both
binaries and that `install.sh` PLACES and REMOVES both on a real host. Nothing in `cargo
xtask ci` can build a tarball (needs Docker + network, `dist.rs:37-39`), and `install.sh`
refuses non-root (`install.sh:92`), writes `/etc/wyrd`, creates a user and stages units, so no
test on the gate host can execute it. What stands in: the text contract over both of its
sites, and the release smoke step `.github/workflows/release.yml:60-100`, which this slice
extends to run the validator — placed BEFORE `libfdb_c` is installed, so the run also proves
the "no FoundationDB client needed" claim — and to assert its absence after uninstall.
Observing that run is the release workflow's job on the next `v*` tag or a
`workflow_dispatch` (`release.yml:23`). The PR-time `fdb-image.yml` smoke of the validator
inside the image is the one NEW observation the draft PR's CI gives the human before a tag.

## Sign-off items (pre-declared by the brief; stated so they are accepted, not assumed)

1. **Option A confirmed?** The patch builds the image with both binaries and extracts both
   from it (`Dockerfile:78`, `:137`; `dist.rs:542-640`). If option A is NOT confirmed at §9,
   the brief says reject back to Plan — this patch is not patchable into B.
2. **The literal DoD is not observed this cycle.** "Tarball contains both; `install.sh`
   places both" is carried by structure (one build, one `docker cp` per table entry, one
   staging callable exercised for real) plus the release smoke; the remedy if that trade is
   refused is a `v*` tag or `workflow_dispatch`, not a weaker test here.
3. **Packaging lands before the tool works.** `wyrd-validate` still resolves its
   configuration, echoes it and exits (crates/validate as of #852); until #743 and the
   endurance verdict, only the maintainer not cutting a `v*` tag keeps a stub off operators'
   hosts (`release.yml` publishes on any `v*` tag with no other condition).
4. **The image now carries a tool that deletes objects** (brief "Impact"). Bounds: it is not
   the `ENTRYPOINT` (`Dockerfile:150`, unchanged); it refuses to run without `--endpoint`
   and the other required flags (ran it: exit 2, usage); run-id-scoped keys are proposal
   0017 §15's stated safety requirement.

## Residuals a reviewer will ask about

- **`shellcheck` is not installed on this host**, so `install.sh` was not linted locally;
  the release workflow runs it (`release.yml:41-42`). The added lines are the same shape as
  the existing `install` / `rm -f` / `echo` lines. Not needed to build or exercise the fix,
  so not declared as an external dependency; a doctor row would be
  `cmd = "shellcheck --version"`, level WARN.
- **`fdb-image.yml`'s path filter was deliberately NOT widened to `crates/validate/**`.**
  The workflow's own rule (`fdb-image.yml:28-40`) lists "feature-gated inputs invisible to
  the default gate"; the validator is featureless, `cargo xtask ci` compiles and tests it,
  and its usage line is pinned by its own test. Widening would add a ~20-minute image build
  to every PR on a crate under active development (#853, #854, #743). Recorded here rather
  than left as a blind spot (rubric "Workflow edits").
- **`install.sh --help` prints a fixed line range** (`sed -n '2,17p'`, was `2,16p`,
  `install.sh:38`): the header grew by two lines, so the range moved with it. The old range
  also printed the `set -eu` line; the new one prints exactly the header comment.
- **The included module runs the layout tests twice** (once in its own target, once inside
  `dist_templates`): harmless, and the price of "one checker, no helper file" under the C4
  classifier (`run-verify.sh:144`).
- **Docs currency:** `docs/design/architecture/07-deployment-view.md:42` (§7.2, the artifact
  list) now names both binaries and the declaration/gate; the tarball README gets the
  validator paragraph the brief asked for, with the opening roles sentence kept verbatim.
- The TLS note re-checked on this base: `cargo tree -p wyrd-validate -e normal` shows
  `rustls-native-certs`, `rustls-pki-types`, `openssl-probe` only — pure Rust, no native
  toolchain beyond the build stage; the full two-bin `cargo check` with `fdb,etcd` passed.
- `IMAGE_BINARY_PATH` is removed (it had no other reference in the repo); `shipped_binaries()`
  replaces it.

## Scratch artifacts (under `$PDCA_SCRATCH`, prefix `pdca-builder-742-`)

`ci.log`, `red.log`, `wip.diff`, `check-two-bins.log`, `cov.log`, `lcov.info`, `lines.txt`,
`mutants.log` (the gate script's run), `mutants-dist.log` / `mutants-out/` (uncapped) and
`mutants-dist2.log` / `mutants-out2/` (capped lints), `usage.txt`, `wyrd-usage.txt`. The
gate script also left the gitignored `mutants.out/` in the lane worktree, as it does on every
Check.
