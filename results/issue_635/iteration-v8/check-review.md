Task under review: add byte-compatible segmented inode chunk maps with staged publication and shared resolution across read and maintenance consumers (issue #635).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the wire bytes, decode invariants, consumer scope, containment behavior, and precursor boundary as independently testable outcomes. |
| C2 Reproduction (red pre-fix) | PASS | The base compiled with the raw-record fixture but all 9 consumer tests failed at decode/assertion, so RED is behavioral rather than compile-only (`crates/custodian/tests/segmented_map_consumers.rs:511`). |
| C3 Change | PASS | The shared resolver, staged publisher, and consumer migration cover the specified seam without unrelated dependency or build-system expansion (`crates/core/src/metadata.rs:1059`). |
| C4 Verification (red→green) | PASS | The focused suite moved from 0/9 on base to 9/9 patched, and `cargo xtask ci` passed typos, docs, deny, conformance, workspace tests, and the 50-seed DST (`crates/custodian/tests/segmented_map_consumers.rs:511`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | The rebuild must cover every valid suffix of a truncated ID — prefix `1` returns 1,999,999,999,999,999,999 although `2^64−1` is valid, so recovery can let the allocator remint a live ID (`crates/core/src/metadata.rs:3823`). |
| T1 Structure | PASS | One core resolver and one custodian live-root adapter centralize the cross-consumer invariant without a new dependency edge or manifest churn (`crates/custodian/src/resolve.rs:76`). |
| T2 Shape | PASS | JSON-type discrimination preserves legacy flat bytes while decode rejects invalid segmented cross-field structure (`crates/core/src/metadata.rs:1112`). |
| T3 Runtime | NEEDS-HUMAN | Maintainers must accept landing the publication API before #636 supplies `Completing` and a production caller — this decides whether dormant persistence machinery may ship independently (`crates/core/src/metadata.rs:2660`). |
| T4 Contribution | NEEDS-HUMAN | A human must triage the six reported batch-review blockers — affected-path merged/closed history found only merged precedents, but the target lacks `scripts/review-branch` and the finding bodies are unavailable, so their validity cannot be re-derived. |
| T5 Judgment | NEEDS-HUMAN [impl] | The tests must use an independent boundary oracle for truncated IDs — the current expectations encode the same unsafe approximation and omit the prefix-`1` ceiling case, so they cannot catch the allocator regression (`crates/core/src/metadata.rs:7709`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainers must decide whether raw-record and Redb evidence is fit for the >10 GiB purpose before #636 supplies the real multipart producer — production publication topology was not exercised, so launch fitness remains unproven (`crates/core/src/metadata.rs:2660`). |
