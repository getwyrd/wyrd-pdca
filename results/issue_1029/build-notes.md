# Build notes — #1029 (657.2c), iteration 4: a read fault keeps what earlier reads proved

Target: the stack base `pdca-integration/main-r9dd123c9ed70` at `1f6d6a4` (the worktree's `HEAD`),
which folds in #1027 and #1028. `patch.diff` is `git diff HEAD` on that tree. All `path:line`
citations are on the patched tree unless marked "v3" (iteration 3's patch) or "base".

## What the carry-forward asked, and what this iteration changes

The human's sign-off on iteration 3: **fix R1 only, keep everything else as is.** R1 (all three T4
findings, `review-batch.md`, are the same defect): `settle` read its four reads through `read_lap`
(v3 `commit.rs:848-863`), which returned `Err` on the first failing read and dropped the reads that
had already returned. So when the part read found a rival's record where the commit required none,
or the first session read found the session at a higher epoch, a fault on a later read in the same
lap answered `InDoubt { may_still_commit: true }` although an earlier read had proved the commit
can no longer land.

**The fix (`crates/core/src/multipart/commit.rs`).** The carry-forward offered two shapes: fold each
read into `Doubt` as it returns, or return the partial reads with the error. I took the first.

- `Doubt::read` (`commit.rs:832-844`), new: one read of a lap; a fault ends the reads with
  `doubt.end(upload_id, Stop::Store(fault))`, so it reports `may_land` as the reads before it left it.
- `settle` (`commit.rs:876-952`) now makes the lap's four reads one at a time, in the same order as
  before, and folds each before the next read starts:
  - the part read (`commit.rs:890-896`), then its fold, including the required-absent rival
    (`commit.rs:904`);
  - the first session read (`commit.rs:911`), then its fold (`commit.rs:912`, the line the
    carry-forward pinned as v3 `commit.rs:922`);
  - the slot read (`commit.rs:913-914`);
  - the last session read (`commit.rs:915-917`).
  Everything from the own-commit proof down (`commit.rs:918-951`) is iteration 3's code unchanged,
  except `lap.part.is_none()` became `part.is_none()` (`commit.rs:941`).
- `Lap` and `read_lap` are gone. Once `settle` stopped using them, `Lap::first` was never read and
  the build failed (`-D dead-code`). Rather than keep a struct whose only caller ignores a field, I
  put `lost_commit` back to the stack base's inline reads (base `commit.rs:966-977`). Its only
  remaining change from base is the comment that replaces the base's `// deferred: #1029` marker
  (`commit.rs:1257-1258`). This makes the patch smaller, not larger: `commit.rs` is 44 lines added,
  50 removed against v3, and 657.2b's `Conflict` path is now byte-identical to base apart from that
  comment.
- Module doc, *A commit in doubt* (`commit.rs:161-162`): one sentence saying each read is weighed as
  it returns, so a later failing read keeps what earlier reads showed.

`docs/design/architecture/05-building-block-view.md:250` is unchanged from v3. It already says the
error "says whether the commit may still land and names what ended the reads", which the fix makes
true in more cases; no port, API operation, RPC, CLI flag or persisted field changed.

Nothing else from iteration 3 changed: the proof, the read order, `SETTLE_LAPS = 2`, the damage
arms, `gone_in_doubt`, `InDoubt::cause`, `SessionMoved`, `OutcomeUnknown`, no batch after an
unknown outcome, and every iteration 3 test.

## The new tests (carry-forward legs (1) and (2))

`crates/core/tests/multipart_upload_part_commit.rs`, next to
`c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (`:5147`):

- `enum Ended` (`:5216`) and the shared body `read_fault_after_the_end` (`:5231`).
- **(1)** `c_a_read_fault_after_the_session_moved_keeps_the_commit_unable_to_land` (`:5298`): a held
  commit; at gap 1 (right after the part read) a rollback moves the session `Open@E` → `Open@E+2`
  and the slot read is armed to fault. The first session read sees the higher epoch, then the slot
  read faults.
- **(2)** `c_a_read_fault_after_a_rival_where_none_was_required_keeps_the_commit_unable_to_land`
  (`:5305`): a held first upload of part 1 (so the commit requires the part key absent); at gap 0 a
  same-number rival commits for real (`Rig::rival`, production `commit_part` on the inner store) and
  the session key is armed to fault. The part read sees the rival's record, then the first session
  read faults.

Each asserts: `InDoubt`, batch `Commit`, `may_still_commit: false` (`:5258-5261`), cause the
scripted read fault, exactly the reads before the fault were made, one commit batch, no
compensation, the slot still held; then the held batch, released, answers `Conflict` (`:5275-5279`)
and nothing of the attempt was released. (1) also checks the session really sits at `E+2` and no
part exists; (2) that the part is the rival's. I split the two legs into two `#[tokio::test]`
functions so each red names its leg (the same reason iteration 3 split the read-gap leg).

Total: 77 legs, all green.

## Demonstrated reds

Each row is a temporary edit of the final `commit.rs`, run against the final test file by
`red.sh` (scratch), which puts the final file back after each run; `cmp` against the saved copy
confirmed the restore. Variants are exact one-match string swaps (`variants.py`, which aborts if a
swap does not match exactly once).

| Edit | Named leg(s) | Result |
|---|---|---|
| **RA**: iteration 3's whole `commit.rs` (rebuilt from `iteration-v3/patch.diff`) — the R1 defect | both new legs | **2 red, exactly the two new legs**, each at the `may_still_commit` check (`:5258`: "a read before the fault showed it cannot land"); the other 75 green. |
| **RB**: the first-session-read fold removed (`commit.rs:912` → `let _ = judge_session(..)`) — the carry-forward's pin | leg (1) | **1 red: `c_a_read_fault_after_the_session_moved_…`**, nothing else. Without leg (1) this line was unpinned: the last session read made the same fold whenever no fault came between. |
| **RC**: the part-read fold removed (`commit.rs:904` → `{}`) | leg (2) | 5 red: `c_a_read_fault_after_a_rival_where_none_was_required_…`, `b_a_same_number_rival_…`, `b_read_gap_not_landed_then_a_rival_…`, `c1_…`, `x_…`. |
| **M13a**: the doubt dropped at the read-fault site (`Doubt::read` maps a fault to `stopped(..)`) | read-fault legs | 4 red: both new legs, `c_a_read_fault_at_each_read_…`, `c_a_conflict_then_a_held_round_…`. |
| **M5**: the proof takes the session from the read made before the slot read | C2 | 2 red: `c2_a_torn_read_never_answers_committed`, `b_read_gap_landed_then_a_rollback_…`. |
| **M8**: the own-commit proof removed | (B) landed, C3, one-batch | 8 red, incl. `b_a_landed_…`, `c3_…`, `h4_a_commit_answered_unknown_…`. |
| **M3**: a part present under an unchanged session taken as `Committed` | (B) re-upload | 12 red, incl. `b_an_unlanded_reupload_…`, `c1_…`, both `x2_…`. |
| **M7**: a rival record where absent was required checked before the proof | C3 | 9 red, incl. `c3_…`. |
| **M10**: any changed prior taken as "cannot land" | (B) changed-prior legs | 5 red: the three changed-prior legs and two (R) legs. |
| **M12a**: the doubt dropped on the torn-part damage arm | (R) | 1 red: `r_damage_under_a_held_commit_keeps_the_doubt`. |
| **M14**: `SETTLE_LAPS = 1` | (binds iteration 3's decision 1) | 7 red, incl. `b_read_gap_landed_then_a_rival_is_committed`. |
| **M0**: in doubt at once with no reads (the stack base's behaviour under the new API) | "the fix reverted" | 26 red, incl. both new legs, (B) landed, C3, (X), one-batch. |

**Carried forward from iteration 3, not re-run:** V2/M1/M2 (iteration 2's settle and its resend:
(X), (X2) both runs, the rival leg, read-gap not-landed, re-upload, one-batch), M4 (C1), M6 (C3′),
M9 ((H)), M11a/b ((F)), M12b-f (the other damage arms), M13b (the slot re-read behind a missing
session). The code each mutates is unchanged in this iteration: the commit loop, `judge_slot`,
the lease check, `gone_in_doubt` and the damage arms below the proof (`commit.rs:918-951`) are
iteration 3's lines, and the test legs that caught them are unchanged. The rows are in
`iteration-v3/build-notes.md` §"Demonstrated reds".

## Refuting my own test (forced)

- **(a) Genuine red? Yes.** With the fix reverted, i.e. iteration 3's `commit.rs` put back (RA),
  both new legs fail and only they fail, each at `may_still_commit`. Removing just the pinned fold
  (RB) fails leg (1) alone; removing just the part fold (RC) fails leg (2) among others.
- **(b) Production path? Yes.** Both legs call production `commit_part` (`Rig::commit`, test
  `:1082-1085`) over a real `RedbMetadataStore` through the scripted wrapper, which only holds the
  commit batch and fails reads of one key. The rival is a real `commit_part` on the inner store
  (`Rig::rival`, `:1145-1157`); the rollback is 657.2b's test-local re-encode of the session at
  `E+2` under `require` on the bytes read (`roll_back`, `:1412-1438`).
- **(c) Fixture includes the fault? Yes.** The commit really is held unapplied, the session really
  moved (asserted at `E+2`) or the rival's record really sits at the part key (asserted), and the
  read really faults (the cause is the wrapper's "scripted read fault" and the logged reads stop
  just before it). The held batch is then really released and the store answers `Conflict`, so the
  `false` the attempt reported is the true answer, not just a different one.

## Gates and tools run

- `cargo test -p wyrd-core --test multipart_upload_part_commit` under `timeout 1500`: 77 passed in
  0.6 s. As in iteration 3, the project has no single-test wrapper besides
  `engine/scripts/run-verify.sh` (which needs the Check gate's own checkout), so the red/green loop
  used this bounded command; every leg also runs under its own 60 s `within` bound.
- `cargo fmt --all`: applied (it re-wrapped the first `doubt.read(..)` call).
- `./engine/xtask.sh ci` (the project's gate, `cargo xtask ci`) on the final tree, under
  `timeout 3000`: "xtask ci: all checks passed", exit 0. It ran `typos` (installed, `typos-cli
  1.48.0`, not skipped), `lint_docs.py`, `render_site.py --check` (the `docs-renderer` modules
  import), `cargo fmt --all -- --check`, clippy over the workspace, the build, every workspace test
  and `cargo deny`. Both external dependencies the brief lists were present, so this is full CI
  parity for the prose gates.

## C5 (advisory) survivors

**None.** I ran the project's own C5 script, `scripts/mutants-in-diff` (`cargo mutants --in-diff
patch.diff --no-shuffle --cap-lints=true`), on this bundle's final `patch.diff` in the worktree:
**32 mutants, 18 caught, 11 unviable, 3 timeouts, 0 missed** (exit 3, cargo-mutants' code for "some
timed out"). Against iteration 3 (31 / 16 / 12 / 3 / 0): `Lap`/`read_lap` went (one unviable fewer)
and `Doubt::read` came in with two mutants, both caught.

- **Caught (18):** the `Display` and `source` bodies; `Doubt::cannot_land` and
  `Doubt::seen_session` emptied (`commit.rs:807`, `:815`); **`Doubt::read` replaced with `Ok(None)`
  and with `Ok(Some(Default::default()))` (`commit.rs:840`)**, the new function; the lap counter
  decremented (`:887`); the part-key `!=` (`:900`); the required-absent guard forced `true` and
  `false` (`:904`); the `PartRecordMissing` guard forced both ways and its `&&` (`:941`); the lap
  guard forced `false` and its `<` turned into `==`, `>` or `<=` (`:947`).
- **Timeouts (3), detected, not survivors:** `commit.rs:887:14` (`laps += 1` → `*=`, so `laps`
  stays 0), `commit.rs:947:39` (the lap guard → `true`) and `commit.rs:947:54` (`&&` → `||`). Each
  makes `settle` read laps without bound while a held batch never lands, so the held-commit legs
  hang until cargo-mutants' 20 s timeout. The same three as iteration 3, at their new lines.
- **Unviable (11):** each replaces a return value with `Default::default()` (or a leaked
  `Box::new(Default::default())`) for a type that has no `Default` impl: `Stop`, `CommitError`,
  `Doubt`, `UploadPartOutcome`, `Compensated`, `Lost`, `&dyn Error` (`commit.rs:565`, `:707`,
  `:714`, `:773`, `:798`, `:823`, `:858`, `:883`, `:1129`, `:1249`, `:1312`). They do not compile,
  so they say nothing about the tests.

`mutants.out/` is left in the worktree (git-ignored by the target's `.gitignore`), as the gate
itself would leave it.

## Scratch

Throwaway files are under `$PDCA_SCRATCH/pdca-builder-1029-reds/`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1029/`): `red.sh`, `variants.py`, the saved final and
iteration 3 `commit.rs`, every variant and its log, the gate log (`ci.log`) and the mutants log
(`mutants.log`). The gate's docs render also left `wyrd-docs-build-51010` in `$PDCA_SCRATCH`. Not
deleted: the builder role's filesystem rule says cleanup is the harness's, and no cargo build
directory lives there.

## Rubric self-review (the target's `## Review rubric & protocol`)

Checked against the change since iteration 3. It adds no clock read, no new seam or dependency
edge, no crate, and no global state. Validation boundaries are unchanged: damage is still judged
after the proof, as before. Docs currency: no port, API operation, RPC, CLI flag or persisted field
changed, and the module doc now states the fold order (`commit.rs:161-162`). Await discipline: each
read is one `MetadataStore::get`, bounded by the store contract as before, with no new spawn and no
new loop. Test fidelity: the new legs drive production `commit_part` over the real store. Nothing
found to fix.

## For the reviewer and the human

- R1 only, as asked. The fix is ordering: each read's fold now happens before the next read can
  fail. `Lap`/`read_lap` went because the fix made them dead code; `lost_commit` is back to base.
- C4-diff-cov will still say the patch does not apply on `origin/main` until #1028 merges
  (expected, per the brief). C4-verify will report green-only (the suite is in an existing file).
- No push, no PR.
