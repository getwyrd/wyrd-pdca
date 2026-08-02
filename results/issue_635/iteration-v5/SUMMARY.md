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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 315 mutants tested in 8m: 160 caught, 155 unviable

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

Task under review: implement a byte-compatible Flat-or-Segmented chunk map with bounded shared resolution and fenced staged publication for objects beyond the single-value ceiling.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief settles the wire form, consumer set, failure semantics, publication envelope, and assertion-red stack posture, so no implementation-defining plan choice remains open. |
| C2 Reproduction (red pre-fix) | FAIL | On the normative #634 stack no binding tests run: the added `MemMeta` omits #634's required `scan_page`, so its impl at `crates/custodian/tests/segmented_map_consumers.rs:83` fails to compile instead of producing the required assertion-red. |
| C3 Change | PASS | The production change covers the declared persisted shape, staged committer, shared resolver, eight consumers, DST surface, and living architecture documentation without expanding into the deferred multipart-session slice. |
| C4 Verification (red→green) | NEEDS-HUMAN | Sign-off must require a refreshed #634-stack rerun before trusting green — the stale main target produced 8 assertion-red→8 green and full CI passed, but #634 makes the added store double at `crates/custodian/tests/segmented_map_consumers.rs:83` compile-fail. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must make the binding test compile and assert on #634 before its causal claim is usable — 315/315 main-base mutants were caught or unviable, but that cannot replace the zero-test stack leg at `crates/custodian/tests/segmented_map_consumers.rs:83`. |
| T1 Structure | PASS | One total core resolver plus the thin custodian adapter keeps ownership decisions on the same bounded seam (`crates/core/src/metadata.rs:2206`; `crates/custodian/src/resolve.rs:53`). |
| T2 Shape | PASS | Decode rejects inconsistent segment count/order/tiling and root size while preserving the legacy array identity, satisfying the repository's metadata-boundary and CAS rules (`crates/core/src/metadata.rs:930`; `crates/core/src/metadata.rs:1374`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must decide whether precursor-only seam tests are sufficient before #636 supplies the real `Completing@E` caller — otherwise fence/progress coupling remains unexercised at integration (`crates/core/src/metadata.rs:2485`). |
| T4 Contribution | NEEDS-HUMAN | A reviewer must recover and triage the unavailable five-blocker batch-review report before sign-off — `scripts/review-branch` and its truncated result were unavailable, although contribution checks and affected-path prior-art inspection found no relevant overlap. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add #634's required `scan_page` to all six new `MetadataStore` doubles before the branch is stackable; representative omissions are at `crates/core/src/metadata.rs:5153`, `crates/custodian/tests/segmented_map_consumers.rs:83`, and `crates/server/src/lib.rs:882`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must accept the pinned encoding and precursor-only publication seam as fit for the >10 GiB launch requirement before #636's real caller exists, because the runtime contract is now documented as operational (`docs/design/architecture/06-runtime-view.md:24`). |

### Advisory — adversary

# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Method: re-ran the asserted red→green myself in throwaway scratch copies (now deleted), then
attacked the fix with three probe tests written against the **production** API in a scratch
copy of the patched tree. Two probes reproduced concrete failing cases; a third pinned a
blast-radius escalation. Everything below is grounded on the target source at `$PDCA_TARGET`.

## Evidence re-run (what I could confirm)

- **C4-verify's claim reproduces.** Base tree (`git archive HEAD`) + only the added test file:
  `cargo test -p wyrd-custodian --test segmented_map_consumers` → **8 failed / 0 passed**, and
  every failure is an *assertion / unwrap*, not a build error (`invalid type: map, expected a
  sequence at line 1 column 23`), i.e. the brief's "RED is an assertion" requirement holds and
  the `run-verify.sh` PASS-over-a-build-failure hazard did **not** bite. Patched tree → **8
  passed**. Full `cargo test --workspace` on a clean copy is green apart from
  `xtask/tests/repo_hygiene_guards.rs:129`, which shells out to `git ls-files` and fails only
  because my scratch copy has no `.git` — an artefact of my sandbox, not of the patch.
