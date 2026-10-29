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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 90.8% — 474 of 522 instrumentable changed lines executed (floor 80%); 522 of 1049 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 177 mutants tested in 12m: 18 missed, 151 caught, 7 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_653/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.33s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review issue #653: add fenced, bounded publication of flat or segmented chunk maps; no new implementation defect found, with verification, prior-art and fitness decisions left for sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance legs A–G define observable durability, budget and rollback outcomes, with caller obligations and scope explicit; `brief.md:30`, `brief.md:93`, `brief.md:126`. |
| C2 Reproduction (red pre-fix) | N/A | This is a new committer with no existing behavioral entry point to reproduce; the compile-only reverted leg is addressed under C4; `brief.md:141`, `review-red-green.log:7`. |
| C3 Change | PASS | The five-file patch stays within the approved module, tests and architecture surface; the applied target matches the supplied patch and whitespace checks pass; `patch.diff:1`, `patch.diff:2839`, `review-grounding.log:10`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API exception to behavioral red — independent reversion fails compilation before any test runs, while restoring the identical production files passes 21 core and 3 DST tests; `brief.md:149`, `review-red-green.log:7`, `review-red-green.log:65`, `review-red-green.log:74`. |
| C5 Causal adequacy | PASS | The publication and rollback cannot both win the fence, and the entire planned range is checked before the atomic flip; both rollback orders and below/above-cursor loss are exercised; `crates/core/src/staged_publication.rs:449`, `crates/core/src/staged_publication.rs:837`, `crates/core/tests/segmented_publication.rs:1112`, `crates/core/tests/segmented_publication.rs:1335`. |
| T1 Structure | PASS | The committer depends on the MetadataStore seam, preserves backend outcomes and uses the existing record constructors and timestamp rule without adding a clock or shared global state; `crates/core/src/staged_publication.rs:83`, `crates/core/src/staged_publication.rs:445`, `crates/core/src/staged_publication.rs:552`, `crates/core/src/staged_publication.rs:885`. |
| T2 Shape | PASS | The API and living architecture document agree on cursor-shaped fences, ownership and planned segment counts; the size overage is already explicitly accepted; `crates/core/src/staged_publication.rs:31`, `crates/core/src/staged_publication.rs:409`, `docs/design/architecture/08-crosscutting-concepts.md:91`, `brief.md:330`. |
| T3 Runtime | PASS | Independent tests exercise byte- and operation-limited commits, multi-page verification, ambiguous outcomes and both concurrent winners; the seeded violating rollback also triggers the required invariant failure; `crates/core/tests/segmented_publication.rs:631`, `crates/core/tests/segmented_publication.rs:673`, `crates/core/tests/segmented_publication.rs:747`, `crates/dst/tests/segmented_publication.rs:346`, `review-red-green.log:70`. |
| T4 Contribution | N/A | Contribution artifacts are absent by design and must receive their substantive audit at publish; this deferred row is neither a green result nor a missing-evidence finding; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN | Confirm the affected-path search across merged history and closed/rejected work — the brief records both, but this artifact-only target contains one synthetic commit and no remote, so that search cannot be independently settled here; `brief.md:191`, `review-grounding.log:2`, `review-grounding.log:7`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this committer-only slice for integration by #658 — core/DST behavior is exercised, but the production Complete caller and its fence-before-drain obligations remain outside this patch by design; `brief.md:165`, `crates/core/src/staged_publication.rs:61`. |

Source citations are relative to `$PDCA_TARGET`; brief, patch and evidence citations are relative to this review directory. The target is current for this patch: reverse-application validation succeeds. No builder rationale or other checkout was used.

The five carry-forward test gaps are closed in the supplied patch: unrelated deletes cannot substitute for a fence move, fence deletion can publish, delete bytes/operations bind, both inode pins reject a competing generation, verification crosses a page boundary, foreign/stranded rows refuse publication, and wide segment values respect the byte ceiling (`crates/core/tests/segmented_publication.rs:489`, `crates/core/tests/segmented_publication.rs:580`, `crates/core/tests/segmented_publication.rs:631`, `crates/core/tests/segmented_publication.rs:912`, `crates/core/tests/segmented_publication.rs:1028`, `crates/core/tests/segmented_publication.rs:1060`, `crates/core/tests/segmented_publication.rs:1070`). All of these tests passed independently.

