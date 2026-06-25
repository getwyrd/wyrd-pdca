# Build notes — issue 195 / tier1-disk-fault-harness (iteration 2)

## The two carry-forward rejections, and how this iteration answers both

**(C5, sign-off — causal adequacy):** the v1 scenario drove scrub + reconstruction over
`healthy_view`, which stripped the victim from the fleet *before* the reconstruction pass
(`iteration-v1/patch.diff`, the `healthy_view` helper + its use). So `inject_disk_fault()`
was causally inert for repair — delete it and the reconstruction half passed identically.
The fault was load-bearing only for the two `read_object` assertions. "Faulted chunk driven
back to full redundancy" was demonstrated as an ordinary survivor-only rebuild over an
*absent* server, adding nothing over the Tier-0 in-memory campaign.

**(C4, gate — no red):** the only added `tests/*.rs` file v1 shipped was the `#[ignore]`d
scenario `tier1_disk_faults.rs`. run-verify runs `cargo test --test tier1_disk_faults`;
`#[ignore]` means **0 tests run** both with and without the fix → "the test PASSES without
the fix" → no red. An `#[ignore]`d scenario can never flip at Check (it needs root+dmsetup),
so it cannot be the per-fix red→green test.

The carry-forward named the real cause: *keep the victim IN the reconstruction fleet view so
the fault drives loss classification through the production read in `reconstruction::assess`*
— and predicted that doing so exposes a production divergence: the read path tolerates a
block-layer read error, reconstruction aborts on it. I verified that prediction against
source before writing anything (below). Fixing it is what gives both a causally-adequate
harness AND a genuine Check-running red→green.

## Root cause (verified against source on the target branch)

A placed fragment whose D server hits a block-layer read fault (a `dm-error` device returns
`EIO`) is handled **inconsistently** by the two consumers:

- **Read path tolerates it** — `crates/core/src/read.rs:189` `if let Ok(Some(fragment)) =
  fetched` reads around an `Err` (and an `Ok(None)`), reconstructing from any `k` survivors.
- **`FsChunkStore` surfaces it as `Err`** — `crates/chunkstore-fs/src/lib.rs:241`
  `Err(e) => Err(e.into())` maps only `NotFound`→`Ok(None)`; an `EIO` propagates as `Err`.
- **Reconstruction aborted on it** — `crates/custodian/src/reconstruction.rs:246` was
  `store.get_fragment(frag).await?`; the `?` propagated the `Err` out of `assess` → out of
  `reconcile` → `reconcile_step` returns `Err`. One faulted disk thus stalled repair for
  **every** chunk on the shared queue. A disk that goes bad after its data lands could never
  be repaired — the exact failure a Tier-1 disk-fault harness exists to flush (ADR-0009).

## The fix (the invariant restored)

