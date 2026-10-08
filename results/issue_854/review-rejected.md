# Recorded rejections — issue #854

Format (the gate's triage rule): `<file:line> | <CLASS> | <MATCH> | <reason>`.
Recorded at Plan on 2026-10-04 for the human's sign-off decision of 2026-09-30 (brief.md,
Standing decision 4). Lines are round-4 tree lines; the gate re-anchors each to the nearest
matching finding in the same file.

crates/validate/src/s3.rs:351 | TEST-GAP | Tier-0 | **Settled by the plan (brief.md Standing decision 4).** The seeded Tier-0 DST rule (`AGENTS.md:188-190`) cannot apply to `crates/validate`: it has no `madsim` build, `crates/dst` does not depend on it, the blackbox guard keeps every `wyrd-*` crate out of its normal dependencies (proposal 0017 §9), and the PUT lifecycle is a real OS thread, tokio runtime and socket, none of which madsim models. No rebuild can satisfy the finding. The coverage is the loopback suite in `crates/validate/tests/s3_client_upload_peers.rs`, which drives the production client.
crates/validate/src/s3/body.rs:230 | TEST-GAP | Tier-0 | **Settled by the plan (brief.md Standing decision 4).** Same reason as the `s3.rs` row: `crates/validate` is outside the DST build by design (no `madsim` build, no `wyrd-*` normal dependency, real thread/runtime/socket), so seeded Tier-0 coverage of the request/body ordering is out of scope; the loopback suite covers it.
crates/validate/tests/s3_client_upload_peers.rs:894 | TEST-GAP | Tier-0 | **Settled by the plan (brief.md Standing decision 4).** Same reason as the `s3.rs` row: no seeded Tier-0 DST exists or can exist for `crates/validate`; this loopback suite is the coverage.
