# Build notes — #1009 (636.5c): segmented publication and the segment-charged budget

Base: the cycle worktree at `1f6d6a4` (`pdca-integration/main-r9dd123c9ed70`, which folds #1008
and #1028 onto `main`). All `path:line` below are on that tree **with the patch applied**,
unless marked "base". Planning artifact read: proposal 0016's *Segment write*, *Root flip* and
*Fence release* rows (`0016:662-664`), decision 7(b)–(d) (`0016:2342-2401`), X58 (`0016:2577`)
and the `MAX_COMPLETE_ATTEMPTS` knob row (`0016:1479`).

## What changed, and why

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
    (`multipart.rs:2598`), taken once when the fence is minted (`complete.rs:393`, `Placed::mint`).
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

### 2. The release once a segment landed: the obligation and the charging rule

- `Placed` (`complete.rs:375`) mints, before the fence is sent, the fence record, the attempt's
  group, and **both** releases: `restored` (attempts as read before the fence) and `charged`
  (attempts as the fence left them). This keeps the #1007 property "no fence is placed that could
  not be released".
- `OwnFence` (`complete.rs:1026`) now carries the segment cursor it last read. `release_batch`
  (`complete.rs:1074`):
  - cursor 0: require the fence's bytes, put `restored`. This is the old release, byte for byte.
  - cursor > 0: require the session at that cursor, put `charged`, and install
    `retire:records:s:<id>:<E+1>` `{seg: (nonce, E+1)}` under `require_absent`, through the
    existing `RetireObligation::attempt_segments` (`multipart.rs:4233`, the same row
    `completing_teardown` installs, `multipart.rs:2662`). This is the whole *Fence release* row,
    `0016:664`.
- Why the cursor decides: a segment batch moves the cursor in the same batch that writes its
  records, so cursor > 0 proves segments exist, and cursor 0 proves none do. Every release
  requires the session's **exact bytes** at the cursor it assumes, so a cursor-0 release cannot
  land over a session at cursor k > 0. A wrong guess fails as `Conflict`; it can never land as a
  release without the obligation. That is what makes the invariant hold by construction rather
  than by a flag.
- How the cursor is learned: after a round that loses, the existing session re-read now goes
  through `OwnFence::follow` (`complete.rs:1057`, used at `:1397`). After a round the store
  refuses for good that had segment batches to send, one read is added before the release
  (`complete.rs:1378-1385`). If that read fails, `release` falls back: a `Conflict` whose re-read
  shows this same fence at a later cursor is resent from there without spending a round
  (`complete.rs:1171`). This happens at most once per landed batch, since the cursor only moves
  forward.

### 3. Unknown segment outcomes

- The committer returns the store's `CommitUnknownResult` without saying which batch it came
  from. A round with segment batches to send now reports `DoubtfulCommit::Publication { epoch }`
  (new variant, `complete.rs:501`); a round with only the flip still reports `Flip`
  (`complete.rs:1361-1368`). Nothing is sent after either.

### 4. Docs

- `docs/design/architecture/05-building-block-view.md` (the `complete_upload` paragraph): the
  "segmented publication is not supported yet" sentence is replaced with the segmented path, the
  cursor, the charging rule and the release's obligation (docs-currency rule: a persisted field,
  `segments_written`, gained a writer).
- Module and item docs in `complete.rs` / `multipart.rs` updated wherever they described the
  flat-only verb, "the fence's bytes" as the release's precondition, or #1009 as future work.

## Alternatives ruled out (with cost)

- **Tell segment-batch vs flip unknowns apart with a store wrapper inside core.** A private
  `MetadataStore` adapter that delegates `get` / `scan` / `scan_page` / `commit` and records
  whether the last batch put the inode key: about 40 lines plus an `async_trait` impl in core.
  Rejected. The distinction does not change what #1010's recovery does (re-read the session in
  either case), and it adds a store seam inside core. The new variant costs 12 lines and says
  exactly what is known.
