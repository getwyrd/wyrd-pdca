# Build notes — #777, iteration 3: reconstruction completes the `seg:` repair through #776's primitive

Target: getwyrd/wyrd @ main. Worktree HEAD `4bda59c` (the integration branch). It already holds
#776, so `metadata::repoint_chunk` is on the base (`crates/core/src/metadata.rs:3241`). Line
numbers are **post-patch** unless marked `base:` (`git show HEAD:…`).

This iteration starts from `iteration-v2/patch.diff` (it still applied cleanly). The v2 design
passed review; v3 changes only what the round-2 carry-forward and the T4 batch review asked for.

## Carry-forward items and what I did

| finding | fix |
|---|---|
| The move's store-fault arm (`reconstruction.rs:1188`, `Err(err) => return Err(err)`) was untested; the reviewer's mutant (`return Ok(contained(err.to_string()))`) survived the whole suite | New **leg 11**, `a_store_fault_under_the_moves_own_read_ends_the_pass` (`segmented_map_repoint.rs:847`). `MemMeta::get` now faults the next `get` of a `seg:` key, once (`:111-116`, field `faulting` at `:83`). The resolver never `get`s a `seg:` key (it uses `scan_page`), so this hits only the move's own read. The leg asserts: the fault fired; the pass returns `Err` whose text carries the injected fault; **no** `unresolvable-chunk-map` row, no counter tick, no name for `inode:1`; `CHUNK` still queued; segment 1 and the root byte-identical; no orphan mark; no fragment written on the free server. |
| T4 batch review (gating): 3× "no seeded Tier-0 DST coverage for the new concurrent path" | Added an in-code deferral marker, the mechanism the rubric's reviewer protocol names ("an in-code `// deferred: #N` marker is resolved for review purposes"): `reconstruction.rs:1157-1158` at the move, and in the test's module doc (`segmented_map_repoint.rs:30-31`). See "Why #682" below. |
| C4 diff coverage MISS at the store-fault arm | Now **65 of 65** instrumentable changed lines executed (100%), measured with `cargo llvm-cov --lcov -p wyrd-custodian --test segmented_map_repoint --test segmented_map_reconstruction` and scored with the gate's own hooks (`engine/scripts/run-diff-cov.sh --changed-lines` then `--score`): `TOTAL 65 65`, no `MISS` line. The `UNSCORED` lines are comments. |

### Why #682 for the DST deferral (and not #722)

