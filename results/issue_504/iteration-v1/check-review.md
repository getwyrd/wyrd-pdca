Review of issue #504: refuse unsupported S3 CopyObject-form PUT requests before they can overwrite destination data with an empty body.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: CopyObject-form PUT must return S3 `NotImplemented` without mutation, while ordinary PUT remains supported (`crates/server/tests/s3_copy_object_guard.rs:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independent scratch rerun without the gateway hunk failed at the expected 200-versus-501 assertion, while the ordinary-PUT control passed (`crates/server/tests/s3_copy_object_guard.rs:192`). |
| C3 Change | PASS | The change is confined to rejecting the unsupported header before body consumption, so the data-loss path is closed without claiming CopyObject support (`crates/gateway-s3/src/lib.rs:577`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether targeted red→green plus passing fmt/clippy is sufficient — both patched tests passed, but the asserted aggregate `./engine/xtask.sh ci` runner is absent from the supplied target and could not be independently rerun (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C5 Causal adequacy | PASS | The request is explicitly unsupported, so refusing its discriminating header before the ordinary PUT writer is the invariant-restoring boundary, not a capability probe masking an eager/load-time cause (`crates/gateway-s3/src/lib.rs:577`). |
| T1 Structure | PASS | A separate server integration test exercises the production signed HTTP path and keeps the regression at the externally observable boundary (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| T2 Shape | PASS | The oracle distinguishes refusal status/error code, byte-identical destination survival, and unaffected ordinary PUT behavior, preventing a superficial status-only pass (`crates/server/tests/s3_copy_object_guard.rs:191`). |
| T3 Runtime | PASS | Independent patched execution completed both loopback runtime tests successfully, including the destructive-path regression and ordinary-PUT control (`crates/server/tests/s3_copy_object_guard.rs:220`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected remote work supersedes this contribution — affected-path merged/local-ref history and `-S x-amz-copy-source` found no prior implementation, but closed/rejected forge state was unavailable mechanically (`crates/gateway-s3/src/lib.rs:577`). |
| T5 Judgment | PASS | The refusal is proportionate to the immediate data-loss risk and leaves full server-side copy outside this patch, avoiding ambiguous scope expansion (`crates/gateway-s3/src/lib.rs:574`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether temporary `501 NotImplemented` is acceptable product behavior until full CopyObject support lands — it prevents corruption but deliberately declines a standard S3 operation (`crates/gateway-s3/src/lib.rs:580`). |
