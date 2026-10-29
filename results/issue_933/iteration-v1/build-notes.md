# Build notes — #933 (657.1b): in-flight renewal of the staging leases

Base: the cycle worktree at `93a6f37` ("pdca-integrate: issue_932", the folded 657.1a on
`pdca-integration/main-r42c5348f203d`); target `getwyrd/wyrd@main`. Every `path:line` below is on
the patched worktree unless it says "base" or cites 0016. The reference implementation read was
`results/issue_926/iteration-v3/patch.diff` only.

## What changed

**`crates/core/src/multipart/stage.rs`** (production)

- Module docs: renewal row in the batch table (`:9-14`); *Leases* extended to renewals (`:73-91`);
  new *Renewal* section (`:93-133`): when renewal runs, the armed/disarmed deadline rule, what an
  abandoned renewal is reported as, what a failing renewal does to pending work (`:121-124`), the
  sampled-lease rule and its timing assumptions, the `3 + 2N` batch size and why it is clamped. Bounds
  table and the one-clock section updated; DST deferral (#660, #665) kept and widened.
- `RENEWAL_ROUNDS = 4` (`:237`), `ABANDONED_COMMIT` (`:243`).
- `StageRequest::check` refuses a `max_part_chunks` whose renewal batch would pass `MAX_BATCH_OPS`
  (`:314-318`), shared by `stage_part` and `reserve_slot` as 657.1a's check is.
- `StageClock` split out of `StageRuntime` (`:329-345`), v3's shape: the quiesce renewal needs only
  the clock.
- `RenewOutcome::{Covered { checked_at_millis, lease_expiry_millis }, Refused}` (`:362-377`).
- `OwnedChunk::with_lease` (`:410`); `StagedAttempt` keeps the request's TTL (`lease_ttl_millis`
  field, getter `:526`) and gets `renew(store, clock, cover_millis)` (`:550-580`).
- `Stop::RenewalRoundsSpent` (`:648`), `Stop::CoverShort` (`:655`), Display and `source` arms.
- `RenewalLedger` + `abandoned` (`:1000-1030`): the renewal's doubt and an "awaiting a commit" flag
  kept outside the renewal future, so a renewal dropped at a deadline is reported as InDoubt with
  `may_still_commit` (staging's own `ABANDONED_COMMIT` report unless the store had already said
  `may_still_commit`), and one cut between commits keeps exactly its rounds' doubt.
- 657.1a's free `bounded` wait is replaced by `Raced`/`Woke` (`:1098-1112`) and
  `Stager::renewing` (`:1845-1933`), which keeps `bounded`'s rules (answer taken as it comes,
  `Late::Taken`/`Late::Refused`, early wake waited out, work never restarted) and adds the renewal
  instant to the wait (`:1874`) and a race of renewal/work/deadline timer once a renewal is due.
  Once the work has answered the deadline is disarmed and the renewal is awaited to its end
  (`:1889-1893`).
- `Footprint::renewal`, `renewal_batch`, `renew`, `apply` (`:1531-1708`): the round loop, ported
  from v3 with: a cover check for the quiesce renewal; doubt released after a landed round (every
  other round required the slot bytes it replaced); a session-moved adoption applied before
  `moved_on` so a refusal leaves the attempt holding what landed.
- `Stager`: `renew_at` (`:1766`), `expired` (`:1798`), `renewed` (`:1806`), `renew_if_due`
  (`:1812`), body reads through `renewing` (`:1956`), `renew_if_due` before each intent (`:2016`),
  `write_fragments` takes the landed intent and races renewal (`:2034`, `:2076`, `:2110`, `:2118`).

**`crates/core/src/multipart.rs`**: `RENEWAL_FIXED_OPS = 3` and `renewal_batch_ops` (`:5180-5192`),
the one count both clamps use; `KnobClamp::RenewalOverOps` (`:5465-5469`); the clamp in
`knob_clamps_hold` right after `PartCommitOverOps` (`:5539-5542`); docs of `MAX_PART_CHUNKS`
(`:5034-5036`) and `SlotRecord::with_lease` (`:2777-2780`).

**`docs/design/architecture/05-building-block-view.md:246`**: 657.1a's paragraph extended
(renewal, its deadline rule, lapse refusal, `B_ops`, the quiesce renewal). `:244` untouched.

