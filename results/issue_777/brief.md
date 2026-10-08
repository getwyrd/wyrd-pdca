- **Slug:** reconstruction-completes-seg-repair-through-primitive
- **Defect:** **A chunk whose `ChunkRef` lives in a `seg:` record is never repaired.**
  #697 stopped reconstruction aborting on a segmented object but deliberately writes
  nothing: the repair obligation is routed to `Site::Refused`
  (`crates/custodian/src/reconstruction.rs:552`) and answered `Assessment::Refused`
  (`:609`), every pass, forever — the obligation is not drained (data loss) and, until
  #776, nothing could move the placement. With #776 merged the primitive **exists but
  nothing calls it**: a multipart-published object's redundancy still decays untended,
  permanently. That is the C-1 violation this bundle exists to close — see **Invariant to
  restore**.
- **Success criterion:** the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
  passes, driven **only** through symbols visible on the base *after #776 merges* —
  `wyrd_custodian::{reconcile_step, Custodian, FencedZone, ReconstructionContext,
  Reconciled}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{seg_key, inode_key, encode, decode, MAX_VALUE_BYTES,
  SegmentGroup, SegmentRecord, SegmentRef, SegmentedMap, ChunkMap, InodeRecord, ChunkRef,
  EcScheme}` (**`MAX_VALUE_BYTES`, not `MAX_ROOT_VALUE_BYTES`** — leg 5 pins the FULL ceiling;
  reversed by the human 2026-08-19, see #776's leg 5 for the warrant)
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
     do not reword the two "collectable garbage" comments; #723 owns them).
     **DECIDED BY THE HUMAN 2026-08-19 — the carried-forward "may the pass still certify?"
     question is CLOSED, and this leg now asserts the verdict as well as the store: the pass
     MUST answer `Reconciled::Satisfied`.** That is the flat arm's own answer for a
     conflict-only pass and this child changes it for nobody: `RepairOutcome::Conflict` emits a
     counter and nothing more (`reconstruction.rs:311-318`), and the pass's `hole` covers an
     incomplete reading, a segmented refusal and a ceiling refusal — **not** conflicts
     (`:341-357`). Rationale for keeping it: a lost CAS is a *retry*, not a dead end — nothing
     repair-owned was written, the obligation is still queued, and the next pass re-plans onto
     the winner's bytes, so C-1's "an actor that exits it in bounded time" holds. **Do NOT
     widen `hole` to include conflicts:** that would change the flat arm's long-standing
     behaviour and its existing tests — a different slice with a different blast radius.
     Asserting it here pins the decision instead of leaving it accidental.
  4. **Not independently red** — a superseded root generation is a **CONFLICT**: no
     *repair-owned* metadata written (the leg's own setup writes the competing root, so no
     blanket no-write assertion), obligation still queued, and — same decision, same reason —
     the pass answers **`Reconciled::Satisfied`**.
  5. **Not independently red** — the ceiling refusal holds over a segment record at the
     **full `MAX_VALUE_BYTES`** bound (#776's segmented-arm check, exercised through the whole
     pass): refused, record byte-identical, obligation queued, and the pass answers
     **`Reconciled::Blocked`** — a ceiling refusal *is* a hole (`ceiling_refused`,
     `reconstruction.rs:315-318`, `:341-357`), unlike a conflict. Seed the record just under
     `MAX_VALUE_BYTES` programmatically, never as a byte literal (the 85 KB `patch.diff`
     budget). **Bound reversed from V/2 by the human 2026-08-19** — warrant in #776's leg 5;
     do not restore V/2 and do not name `MAX_ROOT_VALUE_BYTES` here.
  **The race window is reachable deterministically — hook the RIGHT read.** The resolver reads
  the group's `seg:` range with **`scan_page`**, never `get` (`read_group_range`,
  `metadata.rs:2452-2461`, docstring `:2417-2425`); the move's own read is the **only** `get`
  on a `seg:` key. Apply the racing batch **after returning the `scan_page` page**
  (equivalently: on the first `get` of that key, before answering it). Counting `get`s and
  injecting after the first return does NOT work — it quietly inverts leg 2. Leg 4's root flip
  goes on the way into `commit`, after resolve. This gap sank the parent (`RaceAtRepoint`
  injected inside `commit()`) — do not reproduce that shape.
  **Two REGRESSION GUARDS carried from iteration 3 — read the scoping carefully, it was
  corrected at Plan.** Both name defects the *previous attempt introduced by restructuring*,
  not defects on the base; neither licenses fixing the base's own pre-existing behaviour.
  (a) **Refusal containment stays once per OBJECT, not per obligation** — a segmented object
  with two queued chunks in one `seg:` record must produce ONE refusal tick. The base gets
  this right via the `reading.refused` set (`reconstruction.rs:546-552`); iteration 3 lost it
  while restructuring. Preserve it. (b) **Introduce no new silent skip:** if this child
  restructures the committed-object walk, an unattributable object must not fall out of it
  without setting `reading.incomplete` — otherwise an obligation can be drained for a chunk a
  committed map still references. **BUT the base's own `(Some(_), None) => continue`
  (`:511-514`) is deliberate and stays:** its comment assigns it to **#698** ("#698 owns the
  fix"), which is OPEN. Do **not** fix #698's `parse_inode_key` canonicalisation here — that
  is the noncanonical-`inode:01` bug the iteration-4 batch review also raised, and it belongs
  to #698, not this slice.
  **Verification posture:** legs 1–2 are the C4-verify red→green. **Leg 5** passes on the base
  by construction. **Legs 3–4 are now *coarse*-red** — the added `Satisfied` assertion fails on
  the base, which answers `Blocked` from the segmented refusal — but that red only distinguishes
  "refused" from "attempted"; it is **not** evidence the CAS pins are right, so do not present
  it as such. The pins behind legs 3–5 stay bound to the mutation oracle, each with its named
  negation *demonstrated* in `build-notes.md`. Mechanics and the hard constraint on keeping the red
  compilable: see **Falsifiability** below.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** on the target checkout, seed a committed segmented object as raw
  `seg:` records plus a segmented root (per
  `crates/custodian/tests/segmented_map_restore.rs:387-431` — this build ships no producer
  of segmented maps, which is why the fixture hand-writes them) with a lost fragment;
  enqueue its repair; run `reconcile_step` with a `ReconstructionContext`. The obligation
  is refused (`reconstruction.rs:552`, `:609`) and stays queued, every pass, forever —
  deterministic, no seed sweep, no race window needed to observe it. Still true after
  #776 merges: the primitive exists, nothing calls it.
