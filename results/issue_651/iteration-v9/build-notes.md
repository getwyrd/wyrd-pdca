# build-notes — issue 651, iteration 9 (withheld from the reviewer)

**Base:** `origin/main` = `d50f0ca` (the brief's target). Built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`); the human's `../wyrd` checkout was never touched. Every
`path:line` below is against that worktree with `patch.diff` applied.

**Started from iteration 8**, applied verbatim (`git apply` clean) as the brief instructs.
Everything here is the delta on top of it: the four round-8 findings, all **fixed**, none
rejected.

---

## 1. The rebuild — ambiguity keyed on REFERENCE count, not holder count

The carry-forward's binding item. Round 8 asked the wrong question in two places:

* mark half: `canonical.get(&frag)` → `holders.iter().all(|d| present.contains(..))`
* report half: `attributable(..)` → `holders.iter().all(|&d| d == dserver)`

`canonical` is built from `referenced.placed`, a `HashSet<(DServerId, FragmentId)>`
(`gc.rs:267`). **Two committed references that name the same D server collapse into one
holder there and leave no trace.** So `all` degenerated to `any` and `attributable` returned
`true` — the base's behaviour verbatim, on the *commonest* shape of the collision (an empty
placement vector and the M0–M2 route both resolve fragment `i` to D server `i`,
`crates/core/src/placement.rs`, `gc.rs`'s identity fallback).

The rebuild: **`CommittedChunks::claims`** — a `HashMap<ChunkId, usize>` counted in
`committed_chunks` (`restore.rs:602`) over **every** committed `ChunkRef` the walk reaches —
and one predicate, `CommittedChunks::ambiguous` (`restore.rs:562`), asked by both halves:

* mark half, `restore.rs:358`: an ambiguous id ⇒ **no copy of it is marked**, ever
  (`displaced_kept`, `emit_displaced`). Placed *before* the displaced check so it covers every
  copy, which is the brief's own wording ("nor a reclamation mark on **any copy of it**").
* report half, `restore.rs:490`: `anywhere = if ambiguous { placed } else { by_id_alone }` —
  fleet-wide evidence collapses to this reference's own placement, so an empty placement is
  `dangling`, never `misplaced`.

Deleted in the process: `fn attributable` (~10 lines) and the `every_reference_is_satisfied`
mark test. The displaced case is back to the base's `any`, unchanged, guarded by the new gate
above it.

**Counted BEFORE the malformed-placement skip** (`restore.rs:643`). A reference whose
placement is of the wrong length still *claims* its id; counting after the skip reads the
store as singly-claimed. Pinned by `a_malformed_placement_still_claims_its_chunk_id`
(`restore_reconcile.rs:981`), and I proved it binds by moving the counter after the skip and
re-running: `misplaced: [62]` instead of `dangling: [62]`.

### Alternatives, with their cost

* *Keep the holder-set and add a same-placement special case.* ~6 lines, and wrong in the
  direction that loses data: it is still asking "where does the map say this lives?" of a
  store whose maps disagree about which chunk that is. The whole finding is that the
  placement is not the identity.
* *Carve out "legitimately shared" chunks (identical placement ⇒ one chunk).* This is exactly
  the hole round 8 fell into, so I checked whether the carve-out has a subject: nothing in the
  tree shares a chunk id between two committed objects — `write::plan_write` mints one id per
  chunk from the caller's closure, and there is no dedup, copy or link path that reuses a map.
  So the carve-out would have zero true positives and one catastrophic false negative. Dropped.
* *Rebuild `canonical` per-reference as `HashMap<FragmentId, Vec<Vec<DServerId>>>`.* ~12 lines
  and a second source of truth for data `committed` already holds one-per-reference; it also
  still answers a placement question, not an identity one.

### What the rule costs on a healthy store: nothing, and that is asserted

`a_displaced_fragment_no_other_object_claims_is_still_its_own_chunk_s_evidence` (criterion 4c)
plus the six pre-existing `restore_reconcile.rs` legs (`a_stale_duplicate_is_marked_...`,
`the_only_copy_of_a_moved_fragment_is_never_marked`, `a_single_fragment_chunk_whose_bytes_
moved_is_reported_misplaced_not_healthy`, …) all pass unchanged. A singly-claimed id takes
exactly the base's path.

## 2. The reachability premise — withdrawn, and replaced with what is actually true

Round 8's second finding was right: `chunk_id_minter` packs the **inode id** into the high 64
bits (`cli.rs:1716` on the base), and the inode id *is* the record key, so two *live committed*
records cannot collide by that route. I looked for a route that does and **could not find one**
— the gateway draws a random per-process epoch ≥ 2^127 (`server/src/lib.rs:238`),
`high_water_marks` + `seed_next_inode_floor` resume the in-process counter above every
committed id, and `alloc_inode` is a CAS allocator. I did **not** manufacture one.

So I removed the claim everywhere it appeared (three code comments, the fixture header, the
`restore_reconcile.rs` doc comment, both docs) and replaced it with the argument that actually
holds:

> Chunk-id uniqueness is a property of the **allocator** that minted the id. This pass runs
> over a namespace just **imported from a backup** — minted under some other allocator state,
> reconciled against a fragment tier that was not restored with it. It is the one pass whose
> job is to check what that import produced, so it **observes** uniqueness rather than
> assuming it, and the observation is free: the walk it already does reads one entry per
> committed chunk *reference*. (`restore.rs:221`)

That is honest (no unsupported minting path is claimed), it is *not* inert conservatism (it
fires only on positive observed evidence, and a store the shipped allocators minted never
produces it), and it keeps criterion (4), which the brief's Success criterion mandates.

**One genuinely reachable id-reuse path did turn up, and I fixed the doc that denies it.**
`m4-first-deployment-blueprint.md:694` claimed *"chunk ids are random / coordination-free and
are not derived from the inode … a reused inode cannot collide with a stranded chunk"*. True
for `Gateway::mint_chunk_id`; **false for the CLI cluster/local put path**, which mints
`(inode << 64) | seq` — so after a restore rewinds `meta:next_inode`, a new file *does* re-mint
the ids of the post-*V* file that held that inode, and its stranded fragments are on disk under
those ids. That produces one live reference plus stale bytes rather than two live references, so
this slice's rule does not (and cannot) resolve it — but leaving the doc asserting the opposite
of the pass I am shipping would be a docs-currency defect, and #652 is named as the fix.

## 3. and 4. — the CLI paragraph and the load-bearing predicate

* **DANGLING hedge** (`cli.rs:1290`). The paragraph no longer asserts "GC took the bytes; no
  reconstruction can rebuild them" unconditionally; it names both causes and points at
  `action=ambiguous-chunk-id`. Pinned by
  `the_dangling_paragraph_does_not_assert_a_cause_it_cannot_know`. Deliberately **not** a new
  report field or CLI cell — the brief declines that in Scope.
* **`is_clean` made load-bearing, and the claim fixed.** `RestoreReport::needs_human`
  (`restore.rs:192`) is new, `restore_verdict` exits on it (`cli.rs:1262`), and `is_clean` is
  written **in terms of** it (`restore.rs:178`) so a field added to one reaches the other by
  construction. Both are stated on the audit summary (`restore.rs:866`).
  `restore_needs_human_agrees_with_every_paragraph_it_prints` (`cli.rs:2693`) checks each
  finding **alone** — a `||` chain is green for the wrong reason whenever a sibling is set —
  which is also the C5 mutant round 8 missed ("either CLI loss class alone"). The
  discriminator's criterion-(2a) justification no longer says `is_clean` is what the command
  exits on; `restore_reconcile.rs:886` pins `needs_human()` on the same shape.

## 4. Size — over budget, declared, with the arithmetic

| | this iteration | iteration 8 | budget |
|---|---|---|---|
| files | 8 | 8 | ≤ 8 |
| added lines | 1,851 | 1,674 | — |
| **semantic added** (non-blank, non-comment) | **1,046** | 970 | ≤ 950 |
| patch.diff | 132 KB | 118 KB | (100 KB backstop) |

Both columns measured with one script (it reproduces the brief's "iteration 7 = 749" and the
reviewer's "971" for v8). Per file (added / semantic): discriminator 888/567 · `cli.rs` 242/179
· `restore.rs` 387/133 · `restore_reconcile.rs` 193/118 · `desired_state.rs` 81/22 ·
`segmented_map_consumers.rs` 32/13 · `m4-…blueprint.md` 27/27 · `06-runtime-view.md` 1/1.

**The +76 over v8 is the carry-forward's own work, itemised:** the malformed-claimant
regression the new counting order needs (+22), the CLI predicate-coupling and DANGLING-hedge
tests findings 3 and 4 demand (+31), `needs_human` itself (+8), the corrected runbook paragraph
(+11), the second collision shape folded into the existing legs (+4).

**I cut 121 semantic lines before shipping**, so this is what is left after trimming, not the
first draft: the four collision legs were merged into two parameterised ones (−52), the three
`cli.rs` verdict tests into one table plus two (−53), criterion (2a)'s healthy object went from
a segmented seed to a flat one (−10), and six restatements of the attribution rule were
collapsed into one canonical block plus pointers (−6 semantic, −1 KB).

**Why I did not hand back a split** (the brief's own instruction on overage): the human's
iteration-8 sign-off is explicit — *"the size overage … is slight, not the kind of overage that
warrants sending the whole thing back to Plan — keep it as one iterate-do round"* — and a split
here would separate the attribution rule from the four scenarios that are its only evidence.
The brief's own § "Why this is not a split" costs it: each child still needs its own ~350-line
fixture over the same seeded store, so more total lines and two more cycles. The 132 KB is 40 KB
of new discriminator (the brief mandates the file) plus 7.5 KB that is one single-line paragraph
in `06-runtime-view.md` being rewritten (that file has one line per paragraph, so any edit costs
old+new). **This is a judgement call the human should make at sign-off, not one I should hide.**

## 5. Verification — through the project's own runners

**`./engine/scripts/run-verify.sh` (C4-verify, `PDCA_BUNDLE=results/issue_651`) → PASS.**
`--classify` first: `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs` + `CRATE
crates/custodian` + `CRATE crates/server` — exactly what the brief predicted, so the invocation
is `cargo test -p wyrd-custodian --test segmented_map_restore`.

* GREEN with the fix: `7 passed; 0 failed`.
* RED with production reverted, test kept: `1 passed; 6 failed`, all on **assertions** (the
  file compiles against the base — it names no symbol this patch introduces). The one pass is
  criterion (4c), by design.
* The base's own answers, quoted from that leg:
  * (4a) `dangling: [], misplaced: [45312]` — the base says the bytes are one hop away for an
    object none of them belong to.
  * (4b) `stranded_marked: 1` — the base marks the second object's only copy. The data-losing
    leg, reproduced.
  * (1)/(2) `SegmentedMapUnsupported { operation: "restore::committed_chunks" }`.

**`./engine/xtask.sh ci` (C4-ci) → `all checks passed`, twice.** `typos`, `lint_docs: OK`,
`render_site: wrote 98 page(s)` + link audit, gitlink/unsafe guards, `cargo fmt --check`,
clippy `-D warnings`, build, the full workspace test suite, `cargo-machete`, **all three**
`cargo deny` walls (0.20.2 — the tool version that cost v7 its verdict), statics, deploy-guard,
and the DST leg all **ran** rather than warn-skipping. So the patch is commit-ready for the
target's own hooks, not merely gate-green.

### The three forced questions

* **(a) Genuine red?** **Yes**, twice over, both measured rather than argued.
  1. Against the **base**: the C4-verify RED leg reverts `restore.rs`, `desired_state.rs`,
     `cli.rs` and every modified test file, keeps the discriminator, and 6 of 7 fail on
     assertions (`left: 1 / right: 0`, `left: [] / right: [45312]`), never on a missing symbol.
  2. Against **iteration 8's rule** — the thing this round actually had to fix. I simulated it
     in place (made `ambiguous()` count distinct D servers named instead of references, which
     *is* v8's semantics) and re-ran: **both** collision legs go red, reproducing the adversary's
     numbers exactly — `stranded_marked: 1` on the mark half (GC deletes the second object's
     only copy) and `misplaced: [46336, 46336]` on the report half. The new same-placement rows
     therefore bind the **rebuild**, not just the base. Reverted immediately after.
  3. The malformed-claimant leg was refuted the same way (counter moved after the skip →
     `misplaced: [62]`, red).
* **(b) Production path?** **Yes.** Every leg calls `wyrd_custodian::reconcile_after_restore` /
  `wyrd_custodian::desired_state::reconciliation_status` — the production functions this patch
  changes — over the `MetadataStore` / `ChunkStore` **seams** (this crate's established idiom,
  `tests/restore_reconcile.rs`), not stand-ins for the logic under test. The CLI legs drive
  `restore_verdict`, the exact function `cmd_custodian` prints and exits on
  (`cli.rs:1204-1211`). No mock, no copy, no re-implementation.
* **(c) Fixture includes the fault?** **Yes.** The collision legs seed the collision itself —
  two committed objects claiming one id — in **both** shapes (different placements and the same
  placement), and the mark legs additionally seed the displaced copy on a D server no placement
  names, which is precisely the fragment the base and v8 mark. Nothing is curated out: the
  object whose placement is empty stays in the fleet as a live empty `MemDServer`, and
  `seed_damaged` re-resolves its own seeded root and requires `is_err()`, so a containment leg
  can never pass because the fault quietly stopped being one.

## 6. Scope held

No `gc.rs` / `scrub.rs` edit. No `crate::resolve` module, no custodian-level walk (#681).
Nothing written to a chunk map (#682). No change to how ids are minted (#652). No new report
class or CLI cell for a shared id, no owner attribution threaded through
`Expected`/`emit_dangling`/`emit_misplaced` (declined in `review-rejected.md` with the issue
reference, per AGENTS.md's reviewer protocol). No new dependency, no cfg gate, no DST leg —
this patch *narrows* a destructive path (it marks strictly fewer fragments than the base:
`ambiguous` only ever adds a `continue`), so the rubric's "a new destructive path lands with
seeded Tier-0 DST coverage" does not apply.

Self-reviewed against AGENTS.md § Review rubric & protocol: docs currency (both living docs
updated in the same patch — the CLI's new UNREADABLE class and the hedged DANGLING story now
appear in the runbook), *absent or unsupported entries* (an unreadable record produces an
explicit named finding and a non-zero exit, never a silent skip), *await discipline* (no new
await; standing rejection (i) recorded at each line it can re-land), *test fidelity* (the
in-memory doubles mirror the seam, and every new rule ships with a leg that is red without it).

## 7. Housekeeping

`review-rejected.md` rewritten for this iteration: the three standing rejections and the five
scope declines re-keyed to **this** patch's line numbers (verified line-by-line with a script;
they had drifted 20–90 lines), and all four round-8 findings recorded as **FIXED**, with the
`restore_reconcile.rs` round-7 finding still recorded as fixed rather than rejected. Scratch
lived under `$PDCA_SCRATCH` (`pdca-builder-651-restore.bak`, `pdca-builder-651-mine.rs`) and is
removed.
