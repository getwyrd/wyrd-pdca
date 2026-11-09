# Build notes — #1028 (657.2b), iteration 2: a part commit that lost is classified and compensated

Base: the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`, HEAD `60e5d2c` (the stack base
`pdca-integration/main-rdfeb36496b2d`, which folds in 657.2a, #1027). `patch.diff` is
`git diff 60e5d2c` of the final tree: iteration 1's patch (`iteration-v1/patch.diff`, applied
unchanged as the starting point) plus this iteration's changes. Every `path:line` below is on the
patched tree.

## What this iteration changed, and why

The sign-off carry-forward asked for three things. All three are done, plus two small additions.

### (1) (O) for an empty re-upload over a non-empty prior — production fix

The defect (found by the adversary and the code review): a re-upload over a non-empty prior puts
`require_absent` on its obligation key in every commit round (`commit.rs:782-784`), but no read of
a round looks at that key. With other bytes there, every round is "a conflict no read explains".
After four rounds the attempt compensates. For an attempt **with** chunks, the compensation
requires the same key absent, conflicts, and its re-read stops `ObligationKeyTaken`. For an
**empty** attempt, `obligation_for` returns `None` (`commit.rs:752-754`), so the compensation does
not touch that key, lands, and the answer was `Stopped { CommitRoundsSpent, slot_released: true }`:
the damage went unreported.

Fix:

- `obligation_key_free` (`commit.rs:604-622`): read an obligation key a batch of this attempt
  requires absent; any record is `Stop::ObligationKeyTaken`, a fault is `Stop::Store`. The
  compensation's own re-read now calls it too (`commit.rs:863-865`), replacing the inline check
  iteration 1 had there, so both paths share one function.
- `unexplained_stop` (`commit.rs:926-941`): when the commit rounds end after a round no read
  explained, read the commit's obligation key (only when `retire` is `Some`, i.e. a re-upload
  over a non-empty prior) before compensating. A record there stops with no compensation.
- It is called on **both** paths that end such rounds (`commit.rs:1075-1083`):
  - the rounds-spent path (`rounds == COMMIT_ROUNDS`), which sign-off named;
  - the later-round lease-short path. Sign-off named only the rounds-spent path (`:1028/:820` in
    iteration 1's numbering), but the lease-short path is the same silent skip by another route:
    with only that path reverted, the empty re-upload ends `LeaseShort { .. }, slot_released:
    true` with the planted bytes unreported (red N2-empty below). The first-round lease check
    (`commit.rs:1039-1042`) is untouched: no commit has been sent there, so nothing has
    conflicted on the key.
- Docs: the module doc's *A commit that lost* gains a paragraph (`commit.rs:72-78`); *The stops*
  gains a row (`commit.rs:144`); *Bounds* restates the read count, still at most five per commit
  round, since the extra slot read behind a missing session and the obligation-key read are on
  different branches (`commit.rs:169-173`); `COMMIT_ROUNDS` (`commit.rs:238-239`),
  `Stop::ObligationKeyTaken` (`commit.rs:349-353`), `commit_rounds` and `commit_part`'s docs
  (`commit.rs:1105-1106`); the living architecture doc's sentence on unexplained conflicts
  (`docs/design/architecture/05-building-block-view.md:250`, docs-currency rule).

A behaviour change for the attempt **with** chunks, for the better: it used to send one
compensation that conflicted on the key and then stopped `ObligationKeyTaken`; now it stops
`ObligationKeyTaken` without sending that compensation. Same answer, one batch fewer, and "no
compensation for damage" now holds for it too.

Test: `o_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part`
(`crates/core/tests/multipart_upload_part_commit.rs:3607`) now runs four legs: {with chunks,
empty} × {rounds spent, lease short after round 2}. Each asserts `ObligationKeyTaken` naming the
key, the planted bytes and the prior kept, exactly 4 (or 2) commits, **no** compensation, the read
order after the last commit (part, session, slot, session, then the obligation key), the slot
still held, and no release.

### (2) Fenced-session legs for the slot test — kills C5's MISSED mutant

`r_a_slot_lost_or_rewritten_at_a_re_read_stops_without_a_release` (test file `:3524`) gains a
session dimension (`:3525`): live, and fenced by a real `abort_session` just before the slot is
changed. Expected answers (`:3565-3571`):

| how | live | fenced |
|---|---|---|
| gone (absent) | `SlotLost` | `Refused(NoSuchUpload)` |
| foreign (another attempt's id) | `SlotLost` | `Refused(NoSuchUpload)` |
| rewritten (this attempt's id) | `SlotLost` | `SlotLost` |
| torn | `Stop::Record` | `Stop::Record` |

Each at the commit's re-read and at the compensation's: 16 legs. Every leg also asserts one
commit, no further batch after the re-read (0 compensations at the commit's, 1 at the
compensation's), no obligation, no release, the slot key holding exactly what the step left, and
the session `Aborting` in the fenced legs. The Abort runs **before** the slot change so the Abort
never reads a torn slot. The `==` → `!=` mutant at `judge_slot` (`commit.rs:582`) swaps foreign
and rewritten, which only changes the answer under a fence; the fenced legs catch it (red N3),
and iteration 1's test file does not (N3-v1test, green).

### (3) The chase test's "most a session can see" claim reworded

Renamed `l_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` to
`l_the_epoch_chase_lands_within_its_bound_after_one_rollback_per_charged_complete` (test file
`:3758`). The doc (`:3752-3756`) now says three is the most rollbacks **charged** Complete fences
can make, not the most a session can see: a Complete rejected behind its fence gives its attempt
back (#1007, the caveat `COMPENSATION_ROUNDS`'s own doc states at `commit.rs:246-251`), and a
fourth rollback against a compensation spends the budget, which
`l_every_loop_ends_after_exactly_its_bound`'s chase leg shows. The file header's (L) line was
reworded the same way. No behaviour changed.

### Addition A: a per-attempt bound check in the test wrapper

`MOST_ROUNDS = 4` (test file `:107`, a literal, never the imported constant) and
`Scripted::check_rounds` (`:620`, called for every part commit and compensation at `:703`): the
wrapper fails a leg the moment any one attempt sends a fifth part commit or a fifth compensation.

Why: C5's two TIMEOUT mutants (`+=` → `*=` on the two round counters) left the counter at 0, so
the loop never ended. Iteration 1's tests only caught them at the 60 s `LEG_TIMEOUT`, after
cargo-mutants' own 20 s cap had already cut them (reported as TIMEOUT). With the check they fail
in 0.06 s ("…sent 5 batches of Commit, past the bound of 4"; reds R10a/R10b). It also pins the
bound on every leg, not only the (L) legs. Cost: about 20 lines in the test file. No existing leg sends
more than four of either per attempt (all 47 pass).

### Addition B: a `// deferred: #660` marker in the test file

