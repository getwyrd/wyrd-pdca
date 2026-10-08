# Result — issue 842 / restore-fence-completing

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: after child-3, restore names a resurrected `Completing` session but does not
  fence it. That session can still publish over reclaimed bytes (D-B, `0016:717-728`), and the
  `seg:<nonce>:<E>:*` records its attempt already wrote have no deleter anywhere (X57,
  `0016:880`). 0016's `Completing → Aborting` restore fence is one batch: the session goes to
  `Aborting@E+1`, with its bytes obligation **and** `retire:records:{seg:<g>:<E>}` for the
  segments that attempt wrote (`0016:665`, `:2193`).
  One payload 0016 names for that row cannot be written. `{session, parts: <set>}` for a sparse
  part set exceeds the store's value ceiling: 10,000 alternating part numbers (1, 3, …, 19,999)
  encode to 128,916 bytes against `MAX_VALUE_BYTES` = 100,000
  (`crates/core/src/metadata.rs:549`), though 0016 claims they fit (`0016:382-384`). #809
  iteration 1 measured it. FoundationDB refuses the whole batch permanently
  (`2103 value_too_large`), so every run fails and every session after it stays unfenced.
  **Decided at Plan (2026-09-29, the human, option (a)):** the restore fence's bytes obligation
  for a `Completing` session is `{session, all}`, as for `Open`. It owes the same records: no part
  can commit outside `Open` (`crates/core/src/multipart.rs:4236-4250`, `0016:1030`), so the part
  set is frozen from the Complete fence on. And its size does not grow with the part count.
  Rejected: (b) keeping `{session, parts}` and naming an oversized session instead of fencing it,
  which leaves that session able to publish.
