# Adversarial review — #1028 (657.2b, part commit: classify a lost commit)

Short version: the classification logic held up against every legal history I could build, and the
red→green evidence is real (I re-ran it, see the last section). I found two missed cases the
builder can fix, one gating T4 blocker that the rubric says is already settled, and one overclaim
in the brief.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:569` (`judge_slot`'s `attempt_id`
  compare): C5's one MISSED mutant (`==` → `!=`) really is untested. With it applied in a scratch
  copy, all 47 tests in `crates/core/tests/multipart_upload_part_commit.rs` still pass. The mutant
  swaps `Rewritten` and `Gone`, and the two only lead to different answers under a **fenced**
  session (`commit.rs:846-850`, `:943-947`). The module doc fixes both answers: a slot rewritten
  under this attempt's id is `SlotLost` "under any session record", and a fenced session "answers
  its refusal whatever else its slot holds". But `r_a_slot_lost_or_rewritten_at_a_re_read_stops_without_a_release`
  (tests `:3488`) only runs under a live session. Concrete case: real Abort plus `rewritten_slot`
  planted before the commit gives `SlotLost` on the patch and `Refused(NoSuchUpload)` under the
  mutant; Abort plus `foreign_slot` gives `NoSuchUpload` on the patch and `SlotLost` under the
  mutant. Both probes go red under the mutant. Fix: add a fenced ("abort") leg to that test, at
  the commit's re-read and at the compensation's.

- NEEDS-HUMAN [impl] — brief (O) does not hold for an **empty** re-upload over a non-empty prior.
  `commit_batch` puts `require_absent` on the retire key whenever the prior has chunks
  (`commit.rs:749-751`, built at `:1004`), but `lost_commit` (`commit.rs:913-966`) never reads that
  key. So a taken key looks like "a conflict no read explains", and all 4 commit rounds are spent on
  it. For an attempt with chunks, the compensation that follows trips on the same key and ends
  `ObligationKeyTaken`; that is the only case `o_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part`
  (tests `:3538`) covers. For an empty attempt, `obligation_for` returns `None`
  (`commit.rs:719-720`), so the compensation has no `require_absent`, lands, and the answer is
  `Stopped { CommitRoundsSpent { rounds: 4 }, slot_released: true }`. Scratch probe output:
  `cause: CommitRoundsSpent { rounds: 4 }, slot_released: true`, commits=4, compensations=1,
  planted bytes and prior kept. The damage at the key is never reported; the message says "nothing
  a read could see to explain it". That is the rubric's "silent skip" class, and the brief's (O)
  says "ending in `Stop::ObligationKeyTaken`" for any re-upload over a non-empty prior. A fix that
  keeps the brief's four-read round order (657.2c needs it): when `retire.is_some()`, read the
  retire key once on the rounds-spent path (`commit.rs:1028-1030`) before compensating, and stop
  `ObligationKeyTaken` if it holds a record. Add the empty-attempt leg to the (O) test.

- NEEDS-HUMAN [human] — the gating T4-batch-review row fails on one blocker
  (`multipart_upload_part_commit.rs:2950`, "only scripted Tokio coverage … requires seeded Tier-0
  DST coverage"). That re-raises a deferral the rubric treats as settled: `commit.rs:191-192`
  already carries `// deferred: #660 — seeded Tier-0 DST coverage of the commit and compensation
  races (a commit against a rival, an Abort, a Complete's rollback, the reaper)`. The marker
  existed before this patch (it is in the base `HEAD`), and the brief's scope puts DST out of
  scope. Under "Deferrals are settled", this blocker should be recorded as rejected with that
  reason, not fixed in this PR. A human has to record that, because the row gates.

- NEEDS-HUMAN [human] — overclaim in the brief and the test it asked for. Brief (L) and
  `l_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` (tests
  `:3647`) say three rollbacks (`MAX_COMPLETE_ATTEMPTS`) is "the most a session can see". The
  patch's own doc at `commit.rs:235-238` says otherwise: a Complete rejected behind its fence gives
  its attempt back (#1007), so rejected Completes can move the session any number of times.
  Concrete case: four rejected Completes (each a fence plus release, `attempts` unchanged) landing
  between a rival-loser's compensation rounds end in `CompensationRoundsSpent`, with the slot held
  under an `Open` session until `W_session`. The code is honest ("a budget for the work, not for
  the answer"), and the leaked slot falls in the class the brief already accepted (slots held
  after a Complete rollback, sign-off iteration 2). So this is about wording, not behaviour: decide
  whether the test name and the brief's "real bound" claim stand. Low priority.

## The evidence, re-checked (not findings)

- C4-verify proved only green ("PASS (green-only)"), and C4-diff-cov never ran: the bundle does
  not apply on `origin/main`, which lacks #1027. So I rebuilt the target in scratch. The suite
  passes 47/47, and I re-ran nine of the brief's demonstrated reds. Each failed the named tests:
  `COMPENSATION_ROUNDS` lowered by one (both `l_` loop tests); the earlier slot read taken as final
  (both `t_a_terminal_delete_between_…`); a missing session always read as a terminal delete
  (`t_a_missing_session_…`, `t_another_attempts_slot_…`); the lease check skipped on later rounds;
  a non-`Open` record at an unrisen epoch read as a fence; a rollback answered `NoSuchUpload` at
  the commit and in the chase; compensating on any `Conflict`; a damaged part read as a rival.
- The tests drive production `commit_part`. The rival's commit also goes through `commit_part` on
  the inner store, and Abort is the real `abort_session`. One parallel construction: the rollback
  is a text edit of the session record (`roll_back`, tests `:1370`), not Complete's
  `fence_released`. That is enough for this slice, because only the epoch and the state are judged.
- The two TIMEOUT mutants (`commit.rs:822`, `:1009`, `+=` → `*=`) leave the round counter at 0,
  so the loop never ends. The leg's 60 s `LEG_TIMEOUT` (tests `:90`) would fail them; cargo-mutants'
  20 s cap simply cut them first. Not a gap.
- I tried to break these and could not. "Other bytes at an unrisen epoch is damage": every `mpu:`
  writer bumps the epoch (`multipart.rs:2630`, `:2654`, `:2682`, `:2712`). "A gone or rewritten
  slot under a live session is `SlotLost`": only `stage.rs` writes `slot:`, and Complete's release
  (`complete.rs:875-910`) does not touch slots. Also checked: the (T) extra slot read after a
  missing session, the order of judgment (damage before rival), and the stated bound on all three
  loops.

<!-- pdca:leaf-complete -->
