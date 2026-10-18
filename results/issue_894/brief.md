# Brief — issue 894 / multipart-admission-create

> The Plan artifact for slice 3a of #636, re-planned 2026-10-17 after attempt 1 (iterated to
> Plan). Do reads ONLY this file, plus the inputs its `Citations expected` field names
> (attempt 1's `patch.diff` and two peer files). The `- **Label:** value` lines are parsed by the driver;
> keep their shape. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: the **Create session** batch
> row (`0016:657`) and the unknown-outcome recovery paragraph (`0016:2010-2028`). Citations
> verified against `getwyrd/wyrd` `origin/main` @ `71e9b9f` on 2026-10-17.
>
> **Why this re-plan exists.** Attempt 1 was nearly right. Its reviewers found one real defect,
> and the old brief caused it: the brief told Do that the record found under an upload id is
> "not evidence" of ownership, so recovery never compared it. They also found five rules the
> brief named that no test pinned. This brief removes every correctness choice from Do. The
> recovery rules are a decision table (Design § *Every answer, decided*), and every rule has a
> named mutation that must make a named leg fail (Verification posture). Do starts from attempt
> 1's patch and changes what this brief lists. The human chose one slice, not a split, on
> 2026-10-17. The patch will cross the 100 KB size flag again, and the human accepts that.

- **Slug:** multipart-admission-create
- **Track:** alpha
- **Goal:** Nothing on `main` creates a multipart session. The module is deliberately pure
  (`crates/core/src/multipart.rs:5-7`): no `MetadataStore` call and no `WriteBatch`. This slice
  adds `CreateMultipartUpload`'s admission through the serialized fleet ledger `mpuctl`, plus the
  session's creation, as a `wyrd-core` function over `MetadataStore`. It answers in the
  vocabulary already on `main`: `CreateOutcome` (`:4380`), `Refusal` (`:4302`) and
  `Backpressure` (`:4277`). Abort is the sibling slice #895 (636.3b) and is not built here.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_create.rs`, over
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
- **Falsifiability:** Every core leg goes RED in-process on the plain `cargo test -p wyrd-core`
  target, on every run, independent of the scheduler. `RedbMetadataStore` checks every
  `require` / `require_absent` inside one serialized write transaction and answers
  `CommitOutcome::Conflict` (`crates/metadata-redb/src/lib.rs:212-222`). The test wrappers force
  the losing order (A′), the lock-style `Conflict` with nothing moved (B), the interloper
  (C3-C6), the unknown outcome and its fate (F), and the plain faults (F8). So each mutation in `Verification posture` fails its named leg
  deterministically. (G) is madsim-only (`#![cfg(madsim)]`). `cargo xtask ci` runs it with
  `--cfg madsim` and `MADSIM_TEST_NUM=50` (`xtask/src/main.rs:1742-1745`, `run_dst`
  `:1781-1819`, `DST_SEEDS` `:1776`), and since #904 ci runs every step even after a failing
  one. Its reds come from the sim's seed coins, and the witness assertions above turn a sweep
  that misses a branch into a failure. No special topology is needed.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, `:109`, and its §6 row
  *storage lifecycle / reclamation*, `:137`: this slice gives the session state machine its first
  client-driven writer). Every `mpu:` session and `seggrp:` marker a Create installs is counted
  by `mpuctl` **in the same atomic batch**. Every session a Create installs is one its caller is
  told about, by `Created` for that very session or by an `InDoubt` naming its identity. So no
  admitted session is outside the reach of the actor that reclaims it (#659's drain, #625's
  reaper), and no caller is handed a session it did not create. The admission count changes
  only on admission (+1) and on the terminal delete (−1, #659). A contention refusal is bounded,
  not ruled out: it means the create lost or could not settle `max_sessions` commits. On a store
  whose `Conflict` means a precondition key was written by another committed batch (redb,
  `crates/metadata-redb/src/lib.rs:212-222`; FDB, `crates/metadata-fdb/src/lib.rs:14-29`), and
  with no unknown outcomes, each loss is the ledger moving, so a create on a ledger with room
  is refused only after the ledger moved that many times. On TiKV a locking read that meets
  another writer's lock also answers `Conflict` with nothing moved
  (`crates/metadata-tikv/src/lib.rs:1386-1393`, `:928-939`), so lock contention alone can spend
  the budget on a ledger with room. That case is stated and tested (B), not promised away.
  **Sources:** the maintainer's rule 2026-07-25
  (authoritative, via principles.md §5); 0016's rule that no refutation-standard outcome (a
  stranded record first among them) is an accepted cost (`0016:2819-2823`); the batch row
  `0016:657`; the recovery paragraph `0016:2010-2028`; the admission-counter rationale
  `0016:245-254`. SELF-TEST: this relates a batch, a counter and what the caller is told, so
  guarding any one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 768
