# Build notes — #777, iteration 2: reconstruction completes the `seg:` repair through #776's primitive

Target: getwyrd/wyrd @ main. Worktree HEAD `4bda59c` (the integration branch). It already holds
#776 as merge `36f006d` (PR #844), so `metadata::repoint_chunk` is on the base
(`crates/core/src/metadata.rs:3241`). Line numbers are **post-patch** in
`crates/custodian/src/reconstruction.rs` unless marked `base:` (`git show HEAD:…`).

This iteration starts from `iteration-v1/patch.diff` (it still applied cleanly) and fixes every
carry-forward item on top of it. What changed since v1 is listed first; the design that v1
already had is summarised after it.

## Carry-forward items and what I did

| finding | fix |
|---|---|
| Leg 1 checked the orphan mark by count only | Leg 1 now asserts the full key list equals `[metadata::orphan_key(LOST, FragmentId { chunk: CHUNK, index: 1 })]` (`segmented_map_repoint.rs`, leg 1). `orphan_key` is a base symbol (`crates/core/src/metadata.rs:73`), so the red leg still compiles. |
| Move-time containment had no test; hand-disabling it left the whole suite green | New legs 6 and 7 (`a_segment_record_torn_under_the_move_is_contained`, `two_obligations_meeting_one_torn_record_are_one_containment`) arm torn bytes at `seg:…:1` after the resolver's page. They assert `Blocked`, every obligation still queued, the torn bytes untouched, root untouched, no orphan, no fragment written, and exactly ONE `unresolvable-chunk-map` row + ONE `reconstruction_unresolvable_records` tick naming `inode:1`. The reviewer's exact mutation (drop `reading.contain` in the `Contained` arm) now turns legs 6, 7 and 9 red (table below). |
| The `if let Target::Committed(site)` in the `Contained` arm could never fail to match | `RepairOutcome::Contained` now carries the object's key (`:1064-1069`), built in `repair_chunk` (`:1165`). The arm in `reconcile` (`:448-451`) is just `reading.contain(&object, &fault); emit_aborted(..)`. No dead branch. |
| Stale comment at base `:316` ("Like the `seg:` refusal") | Reworded (`:313-319`). |
| Stale comment at base `:418-427` ("conditioned on the generation THE SCAN returned", "a second obligation … still loses the CAS") | Rewritten (`:410-426`): flat objects still lose the second CAS and wait a pass; segmented objects land both, because the root is never rewritten and each move re-reads its own `seg:` record. The staged paragraph now says "as in a flat object". |
| Test module doc wrong about leg 4's base red | Fixed: leg 4 is red on the base at "the race never landed", because the base never reaches `commit`. |
| C4 diff coverage 71% (below 80%) | Now **64 of 65** instrumentable changed lines executed (98.5%), measured with `cargo llvm-cov` over `--test segmented_map_repoint` on the final tree and scored by `engine/scripts/run-diff-cov.sh --changed-lines` / `--score` (the gate's own pure hooks). The one miss is `reconstruction.rs:1185` (`Err(err) => return Err(err)`, a store fault under the move). |
| T4 Contribution: prior art for all four files | See "Prior art" below. This is about what the reviewer's sandbox can see, not about the patch, so there is nothing to change in the diff. |

I also changed two things nobody flagged but that the review rubric would:

- **The `RepairOutcome::Conflict` doc.** v1 reworded the line "The commit lost the CAS race
  (rebuilt fragments are now collectable garbage)". The brief says #723 owns the
  "collectable garbage" comments and they must not be reworded. That line is now restored
  word for word, and a new sentence is added after it for the prepare-time conflict
  (`:1032-1034`).
- **A new silent-conflict path for corrupt flat maps.** v1 summed byte offsets with
  `saturating_add`. A flat map's chunk lengths are **not** checked at decode (only segmented
  maps are, `metadata.rs:1697-1707`). So a corrupt flat map whose lengths overflow `u64` before
  an owed chunk gave that chunk an offset no move can find. The result was `Conflict` every pass,
  with the obligation stuck behind a pass that answers `Satisfied`. The base repaired such a
  chunk by index. The offset is now `Option<u64>` (`:669-672`). An owed chunk past the overflow
  makes the walk contain the object (`:685-690`), so it is named and the pass answers
  `Blocked`. Leg 10 pins this; on the base it is red because the base still repaired by index
  (`Changed`).

