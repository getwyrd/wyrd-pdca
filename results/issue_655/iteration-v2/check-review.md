Issue #655 implements the multipart knob constants and derivations; it needs a rebuild for an incomplete byte-budget clamp and excess review size, plus human sign-off on verification and sizing assumptions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative knob table fixes explicit ranges, derivations, ownership, and failure impacts, making the requested seam independently judgeable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1462`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Human must accept criterion-absence as the reproduction — without the patch, the test loses its imported API and fails compilation before any assertion executes (`crates/core/tests/multipart_knobs.rs:19`; `gate-logs/C4-verify.log:50`). |
| C3 Change | PASS | The change remains additive and confined to the authorized module plus its required integration test, with no new dependency or enforcement surface (`crates/core/src/multipart.rs:4356`; `crates/core/tests/multipart_knobs.rs:1`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Human must require a completed required gate or accept partial evidence — post-fix fmt/test/clippy, typos, machete, and non-advisory deny checks pass, but red never executes and the full gate times out in `custodian_gc` before cargo-deny advisories (`gate-logs/C4-ci.log:2049`; `gate-logs/C4-verify.log:50`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must require `B_bytes` to hold at least one maximal segment mutation and test its rejection — the validator currently checks only slot-pin bytes, so an accepted set can derive a zero-sized segment batch and never publish (`crates/core/src/multipart.rs:4980`; `crates/core/tests/multipart_knobs.rs:178`). |
| T1 Structure | PASS | The one-authority structure is preserved because `KnobSet` projects into the existing `Budget` and reuses its derivations instead of re-spelling them (`crates/core/src/multipart.rs:4787`). |
| T2 Shape | FAIL | The reviewability cap is exceeded: a conservative diff count leaves 587 content-bearing additions after comments, blanks, attributes, punctuation-only, and chain-continuation lines are excluded, versus the brief's 400-line ceiling (`crates/core/src/multipart.rs:5045`; `crates/core/tests/multipart_knobs.rs:636`). |
| T3 Runtime | NEEDS-HUMAN | Human must approve the assumed 5 ms per backend operation or require slowest-backend calibration — that unmeasured value determines whether `MAX_BATCH_OPS` actually stays within the five-second transaction envelope (`crates/core/src/multipart.rs:4544`). |
| T4 Contribution | N/A | `pr-description.md` is absent by design at Check; the substantive contribution audit is mandatory and reruns at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Human must decide whether `W_ref` may ship as a compile-time 4,000,000-reference budget rather than a deployment input sized from reconcile-host RAM, because that choice fixes admitted concurrency (`crates/core/src/multipart.rs:4585`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether this inert value set is fit for the later enforcing slices despite no current production consumer exercising the composition (`crates/core/src/multipart.rs:4371`). |

Prior-art check: an affected-path scan across 300 GitHub PRs found only merged prerequisite edits (#703, #724, #725, #792, #793, #799) to `crates/core/src/multipart.rs`, no closed/unmerged path match, and no prior PR touching the new test; the brief separately records the discontinued #508/#636 issue work.

Mutation note: the two surviving `<` to `<=` mutants select equal-valued branches at the tie and are equivalent, so they add no defect beyond the missing clamp above (`gate-logs/C5-mutants.log:13`).
