- NEEDS-HUMAN [impl] — `crates/validate/tests/payload_and_sizes.rs:284`: Bound the huge-range piece-size check with the existing `within` helper. This test synchronously collects tails near 10 GiB and 1 TiB, outside any timeout. If `range` or the shared `fill_inside` regresses to generating and skipping the prefix, the separate timed test at line 359 fails after five seconds, but this test keeps running; libtest waits for it, so the suite can take hours to exit. Wrap the huge-range work here in `within` too, so the regression this patch explicitly targets produces a bounded test failure.

No production correctness defects or material reuse, simplification, or efficiency findings identified. Frozen CI evidence records all 22 new tests and 21 existing CLI tests passing. An independent in-memory implementation matched all six pinned payload digests and both large-offset tail vectors; Cargo gates were not rerun against the read-only target.

<!-- pdca:leaf-complete -->
