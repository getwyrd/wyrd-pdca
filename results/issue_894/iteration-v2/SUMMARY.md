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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.0% — 311 of 314 instrumentable changed lines executed (floor 80%); 314 of 865 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 58 mutants tested in 2m: 44 caught, 14 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_894/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.50s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #894: add atomic multipart-session admission and creation, with bounded contention, collision handling, and recovery of unknown commit outcomes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The counted-session invariant, ownership rules, refusal cases, and scope are explicit and consistent with the normative Create batch; `brief.md:227`, `brief.md:446`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657`. |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality: stashing production changes while retaining the test produces E0432 before any test executes, so there is no behavioral pre-fix red; `reviewer-core-red.log:2`, `brief.md:298`. |
| C3 Change | PASS | The change protects accounting and ownership together: atomic identity/ledger preconditions prevent overwrite and duplicate admission, and recovery preserves the caller's identity; `crates/core/src/multipart/create.rs:338`, `crates/core/src/multipart/create.rs:614`, `crates/core/tests/multipart_create.rs:1905`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept green tests and mutation evidence without an executable pre-fix discriminator for this new API — the independently reproduced E0432 is not a test failure, while restored core and seeded simulation runs pass; `brief.md:311`, `reviewer-core-red.log:2`, `reviewer-checks.log:212`, `reviewer-dst.log:249`. |
| C5 Causal adequacy | PASS | Ownership and uncertainty are tested against the actual failure mechanisms: another session is not adopted, held outcomes remain indeterminate, and late batches cannot double-admit; `crates/core/tests/multipart_create.rs:1733`, `crates/core/tests/multipart_create.rs:1905`, `crates/core/tests/multipart_create.rs:2021`, `crates/dst/tests/multipart_admission.rs:549`. |
| T1 Structure | PASS | Core retains its narrow MetadataStore seam and caller-owned clock/runtime; canonical record construction and living architecture remain aligned with the new API; `crates/core/src/multipart/create.rs:24`, `crates/core/src/multipart/create.rs:305`, `crates/core/src/multipart.rs:2344`, `docs/design/architecture/05-building-block-view.md:206`. |
| T2 Shape | PASS | The seven-file, 196,262-byte patch stays within the accepted admission/test/docs slice; its size was explicitly accepted at Plan and does not add production callers or sibling operations; `brief.md:263`, `brief.md:318`, `crates/core/src/multipart/create.rs:21`. |
| T3 Runtime | PASS | Independent real-redb and seeded simulation runs exercise contention, byte ceilings, late commits, and outcome ownership; retries are finite and store waits inherit the termination contract; `reviewer-checks.log:212`, `reviewer-dst.log:249`, `crates/core/src/multipart/create.rs:296`, `crates/traits/src/lib.rs:1334`. |
| T4 Contribution | N/A | Publication artifacts are intentionally absent at Check; their substantive audit remains mandatory at publish, so the deferred contribution row is neither a pass nor a missing-evidence finding; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Path-based prior-art checks confirm the existing prerequisites and no affected-path overlap in closed/unmerged PRs; the accepted scope and tracked deferrals remain settled; `reviewer-history-by-path.json:1`, `reviewer-rejected-by-path.json:1`, `brief.md:391`, `brief.md:502`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the tested core admission behavior fulfills #894's intended purpose — this slice exposes no production Create caller, so it establishes protocol behavior rather than an end-user S3 upload workflow; `brief.md:360`, `crates/core/src/multipart/create.rs:21`, `docs/design/architecture/05-building-block-view.md:206`. |

Source citations above resolve under `$PDCA_TARGET` (`target/`); brief, report-log, and frozen gate-log citations resolve from this review directory. The target was readable and matched the patch: reverse-apply checking succeeded, and hashes of all seven affected files matched their starting contents after stash/pop and mutation testing. There is no target-state caveat.