- Success criterion: the NEW file `crates/custodian/tests/restore_completing_fence.rs`
  passes over in-memory doubles, calling the production `reconcile_after_restore`. Records are
  seeded as raw JSON, and every session record carries `segment_nonce` in child-2's spelling
  (immediately after `clock_source`, in every state). The attempt's group is `(that nonce, E)`,
  where `E` is `publish_target.epoch`. Legs:
  **(G) A `Completing` session is fenced with its segments' deleter.** A `Completing@E` session
  with `segments_written > 0`, its nonce on the record, and `seg:<nonce>:<E>:*` records present
  ends as `Aborting@E+1`. **One** commit installs `retire:bytes:s:<id>:<E>` `{session, all}` and
  `retire:records:s:<id>:<E>` naming exactly the group `(nonce, E)`. Both decode through
  `decode_retire_obligation` against their keys, and the records obligation's `segments()` names
  that group. The records obligation is installed even when `segments_written` is 0: 0016's row
  is "1 put" (`0016:665`), and a damaged cursor must not decide whether records get a deleter.
  Atomicity works as in child-3's F-atomic: a double failing the commit that carries any one of
  the three writes leaves none of them. Proving that draining empties the range is #659's job.
  **(G-collision) Neither obligation overwrites one already there.** Both obligation keys are
  installed under `require_absent` (`0016:369-373`, `:675-676`; `multipart.rs:1409-1414`), as
  child-3's F-collision already requires for the bytes key. Seed a `Completing@E` session beside a
  decodable `retire:records:s:<id>:<E>` naming a **different** segment group. After the pass,
  that obligation is byte-identical, none of the three fence writes landed (the session is still
  `Completing@E`, and no `retire:bytes:s:<id>:<E>` exists), the session is named as needing a
  human with child-3's "key taken" cause, and an `Open` session whose key sorts after it is
  still fenced.
  **(G-sparse) The fence fits the store's value ceiling whatever the part count.** Use a store
  double that refuses any value larger than `MAX_VALUE_BYTES`, as FoundationDB does. A
  `Completing` session holding 10,000 parts numbered 1, 3, …, 19,999 is fenced as in G, every
  value the pass writes is at most `MAX_VALUE_BYTES`, and an `Open` session whose key sorts after
  it is fenced too.
  **(H) What cannot be fenced cleanly is never passed off as done.**
  (i) A `Completing` record with **no** nonce (the shape before child-2) fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds is still fenced, and still named as needing a human.
  (iv) A `Completing` session with one `seg:<nonce>:<E>:*` value that will not decode is still
  fenced and named as needing a human. The records obligation would delete that record without
  marking whatever chunks it named.
  (v) A key under the group's `seg:` range that is not a well-formed segment key of that group
  is named the same way.
  (vi) A decodable `Completing` session at epoch `u64::MAX` (its `publish_target.epoch` equal,
  as decode requires, `multipart.rs:2224-2229`; decode does not refuse that epoch) has no
  `E+1`. Child-3's `u64::MAX` guard (its H(ii)) applies to `Completing` too: no transition, no
  `retire:` key for it, no wrap and no panic, the record byte-identical, the session named as
  needing a human, and an `Open` session whose key sorts after it still fenced.
  In every case `needs_human()` is true.
  **(K) A second pass is idempotent and still names what needs a human.** Re-running over the
  fenced store leaves it byte-identical, and a session named under H(ii), H(iv) or H(v) on the
  first pass is named again. "Already `Aborting`" never means "nothing to report"; #664
  iteration 1 got this wrong. A fact Do can rely on: for a session at `Aborting@E'`, only a
  `Completing → Aborting` fence files `retire:records:s:<id>:<E'-1>`. A rollback files at the
  epoch it leaves and lands in `Open@E+1` (`0016:2196`).
  **(Order)** child-3's P3 still holds with a `Completing` session in the store: the fence runs
  after Pass 3.
  **(L) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `Completing` arm of the restore fence: one commit per session installing the
  bytes and records obligations beside the transition. Reading the attempt's `seg:` range to name
  what cannot be fenced cleanly (H). The second-pass re-check (K). Update `multipart.rs`'s retire
  rows table (`:3141-3150`) so the restore `Completing` fence is listed under `{session, all}`,
  with the reason. Extend the fence paragraph child-3 wrote in `06-runtime-view.md` §6.5 and the
  m4 blueprint's step 7 to `Completing` sessions. A `deferred: #659` marker belongs wherever the
  re-check could misread a range the future drain has half-deleted. Size budget: at most 7 files
  and under 80 KB of diff. / out of scope: the `Open` fence, and report fields beyond what G/H
  need (child-3); DST (child-5; the same rule as child-3 for `crates/dst/tests/custodian.rs`); the
  `{session, parts}` rows of other writers; #810; #659; #656, #658; #508; any edit to 0016 or an
  ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 4m: 8 caught, 28 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.18s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #842: fence restored `Completing` uploads atomically with both retirement obligations, fit the value ceiling, and preserve damage warnings on reruns.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The agreed restore-only contract is falsifiable and settles the sparse-part payload choice; no new scope decision is needed (`brief.md:25`, `brief.md:34`, `brief.md:114`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked patch while retaining its new test produced seven assertion failures, with compilation successful (`reviewer-evidence/red.log:146`, `reviewer-evidence/red.log:211`). |
