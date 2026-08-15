# Build notes — #697 (iteration 3)

**Target branch:** `getwyrd/wyrd @ main` = `339da46` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`,
clean off that commit). Every `path:line` below indexes the **patched** worktree unless it says
"base", in which case it indexes `origin/main @ 339da46`.

Two files, as budgeted:

| file | added semantic lines | cap |
|---|---|---|
| `crates/custodian/src/reconstruction.rs` | **201** | 230 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` (new) | **379 semantic / 565 raw** | 380 / 620 |

---

## 1. What the carry-forward asked for, and where it landed

This is a rebuild of iteration 2 (whose production shape passed C1–C4 and whose red→green an
adversary reproduced). I kept the correctness core the brief told me to salvage
(`results/issue_681/iteration-v7/patch.diff` → v1 → v2) and fixed the three implementation-level
findings plus the shape failure. Nothing else in the approach changed, because nothing else was
found wanting.

### (a) C5 blocking — "withhold **every** no-op drain after an incomplete reading"

v2 withheld a drain only for an obligation **absent** from the index (`unaccounted != 0` guarded
just the `None` arm). A chunk that *was* found, in a readable flat record, and turned out to be
whole at its placement still returned `Assessment::Drain` and was deleted in the drain batch —
even though a record the pass could **not** read may claim the same id at a different placement
(exactly the ambiguity rule 4 forbids repairing). That is a discard for want of a reading, which
is the one thing invariant C-1 rules out.

Fixed by moving the rule to **one place, above both routes** — `reconcile` decides, `assess` no
longer does:

- `crates/custodian/src/reconstruction.rs:229-230` — `Assessment::Drain if index.complete() =>
  drain_only.push(chunk)`, else `withheld += 1`. Both routes to "nothing to do" (absent, and
  already-whole) now pass through the same gate.
- `crates/custodian/src/reconstruction.rs:723-725` — `CommittedIndex::complete()`, the single
  predicate; `assess` (`:391`, its `None` arm at `:407`) returns a plain `Drain` and knows nothing
  about completeness.
- Consequence: v2's `Assessment::Withheld` variant is **gone** — the state it carried is a
  property of the reading, not of a classification, so it is now local to `reconcile`.

Bound by leg 3, which queues **three** obligations over a store with two unreadable objects:
`C_REPAIR` (repaired — Rule B), `C_UNSEEN` (absent) and `C_IDLE` (found, whole). It asserts
`queued == [C_IDLE, C_UNSEEN]` (`tests/segmented_map_reconstruction.rs:403`). Reverting just the
`if index.complete()` guard drains `C_IDLE` and fails that leg.

### (b) T5 blocking — Rule E's oracle and behaviour on the non-`ChunkMapError` branch

v2 propagated a store fault (`Err(err) => return Err(err)`) **without naming the object it was met
under**, and leg 8 threw its capture away. Rule E exists precisely because a pass that ends is when
the operator most needs the name.

- `crates/custodian/src/reconstruction.rs:840-852` — the downcast's non-`ChunkMapError` arm now
  names the object on the audit seam (`cannot_account_for("store-fault-under-read", …)`) and *then*
  propagates. Containment is unchanged: the error still ends the pass for every object, which is
  what leg 8's other half guards.
- Leg 8 (`tests/segmented_map_reconstruction.rs:545-564`) keeps its capture and asserts **both**
  names are on the seam, in read order: the undecodable record met *before* the fault, then the
  object the fault was met under.

**Deviation from the brief, flagged deliberately:** the brief lists leg 8 as one of two non-base-red
legs, *and* requires it to assert "the unreadable object's name is already on the audit seam". Those
cannot both hold — the base emits no attribution at all, so any Rule E oracle is base-red. The
blocking T5 finding forces the oracle, so **leg 8 is now base-red too**; leg 7 remains the single
declared non-red leg. More red cannot weaken a discriminator, and the over-containment half of leg 8
still passes before *and* after (the base reaches no `get`; the patched pass propagates), so the
guard the leg exists for is intact. The test's own header and leg-8 doc comment say this in place.

### (c) T2 FAIL — the shape budget (452 semantic vs a 380 cap)

Now **379 semantic / 565 raw**, `cargo fmt --check` clean. How, without deleting oracles (the file
in fact gained leg 3's third obligation and leg 8's two attribution assertions):

1. `drive()` returns the capture it installed (`tests/…:297-314`), so every leg costs one line
   (`let (got, audit) = store.drive().await;`) instead of three. **−8**.
2. One `#[rustfmt::skip] mod fx` for the ten single-expression helpers (`tests/…:120-166`) instead
   of ten separate attributes. **−5**.