The correctness evidence supports the scoped change. The forced schedule checks more CAS losses than the identity budget and a profile larger than the shipped limit (`crates/core/tests/multipart_create.rs:650`, `:681`); the unmoved-conflict case checks bounded refusal without re-minting (`:1163`). Both completing-widest and completed-widest fixtures exercise the encoded ceiling at minus one, equality, and plus one, including teardown through a ceiling-enforcing store (`:1325`). Recovery tests inspect the rebuilt ledger precondition and increment, not merely the final count (`:2087`). The simulation's witness requires an in-doubt session actually to land, and its reverse assertion rejects a Created answer without a stored session (`crates/dst/tests/multipart_admission.rs:426`, `:549`). The new guards express admission and commit-outcome rules; the capability-probe/load-time-side-effect smell test does not fire.

Independent verification produced these results:

- `cargo test --offline --locked -p wyrd-core --test multipart_create`: 31 passed with the patch, E0432 with production changes stashed and tests retained, then 31 passed after restoration and again after mutation testing (`reviewer-core-red.log:2`, `reviewer-checks.log:212`). The compile failure does not establish behavioral red.
- `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50 cargo test --offline --locked -p wyrd-dst --test multipart_admission`: seven passed, including the four scenario legs, explicit seed witnesses, and both expected-panic negative controls (`reviewer-dst.log:240`).
- The diff-selected mutation scan and lint-capped recheck together cover 58 distinct mutants: 44 caught, 14 unviable, none missed or timed out. The first run left warning-producing mutants unviable; the recheck discharged those without treating compilation rejection as behavioral coverage (`reviewer-mutants.log:3`, `reviewer-mutants-capped.log:3`; detailed outcomes under `pdca-reviewer-894-scratch/`). This does not claim independent replay of every bespoke temporary edit listed in the brief.
- Formatting, spelling, docs lint, and the 99-page rendered-site link audit passed. Both named external dependencies, `typos` and the docs renderer, were actually exercised (`brief.md:304`, `reviewer-docs.log:3`). Native repository guards, the statics scanner, 5 valid plus 6 invalid conformance vectors, and core Clippy with all targets also passed (`reviewer-checks.log:7`, `:24`, `:27`, `:173`). `ci-dry-run` executed its repository guards; its printed cargo commands are not counted as executions.

