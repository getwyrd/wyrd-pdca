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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 56 mutants tested in 2m: 14 caught, 42 unviable

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

Task under review: add one bounded segmented chunk-map resolver and route core and gateway whole/ranged reads through it without torn-generation answers.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance is explicit about raw-seeded segmented reads, bounded resolution, retirement arbitration, and the caller-first/no-producer slice boundary (`crates/custodian/src/resolve.rs:21`). |
| C2 Reproduction (red pre-fix) | PASS | On clean #648 with only the two added targets, all 16 core and all 4 gateway tests compiled and failed by assertion at the base-visible read entries (`crates/core/tests/segmented_map_resolution.rs:247`, `crates/server/tests/segmented_object_read.rs:161`). |
| C3 Change | PASS | The change stays on the resolver/read-path surfaces, and the otherwise lock-only `event-listener` 5.4.2 update is necessary to clear the independently reproduced RUSTSEC-2026-0221 failure (`Cargo.lock:1205`). |
| C4 Verification (red→green) | PASS | The independent result is 0/20→20/20 plus green typos, docs render, fmt, clippy, build, workspace tests, three deny walls, conformance, statics, orchestrator scan, and 50-seed DST; the default advisory-db lock was host-read-only, so the real deny tool was rerun with reviewer-owned `CARGO_HOME` (`crates/dst/tests/custodian.rs:1568`). |
| C5 Causal adequacy | PASS | The inline-only cause is replaced by the shared bounded resolver, not a capability probe or downstream guard, and all 14 viable in-diff mutants were killed (`crates/core/src/metadata.rs:2296`, `crates/core/src/read.rs:513`). |
| T1 Structure | PASS | Resolution is single-sourced in core, with read/gateway callers and only a thin maintenance-facing wrapper rather than a second algorithm (`crates/core/src/metadata.rs:2296`, `crates/custodian/src/resolve.rs:43`). |
| T2 Shape | PASS | The substantive files follow the declared resolver/read/test/docs slice, while existing benches/tests contain the separately allowed mechanical `read_object_chunks` migration (`crates/core/src/read.rs:69`). |
| T3 Runtime | PASS | Real redb/fs tests cover whole and boundary-spanning ranged bytes, exact and over ceilings, isolated access footprint, fail-closed anomalies, live-root restart, and parsed-index ordering (`crates/core/tests/segmented_map_resolution.rs:410`, `crates/server/tests/segmented_object_read.rs:184`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the four asserted batch-review blockers are settled—the red wrapper row cannot be reproduced because `scripts/review-branch` and `scripts/pdca` are absent; merged history and all ten closed-unmerged PRs were independently intersected by every affected path. |
| T5 Judgment | PASS | The strengthened exact-ceiling, one-coordinate mismatch, full access-footprint, gateway-generation-framing, mutation, and DST evidence exercises the safety claims rather than a proxy (`crates/core/tests/segmented_map_resolution.rs:528`, `crates/server/tests/segmented_object_read.rs:298`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve staged fitness only if ordering prevents any segmented-map producer before #650/#651 route the remaining maintenance callers—otherwise reads accept a shape that GC still refuses store-wide (`crates/custodian/src/gc.rs:267`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the four asserted batch-review blockers are settled—the red wrapper row cannot be reproduced because `scripts/review-branch` and `scripts/pdca` are absent; merged history and all ten closed-unmerged PRs were independently intersected by every affected path.
- [ ] Validation — fitness-to-purpose — Approve staged fitness only if ordering prevents any segmented-map producer before #650/#651 route the remaining maintenance callers—otherwise reads accept a shape that GC still refuses store-wide (`crates/custodian/src/gc.rs:267`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the four asserted batch-review blockers are settled—the red wrapper row cannot be reproduced because `scripts/review-branch` and `scripts/pdca` are absent; merged history and all ten closed-unmerged PRs were independently intersected by every affected path.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
