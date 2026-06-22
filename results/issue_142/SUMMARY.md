# Result — issue 142 / m3.4-gc-custodian

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: Demonstrable at C4-verify on base `origin/main` @ `40c3413`:
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2 — single line, no
- Scope (one logical fix) / out of scope: promote the GC stand-in into a **running GC custodian loop** driven

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

# Check review — issue 142 / m3.4-gc-custodian

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json` (build-notes.md withheld). Each Basis below is
re-derived independently.

## Grounding note

`$PDCA_TARGET` could not be read in this sandbox (env probes were blocked). The
wyrd checkout at `/home/eddie/wyrd/wyrd` (an environment working dir) matches the
brief's base line numbers **exactly** for the pre-existing #139/#140 seams
(`core/write.rs:251`, `core/metadata.rs:94`, `traits/src/lib.rs:100/108/180`), so
I grounded all `core`/`traits`/`metadata` claims against it. It **predates the
`custodian` crate (#141)** — that crate is absent — so `custodian`-internal
symbols (`reconcile_step`, `FencedZone`, `DurabilityTelemetry`, `gather_prometheus`,
`Custodian::elect`) are grounded against `patch.diff` plus the green `C4-ci` gate
(workspace build+test), not the source tree.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | brief.md carries testable BINDING criteria 1–5, the four invariants, scope/out-of-scope, and an explicit mixed verification posture; success criterion is demonstrable on base `40c3413`. Spec is unambiguous and self-consistent. |
| C2 — C2 Reproduction (red pre-fix) | PASS | No C2 gate configured, but `C4-verify` (check-gates.json: "run-verify.sh: PASS — red without the fix, green with it") evidences a pre-fix red. Net-new/criterion-absence red is pre-agreed with the human (brief.md:127-134), not a surprise. |
| C3 — C3 Change | PASS | Patch is coherent and grounded: `gc::referenced_fragments` maps `chunk.placement[i]→FragmentId{chunk.id, i}` correctly per `core/metadata.rs:84-94`; `expired_pending_chunks` mirrors `<= now_millis` of `write.rs:251-271`; only `InodeState::Committed` (`metadata.rs:44-49`) protects bytes; `inode:` scan prefix matches `metadata.rs:28`; `WriteBatch`/`FragmentId`/`DServerId`/`ChunkId` usage matches `traits/src/lib.rs:25-53,203-256`. One caveat (not failing): the new `gc.rs` doc cites `write.rs:332` / `:330-331` for the stand-in + deferred-reclaim note, but on target source these are at `write.rs:251` / `:249-250` (the brief's own citation) — ~81-line citation drift. |
| C4 — C4 Verification (red→green) | PASS | `C4-ci` = pass (fmt/clippy/build/test/deny/conformance), confirming the crate-wide compile incl. the `reconcile_step` signature change and its updated `skeleton.rs` callers; `C4-verify` = pass (red→green). Criteria 2 & 3 are genuinely flippable: tests `never_reclaims_a_referenced_fragment` (stale orphan record on a referenced frag, grace=0 — only the ref-check protects it) and `honours_the_reader_safe_grace_window` (within vs. past window) each fire when the corresponding gate is negated. |
| C5 — C5 Causal adequacy | PASS | Root cause = `sweep_expired_leases` reclaims `pending:` ledger entries but explicitly defers fragment-byte reclaim (`write.rs:249-250`). The patch addresses it directly: `gc::reconcile` reclaims both input classes via `ChunkStore::delete_fragment`, gated by the committed-reference set (silent-corruption invariant) and the grace window (reader-safety); crash-mid-pass leaves collectable garbage (idempotent `delete_fragment`, `traits:102-108`; metadata cleanup committed last). Residual judgment calls routed to §6.2/§6.3. |
| T1 — T1 Structure | PASS | Test lives at the briefed path `crates/custodian/tests/gc.rs`, drives the **real** `reconcile_step` control point (not a parallel entry), uses real `core`/`traits` types over in-memory trait stores. |
| T2 — T2 Shape | PASS | Assertions are behavioural (byte present/absent via `get_fragment`, `Reconciled` outcome, exported metric strings), not implementation-coupled; each invariant leg is independently flippable. |
| T3 — T3 Runtime | PASS | `#[tokio::test]`, fully in-process, deterministic logical-millis clock (no wall-clock/sleep); telemetry asserted in-process via `gather_prometheus`. `C4-ci` confirms these run green. In-process Option-A green is the declared posture (brief.md:140-150). |
| T4 — T4 Contribution | PASS | Anti-#141 guard satisfied: `gc::reconcile` is `pub(crate)` (patch gc.rs:142) — the **only** way to reach it is `reconcile_step` dispatching on `Some(gc)` (reconciliation.rs diff). One production entry, no parallel test-only function. (Deferral noted §6.3: `reconcile_step` itself has no production driver yet — pre-agreed Option A.) |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle is reviewer + human sign-off. The net-new born-at-tier red and the in-process Option-A green are explicitly framed by the brief as a "pre-agreed sign-off item, not a NEEDS-HUMAN surprise" (brief.md:143-145) — surfacing it for the human to clear, cf. #141's T4 FAIL. → §6.1 |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always human at sign-off; fitness of the slice to proposal-0005 GC intent. → §6.2 |

