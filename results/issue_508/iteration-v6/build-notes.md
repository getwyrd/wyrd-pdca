# Build notes — issue #508 (multipart-upload), iteration 6

Withheld from the reviewer; written for the human at sign-off.

## What this iteration is

Iteration 5 was rejected on **three cited defects** from sign-off plus C3/T2/T3 from the advisory
review and **33 unchecked T4 rubric rows**. This pass keeps iteration 5's protocol implementation
(the shape 0016 settles; its 19 wire tests were green and remain green) and fixes every named
item. It is *not* a re-submission: two of the three sign-off defects were **permanent
failure modes** (a silent record leak above 4,000 parts; a store that stops admitting multipart
uploads for ever after ~70 successful ones), and closing them changed the drain's batching model,
the admission record's shape, the ordinary-PUT staging path, the reconstruction scan order, the
rebalance evidence rule and two metadata seams.

Read order I actually used: `brief.md` (including both carry-forward blocks) → proposal 0016 (the
admission record `:348`, the state machine `:528-601`, the batch inventory `:603-689`, the
tombstone rule `:964-968`, decision 2's per-consumer table `:820-890`, the knob table
`:1462-1479`) → `AGENTS.md` §"Review rubric & protocol" → `review-batch.md` (33 rows) →
`iteration-v5/check-review.md` + `check-advisory-adversary.md` → `iteration-v5/build-notes.md`
(to know what iteration 5 had already fixed and why).

## The three sign-off defects, and what closed each

### 1. Silent permanent part loss above 4,000 parts in one Complete

`drain_records`' `Parts` arm stops enumerating at `DRAIN_BATCHES_PER_PASS × B_OPS / 2` = 4,000
units, and `commit_units` deleted the obligation as soon as it had committed everything *it had
been handed* — so every `part:`/`psum:` record past the cut lost its only deleter, permanently,
with the ledger entry gone.

Fixed by making the truncation an explicit contract: `drain_records` sets `complete = false`
(`crates/core/src/multipart.rs:2601`,`:2642`) and `commit_units` takes it as
`derivation_complete` (`:2686`), initialising `exhausted` from it — so a partial pass may commit
work but may never retire the obligation.

**The cut also moved, and that is the load-bearing half.** My first version of this fix was
*inert*: I reverted the flag and the test still passed. The reason is worth recording — the old
cut (4,000 units) sat *just above* what one pass can commit (~3,992 units at two keys each), so
the batch budget always tripped first and the truncation branch was unreachable. A flag guarding
an unreachable branch is not a fix; it is dead code that will rot. The cut is now
`DRAIN_BATCHES_PER_PASS × B_OPS / 4` (2,000 units, `:2642`), deliberately **below** one pass's
commit capacity, so the partial path runs on every large drain and the flag's absence is a
test-visible leak. Cost: one extra drain pass per ~2,000 parts.

The regression test is at the boundary the carry-forward named — **4,001 parts**
(`crates/core/src/multipart.rs:3268`, `a_records_obligation_past_the_derivation_cut_is_not_dropped_mid_way`):
one step must leave the obligation standing, and repeated steps must delete every named record
*before* the obligation goes.

### 2. Permanent multipart-admission exhaustion (~70 lifetime uploads)

`mpuctl.count` is decremented only by the terminal session delete, and a `Completed` session's
terminal delete is #625's `W_tombstone` exit — which this wave does not ship. So a successful
upload never released its admission slot and `CreateMultipartUpload` refused everything after
`max_sessions` = ⌊4,000,000 / 56,760⌋ = 70 *lifetime* uploads. 0016 does say tombstones are
counted (`:964-968`), but it bounds them by a window this slice does not own, so the composition
with the confirmed wave order is a permanent, service-wide refusal — which invariant C-1 puts off
the table.

**The fix splits the count along the two resources `mpuctl` actually protects** rather than
weakening either (`crates/core/src/multipart.rs:456-479`):

* `staged` — sessions still owning `part:`/`psum:`/`sidx:` records, the only ones that spend the
  reconcile pass's `W_ref` staged-reference memory. Bounded by `max_sessions` (≈ 70, 0016's
  derivation unchanged). Released the moment a session's obligations drain.
