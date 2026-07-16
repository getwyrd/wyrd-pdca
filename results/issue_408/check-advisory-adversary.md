# Adversarial review — issue #408, iteration v5 (m4-checked-consistency-run-elle-report)

Posture: assumed the patch wrong and the reviewer fooled; tried to prove it. Toolchain was
available (cargo, java, unzip, the pinned elle-cli jar), so every verdict below is
execution-backed, none provisional.

## Refutation attempts that FAILED (each independently re-run in a scratch clone of `origin/main` @ e0e39c1 + patch.diff)

- **Red→green re-run** — green leg: `cargo test -p xtask --test consistency_run_orchestration`
  (40 pass) and `cargo test -p wyrd-server --test consistency_workload` (18 pass) on the patched
  clone. Red leg: reverting the production modules while keeping the added tests fails both
  binaries (E0432/E0599). The red is compile-fail (ADDED_TEST shape), exactly as the brief's
  Falsifiability section pre-declared — and the tests carry real behavioral assertions
  (token-keyed verdicts, nested `deny_unknown_fields`, gate ordering), so post-merge behavioral
  regressions also flip red, not just deletion.
- **The live verdicts are real and history-sensitive, not tautological.** Re-ran the actual jar
  (sha256 matches the report: `c9ba9b9f…f539a`) over the run's own histories at
  `target/consistency-run/`: register → `true`, directory → `true`, reproducing the report.
  Known-bad fixtures → `false`/`false`; degraded-final-read fixture → `:unknown`. Mutating the
  live directory history (final-read element `1`→`999`, i.e. one acknowledged `:add` dropped)
  flips the checker to `false` — the `true` is earned by the history's content.
- **The committed artifact is not a hand-transcription.** The verbatim block in
  `docs/design/reviews/m4-checked-consistency-run.md:161-170` is byte-identical to the
  runner-emitted `target/consistency-run/report.md` (diff clean), and every number in it
  cross-checks against `target/consistency-run/run-summary.json` (120/121/480 ops, counts,
  member map, determinate composed read).
- **Every v4 sign-off defect has a production fix AND a Check-time pin**, verified at the target:
  single-writer delete-pool keys (`crates/server/src/consistency_workload.rs:115`; pins at
  `crates/server/tests/consistency_workload.rs:380,432` — the banded trap is exhibited, not just
  avoided); unresolved-probe degrade to `:info` with bounded re-probe
  (`crates/server/src/consistency_workload.rs:845`, `crates/server/tests/consistency_run_fdb.rs:614-671`;
  pin at `consistency_workload.rs:279`); quiesce before the composed read
  (`crates/server/tests/consistency_run_fdb.rs:98,229`); nested `deny_unknown_fields` + nested
  unknown-field test (`xtask/src/consistency_run.rs:142,169,213,238`;
  `xtask/tests/consistency_run_orchestration.rs:394`); `unzip` in the preflight
  (`xtask/src/consistency_run.rs:629,654`); directory op-count no longer counts sweep probes
  (`crates/server/src/consistency_workload.rs:1002`; pin at `consistency_workload.rs:350`).
- **Gate-wiring attacks that found nothing:** `:unknown`-with-exit-0 parses inconclusive and the
  runner's final match refuses non-Pass pairs (`xtask/src/consistency_run_runner.rs:561-572`);
  a delete-pool `false` fails the run even when the vacuity gate would already have blocked it
  (`consistency_run_runner.rs:551-559`, independence pinned at
  `consistency_run_orchestration.rs:489`); the fixtures self-check runs against the SAME jar
  before any verdict is acted on (`consistency_run_runner.rs:537` precedes the verdict match);
  INV-1 stale-status/dropped-op fabrications from v3 are gone (`OpFailed::into_record` at
  `crates/server/src/consistency_observable.rs:85`, used at `consistency_run_fdb.rs:592-595,658`).

## Findings

- NEEDS-HUMAN — **The materialized fault never touched a single op — is this the "checked run
  under failure" #329 DoD item 2 intends?** The run's own summary shows 720/720 ops `ok`,
  `info: 0`, `fail: 0` across all three pools (`target/consistency-run/run-summary.json`;
  disclosed prominently at `docs/design/reviews/m4-checked-consistency-run.md:86-94`): the
  partition of one of three nodes was fully absorbed by FDB's quorum, so the checked histories
  are observationally identical to ones recorded on a healthy cluster — only the leg's typed
  evidence distinguishes the runs. This *conforms to the brief* (Design §4 gates exactly on
  INV-2 + materialized evidence, both attested) and the report names it as its most important
  caveat, so it is not an implementation defect — but whether a fully-absorbed fault is a
  strong enough witnessed run for the public credibility artifact, or whether the maintainer
  should demand a re-run with a quorum-costing / client-visible fault before ready-mark, is a
  fitness-to-purpose call only the human can make at sign-off.
- `crates/server/tests/consistency_run_fdb.rs:675-698` — `register_outcome_counts` re-derives
  the ok/fail/info classification (`is_indeterminate` + per-kind 2xx/404 arms) in the one file
  no Check gate compiles, duplicating production's `register_completion_type`
  (`crates/server/src/consistency_workload.rs:719`). The two agree today (verified by
  inspection), and the counts are report-fidelity only — never verdict-bearing — so this is a
  drift risk, not a defect: if the production classification ever changes, the summary's
  outcome counts silently diverge from what the EDN says. Cheap hardening: count by
  `register_completion_keyword` instead. Not worth blocking on.

## Verdict

Attempted to refute the red→green evidence, the verdict parser, the gate wiring, the composed
final read, the delete-pool construction, the seam contract, and the committed report's
authenticity — could not. The one open question (the absorbed fault) is architectural/fitness,
already disclosed by the artifact itself, and flagged above for the human.