| C3 Change | PASS | The seven affected files cover the agreed fence, reporting, tests and living documentation; the submitted target blobs match the patch (`crates/core/src/multipart.rs:2291`, `docs/design/architecture/06-runtime-view.md:66`, `reviewer-evidence/source-integrity.log:1`). |
| C4 Verification (red→green) | PASS | The asserted regression independently changes from seven failures to seven passes, with nine Open-fence controls passing; complete CI has frozen evidence, subject to the local host caveat below (`reviewer-evidence/green.log:17`, `reviewer-evidence/green.log:32`, `gate-logs/C4-ci.log:3921`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Repair the missing-deleter rerun case and pin it with a regression — surviving attempt records can lose an existing warning and receive a clean restore verdict (`crates/custodian/src/restore.rs:945`, `reviewer-evidence/missing-obligation.log:11`). |
| T1 Structure | PASS | The fence retains the metadata trait boundary and one conditional batch for the session and both obligations, preserving atomicity and collision protection (`crates/custodian/src/restore.rs:821`, `crates/custodian/tests/restore_completing_fence.rs:413`, `crates/custodian/tests/restore_completing_fence.rs:445`). |
| T2 Shape | PASS | Validated identities and decoder-compatible obligations preserve the data contract; both obligation keys, payloads and excluded states are exercised in core (`crates/core/src/multipart.rs:3665`, `crates/core/src/multipart.rs:5212`). |
| T3 Runtime | FAIL | An Aborting session with surviving unreadable or unheld-chunk segments and no records obligation returns `is_clean=true`, `needs_human=false`; this is the same defect as C5 (`crates/custodian/src/restore.rs:945`, `reviewer-evidence/missing-obligation.log:16`, `reviewer-evidence/missing-obligation.log:21`). |
| T4 Contribution | FAIL | The frozen batch review's three entries are one independently reproduced, unresolved defect; the contribution-artifact subcheck is separately N/A until mandatory publish recheck (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Path-based merged and closed/unmerged prior art was checked; the recorded drain/DST deferrals remain settled, and the remaining defect fits this scope (`reviewer-evidence/prior-art-history.json:3`, `reviewer-evidence/prior-art-closed.json:2`, `crates/custodian/src/restore.rs:929`, `crates/custodian/tests/restore_completing_fence.rs:6`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the evidence is sufficient for this restore-only slice before deployment — live FoundationDB recovery and the operator drill were not exercised; runtime evidence uses seeded in-memory stores with an enforced value ceiling (`brief.md:132`, `crates/custodian/tests/restore_completing_fence.rs:173`, `docs/design/architecture/m4-first-deployment-blueprint.md:643`). |

One implementation defect remains: the rerun can certify surviving attempt records clean when their records-retirement obligation is absent. The requested regression passes, the batch review independently identifies the same defect, and the additional reproduction confirms it. This report is advisory; it does not decide acceptance.

**Repair the absent-obligation branch before treating the rerun as complete.** At `crates/custodian/src/restore.rs:945`, `recheck_fenced` returns success without reading the session's own `(segment_nonce, E'-1)` range when `retire:records:s:<id>:<E'-1>` is absent. Absence alone cannot distinguish an empty, settled attempt from remaining records with no deleter. This also reaches the CLI verdict through `crates/server/src/cli.rs:1439`.

The independent scratch harness calls the unchanged production `reconcile_after_restore`, using a copy of the submitted doubles. An `Aborting@4` session with its bytes obligation, two surviving `seg:<nonce>:3:*` records, and no records obligation incorrectly reports clean in all three cases: readable/held chunks, an unheld chunk, and an undecodable segment. A fourth test starts with the production fence correctly reporting an unreadable segment, removes only the records obligation from the in-memory fixture, and observes the warning disappear on the next pass. The empty Aborting-range control passes. Result: four assertion failures and one passing control (`reviewer-evidence/missing-obligation.log:35`; harness cases at `reviewer-evidence/missing_obligation.rs:703` and `reviewer-evidence/missing_obligation.rs:733`).

The correction should check whether the attempt range is empty before accepting an absent obligation. If records remain, name the missing deleter even when every segment decodes and all chunks have parts. Add both the damaged-range regression and the empty-range control. This is one finding, deduplicating all three entries in `gate-logs/T4-batch-review.log:10`. It does not reopen #659's settled half-drain concern: the reproduction involves no drain, and the missing obligation leaves existing records without any named deleter.

**The supplied positive evidence is reproducible, with explicit limits.**

