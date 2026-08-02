# Build notes — issue 635 / segmented-chunk-map (iteration 16)

Withheld from the reviewer; written for the human at sign-off.

## What this round is

Iteration 15 was rejected on **one gate only** — T4 batched rubric review — with 12 blocking
findings that dedupe to **five sites of one defect**, stated verbatim in the brief's
`## Iteration 15 — carry-forward` block and listed in `review-batch.md`:

> the decode-error arm classifies a malformed inode as unresolvable BEFORE consulting
> `inode_state_hint`, so an uncommitted (Pending) inode is counted as a committed blocker.
> GC already does it in the right order (`referenced_fragments` / `ReferenceSet::unresolvable`).

The five sites named: `backfill.rs:108-110`, `backfill.rs:303`, `rebalance.rs:172-174`,
`reconstruction.rs:649-651`, `restore.rs:408-410`.

So this iteration is **the v15 patch, re-applied verbatim, plus that defect fixed at its
root, plus five new binding tests**. Nothing else in the slice was re-decided: the brief's
design (0016 decision 7, the pinned encoding, staged publication, the containment table) is
unchanged, and iteration 15 already passed C1 Spec, C3 Change, T1 Structure, T2 Shape,
`cargo xtask ci` and C4-verify on this same base.

Baseline provenance: `results/issue_635/iteration-v15/patch.diff` applied to
`origin/main @ 9120f7a` (the brief's target base; `$PDCA_BASE` / `$PDCA_VERIFY_BASE` are
**unset** and there is no `stack-base` file in the bundle, as `Falsifiability` 2 requires me
to check and report).

## The fix — at the root, not per call site

Iteration 9's sign-off already set the standard for this class: *"fix at the foundation, not
by whack-a-mole per call site … a third call site with the same pattern must not become a
future review round."* Five sites re-spelling the same arm is exactly that failure repeating,
so the fix is **one function**, not five edits:

- `crates/custodian/src/resolve.rs:243` — **`classify_root(key, value)`**, the decode arm of
  every `scan("inode:")` loop, with the state ordering inside it. It returns a three-way
  `Root` (`resolve.rs:205`): `Decoded` / `UncommittedUnreadable(fault)` / `Unresolvable(fault)`.
  This mirrors the module's own existing rationale for `contain` being the crate's only
  `downcast_ref::<ChunkMapError>()` (`resolve.rs:195-206` on the v15 baseline): *a decision
  that must be identical everywhere is taken in one place*.
- `crates/custodian/src/resolve.rs:199` — `ChunkMapFault::attribute_uncommitted(pass)`, the
  sibling of `attribute(pass)`. The **return type** is the enforcement: `attribute` hands back
  the blocker a pass must record, `attribute_uncommitted` hands back nothing to record, so a
  site cannot accidentally record an uncommitted record as a blocker.
- `crates/custodian/src/resolve.rs:316` — `emit_uncommitted_unreadable(pass, object, err)`,
  moved out of `gc.rs` (where v15 had it as `gc_unreadable_uncommitted_record` on the
  `wyrd.custodian.gc.audit` target) and given the same `pass` label the sibling
  `emit_unresolvable` carries. Counter renamed `custodian_unreadable_uncommitted_record`,
  target `wyrd.custodian.audit` — consistent with `custodian_unresolvable_chunk_map`. Both
  names are this bundle's own new surface (neither exists on `9120f7a`), so no external
  consumer is broken and no doc names them.

All **six** root-decode sites now go through it — the five defective ones and GC, whose inline
check is deleted rather than left as a second spelling:

| Site (post-patch line) | Arm added |
|---|---|
| `crates/custodian/src/gc.rs:305` | `attribute_uncommitted(GC)`, inline check removed |
| `crates/custodian/src/backfill.rs:108` | `attribute_uncommitted(BACKFILL)`, not in `unresolvable` |
| `crates/custodian/src/backfill.rs:312` | not counted into `backfill_unreadable_records` |
| `crates/custodian/src/rebalance.rs:172` | `attribute_uncommitted(REBALANCE)`, not counted |
| `crates/custodian/src/reconstruction.rs:649` | `attribute_uncommitted(RECONSTRUCTION)`, no blind spot |
| `crates/custodian/src/restore.rs:408` | `attribute_uncommitted(RESTORE)`, not in `RestoreReport::unresolvable` |

`grep -rn "metadata::decode" crates/custodian/src/` now shows exactly one `InodeRecord`
decode in the crate (inside `classify_root`) — a seventh walk inherits the ordering and
**cannot** spell it differently. That is the property the review round asked for.

### Why "not a blocker" is safe, and why the doubt runs one way

