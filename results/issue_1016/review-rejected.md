# Recorded rejections for #1016 (625.2), iteration 5: three decisions.
#
# The iteration-4 batch (review-batch.md) had three groups of findings. All three are FIXED in
# this iteration and need no row here:
#   - custodian.rs:733, the unbiased wait (BUG): the wait is `biased`, the stop first
#     (custodian.rs:739-743); driver test custodian_gc.rs:2254.
#   - custodian_gc.rs:1998, the fault model (TEST-GAP): a failed commit answers
#     CommitUnknownResult { may_still_commit: false } (custodian_gc.rs:2024-2046); the test
#     asserts both draws and at least one reaper `reaper-fence-outcome-unknown` (:2120, :2125).
#   - multipart_reaper_deployed.rs:801 / :387, the parent-side pass count (TEST-GAP): the role is
#     stopped with SIGSTOP, a mark is written into its own stderr while every thread is stopped,
#     and SIGINT is sent before SIGCONT (interrupt_frozen, :388). Where the role stood is read off
#     the lines before the mark (:943-962). The parent-side count is gone.
#
# What stays declined:
#
# 1. "A pass over redb has no time bound / the await on redb's store work is unbounded" (BUG,
#    phrase "synchronous"). Declined as a code change. No pass has a deadline as a whole, on any
#    backend: iteration 2's whole-pass timeout was removed in iteration 3 because cutting a pass
#    short threw away its progress, so a slow store could keep later uploads and the drain from
#    ever being reached. Every pass is bounded instead by what it may do — the reaper's and the
#    drain's per-pass budgets (625.1) — and, on TiKV and FoundationDB, by each store call's own
#    operation deadline in the adapter (crates/metadata-tikv/src/lib.rs:1051-1066 for the
#    connect, crates/metadata-fdb/src/lib.rs:78-89), the store open included. redb answers each
#    call synchronously on local disk; no in-process timer can cut a thread short inside such a
#    call, and every other redb pass the role runs (the fleet loop, both one-shots) has the same
#    bound. The docs claim no time bound (custodian.rs:679-692, 06-runtime-view.md:94).
#
# 2. "No seeded Tier-0 DST coverage for the new loop" (TEST-GAP, phrase "DST"). Decided at the
#    iteration-4 sign-off: "no DST rows here". Tier-0 (madsim, crates/dst) is declined: brief.md's
#    Ordering note forbids this slice to edit anything under crates/dst/, which child-3 of #625's
#    split owns, and crates/dst does not depend on wyrd-server, where the driver lives. What
#    covers the driver instead, all on the real CustodianService::run_until (custodian.rs:694):
#    - seeded coverage in crates/server/tests/custodian_gc.rs (wyrd_testkit::Sim, as
#      dst_commit.rs): no_fleet_loop_rides_out_seeded_store_faults_to_leg_a_teardown (:2063), 32
#      seeds of store faults, a failed commit applied or not and answered CommitUnknownResult;
#      no_fleet_loop_stops_when_a_newer_term_holds_the_zone (:2136);
#      fleet_loop_polls_its_stop_before_its_first_pass (:2185);
#      a_stop_ready_with_the_interval_timer_starts_no_further_pass (:2254), on a paused clock;
#    - the first-pass interruption through the real binary and a real SIGINT:
#      no_endpoints_role_finishes_its_first_pass_when_stopped_during_it
#      (multipart_reaper_deployed.rs:934).
#    The patch adds no reaper or drain logic: every pass it schedules is the reconcile_step call
#    625.1 already covers in DST.
#
# 3. "The empty-fleet role makes one pass a start, so leg A's transition is only reached across
#    restarts" (phrase "restart"). Decided at the iteration-4 sign-off: "one pass per empty-fleet
#    invocation with restart-driven convergence". The brief asks for exactly one pass before the
#    panic; leg A's teardown takes three passes; G2c checks every start for exactly one pass, the
#    panic's status and the diagnostic, and reaches the transition across starts
#    (multipart_reaper_deployed.rs:1017).
crates/server/src/cli.rs:1211 | BUG | synchronous | Decision 1: no whole-pass deadline on any backend (removed in iteration 3: it discarded progress); passes are held to the per-pass budgets and, on TiKV/FDB, each call's adapter deadline; redb's synchronous local-disk calls cannot be cut short by any in-process timer, as for every other redb pass the role runs.
crates/server/src/cli.rs:1313 | BUG | synchronous | Same decision as cli.rs:1211 (the empty-fleet pass's open).
crates/server/src/cli.rs:1951 | BUG | synchronous | Same decision as cli.rs:1211 (open_no_fleet_meta, the no-fleet store open).
crates/server/src/custodian.rs:694 | BUG | synchronous | Same decision as cli.rs:1211 (run_until, the one interval driver).
crates/server/src/custodian.rs:756 | BUG | synchronous | Same decision as cli.rs:1211 (the no-fleet pass in the driver).
crates/server/tests/multipart_reaper_deployed.rs:831 | TEST-GAP | DST | Decision 2 (sign-off: "no DST rows here"): brief.md forbids edits under crates/dst/ (child-3 of #625 owns it); seeded Tier-1 coverage of the driver is in custodian_gc.rs (:2063, :2136, :2185, :2254) and the first-pass SIGINT case runs through the real binary (:934).
crates/server/tests/multipart_reaper_deployed.rs:934 | TEST-GAP | DST | Same decision as multipart_reaper_deployed.rs:831 (the first-pass interruption).
crates/server/tests/multipart_reaper_deployed.rs:1017 | TEST-GAP | DST | Same decision as multipart_reaper_deployed.rs:831 (G2c).
crates/server/src/custodian.rs:545 | TEST-GAP | DST | Same decision as multipart_reaper_deployed.rs:831 (run_reaper_until).
crates/server/src/custodian.rs:714 | TEST-GAP | DST | Same decision as multipart_reaper_deployed.rs:831 (the stop polled before the first pass).
crates/server/src/custodian.rs:739 | TEST-GAP | DST | Same decision as multipart_reaper_deployed.rs:831 (the wait, the stop first).
crates/server/src/cli.rs:1311 | TEST-GAP | DST | Same decision as multipart_reaper_deployed.rs:831 (the empty-fleet pass).
crates/server/tests/multipart_reaper_deployed.rs:1017 | TEST-GAP | restart | Decision 3 (sign-off: "one pass per empty-fleet invocation with restart-driven convergence"): every start is checked for exactly one pass; the transition is reached across starts.
crates/server/src/cli.rs:1311 | BUG | restart | Same decision as multipart_reaper_deployed.rs:1017 (the empty-fleet pass: exactly one, then the unchanged panic).
