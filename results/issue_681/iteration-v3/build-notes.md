# Build notes — issue 681 / passes-read-through-resolver-contained

Target branch: `getwyrd/wyrd @ main`, base `339da46`. All `path:line` citations below are
against that base unless marked "(patched)". Worktree: `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`); the host's primary checkout was never touched.

---

## 1. What I built, and why this shape

The brief's `Scope` names one change: **the three passes that scan `inode:` resolve every
committed object the way every other consumer already resolves one, contain per object what
they cannot read, and refuse — rather than abort or silently discard — the work this slice does
not own.** The seven `as_flat().ok_or(SegmentedMapUnsupported)?` sites are gone; each pass now
resolves through `metadata::resolve_chunk_map` and contains by the exact `gc.rs:366-416` rule.

The brief made the **v2 production hunks the primary lever** ("salvage … with four named
corrections, not a re-derivation"). I took them and applied pinned decisions 3–6. What follows
is only the delta from v2; the unchanged salvage is v2's, which already passed C1–C5,
C4-verify red→green and mutation analysis.

### Correction for pinned decision 3 — the raw key (this is the biggest one)

v2 still committed under `metadata::inode_key(parse(key))`. `u64::from_str` accepts `+3` and
leading zeros, so `"inode:007"` and `"inode:7"` parse to the same id: the pass reads one record
and CASes another. Fixed in all three passes by carrying the **scanned key** and committing under
it:

| what | base | patched |
|---|---|---|
| reconstruction | `let inode_key = metadata::inode_key(plan.inode_id);` (`reconstruction.rs:598`) | `RepairPlan::inode_key: Arc<[u8]>` from the scan; `let inode_key = plan.inode_key.to_vec();` |
| backfill | `let inode_key = metadata::inode_key(inode_id);` (`backfill.rs:142`) | `.require(key.clone(), …).put(key.clone(), …)` |
| rebalance | `let inode_key = metadata::inode_key(plan.inode_id);` (`rebalance.rs:310`) | `EvacPlan::inode_key: Arc<[u8]>`, shared per object via `SharedGeneration` |

This **deletes** `parse_inode_key` from all three files (`reconstruction.rs:648`,
`backfill.rs:64`, `rebalance.rs:332`) — the write path no longer needs an id at all — and with it
v2's `REFUSED_UNADDRESSABLE` / `DECLINED_UNADDRESSABLE` refusal reasons, which existed only
because a key that would not parse had no CAS target. That is a *smaller* patch than v2's, not a
larger one: the fix removes the cause (the re-derivation) instead of guarding the symptom.
Attribution follows the same rule — `emit_backfilled` / `emit_conflict` now name the object by
`gc::object_name(&key)` rather than a parsed `InodeId`, mirroring `gc.rs:280-294`, `:402`.

This also closes the rubric's *Grammar strictness* defect class ("no `+`/`-` signs via
`from_str`") at the root rather than by tightening a parser.

### Correction for pinned decision 5 — one refusal line per object

v2's rebalance called `emit_refused(chunk.id, …)` inside the per-chunk loop. Now the object's
refused fragments are tallied into `refused_chunks` and reported **once**, after the chunk loop,
with the count — matching backfill's `emit_declined(object, reason, chunks)` shape exactly.
Bound by leg 2's sub-assertion over a segmented object with two draining fragments.

### Correction for pinned decision 6 — attribution before the work

All three `emit_unresolvable` calls sit at the point the record is read: reconstruction's inside
`locate_queued_chunks` (which runs before both the assess loop and the repair loop), rebalance's
inside `plan_evacuations` (before the evacuation loop), backfill's in the walk that also fills.
Mirrors `gc.rs:159-166`. Bound by leg 6.

### Correction for pinned decision 4 — framing (constraint, no leg; see §4)

Every commit is CAS'd on, and framed by, `resolved.record` / `resolved.chunks` — never the scan
snapshot the resolve may have moved off.

---

## 2. Rejected alternatives, with the cost shown

**(a) Keep `find_chunk`'s per-obligation scan and only swap in the resolver.** Rejected on the
brief's leg 5 and on C-1-through-the-scheduler: `assess` calls `find_chunk` per obligation
(`reconstruction.rs:322`) and `find_chunk` scans all of `inode:` (`:624`), so wiring the resolver
in there costs **Q×N resolves**. Measured on the counted double in the discriminator: base does
**3** `scan(b"inode:")` for Q=3, this patch does **1** — and with S=2 segmented objects present it
spends **2** bounded `seg:` range reads, not 6. The cost of the chosen fix is the
`CommittedIndex` / `Site` / `FlatSite` structure: **49 added semantic lines** in
`reconstruction.rs` (`locate_queued_chunks` + `insert_site` + the two types). The rejected
alternative is ~5 lines and does not converge as a store grows.

**(b) A second resolving walk for backfill's remaining-placement gauge** (what `emit_remaining`
did on the base, `backfill.rs:171-190`). Rejected on the brief's Bounded-work constraint: on a
store of S segmented objects it doubles every `seg:` range read for a number the pass has already
seen. Cost of the chosen fix: `remaining` accumulated in the walk (2 lines) plus
`live_empty_placements` for the lost-CAS case (**19 added semantic lines**, bounded by the
*conflicts* a pass meets, not by the namespace). The cheap alternative — count `to_fill.len()` on
a conflict, 1 line — was rejected because that count describes a generation that is gone; it is
v2 salvage and removing it would be a fifth, unrequested correction.

