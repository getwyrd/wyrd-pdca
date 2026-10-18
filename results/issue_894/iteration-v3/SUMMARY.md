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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 313 of 314 instrumentable changed lines executed (floor 80%); 314 of 874 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 58 mutants tested in 2m: 44 caught, 14 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_894/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.20s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #894: add atomic multipart admission and session creation; a reproduced recovery/lifecycle conflict needs a scope decision before the outstanding review finding can be disposed of.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The admission contract, answer table, exclusions, and carry-forward obligations are explicit enough to judge this slice; the lifecycle conflict below requires a decision beyond those instructions (`brief.md:263`, `brief.md:446`, `brief.md:650`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no pre-existing Create entry point: independently reverting production while retaining the test produces E0432 before any assertion executes, not a behavioral red (`reviewer-verification.log:4`, `brief.md:298`). |
| C3 Change | PASS | The changes stay within the authorized Create, test, simulation-support, and architecture surfaces; the four carry-forward cases and cancellation contract are present (`crates/core/tests/multipart_create.rs:1809`, `crates/core/tests/multipart_create.rs:2172`, `crates/core/tests/multipart_create.rs:2254`, `crates/core/tests/multipart_create.rs:2593`, `crates/core/src/multipart/create.rs:306`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or decline the declared new-API baseline exception — patched tests pass independently, but removing the API prevents an executable pre-fix red; coverage and mutation evidence cannot establish that missing comparison (`reviewer-verification.log:6`, `reviewer-verification.log:76`, `brief.md:311`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether recovery after a legal session fence must be handled here or explicitly assigned to the later lifecycle slice — the probe returns a second ID while the first admitted session remains counted; the brief mandates the re-mint that causes it (`crates/core/src/multipart/create.rs:347`, `crates/core/src/multipart/create.rs:370`, `brief.md:450`, `reviewer-lifecycle-probe.log:18`). |
| T1 Structure | PASS | The store operation respects the MetadataStore seam and core's runtime boundary; caller-supplied time and randomness avoid a second correctness clock (`crates/core/src/multipart/create.rs:26`, `crates/core/src/multipart/create.rs:72`, `crates/core/src/multipart/create.rs:314`). |
| T2 Shape | PASS | The large patch implements the accepted single-slice scope, keeps construction private, and updates the living architecture; its 213,144-byte size was already accepted, so no new size decision is owed (`brief.md:318`, `brief.md:650`, `crates/core/src/multipart.rs:2355`, `docs/design/architecture/05-building-block-view.md:206`). |
| T3 Runtime | PASS | Within the stated store/runtime contracts, retries terminate, identity and contention budgets remain separate, and seeded tests exercise delayed ambiguous commits; cancellation after send is expressly forbidden for future callers (`crates/core/src/multipart/create.rs:296`, `crates/traits/src/lib.rs:1334`, `crates/core/tests/multipart_create.rs:1592`, `crates/dst/tests/multipart_admission.rs:549`, `reviewer-ci.log:4017`). |
| T4 Contribution | FAIL | The required multi-pass review still records one finding without a fixed or rejected disposition; its lifecycle behavior is independently reproduced below. The contribution-artifact subcheck is N/A until its mandatory publish-time rerun (`gate-logs/T4-batch-review.log:10`, `AGENTS.md:210`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Prior art was checked by every affected path against merged history and closed work; the accepted size, knob, bucket-fence, and terminal-delete deferrals remain settled, and the carry-forward test gaps are covered (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:101`, `brief.md:502`, `brief.md:650`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve or decline this as a store-level foundation — S3 behavior and operation alongside the reaper/drain remain outside this slice and cannot be inferred from its passing store tests (`brief.md:360`, `crates/core/src/multipart/create.rs:21`, `docs/design/architecture/05-building-block-view.md:206`). |

**The outstanding concern is a conflict between the prescribed recovery rule and the wider lifecycle guarantee.** A separate reviewer test uses the actual patched `create_session`, a real in-memory redb store, and the existing `SessionRecord::open_teardown` constructor. The first admission commits; a second atomic batch fences that same session to `Aborting@1` and installs its retirement obligation; the original commit then reports an out-of-flight unknown outcome. Recovery compares the changed record against the original Open bytes, treats it as a collision, and admits another identity. Observed result: `Created(id …02)`, original `id …01` still `Aborting`, `mpuctl.count() == 2`; the assertion requiring the original admitted identity to be reported fails (`reviewer-lifecycle-probe.log:18`, `reviewer-lifecycle-probe.rs:25`). The ownership identity did not change: the existing fence changes only epoch/state (`crates/core/src/multipart.rs:2509`).

This reproduces the behavior reported by the batch review, but does not establish a currently wired production race. Source search found no production caller of Create; the existing restore fence requires writers stopped (`crates/custodian/src/restore.rs:494`, `crates/custodian/src/restore.rs:528`). The future reaper may fence Open sessions (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:572`). The brief explicitly excludes that reaper and explicitly prescribes treating non-R as a collision (`brief.md:289`, `brief.md:450`). C5 therefore requests a human scope/contract decision, rather than directing the builder to silently contradict the decision table. The separate #659 deferral concerns a held batch landing after terminal deletion; this probe has neither a held batch nor a deletion (`brief.md:514`). Reproduce from this sandbox with `RUSTUP_TOOLCHAIN=1.96.0 CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline --manifest-path pdca-reviewer-894-tools/Cargo.toml --test lifecycle -- --nocapture`.

**The supplied acceptance tests pass independently; the baseline limitation is real.** A selective stash removed the production changes while retaining the new test, which failed to compile at its new-module import; stash pop restored the patch and all 35 core tests passed (`reviewer-verification.log:1`, `reviewer-verification.log:34`). The full local `cargo xtask ci` completed its tests, including all seven multipart simulation tests with the configured seed sweep (`reviewer-ci.log:4006`). Its only two reported failures were inability to lock the advisory database under the read-only cargo home (`reviewer-ci.log:3447`, `reviewer-ci.log:4063`). Both advisory checks and the full default dependency check then passed with copied policies whose only change was the advisory-cache directory inside this sandbox (`reviewer-advisories.log:1`). This is a resolved host limitation, not a C4 patch defect. The named external dependencies were exercised: typos, docs lint, and docs rendering/link audit ran successfully (`reviewer-ci.log:3`, `reviewer-ci.log:8`).

**Frozen evidence supports its stated results, with the following limits.** The full CI log ends green (`gate-logs/C4-ci.log:4166`); the verify log records the same compile-only baseline failure (`gate-logs/C4-verify.log:23`). Diff coverage reports 313/314 instrumentable lines, with 560 changed production lines unscored, not 99.7% of all changed lines (`gate-logs/C4-diff-cov.log:106`). Mutation evidence reports 44 caught and 14 unviable mutants, not 58 assertion failures (`gate-logs/C5-mutants.log:13`). Coverage and mutation wrappers were adjudicated from those logs rather than independently reproduced. The TiKV log shows both requested feature compilations finishing successfully; it does not show a live TiKV service test (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). The deferred contribution audit is **N/A**, because its artifacts are intentionally drafted after Check and the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`). No gate evidence file is missing.

**Grounding and protocol checks are complete.** Source citations above resolve within `$PDCA_TARGET`; artifact citations resolve within this review sandbox. All seven affected files exist, reverse application of the complete patch succeeds, and `git diff --check` is clean; the target is not stale (`reviewer-grounding.log:1`). GitHub read-only queries checked merged history and closed-work searches by all seven affected paths, confirming the existing record/knob predecessors and absence of the new Create/test files from base history (`reviewer-prior-art.json:2`). The brief records the rejected local attempts; #636 is independently confirmed closed/not-planned, while #508 and #656 remain open parent issues (`brief.md:404`, `reviewer-prior-art.json:413`). No capability-probe symptom guard was added. No `INTEGRATION.md` was supplied or exists in the target, so no additional §4 checklist could be imported. Judged source was restored unchanged; the counterexample lives only in reviewer artifacts.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 894 (multipart-admission-create)

**Verdict:** I could not break `create_session` on any path its tests cover, or find a wrong answer it gives on `main`. I did find two decision-table rules that the shipped suite does not pin. For each I wrote a probe test in a scratch copy: it passes on the patch and goes red under a mutation that the 35 core legs and the C5 mutants run (58 mutants, 0 missed) all let through. Separately, the gating T4 finding is real in principle, but it can't be reached on `main` and leaks nothing. Deciding it means changing the brief's decision table, so it goes to the human.

## Findings

- NEEDS-HUMAN [human] — **T4-batch-review's blocking finding (`crates/core/src/multipart/create.rs:347-373`).** The scenario: a commit returns `CommitUnknownResult { may_still_commit: false }` after it applied, and another writer then rewrites `mpu:<id>` away from R (for example `Open@0` → `Aborting@1`) before the re-read. The re-read finds a record that is not R, with nothing held. Line 352 clears the doubt, lines 363-373 treat it as a collision and re-mint, and the caller gets `Created` for a second session. Nobody is told about the first one. The code is doing what brief row 3 says ("not held: collision"), so this is a question about the table, not an implementation slip. On `main` it can't happen:
  - The only writer that rewrites an `Open` session is the post-restore fence (`crates/custodian/src/restore.rs:1119`). That fence runs "with writers stopped, before any gateway is re-enabled" (`docs/design/architecture/05-building-block-view.md:208`; 0016 X17).
  - Abort (#895), the reaper (#625) and operator abort (#633) don't exist yet.
  - Even if the race happened, nothing leaks. The fenced session's `retire:` obligation was committed with its fence, so the drain (#659) reclaims it and decrements `mpuctl`. The only thing that breaks is the sentence "no session it installs goes unreported" (`create.rs:15-19`, `05-building-block-view.md:206`).

  My suggestion: reject it for this slice with that reason recorded. Give #625 and #633 a note that a create seeing its own record fenced will re-mint, or that slice can refine row 3 (for example, a record that carries this identity's nonce counts as this create's own).
- NEEDS-HUMAN [impl] — **No leg pins `may_still_commit` when the create is both held and unsettled and its reads then fail (`create.rs:521-523`, reached via `:342` → `finish` `:570-578`).** The settling rule says `may_still_commit` is `true` exactly when the create is held. If `standing()` were changed to prefer `unsettled` over `held`, every one of the 35 legs still passes. Yet that change makes the create report `InDoubt { may_still_commit: false }` ("may have been applied") while a batch can still land. The gap is in the tests:
  - F6 (`crates/core/tests/multipart_create.rs:2288`) does reach held + unsettled, but its next read succeeds and clears `unsettled` at `create.rs:352`.
  - F8 (a) (`:2399`) is held only.

  Failing case: plan `[Hold, Unknown { applied: false, may_still_commit: false }]` on the shipped profile, with `GetFault { after_commits: 2, sessions_only: false, skip: 0, fail: usize::MAX, transient: false }`. Expect `in_doubt(outcome, &original, true)`, then `assert_held_batch_lands`. I ran this as a scratch probe: it passes on the patch and fails under the swap at `multipart_create.rs:359` (`unknown.may_still_commit` was `false`). Add it as an F8 (e) leg.
- NEEDS-HUMAN [impl] — **No leg pins the read order "bucket, then ledger, and only then contention" (brief, Design § Order; `create.rs:415-426` must stay after `:376-413`).** Moving the `lost` block above the bucket check passes all 35 legs. With that change, a lost commit that met a full ledger on the last budgeted attempt answers `AdmissionContention` instead of `SessionCap`, and every refusal met after a loss costs one extra wait. Failing case: `profile_of(1)`, plan `[AdmitFirst(1)]`, starting with no ledger. The right answer is `Refused(SlowDown { SessionCap { count: 1, max_sessions: 1 } })` with `script.waits` empty. Scratch probe: green on the patch, red under the move. The cheapest fix is that probe as a (B) leg, plus `assert!(script.waits.is_empty())` in `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` (`multipart_create.rs:1174-1199`). This is low impact, since both answers are a 503 SlowDown, but the brief states the order as a rule.
- (Note, no action needed.) The `InDoubt::outcome` doc says it is "the first that said `may_still_commit`" (`create.rs:130-132`). Changing `create.rs:514` to keep the latest one instead also survives, because every scripted outcome carries identical fields. Only the audit `detail` would differ, and the answer the caller acts on does not change.

## What I tried and could not break

- **Re-ran the evidence.** In a scratch copy, the core suite is 35/35 green. Brief table rows 5 and 6 go red as claimed (row 5 fails C4; row 6 fails C2, C3, C4 and F3).
- **Extra mutations, all caught:**
  - dropping the `unsettled` clear at `:352` (caught by f9 1021-settled);
  - keeping `lost` after a re-mint at `:372` (c2, c3);
  - ignoring a changed marker at `:378` (B marker-moved);
  - `>` for `>=` at `:419` (B contention, f6, f8c);
  - settling only while held at `:570` (f9);
  - dropping the settle read in the held, identity-taken branch at `:358` (f5, between reads);
  - answering `Created` on any successful read there (c6 ii).

  Making `:347` adopt R only while held or unsettled (instead of after any unknown outcome) also survives. That mutation is equivalent outside the stated same-identity, same-request bound, so it is not a gap.
- **Size headroom.** The test builds each state's widest form itself (`multipart_create.rs:1329-1374`) and decodes each through the production decoder. It does not copy production code. Production (`crates/core/src/multipart.rs:2393-2436`) sets every field to the widest the decoder allows: ETag `N ≤ MAX_PART_NUMBER` (`:4296-4302`) and `segments_written ≤ MAX_SEGMENT_INDEX + 1` (`:2613`). `created_at_millis`, `clock_source`, `object` and `content_type` never change after Create. I found no field it under-counts.
- **The new creation route.** The session decoder checks no string content other than the nonce (`multipart.rs:2603-2644`). So any request strings encode and decode back to the same bytes, and `SessionRecord::open` cannot install a session that later fails to decode.
- **DST.** The witness assertions (an unapplied 1021, 1031 then 1021, a held in-doubt batch that lands) live in plain `#[test]` sweeps (`crates/dst/tests/multipart_admission.rs:483-570`), and `C4-ci.log:4109-4118` shows them running under `--cfg madsim`. `defer_in_flight_landings` (`crates/dst/tests/support/mod.rs` diff) picks one legal FDB schedule ("could get committed at a later point in time") and stays off by default.
- **C4-verify `unverifiable`.** This was declared up front: the test calls an API the patch adds, so it can't compile on the old code. That is not a refutation.
- **Rubric checks.**
  - No `static` in the wrappers.
  - No clock read in core.
  - Docs-currency paragraph updated.
  - The bucket deletion-fence key is deferred to #511 with a `// deferred:` marker (`create.rs:641`). That counts as settled.
  - Awaits on the store have no timeout of their own. Sign-off already settled that by adding the cancel-safety paragraph (`create.rs:306-313`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/create.rs:351`: Recovery can discard an identity that this create successfully admitted. If a commit applies but returns `CommitUnknownResult { may_still_commit: false }`, and a teardown fences its session before recovery reads it, the stored record is now `Aborting@1`, not R. The loop clears `unsettled` and re-mints at line 370, potentially admitting a second session while never reporting the first identity. `SessionRecord::open_teardown` (`crates/core/src/multipart.rs:2509`) provides exactly this state change without deleting the session, so the #659 terminal-delete deferral does not exclude the trace. Preserve the original identity when the differing record may be this create's transitioned session (for example, report `InDoubt`), and add an applied-1021-then-fence regression. The brief's row-3 rule needs the same distinction. This confirms the frozen T4 finding.

No additional correctness or actionable reuse, simplification, or efficiency findings. Validation used target-source inspection and the frozen gate logs; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or decline the declared new-API baseline exception — patched tests pass independently, but removing the API prevents an executable pre-fix red; coverage and mutation evidence cannot establish that missing comparison (`reviewer-verification.log:6`, `reviewer-verification.log:76`, `brief.md:311`).
- [ ] C5 Causal adequacy — Decide whether recovery after a legal session fence must be handled here or explicitly assigned to the later lifecycle slice — the probe returns a second ID while the first admitted session remains counted; the brief mandates the re-mint that causes it (`crates/core/src/multipart/create.rs:347`, `crates/core/src/multipart/create.rs:370`, `brief.md:450`, `reviewer-lifecycle-probe.log:18`).
- [ ] Validation — fitness-to-purpose — Approve or decline this as a store-level foundation — S3 behavior and operation alongside the reaper/drain remain outside this slice and cannot be inferred from its passing store tests (`brief.md:360`, `crates/core/src/multipart/create.rs:21`, `docs/design/architecture/05-building-block-view.md:206`).
- [ ] **T4-batch-review's blocking finding (`crates/core/src/multipart/create.rs:347-373`).** The scenario: a commit returns `CommitUnknownResult { may_still_commit: false }` after it applied, and another writer then rewrites `mpu:<id>` away from R (for example `Open@0` → `Aborting@1`) before the re-read. The re-read finds a record that is not R, with nothing held. Line 352 clears the doubt, lines 363-373 treat it as a collision and re-mint, and the caller gets `Created` for a second session. Nobody is told about the first one. The code is doing what brief row 3 says ("not held: collision"), so this is a question about the table, not an implementation slip. On `main` it can't happen:
- [ ] **No leg pins `may_still_commit` when the create is both held and unsettled and its reads then fail (`create.rs:521-523`, reached via `:342` → `finish` `:570-578`).** The settling rule says `may_still_commit` is `true` exactly when the create is held. If `standing()` were changed to prefer `unsettled` over `held`, every one of the 35 legs still passes. Yet that change makes the create report `InDoubt { may_still_commit: false }` ("may have been applied") while a batch can still land. The gap is in the tests:
- [ ] **No leg pins the read order "bucket, then ledger, and only then contention" (brief, Design § Order; `create.rs:415-426` must stay after `:376-413`).** Moving the `lost` block above the bucket check passes all 35 legs. With that change, a lost commit that met a full ledger on the last budgeted attempt answers `AdmissionContention` instead of `SessionCap`, and every refusal met after a loss costs one extra wait. Failing case: `profile_of(1)`, plan `[AdmitFirst(1)]`, starting with no ledger. The right answer is `Refused(SlowDown { SessionCap { count: 1, max_sessions: 1 } })` with `script.waits` empty. Scratch probe: green on the patch, red under the move. The cheapest fix is that probe as a (B) leg, plus `assert!(script.waits.is_empty())` in `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` (`multipart_create.rs:1174-1199`). This is low impact, since both answers are a 503 SlowDown, but the brief states the order as a rule.
- [ ] `crates/core/src/multipart/create.rs:351`: Recovery can discard an identity that this create successfully admitted. If a commit applies but returns `CommitUnknownResult { may_still_commit: false }`, and a teardown fences its session before recovery reads it, the stored record is now `Aborting@1`, not R. The loop clears `unsettled` and re-mints at line 370, potentially admitting a second session while never reporting the first identity. `SessionRecord::open_teardown` (`crates/core/src/multipart.rs:2509`) provides exactly this state change without deleting the session, so the #659 terminal-delete deferral does not exclude the trace. Preserve the original identity when the differing record may be this create's transitioned session (for example, report `InDoubt`), and add an applied-1021-then-fence regression. The brief's row-3 rule needs the same distinction. This confirms the frozen T4 finding.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_894/review-b
- [ ] **The contention guarantee contradicts the prescribed retry behavior.** `brief.md:205` promises that a refusal never comes from ordinary concurrency while the ledger has room; `brief.md:496` requires publishing that guarantee. Yet row 7 (`brief.md:408`) spends contention budget even when every precondition still holds, and row 10 refuses when that budget expires. This is a real backend behavior: `crates/metadata-tikv/src/lib.rs:1386` routes locking-read failures through `conflict_or_err`, which returns known-unapplied `Conflict` at `:934` without requiring ledger progress. Repeated lock conflicts can therefore exhaust the budget without filling or even changing `mpuctl`, and without an unknown outcome. Narrow the guarantee to schedules where each loss represents another admission; explicitly document and test exhaustion from lock conflicts with unchanged records. This does not require reopening the settled budget decision.
- [ ] **The size mutation requirement has both a missing discriminator and an equivalent mutation.** `brief.md:73` requires only one-byte-under and one-byte-over fixtures, but mutation 18 (`brief.md:306`) requires `>` → `>=` to fail: those fixtures produce identical answers under both comparisons. Add an exactly-at-ceiling admission case; the target's ceiling contract explicitly admits equality (`crates/core/src/metadata.rs:602`). Also, “one state dropped” cannot universally imply a failure: `Aborting {}` has no payload (`crates/core/src/multipart.rs:2132`), whereas `Completing` and `Completed` have longer tags plus required payloads (`:2122`, `:2136`). With identical common fields at their widest values, dropping Aborting cannot change the maximum. Specify non-equivalent omissions, with fixtures where Completing and Completed respectively determine the maximum, instead of treating every surviving omission as missing coverage (`brief.md:285`).
- [ ] **F7's assertions do not establish its advertised fresh-ledger recovery.** `brief.md:134` requires eventual success under the original identity and count `c + 2`; mutation 9 (`brief.md:297`) is supposed to fail F7. A recovery that replays the original batch once, receives `Conflict`, then follows row 7's normal fresh-read retry (`brief.md:408`) satisfies every stated F7 assertion when budget remains. Moreover, placing the competing create merely “before the recovery's commit” can invalidate even a correctly refreshed batch. The target proposal requires rebuilding from a fresh read (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2019`). Place the competing admission before the recovery ledger read and require the wrapper to inspect the first recovery batch's `mpuctl` precondition and increment. That distinguishes the promised behavior from a stale replay followed by eventual success.
- [ ] size backstop — this slice is behaving oversized: patch is 208 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Losing track of an admitted session is the worst outcome, so fix it now rather than deferring to the reaper slice. Fix (overrides brief decision-table row 3 for this one case): after an unknown commit outcome under the current identity, if the re-read of mpu:<id> finds a record that is not R but carries THIS identity's nonce (and request: bucket/object) — e.g. the session was fenced Open@0 -> Aborting@1 via SessionRecord::open_teardown (crates/core/src/multipart.rs:2509) — it is this create's own transitioned session, not a collision. Do not re-mint and do not answer Created under a new id (create.rs:347-373). End with CreateError::InDoubt naming the original upload id and nonce (may_still_commit per the existing held rule). A record with a different nonce stays a collision (C6 behaviour unchanged). Regression: applied 1021 (may_still_commit: false), then a fence to Aborting@1 before the recovery read; assert no second identity minted, mpuctl.count() up by exactly one, and InDoubt names the original id/nonce. This closes the T4 batch-review finding. Also close the adversary's two unpinned rules while rebuilding: (1) F8(e): plan [Hold, Unknown{applied:false, may_still_commit:false}] with every later get failing -> InDoubt{may_still_commit:true}, then the held batch lands; (2) read order bucket -> ledger -> contention: profile_of(1), plan [AdmitFirst(1)], no ledger at start -> Refused(SlowDown{SessionCap{count:1,max_sessions:1}}) with script.waits empty, plus assert waits empty in b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket. No other changes; size already accepted.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
