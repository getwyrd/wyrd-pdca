# dst: seeded Tier-0 coverage for the restore session fence (809.5)

> Child 5 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). This bundle's base is `origin/main` **plus children 1–4**:
> both fence shapes are on it. Locate them by symbol.

- **Slug:** restore-fence-dst
- **Kind:** enhancement
- **Defect:** the restore session fence (children 3 and 4) is a new path. It races concurrent
  writers on the session record and installs obligations that lead to deletes, yet it has no
  seeded Tier-0 DST coverage, only scripted in-process interleavings. The repo's rule is "a new
  destructive or concurrent path lands with seeded Tier-0 DST coverage" (`AGENTS.md:188-190`).
  The existing restore properties (`crates/dst/tests/custodian.rs:1954-2200`) run the pass
  against ordinary inode publication, never a multipart session. The staged-handoff driver
  (`:2617-2625`, `:2850`) has only GC and reconstruction arms. #809 iteration 1's review raised
  this three times as blocking.
- **Success criterion:** `crates/dst/tests/custodian.rs` gains seeded properties that run the
  production `reconcile_after_restore` fence, registered with `dst_campaign_test!` and swept over
  the run seed (50 seeds under `cargo xtask dst`):
  **(D1) The fence and a concurrent session writer never both win, and neither wins by half.**
  Two arms, each with a writer landing at a seed-drawn instant during the pass. `E` is the
  contested preimage epoch; every transition out of it lands at `E+1` (the state machine,
  `0016:538-552`).
  - *Open arm.* A resurrected `Open@E` session. The writer is the Complete fence
    `Open@E → Completing@E+1` (`0016:704-708`, `multipart.rs:2173`): one commit preconditioned
    on the session's exact `Open@E` bytes, stamping `fenced_at_millis`, `segments_written` and
    `publish_target` and bumping `attempts` (0016's Complete-fence row, `0016:660`), installing
    no obligation. The nonce is already on the record (child-2
    puts `segment_nonce` on every session record) and the writer leaves it unchanged.
  - *Completing arm.* A resurrected `Completing@E` session with `seg:` records. The writer is
    the root flip `Completing@E → Completed@E+1`, faithful to 0016's row (`0016:662`): one
    commit preconditioned on the session's exact `Completing@E` bytes that writes the session,
    the published inode and its dirent, and the flip's own `retire:records:s:<id>:<E>`
    `{parts: <the published set>}` (`multipart.rs:3148`). The fixture names every part, so no
    `retire:bytes:{parts}`.
  On every seed, judged from what the pass's own read of that session returned (recorded by the
  store wrapper, see Production reach), exactly one transition out of `@E` landed:
  - *the fence won:* the session is `Aborting@E+1`; every obligation children 3 and 4 install
    under token `s:<id>:<E>` is present and decodes through `decode_retire_obligation` with the
    **fence's** payload (`{session, all}`, plus `{seg: (nonce, E)}` in the Completing arm); and
    nothing was published;
  - *the writer won after the pass read `@E`* (its fence commit was refused): the writer's state
    stands, every `retire:` value under token `E` is the writer's own (none in the Open arm, the
    flip's `{parts}` in the Completing arm) and never a fence payload, and the pass named the
    session;
  - *the writer won before the pass read the session:* the pass treats the writer's state as it
    treats any session: in the Open arm it fences `Completing@E+1` as child-4 specifies
    (`Aborting@E+2`, obligations under token `E+1`); in the Completing arm it leaves
    `Completed@E+1` untouched and does not name it.
  Never an obligation without its transition, a transition without its obligations, or a fence
  payload under a key the writer's transition owns. Every obligation is checked by payload, not
  by key alone.
  **(D2) An ambiguous fence commit is settled by the next pass.** The fence's commit is answered
  as an unknown outcome and is applied whole or not at all, by a seed-drawn fate. The pass returns
  `Err`, and the `DANGLING` line for a dangling chunk seeded in the same store was emitted before
  it. A second pass leaves the session fenced with exactly one set of obligations, whichever fate
  the store took.
  **(D3) The contested window is reached, and shown to be reached.** A reachability leg walks the
  writer's landing span in one run, as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2165-2195`), and asserts on
  the **recorded** order, not on outcome counts: for **each** arm, at least one landing point
  where the pass's read of the session returned the `@E` preimage **and** its fence commit was
  then answered `Conflict` because the writer had landed in between (a stale-preimage conflict),
  plus at least one where the fence won. Two winners alone do not prove the window was
  contested: serial runs produce both. A property that never reaches its interleaving proves
  nothing.
  **(D4) Demonstrated falsifiability, recorded in `build-notes.md`.** Temporarily break the fence
  on the builder's machine, **twice**, one mutation at a time: (a) drop its precondition on the
  session bytes, which at least one seed of D1 must then fail; (b) split its transition and
  obligations into two commits, which at least one seed of D1 or D2 must then fail. Paste each
  seed and failure into `build-notes.md`, then restore the fence. None of the breakage ships.
  Mutation (a) is the one D1 and D3 exist for, so it is not optional.
  **(L) `cargo xtask ci` green**, with every existing property in the file unchanged in what it
  asserts. A span constant may move if the longer pass needs it; say so if one does.
