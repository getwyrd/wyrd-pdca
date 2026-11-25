## Summary
**User impact:** When two uploads of the same part race, or an abort or a failed Complete lands while a part is being saved, the losing upload used to stop and keep its place in the upload's limited set of in-flight slots. Nothing frees that place while the upload stays open, so a client that keeps retrying could fill the upload's slots and be locked out until the upload times out. Clients see no change from this PR alone: the S3 `UploadPart` request is connected later, in #669.

This PR makes a part that lost its save work out why it lost, clean up after itself when the upload is still open, and tell the client either to retry the part or that the upload is gone.

It is the second of four changes to this step and builds on #1027 (PR #1041), which must merge first. #1029 and #1030 follow.

## What to look at
The main question is which answer each kind of loss gets, and whether the cleanup ever runs when it should not:

| What the attempt finds after losing | Answer | Writes |
|---|---|---|
| The upload is still open; another upload of the same part won, or the old part was repaired | retry the part (`OperationAborted`) | frees its slot and schedules its own data for deletion, in one write |
| The upload was rolled back to a newer open state by a failed Complete | retry the part (`OperationAborted`) | the same cleanup, under the newer state |
| The upload was aborted | the upload is gone (`NoSuchUpload`) | nothing; the existing cleanup of aborted uploads handles it |
| The upload was deleted, and its slot went with it | the upload is gone (`NoSuchUpload`) | nothing |
| Nothing explains the conflict | send the same save again, up to four times | — |
| A record that does not decode, or a state no legal sequence produces | a stop that names the damage | nothing |

To try it, run `cargo test -p wyrd-core --test multipart_upload_part_commit`. The 47 tests use a real in-memory metadata store. A test wrapper forces each race by running an action (a rival save, an abort, a rollback, a delete) between two of the attempt's reads, and can answer any number of rounds with a conflict. There is no S3 path to exercise yet.

## Root cause
After #1027, `commit_part` stops on any `Conflict` from its commit or its compensation and keeps its `slot:` record. The reaper does not walk an `Open` session (`0016:1600-1620`), so under a live session nothing released that slot before `W_session`.

## Fix
- **Classify from a re-read** (`crates/core/src/multipart/commit.rs:959` `lost_commit`). After a `Conflict`, read the part key, the session, the attempt's own slot, then the session again. Only the last session read decides. A session counts as moved only if it decodes at a higher epoch than the batch required. Bytes that do not decode, or a record at the same or a lower epoch, are damage (ADR-0045).
- **Compensate under a live session** (`commit.rs:840` `compensate`, `:893` `refuse`): one batch with the attempt's own `retire:bytes:` obligation (none for an empty part), the slot release and the `sidx:` deletes. The answer is exactly `Refusal::OperationAborted`, never `NoSuchUpload`, because the session is `Open`.
- **The epoch chase** (`commit.rs:881`): a compensation that lost to a newer `Open` epoch is sent again under that epoch, up to `COMPENSATION_ROUNDS` (`:252`).
- **A terminal delete is not damage** (`commit.rs:629` `session_gone`): when the last session read finds nothing, read the slot key once more. Absent means the delete took both in one batch (`0016:674`), so `NoSuchUpload`. Any record there, this attempt's or another's, is `Stop::Record(SessionRecordMissing)` with no compensation, since a reservation requires the session's `Open` bytes (`crates/core/src/multipart/stage.rs:1331`).
- **Bounds:** `COMMIT_ROUNDS = 4` (`commit.rs:240`), `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS + 1` (`:252`), and the lease margin checked before every later commit round (`:1081`).
- **A taken obligation key** (`commit.rs:607` `obligation_key_free`, `:930` `unexplained_stop`): a re-upload over a non-empty prior requires its obligation key absent in every round, but no round reads that key. Before the compensation that ends unexplained rounds, the key is read; a record there is `Stop::ObligationKeyTaken` with nothing written. Without this, an empty re-upload compensated cleanly and the damage went unreported.
- **Docs:** the module doc of `commit.rs` (sections *A commit that lost*, *The compensation, and the epoch chase*, *The stops*, *Bounds*, from `:48`), the widened doc of `Refusal::OperationAborted` (`crates/core/src/multipart.rs:4755-4766`, no new variant), and one sentence in `docs/design/architecture/05-building-block-view.md:250`.

