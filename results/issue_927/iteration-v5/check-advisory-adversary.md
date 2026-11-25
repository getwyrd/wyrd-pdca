# Adversarial review: #927, part commit (iteration 5)

**Result: refuted on one point.** The blocking T4 finding is real, and I reproduced it by running it. It needs a damaged session record, so no legal store history reaches it. The rest of the classification held up under attack. The probes and trial edits ran in a scratch copy of `$PDCA_TARGET`, and none of them are in the patch.

## Refutations

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:666`: `judge_session` treats any non-`Open` record that decodes as a real fence, whatever its epoch. Every legal fence raises the epoch (`Open@E → Aborting@E+1`, `crates/core/src/multipart.rs:2612-2616`; `Completing@E+1`, `0016:705`), so `Aborting@E` is damage, like the `Open` case the code already rejects at `:660`. `Doubt::session_seen` (`commit.rs:516-525`) then clears the doubt, which breaks the rule that damage settles nothing. I planted `{"kind":"Aborting"}` at the same epoch and got these results:
  - **Held commit, damage stays:** the answer is `Refused(NoSuchUpload)`. After the session is repaired, the held batch answers `Committed` and the part is durable.
  - **Held commit over an empty prior with a rival, damage after read 1 and repaired after read 2:** a compensation is sent while the commit can still land, and the answer is `Refused(OperationAborted)`.
  - **Held first upload, same damage and repair, with the compensation answered unknown as the held commit lands:** the answer is `Stopped { CommitRoundsSpent, slot_released: true }`. The attempt's own part is durable and no obligation exists, so the error's "no batch is in doubt" and "chunks are retired" are both false.
  - **Held compensation:** the answer is `Refused(NoSuchUpload)`, and the batch lands after the repair (the `Left` path at `commit.rs:1265-1274`).

  **Trial fix:** in the `Refused` arm, return `Left` only when `record.epoch() > required.epoch`, and otherwise return `SessionEpochNotRisen`. With that one change, all 57 existing tests and the three probes above that assert pass (60 of 60). The fourth case above only printed its result, so I did not re-check it under the fix. The builder should also widen the doc at `crates/core/src/multipart.rs:386-394` and `commit.rs:88-92`, which say only `Open` records can be this damage.

  **Sign-off wording:** the iteration 3 carry-forward says a record that "decodes and is not `Open`" counts as moved, which is what the builder wrote. This fix narrows that rule, but it changes no legal history.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:4008-4021`: this is why the suite stayed green. `session_damage` plants only torn bytes and an `Open` record at the same epoch, and the repaired-mid-round leg plants only `TORN` (`:4117`, `:4154`). Add a non-`Open` record at the same epoch and at a lower epoch to `session_damage`, and use one in the repaired-mid-round leg. Without that, a fix to `:666` has no test that fails if it is reverted.

- NEEDS-HUMAN [human] — `crates/core/src/multipart/commit.rs:645-647`: a missing `mpu:` record has the same flaw, but a test pins the current answer, so this is a decision. The module doc calls it a terminal delete (`commit.rs:90-91`), yet that delete removes surviving slots in the same batch (`0016:350`), so "session absent, own slot still held" is not a legal history either. I erased the key under a held commit: the answer was `Refused(NoSuchUpload)` with the slot held, and after restoring the key the held batch answered `Committed` with the part durable. A prior part record that goes missing is already treated as damage (`commit.rs:1014-1019`). Iteration 3 asked for a test of this "session gone" row, and `multipart_upload_part_commit.rs:4511-4521` expects `NoSuchUpload`. Should a held slot with no session become a record error, or does the current answer stand?

## The gate verdicts

- `check-gates.json` row `T4-batch-review`: the `fail` is correct, and the finding is not noise. Passes 1 and 3 came back clean at high effort and pass 2 caught it (`gate-logs/T4-batch-review.log`), so a clean pass here is weak evidence, even at high effort.
- `C4-verify` is `unverifiable`, as the brief declared in advance, because the new test file cannot compile without the new API. That is not a refutation. I re-ran the green leg and got 57 passed, and the tests call the real `commit_part` over a real redb store.
- `C5-mutants` reports all 68 viable mutants caught, which only covers the edits `cargo mutants` generates. I hand-ran 19 further logic edits, and 18 failed tests. These included:
  - the iteration 4 rule (`commit.rs:938`), which fails four legs
  - the obligation epoch set to 0
  - `COMPENSATION_ROUNDS` lowered by one
  - all four compensation store-fault paths
  - both missing and damaged part checks
  - both read-only laps
  - the obligation byte comparison made always true
  - the slot `Rewritten` case treated as `Gone`
- The one survivor is removing `doubt.session_seen(first)` at `commit.rs:950`, which passes all 57 tests. From reading the code, it looks harmless in legal histories: the sticky mark from the last read plus the read-only lap reach the same answer, one lap later. I did not test that, and I raise no action on it.
- `C4-ci` did run to completion this time: `custodian_gc`, machete, deny and conformance all ran, and the log ends `xtask ci: all checks passed` (`gate-logs/C4-ci.log:4552`). This closes the iteration 2 item.

## Attacked and could not refute

- **False `Committed`:** I traced rival, rollback, teardown and slot-reuse histories through the own-commit proof at `commit.rs:973-986`, and found no legal history that reaches it wrongly.
- **Compensation while a commit can still land:** in legal histories, a doubt leaves only at a slot read that follows the read showing the commit cannot land (`commit.rs:959-963`). The only break I found is the damage case above.
- **The epoch chase:** a round held under `E` and a new round under `E'` both require the same slot bytes, so at most one lands. An empty part whose slot is gone after the session moved answers in doubt rather than guessing (`commit.rs:1229-1237`).
- **Obligation key sharing:** the commit and compensation use the same retire key at one epoch. No collision is reachable, because a compensation is sent only after the commit is settled as not landed.
- **The quiesce renewal:** `stage.rs:1656-1668` returns the in-doubt error for a round still in doubt, so compensating after a settled renewal stop cannot race one of its own renewals.
- **The test wrapper's "every leg" checks:** `check_slot_bytes` and `check_margin` run inside `commit` on the test's own task (`multipart_upload_part_commit.rs:702-708`), so a breach fails the leg. They are not disconnected checks.

I did not re-raise the items ratified at sign-off, or the DST coverage deferred to #660.

<!-- pdca:leaf-complete -->
