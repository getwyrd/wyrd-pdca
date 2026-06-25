# Tier-2 kill-and-reconstruct durability harness

## Summary

The M3 verification campaign is supposed to prove durability on real
infrastructure: kill a live D server and confirm the production custodian
rebuilds the lost data over a real gRPC fleet. That leg never existed — the
`kill-reconstruct` xtask command only ran an external command named by
`WYRD_TIER2_CMD`, an environment variable that is not defined anywhere in the
repository, so the campaign exercised nothing and no test covered the path.
This change replaces that empty shim with a real, in-repo harness that stands
up a containerized cluster, kills a server, and drives the production
reconstruction code to verify the durability guarantees.

## What to look at

- `xtask/src/faults.rs` — `run_kill_reconstruct` now orchestrates a real run
  (bring the compose cluster up, pick and kill a victim container, invoke the
  scenario, tear down) instead of shelling out to an undefined env command.
- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — the scenario: a
  ten-server RS(6,3) cluster, `docker kill` on one server, then the production
  `reconcile_step` -> `reconstruction::reconcile` path, with three durability
  assertions. The assertion helpers and orchestration helpers carry their own
  unit tests in this file and in `xtask/src/kill_reconstruct.rs`.
- To exercise it: `cargo xtask ci` compiles and type-checks the whole harness
  and runs the helper unit tests with no containers needed. The full live run
  is `cargo xtask kill-reconstruct` (opt in with `WYRD_TIER2=1`; needs Docker),
  also run on a schedule by the new `.github/workflows/tier2-kill-reconstruct.yml`.

## Root cause

`run_kill_reconstruct` gated on `WYRD_TIER2=1` and then executed a command from
`WYRD_TIER2_CMD`; that variable is set nowhere in the repo, so the opt-in path
had nothing to run. The only coverage was the opt-in gating decision, never a
kill or a reconstruction, so the Tier-2 durability properties were unverified.

## Fix

Build the harness as real Rust that reuses the existing Tier-2 container
plumbing (`compose_up` / `resolve_endpoints` / `finish_integration` /
`finalize_panic_safe`). The scenario writes an RS(6,3) chunk across nine
servers, kills server 0, and runs the production reconstruction path twice: once
with the metadata commit dropped to model a crash mid-repair (garbage-not-
corruption), then cleanly to confirm full redundancy is restored on a spare in a
distinct failure domain, and finally re-reads and erasure-decodes the repaired
fragments to confirm byte-identity. The live container execution is kept out of
the unprivileged gate and is run by a scheduled, opt-in workflow.

## Verification

- **Claim:** the `kill-reconstruct` command no longer delegates to an undefined
  external command; it stands up the cluster, kills a server, and drives the
  production reconstruction path in-repo.
  - **Checked:** `xtask/src/faults.rs:141-204` — `run_kill_reconstruct` resolves
    the compose file, selects and names the victim, calls `compose_up`, and runs
    the scenario under `finalize_panic_safe`; the `WYRD_TIER2_CMD` shell-out is
    gone.
- **Claim:** the harness drives the real production reconstruction path, not a
  reimplementation, and asserts the three durability properties.
  - **Checked:** `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:456-757`
    — the scenario calls `reconcile_step` with a `ReconstructionContext` over the
    live gRPC fleet, then asserts garbage-not-corruption, full redundancy with the
    victim absent, distinct failure domains (`...:732`), and byte-identical
    erasure-decode of the post-repair fragments.
- **Claim:** the durability logic is covered at gate time (not only in the
  privileged run) and the coverage is load-bearing.
  - **Checked:** assertion helpers at
    `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:208`, `:241`, `:271`
    with non-ignored unit tests at `:301-411`; orchestration helpers and unit
    tests at `xtask/src/kill_reconstruct.rs:33-66`.
  - **Test:** `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` and
    `xtask/src/kill_reconstruct.rs` — these unit tests run inside `cargo xtask ci`.
    Stubbing `assert_garbage_not_corruption` to always succeed turns its two unit
    tests red; with the real implementation the gate is green. The `#[ignore]`d
    scenario is compiled and type-checked by `cargo test --workspace`, so reducing
    the harness back to a stub or an env-var string fails to compile.
- **Claim:** the unprivileged gate stays container-free; the live run is
  exercised off the required path.
  - **Checked:** `.github/workflows/tier2-kill-reconstruct.yml` — a scheduled,
    `workflow_dispatch`-able job (`WYRD_TIER2=1`) runs `cargo xtask
    kill-reconstruct` on a runner with Docker; it is not a required status check.

Fixes #196
