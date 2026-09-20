# Result — issue 803 / staged-reference-set

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged bytes have no protection class. `ReferenceSet` holds committed placements
  only (`crates/custodian/src/gc.rs:383-413`), built from the `inode:` scan alone (`:478-573`).
  A committed part's fragments (`part:`) and an upload's in-flight owned fragments (`sidx:`,
  #772) are in no protected set, so GC reclaims one as soon as it carries an `orphan:` mark past
  grace (`:272`, `:277-326`). Restore gates on the same predicate
  (`crates/custodian/src/restore.rs:385`) and its pending skip (`:435-438`) no longer sees owned
  entries, so it marks a live upload's fragments stranded (`:440-443`) and the next GC pass
  deletes them.
- Success criterion: the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Records are seeded as raw JSON the base decoders accept (`SessionRecord`
  and `PartRecord` have no writer-side constructor, `crates/core/src/multipart.rs:2127`,
  `:2492`; shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Every protection leg also seeds an unprotected control the pass does reclaim or mark.
  Legs:
  **(A) GC protects both staged classes.** An `Open` session with a committed `part:` record
  (fragment `F1`) and an owned `sidx:` entry (`F2`) on D-server doubles, each with an `orphan:`
  mark past grace: after `reconcile_step`, both survive. (Unmarked, GC's conservative arm keeps
  any fragment, `gc.rs:307-310`, so the mark is what makes the leg bite.) Base: both reclaimed.
  **(B) Restore protects them through the same predicate.** `reconcile_after_restore` over the
  same store, unmarked, writes no `orphan:` key for `F1`/`F2` and `stranded_marked` excludes
  them; a GC pass past grace then keeps both. Base: marked, then deleted. (Staged counters are
  #664's.)
  **(C) Source before destination, both handoffs (`0016:782-800`, X67 `:2596`).** A double
  performs a handoff atomically after the first of the two reads involved completes — the
  source range or the destination range/scan, whichever the builder issues first: (i) a part
  commit (one batch deletes the chunk's `sidx:` entry and writes its `part:` record; source
  `sidx:<id>:`, destination `part:<id>:`); (ii) a publication (the committed inode naming the
  chunk is written and its `part:` record removed; source `part:<id>:`, destination the
  `inode:` scan). The fragment is marked past grace and is not reclaimed. Base: reclaimed.
  **(D) Bounded per-session reads (`0016:890`).** With the `scan` cap lowered, more
  sessions-with-parts than a global `scan("part:")` could return: `reconcile_step` succeeds and
  the double records no `scan`/`scan_page` of the bare `part:` or `sidx:` prefix. A guard.
  **(E) What it cannot read or trust fails closed (ADR-0045 decision 3,
  `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).** (i) A `part:` value that
  will not decode, an `sidx:` key naming no chunk, or an `mpu:` key naming no upload makes the
  set incomplete for GC and restore: GC reclaims nothing and answers `Reconciled::Blocked` (as
  `gc.rs:348-355`); restore marks nothing and names the record in `RestoreReport::unresolvable`.
  (ii) A staged placement of the wrong length, or an undecodable owned value under an `sidx:`
  key that names its chunk, holds that whole chunk in both passes and is named on each audit
  seam, while unrelated fragments are still judged. Base: reclaimed or marked.
  **(F) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53`; no new DST file): a concurrent part commit then publication at
  seed-chosen instants during GC's staged build never gets the chunk reclaimed; a coverage
  property proves landings between and outside the builder's reads are both reached, as `:2139`
  does.
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the staged protection class in the shared reference set: the committed `part:`
  records and owned `sidx:` entries of the sessions listed under `mpu:`, read through bounded
  per-session ranges, `sidx:` before `part:` before the `inode:` scan; its own member, disjoint
  from `placed`, honoured by the shared predicate (`gc.rs:424-451`) under its own audit reason.
  Never less than 0016's set; covering every listed session whatever its state is fine (it only
  keeps more). Scrub and drain status keep today's answers — they read `placed` and the
  committed `unresolvable` only — so an unreadable staged record makes the set incomplete for
  GC and restore alone. Restore names each staged record it holds or cannot read on its audit
  seam, and its report fields stay as they are (`restore.rs:105-170`): whether a held staged
  record sets `needs_human()` is #664's, marked `// deferred: #664` at the site. No change to
  `reconcile_step`'s or `reconcile_after_restore`'s signature; no new field on a context struct
  or `RestoreReport`. Docs: one paragraph in `docs/design/architecture/06-runtime-view.md` §6.7
  step 2 — GC never reclaims, and restore never marks, a staged fragment. / out of scope: mark
  shapes, reclaim intent, retirement protection (child-2); drain status, rebalance, restore's
  staged counters and fence (#664); scrub, reconstruction (#663); the fragment-less sweep
  (#800); `desired_state.rs`, `rebalance.rs`, `scrub.rs`, `reconstruction.rs`,
  `crates/core/src/metadata.rs`; 0016 and the ADRs.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 136 of 136 instrumentable changed lines executed (floor 80%); 136 of 418 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 25 mutants tested in 73s: 6 caught, 19 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.88s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing #803’s fix to keep staged multipart fragments protected across GC/restore handoffs; the patch passes technical review, with the fleet-wide fail-closed retention tradeoff left for human validation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit and falsifiable across protection, handoff ordering, bounded reads, corruption handling, DST, and CI (`brief.md:18`). |
| C2 Reproduction (red pre-fix) | PASS | Independent stash reproduction compiled the unchanged black-box test against the base and all 11 cases failed by assertion, matching `gate-logs/C4-verify.log:102`. |
| C3 Change | PASS | The change stays within the staged reference-set slice: the disjoint class feeds the shared deletion predicate and restore audit/report path without changing public pass signatures (`crates/custodian/src/gc.rs:446`, `crates/custodian/src/restore.rs:309`). |
| C4 Verification (red→green) | PASS | Independent reapplication produced 11/11 green; frozen CI also ran the 18-test DST suite and ended fully green (`gate-logs/C4-verify.log:105`, `gate-logs/C4-ci.log:3504`, `gate-logs/C4-ci.log:3550`). |
| C5 Causal adequacy | PASS | The eager omission is removed at its cause: staged sources are read before committed destinations and every destructive consumer uses the shared protection predicate, with no capability probe or runtime fallback (`crates/custodian/src/gc.rs:547`, `crates/custodian/src/gc.rs:789`, `crates/custodian/src/restore.rs:408`). |
| T1 Structure | PASS | A separate `StagedSet` preserves dependency direction and lets GC/restore protect staged bytes while scrub and drain continue reading only committed fields (`crates/custodian/src/gc.rs:416`, `crates/custodian/src/gc.rs:446`). |
| T2 Shape | PASS | Per-session `sidx:` then `part:` reads preserve bounded cardinality and source-before-destination ordering without a global staged scan (`crates/custodian/src/gc.rs:775`, `crates/custodian/src/gc.rs:789`). |
| T3 Runtime | PASS | The focused runtime suite exercises live pass entry points and controls, while seeded simulated-TiKV tests prove between/outside handoff landings (`crates/custodian/tests/staged_protection.rs:750`, `crates/dst/tests/custodian.rs:2926`, `crates/dst/tests/custodian.rs:2942`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check and their substantive audit is required at publish; batch review and TiKV compilation otherwise passed (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:207`). |
| T5 Judgment | PASS | Mechanical affected-file prior-art review found no overlapping open PR and only non-equivalent merged/closed work (#793 pending validation; #647 segmented maps); the carried-forward lifecycle and scrub gaps are pinned (`crates/custodian/tests/staged_protection.rs:501`, `crates/custodian/tests/staged_protection.rs:899`, `crates/custodian/tests/staged_protection.rs:1071`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the intentional fleet-wide GC/restore stall from one unreadable staged record — it prevents data loss but retains all garbage until repair, while scrub remains live and drain behavior stays deferred to #664 (`crates/custodian/src/gc.rs:484`, `crates/custodian/tests/staged_protection.rs:1071`, `crates/custodian/tests/staged_protection.rs:1096`). |

### Advisory — adversary

# Adversarial review — #803 staged protection class (iteration 3)

Verdict: I could not break the production fix. I found one test gap on a path where a mistake would delete data. Everything below was re-run on a scratch copy of `$PDCA_TARGET` (the patched tree, base commit `afa44c5`).

## Findings

- NEEDS-HUMAN [impl] — **No test checks that a store error during the staged build propagates.** `crates/custodian/src/gc.rs:787-788` promises that "a store fault propagates rather than being read as 'this session stages nothing'", and the code does use `?` at `gc.rs:791` (`mpu:`), `:799` (`sidx:<id>:`) and `:802` (`part:<id>:`). But no test would notice if that stopped being true. I replaced those `?`s with `.unwrap_or_default()`, so a failed read turns into "no staged records". That is exactly the data-loss shape: one transient backend error on `sidx:<id>:` empties the staged set, and GC reclaims a live upload's fragments that are marked past grace. Results: all 11 tests in `staged_protection.rs` stayed green, all 21 test suites in `wyrd-custodian` stayed green, and all 18 DST tests in `crates/dst/tests/custodian.rs` stayed green. The committed side already guards the same rule (`crates/custodian/tests/segmented_map_consumers.rs:1122`, "a genuine store fault still propagates"). Suggested fix: let the `Meta` double (`crates/custodian/tests/staged_protection.rs:201-208`) fail `scan` for a chosen prefix. Then, for each of `mpu:`, `sidx:<id>:` and `part:<id>:`, assert that both `reconcile_step` (GC) and `reconcile_after_restore` return `Err`, and that the staged fragment is still on disk and still unmarked. cargo-mutants never generates this mutant because it does not mutate `?`, which is why C5 stayed green.

- The C5-mutants "pass" (`check-gates.json`, row `C5-mutants`: 25 mutants, 6 caught, **19 unviable**) is weak evidence. It rests on only 6 mutants that compiled, against about 400 changed production lines. To fill the gap I ran 10 more hand-written mutants on top of the two above:
  - M1: staged build after the `inode:` scan
  - M2: `part:` read before `sidx:`
  - M3: an `Open`-only session filter
  - M3b: `Open`-only for `sidx:`
  - M4: iteration 2's `unresolvable.extend(staged.unresolvable.clone())`
  - M7: silently skipping a bad `mpu:` key
  - M8: an undecodable owned value treated as a hole instead of a hold
  - M9: filling an empty staged placement with the identity placement (fragment i on server i)
  - M10: dropping the `staged-malformed` branch
  - The M5/M6 error-swallowing pair from the finding above

  The unit tests caught every one except M5/M6. DST property 13 also goes red under M1, M2 and M3. It passes under M3b, which is fine because the brief allows either choice. The iteration-1 carry-forward (test more than `Open` sessions) and the iteration-2 carry-forward (catch M4 via the scrub assertion) are both really closed.

- Informational, low priority: `crates/custodian/src/desired_state.rs:257-258` says `unresolvable-chunk-map` "is the shared action, so one query selects every unreadable-record signal across all the surfaces that read this set". This diff adds a second action, `unresolvable-staged-record` (`gc.rs:1238`, `restore.rs:966`), for a condition that blocks GC across the whole fleet. An alert built on that single documented query will miss a staged blocker. GC still answers `Blocked`, and the restore CLI text names both actions, so nothing is hidden outright. `desired_state.rs` is outside this slice's scope, so this is a note for #664, not a rebuild item.

## Re-running the evidence (holds)

- Green: `cargo test -p wyrd-custodian --test staged_protection` passes 11/11 on the patched tree.
- Red: with `gc.rs` and `restore.rs` reset to `afa44c5` and the new test kept, all 11 fail. Every failure is an assertion panic inside the test file (lines 764, 792, 852, 981, 1025, 1160), not a compile error. This matches `gate-logs/C4-verify.log`.
- The tests exercise the production path: they call the real `reconcile_step` and `reconcile_after_restore` over in-memory doubles, not a copy of the logic. The seeded records pass through the production decoders and re-encode byte-for-byte.
- Both new DST properties pass on the patch, in `gate-logs/C4-ci.log` (lines 3489 and 3499) and in my own run.

## Refutation attempts that failed

- **Read order.** The design requires `sidx:` → `part:` → `inode:` (`0016:782-800`). The code follows it: `staged_fragments` runs at `gc.rs:552`, before the `inode:` scan at `:557`, and within a session `:799` comes before `:802`. Leg C and DST property 13 catch either reversal (M1, M2).
- **Cleanup stall from protecting every session state.** The design orphan-marks and deletes each owned `sidx:` entry in the same batch (`0016:671`). `retire:bytes` marks first and then deletes the records. `mpu:` is deleted last, only once `sidx:` is empty and the retire obligations have drained (`0016:673`). So covering non-`Open` sessions cannot hold bytes forever.
- **Scan caps.**
  - `mpu:` is bounded by `MAX_SESSIONS` (clamped to at most `SCAN_CAP/2`, `crates/core/src/multipart.rs:4624`).
  - Each `sidx:<id>:` range is at most `SCAN_CAP/2` because of the slot key space.
  - `part:<id>:` holds at most 10,000 records (`multipart.rs:4470`), against `SCAN_CAP` = 1,048,576 (`crates/traits/src/lib.rs:286`).
- **Zero-fragment geometry.** A scheme like `rs(0, m)` would make `fragment_count()` 0 and silently place nothing in `add_chunk`. It cannot get there: both decoders reject unsupported schemes (`multipart.rs:2359-2370`, `:3574-3581`).
- **Scrub and drain-status answers.** Both still read only `placed`, `malformed` and `unresolvable` (`scrub.rs:114`, `:205`; `desired_state.rs:191-246`). M4 proves the new scrub assertion catches folding the staged holes into the committed set.
- **Concurrent upload during restore.** The staged set is built once, before the fleet listing, so a part upload that starts mid-restore could get marked. But restore is documented as writers-stopped (`restore.rs:227`, `:267-268`), and GC's own staged build would still keep those fragments. Not a refutation.
- **Not raised.** Drain status ignoring staged fragments (`0016:827`, X65) and whether a held staged record should set `needs_human()` are both #664's (`// deferred: #664` at `restore.rs:326`, and in the brief).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the intentional fleet-wide GC/restore stall from one unreadable staged record — it prevents data loss but retains all garbage until repair, while scrub remains live and drain behavior stays deferred to #664 (`crates/custodian/src/gc.rs:484`, `crates/custodian/tests/staged_protection.rs:1071`, `crates/custodian/tests/staged_protection.rs:1096`).
- [ ] **No test checks that a store error during the staged build propagates.** `crates/custodian/src/gc.rs:787-788` promises that "a store fault propagates rather than being read as 'this session stages nothing'", and the code does use `?` at `gc.rs:791` (`mpu:`), `:799` (`sidx:<id>:`) and `:802` (`part:<id>:`). But no test would notice if that stopped being true. I replaced those `?`s with `.unwrap_or_default()`, so a failed read turns into "no staged records". That is exactly the data-loss shape: one transient backend error on `sidx:<id>:` empties the staged set, and GC reclaims a live upload's fragments that are marked past grace. Results: all 11 tests in `staged_protection.rs` stayed green, all 21 test suites in `wyrd-custodian` stayed green, and all 18 DST tests in `crates/dst/tests/custodian.rs` stayed green. The committed side already guards the same rule (`crates/custodian/tests/segmented_map_consumers.rs:1122`, "a genuine store fault still propagates"). Suggested fix: let the `Meta` double (`crates/custodian/tests/staged_protection.rs:201-208`) fail `scan` for a chosen prefix. Then, for each of `mpu:`, `sidx:<id>:` and `part:<id>:`, assert that both `reconcile_step` (GC) and `reconcile_after_restore` return `Err`, and that the staged fragment is still on disk and still unmarked. cargo-mutants never generates this mutant because it does not mutate `?`, which is why C5 stayed green.
- [x] size backstop — this slice is behaving oversized: patch is 120 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. — Human overrode: ignore the size count, proceed with `iterate-do`.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Fail-closed retention tradeoff accepted as-is (one unreadable staged record stalls GC/restore fleet-wide until fixed or drained — deliberate, keep it). Size backstop overridden: patch is 122 KB vs 100 KB threshold, this is round 3 — human explicitly chose iterate-do over iterate-plan, ignore the size count. Required fix for the rebuild: close the store-error propagation test gap the adversarial reviewer demonstrated. Replacing the `?` at `crates/custodian/src/gc.rs:791` (`mpu:`), `:799` (`sidx:<id>:`) and `:802` (`part:<id>:`) with `.unwrap_or_default()` still passes all 11 tests plus the DST suite — a transient backend read failure silently becomes "nothing staged," which is the exact data-loss shape this feature exists to prevent. Add a test that makes the `Meta` double (`crates/custodian/tests/staged_protection.rs:201-208`) fail `scan` for a chosen prefix, then for each of `mpu:`, `sidx:<id>:` and `part:<id>:` assert both `reconcile_step` (GC) and `reconcile_after_restore` return `Err`, and that the staged fragment is still on disk and unmarked.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