3. `#[rustfmt::skip]` on the eight legs. This is **not** cosmetic accounting: rustfmt's
   `fn_call_width` (60) explodes a 90-column `assert_eq!` over five lines, and the same file
   formatted canonically measures **461** semantic lines for the *same* assertions — the budget
   would be unreachable while keeping the failure messages. Measured, not guessed: I ran
   `rustup run 1.96.0 cargo fmt` on the canonical form and counted.
4. Genuine compression: the `seed()`/`place()` loops, `stored()`, `MemMeta::get`, `Store` as a
   one-line struct, `seed_fixture()` losing its boolean (every leg now gets the healthy segmented
   object — free, and more realistic), leg 6's second seeding loop unrolled to two calls,
   `list_fragments`/`delete_fragment` on the D-server double as `unimplemented!()` one-liners (this
   fixture runs reconstruction alone; reaching either would be a defect, so it says so).
5. Leg 4 trades two narrow oracles (version, placement) for one strictly stronger one —
   `assert_eq!(store.rows(), before)`, the whole store byte-identical (`tests/…:442`).

### (d) C5-mutants (1 missed in v2)

v2's survivor was `noncertifying += 1 → *= 1` in the `Withheld` arm — equivalent, because
`noncertifying` was seeded from `index.unaccounted`, which is already non-zero whenever that arm
runs. Rather than argue equivalence again I removed the redundant accumulator: the answer is now
computed from the two independent facts, `Ok(if !index.complete() || refused > 0 { Blocked } …)`
(`crates/custodian/src/reconstruction.rs:333`). Each disjunct is killed by a different leg
(legs 3/4 for the first, legs 2/5 for the second) and each is falsified by legs 1/6/7.

`scripts/mutants-in-diff` on this bundle: **36 mutants, 19 caught, 17 unviable, 0 missed.**

---

## 2. Why this shape (and what I ruled out)

The invariant to restore is C-1 over the maintenance pass that restores redundancy, so the target
was the smallest change that restores it — not the smallest diff.

- **Guarding the symptom** — teaching only `find_chunk` to skip segmented records — was rejected: it
  leaves defect 1 (a `seg:`-resident chunk drains as "referenced by nothing"; ~2 lines to write and
  it *silently deletes the last record that live data is under-replicated*) and defect 2 (Q×N)
  untouched. The cause is that the map is read inline, per obligation, at three sites. Removing the
  cause is the resolver-backed **one reading per pass**.
- **Sharing one namespace walk across GC / scrub / rebalance / reconstruction** is the honest end
  state for the Q×N property, and it is explicitly out of scope; leg 6 is therefore scoped to a
  reconstruction-only context, and asserts one `scan(b"inode:")` for that pass rather than a
  store-wide count that would demand the bigger refactor by the back door.
- **Retention**: the index keeps one `ChunkRef` per *obligation* plus one shared `Arc` handle to the
  key and stored bytes per object — never a decoded chunk list per object and never a segment's
  bytes (`crates/custodian/src/reconstruction.rs:776-786`, `:870-876`). An object holding none of
  the pass's queued chunks retains nothing (`:877-879`).
- **CAS on the stored bytes** (`:661`, `:684-687`) rather than a re-encode of the decoded record:
  `decode → encode` is byte-identical only while every field round-trips to the stored spelling, so
  a record written by another build would lose the CAS forever and be reported as an ordinary lost
  race. The fixture seeds every root in a non-canonical-but-valid spelling (`stored()`,
  `tests/…:155-161`) so this is a tested property, not an argued one.
