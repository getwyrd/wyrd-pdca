# Adversarial review — #895 (636.3b), iteration 3

Scope of this round: the sign-off allowed one new test, pinning that a held "may still land"
doubt survives a failed read inside the Conflict branch. I tried to refute that test, the rest of
the bundle's production path, and the gate verdicts. I re-ran the evidence myself: I copied the
target into scratch, ran a baseline (24/24 pass), then ran 23 hand-written mutants of
`crates/core/src/multipart/abort.rs` against `crates/core/tests/multipart_abort.rs`.

## Findings

- **Could not refute this round's test.** `crates/core/tests/multipart_abort.rs:1112`
  (`i_a_held_fence_survives_a_failed_read_after_an_unexplained_conflict`) goes red on the
  sign-off's mutant and green on the patch. With `crates/core/src/multipart/abort.rs:337`
  changed from `doubt.end(upload_id, Stop::Store(fault))` to a plain `AbortError::Stopped`,
  **only** that test fails (23 pass, 1 fail). It fails at the `in_doubt` helper (`:333`) with
  `expected InDoubt naming …0001, got Err(Stopped { …, cause: Store("the test wrapper's store failed") })`.
  It drives the real `abort_session` over real redb. The only thing scripted is the commit
  answer (`Hold`, then `ConflictUnmoved`) and the `mpu:` read failing after two commits. So the
  failing read can only be `lost()`'s first session read (`abort.rs:376`): no reads come after a
  Conflict except inside `lost()`. The test is not a copy of production, and it does not pass by
  accident. It also checks that the held batch released afterwards lands (`:1138-1139`).

- **One doubt-keeping arm is still unpinned (informational; production is correct).** Changing
  `abort.rs:335` from `doubt.end(upload_id, Stop::ObligationKeyTaken { key })` to a plain
  `AbortError::Stopped { cause: ObligationKeyTaken }` passes all 24 tests. H3 (`:991`) covers
  "taken key" only when nothing is in doubt. The case that tells them apart:
  1. Round 1 is `Hold` on `Open@0`.
  2. The re-read finds the session unchanged.
  3. A squatter is planted at `retire:bytes:s:<id>:0`.
  4. Round 2 gets a real Conflict, and `lost()` returns `Taken`.

  Production answers `InDoubt { may_still_commit: true, cause: ObligationKeyTaken }`. The mutant
  answers "none landed, and none can land later". That is false only if the squatter is later
  removed and the held batch then lands. The held batch `require_absent`s that key, so the
  real-world risk is small. This is the same kind of gap as last round's item (one `doubt.end`
  arm with no test). I am **not** filing it as NEEDS-HUMAN: the human limited this round to one
  test, and the rubric says not to chase silence. If wanted later, the test needs one new wrapper
  action, "plant this key, then pass the commit".

- **The other survivors are equivalent mutants, not gaps.** Making `abort.rs:291` (first read),
  `:304` (torn record) or `:312` (epoch exhausted) return a plain `Stopped` passes all tests, and
  it should:
  - `:291`: the doubt is still empty at the first read.
  - `:304` and `:312`: `Doubt::settle` (`abort.rs:422-431`) clears `held` on any read that is not
    the required bytes. Torn bytes always differ from them. A held fence can never require
    `Open@u64::MAX` bytes, because `open_teardown` refuses that epoch.

  Always replacing `held` with the latest outcome (`abort.rs:409`) changes only which of two
  equal `may_still_commit: true` reports is shown. I counted 23 mutants in total:
  - 16 were caught, and each failed the leg named for it (H1/H2/H4/I/I2/D/E/J). This includes the
    brief's five demonstrated reds: the `mpuctl` put, the attempt-4 "unchanged means taken" rule,
    no third read, the held doubt clobbered by the latest outcome, and a settle that is a no-op.
  - 6 survived as equivalent: the four above, plus two more (`settle` skipped when the read is
    `None`, and an extra condition in `observe` that never changes anything).
  - 1 survived and is not equivalent: `:335`, above.

- **Restore's refactor holds.** `crates/custodian/src/restore.rs:1055` calls
  `fence_batch(key, read, &fence.session, &fence.obligations)`. It builds the same `require` on
  the exact bytes, `require_absent` per key in order, and puts in the same order as the removed
  inline code. The J leg (`multipart_abort.rs:1311`) pins that shape for the two-obligation
  `Completing` case. In `gate-logs/C4-ci.log`, the workspace tests and the madsim DST run
  (`:3846` onward, including the custodian DST) passed with no restore test edited. Could not
  refute.

- **Other attacks that failed.**
  - Top-level `downcast_ref::<CommitUnknownResult>()` (`abort.rs:341`) matches the seam contract
    (`crates/traits/src/lib.rs:216`). The adapters return that type unwrapped
    (`crates/metadata-fdb/src/lib.rs:1711`, `crates/metadata-tikv/src/lib.rs:898`, `:977`).
  - Unbounded awaits are covered: the store contract says every operation terminates
    (`crates/traits/src/lib.rs:1334`), and every loop pass either returns or spends a round.
  - The "Fenced instead of AlreadyAborting" false positive from byte-identical rival fences is
    now stated correctly in `docs/design/architecture/05-building-block-view.md:206`.

- **The brief's verification posture is out of date (no defect).** It says `cargo xtask ci` stops
  at cargo-deny before the DST step. This round's `gate-logs/C4-ci.log:3591-3597` shows deny
  passing. At `:4147-4157` the new DST file `crates/dst/tests/multipart_abort.rs` ran (6 passed,
  including the `should_panic` red at `:625`). So (G) is now exercised by the gating CI row, not
  only by Do's own runs.

- **C4-diff-cov measured nothing.** It reports `fail` because `patch.diff does not apply on
  origin/main` (`gate-logs/C4-diff-cov.log`). That is the stack on #894, not a coverage result,
  and it has been the same in all three rounds. My mutation run above stands in for it on
  `abort.rs`. It covers neither `restore.rs` nor the DST file. Not a refutation.

- NEEDS-HUMAN [human] — `check-gates.json` row `T4-batch-review` passes as **gating** in 13.03 s
  ("3x codex, union, triaged", `0 blocking, 0 recorded-rejected, 0 noise-dropped`), on a
  2,606-line bundle diff. `gate-logs/T4-batch-review.log` holds one summary line and no proof that
  three fresh reviews of this round's diff ran. A cached result from an earlier diff, or a review
  that read nothing, would give the same line. Please confirm that
  `results/issue_895/review-batch.md` was produced from this round's `patch.diff` before counting
  it as the rubric's "one deep, multi-pass review".

Bottom line: I tried to refute the new test (red→green, real path, right reason), the production
doubt handling (23 mutants), restore's refactor, and the gate verdicts. The only real gap is
untested and low-risk (`abort.rs:335`). The only open question is whether the T4 review row
really reviewed this diff.
<!-- pdca:leaf-complete -->
