# Adversarial review — #842 (809.4) restore fence for `Completing` sessions

**Evidence re-run.** I reproduced C4-verify in a scratch copy of `$PDCA_TARGET`. With the fix,
`cargo test -p wyrd-custodian --test restore_completing_fence` passes 7/7. With the three
production files (`multipart.rs`, `restore.rs`, `cli.rs`) reverted to the base, all 7 fail by
**assertion**, not by a compile error (e.g. `restore_completing_fence.rs:297` sees `Completing@3`
where `Aborting@4` is expected). The tests call the production `reconcile_after_restore`. New
report fields are read only through `Debug`, so the base compiles. The proof is real.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:945-947`: `recheck_fenced` returns
  `Ok(())` as soon as `retire:records:s:<id>:<E'-1>` is **absent**. It never looks at the
  session's own `seg:<nonce>:<E'-1>:` range. That is the T4 gate's blocking finding (all 3
  T4 bullets are this one line), and I confirmed it with two failing cases in a scratch test.
  The production code is unchanged in both.
  **(A1)** `Aborting@4` session `aa…`, `seg:aa…:3:000000` = a valid segment,
  `seg:aa…:3:000001` = `not a segment`, and no `retire:records:s:aa…:3`. The result is
  `segments_unaccounted: []`, `needs_human() == false`, and the CLI exits 0. Those segment
  records have no deleter anywhere (X57 is open again), and nothing names them.
  **(A2)** This is the operator flow the patch itself invites. Take an `Aborting@4` session with
  `retire:records:s:<id>:3` = `not json` (leg K's `fb` shape, `tests/restore_completing_fence.rs:610`)
  and one live `seg:<nonce>:3:000000`. Pass 1 correctly names the obligation `Undecodable`. The
  CLI then says "inspect the named record … then re-run this pass" (`crates/server/src/cli.rs:1411`).
  The obvious repair for an undecodable obligation is to delete it. When the operator does that
  and re-runs, pass 2 reports **clean** (`needs_human() == false`). The same happens with
  K's `fc` (`{parts}` only, `NotOfAttempt`). This breaks the brief's K rule ("Already `Aborting`
  never means nothing to report").
  The `deferred: #659` marker (`restore.rs:929-930`) covers a *half-drained* range read as
  false positives. It does not cover this false negative, so the deferral does not settle it.
  **The fix is cheap and has no false alarms.** On an absent key, read one `staged_page` of
  `seg_range_prefix(group)`. If it is non-empty, name the session at its first record with a new
  `SegmentFault` (e.g. "no records obligation owes it"). My control case (`Open`-fenced
  `Aborting@4`, empty range at 3) stays unnamed. A finished #659 drain empties the range before
  it drops the key, so that case stays unnamed too. Add A1 and A2 to leg K, and update the
  `06-runtime-view.md:65` sentence that lists what "every run also names".

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:925-928`: the doc comment reasons from
  the brief's fact ("only that fence files `retire:records:s:<id>:<E'-1>`") to its converse
  (no key, so no attempt, so nothing to check). The brief never claims the converse, and A1
  shows it is false for a damaged or hand-repaired store. Reword the comment along with the
  fix above. The reviewer's acceptance of leg K rests on this converse.

## Attempted and could not refute

- **G-sparse / value ceiling.** `{session, all}` and `{seg}` are constant-size. The ceiling
  double (`restore_completing_fence.rs:119-125`) refuses oversized puts the way FoundationDB
  does. A `{session, parts}` mutant would make the pass return `Err`, and the test's `unwrap`
  would go red.
- **Atomicity / collisions.** One `WriteBatch` holds `require(mpu)`, two `require_absent`s and
  three puts (`restore.rs:821-828`). Both bytes-key and records-key collisions are tested, and
  the `u64::MAX` guard holds for `Completing` (`multipart.rs` `checked_add`).
- **Group identity.** The token epoch equals `publish_target.epoch` equals the record epoch, as
  decode requires. `recheck_fenced` now compares the payload's group with the session's own
  nonce (the iteration-3 finding is fixed). `parse_seg_key` under the group's own prefix cannot
  smuggle in a foreign epoch, because the trailing `:` stops `3:` from matching `31:`.
- **`ChunkInNoPart` soundness.** 0016 says segment records name the frozen parts' own chunks
  ("bytes still protected by the `part:` records"), so comparing by chunk id is right. Paging
  of both ranges is pinned (the `paged` case, and G-sparse's segment on part 19,999).
- **False positives on legitimate history.** A rollback files at `E` and lands `Open@E+1`. A
  later fence of that `Open` checks `E+1`, which is empty. A root flip lands `Completed`
  (`Settled`). I found no legitimate state that `recheck_fenced` names wrongly.
- C4-diff-cov's "does not apply on origin/main" is a harness limit (the base includes
  child-1..3), not evidence against the fix. C5 reports 0 missed.
