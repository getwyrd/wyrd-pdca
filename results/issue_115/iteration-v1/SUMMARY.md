# Result — issue 115 / parallel-any-k-read

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: for an `rs(6,3)` chunk, a GET reconstructs **byte-identically**
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: in `core::read_chunk` (`crates/core/src/read.rs`, the

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
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

# Check review — issue 115 / parallel-any-k-read

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json` (build-notes.md deliberately withheld). The
planning artifacts (proposal 0004; architecture §6.2/§6.6) cited by the brief
live in `getwyrd/wyrd` and are **not** in my working set, so I could not
independently confirm the patch against the governed plan — that gap feeds the
NEEDS-HUMAN rows below. Every Basis is re-derived from the artifacts I hold.

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | `brief.md:14-39` fully fixes defect (serial in-order fetch, `read.rs:80`), goal (parallel any-*k*-arrive-first), success criterion (byte-identical reconstruct, read-around ≤`m`, clean typed error below `k`, cancel rest, `None` unchanged) and scope. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Red is derivable from the test design: main's serial `for index in 0..n` awaits hung index 0 → never resolves → `READ_BUDGET` timeout → `.expect(...)` panics red (`read_fanout.rs:199-203`). No gate configured (`check-gates.json:14-21`); I could not execute, so the red is reasoned, not observed. |
| C3 — C3 Change | PASS | `read.rs` ReedSolomon arm replaces the serial loop with a `FuturesUnordered` fan-out over all `n=k+m` indices, reconstructs from first `k` valid, then `break` drops `inflight` to cancel the rest (`patch.diff:64-90`); `EcScheme::None` untouched (`patch.diff:40`). Cancellation-by-drop semantics are correct. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json:32-40` — `C4 cargo xtask ci` (fmt/clippy/build/test/deny/conformance), gating, result `pass`, "xtask ci: all checks passed". Confirms green; pre-fix red not mechanically demonstrated by any gate. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Latency causal link is clean (serial wait on slow `m` → parallel any-`k`). But the ADR-0009 determinism-under-simulation requirement (`brief.md:33`) is asserted only by comment (`patch.diff:60-63`) and exercised by nothing in my artifacts — the test runs real `multi_thread` tokio, not the sim harness. Oracle is reviewer + human sign-off (`check-gates.json:42-48`). |
| T1 — T1 Structure | PASS | `read_fanout.rs` is well-formed: `Cluster` gRPC harness, `FaultStore` fault injection (`read_fanout.rs:55-`), two `#[tokio::test]` cases, write/id helpers. Adds `async-trait` dev-dep for the fake (`patch.diff:102-104`). |
| T2 — T2 Shape | PASS | Assertions target the right oracles: byte-identical reconstruction (`read_fanout.rs:203`) and typed `ReadError::InsufficientFragments { have: 5, need: 6 }` (`read_fanout.rs:231-241`). Caveat: order-independence is covered by only one hung arrangement (indices 0,1,2), not a seed sweep — see T5. |
| T3 — T3 Runtime | PASS | `cargo xtask ci` includes the test stage and passed (`check-gates.json:32-40`); tests bound runtime with a 10s `READ_BUDGET` timeout (`read_fanout.rs:150`). Not run by me; relies on the gate. |
| T4 — T4 Contribution | PASS | Test 1 is a genuine discriminator (red on main's serial read via hung index 0, green post-fix; `read_fanout.rs:185-203`). Note: test 2 (`below_k`, `read_fanout.rs:209-`) passes on both main and fix — it is a property/regression test, not a red. |
| T5 — T5 Judgment | NEEDS-HUMAN | Deviation from `brief.md:40-48`: the named seed-reproducible **DST** test (`dst_read_fanout.rs`, style of `dst_erasure.rs`) was not delivered; instead a real loopback-gRPC `multi_thread` timeout test (`read_fanout.rs`) asserting a single hung arrangement, not order-independence across seeds. The brief pre-authorized only the `dst_erasure.rs`-extension deviation; `build-notes.md` (which would justify this one) is withheld. Ambiguous scope → human. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always human: whether the change delivers the M2.5 tail-latency intent (erasure coding as a latency advantage) in production, beyond the artifact-level criteria. Oracle is human at sign-off (`check-gates.json:96-102`). |

## §6 — items the human must clear

1. **C5 causal adequacy — ADR-0009 determinism unverified.** The brief requires
   fragment-completion ordering stay seed-driven under simulation
   (`brief.md:33`). The patch argues this from cooperative single-task polling of
   `FuturesUnordered` with no spawn (`patch.diff:60-63`), but no artifact in my
   set exercises the sim path — the shipped test uses real `multi_thread` tokio.
   Confirm against ADR-0009 and the network DST harness intent that
   `FuturesUnordered` completion order is in fact seed-reproducible.

2. **T5 judgment — test deviates from the named DST oracle.** Brief asked for a
   seed-reproducible DST test asserting any-`k` reconstruction *independent of
   arrival/index order* (`brief.md:40-48`); delivered is a networked timeout test
   over one hung arrangement. Order-independence and seed-reproducibility are
   under-covered. `build-notes.md` is withheld, so the deviation's rationale is
   unavailable to me — read it and confirm the deviation was intended and that
   the order-independence property is adequately asserted (or require the DST
   test / a seed sweep).

3. **V validation — fitness-to-purpose.** Confirm the change actually realizes
   the M2.5 goal (tail-latency win, read-around the slow `m`) as the host plan
   intends, including the planning artifacts I could not access
   (proposal 0004 PR step 5; architecture §6.2/§6.6).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 — C5 Causal adequacy — Latency causal link is clean (serial wait on slow `m` → parallel any-`k`). But the ADR-0009 determinism-under-simulation requirement (`brief.md:33`) is asserted only by comment (`patch.diff:60-63`) and exercised by nothing in my artifacts — the test runs real `multi_thread` tokio, not the sim harness. Oracle is reviewer + human sign-off (`check-gates.json:42-48`).
- [ ] T5 — T5 Judgment — Deviation from `brief.md:40-48`: the named seed-reproducible **DST** test (`dst_read_fanout.rs`, style of `dst_erasure.rs`) was not delivered; instead a real loopback-gRPC `multi_thread` timeout test (`read_fanout.rs`) asserting a single hung arrangement, not order-independence across seeds. The brief pre-authorized only the `dst_erasure.rs`-extension deviation; `build-notes.md` (which would justify this one) is withheld. Ambiguous scope → human.
- [x] V — Validation — fitness-to-purpose — Always human: whether the change delivers the M2.5 tail-latency intent (erasure coding as a latency advantage) in production, beyond the artifact-level criteria. Oracle is human at sign-off (`check-gates.json:96-102`).

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
- Iteration delta (if iterating): V (fitness-to-purpose) accepted — the parallel any-k read delivers the M2.5 tail-latency intent. The implementation patch (read.rs FuturesUnordered fan-out) is not in question. Rework the tests, not the fix: - C5 / T5: deliver the named seed-reproducible DST test (dst_read_fanout.rs, in the style of dst_erasure.rs) that exercises the simulation harness. The shipped read_fanout.rs runs real multi_thread tokio and never touches the sim path, so ADR-0009 determinism-under-simulation (brief.md:33) is asserted only by comment (patch.diff:60-63), not exercised. - The DST test must assert any-k reconstruction is independent of arrival/index order across a seed sweep — not the single hung arrangement (indices 0,1,2) the current networked test covers. Order-independence + seed-reproducibility are the properties under-covered today (brief.md:40-48). - The networked read_fanout.rs may stay as a complementary over-the-wire check, but it does not substitute for the named DST oracle.
- By / date: Eduard Ralph / 2026-06-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
