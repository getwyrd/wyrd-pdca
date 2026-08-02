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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 504 mutants tested in 18m: 13 missed, 264 caught, 224 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of the segmented inode chunk-map implementation, including staged publication, shared consumer resolution, containment behavior, and large-object accounting.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract is specific enough to decide compatibility, publication, containment, and accounting outcomes without an unresolved scope choice (`docs/design/architecture/08-crosscutting-concepts.md:83`). |
| C2 Reproduction (red pre-fix) | PASS | The clean target base compiled the binding suite and failed all 9 cases, establishing the pre-fix behavior the acceptance decision depends on (`crates/custodian/tests/segmented_map_consumers.rs:589`). |
| C3 Change | PASS | The authorized boundary remains one core resolver with a narrow custodian adapter and current architecture documentation, so no unplanned dependency or manifest change needs Plan re-entry (`crates/core/src/metadata.rs:2946`). |
| C4 Verification (red→green) | PASS | The same binding suite changed from 0/9 on the clean base to 9/9 on the target, and a clean-environment `cargo xtask ci` rerun passed every native validator and seeded DST check (`crates/custodian/tests/segmented_map_consumers.rs:589`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The rebuild must require exact current root/table identity, because an independent probe changed the table while retaining its group and the group-only retry check returned stale chunks instead of failing closed (`crates/core/src/metadata.rs:2907`). |
| T1 Structure | PASS | Ownership stays in core while custodian consumers share a small adapter, preserving the dependency direction and a single resolution policy (`crates/custodian/src/resolve.rs:76`). |
| T2 Shape | PASS | Flat wire identity and segmented decode invariants are explicit at the serialization boundary, so malformed persisted shape is rejected before runtime use (`crates/core/src/metadata.rs:1382`). |
| T3 Runtime | NEEDS-HUMAN | The maintainer must accept landing a dormant segmented committer before issue #636 and its extra root read per committed object per maintenance pass, because production reach and operational cost are intentionally deferred (`crates/custodian/src/resolve.rs:39`). |
| T4 Contribution | NEEDS-HUMAN | A human must settle the four recorded batch-review items and closed/rejected prior-art status, because the prescribed `scripts/review-branch` wrapper is absent and its red contribution result cannot be independently adjudicated. |
| T5 Judgment | NEEDS-HUMAN [impl] | The rebuild must add assertions that kill independently reproduced boundary survivors, because mutable GC error guards and raw-recovery branches can change without failing the current suite (`crates/custodian/src/gc.rs:307`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether synthetic raw-record, redb, and DST evidence is fit for release before issue #636 supplies a real production publisher caller, because the staged committer is not yet exercised by deployed topology (`crates/core/src/metadata.rs:3395`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Re-ran the asserted red→green independently in scratch (clone of `origin/main` @ `9120f7a`,
the single added test file copied in, production untouched): **9 tests, 0 passed, 9 failed
with `invalid type: map, expected a sequence` propagating out of `reconcile_step` /
`high_water_marks` / `read_object`** — a genuine assertion red, not a build error. The same
tree with `patch.diff` applied: **9 passed**. The C4-verify claim holds; I could not refute it.
Findings below are what survived the attempt.

- NEEDS-HUMAN [human] — **One damaged object stops *all* space reclamation, cluster-wide, and
  the pass still reports success.** `crates/custodian/src/gc.rs:269` — `protects()` answers
  `true` for *every* `(dserver, fragment)` in the fleet as soon as `unresolvable` is non-empty.
  Demonstrated with a probe in a scratch copy of the bundle: (control) a stray fragment on
  server 5 with a valid `orphan:` record stamped at t=0 is reclaimed by a GC pass at
  t=`GRACE*10`; (probe) seed the leg-7 damaged object — an unrelated object, other servers,
  other chunk ids — and the *same* stray survives, with `reconcile_step` returning
  `Ok(Satisfied)`. The stall lasts as long as the damaged record does, and this slice ships no
  repair path for it; `crates/custodian/src/desired_state.rs:191` blocks every drain on the
  same condition. The brief's containment table can be read as sanctioning this ("an incomplete
  reference set may not authorize any reclamation"), which is why this is a maintainer call and
  not a build defect: it trades one damaged object for the whole cluster's GC and drain
  progress — the same blast-radius trade the brief *rejected* for `high_water_marks`. Note the
  leg-7 assertions "nothing of the damaged object is reclaimed" / "the healthy objects'
  fragments are untouched"
  (`crates/custodian/tests/segmented_map_consumers.rs:1275`,`:1319`) pass for this reason —
  nothing anywhere is reclaimed — so they do not discriminate a per-object containment from a
  fleet-wide one; there is no positive control that genuine garbage is still collected.
- NEEDS-HUMAN [impl] — **The blocked pass mis-attributes itself.**
  `crates/custodian/src/gc.rs:162-170`: a fragment protected only because the reference set is
  incomplete is emitted with `reason = "referenced"`, which is false — it is referenced by
  nothing, and an operator reading the audit trail cannot tell the two apart. And
  `crates/custodian/src/gc.rs:128` returns `Reconciled::Satisfied` for that pass (observed
  `Ok(Satisfied)` above): a "policy satisfied" verdict for a pass that reclaimed nothing
  *because it was blocked* is the rubric's *Absent or unsupported entries* forbidden move
  ("never silent success … or a count-based assertion that can pass while the property fails",
  `AGENTS.md:175-177`). A third skip reason (`reference-set-incomplete`) and a non-`Satisfied`
  outcome would close it; the `gc_unresolvable_chunk_map` counter at `gc.rs:430` is the only
  signal today and it does not say that reclamation stopped.
- NEEDS-HUMAN [impl] — **A transient concurrency outcome is classified as object damage.**
  `crates/core/src/metadata.rs:3026` raises `ChunkMapError::MapResolutionUnstable` when a
  segmented generation is retired under the resolver `MAX_RESOLVE_RESTARTS` times, and
  `crates/custodian/src/gc.rs:339` contains *any* `ChunkMapError` as "this object's map cannot
  be resolved". Concrete case: a segmented object republished three times while one GC pass
  resolves it — no corruption anywhere — is recorded in `unresolvable`, which (previous
  findings) stops every reclamation in the fleet for that pass and names a **healthy** object
  to the operator on the drain surface (`desired_state.rs:191`). Nothing in the bundle tests
  it; the fix is to distinguish a retry-exhausted resolve from a structural fault.
- NEEDS-HUMAN [impl] — **The containment classifier itself is unpinned.** `cargo mutants`
  reports both guards — `crates/custodian/src/gc.rs:307` and `:339`,
  `err.downcast_ref::<ChunkMapError>().is_some()` — replaced by `true` with the suite still
  green (`mutants.out/missed.txt`). The documented contract at `gc.rs:286-291` ("anything that
  is **not** the object's own fault — a store error … — still propagates") therefore has no
  test: under that mutation a transient metadata-store failure mid-pass is silently recorded as
  a damaged object and the pass reports success. A store double whose `get`/`scan_page` fails,
  asserting propagation, would go red on the mutant.
- NEEDS-HUMAN [human] — **Two extra metadata point reads per committed object per pass, for
  objects that are entirely flat.** Measured with a counting `MetadataStore` over 50 committed
  **flat** objects, same probe on both trees: base `backfill::reconcile` = `gets=0`, patched =
  `gets=100`; base restore pass = `gets=0`, patched = `gets=100`. The cause is
  `crates/custodian/src/resolve.rs:81`, which re-reads the root through
  `metadata::resolve_current_chunk_map` for *every* record of every pass
  (`gc.rs:337`, `restore.rs:383`, `rebalance.rs:168`, `backfill.rs:109`,`:252`), including flat
  maps that need no resolution. In `crates/custodian/src/reconstruction.rs:615` the multiplier
  is per **repair obligation** — `find_chunk` re-resolves every committed object in the store,
  including every segmented object's whole `seg:` range, once per queued repair. The brief
  asked for one shared resolver, not for maintenance passes to switch from their scan snapshot
  to a live per-object re-read; the narrower option exists and is already used by the read
  paths (`metadata::resolve_live_chunk_map`, `crates/core/src/metadata.rs:3063`). It is
  documented as deliberate at `resolve.rs:27-44`, so overriding it is a scope/fitness decision —
  but no gate measures it and no test covers the cost.
- NEEDS-HUMAN [impl] — **Backfill resolves every committed object twice per pass.**
  `crates/custodian/src/backfill.rs:109` resolves each record to classify it and then
  `crates/custodian/src/backfill.rs:252` (`emit_remaining`) resolves the whole store again for
  the gauge — for a segmented object that is a second full `seg:` range read of every segment
  record, discarding the list the first loop already had. Accumulating the count in the main
  loop removes the second walk without changing the gauge.
- NEEDS-HUMAN [impl] — **Orphaned doc comment on the containment attribution surface.**
  `crates/custodian/src/gc.rs:401-408` is written for `emit_unresolvable` ("Emit a committed
  object whose chunk map could not be resolved …") but sits directly on
  `emit_uncommitted_unreadable` (`:418`), whose own doc begins at `:409`; the real
  `emit_unresolvable` (`:429`) is left undocumented. rustdoc renders the wrong description for
  the function, on exactly the surface leg 7's containment story rests on.

## Attempted and could not refute

- The red→green itself (reproduced independently, above) — and the test drives the **real**
  `reconcile_step`, `reconcile_after_restore`, `reconciliation_status`,
  `metadata::high_water_marks`, `core::read::read_object`, `backfill::reconcile` and the real
  rebalance/reconstruction repoints, not doubles of them; the only stand-in is the store/fleet.
- Flat-record byte identity: `ChunkMap::Flat` serializes as a bare JSON array
  (`crates/core/src/metadata.rs:1382-1389`), field order and every `skip_serializing_if` are
  unchanged (`:1723-1763`), and leg 5 asserts stored `inode:` bytes byte-for-byte after every
  pass — all green in my run.
- `high_water_marks` totality (`crates/core/src/metadata.rs:5456`): I could not construct an
  arrangement of store contents that returns `Err` — every walk is paged, an undecodable
  `inode:`/`seg:` value is attributed and its ids recovered from the bytes, and no root is
  resolved.
- Resolver boundedness and ordering: `read_group_range`
  (`crates/core/src/metadata.rs:2763-2805`) refuses a table past `MAX_ROOT_SEGMENTS`, pages one
  row wider than the root's own claim, and orders by the **parsed** index; `seg_range_prefix`
  ends in `:` so epoch `1` cannot bleed into epoch `12`; `parse_seg_key` rejects a non-canonical
  epoch and a non-fixed-width index.
- Missed mutants in `json_chunk_id_floor` / `json_string_token`
  (`crates/core/src/metadata.rs:5044-5275`, `mutants.out/missed.txt`): I expected a floor
  under-approximation, but `raw_chunk_id_floor` takes `max(parsed, scanned)` and the byte
  scanner recovers the same ids, so those survivors are redundancy, not a hole.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The rebuild must require exact current root/table identity, because an independent probe changed the table while retaining its group and the group-only retry check returned stale chunks instead of failing closed (`crates/core/src/metadata.rs:2907`).
- [ ] T3 Runtime — The maintainer must accept landing a dormant segmented committer before issue #636 and its extra root read per committed object per maintenance pass, because production reach and operational cost are intentionally deferred (`crates/custodian/src/resolve.rs:39`).
- [ ] T4 Contribution — A human must settle the four recorded batch-review items and closed/rejected prior-art status, because the prescribed `scripts/review-branch` wrapper is absent and its red contribution result cannot be independently adjudicated.
- [ ] T5 Judgment — The rebuild must add assertions that kill independently reproduced boundary survivors, because mutable GC error guards and raw-recovery branches can change without failing the current suite (`crates/custodian/src/gc.rs:307`).
- [ ] Validation — fitness-to-purpose — The maintainer must decide whether synthetic raw-record, redb, and DST evidence is fit for release before issue #636 supplies a real production publisher caller, because the staged committer is not yet exercised by deployed topology (`crates/core/src/metadata.rs:3395`).
- [ ] **One damaged object stops *all* space reclamation, cluster-wide, and
- [ ] **The blocked pass mis-attributes itself.**
- [ ] **A transient concurrency outcome is classified as object damage.**
- [ ] **The containment classifier itself is unpinned.** `cargo mutants`
- [ ] **Two extra metadata point reads per committed object per pass, for
- [ ] **Backfill resolves every committed object twice per pass.**
- [ ] **Orphaned doc comment on the containment attribution surface.**
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): T4 batched rubric review (gating) fails genuinely — 4 blocking findings independently re-verified against the patched worktree (applied patch.diff to a scratch clone of main @ 9120f7a), all still present: 1. crates/core/src/metadata.rs:3273 (repoint_chunk, flat root arm) — `version: prior_root.version + 1` has no overflow guard, and this arm never calls check_record_ceilings, so a repair can silently grow a record past the 100 KB ceiling. 2. crates/core/src/metadata.rs:2409 (overwrite/repoint path) — `version: prior.version + 1` is computed before the segmented-prior refusal check runs, so a segmented inode at u64::MAX panics on overflow instead of returning SegmentedRetirementUnsupported. 3. crates/core/src/metadata.rs:5366 (segment_chunk_floor) — an unreadable segment record with no recoverable ID digits only emits telemetry and leaves max_chunk unchanged, risking re-mint of a still-live chunk id at startup. 4. Same overflow class as #2, distinct call site named in review-batch.md at metadata.rs:2426. None of these are Plan/brief gaps — the brief already specifies overflow-safe versioning, ceiling enforcement, and floor totality; these are implementation completeness bugs against already-settled spec, consistent with every prior iteration's carry-forward pattern. Fix in place; no scope/design change needed. Separately (not blocking, re-affirmed not reopened): the advisory adversary review's "one damaged object stalls fleet-wide GC/drain" finding is the exact behavior the brief's containment table pre-authorizes for deletion-capable passes (confirmed at Plan, 2026-07-27) — carry it forward as a re-affirmation item for the next sign-off, not a design question to redecide. §6 items left unticked this round — carry forward unchanged to the next Check attempt's sign-off.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