The gating T4-batch-review blocker was filed against the test file (`:2950` in iteration 1),
asking for seeded Tier-0 DST coverage. Sign-off settled it as a deferral to #660, whose marker
lives in `commit.rs:201`. I added the same marker at the head of this slice's section of the test
file (`:2773-2775`), so a reviewer reading the test sees it there. I did **not** write
`review-rejected.md`: recording the rejection is the human's decision. If the batch review
raises it again, it still needs that line from the human.

## What I ruled out

- **Read the obligation key in every unexplained round** (inside `lost_commit`, as a fifth read
  after the four). It would stop a taken key after round 1 rather than round 4. Rejected because
  sign-off chose the rounds-spent path, and because it changes the four-read round order the brief
  pins for 657.2c ("Use that read order: 657.2c needs it") and adds one read to every retry round
  of every re-upload over a non-empty prior, where the common unexplained conflict is a TiKV lock,
  not damage. The code size is about the same either way (about 8 lines), so this is not a cost
  call: it is the read-order constraint plus sign-off's decision.
- **Make the empty compensation require the commit's obligation key absent.** That would turn the
  missed key into a compensation conflict whose re-read has no obligation key to look at
  (`obligation_for` is `None`), so it would loop as unexplained until `CompensationRoundsSpent`.
  It also adds a precondition to a batch whose contract (`commit.rs:11`) names none for an empty
  part. Rejected.