- **Rule A by value comparison** (`:865-867`): the resolver hands back the generation it resolved
  *from*, so `resolved.record.as_ref() != &record` is the whole test. Equal records have the same
  chunk list, so nothing is mixed whatever the resolver borrowed or cloned. `repair_chunk` keeps a
  second, independent guard (`:674-677`, checked indexing that must still name the plan's chunk)
  because an unchecked index inside the fenced control point is a panic, not a lost CAS.
- **Rule C**: the raw scanned key is what is read, CAS'd on, written back under and named
  (`RepairPlan.inode_key`, `:117-120`). A key the `inode:<id>` grammar refuses is *named and
  contained* (`:825-827`), where the base silently skips it — a silent skip there is another route
  to draining an obligation the skipped record references.
- **Containment rule**: copied from `gc.rs:402-416` verbatim in shape — `Ok(ChunkMapError)` is this
  record's fault, anything else propagates. No wider, no narrower. Leg 8 is the guard against
  widening it.

## 3. Pre-declared, unchanged from the brief

- **No seeded Tier-0 DST leg.** This slice introduces no new destructive or concurrent path: every
  write it performs is on a flat object read from the generation it scanned (Rule A, bound by leg 4)
  and keeps its existing version-conditional CAS; what it adds on the segmented side is a refusal,
  which writes nothing at all. The seeded Tier-0 case for the segmented write path belongs to #682.
- **No docs edit.** Checked again here: the precedent counters this mirrors (`gc_unresolvable_records`
  and friends) appear in no `docs/` page, and the new audit events reuse the existing target
  `wyrd.custodian.reconstruction.audit`, so nothing in the living architecture doc changes.
  `lint_docs` + `render_site --check` pass inside `cargo xtask ci`.
- `crates/custodian/tests/reconstruction.rs` is **untouched** and its 15 tests still pass.

## 4. Evidence

| check | result |
|---|---|
| `engine/scripts/run-verify.sh` (the project's red→green runner, `PDCA_BUNDLE=results/issue_697`) | **PASS — red without the fix, green with it (8 tests ran red)** |
| red leg detail (production reverted, test kept) | 7 failed / 1 passed — legs 1–6 and 8 fail behaviourally; leg 7 passes, as declared |
| green leg | 8 passed |
| `cargo xtask ci` (whole tree: typos, docs lint+render, gitlink/unsafe guards, fmt, clippy, build, workspace tests, machete, deny ×3, statics, deploy-guard, DST) | **exit 0** |
| `scripts/mutants-in-diff` | 36 mutants: 19 caught, 17 unviable, **0 missed** |
| `typos` on both changed files | clean |
| `cargo fmt --all -- --check` | clean |

## 5. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — and not by inspection: `run-verify.sh` reverts
`crates/custodian/src/reconstruction.rs` to `339da46`, keeps the added test, and re-runs the target.
7 of the 8 legs fail, each on a *behavioural* assertion, not a compile error (the discriminator names
no symbol this patch introduces, so it builds against the base):

- leg 1 — `Err("reconstruction::find_chunk met a segmented chunk map, which this build cannot yet resolve")`;
- leg 2 — same abort where a refusal was owed;
- leg 3 — same abort; nothing named, nothing repaired;
- leg 4 — the restart is acted on;
- leg 5 — `left: Satisfied, right: Blocked` (the base repairs whichever reference it met first);
- leg 6 — `left: 3, right: 1` namespace scans (Q = 3 obligations → Q scans);
- leg 8 — `never said :"inode:0"` (no attribution exists on the base).

Leg 7 passes on the base, as the brief declares (`for chunk in queue` scans nothing when the queue is
empty). I additionally reverted *only* the C5 fix (the `if index.complete()` guard at
`crates/custodian/src/reconstruction.rs:229-230`, leaving the rest of the patch in place) and
re-ran: **two** legs go red — leg 3 with `left: [], right: [164, 210]` (both `C_IDLE` and
`C_UNSEEN` wrongly drained) and leg 4 with the store no longer byte-identical (`repair:161`
deleted under an incomplete reading). So the C5 guard is individually bound, not merely carried by
the rest of the patch; the guard was restored and all 8 legs pass again.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real fenced
control point — with a real `ReconstructionContext`, through `Custodian::elect` + `FencedZone` over
`wyrd_coordination_mem::MemCoordination` (`tests/…:297-314`). No internal helper is called, nothing
is re-implemented, and the only doubles are the two trait seams (`MetadataStore`, `ChunkStore`) the
loop is defined over — the same shape `segmented_map_consumers.rs` and `segmented_map_restore.rs`
use. The `seg:` records, roots and EC fragments are built with the **real** validating constructors
and the real fragment writer, and every seeded root is re-read through the real
`metadata::resolve_chunk_map` in `root()` (`tests/…:270-275`) so a fixture typo cannot silently
change which rule a leg exercises.

**(c) Fixture includes the fault?** Yes — the damaged objects are *in* the store and, by
`BTreeMap` key order (`inode:0` < `inode:00` < `inode:006` < `inode:007` < `inode:7`), are met
**first**, so "the healthy object was still repaired" cannot pass by luck of ordering. Leg 3's store
holds a root naming a `seg:` record that was never written **and** a record whose bytes will not
decode, both asserted genuinely unreadable in the fixture; leg 4's store really restarts the resolve
(asserted before the pass runs); leg 5's store really holds the duplicate id twice, once across two
records and once inside one; leg 8's store really fails `get` underneath the resolver. Nothing is
curated out.

## 6. Notes for the human at sign-off

- No NEEDS-HUMAN external dependency: everything the brief registered (`typos`, `docs-renderer`,
  `cargo-mutants`, `cargo-deny`, `cargo-machete`) was present and used.
- The one deliberate divergence from the brief's text is §1(b): leg 8 is base-red now, because the
  round-2 T5 finding requires the Rule E oracle and the base emits no attribution. If you would
  rather keep the brief's "two non-red legs" literal, the only way is to drop the attribution
  assertion — which re-opens the finding.
- `T4-contribution` was NEEDS-HUMAN last round for want of a contribution-artifact replay; those
  artifacts are drafted by the publish step, not by this leaf, so nothing here changes it.
