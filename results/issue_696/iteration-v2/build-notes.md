# Build notes — #696 (iteration 2): rebalance reads through the resolver, contained

Withheld from the reviewer. Line citations are against the patched worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l0` (= `origin/main @ 339da46` + `patch.diff`); base citations say
"on the base".

---

## 1. What the patch does

Two files, exactly as budgeted.

**`crates/custodian/src/rebalance.rs`** (+94 semantic lines, cap 130; 205 raw added / 46 removed):

| Site | Change |
|---|---|
| `:136`, `:343` | `plan_evacuations` now answers `(Vec<EvacPlan>, Refusals)` — the moves it can prove, beside what it could not account for. |
| `:152` | `refused.total() > 0` → `Reconciled::Blocked`, ahead of `Changed`/`Satisfied`. The pass never claims more than it read. |
| `:172-218` | `Refusals` — ONE entry point per outcome, so a site can neither name without counting nor count without naming (Rule D). |
| `:238-244` | `metadata::decode` failure contained per object (`gc.rs:378-384`'s shape), never a `?` that ends the scan. |
| `:253-267` | The read goes through `metadata::resolve_chunk_map` — the one resolver GC (`gc.rs:402`), restore (`restore.rs:644`) and the read path share. Containment by **exactly** `gc.rs:405-415`'s downcast rule: `Ok(ChunkMapError)` is this record's fault, anything else propagates. |
| `:279-286` | **Rule A** — a resolve that restarted onto a generation this scan did not read (`Cow::Owned`, `metadata.rs:2629`) is contained: nothing planned, nothing reported, nothing written. |
| `:289-292` | **Rule B/D** — a segmented record is refused *once, per object*; the walk continues. |
| `:333`, `:442` | **Rule C** — the plan carries the `inode:` key **as the store spelled it**; the CAS uses it verbatim instead of `metadata::inode_key(parse(key))`. |
| `:387-389` | The second base `?`-site (`evacuate_chunk`'s `as_flat()`, base `:259`) becomes a fail-safe `Aborted` — unreachable in practice (only flat generations are planned) and never a whole-pass abort. |
| `:511-539` | `emit_unaccounted` / `emit_refused` on the durability seam, named with `gc::object_name` (`gc.rs:470`). |

Deleted: `parse_inode_key` (the Rule C hazard) and both
`ChunkMapError::SegmentedMapUnsupported` raises (base `:162`, `:259`).

**`crates/custodian/tests/segmented_map_rebalance.rs`** — NEW, 7 legs over one fixture
(327 semantic / 472 raw; caps 330 / 540).

The production hunks are salvaged from `results/issue_681/iteration-v7/patch.diff` as the brief
instructed, plus rules A and C, plus the two changes iteration 1's gates demanded (§2).

## 2. The carry-forward, item by item

### C5 — three surviving mutants (`rebalance.rs:199:32` ×2, `:205:22`)

Diagnosed from `iteration-v1/mutants.out` rather than guessed: all three survivors sat on the
**`unclassified` (v1: `unreadable`) counter**, and they survived for one reason — **no leg ever
seeded a chunk whose committed placement could not be classified**, so the `Err(_)` arm of
`checked_fragments()` never executed and the counter was always 0. `unreadable *= 1` and
`unreadable -= 1` were then unreachable, and `fragments + unreadable` was indistinguishable from
`fragments - unreadable`.

Two changes, both required (either alone leaves a survivor):

1. **The fixture now contains the fault** — leg 6 (`:432-457`) seeds a segmented object of three
   chunks: two fragments on the draining server **plus one whose committed `placement` is
   non-empty and of the wrong length for its scheme** (`vec![TARGET, DRAINING]` against
   `EcScheme::None`'s single fragment, `metadata.rs:204-230`). It asserts the accounting exactly:
   `"fragments":2`, `"unclassified":1`, one `refused` line, one `needs-human` line.
2. **The predicate lost its arithmetic** — `if fragments + unclassified == 0` became
   `if fragments == 0 && unclassified == 0` (`:208`). Not cosmetic: the `+`→`-` mutant class
   disappears entirely, and the three mutants the `&&`/`==` form *does* generate are each killed
   by an existing leg (`&&`→`||` by leg 2; `fragments != 0` by legs 1/5; `unclassified != 0` by
   leg 5).

Measured, not asserted: `scripts/mutants-in-diff` on this bundle →
**`25 mutants tested in 29s: 15 caught, 10 unviable`, 0 missed** (v1: 3 missed, 12 caught).
`mutants.out/caught.txt` shows both `:199:34` variants (`unclassified += 1`) and all three
`:208` predicate mutants now caught.

### T4 — two blocking review findings

Both are **out-of-scope / pre-settled**, so they are recorded-rejected rather than fixed, in
`review-rejected.md` (the file `review-batch.md`'s triage rule names). Neither is re-litigated in
the patch:

- `rebalance.rs:148` "an aborted evacuation … is not counted as a refusal" — the brief's
  `/ out of scope` assigns exactly this question to **#682**; `EvacOutcome::Aborted` is base
  behaviour this slice neither adds nor widens. `AGENTS.md` §Reviewer protocol says an
  out-of-scope finding gets a decline-with-issue-reference.
- the DST TEST-GAP — pre-declared settled at Plan (`Verification posture`), with the reason
  restated: no new destructive or concurrent path; the refusal writes nothing at all.

### T2 — 439 semantic lines against a 330 cap

Rewritten, not trimmed. Now **327 semantic / 472 raw**. What actually cost the lines was
rustfmt's `fn_call_width = 60`: every `assert_eq!` whose argument text exceeds 60 chars explodes
to 4-6 lines. The fixture is therefore expressed so each assertion's arguments stay short —
outcome/state/audit bundled into one tuple compared against one named `expected` binding
(`:305-308`, `:331-334`, `:369-372`, `:399-402`) — which reads better *and* fits. Other
structural savings: one `committed(size, map)` root builder shared by all three seeding paths
(`:187`), `stored`/`placement_at` accessors (`:215`, `:220`), one `NONCE` (v1 carried two), and
`scan_page`'s signature on one line. No leg lost an assertion in the process; legs 4, 5 and 6
each gained one (§3).

### C1 — "leg 5 is declared non-red but is actually base-red"

Confirmed, and it is a **brief accounting error, not something the patch can fix**: leg 5 must
hold a segmented object in the store, and on the base *any* segmented object makes the pass
`Err`. C4-verify's red leg here shows 6 of 7 red (legs 1-6), 1 pass (leg 7). Leg 7 is the only
genuinely non-red leg, and it is non-red by design. Nothing to do at Do; flagged so the human
can have Plan correct the brief's Falsifiability line at sign-off.

## 3. Two legs that were vacuous in v1 and are not now

**Leg 4 (Rule A) did not bind Rule A in v1.** Ablation-proven: delete the whole
`if !matches!(resolved.record, Cow::Borrowed(_))` block (`:279-286`) and v1's leg 4 would still
have passed — because the scanned record in that fixture is *segmented*, so the very next branch
(`:289`) refuses it anyway and the observable outcome (`Ok(Blocked)`, nothing written, nothing
copied) is identical. A restart is only ever possible **from** a segmented snapshot
(`metadata.rs:2585` returns `Cow::Borrowed` for a flat map without reading anything), so the two
guards overlap on outcome and differ only in what the pass *says*.

So leg 4 now asserts what genuinely differs: the object is named **`unresolvable-chunk-map`**,
and `lines(&log, "refused") == 0` (`:405-409`) — the pass may not describe an object with counts
drawn from a generation it never scanned. It also now seeds a **real** fragment on the draining
server via `write_flat_on` (`:390`), so "nothing was copied to the target" (`:400`) is a
statement about a move that *could* have happened rather than one that never could.
Ablation: removing `:279-286` → leg 4 FAILS, all six others pass.

**Leg 5 now runs the pass twice.** The brief's `step(false, true)` shape: pass 1 evacuates the
flat chunk (`Changed`), pass 2 must answer `Satisfied` over a store that still holds the healthy
segmented object. A single pass could only ever have observed `Changed`, which the
over-containment bug does not disturb.
Ablation: replace the `:208` guard's condition with `false` (the v7 adversary's exact move —
"replaced the guard's body with a no-op and all six legs still passed") → legs 5 and 1 FAIL.

**Rule C** is likewise ablation-proven: re-derive the CAS key as
`metadata::inode_key(parse(&plan.inode_key))` at `:442` → leg 3 FAILS with
`left: ([0], 1) / right: ([1], 2)` — `inode:007`'s fragment is never evacuated because the CAS
went to `inode:7`.

## 4. Alternatives considered and rejected

- **Keep v1's `fragments + unclassified == 0` and kill the `+`→`-` mutant with a second leg**
  (fixture with `fragments == unclassified == 1`, so the mutated guard early-returns). Rejected:
  it costs a whole extra `#[tokio::test]` (~18 semantic lines measured against the 330 cap that
  had just failed T2) to bind an operator that need not exist. The `&&` form is the same one
  line of code, states the intent directly ("nothing on a draining server **and** nothing
  unclassifiable"), and every mutant it generates is killed by a leg the brief already required.
- **Drop the `unclassified` count and refuse on `fragments > 0` alone.** Rejected on
  correctness, not size: a chunk whose placement cannot be classified may put fragments on the
  draining server and nothing can show it does not (`gc.rs:443-445` treats exactly this class as
  fully referenced, i.e. protected). Refusing only on countable fragments would let a drain
  certify over an object with an unreadable placement — the C-1 violation in miniature. It would
  have saved 4 semantic lines (`:194`, `:198-200`) and one leg assertion.
- **Report the refusal per chunk** (drop `Refusals::refuse`'s per-object aggregation, call
  `emit_refused` inside the loop): −6 semantic lines, and it is exactly the carried-forward
  finding "per-chunk logging floods the seam" (Rule D). Rejected; leg 6 binds against it.
- **Make `EvacOutcome::Aborted` non-certifying** (the T4 BUG finding). Out of scope per the brief
  and #682; see `review-rejected.md`. For the record the change itself is small — `:145-150`,
  ~4 lines — which is *why* it must not be smuggled in: it changes the operator contract for the
  no-free-domain and missing-fragment paths this patch never touches, and both `tests/rebalance.rs`
  legs that assert `Satisfied` over an aborted move would have to move with it.
- **Ship the test's doubles as a third file / shared module.** Refused by the budget ("a third
  file … means the shape is wrong: STOP"). Not attempted.

## 5. Forced refutation — the three questions

**(a) Genuine red?** Yes — proven by the project's own runner, `engine/scripts/run-verify.sh`
(the C4-verify gate), which applies the patch to a clean `origin/main` worktree, runs the test
(GREEN, 7/7), then **reverts `rebalance.rs` and keeps the test**:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_rebalance (fix applied)
... test result: FAILED. 1 passed; 6 failed
    a_fragment_in_a_seg_record_stays_put_refused_and_does_not_certify
    a_healthy_segmented_object_holding_nothing_draining_still_certifies_the_drain
    a_segmented_object_no_longer_stops_the_flat_evacuation_beside_it
    an_unreadable_object_is_named_the_walk_continues_and_nothing_certifies
    rule_a_the_pass_never_acts_on_a_generation_it_did_not_read
    rule_d_one_refusal_per_object_accounting_for_what_it_leaves_behind
run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red)
```

Every red is an **assertion/behaviour** red on base-visible symbols
(`Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" })`), never a compile
error: the discriminator names no symbol this patch introduces. Leg 7 passing on base is
declared and deliberate — it guards against over-containment, which has no base behaviour to
flip. Beyond the whole-patch revert, each guard was ablated individually (§3): every one of the
three has a leg that goes red without it.

