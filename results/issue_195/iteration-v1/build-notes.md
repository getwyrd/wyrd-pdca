# Build notes — issue 195 / tier1-disk-fault-harness

## What the brief asked for (the real end result)

Replace the inert `WYRD_TIER1_DISK_CMD` external-command shell-out in `xtask/src/faults.rs`
with a **real, in-repo, test-exercised** Tier-1 disk-fault harness that drives the
**production** custodian repair path over a device-mapper (`dm-flakey`/`dm-error`) faulted
block device. Invariant to restore (proposal 0005 §13.2, `0005:405-408`, `0005:437`): a
deferred/off-Check tier means its *green is observed off-Check*, NOT that the deliverable is
unbuilt — the harness must exist and be exercised by something at Check.

This is infrastructure/net-new work, so minimalism does not govern (principles.md §1.3): the
target is a harness that actually runs the scrub/checksum + reconstruction path against real
block-layer faults, mirroring the verified Tier-2 *container* precedent.

## What I built (cited against the worktree off `getwyrd/wyrd@main`)

1. **`xtask/src/disk_faults.rs` (new) — the harness module.** Contains all the
   host-independent orchestration logic the brief names, each unit-tested:
   - `DmTablePlan` (`disk_faults.rs:81-136`) — **device-mapper table planning**: pure
     `linear` / `error` / `flakey` table-string construction. The live run creates the
     device with the healthy linear table, then reloads to the fault table.
   - `DiskFaultKind` (`disk_faults.rs:38-58`) — picks `dm-error` (default, always-erroring)
     vs `dm-flakey` from `WYRD_TIER1_FAULT`; default-to-stricter so a typo never weakens it.
   - `setup_steps` / `teardown_steps` (`disk_faults.rs:166-191`) — **the fault-scenario
     steps** as planned `HarnessStep` data (create→mkfs→mount; umount→remove→detach),
     unit-testable without spawning anything.
   - `CampaignReport` + `parse` + `assert_campaign_passed` (`disk_faults.rs:208-261`) — the
     **post-repair redundancy / no-read-error verdict** (`0005:381-384`): passes iff ≥1
     faulted chunk was exercised, every faulted chunk returned to full redundancy, and zero
     reads errored during repair.
   - `run` (`disk_faults.rs:~315`) — the privileged orchestration: sparse backing file →
     `losetup` attach → execute setup steps → run the `#[ignore]`d scenario with the roots /
     error-table / report path exported → parse + assert the report → panic-safe teardown
     (reuses `crate::finalize_panic_safe`, the same primitive the Tier-2 runner uses).