- **Scope:** the repair pass stops refusing a `seg:`-resident chunk and completes the move
  through **#776's primitive**. The placement change, the obligation discharge
  (`repair::repair_key` delete) and the orphan evidence per displaced position stay **ONE
  batch** — the caller adds its evidence to the batch the primitive hands back
  (`0005:277`, ADR-0015); if the primitive's shape makes that awkward, the finding goes
  back to #776, this child does not fork the batch. Mirror `repair_chunk` and the seeding
  fixture (see **Citations expected**). Duplicate chunk ids get one plan, not independent
  ones — the narrow rule only, no cross-object claim-counting (#651's replan). **A losing
  CAS does not retract already-published bytes** — settled, rejected 4×
  (`results/issue_638/review-rejected.md:15-16`); refusal and conflict paths write no
  repair-owned metadata at all. **Salvage:** the reconstruction caller in
  `results/issue_711/iteration-v1/patch.diff` (harness repo) passed C4-ci and C4-verify —
  reuse it minus everything in the rebalance and DST files (#722's). This bundle's own
  `iteration-v4/patch.diff` is the most refined prior attempt.
  **Untouched:** `crates/core/src/metadata.rs` entirely (#776 owns it — if this child
  needs a core edit, the split is wrong: STOP and hand back); `rebalance.rs`, `backfill.rs`,
  `restore.rs`, `gc.rs`, `desired_state.rs`; `custodian/tests/segmented_map_rebalance.rs` and
  `crates/dst/tests/custodian.rs` (#722); the read side (`resolve_chunk_map`, the
  #695/#696/#697 containment rule); **`parse_inode_key`'s noncanonical-key handling — #698
  (OPEN) owns it, and the base comment at `:513` says so**; any ADR/spec/proposal, conformance
  vector, or new
  dependency. **Forced edit, budgeted:** `custodian/tests/segmented_map_reconstruction.rs:489`
  (`:484` was stale; re-verified at `a801997` — cite the symbol, not the number)
  (`an_obligation_inside_a_segmented_object_is_refused_never_discarded`) asserts the refusal
  this child removes and is rewritten to assert the repair lands.
  **Budget:** 3 files — `custodian/src/reconstruction.rs`,
  `custodian/tests/segmented_map_repoint.rs` (**new**),
  `custodian/tests/segmented_map_reconstruction.rs` — ≤ 100 added semantic non-test lines,
  `patch.diff` **≤ 85 KB**. (Budget calibrated at Plan against the parent's v4 patch: the
  custodian half of it measured **~74 KB**, so the 60 KB first proposed was unachievable and
  would have failed its own budget on day one. 85 KB leaves headroom and still sits well under
  the 95 KB cap the parent's 107 KB blew.) Downstream #722's ordering is in the **Ordering
  note**: its real prerequisite is #776, not this child.
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
  base+#776:** `_resolve_base_ref` honours `$PDCA_VERIFY_BASE` (the wave's folded branch)
  at precedence `$PDCA_BASE > $PDCA_VERIFY_BASE > $WYRD_VERIFY_BASE > $PDCA_BRIEF_BASE`, so a
  wave-1 bundle verifies against a tree that **contains** the primitive — the red reads "it
  exists and nothing calls it", never "the symbol is missing". **HARD CONSTRAINT keeping the
  red compilable:** the test must name no symbol *this* patch introduces, and must not call
  #776's primitive directly — drive through `reconcile_step`, assert on the **store**.
  Expect **5 tests ran, 4 failing** (legs 1–2 binding, legs 3–4 coarse-red on the `Satisfied`
  assertion, leg 5 green by construction); the gate's line counts tests that **ran**, not tests
  that failed — read the count as a count.
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
- **Ordering note:** wave 1 — **`Depends on: #776`** is a genuine build-on dependency: the
  production code *calls* the primitive, so it neither compiles nor goes honestly red without
  it; the wave fold supplies #776's accepted diff via `$PDCA_VERIFY_BASE`, so no human
  merge is needed between them. **No conflicts:** touching only `crates/custodian/*`, this
  child collides with neither #772 (`metadata.rs`) nor #722 (`dst/tests/custodian.rs`) — a
  real gain of cutting by layer. Downstream **#722**'s true prerequisite is **#776**, not
  this child (disjoint files: `rebalance.rs` vs `reconstruction.rs`), so it may run in
  parallel with this child rather than behind it.
- **Disposition hint:** likely-fix
- **Difficulty:** high
- **Depends on:** 776

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 Contribution — Confirm affected-path merged and closed/rejected prior art for all four files — the brief records merged reconstruction history and archived attempts, while the supplied target exposes only one synthetic base commit; `brief.md:173`, `reviewer-prior-art.log:1`.; T5 Judgment — Assert the exact displaced orphan identity — checking only one mark also passes for a mark naming the survivor or rebuilt destination, so the test does not establish its claimed reclamation evidence; `crates/custodian/tests/segmented_map_repoint.rs:421`, `AGENTS.md:175`.; **The new move-time containment arm has no test at all.** `crates/custodian/src/reconstruction.rs:448-454` (with `:1157-1167` and `Reading::contain`'s once-per-object dedupe at `:541-546`) is how a typed `ChunkMapError` from the move becomes `Blocked` rather than a clean pass. C4-diff-cov lists all of these lines as MISS. C5 reports "0 missed" only because cargo-mutants made no mutant for an `if let` body. I disabled the containment by hand (the `if let Target::Committed(site)` at `:449` never matches) and ran the **whole** `wyrd-custodian` suite (27 test binaries, including the 5 new legs and the rewritten `segmented_map_reconstruction.rs` leg 2): **everything still passed.** What that mutant does: if a `seg:` record becomes undecodable between the resolve and the move while the root still names it, the pass answers `Satisfied` instead of `Blocked`, which tells the operator redundancy is fine when it is not (C-1). The current code handles this correctly. My attack test (arm `Race::AfterSegmentPage` with bytes `{not a segment record` at `seg:…:1`) gets `Blocked`, the obligation stays queued, the record is byte-identical, the root is untouched and no orphan mark is written. Fix: add that leg to `segmented_map_repoint.rs`, and add a two-obligations-in-one-object variant that checks exactly one `unresolvable` row for `inode:1`. That covers guard (a), "once per OBJECT", on the path that replaced the refusal.; **Leg 1 checks the orphan mark by count only** (`crates/custodian/tests/segmented_map_repoint.rs:420-424`, `orphans(&meta).await.len() == 1`). The rubric's *Absent or unsupported entries* class names count-based assertions, and the T4 batch review raised this twice. The production code is right: I checked, and the single key is `orphan:1:41472:1` = `metadata::orphan_key(LOST, FragmentId { chunk: CHUNK, index: 1 })`. But a bug that marked the survivor or the destination would still pass this assertion. Assert equality on that key instead. `orphan_key` is a base symbol, so the red leg still compiles.; **Comments the patch made false and left in place.** `reconstruction.rs:316` still says "Like the `seg:` refusal", but that refusal is gone. `reconstruction.rs:410-419` still says each commit is "conditioned on the generation THE SCAN returned" and that "a second obligation inside the same object still loses the CAS it always lost". Neither holds for segmented objects any more: the snapshot is now the *resolved* generation, and two owed chunks in the **same** `seg:` record both land in one pass. My attack test got `Changed`, placements `[[0,2],[0,2]]`, an empty queue and 2 orphans, and `segmented_map_reconstruction.rs:490` asserts the same across segments. Also, the new test's module doc (`segmented_map_repoint.rs`, "Legs 3–4 fail on the base only on their `Satisfied` verdict") is wrong for leg 4. On the base, leg 4 fails at `assert!(meta.raced())` (`:357`, "the race never landed"), because the base never commits. Harmless, but the doc should say so.; `crates/custodian/tests/segmented_map_repoint.rs:421`: The success case checks only that one orphan mark exists. It still passes if repair marks the survivor, the rebuilt destination, or another fragment instead of the displaced position. Compare the complete returned key list with the expected orphan key for `(LOST, FragmentId { chunk: CHUNK, index: 1 })`; this proves the identity claimed by the assertion and excludes extra marks.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 71.0% — 44 of 62 instrumentable changed lines executed (below the 80% floor); 62 of 169 changed lines were
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — **The move's store-fault arm is untested, and a mutant there survives the whole `wyrd-custodian` suite.** At `reconstruction.rs:1185`, `Err(err) => return Err(err)` is the diff-cov MISS. I changed it to `return Ok(contained(err.to_string()))` and ran `cargo test -p wyrd-custodian`: every test binary passed. With that mutant, a metadata-store outage during the move's `seg:` read is reported as "object `inode:N` is unreadable" and the pass continues. That is the exact confusion `06-runtime-view.md:29` says a maintenance pass must avoid ("this object is unreadable" vs "the store is failing"). C5's "0 missed" does not cover this: cargo-mutants made no mutant for the arm. Fix: add the move-side twin of `segmented_map_reconstruction.rs:673` (`a_store_fault_under_the_resolver_ends_the_pass_after_the_name_is_out`). Make `MemMeta::get` return `Err` for a `seg:` key, then assert the pass returns `Err`, nothing drains, and no `unresolvable-chunk-map` row names the object.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — **The abort offset on a contained move is not tested.** `crates/custodian/src/reconstruction.rs:448-451`: I deleted `emit_aborted(plan.chunk_id);` at `:450` and ran the whole `wyrd-custodian` suite. Every test binary passed. With that mutant, a move that was contained (torn `seg:` record, version exhausted, …) still counts as a successful repair on the durability plane. The success figure `reconstruction_repaired − conflict − aborted − ceiling_refused` (`:386-393`) goes up by one for a repair that never happened. That is the "silent success" class in the rubric, and the comment at `:444-447` claims the opposite. C5's "0 missed" does not cover this line because cargo-mutants made no mutant for it. Fix: in `torn_under_the_move` (`crates/custodian/tests/segmented_map_repoint.rs:700-738`), assert that `monotonic_counter.reconstruction_aborted` ticks exactly `owed.len()` times. Smaller nit in the same place: the row `emit_aborted` prints (`reconstruction.rs:1425`) still says "could not place the rebuilt shard(s)", which is the wrong reason for a contained move.. 12 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the fix as built; the 4th file (staged.rs one-line cleanup) and the flat-arm containment of corrupt records are accepted. Ceiling-refusal false alarm (item 206) and the DST deferral to #682 are accepted as-is. Two changes for the rebuild: 1. Non-canonical key guard: in reconstruction.rs, contain (report Blocked, do not attempt the move) when a segmented object's scanned key differs from inode_key(id), e.g. `inode:01`, so it no longer answers Satisfied forever with a never-drained obligation and a fragment rewritten each pass. Narrow guard only; do NOT fix parse_inode_key (#698 owns that). Add a test leg: segmented root at `inode:01`, owed chunk -> Blocked, nothing written, obligation still queued. 2. Discharge the docs deferral that names #777: update `docs/design/architecture/06-runtime-view.md` §6.3 and `docs/design/architecture/08-crosscutting-concepts.md` §8.7 to describe the custodian repairing seg:-resident chunks through repoint_chunk, and fix/remove the `deferred: #777` comment at crates/core/src/metadata.rs:3229-3231 ("nothing calls this yet" is now false). The human lifts the 3-file budget and the "metadata.rs untouched" fence for exactly these doc + comment edits.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the fix as built; the 4th file (staged.rs one-line cleanup) and the flat-arm containment of corrupt records are accepted. Ceiling-refusal false alarm (item 206) and the DST deferral to #682 are accepted as-is. Two changes for the rebuild:
  1. Non-canonical key guard: in reconstruction.rs, contain (report Blocked, do not attempt the move) when a segmented object's scanned key differs from inode_key(id), e.g. `inode:01`, so it no longer answers Satisfied forever with a never-drained obligation and a fragment rewritten each pass. Narrow guard only; do NOT fix parse_inode_key (#698 owns that). Add a test leg: segmented root at `inode:01`, owed chunk -> Blocked, nothing written, obligation still queued.
  2. Discharge the docs deferral that names #777: update `docs/design/architecture/06-runtime-view.md` §6.3 and `docs/design/architecture/08-crosscutting-concepts.md` §8.7 to describe the custodian repairing seg:-resident chunks through repoint_chunk, and fix/remove the `deferred: #777` comment at crates/core/src/metadata.rs:3229-3231 ("nothing calls this yet" is now false). The human lifts the 3-file budget and the "metadata.rs untouched" fence for exactly these doc + comment edits.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the fix as built. Approved, do not revisit: the 93.5 KB patch size (the 85 KB budget is lifted for this round's additions), the non-canonical-key guard + leg 12, the docs/metadata.rs deferral discharge, the staged.rs cleanup, and every deferral already settled (#682, #698, #723, ceiling false alarm). Make only these three changes: 1. Add a test leg to crates/custodian/tests/segmented_map_repoint.rs pinning the move to the root the resolver ANSWERED from (reconstruction.rs `prior: resolved.record`): arm Race::AfterSegmentPage, write the same records() under SegmentGroup::new(NONCE, EPOCH + 1) plus a version-2 root naming that group; assert Changed, the new group's seg:…:1 names the rebuilt placement, the old group untouched, queue empty. It must fail when `prior` is mutated to the scanned `record.clone()`. 2. In assert_lost (legs 3 and 4), assert exactly one reconstruction_conflict tick and zero reconstruction_aborted ticks, so a Repoint::Conflict relabelled as Aborted (reconstruction.rs ~:1185) fails a test. 3. Fix docs/design/architecture/06-runtime-view.md §6.3 wording: the move re-reads only a segment record; the flat arm pins its own snapshot. E.g. "…a compare-and-swap on the root generation, and, for a segmented map, on the segment record as the move itself re-reads it, and on the chunk's own reference…". Show both new mutants (wrong `prior`, Conflict→Aborted) as caught in build-notes.md.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the fix as built. Approved, do not revisit: the 93.5 KB patch size (the 85 KB budget is lifted for this round's additions), the non-canonical-key guard + leg 12, the docs/metadata.rs deferral discharge, the staged.rs cleanup, and every deferral already settled (#682, #698, #723, ceiling false alarm). Make only these three changes:
  1. Add a test leg to crates/custodian/tests/segmented_map_repoint.rs pinning the move to the root the resolver ANSWERED from (reconstruction.rs `prior: resolved.record`): arm Race::AfterSegmentPage, write the same records() under SegmentGroup::new(NONCE, EPOCH + 1) plus a version-2 root naming that group; assert Changed, the new group's seg:…:1 names the rebuilt placement, the old group untouched, queue empty. It must fail when `prior` is mutated to the scanned `record.clone()`.
  2. In assert_lost (legs 3 and 4), assert exactly one reconstruction_conflict tick and zero reconstruction_aborted ticks, so a Repoint::Conflict relabelled as Aborted (reconstruction.rs ~:1185) fails a test.
  3. Fix docs/design/architecture/06-runtime-view.md §6.3 wording: the move re-reads only a segment record; the flat arm pins its own snapshot. E.g. "…a compare-and-swap on the root generation, and, for a segmented map, on the segment record as the move itself re-reads it, and on the chunk's own reference…".
  Show both new mutants (wrong `prior`, Conflict→Aborted) as caught in build-notes.md.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
