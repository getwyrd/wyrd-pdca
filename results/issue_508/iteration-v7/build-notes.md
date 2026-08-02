# Build notes — issue 508 / multipart-upload (iteration 7)

**Withheld from the reviewer by the driver; written for the human at sign-off.**

## What this iteration is

Iteration 6's patch was rejected with a **specific, cited defect list**, not a rejection of the
approach ("overall finding volume is down from prior iterations"). So this iteration starts from
`iteration-v6/patch.diff` applied to the worktree and fixes, one at a time, (a) the four
adversarial defects plus the stale comment named in the `## Iteration 6 — carry-forward` block,
and (b) every finding in the T4 batch list (`review-batch.md`, 37 rows, 24 distinct sites) that
was not already settled by the plan. The delta over iteration 6 is **1,802 insertions / 223
deletions across 15 files**; the whole patch is 14,112 insertions across 44 files.

Nothing from the rejected shape is re-submitted unchanged: every named defect is fixed in code,
each with a negation run recorded below.

## The carry-forward list, fix by fix

**1. `UploadPartCopy` was silently served as a 0-byte `UploadPart`** (the data-loss one).
`crates/gateway-s3/src/lib.rs:1938` intercepts every multipart form *before* the
`x-amz-copy-source` guard on the plain-PUT arm (`:1998`), so `PUT /b/k?partNumber=1&uploadId=U`
with `x-amz-copy-source` staged a zero-byte part and answered `200` with the SHA-256 of nothing;
a Complete naming it published the destination **empty**. Fixed with one guard at the top of the
multipart interception (`crates/gateway-s3/src/lib.rs`, `has_multipart_marker` block): the form
is `501 NotImplemented`, refused before a body byte is read, exactly as the sibling CopyObject
guard refuses `PUT` with the same header. Out of scope means *refused*, not *mishandled*.

**2. Any recorded D-server drain turned every `UploadPart` into `404 NoSuchUpload` fleet-wide.**
This was flagged as a design call, so here is the call, with what I ruled out.

* The fence itself stays. `stage_chunk_batch` carries `require_absent(desired:dserver:<S>)` per
  placed server (`crates/core/src/multipart.rs`), and 0016 makes it load-bearing: without it a
  drain recorded inside the selection→commit window escapes any filter, the status reads
  `Satisfied`, the operator wipes the disk, and only then does the intent land (the F6 wipe
  trace; leg C(iii)(a) asserts the `Pending` half). Removing it would trade a client-visible
  refusal for a silent wipe, which invariant C-1 puts off the table.
* **0016's re-plan is not implementable at M0**, and this is checkable rather than an assertion:
  `PlacementChunkStore::placement()` — the seam that would name an alternative placement — has
  **no caller anywhere in the workspace** (`rg '\.placement\('` returns only the definition at
  `crates/traits/src/lib.rs:633`). Every write path, ordinary `PutObject` included, hard-codes
  the identity vector (`crates/core/src/write.rs:905`, base shape at `:244`/`:440`). There is no
  second placement to re-plan onto; "exclude the draining server" has an empty candidate set.
  Implementing a selector here would be a placement-policy change to the *whole* write path,
  well outside this brief's Scope.
* So the fix is the half that is this slice's: **the answer**. The staging conflict is now
  classified — a bounded probe of the `desired:dserver:` keys on the conflict path only, at most
  `fragments` point reads — into `WriteError::StagingDrainFenced` (`crates/core/src/write.rs`),
  mapped to a new neutral `MultipartFault::PlacementDraining` and answered
  **`503 SlowDown`**, with an operator `tracing::warn!` naming the drained server. A moved
  session fence still answers `404`. That is the whole of the reported harm: aws-cli abandons a
  transfer on `404` and backs off and retries on `503`, and the session it was told did not
  exist is demonstrably still open — the test asserts both (status **and** that `ListParts`
  still succeeds).
* **Residual, for the human:** while a drain record stands, multipart staging stays refused
  (retryably) on this deployment. Ordinary `PutObject` keeps succeeding because it carries no
  fence — and it keeps writing onto the draining server, which is the same M0 gap seen from the
  other side. A placement selector closes both; it is not this bundle's.

**3. A same-part-number retry racing the original answered `404 NoSuchUpload`.**
`commit_part_batch` preconditions `require_absent(part:<id>:<n>)` when the attempt read no prior,
and `upload_part` mapped *every* commit `Conflict` to `NoSuchUpload`. Fixed in
`crates/server/src/multipart.rs`: the commit is now a bounded loop (`R_PUBLISH` rounds) that, on
a conflict, re-reads the **session** — only a moved fence means "your upload is gone" — and
otherwise re-reads the prior `part:` record and commits again, so last writer wins as real S3
does. Persistent contention answers `503 SlowDown` after compensating, never a false 404.

