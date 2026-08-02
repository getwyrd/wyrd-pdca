# review-rejected.md — issue 650

Triage decisions for the batched rubric review. Machine-readable lines below are
`<file:line> | <CLASS> | <MATCH> | <reason>`; `MATCH` is a case-insensitive substring of the
finding's own rationale, so a *different* defect later reported at the same line still
blocks.

Round-5 disposition, for the human: round 4's Check returned **0 blocking** review findings
and every gating gate green; its one open implementation-tagged item was
*T4 Contribution — the reported review/contribution passes cannot be reproduced because
`scripts/review-branch` and `scripts/pdca` are absent from the reviewer's artifact-only
inputs*. That is a harness-input matter, not a defect in the patch — see build-notes §1 for
the exact commands a human can re-run at sign-off.

This round's own change over iteration 4 is a **third consumer** of the shared reference set
that nothing in the patch had documented: `restore::reconcile_after_restore` reads the same
`gc::referenced_fragments` (`crates/custodian/src/restore.rs:200`) and gates on the same
`protects`. Its comment now states — and a throwaway probe confirmed — that it stays
fail-closed over an incomplete set, with the attributed answer deferred to #651 by an in-code
marker (AGENTS.md *Deferrals are settled*). No behaviour change: **0 semantic lines**.

Carried from round 4 and still true: the twice-reported **C3 scope** finding (iteration 3's
public `ReconciliationStatus::PendingUnresolvable` and its `tests/rebalance.rs` leg) stays
**removed**; what is left in `desired_state` is a non-regression, see §(v).

## (i) Caller-side timeout around the resolver await — standing rejection, re-landed

The brief's do-not-re-earn (i), rejected 3× across #508/#636: **the store implementation owns
the network bound, not the caller.** Four supporting facts, each checkable on this tree:

* the *same function* already awaits `meta.scan(b"inode:")` unbounded, on the identical
  `MetadataStore` seam, and did so before this patch (`crates/custodian/src/gc.rs:365`) — a
  timeout on the new await and not the old one would not bound the pass, it would only look
  like it did;
* no await in any of the four custodian loops (`gc.rs`, `scrub.rs`, `reconstruction.rs`,
  `rebalance.rs`) carries a caller-side timeout; the seam's implementations own it;
* `wyrd-custodian` has **no production `tokio` dependency** (`crates/custodian/Cargo.toml`:
  `tokio` is dev-only), and its declared boundary is `traits` / `core` / `tracing`
  (ADR-0010, `crates/custodian/src/gc.rs:27-28`) — so a caller-side `tokio::time::timeout`
  buys the bound with a new production runtime dependency in a seam-only crate;
* the rubric's *await discipline* clause is "bounded (timeout, **fail-closed**)", and this
  await is fail-closed by construction: an error either propagates or contains the object —
  it is never read as "this object owns no bytes", which is the whole point of the slice.

Recorded at the resolver-await callsite and its arms, and at the `referenced_fragments` awaits
the other consumers make, since the same finding lands at whichever of them a pass reads:

- `crates/custodian/src/gc.rs:402` | CONVENTION | timeout | Standing rejection (i), #508/#636: the ChunkStore/MetadataStore implementation owns the network bound, not the caller; the pre-existing `meta.scan` await in the same function is likewise unbounded, and this await is already fail-closed.
- `crates/custodian/src/gc.rs:402` | CONVENTION | bounded | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:402` | BUG | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:401` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:403` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:405` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:414` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:416` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller.
- `crates/custodian/src/gc.rs:365` | CONVENTION | timeout | Standing rejection (i), #508/#636: this `meta.scan` await is unbounded on base and unchanged by this patch; the store implementation owns the bound.
- `crates/custodian/src/gc.rs:360` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller; this function adds no bound of its own to any seam await.
- `crates/custodian/src/scrub.rs:88` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound; this await is the pre-existing reference-build call, unbounded on base for the same reason.
- `crates/custodian/src/desired_state.rs:160` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound; this await is unchanged by this patch.
- `crates/custodian/src/restore.rs:200` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound; this await is the pre-existing reference-build call and this patch adds only a comment above it.
- `crates/core/src/metadata.rs:2243` | CONVENTION | timeout | Standing rejection (i), #508/#636: the `MetadataStore` implementation owns the network bound; every `store.get` / `scan_page` await in this resolver is unbounded on base for the same reason, and this patch adds no await at all here.
- `crates/core/src/metadata.rs:2571` | CONVENTION | timeout | Standing rejection (i), #508/#636: the store implementation owns the network bound, not the caller; this patch changes only how the re-read root's decode failure is TYPED, adding no await.

