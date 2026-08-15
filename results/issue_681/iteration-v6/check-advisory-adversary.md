# Adversarial review — issue #681 (`passes-read-through-resolver-contained`)

Method: rebuilt the workspace from `$PDCA_TARGET` into scratch and re-ran the asserted proof
myself — green leg (patch applied) **6/6 pass**; red leg (the three production files reverted to
`HEAD`, the new test kept) **6/6 fail, compiling**, on behavioural assertions and `expect` panics,
not on a compile error. `C4-verify`'s claim survives. Then ran the whole `wyrd-custodian` suite
(all 95 pre-existing tests green, none edited) and `cargo test --workspace --exclude wyrd-dst`
(green; the single `xtask::scan_gitlinks_is_green_over_the_real_index` failure is my scratch copy
having no `.git`, not the patch). Then wrote five probe tests against the fixed tree to try to
break it. Findings below; two probes landed.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:511`: a refusal that leaves fragments
  on a decommissioning server ticks the refusal counter by ZERO.** `emit_refused` does
  `monotonic_counter.rebalance_evacuation_refused = fragments as u64`. `held_for_drain`
  (`rebalance.rs:184-195`) returns `(fragments, unreadable)` and `plan_evacuations`
  (`rebalance.rs:263-265`) refuses whenever `fragments + unreadable > 0` — so the case
  `fragments == 0, unreadable >= 1` (a segmented object whose chunks carry a malformed
  `placement`, the very case the `unreadable` counter was added for) emits a counter increment of
  **0**, i.e. records nothing at all on the metric seam for a refusal that is leaving fragments
  where nobody can see them. Concrete case, run against the patched tree: seed
  `Shape::Segmented(NONCE, 1, vec![seeded(S_DRAIN_A, &[0], &[])])` under `inode:20`, drain server
  3, run rebalance — the pass answers `Blocked` and the captured metric event is literally
  `{"monotonic_counter.rebalance_evacuation_refused":0}` beside an audit line reading
  `"fragments":0,"unreadable":1`. Every other refusal/decline counter this diff adds counts
  **objects** (`= 1_u64` at `backfill.rs:278`, `backfill.rs:292`, `rebalance.rs:496`,
  `reconstruction.rs:971`, `reconstruction.rs:1000`); this one alone counts fragments and so can
  count nothing. The discriminator drives exactly this store
  (`crates/custodian/tests/segmented_map_passes.rs:584-589`) but asserts only `Reconciled::Blocked`
  and never reads the counter, so no test would have gone red — and `C5-mutants`' "0 missed" does
  not cover it either, because mutating the *whole body* of `emit_refused` away is caught by
  leg 2's audit-line assertion at `segmented_map_passes.rs:549-557`.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:271`: the #350-step-2 gauge is now split
  by a varying integer OTel attribute, so the series an operator watches goes stale exactly when
  the incident starts.** `tracing::info!(gauge.backfill_placement_remaining = remaining,
  unaccounted,)` — `unaccounted` carries no metric prefix, and this workspace's bridge
  (`tracing-opentelemetry 0.33.0`, `src/metrics.rs:198-201`: the non-prefixed arm pushes
  `KeyValue::new(field.name(), Value::I64(..))` into `attributes`) therefore records it as an
  **attribute on the gauge**, not as its own instrument. Concrete consequence: on a healthy store
  the population is published as `backfill_placement_remaining{unaccounted="0"}`; the pass after a
  record becomes undecodable it is published as `{unaccounted="1"}` instead, and under cumulative
  temporality the `{unaccounted="0"}` series keeps being exported at its last, pre-incident value.
  An alert or dashboard keyed on the metric name now reads two contradicting series, and the base
  emitted exactly one unlabelled one. The cited precedent, `emit_domain_utilization`
  (`rebalance.rs:454-460`), labels by a *bounded, stable* `domain` string — not by a count. Fix
  shape that keeps the brief's "one sample carries both": give the caveat its own `gauge.`-prefixed
  field on the same event, so the visitor produces two instruments and zero attributes. No test in
  the bundle exercises the bridge — `assert_gauge`
  (`crates/custodian/tests/segmented_map_passes.rs:178-192`) reads the JSON *log* layer, where a
  field and an attribute look identical.

- **NEEDS-HUMAN [human] — the brief's line budget is exceeded and no gate measured it
  (`check-gates.json:66-73`, `"T2 Shape": "none"`).** Counting added lines that are non-blank and
  not comment-only (the same methodology the brief's own v2 table implies — its ratios reproduce):
  `tests/segmented_map_passes.rs` **544 semantic vs the pinned ≤ 470**, `src/rebalance.rs` **103 vs
  ≤ 100**, `src/reconstruction.rs` 180 (≤ 210 ✓), `src/backfill.rs` 88 (≤ 100 ✓) — **total 915 vs
  the pinned ≤ 880**. The brief's *hard* STOP thresholds are not tripped (4 files; test file 775
  raw ≤ 780; 1466 raw ≤ 1520), which is presumably why this passed unnoticed, but iterations 4 and
  5 were both returned for this class (the last one for eight lines). A human should decide whether
  a 16 %-over test allocation is accepted or trimmed; my semantic counter is mine, not the
  project's, so the exact numbers want re-measuring with whatever tool the earlier "788 raw / 780
  cap" finding used.

