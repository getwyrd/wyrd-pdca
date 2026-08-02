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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 246 mutants tested in 6m: 1 missed, 105 caught, 140 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #635: add byte-compatible flat/segmented inode chunk maps, staged publication, and shared resolution across every read and maintenance consumer.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | No Plan decision is missing: legacy byte identity, the segmented record contract, bounded resolution, staged publication, and the later-session seam are explicit at `crates/core/src/metadata.rs:732` and `crates/core/src/metadata.rs:2092`. |
| C2 Reproduction (red pre-fix) | PASS | Clean `HEAD` plus only the added test target compiled and ran 8 tests, all 8 failing on the segmented-map behavior rather than a build error; the binding maintenance assertion is `crates/custodian/tests/segmented_map_consumers.rs:504`. |
| C3 Change | PASS | The scoped change has one core resolver, typed malformed/retired outcomes, a caller-extensible atomic flip, and architecture currency, so consumers share the same persisted-map decision at `crates/core/src/metadata.rs:1822`, `crates/core/src/metadata.rs:2294`, and `docs/design/architecture/06-runtime-view.md:30`. |
| C4 Verification (red→green) | PASS | With the patch, the same target passed 8/8 and a scratch-cache rerun of `cargo xtask ci` completed typos, docs render/link audit, fmt, clippy, build, tests, deny, conformance, guards, and DST; the exercised positive oracles begin at `crates/custodian/tests/segmented_map_consumers.rs:448`. |
| C5 Causal adequacy | PASS | The red→green consumers pin the data-loss mechanism, and the 246-mutant rerun's sole survivor is equivalent: deleting `size: record.size` at `crates/custodian/src/backfill.rs:158` leaves `..record.clone()` at `crates/custodian/src/backfill.rs:165` to supply the identical value. |
| T1 Structure | PASS | Resolver ownership is centralized in core and the custodian adds only one maintenance wrapper, preventing representation-specific logic from diverging at `crates/core/src/metadata.rs:1822` and `crates/custodian/src/resolve.rs:34`. |
| T2 Shape | PASS | The compatibility decision is enforced structurally: JSON type preserves the flat wire shape, segmented decode validates its table, and inode decode validates table span before admitting a value at `crates/core/src/metadata.rs:718`, `crates/core/src/metadata.rs:808`, and `crates/core/src/metadata.rs:1090`. |
| T3 Runtime | PASS | Runtime race behavior is covered by the full green gate and dedicated seeded DST properties for retirement, atomic flip, and repoint-versus-supersede at `crates/dst/tests/custodian.rs:1829`, `crates/dst/tests/custodian.rs:1835`, and `crates/dst/tests/custodian.rs:1841`. |
| T4 Contribution | NEEDS-HUMAN | Triage or reproduce the 10 reported batched-review blockers — `scripts/review-branch` and its finding output are absent, so that red row is provisional; independent affected-path merged history plus all 9 closed-unmerged PRs found no competing segmented implementation. |
| T5 Judgment | PASS | Independent source judgment found no grounded implementation, causal, scope, or rubric defect; malformed or unstable resolutions fail closed rather than becoming an empty ownership answer at `crates/core/src/metadata.rs:1752` and `crates/core/src/metadata.rs:1905`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer must accept the exact persisted JSON contract and landing the staged committer ahead of its #636 production caller — these choices become durable compatibility and sequencing commitments at `crates/core/src/metadata.rs:697` and `crates/core/src/metadata.rs:2092`. |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map

Attacked: (1) the asserted red→green base, (2) the staged-publication committer's inputs,
(3) the resolver's fail-closed blast radius, (4) the leg-B oracles. Two attacks landed with
a reproduced failing case; two more are conformance-level. What I tried and could **not**
refute is listed at the end.

## Findings

