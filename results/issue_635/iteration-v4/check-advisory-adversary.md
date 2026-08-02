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
