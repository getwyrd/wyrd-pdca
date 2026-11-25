# Build notes — issue 1030 (657.2d), iteration 4: a compensation in doubt is settled from reads; the own-commit proof's false `Committed`

Base: the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1` at `db1a066` (`pdca-integrate: issue_1029`, on #1029's PR
head `1d1a1d0`, which folds in #1027, #1008 and #1028). `path:line` below is on the patched tree unless marked "base".

Files changed (`patch.diff`, against `HEAD` = `db1a066`):

- `crates/core/src/multipart/commit.rs` (production; this iteration changes comments only)
- `crates/core/tests/multipart_upload_part_commit.rs` (the named test file; copy shipped in the bundle as
  `multipart_upload_part_commit.rs`)
- `docs/design/architecture/05-building-block-view.md:250` (the living doc's part-commit paragraph)

No new public type, variant or field. `CommitError`, `InDoubt`, `Stop`, `DoubtfulBatch` keep their shape. One `Display`
string changed wording in iteration 2 and is unchanged since: `Stop::SessionMoved` says "the batch" for "the commit",
since it is now also a compensation's cause; no test or caller matched the old text.

## Iteration 4: what the carry-forward asked, and what I did

I started from iteration 3's patch (`iteration-v3/patch.diff`; it applied cleanly on `db1a066`) and changed only what
the sign-off named. The design stands, as the human said. **No production logic changed in this iteration**: the
`commit.rs` edits are a code comment and module-doc text.

1. **The untested second settle lap for an empty part** (iteration 3's `commit.rs:1284`, now `:1298`, the guard
   `(SessionSeen::Same, SlotSeen::Gone) if obligation.is_none()`). The mutant `obligation.is_none() && laps == 1`
   survived all 92 tests. I added the row the sign-off described to (S),
   `s_a_compensation_answered_unknown_is_settled_from_reads` (test file `:6110`; row `:6152`, act `:6165-6172`,
   assertions `:6195-6213`): on the rival route, the compensation `Act::hold()`, and `land_held(Committed)` at the gap
   right after the first lap's last read (`at_compensation_gaps(&attempt, 0, MOST_LAPS, [(lap width, ...)])`). It runs
   on both columns, as (S)'s other rows do:
   - **empty part**: lap 1 reads the slot held under the unchanged session, lap 2 reads it gone and the session after it
     unchanged → `Refused(OperationAborted)`, reads exactly two `compensation_lap`s (`[slot, session, slot, session]`).
   - **part with chunks**: lap 1 finds no obligation and the slot held, lap 2's first read finds the obligation →
     `Refused(OperationAborted)`, reads `compensation_lap` + the obligation key.
   - Both: one compensation batch, nothing after it, `assert_compensated` (its own obligation or none, no slot, no owned
     entry, released once, `assert_every_fragment_named`).

   **The mutant is now killed** (temporary edit, run, restore, `cmp` byte for byte): the test file goes 91 passed,
   1 failed, the failure `s_a_compensation_answered_unknown_is_settled_from_reads` at test file `:1483` with
   `Err(InDoubt { batch: Compensation, may_still_commit: true, cause: OutcomeUnknown })` where
   `Some(Refused(OperationAborted))` was expected. Only the new row can fail this way: every other empty-part row
   settles in lap 1 or ends in doubt.

2. **Rival record + erased slot** (iteration 3's `commit.rs:1032`, now `:1046`, `Ok(Some(_)) => doubt.cannot_land()`).
   **Reproduced first** with a throwaway test (appended, run, then the test file restored from a saved copy and checked
   with `cmp`; not shipped). Two histories on a first upload, the commit `Act::hold()`, everything at gap 0 of `at_gaps`:
   - *damage*: a same-number rival's first upload lands (`rig.rival`), then the slot key is erased; the held commit
     never lands (released afterwards it answers `Conflict`).
   - *legal*: the held commit lands (`land_held(Committed)`), then the same rival lands as a re-upload over it.

   Result: **both answer `Ok(Committed { part_number: 1, digest: <this attempt's> })`**, both make exactly the five
   reads of `proof_lap`, and in both the part key holds the rival's record. `assert_every_fragment_named` passes on
   both. The only difference in the store is the rival's re-upload obligation in the legal history,
   `retire:` keyed under the **rival's** attempt id (`AttemptId(...a77e0002)`), naming this attempt's chunks.

   **Narrowing rejected, documented as a third limit instead**, because narrowing cannot keep every (K) outcome and is
   not small:
   - Narrowing to "the record at the part key is this attempt's own bytes" breaks a (K) outcome the brief requires:
     `k_the_own_commit_proofs_part_read_keeps_the_coverage_of_the_reads_before_it` (test file `:7071`), event `rival` at
     gaps 3 and 4, answers `Committed` with the rival's part standing (`:7102-7106`). That is exactly the legal history.
   - Narrowing by looking for the rival's re-upload obligation needs its key, and the key names the rival's attempt,
     which this attempt cannot know. The only way is a new scan of the session's `retire:` range for this part, filtered
     by this attempt's chunks: a new read in the proof lap (every `proof_lap` trace and every `at_proof_gaps` history
     after gap 3 moves; 10 call sites in iteration 3's count, test file `:2646`, `:4427`, `:4767`, `:5820`, `:5953`,
     `:6812`, `:6886`, `:6939`, `:7054`, `:7129`), an evidence rule the plan never reviewed, and it still fails for an
     empty part: a re-upload over an empty prior installs no obligation, so the two histories leave identical stores.
     The drain can also delete that obligation, so even for a part with chunks its absence would prove nothing.

   What I changed:
   - Module doc, *A commit in doubt* (`commit.rs:156-166`): a new bullet, **"Two limits of the proof, each reached only
     through damage"**, holding the re-upload limit (moved out of the bullet above, text kept) and the new one: a
     same-number rival's record found at the part key reads exactly as this commit landing and then being replaced by
     that rival's re-upload, because a `PartRecord` names no attempt and the re-upload's obligation is keyed under the
     rival's attempt (none over an empty prior). It says only damage reaches it (a slot key erased while the session
     is `Open@E`, which no legal writer does), and that no chunk is orphaned: the commit never landed, so this attempt's
     owned `sidx:` entries still name its chunks, and Complete checks the digest it names against the record that
     stands (`super::complete::fence_complete`, `complete.rs:950-965`).
   - The empty-part compensation limit (`commit.rs:247-251`) now says it is reached only through damage, as the
     own-commit proof's two are.
   - Code comment at the arm itself (`commit.rs:1041-1046`), pointing at the module doc.
   - Living doc (`05-building-block-view.md:250`): the two own-commit limits (re-upload, rival record) in one sentence,
     with "reached only through damage" and "none is orphaned"; and the empty-part compensation limit after the
     compensation's landed answer. The living doc had none of the three limits before.