And one doc fix: `emit_aborted` now lists the move-time containment among its causes
(`:1408-1411`). ADR-0011 names this function's code as the source of truth for the counter
(`docs/design/adr/0011-durability-telemetry-and-declarative-management.md:42`), and the brief puts ADRs out of scope, so the ADR is unchanged.

## The design (unchanged from v1 except where noted above)

- The base sent a `seg:`-resident obligation to `Site::Refused` (base: `:701`) and answered
  `Assessment::Refused` (base: `:783`). Both are gone, along with `emit_refused` (base:
  `:1375`) and `Reading::refused` (base: `:534`).
- `read_committed` records a `Site { object, byte_offset, chunk_ref }` (`:571`) for every owed
  chunk, flat or segmented. The held generation is `resolved.record` (`:694`), the root the
  chunks were actually resolved from, so the root pin matches the plan even when a segmented
  resolve restarted onto a newer root.
- `repair_chunk` follows the peer's order: rebuild → choose targets → abort if a target is
  outside the fleet → **prepare the move** with `repoint_chunk` (`:1156`; reads only) → write
  fragments (`:1192`) → add the `repair_key` delete and one orphan mark per displaced position
  to the batch the primitive handed back (`:1195-1201`) → one `commit`. The base's inline record
  build and ceiling check (base: `:1143-1178`) are gone. The ceiling refusal now comes from
  inside the primitive, as the brief asked.
- How each `Repoint` answer maps: `Prepared` → commit. `Refused` → `RepairOutcome::Refused`, a
  hole, so `Blocked`. `Conflict` → `RepairOutcome::Conflict`, not a hole, so `Satisfied` when it
  is the only outcome (the human's 2026-08-19 decision). `VersionExhausted` and a typed
  `ChunkMapError` from the move's re-read → `RepairOutcome::Contained`. Any other `Err` (a store
  fault) still ends the pass.
- `hole` (`:490`) only loses the `!reading.refused.is_empty()` term. Conflicts were not added
  to it.
- Flat objects go through the same primitive. It writes the same record the base did
  (`..prior.clone()`, `version + 1`, the same `require(inode_key, encode(prior))` pin). There
  are three differences, all in corrupt or exhausted cases. (1) `version == u64::MAX`: the base
  panicked on overflow in debug builds and wrapped to 0 in release. Now the object is contained
  (leg 9). (2) Lengths that overflow `u64`: see above (leg 10). (3) `state` is kept as it was
  rather than forced to `Committed`. Only `Committed` records reach this point
  (`:629`, and `resolve_current_chunk_map` checks it too, `metadata.rs:3085`), so this changes
  nothing.

### Regression guards from the brief

- **(a) Once per OBJECT.** `Reading::contain` (`:538`) de-duplicates by key through
  `contained: BTreeSet<Vec<u8>>` (`:532`). Leg 7 pins this on the move path (two obligations,
  one torn record, one row). The ceiling refusal is still once per chunk. That is the flat
  arm's existing behaviour and this slice does not change it.
- **(b) No new silent skip.** The base's `(Some(_), None) => continue` is kept byte-identical
  (`:656`; #698). A **segmented** record under an unparsable key used to be refused (kept,
  `Blocked`). A plain skip would let its owed chunks look unreferenced and be drained, so it is
  contained instead (`:679-683`). That sets `reading.incomplete` and withholds every drain. Leg
  8 pins it with a `DELETED` obligation beside it: both stay queued. `parse_inode_key` is
  untouched.

## Files and budget

| file | + | − | semantic + |
|---|---|---|---|
| `crates/custodian/src/reconstruction.rs` | 199 | 211 | **81** |
| `crates/custodian/src/reconstruction/staged.rs` | 0 | 1 | 0 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` (forced edit) | 50 | 51 | — |
| `crates/custodian/tests/segmented_map_repoint.rs` (new) | 813 | 0 | — |

Added semantic production lines (non-blank, non-comment): **81** (budget ≤ 100). Production is
net −12 lines. `patch.diff` is **81,189 bytes** (budget ≤ 85 KB).

**Fourth file, still flagged (C3 deferred finding).** `RepairPlan.chunk_index` was read only by
the base's inline record build (base: `:1152`), which this patch deletes. If the field stays,
it is written by `staged.rs:369` and read by nothing. That is a `dead_code` error under
`clippy -D warnings`. The alternatives, with their real cost:

- Keep the field and add `#[allow(dead_code)]`: +1 line, and a field nothing reads.
- Keep addressing by index: the primitive takes `byte_offset` + `prior`
  (`metadata.rs:3241-3248`), not an index, so the index would have to be turned into an offset
  inside `repair_chunk`. That means holding each object's resolved chunk list (≈ +6 lines). For
  a flat object it also holds the chunk list twice: once in `prior.chunk_map`, once in the copy.
