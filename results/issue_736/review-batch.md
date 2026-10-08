# Batched review — 3 passes, union of findings

- [ ] `crates/server/build.rs:154` **BUG** (seen by 1 pass): The explicit rerun directives omit the package/workspace source files consumed by the build, so an unstaged code edit can rebuild the binary without rerunning `git describe --dirty`, causing the binary to advertise a stale clean build identity.
- [ ] `crates/server/build.rs:139` **BUG** (seen by 1 pass): Restricting reruns to the index and two script sources means ordinary unstaged edits elsewhere rebuild the binary without rerunning version derivation, so it can falsely advertise a clean build without the required `.dirty` suffix.
- [ ] `crates/server/build.rs:174` **BUG** (seen by 1 pass): Skipping nonexistent watch targets means a repository with no `refs/tags` directory will not rerun this build script when its first loose tag is created, leaving the baked version stale.
- [ ] `crates/server/build.rs:136` **BUG** (seen by 1 pass): Watching only Git metadata and the index misses ordinary unstaged tracked-file edits, so Cargo can rebuild changed code without rerunning this script and the binary falsely advertises a clean build identity instead of the required `.dirty` version.

Triage rule: every finding above must be fixed (it then leaves the next run) or recorded-rejected in the decisions file ($PDCA_BUNDLE/review-rejected.md) as `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase from the finding's rationale — not re-reviewed to silence. The gate blocks while any finding here is unchecked.
