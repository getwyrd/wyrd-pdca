# Result — issue 813 / staged-scrub-and-keep

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: two custodian loops ignore a multipart upload's staged bytes. **Scrub** walks only
  the committed reference set (`crates/custodian/src/scrub.rs:88`, grouped `:130-135`, fetched
  `:137-203`), so a fragment named by a committed `part:` record is never fetched or checked. Rot
  or loss during a staging window that can last hours never becomes a repair obligation, though
  0016 says scrub must check it with the scheme the part record carries (`0016:824`).
  **Reconstruction** resolves an obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk has no committed site, so `assess`
  returns `Drain` (`:613`), the chunk joins `drain_only` (`:218`) and its obligation is deleted
  (`:333-339`). The only record that the chunk is short a fragment is thrown away, and the pass
  can answer `Satisfied`.
- Success criterion: the NEW file `crates/custodian/tests/staged_scrub.rs` passes (legs A–C),
  legs D–F appended to the existing `crates/custodian/tests/staged_protection.rs` pass, and
  `cargo xtask ci` is green. All run over in-memory doubles. Records are seeded as raw JSON the
  base decoders accept (shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Legs:
  **(A) Scrub checks committed-part fragments.** An `Open` session has one committed `part:`
  record. Its chunk's fragment on one D server carries one flipped bit (`corrupt_fragment`,
  `crates/custodian/tests/scrub.rs:157-162`). One `reconcile_step` with a `ScrubContext` answers
  `Changed` and leaves that chunk in `wyrd_core::repair::queued_repairs`
  (`crates/core/src/repair.rs:151`). The same holds for a missing fragment, and for an intact
  fragment whose header names a different EC scheme from the part record's `ChunkRef` (this proves
  the part's scheme is the one checked). Control: with every fragment intact, nothing is queued
  and the pass answers `Satisfied`.
  **(B) Scrub leaves in-flight chunks alone.** A chunk named only by an owned `sidx:` entry (no
  `part:` record yet) with a fragment missing queues nothing: checking needs the committed scheme,
  which an in-flight chunk does not have yet (`0016:776-781`).
  **(C) Scrub fails closed on what it cannot read.** With one `part:` record whose value will not
  decode, the pass still checks every other fragment (A's corrupt chunk is still queued), names
  the record on the audit seam, and answers `Blocked` — scrub's rule for an unreadable committed
  map (`scrub.rs:99-116`, `:205-215`). A store fault while reading a session's `part:` range fails
  the pass with `Err`, as it fails GC (`docs/design/architecture/06-runtime-view.md:80`).
  **(D) Reconstruction keeps a staged chunk's obligation.** A committed part's fragment is lost
  and its chunk is enqueued (`enqueue_repair`). One `reconcile_step` with a
  `ReconstructionContext`: the obligation is still queued; the pass answers `Blocked`, as it does
  for a `seg:` repair it refuses (`reconstruction.rs:249-256`, `:341-358`); no D server received a
  write; the `part:` record is byte-identical. The same for an `sidx:`-only chunk. Control: an
  obligation for a chunk that no committed map and no staged record names still drains, and the
  pass answers `Satisfied`.
  **(E) Source before destination.** The pass reads the staged classes before the committed
  namespace, `sidx:` → `part:` → `inode:` (normative, `0016:782-800`; GC's order, `gc.rs:286`,
  `:301`). A store hook (`Meta::hook`, `staged_protection.rs:201`, as leg C uses it at
  `:1150-1468`) publishes the chunk — writes a committed inode naming it and deletes its `part:`
  record — right after the pass's first `inode:` read returns. The obligation must not drain. A
  pass that reads `inode:` first misses the chunk in both classes and drains it. Seed the chunk
  with one fragment lost: #814 drains an intact staged chunk as a duplicate finding, and this leg
  must stay green after it.
  **(F) An unreadable staged record holds back every drain.** With one `part:` record that will
  not decode, an obligation for a chunk that no class names is NOT drained, and the pass answers
  `Blocked`. This is the existing rule for an unreadable committed object
  (`reconstruction.rs:322-339`), applied to the staged read.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (1) scrub fetches and checks every fragment a committed `part:` record places, with
  the scheme that record carries, inside the loop it already runs (`scrub.rs:137-203`), and reads
  no `sidx:` entry. (2) Reconstruction reads the staged classes before the committed namespace and
  never drains an obligation for a chunk a staged record names: it keeps it, keeps it off the
  repairable-backlog gauge (`reconstruction.rs:175-199`) and names it on the audit seam, like the
  `seg:` refusal (`:1107`). An empty queue still reads nothing (`:161-169`). (3) The seam #814
  reads: `ReconstructionContext` (`reconstruction.rs:72-95`) gains
  `clock: &'a (dyn wyrd_testkit::Clock + Sync)` (ADR-0024, `docs/design/adr/0024-clock-and-time-source-trust.md`;
  `crates/testkit/src/lib.rs:23`) and `staged_write_window_millis: u64`. Use these names; #814's
  brief names them. The deployed loop passes the clock it already advances
  (`crates/server/src/custodian.rs:465-479`, context at `:502-509`) and a window value owned by
  `crates/server/src/cli.rs` as a `pub(crate) const` beside `LEASE_TTL_MILLIS` (`cli.rs:78`),
  passed down the way `GC_GRACE_WINDOW_MILLIS` feeds `GcContext::grace_window_millis`
  (`server/custodian.rs:114`, `gc.rs:200`). If a `W_write` constant already exists on the base
  (#800 names one), use it — never two definitions. Its doc comment states
  `G_orphan > W_repoint + W_write + δ_clock` (`0016:1348`) and that #800's late-write deadline
  must not be sized below it. `wyrd-testkit` moves from dev- to normal dependency where production
  code names `Clock` (as `crates/chunkstore-fs/Cargo.toml:17-19` has it). Update every existing
  construction site. Nothing in this slice reads either field. (4) Keep the prose true: leg F of
  `staged_protection.rs` (`:2035-2209`) now says scrub reads committed parts but no owned entry;
  the last sentence of `06-runtime-view.md:80` and the doc comments that say scrub and
  reconstruction read no staged record (`gc.rs:265`, `:873-876`; `staged_protection.rs:34`,
  `:2160`); narrow the `deferred: #663` marker at `gc.rs:893` to the rebuild, which #814
  removes. Reusing GC's staged reader (`staged_fragments`,
  `gc.rs:1033`; `walk_staged_range`, `:1069`) is expected; what GC and restore conclude must not
  change, so `staged_protection.rs` legs A–E stay green unedited.
  / out of scope: rebuilding or re-placing anything (#814); servers absent from the live fleet
  (scrub has only ever visited `ctx.fleet`, `scrub.rs:138`, and the deployed loop drops
  unreachable peers and reads around them, `server/custodian.rs:495-497` — the same for committed
  chunks today); drain status (#808); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.0% — 127 of 138 instrumentable changed lines executed (floor 80%); 138 of 536 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 31 mutants tested in 5m: 15 caught, 16 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_813/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.07s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #813: detect damaged committed multipart fragments and preserve staged repair obligations; no implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The detection-and-retention boundary is falsifiable through legs A–F and expressly leaves staged rebuilding to #814; brief.md:19, brief.md:121. |
| C2 Reproduction (red pre-fix) | PASS | Independent base runs fail by assertion: 9 scrub tests and 5 new reconstruction tests, plus the revised scrub isolation test; reviewer-red.log:228, reviewer-red.log:323. |
| C3 Change | PASS | The scoped change covers both lost detection and premature draining, including the four carried-forward corrections; crates/custodian/src/scrub.rs:123, crates/custodian/src/reconstruction.rs:210, crates/custodian/src/gc.rs:1464, crates/dst/tests/custodian.rs:3122. |
| C4 Verification (red→green) | PASS | Restoring the patch makes all 43 staged tests pass; independent workspace, DST and feature checks pass, with the advisory-audit host limitation adjudicated from the successful frozen log; reviewer-restored-green.log:40, reviewer-restored-green.log:57, gate-logs/C4-ci.log:3668. |
| C5 Causal adequacy | PASS | Reading the source before publication's destination closes the omission race, and staged membership prevents the erroneous no-reference conclusion; this removes the cause without a capability probe; crates/custodian/src/scrub.rs:123, crates/custodian/src/reconstruction.rs:727, crates/dst/tests/custodian.rs:3162. |
| T1 Structure | PASS | Shared paged readers, record decoders and strict staged placement validation preserve the trait boundary and GC/restore semantics; crates/custodian/src/gc.rs:1412, crates/custodian/src/gc.rs:1460, crates/custodian/src/gc.rs:1590. |
| T2 Shape | PASS | The deployed context is wired, the existing write-window definition is reused, and living architecture describes the limited repair behavior; crates/server/src/custodian.rs:132, crates/server/src/custodian.rs:533, docs/design/architecture/06-runtime-view.md:82. |
| T3 Runtime | PASS | Empty queues avoid namespace reads, staged scans remain paged, and incomplete staged readings withhold drains while preserving fault attribution; crates/custodian/src/reconstruction.rs:207, crates/custodian/src/reconstruction.rs:218, crates/custodian/src/reconstruction.rs:427, crates/custodian/src/gc.rs:1598. |
| T4 Contribution | N/A | Commit/PR artifacts are intentionally absent at Check; the substantive contribution audit must rerun at publish, as the deferred gate records; gate-logs/T4-contribution.log:10. |
| T5 Judgment | NEEDS-HUMAN | Confirm the unpublished #663/#637 rejection history imposes no remaining constraint on this split — all affected paths and closed PRs were checked, but those local rejection artifacts are not supplied; brief.md:146, brief.md:150, reviewer-prior-art-summary.txt:1. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept detection plus durable retention as the useful milestone while #814 owns rebuilding — staged losses remain queued and Blocked rather than having redundancy restored; brief.md:121, crates/custodian/src/reconstruction.rs:727, docs/design/architecture/06-runtime-view.md:82. |

The four carried-forward implementation concerns are discharged. Scrub's two publication schedules now retain the repair finding and assert that both publication batches actually land (`crates/custodian/tests/staged_scrub.rs:907`). Empty staged placement produces an attributed malformed-placement signal without fabricated repair targets (`crates/custodian/tests/staged_scrub.rs:980`). Reconstruction attributes an unreadable staged record before a subsequent inode-store fault (`crates/custodian/tests/staged_protection.rs:2588`). Its seeded handoff campaign preserves the specific moving chunk's obligation, drains an unreferenced control, and proves the publication windows were reached (`crates/dst/tests/custodian.rs:3046`, `crates/dst/tests/custodian.rs:3086`, `crates/dst/tests/custodian.rs:3135`). No tracked deferral is reopened.

Independent execution supports the verdict:

- Stashed the tracked fix in the harness's disposable target, kept the new scrub test, and retained the modified staged-protection suite with only the two new context-field initializers removed for base compatibility. Both suites compiled and failed by assertion: scrub 2 passed/9 failed; staged protection 26 passed/6 failed. After restoring the patch, the suites passed 11/11 and 32/32. Commands and failures are in `reviewer-red.log:1`; restored results are in `reviewer-restored-green.log:1`. Every patched file was then compared with `patch.diff`: all 17 matched exactly.
- `cargo xtask ci` independently passed typos, docs lint/render, hygiene guards, formatting, Clippy, build, workspace tests (1,382 passed, 14 ignored), and cargo-machete. It stopped at Cargo's read-only advisory-database lock (`reviewer-ci.log:2969`); an offline retry met the same host restriction (`reviewer-deny.log:1`). This is not a patch failure. The frozen log shows all three dependency-wall invocations passing and CI completing (`gate-logs/C4-ci.log:3049`, `gate-logs/C4-ci.log:3064`, `gate-logs/C4-ci.log:3075`, `gate-logs/C4-ci.log:3668`). Both declared external dependencies were exercised locally, including the renderer's 99-page output and link audit (`reviewer-ci.log:2`, `reviewer-ci.log:7`).
- Ran the remaining accessible checks separately: `cargo xtask dst` passed 78 tests under the configured 50-seed sweep, including both reconstruction handoff tests (`reviewer-dst.log:519`, `reviewer-dst.log:533`); conformance passed 5 valid and 6 invalid vectors (`reviewer-conformance.log:1`); the statics guard passed (`reviewer-statics.log:3`). Both TiKV/server feature-Clippy commands passed (`reviewer-tikv.log`).
- Read every frozen gate log. Instance-scoped wrapper evidence reports 92.0% diff coverage (127/138 instrumentable changed lines, 398 unscored: `gate-logs/C4-diff-cov.log:901`), 15 caught and 16 unviable mutants with none reported surviving (`gate-logs/C5-mutants.log:13`), and zero findings from the batch review (`gate-logs/T4-batch-review.log:10`). These are captured gate results, not claimed independent reruns; the mutant evidence does not establish that the 16 unviable mutations were exercised. The contribution gate remains N/A until publish.

The repository-history part of prior-art checking is complete. Queried default-branch history separately for all 17 affected paths, enumerated all 343 closed/merged PRs with complete changed-file pagination, and found no open PRs. Related work includes staged GC/restore protection in PR #807 and the closed segmented-map work in PR #647 (`reviewer-history-summary.txt:1`, `reviewer-prior-art-summary.txt:10`, `reviewer-prior-art-summary.txt:26`, `reviewer-open-prs.json:1`). The unpublished rejected iterations named by the brief remain the specific limit behind T5; no other checkout or builder notes were read.

Fitness remains a human decision, not an implementation failure. The in-memory and simulated tests can exhibit the forbidden loss and independently reproduce it on the base. Tier-1 disk-fault and Tier-2 kill/reconstruct observation is warranted for this durability surface, as the standing rubric requests (`AGENTS.md:79`, `AGENTS.md:81`); these environmental campaigns were not run here. Their ordinary fixtures do not replace the new staged-handoff evidence, and rebuilding staged content remains the settled #814 deferral.

Repository-relative source citations resolve under `$PDCA_TARGET` (`target/`); brief, patch and log citations resolve in this review directory. The target applied cleanly and was restored exactly; no stale-target caveat applies. The target contains no `INTEGRATION.md` with additional human-only items.

### Advisory — adversary

# Adversarial review — issue #813 (663.1: staged scrub + keep)

Verdict: the red→green evidence holds up. I re-ran it and it still stands. I found no input
that makes scrub or reconstruction lose a staged obligation. Two findings are about what the
patch sets up for #814 and for the window after publication, and one is about a gate result
that looks too fast to trust.

## Evidence I re-ran (in a scratch copy of `$PDCA_TARGET`)

- Green: `cargo test -p wyrd-custodian --test staged_protection --test staged_scrub` → 32 + 11 passed.
- Red for legs D–F, which C4-verify never proves (brief.md:64-72). I put back base `gc.rs`,
  `reconstruction.rs` and `scrub.rs`, kept the new tests, and removed the two new field
  initialisers at `crates/custodian/tests/staged_protection.rs:518-519` →
  **6 failed / 26 passed**, every one by assertion (`:2349`, `:2400`, `:2504`, `:2559`,
  `:2620`, and rewritten leg F at `:2265`). None failed to compile, and none failed for an
  unrelated reason.
- Read-order mutant (the round-1 defect class). In the patched code I moved the committed read
  ahead of the staged read in `crates/custodian/src/reconstruction.rs:210/221` and
  `crates/custodian/src/scrub.rs:123/152`. Caught by: `staged_protection.rs:2506` (leg H),
  `staged_protection.rs:2622` (name-before-fault leg), `staged_scrub.rs:922` (flip+drain
  between reads), and the DST sweep leg at gaps `[0,0,0]` (`crates/dst/tests/custodian.rs:3046`).
  The seeded DST leg misses it on one random seed but catches it under `MADSIM_TEST_NUM=50`
  (gaps `[17,10,1]`), which is the sweep the CI gate runs. The eight `REGRESSION_SEEDS` do not
  catch it. That is fine because the sweep does, but no committed seed pins this bug yet.
- Round-1 carry-forward. All four items are fixed, and each is guarded by a test that goes red
  on base: part-before-inode order in scrub, exact-length staged placement
  (`crates/custodian/src/gc.rs:1446-1464`, empty placement → malformed, no fake repair),
  staged names emitted before `read_committed` can fail (`reconstruction.rs:218-221`), and
  seeded DST coverage for reconstruction.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/src/custodian.rs:501-505` hard-codes
  `wyrd_testkit::SystemClock` for `ReconstructionContext::clock`. Its comment says this is
  "the same clock this loop's own `clock` closure already advances". That is only true when the
  caller passes wall time. `crates/server/tests/custodian_day_one.rs:1174`, `:1234` and `:1344`
  drive this same loop with `|| 500`. In those runs the pass's `now_millis` is 500 while
  `ctx.clock.now_millis()` is about 1.79e12. The brief asked the deployed loop to pass "the
  clock it already advances" (brief.md:104-105). Nothing reads the field yet. But #814 will
  read `ctx.clock` to check a staged re-place's write window against a pre-mark stamped from
  the pass's `now_millis`. That mixes a manual clock and the wall clock in one lifecycle, which
  is the #557/#565 class the rubric's first MUST forbids. Fix: keep one
  `wyrd_testkit::ManualClock` in the loop, `set(clock())` once per reconstruction pass, and pass
  that same reading as the pass's `now_millis`, so the field and the argument come from one
  source. The test and DST sites that pair `SystemClock` with a fixed `NOW`/`HANDOFF_NOW` (for
  example `crates/custodian/tests/staged_protection.rs:518` with `now: u64` at `:508`) have the
  same mismatch and should follow the same pattern before #814 builds on them.

- NEEDS-HUMAN [human] — after publication, scrub still checks the `part:` record's placement
  even when it no longer matches the committed one, and scrub and reconstruction then keep
  undoing each other. `crates/custodian/src/scrub.rs:199-200` only de-duplicates identical
  `(dserver, fragment)` keys. Concrete case, following the path this patch creates:
  1. Chunk C (RS k=2, m=1) loses fragment 0 on server 3 while staged. Scrub enqueues it, and
     reconstruction keeps the obligation (`reconstruction.rs:727`).
  2. The upload publishes, and the inode copies placement `[3,1,2]`.
  3. Reconstruction now takes the committed path, rebuilds fragment 0 on server 0, and moves
     the inode to `[0,1,2]`.
  4. The `part:` record still says `[3,1,2]` until the retirement drain deletes it.
  5. Every later scrub fetches `(3,(C,0))`, gets `Ok(None)`, enqueues C, reports `emit_missing`
     and answers `Changed`.
  6. Every reconstruction pass finds the committed chunk whole and drains it
     (`reconstruction.rs:827-829`).
  So scrub never answers `Satisfied` and keeps raising false "missing fragment" signals for a
  location nothing reads. `crates/custodian/src` has no `retire:records` drain yet, so this
  window has no bound in the current tree (though no multipart writer exists yet either). One
  option: skip a staged placement for any chunk the committed reading also names. Once
  published, the committed placement is the one reads use, and the staged-first read order
  still sees each chunk in at least one class. This departs from the brief's literal "every
  fragment a committed `part:` record places" (brief.md:95), so it is a scope call.

- NEEDS-HUMAN [human] — `check-gates.json` row T4-batch-review
  (`gate-logs/T4-batch-review.log:10`) reports "0 blocking" after 30.48 s for three codex
  passes over a 175 KB patch. The same gate found 9 blocking items in the previous round
  (brief.md:159). The log is one summary line with no evidence from the individual passes.
  Before counting this gating row as green, open `results/issue_813/review-batch.md` and
  confirm all three passes actually ran to completion rather than erroring out or returning an
  empty or cached result.

## Tried to refute, could not

- The drain gate while a staged record is unreadable (`reconstruction.rs:553` in the patch)
  withholds every drain and certifies nothing. Red on base, green on the fix.
- A kept staged obligation stays off the repair-backlog gauge: `Assessment::Staged` never
  increments `under_replicated`.
- `W_write` has one definition: `crates/custodian/src/gc.rs:201` is reused at
  `crates/server/src/custodian.rs:132`.
- Scrub never reads `sidx:`. Rewritten leg F checks the read log across every damaged and
  faulted fixture.
- The `StagedSet::place` refactor (`gc.rs:1410-1432`) behaves the same as before, and GC and
  restore legs A–E pass unedited.
- The C4-diff-cov misses (`reconstruction.rs:219-220`, `:1229-1238`, `gc.rs:1425-1428`) come
  from a tool artifact: that gate only measured `--test staged_scrub`
  (`gate-logs/C4-diff-cov.log:14`), and the `staged_protection` legs I ran do execute those
  lines. The one path really left untested is session-listing pagination in the new
  `staged_committed_parts` (`gc.rs:1611-1613`), which is line-for-line the same as the loop in
  `staged_fragments`. That is not a refutation.

### Advisory — code-review

No findings. This diff is clean on both advisory lenses: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified.

- Correctness: checked staged-before-committed reads, failure attribution, and drain protection at `crates/custodian/src/scrub.rs:123`, `crates/custodian/src/reconstruction.rs:210`, and `crates/custodian/src/reconstruction.rs:427`; reviewed the publication-race coverage at `crates/dst/tests/custodian.rs:2841`.
- Reuse and efficiency: the strict placement rule is shared at `crates/custodian/src/gc.rs:1460`; the part-only reader reuses the paged range walker at `crates/custodian/src/gc.rs:1607`; reconstruction retains its empty-queue shortcut at `crates/custodian/src/reconstruction.rs:207`.

Validation used the frozen gate logs: CI and the reconstruction handoff simulations passed; staged scrub had 11 green tests versus 9 failures and 2 passes on the base; diff coverage was 92.0%; mutation testing reported 15 caught and 16 unviable mutants. No builds or tests were rerun against the read-only target.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm the unpublished #663/#637 rejection history imposes no remaining constraint on this split — all affected paths and closed PRs were checked, but those local rejection artifacts are not supplied; brief.md:146, brief.md:150, reviewer-prior-art-summary.txt:1.
- [x] Validation — fitness-to-purpose — Accept detection plus durable retention as the useful milestone while #814 owns rebuilding — staged losses remain queued and Blocked rather than having redundancy restored; brief.md:121, crates/custodian/src/reconstruction.rs:727, docs/design/architecture/06-runtime-view.md:82.
- [ ] `crates/server/src/custodian.rs:501-505` hard-codes
- [ ] after publication, scrub still checks the `part:` record's placement
- [ ] `check-gates.json` row T4-batch-review
- [x] size backstop — this slice is behaving oversized: patch is 171 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. — Human explicitly overrode this recommendation and chose `iterate-do`, staying as one slice.

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
- Iteration delta (if iterating): Fix the clock source mismatch in custodian.rs:501-505 so ReconstructionContext::clock and the pass's now_millis come from one source, and either prevent or explicitly bound the post-publish scrub/reconstruction flip-flop on stale part: placements. Size backstop overridden by the human — stay as one slice, do not split.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
