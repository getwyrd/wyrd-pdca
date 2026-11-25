# Adversarial review — #1013 (owned `sidx:` walk, untrusted-record retirement, terminal session delete)

I re-ran the evidence in a scratch copy of the target (cargo works on this host). The 21 new tests
pass. All six reds the brief requires come back when I break the production code by hand, and seven
more breakages I tried are caught too. I could not break the safety core: mark before delete,
the whole-fleet gate, and the exactly-once `mpuctl` decrement all held. What I found: one scope gap,
one piece of production wiring no test guards, two test gaps, and a narrower reading of T4's blocking
item.

## Findings

- NEEDS-HUMAN [human] — **An aborted session with a damaged `{chunks}` obligation never ends.**
  `crates/core/src/multipart/commit.rs:1551` copies a re-uploaded part's *prior* chunks into a
  session-scoped `retire:bytes:s:<id>:<E>:<n>:<attempt>` obligation. `crates/custodian/src/retirement/marks.rs:994-996`
  holds a wrong-length chunk in that list as `untrusted-placement` for every session state, and
  `crates/custodian/src/retirement/sessions.rs:755-761` makes the terminal delete wait while any
  session-scoped obligation is left. Probe: an `Aborting` session holding such an obligation (RS(4,2)
  on 5 servers, fragments on 0–5). After 6 deployed whole-fleet passes, every pass was `Blocked`
  with `retire-obligation-held` / `untrusted-placement`, and the session record, the obligation and
  `mpuctl.count` (1) were all unchanged. This is the same damage E1 retires by chunk id when it sits
  in a `part:` record. Once a re-upload moves it into a `{chunks}` list, the session keeps its
  admission slot for good. The brief's invariant ("every session state the drain owns has an exit")
  is not met for this record kind, but its Scope names only `part:`/`sidx:`. The human decides:
  extend this slice (retire `{chunks}` lists by chunk id for `Aborting` sessions, also marking the
  positions each list names, because a compensated attempt's write may still land), or file a
  follow-up issue. It is audited, and no bytes are lost.

- NEEDS-HUMAN [impl] — **The deployed caller's two new inputs are not guarded by any test.**
  `crates/server/src/custodian.rs:577` (`operator_fleet_size`) and `:580` (`Tombstone::Retain`). I
  changed them in scratch to `operator_fleet_size: fleet.len()` and `Tombstone::Elapsed`, and every
  server test that drives the deployed loop still passed: `custodian_gc` (1), `custodian_day_one` (15)
  and `closed_write_path` (10). The new test file builds its own context
  (`crates/custodian/tests/multipart_session_terminal.rs:650-659`, `View::deployed`), so it cannot see
  the real caller. Both changes would be safety regressions. The first runs retirement by chunk id on
  a fleet that was short from startup, which strands a missing server's fragments for good. The
  second deletes `Completed` tombstones, so an identical Complete retry gets `NoSuchUpload`. So the
  brief's SELF-TEST claim that the test "binds … their dispatch in the deployed custodian" does not
  hold for these two inputs. Fix: add a server test through `run_reconstruction_until`
  (`custodian_gc.rs` already drives it) where `configured` is short from startup, with an `Aborting`
  session holding an untrusted entry (expect a `partial-fleet` hold) and a `Completed` session (expect
  its record to stay).

- NEEDS-HUMAN [impl] — **T4's blocking item at `sessions.rs:678` is real, but only applies while
  another entry is held.** Probe: an `Aborting` session with entry A (wrong-length, part 1) and entry
  B (RS(128,128), 256 positions, part 2), on a fleet missing server 5 from startup. Over 5 passes, B
  stayed at 233/256 positions marked, no bookmark was written, and every pass re-read the same 233
  marks. Once the fleet was whole, the session ended within one pass. Cause: held entries claim
  `first_left` with a count of 0 (`sessions.rs:619`, `:626`), and the bookmark filter drops a zero
  count (`:679`), so B's progress is never saved. A stored bookmark naming B would be deleted the
  same way. The stall only lasts while another entry of the same session is held, and that hold
  already blocks the terminal delete, so nothing is lost or stranded. The cost is 233 wasted reads
  per pass for each such session, and B's bytes are kept until the hold clears (an entry smaller
  than one step's budget would have been retired). Small fix: put the bookmark on the first entry
  with `covered > 0` instead of the first entry left. Resume matches by key (`:601-606`), so that is
  safe. Keep the probe as a regression test.

- NEEDS-HUMAN [impl] — **Recovering from a lost bookmark works but is untested.**
  `sessions.rs:704`: if I flip `!moved` to `moved`, all 21 shipped tests still pass (C5 also lists
  `704:39` and `910:5` `emit_unusable_bookmark` as missed). Probe: a wide entry whose first 233
  positions already carry the walk's mark, with no bookmark (I reached this state with the held-A
  pass above, then deleted A). The real code ends the session. The mutant stalls for good with no
  hold, which is the same stall class iteration 1 was sent back for. Separately, the C5 mutant that
  makes `owned_walk_key` (`sessions.rs:153-157`) return `vec![]` survives, because the test builds
  the key with that same function (`multipart_session_terminal.rs:1862`). So the stored key spelling
  `custodian:owned-walk:<id>`, which the docs name, is not pinned. Add the probe as a test and assert
  the literal key.

