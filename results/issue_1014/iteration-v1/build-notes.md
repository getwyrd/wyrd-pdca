# Build notes — #1014 (#659 child 3): orphan-identity migration gate and the unlink/supersede routing

Target: `getwyrd/wyrd` @ main, stacked base `pdca-integration/main-r9dd123c9ed70` (`3c600b6`,
which carries #1061, #1062 and #1013). Every `path:line` below is on that base unless it says
"new file".

## What the patch does

1. **The marker and its gate** — new file `crates/core/src/orphan_identity.rs`.
   - `ORPHAN_IDENTITY_KEY = b"migration:orphan-identity"`, value `CleanupMarker`
     `{"fence_generation":N}` or `{}` (absent restore-fence record). Closed, canonical, absent
     generation omitted (rubric: serialization identity; round-trip and refusal unit tests in the
     same file).
   - Valid only while `mpufence` (`crates/core/src/multipart.rs:1312`) names the same
     generation **number** (`CleanupMarker::is_current`). I bound to the number, not the bytes:
     the post-restore pass rewrites the record when it *completes* its generation
     (`crates/custodian/src/restore.rs:952-963`, `close_generation`) without opening a new one,
     so a bytes binding would discard a marker for no restore. Every post-restore pass opens a
     new number first (`restore.rs:918`, `open_generation`), so an image's marker reads stale
     the moment that pass runs. This is the brief's mechanism (Design, "The marker"); I did not
     choose the "post-restore pass deletes the marker" alternative because that needs a write in
     `restore.rs` and still relies on the same pass having run.
   - `read_gate()` = two keyed reads; fail-closed on anything unreadable. `OpenGate::pinned()`
     adds `require(marker bytes)` and `require(mpufence bytes)`/`require_absent` to a batch.
   - The cleanup's own lap record `CleanupWalk` at `custodian:orphan-identity-walk` lives here
     too, only because the custodian crate has no `serde` dependency and adding one would edit
     `Cargo.lock` (out of scope per the brief).

2. **Routing** — `crates/core/src/metadata.rs`.
   - `unlink` (`metadata.rs:2143-2195` on base; the inline mark loop was `:2169-2192`, the
     segmented refusal `:2173-2179`) and `commit_chunk_map_superseding{,_leased}`
     (`:2263-2308`, `:2462-2515`) now build their base batch and hand it to `with_retirement`.
   - `with_retirement`: flat prior → build the inline batch exactly as before; if
     `fits_one_batch` (preconditions + puts + deletes ≤ `MAX_BATCH_OPS` = 500,
     `multipart.rs:5926`; bytes ≤ `MAX_BATCH_BYTES`, `:5887`) keep it **without reading the
     gate** (so a small delete costs no extra read and is byte-identical to base). Otherwise read
     the gate; closed → inline (base); open → `gate.pinned(base) + require_absent(retire key) +
     put(obligation)` via `RetireObligation::superseded_generation` (`multipart.rs:4713`, the
     constructor Complete already uses, `complete.rs:1904`).
   - Segmented prior: `segmented_gate` reads the gate first ("shape first, lease second" kept
     for the leased variant). Closed → the base `SegmentedMapUnsupported` error. Open →
     obligation naming the group.
   - Fallback to inline (base behaviour) if the obligation would pass
     `RETIRE_VALUE_CEILING` (`multipart.rs:4092`) — a legacy flat map from before the map
     ceiling — or if its constructor refuses a chunk's scheme. Both would be obligations the
     drain holds forever.
   - Batch order is unchanged for the inline path: preconditions vector and puts vector match
     base exactly (base `require(inode), put(inode), marks…, guards…`; the new code puts guards
     into `preconditions` before marks are appended to `puts` — separate vectors, same content).

3. **The cleanup walk** — new file `crates/custodian/src/orphan_identity.rs`, dispatched from
   `reconcile_step` right after the retirement leg (`crates/custodian/src/reconciliation.rs:168`
   on base), i.e. on the reconstruction input the deployed loop passes every interval
   (`crates/server/src/custodian.rs:612-622`, `reconcile_pass` → `reconcile_step` at `:397`).
   - Valid marker → two reads, `Satisfied`. Otherwise one bounded window per pass
     (`MARKS_PER_PASS` 16,384 in pages of 1,024; 4,096 keyed reads; 256 drop commits).
   - Reference picture read in GC's order (staged sidx → parts → committed inodes → pending
     `retire:bytes:`), each position mapped to its strongest namer.
   - Judgement table in the module doc. Brief's rows kept as written, with three deliberate
     differences, each argued below.
   - Every drop is one commit: `require(mark bytes)` + `require` on every judged record,
     **re-read and re-checked** at drop time (inode [+ `seg:`], obligation [+ `seg:`], `part:`
     or `sidx:` + `mpu:`). Lost precondition → re-judge on the fresh mark value (2 attempts);
     record no longer names the position → **unsettled** → that lap writes no marker.
   - Marker commit requires the lap record, the `mpufence` record and the marker as read, puts
     the marker, deletes the lap record. A lap that met an unreadable record ends with no marker,
     keeps the blocker's name readable (`status()` → `CleanupStatus::Blocked { record }`) until the
     next pass starts a fresh lap.