- Chosen: delete the field and its one writer, `chunk_index: site.index,` (base:
  `staged.rs:369`). That is −1 line with no behaviour change. `staged` still reads `site.index`
  at `:285` and `:355`.

## Test: `crates/custodian/tests/segmented_map_repoint.rs` (new, no `#![cfg]`)

Drives only `reconcile_step` and asserts on the store, plus the audit seam for the
once-per-object counts. It names only base symbols (`MAX_VALUE_BYTES`, not
`MAX_ROOT_VALUE_BYTES`; `orphan_key`) and never calls `repoint_chunk`. The race hooks are as
the brief requires: `AfterSegmentPage` fires inside `scan_page` for a `seg:` prefix, after the
page is built and before it returns. `IntoCommit` fires at the top of `commit`. Every racing
leg asserts `raced()`.

**10 tests, not the brief's 5.** The carry-forward asked for legs 6 and 7. Legs 8–10 pin the
three other new caller paths (guard (b), version exhaustion, length overflow). Without them,
each of those lines is unexecuted or unasserted. So C4-verify will report **10 ran, 9 failed**
on the red leg, not "5 ran, 4 failed".

1. seg-resident chunk repaired: `Changed`, `[[0,2],[0,1]]`, fragment on server 2 (distinct
   domain), queue empty, orphan keys == `[orphan_key(LOST, (CHUNK,1))]`, root and segment 0
   byte-identical.
