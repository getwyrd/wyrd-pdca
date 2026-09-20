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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (38 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.0% — 492 of 502 instrumentable changed lines executed (floor 80%); 502 of 1324 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 79 mutants tested in 2m: 34 caught, 45 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.04s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: extend custodian scrub and reconstruction to detect and safely re-place degraded fragments of committed multipart parts without stranding bytes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief is falsifiable and complete across detection, fenced re-placement, deadline/evidence ordering, seeded DST, scope, and an explicit no-external-dependency claim. |
| C2 Reproduction (red pre-fix) | PASS | With production changes stashed and the new test retained, 36 of 38 focused tests failed, including committed-part scrub and whole-protocol repair cases at `crates/custodian/tests/staged_repair.rs:848` and `crates/custodian/tests/staged_repair.rs:970`. |
| C3 Change | PASS | The patch stays on the staged scrub/reconstruction surface plus the explicitly carried-forward deployment timing constant, whose safety inequality is stated at `crates/server/src/cli.rs:101`. |
| C4 Verification (red→green) | PASS | The focused suite reproduced 36/38 red and 38/38 green; local full CI passed lint, docs, fmt, clippy, build, and workspace tests before a host-only read-only advisory-lock failure, while the frozen full-CI and TiKV logs show green. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must cover valid cross-fragment destination permutations and settle indeterminate writes before claiming the no-stranding invariant, because current exits at `crates/custodian/src/reconstruction/staged.rs:392` and `crates/custodian/src/reconstruction/staged.rs:536` miss those cases. |
| T1 Structure | FAIL | The new correctness clock combines a caller-supplied epoch with `Instant::elapsed` at `crates/custodian/src/reconciliation.rs:145`, violating the one-source lifecycle rule at `AGENTS.md:132` and allowing wall-clock adjustments to skew pre-mark timestamps from GC/D-server time. |
| T2 Shape | PASS | Staged repair is isolated behind a dedicated module and all three consumers share one bounded staged-record walk at `crates/custodian/src/gc.rs:1106`, preserving the existing trait/dependency direction. |
| T3 Runtime | FAIL | A fragment-specific unusable mark excludes its whole server and can falsely block a swappable multi-loss repair at `crates/custodian/src/reconstruction/staged.rs:388`; an `Unknown` landing returns without the required re-read at `crates/custodian/src/reconstruction/staged.rs:526`, so GC can remove evidence before a late publication. |
| T4 Contribution | FAIL | The frozen multi-pass review's five reports reduce to two grounded blockers above; the contribution-artifact subgate is N/A because it explicitly defers its substantive audit to publish. |
| T5 Judgment | NEEDS-HUMAN | Confirm affected-path prior art across closed and rejected work before sign-off — this self-contained target exposes only one synthetic base commit, so the brief's merged/open and #637 account cannot be mechanically completed here. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide fitness only after the safety/liveness defects are rebuilt, and decide whether to trigger the Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns because this changes concurrent reconstruction and evidence reclamation (`AGENTS.md:78`, `AGENTS.md:81`). |

### Advisory — adversary

# Adversarial review — issue 663 (staged scrub and repair)

Advisory only. I rebuilt the patched tree in a scratch copy, re-ran `staged_repair` (38/38 green),
and ran two probe tests of my own. The red leg in `gate-logs/C4-verify.log` fails legs A, B, C and
D(iv) by assertion on the base, and the tests drive the production `reconcile_step`, not a copy.
One liveness defect is confirmed by a probe. The rest are judgment calls or notes.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction/staged.rs:388-394` (`choose_destinations`):
  if one *(server, fragment index)* position is unusable, the whole server is excluded. It is not
  offered to the chunk's other missing fragments. **Confirmed by probe:** RS(2,2) on servers 0..3
  (domains A..D), servers 2 and 3 unreachable, free reachable domains E (S4) and F (S5), and one
  stale `reclaiming` mark at `orphan:<S4, frag 2>`. That is the state GC leaves when it dies between
  deleting a fragment and deleting its key; `gc.rs:622-626` defers cleaning it up to #800, so it
  stays. Result: 3 passes in a row, 0 writes, obligation still queued, every pass reports
  `Satisfied`. The swap (S4 → fragment 3, S5 → fragment 2) was valid the whole time. The chunk
  sits at survivors == k, the most urgent repair there is. An unreadable mark at that position
  (`:435-437`) blocks it the same way until a human steps in. Fix: exclude the position, not the
  server. For example, keep a per-index exclusion set, or try a rejected server against the other
  missing indices before dropping it. Add this case as a test. This confirms T4's three duplicate
  findings at `:392`.

- NEEDS-HUMAN [human] — `crates/server/src/cli.rs:126-133` vs `crates/custodian/src/reconstruction.rs:95`:
  carry-forward item 4 ("a CLI-configurable constant in `cli.rs`, following `LEASE_TTL_MILLIS`")
  is met in name only. At runtime the library's constant is used. The `cli.rs` value is a copy held
  equal by a compile-time assert, so changing it breaks the build instead of changing behaviour.
  `cli.rs` owns `LEASE_TTL_MILLIS` and the server passes it down (`custodian.rs:114`). Here the
  ownership is reversed. The builder had no choice: the brief forbids a new context field or a
  `reconcile_step` signature change. You decide: accept the copy, or relax the brief so the
  composition root can pass the window in. The inequality doc against #800's `D` is present
  (`cli.rs:108-121`).

- NEEDS-HUMAN [human] — `crates/custodian/src/reconciliation.rs:126-147` (`StepClock`): the fix for
  carry-forward item 2 adds an `Instant` read to the pre-mark and deadline lifecycle: the caller's
  `now_millis` plus the real time elapsed. **I found no production failure.** In the deployed loop
  both parts are real time (`server/src/custodian.rs:529,543`). I tried wall-clock steps both
  forward and back mid-pass: the pre-mark's grace and the write deadline move together, so the
  inequality holds. Under madsim, `Instant` is simulated time. But every non-madsim test passes a
  manual clock, so stamps become "manual + real elapsed". They are not test-controlled: the tests
  need a 60 s `PASS_SLACK` tolerance (`tests/staged_repair.rs:100`), and no test can script a write
  landing exactly at `stamp + W_write`. The rubric's first MUST says test-controlled time goes
  through the testkit `Clock` seam (ADR-0024), which the brief's no-new-field rule blocks. You
  decide whether this compromise stands.

- NEEDS-HUMAN [human] — the gating T4 failure (`gate-logs/T4-batch-review.log`), for the two findings
  besides the one above. I recommend rejecting both with these recorded reasons:
  (1) `staged.rs:536` (a late `Unknown` landing is stranded). This needs two things at once: a stale
  fragment already at P_new, so GC's list-driven walk visits the position and reclaims the
  pre-mark; and a publication hung more than `G_orphan − W_write` (40 s) past its deadline. The
  late bytes then land unmarked, and GC never collects unmarked bytes (`gc.rs:552`). The same gap
  exists for every writer that carries a deadline (`traits/src/lib.rs:886-891`, "position
  coverage"). It belongs to 0016's deadline model, not to this diff.
  (2) `staged.rs:314` (`EcScheme::None` → `Unrepairable` before any fetch). This is the same code
  as the committed path at `reconstruction.rs:750`, so it keeps existing behaviour and is out of
  this slice's scope.

- Claim in `check-gates.json` (C4-verify): "38 test(s) ran red" is overstated. The log shows
  `2 passed; 36 failed` on the base. The two passes are
  `a_scrub_verifies_a_chunk_named_by_both_classes_once` and
  `a_staged_chunk_already_whole_drains_its_obligation`. Both are regression guards that *should*
  pass on the base, so the red→green proof still holds. This is the harness's count, not the
  builder's.

## Attempted and could not refute

- **X29 losing branches** (`staged.rs:492-503`, `:547-575`): the pre-mark and the adoption each pin
  the session bytes, the part bytes, and the destination's drain key. The adoption also pins the
  pre-mark bytes. Every fence position I traced loses a CAS with the written bytes still covered.
  Two custodians re-placing the same chunk (split-brain) also works out: the second one loses on
  either the pre-mark pin or the part pin.
- **In-place rebuild** (`:559-564`): a lost adoption leaves a pre-mark on a position the part still
  names. That is harmless: GC skips protected fragments before it looks at marks
  (`gc.rs:476-483`), and unlink overwrites marks with a plain put (`core/src/metadata.rs:2116-2119`).
  The fs store's rename overwrites a corrupt fragment in place.
- **`SourceMark::Reclaiming` adds no precondition** (`:573`): this is safe. GC cannot clear a
  `reclaiming` mark over a fragment the part still protects, and a fragment-less one strands
  nothing.
- **Two owed chunks in one part (probe):** pass 1 repairs one. The other loses at the pre-mark with
  nothing written. Pass 2 repairs it, and nothing is left unmarked. That is one chunk per record
  per pass, the same pace as the committed path (`reconstruction.rs:207-211`).
- **`repointed_part` byte splice** (`:593-619`): the canonical spelling puts the chunk list first,
  and the result is decoded again before use. I could not make it rewrite any other field.
- **Deadline enforcement:** both test doubles now refuse expired writes (NotApplied) and report
  unverified ones (Unknown). CI ran `staged_replace_never_strands` and
  `staged_replace_reaches_every_window` under `--cfg madsim`, both green (`gate-logs/C4-ci.log:3577,3585`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must cover valid cross-fragment destination permutations and settle indeterminate writes before claiming the no-stranding invariant, because current exits at `crates/custodian/src/reconstruction/staged.rs:392` and `crates/custodian/src/reconstruction/staged.rs:536` miss those cases.
- [x] T5 Judgment — Confirm affected-path prior art across closed and rejected work before sign-off — this self-contained target exposes only one synthetic base commit, so the brief's merged/open and #637 account cannot be mechanically completed here. Human: fine, checked across iterations already.
- [x] Validation — fitness-to-purpose — Decide fitness only after the safety/liveness defects are rebuilt, and decide whether to trigger the Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns because this changes concurrent reconstruction and evidence reclamation (`AGENTS.md:78`, `AGENTS.md:81`). Human: fine, campaigns not required now.
- [ ] `crates/custodian/src/reconstruction/staged.rs:388-394` (`choose_destinations`): confirmed liveness bug, required for the rebuild — stays open until fixed.
- [x] `crates/server/src/cli.rs:126-133` vs `crates/custodian/src/reconstruction.rs:95`: Human decision — override the brief's no-new-field constraint for this constant so it can be genuinely threaded down like `LEASE_TTL_MILLIS`, not just compile-time-asserted equal.
- [x] `crates/custodian/src/reconciliation.rs:126-147` (`StepClock`): Human decision — must follow ADR-0024 (route through the testkit `Clock` seam), which will also need the brief's no-new-field constraint waived.
- [x] the gating T4 failure (`gate-logs/T4-batch-review.log`), for the two findings besides `choose_destinations`: Human agreed — reject both. `staged.rs:536` (late `Unknown` write) tracked separately as an Act candidate (system-wide write-deadline gap, out of this slice's scope). `staged.rs:314` (`EcScheme::None`) confirmed correct by design — zero-redundancy storage has nothing to reconstruct from, mirrors existing committed-path behaviour.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_663/review-b — stays open pending rebuild.
- [x] size backstop — this slice is behaving oversized: patch is 210 KB (threshold 100 KB). Human explicitly overrode the recommendation and chose `iterate-do` over `iterate-plan`, despite this being the second consecutive iteration to trip the backstop (173 KB → 210 KB).

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
- Iteration delta (if iterating): Human confirmed the size backstop's iterate-plan recommendation (210 KB vs. 100 KB threshold, second consecutive iteration to trip it) and overrode it in favor of a direct rebuild. Required fix: 1. `choose_destinations` (`crates/custodian/src/reconstruction/staged.rs:388-394`): confirmed liveness bug. Rejecting one unusable (server, fragment-index) position excludes the whole server, so a repair with multiple missing fragments can stall forever even when a valid swap exists (adversary-confirmed with an RS(2,2) probe: two missing fragments, two free domains, one stale `reclaiming` mark — 3 passes, 0 writes, obligation never drains, every pass reports Satisfied, even though the swap was valid the whole time). Fix: exclude the position, not the server — e.g. a per-index exclusion set, or try a rejected server against the chunk's other missing indices before dropping it entirely. Add a regression test for this case. Brief waivers for this rebuild (both require lifting the "no new context-struct field / no `reconcile_step` signature change" constraint, which blocked a clean fix last iteration): 2. `crates/server/src/cli.rs:126-133` vs `crates/custodian/src/reconstruction.rs:95`: the "CLI-configurable" constant is currently only a compile-time-asserted copy (changing it breaks the build, doesn't change runtime behaviour) — ownership is reversed vs. the `LEASE_TTL_MILLIS` convention it's meant to follow (there, `cli.rs` owns the value and the server passes it down). Human: override the brief so this can be threaded down for real, matching the `LEASE_TTL_MILLIS` pattern. 3. `crates/custodian/src/reconciliation.rs:126-147` (`StepClock`): currently mixes a caller-supplied timestamp with real `Instant::elapsed`, so tests cannot fully control time (forced a 60s slack tolerance, no test can pin a write landing exactly at a deadline) and it doesn't go through the project's single testkit `Clock` seam (ADR-0024), which T1 Structure flagged as a one-time-source-lifecycle violation (AGENTS.md:132). No production failure was found under probing — this is a test-control and structure concern, not a runtime bug. Human: must follow ADR-0024 (route through the testkit Clock seam) even though it requires the same brief waiver as item 2. Not carried forward as blockers (confirmed non-bugs / out of scope, do not re-attempt): - `staged.rs:536` (late `WriteEffect::Unknown` write can land after GC reclaims the destination's pre-mark, stranding an unmarked fragment): confirmed real by the adversary, but it is a system-wide gap in the write-deadline model shared by every deadline-carrying writer (`crates/traits/src/lib.rs:886-891`), not introduced by or in scope for this slice. Filed as an Act candidate (§10) to become its own issue — do not fix inside this bundle. - `staged.rs:314` (`EcScheme::None` → `Unrepairable` without a fetch attempt): confirmed correct by design. `EcScheme::None` (`--durability none` / `replication(1)`, k=1 m=0) is zero-redundancy storage — the one fragment written *is* the data, so there is no second source to reconstruct from if it's lost. Mirrors the existing committed-path behaviour at `reconstruction.rs:641` verbatim; the brief requires the committed path's behaviour be kept unchanged. No fix needed. T5 Judgment and Validation/fitness-to-purpose: human confirmed both fine as-is (prior art checked across iterations already; no Tier-1/Tier-2 fault campaigns required for this round) — not required to be re-cleared for this iterate-do, since disposition is not accept.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- File as a new issue: a late `WriteEffect::Unknown` write can land after GC has reclaimed the destination's pre-mark, stranding an unreferenced/unmarked fragment (`crates/custodian/src/reconstruction/staged.rs:536`, adversary review). This is a system-wide gap in the write-deadline model shared by every writer that carries a deadline (`crates/traits/src/lib.rs:886-891`), not specific to this slice — track and fix separately from #663.
