# Build notes — issue #697, round 5 (rebuild after the round-4 sign-off)

**Withheld from the reviewer. Written for the human at sign-off.**

All `path:line` citations below are **on the patched tree** (`$PDCA_WORKTREE`
= `/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `origin/main @ 339da46`, verified
`HEAD == origin/main == 339da46`) unless the line is explicitly labelled *base* or *v4*.

---

## 1. What changed relative to iteration-v4, item by item

Round 4's sign-off listed six required changes. Each is addressed below with the code that
does it. The rest of v4's patch (the one-reading restructure, rules A/C/D/E, containment,
the refusal vocabulary) is salvage and is unchanged except where noted.

### (1) The permanent-stall bug — `may_land` applied unconditionally

**v4:** `repair_chunk` consulted `may_land` on *every* claimed slot (v4 `reconstruction.rs:671`),
so a healthy store — complete reading, one stray fragment standing at the deterministic
re-placement target — went `Blocked` on pass 0, 1, 2, … forever, with the chunk permanently
under-replicated. The adversary reproduced it in both directions.

**Now:** the probe runs **only while the pass's reading has a hole in it** —
`if !index.complete()` at `crates/custodian/src/reconstruction.rs:557`. Over a complete reading
no claimant can be hidden (a duplicate id is already `Site::Refused` via
`CommittedIndex::note`, `:812`, and never reaches the plan), so the pass repairs exactly as the
base does — the stray is overwritten. That is now a **direct oracle**, not an argument: leg 1
plants a stray at the claimed slot and asserts the rebuild's own bytes replaced it
(`crates/custodian/tests/segmented_map_reconstruction.rs:360-376`, via
`assert_repair_landed`'s byte comparison at `:335-339`).

### (2) The gauge floor — a withheld repair must not floor `reconstruction_under_replicated`

**v4:** the refusal happened in the *repair* loop, after `under_replicated += 1` and after
`emit_repaired`, so a never-landing repair sat on the repairable-backlog gauge every pass.

**Now:** the decision moved **into the assessment frame** and reuses the classification the
file already has for "repairable, but nowhere to put it this pass" —
`Assessment::Blocked` (`:557-566`), which is *off* the backlog gauge and on the distinct
`reconstruction_repair_blocked` level (`:239`). This is the file's own precedent applied a
second time: the base already mirrors `select_distinct_domains_excluding` inside `assess`
precisely "so a chunk that WOULD abort in the repair loop is diverted before it inflates the
backlog" (base `reconstruction.rs:436-448`; now `:522-534`). Consequences:

* no plan is dispatched, so `emit_repaired` never fires for it → **no offset is owed** → the
  ADR-0011 netting formula is untouched (see (5a));
* the chunk is counted where a not-this-pass repair belongs, and the day-one
  "rise then return to zero" gauge is not floored.

### (3) The missing `inode` field on the `would-overwrite` row

`emit_would_overwrite` now names the object (`:1164-1176`): `inode = %object_name(object)`
beside `dserver` / `chunk` / `index`. The caller has the key from the index (`:563`). Leg 3
asserts the name is on **that row**, not merely somewhere in the log, by matching the adjacent
JSON fields: `"action":"would-overwrite","inode":"inode:8"` (`tests/…:451`).

### (4) + (6) Leg 9 is gone; the guard's oracles live inside legs 1 and 3

The v4 leg-9 fixture never built the hazard it named (its reading was complete), so it only
pinned the over-broad guard. It is deleted. The two facts that matter are now bound where the
condition they depend on actually exists:

* **leg 3** (the incomplete-reading leg) seeds a *second* under-replicated object `inode:8`
  whose rebuild claims the same server the healthy one's does, with foreign bytes already
  standing at its own `FragmentId` — asserts the bytes are unchanged, the record is not
  repointed, the obligation is still queued, the object is named on the `would-overwrite` row,
  and — the direct oracle for (2) — the backlog gauge reads **1**, not 2, though two chunks
  were repairable (`tests/…:427-468`, gauge at `:454`, name-on-row at `:455`);
* **leg 1** (the complete-reading leg) pins the converse — the stray is overwritten and the
  pass answers `Changed` with the obligation drained (`tests/…:356-377`).

The file is back to **8 legs** and inside budget (numbers in §3).

### (5) The two contradicted written contracts

**(5a) ADR-0011's counter/netting formula — RESOLVED, no ADR edit.** v4 added a *fourth*
terminal offset (`reconstruction_would_overwrite`) to a formula the ADR pins as
`repaired − conflict − aborted`. This patch adds **no** terminal offset: the withholding
happens before dispatch, so the up-front `emit_repaired` never fires for a withheld repair and
the formula is exactly as ADR-0011 documents it. The code comment that v4 rewrote is restored
to the ADR's wording, with the reason the formula still holds (`:277-285`). The
`reconstruction_would_overwrite` counter still exists as an **anomaly** counter (not a netting
offset), which is ADR-0011 §1's "durability-plane telemetry + audit stream" and not part of the
three-counter set the ADR fixes.

**(5b) `Reconciled::Blocked`'s rustdoc — NARROWED, but not fully resolvable in 2 files.
This is the one item I could not close; see §5. Please read it before signing off.**

---

## 2. Why this shape (and what I ruled out)

The hazard the round-3 sign-off ordered closed is real and narrow: while some committed object
could not be read, a repair that **claims** a new slot may land rebuilt bytes over a hidden
second reference's only shard. `put_fragment` overwrites, it runs before the CAS, and GC's
`incomplete-reference-set` backstop governs *deletes* (`gc.rs:306-316`), so nothing downstream
takes that write back.

Alternatives considered, with their costs:

| Option | Cost / why not |
|---|---|
| **Withhold every repair while the reading is incomplete** (round 3's first suggestion) | Fails the brief outright: leg 3 requires "the healthy object's repair still happens" beside a damaged record (brief §Success criterion leg 3, rule B). It also costs every healthy object its repair for one bad record — the exact failure this whole slice removes. |
| **Forbid only *claimed* slots while incomplete** (no probe) | Also fails leg 3: the healthy fixture's fragment 2 moves server 4 → 2, i.e. it *claims*. Measured: leg 3's `assert_repair_landed` (`tests/…:335-339`) goes red. |
| **v4's byte-identity guard at the write site** (`may_land`) | Two demonstrated defects (stall + gauge floor, §1(1)(2)) and it cannot be fixed in place: the byte comparison needs the rebuilt shard, which only exists *after* `erasure::reconstruct` + `encode` inside `repair_chunk` — i.e. after `emit_repaired` and after the gauge. Keeping it there and *also* diverting in `assess` costs both: the `may_land` body (`+9` semantic lines, v4 `:768-792`) plus the call-site guard (`+4`) on top of the assessment probe (`+11`) — 24 lines for one decision, in a 230-line budget I am at 219 of. **And it would be untestable**: with the selector pure, `assess`'s prediction and `repair_chunk`'s choice cannot differ, so the write-site copy is unreachable from any honest fixture → a guaranteed `--in-diff` surviving mutant (C5 was a failing gate in rounds 2 and 3). |
| **Re-route to another domain instead of withholding** | Bigger behavioural change (selector retry + placement/displacement bookkeeping, ~15 lines) and a new failure mode to reason about, for a case that only arises while the store is already NEEDS-HUMAN. Out of the brief's scope, which never touches the fragment-landing seam. |

**Chosen:** one probe, in the assessment frame, gated on `!index.complete()`
(`:557-566`), answered by `nothing_stands_at` (`:765-771`): a slot is landable only when the
store says it is **empty**. Occupied is a no whether the occupant is a benign orphan or a
hidden reference's shard — the object that would tell them apart is exactly the one the pass
could not read. A fault is a no for the same reason (unknown is not empty), as is a server
outside the fleet view. **Every no defers a repair; none discards an obligation**, and all of
it clears when the reading does.

Deliberate loss of an allowance v4 had: a *byte-identical* orphan at the target (e.g. a
previous pass's rebuild that lost its CAS) is now also withheld — but only while the reading is
incomplete, i.e. only in a store that is already refusing to certify and already carrying a
NEEDS-HUMAN row naming the record to repair. That is the fail-closed direction, and it costs 9
fewer semantic lines than the three-armed byte comparison.

---

## 3. Budgets (measured, method matches the round-4 adversary's — it reproduced v4 at 229/230)

| Budget | Cap | This patch |
|---|---|---|
| files | exactly 2 | **2** |
| `src/reconstruction.rs` added semantic lines (non-blank, non-comment) | 230 | **219** |
| `tests/segmented_map_reconstruction.rs` semantic | 380 | **380** |
| … raw | 620 | **607** |
| legs | 8 | **8** |

Where the room came from, versus v4: leg 9 deleted; the `inode:` key hoisted out of both `Site`
variants into the index map so the `Site::key()` accessor disappears (`:787`, `:812-825`); the
`hits` filter written as one closure (`:944-945`); the empty-queue rule moved to the reading's
own first line (`:865-870`); the reason strings inlined at their single call sites.

---

## 4. Forced self-refutation (required by the Do beat; all three answered with evidence)

**(a) Genuine red?** **Yes.** Production reverted (`git checkout -- crates/custodian/src/reconstruction.rs`,
test kept — exactly what `run-verify.sh:454-477` does), through the project's per-fix runner
`cargo test -p wyrd-custodian --test segmented_map_reconstruction`:

```
test result: FAILED. 1 passed; 7 failed
  a_segmented_object_ends_no_pass_and_the_flat_repair_still_happens   (Err: "find_chunk met a segmented chunk map")
  work_in_a_segmented_record_is_refused_per_object_never_discarded    (same Err)
  an_unreadable_object_is_named_and_the_walk_continues                (Err: "key must be a string at line 1 column 2")
  a_resolve_that_restarted_is_acted_on_by_nothing                     (same Err class)
  a_duplicate_committed_chunk_id_is_repaired_by_neither_reference     (Satisfied != Blocked)
  the_namespace_is_read_once_per_pass_not_once_per_obligation         (namespace reads 3 != 1)
  a_fault_that_is_not_one_objects_map_still_ends_the_pass             (rule E: no name on the seam)
```

Only leg 7 (`an_empty_queue_reads_nothing_and_certifies`) passes on the base — declared
non-red in the brief, and it is the regression guard on the restructure. With the patch applied:
`8 passed; 0 failed`. Legs go red on **behaviour**, not compilation: no leg names a symbol this
patch introduces (checked: `nothing_stands_at`, `CommittedIndex`, `Site`, `FlatSite`,
`locate_queued_chunks`, `emit_*` appear nowhere in the test file).

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point — through `Store::drive` (`tests/…:317-330`), with a real
`MemCoordination` leadership fence (`Custodian::elect` + `FencedZone::install`). The doubles are
the *store* seams (`MetadataStore`, `ChunkStore`), never a copy of the logic under test. The
withheld-landing leg reaches `assess` → `nothing_stands_at` → `ChunkStore::get_fragment` in the
production module; nothing is re-implemented in the test.

**(c) Fixture includes the fault?** **Yes.** Leg 3's store *contains* the failing elements
rather than curating them out: an undecodable record (`inode:0`), a committed root naming a
`seg:` record that was never written (`inode:00`) — and the fixture **asserts the fault is real**
against the shared resolver before the pass runs (`Store::root`, `tests/…:290-294`: a root
seeded unreadable must really come back unreadable from `metadata::resolve_chunk_map`) — plus
the occupied re-placement slot with foreign bytes actually written to the target server
(`tests/…:433`), and the healthy object beside them whose repair must still land. Both damaged
records sort **first** in the `BTreeMap`-backed store, so "the healthy object was still
repaired" is a property of the walk continuing, not of ordering luck.

Extra checks run: whole `-p wyrd-custodian` suite green with the patch (existing
`tests/reconstruction.rs` **unmodified** and green — the brief's tripwire); `cargo fmt --all --check`
clean; `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` clean; the full
`./engine/xtask.sh ci` run recorded in §6.

---

## 5. NEEDS-HUMAN — the one contract I could not close inside the brief's budget

**`Reconciled::Blocked`'s rustdoc (`crates/custodian/src/reconciliation.rs:25-28`) describes GC's
instance of the outcome, and reconstruction's is wider.** The doc reads: *"The loop ran over
everything it could read and refuses to certify the rest: at least one committed object's chunk
map could not be read (`crate::gc::ReferenceSet::unresolvable`), so the reference set the loop
reasoned over is incomplete."*

What I did about it:

* **Narrowed the behaviour by a third.** v4 answered `Blocked` over a *complete* reading for
  three reasons; the would-overwrite one is gone (it now only fires while the reading is
  incomplete, which is precisely the doc's own condition). The two that remain are
  **brief-pinned and cannot be narrowed without failing the Success criterion**: leg 2 requires
  `Blocked` for a `seg:`-resident refusal, leg 5 for a duplicate chunk id.
* **Documented the relationship at the return site** (`reconstruction.rs:337-348`), naming
  GC's and scrub's narrower instances and why a refusal is the same *claim* the enum's own
  §2 paragraph defines (`Satisfied` = silent success; `Changed` = converged what it declined
  to touch).
* **Did not edit `reconciliation.rs`** — it would be a third file, which the brief says means
  "STOP and hand back" (brief §Budget), and it is public-API rustdoc on an enum shared by four
  loops.

**Three options for you at sign-off**, cheapest first:

1. **Accept as-is** on the reading that the clause after the colon is GC's instance (it cites
   `crate::gc::ReferenceSet::unresolvable`, a type reconstruction cannot use), that each loop
   documents its own condition in its own file (`scrub.rs:72-78` does exactly this), and that
   the enum's general paragraphs already cover a refusal.
2. **Apply this one-hunk follow-up** in the same PR (it is 3 changed lines, and I have
   deliberately *not* included it in `patch.diff` so the 2-file budget is not silently broken):

   ```diff
   --- a/crates/custodian/src/reconciliation.rs
   +++ b/crates/custodian/src/reconciliation.rs
   @@ -25,4 +25,6 @@
   -    /// The loop ran over everything it could read and **refuses to certify the rest**: at
   -    /// least one committed object's chunk map could not be read
   -    /// (`crate::gc::ReferenceSet::unresolvable`), so the reference set the loop reasoned
   -    /// over is incomplete.
   +    /// The loop ran over everything it could read and **refuses to certify the rest**: it
   +    /// either could not read at least one committed object's chunk map
   +    /// (`crate::gc::ReferenceSet::unresolvable`), so the reference set it reasoned over is
   +    /// incomplete, or it read a reference it may not act on and **refused** the work rather
   +    /// than discard it (`crate::reconstruction`).
   ```
3. **Send it back to Plan** for its own slice, if you would rather the outcome contract be
   re-decided with #682 (which adds the segmented *write* path and retires the refusal).

Recorded in `review-rejected.md` as an escalation rather than a rejection, so a reviewer who
raises it lands on this note instead of re-deriving it.

---

## 6. Gate rehearsal

* `cargo test -p wyrd-custodian --test segmented_map_reconstruction` — 8/8 green with the patch;
  7/8 red with production reverted (§4a).
* `cargo test -p wyrd-custodian` — whole crate green, `tests/reconstruction.rs` untouched.
* `cargo fmt --all -- --check` — clean (this is the target's commit hook surface).
* `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` — clean.
* **`./engine/xtask.sh ci` — `xtask ci: all checks passed`, 0 failing test targets** (the full
  C4-ci gate: fmt / clippy -D warnings / build / whole-workspace test incl. DST / deny / machete /
  conformance vectors). Run twice green; the second run is over the **exact tree this
  `patch.diff` was cut from**, and included this bundle's own target:
  `Running tests/segmented_map_reconstruction.rs … running 8 tests … ok`. (Scratch logs swept
  per the Do beat's scratch discipline; C4-ci re-runs it and keeps its own.)
* `git apply --check` of this `patch.diff` against a **pristine `339da46` worktree** — clean
  (worktree created under `$PDCA_SCRATCH` and removed again).
* `./engine/scripts/run-verify.sh --classify` on this `patch.diff` →
  `ADDED_TEST crates/custodian/tests/segmented_map_reconstruction.rs` + `CRATE crates/custodian`,
  which is what earns C4-verify its red leg.

No external dependency beyond the base Rust toolchain was needed; nothing to declare.

**One environment observation for the human, not a defect in this patch:** the *first* full
`xtask ci` run wedged in `crates/server/tests/custodian_day_one.rs` — 15 tests parked in
`futex_do_wait` at 0% CPU for >5 minutes under the parallel `cargo test --workspace` run. Run
alone, that same target finishes in **0.20 s** (15/15 green), and the very next full `xtask ci`
run passed it and everything else. This matches the known target-side test deadlock the harness
already documents (`pdca.toml`: issue_635's row "hung for 19h16m on a target-side test deadlock,
getwyrd/wyrd#646"). If C4-ci ever comes back hung rather than red, that — not this patch — is
the first thing to check.

## 7. Residual risks a reviewer may raise (and my answer)

* **"The assessment probes a slot `repair_chunk` will write — could they disagree?"** No:
  `select_distinct_domains_excluding` is a pure function of `&Topology` (immutable for the whole
  pass) and its arguments (`crates/core/src/placement.rs:265-305` — a sorted `BTreeMap` walk with
  a total tie-break on `(util, id)`). The base already relies on exactly this mirroring for the
  backlog gauge. Documented at `reconstruction.rs:551-556`.
* **"A benign identical orphan now blocks a repair."** Yes, and only while the reading is
  incomplete — see §2's last paragraph. Fail-closed, deferred not discarded.
* **"`Assessment::Blocked` now has two causes; is the level misleading?"** Both are "repairable
  but nowhere to put it this pass", the level's own meaning; they are told apart on the audit
  seam by the `would-overwrite` row and its counter. Enum doc updated at `:373-385`.
* **"Seeded Tier-0 DST for the new write path."** There is no new write path: this patch's only
  new decision *withholds* a write. Recorded-rejected with the brief's Verification-posture
  reasoning in `review-rejected.md`.
