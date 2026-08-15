# Build notes — issue 681 (`passes-read-through-resolver-contained`), iteration 6

Withheld from the reviewer; written for the human at sign-off.

Target branch `origin/main` @ `339da46`; all `path:line` citations below are against the patched
worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0` (the same tree the gates ran in).

---

## 0. What this iteration is

The v5 attempt passed C1–C5 and C4-verify; the human **iterated on the merits**, listing seven
items. This rebuild starts from the v5 production hunks (the brief's declared salvage lever) and
changes exactly what those seven items name, plus the two open `review-batch.md` findings. Nothing
else in the production shape was re-derived — the containment rule, the one-reading index, the
refusal vocabulary and the CAS-on-stored-bytes discipline are v5's, already reviewed twice.

**Final shape:** 4 files · patch 99,700 B (< the driver's 100 KB backstop; v5 was 99,757 B) ·
`tests/segmented_map_passes.rs` **775** lines (cap 780; v5 was 788) · 1,466 raw added lines
(cap 1,520).

---

## 1. The seven carry-forward items, one by one

**1. `rebalance.rs:269` (T4 blocking) — a malformed placement inside a record this pass may not
repoint was skipped without a refusal, so the drain could report `Satisfied` over fragments that
never moved. FIXED.**

The refusal is now taken for the **whole segmented object**, before the per-chunk evacuation loop:
`crates/custodian/src/rebalance.rs:260-267` branches on `record.chunk_map.is_segmented()` and calls
`held_for_drain` (`:184`), which counts *both* the fragments it can see on a draining server *and*
the chunks whose committed `placement` it cannot classify at all (#348's malformed vector). Either
being non-zero refuses the object (`:263`), so the pass answers `Blocked`.

Why this is scoped to the **segmented** branch and not to flat records: for a flat record the
classification is complete, the base already answers `Satisfied` + `emit_needs_human`, and that
answer is pinned by a suite the brief forbids editing —
`crates/custodian/tests/rebalance.rs:1455-1458` requires `Reconciled::Satisfied` over exactly that
store, with the operator-facing stall reported separately as
`ReconciliationStatus::PendingMalformed` (`:1489-1495`). Inside a record **nothing may be moved out
of**, "I cannot read where its fragments are" is a different fact: the pass has no way to show the
object does not hold fragments on the draining server, and no way to move them if it does. That is
the refusal *this slice introduces*, which the brief says must be non-certifying. The v5 attempt
recorded-rejected this finding on scope; the human overrode that, and the override is right — the
`Satisfied` was reachable only on the **newly readable** (segmented) path, which is this slice's.

Bound by test: `crates/custodian/tests/segmented_map_passes.rs:576-589` (a segmented object whose
only chunk has a malformed placement and **no** visible draining fragment → `Blocked`). Reverting
just the `unreadable` half of the condition turns that assertion red (mutation **M1** below).

**2. T2 Shape — the test file was 8 lines over the 780 cap. FIXED: 775 lines.** The reduction is
structural, not deletion of evidence: the standalone stray-key block folded into the damaged-object
loop as a third shape (`:604-608`, `STRAY_KEY = b"inode:-1"` at `:228` — it sorts first like the
other two and `u64::from_str` rejects the sign), the `tracing` install moved into `Store::new`, and
`samples`/`assert_gauge` merged. Net: the file gained four binding sub-assertions and still lost 13
lines.

**3. Pinned decision 3 unbound for reconstruction and rebalance. FIXED.** The decision-3 block now
seeds one record under `inode:007` that owes **all three** kinds of work at once (a repairable
chunk, a fragment on the draining server, an empty placement) beside a *different* record at
`inode:7` (`tests/segmented_map_passes.rs:654-683`). A pass re-deriving `metadata::inode_key(id)`
for its CAS then reads `inode:007` and commits against `inode:7`, whose stored bytes differ → the
CAS conflicts → the repair/evacuation/fill never lands. Mutations **M2** and **M3** confirm both
passes are now bound. The same block is now an honest **over-containment control** (all three
passes must answer `Changed`), which the v5 version was not — there the store had an empty queue
and no draining server, so two of the three passes short-circuited and the `assert_ne!(Blocked)`
was vacuous (the adversary's second finding, also fixed here).

**4. `reconstruction.rs:170` (`refused` seeded from `index.unaccounted`) unbound. FIXED.**
`tests/segmented_map_passes.rs:645-651`: over the store that already holds one unreadable object,
with `C_UNSEEN` dropped from the queue and **only** `C_EVAC` enqueued — a chunk found at a flat site
and healthy, so nothing about the *obligation* is refused — the pass must still answer `Blocked`.
Mutation **M4** (`let mut refused = 0usize;`) makes it `Satisfied` and the assertion fails.

**5. The gauge assertion was a prefix match. FIXED.** `assert_gauge`
(`tests/segmented_map_passes.rs:175-191`) now splits on the field name and **parses the full digit
run**, asserting the exact multiset of samples (`[remaining]`, one sample per pass). Mutation **M5**
(`emit_remaining(remaining * 10, …)`) is caught: `[10] != [1]`.

**6. The DST TEST-GAP claim and the backfill-gauge question. Both settled, neither left ambiguous.**

*DST*: recorded-rejected, unchanged from Plan's pre-declared reason, now re-entered in
`review-rejected.md` with a MATCH phrase from the **current** finding's wording ("seeded Tier-0 DST
coverage"). Re-checked at Do as the brief asks: every write this slice performs is on a FLAT object
and keeps its existing version-conditional CAS on the scan snapshot; the resolver adds no store read
and no supersede check on the flat path (`crates/core/src/metadata.rs:2584-2586`), so no new
concurrent or destructive path exists to seed. See §4 for the falsification attempt.

*Format bug found while re-entering the DST rejection, worth the human's attention:*
`scripts/review-branch`'s `load_rejected` parses **one physical line** per rejection and requires
the reason on that same line. Every entry the previous rounds wrote was a wrapped paragraph, so
**none of them ever parsed** — which is why the DST finding (and, last round, the rebalance one)
came back as blocking every single round even though a reason had been recorded. This round's
`review-rejected.md` is single-line entries, verified against the parser. If findings still repeat
after this, the cause is something else.

*Gauge*: **fixed in code**, not argued away. `Refusals` in backfill is now two counters
(`crates/custodian/src/backfill.rs:227-254`) because the gauge treats them differently: a *declined*
record's empty placements were READ and stay on the reported number, while an *unaccounted* object's
are unknown. `emit_remaining(remaining, refused.unaccounted)` (`:208`, `:270`) publishes both on the
**same sample** — the label shape `emit_domain_utilization` already uses (`rebalance.rs:345-350`), so
no new metric convention. #350 step 2 ("emit every pass") and "a pass never claims more than it read"
are then both satisfied: `remaining = 0, unaccounted = 1` is a floor, not a clean bill.
Bound by value in `tests/segmented_map_passes.rs:630-633` (`assert_gauge(&logged, 0, 1)`) and by
mutation **M6**.

**7. The C4-ci flake.** Could not be reproduced and the failing output was not preserved (v5's
`gate-logs/C4-ci.log` holds only the passing confirm re-run, "attempt 1/1: pass"). I ran the full
gate **three times** on this tree (`./engine/xtask.sh ci`, exit 0 each time, ~130 s), plus
`cargo test -p wyrd-custodian` repeatedly during the mutation sweep. Nothing in this bundle's diff
is time-, order- or environment-dependent: the tests are `#[tokio::test]` over in-memory doubles with
no wall-clock read, no filesystem and no network. The one shared resource these runs contend for is
the single `target/` directory in `$PDCA_WORKTREE`, which `C4-verify` also builds into (it resets and
rebuilds the same crate in `../wyrd-verify`) — a concurrent cargo build lock or a cache eviction is
the most plausible interference, and it is environmental rather than a property of the patch. I could
not prove that, so it is stated as the best available explanation, not a finding.

