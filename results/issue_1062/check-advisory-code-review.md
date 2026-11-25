- NEEDS-HUMAN [human] — `crates/custodian/src/retirement/marks.rs:396`: a non-retained position carrying a `reclaiming` mark always waits for GC. If its D server permanently leaves the fleet, GC cannot destroy its fragment and explicitly retains its fragment-less mark (`crates/custodian/src/gc.rs:904`), so the obligation cannot finish. The wait emits a counter and debug event, without an audited hold or escalation (`crates/custodian/src/retirement.rs:803`). This is the unresolved policy choice in brief iteration 2, item 4: explicitly accept indefinite retention, or require escalation to an operator-repair hold.

No additional implementation defects or worthwhile reuse, simplification, or efficiency findings identified in this diff. Review used the target source and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->