**Tests**: legs appended to `crates/core/tests/multipart_upload_part_stage.rs` (`:4343-6180`); clamp
rows in `crates/core/tests/multipart_knobs.rs`.

## Design decisions, and what I ruled out

1. **One wait for the renewal instant and the deadline** (`stage.rs:1874`,
   `min(renew_at, deadline) - now`), as v3 did, rather than two timers. Two timers through the
   harness's single-semaphore timer would hand a fired permit to whichever wait queued first, so
   the harness timer would need per-wait channels (my estimate: about 40 lines) and every 657.1a
   wait assertion would gain an entry. The one wait changes only the three `[IDLE, W]` assertions
   named below.
2. **Work polled before the renewal in the race** (`stage.rs:1899-1909`). v3 polled the renewal
   first. Polling work first means the writes are always dispatched in the race's first poll, and an
   answer the work already has is never lost when the renewal finishes in the same poll.
3. **The quiesce renewal takes its TTL from the attempt** (stored at staging, `stage.rs:1999`)
   instead of v3's `lease_ttl_millis` argument. One TTL per lease lifecycle, and no way for #927 to
   pass a different one. Cost: one field and a getter.
4. **Cover rule**: success iff `lease_expiry - checked_at >= max(cover_millis, 1)` (`stage.rs:1692`):
   the `max(…, 1)` keeps `cover_millis = 0` from accepting a lease that expires exactly at the
   reading (the reaper's lapse boundary is `<=`). When the rounds run out and at least one landed
   round fell short, the cause is `CoverShort` (the leases did move); otherwise
   `RenewalRoundsSpent`.
5. **In-flight renewal takes a landed round without a post-commit check** (the brief's crossing-
   expiry rule). If its new lease had lapsed by the time the commit returned (commit took more than
   a TTL), the next renewal round's own lapse check refuses it.
6. **A renewal that fails while work is pending stops staging at once** with the renewal's own
   error (documented at `stage.rs:121-124`): a pending read is dropped, pending writes are abandoned
   on positions the durable owned entry names. I did not make it keep waiting for the writes to
   their deadline: that needs the race to keep polling work after the renewal's `Err` and a way to
   report two causes in one `StageError` (my estimate: about 20 lines plus a new error shape). It
   would also delay the error of an attempt that is already dead by up to `W_write`. **For the
   human:** the error then reports the renewal's cause (for example `LeaseLapsed`, classed
   `Terminal`, "no batch … in doubt"), not the write's unknown effect. "Batch" here means metadata
   batches, and the bytes stay covered, but if you want the write's `Indeterminate` class to win,
   that is a follow-up.
7. **Clamp order**: renewal checked after the part commit (`multipart.rs:5537-5542`), so a set that
   breaks both is still named `PartCommitOverOps`, as before. The rows were moved anyway (below), so
   neither choice is load-bearing for the tests.
8. **Harness request fixture**: `request()` used `max_part_chunks: 1_000`, which the new staging
   check refuses (`3 + 2000 > 500`), so it is now `MAX_PART_CHUNKS` (158)
   (`multipart_upload_part_stage.rs:236`). No 657.1a leg stages more than five chunks; G sets its own
   bound of 3.

**Not edited, for the human:** proposal 0016's knob table (`0016:1462-1479`) has no row for the
renewal clamp (`3 + 2 × MAX_PART_CHUNKS ≤ B_ops`). The brief forbids editing 0016 here. Decide
whether 0016 needs the row.

## 657.1a legs updated (each asserted the absence of renewal)

- `a_prime_a_write_pending_past_its_deadline_is_abandoned` (`multipart_upload_part_stage.rs:1828`):
  waits `[IDLE, W]` → `[HALF, W]`. The first body read's wait now runs to the renewal instant.
- `a_prime_a_put_refused_as_late_is_reported_as_nothing_landed` (`:1933`): same change.
- `a_prime_an_ok_ready_as_the_deadline_timer_fires_is_counted_written` (`:2178`): same change on
  `waits()[..2]`.
- `a_double_prime_the_idle_bound_is_per_read_not_per_body` (`:2342`): `req.lease_ttl_millis = 4 *
  TTL`, so no renewal falls due inside the leg. Its exact `armed(n)` syncs and its waits
  `[BOUND, BOUND, 1, BOUND, 1]` assumed no renewal; with the default TTL a renewal at `T0+9998` adds
  two waits and the leg timed out at `armed(5)`. The idle bound beside renewals is covered by
  `r3_the_body_idle_bound_holds_while_renewals_keep_the_leases`.

