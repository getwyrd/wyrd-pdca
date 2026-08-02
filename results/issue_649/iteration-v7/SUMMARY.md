# Result — issue 649 / shared-segmented-map-resolver-and-read-paths

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  After #648 a `chunk_map` can be `Segmented`, but **nothing can read one**. Every
  consumer still takes the inline list off the record (`crates/core/src/read.rs:92`,
  `crates/server/src/lib.rs:360`,`:446` on `origin/main`), so a segmented object is opaque to the
  whole-object read, the ranged read and every maintenance pass, and there is no shared
  resolution call at all (`git -C ../wyrd grep -n "resolve_chunk_map\|MapResolution" origin/main
  -- crates/` is empty) — so the way each consumer learns an object's chunks is about to be
  re-derived once per consumer, which 0016 decision 7(e) forbids.
- Success criterion:
  The two added test targets pass and bind the issue's acceptance, all
  through **base-visible** entry points (`wyrd_core::read::{read_object, read_path}` and
  `wyrd_gateway_core::ObjectGateway`) over directly-seeded raw `seg:` records:
  1. **Byte-identical reads.** A segmented object seeded as raw `seg:` records + a segmented root
     (never via a committer) reads back — whole-object and over a range that **spans a segment
     boundary** — byte-identical to the flat equivalent of the same payload.
  2. **The resolver is bounded.** Reading one object touches the root plus **only** the range
     `seg:<nonce>:<epoch>:` — a second group seeded in the store (different nonce, and the same
     nonce at a different epoch) is **never read**, asserted on an instrumented store double that
     records every key/prefix requested; and a root claiming more segments than the ceiling allows
     is **refused before any range read is performed**, asserted on the same double.
  3. **The resolver is total and never tears.** Both arms of the resolve-retry rule, asserted on
     the interleaving: root **moved on** (or gone) + a segment absent ⇒ the resolution is dropped
     as a concurrently-retired generation (the read restarts / answers no-such-key), never a torn
     half-map and never "this object owns no bytes"; root **unchanged** + a segment absent,
     undecodable, unnamed, or whose extents disagree with the root's table ⇒ **fail closed** with
     a typed error. Chunks are ordered by the **parsed index**, proved against a deliberately
     shuffling store double. A read driven from a stale (superseded) snapshot resolves against the
     **live** root.

  Also in the issue's acceptance and shipped here, but **not** a Check discriminator: the DST
  resolver-tear property, run by `cargo xtask ci` / `dst` (see *Verification posture*).