**(c) Contain the resolver error without the downcast** (treat any `Err` from
`resolve_chunk_map` as "this object is unreadable"). Rejected: that is over-containment — a store
outage would be reported as one damaged record and the pass would answer `Blocked` instead of
`Err`, telling an operator to go repair a record that is fine. The downcast rule is copied
verbatim from `gc.rs:405-415` and is bound by leg 6.

**(d) Share ONE `inode:` walk across all the loops.** Out of scope per the brief; it would change
gc/scrub/restore. Not attempted.

---

## 3. The discriminator — `crates/custodian/tests/segmented_map_passes.rs` (NEW, 775 lines)

Six legs, each driving **all three passes over one store**. Rebuilt from scratch to the brief's
four compression rules, not trimmed from v2's 1,185-line fixture:

* **ONE metadata double** (`MemMeta`) that is `BTreeMap`-backed *and* carries the two counters
  leg 5 reads *and* the injected `get` fault leg 6 needs. Its `scan_page` delegates to
  `wyrd_testkit::test_double_scan_page` as `segmented_map_consumers.rs:109-116` does.
* **ONE parameterised seeding helper** (`Store::seed` + the `Shape` enum), which every leg calls
  with different arguments; `Store::seed_fixture` composes the one shared store that legs 1, 2, 3
  and 6 all reuse.
* **SIX tests**, never one per pass. Reconstruction and rebalance run through the *real* fenced
  control point `reconcile_step`; backfill through its own public entry.
* **ONE audit-capture helper**, used by legs 2, 3, 4 and 6.

Every pinned decision is bound as a sub-assertion of an existing leg, per the brief's mapping
table (1→leg 4, 2→leg 3, 3→leg 3, 5→leg 2, 6→leg 6; 4 is a constraint with no leg, see §4).

**One leg I added to the brief's list of assertions** (inside leg 3, ~5 lines): `C_UNSEEN` is a
chunk referenced *only* by the object whose map cannot be read, and its obligation must survive
the pass. This is the sharpest edge of C-1 — "I could not read the map" vs "no committed map
references this chunk" — and it is a path **this patch creates**: on the base the walk `Err`s
before it can be reached, so `Assessment::Refused(REFUSED_INCOMPLETE)` was otherwise the one new
arm no test bound. Verified binding by mutation: replacing that arm with `Assessment::Drain`
makes leg 3 fail with `left: [] right: [210]` ("never drained for want of a reading").

### The three refutation questions

* **(a) Genuine red? YES.** Proven twice. Manually: with the three production files reverted
  (`git checkout`) and only the test kept, all 6 legs **compile and fail on behavioural
  assertions** — `find_chunk met a segmented chunk map` (legs 1/2/3), `Changed != Blocked`
  (leg 4), `left: 3 right: 1` scans (leg 5), `absorbed a store fault` (leg 6). And through the
  project's own runner: `PDCA_BUNDLE=results/issue_681 ./engine/scripts/run-verify.sh` →
  **`PASS — red without the fix, green with it (6 test(s) ran red)`**, exit 0. No leg names a
  symbol this patch introduces, so the red is behavioural, never a compile failure (the brief's
  exit-77 hazard).
