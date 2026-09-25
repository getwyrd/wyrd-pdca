No actionable findings on either advisory lens: introduced correctness bugs, or reuse, simplification and efficiency.

Reviewed staged reads and placement validation (`crates/custodian/src/gc.rs:1593`), scrub precedence and certification (`crates/custodian/src/scrub.rs:236`), reconstruction retention and discharge (`crates/custodian/src/reconstruction.rs:240`), clock wiring, and the changed regression/DST tests against the target source. The brief's settled scope exclusions were respected.

Validation relied on the frozen gate evidence; no builds were rerun. CI passed, all 16 staged-scrub tests passed with the patch (11 failed by assertion without it), and mutation testing reported 22 caught and 17 unviable mutants, with none surviving.
