# Adversarial review — #927 (UploadPart commit, 657.2)

I re-ran the new test file on a scratch copy of `$PDCA_TARGET`. All 18 tests pass. C4-verify's
UNVERIFIABLE is the brief's pre-declared case (the test cannot compile without the new module).
It is not a refutation. Build-notes were withheld, so I re-made several of the brief's required
"red" edits myself and added five probe tests in scratch only (P1–P5; nothing written to the
target). Findings, worst first:

- NEEDS-HUMAN [impl] — **A commit can land after the attempt's own lease has lapsed. This confirms
  the T4 BLOCKING finding.** `crates/core/src/multipart/commit.rs:627-631` throws away
  `RenewOutcome::Covered { checked_at_millis, lease_expiry_millis }`. The staging API's contract
  says the commit must use them: "The commit checks both against its own deadline"
  (`crates/core/src/multipart/stage.rs:376-378`). The loop at `commit.rs:700-709` then sends up
  to `COMMIT_ROUNDS` = 4 commits, plus a psum scan, a part read and four reads per round, all on
  one 5 s cover. 0016 says the quiesce must make sure the lease "cannot lapse under the commit"
  (`0016:1726-1728`). **Concrete case (probe P3):** three benign `Conflict` rounds, each taking
  4 s (each one inside its own `E_tx`). The fourth commit lands with the clock at `1_012_000`,
  but the slot lease it requires ran out at `1_010_002`. The answer is still `Committed`. Once
  the session's last progress is older than `W_open`, this is the window the reaper's idle arm
  fences in (`0016:665`, the "reap a progressing upload" row at `0016:2258`). Fix: before each
  commit round, check the clock against `lease_expiry_millis - COMMIT_COVER_MILLIS`, and stop
  (or re-quiesce, then re-read) instead of sending.

- NEEDS-HUMAN [impl] — **The guard that stops a compensation while a commit may still land is
  not covered by any test.** If I replace `if doubt.any() { continue; }` at `commit.rs:795` with
  `if false && …`, all 18 shipped tests still pass. The brief assigns this red to the
  held-commit / unknown-empty-compensation leg. That leg cannot catch it: its rival's bytes equal
  the prior's, so with the guard gone the code falls through to the benign retry anyway
  (`commit.rs:802`). **Concrete case (probe P5):** the commit is held; right after the slot read
  (read 3) the held commit lands and a Complete rolls the session back to `Open@E'`. The shipped
  code correctly answers `InDoubt{SessionMoved}` and sends no compensation. With the guard
  deleted, it compensates under `E'` and answers `Stopped{SlotLost}`, which says "no batch of
  this part attempt is in doubt", for a part that is committed (the C3 defect class). Add P5,
  or an equivalent, to the suite.

- NEEDS-HUMAN [impl] — **Four C5 survivors are not equivalent. Each hides a wrong answer that a
  short test exposes:**
  (a) `Doubt::required -> true` (`commit.rs:418-420`). Probe P2: an empty part loses to a
  rival, its compensation is held, then an Abort and a teardown run. The shipped code answers
  `InDoubt`. The mutant answers `Refused(OperationAborted)` ("compensated, the session is still
  usable") for an aborted session where nothing of this attempt landed.
  (b) `delete !` at `commit.rs:862` (and the `found == *bytes` guard at `:879`). Probe P1: the
  compensation lands with an unknown outcome, then a client Abort runs before the re-read. Only
  the obligation re-read settles this. The mutant skips that read and answers `InDoubt` for a
  compensation that landed. No shipped test ever finds the obligation on re-read, and no test
  reaches `Stop::ObligationKeyTaken`.
  (c) `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS - 1` (`commit.rs:154`). Probe P4: one
  rollback beats the commit, then two more beat compensations. Three rollbacks is
  `MAX_COMPLETE_ATTEMPTS`, the most a session can see. The mutant runs out of rounds and leaves
  its slot held under a live `Open` session, which is the trap 0016 warns about
  (`0016:1607-1615`). The bounded-rounds leg cannot catch this: it compares against the imported
  constant (`crates/core/tests/multipart_upload_part_commit.rs:2092`, `:2113`), so it checks
  itself, not the bound.
  (d) `UploadPartError::source -> None` (`commit.rs:355-361`). `classify` would then read an
  in-doubt composed `UploadPart` as `Terminal` (`crates/traits/src/lib.rs:786-803`). This is the
  rubric's *Transactions* class (`CommitUnknownResult` must outrank a definite failure), and the
  composed verb is what #669 will map to the wire. No test checks the class of an
  `upload_part` error.

- NEEDS-HUMAN [human] — **A failed quiesce renewal, or spent commit rounds with nothing in doubt,
  leave the slot and `sidx:` residue under a live session.** `commit.rs:627-631`
  (`CommitError::Renewal`) and `commit.rs:701-706` (`Stopped{CommitRoundsSpent}`, pinned by
  "nothing was released" at `multipart_upload_part_commit.rs:2082`) both return without
  compensating. Their docs say the residue "stays for the reaper" (`commit.rs:176-179`). But the
  reaper does not walk an `Open` session (`0016:1600-1606`), and 0016 is normative that "every
  refusal that happens AFTER chunks are staged must compensate", or repeats fill
  `MAX_INFLIGHT_PARTS` and the session answers `503` until `W_session` (`0016:1420-1431`). When
  the renewal is settled (`StageError::Stopped`, not `InDoubt`), the attempt knows its slot bytes
  and could compensate. The brief only says to "treat its typed coverage error as a refusal to
  commit". A human should decide whether a settled coverage stop counts as a post-staging local
  refusal under `0016:1420`.

- **Attempted to refute, and could not.** These required reds hold. Ignoring `attempt_id` in
  `judge_slot` (`commit.rs:473`) turns C3′ red. Judging the session from the read taken before
  the slot read (`:756`) turns C2 red. Sending the rolled-back `Open@E'` to the reaper (`:814`)
  turns the rollback leg red. Treating an unknown compensation as not landed (`:873`) turns the
  compensation leg and C4 red. Checking the rival-record branch before the own-commit proof
  (`:767`) turns C3 and the landed-unknown leg red. I also checked by hand that every exit to
  `compensate` comes only after a slot read that clears every commit still able to land
  (`commit.rs:748-750`, `:795-818`); apart from the unpinned guard above, I found no reachable
  compensation while a commit is unsettled. Obligation keys cannot collide between a commit and
  a compensation of the same attempt, because the slot compare-and-set lets only one of them
  land. The `psum:` overshoot witness matches `0016:1441-1451`. DST coverage is deferred to #660
  with an in-code marker (`commit.rs:113`), so I treat it as settled.

<!-- pdca:leaf-complete -->
