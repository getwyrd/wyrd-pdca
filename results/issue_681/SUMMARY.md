# Result — issue 681 / passes-read-through-resolver-contained

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The three maintenance passes that walk the committed namespace **themselves** —
  reconstruction, backfill, rebalance — still read the chunk map inline out of the inode record,
  so a **single** segmented object aborts the whole pass for **every** object; and an object whose
  map cannot be read for any other reason ends the walk with an `Err` instead of being contained.
  Seven sites, each re-verified on `origin/main @ 339da46` at this Plan, and they partition
  **exactly by file** — which is what makes the by-pass split clean:

  | Site | Function | Pass |
  |---|---|---|
  | `crates/custodian/src/backfill.rs:99` | `reconcile` | backfill |
  | `crates/custodian/src/backfill.rs:181` | `emit_remaining` | backfill |
  | `crates/custodian/src/rebalance.rs:162` | `plan_evacuations` | rebalance |
  | `crates/custodian/src/rebalance.rs:259` | `evacuate_chunk` | rebalance |
  | `crates/custodian/src/reconstruction.rs:332` | `assess` | reconstruction |
  | `crates/custodian/src/reconstruction.rs:583` | `repair_chunk` | reconstruction |
  | `crates/custodian/src/reconstruction.rs:636` | `find_chunk` | reconstruction |

  Each is `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported { .. })?`.
  Four live consequences on the base:
  1. **One segmented object disables repair, backfill and drain for the entire store** — these are
     the passes that *restore redundancy*, so a store that has published a single multipart object
     stops self-healing.
  2. **A repair obligation whose chunk lives in a `seg:` record is drained as if the chunk were
     deleted** (`reconstruction.rs:322-325` → `Assessment::Drain`, deleted at `:270-276`). Latent
     today (the error fires first), but it is the loss this work must not introduce while removing
     the abort.
  3. **The deployed repair loop is Q namespace scans × N point reads** — `reconcile` calls `assess`
     per obligation (`:185`), `assess` calls `find_chunk` (`:322`) which scans all of `inode:`.
     Wiring the resolver in naively makes each of those N objects also cost a bounded `seg:` range
     read. This is the finding still open at #647's close.
  4. **Containment is not per object.** #650 and #651 contain a damaged record in GC/scrub
     (`gc.rs:360-455`) and restore/drain (`restore.rs:616-688`); these three passes have no such
     rule — a record that will not `decode` ends the walk before any resolver is involved.
- Success criterion: **this bundle ships no code.** It is satisfied when `split-proposal.md`
  has been accepted, three child issues exist as sub-issues of #681, and each child bundle carries
  its own `brief.md`. The shippable criteria live in the children.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: Wyrd has no maintenance
  branches; M4's integration branch is merged and deleted; #648–#652 all landed on `main` directly.)
