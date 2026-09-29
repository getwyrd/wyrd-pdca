# Result — issue 776 / seg-record-placement-move-primitive

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **No maintenance write path in the tree can address a `seg:` record.** The
  only placement writer rebuilds an *inode* record: `repair_chunk`
  (`crates/custodian/src/reconstruction.rs:829`) takes `object.prior.chunk_map.as_flat()`
  at `:894`, aborts on `None`, and CASes `inode:` at `:937-953`. A
  `seg:<nonce>:<epoch>:<index>` record cannot be written by any repair-shaped code. This
  child ships the missing `wyrd_core` primitive; it changes **no** custodian behaviour —
  the pass keeps refusing until #777 wires it in.
- Success criterion: `crates/core/src/metadata.rs` exports a placement-move primitive:
  given a resolved generation, the byte offset of the chunk within the object, the
  `ChunkRef` the caller planned from, and the new placement, it **hands back** (never
  commits) the compare-and-swap batch that lands the move in whichever record holds that
  `ChunkRef` — flat inode **or** segment record — so the caller can add its own evidence
  (obligation delete, orphan marks) and land everything in ONE mutation (`0005:277`,
  ADR-0015). In-crate `#[cfg(test)]` unit tests (module convention `metadata.rs:2776-2780`)
  pin, by building the batch and applying it against a hand-mutated in-memory
  `MetadataStore`:
  1. **Flat arm:** the batch changes `chunk_map` **and** increments `version` via the
     `commit_chunk_map` idiom (`version = prior.version + 1`, `..prior.clone()`,
     `metadata.rs:1769-1797` — ADR-0047 metadata preserved). This kills iteration-1's four
     surviving mutants (`metadata.rs:2859`).
  2. **Segmented arm:** only the covering segment record is rewritten — found via the
     root's own table (`SegmentedMap::new`, `metadata.rs:870`), no `seg:` range walk, no
     other segment decoded — and the **root's bytes are unchanged**.
  3. **The three pins, exactly** (settled at the parent's Plan, do not re-derive): the
     **root generation's** bytes; the **segment record's own freshly-read bytes**; and the
     **`ChunkRef` itself** — the chunk moved is the one that begins at the given offset
     **and equals** the planned reference. A concurrent edit to a *sibling* chunk in the
     same record is **MERGED** (batch still applies); an edit to the planned chunk itself
     is a **CONFLICT** (batch fails cleanly, writes nothing). `ResolvedChunkMap`
     (`metadata.rs:2294-2300`) carries only `record` + flattened `chunks`, so "pin the
     exact bytes the resolve read" is not implementable for the segmented arm and must not
     be claimed — the archived attempt's three doc sites saying otherwise are a known
     defect to correct, not a spec to follow.
  4. **Superseded root:** a root flipped to a different generation makes the batch fail
     cleanly; nothing is written.
  5. **Ceiling:** the re-encoded record is weighed **before** anything is written — **both**
     arms through #710's `flat_value_ceiling_crossed` (`metadata.rs:380`), i.e. the **full
     `MAX_VALUE_BYTES`** (`100_000`, `metadata.rs:327`). **DECIDED BY THE HUMAN 2026-08-19 at
     the parent's plan-review revision: this REVERSES the V/2 (`MAX_ROOT_VALUE_BYTES`) bound
     this brief carried before. Do not re-derive it and do not restore V/2.**
     Why it was reversed: the resolver *reads* every `seg:` row up to the full ceiling
     (`metadata.rs:2493`), so a row in `50_001..100_000` is live, readable data. Refusing to
     repoint it would leave its repair obligation refused every pass, **forever** — precisely
     the C-1 defect this lineage exists to remove, reintroduced through the back door. The V/2
     warrant that stood here is a **knob-sizing** rule (`0016:1466-1467` bounds `MAX_SEG_CHUNKS`
     so a *conforming publication* stays under V/2), and #710 already settled how such a rule
     composes with a maintenance write: `MAX_MAP_CHUNKS` is V/2-sized too and the flat guard
     still weighs V. The resolver comment at `metadata.rs:2488` does not say otherwise — it is
     attached to the `value.len() > MAX_VALUE_BYTES` test and explains why a row *above* V is
     non-conforming; it makes V/2 an enforced boundary nowhere. `MAX_ROOT_VALUE_BYTES` bounds a
     segmented **root's** write (`metadata.rs:371-376`) and this primitive never re-encodes the
     root, so **no `MAX_ROOT_VALUE_BYTES` comparison may appear in this diff**. The two bounds
     differ only for rows a non-conforming writer already stored (a placement-only move grows a
     record by bytes, never from ≤V/2 to V) — and for exactly those rows V/2 means "unrepairable
     forever" where V means "repaired". A differently-*named* helper for the segment arm is
     fine; a second ceiling *value* is not. Unit test seeds a record just under
     `MAX_VALUE_BYTES` whose repoint would cross it and asserts refusal, record byte-identical
     — build that record programmatically (a loop of `ChunkRef`s), never as a byte literal, or
     it eats the 50 KB `patch.diff` budget on its own.
  6. **Checked version advancement** (no wrap, no panic) and **malformed segment decode
     surfaces structural corruption** rather than hiding a persistently damaged record —
     iteration-2's findings (`metadata.rs:2844`, `:2883`).
  7. **Zero-length chunk at a segment boundary** (iteration-4 carry-forward,
     `metadata.rs:2869-2874`, `:2922`): the offset lookup must not match the wrong record
     ahead of the equality check — equality governs; a mismatch is a conflict/`Blocked`,
     never a silent wrong-record write.
  8. Direct unit tests for the two addressing helpers (offset-plus-equality lookup,
     segment coverage) — the parent's C5 residue was 17 missed mutants, all here.
  **Verification posture — declared so sign-off is not surprised:** this child is
  green-only under C4-verify, **deliberately**. Its defect is an *absent* API, so no test
  can go red without naming the new symbols, and a NEW `*/tests/*.rs` naming them would
  make the gate's revert leg fail to **compile** (UNVERIFIABLE, exit 77,
  `run-verify.sh:469-476`, `:492-500`). Therefore: **no added test file**; all tests are
  in-crate `#[cfg(test)]`, covered by C4-ci; the binding oracle is **`cargo mutants` over
  the diff** — expect zero missed mutants in the new code, and `build-notes.md` records
  the named negations (delete the `chunk == prior` equality → test 3's conflict leg goes
  red; delete the root-generation pin → test 4 goes red) **demonstrated, not asserted**.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: **one file** — `crates/core/src/metadata.rs`: the primitive, its two
  addressing helpers, the segmented-arm ceiling check, and the in-crate unit tests.
  **Salvage:** the archived attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a
  working primitive that passed C4-ci — reuse it, correcting every doc site that claims
  the move pins "the exact bytes the resolve read". **Bounded memory:** pin one record at
  a time; no `seg:` range walk; no deep copy of a segmented root. **Untouched:**
  `commit_chunk_map` and its segmented refusal at `metadata.rs:1776-1780` stay exactly as
  they are; `resolve_chunk_map` and the whole read side; every custodian file; proposal
  0016 (draft, stays untouched); no conformance-vector change; no new dependency.
  **Budget:** 1 file, ≤ 170 added semantic non-test lines, `patch.diff` ≤ 50 KB (calibrated
  at Plan: the core half of the parent's v4 patch measured **~31 KB**, so this is comfortable).
  A second file means the shape is wrong — STOP and hand back.
  All merged prerequisites (#710 as PR #718; #695/#696/#697 as PRs #704/#705/#706) are already
  on `origin/main` — re-verified at Plan as commits `d2609b2`, `99c7fcf`, `3e05891` — so no
  `Depends on` is needed. (The scheduling conflict lives in the **Ordering note** field below,
  and it is **#772**, not #717: that inherited claim was stale and is corrected there.)

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 666 of 668 instrumentable changed lines executed (floor 80%); 668 of 1092 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 76s: 27 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.91s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #776’s placement-move primitive for flat and segmented metadata meets the scoped acceptance criteria; no implementation defect was found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is testable and explicitly separates this enabling primitive from #777’s repair integration; the three pins and full value ceiling are settled requirements (`brief.md:9`, `brief.md:25`, `brief.md:37`). |
| C2 Reproduction (red pre-fix) | N/A | This is an absent API, with green-only verification deliberately specified; stashing the patch independently confirmed the symbol is absent, without claiming a pre-fix runtime failure (`brief.md:70`, `reviewer-evidence/summary.log:2`). |
| C3 Change | PASS | Stale plans cannot overwrite newer placements, pending state survives, and caller mistakes remain distinguishable from object corruption (`crates/core/src/metadata.rs:3241`, `crates/core/src/metadata.rs:3308`, `crates/core/src/metadata.rs:3357`, `crates/core/src/metadata.rs:4563`, `crates/core/src/metadata.rs:4953`). |
| C4 Verification (red→green) | PASS | Independent execution passed 290 core tests; all four required correctness negations failed at runtime and restoring the patch passed all 16 move tests; the frozen CI run is green (`reviewer-evidence/summary.log:4`, `reviewer-evidence/named-negations.log:1`, `reviewer-evidence/named-negations.log:23`, `gate-logs/C4-ci.log:3734`). |
| C5 Causal adequacy | PASS | The missing write capability is supplied directly, with no capability-probe workaround; independent mutations found 27 caught, 9 unviable, zero missed, and explicit pin/ceiling/version negations failed (`crates/core/src/metadata.rs:3224`, `reviewer-evidence/mutants.log:40`, `reviewer-evidence/named-negations.log:8`). |
| T1 Structure | PASS | The caller can atomically compose placement and repair evidence through the existing MetadataStore/WriteBatch seam; core acquires no backend dependency (`crates/core/src/metadata.rs:3117`, `crates/core/src/metadata.rs:3225`). |
| T2 Shape | PASS | The authorized single-file budget is met: 144 added nonblank, noncomment production lines, counting delimiters, and 49,615 patch bytes; existing read and custodian paths are untouched (`reviewer-evidence/summary.log:7`, `patch.diff:1`). |
| T3 Runtime | PASS | Work is bounded by the root table and candidate record: no range walk or segmented-root clone; malformed input, arithmetic overflow and value-ceiling crossings fail closed (`crates/core/src/metadata.rs:3264`, `crates/core/src/metadata.rs:3279`, `crates/core/src/metadata.rs:3323`, `crates/core/src/metadata.rs:3360`). |
| T4 Contribution | N/A | Publication artifacts are absent by design at Check; the substantive contribution audit must rerun at publish, as the deferred gate explicitly records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm acceptance of Plan’s affected-path merged and closed/rejected-work check — the disposable target contains only a synthesized base and no remote, so that history cannot be independently corroborated here (`brief.md:133`, `reviewer-evidence/prior-art.log:4`, `reviewer-evidence/prior-art.log:7`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept delivery of the enabling primitive before #777 — verified CAS mechanics do not yet restore user-visible segmented repair, which this brief expressly leaves to the integration child (`brief.md:7`, `brief.md:117`). |

The independent evidence supports the correctness verdict. `cargo test --offline -p wyrd-core` passed all 290 tests, including the 256-seed move/move and move/retirement campaign (`reviewer-evidence/core-tests.log:243`, `crates/core/src/metadata.rs:5242`). Removing chunk equality, the root pin, the output ceiling, or checked version advancement each caused runtime test failures. Forcing Pending to Committed also failed its regression. Restoring the original source returned all 16 placement-move tests to green (`reviewer-evidence/named-negations.log:1`). The source was restored byte-for-byte to the supplied patch after every mutation campaign.

The local CI interruption was a host limitation, not a patch failure. Formatting, clippy, build, workspace tests, spelling, documentation rendering/link audit and dependency-use scanning passed before cargo-deny could not lock its read-only advisory cache (`reviewer-evidence/ci.log:3005`). Both advisory audits then passed using a byte-for-byte copy of that cache under this sandbox; parsed configurations differed only in `advisories.db-path`, with audit rules unchanged (`reviewer-evidence/deny-local-cache.log:1`, `reviewer-evidence/deny-local-cache.log:15`, `reviewer-evidence/deny-local-cache.log:26`). The all-feature license/bans/sources scan also passed (`reviewer-evidence/deny.log:7`). Separately rerun conformance, statics, and DST clippy/tests passed (`reviewer-evidence/ci-remainder.log:2`, `reviewer-evidence/ci-remainder.log:7`, `reviewer-evidence/ci-remainder.log:595`). No declared tool dependency remains undischarged.

Every frozen gate log was readable. Their verdicts are bounded by what they actually recorded:

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | PASS | The frozen full CI run completed successfully, including conformance, statics, deploy guard and DST (`gate-logs/C4-ci.log:3139`, `gate-logs/C4-ci.log:3734`); independent rerun details and the resolved cache limitation are above. |
| C4-verify | PASS | Green-only, explicitly; it does not establish a pre-fix failing test (`gate-logs/C4-verify.log:120`). |
| C4-diff-cov | PASS | Frozen measurement: 666/668 instrumentable changed lines, 99.7%; 424 other lines unscored, with the co-located-test counting limitation disclosed (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:440`, `gate-logs/C4-diff-cov.log:458`). |
| C5-mutants | PASS | Independently reproduced the frozen result: 36 tested, 27 caught, 9 unviable, zero missed (`gate-logs/C5-mutants.log:13`, `reviewer-evidence/mutants.log:40`). |
| T4-batch-review | PASS | Frozen output reports zero blocking, rejected, or noise-dropped findings (`gate-logs/T4-batch-review.log:10`); this review independently inspected the implementation and tests. |
| T4-contribution | N/A | Deferred until publish, when the artifacts exist and this audit reruns (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Frozen output records successful feature-enabled clippy compilation of the metadata crate and server; it does not claim a live-cluster run (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). |

Source citations refer to the supplied `$PDCA_TARGET`, whose uncommitted diff exactly matches `patch.diff`; there is no stale-target caveat. The standing `AGENTS.md` rubric was applied. `INTEGRATION.md` was neither supplied nor present in the target, so no additional human-only rules were inferred (`reviewer-evidence/prior-art.log:10`). The prior-art limitation is a sign-off decision, not an implementation defect or an assertion that conflicting prior work exists.

### Advisory — adversary

# Adversarial review — issue #776 (`repoint_chunk`)

I re-ran the 16 `placement_move` tests on a scratch copy of `$PDCA_TARGET` (all green), then ran
16 hand mutations and 2 probe tests against them. 12 mutations were caught, 3 survived as
harmless equivalents, and 1 survivor matters. Two findings below; the rest of my attempts to
refute the patch failed.

- NEEDS-HUMAN [impl] — **Brief item 2 ("no other segment decoded") is not pinned at a segment boundary, and C5's "0 missed" does not cover it.** `crates/core/src/metadata.rs:3267` passes `prior.len` to `segment_may_hold`. Change that argument to `0` and a non-empty chunk that *starts* a segment also reads and decodes the previous segment, yet all 16 `placement_move` tests stay green. I ran it. cargo-mutants never swaps a call-site argument, so the gate's 27 caught / 0 missed says nothing here. Concrete failing case under the mutant: `two_segments`, segment 0 overwritten with garbage while the root still names the generation, move `b()` at offset 5. The patch returns `Prepared` (correct). The mutant returns `Err(SegmentRecordUndecodable { index: 0 })`: a healthy chunk becomes unrepairable because its neighbour is damaged. The test meant to pin this, `segmented_arm_rewrites_only_the_covering_segment_and_never_the_root` (`:4624`), moves `c()` at offset 8 (`:4635`). Offset 8 is inside segment 1 and never touches segment 0's edge, so it can't see the defect. Fix: add the offset-5 / `b()` move to that test. As a probe it passes on the patch and fails under the mutant.

- NEEDS-HUMAN [human] — **`Repoint::Refused` can be returned for a generation the root has already left, but its doc says it is not a race.** The doc at `crates/core/src/metadata.rs:3125` says Refused "is not transient … an operator signal rather than a retry". The segmented arm weighs the rewritten record (`:3307` → `weighed`, `:3323`) without checking whether the root still names the generation. Only faults go through `retired_or` (`:3316`). Probe: seed a one-segment root whose row the move pushes 1 byte over `MAX_VALUE_BYTES`, then overwrite the root with a flat successor (the old `seg:` row is not yet reclaimed), then call `repoint_chunk` with the old generation. Result: `Ok(Refused { bytes: 100001, ceiling: 100000 })`, not `Conflict`. A #777 caller that follows the doc would raise an operator alert for an object that is fine. `VersionExhausted` (`:3137`) has the same shape, because the flat arm never reads the store. It is rare: it needs a non-conforming row within ~19 bytes of V plus a race with retirement. There are two fixes: spend one root read (`root_dropped`) before returning `Refused`, or change the doc to say the caller must confirm the generation is still live before escalating. Choosing between them is a judgment call.

- Surviving hand mutations I judge harmless, not findings: (a) dropping the early `break` in `chunk_at` (`:3354`) only costs time, because `at` never shrinks; (b) `continue` instead of `return Ok(Conflict)` after a retired fault (`:3316`) can at most turn a prepare-time `Conflict` into a batch whose root pin fails at commit, so it still writes nothing; (c) checking the version before `chunk_at` in the flat arm swaps one refusal for another.

- I tried to refute these and could not. Deleting the equality pin (id-only, or ignoring placement), the root pin in either arm, or the segment pin, and changing the over-ceiling guard from `>` to `>=`: all caught, including by the seeded campaign. Also caught: skipping the ceiling in the segmented arm, a placement check that only runs in the flat arm or accepts an empty vector, a bounds check that ignores length, and forcing `state: Committed` in the flat arm (the iteration-2 finding, now pinned). A `saturating_add` in `chunk_at` (the iteration-1 overflow finding) is now caught by the `u64::MAX` query at `:4994`ff. The over-ceiling live row (iteration 1) is refused before decode and routed through `retired_or`, with tests for both the live and the retired case.

- Brief constraints check out. No `MAX_ROOT_VALUE_BYTES` comparison appears in code, only in docs. There are 144 non-comment production lines (limit ≤170). `patch.diff` is 49,615 bytes (limit ≤50 KB). The patch touches one file. `wyrd-testkit`, `pollster` and `wyrd-metadata-redb` were already dev-dependencies. No stale `#682` or "exact bytes the resolve read" claims remain in `metadata.rs`. The root pin is `encode(generation)`, not the stored bytes. That relies on decode→encode round-tripping, but the whole tree already relies on it (`commit_chunk_map` `:2142`, `reconstruction.rs:1188`, `backfill.rs:251`), so it is not this diff's debt. The seeded campaign plans from bytes the resolver decoded, so it does exercise that round-trip.

- On the evidence: C4-verify is green-only, and the brief declares that up front, so it is not a refutation. The two lines diff coverage misses (`:4487`, `:5202`) are `panic!` arms inside tests. T4 reported 0 blocking. That fits the code as far as it goes, but the review and C5 both missed the boundary gap in the first bullet.

### Advisory — code-review

- No findings on either lens in `crates/core/src/metadata.rs:3224` and `crates/core/src/metadata.rs:4433`: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified. Reviewed placement validation, CAS pins, segment-boundary addressing, corruption handling, value ceilings, version advancement, and the seeded race tests.

Validation relied on the frozen gate evidence: CI passed, all 16 placement-move tests passed, and mutation testing reported 27 caught, 9 unviable, and zero missed mutants. Gates were not rerun; the target remained read-only.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm acceptance of Plan’s affected-path merged and closed/rejected-work check — the disposable target contains only a synthesized base and no remote, so that history cannot be independently corroborated here (`brief.md:133`, `reviewer-evidence/prior-art.log:4`, `reviewer-evidence/prior-art.log:7`).
- [ ] Validation — fitness-to-purpose — Accept delivery of the enabling primitive before #777 — verified CAS mechanics do not yet restore user-visible segmented repair, which this brief expressly leaves to the integration child (`brief.md:7`, `brief.md:117`).
- [ ] **Brief item 2 ("no other segment decoded") is not pinned at a segment boundary, and C5's "0 missed" does not cover it.** `crates/core/src/metadata.rs:3267` passes `prior.len` to `segment_may_hold`. Change that argument to `0` and a non-empty chunk that *starts* a segment also reads and decodes the previous segment, yet all 16 `placement_move` tests stay green. I ran it. cargo-mutants never swaps a call-site argument, so the gate's 27 caught / 0 missed says nothing here. Concrete failing case under the mutant: `two_segments`, segment 0 overwritten with garbage while the root still names the generation, move `b()` at offset 5. The patch returns `Prepared` (correct). The mutant returns `Err(SegmentRecordUndecodable { index: 0 })`: a healthy chunk becomes unrepairable because its neighbour is damaged. The test meant to pin this, `segmented_arm_rewrites_only_the_covering_segment_and_never_the_root` (`:4624`), moves `c()` at offset 8 (`:4635`). Offset 8 is inside segment 1 and never touches segment 0's edge, so it can't see the defect. Fix: add the offset-5 / `b()` move to that test. As a probe it passes on the patch and fails under the mutant.
- [ ] **`Repoint::Refused` can be returned for a generation the root has already left, but its doc says it is not a race.** The doc at `crates/core/src/metadata.rs:3125` says Refused "is not transient … an operator signal rather than a retry". The segmented arm weighs the rewritten record (`:3307` → `weighed`, `:3323`) without checking whether the root still names the generation. Only faults go through `retired_or` (`:3316`). Probe: seed a one-segment root whose row the move pushes 1 byte over `MAX_VALUE_BYTES`, then overwrite the root with a flat successor (the old `seg:` row is not yet reclaimed), then call `repoint_chunk` with the old generation. Result: `Ok(Refused { bytes: 100001, ceiling: 100000 })`, not `Conflict`. A #777 caller that follows the doc would raise an operator alert for an object that is fine. `VersionExhausted` (`:3137`) has the same shape, because the flat arm never reads the store. It is rare: it needs a non-conforming row within ~19 bytes of V plus a race with retirement. There are two fixes: spend one root read (`root_dropped`) before returning `Refused`, or change the doc to say the caller must confirm the generation is still live before escalating. Choosing between them is a judgment call.
- [ ] external dependency.
- [ ] **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.
- [ ] In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — **Brief item 2 ("no other segment decoded") is not pinned at a segment boundary, and C5's "0 missed" does not cover it.** `crates/core/src/metadata.rs:3267` passes `prior.len` to `segment_may_hold`. Change that argument to `0` and a non-empty chunk that *starts* a segment also reads and decodes the previous segment, yet all 16 `placement_move` tests stay green. I ran it. cargo-mutants never swaps a call-site argument, so the gate's 27 caught / 0 missed says nothing here. Concrete failing case under the mutant: `two_segments`, segment 0 overwritten with garbage while the root still names the generation, move `b()` at offset 5. The patch returns `Prepared` (correct). The mutant returns `Err(SegmentRecordUndecodable { index: 0 })`: a healthy chunk becomes unrepairable because its neighbour is damaged. The test meant to pin this, `segmented_arm_rewrites_only_the_covering_segment_and_never_the_root` (`:4624`), moves `c()` at offset 8 (`:4635`). Offset 8 is inside segment 1 and never touches segment 0's edge, so it can't see the defect. Fix: add the offset-5 / `b()` move to that test. As a probe it passes on the patch and fails under the mutant.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
