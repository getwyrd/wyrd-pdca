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
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; M4's
  integration branch is merged and deleted, and every #635 slice to date landed on `main`
  directly. Base at authoring: `92e1b4b`.)
- Scope: **the missing maintenance write path for a `seg:`-resident chunk, and
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny check` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.4% — 376 of 390 instrumentable changed lines executed (floor 80%); 390 of 780 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 41 mutants tested in 79s: 25 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.07s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #721: enable reconstruction to repair a `ChunkRef` stored in a `seg:` record while preserving atomicity, concurrency semantics, and record-size limits.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief settles the refused-forever defect, five observable legs, record-addressing and CAS semantics, the V/2 segment ceiling, scope caps, and explicit exclusions, so the required behavior is decidable. |
| C2 Reproduction (red pre-fix) | PASS | An independent tracked-change stash retained the base-visible discriminator: 7 tests ran, with only the binding repair and sibling-merge legs failing at `crates/custodian/tests/segmented_map_repoint.rs:429` and `crates/custodian/tests/segmented_map_repoint.rs:493`; restoring the patch made all 7 green. |
| C3 Change | PASS | The covering-record CAS primitive and its reconstruction integration directly supply the missing writer while keeping placement, obligation deletion, and displaced-fragment evidence in one batch (`crates/core/src/metadata.rs:2822`, `crates/custodian/src/reconstruction.rs:889`, `crates/custodian/src/reconstruction.rs:912`). |
| C4 Verification (red→green) | FAIL | Red→green, formatting, targeted reconstruction tests, workspace tests, coverage, mutation testing, and TiKV compilation are green, but the required aggregate remains red because `cargo deny` reports unchanged `h2` 0.4.15 as RUSTSEC-2026-0258 (`Cargo.lock:111`; `gate-logs/C4-ci.log:5302`). |
| C5 Causal adequacy | PASS | The change removes the absent segmented write path rather than adding a capability probe or symptom guard, and direct race/ceiling assertions plus 41 mutation trials leave no surviving viable mutant (`crates/core/src/metadata.rs:2873`, `crates/custodian/tests/segmented_map_repoint.rs:510`; `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | Core owns the narrow metadata-move primitive and custodian only composes its returned batch with repair evidence, preserving the trait seam and dependency direction (`crates/core/src/metadata.rs:2754`, `crates/custodian/src/reconstruction.rs:889`). |
| T2 Shape | PASS | The patch touches exactly the four permitted files, adds 183 semantic non-test lines, and is 97,231 bytes—49 bytes below the stated 95 KiB cap—with no out-of-scope path. |
| T3 Runtime | PASS | The segmented arm selects one root-table entry, reads one `seg:` row, CASes root plus row, and the caller atomically commits the repoint, queue deletion, and orphan evidence (`crates/core/src/metadata.rs:2873`, `crates/core/src/metadata.rs:2918`, `crates/custodian/src/reconstruction.rs:912`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and publish reruns their audit (`gate-logs/T4-contribution.log:10`); the frozen review's pre-mark and DST findings are settled out of this slice by #723 and #722 respectively, so the standing rubric forbids re-raising them. |
| T5 Judgment | NEEDS-HUMAN | Confirm the brief's affected-path prior-art survey across merged and closed/rejected work—the self-contained target exposes only its synthetic base commit, so that history could not be mechanically re-derived, and duplicate or conflicting work would change the scope judgment. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether activating maintenance writes for formerly refused multipart repairs meets the intended production repair policy—the change affects live durability state, so code-level red→green evidence cannot settle operational fitness. |

### Advisory — adversary

# Adversarial review — issue #721 (segmented-repair-completes-through-repoint)

Advisory only; I never gate. Inputs: `patch.diff`, `brief.md`, `check-gates.json`, `gate-logs/`.
Every citation is grounded on the target source at `$PDCA_TARGET` (working tree = patch applied on
`cbd8b19 pre-fix base a801997`). The red→green claim was adjudicated from the frozen
`gate-logs/C4-verify.log` (#403), not re-run: it shows both discriminating legs failing against
reverted production with real assertion diffs (`Blocked` vs `Changed`) at
`crates/custodian/tests/segmented_map_repoint.rs:429` and `:493`, and 7/7 green with the fix. I
could not make that evidence tautological — the legs name no symbol the patch introduces, drive the
real `reconcile_step`, and read the store back through `metadata::decode`.

## Findings

- **NEEDS-HUMAN [human] — the segmented arm's `V/2` ceiling re-creates the exact refused-forever
  state this issue exists to remove, for a record band the read side explicitly admits.**
  `segment_value_ceiling_crossed` refuses any re-encoded `seg:` record above `MAX_ROOT_VALUE_BYTES`
  (`crates/core/src/metadata.rs:397`), while the resolver keeps and answers from stored `seg:` rows
  all the way to `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:2528`). Concrete failing case: a
  `seg:` record of 60 000 bytes — readable, resolvable, no anomaly — holding an under-replicated
  chunk. `repoint_chunk` answers `Repoint::Refused` on every pass, `reconcile` sets
  `ceiling_refused` and answers `Blocked` (`crates/custodian/src/reconstruction.rs:334`, `:362`),
  the obligation is never drained, and nothing in the tree shrinks the record. That is a state with
  no actor exiting it in bounded time — the C-1 shape named in the brief's *Invariant to restore*,
  relocated from "all segmented objects" to "segmented objects with a 50–100 KB segment". The
  brief's premise for choosing `V/2` — "a conforming publication never writes a `seg:` value above
  `V/2`" — is **not enforced anywhere in this tree**: `MAX_SEG_CHUNKS` has no definition
  (`crates/core/tests/multipart_budget_admission.rs:337` says so in as many words),
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1465` assigns both the knob and its
  enforcement to **#508**, and `SegmentRecord::new` (`crates/core/src/metadata.rs:1170`) caps
  nothing. Mitigating, and why this is a sign-off question rather than a build defect: this build
  ships no producer of segmented maps, so the band is unreachable in production today, and the brief
  says "flip it at sign-off if you disagree". What the human should know before deciding is that the
  choice is now **test-locked** — leg 5 (`crates/custodian/tests/segmented_map_repoint.rs:612`) goes
  red if the bound is widened to `MAX_VALUE_BYTES` — and that #508 is the load-bearing prerequisite
  the choice silently depends on.

- **NEEDS-HUMAN [impl] — the repair loop's containment names and counts once per *obligation*,
  dropping the once-per-*object* guarantee the deleted `Reading::refused` set carried.**
  `crates/custodian/src/reconstruction.rs:322` calls `reading.contain(...)` inside `for plan in
  &plans`, and `contain` emits unconditionally (`:406-409` → `emit_unresolvable`, `:1059-1068`).
  Concrete failing case: a segmented object with **two** queued chunks in the **same** `seg:` record,
  torn under the move — `repoint_chunk` returns `SegmentRecordUndecodable` twice, so one damaged
  object produces **two** `reconstruction_unresolvable_records` ticks and two NEEDS-HUMAN audit rows,
  while the identical fault met in `read_committed` (`:504`) produces exactly one. The code this
  patch deleted made the property explicit ("counted and named exactly ONCE PER OBJECT: two
  obligations inside one segmented object are one refusal, not two"). The new test cannot catch it:
  the torn leg (`crates/custodian/tests/segmented_map_repoint.rs:692`) enqueues a single chunk. Fix
  is a per-object dedupe on the containment path, mirroring what was removed.

- **NEEDS-HUMAN [impl] — an unattributable committed object is now a silent *skip* that feeds the
  drain path, and this patch widened that from flat objects to segmented ones.**
  `crates/custodian/src/reconstruction.rs:513-515` replaced the base's shape-gated
  `(Some(_), None) => continue` with an unconditional `let Some(inode_id) = parse_inode_key(&key)
  else { continue };`. That `continue` does **not** set `reading.incomplete`, so the object's chunks
  never enter `reading.sites`, `assess` answers `Assessment::Drain` (`:608`), and — because the
  reading still looks complete — the obligation is **deleted** at `:351`, for a chunk a committed map
  does reference. Concrete failing case: an `inode:`-prefixed row that decodes as a `Committed`
  `InodeRecord` with a **segmented** map under a non-canonical key spelling; on the base its chunks
  became `Site::Refused` and the obligation was *kept*, after this patch they are silently drained.
  That is the rubric's "absent or unsupported entries → never silent skip" class, on a line this diff
  rewrote. #698 is cited in the surrounding comment as owning the *key-spelling* hazard ("read at one
  key and written at another"); it does not own this *drain* consequence, and the fail-closed fix is
  one call — `reading.contain(&key, ...)` instead of the bare `continue`.

- **NEEDS-HUMAN [impl] — two public doc comments link to a private item, so the rendered link is dead
  and rustdoc's `private_intra_doc_links` fires.** `crates/core/src/metadata.rs:375` (doc of the
  `pub fn flat_value_ceiling_crossed`, `:380`) and `crates/core/src/metadata.rs:2768` (doc of the
  public variant `Repoint::Refused`) both link `[segment_value_ceiling_crossed]`, declared private at
  `:397`. Nothing catches it — there is no `cargo doc` step in `cargo xtask ci` nor in
  `.github/workflows/` — so it ships as a broken link in the published API docs. Either widen the
  helper's visibility or de-link it in the two public docs.

- **NEEDS-HUMAN [human] — the gating `T4-batch-review` red is composed entirely of Plan-settled
  deferrals, so iterating Do cannot clear it.** `gate-logs/T4-batch-review.log` reports two blocking
  findings: the missing orphan pre-mark at `crates/custodian/src/reconstruction.rs:908` — the 0016
  X47 pre-mark, which the brief's *Out of scope* assigns to **getwyrd/wyrd#723** and marks "DECIDED
  AT PLAN … do not re-open" — and absent seeded Tier-0 DST coverage at `:897`, which the brief
  assigns to **#722** while forbidding any edit to `crates/dst/tests/custodian.rs`. Both name real
  rubric classes on a surface this diff touches, and both are answered by a tracked deferral, which
  the repo's reviewer protocol treats as settled. A human must decide whether to accept the red
  against those two references or re-scope; a rebuild will reproduce it unchanged.

- **NEEDS-HUMAN [human] — the gating `C4-ci` red is an unrelated supply-chain advisory, not this
  diff.** `gate-logs/C4-ci.log:2847` fails `cargo deny check` on RUSTSEC-2026-0258 (`h2 0.4.15`,
  pulled via `hyper`/`tonic`/`aws-smithy-http-client`, `Cargo.lock:111`). Everything else in the run
  — fmt, clippy, the whole test suite including `placement_ceiling.rs` (`:1222`, 5/5 green) and
  `segmented_map_reconstruction.rs`, machete, conformance — is green on both attempts, and the patch
  touches no manifest or lockfile. The remedy is `cargo update -p h2`, outside the brief's 4-file
  budget. Human call whether to bump here or hold.

## Refutations attempted and failed

Recorded so the next reviewer does not respend them.

- *"The merge/conflict legs never reach the read→prepare window."* They do. `MemMeta::scan_page`
  fires the racing batch **after** materialising the `seg:` page
  (`crates/custodian/tests/segmented_map_repoint.rs:114`), which is strictly between the resolver's
  only `scan_page` (`crates/core/src/metadata.rs:2495`, inside `read_group_range`) and the move's
  only `get` (`:2885`) — the shape the brief demanded, not the parent attempt's inside-`commit()`
  injection. Each racing leg asserts `meta.raced()` as a fixture self-check, so a leg whose race
  never landed fails rather than passing vacuously.
- *"`chunk_at`'s offset addressing can mis-target a neighbour."* It cannot: `read_committed`
  accumulates the offset over the resolved list from 0
  (`crates/custodian/src/reconstruction.rs:535-538`) and `SegmentedMap::new` enforces a contiguous
  tiling from 0 at decode (`crates/core/src/metadata.rs:930`), so the caller's absolute offset and
  the root table's `SegmentRef.byte_offset` cannot disagree. Zero-length chunks, an object naming one
  `ChunkId` twice, and `u64` overflow all fall out correctly (`chunk_at`, `:2938-2950`: `checked_add`
  → `None` → `Conflict`).
- *"The two-precondition batch is a new backend contract."* Multi-key preconditions already exist in
  production (`crates/core/src/metadata.rs:1669`, `:1757-1761`, `:1969`) and every backend iterates
  `batch.preconditions` (`crates/metadata-redb/src/lib.rs:214`, `crates/metadata-fdb/src/lib.rs:1485`,
  `crates/metadata-tikv/src/lib.rs:1386`), with the fault-conformance suite exercising a two-`require`
  batch (`crates/metadata-fault-conformance/src/lib.rs:224-225`).
- *"The resolve's restart path lets a repair write into a non-`Committed` generation."* `read_committed`
  gates on the scanned record (`crates/custodian/src/reconstruction.rs:494`) **and**
  `resolve_current_chunk_map` re-checks `state != Committed → Ok(None)` on every restart
  (`crates/core/src/metadata.rs:2726`), so `Object::prior` is always a committed root.
- *"The flat arm regressed when its ceiling check moved into the primitive."* #710's
  `placement_ceiling.rs` — including the exactly-on-the-ceiling admissible leg and the
  aborted-not-refused precedence leg — is green in `gate-logs/C4-ci.log:1222`. The `MISS` on the flat
  refusal lines in `gate-logs/C4-diff-cov.log:397-400` is an artefact of the diff-cov run's narrower
  test selection (it runs `segmented_map_repoint.rs` plus `wyrd-core`, not the whole custodian suite),
  not unreachable code.
- *"A `Repoint::Conflict` can loop forever while the pass certifies `Satisfied`."* Every non-race
  conflict source is caught on the *next* pass by the resolver instead — the extent mismatch by
  `read_segments` (`crates/core/src/metadata.rs:2617`), an absent or undecodable row by `retired_or`
  — which contains the object and forces `Blocked`. The one non-transient fault the move itself meets
  is raised as a typed error rather than folded into `Conflict`, which is the right call.
- *"`decode→encode` identity of a segmented root is assumed but never tested."* Leg 1 exercises it
  end to end: the repair commits only because `require(inode_key, encode(prior))` matches the
  fixture's stored root bytes, and the leg then asserts the root is byte-identical afterwards
  (`crates/custodian/tests/segmented_map_repoint.rs:458`).

## Reading of the gate rows

`check-gates.json`'s `C4-verify` line — "7 test(s) ran red" — counts tests *executed* in the red leg,
not failures: the log shows **2 of 7** failing, exactly the two the brief nominated as
discriminating. Legs 3–7 pass on the base by construction and are bound by the C5 mutation oracle
(41 mutants, 25 caught, 16 unviable, 0 missed), not by C4-verify. No overclaim — the brief
pre-declared it — but the row must be read as the brief instructs, not at face value.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm the brief's affected-path prior-art survey across merged and closed/rejected work—the self-contained target exposes only its synthetic base commit, so that history could not be mechanically re-derived, and duplicate or conflicting work would change the scope judgment.
- [ ] Validation — fitness-to-purpose — Decide whether activating maintenance writes for formerly refused multipart repairs meets the intended production repair policy—the change affects live durability state, so code-level red→green evidence cannot settle operational fitness.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- [ ] The deterministic race recipe for success legs 2–3 is false on the target. `brief.md:62-73` says resolution performs the first `get` of the `seg:` key and the move performs the second, but the resolver reads segment records with `MetadataStore::scan_page`, not `get` (`crates/core/src/metadata.rs:2414-2425`, `crates/core/src/metadata.rs:2455-2461`). The proposed move would introduce the only `get(seg_key)`, so applying the competing write “after the first return” is after the move has captured its CAS bytes: the sibling edit conflicts instead of being merged. Revise the fixture trigger (for example, hook the resolver's segment-page read) before claiming legs 2 and 3 exercise the intended window.
- [ ] The criterion and scope contradict each other about losing CAS writes. Legs 3–4 require “nothing at all is written” (`brief.md:35-49`), while the scope explicitly excludes destination pre-marking and accepts a pre-written destination fragment after a lost CAS (`brief.md:177-187`). The production ordering writes rebuilt fragments before committing (`crates/custodian/src/reconstruction.rs:931-949`), and GC does **not** have the claimed ordinary unreferenced sweep: without an orphan mark or expired lease it conservatively retains the fragment (`crates/custodian/src/gc.rs:196-210`). The target design therefore requires a destination pre-mark and leaves it standing on CAS loss (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:354`, `:2577`). Either add that prerequisite/scope or narrow the criterion and the claimed C-1 restoration; the current plan promises both no write/stranding and the mechanism that causes it.
- [ ] Leg 5 pins the wrong segmented-record ceiling. `brief.md:50-54` tests a `seg:` value just below `MAX_VALUE_BYTES`, and `brief.md:122-124` mandates the flat-record helper, but the target says a segmented placement write must be weighed against `MAX_ROOT_VALUE_BYTES` (`crates/core/src/metadata.rs:371-375`); proposal 0016 gives `MAX_SEG_CHUNKS` the same `V/2` headroom rule (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1462-1467`). As written, the green test requires admitting segment rewrites between 50,001 and 100,000 bytes, contrary to the target's headroom invariant.
- [ ] The tracker and dependency claims cannot be checked from the supplied advisory inputs: `notes.json`, `sources/`, and `dependency-state.json` are absent, while `brief.md:97-112` relies on external state for `main`, conflict #717, merged prerequisites #695/#696/#697/#710, and downstream #722. The target does confirm `HEAD == origin/main == 92e1b4b`, but it cannot establish those issue states or whether a load-bearing tracker comment was omitted; the planner must supply the tracker/dependency evidence or remove claims that depend on it.
- [ ] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rebuild targeting the two substantive implementation defects found by the adversarial review, not the whole §6 list: 1. Repair-loop containment (crates/custodian/src/reconstruction.rs:322) counts/reports a refusal once per *obligation* instead of once per *object* — restore the once-per-object dedupe the deleted `Reading::refused` set guaranteed. A segmented object with two queued chunks in the same seg: record must produce one refusal tick, not two. 2. An unattributable committed object with a segmented map is now silently skipped (crates/custodian/src/reconstruction.rs:513-515, `let Some(inode_id) = parse_inode_key(&key) else { continue }`), which does not set `reading.incomplete` and lets the obligation be deleted for a chunk a committed map still references. Fix by routing through `reading.contain(&key, ...)` instead of the bare `continue`, per the fail-closed "never silent skip" rubric class. Explicitly out of scope for this iteration: the two broken rustdoc intra-doc links (cosmetic, ungated) and the C4 `cargo deny` / RUSTSEC-2026-0258 `h2` advisory (unrelated supply-chain finding, not caused by this patch) — do not spend the iteration on either.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
