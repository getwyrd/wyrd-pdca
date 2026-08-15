# build-notes — issue 696, iteration 4 (rebalance reads through the resolver, contained)

Target branch `getwyrd/wyrd @ main`, base `339da46`. All target-source edits made in
`$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`; every `path:line` below indexes that
worktree with `patch.diff` applied, unless it says "on the base".

## TL;DR — what this rebuild changed, and what it deliberately did not

The **production hunks are byte-identical to iteration 3's** — verified mechanically:

```
$ diff <(sed -n '/^diff --git a\/crates\/custodian\/src\/rebalance.rs/,/^diff --git a\/crates\/custodian\/tests/p' iteration-v3/patch.diff | head -n -1) \
       <(sed -n '/^diff --git a\/crates\/custodian\/src\/rebalance.rs/,/^diff --git a\/crates\/custodian\/tests/p' patch.diff       | head -n -1)
PRODUCTION HUNKS BYTE-IDENTICAL TO ITERATION 3
```

That is deliberate. Iteration 3's `rebalance.rs` passed C4-ci, C4-verify red→green and C5
mutants (`0 missed`, 21 tested / 11 caught / 10 unviable), and **neither** carry-forward
blocker is about it. Both blockers are about the **discriminator** and the **triage file**:

| Carry-forward item | Where it is answered |
|---|---|
| 1. `certifies` silently accepts every `Err`, so leg 5 does not prove `reconcile_step` returns `Changed`/`Satisfied` rather than erroring | `crates/custodian/tests/segmented_map_rebalance.rs:175-180` — the helper now asserts on the `Ok` variant explicitly |
| 2. The two Tier-0 DST findings restate an already-settled question; record-reject them rather than adding DST coverage | `results/issue_696/review-rejected.md` — three new entries (`src/rebalance.rs:259`, `tests/…:400`, `tests/…:408`) |

Total delta vs iteration 3: **4 hunks in one file** (the test) plus the bundle's
`review-rejected.md`. Still **exactly 2 target files**.

## 1. The `certifies` blocker — and the classification it forces

### What was wrong

Iteration 3 shipped (`iteration-v3/patch.diff`, test line 167-171):

```rust
fn certifies(pass: &Pass, expected: Reconciled) {
    if let Ok(answer) = &pass.0 {
        assert_eq!(*answer, expected, "{}", pass.1);
    }
}
```

On `Err` the assertion is not weakened — it is **not executed**. Leg 5 is the leg the brief
made mandatory *because a v7 adversary flipped `Satisfied`→`Blocked` undetected*; with this
helper the same leg accepts a pass that answers nothing at all. It is the "count-based
assertion that can pass while the property fails" defect class from `AGENTS.md:175-177`
(*Absent or unsupported entries*), applied to a test.

### What it is now (`crates/custodian/tests/segmented_map_rebalance.rs:175-180`)

```rust
fn certifies(pass: &Pass, expected: Reconciled) {
    let Ok(answer) = &pass.0 else {
        panic!("the pass must ANSWER: {:?}\n{}", pass.0, pass.1)
    };
    assert_eq!(*answer, expected, "{}", pass.1);
}
```

An `Err` now fails the leg exactly as a wrong `Reconciled` does. That is the whole fix.

### The consequence I am NOT hiding: **leg 5 is base-red**

Iteration 2's T5 finding said "make leg 5 genuinely non-base-red **or return the
classification to Plan**". Iteration 3 took the first branch and produced the vacuous helper
above. I took the **second branch**, because the first is not reachable:

- Leg 5's store must contain a **healthy segmented object** — that is what the leg is about
  (`tests/segmented_map_rebalance.rs:447`, `seed_segmented(&meta, b"inode:1", &chunks, 2)`).
- On the base, **any** segmented root ends the whole pass:
  `git -C ../wyrd show origin/main:crates/custodian/src/rebalance.rs` at `:162`
  (`plan_evacuations`) — `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported{..})?`.
- Therefore any assertion about **what the pass answers** over leg 5's store fails on the
  base. The only way to keep leg 5 off the base red is to assert nothing about the answer —
  which is precisely blocker 1.

