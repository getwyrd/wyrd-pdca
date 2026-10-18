<!-- pdca:split-proposal v1 -->
# Split proposal — issue 656

**Intake-cap override (wyrd-pdca-P1), recorded 2026-10-15 in the Plan session.**
`scripts/plan-cap --track alpha --need 2` printed:
`planned 7/6 (cap, track alpha) — room for 0, need 2: Plan intake closed`
(PLANNED 508 620 625 633 671 768; BUILT 654). The human (Eduard Ralph) allowed **both
children of this split** in over the alpha cap ("please start the split"). The override covers
these two children only. Accepting them takes alpha to 9/6.

**Inherited gate failure, ruled out of scope by the human in the same session:** `cargo xtask ci`
fails on `main` (`7bf7eb9`) at `cargo deny --all-features --config deny-all-features.toml check
advisories` (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client). Neither child fixes it, and
it does not count against either. Each brief says how to get the evidence that step hides.

## Why this slice is oversized

#656 built Create's admission and Abort's fence as one slice, and four attempts did not
converge. The patch grew every round: 136 → 173 → 207 → 238 KB, against a 100 KB backstop. At
the 2026-10-14 sign-off the human sent it back to Plan to be split, "e.g. Create admission and
Abort fence as separate slices".

The seam is real. Create and Abort are two verbs over two different records:

- **Create** writes `mpuctl`, `mpu:` and `seggrp:`, and its hard part is the admission CAS and
  recovering an unknown commit outcome under the original upload id.
- **Abort** writes `mpu:` and `retire:bytes:`, and its hard part is classifying a lost commit and
  keeping the doubt of a fence that may still land.

They share no code path, except that Abort needs a session to exist. Each can ship as its own PR
with its own test, and each has its own success criterion. In attempt 4 the reviewers found
Create, the size headroom, the shared fence batch and the restore refactor sound. The one
blocking bug was in Abort's conflict branch.

Measured on attempt 4's patch: Create's half was about 160 KB (the store code, the record
constructors and size headroom in `multipart.rs`, and about 100 KB of tests and DST). Abort's half
was about 90 KB. **The Create child may still trip the 100 KB size warning**: most of its bytes are
tests, and every test leg in it was required by an earlier sign-off. A third cut, separating
unknown-outcome recovery from admission, is possible but was not chosen. Recovery is part of
0016's Create contract (`0016:2009-2027`), and each extra child costs a full cycle.

## Wave sketch

Two waves, strictly ordered.

- **child-1 (Create) first.** It makes the store module and is the only way to make a session.
- **child-2 (Abort) after it**, with `Depends on: child-1`. Abort's tests and DST scenario need
  `Open` sessions, and `SessionRecord` has no free constructor
  (`crates/core/src/multipart.rs:2259-2263`), so only child-1's Create can make them in a test
  that goes through production code. child-2 also adds to the store module child-1 creates, and
  both edit the multipart module header and
  `docs/design/architecture/05-building-block-view.md:202`. The dependency already puts them in
  different waves, so no `Conflicts with` edge between them is needed.

**One edge outside this proposal.** Both children update the multipart paragraph at
`05-building-block-view.md:202`, and #768 (PLANNED) edits `:197-200` of the same file. #768's
brief already declares `Conflicts with: 656`. A proposal's ordering fields can only name sibling
children, so after `--accept` the planner adds `768` to both children's `Conflicts with`, and the
child ids to #768's. #508 and #625 (`Depends on: … 656 …`) get the same re-pointing. Those are
field edits to existing briefs.