- **Add DST coverage.** Out of scope; sign-off: "Do NOT add DST".

## Demonstrated reds — one temporary edit at a time, each reverted

Script and logs: `$PDCA_SCRATCH/pdca-builder-1028-reds2/` (`reds.py`, one `<name>.log` per run,
`summary-new.txt`, `summary-brief.txt`). Each run applies one edit to `commit.rs` (or, where named,
to the test file), runs the named legs with `timeout 600 cargo test -p wyrd-core --test
multipart_upload_part_commit -- --exact …`, then restores both files from saved copies (checked
byte-for-byte with `cmp` after each batch). Every edit compiled; every red is a behaviour failure
with at least one test run.

This iteration's new legs:

| Edit | Leg | Result |
|---|---|---|
| N1 rounds-spent path calls `compensated_stop` again (no key read) | `o_…` | FAIL: read order lacks the obligation key, and a compensation is sent |
| N1-empty the same, the (O) test narrowed to the empty re-upload | `o_…` | FAIL: `all 4 part commits were answered Conflict …; the attempt's slot is released` (the reported bug) |
| N2 lease-short path calls `compensated_stop` again | `o_…` | FAIL: lease legs |
| N2-empty the same, narrowed to empty + lease | `o_…` | FAIL: `the lease … so no further commit was sent; … the attempt's slot is released` |
| N3 `judge_slot` `==` → `!=` (`commit.rs:582`) | `r_a_slot_lost_or_rewritten…` | FAIL: got `SlotLost` where the fenced foreign leg wants `NoSuchUpload` |
| N3-v1test N3 against iteration 1's test file | same | PASS: the gap the reviewers found, now closed |
| V1-src iteration 1's production `commit.rs` under this test file | `o_…`, `r_a_slot…` | `o_` FAIL, `r_a_slot` PASS (iteration 1's slot code was right; only its test was missing) |

The brief's list, re-run on the final tree:

| Edit | Leg(s) | Result |
|---|---|---|
| R1a slot read before the session read taken as final, commit's re-read | both `t_a_terminal_delete_between_…` | commit leg FAIL (`SessionRecordMissing`), compensation leg ok |
| R1b the same, compensation's re-read | same | compensation leg FAIL, commit leg ok — the two re-reads are bound separately |
| R2 a missing session always read as a terminal delete | `t_a_missing_session_…`, `t_another_attempts_slot_…` | both FAIL: `Ok(Refused(NoSuchUpload))` |
| R3 another attempt's slot counted as gone behind a missing session | same two | foreign leg FAIL, still-held leg ok |
| R4 compensating on any `Conflict` without the re-read | `b_a_fenced_session_…` | FAIL: "no compensation", left 1 |
| R5 the rolled-back `Open@E'` sent to the reaper | `b_a_rolled_back_…` | FAIL: `NoSuchUpload` |
| R6 live-session loser answering `NoSuchUpload` | rollback, rival, repair legs | FAIL ×3 |
| R7 a damaged part read as a rival | `r_a_damaged_part_record_…` | FAIL: `Ok(Refused(OperationAborted))` |
| R8 a non-`Open` record at an unrisen epoch read as a fence | `r_a_damaged_session_at_the_last_read_…` | FAIL: `Ok(Refused(NoSuchUpload))` |
| R9 `require_absent` removed from the commit's obligation key | `o_…` | FAIL: `Ok(Committed …)` |
| R10a commit-round counter `+=` → `*=` | `l_every_loop_ends_after_exactly_its_bound` | FAIL in 0.06 s: "sent 5 batches of Commit, past the bound of 4" |
| R10b compensation-round counter `+=` → `*=` | same | FAIL in 0.06 s: "sent 5 batches of Compensation …" |
| R11 `COMPENSATION_ROUNDS` lowered by one | the chase test, `l_every_loop_…` | both FAIL: `CompensationRoundsSpent { rounds: 3 }` |
| R12 lease check skipped on a later round | `l_a_lease_used_up_between_rounds_…`, `o_…` | both FAIL: the wrapper's margin guard ("sent with 3752 ms left") |

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. With this iteration's production change reverted (iteration 1's
  `commit.rs`, run V1-src) the new (O) legs fail; with only the rounds-spent or only the
  lease-short call reverted, the empty re-upload ends "slot released" (N1-empty, N2-empty). The
  slot-test legs fail under the `==` → `!=` mutant (N3), which iteration 1's test did not catch
  (N3-v1test). Against the 657.2a base the whole appended section fails, as iteration 1 showed
  and the reviewer reproduced (eight compiled reds, `iteration-v1/SUMMARY.md` §5).
