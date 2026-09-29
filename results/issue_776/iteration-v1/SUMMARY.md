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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.3% — 444 of 447 instrumentable changed lines executed (floor 80%); 447 of 733 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 30 mutants tested in 67s: 21 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.21s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #776’s caller-committed placement move for flat inode and segmented chunk maps: two reproduced defects and a required concurrency-coverage gap remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The primitive-only scope, three pins, full 100,000-byte ceiling and deferred caller integration are explicit and testable; the settled ceiling needs no reconsideration (`brief.md:9`, `brief.md:35`, `brief.md:117`). |
| C2 Reproduction (red pre-fix) | N/A | This adds an absent API: stashing removes both the symbol and its co-located tests, so the base runs zero matching tests rather than a behavioral RED (`brief.md:80`, `reviewer-evidence/base-green.log:1`). |
| C3 Change | PASS | The requested composable batch is present in the agreed file, preserves the segmented root and leaves caller integration untouched; the implementation safety gaps are assessed below (`crates/core/src/metadata.rs:3195`, `crates/core/src/metadata.rs:3264`, `reviewer-evidence/grounding.txt:7`). |
| C4 Verification (red→green) | PASS | The declared green-only posture is reproduced: 13 tests pass, all four named negative controls fail at runtime, restoration passes, and mutation testing reproduces 21 caught/9 unviable/0 missed; full frozen CI is green, with independent rerun limits detailed below (`reviewer-evidence/negations-restored-green.log:21`, `gate-logs/C4-ci.log:3731`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Reject malformed replacement placements and classify oversized live segment input before preparing a batch—both omissions let the primitive commit inputs that existing maintenance/read contracts reject (`crates/core/src/metadata.rs:3218`, `crates/core/src/metadata.rs:3243`, `reviewer-evidence/probe.log:17`). |
| T1 Structure | PASS | Production code stays behind the narrow `MetadataStore` seam; the concrete redb dependency is confined to tests, and the caller retains atomic composition of placement and evidence (`crates/core/src/metadata.rs:3196`, `crates/core/src/metadata.rs:3132`, `crates/core/src/metadata.rs:4396`). |
| T2 Shape | PASS | The patch stays within the agreed budget: one file, 32,527 bytes and 122 added nonblank/noncomment production lines; the read side and existing `commit_chunk_map` are unchanged (`reviewer-evidence/grounding.txt:7`, `patch.diff:43`). |
| T3 Runtime | FAIL | The promised bounded segment handling lacks an input-size check: an arbitrarily large stored value reaches decoding before the output ceiling is considered (`crates/core/src/metadata.rs:3243`, `crates/core/src/metadata.rs:3281`; compare the read boundary at `crates/core/src/metadata.rs:2888`). |
| T4 Contribution | FAIL | The frozen batch review's eight entries reduce to three grounded, unresolved classes; the required fixed-or-rejected disposition is incomplete, while contribution drafts themselves are N/A until their publish-time audit (`gate-logs/T4-batch-review.log:10`, `AGENTS.md:206`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add seeded Tier-0 coverage for move/move and move/retirement races—the new tests hand-sequence redb operations and the existing DST suite never calls this primitive, leaving the standing concurrency requirement unmet (`crates/core/src/metadata.rs:4390`, `crates/core/src/metadata.rs:4618`, `AGENTS.md:188`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm acceptance of the primitive-only increment after the implementation findings are resolved—live segmented repair remains refused until the explicitly separate #777 integration ships (`brief.md:2`, `brief.md:117`). |

Source citations are relative to `$PDCA_TARGET`; evidence citations are relative to this review directory. The target's post-image is `b525d8bb735d30e7ee9b17ecd973cfc123abf34c`, matching the patch. Stash/pop restored the supplied patch, and no implementation fix was made (`reviewer-evidence/grounding.txt:1`).

The three findings require correction or a recorded disposition:

1. **Malformed replacements can become durable metadata.** With a valid `ReedSolomon { k: 4, m: 2 }` chunk, passing `vec![99]` returns `Prepared` and commits in both arms. Reading the result reports six expected fragments, one placement entry and `placement_is_valid=false`. Readers identity-fill the missing tail, while maintenance treats it as malformed. Validate the replacement against the existing placement contract before returning a batch and cover refusal without writes. The relevant assignments are `crates/core/src/metadata.rs:3218` and `crates/core/src/metadata.rs:3262`; the existing contract is at `crates/core/src/metadata.rs:413` and `AGENTS.md:146`. Executed evidence: `reviewer-evidence/probe.log:17` and `reviewer-evidence/probe.log:18`.

2. **Oversized live segment input bypasses structural-corruption handling.** Starting with a valid plan, replace its segment with the same valid JSON padded to 100,001 bytes. `resolve_chunk_map` returns `SegmentValueOverCeiling`, but `repoint_chunk` returns `Prepared`; committing silently rewrites it to 130 bytes. The missing check also permits decoding allocations beyond the claimed record bound. Check incoming bytes against the existing full `MAX_VALUE_BYTES` and route the anomaly through `retired_or`, preserving retirement-as-conflict behavior. This concerns input above 100,000 bytes, not the settled admissibility of records between V/2 and V. Source: `crates/core/src/metadata.rs:3243`, `crates/core/src/metadata.rs:2888`, `crates/core/src/metadata.rs:2780`. Executed evidence: `reviewer-evidence/probe.log:19` and `reviewer-evidence/probe.log:20`.

3. **Seeded concurrency coverage is absent.** The 13 added tests use `pollster` and manually ordered operations on in-memory redb; none exercises the primitive under a seeded scheduler. Searching `crates/dst` and `crates/testkit` found no `repoint_chunk` or `placement_move` references. Existing DST passing therefore does not discharge `AGENTS.md:188` for this new concurrent operation. Cover overlapping moves and root retirement with the real batch preconditions; caller wiring deferred to #777 does not explicitly defer this coverage requirement. Source: `crates/core/src/metadata.rs:4396`, `crates/core/src/metadata.rs:4430`, `crates/core/src/metadata.rs:4659`; investigation: `reviewer-evidence/prior-art.txt:29`.

Verification supports the intended successful cases, but does not cover those omissions:

- Stash/base/pop verified API absence, zero matching base tests, and 13 passing patched tests. Separate negative controls removed chunk equality, the root pin, the output ceiling and checked version advancement; each compiled and failed its relevant runtime assertion, followed by 13 passing restored tests (`reviewer-evidence/negation-chunk-equality.log:22`, `reviewer-evidence/negation-root-pin.log:24`, `reviewer-evidence/negation-ceiling.log:24`, `reviewer-evidence/negation-checked-version.log:22`, `reviewer-evidence/negations-restored-green.log:21`). These are mutation REDs, not a claimed pre-fix behavioral RED.
- Independent `cargo mutants --in-diff patch.diff -p wyrd-core` reproduced the frozen result: 30 tested, 21 caught, nine unviable, zero missed. The unviable cases are not counted as detected behavioral faults (`reviewer-evidence/mutants-rerun.log:35`, `gate-logs/C5-mutants.log:13`). No capability-probe/symptom guard was introduced.
- Independent CI passed typos, docs lint/render, hygiene, formatting, workspace clippy/build/tests and cargo-machete. Its aggregate run stopped because cargo-deny could not lock the read-only host advisory cache (`reviewer-evidence/ci-rerun.log:3003`). All three deny checks subsequently passed using a writable copy of that cache and otherwise unchanged policies, offline; conformance, statics, existing seeded DST and both TiKV feature compilations also passed (`reviewer-evidence/remaining-ci.log:15`, `reviewer-evidence/deny-all-features-rerun.log:10`, `reviewer-evidence/remaining-ci.log:634`, `reviewer-evidence/tikv-rerun.log:206`). The cache-location workaround is a host caveat, not a patch defect or a simulated scanner. All external tools named in the brief were actually exercised.
- The instance-scoped coverage wrapper is adjudicated from its frozen log: 444/447 instrumentable changed lines, 99.3%; the frozen verify wrapper explicitly says green-only. The complete frozen CI log also records the deployment guard, which is internal to CI and has no standalone `xtask deploy-guard` command (`gate-logs/C4-diff-cov.log:455`, `gate-logs/C4-verify.log:120`, `gate-logs/C4-ci.log:3144`). The frozen batch-review failure is substantiated by the three findings above. `T4-contribution` is **N/A**: its subject is intentionally drafted later and its substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).

