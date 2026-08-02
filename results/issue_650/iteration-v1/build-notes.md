# Build notes — issue 650 / gc-scrub-through-resolver-fail-closed-containment

## What changed, and where (target branch: `pdca-integration/main`, base `4e78aeb`,
which already carries #648 + #649 per `stack-base`; brief's cited `origin/main` line
numbers predate those two integrations — the actual worktree state is what's cited
below)

- `crates/custodian/src/gc.rs`
  - `ReferenceSet` gains `unresolvable: BTreeSet<String>` (gc.rs:267) — committed
    objects whose chunk map the resolver could not read, keyed by the raw `inode:`
    key rendered lossily (attribution, not a parse).
  - `ReferenceSet::protects` (gc.rs:282) gains the blanket clause
    `!self.unresolvable.is_empty() || ...` — unchanged in spirit from the closed PR
    (`sources/salvage.diff`), and deliberately **not** narrowed (see
    `review-rejected.md` (iv)).
  - `referenced_fragments` (gc.rs:308) now resolves each committed record's map
    through `wyrd_core::metadata::resolve_chunk_map(meta, &key, &record)` — the same
    resolver `crates/core/src/read.rs:528` and `crates/server/src/lib.rs:354,442`
    already call — instead of `ChunkMap::as_flat().ok_or(SegmentedMapUnsupported)?`.
    `Ok(None)` (no live committed generation left under the key) is skipped exactly
    like an already-non-committed record. `Err` is downcast to
    `wyrd_core::metadata::ChunkMapError`: a typed verdict is recorded into
    `unresolvable` and attributed (`emit_unresolvable`, gc.rs:458) with `continue`
    (the walk goes on); anything else (a genuine store fault the resolver doesn't
    describe as a shape problem) is returned with `?` — it is not this object's fault
    and must not be silently folded in.
  - `gc::reconcile`'s outcome (gc.rs:221) answers `Reconciled::Blocked` when
    `unresolvable` is non-empty, ahead of the existing `Changed`/`Satisfied` split.
