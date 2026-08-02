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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 469 mutants tested in 17m: 11 missed, 246 caught, 209 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: only 0/3 passes produced a usable result after one retry — refusing to certify a thinner union. Re-run wh
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — NOT COMPLETED

The reviewer did not produce a verdict table (reviewer leaf failed: Command '['codex', 'exec', '--sandbox', 'workspace-write', '--skip-git-repo-check', '-m', 'gpt-5.6-sol', '-c', 'model_reasoning_effort=xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt-l0', '-c', 'sandbox_workspace_write.network_access=true', '--json']' returned non-zero exit status 1.).

<!-- pdca:leaf-status infra-empty -->

Failure class: **transient infra — safe to re-run.** The leaf exited non-zero with no output and retries did not recover, so it almost certainly hit a usage/rate limit or a transient API/network error rather than reviewing the diff; a sibling advisory leaf of a different family may already have covered it. See `check-review.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.

### Advisory — adversary

# Adversarial review — issue #635 (segmented-chunk-map), advisory

Method: re-read the diff against the target source at `$PDCA_TARGET`, then attacked the
production API directly from a throwaway crate that path-depends on the patched
`wyrd-core` (built and run, then deleted). Findings below that say "verified" were
executed, not argued.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:3224`: this slice's own committer
  mints exactly the record its own backfill declares unrecoverable, and one of them fails
  every backfill pass over the whole store, for ever.** `plan_with`'s write-boundary check
  refuses only a *malformed* placement (`chunk.checked_fragments()` error); an **empty**
  `placement` is *valid* there (ADR-0040 decision 3, the pre-M3 identity spelling), so
  `SegmentedPublication::publish` accepts it. Verified against the patched tree: 1 500
  chunks with `placement: vec![]` ⇒ `plan() -> Ok(3)`, `publish(&store) -> Committed`,
  and the resolve returns them with the placement still empty. That is precisely the
  record `crates/custodian/src/backfill.rs:163-167` collects into `unfillable` and
  `:216-223` converts into `Err(SegmentedPlacementUnfillable)` — raised for the **entire
  pass**, deterministically, on every future run (the diff's own test pins the fatality:
  `crates/custodian/tests/segmented_map_consumers.rs:1107-1109`), with the error text
  itself saying "no other pass drains them". The premise the fatal error rests on is
  stated at `crates/custodian/src/backfill.rs:313` — "structurally impossible today: a
  segmented map is produced only by a multipart Complete, which always records a
  full-length placement" — and the only producer this slice ships falsifies it. Concrete
  failing case: publish any segmented map through the shipped committer with a chunk whose
  `placement` is empty (e.g. a #636 caller that skips `plan.place(topology)`), then run
  `backfill::reconcile` — `Err` on that pass and on every pass after it, with no repair
  path anywhere in the slice. Cheapest fix in Do's hands: refuse an empty placement in
  `plan_with` too (make the "structurally impossible" premise true by construction), or
  make the backfill skip non-fatal for the pass.

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2491` claims a bound the resolver
  does not enforce; the real budget is the untrusted root's own claim.** The doc says the
  segmented resolve reads "the bounded per-object range `seg:<nonce>:<epoch>:` (≤
  `MAX_ROOT_SEGMENTS`)", but `read_group_range` (`:2461`) passes
  `accounted.saturating_add(1)` — where `accounted` is the *root's declared*
  `segment_count` — as both the page limit and the materialisation budget, and nothing
  rejects a root naming more than 512 (`:284-288` makes the decode deliberately liberal).
  Verified: a root value of **97 309 bytes** — inside `MAX_VALUE_BYTES` (100 000), i.e. a
  value every backend in play accepts — decodes to `Ok(segment_count = 2000)`, 3.9× the
  documented ceiling; `n = 513` likewise decodes `Ok`. So a corrupt or non-conforming
  generation is resolved with a ~2 000-record budget (≈200 MB at the value ceiling) on the
  GET path and inside every maintenance pass. That is the same "materialise a corrupt
  generation's whole range" shape iteration 9 raised (`metadata.rs:2170` then) moved from
  `SCAN_CAP` onto the root's claim rather than closed. Fix: clamp `accounted` at
  `MAX_ROOT_SEGMENTS` in `read_group_range` (no publication can produce a longer table, so
  a longer one is already unresolvable) — or restate the claim at `:2491` to the bound the
  code actually holds.

- **NEEDS-HUMAN [human] — the §1(vii) containment leg is proven only through GC; the two
  consumers that abort are never run against the damaged object.**
  `crates/custodian/tests/segmented_map_consumers.rs:1261-1270` calls
  `reconcile_step(&zone, &custodian, Some(&gc), None, None, None, …)` — scrub,
  reconstruction and rebalance are `None`, and the drain is set only afterwards at
  `:1287`. But `crates/custodian/src/reconstruction.rs:615` and
  `crates/custodian/src/rebalance.rs:168` call `resolve::homes_of(…)?` with **no**
  `ChunkMapError` containment (contrast `gc.rs:307`, `:323`). Concrete configuration: the
  same damaged object plus *either* one queued repair obligation *or* one requested drain
  ⇒ `reconcile_step` returns `Err` on every cadence (`reconciliation.rs:96`, `:105`), so
  GC's "attribute the blocker and keep going" containment (`gc.rs:292-330`) never gets to
  matter, reconstruction stops repairing under-replicated chunks fleet-wide, and the drain
  the status surface politely reports as `PendingUnresolvable` can never progress — while
  the slice ships no repair path for the object it names. The brief's containment table
  does permit a byte-moving pass to abort, so this is a sign-off judgement, not a spec
  breach: accept that per-object containment covers reads, the id floor and the status
  surface only, or have Do contain the same downcast at those two call sites.

- **NEEDS-HUMAN [human] — the bundle carries no multi-pass rubric review at all this
  round, and the two findings above are in the classes that review kept producing.**
  `check-gates.json` T4 row: "only 0/3 passes produced a usable result after one retry —
  refusing to certify a thinner union". Rounds 5–9 each produced 5–6 blocking findings
  from that same tool (write-boundary strictness, corrupt-generation materialisation,
  containment spelling), and this round's silence is a tool failure, not a clean bill.
  `C4 ci` and `C4-verify` are unaffected. Treat T4 as "not run", not as "nothing found",
  when weighing the bundle at sign-off.

## Attempted and could not refute

- **The red→green evidence.** `crates/custodian/tests/segmented_map_consumers.rs` names
  only base symbols (raw-byte fixtures at `:266-310`, a local `seg_key`, no
  `ChunkMap`/`SegmentRef`/resolver reference, `ReconciliationStatus` compared through
  `Debug` rendering at `:1305-1312`), and its passes are `unwrap`ped (`:645`, `:652`,
  `:682`, `:747`, `:763`), so on the pre-fix tree the red is genuine assertion/panic failures from
  `metadata::decode` rejecting a segmented value — not a build error. It drives the real
  `reconcile_step` / `reconcile_after_restore` / `reconciliation_status` /
  `high_water_marks` / `read_object`, not re-implementations. Leg 2 (`:810-813`) is genuinely
  discriminating: a resolver that decoded the root but skipped the `seg:` range answers
  `Satisfied`.
- **Serialization identity (leg B(i)).** Verified: legacy flat bytes decode→encode
  byte-identically through the new `ChunkMap` enum, so no `require(key, encode(prior))`
  CAS is broken (`metadata.rs:1232-1272`).
- **The id floor's totality and over-approximation.** Verified over a store holding a
  published segmented map plus a `seg:` value truncated mid-`id` token: `high_water_marks`
  returns `Ok` and widens the torn token upward (`98765…` ⇒ `9876599999999999999`), never
  under-reporting; the escaped/padded-digit lexer at `metadata.rs:4380-4530` and
  `widest_id_with_prefix`'s capped walk both check out by hand at the `2^64` boundary
  (prefix `18` ⇒ `18446744073709551615`, prefix `19` ⇒ `1999999999999999999`).
- **Fail-closed on an in-range stray.** Verified: a row the live root does not name yields
  a typed `SegmentUnknown` through `retired_or`, not a torn map.
- **Publication ordering and the resume probe.** `publish` assembles both phases before
  any write (`metadata.rs:3785-3792`), `flip` verifies `DurableRange::WholePlan`
  (`:3745-3747`), and `verify_durable_range` walks every planned index rather than the
  cursor's last record (`:3660-3700`) — the iteration-5/7/8 defects look genuinely closed.
- **Consumer coverage.** Every non-test `.chunk_map` reader in the workspace now goes
  through the shared resolver or is deliberately refused (`unlink` / `commit_chunk_map*`
  raise `SegmentedRetirementUnsupported`, `metadata.rs:2017-2021`, `:2060`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] leaf did not run (transient infra — safe to re-run) — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: only 0/3 passes produced a usable result after one retry — refusing to certify a thinner union. Re-run wh
- [ ] external dependency: codex reviewer quota (the T4 gate's model credits) — the third

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
- Iteration delta (if iterating): Two verified implementation bugs from the adversary review must be fixed before the next Check attempt: 1. `crates/core/src/metadata.rs:3224` (`SegmentedPublication::publish` / `plan_with`) accepts a chunk with an empty `placement`, which `crates/custodian/src/backfill.rs:163-167,216-223,313` then treats as "structurally impossible" and turns into a fatal, store-wide, permanently repeating `Err` on every future backfill pass. Refuse an empty placement in `plan_with` (make the "structurally impossible" premise true by construction), or make the backfill skip non-fatal for that pass. 2. `crates/core/src/metadata.rs:2461` (`read_group_range`) trusts the root's own declared `segment_count` as the resolve budget instead of clamping to `MAX_ROOT_SEGMENTS` (documented at `:2491`). Verified: a 97,309-byte root (inside `MAX_VALUE_BYTES`) decodes to `segment_count = 2000`, ~4x the stated ceiling, and is resolved anyway (~200MB materialised per read). Clamp `accounted` at `MAX_ROOT_SEGMENTS` in `read_group_range`, or restate the documented bound to match the code. Also: the T4 batched multi-pass rubric review (3x codex) and the primary Check reviewer leaf both failed this round on transient infra (quota) with no usable output — re-run both for real coverage on the next attempt; do not treat this round's silence as a clean bill. Out of scope for this iteration (recorded separately as an Act candidate, §10): the reconstruction/rebalance containment gap for a damaged chunk-map object — file as a foundation-milestone issue rather than blocking this bundle, per the brief's containment table allowing a byte-moving pass to abort.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- File an issue (foundation milestone): reconstruction and rebalance have no per-object containment guard against a damaged chunk-map object (`resolve::homes_of` uncontained at `reconstruction.rs:615`, `rebalance.rs:168`, contrast `gc.rs`'s containment) — from the #635 adversary review; brief's containment table permits a byte-moving pass to abort, so this is a scope call, not a spec breach.