* **(b) Production path? YES.** The legs drive `wyrd_custodian::reconcile_step` (the real fenced
  control point — the anti-#141 entry, `reconciliation.rs:104`) and
  `wyrd_custodian::backfill::reconcile` over in-memory `MetadataStore` / `ChunkStore` doubles.
  Nothing is re-implemented: the resolver exercised is the production
  `wyrd_core::metadata::resolve_chunk_map`, the fence is a real `Custodian::elect` +
  `FencedZone::new` over `wyrd_coordination_mem::MemCoordination`, and the fragments are built
  with the production `erasure::encode` + `write::encode_ec_fragment`, so a gathered survivor has
  to pass the real `repair::intact_shard` and an evacuated one the real
  `repair::fragment_intact`.
* **(c) Fixture includes the fault? YES, and it asserts the fault is real.** The segmented
  object, the root naming a `seg:` record that was never written, and the record whose bytes will
  not decode are all seeded **in the store the passes walk** — and `Store::seed` asserts each one
  (`resolve_chunk_map(…).is_ok() == readable`, `decode::<InodeRecord>(UNREADABLE).is_err()`), so
  a leg can never pass because the damage silently stopped being damage. The `BTreeMap` ordering
  makes "the damaged records are met FIRST" a property of the fixture (`inode:1` < `inode:2` <
  `inode:20` < `inode:3x`), not of luck. Leg 6's store fault is injected into that same store,
  after the seeding, and the leg asserts the *exact* injected text came back.

The existing per-pass suites `tests/{reconstruction,backfill,rebalance}.rs` are **unmodified and
green** (16 `test result: ok` in `-p wyrd-custodian`; 160 across `--workspace --exclude wyrd-dst`,
0 failures).

---

## 4. The pre-declared Tier-0 DST rejection — checked, and one precision correction

The brief asked me to say so if I found a commit path reachable through a **restarted** resolve.

**The core reasoning holds.** `resolve_snapshot` returns `Resolution::Answer(Cow::Borrowed)` for
`ChunkMap::Flat` with no store read and no supersede check (`metadata.rs:2584-2586`), so
`Resolution::Superseded` — the only arm that restarts (`metadata.rs:2629`) — can arise **only**
from a segmented snapshot. In all three passes every segmented outcome is refused before any
write is prepared (`is_segmented()` is checked before a `Site::Flat` is built, before
`next_chunk_map` is materialised, and before any `plans.push`). So no *segmented* generation is
ever written and no new race is added to a path that commits.

**One precision correction to the brief's wording**, for the record — it does not falsify the
rejection, and I did not add a DST file:

> the brief says "*every commit this slice performs is framed by the scan snapshot exactly as
> today*".

Strictly, there is one reachable shape where it is not: a **segmented** snapshot superseded
mid-resolve by a **flat** live generation. The resolver restarts, `resolved.record` is that flat
generation, `is_segmented()` is false, and the pass takes the write path. That is exactly what
pinned decision 4 is written for, and this patch does the right thing: the precondition
(`require(key, encode(resolved.record))`), the preserved fields and the chunk list all come from
`resolved.record` / `resolved.chunks`, never from the stale snapshot. The write stays
version-conditional against a generation that was live at resolve time, so losing the race is a
`Conflict`, never a clobber — no data-losing path. And it is unreachable in a deployed system
today regardless: nothing publishes a segmented map yet (#653 owns the committer), so no object
can transition segmented→flat. Recorded so sign-off sees it as a checked item rather than a gap.

---

## 5. Budget — measured, with one dimension over

| dimension | measured | budget | |
|---|---|---|---|
| files | **4** | exactly 4 | ok |
| total raw added | **1490** | ≤ 1520 | ok |
| `tests/segmented_map_passes.rs` raw | **775** | ≤ 780 (the brief's STOP line) | ok |
| patch size | **97.5 KiB** | < 100 KiB driver backstop (`size_signal.py:251`, `patch_bytes/1024 >= 100`) | ok |
| `src/reconstruction.rs` semantic | **179** | ≤ 210 | ok |
| `src/backfill.rs` semantic | **96** | ≤ 100 | ok |
| `src/rebalance.rs` semantic | **92** | ≤ 100 | ok |
| `tests/…passes.rs` semantic | **528** | ≤ 470 | **58 over** |
| total semantic | **895** | ≤ 880 | **15 over (1.7%)** |

(Measured with the same script that reproduces the brief's own v2 numbers to ±1: v2 scores
373/192, 192/95, 162/88, 1185/810.)

**Why the test file is 58 semantic lines over its allocation, and what I traded to get there.**
All four compression rules were applied (one double, one seeder, six tests, one capture helper),
and the file is **35% smaller than v2's** (1185→775 raw, 810→528 semantic) — the narrowing the
brief asked for landed. The residual is rustfmt, not fixture bloat: with `fn_call_width = 60` and
`struct_lit_width = 18`, an `assert_eq!(a, b, "why")` whose arguments exceed 60 characters is
split across five lines, and every struct literal wider than 18 characters goes vertical. I spent
a full pass converting ~20 assertions to `let named = …; assert!(named, "short why");` to keep
them single-line, which is where the last ~70 lines came from. Going further means deleting the
assertion messages, i.e. trading the diagnosability of a failing durability test for ~50 lines —
I judged that the wrong trade and stopped, rather than reach for `#[rustfmt::skip]` (I tried it,
got under the line, and **reverted it**: winning a budget by suppressing the formatter is gaming
the number, not meeting it). The hard STOP conditions the brief states — a fifth file, or a test
file past 780 raw lines — are both met with margin, as is the patch-size backstop.

If the human wants the semantic total inside 880 at sign-off, the honest lever is dropping ~15
assertion messages; say the word and it is a five-minute edit.

---

## 6. Commit-readiness (the target's own hooks)

`/home/eddie/wyrd/wyrd/.git/hooks/` has **no active hooks** (samples only), and `CONTRIBUTING.md`
names `cargo xtask ci` as the contributor gate. Run in the worktree, all clean:

* `cargo fmt --all -- --check` → clean
* `cargo clippy --workspace --all-targets` → no errors, no warnings (`warnings = "deny"`,
  `clippy::all = "deny"`)
* `cargo test --workspace --exclude wyrd-dst` → 160 `test result: ok`, 0 failures
* `typos` (repo-wide) → exit 0
* `cargo doc -p wyrd-custodian --no-deps` → **14 errors on the base, 14 with the patch** — all
  pre-existing, all in files this patch does not touch (`gc.rs:27`, `gc.rs:29`,
  `reconciliation.rs:94-95`, …). This patch adds none.

## 7. Rubric self-review (`AGENTS.md` § Review rubric & protocol)

* *One clock per correctness lifecycle* — no clock read added; `now_millis` flows unchanged.
* *Narrow trait seams / dependency direction (ADR-0010, ADR-0016)* — custodian still depends only
  on `traits` / `core` / `tracing`. **No `Cargo.toml` change**; every dev-dependency the
  discriminator uses was already declared.
* *Metadata validation boundaries (ADR-0045)* — structural invariants surface as errors at decode
  and are contained per object as errors, never turned into values; the contextual placement
  check (`checked_fragments`) stays strict in the maintenance path.
* *No DST-reachable shared mutable global state* — none added. The test's `static INIT: Once` is
  the same test-only tracing guard `segmented_map_consumers.rs:326-332` uses.
* *`#![forbid(unsafe_code)]`* — no new crate; the new test file carries it as its siblings do.
* *Docs currency* — no port, API operation, RPC, CLI flag or persisted field changes: the refusal
  writes nothing, the new signals are `tracing` events, and `Reconciled::Blocked` already exists.
  Per the brief's check, `06-runtime-view.md` §6.2's clause stays true. **No doc hunk**, as
  instructed.
* *Absent or unsupported entries: never silent success, silent skip, or a count-based assertion
  that can pass while the property fails* — this is the slice. Every refusal is explicit (stated
  reason on the audit seam + counter + non-certifying `Blocked`). The legs assert properties
  (placement moved, obligation still queued, `seg:` bytes byte-identical, fragment still on the
  draining server), not proxies; leg 5's counts **are** the property and are paired with "the
  repairs still land".
* *Transactions* — no live transaction is held across an early return; each commit is one
  `WriteBatch`.
* *Await discipline* — the resolver awaits carry the `MetadataStore` implementation's bound, the
  identical rule `gc.rs:394-401` records for the identical call; cited in place. No spawned tasks.
* *Serialization identity* — the CAS precondition is still `encode(prior)` against the stored
  bytes, unchanged from the base; for a flat map `resolved.record` is the scan's own record, so it
  encodes identically.
* *Test fidelity / seeded Tier-0 DST* — settled at Plan as recorded-rejected; re-checked at Do,
  see §4. No DST file shipped, deliberately.

## 8. Housekeeping

Scratch: `"${PDCA_SCRATCH:-${TMPDIR:-/tmp}}/pdca-builder-681-work"` — removed at the end of the
run. No PR was pushed, opened, or marked ready.
