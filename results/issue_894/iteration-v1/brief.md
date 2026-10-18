# core: multipart admission + Create (636.3a)

> The Plan artifact for slice 3a of #636. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. Split from #656 at the 2026-10-15 re-plan,
> after four attempts at Create and Abort together. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read the **Create session**
> batch row (`0016:657`) and the unknown-outcome recovery paragraph (`0016:2009-2027`) before
> writing code. Citations verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on
> 2026-10-15.

- **Slug:** multipart-admission-create
- **Track:** alpha
- **Goal:** Nothing on `main` creates a multipart session. The module is deliberately pure
  (`crates/core/src/multipart.rs:5-7`): no `MetadataStore` call and no `WriteBatch`. This slice
  adds `CreateMultipartUpload`'s admission through the serialized fleet ledger `mpuctl`, plus the
  session's creation, as a `wyrd-core` function over `MetadataStore`. It answers in the
  vocabulary already on `main`: `CreateOutcome` (`:4380`), `Refusal` (`:4302`) and
  `Backpressure` (`:4277`). Abort is the sibling slice (636.3b) and is not built here.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_create.rs`, over
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
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target
  and does not depend on the scheduler. `RedbMetadataStore` checks every `require` /
  `require_absent` inside one serialized write transaction and answers `CommitOutcome::Conflict`
  (`crates/metadata-redb/src/lib.rs:212-222`). (A′)'s wrapper forces the losing order, so a shared
  or collision-sized budget fails (A′) on every run. (F)'s wrapper forces the unknown outcome, so
  erroring out or re-minting fails (F) on every run. The defect (A) guards against is the one #508
  measured: one budget shared between CAS contention and id collision refused creates on an empty
  store. No special topology is needed. (G) is madsim-only (`#![cfg(madsim)]`). See `Verification
  posture` for why C4-ci cannot run it today and how its evidence is produced instead.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*: this slice gives the session state machine its first client-driven
  writer). Every `mpu:` session and `seggrp:` marker a Create installs is counted by `mpuctl` **in
  the same atomic batch**. Every session a Create installs is one its caller is told about, by
  `Created` or by an in-doubt error naming its upload id, so no admitted session is outside
  the reach of the actor that will reclaim it (#659's drain, #625's reaper). The admission count
  changes only on admission (+1) and on the terminal delete (−1, #659). A refusal never comes
  from ordinary concurrency on a ledger with room. **Sources:** the maintainer's rule
  2026-07-25 (authoritative, via principles.md §5); 0016's refutation standard (`0016:2813`);
  the batch row `0016:657`; the admission-counter rationale `0016:246-255`. SELF-TEST: this
  relates a batch, a counter and what the caller is told, so guarding any one function cannot
  satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 768
- **Ordering note:** Wave before its sibling, the Abort slice #895, which depends on this one.
  Prerequisites #654 (record family) and #655 (knobs) are closed and on `main`. `Conflicts with 768`
  (added by the planner after `--accept`): both edit
  `docs/design/architecture/05-building-block-view.md` (this slice the multipart paragraph at
  `:202`, #768 the object-metadata paragraph at `:197-200`, adjacent hunks). A split proposal's
  ordering fields can name only siblings. **Intake-cap override (wyrd-pdca-P1):** the human
  allowed both children of #656's split in over the alpha cap in the 2026-10-15 Plan session;
  count then 7/6, room 0, need 2.
- **Scope:** `CreateMultipartUpload`'s admission and session creation as a store-I/O function in
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
- **Repro instruction:** n/a (new functionality). Today
  `rg -n 'async fn|WriteBatch::new|\.commit\(' crates/core/src/multipart.rs` finds nothing: no
  store call exists, so nothing creates a session.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_create.rs`, a **NEW** file (C4-verify treats an
  added `crates/*/tests/*.rs` as the per-fix discriminator). The DST scenario
  `crates/dst/tests/multipart_admission.rs` is also NEW.