## (ii) Retraction of already-published bytes

**Not re-proposed.** Nothing in this patch deletes, un-commits or retracts a chunk map or
fragment that was already published. The new clause in `ReferenceSet::protects` only ever
*withholds* a reclaim; it can never trigger one. Recorded pre-emptively at the predicate and
at the reclaim gate, the two places such a finding could land:

- `crates/custodian/src/gc.rs:331` | BUG | retract | Standing rejection (ii), #638 on unchanged evidence: this patch retracts nothing; the new clause only withholds reclamation.
- `crates/custodian/src/gc.rs:306` | BUG | retract | Standing rejection (ii), #638 on unchanged evidence: this patch retracts nothing; the protection predicate only withholds reclamation.
- `crates/custodian/src/gc.rs:191` | BUG | retract | Standing rejection (ii), #638 on unchanged evidence: the reclaim gate only ever declines to delete; nothing already published is un-published.
- `crates/custodian/src/gc.rs:214` | BUG | retract | Standing rejection (ii), #638 on unchanged evidence: the only `delete_fragment` call is gated by the safety gate above it; nothing already published is un-published.

## (iii) "`Completed` releases its admission slot"

**Not applicable / not re-proposed.** This slice has no admission-slot or lease lifecycle in
scope; `InodeState::Committed` is only ever *read* here (a filter on which records the
reference build considers), never written or transitioned.

- `crates/custodian/src/gc.rs:385` | BUG | admission slot | Standing rejection (iii), withdrawn as unsatisfiable: a `Completed` tombstone stays counted; `InodeState::Committed` is only READ here (a filter), and this slice writes no state transition at all.
- `crates/custodian/src/gc.rs:378` | BUG | admission slot | Standing rejection (iii), withdrawn as unsatisfiable: this slice writes no state transition at all.

## (iv) `protects` short-circuiting `true` on an incomplete set is the correct reclamation rule

**Adhered to, not re-litigated.** `ReferenceSet::protects` keeps the settled short-circuit —
while `unresolvable` is non-empty every fragment is withheld — and it is deliberately NOT
narrowed to "only the unresolvable object's own fragments": an unreadable map's chunk ids are
*unknown*, so no fragment in the fleet can be shown not to be one of them. The defect this
patch fixes is the one the finding names — each reading pass's **return value** — not the
predicate.

- `crates/custodian/src/gc.rs:331` | BUG | scope | Standing rejection (iv): the blanket short-circuit is the settled, correct reclamation rule; the defect was the step's return value, which this patch fixes.
- `crates/custodian/src/gc.rs:306` | BUG | scope | Standing rejection (iv): scoping the protection to the unreadable object's own chunks is impossible — those chunk ids are precisely what the unreadable map withholds.
- `crates/custodian/src/gc.rs:311` | BUG | scope | Standing rejection (iv): scoping the protection to the unreadable object's own chunks is impossible — those chunk ids are precisely what the unreadable map withholds.

## (v) The `desired_state` guard is a NON-REGRESSION, and the attributed answer is #651's

Iteration 3's `ReconciliationStatus::PendingUnresolvable` was reported twice as a scope
violation (C3, `deferred-findings.json`) and is **removed**. What is left in
`crates/custodian/src/desired_state.rs:188-190` is three lines returning the **existing**
`Pending` variant, and it is not optional: on `origin/main` an unreadable committed record made
this query fail outright (`referenced_fragments` propagated the decode error / the
segmented-map refusal), so routing the build through the resolver *without* it would silently
convert a fail-closed error into `Satisfied` — "this server is safe to decommission" over a
reference set the system knows is partial (C-1, `docs/principles.md` §5). No public API
changes; the attributed answer (naming the unreadable objects in the status, as
`PendingMalformed` names chunk ids) carries an in-code `// deferred: #651` marker at
`crates/custodian/src/desired_state.rs:183`, which AGENTS.md's reviewer protocol settles.