- **NEEDS-HUMAN [human] — `crates/custodian/src/desired_state.rs:191-197`: the pass now refuses
  forever, but the operator-facing decommission query still answers bare `Pending`.** After this
  patch a draining server holding a fragment of a *segmented* object gets
  `ReconciliationStatus::Pending` — because `gc::referenced_fragments` (`gc.rs:402-435`) resolves
  segmented maps into `placed`, so `genuinely_holds` is true — while `plan_evacuations`
  (`rebalance.rs:260-267`) has just decided that fragment will **never** move until #682 lands.
  `Pending` is documented in that same file (`desired_state.rs:219-224`) as the answer meaning
  "the rebalance loop is moving them", and lines 205-218 say in as many words that a wait with
  nothing to act on is the permanence C-1 forbids "reached through the report instead of through a
  deletion" — which is why the unreadable case got its own attributed
  `PendingUnresolvable`. The base was equally stuck, but loudly (the pass returned `Err`); this
  slice makes the stall quiet and steady, and `desired_state.rs` is explicitly out of the brief's
  scope with no in-code deferral marker covering this case (the `#682` markers at
  `rebalance.rs:203`, `:252`, `backfill.rs:153`, `reconstruction.rs:359` are all about the *write*
  path). Scope/fitness call: accept as #682's, or add an attributed status.

- The brief's falsifiability section is wrong about leg 6: it declares
  `a_fault_that_is_not_one_objects_map_still_ends_the_pass` *"NOT base-red — it passes before and
  after"* (`brief.md:118-119`), and `C4-verify` reports six red tests, not five. Measured on base,
  that leg fails with `reconstruction absorbed: ... key must be a string at line 1 column 2` —
  the base's walk dies on the seeded undecodable record (`segmented_map_passes.rs:761`) long
  before the injected `get` fault is reached, so it is red for the wrong reason. This does not
  weaken the evidence (the leg's post-fix assertion still binds "a store fault is not swallowed"),
  but the brief's prediction and the gate row disagree and the reviewer had no way to see it.

- `crates/custodian/src/restore.rs:616` still reads `deferred: #681 — ... The maintenance walk that
  both would share is that slice's`; this slice deliberately does **not** share the walk
  (`brief.md:298-299`), so the marker points at a closed issue once this lands. The brief forbids
  touching the file and a fifth file trips the STOP, so this is noted, not filed.

## What I tried to refute and could not

- **The red→green itself.** Reproduced both legs; the red is behavioural, not a compile failure,
  and the test drives the real fenced control point (`reconcile_step`) and `backfill::reconcile`,
  not a parallel re-implementation.
- **Decision 4 (CAS framed by the generation actually read).** I looked for a commit path reachable
  through a *restarted* resolve. There is none: `resolve_snapshot`
  (`crates/core/src/metadata.rs:2584-2586`) answers a flat map by borrow with no store read and no
  supersede check, so only a **segmented** snapshot can restart — and all three passes branch on
  `record.chunk_map.is_segmented()` (the snapshot, `backfill.rs:160`, `rebalance.rs:260`,
  `reconstruction.rs:816`) before any write. The brief's recorded-rejection of a Tier-0 DST leg
  holds up on that reasoning.
- **The `as_flat()` "unreachable" fallbacks** (`rebalance.rs:367-372`, `reconstruction.rs:640-645`)
  really are unreachable: `ChunkMap` has exactly two variants (`crates/core/src/metadata.rs:986-993`),
  so `!is_segmented()` implies `as_flat().is_some()` on the same bytes.
- **"Only one obligation per object is repaired per pass"** (second plan loses its CAS on the shared
  `prior_bytes`). Probed it: this is the base's behaviour too — assessment completes for every
  obligation before any repair commits, so both plans were always framed by one snapshot. Not a
  regression.
- **A silent drain through `Ok(None)`.** Probed a segmented root retired mid-resolve while it held
  the only reference to a queued chunk; the obligation was kept and the pass answered `Blocked`.
  `Ok(None)` from the resolver means "no live committed generation"
  (`crates/core/src/metadata.rs:2639-2646`), so skipping it silently is the same answer gc, scrub
  and restore already give.
- **The new `unparsable-inode-key` containment** (`backfill.rs:107`, `rebalance.rs:232`,
  `reconstruction.rs:783`). Probed a healthy repairable object under `inode:-1`: it is never
  repaired and the pass is `Blocked` forever. That looked like an introduced trap until I checked
  the base — where the same store had its obligation silently **drained** (`find_chunk`'s
  `if let Some(inode_id)` fell through to `Ok(None)` → `Assessment::Drain`). The patch's direction
  is fail-closed and matches `metadata::high_water_marks` (`crates/core/src/metadata.rs:2155-2170`).
- **Test-shape false-greens.** `assert_gauge`'s digit-run parse cannot be satisfied by a substring
  of `monotonic_counter.*_unaccounted_records` (the quote-delimited key does not match), it demands
  exactly one sample, and leg 2's `assert_eq!(refusals.len(), 1)` plus `"fragments":3` over a
  fixture with 3 draining fragments across 2 chunks really does discriminate per-object from
  per-chunk logging and fragment-counting from chunk-counting.

Advisory only — nothing here gates.
