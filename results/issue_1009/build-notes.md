# Build notes — #1009 (636.5c): segmented publication and the segment-charged budget

Base: the cycle worktree at `1f6d6a4` (`pdca-integration/main-r9dd123c9ed70`, which folds #1008
and #1028 onto `main`), the same base as iteration 1. All `path:line` below are on that tree
**with the patch applied**, unless marked "base". Planning artifact read: proposal 0016's
*Segment write*, *Root flip* and *Fence release* rows (`0016:662-664`), decision 7(b)–(d)
(`0016:2342-2401`), X58 (`0016:2577`) and the `MAX_COMPLETE_ATTEMPTS` knob row (`0016:1479`).

## Iteration 2 — what changed, and the carry-forward answered

The sign-off kept the production logic ("no code defect was found") and asked for test gaps to
be closed. **No production line changed in this iteration.** `crates/core/src/multipart/complete.rs`
is byte-identical to iteration 1, and the only change to `crates/core/src/multipart.rs` is a new
`#[cfg(test)]` module. Everything else is in `crates/core/tests/multipart_complete.rs`. Patch
size is now 148 KB (was 124 KB); per the sign-off, it is not split.

| Carry-forward item | Done by | Proof |
|---|---|---|
| A leg that forces at least two segment batches, a later batch answers `Conflict` after an earlier one landed, the next round resumes at `0 < own.cursor < plan.segments` (`complete.rs:1350`) | leg **M**, `m_a_later_segment_batch_that_loses_resumes_from_the_cursor_the_earlier_one_left` (`multipart_complete.rs:5185`) on the new **deep fixture** (`planted_deep`, `:4445`) | asserts 2 segment batches, cursor `k` with `0 < k < planned`, and that round 2's first commit is the lost batch again byte for byte (`assert_same_batch`, `:4540`) |
| …must kill `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }` | leg M | mutant run: M fails with `expected Published, got Ok(Refused(OperationAborted))` |
| Cover `OwnFence::follow` (`complete.rs:1057`) following the cursor to a mid-plan position during a release | leg **M-release**, `m_a_later_segment_batch_refused_for_good_releases_from_the_cursor_the_earlier_one_left` (`:5290`), case `read_fails = true` | release from cursor 0 really conflicts on the store, `release` re-reads, `follow` moves to `k < planned`, and the re-sent release requires `at_cursor(fence, k)`. Mutants R1/R2 below make it fail |
| Unit test at `MAX_SEGMENT_INDEX + 1` and `+ 2` for the bound at `multipart.rs:2726` | `multipart::segment_cursor::the_cursor_moves_up_to_the_decode_bound_and_never_past_it` (`multipart.rs:6184`) | all four cargo-mutants survivors there, plus `>`→`<`, now fail it |
| Cover `writes_segments = plan.segments > 0` (`complete.rs:1340`) with an unknown outcome in a resumed flip-only round | `g0_seg_an_unknown_flip_names_the_flip_only_in_a_round_with_no_segment_batch` (`:5108`) | that mutant fails it: the error text says "a segment batch or the root flip…" where `DoubtfulCommit::Flip` was expected |
| Written reason for the equivalent mutant at `complete.rs:1062` | below, *The one surviving mutant* | — |
| Do not split | not split | — |

Failing gates carried forward:

- **C5 surviving mutants:** re-run with the gate's own command (`cargo mutants --in-diff patch.diff
  --no-shuffle --cap-lints=true`): **58 mutants tested in 4m: 1 missed, 45 caught, 12 unviable**
  (iteration 1: 5 missed, 41 caught). The one miss is the equivalent mutant explained below.
- **C4 diff coverage ("patch.diff does not apply on origin/main"):** not a stale bundle. This
  slice depends on #1008, which is **not on `main`** (`git merge-base --is-ancestor 10f03fd main`
  is false). The patch applies cleanly on its stack base `1f6d6a4`. The coverage row in
  `pdca.toml` has `verifies_base = false` (`pdca.toml:1540`), so it gets no `$PDCA_BASE`;
  `run-verify.sh --print-base` then answers `origin/main` (checked with
  `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh --print-base` → `origin/main`). No rebase in Do
  can fix that short of folding #1008 into this patch. Fixing it means giving the row the stack
  base (`verifies_base = true`, or a harness change to pass the stack base to bundle-scoped
  coverage rows), or re-running coverage once #1008 merges. That is a harness/config decision for
  the human, not part of this patch. I did not run `run-diff-cov.sh` by hand: it creates its own
  `../wyrd-cov` worktree outside the roots I'm allowed to write.

### The deep fixture, and why its parts are planted

Two segment batches need more rows than one batch's envelope. With `byte_budget: None` and
`ops_budget: None` (`complete.rs:1351-1352`) the envelope is `MAX_BATCH_BYTES` = 5,000,000 and
`MAX_BATCH_OPS` = 500 (`multipart.rs:5482`, `:5521`), and each row costs 2 ops
(`staged_publication.rs:621`) plus the contribution's 2. So one batch holds at most 249 rows. The
deep fixture is 250 parts of `MAX_PART_CHUNKS` = 158 narrow chunks (`EcScheme::None`, ~55 encoded
bytes each), so each segment closes by count at 158 refs and the plan is 250 segments in ~2.2 MB:
batch 0 is rows 0..249, batch 1 is row 249. The legs assert `DEEP_SEGMENT_BATCHES = 2` from the
commit log, so a change to the envelope fails them loudly.

I first built it by real staging (`upload_part`), as the other #1008/#1009 legs do. Measured on
this host: staging took **35.7 s** of a 38 s leg (about 100–185 ms per 158-chunk part, growing
with the part count). The Complete itself took 0.5 s. The release leg, which stages twice, took
74 s against the 120 s `LEG_TIMEOUT`. The cost is the chunk count, not the bytes: ~39,500 chunks
are needed on the ops route. The bytes route needs ~5 MB of refs, about 240,000 fragment
placements at 20-digit ids, which by the measured per-fragment cost (~38 µs) is still about 9–12 s
per session. It would also add that much to every mutant the C5 gate runs (58 of them).

So the deep fixture **plants** each part's `part:` and `psum:` records directly
(`plant_deep_part`, `:4481`), the way the #1007 legs plant theirs (`plant_part`, `:229`). Each
record is checked canonical by decode → encode, and the `psum:` summary is checked against its
decode. Planting takes under a second, and the whole file runs in 1.9 s. The Complete reads only
those two record kinds for a part (`complete.rs:1468-1494`), so the code under test is the same.
What it costs: no fragment exists, so M checks that the published root **resolves** to the
planted chunk list in order (`metadata::resolve_chunk_map`) and that `inode.size` is their sum,
instead of a byte read. Leg B still reads a staged segmented object back byte for byte.

### The one surviving mutant (equivalent): `complete.rs:1062`, `cursor >= self.cursor` → `true`

`OwnFence::follow` only reaches that guard after `Placed::cursor_of` found the session to be
**byte for byte** this fence's record at some cursor (`complete.rs:422-432`). So the guard only
decides between cursors of this same fence. A lower cursor than the one this Complete already
read would need some writer to move `segments_written` backwards, and none does:

- every segment batch requires the session at `first_index` and puts it at `written > first_index`
  (`complete.rs:1325-1331`; `SegmentBatchInfo`, `staged_publication.rs:134-140`), so it only
  moves the cursor forward;
- the only other writers of a `Completing` session (the release, the flip, `completing_teardown`,
  the reaper's rollback) move it out of `Completing` or to a new epoch, which `cursor_of` does
  not recognise as this fence at all (`None` → `follow` returns `false` before the guard);
- `get` returns the latest committed value, so a read never goes back in time.

With no reachable lower cursor, `>=` and `true` behave the same on every reachable state, and no
honest test can tell them apart. A test that planted a rewound session would be testing a state
the system cannot produce. I kept the guard: it states the invariant, and if a future writer ever
rewound the cursor it would stop a release from requiring a lower cursor's bytes (which, at 0,
would give the attempt back and omit the `{seg}` obligation).

## What the patch does (unchanged since iteration 1)

### 1. Complete now publishes past `MAX_MAP_CHUNKS` through the committer's segment phase

- `crates/core/src/multipart/complete.rs:1287` `publish_held` (was `publish_flat`) hands the
  committer (`crates/core/src/staged_publication.rs:445`) a real segment contribution instead
  of the old `no_segments` stub:
  - the hook at `complete.rs:1325` requires the session at `Completing@E+1` with
    `segments_written = first_index` and puts it at `segments_written = written`, the
    `0016:613-620` / `:662` cursor rule. The per-cursor record comes from the new
    `SessionRecord::with_segments_written` (`crates/core/src/multipart.rs:2717`), which changes
    only `segments_written` and refuses a cursor past the decode bound, so it never mints a record
    the decoder rejects.
  - `group` is the attempt's group from `SessionRecord::attempt_segment_group`
    (`multipart.rs:2598`; base `:2594`; the brief's `:2503` was at `4bac538`), taken once when the
    fence is minted (`complete.rs:393`, `Placed::mint`).
  - `resume_from` is the session's cursor as this Complete last read it (`own.cursor`,
    `complete.rs:1350`; committer field `staged_publication.rs:164`). A later round of the same
    attempt therefore pins every written segment instead of writing it again.
- The segment count the flip requires is `staged_publication::planned_segments`
  (`staged_publication.rs:417`), computed once in `FlipPlan::read` (`complete.rs:1494`). The flip
  contribution pins the session at that cursor (`FlipPlan.ready`, `complete.rs:1571`). It is never
  re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉`.
- Child-2's refusal is removed: `Stop::SegmentedUnsupported` and the `Admit` enum (base
  `complete.rs:467-475`, `:686-718`), both marked `// deferred: #1009 … deleted with it`. With
  `Admit` gone, the private `fence(…, admit)` helper had one caller left, so `fence_complete` now
  holds that body (`complete.rs:790`) and `complete_upload` calls `fence_complete`
  (`complete.rs:1274`).