- **Verification posture:** (a) net-new. `create_session` (or whatever Do names it) does not
  exist on `main`, so the discriminating test cannot compile on C4-verify's reverted base, and
  C4-verify reports **UNVERIFIABLE** (exit 77). That is a pre-declared sign-off item, not a
  defect.
  **Built and exercised at Check:** the Create function itself, driven by the new test against
  a real store with real preconditions and real contention.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - the contention budget capped at `MAX_UPLOAD_ID_ATTEMPTS` → (A′) fails;
  - a re-mint on an unknown outcome → (F) fails;
  - the absence read before a fresh identity's first commit removed, with ownership decided by
    comparing every field of the record found → (C)'s byte-identical leg fails;
  - the recovery replays the original batch unchanged instead of re-reading `mpuctl` → F7 fails;
  - the kept doubt replaced by the latest outcome → F6 fails;
  - (G) only: `Created` returned for a 1021 without settling it by a read → (G)'s reverse
    assertion fails on some seed of the 1021 leg.
  **The inherited deny failure (human's ruling, 2026-10-15).** `cargo xtask ci` stops at the
  all-features cargo-deny step (RUSTSEC-2026-0346/0349) before its conformance, statics,
  orchestrator-guard and DST steps (`xtask/src/main.rs:1683-1687`). So C4-ci's log cannot show
  those steps. Do does not touch `Cargo.lock` or `deny*.toml`. Do runs `cargo xtask
  conformance`, `cargo xtask statics` and `cargo xtask dst` on its own, and puts each command's
  result in `build-notes.md`. The DST command includes the madsim clippy pass and the 50-seed
  sweep (`:1716`, `:1721-1758`). A reviewer can re-run the same three commands.
  The test-local wrappers keep their state in struct fields, never in a `static`. The ADR-0035
  statics scan covers `crates/core` and `crates/dst` (`xtask/src/main.rs:1221-1242`).
- **Production reach:**
  - (a) The test exercises the function for real: a real `MetadataStore`, real preconditions,
    real contention. Production calls nothing in this slice.
  - (b) The S3 verb that calls it lands with #668. That slice also carries 0016's normative rule
    that the reaper (#625) and operator abort ship in the same release as any verb that creates
    sessions (`0016:236-268`).
  - (c) So until #668, no deployment can create a session. This slice **must not** add a
    production caller: no gateway, CLI or custodian wiring.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer callsite for store-I/O shape:** `crates/core/src/write.rs:266` (`commit_create`)
    takes the store as `&impl MetadataStore` and builds one `WriteBatch` with preconditions.
    Mirror that shape.
  - **Peer for the DST scenario:** `crates/dst/tests/commit_ambiguity.rs` (its 1021 and 1031
    legs, the `#![cfg(madsim)]` root at `:67`, and the sweep helpers).
  - **Symbols:** `MPUCTL_KEY` (`multipart.rs:1132`), `MPU_PREFIX` (`:1147`), `mpu_key`
    (`:1227`), `Budget::max_sessions` (`:1683`), `AdmissionRecord` (`:1840`, `count` `:1848`,
    `max_sessions` `:1853`, `profile` `:1858`), `decode_admission_record` (`:1914`),
    `SessionRecord` (`:2265`, no free constructor `:2259-2263`), `decode_session_record`
    (`:2504`), `MAX_UPLOAD_ID_ATTEMPTS` (`:4983`), `MAX_ADMISSION_CAS_ATTEMPTS` (`:4997`),
    `admission_backoff_millis` (`:5023`).
  - **Bucket and sizes:** bucket existence is the `bucket:` record at `metadata::bucket_key`
    (`crates/core/src/metadata.rs:52`). The reservation marker is `metadata::seggrp_key`
    (`:1532`). The value ceiling is `MAX_VALUE_BYTES` (`:549`), and
    `flat_value_ceiling_crossed` (`:605`) measures it.
- **Prior-art check (triage cycles):** searched by path `crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/tests/multipart_create.rs`,
  `crates/dst/tests/multipart_admission.rs` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main`. Merged: `2f41e73` (#847,
  the segment-group nonce on every session), `4533549` (#655, the capacity knobs and the two
  split budgets), `14911ec` (the answer table, #654 3/3). No store module and neither new file
  exists. Rejected: #508's seven monolithic attempts (the false-`503` measurement came from one
  of them); #636, closed not-planned 2026-10-10 as too large; #656's four attempts at Create
  and Abort together (this split; their lessons are folded into Design below). Open PRs on
  getwyrd/wyrd touching these paths: none.
- **Disposition hint:** new-feature

**Sources for the claims taken from the tracker and the Plan session** (for the sign-off reader;
Do need not open them, all paths in the wyrd-pdca repo):
- the #508 measurement (8 → 2 refused, 10 → 3, 16 → 3; aws-cli's `max_concurrent_requests` = 10):
  #656's issue body, "Carried forward from #508's rejected attempts"
  (https://github.com/getwyrd/wyrd/issues/656; copy in `results/issue_656/notes.json`, `body`);
- attempt 2's two lessons (the 100,000 → 100,004-byte `Aborting@1` growth, and `owns()` comparing
  only nonce, parent and object): the attempt-2 review, `results/issue_656/iteration-v2/SUMMARY.md:121`,
  `:129` and its iteration delta at `:228`;
- the intake-cap override and the inherited-deny ruling: the 2026-10-15 Plan session, recorded at
  the top of `results/issue_656/split-proposal.md:4-14` (the `plan-cap` line, the human's words
  "please start the split", and the RUSTSEC-2026-0346/0349 ruling);
- tracker state, checked 2026-10-15 with `gh issue view`: #656 OPEN (bundle closed as `split`),
  #894, #895 and #768 OPEN.

Plan-review response (2026-10-15): all four findings hold and are revised in place.
(1) Sources: listed above. (2) Ownership: whole-record equality is removed as sufficient; the
evidence is the invocation's own absence read plus its own unknown commit (Design § Adopting),
leg C now plants a byte-identical record, and the same-identity concurrent case is stated as a
bound. (3) F7 forces the stale-precondition case (`0016:2018-2023`) with a second create moving
the ledger, and a replay mutation is a required red. (4) (G) adds the reverse assertion (every
`Created` is a counted, stored session) and requires the 1021 leg to observe an unapplied 1021.

## Design

**Where.** A new store-I/O module in `wyrd-core`, beside the pure module rather than inside its
pure sections. Do picks the layout. Update the module header's "deliberately pure" paragraph
(`crates/core/src/multipart.rs:5-7`, and the living-architecture clause at `:104-118`) to name
what now writes. Core stays runtime-free (ADR-0009): the backoff wait, the clock and its
`clock_source`, and the randomness for ids, nonces and jitter all come from the caller.
`admission_backoff_millis` is already pure for this reason (`:5012-5023`).

**The Create batch (`0016:657`)**, one per attempt:
- **Ledger present:** `require(mpuctl == prior bytes)`, with `prior.count < prior.max_sessions`
  (the **stored** limit). The local profile is compared with `prior.profile`; a mismatch is
  `ProfileSkew`, with no write.
- **Ledger absent:** `require_absent(mpuctl)` and a bootstrap `{count: 1, max_sessions,
  profile}` from the local `Budget`.
- **Either way:** `require_absent(mpu:<id>)`, `require_absent(seggrp:<nonce>)`, and
  `require(bucket:<name> == the exact marker bytes read)` (ADR-0046 §4,
  `docs/design/adr/0046-bucket-model-real-namespace.md:93-95`: "`require` on the marker
  bytes"). Then put the session, put the marker, and put the ledger at `count + 1`.
- **The deletion-fence half of ADR-0046 §4 is deferred to #511, stated, not dropped.** §4 also
  asks for `require_absent` on the bucket's deletion fence (decision 5, `:104-121`). No fence
  key exists on `main`: decision 5 only sketches one and leaves the choice to #511, and nothing
  in `crates/` defines or writes one. So nothing on `main` can put a bucket into the deleting
  state, and this slice must not invent the key. Keep the bucket precondition in one place in
  the batch so #511 adds the fence there, mark that place, and record the carry-forward in
  `build-notes.md`.

**On `Conflict`, re-read and classify which precondition lost:**
- `mpuctl` moved → contention. Back off per `admission_backoff_millis` and retry. The
  contention budget is the **profile's own `max_sessions`** (`Budget::max_sessions`; the local
  and stored profiles are equal or the create already answered `ProfileSkew`), not the fixed
  `MAX_ADMISSION_CAS_ATTEMPTS`. When it is spent, answer `SlowDown { AdmissionContention }`.
- `mpu:<id>` or `seggrp:<nonce>` taken by **another** session → collision. Re-mint, up to
  `MAX_UPLOAD_ID_ATTEMPTS`.
- Bucket marker gone or changed → `NoSuchBucket`.

**Adopting a session is narrow, and the evidence is this invocation's own history, never the
record's content.** A Create may answer `Created` for an upload id it finds in the store only
when both hold:
1. it read `mpu:<id>` and `seggrp:<nonce>` **absent** before its first commit for that identity
   (a record present at that read is a collision: re-mint, whatever it contains); and
2. a commit of its **own** for that identity returned an unknown outcome.
Comparing the record is not evidence. `SessionRecord`'s fields carry no per-invocation token
(`crates/core/src/multipart.rs:2265-2279`), so two invocations given the same identity and
request write identical bytes, and a whole-record match proves nothing. Comparing nonce, bucket
and object alone handed one client another's upload in attempt 2. Do may add a whole-record
check as a sanity check on top of (1) and (2), never in place of them. The one case (1) and (2)
cannot settle, two live invocations handed the same 128-bit identity, is outside the guarantee
(leg C's stated bound), and the rustdoc says so.

**An unknown commit outcome (`0016:2009-2027`).** A `CommitUnknownResult` is never read as
`Conflict` and never answered by minting a new id. With `may_still_commit == true`, an absent
re-read does not prove the batch will not land (`crates/traits/src/lib.rs:240-247`). The
contract:
- the attempt's identity (upload id and group nonce) survives the unknown outcome;
- the re-drive uses that same identity, and rebuilds the `mpuctl` precondition from a fresh read
  (replaying the identical batch can never commit once `mpuctl` has moved);
- `require_absent(mpu:<id>)` alone gives the mutual exclusion, so at most one attempt lands;
- with `may_still_commit == false`, one re-read settles it;
- **the doubt is kept.** Once a batch of this invocation may still land, a later outcome that is
  out of flight (`may_still_commit: false`), a failed settling read, or a plain store fault does
  not clear it. The create's final error still says `may_still_commit: true` and names the
  identity. Only a read that proves the batch can no longer land clears it;
- **settle before giving up.** An unknown outcome on the last budgeted attempt is settled by a
  read before the create returns. A capacity refusal met during recovery is checked against the
  original `mpu:<id>` first, because the earlier batch may be what filled the ledger.
Do chooses whether core runs the re-drive itself (with the caller's wait, as for backoff), or
returns an error that carries the identity plus a core entry point to re-drive it. Either way the
gateway (#668) must not have to rebuild the batch, and (F) must pass.

**The creation routes.** `SessionRecord` has no free constructor on purpose
(`crates/core/src/multipart.rs:2259-2263`), and `AdmissionRecord` has none either. Add the **one**
route for each that holds the identity the decoders enforce: `encode(decode(bytes)) == bytes`,
canonical field order, and `content_type` omitted when absent. That covers the ledger's first and
incremented values. The initial epoch, attempt count and `clock_source` follow 0016 §1's `mpu:`
row. Do not loosen a decoder to fit; if a route needs that, say so in `build-notes.md` and stop.

**Size headroom.** A session admitted at `Open` must be able to reach every later state. In
attempt 2, an `Open` record admitted at the ceiling became 4 bytes larger as `Aborting@1`, so
every fence of it (Abort's, restore's) failed forever and its slot leaked. Measure the widest
encoding of every state the session can reach, with the real encoder (so JSON escapes count),
against `MAX_VALUE_BYTES`, and refuse before any read or commit.

**Docs currency** (`AGENTS.md:154-157`, a merge requirement): the paragraph at
`docs/design/architecture/05-building-block-view.md:202` says "Nothing else writes or consumes
them in production yet — the rest of the protocol … arrives with the store round trip
(#656–#659)". Update it to say that Create now writes `mpuctl`, `mpu:` and `seggrp:`, with no
production caller until #668. Also say that a contention refusal means the ledger moved or the
store could not report an outcome, that many times over, never ordinary concurrency on a ledger
with room. Write the same in the function's rustdoc.

## Alternatives considered

- **One shared retry budget.** This is the #508 defect, already split as constants by #655.
  Rejected.
- **A contention budget fixed at the shipped `MAX_SESSIONS`.** A deployment with a larger
  profile would refuse creates while its ledger has room. Rejected; the budget follows the
  profile.
- **Answering `Created` for any found record with this attempt's nonce, bucket and object, or
  with every field equal.** Both adopt another client's session: the first did in attempt 2,
  and the second does whenever the identity repeats with the same request. Rejected (see
  "Adopting a session is narrow").
- **Recovery that replays the identical batch.** Its `mpuctl` CAS is stale once any other
  create moved the ledger, so it can never commit. Rejected (`0016:2009-2027`).
- **Defining the bucket deletion-fence key here.** It decides #511's open choice from a
  multipart slice. Rejected; deferred as stated.

## Open questions

1. If writing a canonical session or ledger record needs a constructor the decoders cannot
   round-trip, say so in `build-notes.md`. Do not loosen a decoder.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized (145 KB patch vs 100 KB threshold; ~1,900 test lines) and every required fix adds more tests. Split in the re-plan: (a) core create + unit/forced-schedule tests (A, A', B, C, F); (b) seeded DST scenario (G) as its own slice on top. Carry forward into the new briefs: - Real defect: recovery adopts any existing mpu:<id> as its own (create.rs:305, :498). A different creator with the same upload id but a different nonce/object gets returned as Created. Decode and check the found record's nonce/request fields in both recovery paths; add a forced-schedule regression (same id, distinct nonce/object). - Test gaps to pin: kept doubt after a plain read/commit fault (:300, :393, :500); require_absent(seggrp:<nonce>) (add a PlantGroupFirst leg); F3 must seed the ledger and replay the c bytes (A-B-A) so only require_absent(mpu:<id>) can stop the late batch; InDoubt must classify as Indeterminate through the source chain; DST must witness at least one InDoubt batch that actually lands at quiesce. - Open design calls, not yet decided by the human: (1) KnobSet::max_admission_cas_attempts / max_upload_id_attempts are clamped but never read by Create: retire them or have Create take a KnobSet; (2) should unknown outcomes get their own small cap instead of spending the max_sessions-sized contention budget; (3) the brief's tracker claims/rulings (#508 numbers, deny ruling) were unverifiable by the reviewer: include source excerpts or mark them unverified.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Slice is oversized (145 KB patch vs 100 KB threshold; ~1,900 test lines) and every required fix adds more tests. Split in the re-plan: (a) core create + unit/forced-schedule tests (A, A', B, C, F); (b) seeded DST scenario (G) as its own slice on top.
  Carry forward into the new briefs:
  - Real defect: recovery adopts any existing mpu:<id> as its own (create.rs:305, :498). A different creator with the same upload id but a different nonce/object gets returned as Created. Decode and check the found record's nonce/request fields in both recovery paths; add a forced-schedule regression (same id, distinct nonce/object).
  - Test gaps to pin: kept doubt after a plain read/commit fault (:300, :393, :500); require_absent(seggrp:<nonce>) (add a PlantGroupFirst leg); F3 must seed the ledger and replay the c bytes (A-B-A) so only require_absent(mpu:<id>) can stop the late batch; InDoubt must classify as Indeterminate through the source chain; DST must witness at least one InDoubt batch that actually lands at quiesce.
  - Open design calls, not yet decided by the human: (1) KnobSet::max_admission_cas_attempts / max_upload_id_attempts are clamped but never read by Create: retire them or have Create take a KnobSet; (2) should unknown outcomes get their own small cap instead of spending the max_sessions-sized contention budget; (3) the brief's tracker claims/rulings (#508 numbers, deny ruling) were unverifiable by the reviewer: include source excerpts or mark them unverified.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 54 mutants tested in 2m: 3 missed, 39 caught, 12 unviable
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
