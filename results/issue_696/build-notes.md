# Build notes — issue 696 (iteration 9)

> Withheld from the reviewer; written for the human at sign-off.
>
> **Delta round.** Iteration 8 came back with every gating gate green (`C4-ci` pass,
> `C4-verify` PASS red→green over 5 tests, `T4-batch-review` **0 blocking**,
> `T4-contribution` pass) and exactly one implementation-level item in §6:
>
> > **T5 Judgment — NEEDS-HUMAN [impl]:** *Require the unreadable-object leg to assert
> > `rebalance_unresolvable_records` — the producer emits this required counter, but the test
> > checks only global action/name substrings, so removing the counter alone would remain
> > green (`crates/custodian/src/rebalance.rs:521`,
> > `crates/custodian/tests/segmented_map_rebalance.rs:374`).*
>
> This iteration carries iteration 8's production change forward **unchanged** (it is the
> approach the reviewer PASSed on C1–C5/T1–T3, and the carry-forward asks for a *test* fix, not
> a different fix) and closes that gap in the test, plus the same class one line over in leg 2.
> Nothing else moved. Everything below the "What changed this round" section is the standing
> rationale for the patch as a whole, kept so the human is not required to page back through
> `iteration-v8/build-notes.md`.

## 1. What changed this round (the whole delta, 2 code lines + 7 comment lines)

All in `crates/custodian/tests/segmented_map_rebalance.rs`; **no production line moved**
(`git diff` of `src/rebalance.rs` is byte-identical to `iteration-v8/patch.diff`'s — verified by
diffing the regenerated patch against the worktree).

| Where | Before (v8) | Now | Why |
|---|---|---|---|
| leg 3, `tests:385-386` | *(absent)* | `let counter = logged.matches(r#""monotonic_counter.rebalance_unresolvable_records":1"#);` + `assert_eq!(counter.count(), DAMAGED.len(), …)` | the carry-forward finding: the brief pins `monotonic_counter.rebalance_unresolvable_records` as required vocabulary and says *"each item MUST be asserted by a leg above"*; leg 3 asserted the action string only, so deleting `src:521` alone stayed green. Now bound **by the whole emitted field** (prefix + name + value) **and once per damaged object** — `DAMAGED.len()` == 2, which additionally binds the per-object (not per-chunk, not per-fault-class) emission rule for the unreadable arm, matching what leg 2 already binds for the refusal arm. |
| leg 2, `tests:353` | `logged.contains("rebalance_refused_records")` | `logged.contains(r#""monotonic_counter.rebalance_refused_records":1"#)` | same defect class one line over, at **zero** line cost: a bare name substring still passes if the emitter loses the `monotonic_counter.` prefix — and without that prefix `tracing-opentelemetry` never exports it, so the metric an operator alerts on silently disappears while the test stays green. |

The two comment blocks (`tests:348-350`, `tests:381-384`) say why the assertion is on the whole
field rather than a name substring, so the next round does not "simplify" it back.

### The mutation that proves it binds (the reviewer's exact hypothesis, run)

Removing **only** `crates/custodian/src/rebalance.rs:521`
(`tracing::warn!(monotonic_counter.rebalance_unresolvable_records = 1_u64);`) and running the
project's own runner over that mutant patch:

```
$ PDCA_BUNDLE=$PDCA_SCRATCH/pdca-builder-696-mutant ./engine/scripts/run-verify.sh
...
an_unreadable_committed_object_is_named_and_the_walk_continues --- FAILED
  panicked at crates/custodian/tests/segmented_map_rebalance.rs:386:
  assertion `left == right` failed: one per object: …   left: 0   right: 2
test result: FAILED. 4 passed; 1 failed
run-verify.sh: FAIL — the bundle's test does not pass with the fix applied
```

Under v8's test that mutant was **green**. That is the finding closed, measured rather than
asserted. (The scratch mutant bundle was removed afterwards; the worktree was restored and
re-verified byte-identical to `patch.diff`.)

## 2. Budget accounting (the brief's hard stop)

Measured with the same rule the reviewer used at T2 (added lines, non-blank, non-comment —
`//`, `///` and `//!` all excluded), which reproduced v8's published 67 / 263 exactly:

| Item | Cap | v8 | Now |
|---|---|---|---|
| files changed | **exactly 2** | 2 | **2** (`src/rebalance.rs`, `tests/segmented_map_rebalance.rs`) |
| `src/rebalance.rs` added semantic | ≤ 85 | 67 | **67** (unchanged) |
| `tests/…` semantic | ≤ 265 | 263 | **265** |
| `tests/…` raw | ≤ 440 | 413 | **421** |

