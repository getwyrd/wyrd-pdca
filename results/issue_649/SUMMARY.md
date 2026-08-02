# Result — issue 649 / shared-segmented-map-resolver-and-read-paths

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  After #648 an `InodeRecord.chunk_map` can be `ChunkMap::Segmented`, but **nothing
  can resolve one**. Every reader still takes the inline list off the record and fails closed —
  `crates/core/src/read.rs:96-97`, `crates/server/src/lib.rs:364-365` (whole-object / streaming)
  and `:459-460` (ranged), each `as_flat().ok_or(SegmentedMapUnsupported)` on this slice's base.
  There is no shared resolution call at all (`git -C ../wyrd grep -n "resolve_chunk_map\|MapResolution"
  pdca-integration/main -- crates/` is empty), so the way each consumer learns which chunks an
  object owns is about to be re-derived once per consumer — which 0016 decision 7(e) forbids, and
  which is exactly how #508's 4th attempt let GC delete a live object's fragments.
- Success criterion:
  Two **added** test files pass, and every assertion is driven through
  **base-visible** entry points (`wyrd_core::read::{read_object, read_path}` and
  `wyrd_gateway_core::ObjectGateway`) over objects seeded as **raw `seg:` records** (never via a
  committer — none exists until #653):
  1. **Byte-identical reads.** A segmented object reads back — whole-object, and over a range
     that **spans a segment boundary** — byte-identical to the flat equivalent of the same
     payload, through both the core read path and the gateway.
  2. **The work a read demands is bounded by the reader, not by the record.** Asserted on a
     **self-contained fake `MetadataStore`** (see *Test file*) whose recorded request log — every
     `get` key, and every `scan_page` prefix / cursor / limit — **is** the oracle:
     a. resolving one object requests the root key plus keys under **only** `seg:<nonce>:<epoch>:`
        — a second group seeded under a different nonce, **and the same nonce at a different
        epoch**, are never requested;
     b. a root naming more segments than `MAX_ROOT_SEGMENTS` is refused with a typed error and
        the log shows **no range request at all** (refused unread);
     c. every page request carries a limit no larger than **a fixed bound this reader sets** —
        never the root's claimed segment count — so a record cannot size one page;
     d. an object that cannot be resolved fails closed **for that object only**: a second,
        well-formed object seeded in the same store still reads.
  3. **Resolution is total and never tears.** Both arms of the resolve-retry rule, asserted on
     the interleaving: root **moved on** (or gone) + a segment absent ⇒ the resolution is dropped
     as a concurrently-retired generation (the read restarts, or answers no-such-key) — never a
     torn half-map, never "this object owns no bytes"; root **unchanged** + a segment that is
     absent, undecodable, over `MAX_VALUE_BYTES`, unnamed, or whose extents disagree with the
     root's table ⇒ **fail closed** with a typed error. Chunks are ordered by the **parsed
     index**, proved against a deliberately shuffling fake. A read driven from a stale
     (superseded) snapshot resolves against the **live** root.

  Also in the issue's acceptance and shipped here, but **not** a Check discriminator: the DST
  resolver-tear property (see *Verification posture*).
- Repo + branch target:
  getwyrd/wyrd @ main
  (Verified at Plan, and written with **no backticks after the `@`** on purpose: origin/main is
  9120f7a, and this bundle's build/verify base is the wave fold origin/pdca-integration/main =
  6e7c255, carrying #648. The bash base-parser in `engine/scripts/run-verify.sh:168-178` takes the
  first backticked span **anywhere** in the field, unlike its Python twin
  `publish._clean_ref:400-414`, which anchors the span at the start after #235 — so a backtick
  later on this line would resolve `origin/origin/main`. Harmless here because a wave>0 bundle is
  handed `PDCA_VERIFY_BASE` explicitly, but do not reintroduce one.)
- Scope (one logical fix) / out of scope:
  **the one shared resolver, and the three read call sites that consume it in this
  slice.** Four files of production change, nothing that reclaims, repairs or publishes:
  - `crates/core/src/metadata.rs` — the resolution result type, the group-range read with its
    pre-read segment-count ceiling and its fixed page bound, the resolve-retry arbiter, the
    resolve-against-the-live-root entry, and the `ChunkMapError` variants they raise. **At most
    two public resolve entries** (one from a caller-held snapshot record, one that reads the live
    root); a third needs a caller **in this slice** or it does not ship. Iteration 7 shipped three
    entries and two result types — that breadth is a named cause of the review surface.
  - `crates/core/src/read.rs` — the placement-aware entries (`read_object`, `read_path`) resolve
    through the resolver before assembling bytes. **`read_object_from` keeps its current
    signature** (`:60`) and keeps failing closed on a segmented map: it takes no `MetadataStore`,
    so it *cannot* resolve, and it has **no production caller** — only tests and one bench
    (verified at Plan). Iteration 7 changed its shape and spent ~200 lines and 10 files migrating
    those callers for nothing. If a signature change ripples past two files, that is the wrong
    shape — stop and hand back.
  - `crates/server/src/lib.rs` — the two gateway sites (`:364-365` streaming, `:459-460` ranged)
    resolve through the resolver instead of `as_flat()`. This is a ~6-line swap at each site; the
    surrounding stream/range logic is not this slice's to rewrite.
  - `crates/dst/tests/custodian.rs` — the DST resolver-tear property, added to the existing file.
  **Out of scope:** `crates/custodian/src/resolve.rs` — the custodian-facing wrapper has **no
  caller in this slice** (its consumers are #650/#651, which name the core resolver, not it); it
  ships with its first caller, not here. The byte-materialisation bound (**#674**, settled above).
  GC / scrub / `ReferenceSet` (#650); restore, reconstruction, backfill, rebalance, `desired_state`,
  `repoint_chunk` (#651); the chunk-id floor and startup recovery (#652); the committer, fence,
  rollback and resume (#653); the base lockfile advisory (**#673**); any new/edited ADR / spec /
  proposal; any conformance-vector change.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny check` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 77 mutants tested in 2m: 16 caught, 61 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review task: add one shared segmented chunk-map resolver and route the core and gateway read paths through bounded, tear-free resolution.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract fixes the observable read entries, bounds, retirement arms, exclusions, and raw-record oracle precisely enough to judge without an unresolved implementation choice. |
| C2 Reproduction (red pre-fix) | PASS | With only the two added tests retained on base `6e7c255`, both targets compiled and failed by assertion—0/13 core and 0/2 gateway—at the intended base-visible read entries (`crates/core/tests/segmented_map_resolution.rs:466`; `crates/server/tests/segmented_object_read.rs:147`). |
| C3 Change | PASS | The patch stays on the declared seven paths, preserves the store-less `read_object_from` boundary, exposes only the two allowed resolver entries, and routes the three store-aware consumers through the shared ownership answer (`crates/core/src/metadata.rs:2481`; `crates/core/src/metadata.rs:2509`; `crates/server/src/lib.rs:354`). |
| C4 Verification (red→green) | PASS | The same targets turned green at 13/13 and 2/2, all non-DST workspace checks and the full DST suite passed; cargo-deny alone reproduces unchanged-base RUSTSEC-2026-0221 tracked in #673, not a patch defect (`crates/dst/tests/custodian.rs:1554`). |
| C5 Causal adequacy | PASS | The missing shared resolution path is directly exercised through every in-scope reader, both retirement outcomes and exact bounds are load-bearing, and an independent 77-mutant run left no survivors (`crates/core/tests/segmented_map_resolution.rs:1089`). |
| T1 Structure | PASS | One resolver owns group-range validation and retry arbitration while callers only consume its record/chunk result, preserving the narrow metadata seam and a single ownership answer (`crates/core/src/metadata.rs:2238`; `crates/core/src/metadata.rs:2481`). |
| T2 Shape | NEEDS-HUMAN | Decide whether to waive the brief's approximately 1,000-semantic-line STOP or return to Plan—an independent patch count is 1,396 nonblank/noncomment additions across seven files, so slice reviewability remains unresolved. |
| T3 Runtime | PASS | Reader-controlled limits refuse oversized tables before range I/O, page at a fixed 128 rows, and terminate churn after three attempts; workspace and madsim execution passed (`crates/core/src/metadata.rs:2303`; `crates/core/src/metadata.rs:2313`; `crates/core/src/metadata.rs:2513`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether to accept the reported contribution-artifact pass without an independent rerun—the reviewer inputs and target contain neither `scripts/pdca` nor the contribution artifacts, so opener/tracker completeness remains provisional. |
| T5 Judgment | PASS | No implementation defect survived the deep path review, affected-path prior-art audit, red→green run, full repository/DST execution, or mutation run; the remaining questions are explicit scope and sign-off decisions, not missed behavior (`docs/design/architecture/06-runtime-view.md:29`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether raw `seg:` seeding plus simulated retirement is sufficient before #653 supplies a real publisher—reader correctness is strongly evidenced, but end-to-end fitness of actually produced segmented data cannot yet be observed (`crates/core/tests/segmented_map_resolution.rs:477`; `crates/dst/tests/custodian.rs:1450`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T2 Shape — Decide whether to waive the brief's approximately 1,000-semantic-line STOP or return to Plan—an independent patch count is 1,396 nonblank/noncomment additions across seven files, so slice reviewability remains unresolved.
- [x] T4 Contribution — Decide whether to accept the reported contribution-artifact pass without an independent rerun—the reviewer inputs and target contain neither `scripts/pdca` nor the contribution artifacts, so opener/tracker completeness remains provisional.
- [x] Validation — fitness-to-purpose — Decide whether raw `seg:` seeding plus simulated retirement is sufficient before #653 supplies a real publisher—reader correctness is strongly evidenced, but end-to-end fitness of actually produced segmented data cannot yet be observed (`crates/core/tests/segmented_map_resolution.rs:477`; `crates/dst/tests/custodian.rs:1450`).
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