* `count` — `mpu:` records in **any** state, tombstones included, exactly 0016's definition: the
  population every bounded `scan("mpu:")` must enumerate. Bounded by a new `max_records`
  (`:212-224`: `min(W_SESSIONS, SCAN_CAP/2)` = 65,536 records ≈ 20 MB at the worst-case record
  size). Released only by the terminal delete — #625's job for a tombstone.

A tombstone therefore holds an `mpu:` key but not a staged-reference slot. The release is
`release_admission_batch` (`:2069`), driven from the drain's `teardown_sessions` walk (`:2827-2856`),
and it is **exactly once** by the same construction the terminal delete uses: `require(mpu ==
prior)` plus an `admission_released` marker written in the same batch (`:567`), which
`terminal_delete_batch` reads off the very bytes it preconditions on (`:2016-2040`) so a
gateway drain and the reaper cannot both release one slot.

**Why I did not just raise `W_ref`:** `W_ref` is a memory budget for the reconcile pass's
reference set (4 M chunk-refs ≈ 400 MB resident), and `U_ref` charges each session its
*worst-case* footprint deliberately (0016:1470, the iteration-4 defect). Inflating either
trades a real OOM for a paper capacity. Charging zero-footprint tombstones against a memory
budget they do not occupy is the actual error.

**What this does to the wave-0 posture, stated plainly:** without a reaper, tombstones still
accumulate, so a reaper-less deployment now refuses Create after 65,536 uploads instead of 70. It
is the same class of misconfiguration the startup warning already names (`warn_if_reaper_absent`),
and the released stack carries #625 by the settled open-question-4 closure. This is a deviation
from 0016's single-counter text, so it is a **NEEDS-HUMAN proposal-correction item** below,
alongside the segment-group-nonce ruling.

### 3. Ordinary-PUT size-limit bypass via an omitted `Content-Length`

Two holes, both closed:

* **No staging ceiling.** `stream_write_data` now takes `max_chunks` and refuses *at the ceiling*
  with `WriteError::MapCeiling` (`crates/core/src/write.rs:550`,`:580`,`:604`), before the
  offending chunk is leased; the gateway passes `mp::MAX_MAP_CHUNKS`
  (`crates/server/src/lib.rs:558`) and maps the refusal to `400 EntityTooLarge` (`:570`). A
  lengthless `aws-chunked` stream declares nothing, so this is its *only* ceiling — previously an
  oversized stream was staged in full and failed (if at all) at the commit.
* **A declared length was never enforced.** `put_object_streaming` now refuses
  `GatewayError::IncompleteBody` when the streamed byte count differs from the declaration
  (`crates/server/src/lib.rs:583`), before the commit. HTTP framing covers `Content-Length`, but
  nothing covered `x-amz-decoded-content-length` unless the client also sent a checksum trailer
  (`streaming.rs:532-538`) — so a client could declare one size, stream another, and have the
  object published.

### C3 — malformed `x-amz-decoded-content-length` fails closed

Extracted as `declared_object_length` (`crates/gateway-s3/src/lib.rs:2142`): absent is legal (a
lengthless stream), *present-but-unparsable* is now `400 InvalidArgument`. Swallowing it with
`.ok()` dropped the request into the lengthless path — sizing the map from a 5 GiB assumption and
losing the length check the declaration exists for.

### T2 — double percent-decoding of multipart query keys

`part-number-marker` / `max-parts` were looked up with `query_param` (which matches the **raw**
key and returns an **already-decoded** value) and then decoded again; the `?uploads` prefix the
same. Both now use `decoded_query_key` once (`crates/gateway-s3/src/lib.rs:653-680`,`:2542`), so an
encoded key spelling is honoured and a literal `%2F` in a value stays `%2F`.

### T3 — `UploadPart` latency coupled to the global drain backlog

`upload_part` ran the whole-namespace drain on every successful commit. It now drains **its own**
obligation by key, and only when a re-upload installed one: `mp::drain_one`
(`crates/core/src/multipart.rs:2244`) called at `crates/server/src/multipart.rs:551`. Cost is
proportional to what the request itself created.

## The T4 rubric rows (33)

