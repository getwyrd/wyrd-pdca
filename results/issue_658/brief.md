# core: Complete — fenced publication, ETag, tombstone answers (636.5) — issue 658

> The Plan artifact for slice 5 of #636. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 3**
> (`0016:895-1036`, the verb × state table at `:973-979`), the **Complete fence**, **Segment
> write**, **Root flip** and **Fence release** rows of the batch inventory (`0016:661-664`), and
> **decision 7(b)–(d)** (`0016:2342-2401`) before writing code. Citations verified against
> `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-19; the passages revised on 2026-10-22
> (D4, F2, H2, the stopped error, the flip's bucket rule) were verified at `4bac538`. This slice builds on two siblings that
> are not on `main` yet: #657 (UploadPart staging) and #653 (the staged-publication committer).
> Both are in wave 1 of this batch; build on the base this bundle is given, which folds them in.

- **Slug:** multipart-complete
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** Parts can be staged (#657) but nothing publishes them. This slice adds
  `CompleteMultipartUpload` as a `wyrd-core` function over `MetadataStore`, answering in
  `CompleteOutcome` (`crates/core/src/multipart.rs:4624`) and `Refusal` (`:4487`): fence the
  session `Open@E → Completing@E+1`, validate the named-part list against the frozen part set,
  assemble the chunk list, publish it through #653's committer (flat at most `MAX_MAP_CHUNKS`
  chunks, segmented above: the first real producer of `ChunkMap::Segmented`), and flip the root
  in one batch with the session's move to `Completed` (recording the ETag and the request
  fingerprint) and the retirement obligations for the parts. A rejected request releases the
  fence, and **a client's mistakes never use up the session's Complete budget**.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_complete.rs`, over
  `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store, and a test-local
  store wrapper that can inject an outcome or a rival write before a chosen commit. Sessions are
  made by #894's Create and parts by #657's staging. It shows all of the following.
  **(A) Publish and read back.** Parts staged **out of order**, at least one **non-final** part
  whose length is not a whole multiple of the chunk size, then Complete naming all of them, answers
  `Published`. `read_object` (`crates/core/src/read.rs:510`) returns the parts' bytes
  concatenated, byte-identical. The inode's ETag and the session's recorded `Completion.etag` both
  equal `multipart_etag` over the named `(part, digest)` list (`multipart.rs:4392`); the session
  decodes as `Completed` with `complete_fingerprint` equal to `complete_fingerprint` of that list
  (`:4423`).
  **(B) Segmented.** With a chunk size small enough that the parts total more than
  `MAX_MAP_CHUNKS` chunks (`multipart.rs:4959`), the published root is `ChunkMap::Segmented` and
  names the group `(session segment_nonce, E)` where `E` is the Completing epoch; the read is still
  byte-identical.
  **(C) Subset Complete** (`0016:1034`). Stage parts 1, 2, 3; Complete naming 1 and 3. The object
  is parts 1 + 3. The flip installed `retire:records:` naming **only** parts 1 and 3 and
  `retire:bytes:` naming part 2 (its bytes and its records); no chunk of part 1 or 3 appears in any
  `retire:bytes:` obligation. Every obligation decodes under `decode_retire_obligation`
  (`multipart.rs:3891`).
  **(D) Typo, then correct.** (D1) `MAX_COMPLETE_ATTEMPTS` (`multipart.rs:5200`) Completes in a
  row with invalid lists (an absent part, a digest mismatch, part numbers out of order) each
  answer `Refused(InvalidPart { .. })` and leave the session `Open`. (D2) **A part replaced
  between the first read and the fence.** `MAX_COMPLETE_ATTEMPTS` Completes in a row, each naming a
  list that is valid against the part set when Complete first reads it, while the wrapper
  re-uploads one named part (new digest) just before the fence commits. Each fences, fails against
  the frozen set, releases, and answers `Refused(InvalidPart { .. })`. After each rejected Complete
  in D1 and D2 the session is `Open` and its `attempts` equals its value before that Complete. A
  following Complete with a valid list answers `Published`, not `CompleteAttemptsExhausted`. After
  a rejected Complete, an Abort answers `Fenced` (`0016:1033`).
  (D3) **A fence that wrote segments stays charged, and the cap stops the next one** (the other
  half of the budget rule; the cap is what bounds a session's segment generations,
  `multipart.rs:5194-5200`). On a fresh session whose parts total more than `MAX_MAP_CHUNKS`
  chunks (as in B), the wrapper lets every segment batch land and answers every flip batch
  `Conflict` without applying it. Run `MAX_COMPLETE_ATTEMPTS` (3) valid Completes in a row. Each
  fences at a new epoch `E_i`, writes its `seg:` records durably, loses `R_PUBLISH` flips,
  releases and answers `Refused(OperationAborted)`. After each one the session is `Open`, its
  `attempts` is one higher than before that Complete (charged, not restored), and a
  `retire:records:` obligation naming that attempt's group `(segment_nonce, E_i)` exists and
  decodes under `decode_retire_obligation`. The next valid Complete answers
  `Refused(CompleteAttemptsExhausted)` with **zero commits**: no fence, and no `seg:` record of a
  new epoch. An implementation that restores `attempts` on every release passes D1 and D2 and
  fails here.
  (D4) **A fence it owns and cannot release** (added 2026-10-22). Set up as D2, so the fence
  lands with a **definite** success and the frozen list is invalid. The wrapper then answers
  every release commit `Conflict` without applying it and without changing the session — the
  conflict no read explains, which TiKV gives when a locking read meets another writer's lock
  (`crates/core/src/multipart/abort.rs:245-247`). No unknown outcome is injected anywhere.
  Complete returns the **stopped** error (see Design § *The stopped error*): it names the upload
  and the epoch of the fence it still holds (`E+1`), its cause says the release rounds ran out,
  and it carries no `CommitUnknownResult`. The session is `Completing@E+1`, byte-identical to the
  fence's bytes; no `seg:` record and no flip was sent; the wrapper counts at most the stated
  bound of release commits. A second run answers the release with a definite store error (an
  `Err` that is not a `CommitUnknownResult`): the same stopped error, with that store error as
  its cause.
  **(E) Answers by state** (`0016:976`). An identical retry against the `Completed` tombstone
  answers `AlreadyCompleted` with the **recorded** publication (same ETag, inode, version); a
  retry naming a different list answers `Refused(NoSuchUpload)`; a Complete against a
  `Completing` session (another publisher's fence) answers `Refused(OperationAborted)` and leaves
  that session record and its `seg:` range byte-for-byte unchanged (`0016:953-958`: only the
  gateway that owns the fence resumes it).
  **(F) The version comes from the re-read prior** (`0016:1025`). Key `K` exists at version `v`;
  the wrapper lands an ordinary overwrite of `K` (version `v+1`) between the fence and the flip.
  Complete retries the flip and publishes `v+2`. When every flip attempt loses (`R_PUBLISH`,
  `multipart.rs:5192`), Complete releases the fence, answers `Refused(OperationAborted)`, and the
  session is `Open`, never left `Completing`. Publishing over an existing generation **that has
  at least one chunk** installs a `retire:bytes:` obligation naming that generation (`0016:663`).
  (F2) **An empty prior owes nothing** (added 2026-10-22). Key `K` exists as a zero-byte object
  (an ordinary write of empty data: an empty plan, committed with an empty chunk map,
  `crates/core/src/write.rs:166-168`, `:277-281`). Complete publishes over it at the next
  version, answers `Published`, installs **no** generation obligation for the empty prior, and
  every obligation it does install decodes under `decode_retire_obligation`; the read returns the
  parts' bytes. A flat generation obligation with no chunks is one the decoder refuses
  (`RetireObligationOwesNothing`, `multipart.rs:3272-3277` via `:3427`), so the rule is "a
  generation with chunks", never "the key existed".
  **(G) Unknown outcomes** (`0016:942-964`, `:1028`). (G1) The wrapper applies the flip batch and
  answers `CommitUnknownResult`. Complete answers `Published` once; the inode version advanced
  once and exactly one set of obligations exists. (G2) **A held flip.** The wrapper holds the
  flip (`may_still_commit: true`) and answers **every** later commit (flip retries and any fence
  release) `Conflict` without applying it, so the session reads `Completing@E+1` unchanged until
  the rounds run out (the shape of `crates/core/tests/multipart_abort.rs:961-980`). Complete returns the in-doubt error
  (see Design), not `Published` and not a refusal. Released after Complete returned, the held
  flip lands: the object is published once, with one set of obligations. (G3) **A failed
  re-read.** After an unknown flip outcome, the wrapper fails the re-read of `mpu:<id>`: Complete
  returns the in-doubt error, never an answer. (G4) **An unknown segment write.** A segment batch
  answers `CommitUnknownResult { may_still_commit: false }`, in one run applied and in another
  not. Complete
  re-reads the session, resumes from its `segments_written` cursor at the same epoch, writes no
  `seg:` record twice, and publishes once. (G5) **An unknown INITIAL fence never proves
  ownership.** With the clock fixed, Complete A (naming parts 1 and 2) and Complete B (naming 1
  and 3) both read `Open@E` and build byte-identical fences (same `fenced_at_millis`, `attempts`
  and target; `Completing` records no request identity, `multipart.rs:2186-2194`). Run 1: the
  wrapper holds A's fence, lets B's commit (B gets a definite success), then answers A's
  `CommitUnknownResult { may_still_commit: false }` unapplied. Run 2: the wrapper applies A's
  fence and answers it `CommitUnknownResult { may_still_commit: false }`, then B's fence
  conflicts. In both runs A's re-read finds `Completing@E+1` with exactly the bytes it proposed,
  and A returns the in-doubt error: it writes no `seg:` record and sends no flip. In run 1 B
  publishes parts 1 + 3, once. In run 2 B answers `Refused(OperationAborted)`, nothing is
  published, and the session stays `Completing@E+1` unchanged for the reaper's rollback (#625).
  An implementation that treats the matching re-read as its own fence publishes twice in run 1.
  **(H) Bucket deleted** (`0016:1035`). With the bucket record removed while the session is open,
  Complete answers `Refused(NoSuchBucket)`, no object is bound, and the session is `Open` again.
  (H2) **Removed after validation, just before the flip** (added 2026-10-22). H alone is caught by
  the validation read, so it passes even with no bucket guard on the flip. Here the wrapper
  deletes the `bucket:` marker after Complete's validation (and, in the segmented run, after the
  last segment batch lands), immediately before the flip commit. The flip pins the marker's
  exact bytes (the `require_bucket` form, `crates/core/src/multipart/create.rs:698-705`; the
  root-flip row's bucket-existence pair, `0016:663`), so it conflicts. Complete answers
  `Refused(NoSuchBucket)` — not `OperationAborted` and not a retried flip; no dirent or inode
  exists for the key; the session is `Open@E+2`. Run it flat and segmented (parts past
  `MAX_MAP_CHUNKS` chunks, as in B): in the segmented run the release installed a
  `retire:records:` obligation naming the attempt's group `(segment_nonce, E+1)`, which decodes,
  and `attempts` stays charged; in the flat run `attempts` is restored. A flip without the bucket
  precondition publishes here, so this leg goes red.
  **(I) Sparse part sets and the value ceiling.** Over a wrapper that refuses any value past
  `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`), as
  `crates/custodian/tests/restore_completing_fence.rs:473-486` does, with parts planted as
  canonical records (staging 10,000 parts is not needed). (I1) Parts 1 to 10,000, Complete naming
  the odd ones: it publishes, and both `{parts}` obligations fit (about 58.9 KB each; S3's
  part-number range 1 to 10,000 cannot do worse). (I2) Parts 1, 3, …, 19,999 (10,000 parts),
  Complete naming all of them, so the published set encodes to about 129 KB: Complete refuses `Refused(SessionTooLarge { bytes, limit })` with zero commits,
  the session stays `Open` with `attempts` unchanged, and an Abort then fences it (`{session,
  all}` fits). (I3) **The boundary is the stored value, not the bare set.** Parts 1, 3, …,
  15,869 (7,935 parts), Complete naming all: the set encodes to 99,981 bytes and the
  `{"parts":…}` obligation value to 99,991, so it publishes. Parts 1, 3, …, 15,871 (7,936 parts):
  the set is 99,995 bytes, under the ceiling, but the obligation value is 100,005, over it, so
  Complete refuses `SessionTooLarge` with zero commits. (Sizes are compact JSON of the range
  encoding, `metadata.rs:1976-1977`, `multipart.rs:3184-3193`; they assume the published-parts
  obligation's payload carries only `parts`. The publishing run asserts the installed
  obligation's stored length is at most `MAX_VALUE_BYTES`; if the real payload carries more,
  move the pair so it still straddles the ceiling and record the numbers in `build-notes.md`.)
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target.
  Every red leg is forced: (F)'s interloper, (G)'s applied, held and failed-read outcomes, (D2)'s
  re-upload before the fence, (H)'s bucket removal and (I)'s value ceiling are wrapper steps, not
  timing; (G5)'s same-millisecond rival fence, (D3)'s lost flips, (D4)'s refused releases and
  (H2)'s marker deletion before the flip are wrapper steps too; (F2)'s empty prior is an ordinary
  write. (D) is the discontinued #636 patch's reproduced defect (three malformed Completes, then a
  correct one answered `CompleteAttemptsExhausted` with every part durable), so it goes red against
  any implementation that spends an attempt on a rejected request; D2 also goes red against one
  that relies on pre-fence validation alone.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*). Every staged part's bytes are, at every instant, named by its `part:`
  record, by the published root, or by a `retire:` obligation installed in the same batch that
  removes the other names; no published chunk is ever named by a `retire:bytes:` obligation; no
  Complete that returns an **answer** (`Published`, `AlreadyCompleted` or a refusal) leaves **its
  own** fence in `Completing`, and a Complete that meets **another** publisher's `Completing`
  fence leaves it untouched; a Complete writes segments or a flip only under a fence a commit of
  its own answered with a definite success, so a session has at most one publisher; only an
  **error** may leave its own fence in place — the in-doubt error (an outcome unknown) or the
  stopped error (a fence it holds for certain and could not release) — for the same gateway's
  recovery or the reaper's rollback (#625); a flip never publishes into a bucket that is gone; every value the flip writes fits the
  store's value ceiling; and, within S3's part-number range (1 to 10,000), no client error can
  leave an `Open` session unable to publish (its only exit Abort and a full re-upload). **Sources:**
  the maintainer's rule 2026-07-25 (via principles.md §5); 0016's refutation standard
  (`0016:2802-2813`); decision 3's scoping rule (`0016:910-925`) and its rejected-Complete rule
  (`0016:994-1000`). SELF-TEST: it binds the fence, the release, the flip's obligations and the
  attempts budget, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 932, 933, 927, 653
- **Conflicts with:**
- **Ordering note:** **RE-POINTED 2026-10-22 — `926` → `932, 933`.** #926 (657.1) was split at its third re-plan
  into #932 (657.1a: slot reserve, owned intent, drain fence, write and body deadlines) and #933
  (657.1b: in-flight renewal and `StagedAttempt::renew`, depends on #932) —
  `results/issue_926/split-proposal.md`. A split parent ends COMPLETE, so `926` would be met at
  once. 
  **RE-POINTED 2026-10-20 — `657` → `926, 927`.** #657 was split at its re-plan
  into #926 (staging: slot reserve, owned intent, drain fence, renewal) and #927 (the part commit
  and the composed `UploadPart`, depends on #926) — `results/issue_657/split-proposal.md`. A split
  parent ends COMPLETE, so `657` here would be met at once and let this bundle build before any
  staging code exists (`src/pdca_harness/flow.py:1616-1620`: dependents are not re-pointed for
  you). `Depends on 926, 927`: Complete publishes the `part:` records #927 writes, and its tests
  stage parts through #927's `UploadPart`. `Depends on 653`: Complete publishes through #653's committer.
  Both are wave 1 of the 2026-10-19 batch; this is wave 2, and #659 (the drain) is wave 3.
  **Intake-cap override (wyrd-pdca-P1):** the human allowed this id in over the alpha cap in the
  2026-10-19 Plan session ("override the alpha cap"); count when written 6/6, room 0, need 1.
- **Scope:** the Complete verb as a store-I/O function in `wyrd-core`: the fence (with the
  attempts budget), validation against the frozen part set, assembly of the ordered chunk list,
  publication through #653's committer with the session's flip contribution, the fence release on
  a rejected request or an exhausted publish, the same-gateway recovery after an unknown outcome
  and the bounded in-doubt error, the value-ceiling check on the flip's part sets (with the
  `SessionTooLarge` doc extended), the session-record transitions it needs (crate-private, like
  `SessionRecord::open`), the
  `retire:` rows it installs (through `RetireObligation`), and the docs sentence. / out of scope:
  - draining any obligation, the terminal delete, and the routing of `unlink` / `PutObject`
    supersede through `retire:bytes:` (#659);
  - the reaper's rollback of a stale `Completing` session and the `W_session` ceiling (#625); the
    operator abort (#633);
  - minting the segment-group nonce: Create already mints it and stores it on every session
    record (`multipart.rs:2271-2287`, #847). Complete only reads it (`attempt_segment_group`,
    `:2503`). Do not add a nonce field or generator;
  - the gateway / S3 `CompleteMultipartUpload` verb and any production caller (#670 under #508);
  - seeded DST races (#660);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality); the defect (D) guards against is in the
  discontinued #636 patch (`/home/eddie/wyrd/wyrd-pdca/results/issue_636/patch.diff`), not on
  `main`.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_complete.rs`, a **NEW** file (C4-verify treats an
  added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new. Complete does not exist on this bundle's base (`main`
  plus #657 and #653), so the discriminating test cannot compile on C4-verify's reverted base and
  C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Built and exercised at Check:** the Complete function end to end over a real store, including
  #653's committer on its first real caller.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - a rejected request counted against `attempts` → (D) fails;
  - pre-fence validation kept, but `attempts` not restored on a post-fence rejection → (D2) fails;
  - `retire:records:` naming every staged part instead of the named ones → (C) fails;
  - the published version frozen at fence time → (F) fails;
  - the flip's `CommitUnknownResult` treated as a loss → (G1) fails;
  - an unchanged re-read after `may_still_commit: true` taken as "did not land" (answering
    instead of returning the in-doubt error) → (G2) fails;
  - the session left `Completing` after a rejected request → (D)'s Abort leg fails;
  - a Complete meeting another publisher's `Completing` fence releases it → (E) fails;
  - the value-ceiling check removed → (I2) fails (the flip is refused by the store);
  - the ceiling check measuring the bare part set instead of the encoded obligation value →
    (I3) fails;
  - `attempts` restored on every fence release, including one that wrote segments → (D3) fails;
  - a re-read matching its own proposed fence bytes taken as ownership after an unknown initial
    fence → (G5) fails;
  - a release that fails definitely answered with the refusal it was releasing for, or reported
    as in doubt → (D4) fails;
  - a generation obligation installed for any prior that exists, empty or not → (F2) fails;
  - the bucket precondition dropped from the flip → (H2) fails.
- **Production reach:**
  - (a) The test exercises Complete for real against a real store. Production completes no
    upload in this slice.
  - (b) The S3 verb lands with #670 under #508, after #659 and the reaper (#625), per 0016's
    same-release rule (`0016:236-268`).
  - (c) This slice **must not** add a production caller.
  - (d) It changes no existing production path. Until #659 lands, a published segmented object
    cannot be deleted or overwritten (`unlink` and the superseding commits refuse a segmented
    prior, `crates/core/src/metadata.rs:2148-2151`, `:2188-2193`); harmless while nothing in
    production publishes one, and #659 is the next wave.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **The committer** is #653's new module on this bundle's base: pass the fence key
    (`mpu:<id>`), a per-batch contribution that pins `Completing@E` and moves `segments_written`
    (`0016:613-620`, `:662`), `resume_from` from the session's cursor, and the flip contribution
    (session → `Completed`, the obligations, the bucket-existence precondition). It pins the
    dirent and inode and applies `overwrite_modified`
    (`docs/design/architecture/05-building-block-view.md:219` names this flip).
  - **Peer for classification after a lost or unknown commit:** #895's Abort,
    `crates/core/src/multipart/abort.rs:282` (re-read; never read `CommitUnknownResult` as
    `Conflict`; keep a `may_still_commit: true` doubt).
  - **Peer for the bucket precondition:** `require_bucket`,
    `crates/core/src/multipart/create.rs:700`.
  - **Records and answers:** `PublishTarget` (`multipart.rs:2119`), `Completion` (`:2141`),
    `SessionState` (`:2173`), `SessionRecord::open` as the model for a crate-private transition
    (`:2360`), `attempt_segment_group` (`:2503`), `RetireObligation` (`:3913`, the writer route
    for every `retire:` row), `PartScope` (`:3173`), `complete_answer` (`:4782`),
    `multipart_etag` (`:4392`), `complete_fingerprint` (`:4423`), `InvalidPart` (`:4445`),
    `Publication` (`:4548`), `R_PUBLISH` (`:5192`), `MAX_COMPLETE_ATTEMPTS` (`:5200`).
- **Prior-art check (triage cycles):** searched by path `crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/tests/` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main`. Merged: the answer table
  and digests (PR #799), the nonce on every session (PR #847), Create (#908), Abort (#909). The
  2026-09-12 tracker comment's decision (store the nonce on the session for the restore fence) is
  already implemented by #847, for every state rather than only `Completing`. Rejected: #636's
  single-slice attempt (discontinued 2026-10-10), whose Complete wedged an `Open` session after
  three malformed requests. The new test file does not exist. Open PRs on getwyrd/wyrd: none.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader; Do need not open them):
the 2026-08-02 comment on the nonce generator, https://github.com/getwyrd/wyrd/issues/658#issuecomment-5157806514,
and the 2026-09-12 design note on storing the nonce,
https://github.com/getwyrd/wyrd/issues/658#issuecomment-5648455120; copies in
`results/issue_658/notes.json`.

## Motivation

Complete is where a multipart upload becomes an object, and it is the only producer of segmented
maps, which is the only way to store objects past 158 chunks (`0016:2289-2321`). Every other
multipart slice exists to make this publication safe: staging (#657) protects the bytes until it,
the committer (#653) writes the map, and the drain (#659) retires what it leaves behind.

## Design

**Fence** (`0016:661`). Read `mpu:<id>`; answer from `complete_answer` (`multipart.rs:4782`) for
every state but `Open`. On `Open@E`: commit `require(mpu == Open@E)` + put the session at
`Completing@E+1` with `fenced_at_millis`, `segments_written: 0`, `publish_target {parent, name,
epoch: E+1}` and `attempts + 1`. At the budget, refuse with `CompleteAttemptsExhausted`.

**Who owns the fence.** A Complete owns the fence only when a fence commit of its own answers a
definite success. Nothing in a `Completing` record names the request that wrote it (`multipart.rs:2186-2194`),
so two Completes that read the same `Open@E` in the same millisecond build byte-identical
fences while naming different part lists. So after a `CommitUnknownResult` on the **initial**
fence: a re-read showing `Open@E` unchanged with `may_still_commit: false` means it did not land,
and the fence may be retried (bounded); a re-read showing `Completing@E+1` with the bytes it
proposed is **not** proof of ownership (a rival can have written them), and Complete returns the
in-doubt error without writing a segment or a flip. With `may_still_commit: true` and `Open@E`
unchanged, keep the doubt as #895's Abort does; a retried fence that answers a definite success
settles it (the held one requires `Open@E` too, so at most one lands). Any other state answers
from `complete_answer`. The cost is that a fence which really was this request's is left
`Completing` until the reaper's rollback after `W_completing` (#625), and that attempt stays
charged; it happens only on an unknown outcome of the fence commit. No record shape changes. The
resume rules below apply only after ownership is proven.

**The budget must count only fences that wrote segments** (the (D) defect). 0016 caps fences per
session because each can mint a segment epoch and a rollback obligation (X58, `0016:2577`,
`:1479`), and separately requires a rejected request to release the fence (`0016:994-1000`). Read
together naively, three typos exhaust the budget. **The rule (required):** the release of a fence
that wrote **no** `seg:` record restores `attempts` to its value before that fence, since such a
fence minted no `seg:` records and no rollback obligation. Every validation rejection happens
before the segment phase, so every one of them restores the budget. Pre-validating against the
part set read **before** fencing is allowed as an optimization (it saves a fence), never as the
budget's protection: a part can be re-uploaded between that read and the fence (`0016:712-717`,
`:758`), so a request that passed pre-validation can still fail against the frozen set, which is
(D2). The cap still bounds every fence that wrote a segment.

**Validate and assemble** (`0016:993-1000`). Against the frozen part set (the bounded `psum:` and
`part:` ranges): every named part present with the named digest, numbers strictly ascending,
the bucket still present. Concatenate the named parts' chunk lists in part order. A rejection
releases the fence: `Completing@E+1 → Open@E+2`, with a `retire:records:{seg}` obligation for
this attempt's group if any segment was written (`0016:664`), and answers the refusal.

**Publish** through #653's committer with group `(segment_nonce, E+1)`. The flip contribution is
one batch's worth: session → `Completed { completion }` (inode, version, the composed ETag,
`completed_at_millis`, `complete_fingerprint`); `retire:records:` naming only the **published**
parts; `retire:bytes:` naming the unnamed staged parts (bytes and records), if any; and
`retire:bytes:` for a superseded generation, if the key existed **and that generation names at
least one chunk or a segment group** (`0016:663`, `:910-925`). A zero-byte prior has an empty flat
map (`crates/core/src/write.rs:166-168`, `:277-281`); it owns no bytes, so it gets no obligation —
one with an empty chunk list would not decode (`multipart.rs:3272-3277`, `:3427`) — and this
slice changes no retirement format to allow one (F2). Each obligation installed under
`require_absent` with a token per `0016:358-373`. The flip also carries the bucket-existence
precondition: `require` on the exact `bucket:` marker bytes Complete read when it validated (the
`require_bucket` form, `crates/core/src/multipart/create.rs:698-705`).

**Lost and unknown flips.** On `Conflict`, re-read the dirent **and the bucket marker**. A
marker that is absent, or not the bytes Complete validated against (a bucket deleted and created
again, as Create reads it, `create.rs:225-228`), means the bucket is gone: release the fence (with
the `retire:records:{seg}` obligation if segments were written) and answer `NoSuchBucket`, with no
further flip (H2). Otherwise retry within `R_PUBLISH`, recomputing the version from the re-read
prior each time (`0016:926-941`); segments are reused at the same epoch. On exhaustion, release
the fence and answer `OperationAborted`. On
`CommitUnknownResult` of a batch sent **under a fence this Complete owns**, re-read the session
(`0016:942-964`, which covers "its own publishing batch"): `Completed` with this request's
fingerprint ⇒ `Published` with the recorded publication; `Completing@E+1` ⇒ resume the segment
phase from the cursor and flip again; `Open` ⇒ the fence was rolled back, so answer as a fresh
Complete would. Keep a `may_still_commit: true` doubt the way #895's Abort does
(`crates/core/src/multipart/abort.rs:255-274`): an unchanged re-read does not prove the held batch
will not land (`crates/traits/src/lib.rs:240-247`). Every later batch of this attempt (a retried
flip, a segment write, the release) requires the session at the bytes the held batch also
requires, so the first of them to land settles the doubt. The same rule applies to an unknown
segment write: re-read the cursor and resume from it at the same epoch.

**The in-doubt error.** Complete is bounded: at most `R_PUBLISH` flip attempts plus a bounded
number of recovery rounds and reads. When those run out with the doubt unsettled, or a read it
needs fails, Complete returns an error (not a `CompleteOutcome`) in the shape of
`AbortError::InDoubt`, carrying the outcome that left the doubt. It never guesses an answer and
never releases a fence it cannot prove it still holds. Its own fence may then still be
`Completing@E+1`; the same gateway's recovery (#670) or the reaper's rollback after
`W_completing` (#625) ends it.

**The stopped error (a known fence it could not release)** (added 2026-10-22). A release can fail
with nothing in doubt: Complete holds its fence for certain (its own commit answered a definite
success), and every release commit then answers a definite `Conflict` that leaves the session
unchanged (the TiKV lock case, `abort.rs:245-247`) or a definite store error. The in-doubt error
cannot report that — its outcome is an unknown commit result by construction, as in
`AbortError::InDoubt` (`abort.rs:121-135`) — and an answer would break the invariant. So the
error type has a second arm, modelled on `AbortError::Stopped` (`abort.rs:109-117`, causes
`:139-159`): **stopped, no batch in doubt, the fence still standing**, naming the upload, the
fence's epoch, and the cause (release rounds spent, or the store error). Releases are bounded:
at most a fixed number of release commits per Complete (Do picks it and cites it, as
`ABORT_ROUNDS` bounds Abort); a `Conflict` with the session unchanged spends one, and a session
found moved is answered from its new state. The same arm covers every release Complete makes —
after a validation rejection, a lost-flip exhaustion, a post-fence value-ceiling refusal, or a
gone bucket. Recovery is the same as for the in-doubt error: the same gateway may retry the release
(#670), or the reaper rolls the fence back after `W_completing` (#625). No record shape changes.

**Part sets inside the value ceiling** (#842 found 0016's "stays inside one value" claim,
`0016:382-384`, false for sparse numbers). The flip writes each part set inside its own
obligation value, and a value over `MAX_VALUE_BYTES` is a batch the store refuses for good
(FoundationDB `2103 value_too_large`). The `all` wildcard is reserved for teardown
(`multipart.rs:3174-3177`), so it is not a way out. **Decided at this revision, for sign-off to
confirm:** before fencing, Complete builds each obligation value it would install (the
published parts' `retire:records:`, the unnamed parts' `retire:bytes:`) through
`RetireObligation` and measures the **encoded value** — the whole payload as stored, not the
bare part set (the payload wraps the set in `{"parts":…}`, `multipart.rs:3619-3632`, and that
wrapper alone moves a 99,995-byte set to 100,005 bytes; leg I3). If any value is over the
ceiling, it refuses `Refused(SessionTooLarge { bytes, limit: MAX_VALUE_BYTES })` without
fencing; extend that variant's doc (`multipart.rs:4515-4526`) to say Complete answers it too.
Repeat the same check (encoded obligation values) against the frozen set after the fence, since a
part added in between can grow it; that refusal releases the fence like any rejection. Within S3's
part-number range (1 to 10,000) no set can exceed about 59 KB, so this refusal is reachable only
by part numbers S3 itself forbids, and the S3 verb (#669) should refuse those at `UploadPart` (see
Open questions). The session stays abortable: Abort writes `{session, all}`.

**Docs currency** (`AGENTS.md:154-157`): add Complete to the multipart paragraphs of
`docs/design/architecture/05-building-block-view.md` (`:244-248`): the fence, that a rejected
request releases it without spending the session's budget, that the flip moves the session to
`Completed` in the same batch as the root and its obligations, that maps past 158 chunks are
published segmented, and that nothing calls it in production until #670.

## Alternatives considered

- **Delete the session record in the flip batch.** A client retry then gets `404` instead of the
  recorded ETag (`0016:1029`). Rejected; the record is the tombstone.
- **`retire:records:` naming every staged part.** It can delete the unnamed parts' records before
  their bytes are marked: bytes left unreferenced and unevidenced (`0016:910-925`). Rejected.
- **Freeze the published version at fence time.** A concurrent overwrite makes every retry
  publish a stale version (`0016:1025`). Rejected.
- **Count every fence against the budget, including rejected ones.** The discontinued patch's
  wedge. Rejected.

## Impact & compatibility

No record shape changes: `Completing`, `Completed`, `PublishTarget` and `Completion` all decode on
`main` already. The `Refusal` enum gains no variant (`SessionTooLarge` is reused; only its doc
grows); the new error type of this function has two arms, in doubt and stopped (Design). No production behaviour changes
until #670.

## Open questions

- **For #668, not this slice:** the production `CreateRuntime::mint` (the caller-supplied nonce and
  upload-id source, `crates/core/src/multipart/create.rs:70-78`) must be a genuine 128-bit CSPRNG
  draw and must not mirror `random_chunk_epoch()` (64 bits, top bit forced). The 2026-08-02
  comment on this issue states why (a weak nonce turns segment-record reuse from impossible into
  merely unlikely). Sign-off should make sure #668 carries that constraint.
- **For #669, not this slice:** the S3 `UploadPart` verb should refuse a part number above 10,000
  as S3 does (`InvalidArgument`). Core's format allows up to 999,999 (`multipart.rs:939`), and a
  session with part numbers spread past 10,000 can hold part sets too large for one value, which
  (I2) makes Complete refuse. With the gateway bound, that refusal cannot be reached from S3.
- **For sign-off:** the value-ceiling rule (refuse at Complete, reusing `SessionTooLarge`) was
  chosen at the 2026-10-19 plan-review revision, not by the human. The alternative is to refuse
  the out-of-range part number in core at `UploadPart` (#657's verb), the way Create refuses a
  session that could not later be fenced (#894). Confirm or redirect.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-19, revision pass): all four findings accepted and the brief
revised. (1) Budget: restoring `attempts` on the release of a fence that wrote no segment is now
required, pre-validation is only an optimization, and (D2) re-uploads a named part between the
first read and the fence. (2) The fence-release invariant is scoped to Complete's own fence; (E)
asserts another publisher's `Completing` session and segments stay unchanged. (3) Unknown
outcomes: a bounded in-doubt error is specified, and (G2)-(G4) add a held flip, a failed re-read
and an unknown segment write. (4) Value ceiling: (I) adds sparse-set tests over a wrapper that
refuses oversized values; Complete refuses `SessionTooLarge` before fencing. That rule is a Plan
choice made in this pass and is flagged for sign-off in Open questions.

Plan-review response (2026-10-20, second revision pass): all three findings hold at `12dd082` and
are revised in place. (1) Fence ownership: `Completing` names no request, so a matching re-read
after an unknown initial fence is not proof of ownership; Complete then returns the in-doubt error
and writes nothing more (Design § *Who owns the fence*; invariant extended). The cost, a real
fence left for the reaper's rollback, is stated. (G5) adds the same-millisecond, different-list
race in both orders. No record shape changes. (2) Charging: (D3) runs `MAX_COMPLETE_ATTEMPTS`
segmented Completes whose flips all lose, checks each stays charged with its epoch's
`retire:records:` obligation, and checks the next one refuses with zero commits; the
refund-everything edit must fail it. (3) Value ceiling: the check now measures the encoded
obligation value, not the bare set, before and after the fence; (I3) adds the boundary pair
(7,935 parts fit at 99,991 bytes; 7,936 refuse at 100,005 although the set alone is 99,995).

Plan-review response (2026-10-22, third revision pass): all three findings hold at `4bac538` and
are revised in place. (1) A fence Complete holds for certain but cannot release now has its own
error arm, modelled on `AbortError::Stopped`: no batch in doubt, fence still standing, epoch and
cause named, releases bounded. The invariant allows either error to leave the fence, never an
answer. New leg (D4) refuses every release with a definite `Conflict` (and, in a second run, a
definite store error) with no unknown outcome injected. (2) The generation obligation is owed
only by a prior with chunks or a segment group. A zero-byte prior gets none, so the decoder's
`RetireObligationOwesNothing` rule is never hit and no format changes. New leg (F2) publishes over
an empty object. (3) The flip pins the validated `bucket:` marker bytes. On a flip `Conflict`,
Complete re-reads the marker as well as the dirent, and a gone bucket releases the fence and
answers `NoSuchBucket`. New leg (H2) deletes the marker just before the flip, flat and segmented.
Each new leg has a demonstrated red.