**(b) Production path?** Yes. Every leg calls
`wyrd_custodian::reconcile_step(&zone, &leader, None, None, None, Some(&ctx), 500)`
(`tests/…:286`) — the real fenced control point (`reconciliation.rs:104`, fence at `:113`),
which dispatches the production `rebalance::reconcile` (`reconciliation.rs:140`). No internal
helper is called and nothing is re-implemented: the doubles are `MetadataStore` / `ChunkStore`
implementations (the seams the loop is defined over, ADR-0010), and the fence is real
`Custodian::elect` + `FencedZone` over `MemCoordination`. The chunk bytes leg 1/3/4/5 move are
built by the production writer (`plan_write` + `write_fragments`, `tests/…:252-256`) so
`repair::fragment_intact` (`rebalance.rs:290`) really passes; the records are built with the
real validating constructors (`SegmentRecord::new`, `SegmentedMap::new`, `metadata::encode`),
not hand-typed JSON.

**(c) Fixture includes the fault?** Yes, in every leg the failing element is *in* the store the
pass walks, not curated out:
- leg 2/6: the draining server really **holds** the fragment (`d0.put_fragment`, `tests/…:316`)
  and it is still there after the pass (`:326`);
- leg 3: the damaged pair sorts **first** in the `BTreeMap`'s key order (`inode:001`, `inode:002`
  before `inode:007`) — the v1 fixture put the healthy object first, so "the walk continued"
  could have passed on an implementation that abandons the walk at the first blocker. The
  fixture also asserts the seeded root really fails to resolve before relying on it
  (`tests/…:348-349`);
