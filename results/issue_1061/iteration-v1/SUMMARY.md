# Result — issue 1061 / multipart-retirement-drain-records

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Every multipart slice installs `retire:` obligations and nothing on `main` drains
  them. The `retire:records:` ones, the root flip's published parts (`{parts}`, #1008) and a
  rolled-back attempt's segments (`{seg}`, #1009 and the restore fence), leave `part:`, `psum:` and
  `seg:` records for ever. #1012's first attempt built a drain, and Check found three defects in it:
  (1) a pass's budget counted only commits, so obligations that could not progress cost nothing,
  one pass walked the whole ledger (a probe read 4,096 held obligations in 65 pages in one pass),
  and the per-pass bounds the docs promised had no test; (2) writing the advanced cursor back could
  push an obligation past `MAX_VALUE_BYTES`, the store refuses that for good (FoundationDB 2103),
  and the error ended the whole leg on every pass, so every later obligation starved; (3) finding
  the next member of a sparse part set rescanned the runs from the first one each time (O(N²)).
- Success criterion: A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (the core-level parts, (K) and the Complete refusal in (W), MAY go inline in
  `crates/core/src/multipart.rs` or in a NEW `crates/core/tests/*.rs`), over `RedbMetadataStore::in_memory()`, driving the drain **only
  through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), never a test-only entry. It shows all of:
  **(B-rec) Complete, then drain.** After a production subset Complete (#1008) and reconcile
  passes: no `part:` or `psum:` record of a published part remains; the drain wrote **no**
  `orphan:` record at all (`0016:1503`); the published object reads back byte-identical; the
  unnamed parts' records and their `retire:bytes:` obligation are byte-identical to before (that
  mode is child-2's).
  **(C) Convergence at scale.** A `retire:records:` `{parts}` obligation over **at least 4,001
  part numbers, no two adjacent** (one run per part; planted canonical `part:`/`psum:` records are
  fine), and a `{seg}` obligation over more `seg:` records than `MAX_BATCH_OPS`, drain to empty
  across steps. Every commit stays within `MAX_BATCH_BYTES` and `MAX_BATCH_OPS`, preconditions
  counted. The obligation's stored value changes every step (its cursor advanced); a drain resumed
  from fresh contexts (nothing in memory) continues from the stored cursor; the obligation is
  deleted only in the batch that deletes its last record; a second full drain is a no-op.
  **Combined payload:** a `retire:records:` `{parts, seg}` obligation (the row the decoder accepts,
  `crates/core/src/multipart.rs:3866`; no production writer installs it yet, so the fixture plants
  its canonical encoding), sized so its drain crosses from one component to the other **between
  two commits**. A drain resumed from fresh contexts exactly at that boundary continues into the
  second component; after the first component's last record is gone the obligation is still
  present, its cursor in the second component; it is deleted only with the second component's
  last record.
  **(L) Per-pass bounds, no starvation.** With a recording store wrapper counting reads and
  commits per pass, over a ledger at least three times the per-pass visit bound Do chooses:
  (L1) a productive backlog: every pass stays within fixed bounds on obligations visited, store
  reads and commits, whatever the ledger size; (L2) a held prefix: obligations that cannot progress
  (values that will not decode) sorting before one productive obligation: every pass stays within
  the same bounds, each held obligation stays byte-identical with a typed outcome and an audit
  event, and the productive obligation drains within a number of passes the test states from the
  bounds.
  **(W) Value ceiling.** Under a store wrapper that refuses any put over `MAX_VALUE_BYTES`, as
  FoundationDB does: an obligation installed within the installer ceiling (which now leaves the
  cursor's room) drains to empty; one planted just under `MAX_VALUE_BYTES` (e.g. 6,249 non-adjacent
  six-digit part numbers, 99,995 bytes) is held byte-identical with a typed outcome and an audit
  event, its over-ceiling put is never submitted, no pass returns `Err`, and a second obligation
  later in key order drains in the same passes. A Complete whose obligation would leave less than
  the cursor's room under `MAX_VALUE_BYTES` is refused where #1011 refuses one over the ceiling.
  **(G-rec) Restore between records-drain steps.** An `Aborting@E+1` session fenced from
  `Completing@E` with segments written (installed by the production restore fence, as
  `crates/custodian/tests/restore_completing_fence.rs` does) has its `retire:records:` `{seg}`
  obligation drained **one step at a time**. After every step the production
  `reconcile_after_restore` (`crates/custodian/src/restore.rs:597`) does not name the session (in
  particular no `NoDeleter`, `restore.rs:1176`) until that obligation is gone.
  **(K) Cursor codec.** An obligation without a cursor encodes byte-identical to today (no
  installer's bytes change); a cursor round-trips exactly. Decode refuses, with a typed error,
  every cursor it can judge from the key and value it is given: one on a component the payload
  does not carry; one past the end of work the payload itself lists (a part number past the
  `{parts}` set's last member; an index past a `{chunks}` list or a flat `{generation}` list); a
  segment index past the key grammar's bound `MAX_SEGMENT_INDEX`
  (`crates/core/src/metadata.rs:536`). A `{seg}` payload names only the group's nonce and epoch
  (`SegmentGroup`, `metadata.rs:1052-1055`), so decode **cannot** know where the group's `seg:`
  range ends, and (K) does not ask it to. That end is the drain's to find, and (C) tests it: a
  `{seg}` component is finished only when a read of the group's **whole** `seg:` range, from its
  start, finds it empty. A planted `{seg}` cursor past the last record, with records still below
  it, is a typed hold (obligation byte-identical, audit event), never a finish; this half of (K)
  is asserted through `reconcile_step`, in the custodian test file.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the optional `cursor` component inside `RetirePayload`, decoded against its payload,
  with its maximum encoded size reserved under `MAX_VALUE_BYTES` in the ceiling installed
  obligations are held to; the retirement leg dispatched from `reconcile_step` in the deployed
  custodian; the `retire:` ledger walk with per-pass bounds on obligations visited, reads and
  commits, resuming past obligations that cannot progress; the drain step for `retire:records:`
  (`{parts}`: each member's `part:` and `psum:`; `{seg}`: the group's `seg:` records; both
  together), delete only, never mark; typed holds with audit events for an obligation that will not
  decode and one whose advanced value would cross the ceiling; next-member lookup over a part set
  that does not rescan from the first run; the `wyrd-metadata-redb` dev-dependency of
  `wyrd-custodian` and its `Cargo.lock` line (approved 2026-11-11); the docs. / out of scope:
  - every `retire:bytes:` obligation, in any shape: this child leaves each byte-identical. Marking,
    the mark guard and its `reclaiming` check, the stamp, validated placements and their hold, the
    owned-residue hold, segments-before-parts, the DST row and the `deferred: #659` marker at
    `restore.rs:1156` are child-2's;
  - the owned `sidx:` walk and the terminal session delete (#1013); the orphan-identity gate and
    unlink/supersede routing (#1014); the reaper (#625); the operator abort (#633); concurrent
    drainers (#660); the S3 verbs (#668–#670);
  - any edit to proposal 0016 or an ADR; `deny*.toml`; any `Cargo.lock` change beyond the
    dev-dependency line.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.4% — 354 of 383 instrumentable changed lines executed (floor 80%); 383 of 980 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 172 mutants tested in 32m: 24 missed, 121 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1061/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.62s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1061’s bounded, resumable, records-only multipart retirement drain: three reproduced correctness failures warrant implementation iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The records-only boundary and bounded-work, persistence, isolation and compatibility requirements are explicit and testable; `brief.md:27`, `brief.md:119`, `brief.md:151`. |
| C2 Reproduction (red pre-fix) | PASS | An independent test using existing APIs executes and fails on the stashed base, then passes after restoration: the base retains the canonical parts obligation through `reconcile_step`; `crates/custodian/src/reconciliation.rs:166`, `pdca-reviewer-1061-probes/base-red-result.log:115`, `pdca-reviewer-1061-probes/base-green-result.log:13`. |
| C3 Change | PASS | The changes stay within the authorized codec, installer ceiling, reconstruction dispatch, records drain, tests and living docs; the concrete backend remains a dev dependency; `crates/core/src/multipart/complete.rs:1200`, `crates/custodian/Cargo.toml:65`, `docs/design/architecture/06-runtime-view.md:88`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Confirm the planned full-discriminator red-leg exception at sign-off — its tests cannot compile on the base, although the narrower independent behavioral red→green succeeds and all 29 focused target tests pass; `brief.md:151`, `gate-logs/C4-verify.log:140`, `review-red-green.log:125`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve the obligation across backend-clamped segment pages — ignoring a returned continuation deletes the obligation while a segment remains, defeating eventual drainage; finding 1, `crates/custodian/src/retirement.rs:496`. |
| T1 Structure | PASS | Production maintenance stays on the metadata trait seam and existing deployed reconstruction input, with durable progress and no new clock or shared mutable global; `crates/custodian/src/retirement.rs:195`, `crates/custodian/src/reconciliation.rs:155`, `crates/server/src/custodian.rs:397`. |
| T2 Shape | PASS | Optional cursor omission, canonical decoding and typed bounds preserve existing encodings; sparse-member lookup uses binary search; six codec tests pass; `crates/core/src/multipart.rs:3517`, `crates/core/src/multipart.rs:4250`, `crates/core/src/multipart.rs:4552`, `crates/core/tests/multipart_retire_cursor.rs:148`. |
| T3 Runtime | FAIL | Completion is falsely certified with outstanding work both at a ledger wrap and after retry-budget exhaustion; reproduced with 16 unfinished obligations and with 16 conflicts respectively; findings 2–3, `crates/custodian/src/retirement.rs:240`, `crates/custodian/src/retirement.rs:300`. |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive check must rerun at publish, as the deferred gate records; `gate-logs/T4-contribution.log:10`. Other T4 gate outcomes and the completed path-based prior-art check are recorded below. |
| T5 Judgment | NEEDS-HUMAN [impl] | Add regressions for clamped pages, unfinished obligations across ledger wraps and exhausted retries — the current default-cap, one-step-backlog and single-conflict fixtures miss all three reproduced failures; `crates/custodian/tests/multipart_retirement_drain.rs:856`, `crates/custodian/tests/multipart_retirement_drain.rs:979`, `crates/custodian/tests/multipart_retirement_drain.rs:1312`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Judge whether the records-only rollout and documented downgrade/restore behavior meet operational needs after the defects are addressed — automated convergence checks cannot establish deployment fitness; `docs/design/architecture/06-runtime-view.md:88`. |

Source citations above and below are relative to the patched `$PDCA_TARGET`; evidence files are relative to this review directory. The target is readable and contains the prerequisites. All 13 patch files were restored byte-for-byte after the stash/reapply checks; no implementation fix was made.

The three findings are executable failures against the supplied patch. They concern lost retirement work and false completion, not the deferred bytes-mode or multi-drainer simulation work.

1. **FAIL — A backend-clamped segment page loses the obligation before the group is drained.** At `crates/custodian/src/retirement.rs:496`, `seg_step` discards the store’s continuation and only creates its own continuation when the returned page contains a lookahead row beyond `take` (`:507`). A conforming store can clamp the requested limit: `crates/traits/src/lib.rs:1402` explicitly permits this, and `crates/metadata-redb/src/lib.rs:86` exposes the cap. With a real in-memory redb store capped at two rows and three canonical segment records, the store returns two rows **and a continuation**. One reconciliation pass deletes those two records and the obligation, leaving the third segment permanently without this drain obligation. The default backend cap does not trigger this case; the supported bounded-page contract does. Preserve progress when the backend indicates more work and establish exhaustion before deleting the obligation. Evidence: `pdca-reviewer-1061-probes/results.log:23`.

2. **FAIL — Reaching an empty ledger tail certifies unfinished obligations before the bookmark.** At `crates/custodian/src/retirement.rs:240`, a terminal suffix sets `reached_end`; `:249` resets the bookmark, and `:263` can return `Satisfied` without examining the prefix. With 16 obligations of 300 parts each on default redb, the first pass returns `Changed` after partially draining each obligation and saves a bookmark after the last one. The second pass reads the empty suffix and returns `Satisfied` while **16 obligations and 816 part records remain**, with their summaries also still present. A caller stopping at `Satisfied` stops before convergence; that violates the outcome’s documented contract (`crates/custodian/src/reconciliation.rs:55`). Return a non-certifying outcome when wrapping an incomplete walk, or establish that the earlier work is finished. Evidence: `pdca-reviewer-1061-probes/results.log:27`.

3. **FAIL — Exhausting conflict retries certifies an obligation that still exists.** At `crates/custodian/src/retirement.rs:300`, the retry loop returns without recording incomplete work when its budget reaches zero. If this is the final ledger row, the outer loop reaches the end with neither `changed` nor `held` set and returns `Satisfied`. A recording wrapper returning `Conflict` for conditional attempts reproduced **16 conflicts, an unchanged obligation, and `Satisfied`**. Record a non-certifying result when the retry budget expires with the obligation still present. This confirms the frozen batch-review finding (`gate-logs/T4-batch-review.log:10`); it concerns the existing retry path, not adding the deferred concurrent-drainer feature. Evidence: `pdca-reviewer-1061-probes/results.log:16`.

The probes use the target’s existing redb fixtures and public `reconcile_step` entry. They live outside the target source. Their source and compilation command are in `pdca-reviewer-1061-probes/probe.rs` and `compile-command.json`; the compiled reproduction runs with:

```sh
./pdca-reviewer-1061-probes/probe reviewer_ --nocapture
```

Independent verification establishes the ordinary success paths, while retaining the limits of each evidence source:

- **PASS, rerun:** the target’s 13 drain tests, six cursor tests and ten Complete-ceiling tests; `git diff --check`; formatting; `typos`; docs lint; docs rendering and link audit (101 pages); and Clippy for core and custodian with all targets and warnings denied. The declared `typos` and renderer dependencies were exercised, not substituted.
- **PASS, narrow behavioral red→green:** `pdca-reviewer-1061-probes/run_base_api.py` stashes production changes, runs a standalone test using existing production APIs and real redb, restores the patch, and reruns the identical test. The red executes and fails with the obligation retained; the green executes and drains it. The standalone harness has its own dependency lock and does not replace the target’s locked suite.
- **UNVERIFIABLE, full discriminator:** independently reproducing the gate’s production-reverted/test-retained setup gives compilation errors for the new cursor APIs, then 19 passing tests after restoration (`review-red-green.log:3`, `:165`, `:184`). This is the anticipated exception, not a behavioral red. The narrower test above does not prove every required mutation scenario in `brief.md:157`.
- **PASS, frozen full-CI evidence:** the captured log actually shows spelling, rendered docs, compilation, tests, dependency checks and DST, ending in success (`gate-logs/C4-ci.log:11`, `:16`, `:4270`, `:4903`). The instance-scoped wrapper was not rerun as a whole; the independent checks above were rerun directly.
- **PASS, frozen coverage evidence:** 354 of 383 instrumentable changed lines executed, 92.4%; 597 other changed lines were unscored (`gate-logs/C4-diff-cov.log:120`, `:162`). Coverage does not cover the missing behavioral cases proved above.
- **FAIL, advisory mutation evidence:** the frozen run reports 172 mutants, 24 missed, 121 caught and 27 unviable (`gate-logs/C5-mutants.log:37`). Its survivor list includes redundant bounds and diagnostic behavior; the count alone is not a finding. The implementation requests above rest on reproduced failures. The instance-scoped mutation wrapper was not rerun.
- **FAIL, frozen T4 batch review:** its one finding is independently corroborated by finding 3; it remains unresolved (`gate-logs/T4-batch-review.log:10`). **PASS, frozen TiKV feature check:** the log shows actual crate/server Clippy compilation completing (`gate-logs/host-tikv.log:211`), not a shim or code-read. **N/A, contribution check:** publication artifacts are intentionally deferred to the publish gate (`gate-logs/T4-contribution.log:10`).

**PASS — Prior-art checking ran by affected file path.** GitHub `main` commit-history queries covered all 13 changed paths; all three new Rust files have no merged history. Exact-path closed-work searches were supplemented by enumerating all 19 closed, unmerged PRs and comparing their changed filenames against the patch. None touched the new drain or cursor-test paths. The overlapping non-dependency PR [#647](https://github.com/getwyrd/wyrd/pull/647) was closed with a request to split its scope. [#636](https://github.com/getwyrd/wyrd/issues/636) is closed `not_planned` and records the earlier cursor/convergence failures; #1012’s unpublished attempt remains described by the supplied brief, not independently available as a patch. Query evidence is retained in `review-prior-art.json`, `review-closed-pr-paths.json`, `review-prior-art-issues.json` and `review-prior-art-647-comments.json`. No new scope decision arose from that check.

The standing rubric’s serialization, metadata-boundary, dependency-direction, bounded-await and docs requirements were checked. The symptom-guard smell test does not fire: the new holds validate malformed or oversized persisted work, rather than probe an expected capability to hide a load-time cause. The explicit DST/GC and concurrent-drainer deferrals at `crates/custodian/src/retirement.rs:184` remain settled. This change does not alter fragment I/O, so no additional Tier-1 disk-fault campaign is indicated; the scheduled Tier-2 custodian run can provide deployment observation after correction.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1061 (retirement drain, records mode)

I wrote two probe tests and ran them against the patched tree. Both failed, so both bugs below are real. The probes are appended to a scratch copy of `crates/custodian/tests/multipart_retirement_drain.rs` and saved as `adversary-probes.rs` next to this file. In the same rebuild, the builder's 13 drain tests pass.

- NEEDS-HUMAN [impl] — **The `{seg}` drain can delete the obligation while records remain (truncated derivation, the #636 class).** `crates/custodian/src/retirement.rs:496` throws away `next` from `checked_page` (`let (page, _) = …`). Lines `:497-512` then treat any page shorter than `take` as the end of the group. The whole-range check at `:513` runs only when `entry > 0`, and even then it only looks for records *below* the cursor, never above the page. Under the trait contract, a page is `min(limit, the store's effective cap)` long and `next` is the only end signal (`crates/traits/src/lib.rs:1402-1405`). Every production backend has a public `with_scan_cap` (`crates/metadata-redb/src/lib.rs:86`, `crates/metadata-fdb/src/lib.rs:1336`, `crates/metadata-tikv/src/lib.rs:1083`), and the custodian DST suite already runs under lowered caps (`crates/dst/tests/custodian.rs:2390`). **Failing case** (probe A): `RedbMetadataStore::in_memory().with_scan_cap(64)`, 200 `seg:` records, one `retire:records:{seg}` obligation. One pass answers `Changed`, deletes 64 records **and the obligation**, and leaves 136 `seg:` records with nothing to drain them (`obligation_present=false seg_left=136 of 200`). This contradicts the module doc at `retirement.rs:56-60` and the brief's (K)/(C) promise that a `{seg}` component finishes only on a read of the whole range. Today's default cap (`1 << 20`) hides it. Fix: never finish on a page whose `next` is `Some`; resume after the page's last record instead. Add a (C)-style leg under a lowered cap.

- NEEDS-HUMAN [impl] — **A pass answers `Satisfied` while a records obligation still owes work, with no concurrency needed.** This is broader than the T4 blocking finding at `:300`. `retirement.rs:249-273` answers `Satisfied` whenever the walk reached the end, committed nothing and held nothing, even when the pass started from a stored mid-ledger position and never saw the head. **Failing case** (probe B): 63 `retire:bytes:` `{"parts":[[1,1]]}` obligations, then one `retire:records:{parts}` obligation over 600 sparse parts. The bytes obligations sort first, every Complete with unnamed parts installs one, and this child drains none of them. Pass 1 spends its 64th visit stepping the records obligation (`Changed`), so the resume point is that key. Pass 2 reads the empty last page and answers `Satisfied`. The obligation still holds `cursor parts@499` and 351 `part:` records (`first=Changed second=Satisfied … part_records_left=351`). `Reconciled`'s own docs call `Satisfied` a certification a caller may stop on (`crates/custodian/src/reconciliation.rs:22`, `:47-58`). A fix scoped to line 300 alone leaves this case. Impact is limited today: the deployed loop ignores the outcome (`crates/server/src/custodian.rs:604-617`) and no in-repo caller loops until `Satisfied`. GC's walk has the same "a resumed walk that reaches the end certifies" shape, so the builder needs to choose one rule. Options: answer `Partial` unless the walk started at the head, or whenever a step this pass left an advanced cursor.

- The T4-batch-review blocking finding (`retirement.rs:300`) is **confirmed** by reading the code. A conflict re-read can find the obligation still present with `steps_left == 0`. `visit` then returns without setting `held` or `changed`, the walk moves past the obligation (`:238`), and if it was the last ledger entry (`:240-242`) the pass answers `Satisfied`. It needs all 16 steps lost to a concurrent writer (#660, deferred), so it is a narrow case of the finding above. Fixing that one fixes this too.

- NEEDS-HUMAN [impl] — **The tests check the code against itself, so two claimed properties are never pinned.**
  - (a) Every hold-reason assertion compares the audit `hold` field to the production `Hold::X.as_str()`: `multipart_retirement_drain.rs:1052`, `:1124`, `:1272`, `:1296`, `:1411`. That is why the C5 mutants `Hold::as_str → ""` / `"xyzzy"` (`retirement.rs:165`) survive: the suite cannot tell one hold reason from another. `reconcile` returns only `Blocked`, so the audit string is the only place the "typed outcome" the brief's (L2)/(W) ask for can be seen. Assert the literal strings.
  - (b) (L1)'s fixture (`:979-999`) is 192 one-step `{parts:[[1,1]]}` obligations. Those steps read nothing, and 16 steps exactly fill one 16-entry page, so no pass makes more than 6 reads. The C5 survivors confirm this: `READS_PER_PASS` shrunk to 6 (`retirement.rs:121`, `* → /`), the page cap disabled (`:222`, `-= → +=`; `:109`), and the walk continuing after steps run out (`:233`, `|| → &&`, which visits obligations and skips them without a step) all pass. The read bound for `{seg}` steps and fairness within a page are untested. Add a `{seg}` backlog, and a page holding more records obligations than `STEPS_PER_PASS`.

- **Attempted to refute, could not:**
  - The 69-byte cursor reservation (`crates/core/src/multipart.rs:4065`, `:4079`). The cursor is encoded last (`:4187`), the widest entry is capped at 999,999 by const asserts, and `item` is omitted when zero.
  - The (W) arithmetic: 6,245 × 16 + 11 = 99,931.
  - The Complete refusal (`crates/core/src/multipart/complete.rs:1203`).
  - `members_from`'s binary search start (`multipart.rs:3517`) for `from = 0`, inside a run, and between runs.
  - Decode refusing a non-member `{parts}` cursor, a `seg` cursor past `MAX_SEGMENT_INDEX`, and a non-zero `item` under records mode (`multipart.rs:4250`).
  - The `{parts, seg}` component switch landing on a commit boundary (`retirement.rs:453-456`).
  - The ledger walk honouring `next` (`retirement.rs:240`).

  C4-verify's UNVERIFIABLE result was accepted in advance and is not counted against the fix. C5's 121 caught mutants are the independent evidence that the drain tests go red when the code is broken.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement.rs:496`: The segment walk discards `scan_page`'s continuation and treats any page with at most `take` records as the end. The store may clamp the requested limit (`crates/traits/src/lib.rs:1402`; redb does so at `crates/metadata-redb/src/lib.rs:170`). With `RedbMetadataStore::in_memory().unwrap().with_scan_cap(1)`, a cursorless obligation over two segments returns one segment plus `Some(next)`. This code leaves its own `next` as `None`, skips the `entry > 0` check, and deletes the obligation alongside the first segment, permanently stranding the second. Honor the backend continuation before declaring completion and add a capped-page regression through `reconcile_step`.

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement.rs:300`: Exhausting all `STEPS_PER_PASS` retries while the final obligation still exists returns without recording incomplete work. If no step committed and nothing was held, the terminal ledger page sets `reached_end` at `crates/custodian/src/retirement.rs:240`, so the pass reports `Satisfied` with an undrained obligation. A caller driving until satisfied can stop prematurely. Preserve a `Partial` outcome on retry exhaustion and test repeated conflicts on a single obligation; the existing conflict test covers only one conflict. This independently confirms the finding in the frozen T4 review log.

No additional reuse, simplification, or efficiency findings. Reviewed against the target source and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Confirm the planned full-discriminator red-leg exception at sign-off — its tests cannot compile on the base, although the narrower independent behavioral red→green succeeds and all 29 focused target tests pass; `brief.md:151`, `gate-logs/C4-verify.log:140`, `review-red-green.log:125`.
- [ ] C5 Causal adequacy — Preserve the obligation across backend-clamped segment pages — ignoring a returned continuation deletes the obligation while a segment remains, defeating eventual drainage; finding 1, `crates/custodian/src/retirement.rs:496`.
- [ ] T5 Judgment — Add regressions for clamped pages, unfinished obligations across ledger wraps and exhausted retries — the current default-cap, one-step-backlog and single-conflict fixtures miss all three reproduced failures; `crates/custodian/tests/multipart_retirement_drain.rs:856`, `crates/custodian/tests/multipart_retirement_drain.rs:979`, `crates/custodian/tests/multipart_retirement_drain.rs:1312`.
- [ ] Validation — fitness-to-purpose — Judge whether the records-only rollout and documented downgrade/restore behavior meet operational needs after the defects are addressed — automated convergence checks cannot establish deployment fitness; `docs/design/architecture/06-runtime-view.md:88`.
- [ ] **The `{seg}` drain can delete the obligation while records remain (truncated derivation, the #636 class).** `crates/custodian/src/retirement.rs:496` throws away `next` from `checked_page` (`let (page, _) = …`). Lines `:497-512` then treat any page shorter than `take` as the end of the group. The whole-range check at `:513` runs only when `entry > 0`, and even then it only looks for records *below* the cursor, never above the page. Under the trait contract, a page is `min(limit, the store's effective cap)` long and `next` is the only end signal (`crates/traits/src/lib.rs:1402-1405`). Every production backend has a public `with_scan_cap` (`crates/metadata-redb/src/lib.rs:86`, `crates/metadata-fdb/src/lib.rs:1336`, `crates/metadata-tikv/src/lib.rs:1083`), and the custodian DST suite already runs under lowered caps (`crates/dst/tests/custodian.rs:2390`). **Failing case** (probe A): `RedbMetadataStore::in_memory().with_scan_cap(64)`, 200 `seg:` records, one `retire:records:{seg}` obligation. One pass answers `Changed`, deletes 64 records **and the obligation**, and leaves 136 `seg:` records with nothing to drain them (`obligation_present=false seg_left=136 of 200`). This contradicts the module doc at `retirement.rs:56-60` and the brief's (K)/(C) promise that a `{seg}` component finishes only on a read of the whole range. Today's default cap (`1 << 20`) hides it. Fix: never finish on a page whose `next` is `Some`; resume after the page's last record instead. Add a (C)-style leg under a lowered cap.
- [ ] **A pass answers `Satisfied` while a records obligation still owes work, with no concurrency needed.** This is broader than the T4 blocking finding at `:300`. `retirement.rs:249-273` answers `Satisfied` whenever the walk reached the end, committed nothing and held nothing, even when the pass started from a stored mid-ledger position and never saw the head. **Failing case** (probe B): 63 `retire:bytes:` `{"parts":[[1,1]]}` obligations, then one `retire:records:{parts}` obligation over 600 sparse parts. The bytes obligations sort first, every Complete with unnamed parts installs one, and this child drains none of them. Pass 1 spends its 64th visit stepping the records obligation (`Changed`), so the resume point is that key. Pass 2 reads the empty last page and answers `Satisfied`. The obligation still holds `cursor parts@499` and 351 `part:` records (`first=Changed second=Satisfied … part_records_left=351`). `Reconciled`'s own docs call `Satisfied` a certification a caller may stop on (`crates/custodian/src/reconciliation.rs:22`, `:47-58`). A fix scoped to line 300 alone leaves this case. Impact is limited today: the deployed loop ignores the outcome (`crates/server/src/custodian.rs:604-617`) and no in-repo caller loops until `Satisfied`. GC's walk has the same "a resumed walk that reaches the end certifies" shape, so the builder needs to choose one rule. Options: answer `Partial` unless the walk started at the head, or whenever a step this pass left an advanced cursor.
- [ ] **The tests check the code against itself, so two claimed properties are never pinned.**
- [ ] `crates/custodian/src/retirement.rs:496`: The segment walk discards `scan_page`'s continuation and treats any page with at most `take` records as the end. The store may clamp the requested limit (`crates/traits/src/lib.rs:1402`; redb does so at `crates/metadata-redb/src/lib.rs:170`). With `RedbMetadataStore::in_memory().unwrap().with_scan_cap(1)`, a cursorless obligation over two segments returns one segment plus `Some(next)`. This code leaves its own `next` as `None`, skips the `entry > 0` check, and deletes the obligation alongside the first segment, permanently stranding the second. Honor the backend continuation before declaring completion and add a capped-page regression through `reconcile_step`.
- [ ] `crates/custodian/src/retirement.rs:300`: Exhausting all `STEPS_PER_PASS` retries while the final obligation still exists returns without recording incomplete work. If no step committed and nothing was held, the terminal ledger page sets `reached_end` at `crates/custodian/src/retirement.rs:240`, so the pass reports `Satisfied` with an undrained obligation. A caller driving until satisfied can stop prematurely. Preserve a `Partial` outcome on retry exhaustion and test repeated conflicts on a single obligation; the existing conflict test covers only one conflict. This independently confirms the finding in the frozen T4 review log.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1061/review-
- [ ] **(K) requires an end-of-payload check that the segment payload cannot support.** `brief.md:66` and `brief.md:222` require `decode_retire_obligation` to reject a cursor past its payload's end; `brief.md:212` makes the segment cursor the next segment index. But `RetirePayload.seg` holds only a `SegmentGroup` (`crates/core/src/multipart.rs:3822`), whose fields are nonce and epoch, with no count or last index (`crates/core/src/metadata.rs:1052`). Decode receives only key/value bytes (`crates/core/src/multipart.rs:4081`). Identical payload/cursor bytes cannot distinguish an index beyond the actual segment range from one within it. Revise (K) to distinguish payload-checkable bounds from segment key-space bounds (`metadata.rs:536`), and require actual range exhaustion to be checked by the drain. Otherwise Do must weaken the criterion silently or add an unstated persisted extent field.
- [ ] **The criteria miss the combined obligation's component transition.** Scope expressly includes parts and segments “both together” (`brief.md:109`); the target codec likewise permits one `{parts, seg}` obligation (`crates/core/src/multipart.rs:3774`). Yet (C) specifies separate `{parts}` and `{seg}` obligations (`brief.md:38`), while (B-rec) and (G-rec) also exercise single-component obligations. A drain that deletes the obligation upon finishing its first component can satisfy these fixtures while stranding the second component's records. Add a combined-payload fixture that crosses the component boundary across commits, resumes with fresh contexts there, and proves the obligation survives until both components are empty; require the premature-deletion mutation to fail this case.
- [ ] size backstop — this slice is behaving oversized: patch is 186 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rejected for three reproduced defects (the step logic and the cursor codec held up); keep the slice, fix these: 1. seg_step (retirement.rs:496) discards the backend's `next` continuation and finishes a `{seg}` component on a page the store clamped. Honor `next`: never delete the obligation while `next` is Some; resume after the page's last record. Add a (C)-style leg under `RedbMetadataStore::in_memory().with_scan_cap(..)`. 2. A pass that resumed mid-ledger and reached the end answers `Satisfied` without seeing the head (retirement.rs:240-273), leaving undrained obligations certified done. Choose one rule: answer Partial unless the walk started at the head, or whenever a step this pass left an advanced cursor. The same rule must cover retry-budget exhaustion at retirement.rs:300 (the T4 blocking finding). Add regressions for both: a wrap with unfinished obligations, and repeated conflicts on one obligation. 3. Tests check the code against itself: assert literal hold-reason strings, not `Hold::X.as_str()`; add a `{seg}` backlog and a page holding more records obligations than STEPS_PER_PASS so the L1 read/visit bounds and page cap are actually pinned (C5 survivors at retirement.rs:121, :222, :233). Size: the 186 KB patch is about two-thirds tests; the human judged this a fix round, not a re-split.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
