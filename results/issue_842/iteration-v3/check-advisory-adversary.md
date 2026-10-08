# Adversarial review — #842 (restore fence for `Completing` sessions)

I re-ran the green leg in a scratch copy of `$PDCA_TARGET`: all 7 tests in
`restore_completing_fence.rs` pass. The frozen red leg (`gate-logs/C4-verify.log`) is red by
assertion on every test, not by a compile error. Every leg calls the production
`reconcile_after_restore`. The three gaps the last round found are now covered: the `seg:` page
boundary (case `paged`), the `part:` page boundary (G-sparse's segment on part 19,999), and the
undecodable records obligation (K's `torn` session). I then wrote extra cases and mutants against
the patched source. One production defect holds up (it is the same one T4's gating batch review
blocks on). Two more are test gaps that let a plausible regression through.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:943-948`: `recheck_fenced` trusts whatever
  group the `retire:records:s:<id>:<E'-1>` obligation names. It never checks that group against the
  session's own `(segment_nonce, E'-1)`. When the payload owes no segments, it skips silently
  (`None => Ok(())`). I ran two failing cases (scratch test, production code unchanged):
  **(A1)** an `Aborting@4` session with nonce `a7…`, `seg:a7…:3:000000` = `not a segment`, and a
  decodable `retire:records:s:a7…:3` = `{"seg":{"nonce":"9f…","epoch":3}}`. The pass reads the
  empty `seg:9f…:3:` range and returns `segments_unaccounted: []` with `needs_human() == false`.
  The session's own segment records have no deleter (X57 is open again) and nothing names them.
  **(A2)** the same setup, but the obligation is `{"parts":[[1,1]]}`, which decodes. The result is
  the same: clean, unnamed. This is the exact obligation shape leg G-collision seeds
  (`tests/restore_completing_fence.rs:455`). A session named "key taken" and then torn down by
  hand, as the CLI tells the operator to do, reaches A1 on the next run. The brief's "fact Do can
  rely on" (K) says *who* files that key. It does not say the payload names the session's own
  group, so trusting the payload is unwarranted. Fix shape: carry `record.segment_nonce()` in
  `Plan::Fenced` (`restore.rs:900-903`). Compare it with `payload.segments()`. On a mismatch or
  `None`, name the session (a new `SegmentFault` variant) and check the session's own range. Add
  A1 to leg K. The `deferred: #659` marker covers half-drained ranges, not a foreign group, so
  that deferral does not settle this.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:996-1000` (`segment_fault`): no test
  segment carries more than one chunk. `seed_segment` always builds `vec![chunk]`
  (`tests/restore_completing_fence.rs:254`). The mutant `.chunks().iter().take(1).find(..)` (check
  only the first chunk) **survives all 7 tests**. Real segments hold many chunks. Failing case for
  the mutant: one segment naming `[held, 0xD2F]`. The mutant reports the session clean. Production
  (`.find` over all chunks) names it. Fix: make H(ii)'s bad chunk the second chunk of a two-chunk
  segment.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:835-837`: G-collision seeds only the
  **records** key. Nothing tests a `Completing` session whose **bytes** key is already taken. The
  only bytes-key collision test is child-3's, and it covers `Open` sessions, which have one key. The
  mutant `for obligation in keys.iter().rev().take(1)` (require-absent on the last key only)
  **survives all 16 fence tests** (7 here plus 9 in `restore_open_fence.rs`). Failing case: a
  `Completing@3` session beside `retire:bytes:s:<id>:3` = `{"session":true}`. The mutant overwrites
  that obligation and fences the session. Production keeps it byte-identical and names
  `ObligationKeyTaken { key: "retire:bytes:s:…:3" }` (I checked both). The leg's own title is
  "Neither obligation overwrites one already there". Fix: add this as a second arm of G-collision.

- Not a refutation: C4-diff-cov's "patch.diff does not apply on origin/main" happens because this
  bundle's base is `origin/main` plus the unmerged child-1 to child-3 patches. The gate could not
  measure coverage. It is not a defect in the fix. The two mutants above partly stand in for that
  missing measurement.

- Attempted to refute these and could not:
  - `Completing@0`: fenced to `Aborting@1`, and the second pass is clean.
  - `Completing@u64::MAX`: no write and named (H(vi), plus the updated `restore_open_fence.rs`
    case).
  - A `seg:…:999999` key: this equals `MAX_SEGMENT_INDEX`, so it parses as one of the group's keys,
    and its value names a held chunk, so it is harmless.
  - Commit atomicity: a failure on any one of the three keys lands none of them.
  - The value ceiling under a real 10,000-part sparse set.
  - The fence still runs after Pass 3.
  - `needs_human` and `is_clean` include `segments_unaccounted`.
  - The blueprint's "five different bills" count matches the five bills it lists.