The remaining frozen gate evidence is consistent with those re-runs. The instance-root wrappers are not supplied in this target; their full captured logs were used for the broader checks: C4-ci reaches “all checks passed” (`gate-logs/C4-ci.log:4162`); C4-diff-cov records 311/314 instrumentable changed lines covered, with 551 changed lines unscored (`gate-logs/C4-diff-cov.log:104`); C5-mutants records 44 caught and 14 unviable (`gate-logs/C5-mutants.log:13`). T4-batch-review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`), and host-tikv shows real feature-enabled Clippy compilations finishing for the adapter and server (`gate-logs/host-tikv.log:112`, `:211`). These are frozen-log results, not claims that the entire CI, coverage wrapper, or TiKV compilation was independently repeated here. Every configured gate has readable evidence; none needs escalation for a missing oracle or log.

The prior-art check covered affected paths, not only issue titles. GitHub commit-history queries covered all seven paths, recovered the landed nonce, capacity, and answer-table prerequisites, and found no merged history for the three new files (`reviewer-history-by-path.json:1`). Comparing all 19 closed/unmerged PR file lists against those paths found no overlap (`reviewer-rejected-by-path.json:1`). The brief's rejected-attempt history was also checked against the #508 and #656 issue descriptions (`brief.md:404`, `reviewer-issue-508.json:1`, `reviewer-issue-656.json:1`). Bucket-fence work (#511), configuration reconciliation (#508), and terminal-delete/tombstone work (#659) remain explicit, settled deferrals; no finding is raised against them.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 894 (multipart-admission-create)

**Verdict: I could not refute the fix itself. I could refute the claim that every rule is pinned.**
Every path I drove gave the answer the brief's decision table asks for, and the tests call the real
`create_session` over real stores (redb and the simulated FoundationDB), not a copy of it. But seven
hand-made mutants (small deliberate code changes, used to check that some test notices them) of
`crates/core/src/multipart/create.rs` pass all 31 core legs. None of them is on a path a DST
(deterministic simulation) leg could fail on: no DST leg deletes a bucket, changes the profile,
injects a read fault or repeats an identity. I checked that by reading the scenario; I ran the DST
only against the brief's mutations 19 and 20. Three of the seven break C-1: two strand a session,
and one adopts a session the caller did not create.
For the six mutants that change an answer (the seventh is cosmetic) I wrote a probe test in a
scratch copy. Each probe passed on the patch as shipped and failed on its mutant. All of these are test gaps the builder can close.

## Findings

- NEEDS-HUMAN [impl] — **No leg pins the settling read before `NoSuchBucket` or `ProfileSkew`.** The table's rows 5 and 6 say "after the settling rule", but only the capacity ending is pinned (F5, `crates/core/tests/multipart_create.rs:1990`, brief mutation 16). Mutant: replace `return finish(store, &identity, doubt, Refusal::NoSuchBucket.into()).await` at `crates/core/src/multipart/create.rs:373` with `return Ok(CreateOutcome::Refused(Refusal::NoSuchBucket))`, or make the same change for skew at `create.rs:392`. All 31 core legs still pass. The DST cannot see it either, because no DST leg deletes a bucket or rewrites the profile. **Concrete failing case:** commit 1 is held (`OnCommit::Hold`). During the re-read, after the `mpu:<id>` get has returned absent, the held batch lands and then the bucket marker is deleted (or `mpuctl` is rewritten under another profile). The mutant answers `Refused(NoSuchBucket)` / `Refused(ProfileSkew)` while its own session is stored and counted. That session is stranded, which is exactly the C-1 breach this slice exists to prevent. The shipped code answers `Created` through `finish`'s settling read. **Probe:** add a `Scripted::get` hook beside `on_ledger_read` (`multipart_create.rs:875-886`). On the first `bucket:` read (or `mpuctl` read) while a batch is held, it releases the batch, then deletes the marker (or puts `ledger_bytes(1, &profile_of(9))`). Assert that one session exists and that `created(&outcome) == script.minted[0].0`. The same gap applies to a torn ledger met while in doubt: `create.rs:382` → `Ending::Ledger` → `create.rs:535` is never driven. The diff-coverage gate already lists `create.rs:535` and `create.rs:186` as MISS, and the (B) torn-ledger leg has no doubt in play.

- NEEDS-HUMAN [impl] — **C5's protection is pinned only for the first identity.** Delete `doubt = Doubt::default();` at `create.rs:362` and every leg still passes. In that mutant, a re-minted identity inherits `doubt.unknown = true` from the identity it replaced. Its first read then adopts a byte-identical record that existed before, handing the caller a session it did not create (the case C5, `multipart_create.rs:1674`, exists to rule out). **Concrete case:** plant identity 5's R with a real create for the same request. Use plan `[RivalFirst { may_still_commit: false }]` and script `[identity(1), identity(5), identity(6)]`. The shipped code ends `IdentityCollisions`. The mutant answers `Created(identity(5))`, the planted session.

- NEEDS-HUMAN [impl] — **The "found something else and not held, so the ending stands" half of the settling rule is unpinned for a 1021 (`may_still_commit: false`), at both places that clear it:** the loop's re-read (`create.rs:343`) and `finish`'s read (`create.rs:564`). Removing either line passes every leg. The mutants end `InDoubt { may_still_commit: false }`, which classifies `Indeterminate` ("do not retry"), after a read has already shown the batch absent. The table says the plain ending stands: `CreateError::Store`, and `Transient` for a `TransientFault`. F9 (`multipart_create.rs:2281`) pins "nothing in doubt → `Transient`" only before any commit. **Cases:** (X1, for `:564`) plan `[Unknown { applied: false, may_still_commit: false }]` with `GetFault { after_commits: 1, sessions_only: false, skip: 0, fail: 1, transient: true }`; expect `Store` and `Transient`. (X2, for `:343`) plan `[Unknown { applied: false, may_still_commit: false }, Fail]` with `GetFault { after_commits: 2, sessions_only: true, skip: 0, fail: usize::MAX, transient: false }`; expect `Store`. Lower severity: these mutants over-report doubt but never hide a session.

- NEEDS-HUMAN [impl] — **The settling read in the held, identity-taken branch (`create.rs:349`) is unpinned.** Replacing it with `if false` passes every leg. It matters when the held batch lands between the `mpu:<id>` get and the `seggrp:<nonce>` get (`create.rs:596-597`). The group read then finds the create's own marker. Shipped: `Created`. Mutant: `InDoubt(IdentityTaken)` for a session that is its own. **Probe:** a hook that releases the held batch on the first `seggrp:` read. Lower severity: the answer is cautious, and the id is still named.

- NEEDS-HUMAN [impl] — **The rustdoc does not say the future is unsafe to cancel** (`create.rs:296-304`, "Termination"). Suppose a caller drops `create_session` after a commit was sent, for example by wrapping it in a request timeout, which the rubric's await-discipline rule invites #668 to do. Then a batch can land with no `Created` or `InDoubt` naming it. That is 0016's own failure case (`0016:2015-2018`, "erroring out … can leave an `Open` session no client will ever name"). The module's C-1 promise (`create.rs:15-19`) and the doc paragraph's "no session it installs goes unreported" then no longer hold. The store contract already bounds each call (`crates/traits/src/lib.rs:1334-1335`), and a backend's own timeout comes back as `CommitUnknownResult`. So the fix is one paragraph: bound the create through the store's own deadlines, never by dropping the future. Wiring the caller stays #668's.

## Notes (no action needed)

- The `check-gates.json` C5-mutants row ("58 mutants tested … 44 caught, 14 unviable") reads as "every rule pinned". It is not: `cargo mutants` does not delete statements or flip conditions, and every survivor above came from that kind of edit. Do not read the C5 pass as covering the decision table.
- `create.rs:505`: changing `else if self.held.is_none()` to `else` passes every leg. It only changes which held `CommitUnknownResult` `InDoubt.outcome` reports (the latest instead of the first, both `may_still_commit: true`), which contradicts the rustdoc at `create.rs:130-131`. Cosmetic.
- **Attempted to refute, and could not:**
  - (1) *The evidence.* The legs drive the production function over `RedbMetadataStore::in_memory()` and `SimFdbMetadataStore`. The size legs build each widest state from their own spelling and decode it with the production decoder (`multipart_create.rs:1257-1302`), rather than reusing `widest_reachable_bytes`.
  - (2) *The brief's mutation table.* I applied rows 4, 5, 6, 10, 11, 12 (a held doubt cleared by a later 1021), 19 and 20 to the production code. Each went red in its named leg: C3, C4, F3, C6 (i)/(ii), C5, F6, and the DST reverse and stranded-record assertions (madsim, `MADSIM_TEST_NUM=50`).
  - (3) *Races and edges.* These all held up: a held batch landing during a re-drive (stopped by `require_absent`), the race to create the first ledger, the ledger returning to its old bytes (F3), and the widest-state measurement (it respects the decoder's bounds on `segments_written`, `MAX_PART_NUMBER` and epoch agreement). The contention budget is never 0, because rule G7 forces `max_sessions ≥ 1`.
- C4-verify UNVERIFIABLE is the pre-declared net-new case, not a refutation. On my re-run, all 31 core tests and all 7 DST tests passed. My scratch copy is removed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No findings on either requested lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in the diff. Reviewed admission and recovery (`crates/core/src/multipart/create.rs:305`), encoded-size headroom (`crates/core/src/multipart.rs:2388`), deterministic regression tests (`crates/core/tests/multipart_create.rs:443`), and seeded simulation assertions (`crates/dst/tests/multipart_admission.rs:350`).

Validation evidence was read from the frozen gate logs; tests were not rerun. The pre-fix compilation limitation is explicitly anticipated by the brief.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept green tests and mutation evidence without an executable pre-fix discriminator for this new API — the independently reproduced E0432 is not a test failure, while restored core and seeded simulation runs pass; `brief.md:311`, `reviewer-core-red.log:2`, `reviewer-checks.log:212`, `reviewer-dst.log:249`.
- [x] Validation — fitness-to-purpose — Confirm that the tested core admission behavior fulfills #894's intended purpose — this slice exposes no production Create caller, so it establishes protocol behavior rather than an end-user S3 upload workflow; `brief.md:360`, `crates/core/src/multipart/create.rs:21`, `docs/design/architecture/05-building-block-view.md:206`.
- [ ] **No leg pins the settling read before `NoSuchBucket` or `ProfileSkew`.** The table's rows 5 and 6 say "after the settling rule", but only the capacity ending is pinned (F5, `crates/core/tests/multipart_create.rs:1990`, brief mutation 16). Mutant: replace `return finish(store, &identity, doubt, Refusal::NoSuchBucket.into()).await` at `crates/core/src/multipart/create.rs:373` with `return Ok(CreateOutcome::Refused(Refusal::NoSuchBucket))`, or make the same change for skew at `create.rs:392`. All 31 core legs still pass. The DST cannot see it either, because no DST leg deletes a bucket or rewrites the profile. **Concrete failing case:** commit 1 is held (`OnCommit::Hold`). During the re-read, after the `mpu:<id>` get has returned absent, the held batch lands and then the bucket marker is deleted (or `mpuctl` is rewritten under another profile). The mutant answers `Refused(NoSuchBucket)` / `Refused(ProfileSkew)` while its own session is stored and counted. That session is stranded, which is exactly the C-1 breach this slice exists to prevent. The shipped code answers `Created` through `finish`'s settling read. **Probe:** add a `Scripted::get` hook beside `on_ledger_read` (`multipart_create.rs:875-886`). On the first `bucket:` read (or `mpuctl` read) while a batch is held, it releases the batch, then deletes the marker (or puts `ledger_bytes(1, &profile_of(9))`). Assert that one session exists and that `created(&outcome) == script.minted[0].0`. The same gap applies to a torn ledger met while in doubt: `create.rs:382` → `Ending::Ledger` → `create.rs:535` is never driven. The diff-coverage gate already lists `create.rs:535` and `create.rs:186` as MISS, and the (B) torn-ledger leg has no doubt in play.
- [ ] **C5's protection is pinned only for the first identity.** Delete `doubt = Doubt::default();` at `create.rs:362` and every leg still passes. In that mutant, a re-minted identity inherits `doubt.unknown = true` from the identity it replaced. Its first read then adopts a byte-identical record that existed before, handing the caller a session it did not create (the case C5, `multipart_create.rs:1674`, exists to rule out). **Concrete case:** plant identity 5's R with a real create for the same request. Use plan `[RivalFirst { may_still_commit: false }]` and script `[identity(1), identity(5), identity(6)]`. The shipped code ends `IdentityCollisions`. The mutant answers `Created(identity(5))`, the planted session.
- [ ] **The "found something else and not held, so the ending stands" half of the settling rule is unpinned for a 1021 (`may_still_commit: false`), at both places that clear it:** the loop's re-read (`create.rs:343`) and `finish`'s read (`create.rs:564`). Removing either line passes every leg. The mutants end `InDoubt { may_still_commit: false }`, which classifies `Indeterminate` ("do not retry"), after a read has already shown the batch absent. The table says the plain ending stands: `CreateError::Store`, and `Transient` for a `TransientFault`. F9 (`multipart_create.rs:2281`) pins "nothing in doubt → `Transient`" only before any commit. **Cases:** (X1, for `:564`) plan `[Unknown { applied: false, may_still_commit: false }]` with `GetFault { after_commits: 1, sessions_only: false, skip: 0, fail: 1, transient: true }`; expect `Store` and `Transient`. (X2, for `:343`) plan `[Unknown { applied: false, may_still_commit: false }, Fail]` with `GetFault { after_commits: 2, sessions_only: true, skip: 0, fail: usize::MAX, transient: false }`; expect `Store`. Lower severity: these mutants over-report doubt but never hide a session.
- [ ] **The settling read in the held, identity-taken branch (`create.rs:349`) is unpinned.** Replacing it with `if false` passes every leg. It matters when the held batch lands between the `mpu:<id>` get and the `seggrp:<nonce>` get (`create.rs:596-597`). The group read then finds the create's own marker. Shipped: `Created`. Mutant: `InDoubt(IdentityTaken)` for a session that is its own. **Probe:** a hook that releases the held batch on the first `seggrp:` read. Lower severity: the answer is cautious, and the id is still named.
- [ ] **The rustdoc does not say the future is unsafe to cancel** (`create.rs:296-304`, "Termination"). Suppose a caller drops `create_session` after a commit was sent, for example by wrapping it in a request timeout, which the rubric's await-discipline rule invites #668 to do. Then a batch can land with no `Created` or `InDoubt` naming it. That is 0016's own failure case (`0016:2015-2018`, "erroring out … can leave an `Open` session no client will ever name"). The module's C-1 promise (`create.rs:15-19`) and the doc paragraph's "no session it installs goes unreported" then no longer hold. The store contract already bounds each call (`crates/traits/src/lib.rs:1334-1335`), and a backend's own timeout comes back as `CommitUnknownResult`. So the fix is one paragraph: bound the create through the store's own deadlines, never by dropping the future. Wiring the caller stays #668's.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The contention guarantee contradicts the prescribed retry behavior.** `brief.md:205` promises that a refusal never comes from ordinary concurrency while the ledger has room; `brief.md:496` requires publishing that guarantee. Yet row 7 (`brief.md:408`) spends contention budget even when every precondition still holds, and row 10 refuses when that budget expires. This is a real backend behavior: `crates/metadata-tikv/src/lib.rs:1386` routes locking-read failures through `conflict_or_err`, which returns known-unapplied `Conflict` at `:934` without requiring ledger progress. Repeated lock conflicts can therefore exhaust the budget without filling or even changing `mpuctl`, and without an unknown outcome. Narrow the guarantee to schedules where each loss represents another admission; explicitly document and test exhaustion from lock conflicts with unchanged records. This does not require reopening the settled budget decision.
- [x] **The size mutation requirement has both a missing discriminator and an equivalent mutation.** `brief.md:73` requires only one-byte-under and one-byte-over fixtures, but mutation 18 (`brief.md:306`) requires `>` → `>=` to fail: those fixtures produce identical answers under both comparisons. Add an exactly-at-ceiling admission case; the target's ceiling contract explicitly admits equality (`crates/core/src/metadata.rs:602`). Also, “one state dropped” cannot universally imply a failure: `Aborting {}` has no payload (`crates/core/src/multipart.rs:2132`), whereas `Completing` and `Completed` have longer tags plus required payloads (`:2122`, `:2136`). With identical common fields at their widest values, dropping Aborting cannot change the maximum. Specify non-equivalent omissions, with fixtures where Completing and Completed respectively determine the maximum, instead of treating every surviving omission as missing coverage (`brief.md:285`).
- [x] **F7's assertions do not establish its advertised fresh-ledger recovery.** `brief.md:134` requires eventual success under the original identity and count `c + 2`; mutation 9 (`brief.md:297`) is supposed to fail F7. A recovery that replays the original batch once, receives `Conflict`, then follows row 7's normal fresh-read retry (`brief.md:408`) satisfies every stated F7 assertion when budget remains. Moreover, placing the competing create merely “before the recovery's commit” can invalidate even a correctly refreshed batch. The target proposal requires rebuilding from a fresh read (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2019`). Place the competing admission before the recovery ledger read and require the wrapper to inspect the first recovery batch's `mpuctl` precondition and increment. That distinguishes the promised behavior from a stale replay followed by eventual success.
- [x] size backstop — this slice is behaving oversized: patch is 192 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Code is correct; the gaps are tests and one doc paragraph. Close the adversary's four unpinned mutants with its probe tests: (1) settling read before NoSuchBucket/ProfileSkew (create.rs:373, :392) plus the torn ledger met while in doubt (create.rs:382 -> :535); (2) doubt reset on re-mint (create.rs:362), the C5 case on a re-minted identity; (3) "found something else and not held, the ending stands" after a 1021 (create.rs:343 and :564; cases X1, X2); (4) settling read in the held identity-taken branch (create.rs:349; release the held batch on the first seggrp: read). Add a cancel-safety paragraph to create_session's rustdoc "Termination" section: do not drop the future after a commit is sent; bound it through the store's own deadlines. No other changes; size already accepted.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