- Constants cited by the brief, re-resolved: `MAX_MAP_CHUNKS` `multipart.rs:5372` (base `:5331`),
  `MAX_COMPLETE_ATTEMPTS` `multipart.rs:5629` (base `:5588`), `R_PUBLISH` `multipart.rs:5621`.

### 2. The release once a segment landed: the obligation and the charging rule

- `Placed` (`complete.rs:375`) mints, before the fence is sent, the fence record, the attempt's
  group, and **both** releases: `restored` (attempts as read before the fence) and `charged`
  (attempts as the fence left them). This keeps the #1007 property "no fence is placed that could
  not be released".
- `OwnFence` (`complete.rs:1026`) carries the segment cursor it last read. `release_batch`
  (`complete.rs:1074`):
  - cursor 0: require the fence's bytes, put `restored`. This is the old release, byte for byte.
  - cursor > 0: require the session at that cursor, put `charged`, and install
    `retire:records:s:<id>:<E+1>` `{seg: (nonce, E+1)}` under `require_absent`, through the
    existing `RetireObligation::attempt_segments` (the same row `completing_teardown` installs,
    `multipart.rs:2662-2673`). This is the whole *Fence release* row, `0016:664`.
- Why the cursor decides: a segment batch moves the cursor in the same batch that writes its
  records, so cursor > 0 proves segments exist, and cursor 0 proves none do. Every release
  requires the session's **exact bytes** at the cursor it assumes, so a cursor-0 release cannot
  land over a session at cursor k > 0. A wrong guess fails as `Conflict`; it can never land as a
  release without the obligation.