The mutation result remains advisory red, not an established production defect. Its full frozen log reports 177 tested, 151 caught, 18 missed, 7 unviable and 1 timeout (`gate-logs/C5-mutants.log:32`). The remaining misses concern byte-accounting and limit boundaries, including single-reference/root value ceilings; they are residual test-sensitivity limitations, not proof that every mutant is equivalent. Inspection of the unmodified checks found no incorrect comparison or omitted accounting term (`crates/core/src/staged_publication.rs:587`, `crates/core/src/staged_publication.rs:617`, `crates/core/src/staged_publication.rs:688`, `crates/core/src/staged_publication.rs:910`). No capability-probe/load-time symptom guard was added.

The independent full CI invocation completed its formatting, clippy, build, workspace tests, conformance, statics/dependency-direction guards and 50-seed DST checks. Its aggregate exit was 1 solely because two cargo-deny checks could not acquire the host advisory database lock (`review-ci.log:3721`, `review-ci.log:4187`). Both checks then passed using the actual tool, unchanged policies except the database location, and a writable copy of the same cached advisory database (`review-deny.log:1`). This is a resolved host restriction, not a patch failure; the frozen full CI invocation also passed (`gate-logs/C4-ci.log:4297`). The exact configured dependency scan passed after reviewer build artifacts were moved outside the source tree (`review-machete-exact.log:2`). Both declared external dependencies were exercised: typos and the real documentation renderer/link audit ran in the independent CI invocation (`review-ci.log:2`, `review-ci.log:10`).

The instance-scoped coverage and batch-review wrappers were adjudicated from their frozen output: coverage is 474/522 instrumentable lines (90.8%), and the batch review reports zero blocking findings (`gate-logs/C4-diff-cov.log:135`, `gate-logs/T4-batch-review.log:10`). The TiKV evidence shows real feature-enabled compilation, not a shim (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). Contribution auditing remains due at publish.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 653 (staged segment publication), round 3

**Bottom line.** I could not break the committer's safety logic. Every red leg the brief demands still goes red against this round's tests, and I found no input that commits a root naming a missing or different `seg:` record. I did find two refusal/accounting paths the tests never pin (a broken check passes all 21 core tests), one weak test claim, and one design gap at the flat/segmented boundary that needs a human decision.

