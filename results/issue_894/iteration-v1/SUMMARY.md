# Result — issue 894 / multipart-admission-create

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Nothing on `main` creates a multipart session. The module is deliberately pure
  (`crates/core/src/multipart.rs:5-7`): no `MetadataStore` call and no `WriteBatch`. This slice
  adds `CreateMultipartUpload`'s admission through the serialized fleet ledger `mpuctl`, plus the
  session's creation, as a `wyrd-core` function over `MetadataStore`. It answers in the
  vocabulary already on `main`: `CreateOutcome` (`:4380`), `Refusal` (`:4302`) and
  `Backpressure` (`:4277`). Abort is the sibling slice (636.3b) and is not built here.
- Success criterion: A NEW test file `crates/core/tests/multipart_create.rs`, over
  `RedbMetadataStore::in_memory()` on a multi-thread tokio runtime, shows all of the following.
  "Writing nothing" below means the store is byte-identical before and after: no `mpu:`, no
  `seggrp:`, and no `mpuctl` write.
  **(A) Admission under free-running contention.** On a fresh empty store, **8, 10 and 16**
  concurrent creates all answer `CreateOutcome::Created`. Each level runs on its own fresh store,
  and all three levels are asserted. Their upload ids are pairwise distinct. Exactly N `mpu:`
  records exist (scan `MPU_PREFIX`), each decoding through `decode_session_record` as an `Open`
  session for the requested bucket and object. Exactly N `seggrp:` markers exist, one per
  session's nonce. `mpuctl` decodes through `decode_admission_record` with `count() == N`. These
  are the levels #508's rejected attempts falsely refused (8 → 2 refused, 10 → 3, 16 → 3;
  aws-cli's default `max_concurrent_requests` is 10). They are not the red, because redb's `get`
  and `commit` never await inside (`crates/metadata-redb/src/lib.rs:113-117`, `:208-234`), so a
  valid schedule can serialize them. (A′) is the red.
  **(A′) Admission under a forced schedule.** N = 8 creates go through a test-local
  `MetadataStore` wrapper over `RedbMetadataStore` that holds every commit until every live
  creator has read `mpuctl` and built its batch. So each round has one winner, and every other
  creator loses the `mpuctl` CAS. The wrapper counts each creator's CAS losses (it can tell
  creators apart by the `mpu:<id>` key in each batch). Assert that all 8 answer `Created`, that
  `mpuctl.count() == 8`, and that at least one creator lost the CAS **more than
  `MAX_UPLOAD_ID_ATTEMPTS` (= 2, `crates/core/src/multipart.rs:4983`) times** and still got its
  session. A second run uses a profile whose `max_sessions` is **above** the shipped `MAX_SESSIONS`
  (`:4943`) and drives one more creator than the shipped limit through the same barrier. All are
  admitted, which shows the contention budget follows the profile in effect, not the fixed
  `MAX_ADMISSION_CAS_ATTEMPTS` (`:4997`). The wrapper also records the waits the create asks its
  caller for: after its k-th lost CAS (k from 0) it asks for `admission_backoff_millis(k, jitter)`
  (`:5023`), so the window grows.
  **(B) The real bounds, each writing nothing.**
  - With the ledger at its stored `max_sessions`, a create answers
    `Refused(SlowDown { pressure: SessionCap { .. } })`.
  - A create whose local `Budget` differs from the profile stored in `mpuctl` answers
    `Refused(ProfileSkew { .. })`.
  - A create for a bucket with no `bucket:` record answers `Refused(NoSuchBucket)`.
  - A create whose bucket marker is **deleted or rewritten between its read and its commit** (the
    wrapper acts at commit) answers `Refused(NoSuchBucket)`.
  - **Size headroom:** a create whose session record, in the widest encoding of any state it can
    later reach (`Aborting`, `Completing`, `Completed`, with every number at the widest the
    decoder allows), would exceed `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`) is
    refused before any read or commit, with a typed answer naming the size and the ceiling. It
    writes nothing. The boundary is found on **encoded bytes**, not character length, through a
    store wrapper that enforces the ceiling: one byte under is admitted and can be torn down,
    and one byte over is refused.
  **(C) Two separate retry budgets, and no adopted session.**
  - When the first minted upload id collides with an existing `mpu:` record, the create re-mints
    and succeeds. When every one of `MAX_UPLOAD_ID_ATTEMPTS` mints collides, it ends with a typed
    error and writes nothing.
  - Losing the `mpuctl` CAS never spends the collision budget, and a collision never spends the
    contention budget.
  - An identity source that repeats an **existing** session's identity gets a re-mint, never
    that session as `Created`. The binding case is **byte-identical**: the planted record is
    exactly what this create would write (same upload id, nonce, bucket, object, content type,
    `created_at_millis` and `clock_source`), so no field comparison can tell them apart. The
    wrapper makes any commit for the repeated identity return
    `CommitUnknownResult { may_still_commit: false }` **unapplied**. Assert: `Created` under a
    **different** upload id, the planted record byte-identical afterwards, and `mpuctl.count()`
    up by exactly one. A create that decides ownership by comparing the record it finds (even
    every field) commits, meets the unknown outcome, re-reads, and adopts the planted session:
    it fails this leg.
  - **Stated bound, not tested as a guarantee:** two invocations handed the **same** upload id
    and nonce while both are live cannot be told apart by anything in the store. Both read
    absent, one batch lands, and the other's unknown-outcome re-read finds bytes it could have
    written. The guarantee assumes the identity source never does this. Upload ids and nonces
    are 128-bit random tokens (`crates/core/src/multipart.rs:814-848`) minted from the caller's
    randomness, so in production the chance is negligible. The function's rustdoc states the
    assumption.
  **(F) An unknown Create outcome is recovered under the original upload id
  (`0016:2009-2027`).** Through the wrapper, a Create commit returns
  `CommitUnknownResult { may_still_commit: true }` (`crates/traits/src/lib.rs:230-248`):
  - **F1:** the batch was applied before the error.
  - **F2:** it was not applied.
  - **F3:** it is held and applied **after** the recovery's own commit has landed, so the late
    batch loses its `require_absent(mpu:<id>)`.
  In each, the create answers `Created` with the **same** upload id and nonce, and the recovery
  rebuilds the `mpuctl` precondition from a fresh read. Afterwards exactly one `mpu:` record (for
  that id), one `seggrp:` marker and one increment of `mpuctl.count()` exist, and no second
  upload id was minted. Three more legs:
  - **F4:** the unknown outcome lands on the **last** attempt the contention budget allows. The
    create still settles it by reading before it returns, and answers `Created` when the batch
    landed.
  - **F5:** another create fills the ledger between the recovery's reads. The create re-checks
    its own `mpu:<id>` before giving up on a capacity refusal, and answers `Created` when its
    earlier batch is there.
  - **F6, the doubt is kept:** on a profile small enough that the contention budget is spent,
    commit 1 is held (`may_still_commit: true`) and commit 2 returns
    `CommitUnknownResult { may_still_commit: false }`. The create must end with an error that
    still says `may_still_commit: true` and names the upload id and nonce. It must not end with
    a contention refusal. Released **after** the create has returned, the held batch lands, and
    the store then holds that one session, counted.
  - **F7, the `mpuctl` precondition is rebuilt, not replayed (`0016:2018-2023`):** commit 1
    returns `CommitUnknownResult { may_still_commit: false }` and is **not** applied. Before the
    recovery's commit, the wrapper admits a second create, which moves `mpuctl` from count c to
    c + 1 and still leaves room. The first create must answer `Created` under its **original**
    upload id and nonce. Afterwards both sessions exist, each with its `seggrp:` marker, and
    `mpuctl.count() == c + 2`. A recovery that replays the original batch carries the stale
    `require(mpuctl == c)`, which can never commit however often it is driven, even with room
    and the id still free; it fails this leg. (F2 and F3 cannot catch that, because nothing
    moves the ledger in them; F5 cannot either, because its earlier batch is already there.)
  **(G) Seeded coverage** (`AGENTS.md:188-190`: a new concurrent path lands with seeded Tier-0
  DST coverage). A NEW madsim scenario `crates/dst/tests/multipart_admission.rs` runs concurrent
  Creates on `SimFdbMetadataStore` with the commit-ambiguity nemesis armed
  (`crates/dst/tests/support/mod.rs:696`, `:727` `quiesce`). It has three legs: 1021 alone, 1031
  alone, and a mixed leg where each commit's code is a seed-derived coin, so one writer can meet a
  1031 and then a 1021. After every seed, on the store after `quiesce`:
  - the number of `mpu:` records equals `mpuctl.count()`;
  - every `mpu:` record has its `seggrp:` marker, and no marker is without its session;
  - every `mpu:` record belongs to a create that answered `Created` for it, or to one whose
    in-doubt error named that upload id. None is stranded;
  - **the reverse: every `Created` is real.** For each create that answered `Created`, its
    `mpu:<id>` exists and decodes as an `Open` session for the requested bucket and object, its
    `seggrp:<nonce>` marker exists, and it is one of the records `mpuctl.count()` counts. A
    create that answered `Created` for a 1021 whose batch never applied fails here. Without
    this direction a missing session is never looked at.
  The mixed leg must observe at least one writer meeting 1031 and then 1021 across the sweep. The
  1021 leg must observe at least one 1021 that did **not** apply, read from the sim's own counters
  (`ambiguous_commits > ambiguous_commits_that_landed`, `FdbObservations`,
  `crates/dst/tests/support/mod.rs:527-544`; a 1021 applies on a coin, `:803-806`); a sweep
  where every 1021 landed never tests the reverse assertion and is reported as vacuous.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `CreateMultipartUpload`'s admission and session creation as a store-I/O function in
  `wyrd-core` answering in the landed vocabulary. That includes recovering an unknown commit
  outcome under the original upload id, the encoded-size headroom check, and the one creation
  route each for a session record and the ledger record. Also in scope: the seeded DST scenario
  `crates/dst/tests/multipart_admission.rs`; updating the multipart module header and the
  living-architecture paragraph to say Create now writes; the doc comments on
  `MAX_ADMISSION_CAS_ATTEMPTS` and `ADMISSION_BACKOFF_CAP_MILLIS` (`:4985-4997`, `:5002-5004`),
  which must stop saying the budget is fixed at `MAX_SESSIONS` and that a contention refusal is
  always a full ledger.
  / out of scope:
  - `AbortMultipartUpload`, the shared fence batch builder and any change to
    `crates/custodian/src/restore.rs` (the sibling Abort slice, 636.3b);
  - UploadPart and slot reservation (#657); Complete (#658);
  - the retirement drain and the terminal delete that decrements `mpuctl` (#659);
  - the reaper and operator abort (#625, #633);
  - every gateway, S3, CLI or custodian **caller** of Create (#666–#670; the S3 verb is #668);
  - the bucket deletion-fence key and its `require_absent` (#511, see Design);
  - any edit to proposal 0016 or to any ADR;
  - `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 88.0% — 265 of 301 instrumentable changed lines executed (floor 80%); 301 of 788 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 54 mutants tested in 2m: 3 missed, 39 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_894/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.37s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #894’s multipart admission and Create implementation: one reproduced ownership defect warrants a builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scope and success conditions distinguish admission contention, identity collisions, and ambiguous commits; the excluded case requires both identical upload ID and nonce (`brief.md:61`; `crates/core/src/multipart/create.rs:256`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared new-API verification exception — the independent stash run fails at the missing import before any assertion, so it establishes no behavioral pre-fix red (`brief.md:196`; `review-red-green.log:3`). |
| C3 Change | FAIL | An overlapping creator can win the upload ID with a different nonce and object, yet recovery reports that upload as this caller’s successful Create (`crates/core/src/multipart/create.rs:305`; `review-collision.log:53`). |
| C4 Verification (red→green) | FAIL | The reviewer’s compiled ownership counterexample fails on the patched target, despite independent greens for all 19 supplied core tests and six DST tests (`review-collision.log:56`; `review-red-green.log:53`; `review-dst-test.log:248`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Distinguish another creator’s session before adopting it and add the overlapping-collision regression — absence plus an unknown outcome does not establish ownership when the stored nonce differs (`crates/core/src/multipart/create.rs:305`; `crates/core/src/multipart/create.rs:498`). |
| T1 Structure | PASS | Core retains the narrow MetadataStore seam and caller-supplied runtime; no concrete production backend or runtime dependency is introduced (`crates/core/src/multipart/create.rs:72`; `crates/core/src/multipart/create.rs:272`). |
| T2 Shape | PASS | Canonical creation routes preserve decoder boundaries, and the living architecture documents the new writer and its production reach (`crates/core/src/multipart.rs:1895`; `crates/core/src/multipart.rs:2349`; `docs/design/architecture/05-building-block-view.md:206`). |
| T3 Runtime | PASS | Retry and wait bounds follow the active profile, store awaits inherit the terminating-operation contract, and the caller supplies the lifecycle clock (`crates/core/src/multipart/create.rs:263`; `crates/core/src/multipart/create.rs:371`; `crates/traits/src/lib.rs:1326`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive audit must re-run at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The change stays within the core-only slice, preserves the tracked bucket-fence deferral, and affected-path prior-art queries found no competing rejected PR implementation (`crates/core/src/multipart/create.rs:569`; `review-prior-art-paths.json:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this core-only slice meets #894 after the ownership defect is resolved — end-user multipart behavior is intentionally deferred to #668 and is not established here (`brief.md:221`; `crates/core/src/multipart/create.rs:21`). |

Source citations are relative to `$PDCA_TARGET`; evidence files are relative to this review sandbox. The target matches the supplied patch: reverse-apply checking succeeds, and the stashed changes were restored without modifying the reviewed implementation.

**The ownership defect is reproducible and distinguishable from the documented exception.** The first Create reads upload ID X and nonce A absent. Before its unapplied commit returns `CommitUnknownResult { may_still_commit: false }`, an overlapping real Create installs X with nonce B for another object. Recovery tests only whether X exists and returns `Created(X)`. The caller requested `photos/requested-object` with nonce 100 but receives the ID for `photos/other-object` with nonce 200; only one identity was minted (`review-collision.log:53`). The same unchecked adoption exists in `finish` (`crates/core/src/multipart/create.rs:498`).

The rustdoc excludes concurrent reuse of **both** ID and nonce (`crates/core/src/multipart/create.rs:256`); this counterexample reuses only the ID. Retain the invocation-history requirement and additionally check that the found session belongs to the intended nonce/request. A mismatching record must not become `Created`; collision handling must also retain any unresolved late-commit doubt. The existing repeated-identity test plants its record before the initial read, so it does not cover this interleaving (`crates/core/tests/multipart_create.rs:1242`).

The reviewer-only reproducer uses the unchanged target crates and a real in-memory redb store. Run from this sandbox:

```sh
CARGO_TARGET_DIR="$PWD/review-build-collision" TMPDIR="$PWD/pdca-reviewer-894-tmp" cargo test --offline --manifest-path pdca-reviewer-894-collision/Cargo.toml -- --nocapture
```

**The supplied success paths and repository checks have substantial passing evidence, but do not cover that collision.**

- Independent execution: 19 core tests pass after stash restoration; all six DST tests pass with `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50`, including the mixed-outcome witnesses and two expected-panic negative controls. Conformance, statics, formatting, typos, documentation lint and the 99-page render/link audit all pass (`review-red-green.log:53`; `review-dst-test.log:240`; `review-scanners.log:1`). Both external dependencies named in the brief were exercised.
- Frozen gate adjudication: full CI actually reaches deny, conformance, statics, deployment guard, and DST, ending successfully; the brief’s anticipated deny failure is not present in this round (`gate-logs/C4-ci.log:3537`; `gate-logs/C4-ci.log:4149`). Diff coverage is 265/301 instrumentable changed lines, 88%; TiKV and server feature compilation succeed (`gate-logs/C4-diff-cov.log:138`; `gate-logs/host-tikv.log:211`). The batched-review log reports zero findings but contains no individual review reasoning (`gate-logs/T4-batch-review.log:10`); it does not refute the counterexample.
- Mutation results remain advisory: the frozen run reports 39 caught, 12 unviable and three surviving diagnostic mutations (two Display implementations and Error::source; `gate-logs/C5-mutants.log:13`). My independent 54-mutant run reports 33 caught, 20 unviable and one survivor (`review-mutants.log:5`); for example, its Display mutations are unviable because unused formatter parameters trigger denied warnings, not because tests reject them. These results do not establish ownership under overlapping ID collisions.

**Prior art and scope were checked by affected file path.** GitHub commit queries covered all seven changed paths and confirmed the merged nonce, capacity-knob and answer-table prerequisites (`2f41e73531b3`, `4533549cb131`, `14911ec56693`). File lists for all 19 returned closed, unmerged PRs had no overlap with those paths; the rejected-attempt history in #508, #636 and #656 was also read (`review-prior-art-paths.json:1`; `review-issue-508.json:1`; `review-issue-636.json:1`; `review-issue-656.json:1`). Production-callsite searching found no caller outside the new tests. The #511 deferral is settled, and the diff introduces no capability probe that triggers the symptom-guard rule.

<!-- pdca:leaf-complete -->

### Advisory — adversary

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

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/create.rs:305` (also `crates/core/src/multipart/create.rs:498`): Recovery treats any existing `mpu:<id>` as this invocation's session. A concrete schedule is: A reads upload ID U and nonce NA absent, submits a batch that is held with `CommitUnknownResult { may_still_commit: true }`, then B commits U with a different nonce NB and object. A's recovery returns `Created(U)` for B's session without checking its contents; A's held batch subsequently conflicts. This does not violate the documented exclusion at `crates/core/src/multipart/create.rs:256`, which excludes concurrent reuse of both the upload ID and nonce. Keep the absence-read/unknown-commit history requirement, but also decode and check the recovered record's immutable identity and request fields before returning `Created`, sharing that check between both recovery paths. Add a forced-schedule regression with the same upload ID and distinct nonces/objects.

No additional reuse, simplification, or efficiency findings. Validation used target-source inspection and the frozen gate evidence; no gates were re-run.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared new-API verification exception — the independent stash run fails at the missing import before any assertion, so it establishes no behavioral pre-fix red (`brief.md:196`; `review-red-green.log:3`).
- [ ] C5 Causal adequacy — Distinguish another creator’s session before adopting it and add the overlapping-collision regression — absence plus an unknown outcome does not establish ownership when the stored nonce differs (`crates/core/src/multipart/create.rs:305`; `crates/core/src/multipart/create.rs:498`).
- [ ] Validation — fitness-to-purpose — Decide whether this core-only slice meets #894 after the ownership defect is resolved — end-user multipart behavior is intentionally deferred to #668 and is not established here (`brief.md:221`; `crates/core/src/multipart/create.rs:21`).
- [ ] **Only part of the "doubt is kept" contract is tested.** The brief (Design, "the doubt is kept") says that once a batch may still land, three things must not clear the doubt: a later out-of-flight outcome, a failed settling read, or a plain store fault. Only the first is tested (F6). The other two live at `crates/core/src/multipart/create.rs:300` (a read fault after an unknown outcome), `:393` (a definite commit fault after an earlier unknown outcome) and `:500` (the settling read in `finish` fails). The C4-diff-cov gate already lists all three lines as never executed. I replaced `doubt` with `Doubt::default()` at `:300` and at `:393`, and made `:500` reset the doubt. All three breaks pass all 19 core tests and all 6 DST tests. The simulated store never returns a plain read or commit fault, so the DST cannot reach these paths. Concrete failing case for the `:393` break: commit 1 returns `CommitUnknownResult { may_still_commit: true }` and stays in flight. Commit 2 fails with a plain store error. The create answers `CreateError::Store` ("No batch of this create is in doubt", `:101`), and then the held batch lands. That leaves a counted session no caller was told about, which breaks C-1. Fix: give `Scripted` a plain-`Err` commit action and a failing-`get` hook. Then add a test for each path that asserts `InDoubt { may_still_commit: true }` with the original identity.
- [ ] **Nothing tests `require_absent(seggrp:<nonce>)` in the Create batch** (`crates/core/src/multipart/create.rs:557`). Deleting that line passes all 25 tests. The nonce-collision test (`c_a_colliding_identity_is_re_minted`) only catches a nonce already present at read time. The "collision arriving as a lost commit" leg (`crates/core/tests/multipart_create.rs:1212`, `PlantSessionFirst` at `:763`) plants only `mpu:<id>`. Concrete failing case: another session takes this nonce between the create's read and its commit. Without the precondition, both batches land, and the second `put` of `{}` silently takes over the first session's `seggrp:` marker. Whichever terminal delete runs first (#659) then removes the other session's reservation. Fix: add a `PlantGroupFirst` leg beside `PlantSessionFirst`.
- [ ] **F3 doesn't test what its assertion says.** `crates/core/tests/multipart_create.rs:1362-1371` asserts `late == Conflict` with the message "the late batch lost its require_absent(mpu:<id>)". But the store starts with no ledger, so the held batch carries `require_absent(mpuctl)`. After the re-drive creates the ledger, the held batch loses on that ledger check whatever else it requires. I deleted `require_absent(mpu:<id>)` (`crates/core/src/multipart/create.rs:556`) and F3 still passes. Only the plant-collision leg catches that break. So the brief's claim that "`require_absent(mpu:<id>)` alone gives the mutual exclusion" (Design, unknown outcome) is never shown. Today the `mpuctl` compare-and-set happens to give the same exclusion, because every ledger change is +1. Once #659's decrement can move the ledger back to the held batch's bytes (an A-B-A sequence), `require_absent(mpu:<id>)` is the only guard. Fix: seed the ledger at count c so the held batch requires `mpuctl == c`. Before releasing it, write the c bytes back (as a terminal delete would), then assert `Conflict` and exactly one session counted once.
- [ ] **The shared error classifier never labels an in-doubt create as "indeterminate".** `impl Error for CreateError` (`crates/core/src/multipart/create.rs:174-186`) returns the stop *cause* as `source()`. `wyrd_traits::classify` (`crates/traits/src/lib.rs:786`) walks that chain, so the in-doubt error gets the cause's class. I checked with a scratch test: for `InDoubt { may_still_commit: true, cause: Store(TransientFault) }`, `classify` returns `Transient`, and for `cause: NonceTaken` it returns `Terminal`. The seam's rule is that the unknown-outcome class (`Indeterminate`, `:699`) must not be retried, and `is_transient()` (`:733`) is the one signal a retry policy may act on. The gateway already labels its error counter by `classify` (`crates/gateway-s3/src/lib.rs:3504`). A generic retry would re-run the create under a fresh identity while the first batch may still land, and the identity the error named would be dropped. The C5 surviving mutant at `create.rs:176` (`source → None`) shows no test pins this chain. Fix: make the `InDoubt` source chain carry the `CommitUnknownResult` so `classify` returns `Indeterminate`, and add a test asserting it.
- [ ] **The DST mixed leg's witness passes even when the property is broken.** `crates/dst/tests/multipart_admission.rs:450-461` presents its witness count as "the kept doubt meeting an out-of-flight outcome". I changed `|=` to `=` at `crates/core/src/multipart/create.rs:466`, so the latest outcome replaces the doubt. All six DST tests still pass (core F6 does catch it). I also counted how DST creates end over the same 64 seeds per leg. There were 99 `InDoubt` endings across 192 seed runs, and in **none** did the doubtful batch land at `quiesce`. Other creators always move `mpuctl`, so the held batch loses its stale compare-and-set. Invariant (3)'s in-doubt branch is therefore never tested against a real late landing. That is the rubric's "count-based assertion that can pass while the property fails". Fix: add a guard that requires at least one `InDoubt` whose batch landed at `quiesce`, for example a seed set where the in-doubt creator is alone after its 1031.
- [ ] **The contention budget now has two definitions.** `create_session` reads `Budget::admission_cas_attempts()` (`crates/core/src/multipart/create.rs:288`) and the constant `MAX_UPLOAD_ID_ATTEMPTS`. It never reads `KnobSet::max_admission_cas_attempts` or `max_upload_id_attempts` (`crates/core/src/multipart.rs:5272-5274`), but `knob_clamps_hold` still checks them (`:5430`). The section's own rule (`crates/core/src/multipart.rs:4832-4835`: consumers "re-derive none of them: two derivations of one budget are how a session gets admitted against a number…") is bent. Concrete case: a `KnobSet` with `max_admission_cas_attempts = 200` and `max_sessions = 46` passes the clamp, yet a create still refuses after 46 losses. Someone needs to decide whether to retire those knobs or have Create take a `KnobSet`, before #508 wires configuration.
- [ ] **Unknown outcomes spend the contention budget, and that budget scales with the profile.** The budget is `max_sessions`, which can be as large as `SCAN_CAP/2` = 524,288. Each unit can wait up to 50 ms, plus up to eight reads. Concrete case: a profile with `max_sessions = 100_000`, on a store that answers `CommitUnknownResult { may_still_commit: false }` without applying the batch (a partition). The create then makes about 100,000 commits and waits up to about 83 minutes before it answers `AdmissionContention`. At the shipped 46 it is 2.3 s, and the brief's F6 implies unknown outcomes are meant to spend this budget. So this is a design call, not a bug. Decide before #668 whether unknown outcomes get their own small cap.
- [ ] `crates/core/src/multipart/create.rs:305` (also `crates/core/src/multipart/create.rs:498`): Recovery treats any existing `mpu:<id>` as this invocation's session. A concrete schedule is: A reads upload ID U and nonce NA absent, submits a batch that is held with `CommitUnknownResult { may_still_commit: true }`, then B commits U with a different nonce NB and object. A's recovery returns `Created(U)` for B's session without checking its contents; A's held batch subsequently conflicts. This does not violate the documented exclusion at `crates/core/src/multipart/create.rs:256`, which excludes concurrent reuse of both the upload ID and nonce. Keep the absence-read/unknown-commit history requirement, but also decode and check the recovered record's immutable identity and request fields before returning `Created`, sharing that check between both recovery paths. Add a forced-schedule regression with the same upload ID and distinct nonces/objects.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The tracker basis and claimed exceptions cannot be verified from the supplied inputs.** This sandbox contains `brief.md` but no `notes.json`, `sources/`, or `dependency-state.json`. Consequently the #508 measurements (`brief.md:29`), prior-attempt lessons (`brief.md:263`, `brief.md:295`), intake-cap override (`brief.md:134`) and inherited-deny ruling (`brief.md:178`) are assertions without the underlying thread. The target does resolve to `getwyrd/wyrd` `origin/main` at `7bf7eb906c634d57d28f2ae26fd0b412732948d5`, and the cited #654/#655 commits are ancestors; that does not establish the tracker state of #768/#895 or those human rulings. Include the load-bearing thread excerpts and source references in the brief, or explicitly identify these claims as unverified at sign-off.
- [ ] **The offered ownership check does not establish “this invocation's own” session.** `brief.md:259-265` requires ownership but offers “a whole-record comparison” as an alternative mechanism. At the target, `crates/core/src/multipart.rs:2265-2279` contains no separate invocation identifier: two calls supplied the same identity, timestamp and request fields can produce identical records. If an identical record already exists and this invocation's commit returns an unknown, unapplied outcome, matching every field still adopts the other session, violating criterion C (`brief.md:67-70`). Even a preflight absence read cannot distinguish two identical invocations that race after both read absent. Specify the ownership evidence and any uniqueness assumption, remove whole-record equality as a sufficient alternative, and require a byte-identical collision test; either cover the concurrent case or explicitly bound the guarantee.
- [ ] **The deterministic recovery schedules do not distinguish a rebuilt ledger precondition from replaying the stale batch.** In F2/F3 (`brief.md:75-80`), no competing create is required to advance `mpuctl` before recovery commits. An identical replay can therefore succeed. F5 changes the ledger, but expects the original session already to exist (`brief.md:85-87`), so it can finish by reading without successfully rebuilding any batch. The exact missed failure is documented at target `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2018-2023`: a stale counter precondition cannot commit even with capacity available. Add a forced leg where the original batch remains unapplied, another creator advances the ledger while leaving room, and recovery must create the original identity with a fresh counter precondition. Require a stale-batch-replay mutation to fail that leg.
- [ ] **G's oracle allows a false `Created` response.** `brief.md:100-103` checks that every stored session has a reported owner, but never checks that every reported success has a stored session. Target `crates/dst/tests/support/mod.rs:803-806` deliberately permits a 1021 with no applied batch, and `:824` returns the unknown-result error either way. Returning `Created` for that unapplied attempt, when there was no earlier in-flight commit, can satisfy all three stated G invariants: the missing session is never examined. This contradicts the durability promise of `CreateOutcome::Created` at `crates/core/src/multipart.rs:4381-4385`. Add the reverse assertion after `quiesce`: every `Created` result must resolve to its expected durable session and reservation, with the ledger counting it; ensure the 1021 sweep actually exercises an unapplied outcome.
- [ ] size backstop — this slice is behaving oversized: patch is 145 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice is oversized (145 KB patch vs 100 KB threshold; ~1,900 test lines) and every required fix adds more tests. Split in the re-plan: (a) core create + unit/forced-schedule tests (A, A', B, C, F); (b) seeded DST scenario (G) as its own slice on top. Carry forward into the new briefs: - Real defect: recovery adopts any existing mpu:<id> as its own (create.rs:305, :498). A different creator with the same upload id but a different nonce/object gets returned as Created. Decode and check the found record's nonce/request fields in both recovery paths; add a forced-schedule regression (same id, distinct nonce/object). - Test gaps to pin: kept doubt after a plain read/commit fault (:300, :393, :500); require_absent(seggrp:<nonce>) (add a PlantGroupFirst leg); F3 must seed the ledger and replay the c bytes (A-B-A) so only require_absent(mpu:<id>) can stop the late batch; InDoubt must classify as Indeterminate through the source chain; DST must witness at least one InDoubt batch that actually lands at quiesce. - Open design calls, not yet decided by the human: (1) KnobSet::max_admission_cas_attempts / max_upload_id_attempts are clamped but never read by Create: retire them or have Create take a KnobSet; (2) should unknown outcomes get their own small cap instead of spending the max_sessions-sized contention budget; (3) the brief's tracker claims/rulings (#508 numbers, deny ruling) were unverifiable by the reviewer: include source excerpts or mark them unverified.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