- **NEEDS-HUMAN [impl] — the committer accepts a same-epoch re-publication with a shrunk
  plan, and that permanently bricks the published object.** `SegmentedPublication::flip_batch`
  (`crates/core/src/metadata.rs:2294`) requires only the root's prior bytes; neither
  `write_segments` (`:2311`) nor `publish` (`:2338`) deletes, or requires the absence of,
  segment keys past the current plan's last index. `read_segments` then fails closed on any
  index `>= segment_count` (`crates/core/src/metadata.rs:1775`). **Reproduced** against the
  patched tree (probe test in a throwaway copy, `cargo test -p wyrd-core --lib`): publish
  1 500 chunks at `group(nonce,7)` → 2 segments, `Committed`; then `publish` again at the
  **same** group/epoch with 20 chunks, superseding the just-published root → also
  `Committed`; the store still holds `seg:…:7:000001` from attempt 1 while the new root names
  one segment. Result, verbatim:
  `resolve of the published object => Some("segment 1 exists under seg:0123456789abcdef0123456789abcdef:7 but the root does not name it")`.
  The object is now unreadable **forever** — no code path in this slice can delete the stray
  key. This is not a hypothetical call shape: the patch's own DST property
  (`crates/dst/tests/custodian.rs`, "*the recovery re-runs the WHOLE publication at the same
  epoch*") establishes same-epoch re-publication as the sanctioned `CommitUnknownResult`
  recovery, and it is asserted only for an *identical* chunk list. Leg B(v) only covers the
  cross-epoch disjointness case (`seg:<n>:1:*` vs `:2:*`), so nothing in the bundle exercises
  the same-epoch shrink. Fixable by iterating: have `flip_batch` add
  `require_absent(seg_key(&group, plan.len() as u32))` (and/or delete the tail), plus the
  missing test.

- **NEEDS-HUMAN [impl] — one stray segment record now fails gateway startup and halts the
  whole maintenance plane, not just the affected object.** In the same probe run,
  `high_water_marks` — the function `Gateway::recover()` calls before serving
  (`crates/server/src/lib.rs:124`) — returned the same error store-wide:
  `high_water_marks => Some("segment 1 exists under seg:…:7 but the root does not name it")`,
  because the patch added an unconditional `resolve_live_chunk_map(...)?` inside its `inode:`
  walk (`crates/core/src/metadata.rs:2480`). So the node **never starts**. The identical `?`
  at `crates/custodian/src/gc.rs:265` propagates out of `referenced_fragments` →
  `gc::reconcile` → `reconcile_step`, so GC, scrub, rebalance, reconstruction and
  `reconcile_after_restore` stop for **every** object in the store, and
  `reconciliation_status` stops answering for every drain. Fail-closed on a torn map is the
  right *direction* (rubric: *Absent or unsupported entries*), and a corrupt `inode:` value
  already halted GC before this patch — but the new producer is a committer this slice ships,
  the reach is now cluster-wide from one record, and neither blast radius has a test or an
  operator-visible signal: `resolve.rs`'s emitters cover only the `None`/restart arms
  (`crates/custodian/src/resolve.rs:74`,`:86`), never the `Err` arm. Note the aggravating
  detail: `recover()`'s only use of the scan is `max_inode`; it discards `_max_chunk`
  (`crates/server/src/lib.rs:124`), so the resolve that can refuse the boot is done for a
  value the caller throws away.

- **NEEDS-HUMAN [human] — the red→green and `xtask ci` rows were earned on a base that is
  not the brief's normative base, and the carried-forward blocker is unaddressed.** The brief
  declares the base `origin/main` **plus #634** and instructs, verbatim, that the in-test
  double "must implement `scan_page` (#634's required method, one delegating line)"
  (brief.md, *Falsifiability* corollary); iteration 1 was rejected for exactly this
  ("*Rebuild after adding the stack-base method … `crates/custodian/tests/segmented_map_consumers.rs:77`*",
  brief.md:487). On `origin/enhancement/634-scan-page-seam`, `MetadataStore::scan_page` is
  **required with no default body** (deliberately — "*this method is required*"). This patch
  adds five `MetadataStore` impls, none of which has a `scan_page`:
  `crates/custodian/tests/segmented_map_consumers.rs:83`, `crates/core/src/metadata.rs:3329`,
  `:3621`, `:3857`, `crates/server/src/lib.rs:882`. `scan_page` appears **nowhere** in the
  target tree, and `origin/pdca-integration/main` does not resolve in this checkout (`git
  rev-parse` fails; both wave worktrees sit on plain `main` @ `b0cd199`). So on the normative
  base every one of those five is `E0046` — `cargo xtask ci` would not build and C4-verify's
  RED leg would be a *build* error, which the brief itself warns prints "PASS — red without
  the fix" over a run that executed nothing. On *this* base the red is genuine (I traced the
  file: it names only base symbols, and a segmented `inode:` value makes `metadata::decode`
  fail so `gc.rs`'s `?` propagates into leg A(i) as an assertion failure). This is a human
  call, not a builder iteration: on the un-folded base the required `scan_page` line **cannot
  compile**, so the fix is to fold #634 and re-run, or to accept the C4 rows as
  base-provisional. Marked per issue #236 — my verdict on stack-green is provisional.

- **NEEDS-HUMAN [impl] — `segment_group_adopted` takes an unvalidated `&str` nonce, so the
  predicate #636 will gate marker deletion on can silently answer "not adopted" for a live
  group.** `crates/core/src/metadata.rs:2084` (`nonce: &str`), over `seg_group_prefix(nonce)`
  (`:943`) and `seggrp_key(nonce)` (`:948`) — all three bypass `SegmentGroup::new`, the
  validating constructor the module introduced precisely so "a nonce that could not key a
  reproducible `seg:` range is never representable" (`:566-580`). Concrete case: a caller that
  passes an uppercased or truncated nonce (`"0123456789ABCDEF…"`, or the first 16 chars) gets
  `Ok(false)` — the scan prefix simply matches nothing — and #636's two-arm lifetime rule
  then deletes the `seggrp:` marker of a group whose segments are live, re-opening the nonce
  for reuse. Reuse overwriting a live object's segment records is the exact hazard
  iteration-14 finding 2 introduced the nonce to prevent (`0016:499-527`, quoted in the
  type's own doc at `:561`). Cheap fix: take `&SegmentGroup` (or return
  `Result<_, ChunkMapError>` via `SegmentGroup::new`), and add the negative case — the
  bundle's only test here (`segment_group_adoption_is_one_bounded_range_read`,
  `crates/core/src/metadata.rs:3970`) passes the valid `NONCE` both times.

