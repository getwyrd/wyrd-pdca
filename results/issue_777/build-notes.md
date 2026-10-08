# Build notes — issue 777, iteration 6 (rebuild after the iteration-5 sign-off)

All `path:line` cites are against the cycle worktree (`$PDCA_WORKTREE`, base `4bda59c`,
= `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`) with `patch.diff` applied.

## What this iteration is

The iteration-5 sign-off said: **keep the fix as built**, and make only three changes. So
this patch is `iteration-v5/patch.diff` (applied unchanged; it still applies cleanly to the
base) plus exactly those three changes.

**No production code changed this round.** Per file, the patch bytes for
`crates/core/src/metadata.rs`, `crates/custodian/src/reconstruction.rs`,
`crates/custodian/src/reconstruction/staged.rs`,
`crates/custodian/tests/segmented_map_reconstruction.rs` and
`docs/design/architecture/08-crosscutting-concepts.md` are identical to iteration 5 (table
under **Budget**). Only the test file and one doc sentence moved.

### Change 1 — leg 13: the move pins the root the resolver answered from

`crates/custodian/tests/segmented_map_repoint.rs:929`
(`a_repair_planned_over_a_restarted_resolve_pins_the_root_it_answered_from`).

What it does, as the sign-off specified:

- Arms `Race::AfterSegmentPage` with a whole superseding generation: the same `records()`
  under `SegmentGroup::new(NONCE, EPOCH + 1)`, plus a version-2 root at `inode:1` naming
  that group. It lands after the resolver's page of the old group.
- The resolver sees the root moved, restarts onto the live root, and answers from it. The
  production line under test is `prior: resolved.record.as_ref().clone()`
  (`crates/custodian/src/reconstruction.rs:702`).
- Asserts: the race landed; the pass answers `Changed`; the **new** group's `seg:…:1`
  holds `[[SURVIVOR, FREE], [SURVIVOR, LOST]]`; both records of the **old** group are
  byte-identical; the root is the superseding writer's version-2 root, byte for byte (the
  move never rewrites a segmented root); the queue is empty.

To build that race I split the fixture's seeding in two (same behaviour for every existing
leg):

- `generation(group, version, key, records)` (`:312`) returns one committed segmented
  generation as raw rows — the `seg:` records, then the root last. It is the old body of
  `seed` with the group and version made parameters.
- `seed` (`:338`) now writes `generation(&group(), 1, …)` and keeps its "the seeded object
  must resolve" check.
- `placements_in(bytes)` (`:379`) decodes placements from a record's bytes, so the new leg
  can read a record that lives under the other group. `placements` calls it.

### Change 2 — legs 3 and 4 count the loss as a conflict, not an abort

`assert_lost` (`segmented_map_repoint.rs:469`) now takes the pass's log as well and asserts
(`:491-495`) **exactly one `reconstruction_conflict` tick and zero `reconstruction_aborted`
ticks**. A small shared helper `ticks(logged, name)` (`:234`) counts them;
`torn_under_the_move` uses the same helper instead of its own closure (`:757-758`).

Which leg guards which production line — they are two different arms:

- **Leg 3** (planned chunk rewritten under the plan) loses inside the primitive, so it
  guards `Ok(metadata::Repoint::Conflict) => return Ok(RepairOutcome::Conflict)`
  (`reconstruction.rs:1185`). This is the mutant the sign-off named.
- **Leg 4** (root superseded on the way into the commit) loses at the commit, so it guards
  `CommitOutcome::Conflict => Ok(RepairOutcome::Conflict)` (`reconstruction.rs:1218`).
  Relabelling `:1185` does **not** turn leg 4 red, and that is correct — its conflict does
  not come from there. I ran the `:1218` twin too, to show leg 4's new assertion is not
  decoration.

I put the tick assertion **after** the `Satisfied` assertion. Reason: the file's module doc
says leg 3 fails on the base "on its verdict". With the ticks first, leg 3 would fail on the
base on the tick count instead and that sentence would be false. Under both relabel mutants
the verdict is still `Satisfied` (an abort is not a hole either), so the tick assertion is
the only line that catches them — see the panics at `:491` below.

### Change 3 — `06-runtime-view.md` §6.3 wording

`docs/design/architecture/06-runtime-view.md:40`. The sentence now reads: "It is a
compare-and-swap on the root generation, and, for a segmented map, on the segment record as
the move itself re-reads it, and on the chunk's own reference — so …". That matches the
primitive: only the segmented arm re-reads a record (`crates/core/src/metadata.rs:3288`);
the flat arm pins the caller's own snapshot (`metadata.rs:3257-3275`).

## Both new mutants are caught (and the companion)

Each run: break one line, run `cargo test -p wyrd-custodian --test segmented_map_repoint`
under `timeout 900`, restore the file. 13 tests ran each time.

**Mutant A — wrong `prior`** (`reconstruction.rs:702`):

