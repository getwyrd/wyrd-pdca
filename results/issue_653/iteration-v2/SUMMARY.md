# Result — issue 653 / segmented-staged-publication

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: A published chunk map larger than `MAX_MAP_CHUNKS` (158 chunks,
  `crates/core/src/multipart.rs:4959`) cannot fit in one metadata value, so it is published in
  two phases: write its `seg:<nonce>:<epoch>:<i>` records in bounded batches, then flip the
  inode root in **one** batch (`0016:2342-2355`). The record shapes, key helpers and the shared
  resolver exist (`SegmentedMap::new` `crates/core/src/metadata.rs:1097`, `SegmentRecord::new`
  `:1344`, `seg_key` `:1481`, `resolve_chunk_map` `:3218`), but nothing writes a segmented map.
  This slice adds the **committer**: a `wyrd-core` function over `MetadataStore` that publishes a
  chunk list as a flat or segmented root under a fence the caller supplies. It knows nothing
  about multipart sessions: the caller passes the fence record and, per batch, the fence
  contribution (for #658 that is `require(mpu == Completing@E)` plus the `segments_written`
  cursor move, `0016:613-620`).
- Success criterion: A NEW test file `crates/core/tests/segmented_publication.rs`, over
  `RedbMetadataStore::in_memory()` with a test-local recording wrapper that logs every batch the
  store receives, and a test-local fence record (any key; the committer must not care what it
  is), shows all of the following.
  **(A) Round trip.** A chunk list of more than `MAX_MAP_CHUNKS` chunks (at least three segments'
  worth) publishes, both on a fresh key and superseding an existing flat generation. Afterwards
  `resolve_chunk_map` returns the identical ordered chunk list, the root names the group
  `(nonce, E)` it was given, and the root's version is 1 (fresh) or `prior.version + 1`
  (supersede). A chunk list of at most `MAX_MAP_CHUNKS` publishes as a **flat** root with no
  `seg:` record written.
  **(B) Bounded batches.** With a byte budget set small enough to force at least three
  segment-write batches, every batch the wrapper logged stays within the byte budget measured in
  **encoded bytes** of its mutations, and within the operations budget; the root is written in
  exactly **one** logged commit, which comes after every segment commit.
  **(C) Refusals precede durability.** Each deterministic refusal the committer can make without
  reading the store (a plan needing more than `MAX_ROOT_SEGMENTS` segments, `metadata.rs:544`; a
  flip batch over the byte or operations envelope; a fence contribution that does not pin the
  fence record; a flip that does not move it; a fence sequence that returns to a value it already
  left, see (F); **a caller contribution, in any segment batch or in the flip, that puts, deletes
  or preconditions a key the committer owns**: the target inode, the target dirent, or any key in
  the group's `seg:<nonce>:<epoch>:` range, e.g. a flip contribution that deletes
  `seg:<nonce>:<E>:0`) returns its typed error with **zero** commits logged and the store unchanged.
  **(D) A resume verifies the whole range.** (D1) With a publication's segments planted durable up
  to some index, then one record **below** the caller's claimed `resume_from` deleted (or rewritten
  with different bytes): the publish is refused before the flip, and no root names the group.
  (D2) With `resume_from = r > 0` (records `0..r` planted durable and identical), the committer
  writes `r..segment_count`; right after the **last** segment commit returns, the wrapper deletes
  one record at an index `≥ r`. The publish is refused before the flip: no flip commit is logged
  and no root names the group. (D3) With `resume_from = 0` and every segment already durable and
  identical, the publish completes without rewriting any of them. The check before the flip
  covers all of `0..segment_count`, never only `0..resume_from`; D2 is the case that tells the two
  apart.
  **(E) The flip carries the caller's batch atomically.** The caller's flip contribution (a put on
  some key of its own) commits in the same batch as the root. A wrapper that answers the flip
  `Conflict` leaves neither the root nor that put.
  **(F) Epoch disjointness, fence cycles, and the rollback race.** Two publications of the same
  nonce at epochs `E` and `E' > E` write disjoint key ranges. After `E'` publishes, deleting
  **every** record of epoch `E`'s range (what a rollback drain does) leaves `E'` resolving
  correctly. A fence contribution sequence that goes `A → B → A` across the segment phase and the
  flip is refused under (C). **Rollback, both orders, forced by the wrapper.** The test's rollback
  has the target's shape (`SessionRecord::completing_teardown`,
  `crates/core/src/multipart.rs:2556-2582`): first **one** batch that requires the fence record
  at the value it read and moves it to a value no publisher batch requires, and only after that
  commits, a drain that deletes the group's `seg:` range. (F-r) The rollback's fence move lands
  just before the flip: the flip answers `Conflict`, no root names the group, and the drain then
  deletes the range with nothing published over it. (F-p) The flip lands first: the rollback's
  fence move answers `Conflict`, nothing is deleted, and the root resolves.
  **(G) Seeded publication atomicity** (`AGENTS.md:188-190`). A NEW madsim scenario
  `crates/dst/tests/segmented_publication.rs` runs publications on `SimFdbMetadataStore` with the
  commit-ambiguity nemesis armed (`crates/dst/tests/support/mod.rs:409-415`), including
  `CommitUnknownResult` mid-phase followed by the same caller's recovery at the same epoch, and a
  concurrent rival rollback with (F)'s shape: it first wins a fence move off the publisher's
  value, and only then deletes the epoch's `seg:` range. After every seed: at most one root names
  the group; a root that names it resolves to the plan with every segment present; and no root
  names a missing or different `seg:` record.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the committer as one `wyrd-core` module: planning the segments, the bounded
  segment-write phase, verifying the whole durable prefix, and the single-batch root flip; the
  deterministic refusals; the staged-publication paragraph of the living-architecture docs; the
  two new test files above. / out of scope:
  - the multipart session, its states, the `segments_written` cursor and the Complete verb (#658
    owns them and passes them in);
  - the retry-on-conflict loop around the flip (`R_publish`, `0016:926-941`): the caller's;
  - installing any `retire:` obligation, including `retire:bytes:{generation}` for a superseded
    generation and `retire:records:{seg:<g>:<E>}` for a rollback (#658 and the drain #659);
  - deleting `seg:` records (the drain, #659), and the `seggrp:` marker (Create reserves it; the
    terminal delete, #659, removes it);
  - `crates/core/src/metadata.rs` and `docs/design/architecture/05-building-block-view.md` (no
    edits; #657 edits both in the same wave);
  - any gateway, S3, CLI or custodian caller;
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 89.5% — 467 of 522 instrumentable changed lines executed (floor 80%); 522 of 1049 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 177 mutants tested in 11m: 22 missed, 147 caught, 7 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_653/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.26s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory review of #653’s staged segment publication: the current committer passes its focused tests; one fence-safety test gap needs builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The publication invariant and caller-owned rollback fence make the required safety outcome falsifiable; the target design requires bounded segment writes followed by one fenced flip (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2342`, `brief.md:93`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API with no pre-existing failing behavior to reproduce; the compile-only reverted discriminator is adjudicated under C4 (`brief.md:141`, `review-red-green.log:4`). |
| C3 Change | PASS | The patch stays within the new committer, module export, two tests and living architecture paragraph; reverse-apply validation confirms the disposable target matches the supplied diff (`crates/core/src/lib.rs:17`, `docs/design/architecture/08-crosscutting-concepts.md:91`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or decline the predeclared new-API verification exception — the reverted test cannot import the new module, so behavioral red→green remains unestablished despite 16 core and 3 DST tests passing after restoration (`review-red-green.log:4`, `review-red-green.log:54`, `review-red-green.log:63`). |
| C5 Causal adequacy | PASS | Whole-range verification, cursor-consistent fence transitions and the atomic moving fence address missing segments and the rollback race under the documented caller obligations (`crates/core/src/staged_publication.rs:61`, `crates/core/src/staged_publication.rs:449`, `crates/core/src/staged_publication.rs:507`, `crates/core/src/staged_publication.rs:843`). |
| T1 Structure | PASS | The new operation depends on the MetadataStore seam, reuses validated record constructors and reads no clock; it preserves the existing dependency and time-source boundaries (`crates/core/src/staged_publication.rs:83`, `crates/core/src/staged_publication.rs:445`, `crates/core/src/staged_publication.rs:552`). |
| T2 Shape | NEEDS-HUMAN | Accept the slice’s size or require the planned split — the module is 1,048 lines and its tests total 1,579, above the brief’s approximate 900/1,500-line budgets and explicit human split decision (`brief.md:262`, `crates/core/src/staged_publication.rs:1048`, `crates/core/tests/segmented_publication.rs:1199`, `crates/dst/tests/segmented_publication.rs:380`). |
| T3 Runtime | PASS | Byte and operation limits constrain transactions, scans are paged, and ambiguous commits remain distinguishable for recovery; bounded external-call termination belongs to the backend contract (`crates/core/src/staged_publication.rs:627`, `crates/core/src/staged_publication.rs:982`, `crates/traits/src/lib.rs:1335`, `crates/dst/tests/segmented_publication.rs:345`). |
| T4 Contribution | PASS | The affected-path prior-art check is corroborated against merged history and closed-unmerged #647, and the frozen review reports no blockers; the later contribution-artifact audit is N/A until publish (`review-prior-art.log:1`, `review-prior-art.log:17`, `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add a regression rejecting a flip that deletes an unrelated key without moving the fence — a non-equivalent mutation bypassing that refusal passes all existing publication tests and permits rollback to drain a published group (`crates/core/tests/segmented_publication.rs:814`, `crates/core/src/staged_publication.rs:847`, `review-mutant.log:3`, `review-fence-probe-mutated.log:8`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this helper-only slice is fit for #658’s Complete integration — the evidence exercises a test-local fence, while the real caller must supply retirement obligations and the rollback-before-drain discipline (`crates/core/src/staged_publication.rs:16`, `crates/core/src/staged_publication.rs:61`, `crates/core/tests/segmented_publication.rs:6`). |

The builder-fixable finding is a safety-test gap, not a demonstrated defect in the current committer. At `crates/core/src/staged_publication.rs:847`, changing the fence-delete comparison from `==` to `!=` makes any unrelated delete bypass the “flip must move the fence” check. The existing refusal case keeps the fence unchanged using a put, but supplies no unrelated delete (`crates/core/tests/segmented_publication.rs:814`). The independent mutation run reproduced the survivor while running both publication test targets; its recorded command and outcomes are in `pdca-reviewer-653-mutants/mutants.out/outcomes.json`.

A scratch regression establishes the consequence directly: contribute only `require(FENCE, cursor(segment_count))` and `delete("test:unrelated")` to the flip. The current implementation returns `FenceNotMoved` before any commit (`review-fence-probe.log:40`). With only that comparison mutated in a scratch copy, publication commits without moving the fence; a rollback then successfully requires that same fence, moves it to `aborting`, and drains every segment while the root remains (`review-fence-probe-mutated.log:8`). Add this zero-commit refusal to the target test. The runnable probe is `pdca-reviewer-653-fence-probe/fence_probe.rs:1206`; its module copy has been restored to the unmutated implementation. No production source was repaired by this review.

Verification supports the implementation with explicit limits:

- Independent stash/revert, keeping the discriminator, failed with unresolved import E0432 before any assertion; restoring the patch passed all 19 tests with `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50` (`review-red-green.log:4`, `review-red-green.log:54`, `review-red-green.log:63`). This agrees with the frozen UNVERIFIABLE result (`gate-logs/C4-verify.log:31`).
- The independent full CI attempt completed its build, workspace tests, DST tests, spelling check, documentation lint/render, dependency-use scan, conformance and structural scanners. Its five failures are host limitations: missing rustfmt and clippy components, and an advisory-database lock on a read-only path (`review-ci.log:22`, `review-ci.log:28`, `review-ci.log:3403`, `review-ci.log:3677`). These are not patch verification defects. The frozen full CI log reports success (`gate-logs/C4-ci.log:4297`). Both named external dependencies, `typos` and the docs renderer, actually ran locally (`review-ci.log:2`, `review-ci.log:7`).
- The frozen coverage log reports 467/522 instrumentable changed lines executed, 89.5%, with 527 lines unscored (`gate-logs/C4-diff-cov.log:137`). The frozen TiKV feature log shows real compilation completed (`gate-logs/host-tikv.log:202`, `gate-logs/host-tikv.log:209`). These results are log-backed, not independent reruns of their instance-scoped wrappers.
- The frozen mutation sweep reports 22 missed of 177 mutants (`gate-logs/C5-mutants.log:36`). The safety-relevant survivor above was independently reproduced and given a counterexample; the aggregate advisory failure is not itself a production defect.
- T4-contribution is **N/A**: its artifacts are drafted after Check, and its substantive audit must run at publish (`gate-logs/T4-contribution.log:10`). Nothing is owed to a human merely because that row is deferred.

The target was current and readable, and all source citations above were checked there. The original patch remains intact: reverse-apply checking and whitespace checking passed after the reruns. The previous recovery, precondition-byte accounting, operation-limit, dirent-pin, full-range recheck and public segment-count concerns have corresponding revised code/tests; none is re-raised here. No INTEGRATION.md was present under the supplied target, so no additional human-only checklist could be derived from that file.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #653 staged segment publication (iteration 2)

**Bottom line.** I could not break the production fence logic or any of the round-1 fixes. I did find three guards in `staged_publication.rs` that the shipped tests never exercise. In a scratch copy I removed each one, and all 16 core tests stayed green. Each gap below has a concrete failing case that I wrote as a scratch test: it passes on the patch and fails once the guard is removed. All three are test gaps the builder can close; none is a bug in the code as shipped.

## What I tried to refute and could not

- **Fence track** (`crates/core/src/staged_publication.rs:507-518`, `:790-832`, `:837-857`). I replayed round 1's counterexample: a hook giving `(0,2): A→B`, `(0,1): A→C`, `(1,2): C→B`, with a flip `B→C`. The track is now `[A, C, B]`, so the flip to `C` is refused as `FenceCycled` on the **first** attempt, before any write. A recovery no longer gets a different verdict from the first attempt. Any split's real fence history is a subsequence of the track, and a cycle-free track has only cycle-free subsequences, so the per-batch `FenceOffCursor` check (`:515`) plus `check_track` closes the hole. `f_the_fence_verdict_is_the_same_on_every_attempt` (`crates/core/tests/segmented_publication.rs:1117-1149`) covers a resume after a whole-phase `A→B` batch.
- **Round-1 test fixes really go red.** In a scratch copy:
  - Removing the operations break at `:670` fails `b_the_operation_envelope_binds_where_bytes_do_not`.
  - Swapping out either dirent pin (`:920`, `:924`) fails `a_the_flip_pins_the_dirent_on_both_branches`.
  - Removing the per-batch track check at `:515` fails `c_every_deterministic_refusal_leaves_zero_commits`.
  - D2 now deletes index 3 **and** index 0 (`tests/segmented_publication.rs:932`).
  - The DST (deterministic simulation test) mints a fresh inode per attempt (`crates/dst/tests/segmented_publication.rs:145-147`), and ran in C4-ci (`gate-logs/C4-ci.log:4276-4278`).
- **Verify-then-flip is not atomic.** The brief accepts this under caller obligation 1, and F-r and F-p test both race orders through the real store.
- **C4-verify UNVERIFIABLE (exit 77)** comes from the brief's pre-declared net-new-API exception: the test cannot compile without the new module. This is not a refutation. The scratch mutation runs above are independent red evidence for the legs they touch.

## Findings

- NEEDS-HUMAN [impl] — **The flip's inode pins are untested on both branches.** `crates/core/src/staged_publication.rs:925` (Supersede `require(inode, encode(prior))`) and `:919` (Fresh `require_absent(inode)`). Replacing either with a precondition on an unrelated absent key leaves all 16 core tests green, and the DST only publishes `Fresh`. The brief's Design and `0016:663` require both pins. **Concrete failing case:**
  1. The caller reads inode 8 at version 1 as `prior`.
  2. A concurrent single PUT overwrites inode 8 to version 2 under the same dirent.
  3. `RootTarget::Supersede { id: 8, prior }` then publishes a segmented map.

  The patch returns `Conflict` and keeps the PUT's version 2. With the pin removed, the flip commits over it: the PUT's generation is lost, its chunks get no `retire:` obligation (the caller retires the generation it read), and two different generations both carry version 2. My scratch test of exactly this passes on the patch and fails on the mutant. The existing dirent test (`tests/segmented_publication.rs:512-555`) never changes the inode, so it cannot tell. Fix: add this case, plus `Fresh { id }` over an inode record that already exists.
- NEEDS-HUMAN [impl] — **`read_group`'s paging path has never run in a test.** `crates/core/src/staged_publication.rs:1008-1009`: C4-diff-cov reports `:1009` as MISS (`gate-logs/C4-diff-cov.log`). `SEGMENT_PAGE_LIMIT` is 128 (`crates/core/src/metadata.rs:2839`). The largest map any test publishes is 5 segments (`tests/segmented_publication.rs:40`). The 512-segment case at `:496-500` only calls `planned_segments` and never publishes. A `read_group` that returned after the first page passes all 16 tests. **Concrete failing case for that bug:** a 130-segment map (20,540 chunks, about 20 GiB at 1 MiB chunks, well within `MAX_ROOT_SEGMENTS = 512`) is refused `SegmentNotDurable` on every attempt and can never publish. My scratch test that publishes and resolves 130 segments passes on the patch (0.2 s in memory) and fails with first-page-only reading. Fix: publish one map with more than 128 segments.
- NEEDS-HUMAN [impl] — **The `StrandedRow` and `ForeignRow` refusals never run.** `crates/core/src/staged_publication.rs:1003-1004` and `:997-1002` are both diff-cov MISS (`:998-1001`, `:1004`). The stranded-row check matters for safety: the shared resolver rejects any row at index ≥ `segment_count` as `SegmentUnknown` (`crates/core/src/metadata.rs:3043-3048`), so a root flipped over a stray row never resolves. **Concrete failing case:** a durable `seg:<nonce>:4:00005` sitting next to a 5-segment plan at the same epoch. The patch refuses it with `StrandedRow { index: 5 }`. With the check removed, all 16 tests pass and the publish commits a root that the resolver will reject every time. My scratch test catches this. Fix: one test for each refusal.

## Low priority — not worth a rebuild on its own

- The surviving C5 mutants (`gate-logs/C5-mutants.log`) are mostly boundary changes (`>` to `>=` at `:652`, `:659`, `:670`, `:930`), plus three accounting ones:
  - `row_bytes` at `:617`: `2 * key` becomes `2 + key`, an undercount of about 45 B per row.
  - `batch_bytes` / `batch_ops` at `:696` / `:702`: deletes are subtracted instead of added.

  The real-world error is tiny compared with the headroom between `MAX_BATCH_BYTES = E_TX_BYTES / 2` (`crates/core/src/multipart.rs:5067`) and the transaction limit. A byte budget set one byte below the true two-row cost would kill the `:617` mutant. The `:847` mutant survives because no test has a flip that deletes the fence, so the `:848` branch never runs. That code is correct; only the test is missing.

## Where the verdict may have been too generous

- The brief's Design says the flip "pins the target dirent and inode on both branches" (`0016:663`). Round 1's carry-forward quoted that sentence but asked only for dirent tests. Iteration 2 added them, and they really discriminate. Nothing has tested the inode half in either round (first finding). The T4 pass (`gate-logs/T4-batch-review.log`, 0 blocking) did not catch this, and cargo-mutants cannot, because it never deletes a builder call.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/tests/segmented_publication.rs:476`: the wide-reference test checks the segment count and round trip, but never checks encoded segment sizes. Frozen C5 evidence shows the `bytes + cost` → `bytes - cost` mutant at `crates/core/src/staged_publication.rs:595` survives. Recomputing this fixture's JSON sizes gives 50,765- and 50,773-byte segments under that mutant, exceeding the required 50,000-byte budget while still satisfying every assertion. Assert the byte limit on each recorded `seg:` put; successful resolution alone does not enforce publication's reserved headroom.

- NEEDS-HUMAN [impl] — `crates/core/tests/segmented_publication.rs:737`: caller contributions containing deletes are tested only when another rule rejects them; neither successful deletion nor an unrelated delete beside an unmoved fence is exercised. Frozen C5 evidence shows the equality inversion at `crates/core/src/staged_publication.rs:847` survives: it would accept `require(FENCE, final_cursor).delete(other_key)` without moving the fence, allowing a later rollback to drain published segments. The delete-subtraction mutants at `crates/core/src/staged_publication.rs:696` and `crates/core/src/staged_publication.rs:702` also survive, leaving deletion costs unchecked. Add the unmoved-fence refusal, a successful fence-deleting flip, and a caller-deletion case where the byte/operation envelope binds.

No production correctness defect or material reuse/simplification issue was identified. These are test-discrimination findings grounded in the target source and frozen gate evidence; no source changes or Cargo reruns were performed.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or decline the predeclared new-API verification exception — the reverted test cannot import the new module, so behavioral red→green remains unestablished despite 16 core and 3 DST tests passing after restoration (`review-red-green.log:4`, `review-red-green.log:54`, `review-red-green.log:63`).
- [ ] T2 Shape — Accept the slice’s size or require the planned split — the module is 1,048 lines and its tests total 1,579, above the brief’s approximate 900/1,500-line budgets and explicit human split decision (`brief.md:262`, `crates/core/src/staged_publication.rs:1048`, `crates/core/tests/segmented_publication.rs:1199`, `crates/dst/tests/segmented_publication.rs:380`).
- [ ] T5 Judgment — Add a regression rejecting a flip that deletes an unrelated key without moving the fence — a non-equivalent mutation bypassing that refusal passes all existing publication tests and permits rollback to drain a published group (`crates/core/tests/segmented_publication.rs:814`, `crates/core/src/staged_publication.rs:847`, `review-mutant.log:3`, `review-fence-probe-mutated.log:8`).
- [ ] Validation — fitness-to-purpose — Decide whether this helper-only slice is fit for #658’s Complete integration — the evidence exercises a test-local fence, while the real caller must supply retirement obligations and the rollback-before-drain discipline (`crates/core/src/staged_publication.rs:16`, `crates/core/src/staged_publication.rs:61`, `crates/core/tests/segmented_publication.rs:6`).
- [ ] **The flip's inode pins are untested on both branches.** `crates/core/src/staged_publication.rs:925` (Supersede `require(inode, encode(prior))`) and `:919` (Fresh `require_absent(inode)`). Replacing either with a precondition on an unrelated absent key leaves all 16 core tests green, and the DST only publishes `Fresh`. The brief's Design and `0016:663` require both pins. **Concrete failing case:**
- [ ] **`read_group`'s paging path has never run in a test.** `crates/core/src/staged_publication.rs:1008-1009`: C4-diff-cov reports `:1009` as MISS (`gate-logs/C4-diff-cov.log`). `SEGMENT_PAGE_LIMIT` is 128 (`crates/core/src/metadata.rs:2839`). The largest map any test publishes is 5 segments (`tests/segmented_publication.rs:40`). The 512-segment case at `:496-500` only calls `planned_segments` and never publishes. A `read_group` that returned after the first page passes all 16 tests. **Concrete failing case for that bug:** a 130-segment map (20,540 chunks, about 20 GiB at 1 MiB chunks, well within `MAX_ROOT_SEGMENTS = 512`) is refused `SegmentNotDurable` on every attempt and can never publish. My scratch test that publishes and resolves 130 segments passes on the patch (0.2 s in memory) and fails with first-page-only reading. Fix: publish one map with more than 128 segments.
- [ ] **The `StrandedRow` and `ForeignRow` refusals never run.** `crates/core/src/staged_publication.rs:1003-1004` and `:997-1002` are both diff-cov MISS (`:998-1001`, `:1004`). The stranded-row check matters for safety: the shared resolver rejects any row at index ≥ `segment_count` as `SegmentUnknown` (`crates/core/src/metadata.rs:3043-3048`), so a root flipped over a stray row never resolves. **Concrete failing case:** a durable `seg:<nonce>:4:00005` sitting next to a 5-segment plan at the same epoch. The patch refuses it with `StrandedRow { index: 5 }`. With the check removed, all 16 tests pass and the publish commits a root that the resolver will reject every time. My scratch test catches this. Fix: one test for each refusal.
- [ ] `crates/core/tests/segmented_publication.rs:476`: the wide-reference test checks the segment count and round trip, but never checks encoded segment sizes. Frozen C5 evidence shows the `bytes + cost` → `bytes - cost` mutant at `crates/core/src/staged_publication.rs:595` survives. Recomputing this fixture's JSON sizes gives 50,765- and 50,773-byte segments under that mutant, exceeding the required 50,000-byte budget while still satisfying every assertion. Assert the byte limit on each recorded `seg:` put; successful resolution alone does not enforce publication's reserved headroom.
- [ ] `crates/core/tests/segmented_publication.rs:737`: caller contributions containing deletes are tested only when another rule rejects them; neither successful deletion nor an unrelated delete beside an unmoved fence is exercised. Frozen C5 evidence shows the equality inversion at `crates/core/src/staged_publication.rs:847` survives: it would accept `require(FENCE, final_cursor).delete(other_key)` without moving the fence, allowing a later rollback to drain published segments. The delete-subtraction mutants at `crates/core/src/staged_publication.rs:696` and `crates/core/src/staged_publication.rs:702` also survive, leaving deletion costs unchecked. Add the unmoved-fence refusal, a successful fence-deleting flip, and a caller-deletion case where the byte/operation envelope binds.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The rollback model omits the transition that makes verify-then-flip safe.** `brief.md:65-68` specifies a rival deleting segments under the fence’s “current value”; `brief.md:208-213` requires only the publisher’s flip to move that value. As specified, this admits: publisher verifies all segments; rollback requires fence A and deletes them without changing A; publisher requires A and flips A→B, publishing missing segments. Separate reads provide no protection: `$PDCA_TARGET/crates/traits/src/lib.rs:1390-1395` explicitly disclaims scan snapshot isolation, and `:1445-1446` guarantees atomicity only within a batch. The target’s actual rollback shape changes `Completing@E` to `Aborting@E+1` and installs the deletion obligation together (`crates/core/src/multipart.rs:2556-2582`). Revise the caller contract and (G): rollback must first win an irrevocable transition away from an eligible publication fence, then drain; it cannot authorize deletion from an arbitrary newly read fence value. Force both rollback-wins and flip-wins interleavings. The committer’s local cycle check alone cannot establish the claimed invariant.
- [ ] **The unrestricted caller contribution can invalidate the verified plan inside the flip itself.** `brief.md:200-201` incorporates the caller’s “whole flip contribution (its preconditions and its mutations)” without reserving the committer’s keys. A contribution that correctly pins and advances the fence but also deletes `seg:<nonce>:<epoch>:0` satisfies the stated fence checks: verification succeeds, then the atomic flip publishes a root and deletes its segment, violating `brief.md:78-80`. `WriteBatch` exposes arbitrary puts/deletes (`$PDCA_TARGET/crates/traits/src/lib.rs:1512-1518`), and commit applies all mutations together (`:1445-1446`). Specify ownership/disjointness for the inode, dirent, segment range and fence key, including caller contributions during the segment phase. Add a zero-commit refusal case for a conflicting contribution, or explicitly narrow the invariant with a caller obligation and state how #658 satisfies it.
- [ ] **Acceptance (D) does not distinguish the exact mutant the brief promises to kill.** `brief.md:49-54` corrupts a record strictly below `resume_from`, then separately tests `resume_from = 0` with every segment intact. A verifier restricted to `0..resume_from` catches the first corruption and accepts the second case, so both can pass despite `brief.md:127` promising that this temporary edit makes (D) fail. The tracker explicitly requires “all of `0..segment_count`, never only `0..resume_from`” (`notes.json`, body / What). Add a forced case that deletes or changes a segment at or above `resume_from` after segment writes finish but before the final verification, and require refusal before the root flip. This gives the named mutant an observable red.
- [ ] size backstop — this slice is behaving oversized: patch is 108 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Tests only: the committer's production logic held up under three reviewers in round 2 — keep it, do not redesign. Close the five test gaps so each guard has a discriminating red: 1. Flip that deletes an unrelated key without moving the fence (`require(FENCE, cursor(n)).delete("test:unrelated")`) → zero-commit `FenceNotMoved`; plus a successful fence-deleting flip and a caller-delete case where the byte/ops envelope binds (kills the `:847` `==`→`!=` and `:696`/`:702` delete-subtraction mutants). 2. Inode pins on both flip branches: Supersede after a concurrent PUT bumped the inode → `Conflict`, PUT's version kept; Fresh over an existing inode record → `Conflict`. 3. `read_group` paging: publish and resolve one map with more than 128 segments (`SEGMENT_PAGE_LIMIT`). 4. `StrandedRow` and `ForeignRow` refusals: one test each (e.g. a durable `seg:<nonce>:E:00005` beside a 5-segment plan). 5. Wide-ref test: assert every recorded `seg:` put is within the byte budget (kills the `:595` `bytes + cost`→`bytes - cost` mutant). Size: the human accepts the slight overage (108 KB / ~1,050 prod lines); keep additions to these tests, no matrices, no production growth beyond what a red forces.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
