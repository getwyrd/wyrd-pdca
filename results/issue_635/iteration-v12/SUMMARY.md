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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 494 mutants tested in 17m: 12 missed, 257 caught, 222 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #635: add byte-compatible segmented inode chunk maps with bounded shared resolution and fenced staged publication.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The exact-base brief settles representation, containment, compatibility, and publication boundaries sufficiently to judge the requested change without inferring scope. |
| C2 Reproduction (red pre-fix) | PASS | Independent base rerun compiled and all 9 focused tests failed at their behavioral assertions, demonstrating the missing segmented-map behavior rather than a broken harness (`crates/custodian/tests/segmented_map_consumers.rs:644`). |
| C3 Change | PASS | The representation gap is closed at the owning core abstraction and consumers share its bounded resolver, so the requested behavior is present without a parallel decoding path (`crates/core/src/metadata.rs:2946`). |
| C4 Verification (red→green) | PASS | Independent rerun changed the focused suite from 9/9 failed to 9/9 passed, and typos, docs, formatting, lint, build, workspace tests, dependency checks, and isolated cargo-deny all passed (`crates/custodian/tests/segmented_map_consumers.rs:644`). |
| C5 Causal adequacy | NEEDS-HUMAN | Maintainers must decide whether the parsed-plus-scanned corrupt-record floor is intentional defense-in-depth or redundant causal complexity — seven parsed-reader mutants survive because the scanner subsumes them, affecting maintainability of the startup-safety claim (`crates/core/src/metadata.rs:4793`). |
| T1 Structure | PASS | A single core resolver owns record and range semantics while the custodian wrapper only selects current-root behavior, avoiding divergent consumer implementations (`crates/core/src/metadata.rs:2946`). |
| T2 Shape | PASS | Flat-map JSON identity remains byte-compatible while structured records are rejected unless their cross-field invariants hold, preserving the persisted-data contract (`crates/core/src/metadata.rs:1391`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must accept caller-parameterized staged publication before #636 supplies the real Completing/session fence — it matters because the API is otherwise production-unreachable and its runtime contract is fixture-only (`crates/core/src/metadata.rs:3395`). |
| T4 Contribution | NEEDS-HUMAN | A human must inspect the unavailable four batch-review blockers and closed/rejected prior-art result before sign-off — affected-path merged history was clear, but `scripts/review-branch` and its finding output were absent, so the asserted red review gate could not be reproduced. |
| T5 Judgment | NEEDS-HUMAN [impl] | A rebuild must add negative controls for escaped-id recovery in broken JSON and non-ChunkMap GC errors — surviving mutants show the tests do not protect the claimed recovery and error-classification boundaries (`crates/core/src/metadata.rs:5048`; `crates/custodian/src/gc.rs:305`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether raw-record/redb fixtures are sufficient sign-off before #636 exercises a real session fence — otherwise fitness rests on a curated caller that cannot expose production integration failures (`crates/core/src/metadata.rs:3395`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Attacked the evidence, the fix and the verdict against the patched tree at `$PDCA_TARGET`
(base `9120f7a`). Four findings; the rest of the refutation attempts failed and are listed
at the end.

- **NEEDS-HUMAN [human]** — `crates/core/src/metadata.rs:5237` (`high_water_marks`) — **the
  chunk-id floor this round grew ~320 lines of new production code to compute is read by
  nothing, and the hazard it cites cannot occur on this tree.** The one caller,
  `crates/server/src/lib.rs:124`, binds it to `_max_chunk` and throws it away; no other
  non-test caller exists (`rg high_water_marks` → `server/src/lib.rs:124` plus doc prose).
  Worse, the id space it recovers is unmintable: `Gateway::mint_chunk_id`
  (`crates/server/src/lib.rs:238-240`) always yields ids ≥ 2^127 and the cluster minter
  `chunk_id_minter` (`crates/server/src/cli.rs:1716-1722`) always yields ≥ 2^64, so **no
  live path mints a `< 2^64` chunk id at all**. The code's stated justification — "a floor
  below an id whose fragments are on disk lets the allocator re-mint that id and clobber
  them (issue #364)" (`metadata.rs:4762-4769`) and "a floor below a live id costs an object
  its bytes" (`metadata.rs:5227-5228`) — is unwarranted, and it even cites
  `crates/server/src/lib.rs:123-124`, the exact line that discards the value. Concretely
  unwarranted claims that follow: leg A(vii)(a)'s `max_chunk >= in_segments` assertion
  (`crates/custodian/tests/segmented_map_consumers.rs:1207-1213`) tests a number no caller
  consumes, and the brief's containment-table row "it must **never under-approximate** the
  floor" is unfalsifiable in production. This is the carried §6 item "unreachable
  `max_chunk` cost", which iteration 8's sign-off asked to be resolved or declined *"not
  silent carry"* — it has since driven three review rounds (the escaped-leading-zero lexer,
  the truncated-prefix `2^64` boundary, the id-floor scan cost) and it produces **8 of the
  12 surviving mutants** in `check-gates.json` (`metadata.rs:4825/4829/4835/4847/4848/5033
  ×2/5056`, all inside `json_chunk_id_floor` / `json_chunk_id` / `json_string_token`). Only
  the **totality** half of the containment row is real (`recover` does `?` the call); the
  over-approximation half is buying nothing. A human must decide whether to keep growing
  this surface, shrink it to totality-only, or gate it behind a real consumer.

- **NEEDS-HUMAN [human]** — `crates/custodian/src/gc.rs:268-272` +
  `crates/core/src/metadata.rs:2315-2317` — **one damaged segmented object halts *all* GC
  reclamation fleet-wide, for ever, and the product offers no way to remove it.**
  `ReferenceSet::protects` returns `true` for *every* `(dserver, fragment)` in the fleet
  while `unresolvable` is non-empty (`gc.rs:269`), and it is the sole deletion gate
  (`gc.rs:162`) and the sole marking gate (`restore.rs:222`). Meanwhile the only in-product
  delete path, `Gateway::delete_object` → `metadata::unlink`
  (`crates/server/src/lib.rs:582`), refuses a segmented map outright
  (`metadata.rs:2313-2318`, `SegmentedRetirementUnsupported`), as do both superseding
  committers (`metadata.rs:2356-2360`, `:2426-2431`). Concrete case: take leg A(vii)'s own
  fixture — `DAMAGED_INODE` (root names a `seg:` record that was never written) — and then
  DELETE any *other*, healthy object. Its orphan grace records are laid, the grace window
  elapses, and GC skips every fragment on every pass for ever; the store leaks
  monotonically, and the damaged object cannot be deleted, overwritten or repaired by any
  API in the tree. The brief's containment table authorised "continuing while treating **the
  damaged object** as fully referenced"; the patch treats **the whole store** as fully
  referenced, and the composition with the (separately defensible) segmented-retirement
  refusal leaves the failure with no exit. The blast-radius trade this slice re-planned to
  avoid has moved rather than gone: from gateway availability to unbounded, unremediable
  storage leak. Needs a scope call — a bounded/attributed degradation, an escape hatch, or
  an explicit tracked deferral.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/gc.rs:163-168` — **the containment's own
  "attribute the blocker" rule is broken at the per-fragment audit seam.** When
  `unresolvable` is non-empty, `protects` short-circuits at `gc.rs:269` *before* `placed` or
  `malformed` are consulted, but the reason computed at `gc.rs:163-167` has only two arms,
  so every skipped fragment — including genuinely orphaned, past-grace fragments of
  *healthy* objects — is emitted as `emit_skip(dserver, frag, "referenced")`. Concrete
  failing case: seed the leg A(vii) store, DELETE a flat object, advance past `GRACE`, run
  `gc::reconcile`; the audit trail says its fragments are `referenced` when they are not —
  the operator is told the wrong cause on the one seam that explains why a fragment
  survived, and `emit_unresolvable` (which names the real blocker) fires once per damaged
  object, not per skip. Add an `"unresolvable-map"` reason arm and a test asserting it;
  today no test pins the skip reason for this path.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/gc.rs:307` and `:339` — **the documented
  boundary "anything that is *not* the object's own fault still propagates"
  (`gc.rs:283-291`) has no oracle.** Both match guards
  `err.downcast_ref::<ChunkMapError>().is_some()` survive mutation to `true`
  (`check-gates.json` C5 rows), i.e. the suite cannot tell containment from
  swallow-everything. Concrete failing case the tests would not catch: a `MetadataStore`
  double whose `get` of the inode root returns `Err` — that store fault reaches `:339`
  through `resolve::chunks_of` → `resolve_current_chunk_map` (`metadata.rs:3016`), and under
  the mutant it is reclassified as "one object unresolvable", so `referenced_fragments`
  returns `Ok` with an incomplete set and `reconciliation_status` answers
  `PendingUnresolvable` instead of surfacing the store failure — a silent
  skip of an indeterminate read, the rubric's *Absent or unsupported entries* class. Same
  for `:307` with a legacy **flat** record whose bytes fail serde (a non-`ChunkMapError`),
  which the doc says must abort the pass. Two doubles (failing `get`; corrupt flat record)
  and two `expect_err` assertions close both.

## Refutation attempts that failed

- **The red→green evidence.** I could not break it. Every symbol
  `crates/custodian/tests/segmented_map_consumers.rs` names is base-visible (checked its
  import list at `:56-74` against `git show HEAD:crates/custodian/src/lib.rs:23-44` and
  `crates/core`'s `read`/`write`/`metadata` surface); it seeds roots and segments as **raw
  bytes** in the settled encoding (`:266-311`) and names none of the types this slice adds
  (`rg 'ChunkMap|Segment\w+::|resolve_chunk|PendingUnresolvable'` over the file → no hits),
  so the RED should be assertion-shaped rather than the build error the brief warns falls
  through to an unconditional PASS. The assertions are specific enough not to pass for the
  wrong reason on the base: `:1112` requires the backfill error to *contain* `"segmented
  chunk map"`, which the base's `invalid type: map, expected a sequence` serde error cannot
  satisfy even though `expect_err` alone would. Caveat: I did **not** re-run
  `run-verify.sh` myself — the only warm build cache (87 GB) lives inside the read-only
  target and a scratch rebuild would be from zero — so this leg rests on inspection plus
  the C4-verify row, not on an independent execution.
- **Resolver ordering.** `read_segments` orders by the *parsed* index through a `BTreeMap`
  (`metadata.rs:2836-2853`), and a genuinely reversing store double that asserts it actually
  shuffled exists (`metadata.rs:8981-9075`) — the "must not rely on `scan` order" rule holds.
- **Prior rounds' blocking findings.** Iteration 10's two are closed:
  `read_group_range` now refuses `accounted > MAX_ROOT_SEGMENTS` rather than trusting the
  root's `segment_count` (`metadata.rs:2768-2773`), and `plan_with` refuses an empty
  placement before anything is durable (`metadata.rs:3577-3582`). Iteration 11's headline
  fence-ABA finding is closed too: the cycle rule now spans the durable prefix, the segment
  phase **and** the flip (`metadata.rs:3739-3747`, `:3990-3998`). I tried to construct an
  `A → B → A` that escapes it across a resumed prefix and could not — the adjacent-repeat
  skip at `:4547` is sound because one batch's put is the next batch's pin, and I verified
  the surviving mutant at `:3859` (`+`→`*` on `written`) is behaviourally *equivalent* under
  that dedup, so it is not a real gap.
- **Legacy byte identity.** `ChunkMap::Serialize` delegates straight to `Vec<ChunkRef>`
  (`metadata.rs:1382-1389`), field order and the three `skip_serializing_if` attrs are
  unchanged (`:1725-1763`), and `try_from = "InodeRecordWire"` touches only `Deserialize` —
  I could not find a flat record whose decode→encode moves a byte.
- **The ranged gateway leg** genuinely crosses a segment boundary rather than mirroring it:
  8×8-byte chunks split 4/4, range 28–39 straddling byte 32
  (`crates/server/src/lib.rs:1222-1250`), asserted against the payload slice, on the real
  `get_object_range`.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Maintainers must decide whether the parsed-plus-scanned corrupt-record floor is intentional defense-in-depth or redundant causal complexity — seven parsed-reader mutants survive because the scanner subsumes them, affecting maintainability of the startup-safety claim (`crates/core/src/metadata.rs:4793`).
- [ ] T3 Runtime — Maintainers must accept caller-parameterized staged publication before #636 supplies the real Completing/session fence — it matters because the API is otherwise production-unreachable and its runtime contract is fixture-only (`crates/core/src/metadata.rs:3395`).
- [ ] T4 Contribution — A human must inspect the unavailable four batch-review blockers and closed/rejected prior-art result before sign-off — affected-path merged history was clear, but `scripts/review-branch` and its finding output were absent, so the asserted red review gate could not be reproduced.
- [ ] T5 Judgment — A rebuild must add negative controls for escaped-id recovery in broken JSON and non-ChunkMap GC errors — surviving mutants show the tests do not protect the claimed recovery and error-classification boundaries (`crates/core/src/metadata.rs:5048`; `crates/custodian/src/gc.rs:305`).
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether raw-record/redb fixtures are sufficient sign-off before #636 exercises a real session fence — otherwise fitness rests on a curated caller that cannot expose production integration failures (`crates/core/src/metadata.rs:3395`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): T4 batch review gate re-run manually (authoritative per human), 3 blocking findings, none fixed or triaged in this round's review-rejected.md: - crates/core/src/metadata.rs:3879 [BUG] segment batches overwrite previously verified rows without a CAS — an ambiguous-flip recovery can race a live repoint, restore an already-orphaned stale placement, and lose the root flip after corrupting the live generation. - crates/core/src/metadata.rs:4002 [BUG] the supersede branch permits a segmented prior without requiring a retirement obligation, so callers can silently strand the prior generation's segment records and fragments (segmented retirement is unsupported elsewhere). - crates/core/src/metadata.rs:3965 [CONVENTION] flip_batch() swallows segment_batches()'s Err via unwrap_or_default(), so flip() can publish a root even when phase one is unfenced or fence-cyclic; the in-code rationale (both real entry points already report that error first) may not cover every caller, e.g. a bare recovery flip() — worth re-checking, not just re-asserting the existing comment. Do should fix findings 1 and 2 (or produce a reasoned decline in review-rejected.md the way prior rounds did), and either fix or explicitly decide finding 3. These sit in the core crash-safety property (0016 decision 7 staged publication) this slice exists to implement, so they should be resolved with code/tests, not adjudicated at sign-off.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
