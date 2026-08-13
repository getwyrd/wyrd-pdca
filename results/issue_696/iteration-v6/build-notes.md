# Build notes — issue 696 / rebalance-reads-through-resolver-contained

Target branch: `getwyrd/wyrd @ main`, base `origin/main == 339da46` (verified in the worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`; `git rev-parse HEAD == origin/main == 339da46`).
Exactly **2 files**, as the brief's budget requires:

| File | Change | Size |
|---|---|---|
| `crates/custodian/src/rebalance.rs` | modified | **71** added semantic lines (cap 85) |
| `crates/custodian/tests/segmented_map_rebalance.rs` | NEW | **440** raw (cap 440) / 288 semantic (budget 265) |

---

## 1. What the change is, site by site

The defect is the two inline `record.chunk_map.as_flat().ok_or(SegmentedMapUnsupported)?` reads.
Both are replaced; nothing else in the file moves.

**(a) `plan_evacuations` — the scan (`rebalance.rs:158-164` on the base → `:181-293` after,
the resolve at `:215-234`, the refusal branch at `:239-242`).**
Three edits, each copied from the merged peer the brief names:

* `metadata::decode(&value)?` → contained per object, named, `continue`
  (`restore.rs:631-637` verbatim in shape; `gc.rs:378-384`). Emits `emit_unresolvable`.
* the `as_flat().ok_or(..)?` → `metadata::resolve_chunk_map(meta, &key, &record).await` with
  **exactly** the peers' downcast rule (`gc.rs:402-416`, `restore.rs:644-657`): `Ok(Some)` used,
  `Ok(None)` skipped (`gc.rs:404`, `restore.rs:646` — not counted, not named, per the brief),
  `Err → downcast::<ChunkMapError>()`: `Ok(fault)` contained + named, `Err(err)` **propagated**
  (leg 5).
* the chunk walk now iterates `resolved.chunks` instead of the raw flat vector; the eligibility
  decision is read off **`record.chunk_map`** — the shape the *scan* returned — so the §Scope
  constraint holds by construction (a flat snapshot resolves to a borrow and can never be
  `Superseded`, `crates/core/src/metadata.rs:2585`, `:2629`). No generation comparison exists,
  because the path that would need one is never reached (Rule A is out of scope, #699).

**(b) `evacuate_chunk` — the binding commit (`rebalance.rs:255-261` on the base → `:369-376`).**
`?` → `let Some(..) = .. else { return Ok(EvacOutcome::Aborted) }`. Plans are only ever built over
a flat generation, so this is unreachable in practice; the point is that one object's map may no
longer end **every other plan's** evacuation. It sits before the copy loop, so nothing is written.

**(c) The answer.** `plan_evacuations` now returns `(Vec<EvacPlan>, bool)`; `reconcile` answers
`Blocked` when the bool is set, else `Changed`/`Satisfied` unchanged — the shape `gc.rs:234-246`
already uses, folded by `Reconciled::least_certified` (`reconciliation.rs:55-61`) in
`reconcile_step`. **The work loop still runs**: containment changes the *claim*, never the work.

**(d) `refuse_segmented` + the two emitters.** Vocabulary exactly as pinned at Plan, both on
`"wyrd.custodian.rebalance.audit"`:
`action = "unresolvable-chunk-map"` + `monotonic_counter.rebalance_unresolvable_records`
(the string `gc.rs:563-571`, `restore.rs:827-830`, `scrub.rs:230-233` already publish), and
`action = "refused-segmented"` + `monotonic_counter.rebalance_refused_records`. Naming is by the
store's own key through `crate::gc::object_name` (`gc.rs:470-480`) — the cross-module use is
precedented by `crate::gc::orphan_key` at `rebalance.rs:316`(base) / `:431`(after). Attribution is
emitted **per object, where the object is read, before the work loop**, mirroring `gc.rs:155-166`.

### Frozen at the base, verified in the diff

`git diff origin/main -- crates/custodian/src/rebalance.rs` touches **none** of:
`parse_inode_key` and its skip (base `:152-154`, `:332-338`); `metadata::inode_key(plan.inode_id)`
and its `metadata::encode(&plan.prior)` precondition (base `:310-312`) — Rule C, **#698**;
the malformed-placement arm `checked_fragments() → emit_needs_human(chunk.id); continue;`
(base `:177-183`) and `emit_needs_human` (base `:372-380`). The segmented walk answers a malformed
placement the *same* way (`emit_needs_human`, no refusal), which is what "byte-identical …
including in whatever path now walks a segmented object's chunks" asks for; `crates/custodian/
tests/rebalance.rs:1412` (`Satisfied` at `:1457`, `PendingMalformed` at `:1491`) is green
**unmodified**, which is the mechanical proof that arm did not move.

## 2. Why a `bool`, not a count

The answer needs a predicate, not an arithmetic total; v1's three surviving C5 mutants were on
"the `unreadable` arithmetic/predicate". A `bool` removes the arithmetic entirely, and the
"once per object, never per chunk" property is bound where it is observable — the audit stream
(leg 2 counts the lines), not an internal counter no operator sees. C5 confirms: **0 survivors**.

## 3. Alternatives considered, with their cost

* **Keep `evacuate_chunk`'s `?` (touch one site only).** −3 lines, and every leg still passes,
  because `plan_evacuations` never plans a segmented record. Rejected: the brief's defect table
  names **two** sites and the Invariant to restore is about the *pass*, not the scan — an
  unreachable `?` that would abort the whole pass is the exact shape being removed. Cost of
  including it: 3 added lines (`let Some(..) else { return Ok(EvacOutcome::Aborted); };`).
* **Carry a `Refusals` struct with a count and typed entry points (v5's shape, 55 lines:
  `iteration-v5/patch.diff:100-160`).** Rejected on cost *and* on mutants: it buys a number
  nothing asserts, at ~35 more production lines than the `bool` + `refuse_segmented` (14 lines),
  and re-introduces the arithmetic v1's C5 run found unbound.
* **Refuse every segmented object (drop the `fragments == 0` guard).** −4 lines. Rejected: it is
  this slice's own defect in mirror image — no store holding a multipart object would ever certify
  a decommission. Leg 4 exists precisely to be red against it (and leg 1 also flips).
* **Plumb the resolved chunk list into `EvacPlan` so `evacuate_chunk` needs no second read.**
  Rejected as scope: it changes the CAS input shape (`plan.prior`), which is #682's file, and adds
  a field + clone to every plan (~8 lines) for no behavioural gain in this slice.
* **Count the malformed placement as a refusal** (the v5 finding `src:223`): rejected by the
  brief's three-part carve-out, and mechanically red — `crates/custodian/tests/rebalance.rs:1455`
  asserts `Reconciled::Satisfied` over exactly that fixture, and that suite must stay green
  unmodified.

## 4. The test — and the three refutation questions

`crates/custodian/tests/segmented_map_rebalance.rs`, five legs over ONE fixture (one `BTreeMap`
metadata double carrying the injected `get` fault, one pair of `ChunkStore` doubles, one
parameterised `seed`, one `run` that drives the fence *and* captures the audit seam).

**(a) Genuine red?** YES — proven by the project's own runner, not by inspection.
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → `PASS — red without the fix, green with it
(5 test(s) ran red)`. The RED leg reverts `rebalance.rs` and keeps the test: **5 tests ran** (so it
compiled — no exit-77 degradation) and all 5 failed on **assertions**, e.g.

```
a_healthy_segmented_object_holding_nothing_still_certifies_the_drain … assertion `left == right`
  left: Err("reconciliation store access: rebalance::plan_evacuations met a segmented chunk map…")
 right: Ok(Satisfied)
a_store_fault_under_the_resolver_still_ends_the_pass … THE injected fault: reconciliation store
  access: rebalance::plan_evacuations met a segmented chunk map, which this build cannot yet resolve
```

No assertion names a symbol this patch introduces (no `refuse_segmented`, no
`rebalance_refused_records` *type*; only the audit **strings**, which are data in the log, and
`Reconciled::Blocked`, which exists on the base at `reconciliation.rs:44`).

**(b) Production path?** YES — every leg calls `wyrd_custodian::reconcile_step` over a real
`Custodian::elect` + `FencedZone` (`rebalance::reconcile` is `pub(crate)`; there is no test-only
entry). Only `MetadataStore`/`ChunkStore` are doubles, as in every custodian suite. The fragments
are minted by the **production** encoder (`wyrd_core::write::plan_write`), so
`repair::fragment_intact` verifies them exactly as in production — a fixture of dummy bytes would
have made every evacuation silently `Aborted` and legs 1/3 vacuous.

**(c) Fixture includes the fault?** YES, and each leg asserts its own fault is real:
leg 3 asserts `metadata::resolve_chunk_map` genuinely `is_err()` on the seeded root before the pass
runs, and seeds both damaged records **first in key order** over a `BTreeMap`-backed store (a
fixture property, not luck), with the healthy object *behind* them at `inode:3`; leg 2's segmented
object genuinely holds two fragments on the draining server (asserted still there afterwards, with
`seg:` bytes and root `version` byte-identical); leg 4 asserts the segmented object is genuinely
healthy (`resolve_chunk_map` → `Ok(Some)` with all 3 chunks, every `checked_fragments()` `Ok`) so
the answer cannot be explained by a malformed arm — the round-4 T5 finding; leg 5 injects the
non-`ChunkMapError` store fault and asserts **that exact fault text** came back, so a pre-fix tree
that fails differently is red.

**On the `certifies` trap (round 3's `tests:168`/`:199`).** `run` returns
`Result<Reconciled, String>` and every leg asserts `assert_eq!(outcome, Ok(Reconciled::X))` — an
`Err` can never be read as "did not certify". Nothing folds a `Result` to a bool anywhere.

## 5. Budget: one number is over, stated with its arithmetic

* Production: **71** added semantic lines ≤ 85. ✔
* Test: **440 raw** = the cap (the brief's STOP threshold is "past 440 raw"); **288 semantic**
  vs the 265 figure — **23 over (8.7%)**. Measured as the brief measures it
  (`grep -vcE '^\s*//|^\s*$'`, which reproduces v5's quoted 329/507 to within one line).
  Where the 288 goes, so the human can judge rather than take an adjective:

  | Block (lines) | semantic |
  |---|---|
  | imports (21-42) | 19 |
  | `MemMeta` — 4 required `MetadataStore` methods + the injected fault (43-95) | 34 |
  | `MemDServer` — 5 required `ChunkStore` methods (96-124) | 22 |
  | `Capture` + `lines_with` — `io::Write` for the JSON audit capture (125-145) | 14 |
  | fixture consts, `seed`, `frag`/`store`/`holds`/`chunk_ref`/`commit`/`read_root`/`placed_at` (146-272) | 81 |
  | `run` — fence + elect + topology + capture (273-298) | 25 |
  | the five legs (299-440) | 93 |

  70 of those are the two trait doubles, which this crate ships no shared version of (checked:
  `wyrd-testkit` has `test_double_scan_page` and clocks/faults, **no** in-memory `MetadataStore` or
  `ChunkStore`), so every custodian suite duplicates them. I cut the file from 646 → 440 raw across
  four passes (folded `drive`/`drive_capturing` into one `run`, a 2-server topology instead of 4, a
  `doubles()` constructor, hoisted argument arrays and shortened assert messages so rustfmt's
  `fn_call_width = 60` stops splitting them). Going below ~285 would mean deleting assertions the
  brief requires, so I stopped at the raw cap rather than trade evidence for lines.

## 6. Gates run here (all in `$PDCA_WORKTREE`, via the project's own runners)

* `./engine/scripts/run-verify.sh` (C4-verify) → **PASS**, red→green, 5 tests red on the base.
* `./engine/xtask.sh ci` (C4-ci: typos, docs, fmt --check, clippy, build, whole-tree test, deny,
  conformance) → **`xtask ci: all checks passed`**. So the patch is commit-ready for the target's
  own hooks: `cargo fmt` was run over both files and `fmt --check` is inside that gate.
* `./scripts/mutants-in-diff` (C5) → **14 mutants: 4 caught, 10 unviable, 0 surviving.**
* `crates/custodian/tests/rebalance.rs` (16 tests) green **unmodified**, as required.
* ADR-0035 statics gate: my `static INIT: Once` lives in `tests/`, and `run_statics` scans only
  `<crate>/src` (`xtask/src/main.rs:1303`) — same shape as `tests/rebalance.rs:364`.

## 7. Standing rejections a reviewer may re-raise (do not re-fix)

Each is settled in the brief; the reference is here so it can be record-rejected in one line:

* *"remove `parse_inode_key` / CAS on the store's own key"* → Rule C, **#698**, lines frozen and
  unchanged from `origin/main`; removing it is what blocked #695 rounds 3 & 5 and #696 round 4.
* *"add a generation-restart comparison / a seeded Tier-0 DST leg"* → Rule A, **#699**. This patch
  adds a `continue`, never a write; every write it performs is on a flat chunk resolved by borrow
  (`metadata.rs:2585`) under the base's unmodified CAS. `crates/dst/` is not a file this bundle may
  touch. Already in `review-rejected.md`.
* *"a malformed placement should withhold certification"* → the three-part carve-out in §Scope;
  `desired_state::reconciliation_status` answers `PendingMalformed` cluster-wide
  (`desired_state.rs:234-246`); restore-side analogue is **#690**.
* *"partial progress past an unreadable record is a loss risk"* → `review-rejected.md`
  (`rebalance.rs:140`, `:148`): an evacuation orphan-**marks**, never deletes (`rebalance.rs:314-319`
  base), and GC's `ReferenceSet::protection` withholds every fragment in the fleet while any object
  is unresolvable (`gc.rs:306-312`, consulted at `:191-194`).

## 8. Declared residual (already in the brief's `Production reach` — not a surprise)

`desired_state::reconciliation_status` still answers a bare `Pending` for a *healthy* segmented
object holding a fragment on the draining server (`desired_state.rs:188-196`): `genuinely_holds` is
true, because the reference is real and resolvable. So an operator watching only the drain query is
told "an evacuation is running", which will not finish until **#682** builds the segmented write
path. Deliberately not fixed here (`desired_state.rs` is out of scope; what the query *should* say
about work refused-pending-#682 is #682's call, with the repair surface at #694). Net vs the base:
today one segmented object makes the whole pass `Err`, so no server drains anywhere and nothing is
named; after this slice every flat evacuation proceeds, the blocker is named per object on the
audit seam, and the loop refuses to certify.

No NEEDS-HUMAN external dependency: everything ran on the base Rust toolchain plus the five
registered `[[doctor.checks]]` tools, all present on this host.
