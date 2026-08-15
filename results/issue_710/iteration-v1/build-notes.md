# Build notes — issue #710 (ceiling-refused placement writes do not certify)

## What changed, and why

**Defect 1 — no ceiling check on the flat placement-maintenance write path.**
`crates/core/src/metadata.rs` already states the invariant ("a root that cannot be
re-written is an object whose placement can never be repaired", `:334-352` region) but
enforced it nowhere on a write path. I added one small, pure helper —
`flat_value_ceiling_crossed(encoded_next: &[u8]) -> Option<usize>` (`metadata.rs:370`) —
rather than touching `commit_chunk_map` (`metadata.rs:1741-1768`, unchanged). The brief's
scope line is explicit: *"Carve out only the ceiling helpers the write path needs — not
the committer around them."* `commit_chunk_map` is a shared, tested, general-purpose CAS
helper used directly by ~9 existing tests across 3 crates (`core/tests/mutation_
regressions.rs`, `custodian/tests/reconstruction.rs`/`backfill.rs`/`backfill_telemetry.rs`,
`metadata-redb/tests/conformance.rs`) for scenarios that don't need a ceiling check at
all; wrapping it would touch every one of those call sites for no reason the brief asks
for, and neither `reconstruction.rs` nor `rebalance.rs` actually calls `commit_chunk_map`
in production — both build their `WriteBatch` inline (extra deletes/orphan puts the
generic helper doesn't do). So the check lives where the two binding commits already
build `next` and its bytes, called on the already-computed `encode(&next)` (no double
encode): `reconstruction.rs:909` (`repair_chunk`) and `rebalance.rs:465`
(`evacuate_chunk`), both immediately before the `WriteBatch` is constructed — so a
crossing repoint is refused before a single byte is written for it, and the refusal is a
new outcome variant (`RepairOutcome::Refused` / `EvacOutcome::Refused`, both carrying
`{bytes, ceiling}`) — never a raw backend `Err`, and never conflated with
`Conflict`/`Aborted`.

Cost of the alternative I ruled out (wrapping `commit_chunk_map` in a new
`commit_chunk_map_checked` and rerouting both call sites through it): it would still not
apply to `reconstruction.rs`/`rebalance.rs` as they stand today (neither calls
`commit_chunk_map`), so it would *also* require rewriting their inline `WriteBatch`
construction to go through the wrapper — a strictly bigger diff (touching the
orphan-put / repair-key-delete logic inline in both loops) for the same behavioural
result. The chosen shape is the smaller one AND matches the brief's explicit
instruction, so there's no minimalism-vs-invariant tension here.

**Defect 2 — a move that did not persist neither certifies nor counts.**
- `reconstruction.rs`: the new `Refused` arm joins the durability-plane offset pattern
  `Conflict`/`Aborted` already use (`emit_ceiling_refused`, `reconstruction.rs:1130`),
  and a `ceiling_refused` flag joins `reading.incomplete` / `!reading.refused.is_empty()`
  in the final `Reconciled::Blocked` gate (`reconstruction.rs:353`, was `:331` pre-patch) —
  extending the documented `repaired − conflict − aborted` identity to
  `− ceiling_refused` (`reconstruction.rs:277`, `:1003`).
- `rebalance.rs`: the brief is explicit that this child **settles** the pre-existing
  silent `EvacOutcome::Aborted => {}` arm (`rebalance.rs:159`, was the bare `{}` at the
  base's `:128`) — "#696 deliberately left it to this work." So both the new `Refused`
  arm and the pre-existing `Aborted` arm now set a `move_incomplete` flag that joins
  `scan.withheld` in the final `Reconciled::Blocked` gate (`rebalance.rs:163`). This is a
  **named cost** per the brief's scope, not a scope-creep: it is why
  `crates/custodian/tests/rebalance.rs` is the *named fifth file* — the pinned
  `spread_wins_when_no_free_distinct_domain_remains` test (`tests/rebalance.rs:915-983`)
  asserted `Reconciled::Satisfied` for exactly this shape (no free distinct domain); I
  flipped that one assertion to `assert_ne!(outcome, Reconciled::Satisfied)` (`tests/
  rebalance.rs:971`) — the brief's own suggested discriminator shape ("asserts
  `!matches!(.., Satisfied)` with base symbols only") — and left every other assertion in
  that test (version untouched, placement untouched, `reconciliation_status ==
  Pending`) exactly as it was, since those were already correct.

No new public `CommitOutcome`/`Reconciled` variant: `Reconciled::Blocked` already exists
on base (`reconciliation.rs:44`) and is reused, exactly as the brief specifies.

## Alternative considered and rejected: a `CommitOutcome::Refused` variant

I considered adding a third `CommitOutcome` variant in `crates/traits/src/lib.rs` so the
ceiling check could live *inside* a `MetadataStore::commit`-adjacent helper and be
returned uniformly. Rejected on two grounds: (1) `crates/traits/src/lib.rs` is not one of
the five files the brief's scope names (`core/src/metadata.rs`, `custodian/src/
reconstruction.rs`, `custodian/src/rebalance.rs`, `custodian/tests/placement_ceiling.rs`,
`custodian/tests/rebalance.rs`) — "a sixth file means the shape is wrong"; and (2) it
would ripple into every one of the ~30+ existing `MetadataStore` implementors' and
callers' exhaustive `match`es on `CommitOutcome` across the workspace (every backend,
every test double) for a check that only two call sites need — the crate-private
`RepairOutcome`/`EvacOutcome` enums are the correct-sized surface, matching how `Aborted`
and `Conflict` are already crate-private outcomes in exactly these same two files.

## Test — three legs, one new file

`crates/custodian/tests/placement_ceiling.rs` (new). Driven only through
`reconcile_step`/`Custodian`/`FencedZone`/`ReconstructionContext`/`RebalanceContext`/
`Reconciled`, `desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
ReconciliationStatus}`, `repair::{enqueue_repair, queued_repairs}`, and
`metadata::{inode_key, encode, decode, MAX_VALUE_BYTES, ChunkMap, InodeRecord, ChunkRef,
EcScheme}` plus base `wyrd_traits`/`wyrd_core::{placement,write,erasure}` symbols already
used by the sibling `reconstruction.rs`/`rebalance.rs` test files on base (`write_new_
object_placed`, `Topology`, `ChunkStore`/`MetadataStore`/`PlacementChunkStore` — needed to
build the in-memory doubles the brief calls for). **Never** names
`flat_value_ceiling_crossed`, `RepairOutcome::Refused`, `EvacOutcome::Refused`, or
`emit_ceiling_refused` — the HARD CONSTRAINT — so reverting the production change leaves
the test compiling and it goes genuinely red (verified below), never UNVERIFIABLE.

- **Leg 1** (`repair_refuses_a_placement_move_that_would_cross_the_value_ceiling`):
  hand-seeds a real RS(2,1) chunk (via `write_new_object_placed`, the same real write
  path the sibling tests use — not a hand-rolled fragment format), pads the *committed*
  record via its `content_type` field (ADR-0047 metadata, preserved verbatim across a
  repair's `..prior.clone()`, so it never interferes with the chunk list either loop
  reads/validates) to exactly `MAX_VALUE_BYTES − 10` bytes, then loses one fragment and
  arranges the rebuild's only free domain to be a `u64::MAX` id. Asserts: byte-identical
  record, obligation stays queued, `Reconciled::Blocked` (not `Satisfied`/`Changed`), and
  `reconstruction_ceiling_refused` on the durability seam.
- **Leg 2** (`evacuation_that_would_cross_the_value_ceiling_does_not_certify`): same
  ceiling-crossing shape over `rebalance::reconcile` — a draining server's single
  fragment would move onto a `u64::MAX` id. Asserts `!matches!(outcome, Satisfied)` (the
  brief's own suggested discriminator shape), byte-identical record, the fragment still
  on the draining server, and `reconciliation_status == Pending` (not falsely
  `Satisfied`).
- **Leg 3** (`a_ceiling_refused_repair_is_subtracted_from_reported_successes`): one
  reconstruction pass mixing a `Committed`, an `Aborted`, and a ceiling-`Refused` chunk;
  asserts the `repaired − conflict − aborted − ceiling_refused` identity equals the true
  committed count (1), plus independent version/queue-state assertions per chunk. Per the
  brief, this leg is *not* independently discriminating on its own (on base the
  would-be-refused repair simply commits and correctly counts as one more success, so the
  bare identity holds there too) — it is red only as a derivative of leg 1's
  `remaining`/version assertions, which the test also carries.

### The domain-selector puzzle (leg 3), briefly

Getting three DIFFERENT outcomes (`Committed`/`Aborted`/`Refused`) out of ONE shared
`Topology`+`ReconstructionContext` took real trial and error: `select_distinct_domains_
excluding` (`core/src/placement.rs:290-294`) ranks candidate domains by
`(min utilization, label)` over the *whole* topology, so any two objects whose free-domain
sets are identical are indistinguishable to it — my first attempt had ABORT and REFUSED
both choosing between the same {ghost, huge} pair and always agreeing (confirmed
experimentally: `outcome=Changed` instead of `Blocked`, ABORT's chunk landing on the
`u64::MAX` id it needed). The fix: give ABORT chunk a genuine 4th survivor fragment
already placed on the huge-id domain (a real RS(3,1) write, not a fabricated one) so
`u64::MAX` never even enters ABORT's OWN free-domain candidate set — it's already "held".
Symmetrically, REFUSED's own lost domain is loaded (`set_utilization`) so it can't
cheaply reclaim its own slot back. This is exercised, not merely asserted: the
`assert_eq!(..., vec![0, 1, 2, HUGE], ...)` right after the ABORT write pins the
real placement the selector produced.

## Refutation (the three required questions)

**(a) Genuine red?** Yes — verified two ways. (1) `git stash` on the four production
files (test kept) + `cargo test -p wyrd-custodian --test placement_ceiling`: all 3 tests
fail (`evacuation_...`, `repair_refuses_...`, `a_ceiling_refused_...`), each on the
byte-identical / `Reconciled` assertions. (2) The project's own gate,
`PDCA_BUNDLE=results/issue_710 ./engine/scripts/run-verify.sh`, run from the `wyrd-pdca`
root against `origin/main`: **exit 0**, `run-verify.sh: PASS — red without the fix, green
with it (3 test(s) ran red)`. Not UNVERIFIABLE (the RED leg compiles and runs; it never
references a symbol the patch adds).

**(b) Production path?** Yes — every leg drives the real `reconcile_step` →
`reconstruction::reconcile`/`rebalance::reconcile` → `repair_chunk`/`evacuate_chunk`
functions this patch edits, over `MetadataStore`/`ChunkStore` doubles (no mock of the
ceiling logic itself, no re-implementation — the doubles hold real bytes with no native
ceiling, exactly as the brief specifies: *"an in-memory `MetadataStore` double has no
value ceiling ... so 'the object is now un-repairable' is not observable through it. Do
not copy the parent brief's optional two-phase demo."* I didn't; each leg asserts the
stored byte length / byte-identity directly, per the brief's steer.

**(c) Fixture includes the fault?** Yes — the crossing IS the fault under test in legs 1/2
(a real, growing placement entry against a record seeded genuinely close to the ceiling,
not a record built to stay comfortably clear of it), and leg 3's fixture genuinely
contains one of each of the three outcomes in the SAME pass (not three separate passes
curated to avoid interaction) — the domain-selector engineering above exists precisely
*because* I insisted on all three sharing one topology/pass rather than three easier,
non-interacting ones.

## Formatter / lints

`cargo fmt -p wyrd-core -p wyrd-custodian` (clean, `-- --check` passes) and
`cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` (clean, workspace lint
policy). Also ran the full `cargo test -p wyrd-custodian -p wyrd-core` and `cargo test -p
wyrd-metadata-redb` (which exercises `commit_chunk_map` directly, unchanged) and `cargo
build --workspace --tests`: no regressions anywhere in either crate's existing suite.

## Scope discipline

Touched exactly the five files the brief names (`core/src/metadata.rs`,
`custodian/src/reconstruction.rs`, `custodian/src/rebalance.rs`,
`custodian/tests/placement_ceiling.rs` [new], `custodian/tests/rebalance.rs`) and nothing
else — no `backfill.rs`, no `resolve_chunk_map`/read-side change, no new dependency, no
ADR/spec edit, no segmented-map addressing. Added semantic (non-blank, non-comment)
production lines: `metadata.rs` +3, `reconstruction.rs` +37, `rebalance.rs` +30 (measured
via `git diff | grep '^+' | grep -v '^+++' | grep -vE '^\+\s*(//|$)'`) — well inside the
≤250 budget.

## External dependencies

None beyond the base Rust toolchain — no Docker, no protoc, no live backend, no new
crate dependency (all imports used in the new test are already dev-dependencies of
`wyrd-custodian`: `wyrd-coordination-mem`, `wyrd-testkit`, `async-trait`, `bytes`,
`tracing-subscriber`, `tokio`). Nothing to flag as NEEDS-HUMAN for a missing dependency.
