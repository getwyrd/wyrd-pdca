- **Slug:** segmented-repair-completes-through-repoint
- **Defect:** **A chunk whose `ChunkRef` lives in a `seg:` record can never be repaired.**
  #697 stopped reconstruction aborting on a segmented object, but it deliberately writes
  nothing: a repair obligation for a `seg:`-resident chunk is **refused and stays queued**,
  every pass, forever (`crates/custodian/src/reconstruction.rs:552` routes it to
  `Site::Refused`, `:609` answers `Assessment::Refused`). Nothing exits that state — the
  obligation is not drained (that would be data loss), and no code path can move the
  placement, because the only placement writer in the tree rebuilds an **inode** record:
  `repair_chunk` (`:829`) takes `object.prior.chunk_map.as_flat()` at `:894`, aborts if it
  is `None`, and CASes `inode:` at `:937-953`. It can address a
  `seg:<nonce>:<epoch>:<index>` record not at all. So a multipart-published object's
  redundancy decays untended, permanently.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
  passes, driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, Reconciled}`, `wyrd_core::repair::{enqueue_repair,
  queued_repairs, repair_key}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Five legs:
  1. **BINDING, RED pre-fix — a `seg:`-resident under-replicated chunk is repaired.** Seed a
     committed **segmented** object (raw `seg:` records + a segmented root, never a
     committer) whose chunk has lost a fragment; enqueue its repair; run `reconcile_step`
     with a `ReconstructionContext`. Assert: the rebuilt fragment is on a healthy D server in
     a failure domain distinct from the survivors; the **`seg:` record's** `ChunkRef.placement`
     names it; the repair obligation is **drained** (`queued_repairs` no longer contains it);
     the pass answers `Changed`; and the **root** record's bytes are **unchanged** (a repoint
     rewrites the segment, never the root). Base behaviour: refused, obligation still queued,
     `seg:` bytes byte-identical → **red**.
  2. **BINDING, RED pre-fix — a concurrent rewrite of a DIFFERENT chunk in the same segment
     record is MERGED, not conflicted.** Same fixture, but a competing writer moves a
     *sibling* chunk's placement inside the same `seg:` record between the pass's resolve and
     its commit. Assert **both** survive: the repair lands (obligation drained, pass answers
     `Changed`) **and** the sibling's new placement is still in the record afterwards. Base:
     refused → **red**. This leg pins the design decision below and is the one that would go
     red if the primitive instead pinned the whole resolved record's bytes.
  3. **NOT independently red — the same chunk rewritten under the plan is a CONFLICT.** A
     competing writer moves **the planned chunk's own** placement between resolve and commit.
     Assert: **no METADATA is written** — the `seg:` record still holds exactly the
     competing writer's placement, byte for byte; the root is untouched; the repair obligation
     is **still queued**; no orphan mark was published; the pass does not certify. Note the
     deliberate wording: the rebuilt destination **fragment** may already be on the D server,
     because the production ordering writes fragments before the commit
     (`crates/custodian/src/reconstruction.rs:931-935`) — do **not** assert its absence, and do
     **not** delete it (retracting a published write is the rule #638 rejected 4×). See the
     stranding note in Scope: that fragment is a known, pre-existing leak (getwyrd/wyrd#723),
     not garbage. Pre-fix the pass refuses, so
     this leg also passes on the base — it is **not** C4-verify evidence. It is the leg the
     **mutation** oracle needs: this is the sign-off's named requirement, and `build-notes.md`
     MUST record the named negation — *deleting the `chunk == prior` equality turns leg 3
     red* — demonstrated, not asserted. (Without the pin, a chunk matched on byte offset
     alone is rewritten onto freshly-read bytes and the competing writer's placement is
     silently reverted; the adversary reproduced exactly this.)
  4. **NOT independently red — a superseded root generation is a CONFLICT.** The root is
     flipped to a different generation between resolve and commit. Assert that **the repair
     wrote no metadata of its own** — no placement change, no orphan mark, no obligation
     delete, and the obligation stays queued. Phrase it as *repair-owned* metadata, not
     "nothing is written": the leg's own setup necessarily writes the competing root
     generation, so a blanket no-write assertion would contradict the fixture.
     `build-notes.md` records the named negation for the root precondition too.
  5. **NOT independently red — the ceiling refusal holds over a segment record, at the V/2
     bound.** A `seg:` record seeded just under the bound whose repoint would cross it:
     refused, record byte-identical, obligation queued, pass non-certifying. #710 established
     the rule for the flat arm and its `custodian/tests/placement_ceiling.rs` is on this base;
     this leg pins it for the segmented arm, which #710 could not.
     **WHICH bound — decided at Plan, flip it at sign-off if you disagree, do not re-derive it
     in Do.** A `seg:` record is weighed against **V/2** (= `MAX_ROOT_VALUE_BYTES`, `50_000`,
     `metadata.rs:352`), **not** the full `MAX_VALUE_BYTES`. Evidence for: 0016's knob table
     bounds `MAX_SEG_CHUNKS` by "same rule against a `seg:` record" as the flat map's
     `max_chunkref_bytes × N ≤ V / 2` headroom (`0016:1462-1467`), so a conforming publication
     never writes a `seg:` value above V/2, and a maintenance repoint that lands one in
     50_001..100_000 mints a record **no publication could have produced and no
     re-publication could reproduce**. Evidence against, recorded so this is a choice and not
     an oversight: the *resolver* refuses a stored `seg:` row only above the full
     `MAX_VALUE_BYTES` (`metadata.rs:2493`), and #710's helper
     `flat_value_ceiling_crossed` (`:380`) is V-bound with a doc comment (`:371-375`) that
     assigns `MAX_ROOT_VALUE_BYTES` to a segmented **root's** write specifically. Read the
     resolver's V as a *containment* bound for records a non-conforming writer already
     produced — not a licence for maintenance to write into that band. Consequence for Scope:
     the **flat** arm routes through #710's V-bound helper unchanged; the **segmented** arm
     weighs V/2. That is applying an existing base-visible constant, not authoring a second
     ceiling: a differently-*named* helper for the segment arm is fine, a second ceiling
     *value* is not.

  Legs **1 and 2 are the discriminating evidence**; 3, 4 and 5 pass pre-fix by construction
  and must not be counted as red. **Additionally**, `crates/core/src/metadata.rs` gains
  in-crate `#[cfg(test)]` unit tests for the two addressing helpers the primitive introduces
  (offset-plus-equality lookup, and segment coverage), mirroring the module's own convention
  at `metadata.rs:2776-2780` — the C5 residue was 17 missed mutants, all in this new code.

  **The read→prepare window IS reachable deterministically — legs 2, 3 and 4 are not
  aspirational — but hook the RIGHT read.** The two reads are on **different `MetadataStore`
  methods**, which is the whole trick: the resolver reads the group's `seg:` range with
  **`scan_page`**, never `get` (`read_group_range`, `crates/core/src/metadata.rs:2452-2461`,
  and its docstring at `:2417-2425` explains why `scan` is refused there), while the move's own
  read is the **only `get`** anyone performs on a `seg:` key. So the window is *between the
  resolver's `scan_page` return and the move's `get`*, and the double reaches it by applying
  the racing batch **after returning the `scan_page` page** (equivalently: on the first `get`
  of that `seg:` key, *before* answering it). Counting `get`s and injecting after the first
  return does **not** work — there is only one `get`, and it is already the move's, so the
  racing write would land after the move captured its CAS bytes and leg 2's sibling edit would
  *conflict* instead of merging, quietly inverting the property the leg exists to pin. Leg 4's
  root flip is later and easier: apply it on the way into `commit`, after the resolve has
  completed, so the pass does not simply restart onto the new generation the way
  `resolve_chunk_map` would if the root moved during the resolve itself. **This is the gap that
  sank the parent attempt** — its DST double (`RaceAtRepoint`) applied the racing batch *inside*
  the repoint's own `commit()`, strictly after the primitive's read, so it was structurally
  unable to reach the window and the `chunk == prior` pin went unexercised. Do not reproduce
  that shape here.
- **Falsifiability:** legs 1 and 2 go RED on the ordinary base — `origin/main` at `92e1b4b`,
  no special topology, no external service. The forbidden state is *reachable by seeding*: a
  segmented object is written as raw `seg:` records plus a segmented root (this build ships
  no producer of segmented maps, which is exactly why the fixture hand-writes them, as
  `crates/custodian/tests/segmented_map_restore.rs:387-431` already does). The failure is
  deterministic and present on every pass, so no seed sweep or race window is needed to
  observe it: `reconcile_step` answers with the obligation still queued and the `seg:` bytes
  untouched. Verified by dry-running the gate's classifier — the added
  `crates/custodian/tests/segmented_map_repoint.rs` is the discriminator, the gate runs
  `-p wyrd-custodian --test segmented_map_repoint`, and that file carries no `#![cfg(...)]`,
  so it is genuinely compiled and executed in both legs (`run-verify.sh:_crate_cfgs`,
  `:363-373`). Legs 3–5 are falsifiable only against the **mutation** oracle, which is why
  each carries a required named negation rather than a red claim.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost: every durable byte is, at every instant, protected by a record that names
  it *or* evidenced for reclamation, and every state has an actor that exits it in bounded
  time** (`docs/principles.md:137`, §6 row *Storage lifecycle / reclamation*, sourced to §5
  C-1 at `:109`; the maintainer's standing rule of 2026-07-25; `0016:2802-2813`;
  `crates/custodian/src/gc.rs:22-25`). A refused-forever repair obligation is a state with
  **no** actor that exits it. The invariant is restored only when the maintenance write path
  for a `seg:`-resident chunk **exists and the repair pass completes through it** — not when
  the refusal is made quieter, better-counted or better-explained. Guarding, annotating or
  re-classifying `metadata.rs` alone satisfies nothing here.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; M4's
  integration branch is merged and deleted, and every #635 slice to date landed on `main`
  directly. Base at authoring: `92e1b4b`.)
- **Conflicts with:** 717
- **Ordering note:** **Wave 0 — no in-batch prerequisite.** Every external prerequisite is
  already **merged** into `origin/main`: #710 (the ceiling helper `flat_value_ceiling_crossed`,
  `metadata.rs:380`) as PR #718, and #695/#696/#697 as PRs #704/#705/#706 — verified with
  `git -C ../wyrd log --oneline origin/main` and `gh issue view`, all four CLOSED. So no
  `Depends on (merged):` is required; do not add one. **Never share a wave with #717** — it
  inserts `owner`/`staged` into `PendingEntry` at `metadata.rs:1528`, shifting every citation
  below that point in this child's largest file. (#717 is the terminal child of #692's
  2026-08-09 split, #715 → #716 → #717; #715 and #716 touch only `crates/core/src/multipart.rs`
  and their own new test files and share nothing with this child.) **Cite by symbol, not by
  number** where a citation sits below `:1528` in `metadata.rs`. #722 stacks on this child
  and must not start until this child's PR is **merged** — with `auto_merge = false` the driver
  stops at the wave boundary and the human merges (INTEGRATION §2).
