# Adversarial review — #1010 (Complete: recovery from unknown commit outcomes)

**Bottom line:** I could not break the production logic. The red→green proof holds when re-run. Two test and contract gaps remain, both fixable by the builder. All `path:line` refs are to `$PDCA_TARGET` (the patched tree).

## Evidence, re-run

- **Red→green confirmed.** I ran the patched `crates/core/tests/multipart_complete.rs` against the pre-fix `complete.rs` (base `0b1143d`): 16 of 80 tests fail (G1, G1-open, G2 ×4, G3-fence, G4 ×3, G6 ×6). With the fix, all 80 pass. G3 (flip) and G5 pass on the old code too, because the old code returned the in-doubt error at once. Mutants guard those two instead: dropping the own-bytes check at `crates/core/src/multipart/complete.rs:959` is caught by G5 and G0-fence, and releasing on a failed re-read at `:1538` is caught by G3 and G2-first. The tests drive `complete_upload` / `fence_complete` against a real redb store through the `Logged` wrapper (`crates/core/tests/multipart_complete.rs:542-731`), so they exercise the production path, not a copy.
- **Hand mutants (15).** 11 were caught. 4 survived (finding 1). `cargo mutants` (C5) never tried those 4, because it does not swap one call for another.

## Findings

- NEEDS-HUMAN [impl] — **Four of the new "stay in doubt" branches have no test.** Reverting any one of them to its pre-patch form still passes all 80 tests:
  - `crates/core/src/multipart/complete.rs:945`: a re-sent fence is refused for good while an earlier fence is held. Reverted to `not_placed(..)`, it returns `Stopped{NotPlaced}`, but the held fence can still land. **Probe:** script `[Hold, Fail{transient: true}]`. Expect `InDoubt{Fence{1}, may_still_commit: true}` and 2 commits; afterwards `release_held()` returns `Committed`.
  - `:1562`: the bucket-marker read fails after a held flip. Reverted to `release(..)`, it sends a release that races the held flip and ends `Stopped{Released}`. **Probe:** `[Pass, arm(FailGets(bucket_key(BUCKET)), Hold)]`. Expect `InDoubt{Flip{1}, true}` and 2 commits (no release); then `clear_read_faults()`, `release_held()` lands, and `assert_published_once` holds.
  - `:1577`: the target's dirent read fails after a held flip. Same failure as `:1562`. **Probe:** the same script with `FailGets(dirent_key(ROOT, "photos/object-1"))`.
  - `:1468`: building the flip fails after a held batch, e.g. a target re-read that finds a prior at `u64::MAX`. Same class; harder to set up (a held segment batch, then an overwrite with a max-version prior).

  I ran the three probes: each passed on the patch and failed on its mutant. These branches carry the module doc's own rules: no release while a batch may still land, and never `Stopped` while in doubt (`complete.rs:190-194`, `:239-241`). Each deserves a regression test, like the G2-refused leg the carry-forward asked for at `:1513`.
- NEEDS-HUMAN [impl] — **A session read that does not decode settles an unknown outcome, so the request ends `Stopped` even when the unknown commit landed.**
  - **Fence phase:** `doubt.settle` runs at `complete.rs:888`, before the decode at `:895-896`. Script `[write_after(Unknown{applied: true, may_still_commit: false}, put(mpu, b"not a record"))]` returns `Stopped{fence: NotPlaced, cause: Record(..)}` (probe run here). `NotPlaced` is documented as "No fence commit of this Complete landed" (`:590-591`), which is false here.
  - **Publication phase:** `:1549-1555` sends the same unreadable session read to `moved()` (`:1327-1332`). `[Pass, write_after(Unknown{applied: true, may_still_commit: false}, torn)]` returns `Stopped{Gone}` while the object is published (the dirent is bound to inode 500, probe run here).

  Both contradict "never `CompleteError::Stopped`" while a doubt stands (`:190-194`). It takes a session record that does not decode (corruption, or a newer record format during a rolling upgrade), so severity is low. The fix is to return `doubt.in_doubt(..)` when the read after an unknown outcome does not decode, before settling.
- Note, not a defect: every in-doubt error the new code returns has `fault_behind_fence: None` (`complete.rs:1513-1514`, `:1604-1605`). So a held flip followed by, say, a damaged target (`Stop::Target`) reports only "in doubt", and the cause is lost to the operator. This matches the field's wording ("when that is why it tried" a release, `:534-538`), since no release is sent.
- Note on the gate rows: the C5 row in `check-gates.json` ("26 mutants … 20 caught, 6 unviable") is accurate, but narrower than "no surviving mutants" suggests (see finding 1). I did not re-raise the C4-diff-cov row; the human already cleared it as a stacked-base artifact.

## Attempted to refute; could not

- **Byte-identical fence trap.** A rival's fence with the same bytes is never taken as ownership (`:959-961`). A rival that has moved past cursor 0 gets `OperationAborted` through `answer_without_fence`, which is correct: an unproven fence of this call never writes segments. A held fence cannot land once a re-sent fence of the same call commits, because both require the same `Open@E` bytes and the epoch only moves forward (`:972-973`).
- **Held pin set to bytes "at or past" the held batch** (`:1557`, `Doubt::settle` `:1873-1892`). It can keep a doubt that has in fact landed (a false in-doubt, so the client waits for the reaper). It can never clear one that may still land: only this Complete moves the cursor, and every later batch requires the pinned bytes. That is a deliberate, safe trade-off, not a defect.
- **`unknown_flips`** (`:1460`, `:1541-1546`). Only this fence's flips can put those `Completed` bytes. An entry for a flip that was never sent (a round whose segment batch was the unknown one) is harmless.
- **`open_and_charged_since`** (`:1003-1008`) checked against give-back releases, a charged release after segments, and a rollback followed by a rival's give-back. It is sound, provided #625's rollback keeps the attempt charged even at cursor 0. `0016:661` supports that, and the iteration-2 human decision settled it.
- **Bounds.** Unknown rounds count against `FENCE_ROUNDS` and `R_PUBLISH`. No new unbounded loop or await.
- **Rubric.** `CommitUnknownResult` is never treated as `Conflict` (Transactions class). Docs currency is met (`docs/design/architecture/05-building-block-view.md:252-254`). The DST deferral to #660 is settled (`complete.rs:792`).

<!-- pdca:leaf-complete -->