- How the cursor is learned: after a round that loses, the session re-read goes through
  `OwnFence::follow` (`complete.rs:1057`, used at `:1397`). After a round the store refuses for
  good that had segment batches to send, one read is added before the release
  (`complete.rs:1378-1385`). If that read fails, `release` falls back: a `Conflict` whose re-read
  shows this same fence at a later cursor is resent from there without spending a round
  (`complete.rs:1171`). Leg M-release now drives that fallback to a **mid-plan** cursor.

### 3. Unknown segment outcomes

- The committer returns the store's `CommitUnknownResult` without saying which batch it came
  from. A round with segment batches to send reports `DoubtfulCommit::Publication { epoch }`
  (`complete.rs:501`); a round with only the flip reports `Flip` (`complete.rs:1340`,
  `:1361-1368`). Nothing is sent after either. The new G0 leg pins both sides of that rule.

### 4. Docs

- `docs/design/architecture/05-building-block-view.md` (the `complete_upload` paragraph): the
  "segmented publication is not supported yet" sentence is replaced with the segmented path, the
  cursor, the charging rule and the release's obligation (docs-currency rule: a persisted field,
  `segments_written`, gained a writer). Iteration 2 adds no port, API, RPC, CLI flag or
  persisted field, so no further doc change.

## Alternatives ruled out (with cost)

