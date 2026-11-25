## Summary
**User impact:** a multipart upload stamped under a clock the reaper does not read could never be
cleaned up. It kept its staged bytes and its upload slot forever, and once enough of them piled
up, every new multipart upload on the cluster was rejected with `503 SlowDown`, with no way out.

This PR adds an operator command, `wyrd mpu abort --upload-id ID`, that ends one such upload. The
existing cleanup pass then reclaims everything it held.

This is the first half of the operator exit in proposal 0016 (follow-up FU-6). The `expire` verb
and the alarm for foreign-clocked uploads come in a separate change built on this one. It builds on
the multipart reaper (#625) and the session state machine (#693, #658, #659), and should ship in
the same stack as #625.

## What to look at
- **The command.** `wyrd mpu abort --upload-id ID [--data-dir DIR] [--metadata-backend B]`. It
  takes one upload id and nothing else that could widen it: no second id, no wildcard, no clock
  flag. Unknown flags are refused before the store is opened.
- **What it does.** It moves an open or completing upload to "aborting" and records what has to be
  reclaimed, all in one commit. It deletes nothing itself. The normal cleanup
  (`wyrd custodian --reap-abandoned-uploads`) does the teardown, and the abort prints that command,
  ready to paste, for the store it acted on, plus when to stop running it.
- **What it refuses.** A completed upload or a missing one exits 1 with a message saying which. An
  upload already aborting is left alone and exits 0.
- **To try it:** run `wyrd mpu abort` against a local redb data dir with an open upload in it, then
  run the printed custodian command until the printed check says the upload is gone. The new
  integration test file does exactly this through the real binary.
- **Interim surface.** This lives on the development CLI until the management plane of proposal
  0008 exists. Its authority is host access to the store, with no listener and no new auth path.
  The architecture docs record that it moves to the management API unchanged.

## Root cause
The reaper's clock guard skips any session whose `clock_source` is not its own
(`crates/custodian/src/reaper.rs:49-56`, `:824-830`). For a foreign-clocked `Open` session that is
the right call, since the reaper cannot judge its age, but proposal 0016 names the operator as the
exit and no operator command existed: `cli.rs` had no multipart subcommand.

## Fix
- **`crates/core/src/multipart/abort.rs`:** new `operator_abort` (`:405`) and its answer type
  `OperatorAbortOutcome` (`:335`).
  - An `Open` upload gets the same fence a client abort makes (`open_teardown`).
  - A `Completing` upload gets the same fence the post-restore pass makes (`completing_teardown`):
    the session obligation plus a records obligation naming only that attempt's segment range.
  - Both go through the existing `fence_batch` (`:95`) as **one** commit, conditioned on the
    session record's exact bytes and on the obligation keys being absent. So no state can have a
    fenced upload whose segments nothing will reclaim.
  - The retry/doubt loop that `abort_session` already had moved into a shared private
    `fence_rounds` (`:486`), so the client abort and the operator abort use one copy. The client
    abort's behaviour is unchanged.
- **`crates/server/src/cli.rs`:** the `wyrd mpu` group (dispatch `:432`, usage `:512`) and
  `cmd_mpu_abort` (`:3117`).
  - Strict flag parsing; no store is ever created (redb opens an existing store only).
  - Every path the printed follow-up command carries (redb `--data-dir`, FDB cluster file,
    FDB external-client directory) is made absolute and shell-quoted (`store_spelling` `:3257`,
    `path_environment` `:3307`). A path with no UTF-8 spelling is refused before the store is
    opened (`absolute_path` `:3170`).
  - The exit-0 answers include a stop condition for *this* upload, since a cleanup pass reports
    `BLOCKED` while any other upload is held (`operator_abort_answer` `:3393`).
- **`crates/core/src/multipart.rs:4154-4161`:** doc only. The `RetirePayload` table now says all
  three Completing→Aborting paths install `{session, all}`.
- **Docs:** `docs/design/architecture/06-runtime-view.md:96` and
  `08-crosscutting-concepts.md:113` describe the verb, its refusals, and the move to the
  management API.

Both doors install `{session, all}` rather than a listed part set. That reuses the existing
constructors, and a listed set can outgrow the value size limit for a sparse upload. No part commits
outside `Open`, so `all` names exactly the parts.

## Verification
Line numbers are on this PR's branch.

- **Claim:** aborting an open upload fences it, and the existing cleanup pass alone tears it down.
  - **Test:** `crates/server/tests/multipart_operator_abort.rs:550`
    (`abort_fences_an_open_session_and_the_one_shot_tears_it_down`). It checks
    `Aborting@E+1` plus the obligation after the abort, then records gone, release marks written
    and the admission count decremented after the custodian one-shot.
- **Claim:** aborting a completing upload reclaims that attempt's segments and no other's.
  - **Test:** `multipart_operator_abort.rs:625`. A decoy later-attempt segment range is never
    named by an obligation and survives the drain.
- **Claim:** refusals are real, handled refusals, not "unknown command".
  - **Test:** `multipart_operator_abort.rs:714`. Completed and absent uploads exit **1** with a
    message naming the state; an unknown command would exit 2. An already-aborting upload exits 0
    with no second fence and the epoch unchanged.
- **Claim:** exactly one upload per call, with nothing written on bad input.
  - **Tests:** `:785` (second id, wildcard, unknown flag such as `--clock-source`, repeated flags)
    and `:904` (a taken obligation key is named and nothing is written).
- **Claim:** the printed commands work as printed, from another directory, while another upload
  is held.
  - **Test:** `:937`. It runs the printed lines through a real shell after a directory change,
    with a second upload holding every pass `BLOCKED`.
  - **Tests:** `:1084` (redb) and `:1124` (FDB, `--features fdb`). A path with no UTF-8 spelling
    is refused before the store is opened.
- **Claim:** the fence is one batch, so no interruption leaves a fenced upload with unreclaimed
  segments.
  - **Checked:** `crates/core/src/multipart/abort.rs:405-470`. Each door builds one `fence_batch`
    and sends one commit.
  - **Tests:** `crates/dst/tests/custodian.rs:8502` (`mod operator_abort_rows`) is simulation
    testing.
    - `:9692-9728` cut the operator process at every point across the commit, for each door. Only
      "unchanged" or "fenced with every obligation" are ever reached.
    - `:9730-9740` race the abort against a live Complete. Every end is either `Completed` with no
      obligation on that attempt's segments, or `Aborting` with them named and drained.
    - `:9742` covers a commit whose outcome the store could not report.
- **Red → green:** with the production change reverted and the new test file kept, all 7 tests in
  `multipart_operator_abort.rs` fail (the base answers `unknown command`, exit 2). With the change,
  all 7 pass (8 with `--features fdb`).
- **Mutation checks:** reverting each piece on its own turns a named test red. That covers
  dropping the segment obligation, putting it in a second commit, naming the whole segment range
  instead of one attempt's, letting abort apply to `Completed`, and making the verb delete
  records itself.
- **Gates:** `cargo xtask ci` passes, including the simulation suite at 50 seeds. One known flaky
  gateway test (`crates/gateway-s3/src/lib.rs:4919`) fails about 1 run in 12 on the unpatched base
  too; this PR does not touch gateway code.

**Known limits, left for follow-up:** the FDB `[store: …]` label still shows the cluster file as
given (the printed command uses the absolute path). An unset FDB external-client directory is not
printed as explicitly empty. The "run over the whole fleet" hint is shown for any hold on this
upload, not only partial-fleet holds. The `--features fdb` tests are run by hand, not in CI.

Fixes #1018
