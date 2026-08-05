# build-notes — issue 651 (iteration 12) — *withheld from the reviewer*

Slice 4a of 7 of the #635 re-slicing: **the two surfaces that report whether a reconciliation is
complete answer contained and attributed over a reference set with a hole in it.**

This is a **rebuild of iteration 11 on the same brief**, not a new design. v11 was green on every
gate except T4 and was iterated on the sign-off's three carry-forward items. What changed, and
why, is section 1. Everything else in the patch is v11's content (which the brief itself
sanctioned: § Citations expected § Salvage), re-verified here.

---

## 1. The carry-forward, item by item

### (1) T3/T5, blocking — the attribution-order gap. **FIXED.**

**The defect.** v11 read the committed reference set, then `orphan_leases`, then
`pending_chunks`, then `committed_chunks`, and only then computed the union of unresolvable
records and emitted each on the audit seam. So a store fault in any of those three intervening
reads — a partitioned FDB, a backend blip, anything unrelated to the damaged record — returned
`Err` from the `?` and the name of the object the pass had **already** identified as unreadable
died with it. The operator gets an error naming nothing, and the record keeps blocking every
future pass: exactly the "stall nothing exits" C-1 forbids, reached through the report.

The brief cited `gc.rs:155-165` as the placement to mirror — *emit per object, before the fleet
walk, so a later transient fault cannot cost the operator the record's name* — and v11 mirrored
the **loop** but not the **position**.

**The fix** (`crates/custodian/src/restore.rs:261-278` on the patched tree):

```rust
let referenced = referenced_fragments(ctx.meta).await?;
let mut unreadable = BTreeSet::new();
attribute_unresolvable(&referenced.unresolvable, &mut unreadable);   // ← emitted HERE now
let already = orphan_leases(ctx.meta).await?;
let pending = pending_chunks(ctx.meta).await?;
let committed = committed_chunks(ctx.meta).await?;
attribute_unresolvable(&committed.unresolvable, &mut unreadable);    // ← and HERE
```

`attribute_unresolvable` (`restore.rs:497`) changed shape with it: it is now
`(&BTreeMap<Vec<u8>, String>, &mut BTreeSet<Vec<u8>>)` — called **once per read, the moment that
read returns** — instead of v11's `(&ReferenceSet, &CommittedChunks) -> Vec<String>` which could
only be called after both. The `named` set gives dedup (a record both reads met is emitted and
counted once) and, being a `BTreeSet` of the store's own key bytes, still yields the report's
`unresolvable` list in key order. `ReferenceSet` is no longer imported by `restore.rs`.

**The regression** — `a_record_already_known_unreadable_is_named_before_a_later_read_can_fail`
(`crates/custodian/tests/segmented_map_restore.rs:638`), in the **discriminator**, so it is part
of the red→green leg rather than only the whole-tree gate. It seeds the damaged object, then
fails one intervening ledger scan (`orphan:` on the first pass through the loop, `pending:` on
the second) with a plain, non-`ChunkMapError` store fault, and asserts:

* the pass still ends in `Err` — a store fault is not one object's, so it must propagate; and
* the failure is the **injected** fault (`STORE_FAULT` text), so the leg cannot pass on some
  other error; and
* the audit seam **already carries** `action=unresolvable-chunk-map` naming `inode:1`.

It needs no symbol this patch introduces, so it is assertion-red on the base (verified below),
not compile-red.

Fault injection is a `failing: Mutex<Option<Vec<u8>>>` prefix on the existing `MemMeta` double
(`segmented_map_restore.rs:90-94`, `:116-120`) rather than a wrapper store: ~10 lines against
~25 for a `PoisonedMeta`-style delegating wrapper (`segmented_map_consumers.rs:187-217` is the
peer shape), and the budget below had no room for the wrapper.

**Not covered, deliberately:** attribution *after* the second read is proven by the same two
cases (both ledgers sit between the two reads). A leg that fails the **fleet walk** after the
second read met a record only it could read would additionally pin read-2's placement ahead of
`list_fragments`; it needs a failing `ChunkStore` double (~20 lines) and the budget (§3) had no
room. The code path is one line away from the tested one (`restore.rs:278`).

### (2) T4 gate, blocking, mechanical — the timeout finding at `restore.rs:521`. **RE-FILED.**

The finding is the caller-side-await-timeout class already rejected 3× under #508/#636 (*the
`MetadataStore` implementation owns its own network bound, not the caller* —
`crates/traits/src/lib.rs:1000-1012`). It re-landed only because `committed_chunks` moved and
the rejection rows in `review-rejected.md` were keyed to the old line numbers.

