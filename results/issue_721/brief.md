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
- **Falsifiability:** legs 1, 2 and 6 go RED on the ordinary base — `origin/main` at `a801997`
  (re-verified 2026-08-19; the brief was first authored on `92e1b4b`, and the only commit since
  that touched either production file is `9470de5`, the #638 fragment-write deadline — every
  `path:line` in this brief was re-checked against `a801997` on that date, and the two that had
  moved are corrected below),
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
  `:363-373`). Legs 3 and 4 are coarse-red (base `Blocked` vs post-fix `Satisfied`) and leg 5 is
  green on the base; the **pins** those three legs exist for are falsifiable only against the
  **mutation** oracle, which is why each carries a required named negation rather than a red
  claim.
- **Invariant to restore:** **C-1 — a permanent or data-losing failure mode is never an
  acceptable cost: every durable byte is, at every instant, protected by a record that names
  it *or* evidenced for reclamation, and every state has an actor that exits it in bounded
  time** (`docs/principles.md:137`, §6 row *Storage lifecycle / reclamation*, sourced to §5
  C-1 at `:109`; the maintainer's standing rule of 2026-07-25; `0016:2802-2813`;
  `crates/custodian/src/gc.rs:22-25`). A refused-forever repair obligation is a state with
  **no** actor that exits it. The invariant is restored only when the maintenance write path
  for a `seg:`-resident chunk **exists and the repair pass completes through it** — not when
  the refusal is made quieter, better-counted or better-explained.
  **Precisely what "exits it" means here — the claim is scoped, deliberately.** Every
  `seg:`-resident repair obligation over a record the resolver will read (i.e. `≤
  MAX_VALUE_BYTES`, `metadata.rs:2493`) must exit on **exactly the flat arm's terms**, and this
  child introduces no state class the flat arm does not already have:
  - **repaired** — the placement moves, the obligation is discharged in the same batch (legs 1,
    2, 6);
  - **conflict** — a lost CAS: nothing repair-owned is written, the obligation stays queued, and
    the *next pass re-plans onto the winner's bytes*. That is an actor, in bounded time, so a
    conflict is a live retry and not a no-exit state — which is why leg 3 asserts the base's
    `Satisfied`, not `Blocked`;
  - **ceiling refusal** — the re-encoded record would not survive the backend value ceiling: it
    is classified, writes nothing, keeps the obligation, and the pass reports **`Blocked`**, so
    the condition is never disguised as certified redundancy (leg 5). This is #710's existing
    class, extended to a second record shape at the *same* bound (V) — see leg 5 for why V/2
    would instead mint a *new* no-exit class over readable rows and thereby violate this very
    invariant.
  The one thing this invariant does **not** cover, stated so the claim is not read as universal:
  a row already stored **above** `MAX_VALUE_BYTES` by a non-conforming writer is refused by the
  *read* path itself (`metadata.rs:2493`) and is therefore not repairable by anything this child
  could write — it is a containment case that predates this child, owned by the resolver's
  fail-closed rule, not a state this child creates. Guarding, annotating or
  re-classifying `metadata.rs` alone satisfies nothing here.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single slice; M4's
  integration branch is merged and deleted, and every #635 slice to date landed on `main`
  directly. Base at authoring: `92e1b4b`; **re-verified 2026-08-19 at `a801997`** (one commit
  since, `9470de5`, touched the two production files — citations re-checked, see Falsifiability).)
