# Result — issue 634 / scan-page-seam

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `MetadataStore` gains a **bounded, cursor-keyed range scan** whose semantics are
- Success criterion: two **NEW** test files (see `Test file`), both compiled and run by the
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the `scan_page` trait method and its normative doc contract

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 83 mutants tested in 33s: 17 missed, 28 caught, 38 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #634: add a bounded, cursor-keyed `MetadataStore` page scan that can enumerate namespaces beyond `SCAN_CAP` with consistent semantics across backends.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract is decision-complete on ordering, exclusive cursors, termination, stable-key no-skip behavior, page bounds, and zero-bound errors, so backend behavior has an unambiguous oracle (`crates/traits/src/lib.rs:1025`). |
| C2 Reproduction (red pre-fix) | PASS | With the fix stashed and only the added test targets restored, the run failed before executing tests with 86 missing-API/helper compile errors; after restoration, all 29 demonstrated-red cases passed, including the cap-backed violation oracle (`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:1348`). |
| C3 Change | PASS | The authorized narrow seam is present as a required method with native backend implementations, so no production backend can silently inherit the capped `scan` behavior and existing `scan` semantics remain intact (`crates/traits/src/lib.rs:1089`, `crates/metadata-redb/src/lib.rs:162`, `crates/metadata-fdb/src/lib.rs:1898`, `crates/metadata-tikv/src/lib.rs:1275`). |
| C4 Verification (red→green) | PASS | Independent reruns established compile-red then green: 29 demonstrated-red cases, 10 redb page tests, workspace/conformance/statics/DST checks, all four feature-clippy rows, and live FDB conformance passed; the intervening health-test flake passed in isolation and `cargo deny` passed unchanged against a writable copy of the host's read-only advisory cache (`crates/metadata-redb/tests/scan_page.rs:331`, `crates/metadata-fdb/tests/conformance.rs:24`). |
| C5 Causal adequacy | PASS | The fix removes the cap dependency with native cursor/range reads rather than adding a capability probe or runtime guard; an independent 83-mutant run's only default-feature survivor was killed by its focused test, while the remaining 17 were feature-gated bodies and the settled TiKV deferral is not re-raised (`crates/metadata-redb/src/lib.rs:162`, `crates/metadata-tikv/src/lib.rs:543`). |
| T1 Structure | PASS | The trait remains narrow, shared policy lives beside the trait, production adapters own native I/O, and uniform test-double churn is isolated in the testkit dependency direction (`crates/traits/src/lib.rs:1075`, `crates/testkit/src/lib.rs:759`). |
| T2 Shape | PASS | The required `Result<ScanPage>` API, typed zero-page error, exclusive raw-byte cursor, and clamped bound match the settled shape without a compatibility shim or alternate backend contract (`crates/traits/src/lib.rs:326`, `crates/traits/src/lib.rs:336`, `crates/traits/src/lib.rs:1105`). |
| T3 Runtime | PASS | Real redb and live Docker FDB plus both DST simulator stores passed the shared contract, providing runtime evidence for the supported verification posture; the previously accepted TiKV backseat is settled while redb/FDB remain green (`crates/metadata-conformance/src/lib.rs:1473`, `crates/dst/tests/conformance.rs:33`). |
| T4 Contribution | NEEDS-HUMAN | A maintainer must inspect or rerun the current two reported batch-review blockers—the executable and finding report are absent, so their validity cannot be determined and the repository requires one discharged deep review; affected-path merged/open/closed-PR checks otherwise found no competing implementation (`AGENTS.md:206`). |
| T5 Judgment | PASS | No grounded implementation defect, contested symptom guard, semantic-upstream ambiguity, or untracked scope expansion remains; production adoption is deliberately reserved for later consumer slices and the architecture records why the seam itself is needed now (`docs/design/architecture/05-building-block-view.md:204`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether native redb/FDB plus simulator evidence is sufficient fitness for the future unbounded `retire:` and `orphan:` consumers—no production caller adopts the seam in this slice, so its end-to-end reclamation value is not yet observable (`crates/traits/src/lib.rs:1029`). |

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-adversary.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — A maintainer must inspect or rerun the current two reported batch-review blockers—the executable and finding report are absent, so their validity cannot be determined and the repository requires one discharged deep review; affected-path merged/open/closed-PR checks otherwise found no competing implementation (`AGENTS.md:206`). — Human reviewed both findings directly in patch.diff: (1) `escaped()` helper backslash-escaping collision is message-only (raw-byte comparisons unaffected); (2) missing unconditional `key > after` check in the no-skip mutation clause is a real but narrow test-completeness gap, not a demonstrated production defect. Both accepted as-is; tracked as follow-up bugs rather than blocking this slice further (see §10).
- [x] Validation — fitness-to-purpose — The maintainer must decide whether native redb/FDB plus simulator evidence is sufficient fitness for the future unbounded `retire:` and `orphan:` consumers—no production caller adopts the seam in this slice, so its end-to-end reclamation value is not yet observable (`crates/traits/src/lib.rs:1029`). — Accepted: production adoption is deliberately out of scope for this slice (lands in #636/#637 per brief); fitness for those consumers will be judged there against real callers.
- [x] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand. — Accepted as an infra failure of the leaf itself (non-zero exit), not evidence of a defect; not re-run.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_ — Human accepted both findings as non-blocking (see rationale above); filed as follow-up bugs instead of iterating further.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)

> **Amended 2026-07-26, after §9.** The two bugs below were accepted at sign-off as
> follow-ups; they were then FIXED in this bundle's patch instead. `patch.diff` therefore
> post-dates the §9 decision (45 files unchanged, +191 lines). §6 and §9 are left exactly
> as recorded — they are the record of what was decided, not of what the patch now says —
> and the fresh gate run is the additive stamp `revalidation-2026-07-26.json`: C4-ci pass,
> C4 red→green pass, C5-mutants fail (unchanged, non-gating), and **T4 fail → pass**, the
> gating row, now green because the findings are gone rather than triaged away.

- ~~Bug: `crates/metadata-conformance/src/lib.rs` `escaped()` helper (used in scan_page test diagnostics) doesn't escape literal backslash bytes~~ — **FIXED in this patch.** `escaped()` now escapes `\` as `\\`, so the rendering is reversible as its doc claims.
- ~~Bug: `crates/metadata-conformance/src/lib.rs` scan_page no-skip mutation clause is missing an unconditional `key > after` check~~ — **FIXED in this patch.** The clause now records every page that opened at or before its cursor and judges them *after* the stable-key assertions, so the LIMIT/OFFSET double is still caught by "returned exactly once" (the attribution the light per-page checks exist to protect) while a behind-cursor leak is caught at all. A new violating double, `RecentWriteLeakStore`, holds the gap open: a memtable-shaped store that unions its unflushed buffer into each page without applying the cursor, invisible to every static-population clause.

- **Process (still open): a test can overclaim and pass.** `every_byte_is_rendered_reversibly_and_distinctly` existed before this bundle, asserted exactly the property the first bug violated, and passed anyway — its sample set contained every byte class *except the escape character itself*, and its comment reasoned that a per-byte rendering gives distinctness "for free". The gap was not a missing test but a test whose fixture omitted the one input its own subject turns on. Worth an Act look at whether the ruleset should ask a fixture to include the operator a claim is about (here: the escape character in an escaping test).
