# Design proposal — issue 635 / segmented-chunk-map

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on
> `origin/main` @ `22d71b4`. **Decision 7 (`0016:2280-2496`) IS this slice's design**, together
> with the `seg:` / `seggrp:` rows of the §1 record table (`0016:354`, `:502-527`) and the
> object-ceiling arithmetic (`0016:218-232`). **Do MUST read decision 7 in full before writing
> code** — especially §(a) the record shape, §(b) staged publication, §(c) the epoch-scoping
> crash story, and §(e) bounded resolution by every maintenance consumer. This brief does not
> restate it; it scopes it, **settles the record encoding** (so the test oracle is independent of
> the implementation's choice), resolves the one editorial contradiction 0016 still carries, and
> states the C4 shape.
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-26.
> This is **seam (ii) of five** in #508's re-plan (634 → 635 → 636 → 637 → 508).

- **Slug:** segmented-chunk-map
- **Kind:** enhancement (design proposal)
- **Goal:** `InodeRecord.chunk_map` graduates from a flat list to **`Flat | Segmented`**, so a
  published map larger than one backend value can exist at all — the >10 GiB launch requirement.
  Flat stays exactly as it is (`crates/core/src/metadata.rs:268`, `pub chunk_map: Vec<ChunkRef>`)
  and every existing record keeps decoding **byte-identically**; segmented carries a group
  identity plus `seg:<group-nonce>:<epoch>:<index>` segment records and their `seggrp:`
  reservation, is published by **staged publication** (write the segments, then flip the root),
  and is resolved through **one shared resolver that every `.chunk_map` consumer goes through**.
- **Success criterion:** two **NEW** test files. The first is the binding one and it is
  deliberately written to compile on this bundle's base, so its RED is an assertion, not a build
  error.
  **(A) BINDING — every maintenance consumer resolves the segmented shape, and the proof is a
  positive observable, not an absence.** `crates/custodian/tests/segmented_map_consumers.rs`
  seeds a **committed segmented object by raw record bytes** through `MetadataStore::commit`
  (the encoding is settled below, so no symbol this slice adds is named) plus its fragments on
  in-memory D-server doubles, then asserts, in one test binary:
  (i) **`reconcile_step` succeeds** with GC + scrub + reconstruction + rebalance contexts all
  supplied (`crates/custodian/src/reconciliation.rs:65-73`) — on the base it returns `Err`,
  because `referenced_fragments` decodes every `inode:` value with `metadata::decode(&value)?`
  (`crates/custodian/src/gc.rs:255-256`) and a segmented value is not a JSON array;
  (ii) **the segmented object's fragments are in the protected set — asserted positively.**
  `desired_state::reconciliation_status(meta, S)` for a server `S` that holds one of the
  segmented object's fragments, with `desired:dserver:S` seeded, MUST answer **`Pending`**
  (`crates/custodian/src/desired_state.rs:150-164`). A resolver that decodes the new shape but
  never reads the `seg:` range answers `Satisfied` — this is the leg that catches it, and
  nothing else does;
  (iii) **restore does not strand it.** `reconcile_after_restore` reports
  `RestoreReport::stranded_marked == 0` (`crates/custodian/src/restore.rs:104-145`,
  `:179`). This is the #508-attempt-4 failure mode in its exact shape: a resolver used only by
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
  `.chunk_map` consumer, `crates/server/src/lib.rs:440-460`); **`core`'s read path** resolves it
  (`crates/core/src/read.rs:92`); **reconstruction** resolves a segmented chunk for an
  **explicitly enqueued** repair instead of dropping it; and **backfill** takes its stated decision
  (resolve, or skip with a reason) rather than mangling the map (`crates/custodian/src/backfill.rs:76-130`).
  **(B) The record shape, the CAS identity, and staged publication.** These name types this slice
  ADDS, so they **must NOT ship as a second added `tests/*.rs` file**: `run-verify.sh` collects
  every added test target into **one** cargo invocation (`engine/scripts/run-verify.sh:286-305`,
  `:332-341`) and keeps them all on the RED leg (`:404-415`), so a compile-red file would fail the
  whole invocation and **destroy leg A's assertion red** — the single most valuable thing this
  slice has. Ship leg B as **co-located `#[cfg(test)]` unit tests inside the production modules**
  they exercise (`crates/core/src/metadata.rs`, and the committer's own module), which
  `cargo xtask ci` runs and C4-verify never retains. Over `RedbMetadataStore::in_memory()`:
  (i) **Legacy decode→encode is the identity, byte-for-byte.** Take the *exact* stored bytes of a
  pre-existing flat `InodeRecord` (including one with `etag`/`content_type`/`modified` absent),
  decode and re-encode, assert equality. This is not hygiene: every CAS in
  `crates/core/src/metadata.rs` is `require(key, encode(prior))` compared byte-for-byte against
  the stored value (the `skip_serializing_if` rationale at `crates/core/src/metadata.rs:275-289`),
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
  (the segment-write batches at `≤ E_tx/2`, `0016:2331-2337`; the flip's own inventory bound `≤ 4·V + O(1)`, `0016:654-663`) and no single value exceeds the 100 KB ceiling — measure the
  encoded bytes, do not assert a record count.
  (iv) **The resolver is total, bounded, and orders segments ITSELF.** It reads the root plus the
  bounded range `scan("seg:<nonce>:<epoch>:")` and nothing else — never a global `seg:` scan
  (`0016:2463-2469`). Assert by seeding a *second* group's segments in the same store and checking
  they are neither read nor returned. **And it must not rely on scan order:**
  `MetadataStore::scan` says "Order is unspecified" (`crates/traits/src/lib.rs:770-775`) and #634
  makes byte-lexicographic order normative **only for `scan_page`**, leaving `scan` untouched — so
  the fixed-width zero-padded index is a *debuggability and key-hygiene* property, **not** a licence
  to concatenate in returned order. The resolver parses each segment's `index` and orders by it
  explicitly, rejecting a gap or a duplicate. Assert with a **deliberately shuffling** store double
  that returns the range reversed: resolution must still yield the correct byte order. (This is the
  alternative to consuming `scan_page`, which would make #634 a real dependency rather than a
  file-conflict.)
  (v) **A rolled-back attempt's segments are disjoint from a later attempt's** — seed
  `seg:<nonce>:1:*` and `seg:<nonce>:2:*` and assert resolving the root at epoch 2 returns only
  epoch 2's chunks (the F18 epoch-scoping property, `0016:2352-2380`).
  (vi) **Decision 7(h)'s resolve-retry rule, which the resolver's SIGNATURE must be able to
  express** (`0016:2452-2474`). A generation's `seg:` records are deleted by retirement and
  rollback, so a consumer midway through a segmented resolve can see a segment **absent**. The
  rule is: re-read the **root**; a root now naming a **different group** or **absent** means the
  generation was concurrently retired (a reader restarts against the current root or answers
  `NoSuchKey`; a maintenance pass drops the stale resolution); a root **unchanged** with a segment
  **absent** is an **invariant violation and MUST fail closed** — an error, never a torn success
  (the *Absent or unsupported entries* rule, `../wyrd/AGENTS.md:174-177`). **A resolver that takes
  only a store and an already-decoded `InodeRecord` cannot do this** — it has no way to re-read the
  root. So the API must carry the root's identity (the inode key/id, or a re-read closure) and
  return a retry-or-fail outcome. Assert both arms: changed root → restart/drop; unchanged root
  with a missing segment → typed error and **no partial map**. The interleaving itself (X51) goes
  into the existing `crates/dst/tests/custodian.rs`, never a new DST file (see `Test file`).
  **(C) `cargo xtask ci` green**, including the docs gates — see `Impact & compatibility` for the
  architecture-doc currency requirement, which is a **merge requirement**
  (`../wyrd/AGENTS.md:154-157`), not a follow-up.
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — no container,
  no cluster, no deploy stack. **Leg A carries the binding red and it is a genuine assertion
  failure**: on the base a segmented-shaped `inode:` value makes `metadata::decode` fail and the
  `?` at `crates/custodian/src/gc.rs:256` propagates out of `gc::reconcile` through
  `reconcile_step` (`crates/custodian/src/reconciliation.rs:78-85`), so leg A(i) fails with
  `Err`, not with a compile error. The same is true for restore (`restore.rs:375-380`), rebalance
  (`rebalance.rs:147`), reconstruction (`reconstruction.rs:607`) and backfill (`backfill.rs:79`,
  `:163`) — every consumer decodes with the same strict `?`.
  **Corollary the test MUST obey:** `crates/custodian/tests/segmented_map_consumers.rs` may
  reference **only symbols present on this bundle's base** — that is `origin/main` **plus #634**
  (this is a wave-1 bundle; see `Ordering note`). So the in-test `MemMeta` double it defines must
  implement `scan_page` (#634's required method, one delegating line), and the test must **not**
  name `ChunkMap`, `SegmentRef`, `seg_key`, the resolver, or anything else this slice adds.
  Everything it needs is base-visible: `WriteBatch`, `MetadataStore`, `reconcile_step`,
  `GcContext`/`ScrubContext`/`ReconstructionContext`/`RebalanceContext`,
  `reconcile_after_restore`, `RestoreReport`, `reconciliation_status`, `set_lifecycle`,
  `mark_orphaned` (all re-exported at `crates/custodian/src/lib.rs:33-45`), and raw record bytes.
  Leg B **is** compile-red (it names the new types); that is expected and is corroboration, not
  the binding evidence.
  **Base resolution is a gate-evaluability precondition.** This bundle is **wave 1**, so the
  driver stamps its stack base and `pdca gates` exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`,
  `src/pdca_harness/gates.py:352-360`), which `run-verify.sh` honours ahead of the brief's base
  (`engine/scripts/run-verify.sh:186-206`). If that export is missing, C4-verify resets to
  `origin/main`, #634's `scan_page` does not exist, the added test's `MemMeta` fails to compile,
  and the RED leg's failure branch — which has **no zero-test guard on the non-zero path** (the
  `TESTS_RAN == 0` check at `:416-427` sits inside the cargo-*succeeded* branch; a build failure
  skips it and falls through to the unconditional `PASS` at `:433`) — prints "PASS — red without
  the fix" over a build that ran nothing. **Do MUST record, from the RED leg, how many tests
  actually ran and failed**, and state whether the red was assertions or a build error. Leg A's
  red must be assertions; if Do sees a build error there, something is wrong with the base, not
  with the test.
- **Invariant to restore:** **an object's chunk map is resolvable, in bounded work, by every
  process that is entitled to act on the object — and no representation of it may be
  understood by one consumer and opaque to another.** Stated over the category: the map is the
  authoritative statement of which bytes a live object owns, so any consumer that cannot resolve
  it either fails safe (halting maintenance) or, worse, concludes the bytes are unowned. Both
  are outcomes 0016 forbids. **Source:** 0016 decision 7(a) — "because this changes the shape of
  `InodeRecord` for **every** consumer of `.chunk_map` … it is the strongest ADR-graduation
  candidate here" (`0016:2313-2330`) — and 7(e), which makes bounded resolution by *each* named
  consumer normative (`0016:2463-2469`); resting on the custodian's written safety rule that a
  referenced fragment is never reclaimed (`0005:294-295`, enforced at
  `crates/custodian/src/gc.rs:159-170`). SELF-TEST: this cannot be satisfied by guarding one
  module — a resolver wired into the read path alone satisfies every read test and still lets
  restore strand and GC delete a live object, which is exactly the recorded #508-attempt-4
  failure.
- **Scope:** the `Flat | Segmented` record shape and its settled encoding, the `seg:` /
  `seggrp:` records and their key helpers (`crates/core/src/metadata.rs`); the staged
  segment-write + root-flip committer, with the publication precondition taken as a **parameter**;
  the one shared resolver and **every** `.chunk_map` consumer routed through it (the eight sites
  tabled in `Design`); the architecture-doc currency edit; and the two new test files. **Out of
  scope:** the multipart session/records/protocol (#636), the S3 verbs (#508), the staged-byte
  protection class (#637), `PutObject` chunk-size selection (#508 — a single PUT never segments),
  FU-1's record-shape ADR (#628), FU-5's part-record segmentation (#632), and any file under
  `docs/design/adr/` or `docs/design/specs/`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 634
- **Ordering note:** **wave 1 of the five-slice stack 634 → 635 → 636 → 637 → 508.** There is no
  *dependency* edge to #634 — segment resolution is a deliberately **bounded** range read
  (`scan("seg:<nonce>:<epoch>:")` ≤ `MAX_ROOT_SEGMENTS`, `0016:2463-2469`), so this slice does not
  consume `scan_page`. The edge is a **file conflict**: #634's required trait method touches
  every in-test `MetadataStore` double, and this slice's `Flat | Segmented` change touches the
  `InodeRecord` construction sites in many of the same files
  (`crates/custodian/tests/*.rs`, `crates/core/tests/*.rs`, `crates/server/tests/*.rs`,
  `crates/chunkstore-grpc/tests/*.rs`, `crates/dst/tests/*`). Built blind on one base they
  collide at the fold. The harness orients the pair by name order
  (`src/pdca_harness/waves.py:167-175`, name-lower first) → 634 in wave 0, 635 in wave 1, which
  is the order this stack wants: landing on 634 means every double this slice touches already has
  its `scan_page` line.
  **Not a dependency, but a sequencing fact Do must know:** the *only* producer of a segmented map
  is a multipart Complete (`0016:2287-2312` — segmentation is multipart-only; a single
  `PutObject` stays flat by chunk-size selection). So this slice ships the shape, the records, the
  resolver and the staged-publication committer, and **#636 wires the session fence into it**. See
  `Design § the fence is a parameter`.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `typos`, `docs-renderer`
  <br>*(Both on the field's own line — the driver reads only that line,
  `src/pdca_harness/brief.py:182-190`. Needed because this slice's docs-currency edit to
  `docs/design/architecture/{06,08}` is gated by the prose gates, which `cargo xtask ci`
  **warn-skips** when the tools are absent, so a locally-green docs change opens the PR red on
  the host's always-on jobs, INTEGRATION §3. Both are installed on this host.)*
- **Test file:** `crates/custodian/tests/segmented_map_consumers.rs`
  <br>*(ONE added test file, deliberately — see leg B. The driver parses only this label's own
  line, `src/pdca_harness/brief.py:23-31`.)* Leg B ships as **co-located `#[cfg(test)]` unit tests
  in the production modules** and leg B(vi)'s X51 interleaving is **appended to the existing**
  `crates/dst/tests/custodian.rs` — neither is an added test file, so neither can join
  C4-verify's invocation and break leg A's assertion red.
  The one added file must be under a `tests/` directory: `run-verify.sh` classifies on an **added**
  `*/tests/*.rs` (`engine/scripts/run-verify.sh:92-94`, `:300-311`), so a case appended to an
  existing suite would degrade the gate to green-only and prove nothing per-fix. Both crates
  already carry the dev-dependencies these need — `wyrd-custodian` has `async-trait`, `bytes`,
  `tokio`, `wyrd-coordination-mem`, `wyrd-chunkstore-fs`; `wyrd-core` has `wyrd-metadata-redb`
  and `pollster` — so **no `Cargo.toml` change is needed for the tests, and none may be made**: a
  modified `Cargo.toml` is reverted on the RED leg, which would turn leg A's assertion-red into a
  build error.
- **Verification posture:** DEFAULT — a flippable regression test, red pre-fix and green post-fix
  at Check, for leg A. Leg B is compile-red (it names types this slice adds); that is declared,
  not deferred, and it is corroboration. Nothing in this slice is deferred off-Check: every
  backend it touches (redb in-memory, the in-test doubles) is exercised by `cargo xtask ci`.
- **Production reach:** this slice builds the record shape and the staged-publication committer
  **ahead of the session that will drive them**. (a) What honours the seam at Check: leg A's
  raw-seeded segmented object exercises the resolver through the **real, production**
  `reconcile_step`, `reconcile_after_restore` and `reconciliation_status` code paths — not a
  double of them — and leg B drives the real committer against a real redb store; the only
  stand-in is the *caller* that supplies the publication precondition. (b) Where the production
  wiring lands: **#636** passes `require(mpu == Completing@E)` into this committer and mints the
  group nonce in the Create batch; **#508** exposes the verb that reaches it. (c) The double is
  load-bearing, not scaffolding: leg B(iii) asserts the flip is a single fenced batch and that
  segments are durable before it, which is the whole crash story of decision 7(c). **No
  production path publishes a segmented map when this slice merges, and that is correct** —
  0016 forbids a single `PutObject` from segmenting (it has no session, no epoch, and so no
  anchor for the staged-publication protocol or for the reaper that reclaims a crashed one,
  `0016:2287-2299`).
- **Citations expected:** Do must cite `path:line` on the target branch for every change. **Peer
  callsites Do SHOULD open and mirror** (a deliberate, narrow exception to reading `brief.md`
  only):
  * `crates/core/src/metadata.rs:262-300` — `InodeRecord`, and in particular the
    `skip_serializing_if` comment at `:275-289` explaining *why* decode→encode identity is
    load-bearing for every CAS. The `Flat | Segmented` encoding must satisfy the same rule.
  * `crates/core/src/metadata.rs:540-660` — `commit_chunk_map` / `commit_chunk_map_superseding`
    and their `require(key, encode(prior))` CAS shape; the segmented flip is the same shape with
    an extra caller-supplied precondition.
  * `crates/custodian/src/gc.rs:251-297` (`referenced_fragments`) — the reference build every
    consumer's protection derives from, and the strict `metadata::decode(&value)?` at `:256`.
    This is the first consumer to route through the resolver.
  * `crates/custodian/src/restore.rs:370-385` and `:104-145` — restore's identical inode walk and
    the `RestoreReport::stranded_marked` counter leg A(iii) asserts on.
  * `crates/custodian/src/desired_state.rs:145-170` — `reconciliation_status` and its
    `genuinely_holds` derivation, the positive oracle of leg A(ii).
  * `crates/custodian/src/{rebalance.rs:147-160, reconstruction.rs:601-620, backfill.rs:76-130}`
    — the remaining `.chunk_map` walks. **`backfill.rs` is a consumer the issue body does not
    list and it must not be missed** (see `Open questions` 3).
  * `crates/server/src/lib.rs:356-362` and `:440-460` — the gateway read path's whole-object and
    ranged walks, the two remaining `.chunk_map` consumers.
  * `crates/custodian/tests/gc.rs:26-120` — the in-memory `MemMeta`/`MemDServer` harness shape
    leg A's test file should follow.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  all PRs. `git log -S"Segmented" --all` matches **only** `97e2392` (the proposal draft) — the
  record shape has never been implemented. `crates/core/src/metadata.rs`'s last shape change is
  ADR-0047's object-metadata model (PR #594, `enhancement/503-object-metadata-model`, MERGED),
  whose `skip_serializing_if` lesson this slice must not undo. No open PRs on the repo. The one
  rejected prior art is in this harness: #508's **4th** attempt shipped a resolver used only by
  the read path while `gc.rs`/`restore.rs` still iterated `record.chunk_map` directly (restore
  stranded, a later GC deleted a live segmented object's fragments), and the **7th** shipped the
  whole thing inside a 44-file / 14,117-line cross-plane patch rejected at sign-off on
  reviewability (`results/issue_508/iteration-v4/`, `iteration-v7/`,
  `results/issue_508/review-rejected.md`). Leg A exists to make the 4th attempt's failure
  mechanically impossible to repeat.
- **Disposition hint:** likely-fix

## Motivation

A published chunk map is **one JSON value**, and a value has a ceiling: FoundationDB's 100 KB is
the tightest in play and is therefore the de-facto limit for every backend
(`crates/traits/src/lib.rs:746-752`). Working the arithmetic (`0016:218-232`): a `ChunkRef`
encodes to ~131 B–~302 B, so at 2× headroom a flat map holds only **165–381 chunks** — about
**165–381 MiB** of object at the 1 MiB default chunk size (`crates/server/src/lib.rs:51`). The
launch requirement is objects **over 10 GiB**. A design that stops at a flat map cannot deliver
the feature's own promise, whatever the multipart protocol above it does.

**One correction to the issue body, which Do must not inherit.** #635's *Why* says "the flat map
is the binding ceiling for ordinary PUT as well as for multipart". That was true before 0016
decision 7 settled the question, and it is **no longer the plan**: segmentation is
**multipart-only** (`0016:2287-2312`), because staged publication needs the three things only a
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
range read must reassemble in index order without sorting, and #634's clause (a) makes byte order
the one order every backend agrees on.

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
**But a precondition parameter alone is NOT sufficient for the flip, and this is the correction the
plan review forced.** The root flip is normatively **one batch** that also carries the caller's
*mutations* — `session → Completed`, `retire:records:{parts}` and any `retire:bytes:`
(`0016:2338-2345`, inventory row `:654-663`) — and those mutations are #636's. A committer that
accepted only preconditions would force #636 to commit them in a **second** batch, which breaks the
atomicity the publication instant depends on: a crash between the two either publishes an object
whose parts are never retired, or retires parts for a publication that never landed.
**So the committer's API MUST let the caller contribute BOTH preconditions and additional
mutations to the exact flip batch**, and leg B(iii) must assert it: inject a caller mutation and a
caller precondition, and assert the root CAS and the injected mutation commit **together** or not
at all (drive the precondition false and assert neither landed). If Do finds that API cannot be
made clean, the honest alternative is to **move the root flip into #636** and leave this slice the
segment encoding, the `seg:`/`seggrp:` records and the resolver — say which was chosen in
`build-notes.md`. Either is acceptable; a two-batch flip is not.

Leg B(iii) must therefore exercise the committer with a **real, non-trivial** precondition (a
`require` on a seeded key the test controls) and assert that a failing precondition leaves the
root untouched and the already-written segments in place — the crash/rollback shape of decision
7(c), minus the session.

### Every consumer, through one resolver

The resolver takes a store and a decoded `InodeRecord` and returns the object's ordered chunk
list (or a borrowed view of it), reading the bounded `seg:` range only when the map is segmented.
**Every** current `.chunk_map` consumer goes through it:

| Consumer | Site |
|---|---|
| GC reference build (and therefore scrub, which calls it) | `crates/custodian/src/gc.rs:251-297`, `scrub.rs:43`,`:75` |
| Restore | `crates/custodian/src/restore.rs:370-385` |
| Rebalance evacuation | `crates/custodian/src/rebalance.rs:147-160`, `:266-284` |
| Reconstruction (`assess` / `find_chunk`) | `crates/custodian/src/reconstruction.rs:313-330`, `:601-620` |
| **Backfill** | `crates/custodian/src/backfill.rs:76-130`, `:161-172` |
| Gateway read path (whole-object and ranged) | `crates/server/src/lib.rs:356-362`, `:440-460` |
| Core read path | `crates/core/src/read.rs:92` |
| Core write / publication | `crates/core/src/write.rs:274` |

"Every" is the load-bearing word. Leg A's three observables (reconcile succeeds, drain says
`Pending`, restore strands nothing) cover GC, scrub, rebalance, reconstruction, restore and
desired-state in one binary; backfill and the read paths need their own assertions — see
`Open questions` 3.

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
   about segmentation — `backfill.rs:93` iterates `record.chunk_map` and `:111-124` *rewrites* it,
   so a segmented record reaching it must be handled deliberately (resolve, or skip with a stated
   reason — a pre-M3 empty-placement record cannot be segmented, so skipping is defensible, but
   it must be a decision with an assertion, not an oversight). Do must state which it chose and
   assert it. Flagged here because an unlisted consumer is precisely how attempt 4 failed.
4. **Is a `Completing`-less committer acceptable to the maintainer?** The fence-as-parameter shape
   (above) is what lets this slice land before #636. The alternative is folding staged publication
   into #636 and shipping only the shape + resolver here. The brief takes the first; say so at
   sign-off if you want the second.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Refresh/fold onto #634 and rerun red→green—the stale target passed `cargo xtask ci` and all 5 post-fix tests, but the normative stack currently stops at E0046, so stack-green remains unverified (`crates/custodian/tests/segmented_map_consumers.rs:77`).; C5 Causal adequacy — Strengthen the causal oracles before acceptance: 25/193 survivors were independently reproduced, including removal of flat repoint map/version writes and the segment fence, so the safety mechanisms are not pinned (`crates/core/src/metadata.rs:1775`, `crates/core/src/metadata.rs:1972`).; T4 Contribution — Supply the unavailable `scripts/review-branch --bundle` output and decide its 20 reported blockers—`contribcheck` also lacked the rendered `pdca.toml`, so those red/green rows could not be independently reproduced; affected-path merged and closed/rejected-history searches found no duplicate segmented implementation.; T5 Judgment — Rebuild after adding the stack-base method, overflow-safe decode, and tests that kill the safety-critical survivors; until then the evidence cannot support implementation sign-off (`crates/custodian/tests/segmented_map_consumers.rs:77`, `crates/core/src/metadata.rs:836`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 20 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 193 mutants tested in 4m: 25 missed, 57 caught, 111 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 20 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Triage or reproduce the 10 reported batched-review blockers — `scripts/review-branch` and its finding output are absent, so that red row is provisional; independent affected-path merged history plus all 9 closed-unmerged PRs found no competing segmented implementation.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 246 mutants tested in 6m: 1 missed, 105 caught, 140 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must enforce the 100 KB limit on each caller-supplied flip value: a direct probe was accepted with a 102,401-byte value because `flip_batch` checks only aggregate batch bytes at `crates/core/src/metadata.rs:2494`.; T3 Runtime — Maintainers must accept precursor-only runtime coverage — #636's real `Completing@E` caller was not present or exercised, so publication evidence uses a test-supplied caller contribution at the seam described by `crates/core/src/metadata.rs:2180`.; T4 Contribution — A human must triage the eight reported blockers because `scripts/review-branch --bundle` and its truncated result artifact are unavailable here; affected-path merged-history and closed/rejected-work checks found no competing segmented-map implementation.; T5 Judgment — Rebuild must extend the envelope oracle to adversarial caller contributions — the current test asserts the per-value ceiling only on its small default fixture at `crates/core/src/metadata.rs:3789`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must route the public snapshot read through the resolver or remove it as a segmented-map consumer—the method explicitly rejects segmented records, leaving one read path opaque despite the every-consumer invariant (`crates/core/src/read.rs:72`).; T3 Runtime — Maintainers must accept precursor-only runtime evidence—the real #636 `Completing@E` caller is absent, so publication safety was exercised only with test-supplied segment and flip contributions; this matters because those contributions carry the production fence and atomic session mutations (`crates/core/src/metadata.rs:2379`, `crates/core/src/metadata.rs:2392`).; T4 Contribution — A human must triage the eight reported batch-review blockers because `scripts/review-branch --bundle` and its result artifact are unavailable here; independent affected-path history plus open/closed PR searches found no competing implementation, only the proposal and the earlier object-metadata shape change.; T5 Judgment — Rebuild must restore #634 stack evaluability and close the remaining snapshot-read gap; a main-only green run cannot establish implementation sign-off while the normative stack executes zero binding tests (`crates/custodian/tests/segmented_map_consumers.rs:83`, `crates/core/src/read.rs:72`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Wrong verification base: the brief required verifying against origin/pdca-integration/main (which carries #634's scan_page addition to MetadataStore), but that branch does not exist in this sandbox/checkout, so Do verified red->green against plain origin/main instead. The added test double (MemMeta / segmented_map_consumers.rs:83) is missing scan_page, which the brief told Do to add, and would not compile against the real #634 stack. Both the primary and adversarial reviewers independently flagged this as the root issue (T5 Judgment, C4/C5 NEEDS-HUMAN items, and the stack-base-unverifiable finding). Plan needs to fix the base setup/dependency wiring (make #634's stack actually reachable, e.g. ensure pdca-integration/main exists or otherwise resolve the #634 -> #635 sequencing) before another Do attempt, rather than Do re-guessing at a moving target. Also carry forward for the next Do pass once the base is fixed: (1) SegmentedPublication::publish commits segment writes before validating the flip will succeed (crates/core/src/metadata.rs:2939) - evaluate flip_batch() before write_segments; (2) staged_batches/resume_from trusts the resumed cursor without verifying the durable prefix (crates/core/src/metadata.rs:2698) - add a one-get defence before resuming; (3) high_water_marks turning one damaged segmented object into a store-wide outage (crates/core/src/metadata.rs:3272) needs a human blast-radius decision on whether per-consumer containment (like desired_state's PendingMalformed pattern) is required instead of fail-closed on the whole scan.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