`reconstruction.rs:245-259`: classify a non-recoverable `get_fragment` error as a **missing
shard the rebuild reads around** — `store.get_fragment(frag).await.ok().flatten()` — exactly
mirroring the read path's `if let Ok(Some(_))`. An unreadable fragment (`Err`) or an absent
one (`Ok(None)`) becomes part of `missing` and is rebuilt from the `k` intact survivors,
instead of aborting the reconciliation. This is the smallest change that restores the
invariant (reconstruction tolerates a block-layer read fault precisely as the read path
does); it is behaviourally inert for every `Ok` value, so existing reconstruction tests and
the DST tier are unaffected (verified: `crates/dst/tests/custodian.rs:184` and
`network.rs` D-server stores only ever return `Ok` — the new `Err` arm is never reached
under simulation, so ADR-0009's correctness authority is untouched).

## Scope decision: I widened scope to include the production read-around. Why.

The brief's `Scope` lists "any change to production custodian / reconstruction behaviour" as
out of scope. The carry-forward explicitly re-opened this: *"widen scope, or split the fix
into its own issue and have the harness assert the corrected behaviour."* I widened, because:

1. **The Success criterion is otherwise unsatisfiable.** It is BINDING that the harness
   drives "the REAL production scrub/reconstruction path against real block-layer faults"
   and asserts "faulted chunks driven back to full redundancy with no read errors during
   repair." Pre-fix, reconstruction *aborts* on the real fault — so the end result the
   Check tests "did this work" against cannot hold. The Invariant to restore is precisely a
   tier "honoured by real, in-repo, test-exercised harness code" that exercises "something
   at Check"; a harness that can only abort is not that.
2. **Splitting makes the harness undeliverable in this slice.** The rejected alternative —
   land only the harness, file the production fix as a new issue — leaves the Tier-1
   scenario unable to go green even off-Check (it would abort on the real `EIO`), and leaves
   the Check-running test with nothing to prove (no production seam to flip). Concretely the
   split would defer the 14-line `reconstruction.rs` hunk and the 1-line classification it
   restores, in exchange for a harness that fails its own success criterion until that issue
   lands — i.e. shipping the "empty runner" the brief's Verification-posture forbids ("say
   so rather than ship an empty runner"). The carry-forward's own conclusion ("The real fix
   is to make reconstruction treat a non-NotFound get_fragment error as a missing shard")
   points the same way.

Minimalism does not govern (the brief's Invariant block, principles.md §1.3/§1.2): the
target is the smallest change that *restores the invariant*, which is the read-around hunk
plus the harness — not the smallest diff to `faults.rs`.

## What is shipped (cited on target `getwyrd/wyrd@main`, via $PDCA_WORKTREE)

1. **Production fix** — `crates/custodian/src/reconstruction.rs:245-259` (`assess`): the
   read-around. THE load-bearing change.
2. **Check-running red→green test (the flippable born-at-tier coverage)** —
   `crates/custodian/tests/reconstruction_read_fault.rs`. Drives the production
   `reconcile_step` → `reconstruction::reconcile` over a `FaultyDServer` whose `get_fragment`
   returns `EIO` once faulted (the in-memory analogue of the `dm-error` device), with the
   victim **kept in the fleet view** and its domain heavily utilized so the rebuild moves to
   the healthy spare. No root / dmsetup / GUI — pure in-memory trait stores, so it runs in
   the unprivileged container-free `cargo xtask ci`. Asserts repair to full redundancy on
   `n` distinct domains, victim no longer referenced, one version-conditional commit, and a
   degraded read that never errors.
3. **xtask harness module** — `xtask/src/disk_faults.rs` (new): the in-repo orchestration the
   Success criterion (a)/(b) require — `DmTablePlan` (dm-table planning), `setup_steps` /
   `teardown_steps` (the fault-scenario step plan), `CampaignReport` / `assert_campaign_passed`
   (the redundancy/no-read-error verdict), and the privileged `run`. Each host-independent
   helper is `#[cfg(test)]` unit-tested (13 tests) inside `cargo xtask ci`.
4. **xtask wiring** — `xtask/src/faults.rs:114-141` (`run_disk_faults` now hands off to
   `disk_faults::run`, **replacing** the `WYRD_TIER1_DISK_CMD` shell-out) and
   `xtask/src/main.rs` (`mod disk_faults;`, `pub(crate)` on `finalize_panic_safe`/`print_step`
   so the module reuses the tested panic-safe finalize).
5. **`#[ignore]`d privileged scenario** — `crates/custodian/tests/tier1_disk_faults.rs` (new):
   the real `FsChunkStore`-on-`dm-error` scenario, **fixed per the carry-forward** to keep the
   victim in the reconstruction fleet view (scrub still runs over the healthy survivors only —
   a dead `dm-error` device fails `list_fragments` and scrub treats a non-integrity I/O fault
   as transient/propagate by design, `scrub.rs:108`, so the dead disk is a *health* finding,
   not a scrub one). Compiled+type-checked at Check (`cargo test --workspace`), body runs only
   under the privileged job.
6. **Privileged off-Check CI** — `.github/workflows/tier1-disk-faults.yml` (new): nightly +
   dispatch, `WYRD_TIER1=1`, `sudo cargo xtask disk-faults`, kept out of `ci` (ADR-0016).

## Red→green (the project's C4-verify gate, isolated `../wyrd-verify` worktree)

`PDCA_BUNDLE=… engine/scripts/run-verify.sh` → **PASS — red without the fix, green with it.**
- GREEN with the patch: `cargo test -p wyrd-custodian --test reconstruction_read_fault --test
  tier1_disk_faults` → reconstruction_read_fault passes, scenario ignored.
- RED with `reconstruction.rs` reverted (the added tests kept): reconstruction_read_fault
  **panics** at `:344` — `reconstruction must read around the faulted fragment, not abort on
  its EIO: Store(Os { code: 5, kind: Uncategorized, message: "Input/output error" })`. The
  fault is load-bearing and the production seam is proven necessary.

Born-at-tier flippability of the xtask harness logic (Success criterion (b)): demonstrated red
by stubbing `assert_campaign_passed`→`Ok(())` makes the three `campaign_fails_*` unit tests
fail (re-verified this iteration; reverted).

## Commit-readiness
- `cargo fmt --all -- --check` clean (rustfmt applied to the two new test files).
- `cargo clippy -p wyrd-custodian -p xtask --all-targets` clean (`clippy::all = deny`).
- Full `cargo test -p wyrd-custodian` green (24 tests incl. the 5 pre-existing reconstruction
  tests — no regression) plus `cargo test -p xtask` green (26, incl. 13 new harness tests).
