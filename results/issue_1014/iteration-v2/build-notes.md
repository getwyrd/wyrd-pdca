# Build notes — #1014 (#659 child 3), iteration 2

Target: `getwyrd/wyrd` @ main, on the stacked base the harness gave this cycle,
`pdca-integration/main-r9dd123c9ed70` = `3c600b6` (carries #1061, #1062 and #1013). Every
`path:line` below is on the **patched** tree unless it says "base", which means `3c600b6`.

## Where this iteration starts

The sign-off said: rebuild inside this slice, keep the routing (reviewer-confirmed red→green),
fix the four reproduced safety defects plus three extras, each with a regression test that fails
today. So I applied iteration 1's patch (`iteration-v1/patch.diff`, which applies cleanly on
`3c600b6`) and changed what the carry-forward names. Unchanged from iteration 1: the routing in
`crates/core/src/metadata.rs` (`unlink` :2167, `fits_one_batch` :2214, `segmented_gate` :2238,
`with_retirement` :2261, `retiring` :2322, the superseding commits :2417 / :2605), the marker
record and gate in `crates/core/src/orphan_identity.rs` (key :50, `pinned` :321, `read_gate`
:355), the GC closed-gate arm, the dispatch beside the drain
(`crates/custodian/src/reconciliation.rs:179`, right after the drain at base `:168`), the
fixture changes to existing tests, and the DST seeding. See `iteration-v1/build-notes.md` for
the reasoning behind those; it still holds.

## What changed, item by item

### 1. P1 — a preserved `reclaiming` mark could authorize a reclaim with no grace

Cause: the cleanup leaves every `reclaiming` mark alone (the brief's F4), the marker certified
anyway, and after a routed `unlink` GC's reclaiming arm (`crates/custodian/src/gc.rs:722`, base
`:677`) finishes it at once. Nothing can give that fragment grace later: no writer may replace a
`reclaiming` mark (base `crates/core/src/metadata.rs:154-168`), and the drain's arm (0) waits for
GC on exactly that mark (base `crates/custodian/src/retirement/marks.rs:34-43`). The inline
`unlink` on the base blind-puts over it, which is why the base never showed this.

Fix (`crates/custodian/src/orphan_identity.rs:1016`): a `reclaiming` mark is still left
byte-identical (F4 holds), but when **any** record names its fragment the verdict is `Block`,
naming the mark's own key. A blocked lap writes no marker, so the routing never opens over such a
mark. This mirrors the drain's own rule for the same state, `Hold::ReclaimingRetained`
(base `crates/custodian/src/retirement.rs:353-357`, "needs a human"). A `reclaiming` mark that
nothing names is left as before; that is GC's ordinary work.

Why block and not something else:
- Dropping or rewriting the mark breaks F4 and the no-writer rule.
- Teaching GC to cancel a `reclaiming` mark on a referenced fragment would remove the state at
  its source, but it changes GC's reclaim protocol outside this slice's scope (the brief's scope
  is "GC's fallback to every mark protects"). Rough size: a new arm in `judge` before the
  reference-set skip (~25 lines) plus an exact-value delete and its unknown-outcome handling
  (another ~40, copying `commit_sweep`). I would rather surface it as a follow-up than ship it
  here.
- Liveness: with the gate closed GC writes no `reclaiming` marks at all (the gate arm comes
  first, `gc.rs:717`), so the only such marks a lap can meet are old ones: a restored image, or
  pre-upgrade state. On a healthy store a `reclaiming` mark sits on an unnamed position (GC only
  swaps unnamed ones), so this blocks nothing there.

Residual I am not fixing: after the marker, a repair move that re-places onto a position GC is
mid-way through reclaiming (between its swap and its key delete) could put a `reclaiming` mark
under a reference again. That is the #663 adoption-precondition gap (base `metadata.rs:165-168`
already names it). The routing **does** make that case worse once the marker exists: the base's
inline delete would blind-put a fresh mark over it, while a routed delete leaves it for GC to
finish with no grace. The window is narrow (GC deletes the fragment right after its swap, so the
re-placement must land inside one GC pass), and #663 closes it at the source. Worth saying at
sign-off.

Test: `f4_a_reclaiming_mark_on_a_live_fragment_never_opens_the_routing` (the restore → cleanup →
delete → GC-before-drain sequence the sign-off asked for, plus an operator-repair leg that then
routes and drains). The old F4 test now plants its `reclaiming` mark on an unnamed fragment.

### 2. P1 — an undecodable session was treated as live, and its marks dropped

