# Recorded review rejections — issue #841 (iteration 3)

Format (the T4 gate's triage rule): `<file:line> | <CLASS> | <MATCH> | <reason>`.

Of the six findings in `review-batch.md`, the three clean-verdict BUGs (`restore.rs:713`,
`:1315`) are **fixed** in this iteration (`crates/custodian/src/restore.rs:1320-1321`, covered by
`a_fence_fault_on_an_otherwise_clean_store_is_never_certified_clean`). The three rows below are
one finding class — seeded Tier-0 DST coverage for the fence — declined as out of scope with an
in-code deferral marker (`crates/custodian/src/restore.rs:722` and
`crates/custodian/tests/restore_open_fence.rs:15`, `// deferred: #843`), per the target rubric's
*Deferrals are settled* and *Out of scope* rules. The brief assigns DST coverage to child-5
("DST coverage (child-5; leave `crates/dst/tests/custodian.rs` untouched …)"), and child-5 is
open as getwyrd/wyrd#843, "dst: seeded Tier-0 coverage for the restore session fence (809.5)".
The sign-off human may overrule this deferral.

crates/custodian/tests/restore_open_fence.rs:566 | TEST-GAP | retirement-producing fence has only Tokio tests with synchronous commit hooks | Deferred — tracked in #843 (child-5 of #809, the seeded Tier-0 DST coverage of this fence). The brief puts DST out of this child's scope; markers at `crates/custodian/src/restore.rs:722` and `crates/custodian/tests/restore_open_fence.rs:15`.

crates/custodian/tests/restore_open_fence.rs:566 | TEST-GAP | concurrent teardown path has only Tokio tests with scripted commit hooks | Same finding, same deferral: Deferred — tracked in #843; see the row above.

crates/custodian/tests/restore_open_fence.rs:566 | TEST-GAP | retirement-producing fence has only Tokio tests with synchronous race injection | Same finding, same deferral: Deferred — tracked in #843; see the first row.
