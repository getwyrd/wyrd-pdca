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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 350 mutants tested in 9m: 187 caught, 163 unviable

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

Reviewing issue #635: add byte-identical flat-or-segmented inode chunk maps, staged publication, and shared resolution across readers and maintenance consumers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The target proposal supplies an unambiguous oracle for the two-form record, bounded consumer resolution, and retirement-safe retries (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2314`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2393`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2452`). |
| C2 Reproduction (red pre-fix) | PASS | On the exact `9120f7a` base, the binding target compiled, ran nine tests, and failed all nine on runtime/assertion decode paths rather than a build error (`crates/custodian/tests/segmented_map_consumers.rs:566`). |
| C3 Change | PASS | The scoped representation, committer, shared consumer resolution, and architecture-doc currency are present without manifest, ADR, or specification expansion (`crates/core/src/metadata.rs:2201`, `crates/core/src/metadata.rs:2531`, `docs/design/architecture/06-runtime-view.md:32`). |
| C4 Verification (red→green) | PASS | The independent exact-base run moved from 0/9 passing to 9/9 passing, and `cargo xtask ci` completed with the real `typos`, docs renderer, denial, and simulation gates green (`crates/custodian/tests/segmented_map_consumers.rs:566`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must page allocator-floor scans and add a lowered-cap Redb regression: although all 350 diff mutants reproduced as 187 caught/163 unviable, three orphan rows against a cap of two returned `ScanCapExceeded`, and this namespace can exceed the production cap (`crates/core/src/metadata.rs:3628`, `crates/traits/src/lib.rs:1029`). |
| T1 Structure | PASS | Core owns the single resolver while custodian adapts it for maintenance consumers, preserving inward dependencies without a server/custodian cycle or duplicate resolution logic (`crates/core/src/metadata.rs:2201`, `crates/custodian/src/resolve.rs:53`). |
| T2 Shape | PASS | Legacy flat encoding remains untagged and segmented structural invariants reject at decode, preserving byte-sensitive CAS identity and preventing half-valid persisted maps (`crates/core/src/metadata.rs:983`, `crates/core/src/metadata.rs:996`, `docs/design/architecture/08-crosscutting-concepts.md:83`). |
| T3 Runtime | FAIL | Gateway recovery can still deny healthy-object availability: a real Redb store with cap two and three orphan rows failed at the unpaged orphan scan, contradicting the promised total startup floor (`crates/core/src/metadata.rs:3559`, `crates/core/src/metadata.rs:3628`, `crates/server/src/lib.rs:123`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect and disposition the six reported batch-review items: the asserted `scripts/review-branch` and its itemized output are absent, so their validity cannot be rerun; affected-path merged and closed/unmerged prior-art checks found no competing implementation. |
| T5 Judgment | NEEDS-HUMAN | The maintainer must accept or reject landing the fence-parameter publication precursor before #636 supplies a production `Completing` producer, because the persistence API otherwise ships without a production caller (`crates/core/src/metadata.rs:2531`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether this precursor is fit for the >10 GiB launch goal before #636/#508 supplies the real multipart path; after integration, exercise a beyond-flat-ceiling multipart upload, whole/range reads, reconciliation, and fragment retention (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2311`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Attacked: the leg-A red→green claim, the staged-publication guard that carry-forward item 3
added this round, the containment table's totality row, and the resolver's fail-closed
arms. Two refutations land, two are judgment calls for the human. Findings are grounded on
the target tree at `$PDCA_TARGET` (patch applied in the worktree).

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3010` (`verify_resume_prefix`) with
  `:2129` (`read_segments`' `SegmentUnknown` arm): a same-epoch republication whose
  re-derived plan is SHORTER than a previous attempt's publishes a `Committed` root that no
  consumer can ever resolve.** The guard added this round verifies only `planned[..resume_from]`
  (`:3037`) and short-circuits entirely at `resume_from == 0` (`:3011`); nothing anywhere in the
  committer deletes `seg:<nonce>:<epoch>:<i>` rows for `i >= planned.len()` — `assemble_segment_batches`
  (`:2875`) and `flip_batch` (`:2931`) emit puts only — and the caller cannot delete them
  either, because `merge_contribution` refuses any contribution that names the group's range
  (`:3299`, `OwnedKeys::owner_of` `:3171`). `read_segments` then fails closed on the orphaned
  tail (`:2129`), permanently. **Reproduced** against the patched tree (scratch crate over
  `wyrd-core`, in-memory `MetadataStore`): attempt 1 `write_segments` a 1200-chunk list (2
  segments) at `(nonce, epoch=1)` and crash before the flip; attempt 2 `publish` a 300-chunk
  list (1 segment) at the *same* `(nonce, epoch)` with `resume_from = 0` →
  `publish` returns `Ok(Committed)`, and afterwards
  `resolve_chunk_map` / `resolve_live_chunk_map` both return
  `segment 1 exists under seg:0123…:1 but the root does not name it` — every read, GC,
  restore, rebalance and reconstruction pass over that store now fails, forever. This is
  exactly the "silent at publication, terminal at read" shape leg B(v) / carry-forward item 3
  exists to prevent; the patch closed the *differing-prefix* half of it and left the
  *shorter-plan* half open. Cheapest fix in the same place: after building `planned`, refuse
  (or delete in the flip batch) any durable index `>= planned.len()` in the group's own range —
  `verify_resume_prefix` already has that range in hand at `:3027`.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3559-3560`: the totality claim the
  containment table turns on is unwarranted as written.** The doc asserts "No arrangement of
  store contents makes it return `Err`" (echoed verbatim in
  `docs/design/architecture/06-runtime-view.md:29`, "That floor is **total**: no arrangement of
  stored records can make it refuse"), but three of the four walks in the body are unpaged
  `scan`s that fail loud at `SCAN_CAP` (`crates/traits/src/lib.rs:286`, `1 << 20`):
  `:3571` (`inode:`), `:3617` (`pending:`), `:3628` (`orphan:`). Concrete case: a store with
  more than 1 048 576 inode records — or an orphan ledger past the same cap, and this patch's
  own comment at `:1664` prices one segmented retirement at "~1.78 M fragment orphans" —
  makes `high_water_marks` return `Err(ScanCapExceeded)`, so `Gateway::recover`
  (`crates/server/src/lib.rs:123-124`) refuses to start and every healthy object loses
  availability. That is precisely the blast radius the containment row was written against.
  The `seg:` half was correctly paged (`segment_chunk_floor`, `:3490`) for exactly this
  reason, which makes the remaining three the inconsistency. The scans are pre-existing; the
  absolute claim is new, so the minimum honest fix is to bound the claim ("total against an
  undecodable record") or to page the other three the way `:3490` already shows.

## Judgment calls

- **NEEDS-HUMAN [human] — `crates/server/src/lib.rs:448` + `:479`: the ranged read resolves
  the entire map, discarding the root's own byte index.** The settled encoding gives every
  `SegmentRef` a `byte_offset`/`byte_len` precisely "so the root alone answers 'which segment
  covers byte N' without reading any segment record" (brief, *The settled record encoding*).
  `get_object_range` instead calls `resolve_live_chunk_map` (`:448`) and only then selects
  covering chunks (`:479`). At the shipped ceilings (`MAX_ROOT_SEGMENTS = 512`,
  `SEGMENT_TARGET_BYTES = 50 000`, `metadata.rs:288`,`:330`) a 1-byte `Range:` GET on a
  maximal object reads ~25 MB of `seg:` records and materialises a ~190 000-element
  `Vec<ChunkRef>` before sending a byte. Correct, but the affordance the design added for it
  is unused; whether a range-scoped resolver belongs here or in #508 is a scope decision, not
  a builder nit.

- **NEEDS-HUMAN [human] — `crates/custodian/src/gc.rs:265` (via
  `crates/core/src/metadata.rs:2181`): one damaged segmented object halts the whole
  maintenance plane indefinitely, not just itself.** `chunks_of`'s `?` propagates
  `SegmentAbsent` out of `referenced_fragments`, which is the single reference build behind GC
  (`gc.rs:132`), restore (`restore.rs:183`), scrub (`scrub.rs:75`) and `reconciliation_status`
  (`desired_state.rs:157`) — so until an operator repairs that one record, *no* object's
  garbage is ever reclaimed anywhere in the store and no drain can ever be certified. The
  brief's containment table pre-authorises this ("Aborting the pass is acceptable"), and the
  brief's own invariant ("Its failure, when it does fail, is scoped to the object that
  failed") argues for the other permitted shape (continue, treating the damaged object as
  fully referenced). Cheap confirm-or-redirect at sign-off; note also that leg A(vii)(d)
  (`crates/custodian/tests/segmented_map_consumers.rs:1171-1188`) discards both pass results
  with `let _ =`, so it cannot distinguish "protected" from "aborted before doing anything".

- **NEEDS-HUMAN [impl] — the T4 gate is red for the sixth round on the same cause**
  (`check-gates.json`: "review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped"), and
  the brief's carry-forward item 5 gives the exact disposition and format for five of them
  (record-reject in `review-rejected.md` as `<file:line> | <CLASS> | <MATCH> | <reason>`,
  citing the flat peer `crates/custodian/src/rebalance.rs:274-296`, maintainer-confirmed at
  Plan). Not a diff finding — raised because it is mechanically dischargeable by the builder
  and is the sole gating failure in the bundle.

## Attempted and could not refute

- **Flat byte-identity (leg B(i)).** `ChunkMap::serialize` delegates the flat arm straight to
  `Vec<ChunkRef>` (`metadata.rs:1063`) and `#[serde(try_from = "InodeRecordWire")]`
  (`:1362`) affects only `Deserialize`, so decode→encode on a legacy record is unchanged; the
  `skip_serializing_if` CAS rationale survives.
- **Scan-order independence.** `read_segments` keys a `BTreeMap` on the *parsed* index
  (`:2085`,`:2098`) and the `Shuffling` double (`:5717`) returns every scan reversed.
- **Epoch/nonce scoping (F18).** `seg_range_prefix` pins both (`:1270`), `read_segments`
  re-checks them per row (`:2091`), and `parse_seg_key` rejects non-canonical epochs and
  non-fixed-width indices (`:1300`,`:1311`) — I could not smuggle a second spelling of one
  segment key past it.
- **Repoint identity.** `repoint_chunk`'s segmented arm binds the home to the root's
  generation and to a named index before building either precondition (`:2470-2485`), and
  `SegmentRecord::repoint` makes id/scheme/len unspellable (`:1178`).
- **Leg A's red.** I could not re-run `run-verify.sh` here, but the file names only base
  symbols (imports at `crates/custodian/tests/segmented_map_consumers.rs:53-73`) and a
  segmented value is a JSON object where the base's `chunk_map: Vec<ChunkRef>` demands an
  array, so every leg's red is an assertion/`Err`, not a build error — the claim checks out on
  inspection.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must page allocator-floor scans and add a lowered-cap Redb regression: although all 350 diff mutants reproduced as 187 caught/163 unviable, three orphan rows against a cap of two returned `ScanCapExceeded`, and this namespace can exceed the production cap (`crates/core/src/metadata.rs:3628`, `crates/traits/src/lib.rs:1029`).
- [ ] T4 Contribution — Human must inspect and disposition the six reported batch-review items: the asserted `scripts/review-branch` and its itemized output are absent, so their validity cannot be rerun; affected-path merged and closed/unmerged prior-art checks found no competing implementation.
- [ ] T5 Judgment — The maintainer must accept or reject landing the fence-parameter publication precursor before #636 supplies a production `Completing` producer, because the persistence API otherwise ships without a production caller (`crates/core/src/metadata.rs:2531`).
- [ ] Validation — fitness-to-purpose — Human must decide whether this precursor is fit for the >10 GiB launch goal before #636/#508 supplies the real multipart path; after integration, exercise a beyond-flat-ceiling multipart upload, whole/range reads, reconciliation, and fragment retention (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2311`).
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
- Iteration delta (if iterating): T4 batch-review gate is failing with 6 unresolved blocking findings (review-batch.md), none fixed or recorded-rejected: (1) metadata.rs:2260 — flat map returned without re-reading root, stale maintenance scan can miss live generation; (2) metadata.rs:2932 — fence-value-only check doesn't ensure the flip transitions the record, post-flip rollback can delete segments the live root still names; (3)+(4) metadata.rs:3031 — resume verification silently drops malformed keys / ignores records outside the plan, so a flip can commit a range that later resolves fail on (SegmentKeyMalformed/SegmentUnknown); (5) metadata.rs:3452 — skipping an unreadable/truncated id token can understate the high-water mark, letting the allocator reissue a live ID; (6) dst/tests/custodian.rs:1811 — staged-publication DST missing the mid-segment-batch apply-then-unknown recovery/idempotency test. Also flagged by the adversarial review and worth addressing in the same pass: the same-epoch shorter-resume-plan orphaned-tail bug (metadata.rs:3010/verify_resume_prefix vs :2129/read_segments), and the high_water_marks "totality" claim being false for the three unpaged scans (inode:/pending:/orphan: hitting SCAN_CAP) — either bound the claim or page those scans like segment_chunk_floor already does. Fix and record-reject (with reason) each of the 6 review-batch findings per the existing triage rule before resubmitting.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