- Repo + branch target:
  getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- Scope (one logical fix) / out of scope:
  **the one shared resolver**, and the **read paths** routed through it — nothing that
  reclaims, repairs or publishes. `crates/core/src/metadata.rs` — the resolution result type, the
  bounded per-group range read with its ceiling check, the resolve-retry arbiter, the
  resolve-against-the-live-root entries, and the `ChunkMapError` variants they raise.
  `crates/custodian/src/resolve.rs` (new) + `crates/custodian/src/lib.rs` — the thin
  custodian-facing wrapper and the shared root-classification arm every `scan("inode:")` loop will
  use, **with its unit coverage in this slice**; its pass consumers are #650/#651.
  `crates/core/src/read.rs` — the byte-assembly entry takes an already-resolved chunk list; the
  placement-aware entries resolve first. `crates/server/src/lib.rs` — gateway whole-object /
  streaming and ranged reads route through the resolver; **move #647's co-located `#[cfg(test)]
  mod` gateway tests into the added integration file**, since co-located tests cannot earn a
  per-fix red here. **Caller-first:** every production symbol introduced here has a caller **in
  this slice** — the resolver is called by `read.rs` and the gateway, the wrapper by its own unit
  tests and (next wave) #650; this slice lands no behaviour flip and no producer of segmented
  maps. **Out of scope:** GC / scrub / `ReferenceSet` (#650); restore, reconstruction, backfill,
  rebalance, `desired_state`, `repoint_chunk` and the record-ceiling helpers (#651); the chunk-id
  floor (#652); the committer, fence, rollback and resume (#653); any new/edited ADR / spec /
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 59 mutants tested in 2m: 17 caught, 42 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #649: add one bounded segmented chunk-map resolver and route core and gateway reads through it without torn-generation answers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance is falsifiable across raw-segment whole/ranged reads, bounded access, both retry arms, ordering, dependencies, and an explicit slice boundary. |
| C2 Reproduction (red pre-fix) | PASS | On the clean #648 base with only the two added targets present, all 12 core and all 3 gateway cases failed by assertion on the old unsupported path (`crates/core/tests/segmented_map_resolution.rs:270`; `crates/server/tests/segmented_object_read.rs:166`). |
| C3 Change | NEEDS-HUMAN | Decide whether approximately 1,756 nonblank/noncomment additions after excluding the ten declared mechanical call-site migrations merit the named resolver/read-path split—the brief budgets about 1,500, so reviewability and scope approval are owed. |
| C4 Verification (red→green) | PASS | The same 12+3 tests turned green with the patch, and fmt, clippy, build, workspace tests, docs, typos, conformance, statics, mutation, and 50-seed DST passed; `cargo deny` only reproduced the brief's settled unchanged-base RUSTSEC-2026-0221 deferral (`crates/dst/tests/custodian.rs:1564`). |
| C5 Causal adequacy | PASS | The change removes the inline-only cause through the single resolver and resolved-chunk boundary rather than adding a capability probe or symptom guard (`crates/core/src/metadata.rs:2407`; `crates/core/src/read.rs:69`). |
| T1 Structure | PASS | Nine substantive files hold the resolver, wrapper, read wiring, tests, and living-architecture update; the other ten files are the brief-declared mechanical `read_object_from` migration (`crates/custodian/src/lib.rs:30`; `crates/core/src/read.rs:69`). |
| T2 Shape | FAIL | Public API guidance still says this slice has no resolver and that `as_flat` remains the sanctioned read, which can direct callers around the new single-source contract (`crates/core/src/metadata.rs:262`; `crates/core/src/metadata.rs:952`; `crates/core/src/metadata.rs:965`). |
| T3 Runtime | PASS | Workspace runtime tests and the full madsim campaign passed, including the seeded retire-mid-resolve property that requires one whole live-generation answer (`crates/dst/tests/custodian.rs:1457`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the four reported batch-review blockers are settled—the target has no `scripts/review-branch`, so that gate cannot be rerun; the independent affected-path audit covered all 19 paths and ten closed-unmerged PRs, finding only acknowledged #647 and unrelated #336 overlap. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must reconcile the stale `ChunkMap` documentation with the resolver now shipped, because callers following it can preserve the representation split this change is meant to eliminate (`crates/core/src/metadata.rs:262`; `crates/core/src/metadata.rs:952`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the bounded/retrying resolver semantics are operationally fit for production reads—the automated red→green and 50-seed simulation establish behavior, not final production fitness. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether approximately 1,756 nonblank/noncomment additions after excluding the ten declared mechanical call-site migrations merit the named resolver/read-path split—the brief budgets about 1,500, so reviewability and scope approval are owed.
- [ ] T4 Contribution — Decide whether the four reported batch-review blockers are settled—the target has no `scripts/review-branch`, so that gate cannot be rerun; the independent affected-path audit covered all 19 paths and ten closed-unmerged PRs, finding only acknowledged #647 and unrelated #336 overlap.
- [ ] T5 Judgment — Rebuild must reconcile the stale `ChunkMap` documentation with the resolver now shipped, because callers following it can preserve the representation split this change is meant to eliminate (`crates/core/src/metadata.rs:262`; `crates/core/src/metadata.rs:952`).
- [ ] Validation — fitness-to-purpose — Decide whether the bounded/retrying resolver semantics are operationally fit for production reads—the automated red→green and 50-seed simulation establish behavior, not final production fitness.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] external dependency: RUSTSEC-2026-0221 (event-listener 5.4.1 in the BASE lockfile) — blocks the gating `C4-ci` (`cargo deny check`) on the base tree independently of this patch; already adjudicated out-of-scope at the iteration-6 sign-off (SUMMARY §10), repeated here only because the gate row is still red and C6 will ask again.**
- [ ] C3 Change — Decide whether the unrelated lockfile-only `event-listener` 5.4.1→5.4.2 upgrade belongs—there is no manifest or brief dependency change, so accepting `Cargo.lock:1205` expands supply-chain review scope.
- [ ] C1 Spec — Decide whether universal consumer routing is required in this slice or only at the six-slice endpoint—the living architecture claims it now at `docs/design/architecture/06-runtime-view.md:29`, while the maintenance wrapper explicitly defers its callers to #650/#651 at `crates/custodian/src/resolve.rs:21`; this determines whether the current-state documentation and safety invariant are premature.
- [ ] C3 Change — Decide whether to accept or re-slice the review surface—approximately 1,925 nonblank, noncomment additions remain after excluding the declared call-site migrations, materially above the brief's ~1,500-line ceiling, led by `crates/core/tests/segmented_map_resolution.rs:1` and `crates/core/src/metadata.rs:2040`.

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
- Iteration delta (if iterating): Rejected on the merits of the T4 batch-review BUG findings (metadata.rs:2250-2251, review-batch.md), confirmed by direct code review: read_group_range's row-count ceiling (accounted > MAX_ROOT_SEGMENTS) bounds how many seg: rows a page can hold, but scan_page returns each page's values fully materialized before the per-row MAX_VALUE_BYTES check ever runs (the doc comment at metadata.rs itself concedes "scan_page is bounded in rows, never in bytes"). So a single retired/corrupted/ oversized seg: row is pulled entirely into memory before SegmentValueOverCeiling can fire, and MAX_ROOT_SEGMENTS oversized rows in one page multiply that — the resolver's claimed bound of MAX_ROOT_SEGMENTS x MAX_VALUE_BYTES is not actually enforced. This directly undercuts the brief's own acceptance criterion 2 ("the resolver is bounded"), and unlike the timeout/deadline findings in this same bundle, it was never argued through to an explicit, precedent-backed rejection — it's just an open, unchecked finding. The companion TEST-GAP finding (segmented_map_resolution.rs:553) independently confirms the current test can't actually prove values aren't over-materialized, since its store double delegates to the real backend before truncating. This is a plan-level question, not a build-level bug-fix: the next attempt should resolve whether the byte ceiling on a segment group's range read is the resolver's responsibility (requiring a genuinely page-and-byte-bounded read primitive, e.g. checking/capping value size as each row streams in rather than after scan_page returns the page) or the store/MetadataStore seam's responsibility (in which case that must be argued through and recorded with the same rigor as the standing timeout/deadline rejection in review-rejected.md, not left as an unresolved review finding). iterate-do was rejected in favor of iterate-plan because 7 prior build iterations patching symptoms within the same resolver shape have not converged — the diff has been growing rather than shrinking and this same boundedness gap survived multiple passes — indicating the slice/approach itself, not just the code, needs reconsideration.
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
