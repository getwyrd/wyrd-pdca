# Build notes — issue 648 (`chunkmap-flat-segmented-record-shape`), **iteration 6**

Withheld from the reviewer; written for the human at sign-off.

Round 5 ended with **two** blocking review findings, both of the same class — a
destructive path that now meets the new shape and has no test proving what it does there.
This round closes both with tests, adds two more of the same class the reviewer had not
yet reached, and changes **no production logic**: the production delta over
`iteration-v5/patch.diff` is one sentence of living-architecture prose. Everything else in
the patch — the shape, the codec, the decode invariants, the `seg:`/`seggrp:` helpers, the
fail-closed ripple, the acceptance test file — is byte-identical to round 5, which the
reviewer passed on C1/C2/C3/C4/C5/T1/T2/T3/T5 and which `cargo xtask ci` passed literally.

---

## 1. The carry-forward, and what it asked for

`review-batch.md` (round 5, union of 3 passes):

> - `crates/core/src/metadata.rs:1522` **TEST-GAP**: No test proves unlinking a segmented
>   inode fails without deleting its inode or dirent, leaving this destructive-path guard
>   unevidenced.
> - `crates/custodian/src/gc.rs:263` **TEST-GAP**: No seeded Tier-0 test proves GC aborts
>   on a segmented root before reclaiming fragments, despite the new shape reaching a
>   destructive maintenance path.

Both are correct, and both are the target's own standing rule rather than reviewer taste —
`AGENTS.md:188-190` (*Test fidelity*): "a new destructive or concurrent path lands with
seeded Tier-0 coverage". Round 5 had evidence for every guard on the **write-in** side
(`create`, `create_leased`, `commit_chunk_map`, `commit_chunk_map_superseding_leased`) and
none on the **destroy** side, which is the side where reading an unresolvable shape as "no
chunks" is unrecoverable. Neither was rejected; both are fixed.

## 2. What shipped this round — four tests, no logic change

