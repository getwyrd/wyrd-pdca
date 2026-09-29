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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 657 of 659 instrumentable changed lines executed (floor 80%); 659 of 1086 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 76s: 27 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.97s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #776: the flat/segmented placement-move primitive passes functional checks; its required architecture update conflicts with the brief’s one-file scope.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Resolve the one-file scope against the same-PR architecture requirement — satisfying both is impossible without a Plan amendment or an explicit policy exception (`brief.md:83`, `brief.md:95`, `AGENTS.md:154`). |
| C2 Reproduction (red pre-fix) | N/A | This adds an absent API; stash confirmed its absence and 48 baseline tests passed, consistent with the declared green-only posture rather than a claimed pre-fix assertion failure (`brief.md:104`, `reviewer-evidence/target-check.log:2`). |
| C3 Change | PASS | The scoped primitive permits atomic placement-plus-evidence commits while preserving metadata and rejecting stale plans; existing repair behavior remains outside this child (`crates/core/src/metadata.rs:3245`, `crates/core/src/metadata.rs:3312`, `crates/core/src/metadata.rs:3361`). |
| C4 Verification (red→green) | PASS | The 289-test core rerun and six runtime-red negations followed by 15 restored-green tests support the contract; frozen full CI passes and supplemental audits discharge the local cache fault (`reviewer-evidence/rerun-summary.log:3`, `reviewer-evidence/named-negations.log:1`, `gate-logs/C4-ci.log:3733`). |
| C5 Causal adequacy | PASS | The missing segment-addressable mutation is supplied directly; sibling preservation, retirement conflicts and full-ceiling refusal have executable counterexamples, with no capability probe masking a load-time cause (`crates/core/src/metadata.rs:3275`, `reviewer-evidence/named-negations.log:2`, `reviewer-evidence/mutants.log:40`). |
| T1 Structure | PASS | The production dependency remains the narrow MetadataStore seam and the caller retains the atomic commit boundary; no runtime/backend dependency or persistent schema change is introduced (`crates/core/src/metadata.rs:3229`, `crates/core/src/metadata.rs:3311`, `crates/traits/src/lib.rs:1521`). |
| T2 Shape | PASS | One changed file, 144 added production nonblank/noncomment lines including delimiters, and 49,677 patch bytes fit the explicit budget (`reviewer-evidence/shape.log:1`, `brief.md:93`). |
| T3 Runtime | PASS | At most two candidate segment values are read, arithmetic and both size boundaries are checked, and awaited operations inherit the store’s termination contract (`crates/core/src/metadata.rs:3271`, `crates/core/src/metadata.rs:3283`, `crates/core/src/metadata.rs:3327`, `crates/traits/src/lib.rs:1333`). |
| T4 Contribution | FAIL | The new public API has no living architecture update, leaving its caller-committed batch and flat/segment contracts undocumented there despite the mandatory rule (`crates/core/src/metadata.rs:3228`, `docs/design/architecture/06-runtime-view.md:40`, `AGENTS.md:154`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | PASS | Seeded move/move and retirement campaigns assert store contents and losing-batch nonwrites; path-based merged and rejected-work checks are complete and support this bounded split (`crates/core/src/metadata.rs:5219`, `crates/core/src/metadata.rs:5236`, `reviewer-evidence/prior-art-summary.log:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the primitive-only milestone with operational repair still owed to #777 — the custodian continues refusing segmented placement writes, so this child alone does not restore repair availability (`brief.md:2`, `crates/custodian/src/reconstruction.rs:1143`). |

Source paths above are relative to `$PDCA_TARGET`; brief and evidence paths are relative to this review directory. The disposable target exactly matches `patch.diff` after testing. No implementation edits remain.

The outstanding finding requires a scope decision, not another code rebuild against the same brief. The architecture’s repair description at `docs/design/architecture/06-runtime-view.md:40` does not document the new primitive’s contract. `AGENTS.md:154` requires that update in this PR, while `brief.md:95` forbids a second file. Amend the scope to include the living architecture update, or record an explicit exception. No supplied tracked-issue deferral settles this conflict. The frozen batch review raises the same issue (`gate-logs/T4-batch-review.log:10`); this review does not create an acceptance gate.

The functional evidence is independently reproduced. Stashing the patch confirms an absent API, not an assertion-red baseline; the ordinary C4-verify result is therefore correctly understood as green-only (`gate-logs/C4-verify.log:120`). The patched core suite passes 289 tests. Independent mutation testing reproduces 27 caught, nine unviable and zero missed mutants. Six additional reversible negations each compile and fail at runtime (the unchecked version panics on overflow): removing chunk equality, the root pin, the segment pin, the output ceiling, checked version advancement, or the nonempty chunk’s segment-boundary restriction. Restoring the original source makes all 15 placement-move tests pass (`reviewer-evidence/named-negations.log:1`, `reviewer-evidence/restored-green.log:22`). The 256-seed campaign covers competing moves, sibling merges, root overwrite/deletion and reclamation, with byte comparisons after losing commits (`crates/core/src/metadata.rs:5073`, `crates/core/src/metadata.rs:5238`).

The gate evidence supports its reported limits:

- **C4-ci — PASS:** the frozen log records an uninterrupted complete run (`gate-logs/C4-ci.log:3733`). The pinned 1.96.0 local rerun passes through workspace tests and cargo-machete, then hits a read-only advisory-cache lock (`reviewer-evidence/ci-pinned.log:3103`). All three real cargo-deny checks subsequently pass using identical policies with only the cache path relocated; conformance, statics and DST clippy/tests also pass separately (`reviewer-evidence/rerun-summary.log:6`). This is a resolved local host limitation, not a patch verification defect or an unmet tool dependency.
- **C4-verify — PASS, green-only:** no isolated pre-fix red is claimed; this matches the brief’s chosen verification posture (`gate-logs/C4-verify.log:120`).
- **C4-diff-cov — PASS from frozen evidence:** 657 of 659 instrumentable changed lines execute, or 99.7%; the metric includes co-located tests and is not production-only coverage (`gate-logs/C4-diff-cov.log:455`).
- **C5-mutants — PASS:** the independently reproduced count matches the frozen result (`gate-logs/C5-mutants.log:13`, `reviewer-evidence/mutants.log:40`).
- **T4-batch-review — FAIL:** its sole reported finding is the architecture omission independently grounded above (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution — N/A:** contribution artifacts are intentionally drafted later; the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`).
- **host-tikv — PASS from frozen evidence:** both backend and server feature selections compile; this is compile evidence, not a claim of live-cluster behavior (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`).

The prior-art check is complete by affected path. GitHub history for `crates/core/src/metadata.rs` contains the named merged prerequisites. All 19 closed-unmerged PRs were checked by filename; only [#647](https://github.com/getwyrd/wyrd/pull/647) touches this path, and its [closing comment](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571) rejects its excessive scope. This one-file child addresses that reason. The open-PR query returns zero (`reviewer-evidence/prior-art-summary.log:1`). No additional human prior-art investigation is owed.

### Advisory — adversary

# Adversarial review — issue #776 (`repoint_chunk` placement-move primitive)

I could not refute the fix. I re-ran the evidence myself and attacked the addressing, the pins, the ceiling and the arbiter path. Every attack either got caught by a test or turned out to change nothing observable. One item needs a human call: the only gating red, which is the T4 docs-currency finding.

## Findings

- NEEDS-HUMAN [human] — **The one gating red (T4 "Docs currency" at `crates/core/src/metadata.rs:3228`) is arguable, and it clashes with the brief's one-file scope.** The rubric's trigger list is "a port, an API operation, an RPC, a CLI flag, or a persisted field". `repoint_chunk` is none of these. It is an in-crate library function with no callers, no wire or persisted change, and no trait seam. The living architecture docs don't track functions at this level either: `resolve_chunk_map` and `commit_chunk_map` appear nowhere under `docs/design/architecture/`. The behavioural claim at `docs/design/architecture/08-crosscutting-concepts.md:85` ("the maintenance loops that … move them … treat a shape they cannot resolve as a typed error") stays true until #777 wires the primitive in. One sentence in that same paragraph does go stale in spirit: "an object whose root can no longer be re-written is an object whose placement can never be repaired". This primitive repairs a segmented object's placement without re-writing the root (`metadata.rs:3312` pins the root but never `put`s it). The human has to choose: (a) reject the T4 finding and record the reason (the doc update ships with #777, where behaviour changes), or (b) widen this child to two files. The brief says a second file means "STOP and hand back", so (b) is a scope change, not a rebuild.

## Attempted refutations that failed (evidence, not findings)

- **Red legs re-run by hand, not taken from build-notes.** I ran these on a scratch copy of `$PDCA_TARGET`:
  - Removing the equality check (`chunk == prior` → `(chunk == prior || true)`, `metadata.rs:3361`) makes 5 tests fail, including `a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts` and `a_zero_length_chunk_on_a_segment_boundary_is_found_by_equality`.
  - Removing the root pin from the segmented batch (`:3312`) fails `a_superseded_root_fails_the_batch_or_the_move` and the seeded campaign. Removing it from the flat batch (`:3263`) fails `flat_arm_conflicts_on_a_changed_chunk_and_on_a_superseded_root` and the campaign.
  - Removing the ceiling check (`:3328`) fails both ceiling tests.
  - `checked_add` → `wrapping_add` (`:3251`) fails `flat_arm_refuses_a_version_it_cannot_advance`.
  - Removing the segment pin (`:3312`) fails the sibling-edit test and the campaign.
- **Argument-level mutations cargo-mutants never generates** (the class that slipped through in iteration 3). All were caught:
  - `SegmentRecord::new(chunks, byte_offset)` instead of `segment.byte_offset` (`:3310`)
  - `within = byte_offset` without subtracting the segment start (`:3304`)
  - `continue` → `return Conflict` (`:3306`)
  - dropping `retired_or` so every anomaly becomes a plain `Conflict` (`:3320`)
- **Two mutants survived, and neither changes behaviour.**
  - Removing the early `break` in `chunk_at` (`:3358`) is a pure speed-up: `at` only grows, so nothing past the offset can match.
  - Walking the candidate segments in reverse (`:3271`) only matters for a zero-length chunk on a boundary where the *other* candidate is damaged on a live generation. In that case the resolver already refuses the whole object (`read_segments`, `:2939-2986`), so the object is unreadable either way. The brief doesn't set an order.
- **Never-ending conflict for a correct caller.** A caller that plans from a fresh resolve (offset = sum of the lengths before the chunk) always has a candidate segment:
  - A chunk with bytes lives in the segment its first byte falls in.
  - A zero-length chunk sits at most on two segments' shared edge, and `segment_may_hold` admits both (`:3342-3347`).
  - Every segment is non-empty (`SegmentRecord::checked`, `:1376`), so no more than two segments can meet at one offset.
- **Root pin stability.** The segmented arm pins `encode(generation)` (`:3245`), not the raw root bytes. The segmented wire shape has a fixed field order (`SegmentedMapWireOut`, `:1193`). The inode's optional fields are omitted when absent (`:1631-1644`). There is no production producer of segmented roots yet (`checked_for_publication`, `:1727-1736`). So decode→encode matches byte for byte, which `seed_segmented` and the campaign exercise.
- **Gate evidence.**
  - `gate-logs/C4-ci.log:3733` is a full, clean `xtask ci` run (typos, docs render, deny, all 15 `placement_move` tests at `:870-923`), so iteration 4's CI-timeout caveat is resolved.
  - C4-verify being green-only was declared up front in the brief and is expected.
  - The two diff-coverage MISS lines (`:4491`, `:5196`) are `panic!` arms in the tests.
  - Scope matches the brief: one file, about 144 non-comment non-test lines (under the 170 limit), `patch.diff` at 49,677 bytes, no `MAX_ROOT_VALUE_BYTES` comparison, and `commit_chunk_map` untouched.

### Advisory — code-review

No additional advisory findings: the diff is clean under the introduced-correctness and reuse/simplification/efficiency lenses.

Reviewed the placement primitive (`crates/core/src/metadata.rs:3228`), addressing helpers (`crates/core/src/metadata.rs:3342`, `crates/core/src/metadata.rs:3355`), and regression/seeded tests (`crates/core/src/metadata.rs:4437`). Grounding used only the supplied target source. Validation relied on frozen evidence, without rerunning builds: CI passed; mutation testing reported 27 caught, 9 unviable, and no missed mutants; per-fix verification was green-only as planned.

The existing T4 docs-currency finding concerning the new public operation (`crates/core/src/metadata.rs:3228`) remains recorded in `gate-logs/T4-batch-review.log`; this advisory does not clear that gate or duplicate its finding.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Resolve the one-file scope against the same-PR architecture requirement — satisfying both is impossible without a Plan amendment or an explicit policy exception (`brief.md:83`, `brief.md:95`, `AGENTS.md:154`).
- [ ] Validation — fitness-to-purpose — Accept the primitive-only milestone with operational repair still owed to #777 — the custodian continues refusing segmented placement writes, so this child alone does not restore repair availability (`brief.md:2`, `crates/custodian/src/reconstruction.rs:1143`).
- [ ] **The one gating red (T4 "Docs currency" at `crates/core/src/metadata.rs:3228`) is arguable, and it clashes with the brief's one-file scope.** The rubric's trigger list is "a port, an API operation, an RPC, a CLI flag, or a persisted field". `repoint_chunk` is none of these. It is an in-crate library function with no callers, no wire or persisted change, and no trait seam. The living architecture docs don't track functions at this level either: `resolve_chunk_map` and `commit_chunk_map` appear nowhere under `docs/design/architecture/`. The behavioural claim at `docs/design/architecture/08-crosscutting-concepts.md:85` ("the maintenance loops that … move them … treat a shape they cannot resolve as a typed error") stays true until #777 wires the primitive in. One sentence in that same paragraph does go stale in spirit: "an object whose root can no longer be re-written is an object whose placement can never be repaired". This primitive repairs a segmented object's placement without re-writing the root (`metadata.rs:3312` pins the root but never `put`s it). The human has to choose: (a) reject the T4 finding and record the reason (the doc update ships with #777, where behaviour changes), or (b) widen this child to two files. The brief says a second file means "STOP and hand back", so (b) is a scope change, not a rebuild.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- [ ] **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.
- [ ] In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.
- [ ] T5 Judgment — Confirm acceptance of Plan’s affected-path merged and closed/rejected-work check — the disposable target contains only a synthesized base and no remote, so that history cannot be independently corroborated here (`brief.md:133`, `reviewer-evidence/prior-art.log:4`, `reviewer-evidence/prior-art.log:7`).
- [ ] **`Repoint::Refused` can be returned for a generation the root has already left, but its doc says it is not a race.** The doc at `crates/core/src/metadata.rs:3125` says Refused "is not transient … an operator signal rather than a retry". The segmented arm weighs the rewritten record (`:3307` → `weighed`, `:3323`) without checking whether the root still names the generation. Only faults go through `retired_or` (`:3316`). Probe: seed a one-segment root whose row the move pushes 1 byte over `MAX_VALUE_BYTES`, then overwrite the root with a flat successor (the old `seg:` row is not yet reclaimed), then call `repoint_chunk` with the old generation. Result: `Ok(Refused { bytes: 100001, ceiling: 100000 })`, not `Conflict`. A #777 caller that follows the doc would raise an operator alert for an object that is fine. `VersionExhausted` (`:3137`) has the same shape, because the flat arm never reads the store. It is rare: it needs a non-conforming row within ~19 bytes of V plus a race with retirement. There are two fixes: spend one root read (`root_dropped`) before returning `Refused`, or change the doc to say the caller must confirm the generation is still live before escalating. Choosing between them is a judgment call.
- [ ] external dependency.
- [ ] C1 Spec — Resolve the one-file scope against mandatory architecture documentation for the new public API — the brief forbids the additional document while the standing rubric requires it (`brief.md:83`, `AGENTS.md:154`, `crates/core/src/metadata.rs:3224`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b. 9 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