```
-                    prior: resolved.record.as_ref().clone(),
+                    prior: record.clone(),
test a_repair_planned_over_a_restarted_resolve_pins_the_root_it_answered_from ... FAILED
assertion `left == right` failed: the repair lands on the generation the resolve answered from: …
  left: Satisfied
 right: Changed
test result: FAILED. 12 passed; 1 failed
```

Only the new leg goes red; the other 12 pass. Before this round, nothing caught it.

**Mutant B — `Repoint::Conflict` relabelled `Aborted`** (`reconstruction.rs:1185`):

```
-        Ok(metadata::Repoint::Conflict) => return Ok(RepairOutcome::Conflict),
+        Ok(metadata::Repoint::Conflict) => return Ok(RepairOutcome::Aborted),
test a_racing_move_of_the_planned_chunk_itself_is_a_conflict ... FAILED
panicked at crates/custodian/tests/segmented_map_repoint.rs:491:5:
  left: (0, 1)
 right: (1, 0)
test result: FAILED. 12 passed; 1 failed
```

**Companion — commit-time conflict relabelled `Aborted`** (`reconstruction.rs:1218`):

```
-        CommitOutcome::Conflict => Ok(RepairOutcome::Conflict),
+        CommitOutcome::Conflict => Ok(RepairOutcome::Aborted),
test a_superseded_root_generation_makes_the_repair_lose ... FAILED
panicked at crates/custodian/tests/segmented_map_repoint.rs:491:5:
  left: (0, 1)
 right: (1, 0)
test result: FAILED. 12 passed; 1 failed
```

## One thing I did not change, for the human to decide

`docs/design/architecture/08-crosscutting-concepts.md:89` (§8.7, added in iteration 5) has
the same loose wording change 3 fixes in §6.3: "…as a compare-and-swap on the root
generation, on that record's bytes as the move itself re-reads them, and on the chunk's
reference." For a flat map the move does not re-read anything. The sign-off said "make only
these three changes" and named §6.3 alone, so I left §8.7 as it was. If you want it fixed
too, it is the same one-clause edit: "…on the root generation, and, for a segmented map, on
the segment record's bytes as the move itself re-reads them, and on the chunk's reference."

## Budget

| file | v5 bytes | now | delta |
|---|---|---|---|
| `crates/core/src/metadata.rs` | 964 | 964 | 0 |
| `crates/custodian/src/reconstruction.rs` | 43,024 | 43,024 | 0 |
| `crates/custodian/src/reconstruction/staged.rs` | 432 | 432 | 0 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` | 7,790 | 7,790 | 0 |
| `crates/custodian/tests/segmented_map_repoint.rs` | 35,634 | 39,069 | +3,435 |
| `docs/design/architecture/06-runtime-view.md` | 2,206 | 2,239 | +33 |
| `docs/design/architecture/08-crosscutting-concepts.md` | 3,412 | 3,412 | 0 |
| **total** | **93,462** | **96,930** | **+3,468** |

`patch.diff` is **96,930 bytes**, 7 files. The sign-off approved 93.5 KB and lifted the
85 KB budget for this round's additions; the additions cost 3,468 bytes. It is under the
harness's 100 KB size signal (`pdca.toml`, `[driver.size_signal]`, default `patch_kb = 100`).
It is over the "95 KB cap" the brief's Scope mentions in passing (`brief.md:130`), by about
1.9 KB. I did not trim reviewed material to get under that figure, because the sign-off
said to keep the fix as built.

Added semantic non-test lines this iteration: 0.

## Verification

**Runners used.** The project's runners are `./engine/xtask.sh ci` (the whole gate) and
`./engine/scripts/run-verify.sh` (red→green for one bundle). I ran both. For the quick
loops (the mutation runs and a first red/green pass) I ran
`cargo test -p wyrd-custodian --test segmented_map_repoint` under `timeout 900`, so nothing
could hang.

- `./engine/xtask.sh ci` on the worktree with the final patch applied:
  `xtask ci: all checks passed` (exit 0).
- `run-verify.sh` (`PDCA_BUNDLE=results/issue_777`, `PDCA_VERIFY_BASE` = the stack base
  above, `PDCA_LANE=0`, so it used the harness's own lane-0 verify worktree
  `../wyrd-verify-l0`): `run-verify.sh: PASS — red without the fix, green with it
  (13 test(s) ran red).` On the base 13 ran, 12 failed, 1 passed.
- By hand in the worktree, same test command: production change reverted
  (`crates/custodian/src`, `crates/core/src`) → 13 ran, **12 failed, 1 passed** (leg 5,
  which passes on the base by construction, as the brief says). Fix re-applied → 13 passed.
- `cargo fmt --all -- --check`: clean. `git diff --check`: clean. `typos` over the edited
  test and doc file: clean.
- The patch reverse-applies cleanly to the worktree (`git apply --check -R`).

The brief's "expect 5 tests ran, 4 failing" was written for the five original legs. The
file now has 13 tests (legs 6–11 from carry-forwards 1–3, leg 12 from carry-forward 4,
leg 13 from carry-forward 5), so the count is 13 ran, 12 red.

## Refuting my own test

**(a) Genuine red?** Yes.
- Whole fix reverted: 12 of 13 fail (above). The file compiles on the base — 13 tests
  ran, none failed to build.
- Each pin, by its named negation. Because this round changed the shared fixture (`seed`),
  I **re-ran every earlier negation** against the new test file, not only the new ones. One
  line broken at a time, test file run, file restored; the worktree diff was byte-identical
  before and after the sweep.

| negation | where | legs that went red |
|---|---|---|
| drop the `ChunkRef` equality pin (`chunk == prior` → ids only) | `metadata.rs:3373` | leg 3 only |
| drop the root-generation pin (`root_pin` without `require`) | `metadata.rs:3257` | leg 4 only |
| drop the weigh in the segmented arm (return `Prepared` directly) | `metadata.rs:3323-3327` | leg 5 only |
| answer the move's typed error as a conflict | `reconstruction.rs:1195` | legs 6, 7 |
| drop the abort offset on a contained move | `reconstruction.rs:450` | legs 6, 7 |
| lose the once-per-object dedupe in `Reading::contain` | `reconstruction.rs:539` | leg 7 only |
| contain a store fault under the move instead of ending the pass | `reconstruction.rs:1196` | leg 11 only |
| drop the canonical-key guard | `reconstruction.rs:659` | leg 12 only |
| **new:** pin the scanned root, not the resolved one | `reconstruction.rs:702` | leg 13 only |
| **new:** relabel the move's conflict as an abort | `reconstruction.rs:1185` | leg 3 only |
| **new:** relabel the commit-time conflict as an abort | `reconstruction.rs:1218` | leg 4 only |

  Legs 1–2 are red on the base and need no negation. As the brief says, the red of legs
  3–4 on the base only shows "refused" versus "attempted"; the table is what binds their
  pins.

**(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` with a real
`ReconstructionContext` and asserts on the store (and, for operator signals, on the audit
log the pass itself emits). No test calls `repoint_chunk` directly and none names a symbol
this patch adds, which is why the file compiles on the base.