The test file lands **exactly on** the 265 semantic cap. That constrained the shape of the fix
and is why two candidate strengthenings were rejected below rather than added.

## 3. Rejected alternatives this round — with the line cost, not an adjective

1. **Hoist the matched strings into consts** (`const UNRESOLVABLE: &str = …;`
   `const COUNTER: &str = …;` and reuse them in legs 2 and 3). Cost: **+2 semantic** for the two
   consts, **−1** for the `let action = …` line it would replace, **+1** for the new
   `assert_eq!` = **266 semantic**, i.e. **1 over the cap**, for zero added binding power.
   Rejected on the cap, not on taste.
2. **Correlate name and action on the same audit event** — replace leg 3's two independent
   `contains` checks with `logged.lines().filter(|l| l.contains(&name)).any(|l| l.contains(action))`.
   Strictly stronger (it would also catch an emitter that named object A with object B's action),
   cost **+1 semantic** (4-line loop body instead of 3) = **266**, again 1 over. The
   order-dependent one-line variant (`"action":"unresolvable-chunk-map","inode":"inode:{inode}"`
   as one contiguous substring) *is* free — it would even save a line — but it pins the JSON
   **field order** of `emit_unresolvable`, so a later reordering of the emitter's fields fails a
   test that has nothing to say about ordering. Rejected as brittleness bought with budget I do
   not have; the residual risk it covers (per-object cross-attribution) is already narrowed by
   the exact `DAMAGED.len()` counter count plus two distinct `inode:` names.
3. **Assert the counter through a metrics recorder rather than the log stream.** The repo
   publishes counters as `tracing` fields (`monotonic_counter.*`, e.g. the base's own
   `rebalance_malformed_placement`, `src/rebalance.rs:500`) and has no metrics-exporter test
   seam on `crates/custodian`; wiring one means a new dev-dependency, which the brief forbids
   ("no `Cargo.toml` change … adding one would trip the ADR-0003 audit"). The capture layer the
   sibling suites already use (`segmented_map_restore.rs:221-264`) reads the same event the
   exporter would.
4. **Re-open the production change** (a different containment shape). Explicitly *not* done: the
   carry-forward's finding is a test gap, the reviewer PASSed C3/C5 on this production shape, and
   `T4-batch-review` returned **0 blocking** over it. Changing it would discard that evidence and
   re-open five rounds of settled findings.

## 4. The standing rationale (carried from iteration 8, unchanged)

### 4.1 The defect and the change

The base reads the chunk map inline out of the inode record at two sites, each `?`-ing out of the
**whole** pass (`origin/main @ 339da46`):

* `rebalance.rs:158-164` in `plan_evacuations` — ends the evacuation scan for the entire store;
* `rebalance.rs:255-261` in `evacuate_chunk` — ends the binding evacuation commit.

So one segmented (multipart) object anywhere stops every drain in the store, and a record that
will not `decode` ends the walk at `:148` before any resolver is involved. The change (post-patch
line numbers in `$PDCA_WORKTREE`):

| Site | What it does now |
|---|---|
| `src/rebalance.rs:227-239` | `metadata::decode` failure is **contained**: named via `crate::gc::object_name`, counted, `continue` — the record's own bytes are in hand, so the fault is this object's and no store's. |
| `src/rebalance.rs:246-262` | the map is read through `metadata::resolve_chunk_map` — the ONE resolver gc/restore/scrub already share. `Ok(None)` → skip (as `gc.rs:404`, `restore.rs:646`); `Err` → **downcast**: `Ok(ChunkMapError)` is this object's fault (contained + named), `Err(other)` **propagates** (a store fault is not one object's) — exactly `gc.rs:402-416`. |
| `src/rebalance.rs:308-317` | an evacuation owed by a chunk whose bytes live in a `seg:` record is **refused** — nothing is written — and the object is named once at `:340-343`. |
| `src/rebalance.rs:96-103`, `:332`, `:383-388` | the commit is built from and conditioned on **the generation the scan returned** (`EvacPlan::prior_chunks`), so `evacuate_chunk`'s second "segmented ends the pass" site is **removed**, not guarded. |
| `src/rebalance.rs:156-168` (`EvacScan` at `:171-191`) | `scan.withheld` → `Reconciled::Blocked` (`reconciliation.rs:44`), reusing gc's shape (`gc.rs:234-246`); a healthy segmented object owing nothing still lets the pass answer `Satisfied`. |
| `src/rebalance.rs:509-549` | `emit_unresolvable` / `emit_refused` — the pinned vocabulary, on rebalance's existing audit target, mirroring `gc.rs:563-571`. |

