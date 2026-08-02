Reviewing issue #636: implement the crate-level multipart record family, bounded retirement, and fenced publish/abort state machine over the metadata and chunk-store seams.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The settled records, transitions, batch bounds, ETag oracle, exclusions, and dependency boundary are specific enough to decide conformance. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Confirm the mandatory F/G/E mechanism-negation runs failed as intended — my base replay ran 0 tests because the new tests did not compile, so it proves criterion absence but not load-bearing behavior. |
| C3 Change | FAIL | The slice owes bounded `retire:bytes:` routing for ordinary delete and overwrite, but those paths still fan out inline and reject segmented generations, leaving large operations outside the transaction envelope (`crates/core/src/metadata.rs:2390`, `crates/core/src/metadata.rs:2536`). |
| C4 Verification (red→green) | PASS | Independent scratch replay reproduced the compile-shaped red, then 12/12 targeted tests and the full `cargo xtask ci` passed; the initial read-only advisory-cache lock was discharged with a scratch-local Cargo cache. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Require a segmented root-flip-loss regression — the current DST race stays flat, so it cannot catch the stale resume/fence path, consistent with 162 surviving in-diff mutants (`crates/dst/tests/concurrency.rs:451`). |
| T1 Structure | FAIL | The public classification sweep performs a fleet-wide `sidx:` scan despite the per-session bounded-read invariant, so it fails at the scale the disjoint namespace was introduced to support (`crates/core/src/multipart.rs:4054`). |
| T2 Shape | FAIL | The session decoder accepts state-forbidden or missing cursor, target, and completion shapes, allowing half-understood persisted records past the metadata validation boundary (`crates/core/src/multipart.rs:1341`). |
| T3 Runtime | FAIL | After segmented phase progress, a root-flip loss retries from stale `resume_from` and fence bytes, so fence release can conflict and strand the session in `Completing` (`crates/core/src/multipart.rs:3072`). |
| T4 Contribution | FAIL | Contribution artifacts and affected-path prior-art checks are complete, but the contribution is not review-clean because independent review confirms the required ordinary retirement surface is omitted (`crates/core/src/metadata.rs:2536`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Require event-keyed three-arm orphan restamping or equivalent reader-grace proof — unconditional “present means skip” can retain expired evidence across a later unreference event (`crates/core/src/multipart.rs:3479`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether an intentionally unreachable protocol slice is fit to merge before #625, #637, and #508 — crate tests pass, but no client, reaper, or staged-byte protection consumer exercises the production lifecycle. |
