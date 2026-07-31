# Oversized slices in the multipart stack — why the harness produced >500 KB patches, and what a size-aware loop must do differently

**Instance:** Wyrd PDCA (instance of eduralph/pdca-harness, pre-0.56)
**Runs analyzed:** issues #508 (pre-split attempts), #634, #635, #636, #637, #638 — 2026-07-25 → 2026-07-31
**Evidence:** `results/issue_{508,634,635,636,637,638}/` in this repo (patch archives, per-iteration gate records, sign-off SUMMARYs, review/adversary artifacts). Every number below is measured from those artifacts; "added lines" always means `git apply --numstat` additions on the archived `patch.diff`.

---

## 1. Summary

The #508 multipart monolith (44 files / 14,117 lines) was rejected at sign-off on reviewability and re-planned into five "independently reviewable" slices. **The split did not produce small patches.** Four of the five slices have now run, and they produced 281 KB–1,074 KB patches taking 4–18 builder rounds each; the largest slice (#635, 19,228 added lines) ended up *larger* than the monolith whose size caused the split, grew until it physically exceeded the review tool's input ceiling, and its PR (#647) was closed unmerged. #636 was discontinued at sign-off with the instruction to split again.

The failure has three independent components, and all three need guardrails:

1. **Nothing in the Plan pass bounds a slice by diff size.** Slices were cut along *design decisions* (proposal 0016's decisions 1–7), but a design decision's blast radius is not a diff budget: "one record shape" (#635) touches every consumer of that record across two crates plus a conformance burden; "one trait method" (#634) touches 36 in-test trait impls plus four backends plus a conformance suite.
2. **Nothing in the Do/Check loop pushes back on size.** Patches grew monotonically every round in every run — fixes only ever add code — and review-finding counts on large diffs oscillate rather than converge, so the auto-iterate loop keeps firing without approaching zero. The loop has no signal that says "stop iterating, re-slice."
3. **Stacked-slice base wiring has no first-class support.** The moment the work *was* split into dependent slices, the first dependent slice (#635) lost its first five rounds to a declared base branch that did not exist in the leaf sandbox — a failure class that gets more frequent, not less, as slices get smaller and stack deeper.

Historical data from this instance's ~60 completed bundles shows a sharp knee: **below ~100 KB of patch, bundles converge in a median of 2–3 rounds; above ~250 KB, the median is 8.5 rounds and nothing converged in under 6.** A size gate at roughly the knee, plus a convergence gate, would have caught every size failure in this batch at round 1–2 instead of round 4–18.

---

## 2. The runs at a glance

| Issue | Slice content | Build rounds | Final patch | Files | Added lines | Test share | Brief size | Outcome |
|---|---|---|---|---|---|---|---|---|
| #508 (pre-split) | all of multipart | 6¹ | 731 KB | 44 | 14,117 | — | 63 KB (v7) | rejected at sign-off (T1 Structure) |
| #634 | `scan_page` trait seam | 6 | 313 KB | 45 | 5,726 | ~79% scaffolding² | 42 KB | merged (PR #645) |
| #635 | segmented chunk map | **18** | **1,074 KB** | 50 | 19,228 | ~64% tests | **81 KB** | PR #647 **closed unmerged** |
| #636 | multipart commit protocol | 4 | 664 KB | 18 | 13,987 | ~49% tests | 46 KB | **discontinued** at sign-off |
| #637 | staged-byte protection | 0 (brief only) | — | — | — | — | 41 KB | not started |
| #638 | fragment write deadline | 9 | 281 KB | 54 | 4,176 | ~73% tests³ | 30 KB | accepted; not yet published |

¹ Six built attempts across seven numbered rounds — attempt 3 was blocked at plan review and never built.
² 634: 52.9% is test files, plus the conformance clause library (+1,197) and testkit double (+310); actual backend I/O code is ~592 lines of the 5,726.
³ 638: additionally, 38 of the 54 files are pure mechanical migration (64 added `None` arguments and nothing else) forced by the trait-seam signature change.

**Iteration trajectories were monotone in size, non-convergent in findings:**

- Patch growth, added lines per built round — 508: 2,706 → 3,730 → 7,718 → 10,683 → 12,511 → 14,117; 634: 2,402 → 5,726 over 6 rounds; 635: 3,408 → 19,228 across 17 archived snapshots (per-round growth between +480 and +2,305, never negative); 636: 7,711 → 13,987 in 4 rounds (deletions frozen after round 2 — rounds 3–4 were pure addition); 638: 1,359 → 4,176. **No run ever produced a smaller patch than the previous round**, even when the fix itself was a deletion (638 iteration 7 deleted 19 production lines; the patch still grew).
- Blocking-finding counts per round — 508: 58 → 33 → 37 → 32; 635: 5, 2, 5, 5, 6, 5, 3, 7, 4, 1, 4, 4, 10, 4, —, 6, 3 (74 implementation findings over 16 counted rounds; reached 1 at round 9, then bounced back to 10); 636: 51 → 38 → 21 → **31** (round 4 fixed 19 of 21 findings and the count went *up*); 638: 5 → 7 → 9 → 7 → 8 → 9 → 5. **No large bundle ever trended to zero.**

## 3. Baseline: patch size predicts convergence

Across every bundle in this instance with a bundle-root `patch.diff` (an accepted final patch) and a recorded `iterations_to_pass` — an inclusion criterion that excludes #508's rejected monolith (no final accepted patch, no pass; counting its 731 KB / six unconverged attempts would only worsen the ≥250 KB row):

| Patch size | n | Median rounds | Max rounds | Bundles needing >4 rounds |
|---|---|---|---|---|
| < 50 KB | 34 | 2 | 11 | 2 / 34 (6%) |
| 50–100 KB | 11 | 3 | 9 | 4 / 11 |
| 100–250 KB | 12 | 4.5 | 12 | 6 / 12 |
| ≥ 250 KB | 6 | 8.5 | 18 | 5 / 6 |

The one ≥250 KB bundle that stayed under 5 rounds (#636, 4 rounds) did so only because the human discontinued it rather than iterate again — counting it as a completion is conservative: excluding it raises the ≥250 KB median to 9. The binning must also survive a direction-of-causation objection — patches grow with rounds, so a slow bundle ends bigger — and it does: the studied bundles were already past the knee at *first handoff*, before any review round ran (round-1 patches: 508 124 KB, 638 118 KB, 634 135 KB, 635 239 KB, 636 353 KB — every one ≥ 100 KB), so handoff size, not accumulated iteration growth, is what put them in the degraded bins. The relationship is not subtle: **the harness's Do/Check loop works as designed up to roughly 100 KB / ~2,000 added lines and degrades rapidly past 250 KB.**

Why size breaks the loop specifically (mechanisms observed in the artifacts, not conjecture):

- **Review sampling, not review coverage.** On an 18k-line diff, each batched-review round surfaces a *different* subset of defects. 635 round 12 produced 10 findings after round 9 produced 1. 636's final 31-row batch had zero cross-pass agreement (every row "seen by 1 pass") — three union'd passes over a diff that big are three independent samples, and the union triples the raw count the gate blocks on. This instance's 2026-07-21 Act review had already measured the underlying reviewer property at ordinary PR sizes ("serialized depth": ~1 new real finding per re-review, 13 rounds on one small PR), so oscillation alone is not a size effect — what size adds is that each pass covers a *smaller fraction* of the diff, which is why the knee table above, not the oscillation, carries the causal claim.
- **Each fix spawns the next class.** 635's containment theme ran five rounds: round 14's three fixes were real fixes, but the containment they added "simply sits on the wrong side of the state check" — a *new* five-site defect class in round 15. On a large surface, fixes are themselves large enough to carry fresh defects.
- **Mutation testing stops working exactly when it's needed.** C5 (`cargo-mutants --in-diff`) runtime scaled with the diff: 39 min → 58 min across 508's attempts, then hard 7,200 s timeouts (`unverifiable`) in 3 of 636's 4 rounds and 635's final round — leaving "558 in-diff candidates whose survivor status remains unknown" on the round that shipped. 635's missed-mutant counts never converged either: 25 in round 1, briefly 0 in rounds 4–8, then stuck in an 11–14 band from round 9 to the end as code was added faster than it was covered.
- **The review gate itself has an input ceiling.** 635's final T4 failed with `input_too_large` on all 3 passes + retries: the bundle's prompt was 1,077,300 chars against codex's 1,048,576 hard limit. **The patch grew until the reviewer could no longer read it, and that terminal red was accepted at sign-off, not fixed.**
- **Line-number drift re-opens settled decisions.** The batch gate matches findings by `file:line`. On a growing patch, every settled rejection drifts: 635's `review-rejected.md` reached **85 rows**, with sections titled "the standing decisions re-pinned at their new lines" appearing in rounds 7, 8, 9, and again "at their FINAL lines"; 638's builder recorded the same rejection at three separate line numbers purely so drift couldn't dodge the triage. That is a builder writing defensive glue against its own harness.
- **Stacked-base wiring cost five full rounds before size ever mattered.** 635's brief declared a base branch (`origin/pdca-integration/main`, carrying #634) that did not exist in the leaf sandbox; Do verified against plain `origin/main` and every gate went red on a missing trait method for five straight rounds, until the round-5 human verdict named it ("Plan needs to fix the base setup/dependency wiring … rather than Do re-guessing at a moving target") and #634's merge to `main` mooted it. So of 635's 18 rounds, 5 went to a dependency-wiring failure no gate checks for and two more to pure infrastructure (the next bullet's quota and engine-interrupt rounds) — leaving ~11 attributable to size. #638 likewise had to serialize behind #634's human merge because of a declared file conflict.
- **Whole rounds burned on infrastructure, amplified by size.** 635 lost round 10 to reviewer quota, round 15 to an engine interrupt bug (wyrd-pdca#187 / upstream#369 — the round has *no* review rows at all) plus a 19h16m C5 hang on a pre-existing deadlock; 634 spent 2 of its 6 rounds on a single re-surfacing item — the clearing leaf could not see `scripts/review-branch`, so a green bundle iterated twice over gate *reproducibility*. The same "cannot be independently reproduced" caveat appears in 3 of 636's 4 rounds and in every round of 638.
- **Non-convergence is maximally priced by configuration.** The builder escalation ladder (`min_iteration = 2` → top model/effort tier) means every round from the second onward runs at maximum builder cost — 635 paid ~17 top-tier rounds, each also paying 3 review passes over an up-to-1 MB diff and a 39 min–2 h (once 19 h) mutation run. The config comment says "spinning is expensive"; on a non-converging large bundle, spinning is the *steady state*. (Per-round durations are currently derivable only from artifact mtimes; `loop-telemetry.json` records no timing — a telemetry gap worth closing upstream.)

## 4. Where the bytes came from

Four distinct inflation mechanisms, each attributable in the diffs:

**(a) Trait-seam ripple.** A required method on a widely-implemented trait multiplies mechanically. 634's `scan_page`: 55 added implementations, 34 of them byte-identical 12-line delegations to a testkit helper in unrelated test crates. 638's `put_fragment` signature change: 38 of 54 files are one-line `None` callsite migration. This ripple is *cheap to review* (it is homogeneous) but it lands in the same patch as the semantic change and inflates every size metric and every review pass's input.

**(b) Acceptance-criteria compounding.** Every slice was required to carry, in one patch: demonstrated-red conformance clauses on every backend (634: four stores × seven clauses, a 1,636-line "violating double" suite, a 1,197-line clause library), end-to-end custodian observables (635: eight consumers × a per-consumer failure-containment table, each row needing its own production branch *and* test), seeded DST cases, docs-currency edits, and named-constant derivations in doc comments (636's `multipart.rs` is ~33% prose for this reason). Result: test/scaffolding share of 49–79% everywhere. The criteria are individually well-motivated — several encode real prior failures — but their *sum per slice* guarantees a multi-thousand-line patch even when the production delta is a few hundred lines (634's actual backend code: ~592 lines inside a 5,726-line patch).

**(c) Speculative apparatus with no caller.** 635 shipped a staged-publication committer "no real `Completing` session drives it yet" and a several-hundred-line chunk-id-floor recovery apparatus whose only non-test caller immediately discards the value it computes — the final adversary review: "several hundred lines of production code, and a large share of the surviving-C5 mutant surface, computing a number nobody reads." 636 shipped a drain whose functions "have callers only in tests" — which is not just dead weight; it produced the run-killing regression (deletes routed to obligations nobody drains → **the live DELETE path stopped reclaiming space**). Building a mechanism one wave before its consumer means the slice must also build a synthetic world to exercise it, and real integration defects hide until the consumer lands.

**(d) Monolithic production files.** 636's `crates/core/src/multipart.rs`: 6,317 added lines (2,013 of them comment prose), ~165 top-level items, roughly six subsystems (knob derivations, typed outcomes, record grammar, verbs, drain, classification oracle) in one file. 635's `crates/core/src/metadata.rs`: +12,331 lines (~5,000 production + ~7,300 inline tests). These are single review units by construction.

## 5. Root cause, stated once

**The unit of work was chosen by design coherence ("one 0016 decision per slice") and validated by narrative review ("is this one logical change?"), but never by the one metric the whole pipeline degrades on: diff size.** No stage — Plan, Do, Check, sign-off — measures or bounds it. The only size signals in the entire system were terminal: a human rejecting at sign-off (508 v7, 636), and a reviewer tool refusing input (635). Both fire after the cost is sunk; 635 burned ~3 days of wall clock and 18 builder rounds (~11 of them size-attributable) before its terminal signal.

The instance's own history proves the loop is healthy when slices are small (§3). The guardrails therefore don't need to fix review, mutation testing, or the builder — they need to keep bundles inside the envelope where those already work, and make the stacked-slice topology that smallness requires a first-class citizen.

## 6. Guardrail proposals (for pdca-harness 0.56)

Ordered by how much of this batch each would have prevented:

1. **A gating size budget on `patch.diff`.** Configurable per instance (`[driver] max_patch_lines / max_patch_files`, per-bundle override in the brief for declared-mechanical cases). Measured at Do handoff, before any review leaf runs. Breach ⇒ the bundle's only legal verdicts are *iterate-to-Plan* (re-slice) or an explicit human waiver recorded in §9 — never another Do round. From this instance's data the defensible default is ~2,000 added lines / ~20 files (the knee of §3), with mechanically-migrated lines counted separately (see 8). This alone catches 508 v4 (7,718 lines) three rounds early, 635 at round 1, 636 at round 1.
2. **First-class stacked bases.** Small slices stack; today the stack is a prose convention. The driver should (a) validate at Do handoff that the brief's declared base exists and is reachable in the leaf sandbox — 635's rounds 1–5 die here, at round 1; (b) give a dependent slice a reproducible base ref (the dependency's accepted `patch.diff` folded onto the shared base, which `wave_mode = "stack"` already computes) so Check can verify against the *normative* stack rather than whatever `main` holds; (c) surface "waiting on dependency merge" as a bundle state instead of a Do-time surprise.
3. **A convergence gate on the iterate loop.** Auto-iterate currently fires while the finding count is "not increasing" between adjacent rounds, and its budget resets across flow invocations — 635 legally consumed 5 automatic rounds and then 13 human-driven ones. Add: (a) a *cross-invocation* round ceiling per bundle; (b) a trend test over the last N rounds (distinct blocking findings not strictly decreasing over 3 rounds ⇒ iterate-to-Plan, not Do); (c) patch-growth coupling (a round that grows the patch by >15% while findings don't drop is evidence of non-convergence, not progress).
4. **Deduplicate findings before the gate counts them.** The union of 3 review passes is inflated by duplicates (508 v7: 32 rows ≈ 17 distinct; 636 r2: 38 ≈ 21 distinct; several defects reported 3× at adjacent lines). Cluster by normalized content (file + symbol + claim-hash), not raw rows; block on distinct defects.
5. **Content-stable finding identity for triage.** Match `review-rejected.md` rows by content signature, not `file:line` — line drift on a growing patch re-flagged settled decisions every round (635's 85-row rejection ledger; 638's triple-recorded rejection). A rejection, once recorded, must bind wherever the finding re-lands.
6. **A builder SPLIT verdict.** Do can currently only build or fail. Give the builder a first-class "this brief exceeds one reviewable slice" return (with a proposed seam list) that routes to Plan. 636's builder *knew* — its build-notes flag the size; 635's brief itself names the hazard ("a slice whose diff makes the resolver hard to find among the churn is the reviewability failure this whole re-plan exists to avoid") — but the only legal move was to keep building.
7. **Brief compaction on iterate.** Carry-forward is append-only; briefs grew to 81 KB (635) / 63 KB (508 v7) and are the Do leaf's *only* permitted input. At each iterate the driver should fold resolved carry-forward into the body and keep a bounded "standing decisions" digest; a brief-size lint (warn ~20 KB, block ~40 KB) doubles as a Plan-time slice-size proxy — every brief in this batch that exceeded 40 KB produced a >250 KB patch.
8. **Classify mechanical migration separately.** Let the brief declare migration globs/patterns (e.g. "callsites gaining a trailing `None`"); the size gate reports semantic vs mechanical lines and budgets bind on semantic lines. This keeps guardrail 1 from punishing honest seam ripple (638's 38 migration files were never the problem) and keeps seam changes honest about what needs review.
9. **Fail fast on reviewer input ceilings.** The driver knows the batch reviewer's input limit; a diff that cannot fit should short-circuit to the size-gate verdict at handoff, not run 3 passes + retries into `input_too_large` at round 17 and leave the terminal gate permanently un-runnable.
10. **The clearing leaf must be able to run the gate it is asked to clear.** `scripts/review-branch` was absent from the reviewer/adversary's permitted targets in nearly every round of three bundles (634: all 6; 636: 3 of 4; 638: all 9); the resulting "cannot be independently reproduced" caveat consumed 2 of 634's 6 rounds outright and made most 636/638 T4 verdicts provisional. Whatever tool a gating row runs must be in the allowed target set of the leaf that adjudicates it.
11. **Protect evidence artifacts, and record round timing.** 636's load-bearing negative-test evidence (25 negation runs) lived only in `build-notes.md` — withheld from the reviewer, read by no gate row — and its scratch directory was deleted, leaving builder prose as the only record. Gate-cited evidence should be a declared artifact the reviewer receives and the driver retains. Relatedly, `loop-telemetry.json` should record per-round wall-clock and builder tier, so the cost of non-convergence (which the escalation ladder maximizes — §3) is measurable instead of anecdotal.

Two upstream bugs from this batch, already known but worth restating with this data: the engine interrupt that leaves a bundle CHECKED with no review rows (wyrd-pdca#187 / upstream#369, cost 635 a full round), and C5's fixed 7,200 s timeout, which converts exactly the large-diff case into `unverifiable` — with guardrail 1 in place the timeout is fine; without it, it silently removes coverage adequacy from the biggest patches.

## 7. What this instance is doing manually until 0.56

Recorded here so the upstream guardrails can be checked against the manual practice they replace:

- Plan-time slice budget: briefs state a target of **≤ ~1,500 added semantic lines / ≤ 15 files, one concern per slice** (prefer one crate plus its tests; cross-crate only when the files are the concern's direct surface); the planner splits anything over budget before Do runs.
- A consumer-table rule of thumb: if the brief must enumerate N consumer sites for a change, the slice count is ~N/3, not 1.
- Scaffolding lands separately: a conformance suite + violating-double red demonstration is its own slice; backends adopt it in follow-ups.
- Caller-first sequencing: no slice lands a producer (obligations, records, committers) whose consumer doesn't exist yet; behavior flips (e.g. routing deletes through `retire:`) ship in the slice that lands the consumer.
- Base discipline: every brief names its base ref; Do refuses to build if the base is absent from the sandbox rather than guessing (the 635 lesson).
- Sign-off treats an over-budget patch as *iterate-to-Plan by default* — review rounds are not spent polishing a patch that is already too big to review (the 508 lesson: rounds 5–7 fixed dozens of findings on a patch whose size verdict was already known at round 4).

The concrete re-slicing of the affected Wyrd issues is in `docs/2026-07-31-alpha-reslicing-proposal.md` (instance-specific, not part of this report).
