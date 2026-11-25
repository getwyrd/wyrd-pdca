# Build notes — #1014 (#659 child 3), iteration 3

Target: `getwyrd/wyrd` @ main, on the stacked base the harness gave this cycle,
`pdca-integration/main-r9dd123c9ed70` = `3c600b6` (carries #1061, #1062 and #1013). Every
`path:line` below is on the **patched** tree unless it says "base" (`3c600b6`) or "v2" (iteration
2's patched tree, `iteration-v2/patch.diff`).

## Where this iteration starts

The sign-off said: rebuild inside this slice; round 2 closed round 1's list; fix the new P1s, each
with a regression test that fails today; add X3; record (do not resolve) the repair finding. So I
applied iteration 2's patch (it applies cleanly on `3c600b6`) and changed what the carry-forward
names. Unchanged from iteration 2: the routing in `crates/core/src/metadata.rs`, the marker and the
gate in `crates/core/src/orphan_identity.rs`, GC's closed-gate arm and expired-lease deferral, the
dispatch beside the drain (`crates/custodian/src/reconciliation.rs:179`), the fixture edits to
existing tests and the DST seeding. `iteration-v2/build-notes.md` and `iteration-v1/build-notes.md`
hold the reasoning for those, and it still holds.

## What changed, item by item

### 1. P1 — a damaged walk cursor certified the marker without judging a mark

Cause: `decode_cleanup_walk` accepted any lowercase hex as `after` (v2
`crates/core/src/orphan_identity.rs:235-239`). `{"after":"ff"}` sorts after every `orphan:` key, so
the window came back empty at the end and the pass wrote the marker over every stale mark.

Fix, as the sign-off asked (GC's own cursor check):
- `ORPHAN_KEY_MAX_LEN` moves from a private const in GC to `crates/core/src/metadata.rs:81`, beside
  `orphan_key`, so both walks share one bound; GC uses it at `crates/custodian/src/gc.rs:2120-2121`
  (no behaviour change there).
- `decode_cleanup_walk` now refuses an `after` that is not under `orphan:` or is longer than that
  bound (`crates/core/src/orphan_identity.rs:248`). A refused record already starts a fresh lap in
  `reconcile` (the `_ =>` arm of the lap match, `crates/custodian/src/orphan_identity.rs:282`),
  so the walk restarts at the head.
- `CleanupWalk::advanced` records a ledger key longer than the bound cut to it
  (`crates/core/src/orphan_identity.rs:196`). Without that, a window that happened to end on such a
  junk key would be refused on the next pass and the walk pinned at the head for good (the liveness
  trap GC's own cursor comment names). A prefix sorts before the key, so the next pass re-reads that
  key (a key the walk only leaves alone) and goes on.

Residual, said plainly: this is a structural bound, the same strength as GC's. A damaged value that
stays canonical JSON and names a key inside the ledger (`orphan:\xff`, or a high-but-valid key)
still skips the marks before it. Closing that needs integrity on the custodian's own record
(nothing in the store has it). The unit test documents that `orphan:\xff` is accepted.

Tests: `f6_a_damaged_walk_cursor_starts_a_fresh_lap` (`crates/custodian/tests/retirement_routing.rs:1888`;
the stale mark on a live 100-chunk object, then the routed delete and GC before the drain), and
unit `the_walk_cursor_must_lie_inside_the_ledger` (`crates/core/src/orphan_identity.rs:474`).

### 2. P1 — a mark naming an unrelated live obligation was left alone and certified

Cause: any present `retire:bytes:` for the mark's token gave `Leave` (v2 `:1025-1032`), whatever
named the fragment.

Fix (`crates/custodian/src/orphan_identity.rs:1199-1223`): the mark is still left byte-identical
(F4), but the lap may certify it only if nothing a later unreference could act on names the
position, other than the mark's own retirement. "Could act on" is the class set `LIVE`
(`:459`): a committed map, a live session's record, an undecodable session's record, or **another**
obligation. To tell the mark's own obligation from another, the window now keeps, per mark, the
`retire:bytes:` key its event names (`Window.tokens`, `token_key` at `:420`), and the reading notes
every position an obligation names that is not that one (`Index.foreign`, set at `:697`). If any
live record names it, the verdict is `Block` naming the mark, with its own audit reason
(`emit_draining_named`, `:1441`). A lap after that retirement drains judges the mark like any other
(token absent → normal judgement → a committed namer drops it), so this block clears itself.

Why not "leave and certify unless the obligation names it" (the sign-off's words, taken literally):
a segmented generation's drain and a `{parts}` drain delete each `seg:` / `part:` record after
marking it, so mid-drain the reading no longer shows the obligation naming those positions even
though the marks are its own. Blocking on "the obligation does not name it" alone would block the
lap on every honest mid-drain position. Blocking only when a live record names the position keeps
the same safety (the unsafe case is exactly a stale stamp under a live reference) without that
liveness cost. Both readings block the reviewers' reproduced case.

I first added a guard that called a drop "stale" when the mark's event changed between the reading
and a retried drop. Working the cases through, without it the result is only ever the more
cautious block, and in the deployed loop nothing can cause it (the drain runs before the cleanup in
the same pass; GC writes no `reclaiming` mark while the gate is closed). I removed it rather than
ship an untested branch; the reasoning is in the module doc at "Every drop is conditional".

Tests: `f4_reclaiming_and_draining_marks_are_left_byte_identical` (`retirement_routing.rs:1176`) now plants the
draining mark on its own obligation's position (a real mid-drain state) and still completes;
`f4_a_draining_mark_on_a_live_fragment_withholds_the_marker_until_its_retirement_drains` (`retirement_routing.rs:1218`)
is the sign-off's extension: the unrelated obligation blocks the marker, drains, the next lap drops
the mark, then the routed unlink and GC before the drain reclaim nothing.

### 3. P1 — a placement the walk cannot trust named its chunk whole, and the walk dropped marks on it

Cause: `names` returned true for every position of a chunk whose placement fails its rule (v2
`:454-461`), `judge` turned a whole-chunk Committed/Obligation/Staged namer into `Drop` (v2 `:1038`),
and the drop re-check agreed, including the undecodable-`sidx:` case (v2 `:1121-1124`).

Fix:
- The judgement keeps the class picture per position and per whole chunk separately (`Held`,
  `Naming`, `:595-616`), and `strongest` (`:735`) says which won. A drop is judged only by a
  position namer whose placement proves the position (`:1234-1242`): if the strongest namer is a
  whole-chunk one and no trusted position namer would drop the mark too, the verdict is `Block`,
  naming that record (the inode, `seg:`, obligation or `sidx:` key), with its own audit reason
  (`emit_untrusted_namer`, `:1459`). The mark stays.
- `names` (`:544`) now proves only under a trusted placement, and the drop re-check of an
  undecodable `sidx:` value is false (`:1322`), so a record whose placement went bad between the
  reading and the drop makes the judgement stale instead of confirming it.

Block rather than Leave: leaving would certify a mark on a position the record may really name; a
later routed delete would then expose it to GC under that stale stamp. Block costs a lap only when
the window marks such a chunk, and the drain already holds the same records for a human
(`Hold::UntrustedPlacement`).

Tests: `f_an_untrusted_committed_placement_never_drops_a_mark` (`retirement_routing.rs:1803`, the adversary's X2:
RS(2,1) placement `[0,1]`, the old copy on server 5; after the operator's repair the copy's own
mark lets GC reclaim it) and `f_an_undecodable_owned_entry_never_drops_a_mark` (`retirement_routing.rs:1852`).

### 4. P1 — a restored `reclaiming` mark only a flat generation obligation names deadlocked drain, GC and walk

Cause: the walk blocked on a `reclaiming` mark under **any** namer (v2 `:1016`); the drain's arm (0)
waits for GC on a `reclaiming` mark no record it keeps names (base
`crates/custodian/src/retirement/marks.rs:477-480`, `AWAITING_GC`); GC with the gate closed holds
every mark, `reclaiming` included (`crates/custodian/src/gc.rs:718`). None of the three could move.

Fix (`crates/custodian/src/orphan_identity.rs:1181-1197`): a `reclaiming` mark whose position is named
only by pending `retire:bytes:` obligations (the `Obligation` class bit alone, through trusted
placements, no whole-chunk namer) is **drain-owned**: it is left and does not block. The lap can
then certify, the gate opens, GC resumes the reclaim it had decided, the drain finds the position
empty and writes arm (c), and the obligation goes. Under any other record (a committed map, a
staged/published/retiring part, an undecodable session, an untrusted placement) the walk still
blocks, as iteration 2's item 1 required. This needs the per-position class set from item 3; the
"strongest namer" alone could not tell "only obligations" from "an obligation and a weaker record".

Why this is safe: the fragment is already unreferenced, by the obligation's own event; no routed
delete can come for it later. Finishing the reclaim is exactly what the deployed #1062 drain
already expects in this state ("GC finishes the reclaim and deletes the key, and a later step takes
arm (c)", `retirement/marks.rs:34-43`), and what GC on the base does. What it does not give is the
drain's fresh grace for that one fragment; the reclaim was GC's decision before the gate existed
(or in the restored image), and no writer may replace a `reclaiming` mark, so nothing could give it
that grace in any case.

Rejected: letting GC finish `reclaiming` marks while the gate is closed. It breaks the brief's
"GC treats every mark as protecting" and the iteration-2 sign-off's "keep the closed-gate design",
and reopens iteration 1's P1 for a `reclaiming` mark under a live reference. Rough size: a guard on
the gate arm at `gc.rs:718` (~3 lines) — small, but the wrong rule.

Test: `f4_a_reclaiming_mark_only_a_retirement_names_lets_drain_gc_and_walk_finish` (`retirement_routing.rs:1264`): a
production Complete over an existing object installs `g:68:1`; a `reclaiming` mark on one of its
fragments; the production post-restore pass; repair and GC alternate. On iteration 2's code the
cycle is `Blocked, Partial` six times over and the obligation never drains.

### 5. P2 — every unreadable record kept its name and fault in memory

Fix (`crates/custodian/src/orphan_identity.rs:192`, `:655`): `Index::block` counts every
unreadable record (`unreadable`) but keeps name and fault only for the first
`MAX_BLOCKER_DIAGNOSTICS` (16). The lap needs only the first (`CleanupWalk` keeps one). The pass
emits the kept ones and one summary count for the rest (`:295`, `emit_unreadable_unnamed`).

Test: unit `the_reading_keeps_a_bounded_number_of_unreadable_records` (`crates/custodian/src/orphan_identity.rs:1610`): 40 undecodable
obligations, 40 counted, 16 named, the first in key order.

### X3 — the segmented-obligation drop path, as a test

`f1_a_stale_mark_only_a_segmented_retirement_names_is_dropped` (`retirement_routing.rs:1762`): a 180-chunk segmented
object from the production Complete is deleted through the routed unlink, its drain frozen; the
production post-restore pass closes the gate and marks every unreferenced fragment (the retired
object's 540 among them); a stale mark is planted on one. The walk drops all of those marks, each
pinned on the obligation and its `seg:` record (three passes at 256 drops a pass), completes, and
then the drain retires every fragment with none reclaimed before its marks or inside their grace.
It passes on iteration 2's code (the adversary said so too); it is red on the mutation the
adversary named (N6 below).

### Review-batch findings beyond the sign-off list

- `:806` (a stray `seg:` row past the root's table): fixed. The reading now treats a row at or past
  the root's `segment_count` as a blocker, as the shared resolver does (`SegmentUnknown`), and the
  drop re-check requires the index to be inside the table (`in_table`, `:1379`; the reading at
  `:982`). Honest note: the resolver already rejects such a row before the second scan, so only a
  row landing between those two reads reaches this check, and no test forces that race (mutation
  N7 below survives). I kept it because it is the resolver's own rule and costs one comparison.
- `:762` (the whole `inode:` scan past the backend's scan cap): recorded-rejected in
  `review-rejected.md`. The same repair pass runs reconstruction first, which makes the same whole
  scan (`crates/custodian/src/reconstruction.rs:631`), as GC does (`gc.rs:1308`); paging this one
  scan would not move the cap any deployment sees.
- The seeded-DST test gap: recorded-rejected as "Deferred — tracked in #660" (the brief puts seeded
  DST out of scope; the module doc carries the marker).
- The code review's efficiency note (resolve, then scan the segments again for their keys): kept as
  is. The shared resolver returns the ordered chunk list without saying which `seg:` record holds
  each chunk; exposing that is a change to a core API many readers share, outside this slice.

## Recorded, not resolved: stale marks from repair after the marker (adversary, iteration 2)

The sign-off asked me to record this and not decide it. The finding: repair can rebuild a fragment
onto a server it was displaced from earlier (`select_distinct_domains_excluding` excludes only the
survivors' domains, `crates/custodian/src/reconstruction.rs:1122`), and the move marks only the
displaced server (`reconstruction.rs:1156-1158`, `:1235`); nothing in the move touches a mark
already at the target, and GC skips a referenced fragment without touching its mark. So after the
marker exists, a live fragment can again carry an old mark. A later **routed** delete of that object
leaves the mark in place (the base's inline delete would have blind-put a fresh one over it), and
GC's keyed lookup, finding no token, judges the fragment by the old stamp before the drain re-stamps
it. Rebalance moves look the same. I have not reproduced the source through production repair (the
adversary could not either: the test rig's fragment bytes do not decode); the consequence is the
one X1 reproduced.

This patch does not change it. The options the adversary listed, with what each would take:
1. Fix the movers: in the repoint / move batch, delete (or re-stamp) any mark at the target
   position under a condition on its value. Two or three call sites plus their tests; it removes
   the cause.
2. Keep the cleanup walking after the marker. A change to `reconcile`'s early `Satisfied` return; it
   narrows the window to one lap but does not close it.
3. A tracked deferral, with the exposure stated in the docs.
This is a decision for the human at sign-off.

## The test file — `crates/custodian/tests/retirement_routing.rs` (new)

30 tests, all through `reconcile_step` with the deployed contexts, over
`RedbMetadataStore::in_memory()` behind a thin wrapper and a placement-aware D-server double;
deletes, overwrites, Complete and restore are the production functions. Iteration 2's 24 are kept
(one rewritten: the F4 draining mark now sits on its own obligation's position); 6 are new. Plus two
unit tests in `crates/custodian/src/orphan_identity.rs` (one new) and one new in
`crates/core/src/orphan_identity.rs`.

Run (bounded with `timeout`): `cargo test -p wyrd-custodian --test retirement_routing` → 30 passed;
`cargo test -p wyrd-core -p wyrd-custodian` → 68 result groups, 1,130 passed, 0 failed.

### Against iteration 2's production code ("fails today")

The new test file compiles against iteration 2's code (it uses no new type), so I put back
iteration 2's `crates/custodian/src/orphan_identity.rs`, `crates/core/src/orphan_identity.rs`,
`crates/custodian/src/gc.rs` and `crates/core/src/metadata.rs` and ran it:

| Test | Result on iteration 2 |
|---|---|
| `f6_a_damaged_walk_cursor_starts_a_fresh_lap` | FAIL: marker written with the stale mark still there (`Some("1000")` vs `None`) |
| `f4_a_draining_mark_on_a_live_fragment_withholds_the_marker_until_its_retirement_drains` | FAIL: pass answers `Changed` (marker written) where `Blocked` is required |
| `f_an_untrusted_committed_placement_never_drops_a_mark` | FAIL: pass answers `Changed` (the copy's mark dropped) where `Blocked` is required |
| `f_an_undecodable_owned_entry_never_drops_a_mark` | FAIL: same, `Changed` vs `Blocked` |
| `f4_a_reclaiming_mark_only_a_retirement_names_lets_drain_gc_and_walk_finish` | FAIL: `[Blocked, Partial] × 6`, the obligation never drains — the deadlock |
| `f1_a_stale_mark_only_a_segmented_retirement_names_is_dropped` | passes: X3 is a coverage test, red only by mutation (N6) |

The bounded-diagnostics unit test lives in the module, so it cannot run against iteration 2's file;
it is red by mutation (N5).

### Demonstrated reds on the final code (each a one-spot mutation, run, then reverted)

Run by a script under `$PDCA_SCRATCH/pdca-builder-1014-logs/mutate.py`, which applies the mutation,
runs the one named test with `timeout 900 cargo test`, records the result, and `git checkout`s the
file. The repo denies warnings, so each mutation is written to compile cleanly.

The brief's required reds:

| # | Mutation | Test | Failing assertion |
|---|---|---|---|
| R1 | drain dispatch removed from `reconcile_step` (`if false { … }`), routing left on | `a2_a_delete_past_the_batch_budget_retires_through_one_obligation` | `drained within 6` |
| R2 | `fits_one_batch` counts puts + deletes only | `a2_the_boundary_counts_preconditions` | 83-chunk delete wrote inline marks (`Some("10000000")` vs `None`) |
| R3 | trap mark dropped (published → drop, nothing pinned) | `f3_the_trap_mark_on_a_published_part_is_kept_byte_identical` | trap mark gone (`None` vs `Some("10000000")`) |
| R4 | stale mark on a staged part kept | `f2_a_stale_mark_on_a_staged_fragment_is_dropped_and_never_judged` | stale mark still there (`Some("1000")` vs `None`) |
| R5 | drop committed without the judged records' bytes | `f5_a_drop_racing_unlink_or_overwrite_fails_and_the_writers_mark_survives` | the unlink's own mark deleted (`None` vs `Some(…)`) |
| R6 | marker written at the end whatever the lap met | `f6_an_unreadable_record_blocks_the_marker_until_it_is_repaired` | `Changed` vs `Blocked` |
| R7 | marker read without its generation (`CleanupMarker::is_current` → `true`) | `f7_a_restore_turns_the_routing_off_until_the_cleanup_walks_again` | status `Complete` right after the restore pass |
| R8 | routing opens without a marker (`read_gate` answers open on no marker) | `f_before_the_marker_the_paths_behave_as_on_the_base_and_gc_holds_every_mark` | 100-chunk delete wrote no inline marks |

This iteration's fixes:

| # | Mutation | Test | Failing assertion |
|---|---|---|---|
| N1 | cursor check removed at decode | `f6_a_damaged_walk_cursor_starts_a_fresh_lap` | stale mark kept under the marker |
| N1u | same | unit `the_walk_cursor_must_lie_inside_the_ledger` | `"\xff"` accepted |
| N2 | draining mark under a live reference left and certified (`if false && live`) | `f4_a_draining_mark_on_a_live_fragment_…` | `Changed` vs `Blocked` |
| N3 | `reclaiming` mark blocks under an obligation-only namer too | `f4_a_reclaiming_mark_only_a_retirement_names_…` | obligation never drained |
| N4c | a whole-chunk namer may justify a drop | `f_an_untrusted_committed_placement_never_drops_a_mark` | `Partial` vs `Blocked` (the drop then fails its re-check and the window stalls) |
| N4s | same | `f_an_undecodable_owned_entry_never_drops_a_mark` | `Partial` vs `Blocked` |
| N5 | diagnostics cap removed | unit `the_reading_keeps_a_bounded_number_of_unreadable_records` | 40 kept vs 16 |
| N6 | `in_table` flipped on the segmented-obligation re-check | `f1_a_stale_mark_only_a_segmented_retirement_names_is_dropped` | status `Walking`, never `Complete` |

Not red, said plainly:
- **N7**, the segment-count check in `in_table` disabled: the whole suite stays green. Only a
  `seg:` row landing between the resolver's scan and the walk's own scan reaches it (see above).
- `names()` becoming trusted-only is reached only when a position namer's placement goes bad
  between the reading and the drop; with the judgement rule in place no whole-chunk namer reaches
  the re-check, so mutating `names()` alone stays green. Defence in depth, untested.
- The first R3 attempt did not compile (a variant left unconstructed under `deny(warnings)`); I
  rewrote it so it compiles and re-ran it. Not counted until it ran.

### C4-verify

The test file still needs this slice's types (`wyrd_core::orphan_identity`,
`wyrd_custodian::orphan_identity::status`), so on the unpatched base it does not compile, and
C4-verify will report **UNVERIFIABLE (exit 77)**, as the brief expects. The real reds are the
tables above, plus iteration 1's reviewer-recorded base red for A2's "one obligation, no inline
mark" (`iteration-v1/check-review.md`, C2), and iteration 2's reviewer's independent base red for
the same (`iteration-v2/check-review.md`, C2).

### Refuting my own test

- **(a) Genuine red?** Yes. Five of the six new integration tests fail against iteration 2's
  production code for the reasons in the table; the sixth (X3) and the new unit tests fail on the
  named mutations. The brief's eight required reds were re-run on this iteration's final code and
  each fails as stated. Every mutation was reverted with `git checkout` and the suite re-run green.
- **(b) Production path?** Yes. Passes go through `reconcile_step` with the contexts the deployed
  loop builds; deletes, overwrites, Complete and the post-restore pass are the production
  functions; the store is the real redb backend. The wrapper only freezes or refuses a commit,
  forces a race, fails one page, or runs a production `unlink` at a chosen read; it never answers a
  read itself. The unit tests call the module's own `Window::read` / `Index::read` over redb.
- **(c) Fixture includes the fault?** Yes. The walk record holds the exact damaged bytes
  `{"after":"ff"}`; the unrelated obligation is a real, decodable obligation in the store, drained by
  the production drain; the malformed placement is in the committed inode and the old copy is on
  the fleet; the undecodable `sidx:` value is in the store under a key naming the chunk; the
  deadlock fixture uses the production Complete's own obligation and the production post-restore
  pass, with GC and repair alternating and nothing frozen; the 40 undecodable obligations are in
  the store.

## Gates

`./engine/xtask.sh ci` (→ `cargo xtask ci` in `$PDCA_WORKTREE`), run twice: once after the code
and test changes, and once more on the final tree after two doc-comment edits. Both: **exit 0,
"xtask ci: all checks passed"** (final run: 234 test result groups, none failed). Steps: `typos`,
docs lint, docs render with link audit (101 pages), gitlink/unsafe/blackbox guards,
`cargo fmt --check`, clippy (workspace, and `wyrd-dst` under `--cfg madsim`), workspace build and
tests, cargo-machete, `cargo fetch --locked`, cargo-deny (both configs), conformance vectors,
statics, deploy-guard, and the madsim DST suite.

- `typos` and the docs renderer (the brief's external dependencies) are installed and ran inside
  that gate. No NEEDS-HUMAN external dependency.
- The target repo configures no git commit hooks (no `core.hooksPath`, no hook files); its format,
  lint, typos and docs checks are the ones `cargo xtask ci` runs. `cargo fmt --all` was run over
  the tree before the gate.
- `patch.diff` = `git diff 3c600b6 <final tree>`: 28 files, +5,308 / −136 (iteration 2's 28 files,
  of which this iteration changes 7: +748 / −125). It applies cleanly to `3c600b6`
  (`git apply --cached --check` against a `3c600b6` index), reverse-applies on the final tree, and
  `git diff --check` finds no whitespace errors. It still cannot apply to bare `origin/main`, for
  iteration 2's reason: its dependencies #1061, #1062 and #1013 are not merged there yet, so
  C4-diff-cov's "does not apply on origin/main" will recur until the stack merges or that gate uses
  the stack base. Nothing in the patch can fix that.
- Scratch logs (gate run, mutation runs and script, the run against iteration 2) are under
  `$PDCA_SCRATCH/pdca-builder-1014-logs`. Nothing was written outside the worktree, the bundle and
  that directory. The worktree carries local WIP commits on its detached HEAD (no remote, nothing
  pushed).
- I also wrote `review-rejected.md` in the bundle: the two review-batch findings I decline, in the
  format its triage rule asks for.

## Out of scope / deferred

- Seeded DST for the walk racing `unlink` / Complete: `deferred: #660` in the module doc.
- The stale-marks-from-repair exposure (above): recorded for the human's decision.
- GC cancelling a `reclaiming` mark under a live reference (iteration 2's item 1 follow-up): still
  a GC protocol change; still blocks for a human here.
- Bulk `DeleteObjects` batching (#509), gateway verbs, proposal/ADR edits, `Cargo.lock`,
  `deny*.toml`: untouched.

## Existing tests touched

Same as iteration 2 (fixtures only, no assertion changed). This iteration touched none of them.