- **C4-verify — PASS:** independent `git stash` / `git stash pop` rerun: seven assertion failures on the base, seven passes with the patch, and nine existing Open-fence passes. The patch was restored; every affected file still matches its submitted blob (`reviewer-evidence/source-integrity.log:1`).
- **C4-ci — PASS from frozen evidence; local rerun incomplete due to host permissions:** local spelling, docs lint/render, hygiene guards, formatting, Clippy, build, workspace tests and dependency-usage checks passed. `cargo deny` then failed to lock the read-only advisory database (`reviewer-evidence/ci.log:3218`). This is a sandbox fault, not a patch defect. The frozen log records the complete CI pass, including deny and DST (`gate-logs/C4-ci.log:3296`, `gate-logs/C4-ci.log:3579`, `gate-logs/C4-ci.log:3921`). Independent statics and conformance reruns also passed (`reviewer-evidence/scanners.log:4`, `reviewer-evidence/scanners.log:7`). Both external tools named in the brief were actually exercised (`reviewer-evidence/ci.log:4`, `reviewer-evidence/ci.log:9`).
- **C4-diff-cov — not measured, base-state caveat:** the frozen failure is patch application against `origin/main`, whereas this bundle explicitly includes accepted prerequisite patches (`gate-logs/C4-diff-cov.log:10`, `brief.md:5`). The supplied target compiles and supports the independent red→green run. No coverage percentage or C4 patch defect follows from that log.
- **C5-mutants — PASS as logged, limited assurance:** the instance-scoped wrapper is unavailable here; its frozen output shows 36 mutants tested, eight caught and 28 unviable, with no surviving viable mutant reported (`gate-logs/C5-mutants.log:10`). Unviable mutants do not establish behavioral coverage and do not cover the reproduced missing-obligation case.
- **T4-batch-review — FAIL:** its three reported entries describe the single confirmed finding above (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution — N/A:** contribution artifacts are absent by design at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is owed for this deferral.
- **host-tikv — PASS from frozen evidence:** the captured command successfully compiles the TiKV crate and server feature selection; this is compile evidence, not a live backend recovery test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`).

**Prior art and standing conventions were checked without reopening settled work.** GitHub history was queried by all seven affected paths. Closed/merged PR file lists were inspected across 356 PRs, including the remaining file pages for #618 and #489; the only closed, unmerged match was #647's earlier segmented-map work (`reviewer-evidence/prior-art-history.json:1`, `reviewer-evidence/prior-art-closed.json:2`, `reviewer-evidence/prior-art-file-pages.json:1`, `reviewer-evidence/prior-art-rejected-647.json:1`). The brief separately records the rejected #809/#637/#664 attempts (`brief.md:145`). Their sparse-value, missing-deleter and rerun concerns are represented in the current tests; the new absent-obligation counterexample remains outstanding.

The prior paging, multi-chunk, foreign-obligation and bytes-collision findings now have exercised cases (`crates/custodian/tests/restore_completing_fence.rs:496`, `crates/custodian/tests/restore_completing_fence.rs:525`, `crates/custodian/tests/restore_completing_fence.rs:546`, `crates/custodian/tests/restore_completing_fence.rs:608`, `crates/custodian/tests/restore_completing_fence.rs:450`). The patch adds no clock source or load-time capability probe. The explicit #843 DST and #659 drain deferrals are settled under the standing protocol. Source citations above resolve against the supplied patched `$PDCA_TARGET`; no other checkout or builder notes were consulted.

### Advisory — adversary

# Adversarial review — #842 (809.4) restore fence for `Completing` sessions

**Evidence re-run.** I reproduced C4-verify in a scratch copy of `$PDCA_TARGET`. With the fix,
`cargo test -p wyrd-custodian --test restore_completing_fence` passes 7/7. With the three
production files (`multipart.rs`, `restore.rs`, `cli.rs`) reverted to the base, all 7 fail by
**assertion**, not by a compile error (e.g. `restore_completing_fence.rs:297` sees `Completing@3`
where `Aborting@4` is expected). The tests call the production `reconcile_after_restore`. New
report fields are read only through `Debug`, so the base compiles. The proof is real.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:945-947`: `recheck_fenced` returns
  `Ok(())` as soon as `retire:records:s:<id>:<E'-1>` is **absent**. It never looks at the
  session's own `seg:<nonce>:<E'-1>:` range. That is the T4 gate's blocking finding (all 3
  T4 bullets are this one line), and I confirmed it with two failing cases in a scratch test.
  The production code is unchanged in both.
  **(A1)** `Aborting@4` session `aa…`, `seg:aa…:3:000000` = a valid segment,
  `seg:aa…:3:000001` = `not a segment`, and no `retire:records:s:aa…:3`. The result is
  `segments_unaccounted: []`, `needs_human() == false`, and the CLI exits 0. Those segment
  records have no deleter anywhere (X57 is open again), and nothing names them.
  **(A2)** This is the operator flow the patch itself invites. Take an `Aborting@4` session with
  `retire:records:s:<id>:3` = `not json` (leg K's `fb` shape, `tests/restore_completing_fence.rs:610`)
  and one live `seg:<nonce>:3:000000`. Pass 1 correctly names the obligation `Undecodable`. The
  CLI then says "inspect the named record … then re-run this pass" (`crates/server/src/cli.rs:1411`).
  The obvious repair for an undecodable obligation is to delete it. When the operator does that
  and re-runs, pass 2 reports **clean** (`needs_human() == false`). The same happens with
  K's `fc` (`{parts}` only, `NotOfAttempt`). This breaks the brief's K rule ("Already `Aborting`
  never means nothing to report").
  The `deferred: #659` marker (`restore.rs:929-930`) covers a *half-drained* range read as
  false positives. It does not cover this false negative, so the deferral does not settle it.
  **The fix is cheap and has no false alarms.** On an absent key, read one `staged_page` of
  `seg_range_prefix(group)`. If it is non-empty, name the session at its first record with a new
  `SegmentFault` (e.g. "no records obligation owes it"). My control case (`Open`-fenced
  `Aborting@4`, empty range at 3) stays unnamed. A finished #659 drain empties the range before
  it drops the key, so that case stays unnamed too. Add A1 and A2 to leg K, and update the
  `06-runtime-view.md:65` sentence that lists what "every run also names".

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:925-928`: the doc comment reasons from
  the brief's fact ("only that fence files `retire:records:s:<id>:<E'-1>`") to its converse
  (no key, so no attempt, so nothing to check). The brief never claims the converse, and A1
  shows it is false for a damaged or hand-repaired store. Reword the comment along with the
  fix above. The reviewer's acceptance of leg K rests on this converse.

## Attempted and could not refute

- **G-sparse / value ceiling.** `{session, all}` and `{seg}` are constant-size. The ceiling
  double (`restore_completing_fence.rs:119-125`) refuses oversized puts the way FoundationDB
  does. A `{session, parts}` mutant would make the pass return `Err`, and the test's `unwrap`
  would go red.
- **Atomicity / collisions.** One `WriteBatch` holds `require(mpu)`, two `require_absent`s and
  three puts (`restore.rs:821-828`). Both bytes-key and records-key collisions are tested, and
  the `u64::MAX` guard holds for `Completing` (`multipart.rs` `checked_add`).
- **Group identity.** The token epoch equals `publish_target.epoch` equals the record epoch, as
  decode requires. `recheck_fenced` now compares the payload's group with the session's own
  nonce (the iteration-3 finding is fixed). `parse_seg_key` under the group's own prefix cannot
  smuggle in a foreign epoch, because the trailing `:` stops `3:` from matching `31:`.
- **`ChunkInNoPart` soundness.** 0016 says segment records name the frozen parts' own chunks
  ("bytes still protected by the `part:` records"), so comparing by chunk id is right. Paging
  of both ranges is pinned (the `paged` case, and G-sparse's segment on part 19,999).
- **False positives on legitimate history.** A rollback files at `E` and lands `Open@E+1`. A
  later fence of that `Open` checks `E+1`, which is empty. A root flip lands `Completed`
  (`Settled`). I found no legitimate state that `recheck_fenced` names wrongly.
- C4-diff-cov's "does not apply on origin/main" is a harness limit (the base includes
  child-1..3), not evidence against the fix. C5 reports 0 missed.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:945`: A missing records obligation silently skips the attempt’s segment range. For an `Aborting@4` session with nonce A, no `retire:records:s:<id>:3`, and an undecodable `seg:A:3:000000`, this branch returns without naming either record. With otherwise empty metadata and fragment stores, the pass can report `is_clean() == true` and `needs_human() == false`, despite remaining segments having no deleter. Probe the attempt range before accepting an absent obligation; if records remain, report the missing deleter even when those records decode. Add this regression alongside the malformed/foreign obligation cases at `crates/custodian/tests/restore_completing_fence.rs:608`, retaining a clean control with neither obligation nor segments.

No additional material reuse, simplification, or efficiency findings. Reviewed against the read-only target and frozen gate evidence; no tests were rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Repair the missing-deleter rerun case and pin it with a regression — surviving attempt records can lose an existing warning and receive a clean restore verdict (`crates/custodian/src/restore.rs:945`, `reviewer-evidence/missing-obligation.log:11`).
- [ ] Validation — fitness-to-purpose — Decide whether the evidence is sufficient for this restore-only slice before deployment — live FoundationDB recovery and the operator drill were not exercised; runtime evidence uses seeded in-memory stores with an enforced value ceiling (`brief.md:132`, `crates/custodian/tests/restore_completing_fence.rs:173`, `docs/design/architecture/m4-first-deployment-blueprint.md:643`).
- [ ] `crates/custodian/src/restore.rs:945-947`: `recheck_fenced` returns `Ok(())` as soon as `retire:records:s:<id>:<E'-1>` is **absent**. It never looks at the session's own `seg:<nonce>:<E'-1>:` range. That is the T4 gate's blocking finding (all 3 T4 bullets are this one line), and I confirmed it with two failing cases in a scratch test. The production code is unchanged in both. **(A1)** `Aborting@4` session `aa…`, `seg:aa…:3:000000` = a valid segment, `seg:aa…:3:000001` = `not a segment`, and no `retire:records:s:aa…:3`. The result is `segments_unaccounted: []`, `needs_human() == false`, and the CLI exits 0. Those segment records have no deleter anywhere (X57 is open again), and nothing names them. **(A2)** This is the operator flow the patch itself invites. Take an `Aborting@4` session with `retire:records:s:<id>:3` = `not json` (leg K's `fb` shape, `tests/restore_completing_fence.rs:610`) and one live `seg:<nonce>:3:000000`. Pass 1 correctly names the obligation `Undecodable`. The CLI then says "inspect the named record … then re-run this pass" (`crates/server/src/cli.rs:1411`). The obvious repair for an undecodable obligation is to delete it. When the operator does that and re-runs, pass 2 reports **clean** (`needs_human() == false`). The same happens with K's `fc` (`{parts}` only, `NotOfAttempt`). This breaks the brief's K rule ("Already `Aborting` never means nothing to report"). The `deferred: #659` marker (`restore.rs:929-930`) covers a *half-drained* range read as false positives. It does not cover this false negative, so the deferral does not settle it. **The fix is cheap and has no false alarms.** On an absent key, read one `staged_page` of `seg_range_prefix(group)`. If it is non-empty, name the session at its first record with a new `SegmentFault` (e.g. "no records obligation owes it"). My control case (`Open`-fenced `Aborting@4`, empty range at 3) stays unnamed. A finished #659 drain empties the range before it drops the key, so that case stays unnamed too. Add A1 and A2 to leg K, and update the `06-runtime-view.md:65` sentence that lists what "every run also names".
- [ ] `crates/custodian/src/restore.rs:925-928`: the doc comment reasons from the brief's fact ("only that fence files `retire:records:s:<id>:<E'-1>`") to its converse (no key, so no attempt, so nothing to check). The brief never claims the converse, and A1 shows it is false for a damaged or hand-repaired store. Reword the comment along with the fix above. The reviewer's acceptance of leg K rests on this converse.
- [ ] `crates/custodian/src/restore.rs:945`: A missing records obligation silently skips the attempt’s segment range. For an `Aborting@4` session with nonce A, no `retire:records:s:<id>:3`, and an undecodable `seg:A:3:000000`, this branch returns without naming either record. With otherwise empty metadata and fragment stores, the pass can report `is_clean() == true` and `needs_human() == false`, despite remaining segments having no deleter. Probe the attempt range before accepting an absent obligation; if records remain, report the missing deleter even when those records decode. Add this regression alongside the malformed/foreign obligation cases at `crates/custodian/tests/restore_completing_fence.rs:608`, retaining a clean control with neither obligation nor segments.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- [ ] **Atomicity alone does not protect the new retirement obligation from overwrite.** G requires three writes in one commit and a failing-commit witness (`brief.md:35-43`), but never requires a collision witness for the newly added `retire:records:s:<id>:<E>` key. The target explicitly requires `require_absent` on obligation installation and classification of collisions, because overwriting a payload permanently loses reclamation evidence (`crates/core/src/multipart.rs:1409-1414`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). A blind put of the records obligation can satisfy the listed fresh-store and atomicity cases. Revise G to require the absence guard and seed an existing, decodable same-epoch obligation naming a different segment group: its bytes must survive, none of the three fence writes may land, and the unresolved collision must be reported.
- [ ] **The promised epoch transition has an accepted input with no successor.** G says `Completing@E` ends at `Aborting@E+1` (`brief.md:35-38`), and the invariant is unconditional (`brief.md:76-79`). On the supplied target, the session epoch is any `u64` (`crates/core/src/multipart.rs:2085`); Completing validation checks cursor bounds and matching target identity/epoch, but does not reject `u64::MAX` (`crates/core/src/multipart.rs:2204-2240`). Such a decodable record cannot take the required transition. Revise H to cover a Completing record at that boundary: checked increment, no wrap/panic or partial obligations, unchanged session reported as needing a human, and continued fencing of a later eligible session. Explicitly apply any inherited child-3 guard to Completing, and qualify the invariant for sessions left unresolved instead of promising universal fencing.
- [ ] T5 Judgment — Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`).
- [ ] T5 Judgment — Confirm merged-history and closed/rejected-work coverage by affected path on the publication base: the brief documents restore.rs/multipart.rs prior art, but this target exposes only one synthetic commit and no remote, so broader coverage cannot be independently settled (`brief.md:145`; `reviewer-evidence/target-grounding.txt:2`).

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Repair the missing-deleter rerun case and pin it with a regression — surviving attempt records can lose an existing warning and receive a clean restore verdict (`crates/custodian/src/restore.rs:945`, `reviewer-evidence/missing-obligation.log:11`).; `crates/custodian/src/restore.rs:945-947`: `recheck_fenced` returns `Ok(())` as soon as `retire:records:s:<id>:<E'-1>` is **absent**. It never looks at the session's own `seg:<nonce>:<E'-1>:` range. That is the T4 gate's blocking finding (all 3 T4 bullets are this one line), and I confirmed it with two failing cases in a scratch test. The production code is unchanged in both. **(A1)** `Aborting@4` session `aa…`, `seg:aa…:3:000000` = a valid segment, `seg:aa…:3:000001` = `not a segment`, and no `retire:records:s:aa…:3`. The result is `segments_unaccounted: []`, `needs_human() == false`, and the CLI exits 0. Those segment records have no deleter anywhere (X57 is open again), and nothing names them. **(A2)** This is the operator flow the patch itself invites. Take an `Aborting@4` session with `retire:records:s:<id>:3` = `not json` (leg K's `fb` shape, `tests/restore_completing_fence.rs:610`) and one live `seg:<nonce>:3:000000`. Pass 1 correctly names the obligation `Undecodable`. The CLI then says "inspect the named record … then re-run this pass" (`crates/server/src/cli.rs:1411`). The obvious repair for an undecodable obligation is to delete it. When the operator does that and re-runs, pass 2 reports **clean** (`needs_human() == false`). The same happens with K's `fc` (`{parts}` only, `NotOfAttempt`). This breaks the brief's K rule ("Already `Aborting` never means nothing to report"). The `deferred: #659` marker (`restore.rs:929-930`) covers a *half-drained* range read as false positives. It does not cover this false negative, so the deferral does not settle it. **The fix is cheap and has no false alarms.** On an absent key, read one `staged_page` of `seg_range_prefix(group)`. If it is non-empty, name the session at its first record with a new `SegmentFault` (e.g. "no records obligation owes it"). My control case (`Open`-fenced `Aborting@4`, empty range at 3) stays unnamed. A finished #659 drain empties the range before it drops the key, so that case stays unnamed too. Add A1 and A2 to leg K, and update the `06-runtime-view.md:65` sentence that lists what "every run also names".; `crates/custodian/src/restore.rs:925-928`: the doc comment reasons from the brief's fact ("only that fence files `retire:records:s:<id>:<E'-1>`") to its converse (no key, so no attempt, so nothing to check). The brief never claims the converse, and A1 shows it is false for a damaged or hand-repaired store. Reword the comment along with the fix above. The reviewer's acceptance of leg K rests on this converse.; `crates/custodian/src/restore.rs:945`: A missing records obligation silently skips the attempt’s segment range. For an `Aborting@4` session with nonce A, no `retire:records:s:<id>:3`, and an undecodable `seg:A:3:000000`, this branch returns without naming either record. With otherwise empty metadata and fragment stores, the pass can report `is_clean() == true` and `needs_human() == false`, despite remaining segments having no deleter. Probe the attempt range before accepting an absent obligation; if records remain, report the missing deleter even when those records decode. Add this regression alongside the malformed/foreign obligation cases at `crates/custodian/tests/restore_completing_fence.rs:608`, retaining a clean control with neither obligation nor segments.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