## Attacked and could not refute

- **The red→green evidence itself, on the base it ran on.** Every symbol
  `crates/custodian/tests/segmented_map_consumers.rs` names exists pre-patch, the fixture is
  raw record bytes (no new symbol), the segmented value is un-decodable pre-fix, and the
  observables are positive (`Pending`, `stranded_marked == 0`, fragments still present, bytes
  byte-identical) rather than "no error". Leg 4 genuinely stands alone (`:585`), and the
  backfill leg is driven by the one discriminating input — an empty `placement` inside a
  segmented map (`:376-389`) — so it cannot pass by short-circuit. I could not construct a
  way for these to pass pre-fix.
- **Consumer completeness.** I grepped every `chunk_map` reader under `crates/*/src` and found
  none left walking the field: the eight tabled sites plus two the brief's table omits —
  `high_water_marks` (`crates/core/src/metadata.rs:2480`) and `read_object_from`, which now
  refuses explicitly (`crates/core/src/read.rs:72`) instead of returning a short read.
- **Flat-path regression.** `resolve_chunk_map` returns a borrow for `Flat` before touching
  the store (`:1822`), so every flat consumer's read count, CAS shape and byte-identity are
  unchanged; `legacy_flat_records_round_trip_byte_identically` + the end-to-end
  `commit_chunk_map` CAS test pin the `skip_serializing_if`/CAS contract.
- **Publication envelope arithmetic.** I checked `SEGMENT_ENVELOPE_BYTES = 80` against the
  widest `u64` rendering (exactly 80), `SEGMENT_TARGET_BYTES` vs `MAX_VALUE_BYTES`, and
  `MAX_ROOT_SEGMENTS = 512` against 0016's derivation (`0016:2432-2440`, 312–520) and its
  constraint `max_segref_bytes × MAX_ROOT_SEGMENTS ≤ V/2` (`0016:1467`): the real worst-case
  `SegmentRef` encodes to ~88 B, so 512 × 88 = 45 056 ≤ 51 200 holds with margin even after
  the root's other fields. I tried to breach it via the unbounded client `content_type` and
  could not reach the 100 KB hard ceiling within HTTP header limits.
- **Repoint safety.** `ChunkHome::Segment` CASes the *raw bytes the resolve read*, not a
  re-encode, so it is strictly stronger than the flat `require(encode(prior))`; both callers
  only ever rewrite `placement`, so the `byte_len == Σ chunk.len` invariant that decode
  enforces cannot be broken by a repoint. `require(inode == prior)` on the segment arm closes
  X47 and is asserted on both interleavings in DST.
- **Scan-order independence.** `read_segments` keys a `BTreeMap` on the *parsed* index
  (`:1739-1758`), so the reversing `Shuffling` double is a real oracle, not decoration; the
  nonce/epoch re-check rejects a neighbour row even inside the bounded prefix.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Triage or reproduce the 10 reported batched-review blockers — `scripts/review-branch` and its finding output are absent, so that red row is provisional; independent affected-path merged history plus all 9 closed-unmerged PRs found no competing segmented implementation.
- [ ] Validation — fitness-to-purpose — Maintainer must accept the exact persisted JSON contract and landing the staged committer ahead of its #636 production caller — these choices become durable compatibility and sequencing commitments at `crates/core/src/metadata.rs:697` and `crates/core/src/metadata.rs:2092`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Triage or reproduce the 10 reported batched-review blockers — `scripts/review-branch` and its finding output are absent, so that red row is provisional; independent affected-path merged history plus all 9 closed-unmerged PRs found no competing segmented implementation.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
