# Build notes — issue 681 (iteration 4), `passes-read-through-resolver-contained`

Target branch: `getwyrd/wyrd @ main` (base `339da46`). All `path:line` citations below are
**post-patch** line numbers in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`) unless the
text says "on the base".

## 1. What this attempt changed relative to iteration v3 (the carry-forward)

v3 passed C1/C2/C4 but was returned on **C3** (rebalance certifying over a malformed placement),
**T2** (size), **T3** (unbounded plan retention), **C5** (11/66 surviving mutants) and five
review-branch findings. Each is addressed below; nothing was re-submitted unchanged.

| carried-forward item | what iteration 4 does |
|---|---|
| **T3 — "choose a bounded plan representation"** (`reconstruction.rs:867`, `rebalance.rs:297` in v3) | Plans no longer retain a decoded map. A reconstruction plan holds the object's key, the **encoded** generation, the chunk index and **one** `ChunkRef` (`reconstruction.rs:765-777`); an evacuation plan holds the key + encoded generation (`rebalance.rs:86-101`). The decoded map is materialised **one object at a time** where the commit is built (`reconstruction.rs:661`, `rebalance.rs:372`). Retention is now `obligations × one metadata value` — a flat map is bounded by the single-value ceiling segmentation exists for (`crates/traits/src/lib.rs:746-752`) — never the namespace's decoded chunk lists. |
| **review findings 1–3 — inode-key validation removed** (`backfill.rs:94`, `rebalance.rs:207`, `reconstruction.rs:805` in v3) | The key grammar is validated again, and a row it refuses is **named and contained** rather than silently skipped *or* mutated: `backfill.rs:115`, `rebalance.rs:234`, `reconstruction.rs:816`. Pinned decision 3 still holds — the raw key is what the record is read, CAS'd on and named under; the parse is a *gate*, never a re-derivation. This mirrors the startup walk's own rule for the same row (`crates/core/src/metadata.rs:2155-2170`, action `unparsable-inode-key`). |
| **review finding 5 — a lost CAS over a live segmented generation counted but not refused** (`backfill.rs:251` in v3) | v3's `live_empty_placements` re-read is **deleted** (−28 lines). The remaining-population gauge is now counted at ONE site in the walk (`backfill.rs:159`) and decremented only when the fill actually lands (`backfill.rs:212`), so a conflicted record keeps the empties this pass read and no second read (and no second refusal path) exists to get wrong. |
| **review finding 4 — malformed chunk with an empty placement dropped from the gauge** | Factually impossible; recorded-rejected with the citation (`review-rejected.md`, `crates/core/src/metadata.rs:204-206`). |
| **C3/T5 — "rebuild the non-certifying malformed path"** (`rebalance.rs:259`) | Recorded-rejected with evidence, not silently ignored — see §4 and `review-rejected.md`. |
| **C5 — 11/66 surviving mutants** | Now **4/74**, and all four are equivalent mutants (§5). |
| **T2 — size** | Every cap met (§6); the one overrun is the test file's *semantic* allocation, quantified there. |

## 2. The change, per file

**`reconstruction.rs`** — the pass reads the committed namespace **once** per pass
(`locate_queued_chunks`, `:790`) and indexes only the queued chunks it finds
(`CommittedIndex`, `:708`). `assess` then classifies each obligation against that reading
(`:390-410`) instead of running its own `inode:` scan per obligation (the base's `find_chunk`,
`reconstruction.rs:620` on the base). Every object is resolved through the shared resolver, and a
record that will not decode / a key the grammar refuses / a generation the resolver cannot read is
named and contained (`:724`) with the walk going on — `gc::referenced_fragments`' rule
(`gc.rs:360-455`), applied here a third time. A queued chunk whose reference lives in a `seg:`
record, or that two committed maps claim, is **refused** (`:259`) — obligation kept, nothing
written, pass answers `Blocked` (`:317`). An obligation with no site is drained **only** over a
complete reading (`:405`).

**`backfill.rs`** — same resolve + contain shape (`:96-134`); a segmented record is declined,
left byte-identical, and its empty placements stay on `backfill_placement_remaining`
(`:159-168`). The gauge is counted in the pass's own walk, so the second resolving reading the
base's `emit_remaining` performed (`backfill.rs:171-190` on the base) is gone.

**`rebalance.rs`** — the evacuation scan resolves per object (`:236-268`); a fragment whose chunk
lives in a `seg:` record stays on the draining server, refused **once per object**
(`:295-297`, `:319-321`), and the drain is not reported satisfied (`:144`).

All three: the write is framed by, and CAS'd on, the generation the **resolver** answered for —
never a scan snapshot a restart moved off (decision 4; `backfill.rs:185`,
`reconstruction.rs:867`, `rebalance.rs:311-316`). This is the one place I changed my own design
mid-build: an earlier draft CAS'd on the *scanned row's* bytes, which for a snapshot that was
superseded mid-resolve would have framed the live generation's chunks with the retired
generation's headers. It is unreachable in this slice either way (a flat map resolves to a borrow
with no restart, `metadata.rs:2584-2586`), but the code now cannot express the bug.

## 3. Alternatives considered, with their costs

- **Retain the decoded record per plan (v3's shape, `Arc<InodeRecord>` + `Arc<[ChunkRef]>`).**
  Rejected: it is exactly what T3 failed. Cost measured: with Q obligations spread over Q objects
  it retains Q decoded chunk lists; the reviewer's worst case (Q ≈ N) is the whole namespace's
  decoded maps. The chosen shape retains Q `ChunkRef`s + Q encoded values.
- **Re-read the record at commit time (`resolve_current_chunk_map` per plan) instead of retaining
  its bytes.** Rejected on correctness, not size: the survivors were gathered against the
  *assessed* placement, so committing against a freshly-read generation would need an equality
  re-check (~10 lines in each of two files) and, without it, would clobber a racing writer whose
  CAS we would now pass. Retaining the resolved generation's bytes keeps the CAS honest for free.
- **Making rebalance's pre-existing malformed-placement skip non-certifying** (the C3 finding).
  Rejected with evidence, not on cost: `crates/custodian/tests/rebalance.rs:1455-1459` asserts
  `Reconciled::Satisfied` for exactly that store, and the brief forbids editing that suite. See
  §4.
- **A racing-writer double in the discriminator to cover backfill's conflict path.** Rejected on
  cost with the number: `tests/backfill.rs`'s `RacingMeta` is 55 lines (`backfill.rs:105-181`),
  which the 780-raw-line test cap cannot absorb. Instead the accounting was restructured so the
  conflict path has **no counter site of its own** (§1, row 3) — the mutant that motivated the
  leg is gone rather than covered.
- **A shared `Refusals`/attribution helper in one module, `use`d by the other two.** Rejected:
  the budget is "exactly 4 files" and each pass owns its own audit target and counter name, so a
  shared type would couple three seams to save ~10 lines.

## 4. Two findings answered by rejection, not by code (see `review-rejected.md`)

1. **Non-certifying malformed placement (C3 / T5, `rebalance.rs:259`).** Out of this slice by the
   brief ("This slice makes only the refusal **it introduces** non-certifying"), and the base
   behaviour is *asserted* by a suite the brief forbids editing
   (`crates/custodian/tests/rebalance.rs:1455-1459`). C-1 is not violated: the operator-facing
   drain answer for that store is `ReconciliationStatus::PendingMalformed { chunks: [CHUNK] }`
   (`crates/custodian/tests/rebalance.rs:1487-1495`) — the decommission is *not* reported safe.
2. **Seeded Tier-0 DST coverage.** The brief pre-declared this as recorded-rejected; the reason
   is pasted verbatim into `review-rejected.md`. I checked the premise as the brief asked: **no
   commit path in this slice can be reached through a restarted resolve** — every write is on a
   flat generation, and a flat map resolves to `Answer(Cow::Borrowed(chunks))` with no store read
   and no supersede check (`crates/core/src/metadata.rs:2584-2586`). The reasoning stands.

## 5. Mutation analysis (advisory C5)

`scripts/mutants-in-diff` on the final patch: **74 mutants, 4 missed, 31 caught, 39 unviable**
(v3: 66 / 11 / 26 / 29). The four survivors are all the same equivalent-mutant class — deleting
`size:` or `state:` from an `InodeRecord { …, ..prior.clone() }` literal, where `..prior.clone()`
supplies the identical value (`reconstruction.rs:672`/`:674`, `rebalance.rs:419`/`:421`). They sit
on pre-existing lines the patch only re-touched (`plan.prior.size` → `prior.size`) and no test can
distinguish them.

What made the difference, deliberately: **every containment and refusal goes through one entry
point that names *and* counts** (`CommittedIndex::cannot_account_for`, `reconstruction.rs:724`;
`Refusals::{cannot_account_for,declined}`, `backfill.rs:246`/`:253`;
`Refusals::{cannot_account_for,refuse}`, `rebalance.rs:169`/`:176`), so a site can neither name
without counting nor count without naming — and each entry point is bound by a leg that exercises
*only* it (leg 2 for refusals, leg 3 for containment).

## 6. Budget (measured, not estimated)

`git diff` of the four files, "semantic" = non-blank, non-comment added lines:

| file | raw added | semantic added | allocation |
|---|---|---|---|
| `src/reconstruction.rs` | 344 | **180** | ≤ 210 ✓ |
| `src/backfill.rs` | 176 | **80** | ≤ 100 ✓ |
| `src/rebalance.rs` | 189 | **93** | ≤ 100 ✓ |
| `tests/segmented_map_passes.rs` (new) | **758** | **525** | ≤ 780 raw ✓ / ≤ 470 semantic ✗ |
| **total** | **1467** | **878** | ≤ 1520 raw ✓ / ≤ 880 semantic ✓ |

`patch.diff` is **99,949 bytes**, under the driver's 100 KB backstop.

**The one overrun is the test file's semantic allocation: 525 vs 470 (+55).** It is spent on two
things the cycle explicitly asked for, and I would rather name them than shave assertions:

- The C5 carry-forward required the two damaged shapes to be **independently** binding, so leg 3
  seeds each one in its **own** store (the loop at `tests/segmented_map_passes.rs:583-620`). A
  single store carrying both — v3's shape — answers `Blocked` with either containment removed,
  which is precisely the mutant that survived. Cost of the split: ~20 semantic lines.
- Pinned decision 3 needs a third store (`inode:007` beside `inode:7`, then `inode:not-an-id`,
  `:622-656`): ~25 semantic lines.

The four compression rules the brief names were applied (one metadata double, one seeding helper,
six tests each driving all three passes, one audit-capture helper). The production files came in
**57 semantic lines under** their allocations, so the patch total still lands at 878 ≤ 880.

## 7. Refuting my own test (forced, recorded)

- **(a) Genuine red?** **Yes.** Through the project's own gate:
  `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` reverts the three production files, keeps the
  added test, and reports `test result: FAILED. 0 passed; 6 failed` →
  `run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red)`. All six fail on
  **behavioural assertions**, not compile errors — e.g. `reconstruction must not end the pass:
  reconciliation store access: reconstruction::find_chunk met a segmented chunk map`, `read ONCE,
  not once per obligation — left: 3, right: 1`, `ambiguity is no repair — left: Changed, right:
  Blocked`. No assertion names a symbol this patch introduces, which is why the red is
  behavioural and not `UNVERIFIABLE`.
- **(b) Production path?** **Yes.** Every leg drives the real entries: reconstruction and
  rebalance through `wyrd_custodian::reconcile_step` — the fenced control point, with
  `Custodian::elect` + `FencedZone` over `MemCoordination` (`tests/segmented_map_passes.rs:428-450`)
  — and backfill through its own public `wyrd_custodian::backfill::reconcile` (`:458`). The only
  doubles are the `MetadataStore` / `ChunkStore` **seams**; no pass logic is re-implemented in the
  test, and the fragments are real erasure-coded bytes from the same two `core` calls the
  production rebuild makes (`:334-341`), so a gathered survivor has to pass the production
  `intact_shard` verify.
- **(c) Fixture includes the fault?** **Yes.** The seeding **asserts its own damage is real**
  before any leg runs — `assert_eq!(resolved.is_ok(), readable, "fixture: resolves")` (`:398`) —
  so a containment leg cannot pass because the damage quietly stopped being damage. The segmented
  object is a genuine `seg:`-backed root (raw `seg:` records + a segmented root, never a
  committer, `:377-395`), the damaged objects sit **first** in the store's key order over a
  `BTreeMap`-backed double (`:227-236`, `:62`) so the walk meets them before any healthy object,
  and the store fault of leg 6 is injected into the double's own `get` (`:75-80`) rather than
  curated out.

Additional evidence beyond the three questions: `./engine/xtask.sh ci` (fmt, clippy `-D warnings`,
build, the whole workspace test suite incl. DST, cargo-deny, conformance) passes on the final
tree, and the three per-pass suites `crates/custodian/tests/{reconstruction,backfill,rebalance}.rs`
are green **unmodified**, as the brief requires.

## 8. Environment

No external dependency beyond the base Rust toolchain was needed; the brief's five registered
doctor.checks ids were all present. **No NEEDS-HUMAN external dependency to declare.**

`cargo fmt` (the target's configured formatter) was run over every touched file; `cargo clippy
--all-targets` is clean under the workspace `-D warnings` policy, so the patch is commit-ready for
the target's hooks.

## 9. Self-review against the target's `## Review rubric & protocol`

