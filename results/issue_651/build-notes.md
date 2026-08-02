# build-notes.md — issue 651 (re-scoped slice 4a of 7)

`restore-and-desired-state-contained-and-attributed`, built against **`origin/main` @ `d50f0ca`**
in `$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt-l0`). Every `path:line` below is
that tree **after** the patch unless it says "base".

---

## 1. What the patch does, in one line per surface

| Surface | Before (base) | After |
|---|---|---|
| `reconcile_after_restore` report half | re-read `inode:` itself and `?`d out on `ChunkMap::Segmented` (`restore.rs:390`, `:403-405` on base) — one segmented or undecodable record ⇒ **whole pass `Err`, no report** | reads the **same `ReferenceSet`** the mark gate uses (`restore.rs:366`, `:444`); the objects it could not read are named in `RestoreReport::unresolvable` (`restore.rs:157`) and `is_clean()` is false (`restore.rs:171`) |
| `reconciliation_status` | bare `Pending` over an incomplete set (`desired_state.rs:188-190` on base) | `PendingUnresolvable { objects }` (`desired_state.rs:118`, `:231`) + the blocker named on `wyrd.custodian.drain.audit` (`desired_state.rs:258`) |
| `wyrd custodian --reconcile-after-restore` | prints "…complete", exits 0 unless dangling/misplaced (`cli.rs:1196-1236` on base) | prints `INCOMPLETE` + a NEEDS-HUMAN block naming the records (`cli.rs:1215`, `:1248`), and **exits non-zero** on them (`cli.rs:1281`) |

Also: the pass's own audit summary no longer says "complete" over a partial reading
(`restore.rs:613-630`), and the runbook's "two bills" list is now three
(`docs/design/architecture/m4-first-deployment-blueprint.md:599`, `:609-616`).

---

## 2. The one real design decision: where the report half gets its chunks

The base's `committed_chunks` did a **second walk of `inode:`**. Three options were on the table.

**(A) Feed the report half from the `ReferenceSet` the mark gate already built. ← CHOSEN.**
`gc::ReferenceSet` (base, `gc.rs:265-295`) already carries everything the report needs:
`schemes` is *exactly* the validly-placed committed chunks (with their `EcScheme`, hence `k`),
and `placed` is *exactly* their `(dserver, fragment)` pairs — both filled in the one `Ok(frags)`
branch of `gc.rs:427-442`. So `committed_chunks` becomes a **pure regroup** of that set
(`restore.rs:444-482`), no store access at all:

* it inherits the resolver for free, so a **segmented** object's chunks are judged (criterion 1);
* it inherits the containment for free, so an **unreadable** object contributes nothing and is
  named rather than raised (criterion 2);
* the two halves become *structurally incapable* of disagreeing about which chunks exist — the
  base could report as dangling precisely the object whose fragments the mark gate was
  protecting;
