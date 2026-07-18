Review of issue 504: refuse `CopyObject`-shaped PUT requests before body storage so an unsupported copy cannot erase the destination object.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: return S3 `501 NotImplemented`, preserve existing bytes, and leave ordinary PUT behavior intact (`crates/server/tests/s3_copy_object_guard.rs:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independent scratch-base execution failed the core wire test with observed status `200` versus required `501`, while the ordinary-PUT control passed (`crates/server/tests/s3_copy_object_guard.rs:192`). |
| C3 Change | PASS | The patch stays within the specified refusal slice and makes the decision before body consumption, avoiding the deferred server-side-copy scope (`crates/gateway-s3/src/lib.rs:577`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether independently confirmed targeted red→green plus clean fmt, affected-package Clippy, and diff checks is sufficient — the asserted aggregate `./engine/xtask.sh ci` runner is absent from the target and therefore could not be rerun (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| C5 Causal adequacy | PASS | Refusing the unsupported operation at dispatch removes the destructive fallthrough rather than probing or masking an eager/load-time cause (`crates/gateway-s3/src/lib.rs:577`). |
| T1 Structure | PASS | The real-wire regression is isolated as the requested integration-test crate and covers both the hazardous request and unaffected control (`crates/server/tests/s3_copy_object_guard.rs:167`). |
| T2 Shape | PASS | Header presence, not value parsing or signed-header membership, is the relevant operation discriminator, and it is checked before the storage stream is formed (`crates/gateway-s3/src/lib.rs:577`). |
| T3 Runtime | PASS | Independent patched execution passed both loopback TCP tests, including `501`/S3 error-body assertion, byte-identical destination survival, and ordinary PUT/GET (`crates/server/tests/s3_copy_object_guard.rs:191`). |
| T4 Contribution | PASS | Affected-path history across all local refs, `-S x-amz-copy-source`, and an all-state forge search found no merged, closed, rejected, or open prior implementation (`crates/gateway-s3/src/lib.rs:577`). |
| T5 Judgment | PASS | The data-loss boundary is unambiguous and the patch neither implements copy nor changes SigV4 semantics, so no additional product or architectural choice is introduced (`crates/gateway-s3/src/lib.rs:572`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether refusing CopyObject with `501` is the acceptable interim client experience — this prevents data loss but intentionally withholds server-side copy until the later metadata-dependent slice (`crates/gateway-s3/src/lib.rs:574`). |
