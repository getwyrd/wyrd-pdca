# Recorded rejections — issue #776 (`<file:line> | <CLASS> | <MATCH> | <reason>`)
#
# One decision. It re-anchors to the nearest CONVENTION finding in the file whose rationale
# carries the MATCH phrase, so a line shift from a doc edit does not re-open it.
# To re-open the finding (e.g. to widen the child to two files), delete the line below.

crates/core/src/metadata.rs:3232 | CONVENTION | architecture | Deferred — tracked in #777 (in-code `deferred: #777` marker on `repoint_chunk`'s doc, the tree's own form, e.g. `custodian/src/backfill.rs:112`). The rubric's Docs-currency trigger is "a port, an API operation, an RPC, a CLI flag, or a persisted field"; `repoint_chunk` is none of these — an in-crate library function with zero callers, no wire, trait-seam or persisted change, and its peers `commit_chunk_map` / `resolve_chunk_map` appear nowhere under `docs/design/architecture/`. The living doc describes what the maintenance loops DO (`06-runtime-view.md:40` §6.3, `08-crosscutting-concepts.md:85` §8.7), and until #777 wires this in they do exactly what it says. The brief (Plan's authority) forbids a second file in this child (`brief.md:83-95`, "STOP and hand back"); the doc update lands with #777, where the behaviour changes. The C1 NEEDS-HUMAN item on this same point stays in §6 for the human to confirm or overturn at sign-off.