**(c) Fixture includes the fault?** Yes.
- Leg 13 asserts `meta.raced()`, so the superseding generation really landed under the
  resolve. The old group's records are left in the store, as a real supersede leaves them —
  that is what lets the mutant prepare a rewrite of the retired group, rather than failing
  for some unrelated reason. The plan is built from the live generation's chunks, and the
  leg checks the record under the **live** group's key.
- Legs 3 and 4 assert the race landed before they look at the counters, so "one conflict"
  cannot be satisfied by a pass that never raced.
- The lost fragment is on a server in no fleet and no topology; the survivor fragment is
  real erasure-coded bytes, so the checksum verify runs for real.

## What I did not do, and what is still open

- **§8.7 wording** — see above; left for the human.
- **No seeded Tier-0 DST coverage for the move.** Deferred to #682 with an in-code marker
  (`reconstruction.rs:1165`); settled at earlier sign-offs. Leg 13 is one more scripted
  interleaving in the test file.
- **A flat record under `inode:01`** still loses its commit every pass, as on the base —
  #698's, settled.
- **Leg 3 leaves a stranded fragment** on the destination server — tracked leak #723,
  inherited as-is per the brief.
- What I read beyond `brief.md`: `iteration-v5/patch.diff` and `iteration-v5/build-notes.md`,
  because the sign-off says to keep that build and the brief points at the folder; the
  `repoint_chunk` doc and body in `metadata.rs`, to get the §6.3 sentence and the mutant
  mapping right; and the harness's `pdca.toml` / `run-verify.sh` header for the runner and
  the size threshold. I did not read earlier reviews.

- **Scratch.** I made no checkout or build directory. I left seven small files in
  `$PDCA_SCRATCH` (the harness's own per-bundle scratch root): `pdca-builder-777-ci.log`,
  `-verify.log`, `-negations.py` (the sweep script, re-runnable from the worktree root),
  `-prod.diff`, `-before-sweep.diff`, `-after-sweep.diff`, `-reconstruction.rs.good`. The
  builder instructions disagree on whether I should delete them (one section says the
  harness reclaims its roots and no `rm` is warranted; another says remove what you
  created), so I deleted nothing. Other `pdca-builder-777-*` entries there (`-prev`,
  `-redleg`, `-v3`, `-v4`, `-v4.diff`) are from earlier iterations, not this one.

## Self-review against the target's rubric

- *Docs currency*: §6.3 corrected in this patch; no new port, flag, RPC or persisted field.
- *Absent or unsupported entries* (count-based assertions): the new tick assertion is a
  count because the property **is** a count of operator signals (one conflict, no abort).
  Leg 13 asserts on placements and bytes, not counts.
- *Test fidelity*: the metadata double commits under one lock with real precondition
  checks, and the racing batch lands whole, as one commit would. DST coverage deferred to
  #682 as above.
- *Await discipline*, *One clock*, *Transactions*, *Serialization identity*: no production
  change this round; nothing new to check. Leg 13 adds one more byte-identity assertion on
  a segmented root.
