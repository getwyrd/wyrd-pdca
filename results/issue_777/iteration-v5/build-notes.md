# Build notes — issue 777, iteration 5 (rebuild after the iteration-4 sign-off)

All `path:line` cites are against the cycle worktree (`$PDCA_WORKTREE`, base `4bda59c`,
= `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`) with `patch.diff` applied.

## What this iteration is

The iteration-4 sign-off said: **keep the fix as built**, and make two changes. So this
patch is `iteration-v4/patch.diff` (applied unchanged, it still applies cleanly to the base)
plus exactly those two changes. Nothing else in the reviewed code or tests was touched.

### Change 1 — the non-canonical key guard

**Defect (reproduced before the fix).** A segmented root stored at `inode:01` parses to id 1
(`parse_inode_key`, `crates/custodian/src/reconstruction.rs:1222`). The move then pins the
root at the key it re-derives from the id, `inode:1` (`crates/core/src/metadata.rs:3256-3257`),
which is not the row the scan read. So every pass rebuilt and wrote the fragment, lost the
commit, and answered `Satisfied` with the obligation never drained. I ran the new test leg
before adding the guard and got exactly that: `left: Satisfied, right: Blocked`, with a
`reconstruction_conflict` tick in the log.

**Fix.** One match arm in `read_committed`, `reconstruction.rs:659`:

```rust
(None, Some(inode_id)) if key != metadata::inode_key(inode_id) => None,
```

A segmented record whose scanned key is not the canonical spelling of its id gets no
`inode_id`, so it falls into the containment that already existed for an unparsable key
(`reconstruction.rs:687-692`): on the first chunk it is owed, the object is named once, the
reading is marked incomplete, and the walk moves on. Result: `Blocked`, nothing written (no
fragment, no metadata), obligation still queued.

The fault text at `reconstruction.rs:690` changed from "is not an `inode:<id>` key" to "is
not the canonical `inode:<id>` key", so the one message is true for both `inode:x` and
`inode:01`. No test asserted on the old text (grep over `crates/` and `docs/`).

**Kept narrow, as told.** `parse_inode_key` is untouched (#698 owns it). The base's
`(Some(_), None) => continue` (`reconstruction.rs:664`) is untouched. A **flat** record under
a non-canonical key behaves exactly as on the base — also #698's. The doc comment on
`Object::inode_id` (`reconstruction.rs:550-556`) now says which half is still #698's.

**Guard keys on the scanned generation's shape**, the same thing the walk's skip decision
already keys on. A scanned-flat record never restarts, so the guard cannot change any flat
object's behaviour.

**Test leg 12**, `crates/custodian/tests/segmented_map_repoint.rs:795`
(`a_segmented_object_under_a_noncanonical_key_is_contained_not_retried_forever`). It shares
a helper with leg 8 (`unaddressable`, `:752`), which I tightened while there: instead of
comparing only segment 1's bytes, it compares **every row in the metadata store** before and
after (`meta.scan(b"")`), and asserts the free D server holds no fragment. That is what
"nothing written" means. Leg 8 (`inode:x`) still passes with the stronger assertions.

### Change 2 — the docs deferral that named #777

- `docs/design/architecture/06-runtime-view.md:40` (§6.3, step 2 "Reconstruct"): says the
  mutation rewrites whichever record holds the chunk's reference (flat inode record or one
  `seg:` record, root pinned and not rewritten), what it is compare-and-swapped on, that a
  sibling's concurrent move is merged, that the same mutation drains the obligation and
  marks orphans, and what happens to an object the move cannot rewrite.
- `docs/design/architecture/08-crosscutting-concepts.md:89` (§8.7): a new paragraph on the
  write side of the segmented shape, naming `repoint_chunk` and the reconstruction pass as
  its caller.
- `crates/core/src/metadata.rs:3238-3239`: the `deferred: #777 … nothing calls this yet`
  comment is replaced by two lines saying the reconstruction pass is the caller and where
  the docs are. Comment only; no code in `metadata.rs` changed.

