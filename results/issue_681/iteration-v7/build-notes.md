# Build notes — issue 681 (`passes-read-through-resolver-contained`), iteration 7

Rebuild against the **Iteration 6 carry-forward**: three T4-blocking findings, the advisory
review's T2/T5/C5 items, and "get the semantic-line count back under budget". Everything below
cites `path:line` on the **final worktree** (`$PDCA_WORKTREE`, base `origin/main` @ `339da46`).

---

## 0. Result at a glance

| gate | result |
|---|---|
| `C4-ci` (`cargo xtask ci`: fmt, clippy, build, workspace tests, deny, machete, typos, docs, conformance) | **all checks passed** |
| `C4-verify` (`engine/scripts/run-verify.sh`) | **PASS — red without the fix, green with it (6 tests ran red)** |
| `C5-mutants` (`scripts/mutants-in-diff`) | **82 mutants: 45 caught, 37 unviable, 0 missed** |
| existing per-pass suites (`tests/{reconstruction,backfill,rebalance,backfill_telemetry}.rs`) | green, **unmodified** |

**Budget (measured, methodology below): exactly 880 semantic added lines — the pinned sum — in
4 files; 1457 raw added lines (cap 1520); test file 732 raw (cap 780); patch 101,978 B = 99.6 KB,
under the driver's 100 KB size-signal threshold (`src/pdca_harness/size_signal.py:251-253`
compares `patch_bytes / 1024 >= 100`).** v6 was 915 semantic / 99,700 B.

Per file: `reconstruction.rs` 180 (≤210) · `backfill.rs` 100 (≤100) · `rebalance.rs` 100 (≤100) ·
`tests/segmented_map_passes.rs` 500 (allocation 470). The test is 30 over its allocation and
reconstruction is 30 under its own, so **the pinned total is met exactly**; the brief frames the
per-file split as "a plan and not a race" (`brief.md:311-319`). The 30 lines are the three
sub-assertions this round's reviews demanded and the Plan-time estimate did not carry (§1).
Counting rule: added lines that are non-blank and not comment-only — the same rule that reproduces
the adversary's v6 numbers (915 / 544 / 103 / 180 / 88) exactly.

---

## 1. The three T4-blocking findings

### (a) `backfill.rs:190` — stale gauge on a CAS conflict — **FIXED**

*Finding:* "A CAS conflict leaves the snapshot's empty placements counted even when the winning
write filled or deleted them, so the gauge documented as the population remaining after the pass
can publish a stale nonzero value."

Real, and it is a v6 regression against the base: the base recomputed the gauge in a **second**
namespace scan after the fill loop, so a lost race was corrected by re-reading; v6 accumulates in
the one walk (which the brief requires — `brief.md:284-287`) and kept the superseded count.

Fix (`crates/custodian/src/backfill.rs:198-201`): on `CommitOutcome::Conflict` the record's empty
placements **leave** the published population and join its caveat instead —

```rust
CommitOutcome::Conflict => {
    remaining -= to_fill.len() as u64;
    refused.superseded(&key);
}
```

`Refusals` gains a third counter (`crates/custodian/src/backfill.rs:229-247`) kept out of
`total()` deliberately: `tests/backfill.rs:275-322` pins that a pass which only lost a race still
answers `Satisfied`, then `Changed` on the uncontested retry — losing a race is ordinary, not work
refused, and this slice may not change that answer. Bound by
`crates/custodian/tests/segmented_map_passes.rs:628-637` (armed racing writer → `Satisfied`, gauge
`remaining=0, incomplete=1`); reverting the `remaining -=` line alone turns that leg red (verified
by hand, §5).

### (b) `rebalance.rs:130` and (c) `reconstruction.rs:383` — **recorded-rejected, with evidence**

Both ask for the same behaviour: *stop repairing / stop evacuating while any object is unreadable*.
I did not make that change, for two independent reasons, and recorded both in
`review-rejected.md` in the parser's one-line format (at the old line **and** the line the
equivalent code now occupies, since `is_rejected` matches `loc` exactly —
`scripts/review-branch:248-253`):

