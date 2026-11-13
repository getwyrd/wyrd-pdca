# Result — issue 1062 / multipart-retirement-drain-marks

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1, `retire:records:` obligations drain, but `retire:bytes:` ones never
  do: Abort's `{session}` / `{session, all}` / `{session, parts}`, the root flip's unnamed parts
  `{parts}`, a losing writer's `{chunks}`, a superseded generation's `{generation}`. Their bytes
  stay protected for ever (GC's keyed protection, `crates/custodian/src/gc.rs:743-762`) and their
  records never leave. #1012's first attempt drained them, and Check found two defects: (1) every
  mark carried the pass's `now_millis`, read before the reconstruction loop ran, so an obligation
  installed during that loop got marks dated before it existed, and GC could reclaim them with less
  than the full grace, tearing a reader that overlapped the supersede; (2) the hold for a
  `{session}` obligation whose session still owns `sidx:` entries had no test: removing it deleted
  the obligation, left the `sidx:` entry with nothing to drain it, and every test still passed.
- Success criterion: A NEW test file `crates/custodian/tests/multipart_retirement_marks.rs`,
  over `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving
  the drain **only through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`). It shows all of:
  **(B) Complete, then drain.** After a production subset Complete (#1008) and reconcile passes:
  **no** `orphan:` mark names a chunk of the published map (`0016:1503`); every fragment of the
  unnamed part is marked **before** its `part:` record is deleted (a recording wrapper shows the
  order); after passes past the orphan grace its fragments are gone from the chunk store; no
  `part:` or `psum:` record of the session remains.
  **(T) Stamped no earlier than the retirement.** With a test clock: a store wrapper that, while
  the pass's reconstruction loop runs, advances the clock past the orphan grace and installs a
  `retire:bytes:` obligation. Every mark the drain writes for it carries `orphaned_at_millis` at
  or after the clock reading at installation, and a GC pass straight after reclaims none of its
  fragments.
  **(C′) Mark guard, the `reclaiming` check first.** For a position the obligation names that
  already carries a mark: **(0)** a mark that decodes as `reclaiming` is never replaced, whatever
  event it carries (`crates/core/src/metadata.rs:154-168`: "No writer overwrites a `reclaiming`
  mark … the three-arm drain (#659) … inherits this rule"). GC deletes the key blind after
  destroying the fragment (`gc.rs:804-825`), so a value written over it would vanish with it. The
  fixture uses a position **no record the drain retains names**: a `{chunks}` obligation (or a
  flat `{generation}`), whose installing batch removed the naming record. It plants a
  `reclaiming` mark carrying a different event there, runs a step, and asserts the stored value
  byte-identical and the obligation still present with its cursor at or before that position;
  then runs **real GC passes** (the deployed pass, not a hand-made key delete), which destroy the
  fragment and delete the key themselves (`gc.rs:673-678`, `:804-825`); runs the next step; and
  asserts a fresh structured mark of this event (arm c), and the obligation removed once its last
  position is marked. Then the three arms (`0016:1220-1226`): (a) same event ⇒ skipped,
  byte-identical; (b) different event ⇒ re-stamped under `require(== prior)` with this event and
  a fresh stamp; (c) absent ⇒ written under `require_absent`.
  **(H) `reclaiming` over a record the drain retains: a stated hold, not a wait on GC.** A
  `reclaiming` mark on a position that a `part:` record under this obligation still names (the
  drain keeps that record until the position carries this event's mark). No in-tree path makes
  this state: GC swaps a mark to `reclaiming` only on a position nothing named when it looked
  (`metadata.rs:165-168`), and a staged position is named by its `sidx:` entry before its
  fragment is written (`crates/core/src/multipart/stage.rs:22-24`). It comes only from damage or a
  store edit, and GC's own comment allows for it (`gc.rs:660-662`), so the fixture plants it. In
  that state GC never moves: the staged protection outranks the `reclaiming` mark
  (`gc.rs:654-668`) and the fragment-less sweep keeps a staged-protected key (`gc.rs:887-893`).
  Alternating drain steps with real GC passes over it, every pass: the fragment, the mark, the
  `part:` record and the obligation stay byte-identical; the step reports a typed hold and an audit
  event naming the position and the record and saying it needs a human; no pass returns `Err`; and
  a second obligation later in the ledger still drains (the hold counts against child-1's visit
  bound like any held obligation). The exit is operator repair, and the docs say so.
  **(D) Every remaining row drains to completion.** Each from a production writer on the base:
  `{chunks}` from a part re-upload (`RetireObligation::part_chunks`,
  `crates/core/src/multipart/commit.rs:1039`); flat `{generation}` from a Complete over an existing
  flat object (`superseded`, `crates/core/src/multipart/complete.rs:1627-1633`); `{session, all}`
  from a client Abort (`abort_session`, `crates/core/src/multipart/abort.rs:286`) of a session with
  committed parts and an **empty** `sidx:` range. For each: every position the row names carries
  this obligation's mark, before any record naming it is deleted (a recording wrapper shows the
  order where the row has records); the obligation is removed; after passes past the orphan grace
  every one of its fragments is gone from the chunk store. The `{session, all}` case is the
  counterpart of (R): the same shape, with no residue, completes.
  **(E-hold) Validated placements.** A `part:` record under a `retire:bytes:` obligation whose
  placement length is not its scheme's fragment count (a decodable case: an RS(4,2) chunk placed on
  5 servers; `RS(0,0)` does not decode as a `part:` record) is never deleted unmarked: no mark from
  that placement, no delete, a typed outcome and an audit event, the obligation and its cursor stay,
  and every fragment of that chunk stays (GC's `StagedSet` holds it, `gc.rs:1337-1360`).
  **(R) Owned residue.** A `{session}` or `{session, all}` obligation whose session still has a
  `sidx:` entry (`sidx_range`, `crates/core/src/multipart.rs:1429`) stays byte-identical across
  passes, the `sidx:` entry stays, and the step reports a typed hold. (#1013's owned walk empties
  the range.)
  **(S) An entry larger than a batch.** A part or generation with more fragment positions than one
  batch can mark makes progress every step, its cursor advancing inside the entry, and finishes.
  A `{generation}` over a segmented map (a Complete over an existing segmented object,
  `complete.rs:1631`) marks each `seg:` record's **current** placement, read at drain time
  (`0016:2432-2440`); its obligation is then removed, and after passes past the orphan grace every
  one of its fragments is gone.
  **(G) Restore stays accurate between drain steps** (the `deferred: #659` at
  `crates/custodian/src/restore.rs:1156`). An `Aborting@E+1` session fenced from `Completing@E`
  with segments written and parts staged (both obligations, `retire:bytes:s:<id>:<E>` and
  `retire:records:s:<id>:<E>` owing `{seg}`, installed by the production restore fence as
  `crates/custodian/tests/restore_completing_fence.rs` does) is drained **one step at a time**.
  After every step the production `reconcile_after_restore` (`restore.rs:597`) does not name the
  session (no `ChunkInNoPart`, `restore.rs:1234`; no `NoDeleter`, `:1176`) until both obligations
  are gone.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `retire:bytes:` drain in every shape of the row table
  (`crates/core/src/multipart.rs:3857-3863`): `{session}`, `{session, all}`, `{session, parts}`,
  `{parts}`, `{chunks}`, and `{generation}` flat and segmented; the mark guard (the `reclaiming`
  check, then the three arms) and its one seeded DST row; the time each mark is stamped with;
  validation of placements before marking, with the hold for an untrusted `part:` record; the
  owned-residue hold for a `session` component; segments-before-parts between a session token's two
  obligations; replacing the `deferred: #659` comment at `restore.rs:1156`; the docs. / out of
  scope:
  - child-1's work (the cursor codec, the walk and its bounds, the dispatch, the records mode),
    except what this mode adds to them;
  - the owned `sidx:` walk, the terminal session delete, and retirement by chunk id of untrusted
    records (#1013); the orphan-identity gate and unlink/supersede routing (#1014); the reaper
    (#625); the operator abort (#633);
  - any seeded DST scenario other than the one drain-versus-GC `reclaiming` row. #660 owns
    concurrent drainers; a review request for part/segment DST is out of scope and is
    record-rejected;
  - the S3 verbs (#668–#670); any edit to proposal 0016 or an ADR; `deny*.toml`; `Cargo.lock`
    (child-1 added the redb line; if this child needs another dev-dependency line, the human's
    2026-11-11 approval covers that and nothing else).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (26 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 167 mutants tested in 43m: 21 missed, 113 caught, 31 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1062/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.68s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #1062 multipart retirement drain passes independent red→green review for mark-before-delete reclamation; departed-server policy, prior-art confirmation, and operational acceptance remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable safety, progress, and hold outcomes for every scoped obligation shape; the unresolved departed-server policy is explicitly reserved for a human (brief.md:28; brief.md:117; brief.md:308). |
| C2 Reproduction (red pre-fix) | PASS | With production changes stashed and the new test retained, all 26 tests compiled and failed assertions, independently demonstrating the missing behavior (reviewer-red.log:233; crates/custodian/tests/multipart_retirement_marks.rs:855). |
| C3 Change | PASS | The changes address the authorized drain and GC mark cleanup, including the prior review corrections; the GC extension and size override have explicit approval (brief.md:288; brief.md:302; crates/custodian/src/gc.rs:918). |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 26 regressions plus 40 existing drain/sweep tests; the frozen full CI passed. Independent CI's missing gitlink and stale-base coverage are target-state caveats detailed below (reviewer-green.log:28; reviewer-green.log:53; reviewer-green.log:85; gate-logs/C4-ci.log:4944). |
| C5 Causal adequacy | PASS | Per-step clock reads remove premature grace, and exact-value mark transitions preserve reclamation intent; tests exercise both race winners and the requested hold/paging cases. No capability-probe workaround was introduced (crates/custodian/src/retirement.rs:428; crates/custodian/src/retirement/marks.rs:261; crates/dst/tests/custodian.rs:6788). |
| T1 Structure | PASS | The drain remains behind MetadataStore and the deployed reconciliation entry, reusing the mark codec and placement validator; instance-owned DST state preserves dependency and global-state conventions (crates/custodian/src/retirement/marks.rs:107; crates/custodian/src/reconciliation.rs:167; crates/dst/tests/custodian.rs:6524). |
| T2 Shape | PASS | The added mode has a separate private walker, while cursor and batch handling remain shared; living architecture documentation covers the changed lifecycle and operator holds (crates/custodian/src/retirement.rs:419; crates/custodian/src/retirement/marks.rs:324; docs/design/architecture/06-runtime-view.md:90). |
| T3 Runtime | PASS | Fixed visit/step/read budgets bound work, and a retained-record hold permits later obligations to drain; independent DST confirms the contested CAS behavior (crates/custodian/src/retirement.rs:139; crates/custodian/src/retirement.rs:184; crates/custodian/tests/multipart_retirement_marks.rs:1343; reviewer-ci.log:4351). |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art for the affected paths — the path-by-path investigation finds only a synthetic baseline with no remotes, so the brief's prior-art account cannot be independently confirmed from the supplied artifacts (reviewer-prior-art.log:1; reviewer-prior-art.log:15; brief.md:223). |
| T5 Judgment | NEEDS-HUMAN | Decide whether a departed D server may leave an obligation indefinitely awaiting GC, or needs an audited repair/escalation policy — GC retains that server's mark and the drain currently emits only a wait counter/debug event; the brief leaves this choice pending (brief.md:308; crates/custodian/src/gc.rs:904; crates/custodian/src/retirement/marks.rs:396; crates/custodian/src/retirement.rs:803). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the operational fit of bounded reclamation and operator-repair holds, and decide Tier-1/Tier-2 follow-up for deployment — automated protocol evidence supports the scoped implementation, while rollout acceptance remains the maintainer's decision (brief.md:57; brief.md:196; AGENTS.md:78; docs/design/architecture/06-runtime-view.md:90). |

No new builder-fixable defect was established. Source citations above are relative to `$PDCA_TARGET`; brief and log citations are relative to this review directory. The patch reverse-application check and whitespace check passed, and the stash was restored.

Independent evidence confirms the required behavior. `cargo test --offline -p wyrd-custodian --test multipart_retirement_marks` produced 26 assertion failures with the production fix stashed, then 26 passes after restoration. The existing `multipart_retirement_drain` and `gc_mark_sweep` suites added 19 and 21 passes. The current tests explicitly preserve unreadable marks and untrusted records, and verify all twenty small segments are marked before deletion and removed before their obligation (crates/custodian/tests/multipart_retirement_marks.rs:1280; crates/custodian/tests/multipart_retirement_marks.rs:1675; crates/custodian/tests/multipart_retirement_marks.rs:2555).

The full CI rerun has a snapshot-only failure, not an implementation finding. Typos, docs lint/render/link audit, formatting, workspace clippy/build, dependency-usage scan, conformance, static-state/dependency guards, and the DST tier ran successfully. The custodian DST suite passed all 35 tests, including both retirement race tests (reviewer-ci.log:4319; reviewer-ci.log:4328; reviewer-ci.log:4351). CI exited 1 because the synthetic Git index omits `.github/advisory-db`: its index test failed and the advisory scan could not obtain the pin (reviewer-ci.log:3784; reviewer-ci.log:4431). The frozen original-worktree log shows the actual pinned advisory scans passing and full CI completing successfully (gate-logs/C4-ci.log:4297; gate-logs/C4-ci.log:4309; gate-logs/C4-ci.log:4944). The brief's `typos` and `docs-renderer` dependencies were exercised independently (reviewer-ci.log:2; reviewer-ci.log:10).

The remaining frozen gates have these dispositions:

- **C4-verify: PASS.** Its captured 26-green/26-red result agrees with the independent rerun (gate-logs/C4-verify.log:13; gate-logs/C4-verify.log:245).
- **C4-diff-cov: unmeasured, target-state caveat.** The wrapper stopped before measuring coverage because the patch did not apply to its `origin/main`; this provides no coverage percentage and no patch-compilation defect (gate-logs/C4-diff-cov.log:10).
- **C5-mutants: recorded FAIL, adjudicated without a new defect.** The log reports 167 tested, 21 missed, 113 caught, 31 unviable, and 2 timeouts. The remaining missed classes fall under the explicit acceptance of the other mutants; the page-ending substitution requested for correction is absent from the current survivor list, while the remaining maximum-index guards are distinct. Do not reopen settled classes or require another split (gate-logs/C5-mutants.log:15; gate-logs/C5-mutants.log:34; gate-logs/C5-mutants.log:36; brief.md:307; brief.md:309).
- **T4-batch-review: PASS as logged.** Its captured output reports zero blocking findings; it supplies no additional finding rationale (gate-logs/T4-batch-review.log:10).
- **T4-contribution: N/A.** Contribution artifacts are drafted after Check; the substantive audit must rerun at publish. This deferred row creates no human clearance item (gate-logs/T4-contribution.log:10).
- **host-tikv: PASS independently reproduced.** Both backend and server feature checks completed with the real toolchain; the independent invocation exited 0 (reviewer-tikv.log:202; gate-logs/host-tikv.log:112; gate-logs/host-tikv.log:211).

The departed-server investigation confirms the policy gap directly: `awaiting-gc` stops at the reclaiming position, GC refuses to sweep a mark whose server is absent, and the wait emits no held-obligation audit. Existing coverage proves recovery with the server available, including real GC deletion and eventual mark cleanup; it does not settle the explicitly pending departed-server policy (crates/custodian/src/retirement/marks.rs:494; crates/custodian/src/gc.rs:904; crates/custodian/tests/multipart_retirement_marks.rs:1080). Retained-record holds and the work assigned to #1013/#660 remain settled scope boundaries. Tier-1/Tier-2 observation is warranted for deployment confidence in this custodian reclamation change, without expanding this slice's mandated DST scope (AGENTS.md:78; brief.md:149).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1062, iteration 3

**Verdict:** I could not refute the fix. The three test items the human asked for in iteration 2 are met in substance, and every leg I attacked fails for the right reason through `reconcile_step`. Two findings remain. The two `:886` mutants the human asked to have killed still survive, and they are killable (one small test, `[impl]`). Item 4, the unbounded and unaudited arm-(0) wait, is still a pending human decision, and a probe shows it also pins live bytes.

## What I re-ran

- I built the patch in a scratch copy and ran `multipart_retirement_marks` (26 tests) and `gc_mark_sweep` (21 tests). All pass. C4-verify's red is a blanket red: the child-1 base leaves every `retire:bytes:` obligation untouched, so all 26 fail. That shows the tests reach the new mode, but not that each leg is sharp, so I mutated production code one leg at a time. Each mutation turned the named leg red:
  - the `list` `Err` arm made a silent skip (`crates/custodian/src/retirement/marks.rs:774-777`) → `e_an_untrusted_chunk_in_a_chunks_list_is_held`, `e_an_untrusted_chunk_in_a_flat_generation_is_held`;
  - the `seg:` `Err` arm made a silent skip (`marks.rs:857-861`) → `e_an_untrusted_chunk_in_a_segment_record_is_held`;
  - an unreadable mark sent to arm (b) (`marks.rs:259`) → `c_a_mark_of_none_of_the_three_shapes_is_held_and_never_restamped`;
  - `retained: false` for `part:` entries (`marks.rs:699`) → `h_…`;
  - segments-before-parts ignored (`marks.rs:187`) → `g_…`;
  - `evidenced` ignored (`marks.rs:433`) → `b_…`, `d_an_aborted_sessions…`, `s_a_part_larger…`, and three `paging_…` tests;
  - owned-residue hold off (`marks.rs:152`) → `r_…`;
  - arm (0) re-stamped as arm (b) (`marks.rs:261`) → both `c_…reclaiming…`/`c_…no_fragment…` tests and `h_…`;
  - the GC sweep arm dropped, or made to sweep while the obligation is present (`crates/custodian/src/gc.rs:918-924`) → `b9_a_retirement_mark_is_kept_while_its_obligation_is_present_and_swept_after` and `c_a_drain_mark_over_no_fragment_is_swept_only_once_its_obligation_is_gone`;
  - the `:886` guard set to false (`Tail::Ended` after one page) → `paging_a_group_of_small_segments_past_one_page`;
  - the `:603` guard set to false → `paging_an_aborted_sessions_parts_past_one_page`.

## Findings

- NEEDS-HUMAN [impl] — The two `:886` mutants the C5 log still lists as MISSED (`replace match guard index < MAX_SEGMENT_INDEX with true`, and `< with <=`) are **killable, not equivalent**. The code at `crates/custodian/src/retirement/marks.rs:886-887` is correct; no test reaches it. Failing case: a segmented `{generation}` whose last page of 16 `seg:` records ends at index 999,999 (`MAX_SEGMENT_INDEX`), with every position already carrying this obligation's mark. The patch finishes and deletes the obligation in one step. Both mutants instead build cursor entry 1,000,000, which `checked_cursor` refuses (`crates/core/src/multipart.rs:4296`). The step then becomes a `CursorRefused` hold, nothing commits, and the obligation is held for good. I wrote that probe in scratch: 16 planted records at 999,984–999,999, all pre-marked, one drain, assert the obligation is gone and nothing was audited. It passes on the patch and fails on both mutants. Iteration 2's item 3 asked for the `:886` mutants to be killed. The `false` mutant is now killed; these two need this one boundary test. Low severity.
- NEEDS-HUMAN [human] — **Item 4 is still open, and it pins live bytes as well as a ledger row.** Arm (0) over a position no retained record names returns `Planned::Wait(AWAITING_GC)` (`marks.rs:396-399`, `:494`). That wait only increments a counter and logs at debug level (`crates/custodian/src/retirement.rs:438-441`, `:803-812`). It has no time bound and emits no audit event. GC keeps a `reclaiming` mark whose server has left the fleet (`gc.rs:905`, `server-not-in-fleet`), so it never deletes the key. Probe: a `{chunks}` obligation over one RS(2,1) chunk on servers 0–2, a `reclaiming` mark of another event on server 2's position, then server 2 removed from the fleet. Over 20 drain + GC rounds, each past the grace and the late-write deadline, the outcomes were `[Changed, Partial ×19]`, with 19 `awaiting-gc` debug waits and **zero audit events**. The obligation was still present. The two live fragments on servers 0 and 1 were kept, because the drain's own marks protect them while the obligation exists (`gc.rs:695`, `draining-retirement`). The patch rightly left production code alone, per iteration 2's instructions, so the policy choice is still owed. Options: after N waits, emit an audit event or turn the wait into a typed hold (as (H) does), or accept it with a tracking issue.

## Attacks that did not land

- **(T) stamp.** The clock is read for each step after the obligation's bytes are in hand (`retirement.rs:428`). A lost precondition re-reads the obligation and loops back to that read (`:470`). The (T) test would catch the cached pass-time shape. It cannot tell a per-step read from one read at the start of the retirement leg, because the install fires during reconstruction. The code reads per step anyway, so there is no defect.
- **A `part:` placement changing between two steps**, which would let the cursor's "positions before me are marked" shortcut skip a re-placed position. Not reachable: the staged re-place requires `mpu == Open@E` (`crates/custodian/src/reconstruction/staged.rs:16`, `:23`), and every token that carries parts is installed by a transition out of `Open`. Placements in a superseded `seg:` record are stable once the supersede's inode compare-and-swap commits (`0016:2432-2440`).
- **A second component silently dropped.** `plan` drains only one component (`marks.rs:156-164`). However, `checked_against_key` (`multipart.rs:4401`) leaves `{session, parts}` as the only legal pair, and the drain handles both halves.
- **The GC sweep change** (`gc.rs:918-924`) only ever deletes marks, never fragments. Its worst case is an unmarked fragment that GC keeps (a leak, not a loss), as its doc says. Both directions of it are tested (mutations above).
- **The budget mutants** (`marks.rs:366-367`, `:422-423`, `:439-440`) are equivalent. Operations cannot bind before `reads_left` does: 233×2 + 16×2 + 2 = 500 = `MAX_BATCH_OPS`, checked by the const asserts at `retirement.rs:187-191`. Bytes are about 200 KB worst case against `MAX_BATCH_BYTES` = `E_TX_BYTES / 2`. The human already accepted these.

## Gate notes (not refutations)

- C4-diff-cov measured nothing: the bundle stacks on child-1 and does not apply on `origin/main`. This is the same as earlier rounds. The targeted mutations above partly cover what it would have shown.
- T4 reports 0 blocking in 38 s for three review passes over a 277 KB diff, where iteration 2 had 3 blocking. I cannot see `review-batch.md`, so I cannot tell whether this was a fresh deep review. I'm noting it, not raising it.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [human] — `crates/custodian/src/retirement/marks.rs:396`: a non-retained position carrying a `reclaiming` mark always waits for GC. If its D server permanently leaves the fleet, GC cannot destroy its fragment and explicitly retains its fragment-less mark (`crates/custodian/src/gc.rs:904`), so the obligation cannot finish. The wait emits a counter and debug event, without an audited hold or escalation (`crates/custodian/src/retirement.rs:803`). This is the unresolved policy choice in brief iteration 2, item 4: explicitly accept indefinite retention, or require escalation to an operator-repair hold.

No additional implementation defects or worthwhile reuse, simplification, or efficiency findings identified in this diff. Review used the target source and frozen gate evidence; tests were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm merged and closed/rejected prior art for the affected paths — the path-by-path investigation finds only a synthetic baseline with no remotes, so the brief's prior-art account cannot be independently confirmed from the supplied artifacts (reviewer-prior-art.log:1; reviewer-prior-art.log:15; brief.md:223).
- [x] T5 Judgment — Decide whether a departed D server may leave an obligation indefinitely awaiting GC, or needs an audited repair/escalation policy — GC retains that server's mark and the drain currently emits only a wait counter/debug event; the brief leaves this choice pending (brief.md:308; crates/custodian/src/gc.rs:904; crates/custodian/src/retirement/marks.rs:396; crates/custodian/src/retirement.rs:803).
- [x] Validation — fitness-to-purpose — Accept the operational fit of bounded reclamation and operator-repair holds, and decide Tier-1/Tier-2 follow-up for deployment — automated protocol evidence supports the scoped implementation, while rollout acceptance remains the maintainer's decision (brief.md:57; brief.md:196; AGENTS.md:78; docs/design/architecture/06-runtime-view.md:90).
- [x] The two `:886` mutants the C5 log still lists as MISSED (`replace match guard index < MAX_SEGMENT_INDEX with true`, and `< with <=`) are **killable, not equivalent**. The code at `crates/custodian/src/retirement/marks.rs:886-887` is correct; no test reaches it. Failing case: a segmented `{generation}` whose last page of 16 `seg:` records ends at index 999,999 (`MAX_SEGMENT_INDEX`), with every position already carrying this obligation's mark. The patch finishes and deletes the obligation in one step. Both mutants instead build cursor entry 1,000,000, which `checked_cursor` refuses (`crates/core/src/multipart.rs:4296`). The step then becomes a `CursorRefused` hold, nothing commits, and the obligation is held for good. I wrote that probe in scratch: 16 planted records at 999,984–999,999, all pre-marked, one drain, assert the obligation is gone and nothing was audited. It passes on the patch and fails on both mutants. Iteration 2's item 3 asked for the `:886` mutants to be killed. The `false` mutant is now killed; these two need this one boundary test. Low severity.
- [x] **Item 4 is still open, and it pins live bytes as well as a ledger row.** Arm (0) over a position no retained record names returns `Planned::Wait(AWAITING_GC)` (`marks.rs:396-399`, `:494`). That wait only increments a counter and logs at debug level (`crates/custodian/src/retirement.rs:438-441`, `:803-812`). It has no time bound and emits no audit event. GC keeps a `reclaiming` mark whose server has left the fleet (`gc.rs:905`, `server-not-in-fleet`), so it never deletes the key. Probe: a `{chunks}` obligation over one RS(2,1) chunk on servers 0–2, a `reclaiming` mark of another event on server 2's position, then server 2 removed from the fleet. Over 20 drain + GC rounds, each past the grace and the late-write deadline, the outcomes were `[Changed, Partial ×19]`, with 19 `awaiting-gc` debug waits and **zero audit events**. The obligation was still present. The two live fragments on servers 0 and 1 were kept, because the drain's own marks protect them while the obligation exists (`gc.rs:695`, `draining-retirement`). The patch rightly left production code alone, per iteration 2's instructions, so the policy choice is still owed. Options: after N waits, emit an audit event or turn the wait into a typed hold (as (H) does), or accept it with a tracking issue.
- [x] `crates/custodian/src/retirement/marks.rs:396`: a non-retained position carrying a `reclaiming` mark always waits for GC. If its D server permanently leaves the fleet, GC cannot destroy its fragment and explicitly retains its fragment-less mark (`crates/custodian/src/gc.rs:904`), so the obligation cannot finish. The wait emits a counter and debug event, without an audited hold or escalation (`crates/custodian/src/retirement.rs:803`). This is the unresolved policy choice in brief iteration 2, item 4: explicitly accept indefinite retention, or require escalation to an operator-repair hold.
- [x] **The reclaiming test manually removes a possible permanent hold.** `brief.md:193-201` retains a naming record until its position carries this retirement's mark, waits on `reclaiming`, and assumes a later pass finds the key gone. For a session with a retained `part:` record naming that position, however, GC's staged protection outranks even `reclaiming` and prevents fragment deletion (`crates/custodian/src/gc.rs:654-668`); its fragment-less sweep also preserves a staged-protected key (`gc.rs:887-894`). Thus the seeded state can leave the drain waiting for GC and GC waiting for removal of the part. Criterion C′ explicitly deletes the key from the fixture (`brief.md:47-50`), bypassing this interaction. Add a case alternating drain steps with real GC over the retained-part state, and specify its expected resolution. If this is intentionally an operator-repair hold or excluded as unreachable under stated prerequisites, say so and qualify the promise to drive “every” obligation to empty (`brief.md:89-90`); do not imply GC alone guarantees recovery.
- [x] **The acceptance cases do not discriminate the promised coverage of every retirement shape.** Scope includes `{chunks}` and both generation forms (`brief.md:104-106`), which are distinct inputs in the target's writer table (`crates/core/src/multipart.rs:3769-3771`). Yet B requires only unnamed parts, R requires a session hold, and S permits “a part or generation” while requiring a segmented-generation placement check (`brief.md:32-65`); T and C′ leave the obligation shape unspecified. All prescribed cases could therefore use parts and segmented generations while `{chunks}` and flat-generation draining remain unimplemented. Require explicit successful completion cases for those two rows: expected fragment positions marked, obligation removed, and actual reclamation after grace. Also distinguish successful empty-residue session completion from R's intentionally held session. These are checks of the existing scope, not additional features.
- [x] size backstop — this slice is behaving oversized: patch is 271 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (issue_1062): departed-server policy — arm-(0) `awaiting-gc` wait (`marks.rs:396-399`, `:494`) has no bound and no audit; after N waits, raise an audited operator-repair hold like (H). Accepted as indefinite wait for now.
- Follow-up (issue_1062): kill the two surviving `:886` mutants (`marks.rs:886`) with a boundary test — 16 pre-marked `seg:` records at indices 999,984–999,999; obligation must finish in one step with no audit.