- **(b) Production path?** Yes. Every leg calls the production `commit_part` through `Rig::commit`
  over a real `RedbMetadataStore`; attempts are staged by the real `stage_part`; the fence is the
  real `abort_session`; the prior part is committed by the real `commit_part`. The wrapper only
  logs, scripts answers and runs steps; the new `check_rounds` only counts.
- **(c) Fixture includes the fault?** Yes. The planted obligation bytes are a real record at the
  real key the commit requires absent, and the commit's conflicts in the (O) legs are the store's
  own answers to that precondition (no scripted `Conflict`). The lease legs move the real test
  clock through the reads. The fenced legs run a real Abort, then plant or delete at the real slot
  key; the read order and the stored slot bytes are asserted afterwards.

## Gates

- `cargo fmt --all -- --check`: clean.
- `cargo test -p wyrd-core --test multipart_upload_part_commit`: 47 passed (same count as
  iteration 1: legs were extended, none added or removed).
- `cargo clippy -p wyrd-core --all-targets`: clean.
- `cargo xtask ci` via `./engine/xtask.sh ci` on the final tree: **exit 0, `xtask ci: all checks passed`** (log
  `$PDCA_SCRATCH/pdca-builder-1028-reds2/ci.log`): typos, docs lint, docs render with link check
  (both declared external dependencies, `typos` and `docs-renderer`, ran and passed), the three
  guards, fmt, clippy, build, `cargo test --workspace --exclude wyrd-dst` (215 suites, 2,231
  passed, 0 failed), machete, the three `cargo deny` runs, the statics gate, deploy-guard, DST
  clippy and DST tests (101 passed). No hang this time (iteration 1 hit an intermittent hang in
  `custodian_day_one`, unrelated to this code).
- `patch.diff` applies cleanly to `60e5d2c` (`git apply --cached --check` on a scratch index).

## C5 (advisory) survivors

Not run as `cargo mutants` here (it shares the target dir with the gate). By hand: iteration 1's
MISSED mutant (`commit.rs:582` `==` → `!=`) is caught (N3), and its two TIMEOUT mutants are now
caught fast (R10a/R10b). Likely remaining survivors: the `Display` strings of the stops are matched
by phrase, not whole; and in `obligation_key_free` the `Err(fault)` arm (a store fault on the key
read) has no leg, as iteration 1 noted for every re-read fault (the wrapper's `fail_reads` could
script one, but no leg arms a fault mid-run).

## For sign-off

- The T4-batch-review DST finding, if raised again: settled deferral to #660 (markers at
  `commit.rs:201` and test file `:2773`). Recording it in `review-rejected.md` is yours.
- The size backstop flagged iteration 1 at 116 KB. `patch.diff` grows from 119,056 to 129,685
  bytes, mostly test legs and doc text; the production change is two small functions and two call
  sites. Sign-off said not to re-split.
