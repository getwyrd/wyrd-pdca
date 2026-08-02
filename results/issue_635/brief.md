# Design proposal — issue 635 / segmented-chunk-map

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on
> `origin/main` @ `9120f7a`. **Decision 7 (`0016:2280-2496`) IS this slice's design**, together
> with the `seg:` / `seggrp:` rows of the §1 record table (`0016:354`, `:502-527`) and the
> object-ceiling arithmetic (`0016:218-232`). **Do MUST read decision 7 in full before writing
> code** — especially §(a) the record shape, §(b) staged publication, §(c) the epoch-scoping
> crash story, and §(e) bounded resolution by every maintenance consumer. This brief does not
> restate it; it scopes it, **settles the record encoding** (so the test oracle is independent of
> the implementation's choice), resolves the one editorial contradiction 0016 still carries, and
> states the C4 shape.
>
> **Iteration 6. The previous attempt was rejected on the BASE, not on the design** — the brief
> named a wave-fold branch (`origin/pdca-integration/main`) that never existed, so Do built and
> verified on a tree WITHOUT #634 and shipped store doubles that cannot compile on the real stack.
> **That is fixed at the root here:** #634 merged to `main` as PR #645 (`9120f7a`) on 2026-07-27,
> so the plain target base now carries `scan_page` and there is no stack, no fold branch, and no
> `$PDCA_VERIFY_BASE` in play. See `Ordering note` and `Falsifiability`.
>
> Every citation below re-verified against `origin/main` @ `9120f7a` on 2026-07-27.
> This is **seam (ii) of five** in #508's re-plan (634 ✅ merged → **635** → 636 → 637 → 508).

- **Slug:** segmented-chunk-map
- **Kind:** enhancement (design proposal)
- **Goal:** `InodeRecord.chunk_map` graduates from a flat list to **`Flat | Segmented`**, so a
  published map larger than one backend value can exist at all — the >10 GiB launch requirement.
  Flat stays exactly as it is (`crates/core/src/metadata.rs:268`, `pub chunk_map: Vec<ChunkRef>`)
  and every existing record keeps decoding **byte-identically**; segmented carries a group
  identity plus `seg:<group-nonce>:<epoch>:<index>` segment records and their `seggrp:`
  reservation, is published by **staged publication** (write the segments, then flip the root),
  and is resolved through **one shared resolver that every `.chunk_map` consumer goes through**.
- **Success criterion:** **ONE** new test file (leg A, the binding one — deliberately written to
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
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — no container,
  no cluster, no deploy stack. **Leg A carries the binding red and it is a genuine assertion
  failure**: on the base a segmented-shaped `inode:` value makes `metadata::decode` fail and the
  `?` at `crates/custodian/src/gc.rs:256` propagates out of `gc::reconcile` through
  `reconcile_step` (`crates/custodian/src/reconciliation.rs:65-114`), so leg A(i) fails with
  `Err`, not with a compile error. The same is true for restore (`restore.rs:373-376`), rebalance
  (`rebalance.rs:147-148`), reconstruction (`reconstruction.rs:603-608`), backfill
  (`backfill.rs:76-81`) and `high_water_marks` (`crates/core/src/metadata.rs:847-857`) — every
  consumer decodes with the same strict `?`.
  **The base is `origin/main` @ `9120f7a`, and it CARRIES #634.** PR #645 merged on 2026-07-27, so
  `MetadataStore::scan_page` is a **required** trait method on the base
  (`crates/traits/src/lib.rs:1105`; the trait states deliberately why it has no default body).
  Three consequences Do must honour, and they are the reason iteration 5 was rejected:
  1. **Every `MetadataStore` double this slice adds MUST implement `scan_page`** — one delegating
     line to `wyrd_testkit::test_double_scan_page` (signature at `crates/testkit/src/lib.rs:810`).
     The peer to copy verbatim is `crates/custodian/tests/gc.rs:73-80`. `wyrd-testkit` is already a
     dev-dependency of `wyrd-custodian` (`crates/custodian/Cargo.toml:34`), `wyrd-core` and
     `wyrd-server`, so **no `Cargo.toml` edit is needed and none may be made**: a modified
     `Cargo.toml` is reverted on C4-verify's RED leg (`run-verify.sh:407-414`), which would turn
     leg A's assertion-red into a build error.
  2. **There is no stack and no fold branch.** The brief's base is a plain `main`, so
     `run-verify.sh` resolves `origin/main` by its brief-base rule (`_resolve_base_ref`,
     `engine/scripts/run-verify.sh:186-192`) — verified by dry-run: `--print-base` ⇒ `origin/main`.
     Do's worktree resolves from the SAME field (`src/pdca_harness/worktree.py:46-69`), and publish
     opens the PR against it (`publish._resolve_target`), so build base == test base == PR base.
     **If Do observes `$PDCA_BASE` or `$PDCA_VERIFY_BASE` set in its environment, or a `stack-base`
     file in the bundle, STOP and report it** — either would override the brief and reintroduce
     exactly the iteration-5 divergence (dry-run confirms a stale `$PDCA_VERIFY_BASE` wins over
     this field). Neither is present today.
  3. **Do MUST record, from the RED leg, how many tests actually ran and failed**, and state
     whether the red was assertions or a build error. Leg A's red must be **assertions**; a build
     error there means the base or the test's symbol set is wrong, not that the bug is caught.
     (This matters because the RED leg's `TESTS_RAN == 0` guard sits inside the cargo-*succeeded*
     branch (`run-verify.sh:416-427`); a build failure skips it and falls through to the
     unconditional `PASS` at `:433`.)
  **Corollary the test MUST obey:** `crates/custodian/tests/segmented_map_consumers.rs` may
  reference **only symbols present on the base** — it must **not** name `ChunkMap`, `SegmentRef`,
  `seg_key`, the resolver, or anything else this slice adds. Everything it needs is base-visible:
  `WriteBatch`, `MetadataStore` (incl. `scan_page`), `reconcile_step`,
  `GcContext`/`ScrubContext`/`ReconstructionContext`/`RebalanceContext`/`BackfillContext`,
  `reconcile_after_restore`, `RestoreReport`, `reconciliation_status`, `set_lifecycle`,
  `mark_orphaned` (all re-exported at `crates/custodian/src/lib.rs:33-45`),
  `metadata::high_water_marks`, and raw record bytes. Leg B lives inside the production modules, so
  it may name the new types freely.
  **Gate-evaluability, dry-run at Plan:** `run-verify.sh --classify` over the expected file set
  returns exactly one `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs` (plus
  `CRATE crates/{core,custodian,dst}`), so C4-verify runs
  `cargo test -p wyrd-custodian --test segmented_map_consumers` with production reverted and that
  file kept — a real per-fix RED. The file carries no `#![cfg(...)]`, so it is not compiled out
  (`run-verify.sh:344-366`, the `#![cfg(madsim)]` hazard that applies to `crates/dst/tests/*`).
- **Invariant to restore:** **an object's chunk map is resolvable, in bounded work, by every
  process that is entitled to act on the object — and no representation of it may be
  understood by one consumer and opaque to another. Its failure, when it does fail, is scoped to
  the object that failed.** Stated over the category: the map is the
  authoritative statement of which bytes a live object owns, so any consumer that cannot resolve
  it either fails safe (halting maintenance) or, worse, concludes the bytes are unowned. Both
  are outcomes 0016 forbids — and a third, added this round, is a per-object fault that removes
  the availability of every *other* object. **Source:** 0016 decision 7(a) — "because this changes
  the shape of `InodeRecord` for **every** consumer of `.chunk_map` … it is the strongest
  ADR-graduation candidate here" (`0016:2313-2330`) — and 7(e), which makes bounded resolution by
  *each* named consumer normative (`0016:2463-2469`); resting on the custodian's written safety
  rule that a referenced fragment is never reclaimed (`0005:294-295`, enforced at
  `crates/custodian/src/gc.rs:159-170`), and on this repo's existing containment precedent for an
  untrustworthy record — refuse to certify, **attribute**, and keep going
  (`ReconciliationStatus::PendingMalformed`, `crates/custodian/src/desired_state.rs:166-179`).
  SELF-TEST: this cannot be satisfied by guarding one module — a resolver wired into the read path
  alone satisfies every read test and still lets restore strand and GC delete a live object, which
  is exactly the recorded #508-attempt-4 failure.
- **Scope:** the `Flat | Segmented` record shape and its settled encoding, the `seg:` /
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
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 634
- **Ordering note:** #634 is a **build-on dependency, not a file conflict** — that misclassification
  is what sank iteration 5. #634 adds `scan_page` as a **required** `MetadataStore` method with no
  default body, so every store double this slice writes must implement it; and #634 touched the
  same in-test doubles this slice's `InodeRecord` change touches. The dependency is now
  **discharged by a merge, not by a fold**: #634 reached COMPLETE, published as getwyrd/wyrd PR
  **#645**, and the human merged it to `main` (`9120f7a`) on 2026-07-27. So this bundle is an
  ordinary wave-0 bundle against plain `main`; there is no integration branch to name and none
  should be invented. Downstream, **#636 / #637 / #508 now depend on THIS slice** in the same
  way — their briefs are their own, but their base should be `main` once this one merges, by the
  same rule.
  **Not a dependency, but a sequencing fact Do must know:** the *only* producer of a segmented map
  is a multipart Complete (`0016:2287-2299` — segmentation is multipart-only; a single
  `PutObject` stays flat by chunk-size selection). So this slice ships the shape, the records, the
  resolver and the staged-publication committer, and **#636 wires the session fence into it**. See
  `Design § the fence is a parameter`.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `typos`, `docs-renderer`
  <br>*(Both on the field's own line — the driver reads only that line,
  `src/pdca_harness/brief.py:242-260` — and both are registered `[[doctor.checks]]` rows
  (`pdca.toml:421`, `:428`). Needed because this slice's docs-currency edit to
  `docs/design/architecture/{06,08}` is gated by the prose gates, which `cargo xtask ci`
  **warn-skips** when the tools are absent, so a locally-green docs change opens the PR red on
  the host's always-on jobs, INTEGRATION §3. Both are installed on this host.)*
- **Test file:** `crates/custodian/tests/segmented_map_consumers.rs`
  <br>*(ONE added test file, deliberately — see leg B. The driver parses only this label's own
  line, `src/pdca_harness/brief.py:23-31`.)* Leg B ships as **co-located `#[cfg(test)]` unit tests
  in the production modules** — as do **leg A(vi)'s two gateway legs**, which cannot live in the
  custodian binary (see the placement rule in leg A(vi)) — and leg B(viii)'s X51 interleaving is
  **appended to the existing** `crates/dst/tests/custodian.rs`. None of these is an added test
  file, so none can join C4-verify's invocation and break leg A's assertion red.
  The one added file must be under a `tests/` directory: `run-verify.sh` classifies on an **added**
  `*/tests/*.rs` (`engine/scripts/run-verify.sh:93`, `:301-311`), so a case appended to an
  existing suite would degrade the gate to green-only and prove nothing per-fix. Both crates
  already carry the dev-dependencies these need — `wyrd-custodian` has `async-trait`, `bytes`,
  `tokio`, `wyrd-testkit`, `wyrd-coordination-mem`, `wyrd-chunkstore-fs`, `tempfile`; `wyrd-core`
  has `wyrd-metadata-redb`, `wyrd-testkit` and `pollster` — so **no `Cargo.toml` change is needed
  for the tests, and none may be made** (see `Falsifiability` 1).
- **Verification posture:** DEFAULT — a flippable regression test, red pre-fix and green post-fix
  at Check, for leg A. Leg B is co-located and runs under `cargo xtask ci` (leg C); it is declared,
  not deferred, and it is corroboration. Nothing in this slice is deferred off-Check: every
  backend it touches (redb in-memory, the in-test doubles) is exercised by `cargo xtask ci`.
- **Production reach:** this slice builds the record shape and the staged-publication committer
  **ahead of the session that will drive them**. (a) What honours the seam at Check: leg A's
  raw-seeded segmented object exercises the resolver through the **real, production**
  `reconcile_step`, `reconcile_after_restore`, `reconciliation_status` and `high_water_marks` code
  paths — not a double of them — and leg B drives the real committer against a real redb store; the
  only stand-in is the *caller* that supplies the publication precondition. (b) Where the production
  wiring lands: **#636** passes `require(mpu == Completing@E)` into this committer and mints the
  group nonce in the Create batch; **#508** exposes the verb that reaches it. (c) The double is
  load-bearing, not scaffolding: leg B(iii)–(v) assert the flip is a single fenced batch, that
  segments are durable before it, and that a refusal makes nothing durable — the whole crash story
  of decision 7(c). **No production path publishes a segmented map when this slice merges, and that
  is correct** — 0016 forbids a single `PutObject` from segmenting (it has no session, no epoch, and
  so no anchor for the staged-publication protocol or for the reaper that reclaims a crashed one,
  `0016:2287-2299`).
- **Citations expected:** Do must cite `path:line` on the target branch for every change. **Peer
  callsites Do SHOULD open and mirror** (a deliberate, narrow exception to reading `brief.md`
  only):
  * `crates/custodian/tests/gc.rs:73-80` — **the `scan_page` body every new store double copies**
    (`wyrd_testkit::test_double_scan_page`, one delegating line). This is the omission that sank
    iteration 5; mirror it in every double this slice adds, in `core`, `custodian` and `server`.
  * `crates/core/src/metadata.rs:262-300` — `InodeRecord`, and in particular the
    `skip_serializing_if` comment at `:277-289` explaining *why* decode→encode identity is
    load-bearing for every CAS. The `Flat | Segmented` encoding must satisfy the same rule.
  * `crates/core/src/metadata.rs:540-660` — `commit_chunk_map` / `commit_chunk_map_superseding`
    and their `require(key, encode(prior))` CAS shape; the segmented flip is the same shape with
    an extra caller-supplied precondition.
  * `crates/custodian/src/gc.rs:251-297` (`referenced_fragments`) — the reference build every
    consumer's protection derives from, and the strict `metadata::decode(&value)?` at `:256`;
    with `:159-170`, the **fail-safe protection** rule (a fragment of an untrustworthy placement
    is off-limits) that the containment table below extends to an unresolvable map.
  * `crates/custodian/src/desired_state.rs:166-179` — `PendingMalformed`: the repo's existing
    "refuse to certify, attribute, keep going" containment shape.
  * `crates/custodian/src/restore.rs:373-385` and `:104-145`, `:179` — restore's identical inode
    walk and the `RestoreReport::stranded_marked` counter leg A(iii) asserts on.
  * `crates/custodian/src/{rebalance.rs:145-160, reconstruction.rs:599-620, backfill.rs:76-130}`
    — the remaining `.chunk_map` walks. **`backfill.rs` is a consumer the issue body does not
    list and it must not be missed** (see `Open questions` 3).
  * `crates/server/src/lib.rs:360-380` and `:438-460` — the gateway read path's whole-object and
    ranged walks; `:120-125` — `Gateway::recover`, the `high_water_marks` caller that runs before
    serving and whose blast radius leg A(vii)(a) pins.
  * `crates/core/src/metadata.rs:847-880` — `high_water_marks` as it stands: it already derives
    `max_chunk` from `pending:` and `orphan:` **keys** without decoding a record, which is the
    shape the containment rule asks it to extend to `seg:`.
  * `crates/custodian/tests/gc.rs:26-120` — the in-memory `MemMeta`/`MemDServer` harness shape
    leg A's test file should follow.
- **Prior-art check (triage cycles):** re-run on 2026-07-27 against `origin/main` @ `9120f7a`, by
  affected file path, across merged history and closed/rejected work.
  `git log -S"Segmented" --all -- crates/core/src/metadata.rs` returns **nothing** — the record
  shape has never been implemented; `git log -S"seggrp" --all` matches only the two docs commits
  that landed proposal 0016 (`c35d39d`, `cca16fe`). `crates/core/src/metadata.rs`'s last shape
  change is ADR-0047's object-metadata model (`68403eb`, MERGED), whose `skip_serializing_if`
  lesson this slice must not undo. **No open PRs on the repo** (#645 merged; the tree is clean).
  The one rejected prior art is in this harness: #508's **4th** attempt shipped a resolver used only
  by the read path while `gc.rs`/`restore.rs` still iterated `record.chunk_map` directly (restore
  stranded, a later GC deleted a live segmented object's fragments), and the **7th** shipped the
  whole thing inside a 44-file / 14,117-line cross-plane patch rejected at sign-off on
  reviewability (`results/issue_508/iteration-v4/`, `iteration-v7/`,
  `results/issue_508/review-rejected.md`). Leg A exists to make the 4th attempt's failure
  mechanically impossible to repeat.
- **Disposition hint:** likely-fix

## Motivation

A published chunk map is **one JSON value**, and a value has a ceiling: FoundationDB's 100 KB is
the tightest in play and is therefore the de-facto limit for every backend
(`crates/traits/src/lib.rs:997`). Working the arithmetic (`0016:218-232`): a `ChunkRef`
encodes to ~131 B–~302 B, so at 2× headroom a flat map holds only **165–381 chunks** — about
**165–381 MiB** of object at the 1 MiB default chunk size. The
launch requirement is objects **over 10 GiB**. A design that stops at a flat map cannot deliver
the feature's own promise, whatever the multipart protocol above it does.

**One correction to the issue body, which Do must not inherit.** #635's *Why* says "the flat map
is the binding ceiling for ordinary PUT as well as for multipart". That was true before 0016
decision 7 settled the question, and it is **no longer the plan**: segmentation is
**multipart-only** (`0016:2287-2299`), because staged publication needs the three things only a
session provides — an upload id, a fenced `Completing@E` state to write segments under, and the
epoch that scopes the segment keys. A single `PutObject` has none of them, so it stays **flat** and
reaches large sizes by *chunk-size selection*
(`chunk_size_effective = max(DEFAULT_CHUNK_SIZE, ⌈Content-Length / MAX_MAP_CHUNKS⌉)`), or is
refused `400 EntityTooLarge` past `chunk_size_max`. **That `PutObject` behaviour is #508's slice,
not this one.** Consequently this slice's success criterion is *not* "a large object round-trips"
— no production path can produce a segmented map until #636 lands the session — it is the record
shape, the resolver, and the per-consumer resolution contract, which is what legs A and B assert.

## Design

### The settled record encoding (pinned here, so the test oracle is implementation-independent)

`InodeRecord.chunk_map` is one JSON value with two shapes, discriminated by **JSON type** so a
legacy record decodes and re-encodes byte-identically:

* **Flat** — a JSON **array** of `ChunkRef` objects. Byte-identical to today, in both directions.
* **Segmented** — a JSON **object**, exactly:

  ```json
  {"group":{"nonce":"<32 lowercase hex chars>","epoch":<u64>},
   "segment_count":<u32>,
   "segments":[{"index":<u32>,"byte_offset":<u64>,"byte_len":<u64>}]}
  ```

  `segments` is ordered by ascending `index`; `segment_count == segments.len()`; `byte_offset` is
  the segment's first byte within the object and `byte_len` its length, so the root alone answers
  "which segment covers byte N" without reading any segment record.

The **segment record** `seg:<nonce>:<epoch>:<index>` holds that segment's chunks
(`0016:354`): a JSON object `{"chunks":[<ChunkRef>…],"byte_offset":<u64>,"byte_len":<u64>}`.
`<nonce>` is the 32-hex group nonce, `<epoch>` is decimal, and `<index>` is **fixed-width
zero-padded decimal (6 digits)** so the key's byte-lexicographic order equals index order — the
range read reassembles in index order without relying on `scan`, which leaves order unspecified
(`crates/traits/src/lib.rs:1021-1023`); the padding is key hygiene, and the resolver still orders
by the parsed index (leg B(vi)).

The **group reservation** `seggrp:<nonce>` is a marker whose *presence* is the whole meaning;
its value is the empty JSON object `{}`. Do not add fields to it: its lifetime rule (below) is
tested by presence/absence, and a later slice that wanted to CAS on its exact bytes would be
broken by a value that changed shape.

Do MAY choose any Rust spelling that produces exactly these bytes. If Do believes a different
encoding is better, that is an `Open questions` item for sign-off — **not** a unilateral change,
because leg A's fixture is hand-written from this spelling.

### The one editorial contradiction in 0016, and how it resolves

0016 decision 7(a) still describes the segment group as "the minting upload-id paired with the
fence epoch" (`0016:2318`), while §1 records the **later, corrective** rule (iteration-14 finding
2, `0016:502-527`): segment groups **must not** be keyed by the upload id, because a `Completed`
session's `mpu:` record is deleted at the end of its tombstone window while its `seg:` records
must survive as long as the object they map — so the one stated guard,
`require_absent(mpu:<id>)`, is already gone by the time a colliding id would matter, and the
collision would **overwrite a live object's segment records**. **Implement the corrective rule:**
an independent, session-minted 128-bit nonce, reserved by `require_absent(seggrp:<nonce>)` plus
the marker record. The editorial fix to 0016's summary is tracked with **#628** and is *not* this
slice's to make — do **not** edit `0016` or any file under `docs/design/adr/` or
`docs/design/specs/` (ADR immutability, ADR-0001; architecture-board authority, INTEGRATION §4).

The marker's lifetime is the two-arm rule at `0016:513-527`, and both arms belong to the session
slices (#636), not here. This slice ships the record, the key helper, and the **predicate** each
arm needs — "does any `seg:` record name this nonce?", answered by one bounded range read — so
#636 can gate its terminal delete on it exactly as it gates on the `sidx:` range being empty.

### The fence is a parameter, not a session dependency

Staged publication is `write the segments → flip the root`, and the segment-write batches carry
`require(mpu == Completing@E)` (`0016:2331-2337`). The `mpu:` record type is **#636's**. So the
committer this slice ships is parameterised over the caller's **preconditions**.
**But a precondition parameter alone is NOT sufficient for the flip.** The root flip is normatively
**one batch** that also carries the caller's *mutations* — `session → Completed`,
`retire:records:{parts}` and any `retire:bytes:` (`0016:2338-2345`, inventory row `:654-663`) — and
those mutations are #636's. A committer that accepted only preconditions would force #636 to commit
them in a **second** batch, which breaks the atomicity the publication instant depends on: a crash
between the two either publishes an object whose parts are never retired, or retires parts for a
publication that never landed.
**So the committer's API MUST let the caller contribute BOTH preconditions and additional
mutations to the exact flip batch**, and leg B(iii) must assert it: inject a caller mutation and a
caller precondition, and assert the root CAS and the injected mutation commit **together** or not
at all (drive the precondition false and assert neither landed). The caller's contribution may not
write the publication's own records (the inode root, this group's `seg:` range, the `seggrp:`
marker) — refuse a collision rather than merge it. If Do finds that API cannot be
made clean, the honest alternative is to **move the root flip into #636** and leave this slice the
segment encoding, the `seg:`/`seggrp:` records and the resolver — say which was chosen in
`build-notes.md`. Either is acceptable; a two-batch flip is not.

Leg B(iii) must therefore exercise the committer with a **real, non-trivial** precondition (a
`require` on a seeded key the test controls) and assert that a failing precondition leaves the
root untouched and the already-written segments in place — the crash/rollback shape of decision
7(c), minus the session. Leg B(iv) adds the ordering rule that iteration 5 got backwards: a
refusal the committer can decide with **zero I/O** must be decided **before** anything is durable.

### Every consumer, through one resolver

The resolver takes a store and a decoded `InodeRecord` (plus the root's identity, per leg B(viii))
and returns the object's ordered chunk list, reading the bounded `seg:` range only when the map is
segmented. **Every** current `.chunk_map` consumer goes through it:

| Consumer | Site |
|---|---|
| GC reference build (and therefore scrub, which calls it) | `crates/custodian/src/gc.rs:251-297`, `scrub.rs:43`,`:75` |
| Restore | `crates/custodian/src/restore.rs:373-385` |
| Rebalance evacuation | `crates/custodian/src/rebalance.rs:145-160`, `:266-284` |
| Reconstruction (`assess` / `find_chunk`) | `crates/custodian/src/reconstruction.rs:313-330`, `:599-620` |
| **Backfill** | `crates/custodian/src/backfill.rs:93`, `:111-124`, `:169` |
| Gateway read path (whole-object and ranged) | `crates/server/src/lib.rs:360-380`, `:438-460` |
| Core read path | `crates/core/src/read.rs:92` |
| Core write / publication | `crates/core/src/write.rs:274` |

"Every" is the load-bearing word. Leg A's observables (reconcile succeeds, drain says
`Pending`, restore strands nothing) cover GC, scrub, rebalance, reconstruction, restore and
desired-state in one binary; backfill and the read paths need their own assertions — see
`Open questions` 3.

`metadata::high_water_marks` (`crates/core/src/metadata.rs:847`) is a **ninth** `.chunk_map`
reader that is easy to miss because it is not a maintenance loop — it is the allocator floor read
at gateway startup. It is covered by the containment rule below, not by the resolver.

### Failure containment — who fails closed, and how wide

An unresolvable segmented map (a root that still names its group while a `seg:` record is absent)
is an invariant violation, and leg B(viii) requires it to fail closed. **How WIDE that failure
is allowed to reach is a per-consumer decision, and this brief settles it** — because the previous
attempt made it uniform and turned one damaged object into a store-wide outage: a probe showed
`high_water_marks` returning `Err`, which is exactly what `Gateway::recover` calls before serving
(`crates/server/src/lib.rs:123-124`), so the gateway refused to start and every *healthy* object
became unavailable.

| Consumer class | Required containment |
|---|---|
| **Deletion-capable passes** — GC, restore's strand-marking, and anything that can reclaim or move bytes | **Fail closed.** An incomplete reference set may not authorize any reclamation. Aborting the pass is acceptable (it is what `decode(&value)?` already does today for an undecodable value); continuing while treating the damaged object as fully referenced is also acceptable and is the shape this repo already uses (`gc.rs:159-170`, `desired_state.rs:166-179`). **What is NOT acceptable is deleting anything.** |
| **`high_water_marks` / `Gateway::recover`** | **Must be total.** No arrangement of store contents may make it return `Err`, and it must **never under-approximate** the floor. It has no safety need to resolve a root: the chunk ids it wants are in the `seg:` records themselves, and the function already derives `max_chunk` from `pending:` and `orphan:` **keys** without decoding a record (`crates/core/src/metadata.rs:862-880`). Do chooses the spelling; the requirement is totality plus a floor that is a strict over-approximation. |
| **Read paths** (gateway whole-object + ranged, `core::read`) | **Fail closed for the requested object only.** A typed error / `NoSuchKey`-class answer for the damaged object; every other object in the store still reads. Never torn or partial bytes. |
| **`reconciliation_status`** | Never certify `Satisfied` while an object's map cannot be trusted — the `PendingMalformed` shape (refuse to certify, **attribute** the blocker, keep going). |

Leg A(vii) is the test of this table, and it is a genuine assertion-red on the base (a segmented
value fails `decode` in every one of these paths today).

### Keeping the mechanical churn mechanical

Changing the field's type breaks every `InodeRecord { chunk_map: … }` construction site across
~20 test files. That churn is unavoidable and it is not what a reviewer needs to hold in view, so
keep it to **one uniform, one-line-per-site form** (a constructor or conversion the flat case goes
through), and keep the reviewable surface the resolver, its consumers, and the encoding. A slice
whose diff makes the resolver hard to find among the churn is the reviewability failure this
whole re-plan exists to avoid.

## Alternatives considered

* **An additive sidecar field** (`chunk_map: Vec<ChunkRef>` kept, plus
  `segments: Option<SegmentedRef>`) — smaller blast radius, and it would leave every construction
  site untouched. **Rejected:** it makes an illegal state representable (both set, or neither) and
  every consumer must then remember which field wins; 0016 states the two-variant value
  (`0016:2313-2317`), and making illegal states unrepresentable is what stops the
  read-path-only-resolver failure from recurring in a new spelling.
* **A tagged enum** (`{"Flat":[…]}` / `{"Segmented":{…}}`) — trivially unambiguous, but it breaks
  byte-identity on **every existing record**, which breaks every `require(encode(prior))` CAS in
  `metadata.rs`. Non-starter.
* **Segmenting single `PutObject` too** — 0016 iteration 5's shape, explicitly rejected
  (`0016:2287-2299`): a single PUT cannot create or recover the `Completing` epoch the staged
  publication protocol needs, so its segments would have no owner and no reaper.
* **Deferring the resolver's non-read-path consumers to a later slice** — that *is* #508's 4th
  attempt, and it produced silent data loss. Leg A makes the deferral impossible to ship.
* **Uniform fail-closed everywhere, including startup recovery** — iteration 5's shape.
  **Rejected** on blast radius: it trades one damaged object for total gateway unavailability while
  buying no safety, because the allocator floor can be derived without resolving any root. (If the
  maintainer prefers it, say so at sign-off — `Open questions` 5.)

## Impact & compatibility

* **On-disk:** flat records are **unchanged, byte-for-byte** — that is leg B(i), and it is the
  compatibility contract. `seg:`, `seggrp:` are **new prefixes**; nothing reads them today, so
  their introduction is additive. No migration tool, no record rewrite.
* **Source-breaking** for every `.chunk_map` reader and writer in the workspace, by design.
* **Docs currency is a merge requirement, not a follow-up** (`../wyrd/AGENTS.md:154-157`): this
  change adds a **persisted field/record class**, so `docs/design/architecture/06-runtime-view.md`
  and `08-crosscutting-concepts.md` must gain the segmented map and the `seg:`/`seggrp:` record
  classes **in this PR**. Run the prose gates locally (`typos`, the doc renderer — both are
  installed on this host and both are inside `cargo xtask ci` since wyrd#599); a docs edit that
  passes locally only because a tool is missing opens the PR red (INTEGRATION §3).
* **No ADR/spec/proposal edits.** Graduating this shape to an ADR is **#628** (FU-1) and is
  architecture-board authority, not a model's and not this slice's.
* **Out of scope:** #632 (FU-5, part-record segmentation — the same machinery one record class
  down); the multipart records, the commit protocol and the S3 verbs (#636 / #508); GC's adoption
  of `scan_page` and the staged protection class (#637).

## Open questions

1. **The pinned encoding** above is this brief's decision, not 0016's — 0016 names the fields but
   not the JSON. It is exactly the kind of thing #628 will graduate to an ADR. If the maintainer
   wants a different spelling, say so at sign-off **before** Do runs: leg A's fixture is written
   from it.
2. **Where the resolver lives.** `crates/core/src/metadata.rs` is the natural home (it owns the
   record and the key helpers, and `custodian` already depends on `core`). Do MAY place it
   elsewhere in `core`; it must **not** live in `custodian` (the read path needs it) and it must
   not be duplicated.
3. **`backfill.rs` is a `.chunk_map` consumer the issue body omits.** 0016's decision-2 table says
   backfill has "nothing to do" for *staged* records (`0016:876`), but that is about staging, not
   about segmentation — `backfill.rs` iterates and *rewrites* `record.chunk_map` (`:93` iterates, `:111-124` rewrites, `:169`), so a segmented record reaching it must be handled deliberately (resolve, or skip
   with a stated reason — a pre-M3 empty-placement record cannot be segmented, so skipping is
   defensible, but it must be a decision with an assertion, not an oversight). Do must state which
   it chose and assert it. Flagged here because an unlisted consumer is precisely how attempt 4
   failed.
4. **Is a `Completing`-less committer acceptable to the maintainer?** The fence-as-parameter shape
   (above) is what lets this slice land before #636. The alternative is folding staged publication
   into #636 and shipping only the shape + resolver here. The brief takes the first; say so at
   sign-off if you want the second. (This is the recurring T3 "precursor-only seam" NEEDS-HUMAN —
   pre-declared here so it is a cheap confirm rather than a surprise.)
5. **The containment table** (`Design § Failure containment`) — **settled, not open.** Put to the
   maintainer at Plan after iteration 5's probe showed a store-wide startup outage, and
   **confirmed on 2026-07-27**: per-consumer containment in the `PendingMalformed` shape, rather
   than fail-closed across the whole scan. Listed here only so a reviewer sees it was decided
   deliberately and by whom.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Carry-forward — what the next Do must fix (iterations 1–5)

Five attempts are archived in `iteration-v1/` … `iteration-v5/`. **Do not re-attempt the rejected
approach unchanged**, and do not re-derive the settled parts: iteration 5 passed C1 Spec, C3
Change, T1 Structure and T2 Shape, and `cargo xtask ci` + C4-verify were green on the base it
actually used. What follows is everything still open, in priority order.

1. **THE rejection — the base (fixed in this brief; Do must honour it).** Iteration 5 verified
   against plain `origin/main` while the brief demanded a #634 stack, and its `MemMeta`
   (`crates/custodian/tests/segmented_map_consumers.rs:83`) omitted `scan_page`, so the branch
   could not compile on the real stack. #634 is now **merged into `main`**; implement `scan_page`
   on **every** store double this slice adds — `core`, `custodian` and `server` alike — mirroring
   `crates/custodian/tests/gc.rs:73-80`. Iteration 5 left doubles missing it at
   `crates/core/src/metadata.rs:5153`, `crates/custodian/tests/segmented_map_consumers.rs:83` and
   `crates/server/src/lib.rs:882` (line numbers on that patch, for orientation only).
2. **Publish must refuse before it writes** — `SegmentedPublication::publish` committed the whole
   segment phase before it ever *built* the flip batch, so a flip the committer refuses
   deterministically and with zero I/O (`Unfenced`, `ContributionCollides`, `ValueOverCeiling`,
   `KeyOverCeiling`, `BatchOverBudget`, `BatchOverOps`) was discovered only after `seg:` records
   were durable and the caller's cursor had moved — contradicting the patch's own stated rationale.
   Evaluate the flip batch **before** writing segments. Now leg B(iv).
3. **A resumed publication must verify the durable prefix** — `staged_batches` trusted
   `resume_from` against a prefix it never read. A probe resumed at the same `nonce:epoch` with a
   chunk list differing in one length, got **0 batches**, built the root from the *new* list,
   committed `Committed`, and every later resolve failed with `SegmentBoundsMismatch` — silent at
   publication, terminal at read. One `get` closes it. Now leg B(v); a recorded rejection
   ("this is #636's contract") is an acceptable alternative, but it must be **recorded**.
4. **Blast radius** — settled in `Design § Failure containment` and tested by leg A(vii).
5. **The five open `review-batch.md` findings** (round 5, still unchecked, T4-gating). All five say
   the *segmented* repoint adopts a destination without `require_absent(desired:dserver:<target>)`,
   letting a concurrent drain be declared satisfied and wiped. **Plan's finding: this is
   pre-existing behaviour of the path this slice mirrors, not something the slice introduces.** On
   the base the **flat** evacuation CAS carries only `require(inode == prior)` plus the displaced
   fragments' `orphan:` puts and **no destination fence at all**
   (`crates/custodian/src/rebalance.rs:274-296`); reconstruction's repoint is the same shape. So
   this is the same class as the four rows already recorded in `review-rejected.md` round 1, and
   the drain protocol belongs to #636/#637. **Do should record-reject all five** in
   `review-rejected.md`, in the gate's `<file:line> | <CLASS> | <MATCH> | <reason>` format, citing
   the flat peer above — unless Do finds the segmented path materially worse than the flat one, in
   which case fix it and say so. **The maintainer confirmed this disposition at Plan (2026-07-27).**
   Do **not** leave them unchecked: the T4 gate blocks while any
   finding is untriaged, and "0 recorded-rejected" is what failed the last five rounds.
6. **Earlier rounds, already fixed and to be kept** (do not regress): the caller contribution is
   charged against the whole assembled batch and fails closed (`BatchOverBudget`) and is bounded by
   operations too (`BatchOverOps`, `0016:640-648`); the repoint mutator takes a placement vector so
   id/scheme/len are unspellable; every phase batch **and** the flip must carry a value
   precondition on the publication's declared fence record; a caller contribution may not write the
   publication's own records; a *complete* resolve of a superseded generation is retired and
   restarts; `core`'s public snapshot read is routed through the resolver rather than rejecting
   segmented records. Their tests are in `iteration-v5/` for reference.

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — C5 Causal adequacy — The rebuild must authenticate every trusted durable segment, not only `resume_from - 1`: a same-length earlier-ID change committed a mixed old/new map because segment boundaries stayed unchanged (`crates/core/src/metadata.rs:2997`).; T3 Runtime — The maintainer must accept landing a `Completing`-less precursor committer before #636 supplies the real session fence — this determines whether an otherwise unreachable persistence API should ship now (`crates/core/src/metadata.rs:2540`).; T4 Contribution — A human must inspect and triage the reported six batch-review items — the target lacks `scripts/review-branch` and its output is unavailable, so their novelty and validity cannot be independently settled; affected-path prior art was mechanically clear.; T5 Judgment — The rebuild must add allocator-boundary coverage: all three independently rerun survivors replace the `< 2^64` comparator, so the tests do not protect the ID-space boundary whose failure can undercount or misclassify persisted IDs (`crates/core/src/metadata.rs:3473`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 324 mutants tested in 7m: 3 missed, 164 caught, 157 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batch-review gate is failing with 6 unresolved blocking findings (review-batch.md), none fixed or recorded-rejected: (1) metadata.rs:2260 — flat map returned without re-reading root, stale maintenance scan can miss live generation; (2) metadata.rs:2932 — fence-value-only check doesn't ensure the flip transitions the record, post-flip rollback can delete segments the live root still names; (3)+(4) metadata.rs:3031 — resume verification silently drops malformed keys / ignores records outside the plan, so a flip can commit a range that later resolves fail on (SegmentKeyMalformed/SegmentUnknown); (5) metadata.rs:3452 — skipping an unreadable/truncated id token can understate the high-water mark, letting the allocator reissue a live ID; (6) dst/tests/custodian.rs:1811 — staged-publication DST missing the mid-segment-batch apply-then-unknown recovery/idempotency test. Also flagged by the adversarial review and worth addressing in the same pass: the same-epoch shorter-resume-plan orphaned-tail bug (metadata.rs:3010/verify_resume_prefix vs :2129/read_segments), and the high_water_marks "totality" claim being false for the three unpaged scans (inode:/pending:/orphan: hitting SCAN_CAP) — either bound the claim or page those scans like segment_chunk_floor already does. Fix and record-reject (with reason) each of the 6 review-batch findings per the existing triage rule before resubmitting.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batch-review (round 8) gate fail is genuine, not noise: 6 findings map to 4 distinct bugs, none yet triaged (fixed or recorded-rejected) per the bundle's own round-1..7 triage precedent. Required for round 9: 1. metadata.rs:3200 flip() verifies only 0..resume_from — a partial/zero-write flip can publish a root naming un-written segments, causing permanent SegmentAbsent on read. Independently confirmed by the adversary review too — fix the range verified at flip time to the whole plan. 2. metadata.rs:3455 fence-transition check accepts any differing put, so a later duplicate put can restore the pinned fence value and let a racing rollback delete segments after the flip — needs the same "fence must transition" treatment as prior rounds' fence work, or an explicit recorded decline with reason if judged out of scope. 3. metadata.rs:3757 id-floor fallback scanner matches only literal "id" bytes; an escaped-equivalent duplicate key can hide a larger live chunk id from the startup floor — fix the scanner or record-reject with reason. 4. metadata.rs:3834 truncated-prefix-at-2^64-ceiling underestimate (e.g. prefix "18") lets recovery re-mint a still-live chunk id — same class as the already-flagged C5/T5 NEEDS-HUMAN items; fix the boundary case and give the tests an independent oracle so they'd catch the regression. Also carry forward the other §6 human-only items (T3 dormant-API landing-order call, unreachable max_chunk cost, O(N) maintenance-pass round-trip regression, containment-table reconciliation_status row) for explicit resolution/decline in the rebuild, not silent carry.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 gate (gating) fails on re-run against the real patched worktree — reproduced live at sign-off, not stale: 5 blocking findings, 0 recorded-rejected. - crates/custodian/src/gc.rs:309 [BUG] (seen independently by 2 passes, and matches the adversary review's refutation #1) — structural SegmentRecord decode failures surface as serde_json::Error, not ChunkMapError, so the containment downcast misses them and one malformed segment aborts fleet-wide reference-build / drain-status instead of being contained per-object. This is the exact §1(vii) containment property the brief requires; round 8 fixed only the absent-record spelling, not the malformed-record spelling. - crates/core/src/metadata.rs:2170 [BUG] — same containment class, different consumer: the segment scan materializes a corrupt generation's whole range before enforcing the 512-segment bound, so a damaged object can hit SCAN_CAP / exhaust memory and abort fleet-wide maintenance instead of being contained. - crates/core/src/metadata.rs:3924 [BUG] — the fallback quoted-ID lexer misses IDs with enough escaped leading zeros, under-reporting the allocator's chunk-id floor. - crates/core/src/metadata.rs:1329 [CONVENTION] — SegmentRecord decode admits an empty segment (chunks:[], byte_len:0) as valid despite the repo's parse-don't-validate rule treating that as structural corruption. Directive for the rebuild: fix at the foundation, not by whack-a-mole per call site. The gc.rs:309 / metadata.rs:2170 pair share one root cause — decode-time failures are not uniformly surfaced as ChunkMapError — so fix it once at the decode/error-wrapping boundary (e.g. make every SegmentRecord/root structural failure produce ChunkMapError, or have the shared resolver/containment check recognize both) rather than patching each downstream downcast site individually; a third call site with the same pattern must not become a future review round. Then fix the two remaining findings (metadata.rs:3924 escaped-leading-zero lexer gap, metadata.rs:1329 empty-segment validation) on their own merits. None of these are Plan/brief gaps — the brief already specifies the required containment and decode-time-invariant behavior in detail (§1(vii), §1(B)(ii)); these are implementation completeness bugs against that spec. Re-verified live at sign-off by applying patch.diff to a clean worktree at the stated base (9120f7a) and re-running scripts/review-branch --bundle directly: same 5 blocking findings reproduced, confirming the gate result in the bundle is not stale or flaky. §6 NEEDS-HUMAN items are left open (not cleared) — carry forward to the next iteration's Check along with the other unresolved judgment items already listed there (C5 causal adequacy / redundant parsed reader, T3 runtime deployability, T4 contribution-history provisionality, fitness-to-purpose of synthetic fixtures pre-#636) and the adversary review's two other open findings (the seggrp: marker reservation that's never written by any code path, and the unresolved C5 mutants on raw_chunk_id_floor).
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 437 mutants tested in 14m: 11 missed, 232 caught, 192 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 10 — carry-forward (from the previous attempt)
- Sign-off rationale: Two verified implementation bugs from the adversary review must be fixed before the next Check attempt: 1. `crates/core/src/metadata.rs:3224` (`SegmentedPublication::publish` / `plan_with`) accepts a chunk with an empty `placement`, which `crates/custodian/src/backfill.rs:163-167,216-223,313` then treats as "structurally impossible" and turns into a fatal, store-wide, permanently repeating `Err` on every future backfill pass. Refuse an empty placement in `plan_with` (make the "structurally impossible" premise true by construction), or make the backfill skip non-fatal for that pass. 2. `crates/core/src/metadata.rs:2461` (`read_group_range`) trusts the root's own declared `segment_count` as the resolve budget instead of clamping to `MAX_ROOT_SEGMENTS` (documented at `:2491`). Verified: a 97,309-byte root (inside `MAX_VALUE_BYTES`) decodes to `segment_count = 2000`, ~4x the stated ceiling, and is resolved anyway (~200MB materialised per read). Clamp `accounted` at `MAX_ROOT_SEGMENTS` in `read_group_range`, or restate the documented bound to match the code. Also: the T4 batched multi-pass rubric review (3x codex) and the primary Check reviewer leaf both failed this round on transient infra (quota) with no usable output — re-run both for real coverage on the next attempt; do not treat this round's silence as a clean bill. Out of scope for this iteration (recorded separately as an Act candidate, §10): the reconstruction/rebalance containment gap for a damaged chunk-map object — file as a foundation-milestone issue rather than blocking this bundle, per the brief's containment table allowing a byte-moving pass to abort.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 469 mutants tested in 17m: 11 missed, 246 caught, 209 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: only 0/3 passes produced a usable result after one retry — refusing to certify a thinner union. Re-run wh
- Full previous attempt preserved in `iteration-v10/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 11 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 gate genuinely fails (not flaky/transient): re-ran scripts/review-branch --bundle manually (3 fresh codex passes, --out scratch, bundle files untouched) and got 12 blocking findings, 0 recorded-rejected — overlapping the bundle's own review-batch.md 8 findings plus 4 more. This confirms the gate result rather than refuting it. Primary must-fix: the fence/rollback race, reported independently 5 times across both runs (crates/core/src/metadata.rs:3646 x3, :3500, :4126) — a segment-phase fence transition A->B followed by the flip going B->A re-satisfies a stale rollback's precondition and lets it delete already-published live segments. This is a coverage gap in check_fence_never_cycles (segment-phase batches only; needs to also cover the flip), not a new design decision — the "fence must not cycle" rule is already settled from round 11. Fix within the existing staged-publication design; no brief/scope change needed. Second cluster, also fixable in place: decode/error-classification precision bugs (metadata.rs:2043, :1937, gc.rs:305, :1418, :1655) — tighten the existing ChunkMapError containment/classification functions. Re-record the id-floor scan-cost finding (metadata.rs:4921) against review-rejected.md precisely — it is the same question already decided (Deferred: follow-up, base-wide Gateway::recover question, not this slice's) but the MATCH/line drifted enough that the gate re-flags it as new every round. None of the §6 NEEDS-HUMAN items (C5 causal adequacy, T3 runtime precursor cost, T4 contribution triage, T5 mutants, Validation fitness-to-purpose) required re-planning — items 2 and 5 (T3, Validation) are re-affirmations of scope boundaries the brief already set (#635 vs #636 sequencing), confirmed unchanged; the rest are ordinary implementation/hardening follow-ups. So: iterate-do, not iterate-plan. §6 boxes left unticked — human did not explicitly clear any of them this session; carry them forward for the next iteration's sign-off.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 479 mutants tested in 16m: 11 missed, 253 caught, 212 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v11/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 12 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batch review gate re-run manually (authoritative per human), 3 blocking findings, none fixed or triaged in this round's review-rejected.md: - crates/core/src/metadata.rs:3879 [BUG] segment batches overwrite previously verified rows without a CAS — an ambiguous-flip recovery can race a live repoint, restore an already-orphaned stale placement, and lose the root flip after corrupting the live generation. - crates/core/src/metadata.rs:4002 [BUG] the supersede branch permits a segmented prior without requiring a retirement obligation, so callers can silently strand the prior generation's segment records and fragments (segmented retirement is unsupported elsewhere). - crates/core/src/metadata.rs:3965 [CONVENTION] flip_batch() swallows segment_batches()'s Err via unwrap_or_default(), so flip() can publish a root even when phase one is unfenced or fence-cyclic; the in-code rationale (both real entry points already report that error first) may not cover every caller, e.g. a bare recovery flip() — worth re-checking, not just re-asserting the existing comment. Do should fix findings 1 and 2 (or produce a reasoned decline in review-rejected.md the way prior rounds did), and either fix or explicitly decide finding 3. These sit in the core crash-safety property (0016 decision 7 staged publication) this slice exists to implement, so they should be resolved with code/tests, not adjudicated at sign-off.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 494 mutants tested in 17m: 12 missed, 257 caught, 222 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v12/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 13 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batched rubric review (gating) fails genuinely — 4 blocking findings independently re-verified against the patched worktree (applied patch.diff to a scratch clone of main @ 9120f7a), all still present: 1. crates/core/src/metadata.rs:3273 (repoint_chunk, flat root arm) — `version: prior_root.version + 1` has no overflow guard, and this arm never calls check_record_ceilings, so a repair can silently grow a record past the 100 KB ceiling. 2. crates/core/src/metadata.rs:2409 (overwrite/repoint path) — `version: prior.version + 1` is computed before the segmented-prior refusal check runs, so a segmented inode at u64::MAX panics on overflow instead of returning SegmentedRetirementUnsupported. 3. crates/core/src/metadata.rs:5366 (segment_chunk_floor) — an unreadable segment record with no recoverable ID digits only emits telemetry and leaves max_chunk unchanged, risking re-mint of a still-live chunk id at startup. 4. Same overflow class as #2, distinct call site named in review-batch.md at metadata.rs:2426. None of these are Plan/brief gaps — the brief already specifies overflow-safe versioning, ceiling enforcement, and floor totality; these are implementation completeness bugs against already-settled spec, consistent with every prior iteration's carry-forward pattern. Fix in place; no scope/design change needed. Separately (not blocking, re-affirmed not reopened): the advisory adversary review's "one damaged object stalls fleet-wide GC/drain" finding is the exact behavior the brief's containment table pre-authorizes for deletion-capable passes (confirmed at Plan, 2026-07-27) — carry it forward as a re-affirmation item for the next sign-off, not a design question to redecide. §6 items left unticked this round — carry forward unchanged to the next Check attempt's sign-off.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 504 mutants tested in 18m: 13 missed, 264 caught, 224 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v13/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 14 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batch-review gate is failing: 3 new blocking findings from the latest review-branch pass are not yet fixed or recorded-rejected in review-rejected.md: - crates/custodian/src/backfill.rs:109 — a damaged segmented inode's ChunkMapError aborts the entire backfill pass instead of being contained per-object, blocking healthy flat inodes too. - crates/custodian/src/rebalance.rs:168 — same shape: one unresolvable inode aborts evacuation planning for every object, not just the damaged one. - crates/custodian/src/reconstruction.rs:615 — one damaged segmented inode aborts the full-store lookup for a queued repair, starving healthy under-replicated chunks that sort after it. Rebuild should apply the same containment pattern already used elsewhere in this bundle (e.g. gc.rs's referenced_fragments / ReferenceSet::unresolvable handling) to these three call sites: contain the per-object ChunkMapError, attribute it, and continue the pass for unaffected objects — rather than propagating it and aborting the whole pass. Each fix (or a reasoned decline per the triage rule, appended to review-rejected.md) must land before the T4 gate can pass. The other §6 NEEDS-HUMAN items (C5 causal adequacy, T3 runtime cost, T5 judgment test-gap, validation fitness-to-purpose) were not cleared this session and remain open for the next sign-off pass.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 508 mutants tested in 28m: 12 missed, 264 caught, 228 unviable, 4 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v14/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 15 — carry-forward (from the previous attempt)
- Sign-off rationale: T4 batch review is red with 12 blocking findings, which dedupe to FIVE sites of ONE defect: the decode-error arm classifies a malformed inode as unresolvable BEFORE consulting `inode_state_hint`, so an uncommitted (Pending) inode is counted as a committed blocker. GC already does it in the right order (`referenced_fragments` / `ReferenceSet::unresolvable`) — apply that same ordering at: (1) `crates/custodian/src/backfill.rs:108-110` main arm — a malformed Pending inode makes every backfill pass return `UnresolvableChunkMaps`; (2) `backfill.rs:303` telemetry rescan — the same inode permanently inflates `backfill_unreadable_records`; (3) `rebalance.rs:172-174` — false rebalance blocker metric and audit alert; (4) `reconstruction.rs:649-651` — sets the store-wide `unresolvable` flag, so absent repair targets stay queued instead of draining; (5) `restore.rs:408-410` — an otherwise clean restore falsely fails `is_clean()`. Each fix, or a reasoned decline per the T4 triage rule appended to review-rejected.md, must land before the gate can pass. NOTE this is a NEW class, not a repeat of v14: v14's three findings (a ChunkMapError aborting the whole pass) WERE fixed — the containment that fix added simply sits on the wrong side of the state check. DO NOT chase the C5-mutants row: it reads `fail — Error: interrupted`, but it never tested a single mutant. It hung for 19h16m inside cargo-mutants' BASELINE `cargo test`, on a pre-existing deadlock in the target's own suite (getwyrd/wyrd#646 — `CustodianService::new` registers a process-global tracing dispatch, so parallel tests deadlock ~1.5% of runs). That row says nothing about this patch; it is unrelated to the segmented chunk map and predates this bundle. Round 15's reviewer and adversary leaves NEVER RAN — an engine bug (wyrd-pdca#187 / eduralph/pdca-harness#369) let an interrupt between the gate write and the leaves put the bundle in CHECKED with no review, and §6's "no check-review.md was produced" item is that hole, NOT a reviewer verdict. So round 15 has no C1-T5 judgment rows at all, and the four §6 items carried from v14 — C5 causal adequacy, T3 runtime cost, T5 judgment test-gap, validation fitness-to-purpose — remain open and unadjudicated; they still need a sign-off pass. The auto-iterate budget is spent (count 5 of max_auto_iters 5), so round 16 is human-driven: start it deliberately rather than expecting the loop to pick it up.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — Error: interrupted
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v15/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 16 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on: (1) C5 surviving mutants — 14 missed mutants on the bundle diff (crates/core/src/metadata.rs:5192, :5198 and others per check-review.md) are not yet killed/covered by tests; (2) T4 batched review — 3 fresh blocking findings in review-batch.md are unresolved (not fixed, not recorded-rejected): - crates/custodian/src/gc.rs:335 — unresolvable inode in a partial ReferenceSet lets scrub::reconcile skip its fragments and wrongly report Satisfied. - crates/core/src/metadata.rs:5162 — a structurally invalid seg: value (e.g. {}) contributes zero to high_water_marks instead of the conservative ceiling, under-reporting live chunk IDs. - crates/core/src/metadata.rs:2874 — segment decode/bounds failures bypass the root re-read, so a request racing an overwrite can fail on corruption in a retired generation instead of resolving the healthy replacement. Next attempt: fix (or explicitly record-reject with reason in review-rejected.md) each of the 3 T4 findings above, and add/adjust tests to kill the 14 surviving mutants reported by C5, then re-run the full gate stack.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 537 mutants tested in 19m: 14 missed, 271 caught, 248 unviable, 4 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v16/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
