# Adversarial review — issue #635 (segmented-chunk-map)

Advisory only; the human decides at sign-off. Every `path:line` is the target source at
`$PDCA_TARGET` (`/home/eddie/development/wyrd/wyrd.pdca-wt-l0`, base `b0cd199` + working
tree). Toolchain was available: I rebuilt the bundle out-of-tree and re-ran the asserted
red→green plus three counterfactual builds.

## What I could NOT refute (stated explicitly — this is a strong signal, not a hedge)

- **The red→green is real and I reproduced it independently.** `git archive b0cd199` +
  the added file alone: `crates/custodian/tests/segmented_map_consumers.rs` **compiles**
  and **5/5 tests fail** on the base (`reconcile_step` → `invalid type: map, expected a
  sequence`, i.e. the `?` at `crates/custodian/src/gc.rs:265`), and 5/5 pass with the
  patch. The RED is not a build error, as the brief promised.
- **The suite genuinely catches the #508-attempt-4 defect class.** I built the defect
  deliberately — a resolver that decodes the segmented shape but never reads the `seg:`
  range (`crates/core/src/metadata.rs:1687`) — and 4/5 legs went red: the drain answered
  `Satisfied` instead of `Pending`, and `reconcile_after_restore` reported
  `stranded_marked: 12`. Leg A(ii) is doing exactly the job the brief claims for it.
- **The consumer sweep is complete.** `find crates xtask -path '*/src/*' -name '*.rs' |
  xargs grep -l chunk_map` yields only `core/{metadata,read,write}.rs`,
  `custodian/{backfill,resolve}.rs`, `server/{lib,consistency_observable}.rs` — every one
  routed through `resolve_chunk_map` / `resolve_chunk_homes` / `chunks_of`, or fail-closed.
  I could not find an unrouted `.chunk_map` walk.
