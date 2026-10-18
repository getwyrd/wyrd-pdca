# Design proposal — issue 656 / multipart-admission-create-abort

> The Plan artifact (design-proposal form; enhancement). Do reads ONLY this file. The
> `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> Slice 3 of 7 of #636 (re-sliced from the discontinued monolith; #654 and #655 are merged).
> The design is settled and normative in proposal **0016**,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; read its batch inventory rows
> for **Create session** (`0016:657`) and **Abort / reap fence** (`0016:665`), and decision 3's
> verb × state table (`0016:973-979`), before writing code. Citations verified against
> `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on 2026-10-14.

- **Slug:** multipart-admission-create-abort
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** The first store-I/O slice of the multipart protocol: `CreateMultipartUpload`'s
  admission through the serialized fleet counter (`mpuctl`) plus session creation, and
  `AbortMultipartUpload`'s teardown fence, as `wyrd-core` functions over `MetadataStore`. They
  answer in the typed vocabulary already on `main` (`CreateOutcome`, `AbortOutcome`, `Refusal`,
  `Backpressure`: `crates/core/src/multipart.rs:4277-4452`). Today the module is deliberately
  pure (`crates/core/src/multipart.rs:5-7`, `:104-118`): nothing creates a session, and nothing
  but the restore pass fences one.