- **C5 was not re-run** (`cargo mutants --in-diff` is available but an 8-minute campaign was not
  worth the budget); I hand-checked five high-value mutants instead (drop the final
  `root_still_names` check, invert `retired_or_fail`, drop the contiguity check, relax
  `check_fenced`'s `expected.is_some()`, drop backfill's `to_fill.is_empty()` guard) and each is
  killed by a named test.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2939` (`SegmentedPublication::publish`)
  commits the whole segment phase before it ever *builds* the flip batch**, so a flip the
  committer itself refuses deterministically and with zero I/O is discovered only after the
  `seg:` records are durable *and* the caller's cursor has moved. Probed twice against
  `RedbMetadataStore::in_memory()`: (a) a flip contribution carrying a 100 001-byte value →
  `publish()` = `Err(ValueOverCeiling)` with **1 durable `seg:` row**; (b) a flip with no fence →
  `Err(Unfenced)` with **1 durable `seg:` row and the session record left at
  `Completing@7|written=1`**. This contradicts the patch's own stated rationale at
  `crates/core/src/metadata.rs:3022-3025` ("a publication that emitted it would fail at commit …
  *after its segments were already durable*") — the per-record ceiling check exists precisely to
  avoid this, and `publish()` reintroduces it for six error variants (`Unfenced`,
  `ContributionCollides`, `ValueOverCeiling`, `KeyOverCeiling`, `BatchOverBudget`,
  `BatchOverOps`). Fix is one line: evaluate `self.flip_batch()?` **before** `write_segments`.
- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2698` (`staged_batches`) trusts
  `resume_from` against a durable prefix it never reads, and a violation publishes a
  permanently unreadable object.** Probe: attempt 1 writes 20 segments for chunk list A and
  "crashes"; attempt 2 uses the **same** `nonce:epoch`, `resume_from = 20`, and a chunk list
  differing only in `chunks[0].len` (8 → 16). `segment_batches()` returns **0 batches** (the
  cursor is at the plan end), `root()` is built from list B, the flip commits **`Committed`** —
  and every subsequent resolve fails with `SegmentBoundsMismatch { index: 0, root: (0, 328),
  segment: (0, 16) }`. The doc at `crates/core/src/metadata.rs:2527-2531` asserts the prefix "is
  re-derived identically" but nothing checks it, and the failure is silent at publication and
  terminal at read. A one-`get` defence is available: compare
  `store.get(seg_key(group, resume_from - 1))` with `encode(planned[resume_from - 1].1)` and
  refuse with a typed error before the flip. (If the team prefers to declare this #636's
  contract, that is a legitimate recorded rejection — but it should be recorded, not implicit.)
