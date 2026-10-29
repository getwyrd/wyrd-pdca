# core: staged segment publication committer (635.6) — issue 653

> The Plan artifact for slice 6 of #635. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 7(b)–(d)**
> (`0016:2342-2401`), its failure-mode table (`0016:2490-2503`), and the **Segment write** and
> **Root flip** rows of the batch inventory (`0016:662-663`) before writing code. Citations
> verified against `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-19.
>
> **Why this slice exists, and why now.** #647 (the first attempt at #635) was closed partly
> for shipping apparatus no production path called. This committer has exactly one producer,
> the multipart Complete slice #658, which is in the same batch and builds in the next wave on
> top of this slice's result. So this slice still ships no production caller of its own; #658
> is that caller, in the same run.

- **Slug:** segmented-staged-publication
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** A published chunk map larger than `MAX_MAP_CHUNKS` (158 chunks,
  `crates/core/src/multipart.rs:4959`) cannot fit in one metadata value, so it is published in
  two phases: write its `seg:<nonce>:<epoch>:<i>` records in bounded batches, then flip the
  inode root in **one** batch (`0016:2342-2355`). The record shapes, key helpers and the shared
  resolver exist (`SegmentedMap::new` `crates/core/src/metadata.rs:1097`, `SegmentRecord::new`
  `:1344`, `seg_key` `:1481`, `resolve_chunk_map` `:3218`), but nothing writes a segmented map.
  This slice adds the **committer**: a `wyrd-core` function over `MetadataStore` that publishes a
  chunk list as a flat or segmented root under a fence the caller supplies. It knows nothing
  about multipart sessions: the caller passes the fence record and, per batch, the fence
  contribution (for #658 that is `require(mpu == Completing@E)` plus the `segments_written`
  cursor move, `0016:613-620`).
- **Success criterion:** A NEW test file `crates/core/tests/segmented_publication.rs`, over
  `RedbMetadataStore::in_memory()` with a test-local recording wrapper that logs every batch the
  store receives, and a test-local fence record (any key; the committer must not care what it
  is), shows all of the following.
  **(A) Round trip.** A chunk list of more than `MAX_MAP_CHUNKS` chunks (at least three segments'
  worth) publishes, both on a fresh key and superseding an existing flat generation. Afterwards
  `resolve_chunk_map` returns the identical ordered chunk list, the root names the group
  `(nonce, E)` it was given, and the root's version is 1 (fresh) or `prior.version + 1`
  (supersede). A chunk list of at most `MAX_MAP_CHUNKS` publishes as a **flat** root with no
  `seg:` record written.
  **(B) Bounded batches.** With a byte budget set small enough to force at least three
  segment-write batches, every batch the wrapper logged stays within the byte budget measured in
  **encoded bytes** of its mutations, and within the operations budget; the root is written in
  exactly **one** logged commit, which comes after every segment commit.
  **(C) Refusals precede durability.** Each deterministic refusal the committer can make without
  reading the store (a plan needing more than `MAX_ROOT_SEGMENTS` segments, `metadata.rs:544`; a
  flip batch over the byte or operations envelope; a fence contribution that does not pin the
  fence record; a flip that does not move it; a fence sequence that returns to a value it already
  left, see (F); **a caller contribution, in any segment batch or in the flip, that puts, deletes
  or preconditions a key the committer owns**: the target inode, the target dirent, or any key in
  the group's `seg:<nonce>:<epoch>:` range, e.g. a flip contribution that deletes
  `seg:<nonce>:<E>:0`) returns its typed error with **zero** commits logged and the store unchanged.
  **(D) A resume verifies the whole range.** (D1) With a publication's segments planted durable up
  to some index, then one record **below** the caller's claimed `resume_from` deleted (or rewritten
  with different bytes): the publish is refused before the flip, and no root names the group.
  (D2) With `resume_from = r > 0` (records `0..r` planted durable and identical), the committer
  writes `r..segment_count`; right after the **last** segment commit returns, the wrapper deletes
  one record at an index `≥ r`. The publish is refused before the flip: no flip commit is logged
  and no root names the group. (D3) With `resume_from = 0` and every segment already durable and
  identical, the publish completes without rewriting any of them. The check before the flip
  covers all of `0..segment_count`, never only `0..resume_from`; D2 is the case that tells the two
  apart.
  **(E) The flip carries the caller's batch atomically.** The caller's flip contribution (a put on
  some key of its own) commits in the same batch as the root. A wrapper that answers the flip
  `Conflict` leaves neither the root nor that put.
  **(F) Epoch disjointness, fence cycles, and the rollback race.** Two publications of the same
  nonce at epochs `E` and `E' > E` write disjoint key ranges. After `E'` publishes, deleting
  **every** record of epoch `E`'s range (what a rollback drain does) leaves `E'` resolving
  correctly. A fence contribution sequence that goes `A → B → A` across the segment phase and the
  flip is refused under (C). **Rollback, both orders, forced by the wrapper.** The test's rollback
  has the target's shape (`SessionRecord::completing_teardown`,
  `crates/core/src/multipart.rs:2556-2582`): first **one** batch that requires the fence record
  at the value it read and moves it to a value no publisher batch requires, and only after that
  commits, a drain that deletes the group's `seg:` range. (F-r) The rollback's fence move lands
  just before the flip: the flip answers `Conflict`, no root names the group, and the drain then
  deletes the range with nothing published over it. (F-p) The flip lands first: the rollback's
  fence move answers `Conflict`, nothing is deleted, and the root resolves.
  **(G) Seeded publication atomicity** (`AGENTS.md:188-190`). A NEW madsim scenario
  `crates/dst/tests/segmented_publication.rs` runs publications on `SimFdbMetadataStore` with the
  commit-ambiguity nemesis armed (`crates/dst/tests/support/mod.rs:409-415`), including
  `CommitUnknownResult` mid-phase followed by the same caller's recovery at the same epoch, and a
  concurrent rival rollback with (F)'s shape: it first wins a fence move off the publisher's
  value, and only then deletes the epoch's `seg:` range. After every seed: at most one root names
  the group; a root that names it resolves to the plan with every segment present; and no root
  names a missing or different `seg:` record.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target.
  Each red leg is forced, not left to the scheduler: the recording wrapper measures (B) and (C),
  planted records and a wrapper deletion after the last segment commit drive (D), a wrapper
  answers `Conflict` for (E), and the rollback's fence move and drain in (F) are explicit steps,
  run in both orders. (G) runs under `cargo xtask dst` (madsim), which `cargo xtask ci` runs as
  its last step (`run_dst` at `xtask/src/main.rs:1745`, inside `run_ci` `:1728-1749`), so C4-ci
  reaches it. (G) is invariant coverage;
  (A)–(F) are the deterministic regressions.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*). Every root a publication commits names only `seg:` records that are
  durable and byte-identical to its plan at the instant it commits; no rollback keyed on the
  publication's fence can delete a `seg:` record a committed root names; and every `seg:` record
  written before the flip lives under the attempt's own epoch, where the attempt's rollback
  obligation can name it (`0016:2357-2390`). **The committer cannot hold this alone; two caller
  obligations complete it, and the committer's doc states both.** (1) A rollback deletes a
  group's `seg:` records only after it has committed a move of the fence record off every value
  a publisher batch requires (the target's rollback does this: `Completing@E → Aborting@E+1`
  together with its `retire:records:{seg}` obligation, `multipart.rs:2556-2582`); it never
  authorizes a delete from a fence value it merely read. The flip requires the fence at the
  publication value and moves it, so the flip and that fence move cannot both land. Separate
  reads protect nothing here: `scan_page` gives no snapshot isolation
  (`crates/traits/src/lib.rs:1390-1395`) and only one batch is atomic (`:1445-1446`). (2) The
  caller's contributions never touch a committer-owned key (the target inode, the dirent, the
  group's `seg:` range); the committer refuses one that does (C). #658 meets both: its
  contributions touch only `mpu:<id>`, `retire:` keys and the bucket record, and every path that
  ends its attempt moves the fence first (its own rejection release, the reaper #625, the
  restore fence already on `main`). **Sources:** the maintainer's rule 2026-07-25 (via
  principles.md §5); 0016's refutation standard (`0016:2802-2813`); decision 7(c)'s F18 closure
  (`0016:2374-2390`). SELF-TEST: it binds the committer, the shared resolver, and a rollback keyed
  on a value the caller owns, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 648