- **Success criterion:** A NEW test over `RedbMetadataStore::in_memory()` on a multi-thread
  tokio runtime shows all of the following.
  **(A) Admission under real contention.** On a fresh empty store, **8, 10 and 16** concurrent
  creates (each level on its own fresh store; all three asserted, not one) all answer
  `CreateOutcome::Created`. Their upload ids are pairwise distinct. Exactly N `mpu:` records
  exist (scan `MPU_PREFIX`), each decoding through `decode_session_record` as an `Open` session
  for the requested bucket inode and object name. Exactly N `seggrp:` markers exist, one per
  session's nonce. `mpuctl` decodes through `decode_admission_record` with `count() == N`
  exactly. These are the levels #508's rejected attempts measured as falsely refused (8 → 2
  refused, 10 → 3, 16 → 3; aws-cli's default `max_concurrent_requests` is 10). These free-running
  levels stay, but they are not the red (see A′): redb's `get` and `commit` never await inside
  (`crates/metadata-redb/src/lib.rs:113-117`, `:208-234`), so a valid schedule can serialize them.
  **(A′) Admission under a forced schedule.** The same test drives N = 8 creates through a
  test-local `MetadataStore` wrapper over `RedbMetadataStore` that controls the order: every
  creator's `mpuctl` read is held until all live creators have read it, before any of them may
  commit, so each round has one winner and the rest lose the CAS. The wrapper counts each
  creator's `mpuctl` CAS losses (it can tell creators apart by the `mpu:<id>` key in each batch).
  Assert that all 8 answer `Created`, that `mpuctl.count() == 8`, and that at least one creator
  lost the `mpuctl` CAS **more than `MAX_UPLOAD_ID_ATTEMPTS` (= 2, `:4983`) times** and still
  succeeded. That last assertion is what a shared, collision-sized budget fails on every run.
  **(B) The real bounds, each writing nothing.** With the ledger at its stored `max_sessions`, a
  create answers `Refused(SlowDown { pressure: SessionCap { .. } })`. A create whose local
  `Budget` differs from the profile stored in `mpuctl` answers `Refused(ProfileSkew { .. })`. A
  create for a bucket with no `bucket:` record answers `Refused(NoSuchBucket)`. A create whose
  bucket marker is **deleted or rewritten between Create's read and its commit** (the same
  wrapper, acting at commit) answers `Refused(NoSuchBucket)`. After each, the store is
  byte-identical to before: no `mpu:`, no `seggrp:`, and no `mpuctl` write.
  **(C) Two separate retry budgets.** When the first minted upload id collides with an existing
  `mpu:` record, the create re-mints and succeeds. When every one of `MAX_UPLOAD_ID_ATTEMPTS`
  mints collides, it refuses. Losing the `mpuctl` CAS never spends the collision budget, and a
  collision never spends the contention budget (`MAX_ADMISSION_CAS_ATTEMPTS`).
  **(D) Abort per decision 3 (`0016:977`).** On `Open@E` it answers `Fenced`; afterwards the
  session decodes as `Aborting@E+1`, and `retire:bytes:s:<id>:<E>` holds exactly the
  `{session, all}` obligation `SessionRecord::open_teardown` mints, committed in the **same**
  batch as the fence. A second Abort answers `AlreadyAborting`. **Two concurrent Aborts** of one
  `Open` session answer one `Fenced` and one `AlreadyAborting`, never `OperationAborted`. Abort
  of a `Completing` session answers `Refused(OperationAborted)` and leaves the store unchanged.
  Abort of a `Completed` session, or of an upload id with no record, answers
  `Refused(NoSuchUpload)`.
  **(E) Abort never moves the counter.** `mpuctl.count()` is identical before and after every
  leg of (D), and a `Completed` tombstone stays counted. The decrement belongs only to the
  terminal delete (#659).
  **(F) An unknown Create outcome is recovered under the original upload id (`0016:2009-2027`).**
  Through the wrapper, the first Create commit returns `CommitUnknownResult { may_still_commit:
  true }` (`crates/traits/src/lib.rs:230-248`) in three variants: (F1) the batch was applied
  before the error; (F2) it was not applied; (F3) it is held and applied **after** the recovery
  re-drive has committed (a late commit, which must then lose its own `require_absent(mpu:<id>)`).
  In each, recovery re-drives with the **same** upload id and nonce, rebuilds the `mpuctl`
  precondition from a fresh read, and answers `Created`. Afterwards there is exactly one `mpu:`
  record, for that upload id, exactly one `seggrp:` marker, and `mpuctl.count()` rose by exactly
  one. No second upload id is ever minted.
  **(G) Seeded coverage** (`AGENTS.md:188-190`: a new concurrent path lands with seeded Tier-0
  DST coverage). A NEW madsim scenario `crates/dst/tests/multipart_admission.rs` runs concurrent
  Creates and Aborts with the commit-ambiguity nemesis armed, mirroring
  `crates/dst/tests/commit_ambiguity.rs`'s 1021/1031 legs. After every seed: the number of `mpu:`
  records equals `mpuctl.count()`, every `mpu:` record has its `seggrp:` marker, and every
  `Aborting` session has its `retire:bytes:` obligation. `cargo xtask ci` sweeps it (it includes
  `cargo xtask dst`, 50 seeds), so C4-ci runs it.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core`
  target, and it does not depend on the scheduler. `RedbMetadataStore` evaluates every
  `require` / `require_absent` atomically at commit and answers `CommitOutcome::Conflict`
  (`crates/metadata-redb/src/lib.rs:212-222`). (A′)'s wrapper forces the losing order, so a
  shared or collision-sized budget fails (A′) on every run, not only on an unlucky schedule. (F)'s
  wrapper forces the unknown outcome, so an implementation that errors out or re-mints fails it
  on every run. The defect class (A) guards against is the one #508 measured: one budget shared
  between CAS contention and id collision refused creates on an empty store. No special topology
  is needed. The DST scenario (G) is madsim-only (`--cfg madsim`), which `cargo xtask ci` sets
  for its DST step. The functions do not exist on `main`, so C4-verify cannot compile the test
  on the reverted base (see `Verification posture`).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row, *storage
  lifecycle / reclamation*: this slice gives the session state machine its first client-driven
  writers). Every `mpu:` session and `seggrp:` marker a Create installs is counted by `mpuctl`
  **in the same atomic batch**. Every Abort installs, **in the same batch** as its fence, an
  obligation naming everything the session protects. So no admitted session and no fenced
  session is ever outside the reach of the actor that will reclaim it (#659's drain, #625's
  reaper). The admission count changes only on admission (+1) and on the terminal delete (−1,
  #659), never on a fence. **Sources:** maintainer's rule 2026-07-25 (authoritative); 0016's
  refutation standard `0016:2802-2813`; the batch rows `0016:657`, `0016:665`. SELF-TEST: this is
  a relation between two batches and a counter, so guarding any single function cannot satisfy
  it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 768
- **Ordering note:** Wave 0. Its prerequisites (#654's record family, #655's knobs) are closed
  and on `main`, so there is no batch edge. `Conflicts with 768`: both update the
  living-architecture doc's metadata section, `docs/design/architecture/05-building-block-view.md`
  — this one the multipart paragraph at `:202`, #768 the object-metadata paragraph at
  `:197-200`. Adjacent hunks, so they must be built in different waves. Outside this batch, the
  PLANNED bundles #508 and #625 list 656 in their `Depends on`. **Intake-cap override
  (wyrd-pdca-P1):** the human allowed all seven ids of this batch (656, 666, 671, 767, 768, 620,
  686) in over the alpha cap in the 2026-10-14 Plan session. Count at that moment: alpha 4/6
  planned (508, 625, 633, 654), room 2, need 7.
- **Scope:** `CreateMultipartUpload`'s admission and session creation (including recovery of an
  unknown commit outcome under the original upload id), and `AbortMultipartUpload`'s fence, as
  store-I/O functions in `wyrd-core` answering in the landed vocabulary; the shared fence batch
  builder in `wyrd-core`, and the one behaviour-preserving change to restore's `fence_session`
  (`crates/custodian/src/restore.rs:1045-1057`) to use it; the seeded DST scenario
  `crates/dst/tests/multipart_admission.rs`; the multipart module header and the
  living-architecture paragraph updated to say these writers now exist / out of scope:
  UploadPart and slot reservation (#657); Complete (#658); the retirement drain and the terminal
  delete that decrements `mpuctl.count` (#659); the reaper and operator abort (#625, #633); every
  gateway, S3, CLI or custodian **caller of Create or Abort** (#666–#670); the bucket
  deletion-fence key and its `require_absent` (#511, see Design); the `Completing → Aborting`
  edge, which a client Abort never takes (the table answers `409`); any edit to proposal 0016 or
  to any ADR.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_create_abort.rs` — a **NEW** file (C4-verify treats
  only an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new. The Create and Abort store functions do not exist on
  `main`, so the discriminating test cannot compile on C4-verify's reverted base, and C4-verify
  reports **UNVERIFIABLE** (exit 77). That is a pre-declared sign-off item, not a defect.
  **Built and exercised at Check:** the functions themselves, driven by the new test against a
  real store with real CAS contention (green), and the DST scenario (G) under C4-ci's
  `cargo xtask dst` sweep. **Demonstrated red (required):** Do runs leg (A′) once with the
  contention budget temporarily set to `MAX_UPLOAD_ID_ATTEMPTS` (the shared-budget defect) and
  shows it fail; (A′) forces the schedule, so this red does not depend on luck. Do runs leg (E)
  once with a temporary decrement on Abort and shows the count assertion fire. Do runs leg (F)
  once with a temporary re-mint on an unknown outcome and shows the one-session / one-increment
  assertion fire. All three failing outputs go into `build-notes.md`. The test-local store
  wrapper keeps its state in struct fields, never in a `static`: the ADR-0035 statics scan
  covers `crates/core` and `crates/dst` (`xtask/src/main.rs:1218-1258`).
- **Production reach:** (a) The functions are exercised for real by the test (a real
  `MetadataStore`, real preconditions, real contention). Production calls none of them in this
  slice. (b) The S3 verbs that call them land with #668 (508.3). That slice also carries 0016's
  normative requirement that the reaper (#625) and operator abort ship in the same release as any
  verb that creates sessions (`0016:236-268`). (c) So until #668, no deployment can create a
  session, and no session can sit in `Aborting` with no drain (#659 is the exit). This slice
  **must not** add a production caller of Create or Abort: no gateway, CLI or custodian wiring.
  (d) The one production path it does change is restore's existing fence, which switches to the
  shared batch builder with identical preconditions and puts.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer callsite for the Abort batch:** the restore fence already commits
  `SessionRecord::open_teardown`'s output (`crates/core/src/multipart.rs:2360-2372`) in
  `fence_session`, `crates/custodian/src/restore.rs:1045-1057`. It `require`s the exact session
  bytes it read, `require_absent`s each obligation key, and puts session and obligation in one
  batch. Its conflict classification (re-read once, never treat an unknown outcome as `Conflict`)
  is at `:1058-1089`. #656's first tracker comment asks that the client Abort and the restore
  fence share one fence **batch** builder. #841 built only the record half (`open_teardown`);
  the batch is still built inline at `:1045-1057`, so this slice moves it into core (Design,
  "The shared fence batch builder"). `open_teardown`'s
  `{session, all}` payload is also the shape the 2026-10-05 tracker comment requires, because a
  `{session, parts: <set>}` payload can exceed `MAX_VALUE_BYTES` for a sparse session. For
  admission: `MPUCTL_KEY` (`:1132`), `decode_admission_record` (`:1914`),
  `MAX_UPLOAD_ID_ATTEMPTS` (`:4983`), `MAX_ADMISSION_CAS_ATTEMPTS` (`:4997`),
  `admission_backoff_millis` (`:5023`). Bucket existence is the `bucket:` record at
  `metadata::bucket_key` (`crates/core/src/metadata.rs:52`); the reservation marker is
  `metadata::seggrp_key` (`:1532`).
- **Prior-art check (triage cycles):** searched by path `crates/core/src/multipart.rs`,
  `crates/custodian/src/restore.rs` and `docs/design/architecture/05-building-block-view.md`.
  Merged: `5eeca16` / `a3b2bbe` (the `Budget` / `AdmissionRecord` decode, #715), the
  open/completing teardown builders (#841, #842), and `b2d3922` (the architecture doc naming the
  restore fence as the first production writer, "the rest arrives with #656–#659"). Rejected:
  #508's seven monolithic attempts (the false-`503` measurement above came from one of them) and
  #636, closed not-planned 2026-10-10 as too large for one slice. Open PRs on getwyrd/wyrd: none.
  No PR on #656.
- **Disposition hint:** new-feature

Plan-review response: all four findings checked against `7bf7eb9` and revised in place.
(1) Deletion fence: Create now `require`s the exact marker bytes and refuses a marker moved
between read and commit (B); the fence's `require_absent` is deferred to #511 by name, because no
fence key exists in `crates/` and ADR-0046 decision 5 leaves it to #511 (human: confirm this
deferral, or say to define the key here). (2) Unknown outcomes: the original-id recovery contract
from `0016:2009-2027` is in Design, tested by (F). (3) Scheduler-dependent red: (A′) forces the
losing order and asserts a creator exceeded `MAX_UPLOAD_ID_ATTEMPTS`; (G) adds the seeded DST
scenario `AGENTS.md:188-190` requires. (4) Shared builder: the brief no longer claims #841 built
it; the batch construction moves into core and restore uses it. Difficulty raised to high.

## Motivation

Every later multipart slice needs a session to exist and to be tearable: UploadPart (#657)
reserves slots on an `Open` session, Complete (#658) fences one, the drain (#659) retires what an
Abort fenced, and the S3 verbs (#668) are a thin wire over these functions. The admission counter
is also the one thing that keeps every multipart namespace under `SCAN_CAP` (`0016:246-255`), so
it has to be exact from its first writer. #508's attempts showed the cost of getting its retry
shape wrong: one budget sized for a 2^-128 id collision answered a false `503 SlowDown` to
ordinary client concurrency. #655 already split the two budgets as constants
(`crates/core/src/multipart.rs:4977-5005`); this slice is where they are spent.

## Design

**Where.** New store-I/O functions in `wyrd-core`, beside the pure module rather than inside its
pure sections. Do picks the layout, and the module header's "deliberately pure" paragraph
(`crates/core/src/multipart.rs:5-7`, `:104-118`) is updated to name what now writes. Core stays
runtime-free (ADR-0009): the backoff wait, the clock, the `clock_source` and the randomness for
ids and nonces all come from the caller. `admission_backoff_millis` is already pure for exactly
this reason (`:5012-5023`). The functions take the store as `write.rs`'s functions do.

**Create (`0016:657`).** One batch per attempt:
- **Ledger present:** `require(mpuctl == prior bytes)` with `prior.count < prior.max_sessions`
  (the **stored** limit), and the local profile compared with `prior.profile` (a mismatch is
  `ProfileSkew`, no write).
- **Ledger absent:** `require_absent(mpuctl)` and a bootstrap
  `{count: 1, max_sessions, profile}` from the local `Budget`.
- **Either way:** `require_absent(mpu:<id>)`, `require_absent(seggrp:<nonce>)`,
  `require(bucket:<name> == the exact marker bytes read)` (ADR-0046 §4,
  `docs/design/adr/0046-bucket-model-real-namespace.md:93-95`: "`require` on the marker bytes"),
  put the session, put the marker, and put the ledger at `count + 1`.
- **The deletion-fence half of ADR-0046 §4 is deferred to #511, stated, not dropped.** §4 also
  asks for `require_absent` on the bucket's deletion fence (decision 5, `:104-121`). No fence key
  exists on `main`: decision 5 only sketches one (`bucket-deleting:{name}`) and leaves the choice
  to #511 (DeleteBucket, open), and nothing in `crates/` defines or writes one. So nothing on
  `main` can put a bucket into the deleting state, and this slice must not invent the key (that
  would decide #511's open question). Keep the bucket precondition in one place in the Create
  batch so #511 adds the fence's `require_absent` there. Do records this carry-forward in
  `build-notes.md`; until #511 lands, no deployment has a fence to honour, and none can create a
  session before #668 anyway.

On `Conflict`, re-read and classify **which** precondition lost:
- `mpuctl` moved → contention: back off per `admission_backoff_millis` and retry, up to
  `MAX_ADMISSION_CAS_ATTEMPTS`, then `SlowDown { AdmissionContention }`.
- `mpu:<id>` or `seggrp:<nonce>` taken by **another** session → collision: re-mint, up to
  `MAX_UPLOAD_ID_ATTEMPTS`. (A taken `mpu:<id>` that holds **this attempt's own** session is not
  a collision; see the next paragraph.)
- Bucket marker gone or changed → `NoSuchBucket`.

**An unknown commit outcome (`0016:2009-2027`).** A `CommitUnknownResult` is never read as
`Conflict` (the restore fence's rule) and never answered by minting a new id. With
`may_still_commit == true`, an absent re-read does not prove the batch will not land
(`crates/traits/src/lib.rs:240-247`), so erroring out or re-minting can strand an admitted
session that no client will ever name. The contract:
- the attempt's identity (upload id and group nonce) survives the unknown outcome;
- the re-drive uses that same identity, and rebuilds the `mpuctl` precondition from a fresh read
  (a replay of the identical batch can never commit once `mpuctl` has moved);
- on re-drive, a found `mpu:<id>` that is this attempt's own session (same nonce, bucket and
  object) means the earlier batch landed: answer `Created`, with no second increment;
- with `may_still_commit == false`, one re-read settles it.
The mutual exclusion comes from `require_absent(mpu:<id>)` alone, so at most one attempt lands.
Do chooses whether core runs the re-drive itself (with the caller's wait, as for backoff) or
returns an error that carries the identity plus a core entry point to re-drive it. Either way the
gateway (#668) must not have to rebuild the batch, and (F) must pass. The session record has no
free constructor on purpose (`crates/core/src/multipart.rs:2259-2263`).
Add the **one** creation route, and make it hold the identity `decode_session_record` enforces:
`encode(decode(bytes)) == bytes`, canonical field order, `content_type` omitted when absent. The
same goes for the ledger record's first and incremented values (`AdmissionRecord` has no free
constructor either). The initial epoch, attempt count and `clock_source` follow 0016 §1's
`mpu:` row.

**Abort (`0016:665`, `:977`).**
- Read `mpu:<id>`.
- `Open@E`: commit `open_teardown`'s session plus obligation in one batch, `require`-ing the
  exact bytes read and `require_absent`-ing the obligation key. No slot preconditions for a
  client Abort (`0016:665`). The batch comes from the **shared fence batch builder** below.
- **The shared fence batch builder (the tracker's "one batch builder, not two").** What #841
  shipped is a *record* builder: `open_teardown` returns the session and obligation
  (`crates/core/src/multipart.rs:2360-2372`, `:2426-2443`). The `WriteBatch` itself, with its
  preconditions, is still assembled inline in restore's `fence_session`
  (`crates/custodian/src/restore.rs:1045-1057`). So this slice moves that batch construction
  into `wyrd-core` as one builder: given the `mpu:` key, the exact bytes read, the fenced session
  and its obligations, it `require`s the bytes, `require_absent`s every obligation key, puts the
  session, and puts every obligation. It takes a list of obligations, because restore fences
  `Completing` sessions too (two obligations, `restore.rs:1118-1130`). The client Abort uses it,
  and `fence_session` is changed to call it in place of `:1045-1057`. That restore change is
  behaviour-preserving: the same preconditions and the same puts, and restore's existing tests
  pass unmodified. It is a refactor of an existing writer, not a new production caller of
  Create or Abort.
- `Aborting`: `AlreadyAborting`.
- `Completing`: `OperationAborted`.
- `Completed`, or no record: `NoSuchUpload`.
- On `Conflict`, re-read once and answer from the state found. A concurrent Abort that won leaves
  `Aborting` → `AlreadyAborting`; that is the "no 409 race". A session that moved to a later
  `Open@E'` is fenced again at `E'`.

The fence never touches `mpuctl`.

**Docs currency** (`AGENTS.md:154-158`, a merge requirement): the paragraph at
`docs/design/architecture/05-building-block-view.md:202` and `b2d3922`'s "rest arrives with
#656–#659" sentence say Create and Abort now write these records, with no production caller
until #668.

## Alternatives considered

- **One shared retry budget** — the #508 defect; rejected and already split by #655.
- **A per-session decrement on Abort** — releases a slot while the session still holds staged
  bytes, so admission would stop bounding the namespaces it exists to bound. The withdrawn
  "`Completed` releases its admission slot" criterion is unsatisfiable by a conforming
  implementation and must not be re-earned. Rejected (0016; the issue's corrected leg E).
- **A second Abort batch builder beside the restore fence's** — two writers of one edge drift
  apart; #656's first comment asks for one batch builder. Rejected.
- **Reusing only `open_teardown` and calling that "the shared builder"** — it shares the
  records but leaves two copies of the batch's preconditions (restore's and Abort's), which is
  exactly what drifts. Rejected; the batch construction moves into core.
- **Defining the bucket deletion-fence key here** so Create can `require_absent` it — decides
  #511's open choice from a multipart slice. Rejected; deferred to #511 as stated in Design.
- **`{session, parts: <set>}` as the Abort payload** — can exceed `MAX_VALUE_BYTES`
  (10,000 sparse parts = 128,916 bytes > 100,000), so FoundationDB refuses it permanently.
  Rejected; `{session, all}` is sound because no part commits outside `Open` (`0016:711-716`).

## Impact & compatibility

Additive: new functions and new record writes in namespaces already reserved and disjoint
(`mpuctl`, `mpu:`, `seggrp:`, `retire:`). No production path calls them, so no deployment's
behaviour changes. The records they write are the shapes the decoders already accept, so the
restore fence (`crates/custodian/src/restore.rs`) reads them unchanged.

## Open questions

1. The issue's acceptance names `cargo xtask ci` green; at Check that is the C4-ci gate,
   unchanged.
2. If writing a canonical session or ledger record needs a constructor the decoders cannot
   already round-trip, say so in `build-notes.md`. Do not loosen a decoder to fit.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Shape and scope are right; four reproduced bugs (plus one more) in crates/core/src/multipart/store.rs block accept. Fix each and keep a regression test for it: 1. lost()/owns() (store.rs:196-202, :439-446): accept "this is my own session" only while this invocation has an in-doubt commit outstanding; otherwise treat it as a collision and re-mint. Add a test for a same-object collision with the same id/nonce, and one for the same object with a different nonce (kills the surviving owns() && -> || mutants). 2. Create on budget exhaustion (store.rs:308-314): run the settling read before returning when the last attempt's outcome is unknown. Test an unknown outcome on the final attempt (it answers Created when the batch landed). 3. Abort (store.rs:570-605): read the final round's re-read through abort_answer before returning RoundsExhausted or the unknown error. Test 3 unapplied unknowns + an applied 4th, and a concurrent winning Abort on the last round (answers AlreadyAborting). 4. Contention budget (store.rs:310): derive it from the effective profile's max_sessions (Budget::max_sessions / the stored ledger), not the fixed 46. Test a profile above the shipped limit under the forced schedule. 5. Late Create fills the ledger between recovery reads (store.rs:366): re-check the original mpu:<id> before giving up on a capacity refusal during recovery. Add this ordering to the late-commit test. Also cover the other surviving mutants: a plain (non-unknown) store fault on an Abort commit (store.rs:594), and the backoff window actually growing (store.rs:317). Raise the DST abort strikes to reach the exhaustion path. The "metadata awaits lack a timeout" review finding is rejected: core is runtime-free, and the caller (#668) owns timeouts. C4-ci timed out in the unrelated custodian_gc tests; it needs a clean full run.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Shape and scope are right; four reproduced bugs (plus one more) in crates/core/src/multipart/store.rs block accept. Fix each and keep a regression test for it:
  1. lost()/owns() (store.rs:196-202, :439-446): accept "this is my own session" only while this invocation has an in-doubt commit outstanding; otherwise treat it as a collision and re-mint. Add a test for a same-object collision with the same id/nonce, and one for the same object with a different nonce (kills the surviving owns() && -> || mutants).
  2. Create on budget exhaustion (store.rs:308-314): run the settling read before returning when the last attempt's outcome is unknown. Test an unknown outcome on the final attempt (it answers Created when the batch landed).
  3. Abort (store.rs:570-605): read the final round's re-read through abort_answer before returning RoundsExhausted or the unknown error. Test 3 unapplied unknowns + an applied 4th, and a concurrent winning Abort on the last round (answers AlreadyAborting).
  4. Contention budget (store.rs:310): derive it from the effective profile's max_sessions (Budget::max_sessions / the stored ledger), not the fixed 46. Test a profile above the shipped limit under the forced schedule.
  5. Late Create fills the ledger between recovery reads (store.rs:366): re-check the original mpu:<id> before giving up on a capacity refusal during recovery. Add this ordering to the late-commit test.
  Also cover the other surviving mutants: a plain (non-unknown) store fault on an Abort commit (store.rs:594), and the backoff window actually growing (store.rs:317). Raise the DST abort strikes to reach the exhaustion path.
  The "metadata awaits lack a timeout" review finding is rejected: core is runtime-free, and the caller (#668) owns timeouts.
  C4-ci timed out in the unrelated custodian_gc tests; it needs a clean full run.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 45 mutants tested in 3m: 5 missed, 29 caught, 11 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Shape, scope and slice size are right (most of the 172 KB patch is tests); round 1's five bugs are fixed. Two reproduced bugs in crates/core/src/multipart/store.rs still block accept. Fix each and keep a regression test for it: 1. Transition-size headroom (store.rs:493): Create admits an Open record up to MAX_VALUE_BYTES, but its Aborting@1 replacement is 4 bytes larger, so Abort and the restore fence fail forever and the slot leaks. Complete's publish_target needs more again. Reject at Create, before writing anything, any request whose encoded session cannot fit every later state (Aborting, Completing) under MAX_VALUE_BYTES. Add a boundary test through a size-enforcing store wrapper, measured on encoded bytes, not character length. 2. Ownership under an unknown outcome (store.rs:196-206, :374, :400-407): owns() compares only nonce, parent and object, so a repeated id+nonce whose first commit returns an unapplied CommitUnknownResult is handed the existing upload's id as Created, with no count increment. Make owns() also compare created_at_millis, clock_source and content_type (or read mpu:<id> before a fresh identity's first commit). Add a collision-plus-unknown-outcome regression. Also fix two doc wordings: multipart.rs:5078-5079 (the contention wait bound now scales with the profile's max_sessions) and 05-building-block-view.md:202 (a contention refusal can come from repeated unknown outcomes, not only a full ledger). C4-ci's cargo-deny failure (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client) is inherited and outside this slice; do not touch Cargo.lock or deny.toml here.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Shape, scope and slice size are right (most of the 172 KB patch is tests); round 1's five bugs are fixed. Two reproduced bugs in crates/core/src/multipart/store.rs still block accept. Fix each and keep a regression test for it:
  1. Transition-size headroom (store.rs:493): Create admits an Open record up to MAX_VALUE_BYTES, but its Aborting@1 replacement is 4 bytes larger, so Abort and the restore fence fail forever and the slot leaks. Complete's publish_target needs more again. Reject at Create, before writing anything, any request whose encoded session cannot fit every later state (Aborting, Completing) under MAX_VALUE_BYTES. Add a boundary test through a size-enforcing store wrapper, measured on encoded bytes, not character length.
  2. Ownership under an unknown outcome (store.rs:196-206, :374, :400-407): owns() compares only nonce, parent and object, so a repeated id+nonce whose first commit returns an unapplied CommitUnknownResult is handed the existing upload's id as Created, with no count increment. Make owns() also compare created_at_millis, clock_source and content_type (or read mpu:<id> before a fresh identity's first commit). Add a collision-plus-unknown-outcome regression.
  Also fix two doc wordings: multipart.rs:5078-5079 (the contention wait bound now scales with the profile's max_sessions) and 05-building-block-view.md:202 (a contention refusal can come from repeated unknown outcomes, not only a full ledger).
  C4-ci's cargo-deny failure (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client) is inherited and outside this slice; do not touch Cargo.lock or deny.toml here.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Shape, scope and slice size are right (round 1 ruling stands; the 202 KB size backstop is not a slicing signal here). One reproduced bug and two gaps block accept: 1. R1 — Abort drops in-flight commit ambiguity (crates/core/src/multipart/store.rs:749-753). Once a CommitUnknownResult with may_still_commit: true is seen, keep it across later unknown outcomes with may_still_commit: false, a failing recovery read, and a later plain store fault, while the session is unchanged — the same rule Create already applies at store.rs:541-545. Conflict (:747, session bytes moved) may still clear it. Add mixed 1031 -> 1021 regressions and a subsequent-plain-fault regression that release the held first batch AFTER Abort returns and assert the returned error still says may_still_commit: true. 2. Create's keep-the-doubt rule (store.rs:541-545) has no test: replacing it with an unconditional assignment passes all 29 tests. Add the mixed-code Create regression (tiny profile, commit 1 Unknown::Late, commit 2 Dropped{may_still_commit:false}; must answer Unsettled, not a contention refusal). Consider making the DST scenario check the may_still_commit flag (crates/dst/tests/multipart_admission.rs:172) and mix ambiguity codes within a run. 3. Rustdoc at store.rs:291-296 and :419-421 claims a contention refusal is never a lost race / always a full ledger; an unknown outcome also spends that budget (:395-397). Fix the wording to match 05-building-block-view.md:202. Doc only, no behaviour change. C4-ci's cargo-deny failure (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client) is inherited from main; the waiver goes via Act. Do not touch Cargo.lock or deny*.toml.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Shape, scope and slice size are right (round 1 ruling stands; the 202 KB size backstop is not a slicing signal here). One reproduced bug and two gaps block accept:
  1. R1 — Abort drops in-flight commit ambiguity (crates/core/src/multipart/store.rs:749-753). Once a CommitUnknownResult with may_still_commit: true is seen, keep it across later unknown outcomes with may_still_commit: false, a failing recovery read, and a later plain store fault, while the session is unchanged — the same rule Create already applies at store.rs:541-545. Conflict (:747, session bytes moved) may still clear it. Add mixed 1031 -> 1021 regressions and a subsequent-plain-fault regression that release the held first batch AFTER Abort returns and assert the returned error still says may_still_commit: true.
  2. Create's keep-the-doubt rule (store.rs:541-545) has no test: replacing it with an unconditional assignment passes all 29 tests. Add the mixed-code Create regression (tiny profile, commit 1 Unknown::Late, commit 2 Dropped{may_still_commit:false}; must answer Unsettled, not a contention refusal). Consider making the DST scenario check the may_still_commit flag (crates/dst/tests/multipart_admission.rs:172) and mix ambiguity codes within a run.
  3. Rustdoc at store.rs:291-296 and :419-421 claims a contention refusal is never a lost race / always a full ledger; an unknown outcome also spends that budget (:395-397). Fix the wording to match 05-building-block-view.md:202. Doc only, no behaviour change.
  C4-ci's cargo-deny failure (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client) is inherited from main; the waiver goes via Act. Do not touch Cargo.lock or deny*.toml.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized and growing every round (136 → 173 → 207 → 238 KB vs 100 KB backstop); split it in Plan (`pdca split 656`), e.g. Create admission and Abort fence as separate slices. - Carry into the Abort slice: on Conflict with an unchanged session, read the obligation key before reporting ObligationKeyTaken; if absent, treat as an unexplained lost race — keep doubt and spend a round, or end with the held CommitUnknownResult when may_still_commit (mirror restore.rs:1065-1075). Add both reviewer probes (spurious single Conflict; held round-1 batch + Conflict) as regressions, with a store wrapper that can return Conflict without applying. - DST test gap (manual review-branch): multipart_admission.rs:158 keys fault histories by session+operation, merging the two concurrent Abort calls; track each invocation separately. - What held up and can carry over: Create admission/recovery, the shared fence batch, restore refactor, encoded-size headroom.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Slice is oversized and growing every round (136 → 173 → 207 → 238 KB vs 100 KB backstop); split it in Plan (`pdca split 656`), e.g. Create admission and Abort fence as separate slices.
  - Carry into the Abort slice: on Conflict with an unchanged session, read the obligation key before reporting ObligationKeyTaken; if absent, treat as an unexplained lost race — keep doubt and spend a round, or end with the held CommitUnknownResult when may_still_commit (mirror restore.rs:1065-1075). Add both reviewer probes (spurious single Conflict; held round-1 batch + Conflict) as regressions, with a store wrapper that can return Conflict without applying.
  - DST test gap (manual review-branch): multipart_admission.rs:158 keys fault histories by session+operation, merging the two concurrent Abort calls; track each invocation separately.
  - What held up and can carry over: Create admission/recovery, the shared fence batch, restore refactor, encoded-size headroom.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
