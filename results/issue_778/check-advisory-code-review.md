- NEEDS-HUMAN [impl] — `crates/server/src/version/derivation.rs:205` (also `crates/server/src/version/derivation.rs:42`): Preserve Unicode whitespace in tag names until validation. Git accepts `v1.2.3\u{00A0}` and `v1.2.3\u{3000}`, but `str::trim()` silently removes those characters. A probe compiled against the target resolver returned `1.2.3`, `Rung::Describe`, and no warnings for both tags and plain `v1.2.3`, despite supplying different commit SHAs. Distinct commits can therefore advertise the same identity instead of rejecting the exotic tag and using its SHA. Strip only Git's output terminator at both trimming sites, and add regression coverage for trailing Unicode whitespace. This independently confirms the existing frozen T4 finding.

No additional correctness, reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->