Cause: iteration 1 mapped an undecodable `mpu:` value to `Phase::Unknown` and judged its records
as staged (drop), and its drop-time check treated an undecodable session as live.

Fix: a new namer class `Unknown { session }` (`orphan_identity.rs:410`, ranked after Committed and
Obligation). A mark whose strongest namer is an undecodable session's record is left and **blocks**
the lap, naming the `mpu:` key (`orphan_identity.rs:1041`). Dropping might erase a published
part's trap mark (a permanent leak); leaving might keep a stale mark on an unpublished part that
a later publication and routed delete would be judged by. Neither is safe, so a human repairs the
record. The drop-time check now requires a session record that **decodes** as live
(`orphan_identity.rs:1144`); one that stopped decoding since the reading makes the judgement stale.

The block is lazy: only a mark on such a session's records blocks. GC itself never decodes the
session value, and iteration 1 found that blocking on every undecodable session broke fixtures GC
considers healthy. A committed map or a pending obligation naming the same position still wins
(dropping is safe there whatever the session says).

Test: `f3_a_damaged_session_blocks_the_marker_and_keeps_the_trap` (F3's trap with the
`Completed` session's value damaged).

### 3. P2 — the read budget could underflow

Cause: `READS_PER_MARK` was 7 but two lost drops can read 8 (token lookup, two namer reads,
one mark re-read, twice), and two of the decrements were unchecked.

Fix: `READS_PER_ATTEMPT = 4`, `READS_PER_MARK = ATTEMPTS_PER_DROP * READS_PER_ATTEMPT = 8`
(`orphan_identity.rs:160-164`), and every judgement read now goes through one helper that
saturates (`orphan_identity.rs:1172`); the drop counter saturates too. I kept the token lookup
inside the retry loop rather than hoisting it: after a lost precondition the mark's fresh value
may name a different event, whose obligation has to be looked up.

Test: `f_two_lost_drops_at_the_read_budget_never_run_it_out`, two legs: 4,089 token-bearing
marks ahead (7 reads left: the window stops before the mark) and 4,088 (exactly 8 left: spent to
zero).

### 4. Liveness — a delete between the cleanup's reads discarded the whole lap

Cause: iteration 1 read who-names-what first and the ledger page after it, so the fresh marks of
any inline delete landing in between were judged against a stale reading ("namer moved"), the lap
was marked unsettled, and the next pass began again from the head. With steady deletes the marker
never came.

Fix, two parts:
- **Window first, then the reading** (`orphan_identity.rs:265-266`). A delete that lands before
  the reading is seen by it; its fresh marks are then judged correctly.
- **A stale judgement stops the window, it does not spoil the lap**
  (`orphan_identity.rs:956`). When a drop's records no longer name the position, or both
  attempts lose their precondition, the walk stops just **before** that mark and the lap record
  keeps its place. The next pass judges that mark on a fresh reading. The `unsettled` field is
  gone from `CleanupWalk` (`crates/core/src/orphan_identity.rs:139`) and from `CleanupStatus`.
  I chose "stop" over "settle as moot": a moot check is right for an inline delete (it rewrote
  the mark) but wrong for a Complete racing the drop of a staged part's mark (the mark is
  unchanged and now sits on a published object, so it must be judged again, not skipped).

Tests: `f_steady_deletes_at_the_ledger_head_do_not_stall_the_marker` (the adversary's
interleaving: one inline delete before every ledger-head read; marker within 3 passes with deletes
still landing), `f_a_stale_judgement_stops_the_window_and_keeps_the_lap` (a delete lands after the
reading on every pass; marker within 4 passes, one delete per pass), and
`f5_a_drop_racing_a_complete_is_judged_again_never_skipped` (tells "stop" apart from "skip").

### 5. The pending-lease bypass

Cause: GC's expired-lease arm (`gc.rs:758`, base `:713`) reclaims an unmarked, unreferenced
fragment whose chunk holds an expired `pending:` entry. Its soundness note relies on every
unreferencing commit writing the mark in the same commit. A routed delete does not, and neither
does a multipart Complete's superseded generation (already live on the base). A writer that
crashed between commit and `release` (base `crates/core/src/write.rs:347`) leaves exactly that
`pending:` entry.