3. **Accepted deviation from (K)**: `r_damage_under_a_held_commit_keeps_the_doubt`'s last block (test file
   `:5738-5762`) answers `InDoubt { Commit, may_still_commit: true }` with the torn-part cause instead of
   `committed_as`. Kept, unchanged from iteration 3, as the sign-off said. Why it changed (iteration 3): the history
   "commit landed, then torn bytes planted at the part key" makes exactly the same reads as the adversary's "commit never
   landed, slot erased, torn bytes planted" (`d1_torn_bytes_...`, `:6876`), and the human chose "never `Committed`" for
   torn bytes at the proof read. Its comment (`:5738-5743`) says so.

4. **Human decisions** (no-resend accepted; plan-review findings covered; size backstop overridden, do not split):
   followed. No resend anywhere; still one bundle.

5. **"Make sure the next Check gets a real high-effort review"**: not something the builder can do. Flagged here for the
   human: the brief's *Review depth* line still applies to this patch.

## Repro (the brief's two, run first on the base)

Both still reproduce on the base `commit.rs` (`git show HEAD:crates/core/src/multipart/commit.rs` swapped in, the whole
patched test file run, then the fixed file restored and checked with `cmp`):

1. A compensation answered unknown although it applied: `s_a_compensation_answered_unknown_is_settled_from_reads` and
   the reworked `h4_a_compensation_answered_unknown_sends_no_further_batch` fail; the base answers
   `InDoubt { batch: Compensation, cause: OutcomeUnknown }` unread (base `commit.rs:1143`, `failed` at base `:772`).
2. Part 1 staged, commit `Act::hold()`, slot erased at gap 0: `d1_a_slot_erased_under_an_unlanded_commit_is_never_committed`
   fails on the base with `held: Ok(Committed { part_number: PartNumber(1), .. })`, no `part:` record.

## What the patch does (all iterations)

### The compensation's unknown outcome (`commit.rs`)

- `compensate`'s `Err` arm (`compensate` at `commit.rs:1329`): a `CommitUnknownResult` goes to `settle_compensation`
  with a `Doubt` for `DoubtfulBatch::Compensation`; any other error is the same definite `Stop::Store` as before.
  `failed` (base `:772-783`) is removed; nothing else used it.