Deduplicated they are ~20 distinct findings; all fixed except the three in `review-rejected.md`.
The non-obvious ones:

- **`ChunkRef` base estimate (row `multipart.rs:70`).** The 113-byte base under-counted by 18 —
  the real worst case is 131 B (`u128` id, `ReedSolomon` tag, `u64` len), so a 9-fragment RS(6,3)
  chunk encodes to 320 B while every ceiling claimed 302: the ceilings were unsound *by one
  fragment*. `MAX_CHUNKREF_BYTES` is now **derived** from measured constants
  (`crates/core/src/multipart.rs:64-95`) and pinned by a test that encodes the worst case
  (`the_chunkref_byte_budget_covers_the_worst_case_encoding`) — so it cannot drift from the type.
  `MAX_MAP_CHUNKS` consequently drops 165 → **156** (below 0016's printed range, which was
  computed from the under-count; the safe direction). `check_durability_config` was also wrong in
  the other direction — it refused any scheme wider than the default; it now refuses only schemes
  whose full record would cross the backend **value ceiling** (`:2973`), so an existing RS(10,4)
  deployment still starts.
- **Per-fragment *and* per-precondition operation budget (rows `:2201`/`:2434`).** A position
  expands to one `orphan:` mark per fragment, and `guard_orphan_mark` adds a `require` beside each
  put — so a "1,000-operation" batch was carrying 2,001. Retirement units are now **splittable**:
  `commit_units` fills batches from an ordered `DrainOp` stream (`:2354`) where a unit's record
  deletions follow its marks, so a legal maximal part (156 chunks × 9 fragments) drains in
  budgeted batches instead of one permanently-oversized one. **This defect was found by the test,
  not by reading**: the new `EnvelopeStore` (a store that refuses an over-budget batch, as
  FoundationDB/TiKV effectively do) rejected a 1,999-operation batch on the first run.
- **Compensation state (rows `write.rs:897`/`:900`).** A fragment fan-out failure happens *after*
  the `sidx:` intent and the advanced slot bytes commit. `stage_one_chunk` now returns
  `StageFailure { err, committed }` (`crates/core/src/write.rs:875`) and the caller records the
  partial staging before compensating (`:806`,`:845`) — previously the compensation CAS
  preconditioned on stale slot bytes and omitted that chunk's fragments.
- **Indeterminate commits (rows `multipart.rs:1299`, `server/multipart.rs:303`).** Both are now
  settled by a re-read rather than propagated: `reserve_slot` adopts a reservation that landed by
  matching its own `attempt_id` (`crates/core/src/multipart.rs:1430`), and `create_multipart_upload`
  returns the session that landed rather than minting a second, unreachable one
  (`crates/server/src/multipart.rs:305`).
- **Server faults reported as `400 InvalidArgument` (row `server/multipart.rs:430`).** Only
  client-attributable causes are mapped now; everything else keeps its type so `classify` maps it
  (`crates/server/src/multipart.rs:462`).
- **Reconstruction TOCTOU + scan order (rows `:758`/`:791`/`:807`).** `find_chunk` reads the
  **staged** class first (`crates/custodian/src/reconstruction.rs:749`, 0016's
  source-before-destination rule) and the staged arm re-decodes the session from the bytes its CAS
  will pin (`:805`) — the stale `list_sessions` image could pass an `is_open()` check while the
  CAS matched a `Completing` record perfectly.
- **Rebalance (rows `:223`/`:338`/`:383`).** Destination positions are pre-marked before a byte is
  copied (`crates/custodian/src/rebalance.rs:332`) — the base's "the copied fragments are now
  collectable garbage" was untrue, since unreferenced *and* unevidenced is what GC retains for
  ever — and `EvacLocation` shares its owning record through an `Arc` (`:208`,`:222`) instead of
  deep-cloning it per chunk (gigabytes for a maximal segmented map).
- **`sidx:` entries with no placement (rows `gc.rs:476`/`:487`).** `list_owned_staging` fails
  closed (`crates/core/src/multipart.rs:1256`), so every consumer inherits it; a skipped entry's
  live fragments would have been orphan-marked by the next restore pass and deleted by the GC pass
  after it. A malformed *committed* staged placement is now reported via `emit_malformed`
  (`crates/custodian/src/gc.rs:436`) rather than silently skipped.
