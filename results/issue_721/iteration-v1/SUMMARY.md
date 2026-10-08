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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 84.2% — 149 of 177 instrumentable changed lines executed (floor 80%); 177 of 511 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 37 mutants tested in 84s: 4 missed, 19 caught, 14 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.96s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of the fix that lets reconstruction repair a `ChunkRef` stored in a segmented map by atomically repointing its `seg:` record.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief settles the defect, five observable legs, merge/conflict semantics, the V/2 segment ceiling, atomic evidence, scope, and size limits without leaving an implementation input undecided. |
| C2 Reproduction (red pre-fix) | PASS | With production stashed and the discriminator retained, legs 1 and 2 failed exactly because reconciliation remained `Blocked`, while legs 3–5 passed as declared (`crates/custodian/tests/segmented_map_repoint.rs:423`, `crates/custodian/tests/segmented_map_repoint.rs:487`). |
| C3 Change | PASS | The change stays on the specified writer/caller surface and lets the caller compose the placement CAS, obligation deletion, and orphan evidence into one mutation (`crates/core/src/metadata.rs:2836`, `crates/custodian/src/reconstruction.rs:898`). |
| C4 Verification (red→green) | PASS | Independent red→green reproduced 2 of 5 failures before the fix and 5 of 5 passes after restoration; the affected reconstruction suite passed 6 of 6, core unit subset 8 of 8, and frozen full-CI, diff-coverage, and TiKV logs show their asserted green outcomes (`crates/custodian/tests/segmented_map_repoint.rs:430`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must add in-crate assertions that a flat repoint changes `chunk_map` and increments `version`—four killable mutants remove those semantics while the core mutation run stays green, so the brief's mutation evidence remains incomplete (`crates/core/src/metadata.rs:2859`). |
| T1 Structure | PASS | The core layer owns a narrow batch-producing metadata primitive and custodian adds repair-specific evidence before the single commit, preserving dependency direction and atomic composition (`crates/core/src/metadata.rs:2753`, `crates/custodian/src/reconstruction.rs:921`). |
| T2 Shape | PASS | The patch uses exactly the four allowed files, is 94,957 bytes, adds 157 non-test semantic source lines, and the segmented arm reads only the root-selected record, all within the stated caps (`crates/core/src/metadata.rs:2830`). |
| T3 Runtime | PASS | Applied-target tests, fmt, changed-file typos, and affected-crate clippy pass locally; the frozen complete gate also exercised workspace tests, conformance, dependency scans, docs, and TiKV feature compilation (`crates/custodian/tests/segmented_map_reconstruction.rs:637`). |
| T4 Contribution | FAIL | Deep review is unresolved because the contribution can rewrite a freshly read segment without proving its extent still matches the root; the later PR-artifact audit is correctly N/A until publish (`crates/core/src/metadata.rs:2900`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must compare the fresh `SegmentRecord` offset and length with the selected `SegmentRef` and race-test the mismatch—otherwise maintenance can CAS a root-inconsistent row that normal resolution rejects (`crates/core/src/metadata.rs:2900`, `crates/core/src/metadata.rs:2607`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | After the rebuild, decide whether to trigger or watch `tier2-kill-reconstruct` and the Tier-1 disk-fault campaign before sign-off—unit doubles prove mechanics but not operational durability under process or device failure (`AGENTS.md:78`, `AGENTS.md:81`). |

### Advisory — adversary

# Adversarial review — issue #721 (advisory, never gating)

Method: re-read the frozen gate logs, then rebuilt the target in a scratch copy and ran four
probe tests against the **patched** production code (`reconcile_step`, the real control point,
with the new test file's own fixture helpers). Probe results quoted below are runs, not
readings.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2900-2908`: the freshly-read `seg:`
  record is never re-checked against the root `SegmentRef` the move selected at `:2882-2888`,
  so a racing rewrite whose bounds contradict the root is repointed and the obligation
  DRAINED.** The resolver enforces exactly this invariant (`record.byte_offset()/byte_len()`
  vs. the root's entry, `crates/core/src/metadata.rs:2607`); the write side drops it and
  derives `within` from the record's *own* claim (`:2903`). Concrete failing case, run:
  leg 3's fixture and window (`Race::AfterSegmentPage`), racer writes
  `SegmentRecord::new([SIBLING, CHUNK], 0)` at `seg:…:1` — structurally valid, but it claims
  object bytes `0..16` while the root's table says that segment starts at 8, so it overlaps
  segment 0. `within = 8 − 0 = 8` lands exactly on `prior`, the equality pin matches, and the
  move commits: **outcome `Changed`, `queued_repairs` empty (the obligation deleted), one
  orphan mark published, and `resolve_chunk_map` on the object afterwards returns `Err`.** The
  repair certifies over — and discharges "the last record saying live data is
  under-replicated" for — a map no reader can resolve. Fix is ~3 lines (bounds mismatch →
  `Repoint::Conflict`, which keeps the obligation queued). Reachability today is a foreign /
  future `seg:` writer only (#653 owns the publisher), but that is the same window legs 2–3
  exist to pin. This is also the *correct* form of the three blocking T4 findings
  (`gate-logs/T4-batch-review.log`), whose stated consequence is wrong: the move cannot
  *create* an extent mismatch — `SegmentRecord::new(chunks, record.byte_offset())` (`:2914`)
  preserves both offset and derived length — it silently *acts on* one.

- **NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_repoint.rs:577` and `:637`
  assert a property the pass does not have, under a message claiming it does.** Both legs
  assert only `assert_ne!(outcome, Reconciled::Changed)` with the message "the pass must not
  certify a repair it did not make". Run: leg 3's scenario answers **`Reconciled::Satisfied`**
  (probe A) and leg 4's answers **`Satisfied`** with the obligation still queued and the
  rebuilt fragment left on the free server (probe D). `Satisfied` *is* this module's
  certification — its own words at `crates/custodian/src/reconstruction.rs:310-312` and
  `:350-357` ("an operator reading `Satisfied` is being told redundancy is restored"), which is
  why a ceiling refusal is routed to `Blocked` at `:348`. So brief leg 3's criterion ("the pass
  does not certify") is **not met post-fix**, and the assertion is shaped so it cannot notice.
  Tagged `[human]` because the honest alternatives are (a) accept it — a lost repoint race is
  transient and the flat path has always answered `Satisfied` — and correct the leg's message
  and the brief, or (b) make a lost placement CAS non-certifying, which changes the flat arm
  too and is a scope decision, not an iteration.

## Attempted and could not refute

- **The red→green is genuine and on the production path.** `gate-logs/C4-verify.log` shows
  legs 1 and 2 failing on assertions (`Blocked` vs `Changed`) with production reverted, not on
  compilation; the test file names none of the symbols this patch introduces (no `repoint_chunk`
  / `Repoint` / `segment_value_ceiling_crossed` anywhere in
  `crates/custodian/tests/segmented_map_repoint.rs`), everything is driven through
  `reconcile_step` and read back out of the store, and leg 2's `meta.raced()` self-check
  (`crates/custodian/tests/segmented_map_repoint.rs:505`) forecloses a vacuous pass where the race never fired.
- **Two obligations inside ONE `seg:` record do not serialise or corrupt each other** (probe C):
  both land in the same pass with placements `[[0,2],[0,2]]` and an empty queue — the second
  move re-reads the record the first one committed, and the sibling entry it holds still equals
  its own `prior`.
- **The new root CAS on a *segmented* root does not depend on an unproven encode identity** —
  `require(root_key, encode(generation))` (`crates/core/src/metadata.rs:2924`) rests on
  decode→encode byte identity, which is already pinned for the segmented root shape by
  `crates/core/tests/segmented_map_record.rs:142`.
- **Neither `?` in the segmented arm can end a fleet-wide pass for one object's data.**
  `seg_key(...)?` (`:2891`) is unreachable because `SegmentedMap::new` rejects an unaddressable
  index at decode (`:913`), and `SegmentRecord::new(...)?` (`:2914`) cannot newly fail because
  only a `placement` changed, so `checked_chunk_bytes` and the emptiness/span checks see the
  same values the decode already accepted.
- **Offset addressing does not mis-address a zero-length neighbour or a duplicate id.**
  `chunk_at` (`:2947-2959`) breaks only on `at > byte_offset`, so a zero-length chunk sharing an
  offset is walked past rather than hiding its successor; duplicate committed ids are the
  standing #700 deferral and behave exactly as on the base (first reference in key order wins,
  `crates/custodian/src/reconstruction.rs:534`).
- **The rustdoc link from the public `flat_value_ceiling_crossed` to the new private
  `segment_value_ceiling_crossed` (`crates/core/src/metadata.rs:375`)** does raise `rustdoc::private_intra_doc_links`, but it matches five
  pre-existing instances in the same crate (`InodeRecordWire`, `live_lease_guards`, …) and no
  CI job builds rustdoc — house style, not a finding.
- **Budget conformance**: 4 files, ~157 added semantic non-test production lines (the
  `#[cfg(test)]` hunk excluded), `patch.diff` 92.7 KB — all inside the brief's caps.

## Verdict annotations (no action, so the human is not re-litigating them)

- `C5-mutants` "4 missed" is a mutation **scope** artifact, not a test gap: all four are in
  `repoint_chunk`'s flat arm (`crates/core/src/metadata.rs:2858-2860`) and `cargo mutants`
  tested them with `wyrd-core`'s package tests only. I re-applied the `delete field chunk_map`
  mutation by hand and `cargo test -p wyrd-custodian --test reconstruction` fails 5 tests
  (including `kills_a_d_server_and_reconstructs_to_full_redundancy_through_reconcile_step`).
  Do not route C5 back to Do as a coverage defect.
- T4's fourth blocking finding (Tier-0 DST coverage for the new concurrent path,
  `gate-logs/T4-batch-review.log`) is answered by the brief's carve-out to **#722**, which
  forbids touching `crates/dst/tests/custodian.rs` here; per the rubric's "deferrals are
  settled" it wants a decline-with-reference, not a fix in this slice.
- `check-gates.json`'s C4-verify line ("5 test(s) ran red") counts tests *run*, not failed —
  the log shows 2 discriminating failures (legs 1, 2), exactly the posture the brief declared.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must add in-crate assertions that a flat repoint changes `chunk_map` and increments `version`—four killable mutants remove those semantics while the core mutation run stays green, so the brief's mutation evidence remains incomplete (`crates/core/src/metadata.rs:2859`).
- [ ] T5 Judgment — Rebuild must compare the fresh `SegmentRecord` offset and length with the selected `SegmentRef` and race-test the mismatch—otherwise maintenance can CAS a root-inconsistent row that normal resolution rejects (`crates/core/src/metadata.rs:2900`, `crates/core/src/metadata.rs:2607`).
- [ ] Validation — fitness-to-purpose — After the rebuild, decide whether to trigger or watch `tier2-kill-reconstruct` and the Tier-1 disk-fault campaign before sign-off—unit doubles prove mechanics but not operational durability under process or device failure (`AGENTS.md:78`, `AGENTS.md:81`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- [ ] The deterministic race recipe for success legs 2–3 is false on the target. `brief.md:62-73` says resolution performs the first `get` of the `seg:` key and the move performs the second, but the resolver reads segment records with `MetadataStore::scan_page`, not `get` (`crates/core/src/metadata.rs:2414-2425`, `crates/core/src/metadata.rs:2455-2461`). The proposed move would introduce the only `get(seg_key)`, so applying the competing write “after the first return” is after the move has captured its CAS bytes: the sibling edit conflicts instead of being merged. Revise the fixture trigger (for example, hook the resolver's segment-page read) before claiming legs 2 and 3 exercise the intended window.
- [ ] The criterion and scope contradict each other about losing CAS writes. Legs 3–4 require “nothing at all is written” (`brief.md:35-49`), while the scope explicitly excludes destination pre-marking and accepts a pre-written destination fragment after a lost CAS (`brief.md:177-187`). The production ordering writes rebuilt fragments before committing (`crates/custodian/src/reconstruction.rs:931-949`), and GC does **not** have the claimed ordinary unreferenced sweep: without an orphan mark or expired lease it conservatively retains the fragment (`crates/custodian/src/gc.rs:196-210`). The target design therefore requires a destination pre-mark and leaves it standing on CAS loss (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:354`, `:2577`). Either add that prerequisite/scope or narrow the criterion and the claimed C-1 restoration; the current plan promises both no write/stranding and the mechanism that causes it.
- [ ] Leg 5 pins the wrong segmented-record ceiling. `brief.md:50-54` tests a `seg:` value just below `MAX_VALUE_BYTES`, and `brief.md:122-124` mandates the flat-record helper, but the target says a segmented placement write must be weighed against `MAX_ROOT_VALUE_BYTES` (`crates/core/src/metadata.rs:371-375`); proposal 0016 gives `MAX_SEG_CHUNKS` the same `V/2` headroom rule (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1462-1467`). As written, the green test requires admitting segment rewrites between 50,001 and 100,000 bytes, contrary to the target's headroom invariant.
- [ ] The tracker and dependency claims cannot be checked from the supplied advisory inputs: `notes.json`, `sources/`, and `dependency-state.json` are absent, while `brief.md:97-112` relies on external state for `main`, conflict #717, merged prerequisites #695/#696/#697/#710, and downstream #722. The target does confirm `HEAD == origin/main == 92e1b4b`, but it cannot establish those issue states or whether a load-bearing tracker comment was omitted; the planner must supply the tracker/dependency evidence or remove claims that depend on it.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must add in-crate assertions that a flat repoint changes `chunk_map` and increments `version`—four killable mutants remove those semantics while the core mutation run stays green, so the brief's mutation evidence remains incomplete (`crates/core/src/metadata.rs:2859`).; T5 Judgment — Rebuild must compare the fresh `SegmentRecord` offset and length with the selected `SegmentRef` and race-test the mismatch—otherwise maintenance can CAS a root-inconsistent row that normal resolution rejects (`crates/core/src/metadata.rs:2900`, `crates/core/src/metadata.rs:2607`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