4. **GC gate** — `crates/custodian/src/gc.rs`. In `judge` (`gc.rs:651`, the `match
   window.mark_of` at `:676`), a stamped or `reclaiming` mark holds its fragment while the gate
   is closed (`orphan-identity-gate-closed`), and the pass answers `Partial` rather than
   `Satisfied`. The gate is read lazily (`Sweep::gate_open`), once per pass, the first time a
   mark is met — always after both reference sets. The fragment-less sweep is left ungated: it
   never destroys bytes.

5. **Docs** — `docs/design/architecture/06-runtime-view.md` (after the terminal-delete
   paragraph, base `:92`): two paragraphs, routing and the migration gate, with the restore
   caveat. `05-building-block-view.md` (after the `mpufence` paragraph, base `:256`): the two
   singletons — key, value, writer, deleter, readers, scans (ADR-0046). Server DELETE doc
   comment `crates/server/src/lib.rs:690-695` updated.

## Decisions a reviewer may question

### GC: "every mark protects" taken literally (and what it cost)

The brief (F) and `0016:1269-1273` say GC treats every `orphan:` mark as protecting until a
valid marker is observed. I implemented exactly that. The cost: 84 existing custodian tests
failed (15 files), plus 12 DST tests, all of them GC-only or repair-pass fixtures that reclaim
marked fragments on a store with no marker. I fixed them in the **fixtures**, not the
assertions: each store now starts "migrated" (marker planted), with a comment saying why. The
restore flows that legitimately invalidate the marker got the realistic step instead:
`restore_reconcile.rs` runs one repair pass after the post-restore pass (what the deployed
loop does); `staged_protection.rs` plants the marker for the new generation (a repair pass there
would also drain the sessions the restore fenced, which those legs keep — first attempt did
that and 8 tests failed for the wrong reason).

Rejected alternative, measured: hold marks pre-marker **only while any `retire:bytes:`
obligation is present** (read once after the reference sets). I believe it is sound (every
pre-marker unreference that does not write a fresh inline mark installs a byte obligation). Of
the 84 failures, the 23 in the GC-only files (`gc_delete_backstop` 2, `gc_ledger_walk` 8,
`gc_mark_sweep` 2, `gc_reclaim_intent` 8, `gc` 2, `gc_telemetry` 1) would have stayed green
under it; the rest come from the cleanup running inside repair passes and would have needed the
same fixture change either way. I rejected it because it narrows an explicit
brief/0016 rule, and the F7 wording ("GC treats every mark as protecting") would not hold for a
store with no pending obligation. If the human prefers the narrower gate, it is a ~6-line
change in `Sweep::gate_open` plus the reverts of the fixture seeding.

### Three differences from the brief's judgement table

1. **Added row: pending-obligation positions are dropped.** The brief's table leaves "named by
   nothing" alone. A fragment named only by a `retire:bytes:` obligation installed before the
   walk reached it (multipart Complete's superseded generation, `complete.rs:1904`, is live on
   the base today; a part re-upload's `{chunks}` too) is "named by nothing" by records, but its
   most recent unreference event is that obligation. A stale mark left there is exactly what the
   keyed lookup (`gc.rs:760`) would reclaim on once the gate opens, before the drain re-stamps.
   Dropping it is leak-free (the drain writes arm (c) later). Test leg
   `f_a_stale_mark_on_a_fragment_only_a_pending_obligation_names_is_dropped`, red 9 below.
2. **Ended sessions are "leave", not "drop".** The brief says drop for "the `part:` record of a
   part no Complete published". For an `Aborting` session (never published) and a `Completed`
   session's residue/unnamed parts, I leave the mark: their own drains (teardown
   `retire:bytes:`, the owned walk) re-stamp every position before deleting the record, so no
   hazard remains, and dropping interferes with the owned walk's mark-then-delete (its marks
   carry `<upload-id>:<epoch>`, not a retire token, so the "obligation present" row does not
   cover them). Observed: with them dropped, `multipart_session_terminal` tests failed with
   owned-entry positions found unmarked mid-walk. `Open`/`Completing` sessions (and undecodable ones) are dropped as the brief
   says — F2 tests exactly that.