Every other 657.1a leg passes unchanged.

**Harness additions, not legs:** `Kind::Renewal` (`:328`, classified by what it puts, so a
renewal missing its slot rewrite is still a renewal); `Event::{Gated, ReadGated, Delivered}`
(`:420-425`); `Log::{landed, gated, reads_gated, acked, delivered}` (`:515-546`);
`Act::Gated(gate, act)` (`:643`, `:850`), a commit that waits at a gate, and if its caller drops it
there, it goes to the held queue unapplied (`HeldIfDropped`, `:668`), so a test can later land an
abandoned commit; `Then::Gate` (`:663`, `:794`); `Scripted::held_count` (`:737`); the fake pushes
`Delivered` once a put's answer reaches staging (`:1167`); `impl StageClock for Runtime` (`:1190`).

## `multipart_knobs.rs` rows changed (Leg 2)

- `RENEWAL_FIXED_OPS = 3` counted from the batch shape (`:60-64`); the unused `part_ops` binding is
  gone.
- On-bound part-commit row `with(D, |k| k.batch_ops = part_ops) => Ok(())` (base `:200`) → `with(tiny,
  |k| k.batch_ops = tiny_part_ops) => Ok(())` (`:224`), `tiny = derived(TINY)`, where the part
  commit (9 ops) binds over the renewal (5).
- Base `:242` `with(D, |k| k.batch_ops = part_ops - 1) => Err(PartCommitOverOps)` → `with(tiny, …
  tiny_part_ops - 1)` (`:269`).
- Base `:243` `with(D, |k| k.batch_ops = MAX_PART_CHUNKS - 1) => Err(PartCommitOverOps)` → `with(tiny,
  |k| k.batch_ops = TINY.max_part_chunks - 1)` (`:270`).
- New: `with(D, |k| k.batch_ops = renewal_ops) => Ok(())` (319 ops, `:225`); `with(D, … renewal_ops -
  1) => Err(RenewalOverOps)` (`:273`); the adversary's set
  `derived(with(D, scheme = None, max_part_chunks = N))` at 248 `=> Ok(())` (`:226`) and 249
  `=> Err(RenewalOverOps)` (`:274`), with `3 + 2 × 249 == MAX_BATCH_OPS + 1` asserted (`:217`).

## Demonstrated reds

Each is one temporary edit to production, the named legs run through
`cargo test -p wyrd-core --test <file> -- <filters>`, then the file restored (a diff hash before and
after matched). The script is reproduced in the appendix; it was run on the final files.

