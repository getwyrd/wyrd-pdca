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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 657 of 659 instrumentable changed lines executed (floor 80%); 659 of 1090 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 76s: 27 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.01s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #776: add a placement-move primitive that returns an atomic CAS batch for flat or segmented chunk records, enabling the later custodian integration in #777.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The enabling API has explicit addressing, concurrency, corruption, and full-value-ceiling criteria; operational repair belongs to #777 (`brief.md:9`, `brief.md:117`). |
| C2 Reproduction (red pre-fix) | N/A | This is an absent API, not an existing behavioral regression; stash/restore independently confirmed absence/presence without claiming a behavioral red (`brief.md:70`, `reviewer-evidence/base-probe.log:5`). |
| C3 Change | PASS | The requested batch can preserve publication metadata and reject stale generations without committing independently; existing callers and read paths remain untouched (`crates/core/src/metadata.rs:3249`, `crates/core/src/metadata.rs:3262`, `crates/core/src/metadata.rs:3315`). |
| C4 Verification (red→green) | PASS | Full CI and 289 core tests passed independently; five deliberate faults produced assertion failures followed by restored green, consistent with the declared mutation-based verification posture (`reviewer-evidence/ci-local-cache.log:3724`, `reviewer-evidence/negations.log:298`, `brief.md:104`). |
| C5 Causal adequacy | PASS | The missing write primitive is supplied directly, without a capability fallback; removing either CAS pin or chunk equality demonstrably breaks stale-write protection (`crates/core/src/metadata.rs:3232`, `reviewer-evidence/negations.log:184`, `reviewer-evidence/negations.log:207`, `reviewer-evidence/negations.log:232`). |
| T1 Structure | PASS | Production code stays within the existing MetadataStore seam and shares the resolver's corruption/retirement decision, preserving dependency direction and error boundaries (`crates/core/src/metadata.rs:3233`, `crates/core/src/metadata.rs:3324`). |
| T2 Shape | PASS | One changed file, 49,947 patch bytes, and 144 added production lines even counting delimiters satisfy the scope and size limits (`brief.md:83`, `reviewer-evidence/scope.log:1`). |
| T3 Runtime | PASS | Segment work is limited to candidate records from the validated table, without a range walk or segmented-root clone; oversize input and output fail closed (`crates/core/src/metadata.rs:3271`, `crates/core/src/metadata.rs:3287`, `crates/core/src/metadata.rs:3331`, `crates/core/src/metadata.rs:3346`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive audit is owed to the mandatory publish-time rerun (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The 256-seed campaign exercises competing moves and retirement; affected-path prior art was checked, and the architecture-doc deferral to #777 is settled under the standing protocol (`crates/core/src/metadata.rs:5240`, `reviewer-evidence/prior-art.log:23`, `crates/core/src/metadata.rs:3229`, `AGENTS.md:200`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm this batch API is fit for #777's combined placement/evidence transaction — this child's evidence establishes primitive safety, while end-to-end custodian repair is outside its agreed scope (`brief.md:11`, `brief.md:117`). |

No implementation defect remains identified in this review. The only decision left for human sign-off is fitness-to-purpose. Source citations above resolve under `$PDCA_TARGET`; evidence citations resolve in this bundle.

- Independent verification passed: `cargo test --offline --locked -p wyrd-core` ran 289 tests; `cargo xtask ci` completed its spelling, docs, formatting, clippy, build, workspace tests, dependency audits, conformance, statics, and DST checks. Both requested TiKV feature clippy commands also passed (`reviewer-evidence/scope.log:4`, `reviewer-evidence/ci-local-cache.log:3724`, `reviewer-evidence/tikv-rerun.log:102`, `reviewer-evidence/tikv-rerun.log:202`). The first CI attempt stopped at the read-only advisory-cache lock; rerunning with a writable copy of the real advisory database inside this sandbox completed successfully. No tool was replaced by a shim.
- Causal evidence passed independently: cargo-mutants reproduced 36 mutants, 27 caught and 9 unviable, with none missed (`reviewer-evidence/mutants-rerun.log:40`). Additional fault injections in a separate source copy removed chunk equality, the root pin, the segment pin, and the output ceiling, and replaced checked version advancement with wrapping advancement. Each compiled and failed a test assertion; restoring the source passed all 15 placement tests (`reviewer-evidence/negations.log:1`, `reviewer-evidence/negations.log:199`, `reviewer-evidence/negations.log:224`, `reviewer-evidence/negations.log:249`, `reviewer-evidence/negations.log:274`, `reviewer-evidence/negations.log:321`).
- Frozen evidence was read at its actual strength: C4-verify explicitly reports green-only, not pre-fix red; diff coverage reports 657/659 instrumentable changed lines, includes co-located tests, and excludes 431 unscored lines. The frozen batch review reports zero blocking findings. Instance-scoped wrappers were not treated as missing target tools (`gate-logs/C4-verify.log:120`, `gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:439`, `gate-logs/C4-diff-cov.log:454`, `gate-logs/T4-batch-review.log:10`).
- Prior art was checked by `crates/core/src/metadata.rs`: 20 default-branch commits and the file lists of all 19 closed/unmerged PRs. The sole closed/unmerged path match was [PR #647](https://github.com/getwyrd/wyrd/pull/647), whose closure requested smaller slices; this patch follows that direction (`reviewer-evidence/prior-art.log:2`, `reviewer-evidence/prior-art.log:23`, `reviewer-evidence/prior-art.log:26`). The target was readable and matched the supplied patch; stash/restore preserved it byte-for-byte, and the final diff SHA-256 equals `patch.diff` (`reviewer-evidence/base-probe.log:28`).

### Advisory — adversary

# Adversarial review — issue #776 (`repoint_chunk`, round 6)

Verdict: **I could not break the production code.** The one thing I would raise is a
test gap: two hand-made mutants survive, both on properties the brief names. The code
itself behaves correctly on both inputs.

## Finding

- NEEDS-HUMAN [human] — **Two brief-named properties have no test that goes red, and C5's
  "27 caught, 0 missed" hides this.** cargo-mutants cannot generate either mutant below, so
  reading C5 as "every pin is pinned" is not warranted. I applied each mutant by hand in a
  scratch copy and all 15 `placement_move` tests stayed green:
  1. `crates/core/src/metadata.rs:3241`: change `usize::from(expected)` to
     `prior.placement.len()`. Every test prior carries a full-length placement, including
     the campaign's (`:5092`). So a check against the fragment count and a check against
     the old vector's length look the same to the tests. Failing case under the mutant: a
     pre-M3 RS(2,1) chunk with `placement: []`. The mutant **accepts** an empty replacement
     and **refuses** a correct `[20, 21, 22]` one. The patch's own doc (`:3156`) names
     exactly this empty-vector case. `a_replacement_placement_must_name_every_fragment`
     (`:4934`) never moves such a chunk.
  2. `crates/core/src/metadata.rs:3316`: pin `encode(&record)` (the re-encoded record)
     instead of `bytes` (the bytes actually read). Brief item 3 and the doc at `:3197` say
     the segment pin is "the segment record's own bytes as read here". Failing case under
     the mutant: a stored segment row that decodes but is not in `encode`'s spelling (I
     used the same fields in a different order). Every move on that row would lose its
     CAS, pass after pass: the "refused forever" C-1 class this lineage exists to remove.
  I added one probe test for each case. Both pass on the patch and each fails under its
  mutant (`probe_legacy_empty_placement_prior`,
  `probe_noncanonical_segment_row_is_pinned_by_its_own_bytes`), so the production code is
  right and only the tests are weak. I tagged this `[human]`, not `[impl]`, because it is
  a close call. No writer in the tree produces either input today. This is round 6, and
  the rubric says not to chase silence. `patch.diff` is 49,947 bytes against a ≤ 50 KB
  budget, so adding the two probes (~1.2 KB) means trimming something else. If you want
  it fixed, it is a small, mechanical `[impl]` rebuild.

## Attempted and could not refute

- **Red→green.** C4-verify is green-only, as the brief declares (in-crate tests only).
  That is not a refutation. I re-ran the 15 tests green on a copy of `$PDCA_TARGET`. Then
  I checked the brief's four named negations by hand, and each goes red: drop
  `chunk == prior` (`:3365` in `chunk_at`); drop the root pin; bypass the ceiling in
  `weighed`; replace `checked_add` with `wrapping_add`. The C4-ci log shows all 15 tests
  and "all checks passed", with no timeout this round. The two lines diff-cov missed
  (`:4495`, `:5200`) are `panic!` arms inside tests.
- **Wrong-record writes.** Zero-length chunks on a segment boundary: equality picks the
  record, and a mismatch in both candidates is a `Conflict`. A chunk with bytes that
  starts a segment never reads the previous segment: the garbage-neighbour test at offset
  5 kills the iteration-3 mutant.
- **Over-ceiling and damaged rows.** A row is weighed before it is decoded
  (`:3287`, the same `>` as `read_group_range`). Live versus retired goes through
  `retired_or`. Exactly-V is admitted in both directions.
- **Flat arm.** `state` and ADR-0047 metadata are kept (the test uses a `Pending` root).
  The version advance is checked. The root pin fails a superseded batch and leaves the
  store byte-identical.
- **Seeded campaign.** It uses the real redb store and the production resolver. Its model
  is blind to the pins, and it asserts that all four race classes ran, so it cannot pass
  with nothing tested. The prepare runs as one step, but no read inside it can be
  interleaved in a way the pins do not already cover.
- **Considered and not raised.** (a) Mutant M7, `continue` instead of `Conflict` after a
  *retired* anomaly (`:3324`), survives. It is nearly equivalent: it only changes the
  answer for a generation that is already retired, and the root pin dooms that batch
  anyway. (b) No `MAX_ROOT_SEGMENTS` or extra-row check: the brief bans a range walk, and
  that constant's doc limits the guard to publication and the ranged read. (c) Docs
  currency carries `deferred: #777`, which the protocol treats as settled. (d) The
  `:2493` citation in the `flat_value_ceiling_crossed` doc is stale, but it was already
  stale on the base.
- **Scope.** One file. 144 semantic non-test lines (≤ 170). No `MAX_ROOT_VALUE_BYTES`
  comparison. `commit_chunk_map` is untouched.

### Advisory — code-review

- Correctness: no findings in this diff. The placement move preserves the root and segment CAS pins, checks replacement length and version exhaustion, and handles malformed records and boundary chunks consistently (`crates/core/src/metadata.rs:3232`, `crates/core/src/metadata.rs:3346`, `crates/core/src/metadata.rs:3359`).
- Reuse / simplification / efficiency: no actionable findings. The implementation reuses the ceiling check and retirement arbiter, reads only candidate segments, and consumes the decoded segment without cloning its chunks (`crates/core/src/metadata.rs:3272`, `crates/core/src/metadata.rs:3312`, `crates/core/src/metadata.rs:3331`).

Validation: all patch hunks match the read-only target. Frozen evidence records passing CI, including the seeded race campaign, and 36 mutants tested: 27 caught, 9 unviable, none missed. C4-verify is green-only as declared in the brief. Tests were not rerun during this advisory review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Confirm this batch API is fit for #777's combined placement/evidence transaction — this child's evidence establishes primitive safety, while end-to-end custodian repair is outside its agreed scope (`brief.md:11`, `brief.md:117`).
- [x] **Two brief-named properties have no test that goes red, and C5's "27 caught, 0 missed" hides this.** cargo-mutants cannot generate either mutant below, so reading C5 as "every pin is pinned" is not warranted. I applied each mutant by hand in a scratch copy and all 15 `placement_move` tests stayed green:
- [x] **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.
- [x] In the flat arm, `Repoint::Conflict` can be returned for a mismatch that no retry can fix. At `crates/core/src/metadata.rs:3234-3236` the flat arm reads nothing from the store, so a `chunk_at` miss is a pure function of the caller's own `(generation, byte_offset, prior)`. It is a caller inconsistency, not a race. The test at `:4600-4603` asserts `Conflict` for `repoint(&store, &root, 0, &b(), ..)` against root `[a, b]`, a call no store state can ever make succeed. Yet the `Conflict` doc (`Repoint::Conflict`, patch `:93-97`) tells callers to "keep its obligation and re-plan next pass". A planner with a deterministic offset bug would then retry forever in silence — the "refused every pass, forever" shape this lineage exists to remove. The brief's item 7 allows "conflict/`Blocked`", so this is allowed as written. The open question for a human is whether #777 needs a distinct non-transient outcome (for example `Blocked`, or an error) for the flat-arm miss before it wires this in.
- [x] T5 Judgment — Confirm acceptance of Plan’s affected-path merged and closed/rejected-work check — the disposable target contains only a synthesized base and no remote, so that history cannot be independently corroborated here (`brief.md:133`, `reviewer-evidence/prior-art.log:4`, `reviewer-evidence/prior-art.log:7`).
- [x] **`Repoint::Refused` can be returned for a generation the root has already left, but its doc says it is not a race.** The doc at `crates/core/src/metadata.rs:3125` says Refused "is not transient … an operator signal rather than a retry". The segmented arm weighs the rewritten record (`:3307` → `weighed`, `:3323`) without checking whether the root still names the generation. Only faults go through `retired_or` (`:3316`). Probe: seed a one-segment root whose row the move pushes 1 byte over `MAX_VALUE_BYTES`, then overwrite the root with a flat successor (the old `seg:` row is not yet reclaimed), then call `repoint_chunk` with the old generation. Result: `Ok(Refused { bytes: 100001, ceiling: 100000 })`, not `Conflict`. A #777 caller that follows the doc would raise an operator alert for an object that is fine. `VersionExhausted` (`:3137`) has the same shape, because the flat arm never reads the store. It is rare: it needs a non-conforming row within ~19 bytes of V plus a race with retirement. There are two fixes: spend one root read (`root_dropped`) before returning `Refused`, or change the doc to say the caller must confirm the generation is still live before escalating. Choosing between them is a judgment call.
- [x] external dependency.
- [x] C1 Spec — Resolve the one-file scope against mandatory architecture documentation for the new public API — the brief forbids the additional document while the standing rubric requires it (`brief.md:83`, `AGENTS.md:154`, `crates/core/src/metadata.rs:3224`).
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [x] C1 Spec — Resolve the one-file scope against the same-PR architecture requirement — satisfying both is impossible without a Plan amendment or an explicit policy exception (`brief.md:83`, `brief.md:95`, `AGENTS.md:154`).
- [x] **The one gating red (T4 "Docs currency" at `crates/core/src/metadata.rs:3228`) is arguable, and it clashes with the brief's one-file scope.** The rubric's trigger list is "a port, an API operation, an RPC, a CLI flag, or a persisted field". `repoint_chunk` is none of these. It is an in-crate library function with no callers, no wire or persisted change, and no trait seam. The living architecture docs don't track functions at this level either: `resolve_chunk_map` and `commit_chunk_map` appear nowhere under `docs/design/architecture/`. The behavioural claim at `docs/design/architecture/08-crosscutting-concepts.md:85` ("the maintenance loops that … move them … treat a shape they cannot resolve as a typed error") stays true until #777 wires the primitive in. One sentence in that same paragraph does go stale in spirit: "an object whose root can no longer be re-written is an object whose placement can never be repaired". This primitive repairs a segmented object's placement without re-writing the root (`metadata.rs:3312` pins the root but never `put`s it). The human has to choose: (a) reject the T4 finding and record the reason (the doc update ships with #777, where behaviour changes), or (b) widen this child to two files. The brief says a second file means "STOP and hand back", so (b) is a scope change, not a rebuild.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Harness bug (upstream, eduralph/pdca-harness): auto-iterate carries `deferred-findings.json` into every later §6 without re-checking against the current round's gates (issue_776: round-4 "C4 … exceeded its 7200s timeout" still listed after C4 passed in v5 and v6), and it parses the text "NEEDS-HUMAN" inside prose as an item (`iteration-v2/build-notes.md:243` "No NEEDS-HUMAN external dependency." → §6 item "external dependency.").
- Harness bug (upstream, eduralph/pdca-harness): deferred NEEDS-HUMAN items are never de-duplicated — the same question raised in different words by different reviewers/rounds lands in §6 as separate items (issue_776: §6 items 8, 10, 11 are all "one-file scope vs. architecture-doc update", already settled once in `review-rejected.md`).
- Leaf improvement (sign-off/review): present §6 as a standard triage table — `# | item (short) | kind (judgment / scope / contract design / test gap / process / stale / noise / duplicate-of-N) | implementation issue? (yes / maybe + what the fix is / no + why)` — so the accept-vs-iterate choice is visible at a glance (human asked for this on issue_776).
- Carry to #777 (from issue_776 §6 item 3): the in-crate seeded `wyrd_testkit::Sim` campaign was accepted for #776; #777 must land seeded Tier-0 DST coverage in `crates/dst` for the custodian repair move racing a concurrent move and the root's supersede/retirement, when it wires `repoint_chunk` in (`AGENTS.md:188-190`).
- Carry to #777 (from issue_776 §6 item 4): `repoint_chunk`'s flat arm returns `Repoint::Conflict` for a `chunk_at` miss that is a caller inconsistency, not a race (no store read; test `patch.diff:485-488`), while the `Conflict` doc says "re-plan next pass". When #777 wires it in, make that miss visible — a caller error like `MalformedReplacement`, or flag a `Conflict` that repeats on an unchanged generation — so a planner offset bug cannot retry forever in silence.
- Carry to #777 (from issue_776 §6 item 6): `Repoint::Refused` and `Repoint::VersionExhausted` are judged on the caller's planned generation alone (no root pin on a refusal; doc `patch.diff:46-50`, `:59-61`). #777's caller must re-resolve and confirm the generation is still current before escalating either to an operator; on a superseded generation it should just re-plan.
- Carry to #777 (from issue_776 §6 item 2): add the two regression tests the adversary wrote as probes — `probe_legacy_empty_placement_prior` (placement length checked against `fragment_count`, not the old placement's length; pre-M3 empty-placement chunk) and `probe_noncanonical_segment_row_is_pinned_by_its_own_bytes` (segment pin = bytes read, not re-encoded). Both land in `crates/core/src/metadata.rs`, so #777's brief must allow that file (or split them into a small follow-up).
- Before #777 is built, re-open its Plan: fold in the five items carried from #776 (seeded DST coverage in `crates/dst`; visible caller-bug `Conflict`; re-check before escalating `Refused`/`VersionExhausted`; the two `metadata.rs` regression tests; the architecture-doc update) and widen its 3-file / 85 KB budget (`issue_777/brief.md:124-127`) or split it.
- Harness gap (upstream, eduralph/pdca-harness): a sign-off can carry work forward to another issue ("carry to #N" notes), but nothing picks it up — the planner for #N should check other bundles for items passed forward to it and fold them in (or flag them) during Plan, so carried work cannot be silently dropped or land on an already-frozen brief.
