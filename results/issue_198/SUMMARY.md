# Result — issue 198 / read-path-chunk-id-recheck

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The read path accepts a stored fragment after verifying only its
- Success criterion: A read whose backing store returns a misplaced-but-intact
- Repo + branch target: getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- Scope (one logical fix) / out of scope: the read path's fragment-acceptance must not omit the `chunk_id` match that

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

# Check review — issue 198 / read-path-chunk-id-recheck

**Role:** advisory, artifact-only, decorrelated from the builder.
**Inputs seen:** `patch.diff`, `brief.md`, `check-gates.json`. (`build-notes.md` deliberately withheld.)

**Grounding source.** `$PDCA_TARGET` could not be read in this sandbox (`printenv` is
permission-blocked). I grounded path:line citations on the granted working directory
`/home/eddie/wyrd/wyrd` (branch `main`), which I verified *is* the unpatched base the
brief and patch reference, not an arbitrary checkout: its `crates/core/src/read.rs`
carries the bare `decode` at the `None` site (`read.rs:138-139`) and the RS site
(`read.rs:176-177`) exactly as the brief cites, the shared-verify functions sit at the
cited `repair.rs:53-54` / `:66-68` / doc `:48-51`, and both patch hunk headers
(`@@ -136` / `@@ -174`) apply cleanly against it. I did **not** read the sibling worktree
`/home/eddie/wyrd/wyrd/.claude/worktrees/architecture-work`. **§6 item:** a human should
confirm `$PDCA_TARGET` is this base tree (see §6.2).

