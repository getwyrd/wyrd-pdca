# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Advisory only; I gate nothing. Evidence was re-run independently at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `339da46`) in a throwaway worktree under
`$PDCA_SCRATCH` (removed).

**The red→green survives the attack.** I reproduced it rather than taking it on trust: the new
`crates/custodian/tests/segmented_map_passes.rs` compiles and fails **6/6** on `origin/main` with
the three production files reverted (assertion/behavioural reds — `SegmentedMapUnsupported`,
`Satisfied` where `Blocked` is required, `3` namespace scans where `1` is required), and passes
**6/6** with the patch. The legs drive the real `reconcile_step` fence and the real
`backfill::reconcile` entry, over trait doubles — no parallel re-implementation. `check-gates.json`'s
`C4-verify` row is honest. Note in passing that leg 6 *is* base-red (it seeds an undecodable record
first), which contradicts the brief's `:118-119` "NOT base-red" prediction — that makes the
discriminator stronger, not weaker, so it is not a finding.

What I could break is the **causal adequacy of the fixture** and one **operator-visible claim**.

- **NEEDS-HUMAN [impl] — pinned decision 3 is unbound for two of the three passes; a re-derived
  canonical CAS key survives the whole suite.** `crates/custodian/src/reconstruction.rs:825` keeps
  `Arc::from(key.as_slice())` (and `:663` CASes on it), `crates/custodian/src/rebalance.rs:304`
  does the same (`:416`). I replaced both with
  `Arc::from(metadata::inode_key(parse_inode_key(&key).expect("parsed")).as_slice())` — i.e. exactly
  the defect decision 3 exists to prevent, "reads one record and commits over the other" — and
  **all 6 discriminator tests plus the entire `cargo test -p wyrd-custodian` suite stayed green.**
  The reason is visible at `crates/custodian/tests/segmented_map_passes.rs:663-675`: the only
  non-canonical keys in the file (`inode:007` / `inode:7`) are seeded into a store with an **empty
  repair queue and no draining server**, so reconstruction returns `Satisfied` unread
  (`reconstruction.rs:163-170`) and rebalance early-returns at `rebalance.rs:125-128` — neither pass
  ever reads those keys. Concrete missing case: seed a repairable chunk (and a fragment on the
  draining server) under `inode:007` beside a *different* record at `inode:7`, then assert the
  repoint/evacuation landed on `inode:007` and left `inode:7` byte-identical. `cargo-mutants` cannot
  generate this mutant (it does not rewrite a field/expression of this shape), so the `C5-mutants`
  "0 missed" row does not cover it.
- **NEEDS-HUMAN [impl] — the same block's comment is an unwarranted claim.**
  `crates/custodian/tests/segmented_map_passes.rs:666-670` says "this store is the control against
  OVER-containment: every pass does its work AND certifies it", and asserts
  `assert_ne!(got, Reconciled::Blocked)` for all three. For reconstruction and rebalance that
  assertion is **vacuously true** — they short-circuit before touching `inode:`. Only backfill is
  actually a control. Either drive the other two over that store (queue an obligation, mark a server
  draining) or drop the claim.
- **NEEDS-HUMAN [impl] — reconstruction's "an incomplete reading may not certify" is asserted by
  nothing.** `crates/custodian/src/reconstruction.rs:170` seeds `refused` from `index.unaccounted`.
  I replaced it with `let mut refused = 0usize;` and all 6 legs plus the full custodian suite stayed
  green. Leg 3 only *appears* to bind it: it always enqueues `C_UNSEEN`, a chunk no record
  references, so `assess` independently produces `Refused(REFUSED_INCOMPLETE)` via
  `reconstruction.rs:394` and reaches `Blocked` by a different route. The uncovered case is the
  realistic one — an unreadable committed object beside a queue whose obligations **all** resolve to
  flat sites: the shipped code correctly answers `Blocked`, but nothing would notice if it stopped.
  Add a sub-assertion to leg 3 that enqueues only `C_REPAIR` (drop `C_UNSEEN`) over the damaged
  store and still requires `Blocked`.
