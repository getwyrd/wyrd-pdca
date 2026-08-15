# Build notes — issue 681 (iteration 2)

Target branch: `getwyrd/wyrd @ main`, base `origin/main` = `339da46`.
Worktree: `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`. All `path:line` citations below
are against that worktree (base + this patch), and every base citation was re-read at the file
rather than taken from the brief.

Files touched: **4** of the 8 allocated (`custodian/src/{reconstruction,backfill,rebalance}.rs`
+ the new `custodian/tests/segmented_map_passes.rs`). `docs/design/architecture/06-runtime-view.md`
was **confirm-only** and needed no edit — see §6. `tests/{reconstruction,backfill,rebalance}.rs`
needed no edit either: all 160 workspace test binaries pass unchanged (§5).

---

## 1. What this iteration changed relative to iteration 1

The previous attempt's *approach* was accepted by the reviewer on every mechanical row
(C1–C5, T1–T3 all PASS). It was rejected on two implementation findings and one judgment row,
all recorded in the carry-forward. This iteration keeps the approach — one resolver, per-object
containment, obligation-indexed single walk — and fixes exactly what was found. It is not a
re-submission: three production behaviours changed and four test properties were added.

### (A) BUG, seen by all three review passes — `reconstruction.rs:859` (iteration-1 numbering)

> *"Writability is classified from the stale scanned `record` instead of `resolved.record`."*

Fixed at `crates/custodian/src/reconstruction.rs:866-877`: the segmented/flat classification now
reads `resolved.record.chunk_map.is_segmented()`, i.e. **the generation the resolver answered
for**, which is also the generation `prior`/`prior_chunks` are taken from (`:893-894`) and the one
the repoint CASes on (`repair_chunk`, `:700`). Backfill (`backfill.rs:159-162`) and rebalance
(`rebalance.rs:232-235`) already classified from `resolved.record`; reconstruction was the odd one
out, and now all three read one generation end to end.

**On the reported mechanism, honestly:** the reviewers described a *flat→segmented* race letting
reconstruction CAS a flat map over a live segmented generation. I could not reach that: for a
**flat** snapshot `resolve_snapshot` returns `Answer(Cow::Borrowed(chunks))` without touching the
store (`crates/core/src/metadata.rs:2585`), so `resolved.record` **is** the scanned record and
the two can never disagree in that direction. The reachable defect is the mirror image — a
**segmented snapshot whose live root is flat**, where `root_dropped` reports `Superseded`
(`metadata.rs:2335-2339`) and `resolve_chunk_map` restarts onto the live root
(`metadata.rs:2629`): the stale read then refuses (`REFUSED_SEGMENTED`) a repair the live record
could take, so an under-replicated chunk is left unrepaired and the pass reports `Blocked`. That is
a redundancy-restoration liveness defect under C-1, not corruption — I fixed the line as reported
and am recording the mechanism accurately rather than repeating a story I could not reproduce.

Bound by a new leg, `reconstruction_repairs_the_generation_the_resolver_answered_for`
(`crates/custodian/tests/segmented_map_passes.rs:1061-1122`): a racing publisher lands a flat
generation *inside* the resolve (the `seg:`-range window), and the pass must repair the chunk in
the live record, bump **that** record's version 2→3, and drain the obligation. Reverting only the
one-line fix turns it red (`left: [0, 1, 2], right: [0, 1, 3]` — nothing repaired). Verified below.

### (B) BUG, seen by two review passes — `backfill.rs:214` (iteration-1 numbering)

> *"A CAS conflict does not prove the racing generation still has these empty placements."*

Fixed at `crates/custodian/src/backfill.rs:219-228` + `live_empty_placements`
(`backfill.rs:262-282`). On a lost CAS the pass no longer re-adds the **superseded** generation's
`to_fill.len()` to the drain-to-zero gauge; it reads the **live** generation once by key
(`metadata::resolve_current_chunk_map`, `crates/core/src/metadata.rs:2652`) and counts what that
record actually holds. `Ok(None)` (the winner deleted the object) contributes nothing; a live
generation that cannot be read is contained by the same per-object rule as the walk's own reads —
named on the seam, counted as nothing, and non-certifying.

