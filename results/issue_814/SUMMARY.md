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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (30 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.0% — 456 of 475 instrumentable changed lines executed (floor 80%); 475 of 1190 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 83 mutants tested in 12m: 4 missed, 30 caught, 48 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.95s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #814’s staged-chunk reconstruction under the session fence: functional verification passes, with one reproducible runtime memory defect remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes durability, slow-write progress, and losing-CAS behavior falsifiable, including the latest vacated-mark regression; `brief.md:18`, `brief.md:293`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new test produced 27 assertion failures and 3 passes, establishing the missing staged repair on the actual base; `reviewer-red.log:377`. |
| C3 Change | PASS | The changes stay within the authorized staged-repair surface and update its living architecture description; the latest requested two-arm vacated-mark test is present; `target/crates/custodian/tests/staged_repair.rs:1841`, `target/docs/design/architecture/06-runtime-view.md:82`. |
| C4 Verification (red→green) | PASS | Restoring the patch yielded 30/30 passes; independent DST passed, and frozen CI/feature-build logs support the checks unavailable to this sandbox; the local advisory-lock failure is a host limitation; `reviewer-green.log:38`, `reviewer-remaining-checks.log:550`, `reviewer-ci.log:3018`, `gate-logs/C4-ci.log:3718`. |
| C5 Causal adequacy | PASS | Slow-write concurrency and both vacated-mark adoption pins are exercised: deleting either pin independently makes the new regression fail; the fix removes the sequential-write cause without a capability-probe workaround; `target/crates/custodian/src/reconstruction/staged.rs:669`, `reviewer-focused-mutants.log:35`, `reviewer-focused-mutants.log:58`. |
| T1 Structure | PASS | Staged protection and repair share one ordered read, while the repair uses the existing trait and clock seams; shared gathering preserves committed-path classification; `target/crates/custodian/src/gc.rs:1537`, `target/crates/custodian/src/reconstruction.rs:841`. |
| T2 Shape | PASS | The protocol is isolated in a staged module and its encoding preserves non-placement fields; size and state-result decisions are already settled; `target/crates/custodian/src/reconstruction/staged.rs:746`, `target/crates/custodian/tests/staged_repair.rs:891`, `brief.md:124`, `brief.md:293`. |
| T3 Runtime | FAIL | Whole-part replacement buffers accumulate per queued chunk before any repair: a valid 10,112-obligation probe added 143 MiB RSS before its first destination write, creating avoidable memory pressure during recovery; `target/crates/custodian/src/reconstruction/staged.rs:331`, `target/crates/custodian/src/reconstruction.rs:325`, `reviewer-memory.log:33`. |
| T4 Contribution | FAIL | One batch-review finding is independently substantiated by the memory probe; three duplicate-reference findings are settled. Prior art was rechecked by all ten affected paths; contribution-artifact auditing is deferred to publish; `gate-logs/T4-batch-review.log:10`, `brief.md:284`, `reviewer-prior-art.log:22`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Remove the avoidable per-obligation whole-part allocation before reassessment—large repair backlogs can consume custodian memory before the first repair begins; this is distinct from the already-deferred repeated rebuild work; `target/crates/custodian/src/reconstruction/staged.rs:154`, `brief.md:246`, `reviewer-memory.log:33`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm deployment fitness and a deadline-enforcing D-server fleet—the tests establish the enforced-deadline case, while older servers ignore that field and real-storage fault campaigns were not exercised here; `target/crates/chunkstore-grpc/src/client.rs:274`, `target/AGENTS.md:78`. |

One implementation finding remains: **[P2] Defer encoding each replacement part until its repair executes** (`target/crates/custodian/src/reconstruction/staged.rs:331`). Assessment builds a complete replacement value for every repairable chunk and retains it in `StagedTarget.next`; reconstruction retains every plan until the assessment finishes (`target/crates/custodian/src/reconstruction.rs:321`). Thus a part with N queued chunks retains N encoded copies despite the original part snapshot being shared. This can exhaust recovery-worker memory on a large backlog before any destination write. Keeping the part index and destinations in the plan, then constructing and checking the replacement before that plan’s pre-mark/write, avoids this additional retention without changing the accepted one-chunk-per-part-per-pass behavior.

The independent probe called the production `reconcile_step` with canonical, distinct-chunk part records, RS(2,1), one missing fragment per chunk, and 158 chunks per part—the shipped cap (`target/crates/core/src/multipart.rs:4431`, `target/crates/core/src/multipart.rs:4453`). At 64 parts, each part encoded to 13,100 bytes: the retained replacement buffers therefore account for 132,467,200 bytes (126.3 MiB). Measured RSS rose from 29,172 KiB to 175,776 KiB at the first write, an increase of 143.2 MiB including other assessment allocations. Smaller runs showed the same growth: 16 parts added 36,908 KiB; one part added 2,608 KiB (`reviewer-memory.log:13`, `reviewer-memory.log:23`, `reviewer-memory.log:33`). This proves allocation amplification, not an observed OOM. The probe is retained in `pdca-reviewer-814-memory/tests/memory.rs`; it did not modify target production code. Reproduce from this review directory:

```bash
REVIEW_PARTS=64 CARGO_TARGET_DIR="$PWD/pdca-reviewer-814-build" TMPDIR="$PWD/pdca-reviewer-814-tmp" cargo +1.96.0 test --offline --manifest-path pdca-reviewer-814-memory/Cargo.toml --test memory reviewer_memory_probe -- --nocapture
```

The latest requested test closes its specific gap. Its baseline passed; isolated copies deleting `VacatedMark::Absent`’s absence precondition or `VacatedMark::Stamped`’s exact-value precondition each compiled and failed by assertion because the adoption incorrectly returned `Changed` (`reviewer-focused-mutants.log:10`, `reviewer-focused-mutants.log:21`, `reviewer-focused-mutants.log:44`). The target remained unchanged. The test also requires that the destination actually received and retained the write, preserves the reclaiming mark byte-for-byte, and keeps the obligation (`target/crates/custodian/tests/staged_repair.rs:1879`).

Verification evidence supports the functional verdict with these limits:

- **Independent checks:** 27 red assertions / 30 green tests, `git diff --check`, formatting, workspace clippy/build/tests, typos, docs lint and the 99-page render/link audit, cargo-machete, conformance vectors, the statics scanner, and DST clippy/tests passed. `cargo xtask dst` supplies 50 seeds; both staged-repair properties passed (`target/xtask/src/main.rs:1573`, `reviewer-remaining-checks.log:533`, `reviewer-remaining-checks.log:542`). The target matches the supplied patch, as confirmed by reverse-apply checking after the stash was restored.
- **C4-ci host caveat:** the independent full CI invocation stopped at cargo-deny because its advisory database lock is outside the writable sandbox (`reviewer-ci.log:3018`). The frozen log explicitly shows all three dependency-policy checks succeeding and the complete CI finishing (`gate-logs/C4-ci.log:3110`, `gate-logs/C4-ci.log:3121`, `gate-logs/C4-ci.log:3124`, `gate-logs/C4-ci.log:3718`). This is not a verification defect in the patch. The brief’s named external dependencies, typos and docs-renderer, actually ran in this review (`reviewer-ci.log:3`, `reviewer-ci.log:9`).
- **C4-diff-cov and host-tikv:** assessed from their captured logs, not claimed as independent reruns. Coverage is 456/475 instrumentable changed lines, 96.0%; 715 further changed lines were unscored (`gate-logs/C4-diff-cov.log:763`, `gate-logs/C4-diff-cov.log:795`). The feature log shows actual TiKV and server compilation, ending successfully (`gate-logs/host-tikv.log:207`).
- **C5-mutants:** the advisory gate remains red: 4 missed, 30 caught, 48 unviable, 1 timeout (`gate-logs/C5-mutants.log:18`). The four survivors weaken defensive read-back equalities after canonical chunk-array replacement (`target/crates/custodian/src/reconstruction/staged.rs:755`, `target/crates/core/src/multipart.rs:1927`). The scalar fields are outside the replaced array, and the end-to-end test asserts their preservation (`target/crates/custodian/tests/staged_repair.rs:891`); the log does not establish a reachable defect from those survivors. They are distinct from the two newly checked, assertion-killed adoption-pin mutations.
- **T4 batch review:** three of its four entries repeat the duplicate-reference finding explicitly settled at sign-off (`brief.md:284`). The remaining memory finding is grounded above. The accepted batch-budget, size, state-result, and #825 deferral decisions remain settled; the previously deferred repeated rebuild CPU cost is not raised again.
- **T4-contribution: N/A.** `pr-description.md` is intentionally not drafted at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).

