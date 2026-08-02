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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 57 mutants tested in 2m: 17 caught, 40 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #649: add one bounded, tear-safe segmented chunk-map resolver and route core and gateway reads through it.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: one shared resolver, three store-aware read sites, bounded request work, typed per-object refusal, and both retirement arms are explicitly required (crates/core/src/metadata.rs:2422). |
| C2 Reproduction (red pre-fix) | PASS | Independent base reruns compiled both added targets and failed all 12 core plus 2 gateway cases at the base `SegmentedMapUnsupported` path, so the asserted symptom is assertion-red (crates/core/tests/segmented_map_resolution.rs:458; crates/server/tests/segmented_object_read.rs:146). |
| C3 Change | PASS | The production change stays on the requested resolver/read surfaces, keeps `read_object_from` store-free and fail-closed, and wires whole, streaming, and ranged reads without the excluded custodian wrapper (crates/core/src/read.rs:65; crates/server/src/lib.rs:353; crates/server/src/lib.rs:441). |
| C4 Verification (red→green) | PASS | The same targets turned 12/12 and 2/2 green; docs, fmt, clippy, build, workspace tests, conformance, statics, and DST also passed, while full CI stopped only on the unchanged, tracked #673 base advisory at Cargo.lock:91. |
| C5 Causal adequacy | PASS | The independent in-diff mutation run left no survivors (57 tested: 17 caught, 40 unviable), including the exact ceiling edge and both one-component group mismatches (crates/core/tests/segmented_map_resolution.rs:611; crates/core/tests/segmented_map_resolution.rs:813). |
| T1 Structure | PASS | Exactly two public resolution entries share one private range/retry implementation, and every in-slice chunk-consuming read site calls that authority (crates/core/src/metadata.rs:2422; crates/core/src/metadata.rs:2446; crates/core/src/read.rs:527). |
| T2 Shape | NEEDS-HUMAN | Whether to waive the brief's explicit ~1,000-semantic-line STOP for this measured 1,307-line, seven-file patch — the 30.7% overage, including 647 semantic lines in the core discriminator, materially enlarges the review surface (crates/core/tests/segmented_map_resolution.rs:1). |
| T3 Runtime | PASS | Real redb reads, request-log bounds, the multi-page ceiling edge, per-object containment, and the madsim retirement property all passed under execution (crates/core/tests/segmented_map_resolution.rs:514; crates/core/tests/segmented_map_resolution.rs:611; crates/dst/tests/custodian.rs:1450). |
| T4 Contribution | NEEDS-HUMAN | Whether the single blocker reported by `T4-batch-review` is acceptable — its `scripts/review-branch` scanner is absent from both the supplied artifacts and target, so I could not rerun or ground that result; contribution checks, docs currency, and the exhaustive affected-path prior-art scan otherwise passed (docs/design/architecture/06-runtime-view.md:29). |
| T5 Judgment | PASS | No capability probe or runtime symptom guard was introduced; resolution replaces the unsupported store-aware read branches at their common cause, with mutation-sensitive boundary evidence (crates/core/src/metadata.rs:2247; crates/core/src/read.rs:527). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whether raw-seeded redb/fake-store plus DST evidence is sufficient fitness proof before #653 supplies the producer — this slice cannot yet demonstrate a producer-to-reader end-to-end segmented object, even though its declared `typos` and docs-renderer dependencies were exercised. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T2 Shape — Whether to waive the brief's explicit ~1,000-semantic-line STOP for this measured 1,307-line, seven-file patch — the 30.7% overage, including 647 semantic lines in the core discriminator, materially enlarges the review surface (crates/core/tests/segmented_map_resolution.rs:1).
- [ ] T4 Contribution — Whether the single blocker reported by `T4-batch-review` is acceptable — its `scripts/review-branch` scanner is absent from both the supplied artifacts and target, so I could not rerun or ground that result; contribution checks, docs currency, and the exhaustive affected-path prior-art scan otherwise passed (docs/design/architecture/06-runtime-view.md:29).
- [ ] Validation — fitness-to-purpose — Whether raw-seeded redb/fake-store plus DST evidence is sufficient fitness proof before #653 supplies the producer — this slice cannot yet demonstrate a producer-to-reader end-to-end segmented object, even though its declared `typos` and docs-renderer dependencies were exercised.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Unresolved blocking finding at crates/core/src/metadata.rs:2162 (T4 batch-review, not recorded-rejected): the resolve-retry arbiter collapses an absent root into the same `false` result as a superseded generation, so a deletion observed on the final allowed retry attempt returns `MapResolutionUnstable` instead of `None`. This is a real correctness gap in the resolve-retry logic that success criterion 3 of the brief (resolution is total and never tears; root-gone must be distinguished from root-superseded) directly requires. Fix the arbiter to distinguish "root absent" from "root superseded" so a genuinely deleted object resolves to not-found rather than an unstable/retry error, then re-run the batch review to confirm the finding clears. The other §6 items (T2 Shape line-count overage, Validation fitness-to-purpose pending #653) were not adjudicated this round — revisit at the next sign-off once this finding is fixed.
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
