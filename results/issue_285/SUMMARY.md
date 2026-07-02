# Result — issue 285 / validate-ec-scheme-at-read-boundary

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: A corrupted or malformed inode record carrying `EcScheme::ReedSolomon { k: 0, .. }`
- Success criterion: Calling `erasure::reconstruct` with `k == 0` (and, at the read path, a
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Malformed EC-scheme parameters read back from inode metadata reach shard

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
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

# Check review — issue 285 / validate-ec-scheme-at-read-boundary

**Task under review:** A corrupted/tampered inode carrying an unsupported `EcScheme::ReedSolomon`
(canonically `k == 0`, also `m == 0` / unsupported `k+m`) is trusted by the read/reconstruct
layers: `k == 0` sails past the `available.len() < k` guard (`0 < 0`) and `reconstruct` panics
indexing an empty shard list. Fix: validate the stored scheme at the erasure API boundary and
the read boundary so untrusted EC params fail as a typed `Err`, never a panic or OOB index.

_Grounding: `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`, read-only, already carries the
applied patch and matches `patch.diff` (not stale). Predicate semantics verified directly from
`reed-solomon-simd-3.1.0` source. `cargo` could not be re-run under the review sandbox; C4 rests
on the recorded gate re-runs (C4-ci pass/gating, C4-verify pass) plus direct source verification._

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is coherent and binding ("invalid EC params from stored metadata yield a clean `Err`, never a panic"), grounded in the read path's own "never bad data" contract (`crates/core/src/read.rs:8-16`) and the CLI's `k>=1` rule (`cli.rs:110`); scope excludes on-disk format / CLI parse. No ambiguity that gates. |
| C2 Reproduction (red pre-fix) | PASS | Panic mechanism re-derived: with `k==0`, `available.len() < k` is `0 < 0` (false), so `reconstruct` falls to `available[0]` on an empty slice (`erasure.rs:153`) → panic; matches brief repro `reconstruct(0,1,0,&[])`. The `m==0` case pre-fix returns bytes for an illegal scheme (all `k` shards present, no guard). Both are genuine red states the new tests convert to green. |
| C3 Change | PASS | Adds `ErasureError::InvalidScheme` (`erasure.rs:42`), `pub fn supported` (`erasure.rs:120`), scheme guard in `reconstruct` (`erasure.rs:144`), `ReadError::InvalidEcScheme` (`read.rs:353`) and the read-boundary guard (`read.rs:192`). Minimal, targeted to the two boundaries the invariant names; no drive-by edits. |
| C4 Verification (red→green) | PASS | Gate C4-ci pass (gating) + C4-verify pass. Independently confirmed the predicate: `supported`→`ReedSolomonDecoder::supports`→`use_high_rate` returns Err for `original==0 \|\| recovery==0` (`reed-solomon-simd-3.1.0/src/rate/rate_default.rs:29`; crate test table `(0,1)`/`(1,0)`→err, lines 446-447). So `supported(0,m)`/`supported(k,0)` are `false` → typed `Err` before any indexing. (cargo not re-runnable in review sandbox; rests on gate + source.) |
| C5 Causal adequacy | PASS | Fix validates untrusted params at the API/read boundary — it removes the cause (unvalidated scheme reaching shard indexing/fan-out), not a capability probe or runtime guard over a present optional capability, so the C5 symptom-guard smell-test does NOT fire. Uses the SAME predicate the coder applies (`erasure::supported`), so read-side and coder-side agree; the earlier narrow-`k==0` scope objection (iteration-1) is resolved by broadening to all coder-unsupported schemes. Root cause is not contested. |
| T1 Structure | PASS | Erasure tests extend the existing `mod tests` (`erasure.rs:~300-336`); read tests add a new `#[cfg(test)] mod tests` with an `EmptyChunks` `PlacementChunkStore` — placed in the modules that own the changed code. |
| T2 Shape | PASS | Assertions bind the observable contract: `matches!(err, InvalidScheme{..})` / `InvalidEcScheme{chunk_id,k,m}` and `corrupt.is_empty()` (rejection is validation, not a corruption finding) — not incidental internals. `ChunkRef` fields (`id/scheme/len/placement`) match `metadata.rs:84`. |
| T3 Runtime | PASS | Read-path guard sits before `let n=(k+m)` and any `get_fragment_at`, so an invalid scheme returns without firing a fetch; `EmptyChunks` never needs to serve. Behaviour confirmed via gate test-run green. |
| T4 Contribution | PASS | Each test fails pre-fix (`k==0` panics; `m==0` returns bytes) and passes post-fix, and `supported_accepts_...` guards against regressing legitimate schemes `(2,1)(3,2)(6,3)(4,4)(4,3)` — all confirmed supported by `use_high_rate`. Tests earn their keep. |
| T5 Judgment | PASS | The judgment to reject ALL coder-unsupported stored schemes (not just `k==0`, incl. `m==0`) was explicitly directed by the iteration-1 carry-forward / codex finding (brief lines 62-66), so it is within-plan, not scope creep; predicate is shared with the coder so no divergence risk. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: confirm the operational intent that a committed-but-tampered `rs(k,0)` whose `k` fragments are all present should be a HARD read error (`InvalidEcScheme`) rather than a best-effort byte return — i.e. that turning a previously-silently-served scheme into a rejection is the desired data-path behaviour, and that no live/legacy inode legitimately carries a scheme `erasure::supported` now rejects. Impact: read availability of any object whose stored scheme fails the predicate. Human owns this fitness call at sign-off. |

### Advisory — codex

No advisory findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decision owed: confirm the operational intent that a committed-but-tampered `rs(k,0)` whose `k` fragments are all present should be a HARD read error (`InvalidEcScheme`) rather than a best-effort byte return — i.e. that turning a previously-silently-served scheme into a rejection is the desired data-path behaviour, and that no live/legacy inode legitimately carries a scheme `erasure::supported` now rejects. Impact: read availability of any object whose stored scheme fails the predicate. Human owns this fitness call at sign-off.

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
- By / date: Eduard Ralph / 2026-07-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