- Scope (one logical fix) / out of scope: **split this bundle into three children, one per pass**, each carrying the complete
  rule set for its own pass and its own new test file:

  1. **backfill** — `crates/custodian/src/backfill.rs` (2 sites),
     `crates/custodian/tests/segmented_map_backfill.rs` (new).
  2. **rebalance** — `crates/custodian/src/rebalance.rs` (2 sites),
     `crates/custodian/tests/segmented_map_rebalance.rs` (new).
  3. **reconstruction** — `crates/custodian/src/reconstruction.rs` (3 sites),
     `crates/custodian/tests/segmented_map_reconstruction.rs` (new). Keeps the Q×N→O(N)
     restructure, the duplicate-`ChunkId` rule, and the drain-vs-refuse loss rule.

  **Why by pass and not by property** (the sign-off's hint was by property — this is the
  considered departure, agreed with the human at this Plan): a by-property cut has every child
  editing all three files, so the children serialise into three waves *and* conflict; and
  properties 1 and 2 are not separable — the moment a child calls `resolve_chunk_map` it must
  already decide what to do with a typed read fault, so the second child would rewrite the first's
  error handling. By pass, the children are disjoint, parallel, and each is independently
  reviewable. The measured v7 patch supports this: production was **380 semantic lines across
  three independent files with no shared module** (`backfill` 100, `rebalance` 100,
  `reconstruction` 180), while the single shared test file was **500 semantic lines — 68% of the
  patch** — precisely because every leg had to drive all three passes over one store. Splitting by
  pass removes that pressure entirely. Fixture duplication across the children is **the house
  pattern, not a new cost**: `git grep "struct MemMeta" -- crates/custodian/tests/` finds **twelve**
  independent definitions today.

  **Rules every child carries (settled here; no Do round and no review round relitigates them).**

  **Rule A — a pass acts only on a resolve that did not restart.** *(This REPLACES the old
  decision 4, which claimed the path was "unreachable by construction" and forbade any test from
  binding it. That claim is false and was demonstrated false with a working double at
  `iteration-v4/check-advisory-adversary.md:25`; because nothing bound it, four consecutive review
  rounds rediscovered it, and round 7's two T4 blockers — `rebalance.rs:412`,
  `reconstruction.rs:659` — are the same fact in a third form.)* `resolve_chunk_map` restarts onto
  the live root when the caller's **segmented** snapshot was superseded (`metadata.rs:2338-2339`),
  and then answers a `ResolvedChunkMap` whose `record` is a generation the pass never scanned.
  Mixing that generation's chunk list with the snapshot's `prior_bytes` is what makes unchecked
  indexing panic before the stale CAS can reject the plan. Therefore: **if the resolve did not
  answer from the generation the pass scanned** (detectable as `Cow::Owned`, or
  `resolved.record.version != snapshot.version`), the object changed under the scan — contain it,
  **keep the obligation queued**, answer `Blocked`, re-assess next pass. Bounded per C-1: nothing
  is discarded, the next pass re-reads. **Every child MUST bind this with a test leg** (a metadata
  double whose `scan` answers a stale segmented root while `get` answers a live flat one; the pass
  must write nothing). A rule bound by no test is a rule the next reviewer re-opens.

  **Rule B — an incomplete reading changes what a pass may CLAIM and what it may DISCARD, never
  what it may DO for the objects it read successfully.** A pass that met an unreadable object
  answers `Blocked` (never `Satisfied`) and never drains an obligation, but still repairs, fills
  and evacuates the healthy objects in the same store. Verified safe at this Plan rather than
  argued: these passes **orphan-mark, never delete**, and GC reclaims a marked fragment only past
  `ReferenceSet::protection`, which returns `incomplete-reference-set` and withholds **every**
  fragment in the fleet while any object is unresolvable (`gc.rs:306-316`, consulted before every
  delete at `:191-194`). So the loss chain cannot close and strictness buys **no** safety — while
  costing every healthy object its repair, which is the C-1 violation this work exists to remove.
  Weight for the next reader: a genuinely corrupt root has **no repair path at all** (a fragment
  carries only `FragmentId { chunk, index }`, `crates/traits/src/lib.rs:45-48`, so chunks cannot be
  walked back to their object) and no operator tooling (**#694**), so "until a human fixes it" is
  an unbounded window — strictness would halt fleet-wide redundancy repair with no supported way
  to un-halt it.

  **Rule C — a record is read, written and named under exactly the key the store gave it.** A key
  that is not the canonical spelling of its id is never silently reinterpreted. On the base all
  three passes parse the scanned key to an `InodeId` then re-derive `metadata::inode_key(id)` for
  the CAS (`backfill.rs:142`, `rebalance.rs:310`, `reconstruction.rs:598`); `"inode:007"` and
  `"inode:+3"` both parse, so the pass reads one record and CASes another. `gc.rs:280-294`, `:402`
  is the precedent — resolve against `&key`, key attribution by the raw bytes.

  **Rule D — a refusal is reported once per object, not once per chunk.**

  **Rule E — attribution for an object the pass could not read is emitted where the object is
  read, before the work loop** (mirroring `gc.rs:164-166`), so a later transient store fault
  cannot cost the operator the name of the record to repair. **Load-bearing, not logging
  hygiene:** with no repair tooling (#694) and reclamation halted store-wide, that name is the
  operator's entire situational awareness — it is what distinguishes a rolling-upgrade artefact
  that will clear itself from permanent loss. Do not let a child optimise it away.

  **Constraints (they bound the shape; they do not name it):**
  - **Bounded memory** — work proportional to the obligations held and to one object at a time;
    never the whole namespace's decoded chunk lists, never any segment's exact bytes.
  - **Bounded work** — one resolving reading of the namespace per pass.
  - **Containment on *any* read fault**, by exactly gc.rs's downcast rule: `Ok(ChunkMapError)` is
    contained as *this record's* fault; any other error propagates, because a store fault is not
    one object's.

  **/ out of scope:**
  - **Any write to a segmented record** — `repoint_chunk`, the record ceilings, and the
    repair/evacuation write path for a `seg:`-resident chunk are **#682**. A refusal writes
    **nothing at all**.
  - Restore / `desired_state` (**#651**, merged) — `restore.rs`, `desired_state.rs` untouched;
    **leave** the deferral marker at `restore.rs:616`.
  - `gc.rs` / `scrub.rs` (**#650**, merged) — untouched. Sharing ONE namespace walk across all
    loops is a separate refactor.
  - The chunk-id floor (**#652**); the committer, fence, rollback and resume (**#653**).
  - **The operator surface for an unreadable record — #694, milestone M8.** No child touches it.
  - **No docs edit** (checked: `06-runtime-view.md` §6.2 already states the containment rule
    fleet-wide and stays true); no new or edited ADR / spec / proposal; no conformance-vector
    change; no `Cargo.toml` change (every dev-dependency the discriminators need is already
    declared on `crates/custodian`).

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

### Advisory — adversary

# Adversarial review — issue_681 (advisory, non-gating)

Bundle ships **no code** (disposition `split`), so there is no red→green to re-run: the artefact
under attack is the accepted split and the claim that closing #681 leaves nothing behind. Every
citation below is grounded on `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`, worktree at the
Plan base `339da46`; post-child state read via `git show origin/main:` — `origin/main` is now
`9dbcd72`, i.e. the three children have already merged).

## Refutations

- **NEEDS-HUMAN [human] — the parent's "settled, no round relitigates them" rule set did not
  survive the split, and the close comment claims otherwise.** `brief.md:113` pins Rules A–E as
  *"settled here; no Do round and no review round relitigates them"* and `brief.md:128` makes Rule A
  a MUST-bind for **every** child. In fact **Rule A and Rule C were removed from all three children**
  at their own re-plans and re-tracked as **#698** (OPEN — *"backfill reads a record at one inode key
  and CASes it at another"*) and **#699** (OPEN). This is verifiable in the shipped code, not just in
  paperwork: the Rule C construct the brief called a defect at `crates/custodian/src/backfill.rs:142`
  (base) is **byte-for-byte still there** after #695 — `parse_inode_key` still accepts a
  non-canonical spelling (`git show origin/main:crates/custodian/src/backfill.rs`, `:70-76`;
  `"007".parse()` and `"+3".parse()` both succeed) and the CAS is still re-derived at `:249`
  (`let inode_key = metadata::inode_key(inode_id);` → `.require(inode_key.clone(), encode(&record))`
  at `:251`). Concrete failing case, unchanged from the base: two committed records under
  `inode:007` and `inode:7` whose bytes are equal → the pass reads `inode:007`, CASes `inode:7`,
  emits `backfilled(inode = 7)`, decrements `remaining` for a fill that never landed on the record it
  read. The rubric's *"deferrals are settled"* covers the deferral itself, so this is **not** a
  request to re-open #698/#699 — it is that #681's closing comment states *"Nothing of the original
  scope sits outside those three seams, so there is no remaining work here"*, which is unwarranted:
  #698, #699 and #707 (*"backfill silently skips a committed record whose inode key will not
  parse — after #695 it lands on neither gauge and the pass still certifies"*) are all open and all
  descend from this scope. The human should decide whether the close text is amended or the residue
  is explicitly accepted; the split itself is not thereby refuted.

- **NEEDS-HUMAN [human] — the split created one seam that no issue owns, and shipped an
  operator-facing message that misroutes it.** `brief.md:172-174` sends *"the repair/evacuation write
  path for a `seg:`-resident chunk"* to #682, and the merged backfill child accordingly carries four
  `#682` markers — `git show origin/main:crates/custodian/src/backfill.rs` `:112`, `:217`, `:347`,
  and the **audit string** at `:367` (*"the segmented write path is #682"*). But #682 explicitly
  disowns it: `results/issue_682/brief.md:234-237` — *"`backfill.rs` (#695 — this slice does not
  touch it … a backfill fill that would cross the ceiling is a real gap, but it is a different write
  path and belongs to whichever slice owns it next)"* — and its budget makes editing `backfill.rs` a
  STOP; its two open children #710/#711 are rebalance/reconstruction only. So the concrete state is:
  a committed object whose chunks live in `seg:` records **and** carries an empty placement makes
  `backfill::reconcile` answer `Reconciled::Blocked` on **every** pass forever
  (`origin/main:crates/custodian/src/backfill.rs:216-222` → `incomplete += 1` → `:275-289`), and the
  operator is told to wait for #682, which will never do it. Honest weighting: this is **latent**
  today — no producer of segmented maps exists yet (#653 owns the committer), so the two conditions
  cannot co-occur in a live store — and it is a decline, not a loss, so C-1's data-loss arm is not
  breached. It is nonetheless a permanent state with no owner, and the rubric's reviewer protocol
  (`AGENTS.md:200-203`) says to raise the tracking issue when the deferral itself looks wrong. That
  is a scope/ownership decision, not something a Do round on #681 can fix.

- **The gate row that says nothing.** `check-gates.json:3` records `"overall": "pass"` while all
  eleven rows are `"none"` with `"N/A — close disposition (no patch to verify)"`, and
  `check-review.md` records the reviewer leaf as SKIPPED. Nothing in the Check tested this bundle's
  *actual* success criterion (`brief.md:56-58`). I tested it by hand and it **holds** — so this is a
  note, not a refutation: GitHub `subIssues` of #681 = {695, 696, 697} (all CLOSED/merged);
  `results/issue_{695,696,697}/brief.md` all exist; the target worktree is clean at `339da46` and the
  bundle carries no `patch.diff`, so "ships no code" is true. The `pass` is vacuous evidence, but the
  criterion behind it is satisfied.

## Attempted and could not refute

- **The seven-site table (`brief.md:32-38`) is exact and complete.** On the base, `SegmentedMapUnsupported`
  occurs in `crates/custodian/src/` at precisely `backfill.rs:99`, `:181`, `rebalance.rs:162`, `:259`,
  `reconstruction.rs:332`, `:583`, `:636` — no eighth site was missed, and no other `chunk_map` read
  in those three files bypasses `as_flat()`. On `origin/main` every one of them now goes through
  `metadata::resolve_chunk_map` (backfill `:156`, rebalance `:256`, reconstruction `:474`) with the
  `Ok(None)` / downcast arms mirroring `gc.rs:402-416`. The stated defect really is gone.
- **The disjointness / one-parallel-wave claim (`brief.md:83-85`).** `git diff --stat
  339da46..origin/main` over the three children touches exactly `src/{backfill,rebalance,reconstruction}.rs`
  plus three **new** `tests/segmented_map_*.rs` — no shared module, no `crates/custodian/src/lib.rs`,
  no `crates/core/src/metadata.rs`, no `Cargo.toml`. I looked specifically for a forced shared edit
  (the `Reconciled::Blocked` fold in `reconciliation.rs:44`, `emit_remaining`'s second namespace walk
  at `backfill.rs:156`/`:171` needing a caller change) — both are containable inside one file.
- **The fixture-duplication justification (`brief.md:110-111`).** `struct MemMeta` really is defined
  in **twelve** independent files under `crates/custodian/tests/` on the base. The number is not
  inflated.
- **A sibling in flight colliding with Rule C's surface.** #691 (`d986069`, merged between the base
  and the children) adds validated identity types and fail-closed key parsing — exactly the Rule C
  surface — but it is additive and *"nothing in this module is consumed by production code yet"*, so
  it neither conflicts with nor silently satisfies the children.
- **The #682 repointing the brief demanded (`brief.md:18`).** `results/issue_682/brief.md:159` now
  reads `Depends on: 696, 697`; the drop of #695 is argued at `:7-9` rather than overlooked. (The
  parent's "#682 depends on all three" was over-broad, and the downstream correction is what exposed
  the ownership gap in the second finding above.)

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered
- [x] **The brief's decision-4 "unreachable by construction" claim is false, and

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
- By / date: Eduard Ralph / 2026-08-08

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
