# Result — issue 711 / repoint-chunk-segmented-placement-moves

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_repoint.rs` passes,
  driven **only** through symbols visible on the base (post-child-1, post-#696/#697) —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, ReconstructionContext, RebalanceContext,
  Reconciled}`, `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle,
  reconciliation_status, ReconciliationStatus}`, `wyrd_core::repair::{enqueue_repair,
  queued_repairs, repair_key}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Four legs:
  1. **A `seg:`-resident under-replicated chunk is repaired.** Seed a committed **segmented**
     object (raw `seg:` records + a segmented root, never a committer) whose chunk has lost a
     fragment, enqueue its repair, run `reconcile_step` with a `ReconstructionContext`. Assert: the
     rebuilt fragment is on a healthy D server in a distinct failure domain; the `seg:` record's
     `ChunkRef.placement` now names it; the repair obligation is **drained** (`queued_repairs` no
     longer contains it); the pass answers `Changed`; and the **root** record's bytes are unchanged
     except as the move itself requires. Base behaviour: refused, obligation still queued, `seg:`
     bytes unchanged → **red**.
  2. **A `seg:`-resident fragment is evacuated off a draining server.** Same fixture shape with
     `set_lifecycle(.., Draining)` on the server holding a fragment; run `reconcile_step` with a
     `RebalanceContext`. Assert the fragment is copied to a non-draining server in a distinct
     domain, the `seg:` record names it, the vacated position is orphan-marked, and the pass answers
     `Changed`. Base: refused, placement unchanged → **red**.
  3. **The ceiling refusal holds over a segmented record.** A `seg:` record seeded just under
     `MAX_VALUE_BYTES` whose repoint would cross it: refused, record byte-identical, obligation
     queued, pass non-certifying. (This leg is **not** independently red — pre-fix the move is
     refused for the *other* reason, and child-1 already established the rule. It ships because it
     pins the rule for the segmented arm, which child-1 cannot; **do not count it as discriminating
     evidence**.)
  4. **Two committed references to the same `ChunkId` get one plan, not independent ones.** Seed
     two committed objects whose maps both name the same `ChunkId`, with a repair queued for it.
     Assert the pass does not repoint or overwrite the same `FragmentId`s twice and does not orphan
     copies the other object still references — neither object is left naming a fragment that was
     reclaimed.
  Legs (1), (2) and (4) are binding. **Additionally**, the DST **repoint-versus-supersede** property
  ships in the **existing** `crates/dst/tests/custodian.rs` (a new `crates/dst/tests/*.rs` would put
  `#![cfg(madsim)]` on the C4-verify invocation and change what the gate compiles): a repoint whose
  pinned root generation **or** segment bytes changed under it commits **nothing** — neither the
  placement nor any orphan mark — and the object is left naming a fragment that exists. Assert it
  across the seed sweep, in **both** interleavings (repoint wins before the supersede's inode CAS;
  repoint loses after it). C4-ci runs it; it is **not** the C4-verify discriminator.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly.)
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: Fixed
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 36 mutants tested in 83s: 17 missed, 6 caught, 13 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_711/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: enable atomic repair and drain placement moves for chunks whose `ChunkRef` lives in a segmented-map `seg:` record.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract identifies the permanent maintenance failure, exact-byte CAS semantics, atomic evidence, boundedness, and binding repair/drain/race cases; the target primitive's stated responsibility matches that context (`crates/core/src/metadata.rs:2764`). |
| C2 Reproduction (red pre-fix) | PASS | The base compiled and ran all four discriminator tests: repair, evacuation, and shared-reference legs failed by assertion while the declared non-discriminating ceiling leg passed (`crates/custodian/tests/segmented_map_repoint.rs:341`, `crates/custodian/tests/segmented_map_repoint.rs:402`, `crates/custodian/tests/segmented_map_repoint.rs:536`, `crates/custodian/tests/segmented_map_repoint.rs:595`). |
| C3 Change | NEEDS-HUMAN | Plan must choose which optional test edit to drop or split — the patch touches seven files and edits both optional custodian test files despite the hard six-file/one-optional-file scope (`crates/custodian/tests/segmented_map_rebalance.rs:5`, `crates/custodian/tests/segmented_map_reconstruction.rs:6`). |
| C4 Verification (red→green) | PASS | The same four-test discriminator turned green, and an isolated full `cargo xtask ci` passed with real typos/docs/deny/machete tools plus the 50-seed madsim race property (`crates/custodian/tests/segmented_map_repoint.rs:340`, `crates/dst/tests/custodian.rs:2524`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must carry the segment bytes returned by resolve (or an equivalent snapshot) — the primitive instead re-reads and CASes later bytes, and a driven sibling-record rewrite was merged and its repair obligation drained rather than conflicted (`crates/core/src/metadata.rs:2866`, `crates/core/src/metadata.rs:2891`). |
| T1 Structure | FAIL | The structure exceeds the brief's hard cap with seven touched files and both mutually exclusive optional test files, so the change must be reduced or split before it has the required review shape (`crates/custodian/tests/segmented_map_rebalance.rs:5`, `crates/custodian/tests/segmented_map_reconstruction.rs:6`, `crates/custodian/tests/segmented_map_repoint.rs:1`). |
| T2 Shape | FAIL | The API cannot express the promised exact segment snapshot because it accepts only the root generation, offset, prior reference, and new placement; callers therefore cannot condition on the segment bytes they resolved (`crates/core/src/metadata.rs:2805`). |
| T3 Runtime | FAIL | A driven two-object drain issued two puts for the same `FragmentId`: planning emits one move per reference and each independently publishes an orphan mark, creating a crash window where another committed object still names the marked source (`crates/custodian/src/rebalance.rs:342`, `crates/custodian/src/rebalance.rs:390`, `crates/custodian/src/rebalance.rs:558`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the unavailable batch-review report adds blockers — `scripts/review-branch --bundle` and its five-finding log were not supplied, although contribcheck and affected-path checks across merged history plus every closed-unmerged PR independently completed. |
| T5 Judgment | NEEDS-HUMAN [impl] | The exact mutation rerun reproduced 17 missed mutants; rebuild must add tests for a segment rewrite before primitive preparation and duplicate-reference evacuation because the DST double races only when commit begins and the supplied duplicate test drives reconstruction only (`crates/dst/tests/custodian.rs:2207`, `crates/custodian/tests/segmented_map_repoint.rs:595`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | After rebuild, a human must confirm operator-level durability and decommission fitness — automated green paths cannot sign off exact-snapshot concurrency or crash-safe shared-chunk evacuation behavior. |

### Advisory — adversary

# Adversarial review — issue #711 (`repoint-chunk-segmented-placement-moves`)

Advisory only; nothing here gates. Every citation is against the target working tree at
`$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Red→green was re-run from a throwaway
clone of the base commit `92e1b4b`, not from the bundle's own logs.

## Findings

- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:2920`: the `chunk == prior` equality —
  the primitive's *only* guard for the read→prepare window, and the one its own doc calls "the
  pin that turns a map rewritten under the plan into a `Repoint::Conflict` instead of a silent
  overwrite of somebody else's newer placement" (`:2909-2912`) — is exercised by **nothing** in
  this bundle. I deleted it outright (`chunk_at` matching by byte offset alone, `prior` unused)
  and re-ran `cargo test -p wyrd-custodian -p wyrd-core` **and** the full 15-property DST
  campaign at the gate's own `MADSIM_TEST_NUM=50`: **all green**, including
  `segmented_map_repoint.rs`, `placement_ceiling.rs`, and
  `repoint_versus_supersede_commits_all_or_nothing`.
  Concrete failing case the pin is the sole defence against: a repair resolves segmented object
  `O`, chunk `C` at offset 0 with `placement [0,1]`, fragment 1 lost, and plans `new_placement
  [0,2]`. Before `repoint_chunk`'s own re-read at `:2866`, another writer moves fragment 0 to
  server 5, leaving the `seg:` record at `[5,1]`. With the pin: `chunk_at` → `None` →
  `Repoint::Conflict`, nothing written. Without it: index 0 matches on offset alone, the CAS at
  `:2894` is pinned to the *freshly read* `[5,1]` bytes so it **succeeds**, and the record is
  rewritten to `[0,2]` — silently reverting the other writer and re-pointing fragment 0 at a
  server that no longer holds it. Add a leg that seeds a `seg:` record whose chunk at the plan's
  offset differs from `prior` and asserts `Repoint::Conflict` behaviour through the store
  (record byte-identical, obligation still queued, no orphan mark).
- **NEEDS-HUMAN [impl]** — `crates/dst/tests/custodian.rs:2207-2226` is *structurally* unable to
  reach that window, so the brief's claim that the DST leg proves "a repoint whose pinned root
  generation **or** segment bytes changed under it commits nothing" holds only for changes that
  land **after** the prepare. `RaceAtRepoint::commit` takes `self.pending` and applies the racing
  batch **inside the repoint's own `commit()` call** — i.e. strictly after
  `metadata::repoint_chunk` has already done its `store.get(&key)` at `metadata.rs:2866`. Both
  interleaving flags (`repoint_first`) only reorder *two commits*; neither mutates the record in
  the read→prepare gap. I confirmed the two CAS preconditions themselves **are** genuinely
  proven — deleting `.require(root_key, root_bytes)` (`:2893`) and deleting
  `.require(key.clone(), bytes)` (`:2894`) each turn the DST property red at 50 seeds — so the
  gap is exactly and only the reference pin.
- **NEEDS-HUMAN [impl]** — `crates/core/src/metadata.rs:2784` ("**What it pins — the exact bytes
  the resolve read**"), echoed at `crates/custodian/src/reconstruction.rs:877` and
  `crates/custodian/src/rebalance.rs:516` and in the brief's scope text, is inaccurate for the
  segmented arm: `:2866` performs a **second, later** `store.get` and `:2894` pins *those* bytes,
  not the ones `resolve_chunk_map` read. That is defensible (it merges a concurrent edit to a
  neighbouring chunk instead of losing its CAS), but it is a different design from the one the
  brief specified, and it is what shifts the whole weight of the window onto the untested
  `:2920` pin. Either pin the resolve's bytes as written, or correct the three doc sites and land
  the test above.
- **NEEDS-HUMAN [impl]** — C5's "17 missed" is not the noise a non-gating mutation row usually
  is, and `AGENTS.md:72-74` explicitly says to inspect survivors "when the change touches
  correctness logic". **All 17 sit inside code this patch introduced** (`repoint_chunk`,
  `covers`, `chunk_at` — `mutants.out/missed.txt`). Two caveats in the fix's favour, which I
  verified: (a) the run scoped every `crates/core` mutant to `--package=wyrd-core@0.0.0` alone
  (`mutants.out/log/crates__core__src__metadata.rs_line_2902_col_77.log`), so it never ran the
  custodian tests that actually drive the primitive; (b) re-running two of them (`covers` `<`→`<=`
  at `:2902`, `chunk_at` `>`→`>=` at `:2917`) against the full custodian suite **kills** both, via
  `segmented_map_reconstruction.rs`'s two-segment fixture. So 17 overstates the gap — but the
  residue is real: `repoint_chunk`/`covers`/`chunk_at` ship with **zero** `wyrd-core` unit tests,
  against a module whose own convention (child-1's boundary test at `:2978-2994`) is to pin
  exactly these rules in-crate.
- **NEEDS-HUMAN [human]** — the patch touches **7** files against the brief's hard "**≤ 6** files"
  budget, and the two test files it edits (`crates/custodian/tests/segmented_map_rebalance.rs`,
  `crates/custodian/tests/segmented_map_reconstruction.rs`) are **not** the pair the brief
  allowed ("at most one of `custodian/tests/{reconstruction,rebalance}.rs`"). The brief's own
  stop rule reads "A seventh file means the shape is wrong … STOP and hand back a proposed
  split"; Do proceeded instead. Mitigation a human should weigh: both edits are *forced* — leg 2
  of each file asserted the #696/#697 refusal that this slice removes, so C4-ci could not stay
  green without them — and none of the named drift files (`backfill.rs`, `restore.rs`, `gc.rs`,
  `desired_state.rs`) is touched. This is a ratify-or-split call, not a code defect.
- `check-gates.json:48` reports C4-verify as "4 test(s) ran red". Only **3** of the 4 legs are
  actually red on the base — `a_repoint_that_would_cross_the_ceiling_over_a_segment_record_is_refused`
  passes pre-fix, exactly as the brief predicted ("do not count it as discriminating evidence",
  brief.md:43-44). The gate line is a test *count*, not a failure count; do not read it as four
  discriminating legs. (Advisory note, no action.)

## Refutations attempted that did not land

I could not break the following, and each was a real attempt, not a skim:

- **The red→green itself.** Cloned base `92e1b4b`, added only the new test file: 3/4 fail
  (`Blocked` vs `Changed`), leg 3 passes. Applied `patch.diff`: 4/4 green, plus
  `segmented_map_{reconstruction,rebalance}.rs` and `placement_ceiling.rs` all green. The test
  drives the real `reconcile_step` over in-memory doubles and reads the store back — no parallel
  re-implementation, no mocked-away defect, and it names no symbol this patch introduces.
- **Serialization identity of the CAS precondition.** `require(root_key, encode(generation))`
  re-encodes a *decoded* record — the rubric's named "serialization identity" class. Refuted:
  `InodeRecord`'s `skip_serializing_if = "Option::is_none"` on `etag`/`content_type`/`modified`
  (`metadata.rs:1409`, `:1415`, `:1421`) makes decode→encode the identity on legacy bytes, and
  that comment says so for precisely this reason.
- **`SegmentRecord::new` re-deriving `byte_len` desynchronising the root table.** Refuted:
  `byte_len == sum(chunk.len)` is a decode invariant (`:1180-1188`) and the resolver rejects a
  record whose extent disagrees with the root's `SegmentRef` (`:2585`), so a placement-only
  rewrite cannot move the span.
- **The flat arm no longer forcing `state: InodeState::Committed`.** Refuted:
  `resolve_current_chunk_map:2694` returns `Ok(None)` for a non-`Committed` root and both callers
  filter the scanned record on `Committed` (`reconstruction.rs:481`, `rebalance.rs:290`), so
  `prior.state` is always `Committed` and a repoint can never publish or un-publish.
- **Multi-segment addressing (`covers` picking the wrong segment).** Refuted: the two-segment
  fixture in `segmented_map_reconstruction.rs` leg 2 kills the boundary mutants, as I verified.
- **`?` escaping from `seg_key` (`:2860`) or `SegmentRecord::new` (`:2883`) and ending a whole
  pass.** Refuted: both failure conditions are decode invariants of a record already in hand.
- **Orphan-marking a position a second object still references** (the brief's leg-4 hazard, and
  the case where `rebalance::plan_evacuations` emits independent plans per object for one
  `ChunkId` with no dedup, `rebalance.rs:342-399`). Refuted by GC itself: `gc.rs:146` /
  `:190-193` treat the committed reference set as a hard safety gate — a fragment any committed
  chunk map still names is never reclaimed regardless of its `orphan:` mark. A stale mark on a
  still-referenced position is inert, not a deletion. The leg-4 property therefore holds for the
  evacuation caller too, by a different mechanism than the repair caller's `sites` dedup.
- **Zero-length chunks, duplicate `ChunkRef`s at one offset, and `checked_add` overflow in
  `chunk_at`.** Walked each by hand; the offset-plus-equality rule disambiguates correctly and
  overflow degrades to `Conflict`.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Plan must choose which optional test edit to drop or split — the patch touches seven files and edits both optional custodian test files despite the hard six-file/one-optional-file scope (`crates/custodian/tests/segmented_map_rebalance.rs:5`, `crates/custodian/tests/segmented_map_reconstruction.rs:6`).
- [ ] C5 Causal adequacy — Rebuild must carry the segment bytes returned by resolve (or an equivalent snapshot) — the primitive instead re-reads and CASes later bytes, and a driven sibling-record rewrite was merged and its repair obligation drained rather than conflicted (`crates/core/src/metadata.rs:2866`, `crates/core/src/metadata.rs:2891`).
- [ ] T4 Contribution — Decide whether the unavailable batch-review report adds blockers — `scripts/review-branch --bundle` and its five-finding log were not supplied, although contribcheck and affected-path checks across merged history plus every closed-unmerged PR independently completed.
- [ ] T5 Judgment — The exact mutation rerun reproduced 17 missed mutants; rebuild must add tests for a segment rewrite before primitive preparation and duplicate-reference evacuation because the DST double races only when commit begins and the supplied duplicate test drives reconstruction only (`crates/dst/tests/custodian.rs:2207`, `crates/custodian/tests/segmented_map_repoint.rs:595`).
- [ ] Validation — fitness-to-purpose — After rebuild, a human must confirm operator-level durability and decommission fitness — automated green paths cannot sign off exact-snapshot concurrency or crash-safe shared-chunk evacuation behavior.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_711/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 124 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice is oversized for one Do attempt: 7 files against the brief's hard 6-file budget, both mutually-exclusive optional test files edited instead of one, and 124 KB against the 100 KB size backstop (which itself recommended iterate-plan). Two build attempts already converged on this same oversized shape rather than a smaller one, so a further iterate-do would likely repeat it. Additionally, the adversary review found a real, unaddressed gap — the chunk == prior race guard (the only defence against a read/prepare race during repair) has zero test coverage; deleting it left the whole suite green. Return to Plan to author a split (pdca split 711) that separates the repoint primitive from its two callers / the DST property, and have the split brief require a leg that proves the chunk == prior guard.
- By / date: Eduard Ralph / 2026-08-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
