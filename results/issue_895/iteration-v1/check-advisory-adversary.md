# Adversarial review — #895 (multipart Abort fence + shared fence batch)

Short version: I could not break the fix. Every Abort ending I could force is correct, and the
restore refactor is equivalent to the code it replaces. What I did find is one real test gap
(the C5 survivor), one doc sentence that overclaims, and one design question about retry spacing
on TiKV.

## Findings

- NEEDS-HUMAN [impl] — **Half of `Doubt::settle` has no test anywhere; brief leg (I)'s last
  sentence is unproven.** `crates/core/src/multipart/abort.rs:422-431` clears the held
  `may_still_commit: true` doubt when a read finds the session moved. The test that claims to
  cover this, `crates/core/tests/multipart_abort.rs:1109`
  (`i_a_session_moved_under_a_held_fence_ends_without_the_doubt`), moves the session to
  `Completing`, so the Abort returns `Ok(Refused(OperationAborted))`. An `Ok` answer cannot carry
  a doubt, so the test passes whether or not the doubt is cleared. I confirmed this in a scratch
  copy. With the C5 mutant (`settle` → no-op), all 21 shipped core tests still pass. With only
  the held-clearing removed (`unsettled = None` kept), all 21 core tests **and** all 6 madsim DST
  tests (`MADSIM_TEST_NUM=50`) still pass. **Concrete failing case the suite cannot see:** round 1
  is held (`may_still_commit: true`) and the session moves to `Open@2`. The re-read finds `Open@2`,
  so the held batch (which requires `Open@0` bytes) can never land. The Abort re-fences at epoch 2,
  and that commit fails with a plain `TransientFault`. Correct ending: `AbortError::Stopped`,
  class `Transient`. With the held-clearing removed, it ends `InDoubt { may_still_commit: true }`,
  class `Indeterminate`. That would make the caller re-read for a fence that cannot land. It errs
  on the safe side, so it is not a C-1 breach, but it breaks the brief's "only otherwise return the
  plain fault". A test that pins it, using the existing `Scripted` wrapper: plan
  `[HoldThenMove(session_bytes(&open, 2, 1, r#"{"kind":"Open"}"#)), Fail { transient: true }]`,
  then assert `Err(Stopped{..})`, `classify == Transient`, and that `release_held()` returns
  `Conflict`. I ran it: green on the patch, red on both mutants. The other half (an out-of-flight
  outcome settled by a read) **is** caught, but only by the madsim DST's 1021 leg, invariant (4)
  at `crates/dst/tests/multipart_abort.rs:453`. That is why C5 (plain `cargo test`) reports the
  mutant missed. A core test with plan
  `[Unknown { applied: false, may_still_commit: false }, Fail { transient: true }]` expecting
  `Stopped`/`Transient` would let the in-process target catch it too. I ran that one as well:
  green on the patch, red on the full mutant.

- NEEDS-HUMAN [impl] — **A living-doc sentence overclaims the race answer.**
  `docs/design/architecture/05-building-block-view.md:206` says "two Aborts racing for one session
  answer one fenced and one already aborting". That holds only when neither commit's outcome is
  unknown. Counter-case (I ran it as a probe): Abort B fences, and Abort A's own commit comes back
  `CommitUnknownResult { may_still_commit: false }` without being applied. A re-reads and finds
  `Aborting@1`. `open_teardown` is deterministic, so B's put is byte-identical to A's.
  `Doubt::landed` (`abort.rs:295`, `:415`) matches it, and A answers `Fenced`. **Both** Aborts
  answer `Fenced`. The behaviour is harmless: both answers are success, and the one obligation is
  installed. The unconditional sentence is wrong, though. Qualify it, e.g. "…when neither commit's
  outcome is unknown; an Abort that cannot tell its own fence from a rival's identical one answers
  `Fenced`". The brief's (D) race leg (`tests/multipart_abort.rs:662`) is a pure-Conflict race, so
  it does not contradict this.

- NEEDS-HUMAN [human] — **No wait between rounds, unlike Create.** `abort.rs:34` ("retried at
  once, with no backoff") and `abort.rs:58` (`ABORT_ROUNDS` = `MAX_COMPLETE_ATTEMPTS + 1` = 4) give
  four back-to-back rounds. A TiKV lock conflict with nothing moved spends one of them
  (`abort.rs:332`). Create, in the same spot, backs off with jitter
  (`crates/core/src/multipart/create.rs:454`). Concrete case: on TiKV, any other transaction
  holding a lock on `mpu:<id>` for longer than about 4 × (one commit + up to 3 reads) ends the
  Abort with `Stopped { RoundsSpent }`, though nothing is wrong. Examples are a stalled transaction
  until its lock TTL expires or, once #657 lands, part commits that `require` the session.
  Correctness holds: the error is honest, and the tests prove it never claims a fence it lacks.
  This is a liveness question, and today only rival Aborts and the restore fence lock that key.
  Decision: does Abort need a `CreateRuntime`-style wait seam before #668 gives it a production
  caller, or is a 500 under lock contention acceptable? The brief's "spend a round and try again"
  asked for exactly what shipped, so the call is on scope, not on the build.

## Refutation attempts that failed (the fix held)

- **Evidence re-run.** The frozen C4-ci log shows the 21 new core tests and the 6 new DST tests
  green. It also shows the madsim DST step ran: `cargo test -p wyrd-dst (--cfg madsim)` at log
  line 3843, past the deny step the brief expected to block it. That covers (G), and (J) via the
  custodian DST (33 passed). I rebuilt in scratch and got the same results. C4-verify is
  UNVERIFIABLE because the test imports `multipart::abort`, which does not exist on the reverted
  base (E0432). The brief pre-declared this, and it is not a refutation. C4-diff-cov's "does not
  apply on origin/main" happens because the bundle's base is main plus #894. That is a gate
  environment issue, not evidence against the fix.
- **Production path, not a copy.** The tests call the real `abort_session` (`abort.rs:282`) and
  `fence_batch` (`abort.rs:77`) over a real redb store. Wrappers only script store answers. The
  rival in H4 and in the last-round test is the real `abort_session` running on the inner store.
- **A rival lands between `lost`'s reads** (`abort.rs:370`): the third session read sees the move,
  and the answer is `AlreadyAborting` (H4, `tests/multipart_abort.rs:1009`). If this Abort's own
  held batch lands between the reads, the same path finds its put and answers `Fenced`.
- **A taken key while a held doubt exists** (`abort.rs:333`) ends `InDoubt`, not as damage. That
  is the right call: a held batch could still land if the key were later removed.
- **The last round's re-read is answered**, because the budget check (`abort.rs:314`) comes after
  the state answer. A session reopened at `Open@E'` is re-fenced there, with its key at `E'`.
- **Restore equivalence:** `crates/custodian/src/restore.rs:1055` builds the same
  `require` → `require_absent`* → put session → put obligations* sequence as the removed inline
  code. The custodian unit tests and the custodian DST pass unmodified.
- **(E):** no batch touches `mpuctl`, and every leg asserts the count. The DST checks
  `mpu:` records == `mpuctl.count()` after every seed.

Scratch work ran under `$PDCA_SCRATCH/pdca-adversary-895-settle` and is removed.

<!-- pdca:leaf-complete -->