- leg 4: the live generation's fragment really sits on the draining server, so an unguarded pass
  would have copied it;
- leg 6: the malformed placement is a real `MalformedPlacement` (`metadata.rs:225`), seeded
  inside a `seg:` record the resolver actually reads.

## 6. Gates run here (the driver re-runs them at Check)

| Check | Result |
|---|---|
| `cargo test -p wyrd-custodian --test segmented_map_rebalance` | 7 passed |
| `cargo test -p wyrd-custodian` (whole crate, incl. the untouched `tests/rebalance.rs`) | all green — 10/15/16/13/8/7/5/3… passed, 0 failed |
| `engine/scripts/run-verify.sh` (C4-verify) | PASS — red without the fix, green with it |
| `scripts/mutants-in-diff` (C5) | 25 mutants: 15 caught, 10 unviable, **0 missed** |
| `cargo clippy -p wyrd-custodian --all-targets` | clean |
| `cargo fmt --all --check` | clean (commit-hook readiness) |
| `typos` on both touched files | clean |
| `./engine/xtask.sh ci` (C4-ci, whole tree: prose gates, fmt, clippy, build, tests, deny, conformance, statics, DST) | `xtask ci: all checks passed` (exit 0) |
| `run-verify.sh --classify` | `ADDED_TEST crates/custodian/tests/segmented_map_rebalance.rs` + `CRATE crates/custodian`, matching Plan's dry run |

Budgets: `src/rebalance.rs` **94** added semantic (cap 130); `tests/segmented_map_rebalance.rs`
**327** semantic / **472** raw (caps 330 / 540); **2** files.

## 7. For the human at sign-off

1. **The brief's Falsifiability line is wrong about leg 5** (§2, C1). Legs 1-6 are base-red; only
   leg 7 is not. Worth correcting in Plan's next brief for this family rather than in the patch.
2. **`Reconciled::Blocked` now reaches operators through `reconcile_step`'s `least_certified`
   fold** (`reconciliation.rs:55-61`): a store holding one segmented object with a draining
   fragment will report the whole *step* as Blocked, not just rebalance. That is the intended
   contract (the drain is genuinely not safe to certify), and it is the "fitness-to-purpose"
   question v1's review flagged as NEEDS-HUMAN — unchanged by this rebuild, still a maintainer
   call.
3. **No PR was opened, nothing pushed** — patch only, per STOP discipline.

Scratch: everything transient lived under `$PDCA_SCRATCH/pdca-builder-696-*` and was removed;
`mutants.out/` inside the worktree is gitignored and outside the diff.
