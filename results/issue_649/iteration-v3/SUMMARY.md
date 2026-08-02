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
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: implement one shared, bounded, tear-safe segmented chunk-map resolver and route core and gateway whole-object/ranged reads through it.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance is falsifiable across byte identity, one-group bounded access, both retirement arms, parsed-index ordering, and typed fail-closed behavior. |
| C2 Reproduction (red pre-fix) | PASS | Both added targets compiled on base `6e7c255` and failed by assertion—17 core cases and 4 gateway cases—through the base-visible entries exercised at `crates/core/tests/segmented_map_resolution.rs:248` and `crates/server/tests/segmented_object_read.rs:162`. |
| C3 Change | NEEDS-HUMAN | Decide whether the unrelated lockfile-only `event-listener` 5.4.1→5.4.2 upgrade belongs—there is no manifest or brief dependency change, so accepting `Cargo.lock:1205` expands supply-chain review scope. |
| C4 Verification (red→green) | PASS | The same targets changed from 17/17 + 4/4 assertion failures to 17/17 + 4/4 passes, and scratch-local `cargo xtask ci` passed through real typos/docs/deny/conformance and the DST tear property at `crates/dst/tests/custodian.rs:1569`. |
| C5 Causal adequacy | PASS | The inline-only cause is replaced by the central resolver at `crates/core/src/metadata.rs:2328` and its read-path call at `crates/core/src/read.rs:513`, with no capability probe/symptom guard; all 56 diff mutants were caught (14) or unviable (42). |
| T1 Structure | PASS | One core resolver owns resolution while the custodian exposes only the thin live-root wrapper at `crates/custodian/src/resolve.rs:43`, preserving the core/traits dependency boundary. |
| T2 Shape | PASS | The 10 substantive files stay within the 15-file cap, while the other 10 files are the declared mechanical `read_object_chunks` callsite migration; the discriminators remain added integration targets such as `crates/core/tests/segmented_map_resolution.rs:1`. |
| T3 Runtime | PASS | Reader-controlled work is bounded: over-ceiling tables are refused before paging at `crates/core/src/metadata.rs:2185`, and permitted groups use only their exact prefix with a claim-bounded page limit at `crates/core/src/metadata.rs:2193`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether to trust the reported batch-review/contribution gates—the target lacks `scripts/review-branch` and `scripts/pdca`, so those wrappers could not be rerun; the independent affected-path check did cover all 20 paths plus all six closed-unmerged PRs and found only acknowledged #647 or unrelated work. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must assert that every recorded metadata `get` is the target inode key—the bounded-footprint oracle only forbids direct `seg:` gets at `crates/core/tests/segmented_map_resolution.rs:546`, so unrelated metadata reads would still pass. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether raw-seeded records plus the simulated retirement race are sufficient evidence for the intended production lifecycle—the slice has no segmented producer, so final fitness depends on future publication matching the seeded shape exercised at `crates/dst/tests/custodian.rs:1390`. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether the unrelated lockfile-only `event-listener` 5.4.1→5.4.2 upgrade belongs—there is no manifest or brief dependency change, so accepting `Cargo.lock:1205` expands supply-chain review scope.
- [ ] T4 Contribution — Decide whether to trust the reported batch-review/contribution gates—the target lacks `scripts/review-branch` and `scripts/pdca`, so those wrappers could not be rerun; the independent affected-path check did cover all 20 paths plus all six closed-unmerged PRs and found only acknowledged #647 or unrelated work.
- [ ] T5 Judgment — Rebuild must assert that every recorded metadata `get` is the target inode key—the bounded-footprint oracle only forbids direct `seg:` gets at `crates/core/tests/segmented_map_resolution.rs:546`, so unrelated metadata reads would still pass.
- [ ] Validation — fitness-to-purpose — Decide whether raw-seeded records plus the simulated retirement race are sufficient evidence for the intended production lifecycle—the slice has no segmented producer, so final fitness depends on future publication matching the seeded shape exercised at `crates/dst/tests/custodian.rs:1390`.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Decide whether to trust the reported batch-review/contribution gates—the target lacks `scripts/review-branch` and `scripts/pdca`, so those wrappers could not be rerun; the independent affected-path check did cover all 20 paths plus all six closed-unmerged PRs and found only acknowledged #647 or unrelated work.; T5 Judgment — Rebuild must assert that every recorded metadata `get` is the target inode key—the bounded-footprint oracle only forbids direct `seg:` gets at `crates/core/tests/segmented_map_resolution.rs:546`, so unrelated metadata reads would still pass.. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
