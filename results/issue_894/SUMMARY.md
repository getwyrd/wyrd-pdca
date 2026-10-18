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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.37s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No patch defects found in #894’s atomic multipart admission/session creation and unknown-commit recovery, including the latest nonce-ownership and refusal-order regressions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accounting/ownership invariant is falsifiable, and the latest two regression obligations are explicit; scope and prior decisions are settled (`brief.md:227`, `brief.md:674`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API, with no pre-existing operation to reproduce; stashing production removes the imported module and prevents test execution (`brief.md:298`, `reviewer-evidence/core-red.log:2`). |
| C3 Change | PASS | The agreed core-only scope is met, including living architecture documentation and both latest regression cases (`target/crates/core/tests/multipart_create.rs:1230`, `target/crates/core/tests/multipart_create.rs:2157`, `target/docs/design/architecture/05-building-block-view.md:206`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API exception to an executable pre-fix red — the reverted test cannot compile; restored core tests and seeded DST pass, and both latest counterfactuals fail at assertions (`brief.md:311`, `reviewer-evidence/core-red.log:2`, `reviewer-evidence/core-final-green.log:50`, `reviewer-evidence/ci.log:4123`). |
| C5 Causal adequacy | PASS | Atomic accounting, preserved uncertainty, and ownership discrimination address the stated failures; removing nonce comparison and reversing refusal order each breaks its required test (`target/crates/core/src/multipart/create.rs:376`, `target/crates/core/src/multipart/create.rs:687`, `reviewer-evidence/nonce-red.log:79`, `reviewer-evidence/order-red.log:14`). |
| T1 Structure | PASS | Dependency direction and runtime independence remain intact: core uses the metadata trait and caller-owned time/randomness/waits, with no new shared global state (`target/crates/core/src/multipart/create.rs:24`, `target/crates/core/src/multipart/create.rs:336`, `reviewer-evidence/ci.log:3567`). |
| T2 Shape | PASS | Canonical records remain suitable for byte-CAS, and admitted sessions retain encoded-size headroom for later states; both widest-state boundary fixtures pass (`target/crates/core/src/multipart.rs:1888`, `target/crates/core/src/multipart.rs:2344`, `target/crates/core/tests/multipart_create.rs:1542`). |
| T3 Runtime | PASS | Separate bounded retry budgets preserve admission under contention; held commits retain their original identity, and cancellation/store-deadline obligations are explicit (`target/crates/core/src/multipart/create.rs:318`, `target/crates/core/src/multipart/create.rs:448`, `target/crates/core/src/multipart/create.rs:553`, `reviewer-evidence/ci.log:4123`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit must rerun at publish, so the deferred row creates no human clearance item (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The latest two requested gaps are closed with demonstrated reds; affected-path prior art includes merged history and closed/rejected work, and accepted scope/size/deferrals are not reopened (`brief.md:674`, `reviewer-evidence/prior-art.log:2`, `reviewer-evidence/prior-art.log:30`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Judge whether the reviewed admission/recovery contract is sufficient for core-only slice #894 — deployment-facing multipart remains unavailable until #668, with its reaper/operator-abort release obligations (`brief.md:360`, `target/crates/core/src/multipart/create.rs:21`). |

The implementation and test evidence support the agreed slice. No implementation iteration is requested. The two NEEDS-HUMAN rows concern acceptance of the new-function verification limitation and fitness to purpose; neither is a deterministic patch failure.

- **Independent red/green evidence:** stashed only the new production module and its parent changes, retaining the new tests. The pre-fix run exited 101 on the missing `multipart::create` import before any assertion ran; this reproduces the frozen gate’s UNVERIFIABLE result, not an executable behavioral red (`reviewer-evidence/core-red.log:2`, `gate-logs/C4-verify.log:22`). Restoring the patch passed all 43 core tests. Removing the nonce conjunct, while removing its now-unused binding, made C6-IV return `InDoubt(SessionMoved)` instead of creating under a new identity. Moving the bucket block after the ledger block made the missing-bucket test return `SessionCap` instead of `NoSuchBucket`. Both tests executed and failed; restoring the exact source passed all 43 again (`reviewer-evidence/nonce-red.log:79`, `reviewer-evidence/order-red.log:14`, `reviewer-evidence/core-final-green.log:50`).
- **Independent gates and resolved host caveat:** the pinned-toolchain `cargo xtask ci` rerun completed its checks, including typos, docs lint/render, fmt, clippy, build, tests, conformance, global-state/dependency guards, and the 50-seed DST invocation. The new DST target ran seven tests, including mixed ambiguity, late landing, and negative witnesses (`target/xtask/src/main.rs:1776`, `reviewer-evidence/ci.log:4114`). The overall command exited 1 solely because two cargo-deny scans could not lock the read-only home advisory cache (`reviewer-evidence/ci.log:3553`, `reviewer-evidence/ci.log:4169`). Both scans then passed with real cargo-deny and an advisory cache inside this sandbox; copied policies differed only in `db-path` (`reviewer-evidence/deny-default.log:1`, `reviewer-evidence/deny-all-features.log:1`). Both TiKV feature clippy commands also passed (`reviewer-evidence/tikv.log:203`). The brief’s external dependencies, typos and docs renderer, were exercised (`reviewer-evidence/ci.log:2`, `reviewer-evidence/ci.log:7`).
- **Mutation and frozen evidence:** independent mutation scanning tested 75 mutants: 45 caught, 30 unviable. A follow-up with compiler lints capped exercised 33 selected mutations: 17 caught, 16 unviable, no survivors (`reviewer-evidence/mutants.log:3`, `reviewer-evidence/mutants-capped.log:3`). Unviable mutations are not credited as behavioral reds. The frozen mutation gate separately records 59 caught and 16 unviable (`gate-logs/C5-mutants.log:13`). The instance-scoped diff-coverage and batched-review wrappers were adjudicated from their captured logs: 343/344 instrumentable changed lines covered, with 617 unscored lines, and zero reported blocking findings (`gate-logs/C4-diff-cov.log:113`, `gate-logs/T4-batch-review.log:10`). These are frozen results, not claimed independent reruns. The contribution audit remains deferred to publish.
- **Grounding and prior art:** all source citations resolve under the supplied disposable `$PDCA_TARGET`; every affected file’s final Git blob hash matches `patch.diff`, and reverse-apply/check-whitespace checks pass (`reviewer-evidence/target-integrity.log:1`). GitHub merged history was queried for all seven affected paths; none of the three new files has merged history. File lists of all 19 closed, unmerged PRs intersect none of those paths. Read-only inspection of #636 and #656 corroborates the rejected shared-budget approach; the brief supplies the local iteration dispositions (`reviewer-evidence/prior-art.log:3`, `reviewer-evidence/prior-art.log:30`, `brief.md:391`). The standing rubric was applied; no capability-probe/load-time workaround was introduced, and tracked deferrals remain settled. Builder notes and withheld iteration artifacts were not consulted.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 894, iteration 5 (multipart-admission-create)

**Verdict: I could not refute the fix.** This round's job was two new tests and no production change (the iteration-4 carry-forward). Both tests go red under the exact edit each was written for, and every extra edit I made to the recovery logic was caught by some test. I have no NEEDS-HUMAN items to file.

## What I re-ran (scratch copy of `$PDCA_TARGET`, removed afterwards)

- **Baseline:** `cargo test --offline -p wyrd-core --test multipart_create` passed, 43 of 43, the same as `gate-logs/C4-diff-cov.log`. I did not re-run the madsim suite. `gate-logs/C4-ci.log:4117-4128` shows all 7 `multipart_admission` tests green, including both `should_panic` reds and the witness leg. `xtask ci: all checks passed` is at `:4174`.
- **Iteration-4 test (1), the nonce check in `same_session`:** I replaced `&& *segment_nonce == other.segment_nonce` (`crates/core/src/multipart.rs:2456`) with `true`. Only `c6_iv_this_creates_request_under_another_nonce_is_still_a_collision` fails, with an `InDoubt` where `Created` was expected (`crates/core/tests/multipart_create.rs:334` panic via `:2170`). The fixture really does isolate the nonce. The rival is built through the same `request()` helper (same `CREATED_AT`, `CLOCK`, no content type), and `:2193-2203` checks that every other field `same_session` compares is equal. So the test cannot pass because some other field happens to differ.
- **Iteration-4 test (2), bucket checked before ledger:** I moved the bucket block (`crates/core/src/multipart/create.rs:407-415`) after the ledger block (`:417-444`). Only `b_a_missing_bucket_is_judged_before_a_full_ledger` fails (`multipart_create.rs:1255`, `SessionCap` where `NoSuchBucket` was expected).

## Extra edits I made to try to break it (cargo-mutants does not generate these)

All were caught. Each line gives the edit and the test that failed.

- `create.rs:381`: removed the line that clears the unsettled flag after a loop read finds some other record. Caught by `f9_a_1021_settled_absent_leaves_nothing_in_doubt`.
- `create.rs:633`: the settling read no longer clears the unsettled flag when it finds some other record. Caught by `f9_a_1021_settled_absent_leaves_nothing_in_doubt`.
- `create.rs:389`: while held, the branch for an identity taken by another record now reports our own moved-on session as `IdentityTaken`. Caught by `f10_its_own_session_moved_on_while_held_is_not_an_identity_taken`.
- `create.rs:389-390`: same idea, with `Moved`, `Other` and `Err` all collapsed into `IdentityTaken`. Caught by the same test.
- `create.rs:376`: the in-loop check for our own moved-on session now runs only when held, not when merely unsettled. Caught by `f10_its_own_session_fenced_before_the_re_read_ends_in_doubt_never_in_a_re_mint`.
- `create.rs:630-632`: when the settling read finds our session moved on, the error now carries the ending as its cause instead of `SessionMoved`. Caught by `f10_its_own_session_moved_on_is_kept_by_the_settling_read`.
- `create.rs:627`: the settling read now runs only when held (an unsettled 1021 is skipped). Caught by `f9_a_1021_settled_absent_leaves_nothing_in_doubt` and `f10_its_own_session_moved_on_is_kept_by_the_settling_read`.
- `create.rs:403`: the "lost" flag is no longer reset when a new identity is minted. Caught by `c2_…`, `c3_…`, and the new `c6_iv_…` (its `waits.is_empty()` assertion at `:2175`).

## Refutation attempts that failed (no finding)

- **Zero contention budget or a bad bootstrap ledger** (`create.rs:352`, `:426`). Ruled out. G7 (`multipart.rs:1775`) forces `w_ref ≥ U_ref`, so `max_sessions() ≥ 1`. That means `admission_cas_attempts()` (`:1711`) is at least 1, and `bootstrap`'s `count: 1` is never above its own limit.
- **A new identity minted while a batch may still land** (`create.rs:383-405`). The only re-mint path is guarded by `if let Some(held)` at `:384`. Any unsettled 1021 has already been cleared at `:381`, by a read of `mpu:<id>` that found neither R nor our own moved-on session. So an out-of-flight batch from an earlier identity cannot land unreported.
- **The termination claim** (`create.rs:320-323`, "at most `max_sessions + MAX_UPLOAD_ID_ATTEMPTS` commits"). Holds. Every commit that does not admit is followed by a return, a counted loss (`:448-450`, capped at the budget), or a counted collision (`:395-396`, capped at 2).
- **A wrong error class when in doubt** (`create.rs:207`). `classify` (`crates/traits/src/lib.rs:786-804`) stops at the first seam type in the chain, and `InDoubt`'s source is the `CommitUnknownResult` itself. A store fault, `TransientFault` or `IntegrityFault` that stopped the create therefore sits under `cause` and never takes over the class. That matches F9's "whatever stopped the create". It does mean an `IntegrityFault` hit while in doubt reads as `Indeterminate`, which is how the brief specified it.
- **Size headroom measured by a copy of production** (`multipart.rs:2388`). It isn't. The test writes its own JSON for each widest state (`multipart_create.rs:1474-1519`), decodes each through the production decoder, and pushes each through a `Ceiling` store that enforces `MAX_VALUE_BYTES` (`:1455-1465`). So the measurement is checked independently, not compared against a mirror of itself.
- **Scope:** no production caller was added. `create_session` is referenced only in `crates/core/src/multipart{.rs,/create.rs}`, `crates/core/tests/multipart_create.rs` and `crates/dst/tests/multipart_admission.rs`, as Production reach (c) requires.
- **Not raised, per the rubric:** a bucket marker deleted and re-created with identical bytes (an ABA case) is the deletion-fence gap marked `// deferred: #511` at `create.rs:701`, so it is settled. The cancel-safety caveat (`create.rs:328-335`) is the paragraph the iteration-2 sign-off asked for.

## Limits of this pass (not refutations)

- I can't confirm from my inputs that this round changed **no production code** compared with iteration 4. `iteration-v4/patch.diff` is not among my inputs. The current production code is what I attacked, and it holds either way.
- C4-verify is `unverifiable` (exit 77, `gate-logs/C4-verify.log`) because the new test calls an API that doesn't exist on the base. The brief (Verification posture (a)) declared this in advance, and it is not a defect. The red side of the evidence is the brief's table of demonstrated reds plus the mutation runs above.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed the changed implementation and tests against the read-only target source, the brief, and frozen gate evidence. The logs report 43 core tests and 7 DST tests passing, with 59 mutants caught and 16 unviable. Gates were not re-run.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the declared new-API exception to an executable pre-fix red — the reverted test cannot compile; restored core tests and seeded DST pass, and both latest counterfactuals fail at assertions (`brief.md:311`, `reviewer-evidence/core-red.log:2`, `reviewer-evidence/core-final-green.log:50`, `reviewer-evidence/ci.log:4123`).
- [x] Validation — fitness-to-purpose — Judge whether the reviewed admission/recovery contract is sufficient for core-only slice #894 — deployment-facing multipart remains unavailable until #668, with its reaper/operator-abort release obligations (`brief.md:360`, `target/crates/core/src/multipart/create.rs:21`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The contention guarantee contradicts the prescribed retry behavior.** `brief.md:205` promises that a refusal never comes from ordinary concurrency while the ledger has room; `brief.md:496` requires publishing that guarantee. Yet row 7 (`brief.md:408`) spends contention budget even when every precondition still holds, and row 10 refuses when that budget expires. This is a real backend behavior: `crates/metadata-tikv/src/lib.rs:1386` routes locking-read failures through `conflict_or_err`, which returns known-unapplied `Conflict` at `:934` without requiring ledger progress. Repeated lock conflicts can therefore exhaust the budget without filling or even changing `mpuctl`, and without an unknown outcome. Narrow the guarantee to schedules where each loss represents another admission; explicitly document and test exhaustion from lock conflicts with unchanged records. This does not require reopening the settled budget decision.
- [x] **The size mutation requirement has both a missing discriminator and an equivalent mutation.** `brief.md:73` requires only one-byte-under and one-byte-over fixtures, but mutation 18 (`brief.md:306`) requires `>` → `>=` to fail: those fixtures produce identical answers under both comparisons. Add an exactly-at-ceiling admission case; the target's ceiling contract explicitly admits equality (`crates/core/src/metadata.rs:602`). Also, “one state dropped” cannot universally imply a failure: `Aborting {}` has no payload (`crates/core/src/multipart.rs:2132`), whereas `Completing` and `Completed` have longer tags plus required payloads (`:2122`, `:2136`). With identical common fields at their widest values, dropping Aborting cannot change the maximum. Specify non-equivalent omissions, with fixtures where Completing and Completed respectively determine the maximum, instead of treating every surviving omission as missing coverage (`brief.md:285`).
- [x] **F7's assertions do not establish its advertised fresh-ledger recovery.** `brief.md:134` requires eventual success under the original identity and count `c + 2`; mutation 9 (`brief.md:297`) is supposed to fail F7. A recovery that replays the original batch once, receives `Conflict`, then follows row 7's normal fresh-read retry (`brief.md:408`) satisfies every stated F7 assertion when budget remains. Moreover, placing the competing create merely “before the recovery's commit” can invalidate even a correctly refreshed batch. The target proposal requires rebuilding from a fresh read (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2019`). Place the competing admission before the recovery ledger read and require the wrapper to inspect the first recovery batch's `mpuctl` precondition and increment. That distinguishes the promised behavior from a stale replay followed by eventual success.
- [x] size backstop — this slice is behaving oversized: patch is 234 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