---

## 2. The change, per file

**`crates/custodian/src/reconstruction.rs`** — unchanged in shape from v5 (already reviewed twice):
one namespace reading per pass indexed by chunk id (`locate_queued_chunks`, `:759`), obligations
assessed against it, `Site::Refused` for a `seg:`-resident or ambiguous chunk (`:825`, `:708-710`), the
CAS on the row's own key and stored bytes, and `refused` seeded from `index.unaccounted` (`:168`).
Only comments changed (deduplicated against `gc.rs`/`backfill.rs` rather than restated three times).

**`crates/custodian/src/backfill.rs`** — v5's containment plus the two-counter `Refusals` and the
qualified gauge (§1.6).

**`crates/custodian/src/rebalance.rs`** — v5's containment plus the whole-object refusal and
`held_for_drain` (§1.1). `Refusals(usize)` now counts refused **objects**, and `emit_refused`
(`:510`) carries `fragments` *and* `unreadable`; the malformed chunk still raises #348's
`rebalance_malformed_placement` NEEDS-HUMAN signal, so no operator signal was traded away.

---

## 3. Forced refutation of my own test (the three questions)

**(a) Genuine red?** **Yes.** Through the project's own runner:
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → `RED — production reverted, test kept:
0 passed; 6 failed`, then `GREEN — fix applied: 6 passed`, verdict
`PASS — red without the fix, green with it (6 test(s) ran red)`. The reds are behavioural
(`SegmentedMapUnsupported` from the base's seven fail-closed sites, `Satisfied` where `Blocked` is
required, `3` namespace scans where `1` is required) — the file compiles on the base, so no red is a
missing symbol.