1. **The loss chain cannot close.** Neither pass deletes the displaced fragment; it writes an
   orphan *grace record* (`rebalance.rs:425-430`, `reconstruction.rs:665-671`). GC reclaims a
   marked fragment only through `ReferenceSet::protection` (`gc.rs:306-316`), consulted before
   every delete (`gc.rs:191-194`), and that returns `Some(...)` — i.e. withholds — for **every
   fragment in the fleet while any object is unresolvable** (`incomplete-reference-set`) and for
   any fragment a readable committed map still places (`referenced`). So while the hidden object
   stays unreadable nothing is reclaimed; once it is readable its reference protects the fragment;
   and if it ceased to exist, reclaiming is correct. There is no state in which the described loss
   happens.
2. **It re-introduces the defect the slice exists to remove.** "One damaged record costs every
   healthy object its repair / evacuation" *is* the C-1 violation named as the invariant to restore
   (`brief.md:160-175`), and the brief pins the opposite twice: leg 3 REQUIRES the healthy object's
   repair, fill and evacuation to still happen beside two unreadable objects (`brief.md:96-101`),
   and pinned decision 2 settles that an incomplete reading changes what a pass may **claim**
   (`Blocked`) and what it may **discard** (never an obligation) — not what it may do
   (`brief.md:243-247`).

Because a rejection that lives only in a decisions file gets re-derived by the next reviewer, the
reasoning is now **in the code** at the two sites the finding named
(`crates/custodian/src/rebalance.rs:132-140`, `crates/custodian/src/reconstruction.rs:292-300`) —
zero semantic lines, comments only.

*Cost of the alternative, concretely.* The nearest change that is not a blanket refusal is
"suppress the orphan mark while the reading is incomplete": a `bool` threaded into
`repair_chunk`/`evacuate_chunk` plus a branch at each `batch.put(orphan_key(...))` — 8 added lines
across the two files, and it trades a refuted loss for a **real permanent leak**: nothing re-marks
that fragment later (GC only reclaims on an orphan record or an expired pending lease,
`gc.rs:196-211`), so the bytes are stranded forever. That is a worse answer under C-1, not a safer
one.

---

## 2. The advisory findings from v6

| finding | disposition |
|---|---|
| **C5 (advisory review):** all three `segmented-chunk-map` reason constants could be changed to an invalid value with every test still green | **FIXED (test):** each pass's stated reason is now asserted on its audit seam — `segmented_map_passes.rs:536` (all three passes, `SEGMENTED`), `:601` (`INCOMPLETE`), `:668` (`AMBIGUOUS`) |
| **T5:** leg 3 exercised each damaged shape "ALONE", so combined containment and dual attribution were not demonstrated | **FIXED:** leg 3 is now ONE store carrying all three damaged shapes at once (`segmented_map_passes.rs:570-604`), first in `BTreeMap` key order, each named by all three passes, with `incomplete = 3` on the gauge |
| **T2:** 915/880 semantic, rebalance 103/100, test 544/470 | **FIXED:** 880/880, rebalance 100/100, test 500 with reconstruction 30 under (§0) |
| **Adversary:** `rebalance.rs:511` — the refusal counter is `fragments as u64`, so a refusal that leaves only unclassifiable chunks ticks the seam by **0** | **FIXED:** counts the OBJECT (`rebalance.rs:517`), as every other refusal counter here does; bound at `segmented_map_passes.rs:546-550` (reverting to `fragments as u64` turns leg 2 red — verified by hand, §5) |
| **Adversary:** `backfill.rs:271` — the caveat rode as an unprefixed field, which `tracing-opentelemetry`'s `MetricVisitor::record_u64` turns into an **attribute**, splitting the gauge series | **FIXED:** its own instrument, `gauge.backfill_placement_incomplete` (`backfill.rs:287-292`); the discriminator asserts the `gauge.`-prefixed field name, so a plain field fails the assertion |
| **Adversary:** the brief predicts leg 6 is not base-red, but `C4-verify` reports six | **Confirmed and noted, not "fixed":** on the base that leg dies at the seeded undecodable record before the injected `get` fault is reached, so it is red for a different reason than its post-fix assertion binds. The post-fix assertion still binds "a store fault is not swallowed" and is base-visible; nothing is vacuous. Flagged here so sign-off sees the brief's prediction and the gate row disagreeing |
| **Adversary:** `desired_state.rs:191-197` — a fragment of a segmented object on a draining server now answers a quiet, steady `Pending` instead of the base's loud `Err` | **Out of scope, for sign-off:** `desired_state.rs` is explicitly excluded (`brief.md:295-296`) and #682 builds the write path that resolves it. Recorded as a NEEDS-HUMAN item below rather than silently absorbed |
| **Adversary:** `restore.rs:616`'s `deferred: #681` marker will point at a closed issue | Noted; touching it would make a fifth file and trip the brief's STOP (`brief.md:311-319`). Left for #682 |
| **Validation (advisory review):** is publishing `remaining = 0` beside `unaccounted = 1` operationally safe? | Partly addressed by the instrument split above (two named series, no silent label); the residual "should an incomplete pass publish the gauge at all" question stays a human call — NEEDS-HUMAN below |