- **Falsifiability:** no gate can produce a RED for this child. It adds tests only, over a fence
  already on its base, and a test-only patch has no production change for C4-verify to revert.
  D4 is where the red is shown: on the builder's machine, by breaking the fence on purpose. Check
  can re-run it. The harness is madsim (`--cfg madsim`, ADR-0009), and the gate sets that cfg
  for `crates/dst` (`engine/scripts/run-verify.sh:21-27`, `:155-180`).
- **Verification posture:** (a) net-new coverage: "red" is the property's absence. C4-verify
  runs **green-only** for this patch. It modifies an existing test file and adds none, so the gate
  runs the whole `wyrd-dst` crate under `--cfg madsim` with 50 seeds and passes on green
  (confirmed with `run-verify.sh --classify` on a synthetic patch, 2026-09-29: `CRATE crates/dst`,
  no `ADDED_TEST`). Do NOT add a new file under `crates/dst/tests/`. The gate would keep it on the
  red leg, find no production change to revert, see it pass, and FAIL the bundle ("passes
  without the fix"). What is built and exercised at Check: the properties themselves, over the
  production fence. The demonstrated red is D4's.
- **Invariant to restore:** every schedule the seed can draw between the restore fence and a
  concurrent session writer, or a commit whose outcome is unknown, ends with exactly one
  transition out of the contested epoch: the fence with all its obligations, or the writer with
  only its own (and named, when its landing refused the fence). Never half of either, and never a
  fence payload under a key the writer's transition owns. Source:
  `AGENTS.md:188-190` (test fidelity), `:178-180` (unknown commit outcomes); ADR-0009; 0016
  `:665`, `:717-728`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 842
- **Conflicts with:** 682, 722
- **Ordering note:** wave 4 of #809's split; it needs both fence shapes on its base. Conflicts
  with #682 and #722 because both edit `crates/dst/tests/custodian.rs` (added after acceptance,
  2026-09-29: the proposal format admits only sibling labels in that field).
- **Surfaces:** data
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** new seeded properties and their harness in `crates/dst/tests/custodian.rs`,
  mirroring the restore nemesis harness and the staged-handoff driver. Nothing outside that file.
  / out of scope: any production change. If a property finds a real defect in the fence, stop,
  record the seed and the failure in `build-notes.md` and report it, rather than fixing
  production here. Also out: any other test file; docs; any edit to 0016 or an ADR.