- **`HEAD ?uploadId` (rows `:650`/`:673`)** is no longer classified as ListParts — S3 names no
  such operation, so it takes the same `400 InvalidArgument` refusal every other unmatched
  multipart form takes (`crates/gateway-s3/src/lib.rs:653`).
- **Streaming-signed Complete body (rows `:2312`/`:2342`).** The body is de-framed through
  `streaming::decode` and buffered under the same cap (`buffer_stream_capped`,
  `crates/gateway-s3/src/lib.rs:2391`), so a legitimate `aws-chunked` document parses and a forged
  chunk signature is refused `403` via `classify` — previously the framing was parsed as XML and
  the per-chunk signatures were never checked.
- **Uppercase hex (row `:1002`).** `unhex` accepts lowercase only
  (`crates/core/src/multipart.rs:1073`): `to_digit(16)` let `…AB` and `…ab` compose the *same*
  published ETag.
- **Poisoned drain-task mutex (row `server/src/lib.rs:136`).** Recovered through
  `PoisonError::into_inner` (`crates/server/src/lib.rs:144`,`:155`) instead of dropping the handle,
  which detached the task the rubric requires be aborted on drop.
- **Bounded-step test gap (row `:2825`).** The old test seeded 64 obligations against an
  8,000-obligation page budget and asserted only "some progress" — it would have passed against an
  unbounded step. It now drives a 9,000-part range and asserts the step **leaves work pending**
  and stays inside one pass's operation budget (`crates/core/src/multipart.rs:3219`).

## The T4 conformance review's DST gap (X67, X59)

`iteration-v5/check-review.md` T4 FAIL named two missing seeded properties. Both are appended to
the **existing** `crates/dst/tests/custodian.rs` (never a new `#![cfg(madsim)]` file beside the
added server tests) and both are in `REGRESSION_SEEDS`' sweep:

- `prop_publication_handoff_never_drops_a_repair` (`:1863`) — **X67**. A `HandoffMeta` store
  (`:1814`) lands the **real** publication batch at the instant the pass reads the staged class,
  so the interleaving is the production one. The assertion is the disjunction that matters: the
  repair obligation may be drained only if the chunk was actually repaired.
- `prop_drain_request_versus_intent_admits_exactly_one` (`:2005`) — **X59**. The seed picks which
  racer commits first; an intent that wins must leave the drain reporting `Pending` (the F6 wipe
  trace), and one that loses must fail its `require_absent(desired:dserver:<S>)` and leave no
  staging entry.

`prop_publication_handoff_never_drops_a_repair` was **inert on its first version** (interposing on
the `inode:` scan let the broken order see the just-published inode). Recorded because it is the
same failure mode as the drain flag above: the injection point now sits where both orderings
differ, and reverting `find_chunk` to destination-first reds it (see below).

## Numbers the brief asks me to record

**RED leg** (both added test files, unmodified, copied into a throwaway `git worktree` at
`origin/main` @ `22d71b4`; scratch dir removed afterwards):

- both files **compile** against the base (`cargo test` reached the running phase) — the gate
  hazard the brief names did not fire;
- `s3_multipart_upload`: **11 tests ran, 11 failed**;
- `s3_multipart_lifecycle`: **8 tests ran, 8 failed**;
- total **19 ran / 19 failed**, every one an assertion panic at
  `crates/server/tests/s3_multipart_*.rs:<line>`, not a build error.

**Leg D arithmetic** (re-checked against the new ceiling): 64 KiB chunks × six 5 MiB parts ⇒
⌈5 MiB / 64 KiB⌉ = 80 chunks per part × 6 = **480 chunks**, crossing `MAX_MAP_CHUNKS = ⌊(100 KB /
2) / 320 B⌋ = 156` (was 165) while each part's 80 chunks stays under `MAX_PART_CHUNKS` (156) and
every non-final part is exactly at the 5 MiB minimum. The added test derives its own ceiling from
`(100_000 / 2) / 302` = 165 and asserts `chunks <= 165`, so the implementation's *lower* 156 still
satisfies it — the test was not weakened to fit.

