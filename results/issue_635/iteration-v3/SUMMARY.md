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
  two **NEW** test files. The first is the binding one and it is
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope:
  the `Flat | Segmented` record shape and its settled encoding, the `seg:` /
  `seggrp:` records and their key helpers (`crates/core/src/metadata.rs`); the staged
  segment-write + root-flip committer, with the publication precondition taken as a **parameter**;
  the one shared resolver and **every** `.chunk_map` consumer routed through it (the eight sites
  tabled in `Design`); the architecture-doc currency edit; and the two new test files. **Out of
  scope:** the multipart session/records/protocol (#636), the S3 verbs (#508), the staged-byte
  protection class (#637), `PutObject` chunk-size selection (#508 — a single PUT never segments),
  FU-1's record-shape ADR (#628), FU-5's part-record segmentation (#632), and any file under
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

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

Reviewing issue #635: add backward-compatible segmented chunk maps, staged publication, and shared resolution across every consumer as the >10 GiB multipart precursor.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief settles the wire identity, structural failures, bounded resolver behavior, consumer inventory, publication seam, scope exclusions, and required external tools sufficiently to make acceptance objective. |
| C2 Reproduction (red pre-fix) | PASS | An isolated base-plus-binding-test run executed all eight cases and all eight failed behaviorally on the segmented JSON shape, including the live-GC loss oracle at `crates/custodian/tests/segmented_map_consumers.rs:449`. |
| C3 Change | PASS | The patch stays within the precursor scope: it introduces the discriminated map and staged committer while routing maintenance through the shared entry at `crates/custodian/src/resolve.rs:34`, without adding a multipart producer. |
| C4 Verification (red→green) | PASS | The same eight-test target moved from 0/8 to 8/8 in isolated builds, and a fresh full `cargo xtask ci` passed including the seeded retirement/publication/repoint DSTs at `crates/dst/tests/custodian.rs:1980`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must enforce the 100 KB limit on each caller-supplied flip value: a direct probe was accepted with a 102,401-byte value because `flip_batch` checks only aggregate batch bytes at `crates/core/src/metadata.rs:2494`. |
| T1 Structure | PASS | The production dependency direction remains coherent: custodian consumers share one wrapper and the core resolver owns metadata semantics (`crates/custodian/src/resolve.rs:1`, `crates/core/src/metadata.rs:1863`). |
| T2 Shape | PASS | Legacy arrays serialize unchanged while segmented objects are type-discriminated, and malformed tables are rejected during construction/decode (`crates/core/src/metadata.rs:683`, `crates/core/src/metadata.rs:849`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must accept precursor-only runtime coverage — #636's real `Completing@E` caller was not present or exercised, so publication evidence uses a test-supplied caller contribution at the seam described by `crates/core/src/metadata.rs:2180`. |
| T4 Contribution | NEEDS-HUMAN | A human must triage the eight reported blockers because `scripts/review-branch --bundle` and its truncated result artifact are unavailable here; affected-path merged-history and closed/rejected-work checks found no competing segmented-map implementation. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must extend the envelope oracle to adversarial caller contributions — the current test asserts the per-value ceiling only on its small default fixture at `crates/core/src/metadata.rs:3789`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether to freeze this durable format and publication API before #636 supplies its first production caller, because that choice fixes the integration contract exposed at `crates/core/src/metadata.rs:2173`. |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Attacked the red→green evidence, the encoding/resolver core, and every `.chunk_map`
consumer the diff touches. Leg A's evidence held up (see "could not refute" below); the
landed findings are all on the **write/retirement side** of the slice, which no leg of the
success criterion observes.

## Findings

- **NEEDS-HUMAN [impl] — the three `SegmentedRetirementUnsupported` refusals ship with zero
  test coverage.** `crates/core/src/metadata.rs:1392` (`unlink`), `:1490` and `:1557`
  (`commit_chunk_map_superseding{,_leased}`) are three `.chunk_map` consumers the brief's
  design table does not list — the very hazard `Open questions` 3 flags — and each now hard-
  fails on a segmented prior. Nothing asserts it. Concrete failing case: mutate `:1391` from
  `.as_flat().ok_or(…)?` to `.as_flat().unwrap_or(&[])` and the entire suite stays green,
  while `unlink` would delete a segmented object's `inode:`+dirent and write **zero**
  `orphan:` grace records — the "unreferenced-but-undeadlined fragment ⇒ GC keeps it
  forever" hazard this same function documents at `:1341-1343`. Backfill's analogous
  unlisted-consumer decision got an error type *and* an assertion
  (`crates/custodian/tests/segmented_map_consumers.rs:961`); these three got only prose.

- **NEEDS-HUMAN [impl] — `SegmentedPublication`'s `Supersede` arm drops the prior
  generation's orphan fan-out.** `crates/core/src/metadata.rs:2489`: `flip_batch` emits
  `require(inode == encode(prior))` + `put(root)` + the caller's merged batch, and nothing
  writes `orphan:` records for the fragments the superseded generation placed — while every
  other supersede in the module does it inline and documents why (`:1487-1504`). Concrete
  failing case: publish a segmented map with `RootPrecondition::Supersede(prior)` where
  `prior` is a **flat** committed record holding N chunks (the only prior shape reachable
  today) and `flip = WriteBatch::new()`; the flat generation's fragments become unreferenced
  with no grace record and, per `:1341-1343`, are kept forever. The envelope argument the
  refusals above cite does not apply here — a flat map is ≤ the 100 KB value ceiling by
  construction, so its fan-out is bounded. It is invisible because **every** publication test
  uses `prior = InodeRecord::new_empty()` — a prior with zero chunks
  (`:3123`, `:3190`, `:3829`). Fix: fan out the flat prior's orphans, or state the caller's
  obligation on `RootPrecondition::Supersede` and assert it.

- **NEEDS-HUMAN [impl] — `seggrp_key` / `SEGGRP_MARKER` are dead code, and the docs edit
  claims the guarantee they would provide.** `crates/core/src/metadata.rs:998` and `:282`
  have no caller anywhere in `crates/`: `staged_batches` emits bare `put(seg_key(…))`
  (`:2390`) with no `require_absent(seggrp:<nonce>)`, and no path ever writes the marker. So
  the corrective rule the brief makes normative ("reserved by `require_absent(seggrp:<nonce>)`
  plus the marker record", 0016:499-527) is unenforced by the only committer that exists.
  Meanwhile the architecture edit this PR makes states it as fact —
  "`seggrp:<nonce>` is a presence-only reservation marker **that makes a group nonce
  unrepeatable**" (`docs/design/architecture/08-crosscutting-concepts.md`) — a durable
  property the shipped code does not implement. `seggrp_key`'s byte shape is also unasserted
  (`seg_key`'s is, `:3053`): delete the `:` from `format!("seggrp:{nonce}")` and nothing goes
  red. Either wire the reservation into the committer, or add the key-shape test and reword
  the doc to attribute the reservation to #636.

- **NEEDS-HUMAN [human] — the halting arm the brief calls forbidden.** The brief's *Invariant
  to restore* says a consumer that cannot resolve the map "either fails safe (halting
  maintenance) or, worse, concludes the bytes are unowned. **Both are outcomes 0016
  forbids.**" With `crates/core/src/metadata.rs:1392`/`:1490`/`:1557` in place, once #636
  publishes a segmented map an S3 `DELETE` (`crates/server/src/lib.rs:582` → `unlink`) and a
  PUT-overwrite of that key both return a hard error until the `retire:bytes:{generation}`
  obligation lands. That may well be the right stack ordering, but it is a scope/fitness call
  the brief does not settle, and the in-code deferral carries no tracked-issue marker at those
  three sites (unlike `repoint_chunk`'s `deferred: #636` at `:2070`), so the rubric's
  "Deferrals are settled" protection does not attach to it.

- **NEEDS-HUMAN [human] — the root's byte index is carried but never used to bound a ranged
  read.** `SegmentRef::byte_offset`/`byte_len` exist so "the root alone answers 'which segment
  covers byte N' without reading any segment record" (brief §Design), yet
  `crates/server/src/lib.rs:448` resolves through `resolve_live_chunk_map`, which reads
  **every** segment of the group (`crates/core/src/metadata.rs:1780`) and then walks the whole
  chunk list. At the >10 GiB launch target that is up to `MAX_ROOT_SEGMENTS` × ~50 KB ≈ 25 MB
  of metadata per one-byte ranged GET, and the same whole-map cost lands on
  `high_water_marks` (`:2705`) at every gateway restart. The co-located oracle
  (`crates/server/src/lib.rs:1118`) uses a 2-segment fixture, so it cannot see it. Whether to
  bound this now or in #508 is a design call.

- **NEEDS-HUMAN [human] — C5's red is an unrelated flake, so this iteration carries no
  mutation evidence at all.** `mutants.out/log/baseline.log` shows the *unmutated* baseline
  failing on exactly one test, `crates/server/tests/health_probe.rs:239`
  `shutdown_publishes_not_serving_before_draining` ("connect to the configured health endpoint
  within budget: transport error") — a file this diff does not touch, in a port-binding test,
  under a fully parallel `cargo test -p wyrd-core -p wyrd-custodian -p wyrd-server`. Per issue
  #236 that is **not** a refutation of the fix. But it does mean `C5 surviving mutants`
  measured **zero** mutants for this diff (iteration 2's "1 missed" was a different diff),
  which is exactly how the three unpinned surfaces above survived. Recommend re-running C5
  (serially, or with `health_probe` excluded) before any sign-off leans on causal adequacy.

## Attempted and could not refute

- **The red→green mechanics.** `crates/custodian/tests/segmented_map_consumers.rs` names only
  base symbols — `MemMeta` implements exactly the base trait's `get`/`scan`/`commit`, and the
  bundle base has no `scan_page` (`crates/traits/src/lib.rs:776`), so the file compiles pre-fix
  and its RED is a genuine assertion (`metadata::decode` cannot parse a JSON-object
  `chunk_map` into `Vec<ChunkRef>`, so `gc.rs:256`'s `?` propagates out of `reconcile_step`).
  Post-fix green is independently visible outside the gate's own claim, in
  `mutants.out/log/baseline.log:1420-1427` (all 8 consumer tests) plus the ~30 co-located
  `metadata::tests::*` at `:1110-1161`.
- **"Passes for the wrong reason" on leg 2.** The two objects sit on disjoint halves of the
  fleet, so `Pending` for server 0 can only come from reading the `seg:` range, and the
  server-9 mirror rules out vacuity (`segmented_map_consumers.rs:661-686`). Same for the
  backfill leg, which is driven by the discriminating empty-placement input (`:965`).
- **Flat byte-identity.** `deserialize_any` dispatches a JSON array to `ChunkMap::Flat`,
  `Serialize` emits the bare array, and `#[serde(try_from = "InodeRecordWire")]` preserves the
  field order and the `default`/`skip_serializing_if` pairing — legacy round-trip is asserted
  at `crates/core/src/metadata.rs:2848` and I could not construct a legacy record that
  re-encodes differently.
- **Scan-order dependence.** The resolver keys a `BTreeMap` on the *parsed* index
  (`:1779`,`:1792`) and the `Shuffling` double reverses every scan (`:3901`); a
  concatenate-in-arrival-order resolver dies there.
- **Key-grammar ambiguity.** `+7`, `07`, `007`, 5- and 7-digit indices, and a short nonce are
  all rejected (`parse_canonical_u64` at `:1038`, width check at `:1017`), so one segment has
  exactly one key.
- **Aggregate overflow.** Chunk-length sums and the root tiling are both `checked_add` with
  typed errors at decode (`:930`, `:712`), including the forged-`byte_len`-matching-a-wrapped-
  total case.
- **An unrouted production `.chunk_map` consumer.** Every remaining direct read in
  `crates/*/src` is either `as_flat()`-guarded (`core/src/read.rs:72`, and the three refusals
  above) or an `is_segmented()` dispatch (`custodian/src/backfill.rs:153`); GC, scrub,
  desired-state, restore, rebalance, reconstruction, backfill, both gateway read paths, the
  core read path and `high_water_marks` all go through the one resolver.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must enforce the 100 KB limit on each caller-supplied flip value: a direct probe was accepted with a 102,401-byte value because `flip_batch` checks only aggregate batch bytes at `crates/core/src/metadata.rs:2494`.
- [ ] T3 Runtime — Maintainers must accept precursor-only runtime coverage — #636's real `Completing@E` caller was not present or exercised, so publication evidence uses a test-supplied caller contribution at the seam described by `crates/core/src/metadata.rs:2180`.
- [ ] T4 Contribution — A human must triage the eight reported blockers because `scripts/review-branch --bundle` and its truncated result artifact are unavailable here; affected-path merged-history and closed/rejected-work checks found no competing segmented-map implementation.
- [ ] T5 Judgment — Rebuild must extend the envelope oracle to adversarial caller contributions — the current test asserts the per-value ceiling only on its small default fixture at `crates/core/src/metadata.rs:3789`.
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether to freeze this durable format and publication API before #636 supplies its first production caller, because that choice fixes the integration contract exposed at `crates/core/src/metadata.rs:2173`.
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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must enforce the 100 KB limit on each caller-supplied flip value: a direct probe was accepted with a 102,401-byte value because `flip_batch` checks only aggregate batch bytes at `crates/core/src/metadata.rs:2494`.; T3 Runtime — Maintainers must accept precursor-only runtime coverage — #636's real `Completing@E` caller was not present or exercised, so publication evidence uses a test-supplied caller contribution at the seam described by `crates/core/src/metadata.rs:2180`.; T4 Contribution — A human must triage the eight reported blockers because `scripts/review-branch --bundle` and its truncated result artifact are unavailable here; affected-path merged-history and closed/rejected-work checks found no competing segmented-map implementation.; T5 Judgment — Rebuild must extend the envelope oracle to adversarial caller contributions — the current test asserts the per-value ceiling only on its small default fixture at `crates/core/src/metadata.rs:3789`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
