# Build notes — #927 attempt 6 (UploadPart commit, 657.2)

Target: `getwyrd/wyrd` @ `main` = `376b106`, built in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`).
Starting point, per the iteration-5 carry-forward: `iteration-v5/patch.diff`, which applies cleanly to
`376b106` (`git apply --check` passed). Nothing else from `iteration-v5/` was read. Every line number
below is on the patched tree.

## 1. What this attempt changes, and why

The iteration-5 sign-off rejected one finding: `judge_session` read **any** decodable non-`Open`
session record as a fence this attempt had lost to, whatever its epoch. The rest of the slice
(classification design, settle order, own-commit proof, compensation, ceilings, docs paragraph) is
kept as attempt 5 left it. Three things changed.

### 1.1 The fix — `crates/core/src/multipart/commit.rs:653-673` (`judge_session`)

Before (v5): decode, then `match upload_part_answer(state)`: `Accepted` ⇒ `Open(newer)` only if the
epoch rose, else `Err(SessionEpochNotRisen)`; `Refused(r)` ⇒ `Left(r)` **at any epoch**.

After: decode, then **first** `if record.epoch() <= required.epoch { return Err(SessionEpochNotRisen
{ found, required }) }` (`commit.rs:661-666`); only a record at a higher epoch is judged by state:
`Open` ⇒ `SessionSeen::Open(newer)` (a rollback), not `Open` ⇒ `SessionSeen::Left(refusal)` (a fence)
(`commit.rs:667-673`).

Why this is right, from the tree and the proposal:

- `SessionRecord::epoch` doc, `crates/core/src/multipart.rs:2585-2586`: "every transition is a
  fenced CAS that bumps it"; `open_teardown` / `completing_teardown`, `multipart.rs:2610-2611`,
  `:2636`: "every fence bumps the epoch (`0016:704-708`)"; proposal 0016 line 364: "an epoch is
  bumped by every fence". So a non-`Open` record at the same or a lower epoch than the bytes the
  batch required cannot have been written by any legal history. It is damage of the same class as
  the `Open`-at-same-epoch case v5 already treated as `SessionEpochNotRisen`.
- What the old reading did wrong (traced against v5's code, then shown by the new legs going red,
  §3.2): `Doubt::session_seen` (`commit.rs:521-530`) marks a round `may_land = false` on
  `Ok(Open | Left)`. With a non-`Open` record at a non-risen epoch planted while a commit was
  **held**, the round was marked unable to land, the next `slot_held()` (`commit.rs:541-543`)
  dropped it from the doubt, and the attempt answered `Refused(NoSuchUpload)` — for a commit that
  a repair of the session record could still let land (the held batch requires the original
  `Open@E` bytes, which the repair restores). Under a held **compensation**, `compensate`
  (`commit.rs:1152-1285`) returned `Compensated::Left(NoSuchUpload)` the same way. With nothing in
  doubt it answered `Refused(NoSuchUpload)` and left the slot, where the same record in the `Open`
  state answered `Stop::Record`.
- After the fix a non-`Open` record at a non-risen epoch is `Err(SessionEpochNotRisen)` everywhere
  `judge_session` is read: `session_seen` ignores it (keeps `may_land`), the round's last read turns
  it into `Stop::Record` through `torn` (`commit.rs:929-930`, used at `:963` and `:977`) or `end`
  (`:1218`, `:1232`), and `InDoubt::may_still_commit` stays as the store reported it.

Not changed, per the carry-forward's "not decided" note: a **missing** `mpu:` record is still
`Left(NoSuchUpload)` (`commit.rs:654-656`), pinned by
`g_a_session_with_no_record_answers_no_such_upload` (test file `:4564`).

### 1.2 The tests — `crates/core/tests/multipart_upload_part_commit.rs`

- `aborting_at(bytes, upload, epoch)` (`:4013-4031`): takes the session's **real** teardown fence
  (`SessionRecord::open_teardown`, the record a real Abort writes, `Aborting@E+1`), encodes it with
  the codec (`metadata::encode`), and rewrites only the `"epoch":` field to the asked epoch. It
  asserts the result decodes as `Aborting` at that epoch. So the planted damage is byte-for-byte a
  fence record except for the epoch — the shape a stale or misapplied fence would have.
- `session_damage(rig, session)` (`:4039-4069`) now builds four kinds: torn bytes, an `Open` record
  under other bytes at the same epoch (v5's), an `Aborting` record at the **same** epoch, and an
  `Aborting` record at a **lower** epoch; `DAMAGE_KINDS = 4` (`:4033`). It requires the session at
  epoch ≥ 1 so a lower epoch exists, so each caller runs one rollback (`Open@0 → Open@2`,
  `roll_back`, `:1370`) before staging: `:4094`, `:4153`, `:4201`, `:4232`, `:4263`, `:4324`.
- Held commit: `b_a_damaged_session_under_a_held_commit_keeps_the_doubt` (`:4087`) now loops the
  four kinds at each of the four read gaps (16 runs). Expected unchanged: in-doubt error keeping
  `may_still_commit`, cause `Record` (`SessionEpochNotRisen { found, required }` for the three
  decodable kinds), no compensation, no retry, then the repaired session lets the held commit land.
- Repaired mid-round: `b_a_session_damaged_and_repaired_mid_round_settles_nothing` (`:4147`) now
  loops the four kinds for both sub-legs (rival over an empty prior; no rival, every later round
  `Conflict`) and for the landed-during-damage run — the carry-forward's `:4117` / `:4154` TORN
  plantings are now every kind in turn.
- Held compensation: `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt` (`:4257`) loops
  the four kinds for a non-empty and an empty part; `assert_compensated(&attempt, epoch)` (`:4298`)
  now takes the live epoch (2) instead of the literal 0.
- Nothing in doubt: `g_a_session_whose_epoch_has_not_risen_is_damage_not_a_rollback_or_a_fence`
  (`:4312`, renamed from `g_an_open_session_whose_epoch_has_not_risen_is_damage_not_a_rollback`)
  runs `Open`/`Aborting` × same/lower epoch at the commit's re-read and at the compensation's:
  `Stop::Record(SessionEpochNotRisen { found, required })`, the message still saying "only a higher
  epoch" (`:4356`), nothing sent under the planted record, slot kept.
- Module doc bullet for this attempt's legs (`:37-39`).

### 1.3 The docs — the sentences that stated the old rule

- `commit.rs:79` (classification table row): "not `Open`, at a higher epoch (a fence), or gone".
- `commit.rs:88-99` (the "judged the same way" paragraph): moved = decodes at a higher epoch, `Open`
  or not, or no record; damage = undecodable, or any record under other bytes at a non-risen epoch
  **whatever its state**, with the reason (`0016:364`, `:704-708`) and the consequence of reading it
  as a fence.
- `commit.rs:291-293` (`InDoubt::may_still_commit`): "a damaged session record (one that does not
  decode, or one at an epoch that has not risen, in any state) shows nothing".
- `commit.rs:637-652` (`SessionSeen` variants and `judge_session` doc).
- `multipart.rs:386-391` (`RecordError::SessionEpochNotRisen` doc): "in any state", "not a rollback
  and not a fence the batch lost to".
- `multipart.rs:742-746` (its `Display`): "a session record at epoch {found} differs from the bytes
  required at epoch {required}: only a higher epoch can replace them, whatever the state". The
  phrase the test asserts on ("only a higher epoch", test file `:4356`) is kept.

Everything else in `patch.diff` is attempt 5's content, reproduced unchanged: `commit.rs` otherwise,
the `multipart.rs` additions (`PartRecord::commit`, `PartRecord::summary`, `OwedChunks`,
`RetireObligation::part_chunks`, `RecordError::{PartRecordMissing, PartChunkCountOverflow, …}`, the
widened `Refusal::OperationAborted` doc), the `stage.rs:363` doc fix, and the one new paragraph in
`docs/design/architecture/05-building-block-view.md` (between `:246` and `:248`, neither edited).

## 2. Alternatives considered

- **Treat only `Aborting` at a non-risen epoch as damage, keep `Completing`/`Completed` as
  fences.** Rejected: the rule is about the epoch, not the state — every transition bumps it
  (`multipart.rs:2585`), so there is no non-`Open` state a non-risen epoch can legally carry. A
  state-by-state carve-out would be two rules where one is true. Cost of the chosen fix: one
  `if` moved ahead of the `match` (`commit.rs:661-666`, +6/-11 lines).
- **Answer `Refused(NoSuchUpload)` for the nothing-in-doubt case anyway, since the record is not
  `Open`.** Rejected: it would leave the slot while telling the client the session is gone, for a
  record no legal history wrote; the table row "a store fault or a damaged record ⇒ no compensation;
  the residue stays" (`commit.rs:118`) is what the `Open`-at-same-epoch case already does, and the
  carry-forward asks for the same `SessionEpochNotRisen`.
- **Build the `Aborting` test record by text-editing `"kind":"Open"` in the session JSON.**
  Rejected for `open_teardown` + an epoch rewrite: the planted bytes are then the codec's own
  spelling of a real fence, so the leg cannot pass by accident of a hand-written shape.

## 3. Evidence

### 3.1 Runner and formatting

- Runner used: the configured per-fix gate, `./engine/scripts/run-verify.sh` (`pdca.toml`
  `C4-verify`), with `PDCA_LANE=0` and `PDCA_BUNDLE` pointing at the bundle (or at a scratch bundle
  for the reds below). It applies the patch to the lane's verify worktree off `origin/main` and runs
  `cargo test -p wyrd-core --test multipart_upload_part_commit`.
- GREEN leg (final patch): `test result: ok. 57 passed; 0 failed`. RED leg: **UNVERIFIABLE (77)** —
  the test imports `wyrd_core::multipart::commit`, which does not exist on `main`, exactly as the
  brief's *Verification posture* pre-declares for a net-new slice.
- `cargo fmt --all`: applied; the final tree is fmt-clean. `cargo clippy -p wyrd-core --all-targets
  -- -D warnings`: clean. `typos` over the five touched files: clean. `docs-renderer` (`markdown_it`,
  `yaml`) is installed, so the docs step of the gate runs rather than skips.
- `cargo xtask ci` (the whole gate, criterion (E)): see §3.6.
- `cargo doc -p wyrd-core --no-deps` (not a gate step, but `rustdoc::broken_intra_doc_links` is
  `deny` and `warnings = "deny"` makes `private_intra_doc_links` an error): attempt 5's module doc
  linked the crate-private `OwedChunks` from the public `commit` module (`commit.rs:130`); fixed to
  plain code text. After the fix **no rustdoc error is in `commit.rs`**; the 22 that remain are all
  on `main` already, in lines this patch does not touch (`erasure.rs:2`, `metadata.rs` ×14,
  `read.rs` ×2, `write.rs` ×3, `multipart.rs:5213` (`max_part_bytes`, #801's knobs), `stage.rs:55`).
  That comment-only rewrap (`commit.rs:130-132`) is the one edit made after the §3.6 gate run; the
  per-fix runner was re-run on the final tree afterwards (57/57) and `cargo fmt --all` left it
  unchanged.

### 3.2 Red → green for this attempt's fix (the carry-forward's "must fail if the fix is reverted")

Edit: `judge_session` put back to v5's shape (`match upload_part_answer(..)` with
`Refused(r) ⇒ Ok(Left(r))` at any epoch), nothing else touched. Run through `run-verify.sh` on a
scratch bundle. Result: `53 passed; 4 failed`, and exactly these four:

| leg | first panic |
|---|---|
| `b_a_damaged_session_under_a_held_commit_keeps_the_doubt` | `expected InDoubt, got Ok(Refused(NoSuchUpload))` |
| `b_a_session_damaged_and_repaired_mid_round_settles_nothing` | `expected InDoubt, got Ok(Refused(OperationAborted))` — a compensation was sent while the commit was held |
| `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt` | `expected InDoubt, got Ok(Refused(NoSuchUpload))` |
| `g_a_session_whose_epoch_has_not_risen_is_damage_not_a_rollback_or_a_fence` | `expected Stopped, got Ok(Refused(NoSuchUpload))` |

Fix restored; the final worktree diff is byte-identical to `patch.diff` (checked with `diff`).

### 3.3 The brief's demonstrated reds, re-run on this attempt's tree

Each row: one temporary edit applied to the pristine file, the test run through `run-verify.sh`,
the file restored and checked byte-identical afterwards. The driving script and the per-run logs
were written under the harness's per-issue scratch root (`$PDCA_SCRATCH/pdca-builder-927-redleg/`,
`reds.py`, `R*.log`, `results.md`), which is transient; everything the table needs is in this file.
"Expected" is the leg the brief or a carry-forward names; every expected leg was among the failures
in every row (`expected-but-green=[]`), and the baseline before each edit is 57/57 green. The
"failures" column is how many of the 57 legs failed under the edit; the expected leg is always one
of them, and the others are legs the same property also binds.

| id | the edit (file: what) | expected red leg | failures |
|---|---|---|---|
| R01 | `commit.rs` commit loop: `Some(unknown) => doubt.observe(..)` → compensate at once with `OperationAborted` | `b_a_landed_commit_answered_unknown_is_committed` | 25 (incl. expected) |
| R02 | after the part read: `if doubt.any() && part_now.is_some() { return Ok(committed) }` | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` | 11 |
| R03 | after the part read: `Committed` when the decoded part's chunks+digest equal the attempt's | `c1_an_empty_part_is_never_committed_on_its_bytes` | 4 |
| R04 | `judge_session(last_found..)` → `judge_session(first..)` (session state from the read before the slot read) | `c2_a_torn_read_never_answers_committed` | 4 |
| R05 | `Some(unknown) if !unknown.may_still_commit => {}` (out-of-flight doubt dropped before the re-read) | `c3_a_landed_commit_then_a_rival_is_committed` | 4 |
| R06 | `SessionSeen::Open(newer) =>` compensate → `return Ok(Refused(NoSuchUpload))` (rollback to the reaper) | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | 5 |
| R07 | `compensate`: `Some(unknown) => doubt.observe(..)` → `Some(_) => {}` (unknown compensation = not landed) | `c_an_unknown_compensation_outcome_is_settled_by_a_re_read` | 8 |
| R08 | commit loop `rounds += 1` → `rounds += 0` | `c_every_loop_ends_after_exactly_its_bound` (the wrapper's 200-batch runaway stop) | 12 |
| R09 | `judge_slot`: any other bytes → `Rewritten` (attempt_id ignored) | `c3_a_landed_commit_then_a_rival_is_committed` (C3′) | 1 |
| R10 | `SlotSeen::Held`: `if doubt.any()` → `if false && doubt.any()` (compensate while a commit is held) | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` | 6 |
| R11 | before the slot read: a present key the commit required absent ⇒ compensate (rival branch before the own-commit proof) | `c3_a_landed_commit_then_a_rival_is_committed` | 13 |
| R12 | `multipart.rs` `OwedChunks::validate`: empty list no longer `None` (empty `{chunks}` obligation written) | `c4_empty_parts_never_install_an_empty_obligation` (the wrapper's decode of every put refuses it) | 14 |
| R13 | both live-session losses answer `NoSuchUpload` instead of `OperationAborted` | `b_a_same_part_loser_compensates_its_own_chunks`, `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | 11 |
| R14a | lease check `&& false` (removed) | `f_a_lease_used_up_by_the_reads_sends_no_commit`, `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates`, `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | 6 |
| R14b | lease check `<` → `<=` (off by one) | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | 1 |
| R15 | halt with doubt: `LeaseShort` compensates instead of keeping the doubt (expired lease as proof) | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt`, `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` | 2 |
| R16 | lease check `&& !doubt.any()` (a round sent with the lease short while in doubt) | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` (the wrapper's margin check) | 2 |
| R17 | `doubt.slot_held()` repeated after the **last** session read (a move seen after the slot read settles) | `b_read_gap_…settles_only_at_a_later_slot_read`, `h_a_commit_held_while_a_rollback_lands_is_in_doubt` | 2 |
| R18 | `slot_held`: `retain(\|u\| u.may_land)` → `retain(\|_\| true)` (never settle) | `b_read_gap_…` not-landed runs | 7 |
| R19 | `commit_stopped` returns without compensating and `compensates_after` → `false && …` | `g_a_settled_renewal_stop_compensates_and_keeps_its_error`, `g_settled_stops_never_fill_the_slot_cap`, `f_a_lease_used_up_by_retries…`, `g_every_commit_round_out_of_flight_compensates…` | 9 |
| R20 | `compensates_after` → `true \|\| …` (compensate after `SlotLost` / renewal `InDoubt`) | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | 1 |
| R21 | `if attempt.chunk_bound_exceeded()` → `if false && …` | `d_a_part_past_the_per_part_bound_is_refused_and_compensated` | 2 |
| R22 | `compensate` slot-gone: `doubt.required(found)` → `doubt.any() \|\| doubt.required(found)` (session check always true) | `h_a_held_compensation_under_a_teardown_is_in_doubt` | 2 |
| R23a | obligation re-read loop `sent.iter().take(0)` (skipped) | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | 5 |
| R23b | `found == *bytes` → `found == *bytes \|\| true` | same leg, its `ObligationKeyTaken` run | 2 |
| R24 | `COMPENSATION_ROUNDS = MAX_COMPLETE_ATTEMPTS` (lowered by one) | `h_the_epoch_chase_lands_within_its_bound…`, `c_every_loop_ends_after_exactly_its_bound` | 3 |
| R25 | `UploadPartError::source` → `None` | `h_a_composed_error_keeps_its_class_and_its_message` | 1 |
| R26 | changed prior: `Ok(_) => {}` → `Ok(_) => doubt.cannot_land()` (iteration-2's `can_recur=false`, iteration-4's rule) | `b_a_rival_over_an_empty_prior_settles_nothing…`, `b_a_held_reupload_over_a_non_empty_prior_that_loses_to_a_rival_keeps_the_doubt`, `b_a_held_reupload_survives_a_repair…` | 5 |
| R27 | `staged_elsewhere`: own part number no longer skipped | `d_the_cumulative_ceiling_leaves_out_the_parts_own_prior` | 1 |
| R28 | `commit_batch`: `require_absent(obligation.key())` removed | `d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part` | 1 |
| R29 | `Doubt::end`: `.find(may_land).or_else(last)` → `.last()` | `b_the_kept_doubt_is_the_round_that_may_still_land` | 1 |
| R30 | epoch chase: `sent.clear(); sent.push(put)` (only the latest obligation re-read) | `h_the_epoch_chase_re_reads_every_obligation_it_sent` | 1 |
| R31 | `judge_session`: undecodable bytes → `Left(NoSuchUpload)` (iteration-3's bug) | `b_a_damaged_session_under_a_held_commit_keeps_the_doubt`, `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt`, `b_a_session_damaged_and_repaired_mid_round_settles_nothing` | 4 |
| R32 | `PartRecordMissing` check `&& false` (missing prior read as a rival) | `g_a_prior_part_record_gone_under_the_same_session_is_damage` | 1 |
| R33 | `judge_slot`: own id under other bytes → `Held` | `b_a_slot_rewritten_under_the_attempts_own_id_during_a_held_batch_is_slot_lost` | 2 |
| R34 | `compensate`: the slot and session read faults return `stopped(..)` instead of `end(doubt, ..)` | `h_a_store_fault_while_a_held_compensation_is_settled_keeps_the_doubt` | 1 |
| R35 | re-upload's obligation epoch hard-coded to `0` | `d_a_reupload_under_a_newer_epoch_owes_the_prior_under_that_epoch` | 1 |
| R36 | damaged part record: `Stop::Record` only under `SessionSeen::Same` | `g_a_damaged_part_record_under_a_moved_session_is_a_record_stop` | 1 |
| R37 | **this attempt's fix reverted** (§3.2) | the four legs of §3.2 | 4 |

Two edits first failed to compile under the workspace's `warnings = "deny"` (R22 left `Doubt::required`
unused; R33 left `SlotSeen::Rewritten` never constructed); they were rewritten to keep the symbol
used and re-run, with the results above. No edit was accepted as "red" on a compile error.

**Slot bytes, by invariant rather than by red** (brief, *Verification posture*). Both batch
builders take the slot bytes from the attempt's own state: `commit_batch`, `commit.rs:715`
(`batch.require(attempt.slot_key().to_vec(), attempt.slot_bytes().clone())`) and
`compensation_batch`, `commit.rs:744` (same call). Neither reads the key. The test wrapper checks on
**every** batch of every leg that a batch naming an attempt's `slot:` key requires exactly the bytes
that attempt last wrote there (`check_slot_bytes`, test file `:584-609`, recorded from the attempt's
own landed reserve/renewal batches). The "re-read bytes" mutant (a retry builder requiring bytes
read back from the key) stays **unreachable under the classification**: a conforming attempt
recognises its own commit (slot gone or another attempt's id, session still `Open@E`) and returns
`Committed` before any later batch is built, and no conforming path sends a batch after another
attempt holds the index, so no fixture can run such a builder; the wrapper's per-batch check is the
guard instead.

### 3.4 C5 — `cargo mutants --in-diff` over this slice's code

Full run over the bundle's `patch.diff` (the same invocation as `scripts/mutants-in-diff`:
`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true`, here with `--jobs 4`, output
under `$PDCA_SCRATCH/pdca-builder-927-mutants/full-out`): **83 mutants: 66 caught, 17 unviable,
0 missed, 0 timeout** (baseline 12 s build + 3 s test; the oracle was `cargo test
--package=wyrd-core`, 29 test binaries including `multipart_upload_part_commit`).

Survivors ("missed"): **none**. Attempt 1's nine survivors are gone; in particular the three
message-text ones the brief names are caught: `lossy -> String::new()` and `lossy -> "xyzzy"`
(`commit.rs:585`), and `<impl Display for Stop>::fmt` (`commit.rs:402`), `… for CommitError`
(`:361`), `… for UploadPartError` (`:476`), `… for RecordError` (`multipart.rs:618`) — the test
helpers `in_doubt`/`stopped`/`renewal_stopped` (test file `:1427-1514`) assert on the message text.

Unviable (17; each is cargo-mutants' "replace the whole body with `Default::default()`" on a type
with no `Default`, so the mutant does not compile; none is a surviving behaviour change):
`RetireObligation::part_chunks` (`multipart.rs:4135`), `PartRecord::summary` (`:3088`),
`PartRecord::commit` (`:3064`), `OwedChunks::validate` (`:4183`), `CommitError::source`
(`commit.rs:444`), `UploadPartError::source` (`:485`), `Doubt::end` (`:557`), `stopped` (`:577`),
`judge_slot` (`:602`), `Session::of` (`:627`), `judge_session` (`:654`), `commit_part` (`:790`),
`renewal_stopped` (`:1086`), `commit_stopped` (`:1111`), `refuse` (`:1130`), `compensate` (`:1158`),
`upload_part` (`:1304`).

A narrow run over only this attempt's production delta (v5 → now, `delta.diff`): 3 mutants, 2
caught (`<=` → `>` in `judge_session`, `commit.rs:661`; `RecordError::fmt` stub), 1 unviable
(`judge_session` body stub).

### 3.5 The three refutation questions

- **(a) Genuine red?** Yes. With only `judge_session` reverted to v5's shape and every test kept,
  the run goes `53 passed; 4 failed`, the four being exactly the legs changed for this attempt
  (§3.2, R37). Each of the other 36 named edits also goes red on the leg it names (§3.3).
- **(b) Production path?** Yes. The tests call `wyrd_core::multipart::commit::{commit_part,
  upload_part}` as shipped, over `RedbMetadataStore::in_memory()` (the real store crate) behind a
  logging/scripting `MetadataStore` wrapper that only decides how a batch is answered and runs
  actions between reads; the rival commits and the prior parts are produced by the same
  `commit_part` on the inner store; sessions by `create_session`, parts by `stage_part`, aborts by
  `abort_session`. No part of the commit is re-implemented or mocked.
- **(c) Fixture includes the fault?** Yes. The damaged record is planted at the real `mpu:` key of
  the session the attempt is committing under, at the read gap under test, and it is the codec's
  own encoding of a real `open_teardown` fence with only the epoch rewritten (`aborting_at`); the
  held batch is the attempt's real batch, held by the wrapper and later applied to the inner store,
  and the leg asserts it lands once the record is repaired. The repaired-mid-round runs plant and
  repair within one round, at each gap in turn.

### 3.6 Criterion (E) — the whole gate

`./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`, with `CARGO_BUILD_JOBS=8` to stay
inside the builder leaf's 16 GB memory bound) on the final tree: **`xtask ci: all checks passed`,
exit 0.** Steps seen in the log: gitlink-guard, unsafe-guard, blackbox-guard, typos, docs, fmt
`--check`, clippy `--workspace --all-targets`, build, `cargo test --workspace` (223 `test result`
lines), cargo-machete, cargo-deny, conformance (5 valid + 6 invalid vectors), statics (ADR-0035),
deploy-guard, and the DST run (`crates/dst`, madsim). The iteration-2 note about `custodian_gc`
stalling did not recur: `tests/custodian_gc.rs` ran `10 passed; 0 failed … finished in 0.17s`.
The log is `$PDCA_SCRATCH/xtask-ci.log` (transient).

## 4. Decisions kept from the sign-offs (not re-opened here)

- Dispatch-only lease promise (`DISPATCH_MARGIN_MILLIS`, `commit.rs:202-208`); the 0016 wording at
  `:1727-1728` is the human's to take up — this slice edits no proposal.
- `Refused(OperationAborted)` for a compensated live-session loss (`commit.rs:1043`, `:1049`).
- Slots left held when a Complete fence rolls back (iteration-2 note): unchanged.
- A rollback before the quiesce renewal is adopted by the renewal (`commit.rs:807-809`): unchanged.
- The missing `mpu:` record with the slot still held answers `NoSuchUpload` (`commit.rs:654-656`):
  unchanged, "not decided" at the iteration-5 sign-off.
- With nothing in doubt, a part record changed by a plain repair compensates and answers
  `OperationAborted`, no retry against the new record (`commit.rs:1040-1045`, the comment at `:1040`).
- The size backstop stays overridden (patch 313 KB, mostly test legs); nothing was trimmed.

## 5. Self-review against `AGENTS.md` § Review rubric & protocol

- *One clock per lifecycle*: no clock read added or moved; the commit still reads only the caller's
  `StageClock` (`commit.rs:165-170`).
- *Narrow trait seams*: `commit_part` still takes `&impl MetadataStore` only; no new seam.
- *Metadata validation boundaries (ADR-0045)*: the change makes a structurally impossible record
  (non-`Open` at a non-risen epoch) an **error** (`RecordError::SessionEpochNotRisen`) instead of a
  value the classification acted on — the direction the convention asks for.
- *No DST-reachable shared mutable global state*: the test helpers add no `static`; the gate's
  `statics` step passed.
- *`#![forbid(unsafe_code)]`*: no new crate root; `unsafe-guard` passed.
- *Docs currency*: no new port, API operation, RPC, CLI flag or persisted field; the architecture
  paragraph from attempt 5 stands; the doc comments that stated the old rule are all rewritten (§1.3).
- *Absent or unsupported entries*: the new case produces an explicit error, never silent success;
  the held batch stays in doubt rather than being declared lost.
- *Serialization identity*: the test's planted record is the codec's own encoding of a real fence
  with one field rewritten, and the helper asserts it decodes to the intended epoch and state.
- *Test fidelity*: every new leg drives the production `commit_part`/`upload_part` over the real
  redb store; seeded DST coverage of the races stays deferred to #660 (`commit.rs:178-179`).

## 6. For the human at sign-off

- **C4-verify is UNVERIFIABLE (77) by construction** (net-new test file importing the new module);
  the per-fix red for this attempt is §3.2 and the brief's full list is §3.3.
- No `NEEDS-HUMAN external dependency` to declare: `typos`, the docs renderer, `cargo-mutants`,
  `cargo-deny` and `cargo-machete` are all present and were run.
- The narrowing of iteration 3's wording ("a record that decodes and is not `Open` counts as
  moved") to "…at a higher epoch" is exactly the iteration-5 instruction; it changes no legal
  history (every legal non-`Open` record is at a higher epoch than the `Open@E` any batch of this
  slice requires).