## Refutation — the three forced questions

**(a) Genuine red? Yes — eleven revert-and-rerun runs, each recorded. Two of them failed the
first time and are the reason two fixes changed shape:**

| Fix reverted | Test that went red | Failure |
|---|---|---|
| `drain_records`' `complete = false` | `a_records_obligation_past_the_derivation_cut_is_not_dropped_mid_way` | "the obligation MUST survive a truncated derivation — … (2001 of them)" |
| the per-fragment/precondition op cost | `a_part_whose_marks_exceed_one_batch_still_drains` | `batch of 2812 operations exceeds the 1001 the transaction envelope admits` |
| the tombstone admission release | `a_published_session_returns_its_admission_slot_and_keeps_its_tombstone` | `the slot comes back: left: 0, right: 1` |
| `stream_write_data`'s `max_chunks` guard | `a_stream_past_the_map_ceiling_is_refused_at_the_ceiling` | a 10-chunk plan was returned against a 4-chunk ceiling |
| `put_object_streaming`'s declared-length check | `a_declared_length_that_does_not_match_the_body_is_refused_before_commit` | the 16-byte body published under a 64-byte declaration |
| `declared_object_length`'s fail-closed arm | `a_malformed_declared_length_fails_closed_rather_than_reading_as_absent` | `left: Ok(None), right: Err("content-length")` |
| `find_chunk` → destination-first | DST `publication_handoff_never_drops_a_repair` (+ the seed sweep) | "a referenced chunk's repair obligation was dropped while the chunk was still damaged" |
| rebalance's destination pre-mark | `a_racing_writer_loses_the_version_conditional_commit_and_leaves_only_garbage` | the lost CAS left the copy unevidenced |
| `list_owned_staging`'s fail-closed arm | `an_owned_staging_entry_without_a_placement_fails_the_read_closed` | the malformed entry was skipped |
| `unhex` → `to_digit(16)` | `multipart_etag_is_order_sensitive_and_pure` | an uppercase digest composed an ETag |
| `reserve_slot`'s unknown-result re-read | `an_indeterminate_slot_reservation_is_adopted_rather_than_leaked` | the landed reservation was leaked |

Two **negative results**, recorded honestly because each one changed the fix:
1. the drain-flag revert first stayed **green** — the truncation branch was unreachable behind the
   batch budget. The cut moved below one pass's capacity so the branch is live (see §1).
2. the X67 property first stayed **green** with the broken scan order — the store's interposition
   fired too early. The injection point moved to the staged-class read.
If I had stopped at the first attempt in either case I would have reported a false "genuine red".

Plus the whole-file red leg: 19/19 on the base.

**(b) Production path? Yes.** The 19 wire tests drive the real gateway over HTTP (`aws-sdk-s3`
against an in-process `S3Gateway` on a real TCP listener) and the real custodian loops through the
real `reconcile_step` / `reconcile_after_restore` control point. The core unit tests drive
`wyrd_core::multipart::{drain, drain_step, reserve_slot, list_owned_staging, terminal_delete_batch,
release_admission_batch}` over a real `RedbMetadataStore::in_memory()` — the production redb
backend, including its native `scan_page`. `a_stream_past_the_map_ceiling_is_refused_at_the_ceiling`
(`crates/core/tests/stream_lease_lapse.rs:429`) drives the production
`write::stream_write_data` over redb + a real `FsChunkStore`. The declared-length test drives the
production `Gateway::put_object_streaming`. The two new DST properties drive the real batch
builders and the real reconstruction pass; `HandoffMeta`/`EnvelopeStore`/`UnknownResultStore` are
*store* doubles that add a fault or an interleaving to a real backend or a real map — none
re-implements a transition or the code under test.

**(c) Fixture includes the fault? Yes, and in three cases it is what makes the test
discriminating:**
- the derivation-cut test seeds **4,001 parts** — one past the boundary — and asserts on the state
  *after one step*, which is the only instant at which the leak is visible;
- `a_part_whose_marks_exceed_one_batch_still_drains` runs against a store that **refuses** an
  over-budget batch, because redb accepts one and would hide the defect (it did hide it: the
  finding was invisible until the envelope store existed);
