No findings on either advisory lens: introduced correctness bugs, or actionable reuse, simplification, and efficiency changes.

- Correctness: `crates/core/src/metadata.rs:2218` and `crates/core/src/metadata.rs:2330` derive the timestamp from the same prior record their respective CAS guards protect. The shared helper at `crates/core/src/metadata.rs:2269` preserves absent publisher timestamps and uses saturating arithmetic. Creation, repair metadata, and lease clocks remain unchanged.
- Reuse and test fidelity: both overwrite APIs share one constant-time helper. `crates/server/tests/last_modified_monotonic.rs:129` and `crates/server/tests/last_modified_monotonic.rs:155` exercise real gateway overwrites; `crates/server/tests/last_modified_monotonic.rs:297` checks exact values through both core APIs, including saturation, repeated writes, and the documented unstamped reset. No worthwhile simplification identified.

Frozen evidence: C4-verify reports five passing tests with the fix and three assertion failures without it; C5 reports three caught mutants and two unviable, with none surviving. C4-ci fails on rustls dependency advisories outside this diff. Diff coverage remains unmeasured: its log shows a streaming API argument-count compilation error, whereas the supplied target's signature at `crates/gateway-core/src/lib.rs:330` accepts the test's arguments. This evidence discrepancy is not a demonstrated defect in this patch. No builds were rerun and no target files were modified.

<!-- pdca:leaf-complete -->
