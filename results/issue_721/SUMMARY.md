# Result — issue 721 / segmented-repair-completes-through-repoint

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **A chunk whose `ChunkRef` lives in a `seg:` record can never be repaired.**
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
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
  passes, driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, Reconciled}`, `wyrd_core::repair::{enqueue_repair,
  queued_repairs, repair_key}`, `wyrd_core::metadata::{seg_key, inode_key, encode, decode,
  MAX_VALUE_BYTES, SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Six legs:
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
  3. **The same chunk rewritten under the plan is a CONFLICT — and the conflict answers
     exactly as the flat arm's does.** A competing writer moves **the planned chunk's own**
     placement between resolve and commit.
     Assert: **no METADATA is written** — the `seg:` record still holds exactly the
     competing writer's placement, byte for byte; the root is untouched; the repair obligation
     is **still queued**; no orphan mark was published; and — **MANDATORY, this is the leg that
     tests the certification decision below** — the pass answers **`Reconciled::Satisfied`**,
     the base's own answer for a conflict-only pass. Do NOT assert `Blocked` / "does not
     certify": on the target, `RepairOutcome::Conflict` only emits a counter
     (`crates/custodian/src/reconstruction.rs:311-318`) and conflicts are deliberately **not**
     in the pass's `hole` (`:341-357`, `hole = reading.incomplete || !reading.refused.is_empty()
     || ceiling_refused`), so a conflict-only pass answers `Satisfied` on `main` today for the
     **flat** arm. Widening `hole` to cover conflicts would change that long-standing flat
     behaviour and is **out of scope** — see *Certification semantics* in Scope. Note the
     deliberate wording: the rebuilt destination **fragment** may already be on the D server,
     because the production ordering writes fragments before the commit
     (`crates/custodian/src/reconstruction.rs:931-935`) — do **not** assert its absence, and do
     **not** delete it (retracting a published write is the rule #638 rejected 4×). See the
     stranding note in Scope: that fragment is a known, pre-existing leak (getwyrd/wyrd#723),
     not garbage. Pre-fix the pass answers `Blocked` (the segmented refusal puts a hole in the
     reading), so the mandatory `Satisfied` assertion makes this leg red on the base too — but
     that red is **coarse**: it distinguishes "refused" from "attempted", not a correct pin from
     a broken one. Do **not** present it as evidence for the CAS pin. That evidence is the
     **mutation** oracle: this is the sign-off's named requirement, and `build-notes.md`
     MUST record the named negation — *deleting the `chunk == prior` equality turns leg 3
     red* — demonstrated, not asserted. (Without the pin, a chunk matched on byte offset
     alone is rewritten onto freshly-read bytes and the competing writer's placement is
     silently reverted; the adversary reproduced exactly this.)
  4. **A superseded root generation is a CONFLICT.** The root is
     flipped to a different generation between resolve and commit. Assert that **the repair
     wrote no metadata of its own** — no placement change, no orphan mark, no obligation
     delete, and the obligation stays queued. Phrase it as *repair-owned* metadata, not
     "nothing is written": the leg's own setup necessarily writes the competing root
     generation, so a blanket no-write assertion would contradict the fixture. Assert the same
     pass answer as leg 3 (**`Reconciled::Satisfied`**) and for the same reason — a lost CAS is
     a conflict, not a hole. Coarse-red on the base for leg 3's reason; the binding evidence is
     again the oracle, and `build-notes.md` records the named negation for the root precondition
     too.
  5. **NOT independently red — the ceiling refusal holds over a segment record, at the FULL
     `MAX_VALUE_BYTES` bound.** A `seg:` record seeded just under `MAX_VALUE_BYTES` whose
     repoint would cross it: refused, record byte-identical, obligation queued, and the pass
     answers **`Reconciled::Blocked`** — a ceiling refusal *is* a hole (`ceiling_refused`,
     `reconstruction.rs:315-318`, `:341-357`), unlike a conflict. #710 established
     the rule for the flat arm and its `custodian/tests/placement_ceiling.rs` is on this base;
     this leg pins it for the segmented arm, which #710 could not.
     Build that record programmatically (a loop of `ChunkRef`s), never as a byte literal — a
     ~100 KB literal would blow the 95 KB `patch.diff` cap in Budget on its own.
     **WHICH bound — REVERSED at Plan on 2026-08-19 (plan-review round 3); this reverses the
     iteration-4 sign-off note and contradicts child #776's brief (see *Plan-review response*
     item 1 — the flip does not reach the children by itself), so it is the one thing to look at
     first at sign-off. Do not re-derive it in Do.** A `seg:` record is weighed against the **full `MAX_VALUE_BYTES`**
     (`100_000`, `metadata.rs:327`) — the same `>` boundary the resolver refuses a stored row
     on (`metadata.rs:2493`) and the same bound #710's `flat_value_ceiling_crossed`
     (`metadata.rs:380`) applies to the flat arm — **not** `MAX_ROOT_VALUE_BYTES` (V/2). Three
     reasons, in order of weight:
     - **V/2 would re-create this issue's own defect.** The resolver *reads* every `seg:` row
       through `100_000` (`metadata.rs:2493`), so a row in `50_001..100_000` is live, readable
       data. Refusing to repoint it leaves its repair obligation refused every pass, forever —
       the exact no-exit state named in Defect and forbidden by the Invariant. A maintenance
       write path must not refuse a record the read path accepts.
     - **The in-tree doc assigns V/2 to the segmented *root's* write, not to a `seg:` record.**
       `metadata.rs:371-375`: the half "exists to budget a **segmented root's segment table**
       against the reserve its object metadata is spent from, and a flat record has no segment
       table — its whole value *is* the record… A segmented root's placement write is #682's,
       and it is the one that must weigh `MAX_ROOT_VALUE_BYTES`." A `seg:` record has neither a
       segment table nor an ADR-0047 metadata reserve — it is a bare chunk list, structurally
       the flat body — so the rationale does not transfer to it. **And this child never
       re-encodes the root** (leg 1 asserts the root's bytes are unchanged), so V/2 is not at
       stake here at all: a design that finds itself re-encoding the root has drifted into
       #722/#653 territory — STOP, per the scope-stop rule in Budget.
     - **The tracker says so.** This child's own issue body specifies leg 5 as "a `seg:` record
       seeded just under `MAX_VALUE_BYTES` whose repoint would cross it" and "Weigh the
       re-encoded record through **#710's** `flat_value_ceiling_crossed` — do not re-implement
       the guard and do not add a second ceiling constant" (`notes.json`, body).
     Evidence the other way, recorded so this stays a choice and not an oversight: 0016's knob
     table bounds `MAX_SEG_CHUNKS` by "same rule against a `seg:` record" as `MAX_MAP_CHUNKS`'
     `max_chunkref_bytes × N ≤ V/2` headroom (`0016:1462-1467`), so a *conforming publication*
     never writes a `seg:` value above V/2. That is a **knob-sizing** rule, and #710 already
     settled how it composes with a maintenance write: `MAX_MAP_CHUNKS` is V/2-sized too, and
     the flat guard still weighs V. Same composition here. Practical note: a placement-only
     repoint grows a record by a few bytes per entry, so a conforming (≤ V/2) record cannot
     approach V — the band is reachable only for rows a non-conforming writer already stored,
     which is precisely where refusing costs a permanent no-exit and gains nothing.
     Consequence for Scope: **both** arms weigh V through #710's helper. No second ceiling
     *value*, and no V/2 comparison anywhere in this diff.
  6. **A zero-length chunk at a segment boundary is addressed by the record that actually holds
     it — never mis-addressed and never silently certified.** Seed a segmented object whose
     segment 0 ends with a **zero-length** `ChunkRef` (legal: `ChunkRef::len` has no nonzero
     invariant, `metadata.rs:128-139`, and `SegmentRecord::checked` rejects only an empty chunk
     list or a zero *total* span, `:1146-1162`) — so the zero-length ref and segment 1's first
     chunk share the same byte offset `B`. Keep the fixture legal: the segment carrying the
     zero-length ref must still have a **nonzero** `byte_len`, since the root table rejects a
     zero-length *segment* per entry (`metadata.rs:902-907`) — a zero-length **chunk** inside a
     nonzero segment is the representable shape, and the one to seed. Enqueue the repair of **segment 1's first chunk** (the one that
     genuinely lost a fragment). Assert: the rebuilt placement lands in **segment 1's** record;
     **segment 0's record is byte-identical**; the root is unchanged; the obligation is drained;
     the pass answers `Changed`. Then the mirror case, in the same leg or a sibling one: with
     the zero-length ref itself carrying the obligation, assert the pass **never** answers
     `Satisfied` with the obligation dropped — either it repairs it in segment 0 (its true home)
     or it writes nothing and answers `Blocked` with the obligation still queued. Red pre-fix
     (base refuses). This leg exists because the archived attempt got exactly this wrong: it
     selected the record by byte offset alone, picked the wrong record for a boundary chunk, and
     then **silently certified** — the sign-off's carried-forward `[impl]` finding, and this
     issue's own C-1 defect re-entering through another door. The addressing rule it tests is
     stated in Scope (*Boundary addressing*) and is **not** left to Do.

  **Which legs are red on the base.** Legs **1, 2 and 6** are the discriminating evidence — they
  fail on `main` because the repair is refused and pass green only when it lands. Legs **3 and 4**
  are *coarse*-red (base `Blocked` vs post-fix `Satisfied`): they pin the certification decision,
  **not** the CAS pins, which remain the mutation oracle's. Leg **5 passes pre-fix by
  construction** and must never be counted as red. So expect the red leg to report **6 tests ran,
  5 failing**; read the count as a count (see Verification posture). **Additionally**,
  `crates/core/src/metadata.rs` gains
  in-crate `#[cfg(test)]` unit tests for the two addressing helpers the primitive introduces
  (offset-plus-equality lookup, and segment coverage **including both zero-length placements —
  a zero-length ref at the start of a segment and one at its end, plus a zero-length ref at
  offset == the object's total length, which has no covering segment at all**), mirroring the
  module's own convention
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
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; M4's
  integration branch is merged and deleted, and every #635 slice to date landed on `main`
  directly. Base at authoring: `92e1b4b`; **re-verified 2026-08-19 at `a801997`** (one commit
  since, `9470de5`, touched the two production files — citations re-checked, see Falsifiability).)
- Scope: **the missing maintenance write path for a `seg:`-resident chunk, and
  reconstruction completing through it.**
  - `crates/core/src/metadata.rs` — the placement move: given a resolved generation, the byte
    offset of the chunk within the object, the `ChunkRef` the caller planned from, and the new
    placement, produce the compare-and-swap batch that lands the move in whichever record
    holds that `ChunkRef` — flat inode **or** segment record — plus the in-crate unit tests for
    its addressing helpers. It **hands the batch back** rather than committing: the caller adds
    its own evidence for the same move (the obligation delete, the orphan marks) and lands all
    of it in ONE mutation (`0005:277`, ADR-0015). Weigh the re-encoded record before writing
    anything: **both** arms through **#710's** `flat_value_ceiling_crossed`
    (`metadata.rs:380`, the full `MAX_VALUE_BYTES`) — see leg 5, which settles which bound and
    why, and reverses the iteration-4 note that said V/2. Do not re-implement #710's guard, do
    not introduce a second ceiling *value*, and let **no** `MAX_ROOT_VALUE_BYTES` comparison
    appear in this diff: that constant bounds a segmented **root's** write
    (`metadata.rs:371-375`), and this child never re-encodes the root.
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
  - **BOUNDARY ADDRESSING — the rule, settled at Plan, not left to Do.** "Find the covering
    segment by byte offset" is **not sufficient**, and getting it wrong is what the archived
    attempt did: a **zero-length** `ChunkRef` is legal (`ChunkRef::len` carries no nonzero
    invariant, `metadata.rs:128-139`; `SegmentRecord::checked` rejects only an empty chunk list or
    a zero *total* span, `:1146-1162`), so a chunk's offset can coincide with a segment boundary and the
    half-open covering test picks the **wrong record** — after which the `ChunkRef` equality
    fails, the repair quietly does nothing, and the obligation is refused or certified away
    forever. The rule:
    1. **Candidates, from the root's own table only** (contiguous tiling from 0, enforced at
       decode by `SegmentedMap::new`, `metadata.rs:868-914`): the segment whose half-open span
       `[byte_offset, byte_offset + byte_len)` contains the offset, **plus** the immediately
       preceding segment when the offset equals a segment's `byte_offset`, **plus** the last
       segment when the offset equals the object's total length (a trailing zero-length chunk has
       **no** covering segment — the case a naive lookup returns `None` for and silently drops).
       **At most two records are read**, so the bounded-memory constraint below is untouched: no
       `seg:` range walk, no third decode.
    2. **Selection is by `ChunkRef` equality at the accumulated offset within a candidate**, not
       by offset alone. Exactly one match → repoint there.
    3. **A candidate whose freshly-read `SegmentRecord` disagrees with the root's `SegmentRef`
       for that index** — `byte_offset` or `byte_len` mismatch — is **not** a repoint target:
       treat it as a conflict and write nothing (the root-table extent invariant the resolver
       itself enforces, `metadata.rs:2582-2589`; carried forward from iteration 1's T5 finding).
    4. **Fail closed on anything else.** Zero matches, or a match in more than one candidate
       (two byte-identical zero-length refs straddling a boundary), → write **nothing** and
       classify it as a **refusal**, not a conflict, so it flows into the pass's existing
       hole and the answer is `Blocked` — never a silent `Satisfied`-with-the-obligation-dropped.
       An unaddressable chunk is a fact to report, not one to certify over. (Refusals are
       already holes and conflicts already are not, `reconstruction.rs:341-357`; this adds a
       member to the *refusal* class and nothing to the conflict class — see *Certification
       semantics*.)
    Leg 6 observes this end-to-end; the in-crate unit tests pin the helper on all three
    zero-length placements (segment start, segment end, object end) without needing a
    reconstruct of a zero-length chunk. Latent today — no in-tree producer mints such a chunk —
    but it is this issue's own C-1 defect shape re-entering through another door, so it is pinned
    rather than deferred.
  - **CERTIFICATION SEMANTICS — decided at Plan: parity with the flat arm, and no more.** On the
    target, a repair conflict emits a counter and nothing else (`reconstruction.rs:311-318`), and
    the pass's `hole` covers only an incomplete reading, a segmented refusal and a ceiling refusal
    (`:341-357`) — so a conflict-only pass answers `Satisfied` today, on the **flat** arm, and has
    for as long as the loop has existed. This child **keeps that answer** for the segmented arm
    (leg 3, leg 4) and adds **nothing to the conflict class**; the only things it routes into the
    hole are *refusals*, which are already there — the ceiling refusal (leg 5) and the
    unaddressable-chunk refusal of *Boundary addressing* rule 4. Rationale: a conflict is a *retry*, not a dead end — nothing repair-owned was
    written, the obligation is still queued, and the next pass re-plans onto the winner's bytes,
    so C-1's "an actor that exits it in bounded time" is satisfied. **Do NOT widen `hole` to
    include conflicts**: it would change the flat arm's long-standing behaviour and its existing
    tests, which is a different slice with a different blast radius. **SIGN-OFF DECISION POINT** —
    if the human judges that a conflict-only pass must answer `Blocked`, that is a separate issue
    against the shared loop (flat arm included), not a Do-level tightening here; flip it at
    sign-off and file it, do not smuggle it in.
  - `crates/custodian/src/reconstruction.rs` — the repair pass stops refusing a `seg:`-resident
    chunk (#697's placeholder at `:552` / `:609`) and completes the move. The placement change,
    the discharge of the repair obligation (`repair::repair_key` delete) and the orphan evidence
    for each displaced position stay **one batch** — do not split the batch to fit the new
    primitive; if the primitive's shape makes that awkward, change the primitive.
  - `crates/custodian/tests/segmented_map_reconstruction.rs` — **that file's own** second leg
    (`an_obligation_inside_a_segmented_object_is_refused_never_discarded`, `:489` at `a801997`
    — the brief's earlier `:484` was stale; cite the symbol, not the number — not to be
    confused with the success criterion's leg 2 above) asserts the
    refusal this child removes and MUST be rewritten to assert the repair now lands. This is a
    **forced** edit, budgeted for, not drift.
  - **Constraints carried forward (blockers from #651 / #638 — these bound the shape, they do
    not name it):** duplicate chunk ids get one plan, not independent ones — keep it to the
    narrow rule, do **not** rebuild the cross-object claim-counting apparatus dropped at #651's
    replan. **Bounded memory:** pin and rewrite the bytes of **exactly one** record; find the
    candidate segment(s) in the root's own table (the tiling is contiguous and checked at decode,
    `SegmentedMap::new`, `metadata.rs:868-914`), so no `seg:` range is walked — **at most two**
    records are read (the covering one and, at a boundary offset, its neighbour, per *Boundary
    addressing* above; that bound is the whole allowance and a third read means the shape is
    wrong), and exactly one is decoded into a rewrite;
    do not retain the namespace's decoded chunks and do not deep-copy a segmented
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] The brief's universal defect/invariant claim is contradicted by its new `V/2` refusal: it says every `seg:`-resident repair must gain an exit (`brief.md:2-12`, `brief.md:121-129`) but requires rows crossing 50,000 bytes to remain byte-identical and queued (`brief.md:59-80`). The tracker instead specifies a row crossing `MAX_VALUE_BYTES`, and the target resolver accepts every `seg:` value through 100,000 bytes (`crates/core/src/metadata.rs:2493-2500`). Thus a readable 50,001..100,000-byte row remains refused forever under the brief. Decide whether the issue fixes all source-readable rows (the tracker criterion) or also introduces a stricter segmented-maintenance policy, and narrow the defect/invariant claim if the latter is intentional.
- [x] Leg 3 promises that a lost same-chunk CAS is non-certifying (`brief.md:35-58`), but this is an unresolved hidden behavior change, not merely a repoint assertion. The target's common reconstruction loop only emits on `RepairOutcome::Conflict` and does not include conflicts in its `hole`, so a pass with only a conflict answers `Satisfied` (`crates/custodian/src/reconstruction.rs:311-318`, `crates/custodian/src/reconstruction.rs:341-357`). The prior sign-off explicitly warned that tightening this changes the flat arm's long-standing behavior (`brief.md:380-384`), yet the scope never decides between changing all repair conflicts and introducing segmented-only certification semantics. State and test that decision before Do.
- [x] The brief still does not pin the zero-length boundary case that a prior attempt got wrong. It directs the primitive to "find the covering segment" from the root table (`brief.md:185-191`), while the carry-forward reports that offset-first selection chose the wrong record for a zero-length chunk at a segment boundary and then silently certified (`brief.md:373-384`). That state is representable on the target: `ChunkRef::len` has no nonzero invariant (`crates/core/src/metadata.rs:128-139`), and `SegmentRecord` rejects only an empty list or a zero *total* span (`crates/core/src/metadata.rs:1135-1155`). Add an explicit addressing rule and observable reconstruction leg for this case; unnamed "segment coverage" helper tests (`brief.md:83-87`) do not establish that the permanent-queue regression is closed.
- [x] The repo/branch itself resolves, and local history substantiates the merged #695/#696/#697/#710 prerequisites, but the coordination claims still do not resolve from the permitted bundle: `brief.md:134-146` declares conflict #717 and downstream stack #722, and `brief.md:235-240` assigns the permanent fragment leak to #723, while `notes.json` contains only issue #721 with `comments: []` and neither `sources/` nor `dependency-state.json` is present. Supply tracker/dependency evidence for #717/#722/#723 or mark those claims unverified; otherwise the wave exclusion and promised follow-up owner cannot be checked before Do.
- [x] The deterministic race recipe for success legs 2–3 is false on the target. `brief.md:62-73` says resolution performs the first `get` of the `seg:` key and the move performs the second, but the resolver reads segment records with `MetadataStore::scan_page`, not `get` (`crates/core/src/metadata.rs:2414-2425`, `crates/core/src/metadata.rs:2455-2461`). The proposed move would introduce the only `get(seg_key)`, so applying the competing write “after the first return” is after the move has captured its CAS bytes: the sibling edit conflicts instead of being merged. Revise the fixture trigger (for example, hook the resolver's segment-page read) before claiming legs 2 and 3 exercise the intended window.
- [x] The criterion and scope contradict each other about losing CAS writes. Legs 3–4 require “nothing at all is written” (`brief.md:35-49`), while the scope explicitly excludes destination pre-marking and accepts a pre-written destination fragment after a lost CAS (`brief.md:177-187`). The production ordering writes rebuilt fragments before committing (`crates/custodian/src/reconstruction.rs:931-949`), and GC does **not** have the claimed ordinary unreferenced sweep: without an orphan mark or expired lease it conservatively retains the fragment (`crates/custodian/src/gc.rs:196-210`). The target design therefore requires a destination pre-mark and leaves it standing on CAS loss (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:354`, `:2577`). Either add that prerequisite/scope or narrow the criterion and the claimed C-1 restoration; the current plan promises both no write/stranding and the mechanism that causes it.
- [x] Leg 5 pins the wrong segmented-record ceiling. `brief.md:50-54` tests a `seg:` value just below `MAX_VALUE_BYTES`, and `brief.md:122-124` mandates the flat-record helper, but the target says a segmented placement write must be weighed against `MAX_ROOT_VALUE_BYTES` (`crates/core/src/metadata.rs:371-375`); proposal 0016 gives `MAX_SEG_CHUNKS` the same `V/2` headroom rule (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1462-1467`). As written, the green test requires admitting segment rewrites between 50,001 and 100,000 bytes, contrary to the target's headroom invariant.
- [x] The tracker and dependency claims cannot be checked from the supplied advisory inputs: `notes.json`, `sources/`, and `dependency-state.json` are absent, while `brief.md:97-112` relies on external state for `main`, conflict #717, merged prerequisites #695/#696/#697/#710, and downstream #722. The target does confirm `HEAD == origin/main == 92e1b4b`, but it cannot establish those issue states or whether a load-bearing tracker comment was omitted; the planner must supply the tracker/dependency evidence or remove claims that depend on it.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Plan re-ran on an already-split parent (brief.md + plan-advisory rewritten 2026-09-30, producing 8 stale §6 items); likely eduralph/pdca-harness#481 — check it covers this case.
