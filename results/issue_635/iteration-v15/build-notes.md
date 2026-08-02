# Build notes — issue #635 (segmented-chunk-map), iteration 15

**Withheld from the reviewer by the driver; written for the human at sign-off.**

Base: `origin/main` @ `9120f7a` (carries #634's `scan_page`). Worktree:
`$PDCA_WORKTREE = /home/eddie/development/wyrd/wyrd.pdca-wt-l0`. No `$PDCA_BASE` /
`$PDCA_VERIFY_BASE` in the environment and no `stack-base` file in the bundle — checked
first, per the brief's `Falsifiability` 2. `run-verify.sh --print-base` resolution is the
brief's own field; `--classify` returns exactly one `ADDED_TEST`
(`crates/custodian/tests/segmented_map_consumers.rs`), as the brief predicted. **No
`Cargo.toml` was modified** (`git diff --name-only | grep -c Cargo.toml` ⇒ 0).

## 1. What this iteration changed, and why only this

Iteration 14 was rejected on the **T4 batch-review gate** with three blocking findings, all
one class, and the sign-off gave an explicit directive: *"apply the same containment pattern
already used elsewhere in this bundle (e.g. `gc.rs`'s `referenced_fragments` /
`ReferenceSet::unresolvable` handling) to these three call sites: contain the per-object
`ChunkMapError`, attribute it, and continue the pass for unaffected objects — rather than
propagating it and aborting the whole pass."*

So this iteration is **the round-14 patch plus that fix**, not a rebuild: the record shape,
the settled encoding, the staged-publication committer, the shared resolver and every
earlier round's fix are carried forward unchanged (`iteration-v14/patch.diff` applied first,
then edited). Re-deriving them would have thrown away 14 rounds of settled review.

### The defect, precisely

Six loops walk `scan("inode:")` and decode every record. Five of them then resolve the map
through `resolve::chunks_of` / `homes_of`. On the round-14 patch, only `gc.rs` contained a
per-object `ChunkMapError`; the others used `?`, so **one damaged object ended the whole
pass**:

| Site | What one damaged object cost |
|---|---|
| `backfill.rs:109` (+ `:90`, and the gauge walk at `:245`,`:252`) | every healthy pre-M3 record stayed un-drained, and `backfill_placement_remaining` — the gauge the drain is watched by — was never emitted |
| `rebalance.rs:153`,`:168` | evacuation planning stopped for **every** object, so a decommission of a server the damaged object never touched could not progress |
| `reconstruction.rs:607`,`:615` | the store-wide lookup for a queued repair aborted, starving every healthy under-replicated chunk whose object sorts after the damaged one |
| `restore.rs:376`,`:383` (not in the batch; same pattern) | the post-restore audit returned nothing at all — no dangling, misplaced or under-replicated verdict — for the whole store |

### The fix, at the boundary rather than per call site

Round 9's directive was *"fix at the foundation, not by whack-a-mole per call site … a third
call site with the same pattern must not become a future review round."* So the
classification is now **one function**, `resolve::contain`
(`crates/custodian/src/resolve.rs:195`), and it holds the crate's **only**
`downcast_ref::<ChunkMapError>()` (`:198`) — `gc.rs`'s two arms were rewritten onto it and
its private duplicate (`contained` / `emit_unresolvable`) deleted. Every site now reads:

```rust
match crate::resolve::contain(&key, metadata::decode(&value))? {
    Contained::Resolved(record) => record,
    Contained::Unresolvable(fault) => { …attribute + skip… }
}
```

`ChunkMapFault::attribute(pass)` consumes the fault, so a blocker cannot be recorded without
an operator signal, nor emitted twice. The audit event moved from `gc`'s private
`gc_unresolvable_chunk_map` to a shared `custodian_unresolvable_chunk_map` carrying a `pass`
field (no test or doc referenced the old name — checked).

**What each pass then does** (each refuses to certify, none deletes/moves/rewrites anything
of the object it could not read):

* **backfill** — attributes, skips, fills every other record, emits both gauges (the new
  `backfill_unreadable_records` is the qualifier that keeps `backfill_placement_remaining`
  from reading as complete), then raises the population **once at the end** as the typed
  `UnresolvableChunkMaps`. Raised rather than silent because backfill's only other report is
  a *count*, and a population that silently excludes a record is the count-based reassurance
  `AGENTS.md:175-177` forbids. The ordering (work → gauges → diagnostic) is the one the
  sibling `SegmentedPlacementUnfillable` already used in the same function, so the pass has
  one rule, not two.
* **rebalance** — attributes, skips, evacuates every plan it could build, emits
  `rebalance_unresolvable_records`. It deliberately does **not** raise: the drain's
  certification surface is `reconciliation_status`, which answers `PendingUnresolvable` and
  names every blocking object (`desired_state.rs:184-195`), so progress for the readable
  store can never be mistaken for certification over the unreadable part.