`results/issue_651/review-rejected.md` § *Standing rejections* is re-filed against **this**
patch's lines — the resolve await (`restore.rs:543`), the `inode:` scan it sits in (`:524`), the
moved `committed_chunks` call (`:277`), the three unchanged calls (`:261`, `:271`, `:272`) and
`desired_state.rs:188` — each under **both** classes a reviewer files this as (`BUG` and
`CONVENTION`) and both match phrases the rubric sentence uses (`timeout`, `bounded`). The gate
matches on `(loc, CLASS)` + a substring of the finding's rationale
(`scripts/review-branch:248-253`), so a row per (line, class, phrase) is what makes the standing
rejection survive a rebuild. The file's header now also says which of its historical notes
describe the dropped ambiguity apparatus, so a reader does not chase line numbers into code that
no longer exists.

### (3) Validation / fitness-to-purpose NEEDS-HUMAN — untouched, as instructed.

Not a rebuild item per the carry-forward; it returns to the human at sign-off. §5 below lists
what a human can check by hand.

---

## 2. What the patch does (unchanged from v11 in substance)

* **`restore.rs`** — the report half re-reads the committed namespace through the shared resolver
  and **contains** what it cannot read (`committed_chunks`, `restore.rs:508-585`), by exactly
  `gc::referenced_fragments`'s downcast rule: `Ok(ChunkMapError)` is *this record's* fault and is
  recorded; any other error propagates because a store fault is not one object's
  (`gc.rs:405-415`). The names land in `RestoreReport::unresolvable`, `is_clean()` is written in
  terms of the new `needs_human()` so the two cannot drift, and the mark gate withholds the whole
  fleet while **either** read found a hole (`restore.rs:326`) — one reading, one conclusion.
* **`desired_state.rs`** — `reconciliation_status` answers `PendingUnresolvable { objects }`
  instead of a bare `Pending`, ranked below the genuine-reference check and above the malformed
  one, matching the base's existing order (a Plan decision, not revisited).
* **`cli.rs`** — one `restore_verdict(&report)` produces the printed lines *and* the exit status,
  and the status **is** `report.needs_human()` rather than a second `||` chain, so a finding
  added to the report reaches the exit code by definition.
* **docs** — `06-runtime-view.md:31` extended to state the restore pass's and the drain surface's
  version of the invariant; the m4 runbook's `--reconcile-after-restore` block gains the
  UNREADABLE outcome and the non-zero exit, with every claim derived from a field the report
  actually carries (the v10 T2 finding).

## 3. Budget — measured, not asserted

Brief: **≤ 700 added semantic lines (non-blank, non-comment, non-mechanical), ≤ 8 files.**

Files: **8** — exactly the eight the brief names. No ninth.