## Verdict matrix (5 / 5 / 1)

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | brief.md gives a single, testable success criterion (brief.md:23-33) with an explicit BINDING ("a wrong-`chunk_id` fragment is never admitted on the read path") plus a restorable invariant (brief.md:34-45); illustrative vs binding cleanly separated. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Both new tests are flippable against the base: pre-fix the bare `Ok(decoded) => Ok(decoded.payload)` (read.rs:139) returns the foreign payload, and the bare `Ok(decoded) =>` RS arm (read.rs:177) admits the foreign shard → corrupt reconstruct; corroborated by check-gates C4-verify=pass (check-gates.json:42-48). |
| C3 — C3 Change | PASS | Patch adds `decoded.header.chunk_id == chunk.id` at both and only admit sites (patch.diff:14 for `None`, patch.diff:38 for RS); semantics identical to the shared verify (`repair.rs:54`, `:68`); hunks apply cleanly to the base (read.rs:138, :176); edits confined to read.rs + the brief's test file. |
| C4 — C4 Verification (red→green) | PASS | check-gates.json: C4-ci=pass (gating, "all checks passed", :33-40) and C4-verify=pass (per-fix red→green, :42-48); overall=pass (:3). Red→green logic independently re-derived per C2. (Execution itself is the gate's claim — not re-run here; target is read-only.) |
| C5 — C5 Causal adequacy | PASS | read.rs has exactly two fragment-admit sites (read.rs:138, :176; the other `decode`s at read.rs:35/48 are `metadata::decode`) — both fixed, so the cause (read omitting the recheck) is removed, not symptom-guarded. Read was the sole non-compliant consumer: scrub (scrub.rs:79), reconstruction (reconstruction.rs:251), rebalance (rebalance.rs:240) already enforce the gate (repair.rs:45-48). Root cause uncontested; advisory — gate oracle co-requires human sign-off. |
| T1 — T1 Structure | PASS | Tests appended to the brief's designated file `crates/core/tests/read_repair.rs` (brief.md:63), as `#[tokio::test]` fns reusing the existing module harness (MemMeta/MemChunks, helpers at read_repair.rs:108/116) — same idiom as the established tests there. |
| T2 — T2 Shape | PASS | Each test is arrange (build a valid fragment with a foreign `chunk_id`) / act (`read::read_object`) / assert (`is_err`) and pins the binding criterion — foreign bytes never admitted — once per scheme (patch.diff:120-125 `None`, :193-197 RS), with diagnostic messages. |
| T3 — T3 Runtime | PASS | Exercises real `encode`/`decode`, real `erasure::encode`, and real `read::read_object` (read.rs:210) over the trait stores; `fragment()` (read_repair.rs:108-113) emits genuinely valid, checksum-passing fragments, so the `chunk_id` gate is the *only* thing that can reject — the test isolates the change under test rather than mocking it away. |
| T4 — T4 Contribution | PASS | Two independent flippable tests, one per admit site, satisfy the brief's self-test that a one-site patch must visibly fail (brief.md:42-45): dropping either `chunk_id` guard fires the matching assertion. Genuine new regression coverage for both schemes. |
| T5 — T5 Judgment | PASS | Deliberate isolation: `None` test sizes the foreign payload equal to the chunk so a pre-fix admit clears the inode size check (patch.diff:88-90); RS test makes the payload span both data shards so a foreign shard yields *observable* corruption, not discarded padding (patch.diff:138-141). Strong test judgment; advisory — gate oracle co-requires human sign-off. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (check-gates oracle "human at sign-off", :107). Only a human can judge whether the chosen behavior — surface `MissingFragment` for a misplaced fragment and route it through the *existing* corrupt-repair plumbing (`corrupt.push`, patch.diff:22) rather than a distinct "misplaced" signal — is fit for the data-durability purpose. See §6.1. |

## §6 — items the human must clear

### §6.1 Validation — fitness-to-purpose (V → NEEDS-HUMAN)
The fix correctly *stops admitting* the misplaced fragment on both schemes (the BINDING).
Confirm the surrounding behavioral choices serve the purpose:
- A misplaced-but-intact fragment is reported to the caller as `MissingFragment`
  (patch.diff:23) and the chunk is enqueued for repair via the same `corrupt` path used
  for checksum failures (patch.diff:22). This matches the restored invariant
  (repair.rs:45-48: a misplaced fragment is excluded **and** its chunk enqueued) and the
  brief's "retain existing repair-trigger plumbing as-is" (brief.md:52-55). Human to
  accept that conflating "misplaced" with "corrupt" at the caller surface is desired, and
  that no distinct misplacement signal is needed for #207 / the store contract (declared
  out of scope, brief.md:52-54).

### §6.2 Grounding target confirmation (process)
Confirm `$PDCA_TARGET` is the base tree I grounded on (`/home/eddie/wyrd/wyrd` @ `main`,
brief's `c2223a5`) and not the `architecture-work` worktree. All citations above were
verified against that tree and against `patch.diff`; if the intended target differs, the
path:line bases should be re-confirmed there.

## Notes (advisory, non-gating)
- C4-verify's recorded `path_line` excerpt ("…as its own file to earn the full
  red→green.", check-gates.json:46) hints the verify harness may prefer the per-fix test
  as a standalone file, whereas the two tests were appended to `read_repair.rs`. The gate
  nonetheless records **pass**; flagged only so the human is aware of the wording — not a
  basis to override a passing gate.
- The inline `chunk_id` check duplicates `repair::fragment_intact`/`intact_shard` rather
  than calling them. The brief explicitly authorizes inline as Do's call provided the
  check is identical (brief.md:30-33); it is. Consequently the repair.rs:50-51 doc claim
  that the read path "decodes for the same effect inline" now holds literally.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] V — Validation — fitness-to-purpose — Always-human (check-gates oracle "human at sign-off", :107). Only a human can judge whether the chosen behavior — surface `MissingFragment` for a misplaced fragment and route it through the *existing* corrupt-repair plumbing (`corrupt.push`, patch.diff:22) rather than a distinct "misplaced" signal — is fit for the data-durability purpose. See §6.1.

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
- By / date: Eduard Ralph / 2026-06-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
