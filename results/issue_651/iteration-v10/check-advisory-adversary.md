# Adversarial review — issue 651 (advisory, non-gating)

Re-ran the asserted red→green in a scratch clone (`cargo test -p wyrd-custodian --test
segmented_map_restore`): **8/8 green with the patch**, and with `restore.rs` /
`desired_state.rs` / `cli.rs` / both modified test files reverted to `origin/main` the
discriminator **compiles and fails 7/8 on assertions** (the 8th, criterion 4c, is the
"no-collision ⇒ no change" leg and is meant to be green on base). The evidence is real, it
drives the production entry points (`reconcile_after_restore`, `reconciliation_status`), and
it is assertion-red rather than compile-red. The attacks below are on the fix, not the proof.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:394-411`: the **mark withdrawal is
  skipped on the displaced shape**, so the data-losing leg the patch claims to close is still
  open. The `if let Some(holders)` arm `continue`s at `:410` *before* the already-marked
  withdrawal at `:418-441` ever runs, so an ambiguous-id copy that arrives already carrying an
  `orphan:` record keeps it whenever **no** claimant's placement holds the bytes. Concrete
  case, reproduced: two committed objects claim chunk id `0xBB00` with placements `d0`/`d1`,
  neither holds the fragment, the only copy is on unnamed server `d8` and carries a mark an
  earlier run wrote → `RestoreReport { already_marked: 0, displaced_kept: 1, dangling:
  [0xBB00, 0xBB00] }` and `orphan:` **still present**; running `gc::reconcile` immediately
  after (same `GcContext`, past the grace window) **deletes the fragment** — the only copy of
  both claimants — because `gc::ReferenceSet::protection` (`gc.rs:306-318`) has no
  ambiguity clause. That directly refutes `restore.rs:263-264` ("neither marks a copy of the
  id **nor leaves one carrying a mark an earlier run wrote**"), `docs/design/architecture/
  06-runtime-view.md:31` ("a reclamation mark an earlier pass left on one is **withdrawn**
  rather than left for collection") and `m4-first-deployment-blueprint.md:716`. It also breaks
  the `already_marked` contract at `restore.rs:117-123` ("It is still counted here, because it
  still *arrived* carrying a mark"): the fragment is counted as `displaced_kept` and
  `already_marked` stays 0. The test that is supposed to pin this,
  `crates/custodian/tests/segmented_map_restore.rs:882-933`, seeds a copy at `d0` **as well**,
  which is exactly the one arm that reaches the withdrawal — delete `d0.put(...)` from it and
  it goes red on the patched tree. Fix: move the ambiguity/withdrawal decision below the
  already-marked check for the displaced arm too (or hoist the `already` lookup above the
  `canonical` branch).

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:645-647` + `:726`: ambiguity is keyed
  on the number of committed **references**, not on the number of committed **objects**, so a
  single object whose chunk map lists one id twice (two identical/deduped chunks — an import
  artifact exactly as plausible as the cross-object collision this rule targets) is declared
  lost. Reproduced: one committed record with `chunk_map = [ChunkRef{id:0xCC00,..},
  ChunkRef{id:0xCC00,..}]`, its fragment present at its own placement, healthy store →
  `dangling: [0xCC00, 0xCC00]`, `is_clean() == false`, and `restore_verdict` (`cli.rs:1262`)
  prints "2 chunk(s) are LOST" and exits non-zero. Nothing here is unattributable — there is
  only **one** claimant object, so the bytes provably belong to it, and the invariant the brief
  states ("evidence must be attributable to the object that references it") is *satisfied*, not
  violated. The audit line the operator is sent to is wrong in the same way:
  `restore.rs:922` says "claimed by more than one committed **object**" while `claims` counted
  references inside one record. Fix: count distinct owning `inode:` keys per chunk id (the walk
  at `:687-748` already has the key in hand), not `ChunkRef`s. Secondary: `report.dangling`
  now carries one entry **per reference**, so `dangling.len()` in the summary and the
  NEEDS-HUMAN paragraph counts a single chunk id twice.

