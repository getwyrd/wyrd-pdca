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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

**Task under review:** a corrupted/tampered inode whose stored chunk scheme is
`EcScheme::ReedSolomon { k: 0, m }` is trusted by the lower read/reconstruct layers;
`k == 0` sails past `available.len() < k` (`0 < 0` false) and reaches `available[0]`
on an empty shard list, turning untrusted metadata into a process **panic**. The fix
must make invalid EC parameters from stored metadata yield a clean typed `Err`, never
a panic, at the erasure/read API boundary.

Grounded on the target worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0` (patch applied,
base current — files match `patch.diff`). Note: cargo is not runnable in the reviewer
sandbox (approval-blocked), so the gating workspace-CI failure could not be re-run
or attributed here; all code judgments are from reading the applied target source and
the two passing per-fix regression tests (C4-verify = pass).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's binding condition — "invalid EC params from stored metadata → clean `Err`, never panic" — is implemented at both cited boundaries: `erasure.rs:116` and `read.rs:184`. Out-of-scope items (CLI parse, on-disk format/ADR-0002, #291) untouched. |
| C2 Reproduction (red pre-fix) | PASS | Two regressions mirror the brief's repro — `reconstruct(0,1,0,&[])` (`erasure.rs:271`) and a stored `k:0` scheme through `read_chunk` (`read.rs:433`); C4-verify confirms red (panic/index) pre-fix, green post-fix. |
| C3 Change | PASS | Minimal, additive: early-return guards `if k == 0` (`erasure.rs:116`, `read.rs:184`) plus typed variants `ErasureError::InvalidScheme` (`erasure.rs:37`) / `ReadError::InvalidEcScheme` (`read.rs:344`). Touches only the two files the brief names; no exhaustive match elsewhere breaks (searched — external tests use `matches!`/`downcast_ref`, not wildcard-less matches). |
| C4 Verification (red→green) | FAIL | **Gating** `cargo xtask ci` is red: `cargo test --workspace --exclude wyrd-dst` exit 101 (`check-gates.json` C4-ci). The per-fix red→green (C4-verify) **passed** and the patch is additive with no static breakage I can find, so the workspace failure is **not attributable to this patch from static review** — but it blocks. Decision owed: pull the actual failing-test name from the CI log and confirm it is pre-existing/flaky (or a 290-wave base drift), not this change. Could not re-run — cargo is approval-blocked in the reviewer sandbox. |
| C5 Causal adequacy | PASS | Root-cause fix, not a symptom guard: `k == 0` is untrusted **input** validated at the trust boundary the brief's invariant names — not a capability probe or a guard over an optional-but-present capability, so the C5 symptom smell-test does not fire. `k == 0` is the sole panic vector; a `k >= 1` unsupported `k/m` already surfaces as a typed `ErasureError::Coder` from `reed_solomon_simd`, not a panic. Decode-time validation was pre-adjudicated out of scope (ADR-0002 / #291). |
| T1 Structure | PASS | Guards sit at the top of each dispatch arm before any I/O/indexing; tests colocated in-module; additive enum variants — idiomatic and well-placed. |
| T2 Shape | PASS | Error variants carry actionable context (`k`, `m`, and `chunk_id` at the read layer) with sensible `Display` (`erasure.rs:54`, `read.rs:380`); field types match the layer (`usize` in erasure, `u8`/`ChunkId` in read). |
| T3 Runtime | PASS | The read-path guard returns before firing a single fragment fetch (`read.rs:184`), and asserts `corrupt` stays empty (a validation rejection, not a corruption finding) — correct semantics; no perf concern. (Workspace-CI runtime failure tracked under C4, not attributable to this change.) |
| T4 Contribution | PASS | Three genuine regressions (empty-`available`, non-empty-`available`, read-path) that were red pre-fix; doc comments state the mechanism and cite the CLI's existing `k>=1` rule. Comment line-refs updated (`:48/:49`→`:64/:65`) to track the shift. |
| T5 Judgment | PASS | Defense-in-depth double guard (read.rs rejects before reconstruct; erasure.rs rejects at the API) is reasonable given the erasure API is also reachable from custodian reconstruction. No scope creep. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must (1) **resolve the gating workspace-CI failure** — get the failing test's identity and decide whether it is pre-existing/flaky/base-drift (285↔290 both edit read.rs) or a real regression before sign-off; and (2) confirm that returning a typed rejection (rather than routing tampered metadata to the repair queue / rejecting at decode) matches operational intent for corrupt inodes. The invariant "never a panic on untrusted EC params" holds in the two unit tests; end-to-end fitness through a real read is the human's call. |

## Notes
- Prior-art (by affected path) per brief: `erasure.rs` / `read.rs` recently touched; no open PR referencing 285. Consistent with the target history I can see; no in-flight duplicate found. Mechanical closed/rejected-PR confirmation is not available from the reviewer sandbox — carry as a human spot-check if not already cleared at triage.
- The only unresolved blocker is C4-ci. Everything the reviewer could ground statically is clean; the gate must go green (or the failure be shown pre-existing) before accept.

### Advisory — codex

- NEEDS-HUMAN — [crates/core/src/read.rs:184](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/core/src/read.rs:184) only rejects stored `k == 0`; other unsupported stored EC schemes such as `rs(k,0)` still drive read fan-out and can reach `erasure::reconstruct` at [crates/core/src/read.rs:267](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/core/src/read.rs:267), where all data shards present can return bytes without the Reed-Solomon coder ever rejecting the unsupported `m == 0` scheme. The brief calls out “otherwise unsupported `k`/`m`” / unsupported `k + m`, so the read-boundary validation may need to use the same supported-scheme predicate as the erasure coder, not just `k != 0`.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Human must (1) **resolve the gating workspace-CI failure** — get the failing test's identity and decide whether it is pre-existing/flaky/base-drift (285↔290 both edit read.rs) or a real regression before sign-off; and (2) confirm that returning a typed rejection (rather than routing tampered metadata to the repair queue / rejecting at decode) matches operational intent for corrupt inodes. The invariant "never a panic on untrusted EC params" holds in the two unit tests; end-to-end fitness through a real read is the human's call.
- [ ] [crates/core/src/read.rs:184](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/core/src/read.rs:184) only rejects stored `k == 0`; other unsupported stored EC schemes such as `rs(k,0)` still drive read fan-out and can reach `erasure::reconstruct` at [crates/core/src/read.rs:267](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/core/src/read.rs:267), where all data shards present can return bytes without the Reed-Solomon coder ever rejecting the unsupported `m == 0` scheme. The brief calls out “otherwise unsupported `k`/`m`” / unsupported `k + m`, so the read-boundary validation may need to use the same supported-scheme predicate as the erasure coder, not just `k != 0`.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

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
- Iteration delta (if iterating): Rebuild must address the codex finding: read-boundary validation (read.rs:184) currently rejects only k == 0. Extend it to reject ALL invalid/unsupported stored EC schemes — notably m == 0 and unsupported k+m — using the same supported-scheme predicate the erasure coder uses, so a tampered rs(k,0) inode can no longer drive read fan-out and return bytes without rejection (path: read.rs:267). The builder previously (and defensibly) scoped this to k == 0 per the brief's narrow success criterion, so the rebuild must NOT repeat the narrow fix: the k==0-only reading is explicitly rejected here. Note: the gating C4-ci failure (cargo test --workspace exit 101) was base-drift / transient, NOT attributable to this patch — re-running the identical command on the applied patch (base now at #402 merge) is green, exit 0, zero failures. It is not a reason to iterate; the codex scope gap is the sole reason.
- By / date: Eduard Ralph / 2026-07-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
