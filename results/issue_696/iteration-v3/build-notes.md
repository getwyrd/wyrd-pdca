# Build notes — #696 (iteration 3): rebalance reads through the resolver, contained

Withheld from the reviewer. Line citations are against the patched worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l1` (= `origin/main @ 339da46` + `patch.diff`); base citations say
"on the base".

---

## 1. What the patch does

Two files, exactly as budgeted.

**`crates/custodian/src/rebalance.rs`** (+90 semantic lines, cap 130; 211 raw added / 46 removed):

| Site | Change |
|---|---|
| `:136`, `:349` | `plan_evacuations` answers `(Vec<EvacPlan>, Refusals)` — the moves it can prove, beside what it could not account for. |
| `:152` | `refused.total() > 0` → `Reconciled::Blocked`, ahead of `Changed`/`Satisfied`. The pass never claims more than it read. |
| `:165-225` | `Refusals` — ONE entry point per outcome, so a site can neither name without counting nor count without naming (Rule D). |
| `:202-219` | `refuse` — a segmented object is refused iff it holds fragments the pass can SEE on a draining server. A chunk whose placement cannot be classified gets the malformed-placement NEEDS-HUMAN signal and **no refusal**, exactly as the flat path answers the same corruption at `:314` (§2, the T4 blockers). |
| `:244-250` | `metadata::decode` failure contained per object (`gc.rs:378-384`'s shape), never a `?` that ends the scan. |
| `:259-273` | The read goes through `metadata::resolve_chunk_map` — the one resolver GC (`gc.rs:402`), restore (`restore.rs:644`) and the read path share. Containment by **exactly** `gc.rs:405-415`'s downcast rule: `Ok(ChunkMapError)` is this record's fault, anything else propagates. |
| `:285-291` | **Rule A** — a resolve that restarted onto a generation this scan did not read (`Cow::Owned`, `metadata.rs:2629`) is contained: nothing planned, nothing reported, nothing written. |
| `:295-298` | **Rule B/D** — a segmented record is refused *once, per object*; the walk continues. |
| `:339`, `:448` | **Rule C** — the plan carries the `inode:` key **as the store spelled it**; the CAS uses it verbatim instead of `metadata::inode_key(parse(key))`. |
| `:393-395` | The second base `?`-site (`evacuate_chunk`'s `as_flat()`, base `:259`) becomes a fail-safe `Aborted` — unreachable in practice (only flat generations are planned) and never a whole-pass abort. |
| `:512-545` | `emit_unaccounted` / `emit_refused` on the durability seam, named with `gc::object_name` (`gc.rs:470`). |

Deleted: `parse_inode_key` (the Rule C hazard) and both `ChunkMapError::SegmentedMapUnsupported`
raises (base `:162`, `:259`).

**`crates/custodian/tests/segmented_map_rebalance.rs`** — NEW, 7 legs over one fixture
(329 semantic / 500 raw; caps 330 / 540).

Production hunks salvaged from `results/issue_681/iteration-v7/patch.diff` as the brief
instructed, plus rules A and C, plus the deltas iterations 1 and 2's gates demanded (§2).

## 2. The carry-forward, item by item

### T5 (round 2, gating via T4) — "make leg 5 genuinely non-base-red"

The reviewer was right and iteration 2's answer ("the brief's Falsifiability line is wrong")
was wrong — worse, it was recorded only in `build-notes.md`, which the driver withholds from
the reviewer, so the contradiction stayed live. Fixed in the test, not argued.

The logical constraint first, because it shapes the fix: **any leg-5 assertion that requires the
pass to ANSWER is necessarily base-red**, since on the base a store holding one segmented
object aborts the whole walk (base `rebalance.rs:158-164`). So leg 5 can only be non-red if its
assertions are ones an aborting base also satisfies. That is not a weakening — it is exactly
what "binds over-containment, which has no base behaviour to flip" means:

- `certifies(&pass, want)` (`tests/…:161-171`, the fn at `:167`) asserts *what the pass may not answer*: if it
  answered at all, the answer is `want`. An `Err` is not an over-containment, and legs 1-4 bind
  it. Used for both passes — `Changed` then `Satisfied`.
- `assert_eq!(lines(&second.1, "refused"), 0)` (`tests/…:454`) is **unconditional** and holds in
  both worlds (a base that cannot walk the store emits no refusal either).

Measured, not asserted — `run-verify.sh`'s RED leg now reports **`2 passed; 5 failed`**, and the
5 named failures are legs 1, 2, 3, 4, 6. The brief's declared signature ("Legs 1–4 and 6 go red
on the base; legs 5 and 7 are declared non-red") is now the observed one. Iteration 2 reported
6 failed / 1 passed.

Leg 5 is also **renamed** — `a_segmented_object_holding_nothing_evacuable_still_certifies_the_drain`
(was `…a_healthy_segmented_object…`): its object now carries the malformed chunk the T4 fix needs,
so "healthy" would have overstated what the fixture seeds.

Two assertions were moved out of leg 5 to get there:
- `first.expect(...)` / `second.unwrap()` → `certifies` (they were the base-red the reviewer cited
  at `segmented_map_rebalance.rs:417` **on the v2 tree**, not this one);
- `lines(log, "needs-human") == 1` → dropped from leg 5. On the base the pass never reads inside
  a `seg:` record, so the count is 0 there. Leg 6 already asserts that same naming
  (`tests/…:475`) over a store the base *can* be compared on, so nothing is lost.

### T4 — the two blocking batch-review findings (`rebalance.rs:153`, `:310` on the v2 tree)

Both say the same thing: a chunk whose committed placement cannot be classified only emits
`needs-human`, so a pass can answer `Satisfied` over it — and iteration 2 counted such a chunk
toward the refusal inside a **segmented** object while leaving a **flat** one uncounted.

**The asymmetry is a real defect and is fixed** (`rebalance.rs:207` vs `:314`): the same
corruption is now answered the same way in both shapes. Iteration 2's `unclassified` counter and
its audit field are gone.

**Making a malformed placement itself non-certifying is declined**, recorded in
`review-rejected.md` with three checkable reasons:

1. It is **base** behaviour this slice neither adds nor widens (base `rebalance.rs:177-183`), and
   the brief's `/ out of scope` pins "This child makes only the refusal **it introduces**
   non-certifying".
2. The finding's premise — that an operator could decommission over it — is **false at the
   operator surface**. The drain-certification query answers
   `ReconciliationStatus::PendingMalformed { chunks }` **cluster-wide** while any malformed
   placement exists (`desired_state.rs:234-246`), GC protects every fragment bearing that chunk's
   id (`gc.rs:309-310`, `:443-445`), and GC's own loop certifies over a malformed placement and
   blocks only on an unreadable map (`gc.rs:234-246`). This pass now answers exactly as GC does.
3. The change is **forbidden by the brief's own constraint**: `crates/custodian/tests/rebalance.rs:1455-1459`
   asserts `Reconciled::Satisfied` over exactly this fixture, beside `PendingMalformed` at
   `:1489-1496`, and the brief requires that suite stay green **unmodified**. Verified, not
   assumed — counter-ablation in §4: adding `refused.unreadable(&key, &"malformed committed
   placement")` to the flat arm (`rebalance.rs:314`) fails
   `malformed_placement_rebalance_skips_and_leaves_fragment_in_place` at
   `crates/custodian/tests/rebalance.rs:1455` ("rebalance must move nothing and repoint
   nothing").

The new rule is **bound by a leg**, not just asserted here: leg 5's segmented object now carries a
second chunk whose placement is malformed (`tests/…:437-438`), so restoring iteration 2's
behaviour turns leg 5 red (ablation 4 in §4).

### T4 Contribution (round 2) — "the bundle omits the failing review wrapper/log and rejection archive"

Assembly-side, not something `patch.diff` can carry. `review-rejected.md` (bundle root) is updated
with all four decisions and `iteration-v2/gate-logs/T4-batch-review.log` holds the wrapper output;
if the assembler still does not surface them, the human should attach both at sign-off — the
reviewer cannot judge "settled or not" without them.

### C5 (round 1) — surviving mutants

Held. `scripts/mutants-in-diff` on this bundle: **21 mutants tested in 25s: 11 caught, 10
unviable, 0 missed** (v1: 3 missed; v2: 25 tested, 15 caught, 0 missed). The mutant count dropped
because the `unclassified` arithmetic that iteration 1 could not kill no longer exists — the
survivors were removed by deleting the operator, not by adding a leg to cover it. What remains is
covered: `fragments += …` → `-=` underflow-panics and `*=` zeroes the count (legs 2/6);
`fragments == 0` → `!=` refuses leg 5's store and stops refusing leg 2's; `Refusals::total` → `0`
fails legs 2/3/4/6 and → `1` fails legs 1/5.

## 3. Two legs that were vacuous in v1 and are not now (unchanged from v2, re-verified)

**Leg 4 (Rule A)** asserts what genuinely differs when the guard is absent: the object is named
`unresolvable-chunk-map` and `lines(&log, "refused") == 0` (`tests/…:417`, `:420`) — the pass may not
describe an object with counts drawn from a generation it never scanned. Without that, the next
branch (`rebalance.rs:295`) refuses the segmented snapshot anyway and the outcome is identical.

**Leg 5 runs the pass twice** — pass 1 evacuates the flat chunk (`Changed`), pass 2 must certify.
A single pass could only ever observe `Changed`, which the over-containment bug does not disturb.

## 4. Ablation matrix (each guard, individually reverted, `cargo test -p wyrd-custodian --test segmented_map_rebalance`)

| Ablation | Result |
|---|---|
| delete `if fragments == 0 { return; }` (`:214-216`) — the v7 adversary's exact move | **legs 5 and 1 FAIL**, 5 pass |
| count an unclassifiable chunk as a refusal again (iteration 2's behaviour, `:207`) | **legs 5 and 6 FAIL**, 5 pass |
| delete the Rule A generation guard (`:285-291`) | **leg 4 FAILS**, 6 pass |
| re-derive the CAS key via `metadata::inode_key(parse(&plan.inode_key))` (`:448`) | **leg 3 FAILS**, 6 pass |
| *counter*-ablation: make a malformed **flat** placement non-certifying too (`:314`) — the symmetric alternative the T4 finding asks for | the pinned suite breaks: `tests/rebalance.rs:1455` FAILS (`Satisfied` → `Blocked`) |

Every guard in the diff has a leg that goes red without it, and the leg the brief calls REQUIRED
(leg 5) is the one that catches both containment errors.

## 5. Alternatives considered and rejected

- **Keep iteration 2's `unclassified` counting and record-reject the two T4 blockers outright.**
  Rejected on correctness, not on review pressure: it leaves the identical corruption blocking
  the step inside a segmented object and not inside a flat one, with no principle telling them
  apart. Cost of keeping it, measured: +4 semantic lines in `refuse` (`unclassified` binding, the
  `Err` arm's braces, the `&& unclassified == 0` half of the predicate, the extra `emit_refused`
  argument) and +4 mutants to bind — and it contradicts `gc.rs:234-246`, the loop this file is
  told to mirror.
- **Extend the refusal to the FLAT path too** (the symmetric alternative — make a malformed
  placement non-certifying everywhere). Rejected because it is *mechanically forbidden here*, not
  because it is heavy: it flips `crates/custodian/tests/rebalance.rs:1455-1459` from `Satisfied`
  to `Blocked` (verified by running that suite), and the brief requires that file stay green
  **unmodified**. It is a 2-line change (`:207` and `:314` both `+= 1` into `fragments`) plus a
  rewrite of that test's contract — i.e. re-opening #348 / ADR-0040 decision 4, which belongs to
  a brief of its own.
- **Assert leg 5 with `matches!(answer, Ok(Satisfied) | Err(_))`** instead of the `certifies`
  helper. Same non-redness, 2 fewer lines, but it spells the base's *bug* into the discriminator
  ("tolerate the abort") rather than the property ("never answer a withheld certification"), and
  it would silently keep passing if the pass started answering `Changed` forever.
- **Report the refusal per chunk** (drop `refuse`'s per-object aggregation): −6 semantic lines,
  and it is exactly the carried-forward finding "per-chunk logging floods the seam" (Rule D).
  Leg 6 binds against it.
- **Make `EvacOutcome::Aborted` non-certifying** (round-1 T4 finding). Out of scope per the brief
  and #682; `review-rejected.md`. The change itself is ~4 lines at `:145-150` — which is *why* it
  must not be smuggled in: it changes the operator contract for the no-free-domain and
  missing-fragment paths this patch never touches, and both `tests/rebalance.rs` legs that assert
  `Satisfied` over an aborted move would have to move with it.
- **Ship the test's doubles as a third file / shared module.** Refused by the budget ("a third
  file … means the shape is wrong: STOP"). Not attempted.

## 6. Forced refutation — the three questions

**(a) Genuine red?** Yes — proven by the project's own runner, `engine/scripts/run-verify.sh`
(the C4-verify gate), which applies the patch to a clean `origin/main` worktree, runs the test,
then reverts `rebalance.rs` and keeps the test:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_rebalance (fix applied)
    test result: ok. 7 passed; 0 failed
run-verify.sh: RED — … (production reverted, test kept)
    a_fragment_in_a_seg_record_stays_put_refused_and_does_not_certify
    a_segmented_object_no_longer_stops_the_flat_evacuation_beside_it
    an_unreadable_object_is_named_the_walk_continues_and_nothing_certifies
    rule_a_the_pass_never_acts_on_a_generation_it_did_not_read
    rule_d_one_refusal_per_object_accounting_for_what_it_leaves_behind
    test result: FAILED. 2 passed; 5 failed
run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
```

Every red is an **assertion/behaviour** red on base-visible symbols
(`Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" })`), never a compile
error: the discriminator names no symbol this patch introduces. The 2 that pass are legs 5 and 7,
declared non-red by the brief and now genuinely so. Beyond the whole-patch revert, each guard was
ablated individually (§4).

**(b) Production path?** Yes. Every leg calls
`wyrd_custodian::reconcile_step(&zone, &leader, None, None, None, Some(&ctx), 500)`
(`tests/…:297`) — the real fenced control point (`reconciliation.rs:104`, fence at `:113`), which
dispatches the production `rebalance::reconcile` (`reconciliation.rs:140`). No internal helper is
called and nothing is re-implemented: the doubles are `MetadataStore` / `ChunkStore`
implementations (the seams the loop is defined over, ADR-0010), and the fence is a real
`Custodian::elect` + `FencedZone` over `MemCoordination`. The chunk bytes legs 1/3/4/5 move are
built by the production writer (`plan_write` + `write_fragments`, `tests/…:268-269`) so
`repair::fragment_intact` (`rebalance.rs:296` on the base / `:400` here) really passes; records are
built with the real validating constructors (`SegmentRecord::new`, `SegmentedMap::new`,
`metadata::encode`), not hand-typed JSON.

**(c) Fixture includes the fault?** Yes, in every leg the failing element is *in* the store the
pass walks, not curated out:
- leg 2/6: the draining server really **holds** the fragment (`d0.put_fragment`, `tests/…:333`)
  and it is still there after the pass (`:344`);
- leg 3: the damaged pair sorts **first** in the `BTreeMap`'s key order (`inode:001`, `inode:002`
  before `inode:007`), and the fixture asserts the seeded root really fails to resolve before
  relying on it (`tests/…:364-365`);
- leg 4: the live generation's fragment really sits on the draining server, so an unguarded pass
  would have copied it;
- leg 5: the malformed placement is a real `MalformedPlacement` (`metadata.rs:225`) seeded inside
  a `seg:` record the resolver actually reads — the fixture that makes the T4 fix binding rather
  than asserted;
- leg 6: two genuine draining fragments beside that same malformed chunk, so `"fragments":2`
  discriminates "counted what it read" from "counted what the corrupt vector claimed".

## 7. Gates run here (the driver re-runs them at Check)

| Check | Result |
|---|---|
| `cargo test -p wyrd-custodian --test segmented_map_rebalance` | 7 passed |
| `cargo test -p wyrd-custodian` (whole crate, incl. the untouched `tests/rebalance.rs`) | all green — 10/15/16/13/8/7/5/3… passed, 0 failed |
| `engine/scripts/run-verify.sh` (C4-verify) | PASS — red without the fix (5 red / 2 non-red), green with it |
| `scripts/mutants-in-diff` (C5) | 21 mutants: 11 caught, 10 unviable, **0 missed** |
| `./engine/xtask.sh ci` (C4-ci, whole tree: prose gates incl. `typos`, fmt, clippy, build, tests, deny, conformance, statics, DST) | `xtask ci: all checks passed` (exit 0) |
| `cargo fmt --all --check` | clean (commit-hook readiness) |

Budgets: `src/rebalance.rs` **90** added semantic (cap 130); `tests/segmented_map_rebalance.rs`
**329** semantic / **500** raw (caps 330 / 540); **2** files. Counting script reproduces
iteration 2's published numbers exactly (94 / 327 / 205 / 472) on that patch, so the measure is
the same one T2 passed against.

## 8. For the human at sign-off

1. **Leg 5's shape drifted slightly from the brief's wording** (deliberately, and it is the T5
   fix): the brief says "*A healthy segmented* object that holds nothing on the draining server".
   Leg 5's segmented object now also carries one chunk with a malformed placement, because that
   is what binds the T4 fix. The property the brief asks for is unchanged and still asserted:
   with every flat evacuation complete the pass answers `Satisfied` and refuses nothing.
2. **`Reconciled::Blocked` reaches operators through `reconcile_step`'s `least_certified` fold**
   (`reconciliation.rs:55-61`): a store holding one segmented object with a draining fragment
   reports the whole *step* as Blocked, not just rebalance. Intended (the drain is genuinely not
   safe to certify), and still the "fitness-to-purpose" question the review flagged NEEDS-HUMAN —
   a maintainer call, unchanged by this rebuild.
3. **The two T4 blockers are half-fixed, half-declined** (§2). If the maintainer disagrees with the
   declined half, the right move is a new issue re-opening #348 / ADR-0040 decision 4 for *both*
   map shapes — not an in-PR change here, which would break `tests/rebalance.rs` that the brief
   pins green unmodified.
4. **No PR was opened, nothing pushed** — patch only, per STOP discipline.

Scratch: everything transient lived under `$PDCA_SCRATCH/pdca-builder-696-ablate` and was removed.
