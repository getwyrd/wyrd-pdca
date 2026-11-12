# Adversarial review — issue 1030 (compensation settle + own-commit part read)

I rebuilt the patched tree in scratch and re-ran `cargo test -p wyrd-core --test multipart_upload_part_commit`. With the patch: 90/90 pass. With the base `commit.rs` and the patched tests: 18 fail, D1 among them (the base answers `Committed`). Then I applied, one at a time, each wrong-fix edit the brief lists, and each one turned its named leg red (details at the end). So the red→green evidence holds. One gap in the fix and one test gap remain, both on the new part read.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1022-1027` with `:1035-1043`: **the own-commit proof still answers `Committed` for a commit that never landed when the part key holds bytes that don't decode.** The proof read treats undecodable bytes as "something there" (`Some(_) => {}`, `:1027`), so `no_record` stays false and `(Same, Gone)` returns `Committed`. Concrete case, reproduced in scratch: stage part 1 on a fresh rig (the commit requires the key absent), script the commit `Act::hold()`, and at gap 0 erase the slot key and plant `b"torn"` at the `part:` key. The session is untouched. The answer is `Ok(Committed { part_number: 1, .. })`, the `part:` key still holds `b"torn"`, and the held batch, released afterwards, answers `Conflict`, so no commit ever landed. The same answer comes when the first part read finds the key absent and the torn bytes are planted at proof gap 3. This is the D1 false-success class (the client is told a part exists that has no record) under two pieces of damage. It conflicts with the brief's Design ("the proof also needs that key to hold a **record**", brief.md:327-329), with "Damage settles nothing" (brief.md:320), and with the rubric's ADR-0045 rule that structural damage surfaces as an error, never as a value. The reviewer likely accepted the module doc's "bytes that do not decode are not an absent key" (`commit.rs:151-152`) because the brief's Invariant sentence (brief.md:141-142) only names "absent". Fix: when the proof read finds bytes that don't decode, keep the doubt with `Stop::Record(err)` and the flag unchanged instead of falling through to `Committed`. A real landed commit leaves a decodable record, so the only cost is availability under damage, which 0016 allows.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1025`: **the C5 MISSED mutant is a real test gap.** No test pins that undecodable bytes at the proof read leave `may_still_commit` alone. Replacing the guard with `true` keeps all 90 tests green (I reproduced this). Here is a killing case that I ran: held first-upload commit, with `at_proof_gaps` used to erase the slot at gap 0, plant `b"torn"` at the part key at gap 3, and fail reads of the session key at gap 4. Expect `InDoubt { batch: Commit, may_still_commit: true }` with the store fault as its cause. The patch passes; the mutant answers `may_still_commit: false`. Add it to (K) (`crates/core/tests/multipart_upload_part_commit.rs:6938`). If the first finding is adopted, that arm changes, but its flag still needs this test.

- NEEDS-HUMAN [human] — `check-gates.json` row `T4-batch-review`: the row claims "3x codex, union, triaged" with "0 blocking, 0 recorded-rejected, 0 noise-dropped", and the run took **17.13 s** (`gate-logs/T4-batch-review.log`). The diff is about 1,500 lines, so three model review passes in 17 s are implausible. The result may be cached from an earlier patch, or empty. The brief requires "a confirmed high-effort review of the final patch" (brief.md:208-209). Before sign-off, confirm that `results/issue_1030/review-batch.md` was produced for this `patch.diff`. I didn't read it, since it's outside my inputs.

- Note (no action): `C4-verify` is green-only by design (brief.md:188-191), so the gate never showed a red. My re-run supplies that red (above). Some brief claims of "green on base" are off, though they don't affect the fix. `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` (test file `:6445`) is red on base: the base keeps `may_still_commit: true`, and the test expects `false`. The brief (brief.md:132) says (T)'s first history is green on base. `d2_…` (`:6822`) and `k_…` (`:6938`) are red on base only through their read-trace assertions. Their value as guards comes from the mutation reds below, which do hold.

## Attempted, could not refute

- **Resending after an unknown outcome** (attempt 7's `compensate`): red in `s_…`, `x3_…`, `h4_a_compensation_…`, `d3_…`.
- **Obligation keyed under the commit's (staged) epoch** instead of the round's: red in `o_a_compensation_under_a_newer_epoch_…`. The re-upload commit's obligation uses the same key form, but every commit sent before a compensation was answered `Conflict` (`commit.rs:1541`, `:1554-1569`), so it can't forge those bytes.
- **Empty part, slot gone accepted under any session**: red in both (T) runs. Late staging only adds slots, so it can't fake "gone"; the X3 empty run keeps the doubt.
- **Non-empty part settling on its slot alone**: red in `d3_…`.
- **Dropping the final obligation read**: red in `t_a_compensation_landing_before_a_rollback_…`, `t_a_missing_session_…`, `f_…`, `s_…`.
- **Lowering the flag on other bytes at the obligation key**: red in `o_other_bytes_…`.
- **Lowering the flag on a missing session without the later slot read, or skipping that read with the flag kept**: red in `t_a_missing_session_…` and `f_…`.
- **Turning torn slot or session bytes into a plain stop**: red in `f_…`.
- **Removing the part-key check**: red in `d1_…`.
- **Moving the decision to the lap's first part read**: red in `d2_…`, `b_read_gap_landed_then_a_rival_is_committed`, `k_…`, `r_a_session_damaged_and_repaired_mid_lap_…`.
- **A rival's decodable record at the part key plus an erased slot** also answers `Committed` without this attempt landing. I did not raise it: brief.md:328 explicitly accepts "this attempt's or a rival's" record.
- **Docs currency**: `docs/design/architecture/05-building-block-view.md:250` is updated, and the module doc tables match the code paths I traced.

The probe tests and mutations lived only in my scratch copy and are not part of the patch.

<!-- pdca:leaf-complete -->
