# Build notes — issue 1030 (657.2d), iteration 3: a compensation in doubt is settled from reads; the own-commit proof's false `Committed`

Base: the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt-l2` at `db1a066` (`pdca-integrate: issue_1029`, on #1029's PR
head `1d1a1d0`, which folds in #1027, #1008 and #1028). `path:line` below is on the patched tree unless marked "base".

Files changed (`patch.diff`, against `HEAD` = `db1a066`):

- `crates/core/src/multipart/commit.rs` (production; most of the change is docs)
- `crates/core/tests/multipart_upload_part_commit.rs` (the named test file; copy shipped in the bundle as
  `multipart_upload_part_commit.rs`)
- `docs/design/architecture/05-building-block-view.md:250` (the living doc's part-commit paragraph)

No new public type, variant or field. `CommitError`, `InDoubt`, `Stop`, `DoubtfulBatch` keep their shape. One `Display`
string changed wording: `Stop::SessionMoved` says "the batch" for "the commit" (`commit.rs:645-648`), since it is now also
a compensation's cause; no test or caller matched the old text.

## Iteration 3: what the carry-forward asked, and what I did

I started from iteration 2's patch (it applied cleanly on `db1a066`) and changed only what the sign-off named. The design
stands, as the human said.

1. **The false success at the own-commit proof's part read.** Iteration 2 treated bytes that do not decode at the part
   key, read after the slot read, as "something there" (`Some(_) => {}`), so a first upload whose commit never landed
   answered `Committed` once its slot was erased and `torn` planted at the part key. Now that read decodes what it finds
   (`commit.rs:1023-1038`):
   - absent → the cause `Stop::SlotLost` (as before);
   - a record that decodes → the flag falls (as before);
   - bytes that do not decode → the cause `Stop::Record(err)`, **the flag untouched**.

   The proof answers `Committed` only when that read found a record (`commit.rs:1045-1052`): under `(Same, Gone)` with
   a cause recorded, it returns `InDoubt` with that cause. The reads still continue to the last session read after torn
   bytes (they don't stop there), which is what the carry-forward's fault-at-gap-4 case requires (the fault, not the
   damage, is its cause). When the proof does not apply (the session moved, gone or damaged), the part read's finding is
   not used, and the existing flow picks the cause, as before. I folded the two outcomes into one local
   (`disproof: Option<Stop>`, `:1023`) in place of iteration 2's `no_record: bool`, so the arm that sets the cause cannot
   also lower the flag by accident.

2. **The C5 survivor at iteration 2's `commit.rs:1025`** (the guard `decode_part_record(&found).is_ok()` replaced with
   `true`). That guard is gone in this shape. Its two analogues on the new code are both killed (E13 and E14 below) by
   the new (K) case the carry-forward asked for, plus the new D1 regression.

3. **The adversary's reproducer as a regression:** `d1_torn_bytes_at_the_part_key_under_an_unlanded_commit_are_never_committed`
   (test file `:6845`).

4. **Human decisions recorded at sign-off** (the no-resend trade-off accepted; plan-review findings covered; do not
   split): followed. No resend anywhere, and the patch is still one bundle.

5. **"Make sure the next Check gets a real high-effort review."** Not something the builder can do. Flagged here for the
   human: the brief's *Review depth* line still applies to this patch.

### One existing assertion this forced me to change (outside (K)'s list)

The brief's (K) says the h4 compensation leg is the only existing test whose outcome changes. The carry-forward's fix
makes a second one change, and I could not avoid it:

- `r_damage_under_a_held_commit_keeps_the_doubt`, its block "Damage at the part key after the held commit landed: the
  own-commit proof comes first" (base test file `:5619-5634`, now `:5738-5761`). The held first-upload commit lands,
  then `torn` is planted at the part key, then the settle runs. The base (#1029) and iteration 2 answered `Committed`.
- The adversary's history (commit never lands; at gap 0 the slot is erased and `torn` planted at the part key) gives
  **the exact same reads**: part key `torn`, session `Same`, slot absent, part key `torn`, session `Same`. The settle
  reads nothing else. So no read-only settle can answer `Committed` to one and not the other. The human chose "never
  `Committed`" for torn bytes at that read. So this block now expects `InDoubt { Commit, may_still_commit: true }`
  with the part-key record stop (`assert_torn_part`), and keeps its other assertions (no compensation, released once).
  Its comment explains why and names the D1 test with the same reads.
- The base's own module doc stated the old rule ("`Committed`, whatever the part key holds", base `commit.rs:136`), so
  this is a real behaviour change from #1029 for one damage-only case. It only affects a first upload with bytes at
  its part key that do not decode, and the cost is availability (an in-doubt answer where the commit did land), which
  0016's refutation standard allows.

Rejected alternative: tell the two histories apart by also reading the commit's `psum:` summary key (a landed commit
puts it in the same batch, `commit.rs:1196-1199`). Two shapes, both rejected:

- In every proof lap: one more read per lap, changing `proof_lap` and every trace and gap history built from it (10
  call sites: test file `:2646`, `:4427`, `:4767`, `:5820`, `:5953` (`at_proof_gaps`), `:6812`, `:6886`, `:6939`,
  `:7054`, `:7129`).
- Only when the proof read found torn bytes: one extra read in that case only, 3 trace sites (`d1_torn_…`, `k_torn_…`,
  the `r_damage_…` block). Cheaper, and it would give the `r_damage_…` block back its `Committed`.

Both add an evidence rule the plan never reviewed (a `psum:` record standing in for a `part:` record; a rival's or a
planted `psum:` would pass it just as a rival's `part:` record passes today), and the human's instruction named the
answer outright ("keep the doubt with `Stop::Record(err)` and leave `may_still_commit` unchanged"). The cost of not
doing it is availability under damage only. If the human wants the conditional read, it is a small follow-up.

Still a documented limit, unchanged: a **re-upload** (the commit required the prior's bytes) makes no second part read,
so torn bytes at its part key with its slot erased still answer `Committed` (module doc `commit.rs:155-157`; the brief's
*Scope* puts a re-upload with its slot erased out of scope).

## Repro (the brief's two, run first on the base)

Both still reproduce on the base `commit.rs` (`git show HEAD:…`, the whole test file run against it):

1. A compensation answered unknown although it applied: on the base, `s_a_compensation_answered_unknown_is_settled_from_reads`
   and the reworked `h4_a_compensation_answered_unknown_sends_no_further_batch` fail (the base answers
   `InDoubt { batch: Compensation, cause: OutcomeUnknown }` unread, base `commit.rs:1143`, `failed` at base `:772`).
2. Part 1 staged, commit `Act::hold()`, slot erased at gap 0: on the base,
   `d1_a_slot_erased_under_an_unlanded_commit_is_never_committed` fails with
   `held: Ok(Committed { part_number: PartNumber(1), .. })`, no `part:` record.

And the adversary's reproducer, on iteration 2's `commit.rs`: `d1_torn_bytes_…` fails with
`torn at gap 0, held: Ok(Committed { part_number: PartNumber(1), .. })`.

## What the patch does (all iterations)

### The compensation's unknown outcome (`commit.rs`)

- `compensate`'s `Err` arm (`commit.rs:1334-1345`): a `CommitUnknownResult` goes to `settle_compensation` with a `Doubt`
  for `DoubtfulBatch::Compensation` (`:1339`); any other error is the same definite `Stop::Store` as before. `failed`
  (base `:772-783`) is removed; nothing else used it.
- `settle_compensation` (`commit.rs:1254-1306`), mirroring `settle`: laps of the base re-read's order (obligation key for
  a non-empty part, slot, session), at most `SETTLE_LAPS`, then the obligation key once more for a non-empty part. Each
  read is folded into the `Doubt` as it returns. It answers `Compensated::Released`, so each caller's existing landed
  answer (`refuse` `:1378`, `compensated_stop` `:1395`, the renewal arm), only on:
  - a non-empty part: `Doubt::owns` (`commit.rs:936-947`): the obligation key at exactly the bytes `compensate` built
    for *that round*, so keyed under the round's epoch (after an epoch chase, `E′`);
  - an empty part: slot `Gone` and the session read after it `Same` (`:1284-1286`). A non-empty part never takes this
    branch.
  Everything else keeps the doubt with a cause: other bytes at the obligation key → `ObligationKeyTaken` (flag kept);
  session moved → `SessionMoved`; session gone → the slot read once more (`gone_in_doubt`); slot rewritten → `SlotLost`;
  slot held (or, non-empty, gone) under the unchanged session with no obligation found → `OutcomeUnknown`; damage or a
  fault → `Record` / `Store`.
- `Doubt` (`commit.rs:868-948`) gains `batch: DoubtfulBatch`; `Doubt::of` takes it.
- `gone_in_doubt` (`commit.rs:957-975`) returns the cause through `Doubt::read`, so both settles share it. Same reads and
  flag rule for the commit as before.
- Flag rules as the brief's *Keeping the doubt*: falls on a session at a higher epoch (`seen_session`), and on a missing
  record only when the slot read after it finds the slot absent; kept on damage and on other bytes at the obligation key.

Why laps for the compensation: the brief points at `settle` as the peer, and the commit's reason for a second lap
applies (a round still in flight can land after the first slot read). Cost: at most 3 more reads (non-empty) or 2
(empty) on a held round under an unchanged session, no batch.

Why the obligation is read first and last: first, so an applied compensation settles on one read (S row 1 pins
`[obligation]`); last, because a session move does not erase an obligation, so a compensation that landed after the
first obligation read and before a rollback is still proven ((T)'s second history; demonstrated red E5).

### The own-commit proof (`commit.rs:1016-1052`)

When the commit required the part key absent and the lap's slot read found the slot gone, the lap reads the part key
again between that slot read and the last session read. With the last session read `Same`:

- absent → `InDoubt { Commit }`, `Stop::SlotLost` (no commit of this attempt landed; `SlotLost`'s doc says so,
  `commit.rs:516-523`);
- bytes that do not decode → `InDoubt { Commit }`, `Stop::Record`, flag untouched (iteration 3);
- a record → `Committed`, the flag already lowered by that read.

Choices:

- **Read only when it can decide (required absent, slot gone), not in every lap.** An every-lap read would change `lap()`
  itself, so every exact trace over a held lap (8 base sites) and the gap numbering of every history at the old gap 4
  (3 base sites), and it would add a read to every re-upload lap where it cannot decide anything. The conditional read
  changes 4 exact comparisons and renumbers no gap: it sits inside old gap 3 (test file doc `:4047-4052`).
- **`SlotLost` for D1**, `OutcomeUnknown` for D3: for the commit, the absent key shows no commit landed; for a
  compensation, an absent obligation does not show it did not land (the drain may have deleted it).

### Docs (same PR, per the rubric's docs-currency rule)

- `commit.rs` module doc: header `:7-9`; *A commit in doubt*, including the part read and its torn-bytes rule
  (`:131-160`) and table rows (`:165-167`); new section *A compensation in doubt* (`:209-268`); stops table row
  (`:290`); bounds (`:301-320`).
- Item docs: `SETTLE_LAPS` (`:399-403`), `CommitError::InDoubt` (`:447-451`), `slot_released` (`:465-470`),
  `InDoubt::may_still_commit` (`:488-492`), `Stop::SlotLost` (`:516-523`), `Stop::ObligationKeyTaken` (`:528-533`),
  `Stop::SessionMoved` (`:545-550`), `Stop::OutcomeUnknown` (`:551-557`), `settle` (`:977-982`), `Compensated::Released`,
  `compensate`.
- `docs/design/architecture/05-building-block-view.md:250`: the old "settling it is still to land" sentence replaced
  with the compensation rule, plus the own-commit proof's part read, now saying that the key must hold a part record and
  that bytes that do not decode prove no landed commit either.

## Tests (`crates/core/tests/multipart_upload_part_commit.rs`)

| Brief item | Test (line) |
|---|---|
| (S) | `s_a_compensation_answered_unknown_is_settled_from_reads` (`:6109`): row 1 on renewal/lease/ceiling/rival × both flags × non-empty/empty (ceiling skipped for empty); rows 2-4 on the rival route, both columns. |
| (X3) | `x3_late_staging_that_rebuilds_a_compensated_slot_never_sends_it_again` (`:6224`) |
| (O) | `o_an_abort_after_a_landed_compensation_keeps_its_landed_answer` (`:6313`), `o_a_compensation_under_a_newer_epoch_is_settled_by_its_obligation_under_that_epoch` (`:6344`), `o_other_bytes_at_the_obligation_key_are_never_the_landed_answer` (`:6393`) |
| (T) | `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` (`:6467`), `t_a_compensation_landing_before_a_rollback_is_settled_only_by_its_obligation` (`:6504`), `t_a_missing_session_behind_a_held_compensation_is_judged_by_a_later_slot_read` (`:6558`) |
| (F) | `f_a_fault_or_damage_under_a_held_compensation_keeps_the_doubt` (`:6634`) |
| (D) | `d1_a_slot_erased_under_an_unlanded_commit_is_never_committed` (`:6782`), **new:** `d1_torn_bytes_at_the_part_key_under_an_unlanded_commit_are_never_committed` (`:6845`), `d2_…` (`:6924`), `d3_…` (`:6989`) |
| (K) new read | `k_the_own_commit_proofs_part_read_keeps_the_coverage_of_the_reads_before_it` (`:7040`), **new:** `k_torn_bytes_at_the_proofs_part_read_keep_the_flag_through_a_later_fault` (`:7123`) |

New in iteration 3:

- `d1_torn_bytes_…` (`:6845`): a held commit, or `Act::unknown(false, false)` never applied; slot erased at gap 0;
  `torn` at the part key at gap 0, or at proof gap 3 (both of the adversary's histories). Asserts: never `Committed`;
  `InDoubt { Commit }` with the flag kept (`true` held, `false` otherwise); cause the `part:` record stop; the reads are
  exactly `proof_lap`; one commit batch, no compensation, nothing after it; the torn bytes still in place; the held
  batch, released afterwards, answers `Conflict`; nothing released. Then the torn bytes are erased (the C-1 helper
  decodes every `part:` value, test file `:1408-1412`) and `assert_every_fragment_named` runs.
- `k_torn_bytes_…` (`:7123`): the carry-forward's case exactly: held commit; slot erased at proof gap 0; `torn` at gap 3;
  every session read fails from gap 4. Asserts `InDoubt { Commit, may_still_commit: true }` with the store fault as its
  cause, the reads `proof_lap[..4]`, one commit batch, nothing after it. Then repairs the slot to the bytes the commit
  required, erases the torn bytes, releases the held batch: it lands (`Committed`), the attempt's own `part:` record,
  released once, no slot left, `assert_every_fragment_named`.
- `assert_torn_part` (`:6084`): `Stop::Record(MalformedRecordValue { namespace: "part:", .. })`.
- The changed `r_damage_…` block (`:5738-5761`), above.

Carried from iteration 2: the h4 compensation leg reworked and renamed `h4_a_compensation_answered_unknown_sends_no_further_batch`
(`:2774`); the (K) trace vocabulary (`gap_chain` `:4333`, `at_gaps` `:4053`, `proof_lap` `:4369`, `laps_ending_gone`
`:4376`, `at_proof_gaps` `:5952`) and the 4 exact traces it updated; `compensating` (`:2700`) delegating to `compensating_part`
(`:2707`); `Rig::reclaimed` (`:5973`) and `Fleet::forget` (`:5939`), so (T)'s terminal-delete runs can still end
with the C-1 check.

Every new leg asserts no batch after the one answered unknown (`assert_nothing_after`) and ends with
`assert_every_fragment_named`, directly or through `assert_compensated` / `assert_finally_compensated`. The one
exception is the changed `r_damage_…` block, which (like the base block) leaves torn bytes at the part key and never
ran the C-1 check.

### Base vs fix (whole test file, 92 tests)

| `commit.rs` | Result |
|---|---|
| fixed (this patch) | 92 passed, 0 failed |
| iteration 2 | 90 passed, 2 failed: `d1_torn_bytes_…` (`torn at gap 0, held: Ok(Committed {..})`) and `r_damage_under_a_held_commit_keeps_the_doubt` (`expected InDoubt, got Ok(Committed {..})`) |
| base (`db1a066`) | 71 passed, 21 failed: `s_…`, `x3_…`, the three new `o_…`, the three new `t_…`, `f_…`, `d1_a_slot_erased…`, `d1_torn…`, `d2_…`, both `k_…`, `r_damage…`, the reworked h4 compensation leg, and five existing tests whose exact traces (K) extended (`h4_a_commit…`, `b_a_landed…`, `b_read_gap_landed_then_a_rival…`, `b_read_gap_landed_then_a_rollback…`, `r_a_missing_session_behind_a_held_commit…`) |

Differences from the brief's "green on the base" list: `d2_…` and both `k_…` tests are red on the base only on their
read-trace assertions (they pin the new read); `k_torn_…` is red on the base only because the base makes no proof read
(its trace is shorter). Their binding reds are the targeted edits below. `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt`
((T)'s first history) is red on the base: the base keeps the store's `may_still_commit: true` with `OutcomeUnknown`,
while the brief's own expected answer is `false` / `SessionMoved`. `k_torn_…` is **green on iteration 2's code**: that
code also kept the flag on torn bytes. It is a guard against the wrong fix (E13, E14), as the carry-forward asked.

### Demonstrated reds (required), all re-run on this iteration's final tree

Each is a temporary edit to the fixed `commit.rs` made by a scratch script: write the edit, run
`cargo test -p wyrd-core --test multipart_upload_part_commit -- <named legs>` under `timeout`, restore the saved fixed
copy, and `cmp` it (and the test file) byte for byte after every run. All restores matched.

| Temporary edit | Named leg(s) | Result |
|---|---|---|
| E1 a compensation answered unknown sent again (attempt 7's resend: `continue` on unknown while rounds are left) | (X3), (S) one-batch | both red: "ceiling, 64 bytes, may false: exactly one compensation batch"; (S) caught by the wrapper's slot guard on the second batch |
| E2 a slot held at the bytes it required taken as "did not land" (`OutcomeUnknown` → send again) | (X3) empty run, (S) row 2 | both red: "rival, 0 bytes, may false: exactly one compensation batch"; "not applied, 65 bytes: batches after the Compensation answered unknown: [Compensation]" |
| E3 the slot-gone proof accepted for a non-empty compensation | D3 | red: "slot erased: Ok(Refused(OperationAborted))" |
| E4 the obligation read keyed under the staged (commit's) epoch | (O) rollback leg | red: `InDoubt { .. cause: OutcomeUnknown }` where the landed answer was expected |
| E5 no obligation read after the read that found the session moved | (T) second history, non-empty | red: `InDoubt { .. cause: SessionMoved }` where the landed answer was expected |
| E6 the session check after the slot read made always true (`(_, Gone)`) | (T) first history | red: "expected InDoubt, got Ok(Refused(OperationAborted))" |
| E7 the flag lowered on other bytes at the obligation key | (O) held run | red: "held: the flag" |
| E8 the flag lowered on a missing session record with no slot read after it | (T) erased-session runs | red: "terminal delete false, 64 bytes: the flag" (`deleted = false` is the erased-session run) |
| E9 that slot read skipped (`Gone` → `SessionMoved`), flag kept | (T) terminal-delete runs | red on the erased-session run first (cause); with the loop narrowed to `deleted = true`: "terminal delete true, 64 bytes: the flag" |
| E10 each doubt-keeping return made a plain stop: (a) a read fault (compensation only), (b) slot damage, (c) session damage | (F) | each red alone: (a) "a fault at read 0 of the lap, 64 bytes: Err(Stopped { .. Store(..) })"; (b) "a torn slot, 64 bytes: Err(Stopped { .. })"; (c) "a torn session, 64 bytes: Err(Stopped { .. })" |
| E11 the part-key check removed (`Ok(None) => {}`) | D1 | red: "held: Ok(Committed { .. })" |
| E12 the part-key check made on the lap's first part read | D2, `b_read_gap_landed_then_a_rival_is_committed` | both red at gap 1; with both loops narrowed to gap 2, both red at gap 2 (`InDoubt { .. cause: SlotLost }`) |
| **E13** (new) torn bytes at the proof read lower the flag (`cannot_land()` added to the torn arm) | `k_torn_…`, `d1_torn_…` | both red: "torn at gap 3, a session fault from gap 4: torn bytes are no record, so the flag is kept"; "torn at gap 0, held: the flag kept" |
| **E14** (new) torn bytes at the proof read taken as a record (iteration 2's guard mutated to `true`) | `k_torn_…`, `d1_torn_…`, `r_damage…` | all three red: the flag; `Ok(Committed { .. })`; "expected InDoubt, got Ok(Committed { .. })" |
| **E15** (new) torn bytes at the proof read disprove nothing (iteration 2's behaviour) | `d1_torn_…`, `r_damage…` | both red on `Ok(Committed { .. })`; `k_torn_…` green, as expected (it pins the flag, which this keeps) |

### Refute-your-own-test answers

- **(a) Genuine red?** Yes. With this iteration's production change reverted (iteration 2's `commit.rs`), `d1_torn_…`
  and the changed `r_damage_…` block fail on a false `Ok(Committed)`. With the base `commit.rs`, 21 tests fail,
  D1 among them on `Ok(Committed)` and every compensation leg on the unread `InDoubt`. The new `k_torn_…` goes red on
  both wrong fixes that touch the flag (E13, E14). Every brief-named edit goes red on its named leg (table above).
- **(b) Production path?** Yes. Every leg calls the production `commit_part` (via `Rig::commit` or `upload_part`)
  over a real `RedbMetadataStore`. The wrapper only scripts store answers (unknown, held, read faults) and runs real
  writers (Abort, `commit_part` rivals, `stage_part` staging rounds released from hold, plants and erasures on the
  inner store). Nothing stands in for `settle` / `settle_compensation`.
- **(c) Fixture includes the fault?** Yes. The failing element is in each fixture: the slot really erased and the torn
  bytes really planted at the exact gap (`at_proof_gaps` hooks on the real read order); the held batch really held and
  then released to show it cannot land (`Conflict`) or does land after repair (`Committed`); the read fault injected
  at the exact session read; compensations really applied (`Act::unknown(true, ..)`) or really held; real rollbacks,
  Aborts, teardowns and terminal deletes.

## C5 survivors

- Iteration 2's one MISSED mutant (`commit.rs:1025` guard → `true`) no longer exists in that form. Its analogues on the
  new code are killed: E14 (torn taken as a record) and E13 (the flag lowered on torn bytes). I did not run
  `cargo mutants` itself; the Check gate will.
- What I expect may still survive, unchanged from iteration 2: `impl Display for Stop` returning `Ok(())` without
  writing. `in_doubt` / `stopped` only check that the error text *contains* `cause.to_string()`, which an empty string
  always satisfies. This is a pre-existing pattern from #1027-#1029, not introduced here. The one wording change
  (`SessionMoved`) is covered only that way.

## Gates and commit-readiness

- `cargo fmt --all -- --check`: clean on the final tree.
- How I ran the red→green: the project has no single-test entry point in `cargo xtask`. So I ran the brief's named suite
  (`cargo test -p wyrd-core --test multipart_upload_part_commit`) under `timeout` for the quick checks, and the full
  `cargo xtask ci` through `engine/xtask.sh` (with `PDCA_WORKTREE` set) for the gate. Its result is at the end of this
  file.

## Out of scope, not done

Per the brief: no resend of a compensation, no "did not land" proof, no change to staging, record shapes, the drain,
the reaper, Complete or DST (`// deferred: #660` marker in the test file's #1030 helpers section, test file
`:5933-5935`), no production caller. No external dependency was missing: `typos` and the docs renderer are installed and
ran inside `cargo xtask ci`.

## `cargo xtask ci` result (final tree)

`./engine/xtask.sh ci` with `PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt-l2` (so `cargo xtask ci` in the worktree), on
the exact tree `patch.diff` describes: **exit 0, "xtask ci: all checks passed"**. Every step ran: `typos`, `lint_docs`
(OK), `render_site --check` (101 pages, link audit OK), the gitlink, unsafe and blackbox guards, `cargo fmt --all --
--check`, workspace `clippy --all-targets`, `cargo test --workspace --exclude wyrd-dst` (this file: 92 passed, 0
failed), `cargo-machete`, the three `cargo deny` checks, conformance vectors (5 valid + 6 invalid), `statics`,
`deploy-guard`, and the `wyrd-dst` clippy and tests under `--cfg madsim`. 228 `test result: ok` lines, 0 `FAILED`.

`patch.diff` was written with `git diff HEAD` after that run and checked with `git apply --check -R` against the
worktree; the bundle's `multipart_upload_part_commit.rs` is byte-identical to the worktree's.
