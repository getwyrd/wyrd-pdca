# What prior plan-stage review already rejected (don't re-earn these findings)

Rev 3 of this brief drew **five** independent plan-stage reviews before it was blocked.
Read them before authoring rev 4 — they are the record of what a reviewer will attack.
All paths are relative to this harness repo root.

## On this bundle's brief

- `results/plan-review-codex-508.md` — **VERDICT: FAIL.** Cross-vendor review of rev 3.
  Two classes: (a) citation drift — `flow.py:637` wrong (the call is `:461`), the GC prose
  misreads `gc.rs:157-187` by omitting the reclaim-evidence gate, and "base reclaims
  everything unreferenced" directly contradicts `gc.rs:183-186`; (b) the substantive one —
  *"the heart of iteration 2's rejection, the permanent session-less-part leak, is still not
  specified in a production-faithful, mechanically decidable way."* That leak is exactly
  what proposal 0016 now settles (decisions 2, 3 and 5) — so rev 4 should **cite 0016**
  rather than re-derive a mechanism in the brief.
- `results/plan-review-adversary-508.md` (R1), `-508-r2.md` (R2), `-508-r3.md` (R3),
  `-508-r4.md` (R4) — four rounds of refutation. R4 confirms which parts held: the
  staged-set-in-`protects` decision, `stranded_marked == 0` as an observable oracle,
  the bounded phase-3 batching and its idempotence argument, the compile-red / false-PASS
  defence, and every load-bearing citation re-checked in rev 3's new text. R4 also records
  that the brief **changed under the reviewer mid-review** — don't do that again.
- `results/plan-review-adversary-507-510.md` — cross-bundle pass covering 508 alongside
  its sibling slices; check it for consistency obligations against #507/#509/#510.

## On the design the slice now rests on

`results/issue_626/` ran seven iterations over the same protocol and was signed off
**discontinued** (moved to interactive refinement, which produced `97e2392`). Its review
reports — `results/plan-review-codex-626.md` and `-626-r2.md` … `-626-r6.md`
(r6 verdict: **READY**), plus `results/plan-review-adversary-508-r*.md` — enumerate the
attacks the protocol survived and the six findings the hand-refinement then closed.
`results/issue_626/SUMMARY.md` §10 is that punch list.

**Caveat:** those reports predate `97e2392`. Where a report attacks the `sinf:` counter,
the retry bound, the slot-before-lease ordering, the drain CAS, or the X52 compensation
branch, the landed document has **removed the counter** and resolved the family by
construction. Read the landed text as authoritative; read the reports for the *shape* of
attack rev 4 must withstand.
