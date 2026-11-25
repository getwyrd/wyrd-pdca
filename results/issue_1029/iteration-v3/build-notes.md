# Build notes — #1029 (657.2c), iteration 3: a commit answered unknown sends nothing more

Target: the stack base `pdca-integration/main-r9dd123c9ed70` at `1f6d6a4` (the worktree's `HEAD`),
which folds in #1027 and #1028. `patch.diff` is `git diff HEAD` on that tree. All `path:line`
citations below are on the patched tree unless marked "base".

## What the patch does

After a commit round is answered `CommitUnknownResult`, the attempt sends **no further batch**:
not the commit again, and not a compensation. It reads instead, and answers `Committed` only on
the own-commit proof; anything else is `CommitError::InDoubt`, which now carries a `cause`.

**Production (`crates/core/src/multipart/commit.rs`):**

- `commit_rounds`, the unknown-outcome arm (`commit.rs:1357-1366`, was the base's
  `// deferred: #1029` arm at base `commit.rs:1065-1066`): a `CommitUnknownResult` goes to
  `settle`; any other error is a plain `Stopped(Store)` as before. Nothing after it can reach
  `store.commit`, `compensate`, `refuse` or `compensated_stop`.
- `settle` (`commit.rs:889-959`): at most `SETTLE_LAPS` laps of the same four reads, no batch,
  no clock. The own-commit proof is checked first (`commit.rs:928-933`): slot gone (absent, or
  another `attempt_id`) **and** the session read made after that slot read is still the exact
  `Open@E` bytes. Then the part-key damage, then slot/session damage, then the table: session gone
  → `gone_in_doubt`; slot rewritten → `SlotLost`; session moved → `SessionMoved`; prior gone under
  `Open@E` → `PartRecordMissing`; slot held under `Open@E` → one more lap while the round may still
  land and a lap is left (`commit.rs:955`), else `OutcomeUnknown` (`commit.rs:956`).
- `SETTLE_LAPS = 2` (`commit.rs:307-316`), new public const, documented as a budget for the work.
- `Doubt` (`commit.rs:784-830`): one round only (the brief: "holds at most one round, the last"),
  `may_land` starts at the store's flag and only falls (`cannot_land`, `seen_session`). `end`
  builds `InDoubt` with the flag and the store's own outcome as `source`.
- `Lap` + `read_lap` (`commit.rs:832-862`): the four reads in one place. 657.2b's `lost_commit`
  now reads through it (`commit.rs:1257-1268`): same reads, same order, same fault mapping
  (`stopped(Store)`), so the `Conflict` path is unchanged. This also removes the base's second
  `// deferred: #1029` marker (base `commit.rs:973-974`).
- `gone_in_doubt` (`commit.rs:864-887`): the slot key read once more behind a missing session;
  absent → `SessionMoved` with the flag dropped, a record → `SessionRecordMissing`, fault → `Store`,
  all keeping the doubt.
- `InDoubt::cause` (`commit.rs:406-408`), `Stop::SessionMoved` (`commit.rs:451-455`),
  `Stop::OutcomeUnknown` (`commit.rs:456-460`), their `Display` (`commit.rs:548-555`) and
  `source` arms (`commit.rs:563-582`); `InDoubt`'s `Display` names the cause and "nothing more was
  sent" (`commit.rs:489-504`). `failed` (`commit.rs:768-782`) is now the compensation's only, with
  cause `OutcomeUnknown`; the compensation's `// deferred: #1030` stop is untouched
  (`commit.rs:1151-1152`).
- Docs: module header (`commit.rs:7-8`), *A commit that lost* (`commit.rs:55-57`, `:88-90`), the
  new *A commit in doubt* section (`commit.rs:115-171`), the stops table (`commit.rs:210-211`),
  bounds (`commit.rs:227-237`), `COMMIT_ROUNDS` (`commit.rs:300-305`), `CommitError::InDoubt`
  (`commit.rs:360-364`, the brief's `commit.rs:285` doc comment), `InDoubt` fields, `commit_part`
  (`commit.rs:1407-1410`).
- `docs/design/architecture/05-building-block-view.md:250`: "settling it is still to land" replaced
  by what settling does now, including that no batch follows an unknown outcome.

Out of scope and untouched: `stage.rs`, 0016, ADRs, `Cargo.lock`, `deny*.toml`, the gateway, any
production caller (none added), Complete, the drain, the reaper, DST.

**Tests (`crates/core/tests/multipart_upload_part_commit.rs`, appended; 75 legs, all green):**

| Brief item | Test(s) |
|---|---|
| (B) landed, unknown, per flag | `b_a_landed_commit_answered_unknown_is_committed` (`:4305`) |
| (B) unlanded re-upload | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` (`:4340`) |
| (B) held, (i) before / (ii) after return | `b_a_held_commit_is_committed_only_once_a_lap_finds_its_slot_gone` (`:4369`) |
| (B) same-number rival, per flag | `b_a_same_number_rival_over_an_unapplied_commit_keeps_the_doubt` (`:4416`) |
| (B) changed prior: rival over non-empty / repair away-and-back / rival over empty | `:4461`, `:4497`, `:4563` |
| (B) read-gap, four histories × gaps 0-4 | `read_gap` (`:4623`) + one test per history (`:4698`-`:4721`) |
| (X) | `x_late_staging_that_rebuilds_a_landed_commits_slot_never_compensates` (`:4734`) |
| (X2) (a) per flag, (b) | `replay` (`:4815`), `x2_a_…` (`:4893`), `x2_b_…` (`:4899`) |
| C1 / C2 / C3, C3′ | `:4913`, `:4993`, `:5032` |
| One batch, then reads (reworked H4, in place) | `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` (`:2610`) |
| Out of flight, empty and non-empty (rework of v2's every-round-out-of-flight leg) | `c_a_commit_out_of_flight_is_never_sent_again_or_compensated` (`:5075`) |
| A conflict, then a held round | `c_a_conflict_then_a_held_round_sends_no_third_batch` (`:5109`) |
| Read faults at each read | `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (`:5147`) |
| Slot rewritten / slot gone under a moved session | `:5217`, `:5247` |
| (F) i-iv | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` (`:5292`) |
| (H) both orders | `held_commit_and_rollback` (`:5361`), `h_a_held_commit_landing_before_a_rollback_is_in_doubt` (`:5404`), `h_a_rollback_landing_before_a_held_commit_is_in_doubt_and_never_compensated` (`:5410`) |
| (R) | `r_damage_under_a_held_commit_keeps_the_doubt` (`:5427`), `r_a_missing_session_behind_a_held_commit_is_in_doubt` (`:5578`), `r_a_session_damaged_and_repaired_mid_lap_settles_nothing` (`:5619`) |

Shared changes: `in_doubt` (`:1478`) also checks the cause and "nothing more was sent" appear in
the message, and that the flag is never above the store's report; new helpers from `:3902`.

## Decisions, and what I ruled out

**1. Two laps, not one (the call I'd most like a human to check).** The brief's read-gap leg says
"*Landed, then a rival:* `Committed`" at **each** gap between the classifying reads, which
includes the gap between the slot read and the last session read (gap 3). With a single lap that
history cannot be `Committed`: the only slot read came before the landing. Its (H) leg also
expects "the next slot read". So settling must be able to read the slot again. I bounded it at
`SETTLE_LAPS = 2`, and a second lap is read only when the first ended with the slot held under the
unchanged session **and** the round may still land (a round that cannot land any more, with its
slot held after that was known, can never be proven later: nothing but its own commit or
compensation deletes the slot under `Open@E`).
- Ruled out *one lap*: about 6 lines smaller in `settle` (no loop), but the read-gap leg's
  "landed, then a rival" fails at gaps 3 and 4. Demonstrated: mutation M14 (`SETTLE_LAPS = 1`)
  turns 7 legs red, including `b_read_gap_landed_then_a_rival_is_committed`.
- Ruled out *laps funded by the rounds left* (`COMMIT_ROUNDS - rounds + 1`): it ties a read budget
  to how many `Conflict` rounds happened first, for no reason the reads care about.
- Honest caveat: laps are sent back to back with no wait, so in production a second lap only
  catches a commit landing during the first lap's few reads. That is a small gain; the bound is a
  work budget, not a correctness argument. Correctness does not depend on the number: every lap
  applies the same proof, so more laps can only turn a correct `InDoubt` into a correct
  `Committed`.

**2. No "read again" after the session moved (iteration 2's `ReadAgain`).** With no compensation
after an unknown outcome, a moved session can never lead to `Committed` (the proof needs the
session at the exact `Open@E` bytes after the slot read, and an epoch only rises) and its flag is
already `false`. A further lap would change neither the answer nor the flag, so `settle` stops at
the first moved session with `SessionMoved`. The (H) text's "the next slot read finds the slot
gone" describes iteration 2's extra lap; the (H) tests assert the observable end result the brief
lists (in doubt, released once, own part durable / slot still held, held batch `Conflict`) and pin
exactly one lap of reads.

**3. A separate `settle`, not iteration 2's merged `judge_round`.** Iteration 2 merged the
`Conflict` classification and the doubt into one function with a `Doubt(Vec<Unsure>)` and a
`send` flag (v2 `commit.rs:768-841`, `:1134-1268`, `:1307-1349`). Once nothing is sent after an
unknown round, the doubt can only be the last round and the `Conflict` path never sees a doubt, so
I kept 657.2b's `lost_commit` behaviour as it is (only its reads go through `read_lap`) and put
the doubt in its own function. Kept from iteration 2 as the brief asks: the proof and its place
before damage, the read order, the damage arms, the read-fault handling, `InDoubt::cause`,
`SessionMoved`, `OutcomeUnknown`, the `may_still_commit` rules (part record where absent was
required, moved session, terminal delete), and "a changed prior settles nothing".

**4. One `OutcomeUnknown` cause for both batches.** For the compensation (unread) and for a
commit whose laps ended with the slot held, the true statement is the same: the store could not
report the outcome and no read showed it landed. One variant keeps the public enum smaller.

**5. The `(SessionSeen::Same, _)` arms (`commit.rs:949-956`).** After the proof returned, slot
`Gone` under `Same` cannot reach the match; I matched `_` with a comment rather than add an arm that
can't run. If it ever did run it would answer `InDoubt`, the safe direction.

**6. Iteration 2 test reworks.** Reworked to "in doubt, no compensation":
`b_a_rival_after_an_unapplied_unknown_commit_is_compensated` (now `b_a_same_number_rival_…`), the
read-gap not-landed runs, `c_every_commit_round_out_of_flight_…` (now `c_a_commit_out_of_flight_…`),
C1's rival run, H's unlanded run. Reworked to "one batch": `b_an_unlanded_reupload_…`,
`b_a_held_commit_is_never_compensated_and_at_most_one_lands` (now `b_a_held_commit_is_committed_only_once_…`;
its run (iii) needed later rounds and is gone), `b_a_held_reupload_survives_…` (its `BeforeRetry` /
`WithRelease` variants needed a second batch; now "back at the end of lap 1" or "after return"),
C1, `c_the_kept_doubt_is_the_round_that_may_still_land` (now the brief's "conflict, then a held
round"). Dropped: `c_a_later_round_failing_definitely_keeps_the_held_rounds_doubt`,
`f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt`. Also dropped iteration 2's wrapper
`released` map and `h3_a_round_sent_again_requires_the_bytes_its_own_landed_round_released`: no
round is ever sent again, so the slot guard stays exactly as strict as the base's. Put back: the
base's H4 one-batch leg, reworked in place (`:2610`), its `no retry` assertion kept.

**7. Tests split so every red is attributable.** My first mutation run showed the read-gap loop
stopping at "landed, then a rollback" under M1, which hid whether the not-landed runs went red. I
split the read-gap leg per history, (X2) per run and (H) per order (thin `#[tokio::test]` wrappers
over `read_gap`, `replay`, `held_commit_and_rollback`). (X2) checks the C-1 history first (at most
one of A's commit batches landed; no published chunk owed) and A's answer last, so a resend fails
on the history it leaves, not on the answer.

**8. (X)/(X2) histories are real.** Staging really holds and retries: the wrapper scripts
`Reserve`, `Intent` and `Renewal` `[hold, pass]`, `stage_part` re-reads and sends the next round,
and the three held batches are the real staging batches. In (X) and (X2a) each release is asserted
`Committed`, and a step reads back the slot (equal to the bytes the commit required) and the owned
entry (back) before the re-read.

## Demonstrated reds (required by the brief)

Each row is a temporary edit of the final `commit.rs` (or a whole file), run against the final
test file by a driver that restores the file after each run (`cmp` against the saved copy
confirmed). Counts are from the run with 74 legs; M0, M2 and V2 were re-run after the 75th leg was
added and also turn it red.

| Edit | Named leg(s) it must fail | Result |
|---|---|---|
| **V2**: iteration 2's whole `commit.rs` (rebuilt from `iteration-v2/patch.diff`) | (X), (X2) both runs | 30 red. `x_…`: "no compensation" fails (it compensated). `x2_a_…`: "Applied(false): at most one of A's commit batches lands" fails. `x2_b_…`: "Held: at most one of A's commit batches lands" fails. Both (X2) runs go red exactly as the plan review predicted: the resend lands a second time. |
| **M1**: iteration 2's settle (held slot read as "did not land", then compensate) | (X), the rival leg, read-gap not-landed | `x_…` ("no compensation"), `b_a_same_number_rival_…` (got `Refused(OperationAborted)`), `b_read_gap_not_landed_then_a_rival_…`, `b_read_gap_not_landed_then_a_rollback_…` red (8 total). |
| **M2**: the same batch again after an unknown round (`Judged::Again`) | (X2) both runs, (B) re-upload, one-batch | `x2_a_…`, `x2_b_…` ("at most one … lands"), `b_an_unlanded_reupload_…` (got `Committed`), `h4_a_commit_answered_unknown_…` ("no retry"), `c_a_commit_out_of_flight_…` red (17 total). |
| **M3**: a part present + session unchanged → `Committed` | (B) re-upload | `b_an_unlanded_reupload_…` red (12 total). |
| **M4**: part bytes equal to its own record → `Committed` | C1 | `c1_an_empty_part_is_never_committed_on_its_bytes` red (13 total). |
| **M5**: the proof uses the session read made **before** the slot read | C2 | `c2_a_torn_read_never_answers_committed` red (+ read-gap). |
| **M6**: "slot gone" only when the key is absent (ignores `attempt_id`) | C3′ | `c3_a_landed_commit_then_a_rival_is_committed` red (the only one). |
| **M7**: a rival record where absent was required, checked before the proof | C3 | `c3_…` red (8 total). |
| **M8**: the own-commit proof removed | (B) landed, C3, one-batch | `b_a_landed_…`, `c3_…`, `h4_a_commit_answered_unknown_…` red (8 total). |
| **M9**: compensate when the slot is held under a newer `Open` epoch | (H) | both `h_a_…` tests red (+ two read-gap rollback histories). |
| **M10**: any changed prior → "cannot land" | (B) changed-prior legs (the flag) | `b_a_held_reupload_over_a_non_empty_prior_…`, `b_a_held_reupload_survives_…`, `b_a_rival_over_an_empty_prior_…` red (+ R mid-lap). |
| **M11a/b**: a lease past expiry drops the flag / compensates | (F) | `f_a_commit_delayed_…` red in both (the only one each). |
| **M12a-f**: the doubt dropped on each damage arm (torn part, torn slot, damaged session, prior gone, record behind a missing session, slot rewritten) | (R) | a–e: `r_damage_under_a_held_commit_keeps_the_doubt` red, one at a time; f: `c_a_slot_rewritten_…` red. |
| **M13a/b**: the doubt dropped at each read-fault site (`read_lap`, the slot re-read) | read-fault legs | a: `c_a_read_fault_…` + `c_a_conflict_then_a_held_round_…`; b: `c_a_read_fault_…` red. |
| **M14**: `SETTLE_LAPS = 1` | (not in the brief's list; binds decision 1) | 7 red, incl. `b_read_gap_landed_then_a_rival_is_committed`. |
| **M0**: the stack base's behaviour (in doubt at once, no reads) under the new API | "the fix reverted" | 24 red, incl. (B) landed, C3, (X), one-batch. |
| **BASE**: the stack base's `commit.rs` unchanged | — | does not compile against the test file (`InDoubt::cause`, `Stop::OutcomeUnknown`, `Stop::SessionMoved` are new API), so M0 stands in for the behavioural red. |

## Refuting my own test (forced)

- **(a) Genuine red? Yes.** With the fix's behaviour reverted (M0: the stack base's "in doubt at
  once") 24 legs fail, including every leg that needs `Committed` from reads and every leg that pins
  the laps. With iteration 2's own `commit.rs`, 30 fail, including (X) and both (X2) runs. Every
  brief-listed edit turns its named leg red (table above). The literal base file does not compile
  against the test, which is why M0 exists; C4-verify will report green-only anyway, since the suite
  lives in an existing file (brief, *Verification posture*).
- **(b) Production path? Yes.** Every leg calls the production `commit_part` / `upload_part`
  (`Rig::commit`, test `:1082-1085`) on a real `RedbMetadataStore`; the wrapper
  (`Scripted`) only holds, delays or fails batches and reads, and lands held batches on the same
  store. Staging is the real `stage_part`; rivals are real `commit_part` calls; Abort is the real
  `abort_session`. No copy of the settling logic exists in the test.
- **(c) Fixture includes the fault? Yes.** The unknown outcome is injected per batch; held batches
  really land later (`release_held`), and the legs assert what they answered (`Committed` /
  `Conflict`). In (X)/(X2) the late staging batches are the attempt's own held reserve, intent and
  renewal, and the test asserts each lands and that the slot then holds exactly the bytes the commit
  required. Test-local stand-ins, where the production writer doesn't exist yet or lives elsewhere:
  the rollback (byte-level re-encode of the session, the 657.2b helper), the idle fence (#625),
  the teardown (#659) and the staged re-place (custodian's batch shape, `0016:671`). Each is a real
  batch on the real store with the production writer's preconditions.

## Gates and tools run

- `cargo test -p wyrd-core --test multipart_upload_part_commit` under `timeout 1500`: 75 passed in
  0.6 s. The project has no single-test runner besides `engine/scripts/run-verify.sh`, which needs
  its own clean `../wyrd-verify-l0` checkout (the Check gate's), so I used this command for the
  red/green loop; every leg also runs under its own 60 s `within` bound.
- `./engine/xtask.sh ci` (the project's gate, `cargo xtask ci`) twice on the patched tree, the second
  on the final tree: "xtask ci: all checks passed". It ran `typos`, `lint_docs.py` and
  `render_site.py --check` (both external dependencies the brief lists are installed), fmt, clippy,
  build, the workspace tests and the DST tier.
- `cargo fmt --all -- --check` and `cargo clippy -p wyrd-core --all-targets`: clean.

## C5 (advisory) survivors

**None.** I ran the project's own C5 script, `scripts/mutants-in-diff` (`cargo mutants --in-diff
patch.diff --no-shuffle --cap-lints=true`), on this bundle's `patch.diff` in the worktree: **31
mutants, 16 caught, 3 timeouts, 12 unviable, 0 missed** (exit 3, the code cargo-mutants uses when
timeouts occurred).

- **Timeouts (3), detected, not survivors:** `commit.rs:904:14` (`laps += 1` → `*=`),
  `commit.rs:955:39` (the lap guard → `true`) and `commit.rs:955:54` (`&&` → `||`). Each makes
  `settle` read laps without bound while a held batch never lands; the held-commit legs then hang
  until cargo-mutants' 20 s timeout. So the lap bound is bound by tests, not only by the code.
- **Unviable (12):** every one is a `Default::default()` (or `Some(Box::leak(Box::new(Default::default())))`)
  replacement of a return type with no `Default` impl (`Stop`, `CommitError`, `Doubt`, `Lap`,
  `UploadPartOutcome`, `Compensated`, `Lost`, `&dyn Error`): `commit.rs:564`, `:706`, `:713`,
  `:772`, `:797`, `:822`, `:849`, `:876`, `:901`, `:1137`, `:1257`, `:1319`. They fail to type-check,
  so they say nothing about the tests; `--cap-lints` cannot help a type error.
- **Caught (16):** the `Display` and `source` bodies, `Doubt::cannot_land` and
  `Doubt::seen_session` emptied, the lap counter decremented, the part-key `!=`, both `required`
  guards forced either way, the `PartRecordMissing` guard and its `&&`, and the lap guard forced
  `false` or its `<` turned into `==`, `>` or `<=`.

`mutants.out/` is left in the worktree (git-ignored by the target's `.gitignore:16`), as the gate
itself would leave it.

## Scratch

Throwaway files live under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1029`):
`pdca-builder-1029-mut/` (the mutation driver `mutate.py`, saved copies of the good, base and
iteration-2 `commit.rs`, every run's log, `results.json`, both gate logs, the mutants log) and
`pdca-builder-1029-v2tests/` (iteration 2's test file rebuilt, used once to confirm the bundle's
old test file was iteration 2's). The gate's docs render left `wyrd-docs-build-*` there too. Not
deleted: the builder role says cleanup is the harness's.

## For the reviewer and the human

- Decision 1 (two laps) is my reading of the read-gap and (H) legs; a single-lap design is simpler
  but cannot meet "landed, then a rival: `Committed`" at gap 3.
- C4-diff-cov will say the patch does not apply on `origin/main` until #1028 merges (expected, per
  the brief).
- No push, no PR.