- **NEEDS-HUMAN [human] — backfill publishes `backfill_placement_remaining = 0` over a reading it
  admits is incomplete, and the patch treats "unreadable" inconsistently with "declined".**
  `crates/custodian/src/backfill.rs:166-169` adds a declined **segmented** record's empty placements
  to `remaining` (leg 2 pins that: gauge `1`), but an object taken out at
  `backfill.rs:98-104`/`:118-126` (undecodable record, unparsable key, unresolvable map) contributes
  **zero** — its unknown empty placements are silently counted as none. Probed on the target: a store
  of one undecodable record plus one fillable flat record makes the pass emit
  `outcome=Blocked gauge=0`. On the base the pass returned `Err` and emitted **no** sample at all, so
  this is a new false clean bill on the very gauge `backfill.rs:253-262` calls "ADR-0040 decision 6's
  first precondition". The counter-argument is real (`backfill_unaccounted_records` fires and the
  outcome is `Blocked`), and #350 step 2 requires a sample *every* pass — which is why this needs a
  human: the brief's own invariant "a pass never claims more than it read" (`brief.md:176-178`) and
  #350's "always emit" pull opposite ways here, and neither the code nor the fixture records which
  one won.
- **NEEDS-HUMAN [impl] — the gauge assertion is a prefix match, so it cannot pin the value it
  claims to.** `crates/custodian/tests/segmented_map_passes.rs:190-193` asserts
  `logged.contains(r#""gauge.backfill_placement_remaining":{value}"#)` with no trailing delimiter.
  I changed `emit_remaining(remaining)` to `emit_remaining(remaining * 10)` — a gauge over-reporting
  the drain backlog tenfold — and all 6 legs passed (`:575` wants `1`, sees `10`, matches on the
  prefix; `:672` wants `0`, sees `0`). Given that the round-3 carry-forward
  (`brief.md:420`) rebuilt this bundle *specifically* because the remaining gauge was undetectable,
  the helper should match `":{value},"` / `":{value}}}"` or parse the JSON field.

## Attacks that failed (stated, so the silence is informative)

- **Decision 4 ("write only the generation you read").** I traced the restart path: only a
  *segmented* snapshot can reach `resolve_current_chunk_map` (`crates/core/src/metadata.rs:2584-2586`
  returns `Answer(Cow::Borrowed)` for flat with no store read), and all three passes branch on the
  **snapshot's** `record.chunk_map.is_segmented()` — `reconstruction.rs:834`, `backfill.rs:166`,
  `rebalance.rs:248` — before any write. The leg-2 supersede sub-case
  (`segmented_map_passes.rs:590-612`) exercises it for real. I could not construct a flat-snapshot
  restart.
- **CAS on stored bytes.** `reconstruction.rs:663`, `rebalance.rs:416`, `backfill.rs:194-196` all
  `require` the row's own bytes; the fixture's `stored()` helper (`segmented_map_passes.rs:304-313`)
  seeds every root non-canonically, so a re-encoding regression would lose every CAS and fail
  `assert_flat_work_done`. This is genuinely bound.
- **Over-containment.** `metadata::decode(&plan.prior_bytes)?` at `reconstruction.rs:649` /
  `rebalance.rs:361` and the `Err(err) => return Err(err)` non-`ChunkMapError` arms match
  `gc.rs:402-416` exactly; leg 6 proves a store fault still ends all three passes with the injected
  error text.
- **Duplicate `ChunkId`.** I tried same-record, cross-record, and segmented-then-flat orderings
  against `CommittedIndex::note` (`reconstruction.rs:713-722`); every ordering ends in
  `Site::Refused(REFUSED_AMBIGUOUS, ..)` with both keys named.
- **Arithmetic.** `remaining -= to_fill.len()` (`backfill.rs:202`) is always preceded by the matching
  `+=` in the same iteration — no underflow. `fragment_count()` is `>= 1` for every `EcScheme`
  (`crates/core/src/metadata.rs:148-153`), so the "fill an empty placement to an empty vector"
  gauge-drift case I looked for is unreachable.
- **Retained state.** `FlatSite`/`EvacPlan` hold `Arc<[u8]>` of at most one record per object that
  yields work (`<= Q` objects), not decoded chunk lists — within the brief's "proportional to the
  obligations it holds".
- **Scope.** The DST deferral is recorded-rejected at Plan (`brief.md:349-360`) and the AGENTS.md
  reviewer protocol makes deferrals settled; I did not re-raise it. Rebalance's lack of a duplicate-id
  rule and `EvacOutcome::Aborted`'s certification are pre-existing / #682's by the brief, so they are
  not filed here.