| # | test | site under judgment | what it pins |
|---|---|---|---|
| 1 | `unlink_refuses_a_segmented_inode_rather_than_unbind_fragments_it_cannot_orphan` (`crates/core/src/metadata.rs:2656`) | the guard at `crates/core/src/metadata.rs:1522-1527` (`operation: "unlink"`, `:1526`) | the refused unlink leaves the **inode byte-identical**, the **dirent still bound**, and **no `orphan:` record** written; a flat sibling still unlinks and still deadlines its fragment |
| 2 | `a_segmented_root_aborts_the_pass_before_any_fragment_is_reclaimed` (`crates/custodian/tests/gc.rs:806`) | the guard at `crates/custodian/src/gc.rs:263-269` | seeded Tier-0 over the file's own trait stores: the pass returns the typed `ChunkMapError`, the segmented object's fragment survives, **and so does an unrelated collectable orphan** — i.e. the abort precedes the reclaim walk, it is not a per-fragment skip |
| 3 | `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids` (`:2733`) | the guard at `crates/core/src/metadata.rs:1898-1903` | id recovery fails closed instead of reporting `max_chunk = 0` for an object whose chunks it cannot enumerate; the same scan without the shape still reports `(2, 8)` |
| 4 | `every_chunk_map_commit_refuses_a_segmented_prior_instead_of_stranding_its_segments` (`:2529`, extended from round 5's `commit_chunk_map_…`) | the guard at `crates/core/src/metadata.rs:1630-1635` | the **superseding overwrite** is refused too, with the stored root unchanged and no `orphan:` record — an overwrite that cannot orphan the prior generation must not publish |

(3) and (4) are the same defect class as the two blockers at the two remaining guards a
reviewer would have reached next: they are both **silent** losses rather than errors —
(3) hands out a chunk id whose fragments are still live (issue #364's re-mint hazard, this
time triggered by a shape rather than a restart), (4) publishes an overwrite while
deadlining nothing. Together the four leave **no `.as_flat()` guard in `crates/core` or in
a destructive custodian path without a test**.

The only production edit this round is one clause in the living-architecture paragraph
(`docs/design/architecture/08-crosscutting-concepts.md:85`) stating the consumer half of
the contract — every existing consumer treats an unresolvable shape as a typed error *for
that object*, never as an empty chunk list. The paragraph previously documented only the
refuse-on-the-way-in half, while the patch ships the fail-closed ripple across 43 files.
Still the one paragraph the brief's Docs-currency line permits; no other doc touched.

## 3. What I deliberately did **not** change

* **Production logic — not one line.** The blockers were evidence gaps, not behaviour
  defects; the guards they name already did the right thing (proved in §4). Changing the
  shape or a guard to answer a test-gap finding would re-open a design the reviewer passed.
* **The acceptance test file** (`crates/core/tests/segmented_map_record.rs`) — byte-identical
  to round 5. Its capacity bound was round 5's blocker and was passed; churning it would put
  a settled argument back in front of a new reviewer for nothing.
* **`review-rejected.md`** — untouched. Both round-5 findings were *fixed*, so neither
  belongs there; the two standing rejections (0016's `status: draft`, recorded at both
  lines the finding lands on) were not re-raised in round 5 and stand as written.
* **The three surviving mutants** (advisory C5) — see §6. Still equivalent, still in
  pre-existing base lines.

## 4. Refuting my own test (the three questions)

**(a) Genuine red? — Yes. Each of the four new tests was run with *its own guard* reverted
to the "empty chunk list" reading, through the project's own runner
(`./engine/xtask.sh ci`, the configured C4-ci gate).** The flip is exactly the mistake the
guard exists to prevent: `as_flat().ok_or(SegmentedMapUnsupported{..})?` →
`as_flat().ok_or(..).unwrap_or_default()`.

*Probe 1 — the three `crates/core` guards flipped* (`unlink`, `commit_chunk_map_superseding`,
`high_water_marks`):

```
---- unlink_refuses_a_segmented_inode_rather_than_unbind_fragments_it_cannot_orphan ----
the call must fail closed: Some(Unlinked { outcome: Committed,
    inode: Some(InodeRecord { size: 12, chunk_map: Segmented(SegmentedMap { … }), … }) })
---- high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids ----
the call must fail closed: (2, 0)
---- every_chunk_map_commit_refuses_a_segmented_prior_instead_of_stranding_its_segments ----
the call must fail closed: Committed
test result: FAILED. 39 passed; 3 failed
```

Read the first one: with the guard removed the delete **commits** — the object is unbound
and its inode deleted, with zero orphan records for the fragments it owned. That is the
permanent leak C-1 forbids, produced on demand. The second returns `max_chunk = 0`, i.e.
the next PUT would mint an id the segmented object's fragments already occupy.

*Probe 2 — only the GC guard flipped* (run separately because `cargo test` stops at the
first failing target, so the custodian binary is not reached while `wyrd-core` is red):

```
---- a_segmented_root_aborts_the_pass_before_any_fragment_is_reclaimed ----
a committed map GC cannot resolve must abort the pass: Changed
test result: FAILED. 9 passed; 1 failed
```

`Reconciled::Changed` is the loop reporting that it **reclaimed fragment bytes** — the live
segmented object's among them. Both probe files were restored from copies under
`$PDCA_SCRATCH/pdca-builder-648-flip/` and verified byte-identical (`cmp`) before the final
artifacts were generated; `git diff | grep -c unwrap_or_default` = 0 on the shipped tree.

The **acceptance** file's red is unchanged and re-measured on the final artifact through
`./engine/scripts/run-verify.sh` (C4-verify): 14/14 green with the fix, and with
`crates/core/src/metadata.rs` reverted, 4 **assertion** reds on a file that still compiles
(`well_formed_segmented_root_decodes`, `segmented_root_round_trips_byte_identically`,
`segmented_root_cas_commits_against_the_stored_bytes`,
`segmented_root_at_max_root_segments_stays_inside_the_value_ceiling`) →
`run-verify.sh: PASS — red without the fix, green with it.`

Criterion 1's **demonstrated** red (required by the brief's Verification posture, because
byte-identity of the flat shape is trivially true on the base and so cannot flip on the C4
red leg) was performed in round 5 against the identical codec bytes shipped here: making
`ChunkMap::Flat` serialize as a tagged variant turned
`legacy_flat_record_round_trips_byte_identically` red and turned the legacy CAS into
`Conflict` (round 5 notes §5.3). The codec in this patch is byte-identical to the one that
demonstration ran against (`crates/core/src/metadata.rs` production region unchanged
between rounds 5 and 6 — verified: the only hunks that differ are inside
`mod segmented_shape_invariants`), so the demonstration stands without re-running it.

