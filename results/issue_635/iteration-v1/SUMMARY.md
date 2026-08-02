# Result — issue 635 / segmented-chunk-map

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  `InodeRecord.chunk_map` graduates from a flat list to **`Flat | Segmented`**, so a
  published map larger than one backend value can exist at all — the >10 GiB launch requirement.
  Flat stays exactly as it is (`crates/core/src/metadata.rs:268`, `pub chunk_map: Vec<ChunkRef>`)
  and every existing record keeps decoding **byte-identically**; segmented carries a group
  identity plus `seg:<group-nonce>:<epoch>:<index>` segment records and their `seggrp:`
  reservation, is published by **staged publication** (write the segments, then flip the root),
  and is resolved through **one shared resolver that every `.chunk_map` consumer goes through**.
- Success criterion:
  two **NEW** test files. The first is the binding one and it is
  deliberately written to compile on this bundle's base, so its RED is an assertion, not a build
  error.
  **(A) BINDING — every maintenance consumer resolves the segmented shape, and the proof is a
  positive observable, not an absence.** `crates/custodian/tests/segmented_map_consumers.rs`
  seeds a **committed segmented object by raw record bytes** through `MetadataStore::commit`
  (the encoding is settled below, so no symbol this slice adds is named) plus its fragments on
  in-memory D-server doubles, then asserts, in one test binary:
  (i) **`reconcile_step` succeeds** with GC + scrub + reconstruction + rebalance contexts all
  supplied (`crates/custodian/src/reconciliation.rs:65-73`) — on the base it returns `Err`,
  because `referenced_fragments` decodes every `inode:` value with `metadata::decode(&value)?`
  (`crates/custodian/src/gc.rs:255-256`) and a segmented value is not a JSON array;
  (ii) **the segmented object's fragments are in the protected set — asserted positively.**
  `desired_state::reconciliation_status(meta, S)` for a server `S` that holds one of the
  segmented object's fragments, with `desired:dserver:S` seeded, MUST answer **`Pending`**
  (`crates/custodian/src/desired_state.rs:150-164`). A resolver that decodes the new shape but
  never reads the `seg:` range answers `Satisfied` — this is the leg that catches it, and
  nothing else does;
  (iii) **restore does not strand it.** `reconcile_after_restore` reports
  `RestoreReport::stranded_marked == 0` (`crates/custodian/src/restore.rs:104-145`,
  `:179`). This is the #508-attempt-4 failure mode in its exact shape: a resolver used only by
  the read path while `gc.rs` and `restore.rs` still iterated `record.chunk_map` directly, so a
  restore pass stranded a live segmented object and a later GC pass deleted its fragments;
  (iv) **and the data loss that follows is pinned.** Advance past the orphan grace window and run
  a second GC pass: **every fragment the segmented object's resolved map names is still present**.
  Under the (iii) failure the marks laid by restore would now be reclaimed — assert the fragment
  count directly, not `Reconciled::Satisfied`;
  (v) **a flat object in the same store is unaffected** — same passes, same assertions, and its
  stored `inode:` bytes are unchanged byte-for-byte after every pass.
  (vi) **The consumers `reconcile_step` does NOT dispatch get their own positive observable.**
  `reconcile_step` runs GC, scrub, reconstruction and rebalance only
  (`crates/custodian/src/reconciliation.rs:65-114`) — it dispatches **neither backfill nor either
  read path**, and reconstruction reaches `find_chunk` only for a **queued** repair
  (`crates/custodian/src/reconstruction.rs:130-183`), which legs A(i)–(v) never enqueue. So
  `reconcile_step(...).is_ok()` binds four consumers, not eight, and the remaining four would ship
  unresolved behind a green criterion. Add, each asserting a **positive** result rather than
  absence of error: **the gateway read path** returns a segmented object's bytes byte-identical
  (whole-object *and* a range that spans a segment boundary — the ranged walk is a separate
  `.chunk_map` consumer, `crates/server/src/lib.rs:440-460`); **`core`'s read path** resolves it
  (`crates/core/src/read.rs:92`); **reconstruction** resolves a segmented chunk for an
  **explicitly enqueued** repair instead of dropping it; and **backfill** takes its stated decision
  (resolve, or skip with a reason) rather than mangling the map (`crates/custodian/src/backfill.rs:76-130`).
  **(B) The record shape, the CAS identity, and staged publication.** These name types this slice
  ADDS, so they **must NOT ship as a second added `tests/*.rs` file**: `run-verify.sh` collects
  every added test target into **one** cargo invocation (`engine/scripts/run-verify.sh:286-305`,
  `:332-341`) and keeps them all on the RED leg (`:404-415`), so a compile-red file would fail the
  whole invocation and **destroy leg A's assertion red** — the single most valuable thing this
  slice has. Ship leg B as **co-located `#[cfg(test)]` unit tests inside the production modules**
  they exercise (`crates/core/src/metadata.rs`, and the committer's own module), which
  `cargo xtask ci` runs and C4-verify never retains. Over `RedbMetadataStore::in_memory()`:
  (i) **Legacy decode→encode is the identity, byte-for-byte.** Take the *exact* stored bytes of a
  pre-existing flat `InodeRecord` (including one with `etag`/`content_type`/`modified` absent),
  decode and re-encode, assert equality. This is not hygiene: every CAS in
  `crates/core/src/metadata.rs` is `require(key, encode(prior))` compared byte-for-byte against
  the stored value (the `skip_serializing_if` rationale at `crates/core/src/metadata.rs:275-289`),
  so a `chunk_map` whose encoding gained a tag or a wrapper turns **every overwrite, backfill,
  reconstruction and rebalance of every pre-existing object** into a permanent `Conflict`. Assert
  it end-to-end too: `metadata::commit_chunk_map` against a legacy record must return
  `Committed`, not `Conflict`.
  (ii) **The segmented encoding is exactly the one settled below, and its structural invariants
  are enforced AT DECODE.** Assert `encode(Segmented{…})` equals the canonical JSON the test spells
  out literally and decodes back — that keeps leg A's hand-written fixture honest. But a single
  valid example is not an oracle for an invariant: this repo requires structural invariants to be
  **rejected at decode rather than admitted as values** (parse-don't-validate,
  `../wyrd/AGENTS.md:146-149`). So add a **raw-byte negative case per invariant**, each asserting a
  typed decode error and **no partial resolution**: `segment_count != segments.len()`; a duplicate
  `index`; a gap in the index sequence; non-monotonic or overlapping `byte_offset`/`byte_len`; a
  `nonce` that is not 32 lowercase hex; a segment key whose index is not the fixed width. Without
  these the shape is a suggestion, and a malformed record becomes a torn map at the first
  consumer.
  (iii) **Staged publication**: writing a segmented map's `seg:` records in byte-budgeted batches
  and then flipping the root is one committer, and the flip is **one** batch carrying the root
  CAS. Assert: after the segment-write phase and before the flip, the root still names the prior
  generation; after the flip, the root names the group and a resolve returns the full ordered
  chunk list; the flip batch's total mutation **bytes** stay inside the stated envelope
  (the segment-write batches at `≤ E_tx/2`, `0016:2331-2337`; the flip's own inventory bound `≤ 4·V + O(1)`, `0016:654-663`) and no single value exceeds the 100 KB ceiling — measure the
  encoded bytes, do not assert a record count.
  (iv) **The resolver is total, bounded, and orders segments ITSELF.** It reads the root plus the
  bounded range `scan("seg:<nonce>:<epoch>:")` and nothing else — never a global `seg:` scan
  (`0016:2463-2469`). Assert by seeding a *second* group's segments in the same store and checking
  they are neither read nor returned. **And it must not rely on scan order:**
  `MetadataStore::scan` says "Order is unspecified" (`crates/traits/src/lib.rs:770-775`) and #634
  makes byte-lexicographic order normative **only for `scan_page`**, leaving `scan` untouched — so
  the fixed-width zero-padded index is a *debuggability and key-hygiene* property, **not** a licence
  to concatenate in returned order. The resolver parses each segment's `index` and orders by it
  explicitly, rejecting a gap or a duplicate. Assert with a **deliberately shuffling** store double
  that returns the range reversed: resolution must still yield the correct byte order. (This is the
  alternative to consuming `scan_page`, which would make #634 a real dependency rather than a
  file-conflict.)
  (v) **A rolled-back attempt's segments are disjoint from a later attempt's** — seed
  `seg:<nonce>:1:*` and `seg:<nonce>:2:*` and assert resolving the root at epoch 2 returns only
  epoch 2's chunks (the F18 epoch-scoping property, `0016:2352-2380`).
  (vi) **Decision 7(h)'s resolve-retry rule, which the resolver's SIGNATURE must be able to
  express** (`0016:2452-2474`). A generation's `seg:` records are deleted by retirement and
  rollback, so a consumer midway through a segmented resolve can see a segment **absent**. The
  rule is: re-read the **root**; a root now naming a **different group** or **absent** means the
  generation was concurrently retired (a reader restarts against the current root or answers
  `NoSuchKey`; a maintenance pass drops the stale resolution); a root **unchanged** with a segment
  **absent** is an **invariant violation and MUST fail closed** — an error, never a torn success
  (the *Absent or unsupported entries* rule, `../wyrd/AGENTS.md:174-177`). **A resolver that takes
  only a store and an already-decoded `InodeRecord` cannot do this** — it has no way to re-read the
  root. So the API must carry the root's identity (the inode key/id, or a re-read closure) and
  return a retry-or-fail outcome. Assert both arms: changed root → restart/drop; unchanged root
  with a missing segment → typed error and **no partial map**. The interleaving itself (X51) goes
  into the existing `crates/dst/tests/custodian.rs`, never a new DST file (see `Test file`).
  **(C) `cargo xtask ci` green**, including the docs gates — see `Impact & compatibility` for the
  architecture-doc currency requirement, which is a **merge requirement**
  (`../wyrd/AGENTS.md:154-157`), not a follow-up.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope:
  the `Flat | Segmented` record shape and its settled encoding, the `seg:` /
  `seggrp:` records and their key helpers (`crates/core/src/metadata.rs`); the staged
  segment-write + root-flip committer, with the publication precondition taken as a **parameter**;
  the one shared resolver and **every** `.chunk_map` consumer routed through it (the eight sites
  tabled in `Design`); the architecture-doc currency edit; and the two new test files. **Out of
  scope:** the multipart session/records/protocol (#636), the S3 verbs (#508), the staged-byte
  protection class (#637), `PutObject` chunk-size selection (#508 — a single PUT never segments),
  FU-1's record-shape ADR (#628), FU-5's part-record segmentation (#632), and any file under
  `docs/design/adr/` or `docs/design/specs/`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 193 mutants tested in 4m: 25 missed, 57 caught, 111 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 20 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: implement issue #635’s byte-compatible segmented chunk map, staged publication, and shared resolution across every read and maintenance consumer.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit: preserve legacy bytes, validate the new stored shape, publish in two fenced phases, and resolve it in bounded work across every consumer. |
| C2 Reproduction (red pre-fix) | FAIL | The intended `origin/main + #634` red must run five assertion failures, but its binding double omits #634’s required `scan_page`; the rerun stopped at E0046 before any test ran (`crates/custodian/tests/segmented_map_consumers.rs:77`). |
| C3 Change | FAIL | Bring the binding fixture up to its declared wave-1 contract—without `scan_page`, the patch cannot build on its normative stack base and its claimed base-visible regression is invalid (`crates/custodian/tests/segmented_map_consumers.rs:77`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Refresh/fold onto #634 and rerun red→green—the stale target passed `cargo xtask ci` and all 5 post-fix tests, but the normative stack currently stops at E0046, so stack-green remains unverified (`crates/custodian/tests/segmented_map_consumers.rs:77`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Strengthen the causal oracles before acceptance: 25/193 survivors were independently reproduced, including removal of flat repoint map/version writes and the segment fence, so the safety mechanisms are not pinned (`crates/core/src/metadata.rs:1775`, `crates/core/src/metadata.rs:1972`). |
| T1 Structure | PASS | The ownership boundary is coherent: one core resolver serves read paths while a thin custodian adapter centralizes maintenance handling, avoiding duplicated representation logic (`crates/core/src/metadata.rs:1678`, `crates/custodian/src/resolve.rs:28`). |
| T2 Shape | FAIL | Require overflow-safe decode of stored segment spans—unchecked `u64` aggregation can panic or wrap instead of returning a structural error, violating the metadata integrity boundary (`crates/core/src/metadata.rs:836`). |
| T3 Runtime | PASS | On the readable target, the full workspace gate, real in-memory redb exercises, and the 50-seed DST campaign passed; both declared prose-tool dependencies were present and exercised (`crates/dst/tests/custodian.rs:1483`). |
| T4 Contribution | NEEDS-HUMAN | Supply the unavailable `scripts/review-branch --bundle` output and decide its 20 reported blockers—`contribcheck` also lacked the rendered `pdca.toml`, so those red/green rows could not be independently reproduced; affected-path merged and closed/rejected-history searches found no duplicate segmented implementation. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild after adding the stack-base method, overflow-safe decode, and tests that kill the safety-critical survivors; until then the evidence cannot support implementation sign-off (`crates/custodian/tests/segmented_map_consumers.rs:77`, `crates/core/src/metadata.rs:836`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must accept the pinned durable encoding and a fence-as-parameter committer landing before #636 wires its sole producer—this determines whether the otherwise unreachable publication surface fits the staged rollout (`crates/core/src/metadata.rs:641`, `crates/core/src/metadata.rs:1819`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Refresh/fold onto #634 and rerun red→green—the stale target passed `cargo xtask ci` and all 5 post-fix tests, but the normative stack currently stops at E0046, so stack-green remains unverified (`crates/custodian/tests/segmented_map_consumers.rs:77`).
- [ ] C5 Causal adequacy — Strengthen the causal oracles before acceptance: 25/193 survivors were independently reproduced, including removal of flat repoint map/version writes and the segment fence, so the safety mechanisms are not pinned (`crates/core/src/metadata.rs:1775`, `crates/core/src/metadata.rs:1972`).
- [ ] T4 Contribution — Supply the unavailable `scripts/review-branch --bundle` output and decide its 20 reported blockers—`contribcheck` also lacked the rendered `pdca.toml`, so those red/green rows could not be independently reproduced; affected-path merged and closed/rejected-history searches found no duplicate segmented implementation.
- [ ] T5 Judgment — Rebuild after adding the stack-base method, overflow-safe decode, and tests that kill the safety-critical survivors; until then the evidence cannot support implementation sign-off (`crates/custodian/tests/segmented_map_consumers.rs:77`, `crates/core/src/metadata.rs:836`).
- [ ] Validation — fitness-to-purpose — The maintainer must accept the pinned durable encoding and a fence-as-parameter committer landing before #636 wires its sole producer—this determines whether the otherwise unreachable publication surface fits the staged rollout (`crates/core/src/metadata.rs:641`, `crates/core/src/metadata.rs:1819`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 20 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Refresh/fold onto #634 and rerun red→green—the stale target passed `cargo xtask ci` and all 5 post-fix tests, but the normative stack currently stops at E0046, so stack-green remains unverified (`crates/custodian/tests/segmented_map_consumers.rs:77`).; C5 Causal adequacy — Strengthen the causal oracles before acceptance: 25/193 survivors were independently reproduced, including removal of flat repoint map/version writes and the segment fence, so the safety mechanisms are not pinned (`crates/core/src/metadata.rs:1775`, `crates/core/src/metadata.rs:1972`).; T4 Contribution — Supply the unavailable `scripts/review-branch --bundle` output and decide its 20 reported blockers—`contribcheck` also lacked the rendered `pdca.toml`, so those red/green rows could not be independently reproduced; affected-path merged and closed/rejected-history searches found no duplicate segmented implementation.; T5 Judgment — Rebuild after adding the stack-base method, overflow-safe decode, and tests that kill the safety-critical survivors; until then the evidence cannot support implementation sign-off (`crates/custodian/tests/segmented_map_consumers.rs:77`, `crates/core/src/metadata.rs:836`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 20 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