Lines, by `git diff --cached` over added lines with blanks, comments (`//`, `/*`, `*`, `<!--`,
and `#`-prefixed prose in the runbook's shell block), string-literal continuation lines and
bare-delimiter closers removed:

| file | semantic added |
|---|---|
| `crates/custodian/src/desired_state.rs` | 21 |
| `crates/custodian/src/restore.rs` | 72 |
| `crates/custodian/tests/restore_reconcile.rs` | 63 |
| `crates/custodian/tests/segmented_map_consumers.rs` | 9 |
| `crates/custodian/tests/segmented_map_restore.rs` (new) | 424 |
| `crates/server/src/cli.rs` | 109 |
| `docs/…/06-runtime-view.md` | 1 |
| `docs/…/m4-first-deployment-blueprint.md` | 0 |
| **total** | **699** |

That measure still counts rustfmt's one-argument-per-line splits, which are mechanical by the
brief's own wording; the statement-level count (added lines ending in `;` or `{`) is **324**.

v11's content measured **700** on the identical filter, so the mandated regression (§1) was
**funded by pruning**, not added on top. What was pruned, and why each is not a property loss:

| pruned | lines | covered instead by |
|---|---|---|
| `restore_reconcile.rs`: `dangling/misplaced/under_replicated` all-empty assert | 4 | the discriminator's (2a) asserts the same over the same shape, base-visibly |
| `restore_reconcile.rs`: `!report.needs_human()` true-branch | 2 | `cli.rs`'s `routine` case pins `needs_human == false` |
| `segmented_map_restore.rs`: `inode_scans() >= 1` fixture check + accessor | 6 | the decode check two lines below is the load-bearing one (it proves the decay armed); scan *count* is the implementation's choice and must not be asserted |
| `segmented_map_restore.rs`: (2a)'s damaged-fragment `is_marked_collectable` | 4 | (2b) asserts it on the same seeded object, plus `get_fragment` still present |
| `segmented_map_restore.rs`: `seed_damaged`'s chunk-id parameters → consts | 4 | each leg has its own store; one pair of ids serves them all |
| `cli.rs`: the third `needs_human` agreement assert | 4 | folded into the first as a tuple compare — same binding, one assert |
| m4 runbook: UNREADABLE paragraph tightened | 2 | prose only; every claim it makes is still derived from a report field |

I did **not** prune `restore_reconcile.rs`'s decaying `inode:3` half even though it is the
largest remaining candidate: it is the only leg that binds the **second** read's containment.
Delete the `attribute_unresolvable(&committed.unresolvable, …)` call and the discriminator's
(2c) leg still passes (it is deliberately implementation-neutral: a single-reading pass marks
the stray and names nothing, which satisfies the conjunction) — only
`every_unreadable_committed_record_is_named_and_stops_the_run_being_certified`'s
`report.unresolvable == ["inode:2", "inode:3"]` catches it. That is a mutation the suite must
kill, so the leg stays.

## 4. Alternatives considered and rejected

* **Read the committed namespace ONCE.** The brief calls this "the better implementation" for
  criterion (2c), and it is — but it costs `gc.rs`, which the brief puts out of scope
  (§ Scope: "`gc.rs` and `scrub.rs` untouched"). Either `referenced_fragments` grows a second
  return value (per-chunk `Expected`) — a signature change in the builder GC, scrub, restore and
  the drain surface all share — or restore rebuilds the reference set locally, duplicating
  gc's classification, which is #681's shared-walk work by name. Concretely: the first is ~40
  lines across `gc.rs` + 4 call sites, the second ~60 lines of duplicated walk in `restore.rs`
  plus a second copy of the malformed/`checked_fragments` classification. The brief's criterion
  (2c) explicitly accepts "reads twice and withholds marks while either reading found a hole",
  which is what ships, and `deferred: #681` marks the unification at `restore.rs:516`.
* **A `PoisonedMeta`-style wrapper store for the new leg** (the peer shape,
  `segmented_map_consumers.rs:187-217`): ~25 lines for the wrapper + 4 trait methods, versus ~10
  for a prefix field on the existing double. Same fault, same production path; the wrapper only
  buys separation this file does not need (it has one double).
* **Re-raising the timeout finding as a fix** (wrap the resolve await in `tokio::time::timeout`):
  rejected 3× already, and it would put a runtime dependency in a crate whose seam boundary is
  traits/core/tracing (ADR-0010). Re-filed as a rejection, per the carry-forward's own
  instruction.
* **Emitting attribution from inside `referenced_fragments`** (so every consumer gets it for
  free): rejected for the reason `gc.rs:161-163` already states — the shared builder backs GC,
  scrub, restore and the drain query, so a `restore_` counter ticked inside it would report a
  restore pass that never ran. Attribution is the consumer's, per object.

## 5. Verification

**Red→green, through the project's own runner** (`./engine/scripts/run-verify.sh`, the
configured C4-verify gate; it applies `patch.diff` to a clean worktree off `origin/main`):

```
run-verify.sh: PASS — red without the fix, green with it.
```

The RED leg (production reverted, the added test kept) failed **all five** legs on *assertions
and returned values*, never a missing symbol:

```
a_segmented_object_no_longer_stops_the_post_restore_pass       … SegmentedMapUnsupported
an_unreadable_object_is_contained_and_neither_surface_certifies_it … SegmentedMapUnsupported
an_unreadable_object_does_not_starve_the_objects_the_pass_could_read … SegmentedMapUnsupported
marks_and_report_rest_on_one_reading                            … Error("expected ident")
a_record_already_known_unreadable_is_named_before_a_later_read_can_fail
    … "must classify the blocker as an unreadable chunk map AND name inode:1 … got: "
```

**Whole tree:** `./engine/xtask.sh ci` → `xtask ci: all checks passed` (fmt, clippy `-D
warnings`, build, test, deny, conformance, typos, statics). The patch is commit-ready for the
target's own hooks — `cargo fmt --all -- --check` is clean, including after the last edit.

### The three forced questions