No change to `stage.rs`, to Complete, the reaper, the gateway or any production caller.

## Verification
Line numbers are for this PR's branch. Tests are in `crates/core/tests/multipart_upload_part_commit.rs`, appended after #1027's.

| Claim | Checked in code | Test |
|---|---|---|
| A session fenced before the commit answers `NoSuchUpload` and writes nothing | `commit.rs:993` | `:2952` `b_a_fenced_session_answers_no_such_upload_and_writes_nothing` |
| A rollback to a newer `Open` epoch compensates under that epoch and answers `OperationAborted` | `commit.rs:996` | `:2981` `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` |
| A same-part loser compensates its own chunks only; a repair of the prior is not retried | `commit.rs:1005` | `:3011`, `:3035` |
| A conflict no read explains sends the same batch again, for the commit and the compensation | `commit.rs:998`, `:883` | `:3067`, `:3099` |
| A terminal delete between reads answers `NoSuchUpload`, at the commit and at the compensation | `commit.rs:629` | `:3149`, `:3197` |
| A missing session with a slot still there, this attempt's or another's, is damage | `commit.rs:636-641` | `:3235`, `:3274` |
| Damaged part or session records are record stops, never a rival or a fence | `commit.rs:538`, `:980` | `:3322`, `:3364`, `:3425`, `:3490` |
| A lost or rewritten slot stops without a release, under a live and an aborted session, at both re-reads | `commit.rs:574`, `:989`, `:875` | `:3524` (16 cases) |
| A taken obligation key keeps those bytes and the prior part, with and without chunks | `commit.rs:930` | `:3607` (4 cases) |
| Every loop ends after exactly its bound, checked against numbers written in the test | `commit.rs:1075`, `:885` | `:3676`, `:3758` |
| A compensation that finds the session aborted stops; a lease used up between rounds stops and compensates | `commit.rs:879`, `:1081` | `:3792`, `:3835` |

- **Pre-fix and post-fix:** the tests are appended to an existing file, so they were checked by breaking each guard on purpose rather than by a separate red run. Each of these edits failed the named test: taking a slot read made before the session read as final (once for the commit, once for the compensation, so each re-read is checked on its own); treating every missing session as a delete; counting another attempt's slot as gone; compensating without the re-read; sending a rolled-back session to the reaper; answering anything but `OperationAborted` for a live session; reading a damaged part as a rival; reading an unrisen epoch as a fence; removing the obligation key's precondition; a loop counter that never reaches its bound; lowering `COMPENSATION_ROUNDS` by one; skipping the lease check on a later round. All 47 tests pass with the fix. The test wrapper also fails any case where one attempt sends a fifth commit or a fifth compensation.
- **Mutation testing:** `cargo mutants --in-diff` tested 36 mutants: 24 caught, 12 did not compile, none survived.
- **Full gate:** `cargo xtask ci` passed on this change stacked on #1027. The `tikv` feature build also passed.
- **Coverage:** diff coverage was not measured, because this branch depends on #1027 and does not apply to `main` alone.

**Known gaps**
- A compensation that finds its own obligation key taken (`commit.rs:863-865`) has no test of the answer, only of the read order. Ignoring the result of that read still passes all 47 tests.
- The case "rolled back to a newer epoch, and the slot is gone" (`commit.rs:989`, `:875`) is not tested at either re-read. A break that sends one extra batch before the same `SlotLost` answer survives.
- Under an aborted session, a missing or foreign slot answers `NoSuchUpload`, not a damage stop. This was decided on purpose; nothing is written either way.
- A store fault on the obligation-key read has no test.
- Seeded simulation testing of these races is deferred to #660 (marked at `commit.rs:201` and in the test file at `:2773`). Here each race is forced by the test wrapper.
- An unknown result from the store still stops the attempt; #1029 and #1030 handle it.

Fixes #1028