Prior art was independently checked by `crates/core/src/metadata.rs`: 20 path-filtered commits and all 355 available PRs were examined; 21 PRs touched the path and none was open. The merged prerequisites are present. Closed PR #647 was rejected for excessive scope, consistent with this one-file split; no separate rejection of this primitive's approach was found. This check is settled rather than handed back as an investigation (`reviewer-evidence/prior-art.txt:1`, `reviewer-evidence/closed-pr-647.json:1`).

### Advisory — adversary

# Adversarial review — issue #776 (`repoint_chunk` placement-move primitive)

Probes were run on a scratch copy of `$PDCA_TARGET` (patch applied) with
`cargo test -p wyrd-core --lib placement_move`. Line numbers are in the target's
`crates/core/src/metadata.rs`.

## Findings

- NEEDS-HUMAN [impl] — **A live `seg:` row over the full ceiling is rewritten, not reported as corruption.** `crates/core/src/metadata.rs:3237-3243` reads and decodes the segment without the `value.len() > MAX_VALUE_BYTES` check that `read_group_range` applies (`metadata.rs:2888-2895`). Concrete case, run as a probe: seed a live one-segment root whose segment row is 100_100 bytes. The lead chunk is RS(10,4) with placement `[u64::MAX; 14]`. `resolve_chunk_map` on that same generation returns `Err` ("segment 0's record is 100100 bytes, over the 100000-byte value ceiling"). `repoint_chunk(.., vec![0; 14])` returns `Prepared`, and the commit rewrites the row down to 99_834 bytes. The read side calls this row structural corruption, while the write side quietly "repairs" it. This goes against brief item 6 ("never hiding a persistently damaged record") and the rubric's *Protocol input* class (oversize input is never silently accepted). The `# Anomalies` doc at `:3184` also leaves this shape out. It is reachable only when the row changed after the caller's resolve, or when a caller plans without resolving, so severity is moderate. The fix is small: map an over-ceiling `bytes` to `ChunkMapError::SegmentValueOverCeiling` and send it through `retired_or` at `:3273`, as the resolver does. Add the test described here. (This is the same issue as the T4 BUG row, now confirmed with a probe.)

