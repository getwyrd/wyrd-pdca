# Adversarial review: #1028 (657.2b, classify a lost part commit)

**Bottom line:** I could not break the fix's behaviour. The patched tree passes all 47 tests in `crates/core/tests/multipart_upload_part_commit.rs` (cargo 1.96, offline, on a scratch copy of `$PDCA_TARGET`). I then ran 37 deliberate code breaks against `crates/core/src/multipart/commit.rs`. Every red the brief asks for showed up as a failing test. Four breaks survived. Each one points at a branch no test checks. Probe tests show the patched code answers those cases correctly, so these are test gaps, not wrong answers.

## Findings

- NEEDS-HUMAN [impl] — **A compensation that finds its own obligation key taken has no behaviour test.** The branch is `crates/core/src/multipart/commit.rs:863-865`, and the module doc's table promises it at `commit.rs:119`. Break: change `obligation_key_free(store, attempt, obligation).await?;` at `commit.rs:864` to `let _ = obligation_key_free(...).await;`. All 47 tests still pass. A concrete case that tells the two apart: a same-part loser (`losing_to_a_rival`) with bytes planted at `obligation_key(&loser, 0)`. The patched code answers `Stop::ObligationKeyTaken` after one compensation. The broken code answers `CompensationRoundsSpent { rounds: 4 }` after four. The only test that touches this read checks the read *order* (`multipart_upload_part_commit.rs:3125-3133`), not what the code does with the result. The (O) test (`:3607`) stops before any compensation is sent. C5 missed this because cargo-mutants never drops a `?` result. Fix: add that leg, plus a chase variant (one rollback, key planted under epoch 2). I ran both as a probe: they pass on the patch and fail on the break.
- NEEDS-HUMAN [impl] — **"Rolled back, and the slot is gone" is untested at both re-reads.** The arms are `commit.rs:989` (commit) and `commit.rs:875-877` (compensation), both `(SessionSeen::Open(_), SlotSeen::Gone) => SlotLost`. Two breaks survive. One makes the commit compensate under the newer epoch. The other makes the compensation chase the newer epoch. In both, the attempt still ends in `SlotLost`, but only after one extra batch that the module doc (`commit.rs:104-110`) says is never sent. The slot test (`multipart_upload_part_commit.rs:3524-3595`) covers only a `live` session and a `fenced` one, never a rolled-back one. Fix: add a `rolled back` session to that test. My probe (`rig.rollback()` plus the slot erased before the batch, at each re-read) passes on the patch and fails on both breaks.
- NEEDS-HUMAN [human] — **A fenced session with its slot missing or held by another attempt is answered as a clean `NoSuchUpload`, not as damage.** The arms are `commit.rs:993` and `:879`, with the doc at `commit.rs:108-110`. The iteration-1 sign-off asked for exactly this ("foreign slot -> NoSuchUpload"), so the builder followed instructions. But 0016 says a slot is removed only by its own commit or compensation, or together with the session at the terminal delete (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:578`, `:1683-1684`), and `abort.rs` writes no slot. The slot read (`commit.rs:976`) comes before the last session read (`:977`). So if that last read still finds a fenced session, the session also existed when the slot was read, and a missing or foreign slot then has no legal cause. That is the same argument the patch uses to call a slot behind a *missing* session damage (`commit.rs:89-95`). The code is strict in that case and lenient for a fenced session. The impact is small: nothing is written either way, and the client gets a refusal instead of a record stop. A human should confirm the sign-off decision still stands given `0016:578`. If it does, nothing changes.
- NEEDS-HUMAN [human] — **The T4 "pass" is weak evidence.** The `T4-batch-review` row in `check-gates.json` reports "3x codex" with 0 blocking findings in **22.41 s** (`gate-logs/T4-batch-review.log`) over a 2,341-line patch. That is too fast for three deep passes; the result was probably cached or run at default settings. The brief (`brief.md:111-112`) says a clean T4 row at default settings is weak evidence and that sign-off needs a confirmed high-effort review of the final patch. Confirm that review happened before accepting.

## The evidence, re-checked

- The `C4-verify` "pass" is green-only. The test lives in an existing file, so the gate could not isolate a red (`gate-logs/C4-verify.log`). I did that check myself. Each red the brief lists fails the named test:
  - a pre-read slot taken as final fails `t_a_terminal_delete_between_the_{commits,compensations}_reads…`;
  - treating every missing session as a terminal delete fails `t_a_missing_session_with_the_slot_still_held…`;
  - counting a foreign slot as gone fails `t_another_attempts_slot…`;
  - compensating blind fails 16 tests, including `b_a_fenced…`;
  - sending a rollback to the reaper fails `b_a_rolled_back…`;
  - a loser answering `NoSuchUpload` fails `b_a_same_part_loser…`;
  - a damaged part read as a rival fails `r_a_damaged_part…`;
  - an unrisen non-`Open` epoch read as a fence fails `r_a_damaged_session…`;
  - dropping `require_absent` on either obligation key fails `o_…` and `assert_compensated`;
  - unbounded counters, `COMPENSATION_ROUNDS - 1` and `COMMIT_ROUNDS - 1` fail the `l_…` tests;
  - skipping the later-round lease check fails `l_a_lease_used_up_between_rounds…`.

  The tests call the production `commit_part` over a real redb store. They do not test a copy of the logic.
- The `C5-mutants` row ("24 caught, 0 missed") overstates the coverage. The first finding above is a break it never generated.
- `C4-diff-cov` measured nothing: the patch does not apply on `origin/main`, the same as in iteration 1. That is probably because 657.2a is not on main yet, but this checkout has no remote refs, so I could not confirm it. If 657.2a has landed in a different form, the base needs a refresh (the rubric's stacked-branch rule).
- Minor, untagged: `commit.rs:1037-1038` and `:191-192` promise that every round sends the same bytes (the stamp is taken once). But the test that checks this (`multipart_upload_part_commit.rs:3067-3092`) runs with a frozen clock. A break that re-stamps every round passes; adding `tick_reads` to that leg would catch it.
- Minor, untagged: `SessionRecordMissing.required` is only ever asserted at epoch 0 (`:3243`, `:3255`, `:3301`). Hard-coding `required: 0` at `commit.rs:640` passes.

## Tried and could not break

- **Re-read order and first vs. last session read:** the order of both re-reads, and using the first session read where the last one should decide.
- **Epoch judgment:** `<=` changed to `<` in `judge_session` is caught. So are a damaged part under every session state, and `PartRecordMissing` raised only when the session is unchanged.
- **The epoch chase:** it moves both the required session and the obligation key's epoch.
- **Arm order:** `Rewritten` is checked before `Left` at both re-reads.
- **Obligation key before compensating:** the key is read before the rounds-spent and lease-short compensations, including for an empty re-upload.
- **Clock:** only `StageClock` is read (ADR-0009).
- **Awaits:** every new await is a store call, which the store trait promises will terminate (`crates/traits/src/lib.rs:1333-1336`).
- **Docs:** `05-building-block-view.md` is updated.
- **Error class:** the new stops classify the same way as `abort.rs:220`.
- **Test fidelity:** the test's rollback is a text edit (attempts +1), not `complete.rs`'s release (attempt given back, `complete.rs:529-530`). Classification reads only the epoch and the state, so this changes nothing.
- **Out of scope:** DST coverage is a settled deferral (#660), so I did not raise it.

All runs used a scratch copy under `$PDCA_SCRATCH`, which I deleted afterwards.
<!-- pdca:leaf-complete -->
