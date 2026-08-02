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
  **ONE** new test file (leg A, the binding one — deliberately written to
  compile on this bundle's base so its RED is an assertion, not a build error), plus co-located
  unit tests (leg B) and the whole gate (leg C).
  **(A) BINDING — every maintenance consumer resolves the segmented shape, and the proof is a
  positive observable, not an absence.** `crates/custodian/tests/segmented_map_consumers.rs`
  seeds a **committed segmented object by raw record bytes** through `MetadataStore::commit`
  (the encoding is settled below, so no symbol this slice adds is named) plus its fragments on
  in-memory D-server doubles, then asserts, in one test binary:
  (i) **`reconcile_step` succeeds** with GC + scrub + reconstruction + rebalance contexts all
  supplied (`crates/custodian/src/reconciliation.rs:65-74`) — on the base it returns `Err`,
  because `referenced_fragments` decodes every `inode:` value with `metadata::decode(&value)?`
  (`crates/custodian/src/gc.rs:251`, `:256`) and a segmented value is not a JSON array;
  (ii) **the segmented object's fragments are in the protected set — asserted positively.**
  `desired_state::reconciliation_status(meta, S)` for a server `S` that holds one of the
  segmented object's fragments, with `desired:dserver:S` seeded, MUST answer **`Pending`**
  (`crates/custodian/src/desired_state.rs:150-165`). A resolver that decodes the new shape but
  never reads the `seg:` range answers `Satisfied` — this is the leg that catches it, and
  nothing else does;
  (iii) **restore does not strand it.** `reconcile_after_restore` reports
  `RestoreReport::stranded_marked == 0` (`crates/custodian/src/restore.rs:108`, `:145`, `:179`).
  This is the #508-attempt-4 failure mode in its exact shape: a resolver used only by
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
  `.chunk_map` consumer, `crates/server/src/lib.rs:446`); **`core`'s read path** resolves it
  (`crates/core/src/read.rs:92`); **reconstruction** resolves a segmented chunk for an
  **explicitly enqueued** repair instead of dropping it; and **backfill** takes its stated decision
  (resolve, or skip with a reason) rather than mangling the map (`crates/custodian/src/backfill.rs:76-130`).
  **WHERE each of these lives — this is not free choice, and getting it wrong breaks the gate.**
  `wyrd-server` depends on `wyrd-custodian` (`crates/server/Cargo.toml:63`), so the reverse edge is
  a **dependency cycle**: the gateway legs **cannot** live in the custodian test binary, and
  `wyrd-server` must not be added to `crates/custodian/Cargo.toml`. Nor may they ship as a new
  `crates/server/tests/*.rs` file — that is a second **added** test target, which C4-verify folds
  into the same cargo invocation and keeps on the RED leg, so its compile error (it names types
  this slice adds) would destroy leg A's assertion red. **The two gateway legs therefore ship as
  co-located `#[cfg(test)]` tests inside `crates/server/src/lib.rs`** (where the ranged walk lives),
  exactly as iteration 5 did. Everything else in leg A stays in the one added custodian test file:
  `wyrd-custodian` depends on `wyrd-core`, and `wyrd_core::read` (`crates/core/src/lib.rs:14`,
  entry points `read.rs:29`, `:44`, `:58`) and `metadata::high_water_marks` are public, so the core
  read path, the custodian consumers and the allocator floor are all reachable from it.
  (vii) **NEW — one damaged object does not take the store down (the containment rule, see
  `Design § Failure containment`).** Seed a THIRD object: segmented, committed, whose root still
  names its group but whose `seg:<nonce>:<epoch>:000001` record is **absent**. Then assert, in the
  same store that holds the healthy flat and healthy segmented objects:
  **(a) `metadata::high_water_marks` returns `Ok`** (`crates/core/src/metadata.rs:847`) and its
  chunk floor is **≥ every chunk id present in any `seg:` record in the store** — this is the leg
  that stops `Gateway::recover` (`crates/server/src/lib.rs:123-124`, called before serving) from
  refusing to start the whole gateway because one object is damaged;
  **(b) the healthy objects still read** — byte-identical, with the damaged object present in the
  same store (the `core::read` half in the custodian test file, the whole-object + ranged gateway
  half with the co-located server tests, per the placement rule in (vi));
  **(c) the damaged object fails closed, per object** — a read of *it* is a typed error, never
  torn or partial bytes;
  **(d) nothing of the damaged object is reclaimed** — after a GC pass and past the grace window,
  the fragments its readable segments name are still present. (Whether the deletion-capable pass
  returns `Err` or completes while protecting them is Do's call — see the containment table — but
  **no fragment may be deleted**.)
  **(B) The record shape, the CAS identity, and staged publication.** These name types this slice
  ADDS, so they **must NOT ship as a second added `tests/*.rs` file**: `run-verify.sh` collects
  every added test target into **one** cargo invocation (`engine/scripts/run-verify.sh:286-311`,
  `:332-342`) and keeps them all on the RED leg (`:404-415`), so a compile-red file would fail the
  whole invocation and **destroy leg A's assertion red** — the single most valuable thing this
  slice has. Ship leg B as **co-located `#[cfg(test)]` unit tests inside the production modules**
  they exercise (`crates/core/src/metadata.rs`, and the committer's own module), which
  `cargo xtask ci` runs and C4-verify never retains. Over `RedbMetadataStore::in_memory()`:
  (i) **Legacy decode→encode is the identity, byte-for-byte.** Take the *exact* stored bytes of a
  pre-existing flat `InodeRecord` (including one with `etag`/`content_type`/`modified` absent),
  decode and re-encode, assert equality. This is not hygiene: every CAS in
  `crates/core/src/metadata.rs` is `require(key, encode(prior))` compared byte-for-byte against
  the stored value (the `skip_serializing_if` rationale at `crates/core/src/metadata.rs:277-289`),
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
  (the segment-write batches at `≤ E_tx/2`, `0016:2331-2337`; the flip's own inventory bound
  `≤ 4·V + O(1)`, `0016:654-663`) and no single value exceeds the 100 KB ceiling
  (`crates/traits/src/lib.rs:997`) — measure the encoded bytes, do not assert a record count.
  **The operation-count half of the envelope is also normative** — "`B` is therefore
  `min(B_bytes, B_ops)` … every row of the batch inventory whose mutation or precondition **count**
  can grow is bounded by both" (`0016:640-648`) — so the split and the flip are bounded by ops as
  well as bytes, with a typed refusal for each. (Iteration 5 already implemented this; it is
  restated because an earlier round wrongly declined it.)
  (iv) **The publication refuses BEFORE it makes anything durable.** Every deterministic,
  zero-I/O refusal the committer can raise — unfenced, colliding caller contribution, a value over
  the ceiling, a key over the ceiling, batch over bytes, batch over ops — must be decided
  **before the first `seg:` record is written**, so a refused publication leaves **zero** durable
  `seg:` rows and no caller cursor movement. Assert both: a flip contribution carrying an
  over-ceiling value, and a flip with no fence, each ⇒ typed `Err` **and** a store containing no
  `seg:` record for that group. (Iteration-5 refutation 1 — the patch shipped the opposite order.)
  (v) **A resumed publication verifies the durable prefix it is trusting.** A caller resuming at
  `resume_from = N` must not be taken at its word: before the flip, the committer re-reads at least
  the last durable segment (`seg:<group>:<epoch>:<N-1>`) and compares it against the segment its
  own re-derived plan puts at that index, refusing with a typed error on mismatch. Assert the
  probe: publish attempt 1 writes N segments for chunk list A and stops; attempt 2 resumes at N
  with a chunk list differing only in `chunks[0].len` — the flip must **refuse**, not commit a
  root that no consumer can ever resolve. (Iteration-5 refutation 2. If Do concludes this belongs
  to #636's session contract instead, that is a legitimate call — but it must then be **recorded**
  in `review-rejected.md` with its reason, not left implicit.)
  (vi) **The resolver is total, bounded, and orders segments ITSELF.** It reads the root plus the
  bounded range `scan("seg:<nonce>:<epoch>:")` and nothing else — never a global `seg:` scan
  (`0016:2463-2469`). Assert by seeding a *second* group's segments in the same store and checking
  they are neither read nor returned. **And it must not rely on scan order:**
  `MetadataStore::scan` leaves order *unspecified* (`crates/traits/src/lib.rs:1021-1023`) and #634
  makes byte-lexicographic order normative **only for `scan_page`** (`:1037-1046`), leaving `scan`
  untouched — so the fixed-width zero-padded index is a *debuggability and key-hygiene* property,
  **not** a licence to concatenate in returned order. The resolver parses each segment's `index`
  and orders by it explicitly, rejecting a gap or a duplicate. Assert with a **deliberately
  shuffling** store double that returns the range reversed: resolution must still yield the correct
  byte order. (The bounded `scan` is deliberate: `MAX_ROOT_SEGMENTS` keeps one group's range inside
  the cap, so this slice does not need `scan_page` even though the base now offers it.)
  (vii) **A rolled-back attempt's segments are disjoint from a later attempt's** — seed
  `seg:<nonce>:1:*` and `seg:<nonce>:2:*` and assert resolving the root at epoch 2 returns only
  epoch 2's chunks (the F18 epoch-scoping property, `0016:2352-2380`).
  (viii) **Decision 7(h)'s resolve-retry rule, which the resolver's SIGNATURE must be able to
  express** (`0016:2452-2474`). A generation's `seg:` records are deleted by retirement and
  rollback, so a consumer midway through a segmented resolve can see a segment **absent**. The
  rule is: re-read the **root**; a root now naming a **different group** or **absent** means the
  generation was concurrently retired (a reader restarts against the current root or answers
  `NoSuchKey`; a maintenance pass drops the stale resolution); a root **unchanged** with a segment
  **absent** is an **invariant violation and MUST fail closed** — an error, never a torn success
  (the *Absent or unsupported entries* rule, `../wyrd/AGENTS.md:175-177`). **A resolver that takes
  only a store and an already-decoded `InodeRecord` cannot do this** — it has no way to re-read the
  root. So the API must carry the root's identity (the inode key/id, or a re-read closure) and
  return a retry-or-fail outcome. Assert both arms: changed root → restart/drop; unchanged root
  with a missing segment → typed error and **no partial map**. A *complete* resolve of a
  superseded generation settles the same way (the currency re-read is not skipped just because
  every segment happened to be readable). The interleaving itself (X51) goes into the existing
  `crates/dst/tests/custodian.rs`, never a new DST file (see `Test file`).
  **(C) `cargo xtask ci` green**, including the docs gates — see `Impact & compatibility` for the
  architecture-doc currency requirement, which is a **merge requirement**
  (`../wyrd/AGENTS.md:154-157`), not a follow-up.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope:
  the `Flat | Segmented` record shape and its settled encoding, the `seg:` /
  `seggrp:` records and their key helpers (`crates/core/src/metadata.rs`); the staged
  segment-write + root-flip committer, with the publication precondition taken as a **parameter**;
  the one shared resolver and **every** `.chunk_map` consumer routed through it (the eight sites
  tabled in `Design`), each with its stated failure-containment behaviour; the architecture-doc
  currency edit; and the one new test file plus co-located units. **Out of
  scope:** the multipart session/records/protocol (#636), the S3 verbs (#508), the staged-byte
  protection class (#637), `PutObject` chunk-size selection (#508 — a single PUT never segments),
  FU-1's record-shape ADR (#628), FU-5's part-record segmentation (#632), the destination
  drain-fence question carried in `review-batch.md` (see `Carry-forward`), and any file under
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 324 mutants tested in 7m: 3 missed, 164 caught, 157 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #635: add the persistent segmented chunk-map shape, staged publisher, and shared resolution path for all chunk-map consumers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit and testable: preserve legacy array bytes, reject malformed segmented shapes at decode, and resolve every consumer (`crates/core/src/metadata.rs:990`). |
| C2 Reproduction (red pre-fix) | PASS | The binding regression is causal: on the exact base its nine tests all failed assertions, while the patch makes all nine pass (`crates/custodian/tests/segmented_map_consumers.rs:510`). |
| C3 Change | PASS | The change stays inside the declared data/docs/test slice and centralizes maintenance resolution without expanding into the multipart protocol or session model (`crates/custodian/src/resolve.rs:53`). |
| C4 Verification (red→green) | PASS | Independent red→green was 9 failures→9 passes; CI constituents, conformance, statics, DST, and cargo-deny all passed after moving cargo-deny's read-only advisory-cache lock into scratch (`crates/custodian/tests/segmented_map_consumers.rs:510`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The rebuild must authenticate every trusted durable segment, not only `resume_from - 1`: a same-length earlier-ID change committed a mixed old/new map because segment boundaries stayed unchanged (`crates/core/src/metadata.rs:2997`). |
| T1 Structure | PASS | One core resolver owns flat/segmented semantics and the custodian adapter preserves live-generation identity, avoiding consumer-specific interpretation (`crates/core/src/metadata.rs:2198`). |
| T2 Shape | PASS | The persisted shape preserves legacy-array identity and validates count, order, and contiguous spans during decode, protecting bytewise CAS compatibility (`crates/core/src/metadata.rs:976`). |
| T3 Runtime | NEEDS-HUMAN | The maintainer must accept landing a `Completing`-less precursor committer before #636 supplies the real session fence — this determines whether an otherwise unreachable persistence API should ship now (`crates/core/src/metadata.rs:2540`). |
| T4 Contribution | NEEDS-HUMAN | A human must inspect and triage the reported six batch-review items — the target lacks `scripts/review-branch` and its output is unavailable, so their novelty and validity cannot be independently settled; affected-path prior art was mechanically clear. |
| T5 Judgment | NEEDS-HUMAN [impl] | The rebuild must add allocator-boundary coverage: all three independently rerun survivors replace the `< 2^64` comparator, so the tests do not protect the ID-space boundary whose failure can undercount or misclassify persisted IDs (`crates/core/src/metadata.rs:3473`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether this fixed on-disk encoding and precursor API are fit for the >10 GiB roadmap before #636 creates segmented maps — the choice locks durable compatibility for later writers (`docs/design/architecture/08-crosscutting-concepts.md:83`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Method note: I did **not** re-execute the red→green. `cargo` is present, but `$PDCA_TARGET`
is read-only (its 63 GB `target/` is the only warm cache) and a cold rebuild of the
workspace in scratch was disproportionate to the budget. Everything below is grounded by
reading the target source at `$PDCA_TARGET` with the patch applied; where a claim depends
on execution I say so. This is a *scope* choice, not a missing toolchain, so it does not
change any verdict below.

## Refutations

- **NEEDS-HUMAN [impl] — `high_water_marks` is still not total against the record class this
  patch introduces, so the containment leg proves less than it claims.**
  `crates/core/src/metadata.rs:3461` still decodes every `inode:` value with a strict `?`,
  while `crates/core/src/metadata.rs:1421-1428` (`SizeSpanMismatch`) and
  `SegmentedMap::new`/`SegmentGroup::new` (`:900-925`, `:828-838`) make a whole family of
  *new* segmented-root shapes undecodable. Concrete failing case: store
  `inode:4 = {"size":1,"chunk_map":{"group":{"nonce":"<32 hex>","epoch":1},"segment_count":1,
  "segments":[{"index":0,"byte_offset":0,"byte_len":2}]},"state":"Committed","version":1}`
  (size 1, table spans 2 — or equally an uppercase nonce digit, or a `segment_count` that
  disagrees). `high_water_marks` returns `Err`, `Gateway::recover`
  (`crates/server/src/lib.rs:124`) returns `Err`, and every *healthy* object in the store
  loses availability — the exact blast radius the brief's containment table calls
  impossible ("No arrangement of store contents may make it return `Err`"). Leg A(vii)(a)
  (`crates/custodian/tests/segmented_map_consumers.rs:1107`) and the co-located
  `the_id_floor_is_total_over_a_damaged_segmented_object`
  (`crates/core/src/metadata.rs:6442`) only seed the one damage mode where the root itself
  decodes cleanly, so both pass while the property fails. Not a regression from base (a
  segmented value fails `decode` there too) — but establishing this property is the *only*
  reason leg A(vii)(a) exists, and it is not established. The fix is mechanical and
  provably lossless here: `recover` consumes only `max_inode`, which is parsed from the
  **key** at `:3458`, so skipping-and-attributing an undecodable `inode:` value — the shape
  the patch already chose one loop down at `:3397` — costs the floor nothing.

- **NEEDS-HUMAN [human] — the new full-namespace `seg:` walk feeds a value no production
  caller reads.** `segment_chunk_floor` (`crates/core/src/metadata.rs:3389-3419`) pages
  through and JSON-decodes **every** `seg:` record in the store on each call. Its only
  consumer is `high_water_marks`'s `max_chunk`, and the only production caller of
  `high_water_marks` discards it: `crates/server/src/lib.rs:124` binds `_max_chunk` (also on
  base — chunk ids are coordination-free per ADR-0019, as the doc comment there states).
  So this slice adds, to every gateway start, a decode of the entire segment corpus
  (~50 KB per record, up to `MAX_ROOT_SEGMENTS`=512 records per 10 GiB-class object —
  ~25 MB per object, per start) whose result is dropped, and leg A(vii)(a)'s "floor ≥ every
  `seg:` id" assertion (`crates/custodian/tests/segmented_map_consumers.rs:1120`) pins a
  number nothing consumes. Related: the brief's containment row also demands the floor
  "never under-approximate", yet `:3397` skips an unreadable `seg:` record and the
  co-located test *enshrines* that departure (`crates/core/src/metadata.rs:6503-6517`)
  without a `review-rejected.md` entry. Human call: keep the floor (and record the
  under-approximation rejection), or stop computing `max_chunk` and delete the walk.

- **NEEDS-HUMAN [impl] — `reconciliation_status` gets the wrong containment arm, and nothing
  tests it.** The brief's containment table settles this row as the `PendingMalformed`
  shape ("refuse to certify, **attribute** the blocker, keep going"). As shipped,
  `crates/custodian/src/desired_state.rs:157` calls `referenced_fragments`, which propagates
  the resolver's `Err` at `crates/custodian/src/gc.rs:265`. Concrete failing case: seed the
  damaged object of `seed_damaged` (`crates/custodian/tests/segmented_map_consumers.rs:457`)
  plus `desired:dserver:5`, then call `reconciliation_status(meta, 5)` for a server that
  holds nothing of the damaged object — it returns `Err` instead of
  `PendingMalformed { chunks }`, i.e. the operator's drain surface goes dark **store-wide**
  for every server because one object is damaged, and the blocker is never attributed to an
  inode. No test covers it: leg 2 (`:707`) runs on a healthy store only, and leg 7 (`:1098`)
  never calls `reconciliation_status`. Either implement the row (an "unresolvable" bucket
  beside `ReferenceSet::malformed`, `gc.rs:241-247`) or record-reject it with a reason.

- **NEEDS-HUMAN [impl] — leg A(vii)(d) is vacuous: it passes on the pre-fix base and cannot
  fail.** At `crates/custodian/tests/segmented_map_consumers.rs:1171-1181` both passes are
  invoked as `let _ = reconcile_after_restore(...)` / `let _ = reconcile_step(...)`. With the
  damaged object present, `referenced_fragments` (`crates/custodian/src/gc.rs:265`) and
  `committed_chunks` (`crates/custodian/src/restore.rs:375-386`) both return `Err` at the
  *first* segmented record, so no pass ever reaches a reclamation decision — and on the base
  the same call errors at `metadata::decode`. The subsequent "fragments still present"
  assertions (`:1183-1197`) therefore hold for any implementation that errors anywhere, and
  contribute nothing to the red. As written the leg cannot distinguish "an incomplete
  reference set authorized no reclamation" from "nothing ran at all". Pin the arm the pass
  actually took (assert the `Err` type, or assert that a store *without* the damaged object
  does reclaim on the same fixture) so the assertion has a way to fail.

- **NEEDS-HUMAN [impl] — the C5 survivors are in the loop this patch restructured and are a
  one-line fixture away.** All three missed mutants sit on
  `crates/core/src/metadata.rs:3473` (`chunk.id < IN_PROCESS_CHUNK_CEILING`, now nested
  under the new `if let ChunkMap::Flat` at `:3471`): `<` → `>`, `==`, `<=` all survive, i.e.
  no test in the suite pins that a **flat** record's sub-2^64 chunk id raises the floor
  while an above-2^64 one does not. `the_high_water_scan_sees_segmented_ids_and_ignores_the
  _out_of_range_ones` (`:6362`) covers the segmented half only. A single record carrying one
  id below and one above the ceiling closes it.

- **NEEDS-HUMAN [human] — a ranged GET now reads the whole map, and the encoding's stated
  reason for `byte_offset`/`byte_len` is unused.** `get_object_range`
  (`crates/server/src/lib.rs:447-479`) resolves the *entire* chunk list through
  `resolve_live_chunk_map` before trimming to the requested span, so a 1-byte
  `Range: bytes=0-0` on a max-size segmented object reads all 512 `seg:` records
  (~25 MB of metadata) to return one byte. The root's segment table exists precisely so
  that "which segment covers byte N" is answerable without reading a segment record (brief,
  *Design § the settled record encoding*), but `SegmentRef::byte_offset`/`byte_len` are read
  nowhere except the validation checks at `crates/core/src/metadata.rs:908` and `:2111` — no
  consumer selects by span. The brief did mandate the ranged walk go through the one
  resolver, so this is a fitness-to-purpose call for sign-off (resolver API gains a byte-range
  arm now, or the amplification is accepted and tracked), not a builder slip.

## Attempted and could not refute

- **Flat byte-identity (the CAS contract).** `Serialize for ChunkMap`
  (`crates/core/src/metadata.rs:1057-1064`) emits `Flat` as the bare array, `InodeRecord`
  keeps its derived field order and `skip_serializing_if` (`:1358-1398`), and the new
  `InodeRecordWire` (`:1403-1415`) is field-for-field identical with no
  `deny_unknown_fields`. I could not construct a legacy record whose decode→encode moves.
- **A `.chunk_map` consumer left un-routed** (the recorded #508-attempt-4 failure). A
  workspace-wide grep leaves no production reader of the field outside the resolver except
  `backfill.rs:163`'s deliberate `is_segmented()` skip and `write.rs:274`'s construction.
- **Fooling the bounded range read.** Nonce/epoch prefix confusion is blocked by the fixed
  32-char nonce plus the trailing `:` in `seg_range_prefix` (`:1267`); a foreign row inside
  the range is rejected at `:2088`; ordering never trusts `scan` (BTreeMap keyed by the
  parsed index, `:2082`,`:2147`); an index past the table fails closed at `:2126`.
- **Writing a `seg:` record before a deterministic refusal** (iteration-5 refutation 1).
  `publish` assembles both phases before any I/O (`:3069-3070`) and `verify_resume_prefix`
  is the only pre-write read (`:2997-3026`); I found no path from `publish` that commits a
  segment ahead of an `Unfenced`/`ContributionCollides`/ceiling/envelope refusal. (Note only:
  `write_segments` (`:2971`) does not validate the flip, so a caller composing the two phases
  by hand re-opens that order — out of this diff's control.)
- **Retirement/supersede races.** `root_still_names` (`:2153`) settles a flat, absent or
  re-grouped root as `Retired`; `resolve_live_*` re-resolve the replacement rather than
  answering "no chunks" (`:2252-2281`, `:2345-2374`); `repoint_chunk` binds the home's group
  to the live root before building either precondition (`:2463-2482`). I could not build an
  interleaving that yields a torn map or a silently-lost repoint.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The rebuild must authenticate every trusted durable segment, not only `resume_from - 1`: a same-length earlier-ID change committed a mixed old/new map because segment boundaries stayed unchanged (`crates/core/src/metadata.rs:2997`).
- [ ] T3 Runtime — The maintainer must accept landing a `Completing`-less precursor committer before #636 supplies the real session fence — this determines whether an otherwise unreachable persistence API should ship now (`crates/core/src/metadata.rs:2540`).
- [ ] T4 Contribution — A human must inspect and triage the reported six batch-review items — the target lacks `scripts/review-branch` and its output is unavailable, so their novelty and validity cannot be independently settled; affected-path prior art was mechanically clear.
- [ ] T5 Judgment — The rebuild must add allocator-boundary coverage: all three independently rerun survivors replace the `< 2^64` comparator, so the tests do not protect the ID-space boundary whose failure can undercount or misclassify persisted IDs (`crates/core/src/metadata.rs:3473`).
- [ ] Validation — fitness-to-purpose — The maintainer must decide whether this fixed on-disk encoding and precursor API are fit for the >10 GiB roadmap before #636 creates segmented maps — the choice locks durable compatibility for later writers (`docs/design/architecture/08-crosscutting-concepts.md:83`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — C5 Causal adequacy — The rebuild must authenticate every trusted durable segment, not only `resume_from - 1`: a same-length earlier-ID change committed a mixed old/new map because segment boundaries stayed unchanged (`crates/core/src/metadata.rs:2997`).; T3 Runtime — The maintainer must accept landing a `Completing`-less precursor committer before #636 supplies the real session fence — this determines whether an otherwise unreachable persistence API should ship now (`crates/core/src/metadata.rs:2540`).; T4 Contribution — A human must inspect and triage the reported six batch-review items — the target lacks `scripts/review-branch` and its output is unavailable, so their novelty and validity cannot be independently settled; affected-path prior art was mechanically clear.; T5 Judgment — The rebuild must add allocator-boundary coverage: all three independently rerun survivors replace the `< 2^64` comparator, so the tests do not protect the ID-space boundary whose failure can undercount or misclassify persisted IDs (`crates/core/src/metadata.rs:3473`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