**(b) Production path? — Yes, for all four new tests.** (1), (3), (4) call the shipped
`metadata::{unlink, high_water_marks, commit_chunk_map, commit_chunk_map_superseding}`
against a **real `wyrd_metadata_redb::RedbMetadataStore`** (in-memory mode — a real backend
implementation, not a fake map), and assert on the store's own bytes afterwards. (2) drives
the **real fenced control point** `wyrd_custodian::reconcile_step` → `gc::reconcile` →
`gc::referenced_fragments`, the same entry the file's other criteria use, over the trait
stores that file already defines (the loop is deliberately proven over the seams — see its
header, `crates/custodian/tests/gc.rs:4-5`). Nothing is re-implemented or mocked: the
segmented root is raw stored bytes decoded by the production `metadata::decode`, and the
typed error is recovered by downcast from the production error seam.

**(c) Fixture includes the fault? — Yes, and this is where the GC test had to be built
carefully.** The GC fixture contains (i) the committed segmented root as raw stored bytes,
(ii) **a fragment of that object on d0, under a long-lapsed orphan grace record** — the
element the guard must protect, deliberately *not* curated out, and the exact input the
file's `reclaims_…` test proves GC deletes, and (iii) a second, unrelated collectable
orphan whose survival distinguishes "aborted before the walk" from "found nothing to do".
The unlink fixture holds the dirent *and* the inode, so the assertions can prove neither
was removed; had it held only the inode, a half-done delete would have passed. The
`high_water_marks` fixture is paired with a segmented-free store so the refusal is
attributed to the shape, not to the scan.

## 5. Gate runs (local, through the project's own runners)

| runner | result |
|---|---|
| `./engine/xtask.sh ci` (C4-ci, repo, gating) | **`xtask ci: all checks passed`** — typos, docs lint + render, gitlink/unsafe guards, `cargo fmt --check`, clippy `-D warnings`, build, the whole workspace test suite, cargo-deny, conformance vectors, statics, DST |
| `./engine/scripts/run-verify.sh` (C4-verify, bundle) | **PASS** — 14/14 green with the fix, 4 assertion reds with production reverted; re-run on the **final** patch.diff |
| `./engine/xtask.sh ci` ×2, guards flipped (probe) | **RED as designed** — §4(a); tree restored and verified byte-identical afterwards |
| `cargo fmt --all` (the target's formatter) | no changes on the shipped tree — the publish commit's hooks have nothing to rewrite |
| `scripts/mutants-in-diff` (C5, advisory) | `127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable` — the same three equivalent survivors as rounds 4–5, none in this slice's code (§6) |

## 6. C5 — the surviving mutants, again

Re-run on this round's patch: `127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable`
— identical to round 5. The survivors are `delete field size from struct
InodeRecord expression` at `crates/custodian/src/backfill.rs:133`,
`crates/custodian/src/rebalance.rs:301` and `crates/custodian/src/reconstruction.rs:589`.
Each of those struct expressions ends `..prior.clone()` / `..record.clone()`, so deleting
`size: …` produces a **byte-identical record** — an equivalent mutant no test can
distinguish. They are in-diff only because this patch changed the `chunk_map:` line beside
them; `git diff crates/custodian/src/backfill.rs` shows the `size:` line is not touched at
all. Killing them means deleting three lines of **pre-existing base code**, which the
target's reviewer protocol treats as an out-of-scope fix (`AGENTS.md:204-205`), and the
redundancy is defensive: if a struct base ever changed from `..prior.clone()` to
`..Default::default()`, the explicit `size:` is what keeps the record correct. Declined
with the reason recorded, not silently done. Round-5's reviewer reached the same verdict
(C5 PASS).

## 7. Budget — declared, including the part that is over

Measured as the brief defines it (added lines that are non-blank, non-comment, and not the
mechanical `.into()` / `.as_flat()` ripple), against `origin/main`:

| region | semantic lines |
|---|---|
| `crates/core/src/metadata.rs` — shape, codec, invariants, key helpers (**production**) | 609 |
| fail-closed guard lines across the 7 rippled `src` files (read/gc/backfill/rebalance/reconstruction/restore/server) | 107 |
| `crates/core/src/metadata.rs` — co-located invariant tests | 609 |
| `crates/core/tests/segmented_map_record.rs` — the acceptance target | 350 |
| `crates/custodian/tests/gc.rs` — the seeded Tier-0 leg added this round | 69 |
| **total** | **1 744** |

Production is **716** against the ~990 the brief sized the salvage region at — comfortably
under. The overage is entirely **test** code: 1 028 lines against the brief's "≤ ~1,500
… the headroom is for pruned tests", i.e. ~16 % over the approximate ceiling, of which
**176 lines are this round's** (121 in `metadata.rs`, 55 in `custodian/tests/gc.rs`) and
were added to close Check's two *blocking* findings and the target rubric's Tier-0 rule.
Non-mechanical files touched: **4** (`metadata.rs`, the acceptance test, `custodian/tests/gc.rs`,
the docs paragraph) against a ≤ 15 limit.

I did not hand back a split, and the human should know why: the brief's stop-rule exists to
prevent an unreviewable patch, but the thing over the line here is the evidence the last two
review rounds demanded, and a "split" that separated the shape from the tests that bind it
would ship an unproven codec. If the size is nonetheless judged too large at sign-off, the
cheapest honest prune is **not** any of this round's four tests but the co-located key-space
group — `segment_key_round_trips_over_the_whole_addressable_index_space`,
`a_group_prefix_can_never_alias_another_generations_epoch_range`,
`a_segment_index_past_the_key_space_is_neither_a_key_nor_a_value`,
`only_canonical_epoch_spellings_address_a_segment` (`crates/core/src/metadata.rs:2088-2268`,
~180 semantic lines) — which bind the `seg:` key grammar that only #649 consumes. That would
land the total at ~1 564. I left them in because #649 depends on that grammar being right
and this is the slice that ships it.

## 8. Self-review against the target's standing rubric (`AGENTS.md:122-210`)

Run as the last step before the artifacts were generated, on the shipped tree:

* **One clock per correctness lifecycle** — no new clock read. Every instant in the new
  tests is the caller's logical millis, passed in (`unlink(&store, 1, "obj", 7)`;
  `reconcile_step(…, 1_000_000)`), which is the same source the surrounding tests use.