Fix (`gc.rs:494-499`, `retirement_pending` at `gc.rs:1947`): when the pass has expired-lease input
at all, it first reads one page of at most one `retire:bytes:` entry; while any obligation is
pending, the input is deferred for the pass (audited, `gc.rs:2308`) and the pass answers `Partial`
(`gc.rs:567`). The arm's own soundness note now says so (`gc.rs:775`), as does the module doc
(`gc.rs:43-46`). It is read before the window, so an obligation found absent was deleted after its
drain wrote every mark (mark-then-delete), and those marks are in the window. Deployed passes use
`ExpiredPendingPolicy::Defer` (base `gc.rs:348-375`), so this changes nothing in production
today; it closes the hole for `Reclaim` callers.

Rejected: deleting the prior map's `pending:` entries in the routed batch. That adds one delete
per chunk back into the very batch the routing exists to keep small (100 more operations for the
100-chunk case; unbounded for a segmented prior, whose chunks the batch never lists).

Test: `a2_a_routed_delete_of_unreleased_leases_waits_for_the_drain` (release omitted,
`ExpiredPendingPolicy::Reclaim`).

### 6. Memory of the reference reading

Cause: iteration 1 expanded every pending obligation's positions (a segmented generation up to
~1.78 M) into one in-memory map, the expansion 0016 rules out (`0016:1236-1242`).

Fix: the window's marks are read first and the reading keeps **only** namers of positions (and
chunks) that the window marks (`Index::place`, `orphan_identity.rs:540`). Obligations are still
read whole, but page by page (`OBLIGATION_PAGE = 64`, `read_obligations` at :848) and each one's
`seg:` records a page at a time, keeping at most one namer and one entry per mark of the window.
The same filter applies to committed and staged records. The window is at most 16,384 marks
(≤ 6.6 MB), a quarter of GC's own window.

Rejected: a hard cap on expanded positions with "no marker this lap" past it. It bounds memory
but adds a liveness cliff that a single large segmented obligation trips.

Test: unit test `the_reading_keeps_only_the_windows_positions` (`orphan_identity.rs:1306`).

### 7. Stale marks on committed segmented objects (and the C5 survivors)

Test `f1_a_stale_mark_on_a_segmented_object_its_parts_also_name_is_dropped`: right after a
production Complete of a segmented object, with its records drain held, the inode (via `seg:`)
and the published parts both name the chunks. The stale mark is dropped (committed outranks
published) pinned on the inode and `seg:` record, then the routed delete drains it. This covers
the `read_segments`, segmented-verification and class-ranking survivors.
`f7_a_restore_mid_lap_discards_the_lap` (16,484 marks, so a lap spans two passes; a restore lands
between them) covers the resume guard and `CleanupWalk::is_current` survivors.

### 8. Docs wording

`06-runtime-view.md` no longer says the closed gate "only delays reclamation". It now says GC
reclaims no marked fragment while the gate is closed, that nothing bounds how long that lasts (a
blocked lap needs an operator, and every restore starts it again), and it describes the new
blockers, the stop-the-window rule, the bounded reading, and the expired-lease deferral.
`05-building-block-view.md` updates the marker's writer condition and lists the walk record's
`ended` field. The same wording fix is in the GC doc comment (`gc.rs:803`).

### 9. "Rebase onto current main"

The patch is built and tested on `3c600b6`, the stack base the harness gave this cycle, which
already carries #1013's drain dispatch. It cannot apply to bare `origin/main` (`57aefe0` today)
because its dependencies #1061, #1062 and #1013 are not merged there yet. That is why C4-diff-cov
reported "does not apply on origin/main" last time. Nothing in the patch can fix that; it resolves
when the stack merges or when C4-diff-cov uses the stack base.

## The test file — `crates/custodian/tests/retirement_routing.rs` (new)

24 tests, all through `reconcile_step` with the deployed contexts, over
`RedbMetadataStore::in_memory()` behind a thin wrapper and a placement-aware D-server double;
deletes, overwrites, Complete and restore are the production functions. Iteration 1's 15 are kept
(F4 adjusted as above); 9 are new. Plus one unit test in `crates/custodian/src/orphan_identity.rs`.

Run: `cargo test -p wyrd-custodian --test retirement_routing` → 24 passed;
`cargo test -p wyrd-custodian --lib orphan_identity` → 1 passed;
`cargo test -p wyrd-core -p wyrd-custodian` → all 68 result groups ok.

### Against iteration 1's code (the carry-forward's "fails today")

New test file, iteration-1 production files put back
(`crates/custodian/src/orphan_identity.rs`, `crates/custodian/src/gc.rs`,
`crates/core/src/orphan_identity.rs`), one line of the test adapted to the old `Walking`
shape:

| Test | Result on iteration 1 |
|---|---|
| `f4_a_reclaiming_mark_on_a_live_fragment_never_opens_the_routing` | FAIL: pass answers `Changed` (marker written) where `Blocked` is required; with the assertions reordered the doomed fragment is reclaimed before any drain (shown by R9 below) |
| `f3_a_damaged_session_blocks_the_marker_and_keeps_the_trap` | FAIL: status `Complete`, expected `Blocked { "mpu:…21" }` |
| `f_two_lost_drops_at_the_read_budget_never_run_it_out` | FAIL: panic `attempt to subtract with overflow` at old `orphan_identity.rs:920` |
| `f_steady_deletes_at_the_ledger_head_do_not_stall_the_marker` | FAIL: `Walking { unsettled: true }` after 3 passes |
| `a2_a_routed_delete_of_unreleased_leases_waits_for_the_drain` | FAIL: fragments reclaimed before the drain marks |
| `f_a_stale_judgement_stops_the_window_and_keeps_the_lap` | FAIL, but only incidentally: iteration 1 reads its index before the ledger, so the hook never lands inside its reading. Not counted as a red. |

### Demonstrated reds on the final code (each a one-spot mutation, run, then reverted)

| # | Mutation | Test | Failing assertion |
|---|---|---|---|
| R1 | drain dispatch removed from `reconcile_step` (routing left on) | `a2_a_delete_past_the_batch_budget_retires_through_one_obligation` | `drained within 6` |
| R2 | `fits_one_batch` counts puts + deletes only | `a2_the_boundary_counts_preconditions` | 83-chunk delete wrote inline marks (`Some("10000000")` vs `None`) |
| R3 | trap mark dropped (published → drop, nothing pinned) | `f3_the_trap_mark_on_a_published_part_is_kept_byte_identical` | trap mark gone (`None` vs `Some("10000000")`) |
| R4 | stale mark on a staged part kept | `f2_a_stale_mark_on_a_staged_fragment_is_dropped_and_never_judged` | stale mark still there (`Some("1000")` vs `None`) |
| R5 | drop committed without the judged records' bytes | `f5_a_drop_racing_unlink_or_overwrite_fails_and_the_writers_mark_survives` | the unlink's own mark deleted |
| R6 | marker written at the end whatever the lap met | `f6_an_unreadable_record_blocks_the_marker_until_it_is_repaired` | pass answers `Changed`, not `Blocked` |
| R7 | marker read without its generation | `f7_a_restore_turns_the_routing_off_until_the_cleanup_walks_again` | status `Complete` right after the restore pass |
| R8 | routing opens without a marker | `f_before_the_marker_the_paths_behave_as_on_the_base_and_gc_holds_every_mark` | 100-chunk delete wrote no inline marks |
| R9 | `reclaiming` mark on a named fragment left, not blocking | `f4_a_reclaiming_mark_on_a_live_fragment_never_opens_the_routing` | a fragment reclaimed before any drain (`assert_kept`) |
| R10 | undecodable session's mark left, not blocking | `f3_a_damaged_session_blocks_the_marker_and_keeps_the_trap` | status `Complete`, not `Blocked` |
| R11b | reservation 7 and unchecked decrement (iteration 1's shape) | `f_two_lost_drops_at_the_read_budget_never_run_it_out` | `attempt to subtract with overflow` |
| R12 | stale judgement skipped past instead of stopping | `f5_a_drop_racing_a_complete_is_judged_again_never_skipped` | stale mark survives on the published object |
| R13 | lease input not deferred while a retirement drains | `a2_a_routed_delete_of_unreleased_leases_waits_for_the_drain` | fragments reclaimed before the drain |
| R14 | reading not filtered by the window | `the_reading_keeps_only_the_windows_positions` | 3,000 positions held, not 2 |
| R15 | segmented arm of the drop verification removed | `f1_a_stale_mark_on_a_segmented_object_its_parts_also_name_is_dropped` | marker never written (`within 4`) |
| R16 | class ranking flipped | same | stale mark kept |
| R17 | lap never resumed | `f7_a_restore_mid_lap_discards_the_lap` | marker never written (`within 4`) |
| R18 | lap resumed across a restore | same | the restored stale mark behind the lap survives |

Not red, said plainly: **R11a**, the reservation cut back to 7 with the saturating read kept,
stays green. Saturation alone makes underflow impossible; the 8-read reservation is what keeps the
budget an honest upper bound (no read past it). No test counts reads per pass, so that half is
argued from the code (`orphan_identity.rs:160-164`, one read helper at :1172), not tested.

Six of the first mutation runs failed to **compile** (the repo denies warnings, and a naive
mutation leaves dead code). I did not count those; I rewrote each mutation to compile cleanly
(`if false { … }` around the removed path) and re-ran it. All six then failed as shown.

### C4-verify

The test file needs this slice's new types (`wyrd_core::orphan_identity`,
`wyrd_custodian::orphan_identity::status`), so on the unpatched base it does not compile and
C4-verify will report **UNVERIFIABLE (exit 77)**, as the brief expects. The real reds are the ones
above, plus iteration 1's reviewer-recorded base red for A2's "one obligation, no inline mark"
(`iteration-v1/check-review.md`, C2).