- **Repro instruction:** n/a (new coverage). On its base, `cargo xtask dst` passes, and no
  property in `crates/dst/tests/custodian.rs` runs `reconcile_after_restore` over a store holding
  an `mpu:` session.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/dst/tests/custodian.rs`, an EXISTING file, extended. See Verification
  posture for why it must not be a new one.
- **Production reach:** the fence under test is the production pass. The store is the DST
  crate's simulated TiKV store (`SimTikvMetadataStore`), wrapped in a forwarding tap that records
  what each session read returned and how each fence commit was answered, as `RecordingMeta`
  does (`:1821-1843`, used at `:1956`) and `AmbiguousSweepMeta` does for unknown outcomes
  (`:3974-3975`). **Not** bare `MemMeta` (`:106`): its `get`, `scan` and `commit` run under a
  mutex with no simulated hop (`:112-152`), so a concurrent writer can never land between the
  pass's read and its commit, and D1–D3 would pass without the race they exist for. Sessions are
  seeded because no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror, all in `crates/dst/tests/custodian.rs`:
  * `:1819` (`RESTORE_NEMESIS_SPAN`), `:1954-2140` (`restore_under_a_concurrent_writer`),
    `:2144-2200` (the two restore properties).
  * `:2617-2625` (`Driver`), `:2760-2850` (handoff session fixtures and `staged_handoffs_under`).
  * `:3907-4270` (property 16, the ambiguous-commit store double `AmbiguousSweepMeta` at `:3974`).
  * `:4998-5010` (`rand_seed`, `dst_campaign_test!`), `:5060-5200` (registrations and the seed
    sweep list).
  * The fence itself, by symbol on the base.
- **Prior-art check (triage cycles):** by path (`crates/dst/tests/custodian.rs`), 2026-09-29 on
  `243241e`. The last changes are `5377850` (#814), `f683dbe` (#813) and `dd81029` (the
  ambiguous sweep). #682 (AWAITING_SIGNOFF) and #722 (PLANNED) edit this file for segmented
  repoint and evacuation properties, and no open PR touches it. #809 iteration 1 only edited a
  fixture here and never ran the fence under DST.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-09-29): all four findings revised in place. (1) D1's oracle now
checks obligations by payload: when the root flip wins, its own `retire:records:{parts}` is
expected and only a fence payload is forbidden; the flip fixture follows 0016's row (session,
inode, dirent, obligation, one commit). (2) The Complete fence is now `Open@E → Completing@E+1`
(`0016:704-708`); D1 names `E` as the contested preimage and `E+1` as the winner's epoch, and
covers the writer landing before the pass reads (the pass then fences `Completing@E+1` itself).
(3) Production reach now requires `SimTikvMetadataStore` behind a recording tap, and says why
bare `MemMeta` cannot exercise the race. (4) D3 now asserts a recorded stale-preimage `Conflict`
per arm, and D4 requires both mutations, the dropped precondition included. Following #840's
placement A, the simulated Complete fence leaves the session's nonce as it is (it is on every
record) and bumps `attempts` as 0016's row does.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — `crates/dst/tests/custodian.rs:5660-5666`: when the writer wins, the test accepts `ChangedUnderPass | LostConflict | ObligationKeyTaken { .. }` as the reason the pass gives. That lets a wrong diagnosis through. **Concrete failing case:** change `restore.rs:844-856` to check the obligation keys *before* re-reading the session, so `ChangedUnderPass` is never reported. In the Completing arm the pass then reports `ObligationKeyTaken { retire:records:s:<id>:3 }`. That key is the root flip's own `{parts}` obligation, and the operator summary turns it into "whose retirement key was already taken by another obligation" (`crates/server/src/cli.rs:1484`, `:1494`) for an upload that simply published. The Open arm reports `LostConflict` instead. With this change all three new tests still pass, and so does the existing `crates/custodian/tests/restore_completing_fence.rs`. Only `restore_open_fence.rs:592` catches it, and only for the Open shape. This patch is the only test that models the flip sharing the fence's `retire:records:s:<id>:<E>` key, so it is the place to pin this. In this model, every writer-won schedule re-reads the session (`restore.rs:844`) after the writer has applied, so `ChangedUnderPass` is the only correct cause. I narrowed the match to `ChangedUnderPass` alone and re-ran on the **unmodified** fence: all three tests pass across 50 seeds. So the stricter check costs nothing and adds a real check.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
