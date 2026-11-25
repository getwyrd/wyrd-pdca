# Build notes — #927, attempt 5 (Iteration 4 carry-forward)

Target: `getwyrd/wyrd` @ `main` = `376b106`, built in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`).
Every `path:line` below is against that worktree with `patch.diff` applied.

Start point, as the Iteration 4 carry-forward directs: `iteration-v4/patch.diff`, which applies
cleanly to `376b106`. Everything the brief's criterion asks for that v4 already held is kept as
v4 had it; the changes below are the carry-forward's four items, nothing else. I read nothing of
`iteration-v4/` but the patch, and nothing of earlier iterations.

## What changed, item by item

### 1. The fix: a changed prior settles nothing, empty or not

`crates/core/src/multipart/commit.rs:938-942`. The part-key judgement after a round that did not
land is now:

```rust
Ok(Some(_)) if required.is_none() => doubt.cannot_land(),   // required absent, now present
Ok(_) => {}                                                 // a changed or gone prior: nothing
Err(err) => damaged = Some(err),
```

`can_recur` (v4 `commit.rs:824`, `:901`) is gone. A record over a key the commit required absent
still settles the held commit (a `part:` record is removed only after the session leaves `Open`,
`0016:351`). Any changed prior settles nothing: its bytes can come back.

Why the old rule was false, from the code the sign-off cites: the custodian's staged re-place
adoption is `require(mpu == Open@E) ∧ require(part == prior) → put(part, next)`
(`crates/custodian/src/reconstruction/staged.rs:16-27`, built at `:722-725`), and `next` is
derived from the stored bytes with **only** the chunk list replaced and every other byte kept
(`repointed_part`, `staged.rs:773-798`: it finds `metadata::encode(&record.chunks())` in the
stored bytes and splices the repointed list in, then requires the decoder to read back the same
`len`, `digest`, `committed_at_millis` and `session_epoch`). Because the encoding is canonical,
a later pass that moves the same fragment back to its earlier server writes the prior's exact
bytes again, and a held re-upload's `require(part == prior)` holds once more. The test's
`repoint` step (`crates/core/tests/multipart_upload_part_commit.rs:4622-4655`) builds the
adoption's exact shape the same way and asserts the read-back, so the "exact bytes return"
claim is exercised, not argued: in the `BeforeRetry` runs of the new leg the retry is a `Pass`
on the real store and lands only because the bytes really are the prior's again.

Module docs corrected to the new rule: `commit.rs:53-66` (*Settled, not landed*) and the
classification row `commit.rs:77`. The code comment at `:939-941` states it. The classification
with **nothing** in doubt is unchanged, as the sign-off decided: a changed prior still
compensates and answers `Refused(OperationAborted)` with no retry against the new record
(`commit.rs:1034-1039`).

### 2. New leg: a held re-upload while repair moves the prior away and back

`b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back`
(`tests/multipart_upload_part_commit.rs:4731-4827`), 15 runs: the fragment is moved away at each
of the five read gaps of the first lap (0 = right after the held commit, then after the part
read, the first session read, the slot read, the last session read), and moved back at one of
three moments — before the retry (`Restored::BeforeRetry`, the retry then lands itself on the
real store), right after the retry conflicted with the held batch released beside it
(`WithRelease`), or after the attempt has returned in doubt (`AfterReturn`). Required in every
run: no compensation batch in the log, never `OperationAborted`, the answer `Committed` or the
in-doubt error keeping `may_still_commit`, and at the end the attempt's own part durable, the
prior's chunks owed under `retire:bytes:`, the slot released exactly once. If a compensation is
ever sent, the wrapper makes the race real (moves the prior back, lands the held commit, answers
the compensation `CommitUnknownResult { may_still_commit: true }` unapplied), so an
implementation with the old rule answers `OperationAborted` for a part that committed — which is
exactly what v4 does under this leg (the `[it4]` rows of the reds table).

### 3. New leg: a held re-upload over a non-empty prior that loses to a rival

`b_a_held_reupload_over_a_non_empty_prior_that_loses_to_a_rival_keeps_the_doubt`
(`tests:4667-4707`): in-doubt error, flag kept, `CommitRoundsSpent { 4 }`, no compensation, the
slot still held and covered by the doubt; the held batch released afterwards conflicts on the
rival's record. Same wrapper trick as above if a compensation is sent. The brief said to update
any existing leg that expected the old answer: there is none. I ran all 52 v4 legs against the
fixed code before adding any leg and all passed, which matches the reading that every v4 leg
with a rival over a prior used an empty prior (`c1_…`, `b_a_rival_over_an_empty_prior_…`,
`b_a_held_commit_beside_an_unknown_empty_compensation_…`) or a first upload (required absent,
which still settles). The empty-prior leg's doc now says the rule covers any prior
(`tests:3702-3709`), and the file header lists the attempt-5 legs (`tests:33-36`).

### 4. The adversary's gaps

- **Store faults inside `compensate`** —
  `h_a_store_fault_while_a_held_compensation_is_settled_keeps_the_doubt` (`tests:4836-4901`),
  seven runs: a held round followed by a round that fails outright, and a fault on the obligation
  read, the slot read and the session read, for a non-empty part and (minus the obligation read)
  an empty one. Each expects `InDoubt { batch: Compensation, may_still_commit: true, cause:
  Store }`, then releases the held batch and asserts the one release. The four sites are
  `commit.rs:1195` (the failed round), `:1206` (obligation read), `:1214` (slot read), `:1221`
  (session read); each goes red when its `end(doubt, …)` becomes a plain `stopped(…)` (reds
  table).
- **A re-upload above epoch 0** —
  `d_a_reupload_under_a_newer_epoch_owes_the_prior_under_that_epoch` (`tests:4908-4939`): a
  Complete's rollback before the quiesce renewal is adopted by it, the commit lands under `Open@2`
  and the prior's obligation is keyed `…:2:…`. The epoch comes from the decoded session the batch
  requires (`commit.rs:851-857`, `session.epoch`); hard-coding `0` there is red.
- **A damaged `part:` record after the session moved** —
  `g_a_damaged_part_record_under_a_moved_session_is_a_record_stop` (`tests:4946-4966`): damage is
  judged before the session's row (`commit.rs:1008-1010`), `Stop::Record`, no compensation under
  either epoch, slot and entry kept.
- **`commit.rs:1125` and `:1135` (v4 numbering) deleted.** Neither was reachable: the `session`
  bytes `compensate` decoded were either the attempt's own (`StagedAttempt::session_bytes`, which
  staging and the renewal decoded when they read them, `stage.rs:1504-1507`, `:1797-1800`) or a
  record `judge_session` had just decoded; and `RetireObligation::part_chunks` could refuse only an
  RS scheme the coder does not support, while staging encodes every chunk with the request's
  scheme before it registers it (`stage.rs:1510-1516` plans placement from that scheme; a chunk is
  registered only once its fragments are encoded), so no `StagedAttempt` carries one. No test can
  reach either, so they are deleted, by making the loop's two operations infallible:
  - `Session { bytes, epoch }` (`commit.rs:611-627`) is decoded once where the bytes enter the
    commit and threaded through `renewal_stopped`, `commit_stopped`, `refuse` and `compensate`
    (`:1073-1079`, `:1098-1104`, `:1117-1123`, `:1146-1151`); `SessionSeen::Open` carries one
    (`:636`), minted by `judge_session` from the record it decoded (`:644-668`), so the epoch
    chase re-mints nothing. `Doubt` keeps the required `Session` per batch (`:489-496`, `:545-549`).
    This also removed `judge_session`'s decode of `required` (v4 `:628`), a third never-run path.
  - `OwedChunks` (`crates/core/src/multipart.rs:4160-4187`) validates a chunk list once
    (`checked_chunk_scheme`, the empty-list `None` contract kept), and
    `RetireObligation::part_chunks` takes an `&OwedChunks` and is infallible (`multipart.rs:4125-4150`).
    `compensate` mints the attempt's own obligation per round from the validated list and the
    round's epoch (`commit.rs:1167-1176`), with no failure path.
  What remains fallible is at the boundary where the attempt's own bytes enter `commit_part`:
  `commit.rs:791` (own chunks), `:798` and `:804` (the session, once per path — see the next
  paragraph), `:836` (the prior's bytes, a store read), `:839-845` (`PartRecord::commit`, which
  the brief requires as the writer-side constructor), `:848-849` (the prior's chunks). All but
  `:836` are unreachable in a legal history for the same reasons, and all are `Result`s because
  the decoders and constructors are (ADR-0045 validates at the boundary). The only way to remove
  `:798`/`:804` is a `StagedAttempt::session_epoch()` accessor in `stage.rs`, which the brief closes
  to this slice beyond its one doc fix; I flag it below as a follow-up for the human.

  Why the session is decoded **after** the renewal rather than once at the top: the renewal
  adopts a rollback that landed before it (`stage.rs:1817-1823`, `moved_on` sets
  `self.session`), so the bytes every later batch requires can differ from the ones the attempt
  staged under. `b_a_rollback_before_the_quiesce_renewal_commits_under_the_live_epoch` and the
  new `d_a_reupload_under_a_newer_epoch_…` hold that. The bound-exceeded path decodes before the
  renewal because it never renews (`:798`); the main path decodes after it (`:804`), and
  `renewal_stopped` gets the post-renewal bytes too (a renewal that adopted `Open@E'` and then
  stopped compensates under `E'`). I did not make the second decode conditional on the bytes
  having changed: that `!=` would be an equivalent mutant (`==` only re-decodes needlessly) and
  it buys one decode of a 200-byte record.