- `settle_compensation` (`commit.rs:1268-1320`), mirroring `settle`: laps of the base re-read's order (obligation key
  for a non-empty part, slot, session), at most `SETTLE_LAPS`, then the obligation key once more for a non-empty part.
  Each read is folded into the `Doubt` as it returns. It answers `Compensated::Released`, so each caller's existing
  landed answer (`refuse`, `compensated_stop`, the renewal arm), only on:
  - a non-empty part: `Doubt::owns` (`commit.rs:947-959`): the obligation key at exactly the bytes `compensate` built
    for *that round*, so keyed under the round's epoch (after an epoch chase, `E′`);
  - an empty part: slot `Gone` and the session read after it `Same` (`:1298`). A non-empty part never takes this
    branch.
  Everything else keeps the doubt with a cause: other bytes at the obligation key → `ObligationKeyTaken` (flag kept);
  session moved → `SessionMoved`; session gone → the slot read once more (`gone_in_doubt`); slot rewritten → `SlotLost`;
  slot held (or, non-empty, gone) under the unchanged session with no obligation found → `OutcomeUnknown`; damage or a
  fault → `Record` / `Store`.
- `Doubt` (`commit.rs:879`) gains `batch: DoubtfulBatch`; `Doubt::of` takes it.
- `gone_in_doubt` (`commit.rs:968`) returns the cause through `Doubt::read`, so both settles share it. Same reads and
  flag rule for the commit as before.
- Flag rules as the brief's *Keeping the doubt*: falls on a session at a higher epoch (`seen_session`), and on a missing
  record only when the slot read after it finds the slot absent; kept on damage and on other bytes at the obligation key.

### The own-commit proof (`settle`, `commit.rs:994`; the part read `:1028-1052`)

When the commit required the part key absent and the lap's slot read found the slot gone, the lap reads the part key
again between that slot read and the last session read. With the last session read `Same`:

- absent → `InDoubt { Commit }`, `Stop::SlotLost`;
- bytes that do not decode → `InDoubt { Commit }`, `Stop::Record`, flag untouched (iteration 3);
- a record (this attempt's or a rival's) → `Committed`, the flag already lowered by that read. A rival's record behind a
  slot erased by damage is the documented limit (iteration 4, above).

Read only when it can decide (required absent, slot gone), not in every lap: an every-lap read would change `lap()`
itself, so every exact trace over a held lap and the gap numbering of every history at the old gap 4; the conditional
read changes 4 exact comparisons and renumbers no gap.

### Docs (same PR, per the rubric's docs-currency rule)

- `commit.rs` module doc: header `:7-9`; *A commit in doubt* (`:116-200`), with the part read, its torn-bytes rule, and
  the two limits (`:156-166`); *A compensation in doubt* (`:219-279`), its limit `:247-251`; stops table; bounds.
- Item docs: `SETTLE_LAPS` (`:414`), `CommitError::InDoubt`, `slot_released`, `InDoubt::may_still_commit`,
  `Stop::SlotLost`, `Stop::ObligationKeyTaken`, `Stop::SessionMoved`, `Stop::OutcomeUnknown`, `settle`,
  `Compensated::Released`, `compensate`.
- `docs/design/architecture/05-building-block-view.md:250`: the compensation rule, the own-commit proof's part read, and
  (iteration 4) the three damage-only limits.

## Tests (`crates/core/tests/multipart_upload_part_commit.rs`)

| Brief item | Test (line) |
|---|---|
| (S) | `s_a_compensation_answered_unknown_is_settled_from_reads` (`:6110`): row 1 on renewal/lease/ceiling/rival × both flags × non-empty/empty (ceiling skipped for empty); rows 2-4 on the rival route, both columns; **new in iteration 4:** "held, released between the laps", both columns. |
| (X3) | `x3_late_staging_that_rebuilds_a_compensated_slot_never_sends_it_again` (`:6255`) |
| (O) | `o_an_abort_after_a_landed_compensation_keeps_its_landed_answer` (`:6344`), `o_a_compensation_under_a_newer_epoch_is_settled_by_its_obligation_under_that_epoch` (`:6375`), `o_other_bytes_at_the_obligation_key_are_never_the_landed_answer` (`:6424`) |
| (T) | `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` (`:6498`), `t_a_compensation_landing_before_a_rollback_is_settled_only_by_its_obligation` (`:6535`), `t_a_missing_session_behind_a_held_compensation_is_judged_by_a_later_slot_read` (`:6589`) |
| (F) | `f_a_fault_or_damage_under_a_held_compensation_keeps_the_doubt` (`:6665`) |
| (D) | `d1_a_slot_erased_under_an_unlanded_commit_is_never_committed` (`:6813`), `d1_torn_bytes_at_the_part_key_under_an_unlanded_commit_are_never_committed` (`:6876`), `d2_…` (`:6955`), `d3_…` (`:7020`) |
| (K) new read | `k_the_own_commit_proofs_part_read_keeps_the_coverage_of_the_reads_before_it` (`:7071`), `k_torn_bytes_at_the_proofs_part_read_keep_the_flag_through_a_later_fault` (`:7154`) |