The prior-art check was rerun by every affected path. Read-only GitHub queries traversed merged history at the frozen base `feb1e30` and fetched changed-file lists for all 16 closed, unmerged PRs. The functional-path overlaps remain #647 (segmented maps) and #336 (DST global-state work); additional overlaps are dependency-only `Cargo.lock` changes. Neither new staged file exists in that merged history (`reviewer-prior-art.log:27`, `reviewer-prior-art.log:29`, `reviewer-prior-art.log:39`, `reviewer-prior-art.log:44`). The rejected local iterations are documented in the brief, and their current requested regression is independently verified above. Full query results are in `reviewer-prior-art.json`.

For deployment sign-off, retain the existing requirement that D servers enforce the supplied deadline; this review did not establish mixed-version safety. Tier-1 disk-fault and Tier-2 kill-and-reconstruct observation is warranted for this durability change, as the target rubric requests (`target/AGENTS.md:78`, `target/AGENTS.md:81`). Those operational checks complement the passing seeded simulation and remain outside this review’s exercised environment. All verdicts here are advisory; deterministic gate disposition remains the harness’s responsibility.

### Advisory — adversary

# Adversarial review — issue_814 (iteration 5)

This round changed one thing: a new test, `gc_reclaiming_the_vacated_position_before_the_adoption_makes_it_lose`, which the iteration-4 sign-off asked for so the adoption's two pins on the vacated position's mark would be tested. I tried to refute it, and the production code around it, and could not. I found one item that needs a human decision: the gating T4 gate still reports a blocking finding that is new this round. It is overstated, but only a human can override it.