---

## 3. What the production change is, file by file

Salvage from `iteration-v2`/`v6` with this round's corrections; no re-derivation.

* **`reconstruction.rs`** — the per-obligation `find_chunk` namespace scan is replaced by ONE
  reading per pass (`locate_queued_chunks`, `:766-856`) that indexes only the queued chunks
  (`CommittedIndex`, `:687-720`; an object holding none of them retains nothing). `assess`
  (`:387-397`) classifies against that reading: a `seg:`-resident reference is `Refused`, a
  duplicate id is `Refused` and both objects named (`note`, `:710-719`), and a chunk found nowhere
  is drained **only** when the reading was complete. Every existing classification and its gauge
  accounting survive.
* **`backfill.rs`** — resolves per object (`:111-124`), declines a segmented record byte-identically
  with a stated reason, keeps its empty placements on the population, and publishes the population
  with its own caveat instrument. No second resolving walk (the base's `emit_remaining` scan is
  gone).
* **`rebalance.rs`** — the evacuation scan resolves per object; a segmented object is refused ONCE
  per object with the fragments it leaves behind and the chunks it could not classify
  (`Refusals::refuse`, `:183-201`), and the pass answers `Blocked`.
* **All three** — containment by `gc::referenced_fragments`' rule and no other: a decode failure, an
  unparsable `inode:` key, or a typed `ChunkMapError` from the resolver is named per object and the
  walk continues; anything else propagates (`?`). Writes are CAS'd on the row's own **stored bytes**
  under the key the store gave it, never a re-encoding and never `inode_key(parse(key))`.

---

## 4. Forced self-refutation (the three questions)

**(a) Genuine red?** **Yes.** `engine/scripts/run-verify.sh` reverts the three production files,
keeps the added test, and reports: `test result: FAILED. 0 passed; 6 failed` — every leg failing
*behaviourally* (`reconstruction must not end the pass: ... met a segmented chunk map, which this
build cannot yet resolve`, `... key must be a string at line 1 column 2`, and an
`assert_eq!(Changed, ...)` mismatch), never a compile error — then green with the fix:
`run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red)`. Two of this round's
*new* sub-assertions were additionally refuted one at a time by reverting just their production
line (§5).

**(b) Production path?** **Yes.** The legs drive `wyrd_custodian::reconcile_step` — the real fenced
control point, entered through `Custodian::elect` + `FencedZone` over `wyrd_coordination_mem`
(`segmented_map_passes.rs:417-432`) — and the real public `backfill::reconcile`
(`:436-440`), over in-memory `MetadataStore`/`ChunkStore` **trait doubles**. The chunk maps are
resolved by the real shared `metadata::resolve_chunk_map`; fragments are real erasure-coded bytes
from `erasure::encode` + `encode_ec_fragment`, so a survivor has to pass the production
`repair::intact_shard`. No pass is re-implemented, mocked or wrapped.

