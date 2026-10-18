- NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_evacuate.rs:528`: **The new ceiling regression does not exercise the brief's required V/2 refusal.** It seeds a 100,000-byte segment and tests growth beyond that full ceiling. A 49,999-byte segment growing by the same 19 bytes would instead commit, contrary to success criterion 3. This matches the target primitive's intentional full-ceiling contract (`crates/core/src/metadata.rs:593`, `crates/core/src/metadata.rs:3340`), so the passing test cannot settle the discrepancy. Decide whether to revise the brief to that contract or split out a prerequisite primitive change; the brief forbids changing the primitive or duplicating its ceiling check in this slice.

No additional correctness, reuse/simplification, or efficiency findings in the diff. Validation was assessed from the frozen gate logs; no tests were rerun.

<!-- pdca:leaf-complete -->
