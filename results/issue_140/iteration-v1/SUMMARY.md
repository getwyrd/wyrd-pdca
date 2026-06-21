# Result — issue 140 / m3.2-chunkstore-list-delete

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: Add the two `ChunkStore` affordances M1/M2 deliberately left out — a store
- Success criterion: With the methods present on `ChunkStore`, a store can be
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2)
- Scope (one logical fix) / out of scope: add `list_fragments(&self) -> Result<Vec<FragmentId>>` and

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 140 / m3.2-chunkstore-list-delete

Advisory, artifact-only, decorrelated from the builder (build-notes.md withheld).
Citations re-derived against the read-only target at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd`), which is the **pre-fix** tree (`origin/main`): the
`ChunkStore` trait there carries only `put`/`get`/`health`
(`crates/traits/src/lib.rs:73-83`) and the proto service only three rpcs
(`crates/proto/proto/wyrd/v0/chunk.proto:60-64`). This independently confirms the
NET-NEW ("criterion-absence") posture before the patch is applied.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | brief.md:15-23 states a falsifiable success criterion (`list_fragments` returns exactly the held `FragmentId`s; after `delete_fragment(id)`, `get_fragment(id)` is `Ok(None)`, siblings unaffected) with BINDING signatures — Check-observable in-process/local-tonic. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Re-derived on target: methods absent from the trait (`crates/traits/src/lib.rs:73-83`) and proto (`chunk.proto:60-64`), so the new `tests/list_delete.rs` (patch.diff:290+) cannot compile pre-fix — genuine criterion-absence red; corroborated by check-gates C4-verify "red without the fix, green with it". |
| C3 — C3 Change | PASS | Coherent additive change: trait (patch.diff:570-588), fs backend walk+idempotent delete (patch.diff:18-65, helpers 78-89), proto messages+rpcs (patch.diff:496-526), grpc client (patch.diff:183-207) and D-server service (patch.diff:259-285), fanout union/route (patch.diff:224-235); anchors match target verbatim. |
| C4 — C4 Verification (red→green) | PASS | Gating C4-ci PASS (fmt/clippy/build/test/deny/conformance) and C4-verify PASS per check-gates.json:33-49; the build going green proves every `ChunkStore` impl (incl. forced test-fake updates) compiles and the in-process + local-tonic tests pass. Cannot re-run artifact-only — relies on the gate, which is exactly this oracle. |
| C5 — C5 Causal adequacy | PASS | Root cause is uncontested (the affordances simply do not exist — motivation, brief.md:57-64); the patch adds exactly those methods across trait + wire + both backends, and the test proves the seam load-bearing (bytes present *before* delete, `Ok(None)` *after* — tests/list_delete.rs, patch.diff:395-407). |
| T1 — T1 Structure | PASS | Test in the briefed location `crates/chunkstore-grpc/tests/list_delete.rs` (new, patch.diff:290-294), mirroring `round_trip.rs`'s `connected()` harness (target tests/round_trip.rs:34-60); supplementary fs coverage in `chunkstore-fs/tests/conformance.rs` (patch.diff:99-158). |
| T2 — T2 Shape | PASS | Assertions match the criterion precisely: empty-store-empty, set-equality of the listing, `get_fragment` some-before/none-after delete, siblings unaffected, idempotent re-delete (patch.diff:370-427); fs test additionally pins strict name parsing — `.tmp`/foreign entries skipped (patch.diff:138-158). |
| T3 — T3 Runtime | PASS | Exercised over both an in-process store and a real local-tonic round-trip (HTTP/2 + prost), `tokio::test(multi_thread)` (patch.diff:429-441); runs as part of the green C4-ci test suite. |
| T4 — T4 Contribution | PASS | Non-vacuous: set-equality plus `Ok(None)`-after-delete plus sibling-present catch regressions in both directions; the foreign/`.tmp` case would fail if the walk parsed names loosely (patch.diff:152-156), pinning the `parse_*` helpers (target fragment_path inverse, lib.rs:148-151). |
| T5 — T5 Judgment | PASS | Coverage is well-judged for the slice: empty store, multi-chunk, non-zero EC index, idempotency, and crash-residue/foreign entries; large-store streaming and concurrency are explicitly out of scope per 0005/brief.md:89-94 — no over- or under-testing evident. (Advisory; human confirms at sign-off.) |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human: whether this slice genuinely serves the M3 maintenance plane (scrub diff / GC reclaim) as proposal 0005 intends is a fitness judgement reserved for the human at sign-off (check-gates.json:104-112). |

## §6 — Items the human must clear

1. **Validation (fitness-to-purpose).** Confirm the two added affordances, as
   shaped here (`Vec` listing with unspecified order; idempotent
   `delete_fragment`), are the right primitives for the later scrub/GC custodian
   slices (#141+) per accepted proposal 0005 — and that deferring the
   real-network docker-compose variant (`tier2_integration.rs`) to off-Check
   Tier-2 CI is acceptable evidence. This is the one always-human gate.

## Reviewer notes (advisory, non-gating)

- **No new dependency.** The patch reuses crates already present on the wire/test
  surface (tonic/prost, tempfile, tokio-stream) — so the INTEGRATION §4 / ADR-0003
  new-dependency NEEDS-HUMAN does *not* trigger; the gating `deny` step in C4-ci is
  green, consistent with this.
- **Additive proto evolution verified.** Only new messages
  (`FragmentList*`/`FragmentDelete*`) and two new rpcs were added; no existing
  field or rpc was repurposed (patch.diff:496-526 vs target chunk.proto:25-64) —
  ADR-0002 wire rule honoured.
- **Trait ripple is necessary, not scope creep.** Adding two methods to the
  `ChunkStore` trait forces every implementor — including the in-test fakes in
  `core`/`dst`/`server` (patch.diff:446-565) — to provide them, or those crates
  would not compile. The green C4-ci build confirms the set of touched
  implementors is complete.
- **Open question resolved as briefed.** `DeleteFragment` on a missing id is
  idempotent `Ok(())` at every layer (trait doc patch.diff:582-588, fs
  patch.diff:57-64, proto patch.diff:511-513), matching brief.md:93-94's
  "pick idempotent unless a gate disagrees".


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] V — Validation — fitness-to-purpose — Always-human: whether this slice genuinely serves the M3 maintenance plane (scrub diff / GC reclaim) as proposal 0005 intends is a fitness judgement reserved for the human at sign-off (check-gates.json:104-112).

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
- Iteration delta (if iterating): PR #186 conflicts with M3.1 (#185, now merged) on chunkstore-fs/src/lib.rs + chunkstore-grpc/src/fanout.rs — it was built off pre-M3.1 main. Re-Do off current main (which now carries the placement record) for a clean rebuild.
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
