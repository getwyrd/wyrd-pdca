# Result — issue 894 / multipart-admission-create

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Nothing on `main` creates a multipart session. The module is deliberately pure
  (`crates/core/src/multipart.rs:5-7`): no `MetadataStore` call and no `WriteBatch`. This slice
  adds `CreateMultipartUpload`'s admission through the serialized fleet ledger `mpuctl`, plus the
  session's creation, as a `wyrd-core` function over `MetadataStore`. It answers in the
  vocabulary already on `main`: `CreateOutcome` (`:4380`), `Refusal` (`:4302`) and
  `Backpressure` (`:4277`). Abort is the sibling slice #895 (636.3b) and is not built here.
- Success criterion: A NEW test file `crates/core/tests/multipart_create.rs`, over
  `RedbMetadataStore::in_memory()` on a multi-thread tokio runtime, passes every leg below, and a
  NEW madsim scenario `crates/dst/tests/multipart_admission.rs` passes (G). Terms: an
  **identity** is one minted (upload id, segment-group nonce) pair. **R** is the exact session
  record bytes every batch under that identity puts. **Writes nothing** means the store is
  byte-identical before and after: no `mpu:`, no `seggrp:` and no `mpuctl` write. Each leg names
  the Design table row it pins.
  **(A) Admission under free-running contention.** On a fresh empty store, **8, 10 and 16**
  concurrent creates all answer `CreateOutcome::Created`. Each level runs on its own fresh store,
  and all three levels are asserted. Their upload ids are pairwise distinct. Exactly N `mpu:`
  records exist (scan `MPU_PREFIX`), each decoding through `decode_session_record` as an `Open`
  session for the requested bucket and object. Exactly N `seggrp:` markers exist, one per
  session's nonce. `mpuctl` decodes through `decode_admission_record` with `count() == N`. These
  are the levels #508's rejected attempts falsely refused (8 → 2 refused, 10 → 3, 16 → 3). They
  are not the red, because redb's `get` and `commit` never await inside
  (`crates/metadata-redb/src/lib.rs:113-117`, `:208-234`), so a valid schedule can serialize
  them. (A′) is the red.
  **(A′) Admission under a forced schedule (row 7).** N = 8 creates go through a test-local
  `MetadataStore` wrapper over `RedbMetadataStore` that holds every commit until every live
  creator has read `mpuctl` and built its batch. So each round has one winner, and every other
  creator loses the `mpuctl` CAS. The wrapper tells creators apart by the `mpu:<id>` key in each
  batch and counts each one's CAS losses. Assert that all 8 answer `Created`, that
  `mpuctl.count() == 8`, and that at least one creator lost the CAS **more than
  `MAX_UPLOAD_ID_ATTEMPTS` (= 2, `crates/core/src/multipart.rs:5009`) times** and still got its
  session. A second run uses a profile whose `max_sessions` is **above** the shipped
  `MAX_SESSIONS` (`:4969`) and drives one more creator than the shipped limit through the same
  barrier. All are admitted, which shows the contention budget follows the profile in effect,
  not the fixed `MAX_ADMISSION_CAS_ATTEMPTS` (`:5023`). The wrapper also records the waits the
  create asks for: after its k-th lost CAS (k from 0) it asks for
  `admission_backoff_millis(k, jitter)` (`:5049`).
  **(B) The real bounds, each writing nothing (rows 5, 6, 7, 10, 12).**
  - With the ledger at its stored `max_sessions`, a create answers
    `Refused(SlowDown { pressure: SessionCap { .. } })`.
  - A create whose local `Budget` differs from the profile stored in `mpuctl` answers
    `Refused(ProfileSkew { .. })`.
  - A create for a bucket with no `bucket:` record answers `Refused(NoSuchBucket)`.
  - A create whose bucket marker is **deleted or rewritten between its read and its commit** (the
    wrapper acts at commit) answers `Refused(NoSuchBucket)`.
  - A stored `mpuctl` that does not decode as an admission record ends `CreateError::Ledger`
    (row 9).
  - **Contention spent with nothing moved (rows 7, 10).** On the shipped profile, a wrapper
    answers every commit of the create `Ok(CommitOutcome::Conflict)` without applying it and
    without changing any record, as TiKV does when a locking read meets another writer's lock
    (`crates/metadata-tikv/src/lib.rs:1386-1393`, `:928-939`). The create ends
    `Refused(SlowDown { pressure: AdmissionContention { .. } })`. Every one of its commits names
    the one identity it minted (no re-mint). The number of its commits equals the contention
    budget, the profile's `max_sessions`, and the refusal's `attempts` names that number. It
    writes nothing, and `mpuctl` is byte-identical afterwards. This is the case the narrowed
    guarantee in Invariant names: a contention refusal on a ledger with room.
  - **Size headroom:** a create whose session record, in the widest encoding of any state it can
    later reach (`Aborting`, `Completing`, `Completed`, with every number at the widest the
    decoder allows), would exceed `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`) is
    refused before any read or commit, with a typed answer naming the size and the ceiling. The
    boundary is found on **encoded bytes**, not character length, through a store wrapper that
    enforces the ceiling. Three points, as attempt 1's size test has them: one byte under and
    **exactly at** the ceiling are both admitted (the ceiling admits equality,
    `crates/core/src/metadata.rs:602-606`), and each one's `SessionRecord::open_teardown` record
    (`multipart.rs:2360`) and every widest reachable state also fit under the wrapper. One byte
    over is refused. The three points run on **two fixtures**, so that each payload-carrying
    state is the strictly widest in one of them:
    - **Completing widest:** a long object name made of characters JSON escapes six bytes wide
      (attempt 1's fixture). `Completing` carries the name twice, as `object` and as
      `publish_target.name` (`multipart.rs:2055-2062`).
    - **Completed widest:** a short object name (a few bytes), padded to the boundary with
      `content_type`. `Completed`'s fixed payload (`multipart.rs:2077-2091`: two digests, an
      ETag and three `u64`s) is then wider than `Completing`'s. By a rough count with parent
      inode 0, `Completed` stays wider until the encoded name passes about 150 bytes. Do
      measures the real crossover with the real encoder and records it in `build-notes.md`.
    Each fixture asserts which state is strictly widest, so the fixture cannot drift into the
    other regime without failing.
  **(C) Identities and the two budgets (rows 1, 3, 4, 7, 11).** Unless a bullet says otherwise,
  the interloper below is a test wrapper writing raw records at the create's commit, without
  moving `mpuctl`.
  - **C1.** When the first identity collides with an existing `mpu:` record, the create mints a
    new one and answers `Created`. When every one of `MAX_UPLOAD_ID_ATTEMPTS` identities
    collides, it ends with `CreateError::IdentityCollisions` and writes nothing.
  - **C2.** Losing the `mpuctl` CAS never spends the collision budget, and a collision never
    spends the contention budget.
  - **C3, the id taken under the commit.** An interloper writes `mpu:<id>` (another nonce,
    another object) between the create's read and its commit. The create answers `Created`
    under a **new** identity, and the interloper's record is byte-identical afterwards.
  - **C4, the nonce taken under the commit.** An interloper writes `seggrp:<nonce>` only,
    between the create's read and its commit. The create answers `Created` under a new identity:
    its session's nonce is not the interloper's. (The marker's value is a constant, so only the
    identity the create answers under can tell a re-mint from an overwrite.)
  - **C5, a byte-identical record is never adopted.** The planted record is exactly R, written
    **before** the create's first read (same upload id, nonce, bucket, object, content type,
    `created_at_millis` and `clock_source`). The wrapper makes any commit for that identity
    return `CommitUnknownResult { may_still_commit: false }` **unapplied**. Assert `Created`
    under a **different** upload id, the planted record byte-identical afterwards, and
    `mpuctl.count()` up by exactly one.
  - **C6, another session under this create's upload id is never adopted.** This is attempt 1's
    defect. The create reads its identity absent. Then, before its commit returns, a **real**
    second create (through the same function, minted with the **same upload id**, a different
    nonce and a different object) installs its session. (i) The first create's commit returns
    `CommitUnknownResult { may_still_commit: false }` **unapplied**: it answers `Created` under a
    **new** identity, never under the shared id. Both sessions exist, the second's record is
    byte-identical to what the second create wrote, and `mpuctl.count()` is up by two. (ii) The
    first create's commit returns `CommitUnknownResult { may_still_commit: true }` and is held:
    it ends with `CreateError::InDoubt { may_still_commit: true }` naming **its own** upload id
    and nonce. It never answers `Created` and never mints a second identity. Released afterwards,
    the held batch answers `Conflict`, and the store holds the second create's session only.
  **(F) Unknown outcomes, recovered under the original identity (`0016:2010-2028`; rows 2, 8-10).**
  Through the wrapper, a Create commit returns `CommitUnknownResult { may_still_commit: true }`
  (`crates/traits/src/lib.rs:222-248`):
  - **F1:** the batch was applied before the error.
  - **F2:** it was not applied.
  - **F3:** it is held while the re-drive lands. **The re-drive's exclusion is by the identity's
    keys, not by the ledger.** The store starts with `mpuctl` at count c, with room, so the held
    batch carries `require(mpuctl == <count-c bytes>)`. After the create has answered `Created`,
    the test writes the count-c bytes back into `mpuctl` (as #659's terminal delete of some other
    session would). Then it releases the held batch. The batch answers `Conflict`. Afterwards
    `mpu:<id>` is exactly R, one `seggrp:` marker exists, and `mpuctl` is still exactly the
    written-back bytes.
  In F1 to F3 the create answers `Created` with the **same** upload id and nonce. Afterwards
  exactly one `mpu:` record for that id, one `seggrp:` marker and one increment of
  `mpuctl.count()` exist (F3: before the write-back), and no second identity was minted.
  - **F4:** the unknown outcome lands on the **last** attempt the contention budget allows. The
    create still settles it by reading before it returns, and answers `Created` when the batch
    landed.
  - **F5:** another create fills the ledger between the recovery's reads. The create reads its
    own `mpu:<id>` before giving up on the capacity refusal, and answers `Created` when R is
    there.
  - **F6, the doubt is kept past a later 1021:** on a profile small enough that the contention
    budget is spent, commit 1 is held (`may_still_commit: true`) and commit 2 returns
    `CommitUnknownResult { may_still_commit: false }` unapplied. The create ends with
    `InDoubt { may_still_commit: true }` naming the original upload id and nonce, never
    `SlowDown { AdmissionContention }`. Released after the create has returned, the held batch
    lands, and the store then holds that one session, counted.
  - **F7, the ledger precondition is rebuilt, not replayed (`0016:2018-2025`, X101 at
    `:2640`):** commit 1 returns `CommitUnknownResult { may_still_commit: false }` and is **not**
    applied. **Inside commit 1, before it returns**, the wrapper admits a second create, which
    moves `mpuctl` from count c to c + 1 and still leaves room. So the move lands before any of
    the recovery's reads (attempt 1's `UnknownThenAdmit`). The first create answers `Created`
    under its **original** upload id and nonce. The wrapper records every batch the create
    commits, and the test asserts on those batches, not only on the end state:
    - the create commits **exactly two** batches;
    - batch 2, the first recovery batch, carries `require(mpuctl == <count c + 1 bytes>)`, the
      fresh read, and puts `mpuctl` at count c + 2.
    A recovery that replays batch 1 and then retries would commit three batches, and its
    batch 2 would name count c. Afterwards both sessions exist, each with its `seggrp:`
    marker, and `mpuctl.count() == c + 2`.
  - **F8, the doubt is kept past every fault.** Each case below ends with `CreateError::InDoubt`
    naming the original upload id and nonce. In (a) to (c), the held batch is released after the
    create returns, it lands, and the store then holds that one session, counted. The wrapper
    gains a plain (non-unknown) `Err` for `commit` and a failing `get`.
    - (a) Commit 1 is held (`true`), and the next `get` fails with a plain store error. The
      create ends `InDoubt { may_still_commit: true }`.
    - (b) Commit 1 is held (`true`), and commit 2 fails with a plain store error. The create
      ends `InDoubt { may_still_commit: true }`, not `CreateError::Store`.
    - (c) Commit 1 is held (`true`) on the last budgeted attempt, and the settling read fails.
      The create ends `InDoubt { may_still_commit: true }`.
    - (d) Commit 1 returns `false` (out of flight, applied on the wrapper's choice), nothing
      earlier is in doubt, and the settling read fails. The create ends
      `InDoubt { may_still_commit: false }`, never `CreateError::Store`, because the batch may
      have landed. Run it applied and unapplied, with the same answer both times.
  - **F9, the error contract.** For every `InDoubt` the legs above produce,
    `wyrd_traits::classify(&err) == ErrorClass::Indeterminate` (`crates/traits/src/lib.rs:786`,
    `:699`), whatever stopped the create, a `TransientFault` cause included (assert one), and
    `classify(&err).is_transient()` is false (`:733`). Its `Display` contains the upload id and
    the nonce. Every other `CreateError` variant's `Display` is asserted by at least one leg.
    `CreateError::Store` wrapping a `TransientFault`, with nothing in doubt, still classifies
    `Transient`.
  **(G) Seeded coverage** (`AGENTS.md:189-190`: "a new destructive or concurrent path lands
  with seeded Tier-0 DST coverage"). The madsim scenario runs concurrent creates on
  `SimFdbMetadataStore` with the commit-ambiguity nemesis armed
  (`crates/dst/tests/support/mod.rs:696`, `quiesce` `:727`). It has these legs: 1021 alone,
  1031 alone, a mixed leg where each commit's code is a seed-derived coin (so one writer can
  meet a 1031 and then a 1021), and the single-creator witness leg below. After every seed, on the store after `quiesce`:
  - the number of `mpu:` records equals `mpuctl.count()`;
  - every `mpu:` record has its `seggrp:` marker, and no marker is without its session;
  - every `mpu:` record belongs to a create that answered `Created` for it, or to one whose
    `InDoubt` named that upload id. None is stranded;
  - **the reverse: every `Created` is real.** For each create that answered `Created`, its
    `mpu:<id>` exists and decodes as an `Open` session for the requested bucket and object, its
    `seggrp:<nonce>` marker exists, and `mpuctl.count()` counts it.
  **Witnesses, each a failing assertion when the sweep never meets it** (a sweep that cannot
  exercise a branch is vacuous, not green):
  - the mixed leg observes at least one writer meeting a 1031 and then a 1021;
  - the 1021 leg observes at least one 1021 that did **not** apply (`ambiguous_commits >
    ambiguous_commits_that_landed`, `FdbObservations`, `crates/dst/tests/support/mod.rs:527-555`);
  - **new:** at least one create that ended `InDoubt { may_still_commit: true }` and whose batch
    **landed by `quiesce`**: its `mpu:<id>` is exactly that create's R, and it is counted. In
    attempt 1, 0 of 99 `InDoubt` endings across 192 seed runs landed, because other creators
    always moved `mpuctl`. A fourth leg reaches it: **one** creator, a profile whose contention
    budget is a few units, and 1031 armed for more commits than that budget. The 1031 fates are
    "landed now / in flight / never sent" on a seed coin (`:811-821`), and an in-flight batch
    lands at `quiesce` when nothing moved its preconditions (`:597-608`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `CreateMultipartUpload`'s admission and session creation as a store-I/O function in
  `wyrd-core`, answering in the landed vocabulary. That includes:
  - recovering an unknown commit outcome under the original identity, by the decision table in
    Design;
  - the error contract (F9);
  - the encoded-size headroom check;
  - the one creation route each for a session record and for the ledger record.

  Also in scope:
  - the seeded DST scenario `crates/dst/tests/multipart_admission.rs`, and the small additions
    to `crates/dst/tests/support/mod.rs` it needs;
  - updating the multipart module header (`multipart.rs:5-7`, and its living-architecture
    paragraph, whose `05-building-block-view.md:202-204` reference at `:105` is now `:206`) and
    the living-architecture paragraph itself to say that Create now writes;
  - the doc comments on `MAX_ADMISSION_CAS_ATTEMPTS` and `ADMISSION_BACKOFF_CAP_MILLIS`
    (`:5011-5023`, `:5029-5031`). They must stop saying the budget is fixed at `MAX_SESSIONS`
    and that a contention refusal always means a full ledger;
  - one doc line each on `KnobSet::max_upload_id_attempts` and
    `KnobSet::max_admission_cas_attempts` (`:5098-5101`), saying Create does not read them yet
    and that #508 reconciles them (Design § *Decided*).

  / out of scope:
  - `AbortMultipartUpload`, the shared fence batch builder, and any change to
    `crates/custodian/src/restore.rs` (the sibling #895);
  - UploadPart and slot reservation (#657), and Complete (#658);
  - the retirement drain and the terminal delete that decrements `mpuctl` (#659);
  - the reaper and operator abort (#625, #633);
  - every gateway, S3, CLI or custodian **caller** of Create. That includes implementing
    `MultipartGateway::create_multipart_upload`, which answers `NotImplemented` on `main`
    (`crates/gateway-core/src/lib.rs:742-755`, #666); the S3 verb is #668;
  - the bucket deletion-fence key and its `require_absent` (#511, see Design);
  - changing `KnobSet`'s fields or `knob_clamps_hold`, and adding a separate cap for unknown
    outcomes (both decided, see Design);
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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 343 of 344 instrumentable changed lines executed (floor 80%); 344 of 961 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 75 mutants tested in 3m: 59 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_894/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.02s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #894’s atomic multipart-session admission and unknown-outcome recovery: no implementation defect found; the declared verification exception and fitness approval remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The admission contract, excluded callers, and latest transitioned-session correction are explicit and mutually consistent; the patch follows that correction (`brief.md:263`, `brief.md:662`, `crates/core/src/multipart/create.rs:376`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API with no executable pre-fix discriminator; independently stashing production while retaining tests produces missing-import E0432, not a failing behavioral assertion (`brief.md:298`, `reviewer-core-red.log:2`). |
| C3 Change | PASS | Admission cannot install an uncounted session: the ledger CAS, both identity reservations, and exact bucket marker constrain the same atomic batch (`crates/core/src/multipart/create.rs:683`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared absence of a behavioral red for this new API — restored production passes 41 core tests and 7 seeded DST tests, but reverted production cannot compile their discriminator (`brief.md:311`, `reviewer-core-red.log:2`, `reviewer-core-restored.log:48`, `reviewer-ci.log:4121`). |
| C5 Causal adequacy | PASS | Recovery preserves ownership and reports an already-admitted session even after a fence changes its bytes; regressions exercise both ordinary recovery and settling reads, with no capability-probe workaround (`crates/core/src/multipart/create.rs:376`, `crates/core/src/multipart/create.rs:627`, `crates/core/tests/multipart_create.rs:2878`). |
| T1 Structure | PASS | Core retains the narrow MetadataStore seam and receives time, randomness, and waits from its caller; no concrete backend or runtime dependency enters production core (`crates/core/src/multipart/create.rs:26`, `crates/core/src/multipart/create.rs:37`, `crates/core/src/multipart/create.rs:72`). |
| T2 Shape | PASS | Construction preserves canonical records and admits only sessions whose widest later encoding fits; independent boundary fixtures cover both Completing and Completed dominance and equality at the ceiling (`crates/core/src/multipart.rs:2355`, `crates/core/src/multipart.rs:2388`, `crates/core/tests/multipart_create.rs:1490`). |
| T3 Runtime | PASS | Retries terminate under the store’s deadline contract, collisions have a separate budget, and earlier commit ambiguity survives later faults without becoming retryable; cancellation limits are documented (`crates/core/src/multipart/create.rs:318`, `crates/core/src/multipart/create.rs:448`, `crates/core/src/multipart/create.rs:553`, `crates/core/tests/multipart_create.rs:2687`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must rerun at publish, so their absence needs no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The latest carry-forward cases are covered, the accepted scope and tracked deferrals are respected, and the seeded witness actually exercises a doubtful batch landing after return (`brief.md:662`, `crates/core/tests/multipart_create.rs:2687`, `crates/core/tests/multipart_create.rs:2878`, `crates/dst/tests/multipart_admission.rs:549`, `crates/core/src/multipart/create.rs:701`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this core-only admission/recovery contract satisfies slice 3a — executable evidence covers the store function, while user-visible S3 behavior and caller wiring intentionally await #668 (`brief.md:360`, `crates/core/src/multipart/create.rs:21`). |

Source citations above resolve under `$PDCA_TARGET`; brief, gate, and reviewer-evidence citations resolve in the review directory. The disposable target matched the patch: reverse-apply checking and `git diff --check` passed, and the production stash was restored with no remaining stash. No target-state caveat applies.

Independent execution supports the implementation verdict, with one resolved host caveat:

- **Core and DST: PASS.** Ran `cargo test --offline -p wyrd-core --test multipart_create` against the target, stashed the production changes while retaining tests, confirmed the compile-only pre-fix failure, restored production, and reran successfully (`reviewer-core-red.log:2`, `reviewer-core-restored.log:48`). The full `cargo xtask ci` rerun exercised the seven multipart DST tests, including both expected-panic controls and the late-landing witness, with the runner’s 50-seed setting (`reviewer-ci.log:4110`, `crates/dst/tests/multipart_admission.rs:458`, `xtask/src/main.rs:1812`).
- **CI checks: PASS after separate advisory retries; the original aggregate exited 1.** The full rerun completed 212 successful test summaries with no failing test summary, plus formatting, clippy, builds, documentation lint/render, typos, dependency checks, conformance, and static-state/dependency guards. Only the two advisory checks failed because their database lock was on a read-only path (`reviewer-ci.log:3551`, `reviewer-ci.log:4167`). Both then passed using the actual cargo-deny executable, unchanged policies, and an offline copy of the same advisory database at revision `64f92cdc` dated 2026-10-17; only `advisories.db-path` changed (`reviewer-deny.log:1`, `reviewer-deny-all-features.log:1`). This was a sandbox fault, not a patch defect or an unmet dependency. Both declared external tools, typos and docs-renderer, executed (`reviewer-ci.log:2`, `reviewer-ci.log:7`).
- **TiKV feature checks: PASS.** Independently reran the gate’s two clippy commands for `wyrd-metadata-tikv --features tikv --tests` and `wyrd-server --features tikv,etcd --tests` (`reviewer-host-tikv.log:103`, `reviewer-host-tikv.log:202`). This establishes feature compilation, not a live-cluster exercise.

The remaining frozen evidence was read and kept distinct from independent reruns:

- **C4-ci:** the frozen retry passed every check. Its earlier health-probe failure also passed in this independent run (`gate-logs/C4-ci.log:2623`, `gate-logs/C4-ci.log:6452`, `reviewer-ci.log:2599`). **C4-verify** agrees with the exception already assigned to C4: exit 77, with no pre-fix test executed (`gate-logs/C4-verify.log:22`).
- **C4-diff-cov: PASS.** The frozen measurement executes 343/344 instrumentable changed lines, 99.7%; 617 other changed lines are unscored, not evidence of execution (`gate-logs/C4-diff-cov.log:98`, `gate-logs/C4-diff-cov.log:112`).
- **C5-mutants: PASS.** The frozen run reports a passing baseline, 59 caught mutants, and 16 unviable mutants out of 75; it does not provide a transcript of every brief-specific manual mutation (`gate-logs/C5-mutants.log:10`).
- **T4-batch-review: PASS.** The frozen review reports zero blocking findings; this leaf also independently inspected production, adversarial tests, and the target rubric (`gate-logs/T4-batch-review.log:10`). **T4-contribution: N/A**, with the artifact audit owed at publish (`gate-logs/T4-contribution.log:10`). **host-tikv: PASS**, corroborated by the independent feature checks (`gate-logs/host-tikv.log:211`, `reviewer-host-tikv.log:202`).

The instance-root coverage, mutation, and batch-review wrappers are not supplied in this leaf; their findings above come from their frozen logs, not a claimed rerun. No gate evidence is missing.

Prior-art and documentation checks passed. The brief records merged and rejected work searched by affected path (`brief.md:391`). Independent GitHub queries refreshed merged history for all seven changed paths: the new create module and two new test files have no merged history (`reviewer-prior-art.json:28`, `reviewer-prior-art.json:33`, `reviewer-prior-art.json:52`). Comparing affected paths with all 19 closed, unmerged PRs found no overlap (`reviewer-closed-pr-paths.json:2`, `reviewer-closed-pr-paths.json:12`); #636 remains closed as not planned. Historical local attempts remain grounded in the supplied brief rather than unavailable builder notes. The living architecture describes the new writer and absent production callers (`docs/design/architecture/05-building-block-view.md:206`); the accepted size, #511 fence deferral, and #508 configuration reconciliation are not reopened.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 894, iteration 4 (multipart admission: Create)

I re-ran the green leg in a scratch copy of the patched tree (`multipart_create`: 41/41 pass) and ran 23 one-line mutation probes against the production code. The iteration-3 fix holds: every call site of the new "own session moved on" path goes red when broken. I could not find a wrong answer on any path the tests cover. What I did find is two rules the patch's own docs and the sign-off directives state, where the logic can be changed with every test still green.

## Findings

- NEEDS-HUMAN [impl] — **The nonce compare in `same_session` is not pinned by any test.** `crates/core/src/multipart.rs:2456` (`&& *segment_nonce == other.segment_nonce`) can be deleted and all 41 tests still pass. The iteration-3 directive said "a record with a different nonce stays a collision (C6 behaviour unchanged)". But every rival in the suite is built with `object("rival")` (`crates/core/tests/multipart_create.rs:1078`). So the `object` compare alone catches every rival, and the nonce compare is never what decides. **Concrete failing case:** create A (upload id U, nonce N1, object `photos/first`) gets a 1021 (`may_still_commit: false`, not applied). Before A re-reads, a rival create with the same U, nonce N2 and the **same** request installs its session. The real code reads that as a collision and answers `Created` under a new identity. With the nonce compare dropped, A wrongly reads the rival's session as its own moved-on session and ends `InDoubt(SessionMoved)` naming U/N1. I added a throwaway probe test for exactly this case (a `RivalFirst { may_still_commit: false, nonce: RIVAL }` whose rival uses the create's own object). It passes on the real code and fails on the mutant. The fix is one test: let `RivalFirst` take the rival's object, and add that C6 variant. The `parent`, `content_type`, `created_at_millis` and `clock_source` compares (`multipart.rs:2451-2455`) can each be dropped with no test going red too. Those only matter when the upload id **and** nonce both collide, so the nonce is the one worth pinning. The C5-mutants gate's "0 missed" (`gate-logs/C5-mutants.log`) does not cover this: cargo-mutants flips `&&` to `||` but never deletes a conjunct.

- NEEDS-HUMAN [impl] — **The "bucket before ledger" read order is not pinned.** The brief's Design § *Order* and the iteration-3 directive both say bucket → ledger → contention. The shipped tests pin bucket → contention (`b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket`, `crates/core/tests/multipart_create.rs:1222`) and ledger → contention (`b_a_ledger_filled_under_the_commit_is_judged_before_contention`, `:1261`). Nothing pins bucket → ledger. Swapping the bucket block (`crates/core/src/multipart/create.rs:407-415`) with the ledger block (`:417-444`) leaves all 41 tests green. `b_a_missing_bucket_answers_no_such_bucket_and_writes_nothing` (`multipart_create.rs:1203`) runs on a store with no ledger, so the ledger check never fires there. **Concrete failing case:** bucket marker deleted, and `mpuctl` at `count == max_sessions` (`profile_of(1)`, ledger count 1). The real code answers `Refused(NoSuchBucket)`. The swapped code answers `Refused(SlowDown { SessionCap { count: 1, max_sessions: 1 } })`, a retryable 503 for a bucket that no longer exists. A skewed or torn ledger behind a missing bucket would also come back `ProfileSkew` or `CreateError::Ledger` instead of `NoSuchBucket`. I confirmed this with a throwaway probe test: green on the real code, red on the swap. The fix is that one test.

## Attempted and could not refute

- **The iteration-3 fix (own session fenced before the re-read).** I broke each call site in turn: the loop branch (`create.rs:376`), the settling read in `finish` (`:630`), the held identity-taken branch (`:389`), and "moved on" read as `Created` (`:513`). Each was caught by the F10 legs (`multipart_create.rs:2878`, `:2913`, `:2951`). Dropping the `object` compare is caught by C6 (iii).
- **`same_session` too strict?** If a later transition rewrote one of the fields it compares, a moved-on own session would be misread as a collision and re-minted, leaving it stranded. Proposal 0016's transitions move only `epoch`, `attempts` and `state`, and its clock-guard rule (`0016:1966-1980`) has later writers *check* `clock_source`, never rewrite it. The decoder's cross-field rules (`multipart.rs:2634-2659`) bind only `Completing`. Not refuted.
- **Re-mint while a batch can still land.** A re-mint happens only when `doubt.held` is `None` (`create.rs:384`). An out-of-flight 1021 is cleared only after a successful read of `mpu:<id>` that found neither R nor the moved-on session (`:381`). I found no path that re-mints over a batch that could still land, outside the stated same-id/same-nonce/same-request bound and the #659 terminal-delete case the brief defers.
- **Demonstrated-reds table, spot-checked.** Rows 2, 4, 5, 7, 12, 15, 17, 18a, 18b, 18c and 21 each went red on the leg the brief names. So did the iteration-2 items (doubt reset on re-mint, caught by C5-after-re-mint; 1021 settled by the loop's re-read, caught by F9). One extra mutant survived: replacing an earlier *held* outcome with a later held one. It only changes which `CommitUnknownResult` the error carries, and no answer changes, so I am not filing it.
- **Size headroom.** The widest measurement uses `u32::MAX` / `u64::MAX` for `attempts` and `epoch`, which the decoder does not bound. It uses `MAX_SEGMENT_INDEX + 1` for the segment cursor, the decoder's own bound (`multipart.rs:2640`). `Completing` puts the object name twice, as the decoder requires (`:2645`). So the check matches the brief's "widest the decoder allows".
- **Termination.** Every loop pass returns, spends a collision, spends a contention unit, or commits. A commit that doesn't return sets `lost`, so the next pass spends something. No unbounded path.
- **Unknown-outcome detection.** The single top-level downcast at `create.rs:464` matches the seam rule that `CommitUnknownResult` is distinguishable "by one downcast on every backend" (`crates/traits/src/lib.rs:1309-1312`).

## Gate notes (no action)

- C4-ci is marked flaky. Attempt 1 failed in `crates/server/tests/health_probe.rs:268` (the health service answered `Unimplemented`, `gate-logs/C4-ci.log:2623`). This diff touches no file under `crates/server`, so it is not this patch's failure. Attempt 2 passed.
- C4-verify is UNVERIFIABLE (exit 77), as the brief declared in advance: the test can't compile without the new module. As substitute red evidence I re-ran the green leg and the mutation probes above. I did not rebuild the madsim DST target. I relied on the frozen gate logs, which show the 7 `multipart_admission` tests green under `--cfg madsim` (`gate-logs/C4-verify.log`, `gate-logs/C4-diff-cov.log`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean under both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed the patch against the read-only target source, brief, and frozen gate evidence. No tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the declared absence of a behavioral red for this new API — restored production passes 41 core tests and 7 seeded DST tests, but reverted production cannot compile their discriminator (`brief.md:311`, `reviewer-core-red.log:2`, `reviewer-core-restored.log:48`, `reviewer-ci.log:4121`).
- [x] Validation — fitness-to-purpose — Decide whether this core-only admission/recovery contract satisfies slice 3a — executable evidence covers the store function, while user-visible S3 behavior and caller wiring intentionally await #668 (`brief.md:360`, `crates/core/src/multipart/create.rs:21`).
- [ ] **The nonce compare in `same_session` is not pinned by any test.** `crates/core/src/multipart.rs:2456` (`&& *segment_nonce == other.segment_nonce`) can be deleted and all 41 tests still pass. The iteration-3 directive said "a record with a different nonce stays a collision (C6 behaviour unchanged)". But every rival in the suite is built with `object("rival")` (`crates/core/tests/multipart_create.rs:1078`). So the `object` compare alone catches every rival, and the nonce compare is never what decides. **Concrete failing case:** create A (upload id U, nonce N1, object `photos/first`) gets a 1021 (`may_still_commit: false`, not applied). Before A re-reads, a rival create with the same U, nonce N2 and the **same** request installs its session. The real code reads that as a collision and answers `Created` under a new identity. With the nonce compare dropped, A wrongly reads the rival's session as its own moved-on session and ends `InDoubt(SessionMoved)` naming U/N1. I added a throwaway probe test for exactly this case (a `RivalFirst { may_still_commit: false, nonce: RIVAL }` whose rival uses the create's own object). It passes on the real code and fails on the mutant. The fix is one test: let `RivalFirst` take the rival's object, and add that C6 variant. The `parent`, `content_type`, `created_at_millis` and `clock_source` compares (`multipart.rs:2451-2455`) can each be dropped with no test going red too. Those only matter when the upload id **and** nonce both collide, so the nonce is the one worth pinning. The C5-mutants gate's "0 missed" (`gate-logs/C5-mutants.log`) does not cover this: cargo-mutants flips `&&` to `||` but never deletes a conjunct.
- [ ] **The "bucket before ledger" read order is not pinned.** The brief's Design § *Order* and the iteration-3 directive both say bucket → ledger → contention. The shipped tests pin bucket → contention (`b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket`, `crates/core/tests/multipart_create.rs:1222`) and ledger → contention (`b_a_ledger_filled_under_the_commit_is_judged_before_contention`, `:1261`). Nothing pins bucket → ledger. Swapping the bucket block (`crates/core/src/multipart/create.rs:407-415`) with the ledger block (`:417-444`) leaves all 41 tests green. `b_a_missing_bucket_answers_no_such_bucket_and_writes_nothing` (`multipart_create.rs:1203`) runs on a store with no ledger, so the ledger check never fires there. **Concrete failing case:** bucket marker deleted, and `mpuctl` at `count == max_sessions` (`profile_of(1)`, ledger count 1). The real code answers `Refused(NoSuchBucket)`. The swapped code answers `Refused(SlowDown { SessionCap { count: 1, max_sessions: 1 } })`, a retryable 503 for a bucket that no longer exists. A skewed or torn ledger behind a missing bucket would also come back `ProfileSkew` or `CreateError::Ledger` instead of `NoSuchBucket`. I confirmed this with a throwaway probe test: green on the real code, red on the swap. The fix is that one test.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FLAKY — first run fail, confirm re-run pass (attempts: fail → pass); recorded pass (both runs in gate-logs/C4-ci.log). Confirm the red sample was environmental, not the patch — xtask ci: all checks passed
- [x] **The contention guarantee contradicts the prescribed retry behavior.** `brief.md:205` promises that a refusal never comes from ordinary concurrency while the ledger has room; `brief.md:496` requires publishing that guarantee. Yet row 7 (`brief.md:408`) spends contention budget even when every precondition still holds, and row 10 refuses when that budget expires. This is a real backend behavior: `crates/metadata-tikv/src/lib.rs:1386` routes locking-read failures through `conflict_or_err`, which returns known-unapplied `Conflict` at `:934` without requiring ledger progress. Repeated lock conflicts can therefore exhaust the budget without filling or even changing `mpuctl`, and without an unknown outcome. Narrow the guarantee to schedules where each loss represents another admission; explicitly document and test exhaustion from lock conflicts with unchanged records. This does not require reopening the settled budget decision.
- [x] **The size mutation requirement has both a missing discriminator and an equivalent mutation.** `brief.md:73` requires only one-byte-under and one-byte-over fixtures, but mutation 18 (`brief.md:306`) requires `>` → `>=` to fail: those fixtures produce identical answers under both comparisons. Add an exactly-at-ceiling admission case; the target's ceiling contract explicitly admits equality (`crates/core/src/metadata.rs:602`). Also, “one state dropped” cannot universally imply a failure: `Aborting {}` has no payload (`crates/core/src/multipart.rs:2132`), whereas `Completing` and `Completed` have longer tags plus required payloads (`:2122`, `:2136`). With identical common fields at their widest values, dropping Aborting cannot change the maximum. Specify non-equivalent omissions, with fixtures where Completing and Completed respectively determine the maximum, instead of treating every surviving omission as missing coverage (`brief.md:285`).
- [x] **F7's assertions do not establish its advertised fresh-ledger recovery.** `brief.md:134` requires eventual success under the original identity and count `c + 2`; mutation 9 (`brief.md:297`) is supposed to fail F7. A recovery that replays the original batch once, receives `Conflict`, then follows row 7's normal fresh-read retry (`brief.md:408`) satisfies every stated F7 assertion when budget remains. Moreover, placing the competing create merely “before the recovery's commit” can invalidate even a correctly refreshed batch. The target proposal requires rebuilding from a fresh read (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2019`). Place the competing admission before the recovery ledger read and require the wrapper to inspect the first recovery batch's `mpuctl` precondition and increment. That distinguishes the promised behavior from a stale replay followed by eventual success.
- [x] size backstop — this slice is behaving oversized: patch is 228 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Code is correct; the only gaps are two unpinned rules. Add exactly two tests, change no production code. (1) Pin the nonce compare in same_session (crates/core/src/multipart.rs:2456): let RivalFirst take the rival's object, and add a C6 variant where create A (upload id U, nonce N1) gets a 1021 (may_still_commit: false, not applied), then a rival with the same U, a different nonce N2 and the SAME request (bucket/object) installs its session before A re-reads. Assert A answers Created under a new identity (a collision, not InDoubt(SessionMoved)), and the rival's record is byte-identical afterwards. Must go red if the nonce conjunct is deleted. (2) Pin read order bucket -> ledger: bucket marker absent AND mpuctl at count == max_sessions (profile_of(1), ledger count 1) -> Refused(NoSuchBucket), not SlowDown{SessionCap}, and writes nothing. Must go red if the bucket block (create.rs:407-415) is moved after the ledger block (:417-444). No other changes; size already accepted.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