Cost, since the brief asks for numbers rather than adjectives: **one `get`** per *conflicting*
record (plus that record's own bounded `seg:` range if the winner published a segmented
generation). Conflicts are racing writers, not the namespace, so this is not the second resolving
walk of `inode:` that the base's `emit_remaining` cost (`backfill.rs:171-190` on base) and that the
brief forbids ("Bounded work … not a second resolving walk").

Alternatives considered and rejected:
* **Keep the stale count** (conservative over-count, corrected next pass) — rejected: the gauge is
  the *only* evidence ADR-0040 decision 6 gates the identity-fallback removal on, and a number
  drawn from a record that no longer exists is a fabricated one whichever way it errs. Also, the
  finding must leave the review run (AGENTS.md "Definition of done"), and "it errs safely" is not a
  reason a recorded rejection could carry for a gauge whose whole job is to be believed.
* **Count nothing on conflict** (1 line cheaper) — rejected outright: it can read 0 while records
  still carry empty placements, i.e. it can falsely authorise the very removal the gauge gates.
* **Make a conflict non-certifying** (`refused += 1`, 1 line) — rejected: an ordinary lost CAS is
  not a refusal this slice introduces, the brief scopes that question out, and
  `crates/custodian/tests/backfill.rs:290-295` binds `Satisfied` there today.

### (C) T5 judgment — regression protection for the no-data-loss claim

> *"all three mutations of the incomplete-reading drain guard survive, and no test directly drives
> segmented backfill refusal or segmented evacuation refusal"*

* **The drain guard** (`reconstruction.rs:430`, `None if index.unreadable == 0 => Drain`) is now
  bound from **both sides**: an obligation for a chunk no committed map references is *drained*
  when the reading was complete (leg 1, `segmented_map_passes.rs:668-712`) and *kept* when it was
  not (leg 3, `:872-880`). I applied all three surviving mutations by hand — guard→`true`,
  guard→`false`, `==`→`!=` — and each turns the suite red (2, 1 and 3 legs respectively).
* **Segmented backfill refusal and segmented evacuation refusal** now have direct legs: leg 2
  (`:732-829`) drives all three passes over one segmented object carrying a queued repair, an empty
  placement **and** a fragment on a draining server, and asserts each refuses, each answers
  `Blocked`, the fragment stays put, the declined chunk stays on the gauge, and the store is
  **byte-identical** afterwards.
