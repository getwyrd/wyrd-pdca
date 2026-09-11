# Backlog reversal — making the cycle close more issues than it opens

**Date:** 2026-09-11. Input to the next Act review; nothing here is applied until Act records it.
**Evidence:** GitHub `getwyrd/wyrd` issues and PRs, `getwyrd/wyrd-pdca` issues and PRs, `pdca status`
on 2026-09-11, and the bundle records under `results/`. Every number is measured, not estimated.

## 1. What the records show

| Month | wyrd issues opened | closed | wyrd PRs merged (excl. Dependabot) | wyrd-pdca PRs merged | wyrd commits | wyrd-pdca commits |
|---|---|---|---|---|---|---|
| 2026-06 | 204 | 132 | ~149 | 47 | 197 | 47 |
| 2026-07 | 187 | 108 | ~123 | 76 | 260 | 133 |
| 2026-08 | 78 | 20 | 17 | 25 | 45 | 51 |
| 2026-09 (to the 11th) | 0 | 1 | 0 | 0 | 4 | 0 |

- **208 open wyrd issues.** Of the 64 opened in August, **63** were produced by the cycle itself
  (split children, "review finding on merged PR" items, deferred follow-ups). July: 82 of 97.
- **The cycle has three issue producers and one consumer.** Producers: `pdca split --accept`
  (5 issues → 24 slices on 07-31; #735 → 25 children; #654 → 3 → #692 → 3 → #717 → 2; #711, #721,
  #736 each split again), review/adversary findings filed as tracker items (81 open issues carry
  follow-up/deferred language), and Act "follow-ups routed". Consumer: Do → Check → merge, which
  runs one `pdca flow` at a time, two lanes, 30-minute merge settle per wave, on chains four and
  five deep (773 → 774 → 775 → 741 → 742; 771 → 772 → 693 → 655 → 656 …). A split proposal takes
  minutes; a wave takes hours to days.
- **The critical path was blocked on a dead bundle.** #508, #625, #633 and #637 declared
  `Depends on: 636`; #636 is DISCONTINUED and a dependency must reach COMPLETE
  (`waves.py:57-83`). Fixed 2026-09-11 by repointing to #636's actual children (see each brief's
  ordering note). #637 now waits only on #771/#772.
- **Nearly every planned bundle is `[oversized]`** (16 of 23 non-terminal bundles), including
  two-file items like #736 (a `Server:` header). The sizer's verdict has one consequence in the
  ruleset — iterate-to-Plan, i.e. split — so "oversized" is functionally a split trigger with no
  counterweight. The knee the 07-31 report measured is ~100 KB of patch; most flagged bundles are
  nowhere near it.
- **The harness absorbed August.** More commits landed in wyrd-pdca than in wyrd. Each process
  failure became a harness change (merge-wait, sync-base, diff-cov, split tooling, handoff hooks).

## 2. Process deltas proposed (each is one Act-log line when accepted)

1. **WIP cap on Plan.** Ruleset: no new brief, split acceptance, or reslicing proposal while
   more than **6** bundles read PLANNED or BUILT-unsigned in `pdca status`. Planning resumes when
   the count drops. Instance rule in `docs/INTEGRATION.md §Plan`; ask the harness for a
   `[driver] planned_cap` that makes `pdca split --accept` and the planner leaf refuse above it.
2. **Splits close their parent.** A split may create children only if the parent issue is closed
   (or converted to a GitHub sub-issue container that the milestone view does not count). No
   "seam tracker" parents left open. Retroactively: close #508's, #635's, #636's, #637's, #654's,
   #692's, #717's and #735's parent issues or mark them as containers.