Also: the h4 compensation leg reworked and renamed `h4_a_compensation_answered_unknown_sends_no_further_batch`
(`:2774`); the (K) trace vocabulary (`gap_chain`, `at_gaps`, `proof_lap` `:4369`, `laps_ending_gone`, `at_proof_gaps`
`:5952`) and the 4 exact traces it updated; `compensating` delegating to `compensating_part`; `Rig::reclaimed` and
`Fleet::forget`, so (T)'s terminal-delete runs can still end with the C-1 check.

Every new leg asserts no batch after the one answered unknown (`assert_nothing_after`) and ends with
`assert_every_fragment_named`, directly or through `assert_compensated` / `assert_finally_compensated`. The one
exception is the changed `r_damage_…` block, which (like the base block) leaves torn bytes at the part key and never
ran the C-1 check.

### Base vs fix (whole test file, 92 tests)

| `commit.rs` | Result |
|---|---|
| fixed (this patch) | 92 passed, 0 failed |
| fixed, with the sign-off's mutant (`obligation.is_none() && laps == 1`) | 91 passed, 1 failed: `s_a_compensation_answered_unknown_is_settled_from_reads` (the new row) |
| base (`db1a066`) | 71 passed, 21 failed: `s_…`, `x3_…`, the three new `o_…`, the three new `t_…`, `f_a_fault_or_damage…`, both `d1_…`, `d2_…`, both `k_…`, `r_damage…`, the reworked h4 compensation leg, and five existing tests whose exact traces (K) extended (`h4_a_commit…`, `b_a_landed…`, `b_read_gap_landed_then_a_rival…`, `b_read_gap_landed_then_a_rollback…`, `r_a_missing_session_behind_a_held_commit…`) |

Differences from the brief's "green on the base" list (unchanged from iteration 3): `d2_…` and both `k_…` tests are red
on the base only on their read-trace assertions (they pin the new read); their binding reds are the targeted edits
below. `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` ((T)'s first history) is red on the base:
the base keeps the store's `may_still_commit: true` with `OutcomeUnknown`, while the brief's expected answer is `false` /
`SessionMoved`.

### Demonstrated reds (required)

E1-E15 were run on iteration 3's final tree, each a temporary edit to the fixed `commit.rs` by a scratch script (write,
run `cargo test -p wyrd-core --test multipart_upload_part_commit -- <named legs>` under `timeout`, restore, `cmp`).
This iteration changes no production logic (comments and doc text only) and adds one (S) row, so those results stand.
E16 is new and was run on this iteration's tree.

| Temporary edit | Named leg(s) | Result |
|---|---|---|
| E1 a compensation answered unknown sent again (attempt 7's resend) | (X3), (S) one-batch | both red |
| E2 a slot held at the bytes it required taken as "did not land" | (X3) empty run, (S) row 2 | both red |
| E3 the slot-gone proof accepted for a non-empty compensation | D3 | red: `Ok(Refused(OperationAborted))` |
| E4 the obligation read keyed under the commit's epoch | (O) rollback leg | red |
| E5 no obligation read after the read that found the session moved | (T) second history, non-empty | red |
| E6 the session check after the slot read made always true | (T) first history | red |
| E7 the flag lowered on other bytes at the obligation key | (O) held run | red: "held: the flag" |
| E8 the flag lowered on a missing session record with no slot read after it | (T) erased-session runs | red |
| E9 that slot read skipped, the flag kept | (T) terminal-delete runs | red |
| E10 each doubt-keeping return made a plain stop (fault, slot damage, session damage) | (F) | each red alone |
| E11 the part-key check removed | D1 | red: `Ok(Committed { .. })` |
| E12 the part-key check made on the lap's first part read | D2, `b_read_gap_landed_then_a_rival_is_committed` | both red at gaps 1 and 2 |
| E13 torn bytes at the proof read lower the flag | `k_torn_…`, `d1_torn_…` | both red |
| E14 torn bytes at the proof read taken as a record | `k_torn_…`, `d1_torn_…`, `r_damage…` | all three red |
| E15 torn bytes at the proof read disprove nothing | `d1_torn_…`, `r_damage…` | both red |
| **E16** (new) the empty part's proof only in lap 1 (`obligation.is_none() && laps == 1`, the sign-off's mutant) | (S) "held, released between the laps", empty part | red: `Err(InDoubt { Compensation, true, OutcomeUnknown })` for `Refused(OperationAborted)` |

