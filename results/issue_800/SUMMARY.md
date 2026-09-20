# Result — issue 800 / gc-fragment-less-mark-sweep

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: GC consumes an `orphan:` mark only while iterating a `list_fragments()` result
  (`crates/custodian/src/gc.rs:183-219`), so **a mark whose position holds no fragment is never
  visited and never deleted** (`0016:1359-1368`). `main` already writes such marks: every repair
  of a missing fragment marks that fragment's old position (`crates/custodian/src/reconstruction.rs:865-884`,
  committed at `:942-947`), where by definition no fragment is stored. 0016 adds producers by design:
  teardown marks a failed attempt's full planned placement, and a repoint pre-mark precedes its
  write. After #661 the paged walk survives the ledger's size, but each lap re-reads marks that
  nothing will ever remove, so a lap grows without bound.
- Success criterion: the NEW file `crates/custodian/tests/gc_mark_sweep.rs` passes. It runs
  over in-memory doubles built the way #661's `crates/custodian/tests/gc_ledger_walk.rs` builds
  them: a metadata double with a lowered `scan` cap and a `scan_page` of its own, and D-server
  doubles whose `list_fragments()` the test controls. Every pass builds a fresh `GcContext`, as the
  deployed loop does (`crates/server/src/custodian.rs:600-608`). Let `D` be the late-write
  deadline `W_repoint + W_write + δ_clock` (`0016:1381-1391`). It is a named constant whose parts
  are named constants, each with its derivation in a doc comment. No constant for `W_write` or
  `W_repoint` exists on `main` (since #638 the D server enforces a deadline the caller supplies,
  `crates/traits/src/lib.rs:808-815`), so this slice names them. The test **hard-codes `D`**.
  Legs:
  **(A) A fragment-less mark is swept once that is safe.** A mark at a position that no listed
  server reports, aged exactly `D`, where this pass's listing was taken at or after
  `orphaned_at + D`, is deleted, and the delete is audited on the durability seam and counted,
  as a reclaim is (`gc.rs:542-553`). On the base (`main` + #661 + #662) the mark survives every
  pass — the red.
  **(B) Only then.** Each case survives or is left alone: (i) the same mark aged `D − 1` ms;
  (ii) a mark whose D server is not in the pass's fleet; (iii) a mark whose position **is**
  listed — the reclaim path owns it, not the sweep; (iv) **a listing from an earlier pass never
  licenses a sweep** (X96, `0016:2625`): run a pass while the mark is aged `D − 1` ms, write a
  fragment into that position, then run the next pass with the mark aged at least `D` and still
  inside the grace window — it survives, because only this pass's own listing may show the
  position empty; (v) a fleet that names one server twice never makes a listed position look
  unlisted; (vi) no sweep while the reference set is incomplete (the pass answers `Blocked`,
  `gc.rs:234-241`), and none at a referenced position — the sweep answers to the same gate as a
  reclaim (`gc.rs:191`); (vii) a mark in any of #662's three value shapes is swept on its stamp,
  while a value that decodes as none of them is never swept, is left byte-identical, and is
  surfaced (ADR-0045 decision 3).
  **(C) `D` stays strictly inside the deployed grace.** `D <` the grace window the deployed pass
  uses (`GC_GRACE_WINDOW_MILLIS`, `crates/server/src/custodian.rs:114`, which is
  `LEASE_TTL_MILLIS = 60_000`, `crates/server/src/cli.rs:78`), proved against **that constant
  itself**, not a copy of its value, because `0016:1386-1388` requires `G_orphan > D` strictly.
  The proof must **not** be a new `*/tests/*.rs` file. A second added test file would join
  C4-verify's invocation, and on the base it cannot compile (`D` does not exist there), which
  turns the whole RED leg UNVERIFIABLE. A compile-time assertion beside the deployed constant, or
  a case in an existing server test file, both work. Say which in `build-notes.md`.
  **(D) A sweep never deletes a mark newer than the one it judged, and its accounting matches
  what landed.** (i) A mark re-stamped after the pass read it and before its delete survives,
  and is neither audited nor counted as swept (round 1's finding). (ii) When a commit fails
  partway through a pass's sweep writes, every delete that landed before the failure is still
  audited and counted, and none that did not land is. Evidence is claimed only once it is
  durable, as restore already does (`crates/custodian/src/restore.rs:340-346`). (iii) After a lost
  precondition, whatever the pass concludes about that mark rests on a fresh read of it, never on
  the `Conflict` alone. A `Conflict` says only that a precondition lost
  (`crates/traits/src/lib.rs:1461-1465`), and the key may since be absent. Drive it with a racing
  writer that rewrites and then deletes the mark between the pass's read and its delete: the pass
  claims neither a sweep nor a protecting mark.
  **(E) A differently spelled key never costs a mark.** `parse_orphan_key` reads each field as a
  plain integer (`crates/core/src/metadata.rs:78-85`), so `orphan:5:01:0` and `orphan:5:1:0`
  decode to the same position, while every writer spells keys through `orphan_key`
  (`metadata.rs:72-74`). The sweep never deletes a mark at its own key on the strength of a
  differently spelled key's stamp or of its listing flag, and it never deletes or rewrites the
  differently spelled key, which it surfaces instead. Two cases: the position is listed and the
  alias is old; the position is unlisted, the alias is aged past `D` and the mark is younger
  than `D`. In both, the mark survives (round 2's two findings). This is the rule #661 sets for
  reclaims (its leg E), applied to the sweep.
  **(F) Seeded DST**, appended to the **existing** `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53` — keep it; add no new DST file). While GC walks a paged ledger over
  the simulated-TiKV store, with a page cap the seed picks, a concurrent task re-stamps a sweep
  target at a seed-chosen instant. The refreshed mark always survives. A coverage leg proves that
  some landing point falls between the pass's read and its delete, and some does not, as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2132-2165`). It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1616`). Record the seed count in
  `build-notes.md`.
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: GC's sweep of fragment-less marks: the deadline `D` and its named parts; the rule
  that an absence counts only when this pass observed it after `D`; the same reference-set gate
  a reclaim answers to; decoding through #662's value shapes, failing closed on a value that
  matches none of them; the differently spelled key rule; the sweep's own writes, in batches no
  larger than #661's write bound, with a delete that loses to a newer mark and accounting that
  matches what landed; the proof that `D` is inside the deployed grace; and the DST property.
  If the sweep changes what `docs/design/architecture/06-runtime-view.md` §6.7 step 2 (`:74`) says
  GC does, update that line. Must NOT change `reconcile_step`'s signature, and must NOT add a
  field to `GcContext` or any other context (the test builds them with struct literals). Keep
  #661's walk rules and #662's reclaim ordering exactly as they judge: consume them, do not
  reshape them. / out of scope: the paged walk and its budget (#661); the staged reference set,
  reclamation intent and the value codec itself (#662); deadline enforcement in any writer —
  the staged re-place (#663), flat pre-marking (#723) and multipart teardown — beyond naming the
  obligation next to `D`; tightening `D` per event kind (`0016:1388-1390`; the uniform bound is
  the safe default); restore; `scrub.rs`, `reconstruction.rs`, `rebalance.rs`, `desired_state.rs`;
  any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (16 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 99 of 99 instrumentable changed lines executed (floor 80%); 99 of 371 changed lines were instrume
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 27 mutants tested in 3m: 15 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_800/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.17s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review conclusion for the fragment-less orphan-mark GC sweep: the current leak is fixed and verified red→green, but fixed-deadline safety for future pre-mark writers needs human confirmation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the leak, safety invariant, A–G acceptance legs, scope, dependencies, and falsifiable base behavior explicit (`brief.md:22`). |
| C2 Reproduction (red pre-fix) | PASS | Independent stash execution compiled the retained focused test and produced 16/16 assertion failures, including the direct fragment-less-mark survivor at `crates/custodian/tests/gc_mark_sweep.rs:640`. |
| C3 Change | PASS | The patch stays within the GC sweep, its outcome documentation/tests, the deployed grace proof, seeded DST, and living runtime view required by `brief.md:154`. |
| C4 Verification (red→green) | PASS | Independent restore execution passed all 16 focused tests; frozen CI also ran `typos`, docs render/link audit, fmt, clippy, tests and DST (`gate-logs/C4-ci.log:11`), with 100% instrumentable diff coverage (`gate-logs/C4-diff-cov.log:755`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether future pre-mark writers need a stronger publication obligation than fixed `W_write` — the sweep assumes an empty post-deadline listing stays empty (`crates/custodian/src/gc.rs:784`), but the store contract permits an unbounded publish to leave possibly late bytes under `WriteEffect::Unknown` (`crates/traits/src/lib.rs:871`), which could strand bytes after their mark is swept. |
| T1 Structure | PASS | The change preserves `GcContext` and `reconcile_step` boundaries, isolates bounded sweep and commit phases in GC (`crates/custodian/src/gc.rs:815`), and places the grace assertion beside the deployed constant (`crates/server/src/custodian.rs:116`). |
| T2 Shape | PASS | Exact-value CAS, fresh-read conflict handling, malformed-value fail-closed behavior, and canonical-key identity are explicit and adversarially asserted (`crates/custodian/src/gc.rs:867`, `crates/custodian/tests/gc_mark_sweep.rs:1013`, `crates/custodian/tests/gc_mark_sweep.rs:1468`). |
| T3 Runtime | PASS | The production pass reports landed sweeps only after durable commits (`crates/custodian/src/gc.rs:882`), and the seeded simulated-TiKV campaign reaches both inside and outside the read/delete window (`crates/dst/tests/custodian.rs:3718`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and the publish gate must rerun their audit (`gate-logs/T4-contribution.log:10`); affected-path prior art across merged/open/closed/rejected work is recorded at `brief.md:199`. |
| T5 Judgment | PASS | The tests exercise deadline boundaries, incomplete/reference protection, all value shapes, partial failures, re-stamps, fresh reads, aliases, and positive controls (`crates/custodian/tests/gc_mark_sweep.rs:724`, `crates/custodian/tests/gc_mark_sweep.rs:930`, `crates/custodian/tests/gc_mark_sweep.rs:1115`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm the 41 s cleanup deadline and stated 1 s cross-host skew budget are operationally fit for supported deployments — deterministic tests prove implementation behavior, not those deployment assumptions (`crates/custodian/src/gc.rs:194`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Decide whether future pre-mark writers need a stronger publication obligation than fixed `W_write` — the sweep assumes an empty post-deadline listing stays empty (`crates/custodian/src/gc.rs:784`), but the store contract permits an unbounded publish to leave possibly late bytes under `WriteEffect::Unknown` (`crates/traits/src/lib.rs:871`), which could strand bytes after their mark is swept.
- [x] Validation — fitness-to-purpose — Confirm the 41 s cleanup deadline and stated 1 s cross-host skew budget are operationally fit for supported deployments — deterministic tests prove implementation behavior, not those deployment assumptions (`crates/custodian/src/gc.rs:194`).
- [x] size backstop — this slice is behaving oversized: patch is 119 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