- NEEDS-HUMAN [impl] — **The replacement placement is never checked against the chunk's fragment count, in either arm.** `crates/core/src/metadata.rs:3201` takes `placement: Vec<DServerId>` and writes it straight in at `:3218` (flat) and `:3262` (segmented). Concrete cases, both committed in probes:
  - Flat arm: an RS(2,1) chunk with placement `[1,2,3]`, moved to `vec![9]`. The result is `Committed`, and the stored chunk now fails `placement_is_valid()` and `checked_fragments()`. Every maintenance loop will now flag it as Malformed (`reconstruction.rs:832-834`).
  - Segmented arm: the same chunk moved to `vec![]`. This commits an *empty* placement. That counts as "valid" (pre-M3 identity fallback), so reads now resolve its fragments to D servers 0, 1 and 2, where the fragments are not. The move silently points the chunk at the wrong servers.

  The rubric's hard convention reads "contextual checks (e.g. placement length) are … strict in maintenance paths", and this function *is* the placement maintenance write. Refuse anything other than `placement.len() == prior.fragment_count()` with a typed error before building the batch. (This is the T4 CONVENTION row, confirmed. The peer `commit_chunk_map` doesn't check either, but that is no reason to ship a new write path without the check.)

- NEEDS-HUMAN [impl] — **The overflow guard in `chunk_at` is untested, and the test that claims to cover it passes for a different reason.** At `crates/core/src/metadata.rs:4887-4889` ("Lengths that cannot be summed are no address at all"), `chunk_at(&huge, 0, &c())` returns `None` through the `break` at `:3311`, not through `checked_add(..)?` at `:3317`. Hand mutation: replacing `:3317` with `at = at.saturating_add(chunk.len);` leaves all 13 original `placement_move` tests green. Under that mutant, `chunk_at(&huge, u64::MAX, &c())` returns `Some(2)`, the wrong chunk, which my probe shows. Low impact, because decoded records check their chunk sums. Still, it is a mutant that survives and that `cargo mutants` does not generate. Fix: assert at `byte_offset = u64::MAX`, or correct the comment.

- NEEDS-HUMAN [human] — **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.

## Attempted refutations that did not land

- **The brief's named negations are real, not just asserted.** Hand mutations on the scratch copy:
  - Dropping `&& chunk == prior` (`:3314`, with `prior` kept referenced) turns 4 original tests red: the same-chunk conflict, the zero-length boundary, the flat-arm conflict, and `chunk_at`.
  - Dropping the root pin in both arms (`:3204`) turns 2 red.
  - Dropping only the segmented arm's root pin turns `a_superseded_root_fails_the_batch_or_the_move` red.
  - Dropping the segment-bytes pin turns the sibling/after-read test red.
  - Skipping the `retired_or` arbiter (`:3273`) turns the damaged-segment test red.

  So the tests exercise the real code path and are not tautological.
