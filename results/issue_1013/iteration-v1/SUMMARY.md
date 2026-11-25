# Result — issue 1013 / multipart-session-terminal-delete

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1 a session's `retire:` obligations drain, but nothing ends the
  session: its owned `sidx:` residue (crashed in-flight intents from #657's staging) stays and
  keeps its fragments protected; its `mpu:` and `slot:` records stay; and `mpuctl.count` only
  rises, until every Create answers `503`. And an untrusted staged record on a dead upload (a
  placement that fails validation, or an `sidx:` value that will not decode) is held for ever:
  child-1 holds it, GC holds every fragment of its chunk, and the session can never reach its
  exit. This child adds the owned `sidx:` walk, retirement of untrusted records by chunk id for
  `Aborting` sessions, and the terminal session delete with its exactly-once `mpuctl` decrement,
  dispatched from the drain leg child-1 added to `reconcile_step`.
- Success criterion: A NEW test file
  `crates/custodian/tests/multipart_session_terminal.rs`, over `RedbMetadataStore::in_memory()`
  and a real chunk store or placement-aware double, driving everything **through
  `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`). It shows all of:
  **(D) Owned walk and terminal delete.** An `Aborting` session holding committed parts and
  crashed in-flight `sidx:` residue, and a `Completed` session holding in-flight residue, each
  drain to: every owned fragment marked from its `sidx:` placement, then the entry deleted; no
  `sidx:` record of the session left. **`Aborting`:** the terminal delete fires **once**,
  `mpuctl.count` down by exactly one (two drainers racing the terminal delete through a wrapper
  still decrement once); no `slot:` record left; the `seggrp:` marker deleted when no `seg:`
  record names the nonce and **kept** when one does (`0016:674`). **`Completed`:** with the
  contexts the deployed loop passes, the `mpu:` record (the tombstone) is **kept** after its
  obligations and residue drain, and an identical Complete retry still answers
  `AlreadyCompleted` (`crates/core/src/multipart.rs:4789-4795` on `12dd082`); with the drain
  context's tombstone input saying the window has passed, the terminal delete fires once, as for
  `Aborting`. The terminal delete does not fire while the session's `sidx:` range is non-empty,
  nor while a session-scoped `retire:` obligation is left.
  **(E) Untrusted records.** For an `Aborting` session, each of these is retired **by chunk id**
  on a pass whose fleet is the whole operator-configured fleet: (E1) a `part:` record whose
  placement has the wrong length under a supported scheme (an RS(4,2) chunk placed on 5 servers;
  the maintainer's case); (E2) an owned `sidx:` entry with the same wrong-length placement (it
  decodes: `StagedPlacement` admits a length mismatch, `crates/core/src/multipart.rs:4441` on
  `db1a066`); (E3) an owned `sidx:` value that will not decode (e.g. an unsupported scheme, which
  `StagedPlacement` refuses, `multipart.rs:4481`), its chunk id taken from its key
  (`parse_sidx_key`, `multipart.rs:1436`) exactly as GC's hold takes it
  (`crates/custodian/src/gc.rs:1392`). For each: every fragment bearing that chunk id that the
  whole-fleet listing finds is marked, and for E2 every position the entry itself names as well;
  then the record is deleted (a recording wrapper shows the order); after GC past grace those
  fragments are gone and the terminal delete has fired.
  **Late landing** (a wrapper forces listing → record deletion → fragment publication): for E2,
  a fragment that lands at a position the entry names finds this retirement's mark and is gone
  after grace; for E3, a fragment that lands after the listing stays on disk across GC passes,
  never reclaimed and never an error: the stated residual (Design).
  **Controls:** an `Open` session with the same kind of record keeps the record and every
  fragment of its chunk; a committed object's fragments on the same servers are untouched; a
  server left out at startup, and a server unreachable this pass (each leaving the fleet short of
  `operator_fleet_size`), each leave the record and every fragment of its chunk in place with a
  typed hold, and once the server is back and the fleet is whole the record retires and that
  server's fragments are gone after grace too; a `part:` value that will not decode (an
  unsupported scheme such as `RS(0,0)`, which `decode_part_record` refuses) is held with a typed
  outcome and an audit event saying it needs a human, and the session's terminal delete does not
  fire.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the owned `sidx:` walk for `Aborting` and `Completed` sessions; retirement by chunk
  id of untrusted `part:` and `sidx:` records for `Aborting` sessions, on whole-fleet passes only
  (replacing child-1's hold for that case); the terminal session delete and its exactly-once
  `mpuctl` decrement; the tombstone input on the drain context, with the deployed caller passing
  "retain"; the docs. /
  out of scope:
  - the drain step itself, its cursor, and the segments-before-parts rule (child-1);
  - the migration gate and the routing switch (child-3);
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the
    `W_tombstone` clock (#625); the operator abort (#633);
  - recovering a `part:` value that will not decode: its key names no chunk
    (`part:<id>:<n>`, `multipart.rs:1386`), so this child holds it (E's control);
  - seeded DST (#660 owns "two concurrent drainers"; add no DST scenario);
  - the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 117 mutants tested in 42m: 55 missed, 43 caught, 17 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.53s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1013 multipart session teardown and untrusted-record retirement: two reproducible progress defects require builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required exit, exactly-once admission release, whole-fleet retirement and tombstone behavior are falsifiable and scoped; `brief.md:24`, `brief.md:70`, `brief.md:250`. |
| C2 Reproduction (red pre-fix) | N/A | This is a declared net-new enhancement; the retained discriminator does not compile on the base, so no behavioral pre-fix red is claimed; `brief.md:115`, `reviewer-red.log:4`. |
| C3 Change | PASS | The diff stays within session teardown and its existing reconciliation context, with corresponding caller/test updates and living documentation; `crates/custodian/src/retirement.rs:447`, `crates/server/src/custodian.rs:573`, `docs/design/architecture/06-runtime-view.md:92`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the declared behavioral-red exemption and frozen dependency-audit evidence: the base test never executes, the patched 15 tests pass, and the synthetic target lacks the advisory-db gitlink needed for a complete independent CI green; `reviewer-red.log:4`, `reviewer-ci.log:1828`, `reviewer-ci.log:3804`, `gate-logs/C4-ci.log:4965`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Restore progress across the mark-read limit: owned entries and untrusted parts can permanently retain their records and admission slot despite a whole fleet; three independent regression probes fail; `crates/custodian/src/retirement/sessions.rs:443`, `crates/custodian/src/retirement/marks.rs:520`, `reviewer-probes.log:72`. |
| T1 Structure | PASS | The maintenance layer retains trait-based stores, the shared mark guard and the existing dispatch seam; new stamps use the loop's clock; `crates/custodian/src/retirement.rs:362`, `crates/custodian/src/retirement/sessions.rs:356`, `crates/custodian/src/retirement/sessions.rs:447`. |
| T2 Shape | PASS | Batch accounting, canonical ledger encoding and living architecture documentation remain consistent; formatting, spelling and docs checks pass independently; `crates/custodian/src/retirement/sessions.rs:426`, `crates/core/src/multipart.rs:2091`, `reviewer-ci.log:2`, `reviewer-docs-render.log:3`. |
| T3 Runtime | FAIL | Ten passes leave 233 marks on each oversized owned entry and 232 on the mixed untrusted part, with count still 1 and subsequent outcomes Partial; reclamation and admission recovery stop; `reviewer-probes.log:45`, `reviewer-probes.log:51`, `reviewer-probes.log:59`, `crates/custodian/src/retirement/marks.rs:417`. |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art by affected file path: the supplied target has one synthetic commit and no remotes, while the brief supplies only the parent's older summary; `reviewer-history.log:1`, `brief.md:167`. |
| T5 Judgment | PASS | The late-write residual and tombstone boundary are explicit, and the tracked DST deferral is settled under the review protocol; no capability probe masking an eager-load cause was introduced; `docs/design/architecture/06-runtime-view.md:92`, `crates/custodian/src/retirement/sessions.rs:115`, `AGENTS.md:215`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept or reject permanent retention of late fragments whose positions cannot be recovered, and retaining Completed sessions' admission slots until #625 supplies the window: both affect operational capacity despite passing small-case tests; `docs/design/architecture/06-runtime-view.md:92`, `crates/server/src/custodian.rs:578`. |

Source citations are grounded in the supplied `$PDCA_TARGET`; brief, gate-log and reviewer-evidence paths are relative to this report. The patch was restored after the red-leg stash, and `git apply --reverse --check ../patch.diff` succeeds in the target. No production source was edited by this review.

1. **Owned entries larger than one mark-read budget never drain.** Every `walk_step` rereads the range from its head and starts each entry at its first position. Reading an already-owned mark still spends `reads_left`; at 233 reads, the loop exits before reaching later positions or deleting the entry. The next step repeats the same prefix (`crates/custodian/src/retirement/sessions.rs:342`, `crates/custodian/src/retirement/sessions.rs:426`, `crates/custodian/src/retirement/sessions.rs:443`). I exercised both a decodable damaged RS(4,2) entry naming 300 positions and a supported, correctly sized RS(128,128) entry naming 256 positions. Both stay at 233 marks after ten passes, keep the entry and keep `mpuctl.count == 1` (`reviewer-probes.log:45`, `reviewer-probes.log:59`). The decoder deliberately permits the first case's placement-length mismatch (`crates/core/src/multipart.rs:4848`). Preserve bounded work and mark-before-delete ordering while making successive steps reach the remainder; add regressions exceeding one step's budget.

2. **An untrusted part discards its saved within-record progress.** `Walk::part` extends the trusted positions with the whole-fleet listing, then calls `self.entry(&entry, 0)` regardless of `from`. Its session-state read leaves only 232 mark reads, which subsequent steps spend on the already-marked prefix (`crates/custodian/src/retirement/marks.rs:501`, `crates/custodian/src/retirement/marks.rs:520`). A real Redb-backed session with a decoded profile permitting 80 chunks, 79 trusted RS(2,1) chunks and one wrong-length RS(4,2) chunk reproduces this through `reconcile_step`. All six servers are present, and all six fragments of the damaged chunk exist. After ten passes, the cursor remains `{entry:1,item:232}`, only 232 of 243 positions are marked, the part remains and admission count remains 1 (`reviewer-probes.log:10`, `reviewer-probes.log:51`). This is below the shipped 158-chunk part ceiling (`crates/core/src/multipart.rs:5765`, `crates/core/src/multipart.rs:5792`). Resume safely across changing listings without repeating a permanently budget-exhausting prefix; add a mixed-part regression spanning multiple steps.

The isolated reproduction crate copies the submitted test harness and adds only these three probes; it links the unchanged target crates. Its original 15 tests pass and all three probes fail (`reviewer-probes.log:72`). From this report's directory, reproduce with:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-1013-probe/build" TMPDIR="$PWD" cargo test --manifest-path pdca-reviewer-1013-probe/Cargo.toml --offline --test probe -- --nocapture
```

The gate evidence supports the following narrower conclusions:

- **C4-ci: frozen PASS; independent rerun has a target-snapshot caveat.** The frozen log includes actual typos, docs rendering, build/test, dependency-wall and DST output, ending successfully (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4323`, `gate-logs/C4-ci.log:4965`). My full `cargo xtask ci` rerun completes with two failures sharing the same cause: the snapshot's index has no `.github/advisory-db` gitlink, so its index test fails and the dependency audit cannot run (`reviewer-ci.log:3804`, `reviewer-ci.log:4451`; the check is `xtask/tests/advisory_db.rs:65`). That path is absent from the synthetic base as well and is untouched by this patch. Formatting, clippy, build, the new integration suite, conformance, statics, deployment checks and seeded DST run successfully. This is not a C4 patch defect. The brief's named external tools, `typos` and the docs renderer, were exercised without shims, including a successful 101-page render and link audit (`reviewer-docs-render.log:2`).
- **C4-verify: UNVERIFIABLE, independently reproduced.** Stashing production changes while retaining the new test produces the same three missing-API compiler errors as the frozen log; no test executes (`reviewer-red.log:4`, `gate-logs/C4-verify.log:41`). Restoring the patch gives 15/15 passing integration tests in the CI rerun (`reviewer-ci.log:1828`). The six requested counterfactual reds are not individually evidenced in the supplied logs; withheld builder notes were not consulted.
- **C4-diff-cov: not measured; base-state caveat.** The wrapper stops because the patch does not apply to its `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied applied target builds and passes its new tests. The coverage failure establishes neither a compile defect nor missing line coverage; refresh the gate's base before measuring it.
- **C5-mutants: recorded FAIL, inspected as test evidence.** The log reports 117 mutants: 55 missed, 43 caught, 17 unviable and two timeouts (`gate-logs/C5-mutants.log:70`). Budget and progress mutations survive, supporting scrutiny of these boundaries; the two findings above rest on executed counterexamples, not the survivor count. The instance-scoped mutation wrapper is not supplied, so its frozen output was used rather than claiming an independent mutation rerun.
- **T4-batch-review: two distinct implementation findings confirmed.** Its six progress findings collapse to the two defects above (`gate-logs/T4-batch-review.log:10`). The seventh finding, missing new seeded DST, is rejected for this review: the brief explicitly says to add no DST scenario and assigns it to #660 (`brief.md:112`), with an in-code deferral (`crates/custodian/src/retirement/sessions.rs:115`); the protocol treats tracked deferrals as settled (`AGENTS.md:215`). No agents were spawned for this review.
- **T4-contribution: N/A.** The contribution artifacts are intentionally drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). There is no human action owed for this deferred row.
- **host-tikv: PASS.** Both feature-clippy commands pass independently (`reviewer-tikv.log:103`, `reviewer-tikv.log:202`), consistent with the frozen evidence (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`).

The affected-path history investigation was run and returned only the harness's single base commit; it cannot establish merged history or closed/rejected work (`reviewer-history.log:4`). The earlier rejected #636 behavior is described in the brief, but a current path-based prior-art check remains owed. Tier-1 disk-fault and Tier-2 kill/reconstruct observation is warranted for custodian rollout; those optional campaigns were not run here. This report is advisory and does not gate acceptance.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1013 (owned `sidx:` walk, untrusted-record retirement, terminal session delete)

Verdict: **refuted in part.** The terminal delete and its exactly-once decrement hold up. Retirement by chunk id does not: for any damaged record with more fragment positions than one step can read (232 for a part, 233 for an owned entry), the session never ends, and nothing is held or audited. I reproduced both stalls by running `reconcile_step`, in a scratch copy of the patched tree (since deleted), with the patch's own test fixtures.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/marks.rs:520`: a damaged `part:` record is always walked from position 0 (`self.entry(&entry, 0)`), and the cursor's `from` is thrown away. Already-marked positions still cost one read each (`marks.rs:417-421`), so a part whose trusted and listed positions add up to more than about 232 re-reads the same prefix every step and never reaches the rest. **Failing case:** an `Aborting` session with one part of 40 RS(4,2) chunks: 39 on six servers and one on five, so 240 positions. At the default 1 MiB chunk that is a 40 MiB part, well under `MAX_PART_CHUNKS` = 158. Run through `rig.drain()`: pass 0 is `Changed` (232 marks), then 19 passes are all `Partial`. The obligation answers the `no-progress` wait every pass (`marks.rs:605`), with no hold and no audit event, and the part record, the obligation and the session all stay. **Control:** the same 40-chunk part with every placement valid drains and the session ends, because it goes through the cursor path. E1 (`tests/multipart_session_terminal.rs:1252`) plants a single-chunk part, which is why the suite is green. That breaks C-1 ("every session state … has an exit") for ordinary part sizes, and it is the rubric's *silent skip* class. Fix: resume from the cursor (trusted positions come first in record order, then the listed ones in `BTreeMap` order), or hold with a typed outcome when the positions exceed one step. Add a multi-chunk E1 leg. (The T4 batch review flagged this from reading the code; this run confirms it.)

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:443-444` with `:501-504`: the owned walk has the same flaw, and it is silent. Each step starts the range at its head, so an entry with more than `MARKS_PER_STEP` (233) positions uses the whole budget re-reading its own marks and hits `break 'entries`. The step then has `marked == 0 && deleted == 0` and returns `Walked::Stop` without pushing any hold, so the pass reports `Partial` for ever. **Failing case:** an `Aborting` session with an E2 entry (RS(4,2)) whose damaged placement names 240 servers; decode admits any length (`StagedPlacement`, `multipart.rs:4848`). After 10 passes the entry and the session remain and `session_holds` is empty. Because the walk always starts at the head of the range, that one entry also blocks every later entry of the session. Fix: at minimum, hold with a typed outcome (`Hold::*`) when an entry's positions exceed one step's reads, so an operator sees it.

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:377`: nothing tests the "`Aborting` only" half of the by-chunk rule for owned entries. The C5 mutant `match guard aborting → true` survived. I confirmed it: with the guard bypassed, every `wyrd-custodian` test still passes. The only "other state" control (`tests/multipart_session_terminal.rs:1400`) uses an `Open` session, and the session leg returns for `Open` at `sessions.rs:219` before reading anything, so that control cannot catch a broken guard. The one real other state is `Completed`. Add a `Completed` session (with `Tombstone::Elapsed`) holding a wrong-length entry and an undecodable one, and assert both are held and every fragment of their chunks is kept.

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement.rs:145-146` makes a cost claim that is wrong: "one `list_fragments` per configured server … at most once per step". `walk_step` lists the fleet (`sessions.rs:384`) before it knows whether anything will commit, and a step that commits nothing does not spend the step budget (that is only spent at `sessions.rs:506`). **Case:** 8 `Aborting` sessions, each with one undecodable entry held on an unreadable mark. Every pass made 48 listings (8 sessions × 6 servers) and 0 commits, and this repeats until a human acts. The real bound is held sessions × fleet size (up to 46 sessions at the shipped `MAX_SESSIONS`), not the step budget. The brief's *Production reach (b)* asks for exactly this per-pass cost. Low severity: either charge a listing against the step budget or state the true bound.

- NEEDS-HUMAN [impl] — the budget logic in the new walk is untested (C5: 55 missed mutants). Mutants survived on every budget check in `walk_step` (`sessions.rs:442-446`, `:476-491`), on both step counters (`:506`, `:634`), on the re-judge guards in `settle` (`:643`, `:651`), and on the session-listing page loop (`:152`). No test has more than 16 sessions, so a broken page loop would skip session 17 onward without anyone noticing. The two stalls above sit in exactly this untested area. Add tests that cross `MARKS_PER_STEP`, `SESSION_STEPS_PER_PASS` and `SESSION_PAGE`.

## Checked, could not refute

- **Exactly-once decrement** (`sessions.rs:565`, `:593`). In the race test the rival runs during the first drainer's `mpuctl` read, so the first drainer reads the already-lowered ledger. If the session-record precondition were dropped, its batch would commit and decrement a second time, so the test does tell the two apart. An unknown commit outcome becomes a `CommitFailed` hold and is re-read the next pass; there is no blind retry.
- **Whole-fleet gate** (`retirement.rs:390`). It mirrors the deployed GC gate (`crates/server/src/custodian.rs:688`). The only production builder of `ReconstructionContext` (`custodian.rs:564`) passes the real `operator_fleet_size`. I removed the gate and four fleet-control tests went red. I also dropped the entry's named positions, and `e2_…` went red. So those two "demonstrated reds" from the brief are real.
- **`reclaiming` holds.** GC holds an untrusted staged chunk whole (`gc.rs:1379-1386`), so for these entries a `reclaiming` mark really is one GC never finishes, and holding it is correct.
- **Not raised:** the walk's `<upload-id>:<epoch>` marks over positions where no fragment ever landed are never swept (`gc.rs:925-928`). That identity is the proposal's (`0016:672`), and the runtime-view doc states the effect. The missing seeded DST coverage is deferred to #660 (`sessions.rs:115`), so it is settled.

## Gate notes (not refutations)

- C4-verify reports UNVERIFIABLE because the test does not compile on the base without the new API; the brief declared this in advance. Two of the declared reds were checked above.
- C4-diff-cov never ran: `patch.diff` does not apply on `origin/main`, so the bundle needs a rebase before any coverage claim means anything.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/marks.rs:520`: Restarting every untrusted part at position zero permanently stalls parts exceeding the mark-read budget. The session lookup consumes one of 233 reads, so only 232 positions can be examined per step. For example, a 40-chunk RS(4,2) part with one wrong-length placement and all fragments present has 240 positions. The first step marks 232 and saves its cursor; subsequent steps ignore that cursor, spend all reads on the same already-marked prefix, and return `no-progress`. The remaining fragments, part record, retirement obligation and admission slot never drain. Preserve bounded progress safely across changing fleet listings, and add a regression that crosses this limit.

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:443`: The owned-entry walk has the same permanent stall for a single entry with more than 233 positions. A damaged RS(4,2) placement naming 234 positions is accepted by the decoder and enters the new chunk-id retirement path. Every step starts at its first position; even `Arm::Own` consumes a read, so after marking the first 233, every later step exhausts its budget before reaching position 234. No entry deletion or terminal session deletion can follow. Add resumable progress within an entry, retaining mark-before-delete and late-write coverage, and test an oversized damaged placement or fleet listing through repeated `reconcile_step` calls.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the declared behavioral-red exemption and frozen dependency-audit evidence: the base test never executes, the patched 15 tests pass, and the synthetic target lacks the advisory-db gitlink needed for a complete independent CI green; `reviewer-red.log:4`, `reviewer-ci.log:1828`, `reviewer-ci.log:3804`, `gate-logs/C4-ci.log:4965`.
- [ ] C5 Causal adequacy — Restore progress across the mark-read limit: owned entries and untrusted parts can permanently retain their records and admission slot despite a whole fleet; three independent regression probes fail; `crates/custodian/src/retirement/sessions.rs:443`, `crates/custodian/src/retirement/marks.rs:520`, `reviewer-probes.log:72`.
- [ ] T4 Contribution — Confirm merged and closed/rejected prior art by affected file path: the supplied target has one synthetic commit and no remotes, while the brief supplies only the parent's older summary; `reviewer-history.log:1`, `brief.md:167`.
- [ ] Validation — fitness-to-purpose — Accept or reject permanent retention of late fragments whose positions cannot be recovered, and retaining Completed sessions' admission slots until #625 supplies the window: both affect operational capacity despite passing small-case tests; `docs/design/architecture/06-runtime-view.md:92`, `crates/server/src/custodian.rs:578`.
- [ ] `crates/custodian/src/retirement/marks.rs:520`: a damaged `part:` record is always walked from position 0 (`self.entry(&entry, 0)`), and the cursor's `from` is thrown away. Already-marked positions still cost one read each (`marks.rs:417-421`), so a part whose trusted and listed positions add up to more than about 232 re-reads the same prefix every step and never reaches the rest. **Failing case:** an `Aborting` session with one part of 40 RS(4,2) chunks: 39 on six servers and one on five, so 240 positions. At the default 1 MiB chunk that is a 40 MiB part, well under `MAX_PART_CHUNKS` = 158. Run through `rig.drain()`: pass 0 is `Changed` (232 marks), then 19 passes are all `Partial`. The obligation answers the `no-progress` wait every pass (`marks.rs:605`), with no hold and no audit event, and the part record, the obligation and the session all stay. **Control:** the same 40-chunk part with every placement valid drains and the session ends, because it goes through the cursor path. E1 (`tests/multipart_session_terminal.rs:1252`) plants a single-chunk part, which is why the suite is green. That breaks C-1 ("every session state … has an exit") for ordinary part sizes, and it is the rubric's *silent skip* class. Fix: resume from the cursor (trusted positions come first in record order, then the listed ones in `BTreeMap` order), or hold with a typed outcome when the positions exceed one step. Add a multi-chunk E1 leg. (The T4 batch review flagged this from reading the code; this run confirms it.)
- [ ] `crates/custodian/src/retirement/sessions.rs:443-444` with `:501-504`: the owned walk has the same flaw, and it is silent. Each step starts the range at its head, so an entry with more than `MARKS_PER_STEP` (233) positions uses the whole budget re-reading its own marks and hits `break 'entries`. The step then has `marked == 0 && deleted == 0` and returns `Walked::Stop` without pushing any hold, so the pass reports `Partial` for ever. **Failing case:** an `Aborting` session with an E2 entry (RS(4,2)) whose damaged placement names 240 servers; decode admits any length (`StagedPlacement`, `multipart.rs:4848`). After 10 passes the entry and the session remain and `session_holds` is empty. Because the walk always starts at the head of the range, that one entry also blocks every later entry of the session. Fix: at minimum, hold with a typed outcome (`Hold::*`) when an entry's positions exceed one step's reads, so an operator sees it.
- [ ] `crates/custodian/src/retirement/sessions.rs:377`: nothing tests the "`Aborting` only" half of the by-chunk rule for owned entries. The C5 mutant `match guard aborting → true` survived. I confirmed it: with the guard bypassed, every `wyrd-custodian` test still passes. The only "other state" control (`tests/multipart_session_terminal.rs:1400`) uses an `Open` session, and the session leg returns for `Open` at `sessions.rs:219` before reading anything, so that control cannot catch a broken guard. The one real other state is `Completed`. Add a `Completed` session (with `Tombstone::Elapsed`) holding a wrong-length entry and an undecodable one, and assert both are held and every fragment of their chunks is kept.
- [ ] `crates/custodian/src/retirement.rs:145-146` makes a cost claim that is wrong: "one `list_fragments` per configured server … at most once per step". `walk_step` lists the fleet (`sessions.rs:384`) before it knows whether anything will commit, and a step that commits nothing does not spend the step budget (that is only spent at `sessions.rs:506`). **Case:** 8 `Aborting` sessions, each with one undecodable entry held on an unreadable mark. Every pass made 48 listings (8 sessions × 6 servers) and 0 commits, and this repeats until a human acts. The real bound is held sessions × fleet size (up to 46 sessions at the shipped `MAX_SESSIONS`), not the step budget. The brief's *Production reach (b)* asks for exactly this per-pass cost. Low severity: either charge a listing against the step budget or state the true bound.
- [ ] the budget logic in the new walk is untested (C5: 55 missed mutants). Mutants survived on every budget check in `walk_step` (`sessions.rs:442-446`, `:476-491`), on both step counters (`:506`, `:634`), on the re-judge guards in `settle` (`:643`, `:651`), and on the session-listing page loop (`:152`). No test has more than 16 sessions, so a broken page loop would skip session 17 onward without anyone noticing. The two stalls above sit in exactly this untested area. Add tests that cross `MARKS_PER_STEP`, `SESSION_STEPS_PER_PASS` and `SESSION_PAGE`.
- [ ] `crates/custodian/src/retirement/marks.rs:520`: Restarting every untrusted part at position zero permanently stalls parts exceeding the mark-read budget. The session lookup consumes one of 233 reads, so only 232 positions can be examined per step. For example, a 40-chunk RS(4,2) part with one wrong-length placement and all fragments present has 240 positions. The first step marks 232 and saves its cursor; subsequent steps ignore that cursor, spend all reads on the same already-marked prefix, and return `no-progress`. The remaining fragments, part record, retirement obligation and admission slot never drain. Preserve bounded progress safely across changing fleet listings, and add a regression that crosses this limit.
- [ ] `crates/custodian/src/retirement/sessions.rs:443`: The owned-entry walk has the same permanent stall for a single entry with more than 233 positions. A damaged RS(4,2) placement naming 234 positions is accepted by the decoder and enters the new chunk-id retirement path. Every step starts at its first position; even `Arm::Own` consumes a read, so after marking the first 233, every later step exhausts its budget before reaching position 234. No entry deletion or terminal session deletion can follow. Add resumable progress within an entry, retaining mark-before-delete and late-write coverage, and test an oversized damaged placement or fleet listing through repeated `reconcile_step` calls.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- [ ] **Criterion (E) conflates rejected geometry with a decoded placement mismatch, hiding a possible decoder-recovery change.** `brief.md:43-45` requires a "decoded `RS(0,0)`" example. The current part decoder rejects unsupported schemes (`crates/core/src/multipart.rs:2940-2947`, `:3180`, `:3208`), and `StagedPlacement` rejects them at construction/decode (`:4285-4287`, `:4306-4309`). Consequently an invalid-scheme `part:` makes GC's staged set incomplete, rather than merely holding its chunk (`crates/custodian/src/gc.rs:1405-1415`, `:1360-1361`). Revise (E) to use a supported scheme with the wrong placement length; test invalid-scheme `sidx:` separately through its key-derived chunk id. If recovering invalid-scheme parts is intended, explicitly scope how all their chunk ids are recovered without weakening the decoder. The current fixture cannot exercise the claimed path.
- [ ] **A listing cannot evidence fragments that land afterward.** `brief.md:154-158` marks only listed fragments and then deletes the untrusted record, but its invariant requires marking "every position its chunk could occupy" (`brief.md:55-57`). A staged write authorized before Abort can still publish after that listing: `crates/core/src/multipart/stage.rs:33-53` explicitly distinguishes abandoning a write from retracting it, and `crates/traits/src/lib.rs:1158` defines listing as the fragments currently held. If an undecodable `sidx:` is removed while its chunk is absent from the listing, a subsequent landing has neither staging evidence nor an orphan mark; GC keeps such an unevidenced fragment (`crates/custodian/src/gc.rs:736-738`). Specify durable coverage or a justified quiescence condition before deletion, and require a wrapper-forced listing → record deletion → fragment publication regression. Waiting out GC grace alone cannot repair a mark never created.
- [ ] **Chunk-wide retirement lacks an explicit whole-fleet prerequisite.** `brief.md:112-113` leaves fleet-listing integration open, while `:156-158` authorizes deleting the record after marking what those listings find. The deployed loop can have startup-omitted or temporarily unreachable servers; it deliberately gates GC on `fleet.len() == operator_fleet_size` to prevent deleting chunk-wide evidence while a missing server still holds fragments (`crates/server/src/custodian.rs:636-658`, `:680`). Neither the drain-context contract nor criterion (E) requires equivalent coverage. Specify that untrusted records and terminal deletion remain held unless every configured server's listing succeeds, or define durable evidence for omitted servers. Add startup-missing and runtime-unreachable controls followed by server return; otherwise the proposed recovery can strand exactly the bytes it promises to reclaim.
- [ ] size backstop — this slice is behaving oversized: patch is 198 KB (threshold 100 KB); patch touches 24 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): The design and the terminal delete with its exactly-once mpuctl decrement are sound; keep them. Fix the two confirmed stalls, both reproduced by the reviewer, the adversary and the code review: 1. marks.rs:520 — Walk::part restarts an untrusted part at position 0 (self.entry(&entry, 0)) and ignores the saved cursor `from`; parts with more than ~232 positions (e.g. 40 RS(4,2) chunks, one wrong-length) stall for ever as Partial with no hold or audit. Resume from the cursor safely across changing fleet listings (trusted positions in record order, then listed ones in BTreeMap order), or hold with a typed outcome. 2. sessions.rs:443 — the owned sidx: walk restarts each step at the head of the range and at each entry's first position; an entry with more than MARKS_PER_STEP (233) positions burns the budget re-reading its own marks, returns Walked::Stop with no hold, and blocks every later entry of the session. Make progress resumable within an entry, keeping mark-before-delete and late-write coverage. Add regressions through repeated reconcile_step calls that cross MARKS_PER_STEP (multi-chunk E1 leg, oversized E2 entry), SESSION_STEPS_PER_PASS and SESSION_PAGE (>16 sessions), to kill the surviving budget mutants (C5: 55 missed). Also: add a Completed-session (Tombstone::Elapsed) control holding a wrong-length and an undecodable sidx: entry, asserting both are held and their fragments kept (sessions.rs:377 aborting guard survived mutation); correct the cost claim at retirement.rs:145-146 (real bound is held sessions x fleet size) or charge listings against the step budget. Rebase onto current origin/main so diff coverage can be measured. Size is acceptable as one slice (most of the bulk is the test file and one-line context ripples); do not split.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
