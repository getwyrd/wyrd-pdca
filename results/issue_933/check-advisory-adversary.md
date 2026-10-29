# Adversarial review — #933 (657.1b, in-flight renewal)

Verdict: **could not refute the fix.** I re-ran the evidence, made four of the brief's required red edits myself, and drove the branches no leg reaches. The renewal code behaved correctly every time. What is left is two test and doc gaps the builder can close, and two gate rows a human should check before trusting them.

## Attempts that failed to refute

- **The evidence holds, and it runs the production path.** In a scratch copy of the patched tree, all 89 legs of `crates/core/tests/multipart_upload_part_stage.rs` pass. They call `stage_part` and `StagedAttempt::renew` over a real redb store, not over a copy of the logic. The C4-verify "pass" row is green-only (`gate-logs/C4-verify.log`, last lines), so I made four of the brief's required red edits myself:
  - Slot put dropped from `renewal_batch` (`crates/core/src/multipart/stage.rs:1616-1620`): 26 legs fail. `r2_a_renewal_that_crosses_expiry_defeats_the_reapers_fence` fails for exactly the reason it states: the stand-in fence answers `Committed` instead of `Conflict` (test file `:5024`).
  - Renewal disabled (`renew_at = u64::MAX` at `stage.rs:1838-1841` and `:1909`): 31 legs fail, including every R2 leg.
  - Iteration-1 fix reverted (`stage.rs:2228` returns the halt directly): both `r4_a_renewal_stopping_beside_a_pending_write…` and `r4_…after_the_writes_answered…` fail.
  - Quiesce cover judged at the round's own reading instead of after the commit (`stage.rs:1758`): `r7_the_quiesce_renewal_covers_its_hand_off_or_refuses` fails.
- **The logic.** I tried each of these and found no failing input:
  - abandoning a renewal between its commits or during one (`RenewalLedger::abandoned`, `stage.rs:1072-1090`);
  - matching unknown outcomes by slot bytes only (`stage.rs:1703-1707`);
  - disarming the write deadline once every put has answered (`stage.rs:1992-1995`);
  - poll order in the three-way race (`stage.rs:2004-2012`);
  - the refusal path beside pending writes (`stage.rs:1878-1905`);
  - lease monotonicity (`stage.rs:1567-1571`);
  - the clamp count `3 + 2N` (`crates/core/src/multipart.rs:5190-5192`, `:5539-5542`).
- **The single C5 survivor is an equivalent mutant, not a test gap.** The mutant swaps `>` for `>=` at `stage.rs:326`. `3 + 2N` is always odd and `MAX_BATCH_OPS` is 500, which is even, so the two comparisons agree for every `u32` N.

## Findings

- NEEDS-HUMAN [impl] — **Three changed branches in the renewal race are never run by any leg.** I measured diff coverage with `cargo llvm-cov` over the stage and knob test files, because the gate never measured it.
  - `stage.rs:2024` (`None => break None`): a body answer seen at or after its idle deadline while a renewal commit is still in flight. This is the only path where R3's "renewal never holds staging past the body-idle bound" meets a late answer. `r3_the_body_idle_bound_holds_while_renewals_keep_the_leases` only covers the timer path.
  - `stage.rs:1733`: a renewal round lost to a session moved to a later `Open` epoch. It should adopt the new epoch and retry.
  - `stage.rs:2018`: the timer fires beside a renewal and the work answers in the same instant.

  I drove all three with scratch legs, and the code is correct:
  - **X2.** Setup: `body_idle_millis = 8_000`, renewal `Act::Gated`. The clock advances by `HALF`, then the timer fires, then the renewal reaches its gate. Then `clock.set(T0 + 8_000)` and the rest of the body is sent. Result: `InDoubt { may_still_commit: true, backend: ABANDONED_COMMIT, cause: BodyIdle { idle_millis: 8000 } }`, with no intent.
  - **X1.** Setup: `Act::First(move_session(.., open_at(session, 1)))` on the first renewal. Result: renewal answers `[Conflict, Committed]`. The second renewal and every intent require `Open@1`, and the attempt hands off `Open@1`.
  - **X3.** Setup: an `on_fire` hook sends the rest of the body as an early timer fires beside a gated renewal. Result: staged, digest intact.

  Add these as legs so a regression on any of these branches goes red.
- NEEDS-HUMAN [human] — **C4-diff-cov has not measured this bundle in two rounds** ("patch.diff does not apply on origin/main", `gate-logs/C4-diff-cov.log`). The bundle is either stale against `main` or stacked on an unmerged #932. Either way, it needs a rebase or refresh before publishing, and the rubric asks that stacked branches be refreshed before review.

  My own measurement: `stage.rs` has 394 instrumented added lines, 15 of them uncovered:
  - 808-810 and 815-820: `Display` for `RenewalRoundsSpent` and `CoverShort`;
  - 866, 872, 878: `Display` for `WriteEffectUnknown`, which neither file renders;
  - 1733, 2018, 2024: the three branches above.

  `multipart.rs` has 7 of 7 added lines covered. A human should decide the stack order with #932 before sign-off.
- NEEDS-HUMAN [human] — **The T4-batch-review "pass" may have rested on an empty review.** It is a gating row, and its only evidence is one summary line: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 38.65 s (`gate-logs/T4-batch-review.log`). That is three codex passes over a 212 KB diff, straight after a round that found 3 blocking issues. Open `results/issue_933/review-batch.md` and confirm three real passes over the whole diff before counting this row.
- **The timing promise in the docs is stated too weakly** (doc only, no safety effect). `stage.rs:140-144`, like brief R2, says no lease lapses "while each renewal round … finishes inside half a TTL". But an intent runs with no renewal (`stage.rs:1919`, by design), so the intent's time adds to the next renewal's.

  Concrete case (X5): chunk 2's bytes arrive at `T0+HALF−1`, so the renewal is not yet due. Chunk 2's intent takes 2,000 ms (`PassThenAdvance`). The renewal then starts at `T0+6,999` and takes 3,100 ms. At `T0+10,099`, chunk 1's owned lease (`T0+10,001`) is read lapsed. Every step stayed under half a TTL.

  It does no harm in practice: the slot lease, renewed by the intent, is live until `T0+14,999`, and the reaper reads any live lease as live (`0016:2517`). Suggested wording: "each renewal round, plus any intent that started before the renewal fell due, finishes inside half a TTL".
- **A lost owned entry is reported under the wrong name** (low; I found no realistic trigger). When an `sidx:` entry is removed while the slot and session are unchanged, every renewal round conflicts. That costs 4 commits and ends in `Stop::RenewalRoundsSpent` (class `Terminal`) instead of a "lost entry" error (X4). The cause is that `reconcile` compares only the slot (`stage.rs:1798-1806`), whereas the intent path names the same fact `Stop::EntryLost` (`stage.rs:2361`). Only the reaper deletes `sidx:` entries, and it fences the session first, so this is a naming nit, not a defect.

<!-- pdca:leaf-complete -->