* it **removes** a full `inode:` scan per pass (fewer store round-trips, and the second copy of
  every record's bytes is no longer materialised). Net memory and IO go **down**, which matters
  because two of iteration-5's seven blockers were memory-blowup risks in a walk.

**(B) Keep the second walk, but resolve it through `metadata::resolve_chunk_map` with its own
containment.** Rejected on cost *and* on correctness. Cost, concretely: the contain-and-continue
block in `gc.rs:365-416` is **22 non-comment, non-blank lines** (scan → `decode` match → `resolve_chunk_map`
match → `downcast::<ChunkMapError>` → `unresolvable.insert`); reproducing it here is those 22
lines **on top of** the 40-line second walk it would keep, plus a second
`BTreeMap<Vec<u8>, String>` and a second `meta.scan(b"inode:")` per pass. (A) instead replaces
that 40-line walk with a 26-line pure regroup (`restore.rs:444-482`, base `restore.rs:390-432`)
and adds no store read at all — a **36-line** swing between the two options, all of it duplicated
logic. Correctness: two independent walks of a *live*
store can disagree, and the brief's Scope forbids it outright — "this slice adds **no**
custodian-level walk and no `crate::resolve` module".

**(C) The v5 salvage shape (`iteration-v5/patch.diff:2956-3136`) verbatim** — `resolve::homed_objects` /
`protected_fragments` / `MaintenanceWalk`. Rejected because that module is **#681's** and does
not exist on this base: pulling it in re-creates the resolving namespace walk inside this slice
(`iteration-v5/patch.diff:2717-2955` ships `crates/custodian/src/resolve.rs` as **234 added lines**, before any callsite) and
takes the patch straight past the 700-line budget. The brief names this as "the single most
likely way this bundle fails again". What was salvaged is the **shape** — the
`PendingUnresolvable { objects }` variant, the `unresolvable: Vec<String>` report field, the
`is_clean` clause, the emitter's prose — with every callsite re-pointed to
`gc::referenced_fragments` / `gc::ReferenceSet` / `gc::object_name`.

### Consequences of (A) worth the reviewer's eye

* **Verdict order changed** from store-scan order to **chunk-id order** (`BTreeMap`,
  `restore.rs:448`). Deterministic where it was not; every existing assertion in
  `restore_reconcile.rs` compares single-element vectors, so nothing depended on the old order.
* **A chunk id committed by two different objects is now judged once, not twice.** `placed` is a
  set. No caller depends on duplicate entries in `dangling` / `misplaced`, and a duplicate id is
  a corruption case in either reading. Flagged here because it is the only behavioural difference
  I could find that is not the fix itself.
* `expected.get_mut(&frag.chunk)` (`restore.rs:468`) cannot miss — `placed` and `schemes` are
  filled together — and the miss branch is the fail-safe direction anyway (leave the chunk out of
  the *report*; it stays fully protected from marking regardless). Comment says so at the line,
  because "silent skip" is a rubric defect class and I did not want it read as one.

## 3. Smaller decisions

* **Ranking in `reconciliation_status`.** `PendingUnresolvable` is checked exactly where the base
  checked `unresolvable` — *after* the genuine-reference test, *before* the malformed one
  (`desired_state.rs:224`). Deliberate and commented (`desired_state.rs:218-223`): while valid
  placements still name the server the drain is honestly converging and rebalance is moving them,
  so "wait" is true and actionable; this answer takes over at the moment that wait would
  otherwise become unbounded. It also mirrors `PendingMalformed`'s position, so the surface has
  one ranking rule rather than two.
* **A new audit seam, `wyrd.custodian.drain.audit`** (`desired_state.rs:261`) — the drain surface
  had no emitter at all. Same `action = "unresolvable-chunk-map"` and `inode = <name>` fields as
  `gc::emit_unresolvable` / `scrub::emit_unscrubbable`, so one collector query selects all three.
  Its counter counts **observations** (one per blocking record per status read), documented at
  the emitter because a status read is an operator's poll, not a pass.
* **CLI names the blockers inline, capped at 10** (`cli.rs:106`, `:1262`). A chunk id is opaque
  and belongs in the log; an `inode:` key is the thing an operator repairs. The cap keeps a store
  with a large damaged range from burying the NEEDS-HUMAN block; every record is on the audit seam
  regardless.
* **`emit_summary` no longer says "complete"** over a partial reading (`restore.rs:627`). The
  invariant is "a pass never reports a conclusion it could not reach"; that line is the one place
  an operator greps for the word.
* **`ReconciliationStatus::Pending`'s doc shrank** (`desired_state.rs:85-88`): #650 had widened it
  to cover the incomplete-set case, which now has its own variant.

## 4. Files touched (8 of 8 budget, 614 of 700 added semantic lines)

```
 22  crates/custodian/src/desired_state.rs        63  crates/custodian/src/restore.rs
 36  crates/server/src/cli.rs                    418  crates/custodian/tests/segmented_map_restore.rs  (NEW)
 52  crates/custodian/tests/restore_reconcile.rs  13  crates/custodian/tests/segmented_map_consumers.rs
  1  docs/.../06-runtime-view.md                   9  docs/.../m4-first-deployment-blueprint.md
```

Production is **121** lines against the brief's ~130 estimate; the discriminator's own fixture is
the bulk, as the brief predicted. Two files need explaining:

* **`segmented_map_consumers.rs` (#650's file) had to change.** Two of its legs assert the drain
  status over an unreadable record, and this slice changes that answer — leaving them would leave
  `C4-ci` red. They now pin `PendingUnresolvable { objects }` positively (`:722-733`, `:1100-1112`),
  which is also the "positive match on the new shape in an existing gated file" the brief asks
  for. The second one is a bonus: its three pairwise-distinct-but-lossily-colliding keys now prove
  the **answer** carries injective names, not just the audit line. Its module note is corrected
  (`:5-17`) since it claimed #651 would not need to touch it.
* **`m4-first-deployment-blueprint.md`** is not in the brief's Docs-currency line, but it is the
  operator runbook for the exact command whose exit code changed, and it enumerated "two very
  different bills". An operator meeting a third exit-1 reason with no procedure is a stale-doc
  defect; the addition is 9 lines in the existing style.

`crates/custodian/tests/rebalance.rs` was **not** touched: its 15 `ReconciliationStatus` sites are
all `assert_eq!` comparisons, not exhaustive matches, so the new variant does not break them —
which also keeps the #681 conflict surface the brief warned about at zero.

## 5. Forced self-refutation (the three questions)

**(a) Genuine red?** — **Yes, mechanically.** `./engine/scripts/run-verify.sh` (the project's own
per-fix gate) reverts `restore.rs`, `desired_state.rs`, `cli.rs` and every modified test file,
keeps the added discriminator, and runs `cargo test -p wyrd-custodian --test segmented_map_restore`:

```
0 passed; 4 failed        →  run-verify.sh: PASS — red without the fix, green with it.
  a_segmented_object_no_longer_stops_the_post_restore_pass
      panicked: … : SegmentedMapUnsupported { operation: "restore::committed_chunks" }
  an_unreadable_object_is_contained_and_the_run_is_not_certified          (same, at the `expect`)
  an_unreadable_object_does_not_starve_the_objects_the_pass_could_read    (same, at the `expect`)
  a_drain_over_an_incomplete_reference_set_names_the_blocking_record
      panicked: the blocker must be reported on wyrd.custodian.drain.audit … got: <empty>
```

All four are **assertion** reds on base-visible symbols — the file names no symbol this patch
introduces, so the reverted tree still compiles and the red is behavioural, not "a symbol is
missing". `--classify` returns exactly one `ADDED_TEST`, as the brief's dry-run predicted.

Because three of those four die at the `expect` (the base cannot get far enough to be judged), I
also ran a **mutation check** so the certification assertion is not riding on that panic. With
only `&& self.unresolvable.is_empty()` deleted from `is_clean()` (`restore.rs:171`) and everything
else intact:

```
segmented_map_restore : an_unreadable_object_is_contained_and_the_run_is_not_certified  FAILED
restore_reconcile     : an_unreadable_committed_record_is_named_and_stops_the_run_…     FAILED
(3 + 14 others still pass)
```

so the non-certification clause of criterion (2a) binds on its own. File restored and re-run green
afterwards.

**(b) Production path?** — **Yes.** Every leg calls the real
`wyrd_custodian::reconcile_after_restore` / `wyrd_custodian::desired_state::reconciliation_status`
over in-memory `MetadataStore` / `ChunkStore` **trait implementations** — the seam the loops are
built on (ADR-0010), the same doubles `restore_reconcile.rs` and `segmented_map_consumers.rs` use.
No copy, no mock of the behaviour under test, no re-implementation: the fixture supplies *storage*,
production supplies every decision. The audit assertions read JSON the **production** `tracing`
callsites emitted through a capturing subscriber.

**(c) Fixture includes the fault?** — **Yes, and it is asserted to be a real fault.**
`seed_damaged` (`segmented_map_restore.rs:342-361`) seeds a committed root naming two segments and
writes only the first, then asserts `metadata::resolve_chunk_map(...).is_err()` on the seeded
bytes — so a leg can never pass because the fault it was built around silently stopped being one.
The damaged object is `inode:1` and the store double is a `BTreeMap`, so the walk meets the
**damaged record first**: "the readable object was still reported" cannot pass by the readable
object simply having been handled earlier. Nothing is curated out — the damaged object's own
readable fragment stays on `d0` and is asserted still present and still unmarked, and criterion
(2b)'s readable object carries a *genuine* loss that must still be named.

## 6. Gates run here

| What | Command (project's own runner) | Result |
|---|---|---|
| Whole Wyrd gate | `./engine/xtask.sh ci` (fmt, clippy `-D warnings`, build, test incl. DST, deny, conformance, **typos**, docs render) | `xtask ci: all checks passed` (exit 0) |
| Per-fix red→green | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | `PASS — red without the fix, green with it` |
| Commit-hook readiness | `cargo fmt --all` clean; `typos` clean (it caught one word — "Pendings" in a test section header — now reworded) | ✓ |

`typos` and `docs-renderer` (the brief's two external dependencies) are **both installed on this
host**, so the prose gates actually ran rather than warn-and-skipping. No NEEDS-HUMAN external
dependency to declare.

## 7. What I deliberately did NOT do

* No `crate::resolve`, no custodian-level namespace walk, no `repoint_chunk`, no record ceilings,
  no reconstruction / backfill / rebalance change — #681 / #682, per Scope.
* No DST leg (brief: none in this slice), no conformance-vector change, no ADR/spec/proposal edit.
* No change to `crates/server/src/custodian.rs`: it passes the `RestoreReport` straight through and
  the new field reaches the CLI without it.
* Did **not** re-rank `PendingUnresolvable` above the genuine-reference `Pending` (see §3) — that
  would have changed an answer the base already gives correctly, for no operator benefit.
