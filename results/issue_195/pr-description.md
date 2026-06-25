# Build the Tier-1 disk-fault harness

## Summary

The data-durability story promises that a disk returning real I/O errors is
detected, repaired, and the affected data driven back to full redundancy. That
promise had no in-repo test that exercises it against a real faulty block
device — the Tier-1 disk-fault runner only shelled out to an external command
that does not exist in the tree, so the campaign verified nothing. This change
builds the Tier-1 disk-fault harness as real Rust that drives the production
repair path over a device-mapper-faulted device.

## What to look at

- `xtask/src/faults.rs` — `run_disk_faults` no longer shells out to the absent
  `WYRD_TIER1_DISK_CMD`; it dispatches to the in-repo scenario via
  `cargo test --ignored` (gated on `WYRD_TIER1=1` + `dmsetup` present).
- `crates/custodian/tests/tier1_disk_faults.rs` — the real-device scenario. It
  opens an `FsChunkStore` on an ext4 filesystem on a device-mapper device,
  corrupts a fragment, runs scrub, switches the device to `dm-error`, evicts the
  page cache, then runs reconstruction — asserting full redundancy with no read
  error reaching the caller.
- `xtask/src/disk_faults.rs` + `xtask/tests/disk_faults_orchestration.rs` — the
  host-independent orchestration (device-mapper table plan, campaign verdict)
  and its unit tests, which run with no special privileges.
- `.github/workflows/tier1-disk-faults.yml` — the privileged nightly job that
  runs the real-device scenario where root + device-mapper are available.

To exercise locally on a Linux host with root: `WYRD_TIER1=1 cargo xtask
disk-faults`. The unprivileged orchestration coverage runs as part of the
normal gate (`./engine/xtask.sh ci`).

## Root cause

`run_disk_faults` (`xtask/src/faults.rs:114`) delegated to
`execute(plan, "WYRD_TIER1_DISK_CMD")` (`xtask/src/faults.rs:121`), an
environment-supplied command defined nowhere in the repository. No in-repo code
set up a faulted block device, drove the production custodian path over it, or
asserted the redundancy outcome, so the Tier-1 leg was inert dispatch rather
than a built harness.

## Fix

Replace the shell-out with a real harness modelled on the existing Tier-2
container harness:

- A real-device scenario test drives the **production** control point
  (`reconcile_step`, `scrub::reconcile`, `reconstruction::reconcile`) over a
  `dm-error`-backed `FsChunkStore` — it is not a parallel reimplementation of
  repair.
- The host-independent orchestration (device-mapper table plan + campaign
  verdict for both the scrub and reconstruction legs) is moved into normal
  library code and unit-tested, so the harness is exercised on every gate run.
- A nightly privileged workflow runs the real-device scenario off the merge
  gate; the unprivileged gate stays container-free and only compiles and
  type-checks the scenario.

No production custodian behaviour changes here; the block-layer read-around the
reconstruction leg depends on was delivered separately in #251, and this builds
on it.

## Verification

- **Claim:** The disk-fault runner drives a real in-repo harness, not an absent
  external command.
  - **Checked:** `xtask/src/faults.rs:114` on `main` — `run_disk_faults` now
    dispatches to the in-repo scenario; the `WYRD_TIER1_DISK_CMD` shell-out is
    gone.

- **Claim:** The harness drives the production repair path, not a shadow copy.
  - **Checked:** the scenario calls `reconcile_step`
    (`crates/custodian/src/reconciliation.rs:65`), which fans out to
    `scrub::reconcile` (`crates/custodian/src/scrub.rs:54`) and
    `reconstruction::reconcile` (`crates/custodian/src/reconstruction.rs:121`) —
    the same fenced control point the in-memory Tier-0 campaign drives.

- **Claim:** The harness is built and exercised on every run, not only on
  privileged hardware.
  - **Checked:** the orchestration helpers (table plan + campaign verdict) are
    unit-tested in `xtask/tests/disk_faults_orchestration.rs` (12 tests, no
    privileges required), and the real-device scenario is compiled and
    type-checked by `cargo test --workspace` (`xtask/src/main.rs:413`).

- **Test:** `xtask/tests/disk_faults_orchestration.rs` — the orchestration tests
  fail to build (and so go red) when the harness library helpers are removed,
  and pass once they are present; the scenario
  (`crates/custodian/tests/tier1_disk_faults.rs`) is `#[ignore]`d and runs only
  in the privileged nightly workflow, where it asserts the faulted chunk reaches
  full redundancy with no read error propagated during repair.

Fixes #195