- **Conflicts with:**
- **Ordering note:** `Depends on 648`: the `ChunkMap::Segmented` shape and the `seg:` key helpers
  it builds on (merged; the resolver from #649 is merged too). This bundle is in wave 1 of the
  2026-10-19 batch beside #657; #658 (Complete, its only producer) depends on both and builds on
  their accepted result. **No `Conflicts with` 657, on purpose:** this slice puts the committer
  in a NEW core module and touches neither `crates/core/src/metadata.rs` nor
  `docs/design/architecture/05-building-block-view.md`, which #657 edits; keep it that way (see
  Scope) so the two can build in parallel. Intake cap (wyrd-pdca-P1): this brief is inside the
  room (alpha 4/6 before this batch, room 2; this is one of the two).
- **Scope:** the committer as one `wyrd-core` module: planning the segments, the bounded
  segment-write phase, verifying the whole durable prefix, and the single-batch root flip; the
  deterministic refusals; the staged-publication paragraph of the living-architecture docs; the
  two new test files above. / out of scope:
  - the multipart session, its states, the `segments_written` cursor and the Complete verb (#658
    owns them and passes them in);
  - the retry-on-conflict loop around the flip (`R_publish`, `0016:926-941`): the caller's;
  - installing any `retire:` obligation, including `retire:bytes:{generation}` for a superseded
    generation and `retire:records:{seg:<g>:<E>}` for a rollback (#658 and the drain #659);
  - deleting `seg:` records (the drain, #659), and the `seggrp:` marker (Create reserves it; the
    terminal delete, #659, removes it);
  - `crates/core/src/metadata.rs` and `docs/design/architecture/05-building-block-view.md` (no
    edits; #657 edits both in the same wave);
  - any gateway, S3, CLI or custodian caller;
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). Today no code path writes a `seg:` record or a
  segmented root; `ChunkMap::Segmented` is only ever decoded.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/segmented_publication.rs`, a **NEW** file (C4-verify treats an
  added `crates/*/tests/*.rs` as the per-fix discriminator). The DST scenario
  `crates/dst/tests/segmented_publication.rs` is also NEW.
- **Verification posture:** (a) net-new. The committer does not exist on `main`, so the
  discriminating test cannot compile on C4-verify's reverted base and C4-verify reports
  **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Built and exercised at Check:** the committer, driven by the new test over a real store, and
  by the DST scenario under `cargo xtask ci`'s DST step.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - the pre-flip check reads only `0..resume_from` → (D2) fails;
  - segments batched by a fixed record count instead of encoded bytes → (B) fails;
  - the root written in a batch separate from the caller's flip contribution → (E) fails;
  - one refusal check moved after the first segment commit → (C) fails;
  - the owned-key check on caller contributions removed → (C)'s conflicting-contribution case
    fails;
  - the fence-cycle check removed → (F)'s `A → B → A` leg fails;
  - the fence precondition dropped from the assembled flip batch → (F-r) fails (a root over a
    drained range).
- **Production reach:**
  - (a) The tests drive the committer for real against a real store, with a stand-in fence record.
  - (b) The production caller is #658's Complete, in the next wave of this run. The S3 verb that
    reaches it is #508's.
  - (c) This slice **must not** add a production caller.
  - (d) It changes no existing production path.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Salvage source (the one reference Do may open besides the cited callsites):** #647's
    committer, `/home/eddie/wyrd/wyrd-pdca/results/issue_635/patch.diff`, lines 3226-4760
    (`SegmentedPublication`, `RootPrecondition`, `SegmentBatchInfo`, `write_segments`,
    `verify_durable_range`, `flip`, `publish`, the fence-transition and fence-cycle checks,
    `batch_bytes`, `batch_ops`). It was written against an older shape: there `seg_key` returned
    `Vec<u8>` and nonces were `&str`; on `main` `seg_key` returns a `Result`
    (`metadata.rs:1481`) and nonces are `SegmentNonce` (`:989`). Take the design, not the code
    verbatim, and take only what the legs above need: #647's own tests measured 3,700–4,100
    lines, which is why it was closed.
  - **Peer callsite to mirror for the root CAS:** `commit_chunk_map_superseding_leased`
    (`crates/core/src/metadata.rs:2434`) and `create_leased` (`:2024`), the lease-guarded
    committers this one sits beside: how they pin the prior inode and dirent, mint version 1 or
    `prior.version + 1`, and apply `overwrite_modified` (`:2373`).
  - **Records and budgets:** `SegmentedMap::new` (`:1097`), `SegmentRecord::new` (`:1344`),
    `SegmentGroup::from_nonce` (`:1024`), `seg_key` / `seg_range_prefix` (`:1481`, `:1506`),
    `MAX_ROOT_SEGMENTS` (`:544`), `MAX_VALUE_BYTES` (`:549`); `MAX_MAP_CHUNKS` / `MAX_SEG_CHUNKS`
    (`crates/core/src/multipart.rs:4959`, `:4963`), `MAX_BATCH_BYTES` (`:5067`),
    `MAX_SEGMENT_PUT_BYTES` (`:5080`), `MAX_BATCH_OPS` (`:5106`); `WriteBatch`
    (`crates/traits/src/lib.rs:1512-1552`); `scan_page` (`:1438`).
- **Prior-art check (triage cycles):** searched by path `crates/core/src/metadata.rs`,
  `crates/core/src/lib.rs`, `crates/core/tests/`, `crates/dst/tests/` and
  `docs/design/architecture/08-crosscutting-concepts.md` on `origin/main`. Merged: #648's record
  shape and key helpers, #649's shared resolver (`99c7fcf`, via PR #683), #844's `repoint_chunk`
  (the segmented write side for maintenance). Rejected: PR #647 (closed unmerged 2026-07-30; whole
  of #635 in one ~22,000-line patch, branch deleted; its committer is the salvage above). Neither
  new test file exists. Open PRs on getwyrd/wyrd: none.
- **Disposition hint:** new-feature

## Motivation

The launch requirement is objects over 10 GiB, and a flat map tops out at 158 chunks (158 MiB at
the 1 MiB default chunk size). Decision 7 settles segmentation as the answer
(`0016:2282-2321`), but only a multipart Complete may produce a segmented map, because staged
publication needs a fenced session and an epoch to key segments by (`0016:2289-2297`). #658
publishes through this committer; without it #658 would either write segments with no
fence discipline or be unable to publish anything past 158 chunks.

## Design

**Inputs.** The target (parent inode and name, plus the inode id to use for a fresh key), the root
precondition (fresh, or superseding a prior inode record the caller re-read), the segment group
`(nonce, epoch)`, the ordered chunk list, the object metadata to stamp, the fence record's key,
a per-batch fence contribution, a recovered `resume_from` cursor, optional byte and operation
budgets (clamped to `MAX_BATCH_BYTES` / `MAX_BATCH_OPS`, never above), and the caller's flip
contribution.

**Flat or segmented.** At most `MAX_MAP_CHUNKS` chunks: the root is flat and the publication is
the flip batch alone. More: plan `⌈chunks / MAX_SEG_CHUNKS⌉` segments, filled by **encoded
bytes** against the value ceiling, never a fixed count, and refuse a plan needing more than
`MAX_ROOT_SEGMENTS` before anything is written.

**The segment phase.** Split the planned records into batches by encoded bytes **and** by
operations (`0016:641-649`). Each batch carries the caller's fence contribution for its position
(the cursor value it requires and the one it leaves), and creates each `seg:` row under
`require_absent` or, for a row already durable and byte-identical, pins it rather than rewriting
it. A resume re-derives the same records (same keys, same bytes), so a recovery at the same epoch
never double-writes (`0016:2392-2401`).

**Verify, then flip.** Before the flip, read the group's whole range (bounded by
`MAX_ROOT_SEGMENTS`, paged with `scan_page`) and require every planned record durable and
identical. A caller's `resume_from` is a hint for where to start writing, never evidence that the
prefix is there. The flip is **one** batch: the fence, the root CAS, and the caller's whole flip
contribution (its preconditions and its mutations). It pins the target dirent and inode on both
branches: `require_absent` both for a fresh key, `require` the exact prior bytes for a supersede
(`0016:663`). The version is `prior.version + 1` from the prior the caller passed; the root's
`modified` follows `overwrite_modified` (`metadata.rs:2373`), which
`docs/design/architecture/05-building-block-view.md:219` says the multipart flip must call. Refuse
a flip batch over the envelope before the segment phase starts.

**Fence discipline (why the checks exist).** The rollback that reclaims a crashed attempt is keyed
on the fence record's value. So every batch must pin the fence record; the flip must **move** it
(otherwise the rollback's precondition still holds after the flip and it deletes live segments);
and across the phase plus the flip the fence must never return to a value it already left
(`A → B → A` re-arms a rollback keyed on `A`). All three are checked from the batches alone,
before any commit. The salvage's `check_fence_transitioned` / `check_fence_never_cycles` explain
the hazards in detail. These checks are necessary, not sufficient: the guarantee also rests on
the rollback moving the fence before it deletes anything (Invariant, caller obligation 1). State
that obligation in the committer's doc, and test it in both orders (F).

**Owned keys.** The committer owns the target inode, the target dirent, and the group's whole
`seg:<nonce>:<epoch>:` range. A caller contribution, in a segment batch or in the flip, that puts,
deletes or preconditions any of them is refused before the first commit (C): otherwise the
contribution could delete a segment inside the very batch that publishes a root naming it, and
the verify step would have checked the wrong state. The fence key is the caller's; it must
appear in every contribution, as above.

**Errors.** A typed error enum of this module's own, so `metadata.rs` and its `ChunkMapError` stay
untouched. A store fault, a `Conflict` and a `CommitUnknownResult` are returned to the caller as
they are, never reinterpreted: the caller owns the retry loop and the session re-read.

**Size discipline.** Keep the slice small: the issue measured #647's committer at ~730 production
lines and its tests at 3,700–4,100. One test per acceptance leg, not a matrix. If the design
cannot fit in roughly 900 production lines and 1,500 test lines, stop and say so in
`build-notes.md` rather than grow the slice; the fallback split (committer + flip atomicity, then
rollback/resume + the DST prop) is the human's call.

**Docs currency** (`AGENTS.md:154-157`): add a staged-publication paragraph after the segmented
write-side paragraph at `docs/design/architecture/08-crosscutting-concepts.md:89`: a segmented map
is published by writing its `seg:` records in bounded batches under a fence the publisher holds,
verifying all of them, then flipping the root in one batch that also carries the publisher's own
state change; segment keys carry the attempt's epoch, so a rolled-back attempt's records never
overlap a later attempt's. Note the only producer is the multipart Complete (#658).

## Alternatives considered

- **Publish a segmented map in one batch.** Exceeds the transaction envelope for any map large
  enough to need segments (`0016:2494`). Rejected.
- **Batch segments by record count.** 1,000 records of up to 100 KB is 10× the envelope
  (`0016:2495`). Rejected; batch by encoded bytes and operations.
- **Trust the caller's `resume_from` for the prefix.** A crashed or confused caller would flip a
  root over records nobody wrote: a live object with a missing segment, unresolvable for ever.
  Rejected; verify the whole range.
- **Let the committer know the session states.** Couples core's map writer to multipart's state
  machine and makes it untestable without one. Rejected; the fence is a parameter.
- **Put the committer in `metadata.rs`, as #647 did.** Collides with #657's edits in the same wave.
  Rejected for scheduling, not design.

## Impact & compatibility

No change to any record shape or to any existing path. Adds one module, its tests, one DST
scenario and one docs paragraph. No production behaviour changes until #658 calls it.

## Open questions

- None blocking. If the size discipline above cannot hold, the split point is recorded in
  #653's own issue text.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-19, revision pass): all three findings accepted and the brief
revised. (1) Rollback: the Invariant now states the caller obligation that a rollback moves the
fence off every publisher value before it deletes (the target's `completing_teardown`,
`multipart.rs:2556-2582`), (F) forces both the rollback-wins and flip-wins orders, and (G)'s seeded rival takes the same shape.
(2) Caller contributions: a new "Owned keys" rule and a zero-commit refusal in (C) for a
contribution touching the inode, dirent or `seg:` range; the Invariant says how #658 meets it.
(3) (D2) deletes a record at an index `≥ resume_from` after the last segment commit, which is the
case a `0..resume_from`-only check gets wrong.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or decline the predeclared new-API verification exception — the restored patch passes 12 core and 3 seeded tests, but the reverted discriminator never executes, so behavioral red→green remains unestablished; `review-red-green.log:5`, `review-red-green.log:49`, `review-dst.log:8`.; C5 Causal adequacy — Fix recovery's invented fence history — a valid committed A→B batch followed by a B→C flip is rejected on resume, preventing completion despite all segments being durable; `crates/core/src/staged_publication.rs:477`, `review-probes.log:5`.; T5 Judgment — Strengthen the byte-envelope discriminator — omitted precondition payloads allow incorrect transaction accounting to pass, including the independently surviving accounting mutant; `crates/core/tests/segmented_publication.rs:440`, `crates/core/src/staged_publication.rs:673`, `review-mutant.log:3`.; C4-verify — Both frozen and independent runs establish green tests and a compile-only reverted failure, not behavioral red; `gate-logs/C4-verify.log:23`, `review-red-green.log:5`.; `crates/core/tests/segmented_publication.rs:429` / `:447`: leg (B)'s "within the operations budget" can never fail. With `BUDGET = 12_000` each segment batch holds one full row, so it carries 4–6 operations against `OPS = 64`. I removed both segment-phase operations checks (`crates/core/src/staged_publication.rs:640` refusal and `:651` `ops > budget_ops` break), and all 12 core tests still pass. C5's surviving mutants `640:16` and `651:44` say the same. The check matters in production: with default budgets (5 MB, 500 operations), a 300-segment map (47,400 one-placement chunks, within `MAX_STAGED_CHUNKS`) planned **602 operations in one batch** with the check removed. The real code splits it 500 + 104. That scratch test is **not** in the patch. Fix: add a (B) case where the operations budget binds and bytes do not, e.g. `ops_budget = Some(5)` with the default byte budget.; `crates/core/src/staged_publication.rs:853` and `:857`: the brief's Design says the flip "pins the target dirent and inode on both branches" (`0016:663`, the iteration-9 rename race). No test checks either dirent pin. I deleted `.require_absent(dirent.clone())` from the Fresh flip, and separately `.require(dirent, encode(&DirentRecord { inode: id }))` from the Supersede flip. All 15 tests (core and DST) stayed green both times. cargo-mutants cannot generate a deleted builder call, so C5 could not have caught this. Concrete failing case: `RootTarget::Fresh { id: 9 }` for a name already bound to inode 8 rebinds the name and orphans inode 8, where the pinned flip returns `Conflict`. Fix: one Fresh-over-a-bound-name case and one Supersede-after-rename case, each asserting `Conflict` and an unchanged dirent.; `crates/core/src/staged_publication.rs:416-417`: the brief's (D) says the pre-flip check covers all of `0..segment_count`. The tests only check half of that. I replaced the pre-flip check with one that checks only `resume_from..count`, keeping absolute indices. All 15 tests pass. D1 (`:632`) damages a row below the cursor *before* `publish` runs, so the first range read catches it. D2 (`:660`) deletes a row *at or past* the cursor. Nothing deletes a row below `resume_from` *during* the segment phase. Concrete failing case for such a committer: `resume_from = 2`, and a delete of `seg:…:0` after the last segment commit leaves a root naming a missing record. The fence obligation would also stop this; the brief asks for the check as a separate layer and says D2 is what tells implementations apart. Fix: a D2 twin that deletes index `< resume_from` in the same `Action::After` hook.; `crates/dst/tests/segmented_publication.rs:234`: the (G) property "at most one root names the group" cannot fail in this scenario. The only inode ever published is `ID = 8` (`:45`), so `scan(b"inode:")` returns 0 or 1 rows whatever the committer does. This is the same blind spot as the dirent bullet: with either dirent pin deleted, the DST stays green. Fix: have the recovering publisher mint a fresh inode id per attempt, as a re-resolving Complete may, so that only the dirent pin prevents a second root. The other two (G) properties do mean something. My instrumented sweep reached `resume_from = 1` 58 times and `resume_from = 2` 22 times after landed `CommitUnknownResult`s, and the drain-on-read red fires.; `crates/core/src/staged_publication.rs:557-560` (and the `:155-157` / `:400` API): the doc says that at the sizing scheme's widest ref the byte rule and the count rule agree, "so the plan is `⌈chunks / MAX_SEG_CHUNKS⌉` segments". That is the formula `0016:2346-2348` gives #658. But the byte rule at `:576` does bind for refs wider than the RS(6,3) sizing ref. 316 chunks under `rs(10,4)` with 20-digit placement ids encode to 408 B each, and the committer plans **3** segments where the formula says 2. The caller must pin the final cursor in its `flip_contribution`, and only `publish` is public. So #658 has to copy `split_segments`' arithmetic (80-byte envelope, +1 comma) or get the count wrong. A flip pinned at `cursor(2)` gave `Ok(Conflict)` after the segment rows were already written. A retry from `resume_from = 3` is then refused as `FenceCycled`, so the object can never publish. The tests hide this by hard-coding `SEGMENTS = 5` (core `:39`, DST `:50`). Fix: expose the plan's segment count (e.g. `pub fn planned_segments(&[ChunkRef]) -> Result<u32, PublicationError>`, reusing `split_segments`) and correct the doc. Separate question for #658, not this slice: the committer reads the compile-time `MAX_MAP_CHUNKS`/`MAX_SEG_CHUNKS`, while `KnobSet` (`crates/core/src/multipart.rs:5430`) makes them per-deployment knobs.; `crates/core/src/staged_publication.rs:474-488`: I agree with T4's blocking finding: rebuilding a resumed prefix from one-row batches can raise a false `FenceCycled`. The same assumption also fails in the open direction. If a pure hook's *pin* depends on `written` (allowed by the contract at `:56-58`), the singleton rebuild can miss a value a coarser earlier batch really required, and a later return to it goes unflagged. Neither direction can happen with #658's cursor-shaped hook. No test exercises a resumed cycle at all: C5 mutant `481:28` (`i + 1` → `i * 1`) survived. Whatever fix answers T4 should state, or check, the hook shape the rebuild relies on (pin a function of `first_index` only, put of `written` only) and add a resume-then-cycle test.; `crates/core/src/staged_publication.rs:477`: Recovery invents fence history by replaying hypothetical singleton batches. A permitted pure hook can return `(0,2): A→B`, `(0,1): A→C`, and `(1,2): C→B`, with a flip `B→C`. An initial two-segment batch passes preflight and can land with `CommitUnknownResult`; retrying with `resume_from = 2` then falsely reports `FenceCycled` from the invented `A→C→B→C` sequence, although the actual publication sequence is `A→B→C`. Base recovery validation on actual transitions, or enforce the necessary stronger hook contract before the initial write, and add a regression covering recovery after a multi-segment commit.; `crates/core/tests/segmented_publication.rs:440`: The byte-envelope assertion measures only mutations, while the committer's budget includes precondition keys and expected values (`crates/core/src/staged_publication.rs:669`). A regression omitting those bytes can therefore pass, especially during recovery when witnessed segments become value preconditions with no segment puts. Measure the complete batch independently and include a boundary case with durable segment pins or a large caller precondition. Also exercise an operation-limited split: the current five-segment fixture cannot make its 64-operation limit bind (`crates/core/tests/segmented_publication.rs:429`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_653/review-b. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 166 mutants tested in 12m: 45 missed, 111 caught, 9 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_653/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Tests only: the committer's production logic held up under three reviewers in round 2 — keep it, do not redesign. Close the five test gaps so each guard has a discriminating red: 1. Flip that deletes an unrelated key without moving the fence (`require(FENCE, cursor(n)).delete("test:unrelated")`) → zero-commit `FenceNotMoved`; plus a successful fence-deleting flip and a caller-delete case where the byte/ops envelope binds (kills the `:847` `==`→`!=` and `:696`/`:702` delete-subtraction mutants). 2. Inode pins on both flip branches: Supersede after a concurrent PUT bumped the inode → `Conflict`, PUT's version kept; Fresh over an existing inode record → `Conflict`. 3. `read_group` paging: publish and resolve one map with more than 128 segments (`SEGMENT_PAGE_LIMIT`). 4. `StrandedRow` and `ForeignRow` refusals: one test each (e.g. a durable `seg:<nonce>:E:00005` beside a 5-segment plan). 5. Wide-ref test: assert every recorded `seg:` put is within the byte budget (kills the `:595` `bytes + cost`→`bytes - cost` mutant). Size: the human accepts the slight overage (108 KB / ~1,050 prod lines); keep additions to these tests, no matrices, no production growth beyond what a red forces.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Tests only: the committer's production logic held up under three reviewers in round 2 — keep it, do not redesign. Close the five test gaps so each guard has a discriminating red:
  1. Flip that deletes an unrelated key without moving the fence (`require(FENCE, cursor(n)).delete("test:unrelated")`) → zero-commit `FenceNotMoved`; plus a successful fence-deleting flip and a caller-delete case where the byte/ops envelope binds (kills the `:847` `==`→`!=` and `:696`/`:702` delete-subtraction mutants).
  2. Inode pins on both flip branches: Supersede after a concurrent PUT bumped the inode → `Conflict`, PUT's version kept; Fresh over an existing inode record → `Conflict`.
  3. `read_group` paging: publish and resolve one map with more than 128 segments (`SEGMENT_PAGE_LIMIT`).
  4. `StrandedRow` and `ForeignRow` refusals: one test each (e.g. a durable `seg:<nonce>:E:00005` beside a 5-segment plan).
  5. Wide-ref test: assert every recorded `seg:` put is within the byte budget (kills the `:595` `bytes + cost`→`bytes - cost` mutant).
  Size: the human accepts the slight overage (108 KB / ~1,050 prod lines); keep additions to these tests, no matrices, no production growth beyond what a red forces.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 177 mutants tested in 11m: 22 missed, 147 caught, 7 unviable, 1 timeouts
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