## Refutation attempts

- Attempted to refute the new test by **deleting each pin it is meant to catch**, and could not. I ran this in a scratch copy of `$PDCA_TARGET`: `cargo test -p wyrd-custodian --test staged_repair`.
  - Unmutated: 30 of 30 pass.
  - Deleting `.require_absent(key.clone())` at `crates/custodian/src/reconstruction/staged.rs:718`: the new test fails in its "no mark" arm (`crates/custodian/tests/staged_repair.rs:1888`, "nothing was adopted"). The other 29 tests still pass.
  - Deleting `.require(key.clone(), current.clone())` at `staged.rs:721`: the same test fails in its "a stamped mark" arm (`staged_repair.rs:1888`). The other 29 still pass.

  Both pins that the iteration-4 sign-off said "can be deleted today with all tests green" are now caught, each by its own arm.
- Attempted to make the new test **pass for the wrong reason**, and could not.
  - It runs the production pass (`fx.run()`).
  - Its hook fires after the test double's first `get` of the vacated key (`staged_repair.rs:206-218`, run once and then retired). That first read is `assess`'s read at `staged.rs:296`, before destinations are chosen, so the race it stages is the real one.
  - A pass that writes nothing, or one the pre-mark batch stopped, cannot satisfy it. It asserts:
    - exactly one arrival on the destination (`staged_repair.rs:1879-1883`);
    - the fragment is held there (`:1884-1887`);
    - a fresh pre-mark still stands (`:1899-1903`);
    - a `conflict` audit event names this chunk (`:1905-1908`).
  - Each arm uses its own chunk id (`0x8161`/`0x8162`, `:1846`; no other test uses them), so the audit log that all tests share cannot hand it another test's event.
- Attempted to strand a fragment through the **unpinned arm**, `VacatedMark::Reclaiming => adopt` (`staged.rs:723`), and could not.
  - GC never turns a `reclaiming` mark back into another shape. It resumes the delete (`crates/custodian/src/gc.rs:671-678`), and only the sweep of marks with no fragment under them retires the mark, once the fragment is gone (`gc.rs:1845-1847`).
  - While the part record still names the position, GC's reference check wins over the mark (`gc.rs:654-667`), so GC keeps the mark and finishes after the adoption.
  - So a vacated position read as `reclaiming` stays covered until its bytes are gone.
