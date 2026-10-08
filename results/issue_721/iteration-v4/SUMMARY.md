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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.2% — 383 of 398 instrumentable changed lines executed (floor 80%); 398 of 807 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 48 mutants tested in 2m: 31 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.15s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing the fix that lets reconstruction complete a queued repair by repointing a `ChunkRef` stored in a segmented `seg:` metadata record.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a falsifiable failure, atomicity/race outcomes, the V/2 ceiling decision, and explicit scope boundaries (`brief.md:13`). |
| C2 Reproduction (red pre-fix) | PASS | An independent tracked-change stash made 3 of 7 discriminator tests fail, including the binding segmented repair, and restoring the patch made all 7 pass (`crates/custodian/tests/segmented_map_repoint.rs:496`). |
| C3 Change | PASS | The core prepares the root-plus-segment CAS and the custodian adds obligation deletion and orphan evidence before one commit, matching the requested lifecycle change (`crates/core/src/metadata.rs:2912`, `crates/custodian/src/reconstruction.rs:939`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The human must decide how to clear or waive the unrelated locked `h2` advisory: red→green, fmt, clippy, tests, typos and machete pass, but full CI stops at `cargo deny` on `h2 0.4.15` (`gate-logs/C4-ci.log:5222`, `Cargo.lock:1536`). |
| C5 Causal adequacy | PASS | The change removes the refusal by addressing the covering record with byte offset plus exact prior-reference equality, and the mutation gate reports no surviving viable mutant (`crates/core/src/metadata.rs:2781`, `crates/core/src/metadata.rs:2932`, `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | Metadata owns CAS preparation while custodian owns repair evidence and commit, preserving the existing seam and dependency direction (`crates/core/src/metadata.rs:2816`, `crates/custodian/src/reconstruction.rs:916`). |
| T2 Shape | FAIL | The 107,031-byte `patch.diff` exceeds the explicit 95 KB cap, although it stays within four files and 192 added semantic non-test lines (`brief.md:199`). |
| T3 Runtime | PASS | The repoint reads only the covering segment, race losses write no repair metadata, and the metadata trait assigns network termination to bounded backend adapters (`crates/core/src/metadata.rs:2867`, `crates/traits/src/lib.rs:1337`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit is mandatory at publish, exactly as the deferred gate records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | The human must confirm prior art across merged and closed/rejected work: the supplied target has only one synthetic commit and no remote, so the affected-path history claim cannot be independently settled; the batch-review classes are otherwise settled under #698/#722 or contradicted by the backend timeout contract (`brief.md:311`, `crates/traits/src/lib.rs:1337`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The human must decide whether the in-memory red→green and race evidence is sufficient for production durability fitness, because automation establishes mechanics but not operational fitness (`crates/custodian/tests/segmented_map_repoint.rs:492`). |

### Advisory — adversary

# Adversarial review — issue #721 (advisory, never gating)

Re-ran the asserted red→green myself on a scratch clone of `$PDCA_TARGET`: with
`crates/core/src/metadata.rs` and `crates/custodian/src/reconstruction.rs` reverted to the base
and the new test kept, `cargo test -p wyrd-custodian --test segmented_map_repoint` fails 3 of 7
(legs 1, 2 and the torn-record leg, each with the base's `refused-segmented` audit row in the
panic message); with the two production files restored it is 7/7 green. The discriminator drives
the real control point (`reconcile_step`) and observes the store, not a parallel re-implementation.
The red is genuine and the gate's claim holds. Two findings survive that.

- **NEEDS-HUMAN [impl] — a zero-length chunk at the *tail* of a `seg:` record is unrepairable
  forever, silently, and the pass certifies `Satisfied` over it.**
  `crates/core/src/metadata.rs:2869-2874` picks the covering segment **by offset alone**
  (`covers`, `:2922`) *before* the `ChunkRef` equality pin is applied, so a chunk whose object
  offset equals a segment boundary but which lives at the end of the **previous** record is looked
  up in the wrong record and answered `Repoint::Conflict`. The flat arm has no such split — its
  `chunk_at` (`:2932`) scans every chunk sharing that offset and finds the one that equals `prior`
  — so the two arms of one primitive disagree on the same chunk list. Reproduced twice on the
  patched tree: (a) at the primitive, `repoint_chunk` over segments `[[c(len 8), z(len 0)] @0,
  [c(len 8)] @8]` moving `z` at offset 8 answers `Conflict`, while the identical chunk list as a
  flat map answers `Prepared`; (b) end-to-end through `reconcile_step` with `z` queued, three
  consecutive passes each answered `Reconciled::Satisfied` with the obligation still queued, the
  `seg:` record byte-identical and nothing named on the durability seam. That is exactly the C-1
  shape the brief says the fix must remove ("no actor that exits the state") re-entered through a
  different door, and it is *worse* than the base, which at least named the object and answered
  `Blocked`. Note the report path compounds it: `reconstruction.rs:930` routes this prepare-time
  refusal to `emit_conflict` (`:1099-1106`), whose row states "lost the version-conditional
  commit" when no commit was attempted and no fragment was written — so a permanent condition is
  logged as transient churn. (I am **not** asking for the "collectable garbage" wording, which
  getwyrd/wyrd#723 owns.) Zero-length chunks are format-admissible today —
  `SegmentRecord::checked` (`:1179-1201`) rejects only an empty list and a zero *total* — and
  `erasure::encode(1,1,&[])`/`reconstruct` both succeed, so `assess` does plan such a chunk; I
  found no in-tree producer that mints one yet (the segmented committer is #653's), which is why
  this is a latent input rather than a live outage. Cheap fix: pick the covering segment *and* the
  segment ending at `byte_offset`, then let the existing equality pin choose.

- **NEEDS-HUMAN [human] — leg 3's "the pass does not certify" is not what the shipped test
  asserts, and the behaviour is the opposite.** `crates/custodian/tests/segmented_map_repoint.rs:485-489`
  asserts only `assert_ne!(outcome, Reconciled::Changed)` under the message "the pass must not
  certify a repair it did not make". `Changed` is not the certification — `Satisfied` is
  ("Reality already matched the desired state; nothing was done",
  `crates/custodian/src/reconciliation.rs:21-22`). I ran leg 3's own fixture and printed the
  outcome: the pass answers **`Satisfied`** with the obligation still queued and the chunk still
  under-replicated, because `hole` at `crates/custodian/src/reconstruction.rs:363` counts only
  containment and the ceiling refusal, never a conflict. So the brief's leg-3 requirement ("no
  orphan mark was published; **the pass does not certify**") is unmet, and the assertion is worded
  to look as though it is met. This needs a human because the two exits differ in blast radius:
  tightening the test to assert `Satisfied` amends the stated success criterion, while making a
  repoint conflict a hole changes the **flat** arm's standing base behaviour too (a lost CAS has
  always answered `Satisfied`) and is outside this slice's scope. Finding 1 above is what makes
  the choice load-bearing rather than cosmetic: a *permanent* conflict is certified as `Satisfied`
  on every pass forever.

## Attempted and could not refute

- The read→prepare window in legs 2/3 is genuinely reached, not aspirational: `MemMeta::scan_page`
  fires the racing batch after materialising the `seg:` page (`segmented_map_repoint.rs:106-119`)
  and every racing leg self-checks `meta.raced()`, so a leg cannot pass because the race never
  landed. The parent attempt's `RaceAtRepoint` shape is not reproduced.
- Two obligations inside **one** `seg:` record: I expected the second to lose its CAS on bytes the
  first superseded; both land in one pass (`placements(1) == [[0,2],[0,2]]`, queue empty), because
  the primitive re-reads the record and pins only the planned `ChunkRef`. The merge design holds
  under its own worst case.
- Serialization identity of the segmented CAS: the `seg:` precondition uses the **raw stored
  bytes** (`metadata.rs:2879`, `:2912-2917`), not a re-encode, so a racing writer's row is pinned
  byte-exactly; `SegmentRecord::new(record.chunks().to_vec(), record.byte_offset())` drops no field
  (`byte_len` is re-derived from the same chunks) and the root, which *is* re-encoded, carries no
  `ChunkRef` in the segmented arm.
- Boundary of the V/2 ceiling (`segment_value_ceiling_crossed`, `metadata.rs:386-398`): admits
  exactly `MAX_ROOT_VALUE_BYTES`, refuses `+1`, refuses before any fragment write; a shrinking
  repoint of an already-oversized record is still allowed. No off-by-one.
- Ordering: `repoint_chunk` now runs **before** the fragment writes (`reconstruction.rs:924-936`),
  so a ceiling refusal and a prepare-time conflict strand nothing new; only the commit-time CAS
  loss does, which is the pre-existing #723 leak the brief scopes out.
- Containment dedupe: a torn `seg:` record met under two obligations produces one audit row and
  one counter tick (`Reading::contain`, `reconstruction.rs:412-424`), and the drain batch is gated
  on `incomplete()` *after* the repair loop, so a containment discovered at repoint time still
  suppresses the drain. The iteration-3 carry-forward items look genuinely addressed.
- Duplicate chunk id at two offsets in one segmented object: first reference wins, the second keeps
  the dead placement and the obligation is drained — identical to the base's flat behaviour and
  explicitly #700's (`reconstruction.rs:444-450`), so not filed.
- `C4-ci` red is `cargo deny` / RUSTSEC-2026-0258 (`h2` 0.4.15, reached through `hyper`/`tonic`);
  the log shows every test target green. That is a dependency advisory, not a defect in this patch,
  and the previous sign-off already scoped it out — **not** a refutation.
- The T4 batch review's `[CONVENTION]` item about the new unbounded `MetadataStore::get` await
  (`metadata.rs:2879`) reads as noise against this repo's own written rule: `read_committed`'s
  doc (`reconstruction.rs:494-500`) records that the bound on such awaits is the store
  implementation's, not the caller's (#508/#636), and every peer walk follows it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — The human must decide how to clear or waive the unrelated locked `h2` advisory: red→green, fmt, clippy, tests, typos and machete pass, but full CI stops at `cargo deny` on `h2 0.4.15` (`gate-logs/C4-ci.log:5222`, `Cargo.lock:1536`).
- [ ] T5 Judgment — The human must confirm prior art across merged and closed/rejected work: the supplied target has only one synthetic commit and no remote, so the affected-path history claim cannot be independently settled; the batch-review classes are otherwise settled under #698/#722 or contradicted by the backend timeout contract (`brief.md:311`, `crates/traits/src/lib.rs:1337`).
- [ ] Validation — fitness-to-purpose — The human must decide whether the in-memory red→green and race evidence is sufficient for production durability fitness, because automation establishes mechanics but not operational fitness (`crates/custodian/tests/segmented_map_repoint.rs:492`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_721/review-b
- [ ] The deterministic race recipe for success legs 2–3 is false on the target. `brief.md:62-73` says resolution performs the first `get` of the `seg:` key and the move performs the second, but the resolver reads segment records with `MetadataStore::scan_page`, not `get` (`crates/core/src/metadata.rs:2414-2425`, `crates/core/src/metadata.rs:2455-2461`). The proposed move would introduce the only `get(seg_key)`, so applying the competing write “after the first return” is after the move has captured its CAS bytes: the sibling edit conflicts instead of being merged. Revise the fixture trigger (for example, hook the resolver's segment-page read) before claiming legs 2 and 3 exercise the intended window.
- [ ] The criterion and scope contradict each other about losing CAS writes. Legs 3–4 require “nothing at all is written” (`brief.md:35-49`), while the scope explicitly excludes destination pre-marking and accepts a pre-written destination fragment after a lost CAS (`brief.md:177-187`). The production ordering writes rebuilt fragments before committing (`crates/custodian/src/reconstruction.rs:931-949`), and GC does **not** have the claimed ordinary unreferenced sweep: without an orphan mark or expired lease it conservatively retains the fragment (`crates/custodian/src/gc.rs:196-210`). The target design therefore requires a destination pre-mark and leaves it standing on CAS loss (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:354`, `:2577`). Either add that prerequisite/scope or narrow the criterion and the claimed C-1 restoration; the current plan promises both no write/stranding and the mechanism that causes it.
- [ ] Leg 5 pins the wrong segmented-record ceiling. `brief.md:50-54` tests a `seg:` value just below `MAX_VALUE_BYTES`, and `brief.md:122-124` mandates the flat-record helper, but the target says a segmented placement write must be weighed against `MAX_ROOT_VALUE_BYTES` (`crates/core/src/metadata.rs:371-375`); proposal 0016 gives `MAX_SEG_CHUNKS` the same `V/2` headroom rule (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1462-1467`). As written, the green test requires admitting segment rewrites between 50,001 and 100,000 bytes, contrary to the target's headroom invariant.
- [ ] The tracker and dependency claims cannot be checked from the supplied advisory inputs: `notes.json`, `sources/`, and `dependency-state.json` are absent, while `brief.md:97-112` relies on external state for `main`, conflict #717, merged prerequisites #695/#696/#697/#710, and downstream #722. The target does confirm `HEAD == origin/main == 92e1b4b`, but it cannot establish those issue states or whether a load-bearing tracker comment was omitted; the planner must supply the tracker/dependency evidence or remove claims that depend on it.
- [ ] size backstop — this slice is behaving oversized: patch is 105 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rationale: the T4 rubric review failed (4 blocking), and several §6 NEEDS-HUMAN items are defects in the brief itself, not just gaps Do can close: - The brief's race-window recipe ("resolution performs the first get of the seg: key") does not match the target: the resolver reads segment records via MetadataStore::scan_page, not get, so the described trigger cannot reach the intended read->prepare window as written. - The brief's own success criterion (legs 3-4: "nothing at all is written" on a lost CAS) contradicts its own scope section, which explicitly excludes the destination pre-mark and accepts a pre-written, stranded destination fragment after a lost CAS. - Leg 5 pins the wrong ceiling constant for segmented records (tests against MAX_VALUE_BYTES / the flat-record helper per brief.md:50-54,122-124, while the target's own headroom rule requires weighing a seg: record against MAX_ROOT_VALUE_BYTES / V/2). - Tracker/dependency claims (merged prerequisites #695/#696/#697/#710, conflict #717, downstream #722) are unverifiable from the supplied bundle — supporting evidence files are absent. - Size backstop: patch is 105KB (>100KB threshold), 3 rounds already spent (>2 threshold) — matches the bundle's own iterate-plan recommendation. Also carry forward for the re-plan's consideration (implementation-shaped but likely brief- scoping issues too): - [impl] a zero-length chunk sitting at a segment boundary is matched to the wrong seg: record by offset before the ChunkRef equality check, making it permanently unrepairable and silently certified Satisfied rather than honestly Blocked (crates/core/src/metadata.rs:2869- 2874, 2922). Latent (no in-tree producer mints such a chunk yet) but reintroduces the brief's own C-1 defect shape through a different door. - [human] leg 3's shipped test does not assert the brief's stated success criterion ("the pass does not certify") — it asserts something weaker, and the true behavior is the opposite: the pass certifies Satisfied while the chunk stays under-replicated forever. Tightening the assertion changes the flat arm's long-standing behavior too, so this is a scope call for the re-plan, not a Do-level test fix. Human directive: iterate-plan for 721 — return to Plan for `pdca split`.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
