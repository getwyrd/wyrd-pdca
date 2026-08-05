# Build notes — issue 651 / restore-and-drain-report-contained-and-attributed

Target branch: `getwyrd/wyrd @ main` (`d50f0ca`). All work done in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`), which is clean at that commit; every `path:line` below is
that worktree **with this patch applied**.

## What I built, and why this shape

The brief is a composition slice: #650 already shipped the containment vocabulary
(`gc::ReferenceSet::{unresolvable, protects, protection}`, `gc::object_name`,
`metadata::resolve_chunk_map`) and deferred the two *reporting* surfaces here by name in its own
code (`restore.rs:196` and `desired_state.rs:183-187` on the base). So the work was to make those
two surfaces answer **contained and attributed** rather than erroring or certifying, mirroring the
peer callsites the brief named rather than inventing a shape.

### 1. `crates/custodian/src/restore.rs` — the report half survives what it cannot read

* `committed_chunks` (`restore.rs:508`) now decodes and resolves each committed record through the
  **shared** resolver and contains a per-object fault instead of `?`-ing out. It copies
  `gc::referenced_fragments`'s rule verbatim, including the **downcast rule** the brief cited
  (`gc.rs:405-415`): `Ok(ChunkMapError)` is *this record's* fault (recorded in
  `CommittedChunks::unresolvable`, walk continues); any other error propagates, because a store
  fault is not one object's. The base's `as_flat().ok_or(SegmentedMapUnsupported)` is gone —
  that single line is what turned one segmented object into `Err` for the whole store.
* `RestoreReport::unresolvable: Vec<String>` (`restore.rs:156`) carries the blockers by `inode:`
  key as the store spells it, escaped through `gc::object_name` (injective — two damaged records
  never collapse onto one name).
* `is_clean()` is rewritten **in terms of** a new `needs_human()` (`restore.rs:171,185`) so the two
  cannot drift as fields are added; `needs_human()` is the predicate the CLI turns into its exit
  status.
* `attribute_unresolvable` (`restore.rs:589`) emits the audit line per object **before the fleet
  walk**, mirroring the placement the brief called out at `gc.rs:155-165` — a transient store fault
  later in the pass cannot cost the operator the name of the record to repair. The names are the
  **union** of both reads, deduplicated and ordered by the store's own key bytes.
* `emit_summary` (`restore.rs:726,744`) says `INCOMPLETE` rather than `complete` while any record
  was unreadable, and carries `unresolvable`, `clean`, `needs_human` — so the one line an operator
  greps states the same verdict the report offers its callers.

### 2. The **one-reading** rule (criterion 2c) — the only real design decision here

The brief's `Out of scope` forbids a custodian-level walk (“reading the reference set through
`gc::referenced_fragments` exactly as the base does”) and leaves `gc.rs` untouched. That fixes the
pass at **two** reads of `inode:`: one inside `referenced_fragments` (the mark gate) and one inside
`committed_chunks` (the report). Two reads that can disagree are exactly what criterion (2c) is
about.

Of the two implementations the brief calls honest, only one was available under those constraints:

* *single reading* — would need restore to own the walk, or `gc.rs` to hand back per-reference
  expectations. Both are explicitly out of scope, so **ruled out by the brief, not by cost**.
* *two readings, withhold marks while **either** found a hole* — what I built:
  `committed_chunks` is read up front (`restore.rs:266`), the report's names are the union
  (`restore.rs:272`), `incomplete` is derived from that union (`restore.rs:279`), and the mark gate
  becomes `if incomplete || referenced.protects(..)` (`restore.rs:316`).

Consequence worth noting at sign-off: `stranded_marked > 0 && !unresolvable.is_empty()` is now
**unreachable by construction**, which is why the CLI's UNREADABLE paragraph can state the
fleet-wide fact plainly (it quotes the marked count printed a line above rather than asserting
anything the report cannot carry — the v10 T2 finding).

### 3. `crates/custodian/src/desired_state.rs` — the drain status names its blocker

New `ReconciliationStatus::PendingUnresolvable { objects }` (`desired_state.rs:119`), the sibling of
`PendingMalformed { chunks }` the brief cited (`:101-104` / `:198-203` on the base): same
containment, same "name the blockers in the answer itself". Ranked **below** the genuine-reference
`Pending` and **above** `PendingMalformed`, preserving the base's check order — the brief settled
that ordering at Plan and I did not revisit it (`desired_state.rs:225-232`). It also emits on
`wyrd.custodian.drain.audit` with the shared `action=unresolvable-chunk-map`
(`desired_state.rs:259`), so a collector watching the durability plane sees the same blocker GC and
scrub report.

Salvaged nearly verbatim from `iteration-v10/patch.diff`'s `desired_state.rs` hunk, which the brief
marked as entirely in scope.

### 4. `crates/server/src/cli.rs` — the verdict and the exit status are one decision

`restore_verdict(&RestoreReport) -> RestoreVerdict` (`cli.rs:1256`) returns the lines to print
**and** `needs_human`, taken from `RestoreReport::needs_human()` — not a second `||` chain
re-derived at the callsite. `cmd_custodian` prints the lines and exits on that flag
(`cli.rs:1196-1201`). That is what makes the printed verdict and the status code untestable-apart:
`restore_needs_human_agrees_with_every_paragraph_it_prints` (`cli.rs:2683`) drives one finding at a
time over a bare report (no backend, no fleet, no store) and pins paragraph ⇔ status ⇔
`report.needs_human()` ⇔ `!is_clean()` ⇔ the `complete`/`INCOMPLETE` word.

## What I deliberately did **not** build

* **The cross-object chunk-id ambiguity apparatus** — dropped as the brief instructs, not deferred.
  No `claims` map, no `CommittedChunks::ambiguous`, no `ambiguous-*` audit event, no
  mark-withdrawal, no `attributable`/`by_id_alone` split in pass 3, and none of its four
  discriminator legs or two `restore_reconcile.rs` legs. `restore.rs`'s module doc keeps its
  original "It deletes nothing itself" (v10 had to weaken that to "no **bytes**" because the
  withdrawal deleted `orphan:` records).
* **Report-schema churn** — `dangling` / `misplaced` / `under_replicated` keep their `Vec<ChunkId>`
  shape; `Expected` / `emit_dangling` / `emit_misplaced` are unchanged.
* **Inline naming of unreadable records in the CLI paragraph** (v10 had it, with a
  `NAMED_UNREADABLE_OBJECTS = 10` cap and a flood test). Dropped for two reasons: it is a third
  attribution channel on top of the audit seam and `RestoreReport::unresolvable`, and the two
  sibling paragraphs in this very command already say *"See the audit log for each chunk id"* — so
  following that pattern is the consistent choice. Cost measured, not asserted: the const + capped
  `format!` branch + `a_flood_of_unreadable_objects_is_capped_inline_and_the_rest_counted` were
  **28 semantic lines** of the budget. The paragraph still points at
  `action=unresolvable-chunk-map`, and `restore_verdict`'s test pins that the run's marked count is
  quoted rather than restated.
* **`gc.rs` / `scrub.rs`** — untouched, as the brief requires.

## Budget

Measured on the final `patch.diff`, per the brief's own definition (added lines, non-blank,
non-comment, non-mechanical) — and also with the looser counter that reproduces the planner's own
figure for v10, so the two are comparable:

| file | strict (non-blank, non-comment) | brief's definition (also non-mechanical) |
|---|---|---|
| `custodian/src/desired_state.rs` | 22 | 18 |
| `custodian/src/restore.rs` | 86 | 69 |
| `custodian/tests/restore_reconcile.rs` | 78 | 61 |
| `custodian/tests/segmented_map_consumers.rs` | 13 | 10 |
| `custodian/tests/segmented_map_restore.rs` (new) | 419 | 328 |
| `server/src/cli.rs` | 125 | 102 |
| `docs/…/06-runtime-view.md` | 1 | 1 |
| `docs/…/m4-first-deployment-blueprint.md` | 0 (comment lines) | 0 |
| **total (8 files)** | **744** | **589** |

**8 files exactly, the eight the brief names — no ninth.** Under the brief's stated definition the
patch is **589 ≤ 700**. Under the looser counter (which reproduces the brief's "v10 measured ~1,201"
exactly — same script, same patch — so it is the metric the planner used) it is **744**, i.e. 6%
over, and 62% of v10. I did not stop and hand back a split because the shape is not wrong: the
overage is entirely in the *discriminator's fixture*, which the brief itself flagged as "the largest
single item and the only real risk", and which cannot be shared with `segmented_map_consumers.rs`
(integration-test crates cannot import across files, and `wyrd-testkit` exports no in-memory
`MetadataStore` / `ChunkStore` — its `Double` lives inside its own `#[cfg(test)] mod`).