The **invariant restored** is C-1 (`docs/principles.md:109`): the pass reads every committed
object the way every other consumer reads it, contains a fault to the object that owns it, and
never reports a drain satisfied over an evacuation it did not perform.

### 4.2 Frozen at the base, on purpose (checked, not assumed)

Verified byte-identical to `origin/main` in the patched file (string-containment check against
`git show origin/main:…`): `parse_inode_key` (base `:332-338`) and its skip (`:152-154`), the CAS
key + `encode(&plan.prior)` precondition (`:308-315`), and the malformed-placement arm
(`:177-183`) with `emit_needs_human` (`:372-380`). Those are the #698 / ADR-0040-decision-4
carve-outs; a finding on either is answered by the brief's §Scope, not by a patch.
`crates/custodian/tests/rebalance.rs` is untouched and green (10/10 in `cargo xtask ci`,
including `malformed_placement_rebalance_skips_and_leaves_fragment_in_place`).

### 4.3 Self-review against the target's standing rubric (`AGENTS.md` §Review rubric & protocol)

* *One clock per correctness lifecycle* — no clock read added; `now_millis` is still the caller's.
* *Narrow trait seams / dependency direction* — stays on `traits`/`core` + `tracing`; no
  `Cargo.toml` change; `crate::gc::object_name` reuses the cross-module path `:316` already opened
  for `orphan_key`.
* *Metadata validation boundaries* — decode faults surface as errors and are handled as errors,
  never as values.
* *Absent or unsupported entries — never silent success/skip* — every contained fault is named,
  counted and withholds certification; the one silent skip (`Ok(None)`) is the merged peers' rule.
* *Await discipline* — the added `resolve_chunk_map` await is the `MetadataStore`
  implementation's bound (#508/#636), the same rule `gc`/`restore` follow for the same call.
* *Test fidelity — a new destructive or concurrent path lands with seeded Tier-0 DST coverage* —
  no new destructive or concurrent path: the added code is a `continue`, never a write, and every
  write remains the base's own version-conditional CAS over a flat map resolved by borrow
  (`crates/core/src/metadata.rs:2585`, `:2629`). Pre-declared in the brief's Verification posture
  and recorded-rejected in `review-rejected.md`.
* *Docs currency* — no port/API/RPC/CLI flag/persisted field added;
  `docs/design/architecture/06-runtime-view.md` §6.2 already states this containment rule.
* *Deferrals are settled* — `// deferred: #682` markers are in the diff at the two arms they
  govern.
* *Every crate root forbids unsafe* — the new test file carries `#![forbid(unsafe_code)]`;
  `xtask unsafe-guard` green.

## 5. Verification — what I actually ran

Through the project's own runners only (no hand-rolled cargo invocation for the red→green):

1. `./engine/scripts/run-verify.sh --classify results/issue_696/patch.diff` →
   `ADDED_TEST crates/custodian/tests/segmented_map_rebalance.rs`, `CRATE crates/custodian` — the
   full red→green branch, as the brief predicted at Plan.
2. `PDCA_BUNDLE=results/issue_696 ./engine/scripts/run-verify.sh` →
   **`PASS — red without the fix, green with it (5 test(s) ran red)`**. GREEN 5/5; RED 0/5 with
   `rebalance.rs` reverted and the test kept, every failure the *behavioural* base error
   (`rebalance::plan_evacuations met a segmented chunk map, which this build cannot yet resolve`)
   — an assertion red, not a compile red (no leg names a symbol this patch introduces).
3. `./engine/xtask.sh ci` in `$PDCA_WORKTREE` → **`xtask ci: all checks passed`** (exit 0):
   prose gates, `fmt --check`, `clippy -D warnings`, build, workspace tests
   (`tests/rebalance.rs` 10/10, `tests/segmented_map_rebalance.rs` 5/5), `cargo deny`,
   conformance 5+6 vectors, statics (ADR-0035), deploy-guard, unsafe-guard, DST.
   → **commit-ready**: the target has no `.pre-commit-config.yaml`/`.githooks`; its commit-time
   checks *are* `cargo xtask ci`'s fmt+clippy, and both are green on both touched files.
4. The counter mutant of §1 (`src:521` deleted) → leg 3 red, 4/5 green.

## 6. The forced refutation — all three answered, with the evidence

* **(a) Genuine red?** **Yes.** Not argued — run: `run-verify.sh`'s RED leg reverts
  `crates/custodian/src/rebalance.rs` and keeps the test; **0 passed, 5 failed**, each panicking on
  a base *behaviour* (`the pass must COMPLETE and answer: … met a segmented chunk map`,
  `tests:155`) rather than a build error. And for the *delta* this round specifically, the
  narrower revert in §1 (delete only the counter line) turns leg 3 red on its own — so the new
  assertion binds the thing it names, not just the surrounding fix.
