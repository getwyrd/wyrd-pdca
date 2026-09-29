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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 657 of 659 instrumentable changed lines executed (floor 80%); 659 of 1082 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 76s: 27 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.93s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #776 flat/segmented placement-move primitive meets its functional brief; documentation scope and incomplete full-CI evidence still need human disposition.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Resolve the one-file scope against mandatory architecture documentation for the new public API — the brief forbids the additional document while the standing rubric requires it (`brief.md:83`, `AGENTS.md:154`, `crates/core/src/metadata.rs:3224`). |
| C2 Reproduction (red pre-fix) | N/A | This adds an absent API; stashing confirms both the symbol and its tests are absent, so there is no behavioral pre-fix red to claim; mutation testing is the declared oracle (`brief.md:70`, `reviewer-evidence/base-presence.log:4`). |
| C3 Change | PASS | The change supplies the requested atomic batch without changing callers or the read side; it fits the one-file, 170-line and 50 KB limits (`patch.diff:1`, `reviewer-evidence/shape.log:1`, `crates/core/src/metadata.rs:3241`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether supplemental passing checks discharge the host caveat or require an uninterrupted CI run — frozen CI timed out; my rerun completed workspace tests but stopped on a read-only advisory-cache lock (`gate-logs/C4-ci.log:7`, `reviewer-evidence/ci-rerun.log:3006`). |
| C5 Causal adequacy | PASS | The missing segment-write capability is supplied directly; independent negations demonstrate that the root, segment and chunk pins prevent stale writes, and ceiling/version refusals are effective (`crates/core/src/metadata.rs:3308`, `reviewer-evidence/negations-summary.log:1`). |
| T1 Structure | PASS | The primitive keeps transaction ownership with its caller and uses the existing MetadataStore seam and corruption arbiter, preserving dependency direction (`crates/core/src/metadata.rs:3117`, `crates/core/src/metadata.rs:3225`, `crates/core/src/metadata.rs:3316`). |
| T2 Shape | PASS | The implementation remains within the planned core-only slice: 144 nonblank, noncomment production additions and a 49,324-byte patch; replacement-argument faults remain distinct from object corruption (`reviewer-evidence/shape.log:2`, `crates/core/src/metadata.rs:3148`). |
| T3 Runtime | PASS | Table-directed point reads and one-record rewrites preserve bounded work; the 256-seed campaign checks sibling merges, stale moves and retirement without losing writes (`crates/core/src/metadata.rs:3264`, `crates/core/src/metadata.rs:5232`, `reviewer-evidence/green.log:199`). |
| T4 Contribution | N/A | Contribution artifacts are deliberately absent at Check; their substantive audit must rerun at publish, so the deferred row creates no human action (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Independent mutation testing found no survivors; the boundary, overflow and Pending-state regressions also reject targeted negations, and affected-path prior art was checked through merged history and closed work (`reviewer-evidence/mutants.log:5`, `reviewer-evidence/negations-summary.log:6`, `reviewer-evidence/prior-art-summary.log:51`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this as the enabling core primitive — deployed segmented repair remains incomplete until #777 wires the caller, so this child's successful tests cannot establish end-to-end recovery (`brief.md:7`, `crates/custodian/src/reconstruction.rs:1143`). |

No implementation defect was found. The remaining decisions concern the documentation requirement, the incomplete combined CI verdict, and acceptance of the explicitly limited delivery scope. Source citations above are relative to `$PDCA_TARGET`; bundle and evidence citations are relative to this review directory. The target was readable and matched the supplied patch.

The verification evidence supports the primitive's behavior:

- **Independent green and meaningful reds:** all 15 placement-move tests passed. Stashing confirmed the API's absence, not a failing behavioral test. After restoring the patch, `cargo mutants --in-place --in-diff patch.diff -p wyrd-core` reproduced **27 caught, 9 unviable, zero missed**. Eight additional negations each compiled and failed the intended assertion: root pin, segment pin, chunk equality, output ceiling, version advancement, segment-boundary selection, offset overflow and Pending-state preservation. The final workspace run passed with the original implementation restored (`reviewer-evidence/green.log:199`, `reviewer-evidence/mutants.log:5`, `reviewer-evidence/negations-summary.log:1`, `reviewer-evidence/ci-rerun.log:764`).
- **CI is incomplete for a host reason, not a demonstrated patch failure:** the frozen run timed out during custodian GC tests. My rerun passed typos, docs lint/render, formatting, workspace clippy/build, all default workspace tests and cargo-machete, including those GC tests; it then failed to lock the read-only advisory cache. Running the actual cargo-deny tool against an unchanged cache copy inside this sandbox passed the default and all-feature policies; only the database path changed, and the audit was offline. Conformance, the statics scanner and the madsim DST campaign passed separately. These results do not turn the original combined gate into a green (`gate-logs/C4-ci.log:2250`, `reviewer-evidence/ci-rerun.log:2136`, `reviewer-evidence/ci-rerun.log:3006`, `reviewer-evidence/deny-default-local-cache.log:15`, `reviewer-evidence/deny-features-local-cache.log:11`, `reviewer-evidence/deny-features-licenses.log:3`, `reviewer-evidence/conformance.log:2`, `reviewer-evidence/statics.log:4`, `reviewer-evidence/dst.log:588`).
- **Frozen evidence was read without overstating it:** C4-verify explicitly reports green-only; diff coverage reports 657/659 instrumentable changed lines executed (99.7%); host-tikv shows both feature compilations finishing; batched review reports zero blocking findings. Coverage, the TiKV feature builds and the batched-review wrapper were adjudicated from their captured logs, not claimed as independently reproduced (`gate-logs/C4-verify.log:120`, `gate-logs/C4-diff-cov.log:457`, `gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`, `gate-logs/T4-batch-review.log:10`).

The documentation conflict belongs to Plan: the patch introduces the public `repoint_chunk` API, while the brief prohibits a second changed file. The rubric requires living architecture documentation for an added API operation. Decide the scope or record the applicable interpretation before treating that convention as satisfied; code comments alone do not resolve the conflict (`brief.md:95`, `AGENTS.md:154`, `crates/core/src/metadata.rs:3224`).

The affected-path prior-art check is complete: the remote queries returned 20 commits touching `crates/core/src/metadata.rs`, 355 closed/merged PRs whose file lists were filtered for that path, and no open PRs. The relevant unmerged PR #647 was closed because it needed a smaller scope; this one-file primitive respects that disposition. No unresolved prior-art decision remains (`reviewer-evidence/prior-art-summary.log:1`, `reviewer-evidence/prior-art-summary.log:25`, `reviewer-evidence/prior-art-summary.log:48`, `reviewer-evidence/prior-art-summary.log:51`). No `INTEGRATION.md` was supplied or found in the target; the supplied standing rubric was applied.

All temporary source mutations were restored. Reverse-application checking and the final source hash confirm the reviewed target still contains exactly the supplied patch (`reviewer-evidence/restoration.log:3`).

### Advisory — adversary

# Adversarial review — issue #776 (`repoint_chunk`, seg-record placement move)

**Verdict: I tried to refute the fix and could not.** Every proof I could re-run came back
green, every hand mutation I tried went red, and the one behaviour gap I found is a
low-severity wording issue in the docs, not a wrong write. Nothing below needs a rebuild.

## Evidence re-run (scratch copy of `$PDCA_TARGET`, toolchain present)

- `crates/core/src/metadata.rs:4433` (`mod placement_move`): all 15 tests pass. `cargo mutants --in-diff`
  re-run gives the same result as the gate: 36 mutants, 27 caught, 9 unviable, **0 missed**
  (`missed.txt` is empty).
- cargo-mutants never touches the pins, the `retired_or` call, the placement assignment, or the
  segment rebuild. So I mutated those by hand (14 mutants). **All were caught:**
  - segment pin dropped at `:3308` (sibling test and seeded campaign go red)
  - root pin dropped in the segmented arm at `:3308` (superseded-root test and campaign go red)
  - root pin dropped in the flat arm at `:3259` (flat conflict test and campaign go red)
  - over-ceiling guard at `:3279` forced to `false` (over-ceiling test goes red)
  - anomaly always answered as `Conflict` at `:3316` (damaged-segment test and over-ceiling test go red)
  - `retired_or` pointed at the wrong root key (same two go red)
  - `chunk == prior` weakened to `chunk.id == prior.id` at `:3357` (5 tests go red)
  - version bump made wrapping at `:3247` (exhaustion test goes red)
  - placement never assigned, in either arm (3 and 5 tests go red)
  - segment rebuilt at offset 0 at `:3306` (4 tests go red)
  - `continue` changed to `break` at `:3302` (zero-length boundary test and campaign go red)

  That covers the brief's named negations: equality, root pin, ceiling, and unchecked version.
- The tests run through the real code paths: the real `repoint_chunk`, the production resolver for
  planning, and a real redb store. There is no parallel copy of the logic, and "nothing written" is
  checked by comparing the whole store before and after.
- Budget and scope checks all hold: 144 added non-test code lines (limit 170), `patch.diff` is
  49,324 bytes (limit 50 KB), one file, no `MAX_ROOT_VALUE_BYTES` comparison, and
  `commit_chunk_map` is untouched.

## Findings

- `crates/core/src/metadata.rs:3125-3126` (and `:3135-3136`): the doc says `Repoint::Refused` is
  "not transient — it fails every pass until the record shrinks". That is **not true for a stale
  plan**. I confirmed it with a probe:
  - Segmented: a generation whose segment row sits one byte under the ceiling's reach. Flip the
    root to a small flat generation, leave the old `seg:` row unreclaimed, and repoint to
    `[u64::MAX]`. The answer is `Ok(Refused { bytes: 100001, .. })`, not `Conflict`.
  - Flat: a stale near-ceiling generation whose object has since been overwritten small gives the
    same answer.

  `VersionExhausted` behaves the same way for a stale flat generation. No wrong write is possible,
  because nothing is committed. The cost is that #777 could raise an operator signal for an object
  that no longer has the problem, and it clears on the next pass. **Low severity. I am not flagging
  it for a rebuild:** the patch is within about 700 bytes of its 50 KB budget, and the natural fix
  belongs in the caller. #777 should re-resolve before treating `Refused` or `VersionExhausted` as
  permanent. The cheaper alternative is one caveat line in this doc.
- `check-gates.json` C4-ci row (`gate-logs/C4-ci.log`): the row is `unverifiable` (the gate hit
  its 7200 s timeout while `tests/custodian_gc.rs` hung), yet `overall` is `pass`. I closed that
  gap myself:
  - `cargo test -p wyrd-server --test custodian_gc`: 10/10 pass in 0.17 s.
  - `cargo test --workspace --exclude wyrd-dst`: all green.
  - `cargo xtask statics`, `cargo xtask conformance` and `cargo xtask dst` (madsim clippy and tests): all green.
  - I did not re-run `cargo deny` or `cargo machete`. The diff changes no manifest, so they cannot move.

  The hang is a host problem, not this patch.
- `gate-logs/T4-batch-review.log`: the "3x codex" review finished in 45 s with 0 findings. The
  previous round raised 8 blocking findings. The log is a single summary line, so it does not show
  the passes actually covered this diff. This is weak evidence, but it does not refute the fix,
  because my own attempts above found nothing blocking.

## Attempted and could not refute

- **Zero-length chunk at a segment boundary** (`:3338`, `:3351`): only the two segments touching
  the offset can hold it, because segments are never empty. Equality picks between them, and a
  chunk with bytes never reads the neighbouring segment.
- **Over-ceiling segment rows**: a row at exactly V (`MAX_VALUE_BYTES`, the 100,000-byte value
  limit) is admitted, and a row at V + 1 is typed corruption or a `Conflict`, never rewritten.
- **Retirement, deletion and reclaim races**: covered by `:5069` (the seeded campaign), which
  checks against a model that doesn't know about the pins. All four race tallies come out non-zero.
- **A-B-A on the segment byte pin** (the bytes change and then change back): harmless. The batch
  is built entirely from the pinned bytes, so if the bytes match again, the result is identical to
  a fresh prepare.
- **Root pin is `encode(generation)`, not the raw bytes**: this is the same idiom
  `commit_chunk_map` uses, which the brief says to mirror. It is not new debt.
- **A malformed committed `prior` placement is accepted**: ADR-0040 decision 4 makes that the
  maintenance loop's job (#777), and decision 5 (write a full-length vector) is enforced here by
  `MalformedReplacement`.

### Advisory — code-review

- No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in `crates/core/src/metadata.rs:3224` and its tests at `crates/core/src/metadata.rs:4433`. Checked the root/segment/chunk pins, sibling-edit preservation, retirement conflicts, malformed input handling, value ceilings, checked version advancement, and zero-length boundary addressing.

Validation used the frozen gate evidence; no tests were rerun and the target was not modified. All 15 placement-move tests passed, including the seeded race campaign. C5 reports 27 caught mutants, 9 unviable, none missed; diff coverage is 99.7%. Full C4-ci remains unverifiable: it timed out after 7200 seconds during custodian GC tests. This is a validation limitation, not an attributed patch defect.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Resolve the one-file scope against mandatory architecture documentation for the new public API — the brief forbids the additional document while the standing rubric requires it (`brief.md:83`, `AGENTS.md:154`, `crates/core/src/metadata.rs:3224`).
- [ ] C4 Verification (red→green) — Decide whether supplemental passing checks discharge the host caveat or require an uninterrupted CI run — frozen CI timed out; my rerun completed workspace tests but stopped on a read-only advisory-cache lock (`gate-logs/C4-ci.log:7`, `reviewer-evidence/ci-rerun.log:3006`).
- [ ] Validation — fitness-to-purpose — Accept this as the enabling core primitive — deployed segmented repair remains incomplete until #777 wires the caller, so this child's successful tests cannot establish end-to-end recovery (`brief.md:7`, `crates/custodian/src/reconstruction.rs:1143`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [ ] **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.
- [ ] In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.
- [ ] T5 Judgment — Confirm acceptance of Plan’s affected-path merged and closed/rejected-work check — the disposable target contains only a synthesized base and no remote, so that history cannot be independently corroborated here (`brief.md:133`, `reviewer-evidence/prior-art.log:4`, `reviewer-evidence/prior-art.log:7`).
- [ ] **`Repoint::Refused` can be returned for a generation the root has already left, but its doc says it is not a race.** The doc at `crates/core/src/metadata.rs:3125` says Refused "is not transient … an operator signal rather than a retry". The segmented arm weighs the rewritten record (`:3307` → `weighed`, `:3323`) without checking whether the root still names the generation. Only faults go through `retired_or` (`:3316`). Probe: seed a one-segment root whose row the move pushes 1 byte over `MAX_VALUE_BYTES`, then overwrite the root with a flat successor (the old `seg:` row is not yet reclaimed), then call `repoint_chunk` with the old generation. Result: `Ok(Refused { bytes: 100001, ceiling: 100000 })`, not `Conflict`. A #777 caller that follows the doc would raise an operator alert for an object that is fine. `VersionExhausted` (`:3137`) has the same shape, because the flat arm never reads the store. It is rare: it needs a non-conforming row within ~19 bytes of V plus a race with retirement. There are two fixes: spend one root read (`root_dropped`) before returning `Refused`, or change the doc to say the caller must confirm the generation is still live before escalating. Choosing between them is a judgment call.
- [ ] external dependency.

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide whether supplemental passing checks discharge the host caveat or require an uninterrupted CI run — frozen CI timed out; my rerun completed workspace tests but stopped on a read-only advisory-cache lock (`gate-logs/C4-ci.log:7`, `reviewer-evidence/ci-rerun.log:3006`).. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
