## Summary
**User impact:** S3 clients have no way to abort a multipart upload today, so an upload
they give up on cannot be cancelled. This PR builds the storage half of Abort so that,
once the S3 verb is wired (#668), cancelling an upload is safe: the abandoned upload's
data is always queued for cleanup, two clients cancelling the same upload at once both
get a success answer instead of a spurious conflict error, and a cancel whose result the
database could not confirm says so instead of claiming nothing happened.

This adds the Abort operation in `wyrd-core` and makes the existing post-restore cleanup
and the new Abort build their database write in one shared place. Nothing calls Abort in
production yet, and the restore cleanup behaves exactly as before.

**Stacked on #908** (the Create slice, #894): Abort's tests need sessions that only that
PR's Create can make. Review #908 first; this PR's diff includes its commit until it
merges.

## What to look at
- **The new Abort operation** and its shared batch builder, in one new file:
  `crates/core/src/multipart/abort.rs`. The table in its module header lists what Abort
  answers for each upload state.
- **The one change to existing production code**: the post-restore fence in
  `crates/custodian/src/restore.rs` now calls the shared builder instead of building the
  same batch inline (an eight-line block replaced by one call).
- **To try it**: `cargo test -p wyrd-core --test multipart_abort` (24 tests over an
  in-memory redb store), and `cargo xtask dst` for the seeded concurrent-Abort scenario.

## Root cause
No client Abort existed: the only writer of the `Open → Aborting` session edge was
restore's `fence_session`, which built its fence batch inline. A second writer added
beside it would have been a second copy of the same preconditions, free to drift.

## Fix
- `abort::abort_session` (`crates/core/src/multipart/abort.rs:282`) reads `mpu:<id>` and
  answers per proposal 0016's verb × state table: `Open@E` is fenced to `Aborting@E+1`
  with its `{session, all}` `retire:bytes:` obligation in the **same** batch (`Fenced`);
  `Aborting` answers `AlreadyAborting`; `Completing` is refused `OperationAborted`;
  `Completed` or absent answers `NoSuchUpload`. Only the `Open` case writes anything.
- `abort::fence_batch` (`abort.rs:77`) is the one builder of the fence batch: it
  `require`s the exact session bytes read, `require_absent`s every obligation key, then
  puts the session and each obligation. Restore's `fence_session` uses it
  (`crates/custodian/src/restore.rs:1054-1055`), keeping the same preconditions, puts and
  order.
- The fence never touches `mpuctl`. A fenced session still holds its staged bytes and
  stays counted until the terminal delete (#659).
- **Commits that did not say `Committed`** are classified from what the store holds:
  - A `Conflict` with the session unchanged is not taken as proof the obligation key is
    taken. TiKV returns `Conflict` with nothing applied when another writer holds a lock
    (`crates/metadata-tikv/src/lib.rs:1388-1393`). Abort reads the key. If it is absent,
    Abort spends a round and retries. If it is present, Abort re-reads the session: a
    moved session means a rival's fence landed between the reads, so the answer is
    `AlreadyAborting`. Only a still-unchanged session makes it a typed damage error.
  - An unknown outcome is never read as `Conflict`. A fence that may still land
    (`may_still_commit: true`) keeps that doubt across later faults until a read shows
    the session moved. An out-of-flight outcome no read has settled ends `Indeterminate`
    naming the upload id.
  - Rounds are bounded by `ABORT_ROUNDS` (`abort.rs:58`, `MAX_COMPLETE_ATTEMPTS + 1`).
- Docs: the building-block view describes Abort, the shared builder and the unchanged
  admission count (`docs/design/architecture/05-building-block-view.md:206`).

## Verification
Line numbers are on this PR's branch (`main` + #908 + this change).

- **Claim:** an `Open` session is fenced with its obligation in one batch, and a second
  Abort answers `AlreadyAborting`.
  **Checked:** `abort.rs:77` (builder), `abort.rs:282` (Abort).
  **Test:** `d_an_open_session_is_fenced_with_its_obligation_in_one_batch`
  (`crates/core/tests/multipart_abort.rs:590`).
- **Claim:** two Aborts racing one session answer `Fenced` and `AlreadyAborting`, never
  `OperationAborted`.
  **Test:** `d_two_aborts_forced_to_race_answer_fenced_and_already_aborting` (`:686`).
- **Claim:** `Completing` is refused `OperationAborted` with the store byte-identical;
  `Completed`/absent answers `NoSuchUpload`.
  **Test:** `:726`, `:749`.
- **Claim:** Abort never moves the admission count.
  **Test:** every leg asserts `mpuctl.count()` unchanged. A temporary decrement added to
  the fence batch fails 15 tests.
- **Claim:** a lost commit is classified from what the store holds, not inferred.
  **Checked:** the `Conflict` branch in `abort.rs`, against TiKV's lock-conflict answer at
  `crates/metadata-tikv/src/lib.rs:1388-1393`.
  **Test:** `h1_…` (`:945`), `h2_…` (`:967`), `h3_…` (`:991`), `h4_…` (`:1033`), each
  through a store wrapper that answers `Conflict` without applying anything. Treating an
  unchanged session as a taken key, without reading the key, fails H1, H2 and H4. Calling
  a present key damage without re-reading the session fails H4.
- **Claim:** a doubt that a fence may still land is kept, and an unsettled outcome stays
  `Indeterminate`.
  **Test:** `i_…` (`:1061`–`:1206`) and `i2_…` (`:1246`, `:1273`). Each of these fails
  under a matching one-line change to the doubt tracking, including the new
  `i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict` (`:1112`), which
  is the only test that fails when the failed-read arm inside the `Conflict` branch drops
  the doubt.
- **Claim:** restore is unchanged.
  **Checked:** `restore.rs:1054-1055` calls the builder in place of the removed inline
  batch.
  **Test:** `j_the_shared_builder_keeps_restores_preconditions_and_puts_in_order`
  (`:1311`). Restore's existing tests pass unmodified: `restore_open_fence` 11/11,
  `restore_completing_fence` 7/7, `restore_reconcile` 17/17,
  `restore_fence_generation` 12/12, and the custodian DST file 33/33.
- **Claim:** the invariants hold under concurrency and commit ambiguity.
  **Test:** `crates/dst/tests/multipart_abort.rs:551`: concurrent Creates and Aborts on
  the simulated FDB store with the commit-ambiguity nemesis armed (1021, 1031 and a mixed
  leg). After every seed: `mpu:` records equal `mpuctl.count()`, every `Aborting` session
  has its obligation, and every session's state matches what its Aborts answered.
- **Pre-fix:** this is new functionality, so the test file does not compile without the
  change (it imports `wyrd_core::multipart::abort`).
- **Gates:** `cargo xtask conformance`, `statics` and `dst` pass;
  `cargo test -p wyrd-core -p wyrd-custodian` passes; fmt, clippy `-D warnings` and
  `typos` are clean; `cargo mutants --in-diff` reports 18 caught, 4 unviable, 0 missed.
  `cargo xtask ci` as a whole stops earlier at the existing cargo-deny advisories
  (RUSTSEC-2026-0346/0349), which this PR does not touch, so the steps after it were run
  one by one.

Fixes #895
