# Build notes — issue 1030 (657.2d): a compensation in doubt is settled from reads; the own-commit proof's false `Committed`

Base: the per-cycle worktree `/home/eddie/wyrd/wyrd.pdca-wt-l2` at `db1a066` (`pdca-integrate: issue_1029`, on top of #1029's
PR head `1d1a1d0`, which folds in #1027, #1008 and #1028). The brief's citations were re-resolved on this base and matched:
the compensation's unknown arm was `crates/core/src/multipart/commit.rs:1140-1145` (`// deferred: #1030`), `failed` at
`:769-783`, the own-commit proof at `:918-925`. All `path:line` below are on the patched tree unless marked "base".

Files changed (`patch.diff`, against `HEAD` = `db1a066`):

- `crates/core/src/multipart/commit.rs` (production; +276/−83, most of it docs)
- `crates/core/tests/multipart_upload_part_commit.rs` (the named test file: legs appended, h4 compensation leg reworked,
  read-trace vocabulary extended per (K)); copy shipped in the bundle as `multipart_upload_part_commit.rs`
- `docs/design/architecture/05-building-block-view.md:250` (the living doc's part-commit paragraph)

No new public type, variant or field. `CommitError`, `InDoubt`, `Stop`, `DoubtfulBatch` are unchanged in shape. One
`Display` string changed wording: `Stop::SessionMoved` now says "the batch" for "the commit" (`commit.rs:644-645`),
since it is now also a compensation's cause; no test or caller matched the old literal.

## Repro (run first, on the base)

Both reproduce, run as the new legs against the base `commit.rs` (`git show HEAD:…`):

1. A compensation scripted `Act::unknown(true, false)` (route `renewal`, via `compensating`): the base answers
   `Err(InDoubt(InDoubt { batch: Compensation, may_still_commit: false, cause: OutcomeUnknown, .. }))` although the
   compensation applied (its obligation holds the bytes it put, its slot is gone). Seen in
   `s_a_compensation_answered_unknown_is_settled_from_reads` and the reworked h4 leg ("expected a renewal error, got
   Err(InDoubt(... batch: Compensation ...))").
2. Part 1 staged on a fresh rig, the commit `Act::hold()`, the slot key erased at gap 0 (`at_gaps` + `erased`): the base
   answers `Ok(Committed { part_number: PartNumber(1), .. })`. The held batch never landed, so there is no `part:` record.
   Seen in `d1_a_slot_erased_under_an_unlanded_commit_is_never_committed` ("held: Ok(Committed { ... })").

## What changed, and why

### The compensation's unknown outcome (`commit.rs`)

- `compensate`'s `Err` arm (`commit.rs:1325-1336`): a `CommitUnknownResult` now goes to `settle_compensation` with a
  `Doubt` for `DoubtfulBatch::Compensation`; any other error is the same definite `Stop::Store` as before. `failed`
  (base `:769-783`) is removed; nothing else used it.
- `settle_compensation` (`commit.rs:1237-1297`), mirroring `settle`: laps of the base re-read's order (its obligation key
  for a non-empty part, its slot, the session), at most `SETTLE_LAPS`, then the obligation key once more for a non-empty
  part. Each read folds into the `Doubt` as it returns, so a later fault keeps what earlier reads showed. It answers
  `Compensated::Released` (so each caller's existing landed answer: `refuse` `:1369`, `compensated_stop` `:1386`, the
  renewal arm `:1625-1633`) only on:
  - a non-empty part: `Doubt::owns` (`commit.rs:927-946`): the obligation key at exactly
    `metadata::encode(obligation.payload())`, the obligation `compensate` built for *that round*, so keyed under the
    round's epoch (after an epoch chase, `E′`);
  - an empty part: slot `Gone` and the session read after it `Same` (`commit.rs:1275-1277`). A non-empty part never
    takes this branch (`if obligation.is_none()`).
  Everything else keeps the doubt with a cause: other bytes at the obligation key → `ObligationKeyTaken` (flag kept);
  session moved → `SessionMoved`; session gone → the slot read once more (`gone_in_doubt`); slot rewritten → `SlotLost`;
  slot held or (non-empty) gone under the unchanged session with no obligation found → `OutcomeUnknown`; damage or a
  fault → `Record`/`Store`.
- `Doubt` (`commit.rs:863-947`) gains `batch: DoubtfulBatch` so `end` names the right batch; `Doubt::of` takes it.
- `gone_in_doubt` (`commit.rs:949-973`) now returns the cause (`Result<Stop, CommitError>`) through `Doubt::read`
  instead of building the error itself, so both settles share it. Behaviour for the commit is unchanged (same reads,
  same flag rule, a fault still ends in `Stop::Store` with the flag as it stood).
- Flag rules exactly as the brief's *Keeping the doubt*: falls on a session at a higher epoch (`seen_session`), and on a
  missing record only when the slot read after it is absent; kept on damage and on other bytes at the obligation key.

Why laps for the compensation at all (the brief only required the reads, not a second lap): the brief points at
`settle` as the peer to mirror, and the commit's reason for a second lap applies unchanged (a round still in flight when
the store answered can land after the first slot read). For a non-empty part the trailing obligation read already
catches that; the second lap mainly helps an empty part. Cost: at most 3 more reads (non-empty) or 2 (empty) on a held
round under an unchanged session, no batch.

Why the obligation is read first *and* last: first, so an applied compensation settles on one read (S row 1 pins
`[obligation]`); last, because a session move does not erase an obligation, so a compensation that landed after the
first obligation read and before a rollback is still proven ((T) second history; the demonstrated red "no obligation
read after the read that found the session moved" fails it).

### The own-commit proof (`commit.rs:1015-1044`)

When the commit required the part key absent and the lap's slot read found the slot gone, the lap reads the part key
again *between* that slot read and the last session read. Absent there, with the last session read `Same`, is
`InDoubt { Commit }` with `Stop::SlotLost` (the slot is gone under the live session, and no commit of this attempt
landed). A decodable record there lowers the flag, as the first part read does. Bytes that do not decode there are not
"absent" and disprove nothing.

Choices, against the brief's options:

- **Read only when it can decide (required absent, slot gone), not in every lap.** Rejected alternative: a fifth read in
  every lap. Cost, counted on this test file: every-lap changes `lap()` itself, so every exact trace comparison over a
  held lap changes too (`:2629`, `:4389`, `:4653`, `:5089`, `:5186`, `:5264`, `:5475`, `:5694` on base, 8 sites), and the
  `at_gaps` numbering moves for every existing history at the old gap 4 (after the last session read: gap 4 → 5 in
  `b_a_held_reupload_survives_a_repair…` `:4513`, `read_gap` `:4624` and `c2` `:4994`, all on base), and
  it adds a store read to every lap of a re-upload, where the read cannot decide anything (the documented limit). The
  conditional read changes 4 exact comparisons (`:2645`, `:4427`, `:4761-4775`, `:5812`) and renumbers no gap: the read
  sits inside old gap 3, and `at_gaps`' hooks are keyed by key, so a step at gap 3 still runs right after the slot read
  and a step at gap 4 still runs after the last session read (doc at test file `:4047-4052`). Production cost of the
  conditional: one `if` (`commit.rs:1021`).
- **Cause `SlotLost` for D1**, not `OutcomeUnknown`: the proof shows no commit landed, so the slot was lost to something
  other than this attempt, which is `SlotLost`'s meaning; its doc now says so (`commit.rs:513-520`). D3 (a compensation)
  uses `OutcomeUnknown` instead: there an absent obligation does *not* show the compensation did not land (the drain may
  have deleted it), so "removed by something other than this attempt" could be false.
- **Torn bytes at the new read disprove nothing.** This keeps the (K)-protected assertion in
  `r_damage_under_a_held_commit_keeps_the_doubt` ("Damage at the part key after the held commit landed: the own-commit
  proof comes first", test file `:5738-5754`), which plants `torn` at the part key after the commit landed and expects
  `Committed`. It matches the Invariant's wording: `Committed` is never answered when a later read finds the key
  *absent*.

### Docs (same PR, per the rubric's docs-currency rule)

- `commit.rs` module doc: header line `:7-9`; *A commit in doubt* (`:131-158`, the new read, and its table rows
  `:164-165`); new section *A compensation in doubt* (`:207-266`); the stops table row (`:288`); bounds (`:299-318`).
- Item docs: `SETTLE_LAPS` (`:397-400`), `CommitError::InDoubt` (`:445-449`), `slot_released` (`:463-468`),
  `InDoubt::may_still_commit` (`:487-490`), `Stop::SlotLost` (`:513-520`), `Stop::ObligationKeyTaken` (`:528-531`),
  `Stop::SessionMoved` (`:543-548`), `Stop::OutcomeUnknown` (`:549-555`), `Compensated::Released` (`:1228-1231`),
  `compensate` (`:1299-1305`).
- `docs/design/architecture/05-building-block-view.md:250`: replaced "A compensation with an unknown outcome still stops
  in doubt with no further batch; settling it is still to land." with the new rule, and added the own-commit proof's part
  read.

## Tests (`crates/core/tests/multipart_upload_part_commit.rs`)

Appended (section `// #1030: helpers`, test file `:5923` on):

| Brief item | Test (line) |
|---|---|
| (S) | `s_a_compensation_answered_unknown_is_settled_from_reads` (`:6087`): row 1 on renewal/lease/ceiling/rival × both flags × non-empty/empty (ceiling skipped for empty); rows 2–4 on the rival route, both columns. Exact read traces pinned. |
| (X3) | `x3_late_staging_that_rebuilds_a_compensated_slot_never_sends_it_again` (`:6202`): ceiling non-empty, rival non-empty, rival empty × both flags. |
| (O) | `o_an_abort_after_a_landed_compensation_keeps_its_landed_answer` (`:6291`), `o_a_compensation_under_a_newer_epoch_is_settled_by_its_obligation_under_that_epoch` (`:6322`, direct and chase), `o_other_bytes_at_the_obligation_key_are_never_the_landed_answer` (`:6371`). |
| (T) | `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` (`:6445`), `t_a_compensation_landing_before_a_rollback_is_settled_only_by_its_obligation` (`:6482`), `t_a_missing_session_behind_a_held_compensation_is_judged_by_a_later_slot_read` (`:6536`, erased-only and terminal delete, both columns). |
| (F) | `f_a_fault_or_damage_under_a_held_compensation_keeps_the_doubt` (`:6612`): a fault at each read of the lap, the slot read behind a missing session, the trailing obligation read, a torn slot, a rewritten slot, a torn session; plus a fault after the session moved (flag `false`). |
| (D) | `d1_…` (`:6760`), `d2_…` (`:6822`, gaps 1 and 2, plus a re-upload that reads the key once), `d3_…` (`:6887`). |
| (K) new read | `k_the_own_commit_proofs_part_read_keeps_the_coverage_of_the_reads_before_it` (`:6938`): rival and rollback at gaps 3 and 4 of the proof lap; a fault at the new read (gap 3, flag kept `true`); a fault at the last session read after the new read found a record (gap 4, flag `false`). |

Every new leg asserts no batch after the one answered unknown (`assert_nothing_after`, `:6030`) and ends with
`assert_every_fragment_named` (directly or through `assert_compensated` / `assert_finally_compensated`).

Reworked: `h4_a_compensation_answered_unknown_is_in_doubt_with_no_further_batch` → renamed
`h4_a_compensation_answered_unknown_sends_no_further_batch` (`:2774`), since its applied runs now answer the landed
answer; its not-applied and held runs keep their assertions; it gained `assert_every_fragment_named`.

(K) vocabulary changes, only to add the new read:
`gap_chain` (`:4333`, the old `at_gaps` body, unchanged), `at_gaps` now calls it (`:4053`); `proof_lap` (`:4369`),
`laps_ending_gone` (`:4376`); exact traces updated at `:2645` (h4 commit, applied runs), `:4427`
(`b_a_landed_commit_answered_unknown_is_committed`), `:4761-4775` (`read_gap`: landed runs end with a proof lap, except
landed-then-rollback at gap 3, whose lap found the slot held), `:5812` (`r_a_missing_session_behind_a_held_commit…`).
No history moved; no other assertion changed. `compensating` now delegates to `compensating_part` (`:2698-2730`) with the
same body for its three routes.

Test-only additions worth a reviewer's eye:

- `Rig::reclaimed` + `Fleet::forget`: before (T)'s terminal delete of a non-empty part, the session's staged fragments
  are removed from the fleet, as the retirement drain reclaims them before a terminal delete. Without it the C-1 check
  could not end those runs (the test-local `terminal_delete` takes the owned entries that name those chunks); #1028's and
  #1029's terminal-delete legs simply skip the C-1 check.
- (O)'s other-bytes leg erases the planted bytes before the C-1 check, because `Rig::obligations` decodes every
  `retire:bytes:` value and the planted `other` does not decode.

### Base vs fix

- Fixed tree: `cargo test -p wyrd-core --test multipart_upload_part_commit` → 90 passed, 0 failed.
- Base `commit.rs`, same tests: 72 passed, 18 failed: `s_…`, `x3_…`, all three `o_…`, all three `t_…` (new), `f_…`,
  `d1_…`, `d2_…`, `k_…`, the reworked h4 compensation leg, and the four existing tests whose traces (K) extended
  (`h4_a_commit…`, `b_a_landed…`, `b_read_gap_landed_then_a_rival…`, `b_read_gap_landed_then_a_rollback…`,
  `r_a_missing_session_behind_a_held_commit…`).

Differences from the brief's "green on the base" list, each explained:

- `d2_…` and `k_…` are red on the base **only on their read-trace assertions** (they pin the new read, which the base
  does not make); their outcome assertions hold on the base. Their binding reds are the targeted edits below.
- `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` ((T)'s first history) is red on the base:
  the base answers the held compensation with `may_still_commit: true` (the store's report) and `OutcomeUnknown`, while
  the brief's own expected answer is `false` / `SessionMoved`. So the brief's "green on the base" for this history does
  not hold; the expected answer is the brief's.
- `d3_…`: green on the base, as the brief says.
- Rows 2 and 4 of (S) and (X3)'s empty run are inside the same test functions as red rows, so they show as part of red
  tests on the base; their own assertions pass on the base.

### Demonstrated reds (required)

Each is a temporary edit to the fixed `commit.rs`, applied by a scratch script, run on the named legs, then restored
(`cmp` against the saved fixed copy after every run). All re-run on the final test file.

| Temporary edit | Named leg(s) | Result |
|---|---|---|
| a compensation in doubt sent again (attempt 7's resend: `continue` the round loop on `InDoubt`) | (X3), (S) one-batch | both red: "rival, 0 bytes, may false: exactly one compensation batch"; "not applied, 65 bytes: batches after the Compensation answered unknown: [Compensation]" |
| a slot held at the bytes it required taken as "did not land" (held under `Same` → send again) | (X3) empty run, (S) row 2 | both red, same two messages (X3's failing run is the empty rival run; S's is row 2) |
| the slot-gone proof accepted for a non-empty compensation | D3 | red: "slot erased: Ok(Refused(OperationAborted))" |
| the obligation read keyed under the staged (commit's) epoch instead of the round's | (O) rollback leg | red: `InDoubt` where `OperationAborted` expected |
| no obligation read after a read that found the session moved | (T) second history, non-empty | red: `InDoubt` where `OperationAborted` expected |
| the session check after the slot read made always true (empty part's proof on `(_, Gone)`) | (T) first history | red: "expected InDoubt, got Ok(Refused(OperationAborted))" |
| the flag lowered on other bytes at the obligation key | (O) held run | red: "held: the flag" |
| the flag lowered on a missing session record with no slot read after it | (T) erased-session runs | red: "terminal delete false, 64 bytes: the flag" |
| that slot read skipped (`Gone` → `SessionMoved` directly), the flag kept | (T) terminal-delete runs | red on the erased-session run first ("SessionMoved" cause); with the loop narrowed to the terminal-delete runs: "terminal delete true, 64 bytes: the flag" |
| each doubt-keeping return replaced with a plain stop: (a) a read fault (`Doubt::read`, compensation only), (b) slot damage, (c) session damage | (F) | every run red, each checked alone (temporary run filter): (a) all 9 fault runs, both columns; (b) torn slot, both columns; (c) torn session, both columns |
| the part-key check removed (the read kept, its absence ignored) | D1 | red: "held: Ok(Committed { .. })" |
| the part-key check made on the lap's first part read (the later read kept, unused) | D2, `b_read_gap_landed_then_a_rival_is_committed` gaps 1 and 2 | both red at gap 1; with both loops narrowed to gap 2 only, both red at gap 2 too |

Survivor checks I added after a first pass (each now killed): `laps <= SETTLE_LAPS` and dropping `doubt.may_land` in the
compensation's lap condition (killed by S rows 4 and 2's pinned traces); dropping the `Rewritten` arm (killed by (F)'s
rewritten-slot run); dropping the flag fold on the new part read (killed by (K)'s gap-4 fault run); reading the key again
for a re-upload too (killed by D2's re-upload run).

### Refute-your-own-test answers

- **(a) Genuine red?** Yes. With `commit.rs` reverted to the base, 18 tests fail (listed above), including D1 on
  `Ok(Committed)` and every compensation leg on the unread `InDoubt`. Each brief-named mutation goes red on its named leg
  (table above).
- **(b) Production path?** Yes. Every leg calls the production `commit_part` (through `Rig::commit` or `upload_part`)
  over a real `RedbMetadataStore`; the wrapper only scripts store answers (unknown, held, faults) and runs real writers
  (real Abort, `commit_part` rivals, `stage_part` staging rounds released from hold). No stand-in of
  `settle`/`settle_compensation`.
- **(c) Fixture includes the fault?** Yes. The failing element is in every fixture: compensations really applied
  (`Act::unknown(true, ..)`) or really held and landed at the gap under test; the slot really erased at the gap; real
  late staging batches released after the compensation (X3) and checked to rebuild the exact bytes; real rollbacks,
  Aborts, teardowns and terminal deletes; faults injected at the exact read through the wrapper.

## Gates and commit-readiness

- `cargo fmt -p wyrd-core` run over both touched Rust files; `cargo clippy -p wyrd-core --all-targets -- -D warnings`
  clean.
- `cargo xtask ci` through `engine/xtask.sh` with `PDCA_WORKTREE` set: see the result line appended at the end of this
  file. A first run was stopped by me after it had already reported one clippy `needless_borrow` in my (O) test
  (`&at` on a `&str`); fixed, then re-run on the final tree.

## C5 survivors

From my own mutant checks over the changed production lines (the brief's eleven edits, plus the five above), I know of
none that survives. Two classes I did not test and expect `cargo-mutants` may report:

- `impl Display for Stop` returning `Ok(())` without writing: `in_doubt`/`stopped` check that the error text *contains*
  `cause.to_string()`, which an empty string always satisfies. Pre-existing pattern from #1027-#1029, not introduced
  here; the one wording change (`SessionMoved`) is covered only that way.
- Doc-comment-only lines: no mutants.

## Out of scope, not done

Per the brief: no resend of a compensation; no "did not land" proof; no change to staging, record shapes, the drain, the
reaper, Complete, DST (`// deferred: #660` marker added in the test file's #1030 helpers section, as #1028/#1029 did); no
production caller.

## `cargo xtask ci` result (final tree)

`./engine/xtask.sh ci` (so `cargo xtask ci` in `$PDCA_WORKTREE`), on the tree `patch.diff` describes: **exit 0, "xtask ci:
all checks passed"**. Every step ran: `typos`, `lint_docs`, `render_site --check` (link audit OK), the gitlink, unsafe
and blackbox guards, `cargo fmt --check`, workspace `clippy --all-targets`, build, `cargo test --workspace --exclude
wyrd-dst` (this file: 90 passed), `cargo-machete`, the three `cargo deny` checks, conformance vectors, `statics`,
`deploy-guard`, and the `wyrd-dst` clippy and tests under `--cfg madsim` (101 passed). 228 test binaries reported `ok`,
none `FAILED`. The brief's external dependencies (`typos`, the docs renderer) are installed here and ran; no
NEEDS-HUMAN dependency to declare.
