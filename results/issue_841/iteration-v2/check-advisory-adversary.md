# Adversarial review — #841 (809.3) restore fence for `Open` upload sessions

Advisory only. Re-ran the proof myself in a scratch copy of `$PDCA_TARGET` (post-fix tree):
`cargo test -p wyrd-custodian --test restore_open_fence` → 8/8 green. The red half in
`gate-logs/C4-verify.log` is real: all 8 legs fail by assertion on the base (session still
`Open@3`, no `retire:` key, `Ok` where `Err` is wanted, "the race was not exercised"). None of them
fail for a compile reason. Every leg drives the production `reconcile_after_restore`.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1315` (called from `:713`): when the
  fence fails, the summary line (the only audit record of the counts on that path) still prints
  `clean = report.is_clean()`, and `is_clean()` ignores `fence_finished`. **Concrete failing case,
  reproduced:** a store with nothing wrong in it except one `Open@3` session whose fence commit
  returns `CommitUnknownResult`. The pass returns `Err`, and the summary reads
  `clean=true needs_human=false fence_finished=false message="post-restore reconciliation
  INCOMPLETE — the session fence did not finish …"`. So the line certifies the run as clean while
  the message says it is not finished, and an `Open` session may still be live. The P3 leg hides
  this: `restore_open_fence.rs:815` seeds a dangling chunk, so `needs_human` is true and `clean`
  is false for a different reason. The leg checks `under_replicated`, `dangling` and the message
  (`:838`, `:844`) but never `clean`. Fix: `clean = fence_finished && report.is_clean()`, and add
  a P3 variant with no other finding that asserts `summary["clean"] == "false"`. (The T4 batch
  review found the same thing three times. This confirms it by running it.)

- NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2265`: the "`None` unless `Open`" contract
  of the new public `SessionRecord::open_teardown` is not tested anywhere. Its only caller,
  `crates/custodian/src/restore.rs:800`, calls it from inside the `SessionState::Open {}` arm, so
  the non-`Open` branch never runs. If that guard were deleted, `open_teardown` would return a
  `{session, all}` teardown for a `Completing` session (the exact shape `0016:2187` limits to
  `Open`), and every test would still pass. Downstream callers (#656 Abort, the reaper) will rely
  on that guard. This is also why C5 reports 5 survivors (`multipart.rs:2265`, `:2272`, `:3622`).
  I applied the `delete !` and `key -> vec![]` mutants by hand, and both are killed by the
  custodian tests (9 and 7 failures). They survive only because the mutant run uses `wyrd-core`'s
  own tests, which never call the new API. Fix: a `wyrd-core` unit test that calls
  `open_teardown` on each state and at `u64::MAX`, and round-trips `obligation().key()` /
  `payload()` through `decode_retire_obligation`.

- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log`: the gating T4 review also blocks three
  times on missing seeded Tier-0 DST coverage for the fence (rubric: "a new destructive or
  concurrent path lands with seeded Tier-0 DST coverage"). The brief puts DST out of scope on
  purpose (child-5), but the patch has no `// deferred: #N` marker and no recorded rejection. So
  under the rubric's own deferral rule the finding is not yet settled, and T4 stays red. A human
  must record the deferral against child-5's tracking issue, or decide the fence can't land
  without it.

## Not a refutation (context)

- `C4-diff-cov` "does not apply on origin/main" is expected. This bundle sits on main + child-1 +
  child-2 (brief line 5), so changed-line coverage was not measured at all. It is not evidence
  either way.
- Attempted and could **not** break:
  - Splitting the fence across two commits, in either order, is caught by F-atomic
    (`restore_open_fence.rs` run 1 / run 2) and by `assert_fenced`'s one-commit check.
  - A blind put of the obligation is caught by F-collision.
  - Dropping `require(key, read)` is caught by F-race (it overwrites `Completing@4`).
  - Fencing before Pass 3 is caught by P3's `DANGLING` line.
  - Paging: `fence_open_sessions` (`restore.rs:720`) mirrors `gc::walk_staged_range`, and
    `checked_page` refuses an empty page that still has a cursor, so a cut-short listing
    becomes an error, not a silent stop.
  - Unknown outcome that actually landed: the re-run sees `Aborting@E+1`, skips it, and writes
    nothing (probed).
  - The fence's obligation does not collide with GC's `retirement_draining` (`gc.rs:756`): that
    only reads `retire:bytes:` keys named by a mark's event, and restore marks carry none.
  - A key that names no upload is now named twice, in `unresolvable` and in
    `sessions_unsettled` (probed). The `KeyNamesNoUpload` doc says this is intended, and it is
    harmless.