**4. The "second Abort is 204" assertion was a race.** Two changes. The test now polls for the
held part's `sidx:` entry before the first Abort (the sibling lifecycle test's pattern) — but
that alone does **not** make the cell observable, and I want that on the record: the teardown
drain is spawned *before* Abort's response is written (0016 F9), and in-process it routinely
finishes inside the round trip, so after the poll the second Abort still answered `404`. The
assertion is therefore now on the **agreement between the answer and the store**, read after the
answer: `204` is the idempotent `Aborting` cell; `404 NoSuchUpload` is legitimate **only** if the
session record is gone (nothing recreates one). That is a strictly stronger oracle than either
cell alone — it catches a `404` answered while the session is right there, which is the defect
class — and it has no window. Flagging it because it is a *weakening* of the brief's literal
"a second `Abort` → **204**": the 204 cell is real and reachable, but not deterministically
reachable from a wire test against an in-process gateway.

**5. Stale derived value.** `MAX_STAGED_CHUNKS`' comment said `51_480`; `312 × 156 = 48_672`
(`crates/core/src/multipart.rs`).

## The T4 batch list (37 rows, 24 distinct sites)

Fixed in this iteration — each row of `review-batch.md` maps to one of these:

| Site | Fix |
|---|---|
| `core/write.rs:944` (×3) | An indeterminate intent commit is no longer *assumed* to have landed: the `sidx:` entry it writes is re-read and is its own witness, so compensation preconditions on the slot bytes the store actually holds. An unreadable re-read keeps the conservative assumption (documented: the two errors are not symmetric). |
| `custodian/reconstruction.rs:821/829` (×3) | `find_staged_chunk` decodes the **re-read** bytes and re-locates the chunk in them, so a concurrent part re-upload can no longer pair a stale record with the new CAS image and overwrite the live part with the superseded one. |
| `core/read.rs:520` (×2) | A concurrently-retired generation is a **restart**, not an absence: `read_object` re-resolves up to `RESOLVE_ATTEMPTS`, the constant now defined once in `core::read` and shared with the gateway's streaming resolve (it was a private duplicate). |
| `gateway-s3:2351/2365` (×3) | `UploadPart` now reads and **enforces** the declared length (`x-amz-decoded-content-length` for `aws-chunked`, `Content-Length` otherwise) through a new `declared_length` parameter on the seam, refusing a mismatch with `IncompleteBody` before the `part:` record exists — mirroring `put_object_streaming`'s identical check. |
| `gateway-s3:2156` (×3) | `parse_byte_count`: OWS-trimmed `1*DIGIT` only, so `+1` no longer parses as 1. Unit test extended with `+1`/`0x10`. |
| `gateway-s3:2550` (×2) | `ListMultipartUploads` is paged: `max-uploads`, `key-marker`/`upload-id-marker` (a lone id-marker is refused), computed `IsTruncated`, `NextKeyMarker`/`NextUploadIdMarker`. The seam returns `UploadListing`. |
| `server/multipart.rs:636` | An indeterminate **fence** commit re-reads the session and adopts its own `Completing@E+1` instead of leaving the upload wedged with no owner. |
| `server/multipart.rs:878` | An indeterminate **segment-write** commit re-reads and adopts the record it would have written, so the fence release still CASes on what the store holds. |
| `gateway-s3:1859` (×2) | Bucket-level `?uploads` refuses `400 InvalidArgument` when combined with `uploadId`/`partNumber` — the object path's rule, applied to the bucket path. |
| `gateway-s3:2597` | Duplicate `<PartNumber>`/`<ETag>` children are `MalformedXML`, not last-value-wins. |
| `gateway-s3:2608` | `entity_tag_value`: exactly one pair of quotes or none, no embedded quote — replacing `trim_matches('"')`. `<PartNumber>` gets the same digits-only treatment. |
| `core/multipart.rs:2497` | A mark evidencing bytes that do not exist is now **retired** by GC once its grace window elapses (fleet-visible server only), so a compensation's fragment-less marks stop accumulating for ever. |
| `core/multipart.rs:2745` | `orphan_leases` walks the ledger in **cursor-keyed pages** (`scan_page`) instead of one `SCAN_CAP`-bounded `scan`, so a ledger grown past the cap no longer disables reclamation deployment-wide. |
| `core/multipart.rs:2764` | `commit_units` reserves the terminal obligation/marker deletes in its per-batch budget, so the final batch cannot exceed `B_OPS` (an over-budget batch re-derives identically for ever — a permanent leak). |
| `custodian/gc.rs:191` | **The reclaim handshake**: GC CASes an orphan mark to `reclaiming` *before* deleting the bytes; adoption sites delete the pre-mark under an exact-value `require`. A re-placement racing a reclaim now conflicts and re-plans instead of publishing a placement naming deleted bytes. Claims are batched at `B_OPS/2`. |
| `custodian/rebalance.rs:435` | Adoption requires the pre-mark's exact bytes **and** `require_absent(desired:dserver:<target>)`. |
| `custodian/reconstruction.rs:669` | Adoption requires the pre-mark's exact bytes. Deliberately **no** drain fence here: reconstruction selects from the whole topology with no exclusion, so a destination that later drained would block the repair for ever — availability first, and the rationale is in the code. |
| `core/multipart.rs:837` | Segment resolution validates the record's own `byte_offset`/`byte_len` against the root's table, not just the chunk span. |
| `custodian/gc.rs:558` (×2) | An unparsable `orphan:` key or value is an **error**, not a silent skip (a skipped mark is a permanently unreclaimable fragment). |
| `core/multipart.rs:424` | `parse_scoped_number` requires the canonical fixed width and digits only; the unused lax `parse_part_number` is deleted; `parse_sidx_suffix` gets the same rule. |
| `core/multipart.rs:1256` | `list_owned_staging` enforces the **joint** `owner`/`staged` invariant, so an entry naming another session under this session's prefix is refused rather than attributed. |
| `custodian/gc.rs:495/496` (×2) | A malformed staged placement joins the same fully-referenced `malformed` class a malformed committed one does (new `StagedPlacement::checked_fragments`), instead of being identity-filled. The committed half of the staged class gets the same gate. |
| `core/multipart.rs:1111` | `expand_part_ranges` validates canonical form (ascending, gap-separated, inside `1..=MAX_PARTS_PER_SESSION`) **before** allocating, so a corrupt payload cannot ask for a 16 GB `Vec`. |

