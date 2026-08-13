# Build notes — issue 696 / rebalance-reads-through-resolver-contained (iteration 7)

Target branch: `getwyrd/wyrd @ main`, base `origin/main @ 339da46` (verified: the worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l1` HEAD == `origin/main` == `339da46`). All `path:line`
citations below are against that base plus this patch.

Two files, as the brief's `Budget` requires:

| File | Change |
|---|---|
| `crates/custodian/src/rebalance.rs` | modified (the containment core) |
| `crates/custodian/tests/segmented_map_rebalance.rs` | **new** (5 legs, one fixture) |

---

## 1. What the patch does

**The defect (re-verified on the base).** `plan_evacuations` read the chunk map inline out of
the inode record and `?`-ed the whole scan out on the first segmented one
(`rebalance.rs:158-164` on the base), and `evacuate_chunk` did it a second time
(`rebalance.rs:255-261`). A single segmented object therefore stopped every drain in the
store, and a record that would not `decode` ended the walk even earlier (`:148`).

**The fix, mirroring the two merged peers rather than inventing.**

1. **Read through the shared resolver.** `plan_evacuations` now decodes contained
   (`rebalance.rs:232-239`), skips a non-committed record unchanged (`:240-242`), keeps
   `parse_inode_key` byte-identical to the base (`:243-245` — frozen, #698), and resolves
   through `metadata::resolve_chunk_map` (`:256-271`) with **exactly** the peers' downcast
   rule: `Ok(None)` is skipped (`:258`, as `gc.rs:404` / `restore.rs:646`), a
   `ChunkMapError` is contained as *this record's* fault (`:259-266`, as `gc.rs:405-411`),
   and any other error propagates (`:269`, as `gc.rs:414`). The await's bound is the
   `MetadataStore` implementation's, stated in the code at `:252-256` exactly as the peers
   state it (`gc.rs:394-401`, #508/#636).
2. **Eligibility comes off the generation the scan returned**, never off what the resolve
   answered: `let scanned_flat = record.chunk_map.as_flat();` (`rebalance.rs:276`), taken
   before the chunk walk and used both to decide the refusal (`:314-317`) and to supply the
   bytes the commit is built from (`:332`). That is the brief's §Scope constraint honoured
   *by construction*: a flat snapshot resolves to a borrow and never restarts
   (`crates/core/src/metadata.rs:2585`, `:2629`), and a segmented snapshot — the only one
   that can restart — is one this pass refuses. No generation comparison, no new counter
   (#699 stays closed).
3. **The second fault site is removed, not guarded.** `EvacPlan` carries the scanned flat
   chunk list (`rebalance.rs:96-103`), so `evacuate_chunk` reads `&plan.prior_chunks`
   (`:388`) instead of re-deriving the map's shape. There is no `ok_or(...)?` left in that
   function and no unreachable `Aborted` arm standing in for one (see §3, alternative A).
4. **Refusal accounting, per the brief's pinned vocabulary.** One `refused` flag per object
   (`rebalance.rs:279`, set at `:315`), emitted **once** after the chunk loop (`:340-343`)
   as `action = "refused-segmented"` + `monotonic_counter.rebalance_refused_records`
   (`:540-549`). An unreadable object is named where it is read, before the caller's work
   loop — mirroring `gc.rs:155-166` — as `action = "unresolvable-chunk-map"` +
   `monotonic_counter.rebalance_unresolvable_records` (`:520-529`), by `gc::object_name`
   (`gc.rs:470-480`). No new gauge, no new seam: `rebalance.rs` already reaches into
   `crate::gc` for `orphan_key` (base `:316`).
5. **The answer.** `EvacScan { plans, withheld }` (`rebalance.rs:171-190`, built at `:345`) feeds
   `reconcile`: `withheld` → `Reconciled::Blocked` (`:156-163`), else the base's
   `Changed`/`Satisfied` — the same shape and the same reason GC uses (`gc.rs:234-246`), and
   `Blocked` already existed on the base (`reconciliation.rs:44`), so the answer folds
   through `least_certified` unchanged (`reconciliation.rs:55-61`).

**What is deliberately frozen** (each is a brief carve-out, and each is now also visible in
the code so a reviewer meets the reference rather than re-deriving the question):

* `parse_inode_key` + the CAS key/precondition — byte-identical (#698).
* A malformed placement — `checked_fragments()` → `emit_needs_human(chunk.id); continue;`
  byte-identical (`rebalance.rs:292-298`), in the segmented path exactly as in the flat one;
  not a refusal, does not withhold. `EvacScan::withheld`'s doc says so and why
  (`:174-190`), pointing at `ReconciliationStatus::PendingMalformed`.
* `EvacOutcome::Aborted` — swallowed exactly as the base swallows it, now carrying an
  in-code `// deferred: #682` marker at the arm (`rebalance.rs:145-150`). See §6.

---

## 2. Red → green, through the project's own runner

`./engine/scripts/run-verify.sh` (the `C4-verify` gate command from `pdca.toml`), with
`PDCA_BUNDLE` set — it applies `patch.diff` in an isolated `../wyrd-verify` worktree off
`origin/main`, runs the shipped test green with the fix, then reverts the **production**
change and re-runs:

```
run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
```

The red is **behavioural**, not a compile error (the discriminator names no symbol this
patch introduces — it asserts on `Reconciled::{Satisfied,Changed,Blocked}` and
`ReconcileError`, all base-visible, plus the audit strings):

```
panicked at crates/custodian/tests/segmented_map_rebalance.rs:188:33:
the pass must COMPLETE and answer: reconciliation store access:
rebalance::plan_evacuations met a segmented chunk map, which this build cannot yet resolve
```
(all five legs; leg 5's red is at `:437` — the base's error does not carry the injected
store fault, so `answer.expect_err(..)` succeeds but the fault-string assertion fails.)

Also run in the worktree: `cargo test -p wyrd-custodian` — **all suites green**, including
`crates/custodian/tests/rebalance.rs` **unmodified** (the brief's condition), and
`cargo check --workspace --all-targets`, `cargo clippy -p wyrd-custodian --all-targets -D
warnings`, `cargo fmt --all -- --check`, and `typos` over both touched files: all clean.

---

## 3. Alternatives considered, with their cost

**A. Keep `evacuate_chunk`'s inline `as_flat()` and answer the `None` arm with
`EvacOutcome::Aborted`** (what the earlier, larger attempts did). Cost of the *chosen*
alternative instead: `EvacPlan` grows one field (`rebalance.rs:103`, +1 semantic line), the
plan construction grows one line (`:326`), and `evacuate_chunk` **loses 7 lines** (the base's
`:255-261` `ok_or(...)?` block) for one (`:388`) — net **−5 semantic lines**. Cost of A: a
*permanently unreachable* branch whose only possible answers are `Aborted` (which certifies —
the exact class of finding that has blocked this bundle twice) or a new dead `EvacOutcome`
variant. Removing the site is both smaller and strictly safer, so A was rejected on both axes.

**B. Collect the unreadable objects into a map and emit them in `reconcile` (GC's literal
shape, `gc.rs:164-166`).** Cost: `EvacScan` gains a `BTreeMap<Vec<u8>, String>` field, the two
fault arms each `insert` instead of emit, and `reconcile` grows a 3-line loop — about **+6
semantic lines** for zero behavioural difference here, because `plan_evacuations` *is* called
before the work loop (`rebalance.rs:140` vs the loop at `:143`). GC needs the map because its
builder is shared with scrub/restore/desired-state; rebalance's scan is its own. Emitting at
the fault site satisfies the brief's "per object, where the object is read, before the work
loop" at lower cost, so B was rejected.

**C. A `certifies(...) -> bool` helper in the test.** Rejected outright: the brief records
that round 3 lost the whole gate to a helper that folded `Err` into "did not certify". The
test instead asserts `Ok(..)` explicitly through `answered(..)`
(`tests/segmented_map_rebalance.rs:185-189`), which **panics** on `Err`.

**D. A generation-restart comparison / a seeded Tier-0 DST leg (the previous brief's "Rule
A").** Not built — the constraint in §1.2 closes the path instead of guarding it, and
`crates/dst/` is a file this bundle may not touch. Already recorded-rejected on this bundle
with that reasoning (`review-rejected.md`, the `src/rebalance.rs:259` and
`tests/segmented_map_rebalance.rs:379/400/408` entries).

---

## 4. Ablations — each leg is load-bearing

Run in the worktree by editing `rebalance.rs`, running the new test, then restoring
(`cargo test -p wyrd-custodian --test segmented_map_rebalance`):

| Ablation (production only) | Result |
|---|---|
| **A** — over-broad containment: refuse *any* segmented object, owed or not (`refused = true` before the `evac.is_empty()` guard) | legs **1 and 4 FAIL**, 2/3/5 pass — leg 4 is the isolating one (`Satisfied` → `Blocked` over a store whose flat work is complete) |
| **B** — never withhold: `Ok(if false && scan.withheld { .. })` | legs **2 and 3 FAIL**, 1/4/5 pass |
| **C** — contain *every* resolver error (drop the downcast rule, `Err(err)` → contain) | leg **5 FAILS** alone |
| **D** — refuse once per **chunk** instead of once per object | leg **2 FAILS** alone (`left: 2, right: 1` on the refusal count) |
| the base (production reverted, test kept) | **all five FAIL** — the `run-verify.sh` red leg |

Ablation D's captured stream also pins the emitted shape:
`{"level":"WARN","fields":{"message":"rebalance refused an evacuation …","action":"refused-segmented","inode":"inode:3","reason":"segmented-chunk-map"},"target":"wyrd.custodian.rebalance.audit"}`
beside `{"fields":{"monotonic_counter.rebalance_refused_records":1}}`.

---

## 5. The three refutation questions (forced, recorded)

**(a) Genuine red?** **Yes.** `run-verify.sh` reverts the production change, keeps the test,
and reports 5 tests running **red** (assertion/panic reds quoted in §2), then green with the
fix. Verified again by hand for each guard through ablations A–D (§4), where each ablation
reds exactly the legs that bind it and leaves the others green.

**(b) Production path?** **Yes.** Every leg drives
`wyrd_custodian::reconcile_step` (`reconciliation.rs:104-112`) → the rebalance arm
(`:138-144`) → `rebalance::reconcile` — the real fenced control point, entered through
`Custodian::elect` + `FencedZone::new` over `wyrd_coordination_mem::MemCoordination`
(`tests/segmented_map_rebalance.rs:210-222`). `rebalance::reconcile` is `pub(crate)`, so no
parallel entry is even reachable from a test. Only the `MetadataStore` / `ChunkStore`
backends are doubles — the shape every custodian suite uses. The fragments are **real** v1
on-disk-format bytes (`wyrd_chunk_format::encode` + `FragmentHeader::new_v1`,
`tests:300-305`), so `repair::fragment_intact` (`rebalance.rs:413`) genuinely accepts them
and the flat evacuation really commits through the base's unmodified CAS.

**(c) Fixture includes the fault?** **Yes**, and the fixture asserts its own faults rather
than assuming them. `seed_segmented` re-reads what it wrote through the **production**
resolver and asserts `resolve_chunk_map` errors iff the seeding was short
(`tests:263-270`) — so leg 3's damaged object is provably unreadable and leg 4's healthy
object is provably resolvable *with well-formed placements* (the round-4 T5 finding). Leg 3
additionally asserts its undecodable bytes really do not decode (`tests:389-390`); leg 5
injects the store fault into the double and requires the pass's error to **carry** it
(`tests:431`, `:435-437`) — an over-broad containment that swallowed it would go red. The
damaged records are met **first** by key order over a `BTreeMap`-backed store
(`tests:41-53`, `inode:1` < `inode:2` < `inode:9`), so "the healthy fragment was still
evacuated" cannot pass on a loop that quits at the first blocker. Nothing is curated out:
legs 1 and 3 keep the flat work *in* the same store as the fault and assert it happened
(`tests:320-324`, `:348-349`, `:396-397`).

---

## 6. Carry-forward from iteration 6 (the failing gate)

`T4-batch-review` blocked on one unchecked finding
(`results/issue_696/review-batch.md`): *"`crates/custodian/src/rebalance.rs:145` — `withheld`
is never set for `EvacOutcome::Aborted`, so an impossible flat-map evacuation … can still
return `Satisfied` while fragments remain on the draining server."* It did not pair with the
existing `:148` recorded rejection because that entry's MATCH phrase came from an earlier
round's wording.

Handled three ways, none of which re-submits the rejected approach:

1. **Recorded-rejected with a MATCH phrase from *this* finding's rationale**, appended to
   `review-rejected.md` (`rebalance.rs:150 | BUG | `withheld` is never set for
   `EvacOutcome::Aborted``), with the reason spelled out and checkable: `Aborted` is base
   behaviour (`origin/main @ 339da46` `rebalance.rs:128` swallows it; `:250`, `:277`, `:284`,
   `:287` raise it), the brief pins it out of scope for #682, and changing it would fail
   `crates/custodian/tests/rebalance.rs:916`
   (`spread_wins_when_no_free_distinct_domain_remains`, asserting `Reconciled::Satisfied` at
   `:966-970`) — a suite the brief requires stay green **unmodified**.
2. **Answered in the code**, at the mechanism the repo's own reviewer protocol treats as
   settled (`AGENTS.md` §"Reviewer protocol" → *Deferrals are settled*: an in-code
   `// deferred: #N` marker resolves a finding for review purposes): the arm now carries
   `// deferred: #682` (`rebalance.rs:145-150`), and `EvacScan::withheld` documents exactly
   what does and does not withhold certification, naming both carve-outs and their references
   (`rebalance.rs:174-190`).
3. **The finding's premise is false at the operator surface** and that is now recorded: an
   aborted evacuation leaves a *valid* committed placement naming the draining server, so
   `desired_state::reconciliation_status` answers `Pending`, not "safe to decommission"
   (`desired_state.rs:188-196`). `Reconciled::Satisfied` (one loop's convergence answer) and
   the drain query are different surfaces.

---

## 7. Budget accounting — honest, including one overrun

Measured on the formatted files (semantic = non-blank, non-comment):

| Budget | Cap | Actual | Verdict |
|---|---|---|---|
| files touched | exactly 2 | 2 | OK |
| `src/rebalance.rs` added semantic lines | ≤ 85 | **67** | OK |
| `tests/segmented_map_rebalance.rs` raw | ≤ 440 | **438** | OK (the brief's STOP trigger — not tripped) |
| `tests/segmented_map_rebalance.rs` semantic | ≤ 265 | **311** | **over by 46 — see below** |
| `crates/dst/` hunk | forbidden | none | OK |
| `Cargo.toml` change | forbidden | none | OK (`wyrd-chunk-format` is already a `crates/custodian` dev-dependency, `Cargo.toml:44`) |

The 46-line overrun is in the **fixture**, not the legs, and it survived four compression
passes (the file was 471 semantic before them). Where it goes, measured:

| Block | lines | semantic |
|---|---|---|
| module doc + imports + `#![forbid(unsafe_code)]` | 1-40 | 22 |
| `MemMeta` (`BTreeMap` double + injected `get` fault + the required `scan_page`) | 41-90 | 38 |
| `MemDServer` (the 5 required `ChunkStore` methods) | 91-117 | 24 |
| audit capture (`Capture` + `enable_audit_callsites`) | 118-145 | 18 |
| constants + `frag` / `chunk_ref` / `answered` | 146-190 | 28 |
| `Fixture` + its 10 methods (fence, pass, seeding, self-checks, readbacks) | 191-327 | 110 |
| the five legs themselves | 328-438 | 71 |
| **total** | | **311** |

Compressions already applied: `Arc<Capture>` uses `tracing-subscriber`'s `MakeWriter for
Arc<W>` blanket instead of a hand-written `MakeWriter` impl (−7); the fixture self-check
lives **inside** `seed_segmented` instead of two per-leg helpers (−20); assertion messages are
short enough to stay inside rustfmt's `fn_call_width = 60` (−40, since the repo has no
`rustfmt.toml` and every 3-argument `assert_eq!` past 60 chars explodes to 5 lines); one
segment-group nonce instead of two; three D servers instead of four. What is left is the
irreducible trait-double surface plus the five legs the brief mandates. Cutting further would
mean deleting a leg, a required trait method, or the fixture's own self-checks — each of which
the brief separately forbids. Flagging it rather than trimming the honesty out of the test.

---

## 8. Residuals for sign-off

* **Pre-declared operator-contract residual (the brief's `Production reach`, raised 5/5
  rounds).** This slice changes the *loop's* answer (`Reconciled::Blocked` + a named audit
  line); it does **not** change `desired_state::reconciliation_status`, which for a healthy
  segmented object holding a fragment on the draining server still answers a bare
  `ReconciliationStatus::Pending` (`desired_state.rs:188-196` — the fragment is a genuine,
  resolvable reference). So an operator watching only the drain query is told "an evacuation
  is running" that will not finish until **#682** builds the segmented write path. Net against
  the base this is still strictly better: today one segmented object makes the whole pass
  `Err`, so no server drains at all and nothing is named. The sign-off question is "is
  `Blocked` + named attribution the right contract until #682" — pre-declared, not a surprise.
* **No external dependency gap.** Everything ran on the plain Rust toolchain over in-memory
  doubles: no Docker, no protoc, no live backend, no new dependency, no DST leg. The five
  registered `[[doctor.checks]]` ids were not needed beyond `typos`, which was run and is
  clean.

Scratch: `${PDCA_SCRATCH}/pdca-builder-696-*` (the ablation backup and a diff comparison) —
removed at the end of the run.