<!-- pdca:child child-1 -->
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
  - An identity source that repeats an **existing** session's identity (same upload id and
    nonce, for the same bucket and object) gets a re-mint, never that session as `Created`. This
    holds both when the collision is found before the first commit and when that first commit's
    outcome is unknown and **not** applied.
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
  **(G) Seeded coverage** (`AGENTS.md:188-190`: a new concurrent path lands with seeded Tier-0
  DST coverage). A NEW madsim scenario `crates/dst/tests/multipart_admission.rs` runs concurrent
  Creates on `SimFdbMetadataStore` with the commit-ambiguity nemesis armed
  (`crates/dst/tests/support/mod.rs:696`, `:727` `quiesce`). It has three legs: 1021 alone, 1031
  alone, and a mixed leg where each commit's code is a seed-derived coin, so one writer can meet a
  1031 and then a 1021. After every seed, on the store after `quiesce`:
  - the number of `mpu:` records equals `mpuctl.count()`;
  - every `mpu:` record has its `seggrp:` marker, and no marker is without its session;
  - every `mpu:` record belongs to a create that answered `Created` for it, or to one whose
    in-doubt error named that upload id. None is stranded.
  The mixed leg must observe at least one writer meeting 1031 and then 1021 across the sweep.
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
- **Conflicts with:**
- **Ordering note:** Wave before its sibling, the Abort slice, which depends on this one.
  Prerequisites #654 (record family) and #655 (knobs) are closed and on `main`. **Planner adds
  `Conflicts with: 768` after `--accept`**: both edit
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
  - "this is my own session" accepted with no in-doubt commit outstanding → (C)'s repeated
    identity leg fails;
  - the kept doubt replaced by the latest outcome → F6 fails.
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

**Adopting a session is narrow.** A Create may answer `Created` for an upload id only when the
`mpu:` record under it is the one this invocation's **own** in-doubt batch wrote. A record found
under a freshly minted id before this invocation has a batch in doubt for it is a collision,
whatever it contains. The record must be the same in every field the batch wrote: comparing
nonce, bucket and object alone handed one client another's upload in attempt 2. Do picks the
mechanism, for example a read of `mpu:<id>` before a fresh identity's first commit, or a
whole-record comparison.

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
- **Answering `Created` for any found record with this attempt's nonce, bucket and object.** It
  adopts another client's session. Rejected (see "Adopting a session is narrow").
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core: multipart Abort fence + shared fence batch (636.3b)

> The Plan artifact for slice 3b of #636. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. Split from #656 at the 2026-10-15 re-plan.
> It builds on the sibling Create slice (636.3a), whose Create function makes the sessions this
> slice fences. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read the **Abort / reap
> fence** row (`0016:665`) and decision 3's verb × state table (`0016:973-979`, the Abort row
> `:977`) before writing code. Citations verified against `getwyrd/wyrd` `origin/main` @
> `7bf7eb9` on 2026-10-15. The sibling's store module is not on `main` yet; build on the base
> this bundle is given.

