# Result — issue 405 / networked-client-observable

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The #329 consistency checker (ADR-0041) needs a **networked client it can
- Success criterion: A reusable networked S3 client observable drives a workload of
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4
- Scope (one logical fix) / out of scope: Build a reusable, networked, **observable S3 client** — a type that drives a

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

Issue 405 reviews a reusable networked S3 observable that records real-time PUT/GET/DELETE register history for the consistency-checker harness.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is concrete: a loopback S3 PUT/GET/DELETE observable must record start/end timestamps and observed value/version for a non-vacuous register history; list/rename and real-cluster checker work are explicitly out of scope (`brief.md:19`, `brief.md:68`). |
| C2 Reproduction (red pre-fix) | PASS | In a throwaway pre-patch clone with only the new test applied, `cargo test -p wyrd-server --test consistency_observable --no-run` fails on missing `wyrd_server::consistency_observable`, so the test is red before the client exists (`crates/server/tests/consistency_observable.rs:24`). |
| C3 Change | PASS | The patch adds the public observable module, exports it, and drives the requested loopback overwrite/read/delete workload through it (`crates/server/src/consistency_observable.rs:127`, `crates/server/src/lib.rs:17`, `crates/server/tests/consistency_observable.rs:67`). |
| C4 Verification (red->green) | NEEDS-HUMAN | The green loopback proof could not be independently reproduced here because this sandbox denies listener creation (`TcpListener::bind("127.0.0.1:0")` at `crates/server/tests/consistency_observable.rs:49` failed with `Operation not permitted`); compile/unit/format checks passed, but the live red->green gate remains provisional. |
| C5 Causal adequacy | PASS | The fix supplies the missing networked observable rather than adding a capability probe or runtime guard; operations record client start/end spans around real signed TCP requests and preserve per-key version monotonicity for checker input (`crates/server/src/consistency_observable.rs:157`, `crates/server/src/consistency_observable.rs:176`, `crates/server/src/consistency_observable.rs:199`). |
| T1 Structure | PASS | The module lives in `server`, matching the brief's least-friction home beside the S3 wire test and avoiding `testkit` async/HTTP coupling (`brief.md:97`, `crates/server/src/lib.rs:17`). |
| T2 Shape | PASS | The API exposes operation kind, key, status, version, and start/end timestamps as reusable history records, which is the shape the downstream register checker needs (`crates/server/src/consistency_observable.rs:38`, `crates/server/src/consistency_observable.rs:55`, `crates/server/src/consistency_observable.rs:88`). |
| T3 Runtime | NEEDS-HUMAN | Runtime behavior over the real loopback gateway is the decisive evidence, but the reviewer host blocks loopback binds; a human must run `cargo test -p wyrd-server --test consistency_observable` on a host that permits local listeners (`crates/server/tests/consistency_observable.rs:38`, `crates/server/tests/consistency_observable.rs:75`). |
| T4 Contribution | PASS | Local affected-path history and grep found no prior `consistency_observable` client, matching the brief's prior-art claim that previous Jepsen work was over a different repair/fragment layer (`brief.md:130`, `crates/server/src/consistency_observable.rs:1`). |
| T5 Judgment | NEEDS-HUMAN | Scope sign-off is still owed: accept this slice as register-only PUT/GET/DELETE and defer directory list/rename until wire verbs exist, or require the larger directory-model slice now (`brief.md:78`). |
| Validation -- fitness-to-purpose | NEEDS-HUMAN | Human validation is required by design: decide whether the recorded value-tag history is fit as the slice-2 input to the later off-Check linearizability checker, given that the checker verdict and real cluster nemesis remain out of scope (`brief.md:102`, `brief.md:113`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red->green) — The green loopback proof could not be independently reproduced here because this sandbox denies listener creation (`TcpListener::bind("127.0.0.1:0")` at `crates/server/tests/consistency_observable.rs:49` failed with `Operation not permitted`); compile/unit/format checks passed, but the live red->green gate remains provisional.
- [x] T3 Runtime — Runtime behavior over the real loopback gateway is the decisive evidence, but the reviewer host blocks loopback binds; a human must run `cargo test -p wyrd-server --test consistency_observable` on a host that permits local listeners (`crates/server/tests/consistency_observable.rs:38`, `crates/server/tests/consistency_observable.rs:75`).
- [x] T5 Judgment — Scope sign-off is still owed: accept this slice as register-only PUT/GET/DELETE and defer directory list/rename until wire verbs exist, or require the larger directory-model slice now (`brief.md:78`).
- [x] Validation -- fitness-to-purpose — Human validation is required by design: decide whether the recorded value-tag history is fit as the slice-2 input to the later off-Check linearizability checker, given that the checker verdict and real cluster nemesis remain out of scope (`brief.md:102`, `brief.md:113`).

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
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
