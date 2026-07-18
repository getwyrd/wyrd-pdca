# Build notes — issue 409 / m4-elle-ci-job-dst-regression-loop

## What shipped

Four files against `getwyrd/wyrd @ main` (720a225-derived, current worktree HEAD
`e47cb88`, which already carries #408's fold — `xtask/src/consistency_run_runner.rs`,
`crates/server/tests/consistency_run_fdb.rs`, the `consistency-run` xtask subcommand):

1. **`.github/workflows/elle-register-verdict.yml`** (new) — the standing off-Check job.
   Triggers on `schedule` (`0 7 * * *`, the first free UTC hour among the sibling
   nightlies — 02/03/04/05/06 are taken by tier1-jepsen/tier1-disk-faults/
   integration-nightly+mutants/tier2-kill-reconstruct/fdb-conformance,
   `.github/workflows/*.yml:cron` grepped to confirm) + `workflow_dispatch`, nothing
   else. `timeout-minutes: 60`. Provisions Docker (ubuntu-latest ships it),
   the FDB client package (verbatim `fdb-conformance.yml` step,
   `.github/workflows/fdb-conformance.yml:112-121` on the target branch), a JVM
   (`actions/setup-java@v4`), and the elle-cli standalone jar (curl + sha256 pin, using
   the exact version/hash already recorded in
   `docs/design/reviews/m4-checked-consistency-run.md:52-53` — the same jar #408's first
   witnessed run used). The one load-bearing invocation is
   `cargo xtask consistency-run` with `WYRD_TIER1: "1"` set in the job `env:` (mirrors
   `.github/workflows/tier1-jepsen.yml:53-55,71-72`'s env-then-bare-command shape). A
   `tee` captures the invocation's own stdout to `target/consistency-run/console.log`
   (the checker's output — elle-cli prints its verdict rather than writing a file,
   confirmed by reading `xtask/src/consistency_run_runner.rs:337-351`'s `run_checker`,
   which only `print!`s the captured stdout) so it survives as an artifact alongside the
   EDN histories / run-summary.json / report.md the runner itself writes under
   `target/consistency-run/` (confirmed by reading
   `xtask/src/consistency_run_runner.rs:51-52` `output_dir()`,
   `crates/server/tests/consistency_run_fdb.rs:257-312` (the three writes), and
   `xtask/src/consistency_run_runner.rs:449-452` (`report.md`)). One
   `actions/upload-artifact@v7` step, `if: always()`, `path: target/consistency-run/`.

2. **`.github/workflows/report-nightly-failure.yml`** (modified, `:26-35`) — appended
   `elle-register-verdict` to the watched `workflows:` list (and the header prose
   `:5-6`), so a scheduled failure produces exactly one tracked bug via the existing
   idempotent one-open-issue rule. Left the `conclusion == 'failure'` / `event ==
   'schedule'` filter (`:51-53`) untouched, per the brief's explicit instruction — a
   `timeout-minutes` overrun already surfaces as `failure` on GitHub.

3. **`crates/dst/tests/commit_ambiguity.rs`** (appended, after `:1042` on the target
   branch) — the DST promotion anchor: `const REGRESSION_SEEDS: &[u64] = &[0, 1, 2, 3]`
   plus `#[test] fn committed_regression_seeds_stay_green`, which replays each seed
   through the four existing property bodies (`run_cas_ambiguity`, `run_blind_ambiguity`,
   `run_timeout_ambiguity`, `run_contended_1031`) in their faithful configuration, with
   the same per-seed anti-vacuity counters those bodies' own sweeps assert.

4. **`xtask/tests/consistency_ci_job.rs`** (new) — the pinning test, five independent
   `#[test]` fns, one per deliverable (a)-(e).

## Why this shape, what I read to bind it, what I ruled out

**Read only what the brief cited plus the one-hop peer callsites it named** — the seat
constant + dispatch fn (`crates/server/src/consistency_workload.rs`), `tier1-jepsen.yml`
+ `fdb-conformance.yml` (workflow shape), `report-nightly-failure.yml`'s watched list,
`fdb_image.rs` / `fdb_harness.rs` (pinning-test pattern), `custodian.rs:1343-1375`
(anchor shape) + the four `run_*_ambiguity` bodies it named, and — per the brief's
explicit read-to-bind grant — `xtask/src/consistency_run_runner.rs` +
`crates/server/tests/consistency_run_fdb.rs` + `xtask/src/main.rs`'s dispatch match, to
discover what #408's folded result ACTUALLY emits rather than trust the brief's
provisional path guesses. I did not open anything else in the tree.

**Artifact paths (criterion c) — bound to the read, not the brief's guess.** The brief
flagged `run-summary.json` / `target/consistency-run/` as "provisional until 408's
fold". Reading the folded runner confirmed `output_dir()` really does resolve to
`workspace_root().join("target/consistency-run")`
(`xtask/src/consistency_run_runner.rs:51-52`) and that the scenario test writes
`register-history.edn`, `directory-history.edn`, `run-summary.json` there
(`crates/server/tests/consistency_run_fdb.rs:257-312`), with `report.md` added by the
runner itself (`:449-452`) — so the guess turned out correct, but I verified it rather
than assuming.

**The "checker output" artifact — the one place I add non-decision plumbing to YAML.**
`run_checker` (`consistency_run_runner.rs:337-351`) only `print!`s elle-cli's stdout; it
writes no file. The brief's scope for item (c) is unambiguous that the checker output
must be an uploaded artifact, and out-of-scope explicitly forbids reopening #408's
runner semantics ("consume `cargo xtask consistency-run` as-is"). The only way to
satisfy both without touching #408 is for the *invoking* workflow step to capture the
command's own stdout — `cargo xtask consistency-run 2>&1 | tee
target/consistency-run/console.log`. I checked this against the "thin" bar (ADR-0009 §CI,
"CI logic lives in cargo xtask, not YAML") and concluded a `tee` is not *logic* — it
makes no decision, branches on nothing, and the pinning test's `b_triggers…` still
asserts the workflow's only `cargo xtask` HEAD is `consistency-run` (so no second
xtask subcommand, `cargo test`, or shell decision tree could ride along unnoticed). The
rejected alternative — modifying `consistency_run_runner.rs` to write a
`checker-output.log` file itself — was **out of scope** per the brief ("never reshape
#408"), so it was never on the table, not merely more expensive.

**Cron hour.** Grepped every `.github/workflows/*.yml` for `cron:` on the target branch:
02 (tier1-jepsen), 03 (tier1-disk-faults), 04 (integration-nightly, mutants), 05
(tier2-kill-reconstruct), 06 (fdb-conformance). Picked 07, the brief's own suggested
value, and cited the grep in the workflow's own comment so a future editor sees the
same evidence rather than re-deriving it.

**elle-cli download.** No existing workflow or script in the tree downloads elle-cli (I
grepped `elle-cli\|ELLE_CLI\|WYRD_ELLE_CLI_JAR` across `*.yml`/`*.md`/`*.sh` — only the
design doc and xtask test fixtures reference the pinned `0.1.9` version/hash). I used
the `ligurio/elle-cli` GitHub release asset URL convention with the exact sha256 already
recorded at `docs/design/reviews/m4-checked-consistency-run.md:53` and
`xtask/tests/consistency_run_orchestration.rs:520-530`'s reference to the same jar — so
the pin is not invented, it is the one #408's own report already names. **This one step
is unverified by me** (no network access in this environment to confirm the release
asset URL resolves) — see the NEEDS-HUMAN note below.

**Not asserting "the whole job actually ran green on GitHub."** The Verification
posture (posture (a)+(b), pre-declared) explicitly defers the first live execution
post-merge, confirmed by `workflow_dispatch` (named confirmer: Eduard Ralph). The
pinning test is textual/mechanical per the brief's proof standard ("textual pinning...
never semantic YAML proof").

## The DST anchor's replay design, and what I checked before trusting it

The four property bodies have different signatures
(`run_cas_ambiguity(seed, fidelity, observer) -> FdbObservations`,
`run_blind_ambiguity(seed, observer) -> FdbObservations`,
`run_timeout_ambiguity(seed, observer) -> bool`,
`run_contended_1031(seed, observer) -> FdbObservations`), and their anti-vacuity
counters are asserted at different granularities in the existing file: `:332-333` and
the loop body at `:878-881` are genuinely **per-seed** (`ambiguous_conditional_commits
>= 1` inside the loop over seeds); `:686-699` and the three asserts after the loop at
`:886-900` are **aggregate-over-the-whole-64-seed-sweep** (`landed_late >= 1`,
`deferred_landings >= 1`, etc. summed across ALL seeds). A small committed
`REGRESSION_SEEDS` set cannot honestly assert an aggregate-over-sweep property (e.g.
"some seed in this 4-element set left a 1031 batch deferred past a determinate winner"
is not guaranteed by any 4 fixed seeds without hand-picking for it, which the brief
does not ask for and would make the "not yet bug-finding, deterministic replay set"
provenance claim false). So the anchor asserts only the **per-seed** anti-vacuity shape
that generalizes to any seed:

- Leg 1/4 (CAS legs): `obs.ambiguous_conditional_commits >= 1` — guaranteed for ANY
  seed because the nemesis is budget-based, not a per-commit coin
  (`crates/dst/tests/support/mod.rs:414-423`: "every seed spends the budget and
  explores one point of the ambiguity space" — I read this comment before relying on
  it, then confirmed empirically with seeds `0,1,2,3` below).
- Leg 2 (blind put): `obs.ambiguous_blind_commits == 1` — the property body itself
  already asserts this unconditionally at `:479-483`, so calling it is sufficient; my
  assertion is a belt-and-suspenders duplicate naming the same invariant per the
  brief's "asserting the SAME... observations" instruction.
- Leg 3 (timeout): `timed_out_commit_over` itself `expect_err`s the struck commit
  (`:632-634`) inside the property body, so a seed that never triggers 1031 panics
  *inside the call* — I call it and discard the returned bool rather than re-deriving
  an FdbObservations struct the function does not expose.

**I did not hand-pick seeds for the aggregate landed/deferred/rejected properties** —
seeds `0, 1, 2, 3` are the first four naturals, chosen only to be small, memorable, and
NOT the aggregate-tuned seeds `custodian.rs` uses (`0x5EED_...`) so nobody mistakes them
for already-minimized bug seeds. This is exactly the honesty the brief's Seed
provenance paragraph asks for — I say so in the appended comment
(`crates/dst/tests/commit_ambiguity.rs:1054-1064` on the patched tree).

## What I ruled out

- **Porting `custodian.rs`'s `dst_campaign_test!` / `ChaCha8Rng` machinery** — the brief
  explicitly forbids this. Cost check: `dst_campaign_test!` expands into a
  `#[madsim::test]` sweep entry point that takes an `&mut impl Rng`, but every property
  body in `commit_ambiguity.rs` takes a bare `seed: u64` and builds its own
  `madsim::runtime::Runtime::with_seed_and_config(seed, ...)` internally (see
  `run_cas_ambiguity`, `:318-325`) — porting the macro would mean rewriting all four
  bodies' signatures, a change to every existing test in the file, not a one-block
  append. The plain-`#[test]`-with-a-loop shape the existing
  `the_settling_re_read_covers_both_halves_of_the_ambiguity_space` (`:355-388`) already
  uses costs zero rewrites elsewhere.
- **Modifying `xtask::consistency_run_runner` to write a dedicated checker-output
  file** — out of scope per the brief ("out of scope: any change to #408's runner
  semantics... consume `cargo xtask consistency-run` as-is"). Not a cost trade-off, a
  hard brief boundary.
- **Gating the artifact upload on `if: failure()`** (the `tier1-jepsen.yml` /
  `fdb-conformance.yml` diagnostics-on-failure precedent) — rejected because this job's
  artifacts are the credibility/anomaly evidence itself (item 2 of #329's DoD), not
  merely failure diagnostics; a clean run's histories are the baseline a later anomalous
  run needs to diff against. Used `if: always()` instead. Cost: `if-no-files-found:
  warn` instead of `ignore`, since a run that never reaches `output_dir()` creation
  (an early hard-error before the scenario starts) genuinely has nothing to upload and
  should say so loudly, not silently.
- **Reusing `fdb_harness.rs`'s helpers via `use` rather than duplicating them** — ruled
  out by the Verification posture's own instruction ("helpers local to the test file per
  the `fdb_image.rs` / `readme_dev_section.rs` precedent"); those two precedent files
  define their own copies rather than sharing a module, so I matched that, not
  `fdb_harness.rs`'s (which is itself the newer, cross-test-shared-helper outlier the
  Verification posture steers away from for this file).

## Genuine-red / production-path / fixture-includes-the-fault (mandatory self-check)

**(a) Genuine red — checked by literal revert, not inference.** Reverted all three
production changes in the worktree (`rm -f
.github/workflows/elle-register-verdict.yml`; `git checkout --
.github/workflows/report-nightly-failure.yml crates/dst/tests/commit_ambiguity.rs`),
kept `xtask/tests/consistency_ci_job.rs`, ran `cargo test -p xtask --test
consistency_ci_job`: **all 5 tests failed** — `a`/`b`/`c` on the missing workflow file
(`No such file or directory`), `d` on the reporter's watched list omitting
`elle-register-verdict`, `e` on the DST file having no `REGRESSION_SEEDS` anchor. Then
restored all three files (re-applied the same edits) and re-ran: **all 5 green**. This
is exactly the RED/GREEN pair `engine/scripts/run-verify.sh`'s classifier will produce
mechanically (`.github/workflows/elle-register-verdict.yml` is an ADDED non-test file →
`rm -f`'d in the RED leg; `report-nightly-failure.yml` and `commit_ambiguity.rs` are
MODIFIED files → `git checkout`'d back to base in the RED leg; `xtask/tests/
consistency_ci_job.rs` is the sole ADDED_TEST → kept).

**(b) Production path — yes.** Every assertion reads the actual shipped file
(`crates/server/src/consistency_workload.rs`, the two workflow YAMLs,
`xtask/src/main.rs`, `xtask/src/consistency_run_runner.rs`,
`crates/server/tests/consistency_run_fdb.rs`, `crates/dst/tests/commit_ambiguity.rs`) —
no copy, no mock, no re-implementation. The DST anchor's `committed_regression_seeds_stay_green`
calls the SAME `run_cas_ambiguity` / `run_blind_ambiguity` / `run_timeout_ambiguity` /
`run_contended_1031` functions the pre-existing sweep tests call, over the SAME
`SimFdbMetadataStore` — confirmed by running it: `RUSTFLAGS="--cfg madsim" cargo test -p
wyrd-dst --test commit_ambiguity` (whole file, all 14 tests including the new one) —
green, and I also ran it standalone to confirm the anti-vacuity assertions actually
fire (not dead code): they pass because the properties genuinely hold for seeds 0-3, not
because the assertions are unreachable.

**(c) Fixture includes the fault — yes, by construction of the RED leg above.** The RED
leg is not a curated fixture that excludes the failing element; it is the actual
production files (the two workflows, the DST anchor) removed/reverted, so the pinning
test's file reads hit the genuinely-missing/genuinely-stale state a merge-without-this-
patch would leave. No `healthy_fleet`-style exclusion anywhere — every assertion targets
a file this patch is the sole source of.

## Runner used

`cargo test -p xtask --test consistency_ci_job` (Verification posture (i)'s named C4-verify
command) for the pinning test, and `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst
--test commit_ambiguity` (mirrors `xtask::run_dst`, minus `MADSIM_TEST_NUM` — not needed
since neither the new test nor the whole-file run uses the `#[madsim::test]` sweep
macro) for the DST anchor — both run through `cargo test`, never a hand-rolled
container/process invocation. `cargo fmt --check` (whole workspace) and `cargo clippy -p
xtask --tests --no-deps` / `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --tests
--no-deps` (the two touched crates) are clean — the patch is commit-ready for the
target's `rustfmt`/`clippy` hooks. `python3 -c 'yaml.safe_load(...)'` confirmed both
touched/added workflow YAMLs parse.

## NEEDS-HUMAN

NEEDS-HUMAN external dependency: network access to verify the elle-cli release asset
URL — I could not confirm from this environment that
`https://github.com/ligurio/elle-cli/releases/download/0.1.9/elle-cli-0.1.9-standalone.jar`
resolves to the exact jar whose sha256
(`c9ba9b9fd32640e73d632cb5f15069c162ba6528a67f27a878767187c59f539a`) is already recorded
in `docs/design/reviews/m4-checked-consistency-run.md:53` (the same jar the human used
locally to produce #408's first witnessed run). The sha256 pin means a wrong/moved URL
fails LOUDLY (the `sha256sum -c` step) rather than silently, so this is a safe-fail
gap, not a silent one — but the workflow's first `workflow_dispatch` (the already-
planned post-merge confirmation step named in the brief's Verification posture) is
where a human should confirm the download step actually succeeds, since I have no way
to exercise it here.

```toml
[[doctor.checks]]
id    = "network-egress"
cmd   = "curl --fail --location --silent --show-error --output /dev/null --range 0-0 https://github.com/"
hint  = "This environment has no outbound network access; a workflow-YAML step that downloads a release asset (elle-cli, the FDB client .deb) can only be validated by a human running `workflow_dispatch` post-merge, not by the Do beat."
level = "WARN"
```

This does not block the criterion the pinning test proves (textual pinning of the
workflow's trigger/timeout/thinness/artifact-path/dispatch-coupling shape, per the
Verification posture's explicit deferral of "the workflow's first actual execution" to
post-merge `workflow_dispatch`) — it only means the download step's real success is
unverified by me, exactly the gap the brief's Verification posture already
pre-declared and assigned to the named post-merge confirmer.
