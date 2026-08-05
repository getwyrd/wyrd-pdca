# Adversarial review — issue 651 (advisory, non-gating)

Evidence re-run independently at `$PDCA_TARGET`, not taken on the bundle's word:

* **Green post-fix** — `cargo test -p wyrd-custodian --test segmented_map_restore` → 7/7 pass.
* **Red pre-fix** — in a scratch copy with `restore.rs`, `desired_state.rs`, `cli.rs` and both
  modified test files reverted to `origin/main` (d50f0ca) and only the discriminator kept:
  **6 of 7 fail, all on assertions/behaviour, none on a compile error** — `SegmentedMapUnsupported`
  for (1)/(2a)/(2b), an empty audit capture for (3), `misplaced:[45312]` vs `dangling` for (4a),
  `stranded_marked: 1` for (4b). The 7th (`..._no_other_object_claims_...`, criterion 4c) passes on
  base *by design*. The discriminator drives `wyrd_custodian::reconcile_after_restore` /
  `desired_state::reconciliation_status` — the real entry points, not a parallel re-implementation.
  So the C4-verify row is **not** refutable, and the red is for the right reason.

The findings below are all against the *fix*, not the evidence.

- **NEEDS-HUMAN [human] — the collision path the patch now documents as reachable cannot produce
  the state the patch keys on, and the one it *does* produce is still uncovered.**
  `docs/design/architecture/m4-first-deployment-blueprint.md:693-704` replaces the old
  "chunk ids are random, a reused inode cannot collide" claim with: the CLI path mints
  `(inode << 64) | seq`, so "a reused inode *does* re-mint the ids of the post-*V* file that held
  it. Post-restore reconciliation does not assume otherwise — … where the restored namespace shows
  one chunk id claimed by more than one committed object it withholds both the 'restage it' verdict
  and any reclamation mark." The post-*V* file is precisely the record the restore **removed**, so
  it is not a committed claimant; that mechanism yields `claims == 1`, and `CommittedChunks::ambiguous`
  (`crates/custodian/src/restore.rs:562`) never fires. Executed on the target: one committed object
  at `d0` with an empty placement, plus the dead object's same-id fragment on an unnamed server →
  `RestoreReport { dangling: [], misplaced: [91], displaced_kept: 1 }`, and `restore_verdict`
  (`crates/server/src/cli.rs:1303-1309`) then prints *"Restage those fragments onto the placed D
  servers … Do NOT go to a backup: the data is here"* — i.e. write the dead file's bytes under the
  live file's map. That is exactly the corruption criterion (4a) exists to prevent, in the one
  scenario the diff itself now advertises as reachable. Iteration 8's carry-forward asked to
  *"either name the actual reachable collision path, or fold this concern into #652"*; the path
  named does not reach `claims > 1`, so that question is still open and is a scope call
  (widen the oracle beyond committed references, or drop the doc's coverage claim and defer to #652).

- **NEEDS-HUMAN [impl] — an ambiguous chunk id that never becomes `dangling` is completely silent,
  and the run is certified clean.** `emit_ambiguous_evidence` is reachable only from inside the
  `anywhere < k` arm (`crates/custodian/src/restore.rs:497-504`); the mark half's withhold at
  `restore.rs:358-366` emits `emit_displaced` instead. Executed on the target: two committed objects
  claiming chunk `71`, both healthy at their shared placement `d0`, plus one genuine stray copy on
  `d9` → `RestoreReport { stranded_marked: 0, displaced_kept: 1, dangling: [], misplaced: [],
  unresolvable: [] }`, `is_clean() == true`, `needs_human() == false`. So the CLI prints
  "post-restore reconciliation **complete**" and exits **0**, on every re-run forever, while the pass
  has permanently stopped reclaiming that id — and the only signal emitted is `emit_displaced`, whose
  text (`restore.rs:795-800`) ends *"The placement is stale, not the data; repair repoints it"*, an
  instruction that is wrong here (nothing is stale; the id is duplicated). The brief's own invariant is
  "a pass never reports a conclusion it could not reach" and "an incomplete reading is attributed, not
  merely signalled" — the withheld reclamation is a conclusion it could not reach, and it is neither
  attributed nor allowed to affect the verdict. Fix is cheap and needs no new report class (the brief
  declines one): emit `ambiguous-chunk-id` wherever the ambiguity actually changed a decision, not only
  in the dangling arm.

- **NEEDS-HUMAN [impl] — the ambiguity gate is placed above the `already`-marked check, so a mark an
  earlier run wrote is reported as "kept" while GC still deletes it.** `restore.rs:358-366` `continue`s
  before `already.contains_key(&(dserver, frag))` at `restore.rs:395`, and this pass never deletes an
  `orphan:` record. Executed on the target: chunk `72` claimed by two committed objects, a stray copy on
  `d9`, and an `orphan:` record for `(9, frag(72,0))` already on disk → `displaced_kept: 1`,
  **`already_marked: 0`**, the orphan record still present, `is_clean() == true`. `gc::reconcile`
  (`crates/custodian/src/gc.rs:191-214`) gates only on `ReferenceSet::protection`, which has no
  ambiguity clause, so it reclaims that fragment as soon as the grace window elapses — the (4b)
  data-loss leg, reached through a mark written before the second claimant existed (exactly the m4
  inode-reuse timeline) rather than by this run. Note this is a **regression introduced by the diff**:
  on `origin/main` the same store reports `already_marked: 1` (honest), because the fragment fell
  through to `:395`. Minimum fix: move the ambiguity gate below the `already` check so the count stays
  truthful, and decide whether an ambiguous id's stale mark should be retracted (a mark is an
  authorization to delete, and `emit_displaced` claims the fragment is "kept, never marked").

