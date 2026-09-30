- **Slug:** seg-record-placement-move-primitive
- **Defect:** **No maintenance write path in the tree can address a `seg:` record.** The
  only placement writer rebuilds an *inode* record: `repair_chunk`
  (`crates/custodian/src/reconstruction.rs:829`) takes `object.prior.chunk_map.as_flat()`
  at `:894`, aborts on `None`, and CASes `inode:` at `:937-953`. A
  `seg:<nonce>:<epoch>:<index>` record cannot be written by any repair-shaped code. This
  child ships the missing `wyrd_core` primitive; it changes **no** custodian behaviour —
  the pass keeps refusing until #777 wires it in.
- **Success criterion:** `crates/core/src/metadata.rs` exports a placement-move primitive:
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
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality. Absence shown on the base (verified at Plan):
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` — `:894` takes
  `as_flat()` and aborts on `None`; no writer composes a `seg:` CAS for a placement move.
- **Scope:** **one file** — `crates/core/src/metadata.rs`: the primitive, its two
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
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/core/src/metadata.rs` — in-crate `#[cfg(test)]` module,
  **deliberately not** a new `tests/*.rs` file (see the verification posture above: an
  added test file naming the new symbols makes C4-verify's revert leg fail to compile).
- **Falsifiability:** **Checked against this project's gate at Plan, not assumed.** This child
  is **green-only under C4-verify by design**, a *confirmed* property of the gate:
  `run-verify.sh:143` classifies via `_added_files` filtered by `_is_test_file`
  (`*/tests/*.rs`), and at `:493` an empty `ADDED_TESTS` takes the `PASS (green-only)` branch
  and **exits 0**. An in-crate `#[cfg(test)]` module therefore cannot earn a per-fix RED — and
  shipping the tests as a new `tests/*.rs` would be *worse*: the revert leg strips the
  production change, the test still names the new symbols, so it fails to **compile** →
  UNVERIFIABLE (exit 77, `:538-548`). **So the binding oracle is `cargo mutants` over the diff
  (`C5-mutants`), not C4-verify.** Where each pin goes red — demonstrated in `build-notes.md`,
  never asserted: delete the `chunk == prior` equality → same-chunk-conflict test red; delete
  the root-generation pin → superseded-root test red; delete the ceiling comparison (`>
  MAX_VALUE_BYTES`) → ceiling test red; make the version advance unchecked → exhaustion test red. Expect **zero** missed
  mutants in the new code (parent's residue was 17, all here). `C4-ci` still gates the tree.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost** (`docs/principles.md` §5 C-1 at `:109`, §6 row *Storage lifecycle /
  reclamation* at `:137`). This child does not by itself restore C-1 — **#777 does** — but
  it is the enabling half: C-1 cannot be restored while the tree contains **no write path
  that can address a `seg:` record at all**. Structural/lifecycle category, so the Plan-exit
  gate applies and is recorded as passed on #777, which owns the behavioural change.
- **Surfaces:** data
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer callsite to mirror (do not re-derive the CAS idiom):** `commit_chunk_map`
  (`crates/core/src/metadata.rs:1776`) is the tree's existing flat placement CAS — mirror
  its `version = prior.version + 1` / `..prior.clone()` advance (`:1769-1797`, ADR-0047
  metadata preserved) and its `require(key, encode(prior)) + put(key, encode(next))` shape.
  Leave `commit_chunk_map` itself, including its segmented refusal at `:1778-1781`,
  **exactly as it is**. The segment-table lookup peer is `SegmentedMap::new` (`:870`); the
  ceiling peer is `flat_value_ceiling_crossed` (`:380`); the in-crate unit-test convention
  is the module's own at `:2776-2780`.
- **Prior-art check (triage cycles):** run at Plan by affected file path. `repoint_chunk` is
  **absent** from `origin/main` — `git grep` finds one mention, a deferral comment at
  `crates/custodian/src/backfill.rs:112` naming **#682** (this work's grandparent) as its
  owner. Merged on `metadata.rs`: `d2609b2` (#710 flat ceiling helper), `99c7fcf` (shared
  resolver), `3e05891` (segmented record shape + codec) — every named prerequisite is in.
  **Open PRs: none in the repo at all**, settling the parent brief's previously unverifiable
  "no open PR touches these paths". No closed/rejected prior attempt outside this lineage.
- **Conflicts with:** 772
- **Ordering note:** wave 0 — no in-batch prerequisite; every external one is merged. #772
  edits this same file (`owner`/`staged` into `PendingEntry`, `metadata.rs:1556`); it is
  `PLANNED [blocked-by: 771]` with #771 still `ITERATE_DO`, so ≥2 waves out. The parent's
  inherited `Conflicts with: 717` is **stale and dropped** — #717 is `COMPLETE [close: no
  PR]`, split, ships no code. **Cite by symbol, not line number, below `metadata.rs:1556`.**
- **Disposition hint:** likely-fix
- **Difficulty:** medium — **re-rated at Plan, not inherited.** Blast-radius only: **one
  file**, one new self-contained API, **zero existing call sites changed** (nothing calls it
  until #777). Not `low` — the pin semantics are subtle and `wyrd-core` is widely depended
  on — but the parent's `high` came from spanning two crates and rewriting the repair
  lifecycle, both of which this child sheds. Edge-case density is high and is deliberately
  NOT what this field measures; the mutation gate covers that.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Reject malformed replacement placements and classify oversized live segment input before preparing a batch—both omissions let the primitive commit inputs that existing maintenance/read contracts reject (`crates/core/src/metadata.rs:3218`, `crates/core/src/metadata.rs:3243`, `reviewer-evidence/probe.log:17`).; T5 Judgment — Add seeded Tier-0 coverage for move/move and move/retirement races—the new tests hand-sequence redb operations and the existing DST suite never calls this primitive, leaving the standing concurrency requirement unmet (`crates/core/src/metadata.rs:4390`, `crates/core/src/metadata.rs:4618`, `AGENTS.md:188`).; **A live `seg:` row over the full ceiling is rewritten, not reported as corruption.** `crates/core/src/metadata.rs:3237-3243` reads and decodes the segment without the `value.len() > MAX_VALUE_BYTES` check that `read_group_range` applies (`metadata.rs:2888-2895`). Concrete case, run as a probe: seed a live one-segment root whose segment row is 100_100 bytes. The lead chunk is RS(10,4) with placement `[u64::MAX; 14]`. `resolve_chunk_map` on that same generation returns `Err` ("segment 0's record is 100100 bytes, over the 100000-byte value ceiling"). `repoint_chunk(.., vec![0; 14])` returns `Prepared`, and the commit rewrites the row down to 99_834 bytes. The read side calls this row structural corruption, while the write side quietly "repairs" it. This goes against brief item 6 ("never hiding a persistently damaged record") and the rubric's *Protocol input* class (oversize input is never silently accepted). The `# Anomalies` doc at `:3184` also leaves this shape out. It is reachable only when the row changed after the caller's resolve, or when a caller plans without resolving, so severity is moderate. The fix is small: map an over-ceiling `bytes` to `ChunkMapError::SegmentValueOverCeiling` and send it through `retired_or` at `:3273`, as the resolver does. Add the test described here. (This is the same issue as the T4 BUG row, now confirmed with a probe.); **The replacement placement is never checked against the chunk's fragment count, in either arm.** `crates/core/src/metadata.rs:3201` takes `placement: Vec<DServerId>` and writes it straight in at `:3218` (flat) and `:3262` (segmented). Concrete cases, both committed in probes:; **The overflow guard in `chunk_at` is untested, and the test that claims to cover it passes for a different reason.** At `crates/core/src/metadata.rs:4887-4889` ("Lengths that cannot be summed are no address at all"), `chunk_at(&huge, 0, &c())` returns `None` through the `break` at `:3311`, not through `checked_add(..)?` at `:3317`. Hand mutation: replacing `:3317` with `at = at.saturating_add(chunk.len);` leaves all 13 original `placement_move` tests green. Under that mutant, `chunk_at(&huge, u64::MAX, &c())` returns `Some(2)`, the wrong chunk, which my probe shows. Low impact, because decoded records check their chunk sums. Still, it is a mutant that survives and that `cargo mutants` does not generate. Fix: assert at `byte_offset = u64::MAX`, or correct the comment.; `crates/core/src/metadata.rs:3218` and `crates/core/src/metadata.rs:3262`: Both arms accept a malformed replacement placement. For an RS(2,1) chunk previously on `[10,11,12]`, passing `[20]` produces a committable batch; readers then identity-fill the missing entries as servers 1 and 2, losing the recorded locations, while maintenance rejects the malformed vector. Reuse `ChunkRef::checked_fragments`/`placement_is_valid` to reject nonempty wrong-length replacements before preparing either arm, with short/long-vector regressions.; `crates/core/src/metadata.rs:3243`: The fresh segment read goes straight to decoding without the resolver's `MAX_VALUE_BYTES` check (`crates/core/src/metadata.rs:2888`). An oversized but parseable live row can return `Conflict`, or even `Prepared` if re-encoding/repointing shrinks it, instead of surfacing `SegmentValueOverCeiling`. This can arise after the caller resolved the generation. Check the original byte length before decoding and route the anomaly through `retired_or`; test both live and retired generations while retaining acceptance at exactly the full ceiling.; `crates/core/src/metadata.rs:4390`: The new concurrent placement path has only hand-sequenced redb tests, so it lacks the seeded Tier-0 coverage required by the standing test-fidelity rubric. Add a seeded campaign in this in-crate module for competing moves and root retirement around the segment read and batch commit, asserting preserved sibling edits and no writes from losing batches. The existing `wyrd-testkit` dev-dependency permits this within the brief's one-file scope.; `crates/core/src/metadata.rs:4889`: The purported overflow test never executes an overflowing addition: with offset 0, the first chunk advances `at` to `u64::MAX`, then the next iteration exits at `at > byte_offset`. It would still pass with unchecked addition. Query this fixture at `u64::MAX` with prior `c()` instead; that reaches `u64::MAX + 1` and verifies the checked failure path.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — `crates/core/src/metadata.rs:3194` says the flat arm leaves "`state` left as it was", but no test pins it. Every test builds a `Committed` generation (`flat_root`, patch test helper). Concrete case: change the flat arm's `InodeRecord { .. }` to add `state: InodeState::Committed` — exactly the peer idiom at `metadata.rs:2183` that the brief says to mirror — and all 16 `placement_move` tests still pass. I ran this. Under that change, a move on a `Pending` generation would publish it as `Committed`. Current callers skip non-committed records (`crates/custodian/src/reconstruction.rs:633`), so the risk is low. The fix is small: add a `Pending` flat generation to `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata` (`:4556`) and assert the state is unchanged. Or refuse a non-`Committed` generation outright.; The two new `ChunkMapError` variants break that enum's own doc contract. `crates/core/src/metadata.rs:609-613` says every variant is "a structural violation of the segmented chunk-map shape", raised at decode or at a site that met an unwired `Segmented` map. `VersionExhausted` (`:848`) is neither, and `ReplacementPlacementMalformed` (`:857`) is a bad argument from the caller, not an object fault. This matters for #777: every custodian consumer downcasts `ChunkMapError` to mean "this object is unreadable, contain it and keep walking" (`crates/custodian/src/reconstruction.rs:639-645`, and the same pattern at `gc.rs:1245`, `rebalance.rs:316`, `restore.rs:734`, `backfill.rs:163`). A planner bug that passes a wrong-length placement would then be filed as per-object corruption instead of surfacing as a bug. Fix inside the same file: update the enum doc to admit write-side refusals, or return the placement refusal as its own type (`MalformedPlacement` at `:460` already has the same `{expected, actual}` shape).. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — **Brief item 2 ("no other segment decoded") is not pinned at a segment boundary, and C5's "0 missed" does not cover it.** `crates/core/src/metadata.rs:3267` passes `prior.len` to `segment_may_hold`. Change that argument to `0` and a non-empty chunk that *starts* a segment also reads and decodes the previous segment, yet all 16 `placement_move` tests stay green. I ran it. cargo-mutants never swaps a call-site argument, so the gate's 27 caught / 0 missed says nothing here. Concrete failing case under the mutant: `two_segments`, segment 0 overwritten with garbage while the root still names the generation, move `b()` at offset 5. The patch returns `Prepared` (correct). The mutant returns `Err(SegmentRecordUndecodable { index: 0 })`: a healthy chunk becomes unrepairable because its neighbour is damaged. The test meant to pin this, `segmented_arm_rewrites_only_the_covering_segment_and_never_the_root` (`:4624`), moves `c()` at offset 8 (`:4635`). Offset 8 is inside segment 1 and never touches segment 0's edge, so it can't see the defect. Fix: add the offset-5 / `b()` move to that test. As a probe it passes on the patch and fails under the mutant.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide whether supplemental passing checks discharge the host caveat or require an uninterrupted CI run — frozen CI timed out; my rerun completed workspace tests but stopped on a read-only advisory-cache lock (`gate-logs/C4-ci.log:7`, `reviewer-evidence/ci-rerun.log:3006`).. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b. 9 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_776/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
