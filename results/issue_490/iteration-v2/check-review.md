Review of issue #490: prevent streaming PUT renewal or commit from publishing chunks after their pending leases lapse.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is testable at both renewal and phase-3 commit, including the sweep-aligned `expiry <= now` boundary and an unchanged live path (`crates/core/tests/stream_lease_lapse.rs:184`). |
| C2 Reproduction (red pre-fix) | PASS | An exact-HEAD disposable red leg compiled the added test, failed all three defect assertions, and retained the passing live control (`crates/core/tests/stream_lease_lapse.rs:258`). |
| C3 Change | PASS | Publication now turns on live-lease authority pinned atomically in the inode batch, while renewal refusal is surfaced before another chunk is written (`crates/core/src/metadata.rs:328`; `crates/core/src/write.rs:458`). |
| C4 Verification (red→green) | NEEDS-HUMAN | A human must rerun full `cargo xtask ci` on a host permitting loopback binds — focused red→green is independently confirmed, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:319`). |
| C5 Causal adequacy | PASS | The decision point is the authority itself: absent/expired leases refuse, and read-back values become same-batch preconditions, so neither checked seam relies on a symptom guard or capability probe (`crates/core/src/metadata.rs:682`). |
| T1 Structure | PASS | Renewal, create, and overwrite enforce the invariant at the shared metadata/write boundaries used by production phase 3 (`crates/server/src/lib.rs:178`). |
| T2 Shape | PASS | Both create and overwrite carry every planned chunk into lease guards without weakening the unconditional reconstruction/backfill API (`crates/core/src/write.rs:268`; `crates/core/src/write.rs:303`). |
| T3 Runtime | PASS | The applied target passed all four focused runtime scenarios, including byte-identical publication for a live lease (`crates/core/tests/stream_lease_lapse.rs:337`). |
| T4 Contribution | NEEDS-HUMAN | A human must confirm closed/rejected work contains no competing affected-path fix — local merged history by affected file showed none after `HEAD` dc503cd, but closed/rejected remote work was unavailable to settle mechanically (`crates/core/src/metadata.rs:640`). |
| T5 Judgment | PASS | The patch is confined to the two specified lease seams, the required commit-time plumbing, protocol-correct test setup, and regression coverage; no ambiguous scope re-entry was found (`crates/core/src/write.rs:296`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The human must decide whether fail-closed `Conflict` behavior is operationally acceptable for real streaming PUT clients — it preserves durability but may surface retries at the gateway (`crates/server/src/lib.rs:196`). |