* **`cargo mutants --in-diff`** (the project's own C5 row, `scripts/mutants-in-diff`) went from
  **18 missed / 60 tested** (iteration 1) to **0 missed / 59 tested** on the same command.
  Two of the 18 were equivalent mutants (`delete field size` / `delete field state` from
  backfill's `next` `InodeRecord`, both re-supplied by `..record.clone()`); they left the diff
  because the fix now *shadows* `record` with the resolved generation (`backfill.rs:191`) instead
  of renaming it to `prior`, so those two lines are no longer modified at all — a smaller diff and
  an honest mutation score rather than a suppression.

---

## 2. The change itself

The seven `as_flat().ok_or(SegmentedMapUnsupported)?` sites the brief tabulates are gone; a grep
for `SegmentedMapUnsupported` or `as_flat()` under `crates/custodian/src/` now returns nothing.
Each pass reads every committed object through `metadata::resolve_chunk_map`
(`crates/core/src/metadata.rs:2619`) and contains what it cannot read by exactly the rule
`gc::referenced_fragments` established (`crates/custodian/src/gc.rs:378-416`), including the
downcast rule at `gc.rs:405-415`: `Ok(ChunkMapError)` is *this record's* fault (name it, continue),
anything else propagates because a walk that cannot reach the store has no answer for any object.

* **`reconstruction.rs`** — `find_chunk` (a namespace scan *per obligation*) is replaced by
  `locate_queued_chunks` (`:795-915`), one reading of `inode:` per pass, indexed by the chunk ids
  the queue actually names. Q×N → N. Retention is per *obligation* and per *object*: an object
  holding no queued chunk is not retained at all (`:851-863`), and an object holding several shares
  one `Arc<InodeRecord>` + one `Arc<[ChunkRef]>` across them (`:893-903`) rather than a deep copy
  per obligation. A `seg:`-resident chunk becomes `Assessment::Refused` — the obligation stays
  queued, **nothing is written**, and the pass answers `Blocked`. The duplicate-chunk-id rule the
  brief requires is the narrow one: `insert_site` (`:917-936`) turns a second committed reference
  to one chunk id into an ambiguity that repairs neither and names both — no report schema, no new
  verdict surface.
* **`backfill.rs`** — a segmented record is left byte-identical, declined with a stated reason on
  the audit seam (`emit_declined`) and still counted on the remaining gauge; the gauge is now
  accumulated in the pass's own walk (`:90`, `:231`) instead of a second resolving scan.
* **`rebalance.rs`** — the evacuation scan resolves per object; a fragment whose chunk lives in a
  `seg:` record is refused and **stays on the draining server**, and the drain is not reported
  satisfied (`EvacScan::refused`, `:158-165`).

`Reconciled::Blocked` and `least_certified` (`crates/custodian/src/reconciliation.rs:44`, `:55`)
are reused as-is; no parallel outcome, no new public API, no new dependency, no
`crates/custodian/src/resolve.rs` (the closed PR #647's custodian-local resolver stays dead —
`metadata::resolve_chunk_map` superseded it at #649).

---

## 3. The three forced questions

**(a) Genuine red?** Yes — through the project's own runner, not a hand-rolled command:
`PDCA_BUNDLE=results/issue_681 ./engine/scripts/run-verify.sh` →
`GREEN — cargo test -p wyrd-custodian --test segmented_map_passes (fix applied)`,
`RED — (production reverted, test kept)`, `PASS — red without the fix, green with it (8 test(s)
ran red)`. **All 8 legs fail on the reverted tree — `0 passed; 8 failed` — every one on a
behavioural assertion or on the pass returning `Err`, and the test file still COMPILES there**: no
leg names a symbol this patch introduces, so the red is "the behaviour was wrong", never "a symbol
is missing" (`run-verify.sh:487-497` would report the latter as UNVERIFIABLE/77).
Reverted-tree failures, in the passes' own words:
`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })`,
`SegmentedMapUnsupported { operation: "backfill::emit_remaining" }`,
`Store(Error("expected ident"))` (the undecodable record ending the walk), and
`ONE reading … left: 3 right: 1` (the Q×N scan).
Note on the eighth leg: `backfill_counts_the_live_generation_after_a_lost_cas` is red on the base
only through its **second** half (an unreadable racing generation, which the base's second
namespace scan cannot survive). Its *first* half — the lost CAS whose live generation holds one
empty placement, not two — passes on the base by construction (the base's second scan reads the
live record correctly) and is red against the **iteration-1 patch**
(`got … "backfill_placement_remaining":2` where 1 is correct). That half binds the regression the
single-walk gauge could introduce, which is the fix it exists to hold.
I additionally reverted each of the two review fixes **individually** on the patched tree and
re-ran: (A) → 1 leg red (`left: [0, 1, 2], right: [0, 1, 3]`), (B) → 1 leg red
(`"backfill_placement_remaining":2`), both → 2 legs red. So each fix is separately load-bearing,
not carried by the other or by the base's own defects.

**(b) Production path?** Yes. Every leg drives the real entries: `wyrd_custodian::reconcile_step`
(the fenced control point — the anti-#141 guard, never a test-only entry) for reconstruction and
rebalance, and `wyrd_custodian::backfill::reconcile` for the fill. The only doubles are the
`MetadataStore`/`ChunkStore` **seams** (in-memory `MemMeta`/`MemDServer`), which is how every
custodian test in this repo is written; no pass logic is re-implemented in the test. Fragments are
real on-disk-format bytes (`encode_ec_fragment`, `FragmentHeader::new_v1`) so
`repair::intact_shard`'s full-identity verify is genuinely exercised, and segment records are built
with the real validating constructors (`SegmentRecord::new`, `SegmentedMap::new`, `seg_key`).

**(c) Fixture includes the fault?** Yes, and it is asserted to be real rather than assumed.
`Fixture::seed_damaged` (`segmented_map_passes.rs:559-588`) asserts the undecodable bytes really
fail `metadata::decode` and that the damaged root really makes `metadata::resolve_chunk_map`
return `Err` — the same self-check `segmented_map_restore.rs:415-431` uses. The damaged record is
seeded at `inode:1`, and `MemMeta` is `BTreeMap`-backed, so a walk meets the **blocker first** and
"the healthy object was still handled" cannot pass by accident of ordering. The store fault in
leg 4 is armed *after* seeding and is a genuine non-`ChunkMapError` `io::Error` whose exact text
the assertion matches. Nothing is curated out: legs 1, 3 and 5 hold the damaged/segmented objects
*and* the healthy work in the same store, and leg 2 asserts the whole store is byte-identical
(`meta.snapshot()`) after all three passes ran.

---

## 4. Budget

957 semantic added lines by a strict counter (non-blank, non-comment, excluding
punctuation-only lines); **899** with assertion-message continuation lines also excluded as prose —
against the brief's ≤ 900. Production is **307** of that (78 backfill + 73 rebalance + 156
reconstruction); the discriminator is **650**. The overage is entirely the discriminator, and it is
the thing the carry-forward asked for: leg 2 extended to all three passes, leg 3 split per damage
shape (that split is what makes each pass's decode-site and resolve-site containment individually
load-bearing — it killed 6 of the 18 surviving mutants), and two new legs binding the two review
findings. I did prune for it: the per-leg wiring (D-server tuples, two context literals, election,
capture plumbing — ~18 lines × 8 legs) is now one `Fixture`/`Pass` seam (`:428-651`), which took
the file from 764 to 650 semantic lines with every leg and assertion intact. Files: 4 ≤ 8, so the
shape-is-wrong STOP condition (a ninth file) is not in play.

---

## 5. What else was run

* `cargo test -p wyrd-custodian` — all 15 binaries green, **including** `tests/backfill.rs`'s
  CAS-race leg (`a_racing_writer_wins_the_cas_and_backfill_retries_on_a_later_pass`, which asserts
  `Satisfied` on a conflict — the reason (B) does not raise `refused`) and
  `tests/backfill_telemetry.rs`'s `gather_prometheus` read-back of the gauge through the real
  export surface.
* `cargo test --workspace --exclude wyrd-dst` — 160 test binaries, 0 failures. No per-pass
  regression file needed editing, so `tests/{reconstruction,backfill,rebalance}.rs` are untouched.
* `cargo fmt --all` (clean) and `cargo clippy --workspace --all-targets` (no warnings; the
  workspace runs `-D warnings`) — the target's commit hooks are `cargo xtask ci`'s fmt/clippy legs.
* `cargo xtask ci` — the full gating gate (fmt + clippy + build + test incl. DST + cargo-deny +
  conformance vectors): see §7 for the recorded result.
* `scripts/mutants-in-diff` (C5): **0 missed, 31 caught, 28 unviable of 59** after the final leg
  (`59 mutants tested in 58s: 31 caught, 28 unviable`); 18 missed of 60 before this iteration.

## 6. Docs-currency — confirm-only, no edit

`docs/design/architecture/06-runtime-view.md:29-31` says resolution is single-sourced and "a
consumer that has not yet adopted it refuses a segmented map outright". Still true after this
slice: `grep SegmentedMapUnsupported` shows the remaining refusers are all in `crates/core`
(`read.rs:96`; `metadata.rs:1709` `commit_chunk_map`, `:1749`, `:1817`, `:1872` the lease /
high-water-mark paths), none in `crates/custodian`. The paragraph's maintenance-pass sentences —
containment per object, "names the one it could not", "reports the store not certified" — now
describe three more passes than they did, which is what they already claimed. No edit; claiming
more would be claiming something the passes cannot evidence.

`crates/custodian/src/restore.rs:616`'s `deferred: #681` marker is **left as-is**, per the brief's
condition: it is only to be reworded "if the shared walk genuinely subsumes `committed_chunks`",
and it does not — this slice deliberately does **not** share one `inode:` walk across passes
(the brief scopes that out under criterion (5), and the counted leg is reconstruction-only for
exactly that reason). Editing it would also make restore.rs a fifth file the brief excludes.

## 7. Gate evidence recorded at build time

* `C4-verify` (`engine/scripts/run-verify.sh`, the configured per-fix runner, exit 0): **PASS** —
  green with the fix, red with production reverted (`0 passed; 8 failed`, assertion-red, compiling).
* `C5-mutants` (`scripts/mutants-in-diff`): **0 missed / 59 tested**.
* `C4-ci` (`engine/xtask.sh ci`): **`xtask ci: all checks passed`**, exit 0 — fmt, clippy
  (`-D warnings`), build, the whole workspace test suite, `cargo deny check` (+ the all-features
  advisories config), `xtask conformance: 5 valid + 6 invalid vectors pass`, `xtask statics: no
  DST-reachable shared mutable global state (ADR-0035)`, `xtask deploy-guard`, and the
  `--cfg madsim` DST leg (clippy + the seed sweep).

  **The gate caught something my own clippy run had not** (I had run clippy before the fixture
  refactor): three `useless_conversion` errors on `metadata::encode(&x).into()` in the new test —
  `encode` already returns `Bytes`. Fixed and the gate re-run green. This is exactly the
  laptop-vs-hook asymmetry the "commit-ready" rule warns about, so it is worth recording that the
  final state is green through the project's own runner, not through a hand-rolled command.

## 8. Self-review against the target's standing rubric (`AGENTS.md` §"Review rubric & protocol")

* *One clock per correctness lifecycle* — no clock read added or moved; `now_millis` is threaded
  through unchanged.
* *Narrow trait seams / dependency direction (ADR-0010, ADR-0016)* — `custodian` still names only
  `traits` / `core` / `tracing`. No new dependency, no backend knowledge, and no reintroduction of
  the closed PR #647's custodian-local `resolve.rs`.
* *Metadata validation boundaries (ADR-0045)* — structural faults keep surfacing as errors at
  decode and are contained per object; the *contextual* placement check stays strict in these
  maintenance paths (`checked_fragments` → `Malformed` / NEEDS-HUMAN, untouched).
* *No DST-reachable shared mutable global state* — none added; the test's `Once` is the in-tree
  audit-callsite pattern (`segmented_map_restore.rs`), test-only.
* *`#![forbid(unsafe_code)]`* — carried by the new test file; no new crate root.
* *Docs currency* — no port / API operation / RPC / CLI flag / persisted field added; the living
  architecture doc already states the rule (§6, confirm-only).
* *Absent or unsupported entries — never silent success or silent skip* — this is the change:
  every refusal is explicit (audit line with a stated reason + a counter), keeps the work visible
  (obligation queued, chunk on the gauge, fragment in place), and makes the pass non-certifying.
  The count-based assertions in the discriminator are each paired with a state assertion (record
  version, placement vector, or whole-store byte equality), so no leg can pass on a count while the
  property fails.
* *Transactions* — no live transaction is held across an early return; a refusal builds no batch at
  all, and the `require`/`put` CAS shape is unchanged.
* *Await discipline* — the two added awaits (`resolve_chunk_map`, `resolve_current_chunk_map`)
  follow the seam rule this crate already applies to the same call (`gc.rs:394-401`,
  `restore.rs:604-608`, #508/#636): the bound is the `MetadataStore` implementation's, and both are
  fail-closed — they either propagate or contain the object, never "it owns no bytes".
* *Test fidelity* — no new **destructive** path lands here (a refusal writes nothing), so no new
  DST leg; the brief scoped one out explicitly. The existing seeded Tier-0 campaign does re-validate
  the reworked reconstruction pass over the simulator — `prop_reconstruct_to_full_redundancy`,
  `prop_commit_point_atomic_under_crash`, `prop_durability_emission_rises_then_returns_to_zero`,
  plus `prop_segmented_resolve_never_tears` for the resolver — over the 50-seed sweep and the eight
  committed regression seeds, all run by `cargo xtask ci` (§7a).
* *Reviewer protocol / deferrals* — `restore.rs:616`'s `deferred: #681` marker is left in place
  under the brief's own condition (§6), not silently dropped.

## 9. Nothing was pushed

No branch, no PR, no `gh` call. The patch is `patch.diff` in the bundle; the worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l1` holds the same content (verified byte-identical against
`patch.diff`).