What I actually cut to get there, each verified still-green:
* the ambiguity apparatus and its six legs (the brief's own instruction) — the bulk;
* folded the standalone drain leg into the (2a) leg, since criterion (3) is stated *over the (2a)
  store* — **−11**;
* folded the `ReadableOnce` metadata double into `MemMeta` as an optional decay, in **both** test
  files, instead of a second full `impl MetadataStore` — **−64**;
* merged v10's two new `restore_reconcile.rs` legs into one that seeds *both* an always-unreadable
  record and a decaying one — **−25**;
* dropped `attributed_objects` + its set-equality assertion (the brief names only
  `assert_attributes_blocker` for criterion (3)) — **−14**;
* the CLI inline-naming cap and its test — **−28**;
* one D server instead of two in three legs, and message prose throughout — **≈ −45**.

What I refused to cut, with its cost, so the human can overrule me:
* `MemMeta::commit`'s precondition/delete handling (**6 lines**) — dead in this file, but a double
  that ignores a CAS lies about the seam (rubric: *test fidelity*).
* the (2c) **control run** (**11 lines**) — without it "nothing was marked" is satisfied by a pass
  that marks nothing, ever.
* `seed_damaged`'s `resolve_chunk_map(..).is_err()` fixture assertion (**5 lines**) — it is what
  stops a leg passing because the fault silently stopped being one.

## Verification

* **C4-verify** (`PDCA_BUNDLE=results/issue_651 ./engine/scripts/run-verify.sh`, the configured gate
  cmd): `PASS — red without the fix, green with it`. Classification is the one the brief predicted:
  a single `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs`, so the red leg runs
  `cargo test -p wyrd-custodian --test segmented_map_restore` with `restore.rs`,
  `desired_state.rs`, `cli.rs` and every modified test file reverted.
* **C4-ci** (`PDCA_WORKTREE=… ./engine/xtask.sh ci`): `xtask ci: all checks passed` — including the
  prose gates (`typos`, `lint_docs: OK`, `render_site` 98 pages), `cargo fmt --all -- --check`,
  `clippy --workspace --all-targets` (`-D warnings`), the full test suite, `cargo deny`, and the
  conformance vectors. Run twice (before and after the last doc-comment edit).
* Commit-hook readiness: `cargo fmt --all` applied over every touched file; the formatter and
  clippy are exactly what `xtask ci` re-checks above.

### Forced self-refutation (the three questions)

**(a) Genuine red?** — **Yes**, and measured rather than reasoned: `run-verify.sh` reverts the three
production files and re-runs the kept discriminator. All four legs fail, each on the defect the leg
exists for:

```
a_segmented_object_no_longer_stops_the_post_restore_pass
  → SegmentedMapUnsupported { operation: "restore::committed_chunks" }
an_unreadable_object_is_contained_and_neither_surface_certifies_it
  → SegmentedMapUnsupported { operation: "restore::committed_chunks" }
an_unreadable_object_does_not_starve_the_objects_the_pass_could_read
  → SegmentedMapUnsupported { operation: "restore::committed_chunks" }
marks_and_report_rest_on_one_reading
  → Error("expected ident", line: 1, column: 2)      # the base `?`s out of committed_chunks
test result: FAILED. 0 passed; 4 failed
```

The reds are behavioural, not "a symbol is missing": the discriminator names **no** symbol this
patch introduces — no `RestoreReport::unresolvable`, no `needs_human`, no
`ReconciliationStatus::PendingUnresolvable`. It asserts on base-visible fields, on the emitted audit
text (`"action":"unresolvable-chunk-map"`, `post-restore reconciliation INCOMPLETE`) and on
`!matches!(status, ReconciliationStatus::Satisfied)` (a base variant). The (2a)+(3) leg's drain half
is red on the base for the right reason too: the base answers a bare `Pending` and emits nothing on
`wyrd.custodian.drain.audit`, so `assert_attributes_blocker` fails.

**(b) Production path?** — **Yes**. Every leg calls the exported production entry points —
`wyrd_custodian::reconcile_after_restore` (`restore.rs:239`) and
`wyrd_custodian::desired_state::reconciliation_status` (`desired_state.rs:181`) — over in-memory
`MetadataStore` / `ChunkStore` doubles at the trait seams the pass is *defined* over, exactly as the
in-tree `tests/restore_reconcile.rs` and `tests/segmented_map_consumers.rs` do. Nothing is
re-implemented, mocked or copied: the doubles are stores, not stand-ins for the logic under test.
The `seg:` fixtures are built with the real validating constructors (`SegmentRecord::new`,
`SegmentedMap::new`, `metadata::seg_key`, `metadata::encode`), so the bytes the resolver reads are
the bytes a producer would write.

**(c) Fixture includes the fault?** — **Yes**, and it is asserted rather than assumed:
* `seed_damaged` (`segmented_map_restore.rs:392`) asserts
  `metadata::resolve_chunk_map(..).await.is_err()` on the seeded root — the leg cannot pass because
  the fault quietly stopped being one.
* The damaged object is **in** the store the assertions are drawn over, never curated out: (2b)
  asserts the *readable* object's loss with the damaged one still present and its own fragment
  still on disk and unmarked.
* (2c) carries a **control** over the same store without the decay, proving the stray is genuinely
  markable (`stranded_marked == 1`) — so "nothing was marked" in the decayed run is a refusal, not
  a no-op — plus `meta.inode_scans() >= 1` and a post-run scan asserting the record really is
  undecodable after the first read.
* (1) does not settle for "returned `Ok`": it asserts the pass **judged a chunk of the segmented
  object** (`report.dangling == vec![gone]`), which a pass that returned `Ok` by *skipping*
  segmented maps could not produce.

## Rubric self-review (`AGENTS.md` §"Review rubric & protocol")

* *One clock per lifecycle* — no clock read added or moved; `wall_clock_millis()` at the CLI callsite
  is untouched.
* *Narrow trait seams / dependency direction* — `restore.rs` still sees only `traits` / `core` /
  `tracing`; `metadata::resolve_chunk_map` is the same call `gc.rs` already makes.
* *Metadata validation boundaries* — a structurally invalid record surfaces as an **error** at
  decode and is contained as that object's fault, never as a value; the walk never peeks at `state`
  in bytes that will not decode (same reasoning as `gc.rs:369-377`).
* *Absent or unsupported entries* — the whole point: an unreadable record is an explicit,
  **named**, non-certifying report entry and a non-zero exit, never a silent skip. No count-based
  assertion can pass while the property fails: `is_clean()` is defined through `needs_human()`.
* *Await discipline* — the one new await is `resolve_chunk_map`, bounded by the `MetadataStore`
  implementation exactly as `referenced_fragments`'s identical call is; the precedent and the
  #508/#636 reasoning are cited in `committed_chunks`'s doc so a reviewer does not have to re-derive
  it. No task is spawned.
* *Test fidelity* — the doubles mirror the seam (`commit` honours preconditions → `Conflict`, puts
  and deletes); the `#[cfg(test)]`-free `Once` guard for `tracing` callsite interest follows the
  in-tree #214 pattern.
* *Docs currency* — this PR adds a public field and a public enum variant and changes the command's
  exit contract, so the living architecture docs move with it (both files below), not as a
  follow-up.
* *`#![forbid(unsafe_code)]`* — present on the new test crate root.
* *Deferrals are settled* — the two `deferred: #651` markers #650 left at `restore.rs:196` and
  `desired_state.rs:183-187` are **discharged and removed**, which is what this slice is.

## Docs

* `docs/design/architecture/06-runtime-view.md:31` — the invariant paragraph already covered a
  reclaim-capable pass and a verify-only pass, but the **post-restore** pass is neither (it marks
  *and* reports), and the report-only clause did not say the drain status *names* its blocker. Two
  sentences added for exactly those two gaps, plus one for the one-reading rule. Nothing else in the
  paragraph is touched.
* `docs/design/architecture/m4-first-deployment-blueprint.md:599` — the operator runbook's "two
  different bills" becomes three, with UNREADABLE, the non-zero exit, and the INCOMPLETE summary
  word. Every sentence is derived from a field the report actually carries (the v10 T2 finding): it
  says the pass names each record and reports what it marked, and it states "nothing anywhere in the
  fleet was marked" only in the form the command prints it — *where that count is zero*.

## External dependencies

None beyond the base Rust toolchain: the pass runs over the traits/core seams with in-memory
doubles. No Docker, no protoc, no live backend, no new dev-dependency, no DST leg. The four
`doctor.checks` ids the brief named (`typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`) were
all present and green in the `xtask ci` run above.

## Scratch

No scratch dirs created outside `$PDCA_SCRATCH`; the one measurement file
(`$PDCA_SCRATCH/pdca-builder-651-measure.diff`) is removed at the end of the run.
