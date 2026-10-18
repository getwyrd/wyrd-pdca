# Build notes — issue 894 / multipart-admission-create (attempt 4)

Target: `getwyrd/wyrd` `main`. The worktree base is `d2bd810` (`pdca-integrate: issue_671`), the
same base attempt 3 used (`git diff 71e9b9f d2bd810` is empty, so the brief's citations hold).
Lines below cite the patched tree.

Starting point: attempt 3's `iteration-v3/patch.diff`, applied cleanly to `d2bd810`. The
iteration-3 carry-forward asked for one production fix (the T4 batch-review finding), its
regression test, and two tests for rules the adversary found unpinned. "No other changes." This
attempt does that, plus the doc sentence the fix makes stale (docs currency is a MUST in the
rubric) and three small test legs that pin the fix's other read sites (below).

## The finding, and why it was real

`review-batch.md`, `create.rs:347` (attempt 3): after an applied out-of-flight unknown commit
(`may_still_commit: false`), something fences the session before recovery reads it. The re-read
finds a record that is not R, so attempt 3 cleared the doubt and treated its own session as a
collision: it re-minted and admitted a second session, and the first stayed counted in `mpuctl`
with no answer naming it. That is the stranded session 0016 forbids (`0016:2015-2018`) and the
C-1 invariant rules out.

The fence is not hypothetical on `main`: the custodian's restore fence already moves `Open`
sessions to `Aborting` through `SessionRecord::open_teardown` (`crates/custodian/src/restore.rs`,
`plan_fence` at `:1112`), and the reaper (#625) and Abort (#895) will too.

Reproduced on this tree by reverting the fix's classification (mutation `T4-all` below): the new
leg `f10_its_own_session_fenced_before_the_re_read_ends_in_doubt_never_in_a_re_mint` fails with
`expected InDoubt naming 00000000000000000000000000000001, got Ok(Created { upload_id:
UploadId("00000000000000000000000000000002") })`. That is exactly the bug: a second identity
admitted, the first session left behind.

## The fix (overrides decision-table row 3 for this one case, per the sign-off)

- `crates/core/src/multipart.rs:2432-2457` — new `SessionRecord::same_session(&self, other)`:
  true when every field no transition changes is equal (`parent`, `object`, `content_type`,
  `created_at_millis`, `clock_source`, `segment_nonce`). Every later state change is a fenced
  CAS (`0016:350`) that moves only `epoch`, `attempts` (Complete fence, `0016:661`) and `state`.
  It destructures `Self` exhaustively, so a new field is a compile error there until someone
  decides whether a transition may change it.
- `crates/core/src/multipart/create.rs:508-518` — `Identity::judge` classifies what a read of
  `mpu:<id>` found: `Found::Exact` (R), `Found::Moved` (not R, but `same_session` with R after a
  decode), or `Found::Other` (absent, a different nonce or request, or bytes that do not decode).
  `Found` is at `:597-609`; the settling read `read_own` at `:610-613`.
- Three read sites use it, because each one could otherwise lose the session:
  - **The loop's re-read** (`:369-382`). After an unknown outcome, `Moved` with a standing
    doubt (held or unsettled) ends `InDoubt` naming the original upload id and nonce, cause
    `Unsettled::SessionMoved`. This is the T4 site.
  - **The settling read in `finish`** (`:627-636`). Attempt 3's `Ok(false)` cleared the doubt
    for any non-R record, so an unsettled 1021 whose re-read failed and whose settling read
    then found the fenced session ended `CreateError::Store`. That error classifies `Transient`,
    which invites a retry under a fresh identity: the same stranding by another road. Now
    `Moved` ends `InDoubt(SessionMoved)`.
  - **The held, identity-taken branch** (`:384-393`). Already ended `InDoubt` either way; now
    the cause says `SessionMoved` rather than the inaccurate `IdentityTaken` when the settling
    read finds the create's own session.
- `create.rs:149-154` — new `Unsettled::SessionMoved` variant and its `Display` (`:193`).
- `may_still_commit` follows the existing held rule, as the sign-off asked: `true` when any
  commit under the identity said so, else `false` (`InDoubt::may_still_commit` doc, `:125-129`,
  now says only finding R clears a held doubt). Strictly, once the create has found its own
  session no other batch of it can land (`require_absent(mpu:<id>)`), so `true` overstates. I
  kept the held rule because the sign-off said so and because overstating is the safe
  direction: the caller re-reads either way.

**Why `Moved` needs a standing doubt.** With nothing held and nothing unsettled, every batch of
this create under the identity is known not to be in the store and cannot land (each 1021 was
followed by a read that found no session of ours; a `Conflict` wrote nothing). So a record that
matches R's fixed fields then can only be another invocation's, handed the same 128-bit id and
nonce (the stated negligible bound). Gating on `standing()` also means `InDoubt` always has a
real `CommitUnknownResult` to carry as its `source()`, which F9's contract needs. The `Exact`
case keeps attempt 3's gate (`doubt.unknown`), unchanged.

**Why compare all six fixed fields, not only the nonce.** The sign-off says "carries THIS
identity's nonce (and request: bucket/object)". A record with this create's nonce but another
object cannot be its session, because no transition changes the object. Treating it as ours would
end the create in doubt over a session it never wrote. So it stays a collision; the new leg C6
(iii) pins that (mutations `T4-nonce`, `T4-object`).

**Docs.**
- `create.rs:278-286` and `:299-309` (rustdoc, "An unknown commit outcome" and "Which session is
  this create's") state the rule.
- `docs/design/architecture/05-building-block-view.md:206`: the sentence "a session found under
  that id afterwards is the create's own only when it is byte for byte the record its batch puts"
  was now false, so I rewrote that one sentence: byte-for-byte R is answered as created, and a
  record that keeps the create's nonce and request but has moved on is its own session, never a
  collision, ending the create in the same in-doubt error. Nothing else in the paragraph changed.

## Tests: `crates/core/tests/multipart_create.rs` (35 legs → 41)

Wrapper changes (test-only, state still in struct fields, no `static`):
- `OnCommit::UnknownThenFence { may_still_commit }` (`:746-748`, arm `:1117-1121`): apply the
  batch, fence the session it installed, then answer `CommitUnknownResult`.
- `Meddle::Fence` (`:785-786`): an `OnRead` hook action that fences the session of the create's
  latest commit.
- `Scripted::fence` (`:899-920`): the teardown by the Abort fence's own route,
  `SessionRecord::open_teardown`, CASed on the session's exact bytes with its retirement
  obligation under `require_absent`, the same batch the size leg already builds (`:1549-1553`).
- `OnCommit::RivalFirst` gains `nonce: u128` (`:722-726`), so a rival can carry this create's own
  nonce. The three existing uses pass `nonce: RIVAL` (unchanged behaviour).

New legs:

| Leg | Asked for by | What it pins |
|---|---|---|
| `f10_its_own_session_fenced_before_the_re_read_ends_in_doubt_never_in_a_re_mint` (`:2878`) | the sign-off's regression | Applied 1021, then a fence to `Aborting@1` before the re-read: `InDoubt` naming the original id and nonce, cause `SessionMoved`, one identity minted, one commit, `mpuctl` up by exactly one, the fenced session and its marker present. Run for `may_still_commit` `false` (the T4 case) and `true` (the held rule). |
| `f10_its_own_session_moved_on_is_kept_by_the_settling_read` (`:2913`) | the fix's `finish` site | Applied 1021 + fence, then the recovery's first read fails with a `TransientFault`; the settling read finds the fenced session: `InDoubt { false }`, never `CreateError::Store`. |
| `f10_its_own_session_moved_on_while_held_is_not_an_identity_taken` (`:2951`) | the fix's held-branch site | Held batch released and fenced on the recovery's `seggrp:` read: `InDoubt { true }` with cause `SessionMoved`. |
| `c6_iii_another_request_under_this_creates_nonce_is_still_a_collision` (`:2055`) | the sign-off's "nonce (and request)" | A rival with this create's id **and** nonce but another object: still a collision, `Created` under a new identity, rival record byte-identical. |
| `f8_e_a_held_create_stays_in_doubt_past_a_later_1021_and_failing_reads` (`:2687`) | carry-forward item (1) | Plan `[Hold, Unknown{applied: false, may_still_commit: false}]`, every later get failing: `InDoubt { may_still_commit: true }`, cause the store fault; the held batch then lands. |
| `b_a_ledger_filled_under_the_commit_is_judged_before_contention` (`:1261`) | carry-forward item (2) | `profile_of(1)`, plan `[AdmitFirst(1)]`, no ledger at start: `Refused(SlowDown { SessionCap { count: 1, max_sessions: 1 } })`, `script.waits` empty. |
| `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` (`:1240`, assertion added) | carry-forward item (2) | `script.waits` empty for both marker moves. |

The test module header (`:11-33`) lists the new legs.

## Demonstrated reds (the convergence contract), re-run on this tree

Same method as attempt 3: a scratch script applies one edit to a pristine copy of the production
file, runs `cargo test -p wyrd-core --test multipart_create` under `timeout 900`, records the
failing legs and restores the file. `create.rs` edits carry a temporary
`#![allow(unused, unreachable_code, unreachable_patterns)]` (the workspace denies warnings).
`cmp` against the pristine copies confirmed the restore after the last row. Rows 19 and 20 ran the
way `xtask::run_dst` runs the DST tier (`RUSTFLAGS=--cfg madsim`, `MADSIM_TEST_NUM=50`, `cargo
test -p wyrd-dst --test multipart_admission`).

**Every edit failed its named leg.** "Also failed" lists other legs the same edit broke.

| # | Edit | Named leg failed | Also failed |
|---|---|---|---|
| 1 | contention budget = `MAX_UPLOAD_ID_ATTEMPTS` | `a_prime_a_create_that_loses_the_cas_many_times_is_still_admitted` | A storm, A′ second run, B contention, C2, F4, F8 (c) |
| 2 | contention budget = `MAX_ADMISSION_CAS_ATTEMPTS` | `a_prime_the_contention_budget_follows_the_profile_in_effect` | F6, F8 (c) |
| 3 | one counter for losses and collisions | `c2_the_contention_and_collision_budgets_never_share` | — |
| 4 | drop `require_absent(mpu:<id>)` | `c3_an_id_taken_under_the_commit_is_re_minted_and_left_as_it_is` | C2 |
| 5 | drop `require_absent(seggrp:<nonce>)` | `c4_a_nonce_taken_under_the_commit_is_re_minted` | — |
| 6 | drop both identity preconditions | `f3_a_late_batch_is_excluded_by_the_identity_keys_not_the_ledger` | C2, C3, C4 |
| 7 | drop the bucket `require` | `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` | F5 every-other-ending |
| 8 | re-mint after an unknown outcome | `f1_…`, `f2_…` | 18 more (all three F10, F8 (e), C5 re-mint, C6 (ii), F3–F9) |
| 9 | replay the original batch after an unknown outcome | `f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read` | 15 more (all three F10) |
| 10 | any record found under the id is `Exact` (attempt 1's code; one edit in `judge`, which both the loop and the settling read use) | `c6_i_…`, `c6_ii_…` | C6 (iii), C5 re-mint, all three F10 |
| 11 | collision check only after an unknown outcome (no absence read) | `c5_a_byte_identical_record_is_never_adopted` | C1 ×2, C2, C3, C4, C5 re-mint |
| 12 | `observe` keeps only the latest outcome | `f6_a_batch_that_may_still_land_keeps_the_create_in_doubt` | F8 (e) |
| 13a | reset the doubt on a read fault | `f8_a_a_held_create_stays_in_doubt_past_a_read_fault` | F8 (d), F8 (e), F10 settling read |
| 13b | reset the doubt on a plain commit fault | `f8_b_a_held_create_stays_in_doubt_past_a_commit_fault` | — |
| 13c | reset the doubt on a failed settling read | `f8_c_a_held_create_stays_in_doubt_past_a_failed_settling_read` | F8 (d), F8 (e) |
| 14 | a failed settling read clears `unsettled` (ends `Store`) | `f8_d_an_unsettled_1021_with_a_failed_settling_read_ends_in_doubt` | — |
| 15 | on an unknown outcome at the last budgeted unit, refuse without reading | `f4_an_unknown_outcome_on_the_last_budgeted_attempt_is_settled_before_returning` | F6, F8 (c) |
| 16 | `SessionCap` returned without the settling read | `f5_a_capacity_refusal_met_in_recovery_is_checked_against_the_earlier_batch` | — |
| 17 | `InDoubt`'s `source()` = the stop cause (attempt 1's code) | `f8_a_…` (`Transient`, not `Indeterminate`) | C6 (ii), F5 every-other-ending, F6, F8 (b)–(e), all three F10 |
| 18a | `>` → `>=` in the size check | both size legs | — |
| 18b | drop `Completing` from `widest_reachable_bytes` | `…_completing_widest` only | — |
| 18c | drop `Completed` from `widest_reachable_bytes` | `…_completed_widest` only | — |
| 19 | (G) `Created` straight after a 1021 | `multipart_admission_invariants_hold_under_the_dst_seed_sweep` (reverse assertion) | the 1021 leg, the mixed leg |
| 20 | (G) `InDoubt` names the upload id reversed | `the_witness_leg_holds_and_a_held_in_doubt_batch_lands_by_quiesce` (stranded record) | the seed sweep |
| 21 | a `Conflict` whose re-read finds the same ledger bytes → collision | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | C2, C3, C4 |
| 22 | the same, retried without spending a unit | `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` | — |

This iteration's mutants. Each failed **only** the leg written for it (or that leg plus a
sibling of the same rule), which shows the new legs are what pin them:

| Mutant | Edit | Failed |
|---|---|---|
| `T4-all` | `judge` never returns `Moved` (attempt 3's behaviour) | all three `f10_…` legs |
| `T4-loop` | the loop's `Moved` branch never taken (`create.rs:376`) | `f10_its_own_session_fenced_before_the_re_read_…` |
| `T4-finish` | `finish`'s `Moved` arm clears the doubt like `Other` (`:630-632`) | `f10_its_own_session_moved_on_is_kept_by_the_settling_read` |
| `T4-held` | held branch maps `Moved` to `IdentityTaken` (`:389`) | `f10_its_own_session_moved_on_while_held_is_not_an_identity_taken` |
| `T4-nonce` | `Moved` judged by the nonce alone (`:513`) | `c6_iii_another_request_under_this_creates_nonce_is_still_a_collision` |
| `T4-object` | `same_session` ignores `object` (`multipart.rs:2452`) | `c6_iii_…` |
| `CF-standing` | `standing()` prefers unsettled over held (`create.rs:565`) | `f8_e_a_held_create_stays_in_doubt_past_a_later_1021_and_failing_reads` |
| `CF-ledger-order` | contention judged before the ledger | `b_a_ledger_filled_under_the_commit_is_judged_before_contention` |
| `CF-bucket-order` | contention judged before the bucket | `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` (and the ledger-order leg) |

`T4-nonce` first failed to compile (the edit left `same_session` unused, and dead code is an
error); I rewrote it to keep the call alive behind `false &&` and re-ran that row alone.

Attempt 3's adversary mutants, re-run (current lines): `X-373` (`NoSuchBucket` without
`finish`), `X-392` (`ProfileSkew` without `finish`), `X-382` (torn ledger without `finish`) and
`X-535` (`Ending::Ledger` → `Unsettled::Store`) each fail `f5_every_other_ending_…`; `X-362`
fails `c5_…_after_a_re_mint`; `X-564` (`Ok(Found::Other) => {}`) and `X-343` fail
`f9_a_1021_settled_absent_…`; `X-349` (skip the held branch's settling read) fails
`f5_a_held_batch_landing_between_the_identity_reads_…` and the F10 held leg. `X-505`
(`else if self.held.is_none()` → `else`) still survives, as attempt 3 recorded (open item 4).

Clean runs on the final tree: `multipart_create` 41/41; the DST target `multipart_admission` 7/7
under `--cfg madsim` with 50 seeds.

**Known equivalent mutation:** dropping `Open` or `Aborting` from `widest_reachable_bytes`
changes no answer. Not run, per the brief.

## Refute your own test

- **(a) Genuine red?** Yes, through mutation, because `create_session` does not exist on the base
  (C4-verify is UNVERIFIABLE, exit 77, as the brief pre-declares). For this iteration's fix, the
  binding check is `T4-all`: it restores attempt 3's classification (no `Moved`) and all three
  F10 legs go red, the first with the exact T4 symptom (`Created` under a second upload id).
  Each of the other 26 brief edits and 9 new mutants also turns a named leg red.
- **(b) Production path?** Yes. Every leg calls `wyrd_core::multipart::create::create_session` over
  a real `RedbMetadataStore::in_memory()`. The fence is the production teardown route
  (`SessionRecord::open_teardown`), committed as a real CAS batch on the inner store. The rival in
  C6 (iii) is a real `create_session`. Nothing re-implements the create.
- **(c) Fixture includes the fault?** Yes. The batch really lands (asserted through the store
  contents and, for the held leg, `late == Committed`), the fence really commits (the wrapper
  asserts `Committed`), the session afterwards really is `Aborting@1` with the original nonce,
  the read fault really fails the read, and each hook asserts it fired.

## Open items for sign-off

1. **Size flag:** the patch is 234 KB (attempt 3 was 213 KB), over the 100 KB backstop, accepted
   at Plan 2026-10-17. No leg was dropped or merged.
2. **Beyond "no other changes":** (i) the one-sentence rewrite at
   `05-building-block-view.md:206`, which the fix made false (the rubric makes docs currency a
   merge requirement); (ii) the two extra F10 legs and C6 (iii), which pin the fix's other two
   read sites and its "and request" half. Without them `T4-finish`, `T4-held`, `T4-nonce` and
   `T4-object` survive. If the human wants strictly the one regression leg, these three legs can
   go, but the mutants above then survive.
3. **A torn `mpu:<id>` after this create's own unknown outcome** decodes to nothing, so `judge`
   calls it `Other`: row 3 applies (collision when not held, `IdentityTaken` when held), as in
   attempt 3, where any non-R record was a collision. Only storage corruption or a foreign writer
   can put one there (Create writes R, a fence writes a canonical record), so I did not invent a
   new answer. If the human prefers the conservative ending (`InDoubt` naming the identity
   whenever a doubt stands and the record cannot be judged), it is one match arm in `judge`.
4. **`create.rs:557` (`else if self.held.is_none()`) is still unpinned**, as attempt 3 recorded:
   it only changes which held outcome `InDoubt.outcome` reports.
5. **No DST coverage of a concurrent fencer.** The (G) scenario has no actor that fences
   sessions, so the `SessionMoved` path is pinned by the core legs only. Abort (#895) or the
   reaper (#625) is the natural place to add a fencer to the seeded sweep. The rubric's "new
   destructive or concurrent path lands with seeded DST coverage" could be read to cover this; I
   did not add it, per "no other changes".
6. Attempt 2's and 3's other open items (F8 (d)'s reading, row 3's settling read while held,
   await bounds, cancel safety in the other two C-1 texts) stand as recorded in their notes.

## Gate

`cargo xtask ci` through the project's runner (`PDCA_WORKTREE=… ./engine/xtask.sh ci`), run on
the final tree (tree hash `2ee98ee9`, the one `patch.diff` produces): **exit 0, "xtask ci: all
checks passed"**, with no `FAILED` line in the log. Steps it ran: `typos`; `lint_docs.py` (OK);
`render_site.py --check` (99 pages, link audit OK), so both of Plan's external dependencies
(`typos`, the doc renderer) were present and exercised; the gitlink, unsafe and blackbox guards;
`cargo fmt --all -- --check`; workspace clippy, build and test (`multipart_create` 41/41,
`multipart_state_machine` 22/22); cargo-machete; the three `cargo deny` checks; the ADR-0035
statics gate; the deploy guard; and the DST tier under `--cfg madsim` with `MADSIM_TEST_NUM=50`
(clippy, then tests; `multipart_admission` 7/7, every other DST file green).

An earlier full run, before two comment-only edits (the `Found::Other` doc and a line wrap in the
test header), also passed; I re-ran the whole gate after them so the result above is for the
shipped bytes.

Commit-readiness: the target repo configures no git hooks (no `core.hooksPath`, no pre-commit
config); its formatter (`cargo fmt --all -- --check`) and spell-checker (`typos`) run inside the
gate above.

`patch.diff` applies to `d2bd810` (`git apply --cached --check` into a scratch index), and base +
patch gives exactly the worktree's tree (`git write-tree` of both is the same hash). The bundle
carries the core test at the root (`multipart_create.rs`, as before) and at
`crates/core/tests/multipart_create.rs`, and the DST scenario at
`crates/dst/tests/multipart_admission.rs`.

## Reading scope

Beyond `brief.md`: `iteration-v3/patch.diff` (the starting point), `iteration-v3/build-notes.md`
(to carry the reds table forward), `iteration-v3/session-carry-forward`, and the bundle's
`review-batch.md` (the T4 finding). On the target: `crates/core/src/multipart.rs` around
`SessionRecord` and `open_teardown`, 0016's `mpu:` row (`:350`) and Complete-fence row (`:661`) to
confirm which fields a transition changes, `xtask/src/main.rs` `run_dst`, and the existing test
file. I did not read any SUMMARY.md or the check-* files.

## Scratch

Mutation scripts, pristine copies, per-row logs and the gate log are in
`$PDCA_SCRATCH/pdca-builder-894-mutants-v4`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_894/pdca-builder-894-mutants-v4`), inside the harness's
per-issue scratch root. Left for the harness to reclaim.