- **Difficulty:** high
- **Scope:** **the missing maintenance write path for a `seg:`-resident chunk, and
  reconstruction completing through it.**
  - `crates/core/src/metadata.rs` — the placement move: given a resolved generation, the byte
    offset of the chunk within the object, the `ChunkRef` the caller planned from, and the new
    placement, produce the compare-and-swap batch that lands the move in whichever record
    holds that `ChunkRef` — flat inode **or** segment record — plus the in-crate unit tests for
    its addressing helpers. It **hands the batch back** rather than committing: the caller adds
    its own evidence for the same move (the obligation delete, the orphan marks) and lands all
    of it in ONE mutation (`0005:277`, ADR-0015). Weigh the re-encoded record before writing
    anything: the **flat** arm through **#710's** `flat_value_ceiling_crossed`
    (`metadata.rs:380`) unchanged, the **segmented** arm against **V/2** — see leg 5, which
    settles which bound and why. Do not re-implement #710's guard, and do not introduce a
    second ceiling *value*.
  - **WHAT IT PINS — settled at Plan, do not re-derive.** `ResolvedChunkMap`
    (`metadata.rs:2294-2300`) carries only `record` + the flattened `chunks`; it **cannot**
    hand back per-segment bytes, so "pin the exact bytes the resolve read" is not
    implementable for the segmented arm and must not be claimed. The move pins **three**
    things: the **root generation's** bytes (a supersede always flips the root first,
    `0016:2452-2462`, so a repoint racing one loses its CAS); the **segment record's own
    freshly-read bytes**; and the **`ChunkRef` itself** — the chunk moved is the one that
    begins at the given offset **and equals** the reference the caller planned from, anything
    else is a conflict. **A concurrent edit to a *sibling* chunk in the same segment record is
    therefore MERGED, deliberately** — two repairs inside one multipart object must not
    serialise on the whole record — **while an edit to the planned chunk itself is a
    conflict.** Leg 2 pins the first half, leg 3 the second. Any prose Do writes about what the
    move pins must say this; the archived attempt's three doc sites saying otherwise are a
    **known defect to correct, not a spec to follow**.
  - `crates/custodian/src/reconstruction.rs` — the repair pass stops refusing a `seg:`-resident
    chunk (#697's placeholder at `:552` / `:609`) and completes the move. The placement change,
    the discharge of the repair obligation (`repair::repair_key` delete) and the orphan evidence
    for each displaced position stay **one batch** — do not split the batch to fit the new
    primitive; if the primitive's shape makes that awkward, change the primitive.
  - `crates/custodian/tests/segmented_map_reconstruction.rs` — **that file's own** second leg
    (`an_obligation_inside_a_segmented_object_is_refused_never_discarded`, `:484` — not to be
    confused with the success criterion's leg 2 above) asserts the
    refusal this child removes and MUST be rewritten to assert the repair now lands. This is a
    **forced** edit, budgeted for, not drift.
  - **Constraints carried forward (blockers from #651 / #638 — these bound the shape, they do
    not name it):** duplicate chunk ids get one plan, not independent ones — keep it to the
    narrow rule, do **not** rebuild the cross-object claim-counting apparatus dropped at #651's
    replan. **Bounded memory:** pin the bytes of **one** record at a time; find the covering
    segment in the root's own table (the tiling is contiguous and checked at decode,
    `SegmentedMap::new`, `metadata.rs:870`), so no `seg:` range is walked and no other segment
    is decoded; do not retain the namespace's decoded chunks and do not deep-copy a segmented
    root into every plan. **A losing CAS does not retract already-published bytes** — settled,
    rejected 4× in #638 (`results/issue_638/review-rejected.md:15-16`); the refusal and conflict
    paths write **no METADATA at all** — the destination fragment written ahead of the
    commit stays where it is (see the stranding note in Out of scope). Keep `commit_chunk_map`'s CAS idiom for the flat arm
    (`metadata.rs:1769-1797`: `version = prior.version + 1` and `..prior.clone()`, so ADR-0047
    object metadata is **preserved**); its own segmented refusal at `:1776-1780` **stays** —
    `commit_chunk_map` is not what this child changes.
  - **Budget:** ≤ **4** files — `core/src/metadata.rs`, `custodian/src/reconstruction.rs`,
    `custodian/tests/segmented_map_repoint.rs` (**new**), `custodian/tests/segmented_map_reconstruction.rs`
    — ≤ **250** added **semantic** lines of non-test code (non-blank, non-comment, and
    excluding both `tests/` files and `#[cfg(test)]` modules, so the in-crate unit tests above
    do not count against it), and `patch.diff` ≤ **95 KB** (the
    driver's size backstop trips at 100 KB, and the parent's attempt hit 124 KB). A fifth file
    means the shape is wrong; in particular needing to edit `rebalance.rs`, `backfill.rs`,
    `restore.rs`, `gc.rs` or `desired_state.rs` means the scope has drifted: **STOP and hand
    back a proposed split.** Keep the new test lean by reusing the fixture shape cited below
    rather than re-authoring one.
    **On the sizer's `oversized` verdict:** this child trips it, and at Plan the reasons were `brief ~26 KB (cutoff 12 KB)` plus difficulty — i.e. it sizes this brief's PROSE, long because it carries two plan-review rounds' corrections, not the slice. The caps just above are the slice, against the parent's actual 7 files / 1595 added lines / 124 KB. `sizing.py:263-267` puts that predictor at 55% precision and reports it separately for exactly this reason. Judged a FALSE POSITIVE at Plan (2026-08-10), after the split that produced this child; do **not** re-split on it without new evidence from the diff itself.
  - **Out of scope:** the **drain / evacuation** caller and its tests, and the DST
    repoint-versus-supersede property (**#722** — this child must leave
    `crates/custodian/src/rebalance.rs`, `crates/custodian/tests/segmented_map_rebalance.rs`
    and `crates/dst/tests/custodian.rs` untouched). The write-side ceiling helper itself
    (**#710**, merged — consume it). **The committer, the destination pre-mark, the drain fence,
    rollback and resume (#653).** Proposal 0016's full segment-repoint precondition set
    (`0016:669`) is `require(seg == prior)` + `require(inode == prior)` + `require(orphan:<P_new>
    == prior)` (the destination pre-mark) + `require_absent(desired:dserver:<S_new>)` (the drain
    fence); **this child ships only the first two.** That is the parent issue's own carve-out —
    but it is a **sharper sign-off item than the parent brief claimed, and the correction is
    load-bearing.** Without the pre-mark, a repoint that loses its CAS leaves the already-written
    destination fragment **unreferenced AND unmarked**, and such a fragment is **not** collected:
    GC reclaims only on an orphan mark past its grace or an expired pending lease, and otherwise
    *conservatively keeps* it — "no evidence the grace window elapsed — conservatively keep it
    (reader-safe: a fragment is never reclaimed without a deadline)"
    (`crates/custodian/src/gc.rs:196-212`). So it is a **permanent leak, not "collectable
    garbage"**; the in-tree comments that call it garbage
    (`crates/custodian/src/reconstruction.rs:931-935`) are inaccurate, and the parent brief's
    claim that GC's "ordinary unreferenced sweep" reclaims it was **false** — do not repeat it.
    Proposal 0016 knows this and answers it with exactly the pre-mark: X47 requires the repoint
    to "pre-mark `orphan:<P_new>` **before** writing the destination fragment", so a lost CAS
    leaves the mark standing for GC (`0016:2577`, `0016:669`). What remains true is the
    *comparative* claim: the **flat** repair path already behaves identically today, so this
    child **introduces no new stranding class** — it extends an existing one to a second record
    shape. **DECIDED AT PLAN, 2026-08-10 — do not re-open it in Do or at sign-off.** The
    pre-existing leak is filed as **getwyrd/wyrd#723** ("reconstruction/rebalance strand an
    unreclaimable fragment when the placement CAS loses", milestone *Foundations*), which owns
    the flat path's leak, the 0016 X47 pre-mark that closes it, and the two inaccurate
    "collectable garbage" comments. This child therefore ships the extension **as is**: it
    inherits a tracked defect rather than creating an untracked one, which is what resolves the
    tension with C-1 — the permanent failure mode has a named owner and a bounded closure, so
    it is no longer an *accepted* cost. Do **not** implement the pre-mark or the fence here, do
    **not** re-argue the trade in `build-notes.md`, and do **not** "improve" the two garbage
    comments in the files this child touches — #723 owns that wording, and editing it here
    would put this child's diff into a second slice's territory. Also out: the chunk-id floor (**#652**, merged);
    restore and `desired_state` (**#651**, merged); `gc.rs` / `scrub.rs` (**#650**, merged);
    `backfill.rs` (**#695**, merged — untouched here). The read side generally: no new resolving
    walk, no change to `resolve_chunk_map`, no change to the containment rule #695/#696/#697
    landed. Any new or edited ADR / spec / proposal (0016 is a **draft** and stays untouched);
    any conformance-vector change; any new dependency.
  - **KEEP THE DISCRIMINATOR ASSERTION-RED — HARD CONSTRAINT.** The new test MUST NOT name the
    primitive or any other symbol this patch introduces. The RED leg reverts production
    (`run-verify.sh:469-476`), so such a reference makes the target fail to **compile** and the
    gate reports UNVERIFIABLE (exit 77, `:492-500`) instead of a red. Drive everything through
    `reconcile_step` and observe the **store**. `MAX_VALUE_BYTES` is base-visible and may be
    named.
- **Repro instruction:** on the target checkout, read the binding commit with
  `git -C ../wyrd show origin/main:crates/custodian/src/reconstruction.rs` — `:894` takes
  `as_flat()` and aborts on `None`, `:937-953` CASes `inode:`; nothing addresses a `seg:`
  record. Then seed a committed segmented object (raw `seg:` records + a segmented root,
  per `crates/custodian/tests/segmented_map_restore.rs:387-431`) with a lost fragment, enqueue
  its repair, and run `reconcile_step` with a `ReconstructionContext`: the obligation is
  refused (`reconstruction.rs:552`, `:609`) and stays queued, every pass, forever.
- **External dependencies:** `typos`, `docs-renderer`, `cargo-deny`, `cargo-machete`, `cargo-mutants`
- **Test file:** `crates/custodian/tests/segmented_map_repoint.rs` — a **NEW** file, not
  optional, completing the `segmented_map_*` family (`_consumers.rs` #650, `_restore.rs` #651,
  `_backfill.rs` #695, `_rebalance.rs` #696, `_reconstruction.rs` #697). This project's
  `C4-verify` earns its red **only** from an *added* `*/tests/*.rs` (`run-verify.sh:_added_files`
  + `_is_test_file`, `:97-98`); a test appended to an existing file makes the gate take the
  green-only branch (`:454-464`) and prove no red. Confirmed at Plan by dry-running
  `run-verify.sh --classify` over a synthetic patch of this child's exact file set: it returns
  `ADDED_TEST crates/custodian/tests/segmented_map_repoint.rs`, and because that is the only
  added test the gate runs `-p wyrd-custodian --test segmented_map_repoint` — so the edit to
  `segmented_map_reconstruction.rs` ships in addition and is covered by C4-ci, not by the
  discriminator. The in-crate `metadata.rs` unit tests are likewise C4-ci's, not the
  discriminator's.
- **Verification posture:** the DEFAULT flippable-regression posture holds and is what
  C4-verify measures — legs 1 and 2 are red pre-fix and green post-fix. Declared here only to
  pre-empt a §6 surprise: **legs 3, 4 and 5 pass on the base too**, because pre-fix the pass
  refuses and therefore also writes nothing. The gate's summary line reports how many tests
  *ran* in the red leg, **not** how many failed — the parent's `check-gates.json` said "4
  test(s) ran red" when only 3 legs were actually discriminating, and the adversary had to
  correct it by hand. So: expect the red leg to report 5 tests ran with 2 failing, and read
  the count as a count. Legs 3–5 are bound by the **mutation** oracle instead, which is why
  each carries a required named negation in `build-notes.md` (delete the pin, show the leg go
  red, restore it) — an assertion that the negation *would* fail is not the evidence asked
  for. Nothing here is deferred off-Check: no Docker host, no env var, no live CI run.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **This is a composition slice — mirror these peers rather than invent a shape:**
  `crates/core/src/metadata.rs:1769-1797` (`commit_chunk_map`, the flat CAS idiom — `version + 1`,
  `..prior.clone()`); `crates/custodian/src/reconstruction.rs:829-956` (`repair_chunk`, the
  binding commit being replaced, including the `repair::repair_key` delete and the
  `gc::orphan_key` puts that must stay **in the same batch** as the placement change, and the
  ceiling refusal at `:923-929` that must now run inside the primitive);
  `crates/core/src/metadata.rs:2294-2300` and `:2647-2660` (`ResolvedChunkMap` /
  `resolve_chunk_map` — what a caller actually holds after a resolve, and therefore what the
  move can and cannot pin); `crates/core/src/metadata.rs:1258-1333` (`seg_key` /
  `seg_range_prefix` / `parse_seg_key` — the only sanctioned way to address a segment record);
  `crates/core/src/metadata.rs:1127-1200` (`SegmentRecord::new` / `chunks()` / `byte_offset()`,
  the validating constructor) and `:2536-2552` (`decode_segment_record`);
  `crates/core/src/metadata.rs:2493-2500` (the resolver's read-side `MAX_VALUE_BYTES` refusal —
  the boundary a write must not cross) and `:2582-2589` (the root-table extent invariant a
  placement-only rewrite must preserve); `crates/core/src/metadata.rs:2776-2780` (#710's in-crate
  boundary test — the convention the new unit tests follow);
  `crates/custodian/tests/segmented_map_restore.rs:387-431` (`seed_segmented` / `seed_damaged`:
  raw `seg:` + root seeding with a fixture self-check). **Salvage:** the archived attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a working
  primitive and a working reconstruction caller that passed C4-ci and C4-verify — reuse them,
  but (a) correct every doc site claiming the move pins "the exact bytes the resolve read", (b)
  add legs 2–5 and the in-crate unit tests, and (c) drop everything in the rebalance and DST
  files, which belong to #722.
- **Prior-art check (triage cycles):** by affected file path, across merged history and
  closed/rejected work. `git -C ../wyrd log origin/main -- crates/core/src/metadata.rs` → most
  recently `d2609b2` (#710, the ceiling helper this consumes), `b083ec4` (#652), `11aa85f`
  (#650), `99c7fcf` (#649, the shared resolver — the premise this builds on), `3e05891` (#648,
  the segmented record shape). None implements a placement move in a `seg:` record.
  `git -C ../wyrd log origin/main -- crates/custodian/src/reconstruction.rs` → `1f871ce` (#697,
  the containment this completes), the repair loop (#144) and its fixes (#197 *"don't count
  aborted repairs as successes"*, PR #238; #346 identity-placement fallback; #348 malformed
  placement). No open PR touches these paths. **Closed/rejected:** PR **#647** (CLOSED
  2026-07-30, unmerged) is the un-split ancestor and contained a `repoint`-shaped write; it was
  closed for **size and reviewability**, not direction. Its custodian-local
  `crates/custodian/src/resolve.rs` has been superseded by the shared resolver — **do not
  reintroduce it.** Within the harness, `results/issue_638/review-rejected.md:15-16` records the
  standing, four-times-rejected rule that a losing/late write is **not** retracted; do not
  re-litigate it.
- **Disposition hint:** likely-fix