- **Ordering note:** Wave before #895 (the Abort slice), which declares `Depends on: 894`. The
  prerequisites #654 (record family) and #655 (knobs) are closed and on `main`.
  `Conflicts with 768`: both edit `docs/design/architecture/05-building-block-view.md`. This
  slice edits the multipart paragraph at `:206`, which is one 4,929-character line, so the
  whole line is the hunk. #768 edits the object-metadata text just above it, at `:189-204` on
  `71e9b9f` (its brief cites `:197-200` on an older base), so the two hunks are adjacent.
  #768's brief declares the same conflict. **Intake cap (wyrd-pdca-P1):**
  this re-plan repairs an ITERATE_PLAN brief, which is exempt, and adds no new intake.
  `scripts/plan-cap --track alpha` on 2026-10-17 printed 7/6, room 0. This bundle entered over
  the cap with #656's split, by the human's override in the 2026-10-15 Plan session.
- **Surfaces:** data
- **Scope:** `CreateMultipartUpload`'s admission and session creation as a store-I/O function in
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
- **Repro instruction:** n/a (new functionality). Today
  `rg -n 'async fn|WriteBatch::new|\.commit\(' crates/core/src/multipart.rs` finds only the
  header's own sentence saying there is none (`:7`), and `crates/core/src/multipart/` does not
  exist. Nothing creates a session.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_create.rs`, a **NEW** file. The DST scenario
  `crates/dst/tests/multipart_admission.rs` is also NEW. `run-verify.sh --classify` on
  attempt 1's patch lists both as `ADDED_TEST` discriminators (crates `crates/core`,
  `crates/dst`). The only existing test attempt 1 touched,
  `crates/core/tests/multipart_state_machine.rs`, needs only the new `Refusal::SessionTooLarge`
  arm in its exhaustive match (`:766-772`).
- **Verification posture:** (a) net-new, so C4-verify is **UNVERIFIABLE** (exit 77), as it was
  for attempt 1. `create_session` does not exist on `main`, so the discriminating test cannot
  compile on the reverted base. That is a pre-declared sign-off item, not a defect.
  **Built and exercised at Check:** the Create function itself, driven by both new tests against
  real stores with real preconditions, real contention and real (simulated) ambiguity. C4-ci
  runs everything: the deny waiver landed in #903 and #904 makes ci run every step, so the
  2026-10-15 "inherited deny failure" ruling no longer applies.
  **The size flag is pre-answered.** The patch will be over the 100 KB size backstop again
  (attempt 1 was 148 KB, and this brief adds legs). The human accepted that at Plan on
  2026-10-17, so the §6 size item is expected. Do must not drop or merge legs to get under it.
  **The demonstrated reds are the convergence contract.** Do makes each temporary edit below,
  runs the tests, records the failing test's name in `build-notes.md`, then restores the file.
  Every edit must fail its named leg. An edit that fails nothing means a test is missing, and
  the patch is not done.
  | # | Temporary edit to the patched code | Must fail |
  |---|---|---|
  | 1 | Cap the contention budget at `MAX_UPLOAD_ID_ATTEMPTS` | A′ |
  | 2 | Use the shipped `MAX_SESSIONS` / `MAX_ADMISSION_CAS_ATTEMPTS` instead of the profile's `max_sessions` | A′ (second run) |
  | 3 | Count collisions and lost CASes in one counter | C2 |
  | 4 | Drop `require_absent(mpu:<id>)` from the batch | C3 |
  | 5 | Drop `require_absent(seggrp:<nonce>)` from the batch | C4 |
  | 6 | Drop both identity preconditions | F3 |
  | 7 | Drop the bucket `require` | B (marker moved under the commit) |
  | 8 | Mint a new identity after an unknown outcome | F1, F2 |
  | 9 | Re-drive by replaying the original batch | F7 |
  | 10 | Answer `Created` for any record found under the id after an unknown outcome (attempt 1's code) | C6 |
  | 11 | Answer `Created` on a found R with no absence read before the first commit | C5 |
  | 12 | Replace the kept doubt with the latest outcome | F6 |
  | 13 | Reset the doubt on a read fault / a plain commit fault / a failed settling read (three edits) | F8 (a), (b), (c) |
  | 14 | End with `CreateError::Store` after a 1021 whose settling read failed | F8 (d) |
  | 15 | Skip the settling read on the last budgeted attempt | F4 |
  | 16 | Skip the own-record read before a capacity refusal | F5 |
  | 17 | Make `InDoubt`'s `source()` chain end at the stop cause (attempt 1's code) | F9 |
  | 18a | `>` → `>=` in the size check | B (size headroom, the exactly-at-ceiling point) |
  | 18b | Drop `Completing` from the widest measurement | B (size headroom, Completing-widest fixture, one byte over) |
  | 18c | Drop `Completed` from the widest measurement | B (size headroom, Completed-widest fixture, one byte over) |
  | 19 | (G) answer `Created` for a 1021 without a settling read | G reverse assertion |
  | 20 | (G) name a fresh identity, not the doubtful one, in `InDoubt` | G stranded-record check on the witness leg |
  | 21 | Treat a `Conflict` whose re-read finds every precondition holding as a collision (mint a new identity) | B (contention spent with nothing moved) |
  | 22 | Retry a `Conflict` whose re-read finds every precondition holding without spending a contention unit | B (contention spent with nothing moved: the leg's timeout fires) |
  A reviewer can repeat any row. Do also runs `cargo xtask ci` and records its result.
  **Known equivalent mutation, not a missing test:** dropping `Open` or `Aborting` from the
  widest measurement changes no answer. Both carry the same common fields as the other states,
  no payload, and a shorter tag, so neither is ever the widest. Do does not need a red for it.
  If `cargo mutants` reports it surviving, `build-notes.md` names it as this equivalent.
  C5-mutants (`cargo mutants --in-diff`) is advisory. Attempt 1's three survivors (two
  `Display` impls and `Error::source`) are now pinned by F9.
  The test-local wrappers keep their state in struct fields, never in a `static`. The ADR-0035
  statics scan covers `crates/core` and `crates/dst` (`xtask/src/main.rs:1221-1242`).
- **Production reach:**
  - (a) The tests exercise the function for real: real stores, real preconditions, real
    contention. Production calls nothing in this slice.
  - (b) The S3 verb that calls it lands with #668. That slice also carries 0016's normative rule
    that the reaper (#625) and operator abort ship in the same release as any verb that creates
    sessions (`0016:236-268`).
  - (c) So until #668, no deployment can create a session. This slice **must not** add a
    production caller: no gateway, CLI or custodian wiring.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Starting point (a named input, like a peer callsite):** attempt 1's patch,
    `iteration-v1/patch.diff` in this bundle. It applies cleanly to `main` @ `71e9b9f`
    (`git apply --check`, 2026-10-17). Do may read that file. Do does not read the rest of
    `iteration-v1/`. Design § *Attempt 1: keep and change* lists what to change in it.
  - **Peer callsite for store-I/O shape:** `crates/core/src/write.rs:266` (`commit_create`)
    takes the store as `&impl MetadataStore` and builds one `WriteBatch` with preconditions.
  - **Peer for the DST scenario:** `crates/dst/tests/commit_ambiguity.rs` (its 1021 and 1031
    legs, the `#![cfg(madsim)]` root at `:67`, and the sweep helpers at `:91`, `:335`).
  - **Symbols:** `MPUCTL_KEY` (`multipart.rs:1132`), `MPU_PREFIX` (`:1147`), `mpu_key`
    (`:1227`), `Budget` (`:1567`, `max_sessions` `:1683`), `AdmissionRecord` (`:1840`, `count`
    `:1848`, `max_sessions` `:1853`, `profile` `:1858`), `decode_admission_record` (`:1914`),
    `SessionRecord` (`:2265`, no free constructor `:2259-2263`, `open_teardown` `:2360`),
    `decode_session_record` (`:2504`), `UploadId` (`:838-848`), `MAX_SESSIONS` (`:4969`),
    `MAX_UPLOAD_ID_ATTEMPTS` (`:5009`), `MAX_ADMISSION_CAS_ATTEMPTS` (`:5023`),
    `admission_backoff_millis` (`:5049`), `KnobSet` (`:5069`).
  - **Bucket and sizes:** bucket existence is the `bucket:` record at `metadata::bucket_key`
    (`crates/core/src/metadata.rs:52`). The reservation marker is `metadata::seggrp_key`
    (`:1521`). The value ceiling is `MAX_VALUE_BYTES` (`:549`), and
    `flat_value_ceiling_crossed` (`:605`) measures it.
  - **Seam:** `CommitUnknownResult` (`crates/traits/src/lib.rs:222-248`, `may_still_commit`
    `:240-247`), the one-downcast rule (`:1306-1312`), `classify` (`:786`), and
    `ErrorClass::Indeterminate` (`:699`).