**(c) Fixture includes the fault?** **Yes.** The damaged objects sit in the SAME store as the
healthy work and are met **first**: the double is `BTreeMap`-backed and the keys are ordered
`inode:-1` < `inode:0` < `inode:00` < `inode:006` < `inode:007` (`:216-226`), so "the healthy
object was still handled" cannot pass on a walk that gives up at the first blocker. The fixture
**asserts its own fault is real** — `Store::root` (`:369-380`) requires
`resolve_chunk_map(...).is_ok() == readable`, so the dangling-`seg:` root really does return a
typed `ChunkMapError`, and the undecodable row really is undecodable (leg 3 would otherwise read
`incomplete = 2`, not 3). Segmented objects are seeded as raw `seg:` records + a root, never
through a committer.

---

## 5. Hand-refutation of the two new sub-assertions

Both were run in the worktree, one production line at a time:

| reverted line | leg | result |
|---|---|---|
| `rebalance.rs:517` `= 1_u64` → `= fragments as u64` | leg 2 | `FAILED ... one refusal, named, counting fragments` — then `ok` when restored |
| `backfill.rs:199` `remaining -= to_fill.len() as u64;` removed | leg 3 | `FAILED ... assertion left == right failed: the remaining sample` — then `ok` when restored |

`C5-mutants` covers the rest mechanically: **0 missed** over 82 mutants on the bundle diff.

---

## 6. NEEDS-HUMAN at sign-off

1. **`desired_state::reconciliation_status` still answers a bare `Pending`** for a draining server
   holding a fragment of a segmented object — a wait that will not clear until #682 lands. The base
   was equally stuck but loud (the pass returned `Err`); this slice makes it quiet. `desired_state.rs`
   is out of this slice's scope by the brief; the call is whether to accept it as #682's or to file
   an attributed status now.
2. **Gauge semantics:** `backfill_placement_remaining = 0` is now published beside
   `backfill_placement_incomplete = N` as two instruments on one event. A consumer that watches only
   the first still reads "the pre-M3 tail is gone" during an incident. Two instruments and an audit
   line per unread object is what this slice can honestly do; suppressing the sample entirely on an
   incomplete pass would break #350 step 2's "a sample every pass".
3. **Leg 6's base-red reason** differs from the brief's prediction (§2). The evidence is unaffected,
   but the brief and the gate row disagree and a reviewer cannot see why without this note.

No external dependency was missing: everything ran on the base Rust toolchain plus the five
registered doctor.checks tools (`typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny`,
`cargo-machete`), all present.

---

## 7. What I ruled out

* **Blanket refusal under an incomplete reading** (the shape findings (b)/(c) ask for) — §1, refuted
  at `gc.rs:306-316` and pinned against by the brief.
* **A seeded Tier-0 DST leg** — recorded-rejected at Plan and re-checked here; no commit in this
  slice is reachable through a restarted resolve (all three passes branch on the *snapshot's* shape
  before any write), so there is no new concurrent path to seed. I also **dropped v6's supersede
  sub-case** (21 semantic lines) that probed it: the brief states in as many words that decision 4
  is bound by "none — by design" and that no leg may be built for a path this slice cannot execute
  (`brief.md:264-268`).
* **A GC leg binding the refutation in §1** — it would need a `GcContext` (~12 lines, a symbol
  outside the brief's success-criterion list) and gc.rs is another slice's. The refutation is
  instead a code citation anyone can check in ten seconds, plus the in-code comments.
* **Trimming the patch below 100 KB with `-U2` context** — that games the size signal rather than
  reducing the change. The 4.5 KB came out of genuinely verbose comment prose instead.