- **Conflicts with:** 717
- **Ordering note:** **Wave 0 — no in-batch prerequisite.** Every external prerequisite is
  already **merged** into `origin/main`: #710 (the ceiling helper `flat_value_ceiling_crossed`,
  `metadata.rs:380`) as PR #718, and #695/#696/#697 as PRs #704/#705/#706 — verified with
  `git -C ../wyrd log --oneline origin/main` and `gh issue view`, all four CLOSED. So no
  `Depends on (merged):` is required; do not add one. **Never share a wave with #717** — it
  inserts `owner`/`staged` into `PendingEntry` (`metadata.rs:1556` at `a801997`; the brief's
  earlier `:1528` was stale), shifting every citation
  below that point in this child's largest file. (#717 is the terminal child of #692's
  2026-08-09 split, #715 → #716 → #717; #715 and #716 touch only `crates/core/src/multipart.rs`
  and their own new test files and share nothing with this child.) **Cite by symbol, not by
  number** where a citation sits below `PendingEntry` in `metadata.rs`. #722 stacks on this child
  and must not start until this child's PR is **merged** — with `auto_merge = false` the driver
  stops at the wave boundary and the human merges (INTEGRATION §2).
  **Tracker evidence for the three coordination claims, so a bundle-only reader can check them
  (re-verified 2026-08-19 with `gh issue view <n> --repo getwyrd/wyrd`; the bundle's `notes.json`
  carries only #721, which is why the evidence is transcribed here):**
  - **#717** — `multipart-staging-retire-pending`, **OPEN**, `closedAt: null`. Its bundle is live
    at `results/issue_717/`. **Superseded by the children:** #776's brief drops this conflict as
    stale (#717's bundle is `COMPLETE [close: no PR]` — itself split, ships no code) and names
    **#772** as the real `metadata.rs` collision. Take the children's, not this line's.
  - **#722** — `segmented-drain-evacuation-completes`, **OPEN**, milestone *0.1 Alpha*. It owns
    the drain/evacuation caller and the DST repoint-versus-supersede property this child excludes.
  - **#723** — `reconstruction/rebalance strand an unreclaimable fragment when the placement CAS
    loses`, **OPEN**, milestone *Foundations*. It is the named owner of the pre-existing fragment
    leak in Out of scope, and of the two inaccurate "collectable garbage" comments.
  All three are OPEN as of 2026-08-19; nothing here depends on any of them being closed.
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
  C4-verify measures — legs 1, 2 and 6 are red pre-fix and green post-fix. Declared here only to
  pre-empt a §6 surprise: **leg 5 passes on the base too** (pre-fix the pass refuses the
  segmented obligation and answers `Blocked`, which is also the post-fix ceiling answer), and
  **legs 3 and 4 are only coarse-red** — they flip because the base answers `Blocked` where the
  fixed pass answers `Satisfied`, which says nothing about whether the CAS pins are right. The
  gate's summary line reports how many tests
  *ran* in the red leg, **not** how many failed — the parent's `check-gates.json` said "4
  test(s) ran red" when only 3 legs were actually discriminating, and the adversary had to
  correct it by hand. So: expect the red leg to report **6 tests ran with 5 failing**, and read
  the count as a count. The pins behind legs 3–5 are bound by the **mutation** oracle instead,
  which is why
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
  the boundary a write must not cross, and the bound leg 5 now weighs the segmented arm against)
  and `:2582-2589` (the root-table extent invariant a
  placement-only rewrite must preserve); `crates/core/src/metadata.rs:327` / `:352` / `:371-375`
  (`MAX_VALUE_BYTES`, `MAX_ROOT_VALUE_BYTES`, and the doc that assigns the half to a segmented
  **root's** write — the reason this child weighs V, not V/2);
  `crates/core/src/metadata.rs:868-914` (`SegmentedMap::new`, the contiguous tiling the candidate
  lookup reads, plus its per-entry `byte_len == 0` rejection at `:902-907`) and `:128-139` /
  `:1146-1162` (`ChunkRef::len` with no nonzero invariant, and
  `SegmentRecord`'s only structural rejections — why the zero-length boundary case of leg 6 is
  representable); `crates/custodian/src/reconstruction.rs:311-318` and `:341-357` (`emit_conflict`
  and the pass's `hole` — the certification semantics legs 3–5 assert, unchanged by this child);
  `crates/core/src/metadata.rs:2776-2780` (#710's in-crate
  boundary test — the convention the new unit tests follow);
  `crates/custodian/tests/segmented_map_restore.rs:387-431` (`seed_segmented` / `seed_damaged`:
  raw `seg:` + root seeding with a fixture self-check). **Salvage:** the archived attempt at
  `/home/eddie/wyrd/wyrd-pdca/results/issue_711/iteration-v1/patch.diff` contains a working
  primitive and a working reconstruction caller that passed C4-ci and C4-verify — reuse them,
  but (a) correct every doc site claiming the move pins "the exact bytes the resolve read", (b)
  add legs 2–6 and the in-crate unit tests, (c) replace its offset-only record lookup with the
  *Boundary addressing* rule in Scope — the salvaged primitive has the zero-length boundary
  defect and must not be reused unchanged, and (d) drop everything in the rebalance and DST
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

## Plan-review response (revision pass, 2026-08-19 — `plan-advisory-plan-reviewer.md`)

**Read this first — where these revisions land.** This bundle closed with
`close-disposition = split`: #721 is now the **parent**, and the operative briefs are its
children **#776** (the `wyrd_core` primitive) and **#777** (the reconstruction caller),
`split-lineage.json`. Do builds *those*, not this file. So the revisions below are the parent's
plan of record, and **two of them change what the children currently say** — they are not
inherited automatically and are called out as such in items 1 and 2. Nothing here edits a child
bundle; that is the human's call at sign-off.

All four findings were accepted; the brief was revised in place. Nothing stands unchanged.

1. **Universal invariant vs. the V/2 refusal — REVISED, and the ceiling decision is REVERSED.**
   The reviewer is right that a V/2 write bound would leave every readable `seg:` row in
   `50_001..100_000` refused forever, which is this issue's own defect re-created. Leg 5 and Scope
   now weigh **both** arms against the full `MAX_VALUE_BYTES` through #710's helper. Three
   grounds: the resolver reads through V (`metadata.rs:2493`); the in-tree doc assigns V/2 to a
   segmented **root's** write and this child never re-encodes the root (`metadata.rs:371-375`);
   and the tracker body specifies V explicitly. The Invariant field is also now scoped — it
   enumerates the three exit states (repaired / conflict-retry / classified ceiling refusal
   reported as `Blocked`) and names the one case it does **not** cover (a row already stored above
   V, which the read path itself refuses). **This reverses the iteration-4 sign-off directive; it
   is flagged in leg 5 as the first thing to look at at sign-off.**
   **CARRY-OVER REQUIRED — this contradicts child #776.** `results/issue_776/brief.md:37-55`
   settles the segmented arm at **V/2** and disposes of the `metadata.rs:371-375` doc comment as
   "about the root record", warranting V/2 from the resolver's comment at `metadata.rs:2488` and
   0016's knob table. Neither warrant answers the C-1 consequence the reviewer raised, and on a
   fresh read the resolver comment argues the other way: it is attached to the `>
   MAX_VALUE_BYTES` test and says a row *above V* is one "no conforming publication wrote", with
   the V/2 parenthetical explaining **why** — it does not make V/2 an enforced boundary anywhere.
   The two bounds differ only for rows a non-conforming writer already stored (a placement-only
   repoint grows a conforming ≤V/2 record by bytes, never to V), and for exactly those rows V/2
   means "refused every pass, forever" while V means "repaired". Plan's recommendation is
   therefore V, and **#776's brief must be flipped to it (and #777's leg 5, which names
   `MAX_ROOT_VALUE_BYTES` in its symbol list at `:15-17`) before those children build** — or the
   V/2 warrant must answer the permanent-refusal consequence in writing. **Human's call; it does
   not happen by itself.**
2. **Leg 3's "does not certify" — REVISED to the decided semantics, and now tested.** The
   reviewer's reading of the target is correct (`reconstruction.rs:311-318`, `:341-357`: conflicts
   are not in `hole`, so a conflict-only pass answers `Satisfied`). Decision recorded in Scope
   (*Certification semantics*): **parity with the flat arm**, on the C-1 ground that a conflict is
   a bounded retry rather than a no-exit state; legs 3 and 4 now MUST assert
   `Reconciled::Satisfied`, so the decision is pinned by a test instead of left implicit. Widening
   `hole` to cover conflicts is explicitly out of scope and marked a sign-off decision point —
   it would change the flat arm and belongs to its own issue.
   **Carry-over, optional but recommended:** child #777 currently leaves this open ("whether the
   pass may still certify `Satisfied` here is a separate scope decision left to the human … this
   leg asserts the store, not the verdict", `results/issue_777/brief.md:36-37`). That posture is
   *safe* — it asserts nothing false — but it leaves the verdict unpinned, so a later change to
   `hole` would pass its tests silently. Adding the `Satisfied` assertion there converts the open
   question into a tested decision.
3. **Zero-length chunk at a segment boundary — REVISED; the rule is now stated, not left to Do.**
   Scope gains *Boundary addressing*: candidates from the root table (covering segment, plus the
   predecessor at a boundary offset, plus the last segment at offset == object length — **at most
   two records read**, so bounded memory holds), selection by `ChunkRef` equality, a root-extent
   agreement check, and fail-closed `Blocked` on zero or ambiguous matches — never a silent
   `Satisfied`. New **leg 6** observes it end-to-end and the in-crate unit tests pin all three
   zero-length placements. The salvage instruction now says the archived primitive carries this
   defect and must not be reused unchanged. **Already inherited by the children** — #776's brief
   carries it as item 7 (`results/issue_776/brief.md:59-62`); what is new here is the *rule*
   (candidate set, at-most-two reads, root-extent agreement, fail-closed), which #776 states only
   as "equality governs". Worth folding the candidate-set clause into #776.
4. **Unverifiable coordination claims — REVISED by transcribing the evidence into the brief.**
   #717 (OPEN), #722 (OPEN, *0.1 Alpha*), #723 (OPEN, *Foundations*) re-verified 2026-08-19 with
   `gh issue view … --repo getwyrd/wyrd`; titles, states and milestones are now quoted in the
   Ordering note so a bundle-only reader can check them without the tracker. While re-verifying,
   the base was refreshed to `a801997` (only `9470de5` has touched the two production files since
   `92e1b4b`) and four stale citations were corrected: `PendingEntry` `:1528`→`:1556`; the refusal
   test `an_obligation_inside_a_segmented_object_is_refused_never_discarded` `:484`→`:489`;
   `SegmentRecord::checked` `:1135-1155`→`:1146-1162`; `SegmentedMap::new` →`:868-914`. Every
   other `path:line` in the brief was re-checked at `a801997` and holds.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must add in-crate assertions that a flat repoint changes `chunk_map` and increments `version`—four killable mutants remove those semantics while the core mutation run stays green, so the brief's mutation evidence remains incomplete (`crates/core/src/metadata.rs:2859`).; T5 Judgment — Rebuild must compare the fresh `SegmentRecord` offset and length with the selected `SegmentRef` and race-test the mismatch—otherwise maintenance can CAS a root-inconsistent row that normal resolution rejects (`crates/core/src/metadata.rs:2900`, `crates/core/src/metadata.rs:2607`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 37 mutants tested in 84s: 4 missed, 19 caught, 14 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T5 Judgment — Rebuild must use checked version advancement and surface malformed segment decoding as structural corruption—otherwise maintenance can panic/wrap or hide a persistent damaged record (`crates/core/src/metadata.rs:2844`, `crates/core/src/metadata.rs:2883`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild targeting the two substantive implementation defects found by the adversarial review, not the whole §6 list: 1. Repair-loop containment (crates/custodian/src/reconstruction.rs:322) counts/reports a refusal once per *obligation* instead of once per *object* — restore the once-per-object dedupe the deleted `Reading::refused` set guaranteed. A segmented object with two queued chunks in the same seg: record must produce one refusal tick, not two. 2. An unattributable committed object with a segmented map is now silently skipped (crates/custodian/src/reconstruction.rs:513-515, `let Some(inode_id) = parse_inode_key(&key) else { continue }`), which does not set `reading.incomplete` and lets the obligation be deleted for a chunk a committed map still references. Fix by routing through `reading.contain(&key, ...)` instead of the bare `continue`, per the fail-closed "never silent skip" rubric class. Explicitly out of scope for this iteration: the two broken rustdoc intra-doc links (cosmetic, ungated) and the C4 `cargo deny` / RUSTSEC-2026-0258 `h2` advisory (unrelated supply-chain finding, not caused by this patch) — do not spend the iteration on either.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild targeting the two substantive implementation defects found by the adversarial review, not the whole §6 list:
  1. Repair-loop containment (crates/custodian/src/reconstruction.rs:322) counts/reports a refusal once per *obligation* instead of once per *object* — restore the once-per-object dedupe the deleted `Reading::refused` set guaranteed. A segmented object with two queued chunks in the same seg: record must produce one refusal tick, not two.
  2. An unattributable committed object with a segmented map is now silently skipped (crates/custodian/src/reconstruction.rs:513-515, `let Some(inode_id) = parse_inode_key(&key) else { continue }`), which does not set `reading.incomplete` and lets the obligation be deleted for a chunk a committed map still references. Fix by routing through `reading.contain(&key, ...)` instead of the bare `continue`, per the fail-closed "never silent skip" rubric class.
  Explicitly out of scope for this iteration: the two broken rustdoc intra-doc links (cosmetic, ungated) and the C4 `cargo deny` / RUSTSEC-2026-0258 `h2` advisory (unrelated supply-chain finding, not caused by this patch) — do not spend the iteration on either.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rationale: the T4 rubric review failed (4 blocking), and several §6 NEEDS-HUMAN items are defects in the brief itself, not just gaps Do can close: - The brief's race-window recipe ("resolution performs the first get of the seg: key") does not match the target: the resolver reads segment records via MetadataStore::scan_page, not get, so the described trigger cannot reach the intended read->prepare window as written. - The brief's own success criterion (legs 3-4: "nothing at all is written" on a lost CAS) contradicts its own scope section, which explicitly excludes the destination pre-mark and accepts a pre-written, stranded destination fragment after a lost CAS. - Leg 5 pins the wrong ceiling constant for segmented records (tests against MAX_VALUE_BYTES / the flat-record helper per brief.md:50-54,122-124, while the target's own headroom rule requires weighing a seg: record against MAX_ROOT_VALUE_BYTES / V/2). - Tracker/dependency claims (merged prerequisites #695/#696/#697/#710, conflict #717, downstream #722) are unverifiable from the supplied bundle — supporting evidence files are absent. - Size backstop: patch is 105KB (>100KB threshold), 3 rounds already spent (>2 threshold) — matches the bundle's own iterate-plan recommendation. Also carry forward for the re-plan's consideration (implementation-shaped but likely brief- scoping issues too): - [impl] a zero-length chunk sitting at a segment boundary is matched to the wrong seg: record by offset before the ChunkRef equality check, making it permanently unrepairable and silently certified Satisfied rather than honestly Blocked (crates/core/src/metadata.rs:2869- 2874, 2922). Latent (no in-tree producer mints such a chunk yet) but reintroduces the brief's own C-1 defect shape through a different door. - [human] leg 3's shipped test does not assert the brief's stated success criterion ("the pass does not certify") — it asserts something weaker, and the true behavior is the opposite: the pass certifies Satisfied while the chunk stays under-replicated forever. Tightening the assertion changes the flat arm's long-standing behavior too, so this is a scope call for the re-plan, not a Do-level test fix. Human directive: iterate-plan for 721 — return to Plan for `pdca split`.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rationale: the T4 rubric review failed (4 blocking), and several §6 NEEDS-HUMAN items are
  defects in the brief itself, not just gaps Do can close:
  - The brief's race-window recipe ("resolution performs the first get of the seg: key") does
    not match the target: the resolver reads segment records via MetadataStore::scan_page, not
    get, so the described trigger cannot reach the intended read->prepare window as written.
  - The brief's own success criterion (legs 3-4: "nothing at all is written" on a lost CAS)
    contradicts its own scope section, which explicitly excludes the destination pre-mark and
    accepts a pre-written, stranded destination fragment after a lost CAS.
  - Leg 5 pins the wrong ceiling constant for segmented records (tests against
    MAX_VALUE_BYTES / the flat-record helper per brief.md:50-54,122-124, while the target's own
    headroom rule requires weighing a seg: record against MAX_ROOT_VALUE_BYTES / V/2).
  - Tracker/dependency claims (merged prerequisites #695/#696/#697/#710, conflict #717,
    downstream #722) are unverifiable from the supplied bundle — supporting evidence files are
    absent.
  - Size backstop: patch is 105KB (>100KB threshold), 3 rounds already spent (>2 threshold) —
    matches the bundle's own iterate-plan recommendation.

  Also carry forward for the re-plan's consideration (implementation-shaped but likely brief-
  scoping issues too):
  - [impl] a zero-length chunk sitting at a segment boundary is matched to the wrong seg:
    record by offset before the ChunkRef equality check, making it permanently unrepairable and
    silently certified Satisfied rather than honestly Blocked (crates/core/src/metadata.rs:2869-
    2874, 2922). Latent (no in-tree producer mints such a chunk yet) but reintroduces the
    brief's own C-1 defect shape through a different door.
  - [human] leg 3's shipped test does not assert the brief's stated success criterion ("the
    pass does not certify") — it asserts something weaker, and the true behavior is the
    opposite: the pass certifies Satisfied while the chunk stays under-replicated forever.
    Tightening the assertion changes the flat arm's long-standing behavior too, so this is a
    scope call for the re-plan, not a Do-level test fix.

  Human directive: iterate-plan for 721 — return to Plan for `pdca split`.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
- **2026-08-19, plan-review revision pass:** the iteration-4 item "leg 5 pins the wrong ceiling
  constant … requires `MAX_ROOT_VALUE_BYTES` / V/2" is **SUPERSEDED** — see leg 5 and
  *Plan-review response* item 1 above (the bound is the full `MAX_VALUE_BYTES`). The record of
  the directive is left intact; only its verdict changed. The other two carried-forward items are
  now closed in the brief: the zero-length boundary chunk by the *Boundary addressing* rule and
  leg 6, and the "does not certify" contradiction by *Certification semantics* and leg 3.
- This bundle then closed as `split` (children **#776**, **#777**, `split-lineage.json`); those
  children, not this brief, are what Do builds.