### Also carried from v4 unchanged

The writer constructor `PartRecord::commit` and `PartRecord::summary`, `RecordError`'s three new
variants, the `Refusal::OperationAborted` doc, the `StageOutcome::Staged` doc fix (`stage.rs:363-367`,
the only `stage.rs` edit), the architecture paragraph
(`docs/design/architecture/05-building-block-view.md:248`, one new line between `:246` and the
`mpufence` paragraph, neither edited), and the whole test harness.

## Alternatives considered

- **`RetireObligation::under_epoch(self, epoch) -> Self` to re-mint under a newer epoch** (≈6
  lines against `OwedChunks`'s ≈28). Rejected: it needs an arm for `RetireToken::Generation`
  that silently does nothing (the rubric's "never silent skip"), or a panic; and the first mint
  stays fallible, so the Record path moves rather than goes.
- **Keep the fallible mint and decode in the loop and add a test that reaches them.** No legal
  history reaches them (above), so the test would have to construct a `StagedAttempt` by hand,
  which its private fields forbid. Rejected.
- **`StagedAttempt::session_epoch()` in `stage.rs`** (4 lines) would delete `:798`/`:804`.
  Rejected for this slice only because the brief's Scope says the `StageOutcome` doc fix is the
  whole of this slice's `stage.rs` edit (decided 2026-10-30). Follow-up below.
- **Decode the session once before the renewal and reuse it.** Wrong, per `moved_on` above.
- **Treat only a *non-empty rival's* record over a non-empty prior as settling, keeping the empty
  case as v4 had it.** That is the rule the sign-off refuted: a repair's rewrite is also a
  non-empty record over a non-empty prior and its bytes come back. Rejected.
- **Retry the commit against the new record when nothing is in doubt and a repair rewrote the
  prior.** The sign-off decided against it: the existing classification (compensate,
  `OperationAborted`, the client retries) stands.

## Demonstrated reds — 46 temporary edits, each restored before the next

Method: a script under `$PDCA_SCRATCH/pdca-builder-927-reds/` (`reds.py`; its log `reds.log` and
`reds-rerun.log`) applied one exact-string edit to `commit.rs` or `multipart.rs`, ran
`cargo test --quiet -p wyrd-core --test multipart_upload_part_commit <leg>` under `timeout 900`
(the invocation `engine/scripts/run-verify.sh:441` uses), recorded red / green / compile error,
and restored the pristine file (`cmp` against the kept copy, byte-identical, after each run). An
edit that failed to **compile** was not counted red; five first drafts did, because the
workspace denies warnings (`Cargo.toml:255`, `[workspace.lints.rust]`) and dead code or an
unused binding is an error — each was rewritten to keep every symbol used and re-run. Every
row is RED. Where the leg filter is a prefix (`g_`, `b_a_held_reupload`) the whole group ran.

| edit (what was made wrong) | leg(s) run | result |
|---|---|---|
| compensating on `CommitUnknownResult` without re-reading: `Some(unknown) => doubt.observe(..)` → `return refuse(.., OperationAborted)` | `b_a_landed_commit_answered_unknown_is_committed` | RED |
| after an unknown outcome, "a part is there" taken as `Committed`: `if doubt.any() && part_now.is_some() { return Ok(committed) }` before the part judgement | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` | RED |
| part bytes alone (same chunks and digest) taken as the own-commit proof | `c1_an_empty_part_is_never_committed_on_its_bytes` | RED |
| the own-commit proof judged on the session read **before** the slot read (`first`) | `c2_a_torn_read_never_answers_committed` | RED (gap 2 answers `Committed`) |
| a `may_still_commit: false` unknown dropped from the doubt before the re-read | `c3_a_landed_commit_then_a_rival_is_committed` | RED |
| the rolled-back `Open@E'` answered `NoSuchUpload` instead of compensated | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | RED |
| an unknown compensation outcome not observed (treated as "did not land") | `c_an_unknown_compensation_outcome_is_settled_by_a_re_read` | RED (the held-then-conflicts run) |
| `rounds += 1` → `rounds += 0` in the commit loop | `c_every_loop_ends_after_exactly_its_bound` | RED (the wrapper's 200-batch runaway guard) |
| "my slot is gone" read as "the key is absent": any other bytes → `Rewritten` | `c3_a_landed_commit_then_a_rival_is_committed` | RED (C3′) |
| the `doubt.any()` guard before the classification disabled (a compensation while the commit is held) | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` | RED |
| the rival-record branch checked before the own-commit proof | `c3_a_landed_commit_then_a_rival_is_committed` | RED |
| `OwedChunks::validate` returning `Some` for an empty list (an empty `{chunks}` obligation written) | `c4_empty_parts_never_install_an_empty_obligation` | RED (the wrapper's decode check) |
| the same-part loser answering `NoSuchUpload` | `b_a_same_part_loser_compensates_its_own_chunks` | RED |
| the rollback loser answering `NoSuchUpload` | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | RED |
| the lease coverage check removed (`else if false`) | `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates` | RED |
| the lease coverage check off by one (`<` → `<=`) | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | RED |
| a compensation sent when the lease is short with a commit in doubt (the `doubt.any()` guard at the halt removed) | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` | RED |
| a lease past expiry taken as proof a held commit did not land (`LeaseShort` compensates despite the doubt) | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` | RED |
| a rival seen after the last slot read counted as settling (`cannot_land(); slot_held()`) | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | RED (landed-then-rival) |
| a moved session seen by the last read counted as settling (`slot_held()` after the last `session_seen`) | same | RED (landed-then-rollback) |
| settling never allowed after a precondition dies (`cannot_land` and `session_seen` made no-ops) | same | RED (the not-landed runs) |
| a settled renewal stop returning without compensating (`compensates_after` → `false`) | `g_*` | RED (incl. the cap witness) |
| a settled commit stop returning without compensating (`commit_stopped` never compensates) | `g_*` | RED (incl. the cap witness) |
| a compensation after `SlotLost` / a renewal `InDoubt` (`compensates_after` → `true`) | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | RED |
| the over-limit check in the commit removed | `d_a_part_past_the_per_part_bound_is_refused_and_compensated` | RED |
| the session check after an unknown compensation made always true (`doubt.required(..) \|\| doubt.any()`) | `h_a_held_compensation_under_a_teardown_is_in_doubt` | RED |
| the obligation re-read skipped (`sent.iter().take(0)`) | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | RED |
| the obligation byte comparison made always true | same | RED (the `ObligationKeyTaken` run) |
| `COMPENSATION_ROUNDS` lowered by one | `h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` | RED |
| `UploadPartError::source` returning `None` | `h_a_composed_error_keeps_its_class_and_its_message` | RED |
| **[it4] attempt 4's rule restored**: `Ok(Some(_)) if !prior.is_empty() => doubt.cannot_land()` | `b_a_held_reupload*` (both new legs) | RED |
| [it4] the same, run on the repair leg alone | `b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back` | RED |
| [it2] `can_recur` forced false: any changed record settles | `b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back` | RED |
| [it2] the ceiling's exclusion of the part's own prior disabled | `d_the_cumulative_ceiling_leaves_out_the_parts_own_prior` | RED |
| [it2] `require_absent` on the re-upload's obligation key removed | `d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part` | RED |
| [it2] the kept doubt `.find(may_land)` → `.last()` | `b_the_kept_doubt_is_the_round_that_may_still_land` | RED |
| [it2] the epoch chase keeping only the latest obligation (`sent.clear()`) | `h_the_epoch_chase_re_reads_every_obligation_it_sent` | RED |
| [it3] an undecodable session read treated as a moved session | `b_a_damaged_session_under_a_held_commit_keeps_the_doubt` | RED |
| [it3] a missing prior part record answered `OperationAborted` | `g_a_prior_part_record_gone_under_the_same_session_is_damage` | RED |
| [it3] a slot rewritten under the attempt's own id read as `Held` | `b_a_slot_rewritten_under_the_attempts_own_id_during_a_held_batch_is_slot_lost` | RED |
| [it4] a failed compensation round after a held one → plain stop (`commit.rs:1195`) | `h_a_store_fault_while_a_held_compensation_is_settled_keeps_the_doubt` | RED |
| [it4] the obligation read fault → plain stop (`:1206`) | same | RED |
| [it4] the slot read fault → plain stop (`:1214`) | same | RED |
| [it4] the session read fault → plain stop (`:1221`) | same | RED |
| [it4] the re-upload's obligation epoch hard-coded to `0` (`:853`) | `d_a_reupload_under_a_newer_epoch_owes_the_prior_under_that_epoch` | RED |
| [it4] the damaged-part check under a moved session removed (`:1008-1010`) | `g_a_damaged_part_record_under_a_moved_session_is_a_record_stop` | RED |

The brief's list also names "a compensation after `EntryLost`": `StagedAttempt::renew` cannot
produce `EntryLost` (only a chunk's intent does), so no store history reaches that row through
the renewal; the unit test `a_stopped_renewal_compensates_only_on_the_rows_that_say_so`
(`commit.rs:1326-1354`) pins the table row instead, as v4 did.

## Slot bytes, by invariant rather than by red

Both batch builders take the slot bytes from the attempt's own state, never from a read of the
key: `commit_batch` at `commit.rs:708-709` and `compensation_batch` at `:737-738`, each
`batch.require(attempt.slot_key().to_vec(), attempt.slot_bytes().clone())` — `slot_bytes()` is
what the last renewal that landed left (`stage.rs:1774-1779`, `apply`). The wrapper checks the
invariant on **every** batch of **every** leg: `Scripted::check_slot_bytes`
(`tests/multipart_upload_part_commit.rs:581-606`) requires that a batch naming an attempt's `slot:`
key requires exactly the bytes that attempt last wrote there (recorded from its own landed
reserve and renewal batches, `landed`, `:565-576`), and that a slot is deleted only under such a
requirement; it is called from the wrapper's `commit` before any scripting (`:705-707`). A
mutant that re-reads the slot bytes from the key is unreachable under the classification: a
conforming attempt recognises its own commit (slot gone, session still `Open@E`) and returns
`Committed` before any later batch is built, and no conforming path sends a batch after another
attempt holds the index, so no fixture drives a builder with re-read bytes — which is why the
check is an invariant on the log and not a red.

## Refuting my own test — the three questions

- **(a) Genuine red?** Yes. With attempt 4's rule put back (`Ok(Some(_)) if
  !prior.as_ref().is_some_and(PartRecord::is_empty) => doubt.cannot_land()` in place of
  `commit.rs:938`), both new legs fail (rows `[it4]` of the table; run again by hand at the end
  with the output captured):
  - the repair leg panics at `tests:4781` — `BeforeRetry, moved away at gap 0: no compensation
    while the commit may land` (one compensation batch in the log where none is allowed);
  - the rival leg panics at `tests:4688`: the answer is `InDoubt { batch: Compensation,
    may_still_commit: true, cause: ObligationKeyTaken { key:
    "retire:bytes:s:<id>:0:1:<attempt>" } }` with one compensation in the log, where the in-doubt
    error must name the **commit** and no compensation may be sent.

  That second message is worth a sentence, because it differs from the symptom the Iteration 4
  rationale predicted (a false `Refused(OperationAborted)` or a definite `SlotLost`). Under the
  old rule the attempt compensates; the wrapper then moves the prior back and lands the held
  commit **before** answering the compensation unknown. That commit is a re-upload over a
  non-empty prior, so it installed the prior's chunks under
  `retire:bytes:s:<id>:<E>:<n>:<attempt>` — the same key the compensation's obligation for the
  attempt's own chunks would take (0016's token is per attempt and epoch, `0016:358-373`). The
  compensation's re-read finds other bytes there and stops with `ObligationKeyTaken`, in doubt.
  So the old rule's actual failure is a false "damage" stop with a compensation sent against a
  landed commit — still never `Committed`, still safe, still wrong — and over an **empty** prior
  (no obligation in the commit) the same history would be the predicted false
  `Refused(OperationAborted)`. Nothing about this changes the fix: the shipped code sends no
  compensation while the commit may land, so the collision never arises. It is a property to
  know: the commit's and the compensation's obligations share a key by design and are mutually
  exclusive by the slot CAS, and both batches `require_absent` that key (`commit.rs:710-712`,
  `:739-741`), so if an implementation ever did send both, the second fails loudly instead of
  overwriting the first. The other 44 edits cover the rest of the file; every one is red.
- **(b) Production path?** Yes. The legs call `wyrd_core::multipart::commit::commit_part` and
  `upload_part` (`tests:61-63`, `Rig::commit` `:1039-1041`, `Rig::upload` `:1120-1149`) over
  `RedbMetadataStore::in_memory()`; the scripted wrapper delegates every read and every
  un-scripted batch to that real store (`:670-776`) and only chooses *when* a batch lands, holds,
  or is answered unknown. Staging is the real `stage_part`, the rival's commit is the real
  `commit_part` on the inner store, Abort is the real `abort_session`. Two writers are test-local
  batches in the exact shape of code this crate cannot call: the Complete's rollback (`roll_back`,
  `:1370-1399`, decision 3; Complete is #658) and the custodian's re-place adoption (`repoint`,
  `:4622-4655`; the custodian is another crate with a pass-level API). The carry-forward allows
  "real re-place batches or their exact shape"; `repoint` carries the adoption's two
  preconditions and derives the new value as `repointed_part` does, asserting the read-back.
- **(c) Fixture includes the fault?** Yes. In the repair leg the stored part bytes really change
  (the `BeforeRetry` runs prove it from the other side: the retry is a plain `Pass` on the real
  store and lands only once the bytes are the prior's again; in `WithRelease` the released held
  batch really lands, asserted `Committed`, and in `AfterReturn` the same after the in-doubt
  return). In the rival leg the rival really commits through `commit_part` and the held batch,
  released over its record, is asserted to `Conflict`. The store-fault legs inject the fault on
  the exact read (`fail_read(n)`, counted from the first commit) or round, and then release the
  held compensation and assert its landing is the one release. Nothing is curated out: the
  compensation script that would expose the old rule stays armed in every run and is asserted
  never to have fired.

## Gates I ran

- `cargo fmt -p wyrd-core` — applied; the patch is formatted.
- `cargo clippy -p wyrd-core --all-targets -- -D warnings` — clean (the workspace already denies
  warnings in every build, `Cargo.toml:255-268`).
- `cargo test -p wyrd-core --test multipart_upload_part_commit` — 57 passed (52 v4 legs + 5).
- **C4-ci** — `./engine/xtask.sh ci` on the patched worktree: **green**, `xtask ci: all checks
  passed`, exit 0 — see *Full gate result* below.
- **C5 mutants** — `cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true` (the
  `scripts/mutants-in-diff` invocation, plus `-j 6` and `-o` into the scratch dir): **no
  survivor** — see *Mutants* below.

Runner note: the project's gate runner is `cargo xtask ci` (via `engine/xtask.sh`), which has no
single-target mode; for the red→green passes I used the same `cargo test --quiet -p wyrd-core
--test multipart_upload_part_commit` invocation `engine/scripts/run-verify.sh:441` uses, wrapped
in `timeout 900` so no run can hang the beat.

## Full gate result

`PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l1 ./engine/xtask.sh ci` (the project's gate runner,
delegating to `cargo xtask ci`), run once over the final tree, log at
`$PDCA_SCRATCH/pdca-builder-927-ci.log`. It **completed**, in about six minutes of wall clock, and
ended `xtask ci: all checks passed`, exit 0. Every step ran, none warn-skipped — the prose gates
had their tools (`typos`, `lint_docs.py`, `render_site.py --check`), then `gitlink-guard`,
`unsafe-guard`, `blackbox-guard`, `cargo fmt --all -- --check`, `cargo clippy --workspace
--exclude wyrd-dst --all-targets`, `cargo build --workspace --exclude wyrd-dst --all-targets`,
`cargo test --workspace --exclude wyrd-dst`, `cargo-machete`, `cargo fetch --locked`, the three
`cargo deny` walls (the all-features advisories one included), `xtask statics`, `xtask
deploy-guard`, `cargo clippy -p wyrd-dst (--cfg madsim)` and `cargo test -p wyrd-dst (--cfg
madsim)`. Totals over the 223 test binaries: 2148 passed, 0 failed.

On the Iteration 2 carry-forward's item 4 (the earlier run that hit its 7200 s limit in
`custodian_gc`): this run did not stall. The one test the harness flagged as slow,
`drain_ceiling_refusal_is_confirmed_on_the_segment` in `crates/dst/tests/custodian.rs:6626` (a
`dst_campaign_test!` sweeping madsim seeds, unrelated to multipart), printed "has been running
for over 60 seconds" once and then `ok`; `custodian_gc` ran to completion with the rest. So the
full gate is complete evidence for criterion (E) this time, and I have nothing to add about the
earlier stall beyond: it did not reproduce on this host with this tree.

## Mutants

`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true -j 6` (the `scripts/mutants-in-diff`
invocation plus parallelism and an `-o` into `$PDCA_SCRATCH/pdca-builder-927-mutants/`; the
target's `.cargo/mutants.toml` exclusions apply, none of them in these files):

```
Found 85 mutants to test
ok       Unmutated baseline in 18s build + 3s test
85 mutants tested in 2m: 68 caught, 17 unviable
```

**Survivors: none.** `missed.txt` and `timeout.txt` are empty. So there is no mutant left alive to
list; attempt 1's nine survivors are all caught now, including the three that changed only error
text: `lossy -> String::new()` / `"xyzzy".into()` (`commit.rs:580`, caught by `assert_slot_lost`
and the `ObligationKeyTaken` legs, which assert the key appears in the message) and the three
`Display::fmt -> Ok(Default::default())` mutants (`CommitError`, `Stop`, `UploadPartError`,
`commit.rs:356`, `:397`, `:471`; caught by `in_doubt`, `stopped`, `renewal_stopped` and the
composed-error leg, which assert the message names the upload id, the cause and the residue).
The 68 caught, by function: `commit_part` 20, `compensate` 9, `judge_session` 4, `judge_slot` 2,
`staged_elsewhere` 3, `Doubt::{observe,session_seen,cannot_land,slot_held,any,required}` 8,
`compensates_after` 2, `CommitError::slot_released` 2, `residue` 2, `lossy` 2, the two `source`
impls 2, the four `Display` impls 4 (`RecordError`'s included), `commit_batch` 1,
`compensation_batch` 1, `renewal_stopped` 1, `COMPENSATION_ROUNDS`'s `+` 2, `OwedChunks::validate
-> Ok(None)` 1. The 17 unviable are every `-> Ok(Default::default())` / `Default::default()`
replacement of a function returning a type with no `Default` (`commit_part`, `compensate`, `refuse`,
`Session::of`, `judge_session`, `PartRecord::commit`, `RetireObligation::part_chunks`, …): they
do not compile, so they are not mutants of the shipped code.

The re-read-slot-bytes mutant the brief names is not among the 85 because no function reads the
slot key into a batch builder; see *Slot bytes, by invariant* above for why no fixture could
reach one if it existed.

## Rubric self-review (AGENTS.md § Review rubric & protocol), final step

- *One clock per lifecycle*: the patch adds no clock read; the two in `commit.rs` are the
  caller's `StageClock` (`:842` the `committed_at_millis` stamp, `:885` the dispatch check), as v4
  had them. The compensation reads no clock.
- *Narrow seams / dependency direction*: `commit.rs` uses `MetadataStore` and
  `PlacementChunkStore` only; `OwedChunks` and `Session` are crate-private.
- *Metadata validation boundaries (ADR-0045)*: validation moved **to** the boundary
  (`OwedChunks::validate`, `Session::of`), out of the retry loops; stored bytes are still judged
  only by their decoders (`decode_part_record`, `decode_session_record`, `decode_slot_record`),
  and damage still stops with `Stop::Record`, never as a value.
- *No DST-reachable statics*: none added; the test keeps its state in struct fields.
- *Docs currency*: no port, API, RPC, flag or persisted field changes against v4; v4's
  architecture paragraph (`05-building-block-view.md:248`) stands.
- *Absent or unsupported entries — never silent skip*: the one `Ok(_) => {}` arm
  (`commit.rs:942`) is a documented "settles nothing", not a skip: the attempt keeps retrying or
  returns in doubt, and a prior that has gone is judged as damage at `:1014-1019`.
- *Transactions / await discipline / probes / protocol input*: not touched by this delta.
- *Test fidelity*: the five legs run against the production `commit_part`/`upload_part` over the
  real redb store; the test-local batches (`roll_back`, `repoint`) are in the exact shape of
  writers this crate cannot call, as the carry-forward allows.
- *Serialization identity*: `repoint` relies on the canonical encoding the decoder enforces and
  asserts the read-back, which is the same property `repointed_part` relies on in production.

## For the human at sign-off

- **Size**: `patch.diff` is 308 KB (v4 was 286 KB; the extra is the five legs). The carry-forward
  keeps the size backstop overridden and says not to trim tests.
- **The brief's Design text still states the refuted rule** (§ *Settle before compensating*: "it
  required a non-empty prior record, whose chunk ids no later write can carry"). The Iteration 4
  rationale overrides it and this patch follows the rationale; the brief text itself is the
  Plan's to correct, as is any wording in 0016 (`0016:1649-1651`) that a Plan may want to add
  about repair rewriting `part:` records under `Open@E` (`0016:671`).
- **Follow-up, not in this slice**: a `StagedAttempt::session_epoch()` accessor in `stage.rs`
  would remove the two remaining decodes of the attempt's own session bytes
  (`commit.rs:798`, `:804`); the brief closes `stage.rs` to this slice.
- **Ratified decisions kept**: dispatch-only lease promise; `Refused(OperationAborted)` for a
  compensated live-session loss; slots left held after a rollback and after an in-doubt return
  ("more in-doubt answers are an allowed availability cost").
- C4-verify will report UNVERIFIABLE (exit 77), as the brief pre-declares for a net-new test.
