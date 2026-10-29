## Summary
**User impact:** none yet, by design: no client can upload a multipart part today, and this PR
adds no caller. It removes a failure users would hit once part uploads ship. A part upload that
takes longer than one lease period (a slow client, or a long pause between pieces of the body)
would fail partway with a "lease lapsed" error, and until then the background cleanup could
mistake the live upload for an abandoned one and tear it down.

The change: while a part is being staged, its leases are now renewed on a timer for the whole
request, including while it waits on the client or on a storage write, without ever delaying
the existing time limits.

This is the second of two slices #926 was split into. It builds on #942 (#932), which must
merge first; until it does, this PR's diff also shows #942's commit.

## What to look at
- The *Renewal* section of the module doc in `crates/core/src/multipart/stage.rs`. It states
  when a renewal runs, which time limits it races, what happens when it fails, and the timing
  assumptions behind "a live request's leases never look lapsed".
- How a renewal failure is reported when storage writes are still pending. This changed late
  in review and is the part most worth a second opinion: such a failure is now reported as
  "write effect unknown" (with the renewal's cause attached), not as a clean error or a plain
  `NoSuchUpload`, because a dropped write may still publish.
- How to run it: `cargo test -p wyrd-core --test multipart_upload_part_stage --test
  multipart_knobs`. The new legs are the `r1_`–`r8_` tests appended to the existing staging
  file. They use the same in-memory store, deadline-enforcing chunk-store fake and hand-moved
  clock as #942's legs, so every race is scripted, and each leg has a watchdog.

## Root cause
#942 writes the slot lease at the reserve and the owned `sidx:` leases at each chunk's intent,
and never again. So any request slower than one TTL before its first chunk or between chunks
lets a lease lapse: staging stops itself, and the reaper and idle fence (proposal 0016
`:1824-1837`) read the request as abandoned.

## Fix
All in `crates/core/src/multipart/stage.rs` unless noted; lines are on this branch.
- **One renewal batch** (`Footprint::renewal_batch`, `:1616`). It rewrites the attempt's
  `slot:` record and every owned `sidx:` lease together, under `require(mpu == Open@E)`,
  `require(slot == prior)` and `require(sidx == prior)` for each entry (0016 `:349`, `:659`).
  Each new lease is strictly later than the one it replaces.
- **When it runs** (`Stager::renewing`, `:1948`; `renew_if_due`, `:1914`). Half a TTL after
  the last renewal, for the whole request. While a body read or fragment write is pending, the
  work, the renewal and one timer for `min(renew_at, deadline)` are raced. The write deadline
  and the body-idle bound still cut staging off on time; a write deadline stops constraining
  the renewal once every write it bounds has answered. A renewal never resets the idle bound.
  A renewal is never sent while the attempt's own intent batch is in flight.
- **Lapse refusal and settling** (`Footprint::renew`, `:1638`; `check_live`, `:1577`). Every
  round re-reads the clock and re-checks every lease before sending, and refuses a lapsed one.
  A `CommitUnknownResult` is settled by re-reading the slot's exact bytes, or reported in doubt
  with its `may_still_commit`. At most `RENEWAL_ROUNDS` (4, `:248`) commits per renewal. A
  renewal is judged by the leases it *sampled*: one that sampled live leases and lands after
  expiry is taken, and its slot rewrite makes a reaper's fence batch conflict (0016
  `:1843-1874`).
- **Failure beside pending writes** (`write_fragments`, `:2188-2236`;
  `halted_beside_writes`, `:1878`; new `Raced::Halted`, `:1169`). Pending puts are abandoned
  and accounted for the same way as at the write deadline. If any write was abandoned or came
  back unverified, the error is `Stop::WriteEffectUnknown` with the renewal's cause in a new
  `renewal` field (`:736`); a session found gone from `Open` appears there as the new
  `Stop::SessionLeftOpen` (`:741`). Only when every write answered `Ok`, or was refused as
  late, does the renewal's own error or refusal stand.
- **Quiesce renewal for the part commit** (`StagedAttempt::renew`, `:563`). It takes the
  caller's clock and a `cover_millis`. It answers `RenewOutcome::Covered` only if, at a clock
  reading taken after its last commit returned, every lease it wrote still has `cover_millis`
  left; otherwise it renews again within its rounds and then answers `Stop::CoverShort`. A
  `cover_millis` not below the TTL is an input error that writes nothing. #927 will call it.
- **Batch size.** `renewal_batch_ops(n) = 3 + 2n` (`crates/core/src/multipart.rs:5183-5191`)
  is the one count both checks use. `knob_clamps_hold` refuses an over-size set by name
  (`KnobClamp::RenewalOverOps`, `multipart.rs:5469`, checked at `:5541`), and
  `StageRequest::check` refuses such a `max_part_chunks` before writing anything
  (`stage.rs:325-329`).
- **Smaller edits.** `StageClock` is split out of `StageRuntime` (`:340`). The private `Halt`
  now boxes its `StageError`, because the new field tripped clippy's `result_large_err`; no
  public type changes. One sentence added to
  `docs/design/architecture/05-building-block-view.md:246`.