**(a) Genuine red?** Yes, and twice over. The whole patch: the run-verify RED leg above, five
assertion reds. The *specific* carry-forward fix, isolated: I reverted only the attribution-order
change — moved `attribute_unresolvable(&referenced.unresolvable, …)` back down below
`committed_chunks`, i.e. exactly v11's shape, leaving everything else in place — and re-ran the
discriminator:

```
test a_record_already_known_unreadable_is_named_before_a_later_read_can_fail ... FAILED
  panicked at crates/custodian/tests/segmented_map_restore.rs:283:5:
  wyrd.custodian.restore.audit must classify the blocker as an unreadable chunk map AND
  name inode:1 … got:            ← the capture is EMPTY: nothing was attributed
4 passed; 1 failed
```

so the new leg binds the fix and nothing else in the file does. Restored, re-run, 5 passed.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_after_restore` and
`wyrd_custodian::desired_state::reconciliation_status` — the exported production entry points —
over the real `MetadataStore` / `ChunkStore` traits. The doubles are *stores*, not stand-ins for
the pass: the resolver under test is the production `metadata::resolve_chunk_map`, the reference
set is the production `gc::referenced_fragments`, the audit assertions read the bytes the
production `tracing` callsites emitted. The CLI verdict test calls the production
`restore_verdict` the command itself calls.

**(c) Fixture includes the fault?** Yes, and each fixture asserts its own fault is real before
relying on it:
* `seed_damaged` asserts `metadata::resolve_chunk_map` genuinely returns `Err` on the seeded root
  (`segmented_map_restore.rs:428-436`) — the leg cannot pass because the damage silently stopped
  being damage;
* the one-reading leg asserts a post-pass scan really fails to decode, and runs a **control**
  store first proving the stray *is* marked under a complete reading (so "nothing was marked" is
  a refusal, not a no-op);
* the new attribution-order leg asserts the pass failed on the **injected** `STORE_FAULT` text,
  not on any error;
* nothing is curated out: (2a) keeps the healthy object beside the damaged one so the incomplete
  reading is the *sole* cause of non-certification, and (2b) keeps a genuinely-lost chunk beside
  it so containment is proven by the loss still being reported.

**Not machine-checked, for the human at sign-off** (this is the standing validation
NEEDS-HUMAN, unchanged): that the operator-facing wording is the wording an operator wants —
the summary line's `INCOMPLETE`, the third NEEDS-HUMAN paragraph, and the runbook's UNREADABLE
bullet. `cargo xtask ci` proves they are consistent with the report's fields; whether they are
*useful at 3am* is a human judgement. `PendingUnresolvable`'s name and the shape of its
`objects: Vec<String>` are likewise a maintainer's call — the alternative (`Vec<Vec<u8>>`, the
raw keys) keeps the bytes but makes every consumer render them, and #650 already chose
`object_name`'s escaped rendering for the audit seam.

## 6. Scope discipline

No cross-object chunk-id ambiguity apparatus anywhere: no claim counting, no `ambiguous-*` event,
no mark-withholding keyed on a shared id — grep the patch for `ambiguous` and `claims`, both
absent. `gc.rs`, `scrub.rs`, `rebalance.rs`, `reconstruction.rs` and `core/src/metadata.rs`
untouched. No new module, no new CLI subcommand, no report-schema widening of
`dangling`/`misplaced`/`under_replicated`, nothing written to a chunk map. No new dependency
(the patch adds `std::collections::BTreeSet` to an existing `use`).

## 7. Rubric self-review (target `AGENTS.md` § Review rubric & protocol)

* *One clock per lifecycle* — no clock read added; `now_millis` stays the caller's.
* *Narrow trait seams / dependency direction* — custodian still sees only traits/core/tracing.
* *Metadata validation boundaries* — decode failures surface as errors and are contained per
  record, strictly, in a maintenance path.
* *No DST-reachable shared mutable global state* — none added (the tests' `Once` tracing default
  is the in-tree pattern for #214 and is test-only); the statics gate is green.
* *`#![forbid(unsafe_code)]`* — present on the new test file.
* *Docs currency* — the two living architecture docs updated in the same patch.
* *Absent or unsupported entries* — an unreadable record is never a silent skip: it is recorded,
  named on the seam, carried in the report, and blocks certification and marking.
* *Await discipline* — no new unbounded await beyond the base's own rule; the rule is stated at
  the call site (`restore.rs:503-508`) and the standing rejection re-filed.
* *Test fidelity* — doubles mirror the seam; the new leg injects a real store fault rather than
  simulating its consequence.
