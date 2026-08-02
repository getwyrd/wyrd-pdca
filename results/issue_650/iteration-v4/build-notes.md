# Build notes — issue 650 / gc-scrub-through-resolver-fail-closed-containment

**Iteration 4.** Base `4e78aeb` (= `origin/pdca-integration/main`, carrying #648 and #649);
patch cut against it in `$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt`). Every
`path:line` below is on the **patched** worktree unless it says "base".

This round keeps iteration 3's design — it passed C1/C2/C4/C5/T1/T2/T3/T5, the whole
`cargo xtask ci`, and the mutation sweep — and changes exactly three things: the two blocking
review findings, and the scope finding the reviewer raised twice and the driver deferred twice.

## 1. The carry-forward, item by item

| Carry-forward item | Answer in this patch |
|---|---|
| **T4 blocker 1 — TEST-GAP** `segmented_map_consumers.rs:1206`: "Both expired leases are present when `gc_alone` runs, so it reclaims `order_b_garbage` before the combined reverse-order step and the later `is_none` assertion cannot prove that step contained a genuinely `Changed` result." | **Fixed at the cause: the seeding order.** Only `probe_garbage` is seeded up front (`crates/custodian/tests/segmented_map_consumers.rs:1203-1211`); `order_b_garbage` is seeded **after** the fixture probe has spent its own (`:1288`) and asserted **present on disk** immediately before the two-loop step (`:1289-1297`). Its absence afterwards (`:1317-1327`) is therefore that step's doing and nothing else's. Refuted by mutation — see §4. |
| **T4 blocker 2 — CONVENTION** `gc.rs:402`: "The newly added metadata resolution awaits external range/root reads without any timeout." | **Recorded rejection**, the brief's standing do-not-re-earn (i) (rejected 3× across #508/#636) — `results/issue_650/review-rejected.md` §(i), at the callsite line and its arms — **and** answered in the code where a reviewer meets it: `crates/custodian/src/gc.rs:393-401` now states who owns the bound, with the checkable facts. Cost of the alternative in §3. |
| **C3 (deferred twice, `deferred-findings.json`)** — "`PendingUnresolvable` and its rebalance coverage pull the brief's explicitly deferred desired-state surface into #650, changing the public status API and #651 boundary." | **Answered by removing the surface.** The public variant and `crates/custodian/tests/rebalance.rs`'s leg are gone. What is left in `desired_state` is **3 semantic lines** (`crates/custodian/src/desired_state.rs:188-190`) returning the **existing** `Pending`, plus an in-code `// deferred: #651` marker (`:183-187`) — a non-regression, not a surface. Why it cannot be zero lines: §3. |
| **T4 Contribution — tools unreproducible** | Not mine to answer: `scripts/review-branch` / `scripts/pdca` are PDCA-side tooling absent from the reviewer's artifact-only inputs. A sign-off item. |

## 2. The change — 11 files, 1,267 semantic added lines (129 production, 1,138 tests/docs/lock)

Budget: ≤ ~1,500 semantic lines, ≤ 15 files. The `Reconciled` third variant needed **no**
mechanical `match`-arm migration: every consumer outside `reconciliation.rs` compares with `==`
or forwards the value (`crates/server/src/custodian.rs:341` forwards it untouched), so the
brief's "22 files across 4 crates" allowance went unused — verified by
`cargo check --workspace --all-targets`.