So the honest set is: **legs 1-6 base-red, leg 7 the one deliberately non-red leg**, one leg
more red than the brief's `Falsifiability`/`Verification posture` predicted. This is written
into the artifact the reviewer reads, not only here — `tests/segmented_map_rebalance.rs:16-23`
(module header) and `:164-174` (the helper's own doc comment) state it.

**Why this is harmless to the evidence.** C4-verify's RED leg reverts production, keeps the
test, and requires the *target* to fail (`engine/scripts/run-verify.sh:466-509`); it does not
require any particular leg to pass. Leg 5 going red on the base **adds** a failing leg to the
red and removes nothing. Measured, not argued — see §3.

**For the human at sign-off:** the brief's `Falsifiability` line "legs 5 and 7 are declared
non-red" and the `Verification posture`'s "legs 5 and 7 are deliberately not base-red" are
inaccurate for leg 5 and should be corrected at Plan if this text is reused for #682. Nothing
else in the brief depends on it.

## 2. Alternatives considered, with their cost

**(a) Keep tolerating `Err` (iteration 3's shape).** Rejected: it is the blocker. Cost is not
lines, it is evidence — in the RED leg run in §3, leg 5 fails at
`tests/segmented_map_rebalance.rs:177` with
`the pass must ANSWER: Err(Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" }))`.
Under iteration 3's helper that identical run scored leg 5 **green**. Any post-fix regression
that re-aborted the pass over this store would have been accepted in silence.

**(b) Make leg 5 non-red by dropping the segmented object from its store** (delete
`tests/segmented_map_rebalance.rs:447`). Rejected: the leg would then bind nothing. The guard
it exists to pin lives at `crates/custodian/src/rebalance.rs:214-216`
(`if fragments == 0 { return; }`) inside `Refusals::refuse`, and `refuse` is reached from
exactly one place — `crates/custodian/src/rebalance.rs:295-296`, under
`if record.chunk_map.is_segmented()`. A store with no segmented object never executes the
guard, so the §3 ablation would go green. One deleted line, and the leg is decoration.

**(c) Keep only the seam half** — `assert_eq!(lines(&second.1, "refused"), 0)` at
`tests/segmented_map_rebalance.rs:462`, dropping the two `certifies` calls. This *is*
non-base-red and non-vacuous, so it is the tempting option. Rejected because it stops binding
the **answer**, which is the exact thing the v7 adversary flipped and the exact thing the
brief's leg 5 requires ("the pass answers `Satisfied` — a `step(false, true)` shape").
Concretely checkable: `Reconciled::Blocked` is returned iff `refused.total() > 0`
(`crates/custodian/src/rebalance.rs:152-162`), and `total()` is fed by `self.0 += 1` at
`:181` and `:218` — a mutation that increments the counter without emitting (drop the
`emit_refused` call at `:217`, or hoist `self.0 += 1` above the `fragments == 0` return)
flips the answer while emitting **no** refusal line. The seam-only leg stays green over it.
Both halves cost 3 lines together (`:451`, `:457`, `:462`); keeping them is not a size
question.

**(d) Add the seeded Tier-0 DST case the two review findings ask for.** Rejected and
**recorded**, not silently dropped — see §4.

## 3. Red→green, run through the project's own runner (never hand-rolled)

Runner: `engine/scripts/run-verify.sh`, the configured `C4-verify` gate cmd
(`pdca.toml:818`), which applies `$PDCA_BUNDLE/patch.diff` to a clean lane-scoped checkout of
`origin/main` and runs both legs itself. Classification first:

```
$ ./engine/scripts/run-verify.sh --classify results/issue_696/patch.diff
ADDED_TEST crates/custodian/tests/segmented_map_rebalance.rs
CRATE crates/custodian
```

Then the gate (`PDCA_BUNDLE=results/issue_696 PDCA_LANE=1 ./engine/scripts/run-verify.sh`):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_rebalance (fix applied)
  ... 7 passed; 0 failed
run-verify.sh: RED — (production reverted, test kept)
  test result: FAILED. 1 passed; 6 failed
    a_fragment_in_a_seg_record_stays_put_refused_and_does_not_certify
    a_segmented_object_holding_nothing_evacuable_still_certifies_the_drain   <-- leg 5, NEW
    a_segmented_object_no_longer_stops_the_flat_evacuation_beside_it
    an_unreadable_object_is_named_the_walk_continues_and_nothing_certifies
    rule_a_the_pass_never_acts_on_a_generation_it_did_not_read
    rule_d_one_refusal_per_object_accounting_for_what_it_leaves_behind
run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
```

The one passing leg on the base is leg 7
(`a_fault_that_is_not_one_objects_map_still_ends_the_pass`) — the brief's declared non-red
leg, which guards against over-containment and has no base behaviour to flip.

### Ablation — leg 5 really binds the over-containment guard

The brief makes leg 5 mandatory because at v7 an adversary replaced the guard's body with a
no-op and everything stayed green. Re-run that experiment against this bundle. Ablation
applied to `crates/custodian/src/rebalance.rs:214-216`:

```rust
-        if fragments == 0 {
-            return;
-        }
+        if fragments == 0 { /* ABLATION: guard body removed */ }
```

Result (same runner, ablated patch in a scratch bundle):

```
run-verify.sh: GREEN — ... test result: FAILED. 5 passed; 2 failed
  a_segmented_object_holding_nothing_evacuable_still_certifies_the_drain   (leg 5)
    tests/segmented_map_rebalance.rs:179  left: Blocked   right: Changed
  a_segmented_object_no_longer_stops_the_flat_evacuation_beside_it         (leg 1)
    tests/segmented_map_rebalance.rs:331  left: (Blocked, ([1], 2), true)  right: (Changed, ([1], 2), true)
```

The guard is bound, and it is bound **at the `certifies` assertion** (`:179`), i.e. by the
helper this rebuild repaired. The audit stream in that failure also shows the over-broad
behaviour in the clear — `"action":"refused","inode":"inode:1",…,"fragments":0` — a refusal
over an object holding **zero** fragments on the draining server, which is exactly "no
decommission ever certifies on a store holding a multipart object". The production file was
restored from a byte copy afterwards and the shipped `patch.diff` regenerated from the
restored tree.

### Whole-tree gate

`./engine/xtask.sh ci` (the `C4-ci` gate cmd, `pdca.toml:811` — fmt / clippy `-D warnings` /
build / whole-workspace test / deny / conformance / prose): **all checks passed** (log kept
at `$PDCA_SCRATCH/pdca-builder-696-ablate/xtask-ci.log` during the run; the scratch dir is
removed at handover). `cargo fmt -p wyrd-custodian` was run over both touched files and
`cargo fmt --check` is clean, so the target's own commit hooks have nothing to reject.
`crates/custodian/tests/rebalance.rs` is untouched and green inside that run.

## 4. The two Tier-0 DST findings — recorded-rejected, not worked around

Carry-forward item 2. Three entries appended to `results/issue_696/review-rejected.md`
(`crates/custodian/src/rebalance.rs:259`, `crates/custodian/tests/segmented_map_rebalance.rs:400`,
and `:408`). Verified they parse and pair, using the gate's **own** loader rather than by eye:

```
$ python3 -c '<import scripts/review-branch>; rb.load_rejected(...); rb.is_rejected(f, rej)'
crates/custodian/src/rebalance.rs:259                   TEST-GAP 'seeded Tier-0 DST coverage'  -> True
crates/custodian/tests/segmented_map_rebalance.rs:400   TEST-GAP 'seeded Tier-0 DST coverage'  -> True
crates/custodian/tests/segmented_map_rebalance.rs:408   TEST-GAP 'seeded Tier-0 DST coverage'
total 8 recorded decisions
```

The `:408` entry is the `:400` fixture line **re-anchored**: this rebuild adds 8 lines above
it (module header + `certifies` doc), so leg 4's `let stale = …` moved `:379` → `:400` →
`:408` across three iterations while the finding stayed the same. Recording the new anchor is
maintenance of the decisions file, not a new decision — the reason text says so.

The reason itself is the brief's own pre-declared `Verification posture`, restated with
citations: this slice adds no new destructive or concurrent path. It calls the **shared**
read-side resolver every other consumer already calls on this base
(`crates/custodian/src/rebalance.rs:259`), whose supersession/restart arm belongs to `core`
(`crates/core/src/metadata.rs:2619-2632`); what the patch adds on top is a **containment**
(Rule A, `crates/custodian/src/rebalance.rs:285-291`) under which **nothing at all is
written** — leg 4 (`tests/segmented_map_rebalance.rs:403-430`) asserts the live record is
byte-identical and its `version` never moved. Every write the pass still performs is the
pre-existing version-conditional CAS on a **flat** object read from the generation it scanned
(`crates/custodian/src/rebalance.rs:448-451`, unchanged in shape from the base's `:310-313`).
The seeded Tier-0 DST case for the segmented **write** path belongs to #682. `AGENTS.md:200-203`
(*Deferrals are settled*) and `:204-205` (*Out of scope*) are the repo's own rules for this.

## 5. Self-review against the target's standing rubric (`AGENTS.md:122-211`)

Read `## Review rubric & protocol` in the worktree before emitting the patch, as required.
The delta this iteration is test-only, so most rows are inherited unchanged from iteration 3's
green C4-ci; the ones the delta touches:

- *Absent or unsupported entries* (`AGENTS.md:175-177`) — "never … a count-based assertion
  that can pass while the property fails". This is the blocker, and §1 is the fix.
- *Test fidelity* (`AGENTS.md:188-190`) — "a new destructive or concurrent path lands with
  seeded Tier-0 DST coverage". No new destructive or concurrent path is added; declined with
  a recorded reason and an issue reference (#682), per *Out of scope* / *Deferrals are
  settled*. §4.
- *One clock per correctness lifecycle* — untouched: `now_millis` is still the caller's
  logical clock, threaded from `reconcile_step` (`crates/custodian/src/rebalance.rs:120`,
  `:367-372`); no new clock read.
- *Docs currency* (`AGENTS.md:154-157`) — no port, API operation, RPC, CLI flag or persisted
  field changes. The `seg:` / `inode:` record shapes are read through `wyrd_core::metadata`
  only; no encoding is added or altered. The brief pins "no docs edit (checked at Plan)".
- *Every new crate root carries `#![forbid(unsafe_code)]`* — no new crate; the new test file
  carries it anyway (`tests/segmented_map_rebalance.rs:25`).
- *Reviewer protocol → Definition of done* — deterministic gates green plus one deep review
  whose findings are each fixed or recorded with a reason: 2 fixed, 2 (+1 re-anchor) recorded.

## 6. Forced self-refutation (recorded per the Do protocol)

**(a) Genuine red — does the test fail with the fix reverted?** **Yes**, actually reverted and
re-run by the project's own gate, not asserted: the RED leg in §3 reverts
`crates/custodian/src/rebalance.rs` to `origin/main` and keeps the added test —
`test result: FAILED. 1 passed; 6 failed`, each failure carrying
`Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" })`, the base defect
at `rebalance.rs:162`. Leg 5 in particular — the leg this iteration repaired — fails at
`tests/segmented_map_rebalance.rs:177`, the new `panic!`; under iteration 3's helper the same
leg passed. A second, sharper revert (the guard-only ablation, §3) fails legs 5 and 1 while
the other five stay green, so the red is *specific* and not just "everything falls over".

**(b) Production path — does the test drive the production code the fix changes?** **Yes**.
Every leg calls `wyrd_custodian::reconcile_step` (`tests/segmented_map_rebalance.rs:306`), the
real fenced control point, behind a real `Custodian::elect` + `FencedZone` over
`wyrd_coordination_mem::MemCoordination` (`:299-301`), and `reconcile_step` dispatches the
production `rebalance::reconcile` this patch edits. No internal helper is called, nothing is
re-implemented in the test, and the fixtures build their records through the production
validating constructors (`SegmentedMap::new`, `SegmentRecord::new`, `metadata::encode`,
`seg_key` — `:224-274`), so a fixture typo cannot silently change which rule a leg exercises.
The only doubles are the trait seams themselves (`MetadataStore`, `ChunkStore`), which is how
every sibling suite in this crate drives the same code. Corroborated independently by the
ablation: editing three production lines changes the test's verdict, which a test driving a
copy could not do.

**(c) Fixture includes the fault?** **Yes** — each leg seeds the *real* failing element, none
is curated out. Leg 1/2/5/6: genuine `seg:` records plus a segmented root, so the resolver
really has a segmented map to resolve. Leg 3: a root naming a `SegmentRef` whose `seg:` record
was **never written**, with the fixture *proving* the fault before asserting on it —
`assert!(resolve.is_err(), "fixture: (a) really is unresolvable")`
(`tests/segmented_map_rebalance.rs:374`) — beside a second record whose bytes genuinely do not
decode, both **first in `BTreeMap` key order** so the walk meets the damage before the healthy
object, and the healthy fragment it must still evacuate is in the same store. Leg 4: a real
stale-vs-live generation, `scan` answering the superseded segmented root while `get` answers
the live flat one, so the production resolver really takes its restart arm. Leg 7: a real
non-`ChunkMapError` store fault injected at `get`. Leg 2 asserts the draining server **still
holds** the fragment afterwards rather than asserting on a filtered view of the fleet.

## 7. Anything a human still has to do

Nothing mechanical is unverified — the fix is fully headless-testable and both legs ran. Two
judgment items for sign-off:

1. **Leg 5 is base-red** (§1). Accepted deliberately, stated in the test file itself. If the
   brief's `Falsifiability` wording is reused for #682, correct it there.
2. **The Tier-0 DST question has now been raised in three consecutive rounds** at three line
   numbers for one fixture line (§4). It is settled at Plan and recorded three times; if the
   human disagrees with the settlement, that is a Plan-level re-decision (and #682's scope),
   not an in-PR fix.

No external dependency beyond the base Rust toolchain was needed; nothing was substituted,
shimmed, or read-instead-of-run. Scratch (`$PDCA_SCRATCH/pdca-builder-696-ablate/`) is removed
at handover.
