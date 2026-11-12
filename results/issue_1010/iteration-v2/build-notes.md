# Build notes — issue 1010 (636.5d), iteration 2: close the test gaps

Base: the stacked integration tip `db1a066` (`stack-base`), which carries child-3 (#1009).
Every `path:line` below is on the patched worktree unless it says `base`.

## What this iteration changes, and what it does not

The sign-off said the production logic holds and the brief's design stands; the gaps are in
the tests. So `patch.diff` keeps iteration 1's production change byte for byte
(`crates/core/src/multipart/complete.rs`, `crates/core/src/multipart.rs`,
`docs/design/architecture/05-building-block-view.md`; iteration 1's notes describe it), and
everything new is in `crates/core/tests/multipart_complete.rs`:

* **Wrapper support** (the carry-forward's "wrapper support needed for the two pin rules"):
  * `OnCommit::Hold` now queues (doc `:398-402`): `Script.held` is a `VecDeque<WriteBatch>`
    (`:515`), `Hold` pushes (`:596-599`), `Logged::release_held` lands the oldest (`:552-555`).
    The old single slot asserted "one batch held at a time"; the re-pin leg and the first-held
    leg need two held batches at once. Shape still that of `multipart_abort.rs:961-980`.
  * `OnCommit::Land(step)` (`:403-406`, `:600-607`, helper `OnCommit::land` `:498`): land the
    oldest held batch on the inner store (asserting `Committed`), then do the step. It is the
    only way to land a held batch *after* the read that pins it and *before* the next read,
    which is what the pin-clearing rule needs.
* **G5 can no longer hang** (`then_open`, `:459-470`; used at `:5781-5788`, `:5815`, `:5835`).
  Each of G5's two requests opens all three gates when it returns. That is what C5's two
  TIMEOUT mutants were (`complete.rs:891:34` and `:919:19`): both make every Complete stop
  before its first commit, so G5's other request waited on a gate forever, the test binary ran
  into `LEG_TIMEOUT` (120 s), and cargo-mutants' 29-30 s limit recorded TIMEOUT. With the fix
  they fail fast and are **caught**. With the production code the extra openings do nothing:
  each gate has one wait, and a `Notify` holds at most one permit.
* **Nine new legs** (`:5959` on), one per rule the reviewers found unpinned. Module doc
  updated (`:93-138`).

## Carry-forward item → leg → evidence

Each reviewer probe became a regression leg. "Hand mutant" = the reviewer's mutation applied by
`mutate.py` (in the scratch dir) to the fixed `complete.rs`, then the whole test file run. In
every case only the new legs go red; no leg from iteration 1 catches any of these mutants,
which is the gap the reviewers reported.

| Carry-forward item | Leg (`multipart_complete.rs`) | Goes red under the mutant |
|---|---|---|
| `Doubt::holds -> false` (`complete.rs:1833-1835`, used at `:912`): held first fence + bucket deleted + conflicting retries → InDoubt, held fence lands after | **G6-held** `:5985`, both cases: retries `Conflict` → `InDoubt{Fence{1}, true}` after 4 fence commits, then `release_held` → `Committed`; retries land → `NoSuchBucket` after fence, fence again, release, then `release_held` → `Conflict` (the adversary's variant) | G6-held. cargo-mutants `:1834:9 replace Doubt::holds -> bool with false`: **caught** (was MISSED) |
| fence `stop` ignoring the doubt (`:867`, read at `:943`): failed `mpu:` read after an unknown fence with `may_still_commit: true` → `InDoubt{Fence, true}`, not `Stopped{NotPlaced}` | **G3-fence** `:6104`: applied+`false`, held, and held-then-out-of-flight. Each → `InDoubt{Fence{1}, ..}`, 1-2 commits; the held cases land on `release_held` | G3-fence and G6-held (its `FenceRoundsSpent` stop) |
| no `doubt.settle` at `:877`: unapplied unknown, `false`, then conflicts → `Stopped{NotPlaced, FenceRoundsSpent}` | **G6-spent** `:6161` | G6-spent, G6-held (moved case) |
| no doubt check before a definite refusal (`:1481-1483`): `[Pass, Hold, Fail{transient}]` → `InDoubt{Flip, true}`, no release | **G2-refused** `:6199`: 3 commits, fence still stands, then the held flip lands and publishes once | G2-refused |
| `Doubt::in_doubt` ordering (`:1862-1867`): first held outcome is kept | **G2-first** `:6259`, flat case `[Pass, Hold, arm(FailGets(mpu:), Unknown{false,false})]` → `may_still_commit: true` | G2-first, G3-fence (third case) |
| `Doubt::settle` no-op / no settle at `:1525`: unapplied unknown flip, unchanged re-read, conflicts, release → `Refused(OperationAborted)`, session `Open` | **G2-settled** `:6355`: 5 commits, `assert_released_with_nothing_published` (Open@2, attempt given back) | G2-settled, G4-landed. cargo-mutants `:1842:9 replace Doubt::settle with ()`: **caught** (was MISSED) |
| pin rule: clear a pinned held doubt once a read finds the cursor moved (`:1847`) | **G4-landed** `:6404` (wide fixture: held segment batch; next round's commit step lands it and removes the bucket; that batch conflicts; read finds cursor at the committer's count → doubt cleared → release from there + `NoSuchBucket`). Fence-phase twin: **G6-held** moved case `:6059` (held fence, session moved to `Open@2` → doubt cleared → the check before the fence answers, 1 commit) | hand mutant `kept = true`: G4-landed, G6-held moved. cargo-mutants `:1847:46 replace == with !=`: caught |
| pin rule: re-pin on a second held outcome (`:1826`) | **G4-repin** `:6463` (deep fixture: round 1 batch 1 held; round 2 batch 1 lands (cursor k), batch 2 held after the bucket is removed → `InDoubt{Publication{1}, true}`, no release; afterwards held batch 1 → `Conflict`, held batch 2 → `Committed`) | hand mutant "keep old pin": G4-repin (the mutant releases at k and answers `NoSuchBucket`) |

One leg goes beyond the carry-forward: G2-first's **wide** case holds two flips (round 1's is
about a round with a segment batch, `Publication`; round 2's about the flip alone, `Flip`) and
requires the error to name the first. That is the brief's Scope rule "the first
`may_still_commit: true` outcome is kept, as `abort.rs` does", which no leg pinned against a
*later held* outcome. A hand mutant that lets a later held outcome replace the first one
(drop the `else if` branch at `complete.rs:1822-1826`) is red on G2-first alone, for the right
reason: the error names "its root flip" where the first held outcome was the publication round
(`mut-latest_held_replaces.log`).

## Red → green evidence

* **Green**: `cargo test -p wyrd-core --test multipart_complete` → 78 passed (69 + 9).
  Full gate `./engine/xtask.sh ci` (with `PDCA_WORKTREE` = this worktree): `xtask ci: all
  checks passed`, rc 0; `multipart_complete` 78 passed inside it. Log: `ci.log`.
* **No flakiness seen**: G5, every G2/G3-fence/G4/G6 leg and the nine new legs, 25 repeat runs,
  25 passed.
* **Red on unpatched production** (base `complete.rs` and `multipart.rs` restored, new test
  file kept): 14 FAIL, 64 pass. The 14 are G1, G1-open, G2, G4, G6 (as in iteration 1) and all
  nine new legs. G3 and G5 still pass on base, as in iteration 1: the base returns the in-doubt
  error at once on every unknown outcome, which is the cautious answer those legs accept. Their
  reds are the brief's demonstrated breakages, shown in iteration 1 and unchanged. Log:
  `base-red.log`.
* **Mutants**: `cargo mutants --in-diff <this patch> --no-shuffle --cap-lints=true` (the
  C5 gate's own invocation, `scripts/mutants-in-diff`): **20 mutants: 14 caught, 6 unviable,
  0 missed, 0 timeouts, exit 0.** Iteration 1: 2 missed, 2 timeouts, exit 3. The 6 unviable
  are `Ok(Default::default())` / `Default::default()` replacements for types with no
  `Default`; they do not compile. Log: `mutants.log`, `mutants-out/`.
* **Hand mutants** (the reviewers' probes cargo-mutants does not generate, plus the
  later-held one): all nine are red, each only on the new legs named above. Logs:
  `mut-<name>.log`.

## Refuting my own test

* **(a) Genuine red?** Yes. With production reverted to base, all nine new legs fail (14 fail
  in all). With each reviewer mutant applied to the fixed production, the named new leg fails.
  cargo-mutants finds no survivor. G3 and G5 do not fail on base, and I say so above; their red
  is the iteration-1 breakages (M3, M5).
* **(b) Production path?** Yes. Every leg calls the production `complete_upload` /
  `fence_complete` on a real `RedbMetadataStore` through the logging wrapper. Parts are staged
  by the real `upload_part` (flat and wide legs) or planted as canonical records (fence-only
  legs, the deep fixture), as iteration 1's legs do. The wrapper only chooses commit outcomes
  and order; `Land` and `release_held` hand the real held batch to the real store, which
  decides `Committed` or `Conflict`.
* **(c) Fixture includes the fault?** Yes. Each held batch is the batch the Complete sent, kept
  and later applied for real. Each leg proves the batch could still land (`release_held` →
  `Committed` in G6-held, G3-fence, G2-refused, G2-first, G4-repin) or could no longer land
  (`Conflict` in G6-held, G2-first, G4-repin). Bucket removal and the session moving to
  `Open@2` are real writes on the inner store, at the commit point. Failed reads are armed on
  the real read path (`Arm::FailGets`).

## Decisions and alternatives (for the human)

1. **No production change.** The sign-off said the logic holds, and every mutant the reviewers
   named is now killed by a test, not by a code change. I left the `// deferred: #660` comment
   at `complete.rs:783-790` alone: it lists the kinds of interference the integration test
   forces, and the new legs add held fences and held segment batches it does not name. It is
   incomplete, not wrong. A one-line addition there is the only edit I would add if you want it.
2. **Queue of held batches, not a single slot.** About 4 changed lines (field type,
   `push_back`, `pop_front`, drop the assert). The other option, dropping or overwriting the
   first held batch, could not show that batch then conflicts, so G4-repin and G2-first would
   lose half of what they prove.
3. **`Land` as a commit step, not an armed read.** The clearing rule needs a read that pins the
   held bytes, then the batch landing, then a later read. The only hook between the request's
   read and its next commit is that commit, so the step lands there. Landing on the next read
   (an `Arm` variant) would land the batch *before* the pinning read, so the pin would be taken
   at the moved bytes and the clearing rule never run.
4. **G5's hang fixed by opening gates on return (`then_open`, 10 lines), not by timeouts on
   `Gate::wait`.** A wait timeout short enough to beat cargo-mutants' ~30 s limit would turn a
   slow CI run into a spurious failure. Routing each request's commits to its own script (a
   task-local request id in the wrapper) would also remove the hang, but rewrites G5's
   scripting and the wrapper's dispatch; I estimate 50+ lines for no extra coverage.
5. **The size backstop is overridden (sign-off), so no split.** The test file grows by 632
   lines over iteration 1 (5905 → 6537); `patch.diff` from 1827 to 2466 lines. The production
   hunks are identical to iteration 1's (checked by diffing them).

Human decisions recorded at sign-off and kept: `OperationAborted` when the re-read finds `Open`
(including G1-open) is approved; the C4-diff-cov / green-only C4-verify rows are a stacked-base
artifact, cleared.

## External dependencies

`typos` (the brief's only one) is installed and passes on every changed file. Nothing
undeclared was needed: cargo-mutants and the gate runner were already present.

## Commit-readiness

`cargo fmt --all` applied and `--check` clean; `cargo clippy -p wyrd-core --all-targets` clean
(workspace lints deny warnings); `typos` clean on all four files; `./engine/xtask.sh ci` passed
(fmt, clippy, build, tests incl. DST, deny, conformance, statics). `patch.diff` is
`git diff HEAD` of the worktree and passes `git apply --check --cached` against the base.

## Scratch

Logs, the hand-mutant script and the base/fixed copies are under
`$PDCA_SCRATCH/pdca-builder-1010-redleg2/`, inside the harness's scratch root. I left removal
to the harness.