2. sibling race merged: `Changed`, `[[0,2],[0,7]]`, queue empty.
3. planned chunk raced: segment 1 == racer's bytes, root unchanged, still queued, no orphan,
   `Satisfied`. No assertion about the destination fragment (#723).
4. root superseded into `commit`: segment 1 unchanged, still queued, no orphan, root == the
   competing bytes, `Satisfied`.
5. ceiling: segment 1 seeded so the repoint encodes to exactly `MAX_VALUE_BYTES + 1` (growth
   measured), seeded `< MAX_VALUE_BYTES`. Byte-identical, queued, `Blocked`.
6. torn record under the move, one obligation: contained once (see table above).
7. same, two obligations in one record: still ONE row, ONE tick.
8. segmented root at `inode:x`: `Blocked`, `[DELETED, CHUNK]` both still queued, segment 1
   unchanged, one row naming `inode:x`.
9. flat record at `version = u64::MAX`: `Blocked`, root byte-identical, queued, no fragment
   written, one row.
10. flat map whose lengths overflow before the owed chunk: same assertions as 9.

`segmented_map_reconstruction.rs` (the forced edit, unchanged from v1):
`an_obligation_inside_a_segmented_object_is_refused_never_discarded` →
`…_is_repaired_never_discarded`. Both chunks, one per segment, are repaired in one pass. Each
`seg:` record names `[0,2]`, the queue is empty, the root is byte-identical and the backlog gauge
reads 2. The survivor loop moved into `survivors()`, and the now-unused `MemMeta::records()` was
removed.

## Red → green

I did not run `engine/scripts/run-verify.sh` itself. It does `git worktree add` against the host
repo (`../wyrd-verify-l2`), which is outside the roots I may write to. Instead I ran its red
leg by hand in the cycle worktree, with `timeout`-bounded `cargo test` (the same invocation the
gate makes). I copied the two production files aside to
`$PDCA_SCRATCH/pdca-builder-777-redleg`, ran `git checkout HEAD --` on them, ran the tests,
copied the files back, and checked with `cmp` that `git diff` was byte-identical to before. It
was.

- **Green (fix applied):** `segmented_map_repoint` 10 passed, 0 failed.
  `segmented_map_reconstruction` 6 passed. Whole `-p wyrd-custodian` suite: every binary
  `ok`.
- **Red (production reverted, tests kept):** `segmented_map_repoint`: **10 ran, 9 failed**:
  - leg 1: `left: Blocked, right: Changed`
  - leg 2: `left: Blocked, right: Changed`
  - leg 3: `left: Blocked, right: Satisfied`
  - leg 4: at `assert!(meta.raced())` (the base never commits)
  - legs 6, 7: `left: (0, 0, true), right: (1, 1, true)` (the base names a refusal row, not an
    unresolvable one)
  - leg 8: `left: [41472], right: [3584, 41472]` (the base drained `DELETED`)
  - leg 9: panicked at base `reconstruction.rs:1157:18` (`version + 1` overflow)
  - leg 10: `left: Changed, right: Blocked`
  - leg 5: ok

  `segmented_map_reconstruction`: the rewritten leg 2 fails (`left: Blocked, right: Changed`);
  the other 5 pass.

Legs 3 and 4 going red only shows "refused" versus "attempted". It is **not** evidence that the
pins are right. That evidence is the mutation table below.

## Named negations (mutation oracle), each demonstrated

Each mutation was applied to the source file, `cargo test -p wyrd-custodian --test
segmented_map_repoint` was run, and the file was restored (`cmp` confirmed the tree unchanged
afterwards). M1–M3 mutate `crates/core/src/metadata.rs`, which is not part of the patch.

| # | mutation | legs that went red (others green) |
|---|---|---|
| M1 | `chunk_at` (`metadata.rs:3374`): `chunk == prior` → `chunk.id == prior.id` | leg 3 only |
| M2 | segmented arm (`metadata.rs:3325`): `root_pin.require(..)` → `WriteBatch::new().require(..)` (drop the root pin) | leg 4 only |
| M3 | segmented arm (`metadata.rs:3324-3328`): `weighed(..)` → `Repoint::Prepared(.. .put(key, encode(&next)))` (drop the ceiling weigh) | leg 5 only |
| M5 | `repair_chunk` (`:1184`): typed move error → `RepairOutcome::Conflict` | legs 6, 7 |
| M6 | `Reading::contain` (`:539`): name on every call, not once per key | leg 7 only |
| M7 | `read_committed` (`:682`): drop the `reading.contain` for an unparsable segmented key (silent skip) | leg 8 only |
| M8 | `repair_chunk` (`:1175`): `VersionExhausted` → `RepairOutcome::Conflict` | leg 9 only |
| M9 | `reconcile` (`:449`): drop `reading.contain` in the `Contained` arm (the reviewer's hand mutation) | legs 6, 7, 9 |
| M10 | `read_committed` (`:685`): overflowed offset saturated to `u64::MAX` instead of contained | leg 10 only |

Leg 2's negation (pin the bytes the resolve saw, not the move's fresh read) has no one-line form
inside the primitive. Leg 2 is binding-red on the base anyway. On the caller side, widening
`hole` to include conflicts would turn legs 3 and 4 red on their `Satisfied` line.

What leg 5 does **not** pin: that the bound is `MAX_VALUE_BYTES` and not `MAX_ROOT_VALUE_BYTES`.
A V/2 bound would also refuse this record. An "admitted at exactly `MAX_VALUE_BYTES`" leg would
be red on the base and break "leg 5 green by construction". Core pins both sides of the bound
(`the_value_ceiling_admits_the_boundary_and_refuses_only_past_it`, `metadata.rs` tests).

## Refute-your-own-test

- **(a) Genuine red? Yes.** With `reconstruction.rs` and `staged.rs` reverted to `HEAD` and the
  tests kept, 10 ran and 9 failed, each for the reason listed above. The rewritten
  reconstruction leg also goes red.
- **(b) Production path? Yes.** Every leg calls the public `reconcile_step` with a real
  `ReconstructionContext`. The pass runs the production `read_committed` → `assess` →
  `repair_chunk` → `metadata::repoint_chunk` → `MetadataStore::commit`. Fragments are real
  `erasure::encode` + `encode_ec_fragment` bytes, so the production verify passes. The only
  doubles are the in-memory `MetadataStore` / `ChunkStore` the brief names. The race hooks sit on
  the seam methods (`scan_page`, `commit`) and do not change their answers.
- **(c) Fixture includes the fault? Yes.** The lost server 1 is in the committed placement and in
  neither the fleet nor the topology, so the fragment really is missing. Racing writes are real
  encoded records, or real torn bytes, that actually land (`raced()` is asserted). The ceiling
  record really crosses the bound after the repoint. The version and overflow fixtures are real
  flat records the decoder accepts.

## Prior art (T4 Contribution finding)

Checked in the worktree by path (`git log -- <file>`) and with `gh pr list -R getwyrd/wyrd --state all`:

- `reconstruction.rs`: `1f871ce` (#697/PR #706, the refusal this completes), `d2609b2` (#710,
  flat ceiling), `9470de5` (#638, write deadline), `f683dbe`, `5377850`, `19cec1a` (staged
  repair slices). None touches the segmented write path.
- `reconstruction/staged.rs`: `5377850`, `19cec1a` only. This patch's one-line deletion there
  does not interact with either.
- `tests/segmented_map_reconstruction.rs`: `1f871ce`, `9470de5`, `f683dbe`.
- `tests/segmented_map_repoint.rs`: new file, no history.
- A `gh` search for "reconstruction" in titles finds only PR #706 (merged), #335 and #338
  (merged, unrelated). There are no closed or rejected PRs on these paths.

## Things for the human

- **DST coverage (deferred finding).** Reconstruction now writes `seg:` records. There is still
  no seeded Tier-0 DST property for it. I checked #722: its DST property is the **drain**
  (rebalance) repoint-versus-supersede, in `crates/dst/tests/custodian.rs`. It does not cover
  reconstruction. So a `// deferred: #722` marker would point at the wrong issue, and I added
  none. The brief puts the DST file off-limits. This needs a decision: open a new issue and add
  a marker, or widen the scope.
- **Stale deferral in core (deferred finding).** `crates/core/src/metadata.rs:3238-3240` says
  "nothing calls this yet. It moves with the custodian wiring in #777." After this patch,
  something does call it. The brief forbids touching `metadata.rs` and the docs. I searched
  `06-runtime-view.md` and `08-crosscutting-concepts.md`: no sentence there says
  reconstruction refuses segmented objects, so nothing in them is now false.
- **#698 interaction (not fixed, by instruction).** A segmented root under a noncanonical key
  such as `inode:01` parses to inode 1, so its move pins `inode:1`. The move then loses every
  pass and answers `Conflict`, which a conflict-only pass reports as `Satisfied`. On the base it
  was refused and reported `Blocked`. The flat arm has always behaved this way, and the brief
  gives noncanonical keys to #698. It is worth noting on #698 that it now covers segmented
  objects too. A key that does not parse at all (`inode:x`) is contained (leg 8).
- **`emit_conflict`'s message** still says "rebuilt fragments are collectable garbage". For a
  prepare-time conflict nothing was written. I left it because #723 owns that wording; the
  `RepairOutcome::Conflict` doc now says which case writes nothing.

## Gates run locally

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean.
- `typos` on the four changed files: clean. The first full gate run caught "unparseable" in a
  test name; it is now "unparsable".
- `cargo test --no-fail-fast -p wyrd-custodian`: every binary `ok`.
- `engine/xtask.sh ci` (`cargo xtask ci`, the project's gate, with `PDCA_WORKTREE` set to the
  cycle worktree): **`xtask ci: all checks passed`, rc=0**. That covers typos, `lint_docs`,
  `render_site --check`, the gitlink/unsafe guards, fmt, clippy, build, the workspace tests,
  cargo-machete, cargo-deny, the statics and deploy guards, and the `--cfg madsim` DST build
  and tests. One doc comment in the new test file (the `CHUNK` constant's) was reworded while
  that run was in flight. Afterwards I re-ran fmt `--check`, `typos`, clippy `-D warnings` and
  both test files on the final tree: all clean.

## Scratch left for the harness

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_777`): `pdca-builder-777-prev`
(empty) and `pdca-builder-777-redleg`. The second holds the reverted-file copies, the mutation
driver `mutate.py`, `lcov.info`, `lines.txt`, and `xtask-ci.log` (the full gate log). The
`cargo llvm-cov` build cache is `target/llvm-cov-target/` inside the cycle worktree. I deleted
nothing; the harness reclaims both roots.
