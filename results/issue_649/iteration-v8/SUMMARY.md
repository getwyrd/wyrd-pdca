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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 57 mutants tested in 2m: 2 missed, 15 caught, 40 unviable

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

Task under review: add one shared segmented chunk-map resolver and wire core and gateway reads so whole and cross-segment ranged reads are bounded, fail closed, and never tear across generations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief settles the resolver contract, read-path scope, byte-bound ownership, discriminator tests, dependencies, and hard size budget, so no Plan ambiguity prevents judgment. |
| C2 Reproduction (red pre-fix) | PASS | On the #648 base, all 10 core and both gateway discriminator tests compiled and failed by assertion at the old blanket refusal, proving a behavioral red rather than a compile red (crates/core/tests/segmented_map_resolution.rs:447; crates/server/tests/segmented_object_read.rs:146). |
| C3 Change | FAIL | The brief makes rejection-ledger coverage mandatory at the materialisation/range checks, but the seven-file patch ships no `review-rejected.md`; omitting it lets settled #674 and timeout findings be re-raised (crates/core/src/metadata.rs:2094). |
| C4 Verification (red→green) | PASS | The same 12 discriminator tests turned green; typos, docs lint/render, fmt, clippy, build, workspace tests, conformance, statics, and DST also passed, while cargo-deny's sole substantive red is the declared base-only RUSTSEC-2026-0221 at Cargo.lock:1205, already tracked as #673. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add exact-ceiling and one-component group-mismatch coverage (or remove the unsupported defense): mutation rerun left `>`→`>=` and `||`→`&&` alive, so two claimed boundary checks are not test-bound (crates/core/src/metadata.rs:2271; crates/core/src/metadata.rs:2290). |
| T1 Structure | PASS | The production shape is one resolver module with exactly two public resolve entries, three store-capable read consumers, and the snapshot-only `read_object_from` signature left intact (crates/core/src/metadata.rs:2422; crates/core/src/metadata.rs:2446; crates/core/src/read.rs:66). |
| T2 Shape | FAIL | Re-enter Plan or approve an explicit scope waiver: about 1,249 added nonblank/non-comment lines exceed the brief's ~1,000-line hard stop by roughly 25%, with the core discriminator alone reaching 893 lines (crates/core/tests/segmented_map_resolution.rs:893). |
| T3 Runtime | PASS | Real-redb reads, self-contained fake-store request oracles, gateway streaming/range tests, and the madsim resolver-tear campaign all passed; both declared external tools were exercised rather than skipped (crates/dst/tests/custodian.rs:1553). |
| T4 Contribution | NEEDS-HUMAN | Confirm the closed/rejected prior-art search and T4 review/contribution results: `scripts/review-branch`, `scripts/pdca`, and forge state are absent from the allowed artifact/target, so local path history cannot independently reproduce those pass claims or settle rejected work (AGENTS.md:206). |
| T5 Judgment | NEEDS-HUMAN | Decide whether to return this slice to Plan for the explicit budget and ledger breaches or consciously waive them; acceptance without that decision recreates the reviewability failure the re-slice was meant to cure (crates/core/tests/segmented_map_resolution.rs:893). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether reader-bounded paging and live-root retry are operationally fit for production data ownership; automated red→green and DST evidence cannot establish the acceptable real-workload latency and availability tradeoff (docs/design/architecture/06-runtime-view.md:29). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Add exact-ceiling and one-component group-mismatch coverage (or remove the unsupported defense): mutation rerun left `>`→`>=` and `
- [x] T4 Contribution — Confirm the closed/rejected prior-art search and T4 review/contribution results: `scripts/review-branch`, `scripts/pdca`, and forge state are absent from the allowed artifact/target, so local path history cannot independently reproduce those pass claims or settle rejected work (AGENTS.md:206). — human OK: trusting the deterministic gate evidence (check-gates.md T4 batch-review and contribution-artifact checks both pass) over the reviewer's sandbox limitation.
- [x] T5 Judgment — Decide whether to return this slice to Plan for the explicit budget and ledger breaches or consciously waive them; acceptance without that decision recreates the reviewability failure the re-slice was meant to cure (crates/core/tests/segmented_map_resolution.rs:893). — human OK: waived; overage (1,249 vs ~1,000) is entirely in the two new discriminator test files, production code is under budget, and build-notes.md §3 justifies each line.
- [x] Validation — fitness-to-purpose — Decide whether reader-bounded paging and live-root retry are operationally fit for production data ownership; automated red→green and DST evidence cannot establish the acceptable real-workload latency and availability tradeoff (docs/design/architecture/06-runtime-view.md:29). — human OK: fit for purpose.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1 — human OK: confirmed pre-existing base-tree failure (RUSTSEC-2026-0221, event-listener 5.4.1), fails identically on unmodified base, tracked as getwyrd/wyrd#673, out of scope for this slice per brief.
- [x] external dependency.** — moot: parsing artifact from build-notes.md's "**No NEEDS-HUMAN external dependency.**"; not a real finding (human-confirmed, see §10).
- [x] C3 Change — Decide whether the unrelated lockfile-only `event-listener` 5.4.1→5.4.2 upgrade belongs—there is no manifest or brief dependency change, so accepting `Cargo.lock:1205` expands supply-chain review scope. — moot: stale finding from an earlier build round (through_round: 5); the final patch.diff touches no Cargo.lock at all (human-confirmed, see §10).
- [x] C1 Spec — Decide whether universal consumer routing is required in this slice or only at the six-slice endpoint—the living architecture claims it now at `docs/design/architecture/06-runtime-view.md:29`, while the maintenance wrapper explicitly defers its callers to #650/#651 at `crates/custodian/src/resolve.rs:21`; this determines whether the current-state documentation and safety invariant are premature. — human OK: deferring custodian routing to #650/#651 is intentional slicing to get better per-slice results; not premature.
- [x] C3 Change — Decide whether to accept or re-slice the review surface—approximately 1,925 nonblank, noncomment additions remain after excluding the declared call-site migrations, materially above the brief's ~1,500-line ceiling, led by `crates/core/tests/segmented_map_resolution.rs:1` and `crates/core/src/metadata.rs:2040`. — moot: stale finding from an earlier build round; the final patch.diff drops crates/custodian/src/resolve.rs entirely, so this 1,925-line measurement no longer applies (the current, live measurement is the T2/T5 item: 1,249 vs ~1,000, still open) (human-confirmed, see §10).

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
- Iteration delta (if iterating): C5 Causal adequacy is unresolved: mutation testing left two mutants alive at crates/core/src/metadata.rs:2271 (`>`->`>=`) and crates/core/src/metadata.rs:2290 (`||`->`&&`) — the segment-count ceiling's exact edge and a group-mismatch condition are not proven by any test. Add targeted test coverage that kills both mutants (an exact-ceiling case for MAX_ROOT_SEGMENTS, and a one-component group-mismatch case), or — if on inspection either check turns out not to be load-bearing — remove the unsupported defense instead. Re-run the C5 mutants-in-diff gate after. All other §6 items were cleared by the human at sign-off: - C1 Spec, T4 Contribution, T5 Judgment, Validation fitness-to-purpose, and the C4 deny-gate (pre-existing base issue #673) were explicitly accepted/waived. - Two §6 bullets (the event-listener/Cargo.lock item and the ~1,925-line item) were stale findings from an earlier internal build round (deferred-findings.json, through_round: 5) referencing code no longer in the final patch.diff; confirmed moot and cleared. - One §6 bullet ("external dependency.**") was a markdown-parsing artifact from a bolded sentence in build-notes.md that explicitly denied any such finding; confirmed moot and cleared. - Both pipeline bugs (stale deferred-findings not re-validated against the final diff; markdown-bold parsing producing a spurious NEEDS-HUMAN bullet) are recorded as Act candidates in SUMMARY.md §10.
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- issue_649: `deferred-findings.json` (stale from an internal mid-loop round) was folded verbatim into SUMMARY §6 without re-validating each item against the final patch.diff — two of the nine §6 bullets referenced code (a Cargo.lock bump, custodian/resolve.rs) that later build rounds had already removed. The bundle-assembly step should re-check deferred findings against the final diff before surfacing them at sign-off.
- issue_649: §6 extraction also mis-parsed a bolded sentence in build-notes.md ("**No NEEDS-HUMAN external dependency.**") into a spurious checklist bullet `- [ ] external dependency.**`, inverting a statement that explicitly denied any such item. The extractor should not split on markdown bold spans when pulling NEEDS-HUMAN items.
