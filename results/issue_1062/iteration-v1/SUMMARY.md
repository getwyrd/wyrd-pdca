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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (13 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 164 mutants tested in 44m: 51 missed, 85 caught, 26 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1062/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.52s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1062’s bounded `retire:bytes:` drain with reader grace and reclaiming-safe marks: one implementation defect remains, reproduced for both part and segment records.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted split defines observable reclamation, grace, ordering and hold outcomes; proposal 0016 requires evidence before protection is lifted (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1123`; `brief.md:28`). |
| C2 Reproduction (red pre-fix) | PASS | With the tracked fix stashed and the new tests retained, all 13 tests compiled and failed by assertion; the pre-fix drain cannot discharge bytes obligations (`crates/custodian/tests/multipart_retirement_marks.rs:802`; `reviewer-red.log:8`). |
| C3 Change | PASS | The diff stays within the approved bytes-drain split, its integration tests and living architecture documentation; deferred session cleanup remains explicit (`crates/custodian/src/retirement.rs:418`; `crates/custodian/src/retirement/marks.rs:205`; `docs/design/architecture/06-runtime-view.md:90`). |
| C4 Verification (red→green) | PASS | Restoring the patch gives 13/13 new tests and 19/19 peer tests passing, and the local custodian DST suite passes 35/35; full-CI host limitations and unmeasured diff coverage are qualified below (`reviewer-restored-green.log:26`; `reviewer-restored-green.log:45`; `reviewer-ci.log:3905`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve the obligation and report `RecordUnreadable` when a nonzero cursor's final naming record disappears—both walkers currently erase unfinished work with seven positions still unmarked (`crates/custodian/src/retirement/marks.rs:590`; `crates/custodian/src/retirement/marks.rs:839`; `reviewer-counterexamples.log:6`). |
| T1 Structure | PASS | The maintenance implementation remains behind `MetadataStore` and shares GC's staged-placement validation, preserving dependency direction and validation semantics (`crates/custodian/src/retirement/marks.rs:104`; `crates/custodian/src/gc.rs:1472`). |
| T2 Shape | PASS | Existing codecs and exact-value preconditions preserve mark identity and atomic progress; the remaining missing-record error is isolated in C5 (`crates/custodian/src/retirement/marks.rs:251`; `crates/custodian/src/retirement.rs:444`). |
| T3 Runtime | PASS | Per-pass work is bounded, step timestamps use the deployed loop's clock, and seeded tests exercise both drain/GC winners (`crates/custodian/src/retirement.rs:177`; `crates/custodian/src/retirement.rs:427`; `crates/server/src/custodian.rs:538`; `reviewer-ci.log:3873`; `reviewer-ci.log:3889`). |
| T4 Contribution | FAIL | One confirmed review defect remains unresolved; the three frozen review findings describe this same C5 defect. Contribution-artifact checking is N/A until its required publish rerun (`gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`; `AGENTS.md:221`). |
| T5 Judgment | PASS | The implementation addresses absent draining and stale timestamps directly, without a capability-probe workaround; path-based prior-art checks ran, and the accepted #1013/#1014/#660 scope deferrals remain settled (`crates/custodian/src/retirement.rs:423`; `brief.md:149`; `reviewer-prior-art.log:1`; `reviewer-rejected-paths.log:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | After C5 is corrected, decide whether the documented operator-repair holds and deferred residue cleanup meet rollout needs, and select Tier-1/Tier-2 follow-up observation; automated lifecycle tests do not establish operational suitability (`docs/design/architecture/06-runtime-view.md:90`; `AGENTS.md:79`). |

**The missing-record case silently discards an unfinished retirement.** This is one defect with two affected walkers, not three separate defects as the frozen batch-review summary counts it. All source citations above and below resolve against the supplied `$PDCA_TARGET`; evidence-log citations resolve in this review directory.

`parts_all` checks for a missing cursor record only while iterating a nonempty page (`crates/custodian/src/retirement/marks.rs:569`). `segments` has the equivalent check inside its loop (`crates/custodian/src/retirement/marks.rs:781`). When the partially marked record is the last record and disappears, the page is empty, both checks are bypassed, and each walker returns `Tail::Ended` (`:590`, `:839`). `Walk::finish` then returns a step with `value: None` (`crates/custodian/src/retirement/marks.rs:470`), which deletes the obligation (`crates/custodian/src/retirement.rs:457`). This contradicts the explicit absent-record hold (`crates/custodian/src/retirement/marks.rs:71`) and the standing rule against silently accepting absent entries (`AGENTS.md:190`).

Two independent counterexamples use the submitted test rig against unchanged production code. Each starts with 240 real fragment positions, runs `reconcile_step` to write 233 marks and persist `cursor.item = 233`, then removes the final naming record to model damage or an operator edit. The next step returns `Changed`, removes the obligation, leaves only 233 marks, and emits no hold. Both preservation assertions fail (`reviewer-counterexamples.log:6`, `reviewer-counterexamples.log:14`). This does **not** claim that normal writers delete these fenced records; it demonstrates the damaged-state containment failure that the new code explicitly promises to handle. The seven unmarked positions lose their retirement obligation, while already marked positions lose its keyed protection.

The builder should check an empty page against a nonzero stored item before declaring the range exhausted, retain the obligation byte-identically, and emit the existing typed/audited hold. Add regressions for both the `{session, all}` and segmented-generation cases. The standalone counterexamples are in `pdca-reviewer-1062-repro/tests/counterexamples.rs`; they can be rerun from this directory with:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-1062-repro/build" \
TMPDIR="$PWD/pdca-reviewer-1062-tmp" \
cargo test --offline --manifest-path pdca-reviewer-1062-repro/Cargo.toml \
  --test counterexamples reviewer_missing_cursor -- --nocapture
```

**The supplied tests establish real red→green; the complete local CI command is not green.** The reviewer ran `cargo test -p wyrd-custodian --test multipart_retirement_marks` against the supplied disposable checkout, stashed the tracked patch, reran with the new test file retained, restored the stash, and reran both retirement suites. Results were 13 green → 13 assertion failures → 13 green, with the 19 peer tests also green. Tracked changes and both added source files were verified byte-identical to their supplied versions afterward. `git diff --check` passed. No proposed fix was applied.

The independent `cargo run --offline --quiet -p xtask -- ci` invocation built the workspace and exercised its tests and scanners. Spellcheck, documentation lint/render/link audit, repository guards, dependency-use scanning, conformance and statics checks passed. The 50-seed DST invocation passed, including all 35 custodian tests and both new retirement-race tests (`reviewer-ci.log:3905`; seed configuration: `xtask/src/main.rs:1780`). It exited 1 because rustfmt/clippy are absent and the synthetic one-commit checkout lacks the `.github/advisory-db` gitlink required by the advisory scanner and one xtask test (`reviewer-ci.log:22`, `:28`, `:3528`, `:3985`). These are reviewer-host/checkout caveats, not patch defects. The frozen logs supply the unavailable checks' round evidence; no tool was replaced with a success shim.

| Frozen rule | Adjudication |
|-------------|--------------|
| C4-ci | PASS on the second recorded attempt, including full CI and DST (`gate-logs/C4-ci.log:7687`). The first attempt failed in the unchanged health-probe test (`:3066`); the retry passed. This is frozen evidence, not a claim that the independent full CI rerun was green. |
| C4-verify | PASS, independently reproduced; the log records all 13 pre-fix assertion failures and the green run (`gate-logs/C4-verify.log:10`, `:133`). |
| C4-diff-cov | Coverage was **not measured**: the wrapper could not apply this stacked patch on its `origin/main` base (`gate-logs/C4-diff-cov.log:10`). The supplied folded target builds and runs; this is a base-state caveat, not a C4 patch failure. |
| C5-mutants | The log reports 51 missed, 85 caught, 26 unviable and two timed-out mutants (`gate-logs/C5-mutants.log:66`). Survivors include telemetry, budget arithmetic and cursor-boundary checks; they are not 51 independently established defects. The missing-record branch is independently confirmed above. The long mutation campaign was adjudicated from its frozen log, not rerun. |
| T4-batch-review | FAIL: its three entries duplicate the same empty-page defect in the two walkers (`gate-logs/T4-batch-review.log:10`); both variants were independently reproduced. |
| T4-contribution | N/A: PR/commit artifacts are absent by design at Check; their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS from the captured real clippy invocations for both the TiKV crate and server selection arms (`gate-logs/host-tikv.log:2`, `:211`); local clippy is unavailable. |

The brief's external tools, `typos` and `docs-renderer`, actually ran successfully in this review (`reviewer-ci.log:2`, `:10`). The target contains the folded prerequisite functionality and supports the red/green experiment. No unmet external service or curated topology is being substituted for the claimed drain/GC race: the seeded model reaches both winners, and the real redb-based tests exercise production reconciliation and GC.

**Prior-art searches covered all nine affected paths.** GitHub default-branch commit history was queried by each exact file path (`reviewer-prior-art.log`), followed by closed-PR body searches (`reviewer-closed-paths.json`) and explicit closed/unmerged searches (`reviewer-rejected-paths.log`). The retirement implementation/test paths have no default-branch history yet, consistent with the folded prerequisite and the coverage wrapper's lagging base. Existing GC/restore/DST history includes the staged-protection, reclaim-intent and restore-fence work. The closed/unmerged matches identify [PR #647](https://github.com/getwyrd/wyrd/pull/647); its maintainer [closed it for excessive scope](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571). [Issue #636](https://github.com/getwyrd/wyrd/issues/636) is closed as not planned, as the brief states. Those scope decisions are already addressed by this accepted split; the unpublished #1012 attempt is treated only as the prior-art account in `brief.md:223`, not as independently reproduced evidence (`reviewer-prior-disposition.json`). No settled deferral is re-raised.

Operationally, Tier-1 disk-fault and Tier-2 kill/reconstruct observation are warranted follow-ups for this reclamation change, as the standing rubric requests; they were not run here and are not claimed as merge gates (`AGENTS.md:79`). Fitness approval remains with the human after the implementation defect is corrected.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1062 (retirement drain, `retire:bytes:` marking mode)

I re-ran the red→green proof in a scratch copy of `$PDCA_TARGET`. All 13 tests in `crates/custodian/tests/multipart_retirement_marks.rs` pass, and they go through the production `reconcile_step` and real GC passes. The C4-verify red (all 13 fail on child-1's base) only shows that the feature was missing there. It does not prove each defect separately. The caught mutants do cover the per-defect reds the brief names (arm 0, segments-first, owned residue). I also added scratch-only probe tests and a mutant. That gave three findings: one is the T4 blocker, now shown with a concrete case; two are new.

- NEEDS-HUMAN [impl] — **An empty page with a partial cursor silently finishes the obligation and leaves positions unmarked.** This is the T4 blocking finding, now reproduced. At `crates/custodian/src/retirement/marks.rs:589-594` (`parts_all`) and `:838-843` (`segments`), an empty page returns `Tail::Ended` even when the stored cursor is `(entry, item > 0)`. `Walk::finish` (`:470-477`) then deletes the obligation. The non-empty-page branch handles the same state correctly as a `RecordUnreadable` hold (`:569-581`, `:781-801`). Concrete case, run on the patch: a production `abort_session` with parts 1–3. Set the cursor to `{parts, entry 3, item 1}`, mark part 3's position 0 with this event, then delete part 3's `part:` record (damage or a foreign delete). One `reconcile_step` returns `Changed`, the obligation is gone, part 3's positions 1 and 2 (chunk 30300, servers 1 and 2) have no mark, and no audit event fires. That is "silent success" under the rubric's *Absent or unsupported entries* rule. GC will never reclaim those fragments, because it keeps any unmarked fragment (`gc.rs:736-738`). Fix: when the page is empty, `last` is `None` and `at` is `Some((e, i > 0))`, take the same `held_at(e, i, RecordUnreadable)` path the non-empty branch takes, in both functions. Add the case above as a test.

- NEEDS-HUMAN [human] — **A drain mark over a position with no fragment is never deleted.** The fragment-less sweep keeps every structured, non-`reclaiming` mark that carries an event (`crates/custodian/src/gc.rs:901-904`, `event-may-await-write`). Every mark this drain writes is one of those (`marks.rs:141-142`, `:371-374`). The brief's (C′) leg and the DST row both *require* the drain to write a fresh arm-(c) mark after GC has destroyed the fragment and deleted its key. That end state is a mark over nothing, with no deleter. Concrete case, run on the patch: the (C′) arm-0 flow on a `{chunks}` obligation over chunk 31000. After the drain finishes, `orphan:1:31000:1 = {"orphaned_at_millis":10000000,"event":"s:…:0:1:…a77e001f"}` is still there after six GC passes spread over 24 simulated hours (past both grace and `LATE_WRITE_DEADLINE_MILLIS`). The same leak hits any drained position whose fragment never landed or was lost. This contradicts proposal 0016's "every mark this protocol writes has a deleter whether or not its position is ever written" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1408`). The new docs paragraph (`docs/design/architecture/06-runtime-view.md:90`, "the drain writes a fresh one after") does not say who deletes the fresh mark. A human needs to choose: either GC's sweep adds retirement-event marks whose `retire:bytes:` obligation is absent (one keyed read, as `retirement_draining` already does), or the leak is accepted and tracked in an issue. Changing GC is outside what this brief scoped, so this is not a builder iteration.

- NEEDS-HUMAN [impl] — **The paging paths have no test, and a surviving mutant stalls the drain.** C5 reports 51 surviving mutants, and C4-diff-cov measured nothing (the bundle is stale against `origin/main`). Every new test has at most 7 entries, so the multi-page paths never run: `parts_set` beyond 16 members (`marks.rs:515-518`), `parts_all` paging (`:556-599`) and `segments` paging (`:782-783`, `:838-843`). Finding 1 sits in these same paths. Concrete case: I applied the surviving mutant at `marks.rs:481` (`&&` → `||`). All 13 shipped tests still pass. A probe that aborts a session with 20 one-chunk parts then never drains: the step that deletes parts 1–16 writes no marks, so the mutant answers `Wait` and nothing commits. Without the mutant, the same probe drains in 4 passes. Fix: add a test that aborts a session with more than 16 parts, and one for a segmented generation with more than 16 segments. Separately, the (C′) arm-0 fixture puts the `reclaiming` mark at position 1 (`multipart_retirement_marks.rs:942`), so its first step always makes progress, and no test checks the wait-versus-hold choice. That is why mutants `marks.rs:379` (`entry.retained` → `true`: every `reclaiming` mark gets audited as "needs a human") and `:484` (the `AwaitingGc` arm deleted) survive. No test asserts `Reconciled::Partial` anywhere. Fix: put the mark at position 0 and assert `Partial` with an empty audit capture.

## Attacks that failed

I tried these and could not break them:

- **(T) stamp.** `retirement.rs:427` reads `ctx.clock` once per step, after the obligation's bytes are in hand, and reads it again after a `Conflict`. The deployed `LoopClock` (`crates/server/src/custodian.rs:134-167`) is the same source as GC's `now_millis`. GC's grace test uses `saturating_add` (`gc.rs:682-688`), so a stamp later than GC's pass reading is held, never reclaimed early.
- **Arm 0 ordering.** `read_arm` (`marks.rs:255-262`) checks `is_reclaiming()` before it compares events, so this obligation's own `reclaiming` mark cannot reach arm (a).
- **The DST row.** I instrumented it and ran it under madsim. It reaches a real compare-and-swap race in both directions: with the drain starting at 0–2 ms, GC's swap loses its precondition on the stale bytes; at 3–5 ms, the drain's re-stamp loses, and its re-read meets `reclaiming`. Optional hardening: the row never asserts that a commit lost, so a timing change could reduce it to back-to-back runs without failing.
- **(G) segments-first.** The 7 parts finish in 2 steps and the 1,100 segments need 3, so removing the check does turn the test red. One gap I checked: a Complete release followed by an Abort leaves the `{seg}` obligation at epoch E+1 and the bytes obligation at E+2, and `segments_drained` (`marks.rs:226-235`) only looks up its own epoch. Restore's `recheck_fenced` only examines the group at E′−1 (`restore.rs:1165-1200`), so restore reports nothing false. Not a finding.
- **Arm (b) against another obligation's mark.** Two drains can re-stamp each other's marks back and forth, but each re-stamp only refreshes the stamp, so grace only gets longer. That is the safe direction.
- **Placement change between steps.** A repaired `seg:` placement can leave a moved position unmarked when the record is deleted. GC then never reclaims it without a mark of its own, which is the safe direction.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/marks.rs:590` and `crates/custodian/src/retirement/marks.rs:839`: Preserve the obligation when an empty page loses a partially marked record. If a previous step stored `cursor.item > 0` and that last part or segment subsequently disappears, the loop never executes its missing-record check. Both walkers return `Tail::Ended`; `Walk::finish` then deletes the obligation at `crates/custodian/src/retirement/marks.rs:470`, silently abandoning the remaining unmarked positions. Return the existing `RecordUnreadable` hold for this case and add regressions for both range walkers. This confirms the frozen batch-review finding; its three entries describe the same defect.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:6721` and `crates/dst/tests/custodian.rs:6793`: The race test does not require competing commits against the same stale mark. Its winner XOR and campaign flags also pass when one run lets the drain finish before GC reads, and another lets GC swap before the drain first reads `reclaiming`. Neither requires a failed conditional commit or the conflict-triggered re-read the scenario is meant to test. Use the existing `RaceEvent::Commit` outcomes to require a witnessed race with both stale-value attempts, exactly one `Committed` and one `Conflict`; for the GC-wins case, assert the drain's subsequent re-read observes `reclaiming` before GC deletes its key.

No additional actionable reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Preserve the obligation and report `RecordUnreadable` when a nonzero cursor's final naming record disappears—both walkers currently erase unfinished work with seven positions still unmarked (`crates/custodian/src/retirement/marks.rs:590`; `crates/custodian/src/retirement/marks.rs:839`; `reviewer-counterexamples.log:6`).
- [ ] Validation — fitness-to-purpose — After C5 is corrected, decide whether the documented operator-repair holds and deferred residue cleanup meet rollout needs, and select Tier-1/Tier-2 follow-up observation; automated lifecycle tests do not establish operational suitability (`docs/design/architecture/06-runtime-view.md:90`; `AGENTS.md:79`).
- [ ] **An empty page with a partial cursor silently finishes the obligation and leaves positions unmarked.** This is the T4 blocking finding, now reproduced. At `crates/custodian/src/retirement/marks.rs:589-594` (`parts_all`) and `:838-843` (`segments`), an empty page returns `Tail::Ended` even when the stored cursor is `(entry, item > 0)`. `Walk::finish` (`:470-477`) then deletes the obligation. The non-empty-page branch handles the same state correctly as a `RecordUnreadable` hold (`:569-581`, `:781-801`). Concrete case, run on the patch: a production `abort_session` with parts 1–3. Set the cursor to `{parts, entry 3, item 1}`, mark part 3's position 0 with this event, then delete part 3's `part:` record (damage or a foreign delete). One `reconcile_step` returns `Changed`, the obligation is gone, part 3's positions 1 and 2 (chunk 30300, servers 1 and 2) have no mark, and no audit event fires. That is "silent success" under the rubric's *Absent or unsupported entries* rule. GC will never reclaim those fragments, because it keeps any unmarked fragment (`gc.rs:736-738`). Fix: when the page is empty, `last` is `None` and `at` is `Some((e, i > 0))`, take the same `held_at(e, i, RecordUnreadable)` path the non-empty branch takes, in both functions. Add the case above as a test.
- [ ] **A drain mark over a position with no fragment is never deleted.** The fragment-less sweep keeps every structured, non-`reclaiming` mark that carries an event (`crates/custodian/src/gc.rs:901-904`, `event-may-await-write`). Every mark this drain writes is one of those (`marks.rs:141-142`, `:371-374`). The brief's (C′) leg and the DST row both *require* the drain to write a fresh arm-(c) mark after GC has destroyed the fragment and deleted its key. That end state is a mark over nothing, with no deleter. Concrete case, run on the patch: the (C′) arm-0 flow on a `{chunks}` obligation over chunk 31000. After the drain finishes, `orphan:1:31000:1 = {"orphaned_at_millis":10000000,"event":"s:…:0:1:…a77e001f"}` is still there after six GC passes spread over 24 simulated hours (past both grace and `LATE_WRITE_DEADLINE_MILLIS`). The same leak hits any drained position whose fragment never landed or was lost. This contradicts proposal 0016's "every mark this protocol writes has a deleter whether or not its position is ever written" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1408`). The new docs paragraph (`docs/design/architecture/06-runtime-view.md:90`, "the drain writes a fresh one after") does not say who deletes the fresh mark. A human needs to choose: either GC's sweep adds retirement-event marks whose `retire:bytes:` obligation is absent (one keyed read, as `retirement_draining` already does), or the leak is accepted and tracked in an issue. Changing GC is outside what this brief scoped, so this is not a builder iteration.
- [ ] **The paging paths have no test, and a surviving mutant stalls the drain.** C5 reports 51 surviving mutants, and C4-diff-cov measured nothing (the bundle is stale against `origin/main`). Every new test has at most 7 entries, so the multi-page paths never run: `parts_set` beyond 16 members (`marks.rs:515-518`), `parts_all` paging (`:556-599`) and `segments` paging (`:782-783`, `:838-843`). Finding 1 sits in these same paths. Concrete case: I applied the surviving mutant at `marks.rs:481` (`&&` → `||`). All 13 shipped tests still pass. A probe that aborts a session with 20 one-chunk parts then never drains: the step that deletes parts 1–16 writes no marks, so the mutant answers `Wait` and nothing commits. Without the mutant, the same probe drains in 4 passes. Fix: add a test that aborts a session with more than 16 parts, and one for a segmented generation with more than 16 segments. Separately, the (C′) arm-0 fixture puts the `reclaiming` mark at position 1 (`multipart_retirement_marks.rs:942`), so its first step always makes progress, and no test checks the wait-versus-hold choice. That is why mutants `marks.rs:379` (`entry.retained` → `true`: every `reclaiming` mark gets audited as "needs a human") and `:484` (the `AwaitingGc` arm deleted) survive. No test asserts `Reconciled::Partial` anywhere. Fix: put the mark at position 0 and assert `Partial` with an empty audit capture.
- [ ] `crates/custodian/src/retirement/marks.rs:590` and `crates/custodian/src/retirement/marks.rs:839`: Preserve the obligation when an empty page loses a partially marked record. If a previous step stored `cursor.item > 0` and that last part or segment subsequently disappears, the loop never executes its missing-record check. Both walkers return `Tail::Ended`; `Walk::finish` then deletes the obligation at `crates/custodian/src/retirement/marks.rs:470`, silently abandoning the remaining unmarked positions. Return the existing `RecordUnreadable` hold for this case and add regressions for both range walkers. This confirms the frozen batch-review finding; its three entries describe the same defect.
- [ ] `crates/dst/tests/custodian.rs:6721` and `crates/dst/tests/custodian.rs:6793`: The race test does not require competing commits against the same stale mark. Its winner XOR and campaign flags also pass when one run lets the drain finish before GC reads, and another lets GC swap before the drain first reads `reclaiming`. Neither requires a failed conditional commit or the conflict-triggered re-read the scenario is meant to test. Use the existing `RaceEvent::Commit` outcomes to require a witnessed race with both stale-value attempts, exactly one `Committed` and one `Conflict`; for the GC-wins case, assert the drain's subsequent re-read observes `reclaiming` before GC deletes its key.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FLAKY — first run fail, confirm re-run pass (attempts: fail → pass); recorded pass (both runs in gate-logs/C4-ci.log). Confirm the red sample was environmental, not the patch — xtask ci: all checks passed
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1062/review-
- [ ] **The reclaiming test manually removes a possible permanent hold.** `brief.md:193-201` retains a naming record until its position carries this retirement's mark, waits on `reclaiming`, and assumes a later pass finds the key gone. For a session with a retained `part:` record naming that position, however, GC's staged protection outranks even `reclaiming` and prevents fragment deletion (`crates/custodian/src/gc.rs:654-668`); its fragment-less sweep also preserves a staged-protected key (`gc.rs:887-894`). Thus the seeded state can leave the drain waiting for GC and GC waiting for removal of the part. Criterion C′ explicitly deletes the key from the fixture (`brief.md:47-50`), bypassing this interaction. Add a case alternating drain steps with real GC over the retained-part state, and specify its expected resolution. If this is intentionally an operator-repair hold or excluded as unreachable under stated prerequisites, say so and qualify the promise to drive “every” obligation to empty (`brief.md:89-90`); do not imply GC alone guarantees recovery.
- [ ] **The acceptance cases do not discriminate the promised coverage of every retirement shape.** Scope includes `{chunks}` and both generation forms (`brief.md:104-106`), which are distinct inputs in the target's writer table (`crates/core/src/multipart.rs:3769-3771`). Yet B requires only unnamed parts, R requires a session hold, and S permits “a part or generation” while requiring a segmented-generation placement check (`brief.md:32-65`); T and C′ leave the obligation shape unspecified. All prescribed cases could therefore use parts and segmented generations while `{chunks}` and flat-generation draining remain unimplemented. Require explicit successful completion cases for those two rows: expected fragment positions marked, obligation removed, and actual reclamation after grace. Also distinguish successful empty-residue session completion from R's intentionally held session. These are checks of the existing scope, not additional features.
- [ ] size backstop — this slice is behaving oversized: patch is 207 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human decision: fix all the bugs the Check review found, including the GC mark leak. 1. Empty page with a partial cursor (all three reviewers): in `parts_all` (marks.rs:589-594) and `segments` (:838-843), when the page is empty and the stored cursor is (entry, item > 0), hold with `RecordUnreadable` (keep the obligation byte-identical, typed outcome plus audit event) instead of `Tail::Ended`. Add a regression test for each walker ({session, all}/parts, and segmented generation). 2. Drain marks over a position with no fragment are never deleted (adversary finding 2): the human authorizes the GC change. GC's fragment-less sweep must remove a retirement-event mark once its `retire:bytes:` obligation is gone (one keyed read, as `retirement_draining` does), so every mark the drain writes has a deleter (0016:1408). Test: the (C') arm-0 flow ends with no orphan mark left after grace. Update 06-runtime-view.md to say who deletes the fresh mark. 3. Paging paths untested: add tests that abort a session with more than 16 parts, and that drain a segmented generation with more than 16 segments (this kills the surviving mutant at marks.rs:481). Move the (C') `reclaiming` mark to position 0 and assert `Reconciled::Partial` with no audit event (kills mutants at :379 and :484). 4. DST race row (crates/dst/tests/custodian.rs:6721, :6793): require a real race using `RaceEvent::Commit` outcomes (both stale-value attempts, exactly one `Committed` and one `Conflict`). In the GC-wins case, assert the drain's re-read sees `reclaiming` before GC deletes the key.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