| # | Edit (brief's label) | Legs run → result |
|---|---|---|
| M1 | renewal removed entirely: `renew_at = reserved_at + u64::MAX` (657.1a's behaviour) | all 7 `r2_*` FAILED |
| M2 | slot rewrite dropped from `renewal_batch` | `r1_a_renewal_rewrites_the_slot_with_every_owned_lease` FAILED; `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` FAILED (the fence committed) |
| M3 | renewal checked only at the start of a read, wait = deadline only | `r2_d_i_a_renewal_commits_while_a_body_read_is_pending` FAILED |
| M4 | no renewal raced against writes (`Late::Taken` waits only the deadline) | `r2_d_ii_a_renewal_commits_while_a_fragment_write_is_pending` FAILED |
| M5 | `renew_if_due` called on every intent round | `r2_no_renewal_is_sent_while_an_intent_is_in_flight` FAILED (`r2_a_renewal_due_as_an_intent_starts_is_sent_before_it` still passes: the renewal is still before the intent commit) |
| M6 | renewal awaited first (`break Some((renewal.await, None))`) | `r3_the_write_deadline_cuts_off_a_renewal_still_committing`, `r3_a_write_answered_while_a_renewal_commits_is_taken` FAILED |
| M7 | `RenewalLedger::abandoned` ignores `awaiting` | `r3_the_write_deadline_cuts_off…`, `r3_a_renewal_cut_off_between_its_commits_keeps_exactly_its_doubt` FAILED |
| M8 | a landed renewal re-arms the body-idle deadline | `r3_the_body_idle_bound_holds_while_renewals_keep_the_leases` FAILED |
| M9 | `check_live` removed from the renewal round | `r4_a_renewal_refuses_a_lapsed_slot_lease`, `r4_a_renewal_refuses_a_lapsed_owned_lease`, `r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds`, `r7_the_quiesce_renewal_refuses_a_lapsed_lease` FAILED |
| M10 | retry reuses the first round's reading | `r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds` FAILED |
| M11 | `doubt.release()` instead of `doubt.settle()` after an absent slot read | `r5_a_held_renewal_that_never_settles_ends_in_doubt` FAILED |
| M12 | `rounds += 0` | `r6_the_renewal_loop_is_bounded` FAILED (the wrapper's runaway-commit guard) |
| M13 | `StagedAttempt::renew` passes a one-reading clock | `r7_the_quiesce_renewal_reads_the_callers_clock_every_round`, `r7_the_quiesce_renewal_refuses_a_lapsed_lease` FAILED |
| M14 | cover check made always true | `r7_the_quiesce_renewal_covers_its_hand_off_or_refuses` FAILED (both runs' assertions) |
| M15 | write deadline kept armed after the writes answered | `r3_a_renewal_outliving_an_answered_writes_deadline_is_awaited` FAILED |
| M16 | in-flight renewal re-checks the old leases after its commit returns | `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` FAILED |
| M17 | `RenewalOverOps` clamp removed from `knob_clamps_hold` | `leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it` FAILED |
| M18 | staging's renewal check disabled (`false && …`) | `r8_a_chunk_bound_past_a_renewal_batch_is_refused` FAILED |

M15 did not go red in my first pass. That leg fired the timer and released the renewal gate in the
same driver poll, so the renewal won before staging could look at the deadline. I fixed the leg, not
the code: it now waits for `Event::Delivered`, which proves staging has taken the put's answer. After
firing the timer it asserts that no wait is armed (`timer.available_permits() == 1`). It also asserts
that the renewal which landed is the gated one (`clock == T0 + HALF`) and that nothing was abandoned
(`held_count() == 0`). I strengthened the adversary leg the same way.

**657.1a's base.** The new legs do not compile there (`RenewOutcome`, `StageClock`,
`RENEWAL_ROUNDS`, `ABANDONED_COMMIT`, `KnobClamp::RenewalOverOps` don't exist). M1 is the
functional stand-in for "no renewal at all", and M17 for "the adversary's knob set passes".

## Refuting my own tests

- **(a) Genuine red?** Yes. The 18 edits above each turned their named legs red on the final files.
  The leg that first stayed green under M15 was fixed until it went red.
- **(b) Production path?** Yes. The legs call `wyrd_core::multipart::stage::stage_part`,
  `reserve_slot` and `StagedAttempt::renew`, and the knob rows call
  `wyrd_core::multipart::knob_clamps_hold`. The store is `RedbMetadataStore::in_memory()` behind a
  wrapper that delegates every read and commit to it. `renewals()` only inspects batches production
  sent; no renewal logic is copied into the tests. The op count `3 + 2N` is counted separately
  from the batch shape in both test files, not imported from `renewal_batch_ops`.
- **(c) Fixture includes the fault?** Yes. A held renewal is a real pending commit future
  (`Act::Gated`); dropped at its gate, it is kept and later applied to the real store, which proves
  `may_still_commit` (`r3_the_write_deadline_cuts_off…` lands it and sees the slot move). The held
  put waits inside the fake's publication path, which enforces the deadline at publication. Lapses
  are the real clock moved past the real stored lease. The reaper's fence is a real commit against
  the real store, and Abort is the real `abort_session`.

## C5: `cargo mutants --in-diff` (advisory)

Command (the gate's flags, plus `-j 4` and output in scratch):
`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true -j 4`.
Result: **64 mutants, 51 caught, 12 unviable, 1 missed.**

- **Missed: `stage.rs:315` `>` → `>=` in `StageRequest::check`.** Equivalent at the shipped
  constants. `renewal_batch_ops(N) = 3 + 2N` is always odd and `MAX_BATCH_OPS` is 500 (even), so
  no chunk bound makes the two sides equal. The knob clamp's `<=` (`multipart.rs:5540`) is not
  equivalent, because `batch_ops` can be any value; the on-bound row (`multipart_knobs.rs:225`)
  kills `<=` → `>`.
- **Unviable (12):** each replaces a return value with `Default::default()` (or `Raced::new()` and
  similar) for a type that has none: `OwnedChunk::with_lease`, `StagedAttempt::renew`,
  `StageError::source`, `RenewalLedger::abandoned`, `Footprint::renewal`, `Footprint::renew`,
  `Stager::expired`, `Stager::renewing` (×4), `Stager::run`. They don't compile, so they are not test
  gaps.

## Gates run

- `cargo test -p wyrd-core --test multipart_upload_part_stage --test multipart_knobs`: 87 + 8 pass;
  the staging file 15 times in a row with no failure.
- `cargo fmt --all -- --check` and `cargo clippy -p wyrd-core --all-targets`: clean.
- `./engine/xtask.sh ci` (`cargo xtask ci`, with `PDCA_WORKTREE` set): "xtask ci: all checks passed",
  exit 0. That covers typos, docs lint and render (the brief's `typos` and `docs-renderer`
  dependencies were present), fmt, clippy, build, workspace tests, deny, the statics gate and DST.
  It was run twice: once before the last doc-only edits to `stage.rs`, and again on the final tree,
  the one `patch.diff` holds. Both runs ended "xtask ci: all checks passed", exit 0.
- `git apply --check -R patch.diff` on the worktree: clean, so the patch is exactly the tree that
  was gated.

No external dependency was missing, so there is no NEEDS-HUMAN marker.

C4-verify: as the brief says, there is no added test file, so C4-verify takes its green-only branch.
The per-fix red is the table above.

## Appendix: the demonstrated-red script

Run as `python3 -I reds.py <worktree> <results-file> [M1 …]` from outside the worktree.

```python
#!/usr/bin/env python3
"""Demonstrated reds for #933: apply one named edit to production, run the named legs, restore.

Usage: reds.py <worktree> <results-file> [mutant-id ...]
Every edit must match exactly once; the original file is always restored.
"""
import pathlib
import subprocess
import sys

WT = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
ONLY = set(sys.argv[3:])
STAGE = "crates/core/src/multipart/stage.rs"
MP = "crates/core/src/multipart.rs"
STAGE_T = "multipart_upload_part_stage"
KNOBS_T = "multipart_knobs"

FIXED_CLOCK = """

/// MUTANT: a clock that only ever gives one reading.
struct FixedReading(u64);

impl StageClock for FixedReading {
    fn now_millis(&self) -> u64 {
        self.0
    }
}
"""

MUTANTS = [
    ("M1", "renewal removed entirely (657.1a's behaviour): the renewal instant never comes",
     [(STAGE, "            .reserved_at_millis()\n            .saturating_add(request.lease_ttl_millis / 2);",
       "            .reserved_at_millis()\n            .saturating_add(u64::MAX);")],
     [(STAGE_T, ["r2_"])]),
    ("M2", "the slot rewrite dropped from the renewal batch",
     [(STAGE, "            .require(self.slot_key.clone(), self.slot.clone())\n            .put(self.slot_key.clone(), renewal.slot.clone());\n        for (held, next)",
       "            .require(self.slot_key.clone(), self.slot.clone());\n        for (held, next)")],
     [(STAGE_T, ["r1_", "r2_a_renewal_that_crosses_expiry"])]),
    ("M3", "renewal checked only between delivered pieces, with no timer",
     [(STAGE, "            started = true;\n            if now < self.renew_at {\n                let wake = self.renew_at.min(deadline) - now;",
       "            let entered = !started;\n            started = true;\n            if !entered || now < self.renew_at {\n                let wake = deadline - now;")],
     [(STAGE_T, ["r2_d_i_"])]),
    ("M4", "the timer not raced against a pending fragment put",
     [(STAGE, "            started = true;\n            if now < self.renew_at {\n                let wake = self.renew_at.min(deadline) - now;",
       "            started = true;\n            let renews = late == Late::Refused;\n            if !renews || now < self.renew_at {\n                let wake = if renews { self.renew_at.min(deadline) } else { deadline } - now;")],
     [(STAGE_T, ["r2_d_ii_"])]),
    ("M5", "a renewal sent while the attempt's own intent is in flight (renewal checked every intent round)",
     [(STAGE, "            rounds += 1;\n            // Plan against the topology minus every server whose drain is recorded, read afresh",
       "            rounds += 1;\n            self.renew_if_due(store, runtime).await?;\n            // Plan against the topology minus every server whose drain is recorded, read afresh")],
     [(STAGE_T, ["r2_no_renewal_is_sent_while_an_intent_is_in_flight", "r2_a_renewal_due_as_an_intent_starts_is_sent_before_it"])]),
    ("M6", "the renewal awaited before the pending work instead of raced against it",
     [(STAGE, "                loop {\n                    if answered.is_some() {",
       "                loop {\n                    if true {\n                        break Some((renewal.as_mut().await, None));\n                    }\n                    if answered.is_some() {")],
     [(STAGE_T, ["r3_the_write_deadline_cuts_off_a_renewal_still_committing", "r3_a_write_answered_while_a_renewal_commits_is_taken"])]),
    ("M7", "a renewal still committing at the deadline reported as clean instead of in doubt",
     [(STAGE, "        if !self.awaiting {\n            return self.doubt.end(upload_id, cause);",
       "        if true {\n            return self.doubt.end(upload_id, cause);")],
     [(STAGE_T, ["r3_the_write_deadline_cuts_off_a_renewal_still_committing", "r3_a_renewal_cut_off_between_its_commits"])]),
    ("M8", "renewal deferring the body-idle bound (a landed renewal re-arms the idle deadline)",
     [(STAGE, "        deadline: u64,\n        late: Late,\n    ) -> Result<Raced<F::Output>, Halt> {",
       "        mut deadline: u64,\n        late: Late,\n    ) -> Result<Raced<F::Output>, Halt> {"),
      (STAGE, "        let judge = |output| match late {", "        let judge = move |output| match late {"),
      (STAGE, "            self.renewed(renewed?.at);\n            if let Some(output) = answered {",
       "            self.renewed(renewed?.at);\n            if late == Late::Refused {\n                deadline = runtime.now_millis().saturating_add(self.request.body_idle_millis);\n            }\n            if let Some(output) = answered {")],
     [(STAGE_T, ["r3_the_body_idle_bound_holds_while_renewals_keep_the_leases"])]),
    ("M9", "renewal that moves a lapsed lease forward instead of refusing (no lapse check)",
     [(STAGE, "            let now_millis = clock.now_millis();\n            self.check_live(now_millis)\n                .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            rounds += 1;\n            let renewal = self",
       "            let now_millis = clock.now_millis();\n            rounds += 1;\n            let renewal = self")],
     [(STAGE_T, ["r4_", "r7_the_quiesce_renewal_refuses_a_lapsed_lease"])]),
    ("M10", "a renewal retry that reuses the first round's clock reading",
     [(STAGE, "        let mut rounds = 0u32;\n        loop {\n            if rounds == RENEWAL_ROUNDS {",
       "        let mut rounds = 0u32;\n        let first_reading = clock.now_millis();\n        loop {\n            if rounds == RENEWAL_ROUNDS {"),
      (STAGE, "            let now_millis = clock.now_millis();\n            self.check_live(now_millis)",
       "            let now_millis = first_reading;\n            self.check_live(now_millis)")],
     [(STAGE_T, ["r4_a_renewal_retry_refuses_a_lease_that_lapsed_between_rounds"])]),
    ("M11", "after an unknown renewal outcome, an absent read taken as 'did not land' with may_still_commit: true",
     [(STAGE, "                        doubt.settle();\n                        let Some(moved) = self.reconcile(store, found, doubt).await? else {",
       "                        doubt.release();\n                        let Some(moved) = self.reconcile(store, found, doubt).await? else {")],
     [(STAGE_T, ["r5_"])]),
    ("M12", "a renewal loop counter that never reaches its bound",
     [(STAGE, "                .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            rounds += 1;\n            let renewal = self",
       "                .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            rounds += 0;\n            let renewal = self")],
     [(STAGE_T, ["r6_the_renewal_loop_is_bounded"])]),
    ("M13", "StagedAttempt::renew given a fixed now_millis",
     [(STAGE, "        let mut ledger = RenewalLedger::default();\n        let renewed = self\n            .footprint\n            .renew(\n                store,\n                clock,",
       "        let mut ledger = RenewalLedger::default();\n        let fixed = FixedReading(clock.now_millis());\n        let renewed = self\n            .footprint\n            .renew(\n                store,\n                &fixed,"),
      (STAGE, None, FIXED_CLOCK)],
     [(STAGE_T, ["r7_the_quiesce_renewal_reads_the_callers_clock_every_round", "r7_the_quiesce_renewal_refuses_a_lapsed_lease"])]),
    ("M14", "the post-commit coverage check removed from renew",
     [(STAGE, "            if lease_expiry_millis.saturating_sub(checked_at_millis) >= cover_millis.max(1) {",
       "            if lease_expiry_millis.saturating_sub(checked_at_millis) >= cover_millis.min(0) {")],
     [(STAGE_T, ["r7_the_quiesce_renewal_covers_its_hand_off_or_refuses"])]),
    ("M15", "the renewal awaited past a put's deadline treated as abandoned although the put had answered in time",
     [(STAGE, "                    if answered.is_some() {\n                        // Every write the deadline bounds has answered, or the read has: the\n                        // deadline is disarmed, and the renewal runs to its end.\n                        break Some((renewal.as_mut().await, answered));\n                    }\n",
       ""),
      (STAGE, "                    let woke = poll_fn(|cx| {\n                        if let Poll::Ready(output) = work.as_mut().poll(cx) {\n                            return Poll::Ready(Woke::Work(output));\n                        }",
       "                    let done = answered.is_some();\n                    let woke = poll_fn(|cx| {\n                        if !done {\n                            if let Poll::Ready(output) = work.as_mut().poll(cx) {\n                                return Poll::Ready(Woke::Work(output));\n                            }\n                        }"),
      (STAGE, "                        Woke::Renewed(renewed) => break Some((renewed, None)),",
       "                        Woke::Renewed(renewed) => break Some((renewed, answered.take())),"),
      (STAGE, "                        Woke::Timer => match work.as_mut().now_or_never() {",
       "                        Woke::Timer if done => continue,\n                        Woke::Timer => match work.as_mut().now_or_never() {")],
     [(STAGE_T, ["r3_a_renewal_outliving_an_answered_writes_deadline_is_awaited"])]),
    ("M16", "a renewal that sampled its leases live refused after its commit returned late",
     [(STAGE, "            let (at, lease_expiry_millis) = (landed.at, landed.record.lease_expiry_millis());\n            self.apply(landed);",
       "            let (at, lease_expiry_millis) = (landed.at, landed.record.lease_expiry_millis());\n            if cover_millis.is_none() {\n                self.check_live(clock.now_millis())\n                    .map_err(|cause| doubt.end(&self.upload_id, cause))?;\n            }\n            self.apply(landed);")],
     [(STAGE_T, ["r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence"])]),
    ("M17", "the renewal clamp removed from knob_clamps_hold",
     [(MP, "    ensure(\n        renewal_batch_ops(set.max_part_chunks) <= ops,\n        RenewalOverOps,\n    )?;\n", "")],
     [(KNOBS_T, ["leg2_"])]),
    ("M18", "the renewal clamp removed from staging's request check",
     [(STAGE, "                renewal_batch_ops(self.max_part_chunks) > u64::from(MAX_BATCH_OPS),",
       "                false && renewal_batch_ops(self.max_part_chunks) > u64::from(MAX_BATCH_OPS),")],
     [(STAGE_T, ["r8_"])]),
]


def run(target, filters):
    cmd = ["timeout", "900", "cargo", "test", "-p", "wyrd-core", "--test", target, "--"] + filters
    proc = subprocess.run(cmd, cwd=WT, capture_output=True, text=True)
    text = proc.stdout + proc.stderr
    keep = [l for l in text.splitlines()
            if l.startswith("test ") or "test result" in l or l.startswith("error")
            or "panicked at" in l]
    return proc.returncode, keep


def main():
    with OUT.open("a") as out:
        for mid, label, edits, runs in MUTANTS:
            if ONLY and mid not in ONLY:
                continue
            originals = {}
            try:
                for path, old, new in edits:
                    f = WT / path
                    originals.setdefault(path, f.read_text())
                    text = f.read_text()
                    if old is None:
                        text = text + new
                    else:
                        n = text.count(old)
                        assert n == 1, f"{mid}: edit matched {n} times in {path}: {old[:60]!r}"
                        text = text.replace(old, new)
                    f.write_text(text)
                out.write(f"=== {mid}: {label}\n")
                for target, filters in runs:
                    rc, keep = run(target, filters)
                    out.write(f"--- cargo test -p wyrd-core --test {target} -- {' '.join(filters)}  (exit {rc})\n")
                    out.write("\n".join(keep) + "\n")
                out.flush()
            finally:
                for path, text in originals.items():
                    (WT / path).write_text(text)


if __name__ == "__main__":
    main()
```