- **Release-side fallback only, no read before releasing after a definite failure.** It works,
  but J-seg would then always send one release that is certain to conflict, then a read, then the
  real release: one wasted commit on every segmented definite failure. The read first costs one
  `get` (8 lines). I kept the fallback too. Without it, a failed read followed by a release
  `Conflict` would reach `moved()`, which answers `OperationAborted` while this Complete's own
  fence still stands. That breaks C-1 ("every answer finds the fence down").
- **Decide "charged" from a flag the committer returns.** The committer has no such output
  (`staged_publication.rs:445` returns `CommitOutcome` or the store error), and changing its API
  is outside this slice. The store-held cursor is the proof the protocol already defines
  (`0016:613-620`).
- **Keep `Admit` with only `Any`.** That is a one-variant enum guarding nothing. Deleting it
  removes 46 lines and the deferral markers.

## Test — `crates/core/tests/multipart_complete.rs` (appended; copy in this bundle)

New legs, all driving the production verb `complete_upload` over a real `RedbMetadataStore`,
with parts staged by the production `upload_part` and read back by the production `read_object`:

| Leg | Test fn (line) |
|---|---|
| B | `b_a_list_past_one_flat_map_publishes_a_segmented_root_that_reads_back` (`:4528`) |
| B, boundary (replaces K's flat half) | `b_one_flat_map_stays_flat_and_one_chunk_more_is_segmented` (`:4649`) |
| D3 | `d3_a_fence_that_wrote_segments_stays_charged_and_the_cap_stops_the_next_one` (`:4708`) |
| H2-seg | `h2_seg_a_bucket_removed_after_the_segments_land_releases_with_their_obligation` (`:4802`) |
| J-seg (+ release fallback) | `j_seg_a_flip_refused_for_good_after_the_segments_land_releases_with_their_obligation` (`:4851`) |
| G0-seg | `g0_seg_an_unknown_segment_batch_outcome_returns_the_in_doubt_error_and_nothing_more` (`:4913`) |

Leg K (three tests: `k_a_list_past_one_flat_map_stops_before_the_fence`,
`k_a_list_only_the_frozen_set_shows_past_one_flat_map_releases_the_fence`,
`k_the_fence_phase_on_its_own_holds_a_list_past_one_flat_map`) is removed; B replaces it.

The **wide-ref fixture** (`wide_rig`, `:4359`): 32 D servers with 20-digit ids, RS(16,16)
(one fragment per server), chunk ids starting at 10^18. Measured: 165 chunks, each ref 762
encoded bytes (vs `MAX_CHUNKREF_BYTES` 315), committer plan 3 segments vs
`⌈165 / 158⌉ = 2`. B asserts `planned > by_count`, so the fixture cannot silently go narrow.
The narrow boundary leg passes even under red 3 below, which shows why the wide fixture is
needed.

Wrapper addition: `Arm::FailGetOnce` (`:383`), a one-shot read fault, used only by J-seg's
second case to reach the release fallback. Helpers: `assert_every_segment_named` (`:3226`, C-1
for `seg:` records), `assert_every_fragment_named` now resolves segmented roots through
`metadata::resolve_chunk_map`.

Runner: the brief names `cargo test -p wyrd-core`; I ran
`timeout 1500 cargo test -p wyrd-core --test multipart_complete` (bounded), and the full gate
`cargo xtask ci` (see below). Result with the patch: **62 passed, 0 failed**.

### Demonstrated reds (each a mutation of the patched source, reverted from a byte-identical backup)

| Mutation | Legs red | Observed |
|---|---|---|
| attempts restored on every release, incl. after segments (`Placed::mint` charged := `opened.attempts()`) | D3, H2-seg, J-seg | `Open@E+2, the fence's attempt still spent` — release put `"attempts":0` |
| release's `retire:records:{seg}` omitted | D3, H2-seg, J-seg | release batch has 1 precondition / 1 put, no obligation |
| segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` | B (also D3, H2-seg, J-seg, G0-seg) | `Stopped { cause: Publication(FenceOffCursor { phase: Flip }) }` instead of `Published` |
| unknown segment outcome treated as `Conflict` | G0-seg | `Ok(Answered(Published(..)))` instead of the in-doubt error |
| **whole fix reverted** (base `multipart.rs` + `complete.rs`; test's two `Publication` references swapped to `Flip` so it compiles) | all six new legs | `SegmentedUnsupported { chunks: 165, limit: 158 }` (159 for the boundary leg); 56 older legs pass |

With the test file unmodified, the base fails to compile it (`no variant named Publication`).

### Refuting my own test (forced)

- **(a) Genuine red? Yes.** With the fix reverted (row 5), all six new legs fail on behavior.
  Each of the brief's four demonstrated reds fails the legs the brief names (rows 1-4).
- **(b) Production path? Yes.** Every leg calls `complete_upload` (`complete.rs:1274`) over
  `RedbMetadataStore::in_memory()`. Parts are staged by `commit::upload_part` onto the in-process
  fleet, and objects are read back by `read::read_object`. The wrapper `Logged` only scripts
  commit outcomes (pass, `Conflict` unapplied, definite error, unknown applied/unapplied, a write
  first). It never re-implements a decision of the verb.
- **(c) Fixture includes the fault? Yes.** The segment batch really lands in D3 / H2-seg /
  J-seg / G0-seg(applied): `segments_of(group) == planned` is asserted on the store. The flips
  really lose (`ConflictUnmoved`), the marker is really deleted from the inner store, and the
  unknown outcome is really applied or not. The wide fixture's early segment close is asserted
  (`planned > ⌈chunks / MAX_SEG_CHUNKS⌉`), not assumed.

## Gates / commit readiness

- `cargo fmt -p wyrd-core` applied. `cargo clippy -p wyrd-core --all-targets`: clean.
- **`cargo xtask ci` (the project's gate): `xtask ci: all checks passed`, exit 0.** It ran
  `typos`, `render_site.py --check` (link audit OK), `cargo fmt --all -- --check`, workspace
  clippy, the workspace tests (`multipart_complete`: 62 passed), conformance vectors, the
  statics / deploy / unsafe / blackbox guards, and the DST suite. One comment-only edit
  (`complete.rs:199`, "#1009–#1011" → "#1010 and #1011") landed while the DST phase was
  running; `typos`, `cargo fmt --check` and `cargo clippy -p wyrd-core --all-targets` were
  re-run clean after it.
- `RUSTDOCFLAGS=-D warnings cargo doc -p wyrd-core` is **not** part of the gate, and it fails on
  this base already (private/broken intra-doc links in `metadata.rs`, `erasure.rs`, `read.rs`,
  `write.rs`, `stage.rs`, and `multipart.rs:5396` `one_value_bytes`). None of those come from
  this patch. `complete.rs` produces no rustdoc warning.
- External dependencies named by the brief: `typos` (typos-cli 1.48.0) and `docs-renderer`
  (`python3 -c "import markdown_it, yaml"` succeeds) are both present on this host.
- `patch.diff` is `git diff HEAD` of the worktree and applies cleanly to `1f6d6a4`
  (`git apply --check --cached`). It touches 4 files: `crates/core/src/multipart.rs`,
  `crates/core/src/multipart/complete.rs`, `crates/core/tests/multipart_complete.rs`, and
  `docs/design/architecture/05-building-block-view.md`. Out-of-scope files (0016, ADRs,
  `Cargo.lock`, `deny*.toml`) are untouched.

## Known limits (not defects of this slice)

- No leg drives Complete across **two or more** segment batches. Complete passes no byte/ops
  budget to the committer (`byte_budget: None`), and exceeding `MAX_BATCH_BYTES` (5 MB) needs
  about 100 segments of ~50 KB, roughly 6,500 wide chunks per test. The committer's own tests
  cover batch splitting (`crates/core/tests/segmented_publication.rs`), and `SEGMENT_BATCHES = 1`
  is asserted by B, so a change there fails B first.
- `DoubtfulCommit::Publication` is a new public variant: #670 (the S3 verb) maps it, and #1010
  settles it by re-reading the session.
- A published segmented object cannot be deleted or overwritten until #659 (brief, *Production
  reach* (d)). No production caller exists yet (#670).
- Seeded DST coverage of the segment phase racing the reaper / `PutObject` stays #660's (the
  `// deferred: #660` marker in `complete.rs` is updated to name the segment batches).
