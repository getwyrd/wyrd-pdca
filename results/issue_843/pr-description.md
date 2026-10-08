## Summary
**User impact:** after an operator restores the metadata store from an older image, a
cleanup step closes any upload that came back from that image and records which data must
later be deleted. A client finishing or publishing that same upload can act at the same
moment. If the two ever both "won", or one won only halfway, data could be deleted that a
published object still needs, or left behind forever. That step had no randomized
concurrency testing, only a few hand-scripted orderings. No client can create multipart
uploads yet, so nobody is exposed today; this adds the testing before they can.

This PR adds seeded simulation tests that race the real post-restore step against a
concurrent upload writer and against a store that loses track of whether a write landed.
Tests only; no production code changes.

This PR is stacked on #846, #847, #851 and #856 (the earlier parts of the same restore
work); its diff reads cleanly once those land.

## What to look at
- One file changes: the custodian DST suite (deterministic simulation tests, run under
  madsim with a seed). It gains a recording wrapper around the simulated TiKV store, a
  fixture that seeds a restored upload session, and three properties registered with the
  existing campaign.
- The three properties, in plain terms:
  1. A writer lands at a random moment during the restore step. Exactly one side wins,
     with all of its records and none of the other side's.
  2. The restore step's write comes back "outcome unknown". Whatever the store really did,
     the next run settles it to exactly one closed session with one set of cleanup records.
  3. A walk over every landing moment proves the tests actually hit the race (the writer
     slipping in between the step's read and its write), for both session shapes.
- To try it: `cargo xtask dst` runs them at 50 seeds. To see them catch a bug, drop the
  session-bytes precondition in `fence_session` (`crates/custodian/src/restore.rs`) and
  re-run: the first property fails.

## Root cause
The restore fence (`fence_session` / `plan_fence` in `crates/custodian/src/restore.rs`,
from #851 and #856) commits a session transition and its `retire:` obligations in one
batch, preconditioned on the session bytes it read, and so races any concurrent writer on
the `mpu:` record. The repo rule is that a new destructive or concurrent path lands with
seeded Tier-0 DST coverage (`AGENTS.md:188-190`); the existing restore properties ran the
pass only against ordinary inode publication, never a multipart session.

## Fix
All in `crates/dst/tests/custodian.rs` (line numbers on this branch):
- **`FenceMeta`** (`:5099-5230`): a forwarding tap over `SimTikvMetadataStore`, hops
  included. It logs what each `mpu:` listing and each point read returned for the session,
  and how each commit writing the session was answered. With a strike fate set, the
  pass's first such commit is answered `CommitUnknownResult` and applied whole or not at
  all, as `AmbiguousSweepMeta` does. Bare `MemMeta` was not used: it never yields, so no
  writer could land between the pass's read and its commit.
- **Fixtures** (`seed_restored_session`, `:5274`): an `Open@3` or `Completing@3` session
  with one committed part, the attempt's `seg:` record, and a dangling chunk. The two
  writers follow the multipart design's rows: the Complete fence `Open@3 → Completing@4`
  (no obligation), and the root flip `Completing@3 → Completed@4`, which writes the session,
  inode, dirent and its own `retire:records:s:<id>:3 {parts}` in one commit.
- **`assert_owed`** (`:5440`): the whole `retire:` namespace must equal the expected key
  set, and every value is decoded with the production `decode_retire_obligation` and
  compared by payload.
- **Properties:** `prop_restore_fence_never_shares_the_epoch` (`:5748`),
  `prop_restore_fence_settles_an_ambiguous_commit` (`:5857`),
  `prop_restore_fence_reaches_the_contested_window` (`:5877`), registered with
  `dst_campaign_test!` (`:6069-6085`). The two seeded legs are also appended to
  `committed_regression_seeds_stay_green`, after every existing property, so no existing
  property's random draws change.
- Two new constants, `RESTORE_FENCE_SPAN = 14` (`:5046`) and `RESTORE_FENCE_DRAWS = 4`
  (`:5051`). No existing constant or assertion changed.

## Verification
- **Claim:** the fence and a concurrent session writer never both win, and neither wins by
  half, in both the Open and Completing shapes.
  - **Checked:** the core "exactly one transition out of `@E`" assertion
    (`custodian.rs:5585`), judged from the pass's own recorded read; per-outcome state and
    obligation checks follow.
  - **Test:** `prop_restore_fence_never_shares_the_epoch`. Fails at `MADSIM_TEST_SEED=843008`
    when the fence's `require` on the session bytes is dropped (`restore.rs:824`): both
    commits land in the Open arm.
- **Claim:** when the writer wins after the pass read the session, the pass names it as
  `ChangedUnderPass`, and that diagnosis comes from re-reading the session.
  - **Checked:** exact cause match at `custodian.rs:5671-5678`, then the recorded re-read
    returned the writer's state (`:5679-5689`). This matters in the Completing arm, where
    the root flip owns the same `retire:records:s:<id>:<E>` key the fence wants; a wrong
    diagnosis would tell the operator a cleanup key "was already taken" for an upload that
    simply published (`crates/server/src/cli.rs:1484`, `:1494`).
  - **Test:** fails at `MADSIM_TEST_SEED=843009` when `restore.rs:841-856` is changed to
    check obligation keys before re-reading the session. A looser three-way match passed
    under the same change, so the exact match is what catches it.
- **Claim:** an ambiguous fence commit is settled by the next pass, and the `DANGLING` line
  for the seeded dangling chunk is emitted before the pass's `Err`.
  - **Checked:** `restore_fence_after_an_ambiguous_commit` (`custodian.rs:5760`).
  - **Test:** `prop_restore_fence_settles_an_ambiguous_commit`. Fails at
    `MADSIM_TEST_SEED=843000` when the transition and its obligations are split into two
    commits: the session lands as `Aborting@4` with an empty `retire:` namespace.
- **Claim:** the race window is reached, not assumed.
  - **Checked:** `custodian.rs:5877`, which walks all 15 landing points per arm and asserts
    from the recorded order at least one stale-preimage `Conflict` and one fence win per
    arm. In the Completing arm the fence would also lose on the flip's `retire:records:` key
    without the session precondition; the doc comment says so (`:5873-5876`).
  - **Test:** fails deterministically under both the dropped-precondition and the
    wrong-diagnosis mutations above.
- **No red before this PR:** this is new coverage over a fence that is already correct, so
  there is no pre-fix failure to show. The red is the three mutations above, each applied
  alone to the production fence and then reverted.
- **Whole gate:** `cargo xtask ci` passes, including the DST crate under `--cfg madsim` at
  50 seeds (31/31 in `custodian.rs`, the 28 existing properties unchanged). The new
  properties also pass on the unmodified fence across seed ranges starting at 1, 1000, 5000
  and 843000, and an independent run at 1000 seeds.

**Follow-up (not in this PR):** `crates/custodian/src/restore.rs:785` still carries a
`deferred: #843` marker, and the DST pin list at `restore.rs:478-482` should name these new
properties. Both are production-file edits, left out to keep this PR test-only.

Fixes #843