`metadata::inode_state_hint` (`crates/core/src/metadata.rs:1948`) answers `Some` only from a
strict streaming decode of the one field: bytes that are not JSON, a duplicate `state`, an
unknown state string are all `None`. `classify_root` treats `None` as **possibly committed**
(→ `Unresolvable`), so the only records that lose blocker status are ones whose still-readable
bytes say plainly `Pending`. Reading a committed record as pending would be the #508-attempt-4
data loss; that direction is structurally excluded.

### Attribution is never dropped

The rubric's *Absent or unsupported entries* rule (`AGENTS.md:175-177`) is why the uncommitted
arm still emits: not-a-blocker is not not-seen. The one site that drops the fault without
emitting is backfill's telemetry rescan (`backfill.rs:312`), which runs immediately after the
pass loop over the same scan and would otherwise double-emit for one object — the same reason
v15 already gave for its `Unresolvable(_)` arm two lines below.

## Tests — five, each a positive observable, all in EXISTING test files

The brief forbids a second **added** `tests/*.rs` file (`run-verify.sh` folds every added
target into one cargo invocation and keeps it on the RED leg, so a compile-red file would
destroy leg A's assertion red). Every new test therefore lands in a file that already exists:

| Leg | File (post-patch line) | Positive observable |
|---|---|---|
| backfill pass | `crates/custodian/tests/backfill.rs:649` | `reconcile` returns `Ok(Changed)` and the healthy record is filled; control (`Committed`) still `Err(UnresolvableChunkMaps)` |
| backfill telemetry | `crates/custodian/tests/backfill_telemetry.rs:398` | `backfill_unreadable_records == 0` (control: `1`), `backfill_placement_remaining == 0`, damaged record still named on the seam in **both** spellings |
| rebalance | `crates/custodian/tests/rebalance.rs:1403` | `rebalance_unresolvable_records == 0` (control: `1`) while the healthy object still evacuates to `[0, 3, 2]` |
| reconstruction | `crates/custodian/tests/reconstruction.rs:1830` | the absent chunk's obligation **drains** off `repair::queued_repairs` (control: stays queued) |
| restore | `crates/custodian/tests/restore_reconcile.rs:931` | `RestoreReport::unresolvable` is empty (control: `["inode:1"]`) while `dangling == [77]` in both |

Each is a two-arm loop over the *same bytes* with only `"state"` changed, so the containment
the slice exists for is the control on every one of them: a "fix" that simply stopped
recording blockers would pass the first arm and fail the second. The fixture is honest —
each test asserts `metadata::decode::<InodeRecord>(stored) .is_err()` before running the
pass, so the damaged record really is damaged, and it really is in the store beside the
healthy one.

Two of the five are gauge read-backs rather than behaviour, because for `plan_evacuations`
and the telemetry rescan the count *is* the whole observable — it feeds no decision. I did
**not** invent a behavioural seam to make them testable (that would be production API added
for a test). `rebalance.rs` already carries the `enable_metric_callsites` +
`DurabilityTelemetry`/`gather_prometheus` harness (`rebalance.rs:362`, `:1014`), so the leg
uses the suite's own idiom; I added `gauge_value`/`counter_value` there, copied from
`backfill_telemetry.rs:203`.

## Alternatives considered and rejected

- **Five local edits (one `if` per site), the shape the review report literally describes.**
  Rejected on the recorded directive from iteration 9 and on cost *measured*, not adjectival:
  five near-identical 6-line arms = ~30 lines of duplicated policy versus the shared
  `classify_root` at 14 lines of body + 3-line arms at each of six sites, and, decisively, it
  leaves the seventh walk free to get it wrong — which is how this defect reached round 15
  with GC already doing it correctly since round 14.
- **Putting the state check inside `contain`.** Rejected: `contain` is also used on the
  *resolve* arm (`chunks_of` / `homes_of`), where `record.state != Committed` has already been
  filtered, so a state re-check there would be dead code that reads as if it mattered.
- **Emitting from inside `classify_root`.** Rejected: it would double-emit in backfill's
  rescan, breaking the "attributed exactly once, by the pass that contained it" property
  `ChunkMapFault` is built around.
- **A co-located `#[cfg(test)]` module in `crates/custodian/src/rebalance.rs`** to call
  private `plan_evacuations` directly. Rejected: `crates/custodian/src/` carries no
  `#[cfg(test)]` module at all today (the crate's convention is integration tests under
  `tests/`), and the gauge read-back drives the same production function through the real
  `reconcile_step`, so the co-located module would buy nothing but a convention break.
- **A new `rebalance_telemetry.rs` test binary** (mirroring the `gc`/`gc_telemetry` split).
  Rejected: it is an **added** `tests/*.rs`, which C4-verify keeps on the RED leg — precisely
  the thing the brief forbids because it would destroy leg A's assertion red.

## Docs

`docs/design/architecture/06-runtime-view.md` — the containment paragraph gains the
uncommitted-record precision and the one-way doubt. This is not strictly a docs-currency
trigger (no port, API operation, RPC, CLI flag or persisted field changed this round —
`AGENTS.md:154-157`), but the paragraph already states the containment rule this round makes
narrower, and leaving it stating the old, wider rule would make the living doc wrong.

## Refutation — the three forced questions

**(a) Genuine red?** Yes, and it was *actually run*, not asserted. I stashed the six fixed
production files, restored the v15 (pre-delta) sources — the fix reverted, the tests kept —
and re-ran all five through the project's runner. All five failed, and every one failed on an
**assertion**, not a build error:

```
backfill            an_unreadable_uncommitted_record_does_not_fail_the_pass          FAILED (backfill.rs:705)
backfill_telemetry  the_unreadable_level_counts_committed_records_only               FAILED  left: Some(1.0) right: Some(0.0)
rebalance           the_evacuation_blind_spot_level_counts_committed_records_only    FAILED  left: Some(1.0) right: Some(0.0)
reconstruction      an_absent_chunks_obligation_drains_past_an_unreadable_...        FAILED  queue was [24225]
restore_reconcile   an_unreadable_uncommitted_record_is_not_a_hole_in_the_audit      FAILED  left: ["inode:1"] right: []
```

Restoring the fix turns all five green, and the whole `wyrd-custodian` suite (13 binaries)
passes. (The bundle's *own* binding red is still leg A on the base — that is the brief's
Success criterion and it is unchanged from v15; this is the red for **this round's delta**.)

**(b) Production path?** Yes. Every leg drives the real entry points — `backfill::reconcile`,
`reconcile_step(..., Some(&RebalanceContext), ...)`, `reconcile_step(..., Some(&Reconstruction
Context), ...)`, `reconcile_after_restore` — over the crate's own in-memory `MetadataStore`
doubles (the seam, not a copy of the pass). No pass is re-implemented, mocked or stubbed; the
only doubles are the store and the D servers, which is the suite's established shape and the
same one every sibling test in these files uses.

**(c) Fixture includes the fault?** Yes. Each test seeds the **damaged record itself** into
the store under a real `inode:` key and asserts it genuinely fails `metadata::decode` before
the pass runs; the healthy object it must not disturb is seeded beside it (and, in backfill,
*after* it in the scan, so a walk that stopped early would be caught). Nothing is curated out
— the two arms differ only in the four bytes of the `state` string.

## Environment / dependency checks the brief asks for

- `$PDCA_BASE` and `$PDCA_VERIFY_BASE`: **unset**. No `stack-base` file in the bundle. Base is
  plain `origin/main @ 9120f7a`, which carries #634 (`scan_page`). Reported as the brief's
  `Falsifiability` 2 instructs.
- No `Cargo.toml` was modified (checked: `git status` shows none), per `Falsifiability` 1.
- No external dependency was missing: `typos` and `docs-renderer` are both present and the
  prose gates ran (`render_site: link audit OK`).
- `cargo fmt --all` run over the tree; the gate's `cargo fmt --all -- --check` leg is green.

## Still open for the human at sign-off (carried, not resolved here)

These are the §6 NEEDS-HUMAN items the iteration-14/15 sign-offs left unticked. None is a
code defect this round could close:

1. **T3 runtime** — landing a `Completing`-less precursor committer before #636 supplies the
   real session fence (the brief's `Open questions` 4; pre-declared).
2. **C5 causal adequacy / T5 judgment** — carried from v14; note that round 15's C5 row read
   `Error: interrupted` and never tested a mutant (it hung in cargo-mutants' baseline on
   getwyrd/wyrd#646, a pre-existing tracing-dispatch deadlock unrelated to this patch). The
   iteration-15 sign-off says explicitly not to chase it.
3. **Validation fitness-to-purpose** — synthetic fixtures pre-#636, since no production path
   publishes a segmented map until #636 lands (the brief's `Production reach` (c)).
4. Round 15's reviewer and adversary leaves **never ran** (wyrd-pdca#187 / eduralph
   pdca-harness#369), so there are no C1–T5 judgment rows from that round to weigh.

## Scratch hygiene

`${PDCA_SCRATCH}/pdca-builder-635-redleg` held the six stashed production files for the red
leg; removed at the end of the run.
