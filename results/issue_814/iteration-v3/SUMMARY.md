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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (26 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.0% — 456 of 475 instrumentable changed lines executed (floor 80%); 475 of 1190 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 83 mutants tested in 12m: 4 missed, 30 caught, 48 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.97s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #814: rebuild degraded committed multipart chunks while their sessions are Open, preserving repair progress and evidence for every written fragment.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable payload, fence, deadline, GC and progress obligations, including the previous concurrency regression; `brief.md:17`, `brief.md:104`, `brief.md:245`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the new test reproduced 23 assertion failures and 3 passes; `reviewer-red.log:346`. |
| C3 Change | PASS | The patch stays within staged reconstruction, shared reading, tests and living documentation; the single-copy behavior and bundle size were explicitly accepted; `crates/custodian/src/reconstruction.rs:445`, `brief.md:134`, `brief.md:246`. |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 26 new tests, 51 existing reconstruction/protection tests and the 50-seed DST run; frozen CI supplies the remaining gate evidence, with the local advisory-cache caveat below; `reviewer-restored-green.log:96`, `reviewer-dst.log:541`, `gate-logs/C4-ci.log:3714`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Restore bounded progress for wide supported schemes — the production path creates over-budget transactions, so passing small-scheme tests does not establish the brief's progress invariant; `crates/custodian/src/reconstruction/staged.rs:621`, `crates/custodian/src/reconstruction/staged.rs:704`, `reviewer-probes.log:14`. |
| T1 Structure | PASS | The repair uses existing trait seams and one lifecycle clock, and its concurrent writes remain owned by the calling task; `crates/custodian/src/reconstruction.rs:125`, `crates/custodian/src/reconstruction/staged.rs:669`. |
| T2 Shape | PASS | The staged protocol has a private module, the living runtime description is current, and formatting/clippy pass; the accepted size exception is not reopened; `crates/custodian/src/reconstruction.rs:67`, `docs/design/architecture/06-runtime-view.md:82`, `brief.md:246`. |
| T3 Runtime | FAIL | RS(2,255) produces 767-operation pre-mark and 1,279-operation adoption batches against the documented 500-operation budget, exposing repeated timeout/no-progress risk; `crates/custodian/src/reconstruction/staged.rs:699`, `crates/core/src/multipart.rs:4551`, `reviewer-probes.log:14`. |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit is mandatory at publish; path-based prior-art checking completed separately below; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN | Decide whether duplicate committed-part references are supported or must be rejected — repeated repair passes certify the first while the second remains degraded, but normal gateway allocation makes IDs unique; `crates/custodian/src/reconstruction/staged.rs:212`, `crates/custodian/tests/staged_repair.rs:2060`, `crates/server/src/lib.rs:245`, `reviewer-probes.log:6`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Judge durability fitness for the intended deployment and the need for Tier-1 disk-fault/Tier-2 kill-reconstruct observation — the exercised protocol races do not establish those deployment outcomes; `AGENTS.md:78`, `crates/custodian/src/reconstruction/staged.rs:60`. |

The advisory conclusion is to repair the batch-budget gap and settle duplicate-reference scope before sign-off. The requested red→green behavior and concurrency regression pass; the two additional probes expose limits that those tests do not cover. This review does not gate acceptance or change the frozen gate verdicts.

Source citations are relative to `$PDCA_TARGET` (`target/`); evidence citations are relative to this review directory. The target matches the supplied patch: `git apply --reverse --check ../patch.diff` passed after restoration. No target-state caveat applies, and no production source was edited by this review.

**Confirmed implementation gap: bound both transaction stages.** With two surviving shards and 255 missing shards on distinct available domains, the real `reconcile_step` creates 767 operations before writing and 1,279 when adopting. The independent probe compiled and ran against the patched production crates (`reviewer-probes/tests/probe.rs:2138`; `reviewer-probes.log:14`). The counts follow directly from `2 + 3N` pre-mark operations and `4 + 5N` adoption operations for relocated positions whose old marks are absent. Both exceed `MAX_BATCH_OPS = 500` (`crates/core/src/multipart.rs:4562`). The adoption would take 6.395 seconds at the documented 5 ms/operation assumption, beyond its five-second envelope; TiKV actually awaits these operations sequentially (`crates/metadata-tikv/src/lib.rs:1386`, `crates/metadata-tikv/src/lib.rs:1413`).

Bound each move's fragments so both batches fit, retain the repair obligation while fragments remain missing, and add a wide-scheme progress regression. The probe establishes the operation-budget violation. A real TiKV timeout was not measured; the latency figure remains the documented assumption, whose calibration is already assigned to #625. This supports the first frozen batch-review finding with that qualification (`gate-logs/T4-batch-review.log:10`).

**Scope decision: duplicate references cannot currently converge.** Starting with the new test's own two-part fixture, the first pass returns `Changed`, points the first part at `[0,1,2]`, and drains the obligation. Two further requeues each return `Satisfied` and drain again, while the second part still names `[0,1,3]` and server 3 lacks its fragment (`reviewer-probes.log:6`). Scrub checks both staged placements and requeues the missing one (`crates/custodian/src/scrub.rs:239`, `crates/custodian/src/scrub.rs:296`). The new first-reference test checks only the initial repair and expressly requires the second record to remain unchanged (`crates/custodian/tests/staged_repair.rs:2080`).

The second frozen batch-review finding therefore reproduces on accepted metadata. Its production reach is a scope question: gateway allocation promises unique chunk IDs, and the brief asks for duplicate filtering without specifying alias-repair semantics. Choose whether such duplicate records require repair, explicit inconsistency handling, or a documented invariant excluding them. This is plain NEEDS-HUMAN rather than an implementation-only rebuild instruction; no normal writer path producing this duplicated fixture was established.

**Verification is substantive, with one local host limitation.** The independent red leg compiled successfully and failed by assertion; the restored green leg passed 26 staged-repair, 36 staged-protection and 15 reconstruction tests. The concurrency case makes each write take 20 seconds, yields before publication, and requires both arrivals at the pre-mark instant (`crates/custodian/tests/staged_repair.rs:358`, `crates/custodian/tests/staged_repair.rs:1541`). Sequential sends cannot satisfy those assertions. Payload comparisons and GC's between-write-and-adoption reclaim case are present (`crates/custodian/tests/staged_repair.rs:1598`, `crates/custodian/tests/staged_repair.rs:1623`). The prior concurrency and outage-classification gaps are addressed.

The independent `cargo xtask ci` passed typos, docs lint/render, repository guards, fmt, workspace clippy/build/tests and cargo-machete, then stopped because cargo-deny could not lock the read-only global advisory cache (`reviewer-ci.log:3013`). This is a host limitation. Separate `cargo xtask conformance`, `cargo xtask statics` and `cargo xtask dst` passed; DST uses 50 seeds and exercises all four fence landings (`xtask/src/main.rs:1573`, `crates/dst/tests/custodian.rs:4960`, `reviewer-dst.log:533`). Both named external tools, typos and docs-renderer, actually ran (`reviewer-ci.log:2`, `reviewer-ci.log:7`).

The instance-scoped wrappers were adjudicated from their frozen logs: complete CI passed (`gate-logs/C4-ci.log:3714`); diff coverage was 456/475 instrumentable changed lines, 96.0%, with 715 changed lines unscored (`gate-logs/C4-diff-cov.log:791`); actual TiKV and server feature compilation passed (`gate-logs/host-tikv.log:209`). These are captured gate outcomes, not claims of independent reruns of those wrappers. The contribution row remains deferred to its publish audit, not an unmet dependency.

**The remaining mutants do not establish another defect.** The frozen campaign reports four missed mutants, 30 caught, 48 unviable and one timeout (`gate-logs/C5-mutants.log:13`). All four misses weaken the final `repointed_part` conjunction. For inputs admitted by the canonical decoder, the byte splice changes only the chunk placement; length, digest, commit time and session epoch are copied unchanged, making these alternatives equivalent on the reachable input set (`crates/core/src/multipart.rs:1927`, `crates/core/src/multipart.rs:2495`, `crates/custodian/src/reconstruction/staged.rs:755`). The destination-selection mutant timed out; it is not a surviving green case. The fix removes the sequential timing cause rather than adding a capability probe around an eager side effect, so the C5 symptom-guard trigger does not apply.

**Prior art was checked by all ten affected paths.** The independent GitHub query read each path's merged history and all 344 closed PRs, then compared the changed-file lists of all 16 closed-unmerged PRs (`reviewer-prior-art.json:890`). Relevant closed overlaps were #647 (segmented maps) and #336 (DST global-state policy); the remaining matches were Cargo.lock dependency updates. The new staged module and test have no merged history. The base's behavior is independently covered by the red leg. The brief records the rejected earlier #814/#663/#637 attempts and their dispositions; the accepted size, single-copy result, efficiency follow-up and tracked #825 settlement are not re-raised (`brief.md:195`, `brief.md:246`, `crates/custodian/src/reconstruction/staged.rs:635`).

The additional probes are review artifacts, outside the target source. Reproduce their two assertions with `CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline --manifest-path reviewer-probes/Cargo.toml --test probe reviewer_ -- --nocapture`. The harness owns disposal of the review directory and its build artifacts.

### Advisory — adversary

# Adversarial review — #814 (staged re-place), iteration 3

Summary: I could not break the production code. I re-ran the green leg on a scratch copy of the patched tree: `staged_repair` passed 26/26 and `staged_protection` 36/36. The red leg in `gate-logs/C4-verify.log` fails on assertions, not on compile errors. The DST (deterministic simulation test) properties ran in C4-ci and passed (28/28 in `crates/dst/tests/custodian.rs`). The real gap is in the tests. I deleted each precondition of the pre-mark batch by hand, and three of those deletions leave every test green. One of them is the precondition that makes C(iii) hold when GC runs at the same time. It needs a test, and I have written that test and shown it works.

## Findings

- NEEDS-HUMAN [impl] — **No test covers the pre-mark's precondition on the destination's existing mark** (`crates/custodian/src/reconstruction/staged.rs:624-625`). If both arms are changed to plain `batch` (no precondition), all 26 + 36 tests still pass. This precondition is what keeps C(iii) ("a position with a `reclaiming` mark is never written") true when GC races the pass. The race: `position()` reads the destination's mark (`staged.rs:534`). GC then swaps that mark to `reclaiming` (`crates/custodian/src/gc.rs:596-604`) before the pre-mark commits. GC's `destroy` deletes the fragment and then deletes the mark key with no precondition (`gc.rs:813-825`). Without the pin, the pre-mark overwrites GC's `reclaiming` mark and the rebuilt fragment is written into a position GC is deleting. Depending on timing, one of two things happens. Either the adoption commits over bytes GC has just deleted, so the obligation drains and the fragment is gone. Or GC deletes the pre-mark first, the adoption loses, and a fragment written after GC's delete is left with no mark and no record naming it (stranded). The current code is correct. I checked with a scratch test: seed a structured mark at `mark_key(2, 2)` on `Fixture::standard`, and add an `after_read_of(mark_key(2, 2))` hook that commits GC's swap to `into_reclaiming()`. Then assert: no write arrived, the `reclaiming` mark is byte-identical, and the obligation is still queued. It passes on the patch. On the mutant it fails with `a position GC decided to reclaim was written: [(2, FragmentId{..index: 2}, Some(40000))]`. Please add it. The existing C(iii) case (`crates/custodian/tests/staged_repair.rs:1149`) seeds the `reclaiming` mark before the pass starts, so it never runs this race. The DST runs no GC pass at all.

- NEEDS-HUMAN [impl] — **The same gap, lower stakes:** the pre-mark's `require(part == prior)` (`staged.rs:620`) and its `require_absent(desired:dserver:<S_new>)` (`staged.rs:628`) can each be deleted with every test still green. The module doc says both make the pre-mark lose with nothing written (`staged.rs:33-35`, `:53-54`). Only the session-pin version of that claim has a test (`staged_repair.rs:1032`). The drain test records the drain after the write is stored (`staged_repair.rs:1767-1769`), so it only exercises the adoption's pin. Safety still holds without these two pins: the adoption CAS refuses, and the wasted write stays under its pre-mark. So this is about pinning the behavior the doc promises. Add two more `after_read_of(mark_key(2, 2))` cases, one that rewrites the part record and one that seeds `desired:dserver:2`. Each should assert that no write arrives.

- NEEDS-HUMAN [human] — **The brief says leg E is "GREEN on the base by design", but one leg-E case ran red on the base.** `a_degraded_chunk_with_no_usable_destination_is_kept` (`staged_repair.rs:1856`) failed on the base with `left: Blocked, right: Satisfied` (`gate-logs/C4-verify.log`). It asserts that the pass now answers `Satisfied` where `main` answered `Blocked`. The below-k-behind-an-outage case (`staged_repair.rs:2006`) makes the same change. Both match the committed path: `Blocked` and `Unreachable` feed gauges, not the pass's `hole` (`crates/custodian/src/reconstruction.rs:336`, `:342`, `:488-492`). But these are a second and third Blocked→Satisfied change, beyond the leg-G one the brief put in front of sign-off. For these chunks an operator now sees only the `reconstruction_repair_blocked` or `reconstruction_unreachable` gauge, never a `Blocked` pass. Sign-off should confirm this is intended.

- NEEDS-HUMAN [human] — **T4 blocking #1 (batch operation limit): real, but only for very wide codes.** The adoption carries 4 operations plus up to 5 per moved fragment (`staged.rs:699-722`), and the pre-mark carries 2 plus 3 per fragment. The limit is `MAX_BATCH_OPS` = 500 (`crates/core/src/multipart.rs:4562`). So the adoption goes over only when one chunk has 100 or more fragments to move. That needs RS with m ≥ 100 and at least 100 free failure domains. Nothing in the config caps m, so it can happen in principle, but not with realistic profiles. The committed repoint has the same unbounded shape at a higher threshold (`reconstruction.rs:1187-1196`). If a guard is wanted, it is cheap: before writing anything, classify a move as `Blocked` when `4 + 5·n > MAX_BATCH_OPS`. This is a judgment call, not a correctness defect at realistic widths.

- NEEDS-HUMAN [human] — **T4 blocking #2 (only the first part record is repaired, `staged.rs:212`): not reachable on today's tree.** It needs two part records that name the same chunk id. Chunk ids are minted fresh for each write (`crates/server/src/lib.rs:255-258`, epoch<<64 | seq). Nothing in the code or in proposal 0016 copies chunks between parts (there is no UploadPartCopy). So only corrupted data gets there. The committed reading applies the same first-reference rule (`reconstruction.rs:677`). I think the "blocking" label overstates it. The test at `staged_repair.rs:2060` covers only that corrupt state.

## Notes (no action)

- The four C5 survivors are equivalent mutants. They are the `&&`→`||` mutants at `staged.rs:766-769`, in `repointed_part`'s read-back check. The splice copies every byte except the chunk list, so length, digest, `committed_at` and `session_epoch` can never differ, and no input can tell these mutants from the real code. The TIMEOUT at `staged.rs:433` (`==`→`!=`) makes the loop infinite, which effectively counts as caught.
- I attempted each of the following and could not refute it:
  - Deleting any single adoption pin (pre-mark bytes, drain key, session, part record) fails a test.
  - Treating every session state as `Open` fails leg E.
  - Removing the `W_repoint` gate fails C(v).
  - Removing the `held` guard fails leg E.
  - Sending the writes one after another fails `a_moves_writes_are_sent_together`, because its double really does interleave.
  - A leftover fragment at a destination does not wedge the pass: `put_fragment` publishes through an overwriting rename.
  - A leftover in-place pre-mark on a position a record still names is harmless. Unlink, supersede and restore all overwrite marks without a precondition (`crates/core/src/metadata.rs:2116`, `:2224`, `:2297`; `restore.rs:494`).
  - `psum:` holds no placement, so repointing the part record leaves it consistent.
  - The move's event string is at most about 61 bytes, well under the 256-byte limit, so the `?` on `OrphanMark::structured` cannot fire.
  - Each destination write's await is bounded by the gRPC client's own request timeout (`crates/chunkstore-grpc/src/client.rs:264-271`).

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction/staged.rs:212`: Keeping only the first part referencing a chunk can permanently starve its other placements. The new two-part fixture (`crates/custodian/tests/staged_repair.rs:2060`) starts both parts at the same degraded placement, repairs only the first, and explicitly leaves the second unchanged. Once scrub requeues the second part's missing position, reconstruction selects the now-healthy first part again and drains the obligation through `Gathered::settle` (`crates/custodian/src/reconstruction.rs:902`). The second part never gets repaired. Retain and assess the remaining staged references before draining; extend this test through another scrub/reconstruction cycle and require both parts to reach intact placements.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction/staged.rs:704`: The adoption batch grows by five operations per relocated fragment, plus four fixed operations, without respecting the shared `MAX_BATCH_OPS = 500` budget (`crates/core/src/multipart.rs:4551`). A wide repair relocating 100 fragments already needs 504 operations; 255 needs 1,279. The pre-mark batch also grows without a budget. These unsplittable transactions can repeatedly exceed a slow backend's deadline, leaving an otherwise repairable chunk stuck while destination writes are retried. Bound each move using the existing transaction budget, preserve the obligation until all missing fragments are adopted, and add a wide-scheme regression that checks batch sizes and eventual completion.

No additional actionable reuse, simplification, or efficiency findings. Reviewed target source and frozen gate evidence; no builds or tests were rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Restore bounded progress for wide supported schemes — the production path creates over-budget transactions, so passing small-scheme tests does not establish the brief's progress invariant; `crates/custodian/src/reconstruction/staged.rs:621`, `crates/custodian/src/reconstruction/staged.rs:704`, `reviewer-probes.log:14`.
- [ ] T5 Judgment — Decide whether duplicate committed-part references are supported or must be rejected — repeated repair passes certify the first while the second remains degraded, but normal gateway allocation makes IDs unique; `crates/custodian/src/reconstruction/staged.rs:212`, `crates/custodian/tests/staged_repair.rs:2060`, `crates/server/src/lib.rs:245`, `reviewer-probes.log:6`.
- [ ] Validation — fitness-to-purpose — Judge durability fitness for the intended deployment and the need for Tier-1 disk-fault/Tier-2 kill-reconstruct observation — the exercised protocol races do not establish those deployment outcomes; `AGENTS.md:78`, `crates/custodian/src/reconstruction/staged.rs:60`.
- [ ] **No test covers the pre-mark's precondition on the destination's existing mark** (`crates/custodian/src/reconstruction/staged.rs:624-625`). If both arms are changed to plain `batch` (no precondition), all 26 + 36 tests still pass. This precondition is what keeps C(iii) ("a position with a `reclaiming` mark is never written") true when GC races the pass. The race: `position()` reads the destination's mark (`staged.rs:534`). GC then swaps that mark to `reclaiming` (`crates/custodian/src/gc.rs:596-604`) before the pre-mark commits. GC's `destroy` deletes the fragment and then deletes the mark key with no precondition (`gc.rs:813-825`). Without the pin, the pre-mark overwrites GC's `reclaiming` mark and the rebuilt fragment is written into a position GC is deleting. Depending on timing, one of two things happens. Either the adoption commits over bytes GC has just deleted, so the obligation drains and the fragment is gone. Or GC deletes the pre-mark first, the adoption loses, and a fragment written after GC's delete is left with no mark and no record naming it (stranded). The current code is correct. I checked with a scratch test: seed a structured mark at `mark_key(2, 2)` on `Fixture::standard`, and add an `after_read_of(mark_key(2, 2))` hook that commits GC's swap to `into_reclaiming()`. Then assert: no write arrived, the `reclaiming` mark is byte-identical, and the obligation is still queued. It passes on the patch. On the mutant it fails with `a position GC decided to reclaim was written: [(2, FragmentId{..index: 2}, Some(40000))]`. Please add it. The existing C(iii) case (`crates/custodian/tests/staged_repair.rs:1149`) seeds the `reclaiming` mark before the pass starts, so it never runs this race. The DST runs no GC pass at all.
- [ ] **The same gap, lower stakes:** the pre-mark's `require(part == prior)` (`staged.rs:620`) and its `require_absent(desired:dserver:<S_new>)` (`staged.rs:628`) can each be deleted with every test still green. The module doc says both make the pre-mark lose with nothing written (`staged.rs:33-35`, `:53-54`). Only the session-pin version of that claim has a test (`staged_repair.rs:1032`). The drain test records the drain after the write is stored (`staged_repair.rs:1767-1769`), so it only exercises the adoption's pin. Safety still holds without these two pins: the adoption CAS refuses, and the wasted write stays under its pre-mark. So this is about pinning the behavior the doc promises. Add two more `after_read_of(mark_key(2, 2))` cases, one that rewrites the part record and one that seeds `desired:dserver:2`. Each should assert that no write arrives.
- [x] **The brief says leg E is "GREEN on the base by design", but one leg-E case ran red on the base.** `a_degraded_chunk_with_no_usable_destination_is_kept` (`staged_repair.rs:1856`) failed on the base with `left: Blocked, right: Satisfied` (`gate-logs/C4-verify.log`). It asserts that the pass now answers `Satisfied` where `main` answered `Blocked`. The below-k-behind-an-outage case (`staged_repair.rs:2006`) makes the same change. Both match the committed path: `Blocked` and `Unreachable` feed gauges, not the pass's `hole` (`crates/custodian/src/reconstruction.rs:336`, `:342`, `:488-492`). But these are a second and third Blocked→Satisfied change, beyond the leg-G one the brief put in front of sign-off. For these chunks an operator now sees only the `reconstruction_repair_blocked` or `reconstruction_unreachable` gauge, never a `Blocked` pass. Sign-off should confirm this is intended.
- [ ] **T4 blocking #1 (batch operation limit): real, but only for very wide codes.** The adoption carries 4 operations plus up to 5 per moved fragment (`staged.rs:699-722`), and the pre-mark carries 2 plus 3 per fragment. The limit is `MAX_BATCH_OPS` = 500 (`crates/core/src/multipart.rs:4562`). So the adoption goes over only when one chunk has 100 or more fragments to move. That needs RS with m ≥ 100 and at least 100 free failure domains. Nothing in the config caps m, so it can happen in principle, but not with realistic profiles. The committed repoint has the same unbounded shape at a higher threshold (`reconstruction.rs:1187-1196`). If a guard is wanted, it is cheap: before writing anything, classify a move as `Blocked` when `4 + 5·n > MAX_BATCH_OPS`. This is a judgment call, not a correctness defect at realistic widths.
- [ ] **T4 blocking #2 (only the first part record is repaired, `staged.rs:212`): not reachable on today's tree.** It needs two part records that name the same chunk id. Chunk ids are minted fresh for each write (`crates/server/src/lib.rs:255-258`, epoch<<64 | seq). Nothing in the code or in proposal 0016 copies chunks between parts (there is no UploadPartCopy). So only corrupted data gets there. The committed reading applies the same first-reference rule (`reconstruction.rs:677`). I think the "blocking" label overstates it. The test at `staged_repair.rs:2060` covers only that corrupt state.
- [ ] `crates/custodian/src/reconstruction/staged.rs:212`: Keeping only the first part referencing a chunk can permanently starve its other placements. The new two-part fixture (`crates/custodian/tests/staged_repair.rs:2060`) starts both parts at the same degraded placement, repairs only the first, and explicitly leaves the second unchanged. Once scrub requeues the second part's missing position, reconstruction selects the now-healthy first part again and drains the obligation through `Gathered::settle` (`crates/custodian/src/reconstruction.rs:902`). The second part never gets repaired. Retain and assess the remaining staged references before draining; extend this test through another scrub/reconstruction cycle and require both parts to reach intact placements.
- [ ] `crates/custodian/src/reconstruction/staged.rs:704`: The adoption batch grows by five operations per relocated fragment, plus four fixed operations, without respecting the shared `MAX_BATCH_OPS = 500` budget (`crates/core/src/multipart.rs:4551`). A wide repair relocating 100 fragments already needs 504 operations; 255 needs 1,279. The pre-mark batch also grows without a budget. These unsplittable transactions can repeatedly exceed a slow backend's deadline, leaving an otherwise repairable chunk stuck while destination writes are retried. Bound each move using the existing transaction budget, preserve the obligation until all missing fragments are adopted, and add a wide-scheme regression that checks batch sizes and eventual completion.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- [ ] **The rebuild criterion can accept the wrong payload.** `brief.md:25-31` requires an intact fragment with the right scheme, but never compares its payload with the lost shard or reconstructs the original data using it. On the target, `crates/core/src/repair.rs:58-90` checks header identity and geometry; even `fragment_intact` (`:94-110`) checks only those plus the fragment's own checksum. A newly encoded wrong payload can satisfy both checks and all the stated placement/ledger assertions. Revise A and C(vii) to require byte-for-byte equality with the seeded missing shards, or reconstruction of the seeded data from a survivor set that must use the repaired shards.
- [ ] **One of the four adoption fences has no adverse test.** Scope requires a CAS pinned to the pre-mark's bytes (`brief.md:118-119`), but B changes the session/part, D changes desired state, C(iii) starts with an already-reclaiming position, and F races only the session fence. None changes the destination mark after a successful write and before adoption. Target GC commits `reclaiming` before deleting bytes (`crates/custodian/src/gc.rs:589-595`, `:773-776`); omitting the adoption's mark precondition can therefore publish a placement naming deleted bytes while satisfying the stated cases. The existing DST adoption test constructs its own fenced batch (`crates/dst/tests/custodian.rs:3448-3452`), so it does not test the new repair path. Add a production-`reconcile_step` case that pauses after writing, lets GC reclaim the destination, then resumes adoption: it must lose, preserve the part record, and retain the obligation.
- [ ] **The instructed leg-G rewrite removes coverage on a false premise.** `brief.md:124-126` says this slice makes the existing Open-session test's “kept, no write” behavior false and requires retargeting it to a non-Open session. Its actual fixture is a lost `EcScheme::None` fragment (`crates/custodian/tests/staged_protection.rs:2359-2364`), which has no surviving redundancy. The brief itself keeps that scheme unrepairable (`brief.md:142-143`; target `crates/custodian/src/reconstruction.rs:769-773`), so keeping the obligation and writing nothing remain correct. Preserve that Open/nonredundant guard; specify any intended change to its `Blocked` result separately, and cover non-Open repair refusal with a repairable RS fixture.
- [ ] size backstop — this slice is behaving oversized: patch is 220 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): issue_814 — rebuild to close the test gaps; the production code is not rejected (the adversary could not break it, and all correctness gates pass). Add the missing tests for the pre-mark batch's preconditions, each of which can be deleted today with every test still green: 1. The pin on the destination's existing mark (staged.rs:624-625) — the important one. GC swaps the destination's mark to `reclaiming` after `position()` reads it (staged.rs:534) and before the pre-mark commits. Use an `after_read_of(mark_key(2, 2))` hook on `Fixture::standard` with a structured mark seeded there, committing GC's `into_reclaiming()` swap. Assert: no write arrived, the `reclaiming` mark is byte-identical, the obligation is still queued. The existing C(iii) case seeds the mark before the pass starts, so it never runs this race. 2. The pre-mark's `require(part == prior)` (staged.rs:620): same hook point, rewrite the part record; assert no write arrives. 3. The pre-mark's `require_absent(desired:dserver:<S_new>)` (staged.rs:628): same hook point, seed `desired:dserver:2`; assert no write arrives. The existing drain test records the drain after the write, so it only covers the adoption's pin. If any of these goes red on the current code, fix the code, not the case. Confirmed at sign-off as intended, do not change: the two further Blocked -> Satisfied answers (no usable destination, staged_repair.rs:1856; below k behind an outage, staged_repair.rs:2006). They follow the brief's "resolves as the committed path resolves one"; the conditions report through the repair_blocked / unreachable gauges and the audit seam. Overridden at sign-off, not a blocker, no code change: the T4 blocking finding on the batch operation budget for very wide schemes (staged.rs:704, pre-mark staged.rs:621). It needs 100+ fragments moved in one chunk, which no realistic profile has, and the committed repoint has the same shape. Overridden at sign-off, not a blocker, no code change: the T4 blocking finding on first-reference-only repair of duplicate part references (staged.rs:212). Sign-off agrees with the adversary: chunk ids are minted fresh per write (crates/server/src/lib.rs:255-258) and nothing copies chunks between parts, so two part records naming one chunk means something went wrong earlier; it is not a state this repair has to converge. The committed reading applies the same first-reference rule (reconstruction.rs:677). Both T4 blocking findings are therefore settled; do not re-raise them. Do not grow the patch beyond the three tests.
- By / date: Eduard Ralph / 2026-09-20

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (sign-off, issue_814): check for chunk duplication — two part records naming the same chunk id means something went wrong earlier; today the repair silently fixes only the first reference (`crates/custodian/src/reconstruction/staged.rs:212`, committed reading `reconstruction.rs:677`). File an issue to detect and name a duplicate chunk reference instead of skipping it quietly.