- **NEEDS-HUMAN [impl] — `is_clean()` has no true-branch assertion anywhere in the tree, so
  criterion (2a)'s central claim is one-sided.** `crates/custodian/src/restore.rs:178`. Every use in the
  repo is negative (`segmented_map_restore.rs:556`, `restore_reconcile.rs:729`, `:881`,
  `cli.rs:2749`'s `!(human && report.is_clean())`); `fn is_clean(&self) -> bool { false }` passes the
  entire suite — which is precisely the C5 row's `MISSED restore.rs:179:9: replace
  RestoreReport::is_clean -> bool with false` (and `:179:30 == → !=`). The bundle asserts
  "an incomplete reading is never clean" without ever asserting "a complete, healthy reading **is**".
  Verified on the target that the true branch is reachable and correct (a single healthy chunk →
  `is_clean() == true`), so the assertion costs one line.

- **NEEDS-HUMAN [impl] — the two remaining C5 misses are both on lines this patch added, and both are
  genuinely unasserted.** (a) `restore.rs:359` `report.displaced_kept += 1` in the ambiguous arm —
  `+= → *=` survives, i.e. no test pins the counter on the ambiguity path (criterion (4b) asserts only
  `stranded_marked == 0` and the absent `orphan:` record). (b) `restore.rs:503`
  `emit_ambiguous_evidence(chunk, committed.claims(chunk), by_id_alone - anywhere)` — `- → +` survives:
  `segmented_map_restore.rs:773-776` greps only for `"action":"ambiguous-chunk-id"` and the chunk hex,
  never for `claims` or `withheld`, yet `withheld` is the number that tells the operator the bytes are
  still on disk and that an older backup may be unnecessary. Both are pinnable in the C4-ci-gated
  `restore_reconcile.rs` without touching the discriminator's compile-safety constraint.

- **NEEDS-HUMAN [impl] — the UNREADABLE operator paragraph asserts a fleet-wide fact the report field
  cannot carry.** `crates/server/src/cli.rs:1314-1320` states unconditionally *"Nothing of theirs was
  marked — and nothing anywhere in the fleet was"*. But `report.unresolvable` is the **union** of both
  walks (`restore.rs:296` → `name_unresolvable`, `restore.rs:684-701`), while marking is gated only on
  `referenced.protects` (`restore.rs:339`), i.e. on the *first* walk's `unresolvable`. The second walk
  (`committed_chunks`, `restore.rs:290`) is a separate read of the same records, and `restore.rs:399-402`
  explicitly contemplates running this pass against a live cluster — a `seg:` record removed between the
  two reads lands in `committed.unresolvable` only, leaving the mark gate open. The command then prints
  "nothing anywhere in the fleet was [marked]" on one line and a non-zero `stranded_marked` on the line
  above it. Derive the sentence from `report.stranded_marked` (or from a flag set by the walk that
  actually gated), rather than asserting it.

## Attempted and could not refute

- **Arithmetic underflow at `restore.rs:503`** (`by_id_alone - anywhere`, with `anywhere == placed`
  under ambiguity): both counts filter the *same* `expected.frags` vector and
  `present.contains(&(d,f)) ⟹ present_anywhere.contains(&f)`, so `placed ≤ by_id_alone` elementwise.
  No panic path.
- **Containment dead-code claim** — I expected `referenced_fragments` at `restore.rs:278` to `?` out on
  an undecodable record before `committed_chunks` could contain it. It does not: `gc.rs:378-385` already
  contains decode failures identically. The two walks' decode/resolve/contain arms are line-for-line
  equivalent.
- **The discriminator smuggling in a new symbol** (which would degrade the red to "a symbol is missing"):
  it does not — `segmented_map_restore.rs` names no new field or variant, and the shape assertions on
  `RestoreReport::unresolvable` / `ReconciliationStatus::PendingUnresolvable` correctly live in the
  C4-ci-gated `restore_reconcile.rs:872` and `segmented_map_consumers.rs:719-731`. The reverted-tree run
  above confirms the red is behavioural, not a compile error.
- **`PendingUnresolvable` shadowing `PendingMalformed`** (`desired_state.rs:225-246` ranks unresolvable
  first, so a store with both loses the malformed chunk ids from the answer until the record is repaired):
  iterative but never certifying, and the same fail-safe ordering `PendingMalformed` itself uses. Not a
  defect.
- **`CopyObject`-style legitimate chunk-map sharing making every copied object ambiguous**: not reachable —
  `crates/gateway-s3/src/lib.rs:1725-1730` refuses `x-amz-copy-source` with 501.
