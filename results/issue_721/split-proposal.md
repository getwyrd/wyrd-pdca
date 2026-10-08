<!-- pdca:split-proposal v1 -->
# Split proposal — issue 721

## Why this slice is oversized

The slice bundles **two independently shippable outcomes** that live in different crates,
carry different verification postures, and — on the evidence of four burned iterations —
fail review for different reasons:

1. **A missing `wyrd_core` primitive.** No symbol on the base can land a placement move in
   a `seg:` record: the only placement writer, `repair_chunk`
   (`crates/custodian/src/reconstruction.rs:829`), takes `as_flat()` at `:894` and CASes
   `inode:` only. The primitive that must exist — "given a resolved generation, a byte
   offset, the planned `ChunkRef` and a new placement, hand back the CAS batch that lands
   the move in whichever record holds that `ChunkRef`" — is a self-contained piece of
   `crates/core/src/metadata.rs`, with its own semantics-dense decisions (the three pins,
   sibling-merge vs. same-chunk-conflict, the V/2 ceiling for the segmented arm, checked
   version advancement, malformed-decode-as-corruption, the zero-length boundary-chunk
   hazard) and its own oracle (in-crate unit tests + `cargo mutants` — the C5 residue was
   17 missed mutants, **all** in this new code).

2. **The custodian pass completing through it.** Removing #697's refusal
   (`reconstruction.rs:552` / `:609`), composing placement change + obligation discharge +
   orphan evidence into one batch, the once-per-object refusal dedupe, the
   `reading.contain(...)` routing for unattributable committed objects, and the five-leg
   integration test with its `scan_page`-hook race doubles. Its oracle is the flippable
   red: legs 1–2 go red on a base where the primitive exists but nothing calls it.

The iteration history is the seam made visible. Iterations 1 and 2 were rebuilt entirely
for **core** findings (`metadata.rs:2859`, `:2900`, `:2607`, `:2844`, `:2883`); iteration
3 entirely for **custodian** findings (`reconstruction.rs:322`, `:513-515`). Each rebuild
re-spent a full cycle re-reviewing the half that had not changed. The parent's attempt
came in at 7 files / 1595 added lines / 124 KB against a 100 KB backstop; each child below
fits comfortably under its own cap.

A split into two, not more: the five test legs must stay together (they pin one pass's
behaviour and share one fixture), and the primitive's pins are one design decision — cutting
either finer would produce children that cannot ship alone.

**Two flags for the human before accepting**, both from the iteration-4 carry-forward,
neither of which this split re-decides:

- *Certification on the conflict path.* The parent brief's leg 3 asserts store-level facts
  (no repair-owned metadata written, obligation still queued). Whether the pass may still
  certify `Satisfied` after a lost CAS — tightening which would change the **flat** arm's
  long-standing behaviour too — is deliberately **not** folded into either child. If you
  want it fixed, file it as its own issue; child-2's leg 3 asserts the store, not the
  certification verdict.
- *The stranded destination fragment* stays the parent's settled carve-out: owned by
  getwyrd/wyrd#723 (the 0016 X47 pre-mark), inherited as-is by child-2, not re-argued.

## Wave sketch

**child-1 → child-2, strictly stacked; no parallel wave inside this split.**

