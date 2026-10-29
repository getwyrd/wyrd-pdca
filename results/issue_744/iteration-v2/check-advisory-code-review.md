No findings. The diff is clean under both advisory lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed payload bounds, direct access and iterator arithmetic (`crates/validate/src/payload.rs:156`, `crates/validate/src/payload.rs:249`), tuning resolution and boundary deduplication (`crates/validate/src/sizes.rs:158`, `crates/validate/src/sizes.rs:212`), and CLI parsing (`crates/validate/src/args.rs:162`). The carried-forward test and diagnostic issues are addressed, including bounded huge-range checks (`crates/validate/tests/payload_and_sizes.rs:522`) and flag-specific overflow errors (`crates/validate/tests/payload_and_sizes.rs:1016`).

Validation: inspected the frozen gate logs (25 payload/size tests and 21 unchanged CLI tests pass; 91 mutants caught, 8 unviable) and independently recomputed all six payload digests and both large-offset known answers. No target files were changed or builds rerun. The declared net-new-API red-leg limitation remains recorded in C4-verify; it is not a new implementation finding.

<!-- pdca:leaf-complete -->
