# Build notes — #1028 (657.2b): a part commit that lost is classified and compensated

Base: the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`, HEAD `60e5d2c` (the stack base
`pdca-integration/main-rdfeb36496b2d`, which folds in 657.2a, #1027, merge `49463ec`). Every
`path:line` below is on the patched tree.

## What changed

**`crates/core/src/multipart/commit.rs`** (the function and file 657.2a shipped; the brief's
*Ordering note* says this slice replaces its `Conflict` stops there).

- The two 657.2a stops `Stop::CommitConflict` / `Stop::CompensationConflict` (with their
  `// deferred: #1028` markers) are gone. In their place, `Stop::CommitRoundsSpent`,
  `CompensationRoundsSpent`, `SlotLost`, `ObligationKeyTaken` (`commit.rs:318`), with `Display`
  and `source()` arms.
- `COMMIT_ROUNDS = 4` (`commit.rs:229`) and `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1`
  (`commit.rs:241`, mirroring `abort::ABORT_ROUNDS`, `abort.rs:62`).
- `judge_session` (`commit.rs:525`): same bytes / newer `Open` (rollback) / newer non-`Open`
  (fence) / gone; bytes that do not decode and any other record at an epoch that has not risen,
  whatever its state, are `RecordError` (ADR-0045).
- `judge_slot` (`commit.rs:561`): held / gone (absent or another attempt's id) / rewritten (own id,
  other bytes); a slot that does not decode is an error.
- `session_gone` (`commit.rs:596`): the slot key read **once more, after** the session read that
  found no record. Absent: `NoSuchUpload`. Any bytes there: `Stop::Record(SessionRecordMissing)`.
  It tests the key itself, not `judge_slot`'s wider "gone".
- `lost_commit` (`commit.rs:913`): the commit's re-read, in the brief's order (part key, session,
  slot, session), then the classification table of the brief's *Design* as a tuple match on
  (last session, slot). The first session read is taken and passed over, with a
  `// deferred: #1029` marker (`commit.rs:927`): the brief requires the read order for 657.2c.
- `commit_rounds` (`commit.rs:971`, was `commit_round`): one batch, stamped once; the same batch
  re-sent only for a conflict no read explains; `CommitRoundsSpent` after `COMMIT_ROUNDS`; a
  lease check (same `StageClock`) before every round; an unknown outcome is still 657.2a's in-doubt
  stop (`// deferred: #1029`, `commit.rs:1018`).
- `compensate` (`commit.rs:807`) now loops: re-read = obligation key, slot, session; any record at
  the obligation key is `ObligationKeyTaken`; a newer `Open` epoch is chased; unchanged is retried;
  fenced is `Compensated::Left(refusal)`; gone goes through `session_gone`. Bounded by
  `COMPENSATION_ROUNDS`. Unknown outcome unchanged (`// deferred: #1030`, `commit.rs:826`).
- `refuse` (`commit.rs:864`), `compensated_stop` (`commit.rs:881`) and the renewal-stop arm
  (`commit.rs:1087`) answer the compensation's refusal when it found the session fenced or
  deleted.
- Module doc: new sections *A commit that lost* (`commit.rs:48`), *The compensation, and the epoch
  chase* (`commit.rs:104`), *Bounds* (`commit.rs:152`); *The stops* table (`commit.rs:122`),
  the lease section and *One clock* updated.

**`crates/core/src/multipart.rs`**: `RecordError::PartRecordMissing` (`multipart.rs:396`) and
`RecordError::SessionRecordMissing` (`multipart.rs:406`) with `Display` (`:750`, `:755`);
`SessionEpochNotRisen`'s doc says "whatever its state" (`:386`); `Refusal::OperationAborted`'s doc
widened to a rival, a repair of the prior and a rollback (`multipart.rs:4755`), no new variant; the
header names #1028 (`multipart.rs:14`).

**`docs/design/architecture/05-building-block-view.md:250`**: the sentence that said a `Conflict`
stops with the slot held is rewritten (docs-currency rule: the commit's answers changed).

**Test** `crates/core/tests/multipart_upload_part_commit.rs`, appended:

- Wrapper: `Step::AfterRead(key, steps)` (`:333`) — **an action between two reads**, armed by a
  step beside a batch and run right after the next `get` of that key (`take_read_hook`, `:642`).
  `Log::reads_after` (`:283`) pins read order. `At` clock (`:885`), `Rig::rival`, `Rig::erased`,
  `Rig::terminal_delete` (`:1132`, session + every `slot:` + every `sidx:` in one batch).
- New legs (`:2919` on): (B) fenced, rollback, rival loser (non-empty and empty), repair,
  unexplained commit retry, unexplained compensation retry on three routes; (T) terminal delete
  between the commit's reads, between the compensation's reads, still-held slot (both phases),
  foreign slot behind a missing session (both phases); (R) torn part (5 cases incl. under a
  rollback and a fence), prior gone (same / rollback / fence, non-empty and empty prior), damaged
  session at the last read (torn + 4 not-risen shapes, both phases), damage at the first session
  read passed over, slot gone / foreign / rewritten / torn (both phases); (O) taken obligation key;
  (L) exact bounds (4 legs, literal `4`s), the chase's real bound (rival-first 4 compensations;
  rollback-first 3), fenced during compensation on 5 routes, lease used up between rounds.