### Refuting my own test

- **(a) Genuine red?** Yes. 18 mutations of the final code and 5 runs against iteration 1's code
  fail on the named test, each for the stated reason (tables above). Each mutation was reverted
  with `git checkout` and the suite re-run green.
- **(b) Production path?** Yes. Passes go through `reconcile_step` with the contexts the deployed
  loop builds; deletes, overwrites, Complete and the post-restore pass are the production
  functions; the store is the real redb backend. The wrapper only forces a race, freezes or
  refuses a commit, fails one page, or runs a production `unlink` at a chosen read; it never
  answers a read itself.
- **(c) Fixture includes the fault?** Yes. The `reclaiming` mark sits on a live object's fragment
  after the production post-restore pass; the session value is actually damaged in the store; the
  two refused drops land at the budget's exact edge; the steady deletes are real production
  unlinks landing between the cleanup's reads on every pass; the unreleased leases come from the
  production write path with `release` skipped; GC runs with `ExpiredPendingPolicy::Reclaim`.

## Gates

`./engine/xtask.sh ci` (→ `cargo xtask ci` in `$PDCA_WORKTREE`), run twice: once before and
once after the last two comment edits in `gc.rs`. Both: **exit 0, "xtask ci: all checks
passed"**. After the second run one sentence was added to `05-building-block-view.md` (the walk
record's `ended` field, for docs currency); for that docs-only change I re-ran the gate's own docs
steps — `typos`, `lint_docs.py` (OK), `render_site.py --check` (101 pages, link audit OK) — not the
whole gate. Steps: `typos`, docs lint, docs
render with link audit (101 pages), gitlink/unsafe/blackbox guards, `cargo fmt --check`, clippy
`-D warnings` (workspace, and `wyrd-dst` under `--cfg madsim`), workspace build and tests,
cargo-machete, `cargo fetch --locked`, cargo-deny (both configs), statics, deploy-guard, and the
madsim DST suite.

- `typos` and the docs renderer (the brief's external dependencies) are installed and ran inside
  that gate. No NEEDS-HUMAN external dependency.
- The target repo configures no git commit hooks (no `core.hooksPath`, no hook files); its
  format, lint, typos and docs checks are the ones `cargo xtask ci` runs above.
- `patch.diff` = `git diff 3c600b6 <final tree>`: 28 files, applies cleanly to `3c600b6`
  (`git apply --cached --check` against a `3c600b6` index) and reverse-applies on the final tree.
- Scratch logs (gate runs, mutation runs, the mutation script) are under
  `$PDCA_SCRATCH/pdca-builder-1014-logs`. Nothing was written outside the worktree, the bundle and
  that directory. The worktree carries local WIP commits on its detached HEAD (no remote, nothing
  pushed).

## Out of scope / deferred

- Seeded DST for the walk racing `unlink`/Complete: `deferred: #660` in the module doc (the brief
  puts seeded DST out of scope).
- GC cancelling a `reclaiming` mark on a referenced fragment (item 1): a GC protocol change; worth
  an issue if the human wants the store to heal without an operator.
- Bulk `DeleteObjects` batching (#509), gateway verbs, proposal/ADR edits, `Cargo.lock`,
  `deny*.toml`: untouched.

## Existing tests touched

Same as iteration 1 (fixtures only, no assertion changed): `crates/custodian/tests/` `gc.rs`,
`gc_delete_backstop.rs`, `gc_ledger_walk.rs`, `gc_mark_sweep.rs`, `gc_reclaim_intent.rs`,
`gc_telemetry.rs`, `reconstruction.rs`, `segmented_map_reconstruction.rs`,
`segmented_map_repoint.rs`, `staged_repair.rs`, `multipart_retirement_drain.rs`,
`multipart_retirement_marks.rs`, `multipart_session_terminal.rs`, `restore_reconcile.rs`,
`staged_protection.rs`; `crates/dst/tests/custodian.rs` and `support/mod.rs`. This iteration
touched none of them further.
