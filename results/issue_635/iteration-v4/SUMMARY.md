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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 287 mutants tested in 7m: 138 caught, 149 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #635: add a backward-compatible segmented inode chunk map, staged publication, and shared resolution across all read and maintenance consumers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is explicit and falsifiable: preserve flat bytes, enforce the pinned segmented encoding, publish under a staged fence, and resolve through every consumer. |
| C2 Reproduction (red pre-fix) | FAIL | The required #634-stack red leg must execute assertions, but it compiles zero tests because the new `MemMeta` implementation omits `scan_page`; stale main does produce a genuine 0/8 assertion red, but it is not the stipulated stack base (`crates/custodian/tests/segmented_map_consumers.rs:83`). |
| C3 Change | PASS | The patch stays on the planned data surface—record shape, core resolver/committer, consumer routing, tests, and living architecture—with core retaining ownership of the shared representation (`crates/core/src/metadata.rs:881`, `crates/core/src/metadata.rs:2026`, `docs/design/architecture/06-runtime-view.md:24`). |
| C4 Verification (red→green) | FAIL | The #634 stack cannot be accepted because both red and green legs stop at E0046 before the binding tests run; main-only CI passed typos/docs/fmt/clippy/build/tests and mutation testing, while its advisory check remained provisional due a read-only cargo-database lock (`crates/custodian/tests/segmented_map_consumers.rs:83`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must route the public snapshot read through the resolver or remove it as a segmented-map consumer—the method explicitly rejects segmented records, leaving one read path opaque despite the every-consumer invariant (`crates/core/src/read.rs:72`). |
| T1 Structure | PASS | Core owns resolution and homed repoints while the custodian layer delegates to it, preserving dependency direction and a single maintenance seam (`crates/core/src/metadata.rs:2026`, `crates/custodian/src/resolve.rs:53`). |
| T2 Shape | PASS | The persisted shape uses JSON-type discrimination, strict decode invariants, parsed-index ordering over a group-scoped range, and caller contributions in the fenced flip batch (`crates/core/src/metadata.rs:943`, `crates/core/src/metadata.rs:1927`, `crates/core/src/metadata.rs:2656`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must accept precursor-only runtime evidence—the real #636 `Completing@E` caller is absent, so publication safety was exercised only with test-supplied segment and flip contributions; this matters because those contributions carry the production fence and atomic session mutations (`crates/core/src/metadata.rs:2379`, `crates/core/src/metadata.rs:2392`). |
| T4 Contribution | NEEDS-HUMAN | A human must triage the eight reported batch-review blockers because `scripts/review-branch --bundle` and its result artifact are unavailable here; independent affected-path history plus open/closed PR searches found no competing implementation, only the proposal and the earlier object-metadata shape change. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must restore #634 stack evaluability and close the remaining snapshot-read gap; a main-only green run cannot establish implementation sign-off while the normative stack executes zero binding tests (`crates/custodian/tests/segmented_map_consumers.rs:83`, `crates/core/src/read.rs:72`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must accept this as a precursor whose real producer/retirement wiring lands later: segmented unlink and overwrite currently fail closed, so the decision affects whether the slice is useful and safely sequenced before #636 (`crates/core/src/metadata.rs:1517`, `crates/core/src/metadata.rs:1637`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (iteration 4)

Advisory only; I never gate. Every `path:line` below is on the target source at
`$PDCA_TARGET` (`/home/eddie/development/wyrd/wyrd.pdca-wt-l0`, HEAD `b0cd199` + the staged patch).

## What I could not refute

- **The red→green is genuine, and I reproduced it independently.** I materialised the
  pre-fix tree (`git archive HEAD`) into scratch, dropped in only
  `crates/custodian/tests/segmented_map_consumers.rs`, and ran it: **8/8 fail**, and they fail
  as *assertions/decode errors* (`Error("invalid type: map, expected a sequence", line: 1,
  column: 23)` propagating out of `reconcile_step`, `reconciliation_status`,
  `reconcile_after_restore`, `rebalance`, `reconstruction`, `read_object`, `backfill`), not as
  a build error — exactly what the brief's Falsifiability clause demands. Post-fix the same
  8/8 pass. The test is not a parallel re-implementation: it seeds raw record bytes and drives
  the real `wyrd_custodian::reconcile_step` / `reconciliation_status` / `reconcile_after_restore`
  and `wyrd_core::read::read_object`.
- **The iteration-3 carry-forward findings are actually fixed, not papered over.** The per-value
  ceiling is now charged on the *assembled* flip batch (`crates/core/src/metadata.rs:2668`,
  `check_record_ceilings` at `:2759`), and it is asserted adversarially against a
  **caller-contributed** 100 001-byte value and a 10 001-byte key, with the inclusive boundary
  pinned on both sides (`crates/core/src/metadata.rs:3896-3985`). I could not get an oversize
  record past either builder.
- Attacked and failed to break: `seg:` vs `seggrp:` prefix disjointness; epoch-prefix confusion
  (`seg:<n>:1:` vs epoch 10 — the trailing colon closes it, `crates/core/src/metadata.rs:1125`);
  scan-order independence (`read_segments` keys a `BTreeMap` on the *parsed* index, `:1930`,`:1949`);
  the `SegmentUnknown` / `SegmentAbsent` / `Retired` trichotomy of decision 7(h) (`:1985-2007`);
  the sibling-repoint CAS race in a segmented map; `u64` overflow in `span()`/`checked_chunk_bytes`;
  and the claim that every `.chunk_map` consumer is routed — `grep '\.chunk_map'` over `crates/*/src`
  leaves only the three deliberate shape *predicates*.

## Refutation attempts that landed

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:1919` documents a `≤ MAX_ROOT_SEGMENTS`
  bound that nothing on the read path enforces, so "bounded resolution" rests on the publisher
  alone.** `MAX_ROOT_SEGMENTS` is a publication-time guard only, by explicit choice
  (`crates/core/src/metadata.rs:284-288`), and `SegmentedMap::new` (`:770-801`) validates
  index order, contiguity and overflow but **not** the count. Concrete case I ran: a raw
  `inode:` value naming **1 536** segments (3× the ceiling) is **70 128 bytes** — comfortably
  inside `MAX_VALUE_BYTES` — and `metadata::decode::<InodeRecord>` accepts it. `read_segments`
  (`:1927-1985`) then issues one unbounded `scan` of that group's range and decodes every row;
  a stored `SegmentRecord` is itself uncapped at decode (I decoded a 45 833-byte one), so a
  single object can force ~1 536 × `MAX_VALUE_BYTES` ≈ 150 MB into one `Vec` — and this runs
  **once per committed inode** in `gc.rs:265`, `restore.rs:383`, `rebalance.rs:168`,
  `reconstruction.rs:617`, `backfill.rs:109`/`:248` and in `high_water_marks` at gateway
  recovery (`crates/core/src/metadata.rs:2953`). Either fail closed in `read_segments` when
  `map.segment_count() as usize > MAX_ROOT_SEGMENTS` (the *Absent or unsupported entries*
  posture the module takes everywhere else) or drop the "bounded (≤ MAX_ROOT_SEGMENTS)" wording
  from `:1919`. The doc and the code currently disagree.

- **NEEDS-HUMAN [human] — leg B(i)'s byte-identity oracle omits the one legacy shape that
  breaks it, so the brief's binding compatibility claim is broader than what was proved.**
  `crates/core/src/metadata.rs:3096-3110` round-trips three fixtures and every one of them
  carries `"placement":[…]`. A genuine pre-M3 record omits the field — `ChunkRef.placement` is
  `#[serde(default)]` with **no** `skip_serializing_if` (`crates/core/src/metadata.rs:137-139`)
  — and this patch's own `empty_placement_is_valid_pre_m3_identity` (`:3016-3021`) asserts such
  records exist. I ran it: `{"size":5,"chunk_map":[{"id":1,"scheme":{"ReedSolomon":{"k":2,"m":1}},"len":5}],"state":"Committed","version":3}`
  re-encodes as `…,"len":5,"placement":[]}…` — **not** byte-identical, so
  `require(key, encode(prior))` against that stored value is a permanent `Conflict`, which is
  precisely the failure the brief made leg B(i) binding on (and it means backfill can never
  fill a pre-M3 record either). The serde attribute is **pre-existing and outside this diff**,
  so I am not asking for a fix here; what is unwarranted is the claim in the brief / the test's
  own message ("decode→encode must be the identity on a pre-segmentation record") — it holds
  only for records that already carry `placement`. A human should decide whether to scope the
  claim, add the failing fixture with an `#[ignore]`+issue, or file the underlying hazard.

- **NEEDS-HUMAN [human] — the bundle is built on `origin/main`, not the wave-1 base the brief
  declares, so the "#634 is a file conflict, not a dependency" premise is untested here.**
  The brief's *Falsifiability* section requires the added test's `MemMeta` to implement
  `scan_page` ("#634's required method, one delegating line"). On this target
  `crates/traits/src/lib.rs:767-776` has **no** `scan_page`, and
  `crates/custodian/tests/segmented_map_consumers.rs:83-113` implements only
  `get`/`scan`/`commit` — as do every double this patch adds
  (`crates/core/src/metadata.rs:4381`, `:4674`, `:5083`; `crates/server/src/lib.rs:882`;
  `crates/dst/tests/custodian.rs:97`, `:198`). That is the E0046 the *Iteration 1*
  carry-forward already recorded, and nothing in this patch addresses it — nor can the builder,
  since adding `scan_page` to an impl of a trait that lacks the method is E0407 on this base.
  So C4's `xtask ci` green, C4-verify's red→green and my own reproduction are all against a base
  the brief says is not the merge target; the stack-green verdict remains **provisional**.

- **NEEDS-HUMAN [human] — backfill now turns a record shape the repo elsewhere calls *valid*
  into a permanent, whole-pass failure.** `crates/custodian/src/backfill.rs:163-169` and `:217`
  make `reconcile` return `Err(SegmentedPlacementUnfillable)` whenever any committed segmented
  record carries a chunk with an empty `placement`. But an empty placement is explicitly a
  **valid** committed placement in this codebase — `ChunkRef::placement_is_valid` /
  `checked_fragments` accept it and the identity fallback resolves it
  (`crates/core/src/metadata.rs:3016-3021`, ADR-0040 decision 3) — so GC, restore, rebalance,
  reconstruction and both read paths all handle such an object happily while backfill red-lines
  on it on **every pass, forever**, with no path in the system that can ever clear the
  condition. The brief asked for "resolve, or skip with a stated reason"; the patch took a third
  option, argued from `AGENTS.md:175-177` — but that rule's other sanctioned arm is *enqueue a
  repair obligation*, and nothing here is absent or unsupported: the entry resolves fine. Blast
  radius is bounded today only because `reconcile_step` does not dispatch backfill
  (`crates/custodian/src/reconciliation.rs:65-114`) and no production loop calls it. This is a
  scope/fitness call: is a permanently-failing maintenance pass the right answer for a legal
  record, or should it be a queued obligation?

- **NEEDS-HUMAN [impl] — the one assertion in the *added* (red-leg) test file that claims the
  sweep-then-fail ordering is vacuous.** `crates/custodian/tests/segmented_map_consumers.rs:993-1003`
  comments "it is reached at all, which is why the failure is raised after the sweep rather than
  at the first record", but the oracle is only "the flat record's bytes are unchanged" — and
  that fixture's flat object has full-length placements, i.e. nothing to fill, so the assertion
  holds identically if `reconcile` had returned at the *first* segmented record. `MemMeta::scan`
  iterates a `HashMap` (`:86-97`), so the visit order is not even fixed. The property *is*
  genuinely pinned — at `crates/custodian/tests/backfill_telemetry.rs:222-231`, with two
  fillable flat records — but that file is **modified**, not added, so it never runs on the
  C4-verify red leg. Give the added file a fillable flat record so the ordering claim in the one
  file that carries the binding red is actually load-bearing.

## Note on the verdict rows

`check-gates.json` reports `overall: fail` with **T4 batched review red at 8 blocking** — the
same count as the *Iteration 3* carry-forward. `scripts/review-branch --bundle` and its result
artifact are not reachable from this leaf, so I can neither reproduce nor triage those eight;
I flag only that the count has not moved between iterations, which is not what a resolved batch
of blockers looks like. That is a human triage item, not a refutation.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must route the public snapshot read through the resolver or remove it as a segmented-map consumer—the method explicitly rejects segmented records, leaving one read path opaque despite the every-consumer invariant (`crates/core/src/read.rs:72`).
- [ ] T3 Runtime — Maintainers must accept precursor-only runtime evidence—the real #636 `Completing@E` caller is absent, so publication safety was exercised only with test-supplied segment and flip contributions; this matters because those contributions carry the production fence and atomic session mutations (`crates/core/src/metadata.rs:2379`, `crates/core/src/metadata.rs:2392`).
- [ ] T4 Contribution — A human must triage the eight reported batch-review blockers because `scripts/review-branch --bundle` and its result artifact are unavailable here; independent affected-path history plus open/closed PR searches found no competing implementation, only the proposal and the earlier object-metadata shape change.
- [ ] T5 Judgment — Rebuild must restore #634 stack evaluability and close the remaining snapshot-read gap; a main-only green run cannot establish implementation sign-off while the normative stack executes zero binding tests (`crates/custodian/tests/segmented_map_consumers.rs:83`, `crates/core/src/read.rs:72`).
- [ ] Validation — fitness-to-purpose — Maintainers must accept this as a precursor whose real producer/retirement wiring lands later: segmented unlink and overwrite currently fail closed, so the decision affects whether the slice is useful and safely sequenced before #636 (`crates/core/src/metadata.rs:1517`, `crates/core/src/metadata.rs:1637`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must route the public snapshot read through the resolver or remove it as a segmented-map consumer—the method explicitly rejects segmented records, leaving one read path opaque despite the every-consumer invariant (`crates/core/src/read.rs:72`).; T3 Runtime — Maintainers must accept precursor-only runtime evidence—the real #636 `Completing@E` caller is absent, so publication safety was exercised only with test-supplied segment and flip contributions; this matters because those contributions carry the production fence and atomic session mutations (`crates/core/src/metadata.rs:2379`, `crates/core/src/metadata.rs:2392`).; T4 Contribution — A human must triage the eight reported batch-review blockers because `scripts/review-branch --bundle` and its result artifact are unavailable here; independent affected-path history plus open/closed PR searches found no competing implementation, only the proposal and the earlier object-metadata shape change.; T5 Judgment — Rebuild must restore #634 stack evaluability and close the remaining snapshot-read gap; a main-only green run cannot establish implementation sign-off while the normative stack executes zero binding tests (`crates/custodian/tests/segmented_map_consumers.rs:83`, `crates/core/src/read.rs:72`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