- **Add a batch-budget knob to `PublishRequest` so a test can force two batches with few chunks.**
  It would make the two-batch leg cheap with staged parts, but it changes a public API (a new
  field, a docs-currency update, and a choice for #670 to make) purely for a test, against the
  sign-off's "keep the production logic". Rejected.
- **Simulate "an earlier batch landed" with a wrapper write of the session at a mid cursor.** One
  real batch plus a `write_first` that moves the cursor and writes the first `k` `seg:` records.
  Rejected: the sign-off asked for a leg that forces two real batches, and that fabricates the
  very state the committer's split should produce.
- **Real staging for the deep fixture.** Measured above: ~36 s per session on the ops route
  (74 s for the two-case release leg, against a 120 s leg timeout), and an estimated 9–12 s on the
  bytes route, multiplied by every C5 mutant. Planting costs under 1 s; what it gives up (a byte
  read-back) is covered by leg B.
- From iteration 1, still standing: a store wrapper inside core to tell segment-batch from flip
  unknowns (about 40 lines plus an `async_trait` impl; it does not change what #1010 does); a
  release-side fallback only, with no read first (one wasted commit on every segmented definite
  failure); a "charged" flag from the committer (no such output, and its API is out of scope);
  keeping `Admit` with one variant (46 lines guarding nothing).

## Test — `crates/core/tests/multipart_complete.rs` (appended; copy in this bundle) and one unit test

All integration legs drive the production verb `complete_upload` (`complete.rs:1274`) over a real
`RedbMetadataStore`. Legs on the wide and narrow fixtures stage parts with the production
`upload_part` and read objects back with `read_object`; the deep fixture plants its parts (above).

| Leg | Test fn (line) | New in |
|---|---|---|
| B | `b_a_list_past_one_flat_map_publishes_a_segmented_root_that_reads_back` (`:4658`) | it. 1 |
| B, boundary (replaces K's flat half) | `b_one_flat_map_stays_flat_and_one_chunk_more_is_segmented` (`:4779`) | it. 1 |
| D3 | `d3_a_fence_that_wrote_segments_stays_charged_and_the_cap_stops_the_next_one` (`:4838`) | it. 1 |
| H2-seg | `h2_seg_a_bucket_removed_after_the_segments_land_releases_with_their_obligation` (`:4932`) | it. 1 |
| J-seg (+ release fallback) | `j_seg_a_flip_refused_for_good_after_the_segments_land_releases_with_their_obligation` (`:4981`) | it. 1 |
| G0-seg | `g0_seg_an_unknown_segment_batch_outcome_returns_the_in_doubt_error_and_nothing_more` (`:5043`) | it. 1 |
| G0-seg, resumed flip-only round | `g0_seg_an_unknown_flip_names_the_flip_only_in_a_round_with_no_segment_batch` (`:5108`) | **it. 2** |
| M, mid-plan resume | `m_a_later_segment_batch_that_loses_resumes_from_the_cursor_the_earlier_one_left` (`:5185`) | **it. 2** |
| M-release, mid-plan release + `follow` in `release` | `m_a_later_segment_batch_refused_for_good_releases_from_the_cursor_the_earlier_one_left` (`:5290`) | **it. 2** |
| cursor bound (unit) | `multipart::segment_cursor::the_cursor_moves_up_to_the_decode_bound_and_never_past_it` (`crates/core/src/multipart.rs:6184`) | **it. 2** |

Leg K (three tests) stays removed; B replaces it. New helpers: `planted_deep` / `Deep` /
`plant_deep_part` (`:4445-4513`), `cursor_left` (`:4516`), `segments_put` (`:4529`),
`assert_same_batch` (`:4540`). The `SEGMENT_BATCHES` doc (`:4352-4356`) now says it is for staged
parts, and the module doc lists the new legs.

Runner: `timeout 1500 cargo test -p wyrd-core --test multipart_complete` (bounded) and
`timeout 1500 cargo test -p wyrd-core --lib`, plus the full gate `./engine/xtask.sh ci` (below).
With the patch: **65 passed, 0 failed** (file runs in 1.9 s), lib **78 passed**.

### Reds

All mutations below were applied to a copy and restored from byte-identical backups in the scratch
dir; after the last one, `git diff 1f6d6a4` was compared with `cmp` against `patch.diff`
(identical).

| Mutation | Legs red | Observed |
|---|---|---|
| **whole fix reverted** (base `multipart.rs` + `complete.rs`; the test's `DoubtfulCommit::Publication` match arm dropped and its 2 uses swapped to `Flip` so it compiles) | all 9 segmented legs: B ×2, D3, H2-seg, J-seg, G0-seg ×2, M, M-release; 56 older legs pass | `SegmentedUnsupported { chunks: 39500, limit: 158 }` for M, `{ chunks: 165, … }` for the wide legs |
| `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }` (`complete.rs:1350`) | M | `expected Published, got Ok(Refused(OperationAborted))` |
| `writes_segments = plan.segments > 0` (`complete.rs:1340`) | G0-seg resumed | got "a segment batch or the root flip of its publication…", expected "its root flip from Completing@1" |
| `release` never re-sends from a later cursor (`complete.rs:1171`, `&& false`) | J-seg, M-release | — |
| `follow` moves the cursor but keeps the stale bytes (`complete.rs:1064` deleted) | D3, H2-seg, J-seg, M-release | — |
| bound `>` → `>=`, `==`, `<`; `+ 1` → `- 1`, `* 1` (`multipart.rs:2726`) | the unit test, each | `expect("a cursor the decoder accepts")` panics at the bound |
| `cursor >= self.cursor` → `true` (`complete.rs:1062`) | none | equivalent, see above |

The brief's four demonstrated reds (iteration 1: attempts restored on every release; the
`{seg}` obligation omitted; the count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` on the wide
fixture; an unknown segment outcome treated as `Conflict`) hit production code that is unchanged
in this iteration, and the legs that caught them are unchanged, so they still hold. With the test
file unmodified, the base fails to compile it (`no variant named Publication`), which is why the
whole-fix row needs the swap.

### Refuting my own test (forced)

- **(a) Genuine red? Yes.** With the fix reverted, all 9 segmented legs fail on behaviour (row 1),
  including the three new ones. Each carry-forward mutant fails the leg written for it (rows 2–6).
  The only survivor is the equivalent one.
- **(b) Production path? Yes.** Every integration leg calls `complete_upload` over
  `RedbMetadataStore::in_memory()`, and the committer, the session codec and the store are the
  production ones. The wrapper `Logged` only scripts commit outcomes (pass, `Conflict` unapplied,
  definite error, unknown applied/unapplied, a one-shot read fault). The unit test calls the
  production `SessionRecord::with_segments_written` and `decode_session_record`. The deep
  fixture's planted records are inputs, checked canonical; they replace `UploadPart`, not any
  step of Complete.
- **(c) Fixture includes the fault? Yes.** In M, the first batch really lands
  (`segments_put(first) == 0..k` and later every segment is durable) and the second really loses;
  the mutant shows the next round would really conflict on the store. In M-release, the first
  batch's `k` records are durable (`segments_of(group) == k`), the second batch really fails, and
  in the `read_fails` case the stale release's `Conflict` comes from the real store, not the
  script (the script has three entries; the releases pass through). The two-batch split is
  asserted from the commit log, not assumed.

## Gates / commit readiness

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-core --all-targets`: clean (exit 0).
  `typos` on all four touched files: clean.
- **`cargo xtask ci` (`./engine/xtask.sh ci` on the worktree, the project's gate): `xtask ci: all
  checks passed`, exit 0**, run after the last source edit. It ran `typos`, `render_site`
  (link audit OK), the gitlink / unsafe / blackbox guards, `cargo fmt --check`, workspace
  clippy, the workspace tests (`multipart_complete`: 65 passed in 1.93 s; the new
  `multipart::segment_cursor` unit test ok), `cargo deny` (advisories, bans, licenses, sources
  ok), the conformance vectors, the statics and deploy guards, and the DST suite.
- External dependencies named by the brief: `typos` and `docs-renderer` are present on this host
  (`typos` ran above; the gate's `render_site.py --check` step uses the renderer).
- `patch.diff` is `git diff 1f6d6a4` of the worktree (4 files: `crates/core/src/multipart.rs`,
  `crates/core/src/multipart/complete.rs`, `crates/core/tests/multipart_complete.rs`,
  `docs/design/architecture/05-building-block-view.md`). Out-of-scope files (0016, ADRs,
  `Cargo.lock`, `deny*.toml`) are untouched.

## Known limits (not defects of this slice)

- The deep fixture does not read its object back (no fragments); leg B covers read-back.
- `DoubtfulCommit::Publication` is a new public variant: #670 (the S3 verb) maps it, and #1010
  settles it by re-reading the session.
- A published segmented object cannot be deleted or overwritten until #659 (brief, *Production
  reach* (d)). No production caller exists yet (#670).
- Seeded DST coverage of the segment phase racing the reaper / `PutObject` stays #660's (the
  `// deferred: #660` marker in `complete.rs`).
- Diff coverage is unmeasured until the coverage row gets the stack base or #1008 merges (above).
