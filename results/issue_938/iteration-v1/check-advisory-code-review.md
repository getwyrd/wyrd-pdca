- NEEDS-HUMAN [impl] — `crates/validate/src/s3/verbs.rs:127`: The new public client operations land without the same-PR living architecture update required by the standing docs-currency rule. `docs/design/architecture/05-building-block-view.md:299` still describes the earlier client; update it for the added verbs, preserved response heads, pagination limit, and bulk-delete results. This corroborates the existing T4 batch-review finding.

No introduced runtime correctness bugs or substantive reuse, simplification, or efficiency findings identified. Frozen CI evidence records all 12 new smoke-verbs tests and the three existing client suites passing; no tests were rerun against the read-only target.

<!-- pdca:leaf-complete -->
