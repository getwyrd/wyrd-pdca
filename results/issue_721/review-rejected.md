# Recorded rejections — issue_721

Format (read by the batched-review triage): `<file:line> | <CLASS> | <MATCH> | <reason>`
where MATCH is a phrase from the finding's own rationale.

**Provenance:** drafted at **Do**, iteration 2, for the human to confirm at sign-off. Its
substance is not a builder opinion — it is this child's brief's own scope carve-out ("Out of
scope: … the DST repoint-versus-supersede property (**#722** — this child must leave
`crates/custodian/src/rebalance.rs`, `crates/custodian/tests/segmented_map_rebalance.rs` and
`crates/dst/tests/custodian.rs` untouched)"). Overrule it by deleting the rows.

Iteration 2 note: **three of the four** iteration-1 batch findings are *fixed*, not rejected, and
should leave the next run — the root/record extent trio (`crates/core/src/metadata.rs:2898`,
`:2903`, `:2904`, one class seen at three lines), closed by comparing the freshly-read
`SegmentRecord`'s `byte_offset`/`byte_len` with the selected `SegmentRef` before any rewrite and
answering `Repoint::Conflict` on a disagreement — the resolver's own check, field for field
(`read_segments`, `crates/core/src/metadata.rs:2582-2589`). It is race-tested end to end through
`reconcile_step` (leg 6, `crates/custodian/tests/segmented_map_repoint.rs:733`) and unit-tested at
the primitive (`crates/core/src/metadata.rs:3328`). What remains below is the one finding this
child's brief puts in another slice; the same decision is recorded at each line the class could
be reported at, since the file's lines moved between iterations.

Iteration 3 note: **both** of the iteration-2 batch findings that were this diff's own are
*fixed*, not rejected, and should leave the next run — the unchecked generation advance
(`crates/core/src/metadata.rs:2844`, now `checked_add` answering the typed
`ChunkMapError::VersionExhausted`) and the structural decode collapsed into a conflict
(`:2892`, now the typed `SegmentRecordUndecodable` propagated and *contained* by the caller at
`crates/custodian/src/reconstruction.rs:311-327`). Each is unit-tested at the primitive
(`crates/core/src/metadata.rs:3340`, `:3368`), the second is race-tested end to end
(`crates/custodian/tests/segmented_map_repoint.rs:692`), and both carry a demonstrated named
negation in `build-notes.md`. The DST rejection below is unchanged in substance and is re-recorded
at this round's reported locations as well.

crates/custodian/tests/segmented_map_repoint.rs:543 | TEST-GAP | seeded Tier-0 DST coverage | Rejected — **out of this child's scope by the brief, and owned by a named successor.** #721's brief assigns the repoint-versus-supersede DST property to **#722** and forbids this child from touching `crates/dst/tests/custodian.rs`, `crates/custodian/src/rebalance.rs` or `crates/custodian/tests/segmented_map_rebalance.rs`, under a 4-file budget a DST leg would break outright. The concurrency this child adds is not left unproven meanwhile: the read→prepare window and the commit window are both driven **deterministically** through the real fenced control point `reconcile_step`, over the production `MetadataStore`/`ChunkStore` seams, in four legs — a sibling's concurrent move MERGES (leg 2), the planned chunk's own move CONFLICTS (leg 3), a superseding root generation CONFLICTS (leg 4), a root-inconsistent rewrite CONFLICTS (leg 6) — three of which carry a fixture self-check (`meta.raced()`) so they cannot pass because the race silently never happened (leg 4's flip fires inside the commit the base never reaches, so it is asserted through its negation instead). Each non-red leg carries a *demonstrated* named negation in `build-notes.md` (delete the pin, watch the leg go red), which is the mutation evidence a seeded DST leg would not by itself provide, and `scripts/mutants-in-diff` reports 0 missed mutants over the whole diff. `cargo xtask dst` was run on the patched tree and is green (nothing there asserted the removed refusal). Widening the DST campaign is #722's work, not a defect of this diff.
crates/custodian/tests/segmented_map_repoint.rs:1 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:543` row above, recorded here because the file's lines moved between iterations and this class can be reported against the module rather than a leg. Evidence and reasoning as in that row: #722 owns the DST property, the brief forbids the files it would touch, and the concurrency is proven deterministically through `reconcile_step` with per-leg named negations.
crates/custodian/tests/segmented_map_repoint.rs:733 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:543` row above, recorded at the leg this iteration added (the root-inconsistent race). #722 owns the DST property; the brief forbids the files a DST leg would touch; the race is driven deterministically through the real fenced control point with a fixture self-check and a demonstrated named negation.
crates/custodian/src/reconstruction.rs:878 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:543` row above, recorded at the production call site the class can be reported against. #722 owns the repoint-versus-supersede DST property and this child must leave `crates/dst/tests/custodian.rs` untouched; `cargo xtask dst` is green on the patched tree.
crates/core/src/metadata.rs:2779 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `crates/custodian/tests/segmented_map_repoint.rs:543` row above, recorded at the line iteration 2's batch review reported this class at (the primitive in `crates/core`). #722 owns the repoint-versus-supersede DST property, the brief forbids the three files a DST leg would touch, and a fifth file would break this child's 4-file budget; the concurrency is instead driven deterministically through the real fenced control point `reconcile_step` in five legs, each with a fixture self-check and a demonstrated named negation, with 0 missed mutants over the whole diff.
crates/core/src/metadata.rs:2822 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:2779` row above, recorded at `repoint_chunk`'s current line — the file's lines move every iteration and this class is reported against the primitive itself. #722 owns the DST property; the brief forbids the files a DST leg would touch.
crates/core/src/metadata.rs:2892 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:2779` row above, recorded at the segmented arm's own read — the concurrent path a DST leg would exercise. #722 owns the DST property; the brief forbids the files a DST leg would touch.
crates/custodian/src/reconstruction.rs:313 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:2779` row above, recorded at the containment arm iteration 3 adds — the one new concurrent path in this file. #722 owns the DST property; the brief forbids `crates/dst/tests/custodian.rs`, and this path is driven end to end by `segmented_map_repoint.rs:692` with a demonstrated named negation.
crates/custodian/src/reconstruction.rs:889 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:2779` row above, recorded at the current production call site (the `:878` row above is its iteration-2 line). #722 owns the repoint-versus-supersede DST property and this child must leave `crates/dst/tests/custodian.rs` untouched.
crates/custodian/tests/segmented_map_repoint.rs:373 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:543` row above, recorded at the shared race driver `raced_and_lost` this iteration factors out — the class can be reported against it rather than a leg. #722 owns the DST property; the brief forbids the files a DST leg would touch.
crates/custodian/tests/segmented_map_repoint.rs:692 | TEST-GAP | seeded Tier-0 DST coverage | Same rejection as the `:543` row above, recorded at the leg this iteration added (the torn-record containment race). #722 owns the DST property; the race is driven deterministically through the real fenced control point with a fixture self-check and a demonstrated named negation.