- **Slug:** multipart-abort-fence
- **Track:** alpha
- **Goal:** Nothing but the post-restore pass fences a multipart session today. Restore's
  `fence_session` builds its fence batch inline (`crates/custodian/src/restore.rs:1045-1057`).
  This slice adds `AbortMultipartUpload`'s teardown fence as a `wyrd-core` function over
  `MetadataStore`, answering in `AbortOutcome` (`crates/core/src/multipart.rs:4440`) and
  `Refusal` (`:4302`). It also moves the fence **batch** construction into one core builder that
  both the client Abort and restore use. That is the "one batch builder, not two" #656's first
  tracker comment (2026-09-12) asks for.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_abort.rs`, over
  `RedbMetadataStore::in_memory()`, shows all of the following. Every `Open` session in it is
  made by the sibling slice's Create. A `Completing` or `Completed` session is planted as
  canonical bytes the decoder accepts.
  **(D) Abort per decision 3 (`0016:977`).**
  - On `Open@E` it answers `Fenced`. Afterwards the session decodes as `Aborting@E+1`, and
    `retire:bytes:s:<id>:<E>` holds exactly the `{session, all}` obligation
    `SessionRecord::open_teardown` mints (`multipart.rs:2360-2372`), committed in the **same**
    batch as the fence.
  - A second Abort answers `AlreadyAborting`.
  - **Two Aborts forced to race one `Open` session** through a test-local wrapper answer one
    `Fenced` and one `AlreadyAborting`, never `OperationAborted`.
  - Abort of a `Completing` session answers `Refused(OperationAborted)` and leaves the store
    byte-identical.
  - Abort of a `Completed` session, or of an upload id with no record, answers
    `Refused(NoSuchUpload)`.
  - An unknown outcome on the **last** round whose batch landed answers `Fenced`, and a rival
    Abort that wins on the last round answers `AlreadyAborting`: the final round's re-read is
    answered like any other.
  - A plain (not unknown) store fault on a fence commit, with nothing in doubt, is returned as
    that fault.
  **(E) Abort never moves the counter.** `mpuctl.count()` is identical before and after every
  leg of this file, and a `Completed` tombstone stays counted. The decrement belongs only to the
  terminal delete (#659).
  **(H) A lost commit is classified from what the store holds, not inferred.** Through a store
  wrapper that can answer `Ok(CommitOutcome::Conflict)` **without applying anything** (what
  TiKV's adapter returns when a locking `get_for_update` fails on another writer's lock,
  `crates/metadata-tikv/src/lib.rs:1388-1393`, `:928-939`):
  - **H1:** one such Conflict, with the session unchanged and the obligation key absent. Abort
    does **not** report the obligation key taken. It spends a round and answers `Fenced`.
  - **H2:** round 1 returns `CommitUnknownResult { may_still_commit: true }` with its batch held,
    and every later round gets such a Conflict until the rounds are spent. Abort ends with an
    error that still says `may_still_commit: true`. Released **after** Abort has returned, the
    held batch lands, and the session is `Aborting` with its one obligation.
  - **H3:** with the obligation key really present (planted) and the session unchanged, Abort
    ends with a typed fault naming that key and writes nothing.
  **(I) The doubt of a fence that may still land is kept.** Once a fence returns
  `CommitUnknownResult { may_still_commit: true }`, Abort keeps that doubt across a later
  unknown with `may_still_commit: false`, a failed re-read, and a later plain store fault, for
  as long as the session is unchanged. Each case releases the held first batch **after** Abort
  has returned. It asserts that the returned error still says `may_still_commit: true` and that
  the batch lands. Only a read that finds the session **moved** clears the doubt. A session moved
  under a held fence ends without that doubt, and the released batch loses.
  **(J) Restore is unchanged.** `fence_session` uses the shared builder and puts the same
  preconditions and records in the same order. Restore's existing tests pass unmodified: the
  `wyrd-custodian` tests under `cargo test`, and the custodian DST file under `cargo xtask dst`.
  **(G) Seeded coverage** (`AGENTS.md:188-190`). A NEW madsim scenario
  `crates/dst/tests/multipart_abort.rs` runs concurrent Creates (the sibling's) and concurrent
  Aborts, including two Aborts of one session, on `SimFdbMetadataStore` with the commit-ambiguity
  nemesis armed (1021, 1031, and a mixed leg drawing each commit's code from a seed-derived
  coin). The nemesis strikes often enough that some Aborts spend all their rounds. Fault
  histories are tracked **per Abort invocation**, never keyed by session and operation, which
  would merge two concurrent Aborts of one session. After every seed, on the store after
  `quiesce`:
  - the number of `mpu:` records equals `mpuctl.count()`;
  - every `Aborting` session has its `retire:bytes:` `{session, all}` obligation;
  - a session none of whose Aborts answered `Fenced` or `AlreadyAborting`, and none of whose
    Aborts ended with an error saying a fence may still land, is still `Open`; one an Abort
    answered `Fenced` for is `Aborting`.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  and every red leg is forced by a wrapper rather than left to the scheduler. (H)'s wrapper
  returns the adapter's conflict on demand. redb alone never produces it, because redb reports
  Conflict only for a failed precondition (`crates/metadata-redb/src/lib.rs:212-222`); so the
  wrapper is what makes the attempt-4 defect fail on every run. (I)'s wrapper holds a batch in
  flight past the Abort's return, which is the case the defect lies about. (D)'s race is forced
  the same way. (G) is invariant coverage, not the red for (H) or (I). In attempt 4 the sim
  landed in-flight batches on a fair coin at nearly every store call, so the window rarely
  opened. The deterministic core tests are the regression for (H) and (I). (G) is madsim-only,
  and C4-ci cannot reach it today (see `Verification posture`).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*). Every Abort that fences a session installs, **in the same batch**,
  an obligation naming everything the session protects. No answer or error lets its caller
  take the fence for one that installed nothing while a fence of it may still land. The
  admission count never changes on a fence. So no fenced session is ever outside the reach of
  the actor that reclaims it (#659's drain), and every writer of the `Open → Aborting` edge
  builds the same batch. **Sources:** the maintainer's rule 2026-07-25 (via principles.md §5);
  0016's refutation standard (`0016:2813`); the batch row `0016:665`. SELF-TEST: it binds two
  writers (client Abort and restore), a batch and a caller-visible answer, so guarding a single
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** `Depends on child-1`: Abort's tests and DST need `Open` sessions, and only
  the sibling's Create can make one through production code (`SessionRecord` has no free
  constructor, `crates/core/src/multipart.rs:2259-2263`). This slice also adds to the store
  module the sibling creates. **Planner adds `Conflicts with: 768` after `--accept`**: both edit
  `docs/design/architecture/05-building-block-view.md` (`:202` here, `:197-200` in #768).
  **Intake-cap override (wyrd-pdca-P1):** the human allowed both children of #656's split in
  over the alpha cap in the 2026-10-15 Plan session; count then 7/6, room 0, need 2.
- **Scope:** `AbortMultipartUpload`'s fence as a store-I/O function in `wyrd-core` answering in
  the landed vocabulary, including how it classifies a lost or unknown commit. Also in scope:
  - the shared fence batch builder in `wyrd-core`;
  - the one behaviour-preserving change to restore's `fence_session`
    (`crates/custodian/src/restore.rs:1045-1057`) to use it;
  - the seeded DST scenario `crates/dst/tests/multipart_abort.rs`;
  - the module header and living-architecture sentence for Abort.
  / out of scope:
  - Create and admission (the sibling slice, 636.3a);
  - restore's own conflict classification (`restore.rs:1067-1083`): only its batch construction
    is shared;
  - the `Completing → Aborting` edge, which a client Abort never takes (the table answers `409`);
  - the reaper and operator abort, including their `Completing → Aborting` rows (#625, #633);
  - the retirement drain and terminal delete (#659);
  - every gateway, S3, CLI or custodian **caller** of Abort (#668);
  - any edit to proposal 0016 or to any ADR;
  - `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). Today no client Abort exists, and the only
  fence batch is the inline one at `crates/custodian/src/restore.rs:1045-1057`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_abort.rs`, a **NEW** file (C4-verify treats an
  added `crates/*/tests/*.rs` as the per-fix discriminator). The DST scenario
  `crates/dst/tests/multipart_abort.rs` is also NEW.
- **Verification posture:** (a) net-new. The Abort function does not exist on this bundle's base,
  which is `main` plus the sibling's accepted patch. So the discriminating test cannot compile
  on C4-verify's reverted base, and C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared
  sign-off item.
  **Built and exercised at Check:** the Abort function and the shared builder, driven by the new
  test over a real store. Restore's fence goes through the builder under restore's existing
  tests.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - an `mpuctl` decrement added to the fence batch → (E) fails;
  - an unchanged session after `Conflict` taken as proof the obligation key is taken, without
    reading it (attempt 4's rule) → H1 and H2 fail;
  - the held doubt replaced by the latest outcome → (I) fails.
  **The inherited deny failure (human's ruling, 2026-10-15).** `cargo xtask ci` stops at the
  all-features cargo-deny step (RUSTSEC-2026-0346/0349) before its conformance, statics,
  orchestrator-guard and DST steps (`xtask/src/main.rs:1683-1687`). Do does not touch
  `Cargo.lock` or `deny*.toml`. Do runs `cargo xtask conformance`, `cargo xtask statics` and
  `cargo xtask dst` on its own and puts each command's result in `build-notes.md`. `cargo xtask
  dst` also covers the custodian DST file that exercises restore's fence (J). Test-local
  wrappers keep state in struct fields, never in a `static` (`xtask/src/main.rs:1221-1242`).
- **Production reach:**
  - (a) The test exercises Abort for real against a real store. Production calls no client
    Abort in this slice.
  - (b) The S3 verb lands with #668, under 0016's same-release rule for the reaper and operator
    abort (`0016:236-268`).
  - (c) This slice **must not** add a production caller of Abort.
  - (d) The one production path it changes is restore's existing fence, which switches to the
    shared builder with identical preconditions and puts.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer callsite to mirror:** restore's `fence_session`, `crates/custodian/src/restore.rs:1027-1089`.
    Its batch (`:1045-1057`) `require`s the exact session bytes it read, `require_absent`s each
    obligation key, and puts session and obligations in one batch. Its `Conflict` branch
    (`:1067-1083`) re-reads the session, and when the session is unchanged it **reads each
    obligation key** before naming one taken. A conflict that a re-read cannot explain is
    `SessionUnsettled::LostConflict` (`:324`), not damage. Its `Err` arm (`:1085-1089`) never
    reads an unknown outcome as `Conflict`. The client Abort must classify the same way. It
    differs only where the client contract needs more: it retries an unexplained conflict
    within its rounds, and it keeps a doubt (see Design).
  - **Records:** `SessionRecord::open_teardown` (`crates/core/src/multipart.rs:2360-2372`) and
    `completing_teardown` (`:2386-2399`) mint the session and obligations.
    `decode_retire_obligation` (`:3710`) is the reader every obligation must satisfy at its key.
  - **Answers:** `AbortOutcome` (`:4440`), `Refusal` (`:4302`), and `MAX_COMPLETE_ATTEMPTS`
    (`:4975`) if Do sizes the round budget from it.
- **Prior-art check (triage cycles):** searched by path `crates/custodian/src/restore.rs`,
  `crates/core/src/multipart.rs`, `crates/core/tests/multipart_abort.rs`,
  `crates/dst/tests/multipart_abort.rs` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main`. Merged: `8d15209` (the
  open-session restore fence, #841), `c0dacf3` / PR #856 (the completing-session restore fence,
  #842), `39bf35a` (the fence-completion record). #664 (637.4, restore fence) is closed. Neither
  new file exists. Rejected: #656's four attempts at Create and Abort together (this split).
  Attempt 4's only blocking finding was this slice's conflict branch, now leg (H). #636 was closed
  not-planned 2026-10-10. Open PRs on getwyrd/wyrd touching these paths: none.
- **Disposition hint:** new-feature

## Design

**Abort (`0016:665`, `:977`).** Read `mpu:<id>` and answer from its state:
- `Open@E`: commit `open_teardown`'s session and obligation in one batch, built by the shared
  builder. The batch `require`s the exact bytes read and `require_absent`s the obligation key.
  A client Abort carries no slot preconditions (`0016:665`).
- `Aborting`: `AlreadyAborting`.
- `Completing`: `OperationAborted`, with nothing written.
- `Completed`, or no record: `NoSuchUpload`.
The fence never touches `mpuctl`.

**Classifying a commit that did not say `Committed`.** Abort runs a bounded number of rounds,
and each round re-reads and answers from what it finds:
- **`Conflict` and the session moved:** answer from the new state. A rival Abort that won leaves
  `Aborting`, so the answer is `AlreadyAborting`: that is the "no 409 race". A session at a later
  `Open@E'` is fenced again at `E'`.
- **`Conflict` and the session unchanged:** read the obligation key. Present: a typed fault
  naming it (a token is minted once, `0016:358-373`, so a taken key is damage a retry cannot
  settle). **Absent:** the conflict is unexplained (TiKV's lock conflict,
  `crates/metadata-tikv/src/lib.rs:1388-1393`). Spend a round and try again, keeping any doubt.
- **`CommitUnknownResult`:** never read as `Conflict`. Re-read. A fence that landed shows as
  `Aborting` and is answered as `Fenced`.
- **The doubt is kept.** Once a fence of this Abort may still land (`may_still_commit: true`),
  keep that doubt while the session is unchanged: across later out-of-flight unknowns, a failed
  re-read, a later plain store fault, and unexplained conflicts. Only a read that finds the
  session moved clears it, because every fence requires the exact bytes it read and the epoch
  only rises. When the rounds run out, or a fault ends the Abort, return the held
  `CommitUnknownResult` if one may still land. Otherwise return a typed error saying no fence of
  this Abort can land.

**The shared fence batch builder.** What #841 shipped is a *record* builder: `open_teardown`
returns the session and obligation. The `WriteBatch` with its preconditions is still built inline
in restore's `fence_session`. Move that batch construction into `wyrd-core` as one builder: given
the `mpu:` key, the exact bytes read, the fenced session and its obligations, it `require`s the
bytes, `require_absent`s every obligation key, puts the session, and puts every obligation. It
takes a list of obligations, because restore also fences `Completing` sessions (two obligations,
`restore.rs:1124-1131`). It writes only obligations that `decode_retire_obligation` accepts at
the key they go under (the 2026-09-12 tracker comment's rule for any first writer).
`fence_session` calls it in place of `:1045-1057`, keeping the same preconditions, the same puts
and the same order. Restore's tests pass unmodified. This is a refactor of an existing writer,
not a new production caller.

**The `{session, all}` payload.** Abort uses `open_teardown`'s `{session, all}`, never a
`{session, parts: <set>}` list. The list form can exceed `MAX_VALUE_BYTES` for a sparse session:
10,000 sparse parts is 128,916 bytes against a 100,000 ceiling, and FoundationDB refuses that
permanently (the 2026-10-05 tracker comment). `all` is sound because no part commits outside
`Open` (`0016:712-717`).

**Docs currency** (`AGENTS.md:154-157`): add Abort to the paragraph at
`docs/design/architecture/05-building-block-view.md:202`. Say that Abort fences an `Open` session
to `Aborting` with its retirement obligation in the same batch, that the client Abort and the
restore fence build that batch in one place, that the fence never moves the admission count, and
that an Abort which ends while a fence may still land ends with that unknown outcome. Note there
is no production caller until #668.

## Alternatives considered

- **A per-session decrement on Abort.** It releases a slot while the session still holds staged
  bytes, so admission would stop bounding the namespaces it exists to bound. The withdrawn
  "`Completed` releases its admission slot" criterion is unsatisfiable by a conforming
  implementation and must not be re-earned. Rejected.
- **A second fence batch builder beside restore's.** Two writers of one edge drift apart.
  Rejected.
- **Reusing only `open_teardown` and calling that "the shared builder".** It shares the records
  but leaves two copies of the batch preconditions. Rejected.
- **Inferring a taken obligation key from an unchanged session.** This held for redb and the
  sim-FDB model, but not for TiKV, and it discarded a fence still able to land (attempt 4's
  blocking finding). Rejected; read the key.
- **A settling look before returning, as Create does.** A fence released after Abort returns
  cannot be found by a look taken before it returns. Abort's contract is to return the doubt
  instead. Rejected.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-2 -->
