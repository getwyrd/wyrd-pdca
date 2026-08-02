# Recorded review rejections — issue 648

Format (the T4 triage rule): `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a
phrase from the finding's own rationale. A row here is a finding **declined with a reason**,
not a finding silenced: it stays declined wherever it re-lands.

crates/core/src/metadata.rs:244 | CONVENTION | while proposal 0016 remains | Out of scope for this slice, by the brief and by the target's own reviewer protocol. The brief's Scope section excludes "any new/edited ADR / spec / proposal (0016 §(a) names this an ADR-graduation candidate — that is the architecture board's, INTEGRATION §2/§4)", and `AGENTS.md` "Reviewer protocol / Out of scope" says a real finding outside the PR's stated scope gets a decline-with-issue-reference, not an in-PR fix. Graduating 0016 from `status: draft` is a document-lifecycle decision on the parent issue (getwyrd/wyrd#635), not something a code slice may perform for itself — a PR that edited the proposal's status to license its own code would be exactly the lifecycle inversion ADR-0037 forbids. Nothing in this patch depends on the status: the slice lands a shape, its decode invariants and its key helpers with **no producer and no resolver**, every legacy record stays byte-identical, and every consumer of the new variant fails closed, so the code is reversible if 0016 changes before it is accepted.

crates/core/src/metadata.rs:800 | CONVENTION | while proposal 0016 remains | Same finding, recorded at the other line the "proposal 0016 decision 7(a)" citation lands on (the `ChunkMap` doc comment), so the rejection binds wherever the finding is reported. Reason as above.