* **reconstruction** — `find_chunk` now answers a three-way `Found`
  (`reconstruction.rs:617`): a chunk found in a readable object wins over any blind spot;
  `Absent` (complete scan) still drains the obligation; `Unresolvable` maps to the new
  `Assessment::Unresolvable`, which **keeps the obligation queued** — the rubric's
  "enqueue a repair obligation" arm. This was the subtle one: containment alone would have
  made a chunk that lives in the damaged map look deleted, and `Assessment::Drain` *deletes
  the repair record*. That would have retired the repair of a still-under-replicated chunk —
  strictly worse than the abort the finding complained about.
* **restore** — attributes, skips, finishes the audit, and reports the objects in the new
  `RestoreReport::unresolvable`; `is_clean()` is false while one is present. Marking is
  unaffected: `ReferenceSet::protects` already holds every fragment off-limits while the set
  is incomplete, so `stranded_marked` stays 0.

One determinism polish went in with them: the blocker lists are **ordered and
deduplicated** — backfill collects into a `BTreeSet` (the same shape
`ReferenceSet::unresolvable` uses) and `restore`'s report is sorted before it is returned.
`MetadataStore::scan` leaves order unspecified (`crates/traits/src/lib.rs:1021-1023`), so a
diagnostic built from scan order would name a different record on each pass over an
unchanged store, and an operator cannot diff that between runs.

### Where the brief's containment table lands on this

The table permits a deletion-capable pass to abort *or* to continue while protecting. It
requires "no fragment may be deleted", which held before and holds now. What it does **not**
license is one object's fault removing every other object's *maintenance* — the
"Invariant to restore" says the failure must be "scoped to the object that failed" — and
that is what this round restores for the four passes that were still uniform.

## 2. Alternatives considered, with their cost

* **Leave rebalance/reconstruction aborting, and record-reject the findings** as
  pre-authorised by the containment table. Cost is not diff size, it is the invariant: the
  brief's own SELF-TEST says containment "cannot be satisfied by guarding one module", and a
  decommission that can never finish because one unrelated object is corrupt is exactly the
  blast radius iteration 5 was rejected for. Also directly contrary to the round-14 sign-off
  directive. Rejected.
