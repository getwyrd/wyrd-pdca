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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.1% — 659 of 665 instrumentable changed lines executed (floor 80%); 665 of 1078 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 77s: 27 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.90s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #776: add an atomic placement-move batch for flat inode and segmented chunk records, enabling later custodian integration in #777.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines falsifiable pinning, corruption, ceiling, and concurrency requirements for this primitive; it explicitly leaves operational repair to #777 (`brief.md:9`, `brief.md:70`, `brief.md:83`). |
| C2 Reproduction (red pre-fix) | N/A | This is an absent API, with green-only verification explicitly agreed; stashing independently confirmed both API and tests absent on the base, then restored the patch byte-for-byte (`brief.md:80`, `reviewer-evidence/base-check.log:2`). |
| C3 Change | PASS | Callers retain atomic placement/evidence composition, and stale batches cannot overwrite newer roots or segment edits; preparation itself writes nothing (`crates/core/src/metadata.rs:3232`, `crates/core/src/metadata.rs:3301`, `crates/core/src/metadata.rs:5174`). |
| C4 Verification (red→green) | PASS | The declared green-only posture is supported by independent core/workspace/DST runs and six failing negations followed by restored green; full gate acceptance also uses the frozen CI log, with the local audit-cache limitation explained below (`reviewer-evidence/named-negations.log:1`, `gate-logs/C4-ci.log:3734`, `gate-logs/C4-verify.log:120`). |
| C5 Causal adequacy | PASS | The missing segment-addressable write capability is supplied directly; removing either CAS pin, reference equality, the output ceiling, or checked arithmetic demonstrably breaks tests, without an optional-capability probe masking another cause (`crates/core/src/metadata.rs:3215`, `reviewer-evidence/named-negations.log:1`). |
| T1 Structure | PASS | Correctness stays within the existing metadata trait, atomic-batch, codec, and retirement-arbitration contracts; no backend coupling or separate placement/evidence commit is introduced (`crates/core/src/metadata.rs:3216`, `crates/core/src/metadata.rs:3279`, `crates/core/src/metadata.rs:3309`). |
| T2 Shape | PASS | The agreed one-file scope is respected: 49,337 patch bytes and 145 added nonblank, noncomment production lines, below both budgets; existing readers and custodian behavior are untouched (`brief.md:83`, `reviewer-evidence/shape.log:1`, `patch.diff:1`). |
| T3 Runtime | PASS | Segment work stays bounded to candidate records selected from the root table, preserving sibling changes and refusing corruption or oversized output; equality resolves zero-length boundary ambiguity (`crates/core/src/metadata.rs:3257`, `crates/core/src/metadata.rs:3272`, `crates/core/src/metadata.rs:3316`, `crates/core/src/metadata.rs:4779`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit must rerun at publish, as the deferred gate explicitly records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | All carried implementation findings now have discriminating regressions, including seeded move/retirement races; affected-path merged and rejected history was independently checked, with #647's excessive-scope rejection respected (`crates/core/src/metadata.rs:4893`, `crates/core/src/metadata.rs:4925`, `crates/core/src/metadata.rs:4997`, `crates/core/src/metadata.rs:5228`, `reviewer-evidence/prior-art-followup.log:136`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this primitive's fitness for the intended repair workflow — the evidence proves batch safety, while actual custodian obligation/orphan composition and operational repair remain the explicitly separate #777 integration (`brief.md:7`, `brief.md:14`). |

No new implementation defect was found. The previous placement-validation, oversized-input, arithmetic-test, and seeded-concurrency findings are discharged. This is an advisory assessment; fitness-to-purpose remains the required human decision.

- **Safety evidence:** the independent core run passed all 290 tests, including 16 placement tests and the 256-seed campaign. Diff mutation testing reproduced 27 caught and 9 unviable mutations, with none missed (`reviewer-evidence/mutants.log:39`); the unviable function-body replacements were rejected by warnings-as-errors, so they are not claimed as test kills. Separate scratch-copy negations removed the root pin, segment pin, equality check, and output ceiling, replaced checked version advancement with unchecked addition, and replaced checked offset accumulation with saturation. Every negation produced a test failure; restoring the source passed all 16 placement tests (`reviewer-evidence/named-negations.log:1`, `reviewer-evidence/negation-restored-green.log:23`). The judged target remains the exact patch blob `674b4849ac5c75675fd3cf20c2f49e2480d3952d`.
- **Verification boundary:** the isolated CI replay passed formatting, clippy, build, 1,440 workspace tests, docs lint/render, repository guards, and cargo-machete. It stopped at cargo-deny's attempt to lock a read-only advisory cache (`reviewer-evidence/ci-isolated.log:3104`); offline retry met the same host restriction. The frozen log actually shows all three dependency-wall audits passing (`gate-logs/C4-ci.log:3126`, `gate-logs/C4-ci.log:3137`, `gate-logs/C4-ci.log:3140`), so this is a local replay caveat, not an unmet gate dependency or patch defect. Conformance, the static-state scanner, and 80 DST tests were rerun separately and passed (`reviewer-evidence/remaining-ci.log:1`). An earlier shared-cache replay's missing-crate doctest error disappeared with the isolated cache. The instance-scoped coverage and TiKV wrappers were adjudicated from their full frozen logs: 659/665 instrumentable changed lines covered, with 413 lines unscored, and both TiKV clippy targets completed (`gate-logs/C4-diff-cov.log:459`, `gate-logs/host-tikv.log:209`). The frozen multi-pass review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`).
- **Prior art and grounding:** source citations were checked in `$PDCA_TARGET`, whose patched file matches `patch.diff`; no stale-target caveat is needed. The affected-path commit query confirmed the merged prerequisites (`reviewer-evidence/prior-art.log:4`). A complete scan of 355 closed/merged PRs checked `crates/core/src/metadata.rs`, including all files of the two PRs exceeding the initial 100-file page (`reviewer-evidence/prior-art-closed.log:1`, `reviewer-evidence/prior-art-followup.log:1`). The relevant unmerged #647 was closed with a request to make it smaller; this one-file primitive follows that constraint. No unresolved prior-art decision remains.

### Advisory — adversary

# Adversarial review — issue #776 (`repoint_chunk`, seg-record placement move)

**Bottom line: I tried to refute the fix and could not break its core.** Every guard the brief names goes red when removed. The three findings below are minor. None shows a wrong write, a lost write, or a batch that lands when it should not.

## What I re-ran (toolchain present, scratch copy of `$PDCA_TARGET`)

- The 16 new `metadata::placement_move` tests pass on the patched tree, matching `gate-logs/C4-ci.log:871-908`.
- `C4-verify` is green-only, as `brief.md` declares (`gate-logs/C4-verify.log`, "PASS (green-only)"). So I did not rely on it. `C5-mutants` reports 36 mutants, 27 caught, 9 unviable, 0 missed, but the log does not name them. To fill that gap I applied 13 hand mutations that `cargo mutants` does not generate. 11 were caught, including all four negations the brief names:
  - drop `&& chunk == prior` (`crates/core/src/metadata.rs:3350`): 5 tests go red (sibling/planned-chunk conflict, zero-length boundary, `chunk_at` unit, flat conflict, seeded campaign).
  - drop the root pin in the segmented arm (`:3301`): superseded-root test and seeded campaign go red.
  - drop the segment pin (`:3301`): sibling-edit test and seeded campaign go red.
  - disable the `bytes.len() > MAX_VALUE_BYTES` guard (`:3272`): over-ceiling-row test goes red.
  - drop the `byte_offset` half of the bounds check, drop the `retired_or` call (`:3309`), turn the zero-length `continue` into `Conflict`, swap `checked_add` for `saturating_add` in `chunk_at`, disable the placement-length check (`:3224`), skip `weighed` in the segmented arm, drop the flat version bump: each caught.
  - Survivor 1: deleting `if at > byte_offset { break; }` in `chunk_at`. This is an equivalent mutant. `at` never decreases, so the break only ends the loop early. Not a finding.
  - Survivor 2: forcing `state: InodeState::Committed` in the flat arm. Finding 1 below.
- Other attacks that did not land:
  - Duplicate identical `ChunkRef`s in one map, which would defeat the equality pin. Chunk ids are minted unique (`crates/server/src/lib.rs:255`), and CopyObject / UploadPartCopy are refused (`crates/gateway-s3/src/lib.rs:1713-1730`), so no conforming writer can produce two in one map.
  - A zero-length segment making the "at most two records read" claim false. It is refused at decode (`metadata.rs:1152`, `:1401`).
  - `SegmentRecord::new` failing after a placement-only edit. It runs the same checks as decode (`:1385-1435`), so it cannot fail.
  - A root table over `MAX_ROOT_SEGMENTS`. The move does not refuse it, which matches the documented rule that the capacity guard applies only where a table becomes work (`:534-539`).
  - Any `MAX_ROOT_VALUE_BYTES` comparison in the diff. None exists; it appears only in doc comments.
- The previous round's 8 blocking findings are addressed and pinned by tests:
  - over-ceiling segment row: `:4893-4922`, and the mutation above is caught.
  - malformed replacement placement: `:4925-4952`.
  - overflow in `chunk_at`: `:4996-4997` now queries at `u64::MAX`, and the `saturating_add` mutant is caught.
  - seeded Tier-0 campaign: `:5002-5235`. It drives the production resolver and redb, and it catches the pin removals.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3194` says the flat arm leaves "`state` left as it was", but no test pins it. Every test builds a `Committed` generation (`flat_root`, patch test helper). Concrete case: change the flat arm's `InodeRecord { .. }` to add `state: InodeState::Committed` — exactly the peer idiom at `metadata.rs:2183` that the brief says to mirror — and all 16 `placement_move` tests still pass. I ran this. Under that change, a move on a `Pending` generation would publish it as `Committed`. Current callers skip non-committed records (`crates/custodian/src/reconstruction.rs:633`), so the risk is low. The fix is small: add a `Pending` flat generation to `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata` (`:4556`) and assert the state is unchanged. Or refuse a non-`Committed` generation outright.
- NEEDS-HUMAN [impl] — The two new `ChunkMapError` variants break that enum's own doc contract. `crates/core/src/metadata.rs:609-613` says every variant is "a structural violation of the segmented chunk-map shape", raised at decode or at a site that met an unwired `Segmented` map. `VersionExhausted` (`:848`) is neither, and `ReplacementPlacementMalformed` (`:857`) is a bad argument from the caller, not an object fault. This matters for #777: every custodian consumer downcasts `ChunkMapError` to mean "this object is unreadable, contain it and keep walking" (`crates/custodian/src/reconstruction.rs:639-645`, and the same pattern at `gc.rs:1245`, `rebalance.rs:316`, `restore.rs:734`, `backfill.rs:163`). A planner bug that passes a wrong-length placement would then be filed as per-object corruption instead of surfacing as a bug. Fix inside the same file: update the enum doc to admit write-side refusals, or return the placement refusal as its own type (`MalformedPlacement` at `:460` already has the same `{expected, actual}` shape).
- NEEDS-HUMAN [human] — In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.

## On the reviewer's verdict

- `check-gates.json` row `C4-verify` = `pass` is green-only. It proves nothing per fix, and the brief says so up front. It is not a hidden rationalization.
- The `C5-mutants` claim of zero survivors holds for what `cargo mutants` generates. My hand mutations reproduce the brief's four named negations. The one gap is the unpinned `state` behaviour in finding 1.
- I found no claim in `brief.md` or `check-gates.json` that the evidence does not support.

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found.

Reviewed the placement primitive (`crates/core/src/metadata.rs:3215`), addressing helpers (`crates/core/src/metadata.rs:3331`, `crates/core/src/metadata.rs:3344`), and regression tests including the seeded race campaign (`crates/core/src/metadata.rs:5228`). All five diff hunks match the read-only target source.

Validation evidence comes from the frozen gate logs: CI passed, diff coverage was 99.1%, and mutation testing reported 27 caught, 9 unviable, and no surviving mutants. C4-verify was green-only as declared in the brief. Tests were not rerun during this advisory review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept this primitive's fitness for the intended repair workflow — the evidence proves batch safety, while actual custodian obligation/orphan composition and operational repair remain the explicitly separate #777 integration (`brief.md:7`, `brief.md:14`).
- [ ] `crates/core/src/metadata.rs:3194` says the flat arm leaves "`state` left as it was", but no test pins it. Every test builds a `Committed` generation (`flat_root`, patch test helper). Concrete case: change the flat arm's `InodeRecord { .. }` to add `state: InodeState::Committed` — exactly the peer idiom at `metadata.rs:2183` that the brief says to mirror — and all 16 `placement_move` tests still pass. I ran this. Under that change, a move on a `Pending` generation would publish it as `Committed`. Current callers skip non-committed records (`crates/custodian/src/reconstruction.rs:633`), so the risk is low. The fix is small: add a `Pending` flat generation to `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata` (`:4556`) and assert the state is unchanged. Or refuse a non-`Committed` generation outright.
- [ ] The two new `ChunkMapError` variants break that enum's own doc contract. `crates/core/src/metadata.rs:609-613` says every variant is "a structural violation of the segmented chunk-map shape", raised at decode or at a site that met an unwired `Segmented` map. `VersionExhausted` (`:848`) is neither, and `ReplacementPlacementMalformed` (`:857`) is a bad argument from the caller, not an object fault. This matters for #777: every custodian consumer downcasts `ChunkMapError` to mean "this object is unreadable, contain it and keep walking" (`crates/custodian/src/reconstruction.rs:639-645`, and the same pattern at `gc.rs:1245`, `rebalance.rs:316`, `restore.rs:734`, `backfill.rs:163`). A planner bug that passes a wrong-length placement would then be filed as per-object corruption instead of surfacing as a bug. Fix inside the same file: update the enum doc to admit write-side refusals, or return the placement refusal as its own type (`MalformedPlacement` at `:460` already has the same `{expected, actual}` shape).
- [ ] In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.
- [ ] **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — `crates/core/src/metadata.rs:3194` says the flat arm leaves "`state` left as it was", but no test pins it. Every test builds a `Committed` generation (`flat_root`, patch test helper). Concrete case: change the flat arm's `InodeRecord { .. }` to add `state: InodeState::Committed` — exactly the peer idiom at `metadata.rs:2183` that the brief says to mirror — and all 16 `placement_move` tests still pass. I ran this. Under that change, a move on a `Pending` generation would publish it as `Committed`. Current callers skip non-committed records (`crates/custodian/src/reconstruction.rs:633`), so the risk is low. The fix is small: add a `Pending` flat generation to `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata` (`:4556`) and assert the state is unchanged. Or refuse a non-`Committed` generation outright.; The two new `ChunkMapError` variants break that enum's own doc contract. `crates/core/src/metadata.rs:609-613` says every variant is "a structural violation of the segmented chunk-map shape", raised at decode or at a site that met an unwired `Segmented` map. `VersionExhausted` (`:848`) is neither, and `ReplacementPlacementMalformed` (`:857`) is a bad argument from the caller, not an object fault. This matters for #777: every custodian consumer downcasts `ChunkMapError` to mean "this object is unreadable, contain it and keep walking" (`crates/custodian/src/reconstruction.rs:639-645`, and the same pattern at `gc.rs:1245`, `rebalance.rs:316`, `restore.rs:734`, `backfill.rs:163`). A planner bug that passes a wrong-length placement would then be filed as per-object corruption instead of surfacing as a bug. Fix inside the same file: update the enum doc to admit write-side refusals, or return the placement refusal as its own type (`MalformedPlacement` at `:460` already has the same `{expected, actual}` shape).. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