- the X67 property's store **performs the publication mid-pass**, so the chunk genuinely passes
  between the two classes rather than being described as doing so;
- the X59 property lets the **seed** choose which racer commits first, so both interleavings are
  covered across runs rather than one hand-picked order;
- the rebalance test keeps its racing writer (the CAS really is lost) and now asserts the
  destination pre-mark stands *and* the source is untouched — two positions, not one aggregate.

## Gates run in this worktree

- `./engine/xtask.sh ci` (fmt + clippy + build + whole-workspace test + deny + conformance +
  statics + typos + docs lint + madsim clippy/test): **all checks passed**.
- `./engine/xtask.sh dst`: **green**, 15 campaign properties (13 + the two new ones) plus the
  regression-seed sweep.
- `cargo fmt --all` over every touched file, and the `typos` gate caught one misspelling in a new
  test fixture (fixed) — so the target's commit hooks have nothing left to reject.

## Existing behaviour deliberately changed (for the human's eye)

1. **`MAX_MAP_CHUNKS` 165 → 156** (a sound ceiling; a flat single PUT at the 1 MiB default now
   caps at 156 MiB rather than 165 MiB before the chunk size grows).
2. **`stream_write_data` gained a `max_chunks` parameter** — 9 call sites in two existing core
   test files pass `usize::MAX` (they are about lease semantics, not ceilings).
3. **The rebalance evacuation pre-marks its destinations**, which changes one existing test's
   assertion from "no orphan record exists" to "the destination's pre-mark stands and the source's
   does not" (`crates/custodian/tests/rebalance.rs:1342`). The old assertion encoded the leak.
4. **A declared body length is now enforced.** For `Content-Length` this is a no-op (HTTP framing
   already enforces it); for `x-amz-decoded-content-length` it is new and strict.

## What I did NOT do, and why

- **No ADR, no proposal edit, no spec, no change to 0016.** Two proposal corrections are owed and
  both are NEEDS-HUMAN below.
- **No split.** Open question 1 says Do reports rather than splits. The patch is ~12.5 K added
  lines across 44 files. The two seams that lift out cleanly are unchanged from last iteration:
  (i) the `scan_page` seam + four backends + conformance (~330 lines, zero coupling to the rest),
  and (iii) the custodian-side reference-set / scrub / reconstruction / rebalance changes (~900
  lines now, coupled to the record model but not to the wire surface).
- **No opt-in switch** (open question 4 settled as (a)).
- **No `W_tombstone` implementation.** The tombstone's terminal delete stays #625's; this slice
  only stops charging a tombstone for memory it does not hold.
- **The 8 GB `aws s3 cp` leg** stays the pre-declared off-Check item for SUMMARY §9.

## NEEDS-HUMAN at sign-off

1. **The 8+ GB `aws s3 cp` round-trip** (pre-declared in the brief): needs a deployed stack, the
   `aws` CLI and ≥8 GB free disk. Everything it exercises is built and driven at Check with
   smaller bodies.
2. **Two 0016 editorial corrections** — architecture-board authority, not a model's:
   (a) decision 7 (`:2314-2320`) and the implementation summary (`:2678`) still name the *upload
   id* as the segment-group nonce, which §1 (`:499-526`) supersedes and the code implements;
   (b) the admission record (`:348`, `:673`, `:964-968`) needs the two-count split — a tombstone
   charged against the `W_ref` staged-reference budget is a permanent refusal in any wave that
   does not also ship `W_tombstone`.
3. **fdb/tikv `scan_page` runtime conformance** — compile-verified only; the behavioural proof is
   `cargo xtask fdb-conformance` / `tikv-conformance`, which need a container runtime and a live
   cluster. The conformance clauses are already wired into both jobs.
4. **`with_durability` is fallible** (a public API change on the composition root, from iteration
   5). Two in-tree call sites updated; an out-of-tree caller needs the same `?`/`expect`.
5. **The reaper-less ceiling is now 65,536 uploads, not unlimited.** If the maintainer wants the
   startup signal upgraded from a warning to a hard refusal (open question 3), this is the
   iteration to say so — it is still a one-line posture change.