* **Make backfill return `Ok` and rely on telemetry alone.** 2 lines smaller (`if let
  Some(first) = unresolvable.first() { return Err(…) }` plus the error type's 25 lines).
  Rejected: backfill's only report is a count, and `AGENTS.md:175-177` forbids a
  count-based reassurance that can pass while the property fails. Rebalance and
  reconstruction *do* get the `Ok` treatment, because each has a non-count surface that
  refuses (drain status; the queued obligation).
* **Have `find_chunk` keep returning `Option` and treat "unresolvable" as "absent".** Zero
  extra lines. Rejected: `Assessment::Drain` deletes the repair record
  (`reconstruction.rs:273-279`), so this silently retires repairs for chunks in the damaged
  map — data-durability loss, and the exact "silent skip" class. The three-way `Found` costs
  ~20 lines and is the cheapest honest shape.
* **Contain inside `chunks_of`/`homes_of` (return `None` on a chunk-map fault).** ~15 lines
  smaller than the `Contained` enum at the call sites. Rejected on safety: `None` already
  means "no live committed generation", which GC and restore read as *"this object owns no
  fragments"* — collapsing an unreadable map into it would put a live object's fragments in
  nobody's reference set, the recorded #508-attempt-4 failure in a new spelling. The two
  conditions must stay distinguishable at every call site.
* **A `.unwrap_or_default()`-style skip in each pass.** Smallest possible. Rejected: an
  unattributed skip is invisible damage, and it is the shape the rubric names first.

## 3. Red → green evidence

**(a) Genuine red? YES — proven twice, at both meanings of "reverted".**

1. *This round's fix reverted* (the informative one): the four pass files were restored to
   their round-14 bodies from `iteration-v14/patch.diff` (backup/restore under
   `$PDCA_SCRATCH/pdca-builder-635-redleg`, removed afterwards) with the new tests in place.
   Result: **3 failed, 9 passed** — exactly the three new legs, with the defect named in the
   failure text:
   * `backfill_fills_a_healthy_record_while_one_object_cannot_be_read` — the healthy record
     still carried `"placement":[]`;
   * `rebalance_evacuates_a_healthy_object_while_another_cannot_be_read` —
     `Store(SegmentAbsent { nonce: "fedcba…", epoch: 11, index: 1 })`;
   * `reconstruction_repairs_a_healthy_chunk_and_keeps_an_unreadable_objects_obligation` —
     same `SegmentAbsent` out of the store-wide lookup.
   The two typed sibling tests (`tests/backfill.rs`, `tests/restore_reconcile.rs`) do not
   compile against the round-14 bodies (`UnresolvableChunkMaps` /
   `RestoreReport::unresolvable` do not exist there), which is why the *binding* red is
   carried by the added test file, whose symbol set is base-only by construction.
2. *The whole slice reverted* (the gate's meaning): `./engine/scripts/run-verify.sh` with
   `$PDCA_BUNDLE` set ⇒ **PASS — red without the fix, green with it**. RED leg:
   **12 tests ran, 12 failed, every one an assertion panic** (`panicked at
   crates/custodian/tests/segmented_map_consumers.rs:<line>`), **not** a build error — the
   brief's `Falsifiability` 3 requirement. GREEN leg: 12 passed, 0 failed.

**(b) Production path? YES.** Every leg drives the real entry points — `backfill::reconcile`,
`reconcile_step` → `rebalance::reconcile` / `reconstruction::reconcile`,
`reconcile_after_restore`, `reconciliation_status` — over in-memory `MetadataStore` /
`ChunkStore` **trait doubles** (the seams the loops are defined over, the shape
`crates/custodian/tests/gc.rs:26-120` uses; every double implements `scan_page` by
delegating to `wyrd_testkit::test_double_scan_page`). No pass logic is re-implemented in the
tests; the fixtures are raw record bytes and real erasure-coded fragments.

**(c) Fixture includes the fault? YES.** The damaged object is *in the same store* as the
healthy ones in every leg (`seed_damaged`: a committed segmented root naming two segments
whose second `seg:` record was never written — plus, in the leg-7 test, two decode-damaged
spellings). Nothing is curated out: the backfill leg asserts the healthy record was filled
*while* the damaged one sits in the scan; the rebalance leg asserts the healthy drain
converges *and* that a drain of a server the damaged object sits on is still not
`Satisfied`; the reconstruction leg asserts the healthy repair completed *and* that the
damaged object's queued obligation survived and its fragments were untouched.

**Gates**, run through the project's own runners (never a hand-rolled command):

* `./engine/xtask.sh ci` ⇒ **exit 0, "all checks passed"** — fmt, clippy `-D warnings`,
  build, the whole test suite, `cargo deny`, conformance, and the prose gates (`typos` ran:
  it is line 2 of the log; the doc renderer is installed on this host, so the
  `docs/design/architecture/06-runtime-view.md` edit is gated for real, not warn-skipped).
* `./engine/scripts/run-verify.sh` ⇒ PASS (above). Re-run after the final edit; both runs
  agree.

## 4. Docs currency

`docs/design/architecture/06-runtime-view.md` §6.2 gains the containment clause for the
walking passes (the drain still drains, the decommission still evacuates, the queued repair
is still assessed, nothing of the unreadable object is filled/moved/reclaimed, the obligation
stays queued). That is a **merge requirement** (`AGENTS.md:154-157`), not a follow-up: this
round changes observable maintenance behaviour. The new telemetry names are not added to the
docs because no architecture doc enumerates metric names (checked).

## 5. Triage bookkeeping

`review-rejected.md` gains a **Round 15** section: all three round-14 findings are recorded
as **fixed** (with the fix and its test), plus a note that two unnamed sites of the same
pattern (`restore.rs`'s audit walk, and `gc.rs`'s private duplicate of the classifier) were
folded in so a third round of the same finding cannot occur. **Nothing was declined this
round** — every prior round's declines stand unchanged in the same file.

## 6. Standing NEEDS-HUMAN items (carried, not resolved here)

Unchanged from iteration 14 and still for the human at sign-off — none is an implementation
gap this round could close:

1. **T3 / `Open questions` 4** — landing a `Completing`-less precursor committer before #636
   supplies the real session fence. The brief takes this position; it is a maintainer confirm.
2. **Validation fitness-to-purpose** — no production path publishes a segmented map until
   #636, which the brief states is correct (`Production reach`); the leg-A fixtures are raw
   record bytes by design.
3. **C5 advisory mutants** — the previous round reported ~12 missed mutants on a ~500-mutant
   in-diff run; not re-run here (advisory, and multi-tens-of-minutes). The three new
   containment arms are each covered by a red→green test, so the delta this round is
   mutation-tested in the only sense that matters at Do.
4. **The "one damaged object stalls fleet-wide GC/drain" re-affirmation** — GC and restore
   still refuse to reclaim anything while any object is unresolvable (the brief's containment
   table pre-authorises exactly this). This round narrows the *other* four passes; it does
   not reopen that decision.

No external dependency was missing: `typos` and the doc renderer are both installed and both
ran, so there is no `NEEDS-HUMAN external dependency` marker to raise.

## 7. Scratch hygiene

Everything throwaway lived under `$PDCA_SCRATCH` (`/var/tmp/pdca`): the backup dir
`pdca-builder-635-redleg/` used for the refutation, and the gate logs
`pdca-builder-635-{ci..ci4,verify..verify4}.log`. **All removed** before handover (`ls
$PDCA_SCRATCH | grep pdca-builder-635` ⇒ nothing). Nothing was written to `/tmp`, and no
clone or build cache was created outside the driver's own worktree.
