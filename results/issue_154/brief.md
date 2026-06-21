# Brief — issue 154 / m2-7-followup-build-bench-repo-hygiene

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed
> by the driver — keep the `- **Label:** value` shape. Success criterion is load-bearing.

- **Slug:** m2-7-followup-build-bench-repo-hygiene
- **Defect / goal:** a grouped set of low-severity hardening items surfaced reviewing #117
  (M2.7); none affects correctness today, each is a small independent cleanup. This is a
  deliberate maintenance bundle (Do MAY land it as several focused commits):
  1. Dockerfile base is the floating `rust:1.96-bookworm`
     (`crates/chunkstore-grpc/tests/dserver/Dockerfile:12`) while `rust-toolchain.toml:4`
     pins `1.96.0` — a base bump can trigger a build-time toolchain re-download; and
     `cargo build --release --bin wyrd` (`Dockerfile:15`) lacks `--locked`.
  2. Tier-2 teardown is not panic-safe: in `run_integration` (`xtask/src/main.rs:111-118`)
     the test runs in a closure whose result is propagated after `compose_down`; a `panic!`
     (vs a non-zero `cargo test` exit) unwinds past teardown and leaks the cluster.
  3. The bench `Cluster` doc (`crates/core/benches/throughput.rs`, struct doc) says
     "dropping the cluster shuts them down" — false for the detached `_servers`
     `JoinHandle`s (drop detaches the task, it does not abort the server).
  4. `WYRD_DSERVER_COUNT` (`xtask/src/main.rs:103-107`) silently clamps `0` / `1` / garbage
     to 9 via `.filter(|&n| n >= 2).unwrap_or(DSERVER_COUNT)` (`DSERVER_COUNT = 9`, `:75`)
     with no warning.
  5. No `.github/dependabot.yml` for a repo gated by a `cargo-deny` advisory wall.
  6. `.dockerignore:7` lists `results/`, which nothing in the repo produces.
  7. Architecture §13 (`docs/design/architecture/10-quality-risks-glossary.md`) numbers
     tiers such that "Tier 2 = a single real machine", colliding with the code's "Tier-2"
     = the container integration test.
- **Success criterion:** each item addressed — Dockerfile pinned to `rust:1.96.0-bookworm`
  + `--locked`; teardown made panic-safe (Drop guard / `catch_unwind`); the bench doc
  corrected (or servers aborted on drop); `WYRD_DSERVER_COUNT` warns on a rejected value;
  `.github/dependabot.yml` added (cargo + github-actions ecosystems); the inert
  `.dockerignore` `results/` line removed; a one-line tier-numbering cross-map note added.
  Demonstrable at C4-verify: `cargo xtask ci` green; inspection confirms each cited change.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data   (build / CI / bench / docs; no data-path change)
- **Conflicts with:** 150
- **Scope:** the seven enumerated hygiene items only. / **out of scope:** anything requiring
  a design decision (the gating model, the bench's loopback scope, the readiness-retry) —
  all by-design per #117's review; no behavior change to the data path.
- **Citations expected:** Do cites path:line on `main` for each item (files listed above).
- **Prior-art check:** each cited file / line confirmed in the target checkout; these are
  the residual nits from #117's post-merge review, untracked elsewhere. Net-new.
- **Disposition hint:** likely-fix

## Scheduling note
Conflicts with #150: item 2 here reworks the same `run_integration` teardown
(`xtask/src/main.rs:111-118`) as #150's log-capture change — land the teardown rework once
and do not co-schedule the two in one concurrent wave.

## STOP discipline
Draft only until Check sign-off.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebase item-2 teardown rework onto #150's shape. #150 lands first and introduces `finish_integration` (capture-logs-before-teardown) on the same `run_integration` teardown (xtask/src/main.rs:114-121) that #154's `with_teardown` Drop-guard reworks. Rebuild #154's panic-safe teardown on top of #150's `finish_integration` so the two compose instead of colliding — i.e. preserve both capture-before-teardown (#150) and teardown-on-panic-unwind (#154). The other six hygiene items reviewed clean and are unaffected; only the teardown item needs reworking against #150.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
