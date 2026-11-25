# Build notes — issue 1010 (636.5d), iteration 3: no second fence after a rollback

Base: the stacked integration tip `db1a066` (`stack-base-tip`), which carries child-3 (#1009).
`patch.diff` is `git diff HEAD` of the worktree: iteration 2's patch, applied unchanged, plus the
changes below. Every `path:line` is on the patched worktree.

## What the carry-forward asked, and what changed

| Carry-forward item | Change |
|---|---|
| After an unknown initial fence, a re-read that finds the reaper's charged `Open@E+2` answers `OperationAborted`; never fence again with `request.fenced_at_millis`; a rival's give-back (`Open@2`, attempts 0) may still fence | `crates/core/src/multipart/complete.rs:955-969` (the check) and `:996-1008` (`open_and_charged_since`) |
| Update the module doc and the building-block view | module doc table and text `complete.rs:206-217`, the clock paragraph `:250-253`, `fence_complete`'s doc `:826-831`; `docs/design/architecture/05-building-block-view.md:252` (fence-phase paragraph) |
| The reviewer's probe as a regression (applied-unknown fence, rolled back to `Open@2` before the re-read, bucket and parts valid → `OperationAborted`, no second fence) | `g6_rolled_back_an_open_session_with_an_attempt_spent_since_is_never_fenced_again`, `crates/core/tests/multipart_complete.rs:6179-6284` |
| A `staged_wide()` run in G1 (`[Pass; 1 + SEGMENT_BATCHES]` + `Unknown { applied: true }`, `Published`, `assert_published_once`, `assert_every_segment_named`) | `g1_a_flip_that_landed_answered_unknown_is_published_once`, `multipart_complete.rs:5430-5474`, both `may_still_commit` values |

Plus, for the rule's other side: `g6_rolled_back_an_open_session_with_no_attempt_spent_since_is_fenced_again`
(`multipart_complete.rs:6286-6352`) and one wrapper step, `OnCommit::WriteAfter` (`:425-428`,
`:504-506`, `:639-647`). Test module doc: `:101-104`, `:124-128`.

The reviewer's probe file (`reviewer-fence-probe.rs`) is not in the bundle, so I rebuilt the
schedule from its description in `iteration-v2/check-review.md` (R1) and the adversary's notes:
real fence committed, conditional rollback to `Open@2, attempts 1`, then the unknown outcome. With
the fix reverted, my leg reproduces the probe's reported result exactly: `Held`, `Completing@3`,
`attempts 2`, `fenced_at_millis 2000` (`mut-no_fix.log`).

## The rule, and why it is shaped this way

After a fence commit of this call whose outcome is unknown, while that doubt stands
(`doubt.in_doubt` is `Some`), the re-read is checked, in order:

1. the fence's own bytes → the in-doubt error (unchanged, G5);
2. **new:** an `Open` session with more attempts spent than the session this fence was built from
   (`opened.attempts()`) → `Ok(Refused(OperationAborted))`, nothing written;
3. anything else → the loop, as before (an `Open` session with no attempt spent since is fenced
   again).

Why "more attempts than the read" and not "exactly `placed.charged`" (the adversary's sketch):

* Only `complete_fence` raises `attempts` (`multipart.rs:2689-2708`), and a fence comes down to
  `Open` either with the attempt given back (`fence_released(attempts read)`, the owner's release
  from cursor 0) or kept (the owner's release once a segment landed, `complete.rs` *The release
  once a segment landed*, or the reaper's rollback, which the brief's Design says keeps the attempt
  charged). A fence of this call whose commit answered unknown has no owner, so only the reaper
  takes it down, and that always leaves `attempts` above the read. So "more attempts" is exactly
  "a charged take-down happened since", which is the evidence that this fence may have been
  rolled back.
* `placed.charged` is only the first such shape. If the reaper rolls the fence back and a rival
  then fences and gives its attempt back before the re-read, the session is `Open@4, attempts 1`:
  the exact-bytes rule fences again with the stale stamp, which is the defect the human ruled out.
  Hand mutant `exact` (the exact-bytes rule) goes red on that case only (`mut-exact.log`).
* Cost is the same: the exact rule is one `if found.as_ref() == Some(&placed.charged)` line; this
  one is that line plus a 13-line helper (`complete.rs:996-1008`, doc included) that decodes the
  record. No cost argument either way, so I took the one that holds for every shape.
* A rival's give-back (`attempts` equal to the read) still fences again, as the human required:
  `g6_held_a_read_that_finds_the_session_moved_clears_a_held_fence` stays green, and the new
  "fenced again" leg pins it with a bucket that is still valid (so the request really fences, not
  just answers `NoSuchBucket` from the check before the fence).

**Decision for the human:** this is a little wider than the sign-off text ("the charged
`Open@E+2` shape"). It also answers `OperationAborted` for `Open@E+2k` with an attempt charged
since, which covers a rollback followed by other Completes. Every case it adds is one where this
request's fence may have been the one rolled back. If you want the narrow exact-bytes rule
instead, it is a one-line swap at `complete.rs:966`, and case 3 of the new leg (`Open@4`) would
then have to go.

Why answering is safe with a held doubt: an `Open` session at a higher `attempts` is at a higher
epoch than the `Open@E` every fence of this call requires, and the epoch never goes back
(`complete.rs:998-999`), so no batch of this call can land. That is also the module doc's own
rule for clearing a held doubt (`complete.rs:189-194`). Answering from a session at the attempts
cap gives `OperationAborted`, and the client's retry then gets `CompleteAttemptsExhausted`. That is
the same answer the publication phase already gives for an `Open` session after a rollback.

Not done (scope): the reaper (#625) is not built, so its exact bytes are not specified in code.
Both the new leg and G1-open model the rollback as `Open@E+2` with the attempt kept, conditioned
on the fence's bytes, as the brief's Design states. If #625 writes something else (gives the
attempt back), this rule would need to follow it; the helper's doc names the assumption.

## Red → green evidence

All runs: `timeout 1500 cargo test -p wyrd-core --test multipart_complete` in the worktree (each
leg also has the file's own 120 s `LEG_TIMEOUT`). Logs under `$PDCA_SCRATCH/pdca-builder-1010-redleg3/`.

* **Green:** 80 passed (78 from iteration 2, plus the 2 new legs). The G1 wide run is inside the
  existing G1 leg, so it adds no new test name.
* **Red, fix reverted** (`no_fix`: the new `if` at `complete.rs:966-968` removed): 79 pass, 1
  fail, `g6_rolled_back_..._never_fenced_again`, case "landed, rolled back, out of flight": the
  request answers `Held` after a second fence `Completing@3, attempts 2, fenced_at_millis 2000`.
  The other new leg (fenced again) passes before and after the fix, as it should: it pins the
  behaviour the human kept.
* **Red, exact-bytes rule** (`exact`): 1 fail, the same leg, case `Open@4`.
* **Red, `>` → `>=`** (`ge`): 6 fail, including both rival-give-back legs.
* **Red, the adversary's [impl] probe** (`flat_only_push`: `unknown_flips.push` only when the round
  writes no segment, around `complete.rs:1504`): 1 fail, `g1_a_flip_that_landed_...`,
  which answers `AlreadyCompleted` instead of `Published` in the wide run. Before this iteration
  that mutant survived all 78 legs.
* For `no_fix` and `exact` the unused helper trips the workspace's `-D dead-code`, so those two
  mutant copies carry `#[allow(dead_code)]` on it; nothing else differs.
* **cargo-mutants** (`scripts/mutants-in-diff` with this bundle, the C5 gate's own command):
  26 mutants, 20 caught, 6 unviable, 0 missed, 0 timeouts, rc 0. All six mutants of
  `open_and_charged_since` are caught (`-> true`, `-> false`, `&&`→`||`, `>`→`==`, `<`, `>=`).
  The 6 unviable are `Default::default()` stubs for types with no `Default`, as in iteration 2.

## Refuting my own test

* **(a) Genuine red?** Yes. Reverting the fix makes the probe leg fail with the reviewer's exact
  bad outcome (second fence, `Held`). The exact-bytes rule and `>=` each fail named legs. The G1
  wide run fails under the adversary's flat-only mutant, which no leg caught before.
* **(b) Production path?** Yes. The new legs call the production `fence_complete` (the probe's
  entry point) on a real `RedbMetadataStore` through the logging wrapper; the G1 wide run calls
  `complete_upload` with parts staged by the real `upload_part`. The wrapper only decides commit
  outcomes and writes other writers' records on the real store.
* **(c) Fixture includes the fault?** Yes. In cases 0-2 the fence really lands on the store, and
  the rollback is a conditional write that requires the fence's own bytes, so the wrapper's
  `assert_eq!(.., Committed)` (`multipart_complete.rs:641-645`) proves the fence was there
  before it was rolled back. The bucket marker and all three parts stay valid, so nothing but
  the rollback can stop the request. In the held case (4) the held fence is the real batch and
  `release_held` shows it can no longer land (`Conflict`).

## Rubric self-review

* One clock per lifecycle: no new clock read. The change stops a stamp from being reused after a
  rollback of its lifecycle, which is what the rule protects.
* Narrow seams: only `MetadataStore::get`/`commit`, as before.
* Absent/unsupported entries: an absent or torn record makes the helper return `false`, and the
  loop then answers `NoSuchUpload` or stops on the torn record (in doubt), as it did before.
* Transactions: `CommitUnknownResult` is still never read as `Conflict`; no new early return
  skips a release (this branch has no fence of its own to release).
* Test fidelity: the rollback is modelled as the brief's Design and G1-open already model it
  (reaper not yet built, #625). Tier-0 DST stays deferred to #660 (`complete.rs:792-799`).
* Docs currency: the module doc, `fence_complete`'s doc and the building-block view all say the
  new rule. No port, API, RPC, CLI flag or persisted field changes.

## Commit-readiness

`cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-core --all-targets` clean (workspace
denies warnings); `typos` clean on all four changed files. Full gate:
`PDCA_WORKTREE=<this worktree> ./engine/xtask.sh ci` ends `xtask ci: all checks passed`, rc 0
(fmt, clippy, build, tests including DST, deny, conformance, statics), with `multipart_complete`
at 80 passed inside it (`ci.log`). `patch.diff` is byte-identical to `git diff HEAD` of the
worktree and reverse-applies cleanly.

## External dependencies

`typos` (the brief's only one) is installed and passes. Nothing undeclared was needed.

## Size

The size backstop is overridden by the sign-off; no split. Against iteration 2, `complete.rs`
gains 42 lines and loses 11 (code, comments and module doc), the building-block view gains part
of one paragraph, and the test file grows by 263 lines (6537 → 6800).

## Scratch

Logs, the mutant script and the saved copies are under
`$PDCA_SCRATCH/pdca-builder-1010-redleg3/`. `cargo mutants` left `mutants.out/` (git-ignored) in the
worktree. I left removal of both to the harness.