**Recorded-rejected** (in `review-rejected.md`, with citations): `server/src/lib.rs:183` (the
reaper warning — settled by the brief's Open question 3, and the admission-exhaustion harm it
descends from was closed in iteration 6), plus the two carried-forward `write.rs:969` /
`multipart.rs:163` rows.

## Forced self-refutation

**(a) Genuine red?** Yes — five targeted negation runs, each reverting exactly one fix and
re-running its test:

| Negation | Result |
|---|---|
| Remove the `x-amz-copy-source` guard | `out_of_scope_and_ambiguous_multipart_forms_are_refused_not_mishandled` FAILS: `left: (200, "<no Code>")`, right `(501, "NotImplemented")` — the zero-byte part is staged, i.e. the reported data loss. |
| Map every part-commit `Conflict` to `NoSuchUpload` | `a_same_part_number_retry_racing_the_original_is_not_a_lost_upload` FAILS with the SDK's `404 NoSuchUpload`. |
| Report the drain fence as `NoSuchUpload` | `a_drain_over_a_live_uploads_bytes_reports_pending_never_satisfied` FAILS: `left: Some(404)`, right `Some(503)`. |
| `list_multipart_uploads` ignores `max`/truncation | `open_upload_listing_is_paged_and_its_truncation_is_computed` FAILS on the row count. |
| Delete bytes without claiming the mark | `a_reclaim_claims_its_mark_first_and_retires_evidence_for_absent_bytes` FAILS: the mark is byte-identical after the pass (`b"100"`), so an adoption preconditioned on it would still win. |

Every fix was restored and the suites re-run green after each negation.

**First-pass failure worth recording:** the *original* version of the same-part-number test drove
two concurrent `upload_part(1)` calls and **passed with the fix reverted** — the real race window
is the few microseconds between one `get` and one `commit`, so the test was inert. It was
rewritten to produce the interleaving deterministically through the store fixture
(`MemMeta::hide_next_read`: the next read of that one key answers absent while the store keeps
the real value and the commit evaluates the real precondition). That is the only reason the
negation now bites.

**(b) Production path?** Yes. Both wire suites drive the real `S3Gateway` over a real TCP
listener with a real `aws-sdk-s3` client (or hand-signed raw requests for forms an SDK cannot
spell), against `Gateway` over `FsChunkStore` + `MemCoordination`. The custodian test drives the
real `reconcile_step` fenced control point over the real `gc::reconcile`. No behaviour is
re-implemented in a test; the only doubles are stores (`MemMeta`, `MemDServer`), which is the
suite's existing shape.