* **`crates/custodian/src/gc.rs`** (+64 semantic) — the reference build resolves every committed
  record through the one shared resolver (`metadata::resolve_chunk_map`, `:402`), so a
  **segmented** object's chunks are in the protected set at all; `ReferenceSet::unresolvable`
  (`:294`, keyed by the record's raw bytes) carries what it could not read;
  `protection`/`protects` (`:306`, `:331`) say *why* a fragment is withheld and withhold
  everything while the set is incomplete; `reconcile` answers `Reconciled::Blocked` there
  (`:234-243`) and attributes each blocker **before** the fleet walk (`:164-166`,
  `emit_unresolvable` at `:563`). One damaged object does not end the walk: a record that will
  not decode (`:378-384`) or a generation the resolver types as a chunk-map anomaly
  (`:405-411`) is contained; a store fault under the read still propagates (`:414`).
  `object_name` (`:470`) is the injective operator name.
* **`crates/custodian/src/scrub.rs`** (+16) — the same set, its own attribution (`:114-116`,
  `emit_unscrubbable` at `:229`), and the **same answer for the same condition** (`:210`).
* **`crates/custodian/src/reconciliation.rs`** (+30) — `Reconciled::Blocked` (`:44`) and
  `least_certified` (`:55-61`), folded through every loop in `reconcile_step` (`:118`…`:139`),
  so a step never claims more than its weakest loop.
* **`crates/custodian/src/desired_state.rs`** (+3) — the non-regression guard (`:188-190`) and
  the deferral marker (`:183-187`); the `Pending` doc gains the second case it now covers
  (`:85-90`). No public API change.
* **`crates/core/src/metadata.rs`** (+16) — kept from iteration 2's C5 fix:
  `ChunkMapError::RootRecordUndecodable` (`:574`) and `decode_root_record` (`:2404`), used at
  **both** root re-reads (`:2243`, `:2571`), so a root rewritten unreadable *under* a resolve is
  still one object's fault and not an unattributable store outage.
* **Tests** — the added discriminator `crates/custodian/tests/segmented_map_consumers.rs`
  (8 legs, 925 semantic lines), the positive `Reconciled::Blocked` matches it cannot carry
  (`crates/custodian/tests/gc.rs:860-864`, `crates/custodian/tests/scrub.rs:1069-1073`), and the seeded
  Tier-0 DST property `crates/dst/tests/custodian.rs:1548-1715` (three arms drawn from the run
  seed: readable, retired-mid-resolve, incomplete). The `tests/gc.rs` leg is #648's own test
  **rewritten in place** (`:788-877`), not deleted: #648 asserted the placeholder contract its
  doc comment named ("no resolver reads `seg:` records until #649", so the pass aborts on the
  shape); with the resolver wired in, the same fixture now asserts the superseding contract —
  the pass completes, certifies nothing, and reclaims nothing fleet-wide.
  `ChunkMapError::SegmentedMapUnsupported` stays live: `restore::committed_chunks`
  (`crates/custodian/src/restore.rs:387`) still raises it, which is #651's to route.
* **`docs/design/architecture/06-runtime-view.md:31`** — the containment sentences the brief
  names, and only those; the report-only clause now states what this slice actually lands
  (never "satisfied" over an incomplete set) rather than the attributed answer #651 will add.
* **`Cargo.lock`** (+2) — `event-listener` 5.4.1 → 5.4.2. Not cosmetic: with the lockfile
  reverted, `cargo deny check advisories` prints `advisories FAILED` for RUSTSEC-2026-0221
  (reproduced this round), and `cargo deny` is inside the gating `cargo xtask ci`.

## 3. What I ruled out, with the cost

**(a) Keeping iteration 3's `ReconciliationStatus::PendingUnresolvable` — rejected on the
brief's own scope line.** Cost of that approach, measured on iteration 3's patch:
`crates/custodian/src/desired_state.rs` +33 raw / +11 semantic (a new **public** enum variant
with a `Vec<String>` payload, plus its return site) and `crates/custodian/tests/rebalance.rs`
+46 raw — in two files the brief lists under *Out of scope* ("restore, reconstruction, backfill,
rebalance and `desired_state` (#651) — do not route them here"), and on a public status API
#651 must then re-cut. Two independent review passes called it blocking scope. My replacement
costs **3 semantic lines and no API change**.

**(b) Touching `desired_state.rs` not at all — rejected, because it is not free.** On the base
tree, an unreadable committed record makes that query **fail**: `referenced_fragments`
propagates the decode error (base `crates/custodian/src/gc.rs:256`) or refuses the segmented
shape (base `:263-269`), and `reconciliation_status` awaits it with `?` (base
`crates/custodian/src/desired_state.rs:157`). Routing the build through the resolver *without*
the guard would silently turn that `Err` into `Ok(Satisfied)` — "this server is safe to
decommission" computed from a reference set the system knows is partial, a permanent,
data-losing outcome from a report (C-1, `docs/principles.md` §5; the brief's own invariant:
"a pass with only the first tells an operator to decommission a server whose bytes a live
object may still own"). So "0 lines" would have been a **C-1 regression introduced by this
patch**, not minimalism. Verified: with the guard disabled
(`if false && !referenced.unresolvable.is_empty()`), two fixture legs go red
(`segmented_map_consumers.rs:721`, `:1094`).

*Also considered:* keeping the base's hard `Err` for that query (construct an error from
`unresolvable`). Same line count, worse answer — it ends the query for a *healthy* server's
drain over another object's damage, which is the "ending the walk" failure the brief's third
invariant rules out. `Pending` is the containment-shaped answer: honest, non-certifying, and
already in the enum.

**(c) A caller-side timeout around the resolver await — standing rejection (i), and the cost is
a dependency, not lines.** `wyrd-custodian` has **no production `tokio` dependency**
(`crates/custodian/Cargo.toml`: `tokio` is under `[dev-dependencies]`), and the crate's declared
boundary is `traits` / `core` / `tracing` (ADR-0010, `crates/custodian/src/gc.rs:27-28`). A
`tokio::time::timeout` at `gc.rs:402` therefore buys a bound on **one** of the pass's many
awaits by adding a production runtime dependency to a seam-only crate — while
`meta.scan(b"inode:")` eight lines above (`:365`), and every await in the other three custodian
loops, stay unbounded because the store implementation owns the bound (#508/#636, 3×). The
rubric's clause is "bounded (timeout, **fail-closed**)"; this await is fail-closed by
construction — an error either propagates or contains the object, never "this object owns no
bytes".

**(d) Fixing the order-B test gap with a second, independent converging store — rejected on
cost.** That needs a second `MemMeta` + `MemDServer`, a second committed reference, a second
corrupt fragment and a second `GcContext`/`ScrubContext` pair: ~28 lines duplicating
`segmented_map_consumers.rs:1174-1206`, and it would *weaken* the leg — the point of order B is
that the **same** GC context that answered `Changed` on its own is lowered when a blocked loop
runs beside it. Re-ordering the seeding costs **12 lines** and keeps that identity.

**(e) Narrowing `protects` to the unreadable object's own chunks — settled rejection (iv), not
re-litigated.** Impossible in principle: an unreadable map's chunk ids are exactly what it
withholds, so no fragment in the fleet can be shown not to be one of them.

## 4. Refutation — the three questions, answered with evidence

**(a) Genuine red?** **Yes.** The project's own runner proves it, not a hand-rolled command:
`./engine/scripts/run-verify.sh` (the `C4-verify` gate) applies `patch.diff` to a clean
`../wyrd-verify` worktree off `origin/pdca-integration/main`, then reverts the production files
and re-runs the added target:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
test result: ok. 8 passed; 0 failed
run-verify.sh: RED — cargo test -p wyrd-custodian --test segmented_map_consumers (production reverted, test kept)
test result: FAILED. 0 passed; 8 failed
run-verify.sh: PASS — red without the fix, green with it.
```

All 8 legs fail by **assertion/`expect`**, not compile error (the red-leg panics read
`... : Store(SegmentedMapUnsupported { operation: "gc::referenced_fragments" })`), because the
file names only base-visible symbols — `Reconciled::Blocked` is asserted as
`!= Satisfied && != Changed`, and the positive match ships in `tests/{gc,scrub}.rs`, which
`C4-ci` runs. `--classify` confirms the single discriminator
`ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`.

Beyond the whole-file red, the two legs this round rewrote were refuted **individually** by
mutating production and watching them fail:

* precedence — reordering `least_certified` (`reconciliation.rs:57-58`) so `Changed` outranks
  `Blocked`: order A fails at `segmented_map_consumers.rs:1239` (`got Changed`); with order A's
  assertion temporarily disabled, **order B** fails at `:1306` (`got Changed`) — i.e. the
  reverse-order leg genuinely pairs a `Changed` GC with a refusal now, which is exactly what the
  carry-forward said it did not;
* drain non-regression — disabling the `desired_state` guard: `:721` and `:1094` go red.

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step` — the real
fenced control point (`crates/custodian/src/reconciliation.rs:104`) — over a real
`FencedZone`/`Custodian` from `wyrd_coordination_mem`, and the reference set is built by the
production `gc::referenced_fragments` calling the production
`wyrd_core::metadata::resolve_chunk_map`. Nothing is re-implemented in the test: the only test
code is the two in-memory `MetadataStore`/`ChunkStore` doubles (the ADR-0010 seams the loops are
specified over) and the raw-record seeding, which writes exactly the bytes `metadata::encode` /
`metadata::seg_key` put in a store. The drain answer comes from the production
`desired_state::reconciliation_status`. The DST leg
(`crates/dst/tests/custodian.rs:1548-1715`) drives the same entry point under madsim with the
arm drawn from the run seed.

**(c) Fixture includes the fault?** **Yes**, and each fault is asserted to *be* a fault before
it is relied on. `seed_damaged` (`segmented_map_consumers.rs:465-478`) asserts the seeded root
genuinely fails to resolve, so a leg can never pass because the fault quietly stopped being one.
The store the pass walks always contains the damaged record — `inode:1` sorts **before** the
healthy `inode:2` and the double is a `BTreeMap` (`:72-82`), so the build meets the blocker
**first**; a walk that abandoned there would fail the continuation legs rather than skip them.
Nothing is curated out: the fleet in the containment legs holds the damaged object's own
fragment *and* an unrelated, plainly collectable expired-lease/orphan fragment, and both must
survive; the rewritten-root leg asserts its scripted rewrites were actually **spent** (`:1002`),
so a race that never happened cannot pass. In the DST property, every fragment of the generation
that is live *while the pass runs* carries a lapsed grace record, so protection is load-bearing
on all three arms.

## 5. Gates run here (the same commands the Check gates use)

* `./engine/xtask.sh ci` → **all checks passed** (fmt, clippy `-D warnings`, build, workspace
  tests incl. DST, `cargo deny check`, conformance, statics, and the prose gates — `typos-cli
  1.48.0` and the docs renderer are both installed here, so neither warn-skipped).
* `./engine/scripts/run-verify.sh` → **PASS** (red→green, above).
* `scripts/mutants-in-diff` → `30 mutants tested in 33s: 6 caught, 24 unviable` — **no
  survivors** on this diff.
* `cargo check --workspace --all-targets` → clean, i.e. the new `Reconciled` variant needs no
  match-arm migration anywhere in the workspace.

All four were re-run on the **final** tree, after the last edit (the `desired_state` comment),
and `patch.diff` in this bundle is byte-identical to `git diff` on that tree.

## 6. External dependencies

None missing. The brief named `typos` and `docs-renderer`; both are present in this environment
(`typos-cli 1.48.0`; `python3 -c "import markdown_it, yaml"` succeeds), so `cargo xtask ci` ran
the prose gates rather than warn-skipping them. No Docker, no protoc, no live backend, no new
dev-dependency. **No NEEDS-HUMAN external dependency for this bundle.**

## 7. For the human at sign-off

The one judgment call worth your eye is §3(a)/(b): I read the brief's *Out of scope* line as
binding for `desired_state`, so the drain surface gets the **non-certifying** answer here and
its **attributed** answer in #651. If you would rather have the attribution now, iteration 3's
`PendingUnresolvable` is preserved verbatim in `iteration-v3/patch.diff` and re-applies cleanly
on top of this one — but it changes a public API the brief assigns to the next slice.