I scoped the doc text to reconstruction. Rebalance still refuses a segmented object
(`crates/custodian/src/rebalance.rs:398`, #722's), and the docs do not claim otherwise.

**Placement of the §8.7 paragraph.** It sits after the orphan-mark paragraph rather than
directly after the chunk-map paragraph. Reason: those paragraphs are single lines of 4.5 KB
and 1.75 KB, and a unified diff carries 3 lines of context. After the orphan paragraph the
hunk costs 3,412 bytes; between the two it would cost about 8 KB. The paragraph stands on
its own either way. If the human prefers the other order, it is a one-line move.

## Budget — over on bytes, and why

| file | v4 bytes | now | delta |
|---|---|---|---|
| `crates/core/src/metadata.rs` | 0 | 964 | +964 |
| `crates/custodian/src/reconstruction.rs` | 42,118 | 43,024 | +906 |
| `crates/custodian/src/reconstruction/staged.rs` | 432 | 432 | 0 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` | 7,790 | 7,790 | 0 |
| `crates/custodian/tests/segmented_map_repoint.rs` | 34,650 | 35,634 | +984 |
| `docs/design/architecture/06-runtime-view.md` | 0 | 2,206 | +2,206 |
| `docs/design/architecture/08-crosscutting-concepts.md` | 0 | 3,412 | +3,412 |
| **total** | **84,990** | **93,462** | **+8,472** |

`patch.diff` is **93,462 bytes: over the brief's 85 KB, under the 95 KB cap the brief
mentions.** v4 was already at 84,990, so any addition crossed the line. The sign-off lifted
the 3-file budget and the `metadata.rs` fence for the doc and comment edits; it did not
mention the byte budget. 6,582 of the 8,472 new bytes are those doc and comment edits. The
guard and its test leg are 1,890.

To get back under 85 KB I would have to cut about 8.5 KB out of code comments and tests
that passed review in iteration 4. I did not: the sign-off said to keep the fix as built,
and re-wording reviewed material invites new findings. **The human should decide at
sign-off whether the byte budget is lifted along with the file budget.** Files: 7 (the
brief's 3, `staged.rs` accepted at the iteration-4 sign-off, and the 3 the sign-off asked
for).

Added semantic non-test lines this iteration: 1 (the match arm).

## Verification

**Runner used.** The project has no single-test wrapper; its runners are
`./engine/xtask.sh ci` (the whole gate) and `./engine/scripts/run-verify.sh` (red→green for
one bundle). I ran both. For the quick loops (the pre-guard red run and the mutation runs
below) I ran `cargo test -p wyrd-custodian --test segmented_map_repoint` under a hard
`timeout`, so nothing could hang.

- `run-verify.sh` (with `PDCA_VERIFY_BASE` set to the stack base above):
  `PASS — red without the fix, green with it (12 test(s) ran red).` On the base 12 tests
  ran, 11 failed, 1 passed (leg 5, which passes on the base by construction, as the brief
  says). With the fix: 12 passed.
- `./engine/xtask.sh ci`: `xtask ci: all checks passed` (exit 0), run on the worktree with the patch applied.
- `typos` over the five edited source and doc files: clean.
- `cargo fmt --all -- --check`: clean. `git diff --check`: clean.
- The patch reverse-applies cleanly to the worktree (`git apply --check -R`).

The brief's "expect 5 tests ran, 4 failing" was written for the five original legs. The
file now has 12 tests (legs 6–11 came from carry-forwards 1–3, leg 12 from carry-forward
4), so the count is 12 ran, 11 red.

## Refuting my own test

**(a) Genuine red?** Yes.
- Whole fix reverted: `run-verify.sh` red leg, 11 of 12 fail (above).
- New leg alone: run before the guard existed, it failed with `Satisfied` vs `Blocked`.
- Each pin, by its named negation. I broke one line at a time, ran the test file, and
  restored the file (the worktree diff stat was identical before and after):

| negation | file | legs that went red |
|---|---|---|
| drop the `ChunkRef` equality pin (`chunk == prior` → ids only), `metadata.rs:3373` | core | leg 3 only |
| drop the root-generation pin (`root_pin` without `require`), `metadata.rs:3257` | core | leg 4 only |
| drop the weigh in the segmented arm (return `Prepared` directly), `metadata.rs:3323-3327` | core | leg 5 only |
| answer the move's typed error as a conflict, `reconstruction.rs:1195` | custodian | legs 6, 7 |
| drop the abort offset on a contained move, `reconstruction.rs:450` | custodian | legs 6, 7 |
| lose the once-per-object dedupe in `Reading::contain`, `reconstruction.rs:539` | custodian | leg 7 only |
| contain a store fault under the move instead of ending the pass, `reconstruction.rs:1196` | custodian | leg 11 only |
| drop the canonical-key guard, `reconstruction.rs:659` | custodian | leg 12 only |

  Legs 1–2 are red on the base and need no negation. As the brief says, the red of legs
  3–4 on the base only shows "refused" versus "attempted"; the table above is what binds
  their pins.

**(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` with a real
`ReconstructionContext` and asserts on the store. No test calls `repoint_chunk` directly
and none names a symbol this patch adds, which is why the file compiles on the base (the
red leg ran 12 tests rather than failing to build).

**(c) Fixture includes the fault?** Yes.
- Leg 12's root really is stored at `inode:01`, and nothing is stored at `inode:1`. The
  fixture asserts up front that the seeded object resolves (`seed`, `:323-327`), so the
  object is a live, readable segmented object, not a broken one that would be contained
  for some other reason.
- The race legs assert `meta.raced()`, so a race that never fired fails the leg.
- Leg 11 asserts the injected fault actually fired.
- The lost fragment is on a server in no fleet and no topology; the survivor fragment is
  real erasure-coded bytes, so the checksum verify runs for real.

## What I did not do, and what is still open

- **No seeded Tier-0 DST coverage for the move.** Deferred to #682 with an in-code marker
  (`reconstruction.rs:1165`); accepted at the iteration-4
  sign-off. The interleavings are scripted in the test file instead.
- **A flat record under `inode:01`** still loses its commit every pass, as on the base.
  That is #698's, and the sign-off said not to fix it here.
- **Leg 3 leaves a stranded fragment** on the destination server. That is tracked leak
  #723, inherited as-is per the brief; the leg does not assert on it.
- I did not re-read the earlier iterations' reviews or notes. I read `brief.md`, and
  `iteration-v4/patch.diff` because the brief names it as the salvage and the sign-off says
  to keep it.

## Self-review against the target's rubric

- *Docs currency*: the two living architecture sections are updated in this patch.
- *Absent or unsupported entries*: the non-canonical key is an explicit containment
  (named, counted, `Blocked`), not a silent skip or a silent retry.
- *Await discipline*: no new await. The move's one read is bounded by the `MetadataStore`
  implementation, as the comment at the call says.
- *One clock*: no new clock read.
- *Serialization identity*: the segmented root is never re-encoded by the move; leg 1
  asserts it is byte-identical.
- *Test fidelity*: DST coverage deferred to #682 as above.
- Assertions are on identities and bytes, not counts, except where the property itself is
  a count of operator signals (once per object).
