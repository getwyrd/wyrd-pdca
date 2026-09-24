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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (29 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.0% — 456 of 475 instrumentable changed lines executed (floor 80%); 475 of 1190 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 83 mutants tested in 12m: 4 missed, 30 caught, 48 unviable, 1 timeouts

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

Review #814: rebuild degraded chunks of committed multipart parts while their sessions remain Open, without stranding fragments when a session, part, drain, or GC fence wins.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The repair, evidence-preservation and bounded-progress requirements are falsifiable; the latest scope is the three pre-mark race regressions, with earlier policy decisions explicitly settled (`brief.md:274`, `brief.md:280`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the regression file produces 26 assertion failures and 3 passing guards, with no compilation failure (`reviewer-red.log:230`; `crates/custodian/tests/staged_repair.rs:869`). |
| C3 Change | PASS | The patch supplies the missing staged repair within the accepted scope, keeps committed repair behavior, and updates the living architecture description (`crates/custodian/src/reconstruction.rs:442`, `docs/design/architecture/06-runtime-view.md:82`). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all 29 regressions pass; independent workspace and 50-seed DST checks pass, with the advisory-database permission fault recovered as detailed below (`reviewer-green-restored.log:36`, `reviewer-ci-remaining.log:533`, `reviewer-ci-remaining.log:595`). |
| C5 Causal adequacy | PASS | The missing repair is supplied at the production reconciliation seam; payload, slow-write and fence assertions exercise the required outcomes, and the four surviving mutations affect redundant postconditions on preserved fields (`crates/custodian/tests/staged_repair.rs:706`, `crates/custodian/tests/staged_repair.rs:1596`, `crates/custodian/src/reconstruction/staged.rs:755`). |
| T1 Structure | PASS | The staged path shares the existing staged walk and survivor classification through narrow store traits, preserving one reading for protection and CAS evidence (`crates/custodian/src/gc.rs:1537`, `crates/custodian/src/reconstruction.rs:901`, `crates/custodian/src/reconstruction/staged.rs:205`). |
| T2 Shape | PASS | Stored-record identity and decode boundaries are preserved, and the documentation describes the deployed protocol; the accepted bundle-size exception is already recorded (`crates/custodian/src/reconstruction/staged.rs:755`, `crates/core/src/multipart.rs:1927`, `docs/design/architecture/06-runtime-view.md:82`; `brief.md:127`). |
| T3 Runtime | PASS | Concurrent writes retain one clock-derived deadline, every adoption pins reclamation evidence, and deployed client timeouts bound external writes; the two-write timing regression and seeded fence sweep pass (`crates/custodian/src/reconstruction/staged.rs:616`, `crates/custodian/src/reconstruction/staged.rs:669`, `crates/custodian/src/reconstruction/staged.rs:707`, `crates/chunkstore-grpc/src/client.rs:265`). |
| T4 Contribution | PASS | Prior art was checked by all ten affected paths across merged history and closed/unmerged PRs; the batch-review budget objection is already settled, and contribution-artifact review is deferred to publish (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:12`, `brief.md:282`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The latest requested tests actually trigger each competing write before the pre-mark and assert zero fragment writes, retained obligation and preserved competing state; no new implementation or scope decision remains (`crates/custodian/tests/staged_repair.rs:1068`, `crates/custodian/tests/staged_repair.rs:1760`, `crates/custodian/tests/staged_repair.rs:1924`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this staged-repair behavior for the intended deployment and decide the real-environment follow-up — deterministic evidence establishes the protocol, while Tier-1 disk-fault and Tier-2 kill/reconstruct observation remains outside this Check (`AGENTS.md:78`, `AGENTS.md:81`). |

No new implementation defect found. The frozen batch-review gate remains red on an explicitly overridden finding; this advisory review does not change deterministic gate results. Source citations above resolve under `$PDCA_TARGET` (`target/`); brief and log citations resolve in this review directory. The target is readable and matches the supplied patch; no stale-target fallback was needed.

The independent checks substantiate the repair and its races:

- **Red→green reproduced:** `cargo test --offline -p wyrd-custodian --test staged_repair` failed by 26 assertions on the production base and passed all 29 cases after stash/pop. The tracked patch was restored byte-for-byte (`reviewer-red-green.log:24`). The frozen verifier's phrase “29 test(s) ran red” counts tests executed; its actual result is also 26 failures and 3 successes (`gate-logs/C4-verify.log:241`).
- **CI checks exercised:** the local `cargo xtask ci` passed typos, docs lint/render, hygiene guards, formatting, clippy, build, workspace tests and cargo-machete. It stopped only because cargo-deny could not lock the read-only shared advisory database (`reviewer-ci.log:3016`). All three real cargo-deny invocations subsequently passed with copied policy files differing only in `advisories.db-path`, using a writable database in this directory (`reviewer-deny.log:13`, `reviewer-deny-extra.log:10`, `reviewer-deny-extra.log:13`). Conformance, statics, and `cargo xtask dst` passed separately; the actual orchestrator scanner also ran against the workspace in its integration test (`reviewer-ci-remaining.log:1`, `reviewer-ci-remaining.log:595`, `reviewer-ci.log:2715`). This is a recovered host limitation, not a full uninterrupted local `xtask ci` success. The frozen complete CI run ended successfully (`gate-logs/C4-ci.log:3717`). Both named external dependencies, typos and docs-renderer, actually ran locally (`reviewer-ci.log:2`, `reviewer-ci.log:7`).
- **DST and payload evidence:** the 50-seed setting is wired by `xtask/src/main.rs:1573` and `xtask/src/main.rs:1607`; both staged-repair campaign cases passed, including the explicit four-position coverage assertion (`reviewer-ci-remaining.log:533`, `reviewer-ci-remaining.log:542`). The ordinary fixtures compare reconstructed shard bytes with the independently seeded encoding, including both multi-fragment cases (`crates/custodian/tests/staged_repair.rs:706`, `crates/custodian/tests/staged_repair.rs:1563`, `crates/custodian/tests/staged_repair.rs:1654`).
- **Remaining frozen evidence inspected:** diff coverage reports 456/475 instrumentable changed lines, 96.0%, with 715 unscored lines; it is not 96% of every changed line (`gate-logs/C4-diff-cov.log:794`). The TiKV log shows actual compilation of both feature selections (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). These two checks were adjudicated from their captured logs, not independently reproduced here.

The surviving mutants do not establish a missing reachable case. I independently reran all four reported `repointed_part` conjunction mutations; all survived (`reviewer-mutants.log:3`). Canonical decoding fixes the chunk-list encoding, the function substitutes only that list, and copies every other byte unchanged before decoding again (`crates/custodian/src/reconstruction/staged.rs:755`, `crates/core/src/multipart.rs:1927`). Thus the unchanged-field equalities cannot become false for a reachable input to this implementation; weakening their conjunction does not alter its result. The frozen timeout changes assignment completion from equality to inequality, preventing termination on a complete assignment; it is detected by timeout, not a survivor (`gate-logs/C5-mutants.log:13`). No capability probe masks an eager initialization cause.

The recorded dispositions remain settled. The wide-scheme batch budget in `gate-logs/T4-batch-review.log:10` was expressly overridden in `brief.md:282`; duplicate-reference convergence, accepted `Satisfied` classifications, bundle size, and the tracked #825 pre-mark-settlement work are not reopened (`brief.md:280`, `brief.md:284`, `crates/custodian/src/reconstruction/staged.rs:635`). **T4-contribution: N/A** — its artifacts are intentionally drafted after Check, and the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).

The prior-art investigation is complete for the affected paths. GitHub history was queried at frozen base `feb1e3047eb1dc194a058aaddb1c60076be70220`; all 344 closed PRs were enumerated and all 16 unmerged PRs checked for path overlap. The relevant non-dependency overlaps are #647 (segmented maps) and #336 (DST global-state policy), neither an alternative staged re-place; rejected local iterations are recorded in the brief (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:12`, `reviewer-prior-art.log:15`, `reviewer-prior-art.log:19`). Tier-1 and Tier-2 follow-up observation is warranted for this durability change, as the standing rubric requests; those campaigns were not run here.

### Advisory — adversary

# Adversarial review — issue #814 (iteration 4)

I tried to refute this in three ways. First, I re-ran the patch's tests in a scratch copy of `$PDCA_TARGET`: 36 passed in `staged_protection` and 29 in `staged_repair`. Second, I deleted each commit precondition in `staged::repair` one at a time and checked whether some test went red. Third, I checked the gate evidence. The production code held up. I found one test gap that the three new tests don't close.

## Findings

- NEEDS-HUMAN [human] — **Both of the adoption's pins on the vacated position's mark can be deleted with every test still green.** This is the same kind of gap iteration 3's sign-off asked to close for the pre-mark batch.
  - The pins are `require_absent` at `crates/custodian/src/reconstruction/staged.rs:718` and `require(key, current)` at `crates/custodian/src/reconstruction/staged.rs:721`.
  - I removed each one in turn and ran `staged_repair` and `staged_protection`: 29/29 and 36/36 passed both times.
  - The DST sweep can't catch them either. It only fences the session (`crates/dst/tests/custodian.rs:4816-4833`) and never touches the vacated mark.
  - **Concrete failing case.** The pass reads the vacated mark `orphan:3:<chunk>:2` in `assess` (`staged.rs:296-307`). GC then swaps that mark to `reclaiming` before the adoption commits. The production code is correct here: the adoption loses and GC's mark survives. Without the pin, the adoption commits and overwrites GC's `reclaiming` mark with `legacy(now)`. That breaks "no writer replaces a reclaiming mark" (`staged.rs:188-191`). The same missing pin would also let the adoption overwrite a mark that became unreadable after `assess` (ADR-0045, leg C(vi)'s rule).
  - I wrote a throwaway probe to confirm this. It uses `Fixture::standard` and an `after_read_of(fx.mark_key(3, 2))` hook that commits GC's `into_reclaiming()` swap. It has two cases: a seeded structured mark, and no mark where another event marks the position first. It asserts the `reclaiming` mark is byte-identical afterwards. The probe passes on the patch. It fails when the pin at `:721` is removed (seeded case) and when the pin at `:718` is removed (no-mark case).
  - Tagged `[human]`, not `[impl]`: iteration 3's sign-off said "Do not grow the patch beyond the three tests." Adding this roughly 40-line test is your call. The production behaviour is already right, so this doesn't block acceptance. It only guards against a future regression.

## Refutation attempts that failed

- **The three iteration-3 tests are real.** Deleting each guarded precondition turns exactly the intended test red:
  - pin on the destination's mark (`staged.rs:625` and `:624`) → `gc_reclaiming_the_destination_before_the_premark_writes_nothing`, which covers both arms;
  - `require(part)` (`:620`) → `a_part_record_rewritten_before_the_premark_writes_nothing`;
  - `require_absent(desired)` (`:628`) → `a_drain_recorded_before_the_premark_writes_nothing`.
- **Every other pin in the pre-mark and adoption batches is tested.** Deleting any of these turns exactly one test red: `:619`, `:700`, `:701`, `:707`, `:709`. The same holds for the `W_repoint` gate (`:657`), for overwriting a `reclaiming` vacated mark (`:723`), and for treating an unverified write as success (`:676`). Deleting the obligation removal (`:703`) turns 6 tests red.
- **Red→green evidence.** `gate-logs/C4-verify.log`: on the base, 26 tests fail by assertion. The 3 that pass are leg E's guards, which pass on the base by design. The log's "29 test(s) ran red" summary line overstates this. That miscount is a known harness issue, not a flaw in the fix.
- **C5's 4 surviving mutants (`staged.rs:766-769`, `&&`→`||` in `repointed_part`) look equivalent, not a missing test.** The decoder accepts only canonical bytes, and there `chunks` is the first field (see the spelling at `crates/custodian/tests/staged_repair.rs:456-458`). So the first match at `staged.rs:756-759` is always the real chunk list. The splice therefore can't change `len`, `digest`, `committed_at_millis` or `session_epoch`, and the other comparisons can never be false.
- **Why T4 fails.** It has one blocking finding: the batch operation budget at `staged.rs:704`. Iteration 3's sign-off already overrode that exact finding, so it is not new. It is the only gating red in `check-gates.json`.
- **Retried moves rewriting a position.** A position left behind by an earlier aborted attempt gets rewritten on retry. `FsChunkStore` publishes by atomic rename, last writer wins (`crates/chunkstore-fs/src/lib.rs:303-311`), so the retry never fails there.
- **Window wiring.** The only production `ReconstructionContext` passes `W_WRITE_MILLIS` (`crates/server/src/custodian.rs:572`). Every `staged_write_window_millis: 0` is in a test that never reaches the staged path.
- **DST coverage.** The new DST cases ran under `--cfg madsim` in C4-ci and passed (`gate-logs/C4-ci.log:3654`, `:3663`).

### Advisory — code-review

No new actionable findings on either lens: correctness bugs introduced by the diff, or reuse/simplification/efficiency, within the brief's accepted scope and recorded deferrals.

- The added pre-mark race tests exercise concurrent part replacement, GC reclaiming, and destination drain, and assert that no write arrives: `crates/custodian/tests/staged_repair.rs:1068`, `crates/custodian/tests/staged_repair.rs:1760`, `crates/custodian/tests/staged_repair.rs:1924`.
- The four surviving C5 mutants weaken defensive comparisons at `crates/custodian/src/reconstruction/staged.rs:765`. No reachable behavioral difference was identified: the canonical input and chunk-list replacement preserve those fields (`crates/core/src/multipart.rs:2584`, `crates/custodian/src/reconstruction/staged.rs:755`). They do not establish an additional test defect.

Validation used the frozen gate logs: CI passed, all 29 staged-repair tests passed, and changed-line coverage was 96.0%. The T4 batch-budget finding is explicitly settled in the brief. No checks were rerun and no target files were changed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept this staged-repair behavior for the intended deployment and decide the real-environment follow-up — deterministic evidence establishes the protocol, while Tier-1 disk-fault and Tier-2 kill/reconstruct observation remains outside this Check (`AGENTS.md:78`, `AGENTS.md:81`).
- [ ] **Both of the adoption's pins on the vacated position's mark can be deleted with every test still green.** This is the same kind of gap iteration 3's sign-off asked to close for the pre-mark batch.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- [ ] **The rebuild criterion can accept the wrong payload.** `brief.md:25-31` requires an intact fragment with the right scheme, but never compares its payload with the lost shard or reconstructs the original data using it. On the target, `crates/core/src/repair.rs:58-90` checks header identity and geometry; even `fragment_intact` (`:94-110`) checks only those plus the fragment's own checksum. A newly encoded wrong payload can satisfy both checks and all the stated placement/ledger assertions. Revise A and C(vii) to require byte-for-byte equality with the seeded missing shards, or reconstruction of the seeded data from a survivor set that must use the repaired shards.
- [ ] **One of the four adoption fences has no adverse test.** Scope requires a CAS pinned to the pre-mark's bytes (`brief.md:118-119`), but B changes the session/part, D changes desired state, C(iii) starts with an already-reclaiming position, and F races only the session fence. None changes the destination mark after a successful write and before adoption. Target GC commits `reclaiming` before deleting bytes (`crates/custodian/src/gc.rs:589-595`, `:773-776`); omitting the adoption's mark precondition can therefore publish a placement naming deleted bytes while satisfying the stated cases. The existing DST adoption test constructs its own fenced batch (`crates/dst/tests/custodian.rs:3448-3452`), so it does not test the new repair path. Add a production-`reconcile_step` case that pauses after writing, lets GC reclaim the destination, then resumes adoption: it must lose, preserve the part record, and retain the obligation.
- [ ] **The instructed leg-G rewrite removes coverage on a false premise.** `brief.md:124-126` says this slice makes the existing Open-session test's “kept, no write” behavior false and requires retargeting it to a non-Open session. Its actual fixture is a lost `EcScheme::None` fragment (`crates/custodian/tests/staged_protection.rs:2359-2364`), which has no surviving redundancy. The brief itself keeps that scheme unrepairable (`brief.md:142-143`; target `crates/custodian/src/reconstruction.rs:769-773`), so keeping the obligation and writing nothing remain correct. Preserve that Open/nonredundant guard; specify any intended change to its `Blocked` result separately, and cover non-Open repair refusal with a repairable RS fixture.
- [ ] size backstop — this slice is behaving oversized: patch is 227 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): issue_814 — rebuild only to close one test gap; the production code is not rejected (the adversary could not break it, every correctness gate passes, and the previous round's three requested tests are confirmed real). Add ONE test in `crates/custodian/tests/staged_repair.rs` for the adoption's two pins on the VACATED position's mark (`crates/custodian/src/reconstruction/staged.rs:718` `require_absent`, `:721` `require(key, current)`). Both can be deleted today with all 29 + 36 tests still green. Shape, per the adversary's probe: `Fixture::standard` with an `after_read_of(fx.mark_key(3, 2))` hook that commits GC's `into_reclaiming()` swap on the vacated mark after `assess` reads it (`staged.rs:296-307`) and before the adoption commits. Two arms: (a) a seeded structured mark on P_old, swapped to `reclaiming` — kills the `:721` pin; (b) no mark on P_old, another event marks it and GC swaps that to `reclaiming` — kills the `:718` pin. Assert: nothing adopted, `part:` byte-identical, the `reclaiming` mark byte-identical to what GC wrote (never overwritten with `legacy(now)`), obligation still queued. If either arm goes red on the current code, fix the code, not the case. Do not grow the patch beyond this one test. Confirmed at this sign-off, do not re-raise: the T4 batch-budget blocking finding (`staged.rs:704`) and the size backstop stay overridden as before; the three plan-advisory §6 findings (wrong payload, adoption-fence test, leg-G premise) are stale — each is already answered by the current brief and patch (`staged_repair.rs:716-717`, `:1678`, `staged_protection.rs` leg G hunk) and should not be acted on.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
