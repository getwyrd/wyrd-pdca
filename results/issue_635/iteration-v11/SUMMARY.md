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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 479 mutants tested in 16m: 11 missed, 253 caught, 212 unviable, 3 timeouts

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

Reviewing issue #635: add an exact `Flat | Segmented` chunk-map format, staged publication, and shared bounded resolution across read and maintenance consumers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is concrete—exact legacy/segmented encoding, bounded shared resolution, and object-scoped containment are documented, so the change can be judged without inventing behavior (`docs/design/architecture/06-runtime-view.md:32`, `docs/design/architecture/08-crosscutting-concepts.md:83`). |
| C2 Reproduction (red pre-fix) | PASS | My base-only run compiled and then failed all 9 binding tests, including the maintenance-protection leg, establishing the pre-fix behavior rather than a build failure (`crates/custodian/tests/segmented_map_consumers.rs:589`). |
| C3 Change | PASS | The diff stays within the representation, publication, resolver, consumer, test, and living-doc surfaces required by the spec; a target-wide search found no unrelated dependency or workflow change (`crates/core/src/metadata.rs:1273`, `crates/custodian/src/resolve.rs:76`). |
| C4 Verification (red→green) | PASS | The independent binding run changed from 0/9 to 9/9, and a full patched `cargo xtask ci` rerun passed docs, lint, build, tests, deny checks, conformance, and DST (`crates/custodian/tests/segmented_map_consumers.rs:589`). |
| C5 Causal adequacy | NEEDS-HUMAN | The maintainer must decide whether the dual parsed-value plus byte-scanner recovery is the intended root solution or should become one total recovery path—the overlapping grammars enlarge a data-loss-critical proof surface (`crates/core/src/metadata.rs:4409`). |
| T1 Structure | PASS | One core resolver owns flat/segmented semantics and the custodian wrapper owns live-root maintenance currency, avoiding consumer-specific decoding (`crates/core/src/metadata.rs:2745`, `crates/custodian/src/resolve.rs:76`). |
| T2 Shape | PASS | The wire shape preserves byte-identical flat arrays, pins the segmented fields, and validates structural invariants during decode, satisfying the standing rubric (`crates/core/src/metadata.rs:1232`, `crates/core/src/metadata.rs:1327`, `AGENTS.md:146`). |
| T3 Runtime | NEEDS-HUMAN | The maintainer must accept a `Completing`-less publication precursor with no non-test caller until #636 plus one extra metadata `get` per object on maintenance passes—this determines whether dormant API and O(objects) fleet cost are acceptable (`crates/core/src/metadata.rs:2796`, `crates/core/src/metadata.rs:3180`). |
| T4 Contribution | NEEDS-HUMAN | A human must triage the gate's 8 reported blockers and mechanically unavailable rejected-attempt history—the target lacks `scripts/review-branch` and its output, so despite clear affected-path merged history and closed-PR search, the required deep-review disposition remains unconfirmed (`AGENTS.md:206`). |
| T5 Judgment | NEEDS-HUMAN [impl] | The rebuild must kill or explicitly justify 11 independently reproduced survivors (plus 3 timeouts), especially array/escaped-ID recovery and non-`ChunkMapError` GC propagation; otherwise the tests do not protect allocator and containment claims (`crates/core/src/metadata.rs:4473`, `crates/custodian/src/gc.rs:307`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether the independently green synthetic binding/DST evidence is sufficient before #636 supplies a real producer and >10-GiB topology—production-scale persistence and containment fitness remain unexercised (`crates/core/src/metadata.rs:3180`, `crates/custodian/tests/segmented_map_consumers.rs:589`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (iteration 11)

Attacked at `$PDCA_TARGET` = `/home/eddie/development/wyrd/wyrd.pdca-wt-l0` (patch applied in the
worktree, base `9120f7a`). Two findings were reproduced by **running the patched production code**
from a throwaway crate under `$PDCA_SCRATCH` (path-deps on `wyrd-core`/`wyrd-custodian`, since I
must not write into the read-only target); the scratch dir has been removed.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:311` (and the claim at `:302-304`): the new
  per-object containment covers only damage that still parses as JSON, so one *torn* `inode:`
  value blanks the drain surface for the whole fleet — the exact blast radius this patch says it
  bounds.** `metadata::decode` re-derives a typed `ChunkMapError` only when the bytes re-parse
  (`crates/core/src/metadata.rs:1875`, `serde_json::from_slice(bytes).ok()?`); a value truncated
  mid-token yields a bare `serde_json::Error`, the downcast at `gc.rs:307` misses, and
  `referenced_fragments` returns `Err` — which `reconciliation_status` propagates verbatim at
  `crates/custodian/src/desired_state.rs:174`, directly contradicting its own comment at
  `desired_state.rs:189` ("Never `Err`: this surface is read per D server, and one damaged object
  may not blank it for the whole fleet"). **Reproduced**: a store holding one healthy flat object
  plus `inode:2` = a valid segmented root with its last 30 bytes missing →
  `reconciliation_status(3) = Err(EOF while parsing a string at line 1 column 161)` and
  `reconciliation_status(0) = Err(...)`; `high_water_marks` stayed `Ok`. Truncation is the likeliest
  physical corruption of exactly the record classes this slice introduces (a 512-entry root is
  ~25 KB, a `seg:` value up to 50 KB), and the patch already argues the fix elsewhere: a `seg:`
  value that "cannot be read at all" is contained *because its key names one object*
  (`crates/core/src/metadata.rs:1822-1856`) — `inode:<id>` names one object just as precisely, so
  the asymmetry is unjustified by the patch's own reasoning. The test that would have caught it
  asserts the opposite: `crates/custodian/tests/segmented_map_consumers.rs:1181-1185` calls its
  three fixtures "the three spellings damage has", and all three are valid JSON. Fix at the
  `inode:` walk, or record-reject with the reason.

- **NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:4400-4403` vs `crates/server/src/lib.rs:124`:
  the entire chunk-id-floor half of `high_water_marks` defends a hazard that cannot occur on this
  tree, and the ~700 lines of new production code + ~1,300 lines of tests spent on it are the
  slice's largest unreviewable surface.** Every doc block on `raw_chunk_id_floor` (`:4431`),
  `scavenged_chunk_id_floor` (`:4510`), `scanned_chunk_id` (`:4570`), `widest_id_with_prefix`
  (`:4725`) and `segment_chunk_floor` (`:4782`) justifies itself with "a floor below a live id lets
  the allocator re-mint that id and clobber its fragments (issue #364)" — but `Gateway::recover`
  **discards** the value (`let (max_inode, _max_chunk) = …`, `server/src/lib.rs:124`) and the
  gateway mints coordination-free ids ≥ 2^127 (`server/src/lib.rs:238`, `:251`); a repo-wide grep
  finds no other reader of `max_chunk`. Only the *totality* half of the requirement is live (an
  `Err` there does stop startup). Worse, the semantics deliberately chosen for the dead value would
  themselves be the bug if it were ever wired: **measured** on the patched code, a single record
  torn at `{"size":8,"chunk_map":[{"id":1` yields `max_chunk = 18446744073709551615` = `2^64 - 1`
  (`widest_id_with_prefix`'s cap at `ceiling - 1`, `:4750`), i.e. one torn byte-range exhausts the
  whole in-process id space. This is a Plan-level call, not a build defect: the brief itself
  mandated the floor property (leg A(vii)(a), asserted at
  `crates/custodian/tests/segmented_map_consumers.rs:1194-1211`), so a human must decide whether to
  wire the floor to a consumer, or drop the recovery machinery and bound the claim to totality.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:307` and `:323`: both containment guards
  survive mutation (`replace match guard err.downcast_ref::<ChunkMapError>().is_some() with true`,
  `mutants.out/missed.txt`), so no test pins the rule the doc states at `gc.rs:289-291` — that a
  fault which is *not* the object's own still propagates.** Concrete missing case: a
  `MetadataStore` double whose `scan_page` returns `Err` for the `seg:` prefix must still make
  `referenced_fragments`/`reconcile_step` fail; with the guard widened it would instead be filed as
  "this object is unresolvable", after which `protects` (`gc.rs:268-273`) silently protects every
  fragment in the fleet and `reconciliation_status` answers `PendingUnresolvable` — a transient
  backend fault rendered as permanent per-object corruption, with GC quietly reclaiming nothing.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_consumers.rs:1260` discards the
  result of `reconcile_after_restore`, and on this very fixture that call returns `Err`, so leg
  A(vii)(d)'s "nothing was reclaimed" is bought by a pass that aborted rather than by the
  containment the leg exists to demonstrate.** `reconcile_after_restore` contains the fault in its
  first walk (`restore.rs:183` → the contained `referenced_fragments`) and then propagates it in
  its third (`restore.rs:309` → `committed_chunks`, `restore.rs:373-386`, which calls
  `resolve::chunks_of(...)?` with no downcast) — for all three damaged fixtures (`SegmentAbsent`,
  `SegmentRecordMalformed`, and the root that fails `decode`). Two seams of one pass disagree about
  containment and neither the discarded `let _ =` nor any other assertion notices. Either assert
  the pass's outcome explicitly (aborting is permitted by the brief's table — then say so in the
  test), or give `committed_chunks` the same containment as the walk above it.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:4450-4489`: the `serde_json`-parsed id reader
  is dead weight against the current fixtures — five mutants inside it survive** (`:4463` `<`→`>`,
  `:4467` `+=`→`*=`, `:4473` delete the `Array` arm, `:4485`/`:4486` delete both `json_chunk_id`
  arms). Every case the suite feeds it is also readable by `scavenged_chunk_id_floor`, so
  `raw_chunk_id_floor`'s `parsed.floor.max(scanned.floor)` (`:4437`) is decided by the scanner
  alone. Either delete the parsed reader (its stated advantages — quoted ids, escaped field names —
  are all handled by the scanner at `:4510-4544`) or add a value only it can read, so the
  redundancy is a tested claim rather than an asserted one.

## Attempted and could not refute

- **The red→green evidence.** `crates/custodian/tests/segmented_map_consumers.rs` names only
  base-visible symbols (`:53-73`), seeds the segmented object as raw record bytes (`:285-311`), and
  drives the real `reconcile_step` / `reconcile_after_restore` / `reconciliation_status` /
  `read_object` — not doubles of them. On the base a segmented `inode:` value cannot decode, so the
  red is an assertion failure, not a build error, as claimed. The legs are not tautologies: leg 2
  carries its mirror (a server holding nothing is `Satisfied`, `:817-826`), leg 4 is re-asserted
  standing alone so it cannot inherit leg 3's pass (`:716-780`), and the containment leg's
  `PendingUnresolvable` attribution has a typed mirror in `crates/custodian/tests/rebalance.rs:1592-1628`
  including the "with the object repaired, the same server certifies" arm.
- **Both iteration-10 carry-forwards are genuinely fixed**, not papered over: `plan_with` now
  refuses an empty placement before anything is durable (`crates/core/src/metadata.rs:3360-3365`),
  and `read_group_range` **refuses** rather than trusts a root's declared `segment_count` past
  `MAX_ROOT_SEGMENTS` (`:2567-2572`), with the refuse-vs-clamp reasoning stated at `:2543-2557`.
- I tried to break the resolver on ordering (it sorts by parsed `index`, `:2643-2654`), on epoch
  reuse (`seg_range_prefix` pins nonce **and** epoch, and `seg:<n>:1:` cannot prefix-match
  `seg:<n>:11:…`), on key-spelling smuggling (`parse_seg_key` at `:1582-1611` is canonical-decimal
  and fixed-width), on the complete-but-stale resolve (settled by the root re-read at `:2688`), and
  on flat byte-identity (`ChunkMap::Serialize` delegates the flat arm unchanged, `:1330`). None of
  these gave a failing case.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The maintainer must decide whether the dual parsed-value plus byte-scanner recovery is the intended root solution or should become one total recovery path—the overlapping grammars enlarge a data-loss-critical proof surface (`crates/core/src/metadata.rs:4409`).
- [ ] T3 Runtime — The maintainer must accept a `Completing`-less publication precursor with no non-test caller until #636 plus one extra metadata `get` per object on maintenance passes—this determines whether dormant API and O(objects) fleet cost are acceptable (`crates/core/src/metadata.rs:2796`, `crates/core/src/metadata.rs:3180`).
- [ ] T4 Contribution — A human must triage the gate's 8 reported blockers and mechanically unavailable rejected-attempt history—the target lacks `scripts/review-branch` and its output, so despite clear affected-path merged history and closed-PR search, the required deep-review disposition remains unconfirmed (`AGENTS.md:206`).
- [ ] T5 Judgment — The rebuild must kill or explicitly justify 11 independently reproduced survivors (plus 3 timeouts), especially array/escaped-ID recovery and non-`ChunkMapError` GC propagation; otherwise the tests do not protect allocator and containment claims (`crates/core/src/metadata.rs:4473`, `crates/custodian/src/gc.rs:307`).
- [ ] Validation — fitness-to-purpose — The maintainer must decide whether the independently green synthetic binding/DST evidence is sufficient before #636 supplies a real producer and >10-GiB topology—production-scale persistence and containment fitness remain unexercised (`crates/core/src/metadata.rs:3180`, `crates/custodian/tests/segmented_map_consumers.rs:589`).
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
- Iteration delta (if iterating): T4 gate genuinely fails (not flaky/transient): re-ran scripts/review-branch --bundle manually (3 fresh codex passes, --out scratch, bundle files untouched) and got 12 blocking findings, 0 recorded-rejected — overlapping the bundle's own review-batch.md 8 findings plus 4 more. This confirms the gate result rather than refuting it. Primary must-fix: the fence/rollback race, reported independently 5 times across both runs (crates/core/src/metadata.rs:3646 x3, :3500, :4126) — a segment-phase fence transition A->B followed by the flip going B->A re-satisfies a stale rollback's precondition and lets it delete already-published live segments. This is a coverage gap in check_fence_never_cycles (segment-phase batches only; needs to also cover the flip), not a new design decision — the "fence must not cycle" rule is already settled from round 11. Fix within the existing staged-publication design; no brief/scope change needed. Second cluster, also fixable in place: decode/error-classification precision bugs (metadata.rs:2043, :1937, gc.rs:305, :1418, :1655) — tighten the existing ChunkMapError containment/classification functions. Re-record the id-floor scan-cost finding (metadata.rs:4921) against review-rejected.md precisely — it is the same question already decided (Deferred: follow-up, base-wide Gateway::recover question, not this slice's) but the MATCH/line drifted enough that the gate re-flags it as new every round. None of the §6 NEEDS-HUMAN items (C5 causal adequacy, T3 runtime precursor cost, T4 contribution triage, T5 mutants, Validation fitness-to-purpose) required re-planning — items 2 and 5 (T3, Validation) are re-affirmations of scope boundaries the brief already set (#635 vs #636 sequencing), confirmed unchanged; the rest are ordinary implementation/hardening follow-ups. So: iterate-do, not iterate-plan. §6 boxes left unticked — human did not explicitly clear any of them this session; carry them forward for the next iteration's sign-off.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
