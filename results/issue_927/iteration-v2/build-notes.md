# Build notes: #927, UploadPart commit (attempt 2)

Target: `getwyrd/wyrd` `main` @ `376b106`. Line numbers are of the patched tree in the worktree.

## Result

- `crates/core/tests/multipart_upload_part_commit.rs`: 34 tests pass, 0 fail.
- `cargo xtask ci` exits 0 on the exact tree `patch.diff` was made from ("all checks passed", 0
  `test result: FAILED`). That run included `typos`, the docs lint and render, fmt, clippy,
  cargo-deny and the DST tier.
- `patch.diff` applies to `376b106` (checked with `git apply --cached --check` on a fresh index).
- No external dependency was missing, so there is no NEEDS-HUMAN marker from Do.
- C4-verify should report UNVERIFIABLE (exit 77), as the brief declares: the functions are net-new,
  so the test cannot compile on the reverted base.

## What changed from attempt 1, and why

I started from `iteration-v1/patch.diff`, as the brief allows, and read nothing else in
`iteration-v1/`.

**Lease check (F).** `commit.rs:731` now keeps `lease_expiry_millis` from `Covered`, where attempt 1
dropped it. Before each commit round the attempt reads the clock and sends only with
`DISPATCH_MARGIN_MILLIS` left (`commit.rs:182`, the check is at `:816`). I renamed
`COMMIT_COVER_MILLIS`, because "the commit's transaction deadline" was the promise the brief says
the store does not make. The module doc now says the promise covers dispatch only.
`checked_at_millis` is not read: `renew` already judged the cover at that reading, and a later
reading supersedes it. Taking the larger of the two could not change any decision, so I left it out.

**Settled stops compensate (G).** `compensates_after` (`commit.rs:960`) is the Design's table.
`renewal_stopped` (`:976`) and `commit_stopped` (`:999`) send the compensation and return the
original typed error. `CommitError::Renewal` and `Stopped` gained `slot_released`, with an accessor
at `:247`. `compensate` (`:1045`) now returns `Compensated::{Released, Left}`, so each caller picks
its own answer: a refusal goes through `refuse` (`:1017`), and a stop keeps its error.

**Read order (B's read-gap leg).** Attempt 1's order (part, session, slot, session) already met the
rule, and a doubt left only at a slot read. Two things I changed:

- A session first seen moved by the *last* read sent attempt 1 round to retry a commit that could
  not land. That spent a round, and it could hit the lease check and return in-doubt when one more
  slot read would have settled it. It now does one lap of reads only (`send`, `commit.rs:930`).
- `compensate` has the same lap for a session found to have left `Open` with a doubt open. Without
  it, a held compensation under an Abort returned in-doubt even though a second slot read proves
  nothing landed. So (G)'s `NoSuchUpload` is now reachable after an unknown outcome.

**Smaller items.**
- `stage.rs:363`: doc fix only, as the brief scopes it.
- `05-building-block-view.md:248`: a new paragraph; `:246` and `:250` are untouched. This removes
  attempt 1's 20 KB rewrite of `:246`.
- The tests now write the round bounds as numbers, not imports.

## Things you should look at at sign-off

1. **`EntryLost` is proved by a unit test, not by a store history.** The brief asks for the log to
   show no compensation for `Stop::EntryLost`. `StagedAttempt::renew` cannot produce it: its only
   source is a chunk's intent (`stage.rs:2380`). From reading the code, a rewritten owned entry under a
   renewal instead ends in `RenewalRoundsSpent`, which does compensate, and the compensation has no
   precondition on the entries, so it would delete the rewritten one. No test forces that history;
   you may want to judge whether that row is right. So the row is held by `a_stopped_renewal_compensates_only_on_the_rows_that_say_so`
   in `commit.rs`, which calls the production `compensates_after`. A fixture that forged the error
   from a mock store would not have driven production.
2. **The epoch-chase leg needed one more run than the brief describes.** The brief's shape (one
   rollback beats the commit, two beat compensations) lands on the third compensation. So lowering
   `COMPENSATION_ROUNDS` from 4 to 3 would stay green, and the required red would not happen. I kept
   4, because the true worst case needs it: a rival beats the commit, then all three rollbacks beat
   compensations. I added that as a second run in the same leg, and that run is what fails.
3. **Patch size:** 230 KB, against the 165 KB the brief expected and the 100 KB signal. Source is
   about 1,260 lines of `commit.rs` (a large share is doc); the test file is 3,575 lines. I did not
   trim tests, as the brief says.
4. **Plan's flags still stand for you:** the dispatch-only narrowing of `0016:1727-1728`, and
   `Refused(OperationAborted)` for a lost commit on a live session (`multipart.rs:4666`).