- *One clock per correctness lifecycle*: no clock read added; `now_millis` is threaded exactly as
  before.
- *Narrow trait seams / dependency direction*: no new dependency and no `Cargo.toml` change; the
  loops stay on `traits` / `core` / `tracing` (`bytes` is a dev-dependency only, which is why the
  retained generation is an `Arc<[u8]>` and not a `Bytes`).
- *Metadata validation boundaries (ADR-0045)*: structural faults surface as typed errors and are
  contained per object by downcast; anything that is not a `ChunkMapError` still propagates
  (`reconstruction.rs:846-856`, and the same two arms in the other two files).
- *No DST-reachable shared mutable global state*: none added (the statics gate inside `xtask ci`
  passes).
- *Absent or unsupported entries — never silent success, silent skip, or a count-based assertion
  that can pass while the property fails*: this is the whole patch; and the discriminator asserts
  the gauge **by value** (`assert_gauge`, `:189-192`) rather than by "a line was emitted".
- *Serialization identity*: the CAS precondition is `encode(resolved.record)` — the generation the
  chunks were read from — so decode→encode identity is exercised on every commit this pass makes.
- *Docs currency*: no port, API operation, RPC, CLI flag or persisted field changes, so no doc
  hunk (checked at Plan, re-checked here).
- *Test fidelity — a new destructive or concurrent path lands with seeded Tier-0 DST coverage*:
  recorded-rejected with the reason the brief pre-declared; this slice adds no new destructive or
  concurrent path (§4).
