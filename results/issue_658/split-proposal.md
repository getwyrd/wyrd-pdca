<!-- pdca:split-proposal v1 -->
# Split proposal — issue 658

**Intake-cap override (wyrd-pdca-P1), 2026-10-30.** `scripts/plan-cap --track alpha --need 5`
answered, when this proposal was written:

```
planned 7/6 (cap, track alpha) — room for 0, need 5: Plan intake closed
  PLANNED            6  508 625 633 658 659 927
  BUILT              1  654
```

The human (Eduard Ralph) overrode the cap explicitly in this session, on 2026-10-30, for #508,
#625, #633, #658 and #659, and then asked for a finer cut of this proposal's first child. If
it is accepted, alpha goes from 7/6 to 11/6 while the five children are open. (The parent leaves
the count when it goes COMPLETE, and the five children enter it.) The override covers this split
of #658 as one of those five ids. It is not a standing exception.

> Every child below is a complete brief carved from the parent `results/issue_658/brief.md`.
> Citations are the parent's, verified against `getwyrd/wyrd` at `4bac538` (2026-10-22). The
> target's `origin/main` is at `376b106` today, and #932, #933 and #653 have been published since.
> Do re-resolves every `path:line` on the base it is given.

## Why this slice is oversized

#658 is the multipart `Complete` verb, the largest single verb in 0016. Its siblings show the
scale. #895 (Abort: one fence and its outcomes) was a 134 KB patch, about 87 KB of it core code
and tests. #653 (the committer Complete calls) was 118 KB. The discontinued single-slice #636,
of which Complete was one part, reached 664 KB. The parent brief has nineteen test legs in nine
groups (A–I), fifteen demonstrated reds, two error arms, and a value-ceiling rule still open for
sign-off. Built as one cycle it would be well over 300 KB. Clean children in this repo have
shipped at about 40–110 KB with one outcome each.

### How the cut was found

**Revision 2 (2026-10-30).** The first version of this proposal had three children. Its child-1,
"definite outcomes", was estimated at 200–280 KB. That is the size that has needed re-splitting
here: #926's children were estimated at 190 and 80 KB and built at 285 and 213 KB. The human
asked for a finer cut. This revision has five children and keeps the two seams already found.

