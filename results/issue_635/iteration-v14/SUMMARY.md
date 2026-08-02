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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 508 mutants tested in 28m: 12 missed, 264 caught, 228 unviable, 4 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add safely published segmented chunk maps and one shared resolution path so objects beyond the single-record map ceiling remain readable and maintainable without changing legacy flat encodings.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the base, format invariants, failure containment, scope boundaries, and downstream ownership; its remaining sequencing choices are expressly reserved for human sign-off. |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed and only its focused test restored, all nine scenarios compiled and failed on the flat-only decoder as expected (`crates/custodian/tests/segmented_map_consumers.rs:589`). |
| C3 Change | PASS | The change stays within chunk-map persistence, publication, resolution, consumers, tests, and corresponding architecture docs, with no new dependency or unrelated capability surface (`crates/core/src/metadata.rs:1329`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch made all nine focused scenarios pass, and an isolated full `cargo xtask ci` passed formatting, lint, build, tests, deny, conformance, docs rendering, statics, and seeded DST (`crates/custodian/tests/segmented_map_consumers.rs:589`). |
| C5 Causal adequacy | NEEDS-HUMAN | The maintainer must decide whether the parsed-JSON and byte-scanner allocator-floor readers are justified independent defenses or redundant complexity — several mutations in one reader are masked by the other, so the causal necessity of both is unproved (`crates/core/src/metadata.rs:5141`). |
| T1 Structure | PASS | One core resolver owns both map representations and the custodian seam only adds live-root semantics, preserving a narrow dependency direction (`crates/core/src/metadata.rs:2974`, `crates/custodian/src/resolve.rs:76`). |
| T2 Shape | PASS | The persisted-shape decision is protected by byte-identical legacy round trips and decode-time rejection of the new structural invariants (`crates/core/src/metadata.rs:5811`, `crates/core/src/metadata.rs:5884`). |
| T3 Runtime | NEEDS-HUMAN | The maintainer must accept landing the currently unreachable pre-#636 publisher and its O(N) costs — every maintenance object adds a root read, while startup scans segment IDs whose only production caller discards the result (`crates/custodian/src/resolve.rs:39`, `crates/server/src/lib.rs:123`). |
| T4 Contribution | NEEDS-HUMAN | A human must inspect and disposition the asserted three unpublished batch-review blockers and closed/rejected prior art — affected-path merged history was clear, but `scripts/review-branch`, `wyrd-pdca contribcheck`, their detailed output, and closed-work refs are unavailable here. |
| T5 Judgment | NEEDS-HUMAN [impl] | The rebuild must add tests proving non-`ChunkMapError` decode and resolver failures propagate — both error-type guards can be mutated to unconditional matches while the suite stays green, masking backend or unrelated record faults (`crates/custodian/src/gc.rs:307`, `crates/custodian/src/gc.rs:339`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The human must decide whether raw-record and synthetic publication fixtures are sufficient before #636 supplies a real producer — no actual multipart completion of a greater-than-10-GiB object exercised this persistence seam (`crates/custodian/tests/segmented_map_consumers.rs:14`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory; never gates)

Method: re-ran the asserted red→green from a pristine `git archive HEAD` of `$PDCA_TARGET`
plus the one added test file (base) against the patched worktree, in throwaway scratch
target dirs; then wrote three probe tests against the **patched production API** looking for
the input that breaks it. Two probes broke it.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5646` (and `:5659`): the id floor
  UNDER-approximates for a segmented root whose named `seg:` record is absent — the exact
  damage leg A(vii) is written about.** The `inode:` walk folds in chunk ids only for
  `ChunkMap::Flat` (`:5646`) and deliberately does not resolve a segmented root; the ids are
  recovered from the `seg:` namespace instead (`:5659`). A segment record the root *names*
  but that is not in the store is therefore seen by neither walk. Concrete, reproduced:
  a committed root naming segments 0 and 1, with `seg:<n>:11:000000` durable (chunk ids
  10, 11) and `seg:<n>:11:000001` absent (chunk ids 9 000 000/9 000 001, whose fragments are
  still on disk) ⇒ `high_water_marks` returns a chunk floor of **11**. That contradicts this
  module's own stated rule at `:5064-5072` ("The floor … is not allowed to under-approximate
  … an allocator resuming there re-mints 900 over fragments that are still on disk") and the
  brief's containment row ("must **never** under-approximate the floor"). The brief's *letter*
  ("≥ every chunk id present in any `seg:` record") is met, which is likely how the reviewer
  passed it. Mitigation the human should weigh: `Gateway::recover` discards the chunk half
  today (`crates/server/src/lib.rs:124`, `let (max_inode, _max_chunk)`), so this is latent,
  not live. Mechanical fix available without resolving any root: a committed segmented root
  whose group range yields fewer rows than `segment_count` contributes the same conservative
  `ceiling - 1` that `RecoveredIds::contribution` (`:5078`) already gives an unreadable record.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5162`: `RecoveredIds::complete` reads
  "no unreadable `id` field" as "every id this record names was seen", which is false for the
  very shape this slice adds.** A segmented root carries no `id` field at all — its ids live in
  `seg:` records — so `json_chunk_id_floor` returns `floor = 0, unreadable = 0, complete = true`
  and the conservative `ceiling - 1` arm (`:5078-5084`) never fires. Reproduced: an `inode:`
  value that is valid JSON, fails `decode` structurally (`segment_count: 9` vs one segment) and
  whose `seg:` records are absent ⇒ `high_water_marks` reports a floor of **0** for a record it
  admits it could not read. (Contrast: the same bytes made non-JSON *do* get the conservative
  contribution — so the safe path exists and this record class slips past it.) Same latency
  caveat as above. Note this sits in the code where 4 of the 12 surviving C5 mutants live
  (`:5192`, `:5198`, `:5244`, `:5454`) — the suite cannot see this region.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:307` and `:339`: nothing in the suite
  distinguishes a *store* fault from an *object* fault in `referenced_fragments`.** Both
  surviving mutants replace the `err.downcast_ref::<ChunkMapError>().is_some()` guard with
  `true` and stay green (C5 `missed.txt`). The module's contract is explicit that the two must
  differ (`:283-289`: "Anything that is **not** the object's own fault (a store error, an
  undecodable record) still propagates"), and the difference is operator-visible: under the
  mutant a transient backend I/O error on one `inode:` read is contained as
  `unresolvable`, so `reconciliation_status` answers `PendingUnresolvable { objects: [inode:N] }`
  — naming an innocent, healthy object as the thing to repair — while GC silently reclaims
  nothing. Missing test: a `MetadataStore` double whose `get`/`scan_page` returns a non-
  `ChunkMapError` error for one object, asserting `referenced_fragments` (and hence
  `reconcile_after_restore`, `reconciliation_status`) returns `Err`. Related, same class:
  `crates/core/src/metadata.rs:2084` — the `span != size` match guard mutates to `true`
  unseen, so `SizeSpanMismatch` attribution vs the `SegmentedRootMalformed` fallback is pinned
  by no test.

- **NEEDS-HUMAN [human] — `crates/custodian/src/gc.rs:269` + `crates/core/src/metadata.rs:2315`:
  one damaged segmented object halts **all** reclamation cluster-wide, and this slice ships no
  way to clear it.** `ReferenceSet::protects` answers `true` for every `(dserver, fragment)` in
  the fleet while `unresolvable` is non-empty, so after a single lost `seg:` row GC reclaims
  nothing anywhere — including the orphan-marked bytes of ordinary *flat* deleted objects,
  which then accumulate without bound — and `restore`'s strand-marking marks nothing. The
  brief's containment table does sanction "an incomplete reference set may not authorize any
  reclamation", which is presumably how the reviewer cleared it; what the table does not
  address is the **exit**: `unlink` refuses *any* segmented root (`metadata.rs:2315`,
  `SegmentedRetirementUnsupported`), so an operator cannot delete the damaged object to
  unblock the fleet, and no repair tool ships here. Note the blast radius is strictly wider
  than the precedent it cites: `PendingMalformed` protection is chunk-scoped
  (`gc.rs:271`), this one is fleet-scoped. A human should decide whether shipping the halt
  without an eject path is acceptable for this wave or wants a tracked follow-up.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/backfill.rs:207`: an inode value is put with
  plain `metadata::encode`, bypassing the guarded `encode_inode`.** The patch's own claim
  (`crates/core/src/metadata.rs:1888-1893`, and the added prose in
  `docs/design/architecture/08-crosscutting-concepts.md`: "a record this system refuses to read
  is one no committer in it will store") holds only *inside* `core::metadata` — `encode_inode`
  is private, so `wyrd-custodian`'s committer cannot use it. Latent, not exploitable today
  (backfill only rewrites a `Flat` map, for which `checked_shape` is a no-op), but the
  invariant is asserted in shipped docs one crate wider than it is enforced.

## Attempted and could not refute

- **The red→green is real, and the red is assertions, not a build error.** Base
  (`git archive HEAD` + only `crates/custodian/tests/segmented_map_consumers.rs`) **compiles**
  and fails 9/9 on `Result::unwrap()` / decode assertions; the patched tree passes 9/9. This
  specifically rebuts the failure mode the brief itself flags (`run-verify.sh:416-433` falling
  through to an unconditional PASS when the RED leg fails to build): it did not happen. The
  test file names no symbol this slice adds (checked: `ChunkMap`, `SegmentRef`,
  `PendingUnresolvable`, `resolve_*`, `repoint_chunk` all absent), and the leg A(ii) drain
  assertion is discriminating, not tautological — the segmented and flat fixtures sit on
  disjoint halves of the fleet, so `Pending` can only come from reading the `seg:` range.
- **Production-scale publish → resolve round trip.** 3 000 chunks through
  `SegmentedPublication::publish` with the *real* constants produced 6 segments; every `seg:`
  value stayed inside the 100 KB ceiling, `resolve_chunk_map` returned the chunk list in exact
  input order, and `high_water_marks` covered every live id. No off-by-one in `plan_with`'s
  budget arithmetic or `SEGMENT_ENVELOPE_BYTES`.
- **Re-publication over a live segmented generation** (same `(nonce, epoch)`, shorter chunk
  list) is refused (`SegmentedRetirementUnsupported`) with the live map intact — no hybrid
  range, no torn map.
- Order-independence of the resolver (`metadata.rs:2850-2880` sorts by *parsed* index into a
  `BTreeMap`, and the `Shuffling` double at `:9818` proves the fixture really shuffles);
  epoch-prefix disjointness (`seg:<n>:7:` vs `seg:<n>:70:`) and `seg:` vs `seggrp:`
  non-overlap; `batch_ranges` (`:4928`) never emits an empty range, so
  `assemble_segment_batches`' `pending[range.end - 1]` cannot underflow; flat
  decode→encode identity (`ChunkMap::Serialize` passes the array through unchanged);
  every `.chunk_map` consumer routed through the shared resolver (grep leaves only shape
  decisions and tests). Rebalance/reconstruction/backfill propagating a damaged object's
  fault with `?` is **explicitly sanctioned** by the brief's containment table
  (deletion-capable class: "aborting the pass is acceptable"), so I did not score it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The maintainer must decide whether the parsed-JSON and byte-scanner allocator-floor readers are justified independent defenses or redundant complexity — several mutations in one reader are masked by the other, so the causal necessity of both is unproved (`crates/core/src/metadata.rs:5141`).
- [ ] T3 Runtime — The maintainer must accept landing the currently unreachable pre-#636 publisher and its O(N) costs — every maintenance object adds a root read, while startup scans segment IDs whose only production caller discards the result (`crates/custodian/src/resolve.rs:39`, `crates/server/src/lib.rs:123`).
- [ ] T4 Contribution — A human must inspect and disposition the asserted three unpublished batch-review blockers and closed/rejected prior art — affected-path merged history was clear, but `scripts/review-branch`, `wyrd-pdca contribcheck`, their detailed output, and closed-work refs are unavailable here.
- [ ] T5 Judgment — The rebuild must add tests proving non-`ChunkMapError` decode and resolver failures propagate — both error-type guards can be mutated to unconditional matches while the suite stays green, masking backend or unrelated record faults (`crates/custodian/src/gc.rs:307`, `crates/custodian/src/gc.rs:339`).
- [ ] Validation — fitness-to-purpose — The human must decide whether raw-record and synthetic publication fixtures are sufficient before #636 supplies a real producer — no actual multipart completion of a greater-than-10-GiB object exercised this persistence seam (`crates/custodian/tests/segmented_map_consumers.rs:14`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): T4 batch-review gate is failing: 3 new blocking findings from the latest review-branch pass are not yet fixed or recorded-rejected in review-rejected.md: - crates/custodian/src/backfill.rs:109 — a damaged segmented inode's ChunkMapError aborts the entire backfill pass instead of being contained per-object, blocking healthy flat inodes too. - crates/custodian/src/rebalance.rs:168 — same shape: one unresolvable inode aborts evacuation planning for every object, not just the damaged one. - crates/custodian/src/reconstruction.rs:615 — one damaged segmented inode aborts the full-store lookup for a queued repair, starving healthy under-replicated chunks that sort after it. Rebuild should apply the same containment pattern already used elsewhere in this bundle (e.g. gc.rs's referenced_fragments / ReferenceSet::unresolvable handling) to these three call sites: contain the per-object ChunkMapError, attribute it, and continue the pass for unaffected objects — rather than propagating it and aborting the whole pass. Each fix (or a reasoned decline per the triage rule, appended to review-rejected.md) must land before the T4 gate can pass. The other §6 NEEDS-HUMAN items (C5 causal adequacy, T3 runtime cost, T5 judgment test-gap, validation fitness-to-purpose) were not cleared this session and remain open for the next sign-off pass.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