**#942 legs this PR changes**, each because it asserted that no renewal happens:
`a_prime_a_write_pending_past_its_deadline_is_abandoned`,
`a_prime_a_put_refused_as_late_is_reported_as_nothing_landed` and
`a_prime_an_ok_ready_as_the_deadline_timer_fires_is_counted_written` now expect the first wait
to end at the renewal instant (half a TTL); `a_double_prime_the_idle_bound_is_per_read_not_per_body`
sets a longer TTL so no renewal falls inside it. The shared `request()` fixture now uses
`MAX_PART_CHUNKS` (158) instead of 1,000, which the new request check refuses. Every other leg
passes unchanged.

**Not in this PR, for the maintainer:**
- Proposal 0016's knob table (`0016:1462-1479`) has no row for the renewal bound
  `3 + 2 × MAX_PART_CHUNKS ≤ B_ops`, which is tighter for large chunk bounds. Whether 0016
  should gain that row is a design-doc decision, left out of this code change.
- Three branches in the renewal race are not run by any leg (`stage.rs:1733`, `:2018`,
  `:2024`). A follow-up should add legs for them.
- The timing-assumption wording at `stage.rs:140-144` should also cover an intent that started
  before the renewal fell due ("each renewal round, plus any intent that started before the
  renewal fell due, finishes inside half a TTL").

## Verification
- **Claim:** every renewal moves the slot and every owned lease forward in one batch.
  **Checked:** `stage.rs:1616` (batch), `:1638` (round loop).
  **Test:** `r1_a_renewal_rewrites_the_slot_with_every_owned_lease`
  (`crates/core/tests/multipart_upload_part_stage.rs:4524`).
- **Claim:** renewal runs for the whole request, while a read or a write is pending, and its
  slot rewrite defeats a reaper that read the old lease.
  **Checked:** `stage.rs:1914-1948`, `:2018-2036`.
  **Test:** the `r2_` legs (`:4607-4978`), including `r2_d_i_…` (read pending, `:4680`),
  `r2_d_ii_…` (write pending, `:4750`) and `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence`
  (`:4978`).
- **Claim:** renewal never holds staging past the write deadline or the body-idle bound.
  **Checked:** `stage.rs:1948-2036`.
  **Test:** the `r3_` legs (`:5066-5474`), including a put answered in time while the renewal
  outlives its deadline (`:5272`) and the idle bound holding under renewals (`:5474`).
- **Claim:** a lapsed lease is refused, never revived, and a renewal stopping beside pending
  writes reports their effect as unknown.
  **Checked:** `stage.rs:1577`, `:2188-2236`, `:1878`.
  **Test:** the `r4_` legs (`:5548-5940`). `r4_a_renewal_stopping_beside_a_pending_write_leaves_its_effect_unknown`
  (`:5819`) releases the abandoned put after staging returns and sees it stored, so the reported
  doubt is real. `r4_a_renewal_stopping_after_the_writes_answered_keeps_their_account`
  (`:5940`) includes control runs where every write answered `Ok` and the renewal's own answer
  stands.
- **Claim:** unknown outcomes are settled by re-read; the loop is bounded.
  **Test:** `r5_` (`:6041-6119`), `r6_the_renewal_loop_is_bounded` (`:6158`, exactly
  `RENEWAL_ROUNDS` commits under constant `Conflict`).
- **Claim:** the quiesce renewal reads the caller's clock, never moves a lease backwards, and
  covers its hand-off or refuses.
  **Checked:** `stage.rs:563`.
  **Test:** `r7_` (`:6219-6388`).
- **Claim:** no renewal batch can exceed `MAX_BATCH_OPS`.
  **Checked:** `multipart.rs:5190`, `:5541`; `stage.rs:325-329`.
  **Test:** `leg2_each_clamp_passes_its_bound_and_names_itself_one_step_past_it`
  (`crates/core/tests/multipart_knobs.rs:187`; passes at 248 chunks, refuses 249) and
  `r8_a_chunk_bound_past_a_renewal_batch_is_refused` (`:6422`).
- **Red/green:** the new legs do not compile against #942 alone (the renewal types do not exist
  there). In place of a pre-fix run, 20 hand-made edits of the production code, each named
  against the legs it must break, turned those legs red; one of them reverts the
  failure-beside-writes fix and fails both new `r4_` legs in every non-control run.
  `cargo mutants --in-diff`: 68 mutants, 51 caught, 16 unviable (types without `Default`),
  1 missed. The miss is `>` → `>=` at `stage.rs:326`, which is equivalent: `3 + 2n` is always
  odd and `MAX_BATCH_OPS` (500) is even, so the two sides are never equal. The staging test file
  passed 10 runs in a row.
- **Whole gate:** `cargo xtask ci` passes on the patched tree, including fmt, clippy
  `-D warnings`, the full test suite, `cargo deny`, typos, the docs lint/render, and the DST
  tier.

Fixes #933
