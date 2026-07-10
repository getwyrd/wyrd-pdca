Issue 468 adds a simulated-FoundationDB DST story for commit ambiguity while keeping `libfdb_c` out of the simulator.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: add a simulated-FDB `MetadataStore`, exercise 1021 commit ambiguity in DST, and guard DST from FDB linkage (`brief.md:9`). |
| C2 Reproduction (red pre-fix) | PASS | A clean `HEAD` archive with only `no_fdb_linkage.rs` copied in failed on the missing `SimFdbMetadataStore` seam, the intended structural red (`crates/dst/tests/no_fdb_linkage.rs:486`). |
| C3 Change | PASS | The patch adds the simulated-FDB store and `MetadataStore` impl, plus the shared conformance leg and linkage/ambiguity tests (`crates/dst/tests/support/mod.rs:556`, `crates/dst/tests/support/mod.rs:735`, `crates/dst/tests/conformance.rs:59`). |
| C4 Verification (red→green) | PASS | Re-ran `cargo xtask ci` green; separately re-ran `cargo test -p wyrd-dst --test no_fdb_linkage` green and reproduced the base red described above (`crates/dst/tests/commit_ambiguity.rs:323`, `crates/dst/tests/no_fdb_linkage.rs:466`). |
| C5 Causal adequacy | PASS | The fix models the unavailable 1021 failure shape directly and proves the settling read is load-bearing with demonstrated-red observers rather than a capability probe or load-time guard (`crates/dst/tests/commit_ambiguity.rs:389`, `crates/dst/tests/commit_ambiguity.rs:413`). |
| T1 Structure | PASS | The model is a second parametrization beside the existing test support skeleton, not production FDB code or a new framework (`crates/dst/tests/support/mod.rs:393`, `crates/dst/tests/support/mod.rs:556`). |
| T2 Shape | PASS | The behavioral ambiguity test is madsim-gated while the linkage discriminator remains bare cargo-test runnable, matching the split verification posture (`crates/dst/tests/commit_ambiguity.rs:55`, `crates/dst/tests/no_fdb_linkage.rs:63`). |
| T3 Runtime | PASS | Runtime evidence exercised both cargo contexts: madsim `wyrd-dst` ran 9/9 commit-ambiguity tests and bare `no_fdb_linkage` ran 9/9, including graph and manifest scanners (`crates/dst/tests/no_fdb_linkage.rs:216`, `crates/dst/tests/no_fdb_linkage.rs:450`). |
| T4 Contribution | PASS | The tests include non-vacuity checks for landed/not-landed ambiguity, blind pending puts, torn apply, and 1031-vs-1021 behavior, so the contribution is load-bearing (`crates/dst/tests/commit_ambiguity.rs:343`, `crates/dst/tests/commit_ambiguity.rs:563`, `crates/dst/tests/commit_ambiguity.rs:704`). |
| T5 Judgment | NEEDS-HUMAN | Closed/rejected PR prior-art could not be mechanically confirmed from this sandbox; local merged history by affected path showed no FDB hits under `crates/dst`, but PR-state sign-off still matters (`brief.md:151`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off owes the product judgment that this simulator model is an adequate substitute for a real topology that cannot deterministically emit 1021 (`brief.md:39`). |