- **C5's 4 surviving mutants** (`staged.rs:766-769`, `&&` → `||` in `repointed_part`) cannot be told apart from the original code in any reachable state, so they are not a test gap.
  - The canonical part-record spelling puts `"chunks":[...]` first. The test fixture checks it matches the decoder's own spelling (`staged_repair.rs:457-470`).
  - The splice at `staged.rs:755-763` therefore always replaces the real chunk list and changes no other byte, so every conjunct at `:765-769` is always true.
  - The same 4 survived v3 and v4, and neither sign-off asked for them.
- **C4-verify's "30 test(s) ran red"** is the harness's known counting bug. The log shows 27 failed and 3 passed on the base (`gate-logs/C4-verify.log:248`). The 3 that pass on the base are the leg-E guards, which the brief says are green on the base by design. The new test fails on the base because nothing is rebuilt ("GC never reclaimed the vacated position"), which the brief allows for rule cases. The mutations above show its rule branch is reached on the fixed code.
- The DST cases (DST: deterministic simulation testing) ran under madsim inside the gating C4-ci run and passed: `staged_replace_under_the_fence_strands_nothing` and `staged_replace_reaches_every_point_of_the_fence` (`gate-logs/C4-ci.log:3376`, `:3655`, `:3664`).

## Needs a human

- NEEDS-HUMAN [human] — **T4 (gating) fails with 4 blocking findings. Three are settled; the fourth is new and overstated.**
  - The three at `crates/custodian/src/reconstruction/staged.rs:212` (the first-reference rule for duplicate part references) were overridden at the iteration-3 sign-off ("do not re-raise").
  - The new one is at `staged.rs:343`: keeping a full repointed part record (`next`) in every staged plan "multiplies assessment memory… tens of gigabytes". My measurement says the claim is overstated:
    - Every `RepairPlan`, committed or staged, already holds its survivors' decoded shard bytes (`crates/custodian/src/reconstruction.rs:167-168`). That is about one chunk per plan: 1 MiB by default (`crates/server/src/lib.rs:51`).
    - `next` is capped near `MAX_VALUE_BYTES = 100_000` (`crates/core/src/metadata.rs:549`). A stored record is at most that size, and `staged.rs:582` refuses anything over the limit.
    - So `next` adds at most about 10% per plan on top of what the base already holds. Any "tens of GB" comes from the base's shape — assess every queued chunk, then repair (`reconstruction.rs:320-327`, `:440-446`) — not from this patch.
  - The shape does depart from the committed path's written rule, "an index, never a copy… not Q×N" (`reconstruction.rs:181-185`). Building `next` lazily inside `staged::repair` would fix that, but it grows the patch, which the brief forbids this round.
  - Decision needed: override T4 `:343` as bounded and non-blocking, or file it as a follow-up. It is a sign-off call, not a rebuild.

## Bottom line

I tried to refute:
- the new test: by deleting each pin, and by looking for ways it could pass wrongly;
- the unpinned `reclaiming` arm;
- the C5 survivors;
- the C4-verify count;
- the new T4 finding.

I could not refute the fix. The only open item is the T4 override above.

### Advisory — code-review