3. **One split per slice, ever.** A child that comes back oversized is built by hand or dropped;
   it is never split a third time (#654 → #692 → #717 → #771 was three levels).
4. **Findings stay in the bundle.** A reviewer/adversary/code-review finding on a bundle is
   fixed in that bundle's next round, recorded in `deferred-findings.json` and shown in §6, or
   dropped at sign-off with one line of reason. A tracker issue is filed only when the finding is
   `release-gating` for the current milestone. "Review finding on merged PR #NNN" items stop.
   Ruleset text in `AGENTS.md` (reviewer) and `.claude/agents/{reviewer,adversary,code-review}.md`.
5. **Recalibrate the sizer before it triggers anything.** Run `scripts/size-calibrate` against the
   bundles frozen since 07-31 and set `[driver.sizing] oversized` at the measured 100 KB knee, not
   the current 7-point sum. Until then `size_guard` stays `"off"` and an `[oversized]` verdict is
   informational: the sign-off default for an over-budget patch becomes **iterate-do with a
   size instruction**, and iterate-to-Plan needs a human reason in §9.
6. **Critical path first.** One named chain is the batch until it ships: multipart
   771 → 772 → 693 → 655 → (656, 657, 658, 659 planned under the WIP cap) → 637 → 625 → 633 → 508.
   The blackbox chain (773 → …) waits. Anything else enters only as a `close` disposition.
7. **Harness freeze.** wyrd-pdca accepts only changes that unblock the named chain. Everything
   else becomes a note in the Act log, not a PR. Review the freeze at the Act review after #508
   merges.
8. **Act judges by flow, not by findings.** Each Act review records four numbers for the period:
   issues opened, issues closed, non-bot PRs merged into wyrd, bundles in PLANNED. The review
   passes when closed ≥ opened and PLANNED shrank. A review that fails twice running triggers
   a re-plan of the process, not another split.

## 3. Immediate actions (done or ready to run)

- **Done 2026-09-11:** #508/#625/#633/#637 repointed off #636. `pdca status` now shows the real
  blockers; #637 is one wave behind #772.
- **Next `pdca flow` line:** `./scripts/pdca flow 771 772 693 655 637` — the named chain, nothing
  else. #771 is in ITERATE_DO; let it finish before adding ids.
- **Plan under the cap:** #656–#659 need briefs before #625 can build. Under delta 1 that is four
  briefs, written only as the chain drains below the cap. Settle the #625/#659 terminal-delete
  ownership overlap at #659's Plan (flagged in #625's ordering note).
- **Tracker hygiene that closes issues without code:** the open parents (delta 2) and the 25
  blackbox children of #735 that duplicate its proposal 0017 text can be converted to sub-issues
  or closed as `not planned` until the blackbox chain is scheduled.

## 4. How effectiveness will be judged

- 2026-09 and 2026-10 each show wyrd issues closed ≥ opened.
- PLANNED count in `pdca status` falls from 19 (2026-09-11) and never exceeds the cap.
- Non-bot PRs merged into wyrd exceed PRs merged into wyrd-pdca in every month of the freeze.
- No third-level split appears in `results/*/split-proposal.md` after this date.

## 5. Filed (2026-09-11)

Upstream, eduralph/pdca-harness (enforcement; optional, nothing above waits on them):
[#544](https://github.com/eduralph/pdca-harness/issues/544) `planned_cap` ·
[#545](https://github.com/eduralph/pdca-harness/issues/545) `split --accept` closes the parent, bounds depth ·
[#546](https://github.com/eduralph/pdca-harness/issues/546) `flow --only`.

Local, getwyrd/wyrd-pdca (the deltas themselves):
[#238](https://github.com/getwyrd/wyrd-pdca/issues/238) WIP cap ·
[#239](https://github.com/getwyrd/wyrd-pdca/issues/239) splits close parent, once only ·
[#240](https://github.com/getwyrd/wyrd-pdca/issues/240) findings stay in the bundle ·
[#241](https://github.com/getwyrd/wyrd-pdca/issues/241) sizer recalibration ·
[#242](https://github.com/getwyrd/wyrd-pdca/issues/242) critical-path run plan ·
[#243](https://github.com/getwyrd/wyrd-pdca/issues/243) harness freeze ·
[#244](https://github.com/getwyrd/wyrd-pdca/issues/244) Act judges by flow.