- **Prior-art check (triage cycles):** searched by path `crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/tests/multipart_create.rs`,
  `crates/dst/tests/multipart_admission.rs`, `crates/dst/tests/support/mod.rs` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main` @ `71e9b9f`.
  - **Merged:** `2f41e73` (#847, the segment-group nonce on every session), `4533549` (#655, the
    capacity knobs and the two split budgets), and `14911ec` (the answer table, #654 3/3).
  - **Merged since the last plan:** `1a7eff0` (#671, the single-PUT ceiling, which moved
    `multipart.rs` lines after `:4754` by +26 and 0016 lines after `:1056` by +1 and after
    `:2297` by +8; every citation here is re-pinned); `b879e7b` (#666, the `MultipartGateway`
    seam answering `NotImplemented`); `c676136` (#767, the doc paragraph above this slice's);
    `0674f4d` (#903, the RUSTSEC-2026-0346/0349 waiver); and `016f98e` (#904, ci runs every
    step).
  - **Neither new file and no store module exists on `main`.**
  - **Rejected:** #508's seven monolithic attempts (the false-`503` measurement came from one of
    them); #636, closed not-planned 2026-10-10 as too large; #656's four attempts at Create and
    Abort together; and this bundle's attempt 1 (iterated to Plan 2026-10-14, see Design).
  - **Open PRs** on getwyrd/wyrd: none (`gh pr list --state open`, 2026-10-17).
- **Disposition hint:** new-feature

## Design

**Where.** The store-I/O module `crates/core/src/multipart/create.rs` (as in attempt 1), beside
the pure module. Core stays runtime-free (ADR-0009): the backoff wait, the clock and its
`clock_source`, and the randomness for ids, nonces and jitter all come from the caller (attempt
1's `CreateRequest` and `CreateRuntime`). Core runs the re-drive itself, using the caller's wait
as it does for backoff, so the gateway (#668) never rebuilds a batch.

**The Create batch (`0016:657`)**, one per attempt, built from that attempt's fresh reads:
- **Ledger present:** `require(mpuctl == prior bytes)`, admitted only while
  `prior.count < prior.max_sessions` (the **stored** limit), and only when the local `Budget`
  equals `prior.profile` (`ProfileSkew` otherwise).
- **Ledger absent:** `require_absent(mpuctl)` and a bootstrap `{count: 1, max_sessions,
  profile}` from the local `Budget`.
- **Either way:** `require_absent(mpu:<id>)`, `require_absent(seggrp:<nonce>)`, and
  `require(bucket:<name> == the marker bytes this create first read)` (ADR-0046 §4,
  `docs/design/adr/0046-bucket-model-real-namespace.md:93-95`). Then put R at `mpu:<id>`, put the
  marker at `seggrp:<nonce>`, and put the ledger at `count + 1`.
- **R is built once per identity.** Every batch under an identity puts exactly R: the clock
  stamp is taken once, never re-stamped on a re-drive.
- **The deletion-fence half of ADR-0046 §4 is deferred to #511, stated, not dropped.** §4 also
  asks for `require_absent` on the bucket's deletion fence (decision 5, `:104-121`). No fence
  key exists on `main`, and nothing in `crates/` defines or writes one. This slice must not
  invent it. Keep the bucket precondition in one marked place (attempt 1's `require_bucket`), so
  that #511 adds the fence there.

### Every answer, decided

Do implements exactly this table. Nothing here is Do's choice. Three terms:
- **Unknown:** a commit of this create under the current identity returned
  `CommitUnknownResult`.
- **Held:** one of those returned `may_still_commit: true`, and no read has since found R. Only
  finding R clears it.
- **Unsettled:** a `may_still_commit: false` outcome that no successful read of `mpu:<id>` has
  followed yet.

| Row | The create meets… | It answers |
|---|---|---|
| 1 | its **first** read for an identity finds `mpu:<id>` or `seggrp:<nonce>` present | collision: mint a new identity (collision budget), whatever the record holds |
| 2 | after an unknown outcome, a read finds `mpu:<id>` **exactly equal to R** | `Created` for this identity |
| 3 | after an unknown outcome, a read finds `mpu:<id>` present but **not R**, or `mpu:<id>` absent and `seggrp:<nonce>` present | if **held**: end `InDoubt { may_still_commit: true }` naming this identity. **No new identity is ever minted while held.** If not held: collision, mint a new identity (collision budget) |
| 4 | a `Conflict` with no unknown outcome under this identity, and `mpu:<id>` or `seggrp:<nonce>` present | collision, as row 1 (a `Conflict` wrote nothing, so the record is another's) |
| 5 | `bucket:<name>` absent, or not the bytes first read | `Refused(NoSuchBucket)`, after the settling rule |
| 6 | `mpuctl`'s profile differs from the local `Budget`, or `mpuctl` is at its stored `max_sessions` | `Refused(ProfileSkew)` / `Refused(SlowDown { SessionCap })`, after the settling rule |
| 7 | a `Conflict` where only `mpuctl` moved, **or where every precondition still holds** on the re-read (TiKV's adapter answers `Conflict` without applying when a locking read meets another writer's lock, `crates/metadata-tikv/src/lib.rs:1386-1393`, `:928-939`) | contention: spend one unit, wait `admission_backoff_millis(k, jitter)`, re-read, retry the same identity with every precondition rebuilt |
| 8 | `CommitUnknownResult` | record it (held if `true`, unsettled if `false`), spend one contention unit, back off, re-read. Then row 2 or 3 applies; otherwise retry the **same** identity with every precondition rebuilt from the re-read, the `mpuctl` CAS included (`0016:2018-2025`) |
| 9 | a plain store error on a read or a commit (not `CommitUnknownResult`), or a stored `mpuctl` that does not decode | after the settling rule, `CreateError::Store` / `CreateError::Ledger` |
| 10 | the contention budget spent | `Refused(SlowDown { AdmissionContention })`, after the settling rule |
| 11 | the collision budget spent (reachable only when not held, by rows 1, 3 and 4) | `CreateError::IdentityCollisions`, writing nothing |
| 12 | the widest reachable encoding of R over `MAX_VALUE_BYTES` | the size refusal, before any read or commit |

**Order.** On every read the create checks the identity's keys first (rows 1-4), then the bucket
(row 5), then the ledger (row 6, and row 9 when it does not decode), and only then treats a lost
commit as contention (row 7). This is attempt 1's order.

**The settling rule** applies before any ending other than `Created` while the create is held or
unsettled. It reads `mpu:<id>` once. If the read finds R, the answer is `Created`. If it finds
anything else and the create is not held, the ending stands. Otherwise the create ends
`InDoubt` naming this identity, and `may_still_commit` is `true` exactly when it is held. So a
failed settling read, or a held create, always ends `InDoubt`.

**Why row 2 compares bytes and row 1 reads first.** Neither check is sufficient alone:
- Attempt 1 answered `Created` for any record found after an unknown outcome. A different create
  that took the same upload id with another nonce and object got handed over (the review's
  counterexample, now C6).
- Byte equality alone adopts a byte-identical record that was there before this create began.
  `SessionRecord` carries no per-invocation token (`multipart.rs:2265-2279`), so two
  invocations with the same identity and request write the same bytes (C5).

Together, the absence read (row 1) and the byte compare (row 2) leave one case outside the
guarantee: two **live** invocations handed the **same** upload id **and** nonce **and** request
bytes. Both read the identity absent, one batch lands, and the other's re-read finds bytes it
could have written. Ids and nonces are 128-bit random tokens from the caller's randomness
(`multipart.rs:814-848`), so the chance is negligible in production. That is a stated bound,
and the rustdoc says so. It is not tested as a guarantee.

**Why row 3 ends `InDoubt` rather than re-minting while held.** A held batch can still land.
A re-mint would then leave a counted session that nothing named, which is `0016:2015-2018`'s
case. The `InDoubt` names this create's own identity. Its rustdoc says the id is a report that
a batch under it may land, not a usable session. What the client sees is #668's to decide.

### The error contract (F9)

`CreateError::InDoubt` carries the upload id, the nonce, `may_still_commit`, and what stopped the
create. `wyrd_traits::classify` must return `Indeterminate` for it, whatever stopped it. That
class is the seam's "must not be retried, re-read instead" signal (`crates/traits/src/lib.rs:676-680`,
`:695-699`). Its `source()` chain must therefore reach a `CommitUnknownResult` before any other
classifiable error. Attempt 1 returned the stop cause as `source()`, so an in-doubt create
with a transient cause classified `Transient`, and a generic retry would have re-run it under a
fresh identity. Do chooses how the chain is built, and F9 pins the result. Every other variant
keeps its cause's class.

### Decided, and not to be re-raised (human, 2026-10-17)

- **Unknown outcomes spend the contention budget** (rows 8 and 10). That budget is the profile's
  `max_sessions`: 46 for the shipped profile, which is at most about 2.3 s of backoff, and it
  grows linearly with a larger profile. No separate cap is added. The function's rustdoc
  states the bound.
- **`KnobSet::max_upload_id_attempts` and `KnobSet::max_admission_cas_attempts` are not read
  by Create.** Create takes `MAX_UPLOAD_ID_ATTEMPTS` and the profile's `max_sessions`.
  Reconciling the configuration-side set with what Create reads is #508's, which wires
  configuration. This slice adds the one doc line per field saying so, and nothing else in
  `KnobSet` or `knob_clamps_hold` changes.
- **The bucket deletion fence** is #511's (above).
- **A held batch landing after #659 has terminal-deleted its identity's record** is outside
  this slice. The tombstone window that rules it out is #659's (0016 X105, `:2644`). F3 shows
  that the exclusion does not rest on the ledger.

### The creation routes

`SessionRecord` has no free constructor on purpose (`crates/core/src/multipart.rs:2259-2263`),
and `AdmissionRecord` has none either. Add the **one** route for each that holds the identity the
decoders enforce: `encode(decode(bytes)) == bytes`, canonical field order, and `content_type`
omitted when absent. That covers the ledger's first and incremented values. The initial epoch,
attempt count and `clock_source` follow 0016 §1's `mpu:` row. Do not loosen a decoder to fit.
If a route needs that, say so in `build-notes.md` and stop. Attempt 1's `SessionRecord::open`,
`AdmissionRecord::bootstrap` and `AdmissionRecord::admit_one` meet this rule. Keep them.

### Size headroom

A session admitted at `Open` must be able to reach every later state. In #656's attempt 2, an
`Open` record admitted at the ceiling became 4 bytes larger as `Aborting@1`, so every fence of
it failed forever and its slot leaked. Measure the widest encoding of every state the session
can reach, with the real encoder (so JSON escapes count), against `MAX_VALUE_BYTES`, and refuse
before any read or commit. The answer is attempt 1's `Refusal::SessionTooLarge { bytes, limit }`.

### Docs currency

Docs currency is a merge requirement (`AGENTS.md:154-157`). The paragraph at
`docs/design/architecture/05-building-block-view.md:206` says "Nothing else writes or consumes
them in production yet — the rest of the protocol … arrives with the store round trip
(#656–#659)". Replace only that sentence. Say that Create now writes `mpuctl`, `mpu:` and
`seggrp:`, with no production caller until #668. Also say what a contention refusal means, in
the narrowed form of the Invariant field: the create lost or could not settle its commit as
many times as the profile admits sessions. Each loss is the ledger moving, an outcome the
store could not report, or (on TiKV) a lock held by another writer with nothing moved. Do not
write "never ordinary concurrency on a ledger with room": TiKV's lock conflicts make that
false. The function's rustdoc says the same. Attempt 1's rustdoc (`create.rs`, its "Two
budgets, never shared" section) and its doc paragraph both carry the old sentence. Replace
it in both.

### Attempt 1: keep and change

Start from `iteration-v1/patch.diff` (Citations). Its structure, names, test wrappers
(`Barrier`, `Scripted`, `Ceiling`) and DST `Creator` held up under three reviews. The adversary
reproduced every red it was asked for and found no wrong answer on a tested path. Keep all of
that. Change exactly this (attempt-1 line numbers, in its `create.rs` and tests):
1. **Rows 2 and 3:** compare the found `mpu:<id>` with R, in the loop (`create.rs:305`) and in
   `finish` (`:498`), and apply row 3 to a record that is not R.
2. **F9:** rebuild `InDoubt`'s `source()` chain (`:174-186`) so that `classify` returns
   `Indeterminate`.
3. **F8 (d):** after a 1021 whose settling read fails with nothing held, end `InDoubt`
   (`may_still_commit: false`), not `CreateError::Store`.
4. **Tests:** add C3 (attempt 1's `PlantSessionFirst`, `:763`, kept as this leg), C4 (a
   `PlantGroupFirst` beside it), C6 (i) and (ii), F8 (a) to (d), and F9. Rebuild F3 (`:1362-1371`)
   as the write-back case. `Scripted` gains a plain-`Err` commit action and a failing `get`.
   Add the (B) contention-with-nothing-moved leg. Add the size test's Completed-widest fixture
   beside its existing one, and make each fixture assert which state is strictly widest.
   Extend F7's recorded batches with the `mpuctl` put, and assert exactly two commits and
   batch 2's precondition and put (attempt 1 asserts the precondition only).
5. **DST:** add the landed-`InDoubt` witness leg. Attempt 1's witness at
   `multipart_admission.rs:450-461` does not test what its message claims, so replace its
   claim with this witness.
6. **Docs:** add the `KnobSet` field lines and the rustdoc bounds from § *Decided*, and re-pin
   the module header's `05-building-block-view.md:202-204` reference to `:206`. Replace the
   "never ordinary concurrency on a ledger with room" sentence in the rustdoc and the doc
   paragraph with the narrowed one (§ *Docs currency*).
7. **Verification:** run every row of the demonstrated-reds table (24 rows, 26 edits with
   row 13's three) and record them.

## Alternatives considered

- **One shared retry budget.** This is the #508 defect, already split into two constants by #655.
  Rejected.
- **A contention budget fixed at the shipped `MAX_SESSIONS`.** A deployment with a larger
  profile would refuse creates while its ledger has room. Rejected: the budget follows the
  profile.
- **Ownership by the record's content alone, or by history alone.** Content alone adopts a
  byte-identical record that existed before this create (C5). History alone adopts another
  create's session under a shared upload id (C6, attempt 1's defect). Rejected: rows 1 and 2
  require both.
- **Re-minting when the found record is not R while a batch is held.** It can leave a counted
  session nothing named (`0016:2015-2018`). Rejected: row 3.
- **Recovery that replays the identical batch.** Its `mpuctl` CAS is stale once anything has
  moved the ledger, so it can never commit (X101, `0016:2640`). Rejected.
- **A separate cap for unknown outcomes, or Create taking a `KnobSet`.** The human decided
  against both for this slice (above).
- **Splitting the DST scenario into its own slice.** The human decided against it on 2026-10-17.
  The scenario would land after the concurrent path it covers, against `AGENTS.md:189-190`,
  and the split saved about 21 KB of 148 KB.
- **Defining the bucket deletion-fence key here.** It would decide #511's open choice from a
  multipart slice. Rejected: deferred as stated.

## Open questions

1. If writing a canonical session or ledger record needs a constructor the decoders cannot
   round-trip, say so in `build-notes.md` and stop. Do not loosen a decoder.

None else. If Do finds a situation the decision table does not cover, record it in
`build-notes.md` as an open item for sign-off. Do not invent an answer for it.

## Sources for claims taken from the tracker and the Plan sessions

These are for the sign-off reader. Do need not open them. All paths are in the wyrd-pdca repo.
- **The #508 measurement** (8 → 2 refused, 10 → 3, 16 → 3; aws-cli's `max_concurrent_requests`
  = 10): #656's issue body, "Carried forward from #508's rejected attempts"
  (https://github.com/getwyrd/wyrd/issues/656; copy in `results/issue_656/notes.json`, `body`).
- **The `Aborting@1` growth and the `owns()` lesson:** the review of #656's attempt 2,
  `results/issue_656/iteration-v2/SUMMARY.md`.
- **Attempt 1's findings:** `results/issue_894/iteration-v1/SUMMARY.md`, §5 and §6. The
  ownership defect is at `:181-195` and `:354`. The test gaps are at `:228-298`. The 0-of-99
  DST count is at `:292-293`. The two design calls are at `:300-320`. The human's iterate-to-Plan
  rationale is at `:391`.
- **This re-plan's decisions** (one slice; unknown outcomes stay on the contention budget;
  `KnobSet` reconciliation to #508): the 2026-10-17 Plan session. The human's words: "I don't
  mind the size or budget issue, but we must spec the thing so it converges."
- **The deny ruling is superseded:** #903 (`0674f4d`) and #904 (`016f98e`) are merged on `main`.
- **Tracker state,** checked 2026-10-17 with `gh issue view`: #894, #895, #768, #656, #508,
  #511, #659 and #668 are OPEN.

## Plan review, 2026-10-17 (all three findings taken)

For the sign-off reader; `plan-advisory-plan-reviewer.md` in this bundle has the findings.
1. **Contention guarantee.** The old "a refusal never comes from ordinary concurrency on a
   ledger with room" was false on TiKV. Narrowed in Invariant and Docs currency, and pinned by
   the new (B) leg and mutations 21-22. The budget decision is unchanged.
2. **Size mutations.** Added the exactly-at-ceiling point (attempt 1's test already had it)
   and a Completed-widest fixture. Split mutation 18 into 18a-c. Named the `Open` / `Aborting`
   omission as an equivalent mutation.
3. **F7.** The competing admission now lands inside commit 1, before any recovery read. The
   test asserts exactly two commits and batch 2's `mpuctl` precondition and put, which a
   replay-then-retry cannot meet.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Code is correct; the gaps are tests and one doc paragraph. Close the adversary's four unpinned mutants with its probe tests: (1) settling read before NoSuchBucket/ProfileSkew (create.rs:373, :392) plus the torn ledger met while in doubt (create.rs:382 -> :535); (2) doubt reset on re-mint (create.rs:362), the C5 case on a re-minted identity; (3) "found something else and not held, the ending stands" after a 1021 (create.rs:343 and :564; cases X1, X2); (4) settling read in the held identity-taken branch (create.rs:349; release the held batch on the first seggrp: read). Add a cancel-safety paragraph to create_session's rustdoc "Termination" section: do not drop the future after a commit is sent; bound it through the store's own deadlines. No other changes; size already accepted.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Code is correct; the gaps are tests and one doc paragraph. Close the adversary's four unpinned mutants with its probe tests:
  (1) settling read before NoSuchBucket/ProfileSkew (create.rs:373, :392) plus the torn ledger met while in doubt (create.rs:382 -> :535);
  (2) doubt reset on re-mint (create.rs:362), the C5 case on a re-minted identity;
  (3) "found something else and not held, the ending stands" after a 1021 (create.rs:343 and :564; cases X1, X2);
  (4) settling read in the held identity-taken branch (create.rs:349; release the held batch on the first seggrp: read).
  Add a cancel-safety paragraph to create_session's rustdoc "Termination" section: do not drop the future after a commit is sent; bound it through the store's own deadlines. No other changes; size already accepted.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Losing track of an admitted session is the worst outcome, so fix it now rather than deferring to the reaper slice. Fix (overrides brief decision-table row 3 for this one case): after an unknown commit outcome under the current identity, if the re-read of mpu:<id> finds a record that is not R but carries THIS identity's nonce (and request: bucket/object) — e.g. the session was fenced Open@0 -> Aborting@1 via SessionRecord::open_teardown (crates/core/src/multipart.rs:2509) — it is this create's own transitioned session, not a collision. Do not re-mint and do not answer Created under a new id (create.rs:347-373). End with CreateError::InDoubt naming the original upload id and nonce (may_still_commit per the existing held rule). A record with a different nonce stays a collision (C6 behaviour unchanged). Regression: applied 1021 (may_still_commit: false), then a fence to Aborting@1 before the recovery read; assert no second identity minted, mpuctl.count() up by exactly one, and InDoubt names the original id/nonce. This closes the T4 batch-review finding. Also close the adversary's two unpinned rules while rebuilding: (1) F8(e): plan [Hold, Unknown{applied:false, may_still_commit:false}] with every later get failing -> InDoubt{may_still_commit:true}, then the held batch lands; (2) read order bucket -> ledger -> contention: profile_of(1), plan [AdmitFirst(1)], no ledger at start -> Refused(SlowDown{SessionCap{count:1,max_sessions:1}}) with script.waits empty, plus assert waits empty in b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket. No other changes; size already accepted.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Losing track of an admitted session is the worst outcome, so fix it now rather than deferring to the reaper slice.
  Fix (overrides brief decision-table row 3 for this one case): after an unknown commit outcome under the current identity, if the re-read of mpu:<id> finds a record that is not R but carries THIS identity's nonce (and request: bucket/object) — e.g. the session was fenced Open@0 -> Aborting@1 via SessionRecord::open_teardown (crates/core/src/multipart.rs:2509) — it is this create's own transitioned session, not a collision. Do not re-mint and do not answer Created under a new id (create.rs:347-373). End with CreateError::InDoubt naming the original upload id and nonce (may_still_commit per the existing held rule). A record with a different nonce stays a collision (C6 behaviour unchanged).
  Regression: applied 1021 (may_still_commit: false), then a fence to Aborting@1 before the recovery read; assert no second identity minted, mpuctl.count() up by exactly one, and InDoubt names the original id/nonce. This closes the T4 batch-review finding.
  Also close the adversary's two unpinned rules while rebuilding: (1) F8(e): plan [Hold, Unknown{applied:false, may_still_commit:false}] with every later get failing -> InDoubt{may_still_commit:true}, then the held batch lands; (2) read order bucket -> ledger -> contention: profile_of(1), plan [AdmitFirst(1)], no ledger at start -> Refused(SlowDown{SessionCap{count:1,max_sessions:1}}) with script.waits empty, plus assert waits empty in b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket.
  No other changes; size already accepted.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_894/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Code is correct; the only gaps are two unpinned rules. Add exactly two tests, change no production code. (1) Pin the nonce compare in same_session (crates/core/src/multipart.rs:2456): let RivalFirst take the rival's object, and add a C6 variant where create A (upload id U, nonce N1) gets a 1021 (may_still_commit: false, not applied), then a rival with the same U, a different nonce N2 and the SAME request (bucket/object) installs its session before A re-reads. Assert A answers Created under a new identity (a collision, not InDoubt(SessionMoved)), and the rival's record is byte-identical afterwards. Must go red if the nonce conjunct is deleted. (2) Pin read order bucket -> ledger: bucket marker absent AND mpuctl at count == max_sessions (profile_of(1), ledger count 1) -> Refused(NoSuchBucket), not SlowDown{SessionCap}, and writes nothing. Must go red if the bucket block (create.rs:407-415) is moved after the ledger block (:417-444). No other changes; size already accepted.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Code is correct; the only gaps are two unpinned rules. Add exactly two tests, change no production code.
  (1) Pin the nonce compare in same_session (crates/core/src/multipart.rs:2456): let RivalFirst take the rival's object, and add a C6 variant where create A (upload id U, nonce N1) gets a 1021 (may_still_commit: false, not applied), then a rival with the same U, a different nonce N2 and the SAME request (bucket/object) installs its session before A re-reads. Assert A answers Created under a new identity (a collision, not InDoubt(SessionMoved)), and the rival's record is byte-identical afterwards. Must go red if the nonce conjunct is deleted.
  (2) Pin read order bucket -> ledger: bucket marker absent AND mpuctl at count == max_sessions (profile_of(1), ledger count 1) -> Refused(NoSuchBucket), not SlowDown{SessionCap}, and writes nothing. Must go red if the bucket block (create.rs:407-415) is moved after the ledger block (:417-444).
  No other changes; size already accepted.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
