# Adversarial review — issue #813 (663.1: staged scrub and keep)

Bottom line: I could not break the fix. The red→green proof holds up, and the main parts of the
fix are each pinned by a test that fails when that part is undone. What I did find: one line of
the "keep" rule that no test pins, two small gaps against what the brief asked for, one stale doc
line, and a red gating T4 row that a human has to rule on.

## What I re-ran (in a scratch copy of `$PDCA_TARGET`)

- **Green:** `staged_protection.rs` 33/33 and `staged_scrub.rs` 13/13 pass on the patched tree.
  The DST legs `reconstruction_staged_handoffs_*` also pass under `--cfg madsim`, 50 seeds.
- **Red for A–C:** confirmed from `gate-logs/C4-verify.log`. On the red run, 9 of the 13 new tests
  fail, each on an assertion and not a compile error. All of them drive the real `reconcile_step`
  (`crates/custodian/tests/staged_scrub.rs:507-515`).
- **Red for D–F, reproduced by hand the way the brief describes:** I used the base production code,
  kept the new legs, and removed only the two new field initialisers
  (`crates/custodian/tests/staged_protection.rs:535-536`). 6 tests fail on assertions: both leg-G
  tests, leg H, both leg-I tests, and the rewritten leg F. Leg J passes on base, which is expected
  because base scrub never reads `part:`.
- **Mutations that the tests catch (each one makes the named test fail):**
  - Removing `!staged_incomplete` from the drain gate (`reconstruction.rs:434`) → leg I fails.
  - Making `staged_kept` never count as a reason not to certify (`:449`) → leg G and leg H fail.
  - Swapping scrub's read order so it reads `inode:` before `part:` → the C′ test with flip and
    drain both between the reads fails.
  - Swapping reconstruction's read order → leg H fails, and so does the DST leg, on its intended
    "drained the moving chunk's obligation" assertion (`crates/dst/tests/custodian.rs:3054`).
  - Removing the supersession check (`scrub.rs:219-226`) → leg J and
    `a_part_placement_a_committed_map_supersedes_is_not_checked` fail.
  - Stopping unreadable staged records from blocking scrub (`scrub.rs:303`) → leg F fails.