- **The C5 claim of "0 missed" holds, but it is weak on its own.** Of the 30 mutants, 9 were unviable, and `cargo mutants` does not delete conditions. My hand mutations fill that gap. Only the overflow guard above survived.
- **C4-verify "pass" is green-only, as the brief declared.** The gate log's last lines show `PASS (green-only)`, so there is no hidden red→green claim to attack.
- **Zero-length chunk on a segment boundary.** At most two segments can touch one offset, because `SegmentedMap::new` refuses an empty segment (`:1137`). Equality picks the record, and a miss in the first candidate falls through to the second. I found no wrong-record write.
- **The ceiling boundary.** Both arms go through `flat_value_ceiling_crossed` (`:605`), and the diff has no `MAX_ROOT_VALUE_BYTES` comparison. The segmented test pins both V+1 (refused) and exactly V (admitted), as the human's 2026-08-19 decision requires.
- **Concurrency.** Two moves in different segments of one root both land correctly, since each pins the unchanged root plus its own segment. Two moves in the same segment: the second loses the CAS (compare-and-swap) cleanly. A supersede between prepare and commit loses on the root pin. In each case I could not find a lost or torn write.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3218` and `crates/core/src/metadata.rs:3262`: Both arms accept a malformed replacement placement. For an RS(2,1) chunk previously on `[10,11,12]`, passing `[20]` produces a committable batch; readers then identity-fill the missing entries as servers 1 and 2, losing the recorded locations, while maintenance rejects the malformed vector. Reuse `ChunkRef::checked_fragments`/`placement_is_valid` to reject nonempty wrong-length replacements before preparing either arm, with short/long-vector regressions.

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3243`: The fresh segment read goes straight to decoding without the resolver's `MAX_VALUE_BYTES` check (`crates/core/src/metadata.rs:2888`). An oversized but parseable live row can return `Conflict`, or even `Prepared` if re-encoding/repointing shrinks it, instead of surfacing `SegmentValueOverCeiling`. This can arise after the caller resolved the generation. Check the original byte length before decoding and route the anomaly through `retired_or`; test both live and retired generations while retaining acceptance at exactly the full ceiling.

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:4390`: The new concurrent placement path has only hand-sequenced redb tests, so it lacks the seeded Tier-0 coverage required by the standing test-fidelity rubric. Add a seeded campaign in this in-crate module for competing moves and root retirement around the segment read and batch commit, asserting preserved sibling edits and no writes from losing batches. The existing `wyrd-testkit` dev-dependency permits this within the brief's one-file scope.

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:4889`: The purported overflow test never executes an overflowing addition: with offset 0, the first chunk advances `at` to `u64::MAX`, then the next iteration exits at `at > byte_offset`. It would still pass with unchecked addition. Query this fixture at `u64::MAX` with prior `c()` instead; that reaches `u64::MAX + 1` and verifies the checked failure path.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Reject malformed replacement placements and classify oversized live segment input before preparing a batch—both omissions let the primitive commit inputs that existing maintenance/read contracts reject (`crates/core/src/metadata.rs:3218`, `crates/core/src/metadata.rs:3243`, `reviewer-evidence/probe.log:17`).
- [ ] T5 Judgment — Add seeded Tier-0 coverage for move/move and move/retirement races—the new tests hand-sequence redb operations and the existing DST suite never calls this primitive, leaving the standing concurrency requirement unmet (`crates/core/src/metadata.rs:4390`, `crates/core/src/metadata.rs:4618`, `AGENTS.md:188`).
- [ ] Validation — fitness-to-purpose — Confirm acceptance of the primitive-only increment after the implementation findings are resolved—live segmented repair remains refused until the explicitly separate #777 integration ships (`brief.md:2`, `brief.md:117`).
- [ ] **A live `seg:` row over the full ceiling is rewritten, not reported as corruption.** `crates/core/src/metadata.rs:3237-3243` reads and decodes the segment without the `value.len() > MAX_VALUE_BYTES` check that `read_group_range` applies (`metadata.rs:2888-2895`). Concrete case, run as a probe: seed a live one-segment root whose segment row is 100_100 bytes. The lead chunk is RS(10,4) with placement `[u64::MAX; 14]`. `resolve_chunk_map` on that same generation returns `Err` ("segment 0's record is 100100 bytes, over the 100000-byte value ceiling"). `repoint_chunk(.., vec![0; 14])` returns `Prepared`, and the commit rewrites the row down to 99_834 bytes. The read side calls this row structural corruption, while the write side quietly "repairs" it. This goes against brief item 6 ("never hiding a persistently damaged record") and the rubric's *Protocol input* class (oversize input is never silently accepted). The `# Anomalies` doc at `:3184` also leaves this shape out. It is reachable only when the row changed after the caller's resolve, or when a caller plans without resolving, so severity is moderate. The fix is small: map an over-ceiling `bytes` to `ChunkMapError::SegmentValueOverCeiling` and send it through `retired_or` at `:3273`, as the resolver does. Add the test described here. (This is the same issue as the T4 BUG row, now confirmed with a probe.)
- [ ] **The replacement placement is never checked against the chunk's fragment count, in either arm.** `crates/core/src/metadata.rs:3201` takes `placement: Vec<DServerId>` and writes it straight in at `:3218` (flat) and `:3262` (segmented). Concrete cases, both committed in probes:
- [ ] **The overflow guard in `chunk_at` is untested, and the test that claims to cover it passes for a different reason.** At `crates/core/src/metadata.rs:4887-4889` ("Lengths that cannot be summed are no address at all"), `chunk_at(&huge, 0, &c())` returns `None` through the `break` at `:3311`, not through `checked_add(..)?` at `:3317`. Hand mutation: replacing `:3317` with `at = at.saturating_add(chunk.len);` leaves all 13 original `placement_move` tests green. Under that mutant, `chunk_at(&huge, u64::MAX, &c())` returns `Some(2)`, the wrong chunk, which my probe shows. Low impact, because decoded records check their chunk sums. Still, it is a mutant that survives and that `cargo mutants` does not generate. Fix: assert at `byte_offset = u64::MAX`, or correct the comment.
- [ ] **"Seeded Tier-0 DST coverage for a new concurrent path" (T4 TEST-GAP, raised 3×) is a scope call, not a build defect.** `repoint_chunk` (`metadata.rs:3195`) commits nothing and has no caller. The race it takes part in (a move against a supersede or retirement) first exists when #777 wires it in. The brief caps this child at one file with in-crate tests only. I'd suggest recording the T4 row as rejected-with-reference: DST coverage lands with #777, tracked there. The other option is widening this child's scope. A human should pick one so the gating T4 row stops re-firing.
- [ ] `crates/core/src/metadata.rs:3218` and `crates/core/src/metadata.rs:3262`: Both arms accept a malformed replacement placement. For an RS(2,1) chunk previously on `[10,11,12]`, passing `[20]` produces a committable batch; readers then identity-fill the missing entries as servers 1 and 2, losing the recorded locations, while maintenance rejects the malformed vector. Reuse `ChunkRef::checked_fragments`/`placement_is_valid` to reject nonempty wrong-length replacements before preparing either arm, with short/long-vector regressions.
- [ ] `crates/core/src/metadata.rs:3243`: The fresh segment read goes straight to decoding without the resolver's `MAX_VALUE_BYTES` check (`crates/core/src/metadata.rs:2888`). An oversized but parseable live row can return `Conflict`, or even `Prepared` if re-encoding/repointing shrinks it, instead of surfacing `SegmentValueOverCeiling`. This can arise after the caller resolved the generation. Check the original byte length before decoding and route the anomaly through `retired_or`; test both live and retired generations while retaining acceptance at exactly the full ceiling.
- [ ] `crates/core/src/metadata.rs:4390`: The new concurrent placement path has only hand-sequenced redb tests, so it lacks the seeded Tier-0 coverage required by the standing test-fidelity rubric. Add a seeded campaign in this in-crate module for competing moves and root retirement around the segment read and batch commit, asserting preserved sibling edits and no writes from losing batches. The existing `wyrd-testkit` dev-dependency permits this within the brief's one-file scope.
- [ ] `crates/core/src/metadata.rs:4889`: The purported overflow test never executes an overflowing addition: with offset 0, the first chunk advances `at` to `u64::MAX`, then the next iteration exits at `at > byte_offset`. It would still pass with unchecked addition. Query this fixture at `u64::MAX` with prior `c()` instead; that reaches `u64::MAX + 1` and verifies the checked failure path.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Reject malformed replacement placements and classify oversized live segment input before preparing a batch—both omissions let the primitive commit inputs that existing maintenance/read contracts reject (`crates/core/src/metadata.rs:3218`, `crates/core/src/metadata.rs:3243`, `reviewer-evidence/probe.log:17`).; T5 Judgment — Add seeded Tier-0 coverage for move/move and move/retirement races—the new tests hand-sequence redb operations and the existing DST suite never calls this primitive, leaving the standing concurrency requirement unmet (`crates/core/src/metadata.rs:4390`, `crates/core/src/metadata.rs:4618`, `AGENTS.md:188`).; **A live `seg:` row over the full ceiling is rewritten, not reported as corruption.** `crates/core/src/metadata.rs:3237-3243` reads and decodes the segment without the `value.len() > MAX_VALUE_BYTES` check that `read_group_range` applies (`metadata.rs:2888-2895`). Concrete case, run as a probe: seed a live one-segment root whose segment row is 100_100 bytes. The lead chunk is RS(10,4) with placement `[u64::MAX; 14]`. `resolve_chunk_map` on that same generation returns `Err` ("segment 0's record is 100100 bytes, over the 100000-byte value ceiling"). `repoint_chunk(.., vec![0; 14])` returns `Prepared`, and the commit rewrites the row down to 99_834 bytes. The read side calls this row structural corruption, while the write side quietly "repairs" it. This goes against brief item 6 ("never hiding a persistently damaged record") and the rubric's *Protocol input* class (oversize input is never silently accepted). The `# Anomalies` doc at `:3184` also leaves this shape out. It is reachable only when the row changed after the caller's resolve, or when a caller plans without resolving, so severity is moderate. The fix is small: map an over-ceiling `bytes` to `ChunkMapError::SegmentValueOverCeiling` and send it through `retired_or` at `:3273`, as the resolver does. Add the test described here. (This is the same issue as the T4 BUG row, now confirmed with a probe.); **The replacement placement is never checked against the chunk's fragment count, in either arm.** `crates/core/src/metadata.rs:3201` takes `placement: Vec<DServerId>` and writes it straight in at `:3218` (flat) and `:3262` (segmented). Concrete cases, both committed in probes:; **The overflow guard in `chunk_at` is untested, and the test that claims to cover it passes for a different reason.** At `crates/core/src/metadata.rs:4887-4889` ("Lengths that cannot be summed are no address at all"), `chunk_at(&huge, 0, &c())` returns `None` through the `break` at `:3311`, not through `checked_add(..)?` at `:3317`. Hand mutation: replacing `:3317` with `at = at.saturating_add(chunk.len);` leaves all 13 original `placement_move` tests green. Under that mutant, `chunk_at(&huge, u64::MAX, &c())` returns `Some(2)`, the wrong chunk, which my probe shows. Low impact, because decoded records check their chunk sums. Still, it is a mutant that survives and that `cargo mutants` does not generate. Fix: assert at `byte_offset = u64::MAX`, or correct the comment.; `crates/core/src/metadata.rs:3218` and `crates/core/src/metadata.rs:3262`: Both arms accept a malformed replacement placement. For an RS(2,1) chunk previously on `[10,11,12]`, passing `[20]` produces a committable batch; readers then identity-fill the missing entries as servers 1 and 2, losing the recorded locations, while maintenance rejects the malformed vector. Reuse `ChunkRef::checked_fragments`/`placement_is_valid` to reject nonempty wrong-length replacements before preparing either arm, with short/long-vector regressions.; `crates/core/src/metadata.rs:3243`: The fresh segment read goes straight to decoding without the resolver's `MAX_VALUE_BYTES` check (`crates/core/src/metadata.rs:2888`). An oversized but parseable live row can return `Conflict`, or even `Prepared` if re-encoding/repointing shrinks it, instead of surfacing `SegmentValueOverCeiling`. This can arise after the caller resolved the generation. Check the original byte length before decoding and route the anomaly through `retired_or`; test both live and retired generations while retaining acceptance at exactly the full ceiling.; `crates/core/src/metadata.rs:4390`: The new concurrent placement path has only hand-sequenced redb tests, so it lacks the seeded Tier-0 coverage required by the standing test-fidelity rubric. Add a seeded campaign in this in-crate module for competing moves and root retirement around the segment read and batch commit, asserting preserved sibling edits and no writes from losing batches. The existing `wyrd-testkit` dev-dependency permits this within the brief's one-file scope.; `crates/core/src/metadata.rs:4889`: The purported overflow test never executes an overflowing addition: with offset 0, the first chunk advances `at` to `u64::MAX`, then the next iteration exits at `at > byte_offset`. It would still pass with unchecked addition. Query this fixture at `u64::MAX` with prior `c()` instead; that reaches `u64::MAX + 1` and verifies the checked failure path.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
