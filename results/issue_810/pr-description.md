## Summary
**User impact:** after an operator restores the metadata store from an older image, a
safety pass closes every upload the restore brought back, so none can publish over data
that was already cleaned up. Nothing in the store said whether that pass had actually
finished. The operator had only the console output of one run, and a gateway would have
nothing to check before accepting multipart uploads again. No client can create multipart
uploads yet, so nobody hits this today; this closes the gap before they can.

This PR makes the pass keep a small durable record of each run and whether it finished
cleanly, prints that in the command's verdict, and documents how a future gateway check
may (and may not) rely on it.

This PR is stacked on #846, #847, #851 and #856 (the earlier parts of the same restore
work); its diff reads cleanly once those land.

## What to look at
- **The record.** Each run of the post-restore pass is numbered (1, 2, 3, …). Before it
  changes anything, the run records "run N, not complete". At the very end it records
  "run N, complete", but only if every write it made was confirmed and no upload is left
  for a human to fix.
- **Unfixed problems are never forgotten.** If an earlier run closed an upload but found
  something wrong with it, every later run checks that upload again from what is in the
  store. Re-running without a repair, or after a run that crashed halfway, does not turn
  the record to "complete".
- **Only upload problems hold it back.** A lost or misplaced file chunk is still reported
  for a human, but it does not stop the record reading complete, since it cannot let an
  upload publish.
