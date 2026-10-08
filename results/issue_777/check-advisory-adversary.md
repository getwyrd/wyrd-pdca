# Adversarial review — issue #777 (iteration 6)

**Verdict: I could not refute the fix.** I re-ran the proof, ran three hand-made mutants, and
ran six attack tests of my own against a scratch copy of `$PDCA_TARGET`. The production change
held every time. Nothing below is gating, and nothing needs a rebuild. The three notes at the
end are small, and two of them are pre-existing or belong to #776.

## Evidence: re-run, and checked that it uses the production path

- `crates/custodian/tests/segmented_map_repoint.rs` passes 13/13 at the target, and the rewritten
  `segmented_map_reconstruction.rs` passes 6/6. The red leg in `gate-logs/C4-verify.log`
  matches the module doc at `segmented_map_repoint.rs:14-27`: 12 of 13 tests fail on the base.
  Leg 3 fails on `Blocked` vs `Satisfied` (`:486`), leg 4 fails on "the race never landed"
  (`:470`), leg 9 fails on the base's own `u64` overflow panic at the old `reconstruction.rs:1157`,
  and leg 5 is green, as expected.
- Every leg drives `reconcile_step` (`segmented_map_repoint.rs:432`), so none of them uses a
  separate copy of the logic. The test double's `commit` (`:132-147`) checks each precondition
  byte for byte, so a CAS cannot pass by accident. The race hooks fire after the `scan_page`
  page (`:126-128`), as the brief requires, and each race leg asserts `meta.raced()`, so a leg
  cannot pass if its race never happened.
- Attempted: a test that passes for the wrong reason. Leg 1 (`:530-536`) now checks the exact
  orphan key, so a mark on the survivor or the destination would fail it. Leg 6/7
  (`:745-765`) checks the abort count and reason, and `assert_lost` (`:491-494`) checks
  conflict=1 and aborted=0. Could not refute.

## Hand-made mutants (cargo-mutants made only 6 viable mutants, so C5 alone says little)

- Removed the once-per-object dedupe in `Reading::contain` (`reconstruction.rs:547-549`).
  Leg 7 fails at `segmented_map_repoint.rs:749`. **Caught.**
- Changed `(None, None) => None` to `continue` at `reconstruction.rs:667`, which silently skips
  an unparsable segmented object. Leg 8 fails at `:794`. **Caught.**
- Built `prior` from the scanned `record.clone()` instead of `resolved.record`
  (`reconstruction.rs:702`). Leg 13 fails at `:940`. **Caught.**

## Attack tests (scratch only, not shipped) — trying to break the fix

- **Restart onto a FLAT live root** (a segmented→flat supersede lands during the resolve). The
  pass answers `Changed`, the flat root goes to version 3 with `CHUNK` on `[0, FREE]`, and the
  queue is empty. `prior = resolved.record` (`reconstruction.rs:702`) handles the shape change
  correctly.
- **One object, two owed chunks: the seg 0 chunk is healthy, the seg 1 record gets torn under
  the move.** The pass answers `Blocked`. The seg 0 chunk is repaired and drained, the object
  is named once (`(1,1,true)`), there is one abort, and `CHUNK` stays queued. Guard (b) holds:
  the move's containment sets `reading.incomplete` before the drain gate at `:477`.
- **Two owed chunks in the same `seg:` record, no race.** Both land in one pass (placements
  `[[0,2],[0,2]]`, 2 orphan marks), which matches the comment at `reconstruction.rs:415-419`.
- **Next pass after leg 3's lost race.** The pass answers `Changed` and the record ends up
  `[[0,2],[0,1]]` with `orphan:7:41472:1`. So "a lost CAS is a retry" holds: the next pass
  re-plans onto the winner's placement (C-1 holds).
- **Sibling edit that lands AFTER the move's own re-read** (on the way into `commit`). The pass
  answers `Satisfied` with one conflict, and the next pass answers `Changed` and keeps the
  sibling's placement. This is safe; see the doc note below.

## Notes (advisory, not routed)

- `docs/design/architecture/06-runtime-view.md:40` says "so a concurrent move of a *sibling*
  chunk in the same segment record is merged". That is only true when the sibling edit lands
  **before** the move's re-read. The attack test above shows a counter-case: a sibling edit
  that lands between the re-read and the commit makes the repair lose (one conflict, one
  stranded rebuilt fragment, #723) and retry next pass. `crates/core/src/metadata.rs:3208-3209`
  states this correctly ("one that lands after it fails the CAS"). The comment at
  `reconstruction.rs:1154-1155` is close enough because it says why ("as IT re-reads it").
  This is a one-clause fix to the doc. I'm not routing it because it is not worth a review
  round on its own (the rubric's definition of done).
- Attempted and not raised: a segmented root whose stored bytes are not the canonical
  `encode(decode(bytes))`. For example, a stray leading space would do it. `InodeRecordWire`
  (`metadata.rs:1659-1671`) does not deny unknown fields, so a root written by a newer build
  would also do it. Such a root loses the root pin every pass: `Satisfied`, the obligation
  never drains, and one fragment is rewritten each pass. I confirmed this over 3 passes in
  scratch. It is the same symptom the human closed for the `inode:01` key. But the flat arm on
  the base had the identical `require(inode_key, encode(&object.prior))` pin, the pin is #776's
  (`metadata.rs:3257`), and no writer in this build emits such bytes (one encoder, with
  `skip_serializing_if`; `08-crosscutting-concepts.md` §8.7 already treats round-trip identity
  as a system-wide rule). So it is not this diff's defect.
- `reconstruction.rs:443` is the only line diff-coverage reports as missed. It is the new
  `"unplaced"` reason label on the abort taken when the selector picks a server outside the
  fleet. No test checks that label, so swapping it for `"unresolvable-chunk-map"` would go
  unnoticed. The cost is cosmetic (the abort counter is right either way), so I'm not routing it.

## Reviewer-verdict check

- No claim in `check-gates.json` looks unwarranted. "13 test(s) ran red" counts tests that
  ran, not tests that failed (12 failed), as the brief warns. T4 shows 0 blocking, and that is
  consistent with what I found. C5's "0 missed" covers only 6 viable mutants, so it is weak
  evidence on its own. The hand-made mutants above cover the gap for the new lines.
