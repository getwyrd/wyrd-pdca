# Recorded review rejections — issue #691

Format (read by `scripts/review-branch`): `<file:line> | <CLASS> | <MATCH> | <reason>`.
`MATCH` is a case-insensitive substring that must occur in the finding's rationale.

Written by the builder at iteration 2 for the **one blocking finding** iteration 1's T4
batched review raised. It is a disposition proposal, not a decision: the human overrides it
at sign-off by deleting the row (which re-blocks the gate) — see `build-notes.md`
§"The T4 docs-currency finding".

Why it is declined rather than fixed, in one line: **this slice persists nothing.** The
docs-currency rule fires on "a port, an API operation, an RPC, a CLI flag, or a persisted
field" (`AGENTS.md:154-157`); this patch adds a key *grammar* with no writer, no store call
and no production consumer, so no field of it is persisted yet. The living architecture doc
"always describes the current system" (`docs/design/README.md:28`, `AGENTS.md:98-99`), so
documenting records nothing emits would make it describe a system that does not exist. The
namespaces enter `docs/design/architecture/05-building-block-view.md` with the slice that
first *writes* one (#656–#659) — the reviewer protocol's decline-with-issue-reference for a
finding outside the PR's stated scope (`AGENTS.md:204-205`). Until then the normative
description of this key space is proposal 0016 §1, already merged on `main`
(`docs/design/proposals/draft/0016-multipart-commit-protocol.md:333-356`), and the module
now states this disposition inline at `crates/core/src/multipart.rs:55-64`.

crates/core/src/multipart.rs:458 | CONVENTION | living architecture | Nothing in this slice is persisted (no writer, no store call, no production consumer), so the docs-currency trigger — a persisted field — is not met; the living doc gains these namespaces with the slice that first writes one (#656–#659). Scope is pinned to 3 files by the brief.
crates/core/src/multipart.rs:458 | CONVENTION | docs-currency | Same finding, other phrasing: docs-currency fires on a persisted field; this patch persists nothing.
crates/core/src/multipart.rs:468 | CONVENTION | living architecture | Same finding re-cited at the prefix-constant block after the iteration-2 rebuild shifted line numbers.
crates/core/src/multipart.rs:468 | CONVENTION | docs-currency | Same finding, other phrasing, at the shifted line.
crates/core/src/multipart.rs:55 | CONVENTION | living architecture | Same finding re-cited at the module header where the disposition is now stated inline.
crates/core/src/multipart.rs:55 | CONVENTION | docs-currency | Same finding, other phrasing, at the module header.
crates/core/src/multipart.rs:475 | CONVENTION | living architecture | Same finding re-cited at `MPUCTL_KEY` after the iteration-3 rebuild shifted the prefix block again.
crates/core/src/multipart.rs:475 | CONVENTION | docs-currency | Same finding, other phrasing, at the iteration-3 line.
crates/core/src/multipart.rs:481 | CONVENTION | living architecture | Same finding re-cited at `MPU_PREFIX` after the iteration-3 rebuild shifted the prefix block again.
crates/core/src/multipart.rs:481 | CONVENTION | docs-currency | Same finding, other phrasing, at the iteration-3 line.