- Also attempted and failed: legacy flat byte-identity (`metadata.rs:2207`, real, exact);
  `seg:` prefix collisions across nonces/epochs (`seg:<n>:1:` cannot match `seg:<n>:11:`);
  duplicate-index smuggling (`parse_seg_key`'s fixed width makes it impossible); flip
  atomicity (`metadata.rs:2467`, both arms, real store).

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:107`: the segmented-skip guard
  is load-bearing and NO test discriminates it.** I replaced `if
  record.chunk_map.is_segmented()` with `if false` and the *entire* `wyrd-custodian` suite
  stayed green, including
  `backfill_resolves_a_segmented_map_and_leaves_it_byte_identical`
  (`crates/custodian/tests/segmented_map_consumers.rs:716-754`). It passes for the wrong
  reason: every chunk in `seed()` already carries a full-length placement, so
  `to_fill.is_empty()` at `backfill.rs:128` short-circuits before the rewrite whether or
  not the guard exists. The guard is not cosmetic — with it neutralised and one segmented
  fixture chunk carrying `"placement":[]`, backfill **rewrote the segmented root into a
  flat map** (`backfill.rs:148`, `next_chunk_map.into()` → `ChunkMap::Flat`), observed:
  `{"chunk_map":{"group":…,"segments":[…]}}` → `{"chunk_map":[{…,"placement":[0,1,2]}]}`
  at `version:2`, with the `seg:` records left behind unreferenced. That is the brief's
  Open-question-3 requirement ("a decision with an assertion, not an oversight") unmet.
  Concrete fix: seed one segmented-fixture chunk with an empty `placement` and assert the
  root is byte-identical *and* `Reconciled::Satisfied`.
- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:1972` /
  `crates/core/src/metadata.rs:2365`: the caller's `segment_fence` is never asserted to
  ride on the segment-write batches.** `publication()` (the only constructor in the tests)
  hard-codes `segment_fence: Vec::new()`, so `fenced_segment_batch` can be replaced
  wholesale by `Default::default()` with no test failing — this mutant is in the bundle's
  own `mutants.out/missed.txt`. The fence is `require(mpu == Completing@E)`
  (`0016:2331-2337`), the *only* thing stopping a lapsed session from writing segments;
  the brief's B(iii) asked for "a real, non-trivial precondition", and the patch supplies
  one for the flip (`metadata.rs:2467`) but never for phase 1. Concrete case: seed a key
  the test controls, pass it as `segment_fence` with a value that does not match, and
  assert `write_segments` returns `Conflict` and writes no `seg:` record — today nothing
  does.
- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:1917-1922`: the byte-budgeted batch
  split never executes in any test, so leg B(iii)'s envelope assertion is vacuous.** I
  instrumented `SegmentedPublication::segment_batches()`: the largest fixture in the
  bundle, `many_chunks(4_000)` at `metadata.rs:2432`, yields **5 segments in 1 batch**
  (`n=4000 segments=5 batches=1`); the split first fires at ~120 000 chunks
  (`n=120000 segments=125 batches=2`). So
  `every_published_batch_stays_inside_its_byte_envelope` (`metadata.rs:2430`) iterates a
  one-element vector, and six mutants on exactly those lines survived in
  `mutants.out/missed.txt` (`replace > with <`, `&& with ||`, `+= with *=`, …). A
  publication of a >5 MB map — the whole point of the slice — takes an untested path.
- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:1878`: the `MAX_ROOT_SEGMENTS`
  ceiling (the object-ceiling guard this slice exists for) has no test.** Both boundary
  mutants (`> → ==`, `> → >=`) survived. No fixture reaches 513 segments (~500 k chunks),
  so an off-by-one that admits an over-ceiling root — the exact failure the >10 GiB
  requirement turns on — ships undetected. The decode-side omission is separately
  *reasoned* at `metadata.rs:284-288` (liberal-on-read), so I am not re-raising that; the
  publication-side guard is the one with no oracle.
- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:1614-1616`: the
  `SegmentBoundsMismatch` invariant is half-untested.** `1615:13 replace || with &&`
  survived (`mutants.out/missed.txt`), i.e. no case exists where a segment record's
  `byte_offset` disagrees with the root's `SegmentRef` while `byte_len` agrees. Concrete
  input: store `seg:<nonce>:<epoch>:000001` with `"byte_offset":999` and the correct
  `byte_len` — under the `&&` spelling that resolves to a silently mis-offset map, and the
  brief's B(ii) rule ("a raw-byte negative case per invariant, each asserting a typed
  decode error and no partial resolution") is not satisfied for the two invariants the
  patch added beyond the brief's list (`SegmentBoundsMismatch`, `SegmentUnknown`). Same
  for `1591:35 || with &&`.
- **NEEDS-HUMAN [human] — `crates/custodian/src/resolve.rs:14-19` states a safety
  argument that two consumers in this same diff falsify.** The claim: dropping a
  `MapResolution::Retired` resolution "is safe for the reference set specifically because
  GC never reclaims on absence: it reclaims only on evidence". But `crates/custodian/src/
  restore.rs:383` routes through the same silently-dropping helper, and `restore.rs:222`
  →`:266` **manufactures** exactly that evidence — an `orphan:` record for every fragment
  of the skipped object — while `crates/custodian/src/desired_state.rs:157-165` turns the
  same absence into a positive `Satisfied` drain certification. Concrete failing case: a
  pass whose `scan(b"inode:")` snapshot holds generation G1's root while a publisher flips
  to G2 and deletes G1's segments; `retired_or_fail` (`metadata.rs:1643`) sees a changed
  root, answers `Retired`, and `gc.rs:265` drops the **whole inode** — so *G2's live
  fragments* leave the protected set, restore marks them `orphan:`, and the next GC pass
  past grace deletes them. That is #508-attempt-4's data loss in a new spelling. It also
  reads as a straight violation of the standing rubric's *Absent or unsupported entries*
  class (`AGENTS.md:175-177`: "never silent success, silent skip"). This needs a human
  because `0016` decision 7(h) *does* say a maintenance pass drops the stale resolution —
  so the honest options (re-read and re-derive against the current root; fail closed in
  `restore`/`desired_state`; or accept and document the window) are a contract decision,
  not a typo fix. Note also that **no test anywhere drives a `Retired` resolution into a
  consumer** — the two `Retired` tests (`metadata.rs:2649`, the DST prop at
  `crates/dst/tests/custodian.rs:1314`) exercise `resolve_chunk_map` in isolation.
- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:165`,
  `crates/custodian/src/reconstruction.rs:613`, `crates/core/src/metadata.rs:2064` drop a
  retired generation with a bare `continue`, contradicting `resolve.rs:16-19`'s "The drop
  is never silent — it is emitted on the durability seam".** Those three bypass
  `crate::resolve::chunks_of` and call `metadata::resolve_chunk_homes` /
  `resolve_chunk_map` directly, so neither the `custodian_retired_generation_skipped`
  counter nor the audit line is emitted. Concrete: a drain that stalls because rebalance
  skipped an evacuation leaves an operator with no observable cause at all.
- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_consumers.rs:474-499`: leg
  A(iv) ("the data loss that follows is pinned") cannot fail independently.** Leg A(iii)
  at `:466` asserts `stranded_marked == 0` and panics first; with no `orphan:` records and
  `ExpiredPendingPolicy::Defer` (`:423`), the post-grace `reconcile_step` at `:485`
  provably reclaims nothing, so the fragment-set equality at `:495-499` is a tautology given
  the line above it. Confirmed in the crippled-resolver run: the test dies at `:466` and
  `:474` onwards never executes. If leg (iv) is meant to be an independent pin, it has to
  survive leg (iii) failing (e.g. assert it in its own `#[tokio::test]` after deliberately
  laying the restore marks).

## Scope note

`unlink` / `commit_chunk_map_superseding` fail closed on a segmented prior
(`crates/core/src/metadata.rs:1198`, `:1296`, `:1363`) with a reasoned, tracked deferral to #636; per
the rubric's *Deferrals are settled* rule I did not spend an attempt on it. The failing
deterministic gates (T4 20 blocking, C5 25 missed) are already in `check-gates.json`; the
findings above are the subset I could ground in a reproduced failing case.
