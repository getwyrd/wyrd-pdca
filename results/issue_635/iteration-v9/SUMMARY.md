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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 437 mutants tested in 14m: 11 missed, 232 caught, 192 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #635: add byte-compatible flat-or-segmented inode chunk maps, staged publication, and shared resolution across read and maintenance consumers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is sufficiently bounded by representation, publication, consumer, containment, and dependency criteria, so no unresolved Plan choice prevents judging this slice. |
| C2 Reproduction (red pre-fix) | PASS | The pre-fix symptom is mechanically genuine rather than a compile failure: the added binding target compiled on the base and all 9 tests failed at assertion/runtime (`crates/custodian/tests/segmented_map_consumers.rs:510`). |
| C3 Change | PASS | No Plan re-entry is indicated: the JSON-discriminated representation, bounded resolver, and staged publisher stay inside the requested slice and preserve the prior flat wire form (`crates/core/src/metadata.rs:1091`; `crates/core/src/metadata.rs:1144`; `crates/core/src/metadata.rs:2285`; `crates/core/src/metadata.rs:2692`). |
| C4 Verification (red→green) | PASS | Release evidence was independently reproduced: 0/9 base tests became 9/9 patched tests, and workspace CI, docs rendering, typos, deny scans, conformance, statics, and DST passed; the shared Cargo-home permission fault was cleared by rerunning the same deny scans with an isolated Cargo home (`crates/custodian/tests/segmented_map_consumers.rs:510`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must either remove/justify the redundant parsed reader or add a fixture that discriminates it: the test claims both readers are load-bearing, yet deleting parsed array/number/string handling survives, so the asserted two-reader causality is not demonstrated (`crates/core/src/metadata.rs:3820`; `crates/core/src/metadata.rs:8240`). |
| T1 Structure | PASS | The architectural boundary is coherent: one core bounded resolver and one custodian seam prevent per-consumer interpretations without adding a dependency edge (`crates/core/src/metadata.rs:2285`; `crates/custodian/src/resolve.rs:76`). |
| T2 Shape | PASS | Wire compatibility and invalid-state handling are mechanically settled: flat arrays serialize unchanged while segmented structural invariants are rejected during deserialization (`crates/core/src/metadata.rs:1063`; `crates/core/src/metadata.rs:1144`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must accept landing a pre-#636 publisher with no production caller and one metadata `get` per committed object per maintenance pass — this determines immediate deployability and fleet-scale pass latency (`crates/core/src/metadata.rs:2692`; `crates/core/src/metadata.rs:2336`). |
| T4 Contribution | NEEDS-HUMAN | A human must inspect the unavailable five-blocker batch-review report and closed/rejected affected-path history — only 119 local merged-history commits were mechanically settled, so blocker disposition and contribution uniqueness remain provisional. |
| T5 Judgment | PASS | No separate judgment defect grounds beyond the routed C5 evidence issue: containment fails closed and the binding test uses positive production observables for fragment protection and consumer behavior (`crates/core/src/metadata.rs:2192`; `crates/custodian/tests/segmented_map_consumers.rs:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether synthetic segmented fixtures are sufficient before #636 supplies the real producer and whether the added per-object maintenance round trip is acceptable at production scale — these choices determine end-to-end fitness, not compilation (`crates/core/src/metadata.rs:2692`; `crates/custodian/src/resolve.rs:39`). |

### Advisory — adversary

# Adversarial review — issue #635 (segmented-chunk-map)

Advisory only; the human decides at sign-off. Every citation is against the target working
tree at `$PDCA_TARGET` (patch applied on `9120f7a`).

## Refutations

- **NEEDS-HUMAN [impl] — the containment rule is keyed on the error *type*, and two of the
  three ways a damaged segmented object presents are not that type, so one damaged object
  still blanks the drain surface fleet-wide.** `crates/custodian/src/gc.rs:309` contains an
  unresolvable map only when `err.downcast_ref::<ChunkMapError>().is_some()`; everything else
  re-raises at `:315`. But the decode-time invariants this slice adds surface as
  `serde_json::Error`, never as `ChunkMapError`: `SegmentRecord`'s `byte_len` check goes
  through `D::Error::custom` (`crates/core/src/metadata.rs:1331`), the root's size-vs-table
  check through `try_from` (`:1511`), and `metadata::decode` boxes the serde error as-is
  (`:1587`). Concrete failing case, a one-line edit to the patch's own fixture at
  `crates/custodian/tests/rebalance.rs:1538-1557`: publish the damaged root with `size: 9`
  against an 8-byte segment table (or leave `size: 8` and seed
  `seg:0123…:7:000000` = `{"chunks":[],"byte_offset":0,"byte_len":8}`, valid JSON that fails
  `SegmentRecord`'s own decode at `metadata.rs:2181`). Then `reconciliation_status(&meta, 3)`
  returns **`Err`** — for *every* D server in the store — instead of `PendingUnresolvable`.
  That is verbatim the outcome `gc.rs:296-301` says must not happen ("one damaged object would
  blank the drain-status surface fleet-wide") and the outcome the brief's containment table
  forbids for the `reconciliation_status` row. `high_water_marks` contains exactly these bytes
  correctly (`metadata.rs:4097`, `:4213`), so the patch is internally inconsistent about the
  same damage. Leg A(vii) and `a_drain_stays_blocked_and_attributed_while_a_map_cannot_be_resolved`
  only ever exercise the **absent-record** spelling, which is the one spelling that *is* a
  `ChunkMapError` — so the green is real but narrower than the claim it is offered for.

- **NEEDS-HUMAN [impl] — the `seggrp:<nonce>` reservation the brief calls "the corrective
  rule" is never written by anything, and the committer forbids the caller from writing it.**
  `SEGGRP_MARKER` (`crates/core/src/metadata.rs:282`) has no production writer: `flip_batch`
  (`:3114-3126`) and `assemble_segment_batches` (`:3052-3079`) emit only the root CAS and the
  `seg:` puts, while `owned_keys` (`:3087`) claims the marker key so a caller's contribution
  that puts it is refused with `ContributionCollides` — pinned as *intended* behaviour by the
  co-located test at `:5197`. So `require_absent(seggrp:<nonce>) + put(marker)` is unreachable
  through the shipped API in either direction, yet `:899-900` and `:3441-3443` both document
  the guard as if it existed. It is not a marked deferral (contrast the explicit
  `deferred: #636` markers at `:2566`, `:2577`), and the brief settles it here
  ("**Implement the corrective rule:** … reserved by `require_absent(seggrp:<nonce>)` plus the
  marker record"). Concrete failing case: publish object A under group `(N,7)` with 2 segments,
  then publish object B under the *same* `(N,7)` with 3 segments and `resume_from: 0`.
  `verify_durable_range` compares nothing (`required = claimed = 0`, `metadata.rs:3269-3273`)
  and the tail check at `:3257` passes because both durable indices are `< 3`, so phase 1
  overwrites A's segment records; A's root still names `(N,7)`, and A resolves to
  `SegmentUnknown` — permanently unresolvable — or, if B's byte extents coincide, to **B's
  chunks**, which is the "hybrid map … nothing downstream ever notices" the patch's own doc at
  `:3203-3211` calls worse than the unresolvable one. Note the fix has a wrinkle the builder
  must handle: a resumed attempt must not re-require the marker absent.

- **NEEDS-HUMAN [impl] — the id-floor's "two readers" rationale is asserted in prose and by no
  test; the C5 row is pointing at exactly that.** `raw_chunk_id_floor`
  (`crates/core/src/metadata.rs:3797-3830`) justifies a JSON-parsing reader beside the byte
  scanner, but all 11 surviving mutants land inside it: deleting *both* arms of `json_chunk_id`
  (`:3873`, `:3874`) and flipping `id < ceiling` to `id > ceiling` (`:3851`) are all unnoticed,
  i.e. the whole parsed reader can be disabled and the suite stays green because the scanner
  covers every input the tests use. Either add the case only the parse can read (and pin it),
  or drop the reader — this is code on `Gateway::recover`'s startup path whose stated purpose
  is not to under-approximate the floor (#364), and right now nothing would notice if it
  stopped contributing.

## Attempted and could not refute

- The round-8 findings appear genuinely fixed, not papered over: `flip` now verifies
  `DurableRange::WholePlan` (`metadata.rs:3346-3350`), `check_fence_transitioned` refuses *any*
  put that restores a pinned value (`:3576-3585`), and `widest_id_with_prefix` caps each range
  at `ceiling - 1` instead of walking the nines — traced by hand at prefix `18`, ceiling `2^64`,
  it returns `18446744073709551615` (`:4037-4063`).
- Flat byte-identity: `InodeRecord`'s field order and `ChunkRef`'s encoding are untouched by
  the diff (only `chunk_map`'s *type* changed, `metadata.rs:1453`), and `ChunkMap`'s
  discriminate-by-JSON-type `Serialize` (`:1144-1150`) re-emits a flat map as the bare array,
  so `require(key, encode(prior))` still matches stored legacy bytes.
- Key-range ambiguity: `seg:<nonce>:<epoch>:` cannot alias a neighbouring epoch or group —
  the nonce is fixed-width hex, the epoch is canonical decimal (`parse_canonical_u64`,
  `:1405-1417`) and the index is fixed width, all rejected strictly in `parse_seg_key`.
- Repoint identity: `SegmentRecord::repoint` takes a placement vector, not a `ChunkRef`
  (`:1262-1272`), and `repoint_chunk` binds the home's key to the live root's group before
  building either precondition (`:2627-2646`) — I could not construct a repoint that changes a
  chunk's id/len or lands in a retired generation's record.
- Reader restart framing: `read_object` and both gateway paths take `size`/ETag from the
  generation the chunks came from, not the stale snapshot (`core/src/read.rs:520`,
  `server/src/lib.rs:366`, `:371`, `:453-465`).
- Restore/GC agreement: both walk the same `resolve::chunks_of` and both gate deletion on the
  single `ReferenceSet::protects` predicate (`gc.rs:269`, `restore.rs:222`), so I could not
  reproduce the #508-attempt-4 split-brain the slice exists to prevent.
- The O(N) extra root `get` per maintenance pass (`resolve.rs:81`, `:101`) and the
  `find_chunk`-per-repair amplification (`reconstruction.rs:604-615`) are real, but they are
  already carried as a §6 human item on this bundle — not re-filed here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must either remove/justify the redundant parsed reader or add a fixture that discriminates it: the test claims both readers are load-bearing, yet deleting parsed array/number/string handling survives, so the asserted two-reader causality is not demonstrated (`crates/core/src/metadata.rs:3820`; `crates/core/src/metadata.rs:8240`).
- [ ] T3 Runtime — Maintainers must accept landing a pre-#636 publisher with no production caller and one metadata `get` per committed object per maintenance pass — this determines immediate deployability and fleet-scale pass latency (`crates/core/src/metadata.rs:2692`; `crates/core/src/metadata.rs:2336`).
- [ ] T4 Contribution — A human must inspect the unavailable five-blocker batch-review report and closed/rejected affected-path history — only 119 local merged-history commits were mechanically settled, so blocker disposition and contribution uniqueness remain provisional.
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether synthetic segmented fixtures are sufficient before #636 supplies the real producer and whether the added per-object maintenance round trip is acceptable at production scale — these choices determine end-to-end fitness, not compilation (`crates/core/src/metadata.rs:2692`; `crates/custodian/src/resolve.rs:39`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): T4 gate (gating) fails on re-run against the real patched worktree — reproduced live at sign-off, not stale: 5 blocking findings, 0 recorded-rejected. - crates/custodian/src/gc.rs:309 [BUG] (seen independently by 2 passes, and matches the adversary review's refutation #1) — structural SegmentRecord decode failures surface as serde_json::Error, not ChunkMapError, so the containment downcast misses them and one malformed segment aborts fleet-wide reference-build / drain-status instead of being contained per-object. This is the exact §1(vii) containment property the brief requires; round 8 fixed only the absent-record spelling, not the malformed-record spelling. - crates/core/src/metadata.rs:2170 [BUG] — same containment class, different consumer: the segment scan materializes a corrupt generation's whole range before enforcing the 512-segment bound, so a damaged object can hit SCAN_CAP / exhaust memory and abort fleet-wide maintenance instead of being contained. - crates/core/src/metadata.rs:3924 [BUG] — the fallback quoted-ID lexer misses IDs with enough escaped leading zeros, under-reporting the allocator's chunk-id floor. - crates/core/src/metadata.rs:1329 [CONVENTION] — SegmentRecord decode admits an empty segment (chunks:[], byte_len:0) as valid despite the repo's parse-don't-validate rule treating that as structural corruption. Directive for the rebuild: fix at the foundation, not by whack-a-mole per call site. The gc.rs:309 / metadata.rs:2170 pair share one root cause — decode-time failures are not uniformly surfaced as ChunkMapError — so fix it once at the decode/error-wrapping boundary (e.g. make every SegmentRecord/root structural failure produce ChunkMapError, or have the shared resolver/containment check recognize both) rather than patching each downstream downcast site individually; a third call site with the same pattern must not become a future review round. Then fix the two remaining findings (metadata.rs:3924 escaped-leading-zero lexer gap, metadata.rs:1329 empty-segment validation) on their own merits. None of these are Plan/brief gaps — the brief already specifies the required containment and decode-time-invariant behavior in detail (§1(vii), §1(B)(ii)); these are implementation completeness bugs against that spec. Re-verified live at sign-off by applying patch.diff to a clean worktree at the stated base (9120f7a) and re-running scripts/review-branch --bundle directly: same 5 blocking findings reproduced, confirming the gate result in the bundle is not stale or flaky. §6 NEEDS-HUMAN items are left open (not cleared) — carry forward to the next iteration's Check along with the other unresolved judgment items already listed there (C5 causal adequacy / redundant parsed reader, T3 runtime deployability, T4 contribution-history provisionality, fitness-to-purpose of synthetic fixtures pre-#636) and the adversary review's two other open findings (the seggrp: marker reservation that's never written by any code path, and the unresolved C5 mutants on raw_chunk_id_floor).
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
