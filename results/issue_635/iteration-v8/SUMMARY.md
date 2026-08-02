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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 393 mutants tested in 9m: 214 caught, 179 unviable

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

Task under review: add byte-compatible segmented inode chunk maps with staged publication and shared resolution across read and maintenance consumers (issue #635).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the wire bytes, decode invariants, consumer scope, containment behavior, and precursor boundary as independently testable outcomes. |
| C2 Reproduction (red pre-fix) | PASS | The base compiled with the raw-record fixture but all 9 consumer tests failed at decode/assertion, so RED is behavioral rather than compile-only (`crates/custodian/tests/segmented_map_consumers.rs:511`). |
| C3 Change | PASS | The shared resolver, staged publisher, and consumer migration cover the specified seam without unrelated dependency or build-system expansion (`crates/core/src/metadata.rs:1059`). |
| C4 Verification (red→green) | PASS | The focused suite moved from 0/9 on base to 9/9 patched, and `cargo xtask ci` passed typos, docs, deny, conformance, workspace tests, and the 50-seed DST (`crates/custodian/tests/segmented_map_consumers.rs:511`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The rebuild must cover every valid suffix of a truncated ID — prefix `1` returns 1,999,999,999,999,999,999 although `2^64−1` is valid, so recovery can let the allocator remint a live ID (`crates/core/src/metadata.rs:3823`). |
| T1 Structure | PASS | One core resolver and one custodian live-root adapter centralize the cross-consumer invariant without a new dependency edge or manifest churn (`crates/custodian/src/resolve.rs:76`). |
| T2 Shape | PASS | JSON-type discrimination preserves legacy flat bytes while decode rejects invalid segmented cross-field structure (`crates/core/src/metadata.rs:1112`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must accept landing the publication API before #636 supplies `Completing` and a production caller — this decides whether dormant persistence machinery may ship independently (`crates/core/src/metadata.rs:2660`). |
| T4 Contribution | NEEDS-HUMAN | A human must triage the six reported batch-review blockers — affected-path merged/closed history found only merged precedents, but the target lacks `scripts/review-branch` and the finding bodies are unavailable, so their validity cannot be re-derived. |
| T5 Judgment | NEEDS-HUMAN [impl] | The tests must use an independent boundary oracle for truncated IDs — the current expectations encode the same unsafe approximation and omit the prefix-`1` ceiling case, so they cannot catch the allocator regression (`crates/core/src/metadata.rs:7709`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether raw-record and Redb evidence is fit for the >10 GiB purpose before #636 supplies the real multipart producer — production publication topology was not exercised, so launch fitness remains unproven (`crates/core/src/metadata.rs:2660`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map

Attempted to refute: the leg-A red→green story, the settled encoding + decode invariants,
the staged committer's refuse-before-durable ordering, the resume-prefix verification, the
epoch scoping, the repoint CAS, and the flat decode→encode identity. **Could not** — the
encoding is byte-identical for legacy records (`crates/core/src/metadata.rs:4161`), every
invariant in the brief's leg B(ii) list has a raw-byte negative case
(`crates/core/src/metadata.rs:4234`), leg A's fixture names only base symbols so its RED is
genuinely an assertion (`crates/custodian/tests/segmented_map_consumers.rs:53-73`, verified
against `git show HEAD:crates/core/src/read.rs`), and the repoint binds home to root
generation (`crates/core/src/metadata.rs:2595-2614`). Three refutations landed.

- NEEDS-HUMAN [impl] — **`flip()` publishes a root over a range it never checked is
  complete.** `crates/core/src/metadata.rs:3244` calls `verify_durable_range`, whose only
  presence check is `planned[..claimed]` where `claimed == self.resume_from`
  (`crates/core/src/metadata.rs:3163`, `:3200`); the tail check
  (`PublicationTailStranded`, `:3188`) only catches *extra* records. Concrete case, using
  the repo's own recovery flow: `crates/dst/tests/custodian.rs:2005` builds `recovered`
  with `resume_from = 2` and `planned_next.len() > 2`. Drop the `write_segments` call at
  `:2018` (or let it return `Conflict` — `commit_batches` stops at the first batch that
  does not commit, `crates/core/src/metadata.rs:3228`, and a caller that retries the flip
  reaches exactly this state) and `recovered.flip(&meta)` returns `Ok(Committed)`: the
  durable range holds indices {0,1}, the flipped root names N segments, and every later
  `resolve_chunk_map` of that inode is `Err(SegmentAbsent)` — permanently unresolvable,
  "silent at publication, terminal at read", the exact failure class carry-forward item 3
  and `:3141-3160` were written for. The doc at `:3239-3243` claims the flip re-verifies
  precisely for "a completer that drove the phases separately", but with `resume_from <
  planned.len()` — the normal recovery value — it verifies only the prefix that was already
  durable before the phase ran. Fix is phase-specific: at flip time the whole plan must be
  present, not `planned[..resume_from]`.

- NEEDS-HUMAN [impl] — **the containment table's `reconciliation_status` row is not
  implemented and not tested.** With the damaged object of leg A(vii) seeded,
  `reconciliation_status` (`crates/custodian/src/desired_state.rs:157`) calls
  `referenced_fragments`, which now propagates the resolver's error with `?` at
  `crates/custodian/src/gc.rs:265` → `retired_or_fail` → `Err(SegmentAbsent)`
  (`crates/core/src/metadata.rs:2233`). So a *single* damaged object makes the drain-status
  surface return `Err` for **every** D server in the store, where the brief settles the
  answer as "`PendingMalformed` — refuse to certify, **attribute** the blocker, keep going"
  (Design § Failure containment, mirroring `desired_state.rs:166-179`). A reviewer can
  rationalise this as conforming ("`Err` is not `Satisfied`"), which is exactly why it slipped:
  the row asks for containment, and `Err` is the store-wide blast radius the containment table
  exists to prevent. Leg A(vii) never calls `reconciliation_status` over the damaged store
  (`crates/custodian/tests/segmented_map_consumers.rs:1097-1198` asserts only the id floor,
  the healthy reads, the typed per-object failure and the fragment counts), and the drain leg
  that does call it (`:707`) seeds no damaged object — so no test in this bundle would have
  gone red on it.

- NEEDS-HUMAN [human] — **the whole `max_chunk` half of `high_water_marks` is unreachable
  from production, and its stated justification is false on this tree.** The sole production
  caller discards it: `crates/server/src/lib.rs:124` — `let (max_inode, _max_chunk) = …`,
  because chunk ids are minted `≥ 2^127` from a random per-gateway epoch
  (`crates/server/src/lib.rs:238`, base code, unchanged by this patch). Yet this diff adds
  `segment_chunk_floor` (`crates/core/src/metadata.rs:3870`, a **paged walk of the entire
  `seg:` namespace executed on every `Gateway::recover`**) plus ~200 lines of byte-level id
  scavenging (`:3691`, `:3710`, `:3753`, `:3808`, `:3823`) and eight tests, all justified at
  `:3947-3954` by "a floor below a live id costs an object its bytes (issue #364)" — a
  hazard no live allocator can reach here. The result is a startup cost that scales with the
  segment namespace, bought for a discarded value, in the slice whose iteration-7 rejection
  was reviewability. This needs a human call because the brief itself asked for it (leg
  A(vii)(a) pins the chunk-floor property), so it is the brief that is stale, not the build.

- NEEDS-HUMAN [human] — **maintenance passes go from one scan to O(N) point reads per pass,
  and reconstruction to O(queue x N).** Every consumer now resolves through
  `resolve_current_chunk_map`/`_homes`, which re-`get`s the root per object even for a
  **flat** map (`crates/custodian/src/resolve.rs:81`, `:101`; `crates/core/src/metadata.rs:2320`).
  `crates/custodian/src/reconstruction.rs:602` (`find_chunk`) is called once per queued
  repair from `assess`, and now performs a `get` per inode inside its `inode:` scan — so a
  pass with Q obligations over N inodes issues up to Q x N sequential round trips where the
  base issued Q. On a networked backend (FDB/TiKV) that is a scalability cliff, not a
  constant factor. It is deliberate (`resolve.rs:27-44` argues the stale-snapshot hazard),
  which is why it is a human call rather than an iteration: keeping the live-root re-read
  for the *segmented* shape only, or resolving lazily in `find_chunk`, would keep the stated
  safety property for the shape that needs it without the flat-map round trips.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — The rebuild must cover every valid suffix of a truncated ID — prefix `1` returns 1,999,999,999,999,999,999 although `2^64−1` is valid, so recovery can let the allocator remint a live ID (`crates/core/src/metadata.rs:3823`).
- [ ] T3 Runtime — Maintainers must accept landing the publication API before #636 supplies `Completing` and a production caller — this decides whether dormant persistence machinery may ship independently (`crates/core/src/metadata.rs:2660`).
- [ ] T4 Contribution — A human must triage the six reported batch-review blockers — affected-path merged/closed history found only merged precedents, but the target lacks `scripts/review-branch` and the finding bodies are unavailable, so their validity cannot be re-derived.
- [ ] T5 Judgment — The tests must use an independent boundary oracle for truncated IDs — the current expectations encode the same unsafe approximation and omit the prefix-`1` ceiling case, so they cannot catch the allocator regression (`crates/core/src/metadata.rs:7709`).
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether raw-record and Redb evidence is fit for the >10 GiB purpose before #636 supplies the real multipart producer — production publication topology was not exercised, so launch fitness remains unproven (`crates/core/src/metadata.rs:2660`).
- [ ] **`flip()` publishes a root over a range it never checked is
- [ ] **the containment table's `reconciliation_status` row is not
- [ ] **the whole `max_chunk` half of `high_water_marks` is unreachable
- [ ] **maintenance passes go from one scan to O(N) point reads per pass,
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
- Iteration delta (if iterating): T4 batch-review (round 8) gate fail is genuine, not noise: 6 findings map to 4 distinct bugs, none yet triaged (fixed or recorded-rejected) per the bundle's own round-1..7 triage precedent. Required for round 9: 1. metadata.rs:3200 flip() verifies only 0..resume_from — a partial/zero-write flip can publish a root naming un-written segments, causing permanent SegmentAbsent on read. Independently confirmed by the adversary review too — fix the range verified at flip time to the whole plan. 2. metadata.rs:3455 fence-transition check accepts any differing put, so a later duplicate put can restore the pinned fence value and let a racing rollback delete segments after the flip — needs the same "fence must transition" treatment as prior rounds' fence work, or an explicit recorded decline with reason if judged out of scope. 3. metadata.rs:3757 id-floor fallback scanner matches only literal "id" bytes; an escaped-equivalent duplicate key can hide a larger live chunk id from the startup floor — fix the scanner or record-reject with reason. 4. metadata.rs:3834 truncated-prefix-at-2^64-ceiling underestimate (e.g. prefix "18") lets recovery re-mint a still-live chunk id — same class as the already-flagged C5/T5 NEEDS-HUMAN items; fix the boundary case and give the tests an independent oracle so they'd catch the regression. Also carry forward the other §6 human-only items (T3 dormant-API landing-order call, unreachable max_chunk cost, O(N) maintenance-pass round-trip regression, containment-table reconciliation_status row) for explicit resolution/decline in the rebuild, not silent carry.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