- NEEDS-HUMAN [human] — `docs/design/architecture/m4-first-deployment-blueprint.md:695-716`
  and `crates/custodian/src/restore.rs:244-254`: the open question the iteration-9 sign-off
  carried forward ("name the actual reachable collision path or fold into #652") is now
  *answered in the diff's own docs* — "Two *live* records still cannot collide", the CLI minter
  packs the inode and the gateway draws a random epoch, and CopyObject is refused
  (`crates/gateway-s3/src/lib.rs:1725-1730`), so no shipped writer can produce two committed
  claimants of one id — yet the rule ships anyway, and its verdict is the most severe one the
  command has (LOST + exit 1). Combined with the previous bullet, the only in-tree-reachable
  trigger of the new rule produces a **false** LOST. This is the architectural/fitness call the
  human deferred twice: keep the inert conservatism (accepting that a corrupt or imported record
  is now reported as data loss rather than as a corrupt record), or fold the whole ambiguity
  rule into #652 and ship only the containment/attribution half this slice was briefed for.

- NEEDS-HUMAN [human] — size against the brief's own stop rule: the diff is **8 files** (the
  cap) and **1,242 semantic added lines** (non-blank, non-comment; 1,203 excluding docs) against
  a `≤ 950` budget whose brief says "If mid-build the tree exceeds this, STOP and hand back a
  proposed split rather than finishing". The trend across rounds is v7 ≈749 → v8 ≈971 → v9
  ≈1,059 → now ≈1,242, i.e. each accepted "nominal" overage has grown; `crates/server/src/cli.rs`
  alone added 232 semantic lines (+315 raw) for a slice whose CLI scope was one summary cell and
  the exit code. Two prior sign-offs accepted this as not a re-slice trigger; at +31% it is no
  longer nominal and deserves an explicit decision rather than a third silent pass.

- Minor (no adjudication needed) — `crates/custodian/src/desired_state.rs:225-246`:
  `PendingUnresolvable` returns before the `PendingMalformed` check, so on a store carrying both
  blockers the malformed chunk ids are withheld from the answer. Consistent with how
  `PendingMalformed` already ranks under `Pending`, and the operator re-polls after repairing the
  named record, so it is a ranking choice rather than an unattributed stall — noted only because
  the slice's stated invariant is that a refusal names *what* to repair.

## Attacked and could not refute

- The red→green itself: re-ran both legs (above). The discriminator names no symbol this patch
  introduces, so the base leg is genuinely assertion-red, and every leg calls the production
  functions over the real `metadata::resolve_chunk_map` / `gc::referenced_fragments` path — no
  parallel re-implementation, no mock of the defect.
- `is_clean()` now has a true-branch assertion (`crates/custodian/tests/restore_reconcile.rs:280`),
  so the "`fn is_clean(&self) -> bool { false }` passes everything" hole from the last round is
  closed; `restore_verdict` derives the exit code from `report.needs_human()` itself
  (`crates/server/src/cli.rs:1360`) and `cli.rs:2720-2790` pins each finding one at a time.
- The claim ordering in `committed_chunks` (`restore.rs:722-729`) — counting a reference *before*
  the malformed-placement skip — is right, and `a_malformed_placement_still_claims_its_chunk_id`
  pins it; making it fail requires counting after the skip.
- `emit_ambiguous_evidence` is now reachable from every ambiguous verdict (`restore.rs:569-581`),
  not only the `anywhere < k` arm, so the "silent forever, certified clean" path from the previous
  round is gone.
- Scope: `gc.rs` and `scrub.rs` are untouched, no `crate::resolve` module, nothing written to a
  chunk map, `RestoreReport::dangling` / `misplaced` keep their `Vec<ChunkId>` shape — the
  out-of-scope list holds.
