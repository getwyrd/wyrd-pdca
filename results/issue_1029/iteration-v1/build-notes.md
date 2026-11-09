# Build notes — #1029 (657.2c): a part commit answered `CommitUnknownResult` is settled or kept in doubt

Target: `getwyrd/wyrd` @ the stack base `pdca-integration/main-r9dd123c9ed70`
(`1f6d6a4`, which folds in #1027 and #1028). All `path:line` below are on the patched tree
(`$PDCA_WORKTREE`).

## What changed, and why

**The defect.** On the base, `commit_rounds` returned `CommitError::InDoubt` the moment a commit
round was answered unknown (`git show HEAD:crates/core/src/multipart/commit.rs`, the
`// deferred: #1029` arm in `commit_rounds`): no read, no retry. A round that landed was reported in
doubt, and the slot of a round that provably could not land stayed held.

**The change** — `crates/core/src/multipart/commit.rs`:

- `Doubt` / `Unsure` (`commit.rs:754-831`): every round answered unknown, with its own
  `may_land` flag. `cannot_land` (`:785`) marks every round unable to land (all rounds send the same
  batch, so one fact speaks for all); `session_seen` (`:796`) applies a session read (only a record at
  a higher epoch, `Open` or not, counts — damage and a missing record settle nothing); `slot_held`
  (`:808`) drops exactly the rounds that could no longer land **before** that slot read; `end`
  (`:814`) reports the first round that may still land, else the latest (Abort's rule,
  `abort.rs:394-448`).
- `judge_round` (`commit.rs:1167-1262`) replaces `lost_commit`. Same four reads in the same order
  (part, session, slot, session). The part and first-session facts are applied **before**
  `slot_held` (`:1204`), the last-session fact **after** it (`:1213`), which is what makes "a slot read
  made after the change" the only thing that settles. Then, in order: the own-commit proof
  (`:1217`, slot gone + last session read `Same` + a round in doubt → `Landed`), then damage, then the
  #1028 classification, with three doubt-only arms: slot gone under a moved session →
  `InDoubt(SessionMoved)` (`:1230-1234`); slot held under a session first seen moved by the last read →
  `ReadAgain` (`:1237-1239`, the peer is `stage.rs:1713-1732`); slot held, session `Same` → `Again`
  (`:1254-1255`).
- `commit_rounds` (`commit.rs:1271-1355`): keeps the `Doubt`, sends the same batch again while a
  round may land, does at most one read-only lap per round (`:1332-1338`; a second `ReadAgain` is
  unreachable — see the doc at `:226-228` — and ends in doubt rather than looping), and ends through
  `rounds_end` (`:1128-1140`): doubt kept → `InDoubt` with the stop as its cause, no compensation;
  no doubt → the #1028 `unexplained_stop` path unchanged.
- `session_gone` (`commit.rs:722-743`) takes the `Doubt`: slot key absent behind a missing session is
  `NoSuchUpload` with nothing in doubt, `InDoubt(SessionMoved)` (no round can land) with a round in
  doubt; any record there is `SessionRecordMissing`, keeping the doubt. The compensation passes an
  empty `Doubt` (`:1052`), so its behaviour is unchanged.
- Public API: `InDoubt` gains `cause: Stop` (`:391`), as Abort's, Stage's and Create's `InDoubt`
  carry; `may_still_commit` is now "the store said so **and** no read showed it unable to land"
  (`:380-385`), never raised above the store's report. `Stop` gains `SessionMoved` (`:438`) and
  `OutcomeUnknown` (`:441`, the cause the compensation's unchanged immediate stop now names,
  `failed`, `:836`). `CommitRoundsSpent`'s doc and message (`:397-402`, Display) no longer claim every
  round was answered `Conflict`, which is false once rounds can be answered unknown (the one
  existing assertion on that text, `multipart_upload_part_commit.rs:3684-3688`, follows it).
  `CommitError::InDoubt`'s Display appends the cause; `source()` stays the store's report so
  `classify` reads `Indeterminate`.
- Docs: the module doc gains *A commit in doubt* (`commit.rs:114-164`), the stops row and the bounds
  (`:203-204`, `:220-230`) are updated; `docs/design/architecture/05-building-block-view.md:250`
  replaces "settling it is still to land" with what settling now does (rubric: docs currency).

**Design source.** The brief's *Design* section and attempt 7's `Doubt` / module doc
(`results/issue_927/iteration-v7/patch.diff:340-411`, `:799-882`, `:1210-1411`), re-fitted onto
#1028's `lost_commit` shape rather than v7's single-function loop. Deliberately **not** taken from
v7: its rule that a changed non-empty prior settles (corrected at iteration 4 — here a changed prior
settles nothing, `commit.rs:1188-1190`), its compensation settling (#1030's scope), and its
`OwedChunks` / `max_staged_chunks` plumbing (not in this base).

## Test file — `crates/core/tests/multipart_upload_part_commit.rs`

Appended (`:3849-5490`): helpers (`held_count`, `Act::after_all`, `Rig::land_held`,
`fail_reads_from_now`, `at_gaps`, `teardown`, `read_slots`, `idle_fence`, `reserve_freed`,
`state_of`, `part_bytes`, `put_at`, `repoint`, `session_damage`, `assert_record_stop`,
`required_slot`, `lap`) and 22 legs, mapped to the success criterion:

| Criterion | Leg |
|---|---|
| (B) landed, answered unknown → `Committed`, released once | `b_a_landed_commit_answered_unknown_is_committed` (`:4160`) |
| (B) unlanded re-upload never `Committed` on the prior, ends own part | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` (`:4190`) |
| (B) held batch, runs (i)(ii)(iii) | `b_a_held_commit_is_never_compensated_and_at_most_one_lands` (`:4225`) |
| (B) rival, key required absent, once per flag | `b_a_rival_after_an_unapplied_unknown_commit_is_compensated` (`:4295`) |
| (B) changed prior settles nothing | `b_a_held_reupload_over_a_non_empty_prior_that_loses_to_a_rival_keeps_the_doubt` (`:4344`), `b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back` (`:4408`, real re-place shape, 3 restore modes × 5 gaps), `b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back` (`:4515`) |
| (B) read-gap leg, 4 histories × 5 gaps | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` (`:4582`) — not-landed runs pin the exact reads before the compensation |
| C1 | `c1_an_empty_part_is_never_committed_on_its_bytes` (`:4683`) |
| C2 | `c2_a_torn_read_never_answers_committed` (`:4761`) |
| C3, C3′ | `c3_a_landed_commit_then_a_rival_is_committed` (`:4799`) |
| every round out of flight → `Stopped(CommitRoundsSpent)`, released | `c_every_commit_round_out_of_flight_compensates_and_reports_the_release` (`:4841`) |
| the kept doubt | `c_the_kept_doubt_is_the_round_that_may_still_land` (`:4867`) |
| slot rewritten under own id → `InDoubt(SlotLost)` | `c_a_slot_rewritten_under_the_attempts_own_id_during_a_held_commit_is_slot_lost` (`:4893`) |
| slot gone under a moved session | `c_a_slot_gone_under_a_moved_session_with_a_commit_in_doubt_is_in_doubt` (`:4923`) |
| (F) short lease | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` (`:4966`) |
| (F) delayed (i)–(iv), idle-fence shape | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` (`:5003`) |
| (H), landed and not-landed | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` (`:5074`) |
| (R) | `r_damage_under_a_held_commit_keeps_the_doubt` (`:5133`, 24 cases — four record kinds, then five kinds of session damage × gaps 0–3 — plus own-commit-first and prior gone), `r_a_missing_session_behind_a_held_commit_is_in_doubt` (`:5287`), `r_a_session_damaged_and_repaired_mid_round_settles_nothing` (`:5332`) |
| wrapper guard after an unseen landing | `h3_a_round_sent_again_requires_the_bytes_its_own_landed_round_released` (`:5463`) |

**Existing code I had to change** (the brief says "appended"; these are the minimum the new
behaviour forces):

1. The two H4 commit legs that pinned the old stop-at-once behaviour
   (`h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch`,
   `h4_a_held_commit_that_lands_later_is_released_once`) are removed (`:2616-2619`); they assert the
   defect. Their histories are covered by the (B) legs above.
2. `h4_a_composed_error_keeps_its_class_and_its_message` (`:2685`) now holds the commit **and**
   conflicts every later round, since a held round alone is now retried and lands.
3. `in_doubt` (`:1494-1527`) checks the cause is named and that the flag is never above the store's
   report (it used to require equality; a read can now lower it).
4. The wrapper's slot guard (`:449`, `:548-555`, `:595-604`, `:628-641`): it dropped an attempt's slot
   from its record once that attempt's own batch released it, so it rejected the retry the design
   requires after a held round lands unseen (the retry requires the bytes that round released). It
   now remembers released bytes and accepts exactly those; H3's rejections are unchanged (the H3 test
   still passes) and a new H3 leg pins both sides.
5. The `CommitRoundsSpent` message assertion in `l_every_loop_ends_after_exactly_its_bound`
   (`:3684-3688`), for the reason above.

## Red → green

- **Post-fix:** `cargo test -p wyrd-core --test multipart_upload_part_commit` → 67 passed.
- **Pre-fix, base `commit.rs`** (`git show HEAD:…` swapped in, then restored and `cmp`-checked):
  the test file does not build — 34 errors, 30 × `no field cause on InDoubt` and
  4 × `no variant SessionMoved`. The new API is part of the fix, so the behavioural red is M0 below.
  As the brief pre-declares, C4-verify will therefore report `PASS (green-only)`.
- **Demonstrated reds** (the brief's list; each a temporary edit to `commit.rs`, applied by a script
  that asserted each edit target occurs exactly once, ran the named leg with `--exact`, and restored
  the fixed file, `cmp`-verified):

| # | Temporary edit | Named leg | Result |
|---|---|---|---|
| M0 | 657.2b's behaviour in the new API: stop in doubt on the unknown outcome, no read | whole file | **21 of 67 fail**: all 21 new legs that inject an unknown outcome. The 45 kept #1027/#1028 legs pass, and so does the new wrapper-guard leg `h3_a_round_sent_again_…`, which injects none |
| M1 | "a part is there" → `Landed` before the slot read | `b_an_unlanded_reupload_…` | FAIL: 1 commit sent, 2 expected |
| M2 | part bytes equal own record → `Landed` | `c1_…` | FAIL: "a round of its own landed", releases 0 |
| M3 | own-commit proof judged on the first session read | `c2_…` | FAIL: `Ok(Committed)` answered |
| M4 | `cannot_land` clears the doubt | `c3_…` | FAIL: `Stopped(SlotLost)` for a landed commit |
| M5 | another attempt's slot read as `Rewritten` (attempt_id ignored) | `c3_…` (C3′ run) | FAIL: `InDoubt(SlotLost)` |
| M6 | rival branch before the own-commit proof | `c3_…` | FAIL: compensated, `SlotLost` |
| M7 | compensate under the newer epoch while a round is held | `h_a_commit_held_…` | FAIL: `Stopped(SlotLost)` after a compensation |
| M8 | a changed non-empty prior settles | both changed-prior legs | FAIL: compensation sent (each leg run separately) |
| M9 | last-session fact applied before `slot_held` | read-gap leg | FAIL at `LandedThenRollback at gap 3` |
| M10 | `slot_held` never settles | read-gap leg | FAIL in a not-landed run: `InDoubt` where `OperationAborted` was required |
| M11 | doubt search → `.last()` | kept-doubt leg | FAIL: "round 1 may still land" |
| M12 | a lapsed lease (now ≥ expiry) drops the doubt | (F) delayed leg | FAIL at run (ii): a compensation sent. The plain short-lease leg stays green under it, so the two legs tell "short" from "lapsed" |
| M13a–e | the doubt dropped on each damage arm (torn part, torn slot, damaged last session read, record behind a missing session, prior gone) | (R) | each FAILs: `Stopped(Record(..))` where `InDoubt` was required |
| M13f | damage at a session read taken as a move | `r_a_session_damaged_and_repaired_…` | FAIL: `Refused(OperationAborted)` |

## Refutation of my own test (forced)

- **(a) Genuine red?** Yes. With the fix reverted the file does not build (the new API is part of the
  fix). With only the settling reverted (M0, the base's stop-at-once behaviour inside the new API),
  21 legs fail — every #1029 leg that injects an unknown outcome — and the 45 kept #1027/#1028
  legs still pass. Each of the 19 targeted mutations fails its named leg.
- **(b) Production path?** Yes. Every leg drives `wyrd_core::multipart::commit::commit_part` (or
  `upload_part`) over a real `RedbMetadataStore::in_memory()`, staged by the real `stage_part`; the
  wrapper only answers or delays batches and runs steps between reads. Rivals are real
  `commit_part` calls, Aborts real `abort_session`, rollbacks real CAS batches, the re-place batch is
  the custodian adoption's shape, the reserve in C3′ is the real `reserve_slot`.
- **(c) Fixture includes the fault?** Yes. The held batch is a real batch kept unapplied and later
  committed on the real store (and each leg asserts what it then meets: `Committed` or `Conflict`);
  unknown outcomes are injected as the store's own `CommitUnknownResult`; the damage, the rollback,
  the Abort, the teardown and the idle fence all land on the real store at the exact read gap. The
  counterfactual legs (`…loses_to_a_rival…`, `…away_and_back`, `…empty_prior…`) make a wrong
  compensation's bad history real rather than merely counting batches.

## Considered and ruled out

- **Keep `may_still_commit` equal to the store's report** (Stage/Create do). Rejected: after a read
  shows the session moved, a held round cannot land; reporting `true` would tell the caller to wait
  for something that cannot happen. Lowering it is the brief's "as Abort does" (Abort clears its held
  doubt on a moved session, `abort.rs:426-435`). `outcome` stays the store's raw report.
- **`cause: Option<Stop>`** for the compensation's immediate stop instead of `Stop::OutcomeUnknown`.
  Rejected to match the peers' non-optional `cause`; the variant is documented as only an
  `InDoubt::cause`, and #1030 replaces that stop.
- **A cheaper read-only lap (slot, then session only).** Rejected: the four-read lap keeps one read
  order for every round and keeps `judge_round` single; cost is at most 4 extra reads once per round
  (bounds doc `commit.rs:220-230`).
- **v7's single `commit_part` loop.** Rejected in favour of keeping #1028's `lost_commit` shape
  (now `judge_round`), so the #1028 classification table and its 45 kept legs are unchanged.

## Gate (E)

`./engine/xtask.sh ci` (= `cargo xtask ci`) on the final patched worktree: **`xtask ci: all checks
passed`**. Every step ran, none skipped: `typos`, docs lint, docs render (the brief's `typos` and
`docs-renderer` dependencies are both present on this host), the repo guards, `cargo fmt --check`,
workspace clippy (`-D warnings`), build, `cargo test --workspace --exclude wyrd-dst`, cargo-machete,
`cargo deny` (all three runs), statics, deploy-guard, and the DST clippy and tests under
`--cfg madsim`. It ran twice: once before, and once after, the last test edit (the fifth session
damage kind, `Open` at a lower epoch, added so (R) covers "the same or a lower epoch, `Open` or
not" in full). The mutation run above was also repeated on the final tree, with the same result for
every row.

No external dependency was missing, so there is no NEEDS-HUMAN item from this beat. No branch was
pushed and no PR opened.

## C5 (advisory) survivors

None known. The only branch no leg reaches is the second `ReadAgain` (`commit.rs:1338`), which the
reasoning at `:226-228` shows unreachable in a legal history; it ends in doubt rather than looping.

## Scratch

Mutation script, backups and logs under `$PDCA_SCRATCH/pdca-builder-1029-reds/` (`mutate.py`,
`reds.json` and `reds-final.json`, `ci.log` and `ci-final.log`).