- **What the record cannot tell.** A restore rewinds this record with everything else, so
  a backup taken after an earlier run finished comes back reading "complete" until the
  new run starts. Nothing reads the record yet. Until the gateway check (#508) adds a
  restore-specific signal beside it, safety rests on the runbook's order: run the pass
  with writers stopped, and re-enable no gateway until a run says COMPLETE.
- **To try it:** run `wyrd custodian --reconcile-after-restore` on a store with one open
  upload. The verdict now has a `restore-fence generation 1 COMPLETE` line, and the store
  holds `mpufence = {"generation":1,"complete":true}`. Add an upload record that cannot
  be read and run it again: generation 2, NOT complete.

## Root cause
0016 (D-B, decision 1.4, `0016:717-728`, X17b, `0016:3017-3021`) requires the
restore-fence generation to complete before any gateway serves multipart verbs on a
restored image, but no record of it existed: `reconcile_after_restore` wrote nothing a
later reader could check. An earlier attempt at this record (#664, first iteration) also
skipped every session already in `Aborting`, so repairing nothing and re-running cleared
the finding and would have certified the generation complete with the fence's obligations
still unmet.

## Fix
- **The record** (`crates/core/src/multipart.rs`): `MPUFENCE_KEY = b"mpufence"` (`:1141`),
  `FenceGeneration { generation: NonZeroU64, complete: bool }` (`:1955`), `next`
  (`:1964`) and `decode_fence_generation` (`:1997`). The wire shape is closed and
  canonical, on the `mpuctl` model. Generation 0 is refused at decode, so "absent" is the
  only spelling of "no run yet". 0016 names the generation but gives it no key, hence
  `mpufence`.
- **Opening a generation** (`crates/custodian/src/restore.rs`): `open_generation`
  (`:901`) is the first thing `reconcile_after_restore` does (`:599`). It reads and decodes
  the record (a torn record or `u64::MAX` is an `Err` before any write), then writes
  `{N+1,false}` conditioned on the exact bytes read (`require` / `require_absent`,
  `:913-916`). Only `Committed` lets the pass continue; `Conflict` and every `Err`,
  including an unknown outcome, stop it (`:923`).
- **Closing it**: `close_generation` (`:946`) runs only on the fence's `Ok` path (`:878`).
  It writes `{N+1,true}` conditioned on the `{N+1,false}` bytes it opened with, and only
  when `RestoreReport::sessions_settled()` (`:409`) holds: no unsettled session and no
  fenced session with unaccounted segments. `needs_human()` is unchanged.
- **Already-fenced sessions are judged every run.** The fence from #841/#842 is untouched:
  its `Aborting` arm already re-checks each fenced session through `recheck_fenced`. The
  residue is therefore re-derived from each session's own records on every run, and
  nothing is stored on the generation record beyond `{generation, complete}`.
- **Mark commits**: `commit_marks` (`:987`) now treats a `Conflict` on a mark batch as an
  `Err`. The base ignored the outcome, which would have let "complete" follow a write the
  store did not take.
- **Report and audit**: `RestoreReport::fence_generation` (`:239`, set at `:662` and on
  close at `:962`); audit actions `fence-generation`, `fence-generation-not-complete`,
  `fence-generation-write-failed`, `fence-generation-fault`; the summary line gains both
  generation fields, and a failed completion write says so instead of reusing the
  "fence did not finish" text.
- **CLI** (`crates/server/src/cli.rs`): `fence_generation_line` (`:1460`) is line 2 of
  the verdict (`:1314`). It does not change the exit status: a not-complete generation on
  a returned report always comes with a session finding that already sets it.
- **Docs**: the key, shape and what it cannot tell in one place
  (`05-building-block-view.md:204`); a paragraph in `06-runtime-view.md` §6.5 (`:67`);
  the m4 runbook's step 7 (`m4-first-deployment-blueprint.md:647-654`).
- **Existing tests**: `restore_completing_fence.rs` and `restore_open_fence.rs` compare
  store snapshots with `mpufence` removed (every run now rewrites it, by design);
  `multipart_keys.rs` adds `mpufence` to the key-disjointness matrix.

## Verification
Line numbers are on this branch (main + #846 + #847 + #851 + #856 + this patch). Tests are
in `crates/custodian/tests/restore_fence_generation.rs` unless noted.

- **Claim:** the record is absent before any run, reads not complete during a run, and
  complete after a clean one.
  - **Checked:** `restore.rs:599`, `:901`, `:946`.
  - **Test:** `the_generation_record_is_durable_and_names_the_pass_that_wrote_it` (`:478`).
- **Claim:** a "complete" restored from the backup is replaced before any mark or fence.
  - **Test:** `a_restored_complete_is_replaced_before_any_mark_or_fence` (`:511`).
- **Claim:** an upload that needs a human (no nonce; a segment naming a chunk no part
  holds) leaves the generation not complete; unrelated findings (a dangling chunk,
  unreadable `inode:` / `pending:` / `part:` records) do not.
  - **Checked:** `restore.rs:409` (`sessions_settled`).
  - **Test:** `a_session_that_needs_a_human_leaves_the_generation_not_complete` (`:556`),
    `only_a_session_finding_withholds_completion` (`:583`).
- **Claim:** unrepaired problems survive a re-run and a crashed run; only repairing both
  completes the generation, and either repair alone does not.
  - **Test:** `residue_survives_a_re_fence_and_only_both_repairs_complete_the_generation`
    (`:647`), `residue_survives_an_interrupted_pass` (`:723`). A version that skips every
    already-`Aborting` session (the #664 first-iteration behaviour) fails both and passes
    the rest.
- **Claim:** "complete" is never written over unfinished or unknown work, and a late
  completion never covers a newer run.
  - **Checked:** `restore.rs:913-916`, `:923`, `:965`, `:987`.
  - **Test:** `a_failed_mark_or_fence_commit_never_completes_the_generation` (`:781`),
    `an_unsettled_opening_write_runs_no_mark_and_no_fence` (`:816`),
    `an_unknown_completing_write_is_true_or_finished_by_the_next_pass` (`:854`),
    `a_late_completion_never_masks_a_newer_pass` (`:886`),
    `a_record_changed_under_the_pass_is_never_written_over` (`:922`, a racer writes the
    record between the run's read and its commit, including the exact bytes the run
    would write), `a_torn_or_exhausted_generation_record_stops_the_pass_before_any_write`
    (`:971`).
- **Claim:** the report and the operator's verdict carry the same generation the run
  stored.
  - **Test:** `the_report_carries_the_generation_the_pass_wrote`
    (`restore_open_fence.rs:867`), `a_completion_that_does_not_land_says_so_in_the_summary`
    (`restore_open_fence.rs:901`), and in `wyrd-server`,
    `restore_verdict_prints_the_generation_the_pass_left_in_the_store` (`cli.rs:3461`),
    which runs the one-shot's real backend dispatch over a redb file twice and reads
    `mpufence` back.
- **Claim:** the record's codec round-trips and refuses torn or foreign values.
  - **Test:** `crates/core/tests/multipart_budget_admission.rs:469`, `:497`.
- **Red → green:** with the production changes reverted and the new test file kept, all 12
  tests in `restore_fence_generation.rs` compile and fail by assertion. With the fix, all
  12 pass. The tests that name the new API (report field, decoder) were checked against
  hand mutants instead; each one fails at least one of them.
- **Whole gate:** `cargo xtask ci` passes (typos, docs render and link audit, fmt, clippy,
  workspace tests, the statics gate, and the DST crate under `--cfg madsim`).

**Follow-ups (not in this PR):**
- A re-run does not yet hold the generation back for a fenced session whose
  `retire:bytes` obligation is missing (`restore.rs:1163-1168` checks only the records
  obligation and the segment range): #855.
- Operator wording: the intended rule is that no writers resume (runbook step 8) until a
  run says COMPLETE, as the new runbook text says. The CLI's NOT-complete line ("Keep
  multipart uploads off this store") and the older runbook line above it
  (`m4-first-deployment-blueprint.md:644`) still say multipart only; they should be
  aligned, and revisited in #508, which can switch off only multipart.
- DST coverage of the fence and this record is #843.

Fixes #810