## §6 — items the human must clear

**§6.1 (T5 Judgment) — accept the net-new / in-process posture.** This slice ships
new GC code WITH its first test (no pre-existing failing assertion to flip), and its
"green" is `reconcile_step` exercised in-process over trait stores, not a deployed
sweep. The brief pre-agreed both with the human (brief.md:127-150). Human confirms
the born-at-tier red and Option-A in-process green are acceptable here and that the
flippable demonstrations (criteria 2 & 3) adequately stand in for a pre-existing red.

**§6.2 (Validation) — fitness-to-purpose.** Does the GC loop as built satisfy
proposal-0005 GC intent for slice 4? Confirm the four binding legs (two-input
reclaim, never-reclaim-referenced, grace-honoured, durability-plane emission) match
0005 §GC / Q3 / graduation invariants, and that deferring the deployed custodian
process (Option A) and the other three loops is the agreed shape.

**§6.3 (causal-adequacy caveats for human attention — within declared scope).**
- *Grace-window derivation.* `grace_window_millis` is a **caller-supplied `GcContext`
  field**, not derived inside GC from reader version-hold / lease semantics. The brief
  carves the numeric *length*/derivation out of scope (`0005:585-586`) and binds only
  that the window is *honoured* — which the test demonstrates — but no linkage to actual
  reader-hold semantics is implemented or exercised. Confirm "parameterized, derivation
  deferred to the runtime" is acceptable for this slice.
- *Non-transactional reference snapshot.* `referenced` is scanned once at pass start,
  then `delete_fragment` runs per-fragment afterward (a TOCTOU window). Safety rests on
  the unstated invariant that fresh writes never reference an expired-pending or
  past-grace-orphan fragment id (new writes allocate fresh chunk ids). No
  concurrent-commit-during-GC test exists (single-threaded in-process). Defensible under
  Option A (no live process), but the human should confirm the argument holds for the
  later live-process slice.
- *Citation drift (C3).* New `gc.rs` doc cites `write.rs:332` / `:330-331`; target source
  has the stand-in + deferred-reclaim note at `write.rs:251` / `:249-250` (the brief's
  own numbers). Cosmetic, but the brief requires accurate `path:line` on base — worth a
  one-line correction.
- *Stale orphan key left for a re-referenced fragment.* When a stale `orphan:` grace
  record points at a now-referenced fragment, the ref-check correctly preserves the bytes
  but the stale ledger key is never retired (cleanup only fires on reclaim). Harmless
  (ref-check re-protects each pass), noted for completeness.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 — T5 Judgment — Oracle is reviewer + human sign-off. The net-new born-at-tier red and the in-process Option-A green are explicitly framed by the brief as a "pre-agreed sign-off item, not a NEEDS-HUMAN surprise" (brief.md:143-145) — surfacing it for the human to clear, cf. #141's T4 FAIL. → §6.1
- [x] V — Validation — fitness-to-purpose — Always human at sign-off; fitness of the slice to proposal-0005 GC intent. → §6.2

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
