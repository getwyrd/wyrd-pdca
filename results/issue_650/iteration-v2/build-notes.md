# Build notes — issue 650 / gc-scrub-through-resolver-fail-closed-containment

**Iteration 2.** Target branch `pdca-integration/main` (`stack-base`), base `4e78aeb`
(= `origin/pdca-integration/main`, which already carries #648 and #649). Every `path:line`
below is on the patched worktree `$PDCA_WORKTREE` unless it says "base".

## What the carry-forward asked for, and where each answer landed

| Carry-forward item | Answer |
|---|---|
| **C5 [impl]** — a segment-count-mismatched *committed* root returned `Err` at the unconditional decode, ending the walk | `crates/custodian/src/gc.rs:371-377` — `metadata::decode` is now matched, not `?`d: a record whose own bytes will not decode is contained per object exactly like an unreadable generation. Bound by `crates/custodian/tests/segmented_map_consumers.rs:722` (a raw root whose `segment_count` disagrees with its table) |
| **T5 [impl]** — inode naming was code-read only; an audit regression would pass | `crates/custodian/tests/segmented_map_consumers.rs:272` (`assert_attributes_blocker`) + `:569`/`:590`: the pass runs under a capturing subscriber and the leg asserts seam + action + **`"inode":"inode:1"`** on both the GC and scrub audit trails. Refuted by mutation — see (d) below |
| **C4** — inherited `cargo deny` red, RUSTSEC-2026-0221 on `event-listener` 5.4.1 | `Cargo.lock` — `cargo update -p event-listener` → 5.4.2, the advisory's own stated fix. `cargo deny check advisories` now `ok`, and the whole `cargo xtask ci` exits 0 |
| **T4** — 19 batched-review findings, 0 recorded-rejected | 17 fixed in the patch (table below); 2 recorded-rejected in `review-rejected.md` **in the parser's format** (`<file:line> \| <CLASS> \| <MATCH> \| <reason>`). Round 1's file was prose only, which is why the gate reported `0 recorded-rejected` — that was the actual gate defect, not a missing argument |

### The 19 review findings

Fixed (they leave the next run on their own):

* *GC-specific telemetry inside the shared builder* (×4, `gc.rs:336`) — `emit_unresolvable`
  moved out of `referenced_fragments` into the GC loop (`gc.rs:163-165`), and scrub emits its
  own (`scrub.rs:113-116`). The shared build now returns data and emits nothing, so a scrub,
  a restore or a drain-status query can no longer tick a `gc_` counter.
* *A blanket protection audited as `"referenced"`* (×3, `gc.rs:174`/`:283`) — the set now
  answers **why** it protected a fragment (`ReferenceSet::protection`, `gc.rs:299-309`), and
  the skip is filed under `"incomplete-reference-set"` when that is what actually held.
  `protects` is `protection(..).is_some()` (`gc.rs:324`), so there is still exactly one rule.
* *`reconciliation_status` certifying a drain over a partial set* (×2, `gc.rs:338`) — the
  drain-status surface gains `PendingUnresolvable { objects }` (`desired_state.rs:118-134`,
  returned at `:209-214`), the same "attribute the blocker, name it, keep answering" shape
  `PendingMalformed` already uses. This was a genuine *regression* the round-1 patch
  introduced: on base an unreadable map made `referenced_fragments` `Err`, so the query failed
  closed; round 1 made it a partial set and the query answered `Satisfied` — literally the
  brief's own sentence about telling an operator to decommission a server a live object may
  still own bytes on.
* *`assert_ne!(Satisfied)` permits `Changed`* (×3) — every outcome assertion in the added file
  now excludes **both** base-visible variants (`segmented_map_consumers.rs:559-563` and
  peers). Still no compile dependency on `Blocked`.
* *No test combines a `Blocked` loop with a `Changed` loop* — `segmented_map_consumers.rs:874`
  runs `reconcile_step` with two different stores (a blocked GC beside a converging scrub, and
  the reverse), so the precedence is bound **through production** in both loop orders.
* *The continuation test could pass on an aborted walk* — the store double is `BTreeMap`-backed
  (`segmented_map_consumers.rs:79-81`) and the damaged object is `inode:1`, the healthy one
  `inode:2`, so the healthy object is reachable **only** if the walk continues.
* *The store-fault leg accepted any error* — it now destructures `ReconcileError::Store` and
  asserts the injected text (`segmented_map_consumers.rs:851`). This is what turned that leg
  from a non-discriminating pass into a red one (round 1: 3 of 4 red; now 6 of 6).
* *No seeded Tier-0 DST coverage for the destructive segmented path* (×3) — property 10,
  `crates/dst/tests/custodian.rs:1518`, three seed-drawn arms: healthy, **retired
  mid-reference-build** (#649's `RetireMidResolve` nemesis, now met by the pass that *deletes*
  rather than the one that reads), and **incomplete**. It asserts the live generation's
  fragments survive, a genuine past-grace orphan IS reclaimed, and the incomplete arm reclaims
  nothing and answers `Blocked`.

Recorded-rejected: the two *bounded-await* CONVENTION findings on the resolver await —
standing rejection (i) (#508/#636: the store implementation owns the network bound). The
reasoning and the three checkable facts are in `review-rejected.md`; the short version is that
the same function's pre-existing `meta.scan(b"inode:")` await is unbounded on base, no await in
any of the four loops carries a caller-side timeout, and this await is already fail-closed.

## The change

* **`crates/custodian/src/gc.rs`**
  * `ReferenceSet::unresolvable: BTreeMap<String, String>` (`:287`) — object key → the fault
    that stopped it. A map, not the salvage's `BTreeSet`, so the consumer that emits can name
    *what* went wrong without the builder emitting anything itself.
  * `protection` / `protects` (`:299`, `:324`) — one predicate, now with a reason.
  * `referenced_fragments` (`:352`) resolves each committed record through
    `wyrd_core::metadata::resolve_chunk_map` (`:386`) — #649's landed resolver, the same one
    `crates/core/src/read.rs` uses. Four arms, deliberately distinct:
    `Ok(Some)` → expand; `Ok(None)` → skip (no live committed generation);
    `Err(ChunkMapError)` → contain (`:389-395`); any other `Err` → propagate (`:398`).
    Plus the decode arm above it (`:371`) for a record that will not decode at all.
  * `gc::reconcile` answers `Reconciled::Blocked` over an incomplete set (`:241`) and emits the
    attribution before the fleet walk (`:163`).
* **`crates/custodian/src/scrub.rs`** — inherits the set, emits its own attribution (`:115`,
  `:229`), and answers `Blocked` under the identical condition (`:210`). Deliberately the same
  rule, not a second one.
* **`crates/custodian/src/reconciliation.rs`** — `Reconciled::Blocked` (`:44`) and
  `least_certified` (`:55`), which `reconcile_step` folds every loop's outcome through
  (`:118`…`:139`).
* **`crates/custodian/src/desired_state.rs`** — `PendingUnresolvable` (`:124`, `:213`).
* **Tests** — the new discriminator file, plus the positive-variant legs the discriminator
  cannot carry: `tests/gc.rs:812` (`Reconciled::Blocked`, replacing the #648 test whose premise
  — abort the whole pass on the *shape* — this slice retires), `tests/scrub.rs:1033` (the same
  answer from scrub, and the healthy object still verified), `tests/rebalance.rs:1518`
  (`PendingUnresolvable { objects: ["inode:9"] }`), `crates/dst/tests/custodian.rs:1518`.
* **`docs/design/architecture/06-runtime-view.md:31`** — the containment sentences the brief
  names, and only those; the repair/evacuation-walk wording stays #651's.
* **`Cargo.lock`** — the advisory bump (2 lines).

## What I ruled out, with the cost

**Uncommitted-but-undecodable records could avoid blocking the fleet — not taken, and here is
what it would have cost.** The closed PR's `classify_root` distinguished
`Root::UncommittedUnreadable` (attribute, do *not* block) from `Root::Unresolvable`. It is a
real refinement: a corrupt *pending* record authorizes nothing, so freezing reclamation for it
is over-strict. Reading `state` out of bytes that will not decode needs a lenient peek, and
there are exactly two ways to get one here:

* add `serde` + `serde_json` to `crates/custodian/Cargo.toml` (2 dependency lines) and a
  private `#[derive(Deserialize)] struct RootPeek { state: InodeState }` (~8 lines in gc.rs) —
  which breaches the boundary this crate's own module docs state and `scrub.rs:32-35` restates
  ("the loop stays over the `traits` / `core` seams plus `tracing`… so `custodian` gains no
  chunk-format dependency and no new on-disk-format knowledge"); or
* add `pub fn peek_inode_state(bytes: &[u8]) -> Option<InodeState>` to
  `crates/core/src/metadata.rs` (~12 lines) — a new public core API in a file the brief's Scope
  does not list.

Both are small in lines and neither is small in *shape*, and the direction I took is the
fail-closed one: an unreadable record blocks until repaired. The recorded reasoning is in the
code (`gc.rs:363-370`), so #651/#653 can lift it deliberately rather than rediscover it.

**Blanket freeze, not a per-object-scoped one.** Unchanged from the settled decision (brief's
do-not-re-earn (iv)): scoping the protection to the unreadable object's own chunks needs those
chunk ids, which are exactly what an unreadable map withholds.

**`gc::reconcile` gets its own `Blocked` branch rather than leaving it to the combinator.**
Criterion (2) calls GC *alone*; a combinator-only fix would leave the single-loop answer
certifying. The combinator is still needed for the multi-loop case (leg 5).

**Restore is untouched, on purpose.** `restore::reconcile_after_restore` (base
`crates/custodian/src/restore.rs:183`, `:222`) gates its only write on the same `protects`, so
it inherits the containment for free — nothing is marked; and its report half still fails
closed on a segmented map through its own `committed_chunks`
(`crates/custodian/src/restore.rs:373-389`, unchanged), so this patch neither weakens nor
extends it. Routing restore through the resolver is #651's, as the brief's Ordering note says.

## Refutation (the three required questions)

**(a) Genuine red?** Yes — established by the project's own per-fix runner, not by hand:
`PDCA_BUNDLE=… PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh`
→ `PASS — red without the fix, green with it`. GREEN leg: 6 passed. RED leg (production
reverted, the added test kept): **0 passed, 6 failed** — all six assertion/`expect` panics on
base-visible symbols, no compile error (the RED log's only `error:` line is cargo's own "test
failed" summary). Sample messages: *"one unreadable object is contained, not an error that ends
the pass: Store(SegmentedMapUnsupported { operation: "gc::referenced_fragments" })"*, and for
the store-fault leg *"…got gc::referenced_fragments met a segmented chunk map, which this build
cannot yet resolve"*. Round 1 had one leg passing vacuously on the base; it does not now.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the fenced
control point — with real `GcContext`/`ScrubContext` over in-memory implementations of the
*actual* `MetadataStore`/`ChunkStore` traits (the shape every other test in this crate uses).
The resolver called inside `referenced_fragments` is the real
`wyrd_core::metadata::resolve_chunk_map`, unmodified. The drain leg calls the real
`desired_state::reconciliation_status`. The audit assertions read what the production
`tracing` callsites actually emitted, through a subscriber — no test-only hook.

**(c) Fixture includes the fault?** Yes, and the fixture asserts its own fault is real:
`seed_damaged` (`segmented_map_consumers.rs:385-397`) re-reads the seeded root and requires
`metadata::resolve_chunk_map(..).await.is_err()` before any leg asserts anything, so a leg can
never pass because the fault quietly stopped being one. The damaged object is met **first**
(`inode:1`, `BTreeMap`-ordered scan), never curated out; the healthy object sits in the same
store and the same pass. The store-fault leg injects a real `std::io::Error` at the exact
`scan_page` seam the production resolver reads, and asserts *that* error came back.

**(d) Extra refutation — the attribution leg.** I mutated production to drop `inode = %object`
from `emit_unresolvable` and re-ran: `one_unreadable_committed_inode_…` **FAILED** with
*"the audit line must NAME the record to repair… got: {…"fields":{"monotonic_counter.gc_unresolvable_records":1},"target":"wyrd_custodian::gc"}"*.
Restored, 6/6 green. So T5's finding ("an audit regression would pass") is closed by a test
that demonstrably goes red on exactly that regression.

## Gates run here (the project's own runners, never hand-rolled)

* `./engine/xtask.sh ci` — **exit 0**, `xtask ci: all checks passed`. Both prose gates ran for
  real on this host (`typos`, and `render_site.py --check` → `link audit OK`), so the
  laptop/CI asymmetry INTEGRATION §3 warns about does not apply to this run.
* `./engine/xtask.sh dst` — the madsim seed sweep, green, including the new
  `gc_over_a_segmented_map_never_reclaims_it_and_never_over_certifies` and the committed
  regression seeds.
* `./engine/scripts/run-verify.sh` — PASS (red→green), single discriminator confirmed by
  `--classify`: `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`.
* `cargo fmt --all -- --check` clean (commit-hook readiness); `cargo deny check advisories` ok.

## Budget

11 files (≤ 15). **1,030** semantic added lines (non-blank, non-comment) against ≤ ~1,500 —
production is **109** of them (`gc.rs` +56, `reconciliation.rs` +30, `scrub.rs` +16,
`desired_state.rs` +7), close to the brief's ~78 estimate plus the drain-status fix the review
demanded; the remaining 921 are tests and the DST property, which is where the brief said the
budget risk was. The anticipated mechanical `match`-arm migration over the new `Reconciled`
variant turned out to be **empty in this tree**: `cargo build --workspace --tests` is clean, so
no consumer matches `Reconciled` exhaustively (all compare by `==`/`assert_eq!`). Same for
`ReconciliationStatus`.

## External dependencies

Both registered ones were present and ran: `typos` (clean) and the docs renderer
(`markdown_it`/`yaml` → `render_site --check` OK). Nothing beyond the base Rust toolchain was
needed; no Docker, no protoc, no live backend, no new dev-dependency. **No NEEDS-HUMAN
external dependency for this bundle.**

## Scratch

`/var/tmp/pdca/pdca-builder-650-*` (three gate logs and one file backup) — removed before
handover.
