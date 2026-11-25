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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (21 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 167 mutants tested in 43m: 26 missed, 108 caught, 31 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1062/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.86s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #1062 adds bounded, reader-safe draining of multipart `retire:bytes:` obligations; this review found no grounded implementation defect, with prior-art confirmation and fitness-to-purpose still owed by a human.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required safety, completion and hold outcomes are falsifiable and agree with evidence-before-unreference and drain-time grace; the GC sweep extension is explicitly authorized (`brief.md:288`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1098`, `:1123`). |
| C2 Reproduction (red pre-fix) | PASS | With tracked production changes stashed and the new regression file retained, the test compiled and all 21 cases failed on behavioral assertions (`reviewer-red.log:331`; `crates/custodian/tests/multipart_retirement_marks.rs:944`). |
| C3 Change | PASS | The patch stays within the authorized drain, GC cleanup, restore guarantee, tests and living documentation; all ten target blobs match the supplied patch (`reviewer-source-check.log:11`; `crates/custodian/src/gc.rs:918`; `docs/design/architecture/06-runtime-view.md:90`). |
| C4 Verification (red→green) | PASS | Restoring the patch made all 21 tests pass; seeded DST also passed independently, and frozen full CI is green; the local full-CI failure is the snapshot's missing advisory-db gitlink, detailed below (`reviewer-green.log:30`; `reviewer-ci.log:4314`, `:4324`, `:4426`; `gate-logs/C4-ci.log:4939`). |
| C5 Causal adequacy | PASS | Tests distinguish fresh retirement grace, terminal reclaim intent and missing-record protection; sampled guard-removal mutants fail, and no optional-capability probe masks a load-time cause (`crates/custodian/tests/multipart_retirement_marks.rs:951`; `reviewer-mutation-check.log:4`, `:6`, `:9`). |
| T1 Structure | PASS | Maintenance continues through the deployed reconciliation seam and `MetadataStore`, sharing GC's placement validation instead of introducing backend coupling or another placement rule (`crates/custodian/src/retirement.rs:348`; `crates/custodian/src/retirement/marks.rs:680`; `crates/server/src/custodian.rs:397`). |
| T2 Shape | PASS | Existing codecs preserve exact-value conflict checks and same-event mark identity; typed holds name damaged records, and the living runtime description covers the new behavior (`crates/custodian/src/retirement.rs:445`; `crates/custodian/tests/multipart_retirement_marks.rs:1178`; `docs/design/architecture/06-runtime-view.md:90`). |
| T3 Runtime | PASS | Per-step and per-pass limits bound ledger work and retries; the independent 50-seed campaign exercised both commit winners and the reclaiming re-read window (`crates/custodian/src/retirement.rs:168`, `:418`; `crates/dst/tests/custodian.rs:6790`, `:6850`; `reviewer-ci.log:4346`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must run at publish, as the deferred gate states (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm merged and closed/rejected prior art for all ten affected paths — the path-specific investigation found only one synthetic base commit and no remotes, so the brief's prior-art narrative cannot be independently corroborated here (`reviewer-prior-art.log:2`, `:6`, `:8`; `brief.md:223`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the tested reclamation behavior and operator hold reporting for the intended deployment — automated correctness checks do not decide operational fitness; apply any additional host-only criteria from INTEGRATION.md §4, which is absent from this bundle and target (`docs/design/architecture/06-runtime-view.md:90`; `reviewer-prior-art.log:9`). |

The independent evidence supports the patch's correctness within the accepted scope. Source citations above and below are relative to `$PDCA_TARGET`; review and gate logs are relative to the supplied bundle. No builder rationale was consulted, and the target's standing rubric was applied.

- **Red→green was reproduced, not inferred.** Ran `cargo test --offline --locked -p wyrd-custodian --test multipart_retirement_marks` with production changes stashed, then after `git stash pop`: 0/21 passed before, 21/21 after. This agrees with `gate-logs/C4-verify.log:10` and `:203`. The patch was restored, no stash remains, and every changed target blob still matches `patch.diff` (`reviewer-source-check.log:1`).
- **The full local CI run has a specific snapshot limitation.** `cargo xtask ci` ran spelling, docs lint/render, repository guards, formatting, workspace Clippy/build/tests, dependency checks, conformance and the 50-seed DST tier. It exited 1 because `.github/advisory-db` has no gitlink in this synthetic index: the corresponding xtask test failed and the dependency audit could not obtain its pin (`reviewer-ci.log:3778`, `:4426`; `xtask/tests/advisory_db.rs:66`). This is a target-state caveat, not a patch defect. The frozen log actually shows the gitlink test passing, the pinned RustSec audit succeeding, and full CI completing (`gate-logs/C4-ci.log:3852`, `:4292`, `:4304`, `:4939`). The declared `typos` and docs-renderer dependencies were exercised independently; rendering produced 101 pages and passed its link audit (`reviewer-ci.log:2`, `:7`). Both TiKV feature Clippy invocations also passed independently (`reviewer-tikv.log:103`, `:202`), agreeing with `gate-logs/host-tikv.log:211`.
- **Coverage remains unmeasured; mutation results remain advisory.** The frozen coverage wrapper could not apply the bundle to `origin/main` (`gate-logs/C4-diff-cov.log:10`); that does not establish a defect on the supplied, buildable target. The frozen mutation run reports 26 missed, 108 caught, 31 unviable and 2 timed out (`gate-logs/C5-mutants.log:41`). I independently sampled eight mutations against the new regression file with lint capping enabled: three critical guard removals were caught and five survived (`reviewer-mutation-check.log:11`). An initial run rejected four mutations solely because they created denied unused-code warnings; the capped rerun resolved that limitation. The sampled survivors affect budget redundancy, wait/progress classification, and the small-segment page-tail branch. They do not establish an incorrect original implementation; neither this sample nor the frozen run proves exhaustive coverage. The ordinary operation envelope is independently bounded by 233 mark reads and sixteen record entries (`crates/custodian/src/retirement.rs:168`); the unexercised small-segment branch is explicit at `crates/custodian/src/retirement/marks.rs:882`. The full 167-mutant campaign was not repeated.
- **The carry-forward fixes have substantive evidence.** Missing parts and segments under partial cursors remain held with and without trailing records (`crates/custodian/tests/multipart_retirement_marks.rs:1993`, `:2067`); real GC removes drained fragment-less marks (`:1060`, `:1112`); production-created obligations cross part and segment pages (`:2180`, `:2209`, `:2246`). The DST assertions require both stale-value attempts, exactly one committed outcome and one conflict, and a reclaiming read before GC's key delete (`crates/dst/tests/custodian.rs:6790`, `:6801`, `:6850`). Both race tests passed in the independent CI run (`reviewer-ci.log:4314`, `:4324`).
- **Contribution and human boundaries remain explicit.** The batch-review log reports zero blocking findings; its summary supplies no further finding details (`gate-logs/T4-batch-review.log:10`). Publish-time contribution checking remains N/A at Check. Deferrals to #660, #1013 and #1014 remain settled. Observing the existing Tier-1 and Tier-2 custodian campaigns is appropriate follow-up for reclamation changes, not an additional merge gate (`AGENTS.md:82`). The two NEEDS-HUMAN rows above concern historical context and operational acceptance, not builder-fixable defects.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #1062 (retirement drain, `retire:bytes:` marking mode)

**Verdict: I could not break the core fix.** The mark guard, the stamp, the hold for owned residue, segments-before-parts and the GC sweep change each failed when I broke them on purpose. Re-run on a scratch copy of `$PDCA_TARGET`: all 21 tests in `crates/custodian/tests/multipart_retirement_marks.rs` pass. What I did find is three new hold paths with no test at all, plus one carry-forward instruction that was not met. Every finding below is a test or audit gap. None makes the drain lose data.

## Findings

- NEEDS-HUMAN [impl] — **Untrusted placements in list rows and `seg:` records are untested, and a silent skip there passes every test.** The `{chunks}` / flat `{generation}` hold is at `crates/custodian/src/retirement/marks.rs:774-777`; the `seg:` record hold is at `:857-861`. I made two separate edits: in each, a chunk whose placement fails is skipped (`Err(_) => {}`) instead of held. Both edits leave `multipart_retirement_marks.rs` (21 tests), `multipart_retirement_drain.rs` (19) and `gc_mark_sweep.rs` all green. Concrete failing case for the first edit: a part re-upload over a `part:` record that names an RS(4,2) chunk on 5 servers. That record decodes, as (E-hold) itself shows. The re-upload installs `{chunks}` naming that chunk (`RetireObligation::part_chunks`). The edited drain then deletes the obligation with that chunk's fragments unmarked and unnamed, and GC keeps them for good (`crates/custodian/src/gc.rs:742`). This is the rubric's "never silent skip" class. (E-hold) only tests a `part:` record (`multipart_retirement_marks.rs:1498`, the only `"untrusted-placement"` assertion, `:1535`). Fix: add a `{chunks}` (or flat `{generation}`) case and a segmented `{generation}` case. Each should assert the typed hold, the audit event, a byte-identical obligation and record, and no marks.

- NEEDS-HUMAN [impl] — **The new `Hold::UnreadableMark` has no test.** It is built at `marks.rs:259` and `:400-408`. I made a mark that will not decode take arm (b) instead: re-stamped under `require(== prior)`. All 21 tests still pass, and nothing anywhere in `crates/` asserts `unreadable-mark` for the drain. Why it matters: GC keeps such a mark and its fragment until a human repairs it (`gc.rs:709-712`). The codec rejects every spelling but its own (`crates/core/src/metadata.rs:304-334`). So "won't decode" includes a newer writer's shape, which could be a `reclaiming` mark. Overwriting it would erase GC's evidence and repeat the very hazard arm (0) exists to prevent. Fix: plant a value that will not decode at a drained position. Assert the hold, the value byte-identical, the obligation byte-identical, and the audit naming the position.

- NEEDS-HUMAN [impl] — **Carry-forward item 3 is not met: the surviving mutants in the `segments` tail are still alive.** C5 still lists six survivors at `marks.rs:886`. Two of them (`886:35` guard → `false`, `886:41` `<` → `>`) turn `Tail::At(index + 1)` into `Tail::Ended`. That deletes the obligation with segments 17 and later never marked and their `seg:` records left behind. The >16-segment test that was added (`multipart_retirement_marks.rs:2246`) cannot reach that arm, for two reasons. Every non-last segment a production Complete writes holds at least 157 chunks (`crates/core/src/staged_publication.rs:588-599`). A step reads at most 233 positions. So no step finishes 16 segments, as the code's own comment admits (`marks.rs:882-885`). Fix: kill it with a planted group of 17 or more one-chunk `seg:` records under a `{generation}` obligation. Alternatively, the human can rule the arm unreachable; either way, record that this instruction was not met.

- NEEDS-HUMAN [human] — **The arm-(0) "wait on GC" has no time limit and no audit, and the claim that GC always finishes is not true everywhere.** The claim appears at `marks.rs:34-35` and in `docs/design/architecture/06-runtime-view.md:90` ("GC destroys the fragment and deletes the record itself"). At a `reclaiming` mark over a position no kept record names, the step returns `Planned::Wait(AWAITING_GC)` (`marks.rs:396-399`, `:494`). That logs only a `debug!` event and a counter (`retirement.rs:438`, `:803-812`). GC judges only fragments it lists, and its sweep keeps any mark whose D server is not in its fleet as `server-not-in-fleet` (`gc.rs:904-905`). Concrete case: GC swaps a mark at a `{generation}` or `{chunks}` position to `reclaiming`. A post-restore legacy mark is the kind of mark the DST row races. Then that D server leaves the fleet before GC destroys the fragment. From then on the obligation waits every pass, forever. No warn-level audit names it, and every position after that one is never marked. The brief's invariant says every exception is "typed and audited". The fix is a policy choice: escalate a wait older than some bound (for example the `reclaiming` stamp plus grace plus `D`) to an audited hold, or accept the gap and document it. The case needs both damage and a lost server, so the risk is low.

- Note (not a defect): no test checks the mark-then-delete order for `seg:` records. When I let a `seg:` record go in the same batch that marks its positions, all tests still pass. That keeps rule 2 (the batch is atomic, and GC does not count superseded groups as referenced, `gc.rs:1200-1206`). But it breaks the "an earlier commit, never the same one" promise in `marks.rs:50-56` and in 06-runtime-view.md. A recording-order assert in the two segmented tests would pin it.

## Refutation attempts that failed

- **Red→green evidence.** C4-verify's red leg reverts all production code, which only proves the feature is needed. So I made one targeted edit per guarantee, and the named leg went red each time:
  - Arm (0) turned off (a `reclaiming` mark re-stamped): both (C′) tests and (H) fail.
  - `reclaiming` over a kept record taken as "already marked": (H) fails.
  - Record deletes allowed before the step has seen this obligation's own mark: (B), (D-abort), (S-part) and both part-paging tests fail.
  - Owned-residue check off: (R) fails.
  - Segments-before-parts off: (G) fails.
  - GC sweep ignoring whether the obligation is present: `b9` and the (C′)-sweep test fail.
- **Stamp (T).** The stamp is read per step from `ctx.clock` after the obligation's bytes are in hand (`retirement.rs:428`). That clock is the loop's single time source (`crates/server/src/custodian.rs:134-167`). The (T) fixture installs the obligation inside the reconstruction leg's queue scan, so the first attempt's cached-time shape would fail it.
- **Batch budget.** The codec accepts only its own exact spelling, so arm (b)'s precondition is at most 328 bytes. `MAX_BATCH_BYTES` is 5 MB and reads run out at 233 first, so the byte and operation checks never trigger. That makes the C5 survivors at `marks.rs:131`, `:366-367` and `:422-440` equivalent mutants (changes that alter no behaviour), not missed defects. The same goes for `:491`, `:495`, `:566` and `:603`.
- **Segments-before-parts across epochs.** A Complete that releases its own fence leaves `retire:records:` `{seg}` at E+1. A later Abort installs `retire:bytes:` at E+2. The tokens differ, so the check does not fire. But restore re-checks only the group at E′−1 (`crates/custodian/src/restore.rs:1132-1137`), so it never reports `ChunkInNoPart`. Not a defect.
- **Bytes-mode payload combinations.** The decoder can't represent `{parts, chunks}` and similar combinations (`crates/core/src/multipart.rs:4360-4366`), so the component choice at `marks.rs:156-164` cannot silently drop one.
- **GC sweep change.** It deletes only marks at positions no listing reported, so it cannot destroy a fragment. A write that lands after its deadline leaves an unmarked fragment that GC keeps: a leak, never a loss.
- **DST row.** `retirement_restamp_races_gc_swap` and `retirement_restamp_reaches_both_winners` both ran and passed under `--cfg madsim` in `gate-logs/C4-ci.log`.
- **Gate claim to qualify.** `check-gates.json` reports `overall: pass`, but C4 diff coverage was never measured ("patch.diff does not apply on origin/main"). No gate measured line coverage of the new code, and the untested holds above are exactly what coverage would have shown.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No actionable findings on either advisory lens: introduced correctness bugs or reuse, simplification, and efficiency issues.

Reviewed the diff against the target source and frozen gate evidence, including cursor progress, mark-before-delete ordering, GC interactions, and the regression tests. CI and red-to-green verification passed. Diff coverage was unavailable; the surviving mutants did not establish an additional actionable defect. Gates were not rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm merged and closed/rejected prior art for all ten affected paths — the path-specific investigation found only one synthetic base commit and no remotes, so the brief's prior-art narrative cannot be independently corroborated here (`reviewer-prior-art.log:2`, `:6`, `:8`; `brief.md:223`).
- [ ] Validation — fitness-to-purpose — Accept the tested reclamation behavior and operator hold reporting for the intended deployment — automated correctness checks do not decide operational fitness; apply any additional host-only criteria from INTEGRATION.md §4, which is absent from this bundle and target (`docs/design/architecture/06-runtime-view.md:90`; `reviewer-prior-art.log:9`).
- [ ] **Untrusted placements in list rows and `seg:` records are untested, and a silent skip there passes every test.** The `{chunks}` / flat `{generation}` hold is at `crates/custodian/src/retirement/marks.rs:774-777`; the `seg:` record hold is at `:857-861`. I made two separate edits: in each, a chunk whose placement fails is skipped (`Err(_) => {}`) instead of held. Both edits leave `multipart_retirement_marks.rs` (21 tests), `multipart_retirement_drain.rs` (19) and `gc_mark_sweep.rs` all green. Concrete failing case for the first edit: a part re-upload over a `part:` record that names an RS(4,2) chunk on 5 servers. That record decodes, as (E-hold) itself shows. The re-upload installs `{chunks}` naming that chunk (`RetireObligation::part_chunks`). The edited drain then deletes the obligation with that chunk's fragments unmarked and unnamed, and GC keeps them for good (`crates/custodian/src/gc.rs:742`). This is the rubric's "never silent skip" class. (E-hold) only tests a `part:` record (`multipart_retirement_marks.rs:1498`, the only `"untrusted-placement"` assertion, `:1535`). Fix: add a `{chunks}` (or flat `{generation}`) case and a segmented `{generation}` case. Each should assert the typed hold, the audit event, a byte-identical obligation and record, and no marks.
- [ ] **The new `Hold::UnreadableMark` has no test.** It is built at `marks.rs:259` and `:400-408`. I made a mark that will not decode take arm (b) instead: re-stamped under `require(== prior)`. All 21 tests still pass, and nothing anywhere in `crates/` asserts `unreadable-mark` for the drain. Why it matters: GC keeps such a mark and its fragment until a human repairs it (`gc.rs:709-712`). The codec rejects every spelling but its own (`crates/core/src/metadata.rs:304-334`). So "won't decode" includes a newer writer's shape, which could be a `reclaiming` mark. Overwriting it would erase GC's evidence and repeat the very hazard arm (0) exists to prevent. Fix: plant a value that will not decode at a drained position. Assert the hold, the value byte-identical, the obligation byte-identical, and the audit naming the position.
- [ ] **Carry-forward item 3 is not met: the surviving mutants in the `segments` tail are still alive.** C5 still lists six survivors at `marks.rs:886`. Two of them (`886:35` guard → `false`, `886:41` `<` → `>`) turn `Tail::At(index + 1)` into `Tail::Ended`. That deletes the obligation with segments 17 and later never marked and their `seg:` records left behind. The >16-segment test that was added (`multipart_retirement_marks.rs:2246`) cannot reach that arm, for two reasons. Every non-last segment a production Complete writes holds at least 157 chunks (`crates/core/src/staged_publication.rs:588-599`). A step reads at most 233 positions. So no step finishes 16 segments, as the code's own comment admits (`marks.rs:882-885`). Fix: kill it with a planted group of 17 or more one-chunk `seg:` records under a `{generation}` obligation. Alternatively, the human can rule the arm unreachable; either way, record that this instruction was not met.
- [ ] **The arm-(0) "wait on GC" has no time limit and no audit, and the claim that GC always finishes is not true everywhere.** The claim appears at `marks.rs:34-35` and in `docs/design/architecture/06-runtime-view.md:90` ("GC destroys the fragment and deletes the record itself"). At a `reclaiming` mark over a position no kept record names, the step returns `Planned::Wait(AWAITING_GC)` (`marks.rs:396-399`, `:494`). That logs only a `debug!` event and a counter (`retirement.rs:438`, `:803-812`). GC judges only fragments it lists, and its sweep keeps any mark whose D server is not in its fleet as `server-not-in-fleet` (`gc.rs:904-905`). Concrete case: GC swaps a mark at a `{generation}` or `{chunks}` position to `reclaiming`. A post-restore legacy mark is the kind of mark the DST row races. Then that D server leaves the fleet before GC destroys the fragment. From then on the obligation waits every pass, forever. No warn-level audit names it, and every position after that one is never marked. The brief's invariant says every exception is "typed and audited". The fix is a policy choice: escalate a wait older than some bound (for example the `reclaiming` stamp plus grace plus `D`) to an audited hold, or accept the gap and document it. The case needs both damage and a lost server, so the risk is low.
- [ ] **The reclaiming test manually removes a possible permanent hold.** `brief.md:193-201` retains a naming record until its position carries this retirement's mark, waits on `reclaiming`, and assumes a later pass finds the key gone. For a session with a retained `part:` record naming that position, however, GC's staged protection outranks even `reclaiming` and prevents fragment deletion (`crates/custodian/src/gc.rs:654-668`); its fragment-less sweep also preserves a staged-protected key (`gc.rs:887-894`). Thus the seeded state can leave the drain waiting for GC and GC waiting for removal of the part. Criterion C′ explicitly deletes the key from the fixture (`brief.md:47-50`), bypassing this interaction. Add a case alternating drain steps with real GC over the retained-part state, and specify its expected resolution. If this is intentionally an operator-repair hold or excluded as unreachable under stated prerequisites, say so and qualify the promise to drive “every” obligation to empty (`brief.md:89-90`); do not imply GC alone guarantees recovery.
- [ ] **The acceptance cases do not discriminate the promised coverage of every retirement shape.** Scope includes `{chunks}` and both generation forms (`brief.md:104-106`), which are distinct inputs in the target's writer table (`crates/core/src/multipart.rs:3769-3771`). Yet B requires only unnamed parts, R requires a session hold, and S permits “a part or generation” while requiring a segmented-generation placement check (`brief.md:32-65`); T and C′ leave the obligation shape unspecified. All prescribed cases could therefore use parts and segmented generations while `{chunks}` and flat-generation draining remain unimplemented. Require explicit successful completion cases for those two rows: expected fragment positions marked, obligation removed, and actual reclamation after grace. Also distinguish successful empty-residue session completion from R's intentionally held session. These are checks of the existing scope, not additional features.
- [ ] size backstop — this slice is behaving oversized: patch is 258 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human decision: iterate-do on a narrow list; the size backstop (258 KB) is overridden. Do not re-split; keep the production code as is unless a test below exposes a bug. 1. Untrusted placements outside `part:` records are untested. Add a `{chunks}` (or flat `{generation}`) case for the hold at marks.rs:774-777, and a segmented `{generation}` case for the `seg:` hold at :857-861. Each asserts the typed hold, the audit event, a byte-identical obligation and record, and no marks. Both must fail if the `Err(_)` arm is turned into a silent skip. 2. `Hold::UnreadableMark` (marks.rs:259, :400-408) has no test. Plant a value that will not decode at a drained position. Assert the hold, the value byte-identical, the obligation byte-identical, and the audit naming the position. It must fail if that mark is re-stamped via arm (b). 3. Carry-forward item 3 (attempt 2) is not met: the surviving mutants at marks.rs:886 (`Tail::At(index + 1)` vs `Tail::Ended`). Kill them with a planted group of 17 or more one-chunk `seg:` records under a `{generation}` obligation, asserting every segment is marked and its record drained before the obligation is removed. 4. Arm-(0) "wait on GC" (marks.rs:396-399, :494) has no time limit or audit; a reclaiming mark whose D server left the fleet waits forever (gc.rs:904-905). Policy: PENDING human choice (see below). Not in scope: anything else in the adversary's notes; the other 26 mutants are accepted as equivalent.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