2. **`xtask/src/faults.rs` — `run_disk_faults` reworked** (`faults.rs:114-135`). Keeps the
   pure gating (`plan`: deferred / missing-tool / run) and, on `Run`, hands off to
   `disk_faults::run()` — **replacing** `execute(..., "WYRD_TIER1_DISK_CMD")`. The Jepsen +
   Tier-2 legs still use the env-supplied `execute` path (Jepsen is split to #250 per brief).

3. **`xtask/src/main.rs`** — `mod disk_faults;` and `pub(crate)` on `finalize_panic_safe` /
   `print_step` so the new module reuses the tested panic-safe finalize instead of
   duplicating it.

4. **`crates/custodian/tests/tier1_disk_faults.rs` (new) — the `#[ignore]`d scenario.**
   Attributed exactly like `tier2_integration.rs`
   (`#[ignore = "Tier-1: needs root + device-mapper — run via cargo xtask disk-faults"]`).
   It opens real `FsChunkStore` D servers (`FsChunkStore::open`, victim rooted on the faulted
   mount), writes an RS(2,1) object over a placement-aware `FsFleet`, injects the real block
   fault (dmsetup suspend→load→resume to the exported error table), then drives the
   **production** path — `reconcile_step` → `scrub::reconcile` / `reconstruction::reconcile`
   (`crates/custodian/src/reconciliation.rs:65`, `scrub.rs:54`, `reconstruction.rs:121`) —
   and asserts: chunk back to full redundancy on N distinct domains, victim no longer
   referenced, exactly one version-conditional commit, and **zero read errors during repair**
   (degraded reads succeed off the k survivors via `read_object`). Writes the campaign report.

5. **`.github/workflows/tier1-disk-faults.yml` (new) — the privileged off-Check CI job.**
   Modelled on `integration-nightly.yml`: nightly + `workflow_dispatch`, `WYRD_TIER1=1`,
   loads dm targets, runs `sudo cargo xtask disk-faults`, uploads the campaign report. Kept
   out of the unprivileged container-free `cargo xtask ci` (ADR-0016).

## Why this shape (and what I ruled out)

- **Why drive the real `reconcile_step` and not a parallel repair (ADR-0009).** The harness
  must verify production behaviour, so it traverses the same fenced control point the Tier-0
  DST campaign drives (`crates/dst/tests/custodian.rs`). A parallel reimplementation would
  verify nothing. Rejected.
- **Why the verdict lives in xtask, not only in the test.** Success criterion (a) requires
  the redundancy/no-read-error assertion to be *xtask harness logic* unit-tested at Check.
  The scenario self-asserts (real Rust) **and** emits a `key=value` report; xtask parses +
  applies `assert_campaign_passed` as the orchestration-level verdict — so a scenario that
  somehow exits 0 while reporting a shortfall is still caught. Cost of the alternative
  (sharing one helper crate-importably): the harness logic would have to leave xtask into a
  lib crate, which contradicts criterion (a)'s "xtask contains the harness module". A trivial
  `key=value` report (3 ints) is cheaper and keeps the verdict in xtask. The error-table
  string the scenario loads is produced by `DmTablePlan` in xtask and **handed to the test
  via env** (`WYRD_TIER1_DM_ERROR_TABLE`), so `DmTablePlan` stays the single dm-table source.
- **Why write-then-reload-to-fault** (not an always-faulted device): an always-erroring
  device can't be written to first, so there'd be nothing to repair. Create healthy (linear),
  write, then atomic table swap to the fault target is the faithful "disk goes bad after the
  data lands" model.

## Verification (red→green via the project's cargo test runner)

The flippable born-at-tier coverage is the xtask `#[cfg(test)]` unit tests in `disk_faults.rs`
(they run inside `cargo xtask ci`'s `cargo test`). Run with `cargo test -p xtask` (scoped
slice of the gate's test step; the full gate is `./engine/xtask.sh ci`, whose 50-seed DST
sweep is impractical as a quick sanity pass).

- **Demonstrated red:** stubbed `assert_campaign_passed` to `Ok(())` →
  `campaign_fails_on_any_read_error_during_repair`, `campaign_fails_when_a_chunk_is_left_under_replicated`,
  and `campaign_fails_when_nothing_was_exercised` all FAILED (3 failed). Reverted.
- **Green after:** `cargo test -p xtask` → 26 passed (13 new `disk_faults::tests`).
- **Scenario compiles/type-checks at Check:** `cargo test -p wyrd-custodian --no-run` builds
  `tier1_disk_faults` — it is real, API-bound Rust against the production
  `FsChunkStore`/`reconcile_step`/`scrub`/`reconstruction` surface (a stub or the old shell
  string would fail to compile). Its `#[ignore]`d body runs only in the privileged job.
- **Commit-ready:** `cargo fmt --all -- --check` clean; `cargo clippy -p xtask -p
  wyrd-custodian --all-targets` clean. No production crate touched, so the rest of
  `cargo xtask ci` is unaffected.

## Off-Check (NOT the Check-gating condition)

The real privileged green (root + `dmsetup`, real `dm-flakey`/`dm-error` faults) is confirmed
by the new `tier1-disk-faults.yml` job (maintainer: Eduard Ralph, INTEGRATION §10), not at
Check (ADR-0016). Privileged-only details the maintainer's run validates: page-cache vs
on-device reads after the table swap (the scenario does a best-effort `drop_caches`), and the
exact dm target availability on the runner.
