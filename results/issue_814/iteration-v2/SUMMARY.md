# Result — issue 814 / staged-replace

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: reconstruction recognises a committed part's chunk but can only keep its obligation
  (`crates/custodian/src/reconstruction.rs:736-739`, `Assessment::Staged`). Nothing rebuilds the
  fragment, so the part stays a fragment short until it is published, or forever if the client
  never completes. 0016 requires the rebuild (`0016:825`), and its failure table names "scrub
  staged fragments but leave reconstruction committed-only" as a wrong implementation (`:889`).
- Success criterion: the NEW file `crates/custodian/tests/staged_repair.rs` passes, one
  seeded case appended to the existing `crates/dst/tests/custodian.rs` passes, and
  `cargo xtask ci` is green. The new test names only symbols on `main`, including
  `ReconstructionContext::{clock, staged_write_window_millis}` (`reconstruction.rs:123`, `:131`),
  with time from a `wyrd_testkit::ManualClock` (`crates/testkit/src/lib.rs:49`). The D-server
  doubles — this file's and the DST's — **enforce** the deadline `put_fragment` carries, refusing
  at or after it through `WriteDeadlineExpired::if_elapsed` (`crates/traits/src/lib.rs:925`), as
  the real D server does since #638. Earlier doubles ignored it (`_deadline_millis`,
  `crates/custodian/tests/gc.rs:127`). Legs:
  **(A) The whole rebuild.** An `Open` session's committed part has one fragment lost and its
  obligation queued. One `reconcile_step` with a `ReconstructionContext` answers `Changed`, and:
  a new D server holds the lost fragment, and its payload (`wyrd_core::repair::intact_shard`,
  `crates/core/src/repair.rs:124`) is byte-for-byte the shard the fixture encoded for that index
  with `wyrd_core::erasure::encode` (`crates/core/src/erasure.rs:90`); the `part:` record's
  `ChunkRef.placement` names that server; the destination's pre-mark `orphan:<P_new>` is gone;
  the vacated `P_old` carries an `orphan:` mark; the obligation has drained. A changed placement
  alone is not enough, and neither is a header check: `header_matches_identity` (`repair.rs:58`)
  and `fragment_intact` (`:106`) both accept a well-formed fragment with the wrong payload. Every
  fragment rebuilt in C(iii) and C(vii) is held to the same byte check.
  **(B) Losing branches strand nothing (X29, `0016:888`).** A `put_fragment` hook moves the
  session from `Open@E` to `Aborting@E+1` after the destination write and before the adoption
  CAS. Then: no adoption, the `part:` record byte-identical, the pre-mark still there, the
  obligation queued. Repeat with the `part:` record rewritten instead of the session fenced.
  **(C) The pre-mark and deadline rules (`0016:1285-1358`), one case each:**
  (i) the pre-mark is durable before the destination write: the double checks `orphan:<P_new>`
  is present when `put_fragment` arrives;
  (ii) a destination position that already carries a mark from another event, or a legacy mark,
  is re-stamped fresh, never reused with its old stamp;
  (iii) a position with a `reclaiming` mark is never written, and ruling it out removes only that
  position, not its server: RS(2,2) with two lost fragments, two free domains and a stale
  `reclaiming` mark on one candidate position repairs both in ONE pass;
  (iv) the write deadline is the time the context clock reads when the pre-mark commits, plus
  `staged_write_window_millis` — never the pass-start time. With the clock moved on between pass
  start and pre-mark, the deadline is still live. A write the double refuses as expired aborts
  the re-place: no adoption, pre-mark still there, obligation queued;
  (v) no destination write is authorized on a pre-mark older than `W_repoint`
  (`0016:1339-1349`, `crates/custodian/src/gc.rs:218`). A hook moves the clock past it between
  pre-mark and write, and no write on the stale pre-mark is ever adopted. Enforcing this through
  the deadline the D server checks is accepted (sign-off, #663 v1);
  (vi) a vacated `P_old` whose existing `orphan:` value decodes as none of the three shapes aborts
  the move before its CAS, keeps the obligation queued and names the fault; it is never
  overwritten (ADR-0045);
  (vii) **a slow but legal write never stops a multi-fragment move (v1's defect).** RS(2,2),
  fragments 2 and 3 lost, two free destinations in two free domains, and every stored write moves
  the `ManualClock` on by 12 s — longer than `W_repoint` (10 s), and twice that is still inside
  `W_write` (30 s, `gc.rs:202`), so no design is refused by the double for timing alone. Within at
  most TWO `reconcile_step` passes both fragments are rebuilt and adopted, the placement names
  both new servers and the obligation has drained. Leg (v) stays green beside it;
  (viii) **GC reclaiming the destination between write and adoption makes the adoption lose.**
  A `put_fragment` hook, after the destination write is stored and before the adoption CAS, does
  what GC's reclaim does: it swaps `orphan:<P_new>` to its `reclaiming` form with the same stamp
  and event (`OrphanMark::into_reclaiming`, `crates/core/src/metadata.rs:180`; GC's own swap,
  `Intent::record`, `gc.rs:589-595`) and then deletes the fragment from the destination, in that
  order (GC commits the swap before it deletes, `gc.rs:765-776`). The production pass then
  resumes: no adoption, the `part:` record byte-identical, the `reclaiming` mark left as GC wrote
  it, the obligation queued. The case asserts the destination received the write, so it cannot
  pass on a pass that writes nothing. This is the only case that fails an adoption CAS missing its
  pre-mark precondition; the existing DST adoption case (`crates/dst/tests/custodian.rs:3441-3455`)
  builds its own batch and never runs this path.
  **(D) The drain fence on the destination (`0016:885`).** A server with ANY
  `desired:dserver:<S>` record (`crates/custodian/src/desired_state.rs:35-40`), whatever its value
  — `maintenance` included — is never chosen, so selection and the CAS test the same fact. A drain
  recorded between selection and the adoption CAS makes the CAS lose
  (`require_absent(desired:dserver:<S_new>)`): not adopted, pre-mark still there.
  **(E) Kept, not rebuilt — nothing is written and the obligation stays queued:** an `sidx:`-only
  chunk; a chunk of a session that is not `Open` (a repair blocked by a Complete is retried after
  publication, `0016:825`), seeded as an RS(2,2) chunk with one fragment lost so that it is
  repairable and the refusal is the session state's, not the scheme's, with the pass answering
  `Blocked` as #813's kept path does (`reconstruction.rs:352`, `:448-462`); a chunk the staged reading holds as untrusted (`StagedSet::held`,
  `gc.rs:1337`) because a corrupt `sidx:` entry names it, even though a valid committed part in an
  `Open` session names it too; and a degraded chunk with no usable destination (every other
  server carries a `desired:` record).
  **(F) Seeded DST for X29**, appended to the existing `crates/dst/tests/custodian.rs` (keep
  `#![cfg(madsim)]`, `:53`). Sweep the session fence across every point of the re-place: before
  the pre-mark, between pre-mark and write, between write and CAS, after the CAS. In every
  interleaving no fragment ends unreferenced and unevidenced, and the session never ends
  `Aborting` with a `part:` record naming a fragment that was not written. It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1578`, `--cfg madsim`). Record the seed count
  in `build-notes.md`.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: rebuild and re-place a committed part's chunk in an `Open` session: pre-mark before
  write; deadline from the context clock at pre-mark commit plus the window; the `W_repoint`
  rule; one adoption CAS pinned to the session state, the prior `part:` bytes, the pre-mark's
  bytes and `require_absent(desired:dserver:<S_new>)`. On any loss the obligation stays queued
  and the pre-mark stands. Time and window come only from the two context fields; add no other.
  A committed part's chunk in an `Open` session that is found at full redundancy resolves as the
  committed path resolves one: its obligation drains as a duplicate finding
  (`reconstruction.rs:836-839`), and one no survivor set can rebuild (`EcScheme::None`, or fewer
  than `k` intact) is `Unrepairable`, as a committed one is (`:772`, `:850`). "Kept" (leg E) is
  for a chunk whose repair this pass may not or cannot make yet.
  Keep #813's legs G–J in `crates/custodian/tests/staged_protection.rs` passing, and do not move
  leg G off an `Open` session. Its committed-part case (`:2353-2401`) seeds an `Open` session
  whose only fragment, `EcScheme::None` (`:2360`), is lost; nothing can rebuild that, so its
  "obligation queued, no write, `part:` byte-identical, chunk named on the reconstruction audit
  seam" assertions stay true and stay as they are. The one intended change is its
  `Reconciled::Blocked` (`:2372`), which becomes `Reconciled::Satisfied`: the chunk is now
  assessed as a committed one, `Unrepairable` raises the data-loss signal (`:329`,
  `emit_data_loss` `:1197`, same audit target, same chunk name) and is not a hole in the pass
  (`:448-466`). Update its doc comment to say so. Refusal for a session that is not `Open` is
  leg E's, on a repairable RS fixture. The other G–J cases stay.
  Docs and markers this slice makes stale, all to be brought up to date: the sentence "it does
  not yet rebuild or re-place a staged chunk itself" in
  `docs/design/architecture/06-runtime-view.md:82`; the `deferred: #814` marker at
  `gc.rs:1322-1328`; the "#814" seam notes in `reconstruction.rs:44-59`, `:110-131`, `:706-707`,
  `:1272` and in `crates/server/src/custodian.rs`.
  / out of scope: the committed repair path (`repair_chunk`, `reconstruction.rs:960-1086`,
  unchanged in behaviour); one degraded chunk per part per pass — every plan in a part pins the
  same `part:` bytes, as a second obligation inside one committed object loses its CAS and waits
  for the next pass (`reconstruction.rs:396-399`); accepted here, say so in a comment;
  **settling a pre-mark** — a late write whose effect is `WriteEffect::Unknown`
  (`crates/traits/src/lib.rs:862-902`) landing after GC reclaims its pre-mark, which can only
  happen on a destination that already held bytes from an earlier aborted attempt, and a
  pre-mark whose write never landed staying in the `orphan:` ledger (GC's sweep keeps it,
  `gc.rs:260-269`). Both are one follow-up, #825: carry an in-code
  `deferred: #825` marker where the re-place aborts after a write, so review treats them
  as settled (target `AGENTS.md:200-205`); `EcScheme::None` answering Unrepairable (by design, as
  `reconstruction.rs:772`); servers absent from the live fleet; `seg:` repair (#777); rebalance
  and restore (#809, #810); the upload-side drain fence (#657); `crates/core/src/multipart.rs`;
  edits to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (23 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.8% — 455 of 475 instrumentable changed lines executed (floor 80%); 475 of 1187 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 83 mutants tested in 12m: 7 missed, 27 caught, 48 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.01s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #814: rebuild degraded committed multipart chunks while their upload remains Open, preserving the session fence, reclamation evidence, and bounded repair progress.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scoped repair and losing-race outcomes are falsifiable, including slow writes and GC reclaim; single-copy semantics, bundle size, and #825 are explicitly settled (`brief.md:25`, `brief.md:124`, `brief.md:134`, `brief.md:163`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production fix while retaining the new test produced assertion failures: 20 failed, 3 passed; the missing rebuild is reproduced without a compilation failure (`reviewer-evidence/red.log:276`, `reviewer-evidence/red.log:321`). |
| C3 Change | PASS | The patch stays within the agreed staged-repair scope, preserves the Open single-copy guard, and updates the living architecture description; no extra context field or multipart codec change was introduced (`crates/custodian/src/reconstruction.rs:429`, `crates/custodian/tests/staged_protection.rs:2353`, `docs/design/architecture/06-runtime-view.md:82`). |
| C4 Verification (red→green) | PASS | After stash restoration all 23 new tests pass; the 36 protection tests and 50-seed DST pass independently, and remaining CI components pass after resolving a local database-lock limitation; frozen diff coverage is 95.8% (`reviewer-evidence/green-after-pop.log:32`, `reviewer-evidence/dst.log:524`, `gate-logs/C4-diff-cov.log:792`). |
| C5 Causal adequacy | PASS | The missing rebuild and sequential-write starvation are addressed at their causes; payload equality, losing adoption, stale authorization, and two-fragment progress are exercised through production reconciliation (`crates/custodian/tests/staged_repair.rs:677`, `crates/custodian/tests/staged_repair.rs:1423`, `crates/custodian/tests/staged_repair.rs:1496`, `crates/custodian/src/reconstruction/staged.rs:654`). |
| T1 Structure | PASS | Reconstruction shares the staged reader and survivor assessment while retaining trait boundaries and a separate staged adoption protocol (`crates/custodian/src/gc.rs:1538`, `crates/custodian/src/reconstruction.rs:897`, `crates/custodian/src/reconstruction/staged.rs:566`). |
| T2 Shape | PASS | Exact stored bytes fence both commits; canonical part encoding preserves unrelated fields, malformed source marks withhold repair, and position exclusions retain other usable positions (`crates/custodian/src/reconstruction/staged.rs:294`, `crates/custodian/src/reconstruction/staged.rs:380`, `crates/custodian/src/reconstruction/staged.rs:743`). |
| T3 Runtime | PASS | One context clock owns mark age and deadlines; joined writes create no detached tasks, and session/part/mark/drain races are checked before adoption; both new DST properties pass across the configured 50 seeds (`crates/custodian/src/reconstruction/staged.rs:613`, `crates/custodian/src/reconstruction/staged.rs:666`, `crates/custodian/src/reconstruction/staged.rs:696`, `xtask/src/main.rs:1573`, `reviewer-evidence/dst.log:533`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and must receive their substantive audit at publish; the deferred row is neither a green nor a missing-evidence finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Close the outage-classification and selective-reading regression gaps — three non-equivalent mutants survive, leaving false permanent-loss classification and loss of queue/first-reference filtering undetected by the affected-package tests (`crates/custodian/tests/staged_repair.rs:624`, `crates/custodian/src/reconstruction.rs:913`, `crates/custodian/src/reconstruction/staged.rs:212`, `reviewer-evidence/mutants.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that Open-upload repair and a Satisfied pass for a lost single-copy chunk meet operator expectations — queue retention and the data-loss signal are tested, but accepting this visible behavior remains a product judgment (`crates/custodian/tests/staged_protection.rs:2353`, `brief.md:240`). |

A test-only rebuild is recommended for the T5 coverage gaps; no in-scope production defect was confirmed. The repair reproduces red→green, the reported single-copy finding conflicts with the agreed scope, and the mutation results identify specific missing regression cases rather than a failed repair.

Source citations above are relative to `$PDCA_TARGET` (`target/`); brief and evidence citations are relative to this review directory. The target is readable and current: its base commit identifies `feb1e3047eb1dc194a058aaddb1c60076be70220`, matching the brief. Production changes were restored after the stash experiment. No implementation edits were made.

1. **The regression suite proves the requested repair.** The independent pre-fix run compiled and failed 20 assertions, including the whole rebuild and slow two-fragment move; three guard cases remained green. Restoring the patch gave 23/23 passes (`reviewer-evidence/red.log:321`, `reviewer-evidence/green-after-pop.log:32`). The initial green run also passed all 36 existing staged-protection tests (`reviewer-evidence/green.log`). The DST rerun passed both the seeded race and the explicit four-position fence sweep (`reviewer-evidence/dst.log:524`, `reviewer-evidence/dst.log:533`; `crates/dst/tests/custodian.rs:4950`, `xtask/src/main.rs:1607`). The new guards protect mutable protocol state; none is an optional-capability probe concealing an eager initialization cause.

2. **Three mutation survivors expose test gaps; four do not establish defects.** A focused independent rerun reproduced all seven survivors (`reviewer-evidence/mutants.log:1`). Add a staged RS(2,1) case with zero fetchable survivors and all three placements classified temporarily unreachable: assert that the obligation remains, no permanent-loss alarm is emitted, and recovery settles normally. With `k=2`, the intended `0 + 3 >= 2` is true, while the surviving multiplication mutation makes `0 * 3 >= 2` false and emits permanent loss; subtraction also fails this case. The new fixture always supplies `unreachable: &[]` (`crates/custodian/tests/staged_repair.rs:624`). The broader server suite does exercise one survivor plus two unreachable placements, and that test passed, but that input does not distinguish addition from multiplication (`crates/server/tests/custodian_day_one.rs:849`, `reviewer-evidence/ci.log:2127`).

   Also exercise the staged reader with unqueued parts and two valid references to a queued chunk: verify it retains only owed records and the first reference. Changing the disjunction at `crates/custodian/src/reconstruction/staged.rs:212` to a conjunction admits unqueued records and overwrites the selected reference; the current tests survive that change. This is a regression-coverage finding, not evidence that the unmutated reader is wrong.

   The four `repointed_part` conjunction survivors are equivalent under the current canonical-record invariant: the rewrite substitutes only the encoded chunk list and preserves every other byte, and the resulting record is decoded before acceptance (`crates/custodian/src/reconstruction/staged.rs:752`, `crates/core/src/multipart.rs:1927`). Their survival alone warrants no defect. The frozen selector timeout is on the inverted completion condition, which makes the mutant loop; it does not demonstrate a hang in the actual selector (`gate-logs/C5-mutants.log:16`, `crates/custodian/src/reconstruction/staged.rs:430`). The frozen full campaign remains accurately reported as 83 tested, 7 missed, 27 caught, 48 unviable, and 1 timeout (`gate-logs/C5-mutants.log:21`).

3. **The frozen batch-review finding is rejected within this brief's scope.** Its concern is grounded: `EcScheme::None` returns before fetching a fragment (`gate-logs/T4-batch-review.log:10`, `crates/custodian/src/reconstruction/staged.rs:280`). However, the brief expressly designates that result as intentional and excludes changing it (`brief.md:169`), matching the committed assessment (`crates/custodian/src/reconstruction.rs:840`). Changing intact or temporarily unreachable single-copy behavior would require a separate scope decision; it is not a rebuild defect against this brief. This records the rejection without changing the frozen batch gate's FAIL. The two #825 residuals remain settled under the standing protocol and are not re-raised (`crates/custodian/src/reconstruction/staged.rs:632`, `AGENTS.md:200`).

4. **Verification dependencies and prior art were checked independently.** `cargo xtask ci` passed spelling, documentation lint/render, formatting, clippy, builds, tests, and dependency-use checks before failing to obtain an exclusive lock on the host's read-only advisory database (`reviewer-evidence/ci.log`). This was a reviewer-host limitation. All three dependency-wall invocations subsequently passed against a writable copy of the same database with unchanged policies and offline fetching; an initial scratch-config formatting error was corrected before the all-features advisory rerun (`reviewer-evidence/deny.log:14`, `reviewer-evidence/deny.log:20`, `reviewer-evidence/deny-all-features.log`). Conformance, the global-state scanner, the real-workspace orchestrator scanner, DST, and both TiKV/server feature clippy checks passed separately (`reviewer-evidence/remaining-checks.log:1`, `reviewer-evidence/remaining-checks.log:15`, `reviewer-evidence/dst.log`). Thus the local one-command CI exit was not green; its components were completed separately. The frozen full CI log reports all checks passed (`gate-logs/C4-ci.log:3711`). Both brief-listed external dependencies, typos and the actual docs renderer, were exercised (`reviewer-evidence/ci.log:2`, `reviewer-evidence/ci.log:7`).

   The instance-root coverage wrapper was unavailable here; its complete frozen log shows 455 of 475 instrumentable changed lines executed, 95.8%, across 386 tests (`gate-logs/C4-diff-cov.log:789`). This is log-adjudicated evidence, not an independently reproduced coverage percentage. Every configured gate log was present and readable. No missing-oracle escalation is needed, and the contribution audit remains deferred to publish.

   Prior-art queries covered all ten affected paths at the brief's base, classified all 344 closed PRs, inspected every one of the 16 closed/unmerged PR file lists, and found no open PRs (`reviewer-evidence/prior-art.log:1`, detailed responses in `reviewer-evidence/prior-art.json` and complete per-path histories in `reviewer-evidence/prior-art-full-history.json`). The substantive closed overlaps are #647's segmented maps and #336's global-state proposal, neither a staged re-place; the other overlaps only affect Cargo.lock. The earlier rejected bundles are accounted for in the supplied carry-forward (`brief.md:192`). No competing implementation was found. The 215,736-byte patch falls under the already accepted size decision (`brief.md:124`).

Tier-1 disk-fault and Tier-2 kill-and-reconstruct observation is warranted for this durability change, as the standing rubric requests (`AGENTS.md:79`). Those campaigns were not run here; they complement the passing Tier-0 evidence and are not represented as required gates or as an unmet dependency in this brief. No visual or GUI acceptance step applies.

### Advisory — adversary

# Adversarial review — #814 staged re-place (advisory)

**Verdict: I could not refute the fix.** I re-ran the green leg at `$PDCA_TARGET` in a scratch copy (23/23 pass). The frozen C4-verify log shows the red leg: 20 tests fail by assertion on the base, and the 3 that pass are the leg-E guards the brief designed to pass there. I then planted the brief's SELF-TEST defects in `crates/custodian/src/reconstruction/staged.rs` to check the tests catch them:

- Dropping `require(orphan:<P_new> == pre-mark)` from the adoption batch (staged.rs:703-706) turns C(viii) red.
- v1's shape (the `W_repoint` gate re-checked before each write, writes awaited one at a time) turns C(vii) red.

I also wrote three extra probes. All three pass on the patch:

- RS(2,2) with two fragments lost, where one write's landing cannot be confirmed in time. Nothing is stranded, and the next pass repairs both fragments.
- Two damaged chunks in one `part:` record. The second chunk's pre-mark loses to the first chunk's adoption, so nothing is written for it that pass, and the next pass repairs it.
- A staged chunk whose shortfall comes only from an unreachable server. It is classed `Unreachable`, with no data-loss alarm.

Other attacks I tried and could not land:

- The session fence, a rewritten part record, a drain record, and GC's reclaim swap, each landing between the steps of a move.
- A pre-mark read by GC's window racing the re-stamp or the adoption. Both sides pin the mark's exact bytes, so whoever commits second loses.
- The deadline and grace arithmetic (`stamp + W_write`, which lands inside `G_orphan`).
- Rebuilding in place, and vacated positions whose mark is absent, stamped or `reclaiming`.
- The Kuhn matching in `assign` and `augment`, and whether the destination loop always ends.
- Byte identity of the repointed part record, and duplicate keys inside one batch.

Findings:

- NEEDS-HUMAN [impl] — **Nothing tests that the writes are sent together, and the code depends on it.** `crates/custodian/src/reconstruction/staged.rs:641-666` says each write "has the whole of `W_write` from the pre-mark to land in, however long the others take". That is true only because of `join_all`. I replaced `join_all` with a plain loop that awaits each write in turn, keeping the single gate: all 23 tests still pass. The DST case cannot catch it either, because it only ever moves one fragment (`crates/dst/tests/custodian.rs:4477`). C(vii) cannot see the difference: its double never yields and the brief sized its writes at 12 s (`crates/custodian/tests/staged_repair.rs:1405-1411`), so the timing works out even when the writes run one after another. Concrete failure after such a regression: RS(2,2), two fragments lost, each write takes 20 s. Each write is legal on its own (under 30 s), but the second arrives at t+20 s and lands at t+40 s, past its deadline of t+30 s. It is refused on every pass, and the chunk stays degraded for good — the brief's invariant "no legal timing of a successful write leaves it degraded". Fix: add a case whose D-server double makes both writes wait until both have arrived (with a timeout), then moves the clock on once by 20 s. A sequential loop then fails and `join_all` passes. Or add an RS(2,2) two-loss fixture to the DST property.
- NEEDS-HUMAN [impl] — **Surviving mutant at `crates/custodian/src/reconstruction.rs:913`** (`Gathered::settle`, `+` → `-`/`*`). No custodian test reaches the `Unreachable` arm, and this patch now sends staged chunks through it too. My probe: `Fixture::standard` (RS(2,1), fragment 2 lost), with server 1 dropped from the fleet and listed in `ctx.unreachable`. On the patch it answers `Unreachable` and raises no `data-loss` event. With the `*` mutant it pages `data-loss` and the probe fails, so the probe kills the mutant. Add it (or a committed twin) to `staged_repair.rs`.
- NEEDS-HUMAN [impl] — **Surviving mutant at `crates/custodian/src/reconstruction/staged.rs:212`** (`||` → `&&` in `read`). Two claims have no test behind them: that a part is held "only once a repair is owed inside it" (`staged.rs:115-116`, `:200-204`), and the first-reference rule. Under the mutant, every part record of every upload is cloned into `parts` on every pass, and a later part overrides an earlier part's site. Nothing notices either. For the C5 row: the other four survivors (`staged.rs:763-766`, `&&` → `||` in `repointed_part`'s read-back check) look equivalent in practice. The canonical encoding puts `"chunks"` first, so the splice cannot touch the other fields. Excluding them with a reason or leaving them is a builder call, not a defect.
- NEEDS-HUMAN [human] — **T4's one blocking finding (`staged.rs:280`) is real, but the brief ordered this behaviour.** The staged path returns `Unrepairable` for `EcScheme::None` before it fetches anything, exactly like the committed path on `main` (`crates/custodian/src/reconstruction.rs:840`). Concrete case: an `Open` upload's single-copy chunk whose only fragment is intact, but whose server is in `ctx.unreachable` or which has a stale obligation. It now pages `data-loss` on every pass and never drains. On the base it was kept quietly as `Blocked`. Scope says "`EcScheme::None` answering Unrepairable (by design, as `reconstruction.rs:772`)", and leg G's `Blocked` → `Satisfied` was accepted on that basis. So the builder cannot fix this without breaking the brief. A human must either decline T4 with an issue that covers both paths, or widen scope.
- NEEDS-HUMAN [human] — **A count in `check-gates.json` is wrong.** C4-verify's summary says "(23 test(s) ran red)". The frozen `gate-logs/C4-verify.log` shows `3 passed; 20 failed` on the base. The three that passed are the leg-E guards `a_chunk_only_an_owned_entry_names_is_kept`, `a_chunk_an_untrusted_owned_entry_holds_is_kept` and `a_chunk_of_an_upload_that_has_left_open_is_kept_then_repaired_once_published`. The PASS verdict still holds, because the brief meant those three to pass on the base. The count is a harness reporting bug, not the builder's.
- NEEDS-HUMAN [human] — **Low: mixed-version D servers.** This re-place is the first writer on `main` whose "no fragment left without a mark" argument depends on the D server enforcing the write deadline (`staged.rs:69-74`). The gRPC client documents that an old D server ignores `deadline_millis`, and that a mixed-version fleet "does **not** get the guarantee" (`crates/chunkstore-grpc/src/client.rs:274-277`). If the custodian is upgraded before the D servers, an old server that holds a write past `G_orphan` can land it after GC has reclaimed and swept the pre-mark. The result is an unmarked stray fragment: a leak, not data loss. This is outside this diff (#638's caveat, next to #825). Decide whether it needs an upgrade-order note or a follow-up issue.

### Advisory — code-review

No new in-scope correctness defect found. One efficiency finding:

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction/staged.rs:589` and `crates/custodian/src/reconstruction.rs:445`: after a placement-changing adoption, later plans for that same part still reconstruct the entire chunk and encode every shard before their pre-mark CAS rejects the now-stale part bytes (`staged.rs:617`). With N degraded chunks that must move, this performs N(N+1)/2 rebuilds across N passes for N adopted repairs. Track which part snapshots this pass has superseded and skip their remaining plans before erasure work. This preserves the accepted one-chunk-per-part-per-pass behavior; an in-place repair that leaves the part bytes unchanged should not invalidate other plans.

Reviewed the frozen CI, red/green, coverage, mutation and batch-review evidence; no gates were rerun. The single-copy classification flagged by T4 at `crates/custodian/src/reconstruction/staged.rs:280` is explicitly accepted in the brief, and pre-mark settlement is deferred to #825; neither is re-raised here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Close the outage-classification and selective-reading regression gaps — three non-equivalent mutants survive, leaving false permanent-loss classification and loss of queue/first-reference filtering undetected by the affected-package tests (`crates/custodian/tests/staged_repair.rs:624`, `crates/custodian/src/reconstruction.rs:913`, `crates/custodian/src/reconstruction/staged.rs:212`, `reviewer-evidence/mutants.log:3`).
- [ ] Validation — fitness-to-purpose — Confirm that Open-upload repair and a Satisfied pass for a lost single-copy chunk meet operator expectations — queue retention and the data-loss signal are tested, but accepting this visible behavior remains a product judgment (`crates/custodian/tests/staged_protection.rs:2353`, `brief.md:240`).
- [ ] **Nothing tests that the writes are sent together, and the code depends on it.** `crates/custodian/src/reconstruction/staged.rs:641-666` says each write "has the whole of `W_write` from the pre-mark to land in, however long the others take". That is true only because of `join_all`. I replaced `join_all` with a plain loop that awaits each write in turn, keeping the single gate: all 23 tests still pass. The DST case cannot catch it either, because it only ever moves one fragment (`crates/dst/tests/custodian.rs:4477`). C(vii) cannot see the difference: its double never yields and the brief sized its writes at 12 s (`crates/custodian/tests/staged_repair.rs:1405-1411`), so the timing works out even when the writes run one after another. Concrete failure after such a regression: RS(2,2), two fragments lost, each write takes 20 s. Each write is legal on its own (under 30 s), but the second arrives at t+20 s and lands at t+40 s, past its deadline of t+30 s. It is refused on every pass, and the chunk stays degraded for good — the brief's invariant "no legal timing of a successful write leaves it degraded". Fix: add a case whose D-server double makes both writes wait until both have arrived (with a timeout), then moves the clock on once by 20 s. A sequential loop then fails and `join_all` passes. Or add an RS(2,2) two-loss fixture to the DST property.
- [ ] **Surviving mutant at `crates/custodian/src/reconstruction.rs:913`** (`Gathered::settle`, `+` → `-`/`*`). No custodian test reaches the `Unreachable` arm, and this patch now sends staged chunks through it too. My probe: `Fixture::standard` (RS(2,1), fragment 2 lost), with server 1 dropped from the fleet and listed in `ctx.unreachable`. On the patch it answers `Unreachable` and raises no `data-loss` event. With the `*` mutant it pages `data-loss` and the probe fails, so the probe kills the mutant. Add it (or a committed twin) to `staged_repair.rs`.
- [ ] **Surviving mutant at `crates/custodian/src/reconstruction/staged.rs:212`** (`||` → `&&` in `read`). Two claims have no test behind them: that a part is held "only once a repair is owed inside it" (`staged.rs:115-116`, `:200-204`), and the first-reference rule. Under the mutant, every part record of every upload is cloned into `parts` on every pass, and a later part overrides an earlier part's site. Nothing notices either. For the C5 row: the other four survivors (`staged.rs:763-766`, `&&` → `||` in `repointed_part`'s read-back check) look equivalent in practice. The canonical encoding puts `"chunks"` first, so the splice cannot touch the other fields. Excluding them with a reason or leaving them is a builder call, not a defect.
- [x] **T4's one blocking finding (`staged.rs:280`) is real, but the brief ordered this behaviour.** The staged path returns `Unrepairable` for `EcScheme::None` before it fetches anything, exactly like the committed path on `main` (`crates/custodian/src/reconstruction.rs:840`). Concrete case: an `Open` upload's single-copy chunk whose only fragment is intact, but whose server is in `ctx.unreachable` or which has a stale obligation. It now pages `data-loss` on every pass and never drains. On the base it was kept quietly as `Blocked`. Scope says "`EcScheme::None` answering Unrepairable (by design, as `reconstruction.rs:772`)", and leg G's `Blocked` → `Satisfied` was accepted on that basis. So the builder cannot fix this without breaking the brief. A human must either decline T4 with an issue that covers both paths, or widen scope.
- [ ] **A count in `check-gates.json` is wrong.** C4-verify's summary says "(23 test(s) ran red)". The frozen `gate-logs/C4-verify.log` shows `3 passed; 20 failed` on the base. The three that passed are the leg-E guards `a_chunk_only_an_owned_entry_names_is_kept`, `a_chunk_an_untrusted_owned_entry_holds_is_kept` and `a_chunk_of_an_upload_that_has_left_open_is_kept_then_repaired_once_published`. The PASS verdict still holds, because the brief meant those three to pass on the base. The count is a harness reporting bug, not the builder's.
- [ ] **Low: mixed-version D servers.** This re-place is the first writer on `main` whose "no fragment left without a mark" argument depends on the D server enforcing the write deadline (`staged.rs:69-74`). The gRPC client documents that an old D server ignores `deadline_millis`, and that a mixed-version fleet "does **not** get the guarantee" (`crates/chunkstore-grpc/src/client.rs:274-277`). If the custodian is upgraded before the D servers, an old server that holds a write past `G_orphan` can land it after GC has reclaimed and swept the pre-mark. The result is an unmarked stray fragment: a leak, not data loss. This is outside this diff (#638's caveat, next to #825). Decide whether it needs an upgrade-order note or a follow-up issue.
- [ ] `crates/custodian/src/reconstruction/staged.rs:589` and `crates/custodian/src/reconstruction.rs:445`: after a placement-changing adoption, later plans for that same part still reconstruct the entire chunk and encode every shard before their pre-mark CAS rejects the now-stale part bytes (`staged.rs:617`). With N degraded chunks that must move, this performs N(N+1)/2 rebuilds across N passes for N adopted repairs. Track which part snapshots this pass has superseded and skip their remaining plans before erasure work. This preserves the accepted one-chunk-per-part-per-pass behavior; an in-place repair that leaves the part bytes unchanged should not invalidate other plans.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- [ ] **The rebuild criterion can accept the wrong payload.** `brief.md:25-31` requires an intact fragment with the right scheme, but never compares its payload with the lost shard or reconstructs the original data using it. On the target, `crates/core/src/repair.rs:58-90` checks header identity and geometry; even `fragment_intact` (`:94-110`) checks only those plus the fragment's own checksum. A newly encoded wrong payload can satisfy both checks and all the stated placement/ledger assertions. Revise A and C(vii) to require byte-for-byte equality with the seeded missing shards, or reconstruction of the seeded data from a survivor set that must use the repaired shards.
- [ ] **One of the four adoption fences has no adverse test.** Scope requires a CAS pinned to the pre-mark's bytes (`brief.md:118-119`), but B changes the session/part, D changes desired state, C(iii) starts with an already-reclaiming position, and F races only the session fence. None changes the destination mark after a successful write and before adoption. Target GC commits `reclaiming` before deleting bytes (`crates/custodian/src/gc.rs:589-595`, `:773-776`); omitting the adoption's mark precondition can therefore publish a placement naming deleted bytes while satisfying the stated cases. The existing DST adoption test constructs its own fenced batch (`crates/dst/tests/custodian.rs:3448-3452`), so it does not test the new repair path. Add a production-`reconcile_step` case that pauses after writing, lets GC reclaim the destination, then resumes adoption: it must lose, preserve the part record, and retain the obligation.
- [ ] **The instructed leg-G rewrite removes coverage on a false premise.** `brief.md:124-126` says this slice makes the existing Open-session test's “kept, no write” behavior false and requires retargeting it to a non-Open session. Its actual fixture is a lost `EcScheme::None` fragment (`crates/custodian/tests/staged_protection.rs:2359-2364`), which has no surviving redundancy. The brief itself keeps that scheme unrepairable (`brief.md:142-143`; target `crates/custodian/src/reconstruction.rs:769-773`), so keeping the obligation and writing nothing remain correct. Preserve that Open/nonredundant guard; specify any intended change to its `Blocked` result separately, and cover non-Open repair refusal with a repairable RS fixture.
- [x] size backstop — this slice is behaving oversized: patch is 211 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. OVERRIDDEN at sign-off (2026-09-21): the brief's own Ordering note already recorded that the human weighed a split on 2026-09-21, found no cut lands a working first slice under 100 KB, and accepted the ~200 KB size knowing the backstop would fire again. Not new information.

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
- Iteration delta (if iterating): Missing regression tests on the core durability property: no test proves destination writes are sent concurrently (join_all) rather than sequentially, and a sequential regression would let a legal-but-slow two-write repair blow its deadline and stay degraded forever — the brief's central invariant. Also close the two surviving-mutant gaps: outage classification (reconstruction.rs:913, Unreachable arm untested for staged chunks) and selective reading (staged.rs:212, stale/duplicate part-record filtering untested). Confirmed not defects, no rebuild needed for these: the EcScheme::None / single-copy "Blocked"->"Satisfied" behavior change (staged.rs:280, also T4 batch review's blocking finding) was requested by the brief. Size backstop overridden — already litigated in the brief's Ordering note on 2026-09-21. Efficiency finding (N(N+1)/2 rebuild cost, staged.rs:589) filed to Act log as a separate enhancement, not a blocker for this bundle. Three other §6 findings (wrong payload accepted, missing adoption-fence test, leg-G rewrite premise) read as stale against the current brief text and were left untouched, as were the harness count bug and the mixed-version D-server caveat.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- Enhancement candidate (from Check sign-off review, code-review finding): `crates/custodian/src/reconstruction/staged.rs:589` / `reconstruction.rs:445` — after a placement-changing adoption, later plans for the same part still redo full chunk rebuild/erasure work before their pre-mark CAS rejects stale part bytes (N(N+1)/2 rebuilds across N passes for N adopted repairs). Track superseded part snapshots and skip their remaining plans before erasure work. File as a standalone enhancement issue.
- (empty is the common case)