- 657.2a legs changed because they pinned the stops this slice replaces:
  `h4_a_commit_answered_conflict_stops_with_the_slot_held` removed (replaced by
  `b_a_conflict_no_read_explains_sends_the_same_batch_again`);
  `h4_a_compensation_answered_conflict_claims_no_release` removed (replaced by
  `b_a_compensation_conflict_no_read_explains_sends_the_same_batch_again`);
  `h4_a_composed_error_keeps_its_class_and_its_message`'s nothing-in-doubt leg now spends every
  commit round (a single `Conflict` now retries and lands); the (F) message string reads "so no
  further commit was sent" (a later round can be the one stopped).

## Decisions and what I ruled out

- **Order of judging.** All four reads are taken first, then damage is judged (part, slot, last
  session), then the tuple match. A torn part key is a record stop whatever the session, a fence
  included (the brief's (R): "never a rival or a fence"; "a damaged part record under a moved
  session"). Rewritten slot beats a fence's refusal (only this attempt writes its own id). A slot
  gone under a fence is answered by the fence (residue is the reaper's); under `Open@E` or a
  rollback it is `SlotLost` directly rather than compensating under `E'` into a batch whose slot
  precondition is already known gone — same final answer, one batch fewer.
- **The extra slot read** is made whenever the last session read is gone, even if the first
  session read was gone too. Ruled out reusing read 3 when read 2 was gone: the brief's table keys
  on the *last* read, and one rule is simpler to check.
- **Obligation key at the compensation's re-read:** any record is `ObligationKeyTaken`. With
  nothing in doubt no batch of this attempt put it. 657.2c will need "holds the bytes it put" for
  doubt; not added here.
- **COMMIT_ROUNDS = 4** is a chosen budget for unexplained conflicts, documented as a work bound,
  not an answer bound. **COMPENSATION_ROUNDS' doc is honest** that a Complete rejected behind its
  fence gives its attempt back (#1007), so more rollbacks than `MAX_COMPLETE_ATTEMPTS` are possible
  (same caveat `ABORT_ROUNDS` states, `abort.rs:48-61`); the brief's "the most a session can see"
  holds for charged fences only.
- **Kept 657.2a's lease-check-before-build order** on round 1 (lease short compensates even if
  the record build would fail).
- **Attempt 7's doubt machinery (`Doubt`, `SessionMoved`, `OwedChunks`, `InDoubt::cause`) not
  carried over**: out of scope (657.2c/657.2d) and nothing here is in doubt.

## Demonstrated reds (required by the brief) — each edit reverted after

Run with `cargo test -p wyrd-core --test multipart_upload_part_commit -- --exact <leg>` under
`timeout 600`, one temporary edit of `commit.rs` at a time (scripts and logs in
`$PDCA_SCRATCH/pdca-builder-1028-reds/`). Every edit compiles; each named leg fails with a behaviour
difference:

| Edit | Leg | Result |
|---|---|---|
| 1a slot read before the session read taken as final, commit's re-read only | `t_a_terminal_delete_between_the_commits_reads_answers_no_such_upload` | FAIL: `SessionRecordMissing` for `NoSuchUpload` (the compensation leg stays green) |
| 1b the same, compensation's re-read only | `t_a_terminal_delete_between_the_compensations_reads_answers_no_such_upload` | FAIL: same (the commit leg stays green) — so the two re-reads are bound separately |
| 2 a missing session always a terminal delete | `t_a_missing_session_with_the_slot_still_held_is_damage` | FAIL: got `Ok(Refused(NoSuchUpload))` (foreign-slot leg fails too) |
| 3 another attempt's slot counted as gone behind a missing session | `t_another_attempts_slot_behind_a_missing_session_is_damage` | FAIL: got `Ok(Refused(NoSuchUpload))` (still-held leg stays green) |
| 4 compensating on any `Conflict` without the re-read | `b_a_fenced_session_answers_no_such_upload_and_writes_nothing` | FAIL: "no compensation", left 1 right 0 |
| 5 the rolled-back `Open@E'` sent to the reaper | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | FAIL: `NoSuchUpload` for `OperationAborted` |
| 6 rollback / rival / repair loser answering `NoSuchUpload` | the rollback, rival and repair legs | FAIL ×3 |
| 7 a damaged part read as a rival | `r_a_damaged_part_record_is_a_record_stop_not_a_rival` | FAIL: got `Ok(Refused(OperationAborted))` |
| 8 a non-`Open` record at an unrisen epoch read as a fence | `r_a_damaged_session_at_the_last_read_is_a_record_stop` | FAIL: got `Ok(Refused(NoSuchUpload))` |
| 9 `require_absent` dropped from the commit's obligation key | `o_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part` | FAIL: got `Ok(Committed …)` |
| 10a commit-round counter never reaches its bound | `l_every_loop_ends_after_exactly_its_bound` | FAIL: leg timeout (60 s) |
| 10b compensation-round counter never reaches its bound | same | FAIL: leg timeout |
| 11 `COMPENSATION_ROUNDS` lowered by one | `l_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` | FAIL: `CompensationRoundsSpent { rounds: 3 }` for `OperationAborted` |
| 12 lease check skipped on a later round | `l_a_lease_used_up_between_rounds_stops_them_and_compensates` | FAIL: the wrapper's dispatch-margin guard panics (a round sent below the margin) |

First tries of edits 2, 4 and 9 did not compile (`warnings = "deny"`: unreachable pattern, dead
code, unused variable); those were redone so they compile, and the table shows the redone runs.

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With `commit.rs` and `multipart.rs` reverted to HEAD and the new test
  file kept, the file does not compile (no `CommitRoundsSpent`, `CompensationRoundsSpent`,
  `SlotLost`, `ObligationKeyTaken`, `PartRecordMissing`, `SessionRecordMissing`). With the 8 legs
  that name those types switched off (`#[cfg(any())]`, temporary), **all 14 remaining appended legs
  fail** on the unpatched source (e.g. the rollback leg gets `Err(Stopped { cause:
  CompensationConflict … })`/`CommitConflict` where it wants `OperationAborted`; the unexplained
  retry leg gets a stop where it wants `Committed`). Plus the 15 targeted edits above.
- **(b) Production path?** Yes. Every leg calls the production `commit_part` (`commit.rs`, via
  `Rig::commit` → `commit_part(&view, &runtime, attempt)`) over a real `RedbMetadataStore`; each
  attempt is staged by the real `stage_part`, sessions made by the real `create_session`, fences
  by the real `abort_session`, rivals committed by the real `commit_part`. The wrapper only logs,
  scripts answers and runs steps; it never stands in for the code under test.
- **(c) Fixture includes the fault?** Yes. The rollback is a real `require`d CAS on the stored
  session; the Abort is real; the rival's commit really lands first; the terminal delete really
  removes the session, slots and owned entries in one batch **between** the re-read's slot read and
  session read (asserted by the logged read order); torn and not-risen records are planted at the
  real keys. Store `Conflict`s in the fence/rollback/rival/repair legs are the inner store's own
  answers (`Act::pass()`), not scripted.

## Gates

- `cargo test -p wyrd-core --test multipart_upload_part_commit`: 47 passed (657.2a's 29 legs less
  the 2 removed, plus 20 new).
- `cargo fmt --all -- --check`: clean (after `cargo fmt --all`, which touched only my two files).
- **`cargo xtask ci` via `./engine/xtask.sh ci` on the final tree: exit 0** (log
  `$PDCA_SCRATCH/pdca-builder-1028-reds/ci2.log`): typos, docs lint and render, the guards, fmt,
  clippy, build, `cargo test --workspace --exclude wyrd-dst` (228 suites, 2,332 tests), machete,
  the three `cargo deny` runs, conformance vectors, the statics gate, deploy-guard, DST clippy and
  DST tests.
- **An intermittent hang, not this patch.** The *first* `cargo xtask ci` run hung in
  `crates/server/tests/custodian_day_one.rs` (13 of 15 tests "running for over 60 seconds", the
  binary parked on a futex at 0% CPU for 30+ minutes); I stopped that binary (my own process) and
  the gate finished every other phase green (log `ci.log`). Then: the full workspace suite with
  those 13 tests skipped by name was green (2,218 tests, exactly 13 filtered); the binary run
  alone passed 15/15 in 0.18 s on the **unpatched** base and on the patched tree; and the second
  full gate run passed it too. The same binary from another cycle's review worktree
  (`issue_927/pdca-review-qir_7o8y`) has been parked for about 78 hours on this host. Nothing in
  that crate calls the code this patch changes (`commit_part`/`upload_part` have no caller
  outside `crates/core`). Worth an issue against `custodian_day_one` if it recurs in CI; not
  raised as NEEDS-HUMAN because the gate is green on the final tree.
- Scratch: scripts, logs and file copies for the red runs are under
  `$PDCA_SCRATCH/pdca-builder-1028-reds/` (inside the harness's scratch root, left for the harness
  to reclaim; the logs are the evidence cited above).

## Not covered by a dedicated leg

- A store fault on one of the re-reads (commit's or compensation's). It is the existing
  `Stop::Store` path, no compensation, no release; the wrapper has no step to arm a fault mid-run.
- DST coverage of these races stays deferred to #660 (existing `// deferred: #660` marker,
  `commit.rs` top), and the brief puts DST out of scope.

## C5 (advisory) survivors

Not run here (the brief lists them as advisory). Likely survivors worth a look: the `Display`
strings of the new stops are checked only for `CommitRoundsSpent`, `CompensationRoundsSpent`,
`SlotLost` (key named), `ObligationKeyTaken` (key named) and `PartRecordMissing`/
`SessionRecordMissing` (one phrase each); swapping the order of the first two arms in the
`(SessionSeen::Gone, _)` / `(_, SlotSeen::Rewritten)` match changes nothing any leg observes (a
rewritten slot behind a missing session is still a record stop, just a different one).