* **(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step`
  (`reconciliation.rs:104-112`, rebalance arm `:138-144`) — the real fenced control point —
  through `Custodian::elect` + `FencedZone::new` over `wyrd_coordination_mem::MemCoordination`
  (`tests:195-197`, in the one `Fixture::pass` helper at `tests:179-205`).
  `rebalance::reconcile` is `pub(crate)`; no test-only entry point, no
  re-implementation, and the patch adds no `pub fn` for the test to call. Only the `MetadataStore`
  and `ChunkStore` backends are doubles — the same construction every existing custodian suite
  uses. The audit assertions read the stream the **production** emitters wrote, captured off a
  real `tracing` dispatch (`tests:198-203`), with the `Once`-installed permissive global default
  (`tests:184-186`) that keeps `Interest` caching from silently emptying the capture (wyrd #214).
* **(c) Fixture includes the fault?** **Yes**, and the fixture proves its own faults rather than
  assuming them. `seed_segmented` asserts through the real `resolve_chunk_map` that the object it
  just wrote is resolvable-and-well-formed **iff** `whole` (`tests:246-249`) — so leg 3's damaged
  object is *verified* unreadable and leg 4's healthy one is *verified* healthy (that assert is
  what makes leg 4 isolate the over-containment guard). Leg 3's undecodable record asserts
  `decode::<InodeRecord>(&garbage).is_err()` before seeding (`tests:367-368`); the damaged records
  are `inode:1`/`inode:2` in a **`BTreeMap`**-backed store so the walk meets them *first* — the
  healthy flat evacuation behind them cannot pass by ordering luck. Leg 2 compares **every key in
  the store, byte for byte, before vs after** (`tests:332`, `:343-344`) — including the root's
  encoded `version` — so "a refusal writes nothing" is measured, not trusted. Leg 5 injects the
  store fault into the live double (`tests:414`) and asserts the pass carries *that* error out.
  Nothing was curated out.

## 7. Open for the human at sign-off (not fixed here, by design)

No missing external dependency: everything ran on the base toolchain over in-memory doubles — no
Docker, no protoc, no live backend, no new dev-dependency. Nothing to declare there.

Two items in v8's §6 were **not** implementation findings and are unchanged by this round:

1. **T4 Contribution (NEEDS-HUMAN, rounds 6–8).** The reviewer could replay merged affected-path
   history (`3e05891` is the nearest change to `crates/custodian/src/rebalance.rs`, unchanged
   since) but was not handed `scripts/review-branch` / `scripts/pdca`, their gate logs, or the
   prior-attempt artifacts, so it could not independently re-run those rows. That is a
   *reviewer-context* gap in the harness, not a property of this patch — there is no target-repo
   edit that closes it. The bundle's own logs (`gate-logs/`, `iteration-v*/`) are what the human
   replays.
2. **Validation — fitness-to-purpose (NEEDS-HUMAN, 5/5 rounds, pre-declared in the brief).** For a
   *healthy* segmented object holding a fragment on the draining server, this slice makes the loop
   answer `Blocked` and name the object on `wyrd.custodian.rebalance.audit`, but it does not touch
   `desired_state::reconciliation_status`, which still answers a bare `Pending`
   (`desired_state.rs:188-196`) because the reference is genuine and resolvable. So an operator
   watching only the drain query still sees "an evacuation is running" for work that cannot finish
   until **#682** builds the segmented write path. Deliberately out of scope (`desired_state.rs`
   is carved out; closing it is #682's call with the repair surface at #694). Net against the base
   it is still strictly better: today one segmented object makes the whole pass `Err`, so **no**
   server drains anywhere and nothing is named. The sign-off question is "is `Blocked` + named
   audit attribution the right interim operator contract until #682".

## 8. Artifacts

* `patch.diff` — 2 files, regenerated from `$PDCA_WORKTREE`
  (`/home/eddie/wyrd/wyrd.pdca-wt-l1`) and verified byte-identical to that tree's `git diff`.
* The test ships **inside** `patch.diff` at the path the brief names,
  `crates/custodian/tests/segmented_map_rebalance.rs` (a **new** file — that is what earns
  C4-verify's red leg, `engine/scripts/run-verify.sh:250-252`, `:454`, `:466-470`); it is not
  duplicated into the bundle, matching every prior iteration of this bundle.
* Scratch: `$PDCA_SCRATCH/pdca-builder-696-mutant` and
  `$PDCA_SCRATCH/pdca-builder-696-xtask-ci.log` were created for §1/§5 and removed at the end of
  the run.
* No branch pushed, no PR opened or marked ready.