- NEEDS-HUMAN [impl] — **Avoid retaining a full part value per queued chunk.** `crates/custodian/src/reconstruction/staged.rs:331` builds an entire rewritten `part:` value for every repairable chunk and retains it in `StagedTarget.next` at `crates/custodian/src/reconstruction/staged.rs:343`. All plans accumulate before any repair executes (`crates/custodian/src/reconstruction.rs:325`), so Q obligations within an N-chunk part require O(Q×N) resident metadata despite sharing the original part snapshot. A server failure affecting many full parts can therefore add gigabytes before the first adoption, risking an out-of-memory restart without repair progress. Keep the shared part index and destination edits in each plan, and build/validate the replacement bytes when executing that repair, before its pre-mark and fragment writes. This concerns peak memory within one pass; the brief's deferred triangular rebuild cost across passes remains settled.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Remove the avoidable per-obligation whole-part allocation before reassessment—large repair backlogs can consume custodian memory before the first repair begins; this is distinct from the already-deferred repeated rebuild work; `target/crates/custodian/src/reconstruction/staged.rs:154`, `brief.md:246`, `reviewer-memory.log:33`.
- [x] Validation — fitness-to-purpose — Confirm deployment fitness and a deadline-enforcing D-server fleet—the tests establish the enforced-deadline case, while older servers ignore that field and real-storage fault campaigns were not exercised here; `target/crates/chunkstore-grpc/src/client.rs:274`, `target/AGENTS.md:78`.
- [x] **T4 (gating) fails with 4 blocking findings. Three are settled; the fourth is new and overstated.**
- [x] **Avoid retaining a full part value per queued chunk.** `crates/custodian/src/reconstruction/staged.rs:331` builds an entire rewritten `part:` value for every repairable chunk and retains it in `StagedTarget.next` at `crates/custodian/src/reconstruction/staged.rs:343`. All plans accumulate before any repair executes (`crates/custodian/src/reconstruction.rs:325`), so Q obligations within an N-chunk part require O(Q×N) resident metadata despite sharing the original part snapshot. A server failure affecting many full parts can therefore add gigabytes before the first adoption, risking an out-of-memory restart without repair progress. Keep the shared part index and destination edits in each plan, and build/validate the replacement bytes when executing that repair, before its pre-mark and fragment writes. This concerns peak memory within one pass; the brief's deferred triangular rebuild cost across passes remains settled.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- [x] **The rebuild criterion can accept the wrong payload.** `brief.md:25-31` requires an intact fragment with the right scheme, but never compares its payload with the lost shard or reconstructs the original data using it. On the target, `crates/core/src/repair.rs:58-90` checks header identity and geometry; even `fragment_intact` (`:94-110`) checks only those plus the fragment's own checksum. A newly encoded wrong payload can satisfy both checks and all the stated placement/ledger assertions. Revise A and C(vii) to require byte-for-byte equality with the seeded missing shards, or reconstruction of the seeded data from a survivor set that must use the repaired shards.
- [x] **One of the four adoption fences has no adverse test.** Scope requires a CAS pinned to the pre-mark's bytes (`brief.md:118-119`), but B changes the session/part, D changes desired state, C(iii) starts with an already-reclaiming position, and F races only the session fence. None changes the destination mark after a successful write and before adoption. Target GC commits `reclaiming` before deleting bytes (`crates/custodian/src/gc.rs:589-595`, `:773-776`); omitting the adoption's mark precondition can therefore publish a placement naming deleted bytes while satisfying the stated cases. The existing DST adoption test constructs its own fenced batch (`crates/dst/tests/custodian.rs:3448-3452`), so it does not test the new repair path. Add a production-`reconcile_step` case that pauses after writing, lets GC reclaim the destination, then resumes adoption: it must lose, preserve the part record, and retain the obligation.
- [x] **The instructed leg-G rewrite removes coverage on a false premise.** `brief.md:124-126` says this slice makes the existing Open-session test's “kept, no write” behavior false and requires retargeting it to a non-Open session. Its actual fixture is a lost `EcScheme::None` fragment (`crates/custodian/tests/staged_protection.rs:2359-2364`), which has no surviving redundancy. The brief itself keeps that scheme unrepairable (`brief.md:142-143`; target `crates/custodian/src/reconstruction.rs:769-773`), so keeping the obligation and writing nothing remain correct. Preserve that Open/nonredundant guard; specify any intended change to its `Blocked` result separately, and cover non-Open repair refusal with a repairable RS fixture.
- [x] size backstop — this slice is behaving oversized: patch is 232 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (issue_814 sign-off): staged repair keeps a full rewritten `part:` value per queued chunk (`StagedTarget.next`, `crates/custodian/src/reconstruction/staged.rs:331`, `:343`) until the whole assessment finishes. That is O(Q×N) memory and breaks the committed path's "an index, never a copy" rule (`reconstruction.rs:181-185`). Build `next` lazily in `staged::repair`, before the pre-mark. The adversary measured about 10% extra per plan, not tens of GB.