- **child-1** (the `metadata.rs` primitive) has no in-batch prerequisite — every external
  dependency (#710's ceiling helper, #695/#696/#697) is already merged into `origin/main`
  (verified at Plan: `d2609b2`, `1f871ce`, `99c7fcf`, `3e05891`). It runs in wave 0.
  **Conflict corrected at Plan — it is #772, not #717.** The parent's inherited
  `Conflicts with: 717` is **stale**: #717 was itself split, is `COMPLETE [close: no PR]`,
  and ships **no code** (its brief is a decomposition record). The real `metadata.rs` editor
  is its child **#772 `multipart-owned-staging-entry`**, which inserts `owner`/`staged` into
  `PendingEntry` at **`metadata.rs:1556`** (not `:1528` — verified on `origin/main`, and
  #772's own brief cites `:1556` too). #772 already declares `Conflicts with: 721, 722`.
  So child-1 carries **`Conflicts with: 772`** as a real machine-readable field. Timing is
  comfortable: #772 is `PLANNED [blocked-by: 771]` and #771 is still `ITERATE_DO`, so #772
  is at least two waves out — but declare the field anyway rather than rely on that.
  **On accept, #772's `Conflicts with: 721` should be re-pointed at child-1's new id**
  (child-2 does not touch `metadata.rs`).
- **child-2** (the reconstruction caller) **`Depends on: child-1`** — its production code
  calls the primitive, and its red legs are only honest on a base that already *contains*
  the primitive (so the red demonstrates "nothing calls it", not "it doesn't exist").
  `compute_waves` will place it in the next wave; with `auto_merge = false` the driver
  stops at the wave boundary and the human merges child-1's PR first (INTEGRATION §2).
  child-2 touches only `crates/custodian/` files, so — a gain of this split — it does
  **not** conflict with #717 and needs no such note.
- **#722** (drain/evacuation + the DST repoint-versus-supersede property) stacked on the
  parent. **Refined at Plan:** its true build-on prerequisite is **child-1** (it calls the
  primitive from `crates/custodian/src/rebalance.rs`), *not* child-2 — the two callers touch
  disjoint files (`rebalance.rs` vs `reconstruction.rs`), so #722 may run **in parallel with
  child-2** once child-1 has landed, rather than strictly after it. Whoever plans #722 should
  set `Depends on: <child-1>` and need not serialise behind child-2. (#722 is `PLANNED
  [blocked-by: 721] [oversized]` — it is likely to need its own split; not this beat's call.)

<!-- pdca:child child-1 -->
- **Slug:** seg-record-placement-move-primitive
- **Defect:** **No maintenance write path in the tree can address a `seg:` record.** The
  only placement writer rebuilds an *inode* record: `repair_chunk`
  (`crates/custodian/src/reconstruction.rs:829`) takes `object.prior.chunk_map.as_flat()`
  at `:894`, aborts on `None`, and CASes `inode:` at `:937-953`. A
  `seg:<nonce>:<epoch>:<index>` record cannot be written by any repair-shaped code. This
  child ships the missing `wyrd_core` primitive; it changes **no** custodian behaviour —
  the pass keeps refusing until child-2 wires it in.
- **Success criterion:** `crates/core/src/metadata.rs` exports a placement-move primitive:
  given a resolved generation, the byte offset of the chunk within the object, the
  `ChunkRef` the caller planned from, and the new placement, it **hands back** (never
  commits) the compare-and-swap batch that lands the move in whichever record holds that
  `ChunkRef` — flat inode **or** segment record — so the caller can add its own evidence
  (obligation delete, orphan marks) and land everything in ONE mutation (`0005:277`,
  ADR-0015). In-crate `#[cfg(test)]` unit tests (module convention `metadata.rs:2776-2780`)
  pin, by building the batch and applying it against a hand-mutated in-memory
  `MetadataStore`:
  1. **Flat arm:** the batch changes `chunk_map` **and** increments `version` via the
     `commit_chunk_map` idiom (`version = prior.version + 1`, `..prior.clone()`,
     `metadata.rs:1769-1797` — ADR-0047 metadata preserved). This kills iteration-1's four
     surviving mutants (`metadata.rs:2859`).
  2. **Segmented arm:** only the covering segment record is rewritten — found via the
     root's own table (`SegmentedMap::new`, `metadata.rs:870`), no `seg:` range walk, no
     other segment decoded — and the **root's bytes are unchanged**.
  3. **The three pins, exactly** (settled at the parent's Plan, do not re-derive): the
     **root generation's** bytes; the **segment record's own freshly-read bytes**; and the
     **`ChunkRef` itself** — the chunk moved is the one that begins at the given offset
     **and equals** the planned reference. A concurrent edit to a *sibling* chunk in the
     same record is **MERGED** (batch still applies); an edit to the planned chunk itself
     is a **CONFLICT** (batch fails cleanly, writes nothing). `ResolvedChunkMap`
     (`metadata.rs:2294-2300`) carries only `record` + flattened `chunks`, so "pin the
     exact bytes the resolve read" is not implementable for the segmented arm and must not
     be claimed — the archived attempt's three doc sites saying otherwise are a known
     defect to correct, not a spec to follow.
  4. **Superseded root:** a root flipped to a different generation makes the batch fail
     cleanly; nothing is written.
  5. **Ceiling:** the re-encoded record is weighed **before** anything is written — the
     **flat** arm through #710's `flat_value_ceiling_crossed` (`metadata.rs:380`)
     unchanged; the **segmented** arm against **V/2** (`MAX_ROOT_VALUE_BYTES`, `50_000`,
     `metadata.rs:352`). **SETTLED — do not re-derive, and do not be misled by the one doc
     comment that appears to say otherwise.** Warrant, re-verified on `origin/main` at Plan:
     (a) the **in-tree, base-visible** resolver comment at **`metadata.rs:2488`** states
     outright that "`0016:1467` bounds a segment value **to V/2**", and treats its own
     `value.len() > MAX_VALUE_BYTES` test (`:2493`) as *containment* of a row no conforming
     publication wrote — not a licence to mint one; (b) `0016:1466`'s knob table gives
     `MAX_SEG_CHUNKS` "same rule against a `seg:` record … ditto for segment values", the
     rule being `max_chunkref_bytes × N ≤ V / 2`. **The apparent counter-evidence, disposed
     of:** `flat_value_ceiling_crossed`'s doc at `metadata.rs:371-376` says "A segmented
     root's placement write is #682's, and it is the one that must weigh
     `MAX_ROOT_VALUE_BYTES`" — that sentence is about the segmented **root** record, whereas
     child-1's segmented arm rewrites the **`seg:` segment** record. Both are V/2-bound, by
     (a) and (b) respectively; the doc comment does not contradict this leg. A
     differently-*named* helper for the segment arm is fine; a second ceiling *value* is
     not. Unit test seeds a record just under the bound whose repoint would cross it and
     asserts refusal, record byte-identical.
  6. **Checked version advancement** (no wrap, no panic) and **malformed segment decode
     surfaces structural corruption** rather than hiding a persistently damaged record —
     iteration-2's findings (`metadata.rs:2844`, `:2883`).
  7. **Zero-length chunk at a segment boundary** (iteration-4 carry-forward,
     `metadata.rs:2869-2874`, `:2922`): the offset lookup must not match the wrong record
     ahead of the equality check — equality governs; a mismatch is a conflict/`Blocked`,
     never a silent wrong-record write.
  8. Direct unit tests for the two addressing helpers (offset-plus-equality lookup,
     segment coverage) — the parent's C5 residue was 17 missed mutants, all here.
  **Verification posture — declared so sign-off is not surprised:** this child is
  green-only under C4-verify, **deliberately**. Its defect is an *absent* API, so no test
  can go red without naming the new symbols, and a NEW `*/tests/*.rs` naming them would
  make the gate's revert leg fail to **compile** (UNVERIFIABLE, exit 77,
  `run-verify.sh:469-476`, `:492-500`). Therefore: **no added test file**; all tests are
  in-crate `#[cfg(test)]`, covered by C4-ci; the binding oracle is **`cargo mutants` over
  the diff** — expect zero missed mutants in the new code, and `build-notes.md` records
  the named negations (delete the `chunk == prior` equality → test 3's conflict leg goes
  red; delete the root-generation pin → test 4 goes red) **demonstrated, not asserted**.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality. The absence is shown on the base:
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` — `:894` takes
  `as_flat()` and aborts on `None`; `git -C ../wyrd grep -n 'seg_key'
  origin/main -- crates` finds no writer that composes a `seg:` CAS for a placement move.
- **Scope:** **one file** — `crates/core/src/metadata.rs`: the primitive, its two
  addressing helpers, the segmented-arm ceiling check, and the in-crate unit tests.
  **Salvage:** the archived attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a
  working primitive that passed C4-ci — reuse it, correcting every doc site that claims
  the move pins "the exact bytes the resolve read". **Bounded memory:** pin one record at
  a time; no `seg:` range walk; no deep copy of a segmented root. **Untouched:**
  `commit_chunk_map` and its segmented refusal at `metadata.rs:1776-1780` stay exactly as
  they are; `resolve_chunk_map` and the whole read side; every custodian file; proposal
  0016 (draft, stays untouched); no conformance-vector change; no new dependency.
  **Budget:** 1 file, ≤ 170 added semantic non-test lines, `patch.diff` ≤ 50 KB (calibrated
  at Plan: the core half of the parent's v4 patch measured **~31 KB**, so this is comfortable).
  A second file means the shape is wrong — STOP and hand back.
  **Ordering note (external — carry onto the filed issue):** never share a wave with
  **#717**, which shifts every `metadata.rs` citation below `:1528`; cite by symbol, not
  by number, below that line. All merged prerequisites (#710 as PR #718; #695/#696/#697 as
  PRs #704/#705/#706) are already on `origin/main` at `92e1b4b` — no `Depends on (merged):`
  needed.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/core/src/metadata.rs` — in-crate `#[cfg(test)]` module,
  **deliberately not** a new `tests/*.rs` file (see the verification posture above: an
  added test file naming the new symbols makes C4-verify's revert leg fail to compile).
- **Falsifiability:** **Checked against this project's gate at Plan, not assumed.** This child
  is **green-only under C4-verify by design**, a *confirmed* property of the gate:
  `run-verify.sh:143` classifies via `_added_files` filtered by `_is_test_file`
  (`*/tests/*.rs`), and at `:493` an empty `ADDED_TESTS` takes the `PASS (green-only)` branch
  and **exits 0**. An in-crate `#[cfg(test)]` module therefore cannot earn a per-fix RED — and
  shipping the tests as a new `tests/*.rs` would be *worse*: the revert leg strips the
  production change, the test still names the new symbols, so it fails to **compile** →
  UNVERIFIABLE (exit 77, `:538-548`). **So the binding oracle is `cargo mutants` over the diff
  (`C5-mutants`), not C4-verify.** Where each pin goes red — demonstrated in `build-notes.md`,
  never asserted: delete the `chunk == prior` equality → same-chunk-conflict test red; delete
  the root-generation pin → superseded-root test red; delete the `> V/2` comparison → ceiling
  test red; make the version advance unchecked → exhaustion test red. Expect **zero** missed
  mutants in the new code (parent's residue was 17, all here). `C4-ci` still gates the tree.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost** (`docs/principles.md` §5 C-1 at `:109`, §6 row *Storage lifecycle /
  reclamation* at `:137`). This child does not by itself restore C-1 — **child-2 does** — but
  it is the enabling half: C-1 cannot be restored while the tree contains **no write path
  that can address a `seg:` record at all**. Structural/lifecycle category, so the Plan-exit
  gate applies and is recorded as passed on child-2, which owns the behavioural change.
- **Surfaces:** data
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer callsite to mirror (do not re-derive the CAS idiom):** `commit_chunk_map`
  (`crates/core/src/metadata.rs:1776`) is the tree's existing flat placement CAS — mirror
  its `version = prior.version + 1` / `..prior.clone()` advance (`:1769-1797`, ADR-0047
  metadata preserved) and its `require(key, encode(prior)) + put(key, encode(next))` shape.
  Leave `commit_chunk_map` itself, including its segmented refusal at `:1778-1781`,
  **exactly as it is**. The segment-table lookup peer is `SegmentedMap::new` (`:870`); the
  ceiling peer is `flat_value_ceiling_crossed` (`:380`); the in-crate unit-test convention
  is the module's own at `:2776-2780`.
- **Prior-art check (triage cycles):** run at Plan by affected file path. `repoint_chunk` is
  **absent** from `origin/main` — `git grep` finds one mention, a deferral comment at
  `crates/custodian/src/backfill.rs:112` naming **#682** (this work's grandparent) as its
  owner. Merged on `metadata.rs`: `d2609b2` (#710 flat ceiling helper), `99c7fcf` (shared
  resolver), `3e05891` (segmented record shape + codec) — every named prerequisite is in.
  **Open PRs: none in the repo at all**, settling the parent brief's previously unverifiable
  "no open PR touches these paths". No closed/rejected prior attempt outside this lineage.
- **Ordering note:** wave 0 — no in-batch prerequisite; every external one is merged.
  **External conflict (carry onto the filed issue): #772**, which edits the same file
  (`owner`/`staged` into `PendingEntry`, `metadata.rs:1556`). It is `PLANNED [blocked-by:
  771]` with #771 still `ITERATE_DO`, so ≥2 waves out. The parent's inherited
  `Conflicts with: 717` is **stale and deliberately dropped** — #717 is `COMPLETE [close: no
  PR]`, split, ships no code. **Cite by symbol, not line number, below `metadata.rs:1556`.**
- **Disposition hint:** likely-fix
- **Difficulty:** medium — **re-rated at Plan, not inherited.** Blast-radius only: **one
  file**, one new self-contained API, **zero existing call sites changed** (nothing calls it
  until child-2). Not `low` — the pin semantics are subtle and `wyrd-core` is widely depended
  on — but the parent's `high` came from spanning two crates and rewriting the repair
  lifecycle, both of which this child sheds. Edge-case density is high and is deliberately
  NOT what this field measures; the mutation gate covers that.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
- **Slug:** reconstruction-completes-seg-repair-through-primitive
- **Defect:** **A chunk whose `ChunkRef` lives in a `seg:` record is never repaired.**
  #697 stopped reconstruction aborting on a segmented object but deliberately writes
  nothing: the repair obligation is routed to `Site::Refused`
  (`crates/custodian/src/reconstruction.rs:552`) and answered `Assessment::Refused`
  (`:609`), every pass, forever — the obligation is not drained (data loss) and, until
  child-1, nothing could move the placement. With child-1 merged the primitive **exists but
  nothing calls it**: a multipart-published object's redundancy still decays untended,
  permanently. That is the C-1 violation this bundle exists to close — see **Invariant to
  restore**.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
  passes, driven **only** through symbols visible on the base *after child-1 merges* —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, ReconstructionContext,
  Reconciled}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{seg_key, inode_key, encode, decode, MAX_ROOT_VALUE_BYTES,
  SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord, ChunkRef,
  EcScheme}` (**`MAX_ROOT_VALUE_BYTES`, not `MAX_VALUE_BYTES`** — leg 5 pins the V/2 bound)
  — over in-memory `MetadataStore` / `ChunkStore` doubles. Five legs, the parent's, with its
  two corrections kept:
  1. **BINDING, RED pre-fix** — a `seg:`-resident under-replicated chunk is repaired:
     rebuilt fragment on a healthy D server in a distinct failure domain; the **`seg:`
     record's** `ChunkRef.placement` names it; obligation **drained**; pass answers
     `Changed`; **root bytes unchanged**. Base: refused, queued, byte-identical → red.
  2. **BINDING, RED pre-fix** — a concurrent rewrite of a *sibling* chunk in the same
     `seg:` record is **MERGED**: repair lands **and** the sibling's new placement
     survives. Base: refused → red. This is the leg that goes red if the primitive had
     pinned whole-record bytes instead of the three pins.
  3. **Not independently red** — the *planned* chunk rewritten under the plan is a
     **CONFLICT**: no repair-owned METADATA written (the `seg:` record holds exactly the
     competing writer's placement, byte for byte; root untouched; obligation **still queued**;
     no orphan mark published). Do **not** assert the destination fragment's absence and do
     **not** delete it — production writes fragments before the commit (`:931-935`); the
     stranded fragment is the tracked leak **#723**, inherited as-is (DECIDED at the parent's
     Plan 2026-08-10 — do not re-open, do not implement 0016's X47 pre-mark or drain fence,
     do not reword the two "collectable garbage" comments; #723 owns them). Whether the pass
     may still certify `Satisfied` here is a **separate scope decision left to the human**
     (iteration-4 carry-forward) — this leg asserts the store, not the verdict.
  4. **Not independently red** — a superseded root generation is a **CONFLICT**: no
     *repair-owned* metadata written (the leg's own setup writes the competing root, so no
     blanket no-write assertion), obligation still queued.
  5. **Not independently red** — the ceiling refusal holds over a segment record at the
     **V/2** bound (child-1's segmented-arm check, exercised through the whole pass):
     refused, record byte-identical, obligation queued, pass non-certifying.
  **The race window is reachable deterministically — hook the RIGHT read.** The resolver reads
  the group's `seg:` range with **`scan_page`**, never `get` (`read_group_range`,
  `metadata.rs:2452-2461`, docstring `:2417-2425`); the move's own read is the **only** `get`
  on a `seg:` key. Apply the racing batch **after returning the `scan_page` page**
  (equivalently: on the first `get` of that key, before answering it). Counting `get`s and
  injecting after the first return does NOT work — it quietly inverts leg 2. Leg 4's root flip
  goes on the way into `commit`, after resolve. This gap sank the parent (`RaceAtRepoint`
  injected inside `commit()`) — do not reproduce that shape.
  **Two forced production fixes from iteration 3**, asserted in the rewritten
  `segmented_map_reconstruction.rs`: (a) refusal containment counts once per **object**, not
  per obligation (`reconstruction.rs:322`); (b) an unattributable committed object routes
  through `reading.contain(&key, ...)`, never a bare `continue` (`:513-515`) — fail closed, no
  obligation deleted for a chunk a committed map still references.
  **Verification posture:** legs 1–2 are the C4-verify red→green; legs 3–5 pass on the base
  by construction and are bound by the mutation oracle, each with its named negation
  *demonstrated* in `build-notes.md`. Mechanics and the hard constraint on keeping the red
  compilable: see **Falsifiability** below.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** on the target checkout, seed a committed segmented object as raw
  `seg:` records plus a segmented root (per
  `crates/custodian/tests/segmented_map_restore.rs:387-431` — this build ships no producer
  of segmented maps, which is why the fixture hand-writes them) with a lost fragment;
  enqueue its repair; run `reconcile_step` with a `ReconstructionContext`. The obligation
  is refused (`reconstruction.rs:552`, `:609`) and stays queued, every pass, forever —
  deterministic, no seed sweep, no race window needed to observe it. Still true after
  child-1 merges: the primitive exists, nothing calls it.
- **Scope:** the repair pass stops refusing a `seg:`-resident chunk and completes the move
  through **child-1's primitive**. The placement change, the obligation discharge
  (`repair::repair_key` delete) and the orphan evidence per displaced position stay **ONE
  batch** — the caller adds its evidence to the batch the primitive hands back
  (`0005:277`, ADR-0015); if the primitive's shape makes that awkward, the finding goes
  back to child-1, this child does not fork the batch. Mirror `repair_chunk` and the seeding
  fixture (see **Citations expected**). Duplicate chunk ids get one plan, not independent
  ones — the narrow rule only, no cross-object claim-counting (#651's replan). **A losing
  CAS does not retract already-published bytes** — settled, rejected 4×
  (`results/issue_638/review-rejected.md:15-16`); refusal and conflict paths write no
  repair-owned metadata at all. **Salvage:** the reconstruction caller in
  `results/issue_711/iteration-v1/patch.diff` (harness repo) passed C4-ci and C4-verify —
  reuse it minus everything in the rebalance and DST files (#722's). This bundle's own
  `iteration-v4/patch.diff` is the most refined prior attempt.
  **Untouched:** `crates/core/src/metadata.rs` entirely (child-1 owns it — if this child
  needs a core edit, the split is wrong: STOP and hand back); `rebalance.rs`, `backfill.rs`,
  `restore.rs`, `gc.rs`, `desired_state.rs`; `custodian/tests/segmented_map_rebalance.rs` and
  `crates/dst/tests/custodian.rs` (#722); the read side (`resolve_chunk_map`, the
  #695/#696/#697 containment rule); any ADR/spec/proposal, conformance vector, or new
  dependency. **Forced edit, budgeted:** `custodian/tests/segmented_map_reconstruction.rs:484`
  (`an_obligation_inside_a_segmented_object_is_refused_never_discarded`) asserts the refusal
  this child removes and is rewritten to assert the repair lands.
  **Budget:** 3 files — `custodian/src/reconstruction.rs`,
  `custodian/tests/segmented_map_repoint.rs` (**new**),
  `custodian/tests/segmented_map_reconstruction.rs` — ≤ 100 added semantic non-test lines,
  `patch.diff` **≤ 85 KB**. (Budget calibrated at Plan against the parent's v4 patch: the
  custodian half of it measured **~74 KB**, so the 60 KB first proposed was unachievable and
  would have failed its own budget on day one. 85 KB leaves headroom and still sits well under
  the 95 KB cap the parent's 107 KB blew.) Downstream #722's ordering is in the **Ordering
  note**: its real prerequisite is child-1, not this child.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/custodian/tests/segmented_map_repoint.rs` — a **NEW** file,
  completing the `segmented_map_*` family (`_consumers` #650, `_restore` #651, `_backfill`
  #695, `_rebalance` #696, `_reconstruction` #697). Why NEW and why no `#![cfg(...)]`:
  **Falsifiability**.
- **Falsifiability:** **Checked against this project's gate at Plan, not assumed.** The test
  ships as a **NEW** `*/tests/*.rs`, which `run-verify.sh:143` (`_added_files` +
  `_is_test_file`) classifies as an added test, so the gate takes the full revert branch
  (`:505-512`) — production reverted, added test kept — and measures a genuine red→green.
  It carries **no** `#![cfg(...)]`, so it compiles in both legs (`_crate_cfgs`); otherwise it
  would report `0 tests` and be UNVERIFIABLE (exit 77) in both. **The red is earned against
  base+child-1:** `_resolve_base_ref` honours `$PDCA_VERIFY_BASE` (the wave's folded branch)
  at precedence `$PDCA_BASE > $PDCA_VERIFY_BASE > $WYRD_VERIFY_BASE > $PDCA_BRIEF_BASE`, so a
  wave-1 bundle verifies against a tree that **contains** the primitive — the red reads "it
  exists and nothing calls it", never "the symbol is missing". **HARD CONSTRAINT keeping the
  red compilable:** the test must name no symbol *this* patch introduces, and must not call
  child-1's primitive directly — drive through `reconcile_step`, assert on the **store**.
  Expect 5 tests ran, 2 failing; read the count as a count.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost** (`docs/principles.md` §5 C-1 at `:109`; §6 *Storage lifecycle /
  reclamation* at `:137`; `0016:2802-2813`; `gc.rs:22-25`). Over the system, not the module:
  **no committed chunk may sit in a state no actor can move it out of.** A `seg:`-resident
  under-replicated chunk is exactly that today. Restored **only when the pass completes
  through the write path** — a quieter, better-counted or better-explained refusal does not
  restore it; per §1.2 this **structural** target outranks "smallest diff".
  **Plan-exit gate (structural/lifecycle) — PASSED:** (1) Scope names a mechanism? **No** —
  it names removing the refusal and completing through an existing primitive. (2) Satisfiable
  by guarding one module? **No** — the write must land in the `seg:` record and the
  obligation must drain.
- **Surfaces:** data
- **Citations expected:** cite `path:line` on the target branch for every change.
  **Peer callsite to mirror — composition slice, do not re-derive the shape:** `repair_chunk`
  (`reconstruction.rs:829-956`) is the flat arm of this exact operation — follow how it plans,
  orders the fragment write before the commit (`:931-935`), and composes one batch. Move its
  ceiling refusal (`:923-929`) *inside* the primitive's answer rather than duplicating it. The
  refusal sites removed are `:552` (`Site::Refused`) and `:609` (`Assessment::Refused`), both
  re-verified on `origin/main` at Plan. Fixture seeding idiom (raw `seg:` records + segmented
  root, hand-written because this build ships **no producer** of segmented maps):
  `crates/custodian/tests/segmented_map_restore.rs:387-431`.
- **Prior-art check (triage cycles):** run at Plan by affected file path. Merged on
  `reconstruction.rs`: `1f871ce` (#697 — the refusal this child completes), `d2609b2` (#710 —
  flat-arm ceiling), `9470de5` (#638 — fragment-write deadline). **Open PRs: none in the repo
  at all**, so nothing in flight touches these paths. The only prior attempts are this
  bundle's four archived iterations and parent #711's — salvage, not shipped prior art.
- **Ordering note:** wave 1 — **`Depends on: child-1`** is a genuine build-on dependency: the
  production code *calls* the primitive, so it neither compiles nor goes honestly red without
  it; the wave fold supplies child-1's accepted diff via `$PDCA_VERIFY_BASE`, so no human
  merge is needed between them. **No conflicts:** touching only `crates/custodian/*`, this
  child collides with neither #772 (`metadata.rs`) nor #722 (`dst/tests/custodian.rs`) — a
  real gain of cutting by layer. Downstream **#722**'s true prerequisite is **child-1**, not
  this child (disjoint files: `rebalance.rs` vs `reconstruction.rs`), so it may run in
  parallel with this child rather than behind it.
- **Disposition hint:** likely-fix
- **Difficulty:** high
- **Depends on:** child-1
<!-- pdca:end child-2 -->
