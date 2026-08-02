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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 56 mutants tested in 2m: 14 caught, 42 unviable

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

Reviewing issue #649: add one bounded shared resolver for segmented chunk maps and route whole-object, streaming, and ranged reads through it without torn generations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: byte identity, bounded metadata access, and both resolve-retry arms are separate obligations, matching the living read-path contract at docs/design/architecture/06-runtime-view.md:29. |
| C2 Reproduction (red pre-fix) | PASS | The base-visible oracle is discriminating: on dependency base 6e7c255 all 10 core and all 3 gateway tests compiled and failed assertions, including the whole-object control at crates/core/tests/segmented_map_resolution.rs:270 and cross-segment range at crates/server/tests/segmented_object_read.rs:166. |
| C3 Change | PASS | The accepted scope is preserved: one core resolver feeds the production read entries at crates/core/src/read.rs:495 and crates/server/src/lib.rs:344, while maintenance adoption remains explicitly deferred at crates/custodian/src/resolve.rs:23. |
| C4 Verification (red→green) | NEEDS-HUMAN | Maintainers must decide whether the unchanged-base security advisory must be cleared before acceptance — the 13 discriminators, workspace checks, docs/typos, conformance, statics, and 50-seed DST are green, but both advisory walls reject event-listener 5.4.1 at Cargo.lock:1204. |
| C5 Causal adequacy | PASS | The inline-only cause is removed through the single resolver at crates/core/src/metadata.rs:2328 rather than guarded by a capability probe, and the rerun caught all 14 viable diff mutants (42 others were unviable). |
| T1 Structure | PASS | Ownership and dependency direction remain narrow: core owns resolution, the custodian exposes only a thin trait-based wrapper at crates/custodian/src/resolve.rs:45, and the extra touched files are the brief-authorized call-site migration. |
| T2 Shape | PASS | The boundary shapes are total and typed: over-ceiling work is refused before paging at crates/core/src/metadata.rs:2185, anomalies are arbitrated centrally at crates/core/src/metadata.rs:2270, and no partial list is returned. |
| T3 Runtime | PASS | Runtime evidence covers byte identity, boundary-spanning range reads, bounded paging, retirement, corruption, and churn; the independently rerun 50-seed DST tear property at crates/dst/tests/custodian.rs:1457 passed. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the unavailable batch-review/contribution wrappers leave the reported one blocker unresolved — merged history for all 19 paths plus every open and closed-unmerged PR was independently intersected, but scripts/review-branch and scripts/pdca are absent, so the gate cannot be reproduced as required by AGENTS.md:206. |
| T5 Judgment | PASS | The tests now bind the prior judgment gaps directly: exact-ceiling paging and fixed root-read cost at crates/core/tests/segmented_map_resolution.rs:710, one-coordinate extent mismatches at crates/core/tests/segmented_map_resolution.rs:775, and whole-footprint metadata access at crates/core/tests/segmented_map_resolution.rs:623. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the bounded restart and per-object fail-closed policy is operationally fit for the intended durability/availability tradeoff; automated red→green evidence cannot make that product judgment. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Maintainers must decide whether the unchanged-base security advisory must be cleared before acceptance — the 13 discriminators, workspace checks, docs/typos, conformance, statics, and 50-seed DST are green, but both advisory walls reject event-listener 5.4.1 at Cargo.lock:1204. HUMAN DECISION: pre-existing base-tree advisory, unrelated to this patch's scope — accepted, to be tracked/fixed in a separate issue, not a blocker here.
- [ ] T4 Contribution — Decide whether the unavailable batch-review/contribution wrappers leave the reported one blocker unresolved — merged history for all 19 paths plus every open and closed-unmerged PR was independently intersected, but scripts/review-branch and scripts/pdca are absent, so the gate cannot be reproduced as required by AGENTS.md:206.
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether the bounded restart and per-object fail-closed policy is operationally fit for the intended durability/availability tradeoff; automated red→green evidence cannot make that product judgment.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] external dependency: RUSTSEC-2026-0221 (event-listener 5.4.1 in the base lockfile) — blocks the gating `C4-ci` (`cargo deny check`) on the BASE tree, independent of this patch; because `cargo_deny_check()` runs before `run_conformance()` / `run_statics()` / `run_dst()` (`xtask/src/main.rs:1562-1567`), a red deny also prevents the gate from reaching the DST tier this slice's verification posture relies on.**
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
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on two grounds, both to be fixed in the next attempt: 1. Unresolved BUG finding (T4 batch review, crates/core/src/metadata.rs:2248): segment values are decoded without enforcing MAX_VALUE_BYTES or a per-segment chunk-size ceiling, so a structurally-valid but oversized segment record can force unbounded allocation despite MAX_ROOT_SEGMENTS bounding the count. Must be fixed (add the size ceiling) or explicitly recorded-rejected with justification in review-rejected.md — it was left untriaged in this bundle. 2. Unscoped Cargo.lock change: event-listener bumped 5.4.1->5.4.2 with no manifest change and no brief justification, and it does not even clear the RUSTSEC-2026-0221 advisory it appears to be reacting to. Drop this lockfile edit from the patch; it is out of scope for this slice. Note: the pre-existing base-tree RUSTSEC-2026-0221 advisory itself (independent of any lockfile bump) is accepted as out of scope for this fix and will be tracked in a separate issue (see SUMMARY.md §10) — it is not part of the rejection.
- By / date: Eduard Ralph / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- File/track RUSTSEC-2026-0221 (event-listener advisory, pre-existing on base main) as its own issue so `cargo deny check` can go green independent of unrelated feature work; do not let unrelated bundles carry ad-hoc lockfile bumps to work around it.