### Refute-your-own-test answers

- **(a) Genuine red?** Yes. With the base `commit.rs`, 21 tests fail, D1 among them on `Ok(Committed)` and every
  compensation leg on the unread `InDoubt`. The new (S) row goes red on the sign-off's mutant (E16), which survived all
  92 tests in iteration 3. Every brief-named edit goes red on its named leg (table above).
- **(b) Production path?** Yes. Every leg calls the production `commit_part` (via `Rig::commit` or `upload_part`) over a
  real `RedbMetadataStore`. The wrapper only scripts store answers (unknown, held, read faults) and runs real writers
  (Abort, `commit_part` rivals, `stage_part` staging rounds released from hold, plants and erasures on the inner store).
  The new row's held compensation is the production batch, landed on the inner store by `land_held` at the exact gap.
- **(c) Fixture includes the fault?** Yes. In the new row the compensation really is held and really lands between the
  laps, so the second lap's slot read finds the slot the compensation deleted (the obligation, for a part with chunks).
  For the documented limit, the throwaway reproduction really erased the slot and really landed the rival's commit.

## C5 survivors

- Iteration 3's gate: 24 mutants, 14 caught, 7 unviable, 3 timeouts, 0 missed. The sign-off found one more by hand
  (E16); it is now killed. The 3 timeouts are caught mutants (they defeat the settle loop's termination), as the
  sign-off recorded.
- I did not run `cargo mutants` itself; the Check gate will. This iteration adds no production logic, so the gate's
  mutant set should be the same as iteration 3's.
- What may still survive, unchanged: `impl Display for Stop` returning `Ok(())` without writing. `in_doubt` / `stopped`
  only check that the error text *contains* `cause.to_string()`, which an empty string always satisfies. A pre-existing
  pattern from #1027-#1029.

## Gates and commit-readiness

- `cargo fmt --all -- --check`: clean on the final tree. The repo has no pre-commit config of its own; its hooks are the
  `cargo xtask ci` steps.
- How I ran the red→green: the project has no single-test entry point in `cargo xtask`, so the quick checks ran the
  brief's named suite (`cargo test -p wyrd-core --test multipart_upload_part_commit`) under `timeout`, and the full gate
  ran `cargo xtask ci` through `engine/xtask.sh` with `PDCA_WORKTREE` set.

## Out of scope, not done

Per the brief: no resend of a compensation, no "did not land" proof, no change to staging, record shapes, the drain,
the reaper, Complete or DST (`// deferred: #660` marker in the test file's #1030 helpers section, test file `:5933`), no
production caller. No external dependency was missing: `typos` and the docs renderer are installed and ran inside
`cargo xtask ci`.

## `cargo xtask ci` result (final tree)

`./engine/xtask.sh ci` with `PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l1` (so `cargo xtask ci` in the worktree):
**exit 0, "xtask ci: all checks passed"**, 228 `test result: ok` lines, 0 `FAILED`; this file 92 passed, 0 failed.
Steps seen in the log: `typos`, `lint_docs` (OK), `render_site --check` (101 pages, link audit OK), the gitlink, unsafe and
blackbox guards, `cargo fmt --all -- --check`, workspace `clippy --all-targets`, build, `cargo test --workspace --exclude
wyrd-dst`, `cargo-machete`, `cargo fetch --locked`, the three `cargo deny` checks, `statics`, `deploy-guard`, and the
`wyrd-dst` clippy and tests under `--cfg madsim`.

I ran it twice. The first run passed too, but I edited doc text while it ran, so I ran it again on the final tree.
Before and after the second run I hashed `git diff HEAD`: the tree did not change during the run, and `patch.diff` was
written from that same tree afterwards (same hash; `git apply --check -R` clean against the worktree). The bundle's
`multipart_upload_part_commit.rs` is byte-identical to the worktree's.

Scratch: `$PDCA_SCRATCH/pdca-builder-1030-scratch` (860 KB: the two CI logs, the saved copies used to restore files after
each temporary edit, the tree hash). No build cache there; left for the harness to reclaim with `$PDCA_SCRATCH`.
