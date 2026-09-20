# Result — issue 663 / staged-scrub-and-repair

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged redundancy decays untended. **Scrub** walks only committed placements
  (`crates/custodian/src/scrub.rs:88`, `:131-199`), so a committed part's fragment can rot for
  the hours a session stays open and nothing notices. **Reconstruction** resolves a repair
  obligation only against committed inodes (`read_committed`, `reconstruction.rs:468`). A staged
  chunk finds no committed map, the obligation is assessed `Drain` (`reconstruction.rs:613`),
  and it is silently dropped (`:218`, committed at `:322`). So even an obligation someone
  queued for a staged chunk is discarded, and the part stays one fragment short until it is
  published — or forever, if the client never completes.
- Success criterion: the NEW file `crates/custodian/tests/staged_repair.rs` passes, plus
  one seeded case appended to the existing `crates/dst/tests/custodian.rs`. It runs over
  in-memory doubles, with records seeded as raw JSON the base decoders accept (the fixture
  shapes in `crates/core/tests/multipart_session_records.rs:81-145`). The D-server doubles
  **enforce** the write deadline `put_fragment` carries: a write arriving at or after its
  `deadline_millis` is refused with `wyrd_traits::WriteDeadlineExpired`, as the real D server
  does since #638. v1's doubles ignored it (`_deadline_millis`,
  `crates/custodian/tests/gc.rs:127`), so the deadline was never exercised. Legs:
  **(A) Scrub checks committed staged fragments and queues repair.** A committed `part:`
  record's fragment carrying a single bit flip (the `corrupt_fragment` idiom,
  `crates/custodian/tests/scrub.rs:154-160`) results, after a scrub pass, in a queued repair for
  that chunk (`wyrd_core::repair::queued_repairs`, `crates/core/src/repair.rs:151`). A
  **missing** committed-part fragment does too. An **in-flight** (`sidx:`-only) chunk with a
  missing fragment queues **nothing**: a still-streaming chunk is expected to be incomplete, and
  verification needs the committed scheme the part record carries (`0016:824`). On the base
  scrub never sees the part's fragments, so the first two go red.
  **(B) Reconstruction repairs a staged chunk — the whole protocol, not just the metadata.**
  With a committed part's fragment lost and its repair queued, run `reconcile_step` with a
  `ReconstructionContext`. All of the following must hold:
  - the new D server holds a fragment for that chunk that is **intact and scheme-correct**
    (its header matches the chunk's identity, `wyrd_core::repair::header_matches_identity`);
  - the `part:` record's `ChunkRef.placement` names **that** server;
  - the destination's pre-mark `orphan:<P_new>` is **gone**;
  - the vacated source `P_old` carries an `orphan:` mark GC will act on after grace;
  - the obligation has **drained**.

  A changed placement alone is not enough: it proves metadata moved, not that a byte was
  rebuilt. On the base the obligation is dropped and nothing is written — the red.
  **(C) The losing branch leaves nothing stranded (X29, `0016:888`).** The D-server double's
  `put_fragment` hook fences the session **after** the destination fragment is written and
  **before** the adoption CAS, by moving the `mpu:` record from `Open@E` to `Aborting@E+1`.
  Then: the re-place makes no adoption; the `part:` record is byte-identical; the pre-mark
  `orphan:<P_new>` **stands**, so GC will reclaim the written fragment; and the obligation stays
  queued. Repeat the same assertions with the `part:` record rewritten instead of the session
  fenced (`require(part == prior)` loses). On the base the obligation is dropped, so the
  "still queued" assertion goes red.
  **(D) The pre-mark and write-deadline rules (`0016:1300-1354`), each with its own case — v1's
  review found every one of these untested:**
  (i) the pre-mark is durable **before** the destination write: the D-server double asserts
  `orphan:<P_new>` is present when `put_fragment` arrives;
  (ii) a destination position already carrying a mark from another event, or a legacy mark, is
  **re-stamped** fresh, never reused as the pre-mark with its old stamp. A reused old stamp
  lets GC's sweep of fragment-less marks take it before the write lands;
  (iii) a destination carrying a `reclaiming` mark is never used;
  (iv) the destination write carries an authorization deadline, and when the double refuses it
  as expired, the re-place aborts: no adoption, pre-mark standing, obligation queued;
  (v) the worker does **not** authorize the write if its own pre-mark is older than `W_repoint`
  (`0016:1338-1346`). A hook advances the clock past it between pre-mark and write; the worker
  then restarts from a fresh pre-mark or aborts, and no write is authorized on the stale one;
  (vi) a source `P_old` whose existing `orphan:` value decodes as none of the three shapes makes
  the move **abort before its CAS**. It never overwrites metadata it cannot parse (ADR-0045),
  the obligation stays queued, and the fault is surfaced. v1 logged it and committed anyway
  (`results/issue_637/iteration-v1/review-batch.md`).
  **(E) The destination is fenced against a drain (`0016:885`, "the same fence applies to the
  destination of a staged re-place").** A draining server (`set_lifecycle`) is never chosen as
  the destination. A drain recorded between destination selection and the adoption CAS makes
  that CAS lose: the batch carries `require_absent(desired:dserver:<S_new>)`, `S_new` is not
  adopted, and the pre-mark stands.
  **(F) Seeded DST for X29**, appended to the **existing** `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53` — keep that attribute). It sweeps the fence across every point of
  the re-place: before the pre-mark, between pre-mark and write, between write and CAS, and after
  the CAS. In every interleaving, no fragment ends unreferenced **and** unevidenced, and the
  session never ends `Aborting` with a `part:` record naming a fragment that was not written.
  It runs under `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1616`, `--cfg madsim`).
  Record the seed count in `build-notes.md`.
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: scrub over committed staged fragments (verify, and queue repair for corrupt or
  missing ones), and reconstruction's resolution and repair of an obligation for a staged chunk.
  The repair re-places the fragment under 0016's rules: pre-mark before write, write deadline
  and `W_repoint`, adoption CAS pinned to the session state and the prior part record, the
  drain fence on the destination. It leaves the obligation queued on any loss. The committed
  repair path (`repair_chunk`, `reconstruction.rs:829-955`) keeps its current behaviour.
  Must NOT change `reconcile_step`'s signature, and must NOT add a field to any context struct.
  / out of scope: `seg:`-resident repair (#777); rebalance, drain status and restore
  (child-4 — do not touch `desired_state.rs`, `rebalance.rs`, `restore.rs`,
  `crates/core/src/multipart.rs` beyond what is already on the base, `crates/server/src/cli.rs`
  or `docs/`); the ledger walk and mark codec (child-1 and child-2 — consume them, do not
  reshape them); the upload-side drain fence (#657, X59); any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (31 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 96.9% — 462 of 477 instrumentable changed lines executed (floor 80%); 477 of 1154 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 79 mutants tested in 2m: 4 missed, 34 caught, 41 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The patch adds staged multipart scrub and fenced reconstruction, but it needs a rebuild to prevent pass-age deadline starvation and to make the new DST model enforce that deadline.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted design requires committed-part scrub plus fenced staged reconstruction, and the brief makes the no-stranding, deadline, drain-fence, and DST outcomes independently decidable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:824`). |
| C2 Reproduction (red pre-fix) | PASS | With only production changes stashed, the focused suite independently failed 29/31 cases, including the end-to-end repair assertion; restoring the patch made all 31 pass (`crates/custodian/tests/staged_repair.rs:823`). |
| C3 Change | PASS | The change stays on the stated data-plane surface: scrub reads committed parts and reconstruction resolves staged records without changing the public control-point signature or context shape (`crates/custodian/src/scrub.rs:100`, `crates/custodian/src/reconstruction.rs:203`). |
| C4 Verification (red→green) | PASS | Independent focused red→green and the 50-seed staged DST rerun passed; frozen CI completed all checks and diff coverage was 96.9%, while this sandbox's only full-CI stop was the read-only Cargo advisory lock (`gate-logs/C4-ci.log:3627`, `gate-logs/C4-diff-cov.log:235`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must timestamp the pre-mark/deadline at the repair attempt rather than pass start—after more than 20 seconds of scans and assessment, every staged write is already expired and repeated passes may never repair (`crates/custodian/src/reconstruction/staged.rs:423`). |
| T1 Structure | PASS | The staged state machine is isolated in a private reconstruction module and all three consumers share one bounded, source-before-destination staged walk (`crates/custodian/src/reconstruction.rs:74`, `crates/custodian/src/gc.rs:1106`). |
| T2 Shape | PASS | Required API shape is preserved: `reconcile_step` is unchanged and `ReconstructionContext` has no new field (`crates/custodian/src/reconciliation.rs:125`, `crates/custodian/src/reconstruction.rs:86`). |
| T3 Runtime | FAIL | A long but valid namespace/queue pass can permanently starve staged repair because the D server receives `pass_start + 20s` instead of a deadline derived when the durable pre-mark is created (`crates/custodian/src/reconstruction/staged.rs:398`, `crates/custodian/src/reconstruction/staged.rs:423`). |
| T4 Contribution | FAIL | The deep-review gate found the two unique blockers above; TiKV feature compilation passed, while the contribution-artifact subcheck is N/A because `pr-description.md` is intentionally audited at publish (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must make `ReplaceDServer` enforce the supplied deadline on simulated time—it delegates to `MemDServer`, whose deadline parameter is ignored, so the green DST campaign cannot judge production refusal semantics (`crates/dst/tests/custodian.rs:3611`, `crates/dst/tests/custodian.rs:287`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the corrected implementation provides production-worthy staged redundancy and no-stranding guarantees under long passes and deadline expiry, because those durability outcomes govern whether unpublished client data remains recoverable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:825`). |

### Advisory — adversary

# Adversarial review — #663 staged scrub and repair

**The red→green evidence holds.** I re-ran it in a scratch copy. With the production files
reverted, 29 of the 31 new tests fail on an assertion. The other two
(`a_scrub_verifies_a_chunk_named_by_both_classes_once`,
`a_staged_chunk_already_whole_drains_its_obligation`) are guards and pass on the base, as they
should. With the fix applied, `staged_repair` passes 31/31 and `staged_protection` passes 26/26.
The tests drive the production `reconcile_step`, not a copy of it.

**The fix still breaks.** I added two probe tests to the scratch copy, and both break it. The
findings below are ordered by how concrete they are.

- NEEDS-HUMAN [impl] — **The drain filter and the drain fence test different facts, so a staged repair can loop forever.** `crates/custodian/src/reconstruction.rs:212` builds the destination exclusion set from `draining_servers`, and that function silently skips any `desired:dserver:<S>` whose value is not `draining`/`decommissioning` (`crates/custodian/src/desired_state.rs:156-163`). The adoption, though, requires the key to be **absent** whatever its value (`crates/custodian/src/reconstruction/staged.rs:461`). Concrete failing case, run as a probe: seed `desired:dserver:3 = "maintenance"`, lose fragment 1, enqueue it, and run 4 passes. Every pass picks server 3, pre-marks it, writes to it (4 writes landed) and loses the adoption. Placement stays `[0, 1, 2]`, the obligation stays queued, and every pass reports `Satisfied`. Nothing is stranded, because the pre-mark stands. But the staged chunk is never repaired, and its fragment is rewritten on every pass. Fix in scope: `choose_destinations` (`staged.rs:492-547`) should rule out any server whose `desired_key` is present at all, so selection and the CAS test the same thing. Add a test for it.

- NEEDS-HUMAN [human] — **Slow passes starve staged repairs, because the pre-mark stamp and write deadline use the pass-start clock.** This confirms T4's blocking finding with a real run. The stamp and the deadline both come from `now_millis`, which is read once when the pass starts (`crates/server/src/custodian.rs:533`), not when the pre-mark commits (`staged.rs:402-423`). Before any staged write is sent, the pass assesses every queued chunk, which means fetching its fragments (`reconstruction.rs:258`). It also runs the sorted plan list, where committed repairs can come first (`:320`, `:378`). Probe: set the D-server clock 25 s past the pass's `now_millis`. Result: three passes, three refusals, placement unchanged, and every pass reports `Satisfied`. For scale: about 2,000 obligations at 9 fetches each already take more than 20 s at ~1 ms per fetch. No staged repair can land until the backlog drains. If enough obligations stay queued for good (Unrepairable or Blocked), no staged repair ever lands. This is safe, since nothing is stranded, but it breaks the brief's invariant that staged redundancy is "maintained the way a committed chunk's is". A real fix needs a clock read at pre-mark time, and the brief forbids the only ways to get one: a new context field or a signature change. So this is a scope decision.

- NEEDS-HUMAN [human] — **D(v) is not built as 0016 and the brief state it, and its test proves D-server enforcement, not a worker-side gate.** 0016:1339-1348 and the `W_repoint` row at 0016:1479 require the worker not to issue the destination write when its pre-mark is older than `W_repoint`: "enforced fail-closed before the worker authorizes the destination write". `staged.rs:415-435` reads no clock between the pre-mark commit and `put_fragment`. It relies only on the deadline fixed at the stamp. `d_v_no_write_is_authorized_on_a_stale_pre_mark` asserts that the write **did** reach the D server on a day-old pre-mark, and that only the double's deadline check refused it (`crates/custodian/tests/staged_repair.rs:1148-1150`). That is the same mechanism D(iv) tests (`:1091-1104`). Now take a pre-#638 D server that ignores the field (`crates/chunkstore-grpc/src/client.rs:274-277`: "a mixed-version fleet does not get the guarantee"). There, a stalled worker's write lands on a pre-mark GC may already have reclaimed, which is 0016's outcome (a), X88. Today this does not bite with one custodian, because the deployed loop runs GC after reconstruction in the same task. Someone needs to decide whether "deadline fixed at stamp time" is an accepted substitute for the normative worker-side gate. If not, the fix needs a clock seam.

- NEEDS-HUMAN [human] — **The 20 s re-place write window is a private constant that the GC side cannot see.** `W_WRITE_MILLIS` (`staged.rs:99`) is sized only in a comment, against the 60 s grace at `crates/server/src/custodian.rs:114`. Nothing checks `G_orphan > W_WRITE + δ_clock`. #800's fragment-less sweep (deferred at `crates/custodian/src/gc.rs:621-626`) must keep a fragment-less re-place pre-mark until at least `stamp + 20 s + δ_clock` (0016:1381-1387). Suppose #800 sizes that deadline from the upload path's `W_repoint + W_write` and the total comes to under 20 s. Then the sweep deletes a pre-mark while its write is still allowed. The write lands with no mark, the adoption loses on `require(orphan:<P_new> == mark)`, and the fragment is stranded. The new tests already pair a 50 ms GC grace (`staged_repair.rs:82`) with this 20 s write window, which is the pairing 0016 forbids. They pass only because the write had already landed by the time GC ran. This needs a shared constant or a note handed to #800, which is in the same wave.

- The C4-verify row's "31 test(s) ran red" in `check-gates.json` overstates the red leg. The red log (`gate-logs/C4-verify.log`) shows 29 failed and 2 passed; the two that pass are the guards named above. The evidence is still sound.

- C5 survivors at `staged.rs:167` (`owes_repairs -> true`) and `staged.rs:217` (`&&` → `||`) show that no test pins two efficiency claims: "a part is held only if a repair is owed inside it", and "drain records are read only when a staged repair is owed". I found no effect on correctness.

**Refutation attempts that failed:**

- **Session fence or part rewrite at every step.** I tried landing one before the pre-mark, between pre-mark and write, and between write and adoption. Both batches pin the exact session bytes and part bytes (`staged.rs:404-406`, `:451-455`). The C tests and the DST sweep reach all four windows (`staged_replace_reaches_every_fence_window ... ok` in `gate-logs/C4-ci.log`).
- **In-place rebuild, and two missing indexes whose servers trade places.** Pre-mark keys and source-mark keys differ by fragment id. The in-place skip is checked per index (`staged.rs:463-468`), and no key is written twice in one batch.
- **The byte splice of the part record** (`staged.rs:559-585`). `decode_part_record` accepts only canonical bytes (`crates/core/src/multipart.rs:2584`), so the chunk list appears exactly once. The result is decoded again before use.
- **Reading staged before committed, with a publication landing mid-pass.** I also tried draining over an incomplete staged reading (`reconstruction.rs:200`, `:404`). I found no case that drops an obligation while a record still names the chunk.
- **Scrub false positives from a concurrent re-place or Abort.** In the deployed loop, scrub, reconstruction and GC run one after another, and GC protects every position a part record names. So a missing part fragment really is a loss.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must timestamp the pre-mark/deadline at the repair attempt rather than pass start—after more than 20 seconds of scans and assessment, every staged write is already expired and repeated passes may never repair (`crates/custodian/src/reconstruction/staged.rs:423`).
- [ ] T5 Judgment — Rebuild must make `ReplaceDServer` enforce the supplied deadline on simulated time—it delegates to `MemDServer`, whose deadline parameter is ignored, so the green DST campaign cannot judge production refusal semantics (`crates/dst/tests/custodian.rs:3611`, `crates/dst/tests/custodian.rs:287`).
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether the corrected implementation provides production-worthy staged redundancy and no-stranding guarantees under long passes and deadline expiry, because those durability outcomes govern whether unpublished client data remains recoverable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:825`).
- [ ] **The drain filter and the drain fence test different facts, so a staged repair can loop forever.** `crates/custodian/src/reconstruction.rs:212` builds the destination exclusion set from `draining_servers`, and that function silently skips any `desired:dserver:<S>` whose value is not `draining`/`decommissioning` (`crates/custodian/src/desired_state.rs:156-163`). The adoption, though, requires the key to be **absent** whatever its value (`crates/custodian/src/reconstruction/staged.rs:461`). Concrete failing case, run as a probe: seed `desired:dserver:3 = "maintenance"`, lose fragment 1, enqueue it, and run 4 passes. Every pass picks server 3, pre-marks it, writes to it (4 writes landed) and loses the adoption. Placement stays `[0, 1, 2]`, the obligation stays queued, and every pass reports `Satisfied`. Nothing is stranded, because the pre-mark stands. But the staged chunk is never repaired, and its fragment is rewritten on every pass. Fix in scope: `choose_destinations` (`staged.rs:492-547`) should rule out any server whose `desired_key` is present at all, so selection and the CAS test the same thing. Add a test for it.
- [ ] **Slow passes starve staged repairs, because the pre-mark stamp and write deadline use the pass-start clock.** This confirms T4's blocking finding with a real run. The stamp and the deadline both come from `now_millis`, which is read once when the pass starts (`crates/server/src/custodian.rs:533`), not when the pre-mark commits (`staged.rs:402-423`). Before any staged write is sent, the pass assesses every queued chunk, which means fetching its fragments (`reconstruction.rs:258`). It also runs the sorted plan list, where committed repairs can come first (`:320`, `:378`). Probe: set the D-server clock 25 s past the pass's `now_millis`. Result: three passes, three refusals, placement unchanged, and every pass reports `Satisfied`. For scale: about 2,000 obligations at 9 fetches each already take more than 20 s at ~1 ms per fetch. No staged repair can land until the backlog drains. If enough obligations stay queued for good (Unrepairable or Blocked), no staged repair ever lands. This is safe, since nothing is stranded, but it breaks the brief's invariant that staged redundancy is "maintained the way a committed chunk's is". A real fix needs a clock read at pre-mark time, and the brief forbids the only ways to get one: a new context field or a signature change. So this is a scope decision.
- [x] **D(v) is not built as 0016 and the brief state it, and its test proves D-server enforcement, not a worker-side gate.** Cleared by human: not a released product yet, so no mixed-fleet backward-compatibility concern; the D-server-enforced deadline is an accepted substitute for now. 0016:1339-1348 and the `W_repoint` row at 0016:1479 require the worker not to issue the destination write when its pre-mark is older than `W_repoint`: "enforced fail-closed before the worker authorizes the destination write". `staged.rs:415-435` reads no clock between the pre-mark commit and `put_fragment`. It relies only on the deadline fixed at the stamp. `d_v_no_write_is_authorized_on_a_stale_pre_mark` asserts that the write **did** reach the D server on a day-old pre-mark, and that only the double's deadline check refused it (`crates/custodian/tests/staged_repair.rs:1148-1150`). That is the same mechanism D(iv) tests (`:1091-1104`). Now take a pre-#638 D server that ignores the field (`crates/chunkstore-grpc/src/client.rs:274-277`: "a mixed-version fleet does not get the guarantee"). There, a stalled worker's write lands on a pre-mark GC may already have reclaimed, which is 0016's outcome (a), X88. Today this does not bite with one custodian, because the deployed loop runs GC after reconstruction in the same task. Someone needs to decide whether "deadline fixed at stamp time" is an accepted substitute for the normative worker-side gate. If not, the fix needs a clock seam.
- [ ] **The 20 s re-place write window is a private constant that the GC side cannot see.** `W_WRITE_MILLIS` (`staged.rs:99`) is sized only in a comment, against the 60 s grace at `crates/server/src/custodian.rs:114`. Nothing checks `G_orphan > W_WRITE + δ_clock`. #800's fragment-less sweep (deferred at `crates/custodian/src/gc.rs:621-626`) must keep a fragment-less re-place pre-mark until at least `stamp + 20 s + δ_clock` (0016:1381-1387). Suppose #800 sizes that deadline from the upload path's `W_repoint + W_write` and the total comes to under 20 s. Then the sweep deletes a pre-mark while its write is still allowed. The write lands with no mark, the adoption loses on `require(orphan:<P_new> == mark)`, and the fragment is stranded. The new tests already pair a 50 ms GC grace (`staged_repair.rs:82`) with this 20 s write window, which is the pairing 0016 forbids. They pass only because the write had already landed by the time GC ran. This needs a shared constant or a note handed to #800, which is in the same wave.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 173 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild required. Human confirmed the following advisory-review findings as real blockers and overrode the size-backstop's iterate-plan recommendation (173 KB vs. 100 KB threshold) in favor of a direct rebuild. Required fixes: 1. Drain filter/drain fence mismatch (`crates/custodian/src/reconstruction.rs:212` vs. `crates/custodian/src/reconstruction/staged.rs:461`): `choose_destinations` (`staged.rs:492-547`) must exclude any server whose `desired:dserver:<S>` key is present at all, not just those with `draining`/`decommissioning` values, so destination selection and the adoption CAS test the same fact. Concrete repro in the adversary review (seed `desired:dserver:3 = "maintenance"`, 4 passes never repair, all report Satisfied). Add a regression test. 2. Pass-start deadline stall (`crates/custodian/src/reconstruction/staged.rs:402-423`, `crates/server/src/custodian.rs:533`): read the clock at pre-mark commit time, not once at pass start, so a slow pass (scans + assessment over ~20s) does not expire every staged write before it is attempted. 3. D-server test double must enforce the write deadline it claims to test (`crates/dst/tests/custodian.rs:3611`, `:287`): `ReplaceDServer` delegates to `MemDServer`, which ignores `deadline_millis`. Make the double actually refuse expired writes so the DST campaign exercises production refusal semantics, not a no-op. 4. New requirement (first time this pattern is raised — human wants it fixed now, not deferred to #800): promote `W_WRITE_MILLIS` (`staged.rs:99`) from a comment-only constant to a CLI-configurable constant in `crates/server/src/cli.rs`, following the existing convention there (e.g. `LEASE_TTL_MILLIS`). Document in its doc comment the inequality it must satisfy against #800's `D = W_repoint + W_write + δ_clock` grace window, so GC's fragment-less-mark sweep (#800) cannot be sized shorter than this window without a visible contradiction. Not carried forward as blockers (resolved by discussion): - D(v) worker-side staleness gate built as D-server enforcement rather than a worker-side clock check: accepted as-is. Human's rationale: no release has shipped yet, so there is no mixed-version-fleet backward-compatibility concern that would require the stricter worker-side gate. - T4's 6 blocking rubric findings are duplicate phrasings of items 2 and 3 above; no separate fix needed. Validation/fitness-to-purpose (advisory review's overall judgment item) was conditioned on items 1-3; treated as resolved once those are fixed in the rebuild.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