- **Clock:** `LoopClock` (`crates/server/src/custodian.rs:146-169`) wraps the caller's one clock
  closure, and each pass's `now_millis` and the seam both read it. Tests that build a context set a
  `ManualClock` to the same instant they pass as `now`. `STAGED_WRITE_WINDOW_MILLIS` reuses
  `gc::W_WRITE_MILLIS` (`gc.rs:201`), and `LATE_WRITE_DEADLINE_MILLIS` is built from it
  (`gc.rs:269`), so the deadline can never be smaller than the window. I found nothing to refute
  here.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:242`
  (`.chain(staged.held.keys().copied())`) is not pinned by any test. If I delete that line, all 46
  staged tests and both reconstruction DST legs still pass. Concrete failing case: an `Open`
  session whose `part:` record places an RS(2,1) chunk on only `[0, 1]`. That record is readable
  but has the wrong length, so the chunk lands in `StagedSet::held`, not `placed`. Queue an
  obligation for that chunk and run one reconstruction pass. The patched code keeps the obligation
  and answers `Blocked`: I checked this with a throwaway test in scratch, and it passes. With the
  line deleted, the pass drains the obligation and answers `Satisfied`: the same test fails with
  "outcome Satisfied". That breaks the brief's invariant: "an obligation is removed only when no
  record, committed or staged, names its chunk". Fix: add this leg, plus its `sidx:` twin, to
  `staged_protection.rs`.
  Lower value, same kind of gap: the `|| referenced.malformed.contains_key(..)` half of scrub's
  supersession check (`scrub.rs:221`) can also be deleted with no test failing.

- NEEDS-HUMAN [impl] — the brief's Scope (2) says "an empty queue still reads nothing", but no test
  checks this for the new staged read. I changed the empty-queue branch
  (`crates/custodian/src/reconstruction.rs:214-215`) so it calls `staged_fragments` anyway, and all
  206 `wyrd-custodian` tests still pass. The existing empty-queue test
  (`crates/custodian/tests/segmented_map_reconstruction.rs:697-717`) only checks that `inode:` is
  not read. Why it matters: under that change, a pass with nothing to do would return `Err` on any
  `mpu:`/`sidx:`/`part:` store fault, where today it answers `Satisfied`. Fix: extend that test to
  also assert that no `mpu:`, `sidx:` or `part:` read happens.

- NEEDS-HUMAN [impl] — when a `part:` placement is malformed (wrong length), scrub reports it but
  loses the record's key. `read_staged_part` stores only the chunk id
  (`crates/custodian/src/gc.rs:1650`, `set.malformed.insert(chunk.id, m)`). `emit_malformed` then
  prints the committed-map message "scrub found a committed placement of the wrong length"
  (`crates/custodian/src/scrub.rs:398-407`) and bumps the same `scrub_malformed_placement` counter
  that committed maps use. An operator who sees chunk X goes looking for an inode, and no inode
  names X. The only way to find the damaged record is to scan every `part:` key. GC's own staged
  reader keeps the key for the same kind of damage (`StagedSet::hold`, `gc.rs:1425`, `:1436`). The
  loop at `scrub.rs:139-141` also runs before the supersession filter, so a damaged leftover part
  record keeps raising this signal after a committed map has taken over the chunk. Fix: keep the
  `part:` key in `StagedPartSet::malformed` and name it, with wording or an action that says it is
  a staged record.

- NEEDS-HUMAN [impl] — a stale doc line the brief's Scope (4) asked to fix is still there.
  `crates/custodian/src/gc.rs:89-90` (module doc) still says "Scrub and the drain-status surface
  do not read the class at all." Scrub now reads the `part:` half, and reconstruction now reads the
  whole class through `staged_fragments` (`reconstruction.rs:217`). The patch updated the
  `StagedSet` and `reconcile` docs but not this line.

- NEEDS-HUMAN [human] — the T4 batch review is a gating row and it is red (`check-gates.json`,
  `gate-logs/T4-batch-review.log`), so it has to be adjudicated before accept. I think both of its
  blocking findings should be turned down, with the reasons recorded:
  - (a) `scrub.rs:233`: a fragment on a D server that is not in `ctx.fleet` is skipped. The brief
    puts this out of scope in so many words ("servers absent from the live fleet … the same for
    committed chunks today"), and the deployed loop does the same thing for committed chunks
    (`crates/server/src/custodian.rs:555-558`).
  - (b) `scrub.rs:302-313`: a malformed part placement still lets the pass answer `Satisfied`.
    That is the same rule base scrub already applies to malformed committed placements (they are
    reported at `scrub.rs:161-168` and never block), and `staged_scrub.rs:1091-1096` pins
    `Satisfied` on purpose. Making both kinds block would change behaviour for committed maps,
    which is outside this slice.

- NEEDS-HUMAN [human] — the iteration-2 carry-forward asked to "prevent or explicitly bound" the
  scrub/reconstruction loop on stale `part:` placements. The patch prevents it only while a
  committed map names the chunk (`scrub.rs:219-226`). Remaining case: a published object is deleted
  or overwritten before its `part:` records are retired, and reconstruction had already moved one
  of its fragments. The leftover part record then names the empty old position again. Scrub
  re-queues the chunk every pass (`scrub.rs:274-278`), and reconstruction keeps it and answers
  `Blocked` every pass (`reconstruction.rs:623-626`), until the `retire:records:` drain deletes the
  record. Nothing on main runs that drain yet: no production code writes `part:` or `retire:`
  records. So this is forward-looking and low severity, and the only bound is future code. It is
  a scope call whether this slice needs to state that bound.

- Unwarranted claim, not a patch defect: C4-verify's summary line says "13 test(s) ran red", but its
  own log shows `4 passed; 9 failed` on the red run. The four that pass on base are leg B (green on
  base by design), the intact control, and the two supersession (A′) tests. The A′ tests only mean
  something against the patched code. I checked that they do catch the supersession check being
  removed.

- Note, not raised as a finding: scrub's own publication-race protection is covered only by the
  scripted-hook tests (`staged_scrub.rs:994-1055`). The seeded DST sweep's `Driver` enum
  (`crates/dst/tests/custodian.rs:2619-2623`) covers GC and reconstruction but not scrub. The
  iteration-1 sign-off asked for a scripted regression test for scrub and DST coverage only for
  reconstruction, so I am treating that as already decided.

## Refutation attempts that failed

I tried all of these and could not break the fix: the red→green evidence (both the automated A–C
proof and the hand-run D–F proof), both read orders (scripted tests and DST), fail-closed handling
of unreadable staged records in both loops, supersession after a fragment is moved, rejecting an
empty staged placement, the single-clock seam, and the `W_write` wiring.