- `crates/custodian/src/desired_state.rs:188` | CONVENTION | scope | Deferred/settled: the attributed drain answer is #651's (in-code `// deferred: #651` marker at :183); these lines only PRESERVE the base's fail-closed behaviour for an unreadable record, adding no public surface.
- `crates/custodian/src/desired_state.rs:189` | CONVENTION | scope | Deferred/settled: the attributed drain answer is #651's; this returns the pre-existing `Pending` variant and adds no public surface.
- `crates/custodian/src/desired_state.rs:188` | BUG | attribut | Deferred/settled: naming the blocking objects in the drain ANSWER is #651's (in-code `// deferred: #651` marker at :183); each pass that reads the set already names them on the durability seam (`gc::emit_unresolvable`, `scrub::emit_unscrubbable`).
- `crates/custodian/src/desired_state.rs:189` | BUG | attribut | Deferred/settled: naming the blocking objects in the drain ANSWER is #651's; the audit seam names them today.

## (vi) The `Cargo.lock` advisory bump

`event-listener` 5.4.1 → 5.4.2 is carried because the gating `cargo xtask ci` runs
`cargo deny check`, which is **red on the unpatched lockfile** for RUSTSEC-2026-0221
(reproduced in iteration 4: `cargo deny check advisories` → `advisories FAILED` with the
lockfile reverted, green with it; `xtask ci` green again this round). No `Cargo.toml`
requirement changes; no other crate moves.

- `Cargo.lock:1205` | CONVENTION | unrelated | Required by the gating `cargo xtask ci`: `cargo deny check` is red on the unpatched lockfile for RUSTSEC-2026-0221; the bump is a patch-level lockfile move with no manifest change.
- `Cargo.lock:1207` | CONVENTION | unrelated | Required by the gating `cargo xtask ci`: `cargo deny check` is red on the unpatched lockfile for RUSTSEC-2026-0221.
- `Cargo.lock:1204` | CONVENTION | unrelated | Required by the gating `cargo xtask ci`: `cargo deny check` is red on the unpatched lockfile for RUSTSEC-2026-0221.

## (vii) `restore` reads the same set — fail-closed here, attributed in #651

`restore::reconcile_after_restore` is the third reader of `gc::referenced_fragments`
(`crates/custodian/src/restore.rs:200`). This patch adds **no code** there, only the comment
that states what its two halves already do over an incomplete set, and defers the rest:

* the **mark** half gates on the same `ReferenceSet::protects`
  (`crates/custodian/src/restore.rs:239`), which withholds every fragment while the set is
  incomplete — so nothing an unreadable object might own is ever marked for GC; and
* the **report** half re-reads the same records through `restore::committed_chunks`
  (called unconditionally at `crates/custodian/src/restore.rs:326`), whose
  `metadata::decode(&value)?` (`:393`) still propagates — so an unreadable committed record
  ends this pass with an `Err`, exactly as on base, and no `RestoreReport` is ever returned
  over an incomplete set. Verified by a throwaway probe on the patched tree
  (`reconcile_after_restore` → `Err(key must be a string at line 1 column 2)`); the probe was
  reverted, since a restore leg belongs to #651's own added file.

- `crates/custodian/src/restore.rs:200` | BUG | certif | Deferred/settled: this pass cannot return a report over an incomplete set — `committed_chunks` (:326/:393) still propagates that record's decode failure — and the contained/attributed answer for the restore surface is #651's (in-code `// deferred: #651` marker at :196).
- `crates/custodian/src/restore.rs:200` | CONVENTION | scope | Deferred/settled: comment only (0 semantic lines) documenting an already-shared call; the restore surface's own outcome is #651's, per the in-code `// deferred: #651` marker at :196.
- `crates/custodian/src/restore.rs:239` | BUG | incomplete | Deferred/settled: the mark gate is the shared `ReferenceSet::protects`, which withholds every fragment while the set is incomplete; nothing is marked for GC over a hole.