- **Definite vs unknown commit outcomes.** The parent's invariant already allows Complete to
  return the in-doubt error, leaving its own fence for the gateway (#670) or the reaper (#625),
  whenever a commit outcome is unknown. So a Complete that answers every `CommitUnknownResult`
  that way (G0) is safe on its own. Recovery (G1–G6) is a separate child.
- **The value ceiling (I)** has its own wrapper, its own fixtures and an open design question. It
  is a separate child.

It adds two cuts inside the old child-1.

- **The fence protocol vs publication.** The fence, the frozen-set validation, the release and
  the attempts budget can ship before anything is published. They form the half of Complete where
  the parent's own defect lives: the discontinued patch's three typos wedged the session. That
  half has its own pass condition. A client's mistakes are refused, the session is `Open` again
  with its budget intact, a valid request still fences, and a fence that cannot be released stops
  with an error, never an answer. It ships as the fence phase's own functions, with no publication
  yet, the way #653's committer shipped before its caller.
- **Flat vs segmented publication.** Everything about segments is one outcome: the group naming,
  the segment phase, the rule that a fence which wrote segments stays charged (D3), and the
  `retire:records:{seg}` obligation on its release. That outcome is "objects past
  `MAX_MAP_CHUNKS` chunks can be published". Until it lands, a flat-only Complete refuses a list
  over `MAX_MAP_CHUNKS` chunks with an error before it fences. Nothing calls Complete in
  production until #670, so the restriction is visible to no one.

### Starting points tested and rejected

- **A, B vs D, E, H.** Publication cannot ship without validation and the release. A fence is
  needed to publish, and a fence with no release breaks the invariant. The cut runs the other way
  round: the fence protocol goes first, publication second.
- **C and F2 as their own child.** A flip that removes `part:` names must, in the same batch,
  install the obligations that name those bytes (C-1). So publication cannot ship without
  `retire:records:` for the named parts. The unnamed-part `retire:bytes:` of C (subset Complete)
  and the empty-prior rule of F2 are each a few lines of the same flip contribution. As a child of
  their own they would be about 15–25 KB, and each would cost a full cycle. They stay with flat
  publication.
- **F out of publication.** F is the lost-flip `Conflict` loop. H2 is that loop re-reading the
  bucket marker, and D3 is that loop running out. A publication child without it would have to
  freeze the version at fence time, which the parent rejects. F stays with flat publication.
- **J and G0** go with whichever child first sends the commit they classify. The fence and
  release cases are in child-1, the flip in child-2, and segment batches in child-3.

### The five outcomes

1. **child-1: the Complete fence, which a client's mistakes never wedge.** Answers by state,
   fence, frozen-set validation, release with the budget rule, the error type and its stopped arm.
   Legs E, D1, D2, D4, H, and G0 for the fence and release.
2. **child-2: Complete publishes flat objects.** Assembly, the flat publish, the flip and its
   obligations, lost flips, the bucket guard on the flip, a refused flip, and the public verb.
   Legs A (flat), C, F, F2, H2 (flat), J (flat), G0 for the flip, and D2's closing "a valid list
   then publishes".
3. **child-3: Complete publishes segmented objects.** Legs B, D3, H2 (segmented), J (segmented),
   and G0 for segment batches.
4. **child-4: Complete recovers from unknown commit outcomes.** Legs G1–G6.
5. **child-5: Complete stays inside the store's value ceiling.** Legs I1–I4.

### Sizes

The estimates are bottom-up from #895, #653 and #932. Past bottom-up estimates here came in low
(#926's by 1.5–2.7x), so the top of each range is the number to plan around.

| child | code | harness | legs | total |
|---|---|---|---|---|
| child-1 | 35–45 KB | 25–30 KB (scripted store wrapper) | 30–40 KB | **90–115 KB** |
| child-2 | 30–40 KB | 10–15 KB (placement-aware chunk store) | 50–60 KB | **90–115 KB** |
| child-3 | 15–25 KB | 0 (reuses) | 40–55 KB | **55–80 KB** |
| child-4 | 20–30 KB | 5–10 KB (hold step) | 40–60 KB | **70–100 KB** |
| child-5 | 10–15 KB | 10–15 KB (value-refusing wrapper) | 20–40 KB | **40–70 KB** |

**child-1 and child-2 sit at the edge of the band, and no honest further cut exists inside
either.** In child-1, a fence without its release breaks the invariant, validation without the
release is unreachable, and the stopped arm is the release's failure path. In child-2, the flip
cannot leave out its obligations (C-1) or its bucket pin, and the version rule (F) is the flip's
conflict loop. Cutting further would give halves that cannot ship alone. If either child crosses
the 100 KB backstop, most of it will be tests.

**Merges were checked.** child-3 with child-5 would be 95–150 KB, and child-1 with anything else
is over the band. Five is the fewest children that keep every child near or under 110 KB.

### Test files and C4-verify

child-1 ships `crates/core/tests/multipart_complete.rs` as a new file. Complete does not exist on
its reverted base, so C4-verify reports UNVERIFIABLE (exit 77), as the parent already declared.
child-2, child-3 and child-4 append to that file and extend its harness. They do not copy the
harness, and they do not move it to a `tests/support/` module:
`engine/scripts/run-verify.sh:141-144` classifies every added `*/tests/*.rs` as a test target, so
such a module would become a test that does not exist (#926's proposal has the dry run). Each of
the three therefore gets C4-verify `PASS (green-only)`, and its red rests on the demonstrated
reds its brief lists, as #933's did.

child-5 ships its own new file, `crates/core/tests/multipart_complete_value_ceiling.rs`, and gets
a real red→green.

**For the human:** child-2 could ship a new file instead, with its own copy of the wrapper (about
+25 KB), to get an ADDED_TEST discriminator for the main publication child. The cost is that
child-2 then sits above the band.

## Wave sketch

Four waves.

- **Wave 1: child-1.** Its tests plant canonical `part:` / `psum:` records through the key and
  record helpers already on `main` (`part_key`, `psum_key`, `crates/core/src/multipart.rs:1321`,
  `:1336`). They do not stage, so child-1 needs neither #927 nor #653 to build or test.
- **Wave 2: child-2** `Depends on child-1`. It composes child-1's fence and release into the public
  verb, in child-1's module. Its read-back legs need staged parts (#927) and the committer (#653).
- **Wave 3: child-3 and child-5, in parallel.**
  - child-3 `Depends on child-2`: it extends child-2's publish phase to segments and removes
    child-2's flat-only refusal.
  - child-5 `Depends on child-2`: the obligations it measures are the ones child-2's flip
    installs.
  - They do not conflict. child-5 edits the pre-fence check and the frozen-set validation (the
    code child-1 wrote), plus the `SessionTooLarge` doc in `multipart.rs`. child-3 edits the
    publish phase and the release's segment obligation, and appends to the shared test file.
    child-5 has its own new test file. Neither touches the error type.
- **Wave 4: child-4** `Depends on child-3`. G4 (an unknown segment write) needs the segment phase.
  It `Conflicts with child-5` because both edit the code around the fence (child-4 the fence's
  unknown-outcome path, child-5 the check before it). The dependency already puts them in
  different waves, and the conflict keeps it that way if child-5 slips.

**For the human:** child-4 could depend on child-2 only and run in wave 3, moving G4 into
child-3. That makes child-3 about 70–100 KB and child-4 about 60–90 KB, and saves a wave. I kept
G4 with the other recovery legs because their resume logic is the same code.

**WARNING: the external `Depends on:` cannot go in the child fields.** `pdca split --accept`
refuses any ordering token that is not a sibling label (`src/pdca_harness/split.py:396-406`), and
it does not copy the parent's `Depends on` into the children (`materialise`, `split.py:770-800`).
**Right after `--accept`, edit child-2's brief to `Depends on: <child-1's id>, 932, 933, 927,
653`.** #927 is still PLANNED, and without it child-2 could be scheduled with no `UploadPart` to
stage through. child-3, child-4 and child-5 inherit the edit through child-2. child-1 needs none.
If you would rather child-1 waited for #927 too (in case #927 changes the `psum:` shape), add
`927` to child-1 as well. That costs child-1's early start. This is a harness gap (a split cannot
carry its parent's outside prerequisites) and belongs upstream in eduralph/pdca-harness.

**Dependents of #658 must be re-pointed after acceptance.** A split parent ends COMPLETE, and the
scheduler does not move its dependents onto its children. Four briefs name #658: #508
(`brief.md:507`), #625 (`:321`), #633 (`:145`) and #659 (`:124`). All four are being split in
parallel. By what each consumes:

- **#508** (the S3 verb, and #670 under it): all five children.
- **#625** (the reaper's rollback of a stale `Completing`): child-1, child-2, child-3 and child-4.
  child-4 decides which fences are left for the reaper.
- **#659** (the drain): child-1, child-2 and child-3. It drains the obligations their flips and
  releases install, including the segment-group obligations from child-3.
- **#633** (the operator abort): child-1, for the fenced `Completing` edge.

Replacing `658` with all five ids everywhere is the simple safe choice. **The human picks.**

<!-- pdca:child child-1 -->
# core: Complete fence — validation, release and a budget a client's mistakes never spend (636.5a)

> First of the five slices #658 (636.5) was split into on 2026-10-30. Later slices publish
> flat (636.5b) and segmented (636.5c) objects, recover from unknown outcomes (636.5d) and
> enforce the value ceiling (636.5e). Do reads ONLY this file. The design is normative in
> proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read
> **decision 3** (`0016:895-1036`, the verb × state table at `:973-979`, the rejected-Complete
> rule `:994-1000`), and the **Complete fence** and **Fence release** rows of the batch inventory
> (`0016:661`, `:664`). Citations were verified against `getwyrd/wyrd` at `4bac538`; re-resolve
> them on your base.

- **Slug:** multipart-complete-fence
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** Nothing fences a multipart session for publication. The discontinued #636 patch
  did, and it wedged the session: three malformed Completes made the fourth, correct one answer
  `CompleteAttemptsExhausted`, with the session still `Open` and every part durable. Abort and a
  full re-upload were the only way out. This slice adds the fence phase of
  `CompleteMultipartUpload` as `wyrd-core` functions over `MetadataStore`:
  - answer every non-`Open` state from `complete_answer` (`crates/core/src/multipart.rs:4782`);
  - fence `Open@E → Completing@E+1`, charging `attempts`;
  - validate the named-part list against the frozen part set and the bucket;
  - on a rejection, release the fence (`Completing@E+1 → Open@E+2`) with `attempts` restored,
    since a fence that wrote no segment minted nothing the cap bounds;
  - when the release cannot land, stop with an error that says the fence still stands, never an
    answer.
  
  A valid request leaves the caller holding the fence, as a value the next slice consumes to
  publish. Nothing here publishes.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_complete.rs`, over
  `RedbMetadataStore::in_memory()` and a test-local store wrapper that logs every batch and,
  before a chosen commit, can:
  - answer `Conflict` without applying the batch;
  - answer `CommitUnknownResult` (applied or not, with either `may_still_commit` value);
  - answer a definite `Err`;
  - write a key first;
  - fail a read.
  
  Build the wrapper so later slices can extend it, for example with a hold step. Sessions are
  made by #894's Create. Parts are planted as canonical `part:` / `psum:` records through the key
  and record helpers on the base (`part_key` `multipart.rs:1321`, `psum_key` `:1336`), with no
  staging. Every leg is bounded by a watchdog. It shows all of the following.
  **(E) Answers by state** (`0016:976`). Against a `Completed` record planted with a
  publication, an identical request answers `AlreadyCompleted` with the **recorded** publication
  (same ETag, inode, version), and a different list answers `Refused(NoSuchUpload)`. Against a
  `Completing` session (another publisher's fence), it answers `Refused(OperationAborted)` and
  leaves that session record and its `seg:` range byte-for-byte unchanged (`0016:953-958`). Each
  of these sends zero commits.
  **(D1) Typos.** `MAX_COMPLETE_ATTEMPTS` (`multipart.rs:5200`) requests in a row with invalid
  lists (an absent part, a digest mismatch, part numbers out of order) each answer
  `Refused(InvalidPart { .. })`. After each one, the session is `Open` and `attempts` equals its
  value before that request.
  **(D2) A part replaced between the first read and the fence.** `MAX_COMPLETE_ATTEMPTS` requests
  in a row. Each names a list that is valid against the part set when first read. The wrapper
  re-plants one named part (new digest) just before the fence commits. Each request fences, fails
  against the frozen set, releases, and answers `Refused(InvalidPart { .. })`, with the session
  `Open` and `attempts` restored.
  After D1 and D2, a request with a valid list **fences**: it returns the held fence at
  `Completing@E'+1` with `attempts` one higher, not `CompleteAttemptsExhausted`. After a rejected
  request, an Abort answers `Fenced` (`0016:1033`).
  **(D4) A fence it owns and cannot release.** Set up as D2, so the fence lands with a **definite**
  success and the frozen list is invalid. The wrapper answers every release commit `Conflict`
  without applying it and without changing the session (the TiKV lock case,
  `crates/core/src/multipart/abort.rs:245-247`). No unknown outcome is injected. The request
  returns the **stopped** error:
  - it names the upload and the epoch of the fence it still holds (`E+1`);
  - it says the fence still stands;
  - its cause says the release rounds ran out;
  - it carries no `CommitUnknownResult`.
  
  The session is `Completing@E+1`, byte-identical to the fence's bytes. The wrapper counts at most
  the stated bound of release commits. A second run answers the release with a definite store
  error: the same stopped error, with that store error as its cause.
  **(H) Bucket deleted** (`0016:1035`). With the bucket record removed while the session is
  open, the request answers `Refused(NoSuchBucket)`, and the session is `Open` again with
  `attempts` restored.
  **(G0-fence) Unknown outcomes stop with the in-doubt error.** The wrapper answers
  `CommitUnknownResult` on the initial fence (once applied, once not) and on a release. Each time
  the request returns the in-doubt error carrying that outcome and its `may_still_commit`. It never
  returns a `CompleteOutcome`, and it sends no further commit. In a further run, the first read of
  `mpu:<id>` fails with a store error, and the request returns an error, never an answer.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target.
  Every red leg is forced by a wrapper step, not by timing: D2's re-plant before the fence, D4's
  refused releases, G0's unknown outcomes and failed read, and H's bucket removal. (D) is the
  #636 patch's reproduced defect.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), for the fence phase. No Complete that returns an **answer** leaves
  **its own** fence in `Completing`. A Complete that meets **another** publisher's `Completing`
  fence leaves it untouched. A fence is owned only when a fence commit of its own answered a
  definite success. Only an **error** may leave its own fence in place: the in-doubt error, or the
  stopped error with the fence standing. Within S3's part-number range, no client error can leave
  an `Open` session unable to fence again. **Sources:** the maintainer's rule 2026-07-25 (via
  principles.md §5); decision 3's rejected-Complete rule (`0016:994-1000`); X58 (`0016:2577`,
  `:1479`). SELF-TEST: it binds the fence, the release and the budget together, so guarding one
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** No sibling dependency: this is the first slice. No external dependency is
  needed either. The tests plant part records through helpers already on `main`, and nothing here
  stages or publishes, so it can build before #927 and #653 land. Add `927` after `--accept` if
  the human wants this slice to wait for #927's record shapes. child-2 to child-5 build on this.
- **Scope:** in `crates/core/src/multipart/complete.rs` (a new module beside `abort.rs`):
  - the request type (named `(part, digest)` list, target key);
  - the entry read and the answers by state;
  - the fence commit with the attempts budget;
  - the bounded read of the frozen `psum:` / `part:` ranges and the validation (named parts
    present with the named digest, numbers strictly ascending, bucket present, recording the
    `bucket:` marker bytes read);
  - the held-fence value returned for a valid request (carrying the frozen set, the epoch and
    the marker bytes for the next slice);
  - the release (`Completing@E+1 → Open@E+2`, `attempts` restored, bounded rounds);
  - the error type with its two arms, **in doubt** and **stopped**;
  - the crate-private session transitions it needs (modelled on `SessionRecord::open`,
    `multipart.rs:2360`).
  
  Out of scope:
  - assembly, any publication, any flip or any `retire:` obligation (child-2,
    child-3);
  - the release's `retire:records:{seg}` obligation and the charging rule for fences that wrote
    segments (child-3);
  - any re-read or resume after an unknown outcome (child-4);
  - the value-ceiling check (child-5);
  - the reaper (#625), the drain (#659), the S3 verb (#670), DST (#660);
  - any edit to 0016 or an ADR, `Cargo.lock` or `deny*.toml`.
- **Repro instruction:** n/a (new functionality). The wedge (D) guards against is in the
  discontinued #636 patch (`/home/eddie/wyrd/wyrd-pdca/results/issue_636/patch.diff`).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/multipart_complete.rs`, a **NEW** file. child-2, child-3 and
  child-4 append to it.
- **Verification posture:** (a) net-new. The module does not exist on the reverted base, so
  C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Each is a temporary edit that
  makes the named leg fail:
  - a rejected request counted against `attempts` → (D1) fails;
  - pre-fence validation kept, with `attempts` not restored on a post-fence rejection → (D2)
    fails;
  - the session left `Completing` after a rejection → D's Abort leg fails;
  - a request meeting another publisher's `Completing` fence releases it → (E) fails;
  - a release that fails definitely answered with its refusal, or reported as in doubt → (D4)
    fails;
  - an unknown initial fence treated as success → (G0-fence) fails.
- **Production reach:** (a) exercised over a real store in-process; (b) the S3 verb (#670 under
  #508) is the eventual caller, after child-2 to child-5; (c) this slice **must not** add a
  production caller; (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` on the target branch for every change.
  - Peer for the error arms and bounded rounds: #895's Abort, `abort.rs` (`AbortError`, `Stop`,
    `ABORT_ROUNDS`, `:100-160`, `:239-282`).
  - Records: `SessionState` (`multipart.rs:2173`), `PublishTarget` (`:2119`), `complete_answer`
    (`:4782`), `complete_fingerprint` (`:4423`), `InvalidPart` (`:4445`),
    `MAX_COMPLETE_ATTEMPTS` (`:5200`).
  - Peer for the bucket marker read: `require_bucket`, `crates/core/src/multipart/create.rs:700`.
- **Prior-art check (triage cycles):** the parent's. Merged: the answer table and digests
  (PR #799), the nonce on every session (PR #847), Create (#908), Abort (#909). Rejected: #636's
  single-slice attempt. The test file does not exist.
- **Disposition hint:** new-feature

## Design

**Fence** (`0016:661`). Read `mpu:<id>`, and answer from `complete_answer` for every state but
`Open`. On `Open@E`, commit `require(mpu == Open@E)` and put the session at `Completing@E+1`, with
`fenced_at_millis`, `segments_written: 0`, `publish_target {parent, name, epoch: E+1}` and
`attempts + 1`. At the budget, refuse `CompleteAttemptsExhausted` with zero commits.
Pre-validating against the part set read before fencing is allowed as an optimization, never as
the budget's protection (D2).

**Validate** against the frozen set (`0016:993-1000`). A rejection releases the fence: one batch
`require(mpu == Completing@E+1)` → `Open@E+2` with `attempts` restored to its pre-fence value.
Releases are bounded (Do picks the bound and cites it, as `ABORT_ROUNDS` bounds Abort). A
`Conflict` with the session unchanged spends a round. A session found moved is answered from its
new state.

**The stopped arm**, modelled on `AbortError::Stopped` (`abort.rs:109-117`, causes `:139-159`),
means no batch is in doubt. It names the upload, the fence's epoch, the cause, and **whether the
fence still stands**. child-2 uses the "released" case for a flip refused for good (J).

**The in-doubt arm**, in the shape of `AbortError::InDoubt` (`abort.rs:121-135`). In this slice,
any `CommitUnknownResult` returns it at once. child-4 adds the re-reads that settle some of them.

**Docs currency** (`AGENTS.md:154-157`). Add the fence to the multipart paragraphs of
`docs/design/architecture/05-building-block-view.md` (`:244-248`). Say that a rejected request
releases the fence without spending the budget, and that nothing publishes yet.

## Open questions

- **For sign-off:** the fence phase is exposed as `pub` functions with a held-fence value, so the
  integration test can drive it before the verb exists. child-2 composes them into the verb, and
  may narrow their visibility then. Confirm or redirect.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core: Complete — flat publication, lost flips and the bucket guard (636.5b)

> Second of the five slices of #658 (636.5). It builds on child-1 (636.5a, the fence phase).
> Do reads ONLY this file. Read 0016 **decision 3** (`0016:895-1036`, above all `:910-941` and
> `:1025-1035`) and the **Root flip** row of the batch inventory (`0016:663`) before writing
> code. Citations were verified at `4bac538`; re-resolve them on your base, which folds in
> child-1, #932, #933, #927 and #653.

- **Slug:** multipart-complete-publish-flat
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** After child-1 a valid request fences, but nothing publishes. This slice adds the
  public `CompleteMultipartUpload` function. It composes child-1's fence phase with the publish
  phase for objects of at most `MAX_MAP_CHUNKS` chunks:
  - assemble the named parts' chunk lists in part order;
  - publish through #653's committer;
  - flip the root in one batch, which:
    - moves the session to `Completed` (recording the ETag and the request fingerprint);
    - installs the retirement obligations (`retire:records:` for the published parts,
      `retire:bytes:` for unnamed parts, and `retire:bytes:` for a superseded generation that has
      chunks);
    - pins the validated `bucket:` marker;
  - on a lost flip, re-read and retry with the version recomputed from the prior;
  - release the fence when the flip cannot win, when the bucket is gone, or when the store
    refuses the flip for good.
  
  A list over `MAX_MAP_CHUNKS` chunks is refused with an error until child-3.
- **Success criterion:** New legs appended to `crates/core/tests/multipart_complete.rs`, reusing
  child-1's wrapper, and adding a test-local placement-aware chunk store. Parts are staged by
  #927's `UploadPart`, and sessions are made by #894's Create. It shows all of the following.
  **(A) Publish and read back.** Parts staged **out of order**, at least one **non-final** part
  whose length is not a whole multiple of the chunk size, total at most `MAX_MAP_CHUNKS` chunks.
  Complete naming all of them answers `Published`, and:
  - `read_object` (`crates/core/src/read.rs:510`) returns the parts' bytes concatenated,
    byte-identical;
  - the root is a flat map;
  - the inode's ETag and the session's `Completion.etag` both equal `multipart_etag` over the
    named `(part, digest)` list (`multipart.rs:4392`);
  - the session decodes as `Completed` with `complete_fingerprint` of that list (`:4423`).
  
  An identical retry then answers `AlreadyCompleted` with that publication.
  **(D2-tail)** After child-1's D1 and D2 rejections, a valid Complete answers `Published`, not
  `CompleteAttemptsExhausted`.
  **(C) Subset Complete** (`0016:1034`). Stage parts 1, 2, 3. Complete naming 1 and 3 publishes
  parts 1 + 3. The flip installed `retire:records:` naming **only** parts 1 and 3, and
  `retire:bytes:` naming part 2 (its bytes and its records). No chunk of part 1 or 3 appears in
  any `retire:bytes:` obligation. Every obligation decodes under `decode_retire_obligation`
  (`multipart.rs:3891`).
  **(F) The version comes from the re-read prior** (`0016:1025`).
  - Key `K` exists at version `v`. The wrapper lands an ordinary overwrite of `K` (version `v+1`)
    between the fence and the flip. Complete retries the flip and publishes `v+2`, and installs a
    `retire:bytes:` obligation naming the superseded generation.
  - When every flip attempt loses (`R_PUBLISH`, `multipart.rs:5192`), Complete releases the fence
    and answers `Refused(OperationAborted)`. The session is `Open`, with `attempts` restored.
  
  **(F2) An empty prior owes nothing.** Key `K` exists as a zero-byte object
  (`crates/core/src/write.rs:166-168`, `:277-281`). Complete publishes over it at the next version
  and answers `Published`. It installs **no** generation obligation for the empty prior, and every
  obligation it installs decodes.
  **(H2) Bucket removed just before the flip.** The wrapper deletes the `bucket:` marker after
  validation, immediately before the flip commit. The flip pins the marker's exact bytes
  (`crates/core/src/multipart/create.rs:698-705`, `0016:663`), so it conflicts. Complete answers
  `Refused(NoSuchBucket)`, not `OperationAborted` and not a retried flip. No dirent or inode exists
  for the key, and the session is `Open@E+2` with `attempts` restored.
  **(J) A flip the store refuses for good.** The wrapper answers the flip with a definite `Err`
  that is neither `Conflict` nor `CommitUnknownResult`. Complete sends no further flip and
  releases the fence (`Open@E+2`, `attempts` restored). It returns child-1's stopped error with
  that store error as its cause, saying the fence was released. No dirent or inode exists.
  **(G0-flip)** The wrapper answers the flip `CommitUnknownResult` (once applied, once not).
  Complete returns the in-doubt error carrying that outcome and sends no further commit.
  **(K) Flat only, for now.** Parts totalling more than `MAX_MAP_CHUNKS` chunks (`multipart.rs:4959`):
  Complete returns the stopped error with a cause naming segmented publication as unsupported.
  It sends zero commits when the pre-fence read already shows the excess. When only the frozen
  set shows it, the fence is released with `attempts` restored. child-3 replaces this leg with B.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. Every red leg is
  forced by a wrapper step: F's interloper and lost flips, H2's marker deletion, J's refused flip,
  and G0's unknown outcomes. F2's empty prior is an ordinary write.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle /
  reclamation*), for publication:
  - every staged part's bytes are, at every instant, named by its `part:` record, by the
    published root, or by a `retire:` obligation installed in the same batch that removes the
    other names;
  - no published chunk is ever named by a `retire:bytes:` obligation;
  - a flip never publishes into a bucket that is gone;
  - a flip is sent only under a fence a commit of its own answered with a definite success;
  - child-1's answer and error rules hold on every path.
  
  **Sources:** decision 3's scoping rule (`0016:910-925`), `0016:1025`, `:663`; 0016's refutation
  standard (`0016:2802-2813`). SELF-TEST: it binds the flip's obligations, its preconditions and
  the release, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** `Depends on child-1`: this slice composes child-1's fence phase into the
  verb, in child-1's module. **After `--accept`, add `932, 933, 927, 653` to this field.** The
  split parser accepts only sibling labels. #927's `UploadPart` stages the parts these legs
  publish, and #653's committer publishes them. child-3 and child-5 build on this slice.
- **Scope:** in `crates/core/src/multipart/complete.rs`:
  - the public `CompleteMultipartUpload` function;
  - assembly of the ordered chunk list;
  - the flat publish through #653's committer, with the flip contribution (session →
    `Completed { completion }` with inode, version, ETag, `completed_at_millis`,
    `complete_fingerprint`; the obligations through `RetireObligation`, each under
    `require_absent` with a token per `0016:358-373`; the `bucket:` marker pin);
  - the lost-flip loop with dirent and marker re-read;
  - the release on exhaustion, gone bucket or refused flip;
  - the temporary flat-only refusal;
  - the docs sentence.
  
  Out of scope:
  - segmented publication and the segment-charging rule (child-3);
  - recovery after unknown outcomes (child-4);
  - the value ceiling (child-5);
  - the drain and `unlink` / `PutObject` routing (#659);
  - the reaper (#625);
  - the S3 verb (#670);
  - DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** n/a (new functionality).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_complete.rs` (appended). C4-verify finds no added
  test file and reports `PASS (green-only)` (`engine/scripts/run-verify.sh:493-504`). The per-fix
  red is carried by the demonstrated reds below.
- **Verification posture:** (b) green-only at C4-verify, by construction; red demonstrated.
  **Demonstrated reds (required, recorded in `build-notes.md`):**
  - `retire:records:` naming every staged part → (C) fails;
  - the published version frozen at fence time → (F) fails;
  - a generation obligation for any existing prior, empty or not → (F2) fails;
  - the bucket precondition dropped from the flip → (H2) fails;
  - a definite flip error answered as `OperationAborted` without a release → (J) fails;
  - an unknown flip outcome treated as `Conflict` → (G0-flip) fails.
- **Production reach:**
  - (a) exercised over a real store in-process, including #653's committer on its first real
    caller;
  - (b) the S3 verb lands with #670 under #508, per 0016's same-release rule (`0016:236-268`);
  - (c) this slice **must not** add a production caller;
  - (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` for every change.
  - **The committer** (#653): the fence key `mpu:<id>`, the flip contribution, and the dirent and
    inode pins with `overwrite_modified` (`docs/design/architecture/05-building-block-view.md:219`).
  - **Records:** `Completion` (`multipart.rs:2141`), `RetireObligation` (`:3913`), `PartScope`
    (`:3173`), `multipart_etag` (`:4392`), `Publication` (`:4548`), `R_PUBLISH` (`:5192`).
  - **The empty-prior rule:** `multipart.rs:3272-3277`, `:3427`.
- **Prior-art check (triage cycles):** the parent's. #653's committer is the landed peer for
  the flip.
- **Disposition hint:** new-feature

## Design

**Publish** through #653's committer. The flip contribution is one batch's worth:
- the session moves to `Completed { completion }`;
- `retire:records:` names only the **published** parts;
- `retire:bytes:` names the unnamed staged parts (bytes and records), if any;
- `retire:bytes:` names a superseded generation **that names at least one chunk** (`0016:663`,
  `:910-925`). A zero-byte prior owns no bytes and gets none: an obligation with an empty chunk
  list would not decode;
- `require` pins the exact `bucket:` marker bytes child-1's validation read.

**Lost flips.** On `Conflict`, re-read the dirent **and the bucket marker**. A marker that is
absent, or not the validated bytes, means the bucket is gone: release and answer `NoSuchBucket`
(H2). Otherwise retry within `R_PUBLISH`, recomputing the version from the re-read prior each time
(`0016:926-941`). On exhaustion, release and answer `OperationAborted`. A definite store `Err` on
the flip: release, then return the stopped arm with the fence released (J). If that release cannot
land, child-1's stopped case with the fence standing applies.

**Docs currency.** In `05-building-block-view.md` (`:244-248`), say that the flip moves the
session to `Completed` in the same batch as the root and its obligations, and that nothing calls
it in production until #670.

## Open questions

- **For sign-off (new at the split):**
  - J is not in the parent; it fills the parent's silence on a definite flip error.
  - K is a temporary refusal that child-3 removes. Its stopped-arm cause is deleted there.
  
  Confirm or redirect.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# core: Complete — segmented publication and the segment-charged budget (636.5c)

> Third of the five slices of #658 (636.5). It builds on child-2 (636.5b, flat publication).
> Do reads ONLY this file. Read 0016 the **Segment write**, **Root flip** and **Fence release**
> rows (`0016:662-664`), **decision 7(b)–(d)** (`0016:2342-2401`), and X58 (`0016:2577`,
> `:1479`) before writing code. Citations were verified at `4bac538`; re-resolve them on your
> base.

- **Slug:** multipart-complete-publish-segmented
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** After child-2, a Complete whose parts total more than `MAX_MAP_CHUNKS` chunks is
  refused, so no object past 158 chunks can be stored by multipart (`0016:2289-2321`). This
  slice makes Complete the first real producer of `ChunkMap::Segmented`. It publishes through
  #653's committer with the group `(session segment_nonce, E+1)`. A fence that wrote segments
  stays charged against `attempts`, because it minted a segment epoch and a rollback obligation
  the cap exists to bound. Its release installs `retire:records:{seg}` for the attempt's group.
- **Success criterion:** New legs appended to `crates/core/tests/multipart_complete.rs`. child-2's
  leg K is replaced by B.
  **(B) Segmented.** With a chunk size small enough that the parts total more than
  `MAX_MAP_CHUNKS` chunks (`multipart.rs:4959`), the published root is `ChunkMap::Segmented` and
  names the group `(session segment_nonce, E)` where `E` is the Completing epoch. The read is
  byte-identical. The session's `segments_written` cursor matches the plan's segment count, taken
  from the committer, never re-derived.
  **(D3) A fence that wrote segments stays charged, and the cap stops the next one**
  (`multipart.rs:5194-5200`). On a fresh session as in B, the wrapper lets every segment batch
  land and answers every flip `Conflict` without applying it. Run `MAX_COMPLETE_ATTEMPTS` (3)
  valid Completes in a row. Each one:
  - fences at a new epoch `E_i`;
  - writes its `seg:` records durably;
  - loses `R_PUBLISH` flips;
  - releases and answers `Refused(OperationAborted)`.
  
  After each one, the session is `Open` and `attempts` is one higher than before (charged, not
  restored). A `retire:records:` obligation naming the group `(segment_nonce, E_i)` exists and
  decodes. The next valid Complete answers `Refused(CompleteAttemptsExhausted)` with **zero
  commits**: no fence, and no `seg:` record of a new epoch.
  **(H2-seg)** child-2's H2, segmented: the marker is deleted after the last segment batch lands.
  Complete answers `Refused(NoSuchBucket)`. The release installed a `retire:records:` obligation
  for `(segment_nonce, E+1)`, which decodes, and `attempts` stays charged.
  **(J-seg)** child-2's J, segmented: released with the segment obligation, charged.
  **(G0-seg)** A segment batch answers `CommitUnknownResult` (once applied, once not). Complete
  returns the in-doubt error carrying it and sends no further commit.
  Every leg of child-1 and child-2 still passes.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. D3's lost flips,
  H2's deletion and G0's outcome are wrapper steps.
- **Invariant to restore:** child-2's (C-1), extended to segments:
  - every `seg:` record a fence wrote is named by the published root or by a `retire:records:`
    obligation installed in the batch that releases that fence;
  - the attempts cap bounds every fence that wrote a segment;
  - a fence that wrote none never spends the budget.
  
  **Sources:** `0016:662-664`, X58 (`0016:2577`, `:1479`), decision 7. SELF-TEST: it binds the
  segment phase, the release and the budget.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-2
- **Conflicts with:**
- **Ordering note:** `Depends on child-2`: it extends child-2's publish phase and removes
  child-2's flat-only refusal. It does not conflict with child-5, which runs in the same wave:
  child-5 edits the pre-fence and frozen-set checks and its own new test file, while this slice
  edits the publish phase and the release's obligation. child-4 builds on this slice.
- **Scope:**
  - the segment group from `attempt_segment_group` (`multipart.rs:2503`);
  - the committer's segment phase with `resume_from` from the session's cursor and a per-batch
    contribution that pins `Completing@E` and moves `segments_written` (`0016:613-620`, `:662`);
  - the release's `retire:records:{seg}` obligation and the charging rule;
  - removing child-2's leg K and its cause;
  - the docs sentence on maps past 158 chunks.
  
  Out of scope:
  - minting the nonce (Create does, #847);
  - recovery after unknown outcomes (child-4);
  - the value ceiling (child-5);
  - the drain (#659), the reaper (#625), the S3 verb (#670), DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** after child-2, leg K: a list past `MAX_MAP_CHUNKS` chunks is refused.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_complete.rs` (appended). C4-verify reports
  `PASS (green-only)`. Its red is carried by the demonstrated reds.
- **Verification posture:** (b) green-only at C4-verify; red demonstrated.
  **Demonstrated reds:**
  - `attempts` restored on every release, including one that wrote segments → (D3) fails;
  - the release's `retire:records:{seg}` obligation omitted → (D3) and (H2-seg) fail;
  - a segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` instead of taken from the
    committer, on a wide-ref fixture → (B) fails (#653's review, `results/issue_653/SUMMARY.md`);
  - an unknown segment outcome treated as `Conflict` → (G0-seg) fails.
- **Production reach:**
  - (a) in-process over a real store;
  - (b) via #670;
  - (c) no production caller;
  - (d) until #659 lands, a published segmented object cannot be deleted or overwritten
    (`crates/core/src/metadata.rs:2148-2151`, `:2188-2193`). That is harmless while nothing in
    production publishes one.
- **Citations expected:** Do cites `path:line` for every change: `attempt_segment_group`
  (`multipart.rs:2503`), the committer's `resume_from` and segment-count API (#653),
  `MAX_MAP_CHUNKS` (`:4959`), `MAX_COMPLETE_ATTEMPTS` (`:5194-5200`).
- **Prior-art check (triage cycles):** the parent's; #653 is the committer this slice is the
  first real caller of for segments.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-3 -->

<!-- pdca:child child-4 -->
# core: Complete — recovery from unknown commit outcomes (636.5d)

> Fourth of the five slices of #658 (636.5). It builds on child-3 (636.5c). Until this slice,
> Complete returns the in-doubt error on every unknown outcome. Do reads ONLY this file. Read
> 0016 `:942-964` and `:1028`, and #895's Abort, `crates/core/src/multipart/abort.rs`
> (`:236-282`, the two kinds of doubt), before writing code. Citations were verified at
> `4bac538`; re-resolve them on your base.

- **Slug:** multipart-complete-unknown-outcomes
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** After child-1 to child-3, any `CommitUnknownResult` on a commit of Complete's
  returns the in-doubt error. That is safe, but coarse in two ways:
  - a flip that **landed** and was answered unknown makes Complete return an error for an object
    it published;
  - an unknown segment write or initial fence that did not land stops a Complete that could
    resume or retry.
  
  0016 requires the publisher to re-read and recover (`0016:942-964`). Two traps must stay closed:
  - an unchanged re-read after `may_still_commit: true` does not prove the held batch will not
    land (`crates/traits/src/lib.rs:240-247`);
  - after an unknown **initial** fence, a re-read showing `Completing@E+1` with the proposed
    bytes is not proof of ownership. A `Completing` record names no request
    (`multipart.rs:2186-2194`).
- **Success criterion:** New legs appended to `crates/core/tests/multipart_complete.rs`. The
  wrapper gains a **hold** step: it keeps a batch with `may_still_commit: true` and lands it later
  (the shape of `crates/core/tests/multipart_abort.rs:961-980`). The G0 cases for the flip, the
  segment batch and the unapplied initial fence are replaced by G1, G4 and G6. The G0 cases for
  the applied fence, the release and the failed read stay.
  **(G1)** The wrapper applies the flip and answers `CommitUnknownResult`. Complete answers
  `Published` once. The inode version advanced once, and exactly one set of obligations exists.
  **(G2) A held flip.** The wrapper holds the flip (`may_still_commit: true`) and answers every
  later commit `Conflict` without applying it, until the rounds run out. Complete returns the
  in-doubt error. Released afterwards, the held flip lands: the object is published once, with
  one set of obligations.
  **(G3) A failed re-read.** After an unknown flip outcome, the re-read of `mpu:<id>` fails.
  Complete returns the in-doubt error, never an answer.
  **(G4) An unknown segment write.** A segment batch answers `CommitUnknownResult {
  may_still_commit: false }`, applied in one run and not in another. Complete re-reads, resumes
  from `segments_written` at the same epoch, writes no `seg:` record twice, and publishes once.
  **(G5) An unknown INITIAL fence never proves ownership.** With the clock fixed, Complete A
  (parts 1, 2) and Complete B (parts 1, 3) both read `Open@E` and build byte-identical fences.
  - **Run 1:** the wrapper holds A's fence, lets B's commit (a definite success), then answers A's
    `CommitUnknownResult { may_still_commit: false }` unapplied.
  - **Run 2:** the wrapper applies A's fence and answers it unknown, then B's fence conflicts.
  
  In both runs, A's re-read finds `Completing@E+1` with exactly its proposed bytes, and A returns
  the in-doubt error with no `seg:` record and no flip. In run 1, B publishes parts 1 + 3 once.
  In run 2, B answers `Refused(OperationAborted)`, nothing is published, and the session stays
  `Completing@E+1` for the reaper (#625).
  **(G6) An unknown initial fence that did not land is retried.** The first fence answers
  `CommitUnknownResult { may_still_commit: false }` unapplied, and the re-read finds `Open@E`
  unchanged. Complete retries the fence (bounded), gets a definite success, and publishes once.
  Every earlier leg still passes.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. Every leg is a
  wrapper step (applied, held, failed-read and same-millisecond rival outcomes), not timing.
- **Invariant to restore:** child-2's and child-3's (C-1), with the recovery clause made exact:
  - a Complete answers only from a state it has read and can attribute to itself;
  - it writes segments or a flip only under a fence a commit of its own answered with a definite
    success;
  - an unknown outcome it cannot settle by a read is returned as the in-doubt error, never
    guessed;
  - a held batch is treated as able to land until a read finds the session moved.
  
  **Sources:** `0016:942-964`, `:1028`; `crates/traits/src/lib.rs:240-247`; #895's Abort.
  SELF-TEST: it binds the fence, segment, flip and release paths together.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-3
- **Conflicts with:** child-5
- **Ordering note:** `Depends on child-3`: G4 resumes child-3's segment phase, and every other leg
  recovers on paths child-1 to child-3 wrote. `Conflicts with child-5`: both edit the code around
  the fence (this slice the fence's unknown-outcome path, child-5 the check before it). The
  dependency already puts them in different waves, and the conflict keeps it that way if child-5
  slips.
- **Scope:**
  - the re-read, resume, retry and settling logic after a `CommitUnknownResult` on the initial
    fence, segment batches, flip and release;
  - the in-doubt error's doubt bookkeeping (the first `may_still_commit: true` outcome is kept,
    as `abort.rs` does);
  - the wrapper's hold step.
  
  Out of scope:
  - any change to the error type's arms (child-1 owns them);
  - any record shape change (none is needed);
  - the value ceiling (child-5);
  - the reaper (#625), the gateway's same-request recovery (#670), DST (#660);
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** n/a (new functionality). After child-3, G1 shows the gap: an applied flip
  answered unknown returns the in-doubt error.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/multipart_complete.rs` (appended). C4-verify reports
  `PASS (green-only)`. Its red is carried by the demonstrated reds.
- **Verification posture:** (b) green-only at C4-verify; red demonstrated.
  **Demonstrated reds:**
  - the flip's unknown outcome returned without a re-read → (G1) fails;
  - an unchanged re-read after `may_still_commit: true` taken as "did not land" → (G2) fails;
  - a failed re-read answered as a refusal → (G3) fails;
  - a resume restarting from segment 0 → (G4) fails;
  - matching fence bytes taken as ownership → (G5) fails;
  - an unapplied unknown fence returned without a retry → (G6) fails.
- **Production reach:**
  - (a) in-process over a real store;
  - (b) the S3 verb (#670 under #508) relies on this contract for its same-gateway recovery;
  - (c) no production caller;
  - (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` for every change. Peers: `abort.rs:236-282`;
  `crates/traits/src/lib.rs:240-247`; the committer's `resume_from` (#653);
  `multipart.rs:2186-2194`.
- **Prior-art check (triage cycles):** #895's Abort solved the same classification for its fence
  (merged as #909).
- **Disposition hint:** new-feature

## Design

**Who owns the fence.** After a `CommitUnknownResult` on the **initial** fence:
- a re-read showing `Open@E` unchanged with `may_still_commit: false` means the fence did not
  land, and it may be retried (bounded, G6);
- a re-read showing `Completing@E+1` with the proposed bytes is **not** ownership: return the
  in-doubt error and write nothing more (G5);
- with `may_still_commit: true` and `Open@E` unchanged, keep the doubt as Abort does. A retried
  fence that answers a definite success settles it.

The cost: a fence that really was this request's is left for the reaper after `W_completing`
(#625), and that attempt stays charged.

**After ownership is proven**, on an unknown outcome under that fence, re-read the session
(`0016:942-964`):
- `Completed` with this request's fingerprint ⇒ `Published` with the recorded publication (G1);
- `Completing@E+1` ⇒ resume from the cursor and flip again (G4);
- `Open` ⇒ answer as a fresh Complete.

Keep a `may_still_commit: true` doubt: every later batch of this attempt requires the session at
the bytes the held batch also requires, so the first of them to land settles it. When the bounded
rounds run out, or a read fails, return the in-doubt error (G2, G3). Never release a fence that
cannot be proven still held.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-4 -->

<!-- pdca:child child-5 -->
# core: Complete — refuse part sets past the value ceiling (636.5e)

> Fifth of the five slices of #658 (636.5). It builds on child-2 (636.5b). Do reads ONLY this
> file. Read `crates/core/src/metadata.rs:549` (`MAX_VALUE_BYTES`), the part-set range encoding
> (`metadata.rs:1976-1977`, `multipart.rs:3184-3193`) and the obligation payload
> (`multipart.rs:3619-3632`) before writing code. #842 found 0016's "stays inside one value"
> claim (`0016:382-384`) false for sparse part numbers. Citations were verified at `4bac538`;
> re-resolve them on your base.

- **Slug:** multipart-complete-value-ceiling
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** Complete's flip writes each part set inside its own obligation value: the
  published parts' `retire:records:` and the unnamed parts' `retire:bytes:`. Part numbers can be
  spread past S3's range (core's format allows up to 999,999, `multipart.rs:939`). Then a set can
  encode past `MAX_VALUE_BYTES`, and the store refuses that flip for good (FoundationDB `2103
  value_too_large`). After child-2, such a Complete fences, is refused at the flip, releases and
  returns the stopped error (child-2's J), every time. The session can never publish, only be
  aborted. The `all` wildcard is reserved for teardown (`multipart.rs:3174-3177`).
- **Success criterion:** A NEW test file `crates/core/tests/multipart_complete_value_ceiling.rs`.
  It runs over a wrapper that refuses any value past `MAX_VALUE_BYTES` (as
  `crates/custodian/tests/restore_completing_fence.rs:473-486` does). Sessions are made by #894's
  Create, and parts are planted as canonical records.
  **(I1)** Parts 1 to 10,000, Complete naming the odd ones: it publishes, and both `{parts}`
  obligations fit (about 58.9 KB each).
  **(I2)** Parts 1, 3, …, 19,999 (10,000 parts), Complete naming all of them (about 129 KB
  encoded): Complete refuses `Refused(SessionTooLarge { bytes, limit })` with **zero commits**.
  The session stays `Open` with `attempts` unchanged, and an Abort then fences it.
  **(I3) The boundary is the stored value, not the bare set.**
  - Parts 1, 3, …, 15,869 (7,935 parts), Complete naming all: the set encodes to 99,981 bytes
    and the `{"parts":…}` value to 99,991, so it publishes. The installed obligation's stored
    length is at most `MAX_VALUE_BYTES`.
  - Parts 1, 3, …, 15,871 (7,936 parts): the set is 99,995 bytes but the value is 100,005, so
    Complete refuses `SessionTooLarge` with zero commits.
  
  If the real payload carries more than `parts`, move the pair so it still straddles the ceiling,
  and record the numbers in `build-notes.md`.
  **(I4) A part added after the first read.** A set fits when Complete first reads it. The
  wrapper plants one more unnamed part just before the fence commits, so the unnamed parts'
  `retire:bytes:` value crosses the ceiling. Complete fences, refuses `SessionTooLarge` against the
  frozen set, and releases (`Open@E+2`, `attempts` restored).
- **Falsifiability:** RED is reachable in-process, and C4-verify sees it. On the reverted base
  (child-2 folded in), Complete exists and the file compiles. I2, I3's second case and I4 then
  fail, because the flip is refused (child-2's stopped error) instead of answering
  `SessionTooLarge`.
- **Invariant to restore:**
  - every value the flip writes fits the store's value ceiling;
  - within S3's part-number range (1 to 10,000), no client error can leave an `Open` session
    unable to publish;
  - outside that range, Complete refuses before it writes anything.
  
  **Sources:** #842's finding against `0016:382-384`; `metadata.rs:549`; `0016:994-1000`.
  SELF-TEST: it binds the pre-fence and the frozen-set checks, so a guard on one alone fails I4
  or I2.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-2
- **Conflicts with:** child-4
- **Ordering note:** `Depends on child-2`: the obligations it measures are the ones child-2's flip
  installs, and the refusal releases through child-1's release. It runs in parallel with child-3
  (see child-3's note). `Conflicts with child-4`: both edit the code around the fence.
- **Scope:**
  - the encoded-value check, before fencing and again against the frozen set, through
    `RetireObligation`;
  - the `SessionTooLarge` doc (`multipart.rs:4515-4526`), extended to say Complete answers it;
  - the new test file.
  
  Out of scope:
  - any change to `UploadPart` or the part-number range;
  - any new `Refusal` variant;
  - other children's legs;
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** after child-2, run I2. Complete fences and returns the stopped error from
  a refused flip, again on every retry.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/multipart_complete_value_ceiling.rs`, a **NEW** file.
- **Verification posture:** (c) red→green at C4-verify.
  **Demonstrated reds:**
  - the check removed → (I2) fails;
  - the check measuring the bare set → (I3) fails;
  - the check run only before the fence → (I4) fails.
- **Production reach:**
  - (a) in-process over a real store;
  - (b) unreachable from S3 once #669 bounds part numbers to 10,000;
  - (c) no production caller;
  - (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` for every change: `MAX_VALUE_BYTES`
  (`metadata.rs:549`), the obligation payload (`multipart.rs:3619-3632`), `RetireObligation`
  (`:3913`), `SessionTooLarge` (`:4515-4526`).
- **Prior-art check (triage cycles):** #842 (the finding). The nearest landed peer is Create's
  refusal of a session that could not later be fenced (#894).
- **Disposition hint:** new-feature

## Open questions

- **For sign-off (carried from the parent, still undecided):** this rule (refuse at Complete,
  reusing `SessionTooLarge`) was chosen at the 2026-10-19 plan review, not by the human. The
  alternative is to refuse the out-of-range part number in core at `UploadPart` (#927's verb). If
  that alternative is chosen, only this child is re-planned.
- **For #669:** the S3 `UploadPart` verb should refuse a part number above 10,000
  (`InvalidArgument`).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-5 -->
