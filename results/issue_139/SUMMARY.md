# Result — issue 139 / m3.1-placement-record

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The committed chunk map records, per fragment index, the **stable D-server
- Success criterion: An `rs(6,3)` write commits a per-fragment placement vector
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2 — single line, no maintenance branches)
- Scope (one logical fix) / out of scope: add a per-fragment placement record to the chunk map (`ChunkRef`,

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

# Check review — m3.1-placement-record (issue #139)

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json` (build-notes.md withheld). Citations grounded on the
target at `$PDCA_TARGET=/home/eddie/wyrd/wyrd` (read-only; it carries the **pre-patch**
origin/main state — `ChunkRef` is still `Copy` with no `placement`, `traits` has no
`DServerId`/`PlacementChunkStore`), or on `patch.diff` for net-new code.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Patch realises the brief's BINDING criterion: per-fragment placement on `ChunkRef` (`patch.diff` metadata.rs `placement: Vec<DServerId>`), a *stable* id (`DServerId = u64`, `patch.diff` traits/src/lib.rs) rather than an endpoint URL, recorded at commit and consumed on read. Brief refs verified on target: `ChunkRef` at `metadata.rs:72-80`, fan-out debt docstring at `fanout.rs:9-12`, `route() = index % n` at `fanout.rs:51-53`. Caveat surfaced under C5/V: the Scope line "introduce the stable D-server id that **registration (Coordination) carries**" is not wired — `DServerId` is defined but never threaded through Coordination/discovery (patch touches no coordination crate); the patch's own doc defers that to #141. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Re-derived: pre-patch `read_object_from` (`read.rs:56`) resolves via `ChunkStore::get_fragment` (`index % n`). The new `Fleet` double (`placement_record.rs:355-417`) segregates each fragment onto exactly one server addressed by stable id, so `get_fragment` (routing `index % n`) misses every moved fragment → MissingFragment → below-`k` error. The `SHIFT=4` rotation + the guard `placement[i] != i ∀i` (`placement_record.rs:541-544`) make `index % n` provably unable to pass. No standalone C2 gate configured (`check-gates.json` C2 = "none"); the red→green is asserted by the C4-verify gate. |
| C3 — C3 Change | PASS | Well-formed diff, one logical change (placement record record-at-write / resolve-on-read). Compiles + full suite green per C4-ci gate. `ChunkRef` loses `Copy` (additive `Vec`), and the consequent `.clone()` ripple is handled consistently across `dst_erasure.rs`, `dst_read_fanout.rs`, `erasure_path.rs`, `conformance.rs`. Backward-compat is real: `#[serde(default)]` empty placement + `fragment_dserver` `.unwrap_or(index)` fallback (`patch.diff` read.rs) preserve the M0–M2 path. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json`: C4-ci gating gate **pass** ("xtask ci: all checks passed"); C4-verify **pass** ("run-verify.sh: PASS — red without the fix, green with it"). Consistent with my C2 re-derivation. Note (not re-runnable here, artifact-only): "red" is structural — the test depends on the new `placement`/`PlacementChunkStore` API, so the red leg is the source-reverted-keep-test sense, not new-test-on-old-source compile. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Oracle is reviewer + human sign-off, and there is a genuine contested-adequacy question. In **production** the record is inert: `WritePlan::chunk_refs` hardcodes the identity vector `(0..fragments.len())` (`patch.diff` write.rs) rather than calling `placement()`, the write still physically routes via `put_fragment` (`index % n`), and `FanoutChunkStore::get_fragment_at` uses the **default** that *ignores* `dserver` and delegates to `get_fragment` (`patch.diff` traits/src/lib.rs). So identity-record→`index % n`-read round-trips to a no-op; the only implementation that honours a non-identity record is the test-only `Fleet`. Whether a seam whose sole honouring consumer is a test double is causally sufficient for this foundation slice (vs. dead scaffolding until the relocatable fan-out in #141 / later 0005 slices) is a human call — defensible given the brief explicitly defers the custodian-aware fan-out, but not auto-clearable. |
| T1 — T1 Structure | PASS | Regression lives exactly where the brief's "Test file" line requires: `crates/core/tests/placement_record.rs` (new, `patch.diff:299`), the core in-process regression home; server-level coverage left supplementary as scoped. |
| T2 — T2 Shape | PASS | Two well-shaped tests with sharp asserts: Property 1 records a length-`N` vector and reconstructs after redb **reopen** (`placement_record.rs:422-471`); Property 2 (BINDING) places every fragment off its `index % n` home, reopens, asserts `placement` survived and reconstructs (`:477-552`), with the explicit non-`index % n` divergence guard. Minor: Property 2 hand-builds the `InodeRecord` (`metadata::create`) rather than driving the write path, so the *moved*-vector write-recording is not end-to-end exercised (the write-records leg is Property 1, identity only) — adequate in combination, flagged for T5. |
| T3 — T3 Runtime | PASS | In-process: `pollster::block_on`, `tempfile::tempdir`, real `RedbMetadataStore::open` reopen as the process-restart equivalent — no Docker, Check-observable. Runs green under the C4-ci gate. |
| T4 — T4 Contribution | PASS | Net-new coverage (brief prior-art check: no placement-record work merged/open). The divergence guard guarantees the test cannot pass via residual `index % n`, so green genuinely attributes to record-driven resolution — the test earns its keep. |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle reviewer + human sign-off. Judgment call: is the `Fleet` double a fair proxy for the real moved-fragment world, and does proving read-from-a-**hand-authored** record (Property 2 bypasses the write path) adequately demonstrate the BINDING condition, given production never yet emits a non-identity record? Reasonable but not mechanically decidable. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Does this slice, as built, advance proposal 0005 M3.1's purpose — given the production read/write still collapse to `index % n` (C5) and the stable id is not yet carried through Coordination (C1)? Fitness-to-purpose and the scope boundary vs. #141 are the human sign-off's to settle. |

## §6 — Items the human must clear

1. **C5 (causal adequacy / ambiguous scope).** The placement record is load-bearing only against the test-only `Fleet`; production write records identity and both write and `FanoutChunkStore::get_fragment_at` still resolve `index % n`. Decide whether a record-consuming seam with no production honouring implementation is sufficient for this foundation slice or is premature scaffolding.
2. **C1/V (scope deferral).** Brief Scope says introduce the stable D-server id "that registration (`Coordination`) carries and discovery resolves to a current endpoint." The patch defines `DServerId` but does not touch any coordination/discovery crate, deferring to #141. Confirm this deferral is in-scope for step 1.
3. **T5 (test judgment).** Confirm `Fleet` is a fair proxy and that Property 2's hand-built record (not write-path-produced) adequately demonstrates the BINDING moved-fragment condition.
4. **V (fitness-to-purpose).** Standard human sign-off on whether the slice meets 0005 M3.1's intent.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 — C5 Causal adequacy — Oracle is reviewer + human sign-off, and there is a genuine contested-adequacy question. In **production** the record is inert: `WritePlan::chunk_refs` hardcodes the identity vector `(0..fragments.len())` (`patch.diff` write.rs) rather than calling `placement()`, the write still physically routes via `put_fragment` (`index % n`), and `FanoutChunkStore::get_fragment_at` uses the **default** that *ignores* `dserver` and delegates to `get_fragment` (`patch.diff` traits/src/lib.rs). So identity-record→`index % n`-read round-trips to a no-op; the only implementation that honours a non-identity record is the test-only `Fleet`. Whether a seam whose sole honouring consumer is a test double is causally sufficient for this foundation slice (vs. dead scaffolding until the relocatable fan-out in #141 / later 0005 slices) is a human call — defensible given the brief explicitly defers the custodian-aware fan-out, but not auto-clearable.
- [x] T5 — T5 Judgment — Oracle reviewer + human sign-off. Judgment call: is the `Fleet` double a fair proxy for the real moved-fragment world, and does proving read-from-a-**hand-authored** record (Property 2 bypasses the write path) adequately demonstrate the BINDING condition, given production never yet emits a non-identity record? Reasonable but not mechanically decidable.
- [x] V — Validation — fitness-to-purpose — Always-human. Does this slice, as built, advance proposal 0005 M3.1's purpose — given the production read/write still collapse to `index % n` (C5) and the stable id is not yet carried through Coordination (C1)? Fitness-to-purpose and the scope boundary vs. #141 are the human sign-off's to settle.

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
