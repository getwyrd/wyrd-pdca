# Adversarial review — #894 (multipart admission + Create)

**Verdict: the fix holds where its tests point, but parts of its own contract are unpinned.**
I re-ran the proof on a scratch copy of the patched tree. The 19 core tests and 6 DST
(deterministic simulation test) tests are green. Every required red in the brief reproduces
when I break the code on purpose: the C4-verify gate is UNVERIFIABLE only because the test
cannot compile without the new API. I found no input that breaks the shipped Create logic
today. I did find five deliberate breaks of rules the brief names that pass all 25 tests,
plus one seam-contract bend in the new error type. All six findings are about tests or the
error contract, not about wrong answers on the paths that are tested.

## Findings

- NEEDS-HUMAN [impl] — **Only part of the "doubt is kept" contract is tested.** The brief
  (Design, "the doubt is kept") says that once a batch may still land, three things must not
  clear the doubt: a later out-of-flight outcome, a failed settling read, or a plain store
  fault. Only the first is tested (F6). The other two live at
  `crates/core/src/multipart/create.rs:300` (a read fault after an unknown outcome), `:393`
  (a definite commit fault after an earlier unknown outcome) and `:500` (the settling read in
  `finish` fails). The C4-diff-cov gate already lists all three lines as never executed. I
  replaced `doubt` with `Doubt::default()` at `:300` and at `:393`, and made `:500` reset the
  doubt. All three breaks pass all 19 core tests and all 6 DST tests. The simulated store
  never returns a plain read or commit fault, so the DST cannot reach these paths. Concrete
  failing case for the `:393` break: commit 1 returns
  `CommitUnknownResult { may_still_commit: true }` and stays in flight. Commit 2 fails with a
  plain store error. The create answers `CreateError::Store` ("No batch of this create is in
  doubt", `:101`), and then the held batch lands. That leaves a counted session no caller was
  told about, which breaks C-1. Fix: give `Scripted` a plain-`Err` commit action and a
  failing-`get` hook. Then add a test for each path that asserts
  `InDoubt { may_still_commit: true }` with the original identity.

- NEEDS-HUMAN [impl] — **Nothing tests `require_absent(seggrp:<nonce>)` in the Create batch**
  (`crates/core/src/multipart/create.rs:557`). Deleting that line passes all 25 tests. The
  nonce-collision test (`c_a_colliding_identity_is_re_minted`) only catches a nonce already
  present at read time. The "collision arriving as a lost commit" leg
  (`crates/core/tests/multipart_create.rs:1212`, `PlantSessionFirst` at `:763`) plants only
  `mpu:<id>`. Concrete failing case: another session takes this nonce between the create's
  read and its commit. Without the precondition, both batches land, and the second `put` of
  `{}` silently takes over the first session's `seggrp:` marker. Whichever terminal delete
  runs first (#659) then removes the other session's reservation. Fix: add a
  `PlantGroupFirst` leg beside `PlantSessionFirst`.

- NEEDS-HUMAN [impl] — **F3 doesn't test what its assertion says.**
  `crates/core/tests/multipart_create.rs:1362-1371` asserts `late == Conflict` with the
  message "the late batch lost its require_absent(mpu:<id>)". But the store starts with no
  ledger, so the held batch carries `require_absent(mpuctl)`. After the re-drive creates the
  ledger, the held batch loses on that ledger check whatever else it requires. I deleted
  `require_absent(mpu:<id>)` (`crates/core/src/multipart/create.rs:556`) and F3 still
  passes. Only the plant-collision leg catches that break. So the brief's claim that
  "`require_absent(mpu:<id>)` alone gives the mutual exclusion" (Design, unknown outcome) is
  never shown. Today the `mpuctl` compare-and-set happens to give the same exclusion, because
  every ledger change is +1. Once #659's decrement can move the ledger back to the held
  batch's bytes (an A-B-A sequence), `require_absent(mpu:<id>)` is the only guard. Fix: seed
  the ledger at count c so the held batch requires `mpuctl == c`. Before releasing it, write
  the c bytes back (as a terminal delete would), then assert `Conflict` and exactly one
  session counted once.

- NEEDS-HUMAN [impl] — **The shared error classifier never labels an in-doubt create as
  "indeterminate".** `impl Error for CreateError` (`crates/core/src/multipart/create.rs:174-186`)
  returns the stop *cause* as `source()`. `wyrd_traits::classify`
  (`crates/traits/src/lib.rs:786`) walks that chain, so the in-doubt error gets the cause's
  class. I checked with a scratch test: for
  `InDoubt { may_still_commit: true, cause: Store(TransientFault) }`, `classify` returns
  `Transient`, and for `cause: NonceTaken` it returns `Terminal`. The seam's rule is that the
  unknown-outcome class (`Indeterminate`, `:699`) must not be retried, and `is_transient()`
  (`:733`) is the one signal a retry policy may act on. The gateway already labels its error
  counter by `classify` (`crates/gateway-s3/src/lib.rs:3504`). A generic retry would re-run
  the create under a fresh identity while the first batch may still land, and the identity
  the error named would be dropped. The C5 surviving mutant at `create.rs:176`
  (`source → None`) shows no test pins this chain. Fix: make the `InDoubt` source chain carry
  the `CommitUnknownResult` so `classify` returns `Indeterminate`, and add a test asserting it.

- NEEDS-HUMAN [impl] — **The DST mixed leg's witness passes even when the property is
  broken.** `crates/dst/tests/multipart_admission.rs:450-461` presents its witness count as
  "the kept doubt meeting an out-of-flight outcome". I changed `|=` to `=` at
  `crates/core/src/multipart/create.rs:466`, so the latest outcome replaces the doubt. All six
  DST tests still pass (core F6 does catch it). I also counted how DST creates end over the
  same 64 seeds per leg. There were 99 `InDoubt` endings across 192 seed runs, and in **none**
  did the doubtful batch land at `quiesce`. Other creators always move `mpuctl`, so the held
  batch loses its stale compare-and-set. Invariant (3)'s in-doubt branch is therefore never
  tested against a real late landing. That is the rubric's "count-based assertion that can
  pass while the property fails". Fix: add a guard that requires at least one `InDoubt`
  whose batch landed at `quiesce`, for example a seed set where the in-doubt creator is
  alone after its 1031.

- NEEDS-HUMAN [human] — **The contention budget now has two definitions.** `create_session`
  reads `Budget::admission_cas_attempts()` (`crates/core/src/multipart/create.rs:288`) and
  the constant `MAX_UPLOAD_ID_ATTEMPTS`. It never reads `KnobSet::max_admission_cas_attempts`
  or `max_upload_id_attempts` (`crates/core/src/multipart.rs:5272-5274`), but
  `knob_clamps_hold` still checks them (`:5430`). The section's own rule
  (`crates/core/src/multipart.rs:4832-4835`: consumers "re-derive none of them: two
  derivations of one budget are how a session gets admitted against a number…") is bent.
  Concrete case: a `KnobSet` with `max_admission_cas_attempts = 200` and `max_sessions = 46`
  passes the clamp, yet a create still refuses after 46 losses. Someone needs to decide
  whether to retire those knobs or have Create take a `KnobSet`, before #508 wires
  configuration.

- NEEDS-HUMAN [human] — **Unknown outcomes spend the contention budget, and that budget
  scales with the profile.** The budget is `max_sessions`, which can be as large as
  `SCAN_CAP/2` = 524,288. Each unit can wait up to 50 ms, plus up to eight reads.
  Concrete case: a profile with `max_sessions = 100_000`, on a store that answers
  `CommitUnknownResult { may_still_commit: false }` without applying the batch (a partition).
  The create then makes about 100,000 commits and waits up to about 83 minutes before it
  answers `AdmissionContention`. At the shipped 46 it is 2.3 s, and the brief's F6 implies
  unknown outcomes are meant to spend this budget. So this is a design call, not a bug.
  Decide before #668 whether unknown outcomes get their own small cap.

## Attempted and could not refute

- **The required reds** (I broke the code in each named way): contention budget capped at 2
  → (A′) and 4 more tests fail. Re-mint on an unknown outcome → F1–F7 fail. Adopting a
  byte-identical found record → leg C fails. Replaying the original batch → F7 fails. Doubt
  replaced by the latest outcome → F6 fails. Settling read skipped → F5 fails. `Created`
  without a settling read → the DST reverse assertion fails on three legs and in the seed
  sweep.
- **The other boundaries also hold:** removing the bucket precondition or the
  marker-changed check fails (B). `>` changed to `>=` in the size check fails the headroom
  leg. Dropping `Completing` from the widest-state measurement fails it too. Re-minting on
  the "nonce taken" path fails three DST tests.
- **The widest-state values match the decoder's real bounds:** the segment cursor at
  `crates/core/src/multipart.rs:2609`, the ETag part count at `:4298`, and the publish target
  pinned to object, parent and epoch at `:2614-2627`. The size test checks production's
  reported byte count against its own spelling at the +1 boundary, so the check is not
  circular.
- **Contention arithmetic:** among creates, L losses mean at least L admissions, and the
  `SessionCap` check runs before the budget check (`create.rs:346-379`). So the budget runs
  out only when the ledger has filled or unknown outcomes and teardowns are involved, as the
  docs say.
- **Unknown-outcome detection:** the top-level `downcast_ref::<CommitUnknownResult>()`
  follows the trait's "one downcast on every backend" rule
  (`crates/traits/src/lib.rs:1306-1312`) and matches `gc.rs:1023`.
- **Note for sign-off:** the brief expected C4-ci to stop at the cargo-deny step. It didn't.
  The gate log shows deny, statics, conformance and the madsim DST step all ran and passed
  (`gate-logs/C4-ci.log:3537-3548`, `:3792`, `:4093-4103`).

<!-- pdca:leaf-complete -->
