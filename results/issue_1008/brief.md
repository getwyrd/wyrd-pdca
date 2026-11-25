# core: Complete — flat publication, lost flips and the bucket guard (636.5b)

> Second of the five slices of #658 (636.5). It builds on child-1 (636.5a, the fence phase).
> Do reads ONLY this file. Read 0016 **decision 3** (`0016:895-1036`, above all `:910-941` and
> `:1025-1035`) and the **Root flip** row of the batch inventory (`0016:663`) before writing
> code. Citations were verified at `4bac538`; re-resolve them on your base.
>
> **Your base must already hold #1007 (child-1) and #1027.** `main` @ `376b106` holds neither:
> `crates/core/src/multipart.rs:153-155` declares only `abort`, `create` and `stage`, and
> `crates/core/src/multipart/complete.rs` and `crates/core/tests/multipart_complete.rs` do not
> exist there. If your base lacks either file, **stop and report it**. Do not write the fence or
> create the test file: they are #1007's (see `Ordering note`). #932, #933 and #653 are merged.

- **Slug:** multipart-complete-publish-flat
- **Track:** alpha
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
- **Depends on:** 1007, 1027
- **Conflicts with:**
- **Ordering note:** **#927 was split on 2026-11-01** into #1027, #1028, #1029 and #1030, and a split parent counts as met at once, so this field now names #1027. That slice lands the part commit and the composed `UploadPart` this bundle needs; the rest of the split only handles lost and unknown commits. Read #927 in this brief as #1027. `Depends on child-1`: this slice composes child-1's fence phase into the
  verb, in child-1's module. #927's `UploadPart` stages the parts these legs publish, and #653's
  committer publishes them. child-3 and child-5 build on this slice. Nothing is left to add to
  `Depends on`: #932 (PR #942), #933 (PR #965) and #653 (PR #930) are merged to `main`.
  **How #1007 reaches the base (a human step before this bundle can build).** #1007 was signed off
  on 2026-10-30 and is COMPLETE, but it has a `patch.diff` and no `publish.json`, no PR and no
  branch, so COMPLETE does not put it on any base. The driver holds this bundle rather than build
  it without #1007 (`_runnable`, `src/pdca_harness/flow.py:681`). Publish #1007 first, then either
  merge its PR to `main`, or name 1007 in the same `pdca flow` command as this bundle so the
  pre-wave carry puts its published branch on the run's line (`_carry_finished`,
  `src/pdca_harness/flow.py:869`). #1027 arrives through the wave fold.
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
- **Test file:** `crates/core/tests/multipart_complete.rs`, appended to the file #1007 creates,
  which must be on the base. On that base C4-verify finds no added test file and reports `PASS
  (green-only)` (`engine/scripts/run-verify.sh:493-504`). The per-fix red is carried by the
  demonstrated reds below.
- **Verification posture:** (b) green-only at C4-verify on a base holding #1007; red demonstrated.
  A patch that adds this test file would mean the base is wrong, and the green-only reading no
  longer applies: stop, as the note at the top says.
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

Plan-review response (2026-11-01, revision pass): the one finding holds. `main` @ `376b106` has no `complete.rs` and no test file, and #1007 has no published branch. The brief now names how #1007 enters the base, makes green-only conditional on that base, and tells Do to stop rather than absorb the fence. Publishing #1007 is the human's step.