Beyond the whole-file red, every **new** assertion was individually refuted by reverting just the
production line it binds and re-running (each caught, file restored after each):

| # | mutation | assertion that went red |
|---|---|---|
| M1 | `held_for_drain`'s `unreadable` count ignored | `:589` "unreadable placement, no drain" |
| M2 | reconstruction CASes on `inode_key(parse(key))` | `:675` "reconstruction did its work" |
| M3 | rebalance CASes on `inode_key(parse(key))` | `:675` "rebalance did its work" |
| M4 | `refused = 0` instead of `index.unaccounted` | `:651` "it read less than the store" |
| M5 | `emit_remaining(remaining * 10, …)` | `:189` gauge sample `[10] != [1]` |
| M6 | `emit_remaining(remaining, 0)` | `:189` unaccounted sample `[0] != [1]` |
| M7 | `Refusals::declined` counts 0 | `:171` "backfill may not certify" |
| M8 | rebalance CAS requires `encode(&prior)` | `:492` "the flat evac happened" |
| M9 | ambiguity resolved last-writer-wins | `:707` "ambiguity is no repair" |
| M11 | segmented chunk not noted as a site | `:171` — the obligation would be **drained** |
| M12 | backfill writes the segmented record | `:171` "backfill may not certify" |

And mechanically: `scripts/mutants-in-diff` → **79 mutants tested in 67 s: 39 caught, 40 unviable,
0 missed** (v3: 11 missed; v4: 4 missed).

**(b) Production path?** **Yes.** The legs drive the real fenced control point
`wyrd_custodian::reconcile_step` over a real `Custodian::elect` + `FencedZone` on
`MemCoordination` (`tests/segmented_map_passes.rs:433-451`) and the real public
`wyrd_custodian::backfill::reconcile` (`:452-456`). The only doubles are the two trait seams
(`MetadataStore`, `ChunkStore`) — the same shape `segmented_map_consumers.rs` and
`segmented_map_restore.rs` use. Fragments are **real** erasure-coded bytes from `erasure::encode` +
`encode_ec_fragment` (`:351-360`), so the production `fragment_intact` / decode path runs for real.
No production logic is re-implemented in the test.

**(c) Fixture includes the fault?** **Yes**, and the fixture asserts its own faults are real:
`seed` checks that the undecodable bytes really fail `metadata::decode` (`:369`) and that
`resolve_chunk_map` really errors on the dangling `seg:` root (`:408-409`). The damaged object is
**first in key order** over a `BTreeMap`-backed store, so "the healthy object was still handled" is a
property of the walk, not of luck; the healthy work asserted afterwards
(`assert_flat_work_done`, `:488-495`) is over objects seeded *behind* the blocker. The segmented
object is a raw `seg:` + root seeding (never a committer), and leg 2 reads the `seg:` bytes and the
root `version` before and after to prove a refusal writes nothing.

---

## 4. Decision 4, the deferred "unreachable by construction" claim

Round 3/4 deferred a reviewer claim that the brief's decision-4 "unreachable by construction" is
false. Re-traced at this build, and the brief's own instruction was to say so if I found a reachable
commit path: **I did not.**

* A **flat** snapshot resolves to `Answer(Cow::Borrowed(chunks))` with no store read, no settle read
  and no supersede check (`crates/core/src/metadata.rs:2584-2586`), so it can never restart.
* Only a **segmented** snapshot reaches `resolve_current_chunk_map` and can restart onto a live root.
* All three passes branch on the **snapshot's** `record.chunk_map.is_segmented()` — `backfill.rs:160`,
  `rebalance.rs:260`, `reconstruction.rs:820` — *before* any write, and every segmented branch
  refuses without writing.