- **NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:3272` (`high_water_marks`) turns one
  damaged segmented object into a store-wide outage.** The patch moves a **store-state-dependent**
  resolution into whole-store scans that previously could only fail on an undecodable *value*.
  Probe: a store holding one healthy flat object and one segmented object whose
  `seg:<nonce>:<epoch>:000001` is missing while its root still names the group →
  `high_water_marks()` = `Err(segment 1 of live generation … is absent — fail closed)`.
  `Gateway::recover()` is exactly that call (`crates/server/src/lib.rs:123-125`) and the
  composition root runs it "after `new` and before serving", so **the gateway refuses to start**
  and every flat object in the store becomes unavailable. The same `?` shape sits in
  `crates/custodian/src/gc.rs:262`, `restore.rs:381`, `rebalance.rs:165`,
  `reconstruction.rs:614`, `backfill.rs:109`, so GC, scrub, reconstruction, rebalance and restore
  all stop store-wide too. Per-object fail-closed is right and is what 0016 asks for; making it
  abort a whole-store scan is a *different* decision, and this repo already has the containment
  precedent (`desired_state::reconciliation_status`'s `PendingMalformed` — refuse to certify, but
  attribute and keep going). Needs a human call on blast-radius per consumer (GC must fail
  closed; startup recovery arguably must not).
- **NEEDS-HUMAN [human] — stack-base evidence is unverifiable here, and the added double still
  lacks `scan_page` (`crates/custodian/tests/segmented_map_consumers.rs:82-114`).** The brief made
  `$PDCA_VERIFY_BASE = origin/pdca-integration/main` (#634 in wave 0) a gate-evaluability
  precondition and told Do that `MemMeta` "must implement `scan_page` … one delegating line".
  `MetadataStore` in this worktree (`crates/traits/src/lib.rs:767-777`) has **no** `scan_page`,
  `origin/pdca-integration/main` does not exist in this checkout, and HEAD is plain `origin/main`
  (`b0cd199`). So the red→green I reproduced is genuine *on this base*, and adding `scan_page`
  now would not compile here — but the iteration-1/-4 carry-forward finding (E0046 on the
  normative stack) is neither reproduced nor closed. **Verdict provisional: toolchain/base
  unavailable in this sandbox (issue #236), not scored as a refutation.**

## Attacked and could not refute

- Leg A's oracles are not vacuous: leg 2's `Pending` has a live mirror (`server 9` → `Satisfied`,
  `:679-686`) and the two objects sit on disjoint halves of the fleet, so `Pending` can only come
  from reading the `seg:` range; a resolver returning `Ok(None)` for segmented maps passes leg 1
  but fails legs 2 and 3. Leg 4 is asserted on fragment presence and again standing alone
  (`:584-640`), not on a `Reconciled` value.
- Every production reader of `.chunk_map` either routes through the resolver or fails closed:
  `unlink` (`metadata.rs:1624`), `commit_chunk_map` (`:1667`), both superseding committers
  (`:1738`, `:1805`) and `repoint_chunk` (`:2376`, `:2409`) all `ok_or`/refuse a segmented shape;
  I found no remaining silent walk in `crates/*/src/**`, and no scan prefix collides with `seg:` /
  `seggrp:`.
- Ordering: `read_segments` (`metadata.rs:2036-2057`) keys a `BTreeMap` on the **parsed** index, so
  a shuffled `scan` cannot reorder the map; the in-test `MemMeta` iterates a `HashMap`, which
  shuffles for free.
- Byte-identity: legacy flat decode→encode is exercised on raw stored bytes end-to-end
  (`metadata.rs:3428`, `:3446`), and the segmented rebalance test proves it for the new shape by
  landing a `require(inode == encode(prior_root))` CAS against hand-written fixture bytes.
- Envelope/ceiling/fence checks are already probed adversarially on the *caller's* contribution
  rather than the committer's own fixture (`metadata.rs:4250-4345`, `:4355`, `:4703`), closing the
  iteration-3/-4 findings I tried to re-raise.
- Non-finding worth one line (no NEEDS-HUMAN): `crates/custodian/src/resolve.rs:34-42` claims
  `LiveMap.record` "is the live one", but the **flat** arm returns immediately with no currency
  re-read, so a stale flat snapshot whose live root has moved yields `restarted: None` and the
  superseded chunk list. I could not build harm from it — backfill/rebalance/reconstruction all
  lose their CAS, and GC only reclaims fragments carrying orphan/expired-lease evidence
  (`gc.rs:171-190`), which a freshly published generation has none of — so this is a doc
  overclaim, not a defect.

## On the verdict

`check-gates.json` is `overall: fail` on **T4** alone (5 blocking findings, artifact path
truncated in the row). Nothing I found contradicts C4-ci or C4-verify; C1/C2/C3/T1–T3/T5 are
`none`, so the only positive claims on the sheet that I could test, I tested.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Sign-off must require a refreshed #634-stack rerun before trusting green — the stale main target produced 8 assertion-red→8 green and full CI passed, but #634 makes the added store double at `crates/custodian/tests/segmented_map_consumers.rs:83` compile-fail.
- [ ] C5 Causal adequacy — Rebuild must make the binding test compile and assert on #634 before its causal claim is usable — 315/315 main-base mutants were caught or unviable, but that cannot replace the zero-test stack leg at `crates/custodian/tests/segmented_map_consumers.rs:83`.
- [ ] T3 Runtime — Maintainers must decide whether precursor-only seam tests are sufficient before #636 supplies the real `Completing@E` caller — otherwise fence/progress coupling remains unexercised at integration (`crates/core/src/metadata.rs:2485`).
- [ ] T4 Contribution — A reviewer must recover and triage the unavailable five-blocker batch-review report before sign-off — `scripts/review-branch` and its truncated result were unavailable, although contribution checks and affected-path prior-art inspection found no relevant overlap.
- [ ] T5 Judgment — Rebuild must add #634's required `scan_page` to all six new `MetadataStore` doubles before the branch is stackable; representative omissions are at `crates/core/src/metadata.rs:5153`, `crates/custodian/tests/segmented_map_consumers.rs:83`, and `crates/server/src/lib.rs:882`.
- [ ] Validation — fitness-to-purpose — Maintainers must accept the pinned encoding and precursor-only publication seam as fit for the >10 GiB launch requirement before #636's real caller exists, because the runtime contract is now documented as operational (`docs/design/architecture/06-runtime-view.md:24`).
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
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Wrong verification base: the brief required verifying against origin/pdca-integration/main (which carries #634's scan_page addition to MetadataStore), but that branch does not exist in this sandbox/checkout, so Do verified red->green against plain origin/main instead. The added test double (MemMeta / segmented_map_consumers.rs:83) is missing scan_page, which the brief told Do to add, and would not compile against the real #634 stack. Both the primary and adversarial reviewers independently flagged this as the root issue (T5 Judgment, C4/C5 NEEDS-HUMAN items, and the stack-base-unverifiable finding). Plan needs to fix the base setup/dependency wiring (make #634's stack actually reachable, e.g. ensure pdca-integration/main exists or otherwise resolve the #634 -> #635 sequencing) before another Do attempt, rather than Do re-guessing at a moving target. Also carry forward for the next Do pass once the base is fixed: (1) SegmentedPublication::publish commits segment writes before validating the flip will succeed (crates/core/src/metadata.rs:2939) - evaluate flip_batch() before write_segments; (2) staged_batches/resume_from trusts the resumed cursor without verifying the durable prefix (crates/core/src/metadata.rs:2698) - add a one-get defence before resuming; (3) high_water_marks turning one damaged segmented object into a store-wide outage (crates/core/src/metadata.rs:3272) needs a human blast-radius decision on whether per-consumer containment (like desired_state's PendingMalformed pattern) is required instead of fail-closed on the whole scan.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
