# Adversarial review — #1009 (segmented publication, segment-charged budget)

Bottom line: I could not break the production logic. The red→green evidence holds up when re-run. One of
the brief's three invariants has no test on the segmented path, and a plausible wrong rule there passes
all 65 legs.

## Findings

- NEEDS-HUMAN [impl] — **The invariant "a fence that wrote none never spends the budget" is untested on the segmented path.** `crates/core/src/multipart/complete.rs:1076` (the `cursor == 0` branch of `release_batch`) and `:1359` (the lost-round path). Every new leg either lands a segment batch first (D3 `multipart_complete.rs:4838`, H2-seg `:4932`, J-seg `:4981`, M `:5185`/`:5290`) or ends in doubt (G0-seg). No leg has a segmented round lose with nothing landed and then release. **Concrete surviving mutant:** charge any attempt whose lost round *sent* segment batches (add `own.charge |= writes_segments` in the `Ok(Conflict)` arm at `:1359`, and take the charged branch at `:1076` when it is set). All 65 legs pass under it. **Concrete failing case it hides:** wide fixture, the segment batch answers `Conflict` without applying in all `R_PUBLISH` rounds. The release then keeps `attempts` spent and installs a `{seg}` obligation for an empty group, so three such Completes reach `CompleteAttemptsExhausted` with zero `seg:` records ever written. That is the #636 wedge, now on the segmented path. The production code gets this right. I checked with a throwaway leg in scratch, which passes on the patch and fails on the mutant: `staged_wide()`, script `[Pass] + [ConflictUnmoved; STATED_PUBLISH_ROUNDS]`, then assert `OperationAborted`, `1 + R_PUBLISH + 1` commits, `segments_of(group_of(fence)) == 0`, `attempts` unchanged, and `obligations(RETIRE_RECORDS_PREFIX)` empty. Fix: add that leg.
- Minor, info only: `complete.rs:1380-1383` is the one new branch no test runs (checked with llvm-cov). It covers a round refused for good while it had segment batches to send, where the follow-up session read finds another writer moved the session. I read `moved()` (`complete.rs:1201`) and it is correct here. `Aborting` answers from its state, and `Open` or a torn record ends `Gone` with the fault. The teardown or rollback that moved the session carries the `{seg}` obligation (0016 *Abort / reap fence* and *Completing → Aborting* rows). Fold a leg in only if the builder iterates for the item above; this is not worth a rebuild on its own.
- NEEDS-HUMAN [human] — **T4-batch-review's gating `pass` is thin evidence.** `check-gates.json` row `T4-batch-review` records "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 17.53 s for a ~148 KB diff. `gate-logs/T4-batch-review.log` holds only that one summary line, so a cache hit or three empty review outputs would look the same. I can't see `results/issue_1009/review-batch.md`. Glance at it before counting T4 as the "one deep, multi-pass review" the rubric's Definition of done asks for. This is uncertain, not a known fault.

## Evidence re-run (the brief's demonstrated reds, plus the carry-forward mutants)

I built and ran `cargo test -p wyrd-core --test multipart_complete` on a scratch copy of `$PDCA_TARGET`: 65/65 pass. The legs drive `complete_upload` (the production path) over a real Redb store through the `Logged` wrapper. Each mutant below was planted alone and the suite re-run:

- `charged` built from `opened.attempts()` (`complete.rs:396`, attempts restored on every release): D3, H2-seg, J-seg and M-release fail.
- Obligation dropped from `release_batch` (`:1082`, `:1084`): the same four fail.
- Segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` (`:1494`): B, D3, H2-seg, J-seg and both G0-seg legs fail.
- Unknown segment-round outcome mapped to `Conflict` (`:1355`): both G0-seg legs fail.
- Carry-forward `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }` (`:1350`): M fails.
- Carry-forward `writes_segments = plan.segments > 0` (`:1340`): G0-seg (resumed) fails.
- `release_batch` always restores (`:1076`): four legs fail.
- A followed cursor spends a release round (`:1171`): J-seg and M-release fail.
- The `follow` guard at `:1062` set to `true` survives. I accept the recorded reason: `with_segments_written` is called only from `complete.rs`, teardown moves the session to `Aborting`, rollback moves it to `Open`, and nothing moves the cursor backwards.

C4-diff-cov's `fail` ("patch.diff does not apply on origin/main") is not a refutation. The brief says this slice depends on #1008, so it probably stacks on an unmerged branch; the target has no `origin` ref, so I can't confirm. I measured coverage myself with `cargo llvm-cov`. In `multipart.rs`, 48/48 instrumented added lines ran. In `complete.rs`, 148/152 ran. The four that did not:
- `:1303`: the existing torn-identity `give_up`; only its signature changed.
- `:1330`: the `step` fallback, unreachable by design because the committer refuses an unfenced batch.
- `:1381-1382`: the branch in the minor item above.

## Attacks tried that did not land

- **Cursor above 0 means a segment landed.** Holds: each batch carries the cursor move in the same atomic batch (`complete.rs:1324-1331`), and the committer's `fence_track`/`prepare` (`staged_publication.rs:507-519`) refuses any contribution off the track.
- **A stale cursor after a failed read.** The release requires the old bytes, conflicts, re-reads, `follow`s forward and goes again without spending a round (`complete.rs:1171`). That is bounded by the number of batches that landed.
- **The count the flip requires vs the committer's plan.** Both come from `split_segments` (`staged_publication.rs:417-423`, `:545`), and a mismatch is refused before any write.
- **Cursor bound.** `with_segments_written` refuses past `MAX_SEGMENT_INDEX + 1`, the same bound the decoder enforces (`multipart.rs:2845`). `MAX_ROOT_SEGMENTS` (512) is far below it.
- **Release batch vs 0016:664.** It matches the row, and adds a stricter `require_absent` on the obligation key.
- **Key collision.** The `{seg}` obligation and the flip's `{parts}` obligation share `retire:records:s:<id>:<E+1>`, but at most one of the two batches can land per fence, so they never collide.
- **`R_PUBLISH` now counts rounds, segment-batch losses included.** Consistent with 0016:1478, where it is the publish-retry bound that makes Complete terminate.
- **Clock.** No new clock reads.
- **DST coverage of the new concurrent paths.** Covered by the `// deferred: #660` marker, which the rubric treats as settled; not raised.

<!-- pdca:leaf-complete -->
