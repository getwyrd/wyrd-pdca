# Adversarial review: issue 927 (part commit)

**Result: refuted on one defect family, damaged session bytes under a commit still in flight.** It is the same family as the gating T4-batch-review failure, but wider than that finding says. I found no false `Committed` and no compensation sent during a live commit unless the session bytes were damaged or the code was deliberately mutated.

All probes and edits ran in a scratch copy of `$PDCA_TARGET`, using the real `commit_part` over the test file's real harness. Baseline: 44 of 44 tests green.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:878`: confirms the T4 blocker. `session_seen` runs on the first session read before anything decodes it. Failing case: hold a commit (`may_still_commit: true`) and plant `b"torn"` at `mpu:` straight after it. The first read clears the commit's may-land flag, the slot read (`Held`) then drops it from the doubt, and the last read returns a record error. The answer is `Stopped(Record)`, classed `Terminal`, with the message "no batch of this part attempt is in doubt ... what it staged stays in the store". After repairing the session, the held batch answered `Committed` and the attempt's own part was durable. So the error's claim was false.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:496`: the root cause is wider than T4's line. `session_seen` treats any different bytes as "an epoch only rises", without decoding them, and it is also called at `:896` and `:1136`. Fixing only `:878` leaves these two failing cases:
  - Damage right after the commit's slot read (`:896`): `InDoubt { may_still_commit: false }`, then the held batch lands after repair.
  - A held compensation with damage straight after it (`:1136`): `InDoubt { batch: Compensation, may_still_commit: false }`, then the held batch lands after repair.

  The fix should count the session as moved only for a record that decodes and is either not `Open` or at a higher epoch. Undecodable bytes should settle nothing, which is the rule the sign-off set for `part:` damage. `crates/core/tests/multipart_upload_part_commit.rs:3642-3683` tests that rule for the part key only. The session-damage rows at `:3212` use `Act::conflict()` with no held batch, so they cannot catch this.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:878`: the same defect can give a wrong answer for a landed commit, not just a wrong error. Failing case:
  1. Empty prior. Hold the commit, land a non-empty rival, and damage the session.
  2. Repair the session right after the first session read.
  3. When the compensation is sent, an empty rival restores the prior's bytes, the held commit lands, and the compensation answers unknown and unapplied.

  The answer is `Ok(Refused(OperationAborted))`, with one compensation sent and the attempt's own part durable. This breaks the rule that no compensation is sent while the attempt's own commit may still land. Its trigger is narrow, since it needs damage that gets repaired mid-round. Replacing the `:878` call with `let _ = &first;` gives the in-doubt error with no compensation here, so that call alone causes this case. The full suite stays green with that edit, so no test pins the call. It does not fix the flag: the first failing case then still reports `may_still_commit: false`.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:490`: no test pins `may_still_commit: false` as "out of flight". With `may_land: true` hard-coded, all 44 tests stay green. Failing case the suite lacks: every commit round answers `Act::unknown(false, false)`. The real code spends 4 rounds, then compensates and returns `Stopped(CommitRoundsSpent)` with `slot_released: true`. The mutant returns `InDoubt { may_still_commit: true }` and leaves the slot held, which is the slot leak leg (G) exists to prevent. The probe passes on the real code and fails on the mutant, so it can be added as written.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:579` (with `:926`): nothing tests the arm that blocks a false `Committed` when a slot is rewritten under the attempt's own id while a commit is in doubt. The existing "rewritten" legs have nothing in doubt, and there `Gone` and `Rewritten` give the same answer. With that check forced to `Gone`, all 44 stay green. Failing case: hold a commit, then rewrite the slot under its own id. The real code answers `InDoubt(SlotLost)`. The mutant answers `Committed` with no part record in the store. A conforming attempt should not reach this history, but the arm is the only guard on the invariant if one does.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:870` (answered at `:954`): an absent part key is treated as a rival. Carry-forward fix 1 made undecodable bytes a `Stop::Record`, but `Ok(None)` still counts as a moved key. Failing case: a re-upload over a non-empty prior, with the prior's `part:` key deleted before a `Conflict` and the session still `Open@E`. The answer is `Refused(OperationAborted)`, plus a compensation, with no rival anywhere. The Design says a `part:` record is removed only after the session leaves `Open`, so this history is damage. The rubric's *Absent or unsupported entries* class wants an explicit error here, and the attempt should not report a same-number loss. The prior part's chunks now have no record naming them, and nothing reports that.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:598` and `:1187`: two arms the suite never runs. Replacing either with `panic!` leaves all 44 green, and diff coverage lists both as MISS.
  - `:598` is the "session gone" row in the module doc's table, and it has no test.
  - `:1187` looks unreachable. The previous lap's session read already cleared every may-land flag, so the slot read on this lap empties the doubt first. Either delete it or show a history that reaches it.

## The verdict's claims

- `check-gates.json` C5-mutants ("63 caught, 15 unviable"): accurate, but weaker than it reads. It shows no missed mutants among the 78 that `cargo mutants` generated. Of my 26 hand edits, 21 were killed and 5 survived: the `:878` call, `:490`, `:579`, `:598` and `:1187`. It should not be read as proof that the classification logic is fully pinned.

- `check-gates.json` C4-verify (`unverifiable`): this is not a refutation. The test cannot compile without the new module, and the brief declared that in advance. The brief's demonstrated reds are recorded in `build-notes.md`, which I was not given, so I did not check them from there.

- `check-gates.json` C4-diff-cov (97.8%): the 14 missed lines are mostly store-fault and decode arms. `:598` and `:1187` are the two that matter, covered above.

## Attacked, and could not refute

- **Carry-forward fix 2:** each of the five named edits is killed by the test written for it: `can_recur` forced false, the ceiling's exclusion of the part's own prior, `require_absent` on the obligation key, `.last()` in the doubt search, and the epoch chase keeping only the latest obligation.
- **Tests:** they drive the production path, and the wrapper's slot-byte and margin checks fail the test from inside `commit`, so they are real checks. Other hand edits were killed too: dropping the `mpu:` requirement from either batch, using the first session read for the own-commit proof, removing the read-only laps, and stamping the wrong epoch or time.
- **Carry-forward fix 4:** the C4-ci log shows a complete run in 357 s. `custodian_gc` finished, and machete, deny, the conformance tests and the DST (deterministic simulation) tests all ran.
- **Classification by reading the code:** I found no input that breaks these outside session damage:
  - the slot-then-session read order;
  - the rule that a commit landed before a rollback stays in doubt;
  - bytes returning only for an empty prior;
  - the loop bounds;
  - the renewal hand-off (`stage.rs:1642-1772`).
- **Not raised, because already settled:** seeded DST coverage (`// deferred: #660`), slots left held after a Complete rollback, and the dispatch-only lease promise.

<!-- pdca:leaf-complete -->