3. **Completeness is GC's own test, plus two.** Blocks the marker: an inode that will not
   decode/resolve, a `seg:` record (named by its own key, not the inode's), a `part:` record, a
   session key that names no upload, an `sidx:` key naming no chunk — GC's `unresolvable` sets
   (`gc.rs:528`). Added: an undecodable pending `retire:bytes:` obligation (its positions are
   unknown) and an undecodable `retire:records:` of a `Completed` session (publishedness
   unknown). Does **not** block, because GC contains them rather than counting them as a hole:
   untrusted placements, an `sidx:` value that won't decode under a key naming its chunk, a
   session value that won't decode. First attempt blocked on those and the marker could never be
   written on stores the GC tests consider healthy.

### No re-stamping

As the brief requires. The cleanup only deletes marks; it never writes one.

### Unsettled drops

When the index read at pass start is stale for a mark (a Complete flipped, an unlink raced),
the drop is not attempted on stale grounds; the lap ends without a marker and the next lap
judges on a fresh reading. Safe direction; costs one more lap under churn.

## The test file — `crates/custodian/tests/retirement_routing.rs` (new)

15 tests, all through `reconcile_step` with the deployed contexts (repair pass = reconstruction
input alone; GC = its own pass), over `RedbMetadataStore::in_memory()` behind a thin wrapper
(forced race, frozen key = "drain not there yet", one failing ledger page), and a
placement-aware D-server double. Deletes via production `metadata::unlink`, overwrites via
production `write::commit_overwrite`, multipart via production Create + Complete.

Coverage map: F (pre-marker base behaviour + GC holds + post-marker reclaim), F1, pending-
obligation row, F2 (sidx + Open part dropped; publish → records drain → routed unlink; GC
before drain / inside grace / after), F3 (trap kept byte-identical, records drain, reclaimed
after its grace), F4 (`reclaiming` + draining marks byte-identical), F5 (race with unlink and
with overwrite, at the same stamp so only the record pin can tell them apart), F6 (inode, `seg:`,
`part:` fixtures; blocked + named + routing off; repaired → marker → routing on; interrupted
walk), F7 (production `reconcile_after_restore`; stale marks; GC holds; inline delete; cleanup
walks; marker `{"fence_generation":1}`; then routed delete retires through the drain), A1 (270
fragments), A2 (one obligation, no mark; present-then-absent; 83-chunk RS(4,2) = 502 ops routes,
82 = 496 stays inline), A3 (overwrite prior generation; segmented delete, 540 fragments).

Run: `cargo test -p wyrd-custodian --test retirement_routing` → 15 passed.

### Demonstrated reds (each: patch the fix, run the named test, restore — all re-run on the final code)

| # | Patch | Test | Failing assertion |
|---|---|---|---|
| 1 | drain dispatch removed from `reconcile_step` (routing left on) | `a2_a_delete_past_the_batch_budget…` | `drained within 6` — obligation never drains, fragments stay |
| 2 | `fits_one_batch` counts puts + deletes only | `a2_the_boundary_counts_preconditions` | 83-chunk delete wrote inline marks (`Some("10000000")` vs `None`) |
| 3 | trap mark dropped (Published → drop, nothing pinned) | `f3_the_trap_mark…` | trap mark gone (`None` vs `Some("10000000")`) |
| 4 | stale mark on a staged part kept | `f2_a_stale_mark…` | stale mark still there (`Some("1000")` vs `None`) |
| 5 | drop committed without the judged record's bytes | `f5_a_drop_racing…` | the unlink's own mark deleted (`None` vs `Some("10000000")`) |
| 6 | marker written at the end whatever the lap met | `f6_an_unreadable_record…` | pass answers `Changed`, not `Blocked` |
| 7 | marker read without its generation | `f7_a_restore…` | status reads `Complete` right after the restore pass |
| 8 | routing opens without a marker | `f_before_the_marker…` | 100-chunk delete wrote no inline marks |
| 9 | pending-obligation stale mark kept | `f_a_stale_mark_on_a_fragment_only_a_pending_obligation…` | stale mark still there |

A first run of reds 3 and 5 reported "green"; both were actually build failures (the repo
denies warnings; the patches left a dead variant / an unused `mut`). Fixed the patches; both
then failed as required.

### C4-verify

The file needs this child's new types (`wyrd_core::orphan_identity`, `wyrd_custodian::
orphan_identity::status`), so on the unpatched base it **does not compile** → C4-verify will
report **UNVERIFIABLE (exit 77)**, as the brief anticipated. The base-behaviour red for A2's
"one obligation, no inline mark" assertion is shown by red 8 (routing behaves as base) and by
the base's own code: `unlink` on base always writes the 900 marks inline (`metadata.rs:2180-2191`).

### Refuting my own test

- **(a) Genuine red?** Yes. Nine reverted-fix patches above, each red on the named test, re-run
  on the final code. Not a stand-in: each patch reverts one specific line of the fix.
- **(b) Production path?** Yes. Passes go through `reconcile_step` with the contexts
  `run_reconstruction_until` builds; deletes/overwrites/Complete/restore are the production
  functions; the store is the real redb backend (the wrapper only forces a race, defers a drain
  commit, or fails one page — it never answers reads itself).
- **(c) Fixture includes the fault?** Yes. The stale marks are planted on the positions that
  matter (committed, staged, trap, pending-obligation); the race runs a real `unlink` /
  `commit_overwrite` against the same store mid-drop at the same stamp; the unreadable records
  are planted in the store the walk reads; the restore is the production post-restore pass;
  the "before the drain" GC passes run with the clock 10× the grace past every older mark.

## Gates

- `cargo xtask ci` (via `engine/xtask.sh ci`) first run: everything green except 12 DST tests
  (`crates/dst/tests/custodian.rs`, the same GC-gate fixture cause) → fixed by seeding the DST
  stores (`MemMeta` Default; a `with_record` builder added to the shared simulated-TiKV support,
  `crates/dst/tests/support/mod.rs`, which already allows dead code). Second run: see the result
  line appended at the end of this file.
- `typos` and the docs renderer (the brief's external dependencies) are installed and ran inside
  `cargo xtask ci` (lint and render OK). No NEEDS-HUMAN external dependency.
- `cargo fmt --all` and clippy (`-D warnings`) clean on every touched crate.

## Out of scope / deferred

- Seeded DST for the walk racing unlink/Complete: `deferred: #660` marker in the module doc
  (the brief puts seeded DST out of scope). The race is forced by a wrapper in F5 instead.
- Bulk `DeleteObjects` batching (#509), gateway verbs, proposal/ADR edits, `Cargo.lock`,
  `deny*.toml`: untouched.

## Existing tests touched (fixtures only, no assertion changed)

`crates/custodian/tests/`: `gc.rs`, `gc_delete_backstop.rs`, `gc_ledger_walk.rs`,
`gc_mark_sweep.rs`, `gc_reclaim_intent.rs`, `gc_telemetry.rs`, `reconstruction.rs`,
`segmented_map_reconstruction.rs`, `segmented_map_repoint.rs`, `staged_repair.rs`,
`multipart_retirement_drain.rs` (store constructors made async to plant the marker; `.await`
added at the call sites), `multipart_retirement_marks.rs`, `multipart_session_terminal.rs` —
marker planted at store construction. `restore_reconcile.rs` — one repair pass inserted after
the post-restore pass (new `repair_pass` helper). `staged_protection.rs` — marker for the new
generation planted after the post-restore pass at five sites (`cleanup_done`).
`crates/dst/tests/custodian.rs` + `support/mod.rs` — seeded stores.

## Final gate result

`./engine/xtask.sh ci` (→ `cargo xtask ci` in `$PDCA_WORKTREE`) on the final tree: **exit 0** —
typos, docs lint + render, repo guards, `fmt --check`, clippy `-D warnings`, workspace build and
tests, cargo-machete, cargo-deny (both configs), statics, deploy-guard, and the madsim DST suite
(`wyrd-dst` custodian: 35 passed). `patch.diff` was written from that same tree
(`git apply -R --check` clean). Scratch logs (gate runs, red runs) are under
`$PDCA_SCRATCH/pdca-builder-1014-*`; nothing was written outside the worktree and the bundle.
