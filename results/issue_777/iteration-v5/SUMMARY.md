# Result — issue 777 / reconstruction-completes-seg-repair-through-primitive

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **A chunk whose `ChunkRef` lives in a `seg:` record is never repaired.**
  #697 stopped reconstruction aborting on a segmented object but deliberately writes
  nothing: the repair obligation is routed to `Site::Refused`
  (`crates/custodian/src/reconstruction.rs:552`) and answered `Assessment::Refused`
  (`:609`), every pass, forever — the obligation is not drained (data loss) and, until
  #776, nothing could move the placement. With #776 merged the primitive **exists but
  nothing calls it**: a multipart-published object's redundancy still decays untended,
  permanently. That is the C-1 violation this bundle exists to close — see **Invariant to
  restore**.
- Success criterion: the NEW file `crates/custodian/tests/segmented_map_repoint.rs`
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the repair pass stops refusing a `seg:`-resident chunk and completes the move
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (12 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.6% — 69 of 70 instrumentable changed lines executed (floor 80%); 70 of 222 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 16 mutants tested in 2m: 6 caught, 10 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_777/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.06s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #777’s repair of `seg:`-resident chunks through `repoint_chunk`: independent evidence supports the implementation; patch-size approval and fitness-to-purpose sign-off remain owed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required exit from permanent under-replication has observable placement, queue, root-byte and race outcomes; subsequent containment/documentation scope is expressly approved (`brief.md:7`, `brief.md:209`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the new test compiled against the primitive-bearing base and produced 11 failures/12 tests, including both binding repair/merge cases; this is behavioral red, not a missing-symbol failure (`reviewer-red.log:147`, `reviewer-red.log:274`). |
| C3 Change | PASS | Placement, obligation discharge and displaced-fragment evidence remain one conditional batch; the approved noncanonical-key containment preserves the separately owned parser boundary (`crates/custodian/src/reconstruction.rs:659`, `crates/custodian/src/reconstruction.rs:1167`, `crates/custodian/src/reconstruction.rs:1206`). |
| C4 Verification (red→green) | PASS | Restoring the fix gives 12/12 new tests and 258 passing custodian tests; workspace checks and the remaining scans support the frozen green CI result, subject to the rerun qualifications below (`reviewer-green.log:284`, `reviewer-green.log:435`, `gate-logs/C4-ci.log:3831`). |
| C5 Causal adequacy | PASS | The formerly refused repair now reaches the owning record’s write path; independent removal of each named pin/ceiling protection makes its test fail, establishing more than the coarse pre-fix refusal (`crates/custodian/src/reconstruction.rs:1167`, `reviewer-negation-chunk-reference-pin.log:28`, `reviewer-negation-root-generation-pin.log:28`, `reviewer-negation-value-ceiling.log:28`). |
| T1 Structure | PASS | Metadata mutation ownership stays in the existing core primitive, while custodian evidence joins its batch; no parallel writer or new backend dependency is introduced (`crates/core/src/metadata.rs:3240`, `crates/custodian/src/reconstruction.rs:1180`, `crates/custodian/src/reconstruction.rs:1206`). |
| T2 Shape | NEEDS-HUMAN | Approve the remaining byte-budget expansion or require the patch to fit 85 KB — the supplied diff is 93,462 bytes, while the later approval explicitly lifts the file-count/core-comment fences without explicitly lifting this ceiling (`reviewer-shape.log:1`, `brief.md:127`, `brief.md:209`). |
| T3 Runtime | PASS | Repair retains the shared namespace reading and bounded segment-candidate reads; successful moves preserve fragment-before-commit ordering and atomic metadata publication (`crates/custodian/src/reconstruction.rs:614`, `crates/core/src/metadata.rs:3235`, `crates/custodian/src/reconstruction.rs:1200`). |
| T4 Contribution | PASS | All seven affected paths were checked against merged history and all 356 closed/merged PRs; the rejected broad predecessor is accounted for, and living architecture describes the resulting recovery contract (`reviewer-prior-art.log:1`, `docs/design/architecture/06-runtime-view.md:40`, `docs/design/architecture/08-crosscutting-concepts.md:89`). |
| T5 Judgment | PASS | Race injection reaches the intended read/commit windows; exact orphan identity, per-object containment, abort offsets and backend-fault propagation are asserted, addressing the carried-forward test weaknesses (`crates/custodian/tests/segmented_map_repoint.rs:118`, `crates/custodian/tests/segmented_map_repoint.rs:506`, `crates/custodian/tests/segmented_map_repoint.rs:717`, `crates/custodian/tests/segmented_map_repoint.rs:872`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the demonstrated recovery contract for the intended rollout and decide operational follow-up — deterministic seam tests establish the repair invariants, but the Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns were not exercised here (`crates/custodian/tests/segmented_map_repoint.rs:417`, `AGENTS.md:79`, `AGENTS.md:81`). |

No new implementation defect was established. The byte-budget decision above is a scope judgment, not a verification failure. The seven-file scope, narrow segmented-key guard, flat-record containment, conflict-only `Satisfied` outcome, and tracked deferrals #682/#698/#723 remain settled by the brief; they are not reopened (`brief.md:24`, `brief.md:32`, `brief.md:209`). The added guard validates a corrupt stored identity; it is not an optional-capability probe masking a load-time cause.

Source citations above are relative to `$PDCA_TARGET`; evidence citations are relative to this review directory. The disposable target contained the prerequisite primitive. All seven reviewed files retained their original patched hashes after stash/pop; mutation experiments ran only in a separate copy inside this directory. No stale-target qualification is needed.

The evidence supports the verdicts with these limits:

| Gate evidence | Verdict | Independently established basis |
|---|---|---|
| C4-ci | PASS | Local typos, docs lint/render/link audit, fmt, clippy, build, 1,493 workspace test passes and machete completed. The aggregate rerun then hit the shared advisory database’s read-only lock (`reviewer-ci.log:3133`); all three real cargo-deny checks passed after changing only the database storage path (`reviewer-deny.log:15`, `reviewer-deny.log:26`, `reviewer-deny.log:29`). Conformance, statics and DST subsequently passed (`reviewer-ci-remainder.log:2`, `reviewer-ci-remainder.log:7`, `reviewer-ci-remainder.log:599`). The internal deployment guard has no standalone CLI task; its actual pass and the complete aggregate pass are present in the frozen log (`gate-logs/C4-ci.log:3244`, `gate-logs/C4-ci.log:3831`). |
| C4-verify | PASS | Independent stash/pop reproduction yielded 1 pass/11 failures before the fix and 12 passes after it (`reviewer-red.log:274`, `reviewer-green.log:284`). Leg 5 is green on the base; the conflict legs’ coarse red does not by itself prove their pins. |
| C4-diff-cov | PASS | The frozen coverage run shows 69/70 instrumentable changed lines executed, 98.6%; its sole miss is the generic `Aborted` dispatch at reconstruction line 443. This is captured coverage evidence, not an independently repeated coverage measurement (`gate-logs/C4-diff-cov.log:482`, `gate-logs/C4-diff-cov.log:486`). |
| C5-mutants | PASS | Independent cargo-mutants reproduced 16 tested: 6 caught, 10 unviable; unviable mutations establish no semantic protection (`reviewer-mutants-rerun.log:5`). Separate executable negations of the chunk-reference pin, root-generation pin and ceiling each compiled and failed the corresponding test (`reviewer-negation-chunk-reference-pin.log:28`, `reviewer-negation-root-generation-pin.log:28`, `reviewer-negation-value-ceiling.log:28`). |
| T4-batch-review | PASS | The captured batch review reports zero blocking findings; this review independently examined the patch and does not treat that count as correctness proof (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Contribution artifacts are intentionally drafted after Check; the substantive audit must rerun at publish. No missing-evidence escalation is warranted (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | The frozen log shows both requested feature-enabled clippy compilations finished successfully; no live TiKV service exercise is claimed (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). |

The listed external tool dependencies were exercised, including the real renderer and scanners; the local advisory-lock failure was a host permission issue resolved by relocating its database, not a substituted tool or a patch defect. Tier-1/Tier-2 observation is warranted for this durability change under the standing rubric; it complements the accepted #682 DST follow-up rather than reopening that deferral.

The affected-path prior-art investigation also supersedes the brief’s old “no open PRs” statement: three are now open. Only #847 shares an affected file, and its multipart nonce hunks do not overlap this patch’s `repoint_chunk` documentation hunk (`reviewer-prior-art.log:26`, `reviewer-prior-art-847-metadata.diff:1`). Closed PR #647 was rejected for breadth and asked to be split; this patch uses the separately merged primitive and the approved reconstruction scope (`reviewer-prior-art.log:23`).

### Advisory — adversary

# Adversarial review — issue #777 (seg:-resident repair through `repoint_chunk`)

Verdict: **could not refute the fix.** I found no input that makes production write a wrong
placement, drain an obligation over a hole, or certify a pass it should not. What I did find
are two changed lines that no test pins: hand-made mutants of them survive the whole
`wyrd-custodian` suite. Both are cheap test additions. Neither is a safety bug.

## Findings

- NEEDS-HUMAN [impl] — **The choice of which root the move is pinned to is untested.**
  `crates/custodian/src/reconstruction.rs:702` sets `prior: resolved.record.as_ref().clone()`.
  The comment at `:648-651` gives the reason: the move must be conditioned on the generation
  the resolver *answered from*, which is the live root when a supersede during the resolve
  made it restart. I changed it to `prior: record.clone()` (the scanned, retired root) and
  ran `cargo test -p wyrd-custodian`: **every test binary passed.** Concrete case: arm
  `Race::AfterSegmentPage` with the same `records()` written under a new group
  `SegmentGroup::new(NONCE, EPOCH + 1)` plus a version-2 root naming that group. The
  resolver sees the group change and restarts onto the new root. Production answers
  `Changed`, the new group's `seg:…:1` names `[[0,2],[0,1]]`, the old group is untouched,
  and the queue is empty. That is correct, and I checked it. The mutant answers
  `Satisfied` with `CHUNK` still queued and nothing repointed, because it pins the retired
  root and loses the CAS. This only costs liveness (one lost pass per supersede, never a
  bad write, because the root pin still fails closed). But the property the comment claims
  has no test. Fix: add this leg to `crates/custodian/tests/segmented_map_repoint.rs`. It
  uses only base symbols and the existing `MemMeta` race hook.

- NEEDS-HUMAN [impl] — **A prepare-time conflict can be relabelled as an abort and no test
  notices.** At `crates/custodian/src/reconstruction.rs:1185`,
  `Ok(metadata::Repoint::Conflict) => return Ok(RepairOutcome::Conflict)`. When I mutated it
  to `RepairOutcome::Aborted`, the whole suite still passed. With that mutant, leg 3
  (`segmented_map_repoint.rs:554`) ticks `reconstruction_aborted` with
  `reason:"unplaced"` instead of `reconstruction_conflict`. That tells the operator "no
  server to place on" for what was a lost race. The verdict and the store do not change.
  `assert_lost` (`:442`) checks the store and the verdict but no counter. Fix: in
  `assert_lost`, assert one `reconstruction_conflict` tick and zero `reconstruction_aborted`
  ticks (legs 3 and 4 both go through it).

- NEEDS-HUMAN [impl] — **A docs sentence is wrong for the flat arm.** At
  `docs/design/architecture/06-runtime-view.md:40`: "a compare-and-swap on the root
  generation, on that record as the move itself re-reads it". The flat arm of
  `repoint_chunk` (`crates/core/src/metadata.rs:3259-3276`) re-reads nothing. It pins
  `encode(generation)` from the pass's own snapshot. Only the segmented arm re-reads (its
  `seg:` record). Suggested wording: "…and, for a segmented map, on the segment record as
  the move itself re-reads it…". A wording fix only.

## The gate evidence and the reviewer's verdict

- **C5 "pass" (16 mutants: 6 caught, 10 unviable) says little about test strength.**
  `cargo mutants` made almost no mutants for the new match arms, the `read_committed` guards
  or the `prior` choice. Earlier rounds (carry-forward items 1–3 in `brief.md`) were sent
  back for exactly this, and the two survivors above are the same class. Of my other hand
  mutants, all were caught: dropping the once-per-object dedupe in `Reading::contain`
  (`:539`), disabling the canonical-key guard (`:659`), skipping `(None, None)` (`:667`),
  containing without naming (`:449`), and mapping the move's `ChunkMapError` to `Conflict`
  (`:1194-1195`).
- **C4-verify red→green holds up.** I re-ran the new file on the target: 12/12 green. The
  frozen red log matches the base: `refused-segmented` rows, `Blocked`, the base's
  unchecked `version + 1` overflow panic at the base's `:1157`, and "fault never fired" for
  leg 11, since the base issues no `get` on a `seg:` key. Every leg goes through
  `reconcile_step` and checks the store. The test names no symbol this patch adds. The race
  timing in leg 3 really discriminates: if the racing write had landed before the resolve,
  the plan would be built from `[0,7]` and the pass would answer `Changed`, not
  `Satisfied`. Leg 11 cannot pass for the wrong reason, because the resolver code is the
  base's and the base never `get`s a `seg:` key.
- Diff-cov's one MISS (`reconstruction.rs:443`, the `Aborted` arm) is a base arm with a new
  reason string. Not worth a round.

## Attempted and could not refute

- **Addressing.** Segments are validated at decode to tile the object contiguously and
  are never empty (`metadata.rs:1385`, `SegmentedMap::new`). So the summed `byte_offset`
  built at `reconstruction.rs:677-680` always matches the primitive's
  `segment_may_hold` + `chunk_at` lookup. I found no valid, stable object that conflicts
  forever at prepare time.
- **Root CAS identity.** The segmented root's group, nonce and table go through strict,
  validating serde (`metadata.rs:987-1071`). The segment pin uses the raw re-read bytes
  (`:3324`). So a segmented move opens no new decode→encode mismatch. (The flat
  `placement` `#[serde(default)]` without `skip_serializing_if` at `:360` is a base
  behaviour of the flat arm, not this diff.)
- **Drain over a hole.** A `Contained` move sets `reading.incomplete` inside the repair loop
  (`:448-451`), before the drain gate at `:477`. So an object found unusable at the move
  never lets `drain_only` discard an obligation.
- **Ceiling boundary.** The writer and the resolver both use `> MAX_VALUE_BYTES`
  (`metadata.rs:606`, `:2887`, `:3295`), so the move never writes a record the read side
  then refuses.
- **Store fault versus object damage.** A non-`ChunkMapError` from the move ends the pass
  (`:1196`). Leg 11 pins this.
- Out of scope or settled, so not raised: the flat noncanonical key (#698); the stranded
  destination fragment (#723); the DST property (`// deferred: #682` at `:1165`, accepted
  at sign-off); the ceiling-refusal false alarm (accepted at sign-off); duplicate chunk ids
  (#700).

### Advisory — code-review

- No correctness findings introduced by this diff. Reviewed offset and generation handling (`crates/custodian/src/reconstruction.rs:648`), move-time containment (`crates/custodian/src/reconstruction.rs:448`), and atomic placement/obligation/orphan composition (`crates/custodian/src/reconstruction.rs:1167`). The regression fixtures exercise sibling and planned-chunk races, root supersession, malformed records, and backend faults (`crates/custodian/tests/segmented_map_repoint.rs:530`).
- No actionable reuse, simplification, or efficiency findings. Reconstruction delegates placement mutation and ceiling checks to the existing `repoint_chunk` primitive while sharing one object snapshot across its obligations (`crates/custodian/src/reconstruction.rs:699`, `crates/custodian/src/reconstruction.rs:1167`).

Validation: reviewed the supplied frozen gate logs; CI passed, all 12 new tests passed, and mutation testing reported 6 caught and 10 unviable mutants. No builds or tests were rerun; the target remained read-only. Settled deferrals and accepted scope decisions were respected.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T2 Shape — Approve the remaining byte-budget expansion or require the patch to fit 85 KB — the supplied diff is 93,462 bytes, while the later approval explicitly lifts the file-count/core-comment fences without explicitly lifting this ceiling (`reviewer-shape.log:1`, `brief.md:127`, `brief.md:209`).
- [ ] Validation — fitness-to-purpose — Accept the demonstrated recovery contract for the intended rollout and decide operational follow-up — deterministic seam tests establish the repair invariants, but the Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns were not exercised here (`crates/custodian/tests/segmented_map_repoint.rs:417`, `AGENTS.md:79`, `AGENTS.md:81`).
- [ ] **The choice of which root the move is pinned to is untested.** `crates/custodian/src/reconstruction.rs:702` sets `prior: resolved.record.as_ref().clone()`. The comment at `:648-651` gives the reason: the move must be conditioned on the generation the resolver *answered from*, which is the live root when a supersede during the resolve made it restart. I changed it to `prior: record.clone()` (the scanned, retired root) and ran `cargo test -p wyrd-custodian`: **every test binary passed.** Concrete case: arm `Race::AfterSegmentPage` with the same `records()` written under a new group `SegmentGroup::new(NONCE, EPOCH + 1)` plus a version-2 root naming that group. The resolver sees the group change and restarts onto the new root. Production answers `Changed`, the new group's `seg:…:1` names `[[0,2],[0,1]]`, the old group is untouched, and the queue is empty. That is correct, and I checked it. The mutant answers `Satisfied` with `CHUNK` still queued and nothing repointed, because it pins the retired root and loses the CAS. This only costs liveness (one lost pass per supersede, never a bad write, because the root pin still fails closed). But the property the comment claims has no test. Fix: add this leg to `crates/custodian/tests/segmented_map_repoint.rs`. It uses only base symbols and the existing `MemMeta` race hook.
- [ ] **A prepare-time conflict can be relabelled as an abort and no test notices.** At `crates/custodian/src/reconstruction.rs:1185`, `Ok(metadata::Repoint::Conflict) => return Ok(RepairOutcome::Conflict)`. When I mutated it to `RepairOutcome::Aborted`, the whole suite still passed. With that mutant, leg 3 (`segmented_map_repoint.rs:554`) ticks `reconstruction_aborted` with `reason:"unplaced"` instead of `reconstruction_conflict`. That tells the operator "no server to place on" for what was a lost race. The verdict and the store do not change. `assert_lost` (`:442`) checks the store and the verdict but no counter. Fix: in `assert_lost`, assert one `reconstruction_conflict` tick and zero `reconstruction_aborted` ticks (legs 3 and 4 both go through it).
- [ ] **A docs sentence is wrong for the flat arm.** At `docs/design/architecture/06-runtime-view.md:40`: "a compare-and-swap on the root generation, on that record as the move itself re-reads it". The flat arm of `repoint_chunk` (`crates/core/src/metadata.rs:3259-3276`) re-reads nothing. It pins `encode(generation)` from the pass's own snapshot. Only the segmented arm re-reads (its `seg:` record). Suggested wording: "…and, for a segmented map, on the segment record as the move itself re-reads it…". A wording fix only.
- [x] **The docs-currency deferral lands on this issue, and this diff drops it.** `crates/core/src/metadata.rs:3238-3240` reads: *"deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777."* This diff *is* that wiring. It touches no doc, and the brief's 3-file budget and its fence on `metadata.rs` rule out both the doc update and a fix to the marker, whose "nothing calls this yet" is now false. After merge, the deferral points at a closed issue. The rubric makes docs currency a merge requirement, and its protocol says to raise the tracking issue when a deferral looks wrong. Here the tracking issue is this one. A human decides: add the doc update to this PR, or file a new issue and re-point the marker.
- [x] **A segmented object under a non-canonical key (`inode:01`) now answers `Satisfied` every pass while its obligation never drains. The base answered `Blocked` and named the object.** `reconstruction.rs:651-652` takes `parse_inode_key("inode:01") = 1`. The move then pins `inode:1` (`core/src/metadata.rs:3257-3258`), which does not exist, so the CAS (compare-and-swap) fails every pass. `RepairOutcome::Conflict` is not a hole (`reconstruction.rs:490`), so the verdict is `Satisfied`. I probed this with `seed(&meta, b"inode:01", &records())` plus `owe(CHUNK)` over 3 passes:
- [x] **#776's docs deferral points at this issue, and this patch does not discharge it.** `core/src/metadata.rs:3238-3240` says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". After this merges, "nothing calls this yet" is false, and the docs-currency duty has no owner. The brief makes `metadata.rs` off-limits and caps the patch at 3 files, so the builder cannot fix this. A human should allow the doc and marker edit here or re-defer both to a named issue. I found no doc sentence that this change makes false outright; §6.3's "single atomic metadata mutation" still holds.
- [x] **The #698 deferral now covers segmented objects, and the verdict got worse.** Concrete case: a segmented root stored at the non-canonical key `inode:01`, owed `CHUNK`. I ran 3 passes on each tree. Base: `Blocked` every pass, nothing written. Patched: `parse_inode_key` turns `inode:01` into 1 (`reconstruction.rs:651-652`), and the move pins `inode:1` (`crates/core/src/metadata.rs:3257-3258`), which does not exist. So the commit loses with `Conflict`, and the pass answers **`Satisfied`** every pass. The obligation stays queued forever, and a rebuilt fragment is rewritten to the free server each pass (the #723 leak). The brief gives non-canonical keys to #698, so under the rubric's deferral rule this does not block the patch. But #698 should record two things: segmented objects now hit it too, and their verdict changed from `Blocked` to `Satisfied`. An object stuck forever (the C-1 case) now sits behind a clean verdict.
- [x] **A docs deferral that points at this issue is now due and is not paid.** `crates/core/src/metadata.rs:3238-3240` (from #776) says: "deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3, `08-crosscutting-concepts.md` §8.7) … nothing calls this yet. It moves with the custodian wiring in #777". This patch is that wiring and touches no doc. The brief keeps `metadata.rs` untouched and its 3-file budget has no room for docs, so the marker's "nothing calls this yet" is now false and stays false. A human should either budget the doc update here or point the deferral at a new issue.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Keep the fix as built. Approved, do not revisit: the 93.5 KB patch size (the 85 KB budget is lifted for this round's additions), the non-canonical-key guard + leg 12, the docs/metadata.rs deferral discharge, the staged.rs cleanup, and every deferral already settled (#682, #698, #723, ceiling false alarm). Make only these three changes: 1. Add a test leg to crates/custodian/tests/segmented_map_repoint.rs pinning the move to the root the resolver ANSWERED from (reconstruction.rs `prior: resolved.record`): arm Race::AfterSegmentPage, write the same records() under SegmentGroup::new(NONCE, EPOCH + 1) plus a version-2 root naming that group; assert Changed, the new group's seg:…:1 names the rebuilt placement, the old group untouched, queue empty. It must fail when `prior` is mutated to the scanned `record.clone()`. 2. In assert_lost (legs 3 and 4), assert exactly one reconstruction_conflict tick and zero reconstruction_aborted ticks, so a Repoint::Conflict relabelled as Aborted (reconstruction.rs ~:1185) fails a test. 3. Fix docs/design/architecture/06-runtime-view.md §6.3 wording: the move re-reads only a segment record; the flat arm pins its own snapshot. E.g. "…a compare-and-swap on the root generation, and, for a segmented map, on the segment record as the move itself re-reads it, and on the chunk's own reference…". Show both new mutants (wrong `prior`, Conflict→Aborted) as caught in build-notes.md.
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
