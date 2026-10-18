# Adversarial review — issue #722 (segmented drain evacuation), iteration 4

**Verdict: I could not refute the fix.** I re-ran the red→green proof, re-ran every named negation the brief requires, and tried five input shapes aimed at the new addressing and containment code. All of them held. One item needs a human decision: the brief and the base disagree about which value ceiling applies to a `seg:` record.

## The evidence holds up

- **Red→green reproduced in a scratch copy, not taken from the log.** Patched tree: `segmented_map_evacuate` 12/12 green, `segmented_map_rebalance` 5/5 green. With only `crates/custodian/src/rebalance.rs` reverted: 11 of 12 red. Leg 1 (`crates/custodian/tests/segmented_map_evacuate.rs:434`) and leg 2 (`:492`) fail on `Blocked` vs `Changed`, which is the right reason: the base refuses the segmented object. Every leg goes through the real `reconcile_step` and checks the store afterwards, so no leg re-implements production logic or passes by tautology. Leg 3 is the one that passes on the base, which the brief predicted.
- **The DST named negations are real** (run with `--cfg madsim`, `MADSIM_TEST_NUM=50`). Deleting the `seg:` precondition at `crates/core/src/metadata.rs:3324` makes `drain_repoint_versus_supersede_is_all_or_nothing` fail at `crates/dst/tests/custodian.rs:6167`. Deleting the root precondition at `crates/core/src/metadata.rs:3257` makes it fail at `:6163`. Both mutations were reverted afterwards.
- **The custodian named negations are real too.** Removing the ceiling check in `weighed` (`crates/core/src/metadata.rs:3339-3346`) turns legs 3 and 5 red (`segmented_map_evacuate.rs:563`, `:683`). Removing the `seg:` precondition turns leg 4 red. Iteration 2's hand mutation (pin the scan's snapshot instead of the resolved record, at `crates/custodian/src/rebalance.rs:439`) now turns leg 12 red, so that round's gap is closed.

## Attempts to break the fix (none landed)

- **Resolve restarts onto a FLAT successor** (`rebalance.rs:364-367`, `:436-442`). The scan reads a segmented root, then a flat root naming the drained chunk replaces it mid-resolve. Result: `Changed`, the flat root is repointed, and the drain status is `Satisfied`. The plan's offset and the pinned root both come from the successor.
- **One chunk named by a flat object and a segmented object.** Both references are repointed in one pass (`Changed`, `Satisfied`). Copying the same fragment to the same server twice is safe in production as well as in the test double: `FsChunkStore` treats a same-id rewrite as a no-op (`crates/chunkstore-fs/src/lib.rs:307`).
- **Two drained chunks in one `seg:` record.** Both move in one pass. The second move re-reads the record, sees the first move's write, and commits on top of it.
- **A flat object owing two moves.** The passes answer `Blocked`, `Changed`, `Satisfied`. The second plan copies its fragment and then loses the root CAS (compare-and-swap) in pass 1. The **base gives exactly the same sequence**, so this is the existing #723 leak class, not something this patch adds.
- **Over-broad containment.** The non-canonical-key guard and the overflow guard (`rebalance.rs:413-424`) only run after `evac.is_empty()` has already skipped the chunk (`:410`). So a healthy object, or an oddly keyed one, that owes nothing on the draining server never blocks certification.
- **The removed `refused-segmented` event and `rebalance_refused_records` counter** (deleted at old `rebalance.rs:554-563`). Nothing in `docs/` or the rest of the tree names either, so no docs page or alert goes stale.

## Items for the human

- NEEDS-HUMAN [human] — **Leg 3 ignores an explicit brief instruction, and the base shows the brief was wrong.** The brief says to key the ceiling fixture "to **V/2, not `MAX_VALUE_BYTES`** — #721 settles that a `seg:` record is weighed at V/2" (`brief.md:40-45`, repeated at `:142`). The merged primitive does the opposite: it weighs a `seg:` record against the full `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:593-600`, `:3339-3346`) and keeps the V/2 budget (`MAX_ROOT_VALUE_BYTES`, `:574`) for root tables only. So a fixture at V/2 would commit, and the builder keyed leg 3 to `MAX_VALUE_BYTES`, saying so in the test's doc comment (`segmented_map_evacuate.rs:548-551`). The builder was right to follow the base, and full-V is consistent with the resolver reading `seg:` rows up to `MAX_VALUE_BYTES`. But the brief's author expected V/2. Someone needs to confirm that #721's full-ceiling rule is the intended contract for `seg:` records. If V/2 was meant, the fix belongs in #721's primitive, not in this slice.

## Notes (no action needed in this slice)

- **The C4-diff-cov MISS lines are a tooling artifact.** `rebalance.rs:329` and `:356` (the scan-time `contain` calls) show as unexecuted only because the coverage run executed `segmented_map_evacuate` and the DST crate alone (`gate-logs/C4-diff-cov.log:25-192`). `segmented_map_rebalance.rs:393-417` covers both lines, and C4-ci runs it.
- **Stale doc line outside this slice.** `crates/core/src/metadata.rs:3238` still says "The custodian's reconstruction pass is the caller". After this patch the drain is a second caller. `metadata.rs` belongs to #721 and the brief's four-file budget rules out editing it here, so this is a follow-up, not a defect. In the same way, `docs/design/architecture/06-runtime-view.md:41` describes Rebalance in one generic line, while `:40` documents the reconstruction move in detail. The rubric's docs-currency rule (ports, API operations, RPCs, CLI flags, persisted fields) is not triggered.
- **I can't check the T4 row.** `check-gates.json` reports `T4-batch-review` as 0 blocking after 29.9 s for "3x codex". Its frozen log is a single summary line pointing at `review-batch.md`, which I wasn't given. My own pass found nothing that contradicts a clean review.

<!-- pdca:leaf-complete -->
