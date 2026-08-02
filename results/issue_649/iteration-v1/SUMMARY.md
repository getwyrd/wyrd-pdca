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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 56 mutants tested in 2m: 3 missed, 11 caught, 42 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #649: add one bounded, fail-closed resolver for segmented chunk maps and route core and gateway read paths through it.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable across byte identity, bounded group access, and retirement-versus-corruption behavior, with the deployment ceiling grounded at `crates/core/src/metadata.rs:322`. |
| C2 Reproduction (red pre-fix) | PASS | With production resolver/read changes stashed, all 11 core and all 4 gateway discriminator cases fail by the base blanket refusal rather than compile failure; the discriminating typed-error check is at `crates/core/tests/segmented_map_resolution.rs:45`. |
| C3 Change | PASS | The change stays on the specified resolver/read/custodian-seam surfaces and updates the prescribed living read-path description; the shared entry is at `crates/core/src/metadata.rs:2275` and the architecture contract at `docs/design/architecture/06-runtime-view.md:29`. |
| C4 Verification (red→green) | FAIL | The red→green targets and 50-seed DST are green, but the required full gate remains red on RUSTSEC-2026-0221 in unchanged `event-listener` 5.4.1, so verification is not gate-clean (`Cargo.lock:1204`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must bind the exact-ceiling and one-coordinate-only extent cases—surviving `>`→`>=` and `||`→`&&` mutants show regressions at `crates/core/src/metadata.rs:2142` and `crates/core/src/metadata.rs:2239` would pass the suite. |
| T1 Structure | PASS | One metadata resolver owns ordering and retirement arbitration, while core and gateway callers delegate to it at `crates/core/src/read.rs:513` and `crates/server/src/lib.rs:354`. |
| T2 Shape | PASS | The API keeps root generation and resolved chunks coupled across restarts, avoiding representation probes and stale framing through `CurrentChunkMap`/`LiveChunkMap` at `crates/core/src/metadata.rs:2301` and `crates/core/src/metadata.rs:2345`. |
| T3 Runtime | PASS | Whole and cross-segment ranged reads pass through public entry points, and the 50-seed simulator exercises the retirement race at `crates/server/tests/segmented_object_read.rs:166` and `crates/dst/tests/custodian.rs:1550`. |
| T4 Contribution | NEEDS-HUMAN | The decision owed is whether the seven asserted batch-review blockers and closed/rejected prior art are settled—`scripts/review-branch` and closed-PR refs are absent from this artifact sandbox, so only merged history by affected path was independently checked. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must assert the recorded metadata-get keys as well as scan prefixes—the double records `gets` at `crates/core/tests/segmented_map_resolution.rs:202`, but the bounded-access oracle inspects only prefixes at `crates/core/tests/segmented_map_resolution.rs:286`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether raw seeded roots plus redb/test-double/DST evidence are sufficient production evidence for C-1 before a segmented-map producer exists; a wrong decision risks a quiet ownership under-approximation (`docs/design/architecture/06-runtime-view.md:29`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must bind the exact-ceiling and one-coordinate-only extent cases—surviving `>`→`>=` and `
- [ ] T4 Contribution — The decision owed is whether the seven asserted batch-review blockers and closed/rejected prior art are settled—`scripts/review-branch` and closed-PR refs are absent from this artifact sandbox, so only merged history by affected path was independently checked.
- [ ] T5 Judgment — Rebuild must assert the recorded metadata-get keys as well as scan prefixes—the double records `gets` at `crates/core/tests/segmented_map_resolution.rs:202`, but the bounded-access oracle inspects only prefixes at `crates/core/tests/segmented_map_resolution.rs:286`.
- [ ] Validation — fitness-to-purpose — Maintainers must decide whether raw seeded roots plus redb/test-double/DST evidence are sufficient production evidence for C-1 before a segmented-map producer exists; a wrong decision risks a quiet ownership under-approximation (`docs/design/architecture/06-runtime-view.md:29`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must bind the exact-ceiling and one-coordinate-only extent cases—surviving `>`→`>=` and `; T4 Contribution — The decision owed is whether the seven asserted batch-review blockers and closed/rejected prior art are settled—`scripts/review-branch` and closed-PR refs are absent from this artifact sandbox, so only merged history by affected path was independently checked.; T5 Judgment — Rebuild must assert the recorded metadata-get keys as well as scan prefixes—the double records `gets` at `crates/core/tests/segmented_map_resolution.rs:202`, but the bounded-access oracle inspects only prefixes at `crates/core/tests/segmented_map_resolution.rs:286`.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_.
- By / date: auto-iterate / 2026-07-31

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