5. **A wording mismatch left alone:** the doc of `StagedAttempt::renew` (`stage.rs`, "so the lease
   cannot lapse under the commit") repeats the promise this slice narrows. The brief limits this
   slice's `stage.rs` edit to the one doc fix, so I did not touch it. It should change alongside
   0016's wording.

## Refuting my own test

**(a) Genuine red? Yes, by edit.** The functions are net-new, so a full revert only fails to compile.
Each temporary edit below was applied to production code, the named test run, and the file restored.
All are recorded in the table. After the set, the file was restored and all tests passed again.

**(b) Production path? Yes.** Every leg calls the real `commit_part` or `upload_part` over a real
`RedbMetadataStore::in_memory()`. Sessions come from the real Create, parts from the real
`stage_part`, Aborts from the real `abort_session`. The stand-ins are the chunk store (a test double,
as the brief names) and two test-local writers the brief also names: the rollback, and the idle-fence
batch, since #625 owns the real fence.

**(c) Fixture includes the fault? Yes.** The wrapper forces each fault in the production path: held
batches, unknown outcomes, rollbacks and Aborts by hook at each read gap, and clock moves on each
read or batch. The not-landed read-gap runs exist so that an attempt cannot pass by never
compensating.

## Demonstrated reds

Run with `timeout 280 cargo test -p wyrd-core --test multipart_upload_part_commit <leg> -- --exact`,
the target the brief's Falsifiability names. The whole gate is `cargo xtask ci`, which ran as above.

| Temporary edit | Leg that failed | How it failed |
|---|---|---|
| Compensate on `CommitUnknownResult` without re-reading | `b_a_landed_commit_answered_unknown_is_committed` | `SlotLost`, not `Committed` |
| "A part is there" taken as `Committed` after an unknown | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` | "it committed again" |
| Part bytes alone taken as proof | `c1_an_empty_part_is_never_committed_on_its_bytes` | "a Committed attempt holds no slot" |
| Session state from a read before the slot read | `c2_a_torn_read_never_answers_committed` | answered `Committed` |
| Possibly-landed commit dropped from the doubt (`cannot_land` clears it) | `c3_a_landed_commit_then_a_rival_is_committed` | `SlotLost` |
| Rolled-back `Open@E'` left to the reaper | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | `NoSuchUpload` |
| Unknown compensation treated as did-not-land | `c_an_unknown_compensation_outcome_is_settled_by_a_re_read` | `Stopped`, not `InDoubt` |
| Commit loop counter never rises (`rounds += 0`) | `c_every_loop_ends_after_exactly_its_bound` | "never stopped committing (201 batches)" |
| Compensation loop counter never rises | same | same |
| Slot "gone" read as key absent, ignoring `attempt_id` | `c3_a_landed_commit_then_a_rival_is_committed` | `InDoubt`, not `Committed` |
| Compensation sent while the commit is held | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` | `SlotLost`, not `InDoubt` |
| Rival-record branch before the own-commit proof | `c3_a_landed_commit_then_a_rival_is_committed` | `SlotLost` |
| Empty list still writes a `{chunks}` obligation | `c4_empty_parts_never_install_an_empty_obligation` | decode refuses, `RetireObligationOwesNothing` |
| Same-part loser answers another refusal | `b_a_same_part_loser_compensates_its_own_chunks` | `NoSuchUpload` |
| Rollback loser answers another refusal | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | `NoSuchUpload` |
| Lease check removed | `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates` | "sent with 3002 ms left" |
| Lease check off by one (`<=`) | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | `LeaseShort` at exactly the margin |
| Lease check off by one (one ms below) | same | "sent with 4999 ms left" |
| Commit round allowed when in doubt and short | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` | "sent with 4999 ms left" |
| Compensation sent when in doubt and short | same | `Stopped`, not `InDoubt` |
| Past-expiry lease taken as proof of not landing | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` | `SlotLost`, not the expected answer |
| Moved session after the last slot read counted as settling | `b_read_gap_...` | failed at `LandedThenRollback at gap 3` |
| Rival after the last slot read counted as settling | `b_read_gap_...` | `SlotLost`, not `Committed` |
| Settling never allowed after a precondition dies | `b_read_gap_...` | `InDoubt`, not `OperationAborted` |
| Commit stop returns without compensating | `g_settled_stops_never_fill_the_slot_cap`, `c_every_loop_ends_after_exactly_its_bound` | no release reported |
| Renewal stop returns without compensating | `g_settled_stops_never_fill_the_slot_cap`, `g_a_settled_renewal_stop_compensates_and_keeps_its_error` | no release reported |
| Compensation after `SlotLost` | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | the compensation's own `SlotLost` |
| Compensation after a renewal `InDoubt` | same | a release reported |
| Over-limit check removed | `d_a_part_past_the_per_part_bound_is_refused_and_compensated` | answered `Committed` |
| Session check after an unknown compensation always true | `h_a_held_compensation_under_a_teardown_is_in_doubt` | `OperationAborted`, not `InDoubt` |
| Obligation re-read skipped | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | `InDoubt` |
| Obligation byte comparison always true | same | `OperationAborted`, not `ObligationKeyTaken` |
| `COMPENSATION_ROUNDS` lowered by one | `h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` | `CompensationRoundsSpent { rounds: 3 }` |
| `UploadPartError::source` returns `None` | `h_a_composed_error_keeps_its_class_and_its_message` | class is not `Indeterminate` |

Two of those reds fail on the answer before the test reaches its log assertion: "compensation after
`SlotLost`" and "compensation while held". The compensation was sent in both, and it produced the
wrong answer that failed the leg.

## Slot bytes, by invariant

- Both builders take the bytes from the attempt's own state: `commit.rs:645` and `:674`
  (`attempt.slot_bytes()`).
- The test wrapper checks every batch of every leg, at `check_slot_bytes` in the test file (`:559`).
- A "re-read the bytes" mutant cannot be reached under the classification, for the reason the brief
  gives: a conforming attempt recognises its own commit in C3′ and returns before any later batch is
  built, and no conforming path sends a batch after another attempt holds the index. It is the one
  mutant I list as unreachable rather than killed.

## C5 survivors

`scripts/mutants-in-diff` over the patch: 78 mutants, 61 caught, 15 unviable, **2 missed**.

Both missed were on `commit.rs:930`, `send = matches!(last, SessionSeen::Same) || !send;`:

| Mutant | Effect | Now killed by |
|---|---|---|
| `\|\|` → `&&` | an extra lap of reads before each retry | `b_a_held_commit_is_never_compensated_and_at_most_one_lands`: reads between commits are `[4, 4, 4]`; the mutant gives `[8, 8, 8]` |
| delete `!` | sends a commit that cannot land after a moved session | `b_read_gap_...`: 1 commit at gaps 0 to 3; the mutant gives 2 at `LandedThenRollback at gap 3` |

I confirmed both by applying the mutant by hand after adding the assertions. **I did not re-run the
full mutation pass after that.** Only test assertions changed since it ran, so nothing caught should
come back, but Check's C5 run is the first full pass on the final tree.

The `|| !send` term itself is a guard for a state no history reaches: a Held slot with a doubt open,
on the lap after the session moved, needs session bytes to come back. I kept it so the read-only lap
has a stated bound by construction rather than by that argument alone.

**Attempt 1's nine survivors are all gone from the missed list:**
- `COMPENSATION_ROUNDS` `+` → `-` and `*`: the epoch-chase leg and the numbers written in the test.
- `UploadPartError` `Display` and `source`: `h_a_composed_error_keeps_its_class_and_its_message`.
- `Doubt::required` → `true`: `h_a_held_compensation_under_a_teardown_is_in_doubt`.
- Both `lossy` mutants: exact key assertions in `g_a_slot_lost_...` and `h_the_obligation_re_read_...`.
- `!` in `sent.contains`, and the `found == *bytes` guard: `h_the_obligation_re_read_...`.

The 15 unviable are `Default::default()` replacements on types with no `Default`, so they do not
compile.

## Alternatives ruled out

- **Retry the commit instead of a read-only lap** (attempt 1). It is about 4 lines shorter per loop
  (the `send` flag and its branch), but it spends a commit round and a lease check on a batch that
  cannot land. With the rounds or the lease nearly gone, that returns in-doubt where one more read
  settles it. In the read-gap leg it shows as a second commit at gap 3.
- **A new `CommitError` variant for "stopped and released".** That would need a doubled set of
  variants (`Renewal`, `Stopped`, each with a released twin) and more match arms at every caller for
  one bit. A field plus `slot_released()` keeps each error's class and its one `source`.
- **Renewing again when the lease is short, sizing the margin to the backend's deadline, and a new
  `StageOutcome` variant:** all rejected by the brief; not tried.

## Rubric self-review

- **One clock:** the lease check, the renewal and the part stamp all read the caller's `StageClock`,
  and the module doc says so. The compensation reads no clock.
- **Docs currency:** `05-building-block-view.md:248`, and the header of `multipart.rs`.
- **Absent entries, transactions:** an unknown outcome is never read as `Conflict`, and the doubt
  outranks later stops (`Doubt::end`).
- **Test fidelity:** seeded DST of these races is deferred to #660, with the in-code marker at the
  top of `commit.rs`.
- **Serialization identity:** the wrapper checks that every part, summary, slot and obligation it
  sees decodes, and that part, summary and slot values re-encode to the same bytes.

## Housekeeping

My throwaway files (the red script and gate logs) are in `$PDCA_SCRATCH`. I did not delete them: the
harness owns that root and reclaims it. The worktree's `mutants.out` is git-ignored.