* **Narrow trait seams / dependency direction** — the stored grammar and its typed error
  stay in `wyrd_core::metadata`; `crates/custodian` consumes them through the dependency
  it already has. No new dependency, no new dev-dependency, no reversal.
* **Metadata validation boundaries (ADR-0045)** — structural invariants raise at decode and
  surface as errors; the `as_flat()` refusals are *maintenance-strict* reads of a shape,
  which is the "liberal on read, strict in maintenance" side of the rule.
* **No DST-reachable shared mutable global state** — none added (statics gate green).
* **`#![forbid(unsafe_code)]`** — no new crate; the added test file carries it (`:32`).
* **Docs currency** — the persisted-record change updates the living architecture doc in
  the same patch, one paragraph (`docs/design/architecture/08-crosscutting-concepts.md:85`).
* **Absent/unsupported entries** — an unresolvable shape is an explicit typed error at
  every consumer, never a silent skip or an empty list; that is now the *tested* claim.
* **Serialization identity** — decode→encode byte-identity for the legacy shape has its
  round-trip test and its CAS test (criterion 1).
* **Transactions** — every refusal returns before `store.commit(batch)`; the `WriteBatch`
  is a value, so there is no live transaction to roll back, and no partial write can escape.
* **Test fidelity — the rule this round exists to satisfy**: the two destructive paths the
  new shape reaches (`unlink`, GC reclamation) now land with seeded Tier-0 coverage, plus
  the two remaining silent-loss guards.
* **Reviewer protocol** — nothing re-raised that was deferred; the two out-of-scope
  declines stay recorded in `review-rejected.md` with their reasons rather than silenced.

## 9. Open for the human at sign-off

* Nothing was unverifiable this round; no external dependency was missing. `typos` and
  `docs-renderer` (the brief's declared externals) are both present and ran inside
  `cargo xtask ci`.
* The standing T4 NEEDS-HUMAN is unchanged and not something a patch can close: the review
  gate's scanner (`scripts/review-branch`) and the contribution checker
  (`scripts/pdca contribcheck`) live in the PDCA project, not in the target, so a reviewer
  reading only the target cannot reproduce their verdicts. The two findings they reported
  in round 5 *are* reproduced and closed here (§4a) — the flips show precisely what each
  missing test would have failed to catch.
* Scratch: `$PDCA_SCRATCH/pdca-builder-648-flip` (guard-flip probe backups + logs),
  `$PDCA_SCRATCH/pdca-builder-648-v5` (round-5 reconstruction for the line-count delta),
  and three `pdca-builder-648-*.log` files under `$PDCA_SCRATCH`. All removed at the end of
  the round; no `/tmp` path was used.