v2's builder declined to add a marker because #722's DST property is the **drain's**
repoint-versus-supersede, not reconstruction's. That is still true: #722's body scopes its
`crates/dst/tests/custodian.rs` edit to the evacuation caller. But **#682** (OPEN,
"core,custodian: repoint_chunk — ceiling-safe placement moves in a segmented record") is the
umbrella both callers came from. Its *What* lists "the two callers: reconstruction's repair and
rebalance's evacuation" and "the repoint-versus-supersede property, added to the existing
`crates/dst/tests/custodian.rs`"; its *Acceptance* lists "the DST repoint-versus-supersede
property holds across the seed sweep". The repo already defers exactly this DST case to #682 for
the sibling write path: `crates/custodian/src/backfill.rs:217-218` ("deferred: #682 — that write
path, and the seeded Tier-0 DST case belonging to it, land together"). So `deferred: #682`
points at an open issue that names the property.

**For the human:** #682 must not be closed when #722 lands unless its DST case covers the
reconstruction caller too; otherwise re-point both markers to a new issue. Writing the DST case
here is ruled out by the brief: `crates/dst/tests/custodian.rs` is #722's file, and a new
`crates/dst/tests/*.rs` would be a fourth/fifth file and `#![cfg(madsim)]`, which the brief's
Falsifiability section says would wreck C4-verify.

## What did not change from v2 (summary)

- The base sent a `seg:`-resident obligation to `Site::Refused` (base: `:701`) and answered
  `Assessment::Refused` (base: `:368`, `:783`). Both are gone, with `emit_refused` (base:
  `:1375`) and `Reading::refused`.
- `read_committed` records a `Site { object, byte_offset, chunk_ref }` (`:571`) for every owed
  chunk, flat or segmented. The held root is `resolved.record` (`:694`), the generation the
  chunks were resolved from.
- `repair_chunk` follows the peer's order (`repair_chunk`, base `:829-956`): rebuild → choose
  targets → abort if a target is outside the fleet → **prepare** the move with
  `metadata::repoint_chunk` (`:1159`, reads only) → write fragments (`:1195`) → add the
  `repair_key` delete (`:1198`) and one orphan mark per displaced position to the batch the
  primitive handed back → one `commit` (`:1206`). The base's inline record build and ceiling
  check (base `:1143-1178`) are gone; the ceiling refusal comes from inside the primitive.
- Mapping of `Repoint`: `Prepared` → commit. `Refused` → `RepairOutcome::Refused` (a hole,
  `Blocked`). `Conflict` → `RepairOutcome::Conflict` (not a hole; `Satisfied` when it is the
  only outcome, per the human's 2026-08-19 decision). `VersionExhausted` and a typed
  `ChunkMapError` from the move → `RepairOutcome::Contained` (`:1064`), which `reconcile` turns
  into `reading.contain` (`:448`). Any other `Err` — a store fault, or a
  `MalformedReplacement` (a caller bug) — ends the pass (`:1188`), now pinned by leg 11.
- `hole` (`:490`) only lost the `!reading.refused.is_empty()` term. Conflicts were not added.
- Guard (a), once per OBJECT: `Reading::contain` (`:538`) de-duplicates through
  `contained: BTreeSet<Vec<u8>>` (`:532`); leg 7 pins it on the move path.
- Guard (b), no new silent skip: the base's `(Some(_), None) => continue` is kept as is
  (`:656`, #698). A **segmented** record under an unparsable key is contained (`:682`), not
  skipped; leg 8 pins it. `parse_inode_key` is untouched.

## Files and budget

| file | + | − |
|---|---|---|
| `crates/custodian/src/reconstruction.rs` | 202 | 211 |
| `crates/custodian/src/reconstruction/staged.rs` | 0 | 1 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` (forced edit) | 50 | 51 |
| `crates/custodian/tests/segmented_map_repoint.rs` (new) | 873 | 0 |

- Added non-blank, non-comment production lines: **81** (budget ≤ 100). v3 added only comment
  lines to production (the marker).
- `patch.diff`: **84,430 bytes** (budget ≤ 85 KB; under 85,000 even read as decimal KB).

**Fourth file — still a C3 question for the human (unchanged from v2).** The brief budgets 3
files. `RepairPlan.chunk_index` was read only by the base's inline record build (base
`reconstruction.rs:1152`), which the brief tells me to replace with the primitive. Left in
place, the field is written by `staged.rs:369` and read by nothing, which is a `dead_code` error
under `clippy -D warnings`. Costs of staying in 3 files, concretely:

- Keep the field with `#[allow(dead_code)]`: +1 attribute line, and a field no code reads.
- Keep it live: `repoint_chunk` takes `byte_offset` + `prior` (`metadata.rs:3241-3248`), not an
  index. Turning an index into an offset inside `repair_chunk` needs each object's resolved chunk
  list held in `Object` (about +6 lines), and for a flat object that list is a second copy of
  `prior.chunk_map`. For a segmented object it would hold the whole resolved list per object.
- Chosen: delete the field and its one writer, `chunk_index: site.index,` (base `staged.rs:369`):
  −1 line, no behaviour change. `staged` still reads `site.index` at `staged.rs:285` and `:355`.

## Test: `crates/custodian/tests/segmented_map_repoint.rs` (new, no `#![cfg]`)

Drives only `reconcile_step` and asserts on the store (plus the audit seam for once-per-object
counts). Names only base symbols: `ReconcileError` (`crates/custodian/src/reconciliation.rs:89`,
already used by `segmented_map_reconstruction.rs:39`), `MAX_VALUE_BYTES`, `orphan_key`. Never
calls `repoint_chunk`. Race hooks as the brief requires: `AfterSegmentPage` fires inside
`scan_page` for a `seg:` prefix after the page is built; `IntoCommit` at the top of `commit`;
the leg 11 fault on the first `get` of a `seg:` key. Every racing/faulting leg asserts its hook
fired.

**11 tests, not the brief's 5.** Legs 6–10 were added in v2 at the carry-forward's request and
to cover the other new caller paths; leg 11 is this round's. So C4-verify should report
**11 ran, 10 failed** on the red leg, not "5 ran, 4 failed".

1. seg-resident chunk repaired: `Changed`, `[[0,2],[0,1]]`, fragment on server 2 (distinct
   domain), queue empty, orphan keys == `[orphan_key(LOST, (CHUNK,1))]`, root and segment 0
   byte-identical.
2. sibling race merged: `Changed`, `[[0,2],[0,7]]`, queue empty.
3. planned chunk raced: segment 1 == racer's bytes, root unchanged, still queued, no orphan,
   `Satisfied`. Nothing asserted about the destination fragment (#723).
4. root superseded into `commit`: segment 1 unchanged, still queued, no orphan, root == the
   competing bytes, `Satisfied`.
5. ceiling at full `MAX_VALUE_BYTES`: seeded so the repoint encodes to exactly
   `MAX_VALUE_BYTES + 1` (growth measured, seeded programmatically); byte-identical, queued,
   `Blocked`.
6. torn record under the move: `Blocked`, one row + one tick for `inode:1`, nothing written.
7. same, two obligations in one record: still ONE row, ONE tick (guard (a)).
8. segmented root at `inode:x`: `Blocked`, `[DELETED, CHUNK]` both kept (guard (b)).
9. flat record at `version = u64::MAX`: contained.
10. flat map whose lengths overflow before the owed chunk: contained.
11. **new** — store fault under the move's `seg:` read: `Err`, no containment row, nothing
    drained or written.

## Red → green

`engine/scripts/run-verify.sh` creates a `git worktree add -B <branch>` under `../wyrd-verify*`
in the host repo — outside the roots I may write, and it creates a branch in the shared git
metadata. So, as in v2, I ran its red leg by hand **in the cycle worktree**, with the same
`cargo test -p wyrd-custodian --test …` the gate runs, bounded by `timeout 900`. Production files
were copied aside to `$PDCA_SCRATCH/pdca-builder-777-v3`, reset with `git checkout HEAD --`,
tested, copied back, and `git diff` before/after compared with `cmp` (identical).

- **Green (fix applied):** `segmented_map_repoint` **11 passed**; `segmented_map_reconstruction`
  6 passed.
- **Red (`reconstruction.rs` + `staged.rs` at `HEAD`, tests kept):** `segmented_map_repoint`
  **11 ran, 10 failed**:
  - leg 1 `:493` `left: Blocked, right: Changed`
  - leg 2 `:552` `left: Blocked, right: Changed`
  - leg 3 `:472` `left: Blocked, right: Satisfied`
  - leg 4 `:456` "the race never landed" (base never reaches `commit`)
  - legs 6, 7 `:733` `left: (0, 0, true), right: (1, 1, true)`
  - leg 8 `:769` `left: [41472], right: [3584, 41472]` (base drained `DELETED`)
  - leg 9 panicked at base `reconstruction.rs:1157:18` (`version + 1` overflow)
  - leg 10 `:803` `left: Changed, right: Blocked`
  - **leg 11 `:854` "fixture: the fault never fired"** — the base refuses the object and never
    reads its `seg:` record with `get`; it answers `Blocked`.
  - leg 5: ok (green by construction, as the brief expects)

  `segmented_map_reconstruction`: the rewritten leg 2 fails at `:500`
  (`left: Blocked, right: Changed`); the other 5 pass.

Legs 3 and 4 going red only shows "refused" versus "attempted"; the pins behind them are bound by
the mutation table, not by that red.

## Named negations (mutation oracle)

New this round, demonstrated on the final tree (file restored after, `cmp` confirmed):

| # | mutation | result |
|---|---|---|
| M11 | `repair_chunk` `:1188`: `Err(err) => return Err(err)` → `Err(err) => return Ok(contained(err.to_string()))` (the reviewer's mutant) | **leg 11 only** fails (`:858`, `expect_err` on an `Ok(Blocked)`); the other 10 pass |

The v2 negations, **re-run this round on the final tree** with the same driver
(`$PDCA_SCRATCH/pdca-builder-777-v3/mutate.py`, which restores each file in a `finally`; `git
diff` before/after compared with `cmp`, and `crates/core` confirmed clean). M1–M3 mutate
`crates/core/src/metadata.rs`, which is not part of the patch. Each row: 11 ran, only the named
legs failed.

| # | mutation | legs red |
|---|---|---|
| M1 | `chunk_at` (`metadata.rs:3374`): `chunk == prior` → `chunk.id == prior.id` | leg 3 |
| M2 | segmented arm (`metadata.rs:3325`): drop the root pin | leg 4 |
| M3 | segmented arm (`metadata.rs:3324-3328`): drop the ceiling weigh | leg 5 |
| M5 | `repair_chunk` `:1187`: typed move error → `RepairOutcome::Conflict` | legs 6, 7 |
| M6 | `Reading::contain` `:539`: name on every call, not once per key | leg 7 |
| M7 | `read_committed` `:682`: drop `reading.contain` for an unparsable segmented key | leg 8 |
| M8 | `repair_chunk`: `VersionExhausted` → `RepairOutcome::Conflict` | leg 9 |
| M9 | `reconcile` `:449`: drop `reading.contain` in the `Contained` arm | legs 6, 7, 9 |
| M10 | `read_committed`: overflowed offset saturated instead of contained | leg 10 |
| M11 | `repair_chunk` `:1188`: store fault → `contained(..)` | leg 11 |

Leg 2's negation (pin the bytes the resolve saw) has no one-line form inside the primitive; leg 2
is binding-red on the base anyway. Leg 5 does not distinguish `MAX_VALUE_BYTES` from a V/2 bound
(a V/2 bound would also refuse); core's own tests pin both sides of the bound.

## Refute-your-own-test

- **(a) Genuine red? Yes.** With `reconstruction.rs` and `staged.rs` at `HEAD` and the tests
  kept: 11 ran, 10 failed, each for the reason listed above. The new leg 11 also goes red under
  the exact mutant the reviewer used (M11), with every other leg green — so it binds the arm it
  was written for, not just "the base never moves".
- **(b) Production path? Yes.** Every leg calls the public `reconcile_step` with a real
  `ReconstructionContext`; the pass runs the production `read_committed` → `assess` →
  `repair_chunk` → `metadata::repoint_chunk` → `MetadataStore::commit`. Fragments are real
  `erasure::encode` + `encode_ec_fragment` bytes so the production verify passes. The only
  doubles are the in-memory `MetadataStore` / `ChunkStore` the brief names; the hooks sit on the
  seam methods. Leg 11's fault is a plain `std::io::Error` from `MetadataStore::get`, the same
  shape the twin leg uses (base: `segmented_map_reconstruction.rs:114`).
- **(c) Fixture includes the fault? Yes.** Server 1 holds a committed fragment position and is in
  neither fleet nor topology, so the fragment really is missing. Racing writes are real encoded
  records or real torn bytes that land (`raced()` asserted). Leg 11's fault really fires on the
  move's read (`!faulting` asserted), after the resolver succeeded, so the pass is past the
  reading when it meets it.

## Things for the human (deferred findings, not code defects)

- **C3, fourth file** — see "Files and budget".
- **DST deferral to #682** — see "Why #682". Confirm #682 keeps the reconstruction-side DST case,
  or re-point the two markers.
- **#776's docs deferral now points at this issue.** `crates/core/src/metadata.rs:3238-3240`
  says "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3,
  `08-crosscutting-concepts.md` §8.7) … nothing calls this yet." After this patch something
  calls it. `metadata.rs` is off-limits and the file budget is 3, so I could not update the
  marker. I read §6.3 (`docs/design/architecture/06-runtime-view.md:35-47`): "update the chunk's
  location via a single atomic metadata mutation" is now true for segmented objects too, and no
  sentence there or in `08-crosscutting-concepts.md` says reconstruction refuses segmented
  objects (grep for `seg:`/`segmented`/`refus`). So no doc sentence became false. The rubric's
  docs-currency trigger (port, API operation, RPC, CLI flag, persisted field) is not hit: no
  persisted format changed. The stale marker still needs a decision: retire it in a follow-up,
  or let this PR touch `metadata.rs` for one comment.
- **#698 interaction (not fixed, by instruction).** A segmented root under a noncanonical key
  (`inode:01`) parses to inode 1; the move pins `inode:1`, loses every pass, and a conflict-only
  pass answers `Satisfied`. The base refused it and answered `Blocked`. The flat arm has always
  behaved this way, and the brief gives noncanonical keys to #698. Worth noting on #698 that it
  now covers segmented objects too.
- **`emit_conflict`'s message** still says rebuilt fragments are "collectable garbage"; #723 owns
  that wording, so it is unchanged.

## Gates run locally

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean.
- `typos` over the four changed files: clean.
- `cargo llvm-cov` + the gate's scorer: 65/65 (see above).
- `./engine/xtask.sh ci` (the project's runner, `cargo xtask ci`, with `PDCA_WORKTREE` set to the
  cycle worktree, bounded by `timeout 5400`): **`xtask ci: all checks passed`, rc=0**, on the
  final tree (before the mutation runs, which restored it byte for byte). The log shows each
  external tool the brief names actually running, none skipped: `typos`, `render_site.py
  --check` ("link audit OK"), `cargo-machete`, `cargo deny` (three invocations), plus fmt,
  clippy, build, the workspace tests (including `segmented_map_repoint`), conformance vectors,
  statics, deploy-guard, and the `--cfg madsim` DST clippy + tests. `cargo-mutants` is the
  advisory C5 gate's tool; I did not run it — the named negations above stand in for it here.

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_777`):
`pdca-builder-777-v3` holds the reverted-file copies, `before.diff`/`after.diff`, `red.log`,
`lcov.info`, `lines.txt`, the candidate `patch.diff` and `xtask-ci.log`. The `cargo llvm-cov`
build cache is `target/llvm-cov-target/` inside the cycle worktree. Nothing deleted; the harness
reclaims both roots.
