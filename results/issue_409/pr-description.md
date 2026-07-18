# PR description

## Summary
**User impact:** Wyrd's strongest data-safety check — the externally checked
consistency run that proves the store does not lose or corrupt data — only ran when
someone remembered to run it by hand. A future change that quietly reintroduces a way
to lose data could sit unnoticed until the next manual run, and even a caught anomaly
would leave no lasting trace: no bug ticket filed, no permanent regression test
committed.

This PR turns that one-shot check into a standing daily CI job and closes the
learning loop around it: a scheduled failure automatically files exactly one tracked
bug, and a confirmed anomaly's minimized reproduction is committed as a permanent
deterministic regression test.

## What to look at
Four pieces, each small on its own:

1. the new nightly workflow (`.github/workflows/elle-register-verdict.yml`) — it
   provisions its own environment and then delegates everything to one
   `cargo xtask consistency-run` invocation, keeping decision logic out of YAML;
2. one entry added to the failure reporter's watch list
   (`.github/workflows/report-nightly-failure.yml`);
3. a committed regression-seed block appended to the deterministic-simulation test
   suite (`crates/dst/tests/commit_ambiguity.rs`), with the promotion procedure for
   future anomalies documented in the workflow header;
4. a pinning test (`xtask/tests/consistency_ci_job.rs`) that fails if any of the
   three drifts out of sync.

To try it locally: `cargo test -p xtask --test consistency_ci_job` (offline, no
Docker or JVM needed). The job itself can be triggered once after merge from the
Actions tab via "Run workflow".

## Root cause
The landed verdict dispatch (`consistency_verdict_dispatch`,
`crates/server/src/consistency_workload.rs:1074`) routes the Elle verdict to a
privileged scheduled job named by `ELLE_OFF_CHECK_VERDICT_JOB =
"elle-register-verdict"` (`:1038`), but no workflow of that name existed — the seat
was empty, `report-nightly-failure.yml` did not watch it, and
`crates/dst/tests/commit_ambiguity.rs` had no committed-seed anchor to promote a
minimized anomaly into.

## Fix
- **`.github/workflows/elle-register-verdict.yml`** (new): triggers on `schedule`
  (07:00 UTC daily — the first free hour; 02:00–06:00 are taken by the sibling
  nightlies) + `workflow_dispatch` and nothing else; `timeout-minutes: 60`;
  `permissions: contents: read`. Provisions Docker, the FoundationDB client package
  (mirroring `fdb-conformance.yml`), a JVM, and the elle-cli standalone jar pinned by
  sha256 (the same jar/hash already recorded in
  `docs/design/reviews/m4-checked-consistency-run.md:53`). The single load-bearing
  step is `WYRD_TIER1=1 cargo xtask consistency-run`; a `tee` captures the checker's
  stdout (elle-cli prints its verdict rather than writing a file) so it survives as an
  artifact. One `upload-artifact` step with `if: always()` uploads
  `target/consistency-run/` on every run — a clean run's histories are the baseline a
  later anomalous run diffs against. It is **not** a required status check, matching
  every sibling nightly (ADR-0016 gating policy).
- **`.github/workflows/report-nightly-failure.yml`**: appends `elle-register-verdict`
  to the watched `workflows:` list; the `conclusion == 'failure'` / `event ==
  'schedule'` filter is deliberately untouched (a timeout overrun already surfaces as
  `failure`).
- **`crates/dst/tests/commit_ambiguity.rs`**: appends `REGRESSION_SEEDS` +
  `committed_regression_seeds_stay_green`, replaying each committed seed through the
  four existing metadata-ambiguity property bodies (`run_cas_ambiguity`,
  `run_blind_ambiguity`, `run_timeout_ambiguity`, `run_contended_1031`) in their
  faithful configuration, with per-seed anti-vacuity assertions. The initial seeds
  `[0, 1, 2, 3]` are documented as a deterministic replay set that keeps the anchor
  live and proves the append path — explicitly **not yet bug-finding** (ADR-0009
  commits bug-finding seeds; provenance is never fabricated).
- **`xtask/tests/consistency_ci_job.rs`** (new): five independent `#[test]` functions,
  one per deliverable, all pure file reads with helpers local to the file — no new
  dependency, no feature gate, no `wyrd-server` import.

## Verification
All citations are on the branch this PR targets (`main`).

- **Claim:** the workflow fills the routed seat — its `name:` equals
  `ELLE_OFF_CHECK_VERDICT_JOB`.
  **Checked:** `crates/server/src/consistency_workload.rs:1038` (the constant) and
  `:1074` (the dispatch that routes to it); pinned by reading that source as text,
  never by importing `wyrd-server` into `xtask`.
- **Claim:** the job runs on `schedule` + `workflow_dispatch` only, in a free cron
  slot, timeout-bounded, and its only `cargo xtask` invocation is a subcommand that
  really exists.
  **Checked:** `xtask/src/main.rs:117` — the `consistency-run` dispatch arm the
  workflow invokes; the pinning test asserts the workflow's only xtask command head
  appears in that dispatch table, so a renamed subcommand fails in CI instead of
  silently on the first cron.
- **Claim:** the uploaded artifacts are read from where the runner actually writes
  them, not a guessed path.
  **Checked:** `xtask/src/consistency_run_runner.rs:52` (the output dir resolves to
  `target/consistency-run`), `:330` (`run-summary.json`), `:449` (`report.md`); the
  EDN histories are written by the scenario in
  `crates/server/tests/consistency_run_fdb.rs`.
- **Claim:** a scheduled failure becomes exactly one tracked bug.
  **Checked:** `.github/workflows/report-nightly-failure.yml:28-32` (the watched
  list the new entry joins) and `:52-53` (the failure/schedule filter, unchanged).
- **Claim:** the promotion anchor exists and cannot replay green vacuously.
  **Checked:** the block appended after `crates/dst/tests/commit_ambiguity.rs:1042`,
  mirroring the anchor shape of `crates/dst/tests/custodian.rs:1351`; executed green
  via `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test commit_ambiguity`
  (all 14 tests, including the new one).
- **Test:** `xtask/tests/consistency_ci_job.rs` — with the two workflow files and the
  DST block reverted, all five tests fail (missing workflow, unwired reporter, absent
  anchor); with the change applied, all five pass. `cargo fmt --check`, clippy on
  both touched crates, and a YAML parse of both workflows are clean.
- **Deferred (post-merge):** the workflow's first live execution — a workflow can
  only run once it exists on GitHub. Eduard Ralph confirms it via
  `workflow_dispatch` after merge and records the run link on the issue; that run
  also confirms the sha256-pinned elle-cli download URL resolves (a wrong URL fails
  loudly at the `sha256sum -c` step, never silently).

Fixes #409