**(c) Fixture includes the fault?** Yes.
* The copy-source test asserts the refusal **and** that `ListParts` shows no part — the failing
  element (a staged empty part) would be in the fixture if it were created.
* The drain test records a real `desired:dserver:0` drain on the **only** server the placement
  uses, with a genuinely in-flight part on it (`fleet.total_fragments() > 0` is asserted).
* The reclaim test seeds a real fragment on the D server plus a real mark, and injects the
  failure through a `delete_fragment` that refuses — so the pass genuinely stops between claim
  and delete, and the phantom mark's position genuinely holds nothing.
* The same-part-number test leaves the first attempt's record in the store; the second attempt's
  commit evaluates `require_absent` against **that** record.

## Gate evidence

* `./engine/xtask.sh ci` (fmt / clippy / build / test / deny / conformance / prose) —
  **`xtask ci: all checks passed`** on the final run. Two intermediate failures were fixed
  rather than suppressed:
  `cargo fmt --check` (formatting of the appended custodian test) and one pre-existing unit test
  (`a_malformed_declared_length_fails_closed_rather_than_reading_as_absent`) that pins
  `" 64 "` → `Ok(Some(64))`. The second one is a real signal and changed the fix: `parse_byte_count`
  strips OWS (which the field grammar allows and hyper already does) and then requires
  `1*DIGIT`, so `+1` is refused without changing the whitespace behaviour a sibling test pins.
* The two named test binaries: `s3_multipart_upload` 14 passed, `s3_multipart_lifecycle` 8
  passed. `wyrd-custodian` 10 passed in `tests/gc.rs` (incl. the new one); `wyrd-core` unit
  tests green.
* **RED-leg discipline, verified rather than asserted.** The brief's gate hazard is that a
  *compile error* in an added test file is scored as a red over a build that ran nothing, so I
  reproduced the red leg myself: a detached worktree of `origin/main` @ `22d71b4` with **only the
  two added test files** copied in.
  * `cargo check -p wyrd-server --test s3_multipart_upload --test s3_multipart_lifecycle` →
    **exit 0**: both files compile against the base, so neither references anything the patch
    adds. (The new helpers they use — `raw_with`, `MemMeta::hide_next_read` — are local to the
    test file and stand on base-visible traits only: `MetadataStore`, `WriteBatch`,
    `CommitOutcome`.)
  * `cargo test …` on that same base tree → **22 tests ran, 22 failed**:
    `s3_multipart_upload` 0 passed / **14 failed**, `s3_multipart_lifecycle` 0 passed /
    **8 failed**. The red is assertions, not a build error, and it is earned by tests that
    actually executed.
  * The scratch worktree was removed afterwards (`git worktree remove --force`); nothing was
    left under `$PDCA_SCRATCH`.
* The one test that *does* reference a patch-added symbol (`decode_orphan_mark`) is in
  `crates/custodian/tests/gc.rs` — an **existing, modified** file, which `run-verify.sh` does not
  add to its invocation.

## Scratch discipline

No scratch directories were created; all work happened in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`). One self-inflicted incident worth recording: a
`git checkout -- crates/gateway-s3/src/lib.rs` (intended as "undo my negation edit") reverted the
file to `origin/main`, discarding both my edits **and** the applied iteration-6 patch for that
file. Recovered with `git apply --3way --include=...` of `iteration-v6/patch.diff` followed by
re-applying the eight edits; the result was re-verified by the full compile, both wire suites and
the negation runs above. Every later negation used a reversible `python3` string replacement
instead.

## For the human at sign-off

1. **Open question 1 (slice size) is still open, and this is the seventh attempt.** The patch is
   14 K insertions across 44 files. Do does not split; the natural seams the brief lists are
   unchanged, and the delta this iteration touches three of them (the gateway wire surface, the
   custodian reference-set/GC changes, and `core`'s records).
2. **The drain-fence posture (carry-forward 2) is a decision, not a fix**: refuse retryably and
   tell the operator, because M0 has no placement selector to re-plan onto. If you want the other
   answer — refuse the drain request while any session holds staged bytes, or implement a
   selector — that is a bigger change than this bundle.
3. **Open question 3** (warn vs. hard startup refusal for a missing reaper) is still yours; it is
   recorded-rejected against the T4 list on the strength of the brief's own wording.
4. **The deferred large-object leg stands**: `aws s3 cp` of an 8+ GB file against a deployed
   stack, `sha256`-identical, confirmed by hand at sign-off or post-merge (brief's
   `Verification posture`).