- `crates/custodian/src/scrub.rs`
  - Emits `emit_unscrubbable` (scrub.rs:224) once per unresolvable object
    (scrub.rs:110) before the fleet walk, and answers `Reconciled::Blocked`
    (scrub.rs:201) under the identical condition — this file already had the
    `Blocked`-shaped rule for a related class (issue #330's "never absorbed
    silently"); I only extended its final `Ok(if ...)` to the new variant and gave
    it the matching attribution emitter.
- `crates/custodian/src/reconciliation.rs`
  - `Reconciled` gains a third variant, `Blocked` (reconciliation.rs:42), with a
    `least_certified` combinator (reconciliation.rs:54: `Blocked` > `Changed` >
    `Satisfied`) that `reconcile_step` (reconciliation.rs:103) now folds each loop's
    outcome through, replacing the old `== Changed` bump.
- `crates/custodian/tests/gc.rs` — the existing
  `a_segmented_root_aborts_the_pass_before_any_fragment_is_reclaimed` test asserted
  the **old** interim contract from #648 (`gc.rs` aborts the whole step with `Err`
  on meeting *any* segmented root — a deliberate placeholder ahead of #649-#651).
  That premise is exactly what this slice replaces, so I renamed and rewrote it as
  `a_segmented_root_gc_cannot_resolve_blocks_certification_and_reclaims_nothing`:
  same fixture (a committed segmented root whose `seg:` records were never
  written — genuinely unresolvable), new assertion (`Ok(Reconciled::Blocked)`, not
  `Err`), same "nothing reclaimed, including an unrelated orphan" proof. Removed the
  now-unused `ChunkMapError`/`ReconcileError` imports that only that assertion used.
- `crates/custodian/tests/segmented_map_consumers.rs` — **new**, the brief's named
  test target. See "Test design" below.
- `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — appended, verbatim to
  the brief's Docs-currency wording, the containment sentences for the two
  maintenance passes that read the resolver's output (GC/scrub); left the existing
  resolver paragraph and the read-path arms untouched, and did not add the
  repair/evacuation-walk sentences (#651's).
- `review-rejected.md` (bundle root) — the brief's four "do-not-re-earn" findings,
  checked against this patch (none re-land; (iv) is the one actually relevant, and
  it is *adhered to*, not re-litigated — see there).

Nothing touches `restore.rs`, `reconstruction.rs`, `rebalance.rs`, `backfill.rs`, or
`desired_state.rs` — all explicitly out of scope (#651). `desired_state.rs`'s
`reconciliation_status` is unchanged and, as an *unintended-but-correct* consequence,
now answers `Pending` for a draining server holding a segmented object's fragment
(criterion (1)'s drain leg) purely because `referenced_fragments` now populates
`placed` for segmented objects — no new code there. It does **not** yet special-case
`unresolvable` (e.g. answer something other than `Satisfied` for a drain when the set
is incomplete, mirroring `PendingMalformed`); that gap is real but is explicitly
#651's ("desired_state (#651)" in Out of scope, and "#651's restore calls
`gc::referenced_fragments` and gates on `protects`" in Ordering note). Flagging it
here for the record, not fixing it — no caller for that behavior lands in this slice.

## Why this shape (and what I ruled out)

**Reuse the landed resolver rather than re-deriving one.** `#649` already landed
`wyrd_core::metadata::resolve_chunk_map`/`resolve_current_chunk_map` with the
retry/restart contract (decision 7(h)) and the typed `ChunkMapError` taxonomy for
every anomaly a segmented range can show. The closed PR's salvage (`sources/salvage.diff`)
predates that landing and invents its own `crate::resolve` module (`classify_root`,
`contain`, `Fault::attribute`, `chunks_of`) that does not exist on this base at all —
those symbols were never real; I did not implement them. Building `referenced_fragments`
on top of the already-typed `resolve_chunk_map`/downcast-to-`ChunkMapError` pattern is
strictly less code and is the exact pattern `crates/core/src/metadata.rs:2635-2639`
documents for "a maintenance pass" recovering the variant. Cost of the rejected
alternative (reimplementing the old `crate::resolve` shape from salvage): the whole
`Root`/`Contained`/`Fault` scaffold plus its own root-decode/committed-state
classification (~120 salvage lines, `sources/salvage.diff` lines 1-144) that
`resolve_chunk_map` already subsumes — not merely "heavier", literally redundant with
code already in the tree.

**Downcast to `ChunkMapError`, not a blanket `Err(_) => unresolvable`.** The
alternative — treat *any* `Err` from `resolve_chunk_map` as "this object is
unresolvable" — is one line shorter (`let object = ...; unresolvable.insert(object);
continue;` with no match) but is exactly the shape criterion (3)'s second half rejects:
a store fault unrelated to this object's map (the `seg:` range read itself failing,
e.g. a timeout) would then silently vanish into "the reference set is incomplete"
instead of propagating and failing the pass loudly. `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed`
(new test, `crates/custodian/tests/segmented_map_consumers.rs`) pins this: it injects
a plain `std::io::Error` (not a `ChunkMapError`) into the `seg:` range read via a
`PoisonedMeta` wrapper and asserts `reconcile_step` still returns `Err`.

**Blanket freeze (`!unresolvable.is_empty()`), not a per-object-scoped one.** The only
way to scope the freeze more narrowly (e.g. exclude only fragments provably NOT of the
unresolvable object) would require knowing which chunk ids the unresolvable object
owns — which is precisely the information an unresolvable map withholds. This is
`review-rejected.md` (iv): the closed PR already settled this as the correct rule: the
defect was the step's *return value*, never the predicate. I kept `protects`
unchanged in shape and only added the new clause.

**`gc::reconcile` gains its own `Blocked` branch (not left to `reconcile_step`'s
combinator alone).** Criterion (2) requires GC *alone* (no scrub context supplied) to
answer `!= Satisfied`. If only `reconcile_step`'s `least_certified` combined answers
across loops, a GC-only call would still see `gc::reconcile`'s own return value
first — so the loop itself must know its set was incomplete, not just the composer.
Scrub already had a shape close to this (the `Blocked`-worthy invariant was already in
its docs, `crates/custodian/src/scrub.rs`'s original `walks_and_verifies_referenced_fragments`
contract); I gave GC the identical rule rather than inventing a second one (brief:
"GC must return the same answer for the same condition, do not invent a second rule").

**Test fixture: typed constructors (`SegmentGroup::new`, `SegmentedMap::new`,
`SegmentRecord::new`) over the salvage file's raw hand-typed JSON strings for the
*healthy* and *damaged* segmented fixtures.** The salvage file (predating #649) had to
hand-write JSON because `ChunkMap`/`SegmentedMap`/`SegmentRecord` did not exist yet on
its base. On this base they do (already `pub`, already used by
`crates/core/tests/segmented_map_resolution.rs`), so building the fixture with the
real, validating constructors is strictly safer — a typo in hand-typed JSON silently
changes *which* rule the fixture exercises, whereas a constructor either succeeds or
panics loudly in the test. This is a deliberate, small deviation from a literal
"transcribe the salvage bytes" reading of "extract and adapt... raw-record seeding
helpers" — I judged "adapt" to license this, and the fixture keys still land at the
real `seg:<nonce>:<epoch>:<index>` / `inode:<id>` bytes a store would hold (verified:
`metadata::encode`/`metadata::seg_key` are the same functions the resolver reads
back with). I did keep the raw-`WriteBatch`-put seeding shape (never `metadata::create`
or a publish path) per the brief, since no producer of segmented maps lands in this
slice.

**Test file scope: GC + scrub only, not the salvage mega-fixture's reconstruction /
rebalance / backfill / restore legs.** The brief is explicit here ("Take only the
fixture's GC/scrub legs... the reconstruction / rebalance / backfill / containment
legs belong to #651 — do not pull them forward... it must ship its own added file").
My file imports only `reconcile_step, Reconciled, GcContext, ScrubContext, Custodian,
FencedZone` plus `desired_state::*` from `wyrd_custodian` (plus `ExpiredPendingPolicy`,
which `GcContext` construction requires structurally and which is base-visible,
unchanged by this patch, and already imported the same way at
`crates/custodian/tests/gc.rs:37`) — never `BackfillContext`/`ReconstructionContext`/
`RebalanceContext`/`restore::*`.

## Self-review against the target's rubric (`AGENTS.md` §"Review rubric & protocol")

- *Metadata validation boundaries* (ADR-0045): the new `unresolvable` classification is
  exactly the "structural invariants surface as errors, never as values" split already
  established — `resolve_chunk_map`'s `Err(ChunkMapError)` is the structural signal,
  never silently turned into "owns nothing".
- *Absent or unsupported entries* (recurring defect class): this is the class the slice
  exists to close — GC/scrub previously could reach a silent-success (`Satisfied` over
  an incomplete set) or, on the interim #648 tree, a blanket abort; now every gap is an
  explicit `Blocked` + a named audit event, never silent.
- *Narrow trait seams* (ADR-0010/0016): no new dependency; `gc.rs`/`scrub.rs` still
  import only `traits`/`core`/`tracing`.
- Every new crate-adjacent test file carries `#![forbid(unsafe_code)]`
  (`crates/custodian/tests/segmented_map_consumers.rs:53`).
- Docs currency: done (`docs/design/architecture/06-runtime-view.md`), scoped to
  exactly the sentences the brief names.

## Refutation (the three required questions)

**(a) Genuine red?** Yes. Reverted `gc.rs`/`scrub.rs`/`reconciliation.rs` to base
(`git stash push` on just those three files) and ran
`cargo test -p wyrd-custodian --test segmented_map_consumers`: 3 of 4 tests fail
(assertion / `.unwrap()` panics on `Err(Store(SegmentedMapUnsupported { operation:
"gc::referenced_fragments" }))`), 1 passes non-discriminatingly (the store-fault
propagation leg, which was already `Err` pre-fix for an unrelated reason — noted
above, not a false green since the other three legs carry the RED signal for the
whole target). Then `git stash pop` restored the fix; the same run went 4/4 green.
Also confirmed the **existing** suite doesn't regress: `cargo test -p wyrd-custodian`
(all files, 60+ tests) green post-fix, and the one existing test whose premise this
slice retires (`crates/custodian/tests/gc.rs`) was rewritten and reverified.

**(b) Production path?** Yes. Every test drives `wyrd_custodian::reconcile_step` —
the fenced control point, never a parallel entry — with real `GcContext`/`ScrubContext`
wired to in-memory `MetadataStore`/`ChunkStore` doubles that implement the *actual*
trait seam (same shape as every other test in this crate), never a mock of
`gc::reconcile`/`scrub::reconcile` themselves. The resolver call inside
`referenced_fragments` is the real `wyrd_core::metadata::resolve_chunk_map`, unmodified.

**(c) Fixture includes the fault?** Yes. Criterion (2)/(3)'s "damaged" object is a
real committed root (real, valid `SegmentedMap` naming two segments) whose second
`seg:` record was genuinely never written to the store — confirmed in-test via
`metadata::resolve_chunk_map(...).await.is_err()` before any assertion on GC/scrub's
behaviour. The "store fault" leg injects a real, distinct `std::io::Error` at the
exact seam (`scan_page` over the segment group's own range) the production resolver
reads. Nothing is curated out: the healthy object in criterion (3) sits in the SAME
`MemMeta` as the damaged one and is scanned in the same pass.

## External dependencies

`typos` was present locally (`~/.cargo/bin/typos`) and ran clean over every touched
file. `docs-renderer` was not present; per the brief this degrades to warn-and-skip in
`cargo xtask ci`'s prose gate and is not itself a blocker for this bundle — no
NEEDS-HUMAN needed for it. No other dependency beyond the base Rust toolchain was
required; `cargo build`/`cargo test`/`cargo clippy`/`cargo fmt` all ran through the
standard `cargo` invocations already configured for this workspace.

## Formatting / commit-readiness

Ran `cargo fmt` over every touched file and re-verified `cargo fmt -- --check` clean;
`cargo clippy -p wyrd-custodian --all-targets` clean (no warnings); `cargo build
--workspace --tests` clean; `cargo test -p wyrd-custodian` (full crate, all existing +
new tests) green; spot-checked downstream consumers
(`cargo test -p wyrd-server --test custodian_day_one --test custodian_gc`) green,
confirming no regression from the `Reconciled` variant addition (grepped the whole
tree for an exhaustive `match` on `Reconciled`; there is none — every existing
consumer compares by `==`/`assert_eq!`, so no "mechanical migration" arms were
needed in this actual tree, unlike the brief's more general 22-file estimate).

## Budget

Production diff (gc.rs + scrub.rs + reconciliation.rs) is dominated by doc comments;
semantic (code) lines added are close to the brief's own ~78-line estimate for the
salvage's production delta. Test file is one new file (~450 semantic lines). Total
files touched: 6 (well under the ≤15 budget); nowhere near the ≤~1,500 semantic-line
ceiling.