- NEEDS-HUMAN [human] — **T4's two DST items conflict with the brief.** T4 also blocks on missing
  seeded Tier-0 DST (deterministic simulation) coverage for the new destructive paths
  (`sessions.rs:714`, `multipart_session_terminal.rs:1067`). The brief says "seeded DST (#660 owns
  'two concurrent drainers'); add no DST scenario", and the in-code `// deferred: #660`
  (`sessions.rs:173`) covers only two drainers. This is not a builder defect. The human decides:
  either accept the brief's scope and widen #660 (or file a new issue) for single-drainer crash
  coverage, or require DST in this slice. My view: each walk step and the terminal delete is one
  atomic batch, and the next pass re-reads everything, so DST here would more likely confirm than
  find something.

- NEEDS-HUMAN [impl] — **The rebase is still not done.** `gate-logs/C4-diff-cov.log`: patch.diff does
  not apply on `origin/main`, so diff coverage is still not measured. Iteration 1's carry-forward
  asked for exactly this rebase. Rebase and re-run C4-diff-cov.

## Attempted to refute; could not

- **Red→green evidence.** C4-verify is UNVERIFIABLE, as the brief said in advance (the test uses
  API this patch adds). I rebuilt the six required reds by hand in scratch. Each turns the named leg
  red:
  1. Empty-`sidx:` gate removed: `d_the_terminal_delete_waits_for_an_empty_owned_range` fails.
  2. Tombstone check removed: the `Completed` leg fails (as do E1–E3, because the committed object's
     session is deleted too).
  3. Session-record precondition removed: the race leg fails.
  4. Listing marks skipped: E2/E3 fail.
  5. Fleet gate forced open: all four fleet controls fail.
  6. E2 named positions skipped: the late-landing leg fails ("(3, chunk 22007 idx 3) is unmarked").

  Also caught: part listing skipped (E1), obligations gate removed (10 tests), bookmark resume or
  bookmark count off by one (bookmark leg), `seggrp:` marker always deleted (adopted-group leg),
  ledger precondition dropped (ledger-moved leg).
- **Mark before delete via the bookmark.** I looked for a path where the bookmark counts a position
  that is not marked. The bookmark is written in the same batch as the marks it counts. GC leaves
  marks alone at positions the staged class protects (`crates/custodian/src/gc.rs:852-854`). Nothing
  else deletes `orphan:` keys at those positions. I found none.
- **Terminal-delete races.** The session-record and `mpuctl` preconditions sit in one batch
  (`sessions.rs:772`, `:799-801`). Nothing can add an `sidx:` entry after `Open` (intents are fenced
  on `Open`), and nothing can add a `seg:` record once the session is `Aborting` or `Completed`, so
  reading those ranges without a precondition is safe.
- **Cost claim** (`crates/custodian/src/retirement.rs:155-166`). At most (16 + 16 + 46) full-fleet
  listings per pass is correct for the shipped profile. The bound grows with `max_sessions` under
  other profiles, and the doc says so.

<!-- pdca:leaf-complete -->