So every commit this slice performs is framed by, and CAS'd on, the bytes the scan itself read. The
leg-2 supersede sub-case (`tests/segmented_map_passes.rs:559-574`) exercises the restart for real: a
root that moves to a *fillable flat* record between the scan and the settle read leaves the pass
holding a chunk list whose stored bytes it never saw — it writes nothing and answers `Blocked`, and
the test asserts the live bytes are untouched. If a future reviewer wants the claim falsified, that
is the leg to attack.

---

## 5. Alternatives considered and rejected, with their cost

* **Make a malformed placement non-certifying everywhere (flat included).** Rejected: it requires
  editing `crates/custodian/tests/rebalance.rs:1455-1458` (which asserts `Satisfied` over that exact
  store) and `crates/custodian/tests/reconstruction.rs`'s malformed cases — the brief names a need to
  edit those suites as "a signal that an answer changed further than this slice intends". Concrete
  cost: 2 assertions rewritten in a suite this slice must not touch, plus a behaviour change to a
  path this slice does not introduce (#682 owns it).
* **Suppress the `backfill_placement_remaining` sample when the reading was incomplete.** Rejected:
  #350 step 2 requires a sample every pass ("so the drain is observable at every cadence",
  `backfill.rs:74-78`); a missing sample is indistinguishable from a stalled custodian on a
  dashboard. The chosen fix costs **1 extra field on 1 existing event** (`backfill.rs:270-274`) and
  one extra counter field in `Refusals` (4 lines) — versus removing the emit, which would be ~2
  lines but would break the gauge contract.
* **A named `Held { fragments, unreadable }` struct in rebalance** (v5-style shape). Dropped for a
  destructured tuple return: the struct plus its `Default`/`any()` impls measured **25 semantic
  lines** against the tuple's **14**, in a file whose budget allocation is ≤100 semantic lines
  (currently 103). Readability is preserved at the call site by destructuring into named bindings
  (`rebalance.rs:262`).
* **Re-deriving the whole test file from scratch.** Rejected: v5's fixture is the compression the
  brief asked for and it already earns a 6/6 red; the four new bindings are ~35 lines of
  sub-assertion on top of it.

---

## 6. Budget honesty

Met: 4 files (exactly the brief's four); test file 775 raw ≤ 780 (the STOP criterion); 1,466 raw
added ≤ 1,520; patch 99,700 B < 100 KB.

Over, measured by "non-blank, non-comment added lines": `rebalance.rs` **103** vs ≤100 (+3, entirely
the malformed-refusal fix the sign-off ordered — v5 was 90) and the test file **544** vs ≤470
(v5 measured 558 on the same counter and was flagged only on the 780-line raw cap). Total 915 vs
≤880 (v5: 910). I did not buy the last 35 lines by deleting assertions: every line of the delta over
v5 is either the ordered fix or one of the four new bindings. If the human wants the semantic caps
met exactly, the cheapest honest cut is leg 1 (14 lines) — but it is the brief's binding leg 1, so I
did not take it.

---

## 7. NEEDS-HUMAN / manual validation

No external dependency was missing: everything ran on the plain Rust toolchain in
`$PDCA_WORKTREE` (no Docker, no protoc, no live backend, no new dev-dependency, no `Cargo.toml`
change). `typos`, `docs-renderer`, `cargo-mutants`, `cargo-deny` and `cargo-machete` were all present
and exercised through `cargo xtask ci` / `scripts/mutants-in-diff`.

Two items for the human at sign-off:

1. **The malformed-in-segmented refusal is an operator-visible behaviour change** the brief did not
   pin (it was raised by review and ordered by sign-off): a store holding a segmented object with a
   corrupt placement now reports the drain `Blocked` instead of `Satisfied`. The flat equivalent is
   unchanged. Worth a maintainer's eye because it is the one place this rebuild answers differently
   from what the brief's scope section anticipated.
2. **The gauge event gained a field** (`unaccounted`) on an existing sample. No dashboard or alert in
   this repo reads it yet (grep: the name appears only in `backfill.rs` and the discriminator), and
   the metric name itself is unchanged, so the change is additive — but a metric surface is the kind
   of thing an operator owns.

Manual validation, if wanted: `cargo test -p wyrd-custodian` (all 16 targets green, existing
per-pass suites unmodified), `./engine/xtask.sh ci` (exit 0), and
`PDCA_BUNDLE=$PWD/results/issue_681 ./engine/scripts/run-verify.sh` (red→green).

Scratch: everything I created lives under `$PDCA_SCRATCH/pdca-builder-681-*` and is removed.