All probes ran in a scratch copy of `$PDCA_TARGET`: `cargo test -p wyrd-core --test segmented_publication` (21 tests), plus the DST file under `--cfg madsim` with `MADSIM_TEST_NUM=50`.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:910`: the root value-ceiling refusal has no test, but leg (C) requires *each* deterministic refusal to leave zero commits. I replaced the check with `if false` and all 21 core tests still passed. With the check gone, a 632-chunk publication whose `meta.content_type` is `MAX_VALUE_BYTES` long committed 4 `seg:` rows and then a **100,388-byte root** on redb. On FDB (100,000-byte hard limit) that flip fails at the store *after* the segment phase, which is exactly the "refusal after durability" that (C) forbids. The shipped code is correct: I checked that it returns `ValueOverCeiling { phase: Flip, key: "inode:8" }` with zero `seg:` rows. Other evidence: C5 survivors `910:22` (`==`, `>=`) and C4-diff-cov MISS on lines 911-915. Two smaller gaps of the same kind: `ChunkTooLarge` at `:588` (diff-cov MISS 589-592, C5 survivors at 588), and `VersionOverflow` at `:881` (swapping `checked_add` for `wrapping_add` passes all tests). Fix: add rows to the existing (C) table at `crates/core/tests/segmented_publication.rs:827`: an over-ceiling `meta.content_type` (expect `ValueOverCeiling { Flip, inode:8 }`), and a `Supersede` whose `prior.version == u64::MAX` (expect `VersionOverflow`).

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:617` (and the split test at `:670`): leg (B)'s byte test never runs at the budget boundary, so nothing checks how the committer counts the bytes of its own rows. I changed `2 * row.key.len()` to `2 + row.key.len()`, which drops the `require_absent` key from each row's cost. All 21 tests passed. I then set the byte budget to 25,252, one byte under the measured two-row transaction (25,253 B, padded contribution as in the shipped test). The shipped code keeps one row per batch (largest batch 17,375 B). The mutant packs two rows and commits a **25,253-byte transaction over a 25,252 budget**. The shipped test at `crates/core/tests/segmented_publication.rs:674` uses `BUDGET = 23_000`, about 2.3 KB below the two-row cost, so it only catches errors of several KB. Round 1's T5 finding asked for "a boundary case", and that part is still open. C5's surviving mutants `617:8` (×2), `652:18`, `659:16`, `670:22`, `670:44`, `587:28` and `595:73` are the same kind. Fix: measure the two-row transaction from an unbounded run, then run once at `two - 1` (assert no batch holds two rows and every batch is within budget) and once at exactly `two` (assert the first batch holds two rows; this kills the `>`→`>=` survivors at `:670`).

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:968` (low priority): deleting the witnessed-row pin (`assembled.require(row.key, row.value)` replaced by nothing) passes all 21 tests. The comment at `crates/core/tests/segmented_publication.rs:729-730` says the recovery "pins them (`require(key, value)`, no put)", but the test only asserts "no put", and D3 does the same. This is not a C-1 hole: the full re-read at `:462` and the fence still guard the flip. But the brief's Design names the pin, and the test text claims a check it does not make. Fix: assert that the recovery's first batch carries `require(seg_key(i), bytes)` for each row that was already durable.

- NEEDS-HUMAN [human] — `crates/core/src/staged_publication.rs:534` / `:418`: the flat-or-segmented choice counts chunks only, while the segment split (`:580-600`) closes segments on encoded bytes. I measured with refs at the widest rendering the project sizes against (`crates/core/src/multipart.rs:4893-4908`):
  - `rs(10,4)` refs are 408 B, so 158 chunks publish a **64,699-byte flat root**. That is over the `V/2 = 50,000` budget that `MAX_MAP_CHUNKS` promises ("Never an over-budget value", `multipart.rs:4958`) and that `metadata.rs:574` calls a durability reserve.
  - `rs(20,10)` refs are 745 B, so 158 chunks are refused `ValueOverCeiling` (117,945 B) on every attempt, while **159** of the same chunks publish fine as 3 segments with a 330-byte root. Fewer chunks fail; more chunks succeed.

  `--durability rs(k,m)` is a live CLI flag (`crates/server/src/cli.rs:550-570`). The brief's Design says "at most `MAX_MAP_CHUNKS` chunks: the root is flat", so the builder did what was asked. A human should decide whether the committer should also segment when a flat root would pass `V/2`, or whether #658 must pass the scheme-aware `KnobSet` cap (`multipart.rs:5430`). This sharpens round 1's note about compile-time caps vs `KnobSet`: even with the shipped constants, the flat path has no byte rule.

- (Calibration, not a defect.) `crates/dst/tests/segmented_publication.rs:11-13`: the DST now mints a fresh inode id per attempt, as round 1 asked, and the header says only the flip's pins on the fence and the dirent keep a second root out. But when I removed the Fresh flip's dirent pin (`staged_publication.rs:920`), all 3 DST tests stayed green over 50 madsim seeds and the 64-seed sweep. The fence pin alone already stops a second flip. So the dirent pin's real red is the core test `a_the_flip_pins_the_dirent_on_both_branches`, not (G). The DST does go red when the flip's contribution preconditions are dropped (resolve panic at `crates/dst/tests/segmented_publication.rs:275`), so (G) does catch a missing fence pin.

- Tried to refute, could not. I re-ran the brief's demanded reds against this round's tests, and each still fails:
  - pre-flip check over only `0..resume_from`: D2 fails;
  - fixed two-row batches: (B) and (D1) fail;
  - fence pin dropped from the flip: F-r fails at `tests:1346` (a root over a drained range);
  - owned-key check off: (C) fails;
  - cycle check off: (F) fails;
  - `read_group` stopping after one page: `a_a_map_over_one_page…` fails.

  These attempts to get a root over a missing or different `seg:` row all failed:
  - a fence that cycles only under another split, or only after a resume: refused on the first attempt (`:790-832`);
  - a stale or inflated `resume_from`: the pre-read and the full re-read catch it;
  - a non-canonical alias key such as `seg:…:3`: `parse_seg_key` enforces the 6-digit width (`metadata.rs:1540`);
  - a flip that deletes the fence: accepted as a move. A flip that both puts and deletes it: `DuplicateMutation`;
  - a fence key inside the owned range: `OwnedKey`.

  I did not raise unbounded awaits: `metadata.rs:2812-2820` documents that backends bound their own waits and that core holds no timer. C4-verify's UNVERIFIABLE is the pre-declared net-new case; the reruns above stand in for its missing red.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness findings: preflight and recovery (`crates/core/src/staged_publication.rs:445`), fence transitions (`crates/core/src/staged_publication.rs:790`), and the atomic root flip (`crates/core/src/staged_publication.rs:900`) are consistent with the documented caller obligations. The deterministic regressions and seeded scenario exercise the relevant failure paths (`crates/core/tests/segmented_publication.rs:788`, `crates/dst/tests/segmented_publication.rs:288`).
- No actionable reuse, simplification, or efficiency findings. Planning uses the existing segment constructors and key helpers (`crates/core/src/staged_publication.rs:552`), and root stamping reuses `overwrite_modified` (`crates/core/src/staged_publication.rs:885`).

Validation evidence was read from the frozen gate logs, not rerun: CI passed, including 21 core tests and three DST tests. The new-API red→green check remains explicitly unverifiable; mutation testing reports 18 survivors. Neither is treated here as proof of an introduced production defect.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the declared new-API exception to behavioral red — independent reversion fails compilation before any test runs, while restoring the identical production files passes 21 core and 3 DST tests; `brief.md:149`, `review-red-green.log:7`, `review-red-green.log:65`, `review-red-green.log:74`.
- [x] T5 Judgment — Confirm the affected-path search across merged history and closed/rejected work — the brief records both, but this artifact-only target contains one synthetic commit and no remote, so that search cannot be independently settled here; `brief.md:191`, `review-grounding.log:2`, `review-grounding.log:7`.
- [x] Validation — fitness-to-purpose — Accept this committer-only slice for integration by #658 — core/DST behavior is exercised, but the production Complete caller and its fence-before-drain obligations remain outside this patch by design; `brief.md:165`, `crates/core/src/staged_publication.rs:61`.
- [x] `crates/core/src/staged_publication.rs:910`: the root value-ceiling refusal has no test, but leg (C) requires *each* deterministic refusal to leave zero commits. I replaced the check with `if false` and all 21 core tests still passed. With the check gone, a 632-chunk publication whose `meta.content_type` is `MAX_VALUE_BYTES` long committed 4 `seg:` rows and then a **100,388-byte root** on redb. On FDB (100,000-byte hard limit) that flip fails at the store *after* the segment phase, which is exactly the "refusal after durability" that (C) forbids. The shipped code is correct: I checked that it returns `ValueOverCeiling { phase: Flip, key: "inode:8" }` with zero `seg:` rows. Other evidence: C5 survivors `910:22` (`==`, `>=`) and C4-diff-cov MISS on lines 911-915. Two smaller gaps of the same kind: `ChunkTooLarge` at `:588` (diff-cov MISS 589-592, C5 survivors at 588), and `VersionOverflow` at `:881` (swapping `checked_add` for `wrapping_add` passes all tests). Fix: add rows to the existing (C) table at `crates/core/tests/segmented_publication.rs:827`: an over-ceiling `meta.content_type` (expect `ValueOverCeiling { Flip, inode:8 }`), and a `Supersede` whose `prior.version == u64::MAX` (expect `VersionOverflow`).
- [x] `crates/core/src/staged_publication.rs:617` (and the split test at `:670`): leg (B)'s byte test never runs at the budget boundary, so nothing checks how the committer counts the bytes of its own rows. I changed `2 * row.key.len()` to `2 + row.key.len()`, which drops the `require_absent` key from each row's cost. All 21 tests passed. I then set the byte budget to 25,252, one byte under the measured two-row transaction (25,253 B, padded contribution as in the shipped test). The shipped code keeps one row per batch (largest batch 17,375 B). The mutant packs two rows and commits a **25,253-byte transaction over a 25,252 budget**. The shipped test at `crates/core/tests/segmented_publication.rs:674` uses `BUDGET = 23_000`, about 2.3 KB below the two-row cost, so it only catches errors of several KB. Round 1's T5 finding asked for "a boundary case", and that part is still open. C5's surviving mutants `617:8` (×2), `652:18`, `659:16`, `670:22`, `670:44`, `587:28` and `595:73` are the same kind. Fix: measure the two-row transaction from an unbounded run, then run once at `two - 1` (assert no batch holds two rows and every batch is within budget) and once at exactly `two` (assert the first batch holds two rows; this kills the `>`→`>=` survivors at `:670`).
- [x] `crates/core/src/staged_publication.rs:968` (low priority): deleting the witnessed-row pin (`assembled.require(row.key, row.value)` replaced by nothing) passes all 21 tests. The comment at `crates/core/tests/segmented_publication.rs:729-730` says the recovery "pins them (`require(key, value)`, no put)", but the test only asserts "no put", and D3 does the same. This is not a C-1 hole: the full re-read at `:462` and the fence still guard the flip. But the brief's Design names the pin, and the test text claims a check it does not make. Fix: assert that the recovery's first batch carries `require(seg_key(i), bytes)` for each row that was already durable.
- [x] `crates/core/src/staged_publication.rs:534` / `:418`: the flat-or-segmented choice counts chunks only, while the segment split (`:580-600`) closes segments on encoded bytes. I measured with refs at the widest rendering the project sizes against (`crates/core/src/multipart.rs:4893-4908`):
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The rollback model omits the transition that makes verify-then-flip safe.** `brief.md:65-68` specifies a rival deleting segments under the fence’s “current value”; `brief.md:208-213` requires only the publisher’s flip to move that value. As specified, this admits: publisher verifies all segments; rollback requires fence A and deletes them without changing A; publisher requires A and flips A→B, publishing missing segments. Separate reads provide no protection: `$PDCA_TARGET/crates/traits/src/lib.rs:1390-1395` explicitly disclaims scan snapshot isolation, and `:1445-1446` guarantees atomicity only within a batch. The target’s actual rollback shape changes `Completing@E` to `Aborting@E+1` and installs the deletion obligation together (`crates/core/src/multipart.rs:2556-2582`). Revise the caller contract and (G): rollback must first win an irrevocable transition away from an eligible publication fence, then drain; it cannot authorize deletion from an arbitrary newly read fence value. Force both rollback-wins and flip-wins interleavings. The committer’s local cycle check alone cannot establish the claimed invariant.
- [x] **The unrestricted caller contribution can invalidate the verified plan inside the flip itself.** `brief.md:200-201` incorporates the caller’s “whole flip contribution (its preconditions and its mutations)” without reserving the committer’s keys. A contribution that correctly pins and advances the fence but also deletes `seg:<nonce>:<epoch>:0` satisfies the stated fence checks: verification succeeds, then the atomic flip publishes a root and deletes its segment, violating `brief.md:78-80`. `WriteBatch` exposes arbitrary puts/deletes (`$PDCA_TARGET/crates/traits/src/lib.rs:1512-1518`), and commit applies all mutations together (`:1445-1446`). Specify ownership/disjointness for the inode, dirent, segment range and fence key, including caller contributions during the segment phase. Add a zero-commit refusal case for a conflicting contribution, or explicitly narrow the invariant with a caller obligation and state how #658 satisfies it.
- [x] **Acceptance (D) does not distinguish the exact mutant the brief promises to kill.** `brief.md:49-54` corrupts a record strictly below `resume_from`, then separately tests `resume_from = 0` with every segment intact. A verifier restricted to `0..resume_from` catches the first corruption and accepts the second case, so both can pass despite `brief.md:127` promising that this temporary edit makes (D) fail. The tracker explicitly requires “all of `0..segment_count`, never only `0..resume_from`” (`notes.json`, body / What). Add a forced case that deletes or changes a segment at or above `resume_from` after segment writes finish but before the final verification, and require refusal before the root flip. This gives the named mutant an observable red.
- [x] size backstop — this slice is behaving oversized: patch is 115 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #653/#658: the flat-or-split choice uses the fixed 158 (sized for rs(6,3)); wider settings go over budget or are always refused. Decide by encoded size, or take the cap from the caller's KnobSet
- Design gap: storage setting and caps are per-process startup flags / compile-time constants, with no central cluster settings record; KnobSet exists but nothing loads it. File an issue for central settings that every process reads at startup
- #653 follow-up tests (adversary r3): add (C) cases for over-limit root value, ChunkTooLarge and VersionOverflow; a (B) batch test at exactly the byte limit (two-1 and two); assert the resume's require(seg_key(i), bytes) pin
